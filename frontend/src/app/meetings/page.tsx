'use client';

import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { useRouter } from 'next/navigation';
import { Search, X } from 'lucide-react';
import { useSearchMeetings } from '@/hooks/useSearchMeetings';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { MeetingCard, MeetingCardData } from '@/components/MeetingCard';
import { filterStopwords } from '@/lib/searchStopwords';
import { ConfirmationModal } from '@/components/ConfirmationModel/confirmation-modal';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from '@/components/ui/dialog';
import { VisuallyHidden } from '@/components/ui/visually-hidden';

type IndexingStatus = {
  total_meetings: number;
  indexed_meetings: number;
  chunks_total: number;
  chunks_done: number;
  in_progress: boolean;
};

export default function MeetingsPage() {
  const router = useRouter();
  const { meetings, deleteMeeting, renameMeeting, setCurrentMeeting } =
    useSidebar();

  // Delete state
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  // Rename state
  const [renameTarget, setRenameTarget] = useState<{
    id: string;
    title: string;
  } | null>(null);
  const [renameValue, setRenameValue] = useState('');

  // Search
  const { query: searchQuery, results: searchResults, isSearching, search, clearSearch } = useSearchMeetings();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [isSearchFocused, setIsSearchFocused] = useState(false);
  const isMac = useMemo(
    () => typeof navigator !== 'undefined' && /mac/i.test(navigator.platform || navigator.userAgent),
    [],
  );

  // ⌘K / Ctrl+K focuses the search — expected affordance for a search-first page.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = isMac ? e.metaKey : e.ctrlKey;
      if (mod && (e.key === 'k' || e.key === 'K')) {
        e.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      }
      if (e.key === 'Escape' && document.activeElement === searchInputRef.current) {
        if (searchQuery) clearSearch();
        searchInputRef.current?.blur();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [isMac, searchQuery, clearSearch]);

  // Semantic indexing status
  const [indexingStatus, setIndexingStatus] = useState<IndexingStatus | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const s = await invoke<IndexingStatus>('get_indexing_status');
        setIndexingStatus(s);
      } catch (e) {
        console.warn('[meetings] get_indexing_status failed:', e);
      }

      unlisten = await listen<{ processed: number; total: number; done: boolean }>(
        'semantic-indexing-progress',
        (ev) => {
          setIndexingStatus((prev) => prev
            ? ({
                ...prev,
                indexed_meetings: ev.payload.processed,
                total_meetings: ev.payload.total,
                in_progress: !ev.payload.done,
              })
            : {
                total_meetings: ev.payload.total,
                indexed_meetings: ev.payload.processed,
                chunks_total: 0,
                chunks_done: 0,
                in_progress: !ev.payload.done,
              });
        }
      );
    })();
    return () => { unlisten?.(); };
  }, []);

  // Meeting cards with rich data
  const [meetingCards, setMeetingCards] = useState<MeetingCardData[]>([]);

  const fetchMeetingCards = useCallback(async () => {
    try {
      const cards = await invoke('api_get_meetings_cards') as MeetingCardData[];
      setMeetingCards(cards);
    } catch (error) {
      console.error('Error fetching meeting cards:', error);
      // Fallback to basic meetings list
      setMeetingCards(meetings.map(m => ({
        id: m.id,
        title: m.title,
        created_at: '',
        duration_seconds: null,
        summary_preview: null,
      })));
    }
  }, [meetings]);

  useEffect(() => {
    fetchMeetingCards();
  }, [fetchMeetingCards]);

  // Filter cards based on search results
  const displayedMeetings = searchQuery.trim()
    ? meetingCards.filter(card => searchResults.some(r => r.meeting_id === card.id))
    : meetingCards;

  // Get search snippet for a card
  const getSearchSnippet = (meetingId: string): string | null => {
    const hits = searchResults.filter(r => r.meeting_id === meetingId);
    if (hits.length === 0) return null;
    // Prefer the highest-scored hit (already sorted by backend, but filter-safe)
    const hit = hits[0];
    const start = hit.char_start ?? 0;
    const end = hit.char_end ?? hit.chunk_text.length;
    const before = Math.max(0, start - 50);
    const after = Math.min(hit.chunk_text.length, end + 50);
    let snippet = hit.chunk_text.slice(before, after);
    if (before > 0) snippet = '...' + snippet;
    if (after < hit.chunk_text.length) snippet += '...';
    return snippet;
  };

  const handleCardClick = (meeting: MeetingCardData) => {
    setCurrentMeeting({ id: meeting.id, title: meeting.title });
    if (searchQuery.trim()) {
      const meetingHits = searchResults.filter(r => r.meeting_id === meeting.id);
      // Prefer a transcript hit with offsets so we can scroll-and-highlight the exact segment.
      const transcriptHit = meetingHits.find(
        r => r.source_type === 'transcript' && r.source_id && r.char_start !== null && r.char_end !== null
      );
      if (transcriptHit) {
        router.push(`/meeting-details?id=${meeting.id}&search=${encodeURIComponent(searchQuery)}&transcript_id=${transcriptHit.source_id}&highlight_start=${transcriptHit.char_start}&highlight_end=${transcriptHit.char_end}`);
        return;
      }
      // No transcript hit but we still have a match (likely a summary/title hit). Pass the
      // search term so the SearchBanner + summary highlighting kick in.
      if (meetingHits.length > 0) {
        router.push(`/meeting-details?id=${meeting.id}&search=${encodeURIComponent(searchQuery)}`);
        return;
      }
    }
    router.push(`/meeting-details?id=${meeting.id}`);
  };

  const handleDeleteConfirm = async () => {
    if (!deleteTarget) return;
    try {
      await deleteMeeting(deleteTarget);
      toast.success('Meeting supprime');
    } catch (err) {
      console.error('Delete failed:', err);
      toast.error('Erreur lors de la suppression');
    } finally {
      setDeleteTarget(null);
    }
  };

  const handleRenameOpen = (meetingId: string, currentTitle: string) => {
    setRenameTarget({ id: meetingId, title: currentTitle });
    setRenameValue(currentTitle);
  };

  const handleRenameConfirm = async () => {
    if (!renameTarget || !renameValue.trim()) return;
    try {
      await renameMeeting(renameTarget.id, renameValue.trim());
      toast.success('Meeting renomme');
    } catch (err) {
      console.error('Rename failed:', err);
      toast.error('Erreur lors du renommage');
    } finally {
      setRenameTarget(null);
      setRenameValue('');
    }
  };

  const indexingProgress = indexingStatus && indexingStatus.total_meetings > 0
    ? Math.round((indexingStatus.indexed_meetings / indexingStatus.total_meetings) * 100)
    : 0;

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Sticky search bar — solid bg + hairline bottom shadow that reads as a subtle lift */}
      <div className="sticky top-0 z-10 bg-gray-50/95 backdrop-blur-[2px] px-6 pt-6 pb-4 shadow-[0_1px_0_0_rgb(0_0_0/0.04)]">
        <div
          className={`relative max-w-xl mx-auto rounded-lg bg-white transition-[box-shadow,border-color] duration-200 ease-out ${
            isSearchFocused
              ? 'shadow-[0_0_0_3px_rgb(251_191_36/0.18),0_1px_2px_0_rgb(0_0_0/0.04)] ring-1 ring-amber-400'
              : 'ring-1 ring-gray-200 shadow-[0_1px_2px_0_rgb(0_0_0/0.03)] hover:ring-gray-300'
          }`}
        >
          <Search
            className={`absolute left-3.5 top-1/2 -translate-y-1/2 h-4 w-4 transition-colors duration-200 ${
              isSearchFocused ? 'text-amber-600' : 'text-gray-400'
            }`}
            strokeWidth={2}
            aria-hidden
          />
          <input
            ref={searchInputRef}
            type="text"
            placeholder="Rechercher dans les meetings, les transcriptions, les résumés…"
            value={searchQuery}
            onChange={(e) => search(e.target.value)}
            onFocus={() => setIsSearchFocused(true)}
            onBlur={() => setIsSearchFocused(false)}
            aria-label="Rechercher"
            className="w-full bg-transparent rounded-lg pl-10 pr-20 py-2.5 text-[14px] text-gray-900 placeholder:text-gray-400 focus:outline-none"
          />

          {/* Right-side slot — ⌘K hint, clear button, or nothing */}
          <div className="absolute right-2.5 top-1/2 -translate-y-1/2 flex items-center">
            {searchQuery ? (
              <button
                type="button"
                onClick={clearSearch}
                aria-label="Effacer la recherche"
                className="flex h-6 w-6 items-center justify-center rounded-md text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-600"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            ) : (
              <kbd
                aria-hidden
                className="pointer-events-none hidden h-5 select-none items-center gap-0.5 rounded border border-gray-200 bg-gray-50 px-1.5 font-mono text-[10px] font-medium text-gray-500 shadow-[inset_0_-1px_0_0_rgb(0_0_0/0.03)] sm:flex"
              >
                <span className={isMac ? 'text-[11px] leading-none' : 'text-[10px] leading-none'}>
                  {isMac ? '⌘' : 'Ctrl'}
                </span>
                <span>K</span>
              </kbd>
            )}
          </div>

          {/* Indeterminate scan line while searching — amber segment sliding along the bottom edge */}
          <div className="pointer-events-none absolute inset-x-1.5 bottom-0 h-[1.5px] overflow-hidden rounded-b-lg">
            <div
              className={`h-full w-1/3 rounded-full bg-gradient-to-r from-transparent via-amber-500 to-transparent ${
                isSearching ? 'animate-search-scan' : 'opacity-0'
              }`}
            />
          </div>
        </div>

        {/* Indexing status — thin progress panel instead of a naked spinner */}
        {indexingStatus?.in_progress && (
          <div className="mt-3 max-w-xl mx-auto relative overflow-hidden rounded-md border border-amber-200/60 bg-amber-50/50">
            <div className="flex items-center gap-2.5 px-3.5 py-2">
              <span className="relative flex h-1.5 w-1.5" aria-hidden>
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-75" />
                <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-amber-500" />
              </span>
              <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-amber-900">
                Indexation sémantique
              </span>
              <span className="ml-auto text-[11px] tabular-nums text-amber-800/80">
                {indexingStatus.indexed_meetings}
                <span className="text-amber-700/40"> / </span>
                {indexingStatus.total_meetings}
                <span className="ml-1.5 text-amber-700/60">({indexingProgress}%)</span>
              </span>
            </div>
            {/* Determinate progress fill along the bottom edge */}
            <div className="h-[1.5px] bg-amber-200/40">
              <div
                className="h-full bg-gradient-to-r from-amber-400 to-amber-500 transition-[width] duration-500 ease-out"
                style={{ width: `${indexingProgress}%` }}
              />
            </div>
          </div>
        )}

        {/* Result count when a query is active and done computing */}
        {searchQuery.trim() && !isSearching && (
          <div className="mt-2.5 max-w-xl mx-auto text-center text-[11px] text-gray-500 tabular-nums">
            {displayedMeetings.length === 0 ? (
              <span>Aucun meeting ne correspond</span>
            ) : (
              <>
                <span className="font-semibold text-gray-700">{displayedMeetings.length}</span>
                <span>
                  {' '}
                  {displayedMeetings.length === 1 ? 'résultat' : 'résultats'}
                </span>
              </>
            )}
          </div>
        )}
      </div>

      {/* Content */}
      <div className="px-6 pb-6">
        {displayedMeetings.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-gray-100">
              <Search className="h-5 w-5 text-gray-400" strokeWidth={1.75} />
            </div>
            <h2 className="text-base font-semibold text-gray-800">
              {searchQuery.trim() ? 'Aucun résultat' : 'Aucun meeting'}
            </h2>
            <p className="mt-1 max-w-xs text-[13px] leading-relaxed text-gray-500">
              {searchQuery.trim()
                ? 'Essayez une autre formulation — la recherche tolère les fautes et les paraphrases.'
                : 'Les meetings enregistrés apparaîtront ici.'}
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-5">
            {displayedMeetings.map((meeting) => (
              <MeetingCard
                key={meeting.id}
                meeting={meeting}
                onClick={() => handleCardClick(meeting)}
                onRename={handleRenameOpen}
                onDelete={(id) => setDeleteTarget(id)}
                searchSnippet={searchQuery.trim() ? getSearchSnippet(meeting.id) : null}
                highlightTerms={searchQuery.trim() ? filterStopwords(searchQuery.split(/\s+/)) : []}
              />
            ))}
          </div>
        )}
      </div>

      {/* Delete confirmation modal */}
      <ConfirmationModal
        isOpen={deleteTarget !== null}
        text="Etes-vous sur de vouloir supprimer ce meeting ? Cette action est irreversible."
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteTarget(null)}
      />

      {/* Rename dialog */}
      <Dialog
        open={renameTarget !== null}
        onOpenChange={(open) => {
          if (!open) {
            setRenameTarget(null);
            setRenameValue('');
          }
        }}
      >
        <DialogContent>
          <VisuallyHidden>
            <DialogTitle>Renommer le meeting</DialogTitle>
          </VisuallyHidden>
          <div className="space-y-4">
            <h3 className="text-lg font-semibold">Renommer le meeting</h3>
            <input
              type="text"
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleRenameConfirm();
              }}
              className="w-full px-3 py-2 rounded-md border border-gray-300 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              autoFocus
            />
          </div>
          <DialogFooter>
            <button
              onClick={() => {
                setRenameTarget(null);
                setRenameValue('');
              }}
              className="px-4 py-2 text-sm text-gray-600 hover:bg-gray-100 rounded-md transition-colors"
            >
              Annuler
            </button>
            <button
              onClick={handleRenameConfirm}
              disabled={!renameValue.trim()}
              className="px-4 py-2 text-sm bg-blue-600 text-white hover:bg-blue-700 rounded-md transition-colors disabled:opacity-50"
            >
              Enregistrer
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
