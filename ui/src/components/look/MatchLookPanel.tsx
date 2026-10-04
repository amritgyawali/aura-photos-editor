import { useCallback, useEffect, useRef, useState } from 'react';

import { api, asIpcError, inTauri, look as lookApi, pickPhotoFolder } from '../../ipc/client';
import { renderSelectedLook } from './applyLook';
import type {
  LookBucketDto,
  LookMatchDto,
  LookProfileDto,
  LookStatusDto,
  MeasureLookDto,
  ReferenceOriginDto,
} from '../../ipc/types';

export type MatchLookPanelProps = {
  projectId: string | null;
  onError?: (error: { code: string; message: string }) => void;
  onBusyChange?: (busy: boolean) => void;
};

/** The routes a photographer may choose, in the order the panel offers them. */
export const SOURCE_CHOICES = ['folder', 'instagram_export', 'public_url'] as const;

/** What the panel calls each route. */
export function sourceLabel(source: string): string {
  switch (source) {
    case 'folder':
      return 'A folder of photographs';
    case 'instagram_export':
      return 'An Instagram data export';
    case 'public_url':
      return 'Download them from the page';
    default:
      return source;
  }
}

/**
 * Whether this build can actually get photographs through a route.
 *
 * Mirrors `MediaSource::can_fetch`, and the panel reads the wire for the same fact rather than
 * trusting this: `LookStatusDto.networkTransportAvailable` is the authority, and this function
 * exists so the radio button is disabled before the status has loaded.
 */
export function routeWorks(source: string): boolean {
  return source !== 'public_url';
}

/**
 * What to say about the route this build does not have.
 *
 * Two facts, both true, neither of them "coming soon". A photographer who has just pasted a link
 * deserves to know why nothing happened and what to do instead, in the same breath.
 */
export const FETCH_UNAVAILABLE =
  'AURA cannot download photographs from a page. Save the ones you want to match into a ' +
  'folder - or ask Instagram for your data export - and point AURA at that instead. The link ' +
  'you pasted is still recorded, so the look will say which page it came from.';

/** The sentence under the address box, once the text has been read back. */
export function originSentence(origin: ReferenceOriginDto | null): string {
  if (!origin || origin.title.length === 0) {
    return '';
  }
  if (!origin.understood) {
    return origin.refusal ?? 'AURA did not understand that address.';
  }
  switch (origin.kind) {
    case 'instagram':
      // Never "verified" and never a tick. Nothing here has been resolved, and a tick beside an
      // unchecked claim is the failure mode ADR-0035 decision 8 named in a different phase.
      return `This look will be recorded as coming from ${origin.title}.`;
    case 'web':
      return `This look will be recorded as coming from ${origin.title}.`;
    default:
      return '';
  }
}

/** A percentage, the way a match figure should read. */
export function percent(fraction: number): string {
  return `${Math.round(Math.max(0, Math.min(1, fraction)) * 100)}%`;
}

/**
 * What a measured match says, in the product's own words.
 *
 * **It leads with how much of the gap closed, not with whether a threshold was met.** A match
 * that closed nine tenths of a large difference and landed just outside the ceiling is a match
 * that worked, and one that landed inside because there was nothing to close is not a result.
 */
export function matchSentence(matched: LookMatchDto | null): string {
  if (!matched) {
    return 'AURA has not measured this look against your photographs yet.';
  }
  if (matched.frames === 0) {
    return 'There were no analysed photographs to measure this look against.';
  }
  if (matched.measuredFrames === 0) {
    // The look still applies - the overall lean reaches every frame - but nothing in this
    // wedding was made in a light the reference also worked in, so there is no figure. Saying
    // "0.0 dE00" here would be the most flattering possible way to report having measured
    // nothing.
    return 'None of your photographs were made in light this reference also worked in, so AURA has no figure for how close they are. The look still applies.';
  }
  if (matched.beforeDe00 <= 0.0001) {
    return 'Your photographs already sat where that reference sits, so nothing needed to change.';
  }
  const closed = percent(matched.realisedShare);
  // Both denominators when they differ. The figure describes the frames it was computed over,
  // and claiming it describes the whole gallery is the one thing this sentence must not do.
  const over =
    matched.measuredFrames < matched.frames
      ? `measured over ${matched.measuredFrames} of the ${matched.frames} it applies to`
      : `measured over all ${matched.frames} of them`;
  const base = `Your photographs moved ${closed} of the way toward that reference, ${over}.`;
  if (matched.userEdited > 0) {
    return `${base} ${matched.userEdited} you had edited by hand were left exactly as you made them.`;
  }
  return base;
}

