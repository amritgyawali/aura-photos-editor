/**
 * PHASE-17. The "Teach My AI" wizard: point at folders, look at what is there, train, adopt.
 *
 * Four steps, and the order is the argument. A photographer sees **what the scan found before
 * anything is fitted** - how many originals, how many finals, how many paired and by what -
 * because the commonest failure in this whole feature is a folder with finals and no RAWs, and
 * finding that out after a twenty-minute training run is the worst possible moment.
 *
 * The panel never claims a profile is "ready". Section 12's fourth failure mode is users
 * expecting one-click magic from twenty photographs, and the mitigation is a strength meter and
 * a minimum-data sentence rather than a false ready state.
 */
import { useCallback, useState } from 'react';

import { asIpcError, style } from '../../ipc/client';
import type { ScanArchiveDto, TrainProfileDto } from '../../ipc/types';

export type TeachMyAiProps = {
  projectId: string | null;
  onTrained?: (profileId: string) => void;
  onError?: (message: string) => void;
};

/** What the scan's headline says. The unmatched finals are the number that matters. */
export function scanHeadline(scan: ScanArchiveDto): string {
  if (scan.originals === 0) {
    return 'No camera originals in that folder. AURA learns from the difference between what the camera recorded and what you delivered, so it needs both.';
  }
  if (scan.finals === 0) {
    return 'No finished photographs in that folder. AURA needs your exported JPEGs or TIFFs beside the originals.';
  }
  if (scan.unmatchedFinals > scan.matched) {
    return `Only ${scan.matched.toLocaleString()} of ${scan.finals.toLocaleString()} finished photographs could be matched to an original. The rest are probably from a wedding whose RAWs are on another disk.`;
  }
  return `${scan.matched.toLocaleString()} pairs found, matched at worst by ${scan.weakestMethod.replace(/_/g, ' ')}.`;
}

/** What the training result says once it has finished. */
export function trainHeadline(result: TrainProfileDto): string {
  if (result.cancelled) {
    return 'Stopped. Everything AURA had already worked out is kept, so starting again picks up where this left off.';
  }
  if (!result.profile) {
    return 'AURA could not find enough it could learn from in those folders, so it has not made a profile.';
  }
  const improvement = result.baselineDe00 - result.overallDe00;
  const better =
    improvement > 0
      ? `That is ${improvement.toFixed(1)} closer to your own edits than AURA managed without it.`
      : 'It is not yet closer to your own edits than AURA manages without it, which usually means one more wedding is needed.';
  return `${result.accepted.toLocaleString()} photographs used, ${result.rejected.toLocaleString()} left out. ${better}`;
}

export function TeachMyAi({ projectId, onTrained, onError }: TeachMyAiProps) {
  const [name, setName] = useState('personal');
  const [roots, setRoots] = useState('');
  const [scan, setScan] = useState<ScanArchiveDto | null>(null);
  const [result, setResult] = useState<TrainProfileDto | null>(null);
  const [busy, setBusy] = useState(false);

  const fail = useCallback(
    (err: unknown) => {
      const ipc = asIpcError(err);
      onError?.(`${ipc.code}: ${ipc.message}`);
    },
    [onError],
  );

  const doScan = useCallback(async () => {
    const list = roots
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    if (list.length === 0) return;
    setBusy(true);
    try {
      setScan(await style.scanArchive({ name, roots: list }));
      setResult(null);
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  }, [fail, name, roots]);

  const doTrain = useCallback(async () => {
    setBusy(true);
    try {
      const trained = await style.trainProfile({ name, cancelId: `style-${name}` });
      setResult(trained);
      if (trained.profile) onTrained?.(trained.profile.profileId);
    } catch (err) {
      fail(err);
    } finally {
      setBusy(false);
    }
  }, [fail, name, onTrained]);

  return (
    <section className="teach-my-ai" aria-label="Teach My AI">
      <h2>Teach AURA how you edit</h2>
      <p>
        Point AURA at weddings you have already finished. It reads the difference between the
        originals and what you delivered, and learns that difference separately for each kind of
        photograph and each kind of light.
      </p>
      <p className="teach-privacy">
        Nothing leaves this machine. AURA reads your files where they are and stores only the
        settings it worked out — no photographs are copied and nothing is uploaded.
      </p>

      <label>
        <span>What to call this look</span>
        <input value={name} onChange={(event) => setName(event.target.value)} />
      </label>

      <label>
        <span>Folders, one per line</span>
        <textarea
          value={roots}
          rows={4}
          onChange={(event) => setRoots(event.target.value)}
          placeholder={'D:\\weddings\\2025-06-smith\nD:\\weddings\\2025-07-patel'}
        />
      </label>

      <button type="button" disabled={busy} onClick={doScan}>
        Look at these folders
      </button>

      {scan ? (
        <div className="teach-scan">
          <p>{scanHeadline(scan)}</p>
          <dl>
            <div>
              <dt>Originals</dt>
              <dd>{scan.originals.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Finished photographs</dt>
              <dd>{scan.finals.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Paired</dt>
              <dd>{scan.matched.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Finished photographs with no original</dt>
              <dd>{scan.unmatchedFinals.toLocaleString()}</dd>
            </div>
          </dl>
          {scan.enough ? null : (
            <p className="teach-warning">
              That is fewer pairs than AURA needs to learn anything it would stand behind. Add
              another wedding.
            </p>
          )}
          <button type="button" disabled={busy || !scan.enough || !projectId} onClick={doTrain}>
            Learn my style
          </button>
        </div>
      ) : null}

      {result ? <p className="teach-result">{trainHeadline(result)}</p> : null}
    </section>
  );
}
