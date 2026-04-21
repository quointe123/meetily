// Audio file import module - allows importing external audio files as new meetings

use crate::api::TranscriptSegment;
use crate::audio::decoder::{decode_audio_file, decode_audio_file_with_progress};
use crate::audio::vad::get_speech_chunks_with_progress;
use crate::config::{DEFAULT_WHISPER_MODEL, DEFAULT_PARAKEET_MODEL};
use crate::parakeet_engine::ParakeetEngine;
use crate::state::AppState;
use crate::whisper_engine::WhisperEngine;
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use super::audio_processing::create_meeting_folder;
use super::common::{create_transcript_segments, split_segment_at_silence, write_transcripts_json};
use super::constants::AUDIO_EXTENSIONS;
use super::recording_preferences::get_default_recordings_folder;

/// Global flag to track if import is in progress
static IMPORT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Global flag to signal cancellation
static IMPORT_CANCELLED: AtomicBool = AtomicBool::new(false);

/// RAII guard for IMPORT_IN_PROGRESS flag
/// Ensures flag is cleared even if import panics or returns early
struct ImportGuard;

impl ImportGuard {
    /// Create guard and set flag atomically
    fn acquire() -> Result<Self, String> {
        if IMPORT_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("Import already in progress".to_string());
        }
        Ok(ImportGuard)
    }
}

impl Drop for ImportGuard {
    fn drop(&mut self) {
        IMPORT_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

/// VAD redemption time in milliseconds - bridges natural pauses in speech
/// Batch processing needs longer redemption (2000ms) than live pipeline (400ms)
/// because the entire file is processed at once by VAD, and 400ms fragments
/// speech at every natural sentence/topic pause (500ms-2s)
const VAD_REDEMPTION_TIME_MS: u32 = 2000;

/// Maximum file size: 20GB (prevents OOM and excessive processing time)
const MAX_FILE_SIZE_BYTES: u64 = 20 * 1024 * 1024 * 1024; // 20GB

/// Information about a selected audio file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFileInfo {
    pub path: String,
    pub filename: String,
    pub duration_seconds: f64,
    pub size_bytes: u64,
    pub format: String,
}

/// Progress update emitted during import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProgress {
    pub stage: String, // "copying", "decoding", "vad", "transcribing", "saving"
    pub progress_percentage: u32,
    pub message: String,
}

/// Result of import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub meeting_id: String,
    pub title: String,
    pub segments_count: usize,
    pub duration_seconds: f64,
}

/// Error during import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportError {
    pub error: String,
}

/// Warning emitted during import (non-fatal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportWarning {
    pub warning: String,
    pub details: Option<String>,
}

/// Response when import is started
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportStarted {
    pub message: String,
}

/// One audio file in an ordered multi-file import list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFilePart {
    pub path: String,
    pub order: u32, // 1-based, already sorted by the frontend
}

/// Check if import is currently in progress
pub fn is_import_in_progress() -> bool {
    IMPORT_IN_PROGRESS.load(Ordering::SeqCst)
}

/// Cancel ongoing import
pub fn cancel_import() {
    IMPORT_CANCELLED.store(true, Ordering::SeqCst);
}

/// Validate an audio file and return its info using metadata-only approach
/// Falls back to full decode if metadata is unavailable
pub fn validate_audio_file(path: &Path) -> Result<AudioFileInfo> {
    // Check file exists
    if !path.exists() {
        return Err(anyhow!("File does not exist: {}", path.display()));
    }

    // Check extension
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if !AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return Err(anyhow!(
            "Unsupported format: .{}. Supported: {}",
            extension,
            AUDIO_EXTENSIONS.join(", ")
        ));
    }

    // Get file size
    let metadata = std::fs::metadata(path)
        .map_err(|e| anyhow!("Cannot read file: {}", e))?;
    let size_bytes = metadata.len();

    // Check file size limit
    if size_bytes > MAX_FILE_SIZE_BYTES {
        return Err(anyhow!(
            "File too large: {:.2}GB. Maximum supported size is {}GB",
            size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            MAX_FILE_SIZE_BYTES / (1024 * 1024 * 1024)
        ));
    }

    // Get filename without extension for title
    let filename = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Audio")
        .to_string();

    // Try fast metadata-only validation first
    let duration_seconds = match extract_duration_from_metadata(path) {
        Ok(duration) => {
            debug!(
                "Got duration from metadata: {:.2}s (fast path)",
                duration
            );
            duration
        }
        Err(e) => {
            // Fallback to full decode if metadata unavailable
            warn!(
                "Metadata extraction failed: {}, falling back to full decode",
                e
            );
            let decoded = decode_audio_file(path)?;
            decoded.duration_seconds
        }
    };

    Ok(AudioFileInfo {
        path: path.to_string_lossy().to_string(),
        filename,
        duration_seconds,
        size_bytes,
        format: extension.to_uppercase(),
    })
}

/// Extract duration from audio file metadata without full decode
/// Returns error if metadata is unavailable, triggering fallback to full decode
fn extract_duration_from_metadata(path: &Path) -> Result<f64> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    // Open the file
    let file = std::fs::File::open(path)
        .map_err(|e| anyhow!("Failed to open audio file: {}", e))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Set up format hint based on file extension
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Probe the file format (lightweight operation)
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| anyhow!("Failed to probe audio format: {}", e))?;

    let format = probed.format;

    // Find the first audio track
    use symphonia::core::codecs::CODEC_TYPE_NULL;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No audio track found in file"))?;

    // Extract duration from metadata
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("Unknown sample rate"))?;

    let n_frames = track
        .codec_params
        .n_frames
        .ok_or_else(|| anyhow!("Frame count not available in metadata"))?;

    let duration_seconds = n_frames as f64 / sample_rate as f64;

    debug!(
        "Extracted metadata: {}Hz, {} frames, {:.2}s",
        sample_rate, n_frames, duration_seconds
    );

    Ok(duration_seconds)
}

