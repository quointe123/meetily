//! Best-effort language consistency filter for ASR output.
//!
//! Parakeet v3 is multilingual with auto-detection per chunk. On noisy or
//! short audio, it can misclassify language and emit (for example) Portuguese
//! text when the audio is actually French. This module provides a lightweight
//! character-class heuristic to drop segments whose text is clearly not in
//! the user's selected language.
//!
//! The heuristic is intentionally simple:
//!   - Count letters in the transcript (unicode alphabetic chars).
//!   - Count how many of those are "signature" characters for the expected
//!     language vs known foreign signatures.
//!   - If foreign signatures dominate over expected-language signatures AND
//!     the transcript is long enough to be confident about, classify as
//!     foreign → caller drops the segment.
//!
//! This will NOT catch subtle cases (e.g. English in a French context, since
//! both share the Latin-1 alphabet). It targets the common failure mode:
//! Parakeet emitting Portuguese/Spanish/German when the audio is French.

/// ISO 639-1 language code extracted from the common `"fr"`, `"en-US"`,
/// `"fr-FR"` etc. forms. Returns lowercase two-letter code or the original
/// string if it doesn't start with two ASCII letters.
fn iso_base(code: &str) -> String {
    let base: String = code
        .chars()
        .take(2)
        .flat_map(|c| c.to_lowercase())
        .collect();
    if base.len() == 2 && base.chars().all(|c| c.is_ascii_alphabetic()) {
        base
    } else {
        code.to_lowercase()
    }
}

/// Classification outcome for a transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageCheck {
    /// Transcript is consistent with the expected language (or we can't tell).
    Match,
    /// Transcript looks like a different language from the expected one.
    Mismatch,
    /// Transcript is too short for a reliable check.
    TooShort,
    /// No expected language was provided, skip the filter.
    Skipped,
}

/// Minimum letter count in the transcript before we attempt a check.
/// Short transcripts ("oui", "d'accord") carry too little signal.
const MIN_LETTERS_FOR_CHECK: usize = 15;

/// Ratio of foreign-signature chars to expected-signature chars above which
/// we flag the transcript as mismatched.
///
/// Tuned conservatively to minimise false positives: a legitimate French
/// transcript may contain a few foreign-looking chars (proper nouns,
/// quotations), but consistently more foreign signatures than native
/// signatures across ~15+ letters is a strong signal of misclassification.
const FOREIGN_RATIO_THRESHOLD: f32 = 1.5;

/// Minimum English-stopword density above which we flag the transcript as
/// English-in-French. Tuned from observed Parakeet v3 hallucinations on
/// meeting audio — typical English output from v3 on a noisy French chunk
/// contains 40-60% stopwords, vs ~5-10% in legitimate French content with
/// borrowed English words.
const ENGLISH_STOPWORD_DENSITY_THRESHOLD: f32 = 0.30;

/// Tokens count below which the English-stopword check is unreliable.
const MIN_TOKENS_FOR_STOPWORD_CHECK: usize = 4;

/// Signature characters that are strong indicators of a given language.
/// Not exhaustive — just the most distinctive markers.
fn signature_chars(lang: &str) -> &'static [char] {
    match lang {
        "fr" => &['é', 'è', 'ê', 'à', 'â', 'î', 'ï', 'ô', 'û', 'ù', 'ç', 'œ', 'æ'],
        "pt" => &['ã', 'õ', 'â', 'ê', 'ô', 'á', 'ç', 'í', 'ó', 'ú', 'à'],
        "es" => &['ñ', 'á', 'é', 'í', 'ó', 'ú', 'ü', '¿', '¡'],
        "de" => &['ä', 'ö', 'ü', 'ß'],
        "it" => &['à', 'è', 'é', 'ì', 'ò', 'ù'],
        "en" => &[],
        _ => &[],
    }
}

