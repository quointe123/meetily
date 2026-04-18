use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::search::types::{EMBEDDING_DIM, EMBEDDING_MODEL_ID};

static EMBEDDER: OnceCell<Arc<Mutex<TextEmbedding>>> = OnceCell::new();

/// Resolve the cache directory for the ONNX model (OS-appropriate APPDATA / Application Support).
pub fn model_cache_dir() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .context("no data_dir (APPDATA / Application Support) available on this platform")?;
    let dir = base.join("Meetily").join("models").join("embeddings");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {:?}", dir))?;
    Ok(dir)
}

/// Returns true if model files are already on disk (best-effort check).
pub fn is_model_cached() -> bool {
    match model_cache_dir() {
        Ok(dir) => std::fs::read_dir(&dir)
            .map(|rd| rd.flatten().next().is_some())
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Download + load the embedder. Blocks until ready. Safe to call repeatedly.
pub async fn ensure_embedder() -> Result<Arc<Mutex<TextEmbedding>>> {
    if let Some(e) = EMBEDDER.get() {
        return Ok(e.clone());
    }
    let cache = model_cache_dir()?;
    // fastembed downloads on first construction; subsequent calls reuse cached files.
    let handle = tokio::task::spawn_blocking(move || -> Result<TextEmbedding> {
        let opts = InitOptions::new(EmbeddingModel::MultilingualE5Small)
            .with_cache_dir(cache)
            .with_show_download_progress(false);
        TextEmbedding::try_new(opts).context("fastembed model init")
    })
    .await
    .context("join spawn_blocking for embedder init")??;
    let arc = Arc::new(Mutex::new(handle));
    let _ = EMBEDDER.set(arc.clone());
    Ok(arc)
}

/// Embed a batch of passages. Prefixes E5 "passage:" as recommended by the model card.
pub async fn embed_passages(texts: &[String]) -> Result<Vec<Vec<f32>>> {
    let prefixed: Vec<String> = texts.iter().map(|t| format!("passage: {}", t)).collect();
    let embedder = ensure_embedder().await?;
    let vecs = tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>> {
        let mut guard = embedder.blocking_lock();
        guard.embed(prefixed, None).context("fastembed embed passages")
    })
    .await
    .context("join spawn_blocking for embed")??;
    for v in &vecs {
        debug_assert_eq!(v.len(), EMBEDDING_DIM, "unexpected embedding dimension");
    }
    Ok(vecs)
}

/// Embed a single query. Prefixes "query:" per E5 convention.
pub async fn embed_query(query: &str) -> Result<Vec<f32>> {
    let q = format!("query: {}", query);
    let embedder = ensure_embedder().await?;
    let vec = tokio::task::spawn_blocking(move || -> Result<Vec<f32>> {
        let mut guard = embedder.blocking_lock();
        let mut out = guard.embed(vec![q], None).context("fastembed embed query")?;
        out.pop().context("empty embedding result")
    })
    .await
    .context("join spawn_blocking for embed_query")??;
    Ok(vec)
}

pub fn current_model_id() -> &'static str {
    EMBEDDING_MODEL_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // downloads ~470 MB on first run; enable manually
    async fn embeds_have_expected_shape() {
        let out = embed_passages(&["bonjour le monde".to_string()])
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), EMBEDDING_DIM);
    }
}
