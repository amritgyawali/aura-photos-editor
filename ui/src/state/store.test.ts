import { beforeEach, describe, expect, it } from 'vitest';

import type { ImageRowLite } from '../ipc/types';
import { useStore } from './store';

const row = (id: string): ImageRowLite => ({
  id,
  fileName: `${id}.CR2`,
  timelineTs: null,
  cameraId: null,
  width: 0,
  height: 0,
  status: 'indexed',
});

describe('view store', () => {
  beforeEach(() => {
    useStore.setState({
      projects: [],
      activeProjectId: null,
      rows: [],
      loadedPages: 0,
      selection: new Set<string>(),
      focusedIndex: 0,
      problems: [],
      progress: { done: 0, total: 0, running: false, jobId: null },
      lastError: null,
    });
  });

  it('does not duplicate rows when a page arrives twice', () => {
    useStore.getState().appendRows([row('a'), row('b')]);
    useStore.getState().appendRows([row('b'), row('c')]);
    expect(useStore.getState().rows.map((r) => r.id)).toEqual(['a', 'b', 'c']);
  });

  it('select all is one set operation over every loaded row', () => {
    const many = Array.from({ length: 4000 }, (_, index) => row(`p${index}`));
    useStore.getState().replaceRows(many);

    const started = performance.now();
    useStore.getState().selectAll();
    const elapsed = performance.now() - started;

    expect(useStore.getState().selection.size).toBe(4000);
    expect(elapsed).toBeLessThan(50);
  });

  it('toggling selection adds then removes', () => {
    useStore.getState().replaceRows([row('a')]);
    useStore.getState().toggleSelection('a');
    expect(useStore.getState().selection.has('a')).toBe(true);
    useStore.getState().toggleSelection('a');
    expect(useStore.getState().selection.has('a')).toBe(false);
  });

  it('switching project clears rows, selection and problems', () => {
    useStore.getState().replaceRows([row('a')]);
    useStore.getState().selectAll();
    useStore.getState().setActiveProject('prj_2');

    const state = useStore.getState();
    expect(state.rows).toHaveLength(0);
    expect(state.selection.size).toBe(0);
    expect(state.problems).toHaveLength(0);
    expect(state.activeProjectId).toBe('prj_2');
  });
});
