import { useCallback, useState, RefObject } from 'react';
import { toast } from 'sonner';
import type { Summary } from '@/types';
import type { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { buildExportPayload, saveToDownloads, saveViaDialog, openContainingFolder } from '@/lib/export';
import type { ExportFormat } from '@/lib/export/types';

interface UseExportOperationsProps {
  meeting: any;
  meetingTitle: string;
  aiSummary: Summary | null;
  blockNoteSummaryRef: RefObject<BlockNoteSummaryViewRef>;
  modelName?: string;
}

function looksLikeWriteError(err: any): boolean {
  const msg = String(err?.message ?? err ?? '').toLowerCase();
  return (
    msg.includes('permission') ||
    msg.includes('denied') ||
    msg.includes('access') ||
    msg.includes('not allowed') ||
    msg.includes('forbidden') ||
    msg.includes('sharing violation') ||
    msg.includes('in use') ||
    msg.includes('os error 5') ||
    msg.includes('os error 13') ||
    msg.includes('os error 32')
  );
}

export function useExportOperations({
  meeting,
  meetingTitle,
  aiSummary,
  blockNoteSummaryRef,
  modelName,
}: UseExportOperationsProps) {
  const [exportingFormat, setExportingFormat] = useState<ExportFormat | null>(null);

  const runExport = useCallback(
    async (format: ExportFormat) => {
      if (!aiSummary) {
        toast.error('Aucun résumé à exporter');
        return;
      }
      setExportingFormat(format);
      try {
        // Prefer the live BlockNote editor content, so in-editor edits are captured.
        let summaryShape: any = aiSummary;
        if (blockNoteSummaryRef.current?.getDocument) {
          try {
            const liveDoc = blockNoteSummaryRef.current.getDocument();
            if (liveDoc) {
              summaryShape = { ...aiSummary, summary_json: liveDoc };
            }
          } catch {
            // fall back to stored aiSummary
          }
        }

        const payload = await buildExportPayload({
          format,
          meeting: { ...meeting, title: meetingTitle ?? meeting.title },
          summary: summaryShape,
          modelName,
        });

        let result;
        try {
          result = await saveToDownloads(payload.filename, payload.content);
        } catch (saveErr: any) {
          if (!looksLikeWriteError(saveErr)) throw saveErr;
          // Fallback: offer the native Save As dialog.
          const dialogResult = await saveViaDialog(payload.filename, payload.content);
          if (!dialogResult) {
            toast.info('Export annulé');
            return;
          }
          result = dialogResult;
        }

        toast.success(`Rapport exporté : ${result.filename}`, {
          action: {
            label: 'Ouvrir le dossier',
            onClick: () => {
              openContainingFolder(result.fullPath).catch(() =>
                toast.error("Impossible d'ouvrir le dossier"),
              );
            },
          },
        });
      } catch (err: any) {
        const message = err?.message ?? String(err);
        toast.error(`Échec de l'export : ${message}`);
        console.error('[export] failed:', err);
      } finally {
        setExportingFormat(null);
      }
    },
    [aiSummary, blockNoteSummaryRef, meeting, meetingTitle, modelName],
  );

  return {
    exportingFormat,
    handleExportMarkdown: useCallback(() => runExport('markdown'), [runExport]),
    handleExportDocx: useCallback(() => runExport('docx'), [runExport]),
    handleExportPdf: useCallback(() => runExport('pdf'), [runExport]),
  };
}
