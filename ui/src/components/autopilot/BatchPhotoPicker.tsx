import { useEffect, useState } from 'react';
import { api, asIpcError } from '../../ipc/client';

type Photo = { id: string; fileName: string };

export function BatchPhotoPicker({ projectId, disabled, selected, onSelect }: {
  projectId: string; disabled: boolean; selected: readonly string[] | null;
  onSelect: (ids: string[] | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const [photos, setPhotos] = useState<Photo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const [search, setSearch] = useState('');
  const [shown, setShown] = useState(120);
  useEffect(() => {
    if (!open) return;
    let active = true;
    setLoading(true); setError(null);
    void (async () => {
      const rows: Photo[] = [];
      for (let offset = 0; active; offset += 240) {
        const page = await api.listImages({ projectId, offset, limit: 240, orderBy: 'timeline' });
        rows.push(...page);
        if (page.length < 240) break;
      }
      if (active) setPhotos([...new Map(rows.map(photo => [photo.id, photo])).values()]);
    })().catch(cause => { if (active) setError(asIpcError(cause).message); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [open, projectId, retry]);
  const chosen = new Set(selected ?? photos.map(photo => photo.id));
  const visible = photos.filter(photo => photo.fileName.toLowerCase().includes(search.toLowerCase()));
  return <details className="batch-picker" onToggle={event => setOpen(event.currentTarget.open)}>
    <summary>Choose photos · {selected === null ? 'All photos' : `${selected.length} selected`}</summary>
    <p>Each photo gets its own measured adjustments and keeps its saved retouch preferences. Manual edits stay protected.</p>
    {loading && <p role="status">Loading photos…</p>}
    {error && <p role="alert">{error} <button type="button" disabled={disabled} onClick={() => setRetry(value => value + 1)}>Retry photo list</button></p>}
    <fieldset disabled={disabled || loading || Boolean(error)}>
      <legend>Photos to edit independently</legend>
      <label>Find photos <input type="search" value={search} onChange={event => { setSearch(event.target.value); setShown(120); }} /></label>
      <button type="button" onClick={() => onSelect(null)}>Select all photos</button>
      <button type="button" onClick={() => onSelect([])}>Clear selection</button>
      <button type="button" disabled={!visible.length} onClick={() => onSelect([...new Set([...chosen, ...visible.map(photo => photo.id)])])}>Select matching photos</button>
      <div className="batch-photo-list">
        {visible.slice(0, shown).map(photo => <label key={photo.id}><input type="checkbox" checked={chosen.has(photo.id)} onChange={event => {
          const next = new Set(chosen);
          if (event.target.checked) next.add(photo.id); else next.delete(photo.id);
          onSelect([...next]);
        }} />{photo.fileName}</label>)}
      </div>
      {visible.length > shown && <button type="button" onClick={() => setShown(value => value + 120)}>Show more photos ({shown} of {visible.length})</button>}
      {!loading && !visible.length && <p>No matching photos.</p>}
    </fieldset>
  </details>;
}
