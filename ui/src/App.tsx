import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { api, asIpcError, inTauri, automaticStart, oneClickCancel } from './ipc/client';
import { automaticBusy, useAutomatic } from './state/automaticStore';
import { AutomaticProgress } from './components/workflow/AutomaticProgress';
import { PhotoAnalysis } from './components/PhotoAnalysis';
import { nav } from './audit/log';
import { AiKeysPanel } from './components/AiKeysPanel';
import { AiSetup } from './components/AiSetup';
import { CacheSettings } from './components/CacheSettings';
import { Filmstrip } from './components/Filmstrip';
import { HardwarePanel } from './components/HardwarePanel';
import { ImportWizard } from './components/ImportWizard';
import { LogPanel } from './components/LogPanel';
import { OnboardingOverlay } from './components/OnboardingOverlay';
import { ProblemsPanel } from './components/ProblemsPanel';
import { ProjectSwitcher } from './components/ProjectSwitcher';
import { ThemeToggle } from './components/ThemeToggle';
import { WelcomeScreen } from './components/WelcomeScreen';
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
import { WorkflowGuide } from './components/workflow/WorkflowGuide';
import { OneClickRunner } from './components/workflow/OneClickRunner';
import { STEPS_BY_ID, type Step, type StepId } from './components/workflow/steps';
import { DEFAULT_TOOL, STAGE_TOOLS, SYSTEM_TOOLS, TOOLS, type ToolId } from './components/workflow/stages';
import { PAGE_SIZE, useStore } from './state/store';
import { useThumbnails } from './stores/thumbnailStore';

