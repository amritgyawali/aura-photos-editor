import { useCallback, useEffect, useRef, useState } from 'react';

import { asIpcError, autopilot as api } from '../../ipc/client';
import type {
  AutopilotEventDto,
  AutopilotPreflightDto,
  AutopilotProgressDto,
  AutopilotStageDto,
  AutopilotStatusDto,
  AutopilotSummaryDto,
} from '../../ipc/types';
import { Autopilot } from './Autopilot';
import { prepareCollection, type PreparedPhoto } from './prepareCollection';
import type { ReferenceSelection } from '../look/referenceStyle';

/**
 * PHASE-28. The container that wires the five autopilot views to the nine autopilot commands.
 *
 * The five views are pure - rows and callbacks in, nothing fetched - which is what makes them
 * testable without a Tauri window. This is the one piece that talks to the shell, and it exists so
 * `App.tsx` can mount the feature with a project id and nothing else. Phase 25's `GalleryPanel`
 * established the split, and phases 26 and 27 followed it.
 *
 * ## Why this one polls when no other panel does
 *
 * Every other long-running command in this product streams progress over the Tauri event bus. This
 * one polls, because `RunWatch` lives in `aura-jobs`, which has no event bus and must not acquire
 * one - it is the crate that has to be drivable from a plain test. ADR-0058 section 4.
 *
 * The poll runs only while a run is in flight and stops the moment one is not, so an idle panel
 * costs nothing. `POLL_MS` is half a second: a progress bar that moved once a second would look
 * broken on a stage doing forty frames a second, and one that moved ten times a second would be
 * ten times the catalog reads for a bar nobody is watching that closely.
 *
 * ## Why every read re-runs after the run ends
 *
 * A finished run changes the status, the stages, the summary and the governor's events at once,
 * and three of those four are only written at the end. A panel that patched its own state would
 * show a photographer the run it *predicted* rather than the one that happened - which, on a phase
 * whose whole subject is what did and did not happen while nobody was looking, is the one mistake
 * worth engineering against.
 */
export type AutopilotPanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
  onBusyChange?: (busy: boolean) => void;
  automaticRequest?: number;
  onAutomaticConsumed?: () => void;
  onRender?: () => void;
  reference?: ReferenceSelection | null;
};

/** How often the panel asks what the run is doing, while one is running. */
const POLL_MS = 500;

/** The app banner's shape, from whatever the wire raised. */
function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-JOB-7005',
    message: ipc?.message ?? 'The autopilot could not be read.',
  };
}

