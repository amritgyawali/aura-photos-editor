import { useCallback, useEffect } from 'react';

import { api, asIpcError, inTauri } from './ipc/client';
import { AiKeysPanel } from './components/AiKeysPanel';
import { CacheSettings } from './components/CacheSettings';
import { Filmstrip } from './components/Filmstrip';
import { HardwarePanel } from './components/HardwarePanel';
import { ImportWizard } from './components/ImportWizard';
import { ProblemsPanel } from './components/ProblemsPanel';
import { ProjectSwitcher } from './components/ProjectSwitcher';
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
      </aside>

      <main className="main">
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
            <VirtualGrid
              rows={rows}
              onNeedMore={() => void loadPage(activeProjectId, loadedPages, false)}
            />
            <Filmstrip rows={rows} />
          </>
        )}
      </main>
    </div>
  );
}
