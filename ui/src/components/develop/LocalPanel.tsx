import { useMemo } from 'react';

import type { LocalPlanDto, LocalStatusDto } from '../../ipc/types';

/**
 * PHASE-19. Local adjustments, with a strength per operation and an overlay of what was
 * applied.
 *
 * Section 9's SFE deliverable: "local panel with per-operation strength, overlay of applied
 * maps, before/after". Four rules, and the first is what this component is for:
 *
 * 1. **It makes an invisible edit visible.** Section 0's mission is "why does this look so
 *    much better and I can't tell what changed", which is a wonderful thing for a gallery and
 *    a terrible thing for a panel: a photographer who cannot see what was done cannot decide
 *    whether they agree with it. So every number here is shown - what each face was moved by,
 *    what stopped it, how much of the allowance was spent - and the reasons are shown in the
 *    product's own sentences rather than as codes.
 * 2. **A strength is a slider, not a switch.** Every operation runs at a strength between zero
 *    and one, and turning one down is a normal thing to do rather than an override of a
 *    decision. Setting one *does* record `userEdited`, and the panel says so, because phase
 *    30's learning loop needs to be able to tell a typed correction from an accepted
 *    suggestion.
 * 3. **A gated operation is shown, not hidden.** An operation that could not run because no
 *    mask arrived is the most common state on this build, and rendering it as "off" would make
 *    "phase 18 is not installed" look like "AURA decided this photograph needed nothing".
 * 4. **Nothing here culls, and nothing here retouches.** There is no keep, reject, deliver or
 *    smooth on this surface, and no prop or handler that could carry one.
 */

/** What the panel needs. The caller fetches both; nothing is derived from a service here. */
export type LocalPanelProps = {
  /** The project's coverage, or `null` while it is loading. */
  status: LocalStatusDto | null;
  /** The open photograph's plan, or `null` when nobody has planned it. */
  plan: LocalPlanDto | null;
  /** Set one operation's strength. Records `userEdited`. */
  onSetStrength: (operation: string, strength: number) => void;
  /** Record that the photographer looked at this plan and agrees. */
  onAccept: () => void;
  /** Show the frame as it was before the local work. */
  onCompare: (showing: boolean) => void;
  /** True while the before/after comparison is held. */
  comparing: boolean;
};

/** An operation slug as words. Unknown slugs read as themselves rather than as an error. */
function operationName(slug: string): string {
  const names: Record<string, string> = {
    face_light: 'Face lighting',
    subject_enhance: 'Subject presence',
    background_balance: 'Background',
    shine_control: 'Shine',
    dodge_burn_low: 'Shaping',
    dodge_burn_mid: 'Evening out',
  };
  return names[slug] ?? slug.replace(/_/g, ' ');
}

/** A mask kind as words, for the sentence that explains a gate. */
function maskName(slug: string): string {
  const names: Record<string, string> = {
    face: 'the face',
    subject: 'the subject',
    background: 'the background',
    skin: 'the skin',
    hair: 'the hair',
    sky: 'the sky',
  };
  return names[slug] ?? slug;
}

/** A percentage, rounded the way the sentences round it. */
function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

/** Stops, signed, to two decimals. */
function stops(value: number): string {
  const sign = value > 0 ? '+' : '';
  return `${sign}${value.toFixed(2)} EV`;
}

