import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  asIpcError,
  inTauri,
  oneClickFinish,
  photoCullCancel,
  photoCullChecks,
  photoCullReview,
  photoCullStatus,
  pickImportPaths,
  setPhotoCullChoice,
  setPhotoCullStrictness,
  startPhotoCull,
} from '../../ipc/client';
import type { PhotoCullCheckDto, PhotoCullPhotoDto, PhotoCullReviewDto, PhotoCullStatusDto } from '../../ipc/types';
import { automaticBusy, useAutomatic } from '../../state/automaticStore';
import { useThumbnails } from '../../stores/thumbnailStore';

/**
 * The Cull screen. ADR-0185.
 *
 * AURA checks every photograph in the collection against a hundred measurements, keeps the
 * best of every moment and leaves the rest out - and then gets out of the way. Both lists are
 * shown; anything left out is one click from coming back; "Keep every photo" overrules the
 * whole cull; and every choice made here is what Finish Folder and Export deliver. Nothing
 * on this screen deletes, moves or edits a photograph.
 *
 * Strictness moves between gentle, balanced and strict without measuring anything again, and
 * says what each would deliver first. A photograph's panel shows the rest of its burst or
 * moment, so the frame AURA chose can be swapped for another in one click; the arrow keys
 * walk the list, K keeps, X leaves out, U undoes and Esc closes.
 */

export type CullTab = 'best' | 'left';
export type CullSort = 'score' | 'time';

export type PhotoCullWorkspaceProps = {
  projectId: string;
  disabled: boolean;
  onError: (error: { code: string; message: string } | null) => void;
  /** Open a photograph in the editor. */
  onOpen?: (photoId: string) => void;
};

/** The header sentence, from the review alone. */
export function cullSummary(review: PhotoCullReviewDto): string {
  const parts = [`${review.best} best`, `${review.leftOut} left out`];
  if (review.topPicks > 0) parts.push(`${review.topPicks} top picks`);
  if (review.keptByYou > 0) parts.push(`${review.keptByYou} brought back by you`);
  if (review.leftOutByYou > 0) parts.push(`${review.leftOutByYou} left out by you`);
  return `${review.frames} photos checked: ${parts.join(' · ')}.`;
}

/** The photographs one tab shows, best first or in shooting order, optionally of one reason. */
export function photosFor(review: PhotoCullReviewDto, tab: CullTab, sort: CullSort, topOnly = false, reason: string | null = null): PhotoCullPhotoDto[] {
  const shown = review.photos.filter((photo) => (tab === 'best' ? photo.keep : !photo.keep)
    && (!topOnly || photo.topPick) && (reason === null || photo.reason === reason));
  if (sort === 'time') return shown;
  return [...shown].sort((a, b) => (b.score ?? -1) - (a.score ?? -1));
}

/** The reasons in one tab and how many photos each holds, most first. */
export function reasonCounts(review: PhotoCullReviewDto, tab: CullTab): { reason: string; label: string; count: number }[] {
  const counts = new Map<string, { reason: string; label: string; count: number }>();
  for (const photo of review.photos) {
    if ((tab === 'best') !== photo.keep) continue;
    const entry = counts.get(photo.reason) ?? { reason: photo.reason, label: photo.reasonLabel, count: 0 };
    entry.count += 1;
    counts.set(photo.reason, entry);
  }
  return [...counts.values()].sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
}

/** Every photograph of the same burst or moment, itself included, in shooting order; empty when it stands alone. */
export function siblingsOf(review: PhotoCullReviewDto, photo: PhotoCullPhotoDto): PhotoCullPhotoDto[] {
  const together = review.photos.filter((other) => (photo.burst !== null && other.burst === photo.burst)
    || (photo.moment !== null && other.moment === photo.moment));
  return together.length > 1 ? together : [];
}

/** The photograph `step` places from `id` in the list shown; the first one when `id` has left the list. */
export function stepFrom(shown: PhotoCullPhotoDto[], id: string, step: number): string | null {
  const at = shown.findIndex((photo) => photo.photoId === id);
  if (at < 0) return shown[0]?.photoId ?? null;
  return shown[at + step]?.photoId ?? null;
}

