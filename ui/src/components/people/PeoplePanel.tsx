import { useCallback, useEffect, useState } from 'react';

import { api, asIpcError, inTauri } from '../../ipc/client';
import type {
  GroupPeopleDto,
  IdentityCardDto,
  PeopleStatusDto,
  ScanFacesDto,
} from '../../ipc/types';
import { IdentityCard } from './IdentityCard';
import { MergeDialog } from './MergeDialog';

/**
 * One sentence about the people pass, including every reason the panel might be empty.
 *
 * The phase 05 rule inherited: report coverage when you report a result. An empty People
 * panel on an unscanned wedding is a support ticket, and this is the sentence that
 * answers it before it is filed.
 */
export function coverageSentence(status: PeopleStatusDto): string {
  if (status.erased) {
    return 'This wedding’s face data has been erased. Culling and editing decisions were kept.';
  }
  if (status.photos === 0) {
    return 'This project has no photographs yet.';
  }
  if (status.scanned === 0) {
    return `None of the ${status.photos} photographs have been read for faces yet.`;
  }
  const percent = Math.round(status.coverage * 100);
  const stale =
    status.staleVersions.length > 0
      ? ' Some faces were read by an older model and are being re-read in the background.'
      : '';
  return `${status.scanned} of ${status.photos} photographs read (${percent}%), ${status.faces} faces found, ${status.identities} people grouped.${stale}`;
}

/**
 * How many faces were found but excluded from grouping, and why that is not a fault.
 *
 * Section 6.1: a face below the gate is detected, stored and displayed, and does not
 * vote on identity. The distinction is invisible unless the panel says it out loud, and
 * a photographer who cannot see it will read "3,100 faces, 12 people" as a bug.
 */
export function gatedSentence(status: PeopleStatusDto): string | null {
  const gated = status.faces - status.votingFaces;
  if (gated <= 0) {
    return null;
  }
  return `${gated} of ${status.faces} faces are too small, too soft or too turned away to identify. They still count towards how many people are in a photograph.`;
}

/** What the tiled detection pass cost, as a sentence. Section 12's fifth failure mode. */
export function tileSentence(status: PeopleStatusDto): string | null {
  if (status.tiledFrames === 0 || status.scanned === 0) {
    return null;
  }
  const percent = Math.round((status.tiledFrames / status.scanned) * 100);
  return `${status.tiledFrames} wide frames (${percent}%) were scanned a second time in tiles to find small faces.`;
}

/**
 * The couple prompt, when there is one to make.
 *
 * This is the most important sentence in the panel. Section 12's first failure mode is
 * "wrong couple identification poisons every later decision", and the mitigation is an
 * explicit confirmation - so the prompt is a call to action rather than a status line,
 * and it says why it matters.
 */
export function couplePrompt(
  status: PeopleStatusDto,
  grouping: GroupPeopleDto | null,
): string | null {
  if (!status.coupleUnconfirmed || status.identities === 0) {
    return null;
  }
  if (grouping?.coupleAmbiguous === true) {
    return 'AURA is not sure which two people are the couple. Two pairs scored almost the same. Confirm below - every later decision depends on this.';
  }
  if (grouping?.sceneStarved === true) {
    return 'AURA has suggested a couple from who appears together most often. It cannot yet tell a getting-ready frame from a ceremony, so please confirm below.';
  }
  return 'Confirm who the couple are. Every later decision weights their faces above everybody else’s.';
}

/** What one face pass did, as a sentence. */
export function scanSentence(scan: ScanFacesDto): string {
  const parts = [`read ${scan.scanned} photographs`, `found ${scan.faces} faces`];
  if (scan.headless > 0) {
    parts.push(`${scan.headless} people with no visible face`);
  }
  if (scan.failed > 0) {
    parts.push(`${scan.failed} could not be read`);
  }
  if (scan.remaining > 0) {
    parts.push(`${scan.remaining} still to do`);
  }
  if (scan.cancelled) {
    parts.push('stopped early');
  }
  return `${parts.join(', ')} in ${(scan.elapsedMs / 1000).toFixed(1)} s.`;
}

/** What one grouping pass did, as a sentence. */
export function groupSentence(group: GroupPeopleDto): string {
  const parts = [
    `${group.identities} people`,
    `${group.assigned} faces grouped`,
  ];
  if (group.unassigned > 0) {
    parts.push(`${group.unassigned} left unassigned rather than guessed at`);
  }
  if (group.refusedMerges > 0) {
    parts.push(`${group.refusedMerges} risky merge(s) refused`);
  }
  if (group.decisionsReplayed > 0) {
    parts.push(`${group.decisionsReplayed} of your decisions kept`);
  }
  if (group.decisionsOrphaned > 0) {
    parts.push(`${group.decisionsOrphaned} of your decisions could not be reapplied`);
  }
  return `${parts.join(', ')} in ${(group.elapsedMs / 1000).toFixed(1)} s.`;
}

export type PeoplePanelProps = {
  projectId: string;
  onError?: (error: { code: string; message: string }) => void;
};

/**
 * The People panel.
 *
 * Section 2.1 asks for merge, split and rename, a one-click "this is the bride", and a
 * per-identity importance slider. All five are here, plus the two things that make them
 * safe to use: the coverage sentence, so an empty panel explains itself, and the erase
 * control, which is behind a typed confirmation because it is the one operation in the
 * product with no undo.
 *
 * Every number shown here is computed in Rust. The panel formats and does not decide -
 * which is why `coverageSentence`, `gatedSentence`, `couplePrompt` and the rest are
 * exported: they are the panel's whole logic, and they are tested directly.
 */
