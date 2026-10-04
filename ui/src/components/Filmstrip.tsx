import { memo, useEffect, useRef } from 'react';
import type { ImageRowLite } from '../ipc/types';
import { useStore } from '../state/store';
import { useThumbnails } from '../stores/thumbnailStore';

const FilmstripPhoto = memo(function FilmstripPhoto({ row }: { row: ImageRowLite }) {
  const projectId = useStore(state => state.activeProjectId);
  const request = useThumbnails(state => state.request);
  const thumbnail = useThumbnails(state => state.entries.get(row.id));
  const failure = useThumbnails(state => state.failed.get(row.id));
  useEffect(() => {
    if (projectId) void request(projectId, row.id);
  }, [projectId, request, row.id]);
  return <>
    {thumbnail ? <img src={thumbnail.dataUrl} alt="" loading="lazy" decoding="async" /> :
      <span className="strip-placeholder">{failure ? 'Preview unavailable' : 'Loading preview…'}</span>}
    <span className="strip-name">{row.fileName}</span>
  </>;
});

export type FilmstripProps = {
  rows: ImageRowLite[];
  window?: number;
};

/**
 * A narrow strip around the focused frame. It renders a fixed number of cells,
 * so its cost does not grow with the wedding.
 */
export function Filmstrip({ rows, window = 24 }: FilmstripProps): JSX.Element {
  const strip = useRef<HTMLDivElement>(null);
  const focusedIndex = useStore((state) => state.focusedIndex);
  const focusIndex = useStore((state) => state.focusIndex);

  const half = Math.floor(window / 2);
  const start = Math.max(0, Math.min(focusedIndex - half, Math.max(0, rows.length - window)));
  const slice = rows.slice(start, start + window);
  useEffect(() => {
    strip.current?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  }, [focusedIndex]);

  return (
    <div ref={strip} className="filmstrip" role="listbox" aria-label="Filmstrip" aria-orientation="horizontal"
      onKeyDown={event => {
        const moves: Record<string, number> = { ArrowLeft: focusedIndex - 1, ArrowRight: focusedIndex + 1, Home: 0, End: rows.length - 1 };
        const next = moves[event.key];
        if (next === undefined || (event.target as HTMLButtonElement).disabled) return;
        event.preventDefault();
        focusIndex(Math.max(0, Math.min(rows.length - 1, next)));
        requestAnimationFrame(() => strip.current?.querySelector<HTMLButtonElement>('[aria-selected="true"]')?.focus());
      }}>
      {slice.map((row, offset) => {
        const index = start + offset;
        return (
          <button
            key={row.id}
            type="button"
            role="option"
            aria-selected={index === focusedIndex}
            aria-label={row.fileName}
            tabIndex={index === focusedIndex ? 0 : -1}
            className={index === focusedIndex ? 'strip-cell strip-cell-active' : 'strip-cell'}
            onClick={() => focusIndex(index)}
          >
            <FilmstripPhoto row={row} />
          </button>
        );
      })}
    </div>
  );
}
