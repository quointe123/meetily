# Semantic Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a fully local hybrid search engine (FTS5 + fuzzy + semantic embeddings) in Rust/Tauri to replace the Python backend-dependent search, so that semantic search works out of the box after onboarding.

**Architecture:** New `search/` Rust module in `frontend/src-tauri/src/`. Three searchers run in parallel (FTS5 via sqlx, cosine over in-RAM embeddings, rapidfuzz rescore of FTS candidates), fused with Reciprocal Rank Fusion. Embeddings produced by `fastembed-rs` + `multilingual-e5-small` (ONNX, ~470 MB, auto-downloaded at onboarding). Storage in the existing SQLite (new tables + FTS5 virtual table). Existing meetings re-indexed in background at first boot.

**Tech Stack:** Rust (tokio, sqlx, fastembed 5.x, rapidfuzz 0.5, once_cell), SQLite FTS5, Tauri 2.6, Next.js 14 / React 18 frontend.
*(Note: fastembed 5 was chosen over 4 because fastembed 4 pins `ort` to rc.5 while the repo already uses `ort = "2.0.0-rc.10"` for Parakeet — fastembed 5 is compatible with rc.10.)*

**Spec reference:** [docs/superpowers/specs/2026-04-18-semantic-search-design.md](../specs/2026-04-18-semantic-search-design.md)

---

## Phase 1 — Foundations (dependencies + schema)

### Task 1: Add crate dependencies

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`

- [ ] **Step 1: Add `fastembed` and `rapidfuzz` to `[dependencies]`**

Edit `frontend/src-tauri/Cargo.toml`, locate the `[dependencies]` block (line 64) and append before the `[target...]` sections (around line 155):

```toml
# Semantic search dependencies
fastembed = "5"        # ONNX-based embeddings (compatible with existing ort = 2.0.0-rc.10)
rapidfuzz = "0.5"      # Fuzzy string matching (Levenshtein / token set ratio)
```

- [ ] **Step 2: Verify crates resolve**

Run: `cd frontend/src-tauri && cargo check --no-default-features`
Expected: compiles without errors (new crates downloaded).

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/Cargo.lock
git commit -m "deps: add fastembed and rapidfuzz for semantic search"
```

---

### Task 2: Add search_chunks + search_embeddings + FTS5 + indexing_state migration

**Files:**
- Create: `frontend/src-tauri/migrations/20260418000000_add_semantic_search.sql`

- [ ] **Step 1: Create the migration file with the full schema**

Write the exact content below to `frontend/src-tauri/migrations/20260418000000_add_semantic_search.sql`:

```sql
-- search_chunks: unité indexable (chunk d'un texte source)
CREATE TABLE IF NOT EXISTS search_chunks (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT,
    chunk_text TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    char_start INTEGER,
    char_end INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_search_chunks_meeting ON search_chunks(meeting_id);
CREATE INDEX IF NOT EXISTS idx_search_chunks_source ON search_chunks(source_type, source_id);

-- search_embeddings: vecteurs denses (une ligne par chunk)
CREATE TABLE IF NOT EXISTS search_embeddings (
    chunk_id TEXT PRIMARY KEY,
    embedding BLOB NOT NULL,
    model_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (chunk_id) REFERENCES search_chunks(id) ON DELETE CASCADE
);

-- FTS5 virtual table backed by search_chunks
CREATE VIRTUAL TABLE IF NOT EXISTS search_chunks_fts USING fts5(
    chunk_text,
    content='search_chunks',
    content_rowid='rowid',
    tokenize='unicode61 remove_diacritics 2'
);

-- Triggers to keep FTS5 in sync
CREATE TRIGGER IF NOT EXISTS search_chunks_ai AFTER INSERT ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(rowid, chunk_text) VALUES (new.rowid, new.chunk_text);
END;
CREATE TRIGGER IF NOT EXISTS search_chunks_ad AFTER DELETE ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(search_chunks_fts, rowid, chunk_text) VALUES('delete', old.rowid, old.chunk_text);
END;
CREATE TRIGGER IF NOT EXISTS search_chunks_au AFTER UPDATE ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(search_chunks_fts, rowid, chunk_text) VALUES('delete', old.rowid, old.chunk_text);
    INSERT INTO search_chunks_fts(rowid, chunk_text) VALUES (new.rowid, new.chunk_text);
END;

-- indexing_state: crash recovery + progress UI
CREATE TABLE IF NOT EXISTS indexing_state (
    meeting_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    chunks_total INTEGER DEFAULT 0,
    chunks_done INTEGER DEFAULT 0,
    model_id TEXT,
    last_error TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
```

- [ ] **Step 2: Verify migration picks up on next build**

