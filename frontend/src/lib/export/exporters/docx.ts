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
  BorderStyle,
  WidthType,
  ShadingType,
} from 'docx';
import type { DocumentAST, BlockNode, InlineText, TableRow as AstTableRow } from '../types';
import { formatFrenchDate, formatDuration } from '../metadata';

const BODY_FONT = 'Calibri';
const CODE_FONT = 'Consolas';
const COLOR_TEXT = '1f2937';
const COLOR_MUTED = '6b7280';
const COLOR_ACCENT = '111827';
const COLOR_BORDER = 'e5e7eb';
const COLOR_HEADER_FILL = 'f3f4f6';

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
    const isCode = i.marks?.includes('code');
    const run = new TextRun({
      text: i.text,
      bold: i.marks?.includes('bold'),
      italics: i.marks?.includes('italic'),
      font: isCode ? CODE_FONT : BODY_FONT,
      color: isCode ? '374151' : COLOR_TEXT,
      shading: isCode
        ? { type: ShadingType.CLEAR, fill: 'f3f4f6', color: 'auto' }
        : undefined,
    });
    if (i.link) {
      return new ExternalHyperlink({
        link: i.link,
        children: [
          new TextRun({
            text: i.text,
            bold: i.marks?.includes('bold'),
            italics: i.marks?.includes('italic'),
            font: isCode ? CODE_FONT : BODY_FONT,
            color: '2563eb',
            underline: {},
          }),
        ],
      });
    }
    return run;
  });
}

function renderCell(cell: InlineText[], isHeader: boolean): TableCell {
  return new TableCell({
    shading: isHeader
      ? { type: ShadingType.CLEAR, fill: COLOR_HEADER_FILL, color: 'auto' }
      : undefined,
    margins: { top: 80, bottom: 80, left: 120, right: 120 },
    children: [
      makeParagraph({
        children: renderInlineRuns(
          cell.map((i) =>
            isHeader ? { ...i, marks: [...(i.marks ?? []), 'bold'] } : i,
          ),
        ),
      }),
    ],
  });
}

function renderTable(rows: AstTableRow[]): Table {
  const cellBorder = { style: BorderStyle.SINGLE, size: 4, color: COLOR_BORDER };
  return new Table({
    width: { size: 100, type: WidthType.PERCENTAGE },
    borders: {
      top: cellBorder,
      bottom: cellBorder,
      left: cellBorder,
      right: cellBorder,
      insideHorizontal: cellBorder,
      insideVertical: cellBorder,
    },
    rows: rows.map(
      (row) =>
        new DocxTableRow({
          tableHeader: row.isHeader,
          children: row.cells.map((c) => renderCell(c, !!row.isHeader)),
        }),
    ),
  });
}

function renderBlock(block: BlockNode): (Paragraph | Table)[] {
  switch (block.type) {
    case 'heading':
      return [
        makeParagraph({
          heading: headingLevelFor(block.level),
          spacing: { before: 240, after: 120 },
          children: renderInlineRuns(block.inlines),
        }),
      ];
    case 'paragraph':
      return [
        makeParagraph({
          spacing: { after: 120, line: 300 },
          children: renderInlineRuns(block.inlines),
        }),
      ];
    case 'bulletList':
      return block.items.map((item) =>
        makeParagraph({
          bullet: { level: 0 },
          spacing: { after: 80, line: 300 },
          children: renderInlineRuns(item.inlines),
        }),
      );
    case 'numberedList':
      return block.items.map((item) =>
        makeParagraph({
          numbering: { reference: 'ordered', level: 0 },
          spacing: { after: 80, line: 300 },
          children: renderInlineRuns(item.inlines),
        }),
      );
    case 'codeBlock':
      return [
        makeParagraph({
          spacing: { before: 120, after: 120 },
          shading: { type: ShadingType.CLEAR, fill: 'f9fafb', color: 'auto' },
          children: [
            new TextRun({ text: block.code, font: CODE_FONT, size: 20, color: '374151' }),
          ],
        }),
      ];
    case 'table':
      return [renderTable(block.rows)];
    case 'divider':
      return [
        makeParagraph({
          spacing: { before: 120, after: 120 },
          border: {
            bottom: { style: BorderStyle.SINGLE, size: 4, color: COLOR_BORDER, space: 1 },
          },
          children: [new TextRun('')],
        }),
      ];
  }
}

function buildMetadataHeaderParagraphs(meta: DocumentAST['metadata']): Paragraph[] {
  const lines: Paragraph[] = [];

  lines.push(
    makeParagraph({
      heading: HeadingLevel.HEADING_1,
      spacing: { after: 60 },
      children: [
        new TextRun({
          text: meta.title,
          bold: true,
          font: BODY_FONT,
          color: COLOR_ACCENT,
          size: 44,
        }),
      ],
    }),
  );

  const metaBits: string[] = [formatFrenchDate(meta.dateIso)];
  const duration = formatDuration(meta.durationSeconds);
  if (duration) metaBits.push(duration);
  if (meta.modelName) metaBits.push(`Généré avec ${meta.modelName}`);
  lines.push(
    makeParagraph({
      alignment: AlignmentType.LEFT,
      spacing: { after: 360 },
      border: {
        bottom: { style: BorderStyle.SINGLE, size: 4, color: COLOR_BORDER, space: 8 },
      },
      children: [
        new TextRun({
          text: metaBits.join(' · '),
          italics: true,
          font: BODY_FONT,
          color: COLOR_MUTED,
          size: 20,
        }),
      ],
    }),
  );

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
    styles: {
      default: {
        document: {
          run: { font: BODY_FONT, size: 22, color: COLOR_TEXT },
        },
      },
      paragraphStyles: [
        {
          id: 'Heading1',
          name: 'Heading 1',
          basedOn: 'Normal',
          next: 'Normal',
          run: { font: BODY_FONT, size: 36, bold: true, color: COLOR_ACCENT },
          paragraph: { spacing: { before: 360, after: 160 } },
        },
        {
          id: 'Heading2',
          name: 'Heading 2',
          basedOn: 'Normal',
          next: 'Normal',
          run: { font: BODY_FONT, size: 30, bold: true, color: COLOR_ACCENT },
          paragraph: { spacing: { before: 280, after: 140 } },
        },
        {
          id: 'Heading3',
          name: 'Heading 3',
          basedOn: 'Normal',
          next: 'Normal',
          run: { font: BODY_FONT, size: 26, bold: true, color: COLOR_ACCENT },
          paragraph: { spacing: { before: 220, after: 120 } },
        },
        {
          id: 'Heading4',
          name: 'Heading 4',
          basedOn: 'Normal',
          next: 'Normal',
          run: { font: BODY_FONT, size: 24, bold: true, color: COLOR_ACCENT },
          paragraph: { spacing: { before: 200, after: 100 } },
        },
      ],
    },
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
        properties: {
          page: {
            margin: { top: 1134, bottom: 1134, left: 1134, right: 1134 },
          },
        },
        children: allChildren,
        footers: {
          default: new Footer({
            children: [
              new Paragraph({
                alignment: AlignmentType.CENTER,
                children: [
                  new TextRun({
                    children: [PageNumber.CURRENT],
                    font: BODY_FONT,
                    size: 18,
                    color: COLOR_MUTED,
                  }),
                ],
              }),
            ],
          }),
        },
      },
    ],
  }) as Document & { __testChildren__?: any[] };

  doc.__testChildren__ = allChildren;

  return {
    document: doc,
    generate: async () => {
      const blob = await Packer.toBlob(doc);
      return { blob };
    },
  };
}
