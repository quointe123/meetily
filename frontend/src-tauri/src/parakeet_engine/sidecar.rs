//! Rust-side IPC for the `parakeet_helper.py` Python sidecar.
//!
//! This module spawns a long-lived Python child process running the reference
//! TDT decoder (via `onnx_asr`) and exposes synchronous request/response
//! methods over newline-delimited JSON. Each `transcribe` call writes the
//! audio samples to a temporary raw f32 little-endian mono file and passes the
//! path to the helper — this avoids base64'ing megabytes of audio through the
//! stdin pipe.
//!
//! The helper's stderr is forwarded via `log::warn!` on a background thread.
//!
//! `ParakeetSidecar` is intentionally **not** `Send`/`Sync`: it owns process
//! handles that are awkward to share. Callers wrap it in `Arc<Mutex<_>>` if
//! they need to share it across threads.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Counter used to build unique temp-file names for each transcribe call.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Timeout for the initial `ping` round-trip after spawn.
const PING_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout for a `load_model` command (large ONNX models + VAD download).
const LOAD_TIMEOUT: Duration = Duration::from_secs(300);
/// Timeout for a `transcribe` command.
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(120);
/// Timeout for an `unload` command.
const UNLOAD_TIMEOUT: Duration = Duration::from_secs(10);
/// How long to wait for the child to exit after sending `shutdown` before killing.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

// ============================================================================
// Public types
// ============================================================================

/// One ASR output segment (matches the helper's response shape).
#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Long-lived Python sidecar process. Not `Send`/`Sync` on purpose — wrap in
/// `Arc<Mutex<_>>` at the call site if you need shared access.
pub struct ParakeetSidecar {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    /// Last successfully-loaded model id, or `None` if nothing is loaded.
    loaded_model_id: Option<String>,
}

// ============================================================================
// RAII temp-file guard
// ============================================================================

/// Deletes the given file when dropped. Used to ensure the raw f32 temp file
/// is cleaned up regardless of success/error paths.
struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.0) {
            // ENOENT is fine (e.g. already cleaned up); everything else is worth a line.
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "[parakeet-helper] failed to remove temp file {}: {}",
                    self.0.display(),
                    e
                );
            }
        }
    }
}

// ============================================================================
// ParakeetSidecar impl
// ============================================================================

impl ParakeetSidecar {
    /// Probe PATH for a usable Python interpreter.
    ///
    /// - Windows: try `py`, then `python`, then `python3`.
    /// - Unix:    try `python3`, then `python`.
    ///
    /// Returns the argv[0] that can be handed back to `Command::new`.
    pub fn locate_python() -> Result<PathBuf> {
        #[cfg(target_os = "windows")]
        let candidates: &[&str] = &["py", "python", "python3"];
        #[cfg(not(target_os = "windows"))]
        let candidates: &[&str] = &["python3", "python"];

        for &name in candidates {
            if probe_python(name) {
                log::info!("[parakeet-helper] located python interpreter: {}", name);
                return Ok(PathBuf::from(name));
            }
        }

        Err(anyhow!(
            "Python 3.10+ not found in PATH. Install Python 3.10 or later and ensure it is in your system PATH."
        ))
    }

    /// Spawn the Python helper as a long-lived child and verify it responds to
    /// `ping`. Returns an informative error if Python imports fail (e.g.
    /// `onnx_asr` missing).
    pub fn spawn(python: &Path, helper_script: &Path) -> Result<Self> {
        if !helper_script.exists() {
            return Err(anyhow!(
                "parakeet helper script not found: {}",
                helper_script.display()
            ));
        }

        log::info!(
            "[parakeet-helper] spawning: {} {}",
            python.display(),
            helper_script.display()
        );

        let mut command = Command::new(python);
        command
            .arg(helper_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);

        let mut child = command.spawn().with_context(|| {
            format!(
                "failed to spawn python helper: {} {}",
                python.display(),
                helper_script.display()
            )
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("failed to open parakeet helper stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("failed to open parakeet helper stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("failed to open parakeet helper stderr"))?;

        // Forward stderr lines as log::warn! on a background thread. The
        // thread dies automatically when the child's stderr closes.
        thread::Builder::new()
            .name("parakeet-helper-stderr".into())
            .spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    match line {
                        Ok(l) => log::warn!("[parakeet-helper stderr] {}", l),
                        Err(e) => {
                            log::warn!("[parakeet-helper stderr] read error: {}", e);
                            break;
                        }
                    }
                }
            })
            .context("failed to spawn stderr-forwarding thread")?;

