import { format, parseISO } from 'date-fns';
import { fr } from 'date-fns/locale';
import type { MetadataHeader, ExportFormat } from './types';

const ILLEGAL_CHARS = /[<>:"/\\|?*\x00-\x1F]/g;
const WHITESPACE = /\s+/g;

function stripAccents(input: string): string {
  return input.normalize('NFD').replace(/[\u0300-\u036f]/g, '');
}

export function slugifyFilename(title: string): string {
  const stripped = stripAccents(title)
    .replace(ILLEGAL_CHARS, '')
    .trim()
    .replace(WHITESPACE, '_');
  return stripped.length > 0 ? stripped : 'Meeting';
}

export function formatFrenchDate(dateIso: string): string {
  return format(parseISO(dateIso), 'd MMMM yyyy', { locale: fr });
}

export function formatDuration(seconds?: number): string {
  if (!seconds || seconds <= 0) return '';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${m}min`;
  if (m > 0) return `${m}min`;
  return `${s}s`;
}

export function buildFilename(
  title: string,
  dateIso: string,
  extension: 'md' | 'docx' | 'pdf',
): string {
  const slug = slugifyFilename(title);
  const datePart = parseISO(dateIso).toISOString().slice(0, 10); // YYYY-MM-DD
  return `${slug}_${datePart}.${extension}`;
}

export function buildMetadataHeader(
  meeting: { title?: string; name?: string; created_at: string; duration_seconds?: number },
  modelName?: string,
): MetadataHeader {
  return {
    title: meeting.title ?? meeting.name ?? 'Meeting',
    dateIso: meeting.created_at,
    durationSeconds: meeting.duration_seconds,
    modelName,
  };
}

export function extensionForFormat(format: ExportFormat): 'md' | 'docx' | 'pdf' {
  return format === 'markdown' ? 'md' : format;
}
