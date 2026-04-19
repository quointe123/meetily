'use client';

import React, { useState, useRef, useEffect } from 'react';
import { MoreHorizontal, Pencil, Trash2, CalendarDays, Clock3 } from 'lucide-react';
import { findMatchSpans } from '@/lib/fuzzyMatch';

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
  /** Rendered at the bottom of the card — typically the match-kind badges during search. */
  badges?: React.ReactNode;
}

const SAME_DAY_MS = 24 * 60 * 60 * 1000;

function formatDate(isoDate: string): { label: string; isRelative: boolean } {
  if (!isoDate) return { label: '', isRelative: false };
  try {
    const date = new Date(isoDate);
    const now = new Date();
    const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
    const daysAgo = Math.round((startOfToday - startOfDate) / SAME_DAY_MS);
    if (daysAgo === 0) return { label: "Aujourd'hui", isRelative: true };
    if (daysAgo === 1) return { label: 'Hier', isRelative: true };
    if (daysAgo > 1 && daysAgo < 7) return { label: `Il y a ${daysAgo} jours`, isRelative: true };
    return {
      label: date.toLocaleDateString('fr-FR', {
        day: 'numeric',
        month: 'short',
        year: 'numeric',
      }),
      isRelative: false,
    };
  } catch {
    return { label: '', isRelative: false };
  }
}

function formatDuration(seconds: number | null): string {
  if (seconds == null || seconds <= 0) return '';
  const minutes = Math.round(seconds / 60);
  if (minutes < 1) return '< 1 min';
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m > 0 ? `${h} h ${m.toString().padStart(2, '0')}` : `${h} h`;
}

function highlightText(text: string, terms: string[]): React.ReactNode[] {
  if (!terms || terms.length === 0) return [text];
  const spans = findMatchSpans(text, terms);
  if (spans.length === 0) return [text];
  const parts: React.ReactNode[] = [];
  let cursor = 0;
  spans.forEach((span, i) => {
    if (span.start > cursor) parts.push(text.slice(cursor, span.start));
    parts.push(
      <mark
        key={i}
        className="bg-amber-100 text-amber-950 rounded-sm px-0.5 -mx-0.5"
      >
        {text.slice(span.start, span.end)}
      </mark>,
    );
    cursor = span.end;
  });
  if (cursor < text.length) parts.push(text.slice(cursor));
  return parts;
}

