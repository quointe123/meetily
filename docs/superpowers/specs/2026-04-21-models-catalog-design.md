# Model Catalog Centralization — Design

**Date:** 2026-04-21
**Status:** Approved
**Author:** quointe123 (via Claude)

## Problem

Meetily's fork currently fetches built-in AI models from three separate hardcoded catalogs scattered across the Rust codebase:

- **LLM (Gemma 3)** — [`summary/summary_engine/models.rs:66-104`](../../../frontend/src-tauri/src/summary/summary_engine/models.rs) — hosted on `meetily.towardsgeneralintelligence.com` (a CDN owned by the upstream Zackriya project, **not** this fork's maintainer)
- **Whisper STT** — [`config.rs:18-36`](../../../frontend/src-tauri/src/config.rs) — hosted on HuggingFace (`ggerganov/whisper.cpp`), fine
- **Parakeet STT** — [`parakeet_engine/parakeet_engine.rs:174-177`](../../../frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs) — v2 on HuggingFace (fine), v3 on the non-owned CDN

Two concrete issues:

1. **Dependency on an external CDN we don't control.** If `meetily.towardsgeneralintelligence.com` goes down or changes policy, `gemma3:1b`, `gemma3:4b`, and Parakeet v3 become undownloadable for our users.
2. **No path to add new models** without code changes spread across three files.

## Goals

1. Remove all dependencies on `meetily.towardsgeneralintelligence.com`.
2. Centralize the three catalogs into a single declarative source of truth.
3. Add `gemma4:e2b` (new Gemma 4 E2B model released by Google, available via Unsloth's GGUF repo).
4. Swap `parakeet-tdt-0.6b-v3-int8` to the HuggingFace source (`istupakov/parakeet-tdt-0.6b-v3-onnx`).
5. Remove `gemma3:4b` from the catalog (replaced by `gemma4:e2b`).
6. Preserve existing user-installed models (no forced re-download).

## Non-goals

- Remote/dynamic catalog fetching (considered and rejected — see "Rejected approaches").
- UI changes — the Tauri commands and emitted events keep their existing signatures.
- GPU acceleration for Parakeet (tracked as follow-up, not this chantier).
- Adding Canary STT (deferred — see follow-ups).
- Self-hosted mirror of model binaries (tracked separately in `todo_self_host_binaries.md`).

## Architecture

### Single JSON catalog, bundled as Tauri resource

**Path:** `frontend/src-tauri/resources/models_catalog.json`

Bundled via `tauri.conf.json` under `bundle.resources`, accessible at runtime via `app.path().resource_dir()`.

### Schema (version 1)

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
      "description": "Latest Gemma model. Multimodal capable (text-only used here). ~3.1 GB on disk, needs ~4 GB RAM."
    }
  ],
  "stt_whisper": [
    {
      "id": "base",
      "filename": "ggml-base.bin",
      "download_url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
      "size_mb": 142,
      "accuracy": "Good",
      "speed": "Fast",
      "description": "Good balance of speed and accuracy"
    }
    // ... 11 more entries, migrated 1:1 from config.rs WHISPER_MODEL_CATALOG
  ],
  "stt_parakeet": [
    {
      "id": "parakeet-tdt-0.6b-v3-int8",
      "base_url": "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main",
      "files": [
        "encoder-model.int8.onnx",
        "decoder_joint-model.int8.onnx",
        "nemo128.onnx",
        "vocab.txt"
      ],
      "size_mb": 670,
      "quantization": "int8",
      "version": "v3",
      "speed": "Ultra Fast (v3)",
      "description": "Real time on M4 Max, latest version with int8 quantization"
    },
    {
      "id": "parakeet-tdt-0.6b-v2-int8",
      "base_url": "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main",
      "files": [
        "encoder-model.int8.onnx",
        "decoder_joint-model.int8.onnx",
        "nemo128.onnx",
        "vocab.txt"
      ],
      "size_mb": 661,
      "quantization": "int8",
      "version": "v2",
      "speed": "Fast (v2)",
      "description": "Previous version with int8 quantization, good balance of speed and accuracy"
    }
  ]
}
```

### Design choices

| Choice | Rationale |
|---|---|
| `schema_version` at the top | Explicit signal for future breaking changes (e.g., when migrating URLs to a self-hosted mirror we can bump to v2 and handle older bundled catalogs gracefully if needed) |
| `download_url` complete per LLM/Whisper entry | More readable, easier to override individually, no URL concatenation logic in Rust |
| `base_url` + `files[]` for Parakeet | A Parakeet model = multiple ONNX files; listing them declaratively avoids hardcoded file arrays in Rust (currently at `parakeet_engine.rs:193-206`) |
| Fatal error on parse failure (no silent fallback) | Single source of truth — if catalog is broken, so is the app. Falling back to a hardcoded list would create invisible drift. |
| Bundled (not remote) | No runtime network dependency for model discovery; deterministic builds; easy git review |

### Rejected approaches

**Remote JSON fetched at startup (e.g., GitHub Raw of this fork):**
- Would allow updating the catalog without a rebuild.
- Adds a runtime network dependency and a new failure mode for every app launch.
- The need this solves ("update catalog without rebuild") is low-priority for a temporary solution; over-engineering.

**Minimal patch (keep three catalogs, just swap URLs):**
- Smallest diff, but leaves the three scattered catalogs and doesn't improve the path to the eventual self-hosted migration.
- Doesn't satisfy the "simpler source" goal stated by the user.

## Implementation

### New module: `models_catalog.rs`

**Path:** `frontend/src-tauri/src/models_catalog.rs`

**Responsibility:** parse the JSON once at first access, cache with `OnceLock`, expose typed views.

```rust
pub struct Catalog {
    pub schema_version: u32,
    pub llm: Vec<LlmEntry>,
    pub stt_whisper: Vec<WhisperEntry>,
    pub stt_parakeet: Vec<ParakeetEntry>,
}

