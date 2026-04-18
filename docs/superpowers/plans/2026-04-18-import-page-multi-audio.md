# Import Page Multi-Audio — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single-file import modal with a dedicated `/import` page supporting 1–4 ordered audio files, processed sequentially by a new Rust multi-audio pipeline that inserts junction markers in the transcript.

**Architecture:** The frontend hook `useMultiImport` manages file list state and delegates to the new Tauri command `start_import_multi_command`. On the Rust side, `run_import_multi` loops over files, accumulates transcripts with timestamp offsets, and inserts a text junction marker between files. All legacy beta-gate code is deleted.

**Tech Stack:** Tauri 2.x (Rust) · Next.js 14 · React 18 · TypeScript · Sonner toasts · shadcn/ui · Whisper/Parakeet engines (unchanged)

---

## File Map

| Action | Path |
|--------|------|
| **Delete** | `frontend/src/contexts/ImportDialogContext.tsx` |
| **Delete** | `frontend/src/components/ImportAudio/ImportAudioDialog.tsx` |
| **Delete** | `frontend/src/components/ImportAudio/ImportDropOverlay.tsx` |
| **Delete** | `frontend/src/components/ImportAudio/index.ts` |
| **Modify** | `frontend/src/types/betaFeatures.ts` |
| **Modify** | `frontend/src/components/BetaSettings.tsx` |
| **Modify** | `frontend/src/components/MeetingDetails/TranscriptButtonGroup.tsx` |
| **Modify** | `frontend/src/app/layout.tsx` |
| **Modify** | `frontend/src/components/Sidebar/index.tsx` |
| **Modify** | `frontend/src-tauri/src/audio/import.rs` |
| **Modify** | `frontend/src-tauri/src/lib.rs` |
| **Create** | `frontend/src/hooks/useMultiImport.ts` |
| **Create** | `frontend/src/app/import/page.tsx` |

---

## Task 1 — Remove beta flag `importAndRetranscribe`

**Files:**
- Modify: `frontend/src/types/betaFeatures.ts`
- Modify: `frontend/src/components/BetaSettings.tsx`
- Modify: `frontend/src/components/MeetingDetails/TranscriptButtonGroup.tsx`

- [ ] **Step 1: Replace `betaFeatures.ts` content**

```typescript
// frontend/src/types/betaFeatures.ts
/**
 * Beta Features Type System
 *
 * No beta features currently active.
 * When adding a new beta feature:
 * 1. Add property to BetaFeatures interface
 * 2. Add default in DEFAULT_BETA_FEATURES
 * 3. Add strings in BETA_FEATURE_NAMES and BETA_FEATURE_DESCRIPTIONS
 */

export interface BetaFeatures {}

export const DEFAULT_BETA_FEATURES: BetaFeatures = {};

export const BETA_FEATURE_NAMES: Record<keyof BetaFeatures, string> = {};

export const BETA_FEATURE_DESCRIPTIONS: Record<keyof BetaFeatures, string> = {};

export type BetaFeatureKey = keyof BetaFeatures;

export function loadBetaFeatures(): BetaFeatures {
  return {};
}

export function saveBetaFeatures(_features: BetaFeatures): void {}
```

- [ ] **Step 2: Simplify `BetaSettings.tsx`**

Replace the full file content with:

```typescript
// frontend/src/components/BetaSettings.tsx
"use client"

export function BetaSettings() {
  return (
    <div className="space-y-6">
      <div className="p-4 bg-gray-50 border border-gray-200 rounded-lg text-sm text-gray-600">
        Aucune fonctionnalité bêta active pour le moment.
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Remove beta conditions from `TranscriptButtonGroup.tsx`**

The file currently has two `betaFeatures.importAndRetranscribe &&` guards. Replace both blocks so the Enhance button and RetranscribeDialog are always rendered (when `meetingId` and `meetingFolderPath` are present), and remove the `useConfig` import.

Replace the entire file with:

```typescript
// frontend/src/components/MeetingDetails/TranscriptButtonGroup.tsx
"use client";

import { useState, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, FolderOpen, RefreshCw } from 'lucide-react';
import { RetranscribeDialog } from './RetranscribeDialog';

interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}

export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript,
  onOpenMeetingFolder,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptButtonGroupProps) {
  const [showRetranscribeDialog, setShowRetranscribeDialog] = useState(false);

  const handleRetranscribeComplete = useCallback(async () => {
    if (onRefetchTranscripts) {
      await onRefetchTranscripts();
    }
  }, [onRefetchTranscripts]);

  return (
    <div className="flex items-center justify-center w-full gap-2">
      <ButtonGroup>
        <Button
          variant="outline"
          size="sm"
          onClick={onCopyTranscript}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? 'No transcript available' : 'Copy Transcript'}
        >
          <Copy />
          <span className="hidden lg:inline">Copy</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          className="xl:px-4"
          onClick={onOpenMeetingFolder}
          title="Open Recording Folder"
        >
          <FolderOpen className="xl:mr-2" size={18} />
          <span className="hidden lg:inline">Recording</span>
        </Button>

        {meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="bg-gradient-to-r from-blue-50 to-purple-50 hover:from-blue-100 hover:to-purple-100 border-blue-200 xl:px-4"
            onClick={() => setShowRetranscribeDialog(true)}
            title="Retranscribe to enhance your recorded audio"
          >
            <RefreshCw className="xl:mr-2" size={18} />
            <span className="hidden lg:inline">Enhance</span>
          </Button>
        )}
      </ButtonGroup>

      {meetingId && meetingFolderPath && (
        <RetranscribeDialog
          open={showRetranscribeDialog}
          onOpenChange={setShowRetranscribeDialog}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onComplete={handleRetranscribeComplete}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
cd frontend && pnpm tsc --noEmit 2>&1 | head -40
```

Expected: no errors related to `importAndRetranscribe` or `betaFeatures`. If `ConfigContext` exposes `betaFeatures` / `toggleBetaFeature`, check if other files still use them and fix.

- [ ] **Step 5: Check ConfigContext for betaFeatures usage**

```bash
grep -rn "betaFeatures\|toggleBetaFeature\|importAndRetranscribe" frontend/src --include="*.ts" --include="*.tsx"
```

If `ConfigContext.tsx` still exposes `betaFeatures` / `toggleBetaFeature`, leave them in place for now (they do no harm with an empty `BetaFeatures` type). Fix any remaining type errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/types/betaFeatures.ts \
        frontend/src/components/BetaSettings.tsx \
        frontend/src/components/MeetingDetails/TranscriptButtonGroup.tsx
git commit -m "feat: graduate importAndRetranscribe to standard — remove beta flag"
```

---

## Task 2 — Rust: add `AudioFilePart` + `select_multiple_audio_files_command` + `start_import_multi_command` (1-file delegate)

**Files:**
- Modify: `frontend/src-tauri/src/audio/import.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Add `AudioFilePart` struct and new commands to `import.rs`**

After the existing `ImportStarted` struct (around line 107), add:

```rust
/// One audio file in an ordered multi-file import list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFilePart {
    pub path: String,
    pub order: u32, // 1-based, already sorted by the frontend
}
```

After the existing `cancel_import_command` (end of Tauri Commands section), add:

```rust
/// Open a multi-file picker and return the selected paths (no validation)
#[tauri::command]
pub async fn select_multiple_audio_files_command<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let app_clone = app.clone();
    let paths = tokio::task::spawn_blocking(move || {
        app_clone
            .dialog()
            .file()
            .add_filter(
                "Audio Files",
                &AUDIO_EXTENSIONS.iter().map(|s| *s).collect::<Vec<_>>(),
            )
            .blocking_pick_files()
    })
    .await
    .map_err(|e| format!("File dialog task failed: {}", e))?;

    Ok(match paths {
        Some(list) => list.iter().map(|p| p.to_string()).collect(),
        None => vec![],
    })
}

