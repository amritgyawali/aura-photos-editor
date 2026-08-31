import type {
  CameraReportDto,
  CameraStatusDto,
  CameraTransformDto,
  MatchedPairDto,
  ShooterBiasDto,
} from '../../ipc/types';

/**
 * PHASE-26. The Camera Match view: what each body needed, and on what evidence.
 *
 * Pure - rows and callbacks in, nothing fetched - like the four phase 25 components beside it.
 * `CameraMatchPanel` is the one piece that talks to the shell.
 *
 * ## The one design rule this view exists to enforce
 *
 * **Evidence before number, everywhere.** A body corrected by 300 K from thirty-four verified pairs
 * of its own ceremony and a body corrected by 300 K from a bundled brand setting are the same
 * arithmetic and completely different claims, and only the second needs a photographer to look at
 * it. So every camera row leads with `headline`, which reads the reason set rather than the
 * magnitude, and the numbers sit underneath.
 *
 * The project header does the same at its own scale: `solvedFromPairs` is rendered beside
 * `cameras`, because "four cameras matched" means nothing without "three of them from a brand
 * setting".
 *
 * ## Three things this view must never render
 *
 * **A measurement claim while `baselinesMeasured` is false.** It is false in this build. Every
 * bundled brand baseline was chosen to be plausible rather than measured from a photographed
 * target, and a panel that presented one as a laboratory result would be making the product's worst
 * available claim.
 *
 * **A skin promise while `skinFieldAvailable` is false.** Nothing about anybody's skin was measured
 * on this build, so the skin figures are absent rather than zero-and-green.
 *
 * **`heldoutImproved === null` as a pass.** It is the third state - there were too few spare pairs
 * to check the correction against - and it is not the same as a check that passed.
 */
export type CameraMatchViewProps = {
  /** The project header, or null while it loads. */
  status: CameraStatusDto | null;
  /** The per-camera report, worst evidence first. */
  reports: CameraReportDto[];
  /** The corrections behind those reports, keyed by `cameraId` and `flash`. */
  transforms: CameraTransformDto[];
  /** Every measured exposure habit. */
  shooterBias: ShooterBiasDto[];
  /** The matched pairs for whichever camera is expanded, or an empty list. */
  pairs: MatchedPairDto[];
  /** Which camera row is expanded, if any. */
  expandedCameraId: string | null;
  /** True while a matching pass is running. */
  running: boolean;
  /** Expand or collapse one camera row. */
  onExpand: (cameraId: string | null) => void;
  /** Run or re-run the matching pass. */
  onRunPass: () => void;
  /** Choose the body everything else is matched to. */
  onSetReference: (cameraId: string) => void;
  /** Switch matching off for one body, or back on. */
  onToggleEnabled: (cameraId: string, disabled: boolean) => void;
};

/** How a source slug reads in the header of a camera row. */
const SOURCE_LABEL: Record<string, string> = {
  matched_pairs: 'from this wedding',
  blended: 'part measured here',
  brand_baseline: 'from the brand',
};

function formatKelvin(value: number): string {
  const rounded = Math.round(value);
  return `${rounded > 0 ? "+" : ""}${rounded} K`;
}

function formatStops(value: number): string {
  return `${value > 0 ? "+" : ""}${value.toFixed(2)} EV`;
}

