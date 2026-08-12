import { create } from 'zustand';

import type { ImageRowLite, ProblemRow, ProjectSummary } from '../ipc/types';

/** Page size for grid paging. Two screens of a 6-column grid at 4k. */
export const PAGE_SIZE = 240;

export type ImportProgress = {
  done: number;
  total: number;
  running: boolean;
  jobId: string | null;
};

export type ViewState = {
  projects: ProjectSummary[];
  activeProjectId: string | null;
  rows: ImageRowLite[];
  loadedPages: number;
  selection: Set<string>;
  focusedIndex: number;
  problems: ProblemRow[];
  progress: ImportProgress;
  lastError: { code: string; message: string } | null;

  setProjects: (projects: ProjectSummary[]) => void;
  setActiveProject: (projectId: string | null) => void;
  appendRows: (rows: ImageRowLite[]) => void;
  replaceRows: (rows: ImageRowLite[]) => void;
  setProblems: (problems: ProblemRow[]) => void;
  setProgress: (progress: Partial<ImportProgress>) => void;
  setError: (error: { code: string; message: string } | null) => void;
  focusIndex: (index: number) => void;
  toggleSelection: (id: string) => void;
  selectOnly: (id: string) => void;
  selectAll: () => void;
  clearSelection: () => void;
};

export const useStore = create<ViewState>((set) => ({
  projects: [],
  activeProjectId: null,
  rows: [],
  loadedPages: 0,
  selection: new Set<string>(),
  focusedIndex: 0,
  problems: [],
  progress: { done: 0, total: 0, running: false, jobId: null },
  lastError: null,

  setProjects: (projects) => set({ projects }),

  setActiveProject: (activeProjectId) =>
    set({
      activeProjectId,
      rows: [],
      loadedPages: 0,
      selection: new Set<string>(),
      focusedIndex: 0,
      problems: [],
    }),

  appendRows: (rows) =>
    set((state) => {
      const seen = new Set(state.rows.map((row) => row.id));
      const fresh = rows.filter((row) => !seen.has(row.id));
      return { rows: [...state.rows, ...fresh], loadedPages: state.loadedPages + 1 };
    }),

  replaceRows: (rows) => set({ rows, loadedPages: 1 }),

  setProblems: (problems) => set({ problems }),

  setProgress: (progress) => set((state) => ({ progress: { ...state.progress, ...progress } })),

  setError: (lastError) => set({ lastError }),

  focusIndex: (index) => set({ focusedIndex: Math.max(0, index) }),

  toggleSelection: (id) =>
    set((state) => {
      const next = new Set(state.selection);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return { selection: next };
    }),

  selectOnly: (id) => set({ selection: new Set<string>([id]) }),

  // Selecting 4,000 images builds one Set, not 4,000 React updates.
  selectAll: () => set((state) => ({ selection: new Set(state.rows.map((row) => row.id)) })),

  clearSelection: () => set({ selection: new Set<string>() }),
}));
