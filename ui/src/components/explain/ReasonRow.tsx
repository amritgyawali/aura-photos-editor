import type { CSSProperties } from 'react';

import type { LedgerReasonDto } from '../../ipc/types';

/**
 * PHASE-13. One reason, as the photographer reads it.
 *
 * Four things are on the row and each one is there for a reason a support case
 * taught somebody:
 *
 * * **The sentence**, which the deciding code wrote. Never a slug.
 * * **The severity**, which the backend read from the registry. A caveat is
 *   rendered as a caveat and never as a fault: "AURA did not check this" and
 *   "AURA checked this and it is bad" are opposite sentences, and this component
 *   is the last place that distinction can be lost.
 * * **The weight**, as a bar rather than a decimal, because a photographer is
 *   comparing reasons rather than reading a number.
 * * **The evidence**, or the honest absence of it.
 *
 * The row never says keep, reject, good or bad. It says what happened.
 */

/** The word beside each severity. Approved copy, not a slug. */
export const SEVERITY_WORD: Record<string, string> = {
  credit: 'in its favour',
  note: 'worth knowing',
  caveat: 'not checked',
  fault: 'counted against it',
};

/** The severity's own accessible description, for a reader that has no colours. */
export function severityLabel(severity: string): string {
  return SEVERITY_WORD[severity] ?? 'worth knowing';
}

/** A caveat is never a fault. Exported so a test can assert the mapping directly. */
export function isCaveat(reason: LedgerReasonDto): boolean {
  return reason.severity === 'caveat';
}

/** The bar's width, as a percentage of the strongest reason on the panel. */
export function barWidth(weight: number, strongest: number): number {
  const scale = Math.max(Math.abs(strongest), 0.01);
  return Math.round((Math.min(Math.abs(weight), scale) / scale) * 100);
}

/** What the evidence chip says, or nothing when there is none to show. */
export function evidenceLabel(reason: LedgerReasonDto): string | null {
  switch (reason.evidenceKind) {
    case 'crop':
      return 'shows where';
    case 'frames':
      return reason.frames.length === 1
        ? 'compare with the other frame'
        : `compare with ${reason.frames.length} frames`;
    case 'params':
      return reason.params.map((param) => `${param.name} ${param.value > 0 ? '+' : ''}${param.value}`).join(', ');
    default:
      return null;
  }
}

export type ReasonRowProps = {
  reason: LedgerReasonDto;
  /** The largest absolute weight on the panel, for the bar's scale. */
  strongest: number;
  /** Called when the photographer asks to see the evidence. */
  onShowEvidence?: (reason: LedgerReasonDto) => void;
};

export function ReasonRow({ reason, strongest, onShowEvidence }: ReasonRowProps) {
  const label = severityLabel(reason.severity);
  const evidence = evidenceLabel(reason);
  const width = barWidth(reason.weight, strongest);

  const barStyle: CSSProperties = {
    width: `${width}%`,
  };

  return (
    <li className={`reason-row reason-${reason.severity}`} data-code={reason.code}>
      <p className="reason-text">{reason.text}</p>
      <p className="reason-meta">
        <span className="reason-severity">{label}</span>
        <span className="reason-domain">{reason.domain}</span>
        <span className="reason-weight-bar" aria-hidden="true">
          <span className="reason-weight-fill" style={barStyle} />
        </span>
        <span className="visually-hidden">
          {`weight ${Math.abs(reason.weight).toFixed(2)}, ${label}`}
        </span>
      </p>
      {evidence !== null && (
        <button
          type="button"
          className="reason-evidence"
          onClick={() => onShowEvidence?.(reason)}
        >
          {evidence}
        </button>
      )}
    </li>
  );
}
