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
