import { useMemo } from 'react';

import type { RecipeDto, ToneDto, ToneStatusDto } from '../../ipc/types';

/**
 * PHASE-15. The Basic panel: exposure and white balance.
 *
 * Section 4 calls for "exposure/WB controls with AI badge and reset-to-AI"; section 9 adds
 * the confidence and the mixed-light indicator. Four rules this component keeps, and each
 * one is a promise the product makes elsewhere that a photographer can only check here:
 *
 * 1. **A value AURA chose says so, and says how sure it was.** Two confidences, not one:
 *    a backlit frame can have an obvious colour and a hard-won exposure, and averaging them
 *    into a single badge hides exactly the case worth looking at.
 * 2. **A value a person set carries the protected dot** - phase 14's mark, reused rather
 *    than re-invented, because two spellings of "you set this" is two things to trust.
 * 3. **A frame with no estimate is not a frame with a neutral estimate.** It renders as
 *    "not estimated yet", never as zero stops at 5500 K, because an untouched frame that
 *    looks like a decision is how a gap in coverage becomes invisible.
 * 4. **Mixed light and preserved colour are stated, not implied.** Section 2.1 marks those
 *    frames so phase 18 can correct them locally; until phase 18 exists, the honest thing
 *    is to tell the photographer that the colour here was set for the people and not for
 *    the room.
 *
 * What is deliberately absent: every control that grades. No curve, no contrast, no
 * saturation, no HSL - section 2.2 puts those in phase 16 - and no route to a skin locus,
 * which stays behind the service (ADR-0032 section 4).
 */

/** What the panel needs. Everything comes from the IPC surface; nothing is derived here. */
export type BasicPanelProps = {
  /** The photograph's tone decision, or `null` when nobody has estimated it. */
  tone: ToneDto | null;
  /** The current edit, for the protected marks. */
  recipe: RecipeDto;
  /** The project's coverage, for the header. */
  status: ToneStatusDto | null;
  /** Record what the photographer set instead. */
  onOverride: (values: { exposureEv?: number; temperatureK?: number; tint?: number }) => void;
  /** Record that they looked and agree. */
  onAccept: () => void;
  /** Give the frame back to AURA. Phase 14's history action, reused. */
  onResetToAi: () => void;
};

/** The three recipe paths this panel can write, and nothing else can be added here. */
const EXPOSURE_PATH = 'global.exposure';
const TEMPERATURE_PATH = 'global.temperature';
const TINT_PATH = 'global.tint';

/** Plain words for a light. The vocabulary is the backend's; the sentences are here. */
function illuminantName(kind: string): string {
  switch (kind) {
    case 'daylight':
      return 'daylight';
    case 'tungsten':
      return 'tungsten';
    case 'fluorescent':
      return 'fluorescent tubes';
    case 'led':
      return 'LED';
    case 'flash':
      return 'flash';
    case 'candle':
      return 'candlelight';
    case 'shade':
      return 'open shade';
    case 'cloudy':
      return 'cloud';
    case 'mixed_discharge':
      return 'discharge lighting';
    case 'coloured':
      return 'coloured stage light';
    default:
      return 'a light AURA could not name';
  }
}

