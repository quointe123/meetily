# Model Catalog Centralization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace three scattered hardcoded model catalogs (Gemma LLM, Whisper STT, Parakeet STT) with a single JSON file bundled as a Tauri resource, swap the non-owned CDN URLs to HuggingFace sources, add `gemma4:e2b`, and remove `gemma3:4b`.

**Architecture:** A new `models_catalog` Rust module parses the bundled JSON at app startup (via Tauri `setup` hook) and caches the result in a `OnceLock`. All three existing engines (LLM in `summary/summary_engine/models.rs`, Whisper in `whisper_engine/`, Parakeet in `parakeet_engine/`) switch from hardcoded arrays to reading the catalog. The existing public functions keep their signatures so downstream call sites don't change.

**Tech Stack:** Rust 1.x, Tauri 2.x, `serde_json`, `once_cell`/`std::sync::OnceLock`, `anyhow`.

**Spec reference:** [docs/superpowers/specs/2026-04-21-models-catalog-design.md](../specs/2026-04-21-models-catalog-design.md)

---

## File Structure

| File | Role | Change |
|---|---|---|
| `frontend/src-tauri/src/models_catalog.rs` | Module: load + cache JSON, expose typed views | **Create** |
| `frontend/src-tauri/resources/models_catalog.json` | The catalog data | **Create** |
| `frontend/src-tauri/tests/catalog_validation.rs` | Integration test for bundled catalog | **Create** |
| `frontend/src-tauri/tauri.conf.json` | Add resource to bundle | Modify (1 line) |
| `frontend/src-tauri/src/lib.rs` | Declare module, call init in setup | Modify (~6 lines) |
| `frontend/src-tauri/src/summary/summary_engine/models.rs` | Reads catalog instead of hardcoded array | Modify (replace `get_available_models` body) |
| `frontend/src-tauri/src/whisper_engine/whisper_engine.rs` | Reads catalog for URL + `discover_models` | Modify (two sites) |
| `frontend/src-tauri/src/whisper_engine/commands.rs` | Stop using `WHISPER_MODEL_CATALOG` | Modify |
| `frontend/src-tauri/src/config.rs` | Remove `WHISPER_MODEL_CATALOG` constant | Modify (delete ~20 lines) |
| `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs` | Reads catalog for `discover_models` + URL | Modify (two sites) |

**Unchanged:** UI (no Tauri command signature change), `Cargo.toml` (all needed deps already present: `serde_json`, `serde`, `once_cell`, `anyhow`).

---

## Task 1: Scaffold the catalog module with types, JSON, and startup init

**Files:**
- Create: `frontend/src-tauri/src/models_catalog.rs`
- Create: `frontend/src-tauri/resources/models_catalog.json`
- Modify: `frontend/src-tauri/tauri.conf.json:98-100` (add to `bundle.resources`)
- Modify: `frontend/src-tauri/src/lib.rs` (declare module + call init in `setup`)

### - [ ] Step 1.1: Create the JSON catalog file

Create `frontend/src-tauri/resources/models_catalog.json` with the following content:

```json
{
  "schema_version": 1,
  "llm": [
    {
      "id": "gemma3:1b",
      "display_name": "Gemma 3 1B (Fast)",
      "gguf_file": "gemma-3-1b-it-Q8_0.gguf",
      "download_url": "https://huggingface.co/unsloth/gemma-3-1b-it-GGUF/resolve/main/gemma-3-1b-it-Q8_0.gguf",
      "size_mb": 1019,
      "template": "gemma3",
      "context_size": 32768,
      "layer_count": 26,
      "sampling": {
        "temperature": 1.0,
        "top_k": 64,
        "top_p": 0.95,
        "stop_tokens": ["<end_of_turn>"]
      },
      "description": "Fastest model. Runs on any hardware with ~1GB RAM. Good for quick summaries."
    },
    {
      "id": "gemma4:e2b",
      "display_name": "Gemma 4 E2B (Quality)",
      "gguf_file": "gemma-4-E2B-it-Q4_K_M.gguf",
      "download_url": "https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf",
      "size_mb": 3109,
      "template": "gemma3",
      "context_size": 32768,
      "layer_count": 30,
      "sampling": {
        "temperature": 1.0,
        "top_k": 64,
        "top_p": 0.95,
        "stop_tokens": ["<end_of_turn>"]
      },
      "description": "Latest Gemma model. ~3.1 GB on disk, needs ~4 GB RAM. Best quality."
    }
  ],
  "stt_whisper": [
    { "id": "tiny", "filename": "ggml-tiny.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin", "size_mb": 74, "accuracy": "Decent", "speed": "Very Fast", "description": "Fastest processing, good for real-time use" },
    { "id": "base", "filename": "ggml-base.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin", "size_mb": 142, "accuracy": "Good", "speed": "Fast", "description": "Good balance of speed and accuracy" },
    { "id": "small", "filename": "ggml-small.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin", "size_mb": 466, "accuracy": "Good", "speed": "Medium", "description": "Better accuracy, moderate speed" },
    { "id": "medium", "filename": "ggml-medium.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin", "size_mb": 1463, "accuracy": "High", "speed": "Slow", "description": "High accuracy for professional use" },
    { "id": "large-v3-turbo", "filename": "ggml-large-v3-turbo.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin", "size_mb": 1549, "accuracy": "High", "speed": "Medium", "description": "Best accuracy with improved speed" },
    { "id": "large-v3", "filename": "ggml-large-v3.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin", "size_mb": 2951, "accuracy": "High", "speed": "Slow", "description": "Most Accurate, latest large model" },
    { "id": "tiny-q5_1", "filename": "ggml-tiny-q5_1.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny-q5_1.bin", "size_mb": 31, "accuracy": "Decent", "speed": "Very Fast", "description": "Quantized tiny model, ~50% faster processing" },
    { "id": "base-q5_1", "filename": "ggml-base-q5_1.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin", "size_mb": 57, "accuracy": "Good", "speed": "Fast", "description": "Quantized base model, good speed/accuracy balance" },
    { "id": "small-q5_1", "filename": "ggml-small-q5_1.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin", "size_mb": 181, "accuracy": "Good", "speed": "Fast", "description": "Quantized small model, faster than f16 version" },
    { "id": "medium-q5_0", "filename": "ggml-medium-q5_0.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin", "size_mb": 514, "accuracy": "High", "speed": "Medium", "description": "Quantized medium model, professional quality" },
    { "id": "large-v3-turbo-q5_0", "filename": "ggml-large-v3-turbo-q5_0.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin", "size_mb": 547, "accuracy": "High", "speed": "Medium", "description": "Quantized large model, best balance" },
    { "id": "large-v3-q5_0", "filename": "ggml-large-v3-q5_0.bin", "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin", "size_mb": 1031, "accuracy": "High", "speed": "Slow", "description": "Quantized large model, high accuracy" }
  ],
  "stt_parakeet": [
    {
      "id": "parakeet-tdt-0.6b-v3-int8",
      "base_url": "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main",
      "files": ["encoder-model.int8.onnx", "decoder_joint-model.int8.onnx", "nemo128.onnx", "vocab.txt"],
      "size_mb": 670,
      "quantization": "int8",
      "version": "v3",
      "speed": "Ultra Fast (v3)",
      "description": "Real time on M4 Max, latest version with int8 quantization"
    },
    {
      "id": "parakeet-tdt-0.6b-v2-int8",
      "base_url": "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main",
      "files": ["encoder-model.int8.onnx", "decoder_joint-model.int8.onnx", "nemo128.onnx", "vocab.txt"],
      "size_mb": 661,
      "quantization": "int8",
      "version": "v2",
      "speed": "Fast (v2)",
      "description": "Previous version with int8 quantization, good balance of speed and accuracy"
    }
  ]
}
```

### - [ ] Step 1.2: Write the failing unit test for the catalog parser