/// Start import of an audio file
pub async fn start_import<R: Runtime>(
    app: AppHandle<R>,
    source_path: String,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportResult> {
    // Acquire guard - ensures flag is cleared even on panic/early return
    let _guard = ImportGuard::acquire().map_err(|e| anyhow!(e))?;

    // Reset cancellation flag
    IMPORT_CANCELLED.store(false, Ordering::SeqCst);

    let use_parakeet = provider.as_deref() == Some("parakeet");
    let result = run_import(
        app.clone(),
        source_path,
        title,
        language,
        model,
        provider,
    )
    .await;

    // Unload the engine after the batch job (success, failure, or cancellation)
    super::common::unload_engine_after_batch(use_parakeet).await;

    // Guard will automatically clear flag on drop
    // No need for manual: IMPORT_IN_PROGRESS.store(false, Ordering::SeqCst);

    match &result {
        Ok(res) => {
            let _ = app.emit(
                "import-complete",
                serde_json::json!({
                    "meeting_id": res.meeting_id,
                    "title": res.title,
                    "segments_count": res.segments_count,
                    "duration_seconds": res.duration_seconds
                }),
            );
        }
        Err(e) => {
            let _ = app.emit(
                "import-error",
                ImportError {
                    error: e.to_string(),
                },
            );
        }
    }

    result
}

/// Internal function to run import
async fn run_import<R: Runtime>(
    app: AppHandle<R>,
    source_path: String,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportResult> {
    let source = PathBuf::from(&source_path);

    // Validate source file
    if !source.exists() {
        return Err(anyhow!("Source file not found: {}", source.display()));
    }

    info!(
        "Starting import for '{}' from {} with language {:?}, model {:?}, provider {:?}",
        title, source_path, language, model, provider
    );

    // Determine which provider to use (default to whisper)
    let use_parakeet = provider.as_deref() == Some("parakeet");

    emit_progress(&app, "copying", 5, "Creating meeting folder...");

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Import cancelled"));
    }

    // Create meeting folder
    let base_folder = get_default_recordings_folder();
    let meeting_folder = create_meeting_folder(&base_folder, &title, false)?;

    // Copy audio file to meeting folder
    emit_progress(&app, "copying", 10, "Copying audio file...");

    let dest_filename = format!(
        "audio.{}",
        source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4")
    );
    let dest_path = meeting_folder.join(&dest_filename);

    let src = source.clone();
    let dst = dest_path.clone();
    tokio::task::spawn_blocking(move || std::fs::copy(&src, &dst))
        .await
        .map_err(|e| anyhow!("Copy task join error: {}", e))?
        .map_err(|e| anyhow!("Failed to copy audio file: {}", e))?;

    info!("Copied audio to: {}", dest_path.display());

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        // Cleanup: remove the meeting folder
        let _ = std::fs::remove_dir_all(&meeting_folder);
        return Err(anyhow!("Import cancelled"));
    }

    emit_progress(&app, "decoding", 15, "Decoding audio file...");

    // Decode the audio file with progress updates
    let app_for_decode = app.clone();
    let decode_progress = Box::new(move |progress: u32, msg: &str| {
        // Map decode progress: 15% + (progress * 0.05) to go from 15% to 20%
        let overall_progress = 15 + ((progress as f32 * 0.05) as u32);
        emit_progress(&app_for_decode, "decoding", overall_progress, msg);
    });

    let path_for_decode = dest_path.clone();
    let decoded = tokio::task::spawn_blocking(move || {
        decode_audio_file_with_progress(&path_for_decode, Some(decode_progress))
    })
    .await
    .map_err(|e| anyhow!("Decode task join error: {}", e))??;
    let duration_seconds = decoded.duration_seconds;

    info!(
        "Decoded audio: {:.2}s, {}Hz, {} channels",
        duration_seconds, decoded.sample_rate, decoded.channels
    );

    emit_progress(&app, "resampling", 20, "Converting audio format...");

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&meeting_folder);
        return Err(anyhow!("Import cancelled"));
    }

    // Convert to 16kHz mono format with progress updates
    let app_for_resample = app.clone();
    let resample_progress = Box::new(move |progress: u32, msg: &str| {
        // Map resample progress: 20% + (progress * 0.05) to go from 20% to 25%
        let overall_progress = 20 + ((progress as f32 * 0.05) as u32);
        emit_progress(&app_for_resample, "resampling", overall_progress, msg);
    });

    let audio_samples = tokio::task::spawn_blocking(move || {
        decoded.to_whisper_format_with_progress(Some(resample_progress))
    })
    .await
    .map_err(|e| anyhow!("Resample task join error: {}", e))?;
    info!(
        "Converted to 16kHz mono format: {} samples",
        audio_samples.len()
    );

    emit_progress(&app, "vad", 25, "Detecting speech segments...");

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&meeting_folder);
        return Err(anyhow!("Import cancelled"));
    }

    // Use VAD to find speech segments
    let app_for_vad = app.clone();

    let speech_segments = tokio::task::spawn_blocking(move || {
        get_speech_chunks_with_progress(
            &audio_samples,
            VAD_REDEMPTION_TIME_MS,
            |vad_progress, segments_found| {
                let overall_progress = 25 + (vad_progress as f32 * 0.05) as u32;
                emit_progress(
                    &app_for_vad,
                    "vad",
                    overall_progress,
                    &format!(
                        "Detecting speech segments... {}% ({} found)",
                        vad_progress, segments_found
                    ),
                );
                !IMPORT_CANCELLED.load(Ordering::SeqCst)
            },
        )
    })
    .await
    .map_err(|e| anyhow!("VAD task panicked: {}", e))?
    .map_err(|e| anyhow!("VAD processing failed: {}", e))?;

    let total_segments = speech_segments.len();
    info!("VAD detected {} speech segments (redemption_time={}ms)", total_segments, VAD_REDEMPTION_TIME_MS);

    // Diagnostic: log segment duration distribution
    if !speech_segments.is_empty() {
        let durations_ms: Vec<f64> = speech_segments.iter()
            .map(|s| s.end_timestamp_ms - s.start_timestamp_ms)
            .collect();
        let total_speech_ms: f64 = durations_ms.iter().sum();
        let avg_duration = total_speech_ms / durations_ms.len() as f64;
        let min_duration = durations_ms.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_duration = durations_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        info!(
            "VAD segment stats: avg={:.0}ms, min={:.0}ms, max={:.0}ms, total_speech={:.1}s/{:.1}s ({:.0}%)",
            avg_duration, min_duration, max_duration,
            total_speech_ms / 1000.0, duration_seconds,
            (total_speech_ms / 1000.0 / duration_seconds) * 100.0
        );
        // Log first 10 segments for detailed inspection
        for (i, seg) in speech_segments.iter().take(10).enumerate() {
            let dur = seg.end_timestamp_ms - seg.start_timestamp_ms;
            debug!("  Segment {}: {:.0}ms-{:.0}ms ({:.0}ms, {} samples)",
                i, seg.start_timestamp_ms, seg.end_timestamp_ms, dur, seg.samples.len());
        }
        if total_segments > 10 {
            debug!("  ... and {} more segments", total_segments - 10);
        }
    }

    if total_segments == 0 {
        warn!("No speech detected in audio");

        // Emit warning to frontend
        let _ = app.emit(
            "import-warning",
            ImportWarning {
                warning: "No speech detected in audio file".to_string(),
                details: Some(
                    "The file was imported successfully, but VAD did not detect any speech. \
                     The meeting was created but contains no transcripts.".to_string()
                ),
            },
        );
        // Still create the meeting, just with no transcripts
    }

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&meeting_folder);
        return Err(anyhow!("Import cancelled"));
    }

    emit_progress(&app, "transcribing", 30, "Loading transcription engine...");

    // Initialize the appropriate engine
    let whisper_engine = if !use_parakeet && total_segments > 0 {
        Some(get_or_init_whisper(&app, model.as_deref()).await?)
    } else {
        None
    };
    let parakeet_engine = if use_parakeet && total_segments > 0 {
        Some(get_or_init_parakeet(&app, model.as_deref()).await?)
    } else {
        None
    };

    // Split very long segments at silence boundaries for better transcription quality.
    // Hard cuts at arbitrary sample positions lose words at boundaries. Instead, scan
    // for the lowest-energy window near the target split point and cut there.
    const MAX_SEGMENT_SAMPLES: usize = 25 * 16000; // 25 seconds at 16kHz

    let mut processable_segments: Vec<crate::audio::vad::SpeechSegment> = Vec::new();
    for segment in &speech_segments {
        if segment.samples.len() > MAX_SEGMENT_SAMPLES {
            debug!(
                "Splitting large segment ({:.0}ms, {} samples) at silence boundaries",
                segment.end_timestamp_ms - segment.start_timestamp_ms,
                segment.samples.len()
            );

            let sub_segments = split_segment_at_silence(segment, MAX_SEGMENT_SAMPLES);
            debug!("Split into {} sub-segments", sub_segments.len());
            processable_segments.extend(sub_segments);
        } else {
            processable_segments.push(segment.clone());
        }
    }

    let processable_count = processable_segments.len();
    info!("Processing {} segments (after splitting)", processable_count);

    // Process each speech segment
    let mut all_transcripts: Vec<(String, f64, f64)> = Vec::new();
    let mut total_confidence = 0.0f32;

    for (i, segment) in processable_segments.iter().enumerate() {
        if IMPORT_CANCELLED.load(Ordering::SeqCst) {
            let _ = std::fs::remove_dir_all(&meeting_folder);
            return Err(anyhow!("Import cancelled"));
        }

        let progress = 30 + ((i as f32 / processable_count.max(1) as f32) * 50.0) as u32;
        let segment_duration_sec = (segment.end_timestamp_ms - segment.start_timestamp_ms) / 1000.0;
        emit_progress(
            &app,
            "transcribing",
            progress,
            &format!(
                "Transcribing segment {} of {} ({:.1}s)...",
                i + 1,
                processable_count,
                segment_duration_sec
            ),
        );

        // Skip very short segments
        if segment.samples.len() < 1600 {
            debug!(
                "Skipping short segment {} with {} samples",
                i,
                segment.samples.len()
            );
            continue;
        }

        // Normalize peak before ASR — both Parakeet and Whisper benefit from consistent amplitude.
        let mut samples = segment.samples.clone();
        let gain = crate::audio::normalize::peak_normalize(&mut samples);
        log::debug!("Segment {}: peak-normalized with gain={:.2}", i, gain);

        // Transcribe
        let (text, conf) = if use_parakeet {
            let engine = parakeet_engine.as_ref().unwrap();
            let text = engine
                .transcribe_audio(samples)
                .await
                .map_err(|e| anyhow!("Parakeet transcription failed on segment {}: {}", i, e))?;
            (text, 0.9f32)
        } else {
            let engine = whisper_engine.as_ref().unwrap();
            let (text, conf, _) = engine
                .transcribe_audio_with_confidence(samples, language.clone())
                .await
                .map_err(|e| anyhow!("Whisper transcription failed on segment {}: {}", i, e))?;
            (text, conf)
        };

        let trimmed = text.trim();
        if trimmed.is_empty() {
            debug!("Segment {}/{}: {:.1}s — empty transcription", i + 1, processable_count, segment_duration_sec);
        } else {
            let lang_check = crate::audio::language_filter::check_language(trimmed, language.as_deref());
            if lang_check == crate::audio::language_filter::LanguageCheck::Mismatch {
                log::info!(
                    "Segment {}/{}: {:.1}s — dropped (language mismatch vs expected={:?}), text='{}'",
                    i + 1, processable_count, segment_duration_sec, language,
                    if trimmed.len() > 80 { let mut end = 80; while !trimmed.is_char_boundary(end) { end -= 1; } &trimmed[..end] } else { trimmed }
                );
                // Do not push to all_transcripts and do not count in confidence.
            } else {
                debug!(
                    "Segment {}/{}: {:.1}s, conf={:.2}, lang_check={:?}, text='{}'",
                    i + 1, processable_count, segment_duration_sec, conf, lang_check,
                    if trimmed.len() > 80 { let mut end = 80; while !trimmed.is_char_boundary(end) { end -= 1; } &trimmed[..end] } else { trimmed }
                );
                all_transcripts.push((text, segment.start_timestamp_ms, segment.end_timestamp_ms));
                total_confidence += conf;
            }
        }
    }

    let transcribed_count = all_transcripts.len();
    let avg_confidence = if transcribed_count > 0 {
        total_confidence / transcribed_count as f32
    } else {
        0.0
    };

    info!(
        "Transcription complete: {} segments transcribed out of {}, avg confidence: {:.2}",
        transcribed_count, processable_count, avg_confidence
    );

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&meeting_folder);
        return Err(anyhow!("Import cancelled"));
    }

    emit_progress(&app, "saving", 85, "Creating meeting...");

    // Create transcript segments
    let segments = create_transcript_segments(&all_transcripts);

    // Save to database
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

    // Write transcripts.json and metadata.json to the meeting folder
    emit_progress(&app, "saving", 90, "Writing transcript files...");

    if let Err(e) = write_transcripts_json(&meeting_folder, &segments) {
        warn!("Failed to write transcripts.json: {}", e);
    }

    if let Err(e) = write_import_metadata(
        &meeting_folder,
        &meeting_id,
        &title,
        duration_seconds,
        &dest_filename,
        "import",
    ) {
        warn!("Failed to write metadata.json: {}", e);
    }

    emit_progress(&app, "complete", 100, "Import complete");

    Ok(ImportResult {
        meeting_id,
        title,
        segments_count: segments.len(),
        duration_seconds,
    })
}