/**
 * What one row of the bucket matrix says.
 *
 * A look is conditioned on light and on nothing else, so this is the only place a photographer
 * can see that it treats candlelight differently from open shade. `afterDe00` is `null` where
 * nothing was measured in that light, and the row says so rather than showing a zero.
 */
export function bucketSentence(bucket: LookBucketDto): string {
  if (!bucket.applied) {
    return 'Too few reference photographs in this light to be sure, so the overall look is used.';
  }
  const moves: string[] = [];
  if (Math.abs(bucket.exposure) >= 0.02) {
    moves.push(`${bucket.exposure > 0 ? '+' : ''}${bucket.exposure.toFixed(2)} EV`);
  }
  if (Math.abs(bucket.temperatureK) >= 10) {
    moves.push(`${bucket.temperatureK > 0 ? 'warmer' : 'cooler'} by ${Math.abs(Math.round(bucket.temperatureK))} K`);
  }
  if (Math.abs(bucket.contrast) >= 1) {
    moves.push(`${bucket.contrast > 0 ? 'more' : 'less'} contrast`);
  }
  if (Math.abs(bucket.vibrance) >= 1 || Math.abs(bucket.saturation) >= 1) {
    const strength = bucket.vibrance + bucket.saturation;
    moves.push(`${strength > 0 ? 'stronger' : 'softer'} colour`);
  }
  if (moves.length === 0) {
    return 'Nothing to change in this light.';
  }
  return moves.join(', ');
}

/**
 * What the coverage line says.
 *
 * The denominator, said out loud. A look applied over 40 % of a wedding is a look over 40 % of a
 * wedding, and a bar with no numbers on it is not something anybody can plan against.
 */
export function coverageSentence(status: LookStatusDto | null): string {
  if (!status || status.photographs === 0) {
    return 'Import some photographs first.';
  }
  if (status.appliable === 0) {
    return `None of your ${status.photographs} photographs has been analysed yet, so there is nothing for a look to be measured against. Run Autopilot, or the tone and colour passes, first.`;
  }
  if (status.appliable < status.photographs) {
    return `A look can be applied to ${status.appliable} of your ${status.photographs} photographs. The rest have not been analysed yet.`;
  }
  return `A look can be applied to all ${status.photographs} of your photographs.`;
}

/**
 * The card on the first screen: paste a page, point at the photographs, match the look.
 *
 * ## What this panel will not do, and says so
 *
 * It will not fetch the page. The route is offered, disabled, with the reason beside it - rather
 * than omitted, which would leave a photographer wondering whether they had missed a setting.
 * `FETCH_UNAVAILABLE` is the sentence and `docs/match-a-look.md` is the long version.
 *
 * It will not apply a look nobody has measured. `measureLook` measures as it goes, the database
 * refuses a selection without one, and the panel does not offer the button until the number
 * exists.
 */
