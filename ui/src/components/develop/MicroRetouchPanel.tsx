import { useMemo } from 'react';

import type {
  MicroMatrixDto,
  MicroOpDto,
  MicroPlanDto,
  MicroStatusDto,
} from '../../ipc/types';

/**
 * PHASE-21. The small fixes, with the per-operation switches, what was refused, and the
 * disclosure of anything borrowed from another photograph.
 *
 * Section 9's SFE deliverable: "micro-retouch panel with per-op toggles, previews and studio
 * defaults". Five rules, and the third is the one this phase exists to get right:
 *
 * 1. **What was left alone is shown as prominently as what was done.** Two thirds of the reason
 *    codes in this phase are withdrawals, and the commonest question a photographer has is why a
 *    particular thing was not fixed. A panel that only listed fixes would make a careful product
 *    look like a careless one - phase 20's rule, inherited.
 * 2. **A switch is a switch, and there is no slider anywhere.** A studio chooses *which*
 *    operations run; how far each may go is a product decision bounded by the contract. There is
 *    no prop on this component that could carry a strength, and adding one would make
 *    `docs/retouch-ethics.md` a description of the defaults rather than a promise.
 * 3. **A borrowed region is disclosed, loudly, and it cannot be rendered as an ordinary edit.**
 *    `renderOp` gives a borrow its own class, its own wording and the source photograph's
 *    identifier. This is one of the five places section 5 of the ethics document promises the
 *    disclosure appears, and it is the only one a photographer sees while working.
 * 4. **The two opt-in clothing issues are marked as opt-in.** Straps and creases start off and
 *    are labelled as choices rather than as settings somebody forgot to enable.
 * 5. **Nothing here reshapes, slims, whitens or recolours.** There is no prop, handler or control
 *    on this surface that could carry one, and there never will be without a CTO-role ADR.
 */

/** What the panel needs. The caller fetches all of it; nothing is derived from a service here. */
export type MicroRetouchPanelProps = {
  /** The project's coverage, or `null` while it is loading. */
  status: MicroStatusDto | null;
  /** The open photograph's plan, or `null` when nobody has planned it. */
  plan: MicroPlanDto | null;
  /** Which operations this project permits, or `null` while it is loading. */
  matrix: MicroMatrixDto | null;
  /** Switch one operation on or off for the whole project. */
  onToggleOperation: (operator: string, allowed: boolean) => void;
  /** Switch one clothing issue on or off for the whole project. */
  onToggleClothing: (kind: string, allowed: boolean) => void;
  /** Switch cross-frame borrowing on or off for the whole project. */
  onToggleBorrowing: (allowed: boolean) => void;
  /** Record that the photographer looked at this plan and agrees. */
  onAccept: () => void;
  /** Show the frame as it was before the small fixes. */
  onCompare: (showing: boolean) => void;
  /** True while the before/after comparison is held. */
  comparing: boolean;
};

/** An operator slug as words, and what it promises. */
const OPERATORS: Record<string, { label: string; hint: string }> = {
  flyaway: {
    label: 'Stray hair',
    hint: 'Calms strands against a clean background. Never erases one.',
  },
  teeth: {
    label: 'Teeth',
    hint: 'Evens them and takes a little yellow out, inside a measured natural range.',
  },
  eyes: {
    label: 'Eyes',
    hint: 'Reduces redness in the whites and adds a little iris definition. Catchlights are kept.',
  },
  clothing: {
    label: 'Clothing',
    hint: 'Lint, threads and small stains. Never the fabric texture.',
  },
  glare: {
    label: 'Glasses glare',
    hint: 'Reduces reflections, and can rebuild a blown-out patch from another frame.',
  },
};

/** A clothing issue as words. */
const CLOTHING: Record<string, string> = {
  lint: 'Lint',
  thread: 'Loose threads',
  stain: 'Small stains',
  strap: 'Visible straps',
  crease: 'Creases',
};

/** A family as words, for the withdrawal notices. */
const FAMILIES: Record<string, string> = {
  hair: 'the stray-hair work',
  teeth: 'the teeth correction',
  eyes: 'the eye work',
};

/** A percentage, rounded the way the sentences round it. */
function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