Create `frontend/src-tauri/src/models_catalog.rs` with tests at the top (we'll add the implementation in the next step):

```rust
// frontend/src-tauri/src/models_catalog.rs

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager, Runtime};

// -- Types --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingParams {
    pub temperature: f32,
    pub top_k: i32,
    pub top_p: f32,
    pub stop_tokens: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmEntry {
    pub id: String,
    pub display_name: String,
    pub gguf_file: String,
    pub download_url: String,
    pub size_mb: u64,
    pub template: String,
    pub context_size: u32,
    pub layer_count: u32,
    pub sampling: SamplingParams,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperEntry {
    pub id: String,
    pub filename: String,
    pub download_url: String,
    pub size_mb: u32,
    pub accuracy: String,
    pub speed: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParakeetEntry {
    pub id: String,
    pub base_url: String,
    pub files: Vec<String>,
    pub size_mb: u32,
    pub quantization: String,
    pub version: String,
    pub speed: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub schema_version: u32,
    pub llm: Vec<LlmEntry>,
    pub stt_whisper: Vec<WhisperEntry>,
    pub stt_parakeet: Vec<ParakeetEntry>,
}

// -- Global cached catalog ---------------------------------------------

static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// Called once at app startup from the Tauri setup hook.
pub fn init<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let resource_path = app
        .path()
        .resolve(
            "resources/models_catalog.json",
            tauri::path::BaseDirectory::Resource,
        )
        .context("failed to resolve models_catalog.json resource path")?;

    let raw = std::fs::read_to_string(&resource_path)
        .with_context(|| format!("failed to read catalog at {:?}", resource_path))?;

    let catalog = parse(&raw)?;
    CATALOG
        .set(catalog)
        .map_err(|_| anyhow!("models_catalog::init called more than once"))?;
    Ok(())
}

/// Access the loaded catalog. Panics if `init` was not called — this is a
/// programming error, not a user-facing situation.
pub fn get() -> &'static Catalog {
    CATALOG
        .get()
        .expect("models_catalog::get() called before init() — fix the startup order")
}

/// Parse and validate a catalog from a JSON string.
pub fn parse(raw: &str) -> Result<Catalog> {
    let catalog: Catalog =
        serde_json::from_str(raw).context("failed to parse models_catalog.json")?;
    validate(&catalog)?;
    Ok(catalog)
}

fn validate(catalog: &Catalog) -> Result<()> {
    if catalog.schema_version != 1 {
        return Err(anyhow!(
            "unsupported catalog schema_version: {} (expected 1)",
            catalog.schema_version
        ));
    }

    check_unique(catalog.llm.iter().map(|e| e.id.as_str()), "llm")?;
    check_unique(catalog.stt_whisper.iter().map(|e| e.id.as_str()), "stt_whisper")?;
    check_unique(catalog.stt_parakeet.iter().map(|e| e.id.as_str()), "stt_parakeet")?;

    Ok(())
}

fn check_unique<'a, I: IntoIterator<Item = &'a str>>(ids: I, section: &str) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        if !seen.insert(id.to_string()) {
            return Err(anyhow!("duplicate id '{}' in section '{}'", id, section));
        }
    }
    Ok(())
}

// -- Unit tests ---------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_VALID: &str = r#"{
        "schema_version": 1,
        "llm": [],
        "stt_whisper": [],
        "stt_parakeet": []
    }"#;

    #[test]
    fn parses_minimal_valid_catalog() {
        let c = parse(MINIMAL_VALID).expect("should parse");
        assert_eq!(c.schema_version, 1);
        assert!(c.llm.is_empty());
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let raw = r#"{"schema_version": 999, "llm": [], "stt_whisper": [], "stt_parakeet": []}"#;
        let err = parse(raw).expect_err("should reject");
        assert!(err.to_string().contains("unsupported catalog schema_version"));
    }

    #[test]
    fn rejects_duplicate_llm_ids() {
        let raw = r#"{
            "schema_version": 1,
            "llm": [
                {"id":"a","display_name":"A","gguf_file":"a.gguf","download_url":"x","size_mb":1,"template":"t","context_size":1,"layer_count":1,"sampling":{"temperature":1.0,"top_k":1,"top_p":1.0,"stop_tokens":[]},"description":""},
                {"id":"a","display_name":"A2","gguf_file":"a2.gguf","download_url":"x","size_mb":1,"template":"t","context_size":1,"layer_count":1,"sampling":{"temperature":1.0,"top_k":1,"top_p":1.0,"stop_tokens":[]},"description":""}
            ],
            "stt_whisper": [],
            "stt_parakeet": []
        }"#;
        let err = parse(raw).expect_err("should reject");
        assert!(err.to_string().contains("duplicate id 'a'"));
    }

    #[test]
    fn rejects_missing_required_fields() {
        let raw = r#"{"schema_version": 1, "llm": [{"id":"oops"}], "stt_whisper": [], "stt_parakeet": []}"#;
        parse(raw).expect_err("should reject missing fields");
    }
}
```

### - [ ] Step 1.3: Declare the module and call init in the Tauri setup hook

Edit `frontend/src-tauri/src/lib.rs`.

Add module declaration in the `pub mod` block (near line 40, alphabetically ordered next to `config`):

```rust
pub mod config;
pub mod console_utils;
pub mod database;
pub mod models_catalog;  // <-- ADD THIS LINE
pub mod notifications;
```

Then find the `.setup(|app| { ... })` call in `lib.rs` (search for `.setup`). Add the catalog init at the very top of the setup closure, before any other work:

```rust
.setup(|app| {
    // Initialize the model catalog first — required by LLM/Whisper/Parakeet engines below.
    crate::models_catalog::init(app.handle())
        .map_err(|e| format!("failed to initialize model catalog: {}", e))?;

    // ... existing setup code ...
    Ok(())
})
```

If there's no existing `.setup()`, add one before `.invoke_handler(...)`.

### - [ ] Step 1.4: Register the JSON as a bundled resource

Edit `frontend/src-tauri/tauri.conf.json` line 98-100:

```json
"resources": [
    "templates/*.json",
    "resources/models_catalog.json"
],
```

### - [ ] Step 1.5: Run unit tests and build

```bash
cd frontend/src-tauri
cargo check
cargo test --lib models_catalog
```

Expected:
- `cargo check` → clean compile
- `cargo test` → 4 tests pass: `parses_minimal_valid_catalog`, `rejects_unknown_schema_version`, `rejects_duplicate_llm_ids`, `rejects_missing_required_fields`

If `cargo test` fails with CRT linker errors (esaxx `/MT` vs whisper `/MD`, per memory `build_cargo_test_crt_mismatch.md`), this is a pre-existing issue unrelated to our changes. Document it and proceed: the unit tests in this module are pure parsing logic and can be validated by running `cargo check` + code review. Note the issue and move on.

### - [ ] Step 1.6: Commit

```bash
git add frontend/src-tauri/src/models_catalog.rs \
        frontend/src-tauri/resources/models_catalog.json \
        frontend/src-tauri/tauri.conf.json \
        frontend/src-tauri/src/lib.rs
git commit -m "feat(catalog): add bundled JSON model catalog + parser module

New single source of truth for built-in LLM/Whisper/Parakeet model
metadata. Loaded once at Tauri startup and cached in OnceLock.
Consumers (LLM/Whisper/Parakeet engines) are migrated in follow-up
commits.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Migrate the LLM catalog (Gemma) to read from the central catalog

**Files:**
- Modify: `frontend/src-tauri/src/summary/summary_engine/models.rs` (replace `get_available_models` body, lines 66-104)

### - [ ] Step 2.1: Replace `get_available_models` to read from the catalog

Edit `frontend/src-tauri/src/summary/summary_engine/models.rs`. Replace the entire function (lines 66-104):

```rust
/// Get all available built-in AI models, sourced from the central catalog.
pub fn get_available_models() -> Vec<ModelDef> {
    crate::models_catalog::get()
        .llm
        .iter()
        .map(|e| ModelDef {
            name: e.id.clone(),
            display_name: e.display_name.clone(),
            gguf_file: e.gguf_file.clone(),
            template: e.template.clone(),
            download_url: e.download_url.clone(),
            size_mb: e.size_mb,
            context_size: e.context_size,
            layer_count: e.layer_count,
            sampling: SamplingParams {
                temperature: e.sampling.temperature,
                top_k: e.sampling.top_k,
                top_p: e.sampling.top_p,
                stop_tokens: e.sampling.stop_tokens.clone(),
            },
            description: e.description.clone(),
        })
        .collect()
}
```

The function signature is identical, so `get_model_by_name()`, `get_default_model()`, and all consumers in `model_manager.rs`, `client.rs`, `service.rs`, `mod.rs` (see spec §Change footprint) keep working unchanged.

### - [ ] Step 2.2: Verify compilation

```bash
cd frontend/src-tauri
cargo check
```

Expected: clean compile. No changes to ModelDef struct, no consumers affected.

### - [ ] Step 2.3: Commit

```bash
git add frontend/src-tauri/src/summary/summary_engine/models.rs
git commit -m "refactor(llm): read Gemma model list from central catalog

Removes hardcoded gemma3:1b + gemma3:4b entries and their old CDN URLs.
The new entries (gemma3:1b pointing to HuggingFace, gemma4:e2b new, no
more gemma3:4b) are now served from models_catalog.json.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Migrate the Whisper catalog

**Files:**
- Modify: `frontend/src-tauri/src/whisper_engine/whisper_engine.rs` (two sites: lines 170-173 and 921-942)
- Modify: `frontend/src-tauri/src/whisper_engine/commands.rs:5, 87` (remove `WHISPER_MODEL_CATALOG` import + usage)
- Modify: `frontend/src-tauri/src/config.rs:14-36` (delete `WHISPER_MODEL_CATALOG`)

### - [ ] Step 3.1: Update `whisper_engine.rs::discover_models()` to use the catalog

In `frontend/src-tauri/src/whisper_engine/whisper_engine.rs`, remove the import at line 13:

```rust
// DELETE THIS LINE:
use crate::config::WHISPER_MODEL_CATALOG;
```

Then edit the `discover_models` function (around line 170-173) to iterate the catalog:

```rust
// BEFORE (line 170-175):
// let models_dir = &self.models_dir;
// let mut models = Vec::new();
// // Use centralized model catalog from config.rs
// let model_configs = WHISPER_MODEL_CATALOG;
//
// for &(name, filename, size_mb, accuracy, speed, description) in model_configs {

// AFTER:
let models_dir = &self.models_dir;
let mut models = Vec::new();
let catalog = crate::models_catalog::get();

for entry in &catalog.stt_whisper {
    let name = entry.id.as_str();
    let filename = entry.filename.as_str();
    let size_mb = entry.size_mb;
    let accuracy = entry.accuracy.as_str();
    let speed = entry.speed.as_str();
    let description = entry.description.as_str();
```

The rest of the loop body (file existence/validation logic, `ModelInfo` construction) is **unchanged** — it uses the same local variable names.

### - [ ] Step 3.2: Replace the URL-construction match block

In `frontend/src-tauri/src/whisper_engine/whisper_engine.rs`, lines 921-942, replace the whole `match model_name { ... }` with a catalog lookup:

```rust
// BEFORE (line 921-942):
// // Official ggerganov/whisper.cpp model URLs from Hugging Face
// let model_url = match model_name {
//     "tiny" => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
//     ... 11 more match arms ...
//     _ => return Err(anyhow!("Unsupported model: {}", model_name))
// };

// AFTER:
let model_url = crate::models_catalog::get()
    .stt_whisper
    .iter()
    .find(|e| e.id == model_name)
    .map(|e| e.download_url.as_str())
    .ok_or_else(|| anyhow!("Unsupported model: {}", model_name))?;
```

Keep the `log::info!("Model URL for {}: {}", ...)` line below unchanged.

### - [ ] Step 3.3: Update `whisper_engine/commands.rs`

Open `frontend/src-tauri/src/whisper_engine/commands.rs`. Remove the import at line 5:

```rust
// DELETE:
use crate::config::WHISPER_MODEL_CATALOG;
```

Then replace the block at lines 86-118 (inside `discover_models_standalone`). The current code:

```rust
// Use centralized model catalog from config.rs
let model_configs = WHISPER_MODEL_CATALOG;

let mut models = Vec::new();

for &(name, filename, size_mb, accuracy, speed, description) in model_configs {
    let model_path = whisper_dir.join(filename);
    let status = if model_path.exists() {
        match std::fs::metadata(&model_path) {
            Ok(metadata) => {
                let file_size_mb = metadata.len() / (1024 * 1024);
                if file_size_mb >= 1 {
                    ModelStatus::Available
                } else {
                    ModelStatus::Missing
                }
            }
            Err(_) => ModelStatus::Missing,
        }
    } else {
        ModelStatus::Missing
    };

    models.push(ModelInfo {
        name: name.to_string(),
        path: model_path,
        size_mb,
        status,
        accuracy: accuracy.to_string(),
        speed: speed.to_string(),
        description: description.to_string(),
    });
}
```

becomes:

```rust
let catalog = crate::models_catalog::get();
let mut models = Vec::new();

for entry in &catalog.stt_whisper {
    let model_path = whisper_dir.join(&entry.filename);
    let status = if model_path.exists() {
        match std::fs::metadata(&model_path) {
            Ok(metadata) => {
                let file_size_mb = metadata.len() / (1024 * 1024);
                if file_size_mb >= 1 {
                    ModelStatus::Available
                } else {
                    ModelStatus::Missing
                }
            }
            Err(_) => ModelStatus::Missing,
        }
    } else {
        ModelStatus::Missing
    };

    models.push(ModelInfo {
        name: entry.id.clone(),
        path: model_path,
        size_mb: entry.size_mb,
        status,
        accuracy: entry.accuracy.clone(),
        speed: entry.speed.clone(),
        description: entry.description.clone(),
    });
}
```

### - [ ] Step 3.4: Remove `WHISPER_MODEL_CATALOG` from `config.rs`

Edit `frontend/src-tauri/src/config.rs`. Delete lines 14-36 (the comment block explaining the format and the constant). Keep lines 1-12 (`DEFAULT_WHISPER_MODEL`, `DEFAULT_PARAKEET_MODEL`) — they're policy defaults, not catalog data.

Final expected content of `config.rs`:

```rust
/// Application configuration constants
///
/// Centralized definitions for default models and settings.
/// Used across database initialization, import, and retranscription.

/// Default Whisper model for transcription when no preference is configured.
/// This is the recommended balance of accuracy and speed.
pub const DEFAULT_WHISPER_MODEL: &str = "large-v3-turbo";

/// Default Parakeet model for transcription when no preference is configured.
/// This is the quantized version optimized for speed.
pub const DEFAULT_PARAKEET_MODEL: &str = "parakeet-tdt-0.6b-v3-int8";
```

### - [ ] Step 3.5: Verify compilation

```bash
cd frontend/src-tauri
cargo check
```

Expected: clean compile. If compilation fails with "cannot find `WHISPER_MODEL_CATALOG` in `config`", grep for any other usage: `grep -r "WHISPER_MODEL_CATALOG" frontend/src-tauri/src/` should return zero matches after this task.

### - [ ] Step 3.6: Commit

```bash
git add frontend/src-tauri/src/whisper_engine/whisper_engine.rs \
        frontend/src-tauri/src/whisper_engine/commands.rs \
        frontend/src-tauri/src/config.rs
git commit -m "refactor(whisper): read model list and URLs from central catalog

Removes the WHISPER_MODEL_CATALOG constant and the hardcoded match for
URL construction. All 12 Whisper entries now live in
models_catalog.json.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Migrate the Parakeet catalog

**Files:**
- Modify: `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs` (two sites: `discover_models` at 167-258, URL logic at 593-615)

### - [ ] Step 4.1: Replace `discover_models` hardcoded config with catalog iteration

In `frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs`, around lines 171-177, replace the `model_configs` array:

```rust
// BEFORE (line 171-177):
// // Parakeet model configurations
// // Model name format: parakeet-tdt-0.6b-v{version}-{quantization}
// // Sizes match actual download sizes (encoder + decoder + preprocessor + vocab)
// let model_configs = [
//     ("parakeet-tdt-0.6b-v3-int8", 670, QuantizationType::Int8, "Ultra Fast (v3)", "Real time on M4 Max, latest version with int8 quantization"),
//     ("parakeet-tdt-0.6b-v2-int8", 661, QuantizationType::Int8, "Fast (v2)", "Previous version with int8 quantization, good balance of speed and accuracy"),
// ];

// AFTER:
let catalog_entries = &crate::models_catalog::get().stt_parakeet;
```

Then adapt the `for` loop (line 182) to iterate catalog entries instead of the tuple array:

```rust
// BEFORE:
// for (name, size_mb, quantization, speed, description) in model_configs {

// AFTER:
for entry in catalog_entries {
    let name: &str = &entry.id;
    let size_mb: u32 = entry.size_mb;
    let quantization = match entry.quantization.as_str() {
        "int8" => QuantizationType::Int8,
        "fp32" => QuantizationType::FP32,
        other => {
            log::warn!("unknown Parakeet quantization '{}' for {}; skipping", other, name);
            continue;
        }
    };
    let speed: &str = &entry.speed;
    let description: &str = &entry.description;
    let required_files: Vec<&str> = entry.files.iter().map(|s| s.as_str()).collect();
```

Now replace the `required_files = match quantization { ... }` block (lines 193-206) — it's no longer needed because we read `files` from the catalog entry directly. Delete the entire `let required_files = match quantization { ... };` block.

The rest of the loop body (existence check, validation, `ModelInfo` construction) is unchanged. Make sure that where `size_mb as u32` or `size_mb as u64` was used, it still compiles (we kept `size_mb: u32`).

### - [ ] Step 4.2: Replace the Parakeet download URL logic

In the same file, around lines 593-615, replace the `base_url` and `files_to_download` logic:

```rust
// BEFORE (line 593-615):
// // HuggingFace base URL for Parakeet models (version-specific)
// let base_url = if model_name.contains("-v2-") {
//     "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main"
// } else {
//     // Default to v3 for v3 models
//     "https://meetily.towardsgeneralintelligence.com/models/parakeet-tdt-0.6b-v3-onnx"
// };
//
// // Determine which files to download based on quantization
// let files_to_download = match model_info.quantization {
//     QuantizationType::Int8 => vec![...],
//     QuantizationType::FP32 => vec![...],
// };

// AFTER:
let catalog_entry = crate::models_catalog::get()
    .stt_parakeet
    .iter()
    .find(|e| e.id == model_name)
    .ok_or_else(|| anyhow!("Parakeet model {} not in catalog", model_name))?;
let base_url: &str = &catalog_entry.base_url;
let files_to_download: Vec<&str> = catalog_entry.files.iter().map(|s| s.as_str()).collect();
```

Note: verify no downstream code expects an owned `Vec<String>`. If it does, clone: `catalog_entry.files.clone()` and adjust type to `Vec<String>`.

### - [ ] Step 4.3: Verify compilation

```bash
cd frontend/src-tauri
cargo check
```

Also verify no CDN references remain:

```bash
grep -r "meetily.towardsgeneralintelligence.com" frontend/src-tauri/
```

Expected: zero matches.

### - [ ] Step 4.4: Commit

```bash
git add frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs
git commit -m "refactor(parakeet): read model list and URLs from central catalog

Removes the name-based URL heuristic and the hardcoded required-files
match. v3 URL swaps from the non-owned CDN to istupakov/parakeet-tdt-0.6b-v3-onnx
on HuggingFace. Model IDs and filenames unchanged, so pre-existing
downloads remain valid.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Integration test for the bundled catalog

**Files:**
- Create: `frontend/src-tauri/tests/catalog_validation.rs`

### - [ ] Step 5.1: Write the integration test

Create `frontend/src-tauri/tests/catalog_validation.rs`:

```rust
// Integration test: loads the actual bundled models_catalog.json
// and verifies its structure and invariants.

use std::path::PathBuf;

fn load_bundled_catalog() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("models_catalog.json");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read catalog at {:?}: {}", path, e))
}

#[test]
fn catalog_parses_successfully() {
    let raw = load_bundled_catalog();
    let catalog =
        app_lib::models_catalog::parse(&raw).expect("bundled catalog must parse and validate");
    assert_eq!(catalog.schema_version, 1);
}

#[test]
fn catalog_has_required_entries() {
    let raw = load_bundled_catalog();
    let catalog = app_lib::models_catalog::parse(&raw).unwrap();

    assert!(
        catalog.llm.iter().any(|e| e.id == "gemma3:1b"),
        "gemma3:1b missing from LLM section"
    );
    assert!(
        catalog.llm.iter().any(|e| e.id == "gemma4:e2b"),
        "gemma4:e2b missing from LLM section"
    );
    assert!(
        !catalog.llm.iter().any(|e| e.id == "gemma3:4b"),
        "gemma3:4b should have been removed from LLM section"
    );
    assert!(
        catalog.stt_whisper.iter().any(|e| e.id == "base"),
        "whisper base model missing"
    );
    assert!(
        catalog.stt_parakeet.iter().any(|e| e.id == "parakeet-tdt-0.6b-v3-int8"),
        "Parakeet v3 missing"
    );
}

#[test]
fn all_urls_are_huggingface() {
    let raw = load_bundled_catalog();
    let catalog = app_lib::models_catalog::parse(&raw).unwrap();

    for entry in &catalog.llm {
        assert!(
            entry.download_url.starts_with("https://huggingface.co/"),
            "LLM entry {} does not point to HuggingFace: {}",
            entry.id,
            entry.download_url
        );
    }
    for entry in &catalog.stt_whisper {
        assert!(
            entry.download_url.starts_with("https://huggingface.co/"),
            "Whisper entry {} does not point to HuggingFace: {}",
            entry.id,
            entry.download_url
        );
    }
    for entry in &catalog.stt_parakeet {
        assert!(
            entry.base_url.starts_with("https://huggingface.co/"),
            "Parakeet entry {} does not point to HuggingFace: {}",
            entry.id,
            entry.base_url
        );
    }
}

#[test]
fn no_references_to_old_cdn() {
    let raw = load_bundled_catalog();
    assert!(
        !raw.contains("meetily.towardsgeneralintelligence.com"),
        "catalog still references the old CDN — migration incomplete"
    );
}
```

Note: the test references `app_lib::models_catalog::parse`. The Meetily Tauri crate name is `app_lib` (check `frontend/src-tauri/Cargo.toml` `[lib] name = "app_lib"` or similar — if the name differs, adjust the use path accordingly).

### - [ ] Step 5.2: Attempt to run the integration test

```bash
cd frontend/src-tauri
cargo test --test catalog_validation
```

Expected outcomes:

- **If `cargo test` works** → all 4 tests pass.
- **If `cargo test` fails with CRT linker errors** (per memory `build_cargo_test_crt_mismatch.md`, the pre-existing esaxx `/MT` vs whisper `/MD` issue) → the tests cannot run in the default build. This is out of scope for this chantier. Options:
    - (a) Add a note in the test file header: `//! Run via: cargo test --test catalog_validation --no-default-features` (if that works with the project's feature setup).
    - (b) Skip the integration test for this chantier and rely on the unit tests from Task 1 + the smoke test in Task 6. Document the skip with a comment in the test file.
    - Pick (a) if it works, (b) otherwise. Record the decision in the commit message.

### - [ ] Step 5.3: Commit

```bash
git add frontend/src-tauri/tests/catalog_validation.rs
git commit -m "test(catalog): integration test for bundled models_catalog.json

Validates schema, required entries (gemma3:1b, gemma4:e2b, Parakeet v3,
Whisper base), URL invariants (all HuggingFace), and absence of old CDN
references.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

If the test can't be run due to the pre-existing CRT issue, add this to the commit message body:
> Known limitation: cargo test currently fails on this project due to the
> pre-existing esaxx /MT vs whisper /MD CRT mismatch. The test is
> committed for documentation and for when the build issue is fixed
> separately.

---

## Task 6: Smoke test, validate Gemma 4 E2B assumptions, and finalize

**Files:**
- Potentially modify: `frontend/src-tauri/resources/models_catalog.json` (adjust `layer_count` or `template` if the smoke test reveals different values)

### - [ ] Step 6.1: Build the app and run a full smoke test

From the project root:

```bash
cd frontend
pnpm run tauri:dev
```

Verify manually in the running app:

1. Go to the built-in model management UI. Verify:
   - LLM section shows **`gemma3:1b`** and **`gemma4:e2b`** (and **NOT** `gemma3:4b`).
   - Whisper section shows all 12 entries.
   - Parakeet section shows v2 + v3.
2. If a previous `gemma3:1b` was already downloaded, confirm it's detected as "installed" (no re-download prompt).
3. Download `gemma4:e2b`. Confirm:
   - Download completes successfully from `unsloth/gemma-4-E2B-it-GGUF`.
   - File lands at `%APPDATA%\Meetily\models\llm\gemma-4-E2B-it-Q4_K_M.gguf`.
4. Download Parakeet v3. Confirm:
   - Download pulls from `istupakov/parakeet-tdt-0.6b-v3-onnx` (check server response logs).
   - All 4 files (encoder, decoder_joint, nemo128, vocab) arrive.

### - [ ] Step 6.2: Validate Gemma 4 E2B `template` assumption

Generate a summary using `gemma4:e2b` as the selected model. On a short test meeting transcript (or any placeholder text):

- **If the summary generates coherent output** → the `"template": "gemma3"` entry is correct. No change needed.
- **If the output is garbled** (tokens like `<start_of_turn>` leaking, or nonsense text) → the chat template is different. Investigate the Unsloth/Google model card for Gemma 4 E2B to find the correct chat template. If a new template is required:
   1. Add a new template variant in the code that renders Gemma prompts (search the codebase for the existing `gemma3` template: `grep -r 'gemma3' frontend/src-tauri/src/summary/`).
   2. Update the catalog entry `"template"` field to the new name.

### - [ ] Step 6.3: Validate Gemma 4 E2B `layer_count` assumption

The downloaded GGUF at `%APPDATA%\Meetily\models\llm\gemma-4-E2B-it-Q4_K_M.gguf` contains the true layer count in its metadata. The simplest check:

- Look at the app's Rust logs when loading the model (`RUST_LOG=debug pnpm run tauri:dev`) — llama.cpp will print the real block count during model load.
- Compare with `layer_count: 30` in the catalog.
- If different, update `frontend/src-tauri/resources/models_catalog.json` with the correct value. This only affects GPU-offloading efficiency, not correctness.

### - [ ] Step 6.4: Final grep to confirm no CDN references remain

```bash
grep -r "meetily.towardsgeneralintelligence.com" frontend/
```

Expected: zero matches.

### - [ ] Step 6.5: Final cargo check + clippy

```bash
cd frontend/src-tauri
cargo check
cargo clippy -- -D warnings
```

Expected: both clean.

### - [ ] Step 6.6: Commit any smoke-test adjustments

Only if Steps 6.2 or 6.3 produced changes:

```bash
git add frontend/src-tauri/resources/models_catalog.json
git commit -m "fix(catalog): correct gemma4:e2b metadata after smoke test

<describe exactly what changed: template, layer_count, or both>

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

If nothing changed, skip this commit. Done.

---

## Definition of Done (from spec)

- [ ] `models_catalog.json` bundled as Tauri resource
- [ ] Three engines (LLM/Whisper/Parakeet) read from the catalog
- [ ] `grep -r "meetily.towardsgeneralintelligence.com" frontend/` returns zero matches
- [ ] `gemma4:e2b` downloadable and usable for summary generation
- [ ] Parakeet v3 downloadable from `istupakov/parakeet-tdt-0.6b-v3-onnx`
- [ ] `gemma3:4b` no longer in the catalog
- [ ] Unit tests for `models_catalog` parser pass (or documented caveat applies)
- [ ] Integration test `catalog_validation.rs` passes (or documented caveat applies)
- [ ] Windows smoke test passes (items in Task 6 Step 6.1)
- [ ] UI unchanged (no visible regression — no Tauri command signatures changed)

---

## Follow-ups (from spec, NOT part of this chantier)

1. Enable `DirectMLExecutionProvider` or `CUDAExecutionProvider` for Parakeet — separate ticket.
2. Add `nvidia/canary-1b-v2` via istupakov ONNX — separate ticket.
3. Swap HuggingFace URLs in `models_catalog.json` to self-hosted mirror when ready (memory note `todo_self_host_binaries.md`).