export function LocalPanel({
  status,
  plan,
  onSetStrength,
  onAccept,
  onCompare,
  comparing,
}: LocalPanelProps) {
  const gatedBy = useMemo(() => {
    const map = new Map<string, string>();
    for (const gate of plan?.gated ?? []) {
      map.set(gate.operation, gate.maskKind);
    }
    return map;
  }, [plan]);

  if (!plan) {
    return (
      <section className="local" aria-label="Local light">
        <header>
          <h2>Local light</h2>
        </header>
        <p data-testid="local-empty">
          AURA has not looked at the light inside this photograph yet.
        </p>
      </section>
    );
  }

  const caveats: string[] = [];
  if (plan.gated.length > 0) {
    caveats.push(
      `AURA could not work out where ${[...new Set(plan.gated.map((g) => maskName(g.maskKind)))].join(
        ', ',
      )} is in this photograph, so ${plan.gated.length === 1 ? 'one adjustment was' : `${plan.gated.length} adjustments were`} left out.`,
    );
  }
  if (plan.reasons.some((reason) => reason.code === 'target_head_unavailable')) {
    caveats.push(
      'AURA is using its built-in guidance for how faces should be lit rather than anything learned from edits.',
    );
  }
  if (plan.reasons.some((reason) => reason.code === 'scene_strength_limited')) {
    caveats.push(
      'AURA has no local shaping guidance recorded for this kind of photograph yet, so it worked very gently here.',
    );
  }

  return (
    <section className="local" aria-label="Local light">
      <header>
        <h2>Local light</h2>
        <p data-testid="local-summary">
          {plan.faces.length === 0
            ? 'No faces here.'
            : `${plan.faces.length} face${plan.faces.length === 1 ? '' : 's'} lit.`}{' '}
          {percent(plan.budgetUsed)} of what AURA allows itself to change in one photograph.
        </p>
        <button
          type="button"
          onMouseDown={() => onCompare(true)}
          onMouseUp={() => onCompare(false)}
          onMouseLeave={() => onCompare(false)}
          data-testid="local-compare"
          aria-pressed={comparing}
        >
          Hold to see it before
        </button>
      </header>

      {caveats.length > 0 && (
        <ul className="local-caveats" data-testid="local-caveats">
          {caveats.map((caveat) => (
            <li key={caveat}>{caveat}</li>
          ))}
        </ul>
      )}

      <ol className="local-operations">
        {plan.operations.map((operation, index) => {
          const strength = plan.strengths[index] ?? 0;
          const gate = gatedBy.get(operation);
          return (
            <li key={operation} className={gate ? 'local-operation gated' : 'local-operation'}>
              <label htmlFor={`local-strength-${operation}`}>{operationName(operation)}</label>
              <input
                id={`local-strength-${operation}`}
                type="range"
                min="0"
                max="100"
                step="5"
                disabled={Boolean(gate)}
                value={String(Math.round(strength * 100))}
                onChange={(event) =>
                  onSetStrength(operation, Number(event.target.value) / 100)
                }
                data-testid={`local-strength-${operation}`}
              />
              <span className="local-strength-value">{percent(strength)}</span>
              {gate && (
                <span className="local-gate" data-testid={`local-gate-${operation}`}>
                  not available - AURA could not find {maskName(gate)}
                </span>
              )}
            </li>
          );
        })}
      </ol>

      {plan.faces.length > 0 && (
        <table className="local-faces" data-testid="local-faces">
          <caption>
            What each face was moved by. Everybody in a group is lit together, so nobody looks
            pasted in.
          </caption>
          <thead>
            <tr>
              <th scope="col">Face</th>
              <th scope="col">Was</th>
              <th scope="col">Aimed at</th>
              <th scope="col">Ended</th>
              <th scope="col">Moved</th>
              <th scope="col">Could have moved</th>
            </tr>
          </thead>
          <tbody>
            {plan.faces.map((face, index) => (
              <tr key={face.identityId ?? `face-${index}`}>
                <th scope="row">{face.identityId ? 'Named' : `Face ${index + 1}`}</th>
                <td>{percent(face.lumaBefore)}</td>
                <td>{percent(face.lumaTarget)}</td>
                <td>{percent(face.lumaAfter)}</td>
                <td>{stops(face.exposureEv)}</td>
                <td>{stops(face.noiseCapEv)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {plan.faces.length > 1 && (
        <p
          className={plan.groupFair ? 'local-group ok' : 'local-group held'}
          data-testid="local-group"
        >
          {plan.groupFair
            ? `Everybody here ended within ${percent(plan.faceSpread)} of each other.`
            : `AURA could not even this group out completely; they are ${percent(plan.faceSpread)} apart. Nobody was darkened to close the gap.`}
        </p>
      )}

      {plan.shaping.length > 0 && (
        <details className="local-shaping" data-testid="local-shaping">
          <summary>
            Shaping - {plan.shaping.reduce((total, zones) => total + zones.length, 0)} moves
          </summary>
          <p>
            The shape of the face, deepened and lifted the way a retoucher would. Nothing here
            touches skin texture.
          </p>
          <ul>
            {plan.shaping.flatMap((zones, faceIndex) =>
              zones.map((zone) => (
                <li key={`${faceIndex}-${zone.zone}`}>
                  {zone.zone.replace(/_/g, ' ')} {stops(zone.gainEv)}
                </li>
              )),
            )}
          </ul>
        </details>
      )}

      <ul className="local-reasons" data-testid="local-reasons">
        {plan.reasons.map((reason) => (
          <li
            key={reason.code}
            className={reason.withdrawal ? 'local-reason withdrawal' : 'local-reason'}
          >
            {reason.text}
          </li>
        ))}
      </ul>

      <footer className="local-footer">
        {plan.userEdited ? (
          <p data-testid="local-user-edited">
            You set these strengths by hand. AURA will not change them.
          </p>
        ) : (
          <button type="button" onClick={onAccept} data-testid="local-accept">
            {plan.reviewed ? 'Looked at' : 'This looks right'}
          </button>
        )}
        {status && (
          <p className="local-coverage" data-testid="local-coverage">
            {percent(status.actedOn)} of the {status.planned} photographs AURA has looked at
            needed a local adjustment.{' '}
            {status.maskCovered < 1 &&
              `${percent(1 - status.maskCovered)} of them were missing a mask AURA needed.`}
          </p>
        )}
      </footer>
    </section>
  );
}
