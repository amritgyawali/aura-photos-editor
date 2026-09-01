import { useCallback, useEffect, useMemo, useState } from 'react';

import { asIpcError, qc } from '../../ipc/client';
import type {
  QcGroupDto,
  QcReplacementDto,
  QcReportDto,
  QcRoundDto,
  QcStatusDto,
} from '../../ipc/types';
import { BeforeAfter } from './BeforeAfter';
import { CategoryFilter } from './CategoryFilter';
import { QcReport } from './QcReport';
import { TicketQueue } from './TicketQueue';

/**
 * PHASE-27. The container that wires the four QC views to the nine QC commands.
 *
 * The three views are pure - rows and callbacks in, nothing fetched - which is what makes them
 * testable without a Tauri window. This is the one piece that talks to the shell, and it exists so
 * `App.tsx` can mount the feature with a project id and nothing else. Phase 25's `GalleryPanel`
 * established the split and phase 26's `CameraMatchPanel` followed it.
 *
 * **The rounds are fetched per finding, not per project.** A wedding with four hundred findings
 * has at most eight hundred rounds, and loading all of them to draw a queue would pull the whole
 * remediation history over the wire to render a list of sentences.
 *
 * **Everything reloads after a write.** Authorising a remedy re-inspects the frame, which can
 * close the finding, open a different one, or revert - so the reads are re-run rather than patched
 * locally. A panel that patched its own state would tell a photographer a finding was fixed when
 * the re-inspection had put the change back, which is exactly the mistake this phase exists to
 * catch in other people's software.
 */
export type QcPanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
};

/** How many findings the queue asks for. A page, not a wedding. */
const QUEUE_PAGE = 120;

/** The app banner's shape, from whatever the wire raised. */
function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-ML-5136',
    message: ipc?.message ?? 'The quality check could not be read.',
  };
}

export function QcPanel({ projectId, onError }: QcPanelProps) {
  const [status, setStatus] = useState<QcStatusDto | null>(null);
  const [report, setReport] = useState<QcReportDto | null>(null);
  const [groups, setGroups] = useState<QcGroupDto[]>([]);
  const [category, setCategory] = useState<string | null>(null);
  const [selectedTicketId, setSelectedTicketId] = useState<string | null>(null);
  const [rounds, setRounds] = useState<QcRoundDto[]>([]);
  const [running, setRunning] = useState(false);

  const reload = useCallback(async () => {
    if (!projectId) {
      setStatus(null);
      setReport(null);
      setGroups([]);
      return;
    }
    try {
      const [nextStatus, nextReport, nextGroups] = await Promise.all([
        qc.qcStatus(projectId),
        qc.qcReport(projectId),
        qc.qcQueueGrouped(projectId, QUEUE_PAGE),
      ]);
      setStatus(nextStatus);
      setReport(nextReport);
      setGroups(nextGroups);
      onError(null);
    } catch (error) {
      onError(toBanner(error));
    }
  }, [onError, projectId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // The rounds behind whichever finding is open. Cleared first, so a slow read never shows one
  // finding's history under another finding's heading.
  useEffect(() => {
    if (!projectId || !selectedTicketId) {
      setRounds([]);
      return;
    }
    setRounds([]);
    qc.qcRounds(projectId, selectedTicketId)
      .then(setRounds)
      .catch((error: unknown) => onError(toBanner(error)));
  }, [onError, projectId, selectedTicketId]);

  const runPass = useCallback(
    async (remediate: boolean) => {
      if (!projectId) {
        return;
      }
      setRunning(true);
      try {
        const next = await qc.qcRun({ projectId, remediate });
        setReport(next);
        await reload();
      } catch (error) {
        onError(toBanner(error));
      } finally {
        setRunning(false);
      }
    },
    [onError, projectId, reload],
  );

  const decide = useCallback(
    async (ticketId: string, next: 'accepted' | 'dismissed', applyRemedy: boolean) => {
      if (!projectId) {
        return;
      }
      try {
        await qc.qcDecide(projectId, { ticketId, status: next, applyRemedy, note: null });
        await reload();
      } catch (error) {
        onError(toBanner(error));
      }
    },
    [onError, projectId, reload],
  );

  const decideBulk = useCallback(
    async (ticketIds: string[], next: 'accepted' | 'dismissed') => {
      if (!projectId) {
        return;
      }
      try {
        await qc.qcDecideBulk({ projectId, ticketIds, status: next, note: null });
        await reload();
      } catch (error) {
        onError(toBanner(error));
      }
    },
    [onError, projectId, reload],
  );

  const exportReport = useCallback(async () => {
    if (!projectId) {
      return;
    }
    try {
      const markdown = await qc.qcReportMarkdown(projectId);
      if (markdown) {
        await navigator.clipboard.writeText(markdown);
      }
    } catch (error) {
      onError(toBanner(error));
    }
  }, [onError, projectId]);

  // Filtering happens here rather than on the wire when a category is chosen, because the grouped
  // queue is already in memory and a second round trip would re-order nothing.
  const shown = useMemo(
    () => (category ? groups.filter((group) => group.category === category) : groups),
    [category, groups],
  );

  const replacement: QcReplacementDto | null = useMemo(() => {
    if (!report || !selectedTicketId) {
      return null;
    }
    return report.replacements.find((swap) => swap.ticketId === selectedTicketId) ?? null;
  }, [report, selectedTicketId]);

  return (
    <div className="qc-panel">
      <QcReport
        status={status}
        report={report}
        running={running}
        onInspect={() => void runPass(false)}
        onRemediate={() => void runPass(true)}
        onExport={() => void exportReport()}
      />
      <CategoryFilter status={status} selected={category} onSelect={setCategory} />
      <TicketQueue
        groups={shown}
        selectedTicketId={selectedTicketId}
        onSelect={setSelectedTicketId}
        onDecide={(ticketId, next, applyRemedy) => void decide(ticketId, next, applyRemedy)}
        onDecideBulk={(ticketIds, next) => void decideBulk(ticketIds, next)}
      />
      {selectedTicketId ? <BeforeAfter rounds={rounds} replacement={replacement} /> : null}
    </div>
  );
}
