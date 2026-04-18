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
