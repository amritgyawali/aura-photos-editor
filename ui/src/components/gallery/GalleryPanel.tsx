import { useCallback, useEffect, useState } from 'react';

import { asIpcError, gallery } from '../../ipc/client';
import type {
  GalleryDeltaDto,
  GalleryOutlierDto,
  GalleryStatusDto,
  SceneNodeDto,
} from '../../ipc/types';
import { ConsistencyView } from './ConsistencyView';

/**
 * PHASE-25. The container that wires `ConsistencyView` to the nine gallery commands.
 *
 * The four components in this directory are pure - rows and callbacks in, nothing fetched - which
 * is what makes them testable without a Tauri window. This is the one piece that talks to the
 * shell, and it exists so `App.tsx` can mount the feature with a project id and nothing else.
 *
 * **The node strip is fetched per node, not per project.** A wedding has forty nodes and four
 * thousand frames; loading every delta to draw a header would pull the whole gallery over the wire
 * to render a summary. ADR-0052 section 2.
 *
 * **Everything reloads after a write.** Pinning an anchor re-solves that node, which changes its
 * target, its deltas and possibly the project's outlier list - so the three reads that could have
 * moved are re-run rather than patched locally. A panel that patched its own state would drift from
 * the catalog the first time a re-solve did something the panel did not predict, and the whole
 * point of this phase is that a photographer can trust what the panel says.
 */
export type GalleryPanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** Which photograph is selected in the grid, if any. */
  selectedPhotoId?: string | null;
  /** Open a photograph. */
  onSelectPhoto?: (photoId: string) => void;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
};

/** How many outliers the queue asks for. A page, not a wedding. */
const OUTLIER_PAGE = 50;

/** The app banner's shape, from whatever the wire raised. */
function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return { code: ipc.code, message: ipc.message };
}

export function GalleryPanel({
  projectId,
  selectedPhotoId,
  onSelectPhoto,
  onError,
}: GalleryPanelProps): JSX.Element | null {
  const [status, setStatus] = useState<GalleryStatusDto | null>(null);
  const [nodes, setNodes] = useState<SceneNodeDto[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [deltas, setDeltas] = useState<GalleryDeltaDto[]>([]);
  const [outliers, setOutliers] = useState<GalleryOutlierDto[]>([]);
  const [busy, setBusy] = useState(false);

  const loadProject = useCallback(async () => {
    if (projectId === null) {
      setStatus(null);
      setNodes([]);
      setOutliers([]);
      setSelectedNodeId(null);
      setDeltas([]);
      return;
    }
    try {
      const [nextStatus, nextNodes, nextOutliers] = await Promise.all([
        gallery.galleryStatus(projectId),
        gallery.galleryNodes(projectId),
        gallery.galleryOutliers(projectId, OUTLIER_PAGE),
      ]);
      setStatus(nextStatus);
      setNodes(nextNodes);
      setOutliers(nextOutliers);
      // Keep the open node when it survived the re-pass; otherwise fall back on the first, because
      // a panel that silently showed nothing after a re-solve reads as a failure.
      setSelectedNodeId((current) => {
        if (current && nextNodes.some((node) => node.nodeId === current)) {
          return current;
        }
        return nextNodes[0]?.nodeId ?? null;
      });
    } catch (error) {
      onError(toBanner(error));
    }
  }, [projectId, onError]);

  const loadStrip = useCallback(
    async (nodeId: string | null) => {
      if (nodeId === null) {
        setDeltas([]);
        return;
      }
      try {
        setDeltas(await gallery.galleryNodeStrip(nodeId));
      } catch (error) {
        onError(toBanner(error));
      }
    },
    [onError],
  );

  useEffect(() => {
    void loadProject();
  }, [loadProject]);

  useEffect(() => {
    void loadStrip(selectedNodeId);
  }, [loadStrip, selectedNodeId]);

  const runPass = useCallback(async () => {
    if (projectId === null) {
      return;
    }
    setBusy(true);
    onError(null);
    try {
      await gallery.galleryPass({ projectId });
      await loadProject();
    } catch (error) {
      onError(toBanner(error));
    } finally {
      setBusy(false);
    }
  }, [projectId, loadProject, onError]);

  const pin = useCallback(
    async (photoId: string, pinned: boolean) => {
      if (selectedNodeId === null) {
        return;
      }
      setBusy(true);
      try {
        await gallery.pinGalleryAnchor({ nodeId: selectedNodeId, photoId, pinned });
        // A pin re-solves the node, so its target and every delta in it may have moved. Re-read
        // rather than patch.
        await loadProject();
        await loadStrip(selectedNodeId);
      } catch (error) {
        onError(toBanner(error));
      } finally {
        setBusy(false);
      }
    },
    [selectedNodeId, loadProject, loadStrip, onError],
  );

  if (projectId === null) {
    return null;
  }

  return (
    <ConsistencyView
      status={status}
      nodes={nodes}
      selectedNodeId={selectedNodeId}
      deltas={deltas}
      outliers={outliers}
      selectedPhotoId={selectedPhotoId ?? null}
      onSelectNode={setSelectedNodeId}
      onSelectPhoto={onSelectPhoto}
      onRunPass={() => void runPass()}
      onPin={(photoId, pinned) => void pin(photoId, pinned)}
      busy={busy}
    />
  );
}
