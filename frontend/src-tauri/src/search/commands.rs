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
