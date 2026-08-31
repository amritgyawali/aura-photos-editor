import { useCallback, useEffect, useState } from 'react';

import { asIpcError, camera } from '../../ipc/client';
import type {
  CameraReportDto,
  CameraStatusDto,
  CameraTransformDto,
  MatchedPairDto,
  ShooterBiasDto,
} from '../../ipc/types';
import { CameraMatchView } from './CameraMatchView';

/**
 * PHASE-26. The container that wires `CameraMatchView` to the eleven camera commands.
 *
 * `CameraMatchView` is pure - rows and callbacks in, nothing fetched - which is what makes it
 * testable without a Tauri window. This is the one piece that talks to the shell, and it exists so
 * `App.tsx` can mount the feature with a project id and nothing else. Phase 25's `GalleryPanel`
 * established the split and this follows it.
 *
 * **The matched pairs are fetched per camera, not per project.** A wedding with three cameras and a
 * long ceremony produces hundreds of verified pairs, and loading all of them to draw a header would
 * pull the whole evidence set over the wire to render a summary. They are fetched when a camera row
 * is expanded and dropped when it collapses.
 *
 * **Everything reloads after a write.** Choosing a reference re-solves every other body against it,
 * which changes every transform, every report and the project header - so the four reads are re-run
 * rather than patched locally. A panel that patched its own state would drift from the catalog the
 * first time a re-solve did something it did not predict, and the whole point of this phase is that
 * a photographer can trust what the panel says about the evidence.
 */
export type CameraMatchPanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
};

/** How many matched pairs the viewer asks for. A page, not a wedding. */
const PAIR_PAGE = 40;

/** The app banner's shape, from whatever the wire raised. */
function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? "AURA-ML-5130",
    message: ipc?.message ?? "Camera matching could not be read.",
  };
}

export function CameraMatchPanel({
  projectId,
  onError,
}: CameraMatchPanelProps) {
  const [status, setStatus] = useState<CameraStatusDto | null>(null);
  const [reports, setReports] = useState<CameraReportDto[]>([]);
  const [transforms, setTransforms] = useState<CameraTransformDto[]>([]);
  const [shooterBias, setShooterBias] = useState<ShooterBiasDto[]>([]);
  const [pairs, setPairs] = useState<MatchedPairDto[]>([]);
  const [expandedCameraId, setExpandedCameraId] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const reload = useCallback(async () => {
    if (!projectId) {
      setStatus(null);
      setReports([]);
      setTransforms([]);
      setShooterBias([]);
      return;
    }
    try {
      const [nextStatus, nextReports, nextTransforms, nextBias] =
        await Promise.all([
          camera.cameraStatus(projectId),
          camera.cameraReports(projectId),
          camera.cameraTransforms(projectId),
          camera.cameraShooterBias(projectId),
        ]);
      setStatus(nextStatus);
      setReports(nextReports);
      setTransforms(nextTransforms);
      setShooterBias(nextBias);
      onError(null);
    } catch (error) {
      onError(toBanner(error));
    }
  }, [projectId, onError]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // Collapsing a row drops its pairs rather than keeping them: the evidence set for one camera can
  // be a hundred and sixty rows, and three expanded cameras is a page nobody scrolls.
  const handleExpand = useCallback(
    async (key: string | null) => {
      setExpandedCameraId(key);
      if (!key || !projectId) {
        setPairs([]);
        return;
      }
      const cameraId = key.split("/")[0] ?? "";
      try {
        setPairs(await camera.cameraPairs(projectId, cameraId, PAIR_PAGE));
        onError(null);
      } catch (error) {
        setPairs([]);
        onError(toBanner(error));
      }
    },
    [projectId, onError],
  );

  const handleRunPass = useCallback(async () => {
    if (!projectId || running) {
      return;
    }
    setRunning(true);
    try {
      await camera.cameraPass({ projectId });
      await reload();
      // A re-solve re-forms every pair, so whatever the viewer was showing describes a split that
      // no longer exists. Collapsing is the honest response.
      setExpandedCameraId(null);
      setPairs([]);
    } catch (error) {
      onError(toBanner(error));
    } finally {
      setRunning(false);
    }
  }, [projectId, running, reload, onError]);

  const handleSetReference = useCallback(
    async (cameraId: string) => {
      if (!projectId) {
        return;
      }
      try {
        await camera.setCameraReference({ projectId, cameraId });
        await reload();
      } catch (error) {
        onError(toBanner(error));
      }
    },
    [projectId, reload, onError],
  );

  const handleToggleEnabled = useCallback(
    async (cameraId: string, disabled: boolean) => {
      if (!projectId) {
        return;
      }
      try {
        await camera.disableCamera({ projectId, cameraId, disabled });
        await reload();
      } catch (error) {
        onError(toBanner(error));
      }
    },
    [projectId, reload, onError],
  );

  return (
    <CameraMatchView
      status={status}
      reports={reports}
      transforms={transforms}
      shooterBias={shooterBias}
      pairs={pairs}
      expandedCameraId={expandedCameraId}
      running={running}
      onExpand={(key) => void handleExpand(key)}
      onRunPass={() => void handleRunPass()}
      onSetReference={(cameraId) => void handleSetReference(cameraId)}
      onToggleEnabled={(cameraId, wasEnabled) =>
        void handleToggleEnabled(cameraId, wasEnabled)
      }
    />
  );
}
