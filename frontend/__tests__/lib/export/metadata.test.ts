import { describe, it, expect } from 'vitest';
import {
  slugifyFilename,
  formatFrenchDate,
  formatDuration,
  buildFilename,
} from '@/lib/export/metadata';

describe('slugifyFilename', () => {
  it('replaces spaces with underscores', () => {
    expect(slugifyFilename('Hello World')).toBe('Hello_World');
  });

  it('strips accents', () => {
    expect(slugifyFilename('Réunion équipe')).toBe('Reunion_equipe');
  });

  it('removes illegal Windows/macOS characters', () => {
    expect(slugifyFilename('bad<>:"/\\|?*name')).toBe('badname');
  });

  it('falls back to "Meeting" when empty after slugification', () => {
    expect(slugifyFilename('')).toBe('Meeting');
    expect(slugifyFilename('<<<>>>')).toBe('Meeting');
  });

  it('trims whitespace', () => {
    expect(slugifyFilename('  Hello  ')).toBe('Hello');
  });
});

describe('formatFrenchDate', () => {
  it('formats ISO date as "19 avril 2026"', () => {
    expect(formatFrenchDate('2026-04-19T10:00:00Z')).toBe('19 avril 2026');
  });
});

describe('formatDuration', () => {
  it('formats seconds as "1h 23min"', () => {
    expect(formatDuration(4980)).toBe('1h 23min');
  });

  it('formats under an hour as "23min"', () => {
    expect(formatDuration(1380)).toBe('23min');
  });

  it('formats under a minute as "45s"', () => {
    expect(formatDuration(45)).toBe('45s');
  });

  it('returns empty string for undefined/0', () => {
    expect(formatDuration(undefined)).toBe('');
    expect(formatDuration(0)).toBe('');
  });
});

describe('buildFilename', () => {
  it('combines slug + date + extension', () => {
    expect(buildFilename('Standup Équipe', '2026-04-19T10:00:00Z', 'pdf'))
      .toBe('Standup_Equipe_2026-04-19.pdf');
  });

  it('falls back to "Meeting_..." when title is empty', () => {
    expect(buildFilename('', '2026-04-19T10:00:00Z', 'docx'))
      .toBe('Meeting_2026-04-19.docx');
  });
});
