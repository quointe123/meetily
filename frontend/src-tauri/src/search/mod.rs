pub mod chunker;
pub mod embedder;
pub mod engine;
pub mod fusion;
pub mod indexer;
pub mod searchers;
pub mod types;

pub use chunker::{chunk_text, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP};
pub use engine::HybridSearchEngine;
pub use types::{
    Chunk, IndexingStatus, MatchKind, RankedHit, SearchHit, SourceType,
    EMBEDDING_DIM, EMBEDDING_MODEL_ID,
};
