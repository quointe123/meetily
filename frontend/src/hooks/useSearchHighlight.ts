'use client';

import { useEffect, RefObject } from 'react';
import { filterStopwords } from '@/lib/searchStopwords';

// Minimal typing for the CSS Highlights API (Chromium 105+, WebView2 shipped).
// Fine to keep inline rather than bumping lib targets.
type HighlightCtor = new (...ranges: Range[]) => any;
type HighlightsRegistry = Map<string, any> & {
  set(name: string, highlight: any): HighlightsRegistry;
  delete(name: string): boolean;
};
interface CSSWithHighlights {
  highlights?: HighlightsRegistry;
}
declare const Highlight: HighlightCtor | undefined;

/**
 * Paints CSS highlights over text nodes inside `containerRef` that match any
 * meaningful token of `searchTerm`. Non-destructive — uses the CSS Highlights
 * API so the underlying DOM (and any contenteditable/BlockNote state) is
 * untouched. Pair with a stylesheet rule like `::highlight(search-match) { ... }`.
 *
 * Stopwords are filtered so "Louis et Quentin" only highlights Louis + Quentin.
 * Falls back to a no-op if the browser lacks `CSS.highlights` or `Highlight`.
 */
export function useSearchHighlight(
  containerRef: RefObject<HTMLElement | null>,
  searchTerm: string | null | undefined,
  highlightName: string = 'search-match',
) {
  useEffect(() => {
    const css = (CSS as unknown) as CSSWithHighlights;
    const registry = css.highlights;
    const HighlightCtorRef = typeof Highlight !== 'undefined' ? Highlight : undefined;
    if (!registry || !HighlightCtorRef || !containerRef.current || !searchTerm?.trim()) {
      return;
    }

    const terms = filterStopwords(searchTerm.split(/\s+/))
      .map(t => t.toLowerCase())
      .filter(t => t.length >= 2); // avoid 1-char noise
    if (terms.length === 0) return;

    // Paint on the next frame so BlockNote has rendered its content.
    let rafId = 0;
    const paint = () => {
      const root = containerRef.current;
      if (!root) return;
      const ranges: Range[] = [];
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      let node = walker.nextNode();
      while (node) {
        const text = node.textContent ?? '';
        const lower = text.toLowerCase();
        for (const term of terms) {
          let idx = lower.indexOf(term);
          while (idx !== -1) {
            const r = new Range();
            r.setStart(node, idx);
            r.setEnd(node, idx + term.length);
            ranges.push(r);
            idx = lower.indexOf(term, idx + term.length);
          }
        }
        node = walker.nextNode();
      }
      if (ranges.length === 0) {
        registry.delete(highlightName);
        return;
      }
      registry.set(highlightName, new HighlightCtorRef(...ranges));
    };

    rafId = requestAnimationFrame(paint);

    // Re-paint when the container's subtree mutates (BlockNote streams content).
    const observer = new MutationObserver(() => {
      cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(paint);
    });
    observer.observe(containerRef.current, { childList: true, subtree: true, characterData: true });

    return () => {
      observer.disconnect();
      cancelAnimationFrame(rafId);
      registry.delete(highlightName);
    };
  }, [containerRef, searchTerm, highlightName]);
}
