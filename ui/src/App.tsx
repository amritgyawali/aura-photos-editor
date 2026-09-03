import { useCallback, useEffect, useMemo, useState } from 'react';

import { api, asIpcError, inTauri } from './ipc/client';
import { AiKeysPanel } from './components/AiKeysPanel';
import { CacheSettings } from './components/CacheSettings';
import { Filmstrip } from './components/Filmstrip';
import { HardwarePanel } from './components/HardwarePanel';
import { ImportWizard } from './components/ImportWizard';
import { ProblemsPanel } from './components/ProblemsPanel';
import { ProjectSwitcher } from './components/ProjectSwitcher';
import { WorkspaceNav, type WorkspaceId } from './components/WorkspaceNav';
import { AutopilotPanel } from './components/autopilot/AutopilotPanel';
import { CameraMatchPanel } from './components/camera/CameraMatchPanel';
import { CleanupPanel } from './components/cleanup/CleanupPanel';
import { CullView } from './components/cull/CullView';
import { CuratePanel } from './components/curate/CuratePanel';
import { DeliveryPanel } from './components/delivery/DeliveryPanel';
import { DevelopWorkspace } from './components/develop/DevelopWorkspace';
import { ToneReviewQueue } from './components/develop/ToneReviewQueue';
import { Inspector } from './components/explain/Inspector';
import { FilterChips } from './components/explain/FilterChips';
import { GalleryPanel } from './components/gallery/GalleryPanel';
import { MomentStack } from './components/grid/MomentStack';
import { PeoplePanel } from './components/people/PeoplePanel';
import { QcPanel } from './components/qc/QcPanel';
import { StoryPanel } from './components/story/StoryPanel';
import { StylePanel } from './components/style/StylePanel';
import { VirtualGrid } from './components/grid/VirtualGrid';
import { PAGE_SIZE, useStore } from './state/store';
import { useThumbnails } from './stores/thumbnailStore';

