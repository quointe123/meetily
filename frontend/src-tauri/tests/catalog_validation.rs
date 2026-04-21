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
