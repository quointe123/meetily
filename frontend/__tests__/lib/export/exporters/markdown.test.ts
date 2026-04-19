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
