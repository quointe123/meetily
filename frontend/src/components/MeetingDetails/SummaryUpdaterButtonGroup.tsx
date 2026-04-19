"use client";

import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, Save, Loader2 } from 'lucide-react';
import { ExportDropdown } from '@/components/MeetingDetails/ExportDropdown';
import type { ExportFormat } from '@/lib/export/types';

interface SummaryUpdaterButtonGroupProps {
  isSaving: boolean;
  isDirty: boolean;
  onSave: () => Promise<void>;
  onCopy: () => Promise<void>;
  onFind?: () => void;
  onOpenFolder: () => Promise<void>;
  hasSummary: boolean;
  // --- new export props ---
  exportingFormat: ExportFormat | null;
  onExportMarkdown: () => void;
  onExportDocx: () => void;
  onExportPdf: () => void;
}

export function SummaryUpdaterButtonGroup({
  isSaving,
  isDirty,
  onSave,
  onCopy,
  onFind,
  onOpenFolder,
  hasSummary,
  exportingFormat,
  onExportMarkdown,
  onExportDocx,
  onExportPdf,
}: SummaryUpdaterButtonGroupProps) {
  return (
    <ButtonGroup>
      {/* Copy button */}
      <Button
        variant="outline"
        size="sm"
        title="Copy Summary"
        onClick={() => { onCopy(); }}
        disabled={!hasSummary}
        className="cursor-pointer"
      >
        <Copy />
        <span className="hidden lg:inline">Copy</span>
      </Button>

      {/* Export dropdown — between Copy and Save per spec */}
      <ExportDropdown
        hasSummary={hasSummary}
        exportingFormat={exportingFormat}
        onExportMarkdown={onExportMarkdown}
        onExportDocx={onExportDocx}
        onExportPdf={onExportPdf}
      />

      {/* Save button */}
      <Button
        variant="outline"
        size="sm"
        className={`${isDirty ? 'bg-green-200' : ''}`}
        title={isSaving ? 'Saving' : 'Save Changes'}
        onClick={() => { onSave(); }}
        disabled={isSaving}
      >
        {isSaving ? (
          <>
            <Loader2 className="animate-spin" />
            <span className="hidden lg:inline">Saving...</span>
          </>
        ) : (
          <>
            <Save />
            <span className="hidden lg:inline">Save</span>
          </>
        )}
      </Button>
    </ButtonGroup>
  );
}
