// rapidfuzz 0.5 exposes `fuzz::ratio(iter1, iter2) -> f64` (Levenshtein ratio, 0..1).
// We build a per-token best-match scorer on top of it so the fuzzy layer actually
// rescues typos like "amzaon" → "amazon" (whereas a whole-string sort+ratio
// comparison drowns a 6-char query in a 200-char chunk).
use rapidfuzz::fuzz::ratio;

use crate::search::types::{Chunk, RankedHit};

pub const FUZZY_THRESHOLD: f32 = 70.0;

fn strip_punctuation_lower(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// For a single query token, return the highest Levenshtein ratio against any
/// whitespace-separated token in the chunk. Scores are in 0..1.
fn best_token_ratio(query_token: &str, chunk_tokens_clean: &[String]) -> f64 {
    if query_token.is_empty() || chunk_tokens_clean.is_empty() {
        return 0.0;
    }
    chunk_tokens_clean
        .iter()
        .map(|ct| ratio(query_token.chars(), ct.chars()))
        .fold(0.0_f64, f64::max)
}

/// Score a chunk against a multi-token query: mean of each query token's best
/// match in the chunk, scaled to 0..100 to preserve the documented threshold.
/// "amzaon" vs a chunk containing "amazon" → ratio ≈ 0.83 → ~83 → passes 70.
pub fn score_chunk(query_tokens_clean: &[String], chunk_text_raw: &str) -> f32 {
    if query_tokens_clean.is_empty() {
        return 0.0;
    }
    let chunk_tokens: Vec<String> = chunk_text_raw
        .split_whitespace()
        .map(strip_punctuation_lower)
        .filter(|t| !t.is_empty())
        .collect();
    let sum: f64 = query_tokens_clean
        .iter()
        .map(|qt| best_token_ratio(qt, &chunk_tokens))
        .sum();
    ((sum / query_tokens_clean.len() as f64) * 100.0) as f32
}

/// Rescore `candidates` against `query`. Caller controls the pool — engine.rs
/// now passes *all* chunks so typo queries (which produce zero FTS hits) still
/// get a chance to surface the real match.
pub fn search(query: &str, candidates: &[Chunk], limit: usize) -> Vec<RankedHit> {
    let q_tokens: Vec<String> = query
        .split_whitespace()
        .map(strip_punctuation_lower)
        .filter(|t| !t.is_empty())
        .collect();
    if q_tokens.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<RankedHit> = candidates
        .iter()
        .map(|c| RankedHit {
            chunk_id: c.id.clone(),
            score: score_chunk(&q_tokens, &c.chunk_text),
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
        assert!(hits[0].score >= 85.0, "typos should still score high: {}", hits[0].score);
    }

    #[test]
    fn amazon_typo_finds_long_chunk() {
        // Regression: with the old whole-string sort+ratio the short query was
        // drowned by the surrounding chunk text and never crossed threshold.
        let cands = [mk(
            "a",
            "Bonjour, on était au travail. Amazon nous a fait un remboursement aujourd'hui.",
        )];
        let hits = search("amzaon", &cands, 10);
        assert_eq!(hits.len(), 1, "typo 'amzaon' should still surface the Amazon chunk");
        assert!(hits[0].score >= 75.0, "expected strong match, got {}", hits[0].score);
    }

    #[test]
    fn punctuation_in_chunk_does_not_block_match() {
        // Words in transcripts often carry trailing punctuation; strip before compare.
        let cands = [mk("a", "on parle d'Amazon, puis de Fnac!")];
        let hits = search("amazon", &cands, 10);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn unrelated_text_is_filtered_out() {
        let cands = [mk("a", "completely unrelated sentence")];
        let hits = search("pricing decision", &cands, 10);
        assert!(hits.is_empty());
    }
}
