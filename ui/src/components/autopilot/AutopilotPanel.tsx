import { useCallback, useEffect, useRef, useState } from 'react';

import { asIpcError, api as jobs, autopilot as api, explain } from '../../ipc/client';
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
import { BatchPhotoPicker } from './BatchPhotoPicker';
import type { ReferenceSelection } from '../look/referenceStyle';
import type { ProfileSelection } from '../../ipc/client';
import { FULL_EDIT_STAGES, OUTCOME_WORDS } from '../../ipc/fullEdit';
import { AutoChoiceNotice, useAutoChoice } from '../workflow/AutoChoice';
import { LiveEditPreview, finishPhoto, photoEvent, startPhoto, type LivePhoto } from './LiveEditPreview';

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
  /** The photos an automatic request edits - the best set confirmed on the Cull screen; all when absent. */
  automaticPhotos?: readonly string[] | null;
  onAutomaticConsumed?: () => void;
  onRender?: () => void;
  /** The panel is on screen: a finished edit goes on to rendering after a minute with nobody there (ADR-0189). */
  active?: boolean;
  reference?: ReferenceSelection | null;
  /** The edit profile chosen on the start screen, and its display name. */
  profile?: ProfileSelection | null;
  profileName?: string;
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

export function AutopilotPanel({ projectId, onError, onBusyChange, automaticRequest = 0, automaticPhotos = null, onAutomaticConsumed, onRender, active = true, reference, profile, profileName }: AutopilotPanelProps) {
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
  const preparationJob = useRef<string | null>(null);
  const preparationError = useRef(onError);
  preparationError.current = onError;
  const consumedRequest = useRef(0);
  const [preparation, setPreparation] = useState<string | null>(null);
  const [prepared, setPrepared] = useState<PreparedPhoto[]>([]);
  const [selected, setSelected] = useState<string[] | null>(null);
  /** The photographs edited while someone watched, with a picture after every step (ADR-0187). */
  const [livePhotos, setLive] = useState<LivePhoto[]>([]);
  const [liveFocus, setLiveFocus] = useState<string | null>(null);
  const watching = useRef(active);
  watching.current = active;
  /** The last edit run that finished without being stopped; it arms the minute before rendering. */
  const runs = useRef(0);
  const [finishedRun, setFinishedRun] = useState(0);
  /** No calibration yet: every deciding step will wait for review rather than act (ADR-0027). */
  const [uncalibrated, setUncalibrated] = useState(false);
  useEffect(() => {
    setUncalibrated(false);
    if (!projectId) return;
    let live = true;
    // A ledger that cannot be read leaves the notice off rather than raising a second banner.
    void Promise.resolve()
      .then(() => explain.ledgerStatus(projectId))
      .then(ledger => { if (live) setUncalibrated(ledger.calibrationVer === 0); })
      .catch(() => undefined);
    return () => { live = false; };
  }, [projectId]);
  useEffect(() => () => {
    stopPreparation.current = true;
    if (preparationJob.current) void jobs.cancelJob(preparationJob.current).catch(error => preparationError.current(toBanner(error)));
  }, []);
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

  const openPreflight = useCallback(async (includeAdvanced = true, photoIds?: readonly string[], retry = false) => {
    if (!projectId || startingRef.current || running.current) {
      return;
    }
    startingRef.current = true;
    setStarting(true);
    setPreparation('Preparing photos for automatic editing…');
    stopPreparation.current = false;
    preparationJob.current = `reference-prepare-${crypto.randomUUID()}`;
    if (!retry) setPrepared([]);
    setLive([]);
    setLiveFocus(null);
    const run = ++runs.current;
    setFinishedRun(0);
    onError(null);
    try {
      const results = await prepareCollection(projectId, () => stopPreparation.current, setPreparation,
        photo => setPrepared(current => [...current.filter(row => row.photoId !== photo.photoId), photo]), reference, profile, profileName, photoIds, preparationJob.current, {
          watching: () => watching.current,
          onStart: photo => setLive(current => startPhoto(current, photo)),
          onEvent: event => setLive(current => photoEvent(current, event)),
          onFinish: (photoId, outcome) => setLive(current => finishPhoto(current, photoId, outcome)),
        });
      if (!retry) setPrepared(results);
      if (stopPreparation.current) { setPreparation('Stopped. Completed edits are saved.'); return; }
      const ready = results.filter(photo => photo.outcome === 'ready').length;
      setFinishedRun(run);
      setPreparation(`${ready} of ${results.length} photos edited: all ten stages ran on each, one photo at a time.`);
      if (ready === 0 || !includeAdvanced) return;
      const report = await api.autopilotPreflight(projectId);
      if (!report.permitsStart) { setPreflight(report); return; }
      setPreflight(null);
      setProgress(await api.autopilotStart({ projectId, disabled, zeroTouch, allowOnBattery: false, quietMode: true }));
    } catch (error) {
      onError(toBanner(error));
    } finally { preparationJob.current = null; startingRef.current = false; setStarting(false); }
  }, [disabled, onError, projectId, zeroTouch, reference, profile, profileName]);

  useEffect(() => {
    if (automaticRequest > 0 && consumedRequest.current !== automaticRequest && projectId) {
      consumedRequest.current = automaticRequest;
      onAutomaticConsumed?.();
      void openPreflight(false, automaticPhotos ?? undefined);
    }
  }, [automaticRequest, automaticPhotos, onAutomaticConsumed, openPreflight, projectId]);

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

  // Edits saved and nobody choosing: a minute later the photos go on to rendering. Failed photos
  // are left for later, as the button would leave them.
  const readyCount = prepared.filter(photo => photo.outcome === 'ready').length;
  const autoRender = useAutoChoice(Boolean(active && onRender && finishedRun > 0 && !starting && progress === null && readyCount > 0),
    () => onRender?.(), finishedRun);

  if (!projectId) {
    return null;
  }

  return (
    <div aria-busy={starting}>
    <section className="quick-edit" aria-label="Automatic photo editing">
      <div><span className="eyebrow">MADE FOR YOUR PHOTO</span><h2>Good light. A natural finish.</h2>
        <p>{[profile ? `The ${profileName ?? profile.profileId} profile at ${Math.round(profile.strength * 100)}% adapts to each photo's own light.` : '',
          reference ? `Your photos will be matched to ${reference.analysis.origin} at ${Math.round(reference.strength * 100)}% strength, with individual tone and color adjustments.` : '',
          !profile && !reference ? 'Every photo gets the same ten stages, one photo at a time, each measured from that photo: basic edit, colour correction, calibration, colour grading, crop & straighten, blemishes, skin smoothing, face, body and the advanced finish. Works locally, with no model downloads or account.' : 'Then every photo gets the retouch stages: crop & straighten, calibration, blemishes, skin smoothing, face, body and the advanced finish.'].filter(Boolean).join(' ')}</p>
        <ol className="full-edit-plan" aria-label="The ten stages">{FULL_EDIT_STAGES.map(([key, title, group]) => <li key={key} data-group={group}>{title}</li>)}</ol></div>
      <button type="button" className="is-primary" disabled={starting || progress !== null || selected?.length === 0} onClick={() => void openPreflight(false, selected ?? undefined)}>{starting ? 'Editing your photos…' : selected === null ? 'Auto edit all photos' : `Auto edit ${selected.length} selected photos`}</button>
    </section>
    <BatchPhotoPicker key={projectId} projectId={projectId} disabled={starting || progress !== null} selected={selected} onSelect={setSelected} />
    {preparation && <section className="automatic-result" aria-label="Automatic preparation">
      <p role="status">{preparation}</p>
      {starting && <button type="button" onClick={() => {
        stopPreparation.current = true;
        if (preparationJob.current) void jobs.cancelJob(preparationJob.current).catch(error => onError(toBanner(error)));
      }}>Stop automatic editing</button>}
      {!starting && !progress && prepared.some(photo => photo.outcome === 'failed') && <button type="button" onClick={() => void openPreflight(false, prepared.filter(photo => photo.outcome === 'failed').map(photo => photo.photoId), true)}>Retry failed photos only</button>}
      {!starting && !progress && prepared.some(photo => photo.outcome === 'ready') && <>
        <h2>{prepared.filter(photo => photo.outcome === 'ready').length} photos have saved edits, ready to render.</h2>
        <p>{prepared.filter(photo => photo.outcome === 'failed').length} photos need attention. Review the steps below whenever you want to see what changed.</p>
        <button type="button" className="is-primary" onClick={onRender}>Render final output →</button>
        <AutoChoiceNotice choice={autoRender} action={`go on to render the ${readyCount} edited photos`} />
      </>}
      {livePhotos.length > 0 && <LiveEditPreview photos={livePhotos} focus={liveFocus} onFocus={setLiveFocus} />}
      <details open={prepared.length > 0 && prepared.length <= 3 ? true : undefined}><summary>Review every photo's ten stages ({prepared.length} photos)</summary>
        <ol className="preparation-log">{prepared.map(photo => <li key={photo.photoId}><strong>{photo.name} · {photo.outcome === 'ready' ? 'Edited' : 'Needs attention'}{photo.millis ? ` · ${(photo.millis / 1000).toFixed(1)} s` : ''}</strong>
          {livePhotos.some(row => row.photoId === photo.photoId && row.pictures.length > 0) && <button type="button" className="live-edit-watch" onClick={() => setLiveFocus(photo.photoId)}>See this photo's edit step by step</button>}<p>{photo.detail}</p>{photo.settings && <p>{photo.settings}</p>}
          {photo.stages && <ul className="full-edit-stages" aria-label={`Stages for ${photo.name}`}>{photo.stages.map(stage => <li key={stage.key} className={`is-${stage.outcome}`} title={[...stage.changes, ...stage.checks].join(' · ')}>
            <span>{stage.number}. {stage.title}</span><b>{OUTCOME_WORDS[stage.outcome]}</b>{(stage.changes[0] ?? stage.checks[0]) && <small>{stage.changes[0] ?? stage.checks[0]}</small>}</li>)}</ul>}</li>)}</ol>
      </details>
    </section>}
    <details className="advanced-tools" open={progress !== null || preflight !== null ? true : undefined}>
    <summary>Advanced wedding workflow &amp; analysis</summary>
    {uncalibrated && <p className="lr-notice" role="note">
      AURA is not yet calibrated on your weddings, so the steps that decide - culling, tone, colour, retouch, the album and the export - stop and wait for your review instead of acting on their own. A run prepares and measures everything; nothing is chosen or delivered until you approve it.
    </p>}
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
