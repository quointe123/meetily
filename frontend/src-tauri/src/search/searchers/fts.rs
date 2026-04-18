use anyhow::Result;
use sqlx::{Row, SqlitePool};

use crate::search::types::RankedHit;

/// Escape a user query for FTS5 MATCH:
/// - wrap each token in double quotes to disable operator parsing
/// - append `*` to the last token for prefix match
pub fn escape_fts_query(query: &str) -> String {
    let tokens: Vec<&str> = query.split_whitespace().filter(|t| !t.is_empty()).collect();
    if tokens.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = tokens
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if let Some(last) = parts.last_mut() {
        // remove trailing closing quote to append *
        if last.ends_with('"') {
            last.pop();
            last.push_str("\"*");
        }
    }
    parts.join(" ")
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
    fn escape_preserves_tokens_and_adds_prefix_star() {
        assert_eq!(escape_fts_query("hello world"), "\"hello\" \"world\"*");
    }

    #[test]
    fn escape_handles_empty() {
        assert_eq!(escape_fts_query(""), "");
        assert_eq!(escape_fts_query("   "), "");
    }

    #[test]
    fn escape_escapes_embedded_double_quotes() {
        assert_eq!(escape_fts_query("say \"hi\""), "\"say\" \"\"\"hi\"\"\"*");
    }
}
