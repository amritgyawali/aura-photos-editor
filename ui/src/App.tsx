import { useCallback, useEffect, useRef, useState } from 'react';

import { api, asIpcError, inTauri, pickPhotos } from './ipc/client';
import { InstagramStyle } from './components/look/InstagramStyle';
import { readReferenceSelection, saveReferenceSelection, type ReferenceSelection } from './components/look/referenceStyle';
import { AiKeysPanel } from './components/AiKeysPanel';
import { CacheSettings } from './components/CacheSettings';
import { Filmstrip } from './components/Filmstrip';
import { HardwarePanel } from './components/HardwarePanel';
import { ImportWizard } from './components/ImportWizard';
import { ProblemsPanel } from './components/ProblemsPanel';
import { ProjectSwitcher } from './components/ProjectSwitcher';
import { AutopilotPanel } from './components/autopilot/AutopilotPanel';
import { MatchLookPanel } from './components/look/MatchLookPanel';
import { CuratePanel } from './components/curate/CuratePanel';
import { DeliveryPanel } from './components/delivery/DeliveryPanel';
import { PhotoStudio } from './components/develop/PhotoStudio';
import { GalleryPanel } from './components/gallery/GalleryPanel';
import { QcPanel } from './components/qc/QcPanel';
import { VirtualGrid } from './components/grid/VirtualGrid';
import { PAGE_SIZE, useStore } from './state/store';
import { useThumbnails } from './stores/thumbnailStore';