export function MeetingCard({
  meeting,
  onClick,
  onRename,
  onDelete,
  searchSnippet,
  highlightTerms,
  badges,
}: MeetingCardProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [menuOpen]);

  const { label: dateLabel, isRelative } = formatDate(meeting.created_at);
  const durationStr = formatDuration(meeting.duration_seconds);
  const previewText = searchSnippet ?? meeting.summary_preview;
  const isSearchSnippet = Boolean(searchSnippet);
  const hasHighlight = Boolean(highlightTerms && highlightTerms.length > 0);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLElement>) => {
    if (e.target !== e.currentTarget) return;
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onClick();
    }
  };

  return (
    <article
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={handleKeyDown}
      className="group relative flex h-full flex-col overflow-hidden rounded-lg border border-gray-200/80 bg-white cursor-pointer outline-none transition-[box-shadow,border-color,transform] duration-200 ease-out hover:border-gray-300 hover:shadow-[0_4px_16px_-4px_rgb(0_0_0/0.06),0_2px_4px_-2px_rgb(0_0_0/0.04)] focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:ring-offset-2 focus-visible:ring-offset-gray-50"
    >
      {/* Left accent rule — neutral at rest, amber + thicker on hover */}
      <span
        aria-hidden
        className="absolute inset-y-0 left-0 w-[2px] bg-gray-200 transition-[width,background-color] duration-200 ease-out group-hover:w-[3px] group-hover:bg-amber-400"
      />

      {/* Header */}
      <header className="relative pl-5 pr-10 pt-4 pb-3 border-b border-gray-100">
        <h3 className="text-[15px] font-semibold leading-snug tracking-[-0.005em] text-gray-900 line-clamp-2">
          {hasHighlight ? highlightText(meeting.title, highlightTerms!) : meeting.title}
        </h3>

        <div className="mt-2.5 flex items-center gap-2.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-gray-500 tabular-nums">
          {dateLabel && (
            <span
              className={`inline-flex items-center gap-1.5 ${
                isRelative ? 'text-amber-700' : 'text-gray-500'
              }`}
            >
              <CalendarDays className="h-3 w-3" strokeWidth={2} aria-hidden />
              <span>{dateLabel}</span>
            </span>
          )}
          {dateLabel && durationStr && (
            <span aria-hidden className="h-1 w-1 rounded-full bg-gray-300" />
          )}
          {durationStr && (
            <span className="inline-flex items-center gap-1.5">
              <Clock3 className="h-3 w-3" strokeWidth={2} aria-hidden />
              <span>{durationStr}</span>
            </span>
          )}
        </div>

        {/* Menu button */}
        <div ref={menuRef} className="absolute right-2 top-3">
          <button
            type="button"
            aria-label="Actions sur ce meeting"
            className="flex h-7 w-7 items-center justify-center rounded-md text-gray-400 opacity-0 transition-[opacity,background-color,color] duration-150 hover:bg-gray-100 hover:text-gray-600 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-300 group-hover:opacity-100"
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen((v) => !v);
            }}
          >
            <MoreHorizontal className="h-4 w-4" />
          </button>

          {menuOpen && (
            <div
              className="animate-fade-in absolute right-0 mt-1 w-44 rounded-md border border-gray-200 bg-white py-1 shadow-lg z-10"
              role="menu"
            >
              <button
                role="menuitem"
                type="button"
                className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-gray-700 transition-colors hover:bg-gray-50"
                onClick={(e) => {
                  e.stopPropagation();
                  setMenuOpen(false);
                  onRename(meeting.id, meeting.title);
                }}
              >
                <Pencil className="h-3.5 w-3.5 text-gray-500" aria-hidden />
                Renommer
              </button>
              <button
                role="menuitem"
                type="button"
                className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-red-600 transition-colors hover:bg-red-50"
                onClick={(e) => {
                  e.stopPropagation();
                  setMenuOpen(false);
                  onDelete(meeting.id);
                }}
              >
                <Trash2 className="h-3.5 w-3.5" aria-hidden />
                Supprimer
              </button>
            </div>
          )}
        </div>
      </header>

      {/* Body */}
      <div className="relative flex-1 pl-5 pr-5 pt-3.5 pb-4">
        {isSearchSnippet && (
          <span className="mb-1.5 inline-block text-[9.5px] font-semibold uppercase tracking-[0.12em] text-amber-600">
            Extrait
          </span>
        )}
        {previewText ? (
          <p className="text-[13.5px] leading-[1.55] text-gray-600 line-clamp-3">
            {hasHighlight ? highlightText(previewText, highlightTerms!) : previewText}
          </p>
        ) : (
          <p className="text-[13px] italic text-gray-400">
            Aucun résumé pour ce meeting.
          </p>
        )}

        {/* Soft fade on clamped text */}
        {previewText && (
          <div
            aria-hidden
            className="pointer-events-none absolute inset-x-5 bottom-4 h-6 bg-gradient-to-t from-white via-white/80 to-transparent"
          />
        )}
      </div>

      {/* Badges footer (rendered only during search) */}
      {badges && (
        <footer className="flex items-center gap-1.5 border-t border-gray-100 bg-gray-50/60 px-5 py-2">
          {badges}
        </footer>
      )}
    </article>
  );
}

interface MatchBadgeProps {
  kind: 'fts' | 'semantic' | 'fuzzy';
}

/** Small match-kind chip. Kept intentionally flat so three of them don't shout. */
export function MatchBadge({ kind }: MatchBadgeProps) {
  const { label, tone } = {
    fts: {
      label: 'Exact',
      tone: 'bg-amber-50 text-amber-800 ring-1 ring-inset ring-amber-200/70',
    },
    semantic: {
      label: 'Sémantique',
      tone: 'bg-white text-gray-600 ring-1 ring-inset ring-gray-200',
    },
    fuzzy: {
      label: 'Similaire',
      tone: 'bg-white text-gray-600 ring-1 ring-inset ring-gray-200',
    },
  }[kind];
  return (
    <span
      className={`inline-flex items-center rounded px-1.5 py-0.5 text-[9.5px] font-semibold uppercase tracking-[0.1em] ${tone}`}
    >
      {label}
    </span>
  );
}
