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

  it('handles BlockNote 0.36+ table cells wrapped as { content } objects', () => {
    const input = [
      {
        type: 'table',
        content: {
          rows: [
            {
              cells: [
                { type: 'tableCell', content: [{ type: 'text', text: 'Task', styles: {} }] },
                { type: 'tableCell', content: [{ type: 'text', text: 'Owner', styles: {} }] },
              ],
            },
            {
              cells: [
                { type: 'tableCell', content: [{ type: 'text', text: 'Deploy', styles: {} }] },
                { type: 'tableCell', content: [{ type: 'text', text: 'Alice', styles: {} }] },
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

  it('does not throw on malformed table cells (defensive)', () => {
    const input = [
      {
        type: 'table',
        content: {
          rows: [
            { cells: [null, undefined, { type: 'tableCell' }, 'not-an-object'] as any },
          ],
        },
      },
    ];
    expect(() => blockNoteToAST(input)).not.toThrow();
    const result = blockNoteToAST(input);
    expect(result).toHaveLength(1);
    expect(result[0].type).toBe('table');
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
