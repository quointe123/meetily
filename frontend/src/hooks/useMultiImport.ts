'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { useRouter } from 'next/navigation';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';

// ── Types ──────────────────────────────────────────────────────────────────────

export interface AudioFileInfo {
  path: string;
  filename: string;
  duration_seconds: number;
  size_bytes: number;
  format: string;
}

export interface AudioFilePart {
  id: string;           // local React key (crypto.randomUUID)
  info: AudioFileInfo;
  validating: boolean;
  error: string | null;
}

export interface ImportProgress {
  stage: string;
  progress_percentage: number;
  message: string;
}

export type MultiImportStatus = 'idle' | 'validating' | 'processing' | 'complete' | 'error';

export interface UseMultiImportReturn {
  files: AudioFilePart[];
  status: MultiImportStatus;
  progress: ImportProgress | null;
  error: string | null;
  isProcessing: boolean;

  addFiles: (paths: string[]) => Promise<void>;
  removeFile: (id: string) => void;
  moveUp: (id: string) => void;
  moveDown: (id: string) => void;
  startImport: (
    title: string,
    language?: string | null,
    model?: string | null,
    provider?: string | null
  ) => Promise<void>;
  cancelImport: () => Promise<void>;
  reset: () => void;
}

const MAX_FILES = 4;

// ── Hook ───────────────────────────────────────────────────────────────────────

export function useMultiImport(): UseMultiImportReturn {
  const router = useRouter();
  const { refetchMeetings } = useSidebar();

  const [files, setFiles] = useState<AudioFilePart[]>([]);
  const [status, setStatus] = useState<MultiImportStatus>('idle');
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const isCancelledRef = useRef(false);

  // ── Tauri event listeners ────────────────────────────────────────────────────
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const cleanedRef = { current: false };

    const setup = async () => {
      const unProgress = await listen<ImportProgress>('import-progress', (e) => {
        if (isCancelledRef.current) return;
        setProgress(e.payload);
        setStatus('processing');
      });
      if (cleanedRef.current) { unProgress(); return; }
      unlisteners.push(unProgress);

      const unComplete = await listen<{
        meeting_id: string;
        title: string;
        segments_count: number;
        duration_seconds: number;
      }>('import-complete', async (e) => {
        if (isCancelledRef.current) return;
        setStatus('complete');
        setProgress(null);
        await refetchMeetings();
        router.push(`/meeting-details?id=${e.payload.meeting_id}`);
      });
      if (cleanedRef.current) { unComplete(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unComplete);

      const unError = await listen<{ error: string }>('import-error', (e) => {
        if (isCancelledRef.current) return;
        setStatus('error');
        setError(e.payload.error);
      });
      if (cleanedRef.current) { unError(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unError);
    };

    setup();

    return () => {
      cleanedRef.current = true;
      unlisteners.forEach(u => u());
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  // router and refetchMeetings are stable refs — intentionally omitted

  // ── addFiles ─────────────────────────────────────────────────────────────────
  const addFiles = useCallback(
    async (paths: string[]) => {
      // Count currently valid (non-errored) files
      const currentValid = files.filter(f => !f.error).length;
      const available = MAX_FILES - currentValid;

      if (available <= 0) {
        toast.error('Maximum 4 fichiers atteint', {
          description: "Retirez un fichier avant d'en ajouter un autre.",
        });
        return;
      }

      const toAdd = paths.slice(0, available);
      const skipped = paths.length - toAdd.length;
      if (skipped > 0) {
        toast.warning(`${skipped} fichier(s) ignoré(s)`, {
          description: `Maximum ${MAX_FILES} fichiers par import.`,
        });
      }

      // Create placeholder entries immediately (validating=true)
      const newParts: AudioFilePart[] = toAdd.map(path => ({
        id: crypto.randomUUID(),
        info: {
          path,
          filename: path.split(/[\\/]/).pop() ?? path,
          duration_seconds: 0,
          size_bytes: 0,
          format: '',
        },
        validating: true,
        error: null,
      }));

      setFiles(prev => [...prev, ...newParts]);

      // Validate each file asynchronously
      for (const part of newParts) {
        try {
          const info = await invoke<AudioFileInfo>('validate_audio_file_command', {
            path: part.info.path,
          });
          setFiles(prev =>
            prev.map(f => (f.id === part.id ? { ...f, info, validating: false } : f))
          );
        } catch (err: unknown) {
          const msg =
            typeof err === 'string'
              ? err
              : (err as { message?: string })?.message ?? 'Validation échouée';
          setFiles(prev =>
            prev.map(f =>
              f.id === part.id ? { ...f, validating: false, error: msg } : f
            )
          );
        }
      }
    },
    [files]
  );

  // ── removeFile ───────────────────────────────────────────────────────────────
  const removeFile = useCallback((id: string) => {
    setFiles(prev => prev.filter(f => f.id !== id));
  }, []);

  // ── moveUp ───────────────────────────────────────────────────────────────────
  const moveUp = useCallback((id: string) => {
    setFiles(prev => {
      const idx = prev.findIndex(f => f.id === id);
      if (idx <= 0) return prev;
      const next = [...prev];
      [next[idx - 1], next[idx]] = [next[idx], next[idx - 1]];
      return next;
    });
  }, []);

  // ── moveDown ─────────────────────────────────────────────────────────────────
  const moveDown = useCallback((id: string) => {
    setFiles(prev => {
      const idx = prev.findIndex(f => f.id === id);
      if (idx === -1 || idx >= prev.length - 1) return prev;
      const next = [...prev];
      [next[idx], next[idx + 1]] = [next[idx + 1], next[idx]];
      return next;
    });
  }, []);

  // ── startImport ──────────────────────────────────────────────────────────────
  const startImport = useCallback(
    async (
      title: string,
      language?: string | null,
      model?: string | null,
      provider?: string | null
    ) => {
      const validFiles = files.filter(f => !f.error && !f.validating);
      if (validFiles.length === 0) return;

      isCancelledRef.current = false;
      setStatus('processing');
      setError(null);
      setProgress(null);

      const parts = validFiles.map((f, idx) => ({
        path: f.info.path,
        order: idx + 1,
      }));

      try {
        await invoke('start_import_multi_command', {
          parts,
          title,
          language: language ?? null,
          model: model ?? null,
          provider: provider ?? null,
        });
      } catch (err: unknown) {
        setStatus('error');
        const msg =
          typeof err === 'string'
            ? err
            : (err as { message?: string })?.message ?? "Échec du démarrage de l'import";
        setError(msg);
      }
    },
    [files]
  );

  // ── cancelImport ─────────────────────────────────────────────────────────────
  const cancelImport = useCallback(async () => {
    isCancelledRef.current = true;
    try {
      await invoke('cancel_import_command');
      setStatus('idle');
      setProgress(null);
    } catch (err) {
      console.error('Failed to cancel import:', err);
    }
  }, []);

  // ── reset ────────────────────────────────────────────────────────────────────
  const reset = useCallback(() => {
    isCancelledRef.current = false;
    setFiles([]);
    setStatus('idle');
    setProgress(null);
    setError(null);
  }, []);

  return {
    files,
    status,
    progress,
    error,
    isProcessing: status === 'processing',
    addFiles,
    removeFile,
    moveUp,
    moveDown,
    startImport,
    cancelImport,
    reset,
  };
}
