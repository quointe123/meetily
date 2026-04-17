# Meetings Page + Advanced Search — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the expandable sidebar meeting list with a dedicated `/meetings` page featuring card-based meeting display, hybrid search (fuzzy + TF-IDF + semantic via Ollama), and scroll-to-highlight navigation on the meeting-details page.

**Architecture:** The sidebar becomes permanently compact (64px). A new `/meetings` page shows meeting cards with search. Search is implemented in the Python backend (`backend/app/search.py`) with a new `POST /search-meetings` endpoint, proxied through a new Tauri command. Embeddings are computed via Ollama's `nomic-embed-text` model and stored in a new SQLite table. The meeting-details page gains a search navigation banner with occurrence-by-occurrence scrolling and highlighting.

**Tech Stack:** Next.js 14 (React), Tailwind CSS, Tauri 2.x (Rust), FastAPI (Python), SQLite, rapidfuzz, numpy, Ollama embeddings

**Important architectural note:** The current search (`api_search_transcripts`) goes through Tauri's local SQLite via Rust (`frontend/src-tauri/src/database/repositories/transcript.rs`). The new advanced search will route through the Python backend which has access to rapidfuzz and Ollama. A new Tauri command `api_search_meetings` will proxy to the backend's `POST /search-meetings` endpoint.

---

## File Structure

### New files

| File | Responsibility |
|---|---|
| `frontend/src/app/meetings/page.tsx` | Meetings list page with search bar and card grid |
| `frontend/src/components/MeetingCard.tsx` | Individual meeting card component |
| `frontend/src/hooks/useSearchMeetings.ts` | Search hook (debounce, API call, state) |
| `frontend/src/components/MeetingDetails/SearchBanner.tsx` | Search navigation banner (term, prev/next, close, back) |
| `backend/app/search.py` | Hybrid search module (fuzzy + TF-IDF + semantic) |

### Modified files

| File | Change |
|---|---|
| `frontend/src/components/Sidebar/index.tsx` | Rewrite to compact icon column only |
| `frontend/src/components/Sidebar/SidebarProvider.tsx` | Remove search/collapse/folder state |
| `frontend/src/components/MainContent/index.tsx` | Always use `ml-16` (no dynamic width) |
| `frontend/src/app/layout.tsx` | Remove collapse-dependent logic |
| `frontend/src/app/meeting-details/page.tsx` | Read search params from URL, pass to page-content |
| `frontend/src/app/meeting-details/page-content.tsx` | Add SearchBanner, highlight logic |
| `frontend/src/components/MeetingDetails/TranscriptPanel.tsx` | Accept highlight props, pass to VirtualizedTranscriptView |
| `frontend/src-tauri/src/api/api.rs` | Add `MeetingCardData` struct, `api_search_meetings` and `api_get_meetings_with_details` commands |
| `frontend/src-tauri/src/lib.rs` | Register new commands |
| `backend/app/main.py` | Add `POST /search-meetings` and `GET /get-meetings-cards` endpoints |
| `backend/app/db.py` | Add `transcript_embeddings` table creation, embedding indexation, hybrid search queries |
| `backend/requirements.txt` | Add `rapidfuzz`, `numpy`, `httpx` |

---

## Task 1: Simplify SidebarProvider — remove search/collapse/folder state

**Files:**
- Modify: `frontend/src/components/Sidebar/SidebarProvider.tsx`

- [ ] **Step 1: Read current SidebarProvider and identify code to remove**

The following must be removed from `SidebarProvider.tsx`:
- Interface `TranscriptSearchResult` (lines 22-27)
- From `SidebarContextType`: `sidebarItems`, `isCollapsed`, `toggleCollapse`, `searchTranscripts`, `searchResults`, `isSearching` (lines 31, 33, 34, 38-40)
- State: `isCollapsed`, `sidebarItems`, `searchResults`, `isSearching` (lines 66, 68, 70-71)
- `baseItems` construction (lines 107-116)
- `toggleCollapse` function (lines 119-121)
- `setSidebarItems` effects (lines 128-134)
- `searchTranscripts` function (lines 137-155)
- From Provider value: remove `sidebarItems`, `isCollapsed`, `toggleCollapse`, `searchTranscripts`, `searchResults`, `isSearching`

- [ ] **Step 2: Apply the changes**

Replace the entire file with the cleaned version. The SidebarProvider keeps: `currentMeeting`, `setCurrentMeeting`, `meetings`, `setMeetings`, `isMeetingActive`, `setIsMeetingActive`, `serverAddress`, `setServerAddress`, `transcriptServerAddress`, `setTranscriptServerAddress`, `activeSummaryPolls`, `startSummaryPolling`, `stopSummaryPolling`, `refetchMeetings`.