        let mut this = Self {
            child: Some(child),
            stdin: Some(stdin),
            stdout: Some(BufReader::new(stdout)),
            loaded_model_id: None,
        };

        // Confirm the child is alive and importable.
        match this.request(&json!({"cmd": "ping"}), PING_TIMEOUT) {
            Ok(resp) => {
                let pong = resp.get("pong").and_then(Value::as_bool).unwrap_or(false);
                if !pong {
                    let _ = this.shutdown();
                    return Err(anyhow!(
                        "parakeet helper returned unexpected ping response: {}",
                        resp
                    ));
                }
                log::info!("[parakeet-helper] ping OK");
                Ok(this)
            }
            Err(e) => {
                // Upgrade missing-dep errors into a friendly install hint.
                let msg = e.to_string();
                let _ = this.shutdown();
                if msg.contains("onnx_asr") {
                    let sidecar_dir = helper_script.parent().unwrap_or_else(|| Path::new("."));
                    let requirements_path = sidecar_dir.join("requirements.txt");
                    Err(anyhow!(
                        "Python dependencies missing. Run: pip install -r \"{}\" (from sidecar directory {})",
                        requirements_path.display(),
                        sidecar_dir.display()
                    ))
                } else {
                    Err(e.context("parakeet helper ping failed"))
                }
            }
        }
    }

    /// Send `load_model` and wait for the helper to confirm.
    pub fn load_model(
        &mut self,
        model_dir: &Path,
        model_id: &str,
        quantization: &str,
    ) -> Result<()> {
        let cmd = json!({
            "cmd": "load_model",
            "model_dir": model_dir.to_string_lossy(),
            "model_id": model_id,
            "quantization": quantization,
        });
        let resp = self
            .request(&cmd, LOAD_TIMEOUT)
            .with_context(|| format!("load_model({}) failed", model_id))?;

        // Helper echoes {"status":"ok","loaded":"<model_id>"}.
        if let Some(loaded) = resp.get("loaded").and_then(Value::as_str) {
            self.loaded_model_id = Some(loaded.to_string());
        } else {
            self.loaded_model_id = Some(model_id.to_string());
        }

        log::info!(
            "[parakeet-helper] model loaded: {} (quant={}) from {}",
            model_id,
            quantization,
            model_dir.display()
        );
        Ok(())
    }

    /// Send `transcribe_raw`. Writes `samples` to a temp raw-f32-LE file, hands
    /// the path to the helper, parses the segments, and cleans up the temp
    /// file unconditionally (even on error).
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        language: Option<&str>,
    ) -> Result<Vec<TranscriptSegment>> {
        let tmp_path = make_temp_raw_path();
        let _guard = TempFileGuard(tmp_path.clone());

        write_f32_le(&tmp_path, samples)
            .with_context(|| format!("failed to write temp audio file {}", tmp_path.display()))?;

        let mut cmd = json!({
            "cmd": "transcribe_raw",
            "path": tmp_path.to_string_lossy(),
            "sample_count": samples.len(),
        });
        if let Some(lang) = language {
            cmd["language"] = Value::String(lang.to_string());
        }

        let resp = self
            .request(&cmd, TRANSCRIBE_TIMEOUT)
            .context("transcribe_raw failed")?;

        let segments_json = resp
            .get("segments")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("transcribe_raw response missing 'segments': {}", resp))?;

        let segments: Vec<TranscriptSegment> = segments_json
            .iter()
            .map(|seg| TranscriptSegment {
                text: seg
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                start_ms: seg.get("start_ms").and_then(Value::as_u64).unwrap_or(0),
                end_ms: seg.get("end_ms").and_then(Value::as_u64).unwrap_or(0),
            })
            .collect();

        Ok(segments)
        // `_guard` drops here → temp file is removed.
    }

    /// Tell the helper to drop its in-memory model. No-op from the caller's
    /// point of view if nothing was loaded.
    pub fn unload(&mut self) -> Result<()> {
        let resp = self
            .request(&json!({"cmd": "unload"}), UNLOAD_TIMEOUT)
            .context("unload failed")?;
        let _ = resp; // status=ok; nothing else to parse.
        self.loaded_model_id = None;
        Ok(())
    }

    /// Graceful shutdown: send `{"cmd": "shutdown"}`, wait up to 3s for the
    /// child to exit on its own, then kill if still alive. Idempotent.
    pub fn shutdown(&mut self) -> Result<()> {
        // Best-effort shutdown notification — ignore I/O errors (the child may
        // already be dead, stdin may be closed, etc.).
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = writeln!(stdin, "{}", json!({"cmd": "shutdown"}));
            let _ = stdin.flush();
        }

        // Drop stdin so the helper's `for line in sys.stdin` loop exits once
        // it has processed the shutdown line.
        self.stdin = None;
        self.stdout = None;

        if let Some(mut child) = self.child.take() {
            let start = Instant::now();
            let mut exited = false;
            while start.elapsed() < SHUTDOWN_WAIT {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        log::info!("[parakeet-helper] exited cleanly: {}", status);
                        exited = true;
                        break;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(50)),
                    Err(e) => {
                        log::warn!("[parakeet-helper] try_wait error: {}", e);
                        break;
                    }
                }
            }

            if !exited {
                log::warn!("[parakeet-helper] did not exit within {:?}, killing", SHUTDOWN_WAIT);
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        self.loaded_model_id = None;
        Ok(())
    }

    /// Whether a model is currently loaded (per the last successful
    /// `load_model` / `unload`).
    #[allow(dead_code)]
    pub fn loaded_model_id(&self) -> Option<&str> {
        self.loaded_model_id.as_deref()
    }

    // ------------------------------------------------------------------
    // Internal IPC helpers
    // ------------------------------------------------------------------

    /// Write one JSON command + `\n` to stdin, flush.
    fn write_command(&mut self, cmd: &Value) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("parakeet helper stdin is closed"))?;
        let line = serde_json::to_string(cmd).context("failed to serialize command")?;
        stdin
            .write_all(line.as_bytes())
            .context("failed to write command to stdin")?;
        stdin
            .write_all(b"\n")
            .context("failed to write newline to stdin")?;
        stdin.flush().context("failed to flush stdin")?;
        Ok(())
    }

    /// Read one line from stdout and parse as JSON. Errors if the response
    /// is `{"status":"error",...}`.
    fn read_response(&mut self) -> Result<Value> {
        let stdout = self
            .stdout
            .as_mut()
            .ok_or_else(|| anyhow!("parakeet helper stdout is closed"))?;
        let mut line = String::new();
        let n = stdout
            .read_line(&mut line)
            .context("failed to read response line from parakeet helper")?;
        if n == 0 {
            return Err(anyhow!(
                "parakeet helper closed stdout (process may have crashed)"
            ));
        }
        let value: Value =
            serde_json::from_str(line.trim()).with_context(|| format!("invalid JSON response from parakeet helper: {:?}", line))?;

        let status = value.get("status").and_then(Value::as_str).unwrap_or("");
        if status == "error" {
            let msg = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("(no message)")
                .to_string();
            let tb = value
                .get("traceback")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !tb.is_empty() {
                log::warn!("[parakeet-helper] error traceback:\n{}", tb);
                // Keep the traceback text in the error message so the
                // `onnx_asr` detection in `spawn()` can pick it up.
                return Err(anyhow!("parakeet helper error: {} | traceback: {}", msg, tb));
            }
            return Err(anyhow!("parakeet helper error: {}", msg));
        }

        Ok(value)
    }

    /// Write a command + read a response. `_timeout` is a best-effort hint —
    /// blocking read on stdout does not support per-read timeouts on Windows
    /// without a channel+thread dance. Documented as a known limitation.
    fn request(&mut self, cmd: &Value, _timeout: Duration) -> Result<Value> {
        self.write_command(cmd)?;
        self.read_response()
    }
}

