# Export Word/PDF Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a unified "Export" dropdown to the meeting-summary toolbar that exports the generated report in Markdown, Word (.docx), or PDF to the user's Downloads folder.

**Architecture:** Frontend-only conversion. Parse the BlockNote JSON summary (with fallbacks to markdown / legacy JSON) into a shared intermediate AST, then feed it to three format-specific exporters (markdown string, `docx` library, `pdfmake` library). Save via the Tauri fs API to the OS Downloads folder, open folder via `tauri-plugin-shell`.

**Tech Stack:** Next.js 14 / React 18 / TypeScript, Tauri 2.x, `docx` npm (Word), `pdfmake` npm (PDF), `@tauri-apps/plugin-fs`, `tauri-plugin-shell` (new), `@radix-ui/react-dropdown-menu` (already installed), `sonner` (already installed), Vitest (new, for pure-function tests).

**Spec:** [`docs/superpowers/specs/2026-04-19-export-word-pdf-design.md`](../specs/2026-04-19-export-word-pdf-design.md)

---

## File Structure

**New files:**
- `frontend/src/lib/export/index.ts` — orchestrator `exportSummary(format, meeting, summary)`
- `frontend/src/lib/export/types.ts` — `ExportFormat`, `MetadataHeader`, `DocumentAST`
- `frontend/src/lib/export/metadata.ts` — `buildMetadataHeader`, `slugifyFilename`, `formatFrenchDate`
- `frontend/src/lib/export/blocknote-to-blocks.ts` — BlockNote JSON → AST
- `frontend/src/lib/export/exporters/markdown.ts` — AST → markdown string
- `frontend/src/lib/export/exporters/docx.ts` — AST → Blob (via `docx` lib)
- `frontend/src/lib/export/exporters/pdf.ts` — AST → Blob (via `pdfmake`)
- `frontend/src/lib/export/file-saver.ts` — save Blob/string to Downloads + open folder
- `frontend/src/hooks/meeting-details/useExportOperations.ts` — React hook
- `frontend/src/components/MeetingDetails/ExportDropdown.tsx` — dropdown UI
- `frontend/__tests__/lib/export/metadata.test.ts` — unit tests
- `frontend/__tests__/lib/export/blocknote-to-blocks.test.ts`
- `frontend/__tests__/lib/export/exporters/markdown.test.ts`
- `frontend/__tests__/lib/export/exporters/docx.test.ts`
- `frontend/__tests__/lib/export/exporters/pdf.test.ts`
- `frontend/vitest.config.ts`

**Modified files:**
- `frontend/package.json` — add `docx`, `pdfmake`, `@tauri-apps/plugin-shell`, `vitest`, `@vitest/ui`, `jsdom`; move `@tauri-apps/plugin-fs` from devDeps to deps
- `frontend/src-tauri/Cargo.toml` — add `tauri-plugin-shell`
- `frontend/src-tauri/src/lib.rs` — register `tauri_plugin_shell`
- `frontend/src-tauri/tauri.conf.json` — add `shell:allow-open` permission
- `frontend/src/components/MeetingDetails/SummaryUpdaterButtonGroup.tsx` — add `ExportDropdown` between Copy and Save
- `frontend/src/app/meeting-details/page-content.tsx` — wire new hook + pass props
- `frontend/src/components/AISummary/index.tsx:595-606` — remove the dead `handleExport` function

---

## Task 1: Install dependencies and set up Vitest

**Files:**
- Modify: `frontend/package.json`
- Create: `frontend/vitest.config.ts`

- [ ] **Step 1: Install runtime dependencies**

From repo root:

```bash
cd frontend
pnpm add docx@^8.5.0 pdfmake@^0.2.10 @tauri-apps/plugin-shell@^2.2.0 @tauri-apps/plugin-dialog@^2.2.0
pnpm add @tauri-apps/plugin-fs@^2.5.0
pnpm remove -D @tauri-apps/plugin-fs
```

Note: `tauri-plugin-dialog` (Rust side) is already in `Cargo.toml` (line 154), so we only need the JS binding.