/** One operation as a sentence a photographer reads. */
function operationText(op: MicroOpDto): string {
  switch (op.kind) {
    case 'flyaway':
      return 'A stray hair was calmed';
    case 'teeth':
      return 'Teeth were evened';
    case 'eyes':
      return op.sclera > 0 && op.irisClarity > 0
        ? 'Redness reduced and the iris sharpened a little'
        : op.sclera > 0
          ? 'Redness in the whites was reduced'
          : 'The iris was sharpened a little';
    case 'clothing':
      return `${CLOTHING[op.clothingKind ?? ''] ?? 'Something'} was cleaned off the clothing`;
    case 'glare':
      return op.method === 'borrow'
        ? 'Glare was rebuilt from another photograph'
        : 'Glare on the glasses was reduced';
    default:
      return op.kind.replace(/_/g, ' ');
  }
}

export function MicroRetouchPanel({
  status,
  plan,
  matrix,
  onToggleOperation,
  onToggleClothing,
  onToggleBorrowing,
  onAccept,
  onCompare,
  comparing,
}: MicroRetouchPanelProps) {
  const withdrawals = useMemo(
    () => (plan?.reasons ?? []).filter((reason) => reason.doubt),
    [plan],
  );
  const borrows = useMemo(() => (plan?.ops ?? []).filter((op) => op.method === 'borrow'), [plan]);

  const caveats: string[] = [];
  if (plan) {
    plan.naturalness.families.forEach((family, index) => {
      if (plan.naturalness.withdrawn[index]) {
        caveats.push(
          `AURA could not do ${FAMILIES[family] ?? family} here without it starting to show, so it left that alone.`,
        );
      }
    });
    if (plan.naturalness.resolves > 0 && !plan.naturalness.withdrawn.some(Boolean)) {
      caveats.push('AURA made some of these fixes more gently so that nothing looks worked on.');
    }
    if (plan.reasons.some((reason) => reason.code === 'micro_region_unavailable')) {
      caveats.push(
        'AURA is not sure enough where the hair, teeth or eyes are in this photograph, so it left them alone.',
      );
    }
    if (plan.reasons.some((reason) => reason.code === 'micro_no_illuminant')) {
      caveats.push(
        'AURA has not worked out what colour the light was here, so it made no colour corrections.',
      );
    }
    if (plan.reasons.some((reason) => reason.code === 'micro_head_untrained')) {
      caveats.push(
        'AURA is using its measured detection rather than a learned model in this build.',
      );
    }
    if (plan.naturalness.measuredOn > 0 && plan.naturalness.measuredOn < 256) {
      caveats.push('There was very little to measure here, so these numbers are rough.');
    }
  }

  return (
    <section className="micro-retouch" aria-label="Small fixes">
      <header>
        <h2>Small fixes</h2>
        {status && (
          <p data-testid="micro-coverage">
            {status.planned} of {status.photos} photographs looked at ({percent(status.coverage)}).{' '}
            {status.regionCovered === 0
              ? 'None of them had the regions AURA needs, so nothing was changed.'
              : `${status.actedOn} had something to fix.`}
          </p>
        )}
        {status && (
          <p
            data-testid="micro-borrow-total"
            className={status.borrows > 0 ? 'micro-borrow-total is-composite' : 'micro-borrow-total'}
          >
            {status.borrows === 0
              ? 'No photograph in this gallery uses pixels from another one.'
              : `${status.borrows} photograph${status.borrows === 1 ? '' : 's'} in this gallery had a reflection rebuilt using pixels from another frame of the same moment.`}
          </p>
        )}
      </header>

      {matrix && (
        <div className="micro-matrix" data-testid="micro-matrix">
          <h3>What AURA may fix</h3>
          <ul>
            {matrix.operators.map((operator, index) => (
              <li key={operator}>
                <label>
                  <input
                    type="checkbox"
                    checked={matrix.allowed[index] ?? false}
                    onChange={(event) => onToggleOperation(operator, event.target.checked)}
                    data-testid={`micro-op-${operator}`}
                  />
                  <span>{OPERATORS[operator]?.label ?? operator}</span>
                </label>
                <p className="micro-hint">{OPERATORS[operator]?.hint ?? ''}</p>
              </li>
            ))}
          </ul>

          <h3>On clothing</h3>
          <ul>
            {matrix.clothingKinds.map((kind, index) => (
              <li key={kind}>
                <label>
                  <input
                    type="checkbox"
                    checked={matrix.clothing[index] ?? false}
                    onChange={(event) => onToggleClothing(kind, event.target.checked)}
                    data-testid={`micro-clothing-${kind}`}
                  />
                  <span>{CLOTHING[kind] ?? kind}</span>
                </label>
                {matrix.clothingOptIn[index] && (
                  <span className="micro-opt-in" data-testid={`micro-opt-in-${kind}`}>
                    Off unless you turn it on
                  </span>
                )}
              </li>
            ))}
          </ul>

          <label className="micro-borrowing">
            <input
              type="checkbox"
              checked={matrix.borrowing}
              onChange={(event) => onToggleBorrowing(event.target.checked)}
              data-testid="micro-borrowing"
            />
            <span>Rebuild blown-out glare from another frame of the same moment</span>
          </label>
          <p className="micro-hint">
            Only where the reflection has destroyed the record completely, only over small areas,
            and always listed in the delivery report.
          </p>
        </div>
      )}

      {!plan && (
        <p data-testid="micro-empty">AURA has not looked at this photograph for small fixes yet.</p>
      )}

      {plan && (
        <>
          <p data-testid="micro-summary">
            {plan.ops.length === 0
              ? 'Nothing was changed on this photograph.'
              : `${plan.ops.length} small fix${plan.ops.length === 1 ? '' : 'es'}.`}{' '}
            {percent(plan.budgetUsed)} of what AURA allows itself to change in one photograph.
          </p>
          <button
            type="button"
            onMouseDown={() => onCompare(true)}
            onMouseUp={() => onCompare(false)}
            onMouseLeave={() => onCompare(false)}
            data-testid="micro-compare"
            aria-pressed={comparing}
          >
            Hold to see it before
          </button>

          {borrows.length > 0 && (
            <div className="micro-borrowed" data-testid="micro-borrowed">
              <h3>Rebuilt from another photograph</h3>
              <ul>
                {borrows.map((op, index) => (
                  <li key={`${op.borrowedFrom ?? 'unknown'}-${index}`}>
                    A patch of glare was rebuilt using pixels from{' '}
                    <code>{op.borrowedFrom ?? 'an unrecorded frame'}</code>, which lined up{' '}
                    {percent(op.alignment)}.
                  </li>
                ))}
              </ul>
              <p className="micro-hint">
                AURA only does this where the reflection had blown out completely, so there was
                nothing left of the eye underneath to recover.
              </p>
            </div>
          )}

          {caveats.length > 0 && (
            <ul className="micro-caveats" data-testid="micro-caveats">
              {caveats.map((caveat) => (
                <li key={caveat}>{caveat}</li>
              ))}
            </ul>
          )}

          <h3>What AURA did</h3>
          {plan.ops.length === 0 ? (
            <p data-testid="micro-none">Nothing.</p>
          ) : (
            <ul data-testid="micro-ops">
              {plan.ops.map((op, index) => (
                <li
                  key={`${op.kind}-${index}`}
                  className={op.method === 'borrow' ? 'micro-op is-composite' : 'micro-op'}
                >
                  {operationText(op)}
                  {op.method === 'borrow' && (
                    <span className="micro-badge" data-testid={`micro-op-badge-${index}`}>
                      from another frame
                    </span>
                  )}
                </li>
              ))}
            </ul>
          )}

          <h3>What AURA left alone</h3>
          {withdrawals.length === 0 ? (
            <p data-testid="micro-nothing-withheld">Nothing was held back on this photograph.</p>
          ) : (
            <ul data-testid="micro-withdrawals">
              {withdrawals.map((reason) => (
                <li key={reason.code}>{reason.text}</li>
              ))}
            </ul>
          )}

          <dl className="micro-measurements" data-testid="micro-measurements">
            <dt>Catchlights kept</dt>
            <dd>{percent(plan.naturalness.catchlightRatio)}</dd>
            <dt>Hairline detail kept</dt>
            <dd>{percent(plan.naturalness.hairEnergyRatio)}</dd>
            <dt>Teeth moved outside natural</dt>
            <dd>{plan.naturalness.teethExcursion.toFixed(4)}</dd>
          </dl>

          <button
            type="button"
            onClick={onAccept}
            disabled={plan.reviewed}
            data-testid="micro-accept"
          >
            {plan.reviewed ? 'You have agreed with this' : 'Looks right'}
          </button>
        </>
      )}
    </section>
  );
}