export function App(): JSX.Element {
  const busy = useAutomatic(automaticBusy);
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

  // Where the middle of the application is: the step, and the tool open inside it.
  //
  // Two pieces of state rather than one because they answer different questions.
  // The step is the phase of the work - what the wedding needs next, and which tools
  // exist at all. The tool is where the photographer is *within* that phase. Switching
  // steps resets to the step's default tool, which is what makes "specific tools come
  // in each step" a property of the shell rather than of discipline: a cull-stage tab
  // list simply does not contain Delivery, so there is nothing to misclick.
  const [step, setStep] = useState<StepId>('import');
  const [tool, setTool] = useState<ToolId>(DEFAULT_TOOL['import']);

  // Which photographs the library is filtered to.
  //
  // The filter is a list of ids rather than a predicate, because the thing doing the filtering
  // is the catalog: `flagged_images`, `flagged_composition` and the review queues all answer
  // with ids, and re-deriving the same answer in the browser would be a second implementation
  // of a judgement the product has already made. `null` is "no filter", never "no matches".
  const [filtered, setFiltered] = useState<string[] | null>(null);

  // Tool changes go to the log as *movements*, not presses: the click listener
  // already recorded the button, and what a support case needs is the sequence of
  // places the photographer was in. One effect rather than twenty call-site edits.
  const previousTool = useRef<ToolId | null>(null);
  useEffect(() => {
    if (previousTool.current !== null) {
      nav(previousTool.current, tool);
    }
    previousTool.current = tool;
  }, [tool]);

  // Whether the AI question has been answered, and whether the screen that asks it is open.
  //
  // `null` is "we have not looked yet", which is a third state rather than a missing boolean:
  // rendering the setup screen while the answer is still in flight would flash it in front of
  // every photographer who answered it months ago. The screen appears only once the catalog has
  // said the question is outstanding.
  const [aiAnswered, setAiAnswered] = useState<boolean | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);

  const [tourOpen, setTourOpen] = useState(false);
  const [guideRefresh, setGuideRefresh] = useState(0);

  /**
   * Enter a step: its tools become the tab list, and its default tool opens.
   *
   * The old bar scrolled a sidebar panel and flashed it. There is no sidebar to
   * scroll any more - a step *is* the view now - so the jump is two setStates and
   * a refresh of the step evidence, which a run may have just changed underneath.
   */
  const goToStep = useCallback((next: Step) => {
    setStep(next.id);
    setTool(next.defaultTool);
    setGuideRefresh((current) => current + 1);
  }, []);

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

  // The tour, once, when there is nothing to look at instead. A photographer who
  // has seen it never sees it again; one with a wedding already open does not
  // need it - the application is the better teacher than a modal over real work.
  useEffect(() => {
    // The tour remains available on request; file selection is never blocked by it.
  }, [projects.length]);

  // The AI setup question, asked once. It is deliberately not gated on a project: choosing a
  // provider is a decision about this installation rather than about one wedding, and asking it
  // before the first import is what makes it the *first* thing rather than an interruption in
  // the middle of one.
  useEffect(() => {
    if (!inTauri()) {
      setAiAnswered(true);
      return;
    }
    void api
      .aiSetupStatus()
      .then((status) => {
        setAiAnswered(status.completed);
        // Cloud setup is optional; local automatic editing needs no credentials.
      })
      .catch(() => {
        // A credential store that will not answer must not stop the application opening.
        // The AI panel reports the same failure in a place where it is actionable.
        setAiAnswered(true);
      });
  }, []);

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

  // The import bar, and the only thing that ever clears it.
  //
  // `startImport` sets `running` and returns; nothing here used to ask what happened, so the
  // wizard read "0 of ? files" for the life of the window, the Stop button stayed live, and the
  // grid did not refresh until the project was switched away and back. The counts come from the
  // pass itself rather than from counting rows, because a file that turned out to be a duplicate
  // is a file the import finished and not a photograph it created.
  useEffect(() => {
    if (!progress.running || !progress.jobId || !inTauri()) {
      return;
    }
    const jobId = progress.jobId;
    let cancelled = false;

    const finish = (): void => {
      setProgress({ running: false, jobId: null });
      setGuideRefresh((current) => current + 1);
      void refreshProjects();
      if (activeProjectId) {
        void loadPage(activeProjectId, 0, true);
        void api.listProblems(activeProjectId).then(setProblems).catch(() => undefined);
      }
    };

    const timer = window.setInterval(() => {
      void api
        .ingestProgress(jobId)
        .then((update) => {
          if (cancelled) {
            return;
          }
          setProgress({ done: update.done, total: update.total });
          if (!update.running) {
            finish();
          }
        })
        .catch(() => {
          if (!cancelled) {
            finish();
          }
        });
    }, 400);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [
    progress.running,
    progress.jobId,
    activeProjectId,
    loadPage,
    setProblems,
    setProgress,
    refreshProjects,
  ]);

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
          setGuideRefresh((current) => current + 1);
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
      if (!inTauri() || roots.length === 0 || automaticBusy(useAutomatic.getState())) {
        return;
      }
      useAutomatic.setState({ starting: true, error: null });
      try {
        const handle = await automaticStart({ projectId: activeProjectId, roots });
        useAutomatic.getState().adopt(handle.jobId, handle.projectId);
        setActiveProject(handle.projectId);
        setProgress({ running: true, jobId: handle.ingestJobId, done: 0, total: 0 });
        void refreshProjects();
      } catch (error) {
        useAutomatic.setState({ starting: false });
        const ipc = asIpcError(error);
        setError({ code: ipc.code, message: ipc.message });
      }
    },
    [activeProjectId, setError, setProgress, setActiveProject, refreshProjects],
  );

  const cancelImport = useCallback(async () => {
    const automaticJob = useAutomatic.getState().jobId;
    if (automaticJob && busy) {
      await oneClickCancel(automaticJob);
      return;
    }
    if (progress.jobId && inTauri()) {
      await api.cancelJob(progress.jobId);
      setProgress({ running: false, jobId: null });
    }
  }, [progress.jobId, setProgress, busy]);

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
   * thing by it: show me that one. In the old shell a focus change while another workspace was
   * up scrolled a hidden grid; here the jump also opens Library, because "show me" has to be
   * *seen* to be an answer. A photograph in this product is only ever understood beside the
   * frames around it.
   */
  const openPhoto = useCallback(
    (photoId: string) => {
      const index = rows.findIndex((row) => row.id === photoId);
      if (index >= 0) {
        focusIndex(index);
        selectOnly(photoId);
        setTool('library');
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

  /** The tabs this step offers, then the always-available system tools, deduplicated. */
  const stageTools = useMemo(() => {
    const seen = new Set<ToolId>();
    return [...STAGE_TOOLS[step], ...SYSTEM_TOOLS].filter((entry) => {
      if (seen.has(entry.id)) {
        return false;
      }
      seen.add(entry.id);
      return true;
    });
  }, [step]);

  /**
   * The middle of the application: one tool's panel, full width.
   *
   * The panels themselves are unchanged and keep their props; what changed is that
   * only the *open* one is mounted. The old shell kept ten sidebar panels mounted and
   * visible at once - every one of them polling the catalog on its own cadence even
   * while the photographer was reading a different one. A stage of one tool is less
   * traffic, less visual noise, and the same "one answer per question" discipline the
   * data layer has always had.
   */
  const stageContent = (): JSX.Element | null => {
    if (!activeProjectId && tool !== 'weddings') {
      return (
        <WelcomeScreen
          onImport={(roots) => void startImport(roots)}
          busy={busy}
          aiAnswered={aiAnswered === true}
          onCreateProject={(name) => void createProject(name)}
          onFirstRun={() => setTourOpen(true)}
        />
      );
    }
    switch (tool) {
      case 'weddings':
        return (
          <ProjectSwitcher
            projects={projects}
            activeProjectId={activeProjectId}
            onSelect={setActiveProject}
            onCreate={(name) => void createProject(name)}
          />
        );
      case 'import':
        return (
          <ImportWizard
            automatic
            disabled={busy}
            running={progress.running}
            done={progress.done}
            total={progress.total}
            onStart={(roots) => void startImport(roots)}
            onCancel={() => void cancelImport()}
          />
        );
      case 'problems':
        return <ProblemsPanel problems={problems} />;
      case 'library':
        return activeProjectId ? (
          <div className="workspace">
            <div className="photo-browser">
              {/* PHASE-09 and PHASE-11. The chips are the catalog's own answers: every one of
                  them is a query rather than a verdict, and a chip that found nothing and a
                  chip nobody could evaluate are drawn differently. */}
              <FilterChips
                projectId={activeProjectId}
                onSelect={(photoIds) => setFiltered(photoIds.length === 0 ? null : photoIds)}
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
        ) : null;
      case 'autopilot':
        return activeProjectId ? <>
          <Filmstrip rows={rows} />
          <button type="button" onClick={() => void loadPage(activeProjectId, loadedPages, false)}>Load more photos</button>
          <PhotoAnalysis projectId={activeProjectId} photoId={focusedPhotoId} />
          <details><summary>Advanced model analysis</summary><AutopilotPanel projectId={activeProjectId} onError={setError} /></details>
        </> : null;
      case 'people':
        return activeProjectId ? <PeoplePanel projectId={activeProjectId} onError={setError} /> : null;
      case 'story':
        return activeProjectId ? <StoryPanel projectId={activeProjectId} onError={setError} /> : null;
      case 'moments':
        return activeProjectId ? <MomentStack projectId={activeProjectId} /> : null;
      case 'gallery':
        return activeProjectId ? (
          <GalleryPanel
            projectId={activeProjectId}
            selectedPhotoId={focusedPhotoId}
            onSelectPhoto={openPhoto}
            onError={setError}
          />
        ) : null;
      case 'qc':
        return activeProjectId ? <QcPanel projectId={activeProjectId} onError={setError} /> : null;
      case 'cameras':
        return activeProjectId ? (
          <CameraMatchPanel projectId={activeProjectId} onError={setError} />
        ) : null;
      case 'cull':
        return activeProjectId ? (
          <CullView projectId={activeProjectId} onOpenImage={openPhoto} />
        ) : null;
      case 'curate':
        return activeProjectId ? <CuratePanel projectId={activeProjectId} onError={setError} /> : null;
      case 'develop':
        return activeProjectId ? (
          <div>
            <Filmstrip rows={rows} />
            <button type="button" onClick={() => void loadPage(activeProjectId, loadedPages, false)}>Load more photos</button>
            <div className="workspace">
            <DevelopWorkspace
              key={`${activeProjectId}:${focusedPhotoId}`}
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
          </div>
        ) : null;
      case 'cleanup':
        return activeProjectId ? (
          <CleanupPanel projectId={activeProjectId} photoId={focusedPhotoId} onError={setError} />
        ) : null;
      case 'style':
        return activeProjectId ? <StylePanel projectId={activeProjectId} onError={setError} /> : null;
      case 'oneClick':
        return (
          <OneClickRunner
            onFinished={() => {
              setGuideRefresh((current) => current + 1);
            }}
          />
        );
      case 'delivery':
        return activeProjectId ? (
          <DeliveryPanel projectId={activeProjectId} profileId={null} onError={setError} />
        ) : null;
      case 'cache':
        return <CacheSettings projectId={activeProjectId} onError={setError} />;
      case 'hardware':
        return <HardwarePanel onError={setError} />;
      case 'ai':
        return (
          <AiKeysPanel
            projectId={activeProjectId}
            onError={setError}
            onOpenSetup={() => setSetupOpen(true)}
          />
        );
      case 'log':
        return <LogPanel />;
      default:
        return null;
    }
  };

  const activeStep = STEPS_BY_ID[step];

  return (
    <div className="app">
      {setupOpen && aiAnswered !== null ? (
        <AiSetup
          onDone={(status) => {
            setAiAnswered(status.completed);
            setSetupOpen(false);
          }}
          onDismiss={() => setSetupOpen(false)}
          onError={setError}
        />
      ) : null}
      {tourOpen ? <OnboardingOverlay onDismiss={() => setTourOpen(false)} /> : null}

      <header className="topbar">
        <span className="sidebar-brand">AURA</span>
        <span className="topbar-project">
          {activeProjectId === null
            ? 'No wedding open'
            : (projects.find((project) => project.id === activeProjectId)?.name ?? 'Wedding')}
        </span>
        <ThemeToggle />
      </header>

      <main className="main">
        <AutomaticProgress onFinished={(projectId) => {
          setGuideRefresh(current => current + 1);
          if (projectId === activeProjectId) void loadPage(projectId, 0, true);
          void refreshProjects();
        }} />
        <WorkflowGuide currentStep={step} onGo={goToStep} refreshToken={guideRefresh} />

        <div className="stage-nav">
          <div className="stage-tabs" role="tablist" aria-label={`${activeStep.title} tools`}>
            {stageTools.map((entry, index) => {
              const isSystem = SYSTEM_TOOLS.some((row) => row.id === entry.id);
              const divider =
                isSystem && index > 0 && !SYSTEM_TOOLS.some((row) => row.id === stageTools[index - 1]?.id);
              return (
                <span key={entry.id} className="stage-tab-wrap">
                  {divider ? <span className="stage-tab-divider" aria-hidden="true" /> : null}
                  <button
                    type="button"
                    role="tab"
                    aria-selected={tool === entry.id}
                    title={entry.purpose}
                    className={tool === entry.id ? 'is-active' : undefined}
                    onClick={() => {
                      setTool(entry.id);
                    }}
                  >
                    {entry.title}
                  </button>
                </span>
              );
            })}
          </div>
          <p className="stage-purpose">
            <span className="stage-step-number" aria-hidden="true">
              {activeStep.number}
            </span>
            {activeStep.purpose}
          </p>
        </div>

        {lastError && (
          <div className="banner" role="alert">
            <strong>{lastError.code}</strong> {lastError.message}
            <button type="button" onClick={() => setError(null)}>
              Dismiss
            </button>
          </div>
        )}

        <section className="stage" aria-label={TOOLS[tool].purpose}>
          {stageContent()}
        </section>
      </main>
    </div>
  );
}