(The last two lines move `@tauri-apps/plugin-fs` from devDependencies to dependencies so it's available at runtime.)

- [ ] **Step 2: Install type definitions + Vitest**

```bash
pnpm add -D vitest@^1.6.0 @vitest/ui@^1.6.0 jsdom@^24.0.0 @types/pdfmake@^0.2.9
```

- [ ] **Step 3: Create `frontend/vitest.config.ts`**

```typescript
import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['__tests__/**/*.test.ts', '__tests__/**/*.test.tsx'],
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
    },
  },
});
```

- [ ] **Step 4: Add test scripts to `frontend/package.json`**

In the `"scripts"` block, add two entries:

```json
"test": "vitest run",
"test:watch": "vitest"
```

- [ ] **Step 5: Verify Vitest runs with no tests**

```bash
cd frontend && pnpm test
```

Expected: Vitest starts, reports "No test files found" or equivalent, exits 0 or 1 (either is fine — it means Vitest is installed correctly).

- [ ] **Step 6: Commit**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml frontend/vitest.config.ts
git commit -m "chore(export): install docx/pdfmake/shell plugin and set up Vitest"
```

---

## Task 2: Add and register tauri-plugin-shell

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`
- Modify: `frontend/src-tauri/src/lib.rs`
- Modify: `frontend/src-tauri/tauri.conf.json`

- [ ] **Step 1: Add the plugin to `Cargo.toml`**

In the `[dependencies]` block (around line 151–157, near the other `tauri-plugin-*` entries), add:

```toml
tauri-plugin-shell = "2.2.0"
```

- [ ] **Step 2: Register the plugin in `frontend/src-tauri/src/lib.rs`**

Find the `tauri::Builder::default()` chain in `lib.rs`. Right after the existing plugin registrations (e.g., `.plugin(tauri_plugin_fs::init())`), add:

```rust
.plugin(tauri_plugin_shell::init())
```

If no existing plugin block exists yet in `lib.rs`, locate the builder chain and add it there — the exact position depends on the current state of the file. Ensure it is chained before `.run(...)`.

- [ ] **Step 3: Add shell permissions to `tauri.conf.json`**

In `frontend/src-tauri/tauri.conf.json`, inside the `"capabilities"[0].permissions` array (currently around lines 45–74), add these two entries at the end of the array:

```json
"shell:default",
{
  "identifier": "shell:allow-open",
  "allow": [
    { "path": "**" }
  ]
}
```

The scope `"**"` allows opening any folder — we only ever call it with the user's Downloads folder path, but Tauri requires a scope declaration.

- [ ] **Step 4: Verify it compiles**

```bash
cd frontend/src-tauri && cargo check
```

Expected: compiles with warnings OK, no errors. If an error mentions `tauri-plugin-shell` version, adjust the version to match the installed Tauri CLI.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/src/lib.rs frontend/src-tauri/tauri.conf.json frontend/src-tauri/Cargo.lock
git commit -m "feat(export): register tauri-plugin-shell for opening Downloads folder"
```

---

## Task 3: Define export types and AST

**Files:**
- Create: `frontend/src/lib/export/types.ts`

- [ ] **Step 1: Create the types file**

```typescript
// frontend/src/lib/export/types.ts

export type ExportFormat = 'markdown' | 'docx' | 'pdf';

export interface MetadataHeader {
  title: string;
  dateIso: string;           // ISO date of meeting created_at
  durationSeconds?: number;  // optional
  modelName?: string;        // e.g., "Claude Sonnet 4.6"
}

// --- Document AST ---

export type InlineMark = 'bold' | 'italic' | 'code';

export interface InlineText {
  text: string;
  marks?: InlineMark[];
  link?: string;
}

export type BlockNode =
  | { type: 'heading'; level: 1 | 2 | 3 | 4 | 5 | 6; inlines: InlineText[] }
  | { type: 'paragraph'; inlines: InlineText[] }
  | { type: 'bulletList'; items: ListItem[] }
  | { type: 'numberedList'; items: ListItem[] }
  | { type: 'codeBlock'; language?: string; code: string }
  | { type: 'table'; rows: TableRow[] }
  | { type: 'divider' };

export interface ListItem {
  inlines: InlineText[];
  children?: BlockNode[]; // nested lists
}

export interface TableRow {
  cells: InlineText[][]; // each cell is an array of inlines
  isHeader?: boolean;
}

export interface DocumentAST {
  metadata: MetadataHeader;
  blocks: BlockNode[];
}

export interface ExportResult {
  filename: string;
  fullPath: string;
  byteSize: number;
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/lib/export/types.ts
git commit -m "feat(export): define export types and intermediate AST"
```

---

## Task 4: Metadata utilities (slugify, filename, date formatting)

**Files:**
- Create: `frontend/src/lib/export/metadata.ts`
- Create: `frontend/__tests__/lib/export/metadata.test.ts`

- [ ] **Step 1: Write failing tests**

Create `frontend/__tests__/lib/export/metadata.test.ts`:

```typescript
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
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd frontend && pnpm test -- metadata.test.ts
```

Expected: FAIL with "Cannot find module '@/lib/export/metadata'".

- [ ] **Step 3: Implement `frontend/src/lib/export/metadata.ts`**

```typescript
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd frontend && pnpm test -- metadata.test.ts
```

Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/export/metadata.ts frontend/__tests__/lib/export/metadata.test.ts
git commit -m "feat(export): add metadata utilities (slugify, filename, date formatting)"
```

---

## Task 5: BlockNote JSON → AST converter

**Files:**
- Create: `frontend/src/lib/export/blocknote-to-blocks.ts`
- Create: `frontend/__tests__/lib/export/blocknote-to-blocks.test.ts`

BlockNote 0.36 represents a document as an array of blocks with `{ type, content, children, props }`. We map each known type to our `BlockNode`, and fall back to `paragraph` for unknown types.

- [ ] **Step 1: Write failing tests**

Create `frontend/__tests__/lib/export/blocknote-to-blocks.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { blockNoteToAST } from '@/lib/export/blocknote-to-blocks';

describe('blockNoteToAST', () => {
  it('converts a heading with level', () => {
    const input = [
      {
        type: 'heading',
        props: { level: 2 },
        content: [{ type: 'text', text: 'Decisions', styles: {} }],
      },
    ];
    const result = blockNoteToAST(input);
    expect(result).toEqual([
      {
        type: 'heading',
        level: 2,
        inlines: [{ text: 'Decisions' }],
      },
    ]);
  });

  it('converts a paragraph with inline bold', () => {
    const input = [
      {
        type: 'paragraph',
        content: [
          { type: 'text', text: 'Hello ', styles: {} },
          { type: 'text', text: 'world', styles: { bold: true } },
        ],
      },
    ];
    const result = blockNoteToAST(input);
    expect(result).toEqual([
      {
        type: 'paragraph',
        inlines: [
          { text: 'Hello ' },
          { text: 'world', marks: ['bold'] },
        ],
      },
    ]);
  });

  it('converts a bullet list with items', () => {
    const input = [
      {
        type: 'bulletListItem',
        content: [{ type: 'text', text: 'First', styles: {} }],
      },
      {
        type: 'bulletListItem',
        content: [{ type: 'text', text: 'Second', styles: {} }],
      },
    ];
    const result = blockNoteToAST(input);
    expect(result).toEqual([
      {
        type: 'bulletList',
        items: [
          { inlines: [{ text: 'First' }] },
          { inlines: [{ text: 'Second' }] },
        ],
      },
    ]);
  });

  it('converts a numbered list with items', () => {
    const input = [
      { type: 'numberedListItem', content: [{ type: 'text', text: 'A', styles: {} }] },
      { type: 'numberedListItem', content: [{ type: 'text', text: 'B', styles: {} }] },
    ];
    const result = blockNoteToAST(input);
    expect(result).toEqual([
      {
        type: 'numberedList',
        items: [
          { inlines: [{ text: 'A' }] },
          { inlines: [{ text: 'B' }] },
        ],
      },
    ]);
  });

  it('converts a table with header row', () => {
    const input = [
      {
        type: 'table',
        content: {
          rows: [
            {
              cells: [
                [{ type: 'text', text: 'Task', styles: {} }],
                [{ type: 'text', text: 'Owner', styles: {} }],
              ],
            },
            {
              cells: [
                [{ type: 'text', text: 'Deploy', styles: {} }],
                [{ type: 'text', text: 'Alice', styles: {} }],
              ],
            },
          ],
        },
      },
    ];
    const result = blockNoteToAST(input);
    expect(result).toEqual([
      {
        type: 'table',
        rows: [
          { isHeader: true, cells: [[{ text: 'Task' }], [{ text: 'Owner' }]] },
          { isHeader: false, cells: [[{ text: 'Deploy' }], [{ text: 'Alice' }]] },
        ],
      },
    ]);
  });

  it('falls back to paragraph for unknown block types', () => {
    const input = [
      {
        type: 'someFancyCallout',
        content: [{ type: 'text', text: 'Important!', styles: {} }],
      },
    ];
    const result = blockNoteToAST(input);
    expect(result).toEqual([
      {
        type: 'paragraph',
        inlines: [{ text: 'Important!' }],
      },
    ]);
  });

  it('handles empty content gracefully', () => {
    expect(blockNoteToAST([])).toEqual([]);
    expect(blockNoteToAST(null as any)).toEqual([]);
    expect(blockNoteToAST(undefined as any)).toEqual([]);
  });

  it('merges consecutive bullet items into a single list', () => {
    const input = [
      { type: 'bulletListItem', content: [{ type: 'text', text: 'A', styles: {} }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Interrupt', styles: {} }] },
      { type: 'bulletListItem', content: [{ type: 'text', text: 'B', styles: {} }] },
    ];
    const result = blockNoteToAST(input);
    expect(result).toHaveLength(3);
    expect(result[0].type).toBe('bulletList');
    expect(result[1].type).toBe('paragraph');
    expect(result[2].type).toBe('bulletList');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd frontend && pnpm test -- blocknote-to-blocks.test.ts
```

Expected: FAIL with "Cannot find module".

- [ ] **Step 3: Implement `frontend/src/lib/export/blocknote-to-blocks.ts`**

```typescript
import type { BlockNode, InlineText, InlineMark, ListItem, TableRow } from './types';

interface BNInlineContent {
  type: string;
  text?: string;
  styles?: { bold?: boolean; italic?: boolean; code?: boolean };
  href?: string;
  content?: BNInlineContent[];
}

interface BNBlock {
  type: string;
  props?: Record<string, any>;
  content?: BNInlineContent[] | { rows: Array<{ cells: BNInlineContent[][] }> };
  children?: BNBlock[];
}

function mapStylesToMarks(styles?: { bold?: boolean; italic?: boolean; code?: boolean }): InlineMark[] | undefined {
  if (!styles) return undefined;
  const marks: InlineMark[] = [];
  if (styles.bold) marks.push('bold');
  if (styles.italic) marks.push('italic');
  if (styles.code) marks.push('code');
  return marks.length > 0 ? marks : undefined;
}

function toInlines(content: BNInlineContent[] | undefined): InlineText[] {
  if (!content) return [];
  return content
    .filter((c) => c.type === 'text' || c.type === 'link')
    .map((c): InlineText => {
      if (c.type === 'link' && c.content) {
        const first = c.content[0];
        const inline: InlineText = { text: first?.text ?? '', link: c.href };
        const marks = mapStylesToMarks(first?.styles);
        if (marks) inline.marks = marks;
        return inline;
      }
      const inline: InlineText = { text: c.text ?? '' };
      const marks = mapStylesToMarks(c.styles);
      if (marks) inline.marks = marks;
      return inline;
    });
}

function toListItem(block: BNBlock): ListItem {
  const content = block.content as BNInlineContent[] | undefined;
  return { inlines: toInlines(content) };
}

function toTable(block: BNBlock): BlockNode {
  const tableContent = block.content as { rows: Array<{ cells: BNInlineContent[][] }> } | undefined;
  if (!tableContent || !Array.isArray(tableContent.rows)) {
    return { type: 'paragraph', inlines: [] };
  }
  const rows: TableRow[] = tableContent.rows.map((row, rowIdx) => ({
    isHeader: rowIdx === 0,
    cells: row.cells.map((cellInlines) => toInlines(cellInlines)),
  }));
  return { type: 'table', rows };
}

export function blockNoteToAST(input: unknown): BlockNode[] {
  if (!Array.isArray(input)) return [];

  const result: BlockNode[] = [];

  for (const raw of input as BNBlock[]) {
    const block = raw as BNBlock;

    switch (block.type) {
      case 'heading': {
        const rawLevel = block.props?.level;
        const level = (typeof rawLevel === 'number' && rawLevel >= 1 && rawLevel <= 6 ? rawLevel : 1) as 1 | 2 | 3 | 4 | 5 | 6;
        result.push({ type: 'heading', level, inlines: toInlines(block.content as BNInlineContent[]) });
        break;
      }
      case 'paragraph': {
        result.push({ type: 'paragraph', inlines: toInlines(block.content as BNInlineContent[]) });
        break;
      }
      case 'bulletListItem': {
        const last = result[result.length - 1];
        if (last && last.type === 'bulletList') {
          last.items.push(toListItem(block));
        } else {
          result.push({ type: 'bulletList', items: [toListItem(block)] });
        }
        break;
      }
      case 'numberedListItem': {
        const last = result[result.length - 1];
        if (last && last.type === 'numberedList') {
          last.items.push(toListItem(block));
        } else {
          result.push({ type: 'numberedList', items: [toListItem(block)] });
        }
        break;
      }
      case 'codeBlock': {
        const textContent = Array.isArray(block.content)
          ? block.content.map((c: BNInlineContent) => c.text ?? '').join('')
          : '';
        result.push({ type: 'codeBlock', language: block.props?.language, code: textContent });
        break;
      }
      case 'table': {
        result.push(toTable(block));
        break;
      }
      default: {
        // Fallback: treat unknown block as a paragraph preserving text content
        const fallbackInlines = Array.isArray(block.content)
          ? toInlines(block.content as BNInlineContent[])
          : [];
        result.push({ type: 'paragraph', inlines: fallbackInlines });
      }
    }
  }

  return result;
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd frontend && pnpm test -- blocknote-to-blocks.test.ts
```

Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/export/blocknote-to-blocks.ts frontend/__tests__/lib/export/blocknote-to-blocks.test.ts
git commit -m "feat(export): convert BlockNote JSON to intermediate AST"
```

---

## Task 6: Markdown exporter

**Files:**
- Create: `frontend/src/lib/export/exporters/markdown.ts`
- Create: `frontend/__tests__/lib/export/exporters/markdown.test.ts`

- [ ] **Step 1: Write failing tests**

Create `frontend/__tests__/lib/export/exporters/markdown.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { astToMarkdown } from '@/lib/export/exporters/markdown';
import type { DocumentAST } from '@/lib/export/types';

const baseMeta = {
  title: 'Standup',
  dateIso: '2026-04-19T10:00:00Z',
  durationSeconds: 1380,
  modelName: 'Claude Sonnet 4.6',
};

describe('astToMarkdown', () => {
  it('emits YAML frontmatter with metadata', () => {
    const ast: DocumentAST = { metadata: baseMeta, blocks: [] };
    const md = astToMarkdown(ast);
    expect(md).toMatch(/^---\n/);
    expect(md).toContain('title: "Standup"');
    expect(md).toContain('date: "19 avril 2026"');
    expect(md).toContain('duration: "23min"');
    expect(md).toContain('model: "Claude Sonnet 4.6"');
    expect(md).toMatch(/---\n/);
  });

  it('renders a heading with # prefix by level', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [{ type: 'heading', level: 2, inlines: [{ text: 'Decisions' }] }],
    };
    expect(astToMarkdown(ast)).toContain('## Decisions');
  });

  it('renders inline bold and italic', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [
        {
          type: 'paragraph',
          inlines: [
            { text: 'Ship ' },
            { text: 'now', marks: ['bold'] },
            { text: ' or ' },
            { text: 'later', marks: ['italic'] },
          ],
        },
      ],
    };
    expect(astToMarkdown(ast)).toContain('Ship **now** or *later*');
  });

  it('renders a bullet list', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [
        {
          type: 'bulletList',
          items: [
            { inlines: [{ text: 'First' }] },
            { inlines: [{ text: 'Second' }] },
          ],
        },
      ],
    };
    const md = astToMarkdown(ast);
    expect(md).toContain('- First');
    expect(md).toContain('- Second');
  });

  it('renders a table in GFM format', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [
        {
          type: 'table',
          rows: [
            { isHeader: true, cells: [[{ text: 'A' }], [{ text: 'B' }]] },
            { isHeader: false, cells: [[{ text: '1' }], [{ text: '2' }]] },
          ],
        },
      ],
    };
    const md = astToMarkdown(ast);
    expect(md).toContain('| A | B |');
    expect(md).toContain('| --- | --- |');
    expect(md).toContain('| 1 | 2 |');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd frontend && pnpm test -- markdown.test.ts
