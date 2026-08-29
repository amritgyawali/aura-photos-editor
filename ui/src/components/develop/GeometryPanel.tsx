import { useMemo } from 'react';

import type { CropVariantDto, GeometryPlanDto, GeometryStatusDto } from '../../ipc/types';

/**
 * PHASE-23. Lens corrections, straightening and crop, with the AI proposal, the aspect
 * switcher and revert.
 *
 * Section 9's SFE deliverable: "geometry panel with proposal preview, aspect switcher, revert,
 * manual crop". Five rules, and the first is what makes this panel different from every other
 * one in the product:
 *
 * 1. **Most of what it has to say is a refusal.** Section 10.1 asks that seventy per cent of
 *    frames keep their framing, so on most photographs the only thing to explain is why
 *    nothing happened. Eleven of the twenty-four reason codes describe restraint, and they are
 *    rendered *first* and in their own list rather than buried under the ones that acted - a
 *    panel that reads as empty on seven frames out of ten is a panel a photographer stops
 *    opening.
 * 2. **"As shot" is a crop, and it is always first.** `crops[0]` is the frame as it was
 *    taken, on every plan, guaranteed by the contract rather than by this component. Reverting
 *    is selecting it, which is why there is no separate revert path and no way for the button
 *    to be missing.
 * 3. **A safety check nobody ran is not a safety check that passed.** `facesChecked` is a
 *    count, and when it is zero the panel says AURA found no faces to protect rather than
 *    showing a tick. `handsChecked` is zero on every photograph in this build and the panel
 *    says that too.
 * 4. **A fabricated lens profile says so.** `lensSynthetic` reaches the panel because a
 *    photographer told a lens was profiled when it was invented has been misled about their
 *    own photographs.
 * 5. **Nothing here fills a corner.** A keystone opens two and a rotation opens four; they are
 *    cropped away. There is no prop or handler on this surface that could carry a fill.
 */

/** What the panel needs. The caller fetches both; nothing is derived from a service here. */
export type GeometryPanelProps = {
  /** The project's coverage, or `null` while it is loading. */
  status: GeometryStatusDto | null;
  /** The open photograph's plan, or `null` when nobody has planned it. */
  plan: GeometryPlanDto | null;
  /** Choose which crop is delivered. Index zero is the frame as shot. */
  onSelectCrop: (index: number) => void;
  /** Set the framing by hand. Records `userEdited`. */
  onSetFraming: (rect: { x: number; y: number; w: number; h: number }, rotateDeg: number) => void;
  /** Record that the photographer looked at this plan and agrees. */
  onAccept: () => void;
  /** Show the frame as it was before the geometry. */
  onCompare: (showing: boolean) => void;
  /** True while the before/after comparison is held. */
  comparing: boolean;
};

/** Where a lens correction's numbers came from, in the product's own words. */
function lensSentence(plan: GeometryPlanDto): string {
  switch (plan.lensSource) {
    case 'embedded':
      return "Corrected with the camera's own lens data.";
    case 'profile':
      return plan.lensSynthetic
        ? `Corrected with the bundled profile for ${plan.lensProfile ?? 'this lens'} - which AURA has not measured itself.`
        : `Corrected with a measured profile for ${plan.lensProfile ?? 'this lens'}.`;
    case 'estimated':
      return 'No profile for this lens, so the distortion was estimated from straight lines in the frame. Fringing was left alone.';
    default:
      return plan.lensId
        ? `No correction profile for ${plan.lensId}, so the optics were left as they are.`
        : 'No lens correction was applied.';
  }
}

/** A percentage, rounded the way the sentences round it. */
function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

/** Degrees, signed, to one decimal. */
function degrees(value: number): string {
  const sign = value > 0 ? '+' : '';
  return `${sign}${value.toFixed(1)}°`;
}

/** How much of the frame a crop keeps, as a percentage of its area. */
function keeps(variant: CropVariantDto): string {
  return percent(Math.max(0, Math.min(1, variant.rect.w * variant.rect.h)));
}

