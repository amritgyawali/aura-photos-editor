import { useMemo } from 'react';

import type {
  RestoreFaceDto,
  RestorePlanDto,
  RestoreReasonDto,
  RestoreStatusDto,
} from '../../ipc/types';

/**
 * PHASE-22. The restoration decision, with the tier a photographer may change, what the
 * self-check measured, and the faces AURA declined to change.
 *
 * Section 9's SFE deliverable: "Restore panel with tiers, per-image override, 100 % zoom
 * preview". Five rules, and the second is the one this phase exists to get right:
 *
 * 1. **What was left alone is shown as prominently as what was done.** Twenty of the thirty
 *    reason codes in this phase are refusals, and the commonest question a photographer has is
 *    why a particular frame was *not* sharpened. A panel that only listed repairs would make a
 *    careful product look like a careless one - phase 20's rule, inherited twice.
 * 2. **A declined face is a headline, not a footnote.** `renderFace` gives a face skipped for
 *    identity drift its own class, its own wording and the measured distance. This is the single
 *    thing this phase most needs to be able to say out loud: AURA stopped short of changing what
 *    somebody looks like.
 * 3. **There is a tier and there is no slider.** A photographer chooses which of four; how far
 *    each goes is a product decision bounded by the contract. There is no prop on this component
 *    that could carry an amount, and adding one would make the guarantees in
 *    `docs/restoration.md` a description of the defaults rather than a promise.
 * 4. **A measurement is shown with its sample count, or not at all.** `measuredOn` decides
 *    whether the two ratios are printed to three decimal places, because a ratio over eleven
 *    samples is arithmetic rather than evidence - phase 21's rule.
 * 5. **An unmeasured camera is named.** It is the one thing on this panel a photographer can act
 *    on: a body on that list is a body capped at `standard` until somebody photographs a
 *    reference for it.
 *
 * The component is pure. It receives a status, a plan and two callbacks; it fetches nothing and
 * renders no pixels of its own. The 100 % preview is the develop view's existing render at the
 * region the photographer is looking at, asked for twice - once with restoration and once
 * without - which is why there is no image prop here.
 */
export type RestorePanelProps = {
  /** What the project pass covered and did. */
  status: RestoreStatusDto | null;
  /** The selected photograph's plan, or `null` when it has not been planned. */
  plan: RestorePlanDto | null;
  /** Record a tier or a switch. */
  onOverride?: (input: {
    denoise?: string;
    sharpen?: boolean;
    faceRecovery?: boolean;
  }) => void;
  /** Record that the photographer has looked at this plan and agrees. */
  onAccept?: () => void;
};

/** The four tiers, in the order the contract declares them. */
const TIERS = ['off', 'light', 'standard', 'strong'] as const;

/** How a tier reads to somebody who has not read the phase document. */
const TIER_LABEL: Record<string, string> = {
  off: 'None',
  light: 'Light',
  standard: 'Standard',
  strong: 'Strong',
};

/** The four groups the reasons are shown in, in the order the panel lists them. */
const SUBJECTS = [
  { key: 'denoise', title: 'Noise' },
  { key: 'sharpen', title: 'Sharpening' },
  { key: 'face_recovery', title: 'Faces' },
  { key: 'plan', title: 'This photograph' },
] as const;

/**
 * The sample count below which a ratio is shown as a word rather than a number.
 *
 * Phase 21's rule and phase 21's threshold. A ratio measured over a handful of pixels is
 * arithmetic, and printing it to three decimal places invites somebody to act on it.
 */
const ENOUGH_SAMPLES = 2000;

function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function RestorePanel({ status, plan, onOverride, onAccept }: RestorePanelProps) {
  const grouped = useMemo(() => {
    const out = new Map<string, RestoreReasonDto[]>();
    for (const reason of plan?.reasons ?? []) {
      const list = out.get(reason.subject) ?? [];
      list.push(reason);
      out.set(reason.subject, list);
    }
    return out;
  }, [plan]);

  const declined = useMemo(
    () => (plan?.faces ?? []).filter((face) => face.skippedBecause === 'restore_identity_drift_skipped'),
    [plan],
  );

  return (
    <section className="restore-panel" aria-label="Restoration">
      <header className="restore-panel__header">
        <h2>Restoration</h2>
        <p className="restore-panel__subtitle">
          Noise removed where the sensor says there is noise, edges recovered where recovering
          them helps, and faces left as themselves.
        </p>
      </header>

      {status ? renderStatus(status) : <p className="restore-panel__empty">No pass has run yet.</p>}

      {plan ? (
        <>
          {renderTiers(plan, onOverride)}
          {renderSwitches(plan, onOverride)}
          {renderMeasurements(plan)}
          {declined.length > 0 ? renderDeclined(declined) : null}
          {renderFaces(plan.faces)}
          {renderReasons(grouped)}
          {renderFooter(plan, onAccept)}
        </>
      ) : (
        <p className="restore-panel__empty">This photograph has not been looked at yet.</p>
      )}
    </section>
  );
}

