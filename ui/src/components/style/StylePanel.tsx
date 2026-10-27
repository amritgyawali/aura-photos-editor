import { useCallback, useEffect, useRef, useState } from 'react';

import { asIpcError, style as styleApi } from '../../ipc/client';
import type {
  ProfileReportDto,
  StyleComparisonDto,
  StyleProfileDto,
  StyleStatusDto,
} from '../../ipc/types';
import { AbCompare } from './AbCompare';
import { ProfileReport } from './ProfileReport';
import { TeachMyAi } from './TeachMyAi';

/**
 * PHASE-17. The container that wires the four style views to the eleven style commands.
 *
 * `TeachMyAi` is the wizard, `ProfileReport` is the honest report a photographer reads before
 * adopting, `BucketMatrix` is the eighty-leaf coverage grid and `AbCompare` is the side-by-side.
 * All four are pure apart from the wizard's own scan and train calls, and none of them was
 * reachable from `main.tsx` before this file (`PHASE-01-30-REVIEW.md` section 6.4).
 *
 * **A profile is adopted from the report, never from the wizard.** Training produces a
 * *candidate*; adopting it is a separate decision made after reading what it can and cannot do,
 * which is why `ProfileReport` owns the adopt button and `TeachMyAi` only reports that a
 * candidate exists. Phase 17's rule that a style is a residual has a matching rule in the
 * interface: the thing a photographer agrees to is the measured improvement, not the fact that
 * a fit converged.
 *
 * **On this build the baseline a residual is measured from is neutral.** Condition C4 of the
 * phase 17 exit report: until an archive can be imported as a project and run through phases 15
 * and 16 first, a learned delta is an absolute edit wearing a residual's shape. The report's own
 * `recommendation` says so; nothing here paraphrases it.
 */
export type StylePanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
  onBusyChange?: (busy: boolean) => void;
};

function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-ML-5100',
    message: ipc?.message ?? 'The style profiles could not be read.',
  };
}

