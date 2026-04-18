pub mod chunker;
pub mod commands;
pub mod embedder;
pub mod engine;
pub mod fusion;
pub mod indexer;
pub mod migration;
pub mod searchers;
pub mod types;

pub use chunker::{chunk_text, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP};
pub use commands::SearchState;
pub use engine::HybridSearchEngine;
pub use types::{
    Chunk, IndexingStatus, MatchKind, RankedHit, SearchHit, SourceType,
    EMBEDDING_DIM, EMBEDDING_MODEL_ID,
};

use std::sync::Arc;
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, Runtime};

/// Register `SearchState` with the Tauri app and spawn the semantic backfill task.
/// Safe to call multiple times — subsequent calls are no-ops (the `try_state` guard
/// prevents double-spawning the backfill).
pub fn attach_to_app<R: Runtime>(app: &AppHandle<R>, pool: SqlitePool) {
    if app.try_state::<Arc<SearchState>>().is_some() {
        log::debug!("SearchState already attached; skipping");
        return;
    }

    let search_state = Arc::new(SearchState::new(pool.clone()));
    app.manage(search_state.clone());

    let app_for_backfill = app.clone();
    let cache_for_backfill = search_state.cache.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = migration::backfill(app_for_backfill, pool, cache_for_backfill).await {
            log::error!("semantic backfill failed: {:?}", e);
        }
    });
    log::info!("Semantic search state attached + backfill spawned");
}