function typing(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || ['INPUT', 'SELECT', 'TEXTAREA'].includes(target.tagName));
}

/** Who decided, in words. */
export function choiceLabel(photo: PhotoCullPhotoDto): string | null {
  if (photo.choice === 'keep') return photo.cullKeep ? 'Kept by you' : 'Brought back by you';
  if (photo.choice === 'leave_out') return 'Left out by you';
  return null;
}

/** The one action a card offers. */
export function actionFor(photo: PhotoCullPhotoDto): { label: string; choice: string } {
  if (photo.choice !== null && photo.choice !== undefined) return { label: 'Undo my choice', choice: 'auto' };
  return photo.keep ? { label: 'Leave out', choice: 'leave_out' } : { label: 'Bring back', choice: 'keep' };
}

/** Checks grouped in the order the backend sent them. */
export function groupChecks(checks: PhotoCullCheckDto[]): { group: string; checks: PhotoCullCheckDto[]; score: number | null }[] {
  const groups: { group: string; checks: PhotoCullCheckDto[]; score: number | null }[] = [];
  for (const check of checks) {
    let entry = groups.find((g) => g.group === check.group);
    if (!entry) {
      entry = { group: check.group, checks: [], score: null };
      groups.push(entry);
    }
    entry.checks.push(check);
  }
  for (const entry of groups) {
    const scored = entry.checks.filter((c) => c.score !== null);
    const weight = scored.reduce((sum, c) => sum + c.weight, 0);
    entry.score = weight > 0 ? Math.round(scored.reduce((sum, c) => sum + (c.score ?? 0) * c.weight, 0) / weight) : null;
  }
  return groups;
}

function scoreClass(score: number | null): string {
  if (score === null) return 'is-none';
  if (score >= 70) return 'is-good';
  if (score >= 45) return 'is-fair';
  return 'is-poor';
}

function Thumb({ projectId, photoId }: { projectId: string; photoId: string }): JSX.Element {
  const entry = useThumbnails((state) => state.entries.get(photoId));
  const request = useThumbnails((state) => state.request);
  useEffect(() => {
    void request(projectId, photoId);
  }, [projectId, photoId, request]);
  return entry ? <img src={entry.dataUrl} alt="" loading="lazy" decoding="async" /> : <div className="cull-thumb-pending" aria-hidden="true" />;
}

function Card({ projectId, photo, selected, disabled, onToggle, onOpen, onChoose }: {
  projectId: string;
  photo: PhotoCullPhotoDto;
  selected: boolean;
  disabled: boolean;
  onToggle: () => void;
  onOpen: () => void;
  onChoose: (choice: string) => void;
}): JSX.Element {
  const action = actionFor(photo);
  const by = choiceLabel(photo);
  return (
    <article className={`cull-card${selected ? ' is-selected' : ''}${photo.topPick ? ' is-top' : ''}`} aria-label={photo.fileName}>
      <button type="button" className="cull-card-image" onClick={onOpen} title="Show its hundred checks">
        <Thumb projectId={projectId} photoId={photo.photoId} />
        {photo.score !== null && <span className={`cull-score ${scoreClass(photo.score)}`}>{Math.round(photo.score)}</span>}
        {photo.topPick && <span className="cull-top" title="Top pick">★ Top pick</span>}
      </button>
      <div className="cull-card-body">
        <label className="cull-select"><input type="checkbox" checked={selected} disabled={disabled} onChange={onToggle} /><span>{photo.fileName}</span></label>
        <span className="cull-reason">{photo.reasonLabel}</span>
        {by && <span className="cull-by">{by}</span>}
        <button type="button" disabled={disabled} onClick={() => onChoose(action.choice)}>{action.label}</button>
      </div>
    </article>
  );
}

