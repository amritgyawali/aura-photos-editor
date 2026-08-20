import { useMemo } from 'react';

import type {
  ProtectedFeatureDto,
  RetouchPlanDto,
  RetouchStatusDto,
} from '../../ipc/types';

/**
 * PHASE-20. Skin retouching, with the preset, the per-person strength, the protected features
 * and the texture measurement.
 *
 * Section 9's SFE deliverable: "retouch panel, presets, per-identity strength, protected-feature
 * list with visual markers". Five rules, and the first two are what this component exists for:
 *
 * 1. **What was left alone is shown as prominently as what was done.** Half the reason codes in
 *    this phase are withdrawals, and the most common question a photographer has about a
 *    retoucher is why a particular mark is still there. A panel that only listed removals would
 *    make a careful product look like a careless one.
 * 2. **A protected feature is a list item with its evidence, not a hidden rule.** "AURA is
 *    keeping this because it was in the same place on her face for six hours" is a sentence
 *    somebody can agree or disagree with; a silently skipped mark is not.
 * 3. **A tattoo has no control.** `absolute` features are rendered without a clear button rather
 *    than with a disabled one - a disabled control invites somebody to look for the setting that
 *    enables it, and there is not one.
 * 4. **The texture number is shown, with its sample count.** Section 0's headline KPI is a
 *    measurement, so it belongs on screen; and a ratio measured over eleven samples is
 *    arithmetic rather than evidence, so the panel says which it is.
 * 5. **Nothing here reshapes, slims or lightens.** There is no prop, handler or control on this
 *    surface that could carry one, and there never will be without a CTO-role ADR.
 */

/** What the panel needs. The caller fetches all of it; nothing is derived from a service here. */
export type RetouchPanelProps = {
  /** The project's coverage, or `null` while it is loading. */
  status: RetouchStatusDto | null;
  /** The open photograph's plan, or `null` when nobody has retouched it. */
  plan: RetouchPlanDto | null;
  /** Choose a preset for this photograph. Records `userEdited`. */
  onSetPreset: (preset: string) => void;
  /** Set one person's strength, which applies across the whole gallery. */
  onSetStrength: (identityId: string, strength: number) => void;
  /** Stop protecting one feature. Never offered for an absolute one. */
  onClearProtection: (feature: ProtectedFeatureDto) => void;
  /** Record that the photographer looked at this plan and agrees. */
  onAccept: () => void;
  /** Show the frame as it was before the retouch. */
  onCompare: (showing: boolean) => void;
  /** True while the before/after comparison is held. */
  comparing: boolean;
};

/** The four presets, in the order the product offers them. */
const PRESETS: ReadonlyArray<{ slug: string; label: string; hint: string }> = [
  { slug: 'off', label: 'Off', hint: 'Nothing is retouched.' },
  { slug: 'light', label: 'Light', hint: 'Blemishes only, and only the confident ones.' },
  { slug: 'natural', label: 'Natural', hint: 'The default. Marks go, texture stays.' },
  { slug: 'polished', label: 'Polished', hint: 'The most AURA will do.' },
];

/** An operator slug as words. Unknown slugs read as themselves rather than as an error. */
function operationName(slug: string): string {
  const names: Record<string, string> = {
    blemish: 'Mark removed',
    under_eye: 'Under-eye lifted',
    tone_evening: 'Skin tone evened',
    shine_reduce: 'Shine reduced',
  };
  return names[slug] ?? slug.replace(/_/g, ' ');
}

/** A protected kind as words. */
function kindName(slug: string): string {
  const names: Record<string, string> = {
    mole: 'Mole',
    freckle: 'Freckles',
    birthmark: 'Birthmark',
    scar: 'Scar',
    tattoo: 'Tattoo',
    dimple: 'Dimple',
  };
  return names[slug] ?? slug;
}

/** Why a feature is protected, in the product's voice. */
function protectionReason(feature: ProtectedFeatureDto): string {
  if (feature.source === 'user') {
    return 'You asked AURA to keep this.';
  }
  if (feature.source === 'cross_frame') {
    const hours = feature.spanMinutes / 60;
    const span = hours >= 1 ? `${hours.toFixed(1)} hours` : `${Math.round(feature.spanMinutes)} minutes`;
    return `Seen in the same place on this face in ${feature.frames} photographs across ${span}.`;
  }
  return 'This looks like part of how this person looks.';
}

