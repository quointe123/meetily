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
