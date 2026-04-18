//! End-to-end semantic search test.
//! Requires the multilingual-e5-small model cache to be present.
//! Run with: cargo test --test semantic_search_fr -- --ignored --nocapture

use app_lib::search::chunker::{chunk_text, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP};
use app_lib::search::embedder;
use app_lib::search::engine::HybridSearchEngine;
use app_lib::search::searchers::semantic::EmbeddingCache;
use app_lib::search::types::{MatchKind, SourceType, EMBEDDING_MODEL_ID};
use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
#[ignore]
async fn french_query_matches_paraphrased_transcript() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    // Apply migrations
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    // Seed a meeting with a paraphrased line
    let mid = "meeting-1";
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO meetings(id,title,created_at,updated_at) VALUES (?1,?2,?3,?3)")
        .bind(mid).bind("Réunion produit").bind(&now)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO transcripts(id,meeting_id,transcript,timestamp) VALUES (?1,?2,?3,?4)")
        .bind("t1").bind(mid)
        .bind("On s'est mis d'accord sur les tarifs à 99 euros pour la V1")
        .bind(&now)
        .execute(&pool).await.unwrap();

    // Build chunks + embeddings manually (bypass indexer for clarity)
    let chunks = chunk_text(mid, SourceType::Transcript, Some("t1"),
        "On s'est mis d'accord sur les tarifs à 99 euros pour la V1",
        DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP);
    let texts: Vec<String> = chunks.iter().map(|c| c.chunk_text.clone()).collect();
    let vecs = embedder::embed_passages(&texts).await.unwrap();

    for (c, v) in chunks.iter().zip(vecs.iter()) {
        sqlx::query("INSERT INTO search_chunks(id,meeting_id,source_type,source_id,chunk_text,chunk_index,char_start,char_end,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)")
            .bind(&c.id).bind(&c.meeting_id).bind(c.source_type.as_str())
            .bind(&c.source_id).bind(&c.chunk_text).bind(c.chunk_index)
            .bind(c.char_start).bind(c.char_end).bind(&now)
            .execute(&pool).await.unwrap();
        let mut blob = Vec::with_capacity(v.len() * 4);
        for f in v { blob.extend_from_slice(&f.to_le_bytes()); }
        sqlx::query("INSERT INTO search_embeddings(chunk_id, embedding, model_id, created_at) VALUES (?1,?2,?3,?4)")
            .bind(&c.id).bind(&blob).bind(EMBEDDING_MODEL_ID).bind(&now)
            .execute(&pool).await.unwrap();
    }

    let cache = EmbeddingCache::new();
    cache.reload(&pool).await.unwrap();

    let engine = HybridSearchEngine::new(pool, cache);
    let hits = engine.search("décision pricing", 5).await.unwrap();

    assert!(!hits.is_empty(), "expected at least one hit for paraphrased query");
    assert!(
        hits.iter().any(|h| h.match_kinds.contains(&MatchKind::Semantic)),
        "expected semantic layer to contribute a hit"
    );
}