```typescript
'use client';

import React, { createContext, useContext, useState, useEffect } from 'react';
import { usePathname } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';

export interface CurrentMeeting {
  id: string;
  title: string;
}

interface SidebarContextType {
  currentMeeting: CurrentMeeting | null;
  setCurrentMeeting: (meeting: CurrentMeeting | null) => void;
  meetings: CurrentMeeting[];
  setMeetings: (meetings: CurrentMeeting[]) => void;
  isMeetingActive: boolean;
  setIsMeetingActive: (active: boolean) => void;
  setServerAddress: (address: string) => void;
  serverAddress: string;
  transcriptServerAddress: string;
  setTranscriptServerAddress: (address: string) => void;
  activeSummaryPolls: Map<string, NodeJS.Timeout>;
  startSummaryPolling: (meetingId: string, processId: string, onUpdate: (result: any) => void) => void;
  stopSummaryPolling: (meetingId: string) => void;
  refetchMeetings: () => Promise<void>;
  deleteMeeting: (meetingId: string) => Promise<void>;
  renameMeeting: (meetingId: string, newTitle: string) => Promise<void>;
}

const SidebarContext = createContext<SidebarContextType | null>(null);

export const useSidebar = () => {
  const context = useContext(SidebarContext);
  if (!context) {
    throw new Error('useSidebar must be used within a SidebarProvider');
  }
  return context;
};

export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [currentMeeting, setCurrentMeeting] = useState<CurrentMeeting | null>({ id: 'intro-call', title: '+ New Call' });
  const [meetings, setMeetings] = useState<CurrentMeeting[]>([]);
  const [isMeetingActive, setIsMeetingActive] = useState(false);
  const [serverAddress, setServerAddress] = useState('');
  const [transcriptServerAddress, setTranscriptServerAddress] = useState('');
  const [activeSummaryPolls, setActiveSummaryPolls] = useState<Map<string, NodeJS.Timeout>>(new Map());

  const pathname = usePathname();

  const fetchMeetings = React.useCallback(async () => {
    if (serverAddress) {
      try {
        const meetings = await invoke('api_get_meetings') as Array<{ id: string, title: string }>;
        const transformedMeetings = meetings.map((meeting: any) => ({
          id: meeting.id,
          title: meeting.title
        }));
        setMeetings(transformedMeetings);
      } catch (error) {
        console.error('Error fetching meetings:', error);
        setMeetings([]);
      }
    }
  }, [serverAddress]);

  useEffect(() => {
    fetchMeetings();
  }, [serverAddress, fetchMeetings]);

  useEffect(() => {
    const fetchSettings = async () => {
      setServerAddress('http://localhost:5167');
      setTranscriptServerAddress('http://127.0.0.1:8178/stream');
    };
    fetchSettings();
  }, []);

  useEffect(() => {
    if (pathname === '/') {
      setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
    }
  }, [pathname]);

  // --- deleteMeeting and renameMeeting moved from Sidebar component ---
  const deleteMeeting = React.useCallback(async (meetingId: string) => {
    await invoke('api_delete_meeting', { meetingId });
    setMeetings(prev => prev.filter(m => m.id !== meetingId));
    if (currentMeeting?.id === meetingId) {
      setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
    }
  }, [currentMeeting]);

  const renameMeeting = React.useCallback(async (meetingId: string, newTitle: string) => {
    await invoke('api_save_meeting_title', { meetingId, title: newTitle });
    setMeetings(prev => prev.map(m => m.id === meetingId ? { ...m, title: newTitle } : m));
    if (currentMeeting?.id === meetingId) {
      setCurrentMeeting({ id: meetingId, title: newTitle });
    }
  }, [currentMeeting]);

  // --- Summary polling (unchanged from original) ---
  const startSummaryPolling = React.useCallback((
    meetingId: string,
    processId: string,
    onUpdate: (result: any) => void
  ) => {
    if (activeSummaryPolls.has(meetingId)) {
      clearInterval(activeSummaryPolls.get(meetingId)!);
    }

    let pollCount = 0;
    const MAX_POLLS = 200;

    const pollInterval = setInterval(async () => {
      pollCount++;
      if (pollCount >= MAX_POLLS) {
        clearInterval(pollInterval);
        setActiveSummaryPolls(prev => {
          const next = new Map(prev);
          next.delete(meetingId);
          return next;
        });
        onUpdate({ status: 'error', error: 'Summary generation timed out after 15 minutes.' });
        return;
      }
      try {
        const result = await invoke('api_get_summary', { meetingId }) as any;
        onUpdate(result);
        if (['completed', 'error', 'failed', 'cancelled'].includes(result.status)) {
          clearInterval(pollInterval);
          setActiveSummaryPolls(prev => {
            const next = new Map(prev);
            next.delete(meetingId);
            return next;
          });
        } else if (result.status === 'idle' && pollCount > 1) {
          clearInterval(pollInterval);
          setActiveSummaryPolls(prev => {
            const next = new Map(prev);
            next.delete(meetingId);
            return next;
          });
        }
      } catch (error) {
        onUpdate({ status: 'error', error: error instanceof Error ? error.message : 'Unknown error' });
        clearInterval(pollInterval);
        setActiveSummaryPolls(prev => {
          const next = new Map(prev);
          next.delete(meetingId);
          return next;
        });
      }
    }, 5000);

    setActiveSummaryPolls(prev => new Map(prev).set(meetingId, pollInterval));
  }, [activeSummaryPolls]);

  const stopSummaryPolling = React.useCallback((meetingId: string) => {
    const pollInterval = activeSummaryPolls.get(meetingId);
    if (pollInterval) {
      clearInterval(pollInterval);
      setActiveSummaryPolls(prev => {
        const next = new Map(prev);
        next.delete(meetingId);
        return next;
      });
    }
  }, [activeSummaryPolls]);

  useEffect(() => {
    return () => {
      activeSummaryPolls.forEach(interval => clearInterval(interval));
    };
  }, [activeSummaryPolls]);

  return (
    <SidebarContext.Provider value={{
      currentMeeting,
      setCurrentMeeting,
      meetings,
      setMeetings,
      isMeetingActive,
      setIsMeetingActive,
      setServerAddress,
      serverAddress,
      transcriptServerAddress,
      setTranscriptServerAddress,
      activeSummaryPolls,
      startSummaryPolling,
      stopSummaryPolling,
      refetchMeetings: fetchMeetings,
      deleteMeeting,
      renameMeeting,
    }}>
      {children}
    </SidebarContext.Provider>
  );
}
```

- [ ] **Step 3: Fix all compile errors from consumers**

Search for all usages of removed properties (`isCollapsed`, `toggleCollapse`, `sidebarItems`, `searchTranscripts`, `searchResults`, `isSearching`) in other files and remove/update them. Key files:
- `frontend/src/components/Sidebar/index.tsx` — will be rewritten in Task 2
- `frontend/src/components/MainContent/index.tsx` — remove `isCollapsed` usage, always use `ml-16`

For `MainContent/index.tsx`, replace the entire component:

```typescript
'use client';

import React from 'react';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  return (
    <main className="flex-1 ml-16">
      <div className="pl-8">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
```

- [ ] **Step 4: Verify the app compiles**

Run: `cd frontend && pnpm run build 2>&1 | head -50`

Note: This will have errors from Sidebar/index.tsx which still references removed properties. That's expected — it gets rewritten in Task 2.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/Sidebar/SidebarProvider.tsx frontend/src/components/MainContent/index.tsx
git commit -m "refactor: simplify SidebarProvider — remove search, collapse, and folder state"
```

---

## Task 2: Rewrite Sidebar as compact icon column

**Files:**
- Modify: `frontend/src/components/Sidebar/index.tsx`
- Modify: `frontend/src/app/layout.tsx`

- [ ] **Step 1: Rewrite Sidebar/index.tsx**

Replace the entire file with a simple compact icon column. The sidebar is always 64px wide, no expand/collapse. The NotebookPen icon navigates to `/meetings`. Keep the edit/delete modals since they'll be reused by the meetings page.

```typescript
'use client';

import React from 'react';
import { Settings, Mic, NotebookPen, Upload } from 'lucide-react';
import { useRouter, usePathname } from 'next/navigation';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useImportDialog } from '@/contexts/ImportDialogContext';
import { useConfig } from '@/contexts/ConfigContext';
import Info from '../Info';

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();
  const { openImportDialog } = useImportDialog();
  const { betaFeatures } = useConfig();

  const isMeetingsPage = pathname === '/meetings' || pathname?.includes('/meeting-details');
  const isSettingsPage = pathname === '/settings';
  const isHomePage = pathname === '/';

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      <div className="h-screen w-16 bg-white border-r shadow-sm flex flex-col">
        {/* Navigation icons */}
        <TooltipProvider>
          <div className="flex flex-col items-center space-y-4 mt-4">
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/')}
                  className={`p-2 rounded-full transition-colors duration-150 shadow-sm ${
                    isHomePage ? 'bg-red-600' : 'bg-red-500 hover:bg-red-600'
                  }`}
                >
                  <Mic className="w-5 h-5 text-white" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Accueil</p>
              </TooltipContent>
            </Tooltip>

            {betaFeatures.importAndRetranscribe && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    onClick={() => openImportDialog()}
                    className="p-2 rounded-lg transition-colors duration-150 hover:bg-blue-100 bg-blue-50"
                  >
                    <Upload className="w-5 h-5 text-blue-600" />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="right">
                  <p>Import Audio</p>
                </TooltipContent>
              </Tooltip>
            )}

            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/meetings')}
                  className={`p-2 rounded-lg transition-colors duration-150 ${
                    isMeetingsPage ? 'bg-gray-200' : 'hover:bg-gray-100'
                  }`}
                >
                  <NotebookPen className="w-5 h-5 text-gray-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Meeting Notes</p>
              </TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/settings')}
                  className={`p-2 rounded-lg transition-colors duration-150 ${
                    isSettingsPage ? 'bg-gray-200' : 'hover:bg-gray-100'
                  }`}
                >
                  <Settings className="w-5 h-5 text-gray-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Settings</p>
              </TooltipContent>
            </Tooltip>

            <Info isCollapsed={true} />
          </div>
        </TooltipProvider>
      </div>
    </div>
  );
};

export default Sidebar;
```

- [ ] **Step 2: Clean up layout.tsx**

In `frontend/src/app/layout.tsx`, the layout already works since `MainContent` was simplified in Task 1. No changes needed — the `<Sidebar />` and `<MainContent>` are already rendered side by side at line 250-252.

- [ ] **Step 3: Verify the frontend compiles**

Run: `cd frontend && pnpm run build 2>&1 | head -50`

Expected: builds successfully (the `/meetings` page doesn't exist yet, but no route depends on it).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/Sidebar/index.tsx
git commit -m "refactor: rewrite Sidebar as compact icon column — always 64px, no expand"
```

---

## Task 3: Create the `/meetings` page with meeting cards

**Files:**
- Create: `frontend/src/app/meetings/page.tsx`
- Create: `frontend/src/components/MeetingCard.tsx`