/** A percentage, rounded the way the sentences round it. */
function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function BasicPanel({
  tone,
  recipe,
  status,
  onOverride,
  onAccept,
  onResetToAi,
}: BasicPanelProps) {
  const protectedPaths = useMemo(
    () => new Set(recipe.userEditedFields),
    [recipe.userEditedFields],
  );

  if (!tone) {
    return (
      <section className="basic-panel" aria-label="Exposure and colour">
        <header className="basic-header">
          <h2>Exposure and colour</h2>
        </header>
        <p className="basic-unestimated" data-testid="basic-unestimated">
          AURA has not worked out the exposure and colour for this photograph yet. It is
          showing as the camera recorded it.
        </p>
      </section>
    );
  }

  const dominant =
    tone.dominantOnSubject !== null ? tone.illuminants[tone.dominantOnSubject] : undefined;

  // What the sliders show. The photographer's own number when they set one, AURA's otherwise
  // - and never a mixture: somebody who corrected only the temperature still sees AURA's
  // exposure, because they did not make a claim about it.
  const shownExposure = tone.userExposureEv ?? tone.exposureEv;
  const shownTemperature = tone.userTemperatureK ?? tone.temperatureK;
  const shownTint = tone.userTint ?? tone.tint;

  return (
    <section className="basic-panel" aria-label="Exposure and colour">
      <header className="basic-header">
        <h2>Exposure and colour</h2>
        {tone.userEdited ? (
          <p className="basic-provenance" data-testid="basic-provenance">
            Your settings. AURA had suggested {Math.round(tone.temperatureK)} K at{' '}
            {tone.exposureEv > 0 ? '+' : ''}
            {tone.exposureEv.toFixed(2)} stops.
          </p>
        ) : (
          <p className="basic-provenance" data-testid="basic-provenance">
            AURA set these. Exposure {percent(tone.exposureConf)} confident, colour{' '}
            {percent(tone.wbConf)} confident.
          </p>
        )}
        {tone.faceAnchored ? (
          <p className="basic-anchor" data-testid="basic-anchor">
            Exposed for the faces in this frame, not for its average brightness.
          </p>
        ) : (
          <p className="basic-anchor" data-testid="basic-anchor">
            No faces here, so this was exposed for the kind of photograph it is.
          </p>
        )}
      </header>

      <ol className="basic-controls">
        <li className="basic-control">
          <label htmlFor="basic-exposure">
            Exposure
            {protectedPaths.has(EXPOSURE_PATH) ? (
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
            id="basic-exposure"
            type="number"
            step="0.05"
            value={String(shownExposure)}
            onChange={(event) => onOverride({ exposureEv: Number(event.target.value) })}
          />
          <span className="basic-unit">stops</span>
        </li>

        <li className="basic-control">
          <label htmlFor="basic-temperature">
            Temperature
            {protectedPaths.has(TEMPERATURE_PATH) ? (
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
            id="basic-temperature"
            type="number"
            step="50"
            value={String(Math.round(shownTemperature))}
            onChange={(event) => onOverride({ temperatureK: Number(event.target.value) })}
          />
          <span className="basic-unit">K</span>
        </li>

        <li className="basic-control">
          <label htmlFor="basic-tint">
            Tint
            {protectedPaths.has(TINT_PATH) ? (
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
            id="basic-tint"
            type="number"
            step="1"
            value={String(Math.round(shownTint))}
            onChange={(event) => onOverride({ tint: Number(event.target.value) })}
          />
          <span className="basic-unit">green to magenta</span>
        </li>
      </ol>

      <ul className="basic-notes" aria-label="What AURA noticed about the light">
        {dominant ? (
          <li data-testid="basic-dominant">
            The people here are lit by {illuminantName(dominant.kind)}, about{' '}
            {Math.round(dominant.cctK)} K.
          </li>
        ) : null}
        {tone.mixedLight ? (
          <li className="basic-mixed" data-testid="basic-mixed-light">
            There is more than one colour of light in this frame. AURA has set the colour for
            the light on the people; the rest can be corrected separately later.
          </li>
        ) : null}
        {tone.colouredLight ? (
          <li className="basic-coloured" data-testid="basic-coloured-light">
            This is coloured stage light and AURA has kept it. It has corrected only far
            enough to keep skin believable.
          </li>
        ) : null}
        {tone.backlit ? (
          <li data-testid="basic-backlit">
            Backlit. AURA exposed for the subject and let the background go bright on purpose.
          </li>
        ) : null}
        {tone.constrainedIdentities === 0 ? (
          <li className="basic-unconstrained" data-testid="basic-unconstrained">
            AURA has not seen enough well-lit photographs of these people yet to know what
            their skin should look like, so the colour here came from the light alone.
          </li>
        ) : null}
      </ul>

      <ol className="basic-reasons" aria-label="Why">
        {tone.reasons.map((reason) => (
          <li key={reason.code}>
            <span className="basic-reason-text">{reason.text}</span>
          </li>
        ))}
      </ol>

      {tone.alternatives.length > 0 ? (
        <details className="basic-alternatives">
          <summary>Other answers AURA considered</summary>
          <ol>
            {tone.alternatives.map((alternative) => (
              <li key={`${alternative.temperatureK}:${alternative.tint}`}>
                <button
                  type="button"
                  onClick={() =>
                    onOverride({
                      temperatureK: alternative.temperatureK,
                      tint: alternative.tint,
                    })
                  }
                >
                  {Math.round(alternative.temperatureK)} K, tint {Math.round(alternative.tint)}
                </button>
              </li>
            ))}
          </ol>
        </details>
      ) : null}

      <div className="basic-actions">
        <button
          type="button"
          disabled={tone.reviewed || tone.userEdited}
          onClick={onAccept}
          data-testid="basic-accept"
        >
          Looks right
        </button>
        <button
          type="button"
          disabled={!tone.userEdited}
          onClick={onResetToAi}
          data-testid="basic-reset-ai"
        >
          Reset to AI suggestion
        </button>
      </div>

      {status ? (
        <footer className="basic-footer" data-testid="basic-coverage">
          <p>
            {percent(status.coverage)} of this wedding has an exposure decision, and{' '}
            {percent(status.faceAnchored)} of those were exposed for a face.
          </p>
          {status.skinConstrained < 0.5 ? (
            <p className="basic-caveat">
              Only {percent(status.skinConstrained)} of them had a skin reference to check the
              colour against.
            </p>
          ) : null}
        </footer>
      ) : null}
    </section>
  );
}