/// Common English stopwords that would never dominate a French transcript.
/// Intentionally small and high-precision: picked words that are hallmarks
/// of English-leaning output (and NOT shared with French vocabulary).
/// Excludes English words that also appear in French (e.g. "a", "an",
/// "on") to avoid false positives on legitimate French content.
const ENGLISH_STOPWORDS: &[&str] = &[
    "the", "and", "is", "are", "was", "were", "be", "been", "being",
    "this", "that", "these", "those", "what", "when", "where", "which",
    "who", "why", "how", "yeah", "okay", "just", "have", "has", "had",
    "will", "would", "could", "should", "there", "here", "with", "from",
    "they", "them", "their", "its", "it's", "i'm", "don't", "didn't",
    "can't", "won't", "you", "your", "my", "me", "we", "us", "our",
    "going", "gonna", "wanna", "about", "because", "some", "all",
];

/// Count how many whitespace-separated tokens in the transcript match an
/// English stopword (case-insensitive, stripped of punctuation).
fn english_stopword_count(transcript: &str) -> (usize, usize) {
    let mut stopword_hits = 0usize;
    let mut total_tokens = 0usize;
    for raw_token in transcript.split_whitespace() {
        let cleaned: String = raw_token
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '\'')
            .collect::<String>()
            .to_lowercase();
        if cleaned.is_empty() {
            continue;
        }
        total_tokens += 1;
        if ENGLISH_STOPWORDS.contains(&cleaned.as_str()) {
            stopword_hits += 1;
        }
    }
    (stopword_hits, total_tokens)
}