/// Start a multi-audio import (1–4 files).
/// With a single file, delegates to the existing single-file pipeline (zero regression).
#[tauri::command]
pub async fn start_import_multi_command<R: Runtime>(
    app: AppHandle<R>,
    parts: Vec<AudioFilePart>,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportStarted, String> {
    if IMPORT_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err("Import already in progress".to_string());
    }
    if parts.is_empty() {
        return Err("No files provided".to_string());
    }

    // Sort by order field (frontend should already send them sorted)
    let mut sorted_parts = parts;
    sorted_parts.sort_by_key(|p| p.order);

    tauri::async_runtime::spawn(async move {
        let use_parakeet = provider.as_deref() == Some("parakeet");

        let result = if sorted_parts.len() == 1 {
            // Single file: delegate to existing run_import (zero regression)
            start_import(
                app.clone(),
                sorted_parts[0].path.clone(),
                title,
                language,
                model,
                provider,
            )
            .await
        } else {
            // Multi-file pipeline
            let _guard = match ImportGuard::acquire() {
                Ok(g) => g,
                Err(e) => {
                    let _ = app.emit("import-error", ImportError { error: e });
                    return;
                }
            };
            IMPORT_CANCELLED.store(false, Ordering::SeqCst);

            let res = run_import_multi(
                app.clone(),
                sorted_parts,
                title,
                language,
                model,
                provider,
            )
            .await;

            super::common::unload_engine_after_batch(use_parakeet).await;

            match &res {
                Ok(r) => {
                    let _ = app.emit(
                        "import-complete",
                        serde_json::json!({
                            "meeting_id": r.meeting_id,
                            "title": r.title,
                            "segments_count": r.segments_count,
                            "duration_seconds": r.duration_seconds
                        }),
                    );
                }
                Err(e) => {
                    let _ = app.emit("import-error", ImportError { error: e.to_string() });
                }
            }
            res
        };

        if let Err(e) = result {
            error!("Import failed: {}", e);
        }
    });

    Ok(ImportStarted {
        message: "Import started".to_string(),
    })
}
```

- [ ] **Step 2: Register the two new commands in `lib.rs`**

Find the `invoke_handler` block in `frontend/src-tauri/src/lib.rs` (around line 687 where the existing import commands are). Add after `audio::import::cancel_import_command`:

```rust
audio::import::select_multiple_audio_files_command,
audio::import::start_import_multi_command,
```

- [ ] **Step 3: Add a stub for `run_import_multi`** (so it compiles before Task 3 fills it in)

In `import.rs`, add this stub right before the Tauri Commands section:

```rust
/// Multi-file import pipeline — full implementation in Task 3
async fn run_import_multi<R: Runtime>(
    app: AppHandle<R>,
    parts: Vec<AudioFilePart>,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportResult> {
    // Stub — will be replaced in Task 3
    let _ = (app, parts, title, language, model, provider);
    Err(anyhow!("run_import_multi not yet implemented"))
}
```

- [ ] **Step 4: Cargo check**

```bash
cd frontend && cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20
```

Expected: compiles clean (zero errors).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/audio/import.rs \
        frontend/src-tauri/src/lib.rs
git commit -m "feat(rust): add AudioFilePart, start_import_multi_command, select_multiple_audio_files_command"
```

---

## Task 3 — Rust: implement `run_import_multi`

**Files:**
- Modify: `frontend/src-tauri/src/audio/import.rs`

- [ ] **Step 1: Write a unit test for the junction marker format** (before implementation)

In the `#[cfg(test)]` block at the bottom of `import.rs`, add:

```rust
#[test]
fn test_junction_marker_format() {
    // 0 ms offset → "--- Audio 2 — 00:00:00 ---"
    let offset_ms: f64 = 0.0;
    let marker = format_junction_marker(2, offset_ms);
    assert_eq!(marker, "--- Audio 2 — 00:00:00 ---");

    // 90 seconds = 1 min 30 sec → "--- Audio 3 — 00:01:30 ---"
    let marker2 = format_junction_marker(3, 90_000.0);
    assert_eq!(marker2, "--- Audio 3 — 00:01:30 ---");

    // 3661 seconds = 1h 1min 1sec → "--- Audio 2 — 01:01:01 ---"
    let marker3 = format_junction_marker(2, 3_661_000.0);
    assert_eq!(marker3, "--- Audio 2 — 01:01:01 ---");
}
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cd frontend && cargo test --manifest-path src-tauri/Cargo.toml format_junction_marker 2>&1 | tail -10
```

Expected: FAIL — `format_junction_marker` not defined.

- [ ] **Step 3: Add `format_junction_marker` helper and replace the stub with the full `run_import_multi`**

Add the helper right before `run_import_multi`:

```rust
/// Format "--- Audio N — HH:MM:SS ---" from a millisecond offset
fn format_junction_marker(file_n: usize, offset_ms: f64) -> String {
    let total_secs = (offset_ms / 1000.0) as u64;
    let hh = total_secs / 3600;
    let mm = (total_secs % 3600) / 60;
    let ss = total_secs % 60;
    format!("--- Audio {} — {:02}:{:02}:{:02} ---", file_n, hh, mm, ss)
}
```

Replace the stub `run_import_multi` with the full implementation:

```rust
async fn run_import_multi<R: Runtime>(
    app: AppHandle<R>,
    parts: Vec<AudioFilePart>,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportResult> {
    let total_files = parts.len();
    let use_parakeet = provider.as_deref() == Some("parakeet");

    // Pre-compute expected durations (fast metadata path) for global progress
    let estimated_durations_ms: Vec<f64> = parts
        .iter()
        .map(|p| {
            extract_duration_from_metadata(Path::new(&p.path))
                .unwrap_or(0.0)
                * 1000.0
        })
        .collect();
    let total_estimated_ms: f64 = estimated_durations_ms.iter().sum::<f64>().max(1.0);

    info!(
        "Starting multi-audio import: {} files, title='{}', total_estimated={:.1}s",
        total_files,
        title,
        total_estimated_ms / 1000.0
    );

    // Create one meeting folder for all files
    let base_folder = get_default_recordings_folder();
    let meeting_folder = create_meeting_folder(&base_folder, &title, false)?;

    // Initialize transcription engine once (reused across all files)
    emit_progress(&app, "copying", 2, "Chargement du moteur de transcription...");

    let whisper_engine = if !use_parakeet {
        Some(get_or_init_whisper(&app, model.as_deref()).await?)
    } else {
        None
    };
    let parakeet_engine = if use_parakeet {
        Some(get_or_init_parakeet(&app, model.as_deref()).await?)
    } else {
        None
    };

    let mut all_transcripts: Vec<(String, f64, f64)> = Vec::new();
    let mut timestamp_offset_ms: f64 = 0.0;
    let mut actual_total_duration_ms: f64 = 0.0;

    for (file_idx, part) in parts.iter().enumerate() {
        let file_n = file_idx + 1; // 1-based for display

        // ── Cancellation check ──────────────────────────────────────────────
        if IMPORT_CANCELLED.load(Ordering::SeqCst) {
            let _ = std::fs::remove_dir_all(&meeting_folder);
            return Err(anyhow!("Import annulé"));
        }

        // ── Copy file ───────────────────────────────────────────────────────
        let source = PathBuf::from(&part.path);
        if !source.exists() {
            let _ = std::fs::remove_dir_all(&meeting_folder);
            return Err(anyhow!("Fichier introuvable : {}", source.display()));
        }
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4");
        let dest_filename = format!("audio_{}.{}", file_n, ext);
        let dest_path = meeting_folder.join(&dest_filename);

        emit_progress(
            &app,
            "copying",
            global_pct(timestamp_offset_ms, 0.0, estimated_durations_ms[file_idx], total_estimated_ms, 5, 85),
            &format!("Traitement audio {} de {} — copie...", file_n, total_files),
        );

        let src = source.clone();
        let dst = dest_path.clone();
        tokio::task::spawn_blocking(move || std::fs::copy(&src, &dst))
            .await
            .map_err(|e| anyhow!("Copy join error: {}", e))?
            .map_err(|e| anyhow!("Failed to copy audio {}: {}", file_n, e))?;

        // ── Decode ──────────────────────────────────────────────────────────
        if IMPORT_CANCELLED.load(Ordering::SeqCst) {
            let _ = std::fs::remove_dir_all(&meeting_folder);
            return Err(anyhow!("Import annulé"));
        }

        emit_progress(
            &app,
            "decoding",
            global_pct(timestamp_offset_ms, 0.05 * estimated_durations_ms[file_idx], estimated_durations_ms[file_idx], total_estimated_ms, 5, 85),
            &format!("Décodage audio {} de {}...", file_n, total_files),
        );

        let path_for_decode = dest_path.clone();
        let decoded = tokio::task::spawn_blocking(move || {
            decode_audio_file_with_progress(&path_for_decode, None)
        })
        .await
        .map_err(|e| anyhow!("Decode join error: {}", e))??;

        let file_duration_ms = decoded.duration_seconds * 1000.0;
        info!(
            "File {}/{}: decoded {:.2}s",
            file_n, total_files, decoded.duration_seconds
        );

        // ── Resample ────────────────────────────────────────────────────────
        emit_progress(
            &app,
            "resampling",
            global_pct(timestamp_offset_ms, 0.1 * file_duration_ms, file_duration_ms, total_estimated_ms, 5, 85),
            &format!("Conversion audio {} de {}...", file_n, total_files),
        );

        let audio_samples = tokio::task::spawn_blocking(move || {
            decoded.to_whisper_format_with_progress(None)
        })
        .await
        .map_err(|e| anyhow!("Resample join error: {}", e))?;

        // ── VAD ─────────────────────────────────────────────────────────────
        if IMPORT_CANCELLED.load(Ordering::SeqCst) {
            let _ = std::fs::remove_dir_all(&meeting_folder);
            return Err(anyhow!("Import annulé"));
        }

        let app_for_vad = app.clone();
        let offset_for_vad = timestamp_offset_ms;
        let file_dur_for_vad = file_duration_ms;
        let total_est_for_vad = total_estimated_ms;

        let speech_segments = tokio::task::spawn_blocking(move || {
            get_speech_chunks_with_progress(
                &audio_samples,
                VAD_REDEMPTION_TIME_MS,
                |vad_pct, _count| {
                    let within_file = vad_pct as f64 / 100.0 * file_dur_for_vad * 0.15;
                    emit_progress(
                        &app_for_vad,
                        "vad",
                        global_pct(offset_for_vad, within_file, file_dur_for_vad, total_est_for_vad, 5, 85),
                        &format!("Détection parole audio {} de {}... {}%", file_n, total_files, vad_pct),
                    );
                    !IMPORT_CANCELLED.load(Ordering::SeqCst)
                },
            )
        })
        .await
        .map_err(|e| anyhow!("VAD join error: {}", e))?
        .map_err(|e| anyhow!("VAD failed on file {}: {}", file_n, e))?;

        // ── Split long segments ─────────────────────────────────────────────
        const MAX_SEGMENT_SAMPLES: usize = 25 * 16000;
        let mut processable: Vec<crate::audio::vad::SpeechSegment> = Vec::new();
        for seg in &speech_segments {
            if seg.samples.len() > MAX_SEGMENT_SAMPLES {
                processable.extend(split_segment_at_silence(seg, MAX_SEGMENT_SAMPLES));
            } else {
                processable.push(seg.clone());
            }
        }
        let processable_count = processable.len();

        // ── Junction marker (not for first file) ───────────────────────────
        if file_idx > 0 {
            let marker = format_junction_marker(file_n, timestamp_offset_ms);
            info!("Inserting junction marker: '{}'", marker);
            all_transcripts.push((marker, timestamp_offset_ms, timestamp_offset_ms));
        }

        // ── Transcribe ──────────────────────────────────────────────────────
        for (i, segment) in processable.iter().enumerate() {
            if IMPORT_CANCELLED.load(Ordering::SeqCst) {
                let _ = std::fs::remove_dir_all(&meeting_folder);
                return Err(anyhow!("Import annulé"));
            }

            // Skip very short segments
            if segment.samples.len() < 1600 {
                continue;
            }

            let seg_progress_frac = (i as f64 + 1.0) / processable_count.max(1) as f64;
            let within_file = (0.3 + seg_progress_frac * 0.55) * file_duration_ms;
            emit_progress(
                &app,
                "transcribing",
                global_pct(timestamp_offset_ms, within_file, file_duration_ms, total_estimated_ms, 5, 85),
                &format!(
                    "Transcription audio {} de {} — segment {}/{} ({})...",
                    file_n,
                    total_files,
                    i + 1,
                    processable_count,
                    format_duration_secs(file_duration_ms / 1000.0)
                ),
            );

            let (text, _conf) = if use_parakeet {
                let engine = parakeet_engine.as_ref().unwrap();
                let t = engine
                    .transcribe_audio(segment.samples.clone())
                    .await
                    .map_err(|e| anyhow!("Parakeet failed on file {}, seg {}: {}", file_n, i, e))?;
                (t, 0.9f32)
            } else {
                let engine = whisper_engine.as_ref().unwrap();
                let (t, c, _) = engine
                    .transcribe_audio_with_confidence(segment.samples.clone(), language.clone())
                    .await
                    .map_err(|e| anyhow!("Whisper failed on file {}, seg {}: {}", file_n, i, e))?;
                (t, c)
            };

            let trimmed = text.trim();
            if !trimmed.is_empty() {
                all_transcripts.push((
                    text,
                    segment.start_timestamp_ms + timestamp_offset_ms,
                    segment.end_timestamp_ms + timestamp_offset_ms,
                ));
            }
        }

        // ── Advance offset ──────────────────────────────────────────────────
        timestamp_offset_ms += file_duration_ms;
        actual_total_duration_ms += file_duration_ms;
    }

    // ── Save ─────────────────────────────────────────────────────────────────
    emit_progress(&app, "saving", 87, "Création du compte-rendu...");

    let segments = create_transcript_segments(&all_transcripts);

    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    let meeting_id = create_meeting_with_transcripts(
        app_state.db_manager.pool(),
        &title,
        &segments,
        meeting_folder.to_string_lossy().to_string(),
    )
    .await?;

    emit_progress(&app, "saving", 93, "Écriture des fichiers...");

    if let Err(e) = write_transcripts_json(&meeting_folder, &segments) {
        warn!("write_transcripts_json failed: {}", e);
    }

    let source_tag = "import_multi";
    if let Err(e) = write_import_metadata(
        &meeting_folder,
        &meeting_id,
        &title,
        actual_total_duration_ms / 1000.0,
        &format!("{} fichiers audio", total_files),
        source_tag,
    ) {
        warn!("write_import_metadata failed: {}", e);
    }

    emit_progress(&app, "complete", 100, "Import terminé");

    info!(
        "Multi-audio import complete: meeting='{}', {} segments, total={:.1}s",
        meeting_id,
        segments.len(),
        actual_total_duration_ms / 1000.0
    );

    Ok(ImportResult {
        meeting_id,
        title,
        segments_count: segments.len(),
        duration_seconds: actual_total_duration_ms / 1000.0,
    })
}

/// Map (offset + within_file_progress) to a global percentage in [lo, hi]
fn global_pct(
    offset_ms: f64,
    within_file_ms: f64,
    file_duration_ms: f64,
    total_ms: f64,
    lo: u32,
    hi: u32,
) -> u32 {
    let _ = file_duration_ms; // kept for future use
    let frac = ((offset_ms + within_file_ms) / total_ms).clamp(0.0, 1.0);
    lo + ((hi - lo) as f64 * frac) as u32
}

/// Format seconds as "HH:MM:SS" or "MM:SS"
fn format_duration_secs(secs: f64) -> String {
    let total = secs as u64;
    let hh = total / 3600;
    let mm = (total % 3600) / 60;
    let ss = total % 60;
    if hh > 0 {
        format!("{:02}:{:02}:{:02}", hh, mm, ss)
    } else {
        format!("{:02}:{:02}", mm, ss)
    }
}
```

- [ ] **Step 4: Run the junction marker test**

```bash
cd frontend && cargo test --manifest-path src-tauri/Cargo.toml format_junction_marker -- --nocapture 2>&1 | tail -15
```

Expected: PASS — 3 assertions pass.

- [ ] **Step 5: Run all import tests**

```bash
cd frontend && cargo test --manifest-path src-tauri/Cargo.toml audio::import -- --nocapture 2>&1 | tail -20
```

Expected: all tests pass (the integration tests flagged `#[ignore]` are skipped).

- [ ] **Step 6: Cargo check**

```bash
cd frontend && cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/audio/import.rs
git commit -m "feat(rust): implement run_import_multi with timestamp offsets and junction markers"
```

---

## Task 4 — Create `useMultiImport` hook

**Files:**
- Create: `frontend/src/hooks/useMultiImport.ts`

- [ ] **Step 1: Create the hook**

```typescript
// frontend/src/hooks/useMultiImport.ts
'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { useRouter } from 'next/navigation';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';

// ── Types ──────────────────────────────────────────────────────────────────────

export interface AudioFileInfo {
  path: string;
  filename: string;
  duration_seconds: number;
  size_bytes: number;
  format: string;
}

export interface AudioFilePart {
  id: string;           // local React key (crypto.randomUUID)
  info: AudioFileInfo;
  validating: boolean;
  error: string | null;
}

export interface ImportProgress {
  stage: string;
  progress_percentage: number;
  message: string;
}

export type MultiImportStatus = 'idle' | 'validating' | 'processing' | 'complete' | 'error';

export interface UseMultiImportReturn {
  files: AudioFilePart[];
  status: MultiImportStatus;
  progress: ImportProgress | null;
  error: string | null;
  isProcessing: boolean;

  addFiles: (paths: string[]) => Promise<void>;
  removeFile: (id: string) => void;
  moveUp: (id: string) => void;
  moveDown: (id: string) => void;
  startImport: (
    title: string,
    language?: string | null,
    model?: string | null,
    provider?: string | null
  ) => Promise<void>;
  cancelImport: () => Promise<void>;
  reset: () => void;
}

const MAX_FILES = 4;

// ── Hook ───────────────────────────────────────────────────────────────────────

export function useMultiImport(): UseMultiImportReturn {
  const router = useRouter();
  const { refetchMeetings } = useSidebar();

  const [files, setFiles] = useState<AudioFilePart[]>([]);
  const [status, setStatus] = useState<MultiImportStatus>('idle');
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const isCancelledRef = useRef(false);

  // ── Tauri event listeners ────────────────────────────────────────────────────
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const cleanedRef = { current: false };

    const setup = async () => {
      const unProgress = await listen<ImportProgress>('import-progress', (e) => {
        if (isCancelledRef.current) return;
        setProgress(e.payload);
        setStatus('processing');
      });
      if (cleanedRef.current) { unProgress(); return; }
      unlisteners.push(unProgress);

      const unComplete = await listen<{
        meeting_id: string;
        title: string;
        segments_count: number;
        duration_seconds: number;
      }>('import-complete', async (e) => {
        if (isCancelledRef.current) return;
        setStatus('complete');
        setProgress(null);
        await refetchMeetings();
        router.push(`/meeting-details?id=${e.payload.meeting_id}`);
      });
      if (cleanedRef.current) { unComplete(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unComplete);

      const unError = await listen<{ error: string }>('import-error', (e) => {
        if (isCancelledRef.current) return;
        setStatus('error');
        setError(e.payload.error);
      });
      if (cleanedRef.current) { unError(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unError);
    };

    setup();

    return () => {
      cleanedRef.current = true;
      unlisteners.forEach(u => u());
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  // router and refetchMeetings are stable refs — intentionally omitted

  // ── addFiles ─────────────────────────────────────────────────────────────────
  const addFiles = useCallback(
    async (paths: string[]) => {
      // Count currently valid (non-errored) files
      const currentValid = files.filter(f => !f.error).length;
      const available = MAX_FILES - currentValid;

      if (available <= 0) {
        toast.error('Maximum 4 fichiers atteint', {
          description: 'Retirez un fichier avant d\'en ajouter un autre.',
        });
        return;
      }

      const toAdd = paths.slice(0, available);
      const skipped = paths.length - toAdd.length;
      if (skipped > 0) {
        toast.warning(`${skipped} fichier(s) ignoré(s)`, {
          description: `Maximum ${MAX_FILES} fichiers par import.`,
        });
      }

      // Create placeholder entries immediately (validating=true)
      const newParts: AudioFilePart[] = toAdd.map(path => ({
        id: crypto.randomUUID(),
        info: {
          path,
          filename: path.split(/[\\/]/).pop() ?? path,
          duration_seconds: 0,
          size_bytes: 0,
          format: '',
        },
        validating: true,
        error: null,
      }));

      setFiles(prev => [...prev, ...newParts]);

      // Validate each file asynchronously
      for (const part of newParts) {
        try {
          const info = await invoke<AudioFileInfo>('validate_audio_file_command', {
            path: part.info.path,
          });
          setFiles(prev =>
            prev.map(f => (f.id === part.id ? { ...f, info, validating: false } : f))
          );
        } catch (err: unknown) {
          const msg =
            typeof err === 'string'
              ? err
              : (err as { message?: string })?.message ?? 'Validation échouée';
          setFiles(prev =>
            prev.map(f =>
              f.id === part.id ? { ...f, validating: false, error: msg } : f
            )
          );
        }
      }
    },
    [files]
  );

  // ── removeFile ───────────────────────────────────────────────────────────────
  const removeFile = useCallback((id: string) => {
    setFiles(prev => prev.filter(f => f.id !== id));
  }, []);

  // ── moveUp ───────────────────────────────────────────────────────────────────
  const moveUp = useCallback((id: string) => {
    setFiles(prev => {
      const idx = prev.findIndex(f => f.id === id);
      if (idx <= 0) return prev;
      const next = [...prev];
      [next[idx - 1], next[idx]] = [next[idx], next[idx - 1]];
      return next;
    });
  }, []);

  // ── moveDown ─────────────────────────────────────────────────────────────────
  const moveDown = useCallback((id: string) => {
    setFiles(prev => {
      const idx = prev.findIndex(f => f.id === id);
      if (idx === -1 || idx >= prev.length - 1) return prev;
      const next = [...prev];
      [next[idx], next[idx + 1]] = [next[idx + 1], next[idx]];
      return next;
    });
  }, []);

  // ── startImport ──────────────────────────────────────────────────────────────
  const startImport = useCallback(
    async (
      title: string,
      language?: string | null,
      model?: string | null,
      provider?: string | null
    ) => {
      const validFiles = files.filter(f => !f.error && !f.validating);
      if (validFiles.length === 0) return;

      isCancelledRef.current = false;
      setStatus('processing');
      setError(null);
      setProgress(null);

      const parts = validFiles.map((f, idx) => ({
        path: f.info.path,
        order: idx + 1,
      }));

      try {
        await invoke('start_import_multi_command', {
          parts,
          title,
          language: language ?? null,
          model: model ?? null,
          provider: provider ?? null,
        });
      } catch (err: unknown) {
        setStatus('error');
        const msg =
          typeof err === 'string'
            ? err
            : (err as { message?: string })?.message ?? 'Échec du démarrage de l\'import';
        setError(msg);
      }
    },
    [files]
  );

  // ── cancelImport ─────────────────────────────────────────────────────────────
  const cancelImport = useCallback(async () => {
    isCancelledRef.current = true;
    try {
      await invoke('cancel_import_command');
      setStatus('idle');
      setProgress(null);
    } catch (err) {
      console.error('Failed to cancel import:', err);
    }
  }, []);

  // ── reset ────────────────────────────────────────────────────────────────────
  const reset = useCallback(() => {
    isCancelledRef.current = false;
    setFiles([]);
    setStatus('idle');
    setProgress(null);
    setError(null);
  }, []);

  return {
    files,
    status,
    progress,
    error,
    isProcessing: status === 'processing',
    addFiles,
    removeFile,
    moveUp,
    moveDown,
    startImport,
    cancelImport,
    reset,
  };
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd frontend && pnpm tsc --noEmit 2>&1 | grep useMultiImport
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/useMultiImport.ts
git commit -m "feat: add useMultiImport hook for 1-4 file import with ordering"
```

---

## Task 5 — Create `/import` page

**Files:**
- Create: `frontend/src/app/import/page.tsx`

The page needs two helper functions already in `ImportAudioDialog.tsx` — copy them inline since that file will be deleted.

- [ ] **Step 1: Create the import page directory and file**

```bash
mkdir -p "D:/Projets/Auto_cr-fork/frontend/src/app/import"
```

- [ ] **Step 2: Create `frontend/src/app/import/page.tsx`**

```typescript
'use client';

import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  Upload, FileAudio, Clock, HardDrive, ChevronDown, ChevronUp,
  ArrowUp, ArrowDown, X, Loader2, AlertCircle, Globe, Cpu,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useMultiImport, AudioFilePart } from '@/hooks/useMultiImport';
import { useTranscriptionModels } from '@/hooks/useTranscriptionModels';
import { LANGUAGES } from '@/constants/languages';
import { isAudioExtension } from '@/constants/audioFormats';

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${secs.toString().padStart(2, '0')}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

// ── FileCard ──────────────────────────────────────────────────────────────────

interface FileCardProps {
  part: AudioFilePart;
  index: number;
  total: number;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
  disabled: boolean;
}

function FileCard({ part, index, total, onMoveUp, onMoveDown, onRemove, disabled }: FileCardProps) {
  return (
    <div
      className={`flex items-center gap-3 p-3 rounded-lg border ${
        part.error
          ? 'border-red-200 bg-red-50'
          : 'border-gray-200 bg-white'
      }`}
    >
      {/* Order badge */}
      <span className="w-6 h-6 flex items-center justify-center rounded-full bg-blue-100 text-blue-700 text-xs font-bold flex-shrink-0">
        {index + 1}
      </span>

      {/* File info */}
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-gray-900 truncate">
          {part.info.filename || part.info.path.split(/[\\/]/).pop()}
        </p>
        {part.validating ? (
          <p className="text-xs text-gray-500 flex items-center gap-1">
            <Loader2 className="h-3 w-3 animate-spin" />
            Validation...
          </p>
        ) : part.error ? (
          <p className="text-xs text-red-600 flex items-center gap-1">
            <AlertCircle className="h-3 w-3" />
            {part.error}
          </p>
        ) : (
          <div className="flex items-center gap-3 text-xs text-gray-500 mt-0.5">
            <span className="flex items-center gap-1">
              <Clock className="h-3 w-3" />
              {formatDuration(part.info.duration_seconds)}
            </span>
            <span className="flex items-center gap-1">
              <HardDrive className="h-3 w-3" />
              {formatFileSize(part.info.size_bytes)}
            </span>
            {part.info.format && (
              <span className="text-blue-600 font-medium">{part.info.format}</span>
            )}
          </div>
        )}
      </div>

      {/* Reorder buttons */}
      <div className="flex flex-col gap-0.5">
        <button
          onClick={onMoveUp}
          disabled={disabled || index === 0}
          className="p-1 rounded hover:bg-gray-100 disabled:opacity-30 disabled:cursor-not-allowed"
          title="Monter"
        >
          <ArrowUp className="h-3.5 w-3.5 text-gray-600" />
        </button>
        <button
          onClick={onMoveDown}
          disabled={disabled || index === total - 1}
          className="p-1 rounded hover:bg-gray-100 disabled:opacity-30 disabled:cursor-not-allowed"
          title="Descendre"
        >
          <ArrowDown className="h-3.5 w-3.5 text-gray-600" />
        </button>
      </div>

      {/* Remove button */}
      <button
        onClick={onRemove}
        disabled={disabled}
        className="p-1 rounded hover:bg-red-100 disabled:opacity-30 disabled:cursor-not-allowed"
        title="Retirer"
      >
        <X className="h-4 w-4 text-gray-500 hover:text-red-600" />
      </button>
    </div>
  );
}

// ── ImportPage ────────────────────────────────────────────────────────────────

export default function ImportPage() {
  const { selectedLanguage, transcriptModelConfig } = useConfig();

  const {
    files,
    status,
    progress,
    error,
    isProcessing,
    addFiles,
    removeFile,
    moveUp,
    moveDown,
    startImport,
    cancelImport,
    reset,
  } = useMultiImport();

  const [title, setTitle] = useState('');
  const [titleModifiedByUser, setTitleModifiedByUser] = useState(false);
  const [selectedLang, setSelectedLang] = useState(selectedLanguage || 'auto');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [isDragging, setIsDragging] = useState(false);

  const { availableModels, selectedModelKey, setSelectedModelKey, loadingModels, fetchModels } =
    useTranscriptionModels(transcriptModelConfig);

  // Fetch models on mount
  useEffect(() => {
    fetchModels();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Auto-populate title from the first valid file (if user hasn't typed)
  const firstValidFile = files.find(f => !f.error && !f.validating);
  useEffect(() => {
    if (firstValidFile && !titleModifiedByUser) {
      setTitle(firstValidFile.info.filename);
    }
  }, [firstValidFile?.info.filename, titleModifiedByUser]); // eslint-disable-line react-hooks/exhaustive-deps

  // Total duration of valid files
  const totalDurationSecs = files
    .filter(f => !f.error && !f.validating)
    .reduce((sum, f) => sum + f.info.duration_seconds, 0);

  // Resolve selected model
  const selectedModel = useMemo(() => {
    if (!selectedModelKey) return undefined;
    const colonIdx = selectedModelKey.indexOf(':');
    if (colonIdx === -1) return undefined;
    return availableModels.find(
      m => m.provider === selectedModelKey.slice(0, colonIdx) &&
           m.name === selectedModelKey.slice(colonIdx + 1)
    );
  }, [selectedModelKey, availableModels]);

  const isParakeetModel = selectedModel?.provider === 'parakeet';

  // Force 'auto' language when Parakeet is selected
  useEffect(() => {
    if (isParakeetModel && selectedLang !== 'auto') setSelectedLang('auto');
  }, [isParakeetModel, selectedLang]);

  // ── Local drag-drop on this page ─────────────────────────────────────────
  useEffect(() => {
    if (isProcessing) return;

    const unlisteners: (() => void)[] = [];
    const cleanedRef = { current: false };

    const setup = async () => {
      const unEnter = await listen('tauri://drag-enter', () => setIsDragging(true));
      if (cleanedRef.current) { unEnter(); return; }
      unlisteners.push(unEnter);

      const unLeave = await listen('tauri://drag-leave', () => setIsDragging(false));
      if (cleanedRef.current) { unLeave(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unLeave);

      const unDrop = await listen<{ paths: string[] }>('tauri://drag-drop', (e) => {
        setIsDragging(false);
        const audioPaths = e.payload.paths.filter(p => {
          const ext = p.split('.').pop()?.toLowerCase();
          return !!ext && isAudioExtension(ext);
        });
        if (audioPaths.length > 0) {
          addFiles(audioPaths);
        } else if (e.payload.paths.length > 0) {
          toast.error('Fichier non supporté', {
            description: 'Formats acceptés : MP4, WAV, MP3, FLAC, OGG, MKV, WebM, WMA',
          });
        }
      });
      if (cleanedRef.current) { unDrop(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unDrop);
    };

    setup();

    return () => {
      cleanedRef.current = true;
      unlisteners.forEach(u => u());
    };
  }, [isProcessing, addFiles]);

  // ── Browse button (multi-select) ─────────────────────────────────────────
  const handleBrowse = useCallback(async () => {
    try {
      const paths = await invoke<string[]>('select_multiple_audio_files_command');
      if (paths.length > 0) {
        await addFiles(paths);
      }
    } catch (err) {
      console.error('File picker error:', err);
    }
  }, [addFiles]);

  // ── Import ───────────────────────────────────────────────────────────────
  const handleImport = useCallback(async () => {
    await startImport(
      title || firstValidFile?.info.filename || 'Import',
      isParakeetModel ? null : selectedLang === 'auto' ? null : selectedLang,
      selectedModel?.name ?? null,
      selectedModel?.provider ?? null,
    );
  }, [startImport, title, firstValidFile, isParakeetModel, selectedLang, selectedModel]);

  const validFiles = files.filter(f => !f.error && !f.validating);
  const hasValidFiles = validFiles.length > 0;

  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <div className="max-w-xl mx-auto px-6 py-8 space-y-6">
      <h1 className="text-2xl font-semibold text-gray-900">Importer des fichiers audio</h1>

      {/* Drop zone */}
      {!isProcessing && status !== 'error' && (
        <div
          className={`border-2 border-dashed rounded-xl p-8 text-center transition-colors ${
            isDragging
              ? 'border-blue-400 bg-blue-50'
              : 'border-gray-300 hover:border-gray-400'
          }`}
        >
          <FileAudio className="h-10 w-10 text-gray-400 mx-auto mb-3" />
          <p className="text-sm text-gray-600 mb-3">
            Glissez vos fichiers ici ou
          </p>
          <Button onClick={handleBrowse} variant="outline" size="sm">
            <Upload className="h-4 w-4 mr-2" />
            Parcourir
          </Button>
          <p className="text-xs text-gray-400 mt-2">
            MP4, WAV, MP3, FLAC, OGG, MKV, WebM, WMA — max 4 fichiers
          </p>
        </div>
      )}

      {/* File list */}
      {files.length > 0 && !isProcessing && (
        <div className="space-y-2">
          {files.map((part, idx) => (
            <FileCard
              key={part.id}
              part={part}
              index={idx}
              total={files.length}
              onMoveUp={() => moveUp(part.id)}
              onMoveDown={() => moveDown(part.id)}
              onRemove={() => removeFile(part.id)}
              disabled={isProcessing}
            />
          ))}
          {totalDurationSecs > 0 && (
            <p className="text-sm text-gray-500 text-right">
              Durée totale : {formatDuration(totalDurationSecs)}
            </p>
          )}
        </div>
      )}

      {/* Title input */}
      {hasValidFiles && !isProcessing && (
        <div className="space-y-1">
          <label className="text-sm font-medium text-gray-700">Titre</label>
          <Input
            value={title}
            onChange={e => {
              setTitle(e.target.value);
              setTitleModifiedByUser(true);
            }}
            placeholder="Titre de la réunion"
          />
        </div>
      )}

      {/* Advanced options */}
      {hasValidFiles && !isProcessing && (
        <div className="border rounded-lg">
          <button
            onClick={() => setShowAdvanced(v => !v)}
            className="w-full flex items-center justify-between p-3 text-sm font-medium text-gray-700 hover:bg-gray-50"
          >
            <span>Options avancées</span>
            {showAdvanced ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
          </button>

          {showAdvanced && (
            <div className="p-3 pt-0 space-y-4 border-t">
              {/* Language */}
              {!isParakeetModel ? (
                <div className="space-y-2">
                  <div className="flex items-center gap-2">
                    <Globe className="h-4 w-4 text-muted-foreground" />
                    <span className="text-sm font-medium">Langue</span>
                  </div>
                  <Select value={selectedLang} onValueChange={setSelectedLang}>
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder="Sélectionner une langue" />
                    </SelectTrigger>
                    <SelectContent className="max-h-60">
                      {LANGUAGES.map(lang => (
                        <SelectItem key={lang.code} value={lang.code}>
                          {lang.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">
                  La sélection de langue n'est pas disponible avec Parakeet (détection automatique).
                </p>
              )}

              {/* Model */}
              {availableModels.length > 0 && (
                <div className="space-y-2">
                  <div className="flex items-center gap-2">
                    <Cpu className="h-4 w-4 text-muted-foreground" />
                    <span className="text-sm font-medium">Modèle</span>
                  </div>
                  <Select
                    value={selectedModelKey}
                    onValueChange={setSelectedModelKey}
                    disabled={loadingModels}
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder={loadingModels ? 'Chargement...' : 'Sélectionner un modèle'} />
                    </SelectTrigger>
                    <SelectContent>
                      {availableModels.map(m => (
                        <SelectItem key={`${m.provider}:${m.name}`} value={`${m.provider}:${m.name}`}>
                          {m.displayName} ({Math.round(m.size_mb)} MB)
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* Progress bar */}
      {isProcessing && progress && (
        <div className="space-y-2">
          <div className="w-full bg-gray-200 rounded-full h-3">
            <div
              className="bg-blue-600 h-3 rounded-full transition-all duration-300 ease-out"
              style={{ width: `${Math.min(progress.progress_percentage, 100)}%` }}
            />
          </div>
          <div className="flex justify-between text-xs text-gray-600">
            <span>{progress.stage}</span>
            <span>{Math.round(progress.progress_percentage)}%</span>
          </div>
          <p className="text-sm text-muted-foreground text-center">{progress.message}</p>
        </div>
      )}

      {/* Error display */}
      {status === 'error' && error && (
        <div className="bg-red-50 border border-red-200 rounded-lg p-4 space-y-3">
          <div className="flex items-center gap-2 text-red-700">
            <AlertCircle className="h-5 w-5" />
            <p className="text-sm font-medium">Erreur lors de l'import</p>
          </div>
          <p className="text-sm text-red-600">{error}</p>
          <Button variant="outline" size="sm" onClick={reset}>
            Réessayer
          </Button>
        </div>
      )}

      {/* Action buttons */}
      {!isProcessing && status !== 'error' && (
        <div className="flex justify-end gap-3">
          <Button
            onClick={handleImport}
            disabled={!hasValidFiles}
            className="bg-blue-600 hover:bg-blue-700 text-white"
          >
            <Upload className="h-4 w-4 mr-2" />
            Importer →
          </Button>
        </div>
      )}

      {isProcessing && (
        <div className="flex justify-end">
          <Button variant="outline" onClick={cancelImport}>
            <X className="h-4 w-4 mr-2" />
            Annuler
          </Button>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && pnpm tsc --noEmit 2>&1 | grep "import/page"
```

Expected: no errors for `import/page.tsx`.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/app/import/page.tsx
git commit -m "feat: add /import page with multi-file drop zone and ordered file list"
```

---

## Task 6 — Update `layout.tsx` — remove import dialog and global drag-drop

**Files:**
- Modify: `frontend/src/app/layout.tsx`

- [ ] **Step 1: Replace the full `layout.tsx`**

Remove all import-dialog state, the global drag-drop listeners, `handleFileDrop`, `handleImportDialogClose`, `handleOpenImportDialog`, `ConditionalImportDialog`, `ImportDialogProvider`, `ImportDropOverlay`, and the imports for `ImportAudioDialog`, `ImportDropOverlay`, `ImportDialogProvider`, `loadBetaFeatures`, `isAudioExtension`, `getAudioFormatsDisplayList`.

Replace the entire file with:

```typescript
'use client'

import './globals.css'
import { Source_Sans_3 } from 'next/font/google'
import Sidebar from '@/components/Sidebar'
import { SidebarProvider } from '@/components/Sidebar/SidebarProvider'
import MainContent from '@/components/MainContent'
import { Toaster, toast } from 'sonner'
import "sonner/dist/styles.css"
import { useState, useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { TooltipProvider } from '@/components/ui/tooltip'
import { RecordingStateProvider } from '@/contexts/RecordingStateContext'
import { OllamaDownloadProvider } from '@/contexts/OllamaDownloadContext'
import { TranscriptProvider } from '@/contexts/TranscriptContext'
import { ConfigProvider } from '@/contexts/ConfigContext'
import { OnboardingProvider } from '@/contexts/OnboardingContext'
import { OnboardingFlow } from '@/components/onboarding'
import { DownloadProgressToastProvider } from '@/components/shared/DownloadProgressToast'
import { RecordingPostProcessingProvider } from '@/contexts/RecordingPostProcessingProvider'

const sourceSans3 = Source_Sans_3({
  subsets: ['latin'],
  weight: ['400', '500', '600', '700'],
  variable: '--font-source-sans-3',
})

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const [showOnboarding, setShowOnboarding] = useState(false)
  const [onboardingCompleted, setOnboardingCompleted] = useState(false)

  useEffect(() => {
    invoke<{ completed: boolean } | null>('get_onboarding_status')
      .then((status) => {
        const isComplete = status?.completed ?? false
        setOnboardingCompleted(isComplete)
        if (!isComplete) {
          setShowOnboarding(true)
        }
      })
      .catch(() => {
        setShowOnboarding(true)
        setOnboardingCompleted(false)
      })
  }, [])

  // Disable context menu in production
  useEffect(() => {
    if (process.env.NODE_ENV === 'production') {
      const handle = (e: MouseEvent) => e.preventDefault()
      document.addEventListener('contextmenu', handle)
      return () => document.removeEventListener('contextmenu', handle)
    }
  }, [])

  // Forward tray recording toggle to the recording page
  useEffect(() => {
    const unlisten = listen('request-recording-toggle', () => {
      if (showOnboarding) {
        toast.error('Veuillez d\'abord terminer la configuration.')
      } else {
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'))
      }
    })
    return () => { unlisten.then(fn => fn()) }
  }, [showOnboarding])

  const handleOnboardingComplete = () => {
    setShowOnboarding(false)
    setOnboardingCompleted(true)
    window.location.reload()
  }

  return (
    <html lang="en">
      <body className={`${sourceSans3.variable} font-sans antialiased`}>
        <RecordingStateProvider>
          <TranscriptProvider>
            <ConfigProvider>
              <OllamaDownloadProvider>
                <OnboardingProvider>
                  <SidebarProvider>
                    <TooltipProvider>
                      <RecordingPostProcessingProvider>
                        <DownloadProgressToastProvider />

                        {showOnboarding ? (
                          <OnboardingFlow onComplete={handleOnboardingComplete} />
                        ) : (
                          <div className="flex">
                            <Sidebar />
                            <MainContent>{children}</MainContent>
                          </div>
                        )}
                      </RecordingPostProcessingProvider>
                    </TooltipProvider>
                  </SidebarProvider>
                </OnboardingProvider>
              </OllamaDownloadProvider>
            </ConfigProvider>
          </TranscriptProvider>
        </RecordingStateProvider>

        <Toaster position="bottom-center" richColors closeButton />
      </body>
    </html>
  )
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd frontend && pnpm tsc --noEmit 2>&1 | grep layout
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/app/layout.tsx
git commit -m "refactor: remove import dialog and global drag-drop from layout"
```

---

## Task 7 — Update Sidebar to navigate to `/import`

**Files:**
- Modify: `frontend/src/components/Sidebar/index.tsx`

- [ ] **Step 1: Replace `Sidebar/index.tsx`**

Remove `useImportDialog`, `useConfig`, the beta gate, and replace `openImportDialog()` with `router.push('/import')`. The Upload icon is now always visible.

```typescript
'use client';

import React from 'react';
import { Settings, Mic, NotebookPen, Upload } from 'lucide-react';
import { useRouter, usePathname } from 'next/navigation';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import Info from '../Info';

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();

  const isMeetingsPage = pathname === '/meetings' || pathname?.includes('/meeting-details');
  const isSettingsPage = pathname === '/settings';
  const isHomePage = pathname === '/';
  const isImportPage = pathname === '/import';

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      <div className="h-screen w-16 bg-white border-r shadow-sm flex flex-col">
        <TooltipProvider>
          <div className="flex flex-col items-center space-y-4 mt-4">

            {/* Home / Record */}
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/')}
                  className={`p-2 rounded-full transition-colors duration-150 shadow-sm ${
                    isHomePage ? 'bg-red-600' : 'bg-red-500 hover:bg-red-600'
                  }`}
                >
                  <Mic className="w-5 h-5 text-white" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right"><p>Accueil</p></TooltipContent>
            </Tooltip>

            {/* Import */}
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/import')}
                  className={`p-2 rounded-lg transition-colors duration-150 ${
                    isImportPage ? 'bg-blue-200' : 'bg-blue-50 hover:bg-blue-100'
                  }`}
                >
                  <Upload className="w-5 h-5 text-blue-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right"><p>Importer un audio</p></TooltipContent>
            </Tooltip>

            {/* Meeting notes */}
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/meetings')}
                  className={`p-2 rounded-lg transition-colors duration-150 ${
                    isMeetingsPage ? 'bg-gray-200' : 'hover:bg-gray-100'
                  }`}
                >
                  <NotebookPen className="w-5 h-5 text-gray-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right"><p>Meeting Notes</p></TooltipContent>
            </Tooltip>

            {/* Settings */}
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/settings')}
                  className={`p-2 rounded-lg transition-colors duration-150 ${
                    isSettingsPage ? 'bg-gray-200' : 'hover:bg-gray-100'
                  }`}
                >
                  <Settings className="w-5 h-5 text-gray-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right"><p>Settings</p></TooltipContent>
            </Tooltip>

            <Info isCollapsed={true} />
          </div>
        </TooltipProvider>
      </div>
    </div>
  );
};