/// Emit progress event
fn emit_progress<R: Runtime>(app: &AppHandle<R>, stage: &str, progress: u32, message: &str) {
    let _ = app.emit(
        "import-progress",
        ImportProgress {
            stage: stage.to_string(),
            progress_percentage: progress,
            message: message.to_string(),
        },
    );
}


/// Create a new meeting with transcripts in the database
async fn create_meeting_with_transcripts(
    pool: &sqlx::SqlitePool,
    title: &str,
    segments: &[TranscriptSegment],
    folder_path: String,
) -> Result<String> {
    let meeting_id = format!("meeting-{}", Uuid::new_v4());
    let now = chrono::Utc::now();

    // Start transaction
    let mut conn = pool.acquire().await.map_err(|e| anyhow!("DB error: {}", e))?;
    let mut tx = sqlx::Connection::begin(&mut *conn)
        .await
        .map_err(|e| anyhow!("Failed to start transaction: {}", e))?;

    // Insert meeting
    sqlx::query(
        "INSERT INTO meetings (id, title, created_at, updated_at, folder_path)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&meeting_id)
    .bind(title)
    .bind(now)
    .bind(now)
    .bind(&folder_path)
    .execute(&mut *tx)
    .await
    .map_err(|e| anyhow!("Failed to create meeting: {}", e))?;

    // Insert transcripts
    for segment in segments {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&segment.id)
        .bind(&meeting_id)
        .bind(&segment.text)
        .bind(&segment.timestamp)
        .bind(segment.audio_start_time)
        .bind(segment.audio_end_time)
        .bind(segment.duration)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("Failed to insert transcript: {}", e))?;
    }

    tx.commit()
        .await
        .map_err(|e| anyhow!("Failed to commit transaction: {}", e))?;

    info!(
        "Created meeting '{}' with {} transcripts",
        meeting_id,
        segments.len()
    );

    Ok(meeting_id)
}