This task creates the meetings page **without search** — just the card grid showing all meetings. Search comes in Task 6.

- [ ] **Step 1: Create MeetingCard component**

Create `frontend/src/components/MeetingCard.tsx`:

```typescript
'use client';

import React, { useState } from 'react';
import { MoreHorizontal, Pencil, Trash2 } from 'lucide-react';

export interface MeetingCardData {
  id: string;
  title: string;
  created_at: string;
  duration_seconds: number | null;
  summary_preview: string | null;
}

interface MeetingCardProps {
  meeting: MeetingCardData;
  onClick: () => void;
  onRename: (meetingId: string, currentTitle: string) => void;
  onDelete: (meetingId: string) => void;
  searchSnippet?: string | null;
  highlightTerms?: string[];
}

function formatDate(isoDate: string): string {
  const date = new Date(isoDate);
  return date.toLocaleDateString('fr-FR', { day: 'numeric', month: 'short', year: 'numeric' });
}

function formatDuration(seconds: number | null): string {
  if (!seconds) return '';
  const mins = Math.round(seconds / 60);
  if (mins < 60) return `${mins} min`;
  const hours = Math.floor(mins / 60);
  const remainingMins = mins % 60;
  return remainingMins > 0 ? `${hours}h${String(remainingMins).padStart(2, '0')}` : `${hours}h`;
}

function highlightText(text: string, terms: string[]): React.ReactNode {
  if (!terms.length) return text;

  const escapedTerms = terms.map(t => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  const regex = new RegExp(`(${escapedTerms.join('|')})`, 'gi');
  const parts = text.split(regex);

  return parts.map((part, i) =>
    regex.test(part)
      ? <mark key={i} className="bg-yellow-200 rounded px-0.5">{part}</mark>
      : part
  );
}

export function MeetingCard({ meeting, onClick, onRename, onDelete, searchSnippet, highlightTerms = [] }: MeetingCardProps) {
  const [showMenu, setShowMenu] = useState(false);

  const previewText = searchSnippet || meeting.summary_preview || 'Pas de resume';

  return (
    <div
      onClick={onClick}
      className="bg-white rounded-xl border border-gray-200 p-4 hover:shadow-md hover:border-gray-300 transition-all duration-200 cursor-pointer group relative"
    >
      {/* Header: title + menu */}
      <div className="flex items-start justify-between mb-2">
        <h3 className="font-semibold text-gray-900 text-sm line-clamp-2 flex-1 mr-2">
          {meeting.title}
        </h3>
        <div className="relative">
          <button
            onClick={(e) => {
              e.stopPropagation();
              setShowMenu(!showMenu);
            }}
            className="p-1 rounded-md opacity-0 group-hover:opacity-100 hover:bg-gray-100 transition-all"
          >
            <MoreHorizontal className="w-4 h-4 text-gray-400" />
          </button>
          {showMenu && (
            <div className="absolute right-0 top-8 bg-white border border-gray-200 rounded-lg shadow-lg py-1 z-10 min-w-[140px]">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setShowMenu(false);
                  onRename(meeting.id, meeting.title);
                }}
                className="w-full flex items-center gap-2 px-3 py-2 text-sm text-gray-700 hover:bg-gray-50"
              >
                <Pencil className="w-3.5 h-3.5" /> Renommer
              </button>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setShowMenu(false);
                  onDelete(meeting.id);
                }}
                className="w-full flex items-center gap-2 px-3 py-2 text-sm text-red-600 hover:bg-red-50"
              >
                <Trash2 className="w-3.5 h-3.5" /> Supprimer
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Date + duration */}
      <p className="text-xs text-gray-500 mb-3">
        {formatDate(meeting.created_at)}
        {meeting.duration_seconds ? ` · ${formatDuration(meeting.duration_seconds)}` : ''}
      </p>

      {/* Preview */}
      <p className="text-sm text-gray-600 line-clamp-3">
        {highlightTerms.length > 0 ? highlightText(previewText, highlightTerms) : previewText}
      </p>
    </div>
  );
}
```

- [ ] **Step 2: Create the meetings page**

Create `frontend/src/app/meetings/page.tsx`:

```typescript
'use client';

import React, { useState, useEffect, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import { Search, LoaderIcon } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { MeetingCard, MeetingCardData } from '@/components/MeetingCard';
import { ConfirmationModal } from '@/components/ConfirmationModel/confirmation-modal';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { VisuallyHidden } from "@/components/ui/visually-hidden";

export default function MeetingsPage() {
  const router = useRouter();
  const { meetings, deleteMeeting, renameMeeting, setCurrentMeeting } = useSidebar();
  const [meetingCards, setMeetingCards] = useState<MeetingCardData[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  // Delete modal state
  const [deleteModal, setDeleteModal] = useState<{ isOpen: boolean; meetingId: string | null }>({ isOpen: false, meetingId: null });

  // Edit modal state
  const [editModal, setEditModal] = useState<{ isOpen: boolean; meetingId: string | null; currentTitle: string }>({
    isOpen: false, meetingId: null, currentTitle: ''
  });
  const [editingTitle, setEditingTitle] = useState('');

  // Fetch meeting card data (with created_at, duration, summary preview)
  const fetchMeetingCards = useCallback(async () => {
    setIsLoading(true);
    try {
      // For now, build cards from the meetings list + individual fetches
      // This will be replaced by a dedicated endpoint in Task 5
      const cards: MeetingCardData[] = meetings.map(m => ({
        id: m.id,
        title: m.title,
        created_at: '',
        duration_seconds: null,
        summary_preview: null,
      }));
      setMeetingCards(cards);
    } catch (error) {
      console.error('Error fetching meeting cards:', error);
    } finally {
      setIsLoading(false);
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
    if (deleteModal.meetingId) {
      try {
        await deleteMeeting(deleteModal.meetingId);
        toast.success('Meeting deleted successfully');
      } catch (error) {
        toast.error('Failed to delete meeting');
      }
    }
    setDeleteModal({ isOpen: false, meetingId: null });
  };

  const handleEditConfirm = async () => {
    const newTitle = editingTitle.trim();
    if (!editModal.meetingId || !newTitle) return;
    try {
      await renameMeeting(editModal.meetingId, newTitle);
      toast.success('Meeting title updated');
    } catch (error) {
      toast.error('Failed to update meeting title');
    }
    setEditModal({ isOpen: false, meetingId: null, currentTitle: '' });
    setEditingTitle('');
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-screen">
        <LoaderIcon className="animate-spin size-6" />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-screen bg-gray-50">
      {/* Sticky search bar */}
      <div className="sticky top-0 z-10 bg-gray-50 px-6 pt-6 pb-4">
        <div className="relative max-w-2xl mx-auto">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400" />
          <input
            type="text"
            placeholder="Rechercher dans les meetings..."
            className="w-full pl-10 pr-4 py-3 bg-white border border-gray-200 rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent shadow-sm"
            disabled
          />
        </div>
      </div>

      {/* Meeting cards grid */}
      <div className="flex-1 overflow-y-auto px-6 pb-6">
        {meetingCards.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-64 text-gray-500">
            <p className="text-lg font-medium">Aucun meeting</p>
            <p className="text-sm mt-1">Vos meetings apparaitront ici apres enregistrement.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4 max-w-6xl mx-auto">
            {meetingCards.map(meeting => (
              <MeetingCard
                key={meeting.id}
                meeting={meeting}
                onClick={() => handleCardClick(meeting)}
                onRename={(id, title) => {
                  setEditModal({ isOpen: true, meetingId: id, currentTitle: title });
                  setEditingTitle(title);
                }}
                onDelete={(id) => setDeleteModal({ isOpen: true, meetingId: id })}
              />
            ))}
          </div>
        )}
      </div>

      {/* Delete confirmation modal */}
      <ConfirmationModal
        isOpen={deleteModal.isOpen}
        text="Are you sure you want to delete this meeting? This action cannot be undone."
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteModal({ isOpen: false, meetingId: null })}
      />

      {/* Edit title modal */}
      <Dialog open={editModal.isOpen} onOpenChange={(open) => {
        if (!open) {
          setEditModal({ isOpen: false, meetingId: null, currentTitle: '' });
          setEditingTitle('');
        }
      }}>
        <DialogContent className="sm:max-w-[425px]">
          <VisuallyHidden>
            <DialogTitle>Edit Meeting Title</DialogTitle>
          </VisuallyHidden>
          <div className="py-4">
            <h3 className="text-lg font-semibold mb-4">Edit Meeting Title</h3>
            <input
              type="text"
              value={editingTitle}
              onChange={(e) => setEditingTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleEditConfirm();
                if (e.key === 'Escape') {
                  setEditModal({ isOpen: false, meetingId: null, currentTitle: '' });
                  setEditingTitle('');
                }
              }}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder="Enter meeting title"
              autoFocus
            />
          </div>
          <DialogFooter>
            <button
              onClick={() => { setEditModal({ isOpen: false, meetingId: null, currentTitle: '' }); setEditingTitle(''); }}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded-md"
            >
              Cancel
            </button>
            <button
              onClick={handleEditConfirm}
              className="px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md"
            >
              Save
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
```

