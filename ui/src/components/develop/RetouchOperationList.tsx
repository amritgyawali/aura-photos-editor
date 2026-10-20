import { useMemo, useState } from 'react';
import { MAX_NATIVE_RETOUCH_EDITS, RETOUCH_TOOLS, type NativeRetouchEdit } from '../../ipc/nativeRetouch';
import { SavedRetouchStrength } from './SavedRetouchStrength';

// Bound the mounted controls independently of the saved recipe size. Paging
// retains keyboard access and stable operation IDs without variable-height
// scroll virtualization (expanded strength controls change each row's height).
export const RETOUCH_PAGE_SIZE = 24;
type Action = 'earlier' | 'later' | 'duplicate' | 'remove';

export function RetouchOperationList({ edits, selected, disabled, automaticLabel, onSelect, onUpdate, onAction }: {
  edits: NativeRetouchEdit[];
  selected: string | null;
  disabled: boolean;
  automaticLabel: (id: string) => string;
  onSelect: (edit: NativeRetouchEdit) => void;
  onUpdate: (edit: NativeRetouchEdit) => void;
  onAction: (action: Action, id: string) => void;
}): JSX.Element {
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<'all' | 'enabled' | 'disabled'>('all');
  const [page, setPage] = useState(0);
  const rows = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return edits.map((edit, index) => ({
      edit, index, label: RETOUCH_TOOLS.find(tool => tool[0] === edit.tool)?.[1] ?? edit.tool,
      origin: automaticLabel(edit.id),
    })).filter(row => (filter === 'all' || row.edit.enabled === (filter === 'enabled')) &&
      (!needle || `${row.index + 1} ${row.label} ${row.origin} ${row.edit.id}`.toLocaleLowerCase().includes(needle)));
  }, [edits, query, filter, automaticLabel]);
  const pages = Math.max(1, Math.ceil(rows.length / RETOUCH_PAGE_SIZE));
  const current = Math.min(page, pages - 1);
  const start = current * RETOUCH_PAGE_SIZE;
  const selectedRow = rows.findIndex(row => row.edit.id === selected);
  return <details open className="retouch-operation-list">
    <summary>Saved retouch operations ({edits.length})</summary>
    {(edits.length > RETOUCH_PAGE_SIZE || query || filter !== 'all') && <div className="retouch-operation-filters">
      <label>Find operation<input type="search" value={query} onChange={event => { setQuery(event.target.value); setPage(0); }} placeholder="Tool, face, or step number" /></label>
      <label>Operation status<select value={filter} onChange={event => { setFilter(event.target.value as typeof filter); setPage(0); }}>
        <option value="all">All operations</option><option value="enabled">Enabled only</option><option value="disabled">Disabled only</option>
      </select></label>
      {selectedRow >= 0 && Math.floor(selectedRow / RETOUCH_PAGE_SIZE) !== current &&
        <button type="button" onClick={() => setPage(Math.floor(selectedRow / RETOUCH_PAGE_SIZE))}>Show selected operation</button>}
    </div>}
    {rows.length === 0 ? <p>{edits.length ? 'No operations match your search.' : 'No saved retouch operations yet.'}</p> : <>
      {edits.length > RETOUCH_PAGE_SIZE && <p role="status">Showing {start + 1}–{Math.min(start + RETOUCH_PAGE_SIZE, rows.length)} of {rows.length} operations</p>}
      <ol start={start + 1}>{rows.slice(start, start + RETOUCH_PAGE_SIZE).map(({ edit, index, label, origin }) => <li key={edit.id} value={index + 1}>
        <button type="button" disabled={disabled} aria-pressed={selected === edit.id} onClick={() => onSelect(edit)}>{index + 1}. {label} · {Math.round(edit.amount * 100)}%{origin}</button>
        <label><input type="checkbox" checked={edit.enabled} disabled={disabled} onChange={event => onUpdate({ ...edit, enabled: event.target.checked })} />Enabled</label>
        <details><summary>Adjust strength</summary>
          <SavedRetouchStrength index={index + 1} amount={edit.amount} disabled={disabled} onApply={amount => onUpdate({ ...edit, amount })} />
        </details>
        <div className="retouch-operation-actions">
          <button type="button" disabled={disabled || index === 0} aria-label={`Move retouch ${index + 1} earlier`} onClick={() => onAction('earlier', edit.id)}>↑</button>
          <button type="button" disabled={disabled || index === edits.length - 1} aria-label={`Move retouch ${index + 1} later`} onClick={() => onAction('later', edit.id)}>↓</button>
          <button type="button" disabled={disabled || edits.length >= MAX_NATIVE_RETOUCH_EDITS} aria-label={`Duplicate retouch ${index + 1}`} onClick={() => onAction('duplicate', edit.id)}>Duplicate</button>
        </div>
        <button type="button" disabled={disabled} aria-label={`Remove retouch ${index + 1}`} onClick={() => onAction('remove', edit.id)}>Remove</button>
      </li>)}</ol>
      {pages > 1 && <nav aria-label="Retouch operation pages">
        <button type="button" disabled={current === 0} onClick={() => setPage(current - 1)}>Previous operations</button>
        <span>Page {current + 1} of {pages}</span>
        <button type="button" disabled={current === pages - 1} onClick={() => setPage(current + 1)}>Next operations</button>
      </nav>}
    </>}
  </details>;
}
