// Client-side fuzzy word matching used by the three highlight paths
// (MeetingCard snippet, TranscriptPanel, useSearchHighlight for summaries).
//
// The backend can match "amaon" → "amazon" via fuzzy scoring, but the UI
// then needs to figure out *which word in the rendered text* is the real
// match so it can highlight it. We do that with a plain Levenshtein ratio
// evaluated per-word, client-side — cheap enough even on long transcripts.

function levenshteinDistance(a: string, b: string): number {
  if (a === b) return 0;
  if (a.length === 0) return b.length;
  if (b.length === 0) return a.length;
  let prev = new Array(b.length + 1);
  for (let j = 0; j <= b.length; j++) prev[j] = j;
  for (let i = 1; i <= a.length; i++) {
    const curr = new Array(b.length + 1);
    curr[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(curr[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost);
    }
    prev = curr;
  }
  return prev[b.length];
}

function levenshteinRatio(a: string, b: string): number {
  const sum = a.length + b.length;
  if (sum === 0) return 1;
  return (sum - levenshteinDistance(a, b)) / sum;
}

const WORD_REGEX = /[\p{L}\p{N}]+/gu;

// Default ratio threshold. Calibrated so "amaon" matches "amazon" (~0.91)
// but "maison" vs "amaon" (~0.73) gets rejected — tighter than the search-
// side 0.70 because highlight precision matters more than recall.
export const HIGHLIGHT_FUZZY_THRESHOLD = 0.82;

/**
 * Does any of `terms` fuzzy-match `word`? Matches if exact/substring OR
 * Levenshtein ratio ≥ threshold. Terms shorter than 3 chars only accept
 * exact matches — too short for fuzzy to be meaningful.
 */
export function wordMatchesAnyTerm(
  word: string,
  terms: string[],
  threshold: number = HIGHLIGHT_FUZZY_THRESHOLD,
): boolean {
  const lowerWord = word.toLowerCase();
  return terms.some(term => {
    const lowerTerm = term.toLowerCase();
    if (lowerTerm.length === 0) return false;
    if (lowerWord === lowerTerm) return true;
    if (lowerWord.includes(lowerTerm) || lowerTerm.includes(lowerWord)) return true;
    if (lowerTerm.length < 3) return false;
    // Length guard: giant length mismatch is never a real fuzzy hit.
    if (Math.abs(lowerWord.length - lowerTerm.length) > 3) return false;
    return levenshteinRatio(lowerWord, lowerTerm) >= threshold;
  });
}

/**
 * Find word-bounded spans in `text` that match any of `terms`, allowing fuzzy
 * matches. Useful for building highlights when the query may contain typos.
 */
export function findMatchSpans(
  text: string,
  terms: string[],
  threshold: number = HIGHLIGHT_FUZZY_THRESHOLD,
): Array<{ start: number; end: number }> {
  const spans: Array<{ start: number; end: number }> = [];
  if (!text || terms.length === 0) return spans;
  const regex = new RegExp(WORD_REGEX.source, WORD_REGEX.flags);
  let m: RegExpExecArray | null;
  while ((m = regex.exec(text)) !== null) {
    if (wordMatchesAnyTerm(m[0], terms, threshold)) {
      spans.push({ start: m.index, end: m.index + m[0].length });
    }
  }
  return spans;
}
