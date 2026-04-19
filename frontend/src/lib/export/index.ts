import type { ExportFormat, ExportResult, DocumentAST, MetadataHeader, BlockNode } from './types';
import { blockNoteToAST } from './blocknote-to-blocks';
import { astToMarkdown } from './exporters/markdown';
import { astToDocx } from './exporters/docx';
import { astToPdfBlob } from './exporters/pdf';
import { buildFilename, extensionForFormat } from './metadata';
import { saveToDownloads, saveViaDialog, openContainingFolder } from './file-saver';

interface RawSummaryShape {
  summary_json?: unknown; // BlockNote JSON
  markdown?: string;      // plain markdown fallback
  [key: string]: unknown; // legacy {sectionName: {title, blocks}}
}

interface MeetingLike {
  id: string;
  title?: string;
  name?: string;
  created_at: string;
  duration_seconds?: number;
}

export interface ExportSummaryInput {
  format: ExportFormat;
  meeting: MeetingLike;
  summary: RawSummaryShape;
  modelName?: string;
}

function fallbackMarkdownToAST(markdown: string): BlockNode[] {
  // Minimal fallback: preserve content as a single paragraph per non-empty line.
  // Formatting is lost, but the rapport content is readable.
  return markdown
    .split(/\n{2,}/)
    .map((para) => para.trim())
    .filter((para) => para.length > 0)
    .map((para): BlockNode => {
      const headingMatch = para.match(/^(#{1,6})\s+(.+)$/);
      if (headingMatch) {
        const level = headingMatch[1].length as 1 | 2 | 3 | 4 | 5 | 6;
        return { type: 'heading', level, inlines: [{ text: headingMatch[2] }] };
      }
      return { type: 'paragraph', inlines: [{ text: para }] };
    });
}

function legacyShapeToAST(summary: RawSummaryShape): BlockNode[] {
  const blocks: BlockNode[] = [];
  const nonSectionKeys = new Set(['markdown', 'summary_json', '_section_order', 'MeetingName']);
  for (const [key, value] of Object.entries(summary)) {
    if (nonSectionKeys.has(key)) continue;
    if (value && typeof value === 'object' && 'title' in value && 'blocks' in value) {
      const section = value as { title: string; blocks: Array<{ content: string }> };
      blocks.push({ type: 'heading', level: 2, inlines: [{ text: section.title }] });
      blocks.push({
        type: 'bulletList',
        items: section.blocks.map((b) => ({ inlines: [{ text: String(b.content) }] })),
      });
    }
  }
  return blocks;
}

function buildAST(summary: RawSummaryShape, meta: MetadataHeader): DocumentAST {
  if (summary.summary_json) {
    const parsed =
      typeof summary.summary_json === 'string' ? JSON.parse(summary.summary_json) : summary.summary_json;
    return { metadata: meta, blocks: blockNoteToAST(parsed) };
  }
  if (typeof summary.markdown === 'string' && summary.markdown.trim().length > 0) {
    return { metadata: meta, blocks: fallbackMarkdownToAST(summary.markdown) };
  }
  return { metadata: meta, blocks: legacyShapeToAST(summary) };
}

export interface ExportPayload {
  filename: string;
  content: Blob | string;
}

export async function buildExportPayload(input: ExportSummaryInput): Promise<ExportPayload> {
  const { format, meeting, summary, modelName } = input;

  const metadata: MetadataHeader = {
    title: meeting.title ?? meeting.name ?? 'Meeting',
    dateIso: meeting.created_at,
    durationSeconds: meeting.duration_seconds,
    modelName,
  };

  const ast = buildAST(summary, metadata);

  if (ast.blocks.length === 0) {
    throw new Error('Aucun contenu exportable dans ce résumé.');
  }

  const ext = extensionForFormat(format);
  const filename = buildFilename(metadata.title, metadata.dateIso, ext);

  let content: Blob | string;
  switch (format) {
    case 'markdown':
      content = astToMarkdown(ast);
      break;
    case 'docx': {
      const { generate } = astToDocx(ast);
      content = (await generate()).blob;
      break;
    }
    case 'pdf':
      content = await astToPdfBlob(ast);
      break;
  }

  return { filename, content };
}

export async function exportSummary(input: ExportSummaryInput): Promise<ExportResult> {
  const { filename, content } = await buildExportPayload(input);
  return await saveToDownloads(filename, content);
}

export { openContainingFolder, saveToDownloads, saveViaDialog };