```

Expected: FAIL with "Cannot find module".

- [ ] **Step 3: Implement `frontend/src/lib/export/exporters/markdown.ts`**

```typescript
import type { DocumentAST, BlockNode, InlineText } from '../types';
import { formatFrenchDate, formatDuration } from '../metadata';

function renderInlines(inlines: InlineText[]): string {
  return inlines
    .map((i) => {
      let text = i.text;
      if (i.marks?.includes('code')) text = `\`${text}\``;
      if (i.marks?.includes('bold')) text = `**${text}**`;
      if (i.marks?.includes('italic')) text = `*${text}*`;
      if (i.link) text = `[${text}](${i.link})`;
      return text;
    })
    .join('');
}

function renderBlock(block: BlockNode): string {
  switch (block.type) {
    case 'heading':
      return `${'#'.repeat(block.level)} ${renderInlines(block.inlines)}`;
    case 'paragraph':
      return renderInlines(block.inlines);
    case 'bulletList':
      return block.items.map((item) => `- ${renderInlines(item.inlines)}`).join('\n');
    case 'numberedList':
      return block.items.map((item, idx) => `${idx + 1}. ${renderInlines(item.inlines)}`).join('\n');
    case 'codeBlock':
      return '```' + (block.language ?? '') + '\n' + block.code + '\n```';
    case 'table': {
      if (block.rows.length === 0) return '';
      const header = block.rows[0];
      const body = block.rows.slice(1);
      const cellText = (cell: InlineText[]) => renderInlines(cell).replace(/\|/g, '\\|');
      const lines: string[] = [];
      lines.push('| ' + header.cells.map(cellText).join(' | ') + ' |');
      lines.push('| ' + header.cells.map(() => '---').join(' | ') + ' |');
      for (const row of body) {
        lines.push('| ' + row.cells.map(cellText).join(' | ') + ' |');
      }
      return lines.join('\n');
    }
    case 'divider':
      return '---';
  }
}

