//! Diagnostic battery: exercises the live search index with a wide range of
//! queries (typos, single words, multi-word, stopwords, proper nouns, English
//! + French mix) and prints the top hits so a human can eyeball quality.
//!
//! Run with:
//!   cargo test --test search_battery -- --ignored --nocapture
//!
//! Expects APPDATA\com.meetily.ai\meeting_minutes.sqlite to exist and be
//! already indexed (the running app handles that). Opens read-only so it can
//! coexist with the running app (WAL).

use app_lib::search::engine::HybridSearchEngine;
use app_lib::search::searchers::semantic::EmbeddingCache;
use app_lib::search::types::{MatchKind, SearchHit};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

fn db_path() -> String {
    if let Ok(p) = std::env::var("MEETILY_DB") {
        return p;
    }
    let appdata = std::env::var("APPDATA").expect("APPDATA not set");
    format!("{}\\com.meetily.ai\\meeting_minutes.sqlite", appdata)
}

fn kinds_badge(kinds: &[MatchKind]) -> String {
    let mut parts = Vec::new();
    if kinds.contains(&MatchKind::Fts) { parts.push("fts"); }
    if kinds.contains(&MatchKind::Semantic) { parts.push("sem"); }
    if kinds.contains(&MatchKind::Fuzzy) { parts.push("fuz"); }
    parts.join("+")
}

fn truncate(s: &str, max: usize) -> String {
    let collapsed = s.replace('\n', " ").replace('\r', " ");
    let tokens: Vec<&str> = collapsed.split_whitespace().collect();
    let joined = tokens.join(" ");
    if joined.chars().count() <= max {
        joined
    } else {
        let cut: String = joined.chars().take(max).collect();
        format!("{}…", cut)
    }
}

fn print_hits(query: &str, hits: &[SearchHit]) {
    println!("\n── QUERY: {:?}  ({} hits)", query, hits.len());
    if hits.is_empty() {
        println!("   (no results)");
        return;
    }
    for (i, h) in hits.iter().take(5).enumerate() {
        println!(
            "  {}. [{}|{}] score={:.3} meeting={:?}",
            i + 1,
            h.source_type.as_str(),
            kinds_badge(&h.match_kinds),
            h.score,
            truncate(&h.meeting_title, 40),
        );
        println!("     {}", truncate(&h.chunk_text, 140));
    }
}

#[tokio::test]
#[ignore]
async fn search_battery_live_db() {
    let path = db_path();
    println!("DB: {}", path);
    let opts = SqliteConnectOptions::from_str(&format!("sqlite:{}", path))
        .expect("bad db url")
        .read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("open db");

    let cache = EmbeddingCache::new();
    cache.reload(&pool).await.expect("reload cache");

    let chunk_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM search_chunks")
        .fetch_one(&pool).await.unwrap();
    let emb_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM search_embeddings")
        .fetch_one(&pool).await.unwrap();
    println!("chunks={}  embeddings={}", chunk_count, emb_count);

    let engine = HybridSearchEngine::new(pool, cache);

    // (label, query). Grouped by intent so a human can scan categories.
    let queries: Vec<(&str, &str)> = vec![
        // ── 1. Exact single words (expect FTS dominant) ───────────────
        ("single-word / proper noun", "amazon"),
        ("single-word / French common", "livraison"),
        ("single-word / English", "pricing"),

        // ── 2. Typos (expect fuzzy + semantic to save us) ─────────────
        ("typo / amazon", "amzaon"),
        ("typo / double swap", "amzon"),
        ("typo / French", "livrison"),
        ("typo / phonetic", "rembourssé"),

        // ── 3. Very short query (prefix match territory) ──────────────
        ("short / 2 chars", "ai"),
        ("short / 3 chars", "dev"),

        // ── 4. Long word ──────────────────────────────────────────────
        ("long / French", "coordination"),
        ("long / English", "communication"),

        // ── 5. Multi-word — stopword filter should help ───────────────
        ("multi / names with stopword", "Louis et Quentin"),
        ("multi / names list", "Chloé Fred Mathéo"),
        ("multi / concept phrase", "retours amazon"),
        ("multi / English phrase", "pricing decision"),
        ("multi / French phrase", "suivi des commandes"),
        ("multi / mixed FR+EN", "Amazon returns unified"),

        // ── 6. Stopwords only (fallback path) ─────────────────────────
        ("stopwords only / FR", "et le ou"),
        ("stopwords only / EN", "the and of"),

        // ── 7. Semantic-only (paraphrases, not verbatim in corpus) ────
        ("semantic / paraphrase", "problème de gestion d'inventaire"),
        ("semantic / abstract", "problèmes de communication dans l'équipe"),
        ("semantic / intent", "comment suivre une commande"),

        // ── 8. Noise / edge cases ─────────────────────────────────────
        ("edge / all caps", "AMAZON"),
        ("edge / numbers", "99 euros"),
        ("edge / punctuation", "d'accord"),
        ("edge / single stopword", "et"),
        ("edge / empty-ish", "   "),
    ];

    for (label, q) in &queries {
        println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("CATEGORY: {}", label);
        let hits = engine.search(q, 5).await.expect("search ok");
        print_hits(q, &hits);
    }
}
