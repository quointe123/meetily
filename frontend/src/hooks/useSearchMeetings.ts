'use client';

import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface SearchMatch {
  transcript_id: string;
  text: string;
  timestamp: string;
  highlight_start: number;
  highlight_end: number;
  match_type: string;
}

export interface SearchMeetingResult {
  meeting_id: string;
  title: string;
  score: number;
  matches: SearchMatch[];
}

interface UseSearchMeetingsReturn {
  query: string;
  results: SearchMeetingResult[];
  isSearching: boolean;
  search: (query: string) => void;
  clearSearch: () => void;
}

export function useSearchMeetings(): UseSearchMeetingsReturn {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchMeetingResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const debounceRef = useRef<NodeJS.Timeout | null>(null);

  const search = useCallback((searchQuery: string) => {
    setQuery(searchQuery);

    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    if (!searchQuery.trim()) {
      setResults([]);
      setIsSearching(false);
      return;
    }

    setIsSearching(true);

    debounceRef.current = setTimeout(async () => {
      try {
        const searchResults = await invoke('api_search_meetings', {
          query: searchQuery,
          limit: 20,
        }) as SearchMeetingResult[];
        setResults(searchResults);
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
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }
  }, []);

  return { query, results, isSearching, search, clearSearch };
}
