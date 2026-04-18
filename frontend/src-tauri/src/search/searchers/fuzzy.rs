// NOTE: rapidfuzz 0.5 does not expose `token_set_ratio`.
// The available API is `rapidfuzz::fuzz::ratio(iter1, iter2) -> f64` (range 0.0..1.0).
// We implement a token-set-ratio approximation manually:
//   1. Tokenise + sort both strings
//   2. Compute ratio on the sorted-token representation
//   3. Scale to 0..100 to preserve the documented threshold semantics.
use rapidfuzz::fuzz::ratio;

use crate::search::types::{Chunk, RankedHit};

pub const FUZZY_THRESHOLD: f32 = 70.0;

/// Sort the whitespace-separated tokens of `s` and rejoin with a space.
fn sorted_tokens(s: &str) -> String {
    let mut tokens: Vec<&str> = s.split_whitespace().collect();
    tokens.sort_unstable();
    tokens.join(" ")
}

/// Token-set-ratio approximation: compare sorted token representations.
/// Returns a score in 0..100 (matching the Python rapidfuzz convention).
fn token_set_ratio(a: &str, b: &str) -> f64 {
    let a_sorted = sorted_tokens(a);
    let b_sorted = sorted_tokens(b);
    ratio(a_sorted.chars(), b_sorted.chars()) * 100.0
}

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