export function CameraMatchView({
  status,
  reports,
  transforms,
  shooterBias,
  pairs,
  expandedCameraId,
  running,
  onExpand,
  onRunPass,
  onSetReference,
  onToggleEnabled,
}: CameraMatchViewProps) {
  if (!status) {
    return (
      <section className="camera-match" aria-busy="true">
        <p>Loading camera matching…</p>
      </section>
    );
  }

  if (status.cameras === 0) {
    return (
      <section className="camera-match">
        <h2>Camera matching</h2>
        <p>No cameras have been matched in this wedding yet.</p>
        <button type="button" onClick={onRunPass} disabled={running}>
          {running ? "Matching…" : "Match cameras"}
        </button>
      </section>
    );
  }

  if (status.cameras === 1) {
    return (
      <section className="camera-match">
        <h2>Camera matching</h2>
        <p>One camera shot this wedding, so there is nothing to match it to.</p>
      </section>
    );
  }

  const measuredHere = status.solvedFromPairs + status.blended;

  return (
    <section className="camera-match">
      <header className="camera-match__header">
        <h2>Camera matching</h2>

        {/* Evidence before number, at the scale of the project. "Four cameras matched" means
            nothing without "three of them from a brand setting". */}
        <dl className="camera-match__evidence">
          <div>
            <dt>Cameras</dt>
            <dd>{status.cameras}</dd>
          </div>
          <div>
            <dt>Matched from this wedding</dt>
            <dd data-thin={measuredHere === 0 ? "true" : undefined}>
              {measuredHere} of {status.cameras}
            </dd>
          </div>
          <div>
            <dt>Overlapping pairs used</dt>
            <dd>
              {status.pairs} used, {status.pairsRejected} rejected
            </dd>
          </div>
          <div>
            <dt>Held back to check</dt>
            <dd>{status.heldoutPairs}</dd>
          </div>
        </dl>

        {status.baselineOnly > 0 && (
          <p className="camera-match__caveat" role="note">
            {status.baselineOnly} correction
            {status.baselineOnly === 1 ? "" : "s"} came from what AURA knows
            about the manufacturer rather than from this wedding
            {status.baselinesMeasured
              ? "."
              : ". Those general settings have not been measured from a photographed colour target in this build, so treat them as a starting point."}
          </p>
        )}

        {status.unknownBrands.length > 0 && (
          <p className="camera-match__caveat" role="note">
            AURA has no measurements for: {status.unknownBrands.join(", ")}.
            Those cameras were left exactly as they were rather than corrected
            by guesswork.
          </p>
        )}

        {/* Never a skin claim while the field is unavailable. A zero here is an unmeasured term,
            not a met promise. */}
        {status.skinFieldAvailable ? (
          <p className="camera-match__skin">
            Skin between cameras was {status.skinDe00Before.toFixed(1)} dE00
            apart and is now {status.skinDe00After.toFixed(1)}. Worst camera:{" "}
            {status.worstSkinDe00.toFixed(1)}.
          </p>
        ) : (
          <p className="camera-match__caveat" role="note">
            Skin was not measured at this wedding, so no claim is made about how
            skin from the different cameras compares.
          </p>
        )}

        <button type="button" onClick={onRunPass} disabled={running}>
          {running ? "Matching…" : "Re-match cameras"}
        </button>
      </header>

      <ol className="camera-match__list">
        {reports.map((report) => {
          const key = `${report.cameraId}/${report.flash}`;
          const transform = transforms.find(
            (row) =>
              row.cameraId === report.cameraId && row.flash === report.flash,
          );
          const expanded = expandedCameraId === key;
          return (
            <li
              key={key}
              className="camera-match__camera"
              data-reference={report.isReference}
            >
              <button
                type="button"
                className="camera-match__row"
                aria-expanded={expanded}
                onClick={() => onExpand(expanded ? null : key)}
              >
                <span className="camera-match__name">
                  {report.shooter ? `${report.shooter} · ` : ""}
                  {report.cameraId || "unidentified camera"}
                </span>
                <span className="camera-match__flash">{report.flash}</span>
                {/* The headline reads the reason set, never the magnitude. */}
                <span className="camera-match__headline">
                  {report.headline}
                </span>
                {transform && !report.isReference && (
                  <span className="camera-match__source">
                    {SOURCE_LABEL[transform.source] ?? transform.source}
                    {transform.evidencePairs > 0 &&
                      ` · ${transform.evidencePairs} pairs`}
                  </span>
                )}
              </button>

              {expanded && (
                <div className="camera-match__detail">
                  <p className="camera-match__evidence-line">
                    {report.evidence}
                  </p>

                  {report.withdrawals.length > 0 && (
                    <ul className="camera-match__withdrawals">
                      {report.withdrawals.map((line) => (
                        <li key={line}>{line}</li>
                      ))}
                    </ul>
                  )}

                  {report.corrections.length > 0 && (
                    <ul className="camera-match__corrections">
                      {report.corrections.map((line) => (
                        <li key={line}>{line}</li>
                      ))}
                    </ul>
                  )}

                  {transform && !report.isReference && (
                    <dl className="camera-match__numbers">
                      <div>
                        <dt>Temperature</dt>
                        <dd>{formatKelvin(transform.dCct)}</dd>
                      </div>
                      <div>
                        <dt>Tint</dt>
                        <dd>{transform.dTint.toFixed(1)}</dd>
                      </div>
                      <div>
                        <dt>Exposure</dt>
                        <dd>{formatStops(transform.dExposure)}</dd>
                      </div>
                      <div>
                        <dt>Saturation</dt>
                        <dd>{transform.dSaturation.toFixed(1)}</dd>
                      </div>
                      <div>
                        <dt>Checked against unused photographs</dt>
                        {/* Three states. `null` is "we could not check", which is not a pass. */}
                        <dd>
                          {transform.heldoutImproved === null
                            ? "not checked - too few spare photographs"
                            : transform.heldoutImproved
                              ? `yes, on ${transform.heldoutPairs}`
                              : `no - the brand setting was used instead`}
                        </dd>
                      </div>
                      {transform.boundedBy && (
                        <div>
                          <dt>Stopped at a limit</dt>
                          <dd>{transform.boundedBy}</dd>
                        </div>
                      )}
                    </dl>
                  )}

                  {pairs.length > 0 && (
                    <table className="camera-match__pairs">
                      <caption>
                        Photographs where both cameras were shooting the same
                        thing. The surroundings are what decides - the people in
                        them differ in exactly the way matching is trying to
                        measure.
                      </caption>
                      <thead>
                        <tr>
                          <th scope="col">Main camera</th>
                          <th scope="col">This camera</th>
                          <th scope="col">Apart</th>
                          <th scope="col">Surroundings agree</th>
                          <th scope="col">Used</th>
                        </tr>
                      </thead>
                      <tbody>
                        {pairs.map((pair) => (
                          <tr key={pair.pairId} data-verified={pair.verified}>
                            <td>{pair.leftPhotoId}</td>
                            <td>{pair.rightPhotoId}</td>
                            <td>{(pair.gapMs / 1000).toFixed(0)} s</td>
                            <td>
                              {(pair.backgroundAgreement * 100).toFixed(0)} %
                            </td>
                            <td>
                              {!pair.verified
                                ? "rejected"
                                : pair.heldOut
                                  ? "held back to check"
                                  : "used"}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}

                  <div className="camera-match__actions">
                    {!report.isReference && (
                      <button
                        type="button"
                        onClick={() => onSetReference(report.cameraId)}
                      >
                        Match everything to this camera instead
                      </button>
                    )}
                    {transform && (
                      <button
                        type="button"
                        onClick={() =>
                          onToggleEnabled(report.cameraId, transform.enabled)
                        }
                      >
                        {transform.enabled
                          ? "Switch matching off"
                          : "Switch matching on"}
                      </button>
                    )}
                  </div>
                </div>
              )}
            </li>
          );
        })}
      </ol>

      {shooterBias.length > 0 && (
        <section className="camera-match__shooters">
          <h3>How each photographer exposes</h3>
          {/* Measured and applied, always both. A panel that showed only the second could not
              explain the cap that is the whole point of section 6.3. */}
          <table>
            <thead>
              <tr>
                <th scope="col">Photographer</th>
                <th scope="col">Kind of photograph</th>
                <th scope="col">How differently they expose</th>
                <th scope="col">How much AURA corrected</th>
              </tr>
            </thead>
            <tbody>
              {shooterBias
                .filter((row) => row.frames > 0)
                .map((row) => (
                  <tr key={`${row.cameraId}/${row.scene}`}>
                    <td>{row.shooter}</td>
                    <td>{row.scene.replace(/_/g, " ")}</td>
                    <td>{formatStops(row.measuredEv)}</td>
                    <td>
                      {formatStops(row.appliedEv)}
                      {row.capped && " (deliberately less than all of it)"}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </section>
      )}
    </section>
  );
}
