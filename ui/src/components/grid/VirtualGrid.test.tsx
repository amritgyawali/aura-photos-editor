import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import type { ImageRowLite } from '../../ipc/types';
import { useStore } from '../../state/store';
import { VirtualGrid } from './VirtualGrid';

const rows: ImageRowLite[] = Array.from({ length: 4000 }, (_, index) => ({
  id: `p${index}`,
  fileName: `IMG_${index}.CR2`,
  timelineTs: null,
  cameraId: null,
  width: 0,
  height: 0,
  status: 'indexed',
}));

describe('VirtualGrid', () => {
  beforeEach(() => {
    useStore.setState({ selection: new Set<string>(), focusedIndex: 0 });
  });

  it('renders a window, not four thousand cells', () => {
    render(<VirtualGrid rows={rows} />);
    const cells = screen.getAllByRole('gridcell');
    expect(cells.length).toBeGreaterThan(0);
    expect(cells.length).toBeLessThan(200);
  });

  it('labels the grid for screen readers', () => {
    render(<VirtualGrid rows={rows} />);
    expect(screen.getByRole('grid', { name: 'Wedding photographs' })).toBeTruthy();
  });

  it('handles an empty project without crashing', () => {
    render(<VirtualGrid rows={[]} />);
    expect(screen.queryAllByRole('gridcell')).toHaveLength(0);
  });
});
