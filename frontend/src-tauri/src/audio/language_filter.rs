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

    // If neither side fired, we can't tell — treat as a match.
    if expected_hits == 0 && foreign_hits == 0 {
        return LanguageCheck::Match;
    }

    // Strong foreign signal with weak native signal → mismatch.
    if expected_hits == 0 && foreign_hits >= 2 {
        return LanguageCheck::Mismatch;
    }

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
    fn english_transcript_in_french_context_is_not_flagged() {
        // We intentionally do NOT flag English content because English and
        // French share the Latin-1 alphabet with no distinctive signatures
        // beyond French accents. Dropping English would risk false positives
        // on ASCII-only French (rare but possible, e.g. code / acronyms).
        let text = "Let me share the screen with you for a moment please.";
        // 0 french sigs, 0 foreign sigs => Match (no signal).
        assert_eq!(check_language(text, Some("fr")), LanguageCheck::Match);
    }
}
