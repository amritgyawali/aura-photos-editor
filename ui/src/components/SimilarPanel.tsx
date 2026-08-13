import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError, inTauri } from '../ipc/client';
import type {
  DescriptorsDto,
  EmbedProgressDto,
  IndexStatusDto,
  SimilarNeighbourDto,
  SimilarResultDto,
} from '../ipc/types';

export type SimilarPanelProps = {
  projectId: string;
  /** The photograph under the cursor, or the selection. */
  photoId: string | null;
  onError?: (error: { code: string; message: string }) => void;
  /** Clicking a neighbour selects it, so the panel can be walked. */
  onSelect?: (photoId: string) => void;
};

/** The time windows the panel offers. `null` is "the whole wedding". */
export const TIME_WINDOWS: ReadonlyArray<{ label: string; seconds: number | null }> = [
  { label: 'Whole wedding', seconds: null },
  { label: 'Within 2 seconds', seconds: 2 },
  { label: 'Within 5 seconds', seconds: 5 },
  { label: 'Within 30 seconds', seconds: 30 },
  { label: 'Within 5 minutes', seconds: 300 },
];

/**
 * The distance, as a photographer would read it.
 *
 * Two numbers for one fact - see ADR-0012. A percentage is what a human compares;
 * the distance is what the next phase's threshold is written against, so both are
 * shown and neither is computed in the UI.
 */
export function similarityLabel(neighbour: SimilarNeighbourDto): string {
  const percent = Math.round(neighbour.similarity * 100);
  return `${percent}% alike (d ${neighbour.distance.toFixed(4)})`;
}

/**
 * What the difference hash says, in words.
 *
 * Deliberately not a verdict. `0` means the two frames are identical to the
 * perceptual hash, which is evidence phase 08 will act on; this panel reports it
 * and stops there.
 */
export function duplicateLabel(neighbour: SimilarNeighbourDto): string {
  if (neighbour.dhashDistance === 0) {
    return 'identical to the hash';
  }
  if (neighbour.nearDuplicate) {
    return `near duplicate (${neighbour.dhashDistance} bits)`;
  }
  return `${neighbour.dhashDistance} bits apart`;
}

/**
 * One sentence about the index, including every reason it might answer nothing.
 *
 * An empty result on an unembedded project is a support ticket. This is the
 * sentence that answers it before it is filed.
 */
export function coverageSentence(status: IndexStatusDto): string {
  if (status.photos === 0) {
    return 'This project has no photographs yet.';
  }
  if (status.vectors === 0) {
    return `None of the ${status.photos} photographs have been analysed yet, so nothing can be compared.`;
  }
  const percent = Math.round(status.coverage * 100);
  const source = status.fromSnapshot ? 'loaded from a snapshot' : `built in ${status.buildMs} ms`;
  const stale =
    status.staleModelVersions.length > 0
      ? ` ${status.staleModelVersions.length} older version(s) are being re-read in the background.`
      : '';
  return `${status.vectors} of ${status.photos} photographs analysed (${percent}%), index ${source}.${stale}`;
}

/** How many frames a filtered query can reach, when that differs from the total. */
export function filterableSentence(status: IndexStatusDto): string | null {
  if (status.vectors === 0 || status.filterable === status.vectors) {
    return null;
  }
  const missing = status.vectors - status.filterable;
  return `${missing} photograph(s) have no capture time or camera, so a time or camera filter cannot reach them.`;
}

/** What one embedding pass did, as a sentence. */
export function passSentence(progress: EmbedProgressDto): string {
  const parts = [`analysed ${progress.embedded}`];
  if (progress.failed > 0) {
    parts.push(`${progress.failed} could not be read`);
  }
  if (progress.remaining > 0) {
    parts.push(`${progress.remaining} still to do`);
  }
  if (progress.cancelled) {
    parts.push('stopped early');
  }
  const seconds = (progress.elapsedMs / 1000).toFixed(1);
  return `${parts.join(', ')} in ${seconds} s.`;
}

/** The descriptor readout, one line per number a later phase will use. */
export function descriptorLines(descriptors: DescriptorsDto): string[] {
  return [
    `hash ${descriptors.dhashHex}`,
    `luminance mean ${descriptors.lumaMean.toFixed(3)}, p1 ${descriptors.lumaP1.toFixed(3)}, p50 ${descriptors.lumaP50.toFixed(3)}, p99 ${descriptors.lumaP99.toFixed(3)}`,
    `clipped ${(descriptors.clipLo * 100).toFixed(1)}% dark, ${(descriptors.clipHi * 100).toFixed(1)}% bright`,
    `edge energy ${descriptors.edgeEnergy.toFixed(4)}`,
    `read from ${descriptors.pixelSource}, model version ${descriptors.modelVer}`,
  ];
}

