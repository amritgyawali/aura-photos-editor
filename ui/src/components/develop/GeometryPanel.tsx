import { useMemo } from 'react';

import type {
  CropVariantDto,
  GeometryPlanDto,
  GeometryReasonDto,
  GeometryStatusDto,
} from '../../ipc/types';

/**
 * PHASE-23. The framing decision: what the lens did, how far the frame was tilted, which
 * rectangle is delivered, and what a crop was not allowed to cut.
 *
 * Section 4's SFE deliverable: "Crop/straighten UI with AI proposal and revert". Five rules, and
 * the first is the one this phase exists to get right:
 *
 * 1. **The revert is a button, always, on every plan.** Section 13: "original framing is always
 *    one click away." It is rendered whether or not the frame was cropped, whether or not a
 *    photographer has edited it, and it is never behind a menu - because the moment somebody
 *    needs it is the moment they have just seen an automated crop they dislike.
 * 2. **What was left alone is shown as prominently as what was done.** Most of this phase's
 *    vocabulary is refusals and most frames keep their framing, so a panel that only listed
 *    changes would render an empty box on the eight frames out of ten that are working correctly.
 *    Phase 20's rule, inherited a third time.
 * 3. **What a crop may not cut is drawn, not described.** `renderProtected` lists the regions
 *    with their rectangles so the develop view can overlay them. A safety promise a photographer
 *    cannot see is a safety promise they have to take on trust.
 * 4. **A count is shown with its denominator.** `facesChecked` beside `facesCut`: over a wedding
 *    whose detector found no faces, "no faces were cut" is arithmetic, and this panel says so in
 *    words rather than printing a reassuring zero. Phase 21's rule.
 * 5. **A refused variant is shown with the reason it was refused.** "Why is there no square crop
 *    of this photograph" is a question the panel answers, because the store keeps refusals as
 *    rows rather than as absences.
 *
 * The component is pure. It receives a status, a plan and three callbacks; it fetches nothing and
 * renders no pixels of its own. The preview is the develop view's existing render at the
 * delivered rectangle, which is why there is no image prop here.
 */
export type GeometryPanelProps = {
  /** What the project pass covered and did. */
  status: GeometryStatusDto | null;
  /** The selected photograph's plan, or `null` when it has not been planned. */
  plan: GeometryPlanDto | null;
  /** Deliver one of this photograph's own variants. */
  onChooseVariant?: (ordinal: number) => void;
  /** Give the photograph back the framing it was shot at. */
  onRevert?: () => void;
  /** Record that the photographer has looked at this plan and agrees. */
  onAccept?: () => void;
};

/** How an aspect reads to somebody who has not read the phase document. */
const ASPECT_LABEL: Record<string, string> = {
  original: 'As shot',
  '4:5': '4:5 portrait',
  '5:4': '5:4 landscape',
  '1:1': 'Square',
  '16:9': '16:9 wide',
};

/** How a lens source reads. */
const SOURCE_LABEL: Record<string, string> = {
  embedded: 'the file’s own correction data',
  database: 'AURA’s lens profile',
  estimated: 'the straight edges in this photograph',
  none: 'nothing - this lens was left alone',
};

function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function degrees(value: number): string {
  return `${value.toFixed(2)}°`;
}

export function GeometryPanel({
  status,
  plan,
  onChooseVariant,
  onRevert,
  onAccept,
}: GeometryPanelProps) {
  const [changes, restraints] = useMemo(() => {
    const done: GeometryReasonDto[] = [];
    const held: GeometryReasonDto[] = [];
    for (const reason of plan?.reasons ?? []) {
      (reason.refusal ? held : done).push(reason);
    }
    return [done, held];
  }, [plan]);

  return (
    <section className="geometry-panel" aria-label="Framing">
      <header className="geometry-panel__header">
        <h2>Framing</h2>
        <p className="geometry-panel__subtitle">
          Lens corrections where a profile exists, horizons levelled where the tilt was a mistake,
          and a tighter crop only where it clearly helps.
        </p>
      </header>

      {status ? (
        renderStatus(status)
      ) : (
        <p className="geometry-panel__empty">No pass has run yet.</p>
      )}

      {plan ? (
        <>
          {renderLens(plan)}
          {renderRotation(plan)}
          {renderVariants(plan, onChooseVariant)}
          {renderProtected(plan)}
          {renderReasons(changes, restraints)}
          {renderFooter(plan, onRevert, onAccept)}
        </>
      ) : (
        <p className="geometry-panel__empty">This photograph has not been looked at yet.</p>
      )}
    </section>
  );
}