impl Drop for ParakeetSidecar {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown() {
            log::warn!("[parakeet-helper] shutdown on drop failed: {}", e);
        }
    }
}

// ============================================================================
// Free helpers
// ============================================================================

/// Return `true` if `Command::new(name).arg("--version")` returns status 0.
fn probe_python(name: &str) -> bool {
    let mut cmd = Command::new(name);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.status() {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// Build a unique temp-file path for one raw audio payload.
fn make_temp_raw_path() -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("meetily-parakeet-{}-{}.raw", pid, counter))
}

/// Write `samples` as little-endian f32 to `path`. Uses `bytemuck::cast_slice`
/// for a zero-copy reinterpretation — this is safe because `f32` is
/// `Pod`/`Zeroable` and little-endian matches every tier-1 Rust target we
/// ship to (x86, x86_64, aarch64).
fn write_f32_le(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    use std::io::BufWriter;
    let file = std::fs::File::create(path)?;
    let mut writer = BufWriter::new(file);
    // All current Meetily target architectures are little-endian, so a raw
    // cast is correct and avoids per-sample to_le_bytes().
    let bytes: &[u8] = bytemuck::cast_slice(samples);
    writer.write_all(bytes)?;
    writer.flush()?;
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_paths_are_unique_and_in_temp_dir() {
        let a = make_temp_raw_path();
        let b = make_temp_raw_path();
        assert_ne!(a, b);
        assert!(a.starts_with(std::env::temp_dir()));
        assert!(b.starts_with(std::env::temp_dir()));
        assert!(a
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("meetily-parakeet-") && n.ends_with(".raw"))
            .unwrap_or(false));
    }

    #[test]
    fn write_f32_le_round_trip() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("meetily-parakeet-test-{}.raw", std::process::id()));
        let samples: Vec<f32> = vec![0.0, 1.0, -1.0, 0.5, std::f32::consts::PI];

        write_f32_le(&path, &samples).expect("write");
        let raw = std::fs::read(&path).expect("read");
        assert_eq!(raw.len(), samples.len() * 4);

        // Reinterpret and compare bitwise.
        let readback: Vec<f32> = raw
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(samples, readback);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn temp_file_guard_cleans_up() {
        let path = make_temp_raw_path();
        std::fs::write(&path, b"hello").expect("seed");
        assert!(path.exists());
        {
            let _g = TempFileGuard(path.clone());
        }
        assert!(!path.exists(), "guard must remove the file on drop");
    }

    #[test]
    fn temp_file_guard_ok_if_missing() {
        // Dropping a guard for a file that doesn't exist must not panic or
        // log a spurious error at a level higher than debug.
        let path = make_temp_raw_path();
        assert!(!path.exists());
        let _g = TempFileGuard(path);
        // If we got here, the Drop didn't panic — success.
    }

    #[test]
    fn locate_python_returns_some_path_or_informative_error() {
        // This test is deliberately lenient: CI images that ship Python will
        // return Ok, images without Python will get the friendly error. We
        // only assert that *if* we get Err, the message mentions Python.
        match ParakeetSidecar::locate_python() {
            Ok(p) => {
                let s = p.to_string_lossy().to_lowercase();
                assert!(s.contains("py") || s.contains("python"));
            }
            Err(e) => {
                let m = e.to_string().to_lowercase();
                assert!(m.contains("python"), "expected error to mention python, got: {}", m);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_probes_prefer_py_launcher() {
        // Confirms the candidate list ordering is what we documented — this
        // is a pure logic assertion, doesn't actually run python.
        // We can't see the `candidates` slice from here without exposing it,
        // so we just verify the public behaviour: `py` being present makes
        // locate_python return Ok.
        if probe_python("py") {
            let p = ParakeetSidecar::locate_python().expect("py is on PATH");
            assert_eq!(p, PathBuf::from("py"));
        }
    }

    /// End-to-end smoke test: actually spawns Python + pings the helper.
    /// Ignored by default because it requires Python 3.10+ and the `onnx_asr`
    /// package (or at least the script to be present — the test does not load
    /// a model, just pings).
    #[test]
    #[ignore]
    fn e2e_spawn_and_ping() {
        let python = ParakeetSidecar::locate_python().expect("python must be on PATH");

        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let helper = PathBuf::from(manifest_dir)
            .join("sidecars")
            .join("parakeet-helper")
            .join("parakeet_helper.py");
        assert!(helper.exists(), "expected helper at {}", helper.display());

        let mut sc = ParakeetSidecar::spawn(&python, &helper).expect("spawn");
        assert!(sc.loaded_model_id().is_none());
        // Second ping via a re-used sidecar.
        let resp = sc
            .request(&json!({"cmd": "ping"}), PING_TIMEOUT)
            .expect("ping");
        assert_eq!(resp.get("pong").and_then(Value::as_bool), Some(true));
        sc.shutdown().expect("shutdown");
    }
}
