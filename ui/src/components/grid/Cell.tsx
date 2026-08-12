import { memo } from 'react';

import type { ImageRowLite } from '../../ipc/types';

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
 * One grid cell. Thumbnails arrive in Phase 02: this component already subscribes
 * by photo id, so filling in real pixels needs no change here.
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
  const classes = ['cell'];
  if (selected) {
    classes.push('cell-selected');
  }
  if (focused) {
    classes.push('cell-focused');
  }
  if (row.status !== 'indexed') {
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
      <div className="cell-thumb" aria-hidden="true" />
      <div className="cell-caption">
        <span className="cell-name">{row.fileName}</span>
        {row.status !== 'indexed' && <span className="cell-status">{row.status}</span>}
      </div>
    </div>
  );
}

export const Cell = memo(CellImpl);
