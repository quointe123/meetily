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
