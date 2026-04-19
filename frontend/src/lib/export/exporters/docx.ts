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

// docx v8 does not persist `options` on the Paragraph instance; we attach it
// manually so tests can inspect it via `(p as any).options?.heading`.
function makeParagraph(opts: ConstructorParameters<typeof Paragraph>[0]): Paragraph {
  const p = new Paragraph(opts as any);
  (p as any).options = opts;
  return p;
}

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
      return [makeParagraph({ heading: headingLevelFor(block.level), children: renderInlineRuns(block.inlines) })];
    case 'paragraph':
      return [makeParagraph({ children: renderInlineRuns(block.inlines) })];
    case 'bulletList':
      return block.items.map(
        (item) =>
          makeParagraph({
            bullet: { level: 0 },
            children: renderInlineRuns(item.inlines),
          }),
      );
    case 'numberedList':
      return block.items.map(
        (item) =>
          makeParagraph({
            numbering: { reference: 'ordered', level: 0 },
            children: renderInlineRuns(item.inlines),
          }),
      );
    case 'codeBlock':
      return [
        makeParagraph({
          children: [new TextRun({ text: block.code, font: 'Courier New' })],
        }),
      ];
    case 'table':
      return [renderTable(block.rows)];
    case 'divider':
      return [makeParagraph({ children: [new TextRun('—'.repeat(40))] })];
  }
}

function buildMetadataHeaderParagraphs(meta: DocumentAST['metadata']): Paragraph[] {
  const lines: Paragraph[] = [];

  lines.push(
    makeParagraph({
      heading: HeadingLevel.HEADING_1,
      children: [new TextRun({ text: meta.title, bold: true })],
    }),
  );

  const metaBits: string[] = [formatFrenchDate(meta.dateIso)];
  const duration = formatDuration(meta.durationSeconds);
  if (duration) metaBits.push(duration);
  if (meta.modelName) metaBits.push(`Généré avec ${meta.modelName}`);
  lines.push(
    makeParagraph({
      alignment: AlignmentType.LEFT,
      children: [new TextRun({ text: metaBits.join(' · '), italics: true, size: 20 })],
    }),
  );

  lines.push(makeParagraph({ children: [new TextRun('')] })); // spacer
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
