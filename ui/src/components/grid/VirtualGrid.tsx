import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { ImageRowLite } from '../../ipc/types';
import { useStore } from '../../state/store';
import { useThumbnails } from '../../stores/thumbnailStore';
import { Cell } from './Cell';

export type VirtualGridProps = {
  rows: ImageRowLite[];
  cellSize?: number;
  gap?: number;
  /** Called when the viewport approaches the end of the loaded rows. */
  onNeedMore?: () => void;
};

/** Rows of cells rendered above and below the viewport. */
const OVERSCAN_ROWS = 2;

/**
 * Windowed grid. Only visible cells plus two rows of overscan exist in the DOM,
 * which is what keeps frame time under 16.6 ms with 4,000 items.
 */
export function VirtualGrid({
  rows,
  cellSize = 160,
  gap = 8,
  onNeedMore,
}: VirtualGridProps): JSX.Element {
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(600);
  const [viewportWidth, setViewportWidth] = useState(960);

  const activeProjectId = useStore((state) => state.activeProjectId);
  const releaseThumbnails = useThumbnails((state) => state.release);
  const previouslyVisible = useRef<string[]>([]);
  const selection = useStore((state) => state.selection);
  const focusedIndex = useStore((state) => state.focusedIndex);
  const focusIndex = useStore((state) => state.focusIndex);
  const selectOnly = useStore((state) => state.selectOnly);
  const toggleSelection = useStore((state) => state.toggleSelection);
  const selectAll = useStore((state) => state.selectAll);

  const stride = cellSize + gap;
  const columns = Math.max(1, Math.floor((viewportWidth + gap) / stride));
  const totalRows = Math.ceil(rows.length / columns);
  const totalHeight = totalRows * stride;

  const firstVisibleRow = Math.max(0, Math.floor(scrollTop / stride) - OVERSCAN_ROWS);
  const visibleRowCount = Math.ceil(viewportHeight / stride) + OVERSCAN_ROWS * 2;
  const firstIndex = firstVisibleRow * columns;
  const lastIndex = Math.min(rows.length, firstIndex + visibleRowCount * columns);

  // Named `visible`, never `window`: shadowing the global here breaks the resize
  // fallback in any environment without ResizeObserver.
  const visible = useMemo(() => rows.slice(firstIndex, lastIndex), [rows, firstIndex, lastIndex]);

  useEffect(() => {
    const element = viewportRef.current;
    if (!element) {
      return;
    }
    const measure = (): void => {
      setViewportHeight(element.clientHeight || 600);
      setViewportWidth(element.clientWidth || 960);
    };
    measure();

    // ResizeObserver is absent in jsdom and in older web views; the window
    // listener keeps the grid correct there instead of throwing on mount.
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure);
      return () => window.removeEventListener('resize', measure);
    }

    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // Cells that have left the window are not merely lower priority: their decode
  // is abandoned, so the queue never fills with frames nobody is looking at.
  useEffect(() => {
    const nowVisible = visible.map((row) => row.id);
    const gone = previouslyVisible.current.filter((id) => !nowVisible.includes(id));
    previouslyVisible.current = nowVisible;
    if (activeProjectId && gone.length > 0) {
      void releaseThumbnails(activeProjectId, gone);
    }
  }, [activeProjectId, releaseThumbnails, visible]);

  const handleScroll = useCallback(
    (event: React.UIEvent<HTMLDivElement>) => {
      const next = event.currentTarget.scrollTop;
      setScrollTop(next);
      const remaining = totalHeight - (next + viewportHeight);
      if (onNeedMore && remaining < stride * 4) {
        onNeedMore();
      }
    },
    [onNeedMore, stride, totalHeight, viewportHeight],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      const step: Record<string, number> = {
        ArrowRight: 1,
        ArrowLeft: -1,
        ArrowDown: columns,
        ArrowUp: -columns,
        PageDown: columns * Math.max(1, Math.floor(viewportHeight / stride)),
        PageUp: -columns * Math.max(1, Math.floor(viewportHeight / stride)),
      };

      if (event.key in step) {
        event.preventDefault();
        const delta = step[event.key] ?? 0;
        const next = Math.min(Math.max(0, focusedIndex + delta), Math.max(0, rows.length - 1));
        focusIndex(next);
        const row = Math.floor(next / columns);
        const element = viewportRef.current;
        if (element) {
          const top = row * stride;
          if (top < element.scrollTop) {
            element.scrollTop = top;
          } else if (top + stride > element.scrollTop + viewportHeight) {
            element.scrollTop = top + stride - viewportHeight;
          }
        }
        return;
      }

      if (event.key === 'a' && (event.ctrlKey || event.metaKey)) {
        event.preventDefault();
        selectAll();
        return;
      }

      if (event.key === ' ' || event.key === 'Enter') {
        event.preventDefault();
        const focused = rows[focusedIndex];
        if (focused) {
          if (event.shiftKey || event.ctrlKey || event.metaKey) {
            toggleSelection(focused.id);
          } else {
            selectOnly(focused.id);
          }
        }
      }
    },
    [columns, focusIndex, focusedIndex, rows, selectAll, selectOnly, stride, toggleSelection, viewportHeight],
  );

  return (
    <div
      ref={viewportRef}
      className="grid-viewport"
      role="grid"
      aria-rowcount={totalRows}
      aria-colcount={columns}
      aria-label="Wedding photographs"
      tabIndex={0}
      onScroll={handleScroll}
      onKeyDown={handleKeyDown}
    >
      <div className="grid-canvas" style={{ height: `${totalHeight}px` }}>
        {visible.map((row, offset) => {
          const index = firstIndex + offset;
          const column = index % columns;
          const gridRow = Math.floor(index / columns);
          return (
            <Cell
              key={row.id}
              row={row}
              index={index}
              size={cellSize}
              x={column * stride}
              y={gridRow * stride}
              selected={selection.has(row.id)}
              focused={index === focusedIndex}
              onFocus={focusIndex}
              onSelect={selectOnly}
              onToggle={toggleSelection}
            />
          );
        })}
      </div>
    </div>
  );
}