function yamlEscape(value: string): string {
  return `"${value.replace(/"/g, '\\"')}"`;
}

export function astToMarkdown(ast: DocumentAST): string {
  const { metadata, blocks } = ast;

  const fmLines: string[] = ['---'];
  fmLines.push(`title: ${yamlEscape(metadata.title)}`);
  fmLines.push(`date: ${yamlEscape(formatFrenchDate(metadata.dateIso))}`);
  const duration = formatDuration(metadata.durationSeconds);
  if (duration) fmLines.push(`duration: ${yamlEscape(duration)}`);
  if (metadata.modelName) fmLines.push(`model: ${yamlEscape(metadata.modelName)}`);
  fmLines.push('---');
  fmLines.push('');

  const body = blocks.map(renderBlock).join('\n\n');

  return fmLines.join('\n') + '\n' + body + '\n';
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd frontend && pnpm test -- markdown.test.ts
```

Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/export/exporters/markdown.ts frontend/__tests__/lib/export/exporters/markdown.test.ts
git commit -m "feat(export): AST to markdown exporter with YAML frontmatter"
```

---

## Task 7: DOCX exporter

**Files:**
- Create: `frontend/src/lib/export/exporters/docx.ts`
- Create: `frontend/__tests__/lib/export/exporters/docx.test.ts`

The `docx` lib returns a `Document` object. We test that the conversion produces the right node types/structure by inspecting the returned `Document`'s section children (rather than generating actual files).

- [ ] **Step 1: Write failing tests**

Create `frontend/__tests__/lib/export/exporters/docx.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { Paragraph, Table, HeadingLevel } from 'docx';
import { astToDocx } from '@/lib/export/exporters/docx';
import type { DocumentAST } from '@/lib/export/types';

const baseMeta = {
  title: 'Standup',
  dateIso: '2026-04-19T10:00:00Z',
  durationSeconds: 1380,
  modelName: 'Claude Sonnet 4.6',
};

function getSectionChildren(doc: any): any[] {
  // The docx Document exposes internal sections via `.Sections` (private),
  // but we can access it through the public API used during build.
  // For testing, we call a helper that returns the children we built.
  return doc.__testChildren__ as any[];
}

describe('astToDocx', () => {
  it('produces a metadata header paragraph followed by body', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [{ type: 'paragraph', inlines: [{ text: 'Hello' }] }],
    };
    const { document } = astToDocx(ast);
    const children = getSectionChildren(document);
    // First children are metadata header paragraphs
    expect(children.length).toBeGreaterThan(1);
    expect(children[0]).toBeInstanceOf(Paragraph);
  });

  it('maps a level-2 heading to HeadingLevel.HEADING_2', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [{ type: 'heading', level: 2, inlines: [{ text: 'Decisions' }] }],
    };
    const { document } = astToDocx(ast);
    const children = getSectionChildren(document);
    const lastBlock = children[children.length - 1];
    expect(lastBlock).toBeInstanceOf(Paragraph);
    // Heading level is stored in the paragraph's options
    const heading = (lastBlock as any).options?.heading;
    expect(heading).toBe(HeadingLevel.HEADING_2);
  });

  it('maps a table AST to a docx Table', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [
        {
          type: 'table',
          rows: [
            { isHeader: true, cells: [[{ text: 'A' }], [{ text: 'B' }]] },
            { isHeader: false, cells: [[{ text: '1' }], [{ text: '2' }]] },
          ],
        },
      ],
    };
    const { document } = astToDocx(ast);
    const children = getSectionChildren(document);
    const tableChild = children.find((c) => c instanceof Table);
    expect(tableChild).toBeInstanceOf(Table);
  });

  it('returns a Blob from the public generate helper', async () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [{ type: 'paragraph', inlines: [{ text: 'Hi' }] }],
    };
    const { blob } = await astToDocx(ast).generate();
    expect(blob).toBeInstanceOf(Blob);
    expect(blob.size).toBeGreaterThan(0);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd frontend && pnpm test -- docx.test.ts
```

Expected: FAIL with "Cannot find module".

- [ ] **Step 3: Implement `frontend/src/lib/export/exporters/docx.ts`**

```typescript
import {
  Document,
  Packer,
  Paragraph,
  TextRun,
  HeadingLevel,
  Table,
  TableRow as DocxTableRow,
  TableCell,
  AlignmentType,
  PageNumber,
  Footer,
  ExternalHyperlink,
} from 'docx';
import type { DocumentAST, BlockNode, InlineText, TableRow as AstTableRow } from '../types';
import { formatFrenchDate, formatDuration } from '../metadata';

function headingLevelFor(level: 1 | 2 | 3 | 4 | 5 | 6): (typeof HeadingLevel)[keyof typeof HeadingLevel] {
  const map = {
    1: HeadingLevel.HEADING_1,
    2: HeadingLevel.HEADING_2,
    3: HeadingLevel.HEADING_3,
    4: HeadingLevel.HEADING_4,
    5: HeadingLevel.HEADING_5,
    6: HeadingLevel.HEADING_6,
  } as const;
  return map[level];
}

function renderInlineRuns(inlines: InlineText[]): (TextRun | ExternalHyperlink)[] {
  return inlines.map((i) => {
    const run = new TextRun({
      text: i.text,
      bold: i.marks?.includes('bold'),
      italics: i.marks?.includes('italic'),
      font: i.marks?.includes('code') ? 'Courier New' : undefined,
    });
    if (i.link) {
      return new ExternalHyperlink({ link: i.link, children: [run] });
    }
    return run;
  });
}

function renderCell(cell: InlineText[]): TableCell {
  return new TableCell({ children: [new Paragraph({ children: renderInlineRuns(cell) })] });
}

function renderTable(rows: AstTableRow[]): Table {
  return new Table({
    rows: rows.map(
      (row) =>
        new DocxTableRow({
          tableHeader: row.isHeader,
          children: row.cells.map(renderCell),
        }),
    ),
  });
}

function renderBlock(block: BlockNode): (Paragraph | Table)[] {
  switch (block.type) {
    case 'heading':
      return [new Paragraph({ heading: headingLevelFor(block.level), children: renderInlineRuns(block.inlines) })];
    case 'paragraph':
      return [new Paragraph({ children: renderInlineRuns(block.inlines) })];
    case 'bulletList':
      return block.items.map(
        (item) =>
          new Paragraph({
            bullet: { level: 0 },
            children: renderInlineRuns(item.inlines),
          }),
      );
    case 'numberedList':
      return block.items.map(
        (item) =>
          new Paragraph({
            numbering: { reference: 'ordered', level: 0 },
            children: renderInlineRuns(item.inlines),
          }),
      );
    case 'codeBlock':
      return [
        new Paragraph({
          children: [new TextRun({ text: block.code, font: 'Courier New' })],
        }),
      ];
    case 'table':
      return [renderTable(block.rows)];
    case 'divider':
      return [new Paragraph({ children: [new TextRun('—'.repeat(40))] })];
  }
}

function buildMetadataHeaderParagraphs(meta: DocumentAST['metadata']): Paragraph[] {
  const lines: Paragraph[] = [];

  lines.push(
    new Paragraph({
      heading: HeadingLevel.HEADING_1,
      children: [new TextRun({ text: meta.title, bold: true })],
    }),
  );

  const metaBits: string[] = [formatFrenchDate(meta.dateIso)];
  const duration = formatDuration(meta.durationSeconds);
  if (duration) metaBits.push(duration);
  if (meta.modelName) metaBits.push(`Généré avec ${meta.modelName}`);
  lines.push(
    new Paragraph({
      alignment: AlignmentType.LEFT,
      children: [new TextRun({ text: metaBits.join(' · '), italics: true, size: 20 })],
    }),
  );

  lines.push(new Paragraph({ children: [new TextRun('')] })); // spacer
  return lines;
}

interface DocxResult {
  document: Document & { __testChildren__?: any[] };
  generate: () => Promise<{ blob: Blob }>;
}

export function astToDocx(ast: DocumentAST): DocxResult {
  const headerChildren = buildMetadataHeaderParagraphs(ast.metadata);
  const bodyChildren = ast.blocks.flatMap(renderBlock);
  const allChildren = [...headerChildren, ...bodyChildren];

  const doc = new Document({
    numbering: {
      config: [
        {
          reference: 'ordered',
          levels: [
            {
              level: 0,
              format: 'decimal',
              text: '%1.',
              alignment: AlignmentType.START,
            },
          ],
        },
      ],
    },
    sections: [
      {
        children: allChildren,
        footers: {
          default: new Footer({
            children: [
              new Paragraph({
                alignment: AlignmentType.CENTER,
                children: [new TextRun({ children: [PageNumber.CURRENT] })],
              }),
            ],
          }),
        },
      },
    ],
  }) as Document & { __testChildren__?: any[] };

  // Expose for tests — harmless in production (no serialization impact)
  doc.__testChildren__ = allChildren;

  return {
    document: doc,
    generate: async () => {
      const blob = await Packer.toBlob(doc);
      return { blob };
    },
  };
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd frontend && pnpm test -- docx.test.ts
```

Expected: all tests PASS. If the `Packer.toBlob` test fails in the jsdom env because of missing APIs, we can skip that test with `it.skip` and rely on manual verification in Task 14.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/export/exporters/docx.ts frontend/__tests__/lib/export/exporters/docx.test.ts
git commit -m "feat(export): AST to docx exporter with headings, tables, page numbers"
```

---

## Task 8: PDF exporter (pdfmake)

**Files:**
- Create: `frontend/src/lib/export/exporters/pdf.ts`
- Create: `frontend/__tests__/lib/export/exporters/pdf.test.ts`

`pdfmake` takes a declarative JSON `docDefinition`. We test the JSON shape, not the actual PDF bytes.

- [ ] **Step 1: Write failing tests**

Create `frontend/__tests__/lib/export/exporters/pdf.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { astToPdfDefinition } from '@/lib/export/exporters/pdf';
import type { DocumentAST } from '@/lib/export/types';

const baseMeta = {
  title: 'Standup',
  dateIso: '2026-04-19T10:00:00Z',
  durationSeconds: 1380,
  modelName: 'Claude Sonnet 4.6',
};

describe('astToPdfDefinition', () => {
  it('builds a docDefinition with content, styles and page numbers', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [{ type: 'paragraph', inlines: [{ text: 'Hi' }] }],
    };
    const def = astToPdfDefinition(ast);
    expect(def.content).toBeInstanceOf(Array);
    expect(def.styles).toBeDefined();
    expect(typeof def.footer).toBe('function');
  });

  it('produces a header block at the top with title + metadata line', () => {
    const ast: DocumentAST = { metadata: baseMeta, blocks: [] };
    const def = astToPdfDefinition(ast);
    const first = def.content[0];
    expect(first.text).toBe('Standup');
    expect(first.style).toBe('title');
    const second = def.content[1];
    expect(second.text).toContain('19 avril 2026');
    expect(second.text).toContain('23min');
    expect(second.text).toContain('Claude Sonnet 4.6');
  });

  it('emits heading entries with correct style', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [{ type: 'heading', level: 2, inlines: [{ text: 'Decisions' }] }],
    };
    const def = astToPdfDefinition(ast);
    const heading = def.content[def.content.length - 1];
    expect(heading.style).toBe('h2');
    // text in pdfmake is either a string or an array of runs
    if (typeof heading.text === 'string') {
      expect(heading.text).toBe('Decisions');
    } else {
      expect(heading.text[0].text).toBe('Decisions');
    }
  });

  it('emits a table entry with header + body rows', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [
        {
          type: 'table',
          rows: [
            { isHeader: true, cells: [[{ text: 'A' }], [{ text: 'B' }]] },
            { isHeader: false, cells: [[{ text: '1' }], [{ text: '2' }]] },
          ],
        },
      ],
    };
    const def = astToPdfDefinition(ast);
    const last = def.content[def.content.length - 1];
    expect(last.table).toBeDefined();
    expect(last.table.body.length).toBe(2); // header + body row
  });

  it('renders bullet list via ul property', () => {
    const ast: DocumentAST = {
      metadata: baseMeta,
      blocks: [
        { type: 'bulletList', items: [{ inlines: [{ text: 'A' }] }, { inlines: [{ text: 'B' }] }] },
      ],
    };
    const def = astToPdfDefinition(ast);
    const last = def.content[def.content.length - 1];
    expect(last.ul).toBeDefined();
    expect(last.ul.length).toBe(2);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd frontend && pnpm test -- pdf.test.ts
```

Expected: FAIL with "Cannot find module".

- [ ] **Step 3: Implement `frontend/src/lib/export/exporters/pdf.ts`**

```typescript
import type { DocumentAST, BlockNode, InlineText } from '../types';
import { formatFrenchDate, formatDuration } from '../metadata';

interface PdfRun {
  text: string;
  bold?: boolean;
  italics?: boolean;
  link?: string;
  color?: string;
  decoration?: string;
  style?: string;
}

function inlinesToRuns(inlines: InlineText[]): PdfRun[] {
  return inlines.map((i) => {
    const run: PdfRun = { text: i.text };
    if (i.marks?.includes('bold')) run.bold = true;
    if (i.marks?.includes('italic')) run.italics = true;
    if (i.marks?.includes('code')) run.style = 'code';
    if (i.link) {
      run.link = i.link;
      run.color = '#1a73e8';
      run.decoration = 'underline';
    }
    return run;
  });
}

function renderBlock(block: BlockNode): any {
  switch (block.type) {
    case 'heading':
      return { text: inlinesToRuns(block.inlines), style: `h${block.level}`, margin: [0, 10, 0, 6] };
    case 'paragraph':
      return { text: inlinesToRuns(block.inlines), margin: [0, 0, 0, 6] };
    case 'bulletList':
      return { ul: block.items.map((item) => ({ text: inlinesToRuns(item.inlines) })), margin: [0, 0, 0, 6] };
    case 'numberedList':
      return { ol: block.items.map((item) => ({ text: inlinesToRuns(item.inlines) })), margin: [0, 0, 0, 6] };
    case 'codeBlock':
      return { text: block.code, style: 'code', margin: [0, 4, 0, 6] };
    case 'table':
      return {
        table: {
          headerRows: block.rows[0]?.isHeader ? 1 : 0,
          widths: block.rows[0]?.cells.map(() => '*') ?? [],
          body: block.rows.map((row) =>
            row.cells.map((cell) => ({
              text: inlinesToRuns(cell),
              bold: row.isHeader,
              fillColor: row.isHeader ? '#f3f4f6' : undefined,
            })),
          ),
        },
        margin: [0, 4, 0, 10],
      };
    case 'divider':
      return { canvas: [{ type: 'line', x1: 0, y1: 0, x2: 515, y2: 0, lineWidth: 0.5, lineColor: '#cccccc' }] };
  }
}

export function astToPdfDefinition(ast: DocumentAST): any {
  const metaBits: string[] = [formatFrenchDate(ast.metadata.dateIso)];
  const duration = formatDuration(ast.metadata.durationSeconds);
  if (duration) metaBits.push(duration);
  if (ast.metadata.modelName) metaBits.push(`Généré avec ${ast.metadata.modelName}`);

  const header: any[] = [
    { text: ast.metadata.title, style: 'title', margin: [0, 0, 0, 2] },
    { text: metaBits.join(' · '), style: 'metaLine', margin: [0, 0, 0, 16] },
  ];

  const body = ast.blocks.map(renderBlock);

  return {
    pageSize: 'A4',
    pageMargins: [50, 50, 50, 60],
    content: [...header, ...body],
    footer: (currentPage: number, pageCount: number) => ({
      text: `${currentPage} / ${pageCount}`,
      alignment: 'center',
      fontSize: 9,
      color: '#666666',
      margin: [0, 20, 0, 0],
    }),
    styles: {
      title: { fontSize: 22, bold: true },
      metaLine: { fontSize: 10, italics: true, color: '#555555' },
      h1: { fontSize: 18, bold: true },
      h2: { fontSize: 15, bold: true },
      h3: { fontSize: 13, bold: true },
      h4: { fontSize: 12, bold: true },
      h5: { fontSize: 11, bold: true },
      h6: { fontSize: 11, italics: true },
      code: { font: 'Courier', fontSize: 10 },
    },
    defaultStyle: {
      fontSize: 11,
    },
  };
}

export async function astToPdfBlob(ast: DocumentAST): Promise<Blob> {
  // Dynamic import so jsdom unit tests (Task 8 tests) don't pull in pdfmake fonts
  const pdfMake = (await import('pdfmake/build/pdfmake')).default as any;
  const pdfFonts = (await import('pdfmake/build/vfs_fonts')).default as any;
  pdfMake.vfs = pdfFonts.pdfMake?.vfs ?? pdfFonts.vfs ?? pdfFonts;

  const definition = astToPdfDefinition(ast);
  return new Promise<Blob>((resolve) => {
    pdfMake.createPdf(definition).getBlob((blob: Blob) => resolve(blob));
  });
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd frontend && pnpm test -- pdf.test.ts
```

Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/export/exporters/pdf.ts frontend/__tests__/lib/export/exporters/pdf.test.ts
git commit -m "feat(export): AST to pdfmake definition + blob generator"
```

---

## Task 9: File saver (Tauri fs + shell integration)

**Files:**
- Create: `frontend/src/lib/export/file-saver.ts`

This module has no pure unit tests — it depends on Tauri runtime. Manual verification in Task 14.

- [ ] **Step 1: Implement `frontend/src/lib/export/file-saver.ts`**

```typescript
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
```

- [ ] **Step 2: Verify it type-checks**

```bash
cd frontend && npx tsc --noEmit
```

Expected: no TypeScript errors related to this file. (Other pre-existing errors may remain — you can filter by filename if needed.)

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/export/file-saver.ts
git commit -m "feat(export): file saver that writes to Downloads and opens the folder"
```

---

## Task 10: Main exportSummary orchestrator

**Files:**
- Create: `frontend/src/lib/export/index.ts`

- [ ] **Step 1: Implement `frontend/src/lib/export/index.ts`**

```typescript
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
```

- [ ] **Step 2: Verify it type-checks**

```bash
cd frontend && npx tsc --noEmit
```

Expected: no TypeScript errors in the export/ folder.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/export/index.ts
git commit -m "feat(export): orchestrator that picks source, routes to format, saves file"
```

---

## Task 11: useExportOperations hook

**Files:**
- Create: `frontend/src/hooks/meeting-details/useExportOperations.ts`

- [ ] **Step 1: Implement the hook**

```typescript
import { useCallback, useState, RefObject } from 'react';
import { toast } from 'sonner';
import type { Summary } from '@/types';
import type { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { buildExportPayload, saveToDownloads, saveViaDialog, openContainingFolder } from '@/lib/export';
import type { ExportFormat } from '@/lib/export/types';

interface UseExportOperationsProps {
  meeting: any;
  meetingTitle: string;
  aiSummary: Summary | null;
  blockNoteSummaryRef: RefObject<BlockNoteSummaryViewRef>;
  modelName?: string;
}

function looksLikePermissionError(err: any): boolean {
  const msg = String(err?.message ?? err ?? '').toLowerCase();
  return (
    msg.includes('permission') ||
    msg.includes('denied') ||
    msg.includes('not allowed') ||
    msg.includes('forbidden')
  );
}

export function useExportOperations({
  meeting,
  meetingTitle,
  aiSummary,
  blockNoteSummaryRef,
  modelName,
}: UseExportOperationsProps) {
  const [exportingFormat, setExportingFormat] = useState<ExportFormat | null>(null);

  const runExport = useCallback(
    async (format: ExportFormat) => {
      if (!aiSummary) {
        toast.error('Aucun résumé à exporter');
        return;
      }
      setExportingFormat(format);
      try {
        // Prefer the live BlockNote editor content, so in-editor edits are captured.
        let summaryShape: any = aiSummary;
        if (blockNoteSummaryRef.current?.getDocument) {
          try {
            const liveDoc = await blockNoteSummaryRef.current.getDocument();
            if (liveDoc) {
              summaryShape = { ...aiSummary, summary_json: liveDoc };
            }
          } catch {
            // fall back to stored aiSummary
          }
        }

        const payload = await buildExportPayload({
          format,
          meeting: { ...meeting, title: meetingTitle ?? meeting.title },
          summary: summaryShape,
          modelName,
        });

        let result;
        try {
          result = await saveToDownloads(payload.filename, payload.content);
        } catch (saveErr: any) {
          if (!looksLikePermissionError(saveErr)) throw saveErr;
          // Fallback: offer the native Save As dialog.
          const dialogResult = await saveViaDialog(payload.filename, payload.content);
          if (!dialogResult) {
            toast.info('Export annulé');
            return;
          }
          result = dialogResult;
        }

        toast.success(`Rapport exporté : ${result.filename}`, {
          action: {
            label: 'Ouvrir le dossier',
            onClick: () => {
              openContainingFolder(result.fullPath).catch(() =>
                toast.error("Impossible d'ouvrir le dossier"),
              );
            },
          },
        });
      } catch (err: any) {
        const message = err?.message ?? String(err);
        toast.error(`Échec de l'export : ${message}`);
        console.error('[export] failed:', err);
      } finally {
        setExportingFormat(null);
      }
    },
    [aiSummary, blockNoteSummaryRef, meeting, meetingTitle, modelName],
  );

  return {
    exportingFormat,
    handleExportMarkdown: useCallback(() => runExport('markdown'), [runExport]),
    handleExportDocx: useCallback(() => runExport('docx'), [runExport]),
    handleExportPdf: useCallback(() => runExport('pdf'), [runExport]),
  };
}
```

- [ ] **Step 2: Verify `BlockNoteSummaryViewRef.getDocument` exists**

```bash
cd frontend && grep -n "getDocument" src/components/AISummary/BlockNoteSummaryView.tsx
```

**If `getDocument` is NOT present on the ref interface:**
- Read `src/components/AISummary/BlockNoteSummaryView.tsx` around the `useImperativeHandle` block.
- Extend the ref to expose `getDocument: () => Promise<unknown>` that returns `editor.document` (the BlockNote JSON).
- Update the `BlockNoteSummaryViewRef` type export accordingly.

**If `getDocument` IS present:** proceed to step 3.

- [ ] **Step 3: Verify it type-checks**

```bash
cd frontend && npx tsc --noEmit
```

Expected: no new errors in this file.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/meeting-details/useExportOperations.ts
git commit -m "feat(export): useExportOperations hook with loading state and toasts"
```