- [ ] **Step 3: Verify the app compiles and the page renders**

Run: `cd frontend && pnpm run build 2>&1 | head -50`

Then start the dev server: `cd frontend && pnpm run dev` and navigate to `/meetings` in the app.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/app/meetings/page.tsx frontend/src/components/MeetingCard.tsx
git commit -m "feat: add /meetings page with card grid layout"
```

---

## Task 4: Backend — meeting cards endpoint with duration and summary preview

**Files:**
- Modify: `backend/app/db.py`
- Modify: `backend/app/main.py`

The meetings page needs `created_at`, `duration_seconds`, and `summary_preview` for each card. Currently `api_get_meetings` only returns `id` and `title`. We add a new backend endpoint that returns enriched meeting data.

- [ ] **Step 1: Add `get_meetings_with_details` to DatabaseManager in `backend/app/db.py`**

Add this method to the `DatabaseManager` class (after `search_transcripts`):

```python
async def get_meetings_with_details(self):
    """Get all meetings with created_at, duration, and summary preview"""
    try:
        async with self._get_connection() as conn:
            cursor = await conn.execute("""
                SELECT 
                    m.id, 
                    m.title, 
                    m.created_at,
                    (SELECT MAX(COALESCE(t.audio_end_time, 0)) FROM transcripts t WHERE t.meeting_id = m.id) as duration_seconds,
                    (SELECT sp.result FROM summary_processes sp WHERE sp.meeting_id = m.id AND sp.status = 'completed' LIMIT 1) as summary_result
                FROM meetings m
                ORDER BY m.created_at DESC
            """)
            rows = await cursor.fetchall()
            
            results = []
            for row in rows:
                meeting_id, title, created_at, duration_seconds, summary_result = row
                
                # Extract summary preview (first 150 chars of markdown)
                summary_preview = None
                if summary_result:
                    try:
                        summary_data = json.loads(summary_result) if isinstance(summary_result, str) else summary_result
                        if isinstance(summary_data, dict):
                            md = summary_data.get('markdown', '')
                            if md:
                                # Strip markdown headers and get first 150 chars of content
                                lines = [l.strip() for l in md.split('\n') if l.strip() and not l.strip().startswith('#')]
                                summary_preview = ' '.join(lines)[:150]
                    except (json.JSONDecodeError, AttributeError):
                        pass
                
                results.append({
                    'id': meeting_id,
                    'title': title,
                    'created_at': created_at or '',
                    'duration_seconds': duration_seconds,
                    'summary_preview': summary_preview,
                })
            
            return results
    except Exception as e:
        logger.error(f"Error getting meetings with details: {str(e)}")
        raise