export function MatchLookPanel({ projectId, onError, onBusyChange }: MatchLookPanelProps): JSX.Element {
  const [status, setStatus] = useState<LookStatusDto | null>(null);
  const [looks, setLooks] = useState<LookProfileDto[]>([]);
  const [matched, setMatched] = useState<LookMatchDto | null>(null);

  const [address, setAddress] = useState('');
  const [origin, setOrigin] = useState<ReferenceOriginDto | null>(null);
  const [source, setSource] = useState<string>('folder');
  const [folder, setFolder] = useState('');
  const [name, setName] = useState('');

  const [buckets, setBuckets] = useState<LookBucketDto[]>([]);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [result, setResult] = useState<MeasureLookDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [strength, setStrengthDraft] = useState(100);
  const stopped = useRef(false);
  useEffect(() => { onBusyChange?.(busy); }, [busy, onBusyChange]);
  useEffect(() => { setStrengthDraft(Math.round((status?.strength ?? 1) * 100)); }, [status?.strength]);
  // Stable for the life of the panel, so a stop press always names the pass that is running.
  const [cancelId] = useState(() => `look-${Math.random().toString(36).slice(2, 10)}`);

  const report = useCallback(
    (error: unknown) => {
      const ipc = asIpcError(error);
      setError(ipc.message);
      onError?.({ code: ipc.code, message: ipc.message });
    },
    [onError],
  );

  const refresh = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    try {
      setLooks(await lookApi.listLooks());
    } catch (error) {
      report(error);
    }
    if (!projectId) {
      return;
    }
    // Three separate calls on purpose: a match query that fails must not stop the card showing
    // which look is selected.
    try {
      setStatus(await lookApi.lookStatus(projectId));
    } catch (error) {
      report(error);
    }
    try {
      setMatched(await lookApi.lookMatchReport(projectId));
    } catch (error) {
      report(error);
    }
  }, [projectId, report]);

  // The matrix for whichever look this project uses. Passing the project is what fills each
  // row's measured figure; without it a row can only say what the look asks for.
  useEffect(() => {
    if (!inTauri() || !status?.selected) {
      setBuckets([]);
      return;
    }
    let cancelled = false;
    void lookApi
      .lookBuckets({ profileId: status.selected, projectId })
      .then((rows: LookBucketDto[]) => {
        if (!cancelled) {
          setBuckets(rows);
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [projectId, status?.selected]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Read the address back as it is typed. This resolves nothing and contacts nothing - it parses
  // - so it is cheap enough to run on every keystroke and honest enough to show without a tick.
  useEffect(() => {
    if (!inTauri() || address.trim().length === 0) {
      setOrigin(null);
      return;
    }
    let cancelled = false;
    void lookApi
      .parseReference(address)
      .then((parsed: ReferenceOriginDto) => {
        if (!cancelled) {
          setOrigin(parsed);
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [address]);

  const measure = useCallback(async () => {
    if (!inTauri() || !projectId || busy) {
      return;
    }
    setBusy(true);
    stopped.current = false;
    setError(null);
    setResult(null);
    setProgress('Reading the reference photographs...');
    try {
      const next = await lookApi.measureLook({
        projectId,
        address: address.trim(),
        source,
        folder: folder.trim().length > 0 ? folder.trim() : null,
        name: name.trim(),
        cancelId,
      });
      setResult(next);
      setMatched(next.matched);
      if (!next.cancelled && next.profile && !stopped.current) {
        setProgress('Applying tone and color to your photos…');
        await lookApi.selectLook({ projectId, profileId: next.profile });
        setProgress(await renderSelectedLook(projectId, cancelId, () => stopped.current));
      } else setProgress('Stopped. No look was applied.');
      await refresh();
    } catch (error) {
      setProgress(null);
      report(error);
    } finally {
      setBusy(false);
    }
  }, [address, busy, cancelId, folder, name, projectId, refresh, report, source]);

  // Stopping is its own call rather than a flag on the pass: `cancel_job` is the command every
  // long job in this product is stopped with, and a second mechanism here would be a second
  // answer to "is this still running".
  const stop = useCallback(async () => {
    if (!inTauri() || !busy) {
      return;
    }
    setProgress('Stopping...');
    stopped.current = true;
    try {
      await api.cancelJob(cancelId);
    } catch (error) {
      report(error);
    }
  }, [busy, cancelId, report]);

  const select = useCallback(
    async (profileId: string | null) => {
      if (!inTauri() || !projectId) {
        return;
      }
      setBusy(true);
      stopped.current = false;
      setError(null);
      setProgress('Applying tone and color to your photos…');
      try {
        await lookApi.selectLook({ projectId, profileId });
        setProgress(await renderSelectedLook(projectId, cancelId, () => stopped.current));
        await refresh();
      } catch (error) {
        setProgress(null);
        report(error);
      } finally {
        setBusy(false);
      }
    },
    [cancelId, projectId, refresh, report],
  );

  const setStrength = useCallback(
    async (fraction: number) => {
      if (!inTauri() || !projectId) {
        return;
      }
      setBusy(true);
      stopped.current = false;
      setError(null);
      setProgress('Updating look strength…');
      try {
        await lookApi.setLookStrength({ projectId, fraction });
        setProgress(await renderSelectedLook(projectId, cancelId, () => stopped.current));
        await refresh();
      } catch (error) {
        setProgress(null);
        report(error);
      } finally { setBusy(false); }
    },
    [cancelId, projectId, refresh, report],
  );

  const fetchBlocked = !routeWorks(source) || status?.networkTransportAvailable === false;
  const canMeasure =
    Boolean(projectId) && inTauri() && (status?.appliable ?? 0) > 0 && routeWorks(source) && folder.trim().length > 0 && !busy;

  return (
    <section className="match-look-panel" aria-label="Match a look">
      <h2>Your photos. A look you love.</h2>

      <p className="match-look-intro">
        Match the color grading of an Instagram feed, a mood board, or your own work.
        Choose at least 8 saved reference photos in a folder; 24 or more gives a stronger starting point.
      </p>
      <p className="match-look-note">A profile link labels your look. Add saved photos to measure its colors; AURA does not download the profile.</p>
      {error && <p role="alert">{error} The selected look may be saved; retry applying to finish the edits.</p>}
      <fieldset className="look-inputs" disabled={busy}>

      <label className="match-look-address">
        The page this look is from
        <input
          type="text"
          value={address}
          placeholder="instagram.com/somebody"
          onChange={(event) => setAddress(event.target.value)}
          aria-label="Page address"
        />
      </label>
      {originSentence(origin).length > 0 && (
        <p className={origin?.understood ? 'match-look-origin' : 'match-look-origin-refused'}>
          {originSentence(origin)}
        </p>
      )}

      <fieldset className="match-look-source">
        <legend>Where the photographs are</legend>
        {SOURCE_CHOICES.map((choice) => (
          <label key={choice}>
            <input
              type="radio"
              name="look-source"
              value={choice}
              checked={source === choice}
              disabled={!routeWorks(choice)}
              onChange={() => setSource(choice)}
            />
            {sourceLabel(choice)}
            {!routeWorks(choice) && <span className="match-look-unavailable"> (not available)</span>}
          </label>
        ))}
      </fieldset>

      {fetchBlocked && source === 'public_url' && (
        <p className="match-look-blocked" role="note">
          {FETCH_UNAVAILABLE}
        </p>
      )}

      {routeWorks(source) && (
        <label className="match-look-folder">
          The folder
          <input
            type="text"
            value={folder}
            placeholder="/photographs/looks/somebody"
            onChange={(event) => setFolder(event.target.value)}
            aria-label="Reference folder"
          />
        </label>
      )}
      <button type="button" disabled={busy || !inTauri()} onClick={() => {
        void pickPhotoFolder('Choose a folder of reference photos').then(path => { if (path) setFolder(path); }).catch(report);
      }}>Choose reference folder</button>

      <label className="match-look-name">
        Call this look
        <input
          type="text"
          value={name}
          placeholder={origin?.understood ? origin.title : 'A name you will recognise'}
          onChange={(event) => setName(event.target.value)}
          aria-label="Look name"
        />
      </label>
      </fieldset>

      <p className="match-look-coverage">{coverageSentence(status)}</p>

      <div className="match-look-actions">
        <button type="button" onClick={() => void measure()} disabled={!canMeasure}>
          {busy ? 'Working…' : 'Match and apply look'}
        </button>
        {busy && (
          <button type="button" className="match-look-stop" onClick={() => void stop()}>
            Stop
          </button>
        )}
      </div>
      {progress && <p role="status" className="match-look-progress">{progress}</p>}
      {busy && (
        <p className="match-look-note">
          AURA measures the references, then applies the look through tone and color editing.
          This can take a few minutes. Stopping keeps edits already completed.
        </p>
      )}

      {result?.cancelled && (
        <p className="match-look-result" role="status">
          You stopped that measurement, so nothing was stored and nothing changed.
        </p>
      )}

      {result && !result.cancelled && (
        <div className="match-look-result" role="status">
          <p>
            Read {result.measured} of {result.found} reference photographs
            {result.refused > 0 ? `, and left ${result.refused} out` : ''}. Measured against{' '}
            {result.baselineFrames} of your own, across {result.buckets}{' '}
            {result.buckets === 1 ? 'kind of light' : 'kinds of light'}.
          </p>
          <p>{matchSentence(result.matched)}</p>
          <ul className="match-look-reasons">
            {result.reasons.map((reason) => (
              <li key={reason.code} className={reason.actionable ? 'actionable' : undefined}>
                {reason.sentence}
              </li>
            ))}
          </ul>
        </div>
      )}

      {looks.length > 0 && (
        <div className="match-look-library">
          <h3>Looks you have measured</h3>
          <ul>
            {looks.map((look) => (
              <li key={look.id}>
                <span className="match-look-name-cell">{look.name}</span>
                <span className="match-look-origin-cell">{look.origin}</span>
                <span className="match-look-count">
                  {look.references} photographs, {look.buckets}{' '}
                  {look.buckets === 1 ? 'kind of light' : 'kinds of light'}
                </span>
                {look.stale && (
                  <span className="match-look-stale">
                    Measured by an earlier version of AURA - measure it again to use it.
                  </span>
                )}
                <button
                  type="button"
                  onClick={() => void select(look.id)}
                  disabled={busy || !projectId || look.stale}
                >
                  {status?.selected === look.id ? 'Apply again' : 'Apply'}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {buckets.length > 0 && (
        <div className="match-look-matrix">
          <h3>What this look does in each light</h3>
          {/* The only axis this look has. A reference photograph does not say whether it is a
              ceremony or a reception, so there is nothing here about subject - and the note
              below says so rather than leaving a photographer to infer it. */}
          <table>
            <thead>
              <tr>
                <th scope="col">Light</th>
                <th scope="col">Reference photographs</th>
                <th scope="col">What it changes</th>
                <th scope="col">How close yours land</th>
              </tr>
            </thead>
            <tbody>
              {buckets.map((bucket) => (
                <tr key={bucket.lighting} className={bucket.weak ? 'weak' : undefined}>
                  <th scope="row">{bucket.title}</th>
                  <td>{bucket.samples}</td>
                  <td>{bucketSentence(bucket)}</td>
                  <td>
                    {/* `null` and zero mean opposite things here, so they never render the
                        same: nothing measured in this light is a dash. */}
                    {bucket.afterDe00 === null ? '—' : `${bucket.afterDe00.toFixed(1)} dE00`}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="match-look-note">
            A look is measured by kind of light, not by what the photograph is of. A page does not
            say which of its photographs are ceremonies, so AURA does not guess.
          </p>
        </div>
      )}

      {status?.selected && (
        <div className="match-look-selected">
          <p>
            Selected look: <strong>{status.selectedName}</strong>
            {status.selectedOrigin.length > 0 ? ` (${status.selectedOrigin})` : ''}.
          </p>
          <label>
            How much of it
            <input
              type="range"
              min={0}
              max={100}
              disabled={busy}
              value={strength}
              onChange={(event) => setStrengthDraft(Number(event.target.value))}
              aria-label="Look strength"
            />
            <span>{strength}%</span>
          </label>
          <button type="button" disabled={busy || strength === Math.round(status.strength * 100)} onClick={() => void setStrength(strength / 100)}>Apply strength</button>
          <p className="match-look-measured">Reference measurement at full strength: {matchSentence(matched)}</p>
          <button type="button" onClick={() => void select(null)} disabled={busy}>
            Go back to AURA's own look
          </button>
        </div>
      )}
    </section>
  );
}