/// Get or initialize the Whisper engine
async fn get_or_init_whisper<R: Runtime>(
    app: &AppHandle<R>,
    requested_model: Option<&str>,
) -> Result<Arc<WhisperEngine>> {
    use crate::whisper_engine::commands::WHISPER_ENGINE;

    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().cloned()
    };

    match engine {
        Some(e) => {
            let target_model = match requested_model {
                Some(model) => model.to_string(),
                None => get_configured_model(app, "whisper").await?,
            };

            let current_model = e.get_current_model().await;
            let needs_load = match &current_model {
                Some(loaded) => loaded != &target_model,
                None => true,
            };

            if needs_load {
                info!(
                    "Loading Whisper model '{}' (current: {:?})",
                    target_model, current_model
                );

                if let Err(e) = e.discover_models().await {
                    warn!("Model discovery error (continuing): {}", e);
                }

                e.load_model(&target_model)
                    .await
                    .map_err(|e| anyhow!("Failed to load model '{}': {}", target_model, e))?;
            }

            Ok(e)
        }
        None => Err(anyhow!("Whisper engine not initialized")),
    }
}

/// Get or initialize the Parakeet engine
async fn get_or_init_parakeet<R: Runtime>(
    app: &AppHandle<R>,
    requested_model: Option<&str>,
) -> Result<Arc<ParakeetEngine>> {
    use crate::parakeet_engine::commands::PARAKEET_ENGINE;

    let engine = {
        let guard = PARAKEET_ENGINE.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().cloned()
    };

    match engine {
        Some(e) => {
            let target_model = match requested_model {
                Some(model) => model.to_string(),
                None => get_configured_model(app, "parakeet").await?,
            };

            let current_model = e.get_current_model().await;
            let needs_load = match &current_model {
                Some(loaded) => loaded != &target_model,
                None => true,
            };

            if needs_load {
                info!(
                    "Loading Parakeet model '{}' (current: {:?})",
                    target_model, current_model
                );

                if let Err(e) = e.discover_models().await {
                    warn!("Model discovery error (continuing): {}", e);
                }

                e.load_model(&target_model)
                    .await
                    .map_err(|e| anyhow!("Failed to load model '{}': {}", target_model, e))?;
            }

            Ok(e)
        }
        None => Err(anyhow!("Parakeet engine not initialized")),
    }
}

