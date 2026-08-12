import { memo, useEffect } from 'react';

import type { ImageRowLite } from '../../ipc/types';
import { useStore } from '../../state/store';
import { useThumbnails } from '../../stores/thumbnailStore';

export type CellProps = {
  row: ImageRowLite;
  index: number;
  size: number;
  x: number;
  y: number;
  selected: boolean;
  focused: boolean;
  onFocus: (index: number) => void;
  onSelect: (id: string) => void;
  onToggle: (id: string) => void;
};

/**
 * One grid cell.
 *
 * Phase 01 shipped this component with a grey placeholder and a promise that
 * real pixels would need no change here. That promise is kept: the cell asks the
 * thumbnail store for its own photograph and renders whatever has arrived. It
 * never blocks, never blanks once it has pixels, and upgrades in place when a
 * higher tier lands.
 */
function CellImpl({
  row,
  index,
  size,
  x,
  y,
  selected,
  focused,
  onFocus,
  onSelect,
  onToggle,
}: CellProps): JSX.Element {
  const projectId = useStore((state) => state.activeProjectId);
  const thumbnail = useThumbnails((state) => state.entries.get(row.id));
  const failure = useThumbnails((state) => state.failed.get(row.id));
  const request = useThumbnails((state) => state.request);

  useEffect(() => {
    if (projectId) {
      // A cell that is mounted is a cell the user can see, so this is the
      // `visible` priority class by definition.
      void request(projectId, row.id);
    }
  }, [projectId, request, row.id]);

  const classes = ['cell'];
  if (selected) {
    classes.push('cell-selected');
  }
  if (focused) {
    classes.push('cell-focused');
  }
  if (row.status !== 'indexed' || failure) {
    classes.push('cell-problem');
  }

  return (
    <div
      className={classes.join(' ')}
      style={{ transform: `translate(${x}px, ${y}px)`, width: `${size}px`, height: `${size}px` }}
      role="gridcell"
      aria-selected={selected}
      aria-label={`${row.fileName}${row.timelineTs ? `, ${row.timelineTs}` : ''}`}
      onClick={(event) => {
        onFocus(index);
        if (event.shiftKey || event.ctrlKey || event.metaKey) {
          onToggle(row.id);
        } else {
          onSelect(row.id);
        }
      }}
    >
      {thumbnail ? (
        <img
          className="cell-thumb"
          src={thumbnail.dataUrl}
          alt=""
          width={size}
          height={size}
          loading="lazy"
          decoding="async"
          data-tier={thumbnail.tier}
          data-source={thumbnail.source}
        />
      ) : (
        <div className="cell-thumb cell-thumb-pending" aria-hidden="true" />
      )}
      <div className="cell-caption">
        <span className="cell-name">{row.fileName}</span>
        {row.status !== 'indexed' && <span className="cell-status">{row.status}</span>}
        {failure && <span className="cell-status">preview failed</span>}
      </div>
    </div>
  );
}

export const Cell = memo(CellImpl);