```

- [ ] **Step 2: Add the endpoint in `backend/app/main.py`**

Add before the `/search-transcripts` endpoint (around line 620):

```python
@app.get("/get-meetings-cards")
async def get_meetings_cards():
    """Get all meetings with card display data (created_at, duration, summary preview)"""
    try:
        results = await db.get_meetings_with_details()
        return JSONResponse(content=results)
    except Exception as e:
        logger.error(f"Error getting meetings cards: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))
```

- [ ] **Step 3: Add Tauri command to call this endpoint**

Add to `frontend/src-tauri/src/api/api.rs`, after the `api_get_meetings` command:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct MeetingCardData {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub duration_seconds: Option<f64>,
    pub summary_preview: Option<String>,
}

#[tauri::command]
pub async fn api_get_meetings_cards<R: Runtime>(
    app: AppHandle<R>,
    auth_token: Option<String>,
) -> Result<Vec<MeetingCardData>, String> {
    log_info!("api_get_meetings_cards called");
    make_api_request::<R, Vec<MeetingCardData>>(&app, "/get-meetings-cards", "GET", None, None, auth_token)
        .await
}
```

Register in `frontend/src-tauri/src/lib.rs` — add `api::api_get_meetings_cards` to the `invoke_handler` list (after `api::api_get_meetings` at line 577).

- [ ] **Step 4: Update the meetings page to use the new endpoint**

In `frontend/src/app/meetings/page.tsx`, replace the `fetchMeetingCards` function:

```typescript
const fetchMeetingCards = useCallback(async () => {
  setIsLoading(true);
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
  } finally {
    setIsLoading(false);
  }
}, [meetings]);
```

Also update the import: remove `MeetingCardData` from the `MeetingCard` import (it's now fetched from the API), but keep using the same interface in `MeetingCard.tsx`.

- [ ] **Step 5: Verify the endpoint works**

Start the backend: `cd backend && python -m uvicorn app.main:app --port 5167`
Test: `curl http://localhost:5167/get-meetings-cards`

- [ ] **Step 6: Commit**

```bash
git add backend/app/db.py backend/app/main.py frontend/src-tauri/src/api/api.rs frontend/src-tauri/src/lib.rs frontend/src/app/meetings/page.tsx
git commit -m "feat: add meetings-cards endpoint with duration and summary preview"
```

---

## Task 5: Backend — hybrid search module (fuzzy + TF-IDF + semantic)

**Files:**
- Create: `backend/app/search.py`
- Modify: `backend/app/db.py`
- Modify: `backend/app/main.py`
- Modify: `backend/requirements.txt`

- [ ] **Step 1: Add dependencies**

Add to `backend/requirements.txt`:

```
rapidfuzz==3.12.2
numpy==2.2.6
httpx==0.28.1
```

Run: `cd backend && pip install rapidfuzz numpy httpx`

- [ ] **Step 2: Add the embeddings table migration**

Add to `backend/app/db.py`, in the `DatabaseManager.__init__` or table creation section, add the new table:

```python
# In the _init_db or create_tables method, add:
await conn.execute("""
    CREATE TABLE IF NOT EXISTS transcript_embeddings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id TEXT NOT NULL,
        transcript_id TEXT NOT NULL,
        chunk_text TEXT NOT NULL,
        embedding BLOB NOT NULL,
        FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
    )
""")
await conn.execute("""
    CREATE INDEX IF NOT EXISTS idx_transcript_embeddings_meeting 
    ON transcript_embeddings(meeting_id)
""")
```

- [ ] **Step 3: Create the search module**

Create `backend/app/search.py`:

```python
"""
Hybrid search module: fuzzy + TF-IDF + semantic (Ollama embeddings).
Degrades gracefully when Ollama is unavailable.
"""

import logging
import math
import struct
from collections import defaultdict
from typing import Optional

import numpy as np
from rapidfuzz import fuzz, process

logger = logging.getLogger(__name__)


class FuzzySearcher:
    """Fuzzy matching with rapidfuzz (Levenshtein-based)."""

    def search(self, query: str, transcripts: list[dict]) -> list[dict]:
        """
        Score each transcript chunk against the query using token_set_ratio.
        Returns list of {meeting_id, transcript_id, text, timestamp, score, highlight_start, highlight_end}.
        """
        results = []
        query_lower = query.lower()

        for t in transcripts:
            text = t["text"]
            text_lower = text.lower()
            
            # Use token_set_ratio for flexible matching (handles word order, partial matches)
            score = fuzz.token_set_ratio(query_lower, text_lower) / 100.0

            if score < 0.4:
                continue

            # Find best substring match position for highlighting
            highlight_start, highlight_end = self._find_best_match_position(text_lower, query_lower)

            results.append({
                "meeting_id": t["meeting_id"],
                "transcript_id": t["transcript_id"],
                "text": text,
                "timestamp": t.get("timestamp", ""),
                "score": score,
                "highlight_start": highlight_start,
                "highlight_end": highlight_end,
                "match_type": "fuzzy",
            })

        return results

    def _find_best_match_position(self, text: str, query: str) -> tuple[int, int]:
        """Find the position of the best fuzzy substring match."""
        # Try exact substring first
        idx = text.find(query)
        if idx >= 0:
            return idx, idx + len(query)

        # Try each word from query
        words = query.split()
        for word in words:
            idx = text.find(word)
            if idx >= 0:
                return idx, idx + len(word)

        return 0, min(len(query), len(text))


class TFIDFSearcher:
    """TF-IDF ranking for keyword-based relevance."""

    def search(self, query: str, transcripts: list[dict], idf_scores: dict[str, float]) -> list[dict]:
        """Score transcripts using TF-IDF."""
        results = []
        query_terms = query.lower().split()

        for t in transcripts:
            text = t["text"]
            text_lower = text.lower()
            words = text_lower.split()
            word_count = len(words) if words else 1

            score = 0.0
            best_start, best_end = 0, 0
            best_term_score = 0.0

            for term in query_terms:
                tf = words.count(term) / word_count
                idf = idf_scores.get(term, 1.0)
                term_score = tf * idf
                score += term_score

                # Track best match position for highlighting
                idx = text_lower.find(term)
                if idx >= 0 and term_score > best_term_score:
                    best_start = idx
                    best_end = idx + len(term)
                    best_term_score = term_score

            if score < 0.01:
                continue

            results.append({
                "meeting_id": t["meeting_id"],
                "transcript_id": t["transcript_id"],
                "text": text,
                "timestamp": t.get("timestamp", ""),
                "score": score,
                "highlight_start": best_start,
                "highlight_end": best_end,
                "match_type": "tfidf",
            })

        return results

    @staticmethod
    def compute_idf(transcripts: list[dict]) -> dict[str, float]:
        """Compute IDF scores across all transcripts."""
        doc_count = len(transcripts)
        if doc_count == 0:
            return {}

        term_doc_freq: dict[str, int] = defaultdict(int)
        for t in transcripts:
            unique_words = set(t["text"].lower().split())
            for word in unique_words:
                term_doc_freq[word] += 1

        return {
            term: math.log((doc_count + 1) / (freq + 1)) + 1
            for term, freq in term_doc_freq.items()
        }


class SemanticSearcher:
    """Semantic search using Ollama embeddings."""

    def __init__(self, ollama_base_url: str = "http://localhost:11434"):
        self.ollama_base_url = ollama_base_url
        self.model = "nomic-embed-text"

    async def get_embedding(self, text: str) -> Optional[list[float]]:
        """Get embedding vector from Ollama."""
        import httpx
        try:
            async with httpx.AsyncClient(timeout=30.0) as client:
                response = await client.post(
                    f"{self.ollama_base_url}/api/embed",
                    json={"model": self.model, "input": text},
                )
                if response.status_code == 200:
                    data = response.json()
                    embeddings = data.get("embeddings", [])
                    if embeddings:
                        return embeddings[0]
                return None
        except Exception as e:
            logger.debug(f"Ollama embedding failed: {e}")
            return None

    async def is_available(self) -> bool:
        """Check if Ollama is running and the embedding model is available."""
        import httpx
        try:
            async with httpx.AsyncClient(timeout=5.0) as client:
                response = await client.get(f"{self.ollama_base_url}/api/tags")
                if response.status_code == 200:
                    models = response.json().get("models", [])
                    return any(m.get("name", "").startswith(self.model) for m in models)
                return False
        except Exception:
            return False

    def search_embeddings(
        self,
        query_embedding: list[float],
        stored_embeddings: list[dict],
    ) -> list[dict]:
        """Compare query embedding against stored embeddings using cosine similarity."""
        if not query_embedding or not stored_embeddings:
            return []

        query_vec = np.array(query_embedding)
        query_norm = np.linalg.norm(query_vec)
        if query_norm == 0:
            return []

        results = []
        for entry in stored_embeddings:
            stored_vec = np.frombuffer(entry["embedding"], dtype=np.float32)
            stored_norm = np.linalg.norm(stored_vec)
            if stored_norm == 0:
                continue

            similarity = float(np.dot(query_vec, stored_vec) / (query_norm * stored_norm))

            if similarity < 0.3:
                continue

            results.append({
                "meeting_id": entry["meeting_id"],
                "transcript_id": entry["transcript_id"],
                "text": entry["chunk_text"],
                "timestamp": "",
                "score": similarity,
                "highlight_start": 0,
                "highlight_end": 0,
                "match_type": "semantic",
            })

        return results


class HybridSearchOrchestrator:
    """Orchestrates fuzzy, TF-IDF, and semantic search with weighted score fusion."""

    WEIGHT_FUZZY = 0.2
    WEIGHT_TFIDF = 0.3
    WEIGHT_SEMANTIC = 0.5

    def __init__(self):
        self.fuzzy = FuzzySearcher()
        self.tfidf = TFIDFSearcher()
        self.semantic = SemanticSearcher()

    async def search(
        self,
        query: str,
        transcripts: list[dict],
        stored_embeddings: list[dict],
        limit: int = 20,
    ) -> list[dict]:
        """
        Run all search layers and merge results with weighted scoring.
        Returns results grouped by meeting_id.
        """
        # 1. Fuzzy search
        fuzzy_results = self.fuzzy.search(query, transcripts)

        # 2. TF-IDF search
        idf_scores = TFIDFSearcher.compute_idf(transcripts)
        tfidf_results = self.tfidf.search(query, transcripts, idf_scores)

        # 3. Semantic search (graceful degradation)
        semantic_results = []
        if await self.semantic.is_available():
            query_embedding = await self.semantic.get_embedding(query)
            if query_embedding:
                semantic_results = self.semantic.search_embeddings(query_embedding, stored_embeddings)
        else:
            logger.info("Ollama not available — running search without semantic layer")

        # Merge results by (meeting_id, transcript_id) with weighted scores
        merged = self._merge_results(fuzzy_results, tfidf_results, semantic_results)

        # Group by meeting and sort
        meetings = self._group_by_meeting(merged, transcripts, limit)

        return meetings

    def _merge_results(
        self,
        fuzzy_results: list[dict],
        tfidf_results: list[dict],
        semantic_results: list[dict],
    ) -> dict[tuple[str, str], dict]:
        """Merge results from all layers using weighted scores."""
        merged: dict[tuple[str, str], dict] = {}

        has_semantic = len(semantic_results) > 0
        w_fuzzy = self.WEIGHT_FUZZY if has_semantic else 0.4
        w_tfidf = self.WEIGHT_TFIDF if has_semantic else 0.6
        w_semantic = self.WEIGHT_SEMANTIC if has_semantic else 0.0

        for r in fuzzy_results:
            key = (r["meeting_id"], r["transcript_id"])
            if key not in merged:
                merged[key] = {**r, "score": 0.0}
            merged[key]["score"] += r["score"] * w_fuzzy
            if r["highlight_start"] > 0 or merged[key]["highlight_start"] == 0:
                merged[key]["highlight_start"] = r["highlight_start"]
                merged[key]["highlight_end"] = r["highlight_end"]

        for r in tfidf_results:
            key = (r["meeting_id"], r["transcript_id"])
            if key not in merged:
                merged[key] = {**r, "score": 0.0}
            merged[key]["score"] += r["score"] * w_tfidf
            if r["highlight_start"] > 0 and merged[key].get("match_type") != "fuzzy":
                merged[key]["highlight_start"] = r["highlight_start"]
                merged[key]["highlight_end"] = r["highlight_end"]

        for r in semantic_results:
            key = (r["meeting_id"], r["transcript_id"])
            if key not in merged:
                merged[key] = {**r, "score": 0.0}
            merged[key]["score"] += r["score"] * w_semantic
            merged[key]["match_type"] = "semantic" if r["score"] > 0.5 else merged[key].get("match_type", "semantic")

        return merged

    def _group_by_meeting(
        self,
        merged: dict[tuple[str, str], dict],
        transcripts: list[dict],
        limit: int,
    ) -> list[dict]:
        """Group matches by meeting_id and return top meetings."""
        # Build title lookup
        title_lookup = {}
        for t in transcripts:
            title_lookup[t["meeting_id"]] = t.get("title", "")

        meetings_map: dict[str, dict] = {}
        for (meeting_id, _), match in merged.items():
            if meeting_id not in meetings_map:
                meetings_map[meeting_id] = {
                    "meeting_id": meeting_id,
                    "title": title_lookup.get(meeting_id, ""),
                    "score": 0.0,
                    "matches": [],
                }
            meetings_map[meeting_id]["matches"].append({
                "transcript_id": match["transcript_id"],
                "text": match["text"],
                "timestamp": match["timestamp"],
                "highlight_start": match["highlight_start"],
                "highlight_end": match["highlight_end"],
                "match_type": match["match_type"],
            })
            meetings_map[meeting_id]["score"] = max(
                meetings_map[meeting_id]["score"], match["score"]
            )

        # Sort by score descending
        sorted_meetings = sorted(meetings_map.values(), key=lambda m: m["score"], reverse=True)

        return sorted_meetings[:limit]
```

- [ ] **Step 4: Add search endpoint to `backend/app/main.py`**

Add before the existing `/search-transcripts` endpoint:

```python
from search import HybridSearchOrchestrator

search_orchestrator = HybridSearchOrchestrator()

class SearchMeetingsRequest(BaseModel):
    query: str
    limit: int = 20

@app.post("/search-meetings")
async def search_meetings(request: SearchMeetingsRequest):
    """Hybrid search across meeting transcripts (fuzzy + TF-IDF + semantic)"""
    try:
        # Fetch all transcripts for search
        async with db._get_connection() as conn:
            cursor = await conn.execute("""
                SELECT t.id, t.meeting_id, m.title, t.transcript, t.timestamp
                FROM transcripts t
                JOIN meetings m ON m.id = t.meeting_id
            """)
            rows = await cursor.fetchall()
            
            transcripts = [
                {
                    "transcript_id": row[0],
                    "meeting_id": row[1],
                    "title": row[2],
                    "text": row[3],
                    "timestamp": row[4],
                }
                for row in rows
            ]

            # Fetch stored embeddings
            emb_cursor = await conn.execute("""
                SELECT meeting_id, transcript_id, chunk_text, embedding
                FROM transcript_embeddings
            """)
            emb_rows = await emb_cursor.fetchall()
            
            stored_embeddings = [
                {
                    "meeting_id": row[0],
                    "transcript_id": row[1],
                    "chunk_text": row[2],
                    "embedding": row[3],
                }
                for row in emb_rows
            ]

        results = await search_orchestrator.search(
            query=request.query,
            transcripts=transcripts,
            stored_embeddings=stored_embeddings,
            limit=request.limit,
        )
        return JSONResponse(content=results)
    except Exception as e:
        logger.error(f"Error in hybrid search: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))
```

- [ ] **Step 5: Verify the endpoint**

Run: `cd backend && pip install rapidfuzz numpy && python -m uvicorn app.main:app --port 5167`
Test: `curl -X POST http://localhost:5167/search-meetings -H "Content-Type: application/json" -d '{"query": "test"}'`

- [ ] **Step 6: Commit**

```bash
git add backend/app/search.py backend/app/main.py backend/app/db.py backend/requirements.txt
git commit -m "feat: add hybrid search module (fuzzy + TF-IDF + semantic via Ollama)"
```

---

## Task 6: Frontend — wire search into meetings page

**Files:**
- Create: `frontend/src/hooks/useSearchMeetings.ts`
- Modify: `frontend/src/app/meetings/page.tsx`
- Modify: `frontend/src-tauri/src/api/api.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Add Tauri command for search**

Add to `frontend/src-tauri/src/api/api.rs`:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMatch {
    pub transcript_id: String,
    pub text: String,
    pub timestamp: String,
    pub highlight_start: usize,
    pub highlight_end: usize,
    pub match_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMeetingResult {
    pub meeting_id: String,
    pub title: String,
    pub score: f64,
    pub matches: Vec<SearchMatch>,
}

#[tauri::command]
pub async fn api_search_meetings<R: Runtime>(
    app: AppHandle<R>,
    query: String,
    limit: Option<u32>,
    auth_token: Option<String>,
) -> Result<Vec<SearchMeetingResult>, String> {
    log_info!("api_search_meetings called with query: '{}'", query);
    let body = serde_json::json!({
        "query": query,
        "limit": limit.unwrap_or(20)
    });
    make_api_request::<R, Vec<SearchMeetingResult>>(
        &app, "/search-meetings", "POST", Some(&body.to_string()), None, auth_token
    ).await
}
```

Register `api::api_search_meetings` in the `invoke_handler` in `frontend/src-tauri/src/lib.rs`.

- [ ] **Step 2: Create the search hook**

Create `frontend/src/hooks/useSearchMeetings.ts`:

```typescript
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
  setQuery: (query: string) => void;
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

  return { query, setQuery, results, isSearching, search, clearSearch };
}
```

- [ ] **Step 3: Wire search into the meetings page**

Update `frontend/src/app/meetings/page.tsx` to integrate the search hook. Key changes:

1. Import and use `useSearchMeetings`
2. Enable the search input (remove `disabled`)
3. When search is active, filter cards to show only matching meetings with search snippets
4. When clicking a card during search, include search params in the URL

Replace the search input section and add filtering logic:

```typescript
// Add imports
import { useSearchMeetings, SearchMeetingResult } from '@/hooks/useSearchMeetings';

// Inside the component, after other hooks:
const { query: searchQuery, results: searchResults, isSearching, search, clearSearch } = useSearchMeetings();

// Determine which meetings to display
const displayedMeetings = searchQuery.trim()
  ? meetingCards.filter(card => searchResults.some(r => r.meeting_id === card.id))
  : meetingCards;

// Helper to get search snippet for a card
const getSearchSnippet = (meetingId: string): string | null => {
  const result = searchResults.find(r => r.meeting_id === meetingId);
  if (!result || !result.matches.length) return null;
  const match = result.matches[0];
  const start = Math.max(0, match.highlight_start - 50);
  const end = Math.min(match.text.length, match.highlight_end + 50);
  let snippet = match.text.slice(start, end);
  if (start > 0) snippet = '...' + snippet;
  if (end < match.text.length) snippet += '...';
  return snippet;
};

// Update handleCardClick for search navigation
const handleCardClick = (meeting: MeetingCardData) => {
  setCurrentMeeting({ id: meeting.id, title: meeting.title });
  if (searchQuery.trim()) {
    const result = searchResults.find(r => r.meeting_id === meeting.id);
    const match = result?.matches[0];
    if (match) {
      router.push(`/meeting-details?id=${meeting.id}&search=${encodeURIComponent(searchQuery)}&transcript_id=${match.transcript_id}&highlight_start=${match.highlight_start}&highlight_end=${match.highlight_end}`);
      return;
    }
  }
  router.push(`/meeting-details?id=${meeting.id}`);
};
```

Update the search input JSX to be functional:

```tsx
<input
  type="text"
  placeholder="Rechercher dans les meetings..."
  value={searchQuery}
  onChange={(e) => search(e.target.value)}
  className="w-full pl-10 pr-10 py-3 bg-white border border-gray-200 rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent shadow-sm"
/>
{searchQuery && (
  <button
    onClick={clearSearch}
    className="absolute right-3 top-1/2 -translate-y-1/2 p-1 hover:bg-gray-100 rounded-full"
  >
    <X className="w-4 h-4 text-gray-400" />
  </button>
)}
{isSearching && (
  <LoaderIcon className="absolute right-10 top-1/2 -translate-y-1/2 w-4 h-4 animate-spin text-gray-400" />
)}
```

Update the cards grid to use `displayedMeetings` and pass search props:

```tsx
{displayedMeetings.map(meeting => (
  <MeetingCard
    key={meeting.id}
    meeting={meeting}
    onClick={() => handleCardClick(meeting)}
    onRename={...}
    onDelete={...}
    searchSnippet={searchQuery.trim() ? getSearchSnippet(meeting.id) : null}
    highlightTerms={searchQuery.trim() ? searchQuery.split(/\s+/) : []}
  />
))}
```

- [ ] **Step 4: Verify search works end-to-end**

Start backend + frontend, type a search query, verify:
1. Cards filter to matching meetings
2. Search snippets appear on cards
3. Loading indicator shows during search

- [ ] **Step 5: Commit**

```bash
git add frontend/src/hooks/useSearchMeetings.ts frontend/src/app/meetings/page.tsx frontend/src-tauri/src/api/api.rs frontend/src-tauri/src/lib.rs
git commit -m "feat: wire hybrid search into meetings page with debounced search and snippets"
```

---

## Task 7: Search banner and scroll-to-highlight on meeting-details

**Files:**
- Create: `frontend/src/components/MeetingDetails/SearchBanner.tsx`
- Modify: `frontend/src/app/meeting-details/page.tsx`
- Modify: `frontend/src/app/meeting-details/page-content.tsx`
- Modify: `frontend/src/components/MeetingDetails/TranscriptPanel.tsx`

- [ ] **Step 1: Create SearchBanner component**

Create `frontend/src/components/MeetingDetails/SearchBanner.tsx`:

```typescript
'use client';

import React from 'react';
import { ArrowLeft, ChevronLeft, ChevronRight, X, Search } from 'lucide-react';
import { useRouter } from 'next/navigation';

interface SearchBannerProps {
  searchTerm: string;
  currentIndex: number;
  totalMatches: number;
  onPrev: () => void;
  onNext: () => void;
  onClose: () => void;
}

export function SearchBanner({
  searchTerm,
  currentIndex,
  totalMatches,
  onPrev,
  onNext,
  onClose,
}: SearchBannerProps) {
  const router = useRouter();

  return (
    <div className="sticky top-0 z-20 bg-yellow-50 border-b border-yellow-200 px-4 py-2 flex items-center gap-3">
      <button
        onClick={() => router.push('/meetings')}
        className="flex items-center gap-1 text-sm text-gray-600 hover:text-gray-800 transition-colors"
      >
        <ArrowLeft className="w-4 h-4" />
        <span>Retour</span>
      </button>

      <div className="flex items-center gap-2 ml-4">
        <Search className="w-4 h-4 text-yellow-600" />
        <span className="text-sm font-medium text-yellow-800">"{searchTerm}"</span>
      </div>

      <div className="flex items-center gap-1 ml-4">
        <button
          onClick={onPrev}
          disabled={currentIndex <= 0}
          className="p-1 rounded hover:bg-yellow-100 disabled:opacity-30 disabled:cursor-not-allowed"
        >
          <ChevronLeft className="w-4 h-4" />
        </button>
        <span className="text-sm text-gray-600 min-w-[3rem] text-center">
          {totalMatches > 0 ? `${currentIndex + 1}/${totalMatches}` : '0/0'}
        </span>
        <button
          onClick={onNext}
          disabled={currentIndex >= totalMatches - 1}
          className="p-1 rounded hover:bg-yellow-100 disabled:opacity-30 disabled:cursor-not-allowed"
        >
          <ChevronRight className="w-4 h-4" />
        </button>
      </div>

      <button
        onClick={onClose}
        className="ml-auto p-1 rounded hover:bg-yellow-100"
      >
        <X className="w-4 h-4 text-gray-500" />
      </button>
    </div>
  );
}
```

- [ ] **Step 2: Update meeting-details/page.tsx to read search params**

In `frontend/src/app/meeting-details/page.tsx`, extract search params and pass to PageContent. In the `MeetingDetailsContent` component, add:

```typescript
// After existing searchParams extraction (line 23-24):
const searchTerm = searchParams.get('search');
const searchTranscriptId = searchParams.get('transcript_id');
const highlightStart = searchParams.get('highlight_start') ? parseInt(searchParams.get('highlight_start')!) : undefined;
const highlightEnd = searchParams.get('highlight_end') ? parseInt(searchParams.get('highlight_end')!) : undefined;
```

Pass these to `PageContent` (add to the JSX around line 359):

```typescript
<PageContent
  // ... existing props ...
  searchTerm={searchTerm}
  searchTranscriptId={searchTranscriptId}
  highlightStart={highlightStart}
  highlightEnd={highlightEnd}
/>
```

- [ ] **Step 3: Update page-content.tsx to show SearchBanner and handle highlighting**

In `frontend/src/app/meeting-details/page-content.tsx`, add the search banner and highlight management:

1. Add new props to `PageContent`:

```typescript
searchTerm?: string | null;
searchTranscriptId?: string | null;
highlightStart?: number;
highlightEnd?: number;
```

2. Add state and logic for search navigation:

```typescript
const [showSearchBanner, setShowSearchBanner] = useState(!!searchTerm);
const [currentMatchIndex, setCurrentMatchIndex] = useState(0);

// Find all occurrences of searchTerm in transcripts
const searchMatches = useMemo(() => {
  if (!searchTerm) return [];
  const term = searchTerm.toLowerCase();
  const matches: { segmentIndex: number; start: number; end: number }[] = [];
  const allSegments = segments || meetingData.transcripts.map((t, i) => ({ ...t, index: i }));
  
  allSegments.forEach((segment, idx) => {
    const text = (segment.text || '').toLowerCase();
    let pos = 0;
    while ((pos = text.indexOf(term, pos)) !== -1) {
      matches.push({ segmentIndex: idx, start: pos, end: pos + term.length });
      pos += term.length;
    }
  });
  
  return matches;
}, [searchTerm, segments, meetingData.transcripts]);

const handleCloseSearch = () => {
  setShowSearchBanner(false);
  // Remove search params from URL without navigation
  const url = new URL(window.location.href);
  url.searchParams.delete('search');
  url.searchParams.delete('transcript_id');
  url.searchParams.delete('highlight_start');
  url.searchParams.delete('highlight_end');
  window.history.replaceState({}, '', url.toString());
};
```

3. Add the `SearchBanner` to the JSX (before the flex container):

```typescript
import { SearchBanner } from '@/components/MeetingDetails/SearchBanner';

// In the return JSX, before the <div className="flex flex-1 overflow-hidden">:
{showSearchBanner && searchTerm && (
  <SearchBanner
    searchTerm={searchTerm}
    currentIndex={currentMatchIndex}
    totalMatches={searchMatches.length}
    onPrev={() => setCurrentMatchIndex(prev => Math.max(0, prev - 1))}
    onNext={() => setCurrentMatchIndex(prev => Math.min(searchMatches.length - 1, prev + 1))}
    onClose={handleCloseSearch}
  />
)}
```

4. Pass highlight props to TranscriptPanel:

```typescript
<TranscriptPanel
  // ... existing props ...
  searchTerm={showSearchBanner ? searchTerm : undefined}
  activeMatchIndex={currentMatchIndex}
  searchMatches={searchMatches}
/>
```

- [ ] **Step 4: Update TranscriptPanel to accept and forward highlight props**

In `frontend/src/components/MeetingDetails/TranscriptPanel.tsx`, add highlight props to the interface and pass them through to `VirtualizedTranscriptView`:

```typescript
// Add to TranscriptPanelProps:
searchTerm?: string | null;
activeMatchIndex?: number;
searchMatches?: { segmentIndex: number; start: number; end: number }[];
```

Pass them to `VirtualizedTranscriptView`:

```typescript
<VirtualizedTranscriptView
  // ... existing props ...
  searchTerm={searchTerm}
  activeMatchIndex={activeMatchIndex}
  searchMatches={searchMatches}
/>
```

Note: The actual text highlighting within `VirtualizedTranscriptView` will require modifications to the transcript rendering to wrap matched text in `<mark>` tags. The approach is the same as `highlightText` in `MeetingCard.tsx` — split text on search term matches and wrap them in `<mark className="bg-yellow-200">` (or `bg-yellow-400` for the active match).

- [ ] **Step 5: Verify the search banner and basic highlighting work**

Navigate from the meetings page with a search term, verify:
1. The search banner appears at the top
2. The ◄ ► buttons update the counter
3. The "Retour" button goes back to `/meetings`
4. The ✕ button closes the banner

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/MeetingDetails/SearchBanner.tsx frontend/src/app/meeting-details/page.tsx frontend/src/app/meeting-details/page-content.tsx frontend/src/components/MeetingDetails/TranscriptPanel.tsx
git commit -m "feat: add search banner and scroll-to-highlight on meeting-details page"
```

---

## Task 8: Backend — embedding indexation on transcript save

**Files:**
- Modify: `backend/app/db.py`
- Modify: `backend/app/main.py`

When a new transcript is saved, compute and store embeddings in the background.

- [ ] **Step 1: Add embedding indexation method to DatabaseManager**

Add to `backend/app/db.py`:

```python
async def index_transcript_embeddings(self, meeting_id: str):
    """Compute and store embeddings for a meeting's transcripts using Ollama."""
    import httpx
    import numpy as np
    
    try:
        # Check if Ollama is available
        async with httpx.AsyncClient(timeout=5.0) as client:
            try:
                resp = await client.get("http://localhost:11434/api/tags")
                if resp.status_code != 200:
                    logger.info("Ollama not available, skipping embedding indexation")
                    return
            except Exception:
                logger.info("Ollama not available, skipping embedding indexation")
                return

        async with self._get_connection() as conn:
            # Get all transcripts for the meeting
            cursor = await conn.execute(
                "SELECT id, transcript FROM transcripts WHERE meeting_id = ?",
                (meeting_id,)
            )
            rows = await cursor.fetchall()

            if not rows:
                return

            # Delete existing embeddings for this meeting
            await conn.execute(
                "DELETE FROM transcript_embeddings WHERE meeting_id = ?",
                (meeting_id,)
            )

            # Chunk and embed each transcript
            async with httpx.AsyncClient(timeout=60.0) as client:
                for transcript_id, text in rows:
                    # Split into chunks of ~200 tokens (roughly 800 chars) with 50 token overlap (~200 chars)
                    chunks = self._chunk_text(text, chunk_size=800, overlap=200)
                    
                    for chunk in chunks:
                        if not chunk.strip():
                            continue
                        
                        try:
                            resp = await client.post(
                                "http://localhost:11434/api/embed",
                                json={"model": "nomic-embed-text", "input": chunk},
                            )
                            if resp.status_code == 200:
                                data = resp.json()
                                embeddings = data.get("embeddings", [])
                                if embeddings:
                                    embedding_bytes = np.array(embeddings[0], dtype=np.float32).tobytes()
                                    await conn.execute(
                                        "INSERT INTO transcript_embeddings (meeting_id, transcript_id, chunk_text, embedding) VALUES (?, ?, ?, ?)",
                                        (meeting_id, transcript_id, chunk, embedding_bytes),
                                    )
                        except Exception as e:
                            logger.warning(f"Failed to embed chunk for transcript {transcript_id}: {e}")
                            continue

            await conn.commit()
            logger.info(f"Indexed embeddings for meeting {meeting_id}")

    except Exception as e:
        logger.error(f"Error indexing embeddings for meeting {meeting_id}: {e}")

@staticmethod
def _chunk_text(text: str, chunk_size: int = 800, overlap: int = 200) -> list[str]:
    """Split text into overlapping chunks."""
    if len(text) <= chunk_size:
        return [text]
    
    chunks = []
    start = 0
    while start < len(text):
        end = start + chunk_size
        chunks.append(text[start:end])
        start += chunk_size - overlap
    
    return chunks
```

- [ ] **Step 2: Trigger embedding indexation after transcript save**

In `backend/app/main.py`, find the endpoint that saves transcripts (the `save-transcripts` or `save-meeting` endpoint). Add a background task to index embeddings:

```python
# In the save-transcripts endpoint handler, after successful save:
background_tasks.add_task(db.index_transcript_embeddings, meeting_id)
```

If the endpoint doesn't already accept `BackgroundTasks`, add it as a parameter:

```python
@app.post("/save-transcripts")
async def save_transcripts(request: SaveTranscriptRequest, background_tasks: BackgroundTasks):
    # ... existing save logic ...
    background_tasks.add_task(db.index_transcript_embeddings, meeting_id)
    return result
```

- [ ] **Step 3: Add the embeddings table creation**

In `backend/app/db.py`, in the database initialization (where other tables are created), add:

```python
await conn.execute("""
    CREATE TABLE IF NOT EXISTS transcript_embeddings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id TEXT NOT NULL,
        transcript_id TEXT NOT NULL,
        chunk_text TEXT NOT NULL,
        embedding BLOB NOT NULL,
        FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
    )
""")
```

- [ ] **Step 4: Verify embedding indexation**

1. Start Ollama: `ollama serve`
2. Pull the embedding model: `ollama pull nomic-embed-text`
3. Start the backend, save a transcript, check the logs for "Indexed embeddings for meeting..."
4. Verify: `sqlite3 backend/meetings.db "SELECT COUNT(*) FROM transcript_embeddings;"`

- [ ] **Step 5: Commit**

```bash
git add backend/app/db.py backend/app/main.py
git commit -m "feat: add background embedding indexation on transcript save via Ollama"
```

---

## Task 9: End-to-end verification and cleanup

**Files:**
- Various cleanup across modified files

- [ ] **Step 1: Verify the complete flow**

1. Start Ollama: `ollama serve`
2. Start backend: `cd backend && python -m uvicorn app.main:app --port 5167`
3. Start frontend: `cd frontend && pnpm run tauri:dev`
4. Navigate to `/meetings` — verify cards display correctly
5. Type a search query — verify cards filter and show snippets
6. Click a card during search — verify navigation to meeting-details with search banner
7. Use ◄ ► to navigate between occurrences
8. Click "Retour" — verify return to meetings page with search preserved

- [ ] **Step 2: Remove the old `/search-transcripts` endpoint**

In `backend/app/main.py`, remove or deprecate the old endpoint (lines 620-631):

```python
# Remove:
# class SearchRequest
# @app.post("/search-transcripts")
# async def search_transcripts(request: SearchRequest):
```

Note: Keep `api_search_transcripts` in the Rust code for now (it may be used elsewhere). Mark it as deprecated with a comment.

- [ ] **Step 3: Final compile check**

Run: `cd frontend && pnpm run build 2>&1 | tail -20`

Expected: Build succeeds with no errors.

- [ ] **Step 4: Commit cleanup**

```bash
git add -A
git commit -m "chore: cleanup — remove old search-transcripts endpoint, final verification"
```
