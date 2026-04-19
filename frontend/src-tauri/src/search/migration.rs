use anyhow::Result;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime};

use crate::search::indexer::reindex_meeting;
use crate::search::searchers::semantic::EmbeddingCache;
use crate::search::types::{IndexingStatus, EMBEDDING_MODEL_ID};

const BACKFILL_PARALLELISM: usize = 2; // tolérant CPU/RAM (embed est le bottleneck)

#[derive(Debug, Clone, Serialize)]
pub struct BackfillProgress {
    pub processed: i64,
    pub total: i64,
    pub current_meeting_id: Option<String>,
    pub done: bool,
}

pub async fn current_status(pool: &SqlitePool) -> Result<IndexingStatus> {
    let total: i64 = sqlx::query("SELECT COUNT(*) AS c FROM meetings")
        .fetch_one(pool).await?.get("c");
    let indexed: i64 = sqlx::query(
        "SELECT COUNT(*) AS c FROM indexing_state WHERE status='embedded' AND model_id=?1",
    )
    .bind(EMBEDDING_MODEL_ID)
    .fetch_one(pool).await?.get("c");
    let chunks_total: i64 = sqlx::query("SELECT COALESCE(SUM(chunks_total),0) AS c FROM indexing_state")
        .fetch_one(pool).await?.get("c");
    let chunks_done: i64 = sqlx::query("SELECT COALESCE(SUM(chunks_done),0) AS c FROM indexing_state")
        .fetch_one(pool).await?.get("c");
    Ok(IndexingStatus {
        total_meetings: total,
        indexed_meetings: indexed,
        chunks_total,
        chunks_done,
        in_progress: indexed < total,
    })
}

/// Flag meetings whose source data contains content not yet reflected in `search_chunks`.
/// Currently: completed summaries in `summary_processes` with no corresponding Summary chunks.
/// Runs at startup before backfill so new sources trigger a re-index without manual action.
async fn flag_stale_indexes(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE indexing_state
        SET status = 'pending'
        WHERE status = 'embedded'
          AND meeting_id IN (
            SELECT sp.meeting_id FROM summary_processes sp
            WHERE sp.status = 'completed' AND sp.result IS NOT NULL
              AND NOT EXISTS (
                SELECT 1 FROM search_chunks sc
                WHERE sc.meeting_id = sp.meeting_id AND sc.source_type = 'summary'
              )
          )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete Whisper noise chunks that made it into the index before the chunker
/// learned to filter them. "...", "Uh", "Ok", lone CJK characters — each used to
/// attract stray semantic hits for unrelated queries. Uses the same threshold as
/// `chunker::is_meaningful` (≥5 alphanumeric chars).
/// `search_embeddings` cascades on delete; FTS is cleaned by the `search_chunks_ad` trigger.
async fn cleanup_noise_chunks(pool: &SqlitePool) -> Result<()> {
    // Approximate "alphanumeric count" in pure SQLite: the chunk must have some
    // letters, and its total length after stripping whitespace/punctuation should
    // be at least MIN_MEANINGFUL_ALNUM_CHARS. We implement this by counting the
    // characters that aren't whitespace/punctuation via LENGTH()-based heuristic.
    // Narrower SQL: drop chunks whose `chunk_text` is <=4 chars OR whose text
    // matches the silence-marker regex-equivalent ("..." / "Uh" / "Ok" / "And" / "No.").
    let res = sqlx::query(
        r#"
        DELETE FROM search_chunks
        WHERE LENGTH(TRIM(chunk_text)) <= 4
           OR chunk_text GLOB '[.][.][.]*'
           OR LOWER(TRIM(chunk_text)) IN ('uh', 'ok', 'and', 'no.', 'oui', 'non', 'you')
        "#,
    )
    .execute(pool)
    .await?;
    if res.rows_affected() > 0 {
        log::info!("cleaned up {} noise chunks from search_chunks", res.rows_affected());
    }
    Ok(())
}

/// Backfill embeddings for every meeting that isn't yet indexed with the current model.
/// Emits `semantic-indexing-progress` events for the UI.
pub async fn backfill<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    cache: Arc<EmbeddingCache>,
) -> Result<()> {
    if let Err(e) = flag_stale_indexes(&pool).await {
        log::warn!("flag_stale_indexes failed (non-fatal): {:?}", e);
    }
    if let Err(e) = cleanup_noise_chunks(&pool).await {
        log::warn!("cleanup_noise_chunks failed (non-fatal): {:?}", e);
    }

    let rows = sqlx::query(
        r#"
        SELECT m.id FROM meetings m
        LEFT JOIN indexing_state s ON s.meeting_id = m.id
        WHERE s.status IS NULL OR s.status <> 'embedded' OR s.model_id IS NULL OR s.model_id <> ?1
        "#,
    )
    .bind(EMBEDDING_MODEL_ID)
    .fetch_all(&pool)
    .await?;

    let total = rows.len() as i64;
    let ids: Vec<String> = rows.into_iter().map(|r| r.get::<String, _>("id")).collect();
    log::info!("semantic backfill: {} meetings to (re)index", total);

    let mut processed: i64 = 0;
    // Chunked parallelism by groups of BACKFILL_PARALLELISM
    for batch in ids.chunks(BACKFILL_PARALLELISM) {
        let mut handles = Vec::new();
        for mid in batch.iter().cloned() {
            let pool_c = pool.clone();
            let cache_c = cache.clone();
            handles.push(tokio::spawn(async move {
                let res = reindex_meeting(&pool_c, &cache_c, &mid).await;
                (mid, res)
            }));
        }
        for h in handles {
            match h.await {
                Ok((mid, Ok(()))) => {
                    processed += 1;
                    let _ = app.emit("semantic-indexing-progress", BackfillProgress {
                        processed, total, current_meeting_id: Some(mid), done: false,
                    });
                }
                Ok((mid, Err(e))) => {
                    log::error!("backfill failed for {}: {:?}", mid, e);
                    let _ = sqlx::query(
                        "UPDATE indexing_state SET status='failed', last_error=?1 WHERE meeting_id=?2",
                    )
                    .bind(format!("{:?}", e)).bind(&mid).execute(&pool).await;
                }
                Err(e) => log::error!("join error during backfill: {:?}", e),
            }
        }
    }

    let _ = app.emit("semantic-indexing-progress", BackfillProgress {
        processed, total, current_meeting_id: None, done: true,
    });
    log::info!("semantic backfill: done ({}/{})", processed, total);
    Ok(())
}