function Detail({ projectId, photo, review, disabled, onClose, onOpen, onSelect, onChoose, onSwap }: {
  projectId: string;
  photo: PhotoCullPhotoDto;
  review: PhotoCullReviewDto;
  disabled: boolean;
  onClose: () => void;
  onOpen?: (photoId: string) => void;
  onSelect: (photoId: string) => void;
  onChoose: (choice: string) => void;
  onSwap: (deliver: string, leaveOut: string) => void;
}): JSX.Element {
  const [checks, setChecks] = useState<PhotoCullCheckDto[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setChecks(null);
    setFailed(null);
    photoCullChecks({ projectId, photoId: photo.photoId })
      .then((found) => { if (live) setChecks(found); })
      .catch((err: unknown) => { if (live) setFailed(asIpcError(err).message); });
    return () => { live = false; };
  }, [projectId, photo.photoId]);
  const instead = photo.keptInstead ? review.photos.find((p) => p.photoId === photo.keptInstead) : undefined;
  const siblings = siblingsOf(review, photo);
  const action = actionFor(photo);
  const by = choiceLabel(photo);
  return (
    <aside className="cull-detail" aria-label={`Checks for ${photo.fileName}`}>
      <header>
        <div><strong>{photo.fileName}</strong><span>{photo.score !== null ? `Score ${Math.round(photo.score)} of 100` : 'Not scored'}</span></div>
        <button type="button" onClick={onClose} aria-label="Close">✕</button>
      </header>
      <div className="cull-detail-image"><Thumb projectId={projectId} photoId={photo.photoId} /></div>
      <p className="cull-detail-reason"><strong>{photo.reasonLabel}.</strong> {photo.detail}</p>
      {by && <p className="cull-by">{by}.</p>}
      <div className="cull-detail-actions">
        <button type="button" className="is-primary" disabled={disabled} onClick={() => onChoose(action.choice)}
          title={action.choice === 'keep' ? 'K' : action.choice === 'leave_out' ? 'X' : 'U'}>{action.label}</button>
        {!photo.keep && instead && instead.keep && (
          <button type="button" disabled={disabled} onClick={() => onSwap(photo.photoId, instead.photoId)}>
            Deliver this instead of {instead.fileName}
          </button>
        )}
        {onOpen && <button type="button" onClick={() => onOpen(photo.photoId)}>Open in the editor</button>}
      </div>
      {instead && <p className="studio-footnote">Delivered instead: {instead.fileName}.</p>}
      {siblings.length > 0 && (
        <section className="cull-moment" aria-label={photo.burst !== null ? 'Same burst' : 'Same moment'}>
          <h4>{photo.burst !== null ? 'Same burst' : 'Same moment'} <small>{siblings.length} photos</small></h4>
          <div>
            {siblings.map((other) => (
              <button key={other.photoId} type="button" aria-label={other.fileName} aria-current={other.photoId === photo.photoId}
                className={`${other.keep ? 'is-kept' : 'is-out'}${other.photoId === photo.photoId ? ' is-current' : ''}`}
                onClick={() => onSelect(other.photoId)} title={`${other.fileName}: ${other.reasonLabel}`}>
                <Thumb projectId={projectId} photoId={other.photoId} />
                <span>{other.score !== null ? Math.round(other.score) : '—'} · {other.keep ? 'Best' : 'Left out'}</span>
              </button>
            ))}
          </div>
        </section>
      )}
      <p className="studio-footnote cull-keys">← → next photo · K keep · X leave out · U undo · Esc close</p>
      <ul className="cull-groups" aria-label="Score by group">
        {review.groups.map((group, index) => {
          const score = photo.categories[index] ?? null;
          return <li key={group}><span>{group}</span><meter min={0} max={100} value={score ?? 0} aria-label={group} /><b>{score ?? '—'}</b></li>;
        })}
      </ul>
      <details className="cull-checks">
        <summary>All 100 checks</summary>
        {failed && <p role="alert">{failed}</p>}
        {!checks && !failed && <p className="studio-footnote">Reading the checks…</p>}
        {checks && groupChecks(checks).map((group) => (
          <section key={group.group}>
            <h4>{group.group} <small>{group.score ?? 'not applicable'}</small></h4>
            <ul>
              {group.checks.map((check) => (
                <li key={check.id} className={scoreClass(check.score)} title={check.value !== null ? `Measured ${check.value}` : 'Did not apply to this photo'}>
                  <span>{check.name}</span><b>{check.score ?? '—'}</b>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </details>
    </aside>
  );
}

export function PhotoCullWorkspace({ projectId, disabled, onError, onOpen }: PhotoCullWorkspaceProps): JSX.Element {
  const [review, setReview] = useState<PhotoCullReviewDto | null | undefined>(undefined);
  const [job, setJob] = useState<PhotoCullStatusDto | null>(null);
  const [tab, setTab] = useState<CullTab>('best');
  const [sort, setSort] = useState<CullSort>('score');
  const [topOnly, setTopOnly] = useState(false);
  const [reason, setReason] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [detail, setDetail] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [destination, setDestination] = useState('');
  const finishing = useAutomatic(automaticBusy);
  const polling = useRef<number | null>(null);
  const running = job?.status === 'running';
  const locked = disabled || running || saving || finishing;

  const load = useCallback(async () => {
    try {
      setReview(await photoCullReview(projectId));
    } catch (err) {
      setReview(null);
      onError(asIpcError(err));
    }
  }, [projectId, onError]);

  useEffect(() => {
    setReview(undefined);
    setSelected(new Set());
    setDetail(null);
    setJob(null);
    void load();
  }, [load]);

  useEffect(() => () => { if (polling.current !== null) window.clearTimeout(polling.current); }, []);

  const watch = useCallback((jobId: string) => {
    const tick = async () => {
      try {
        const status = await photoCullStatus(jobId);
        setJob(status);
        if (status.status === 'running') {
          polling.current = window.setTimeout(() => void tick(), 700);
          return;
        }
        polling.current = null;
        if (status.status === 'failed') onError({ code: 'AURA-CULL', message: status.label });
        await load();
      } catch (err) {
        polling.current = null;
        setJob(null);
        onError(asIpcError(err));
      }
    };
    void tick();
  }, [load, onError]);

  const cull = useCallback(async () => {
    try {
      const handle = await startPhotoCull({ projectId });
      setJob({ jobId: handle.jobId, projectId, status: 'running', done: 0, total: 0, label: 'Starting the cull.' });
      watch(handle.jobId);
    } catch (err) {
      onError(asIpcError(err));
    }
  }, [projectId, watch, onError]);

  const choose = useCallback(async (photoIds: string[], choice: string) => {
    if (photoIds.length === 0) return;
    setSaving(true);
    try {
      setReview(await setPhotoCullChoice({ projectId, photoIds, choice }));
      setSelected(new Set());
    } catch (err) {
      onError(asIpcError(err));
    } finally {
      setSaving(false);
    }
  }, [projectId, onError]);

  const restrict = useCallback(async (strictness: string) => {
    setSaving(true);
    try {
      setReview(await setPhotoCullStrictness({ projectId, strictness }));
      setSelected(new Set());
      setReason(null);
    } catch (err) {
      onError(asIpcError(err));
    } finally {
      setSaving(false);
    }
  }, [projectId, onError]);

  const swap = useCallback(async (deliver: string, leaveOut: string) => {
    setSaving(true);
    try {
      await setPhotoCullChoice({ projectId, photoIds: [deliver], choice: 'keep' });
      setReview(await setPhotoCullChoice({ projectId, photoIds: [leaveOut], choice: 'leave_out' }));
    } catch (err) {
      onError(asIpcError(err));
    } finally {
      setSaving(false);
    }
  }, [projectId, onError]);

  const finish = useCallback(async (onlyPhotos: string[]) => {
    if (!inTauri() || automaticBusy(useAutomatic.getState())) return;
    useAutomatic.setState({ starting: true });
    try {
      const handle = await oneClickFinish({ projectId, destination: destination.trim(), ingestJobId: null, onlyPhotos });
      useAutomatic.getState().adopt(handle.jobId, projectId);
    } catch (err) {
      useAutomatic.setState({ starting: false });
      onError(asIpcError(err));
    }
  }, [projectId, destination, onError]);

  const shown = useMemo(() => (review ? photosFor(review, tab, sort, topOnly, reason) : []), [review, tab, sort, topOnly, reason]);
  const reasons = useMemo(() => (review ? reasonCounts(review, tab) : []), [review, tab]);
  const detailPhoto = review && detail ? review.photos.find((p) => p.photoId === detail) : undefined;

  useEffect(() => {
    if (!detailPhoto) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (typing(event.target) || event.ctrlKey || event.metaKey || event.altKey) return;
      const key = event.key.toLowerCase();
      if (key === 'escape') setDetail(null);
      else if (key === 'arrowright' || key === 'arrowleft') {
        const next = stepFrom(shown, detailPhoto.photoId, key === 'arrowright' ? 1 : -1);
        if (next) setDetail(next);
      } else if (locked) return;
      else if (key === 'k' && !detailPhoto.keep) void choose([detailPhoto.photoId], 'keep');
      else if (key === 'x' && detailPhoto.keep) void choose([detailPhoto.photoId], 'leave_out');
      else if (key === 'u' && detailPhoto.choice) void choose([detailPhoto.photoId], 'auto');
      else return;
      event.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [detailPhoto, shown, locked, choose]);
  const leftOutIds = review ? review.photos.filter((p) => !p.keep).map((p) => p.photoId) : [];
  const chosenIds = review ? review.photos.filter((p) => p.choice).map((p) => p.photoId) : [];
  const broughtBack = review ? review.photos.filter((p) => p.keep && !p.cullKeep).map((p) => p.photoId) : [];
  const toggle = (id: string) => setSelected((current) => {
    const next = new Set(current);
    if (next.has(id)) next.delete(id); else next.add(id);
    return next;
  });

  return (
    <div className="photo-cull">
      <header className="workspace-heading">
        <div><span className="eyebrow">THE BEST OF EVERY MOMENT</span><h1>Cull, then decide for yourself.</h1></div>
        <div className="photo-cull-run">
          {running
            ? <button type="button" onClick={() => job && void photoCullCancel(job.jobId)}>Stop</button>
            : <button type="button" className="is-primary" disabled={locked || !inTauri()} onClick={() => void cull()}>{review ? 'Cull again' : 'Cull all photos'}</button>}
        </div>
      </header>
      <p className="studio-footnote">Every photo is checked against 100 measurements - exposure, focus, motion, noise, colour, composition, faces, eyes, light and how it compares with similar shots. The best of each moment is kept; nothing is ever deleted, and anything left out can be brought back.</p>
      {running && job && (
        <div className="photo-cull-progress" role="status">
          <span>{job.label}</span>
          {job.total > 0 && <progress max={job.total} value={job.done} aria-label="Cull progress" />}
          {job.total > 0 && <span>{job.done} of {job.total}</span>}
        </div>
      )}
      {review === undefined && <p className="studio-footnote">Reading the last cull…</p>}
      {review === null && !running && <div className="studio-empty"><strong>This collection has not been culled yet.</strong><p>Press “Cull all photos”. Nothing is edited or deleted; you review the result here.</p></div>}
      {review && (
        <>
          <p className="photo-cull-summary">{cullSummary(review)}</p>
          {review.levels.length > 0 ? (
            <div className="photo-cull-levels" role="radiogroup" aria-label="Strictness">
              {review.levels.map((level) => (
                <button key={level.strictness} type="button" role="radio" aria-checked={review.strictness === level.strictness}
                  disabled={locked} title={level.description} onClick={() => { if (review.strictness !== level.strictness) void restrict(level.strictness); }}>
                  <strong>{level.label}</strong>
                  <span>{level.best} best · {level.leftOut} left out</span>
                </button>
              ))}
            </div>
          ) : <p className="studio-footnote" role="note">Cull again once to choose how strict the cull is.</p>}
          {review.levels.length > 0 && <p className="studio-footnote">{review.levels.find((l) => l.strictness === review.strictness)?.description} Changing it measures nothing again and keeps every choice you made.</p>}
          {review.notCulled > 0 && <p className="studio-footnote" role="note">{review.notCulled} photo(s) were added after this cull and are not in it. Cull again to include them.</p>}
          <div className="photo-cull-tabs" role="tablist" aria-label="Cull result">
            <button type="button" role="tab" aria-selected={tab === 'best'} onClick={() => { setTab('best'); setSelected(new Set()); setReason(null); }}>Best ({review.best})</button>
            <button type="button" role="tab" aria-selected={tab === 'left'} onClick={() => { setTab('left'); setSelected(new Set()); setReason(null); }}>Left out ({review.leftOut})</button>
          </div>
          {reasons.length > 1 && (
            <div className="photo-cull-reasons" role="group" aria-label="Show only">
              <button type="button" aria-pressed={reason === null} onClick={() => setReason(null)}>All</button>
              {reasons.map((entry) => (
                <button key={entry.reason} type="button" aria-pressed={reason === entry.reason}
                  onClick={() => setReason(reason === entry.reason ? null : entry.reason)}>{entry.label} ({entry.count})</button>
              ))}
            </div>
          )}
          <div className="photo-cull-toolbar">
            <label>Order <select value={sort} onChange={(e) => setSort(e.currentTarget.value as CullSort)}><option value="score">Best score first</option><option value="time">Shooting order</option></select></label>
            {tab === 'best' && <label><input type="checkbox" checked={topOnly} onChange={(e) => setTopOnly(e.currentTarget.checked)} /> Top picks only</label>}
            {selected.size > 0 && (tab === 'left'
              ? <button type="button" disabled={locked} onClick={() => void choose([...selected], 'keep')}>Bring back {selected.size} selected</button>
              : <button type="button" disabled={locked} onClick={() => void choose([...selected], 'leave_out')}>Leave out {selected.size} selected</button>)}
            {tab === 'left' && leftOutIds.length > 0 && <button type="button" className="is-primary" disabled={locked} onClick={() => void choose(leftOutIds, 'keep')}>Keep every photo (remove none)</button>}
            {chosenIds.length > 0 && <button type="button" disabled={locked} onClick={() => void choose(chosenIds, 'auto')}>Undo all my choices ({chosenIds.length})</button>}
          </div>
          <div className="photo-cull-body">
            <div className="cull-grid" aria-label={tab === 'best' ? 'Best photos' : 'Left-out photos'}>
              {shown.length === 0 && <p className="studio-footnote">{tab === 'best' ? 'No photos are in the best set.' : 'Nothing was left out: every photo is delivered.'}</p>}
              {shown.map((photo) => (
                <Card key={photo.photoId} projectId={projectId} photo={photo} selected={selected.has(photo.photoId)} disabled={locked}
                  onToggle={() => toggle(photo.photoId)} onOpen={() => setDetail(photo.photoId)}
                  onChoose={(choice) => void choose([photo.photoId], choice)} />
              ))}
            </div>
            {detailPhoto && (
              <Detail projectId={projectId} photo={detailPhoto} review={review} disabled={locked} onClose={() => setDetail(null)} onOpen={onOpen}
                onSelect={setDetail} onChoose={(choice) => void choose([detailPhoto.photoId], choice)} onSwap={(deliver, leaveOut) => void swap(deliver, leaveOut)} />
            )}
          </div>
          <footer className="photo-cull-finish">
            <label>Export into <input type="text" value={destination} placeholder="Pictures\AURA Exports (default)" disabled={locked} onChange={(e) => setDestination(e.currentTarget.value)} aria-label="Export into" /></label>
            <button type="button" disabled={locked || !inTauri()} onClick={() => void pickImportPaths(true).then((paths) => { if (paths[0]) setDestination(paths[0]); }).catch((err: unknown) => onError(asIpcError(err)))}>Browse…</button>
            <button type="button" className="is-primary" disabled={locked || review.best === 0 || !inTauri()} onClick={() => void finish([])}>Edit &amp; export the {review.best} best photos</button>
            {broughtBack.length > 0 && <button type="button" disabled={locked || !inTauri()} onClick={() => void finish(broughtBack)}>Edit &amp; export only the {broughtBack.length} brought back</button>}
          </footer>
        </>
      )}
    </div>
  );
}
