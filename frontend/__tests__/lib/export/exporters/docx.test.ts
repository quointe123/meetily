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