function renderStatus(status: GeometryStatusDto) {
  return (
    <div className="geometry-panel__status">
      <dl>
        <div>
          <dt>Looked at</dt>
          <dd>
            {status.planned} of {status.photos} ({percent(status.coverage)})
          </dd>
        </div>
        <div>
          {/* Rule 2, at the project level: the number that should be large is the one that
              means AURA left the photographs alone. */}
          <dt>Kept as shot</dt>
          <dd data-testid="geometry-kept-original">
            {status.keptOriginal} ({percent(status.conservatism)})
          </dd>
        </div>
        <div>
          <dt>Straightened</dt>
          <dd>
            {status.straightened}
            {status.straightened > 0 ? ` (${degrees(status.meanRotationDeg)} on average)` : ''}
          </dd>
        </div>
        <div>
          <dt>Cropped</dt>
          <dd>{status.cropped}</dd>
        </div>
      </dl>

      {/* Rule 4. The zero and the denominator behind it, in that order. */}
      <p className="geometry-panel__safety" data-testid="geometry-safety">
        {status.facesChecked > 0
          ? `${status.facesCut} of ${status.facesChecked} protected regions were cut by a delivered crop.`
          : 'No faces were found in this project, so nothing was protected and there is nothing to conclude from a crop that cut none.'}
      </p>

      {status.cropRefusals.length > 0 ? (
        <div className="geometry-panel__refusals">
          {/* Rule 2. "AURA cropped almost nothing" has several causes. */}
          <h3>Why crops were held back</h3>
          <ul>
            {status.cropRefusals.map((refusal) => (
              <li key={refusal.code}>
                <span className="geometry-panel__refusal-count">{refusal.count}</span>
                <span>{refusal.text}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {status.lensesMissing.length > 0 ? (
        <p className="geometry-panel__missing-lenses" data-testid="geometry-missing-lenses">
          {/* The one row on this panel a studio can act on. */}
          AURA has no profile for {status.lensesMissing.map((miss) => miss.lens).join(', ')}, so
          distortion on those photographs is estimated from the frame or left alone.
        </p>
      ) : null}
    </div>
  );
}

function renderLens(plan: GeometryPlanDto) {
  const corrected = plan.lensDistortion || plan.lensCa || plan.lensVignette > 0;
  return (
    <div className="geometry-panel__lens">
      <h3>Lens</h3>
      <p>
        {corrected
          ? `Corrected from ${SOURCE_LABEL[plan.lensSource] ?? plan.lensSource}.`
          : 'Nothing was corrected on this photograph.'}
      </p>
      {corrected ? (
        <ul>
          <li>Distortion {plan.lensDistortion ? 'corrected' : 'left alone'}</li>
          <li>
            Vignetting {plan.lensVignette > 0 ? `corrected ${plan.lensVignette}%` : 'left alone'}
          </li>
          <li>Colour fringing {plan.lensCa ? 'corrected' : 'left alone'}</li>
        </ul>
      ) : null}
      {corrected && !plan.lensMeasured ? (
        <p className="geometry-panel__reference-profile" data-testid="geometry-reference-profile">
          {/* Said out loud rather than implied: nobody has measured this lens. */}
          This profile is a reference model for the lens class rather than a measurement of this
          lens.
        </p>
      ) : null}
    </div>
  );
}

function renderRotation(plan: GeometryPlanDto) {
  return (
    <div className="geometry-panel__rotation">
      <h3>Level</h3>
      {plan.rotateDeg === 0 ? (
        // Rule 2: the confidence is shown even when nothing was rotated, because "the horizon
        // looks off and AURA was not sure enough to move it" is the commonest question here.
        <p data-testid="geometry-rotation">
          Left as shot. Horizon confidence {percent(plan.rotateConf)}.
        </p>
      ) : (
        <p data-testid="geometry-rotation">
          Straightened by {degrees(plan.rotateDeg)}, at {percent(plan.rotateConf)} confidence.
        </p>
      )}
      {plan.keystone ? (
        <p className="geometry-panel__keystone">
          Verticals corrected. The frame was stretched by{' '}
          {plan.keystone.stretch.toFixed(3)}×, within the 1.12× limit.
        </p>
      ) : null}
    </div>
  );
}

function renderVariants(
  plan: GeometryPlanDto,
  onChooseVariant?: GeometryPanelProps['onChooseVariant'],
) {
  return (
    <div className="geometry-panel__variants">
      <h3>Crops</h3>
      <ul>
        {plan.crops.map((variant) => (
          <li
            key={variant.ordinal}
            className={
              variant.ordinal === plan.primaryCrop
                ? 'geometry-panel__variant geometry-panel__variant--delivered'
                : 'geometry-panel__variant'
            }
          >
            <button
              type="button"
              disabled={!variant.safe}
              aria-pressed={variant.ordinal === plan.primaryCrop}
              onClick={() => onChooseVariant?.(variant.ordinal)}
            >
              {ASPECT_LABEL[variant.aspect] ?? variant.aspect}
            </button>
            <span className="geometry-panel__variant-keeps">{describeVariant(variant)}</span>
            {/* Rule 5. */}
            {variant.safe ? null : (
              <span className="geometry-panel__variant-refusal" data-testid="geometry-refused">
                {variant.refusal ?? 'refused by the safety filter'}
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

function describeVariant(variant: CropVariantDto): string {
  if (variant.longEdgeFraction === null || variant.longEdgeFraction === undefined) {
    return variant.purpose;
  }
  return `${variant.purpose} · keeps ${percent(variant.longEdgeFraction)} of the long edge`;
}

function renderProtected(plan: GeometryPlanDto) {
  // Rule 3, and rule 4 again at the photograph level.
  if (plan.safety.considered === 0) {
    return (
      <p className="geometry-panel__protected-empty" data-testid="geometry-protected">
        Nothing was found in this photograph to protect, so the crop was chosen on framing alone.
      </p>
    );
  }
  return (
    <div className="geometry-panel__protected" data-testid="geometry-protected">
      <h3>Kept inside the frame</h3>
      <ul>
        {plan.safety.regions.map((region, index) => (
          <li key={`${region.kind}-${index}`}>{region.text}</li>
        ))}
      </ul>
      <p>
        {plan.safety.considered} checked, {plan.safety.atRisk} at risk. The delivered crop keeps{' '}
        {percent(plan.safety.longEdgeFraction)} of the long edge.
      </p>
    </div>
  );
}

function renderReasons(changes: GeometryReasonDto[], restraints: GeometryReasonDto[]) {
  return (
    <div className="geometry-panel__reasons">
      {changes.length > 0 ? (
        <>
          <h3>What AURA changed</h3>
          <ul>
            {changes.map((reason) => (
              <li key={reason.code}>{reason.text}</li>
            ))}
          </ul>
        </>
      ) : null}
      {/* Rule 2: this list is usually the longer one, and it is never hidden. */}
      {restraints.length > 0 ? (
        <>
          <h3>What AURA left alone</h3>
          <ul data-testid="geometry-restraints">
            {restraints.map((reason) => (
              <li
                key={reason.code}
                className={reason.safety ? 'geometry-panel__reason--safety' : undefined}
              >
                {reason.text}
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}

function renderFooter(
  plan: GeometryPlanDto,
  onRevert?: GeometryPanelProps['onRevert'],
  onAccept?: GeometryPanelProps['onAccept'],
) {
  return (
    <footer className="geometry-panel__footer">
      <p className="geometry-panel__confidence">
        Confidence {percent(plan.confidence)}
        {plan.userEdited ? ' · you framed this one' : ''}
        {plan.reviewed ? ' · reviewed' : ''}
      </p>
      {/* Rule 1: always rendered, never conditional, never behind a menu. */}
      <button
        type="button"
        className="geometry-panel__revert"
        data-testid="geometry-revert"
        onClick={() => onRevert?.()}
      >
        Back to the original framing
      </button>
      <button type="button" className="geometry-panel__accept" onClick={() => onAccept?.()}>
        Looks right
      </button>
    </footer>
  );
}
