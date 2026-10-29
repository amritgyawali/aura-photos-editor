import { useEffect, useState } from 'react';

import type { FullEditProgress, StagePicture } from '../../ipc/fullEdit';

/** One picture of the photograph: the original (stage 0), or after a step that changed it. */
export type LivePicture = StagePicture & { number: number; title: string };

/** A photograph being edited, or edited while someone watched, with every picture of it. */
export type LivePhoto = {
  photoId: string; name: string; index: number; total: number;
  pictures: LivePicture[];
  /** The stage running now, while the photograph is being edited. */
  working: string | null;
  outcome: 'editing' | 'ready' | 'failed';
};

/** How many photographs keep their pictures: each holds a dozen or so screen-sized JPEGs. */
export const KEPT_PHOTOS = 12;

/** A photograph starts: it goes last, and the oldest beyond `KEPT_PHOTOS` lose their pictures. */
export function startPhoto(photos: readonly LivePhoto[], photo: Pick<LivePhoto, 'photoId' | 'name' | 'index' | 'total'>): LivePhoto[] {
  const next = [...photos.filter(row => row.photoId !== photo.photoId), { ...photo, pictures: [], working: null, outcome: 'editing' as const }];
  return next.slice(-KEPT_PHOTOS);
}

/** A stage event or a picture of the photograph it belongs to. */
export function photoEvent(photos: readonly LivePhoto[], event: FullEditProgress): LivePhoto[] {
  return photos.map(photo => {
    if (photo.photoId !== event.photoId || photo.outcome !== 'editing') return photo;
    if (event.state === 'picture' && event.picture) {
      return { ...photo, pictures: [...photo.pictures, { ...event.picture, number: event.number, title: event.title }] };
    }
    if (event.state === 'running') return { ...photo, working: `Stage ${event.number} of ${event.total}: ${event.title}` };
    return photo;
  });
}

/** The photograph is finished: it keeps its pictures and stops saying what is running. */
export function finishPhoto(photos: readonly LivePhoto[], photoId: string, outcome: 'ready' | 'failed'): LivePhoto[] {
  return photos.map(photo => photo.photoId === photoId ? { ...photo, working: null, outcome } : photo);
}

/** How long each picture stays on screen when a finished edit is replayed. */
const REPLAY_MS = 900;

type View = 'steps' | 'compare';

/**
 * The automatic edit as it happens: the photograph being edited, large, changing after every step
 * that changed it, with the steps beneath it - from the original to the finished photograph. A
 * step can be picked to look at it, the original compared beside the result, and the edit of any
 * of the last few photographs replayed.
 */
