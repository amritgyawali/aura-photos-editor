import { useCallback, useEffect, useState } from 'react';

import { asIpcError, inTauri, delivery as api, learning as learnApi } from '../../ipc/client';
import type {
  ConsentDto,
  DeliveryManifestDto,
  DeliveryStatusDto,
  DiagnosticsDto,
  ExportFileDto,
  ExportJobInput,
  ExportNameDto,
  ExportPresetDto,
  ExportStatusDto,
  LearnBucketDto,
  LearnComparisonDto,
  LearnStatusDto,
  ProviderDto,
  UploadItemDto,
} from '../../ipc/types';
import {
  DeliveryView,
  DiagnosticsView,
  ExportView,
  LearningView,
  ManifestView,
} from './Delivery';

/**
 * PHASE-30. The container that wires the five delivery views to the seventeen delivery commands.
 *
 * The views are pure - rows and callbacks in, nothing fetched - which is what makes them testable
 * without a Tauri window. This is the one piece that talks to the shell, and it exists so
 * `App.tsx` can mount the feature with a project id and nothing else. Phase 25's `GalleryPanel`
 * established the split and every panel since has followed it.
 *
 * ## Why this one does not poll
 *
 * Phase 28's panel polls because an autopilot run reports progress from a crate with no event bus.
 * An export is a single blocking command that returns when it is done, so there is nothing to poll:
 * the button disables, the command runs, and every read re-runs when it returns.
 *
 * That is deliberate rather than a simplification. A progress bar over an export would need the
 * export loop to publish, and a loop that publishes is a loop that can be interrupted between a
 * write and its read-back - which is the one moment in this phase where an interruption produces a
 * file nobody checked.
 *
 * ## Why every read re-runs after a job
 *
 * A finished export changes the status, the file list and the manifest at once, and two of those
 * three are only written at the end. A panel that patched its own state would show a photographer
 * the delivery it *predicted* rather than the one on their disk.
 */
export type DeliveryPanelProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** The style profile the learning review is about, or null. */
  profileId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
};

