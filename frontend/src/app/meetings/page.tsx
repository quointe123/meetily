'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import { Search } from 'lucide-react';
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

  const handleCardClick = (meeting: MeetingCardData) => {
    setCurrentMeeting({ id: meeting.id, title: meeting.title });
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
            disabled
            className="w-full pl-10 pr-4 py-2 rounded-lg border border-gray-300 bg-white text-sm text-gray-900 placeholder:text-gray-400 disabled:opacity-60 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </div>
      </div>

      {/* Content */}
      <div className="px-6 pb-6">
        {meetingCards.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 text-center">
            <h2 className="text-lg font-semibold text-gray-700">
              Aucun meeting
            </h2>
            <p className="text-sm text-gray-500 mt-1">
              Les meetings enregistres apparaitront ici.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
            {meetingCards.map((meeting) => (
              <MeetingCard
                key={meeting.id}
                meeting={meeting}
                onClick={() => handleCardClick(meeting)}
                onRename={handleRenameOpen}
                onDelete={(id) => setDeleteTarget(id)}
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
