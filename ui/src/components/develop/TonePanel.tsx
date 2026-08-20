import { useMemo } from 'react';

import type { ColourDto, ColourStatusDto, RecipeDto } from '../../ipc/types';

/**
 * PHASE-16. The Tone panel: contrast, highlights, shadows, whites, blacks, vibrance and
 * saturation, with the skin guarantee reported beside them.
 *
 * Section 4 calls for "tone/curve/HSL UI with AI badges"; section 9 adds the alternatives
 * switcher and the protected-skin indicator. Five rules this component keeps, and each one is
 * a promise the product makes elsewhere that a photographer can only check here:
 *
 * 1. **A value AURA chose says so, and says how sure it was.** One confidence, because unlike
 *    phase 15 there is one decision here rather than two independent ones.
 * 2. **A value a person set carries the protected dot** - phase 14's mark, reused rather than
 *    re-invented, because two spellings of "you set this" is two things to trust.
 * 3. **A frame with no grade is not a frame with a neutral grade.** It renders as "not graded
 *    yet", never as zeroes, because an untouched frame that looks like a decision is how a gap
 *    in coverage becomes invisible.
 * 4. **The skin guarantee is stated as a measurement.** Not "skin is protected" but "skin
 *    moved 0.4 degrees, and the limit is 2" - and when there was nobody in the frame it says
 *    *that* rather than showing a perfect score.
 * 5. **The alternatives are one click and they are complete.** Every one has been through both
 *    guards, so switching is safe rather than only fast.
 *
 * What is deliberately absent: every control phase 15 owns. No exposure, no temperature, no
 * tint. Section 2.2 draws that line and the IPC surface enforces it - there is no path from
 * this panel to a white balance.
 */

/** What the panel needs. Everything comes from the IPC surface; nothing is derived here. */
export type TonePanelProps = {
  /** The photograph's grade, or `null` when nobody has graded it. */
  colour: ColourDto | null;
  /** The current edit, for the protected marks. */
  recipe: RecipeDto;
  /** The project's coverage, for the header. */
  status: ColourStatusDto | null;
  /** Record what the photographer set instead. */
  onOverride: (values: {
    contrast?: number;
    highlights?: number;
    shadows?: number;
    whites?: number;
    blacks?: number;
    vibrance?: number;
    saturation?: number;
  }) => void;
  /** Record that they looked and agree. */
  onAccept: () => void;
  /** Promote one stored alternative. */
  onSelectVariant: (kind: string) => void;
  /** Give the frame back to AURA. Phase 14's history action, reused. */
  onResetToAi: () => void;
};

/** The seven recipe paths this panel can write, and nothing else can be added here. */
const PATHS = {
  contrast: 'global.contrast',
  highlights: 'global.highlights',
  shadows: 'global.shadows',
  whites: 'global.whites',
  blacks: 'global.blacks',
  vibrance: 'global.vibrance',
  saturation: 'global.saturation',
} as const;

type Slider = keyof typeof PATHS;

/** The label a photographer reads for each control. */
const LABELS: Record<Slider, string> = {
  contrast: 'Contrast',
  highlights: 'Highlights',
  shadows: 'Shadows',
  whites: 'Whites',
  blacks: 'Blacks',
  vibrance: 'Vibrance',
  saturation: 'Saturation',
};

/** Plain words for an alternative. */
function variantName(kind: string): string {
  switch (kind) {
    case 'flatter':
      return 'Flatter';
    case 'punchier':
      return 'Punchier';
    case 'warmer':
      return 'Warmer';
    default:
      return kind;
  }
}

