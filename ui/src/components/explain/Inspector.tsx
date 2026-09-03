import { useState } from 'react';

import { SimilarPanel } from '../SimilarPanel';
import { CompositionCard } from './CompositionCard';
import { EmotionCard } from './EmotionCard';
import { ExplainPanel } from './ExplainPanel';
import { IntegrityCard } from './IntegrityCard';
import { MomentBrowser } from './MomentBrowser';

/**
 * Everything AURA has to say about one photograph, in one rail.
 *
 * Six readings of the same frame: whether it worked (phase 09), what it is worth (phase 10),
 * how it is composed (phase 11), why the product decided what it did (phase 13), what looks
 * like it (phase 05) and which frames were shot at the same instant (phases 08 and 10). Every
 * one of them existed with tests and none was reachable from `main.tsx` except the composition
 * card - `PHASE-01-30-REVIEW.md` section 6.4.
 *
 * **Tabs rather than a stack.** These are six answers to six different questions, and a rail
 * that showed all of them at once would be a column a photographer scrolls past rather than
 * reads. Only the open tab fetches, which also keeps six commands off the wire on every
 * selection change.
 *
 * **Nothing here decides anything.** Every card in it is evidence - phase 09's rule, inherited
 * by 10, 11 and 13 - so there is no keep, no reject and no delete on this surface. The one
 * control that writes is `MomentBrowser`'s preference, which records which frame of a moment a
 * photographer likes and culls nothing.
 */
export type InspectorProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** The selected photograph, or null. */
  photoId: string | null;
  /** Jump the grid to another photograph. */
  onSelect?: (photoId: string) => void;
  /** Surface an error to the app's banner. */
  onError: (error: { code: string; message: string } | null) => void;
};

const TABS = [
  { id: 'integrity', title: 'Frame' },
  { id: 'emotion', title: 'Moment' },
  { id: 'composition', title: 'Framing' },
  { id: 'explain', title: 'Why' },
  { id: 'similar', title: 'Alike' },
  { id: 'moments', title: 'Best of' },
] as const;

type TabId = (typeof TABS)[number]['id'];

export function Inspector({
  projectId,
  photoId,
  onSelect,
  onError,
}: InspectorProps): JSX.Element {
  const [open, setOpen] = useState<TabId>('integrity');

  if (!photoId) {
    return (
      <aside className="inspector" aria-label="Inspect selected photograph">
        <p className="empty">Select a photograph to see what AURA makes of it.</p>
      </aside>
    );
  }

  return (
    <aside className="inspector" aria-label="Inspect selected photograph">
      <nav className="inspector__tabs" aria-label="What to inspect">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            aria-pressed={open === tab.id}
            className={open === tab.id ? 'is-open' : undefined}
            onClick={() => setOpen(tab.id)}
          >
            {tab.title}
          </button>
        ))}
      </nav>

      <div className="inspector__body">
        {open === 'integrity' ? <IntegrityCard photoId={photoId} /> : null}
        {open === 'emotion' ? <EmotionCard photoId={photoId} /> : null}
        {open === 'composition' ? <CompositionCard photoId={photoId} /> : null}
        {open === 'explain' ? <ExplainPanel photoId={photoId} /> : null}
        {open === 'similar' && projectId ? (
          <SimilarPanel
            projectId={projectId}
            photoId={photoId}
            onSelect={onSelect}
            onError={onError}
          />
        ) : null}
        {open === 'moments' && projectId ? (
          <MomentBrowser projectId={projectId} onSelect={onSelect} />
        ) : null}
      </div>
    </aside>
  );
}
