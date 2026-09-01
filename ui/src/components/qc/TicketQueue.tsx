import { useMemo, useState } from 'react';

import type { QcGroupDto, QcTicketDto } from '../../ipc/types';

/**
 * PHASE-27. The escalation queue: what AURA could not fix, grouped by inspection, worst first.
 *
 * Pure - rows and callbacks in, nothing fetched.
 *
 * ## Grouped, because that is how a photographer works
 *
 * Eleven soft frames are one decision, not eleven. The groups are ordered by their **worst
 * member** rather than by how many findings they contain, so an inspection with one severe problem
 * outranks one with forty mild ones - which is also the order somebody would work in.
 *
 * Inside a group the order is severity as a **ratio**: 0.4 dE00 over a 0.2 ceiling and 400 K over
 * a 200 K ceiling are the same amount of wrong, and sorting on the raw deviation would put every
 * colour-temperature finding above every skin finding for ever. The wire already sorts; this
 * component does not re-sort, so the panel and the archived report agree.
 *
 * ## Every finding carries the numbers behind it
 *
 * A photographer arrives at this queue sceptical - AURA is telling them something is wrong with
 * work they may have approved - so `deviation`, `threshold` and `unit` sit beside the sentence,
 * never the sentence alone. A queue that showed only prose would be asking to be believed.
 *
 * ## Selecting many findings agrees; it does not authorise
 *
 * `onDecideBulk` takes a verdict and nothing else. Agreeing that forty findings are real is a
 * statement about the findings; instructing AURA to act on forty frames unattended is a statement
 * about the remedies, and the two are different judgements made with different amounts of
 * attention. Per-ticket authorisation is `onApply`, and it lives next to the before and after.
 * ADR-0056 section 5.
 */
export type TicketQueueProps = {
  /** The queue, grouped by inspection and ordered by each group's worst member. */
  groups: QcGroupDto[];
  /** Which finding is open, if any. */
  selectedTicketId: string | null;
  /** Open one finding. */
  onSelect: (ticketId: string) => void;
  /** Record a verdict on one finding, and optionally authorise its remedy. */
  onDecide: (ticketId: string, status: 'accepted' | 'dismissed', applyRemedy: boolean) => void;
  /** Record the same verdict on many findings. Authorises nothing. */
  onDecideBulk: (ticketIds: string[], status: 'accepted' | 'dismissed') => void;
};

/** The ten inspections, with the words a photographer uses. */
const CATEGORY_LABEL: Record<string, string> = {
  consistency: 'Matching the room',
  skin: 'Skin',
  exposure: 'Brightness',
  sharpness: 'Detail',
  retouch: 'Retouching',
  mask: 'Edges',
  crop: 'Framing',
  cleanup: 'Tidying',
  duplicate: 'Near-duplicates',
  coverage: 'Coverage',
};

/** How an autonomy band reads. */
const AUTONOMY_LABEL: Record<string, string> = {
  automatic: 'AURA can do this on its own',
  review: 'AURA would like you to look',
  confirm: 'AURA will not do this without you',
  manual: 'this one is yours',
};

function measurement(ticket: QcTicketDto): string {
  return `${ticket.deviation.toFixed(2)} ${ticket.unit} against ${ticket.threshold.toFixed(2)}`;
}

export function TicketQueue({
  groups,
  selectedTicketId,
  onSelect,
  onDecide,
  onDecideBulk,
}: TicketQueueProps) {
  const [checked, setChecked] = useState<Set<string>>(new Set());

  const total = useMemo(
    () => groups.reduce((sum, group) => sum + group.tickets.length, 0),
    [groups],
  );

  function toggle(ticketId: string) {
    setChecked((previous) => {
      const next = new Set(previous);
      if (next.has(ticketId)) {
        next.delete(ticketId);
      } else {
        next.add(ticketId);
      }
      return next;
    });
  }

  function decideChecked(status: 'accepted' | 'dismissed') {
    if (checked.size === 0) {
      return;
    }
    onDecideBulk([...checked], status);
    setChecked(new Set());
  }

  if (total === 0) {
    return (
      <section className="qc-queue" aria-label="Findings">
        <p className="qc-queue__empty">
          Nothing is waiting for you. That is not the same as nothing being wrong - the report above
          says how much of this gallery AURA was able to check.
        </p>
      </section>
    );
  }

  return (
    <section className="qc-queue" aria-label="Findings">
      {checked.size > 0 ? (
        <div className="qc-queue__bulk" role="group" aria-label="Selected findings">
          <span>{checked.size} selected</span>
          <button type="button" onClick={() => decideChecked('accepted')}>
            These are real
          </button>
          <button type="button" onClick={() => decideChecked('dismissed')}>
            These are not problems
          </button>
          <p className="qc-queue__bulk-note">
            This records what you think. It does not change any photograph - open a finding to let
            AURA act on it.
          </p>
        </div>
      ) : null}

      {groups.map((group) => (
        <article key={group.category} className="qc-queue__group">
          <h3>
            {CATEGORY_LABEL[group.category] ?? group.category}
            <span className="qc-queue__group-count">{group.tickets.length}</span>
          </h3>

          <ul>
            {group.tickets.map((ticket) => (
              <li
                key={ticket.ticketId}
                className={
                  ticket.ticketId === selectedTicketId ? 'qc-queue__row is-open' : 'qc-queue__row'
                }
              >
                <input
                  type="checkbox"
                  checked={checked.has(ticket.ticketId)}
                  onChange={() => toggle(ticket.ticketId)}
                  aria-label={`Select: ${ticket.diagnosis}`}
                />
                <button
                  type="button"
                  className="qc-queue__open"
                  onClick={() => onSelect(ticket.ticketId)}
                >
                  <span className="qc-queue__diagnosis">{ticket.diagnosis}</span>
                  <span className="qc-queue__measurement">{measurement(ticket)}</span>
                  <span className="qc-queue__autonomy">
                    {AUTONOMY_LABEL[ticket.autonomy] ?? ticket.autonomy}
                  </span>
                </button>

                {ticket.ticketId === selectedTicketId ? (
                  <div className="qc-queue__detail">
                    <ul className="qc-queue__reasons">
                      {ticket.reasons.map((reason) => (
                        <li key={reason}>{reason}</li>
                      ))}
                    </ul>
                    <p className="qc-queue__remedy">
                      What AURA would do: {ticket.remedyKind} — {ticket.remedyTarget}. It expects
                      that to close {ticket.expectedGain.toFixed(2)} {ticket.unit} of the{' '}
                      {ticket.deviation.toFixed(2)} it measured.
                    </p>
                    <div className="qc-queue__row-actions">
                      <button
                        type="button"
                        onClick={() => onDecide(ticket.ticketId, 'accepted', false)}
                      >
                        Agree, leave it to me
                      </button>
                      <button
                        type="button"
                        onClick={() => onDecide(ticket.ticketId, 'accepted', true)}
                      >
                        Agree, let AURA fix it
                      </button>
                      <button
                        type="button"
                        onClick={() => onDecide(ticket.ticketId, 'dismissed', false)}
                      >
                        This is not a problem
                      </button>
                    </div>
                  </div>
                ) : null}
              </li>
            ))}
          </ul>
        </article>
      ))}
    </section>
  );
}
