'use client';

import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  Upload, FileAudio, Clock, HardDrive, ChevronDown, ChevronUp,
  ArrowUp, ArrowDown, X, Loader2, AlertCircle, Globe, Cpu,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useMultiImport, AudioFilePart } from '@/hooks/useMultiImport';
import { useTranscriptionModels } from '@/hooks/useTranscriptionModels';
import { LANGUAGES } from '@/constants/languages';
import { isAudioExtension } from '@/constants/audioFormats';

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${secs.toString().padStart(2, '0')}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

// ── FileCard ──────────────────────────────────────────────────────────────────

interface FileCardProps {
  part: AudioFilePart;
  index: number;
  total: number;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
  disabled: boolean;
}

function FileCard({ part, index, total, onMoveUp, onMoveDown, onRemove, disabled }: FileCardProps) {
  return (
    <div
      className={`flex items-center gap-3 p-3 rounded-lg border ${
        part.error
          ? 'border-red-200 bg-red-50'
          : 'border-gray-200 bg-white'
      }`}
    >
      {/* Order badge */}
      <span className="w-6 h-6 flex items-center justify-center rounded-full bg-blue-100 text-blue-700 text-xs font-bold flex-shrink-0">
        {index + 1}
      </span>

      {/* File info */}
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-gray-900 truncate">
          {part.info.filename || part.info.path.split(/[\\/]/).pop()}
        </p>
        {part.validating ? (
          <p className="text-xs text-gray-500 flex items-center gap-1">
            <Loader2 className="h-3 w-3 animate-spin" />
            Validation...
          </p>
        ) : part.error ? (
          <p className="text-xs text-red-600 flex items-center gap-1">
            <AlertCircle className="h-3 w-3" />
            {part.error}
          </p>
        ) : (
          <div className="flex items-center gap-3 text-xs text-gray-500 mt-0.5">
            <span className="flex items-center gap-1">
              <Clock className="h-3 w-3" />
              {formatDuration(part.info.duration_seconds)}
            </span>
            <span className="flex items-center gap-1">
              <HardDrive className="h-3 w-3" />
              {formatFileSize(part.info.size_bytes)}
            </span>
            {part.info.format && (
              <span className="text-blue-600 font-medium">{part.info.format}</span>
            )}
          </div>
        )}
      </div>

      {/* Reorder buttons */}
      <div className="flex flex-col gap-0.5">
        <button
          onClick={onMoveUp}
          disabled={disabled || index === 0}
          className="p-1 rounded hover:bg-gray-100 disabled:opacity-30 disabled:cursor-not-allowed"
          title="Monter"
        >
          <ArrowUp className="h-3.5 w-3.5 text-gray-600" />
        </button>
        <button
          onClick={onMoveDown}
          disabled={disabled || index === total - 1}
          className="p-1 rounded hover:bg-gray-100 disabled:opacity-30 disabled:cursor-not-allowed"
          title="Descendre"
        >
          <ArrowDown className="h-3.5 w-3.5 text-gray-600" />
        </button>
      </div>

      {/* Remove button */}
      <button
        onClick={onRemove}
        disabled={disabled}
        className="p-1 rounded hover:bg-red-100 disabled:opacity-30 disabled:cursor-not-allowed"
        title="Retirer"
      >
        <X className="h-4 w-4 text-gray-500 hover:text-red-600" />
      </button>
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
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [isDragging, setIsDragging] = useState(false);

  const { availableModels, selectedModelKey, setSelectedModelKey, loadingModels, fetchModels } =
    useTranscriptionModels(transcriptModelConfig);

  // Fetch models on mount
  useEffect(() => {
    fetchModels();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Auto-populate title from the first valid file (if user hasn't typed manually)
  const firstValidFile = files.find(f => !f.error && !f.validating);
  useEffect(() => {
    if (firstValidFile && !titleModifiedByUser) {
      setTitle(firstValidFile.info.filename);
    }
  }, [firstValidFile?.info.filename, titleModifiedByUser]); // eslint-disable-line react-hooks/exhaustive-deps

  // Total duration of valid files
  const totalDurationSecs = files
    .filter(f => !f.error && !f.validating)
    .reduce((sum, f) => sum + f.info.duration_seconds, 0);

  // Resolve selected model
  const selectedModel = useMemo(() => {
    if (!selectedModelKey) return undefined;
    const colonIdx = selectedModelKey.indexOf(':');
    if (colonIdx === -1) return undefined;
    return availableModels.find(
      m =>
        m.provider === selectedModelKey.slice(0, colonIdx) &&
        m.name === selectedModelKey.slice(colonIdx + 1)
    );
  }, [selectedModelKey, availableModels]);

  const isParakeetModel = selectedModel?.provider === 'parakeet';

  // Force 'auto' language when Parakeet is selected
  useEffect(() => {
    if (isParakeetModel && selectedLang !== 'auto') setSelectedLang('auto');
  }, [isParakeetModel, selectedLang]);

  // ── Local drag-drop on this page ─────────────────────────────────────────
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
            description: 'Formats acceptés : MP4, WAV, MP3, FLAC, OGG, MKV, WebM, WMA',
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

  // ── Browse button (multi-select) ─────────────────────────────────────────
  const handleBrowse = useCallback(async () => {
    try {
      const paths = await invoke<string[]>('select_multiple_audio_files_command');
      if (paths.length > 0) {
        await addFiles(paths);
      }
    } catch (err) {
      console.error('File picker error:', err);
    }
  }, [addFiles]);

  // ── Import ───────────────────────────────────────────────────────────────
  const handleImport = useCallback(async () => {
    await startImport(
      title || firstValidFile?.info.filename || 'Import',
      isParakeetModel ? null : selectedLang === 'auto' ? null : selectedLang,
      selectedModel?.name ?? null,
      selectedModel?.provider ?? null,
    );
  }, [startImport, title, firstValidFile, isParakeetModel, selectedLang, selectedModel]);

  const validFiles = files.filter(f => !f.error && !f.validating);
  const hasValidFiles = validFiles.length > 0;

  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <div className="max-w-xl mx-auto px-6 py-8 space-y-6">
      <h1 className="text-2xl font-semibold text-gray-900">Importer des fichiers audio</h1>

      {/* Drop zone */}
      {!isProcessing && status !== 'error' && (
        <div
          className={`border-2 border-dashed rounded-xl p-8 text-center transition-colors ${
            isDragging
              ? 'border-blue-400 bg-blue-50'
              : 'border-gray-300 hover:border-gray-400'
          }`}
        >
          <FileAudio className="h-10 w-10 text-gray-400 mx-auto mb-3" />
          <p className="text-sm text-gray-600 mb-3">
            Glissez vos fichiers ici ou
          </p>
          <Button onClick={handleBrowse} variant="outline" size="sm">
            <Upload className="h-4 w-4 mr-2" />
            Parcourir
          </Button>
          <p className="text-xs text-gray-400 mt-2">
            MP4, WAV, MP3, FLAC, OGG, MKV, WebM, WMA — max 4 fichiers
          </p>
        </div>
      )}

      {/* File list */}
      {files.length > 0 && !isProcessing && (
        <div className="space-y-2">
          {files.map((part, idx) => (
            <FileCard
              key={part.id}
              part={part}
              index={idx}
              total={files.length}
              onMoveUp={() => moveUp(part.id)}
              onMoveDown={() => moveDown(part.id)}
              onRemove={() => removeFile(part.id)}
              disabled={isProcessing}
            />
          ))}
          {totalDurationSecs > 0 && (
            <p className="text-sm text-gray-500 text-right">
              Durée totale : {formatDuration(totalDurationSecs)}
            </p>
          )}
        </div>
      )}

      {/* Title input */}
      {hasValidFiles && !isProcessing && (
        <div className="space-y-1">
          <label className="text-sm font-medium text-gray-700">Titre</label>
          <Input
            value={title}
            onChange={e => {
              setTitle(e.target.value);
              setTitleModifiedByUser(true);
            }}
            placeholder="Titre de la réunion"
          />
        </div>
      )}

      {/* Advanced options */}
      {hasValidFiles && !isProcessing && (
        <div className="border rounded-lg">
          <button
            onClick={() => setShowAdvanced(v => !v)}
            className="w-full flex items-center justify-between p-3 text-sm font-medium text-gray-700 hover:bg-gray-50"
          >
            <span>Options avancées</span>
            {showAdvanced ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
          </button>

          {showAdvanced && (
            <div className="p-3 pt-0 space-y-4 border-t">
              {/* Language */}
              {!isParakeetModel ? (
                <div className="space-y-2">
                  <div className="flex items-center gap-2">
                    <Globe className="h-4 w-4 text-muted-foreground" />
                    <span className="text-sm font-medium">Langue</span>
                  </div>
                  <Select value={selectedLang} onValueChange={setSelectedLang}>
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder="Sélectionner une langue" />
                    </SelectTrigger>
                    <SelectContent className="max-h-60">
                      {LANGUAGES.map(lang => (
                        <SelectItem key={lang.code} value={lang.code}>
                          {lang.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">
                  La sélection de langue n'est pas disponible avec Parakeet (détection automatique).
                </p>
              )}

              {/* Model */}
              {availableModels.length > 0 && (
                <div className="space-y-2">
                  <div className="flex items-center gap-2">
                    <Cpu className="h-4 w-4 text-muted-foreground" />
                    <span className="text-sm font-medium">Modèle</span>
                  </div>
                  <Select
                    value={selectedModelKey}
                    onValueChange={setSelectedModelKey}
                    disabled={loadingModels}
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder={loadingModels ? 'Chargement...' : 'Sélectionner un modèle'} />
                    </SelectTrigger>
                    <SelectContent>
                      {availableModels.map(m => (
                        <SelectItem key={`${m.provider}:${m.name}`} value={`${m.provider}:${m.name}`}>
                          {m.displayName} ({Math.round(m.size_mb)} MB)
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* Progress bar */}
      {isProcessing && progress && (
        <div className="space-y-2">
          <div className="w-full bg-gray-200 rounded-full h-3">
            <div
              className="bg-blue-600 h-3 rounded-full transition-all duration-300 ease-out"
              style={{ width: `${Math.min(progress.progress_percentage, 100)}%` }}
            />
          </div>
          <div className="flex justify-between text-xs text-gray-600">
            <span>{progress.stage}</span>
            <span>{Math.round(progress.progress_percentage)}%</span>
          </div>
          <p className="text-sm text-muted-foreground text-center">{progress.message}</p>
        </div>
      )}

      {/* Error display */}
      {status === 'error' && error && (
        <div className="bg-red-50 border border-red-200 rounded-lg p-4 space-y-3">
          <div className="flex items-center gap-2 text-red-700">
            <AlertCircle className="h-5 w-5" />
            <p className="text-sm font-medium">Erreur lors de l'import</p>
          </div>
          <p className="text-sm text-red-600">{error}</p>
          <Button variant="outline" size="sm" onClick={reset}>
            Réessayer
          </Button>
        </div>
      )}

      {/* Action buttons */}
      {!isProcessing && status !== 'error' && (
        <div className="flex justify-end gap-3">
          <Button
            onClick={handleImport}
            disabled={!hasValidFiles}
            className="bg-blue-600 hover:bg-blue-700 text-white"
          >
            <Upload className="h-4 w-4 mr-2" />
            Importer →
          </Button>
        </div>
      )}

      {isProcessing && (
        <div className="flex justify-end">
          <Button variant="outline" onClick={cancelImport}>
            <X className="h-4 w-4 mr-2" />
            Annuler
          </Button>
        </div>
      )}
    </div>
  );
}
