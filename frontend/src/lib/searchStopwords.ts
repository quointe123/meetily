// Keep in sync with STOPWORDS in frontend/src-tauri/src/search/searchers/fts.rs.
// Used by the UI to drop common words from visual highlights so "et" / "the"
// don't appear as if they were the thing that matched.
const STOPWORDS = new Set<string>([
  // French
  'le', 'la', 'les', 'l', 'un', 'une', 'des', 'de', 'du', 'd',
  'et', 'ou', 'mais', 'donc', 'car', 'ni', 'or',
  'à', 'au', 'aux', 'en', 'dans', 'sur', 'sous', 'pour', 'par', 'avec', 'sans',
  'chez', 'vers', 'entre', 'depuis',
  'je', 'tu', 'il', 'elle', 'on', 'nous', 'vous', 'ils', 'elles',
  'me', 'te', 'se', 'lui', 'leur', 'y',
  'ce', 'cet', 'cette', 'ces', 'ça', 'cela',
  'qui', 'que', 'quoi', 'dont', 'où',
  'est', 'sont', 'es', 'ai', 'as', 'a', 'ont', 'avons', 'avez',
  'suis', 'êtes', 'été', 'être',
  'ne', 'pas', 'plus', 'si',
  // English
  'the', 'a', 'an', 'and', 'or', 'but',
  'of', 'to', 'in', 'on', 'at', 'for', 'by', 'with', 'from', 'into', 'as',
  'is', 'are', 'was', 'were', 'be', 'been', 'being',
  'i', 'you', 'he', 'she', 'it', 'we', 'they',
  'this', 'that', 'these', 'those',
  'not', 'no',
]);

export function filterStopwords(tokens: string[]): string[] {
  const kept = tokens.filter(t => t.trim() !== '' && !STOPWORDS.has(t.toLowerCase()));
  // If the query is *only* stopwords, fall back to the raw tokens so the user
  // still gets visual feedback on what they typed.
  return kept.length > 0 ? kept : tokens.filter(t => t.trim() !== '');
}
