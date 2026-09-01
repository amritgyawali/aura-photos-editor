import type { QcStatusDto } from '../../ipc/types';

/**
 * PHASE-27. The ten inspections, as chips, with what each one found.
 *
 * Pure - counts and a callback in, nothing fetched - like every view beside it.
 *
 * ## Why a chip shows two numbers and not one
 *
 * A category with no findings means one of two completely different things: AURA checked every
 * delivered frame against it and they were all fine, or AURA could not check. In this build the
 * second is the common case - phase 06's detector finds no faces, so the skin and crop checks skip
 * on nearly everything - and a chip that read "Skin 0" would tell a photographer their gallery's
 * skin had been inspected when it had not.
 *
 * So a chip that found nothing says **"not checked"** when it also skipped, and it is styled as
 * unavailable rather than as a pass. There is no state of this component in which an unrun check
 * renders like a clean one.
 */
export type CategoryFilterProps = {
  /** The project header, or null while it loads. */
  status: QcStatusDto | null;
  /** Which category is being filtered on, or null for all of them. */
  selected: string | null;
  /** Change the filter. `null` clears it. */
  onSelect: (category: string | null) => void;
};

/**
 * The ten inspections, in `QcCategory::ALL` order, with the words a photographer uses.
 *
 * The slug is the wire's and the label is the panel's, which is phase 09's rule about a reason
 * storing its code rather than its sentence, applied at the other end: the catalog holds
 * `consistency` and this file decides it reads "Matching the room".
 */
const CATEGORIES: Array<{ slug: string; label: string }> = [
  { slug: 'consistency', label: 'Matching the room' },
  { slug: 'skin', label: 'Skin' },
  { slug: 'exposure', label: 'Brightness' },
  { slug: 'sharpness', label: 'Detail' },
  { slug: 'retouch', label: 'Retouching' },
  { slug: 'mask', label: 'Edges' },
  { slug: 'crop', label: 'Framing' },
  { slug: 'cleanup', label: 'Tidying' },
  { slug: 'duplicate', label: 'Near-duplicates' },
  { slug: 'coverage', label: 'Coverage' },
];

export function CategoryFilter({ status, selected, onSelect }: CategoryFilterProps) {
  const counts = status?.byCategory ?? [];
  // One skip count for the whole pass rather than ten, because the outline carries it that way.
  // A category with no findings inside a pass that skipped anything cannot be presented as clean.
  const anySkipped = (status?.inspectionsSkipped ?? 0) > 0;

  return (
    <nav className="qc-categories" aria-label="Findings by inspection">
      <button
        type="button"
        className={selected === null ? 'qc-categories__chip is-selected' : 'qc-categories__chip'}
        aria-pressed={selected === null}
        onClick={() => onSelect(null)}
      >
        Everything
        <span className="qc-categories__count">{status?.open ?? 0}</span>
      </button>
      {CATEGORIES.map((category, index) => {
        const found = counts[index] ?? 0;
        const unchecked = found === 0 && anySkipped;
        const classes = ['qc-categories__chip'];
        if (selected === category.slug) {
          classes.push('is-selected');
        }
        if (unchecked) {
          classes.push('is-unavailable');
        }
        return (
          <button
            key={category.slug}
            type="button"
            className={classes.join(' ')}
            aria-pressed={selected === category.slug}
            onClick={() => onSelect(selected === category.slug ? null : category.slug)}
          >
            {category.label}
            <span className="qc-categories__count">{unchecked ? 'not checked' : found}</span>
          </button>
        );
      })}
    </nav>
  );
}
