import { useMemo } from 'react';

import type { QcReportDto, QcStatusDto } from '../../ipc/types';

/**
 * PHASE-27. What the QC pass checked, what it could not check, and what it found - in that order.
 *
 * Pure - rows and callbacks in, nothing fetched. `QcPanel` is the one piece that talks to the shell.
 *
 * ## The ordering is the design
 *
 * Every other report in this product leads with what it found. This one leads with **what it
 * looked at**, because a QC report is the one place where an empty result is genuinely ambiguous:
 * zero findings means either that AURA inspected the whole gallery and it is clean, or that AURA
 * could not inspect it. Those are opposite conclusions and they produce the same number.
 *
 * In this build the second is the common case. Phase 06's detector finds no faces, phase 18's
 * segmenter is untrained and phase 22's face recovery never runs, so most checks skip on most
 * frames and `completeness` is well below one. The headline says so in a sentence before any count
 * appears, and `detectorTrained` is rendered as a caveat rather than hidden.
 *
 * ## The three numbers that must never be collapsed
 *
 * **Found, fixed and skipped are three separate columns per category.** A category that found
 * eleven problems and fixed nine is a category doing its job; a category that found none and
 * skipped four hundred frames is a category that did not run; and a single "issues" number would
 * render them identically.
 *
 * **Fixed and reverted are separate.** A remedy that was applied and then put back because it
 * delivered less than half of what it promised is not a fix, and a report that counted it as one
 * would be describing work that is not in the delivered file.
 *
 * **`imagesUnreached` is on the header.** A pass that ran out of time inspected a prefix of the
 * gallery, and a report that showed only what it found would be a report about part of a wedding
 * presented as a report about all of it.
 */
export type QcReportProps = {
  /** The project header, or null while it loads. */
  status: QcStatusDto | null;
  /** The most recent pass, or null when none has run. */
  report: QcReportDto | null;
  /** True while a pass is running. */
  running: boolean;
  /** Run an inspection. `remediate` is false: this changes nothing. */
  onInspect: () => void;
  /** Run an inspection and apply the remedies the autonomy bands permit. */
  onRemediate: () => void;
  /** Copy the report out as Markdown, for a studio's records. */
  onExport: () => void;
};

/** The ten inspections, in `QcCategory::ALL` order, with the words a photographer uses. */
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

function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

export function QcReport({
  status,
  report,
  running,
  onInspect,
  onRemediate,
  onExport,
}: QcReportProps) {
  /**
   * The sentence the report leads with, chosen from the completeness rather than from the count.
   *
   * The order of these branches is the guarantee. "Nothing is wrong" is the last thing this
   * component will say and it is only reachable when the pass ran, reached every frame and skipped
   * nothing - which on this build is never.
   */
  const headline = useMemo(() => {
    if (!status || status.checked === 0) {
      return 'This gallery has not been checked yet.';
    }
    if (!status.detectorTrained && status.completeness < 1) {
      return (
        `AURA checked ${status.checked} delivered photographs, and ` +
        `${percent(1 - status.completeness)} of the checks it wanted to run could not run - ` +
        'something they needed had not been measured. What is below is what could be checked, ' +
        'and it is not a clean bill of health for the rest.'
      );
    }
    if (report && !report.complete) {
      return (
        `AURA ran out of time after ${report.images} photographs and did not reach ` +
        `${report.imagesUnreached}. What is below is about the part it reached.`
      );
    }
    if (status.open === 0 && status.completeness >= 1) {
      return `AURA checked every one of ${status.checked} delivered photographs and found nothing.`;
    }
    return `${status.open} findings on ${status.checked} delivered photographs.`;
  }, [report, status]);

  return (
    <section className="qc-report" aria-label="Quality control report">
      <header className="qc-report__header">
        <h2>Before you deliver</h2>
        <p className="qc-report__headline">{headline}</p>

        {status ? (
          <dl className="qc-report__stats">
            <div>
              <dt>Checked</dt>
              <dd>
                {status.checked} of {status.selected} delivered ({percent(status.coverage)})
              </dd>
            </div>
            <div>
              <dt>Checks that ran</dt>
              <dd>
                {status.inspections} of {status.inspections + status.inspectionsSkipped} (
                {percent(status.completeness)})
              </dd>
            </div>
            <div>
              <dt>Could not be checked</dt>
              <dd>{status.inspectionsSkipped}</dd>
            </div>
            <div>
              <dt>Outstanding</dt>
              <dd>{status.open}</dd>
            </div>
            <div>
              <dt>Frames swapped</dt>
              <dd>{status.replaced}</dd>
            </div>
          </dl>
        ) : null}

        {status && !status.detectorTrained ? (
          <p className="qc-report__caveat" role="note">
            No defect-detection model ships in this build. Every check above is a measurement
            against a number another part of AURA already recorded, which finds fewer problems
            rather than inventing them - and it means a category with nothing in it may simply be a
            category that could not run.
          </p>
        ) : null}

        <div className="qc-report__actions">
          <button type="button" onClick={onInspect} disabled={running}>
            {running ? 'Checking…' : 'Check this gallery'}
          </button>
          <button type="button" onClick={onRemediate} disabled={running}>
            Check and fix what AURA is confident about
          </button>
          <button type="button" onClick={onExport} disabled={!report}>
            Save the report
          </button>
        </div>
      </header>

      {report ? (
        <>
          <table className="qc-report__table">
            <caption>What each inspection found</caption>
            <thead>
              <tr>
                <th scope="col">Inspection</th>
                <th scope="col">Found</th>
                <th scope="col">Fixed</th>
                <th scope="col">For you</th>
                <th scope="col">Could not check</th>
              </tr>
            </thead>
            <tbody>
              {report.byCategory.map((row) => (
                <tr
                  key={row.category}
                  className={
                    row.found === 0 && row.skipped > 0 ? 'qc-report__row is-unavailable' : undefined
                  }
                >
                  <th scope="row">{CATEGORY_LABEL[row.category] ?? row.category}</th>
                  <td>{row.found}</td>
                  <td>{row.fixed}</td>
                  <td>{row.escalated}</td>
                  <td>{row.skipped}</td>
                </tr>
              ))}
            </tbody>
          </table>

          <dl className="qc-report__totals">
            <div>
              <dt>Fixed and confirmed</dt>
              <dd>{report.fixed}</dd>
            </div>
            <div>
              <dt>Tried and put back</dt>
              <dd>{report.reverted}</dd>
            </div>
            <div>
              <dt>Left for you</dt>
              <dd>{report.escalated}</dd>
            </div>
            <div>
              <dt>Took</dt>
              <dd>{(report.durationMs / 1000).toFixed(1)} s</dd>
            </div>
          </dl>

          {report.replacements.length > 0 ? (
            <section className="qc-report__swaps" aria-label="Frames swapped">
              <h3>Frames AURA swapped</h3>
              <ul>
                {report.replacements.map((swap) => (
                  <li key={swap.ticketId}>
                    {swap.note} ({swap.metricBefore.toFixed(2)} → {swap.metricAfter.toFixed(2)},{' '}
                    {percent(swap.confidence)} sure
                    {swap.coverageHeld ? ', coverage held' : ''})
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
