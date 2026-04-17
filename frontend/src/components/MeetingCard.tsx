'use client';

import React, { useState, useRef, useEffect } from 'react';
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
  if (!isoDate) return '';
  try {
    const date = new Date(isoDate);
    return date.toLocaleDateString('fr-FR', {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    });
  } catch {
    return '';
  }
}

function formatDuration(seconds: number | null): string {
  if (seconds == null || seconds <= 0) return '';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m > 0 ? `${h}h${m.toString().padStart(2, '0')}` : `${h}h`;
}

function highlightText(text: string, terms: string[]): React.ReactNode[] {
  if (!terms || terms.length === 0) return [text];
  const escaped = terms.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  const pattern = new RegExp(`(${escaped.join('|')})`, 'gi');
  const parts = text.split(pattern);
  return parts.map((part, i) => {
    if (terms.some((t) => t.toLowerCase() === part.toLowerCase())) {
      return (
        <mark key={i} className="bg-yellow-200">
          {part}
        </mark>
      );
    }
    return part;
  });
}

export function MeetingCard({
  meeting,
  onClick,
  onRename,
  onDelete,
  searchSnippet,
  highlightTerms,
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

  const dateStr = formatDate(meeting.created_at);
  const durationStr = formatDuration(meeting.duration_seconds);
  const meta = [dateStr, durationStr].filter(Boolean).join(' \u00B7 ');

  const previewText = searchSnippet ?? meeting.summary_preview;

  return (
    <div
      className="group relative bg-white rounded-lg border border-gray-200 p-4 cursor-pointer transition-shadow hover:shadow-md"
      onClick={onClick}
    >
      {/* Title */}
      <h3 className="font-semibold text-gray-900 line-clamp-2 pr-8">
        {highlightTerms && highlightTerms.length > 0
          ? highlightText(meeting.title, highlightTerms)
          : meeting.title}
      </h3>

      {/* Meta line */}
      {meta && <p className="text-xs text-gray-500 mt-1">{meta}</p>}

      {/* Preview / summary */}
      <p className="text-sm text-gray-600 mt-2 line-clamp-3">
        {previewText
          ? highlightTerms && highlightTerms.length > 0
            ? highlightText(previewText, highlightTerms)
            : previewText
          : 'Pas de resume'}
      </p>

      {/* Menu button */}
      <div ref={menuRef} className="absolute top-3 right-3">
        <button
          className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-gray-100 transition-opacity"
          onClick={(e) => {
            e.stopPropagation();
            setMenuOpen((v) => !v);
          }}
        >
          <MoreHorizontal className="h-4 w-4 text-gray-500" />
        </button>

        {menuOpen && (
          <div className="absolute right-0 mt-1 w-40 bg-white border border-gray-200 rounded-md shadow-lg z-10">
            <button
              className="flex items-center gap-2 w-full px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen(false);
                onRename(meeting.id, meeting.title);
              }}
            >
              <Pencil className="h-3.5 w-3.5" />
              Renommer
            </button>
            <button
              className="flex items-center gap-2 w-full px-3 py-2 text-sm text-red-600 hover:bg-red-50"
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen(false);
                onDelete(meeting.id);
              }}
            >
              <Trash2 className="h-3.5 w-3.5" />
              Supprimer
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