/** A percentage, rounded the way the sentences round it. */
function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function TonePanel({
  colour,
  recipe,
  status,
  onOverride,
  onAccept,
  onSelectVariant,
  onResetToAi,
}: TonePanelProps) {
  const protectedPaths = useMemo(
    () => new Set(recipe.userEditedFields),
    [recipe.userEditedFields],
  );

  if (!colour) {
    return (
      <section className="tone-panel" aria-label="Tone and colour">
        <header className="tone-header">
          <h2>Tone and colour</h2>
        </header>
        <p className="tone-ungraded" data-testid="tone-ungraded">
          AURA has not worked out the contrast and colour for this photograph yet. It is
          showing with its exposure and white balance only.
        </p>
      </section>
    );
  }

  const values: Record<Slider, number> = {
    contrast: colour.contrast,
    highlights: colour.highlights,
    shadows: colour.shadows,
    whites: colour.whites,
    blacks: colour.blacks,
    vibrance: colour.vibrance,
    saturation: colour.saturation,
  };

  const guard = colour.skinGuard;

  return (
    <section className="tone-panel" aria-label="Tone and colour">
      <header className="tone-header">
        <h2>Tone and colour</h2>
        {colour.userEdited ? (
          <p className="tone-provenance" data-testid="tone-provenance">
            Your settings. AURA had suggested {Math.round(colour.contrast)} contrast.
          </p>
        ) : (
          <p className="tone-provenance" data-testid="tone-provenance">
            AURA set these, {percent(colour.confidence)} confident.
          </p>
        )}
        <p className="tone-subtlety" data-testid="tone-subtlety">
          {colour.subtlety < 0.12
            ? 'A light touch: most of this photograph is as the camera recorded it.'
            : 'A normal amount of grading for this kind of photograph.'}
        </p>
      </header>

      {/*
        The guarantee, as a measurement rather than a badge. `measured` false is not a good
        score - it means there was nobody in the frame to protect - and the two are rendered
        as different sentences on purpose.
      */}
      <p
        className={guard.measured ? 'tone-skin' : 'tone-skin tone-skin-absent'}
        data-testid="tone-skin-guard"
      >
        {guard.measured
          ? `Skin moved ${guard.maxHueShiftDeg.toFixed(1)} degrees of hue and ${percent(
              guard.maxChromaChange,
            )} of colour. AURA's limit is 2 degrees and 6%.`
          : 'There is nobody in this frame, so the skin protection did not apply here.'}
        {guard.measured && guard.resolves > 0
          ? ' The colour adjustments were worked out again more gently to keep it there.'
          : ''}
      </p>

      <ol className="tone-controls">
        {(Object.keys(PATHS) as Slider[]).map((slider) => (
          <li className="tone-control" key={slider}>
            <label htmlFor={`tone-${slider}`}>
              {LABELS[slider]}
              {protectedPaths.has(PATHS[slider]) ? (
                <span
                  className="develop-protected"
                  title="You set this. AURA will not change it."
                  aria-label="You set this. AURA will not change it."
                >
                  ●
                </span>
              ) : null}
            </label>
            <input
              id={`tone-${slider}`}
              type="number"
              step="1"
              value={String(Math.round(values[slider]))}
              onChange={(event) => onOverride({ [slider]: Number(event.target.value) })}
            />
          </li>
        ))}
      </ol>

      {colour.alternatives.length > 0 ? (
        <div className="tone-variants" data-testid="tone-variants">
          <h3>Try instead</h3>
          <ul>
            {colour.alternatives.map((variant) => (
              <li key={variant.kind}>
                <button type="button" onClick={() => onSelectVariant(variant.kind)}>
                  {variantName(variant.kind)}
                </button>
              </li>
            ))}
          </ul>
          <p className="tone-variants-note">
            Each of these has been checked the same way as the main grade, including the skin
            protection.
          </p>
        </div>
      ) : (
        <p className="tone-variants-empty" data-testid="tone-variants-empty">
          There is no other version of this grade worth offering - the alternatives would have
          looked the same.
        </p>
      )}

      <ul className="tone-reasons" aria-label="Why AURA graded it this way">
        {colour.reasons.map((reason) => (
          <li key={reason.code} data-testid={`tone-reason-${reason.code}`}>
            {reason.text}
          </li>
        ))}
      </ul>

      {colour.clippingAdded > 0.0001 ? (
        <p className="tone-clipping" data-testid="tone-clipping">
          This grade brightens {percent(colour.clippingAdded)} of the frame to the very top of
          its range.
        </p>
      ) : null}

      <footer className="tone-actions">
        <button type="button" onClick={onAccept} disabled={colour.reviewed}>
          {colour.reviewed ? 'Looked at' : 'Looks right'}
        </button>
        <button type="button" onClick={onResetToAi} disabled={!colour.userEdited}>
          Reset to AI suggestion
        </button>
      </footer>

      {status ? (
        <p className="tone-coverage" data-testid="tone-coverage">
          {status.decided} of {status.photos} photographs graded.{' '}
          {status.skinMeasured === 0
            ? 'The skin protection has not been checked on any of them yet, because no faces have been found.'
            : `The skin protection was checked on ${status.skinMeasured} of them; the largest movement anywhere was ${status.worstSkinHueShift.toFixed(
                1,
              )} degrees.`}
        </p>
      ) : null}
    </section>
  );
}