export function DeliveryPanel({ projectId, profileId, onError }: DeliveryPanelProps) {
  const [status, setStatus] = useState<ExportStatusDto | null>(null);
  const [presets, setPresets] = useState<ExportPresetDto[]>([]);
  const [selected, setSelected] = useState('gallery');
  const [destination, setDestination] = useState('');
  const [verify, setVerify] = useState(true);
  const [names, setNames] = useState<ExportNameDto[] | null>(null);
  const [files, setFiles] = useState<ExportFileDto[]>([]);
  const [manifest, setManifest] = useState<DeliveryManifestDto | null>(null);
  const [running, setRunning] = useState(false);

  const [deliveryStatus, setDeliveryStatus] = useState<DeliveryStatusDto | null>(null);
  const [providers, setProviders] = useState<ProviderDto[]>([]);
  const [items, setItems] = useState<UploadItemDto[]>([]);
  const [backupPath, setBackupPath] = useState('');

  const [learnStatus, setLearnStatus] = useState<LearnStatusDto | null>(null);
  const [buckets, setBuckets] = useState<LearnBucketDto[]>([]);
  const [comparison, setComparison] = useState<LearnComparisonDto | null>(null);
  const [consent, setConsent] = useState<ConsentDto | null>(null);

  const [report, setReport] = useState<DiagnosticsDto | null>(null);

  const fail = useCallback(
    (error: unknown) => {
      const ipc = asIpcError(error);
      onError({ code: ipc.code, message: ipc.message });
    },
    [onError],
  );

  /** Build the job from what the dialog holds. Whole, never field by field. */
  const buildJob = useCallback((): ExportJobInput | null => {
    if (!projectId) {
      return null;
    }
    const preset = presets.find((p) => p.name === selected);
    if (!preset) {
      return null;
    }
    return {
      projectId,
      sets: [
        {
          name: preset.name,
          // The gallery the export is over. The panel that mounts this passes the ids it holds;
          // an empty list is refused by `ExportJob::validate` rather than silently exporting
          // nothing, which is the behaviour a caller wants when it has not loaded yet.
          imageIds: files.map((f) => f.imageId),
          format: preset.format,
          quality: preset.quality,
          colour: preset.colour,
          bitDepth: preset.bitDepth,
          resize: preset.resize,
          sharpen: preset.sharpen,
          naming: preset.naming,
          sidecar: preset.sidecar,
        },
      ],
      destination,
      destinationKind: 'folder',
      copyright: null,
      contact: null,
      creator: null,
      keywords: [],
      stripGps: true,
      stripCameraSerial: true,
      verify,
    };
  }, [projectId, presets, selected, files, destination, verify]);

  const refreshExport = useCallback(async () => {
    if (!projectId) {
      return;
    }
    try {
      const [s, f, m] = await Promise.all([
        api.exportStatus(projectId),
        api.exportFiles(projectId),
        api.exportManifest(projectId),
      ]);
      setStatus(s);
      setFiles(f);
      setManifest(m);
      onError(null);
    } catch (error) {
      fail(error);
    }
  }, [projectId, onError, fail]);

  const refreshDelivery = useCallback(async () => {
    if (!projectId) {
      return;
    }
    try {
      const [s, p] = await Promise.all([
        api.deliveryStatus(projectId),
        api.deliveryProviders(),
      ]);
      setDeliveryStatus(s);
      setProviders(p);
      onError(null);
    } catch (error) {
      fail(error);
    }
  }, [projectId, onError, fail]);

  const refreshLearning = useCallback(async () => {
    try {
      const [s, b] = await Promise.all([learnApi.learnStatus(), learnApi.learnBuckets()]);
      setLearnStatus(s);
      setBuckets(b);
      if (profileId) {
        setComparison(await learnApi.learnCompare(profileId));
      }
      if (projectId) {
        setConsent(await learnApi.learnConsent(projectId));
      }
      onError(null);
    } catch (error) {
      fail(error);
    }
  }, [projectId, profileId, onError, fail]);

  // The two project-independent reads: the preset list and the learning diagnostics.
  //
  // `inTauri` because this is the one effect in the panel with no `projectId` to bail on, so
  // without it the panel calls a command with no window behind it - which `asIpcError` cannot
  // recognise as an IPC failure and reports as `AURA-DB-3006`, a *catalog* error. A banner
  // saying the catalog failed when the truth is that there is no backend is worse than no
  // banner: it sends somebody to the wrong runbook. `HardwarePanel`, `AiKeysPanel` and
  // `CacheSettings` all take the same guard; this panel was the one that did not, and it is
  // what the browser sees against the vite dev server.
  useEffect(() => {
    if (!inTauri()) {
      return;
    }
    void (async () => {
      try {
        setPresets(await api.exportPresets());
        setReport(await learnApi.diagnosticsReport());
      } catch (error) {
        fail(error);
      }
    })();
  }, [fail]);

  useEffect(() => {
    void refreshExport();
    void refreshDelivery();
    void refreshLearning();
  }, [refreshExport, refreshDelivery, refreshLearning]);

  const onPreviewNames = useCallback(() => {
    const job = buildJob();
    if (!job) {
      return;
    }
    void (async () => {
      try {
        setNames(await api.exportPreviewNames(job));
        onError(null);
      } catch (error) {
        fail(error);
      }
    })();
  }, [buildJob, onError, fail]);

  const onRun = useCallback(() => {
    const job = buildJob();
    if (!job) {
      return;
    }
    setRunning(true);
    void (async () => {
      try {
        await api.exportRun(job);
        // Every read re-runs: three of the things a finished export changes are only written at
        // the end, and a panel that patched its own state would show the delivery it predicted.
        await refreshExport();
        await refreshDelivery();
        onError(null);
      } catch (error) {
        fail(error);
      } finally {
        setRunning(false);
      }
    })();
  }, [buildJob, refreshExport, refreshDelivery, onError, fail]);

  const onBackup = useCallback(() => {
    if (!projectId) {
      return;
    }
    void (async () => {
      try {
        await api.deliveryBackup({ projectId, target: backupPath, mapping: [] });
        await refreshDelivery();
        onError(null);
      } catch (error) {
        fail(error);
      }
    })();
  }, [projectId, backupPath, refreshDelivery, onError, fail]);

  const onUpload = useCallback(
    (provider: string) => {
      if (!projectId) {
        return;
      }
      void (async () => {
        try {
          await api.deliveryUpload({
            projectId,
            target: provider,
            mapping: [{ set: selected, remote: selected, publish: false }],
          });
          setItems(await api.deliveryItems(projectId, provider));
          await refreshDelivery();
          onError(null);
        } catch (error) {
          fail(error);
        }
      })();
    },
    [projectId, selected, refreshDelivery, onError, fail],
  );

  const onAdopt = useCallback(() => {
    if (!profileId) {
      return;
    }
    void (async () => {
      try {
        await learnApi.learnAdopt(profileId);
        await refreshLearning();
        onError(null);
      } catch (error) {
        fail(error);
      }
    })();
  }, [profileId, refreshLearning, onError, fail]);

  const onRollBack = useCallback(() => {
    if (!profileId) {
      return;
    }
    void (async () => {
      try {
        await learnApi.learnRollBack(profileId);
        await refreshLearning();
        onError(null);
      } catch (error) {
        fail(error);
      }
    })();
  }, [profileId, refreshLearning, onError, fail]);

  const onConsent = useCallback(
    (next: ConsentDto) => {
      void (async () => {
        try {
          setConsent(await learnApi.learnSetConsent(next));
          onError(null);
        } catch (error) {
          fail(error);
        }
      })();
    },
    [onError, fail],
  );

  return (
    <div className="delivery-panel">
      <ExportView
        status={status}
        presets={presets}
        selected={selected}
        destination={destination}
        verify={verify}
        names={names}
        running={running}
        onSelectPreset={setSelected}
        onDestination={setDestination}
        onVerify={setVerify}
        onPreviewNames={onPreviewNames}
        onRun={onRun}
      />
      <ManifestView manifest={manifest} files={files} />
      <DeliveryView
        status={deliveryStatus}
        providers={providers}
        items={items}
        backupPath={backupPath}
        onBackupPath={setBackupPath}
        onBackup={onBackup}
        onUpload={onUpload}
      />
      <LearningView
        status={learnStatus}
        buckets={buckets}
        comparison={comparison}
        consent={consent}
        onAdopt={onAdopt}
        onRollBack={onRollBack}
        onConsent={onConsent}
      />
      <DiagnosticsView report={report} />
    </div>
  );
}
