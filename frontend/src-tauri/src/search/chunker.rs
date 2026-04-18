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
