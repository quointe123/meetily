// rapidfuzz 0.5 exposes `fuzz::ratio(iter1, iter2) -> f64` (Levenshtein ratio, 0..1).
// We build a per-token best-match scorer on top of it so the fuzzy layer actually
// rescues typos like "amzaon" → "amazon" (whereas a whole-string sort+ratio
// comparison drowns a 6-char query in a 200-char chunk).
use rapidfuzz::fuzz::ratio;

use crate::search::searchers::fts::is_stopword;
use crate::search::types::{Chunk, RankedHit};

pub const FUZZY_THRESHOLD: f32 = 70.0;

fn strip_punctuation_lower(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Minimum char count for *both* sides of a fuzzy comparison. 3-char tokens
/// like "oui" / "not" / "dev" score deceptively high against 5-6 char queries
/// ("louis" vs "oui" → ratio 0.75, above threshold) purely because the sum of
/// lengths is tiny. Short queries should lean on FTS prefix match, not fuzzy.
const MIN_FUZZY_LEN: usize = 4;
/// Max absolute length difference for a pair to be considered a fuzzy candidate.
/// "amazon" (6) ↔ "amazons" (7) passes; "louis" (5) ↔ "oui" (3) doesn't.
const MAX_FUZZY_LEN_DIFF: usize = 3;

/// For a single query token, return the highest Levenshtein ratio against any
/// whitespace-separated token in the chunk. Scores are in 0..1.
///
/// Short tokens (< MIN_FUZZY_LEN on either side) are only allowed to match
/// *exactly* — their fuzzy ratios are dominated by length not semantics.
/// "on" still scores 1.0 against a chunk containing "on", but "oui" can no
/// longer fuzzy-match "louis".
fn best_token_ratio(query_token: &str, chunk_tokens_clean: &[String]) -> f64 {
    if query_token.is_empty() || chunk_tokens_clean.is_empty() {
        return 0.0;
    }
    let q_len = query_token.chars().count();
    let exact_present = chunk_tokens_clean.iter().any(|ct| ct == query_token);
    if q_len < MIN_FUZZY_LEN {
        return if exact_present { 1.0 } else { 0.0 };
    }
    let fuzzy_best = chunk_tokens_clean
        .iter()
        .filter(|ct| {
            let c_len = ct.chars().count();
            if c_len < MIN_FUZZY_LEN {
                return false; // exact-match already handled above
            }
            let diff = if q_len > c_len { q_len - c_len } else { c_len - q_len };
            diff <= MAX_FUZZY_LEN_DIFF
        })
        .map(|ct| ratio(query_token.chars(), ct.chars()))
        .fold(0.0_f64, f64::max);
    if exact_present { 1.0_f64.max(fuzzy_best) } else { fuzzy_best }
}

/// Score a chunk against a multi-token query: mean of each content token's
/// best match in the chunk, scaled to 0..100. Stopwords are dropped from the
/// averaging so a query like "Louis et Quentin" doesn't inflate its score on
/// chunks that happen to contain "et" while missing the actual names.
/// If the whole query is stopwords we keep them as a fallback.
pub fn score_chunk(query_tokens_clean: &[String], chunk_text_raw: &str) -> f32 {
    if query_tokens_clean.is_empty() {
        return 0.0;
    }
    let content_tokens: Vec<&String> = query_tokens_clean
        .iter()
        .filter(|t| !is_stopword(t))
        .collect();
    let tokens_for_scoring: Vec<&String> = if content_tokens.is_empty() {
        query_tokens_clean.iter().collect()
    } else {
        content_tokens
    };
    let chunk_tokens: Vec<String> = chunk_text_raw
        .split_whitespace()
        .map(strip_punctuation_lower)
        .filter(|t| !t.is_empty())
        .collect();
    let sum: f64 = tokens_for_scoring
        .iter()
        .map(|qt| best_token_ratio(qt, &chunk_tokens))
        .sum();
    ((sum / tokens_for_scoring.len() as f64) * 100.0) as f32
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

    #[test]
    fn short_chunk_token_does_not_fuzzy_match_longer_query() {
        // Regression: "Louis" query used to match chunks containing only "Oui"
        // (ratio 0.75, above threshold). Now the length guard rejects the pair
        // before ratio is even computed.
        let cands = [mk("a", "Oui oui oui. Ok. Non.")];
        let hits = search("Louis", &cands, 10);
        assert!(hits.is_empty(), "Oui/Ok/Non must not match Louis");

        // Reverse: short query shouldn't fuzzy-match at all (FTS handles prefix).
        let cands = [mk("b", "développement et coordination")];
        let hits = search("dev", &cands, 10);
        assert!(hits.is_empty(), "3-char query 'dev' must not fuzzy-match");
    }

    #[test]
    fn stopwords_in_query_do_not_inflate_chunk_scores() {
        // "Louis et Quentin" vs a chunk that only contains "et" (plus unrelated
        // words) used to score high because "et" matched perfectly and averaged
        // up with the 0-scores from the two names. Now stopwords are dropped
        // from scoring, so this chunk falls below threshold.
        let cands = [mk(
            "a",
            "ce qui est autour qu'on a fait et sert pour les clients",
        )];
        let hits = search("Louis et Quentin", &cands, 10);
        assert!(
            hits.is_empty(),
            "stopword-only match must not pull the chunk above threshold"
        );
    }

    #[test]
    fn same_length_bucket_still_tolerates_typos() {
        // Sanity: the length guard must not break legitimate typo recovery.
        let cands = [
            mk("a", "the amazon warehouse"),
            mk("b", "livraison urgente"),
        ];
        assert!(!search("amzaon", &cands, 10).is_empty());
        assert!(!search("livrison", &cands, 10).is_empty());
    }
}