---

## Task 12: ExportDropdown component

**Files:**
- Create: `frontend/src/components/MeetingDetails/ExportDropdown.tsx`

- [ ] **Step 1: Verify existence of a `DropdownMenu` wrapper in `@/components/ui`**

```bash
cd frontend && ls src/components/ui | grep -i dropdown
```

Expected: a file like `dropdown-menu.tsx` exists. (If it doesn't, fall back to importing directly from `@radix-ui/react-dropdown-menu` — the import changes in the component below.)

- [ ] **Step 2: Implement the component**

```typescript
'use client';

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
import { Download, FileText, FileType, FileDown, Loader2 } from 'lucide-react';
import type { ExportFormat } from '@/lib/export/types';

interface ExportDropdownProps {
  hasSummary: boolean;
  isGenerating?: boolean;
  exportingFormat: ExportFormat | null;
  onExportMarkdown: () => void;
  onExportDocx: () => void;
  onExportPdf: () => void;
}

export function ExportDropdown({
  hasSummary,
  isGenerating = false,
  exportingFormat,
  onExportMarkdown,
  onExportDocx,
  onExportPdf,
}: ExportDropdownProps) {
  const isBusy = exportingFormat !== null;
  const disabled = !hasSummary || isBusy || isGenerating;
  const titleText = !hasSummary
    ? 'Générer un résumé d\u2019abord'
    : isGenerating
    ? 'Résumé en cours de génération\u2026'
    : 'Export';

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          title={titleText}
          disabled={disabled}
          className="cursor-pointer"
        >
          {isBusy ? (
            <>
              <Loader2 className="animate-spin" />
              <span className="hidden lg:inline">Export&hellip;</span>
            </>
          ) : (
            <>
              <Download />
              <span className="hidden lg:inline">Export</span>
            </>
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={onExportMarkdown} disabled={isBusy}>
          <FileText className="mr-2 h-4 w-4" />
          Markdown (.md)
        </DropdownMenuItem>
        <DropdownMenuItem onClick={onExportDocx} disabled={isBusy}>
          <FileType className="mr-2 h-4 w-4" />
          Word (.docx)
        </DropdownMenuItem>
        <DropdownMenuItem onClick={onExportPdf} disabled={isBusy}>
          <FileDown className="mr-2 h-4 w-4" />
          PDF (.pdf)
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
```

**If Step 1 revealed there is no `src/components/ui/dropdown-menu.tsx`:** replace the first `import` with:

```typescript
import * as DropdownMenuPrimitive from '@radix-ui/react-dropdown-menu';
const DropdownMenu = DropdownMenuPrimitive.Root;
const DropdownMenuTrigger = DropdownMenuPrimitive.Trigger;
const DropdownMenuContent = DropdownMenuPrimitive.Content;
const DropdownMenuItem = DropdownMenuPrimitive.Item;
```

and add appropriate Tailwind classes on `DropdownMenuContent` / `DropdownMenuItem` for visual consistency.

- [ ] **Step 3: Verify it type-checks**

```bash
cd frontend && npx tsc --noEmit
```

Expected: no new errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/MeetingDetails/ExportDropdown.tsx
git commit -m "feat(export): ExportDropdown component with markdown/docx/pdf options"
```

---

## Task 13: Wire ExportDropdown into toolbar and page

**Files:**
- Modify: `frontend/src/components/MeetingDetails/SummaryUpdaterButtonGroup.tsx`
- Modify: `frontend/src/app/meeting-details/page-content.tsx`
- Modify: `frontend/src/components/AISummary/index.tsx` (remove dead handleExport)

- [ ] **Step 1: Extend `SummaryUpdaterButtonGroup.tsx` to accept export props and render the dropdown between Copy and Save**

Open `frontend/src/components/MeetingDetails/SummaryUpdaterButtonGroup.tsx`. Update the imports and component as follows:

```typescript
"use client";

import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, Save, Loader2, Search, FolderOpen } from 'lucide-react';
import { ExportDropdown } from '@/components/MeetingDetails/ExportDropdown';
import type { ExportFormat } from '@/lib/export/types';