/// Get the configured model from database
async fn get_configured_model<R: Runtime>(app: &AppHandle<R>, provider_type: &str) -> Result<String> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    let result: Option<(String, String)> = sqlx::query_as(
        "SELECT provider, model FROM transcript_settings WHERE id = '1'",
    )
    .fetch_optional(app_state.db_manager.pool())
    .await
    .map_err(|e| anyhow!("Failed to query config: {}", e))?;

    match result {
        Some((provider, model)) => {
            if (provider_type == "whisper" && (provider == "localWhisper" || provider == "whisper"))
                || (provider_type == "parakeet" && provider == "parakeet")
            {
                Ok(model)
            } else {
                // Return default model for the requested type
                Ok(if provider_type == "parakeet" {
                    DEFAULT_PARAKEET_MODEL.to_string()
                } else {
                    DEFAULT_WHISPER_MODEL.to_string()
                })
            }
        }
        None => Ok(if provider_type == "parakeet" {
            DEFAULT_PARAKEET_MODEL.to_string()
        } else {
            DEFAULT_WHISPER_MODEL.to_string()
        }),
    }
}

/// Write metadata.json to a meeting folder (atomic write with temp file)
fn write_import_metadata(
    folder: &Path,
    meeting_id: &str,
    title: &str,
    duration_seconds: f64,
    audio_filename: &str,
    source: &str,
) -> Result<()> {
    let metadata_path = folder.join("metadata.json");
    let temp_path = folder.join(".metadata.json.tmp");
    let now = chrono::Utc::now().to_rfc3339();

    let json = serde_json::json!({
        "version": "1.0",
        "meeting_id": meeting_id,
        "meeting_name": title,
        "created_at": now,
        "completed_at": now,
        "duration_seconds": duration_seconds,
        "audio_file": audio_filename,
        "transcript_file": "transcripts.json",
        "status": "completed",
        "source": source
    });

    let json_string = serde_json::to_string_pretty(&json)?;
    std::fs::write(&temp_path, &json_string)?;
    std::fs::rename(&temp_path, &metadata_path)?;

    info!("Wrote metadata.json to {}", metadata_path.display());
    Ok(())
}

/// Format "--- Audio N — HH:MM:SS ---" from a millisecond offset
fn format_junction_marker(file_n: usize, offset_ms: f64) -> String {
    let total_secs = (offset_ms / 1000.0) as u64;
    let hh = total_secs / 3600;
    let mm = (total_secs % 3600) / 60;
    let ss = total_secs % 60;
    format!("--- Audio {} \u{2014} {:02}:{:02}:{:02} ---", file_n, hh, mm, ss)
}

