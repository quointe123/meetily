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
