use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::search::chunker::{chunk_text, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP};
use crate::search::embedder;
use crate::search::searchers::semantic::{EmbeddingCache, EmbeddingEntry};
use crate::search::types::{Chunk, SourceType, EMBEDDING_MODEL_ID};

/// Extract all indexable chunks for a single meeting by reading its title + transcripts + notes.
pub async fn collect_chunks_for_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<Chunk>> {
    let mut all = Vec::new();

    // 1. Title
    let title_row = sqlx::query("SELECT title FROM meetings WHERE id = ?1")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    if let Some(r) = title_row {
        let title: String = r.get("title");
        all.extend(chunk_text(
            meeting_id,
            SourceType::Title,
            None,
            &title,
            DEFAULT_CHUNK_SIZE,
            DEFAULT_OVERLAP,
        ));
    }

    // 2. Transcripts + their per-row summary/action_items/key_points
    let transcripts = sqlx::query(
        "SELECT id, transcript, summary, action_items, key_points FROM transcripts WHERE meeting_id = ?1",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    for r in transcripts {
        let tid: String = r.get("id");
        let text: String = r.get("transcript");
        all.extend(chunk_text(
            meeting_id,
            SourceType::Transcript,
            Some(&tid),
            &text,
            DEFAULT_CHUNK_SIZE,
            DEFAULT_OVERLAP,
        ));
        if let Ok(Some(s)) = r.try_get::<Option<String>, _>("summary") {
            all.extend(chunk_text(meeting_id, SourceType::Summary, Some(&tid), &s, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP));
        }
        if let Ok(Some(s)) = r.try_get::<Option<String>, _>("action_items") {
            all.extend(chunk_text(meeting_id, SourceType::ActionItems, Some(&tid), &s, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP));
        }
        if let Ok(Some(s)) = r.try_get::<Option<String>, _>("key_points") {
            all.extend(chunk_text(meeting_id, SourceType::KeyPoints, Some(&tid), &s, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP));
        }
    }

    // 3. meeting_notes table — schema: (meeting_id PK, notes_markdown, notes_json, ...)
    // Note: no separate `id` column; meeting_id is the PK. We use meeting_id as source_id.
    // Both notes_markdown and notes_json are indexed when present.
    let notes = sqlx::query(
        "SELECT meeting_id, notes_markdown, notes_json FROM meeting_notes WHERE meeting_id = ?1",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for r in notes {
        let nid: String = r.try_get("meeting_id").unwrap_or_default();
        if let Ok(Some(md)) = r.try_get::<Option<String>, _>("notes_markdown") {
            all.extend(chunk_text(meeting_id, SourceType::Notes, Some(&nid), &md, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP));
        }
        if let Ok(Some(json)) = r.try_get::<Option<String>, _>("notes_json") {
            all.extend(chunk_text(meeting_id, SourceType::Notes, Some(&nid), &json, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP));
        }
    }

    // 4. summary_processes — AI-generated summaries stored as JSON {"markdown": "..."}.
    // Falls back to the raw string if the payload isn't JSON or lacks a `markdown` field.
    let summaries = sqlx::query(
        "SELECT result FROM summary_processes WHERE meeting_id = ?1 AND status = 'completed' AND result IS NOT NULL",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for r in summaries {
        let raw: String = r.try_get("result").unwrap_or_default();
        let text = serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| v.get("markdown").and_then(|m| m.as_str()).map(|s| s.to_string()))
            .unwrap_or(raw);
        if !text.trim().is_empty() {
            all.extend(chunk_text(
                meeting_id,
                SourceType::Summary,
                Some(meeting_id),
                &text,
                DEFAULT_CHUNK_SIZE,
                DEFAULT_OVERLAP,
            ));
        }
    }

    Ok(all)
}

/// Replace all chunks + embeddings for one meeting atomically.
pub async fn reindex_meeting(
    pool: &SqlitePool,
    cache: &EmbeddingCache,
    meeting_id: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();

    // Collect and chunk
    let chunks = collect_chunks_for_meeting(pool, meeting_id).await?;

    // Update indexing_state to 'chunked' with total
    sqlx::query(
        r#"INSERT INTO indexing_state(meeting_id, status, chunks_total, chunks_done, model_id, updated_at)
           VALUES (?1, 'chunked', ?2, 0, ?3, ?4)
           ON CONFLICT(meeting_id) DO UPDATE SET
             status='chunked', chunks_total=?2, chunks_done=0, model_id=?3, updated_at=?4, last_error=NULL"#,
    )
    .bind(meeting_id)
    .bind(chunks.len() as i64)
    .bind(EMBEDDING_MODEL_ID)
    .bind(&now)
    .execute(pool)
    .await?;

    // Delete old chunks for this meeting (embeddings cascade)
    let old_ids: Vec<String> = sqlx::query("SELECT id FROM search_chunks WHERE meeting_id = ?1")
        .bind(meeting_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>("id"))
        .collect();
    if !old_ids.is_empty() {
        cache.remove(&old_ids).await;
    }
    sqlx::query("DELETE FROM search_chunks WHERE meeting_id = ?1")
        .bind(meeting_id)
        .execute(pool)
        .await?;

    if chunks.is_empty() {
        sqlx::query("UPDATE indexing_state SET status='embedded', chunks_done=0, updated_at=?1 WHERE meeting_id=?2")
            .bind(&now).bind(meeting_id).execute(pool).await?;
        return Ok(());
    }

    // Insert chunks
    let mut tx = pool.begin().await?;
    for c in &chunks {
        sqlx::query(
            r#"INSERT INTO search_chunks(id, meeting_id, source_type, source_id, chunk_text, chunk_index, char_start, char_end, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
        )
        .bind(&c.id).bind(&c.meeting_id).bind(c.source_type.as_str())
        .bind(&c.source_id).bind(&c.chunk_text).bind(c.chunk_index)
        .bind(c.char_start).bind(c.char_end).bind(&now)
        .execute(&mut *tx).await?;
    }
    tx.commit().await?;

    // Batch-embed in groups of 32
    let mut inserted_in_cache: Vec<EmbeddingEntry> = Vec::with_capacity(chunks.len());
    for batch in chunks.chunks(32) {
        let texts: Vec<String> = batch.iter().map(|c| c.chunk_text.clone()).collect();
        let vecs = embedder::embed_passages(&texts).await.context("embed batch")?;
        let mut tx = pool.begin().await?;
        for (c, v) in batch.iter().zip(vecs.iter()) {
            let mut blob = Vec::with_capacity(v.len() * 4);
            for f in v {
                blob.extend_from_slice(&f.to_le_bytes());
            }
            sqlx::query(
                r#"INSERT INTO search_embeddings(chunk_id, embedding, model_id, created_at)
                   VALUES (?1, ?2, ?3, ?4)
                   ON CONFLICT(chunk_id) DO UPDATE SET embedding=?2, model_id=?3, created_at=?4"#,
            )
            .bind(&c.id).bind(&blob).bind(EMBEDDING_MODEL_ID).bind(&now)
            .execute(&mut *tx).await?;
            inserted_in_cache.push(EmbeddingEntry { chunk_id: c.id.clone(), vector: v.clone() });
        }
        tx.commit().await?;
        // progress update
        sqlx::query("UPDATE indexing_state SET chunks_done = chunks_done + ?1, updated_at=?2 WHERE meeting_id=?3")
            .bind(batch.len() as i64).bind(Utc::now().to_rfc3339()).bind(meeting_id)
            .execute(pool).await?;
    }

    cache.upsert(inserted_in_cache).await;

    sqlx::query("UPDATE indexing_state SET status='embedded', updated_at=?1 WHERE meeting_id=?2")
        .bind(Utc::now().to_rfc3339()).bind(meeting_id).execute(pool).await?;

    Ok(())
}
