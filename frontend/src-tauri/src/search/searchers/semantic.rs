use anyhow::Result;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::search::types::{RankedHit, EMBEDDING_DIM, EMBEDDING_MODEL_ID};

pub const SEMANTIC_THRESHOLD: f32 = 0.35;

#[derive(Debug, Clone)]
pub struct EmbeddingEntry {
    pub chunk_id: String,
    pub vector: Vec<f32>,
}

#[derive(Default)]
pub struct EmbeddingCache {
    inner: RwLock<Vec<EmbeddingEntry>>,
}

impl EmbeddingCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn reload(&self, pool: &SqlitePool) -> Result<()> {
        let rows = sqlx::query(
            "SELECT chunk_id, embedding FROM search_embeddings WHERE model_id = ?1",
        )
        .bind(EMBEDDING_MODEL_ID)
        .fetch_all(pool)
        .await?;

        let mut buf = Vec::with_capacity(rows.len());
        for r in rows {
            let blob: Vec<u8> = r.get("embedding");
            if blob.len() != EMBEDDING_DIM * 4 {
                continue; // skip malformed
            }
            let mut v = Vec::with_capacity(EMBEDDING_DIM);
            for c in blob.chunks_exact(4) {
                v.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
            }
            buf.push(EmbeddingEntry {
                chunk_id: r.get("chunk_id"),
                vector: v,
            });
        }
        let mut w = self.inner.write().await;
        *w = buf;
        Ok(())
    }

    pub async fn upsert(&self, entries: Vec<EmbeddingEntry>) {
        let mut w = self.inner.write().await;
        // naive upsert: drop then push (cache size is small; for hotter paths we'd use a HashMap)
        let ids: std::collections::HashSet<&str> =
            entries.iter().map(|e| e.chunk_id.as_str()).collect();
        w.retain(|e| !ids.contains(e.chunk_id.as_str()));
        w.extend(entries);
    }

    pub async fn remove(&self, chunk_ids: &[String]) {
        let set: std::collections::HashSet<&str> = chunk_ids.iter().map(|s| s.as_str()).collect();
        let mut w = self.inner.write().await;
        w.retain(|e| !set.contains(e.chunk_id.as_str()));
    }

    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len().min(b.len()) {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

pub async fn search(
    cache: &EmbeddingCache,
    query_vec: &[f32],
    limit: usize,
) -> Vec<RankedHit> {
    let guard = cache.inner.read().await;
    let mut scored: Vec<RankedHit> = guard
        .iter()
        .map(|e| RankedHit {
            chunk_id: e.chunk_id.clone(),
            score: cosine(query_vec, &e.vector),
        })
        .filter(|h| h.score >= SEMANTIC_THRESHOLD)
        .collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical_vectors_is_one() {
        let v = vec![0.5, 0.5, 0.0, 0.0];
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_vectors_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_zero_vector_is_zero() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 1.0];
        assert_eq!(cosine(&a, &b), 0.0);
    }
}
