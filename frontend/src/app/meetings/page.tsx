'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import { Search, X, Loader2 } from 'lucide-react';
import { useSearchMeetings } from '@/hooks/useSearchMeetings';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { MeetingCard, MeetingCardData } from '@/components/MeetingCard';
import { ConfirmationModal } from '@/components/ConfirmationModel/confirmation-modal';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from '@/components/ui/dialog';
import { VisuallyHidden } from '@/components/ui/visually-hidden';

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
      // Find the first transcript hit for this meeting (has source_id pointing to a transcript row)
      const hit = searchResults.find(
        r => r.meeting_id === meeting.id && r.source_type === 'transcript' && r.source_id
      );
      if (hit && hit.char_start !== null && hit.char_end !== null) {
        router.push(`/meeting-details?id=${meeting.id}&search=${encodeURIComponent(searchQuery)}&transcript_id=${hit.source_id}&highlight_start=${hit.char_start}&highlight_end=${hit.char_end}`);
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

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Sticky search bar */}
      <div className="sticky top-0 z-10 bg-gray-50 px-6 pt-6 pb-4">
        <div className="relative max-w-xl mx-auto">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-400" />
          <input
            type="text"
            placeholder="Rechercher dans les meetings..."
            value={searchQuery}
            onChange={(e) => search(e.target.value)}
            className="w-full pl-10 pr-10 py-2 rounded-lg border border-gray-300 bg-white text-sm text-gray-900 placeholder:text-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
          {isSearching && (
            <Loader2 className="absolute right-10 top-1/2 -translate-y-1/2 w-4 h-4 animate-spin text-gray-400" />
          )}
          {searchQuery && (
            <button
              onClick={clearSearch}
              className="absolute right-3 top-1/2 -translate-y-1/2 p-0.5 hover:bg-gray-100 rounded-full"
            >
              <X className="w-4 h-4 text-gray-400" />
            </button>
          )}
        </div>
      </div>

      {/* Content */}
      <div className="px-6 pb-6">
        {displayedMeetings.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <h2 className="text-lg font-semibold text-gray-700">
              {searchQuery.trim() ? 'Aucun resultat' : 'Aucun meeting'}
            </h2>
            <p className="text-sm text-gray-500 mt-1">
              {searchQuery.trim()
                ? 'Aucun meeting ne correspond a votre recherche.'
                : 'Les meetings enregistres apparaitront ici.'}
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
            {displayedMeetings.map((meeting) => (
              <MeetingCard
                key={meeting.id}
                meeting={meeting}
                onClick={() => handleCardClick(meeting)}
                onRename={handleRenameOpen}
                onDelete={(id) => setDeleteTarget(id)}
                searchSnippet={searchQuery.trim() ? getSearchSnippet(meeting.id) : null}
                highlightTerms={searchQuery.trim() ? searchQuery.split(/\s+/) : []}
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