export function StylePanel({ projectId, onError, onBusyChange }: StylePanelProps): JSX.Element {
  const [status, setStatus] = useState<StyleStatusDto | null>(null);
  const [profiles, setProfiles] = useState<StyleProfileDto[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [report, setReport] = useState<ProfileReportDto | null>(null);
  const [comparison, setComparison] = useState<StyleComparisonDto[]>([]);
  const [candidateName, setCandidateName] = useState<string | null>(null);
  const [bucket, setBucket] = useState<string | null>(null);
  const generation = useRef(0);
  const selection = useRef<string | null>(null);
  const operation = useRef(false);
  const [actionBusy, setActionBusy] = useState(false);
  const [trainingBusy, setTrainingBusy] = useState(false);
  useEffect(() => { onBusyChange?.(actionBusy || trainingBusy); return () => onBusyChange?.(false); }, [actionBusy, trainingBusy, onBusyChange]);

  const fail = useCallback(
    (error: unknown) => {
      onError(toBanner(error));
    },
    [onError],
  );

  const reload = useCallback(async (preferredId?: string) => {
    const ticket = ++generation.current;
    try {
      const [nextProfiles, nextStatus] = await Promise.all([
        styleApi.listProfiles(), projectId ? styleApi.styleStatus(projectId) : Promise.resolve(null),
      ]);
      if (ticket !== generation.current) return;
      setProfiles(nextProfiles);
      setStatus(nextStatus);
      // Prefer the project's active profile; fall back to whichever exists.
      const wanted = preferredId ?? selection.current;
      const preferred = nextProfiles.some(profile => profile.profileId === wanted) ? wanted
        : nextProfiles.find(profile => profile.profileId === nextStatus?.active)?.profileId ?? nextProfiles[0]?.profileId ?? null;
      selection.current = preferred;
      setSelected(preferred);
      setReport(null); setBucket(null); setComparison([]);
      if (preferred) {
        const next = await styleApi.profileReport(preferred);
        if (ticket === generation.current) setReport(next);
      }
    } catch (error) {
      if (ticket === generation.current) fail(error);
    }
  }, [fail, projectId]);

  useEffect(() => {
    selection.current = null; setSelected(null); setReport(null); setStatus(null); setComparison([]);
    void reload();
    return () => { generation.current += 1; };
  }, [reload]);

  const compare = useCallback(
    async (candidateId: string) => {
      if (!projectId || operation.current) {
        return;
      }
      operation.current = true; setActionBusy(true);
      const ticket = generation.current;
      try {
        const rows = await styleApi.compareProfiles({ projectId, candidateId });
        if (ticket !== generation.current) return;
        setComparison(rows);
        setCandidateName(
          profiles.find((profile) => profile.profileId === candidateId)?.name ?? candidateId,
        );
        onError(null);
      } catch (error) {
        if (ticket === generation.current) fail(error);
      } finally { operation.current = false; setActionBusy(false); }
    },
    [fail, onError, profiles, projectId],
  );

  const adopt = useCallback(
    async (profileId: string) => {
      if (operation.current) return;
      operation.current = true; setActionBusy(true);
      const ticket = generation.current;
      try {
        await styleApi.adoptProfile({ profileId });
        if (projectId) {
          await styleApi.setProjectProfile({ projectId, profileId });
        }
        if (ticket !== generation.current) return;
        onError(null);
        await reload(profileId);
      } catch (error) {
        if (ticket === generation.current) fail(error);
      } finally { operation.current = false; setActionBusy(false); }
    },
    [fail, onError, projectId, reload],
  );

  return (
    <section className="style-panel" aria-label="Teach my AI">
      <header className="style-panel__header">
        <h2>Your look</h2>
        {status ? (
          <p>
            {status.profiles === 0
              ? 'AURA has not learned your look yet. Point it at a wedding you have already finished.'
              : `${status.activeName} v${status.activeVersion}, learned from ${status.trainedPairs.toLocaleString()} pairs.`}
          </p>
        ) : null}
      </header>

      {profiles.length > 1 ? (
        <label className="style-panel__pick">
          Profile
          <select
            value={selected ?? ''}
            disabled={actionBusy || trainingBusy}
            onChange={(event) => {
              const next = event.target.value || null;
              selection.current = next; setSelected(next); setReport(null); setBucket(null); setComparison([]);
              if (next) {
                void reload(next);
              }
            }}
          >
            {profiles.map((profile) => (
              <option key={profile.profileId} value={profile.profileId}>
                {profile.name} v{profile.version} ({profile.status})
              </option>
            ))}
          </select>
        </label>
      ) : null}

      <TeachMyAi
        disabled={actionBusy}
        key={projectId ?? 'no-project'}
        projectId={projectId}
        onBusyChange={setTrainingBusy}
        onTrained={(profileId) => {
          void reload(profileId);
        }}
        onError={(message) => onError(message ? { code: 'AURA-ML-5100', message } : null)}
      />

      <ProfileReport
        report={report}
        disabled={actionBusy || trainingBusy}
        onAdopt={(profileId) => void adopt(profileId)}
        onCompare={(profileId) => void compare(profileId)}
        onSelectBucket={(key) => setBucket(key)}
      />

      {bucket ? (
        <p className="style-panel__bucket">
          {report?.perBucket.find((row) => row.key === bucket)?.title ?? bucket}:{' '}
          {report?.perBucket.find((row) => row.key === bucket)?.matchDe00 === null
            ? 'nothing was held out here, so there is no measurement.'
            : `${report?.perBucket.find((row) => row.key === bucket)?.matchDe00?.toFixed(2)} dE00 on held-out photographs.`}
        </p>
      ) : null}

      {comparison.length > 0 ? (
        <AbCompare
          rows={comparison}
          currentName={status?.activeName}
          candidateName={candidateName ?? undefined}
        />
      ) : null}
    </section>
  );
}
