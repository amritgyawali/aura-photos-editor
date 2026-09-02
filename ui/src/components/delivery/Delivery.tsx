import type {
  ConsentDto,
  DeliveryManifestDto,
  DeliveryStatusDto,
  DiagnosticsDto,
  ExportFileDto,
  ExportNameDto,
  ExportPresetDto,
  ExportStatusDto,
  LearnBucketDto,
  LearnComparisonDto,
  LearnStatusDto,
  ProviderDto,
  UploadItemDto,
} from '../../ipc/types';

/**
 * PHASE-30. The four screens of the last phase: export, delivery, learning and diagnostics.
 *
 * Pure - everything arrives as props and every action leaves as a callback - so the whole surface
 * is testable without a window, which is the split phase 25 established and every panel since has
 * followed.
 *
 * ## What these four have to be honest about, and none of the twenty-nine before them did
 *
 * **The export button writes files.** Every other button in this product changes a row. So the
 * dialog shows the names *before* the job runs, and the header afterwards carries three
 * denominators, because an album export is 80 frames out of a gallery of 700 out of a project of
 * 4,000 and reporting it against the project would call a job that did exactly what it was asked
 * to a 98 % failure.
 *
 * **"Not verified" and "verification failed" are opposite facts.** `unverified` and `corrupt` are
 * separate counts on the header and are rendered in different colours, because the first is a
 * choice somebody made and the second is a drive that should be replaced.
 *
 * **This build cannot reach a gallery.** `networkAvailable` is false and the delivery screen says
 * so above the provider list, rather than letting a photographer configure a provider, press
 * upload, see nothing happen and conclude their credentials are wrong.
 *
 * **Nothing about learning happens without a click.** The review shows what an update would do,
 * on corrections the fit never saw, and the adopt button is the only thing in the product that
 * moves a profile forward.
 */

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export type ExportViewProps = {
  /** The header, or null while it loads. */
  status: ExportStatusDto | null;
  /** The presets the dialog offers, each with the argument for it. */
  presets: ExportPresetDto[];
  /** Which preset is selected. */
  selected: string;
  /** Where the files would go. */
  destination: string;
  /** Whether the read-back runs. */
  verify: boolean;
  /** The names a dry run produced, or null when none has been asked for. */
  names: ExportNameDto[] | null;
  /** Whether a job is running. */
  running: boolean;
  onSelectPreset: (name: string) => void;
  onDestination: (path: string) => void;
  onVerify: (on: boolean) => void;
  onPreviewNames: () => void;
  onRun: () => void;
};

export function ExportView(props: ExportViewProps) {
  const { status, presets, selected, destination, verify, names, running } = props;
  const preset = presets.find((p) => p.name === selected) ?? null;

  return (
    <section className="export-view" aria-label="Export">
      <h2>Export</h2>

      {status && (
        <dl className="export-header">
          {/* Three denominators. A panel that measured an album export against the project would
              report a job that did exactly what it was asked to as having missed most of a
              wedding. */}
          <div>
            <dt>Asked for</dt>
            <dd data-testid="requested">{status.requested}</dd>
          </div>
          <div>
            <dt>Written</dt>
            <dd data-testid="written">{status.written}</dd>
          </div>
          <div>
            <dt>Checked</dt>
            <dd data-testid="verified">{status.verified}</dd>
          </div>
          {/* Separate counts, deliberately: "not checked" is a choice somebody made and "the check
              failed" is a drive that should be replaced. */}
          {status.unverified > 0 && (
            <div className="caveat">
              <dt>Not checked</dt>
              <dd data-testid="unverified">{status.unverified}</dd>
            </div>
          )}
          {status.corrupt > 0 && (
            <div className="fault">
              <dt>Failed the check</dt>
              <dd data-testid="corrupt">{status.corrupt}</dd>
            </div>
          )}
          <div>
            <dt>In this project</dt>
            <dd data-testid="photos">{status.photos}</dd>
          </div>
          <div>
            <dt>In the gallery</dt>
            <dd data-testid="selected">{status.selected}</dd>
          </div>
        </dl>
      )}

      {status && !status.manifestSealed && status.written > 0 && (
        <p className="caveat" data-testid="no-manifest">
          This delivery has no manifest, because part of it did not finish. Run the export again;
          what is already written is not re-rendered.
        </p>
      )}

      <label>
        Set
        <select
          value={selected}
          onChange={(e) => props.onSelectPreset(e.target.value)}
          data-testid="preset"
        >
          {presets.map((p) => (
            <option key={p.name} value={p.name}>
              {p.name}
            </option>
          ))}
        </select>
      </label>

      {/* The argued-over half. A preset nobody can explain is a preset nobody can argue with. */}
      {preset && (
        <p className="reason" data-testid="preset-reason">
          {preset.reason}
        </p>
      )}

      <label>
        Into
        <input
          type="text"
          value={destination}
          onChange={(e) => props.onDestination(e.target.value)}
          data-testid="destination"
        />
      </label>

      <label>
        <input
          type="checkbox"
          checked={verify}
          onChange={(e) => props.onVerify(e.target.checked)}
          data-testid="verify"
        />
        Read every file back and check it
      </label>
      {!verify && (
        <p className="caveat" data-testid="verify-off">
          Without this, AURA cannot tell you whether what landed on the drive is what it sent. The
          delivery will say so, on every file and in its manifest.
        </p>
      )}

      <div className="actions">
        <button type="button" onClick={props.onPreviewNames} data-testid="preview-names">
          Show me the file names
        </button>
        <button
          type="button"
          onClick={props.onRun}
          disabled={running || destination.trim() === ''}
          data-testid="run"
        >
          {running ? 'Exporting…' : 'Export'}
        </button>
      </div>

      {names && (
        <div className="names" data-testid="names">
          <h3>{names.length} files</h3>
          <ol>
            {names.slice(0, 20).map((n) => (
              <li key={`${n.set}/${n.path}`} className={n.renamed ? 'renamed' : undefined}>
                <code>{n.path}</code>
                {n.reasons.map((r) => (
                  <span key={r.code} className="reason">
                    {r.text}
                  </span>
                ))}
              </li>
            ))}
          </ol>
          {names.length > 20 && <p>…and {names.length - 20} more.</p>}
        </div>
      )}
    </section>
  );
}

