import type { ImageRowLite } from '../ipc/types';
import { useStore } from '../state/store';

export type FilmstripProps = {
  rows: ImageRowLite[];
  window?: number;
};

/**
 * A narrow strip around the focused frame. It renders a fixed number of cells,
 * so its cost does not grow with the wedding.
 */
export function Filmstrip({ rows, window = 24 }: FilmstripProps): JSX.Element {
  const focusedIndex = useStore((state) => state.focusedIndex);
  const focusIndex = useStore((state) => state.focusIndex);

  const half = Math.floor(window / 2);
  const start = Math.max(0, Math.min(focusedIndex - half, Math.max(0, rows.length - window)));
  const slice = rows.slice(start, start + window);

  return (
    <div className="filmstrip" role="listbox" aria-label="Filmstrip">
      {slice.map((row, offset) => {
        const index = start + offset;
        return (
          <button
            key={row.id}
            type="button"
            role="option"
            aria-selected={index === focusedIndex}
            className={index === focusedIndex ? 'strip-cell strip-cell-active' : 'strip-cell'}
            onClick={() => focusIndex(index)}
          >
            {row.fileName}
          </button>
        );
      })}
    </div>
  );
}
