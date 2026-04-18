'use client';

import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

export type SourceType =
  | 'transcript' | 'title' | 'summary' | 'action_items' | 'key_points' | 'notes';

export type MatchKind = 'fts' | 'semantic' | 'fuzzy';

export interface SearchHit {
  meeting_id: string;
  meeting_title: string;
  source_type: SourceType;
  source_id: string | null;
  chunk_text: string;
  char_start: number | null;
  char_end: number | null;
  score: number;
  match_kinds: MatchKind[];
}

interface UseSearchMeetingsReturn {
  query: string;
  results: SearchHit[];
  isSearching: boolean;
  search: (query: string) => void;
  clearSearch: () => void;
}

export function useSearchMeetings(): UseSearchMeetingsReturn {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchHit[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const debounceRef = useRef<NodeJS.Timeout | null>(null);

  const search = useCallback((searchQuery: string) => {
    setQuery(searchQuery);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    if (!searchQuery.trim()) {
      setResults([]);
      setIsSearching(false);
      return;
    }
    setIsSearching(true);
    debounceRef.current = setTimeout(async () => {
      try {
        const hits = await invoke<SearchHit[]>('search_meetings', {
          query: searchQuery,
          limit: 20,
        });
        setResults(hits);
      } catch (error) {
        console.error('Search failed:', error);
        setResults([]);
      } finally {
        setIsSearching(false);
      }
    }, 300);
  }, []);

  const clearSearch = useCallback(() => {
    setQuery('');
    setResults([]);
    setIsSearching(false);
    if (debounceRef.current) clearTimeout(debounceRef.current);
  }, []);

  return { query, results, isSearching, search, clearSearch };
}
