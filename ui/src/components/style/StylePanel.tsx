import { useCallback, useEffect, useState } from 'react';

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
};

function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-ML-5100',
    message: ipc?.message ?? 'The style profiles could not be read.',
  };
}

export function StylePanel({ projectId, onError }: StylePanelProps): JSX.Element {
  const [status, setStatus] = useState<StyleStatusDto | null>(null);
  const [profiles, setProfiles] = useState<StyleProfileDto[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [report, setReport] = useState<ProfileReportDto | null>(null);
  const [comparison, setComparison] = useState<StyleComparisonDto[]>([]);
  const [candidateName, setCandidateName] = useState<string | null>(null);
  const [bucket, setBucket] = useState<string | null>(null);

  const fail = useCallback(
    (error: unknown) => {
      onError(toBanner(error));
    },
    [onError],
  );

  const reload = useCallback(async () => {
    try {
      const nextProfiles = await styleApi.listProfiles();
      setProfiles(nextProfiles);
      if (projectId) {
        setStatus(await styleApi.styleStatus(projectId));
      } else {
        setStatus(null);
      }
      // Prefer the project's active profile; fall back to whichever exists.
      const preferred = selected ?? status?.active ?? nextProfiles[0]?.profileId ?? null;
      if (preferred) {
        setSelected(preferred);
        setReport(await styleApi.profileReport(preferred));
      } else {
        setReport(null);
      }
    } catch (error) {
      fail(error);
    }
    // `status` is deliberately not a dependency: it is written by this callback, and reading
    // the previous value to choose a default is what makes the first load pick the active
    // profile rather than the first one in the list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fail, projectId, selected]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const compare = useCallback(
    async (candidateId: string) => {
      if (!projectId) {
        return;
      }
      try {
        setComparison(await styleApi.compareProfiles({ projectId, candidateId }));
        setCandidateName(
          profiles.find((profile) => profile.profileId === candidateId)?.name ?? candidateId,
        );
        onError(null);
      } catch (error) {
        fail(error);
      }
    },
    [fail, onError, profiles, projectId],
  );

  const adopt = useCallback(
    async (profileId: string) => {
      try {
        await styleApi.adoptProfile({ profileId });
        if (projectId) {
          await styleApi.setProjectProfile({ projectId, profileId });
        }
        onError(null);
        setSelected(profileId);
        await reload();
      } catch (error) {
        fail(error);
      }
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
            onChange={(event) => {
              const next = event.target.value || null;
              setSelected(next);
              if (next) {
                void styleApi.profileReport(next).then(setReport).catch(fail);
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
        projectId={projectId}
        onTrained={(profileId) => {
          setSelected(profileId);
          void reload();
        }}
        onError={(message) => onError({ code: 'AURA-ML-5100', message })}
      />

      <ProfileReport
        report={report}
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