interface SummaryUpdaterButtonGroupProps {
  isSaving: boolean;
  isDirty: boolean;
  onSave: () => Promise<void>;
  onCopy: () => Promise<void>;
  onFind?: () => void;
  onOpenFolder: () => Promise<void>;
  hasSummary: boolean;
  // --- new export props ---
  isGeneratingSummary?: boolean;
  exportingFormat: ExportFormat | null;
  onExportMarkdown: () => void;
  onExportDocx: () => void;
  onExportPdf: () => void;
}

export function SummaryUpdaterButtonGroup({
  isSaving,
  isDirty,
  onSave,
  onCopy,
  onFind,
  onOpenFolder,
  hasSummary,
  isGeneratingSummary = false,
  exportingFormat,
  onExportMarkdown,
  onExportDocx,
  onExportPdf,
}: SummaryUpdaterButtonGroupProps) {
  return (
    <ButtonGroup>
      {/* Copy button */}
      <Button
        variant="outline"
        size="sm"
        title="Copy Summary"
        onClick={() => { onCopy(); }}
        disabled={!hasSummary}
        className="cursor-pointer"
      >
        <Copy />
        <span className="hidden lg:inline">Copy</span>
      </Button>

      {/* Export dropdown — between Copy and Save per spec */}
      <ExportDropdown
        hasSummary={hasSummary}
        isGenerating={isGeneratingSummary}
        exportingFormat={exportingFormat}
        onExportMarkdown={onExportMarkdown}
        onExportDocx={onExportDocx}
        onExportPdf={onExportPdf}
      />

      {/* Save button */}
      <Button
        variant="outline"
        size="sm"
        className={`${isDirty ? 'bg-green-200' : ''}`}
        title={isSaving ? 'Saving' : 'Save Changes'}
        onClick={() => { onSave(); }}
        disabled={isSaving}
      >
        {isSaving ? (
          <>
            <Loader2 className="animate-spin" />
            <span className="hidden lg:inline">Saving...</span>
          </>
        ) : (
          <>
            <Save />
            <span className="hidden lg:inline">Save</span>
          </>
        )}
      </Button>
    </ButtonGroup>
  );
}
```

Note: the order inside `ButtonGroup` is now **Copy → Export → Save** (matches the spec: "between Copy and Save"). The old `Find` button comment block is removed for clarity.

- [ ] **Step 2: Wire `useExportOperations` in `page-content.tsx`**

Open `frontend/src/app/meeting-details/page-content.tsx`. Add the import near the other hook imports (around line 19):

```typescript
import { useExportOperations } from '@/hooks/meeting-details/useExportOperations';
```

Then, inside the `PageContent` function body, near where `useCopyOperations` is called, add:

```typescript
const {
  exportingFormat,
  handleExportMarkdown,
  handleExportDocx,
  handleExportPdf,
} = useExportOperations({
  meeting,
  meetingTitle,
  aiSummary,
  blockNoteSummaryRef,
  // modelName: pass the current model name if available in context
});
```

Finally, wherever `SummaryUpdaterButtonGroup` is rendered (search for `<SummaryUpdaterButtonGroup`), pass the new props:

```tsx
<SummaryUpdaterButtonGroup
  // ...existing props...
  isGeneratingSummary={isGenerating /* from useSummaryGeneration hook */}
  exportingFormat={exportingFormat}
  onExportMarkdown={handleExportMarkdown}
  onExportDocx={handleExportDocx}
  onExportPdf={handleExportPdf}