/**
 * The debug "find similar" panel.
 *
 * Section 8 step 8 of PHASE-05 calls this "invaluable for later phases", and it is
 * exactly that: phases 07, 08, 25, 26 and 29 all reason about neighbours, and none
 * of them can be inspected by a human without a screen that shows what the index
 * actually returned and how long it took.
 *
 * It is a debug surface, so it shows the numbers rather than hiding them: the
 * distance as well as the percentage, the Hamming distance as well as the label,
 * the elapsed milliseconds as well as the results.
 */
export function SimilarPanel({
  projectId,
  photoId,
  onError,
  onSelect,
}: SimilarPanelProps): JSX.Element {
  const [status, setStatus] = useState<IndexStatusDto | null>(null);
  const [result, setResult] = useState<SimilarResultDto | null>(null);
  const [descriptors, setDescriptors] = useState<DescriptorsDto | null>(null);
  const [windowIndex, setWindowIndex] = useState(0);
  const [progress, setProgress] = useState<EmbedProgressDto | null>(null);
  const [busy, setBusy] = useState(false);

  const report = useCallback(
    (error: unknown) => {
      const typed = asIpcError(error);
      onError?.({ code: typed.code, message: typed.message });
    },
    [onError],
  );

  const refreshStatus = useCallback(() => {
    if (!inTauri()) {
      return;
    }
    api.indexStatus(projectId).then(setStatus).catch(report);
  }, [projectId, report]);

  useEffect(refreshStatus, [refreshStatus]);

  const search = useCallback(() => {
    if (!inTauri() || photoId === null) {
      return;
    }
    const chosen = TIME_WINDOWS[windowIndex] ?? TIME_WINDOWS[0];
    api
      .findSimilar({
        projectId,
        photoId,
        k: 24,
        timeWindowS: chosen?.seconds ?? null,
        cameraId: null,
        exclude: [],
      })
      .then(setResult)
      .catch(report);
    api.imageDescriptors(projectId, photoId).then(setDescriptors).catch(() => {
      // A frame with no descriptors is not an error worth a banner: the panel
      // simply has nothing to read out, and the neighbour list already says why.
      setDescriptors(null);
    });
  }, [projectId, photoId, windowIndex, report]);

  useEffect(search, [search]);

  const analyse = useCallback(() => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    api
      .embedProject({ projectId })
      .then((pass) => {
        setProgress(pass);
        refreshStatus();
        search();
      })
      .catch(report)
      .finally(() => setBusy(false));
  }, [projectId, refreshStatus, search, report]);

  const rebuild = useCallback(() => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    api
      .buildIndex(projectId)
      .then(setStatus)
      .catch(report)
      .finally(() => setBusy(false));
  }, [projectId, report]);

  return (
    <section className="similar-panel" aria-label="Find similar">
      <h2>Find similar</h2>

      {status === null ? (
        <p>Reading the index…</p>
      ) : (
        <>
          <p className="coverage">{coverageSentence(status)}</p>
          {filterableSentence(status) !== null && (
            <p className="filterable">{filterableSentence(status)}</p>
          )}
        </>
      )}

      <div className="controls">
        <label htmlFor="similar-window">Only frames</label>
        <select
          id="similar-window"
          value={windowIndex}
          onChange={(event) => setWindowIndex(Number(event.target.value))}
        >
          {TIME_WINDOWS.map((window, index) => (
            <option key={window.label} value={index}>
              {window.label}
            </option>
          ))}
        </select>
        <button type="button" onClick={analyse} disabled={busy}>
          Analyse this project
        </button>
        <button type="button" onClick={rebuild} disabled={busy}>
          Rebuild index
        </button>
      </div>

      {progress !== null && <p className="pass">{passSentence(progress)}</p>}

      {photoId === null ? (
        <p>Select a photograph to see what looks like it.</p>
      ) : result === null ? (
        <p>No results yet.</p>
      ) : result.neighbours.length === 0 ? (
        <p>Nothing matched that filter.</p>
      ) : (
        <>
          <p className="timing">
            {result.neighbours.length} neighbours in {result.elapsedMs.toFixed(2)} ms (
            {result.filterKind} filter)
          </p>
          <ol className="neighbours">
            {result.neighbours.map((neighbour) => (
              <li key={neighbour.photoId}>
                <button type="button" onClick={() => onSelect?.(neighbour.photoId)}>
                  {neighbour.photoId}
                </button>
                <span className="similarity">{similarityLabel(neighbour)}</span>
                <span className={neighbour.nearDuplicate ? 'duplicate' : 'distinct'}>
                  {duplicateLabel(neighbour)}
                </span>
              </li>
            ))}
          </ol>
        </>
      )}

      {descriptors !== null && (
        <ul className="descriptors">
          {descriptorLines(descriptors).map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}
    </section>
  );
}