pub fn load(app: &AppHandle) -> Result<&'static Catalog>; // OnceLock-cached
```

Error behavior: if the JSON is absent or fails to parse, return a clear error from `load()` which causes startup to fail with a user-visible message. **No silent fallback to hardcoded data.**

### Changes per existing file

#### 1. [`summary/summary_engine/models.rs`](../../../frontend/src-tauri/src/summary/summary_engine/models.rs)
- Remove the hardcoded `get_available_models()` array (lines 66-104).
- Replace with `get_available_models(app: &AppHandle) -> Vec<ModelDef>` that maps from `catalog.llm`.
- `ModelDef` struct **unchanged** → downstream consumers untouched.
- `gemma3:4b` disappears from the catalog; `gemma4:e2b` appears.

#### 2. [`whisper_engine/whisper_engine.rs`](../../../frontend/src-tauri/src/whisper_engine/whisper_engine.rs)
- Replace the `match model_name` URL-construction block (lines 922-941) with a lookup in `catalog.stt_whisper`.
- `discover_models()` / `discover_models_standalone()` read from the catalog instead of `WHISPER_MODEL_CATALOG`.

#### 3. [`config.rs`](../../../frontend/src-tauri/src/config.rs)
- Remove the `WHISPER_MODEL_CATALOG` constant (lines 14-36).
- Keep `DEFAULT_WHISPER_MODEL` and `DEFAULT_PARAKEET_MODEL` constants (they're policy defaults, not catalog data).

#### 4. [`parakeet_engine/parakeet_engine.rs`](../../../frontend/src-tauri/src/parakeet_engine/parakeet_engine.rs)
- Replace the hardcoded `model_configs` array in `discover_models()` (lines 174-177) with iteration over `catalog.stt_parakeet`.
- Replace the URL-construction logic (lines 594-599) to read `base_url` + `files[]` from the catalog entry.
- Required-files list is now read from the catalog entry (lines 193-206 become data-driven).

#### 5. [`tauri.conf.json`](../../../frontend/src-tauri/tauri.conf.json)
- Add `"resources/models_catalog.json"` to the `bundle.resources` array.

#### 6. [`lib.rs`](../../../frontend/src-tauri/src/lib.rs)
- Add `mod models_catalog;` declaration.

### Change footprint

| File | Nature | Approx. lines |
|---|---|---|
| `models_catalog.rs` (new) | New module | +120 |
| `resources/models_catalog.json` (new) | Data | +150 |
| `summary/summary_engine/models.rs` | Refactor | ~40 replaced |
| `whisper_engine/whisper_engine.rs` | Refactor | ~20 replaced |
| `config.rs` | Deletion | ~20 removed |
| `parakeet_engine/parakeet_engine.rs` | Refactor | ~30 replaced |
| `tauri.conf.json` | Config | +1 line |
| `lib.rs` | Declaration | +1 line |

**No changes** in: UI (`BuiltInModelManager.tsx`, others), Tauri command signatures, download logic (only URLs change), `Cargo.toml` dependencies.

## Gemma 4 E2B open items

Two fields in the `gemma4:e2b` catalog entry are informed estimates and must be validated during implementation:

1. **`template: "gemma3"`** — Gemma 4 almost certainly reuses the Gemma 3 chat template (`<start_of_turn>user\n...<end_of_turn>\n<start_of_turn>model\n...`) since the tokenizer and template changes between Gemma 3 → 4 are not announced as breaking. **Verification**: smoke-test a summary generation with `gemma4:e2b` during implementation. If the template format differs, add a new `gemma4` template in [`prompts/templates.rs`](../../../frontend/src-tauri/src/summary/summary_engine/prompts/templates.rs) (or equivalent) and update the catalog entry.

2. **`layer_count: 30`** — Extrapolated from Gemma 3 architecture (1B=26, 4B=34). Only used by llama.cpp for GPU offloading calculation; a wrong value means suboptimal GPU utilization but not a crash. **Verification**: read the actual layer count from the downloaded GGUF file's metadata (either via `llama.cpp` CLI or by inspecting the sidecar binary output) and update the catalog accordingly.

These are implementation-time validations, not blockers for the design.

## User migration (pre-existing downloads)

Models on disk are stored by `id` (stable key), not by URL. Therefore:

| Pre-existing model | Behavior after update |
|---|---|
| `gemma3:1b` (from old CDN, Q8_0, same `gguf_file` name) | ✅ Preserved. Re-used as-is, no re-download. |
| `gemma3:4b` (from old CDN) | ⚠️ Orphaned. File stays on disk but doesn't appear in UI. User can delete manually. No auto-cleanup (too risky). |
| `parakeet-tdt-0.6b-v3-int8` (from old CDN) | ✅ Preserved. Same `id`, same filenames. |
| `gemma4:e2b` | New — downloadable via UI when selected. |

**No migration code required** because IDs and filenames are kept stable.

## Testing

### Unit tests (in `models_catalog.rs`)

- Parse a valid fixture → OK, correct entries.
- Parse with unknown `schema_version` → explicit error.
- Duplicate IDs within a section → error.
- Missing required fields → serde deserialization error.

### Integration test (`frontend/src-tauri/tests/catalog_validation.rs`)

- Load the real bundled `models_catalog.json`.
- Assert ID uniqueness per section.
- Assert all download URLs match `https://huggingface.co/...`.
- Assert minimum expected entries are present: `gemma3:1b`, `gemma4:e2b`, `base` (Whisper), `parakeet-tdt-0.6b-v3-int8`.