/>
```

To get `isGenerating`: check whether `useSummaryGeneration` (already used in the page) exposes a boolean indicating generation is in progress. If it does, pass it through. If it doesn't, grep for the in-progress indicator used by `SummaryGeneratorButtonGroup` (it shows a loading spinner during generation) and reuse the same flag. If no flag exists, add a `isGenerating` boolean to `useSummaryGeneration`'s return value — it's a single state variable.

If the variables `aiSummary`, `meetingTitle`, or `blockNoteSummaryRef` have different names in the current file, adapt accordingly — the `useCopyOperations` call in the same file already uses the same values, so mirror that call.

- [ ] **Step 3: Remove the dead `handleExport` in `AISummary/index.tsx`**

Open `frontend/src/components/AISummary/index.tsx`. Delete lines 595–606 (the `handleExport` function) and any references/imports that become unused after its removal.

Before deleting, verify nothing references `handleExport`:

```bash
cd frontend && grep -rn "handleExport" src
```

Expected: only `index.tsx` (the definition). If any other call site appears, update it to use the new `ExportDropdown` via the hook. If not, delete the function.

- [ ] **Step 4: Verify build**

```bash
cd frontend && pnpm run build
```

Expected: Next.js build succeeds. Fix any TypeScript or missing prop errors before proceeding.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/MeetingDetails/SummaryUpdaterButtonGroup.tsx frontend/src/app/meeting-details/page-content.tsx frontend/src/components/AISummary/index.tsx
git commit -m "feat(export): integrate ExportDropdown in toolbar between Copy and Save"
```

