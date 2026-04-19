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
use app_lib::search::types::{MatchKind, SearchHit, SourceType};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::time::Instant;

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

fn print_hits(query: &str, hits: &[SearchHit], elapsed_ms: u128) {
    println!("\n── QUERY: {:?}  ({} hits in {} ms)", query, hits.len(), elapsed_ms);
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

fn top_contains(hits: &[SearchHit], n: usize, needle: &str) -> bool {
    let needle_lc = needle.to_lowercase();
    hits.iter().take(n).any(|h| h.chunk_text.to_lowercase().contains(&needle_lc))
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
        ("typo / missing char", "commnication"),
        ("typo / extra char", "commmunication"),
        ("typo / transposition", "recieved"),

        // ── 3. Very short query (prefix match territory) ──────────────
        ("short / 2 chars", "ai"),
        ("short / 3 chars", "dev"),
        ("short / 1 char", "a"),

        // ── 4. Long word ──────────────────────────────────────────────
        ("long / French", "coordination"),
        ("long / English", "communication"),
        ("long / 14 chars", "professionnalisation"),

        // ── 5. Multi-word — stopword filter should help ───────────────
        ("multi / names with stopword", "Louis et Quentin"),
        ("multi / names list", "Chloé Fred Mathéo"),
        ("multi / concept phrase", "retours amazon"),
        ("multi / English phrase", "pricing decision"),
        ("multi / French phrase", "suivi des commandes"),
        ("multi / mixed FR+EN", "Amazon returns unified"),
        ("multi / long sentence", "système de suivi des commandes pour améliorer la coordination"),

        // ── 6. Stopwords only (fallback path) ─────────────────────────
        ("stopwords only / FR", "et le ou"),
        ("stopwords only / EN", "the and of"),

        // ── 7. Semantic-only (paraphrases, not verbatim in corpus) ────
        ("semantic / paraphrase", "problème de gestion d'inventaire"),
        ("semantic / abstract", "problèmes de communication dans l'équipe"),
        ("semantic / intent", "comment suivre une commande"),
        ("semantic / question FR", "combien ça coûte ?"),
        ("semantic / question EN", "who handles the deliveries?"),

        // ── 8. Accents / diacritics (tokenizer uses remove_diacritics 2) ────
        ("diacritics / missing accent", "cafe"),            // data has café or none
        ("diacritics / query accent-free", "matheo"),       // should still find Mathéo
        ("diacritics / query accented", "Mathéo"),
        ("diacritics / e vs é", "decison"),                 // both accent-free AND typo

        // ── 9. Casing ─────────────────────────────────────────────────
        ("case / all caps", "AMAZON"),
        ("case / mixed", "AmaZon"),
        ("case / lowercase proper noun", "mathéo"),

        // ── 10. Numbers / dates / times ───────────────────────────────
        ("number / bare", "99"),
        ("number / year", "2024"),
        ("number / amount", "99 euros"),
        ("number / date iso", "2024-07-05"),
        ("number / time", "03:37"),
        ("number / big", "900000"),

        // ── 11. Special characters ────────────────────────────────────
        ("special / parentheses", "(décisions)"),
        ("special / apostrophe variant", "d'accord"),        // U+0027
        ("special / curly apostrophe", "d’accord"),          // U+2019
        ("special / trailing punctuation", "amazon?"),
        ("special / quotes", "\"pricing\""),
        ("special / hyphen", "marché-client"),

        // ── 12. Out-of-corpus (should return nothing or very low) ─────
        ("unknown / random English", "photosynthesis"),
        ("unknown / random FR", "anthropomorphisme"),
        ("unknown / random noun", "kangaroo"),
        ("unknown / name not in data", "Benoît Dupont"),

        // ── 13. Duplicates / same-text meetings ───────────────────────
        ("dup / very common chunk", "Bonjour on était au travail"),

        // ── 14. Stopword cleanup edge ─────────────────────────────────
        ("edge / single stopword", "et"),
        ("edge / apostrophe-stopword", "l'équipe"),
        ("edge / empty-ish", "   "),
    ];

    let mut total_ms: u128 = 0;
    let mut n_queries: usize = 0;
    for (label, q) in &queries {
        println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("CATEGORY: {}", label);
        let start = Instant::now();
        let hits = engine.search(q, 5).await.expect("search ok");
        let elapsed = start.elapsed().as_millis();
        total_ms += elapsed;
        n_queries += 1;
        print_hits(q, &hits, elapsed);
    }

    // ── Limit scaling ─────────────────────────────────────────────────
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SECTION: limit scaling (same query, different N)");
    for n in [1, 5, 20, 50] {
        let start = Instant::now();
        let hits = engine.search("amazon", n).await.expect("search ok");
        let elapsed = start.elapsed().as_millis();
        println!("  limit={}  got={}  ms={}  top_score={:.3}",
            n, hits.len(), elapsed,
            hits.first().map(|h| h.score).unwrap_or(0.0));
    }

    // ── Source-type boost verification ────────────────────────────────
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SECTION: source-type multiplier verification");
    println!("  (title ×1.3, summary/key_points ×1.15, action_items ×1.1, transcript/notes ×1.0)");
    let hits = engine.search("20min", 10).await.expect("search ok");
    let mut seen_title = false;
    for h in &hits {
        if h.source_type == SourceType::Title {
            seen_title = true;
            println!("  ✓ title hit: score={:.3} kinds={}", h.score, kinds_badge(&h.match_kinds));
        }
    }
    if !seen_title {
        println!("  (no title hit for this query — can't verify boost)");
    }

    // ── RRF 3-way alignment: top hits should have 3 badges ────────────
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SECTION: RRF 3-way verification");
    let hits = engine.search("amazon", 10).await.expect("search ok");
    let three_way = hits.iter().filter(|h| h.match_kinds.len() == 3).count();
    println!("  query=\"amazon\" top-10: {}/10 hits found by all 3 rankers (fts+sem+fuz)", three_way);
    assert!(three_way >= 3, "expected the top 'amazon' hits to align across all 3 rankers");

    // ── Expected-result assertions (regression gates) ─────────────────
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SECTION: regression gates");
    let cases = [
        ("amazon", "amazon"),
        ("amzaon", "amazon"),
        ("livrison", "livraison"),
        ("matheo", "mathéo"),
        ("retours amazon", "amazon"),
        ("Louis et Quentin", "louis"),
    ];
    for (q, needle) in cases {
        let hits = engine.search(q, 5).await.expect("search ok");
        let ok = top_contains(&hits, 3, needle);
        println!("  {} query={:?} → top-3 contains {:?}: {}",
            if ok { "✓" } else { "✗" }, q, needle, ok);
        assert!(ok, "query {:?} failed to surface {:?} in top-3", q, needle);
    }

    // ── Out-of-corpus sanity: MIN_TOP_SCORE should wipe these to zero hits ──
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("SECTION: out-of-corpus ceiling (should all return 0 hits)");
    for q in ["photosynthesis", "anthropomorphisme", "kangaroo"] {
        let hits = engine.search(q, 5).await.expect("search ok");
        let top = hits.first().map(|h| h.score).unwrap_or(0.0);
        println!("  query={:?}  top_score={:.3}  hits={}", q, top, hits.len());
        assert!(hits.is_empty(), "{:?} leaked through MIN_TOP_SCORE", q);
    }

    // ── Timing summary ────────────────────────────────────────────────
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("TIMING: {} queries, total {} ms, avg {:.1} ms/query",
        n_queries, total_ms, total_ms as f64 / n_queries as f64);
}
