pub mod chunker;
pub mod types;

pub use chunker::{chunk_text, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP};
pub use types::{
    Chunk, IndexingStatus, MatchKind, RankedHit, SearchHit, SourceType,
    EMBEDDING_DIM, EMBEDDING_MODEL_ID,
};