// ---------------------------------------------------------------------------
// The manifest and what was written
// ---------------------------------------------------------------------------

export type ManifestViewProps = {
  manifest: DeliveryManifestDto | null;
  files: ExportFileDto[];
};

export function ManifestView({ manifest, files }: ManifestViewProps) {
  if (!manifest) {
    return (
      <section aria-label="Delivery manifest">
        <h2>Delivery manifest</h2>
        {/* Null is not an empty manifest, and the sentence says which of the two this is. */}
        <p data-testid="no-manifest">This wedding has not been delivered yet.</p>
      </section>
    );
  }

  return (
    <section aria-label="Delivery manifest">
      <h2>Delivery manifest</h2>
      <p data-testid="manifest-summary">
        {manifest.files} files, {Math.round(manifest.bytes / 1_000_000)} MB.
      </p>
      {!manifest.fullyHashed && (
        <p className="caveat" data-testid="not-fully-hashed">
          Some files in this manifest carry no checksum, which means they were written without the
          read-back check.
        </p>
      )}
      {manifest.cleanupDisclosures.length > 0 && (
        <div data-testid="disclosures">
          <h3>What was removed</h3>
          <ul>
            {manifest.cleanupDisclosures.map(([image, what]) => (
              <li key={`${image}-${what}`}>{what}</li>
            ))}
          </ul>
        </div>
      )}
      <ul className="files">
        {files.slice(0, 50).map((f) => (
          <li key={f.path} className={f.verified ? 'verified' : 'unverified'}>
            <code>{f.path}</code>
            <span>
              {f.width}×{f.height}
            </span>
            {!f.verified && <span className="caveat">not checked</span>}
          </li>
        ))}
      </ul>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Delivery
// ---------------------------------------------------------------------------

export type DeliveryViewProps = {
  status: DeliveryStatusDto | null;
  providers: ProviderDto[];
  items: UploadItemDto[];
  backupPath: string;
  onBackupPath: (path: string) => void;
  onBackup: () => void;
  onUpload: (provider: string) => void;
};

export function DeliveryView(props: DeliveryViewProps) {
  const { status, providers, items, backupPath } = props;

  return (
    <section aria-label="Delivery">
      <h2>Backup and galleries</h2>

      {/* The caveat comes first. A photographer who configures a provider, presses upload and sees
          nothing happen would otherwise conclude their credentials are wrong. */}
      {status && !status.networkAvailable && (
        <p className="caveat" data-testid="no-network">
          This build cannot reach a client gallery over the internet. Backups to a drive or a
          network share work exactly as they say; a gallery upload does not.
        </p>
      )}

      {status && (
        <dl>
          <div>
            <dt>Backed up</dt>
            <dd data-testid="backed-up">{status.backedUp}</dd>
          </div>
          {status.diverged > 0 && (
            <div className="fault">
              <dt>Different in the backup</dt>
              <dd data-testid="diverged">{status.diverged}</dd>
            </div>
          )}
          <div>
            <dt>Uploaded</dt>
            <dd data-testid="uploaded">{status.uploaded}</dd>
          </div>
          <div>
            <dt>Still to go</dt>
            <dd data-testid="outstanding">{status.outstanding}</dd>
          </div>
          {status.resumes > 0 && (
            <div>
              <dt>Resumed</dt>
              <dd data-testid="resumes">{status.resumes}</dd>
            </div>
          )}
          {status.unmappedSets > 0 && (
            <div className="caveat">
              <dt>Sets with nowhere to go</dt>
              <dd data-testid="unmapped">{status.unmappedSets}</dd>
            </div>
          )}
        </dl>
      )}

      {status && status.diverged > 0 && (
        <p className="fault" data-testid="diverged-warning">
          A file in the backup is different from the one it was copied from. Nothing was
          overwritten and the backup stopped. Check that drive before trusting anything on it.
        </p>
      )}

      <label>
        Back up to
        <input
          type="text"
          value={backupPath}
          onChange={(e) => props.onBackupPath(e.target.value)}
          data-testid="backup-path"
        />
      </label>
      <button
        type="button"
        onClick={props.onBackup}
        disabled={backupPath.trim() === ''}
        data-testid="backup"
      >
        Back up
      </button>

      <h3>Client galleries</h3>
      <ul data-testid="providers">
        {providers.map((p) => (
          <li key={p.id}>
            {p.label}
            {p.hasCredential ? (
              <button type="button" onClick={() => props.onUpload(p.id)}>
                Upload
              </button>
            ) : (
              <span className="caveat">no sign-in saved</span>
            )}
          </li>
        ))}
      </ul>

      {items.length > 0 && (
        <ul className="upload-items" data-testid="items">
          {items.slice(0, 30).map((item) => (
            <li key={item.path} className={item.state}>
              <code>{item.path}</code>
              <span>{stateWord(item.state)}</span>
              {item.resumes > 0 && <span>resumed {item.resumes}×</span>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** The sentence for an upload state. `corrupt` and `failed` read differently on purpose. */
export function stateWord(state: string): string {
  switch (state) {
    case 'verified':
      return 'arrived and checked';
    case 'in_progress':
      return 'part way';
    case 'corrupt':
      return 'arrived wrong — will be sent again';
    case 'failed':
      return 'did not arrive';
    default:
      return 'waiting';
  }
}

// ---------------------------------------------------------------------------
// Learning
// ---------------------------------------------------------------------------

export type LearningViewProps = {
  status: LearnStatusDto | null;
  buckets: LearnBucketDto[];
  comparison: LearnComparisonDto | null;
  consent: ConsentDto | null;
  onAdopt: () => void;
  onRollBack: () => void;
  onConsent: (next: ConsentDto) => void;
};

export function LearningView(props: LearningViewProps) {
  const { status, buckets, comparison, consent } = props;

  return (
    <section aria-label="Learning">
      <h2>What AURA has learned from you</h2>

      {status && !status.fittedOnRealCorrections && (
        <p className="caveat" data-testid="not-fitted">
          Nothing here has been trained on a real photographer&rsquo;s archive yet. The arithmetic is
          real; the numbers are about corrections this build authored.
        </p>
      )}

      {status && (
        <dl>
          <div>
            <dt>Corrections</dt>
            <dd data-testid="corrections">{status.corrections}</dd>
          </div>
          <div>
            <dt>Weddings</dt>
            <dd data-testid="projects">{status.projects}</dd>
          </div>
          <div>
            <dt>Ready to act on</dt>
            <dd data-testid="actionable">{status.actionableBuckets}</dd>
          </div>
          {status.unattributed > 0 && (
            <div className="caveat">
              <dt>Kept, not learned from</dt>
              <dd data-testid="unattributed">{status.unattributed}</dd>
            </div>
          )}
        </dl>
      )}

      {status && status.corrections > 0 && status.actionableBuckets === 0 && (
        <p data-testid="waiting">
          AURA is waiting to see the same thing again. It acts on a change only once several
          corrections from more than one wedding agree about it.
        </p>
      )}

      <ul className="buckets" data-testid="buckets">
        {buckets.map((b) => (
          <li key={`${b.learnable}-${b.scene}-${String(b.subjectClose)}`}>
            <strong>{b.label}</strong>
            <span>{b.scene.replace(/_/g, ' ')}</span>
            <span>{b.corrections} corrections</span>
            <span>{b.projects} weddings</span>
            {/* Shown, so a photographer can see the loop ignored their extreme fixes rather than
                wonder why nothing moved. */}
            {b.outliersDropped > 0 && (
              <span className="caveat">{b.outliersDropped} left out as unusual</span>
            )}
            {!b.actionable && <span className="caveat">not enough yet</span>}
          </li>
        ))}
      </ul>

      {comparison && (
        <div className="comparison" data-testid="comparison">
          <h3>
            Version {comparison.currentVersion} → {comparison.candidateVersion}
          </h3>
          <p data-testid="improvement">
            Tested against {comparison.heldOut} corrections it had not seen, the new version was{' '}
            {(comparison.improvement * 100).toFixed(0)}% closer.
          </p>
          <ul>
            {comparison.rows.map((r) => (
              <li key={`${r.learnable}-${r.scene}`}>{r.summary}</li>
            ))}
          </ul>
          {comparison.reasons.map((r) => (
            <p key={r.code} className="reason">
              {r.text}
            </p>
          ))}
          <button
            type="button"
            onClick={props.onAdopt}
            disabled={!comparison.offerable}
            data-testid="adopt"
          >
            Adopt this
          </button>
          <button type="button" onClick={props.onRollBack} data-testid="roll-back">
            Go back to the previous version
          </button>
        </div>
      )}

      {consent && (
        <fieldset data-testid="consent">
          <legend>What AURA may do with this wedding</legend>
          <label>
            <input
              type="checkbox"
              checked={consent.localLearning}
              onChange={(e) =>
                props.onConsent({ ...consent, localLearning: e.target.checked })
              }
              data-testid="consent-learning"
            />
            Learn from my corrections on this wedding
          </label>
          <label>
            <input
              type="checkbox"
              checked={consent.datasetContribution}
              onChange={(e) =>
                props.onConsent({ ...consent, datasetContribution: e.target.checked })
              }
              data-testid="consent-dataset"
            />
            Share anonymised corrections to improve AURA for everyone
          </label>
          {/* Two switches and not one, because "may this machine learn" and "may evidence leave
              it" are different questions and collapsing them is how the second happens by
              accident. */}
          {!consent.anythingLeaves && (
            <p data-testid="nothing-leaves">Nothing about this wedding leaves your machine.</p>
          )}
        </fieldset>
      )}
    </section>
  );
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

export type DiagnosticsViewProps = {
  report: DiagnosticsDto | null;
};

export function DiagnosticsView({ report }: DiagnosticsViewProps) {
  if (!report) {
    return <section aria-label="Diagnostics">Loading…</section>;
  }

  /* Leads with what is not working, because a support call starts with somebody reading this down
     a telephone and the useful half is the half that says what this machine cannot do. */
  const caveats: string[] = [];
  if (report.renderDegradation) {
    caveats.push(report.renderDegradation);
  }
  if (!report.networkTransport) {
    caveats.push('This build cannot upload to a client gallery.');
  }
  if (!report.trainedModels) {
    caveats.push(
      'No model in this build is trained. Every measurement is real; every model output is a placeholder.',
    );
  }
  for (const stage of report.stagesOff) {
    caveats.push(`${stage} is switched off.`);
  }

  return (
    <section aria-label="Diagnostics">
      <h2>Diagnostics</h2>

      <ul className="caveats" data-testid="caveats">
        {caveats.map((c) => (
          <li key={c} className="caveat">
            {c}
          </li>
        ))}
      </ul>

      <dl>
        <div>
          <dt>Version</dt>
          <dd data-testid="app-version">{report.appVersion}</dd>
        </div>
        <div>
          <dt>Catalog</dt>
          <dd data-testid="schema">schema {report.schemaVersion}</dd>
        </div>
        <div>
          <dt>Rendering on</dt>
          <dd data-testid="backend">{report.renderBackend}</dd>
        </div>
        <div>
          <dt>Model set</dt>
          <dd data-testid="model-set">{report.modelSet.slice(0, 12)}</dd>
        </div>
      </dl>

      {report.recentErrors.length > 0 && (
        <ul data-testid="recent-errors">
          {report.recentErrors.map((e) => (
            <li key={e.code}>
              <code>{e.code}</code> {e.message}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