export function LiveEditPreview({ photos, focus, onFocus }: {
  photos: readonly LivePhoto[];
  /** The photograph to show; the one being edited when null. */
  focus: string | null;
  onFocus: (photoId: string | null) => void;
}): JSX.Element | null {
  const live = photos.find(photo => photo.outcome === 'editing') ?? photos[photos.length - 1] ?? null;
  const photo = (focus ? photos.find(row => row.photoId === focus) : null) ?? live;
  /** The picked step; null follows the newest picture. */
  const [step, setStep] = useState<number | null>(null);
  const [view, setView] = useState<View>('steps');
  const [replaying, setReplaying] = useState(false);
  const photoId = photo?.photoId ?? null;
  useEffect(() => { setStep(null); setReplaying(false); }, [photoId]);
  const count = photo?.pictures.length ?? 0;
  useEffect(() => {
    if (!replaying) return undefined;
    const timer = window.setInterval(() => {
      setStep(current => {
        const next = (current ?? -1) + 1;
        if (next >= count - 1) { setReplaying(false); return null; }
        return next;
      });
    }, REPLAY_MS);
    return () => window.clearInterval(timer);
  }, [replaying, count]);

  if (!photo) return null;
  const pictures = photo.pictures;
  const shown = step === null ? pictures[pictures.length - 1] : pictures[Math.min(step, pictures.length - 1)];
  const original = pictures[0];
  const editing = photo.outcome === 'editing';
  const heading = editing
    ? `Editing photo ${photo.index + 1} of ${photo.total}: ${photo.name}`
    : `${photo.name} · ${photo.outcome === 'ready' ? 'edited' : 'needs attention'}`;
  const position = shown ? pictures.indexOf(shown) : -1;

  return (
    <section className="live-edit" aria-label="Live edit preview">
      <header className="live-edit-head">
        <div>
          <span className="eyebrow">{editing ? 'EDITING NOW' : 'FINISHED EDIT'}</span>
          <h3>{heading}</h3>
          <p role="status">{editing
            ? photo.working ?? 'Reading the photograph…'
            : `${Math.max(pictures.length - 1, 0)} steps changed this photo.`}</p>
        </div>
        <div className="live-edit-actions" role="group" aria-label="How to show the edit">
          <button type="button" aria-pressed={view === 'steps'} onClick={() => setView('steps')}>Step by step</button>
          <button type="button" aria-pressed={view === 'compare'} disabled={pictures.length < 2} onClick={() => setView('compare')}>Before | After</button>
          {!editing && pictures.length > 2 && <button type="button" onClick={() => { setView('steps'); setStep(0); setReplaying(true); }} disabled={replaying}>{replaying ? 'Replaying…' : 'Replay the edit'}</button>}
          {focus && live && live.photoId !== photo.photoId && live.outcome === 'editing' && <button type="button" className="is-primary" onClick={() => onFocus(null)}>Back to the photo being edited</button>}
        </div>
      </header>

      {view === 'compare' && original && shown && pictures.length > 1 ? (
        <div className="live-edit-compare">
          <figure><img src={original.image} alt={`${photo.name} before the edit`} /><figcaption>Before</figcaption></figure>
          <figure><img src={pictures[pictures.length - 1]?.image} alt={`${photo.name} after ${pictures[pictures.length - 1]?.step}`} />
            <figcaption>{editing ? `So far: ${pictures[pictures.length - 1]?.step}` : 'After'}</figcaption></figure>
        </div>
      ) : (
        <figure className="live-edit-stage" data-editing={editing || undefined}>
          {shown ? <img src={shown.image} alt={`${photo.name}: ${shown.step}`} /> : <div className="live-edit-empty">Rendering the original…</div>}
          {shown && <figcaption>{position === 0 ? 'Original' : `Step ${position}: ${shown.step}`}
            {step !== null && !replaying && <button type="button" onClick={() => setStep(null)}>{editing ? 'Follow live' : 'Show the result'}</button>}</figcaption>}
        </figure>
      )}

      {pictures.length > 0 && <ol className="live-edit-steps" aria-label={`Every step of ${photo.name}`}>
        {pictures.map((picture, index) => <li key={`${index}-${picture.step}`}>
          <button type="button" aria-current={picture === shown ? 'step' : undefined} onClick={() => { setReplaying(false); setView('steps'); setStep(index === pictures.length - 1 ? null : index); }}>
            <img src={picture.image} alt="" />
            <span>{index === 0 ? 'Original' : `${index}. ${picture.step}`}</span>
          </button>
        </li>)}
        {editing && <li className="live-edit-next" aria-hidden="true"><span>{photo.working ?? '…'}</span></li>}
      </ol>}

      {photos.length > 1 && <div className="live-edit-recent">
        <span>Recent photos</span>
        <ol aria-label="Recently edited photos">
          {photos.map(row => {
            const last = row.pictures[row.pictures.length - 1];
            return <li key={row.photoId}><button type="button" aria-current={row.photoId === photo.photoId ? 'true' : undefined}
              title={row.name} onClick={() => onFocus(row.outcome === 'editing' ? null : row.photoId)}>
              {last ? <img src={last.image} alt="" /> : <i aria-hidden="true" />}<span>{row.outcome === 'editing' ? 'Now' : row.name}</span>
            </button></li>;
          })}
        </ol>
      </div>}
    </section>
  );
}
