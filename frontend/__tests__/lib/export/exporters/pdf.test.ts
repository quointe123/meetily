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