function renderStatus(status: RestoreStatusDto) {
  const tiers = status.tierNames.map((name, index) => ({
    name,
    count: status.tiers[index] ?? 0,
  }));
  return (
    <div className="restore-panel__status">
      <dl>
        <div>
          <dt>Looked at</dt>
          <dd>
            {status.planned} of {status.photos} ({percent(status.coverage)})
          </dd>
        </div>
        <div>
          <dt>Cleaned up</dt>
          <dd>{status.actedOn}</dd>
        </div>
        <div>
          <dt>Sharpened</dt>
          <dd>{status.sharpened}</dd>
        </div>
        <div>
          {/* Rule 2, at the project level. */}
          <dt>Faces left as themselves</dt>
          <dd data-testid="restore-declined-total">{status.facesSkippedIdentity}</dd>
        </div>
      </dl>

      <ul className="restore-panel__tiers">
        {tiers.map((tier) => (
          <li key={tier.name}>
            <span className="restore-panel__tier-name">{TIER_LABEL[tier.name] ?? tier.name}</span>
            <span className="restore-panel__tier-count">{tier.count}</span>
          </li>
        ))}
      </ul>

      {status.sharpenRefusals.length > 0 ? (
        <div className="restore-panel__refusals">
          {/* Rule 1. "AURA sharpened nothing in this wedding" has six causes. */}
          <h3>Why sharpening was held back</h3>
          <ul>
            {status.sharpenRefusals.map((refusal) => (
              <li key={refusal.code}>
                <span className="restore-panel__refusal-count">{refusal.count}</span>
                <span>{refusal.text}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {status.unmeasuredCameras.length > 0 ? (
        <p className="restore-panel__unmeasured" data-testid="restore-unmeasured">
          {/* Rule 5. The one thing on this panel a photographer can act on. */}
          AURA has not measured the noise of {status.unmeasuredCameras.join(', ')}, so it holds
          back from its strongest setting on those photographs.
        </p>
      ) : null}
    </div>
  );
}

function renderTiers(
  plan: RestorePlanDto,
  onOverride?: RestorePanelProps['onOverride'],
) {
  return (
    <fieldset className="restore-panel__tier-choice">
      <legend>Noise reduction</legend>
      {/* Rule 3: four buttons, and no slider anywhere on this component. */}
      {TIERS.map((tier) => (
        <button
          key={tier}
          type="button"
          className={
            plan.denoise === tier
              ? 'restore-panel__tier restore-panel__tier--on'
              : 'restore-panel__tier'
          }
          aria-pressed={plan.denoise === tier}
          onClick={() => onOverride?.({ denoise: tier })}
        >
          {TIER_LABEL[tier]}
        </button>
      ))}
      {plan.denoiseCamera ? (
        <p className="restore-panel__conditioning">
          Measured against {plan.denoiseCamera}
          {plan.denoiseMeasured ? '' : ', from its published specification rather than a measurement'}
          .
        </p>
      ) : null}
    </fieldset>
  );
}

function renderSwitches(
  plan: RestorePlanDto,
  onOverride?: RestorePanelProps['onOverride'],
) {
  const sharpening = plan.sharpenAmount > 0;
  const recovering = plan.faceRecovery > 0;
  return (
    <fieldset className="restore-panel__switches">
      <legend>What else may run</legend>
      <label>
        <input
          type="checkbox"
          checked={sharpening}
          onChange={(event) => onOverride?.({ sharpen: event.target.checked })}
        />
        <span>Recover soft edges</span>
      </label>
      <label>
        {/* Separate from sharpening: a photographer can want a frame sharpened and want no model
            near anybody's face. */}
        <input
          type="checkbox"
          checked={recovering}
          onChange={(event) => onOverride?.({ faceRecovery: event.target.checked })}
        />
        <span>Recover soft faces</span>
      </label>
    </fieldset>
  );
}

function renderMeasurements(plan: RestorePlanDto) {
  const report = plan.selfcheck;
  if (!report) {
    return (
      <p className="restore-panel__empty">
        Nothing was changed in this photograph, so there was nothing to measure.
      </p>
    );
  }
  // Rule 4: the sample count decides whether the ratios are numbers or a word.
  const enough = report.measuredOn >= ENOUGH_SAMPLES;
  return (
    <div className="restore-panel__measurements">
      <h3>What AURA checked afterwards</h3>
      <dl>
        <div>
          <dt>Texture kept</dt>
          <dd data-testid="restore-texture">
            {enough ? percent(report.textureRetention) : 'too small an area to measure'}
          </dd>
        </div>
        <div>
          <dt>Edge outlines</dt>
          <dd data-testid="restore-ringing">
            {enough ? report.ringing.toFixed(4) : 'too small an area to measure'}
          </dd>
        </div>
      </dl>
      {report.denoiseReduced ? (
        <p>AURA cleaned this photograph more gently, so its fabric kept its texture.</p>
      ) : null}
      {report.sharpenReduced ? (
        <p>AURA sharpened this photograph more gently, or not at all, to keep its edges clean.</p>
      ) : null}
    </div>
  );
}

function renderDeclined(faces: RestoreFaceDto[]) {
  return (
    <div className="restore-panel__declined" data-testid="restore-declined">
      {/* Rule 2. Its own block, above the ordinary face list, with its own wording. */}
      <h3>Left as themselves</h3>
      <p>
        AURA stopped short of recovering detail in {faces.length}{' '}
        {faces.length === 1 ? 'face' : 'faces'}, because going further would have started to change
        what {faces.length === 1 ? 'that person looks' : 'those people look'} like.
      </p>
      <ul>
        {faces.map((face, index) => (
          <li key={`${face.identityId ?? 'unknown'}-${index}`}>
            measured {face.identityDrift.toFixed(4)} after {face.resolves}{' '}
            {face.resolves === 1 ? 'attempt' : 'attempts'} at easing off
          </li>
        ))}
      </ul>
    </div>
  );
}

function renderFaces(faces: RestoreFaceDto[]) {
  if (faces.length === 0) {
    return null;
  }
  return (
    <div className="restore-panel__faces">
      <h3>Faces</h3>
      <ul>
        {faces.map((face, index) => (
          <li
            key={`${face.identityId ?? 'unknown'}-${index}`}
            className={face.skipped ? 'restore-panel__face restore-panel__face--left' : 'restore-panel__face'}
          >
            {renderFace(face)}
          </li>
        ))}
      </ul>
    </div>
  );
}

function renderFace(face: RestoreFaceDto) {
  if (!face.skipped) {
    return (
      <span>
        recovered at {percent(face.strength)}, unchanged as a person by{' '}
        {face.identityDrift.toFixed(4)}
      </span>
    );
  }
  if (face.skippedBecause === 'restore_identity_drift_skipped') {
    return <span>left alone to keep this person looking like themselves</span>;
  }
  if (face.skippedBecause === 'restore_face_too_blurred') {
    return <span>too blurred to recover; anything put back would be invented</span>;
  }
  if (face.skippedBecause === 'restore_face_sharp_enough') {
    return <span>sharp already</span>;
  }
  return <span>left alone</span>;
}

function renderReasons(grouped: Map<string, RestoreReasonDto[]>) {
  return (
    <div className="restore-panel__reasons">
      {SUBJECTS.map((subject) => {
        const reasons = grouped.get(subject.key) ?? [];
        if (reasons.length === 0) {
          return null;
        }
        return (
          <div key={subject.key} className="restore-panel__reason-group">
            <h3>{subject.title}</h3>
            <ul>
              {reasons.map((reason) => (
                <li
                  key={reason.code}
                  className={
                    reason.restraint
                      ? 'restore-panel__reason restore-panel__reason--restraint'
                      : 'restore-panel__reason'
                  }
                >
                  {reason.text}
                </li>
              ))}
            </ul>
          </div>
        );
      })}
    </div>
  );
}

function renderFooter(plan: RestorePlanDto, onAccept?: () => void) {
  return (
    <footer className="restore-panel__footer">
      <span className="restore-panel__confidence">
        Confidence {percent(plan.confidence)}
      </span>
      {plan.userEdited ? <span className="restore-panel__edited">You changed this</span> : null}
      {plan.reviewed ? null : (
        <button type="button" onClick={() => onAccept?.()}>
          Looks right
        </button>
      )}
    </footer>
  );
}