export function App(): JSX.Element {
  const projects = useStore((state) => state.projects);
  const activeProjectId = useStore((state) => state.activeProjectId);
  const rows = useStore((state) => state.rows);
  const loadedPages = useStore((state) => state.loadedPages);
  const problems = useStore((state) => state.problems);
  const progress = useStore((state) => state.progress);
  const lastError = useStore((state) => state.lastError);
  const focusedIndex = useStore((state) => state.focusedIndex);
  const focusIndex = useStore((state) => state.focusIndex);
  const selectOnly = useStore((state) => state.selectOnly);

  // Which workspace is open, and which photographs the library is filtered to.
  //
  // The filter is a list of ids rather than a predicate, because the thing doing the filtering
  // is the catalog: `flagged_images`, `flagged_composition` and the review queues all answer
  // with ids, and re-deriving the same answer in the browser would be a second implementation
  // of a judgement the product has already made. `null` is "no filter", never "no matches".
  const [workspace, setWorkspace] = useState<WorkspaceId>('library');
  const [filtered, setFiltered] = useState<string[] | null>(null);

  const setProjects = useStore((state) => state.setProjects);
  const setActiveProject = useStore((state) => state.setActiveProject);
  const appendRows = useStore((state) => state.appendRows);
  const replaceRows = useStore((state) => state.replaceRows);
  const setProblems = useStore((state) => state.setProblems);
  const setProgress = useStore((state) => state.setProgress);
  const setError = useStore((state) => state.setError);

  const clearThumbnails = useThumbnails((state) => state.clear);
  const prefetchThumbnails = useThumbnails((state) => state.requestMany);
  const putThumbnail = useThumbnails((state) => state.put);
  const markThumbnailFailed = useThumbnails((state) => state.markFailed);

  const refreshProjects = useCallback(async () => {
    if (!inTauri()) {
      return;
    }
    try {
      setProjects(await api.listProjects());
    } catch (error) {
      const ipc = asIpcError(error);
      setError({ code: ipc.code, message: ipc.message });
    }
  }, [setError, setProjects]);

  const loadPage = useCallback(
    async (projectId: string, page: number, replace: boolean) => {
      if (!inTauri()) {
        return;
      }
      try {
        const next = await api.listImages({
          projectId,
          offset: page * PAGE_SIZE,
          limit: PAGE_SIZE,
          orderBy: 'timeline',
        });
        if (replace) {
          replaceRows(next);
        } else if (next.length > 0) {
          appendRows(next);
        }
      } catch (error) {
        const ipc = asIpcError(error);
        setError({ code: ipc.code, message: ipc.message });
      }
    },
    [appendRows, replaceRows, setError],
  );

  useEffect(() => {
    void refreshProjects();
  }, [refreshProjects]);

  // A different wedding is a different set of pixels; keeping the old bitmaps
  // would show the previous couple's frames for a few hundred milliseconds.
  useEffect(() => {
    clearThumbnails();
  }, [activeProjectId, clearThumbnails]);

  // Thumbnails for rows that exist but are not on screen yet are queued at
  // batch priority, so scrolling lands on pixels that are already there.
  useEffect(() => {
    if (activeProjectId && rows.length > 0) {
      void prefetchThumbnails(
        activeProjectId,
        rows.map((row) => row.id),
      );
    }
  }, [activeProjectId, prefetchThumbnails, rows]);

  useEffect(() => {
    if (!inTauri()) {
      return;
    }
    let dispose: (() => void) | null = null;
    void api
      .onPreviewEvent((event) => {
        if (event.kind === 'failed') {
          markThumbnailFailed(event.photoId, event.message);
        } else if (event.kind === 'ready' && activeProjectId) {
          void api
            .getPreview({
              projectId: activeProjectId,
              photoId: event.photoId,
              level: event.tier >= 2 ? 'proxy' : 'thumb',
              priority: 'background',
            })
            .then((payload) => putThumbnail(event.photoId, payload))
            .catch(() => undefined);
        }
      })
      .then((unlisten) => {
        dispose = unlisten;
      });
    return () => {
      dispose?.();
    };
  }, [activeProjectId, markThumbnailFailed, putThumbnail]);

  useEffect(() => {
    if (activeProjectId) {
      void loadPage(activeProjectId, 0, true);
      if (inTauri()) {
        void api
          .listProblems(activeProjectId)
          .then(setProblems)
          .catch((error: unknown) => {
            const ipc = asIpcError(error);
            setError({ code: ipc.code, message: ipc.message });
          });
      }
    }
  }, [activeProjectId, loadPage, setError, setProblems]);

  useEffect(() => {
    if (!inTauri()) {
      return;
    }
    let dispose: (() => void) | null = null;
    void api
      .onIngestEvent((event) => {
        if (event.kind === 'progress') {
          setProgress({ done: event.done, total: event.total, running: true });
        } else if (event.kind === 'finished') {
          setProgress({ running: false, jobId: null });
          if (activeProjectId) {
            void loadPage(activeProjectId, 0, true);
            void api.listProblems(activeProjectId).then(setProblems).catch(() => undefined);
          }
          void refreshProjects();
        } else if (event.kind === 'warning') {
          setError({ code: event.code, message: event.message });
        }
      })
      .then((unlisten) => {
        dispose = unlisten;
      });
    return () => {
      dispose?.();
    };
  }, [activeProjectId, loadPage, refreshProjects, setError, setProblems, setProgress]);

  const startImport = useCallback(
    async (roots: string[]) => {
      if (!activeProjectId || !inTauri()) {
        return;
      }
      try {
        const handle = await api.startIngest({ projectId: activeProjectId, roots });
        setProgress({ running: true, jobId: handle.jobId, done: 0, total: 0 });
      } catch (error) {
        const ipc = asIpcError(error);
        setError({ code: ipc.code, message: ipc.message });
      }
    },
    [activeProjectId, setError, setProgress],
  );

  const cancelImport = useCallback(async () => {
    if (progress.jobId && inTauri()) {
      await api.cancelJob(progress.jobId);
      setProgress({ running: false, jobId: null });
    }
  }, [progress.jobId, setProgress]);

  const createProject = useCallback(
    async (name: string) => {
      if (!inTauri()) {
        return;
      }
      try {
        const handle = await api.createProject({ name, coupleNames: null, eventDate: null });
        await refreshProjects();
        setActiveProject(handle.id);
      } catch (error) {
        const ipc = asIpcError(error);
        setError({ code: ipc.code, message: ipc.message });
      }
    },
    [refreshProjects, setActiveProject, setError],
  );

  const focusedPhoto = rows[focusedIndex] ?? null;
  const focusedPhotoId = focusedPhoto?.id ?? null;

  /**
   * Jump the grid to one photograph, from wherever it was named.
   *
   * Six panels can name a frame - the similar list, the moment browser, the cull view's
   * rejections, the outlier list, the QC queue and the review queue - and all six mean the same
   * thing by it: show me that one. It scrolls the library rather than opening a modal, because a
   * photograph in this product is only ever understood beside the frames around it.
   */
  const openPhoto = useCallback(
    (photoId: string) => {
      const index = rows.findIndex((row) => row.id === photoId);
      if (index >= 0) {
        focusIndex(index);
        selectOnly(photoId);
      }
    },
    [focusIndex, rows, selectOnly],
  );

  // A `Set` rather than the list itself: `Array.includes` inside a `filter` is quadratic, and a
  // filter chip on a four-thousand-frame wedding is exactly the case that matters.
  const filteredSet = useMemo(() => (filtered ? new Set(filtered) : null), [filtered]);
  const visibleRows = useMemo(
    () => (filteredSet ? rows.filter((row) => filteredSet.has(row.id)) : rows),
    [filteredSet, rows],
  );

  return (
    <div className="app">
      <aside className="sidebar">
        <ProjectSwitcher
          projects={projects}
          activeProjectId={activeProjectId}
          onSelect={setActiveProject}
          onCreate={(name) => void createProject(name)}
        />
        <ImportWizard
          disabled={activeProjectId === null}
          running={progress.running}
          done={progress.done}
          total={progress.total}
          onStart={(roots) => void startImport(roots)}
          onCancel={() => void cancelImport()}
        />
        <ProblemsPanel problems={problems} />
        <CacheSettings projectId={activeProjectId} onError={setError} />
        <HardwarePanel onError={setError} />
        <AiKeysPanel projectId={activeProjectId} onError={setError} />
        {/* PHASE-25. The one panel in the sidebar whose subject is the whole wedding rather
            than the selected photograph, which is why it renders nothing until a project is
            open rather than showing an empty frame. */}
        <GalleryPanel
          projectId={activeProjectId}
          selectedPhotoId={focusedPhotoId}
          onSelectPhoto={openPhoto}
          onError={setError}
        />
        {/* PHASE-27. The second whole-wedding panel, and it sits under the first deliberately:
            phase 25 makes a gallery coherent and this checks whether it is. It is the last thing
            a photographer looks at before they deliver, so it is the last thing in the sidebar. */}
        <AutopilotPanel projectId={activeProjectId} onError={setError} />
        <QcPanel projectId={activeProjectId} onError={setError} />
        <CuratePanel projectId={activeProjectId} onError={setError} />
        {/* PHASE-30. The last panel, and the first whose button writes files. `profileId` is null
            until a photographer has trained a style profile, which is what makes the learning
            review show its buckets and no comparison - the ordinary state of the feature. */}
        <DeliveryPanel projectId={activeProjectId} profileId={null} onError={setError} />
      </aside>

      <main className="main">
        <WorkspaceNav
          active={workspace}
          onSelect={setWorkspace}
          disabled={activeProjectId === null}
        />

        {lastError && (
          <div className="banner" role="alert">
            <strong>{lastError.code}</strong> {lastError.message}
            <button type="button" onClick={() => setError(null)}>
              Dismiss
            </button>
          </div>
        )}

        {activeProjectId === null ? (
          <p className="empty">Create a wedding, then point AURA at your cards.</p>
        ) : (
          <>
            {workspace === 'library' ? (
              <div className="workspace">
                <div className="photo-browser">
                  {/* PHASE-09 and PHASE-11. The chips are the catalog's own answers: every one of
                      them is a query rather than a verdict, and a chip that found nothing and a
                      chip nobody could evaluate are drawn differently. */}
                  <FilterChips
                    projectId={activeProjectId}
                    onSelect={(photoIds) =>
                      setFiltered(photoIds.length === 0 ? null : photoIds)
                    }
                  />
                  {filtered ? (
                    <p className="filter-note">
                      Showing {visibleRows.length} of {rows.length} photographs.{' '}
                      <button type="button" onClick={() => setFiltered(null)}>
                        Show everything
                      </button>
                    </p>
                  ) : null}
                  <VirtualGrid
                    rows={visibleRows}
                    onNeedMore={() => void loadPage(activeProjectId, loadedPages, false)}
                  />
                  <Filmstrip rows={visibleRows} />
                </div>
                <Inspector
                  projectId={activeProjectId}
                  photoId={focusedPhotoId}
                  onSelect={openPhoto}
                  onError={setError}
                />
              </div>
            ) : null}

            {workspace === 'people' ? (
              <PeoplePanel projectId={activeProjectId} onError={setError} />
            ) : null}

            {workspace === 'story' ? (
              <StoryPanel projectId={activeProjectId} onError={setError} />
            ) : null}

            {workspace === 'moments' ? <MomentStack projectId={activeProjectId} /> : null}

            {workspace === 'cull' ? (
              <CullView projectId={activeProjectId} onOpenImage={openPhoto} />
            ) : null}

            {workspace === 'develop' ? (
              <div className="workspace">
                <DevelopWorkspace
                  projectId={activeProjectId}
                  photoId={focusedPhotoId}
                  onError={setError}
                />
                <ToneReviewQueue
                  projectId={activeProjectId}
                  onOpen={openPhoto}
                  onError={setError}
                />
              </div>
            ) : null}

            {workspace === 'cleanup' ? (
              <CleanupPanel
                projectId={activeProjectId}
                photoId={focusedPhotoId}
                onError={setError}
              />
            ) : null}

            {workspace === 'style' ? (
              <StylePanel projectId={activeProjectId} onError={setError} />
            ) : null}

            {workspace === 'camera' ? (
              <CameraMatchPanel projectId={activeProjectId} onError={setError} />
            ) : null}
          </>
        )}
      </main>
    </div>
  );
}
