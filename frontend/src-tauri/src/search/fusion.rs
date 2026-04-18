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
