import { writeFile, exists } from '@tauri-apps/plugin-fs';
import { downloadDir, join } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-shell';
import type { ExportResult } from './types';

async function uniquePath(dir: string, filename: string): Promise<{ path: string; finalName: string }> {
  const dot = filename.lastIndexOf('.');
  const stem = dot === -1 ? filename : filename.slice(0, dot);
  const ext = dot === -1 ? '' : filename.slice(dot);

  let candidate = filename;
  let counter = 1;
  while (await exists(await join(dir, candidate))) {
    candidate = `${stem}_${counter}${ext}`;
    counter += 1;
    if (counter > 1000) throw new Error('Too many existing files with similar names');
  }

  return { path: await join(dir, candidate), finalName: candidate };
}

async function toBytes(content: Blob | string): Promise<Uint8Array> {
  if (typeof content === 'string') {
    return new TextEncoder().encode(content);
  }
  const buffer = await content.arrayBuffer();
  return new Uint8Array(buffer);
}

export async function saveToDownloads(filename: string, content: Blob | string): Promise<ExportResult> {
  const dir = await downloadDir();
  const { path, finalName } = await uniquePath(dir, filename);
  const bytes = await toBytes(content);
  await writeFile(path, bytes);
  return { filename: finalName, fullPath: path, byteSize: bytes.length };
}

/**
 * Fallback when the automatic save to Downloads fails (e.g. permission refused).
 * Opens the native "Save As" dialog. Returns null if the user cancels.
 */
export async function saveViaDialog(filename: string, content: Blob | string): Promise<ExportResult | null> {
  const { save } = await import('@tauri-apps/plugin-dialog');
  const chosen = await save({ defaultPath: filename });
  if (!chosen) return null;
  const bytes = await toBytes(content);
  await writeFile(chosen, bytes);
  const lastSep = Math.max(chosen.lastIndexOf('\\'), chosen.lastIndexOf('/'));
  const finalName = lastSep >= 0 ? chosen.slice(lastSep + 1) : chosen;
  return { filename: finalName, fullPath: chosen, byteSize: bytes.length };
}

export async function openContainingFolder(fullPath: string): Promise<void> {
  // `open` with a directory path opens the OS file browser at that location.
  // On Windows this is Explorer, on macOS Finder, on Linux the default file manager.
  const lastSep = Math.max(fullPath.lastIndexOf('\\'), fullPath.lastIndexOf('/'));
  const dir = lastSep > 0 ? fullPath.slice(0, lastSep) : fullPath;
  await open(dir);
}