export default Sidebar;
```

- [ ] **Step 2: TypeScript check**

```bash
cd frontend && pnpm tsc --noEmit 2>&1 | grep -i sidebar
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/Sidebar/index.tsx
git commit -m "feat: sidebar Upload button navigates to /import (no beta gate)"
```

---

## Task 8 — Delete legacy ImportAudio files

**Files:**
- Delete: `frontend/src/contexts/ImportDialogContext.tsx`
- Delete: `frontend/src/components/ImportAudio/ImportAudioDialog.tsx`
- Delete: `frontend/src/components/ImportAudio/ImportDropOverlay.tsx`
- Delete: `frontend/src/components/ImportAudio/index.ts`

- [ ] **Step 1: Verify no remaining imports of these files**

```bash
grep -rn "ImportDialogContext\|ImportAudioDialog\|ImportDropOverlay\|ImportAudio/index\|useImportAudio\|useImportDialog" \
  frontend/src --include="*.ts" --include="*.tsx"
```

Expected: zero results (if any remain, fix them first).

- [ ] **Step 2: Delete the files**

```bash
rm "frontend/src/contexts/ImportDialogContext.tsx"
rm "frontend/src/components/ImportAudio/ImportAudioDialog.tsx"
rm "frontend/src/components/ImportAudio/ImportDropOverlay.tsx"
rm "frontend/src/components/ImportAudio/index.ts"
```

If `frontend/src/components/ImportAudio/` is now empty, remove the directory too:

```bash
rmdir "frontend/src/components/ImportAudio" 2>/dev/null || true
```

- [ ] **Step 3: Full TypeScript check**

```bash
cd frontend && pnpm tsc --noEmit 2>&1 | head -40
```

Expected: zero errors. If `useImportAudio` is still referenced somewhere (e.g. `RetranscribeDialog`), check and update those files.

- [ ] **Step 4: Cargo check (ensure no Rust regressions)**

```bash
cd frontend && cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add -u
git commit -m "refactor: delete ImportAudioDialog, ImportDropOverlay, ImportDialogContext (replaced by /import page)"
```

---

## Self-Review

**Spec coverage check:**

| Spec requirement | Task |
|---|---|
| Page `/import` dédiée | Task 5 |
| 1–4 fichiers maximum | Task 4 (`MAX_FILES = 4`) |
| Boutons ↑ / ↓ | Task 5 (`FileCard`) |
| Marqueur de jonction `--- Audio N — HH:MM:SS ---` | Task 3 (`format_junction_marker`) |
| Drag-drop global supprimé | Task 6 (layout.tsx) |
| Drag-drop local sur la page `/import` | Task 5 |
| Bouton Parcourir multi-sélection | Task 2 (`select_multiple_audio_files_command`), Task 5 |
| Flag bêta supprimé | Task 1 |
| Sidebar → `router.push('/import')` | Task 7 |
| `AudioFilePart` struct Rust | Task 2 |
| `start_import_multi_command` | Task 2 |
| `run_import_multi` avec offsets | Task 3 |
| `useMultiImport` hook | Task 4 |
| Titre pré-rempli, met à jour si fichier 1 change | Task 5 (effect on `firstValidFile`) |
| Durée totale affichée | Task 5 (`totalDurationSecs`) |
| Options avancées (langue + modèle) | Task 5 (accordion) |
| Progression globale multi-fichiers | Task 3 (`global_pct`) |
| Succès → redirection `/meeting-details` | Task 4 (hook `import-complete`) |
| Erreur → inline + Réessayer | Task 5 |
| Annulation pendant import | Task 4 (`cancelImport`), Task 5 (button) |
| `source: "import_multi"` dans metadata | Task 3 |
| Suppression anciens composants | Task 8 |
| `is_import_in_progress_command` conservé | ✅ not deleted |
| `start_import_audio_command` conservé (RetranscribeDialog) | ✅ not deleted |