export function GeometryPanel({
  status,
  plan,
  onSelectCrop,
  onSetFraming,
  onAccept,
  onCompare,
  comparing,
}: GeometryPanelProps) {
  const restraints = useMemo(
    () => (plan?.reasons ?? []).filter((reason) => reason.restraint),
    [plan],
  );
  const actions = useMemo(
    () => (plan?.reasons ?? []).filter((reason) => !reason.restraint),
    [plan],
  );

  if (!plan) {
    return (
      <section className="geometry" aria-label="Geometry">
        <header>
          <h2>Geometry</h2>
        </header>
        <p data-testid="geometry-empty">
          AURA has not looked at this photograph&apos;s framing yet.
        </p>
      </section>
    );
  }

  const caveats: string[] = [];
  if (plan.safety.facesChecked === 0) {
    caveats.push(
      'AURA found no faces in this photograph, so nothing was checked against one. That is not the same as a crop being proven safe.',
    );
  }
  if (plan.safety.handsChecked === 0) {
    caveats.push(
      'AURA cannot see hands yet, so no crop in this build has been checked against a pair.',
    );
  }
  if (plan.lensSynthetic) {
    caveats.push(
      'The lens profile AURA used was not measured from this lens. It is close, and it is not exact.',
    );
  }
  const refusedTotal = plan.safety.refused.reduce((sum, count) => sum + count, 0);
  // The contract guarantees `crops[0]` exists and that `primaryCrop` addresses one of them.
  // The fallback is what a row written by a newer build and read by this one gets, and it is
  // the frame as shot rather than nothing - a panel with no rectangle has nothing to show.
  const delivered: CropVariantDto =
    plan.crops[plan.primaryCrop] ?? plan.crops[0] ?? {
      purpose: 'original',
      title: 'As shot',
      aspect: 'original',
      rect: { x: 0, y: 0, w: 1, h: 1 },
      score: 0,
      safe: true,
    };

  return (
    <section className="geometry" aria-label="Geometry">
      <header>
        <h2>Geometry</h2>
        <p data-testid="geometry-summary">
          {plan.keptOriginal
            ? 'Delivered as shot.'
            : `Re-framed: keeping ${keeps(delivered)} of the frame.`}{' '}
          {plan.rotateDeg === 0
            ? 'Not rotated.'
            : `Levelled by ${degrees(plan.rotateDeg)}.`}
        </p>
        <button
          type="button"
          onMouseDown={() => onCompare(true)}
          onMouseUp={() => onCompare(false)}
          onMouseLeave={() => onCompare(false)}
          data-testid="geometry-compare"
          aria-pressed={comparing}
        >
          Hold to see it as shot
        </button>
      </header>

      {caveats.length > 0 && (
        <ul className="geometry-caveats" data-testid="geometry-caveats">
          {caveats.map((caveat) => (
            <li key={caveat}>{caveat}</li>
          ))}
        </ul>
      )}

      <section className="geometry-lens" aria-label="Lens">
        <h3>Lens</h3>
        <p data-testid="geometry-lens">{lensSentence(plan)}</p>
        {plan.vignette > 0 && (
          <p data-testid="geometry-vignette">
            Corner darkening evened out by {percent(plan.vignette)}.
          </p>
        )}
      </section>

      {(plan.keystoneVertical ?? 0) !== 0 && (
        <section className="geometry-keystone" aria-label="Perspective">
          <h3>Perspective</h3>
          <p data-testid="geometry-keystone">
            Squared up from {plan.keystoneVerticals} vertical lines, stretching the frame by{' '}
            {percent((plan.keystoneStretch ?? 1) - 1)}.
          </p>
        </section>
      )}

      <section className="geometry-crops" aria-label="Crops">
        <h3>Framing</h3>
        <ol className="geometry-variants">
          {plan.crops.map((variant, index) => (
            <li
              key={variant.purpose}
              className={index === plan.primaryCrop ? 'geometry-variant selected' : 'geometry-variant'}
            >
              <button
                type="button"
                onClick={() => onSelectCrop(index)}
                aria-pressed={index === plan.primaryCrop}
                data-testid={`geometry-variant-${variant.purpose}`}
              >
                <span className="geometry-variant-title">{variant.title}</span>
                <span className="geometry-variant-aspect">{variant.aspect}</span>
                <span className="geometry-variant-keeps">{keeps(variant)} of the frame</span>
              </button>
            </li>
          ))}
        </ol>
        <button
          type="button"
          onClick={() => onSetFraming({ x: 0, y: 0, w: 1, h: 1 }, 0)}
          data-testid="geometry-revert"
        >
          Back to how it was shot
        </button>
      </section>

      {restraints.length > 0 && (
        <section className="geometry-restraint" aria-label="What AURA left alone">
          <h3>What AURA left alone</h3>
          <ul data-testid="geometry-restraints">
            {restraints.map((reason) => (
              <li key={reason.code}>{reason.text}</li>
            ))}
          </ul>
        </section>
      )}

      {actions.length > 0 && (
        <section className="geometry-actions" aria-label="What AURA changed">
          <h3>What AURA changed</h3>
          <ul data-testid="geometry-actions">
            {actions.map((reason) => (
              <li key={reason.code}>{reason.text}</li>
            ))}
          </ul>
        </section>
      )}

      {refusedTotal > 0 && (
        <p className="geometry-refusals" data-testid="geometry-refusals">
          AURA tried {refusedTotal} other framing{refusedTotal === 1 ? '' : 's'} and rejected every
          one of them:{' '}
          {plan.safety.refusedNames
            .map((name, index) => ({ name, count: plan.safety.refused[index] ?? 0 }))
            .filter((entry) => entry.count > 0)
            .map((entry) => `${entry.count} ${refusalWords(entry.name)}`)
            .join(', ')}
          .
        </p>
      )}

      <footer className="geometry-footer">
        {plan.userEdited ? (
          <p data-testid="geometry-edited">
            You framed this one. AURA will not change it.
          </p>
        ) : (
          <button type="button" onClick={onAccept} data-testid="geometry-accept">
            Looks right
          </button>
        )}
        {status && (
          <p data-testid="geometry-status">
            {percent(status.keptOriginal)} of this wedding is delivered exactly as it was shot.
            {status.missingProfiles.length > 0 &&
              ` No lens profile for ${status.missingProfiles.slice(0, 2).join(' or ')}.`}
          </p>
        )}
      </footer>
    </section>
  );
}

/** A refusal slug as the words a photographer would use. */
function refusalWords(slug: string): string {
  const names: Record<string, string> = {
    crop_cuts_face: 'that cut a face',
    crop_cuts_hands: 'that cut the couple&apos;s hands',
    crop_too_small: 'that threw away too much resolution',
    crop_loses_content: 'that removed part of what the frame is about',
  };
  return names[slug] ?? slug.replace(/_/g, ' ');
}