export function PeoplePanel({ projectId, onError }: PeoplePanelProps): JSX.Element {
  const [status, setStatus] = useState<PeopleStatusDto | null>(null);
  const [identities, setIdentities] = useState<IdentityCardDto[]>([]);
  const [scan, setScan] = useState<ScanFacesDto | null>(null);
  const [grouping, setGrouping] = useState<GroupPeopleDto | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [mergeFrom, setMergeFrom] = useState<string | null>(null);
  const [eraseConfirm, setEraseConfirm] = useState('');
  const [busy, setBusy] = useState(false);

  const report = useCallback(
    (error: unknown) => {
      const typed = asIpcError(error);
      onError?.({ code: typed.code, message: typed.message });
    },
    [onError],
  );

  const refresh = useCallback(() => {
    if (!inTauri()) {
      return;
    }
    api.peopleStatus(projectId).then(setStatus).catch(report);
    api.listIdentities(projectId).then(setIdentities).catch(report);
  }, [projectId, report]);

  useEffect(refresh, [refresh]);

  const runScan = useCallback(() => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    api
      .scanFaces({ projectId })
      .then((pass) => {
        setScan(pass);
        return api.groupPeople({ projectId });
      })
      .then((group) => {
        setGrouping(group);
        refresh();
      })
      .catch(report)
      .finally(() => setBusy(false));
  }, [projectId, refresh, report]);

  const runGroup = useCallback(() => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    api
      .groupPeople({ projectId })
      .then((group) => {
        setGrouping(group);
        refresh();
      })
      .catch(report)
      .finally(() => setBusy(false));
  }, [projectId, refresh, report]);

  const setRole = useCallback(
    (identityId: string, role: string) => {
      if (!inTauri()) {
        return;
      }
      api
        .setIdentityRole({ identityId, role })
        .then(refresh)
        .catch(report);
    },
    [refresh, report],
  );

  const rename = useCallback(
    (identityId: string, label: string | null) => {
      if (!inTauri()) {
        return;
      }
      api.renameIdentity({ identityId, label }).then(refresh).catch(report);
    },
    [refresh, report],
  );

  const setImportance = useCallback(
    (identityId: string, importance: number) => {
      if (!inTauri()) {
        return;
      }
      api
        .setIdentityImportance({ identityId, importance })
        .then(refresh)
        .catch(report);
    },
    [refresh, report],
  );

  const merge = useCallback(
    (survivorId: string, absorbedId: string) => {
      if (!inTauri()) {
        return;
      }
      setMergeFrom(null);
      api
        .mergeIdentities({ a: survivorId, b: absorbedId })
        .then(refresh)
        .catch(report);
    },
    [refresh, report],
  );

  const erase = useCallback(() => {
    if (!inTauri()) {
      return;
    }
    setBusy(true);
    api
      .eraseBiometrics({ projectId, confirm: eraseConfirm })
      .then(() => {
        setIdentities([]);
        setScan(null);
        setGrouping(null);
        setEraseConfirm('');
        refresh();
      })
      .catch(report)
      .finally(() => setBusy(false));
  }, [projectId, eraseConfirm, refresh, report]);

  const source = identities.find((identity) => identity.id === mergeFrom) ?? null;
  const prompt = status === null ? null : couplePrompt(status, grouping);

  return (
    <section className="people-panel" aria-label="People">
      <h2>People</h2>

      {status === null ? (
        <p>Reading this wedding&rsquo;s people&hellip;</p>
      ) : (
        <>
          <p className="coverage">{coverageSentence(status)}</p>
          {gatedSentence(status) !== null && <p className="gated">{gatedSentence(status)}</p>}
          {tileSentence(status) !== null && <p className="tiles">{tileSentence(status)}</p>}
        </>
      )}

      {prompt !== null && (
        <p className="couple-prompt" role="alert">
          {prompt}
        </p>
      )}

      <div className="controls">
        <button type="button" onClick={runScan} disabled={busy}>
          Find people in this wedding
        </button>
        <button type="button" onClick={runGroup} disabled={busy}>
          Group again
        </button>
      </div>

      {scan !== null && <p className="pass">{scanSentence(scan)}</p>}
      {grouping !== null && <p className="grouping">{groupSentence(grouping)}</p>}

      {identities.length === 0 ? (
        <p>No people grouped yet.</p>
      ) : (
        <ul className="identities">
          {identities.map((identity) => (
            <IdentityCard
              key={identity.id}
              projectId={projectId}
              identity={identity}
              selected={identity.id === selected}
              onSelect={setSelected}
              onSetRole={setRole}
              onRename={rename}
              onSetImportance={setImportance}
              onMergeWith={setMergeFrom}
            />
          ))}
        </ul>
      )}

      {source !== null && (
        <MergeDialog
          source={source}
          candidates={identities}
          onCancel={() => setMergeFrom(null)}
          onConfirm={merge}
        />
      )}

      <details className="erase">
        <summary>Erase this wedding&rsquo;s face data</summary>
        <p>
          This deletes every face, every grouping and the key that protects them, on this
          computer, permanently. Your culling and editing decisions are kept. It cannot be
          undone.
        </p>
        <label htmlFor="erase-confirm">Type the project id to confirm</label>
        <input
          id="erase-confirm"
          value={eraseConfirm}
          onChange={(event) => setEraseConfirm(event.target.value)}
        />
        <button
          type="button"
          onClick={erase}
          disabled={busy || eraseConfirm !== projectId}
        >
          Erase face data
        </button>
      </details>
    </section>
  );
}
