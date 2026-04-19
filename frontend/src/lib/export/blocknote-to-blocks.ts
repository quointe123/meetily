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

function toInlines(content: unknown): InlineText[] {
  if (!Array.isArray(content)) return [];
  return (content as BNInlineContent[])
    .filter((c) => c && (c.type === 'text' || c.type === 'link'))
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
  const tableContent = block.content as { rows?: Array<{ cells?: unknown[] }> } | undefined;
  if (!tableContent || !Array.isArray(tableContent.rows)) {
    return { type: 'paragraph', inlines: [] };
  }
  const rows: TableRow[] = tableContent.rows.map((row, rowIdx) => {
    const rawCells = Array.isArray(row?.cells) ? row.cells : [];
    return {
      isHeader: rowIdx === 0,
      cells: rawCells.map((cell: any) => {
        // BlockNote 0.36+: a cell may be an array of inlines OR an object
        // like { type: 'tableCell', content: [...] }. Newer releases also
        // wrap each inline in a { text, styles } structure at top level.
        if (Array.isArray(cell)) return toInlines(cell);
        if (cell && Array.isArray(cell.content)) return toInlines(cell.content);
        return [];
      }),
    };
  });
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
