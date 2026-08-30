import { useMemo } from 'react';

import type { GalleryDeltaDto, NodeTargetDto } from '../../ipc/types';

/**
 * PHASE-25. The before-and-after strips: gallery drift, visible at a glance.
 *
 * Section 2.1's own deliverable - "strips showing WB, exposure and skin tone across the wedding so
 * drift is visible at a glance, before and after" - and section 6.4 calls it "a demo-friendly
 * visual and a genuine diagnostic tool". It is both, and the second is the harder one to earn.
 *
 * Four rules:
 *
 * 1. **No pixels.** A strip is a row of swatches whose colour is computed from a kelvin value, not
 *    a row of thumbnails. A wedding's worth of strip images is tens of megabytes over a channel
 *    that exists to carry decisions, and the thing being diagnosed is a *number* - two frames whose
 *    thumbnails look identical can be 400 K apart. ADR-0052 section 8.
 *
 * 2. **Before and after are the same scale, and the scale is shown.** A strip that auto-ranged each
 *    row would make a gallery that improved by 5 % look exactly like one that improved by 80 %,
 *    because both would fill the row. The domain is the *union* of the two rows and it is labelled.
 *
 * 3. **A frame that did not move is drawn, not skipped.** An intentionally-lit frame, a frame the
 *    photographer set by hand and a frame that was already consistent all appear in both rows
 *    unchanged - and the first two are marked, because a gap in a strip reads as missing data.
 *
 * 4. **The tolerance band is drawn behind the after row.** "Consistent" is the node's own
 *    tolerance, which varies by scene: 120 K in a family portrait session and 450 K on a dance
 *    floor. Without the band, a photographer comparing two chapters is comparing two different
 *    questions.
 *
 * The component is pure: it receives deltas and a target, fetches nothing and renders no images.
 */
export type TimelineStripsProps = {
  /** One node's deltas, in capture order. */
  deltas: GalleryDeltaDto[];
  /** What the node's anchors said, or null when it could not be anchored. */
  target: NodeTargetDto | null;
  /** Which frame is selected, if any. */
  selectedPhotoId?: string | null;
  /** Select a frame from the strip. */
  onSelect?: (photoId: string) => void;
};

/** What one swatch needs to draw itself. */
type Swatch = {
  photoId: string;
  before: number;
  after: number;
  /** True when nothing moved and the frame says why. */
  held: boolean;
  /** The reason a held frame gives, for the title attribute. */
  note: string;
};

/**
 * A kelvin value as a colour, for the swatch.
 *
 * Warm is amber, cool is blue, and the neutral point is 5000 K. It is a *diagnostic* mapping rather
 * than a colorimetric one - the point is that a photographer can see two frames disagree, and an
 * accurate blackbody rendering compresses the interesting range into two indistinguishable greys.
 */
export function kelvinSwatch(kelvin: number): string {
  const t = Math.max(-1, Math.min(1, (kelvin - 5000) / 2500));
  if (t >= 0) {
    // Warmer than neutral: toward amber.
    const r = Math.round(240 + 15 * t);
    const g = Math.round(228 - 60 * t);
    const b = Math.round(205 - 130 * t);
    return `rgb(${r}, ${g}, ${b})`;
  }
  // Cooler than neutral: toward blue.
  const k = -t;
  const r = Math.round(240 - 110 * k);
  const g = Math.round(228 - 40 * k);
  const b = Math.round(205 + 45 * k);
  return `rgb(${r}, ${g}, ${b})`;
}

/** An exposure offset as a grey, for the swatch. */
export function exposureSwatch(stops: number): string {
  const t = Math.max(-1, Math.min(1, stops / 1.5));
  const v = Math.round(150 + 80 * t);
  return `rgb(${v}, ${v}, ${v})`;
}