Run: `cd frontend/src-tauri && cargo check`
Expected: no errors (migrations are discovered at build time by the existing sqlx setup).

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/migrations/20260418000000_add_semantic_search.sql
git commit -m "db: add semantic search tables (search_chunks, embeddings, FTS5, indexing_state)"
```

---

### Task 3: Create `search/` module skeleton with shared types

**Files:**
- Create: `frontend/src-tauri/src/search/mod.rs`
- Create: `frontend/src-tauri/src/search/types.rs`
- Modify: `frontend/src-tauri/src/lib.rs` (line 38-55 module declarations)

- [ ] **Step 1: Create `search/types.rs` with public types**

Write to `frontend/src-tauri/src/search/types.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const EMBEDDING_MODEL_ID: &str = "multilingual-e5-small@v1";
pub const EMBEDDING_DIM: usize = 384;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Transcript,
    Title,
    Summary,
    ActionItems,
    KeyPoints,
    Notes,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::Title => "title",
            Self::Summary => "summary",
            Self::ActionItems => "action_items",
            Self::KeyPoints => "key_points",
            Self::Notes => "notes",
        }
    }

    pub fn score_multiplier(&self) -> f32 {
        match self {
            Self::Title => 1.3,
            Self::Summary | Self::KeyPoints => 1.15,
            Self::ActionItems => 1.1,
            Self::Transcript | Self::Notes => 1.0,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "transcript" => Some(Self::Transcript),
            "title" => Some(Self::Title),
            "summary" => Some(Self::Summary),
            "action_items" => Some(Self::ActionItems),
            "key_points" => Some(Self::KeyPoints),
            "notes" => Some(Self::Notes),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Fts,
    Semantic,
    Fuzzy,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: String,
    pub meeting_id: String,
    pub source_type: SourceType,
    pub source_id: Option<String>,
    pub chunk_text: String,
    pub chunk_index: i64,
    pub char_start: Option<i64>,
    pub char_end: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RankedHit {
    pub chunk_id: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub meeting_id: String,
    pub meeting_title: String,
    pub source_type: SourceType,
    pub chunk_text: String,
    pub char_start: Option<i64>,
    pub char_end: Option<i64>,
    pub score: f32,
    pub match_kinds: Vec<MatchKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingStatus {
    pub total_meetings: i64,
    pub indexed_meetings: i64,
    pub chunks_total: i64,
    pub chunks_done: i64,
    pub in_progress: bool,
}
```

- [ ] **Step 2: Create `search/mod.rs` re-exporting the types**

Write to `frontend/src-tauri/src/search/mod.rs`:

```rust
pub mod types;

pub use types::{
    Chunk, IndexingStatus, MatchKind, RankedHit, SearchHit, SourceType,
    EMBEDDING_DIM, EMBEDDING_MODEL_ID,
};
```

- [ ] **Step 3: Register the module in `lib.rs`**

Edit `frontend/src-tauri/src/lib.rs` — add `pub mod search;` in the module declarations block (line 38-55), alphabetically between `pub mod parakeet_engine;` and `pub mod state;`:

```rust
pub mod parakeet_engine;
pub mod search;
pub mod state;
```

- [ ] **Step 4: Verify it compiles**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles cleanly (dead_code warnings acceptable at this stage).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/search/ frontend/src-tauri/src/lib.rs
git commit -m "search: scaffold module with shared types (SearchHit, Chunk, SourceType, MatchKind)"
```

---

## Phase 2 — Chunker

### Task 4: Implement chunker with overlap

**Files:**
- Create: `frontend/src-tauri/src/search/chunker.rs`
- Modify: `frontend/src-tauri/src/search/mod.rs`

- [ ] **Step 1: Add failing unit tests**

Write to `frontend/src-tauri/src/search/chunker.rs`:

```rust
use crate::search::types::{Chunk, SourceType};
use uuid::Uuid;

pub const DEFAULT_CHUNK_SIZE: usize = 800;
pub const DEFAULT_OVERLAP: usize = 200;

pub fn chunk_text(
    meeting_id: &str,
    source_type: SourceType,
    source_id: Option<&str>,
    text: &str,
    chunk_size: usize,
    overlap: usize,
) -> Vec<Chunk> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Short texts (title, summary, action_items, etc.) fit in a single chunk.
    if trimmed.len() <= chunk_size {
        return vec![Chunk {
            id: Uuid::new_v4().to_string(),
            meeting_id: meeting_id.to_string(),
            source_type,
            source_id: source_id.map(|s| s.to_string()),
            chunk_text: trimmed.to_string(),
            chunk_index: 0,
            char_start: Some(0),
            char_end: Some(trimmed.len() as i64),
        }];
    }

    let bytes = trimmed.as_bytes();
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut idx = 0i64;

    while start < bytes.len() {
        let mut end = (start + chunk_size).min(bytes.len());
        // Snap to word boundary if not at end of text
        if end < bytes.len() {
            while end > start && !bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
            if end == start {
                end = (start + chunk_size).min(bytes.len());
            }
        }
        // UTF-8 safety: back up until we're on a char boundary
        while end < bytes.len() && !trimmed.is_char_boundary(end) {
            end -= 1;
        }

        let slice = &trimmed[start..end];
        chunks.push(Chunk {
            id: Uuid::new_v4().to_string(),
            meeting_id: meeting_id.to_string(),
            source_type,
            source_id: source_id.map(|s| s.to_string()),
            chunk_text: slice.to_string(),
            chunk_index: idx,
            char_start: Some(start as i64),
            char_end: Some(end as i64),
        });

        if end >= bytes.len() {
            break;
        }
        let step = chunk_size.saturating_sub(overlap).max(1);
        start = (start + step).min(bytes.len());
        // UTF-8 safety on start too
        while start < bytes.len() && !trimmed.is_char_boundary(start) {
            start += 1;
        }
        idx += 1;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_produces_single_chunk() {
        let out = chunk_text("m1", SourceType::Title, None, "Hello world", 800, 200);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].chunk_text, "Hello world");
        assert_eq!(out[0].chunk_index, 0);
    }

    #[test]
    fn empty_text_returns_empty() {
        let out = chunk_text("m1", SourceType::Transcript, None, "", 800, 200);
        assert!(out.is_empty());
    }

    #[test]
    fn long_text_is_split_with_overlap() {
        let text = "word ".repeat(500); // ~2500 chars
        let out = chunk_text("m1", SourceType::Transcript, None, &text, 800, 200);
        assert!(out.len() >= 3);
        // Overlap: char_start of chunk N+1 < char_end of chunk N
        for pair in out.windows(2) {
            assert!(pair[1].char_start.unwrap() < pair[0].char_end.unwrap());
        }
    }

    #[test]
    fn utf8_boundaries_are_respected() {
        let text = "café ".repeat(300);
        let out = chunk_text("m1", SourceType::Transcript, None, &text, 800, 200);
        // Each chunk must contain only valid UTF-8 (implicit: String construction would panic otherwise)
        assert!(out.iter().all(|c| !c.chunk_text.is_empty()));
    }
}
```

- [ ] **Step 2: Register chunker in `search/mod.rs`**

Edit `frontend/src-tauri/src/search/mod.rs`:

```rust
pub mod chunker;
pub mod types;

pub use chunker::{chunk_text, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP};
pub use types::{
    Chunk, IndexingStatus, MatchKind, RankedHit, SearchHit, SourceType,
    EMBEDDING_DIM, EMBEDDING_MODEL_ID,
};
```

- [ ] **Step 3: Run tests — they should pass**

Run: `cd frontend/src-tauri && cargo test --lib search::chunker`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/search/chunker.rs frontend/src-tauri/src/search/mod.rs
git commit -m "search: implement chunker with overlap and UTF-8 safety"
```

---

## Phase 3 — Embedder

### Task 5: Implement embedder with lazy-loaded ONNX model

**Files:**
- Create: `frontend/src-tauri/src/search/embedder.rs`
- Modify: `frontend/src-tauri/src/search/mod.rs`

- [ ] **Step 1: Write `embedder.rs`**

Write to `frontend/src-tauri/src/search/embedder.rs`:

```rust
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
```

- [ ] **Step 2: Register `embedder` in `search/mod.rs`**

Edit `frontend/src-tauri/src/search/mod.rs`, add:

```rust
pub mod embedder;
```

at the top, alongside `chunker` and `types`.

- [ ] **Step 3: Compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/search/embedder.rs frontend/src-tauri/src/search/mod.rs
git commit -m "search: add lazy-loaded multilingual-e5-small embedder via fastembed"
```

---

## Phase 4 — Searchers (FTS, Semantic, Fuzzy)

### Task 6: FTS5 searcher

**Files:**
- Create: `frontend/src-tauri/src/search/searchers/mod.rs`
- Create: `frontend/src-tauri/src/search/searchers/fts.rs`
- Modify: `frontend/src-tauri/src/search/mod.rs`

- [ ] **Step 1: Create searchers module**

Write to `frontend/src-tauri/src/search/searchers/mod.rs`:

```rust
pub mod fts;
pub mod fuzzy;
pub mod semantic;
```

- [ ] **Step 2: Write FTS5 searcher**

Write to `frontend/src-tauri/src/search/searchers/fts.rs`:

```rust
use anyhow::Result;
use sqlx::{Row, SqlitePool};

use crate::search::types::RankedHit;

/// Escape a user query for FTS5 MATCH:
/// - wrap each token in double quotes to disable operator parsing
/// - append `*` to the last token for prefix match
pub fn escape_fts_query(query: &str) -> String {
    let tokens: Vec<&str> = query.split_whitespace().filter(|t| !t.is_empty()).collect();
    if tokens.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = tokens
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if let Some(last) = parts.last_mut() {
        // remove trailing closing quote to append *
        if last.ends_with('"') {
            last.pop();
            last.push_str("\"*");
        }
    }
    parts.join(" ")
}

/// Search FTS5 with BM25 ranking. Returns top `limit` chunk ids with their BM25 score
/// (lower is better, so we negate to keep "higher = better" convention).
pub async fn search(pool: &SqlitePool, query: &str, limit: usize) -> Result<Vec<RankedHit>> {
    let match_expr = escape_fts_query(query);
    if match_expr.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT c.id AS chunk_id, bm25(search_chunks_fts) AS bm25_score
        FROM search_chunks_fts
        JOIN search_chunks c ON c.rowid = search_chunks_fts.rowid
        WHERE search_chunks_fts MATCH ?1
        ORDER BY bm25_score
        LIMIT ?2
        "#,
    )
    .bind(&match_expr)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| RankedHit {
            chunk_id: r.get::<String, _>("chunk_id"),
            score: -r.get::<f64, _>("bm25_score") as f32,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_preserves_tokens_and_adds_prefix_star() {
        assert_eq!(escape_fts_query("hello world"), "\"hello\" \"world\"*");
    }

    #[test]
    fn escape_handles_empty() {
        assert_eq!(escape_fts_query(""), "");
        assert_eq!(escape_fts_query("   "), "");
    }

    #[test]
    fn escape_escapes_embedded_double_quotes() {
        assert_eq!(escape_fts_query("say \"hi\""), "\"say\" \"\"\"hi\"\"\"*");
    }
}
```

- [ ] **Step 3: Register `searchers` in `search/mod.rs`**

Edit `frontend/src-tauri/src/search/mod.rs`, add:

```rust
pub mod searchers;
```

- [ ] **Step 4: Run unit tests**

Run: `cd frontend/src-tauri && cargo test --lib search::searchers::fts`
Expected: 3 tests pass (escape_* tests; the search() function requires a DB so is not unit-tested here).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/search/searchers/ frontend/src-tauri/src/search/mod.rs
git commit -m "search: add FTS5 searcher with BM25 ranking and safe query escaping"
```

---

### Task 7: Semantic searcher with in-RAM cosine similarity

**Files:**
- Create: `frontend/src-tauri/src/search/searchers/semantic.rs`

- [ ] **Step 1: Write semantic searcher**

Write to `frontend/src-tauri/src/search/searchers/semantic.rs`:

```rust
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
```

- [ ] **Step 2: Run tests**

Run: `cd frontend/src-tauri && cargo test --lib search::searchers::semantic`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/search/searchers/semantic.rs
git commit -m "search: add semantic searcher with in-RAM cosine cache"
```

---

### Task 8: Fuzzy rescorer for FTS candidates

**Files:**
- Create: `frontend/src-tauri/src/search/searchers/fuzzy.rs`

- [ ] **Step 1: Write fuzzy searcher**

Write to `frontend/src-tauri/src/search/searchers/fuzzy.rs`:

```rust
use rapidfuzz::fuzz::token_set_ratio;

use crate::search::types::{Chunk, RankedHit};

pub const FUZZY_THRESHOLD: f32 = 70.0;

/// Rescore a set of FTS candidate chunks against the query using token_set_ratio.
/// Returns the subset above FUZZY_THRESHOLD, sorted by score desc, capped at `limit`.
pub fn search(query: &str, candidates: &[Chunk], limit: usize) -> Vec<RankedHit> {
    let q = query.to_lowercase();
    let mut scored: Vec<RankedHit> = candidates
        .iter()
        .map(|c| {
            let s = token_set_ratio(&q, &c.chunk_text.to_lowercase()) as f32;
            RankedHit {
                chunk_id: c.id.clone(),
                score: s,
            }
        })
        .filter(|h| h.score >= FUZZY_THRESHOLD)
        .collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::types::SourceType;

    fn mk(id: &str, text: &str) -> Chunk {
        Chunk {
            id: id.into(),
            meeting_id: "m1".into(),
            source_type: SourceType::Transcript,
            source_id: None,
            chunk_text: text.into(),
            chunk_index: 0,
            char_start: None,
            char_end: None,
        }
    }

    #[test]
    fn exact_match_scores_high() {
        let cands = [mk("a", "decision on pricing")];
        let hits = search("decision on pricing", &cands, 10);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].score >= 95.0);
    }

    #[test]
    fn typo_is_tolerated() {
        let cands = [mk("a", "decision on pricing")];
        let hits = search("decison pricng", &cands, 10);
        assert_eq!(hits.len(), 1, "expected typo to still match");
    }

    #[test]
    fn unrelated_text_is_filtered_out() {
        let cands = [mk("a", "completely unrelated sentence")];
        let hits = search("pricing decision", &cands, 10);
        assert!(hits.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd frontend/src-tauri && cargo test --lib search::searchers::fuzzy`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/search/searchers/fuzzy.rs
git commit -m "search: add fuzzy rescorer using rapidfuzz token_set_ratio"
```

---

## Phase 5 — Fusion + Engine

### Task 9: Reciprocal Rank Fusion

**Files:**
- Create: `frontend/src-tauri/src/search/fusion.rs`
- Modify: `frontend/src-tauri/src/search/mod.rs`

- [ ] **Step 1: Write RRF implementation**

Write to `frontend/src-tauri/src/search/fusion.rs`:

```rust
use std::collections::HashMap;

use crate::search::types::{MatchKind, RankedHit};

pub const RRF_K: f32 = 60.0;

#[derive(Debug, Clone)]
pub struct FusedHit {
    pub chunk_id: String,
    pub score: f32,
    pub match_kinds: Vec<MatchKind>,
}

/// Reciprocal Rank Fusion: score(c) = Σ 1 / (k + rank_i(c))
/// `inputs` pairs each ranking with the MatchKind it corresponds to.
pub fn fuse(inputs: &[(MatchKind, Vec<RankedHit>)], k: f32) -> Vec<FusedHit> {
    let mut by_id: HashMap<String, FusedHit> = HashMap::new();

    for (kind, ranking) in inputs {
        for (idx, hit) in ranking.iter().enumerate() {
            let rrf = 1.0 / (k + (idx + 1) as f32);
            let entry = by_id
                .entry(hit.chunk_id.clone())
                .or_insert_with(|| FusedHit {
                    chunk_id: hit.chunk_id.clone(),
                    score: 0.0,
                    match_kinds: Vec::new(),
                });
            entry.score += rrf;
            if !entry.match_kinds.contains(kind) {
                entry.match_kinds.push(*kind);
            }
        }
    }

    let mut out: Vec<FusedHit> = by_id.into_values().collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(id: &str, score: f32) -> RankedHit {
        RankedHit { chunk_id: id.into(), score }
    }

    #[test]
    fn hit_found_by_all_three_ranks_first() {
        let inputs = vec![
            (MatchKind::Fts, vec![h("a", 1.0), h("b", 0.5)]),
            (MatchKind::Semantic, vec![h("a", 0.9), h("c", 0.8)]),
            (MatchKind::Fuzzy, vec![h("a", 100.0)]),
        ];
        let out = fuse(&inputs, RRF_K);
        assert_eq!(out[0].chunk_id, "a");
        assert_eq!(out[0].match_kinds.len(), 3);
    }

    #[test]
    fn single_source_hit_is_included() {
        let inputs = vec![
            (MatchKind::Fts, vec![h("a", 1.0)]),
            (MatchKind::Semantic, vec![h("b", 0.5)]),
            (MatchKind::Fuzzy, vec![]),
        ];
        let out = fuse(&inputs, RRF_K);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn fusion_ordering_matches_rank_sum() {
        // a: rank 1 in fts (1/61) + rank 3 in sem (1/63) = 0.0164 + 0.0159 = 0.0323
        // b: rank 1 in sem (1/61) = 0.0164
        let inputs = vec![
            (MatchKind::Fts, vec![h("a", 1.0), h("x", 0.9), h("y", 0.8)]),
            (MatchKind::Semantic, vec![h("b", 1.0), h("c", 0.9), h("a", 0.8)]),
            (MatchKind::Fuzzy, vec![]),
        ];
        let out = fuse(&inputs, RRF_K);
        assert_eq!(out[0].chunk_id, "a");
    }
}
```

- [ ] **Step 2: Register in `search/mod.rs`**

Add `pub mod fusion;` to `frontend/src-tauri/src/search/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cd frontend/src-tauri && cargo test --lib search::fusion`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/search/fusion.rs frontend/src-tauri/src/search/mod.rs
git commit -m "search: implement Reciprocal Rank Fusion over the three rankings"
```

---

### Task 10: HybridSearchEngine orchestrator

**Files:**
- Create: `frontend/src-tauri/src/search/engine.rs`
- Modify: `frontend/src-tauri/src/search/mod.rs`

- [ ] **Step 1: Write the engine**

Write to `frontend/src-tauri/src/search/engine.rs`:

```rust
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
```

- [ ] **Step 2: Register in `search/mod.rs`**

Add `pub mod engine;` and re-export `pub use engine::HybridSearchEngine;`.

- [ ] **Step 3: Compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/search/engine.rs frontend/src-tauri/src/search/mod.rs
git commit -m "search: implement HybridSearchEngine orchestrating FTS/semantic/fuzzy with RRF"
```

---

## Phase 6 — Indexer + Migration backfill

### Task 11: Indexer — extract chunks from meeting data and persist

**Files:**
- Create: `frontend/src-tauri/src/search/indexer.rs`
- Modify: `frontend/src-tauri/src/search/mod.rs`

- [ ] **Step 1: Write the indexer**

Write to `frontend/src-tauri/src/search/indexer.rs`:

```rust
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

    // 3. meeting_notes table (added in migration 20251223000000_add_meeting_notes.sql)
    let notes = sqlx::query("SELECT id, content FROM meeting_notes WHERE meeting_id = ?1")
        .bind(meeting_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    for r in notes {
        let nid: String = r.try_get("id").unwrap_or_default();
        let content: String = r.try_get("content").unwrap_or_default();
        all.extend(chunk_text(meeting_id, SourceType::Notes, Some(&nid), &content, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP));
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
```

- [ ] **Step 2: Register in `search/mod.rs`**

Add `pub mod indexer;`.

- [ ] **Step 3: Compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/search/indexer.rs frontend/src-tauri/src/search/mod.rs
git commit -m "search: add indexer that chunks + embeds a single meeting"
```

---

### Task 12: Background backfill migration (M1)

**Files:**
- Create: `frontend/src-tauri/src/search/migration.rs`
- Modify: `frontend/src-tauri/src/search/mod.rs`

- [ ] **Step 1: Write migration backfill**

Write to `frontend/src-tauri/src/search/migration.rs`:

```rust
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

/// Backfill embeddings for every meeting that isn't yet indexed with the current model.
/// Emits `semantic-indexing-progress` events for the UI.
pub async fn backfill<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    cache: Arc<EmbeddingCache>,
) -> Result<()> {
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
```

- [ ] **Step 2: Register in `search/mod.rs`**

Add `pub mod migration;`.

- [ ] **Step 3: Compile**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/search/migration.rs frontend/src-tauri/src/search/mod.rs
git commit -m "search: add background backfill migration with progress events"
```

---

## Phase 7 — Tauri commands, lib.rs wiring, old code removal

### Task 13: Tauri commands + engine state + lib.rs registration + ModelStatus field

**Files:**
- Create: `frontend/src-tauri/src/search/commands.rs`
- Modify: `frontend/src-tauri/src/search/mod.rs`
- Modify: `frontend/src-tauri/src/lib.rs` (setup + invoke_handler)
- Modify: `frontend/src-tauri/src/onboarding.rs` (ModelStatus)

- [ ] **Step 1: Write commands**

Write to `frontend/src-tauri/src/search/commands.rs`:

```rust
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime, State};
use tokio::sync::RwLock;

use crate::search::embedder;
use crate::search::engine::HybridSearchEngine;
use crate::search::migration;
use crate::search::searchers::semantic::EmbeddingCache;
use crate::search::types::{IndexingStatus, SearchHit};

pub struct SearchState {
    pub engine: RwLock<Option<Arc<HybridSearchEngine>>>,
    pub cache: Arc<EmbeddingCache>,
    pub pool: sqlx::SqlitePool,
}

impl SearchState {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        let cache = EmbeddingCache::new();
        Self { engine: RwLock::new(None), cache, pool }
    }

    pub async fn init_engine(&self) {
        let mut guard = self.engine.write().await;
        if guard.is_none() {
            let _ = self.cache.reload(&self.pool).await;
            *guard = Some(Arc::new(HybridSearchEngine::new(self.pool.clone(), self.cache.clone())));
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct ModelDownloadProgress {
    pub progress: f32,     // 0..100
    pub status: String,    // "downloading" | "completed" | "error"
    pub message: Option<String>,
}

#[tauri::command]
pub async fn search_meetings(
    state: State<'_, Arc<SearchState>>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<SearchHit>, String> {
    state.init_engine().await;
    let guard = state.engine.read().await;
    let engine = guard.as_ref().ok_or_else(|| "engine not initialized".to_string())?;
    engine
        .search(&query, limit.unwrap_or(20) as usize)
        .await
        .map_err(|e| format!("{:?}", e))
}

#[tauri::command]
pub async fn get_indexing_status(
    state: State<'_, Arc<SearchState>>,
) -> Result<IndexingStatus, String> {
    migration::current_status(&state.pool).await.map_err(|e| format!("{:?}", e))
}

#[tauri::command]
pub async fn reindex_all<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<SearchState>>,
) -> Result<(), String> {
    // Wipe state so everything is re-considered pending
    sqlx::query("UPDATE indexing_state SET status='pending'")
        .execute(&state.pool)
        .await
        .map_err(|e| format!("{:?}", e))?;
    let pool = state.pool.clone();
    let cache = state.cache.clone();
    tokio::spawn(async move {
        if let Err(e) = migration::backfill(app, pool, cache).await {
            log::error!("reindex_all failed: {:?}", e);
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn semantic_model_is_ready() -> Result<bool, String> {
    Ok(embedder::is_model_cached())
}

#[tauri::command]
pub async fn semantic_model_download<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let _ = app.emit(
        "semantic-model-download-progress",
        ModelDownloadProgress { progress: 0.0, status: "downloading".into(), message: None },
    );
    match embedder::ensure_embedder().await {
        Ok(_) => {
            let _ = app.emit(
                "semantic-model-download-complete",
                ModelDownloadProgress { progress: 100.0, status: "completed".into(), message: None },
            );
            Ok(())
        }
        Err(e) => {
            let msg = format!("{:?}", e);
            let _ = app.emit(
                "semantic-model-download-error",
                ModelDownloadProgress { progress: 0.0, status: "error".into(), message: Some(msg.clone()) },
            );
            Err(msg)
        }
    }
}
```

- [ ] **Step 2: Export commands in `search/mod.rs`**

Update `frontend/src-tauri/src/search/mod.rs`:

```rust
pub mod chunker;
pub mod commands;
pub mod embedder;
pub mod engine;
pub mod fusion;
pub mod indexer;
pub mod migration;
pub mod searchers;
pub mod types;

pub use commands::SearchState;
pub use engine::HybridSearchEngine;
pub use types::{
    Chunk, IndexingStatus, MatchKind, RankedHit, SearchHit, SourceType,
    EMBEDDING_DIM, EMBEDDING_MODEL_ID,
};
```

- [ ] **Step 3: Add `semantic_model` field to ModelStatus**

Edit `frontend/src-tauri/src/onboarding.rs` (lines 20-24):

```rust
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModelStatus {
    pub parakeet: String,
    pub summary: String,
    #[serde(default)]
    pub semantic_model: String,   // "downloaded" | "not_downloaded" | "downloading"
}
```

And update the `Default for OnboardingStatus` (lines 26-39) to set `semantic_model: "not_downloaded".to_string()` in the struct literal.

- [ ] **Step 4: Register `SearchState` and commands in `lib.rs`**

Edit `frontend/src-tauri/src/lib.rs`:

a) In the `setup` closure (near the top of `tauri::Builder::default()`, where other states are managed), after the SQLite pool is available, add:

```rust
// Semantic search state
use std::sync::Arc;
let search_state = Arc::new(crate::search::SearchState::new(pool.clone()));
app.manage(search_state.clone());

// Kick off background backfill (non-blocking)
let app_handle = app.handle().clone();
let pool_bg = pool.clone();
let cache_bg = search_state.cache.clone();
tokio::spawn(async move {
    if let Err(e) = crate::search::migration::backfill(app_handle, pool_bg, cache_bg).await {
        log::error!("initial semantic backfill failed: {:?}", e);
    }
});
```

> Note: the exact variable name of the pool may differ — reuse whatever variable currently holds the SqlitePool from the DB setup step.

b) In the `invoke_handler![...]` block (line 495), append:

```rust
            // Semantic search commands
            search::commands::search_meetings,
            search::commands::get_indexing_status,
            search::commands::reindex_all,
            search::commands::semantic_model_is_ready,
            search::commands::semantic_model_download,
```

- [ ] **Step 5: Compile and lint**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles. Warnings only (unused variables during integration are acceptable).

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/search/commands.rs \
        frontend/src-tauri/src/search/mod.rs \
        frontend/src-tauri/src/onboarding.rs \
        frontend/src-tauri/src/lib.rs
git commit -m "search: expose Tauri commands + register SearchState + add semantic_model onboarding field"
```

---

### Task 14: Remove old `api_search_meetings` (Python HTTP) path

**Files:**
- Modify: `frontend/src-tauri/src/api/api.rs` (around lines 20-60)
- Modify: `frontend/src-tauri/src/lib.rs` (remove `api::api_search_meetings` from invoke_handler)
- Modify: `frontend/src/hooks/useSearchMeetings.ts`

- [ ] **Step 1: Delete `api_search_meetings` in `api.rs`**

Edit `frontend/src-tauri/src/api/api.rs`. Locate the `api_search_meetings` function (it's the Tauri command that POSTs to `/search-meetings` on localhost:5167, with the SQLite LIKE fallback). Delete:
- the function definition
- its related struct definitions used only by it (if any, e.g., `SearchMeetingResult`, `SearchMatch` — verify they're not used elsewhere with `grep -rn SearchMeetingResult frontend/src-tauri/src`)

- [ ] **Step 2: Remove its registration in `lib.rs`**

In `frontend/src-tauri/src/lib.rs` at line 580, remove the line:

```rust
            api::api_search_meetings,
```

- [ ] **Step 3: Update the frontend hook**

Edit `frontend/src/hooks/useSearchMeetings.ts` entirely to match the new Rust types:

```ts
'use client';

import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

export type SourceType =
  | 'transcript' | 'title' | 'summary' | 'action_items' | 'key_points' | 'notes';

export type MatchKind = 'fts' | 'semantic' | 'fuzzy';

export interface SearchHit {
  meeting_id: string;
  meeting_title: string;
  source_type: SourceType;
  chunk_text: string;
  char_start: number | null;
  char_end: number | null;
  score: number;
  match_kinds: MatchKind[];
}

interface UseSearchMeetingsReturn {
  query: string;
  results: SearchHit[];
  isSearching: boolean;
  search: (query: string) => void;
  clearSearch: () => void;
}

export function useSearchMeetings(): UseSearchMeetingsReturn {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchHit[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const debounceRef = useRef<NodeJS.Timeout | null>(null);

  const search = useCallback((searchQuery: string) => {
    setQuery(searchQuery);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    if (!searchQuery.trim()) {
      setResults([]);
      setIsSearching(false);
      return;
    }
    setIsSearching(true);
    debounceRef.current = setTimeout(async () => {
      try {
        const hits = await invoke<SearchHit[]>('search_meetings', {
          query: searchQuery,
          limit: 20,
        });
        setResults(hits);
      } catch (error) {
        console.error('Search failed:', error);
        setResults([]);
      } finally {
        setIsSearching(false);
      }
    }, 300);
  }, []);

  const clearSearch = useCallback(() => {
    setQuery('');
    setResults([]);
    setIsSearching(false);
    if (debounceRef.current) clearTimeout(debounceRef.current);
  }, []);

  return { query, results, isSearching, search, clearSearch };
}
```

- [ ] **Step 4: Build — catch any consumer that still uses the old types**

Run: `cd frontend && pnpm install && pnpm run build` (or `pnpm run dev` for faster feedback)
Expected: TypeScript may flag consumers of the old `SearchMeetingResult`/`matches`. Update each consumer to use the new `SearchHit` shape. Common call sites to check:
- `frontend/src/app/meetings/page.tsx`
- any component rendering search results

Fix these call sites inline until `pnpm run build` succeeds.

- [ ] **Step 5: Run Rust build to confirm nothing dangles**

Run: `cd frontend/src-tauri && cargo check`
Expected: compiles clean.

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/api/api.rs \
        frontend/src-tauri/src/lib.rs \
        frontend/src/hooks/useSearchMeetings.ts \
        frontend/src/app/meetings/page.tsx
git commit -m "search: remove Python backend search path and switch frontend to new Tauri command"
```

---

## Phase 8 — Onboarding + meetings UI

### Task 15: Add 3rd download card for the semantic model

**Files:**
- Modify: `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx`
- Modify: `frontend/src/contexts/OnboardingContext.tsx`

- [ ] **Step 1: Add state + listeners in `OnboardingContext.tsx`**

Search for `summaryModelDownloaded` in the file; replicate the same pattern for `semanticModelDownloaded`:

a) Add to context state:

```ts
const [semanticModelDownloaded, setSemanticModelDownloaded] = useState(false);
```

b) Add to context value returned by the provider, next to `summaryModelDownloaded`.

c) In `startBackgroundDownloads` (around line 441), after the `builtin_ai_download_model` invocation, add a parallel invocation:

```ts
// Also download semantic search model in background
invoke('semantic_model_download').catch((e) => {
  console.error('semantic model download failed:', e);
});
```

d) Add event listeners for `semantic-model-download-progress`, `semantic-model-download-complete`, `semantic-model-download-error`. Follow the same pattern as the Parakeet/Gemma listeners; toggle `setSemanticModelDownloaded(true)` on `complete`.

- [ ] **Step 2: Add 3rd download card in `DownloadProgressStep.tsx`**

In `DownloadProgressStep.tsx`:

a) Add local state similar to `parakeetState`/`gemmaState`:

```ts
const [semanticState, setSemanticState] = useState<DownloadState>({
  status: semanticModelDownloaded ? 'completed' : 'waiting',
  progress: semanticModelDownloaded ? 100 : 0,
  downloadedMb: 0,
  totalMb: 470,
  speedMbps: 0,
});
```

b) Listen to the three `semantic-model-download-*` Tauri events; update `semanticState` in the same way Parakeet/Gemma listeners do.

c) Add a third card below Parakeet and Gemma with the same visual structure. Use `Sparkles` or an existing icon; label "Moteur de recherche sémantique — 470 MB".

d) Ensure it does **not** block the "Continuer" button (it's non-blocking, like Gemma).

- [ ] **Step 3: Manual verification**

Run: `cd frontend && pnpm run tauri:dev`
Manually reset onboarding (via existing reset mechanism or wipe `onboarding-status.json`), re-run onboarding, confirm the 3rd card appears and downloads progress.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/onboarding/steps/DownloadProgressStep.tsx \
        frontend/src/contexts/OnboardingContext.tsx
git commit -m "onboarding: add semantic search model download card"
```

---

### Task 16: Indexing progress banner + match_kinds badges in meetings list

**Files:**
- Modify: `frontend/src/app/meetings/page.tsx`

- [ ] **Step 1: Listen to `semantic-indexing-progress` events in the page**

In `frontend/src/app/meetings/page.tsx`, near the top of the component function, add:

```tsx
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

type IndexingStatus = {
  total_meetings: number;
  indexed_meetings: number;
  chunks_total: number;
  chunks_done: number;
  in_progress: boolean;
};

const [indexingStatus, setIndexingStatus] = useState<IndexingStatus | null>(null);

useEffect(() => {
  let unlisten: (() => void) | undefined;
  (async () => {
    // Prime initial status
    try {
      const s = await invoke<IndexingStatus>('get_indexing_status');
      setIndexingStatus(s);
    } catch (e) { console.warn(e); }

    unlisten = await listen<{ processed: number; total: number; done: boolean }>(
      'semantic-indexing-progress',
      (ev) => {
        setIndexingStatus((prev) => prev ? ({
          ...prev,
          indexed_meetings: ev.payload.processed,
          total_meetings: ev.payload.total,
          in_progress: !ev.payload.done,
        }) : null);
      }
    );
  })();
  return () => { unlisten?.(); };
}, []);
```

- [ ] **Step 2: Render the indexing banner under the search input**

Add directly beneath the existing search input JSX:

```tsx
{indexingStatus?.in_progress && (
  <div className="mt-2 px-3 py-2 text-xs rounded-md bg-muted/50 text-muted-foreground flex items-center gap-2">
    <Loader2 className="w-3 h-3 animate-spin" />
    <span>
      Indexation sémantique : {indexingStatus.indexed_meetings}/{indexingStatus.total_meetings} meetings
    </span>
  </div>
)}
```

(Import `Loader2` from `lucide-react` if not already imported.)

- [ ] **Step 3: Render match_kinds badges on each result**

Locate where search results are rendered (loop over `results`). For each hit, add badges alongside the existing meeting title/snippet:

```tsx
<div className="flex gap-1 mt-1">
  {hit.match_kinds.includes('fts') && <Badge variant="outline" className="text-[10px]">exact</Badge>}
  {hit.match_kinds.includes('semantic') && <Badge variant="outline" className="text-[10px]">sémantique</Badge>}
  {hit.match_kinds.includes('fuzzy') && <Badge variant="outline" className="text-[10px]">fuzzy</Badge>}
</div>
```

(Use the `Badge` component from `@/components/ui/badge` if it exists; otherwise inline a styled `<span>`.)

- [ ] **Step 4: Verify UI in browser**

Run: `cd frontend && pnpm run tauri:dev`
- With at least 2 meetings, type a query.
- Confirm badges display, and the banner appears during background indexation.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/app/meetings/page.tsx
git commit -m "meetings: add indexing banner and match_kinds badges to search results"
```

---

## Phase 9 — Tests

### Task 17: Integration test — French semantic match

**Files:**
- Create: `frontend/src-tauri/tests/semantic_search_fr.rs`

- [ ] **Step 1: Write the integration test**

Write to `frontend/src-tauri/tests/semantic_search_fr.rs`:

```rust
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
```

- [ ] **Step 2: Run it (requires internet on first run to download the model)**

Run: `cd frontend/src-tauri && cargo test --test semantic_search_fr -- --ignored --nocapture`
Expected: passes. First run downloads the model (~470 MB) into the cache dir.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/tests/semantic_search_fr.rs
git commit -m "test: add French paraphrase integration test for hybrid search"
```

---

## Final self-review

Before wrapping up, quickly verify:

- [ ] All Tauri commands registered in `lib.rs` match the command functions defined in `search/commands.rs`.
- [ ] `SourceType` string values in Rust (`as_str`) match those used by the migration (`source_type TEXT`).
- [ ] Frontend `SearchHit` TypeScript shape matches Rust `SearchHit` serialization (snake_case fields, lowercase enum values).
- [ ] `multilingual-e5-small@v1` constant is used consistently: migration doesn't hard-code it but `EMBEDDING_MODEL_ID` does, and both embedder and migration backfill reference it.
- [ ] `cargo build --release` succeeds on the current target.
- [ ] `pnpm run build` succeeds in `frontend/`.
- [ ] Manual smoke test: onboard from scratch, record a French meeting, search a paraphrased phrase, confirm it's returned with both `fts`/`semantic` or `semantic`-only badges.