export function App(): JSX.Element {
  const [workspace, setWorkspace] = useState('library');
  const [reference, setReference] = useState<ReferenceSelection | null>(readReferenceSelection);
  const [analysingReference, setAnalysingReference] = useState(false);
  const [choosingPhotos, setChoosingPhotos] = useState(false);
  const [queuedImport, setQueuedImport] = useState<{ projectId: string; roots: string[] } | null>(null);
  const [ingestReady, setIngestReady] = useState<string | null>(null);
  const changeReference = useCallback((next: ReferenceSelection | null) => { setReference(next); saveReferenceSelection(next); }, []);
  const [editing, setEditing] = useState(false);
  const [matching, setMatching] = useState(false);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [revision, setRevision] = useState(0);
  const [automaticRequest, setAutomaticRequest] = useState(0);
  const automaticSequence = useRef(0);
  const automaticConsumed = useCallback(() => setAutomaticRequest(0), []);
  const editAfterImport = useRef(false);
  const importFinished = useRef(false);
  const editBusyChanged = useCallback((busy: boolean) => {
    setEditing(busy);
    if (!busy) setRevision(value => value + 1);
  }, []);
  const projects = useStore((state) => state.projects);
  const activeProjectId = useStore((state) => state.activeProjectId);
  const rows = useStore((state) => state.rows);
  const loadedPages = useStore((state) => state.loadedPages);
  const problems = useStore((state) => state.problems);
  const progress = useStore((state) => state.progress);
  const lastError = useStore((state) => state.lastError);
  const focusedIndex = useStore((state) => state.focusedIndex);

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
    let disposed = false;
    void api
      .onIngestEvent((event) => {
        if (disposed) return;
        if (event.kind === 'progress') {
          setProgress({ done: event.done, total: event.total, running: true });
        } else if (event.kind === 'finished') {
          importFinished.current = true;
          setProgress({ running: false, jobId: null });
          if (editAfterImport.current && event.inserted > 0) {
            editAfterImport.current = false;
            setWorkspace('edit');
            setAutomaticRequest(++automaticSequence.current);
          }
          editAfterImport.current = false;
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
        if (disposed) { unlisten(); return; }
        dispose = unlisten;
        setIngestReady(activeProjectId);
      }).catch(cause => { if (!disposed) { setQueuedImport(null); const error = asIpcError(cause); setError({ code: error.code, message: error.message }); } });
    return () => {
      disposed = true;
      dispose?.();
    };
  }, [activeProjectId, loadPage, refreshProjects, setError, setProblems, setProgress]);

  const startImport = useCallback(
    async (roots: string[]) => {
      if (!activeProjectId || !inTauri()) {
        return;
      }
      try {
        editAfterImport.current = true;
        importFinished.current = false;
        setProgress({ running: true, jobId: null, done: 0, total: 0 });
        const handle = await api.startIngest({ projectId: activeProjectId, roots });
        if (!importFinished.current) {
          setProgress({ running: true, jobId: handle.jobId, done: 0, total: 0 });
          if (!editAfterImport.current) await api.cancelJob(handle.jobId);
        }
      } catch (error) {
        editAfterImport.current = false;
        setProgress({ running: false, jobId: null });
        const ipc = asIpcError(error);
        setError({ code: ipc.code, message: ipc.message });
      }
    },
    [activeProjectId, setError, setProgress],
  );

  const cancelImport = useCallback(async () => {
    editAfterImport.current = false;
    if (progress.jobId && inTauri()) {
      try { await api.cancelJob(progress.jobId); }
      catch (error) { const ipc = asIpcError(error); setError({ code: ipc.code, message: ipc.message }); }
    }
  }, [progress.jobId, setError]);

  useEffect(() => {
    if (queuedImport && queuedImport.projectId === activeProjectId && ingestReady === activeProjectId) {
      const roots = queuedImport.roots;
      setQueuedImport(null);
      void startImport(roots);
    }
  }, [activeProjectId, ingestReady, queuedImport, startImport]);

  const createProject = useCallback(
    async (name: string) => {
      if (!inTauri()) {
        return;
      }
      try {
        const handle = await api.createProject({ name, coupleNames: null, eventDate: null });
        await refreshProjects();
        setActiveProject(handle.id);
        return handle.id;
      } catch (error) {
        const ipc = asIpcError(error);
        setError({ code: ipc.code, message: ipc.message });
      }
    },
    [refreshProjects, setActiveProject, setError],
  );

  const focusedPhoto = rows[focusedIndex] ?? null;

  const chooseAndImport = async () => {
    if (choosingPhotos) return;
    setChoosingPhotos(true);
    try {
      const roots = await pickPhotos();
      if (!roots.length) return;
      const target = activeProjectId ?? await createProject('My photo collection');
      if (target) { setWorkspace('library'); setQueuedImport({ projectId: target, roots }); }
    } catch (cause) { const error = asIpcError(cause); setError({ code: error.code, message: error.message }); }
    finally { setChoosingPhotos(false); }
  };

  const locked = editing || matching || saving || exporting || progress.running || analysingReference || choosingPhotos || queuedImport !== null;
  const tabs = [
    ['library', 'Photos', 'Browse your collection'],
    ['edit', 'Auto edit', 'One click, start to finish'],
    ['look', 'Instagram style', 'Your reference, your photos'],
    ['export', 'Export', 'Ready to share'],
    ['advanced', 'Advanced', 'Quality, curation & settings'],
  ];

  return (
    <div className="app aura-studio">
      <aside className="sidebar studio-sidebar">
        <div className="studio-brand"><span className="brand-mark" aria-hidden="true">a</span><div><strong>AURA</strong><span>PHOTO STUDIO</span></div></div>
        <nav className="studio-nav" aria-label="Workspace">
          {tabs.map(([id, title, description]) => <button key={id} type="button" aria-current={workspace === id ? 'page' : undefined}
            disabled={locked && workspace !== id} onClick={() => setWorkspace(id ?? 'library')}><strong>{title}</strong><span>{description}</span></button>)}
        </nav>
        <fieldset className="project-picker" disabled={locked}>
          <ProjectSwitcher projects={projects} activeProjectId={activeProjectId} onSelect={setActiveProject} onCreate={name => void createProject(name)} />
        </fieldset>
        <p className="sidebar-note">A little direction.<br />A look that feels like you.</p>
      </aside>
      <main className="main studio-main">
        <header className="studio-topbar">
          <span>{projects.find(project => project.id === activeProjectId)?.name ?? 'Your creative workspace'}</span>
          <span>{activeProjectId ? `${projects.find(project => project.id === activeProjectId)?.photoCount ?? rows.length} photos` : 'Welcome to AURA'}</span>
        </header>
        {lastError && <div className="banner" role="alert"><span>{lastError.message}</span><button type="button" onClick={() => setError(null)}>Dismiss</button></div>}
        <div className="studio-content">
          <div hidden={workspace !== 'library' && workspace !== 'look'}>
            <InstagramStyle selection={reference} disabled={locked} onChange={changeReference} onBusyChange={setAnalysingReference}
              onAddPhotos={() => void chooseAndImport()} onApply={activeProjectId && rows.length ? () => { setWorkspace('edit'); setAutomaticRequest(++automaticSequence.current); } : undefined} />
          </div>
          {!activeProjectId ? <section className="studio-welcome">
            <span className="eyebrow">LESS EDITING. MORE CREATING.</span>
            <h1>Or start with<br /><em>your own photos.</em></h1>
            <p>Portraits, travel, family, or everyday moments. Start a collection, choose your photos, and let AURA balance the light. Review the result, add your touch, and export.</p>
            <button className="is-primary" type="button" disabled={!inTauri() || locked} onClick={() => void chooseAndImport()}>Choose photos to auto edit</button>
            {!inTauri() && <p className="studio-footnote">Open the AURA desktop app to import and edit your photos.</p>}
            <div className="welcome-contact-sheet" aria-hidden="true"><div /><div /><div /><div /><div /><div /></div>
            <div className="welcome-features"><div><strong>One-click editing</strong><span>Light, color and a consistent finish.</span></div><div><strong>Reference matching</strong><span>Learn a look from saved photos.</span></div><div><strong>Always your original</strong><span>Saved edits with undo and reset.</span></div></div>
          </section> : <>
            {workspace === 'library' && <>
              <header className="workspace-heading"><div><span className="eyebrow">YOUR COLLECTION</span><h1>Start with a great photo.</h1></div><button className="is-primary" type="button" disabled={locked || rows.length === 0} onClick={() => setWorkspace('edit')}>Go to auto edit →</button></header>
              <ImportWizard disabled={locked} running={progress.running} done={progress.done} total={progress.total} onStart={roots => void startImport(roots)} onCancel={() => void cancelImport()} />
              {rows.length > 0 ? <div className="studio-library"><VirtualGrid rows={rows} onNeedMore={() => void loadPage(activeProjectId, loadedPages, false)} /></div> : <div className="studio-empty"><strong>Your next great edit starts here.</strong><p>Choose a folder above. Your photos will appear here.</p></div>}
            </>}
            <div hidden={workspace !== 'edit'}>
              <div className="workspace-heading"><div><span className="eyebrow">LIGHT. COLOR. FINISH.</span><h1>A good starting point, in one click.</h1></div><button type="button" disabled={locked} onClick={() => setWorkspace('look')}>Match a reference look →</button></div>
              <AutopilotPanel key={activeProjectId} projectId={activeProjectId} onError={setError} onBusyChange={editBusyChanged}
                automaticRequest={automaticRequest} onAutomaticConsumed={automaticConsumed} onRender={() => setWorkspace('export')} reference={reference} />
              {workspace === 'edit' && focusedPhoto && <>
                <fieldset className="filmstrip-lock" disabled={locked}><Filmstrip rows={rows} /></fieldset>
                <PhotoStudio key={focusedPhoto.id} projectId={activeProjectId} photoId={focusedPhoto.id} disabled={editing} revision={revision} onBusyChange={setSaving} />
                <button type="button" disabled={locked} onClick={() => void loadPage(activeProjectId, loadedPages, false)}>Load more photos</button>
              </>}
            </div>
            {workspace === 'look' && <details className="advanced-tools"><summary>Advanced lighting-bucket look profiles</summary><MatchLookPanel key={activeProjectId} projectId={activeProjectId} onError={setError} onBusyChange={setMatching} /></details>}
            {workspace === 'export' && <><header className="workspace-heading"><div><span className="eyebrow">THE FINISHING TOUCH</span><h1>Render your final output.</h1></div></header><DeliveryPanel key={activeProjectId} projectId={activeProjectId} profileId={null} onError={setError} onBusyChange={setExporting} /></>}
            {workspace === 'advanced' && <><header className="workspace-heading"><div><span className="eyebrow">MORE CONTROL</span><h1>The details make the difference.</h1></div></header>
              <details className="advanced-tools"><summary>Quality review</summary><QcPanel projectId={activeProjectId} onError={setError} /></details>
              <details className="advanced-tools"><summary>Gallery consistency</summary><GalleryPanel projectId={activeProjectId} onError={setError} /></details>
              <details className="advanced-tools"><summary>Albums & curation</summary><CuratePanel projectId={activeProjectId} onError={setError} /></details>
              <details className="advanced-tools"><summary>AI provider</summary><AiKeysPanel projectId={activeProjectId} onError={setError} /></details>
              <details className="advanced-tools"><summary>Performance & storage</summary><HardwarePanel onError={setError} /><CacheSettings projectId={activeProjectId} onError={setError} /></details>
            </>}
            {problems.length > 0 && <details className="advanced-tools"><summary>Problems to review ({problems.length})</summary><ProblemsPanel problems={problems} /></details>}
          </>}
        </div>
      </main>
    </div>
  );
}