export function AutopilotPanel({ projectId, onError, onBusyChange, automaticRequest = 0, onAutomaticConsumed, onRender, reference }: AutopilotPanelProps) {
  const [status, setStatus] = useState<AutopilotStatusDto | null>(null);
  const [stages, setStages] = useState<AutopilotStageDto[]>([]);
  const [summary, setSummary] = useState<AutopilotSummaryDto | null>(null);
  const [events, setEvents] = useState<AutopilotEventDto[]>([]);
  const [progress, setProgress] = useState<AutopilotProgressDto | null>(null);
  const [preflight, setPreflight] = useState<AutopilotPreflightDto | null>(null);
  const [disabled, setDisabled] = useState<string[]>([]);
  const [zeroTouch, setZeroTouch] = useState(true);
  const [starting, setStarting] = useState(false);
  const startingRef = useRef(false);
  const stopPreparation = useRef(false);
  const consumedRequest = useRef(0);
  const [preparation, setPreparation] = useState<string | null>(null);
  const [prepared, setPrepared] = useState<PreparedPhoto[]>([]);
  useEffect(() => { onBusyChange?.(starting || progress !== null); }, [starting, progress, onBusyChange]);

  // What the poll is watching, without making the effect depend on the value it sets.
  const running = useRef(false);
  running.current = progress !== null;

  const reload = useCallback(async () => {
    if (!projectId) {
      setStatus(null);
      setStages([]);
      setSummary(null);
      setEvents([]);
      setProgress(null);
      return;
    }
    try {
      const [nextStatus, nextStages, nextSummary, nextEvents, nextProgress] = await Promise.all([
        api.autopilotStatus(projectId),
        api.autopilotStages(projectId),
        api.autopilotSummary(projectId),
        api.autopilotEvents(projectId),
        api.autopilotProgress(projectId),
      ]);
      setStatus(nextStatus);
      setStages(nextStages);
      setSummary(nextSummary);
      setEvents(nextEvents);
      setProgress(nextProgress);
      setZeroTouch(nextStatus.zeroTouch);
      onError(null);
    } catch (error) {
      onError(toBanner(error));
    }
  }, [onError, projectId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // The progress poll. It reads one command rather than five, because four of the five do not
  // change while a stage is working - and when the run ends it hands over to a full reload, which
  // is where the summary and the governor's events come from.
  useEffect(() => {
    if (!projectId || progress === null) {
      return undefined;
    }
    const timer = window.setInterval(() => {
      api
        .autopilotProgress(projectId)
        .then((next) => {
          setProgress(next);
          if (next === null) {
            void reload();
          }
        })
        .catch((error: unknown) => onError(toBanner(error)));
    }, POLL_MS);
    return () => window.clearInterval(timer);
  }, [onError, progress, projectId, reload]);

  const openPreflight = useCallback(async (includeAdvanced = true) => {
    if (!projectId || startingRef.current || running.current) {
      return;
    }
    startingRef.current = true;
    setStarting(true);
    stopPreparation.current = false;
    setPrepared([]);
    try {
      const results = await prepareCollection(projectId, () => stopPreparation.current, setPreparation,
        photo => setPrepared(current => [...current, photo]), reference);
      setPrepared(results);
      if (stopPreparation.current) { setPreparation('Stopped. Completed edits are saved.'); return; }
      const ready = results.filter(photo => photo.outcome === 'ready').length;
      setPreparation(`${ready} of ${results.length} photos prepared with local enhancement.`);
      if (ready === 0 || !includeAdvanced) return;
      const report = await api.autopilotPreflight(projectId);
      if (!report.permitsStart) { setPreflight(report); return; }
      setPreflight(null);
      setProgress(await api.autopilotStart({ projectId, disabled, zeroTouch, allowOnBattery: false, quietMode: true }));
    } catch (error) {
      onError(toBanner(error));
    } finally { startingRef.current = false; setStarting(false); }
  }, [disabled, onError, projectId, zeroTouch, reference]);

  useEffect(() => {
    if (automaticRequest > 0 && consumedRequest.current !== automaticRequest && projectId) {
      consumedRequest.current = automaticRequest;
      onAutomaticConsumed?.();
      void openPreflight(false);
    }
  }, [automaticRequest, onAutomaticConsumed, openPreflight, projectId]);

  const start = useCallback(async () => {
    if (!projectId) {
      return;
    }
    try {
      const next = await api.autopilotStart({
        projectId,
        disabled,
        zeroTouch,
        allowOnBattery: false,
        quietMode: true,
      });
      setPreflight(null);
      setProgress(next);
    } catch (error) {
      onError(toBanner(error));
    }
  }, [disabled, onError, projectId, zeroTouch]);

  const cancel = useCallback(async () => {
    if (!projectId) {
      return;
    }
    try {
      await api.autopilotCancel(projectId);
      // Not a reload: the run is still finishing the photograph it is on, and the panel says so
      // until the poll sees it stop. Clearing the progress here would show a stopped run that was
      // still writing.
      setProgress((current) => (current ? { ...current, cancelled: true } : current));
    } catch (error) {
      onError(toBanner(error));
    }
  }, [onError, projectId]);

  // The checklist is recorded as it is changed rather than on start, so a photographer who unticks
  // two steps and closes the window finds them unticked tomorrow.
  const persist = useCallback(
    (nextDisabled: string[], nextZeroTouch: boolean) => {
      if (!projectId) {
        return;
      }
      api
        .autopilotSetSettings({
          projectId,
          disabled: nextDisabled,
          zeroTouch: nextZeroTouch,
          allowOnBattery: false,
          quietMode: true,
        })
        .catch((error: unknown) => onError(toBanner(error)));
    },
    [onError, projectId],
  );

  const toggleStage = useCallback(
    (stage: string, enabled: boolean) => {
      setDisabled((current) => {
        const next = enabled ? current.filter((s) => s !== stage) : [...current, stage];
        persist(next, zeroTouch);
        return next;
      });
    },
    [persist, zeroTouch],
  );

  const toggleZeroTouch = useCallback(
    (on: boolean) => {
      setZeroTouch(on);
      persist(disabled, on);
    },
    [disabled, persist],
  );

  if (!projectId) {
    return null;
  }

  return (
    <div aria-busy={starting}>
    <section className="quick-edit" aria-label="Automatic photo editing">
      <div><span className="eyebrow">MADE FOR YOUR PHOTO</span><h2>Good light. A natural finish.</h2>
        <p>{reference ? `Your photos will be matched to ${reference.analysis.origin} at ${Math.round(reference.strength * 100)}% strength, with individual tone and color adjustments.` : 'Each photo gets its own exposure, shadow and highlight adjustments. Works locally, with no model downloads or account.'}</p></div>
      <button type="button" className="is-primary" disabled={starting || progress !== null} onClick={() => void openPreflight(false)}>{starting ? 'Editing your photos…' : 'Auto edit all photos'}</button>
    </section>
    {preparation && <section className="automatic-result" aria-label="Automatic preparation">
      <p role="status">{preparation}</p>
      {starting && <button type="button" onClick={() => { stopPreparation.current = true; }}>Stop automatic editing</button>}
      {!starting && !progress && prepared.some(photo => photo.outcome === 'ready') && <>
        <h2>{prepared.filter(photo => photo.outcome === 'ready').length} photos have saved edits, ready to render.</h2>
        <p>{prepared.filter(photo => photo.outcome === 'failed').length} photos need attention. Review the steps below whenever you want to see what changed.</p>
        <button type="button" className="is-primary" onClick={onRender}>Render final output →</button>
      </>}
      <details><summary>Review automatic preparation ({prepared.length} photos)</summary>
        <ol className="preparation-log">{prepared.map(photo => <li key={photo.photoId}><strong>{photo.name} · {photo.outcome === 'ready' ? 'Prepared' : 'Needs attention'}</strong><p>{photo.detail}</p></li>)}</ol>
      </details>
    </section>}
    <details className="advanced-tools" open={progress !== null || preflight !== null ? true : undefined}>
    <summary>Advanced wedding workflow &amp; analysis</summary>
    <fieldset className="autopilot-lock" disabled={starting}>
    <Autopilot
      status={status}
      stages={stages}
      progress={progress}
      summary={summary}
      events={events}
      preflight={preflight}
      disabled={disabled}
      zeroTouch={zeroTouch}
      onPreflight={() => void openPreflight()}
      onClosePreflight={() => setPreflight(null)}
      onStart={() => void start()}
      onCancel={() => void cancel()}
      onToggleStage={toggleStage}
      onZeroTouch={toggleZeroTouch}
    />
    </fieldset></details>
    </div>
  );
}
