import { useEffect, useId, useState } from 'react';

import type { SelectionDto } from '../../ipc/types';

/**
 * PHASE-12's size control.
 *
 * Section 6.4: "the size slider re-runs only the allocation passes (not
 * analysis), so it feels instant; the coverage guard always runs last so
 * shrinking never breaks must-haves." Section 11 budgets two seconds.
 *
 * Two things this component must never hide.
 *
 * **The delivered count can exceed the requested one.** A guarantee outranks a
 * slider, so asking for 500 on a wedding whose must-haves need 540 delivers 540.
 * The control shows both numbers whenever they differ, with the reason, rather
 * than silently snapping the handle to the number that came back.
 *
 * **A drag is a request, not a state.** The commit is debounced and the handle
 * stays where the photographer put it while the re-selection runs; a control
 * that jumped back mid-drag would feel broken and would also be lying about what
 * was asked for.
 */

/** How long a drag rests before the re-selection is asked for. */
export const COMMIT_DELAY_MS = 220;

export type SizeSliderProps = {
  /** The current stored selection, or null before the first cull. */
  selection: SelectionDto | null;
  /** How many photographs could be delivered at all. The slider's ceiling. */
  eligible: number;
  /** Ask for a new target. Rejected while a previous request is in flight. */
  onResize: (target: number) => Promise<void>;
  /** True while a re-selection is running. */
  busy?: boolean;
};

/** The sentence shown when coverage pushed the gallery past the request. */
export function overshootSentence(requested: number, delivered: number): string {
  const extra = delivered - requested;
  return `${delivered} photographs, ${extra} more than the ${requested} you asked for. AURA cannot go below this without dropping part of the wedding it guarantees.`;
}

/** The lowest target the control offers, so the handle is never useless. */
export function floorFor(eligible: number): number {
  return Math.max(1, Math.round(eligible * 0.05));
}

export function SizeSlider({ selection, eligible, onResize, busy = false }: SizeSliderProps) {
  const inputId = useId();
  const [target, setTarget] = useState(selection?.targetCount ?? 0);

  // Follow the backend only when the photographer is not driving. A slider that
  // reset itself under the cursor mid-drag would fight the person using it.
  useEffect(() => {
    if (!busy && selection) {
      setTarget(selection.targetCount);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection?.targetCount]);

  useEffect(() => {
    if (!selection || target === selection.targetCount) {
      return undefined;
    }
    const handle = window.setTimeout(() => {
      void onResize(target);
    }, COMMIT_DELAY_MS);
    return () => {
      window.clearTimeout(handle);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);

  if (!selection) {
    return (
      <section className="size-slider size-slider--empty" aria-label="Gallery size">
        <p>
          AURA has not chosen a gallery for this wedding yet, so there is no size to
          adjust.
        </p>
      </section>
    );
  }

  const min = floorFor(eligible);
  const max = Math.max(min, eligible);
  const overshot = selection.actualCount > selection.targetCount;

  return (
    <section className="size-slider" aria-label="Gallery size">
      <label htmlFor={inputId}>Gallery size</label>
      <input
        id={inputId}
        type="range"
        min={min}
        max={max}
        step={10}
        value={Math.min(Math.max(target, min), max)}
        disabled={busy}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={target}
        onChange={(event) => {
          setTarget(Number(event.target.value));
        }}
      />
      <output htmlFor={inputId}>
        {selection.actualCount} of {eligible} photographs
      </output>
      {overshot ? (
        <p className="size-slider__note" role="note">
          {overshootSentence(selection.targetCount, selection.actualCount)}
        </p>
      ) : null}
      {busy ? <p className="size-slider__busy">Choosing again…</p> : null}
    </section>
  );
}
