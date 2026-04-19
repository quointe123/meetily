'use client';

import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  Upload, Clock, HardDrive, ArrowUp, ArrowDown, X, Loader2,
  AlertCircle, Globe, Cpu, Plus, Trash2, Check, ArrowRight,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useMultiImport, AudioFilePart, MAX_FILES } from '@/hooks/useMultiImport';
import { useTranscriptionModels } from '@/hooks/useTranscriptionModels';
import { LANGUAGES } from '@/constants/languages';
import { isAudioExtension, getAudioFormatsDisplayList, AUDIO_EXTENSIONS, AUDIO_FORMAT_DISPLAY_NAMES } from '@/constants/audioFormats';

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  return `${m}:${s.toString().padStart(2, '0')}`;
}

function formatDurationHuman(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h === 0) return m === 0 ? '< 1 min' : `${m} min`;
  return m === 0 ? `${h} h` : `${h} h ${m.toString().padStart(2, '0')}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function shortenName(name: string, max = 48): string {
  if (!name || name.length <= max) return name;
  const ext = name.slice(name.lastIndexOf('.'));
  const base = name.slice(0, name.length - ext.length);
  return `${base.slice(0, max - ext.length - 1)}…${ext}`;
}

// ── Decorative SVG waveform for the empty drop zone ──────────────────────────

function WaveformGlyph({ className = '' }: { className?: string }) {
  const bars = [6, 10, 14, 10, 16, 8, 14, 18, 12, 8, 14, 10, 6];
  return (
    <svg
      viewBox="0 0 80 24"
      className={className}
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeWidth={2}
      aria-hidden
    >
      {bars.map((h, i) => (
        <line
          key={i}
          x1={4 + i * 6}
          y1={12 - h / 2}
          x2={4 + i * 6}
          y2={12 + h / 2}
        />
      ))}
    </svg>
  );
}

// ── FileRow ───────────────────────────────────────────────────────────────────

interface FileRowProps {
  part: AudioFilePart;
  index: number;
  total: number;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
  disabled: boolean;
}

function FileRow({ part, index, total, onMoveUp, onMoveDown, onRemove, disabled }: FileRowProps) {
  const displayName = part.info.filename || part.info.path.split(/[\\/]/).pop() || 'Fichier';
  const isError = Boolean(part.error);

  return (
    <div
      className={`group relative flex items-center gap-3 rounded-md border bg-white px-3 py-2.5 transition-colors ${
        isError ? 'border-red-200 bg-red-50/50' : 'border-gray-200 hover:border-gray-300'
      }`}
    >
      {/* Order number — small and quiet */}
      <span
        className={`flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-full text-[10px] font-semibold tabular-nums ${
          isError ? 'bg-red-100 text-red-700' : 'bg-gray-100 text-gray-600'
        }`}
      >
        {index + 1}
      </span>

      {/* File info */}
      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] font-medium text-gray-900">
          {shortenName(displayName)}
        </p>
        {part.validating ? (
          <p className="mt-0.5 flex items-center gap-1.5 text-[11px] text-gray-500">
            <Loader2 className="h-3 w-3 animate-spin text-amber-500" />
            Analyse…
          </p>
        ) : isError ? (
          <p className="mt-0.5 flex items-center gap-1.5 text-[11px] text-red-600">
            <AlertCircle className="h-3 w-3" />
            {part.error}
          </p>
        ) : (
          <div className="mt-0.5 flex items-center gap-2.5 text-[10.5px] font-medium uppercase tracking-[0.06em] text-gray-500 tabular-nums">
            <span className="inline-flex items-center gap-1">
              <Clock className="h-2.5 w-2.5" strokeWidth={2.5} aria-hidden />
              {formatDuration(part.info.duration_seconds)}
            </span>
            <span aria-hidden className="h-0.5 w-0.5 rounded-full bg-gray-300" />
            <span className="inline-flex items-center gap-1">
              <HardDrive className="h-2.5 w-2.5" strokeWidth={2.5} aria-hidden />
              {formatFileSize(part.info.size_bytes)}
            </span>
            {part.info.format && (
              <>
                <span aria-hidden className="h-0.5 w-0.5 rounded-full bg-gray-300" />
                <span className="text-amber-700">{part.info.format.toUpperCase()}</span>
              </>
            )}
          </div>
        )}
      </div>

      {/* Reorder (stacked) — only shown if more than one file */}
      {total > 1 && (
        <div className="flex flex-shrink-0 flex-col opacity-0 transition-opacity group-hover:opacity-100">
          <button
            type="button"
            onClick={onMoveUp}
            disabled={disabled || index === 0}
            aria-label="Monter dans la liste"
            className="flex h-4 w-5 items-center justify-center rounded-sm text-gray-400 hover:bg-gray-100 hover:text-gray-600 disabled:opacity-30 disabled:cursor-not-allowed"
          >
            <ArrowUp className="h-3 w-3" strokeWidth={2.5} />
          </button>
          <button
            type="button"
            onClick={onMoveDown}
            disabled={disabled || index === total - 1}
            aria-label="Descendre dans la liste"
            className="flex h-4 w-5 items-center justify-center rounded-sm text-gray-400 hover:bg-gray-100 hover:text-gray-600 disabled:opacity-30 disabled:cursor-not-allowed"
          >
            <ArrowDown className="h-3 w-3" strokeWidth={2.5} />
          </button>
        </div>
      )}

      {/* Remove */}
      <button
        type="button"
        onClick={onRemove}
        disabled={disabled}
        aria-label="Retirer ce fichier"
        className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md text-gray-400 transition-colors hover:bg-red-50 hover:text-red-600 disabled:opacity-30 disabled:cursor-not-allowed"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

// ── DropOverlay — full-window backdrop during drag ───────────────────────────

function DropOverlay() {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-amber-50/70 backdrop-blur-[2px] pointer-events-none">
      <div className="animate-fade-in flex flex-col items-center gap-4">
        <div className="flex h-24 w-24 items-center justify-center rounded-2xl border-2 border-dashed border-amber-400 bg-white/80 shadow-[0_12px_40px_-12px_rgb(251_191_36/0.5)]">
          <Upload className="h-10 w-10 text-amber-500" strokeWidth={1.75} />
        </div>
        <div className="text-center">
          <p className="text-base font-semibold tracking-[-0.01em] text-amber-900">
            Déposez pour importer
          </p>
          <p className="mt-0.5 text-[12px] text-amber-800/70">
            Audio et vidéo acceptés
          </p>
        </div>
      </div>
    </div>
  );
}

// ── ImportPage ────────────────────────────────────────────────────────────────

export default function ImportPage() {
  const { selectedLanguage, transcriptModelConfig } = useConfig();

  const {
    files,
    status,
    progress,
    error,
    isProcessing,
    addFiles,
    removeFile,
    moveUp,
    moveDown,
    startImport,
    cancelImport,
    reset,
  } = useMultiImport();

  const [title, setTitle] = useState('');
  const [titleModifiedByUser, setTitleModifiedByUser] = useState(false);
  const [selectedLang, setSelectedLang] = useState(selectedLanguage || 'auto');
  const [isDragging, setIsDragging] = useState(false);
  const [isTitleFocused, setIsTitleFocused] = useState(false);

  const { availableModels, selectedModelKey, setSelectedModelKey, loadingModels, fetchModels } =
    useTranscriptionModels(transcriptModelConfig);

  useEffect(() => {
    fetchModels();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const firstValidFile = files.find(f => !f.error && !f.validating);
  useEffect(() => {
    if (firstValidFile && !titleModifiedByUser) {
      setTitle(firstValidFile.info.filename);
    }
  }, [firstValidFile?.info.filename, titleModifiedByUser]); // eslint-disable-line react-hooks/exhaustive-deps

  const totalDurationSecs = files
    .filter(f => !f.error && !f.validating)
    .reduce((sum, f) => sum + f.info.duration_seconds, 0);

  const selectedModel = useMemo(() => {
    if (!selectedModelKey) return undefined;
    const colonIdx = selectedModelKey.indexOf(':');
    if (colonIdx === -1) return undefined;
    return availableModels.find(
      m =>
        m.provider === selectedModelKey.slice(0, colonIdx) &&
        m.name === selectedModelKey.slice(colonIdx + 1),
    );
  }, [selectedModelKey, availableModels]);

  const isParakeetModel = selectedModel?.provider === 'parakeet';

  useEffect(() => {
    if (isParakeetModel && selectedLang !== 'auto') setSelectedLang('auto');
  }, [isParakeetModel, selectedLang]);

  // Full-window drag-drop
  useEffect(() => {
    if (isProcessing) return;
    const unlisteners: (() => void)[] = [];
    const cleanedRef = { current: false };

    const setup = async () => {
      const unEnter = await listen('tauri://drag-enter', () => setIsDragging(true));
      if (cleanedRef.current) { unEnter(); return; }
      unlisteners.push(unEnter);

      const unLeave = await listen('tauri://drag-leave', () => setIsDragging(false));
      if (cleanedRef.current) { unLeave(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unLeave);

      const unDrop = await listen<{ paths: string[] }>('tauri://drag-drop', (e) => {
        setIsDragging(false);
        const audioPaths = e.payload.paths.filter(p => {
          const ext = p.split('.').pop()?.toLowerCase();
          return !!ext && isAudioExtension(ext);
        });
        if (audioPaths.length > 0) {
          addFiles(audioPaths);
        } else if (e.payload.paths.length > 0) {
          toast.error('Fichier non supporté', {
            description: `Formats acceptés : ${getAudioFormatsDisplayList()}`,
          });
        }
      });
      if (cleanedRef.current) { unDrop(); unlisteners.forEach(u => u()); return; }
      unlisteners.push(unDrop);
    };

    setup();
    return () => {
      cleanedRef.current = true;
      unlisteners.forEach(u => u());
    };
  }, [isProcessing, addFiles]);

  const handleBrowse = useCallback(async () => {
    try {
      const paths = await invoke<string[]>('select_multiple_audio_files_command');
      if (paths.length > 0) await addFiles(paths);
    } catch (err) {
      console.error('File picker error:', err);
    }
  }, [addFiles]);

  const handleImport = useCallback(async () => {
    await startImport(
      title || firstValidFile?.info.filename || 'Import',
      isParakeetModel ? null : selectedLang === 'auto' ? null : selectedLang,
      selectedModel?.name ?? null,
      selectedModel?.provider ?? null,
    );
  }, [startImport, title, firstValidFile, isParakeetModel, selectedLang, selectedModel]);

  const handleReset = useCallback(() => {
    reset();
    setTitle('');
    setTitleModifiedByUser(false);
  }, [reset]);

  const handleCancel = useCallback(async () => {
    await cancelImport();
    setTitleModifiedByUser(false);
  }, [cancelImport]);

  const validFiles = files.filter(f => !f.error && !f.validating);
  const hasValidFiles = validFiles.length > 0;
  const canAddMore = files.length < MAX_FILES;

  const selectedLangLabel = useMemo(() => {
    if (selectedLang === 'auto') return 'Auto';
    return LANGUAGES.find(l => l.code === selectedLang)?.name ?? selectedLang;
  }, [selectedLang]);

  // ─── Processing view ───────────────────────────────────────────────────────
  if (isProcessing) {
    const pct = Math.min(Math.round(progress?.progress_percentage ?? 0), 100);
    return (
      <div className="mx-auto max-w-xl px-6 py-10 sm:py-14">
        <header className="mb-8">
          <p className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-amber-700">
            En cours
          </p>
          <h1 className="mt-1 text-[22px] font-semibold tracking-[-0.01em] text-gray-900">
            Import en cours…
          </h1>
        </header>

        {/* Progress ring + counter */}
        <div className="relative overflow-hidden rounded-xl border border-amber-200/60 bg-white p-6 shadow-[0_2px_8px_-2px_rgb(0_0_0/0.04)]">
          {/* Stage badge */}
          <div className="flex items-center justify-between">
            <span className="inline-flex items-center gap-2 rounded-full border border-amber-200/80 bg-amber-50 px-2.5 py-0.5 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-amber-800">
              <span className="relative flex h-1.5 w-1.5">
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-75" />
                <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-amber-500" />
              </span>
              {progress?.stage ?? 'Traitement'}
            </span>
            <span className="text-[24px] font-semibold tabular-nums tracking-[-0.02em] text-gray-900">
              {pct}
              <span className="text-[16px] font-medium text-gray-400">%</span>
            </span>
          </div>

          {/* Progress bar */}
          <div className="mt-5 h-1.5 overflow-hidden rounded-full bg-gray-100">
            <div
              className="h-full rounded-full bg-gradient-to-r from-amber-400 to-amber-500 transition-[width] duration-500 ease-out"
              style={{ width: `${pct}%` }}
            />
          </div>

          {/* Current message */}
          {progress?.message && (
            <p className="mt-4 text-[13px] leading-relaxed text-gray-600">
              {progress.message}
            </p>
          )}

          {/* Indeterminate scan */}
          <div className="pointer-events-none absolute inset-x-6 bottom-0 h-[1.5px] overflow-hidden">
            <div className="h-full w-1/4 rounded-full bg-gradient-to-r from-transparent via-amber-500 to-transparent animate-search-scan" />
          </div>
        </div>

        {/* File queue snapshot */}
        {validFiles.length > 0 && (
          <ul className="mt-6 space-y-1.5">
            {validFiles.map((f, i) => (
              <li
                key={f.id}
                className="flex items-center gap-2.5 rounded-md bg-white px-3 py-2 text-[12px] text-gray-600"
              >
                <span className="flex h-4 w-4 items-center justify-center rounded-full bg-gray-100 text-[9.5px] font-semibold tabular-nums text-gray-500">
                  {i + 1}
                </span>
                <span className="flex-1 truncate">{shortenName(f.info.filename, 40)}</span>
                <span className="tabular-nums text-gray-400">
                  {formatDuration(f.info.duration_seconds)}
                </span>
              </li>
            ))}
          </ul>
        )}

        <div className="mt-8 flex justify-end">
          <Button variant="outline" onClick={handleCancel} className="gap-2">
            <X className="h-4 w-4" />
            Annuler
          </Button>
        </div>
      </div>
    );
  }

  // ─── Error view ────────────────────────────────────────────────────────────
  if (status === 'error' && error) {
    return (
      <div className="mx-auto max-w-xl px-6 py-10 sm:py-14">
        <div className="rounded-xl border border-red-200 bg-red-50/60 p-6">
          <div className="flex items-center gap-2.5 text-red-800">
            <AlertCircle className="h-5 w-5" />
            <h2 className="text-[15px] font-semibold">Erreur lors de l'import</h2>
          </div>
          <p className="mt-2 text-[13px] leading-relaxed text-red-700">{error}</p>
          <div className="mt-5 flex gap-2">
            <Button onClick={handleReset} variant="outline" size="sm">
              Réessayer
            </Button>
          </div>
        </div>
      </div>
    );
  }

  // ─── Idle view ─────────────────────────────────────────────────────────────
  return (
    <>
      {isDragging && <DropOverlay />}

      <div className="mx-auto max-w-xl px-6 py-10 sm:py-14">
        {/* Header */}
        <header className="mb-8 flex items-start gap-3">
          <span aria-hidden className="mt-1 h-8 w-[2px] rounded-full bg-amber-400" />
          <div>
            <p className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-amber-700">
              Import
            </p>
            <h1 className="mt-1 text-[22px] font-semibold tracking-[-0.01em] text-gray-900">
              Importer un enregistrement
            </h1>
            <p className="mt-1.5 text-[13px] leading-relaxed text-gray-500">
              Ajoutez jusqu'à {MAX_FILES} fichiers audio ou vidéo, choisissez la langue
              et le modèle, puis lancez la transcription.
            </p>
          </div>
        </header>

        {/* Drop zone — only when we can still add files */}
        {canAddMore && (
          <div
            onClick={handleBrowse}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                handleBrowse();
              }
            }}
            role="button"
            tabIndex={0}
            className="group relative flex cursor-pointer flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed border-gray-300 bg-white px-6 py-10 text-center transition-[border-color,background-color,transform] duration-200 ease-out hover:border-amber-400 hover:bg-amber-50/30 focus-visible:border-amber-400 focus-visible:bg-amber-50/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400/40"
          >
            <WaveformGlyph className="h-8 w-auto text-gray-300 transition-colors duration-200 group-hover:text-amber-500" />
            <div>
              <p className="text-[14px] font-medium text-gray-800">
                Glissez vos fichiers ici
              </p>
              <p className="mt-0.5 text-[12px] text-gray-500">
                ou <span className="font-semibold text-amber-700 underline-offset-2 group-hover:underline">parcourez votre disque</span>
              </p>
            </div>
            <div className="mt-1 flex flex-wrap items-center justify-center gap-1">
              {AUDIO_EXTENSIONS.map(ext => (
                <span
                  key={ext}
                  className="rounded-sm bg-gray-100 px-1.5 py-0.5 text-[9.5px] font-semibold uppercase tracking-[0.08em] text-gray-500"
                >
                  {AUDIO_FORMAT_DISPLAY_NAMES[ext]}
                </span>
              ))}
              <span className="ml-1 text-[10.5px] text-gray-400 tabular-nums">
                {files.length}/{MAX_FILES}
              </span>
            </div>
          </div>
        )}

        {/* File queue */}
        {files.length > 0 && (
          <section className="mt-6">
            <div className="mb-2 flex items-baseline justify-between">
              <h2 className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-gray-500">
                Fichiers sélectionnés
              </h2>
              {totalDurationSecs > 0 && (
                <span className="text-[11px] tabular-nums text-gray-500">
                  Durée totale :{' '}
                  <span className="font-semibold text-gray-700">
                    {formatDurationHuman(totalDurationSecs)}
                  </span>
                </span>
              )}
            </div>

            <div className="space-y-1.5">
              {files.map((part, idx) => (
                <FileRow
                  key={part.id}
                  part={part}
                  index={idx}
                  total={files.length}
                  onMoveUp={() => moveUp(part.id)}
                  onMoveDown={() => moveDown(part.id)}
                  onRemove={() => removeFile(part.id)}
                  disabled={false}
                />
              ))}
            </div>

            <div className="mt-2.5 flex items-center justify-between text-[11.5px]">
              {canAddMore ? (
                <button
                  type="button"
                  onClick={handleBrowse}
                  className="inline-flex items-center gap-1 font-medium text-amber-700 transition-colors hover:text-amber-800"
                >
                  <Plus className="h-3 w-3" />
                  Ajouter un fichier
                </button>
              ) : (
                <span className="text-gray-400">Limite atteinte ({MAX_FILES}/{MAX_FILES})</span>
              )}
              <button
                type="button"
                onClick={handleReset}
                className="inline-flex items-center gap-1 font-medium text-gray-500 transition-colors hover:text-red-600"
              >
                <Trash2 className="h-3 w-3" />
                Tout effacer
              </button>
            </div>
          </section>
        )}

        {/* Title + Options */}
        {hasValidFiles && (
          <section className="mt-8 space-y-6">
            {/* Title */}
            <div>
              <label className="block text-[10.5px] font-semibold uppercase tracking-[0.12em] text-gray-500">
                Titre
              </label>
              <div
                className={`mt-2 rounded-lg bg-white transition-[box-shadow,border-color] duration-200 ease-out ${
                  isTitleFocused
                    ? 'shadow-[0_0_0_3px_rgb(251_191_36/0.18)] ring-1 ring-amber-400'
                    : 'ring-1 ring-gray-200 hover:ring-gray-300'
                }`}
              >
                <input
                  type="text"
                  value={title}
                  onChange={(e) => {
                    setTitle(e.target.value);
                    setTitleModifiedByUser(true);
                  }}
                  onFocus={() => setIsTitleFocused(true)}
                  onBlur={() => setIsTitleFocused(false)}
                  placeholder="Titre du meeting"
                  className="w-full rounded-lg bg-transparent px-3.5 py-2.5 text-[14px] text-gray-900 placeholder:text-gray-400 focus:outline-none"
                />
              </div>
              <p className="mt-1.5 text-[11px] text-gray-400">
                Par défaut : le nom du premier fichier.
              </p>
            </div>

            {/* Options row */}
            <div>
              <h2 className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-gray-500">
                Transcription
              </h2>
              <div className="mt-2 grid gap-3 sm:grid-cols-2">
                {/* Language */}
                <div className="rounded-lg border border-gray-200 bg-white p-3 transition-colors hover:border-gray-300">
                  <div className="mb-1.5 flex items-center gap-1.5">
                    <Globe className="h-3.5 w-3.5 text-gray-500" strokeWidth={2} aria-hidden />
                    <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-gray-600">
                      Langue
                    </span>
                  </div>
                  {isParakeetModel ? (
                    <p className="text-[12.5px] text-gray-500">
                      Détection automatique (Parakeet)
                    </p>
                  ) : (
                    <Select value={selectedLang} onValueChange={setSelectedLang}>
                      <SelectTrigger className="h-8 border-0 bg-transparent px-0 text-[13px] font-medium text-gray-800 shadow-none focus:ring-0 focus:ring-offset-0">
                        <SelectValue placeholder="Auto">{selectedLangLabel}</SelectValue>
                      </SelectTrigger>
                      <SelectContent className="max-h-60">
                        {LANGUAGES.map(lang => (
                          <SelectItem key={lang.code} value={lang.code}>
                            {lang.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                </div>

                {/* Model */}
                <div className="rounded-lg border border-gray-200 bg-white p-3 transition-colors hover:border-gray-300">
                  <div className="mb-1.5 flex items-center gap-1.5">
                    <Cpu className="h-3.5 w-3.5 text-gray-500" strokeWidth={2} aria-hidden />
                    <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-gray-600">
                      Modèle
                    </span>
                  </div>
                  {availableModels.length === 0 && !loadingModels ? (
                    <p className="text-[12.5px] text-gray-400">Aucun modèle disponible</p>
                  ) : (
                    <Select
                      value={selectedModelKey}
                      onValueChange={setSelectedModelKey}
                      disabled={loadingModels}
                    >
                      <SelectTrigger className="h-8 border-0 bg-transparent px-0 text-[13px] font-medium text-gray-800 shadow-none focus:ring-0 focus:ring-offset-0">
                        <SelectValue placeholder={loadingModels ? 'Chargement…' : 'Sélectionner'} />
                      </SelectTrigger>
                      <SelectContent>
                        {availableModels.map(m => (
                          <SelectItem key={`${m.provider}:${m.name}`} value={`${m.provider}:${m.name}`}>
                            <span className="flex items-center gap-2">
                              <span>{m.displayName}</span>
                              <span className="text-[10px] text-gray-400 tabular-nums">
                                {Math.round(m.size_mb)} MB
                              </span>
                            </span>
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                </div>
              </div>
            </div>
          </section>
        )}

        {/* Action bar */}
        <div className="mt-8 flex items-center justify-between border-t border-gray-100 pt-6">
          <p className="text-[11.5px] text-gray-500">
            {hasValidFiles ? (
              <>
                <Check className="mr-1 inline h-3 w-3 text-amber-600" strokeWidth={3} />
                {validFiles.length} fichier{validFiles.length > 1 ? 's' : ''} prêt
                {validFiles.length > 1 ? 's' : ''}{' '}
                {totalDurationSecs > 0 && (
                  <span className="text-gray-400">
                    · {formatDurationHuman(totalDurationSecs)}
                  </span>
                )}
              </>
            ) : (
              'Ajoutez au moins un fichier pour continuer'
            )}
          </p>
          <Button
            onClick={handleImport}
            disabled={!hasValidFiles}
            className="gap-1.5 bg-amber-500 text-white shadow-[0_2px_8px_-2px_rgb(251_191_36/0.5)] transition-[background-color,transform,box-shadow] hover:bg-amber-600 hover:shadow-[0_4px_12px_-2px_rgb(251_191_36/0.5)] active:translate-y-[0.5px] disabled:bg-gray-200 disabled:text-gray-400 disabled:shadow-none"
          >
            Lancer l'import
            <ArrowRight className="h-4 w-4" strokeWidth={2.5} />
          </Button>
        </div>
      </div>
    </>
  );
}