/// Map (offset + within_file_progress) to a global percentage in [lo, hi]
fn global_pct(
    offset_ms: f64,
    within_file_ms: f64,
    total_ms: f64,
    lo: u32,
    hi: u32,
) -> u32 {
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

/// Multi-file import pipeline — processes multiple audio files as a single meeting
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
            global_pct(timestamp_offset_ms, 0.0, total_estimated_ms, 5, 85),
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
            global_pct(timestamp_offset_ms, 0.05 * estimated_durations_ms[file_idx], total_estimated_ms, 5, 85),
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
            global_pct(timestamp_offset_ms, 0.1 * file_duration_ms, total_estimated_ms, 5, 85),
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
                        global_pct(offset_for_vad, within_file, total_est_for_vad, 5, 85),
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
                global_pct(timestamp_offset_ms, within_file, total_estimated_ms, 5, 85),
                &format!(
                    "Transcription audio {} de {} — segment {}/{} ({})...",
                    file_n,
                    total_files,
                    i + 1,
                    processable_count,
                    format_duration_secs(file_duration_ms / 1000.0)
                ),
            );

            // Normalize peak before ASR — both Parakeet and Whisper benefit from consistent amplitude.
            let mut samples = segment.samples.clone();
            let gain = crate::audio::normalize::peak_normalize(&mut samples);
            log::debug!("File {}, segment {}: peak-normalized with gain={:.2}", file_n, i, gain);

            let (text, _conf) = if use_parakeet {
                let engine = parakeet_engine.as_ref().unwrap();
                let t = engine
                    .transcribe_audio(samples)
                    .await
                    .map_err(|e| anyhow!("Parakeet failed on file {}, seg {}: {}", file_n, i, e))?;
                (t, 0.9f32)
            } else {
                let engine = whisper_engine.as_ref().unwrap();
                let (t, c, _) = engine
                    .transcribe_audio_with_confidence(samples, language.clone())
                    .await
                    .map_err(|e| anyhow!("Whisper failed on file {}, seg {}: {}", file_n, i, e))?;
                (t, c)
            };

            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let lang_check = crate::audio::language_filter::check_language(trimmed, language.as_deref());
                if lang_check == crate::audio::language_filter::LanguageCheck::Mismatch {
                    log::info!(
                        "File {}, segment {}/{} — dropped (language mismatch vs expected={:?}), text='{}'",
                        file_n, i + 1, processable_count, language,
                        if trimmed.len() > 80 { let mut end = 80; while !trimmed.is_char_boundary(end) { end -= 1; } &trimmed[..end] } else { trimmed }
                    );
                    // Do not push to all_transcripts.
                } else {
                    all_transcripts.push((
                        text,
                        segment.start_timestamp_ms + timestamp_offset_ms,
                        segment.end_timestamp_ms + timestamp_offset_ms,
                    ));
                }
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

    if let Err(e) = write_import_metadata(
        &meeting_folder,
        &meeting_id,
        &title,
        actual_total_duration_ms / 1000.0,
        &format!("{} fichiers audio", total_files),
        "import_multi",
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

// ============================================================================
// Tauri Commands
// ============================================================================

/// Select an audio file and validate it
#[tauri::command]
pub async fn select_and_validate_audio_command<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<AudioFileInfo>, String> {
    info!("Opening file dialog for audio import");

    // Use spawn_blocking to avoid blocking async runtime
    let app_clone = app.clone();
    let file_path = tokio::task::spawn_blocking(move || {
        app_clone
            .dialog()
            .file()
            .add_filter("Audio Files", &AUDIO_EXTENSIONS.iter().map(|s| *s).collect::<Vec<_>>())
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("File dialog task failed: {}", e))?;

    match file_path {
        Some(path) => {
            let path_str = path.to_string();
            info!("User selected: {}", path_str);

            match validate_audio_file(Path::new(&path_str)) {
                Ok(info) => Ok(Some(info)),
                Err(e) => {
                    error!("Validation failed: {}", e);
                    Err(e.to_string())
                }
            }
        }
        None => {
            info!("User cancelled file selection");
            Ok(None)
        }
    }
}

/// Validate an audio file from a given path (for drag-drop)
#[tauri::command]
pub async fn validate_audio_file_command(path: String) -> Result<AudioFileInfo, String> {
    info!("Validating audio file: {}", path);
    validate_audio_file(Path::new(&path)).map_err(|e| e.to_string())
}

/// Start importing an audio file (single-file path, kept for retranscribe dialog compatibility)
#[tauri::command]
pub async fn start_import_audio_command<R: Runtime>(
    app: AppHandle<R>,
    source_path: String,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportStarted, String> {
    // Check if import is already in progress (guard will be acquired in start_import)
    if IMPORT_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err("Import already in progress".to_string());
    }

    // Spawn import in background
    tauri::async_runtime::spawn(async move {
        let result = start_import(app, source_path, title, language, model, provider).await;

        if let Err(e) = result {
            error!("Import failed: {}", e);
        }
    });

    Ok(ImportStarted {
        message: "Import started".to_string(),
    })
}

/// Cancel ongoing import
#[tauri::command]
pub async fn cancel_import_command() -> Result<(), String> {
    if !is_import_in_progress() {
        return Err("No import in progress".to_string());
    }
    cancel_import();
    Ok(())
}

/// Check if import is in progress
#[tauri::command]
pub async fn is_import_in_progress_command() -> bool {
    is_import_in_progress()
}

/// Open a multi-file picker and return the selected paths (no validation)
#[tauri::command]
pub async fn select_multiple_audio_files_command<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<String>, String> {
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
    const MAX_IMPORT_FILES: usize = 4;
    if parts.len() > MAX_IMPORT_FILES {
        return Err(format!("Too many files: max {} allowed", MAX_IMPORT_FILES));
    }

    // Sort by order field (frontend should already send them sorted)
    let mut sorted_parts = parts;
    sorted_parts.sort_by_key(|p| p.order);

    tauri::async_runtime::spawn(async move {
        let use_parakeet = provider.as_deref() == Some("parakeet");

        let result = if sorted_parts.len() == 1 {
            // Single file: delegate to existing start_import (zero regression)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_extensions() {
        assert!(AUDIO_EXTENSIONS.contains(&"mp4"));
        assert!(AUDIO_EXTENSIONS.contains(&"wav"));
        assert!(AUDIO_EXTENSIONS.contains(&"mp3"));
        assert!(!AUDIO_EXTENSIONS.contains(&"txt"));
    }

    #[test]
    fn test_create_transcript_segments_empty() {
        let transcripts: Vec<(String, f64, f64)> = vec![];
        let segments = create_transcript_segments(&transcripts);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_create_transcript_segments_single() {
        let transcripts = vec![("Hello world".to_string(), 0.0, 1500.0)];
        let segments = create_transcript_segments(&transcripts);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello world");
        assert_eq!(segments[0].audio_start_time, Some(0.0));
        assert_eq!(segments[0].audio_end_time, Some(1.5));
    }

    #[test]
    fn test_cancellation_flag() {
        IMPORT_CANCELLED.store(false, Ordering::SeqCst);
        IMPORT_IN_PROGRESS.store(false, Ordering::SeqCst);

        assert!(!is_import_in_progress());

        cancel_import();
        assert!(IMPORT_CANCELLED.load(Ordering::SeqCst));

        // Reset
        IMPORT_CANCELLED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn test_extract_duration_from_metadata_wav() {
        // Test with sample WAV file if available
        let test_path = Path::new("../../backend/whisper.cpp/samples/jfk.wav");
        if test_path.exists() {
            let result = extract_duration_from_metadata(test_path);
            // Should succeed and return a reasonable duration
            assert!(result.is_ok());
            let duration = result.unwrap();
            assert!(duration > 0.0 && duration < 60.0, "Duration {} seems unreasonable", duration);
        }
    }

    #[test]
    fn test_extract_duration_from_metadata_mp3() {
        // Test with sample MP3 file if available
        let test_path = Path::new("../../backend/whisper.cpp/samples/jfk.mp3");
        if test_path.exists() {
            let result = extract_duration_from_metadata(test_path);
            // MP3 files may not have n_frames metadata, so fallback is expected
            // We just verify it doesn't panic
            let _ = result;
        }
    }

    #[test]
    fn test_validate_audio_file_with_metadata() {
        // Test validation with actual audio file
        let test_path = Path::new("../../backend/whisper.cpp/samples/jfk.wav");
        if test_path.exists() {
            let result = validate_audio_file(test_path);
            assert!(result.is_ok());
            let info = result.unwrap();
            assert_eq!(info.format, "WAV");
            assert!(info.duration_seconds > 0.0);
            assert!(info.size_bytes > 0);
        }
    }

    #[test]
    fn test_validate_audio_file_nonexistent() {
        let result = validate_audio_file(Path::new("/nonexistent/file.mp4"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_validate_audio_file_wrong_extension() {
        // Create a temporary file with wrong extension
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_audio.txt");
        let _ = std::fs::write(&temp_file, b"dummy content");

        let result = validate_audio_file(&temp_file);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported format"));

        // Cleanup
        let _ = std::fs::remove_file(temp_file);
    }

    #[test]
    fn test_split_segment_at_silence_short_segment() {
        // Segment shorter than max — returned as-is
        let segment = crate::audio::vad::SpeechSegment {
            samples: vec![0.1; 16000], // 1 second
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 1000.0,
            confidence: 0.9,
        };
        let result = split_segment_at_silence(&segment, 25 * 16000);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].samples.len(), 16000);
    }

    #[test]
    fn test_split_segment_at_silence_splits_long_segment() {
        // 60-second segment of low-level noise with a silent gap at ~25s
        let mut samples = vec![0.01f32; 60 * 16000];
        // Insert silence at 25 seconds (sample 400000)
        for i in (25 * 16000)..(25 * 16000 + 3200) {
            samples[i] = 0.0;
        }
        let segment = crate::audio::vad::SpeechSegment {
            samples,
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 60_000.0,
            confidence: 0.9,
        };

        let result = split_segment_at_silence(&segment, 25 * 16000);
        assert!(result.len() >= 2, "Should split into at least 2 segments, got {}", result.len());

        // All sub-segments should have samples
        for (i, seg) in result.iter().enumerate() {
            assert!(!seg.samples.is_empty(), "Segment {} is empty", i);
            assert!(
                seg.start_timestamp_ms < seg.end_timestamp_ms,
                "Segment {} has invalid timestamps: {} >= {}",
                i, seg.start_timestamp_ms, seg.end_timestamp_ms
            );
        }
    }

    #[test]
    fn test_split_segment_at_silence_no_silence_uses_overlap() {
        // Continuous speech (constant energy) — should still split with overlap
        let segment = crate::audio::vad::SpeechSegment {
            samples: vec![0.5f32; 60 * 16000], // 60 seconds of "speech"
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 60_000.0,
            confidence: 0.9,
        };

        let result = split_segment_at_silence(&segment, 25 * 16000);
        assert!(result.len() >= 2);

        // Total samples should exceed input due to overlap
        let total_samples: usize = result.iter().map(|s| s.samples.len()).sum();
        assert!(total_samples >= 60 * 16000, "Overlap should not lose samples");
    }

    #[test]
    fn test_write_transcripts_json() {
        let dir = tempfile::tempdir().unwrap();
        let segments = vec![
            TranscriptSegment {
                id: "t-1".to_string(),
                text: "Hello world".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                audio_start_time: Some(0.0),
                audio_end_time: Some(1.5),
                duration: Some(1.5),
            },
            TranscriptSegment {
                id: "t-2".to_string(),
                text: "Second segment".to_string(),
                timestamp: "2024-01-01T00:00:01Z".to_string(),
                audio_start_time: Some(2.0),
                audio_end_time: Some(3.5),
                duration: Some(1.5),
            },
        ];

        let result = write_transcripts_json(dir.path(), &segments);
        assert!(result.is_ok(), "write_transcripts_json failed: {:?}", result);

        // Verify file exists and is valid JSON
        let path = dir.path().join("transcripts.json");
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["total_segments"], 2);
        assert_eq!(parsed["version"], "1.0");
        assert_eq!(parsed["segments"][0]["text"], "Hello world");
        assert_eq!(parsed["segments"][1]["text"], "Second segment");
        assert_eq!(parsed["segments"][0]["sequence_id"], 0);
        assert_eq!(parsed["segments"][1]["sequence_id"], 1);

        // Verify temp file was cleaned up
        assert!(!dir.path().join(".transcripts.json.tmp").exists());
    }

    #[test]
    fn test_write_import_metadata() {
        let dir = tempfile::tempdir().unwrap();

        let result = write_import_metadata(
            dir.path(),
            "meeting-123",
            "Test Meeting",
            1800.0,
            "audio.mp4",
            "import",
        );
        assert!(result.is_ok(), "write_import_metadata failed: {:?}", result);

        let path = dir.path().join("metadata.json");
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["version"], "1.0");
        assert_eq!(parsed["meeting_id"], "meeting-123");
        assert_eq!(parsed["meeting_name"], "Test Meeting");
        assert_eq!(parsed["duration_seconds"], 1800.0);
        assert_eq!(parsed["audio_file"], "audio.mp4");
        assert_eq!(parsed["status"], "completed");
        assert_eq!(parsed["source"], "import");
    }

    #[test]
    fn test_junction_marker_format() {
        // 0 ms offset → "--- Audio 2 — 00:00:00 ---"
        let marker = format_junction_marker(2, 0.0);
        assert_eq!(marker, "--- Audio 2 — 00:00:00 ---");

        // 90 seconds = 1 min 30 sec → "--- Audio 3 — 00:01:30 ---"
        let marker2 = format_junction_marker(3, 90_000.0);
        assert_eq!(marker2, "--- Audio 3 — 00:01:30 ---");

        // 3661 seconds = 1h 1min 1sec → "--- Audio 2 — 01:01:01 ---"
        let marker3 = format_junction_marker(2, 3_661_000.0);
        assert_eq!(marker3, "--- Audio 2 — 01:01:01 ---");
    }

    /// Integration test that decodes a real audio file and runs VAD.
    /// Run with: TEST_AUDIO_PATH=/path/to/audio.mp4 cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn test_import_pipeline_decode_vad() {
        let audio_path = std::env::var("TEST_AUDIO_PATH")
            .expect("Set TEST_AUDIO_PATH to run this integration test");

        let path = Path::new(&audio_path);
        assert!(path.exists(), "Audio file not found: {}", audio_path);

        // Step 1: Decode
        println!("Decoding {}...", audio_path);
        let decoded = crate::audio::decoder::decode_audio_file(path)
            .expect("Failed to decode audio file");
        println!(
            "Decoded: {:.2}s, {}Hz, {} channels, {} samples",
            decoded.duration_seconds,
            decoded.sample_rate,
            decoded.channels,
            decoded.samples.len()
        );

        // Step 2: Resample to 16kHz mono
        println!("Resampling to 16kHz mono...");
        let samples = decoded.to_whisper_format();
        println!("Resampled: {} samples ({:.2}s at 16kHz)", samples.len(), samples.len() as f64 / 16000.0);

        // Step 3: Run VAD with both redemption times and compare
        for redemption_ms in [400u32, 2000] {
            println!("\n--- VAD with redemption_time={}ms ---", redemption_ms);
            let segments = crate::audio::vad::get_speech_chunks_with_progress(
                &samples,
                redemption_ms,
                |progress, count| {
                    if progress % 20 == 0 {
                        println!("  VAD progress: {}% ({} segments)", progress, count);
                    }
                    true
                },
            ).expect("VAD failed");

            let total_segments = segments.len();
            println!("Found {} segments", total_segments);

            if !segments.is_empty() {
                let durations: Vec<f64> = segments.iter()
                    .map(|s| s.end_timestamp_ms - s.start_timestamp_ms)
                    .collect();
                let total_speech: f64 = durations.iter().sum();
                let avg = total_speech / durations.len() as f64;
                let min = durations.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = durations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

                println!(
                    "Stats: avg={:.0}ms, min={:.0}ms, max={:.0}ms, total_speech={:.1}s/{:.1}s ({:.0}%)",
                    avg, min, max,
                    total_speech / 1000.0,
                    decoded.duration_seconds,
                    (total_speech / 1000.0 / decoded.duration_seconds) * 100.0
                );

                // Segments over 25s that would be split
                let oversized = durations.iter().filter(|d| **d > 25_000.0).count();
                println!("Segments >25s (would be split): {}", oversized);

                // Basic sanity checks
                assert!(total_speech > 0.0, "No speech detected");
                for (i, seg) in segments.iter().enumerate() {
                    assert!(!seg.samples.is_empty(), "Segment {} has no samples", i);
                    assert!(
                        seg.end_timestamp_ms > seg.start_timestamp_ms,
                        "Segment {} has invalid timestamps",
                        i
                    );
                }
            }
        }
    }
}