/// Compare a transcript against the expected language. See module docs.
pub fn check_language(transcript: &str, expected_lang: Option<&str>) -> LanguageCheck {
    let expected = match expected_lang {
        Some(code) => iso_base(code),
        None => return LanguageCheck::Skipped,
    };

    // "auto" and empty are treated as "no expectation".
    if expected.is_empty() || expected == "auto" {
        return LanguageCheck::Skipped;
    }

    let letter_count = transcript.chars().filter(|c| c.is_alphabetic()).count();
    if letter_count < MIN_LETTERS_FOR_CHECK {
        return LanguageCheck::TooShort;
    }

    // English-content check only applies when the expected language is
    // non-English — we try to catch Parakeet v3's English hallucinations on
    // French audio. Skipped for English expected (trivially passes) and for
    // other languages where we haven't tuned the threshold.
    if expected == "fr" {
        let (stopwords, tokens) = english_stopword_count(transcript);
        if tokens >= MIN_TOKENS_FOR_STOPWORD_CHECK {
            let density = stopwords as f32 / tokens as f32;
            if density >= ENGLISH_STOPWORD_DENSITY_THRESHOLD {
                return LanguageCheck::Mismatch;
            }
        }
    }

    let expected_sigs = signature_chars(&expected);
    if expected_sigs.is_empty() {
        // We have no signature vocabulary for this language (e.g. plain English).
        // Don't drop anything — better to keep than to risk losing real content.
        return LanguageCheck::Skipped;
    }

    let lowered: String = transcript.to_lowercase();
    let mut expected_hits: usize = 0;
    let mut foreign_hits: usize = 0;

    for c in lowered.chars() {
        if expected_sigs.contains(&c) {
            expected_hits += 1;
            continue;
        }
        // Count this character as foreign ONLY if it's a signature of some
        // other non-English language we know. A rare accented char outside
        // every signature set (e.g. "ÿ") is ignored.
        for candidate in &["pt", "es", "de", "it"] {
            if *candidate == expected.as_str() {
                continue;
            }
            if signature_chars(candidate).contains(&c) {
                foreign_hits += 1;
                break;
            }
        }
    }

    // A single foreign accent can come from a proper noun (e.g. "São Paulo"
    // in a French sentence). Require at least 2 foreign hits before we'll
    // ever flag — this absorbs the single-proper-noun false positive.
    if foreign_hits < 2 {
        return LanguageCheck::Match;
    }

    // 2+ foreign hits and zero native signal → unambiguous mismatch.
    if expected_hits == 0 {
        return LanguageCheck::Mismatch;
    }

    // Otherwise rely on the ratio: foreign must dominate native by the
    // configured factor to count as mismatched.
    if foreign_hits as f32 > expected_hits as f32 * FOREIGN_RATIO_THRESHOLD {
        LanguageCheck::Mismatch
    } else {
        LanguageCheck::Match
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_typical_french_sentence() {
        let text = "Il a décidé de créer une nouvelle équipe pour gérer le projet très difficile.";
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Match);
    }

    #[test]
    fn rejects_portuguese_in_french_context() {
        // "It wasn't known there, I'm going to do..."
        let text = "Não foi conhecido pra aí, eu vou fazer uma reunião.";
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Mismatch);
    }

    #[test]
    fn rejects_german_in_french_context() {
        let text = "Ich möchte die Überprüfung der Qualität während der Sitzung.";
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Mismatch);
    }

    #[test]
    fn short_transcript_is_skipped() {
        assert_eq!(check_language("oui d'accord", Some("fr")), LanguageCheck::TooShort);
    }

    #[test]
    fn none_language_is_skipped() {
        assert_eq!(check_language("Some text here indeed", None), LanguageCheck::Skipped);
    }

    #[test]
    fn auto_language_is_skipped() {
        assert_eq!(check_language("Some text here indeed", Some("auto")), LanguageCheck::Skipped);
    }

    #[test]
    fn iso_base_trims_region_tag() {
        assert_eq!(iso_base("fr-FR"), "fr");
        assert_eq!(iso_base("en-US"), "en");
        assert_eq!(iso_base("FR"), "fr");
    }

    #[test]
    fn single_foreign_accent_from_proper_noun_is_not_flagged() {
        // ASCII-only French content with a single foreign-language accent
        // from a proper noun must NOT be dropped — requires ≥2 foreign hits.
        let text = "Nous allons parler de So Paulo pendant la prochaine heure avec Martin.";
        let text_with_accent = text.replace("So", "São");
        assert_eq!(check_language(&text_with_accent, Some("fr")), LanguageCheck::Match);
    }

    #[test]
    fn two_proper_noun_accents_with_strong_french_content_still_matches() {
        // Even two foreign accents pass if French accents dominate (ratio test).
        let text = "J'ai visité São Paulo et Ñuño pendant mes vacances d'été très agréables.";
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Match);
    }

    #[test]
    fn rejects_english_hallucination_in_french_context() {
        // Parakeet v3 hallucination observed on 20min meeting audio:
        // "That was a grid. I'm You're gonna-" on a noisy French chunk.
        // Dense English stopwords + no French accents => Mismatch.
        let text = "That was a grid. I'm You're gonna have to do it.";
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Mismatch);
    }

    #[test]
    fn rejects_english_stopword_spam() {
        // Hallucination pattern: stopwords repeated / clustered.
        let text = "Yeah yeah yeah, I think you have the screen there.";
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Mismatch);
    }

    #[test]
    fn french_with_borrowed_english_terms_is_kept() {
        // Legitimate French content may mention English-origin business
        // jargon (le dashboard, le meeting, un call) — must NOT be dropped.
        let text = "On a fait un call avec le client sur le dashboard des KPIs hier soir.";
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Match);
    }

    #[test]
    fn pure_french_without_accents_is_kept() {
        // ASCII-only French — no accents, no English stopwords.
        // Should not trigger either filter.
        let text = "Il faut finir ce travail avant lundi matin sans faute.";
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Match);
    }

    #[test]
    fn english_expected_still_skipped() {
        // English as expected language has no signature set, so we skip
        // — keep the prior behaviour untouched.
        let text = "Let me share the screen with you for a moment please.";
        assert_eq!(check_language(text, Some("en")), LanguageCheck::Skipped);
    }
}
