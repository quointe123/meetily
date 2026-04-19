'use client';

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
import { Download, FileText, FileType, FileDown, Loader2 } from 'lucide-react';
import type { ExportFormat } from '@/lib/export/types';

interface ExportDropdownProps {
  hasSummary: boolean;
  isGenerating?: boolean;
  exportingFormat: ExportFormat | null;
  onExportMarkdown: () => void;
  onExportDocx: () => void;
  onExportPdf: () => void;
}

export function ExportDropdown({
  hasSummary,
  isGenerating = false,
  exportingFormat,
  onExportMarkdown,
  onExportDocx,
  onExportPdf,
}: ExportDropdownProps) {
  const isBusy = exportingFormat !== null;
  const disabled = !hasSummary || isBusy || isGenerating;
  const titleText = !hasSummary
    ? 'Générer un résumé d\u2019abord'
    : isGenerating
    ? 'Résumé en cours de génération\u2026'
    : 'Export';

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          title={titleText}
          disabled={disabled}
          className="cursor-pointer"
        >
          {isBusy ? (
            <>
              <Loader2 className="animate-spin" />
              <span className="hidden lg:inline">Export&hellip;</span>
            </>
          ) : (
            <>
              <Download />
              <span className="hidden lg:inline">Export</span>
            </>
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={onExportMarkdown} disabled={isBusy}>
          <FileText className="mr-2 h-4 w-4" />
          Markdown (.md)
        </DropdownMenuItem>
        <DropdownMenuItem onClick={onExportDocx} disabled={isBusy}>
          <FileType className="mr-2 h-4 w-4" />
          Word (.docx)
        </DropdownMenuItem>
        <DropdownMenuItem onClick={onExportPdf} disabled={isBusy}>
          <FileDown className="mr-2 h-4 w-4" />
          PDF (.pdf)
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
