import { useCallback, useEffect, useState } from 'react';

import { asIpcError, cleanup as cleanupApi, develop as developApi } from '../../ipc/client';
import type {
  CleanupBlockedDto,
  CleanupProposalDto,
  CleanupStatusDto,
  CropRectDto,
  ManualRemoveDto,
} from '../../ipc/types';
import { BeforeAfter } from './BeforeAfter';
import { ManualRemove } from './ManualRemove';
import { ProposalQueue } from './ProposalQueue';

/**
 * PHASE-24. The container that wires the three cleanup views to the nine cleanup commands.
 *
 * The three views are pure - proposals and callbacks in, nothing fetched - which is what makes
 * them testable without a Tauri window, and which is why none of them was reachable from
 * `main.tsx` until this file existed (`PHASE-01-30-REVIEW.md` section 6.4). Phase 25's
 * `GalleryPanel` established the split and this follows it.
 *
 * **On this build the queue is empty on every photograph, and that is correct.** There is no
 * trained distraction detector, so `detect::candidates` classes everything it finds as
 * `Unclassified`, which cannot be shown to be story-irrelevant, so the safety engine refuses all
 * of it. The panel therefore leads with the status - the refusal histogram and `maskCovered` -
 * rather than with an empty list, because an empty list and a build that cannot look are the
 * same picture and completely different facts. Phase 24's own rule: an absent input is
 * ignorance, not permission.
 *
 * **Everything reloads after a write.** Accepting a proposal applies it, which runs the
 * self-check, which can revert it. A panel that patched its own state would tell a photographer
 * a distraction had gone when the self-check had just put it back.
 */
export type CleanupPanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** The selected photograph, or null. */
  photoId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
};

function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-GEN-9000',
    message: ipc?.message ?? 'The cleanup queue could not be read.',
  };
}

export function CleanupPanel({ projectId, photoId, onError }: CleanupPanelProps): JSX.Element {
  const [status, setStatus] = useState<CleanupStatusDto | null>(null);
  const [proposals, setProposals] = useState<CleanupProposalDto[]>([]);
  const [blocked, setBlocked] = useState<CleanupBlockedDto[]>([]);
  const [previewing, setPreviewing] = useState<string | null>(null);
  const [beforeSrc, setBeforeSrc] = useState<string | null>(null);
  const [region, setRegion] = useState<CropRectDto | null>(null);
  const [busy, setBusy] = useState(false);

  const fail = useCallback(
    (error: unknown) => {
      onError(toBanner(error));
    },
    [onError],
  );

  const reload = useCallback(async () => {
    if (!projectId) {
      setStatus(null);
      setProposals([]);
      setBlocked([]);
      return;
    }
    try {
      setStatus(await cleanupApi.cleanupStatus(projectId));
      if (photoId) {
        const [nextProposals, nextBlocked] = await Promise.all([
          cleanupApi.imageCleanup(photoId),
          cleanupApi.cleanupBlocked(photoId),
        ]);
        setProposals(nextProposals);
        setBlocked(nextBlocked);
      } else {
        setProposals([]);
        setBlocked([]);
      }
    } catch (error) {
      fail(error);
    }
  }, [fail, photoId, projectId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // The selection moved: whatever was being previewed belongs to a different photograph.
  useEffect(() => {
    setPreviewing(null);
    setBeforeSrc(null);
    setRegion(null);
  }, [photoId]);

  const write = useCallback(
    async (action: () => Promise<unknown>) => {
      setBusy(true);
      try {
        await action();
        onError(null);
        await reload();
      } catch (error) {
        fail(error);
      } finally {
        setBusy(false);
      }
    },
    [fail, onError, reload],
  );

  /**
   * The before image, rendered on demand.
   *
   * There is no "render this without the cleanup" argument on the wire and there should not be:
   * what a photographer compares is the delivered frame against the frame as shot, and phase 14's
   * `render_image` produces the second at the same level. The after side is the frame the panel
   * already has once the proposal is applied.
   */
  const preview = useCallback(
    async (proposalId: string) => {
      if (!photoId) {
        return;
      }
      setPreviewing(proposalId);
      try {
        const render = await developApi.renderImage({
          photoId,
          level: 'proxy2048',
          purpose: 'interactive',
        });
        setBeforeSrc(`data:image/png;base64,${render.rgbBase64}`);
      } catch (error) {
        fail(error);
      }
    },
    [fail, photoId],
  );

  const previewed = proposals.find((proposal) => proposal.proposalId === previewing) ?? null;

  if (!projectId) {
    return (
      <section className="cleanup-panel">
        <p className="empty">Open a wedding to see what AURA would tidy.</p>
      </section>
    );
  }

  return (
    <section className="cleanup-panel" aria-label="Cleanup">
      {busy ? <p role="status">Working…</p> : null}

      <ProposalQueue
        status={status}
        proposals={proposals}
        blocked={blocked}
        onDecide={(proposalId, accept) => {
          if (!photoId) {
            return;
          }
          void write(() => cleanupApi.decideCleanup({ photoId, proposalId, accept }));
        }}
        onDisable={(disabled) => {
          if (!photoId) {
            return;
          }
          void write(() => cleanupApi.disableCleanup({ photoId, disabled }));
        }}
        onPreview={(proposalId) => void preview(proposalId)}
      />

      {previewed ? (
        <BeforeAfter
          proposal={previewed}
          beforeSrc={beforeSrc}
          afterSrc={null}
          onDecide={(accept) => {
            if (!photoId) {
              return;
            }
            void write(() =>
              cleanupApi.decideCleanup({
                photoId,
                proposalId: previewed.proposalId,
                accept,
              }),
            );
          }}
        />
      ) : null}

      {photoId ? (
        <ManualRemove
          region={region}
          onClear={() => setRegion(null)}
          onRemove={async (drawn: CropRectDto): Promise<ManualRemoveDto> => {
            const result = await cleanupApi.manualRemove({
              photoId,
              region: drawn,
              confirmed: true,
            });
            await reload();
            return result;
          }}
        />
      ) : (
        <p className="empty">Select a photograph to remove something by hand.</p>
      )}
    </section>
  );
}
