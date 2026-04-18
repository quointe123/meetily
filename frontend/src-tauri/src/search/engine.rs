use anyhow::Result;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;

use crate::search::embedder;
use crate::search::fusion::{fuse, RRF_K};
use crate::search::searchers::{fts, fuzzy, semantic};
use crate::search::types::{Chunk, MatchKind, SearchHit, SourceType};

pub const FTS_CANDIDATES: usize = 50;
pub const SEMANTIC_CANDIDATES: usize = 50;
pub const FUZZY_POOL: usize = 200;
pub const DEFAULT_LIMIT: usize = 20;

pub struct HybridSearchEngine {
    pub pool: SqlitePool,
    pub cache: Arc<semantic::EmbeddingCache>,
}

impl HybridSearchEngine {
    pub fn new(pool: SqlitePool, cache: Arc<semantic::EmbeddingCache>) -> Self {
        Self { pool, cache }
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 1. Run FTS5 and semantic in parallel
        let fts_fut = fts::search(&self.pool, query, FTS_CANDIDATES);
        let sem_fut = async {
            match embedder::embed_query(query).await {
                Ok(qv) => Ok::<_, anyhow::Error>(
                    semantic::search(&self.cache, &qv, SEMANTIC_CANDIDATES).await,
                ),
                Err(e) => {
                    log::warn!("semantic search skipped: {:?}", e);
                    Ok(Vec::new())
                }
            }
        };
        let (fts_hits, sem_hits) = tokio::join!(fts_fut, sem_fut);
        let fts_hits = fts_hits?;
        let sem_hits = sem_hits?;

        // 2. For fuzzy, pull chunk_text for the top-N FTS candidates and rescore
        let fts_top_ids: Vec<String> = fts_hits
            .iter()
            .take(FUZZY_POOL)
            .map(|h| h.chunk_id.clone())
            .collect();
        let candidates = if fts_top_ids.is_empty() {
            Vec::new()
        } else {
            self.load_chunks_by_ids(&fts_top_ids).await?
        };
        let fuz_hits = fuzzy::search(query, &candidates, 50);

        // 3. RRF fusion
        let fused = fuse(
            &[
                (MatchKind::Fts, fts_hits),
                (MatchKind::Semantic, sem_hits),
                (MatchKind::Fuzzy, fuz_hits),
            ],
            RRF_K,
        );

        // 4. Materialize top-N hits with chunk + meeting info, apply source multiplier
        let top_ids: Vec<String> = fused.iter().take(limit * 2).map(|f| f.chunk_id.clone()).collect();
        let chunks = self.load_chunks_with_meeting(&top_ids).await?;

        let mut hits: Vec<SearchHit> = fused
            .into_iter()
            .filter_map(|f| {
                let (chunk, meeting_title) = chunks.iter().find(|(c, _)| c.id == f.chunk_id)?;
                let mult = chunk.source_type.score_multiplier();
                Some(SearchHit {
                    meeting_id: chunk.meeting_id.clone(),
                    meeting_title: meeting_title.clone(),
                    source_type: chunk.source_type,
                    source_id: chunk.source_id.clone(),
                    chunk_text: chunk.chunk_text.clone(),
                    char_start: chunk.char_start,
                    char_end: chunk.char_end,
                    score: f.score * mult,
                    match_kinds: f.match_kinds,
                })
            })
            .collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(limit);
        Ok(hits)
    }

    async fn load_chunks_by_ids(&self, ids: &[String]) -> Result<Vec<Chunk>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, meeting_id, source_type, source_id, chunk_text, chunk_index, char_start, char_end FROM search_chunks WHERE id IN ({})",
            placeholders
        );
        let mut q = sqlx::query(&sql);
        for id in ids {
            q = q.bind(id);
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let st: String = r.get("source_type");
                Some(Chunk {
                    id: r.get("id"),
                    meeting_id: r.get("meeting_id"),
                    source_type: SourceType::from_str(&st)?,
                    source_id: r.get("source_id"),
                    chunk_text: r.get("chunk_text"),
                    chunk_index: r.get("chunk_index"),
                    char_start: r.get("char_start"),
                    char_end: r.get("char_end"),
                })
            })
            .collect())
    }

    async fn load_chunks_with_meeting(&self, ids: &[String]) -> Result<Vec<(Chunk, String)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT c.id, c.meeting_id, c.source_type, c.source_id, c.chunk_text, c.chunk_index, c.char_start, c.char_end, m.title AS meeting_title
             FROM search_chunks c LEFT JOIN meetings m ON m.id = c.meeting_id
             WHERE c.id IN ({})",
            placeholders
        );
        let mut q = sqlx::query(&sql);
        for id in ids {
            q = q.bind(id);
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let st: String = r.get("source_type");
                Some((
                    Chunk {
                        id: r.get("id"),
                        meeting_id: r.get("meeting_id"),
                        source_type: SourceType::from_str(&st)?,
                        source_id: r.get("source_id"),
                        chunk_text: r.get("chunk_text"),
                        chunk_index: r.get("chunk_index"),
                        char_start: r.get("char_start"),
                        char_end: r.get("char_end"),
                    },
                    r.try_get::<String, _>("meeting_title").unwrap_or_default(),
                ))
            })
            .collect())
    }
}
