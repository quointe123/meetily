use anyhow::Result;
use sqlx::{Row, SqlitePool};

use crate::search::types::RankedHit;

/// Common French + English stopwords. Kept compact on purpose — these are the tokens
/// that (a) appear in almost every chunk and (b) carry no discriminative signal.
/// Matching is done on the lowercased token with diacritics preserved, so the entries
/// must match the raw text (e.g. "à", "où").
const STOPWORDS: &[&str] = &[
    // French articles / determiners
    "le", "la", "les", "l", "un", "une", "des", "de", "du", "d",
    // French conjunctions / prepositions
    "et", "ou", "mais", "donc", "car", "ni", "or",
    "à", "au", "aux", "en", "dans", "sur", "sous", "pour", "par", "avec", "sans",
    "chez", "vers", "entre", "depuis",
    // French pronouns
    "je", "tu", "il", "elle", "on", "nous", "vous", "ils", "elles",
    "me", "te", "se", "lui", "leur", "y",
    "ce", "cet", "cette", "ces", "ça", "cela",
    "qui", "que", "quoi", "dont", "où",
    // French common verbs / particles
    "est", "sont", "es", "ai", "as", "a", "ont", "avons", "avez",
    "suis", "êtes", "été", "être",
    "ne", "pas", "plus", "si",
    // English articles / conjunctions / prepositions
    "the", "a", "an", "and", "or", "but",
    "of", "to", "in", "on", "at", "for", "by", "with", "from", "into", "as",
    // English pronouns / copulas
    "is", "are", "was", "were", "be", "been", "being",
    "i", "you", "he", "she", "it", "we", "they",
    "this", "that", "these", "those",
    "not", "no",
];

fn is_stopword(token: &str) -> bool {
    let lower = token.to_lowercase();
    STOPWORDS.iter().any(|sw| *sw == lower)
}

/// Escape a user query for FTS5 MATCH:
/// - split on non-alphanumeric (matching how FTS5's `unicode61` tokenizer behaves
///   on the indexed side). This strips quotes, parens, hyphens, etc. so queries
///   like `"pricing"` or `marché-client` produce valid MATCH expressions.
/// - drop stopwords so common words ("et", "the") don't dilute the signal
/// - wrap each remaining token in double quotes to disable operator parsing
/// - append `*` to the last token for prefix match
/// - join with explicit `OR` so BM25 naturally ranks chunks that match rarer tokens
///   (proper nouns, topic words) above chunks that match only common tokens
pub fn escape_fts_query(query: &str) -> String {
    let all_tokens: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    if all_tokens.is_empty() {
        return String::new();
    }

    // Filter stopwords. If *all* tokens are stopwords (rare: the user typed only
    // "et" or "the"), keep them rather than returning an empty query.
    let filtered: Vec<&str> = all_tokens.iter().copied().filter(|t| !is_stopword(t)).collect();
    let tokens: Vec<&str> = if filtered.is_empty() { all_tokens } else { filtered };

    // Tokens are pure alphanumeric after splitting, so the replace is defensive
    // rather than strictly needed — keep it in case future changes loosen the split.
    let mut parts: Vec<String> = tokens
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if let Some(last) = parts.last_mut() {
        if last.ends_with('"') {
            last.pop();
            last.push_str("\"*");
        }
    }
    parts.join(" OR ")
}

/// Search FTS5 with BM25 ranking. Returns top `limit` chunk ids with their BM25 score
/// (lower is better, so we negate to keep "higher = better" convention).
pub async fn search(pool: &SqlitePool, query: &str, limit: usize) -> Result<Vec<RankedHit>> {
    let match_expr = escape_fts_query(query);
    if match_expr.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT c.id AS chunk_id, bm25(search_chunks_fts) AS bm25_score
        FROM search_chunks_fts
        JOIN search_chunks c ON c.rowid = search_chunks_fts.rowid
        WHERE search_chunks_fts MATCH ?1
        ORDER BY bm25_score
        LIMIT ?2
        "#,
    )
    .bind(&match_expr)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| RankedHit {
            chunk_id: r.get::<String, _>("chunk_id"),
            score: -r.get::<f64, _>("bm25_score") as f32,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_joins_tokens_with_or_and_adds_prefix_star() {
        assert_eq!(escape_fts_query("hello world"), "\"hello\" OR \"world\"*");
    }

    #[test]
    fn escape_handles_empty() {
        assert_eq!(escape_fts_query(""), "");
        assert_eq!(escape_fts_query("   "), "");
    }

    #[test]
    fn escape_escapes_embedded_double_quotes() {
        // Quotes now get stripped during tokenization rather than escaped into the MATCH
        // expression. Either behavior keeps us safe from FTS5 syntax errors; this one
        // also produces a query that actually matches chunks.
        assert_eq!(escape_fts_query("say \"hi\""), "\"say\" OR \"hi\"*");
    }

    #[test]
    fn special_chars_are_stripped_not_escaped() {
        // Before this change, "\"pricing\"" produced a malformed MATCH that
        // silently matched nothing. Now it yields a clean token.
        assert_eq!(escape_fts_query("\"pricing\""), "\"pricing\"*");
        assert_eq!(escape_fts_query("(décisions)"), "\"décisions\"*");
        // Hyphenated queries split like FTS5 would on the indexed side.
        assert_eq!(escape_fts_query("marché-client"), "\"marché\" OR \"client\"*");
        // Trailing punctuation no longer leaks into the token.
        assert_eq!(escape_fts_query("amazon?"), "\"amazon\"*");
    }

    #[test]
    fn apostrophe_contractions_split_into_stopword_plus_content_word() {
        // "l'équipe" → tokens (l, équipe). "l" is a stopword → dropped. Left with "équipe".
        assert_eq!(escape_fts_query("l'équipe"), "\"équipe\"*");
        assert_eq!(escape_fts_query("d'accord"), "\"accord\"*");
    }

    #[test]
    fn stopwords_are_dropped_so_proper_nouns_drive_the_match() {
        // Regression: "Louis et Quentin" used to produce `"Louis" "et" "Quentin"*`
        // which required all three (AND) and also let "et" dilute BM25 on fallback paths.
        assert_eq!(escape_fts_query("Louis et Quentin"), "\"Louis\" OR \"Quentin\"*");
        assert_eq!(escape_fts_query("the pricing decision"), "\"pricing\" OR \"decision\"*");
    }

    #[test]
    fn all_stopword_query_is_kept_rather_than_emptied() {
        // If the user *only* typed stopwords, we'd rather search for them than return nothing.
        assert_eq!(escape_fts_query("et"), "\"et\"*");
    }

    #[test]
    fn stopword_detection_is_case_insensitive() {
        assert_eq!(escape_fts_query("Et"), "\"Et\"*"); // single stopword — kept as fallback
        assert_eq!(escape_fts_query("Louis Et Quentin"), "\"Louis\" OR \"Quentin\"*");
    }
}