---

## Task 14: Manual end-to-end verification

**Files:** none (runtime verification)

This task has no tests — it's the final manual check. Every sub-step must be verified in the live app.

- [ ] **Step 1: Launch the Tauri dev app**

```bash
cd frontend && pnpm run tauri:dev
```

Wait for the window to open. Log in / open an existing meeting with a generated summary. If no summary exists on any meeting, generate one first.

- [ ] **Step 2: Verify the Export button appears and is in the right place**

- Open a meeting with a summary.
- In the summary toolbar you should now see, left to right: `Generate Summary` · `AI Model` · `Template` · **`Copy`** · **`Export ▾`** · **`Save`** (the exact ordering depends on the `SummaryGeneratorButtonGroup` layout; what matters is Export is between Copy and Save).
- The Export button shows a download icon and the label "Export" on wide screens.

- [ ] **Step 3: Verify the dropdown opens with 3 options**

- Click Export → a dropdown appears with:
  - Markdown (.md)
  - Word (.docx)
  - PDF (.pdf)

- [ ] **Step 4: Export Markdown and verify the file**

- Click "Markdown (.md)".
- A toast appears: "Rapport exporté : <filename>.md" with "Ouvrir le dossier" action.
- Open the Downloads folder → the `.md` file exists.
- Open it in a text editor → verify:
  - YAML frontmatter contains `title`, `date` (in French: "19 avril 2026"), `duration`, `model`.
  - Body contains the report with headings and lists intact.

- [ ] **Step 5: Export Word and verify the file**

- Click "Word (.docx)".
- Toast appears with the `.docx` filename.
- Open the file in Microsoft Word or LibreOffice:
  - Title appears bold/large at the top.
  - Metadata line (italic) below the title.
  - Headings render as Word Heading 1 / 2 / 3 styles.
  - Bullet and numbered lists render natively.
  - At least one table (if the summary contains one) renders with borders.
  - Page number at the bottom of each page.

- [ ] **Step 6: Export PDF and verify the file**

- Click "PDF (.pdf)".
- Toast appears with the `.pdf` filename.
- Open the file in the OS default PDF viewer:
  - Title and metadata in the top-left of page 1 (no dedicated cover page).
  - Headings have hierarchical sizes.
  - Tables render with header row highlighted.
  - Text is selectable (not a raster).
  - Page numbers visible at the bottom center.

- [ ] **Step 7: Test the "Ouvrir le dossier" toast action**

- Click "Ouvrir le dossier" in the success toast.
- Expected: the OS file explorer opens in the Downloads folder with the exported file visible/selected.

- [ ] **Step 8: Test collision handling**

- Export PDF twice in a row without moving the first file.
- Expected: the second export creates `<basename>_1.pdf` (not overwriting).

- [ ] **Step 9: Test the disabled state**

- Open a meeting with no summary generated yet.
- Expected: Export button is visually disabled, tooltip says "Générer un résumé d'abord", clicking does nothing.

- [ ] **Step 10: Test slugification on a tricky meeting name**

- Rename a meeting to `Réunion: "Équipe" / Q2 | Standup?`.
- Export as PDF.
- Expected filename: `Reunion_Equipe_Q2_Standup_YYYY-MM-DD.pdf` (accents stripped, illegal chars removed, spaces → underscores).

- [ ] **Step 11: Stop the dev app**

Close the Tauri window or `Ctrl+C` in the terminal.

- [ ] **Step 12: Commit a changelog entry if any changes were needed during manual testing**

If any fix was required during manual verification:

```bash
git add <files>
git commit -m "fix(export): <describe fix>"
```

Otherwise, no commit needed for this task.

---

## Definition of Done

- All 14 tasks checked off.
- `pnpm test` passes (unit tests for metadata, blocknote-to-blocks, markdown, docx, pdf).
- `pnpm run build` succeeds.
- Manual verification (Task 14) confirms all 3 formats export correctly on Windows.
- No dead `handleExport` remains in `AISummary/index.tsx`.
- Git history shows one commit per task.