/** A percentage, rounded the way the sentences round it. */
function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function RetouchPanel({
  status,
  plan,
  onSetPreset,
  onSetStrength,
  onClearProtection,
  onAccept,
  onCompare,
  comparing,
}: RetouchPanelProps) {
  const withdrawals = useMemo(
    () => (plan?.reasons ?? []).filter((reason) => reason.withdrawal),
    [plan],
  );
  const removals = useMemo(
    () => (plan?.reasons ?? []).filter((reason) => !reason.withdrawal),
    [plan],
  );

  if (!plan) {
    return (
      <section className="retouch" aria-label="Retouch">
        <header>
          <h2>Retouch</h2>
        </header>
        <p data-testid="retouch-empty">AURA has not retouched this photograph yet.</p>
      </section>
    );
  }

  const caveats: string[] = [];
  if (plan.texture.withdrawn) {
    caveats.push(
      'AURA could not retouch this photograph without losing skin texture, so it left it alone.',
    );
  } else if (plan.texture.resolves > 0) {
    caveats.push('AURA used a gentler retouch here so that the skin kept its own texture.');
  }
  if (plan.reasons.some((reason) => reason.code === 'mask_unavailable')) {
    caveats.push(
      'AURA is not sure enough where the skin is in this photograph, so it did not retouch it.',
    );
  }
  if (plan.reasons.some((reason) => reason.code === 'head_untrained')) {
    caveats.push(
      'AURA is using its measured retouching rather than a learned model in this build.',
    );
  }
  if (!plan.texture.withdrawn && plan.texture.measuredOn > 0 && plan.texture.measuredOn < 256) {
    caveats.push(
      'There was very little skin visible here, so the texture measurement is rough.',
    );
  }

  return (
    <section className="retouch" aria-label="Retouch">
      <header>
        <h2>Retouch</h2>
        <p data-testid="retouch-summary">
          {plan.ops.length === 0
            ? 'Nothing was changed on this photograph.'
            : `${plan.ops.length} adjustment${plan.ops.length === 1 ? '' : 's'}.`}{' '}
          {percent(plan.budgetUsed)} of what AURA allows itself to change in one photograph.
        </p>
        <button
          type="button"
          onMouseDown={() => onCompare(true)}
          onMouseUp={() => onCompare(false)}
          onMouseLeave={() => onCompare(false)}
          data-testid="retouch-compare"
          aria-pressed={comparing}
        >
          Hold to see it before
        </button>
      </header>

      {caveats.length > 0 && (
        <ul className="retouch-caveats" data-testid="retouch-caveats">
          {caveats.map((caveat) => (
            <li key={caveat}>{caveat}</li>
          ))}
        </ul>
      )}

      <fieldset className="retouch-presets">
        <legend>How much care</legend>
        {PRESETS.map((preset) => (
          <label key={preset.slug} htmlFor={`retouch-preset-${preset.slug}`}>
            <input
              id={`retouch-preset-${preset.slug}`}
              type="radio"
              name="retouch-preset"
              value={preset.slug}
              checked={plan.preset === preset.slug}
              onChange={() => onSetPreset(preset.slug)}
              data-testid={`retouch-preset-${preset.slug}`}
            />
            {preset.label}
            <span className="retouch-preset-hint">{preset.hint}</span>
          </label>
        ))}
      </fieldset>

      {/* The texture guarantee, as a number rather than as a claim. */}
      <p className="retouch-texture" data-testid="retouch-texture">
        {plan.texture.withdrawn
          ? `Skin texture could not be kept above ${percent(plan.texture.floor)}, so nothing was applied.`
          : `Skin kept ${percent(plan.texture.bandRatio)} of its own texture, against a floor of ${percent(
              plan.texture.floor,
            )}.`}
        {plan.texture.measuredOn > 0 && ` Measured over ${plan.texture.measuredOn} samples of skin.`}
      </p>

      {plan.identityStrengths.length > 0 && (
        <ol className="retouch-people" data-testid="retouch-people">
          {plan.identityStrengths.map((entry) => (
            <li key={entry.identityId}>
              <label htmlFor={`retouch-strength-${entry.identityId}`}>
                This person, everywhere in this wedding
              </label>
              <input
                id={`retouch-strength-${entry.identityId}`}
                type="range"
                min="0"
                max="100"
                step="5"
                value={String(Math.round(entry.strength * 100))}
                onChange={(event) =>
                  onSetStrength(entry.identityId, Number(event.target.value) / 100)
                }
                data-testid={`retouch-strength-${entry.identityId}`}
              />
              <span>{percent(entry.strength)}</span>
            </li>
          ))}
        </ol>
      )}

      {plan.ops.length > 0 && (
        <ol className="retouch-ops" data-testid="retouch-ops">
          {plan.ops.map((op, index) => (
            <li key={`${op.kind}-${index}`}>
              {operationName(op.kind)}
              {op.kind === 'under_eye'
                ? ` ${op.lumaEv.toFixed(2)} EV`
                : ` at ${percent(op.strength)}`}
            </li>
          ))}
        </ol>
      )}

      {/* Rule 1: what was left alone, in the same weight as what was done. */}
      {withdrawals.length > 0 && (
        <ul className="retouch-left-alone" data-testid="retouch-left-alone">
          {withdrawals.map((reason) => (
            <li key={`${reason.code}-${reason.text}`}>{reason.text}</li>
          ))}
        </ul>
      )}

      {removals.length > 0 && (
        <ul className="retouch-reasons" data-testid="retouch-reasons">
          {removals.map((reason) => (
            <li key={`${reason.code}-${reason.text}`}>{reason.text}</li>
          ))}
        </ul>
      )}

      <ul className="retouch-protected" data-testid="retouch-protected">
        {plan.protected.length === 0 && (
          <li data-testid="retouch-protected-empty">
            AURA has not marked anything on this person as permanent yet.
          </li>
        )}
        {plan.protected.map((feature, index) => (
          <li key={`${feature.identityId}-${feature.kind}-${index}`}>
            <span>{kindName(feature.kind)}</span>
            <span className="retouch-protected-why">{protectionReason(feature)}</span>
            {feature.absolute ? (
              <span data-testid="retouch-protected-absolute">AURA never alters tattoos.</span>
            ) : (
              <button
                type="button"
                onClick={() => onClearProtection(feature)}
                data-testid={`retouch-unprotect-${feature.kind}-${index}`}
              >
                Stop protecting this
              </button>
            )}
          </li>
        ))}
      </ul>

      <footer>
        <button
          type="button"
          onClick={onAccept}
          disabled={plan.reviewed}
          data-testid="retouch-accept"
        >
          {plan.reviewed ? 'Agreed' : 'Looks right'}
        </button>
        {plan.userEdited && <span data-testid="retouch-edited">You set this by hand.</span>}
        {status && (
          <span data-testid="retouch-coverage">
            {percent(status.coverage)} of this wedding looked at, {status.blemishesRemoved} marks
            removed, {status.anomaliesLeft} left alone.
          </span>
        )}
      </footer>
    </section>
  );
}