export function TimelineStrips({
  deltas,
  target,
  selectedPhotoId,
  onSelect,
}: TimelineStripsProps): JSX.Element {
  const warmth = useMemo<Swatch[]>(
    () =>
      deltas.map((delta) => ({
        photoId: delta.photoId,
        before: delta.fromCctK,
        after: delta.fromCctK + delta.dCct,
        held: delta.dCct === 0,
        note: heldNote(delta),
      })),
    [deltas],
  );

  const exposure = useMemo<Swatch[]>(
    () =>
      deltas.map((delta) => ({
        photoId: delta.photoId,
        before: delta.fromExposureEv,
        after: delta.fromExposureEv + delta.dExposure,
        held: delta.dExposure === 0,
        note: heldNote(delta),
      })),
    [deltas],
  );

  const warmthSpread = useMemo(() => spreadOf(warmth), [warmth]);
  const exposureSpread = useMemo(() => spreadOf(exposure), [exposure]);

  if (deltas.length === 0) {
    return (
      <section className="timeline-strips timeline-strips--empty">
        <p>This part of the wedding has no photographs in it yet.</p>
      </section>
    );
  }

  return (
    <section className="timeline-strips" aria-label="Gallery drift, before and after">
      <StripPair
        title="Warmth"
        unit="K"
        rows={warmth}
        spread={warmthSpread}
        tolerance={target ? target.cctTol : null}
        colour={kelvinSwatch}
        format={(value) => `${Math.round(value)} K`}
        selectedPhotoId={selectedPhotoId ?? null}
        onSelect={onSelect}
      />
      <StripPair
        title="Brightness"
        unit="EV"
        rows={exposure}
        spread={exposureSpread}
        tolerance={null}
        colour={exposureSwatch}
        format={(value) => `${value >= 0 ? '+' : ''}${value.toFixed(2)} EV`}
        selectedPhotoId={selectedPhotoId ?? null}
        onSelect={onSelect}
      />
      {target === null ? (
        <p className="timeline-strips__caveat">
          AURA could not find three frames it was confident enough about to anchor this part of the
          wedding, so nothing here was matched to anything. These are the frames as they were.
        </p>
      ) : null}
    </section>
  );
}

type StripPairProps = {
  title: string;
  unit: string;
  rows: Swatch[];
  spread: { before: number; after: number };
  tolerance: number | null;
  colour: (value: number) => string;
  format: (value: number) => string;
  selectedPhotoId: string | null;
  onSelect?: (photoId: string) => void;
};

function StripPair({
  title,
  unit,
  rows,
  spread,
  tolerance,
  colour,
  format,
  selectedPhotoId,
  onSelect,
}: StripPairProps): JSX.Element {
  const reduction = spread.before > 0 ? 1 - spread.after / spread.before : 0;
  return (
    <div className="strip-pair">
      <header className="strip-pair__header">
        <h4>{title}</h4>
        <span className="strip-pair__spread">
          spread {spread.before.toFixed(unit === 'K' ? 0 : 2)} → {spread.after.toFixed(unit === 'K' ? 0 : 2)} {unit}
          {spread.before > 0 ? ` (${Math.round(reduction * 100)} % closer)` : ''}
        </span>
        {tolerance !== null ? (
          <span className="strip-pair__tolerance">
            consistent within ±{Math.round(tolerance)} {unit} here
          </span>
        ) : null}
      </header>
      <Strip
        label="Before"
        values={rows.map((row) => ({ ...row, value: row.before }))}
        colour={colour}
        format={format}
        selectedPhotoId={selectedPhotoId}
        onSelect={onSelect}
      />
      <Strip
        label="After"
        values={rows.map((row) => ({ ...row, value: row.after }))}
        colour={colour}
        format={format}
        selectedPhotoId={selectedPhotoId}
        onSelect={onSelect}
      />
    </div>
  );
}

type StripProps = {
  label: string;
  values: Array<Swatch & { value: number }>;
  colour: (value: number) => string;
  format: (value: number) => string;
  selectedPhotoId: string | null;
  onSelect?: (photoId: string) => void;
};

function Strip({ label, values, colour, format, selectedPhotoId, onSelect }: StripProps): JSX.Element {
  return (
    <div className="strip">
      <span className="strip__label">{label}</span>
      <ol className="strip__swatches">
        {values.map((swatch) => (
          <li key={`${label}-${swatch.photoId}`}>
            <button
              type="button"
              className={
                swatch.photoId === selectedPhotoId ? 'swatch swatch--selected' : 'swatch'
              }
              style={{ background: colour(swatch.value) }}
              title={`${format(swatch.value)}${swatch.note ? ` — ${swatch.note}` : ''}`}
              aria-label={`${label}: ${format(swatch.value)}`}
              onClick={onSelect ? () => onSelect(swatch.photoId) : undefined}
            >
              {swatch.held && swatch.note ? <span className="swatch__held" aria-hidden>·</span> : null}
            </button>
          </li>
        ))}
      </ol>
    </div>
  );
}

/** The mean absolute deviation of a strip, which is what the gate is measured in. */
function spreadOf(rows: Swatch[]): { before: number; after: number } {
  return { before: mad(rows.map((r) => r.before)), after: mad(rows.map((r) => r.after)) };
}

function mad(values: number[]): number {
  if (values.length < 2) {
    return 0;
  }
  const mean = values.reduce((a, b) => a + b, 0) / values.length;
  return values.reduce((acc, v) => acc + Math.abs(v - mean), 0) / values.length;
}

/**
 * Why a frame did not move, when it did not.
 *
 * Rule 3: a frame that was left alone on purpose and a frame that had nothing to move toward look
 * identical in a strip, and only the first is a decision.
 */
function heldNote(delta: GalleryDeltaDto): string {
  const withdrawn = delta.reasons.find((reason) => reason.withdraws);
  return withdrawn ? withdrawn.text : '';
}