**Does NOT check** (out of scope): that URLs return 200, exact file sizes on HF. Those would be flaky in CI.

### Smoke test (manual, before merge)

Run the app on Windows 11:
1. Verify UI shows `gemma3:1b` and `gemma4:e2b` under LLM (not `gemma3:4b`).
2. Verify 12 Whisper entries as before.
3. Verify Parakeet v2 + v3 entries.
4. Download `gemma4:e2b` → succeeds, file at expected path.
5. Download Parakeet v3 from istupakov HF → succeeds.
6. Pre-existing `gemma3:1b` file present → no re-download.

### Known testing caveat

`cargo test` currently fails with CRT mismatch (esaxx `/MT` vs whisper `/MD`) per memory note `build_cargo_test_crt_mismatch.md`. If the integration test is blocked by this, gate it behind a feature flag `--features catalog-tests` or a standalone test binary. To decide at implementation time.

### Verification commands

```bash
cd frontend/src-tauri
cargo check
cargo test models_catalog  # unit tests
cargo test --test catalog_validation  # integration
cargo clippy -- -D warnings
```

## Definition of Done

- [ ] `models_catalog.json` bundled as Tauri resource
- [ ] Three engines (LLM/Whisper/Parakeet) read from the catalog
- [ ] `grep -r "meetily.towardsgeneralintelligence.com" frontend/` returns zero matches
- [ ] `gemma4:e2b` downloadable and usable for summary generation
- [ ] Parakeet v3 downloadable from `istupakov/parakeet-tdt-0.6b-v3-onnx`
- [ ] `gemma3:4b` no longer in the catalog
- [ ] Unit + integration tests pass (or documented caveat applies)
- [ ] Windows smoke test passes
- [ ] UI unchanged (no visible regression)

## Out of scope — future work

1. **GPU acceleration for Parakeet** — enable `DirectMLExecutionProvider` (Windows universal) or `CUDAExecutionProvider` via the `ort/directml` or `ort/cuda` feature flag. Parakeet currently runs CPU-only at [`parakeet_engine/model.rs:91`](../../../frontend/src-tauri/src/parakeet_engine/model.rs). Estimated gain: 3-5×. Separate chantier.

2. **Add Canary** (`nvidia/canary-1b-v2` ONNX via istupakov) — the ONNX export exists and could be integrated without a NeMo runtime, treating it like a third Parakeet-style entry. Deferred per explicit user decision in this round. Separate ticket.

3. **Self-hosted mirror migration** — when the user's own S3/R2 bucket is ready (see memory note `todo_self_host_binaries.md`), swap the HuggingFace URLs in `models_catalog.json` for the self-hosted ones. No code change needed. Bump `schema_version` if any structural change is introduced.
