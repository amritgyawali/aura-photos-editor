/**
 * PHASE-17. What a photographer is told about the profile they just trained.
 *
 * Section 6.3: "this is the number shown in the UI, not a vague 'profile ready'". So the panel
 * leads with a measurement and a strength meter rather than with a status, and it says the two
 * things a status would hide:
 *
 * * **How many pairs were left out.** A run at 91 % acceptance is healthy; a run at 30 % means
 *   the archive contains work AURA cannot model, and the photographer should know before they
 *   ask why it does not look like them.
 * * **Which buckets are weak, and what to add.** The recommendation is generated on the Rust
 *   side from the actual gap, so the panel, the CLI report and the exit report say the same
 *   sentence.
 *
 * It never says "verified" about an imported bundle. What a signature proves here is that the
 * document has not changed since it was signed by whoever holds the key inside it - integrity,
 * not provenance - and `ProfileReport.test.tsx` asserts the word never appears.
 */
import type { ImportProfileDto, ProfileReportDto } from '../../ipc/types';

import { BucketMatrix } from './BucketMatrix';

export type ProfileReportProps = {
  report: ProfileReportDto | null;
  imported?: ImportProfileDto | null;
  onAdopt?: (profileId: string) => void;
  onCompare?: (profileId: string) => void;
};

/** The strength meter's label. A number and a word, never a word alone. */
export function strengthLabel(strength: number): string {
  const percent = Math.round(strength * 100);
  if (strength >= 0.75) return `${percent}% — strong`;
  if (strength >= 0.45) return `${percent}% — usable`;
  if (strength > 0) return `${percent}% — still learning`;
  return `${percent}% — nothing learned yet`;
}

/** What the panel says about a bundle's signature. Never the word "verified". */
export function signatureLabel(imported: ImportProfileDto): string {
  return imported.unchangedSinceSigning
    ? `unchanged since signing · key ${imported.fingerprint}`
    : `this file could not be checked · key ${imported.fingerprint}`;
}

export function ProfileReport({ report, imported, onAdopt, onCompare }: ProfileReportProps) {
  if (!report) {
    return (
      <section className="profile-report empty" aria-label="Style profile report">
        <p>
          No profile has been trained yet. Point AURA at a folder of weddings you have already
          finished, and it will learn how you edit them.
        </p>
      </section>
    );
  }

  const { profile } = report;
  const accepted = report.acceptedPairs;
  const rejected = report.rejectedPairs;

  return (
    <section className="profile-report" aria-label="Style profile report">
      <header>
        <h2>
          {profile.name} <span className="profile-version">v{profile.version}</span>
        </h2>
        <p className="profile-status">{profile.status}</p>
      </header>

      <dl className="profile-figures">
        <div>
          <dt>How close it gets to your own edits</dt>
          <dd>
            {report.metCeiling
              ? `${profile.overallDe00.toFixed(1)} dE00 — inside the target`
              : `${profile.overallDe00.toFixed(1)} dE00 — outside the target`}
          </dd>
        </div>
        <div>
          <dt>Strength</dt>
          <dd>{strengthLabel(profile.strength)}</dd>
        </div>
        <div>
          <dt>Photographs learned from</dt>
          <dd>
            {accepted.toLocaleString()} used, {rejected.toLocaleString()} left out (
            {Math.round(report.acceptance * 100)}% used)
          </dd>
        </div>
        <div>
          <dt>Kinds of photograph it knows</dt>
          <dd>{profile.taughtBuckets}</dd>
        </div>
      </dl>

      <p className="profile-recommendation">{report.recommendation}</p>

      {imported ? <p className="profile-signature">{signatureLabel(imported)}</p> : null}

      <BucketMatrix buckets={report.perBucket} />

      <footer className="profile-actions">
        <button type="button" onClick={onCompare ? () => onCompare(profile.profileId) : undefined}>
          Compare with what you use now
        </button>
        <button
          type="button"
          disabled={profile.status === 'adopted'}
          onClick={onAdopt ? () => onAdopt(profile.profileId) : undefined}
        >
          {profile.status === 'adopted' ? 'In use' : 'Use this profile'}
        </button>
      </footer>
      <p className="profile-caveat">
        Adopting a profile changes how AURA edits from now on. It never changes a setting you
        made yourself, and it can never move somebody&apos;s skin colour further than AURA
        allows — the skin check runs after your style is applied, not before.
      </p>
    </section>
  );
}
