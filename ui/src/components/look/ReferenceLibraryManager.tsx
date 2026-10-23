import { useEffect, useRef, useState } from 'react';
import { asIpcError, inTauri } from '../../ipc/client';
import { DEFAULT_MATCH_CONTROLS, type ColorMatchControls, type ColorReference } from '../../ipc/colorMatch';
import { referenceLibraryWorkflow, type LibraryRequest, type LibrarySnapshot } from '../../ipc/referenceLibraryWorkflows';

const groupLabel = (group: string): string => group || 'Ungrouped';
const MAX_DOCUMENT = 1024 * 1024;

function moved<T>(items: T[], index: number, direction: -1 | 1): T[] {
  const result = [...items];
  const item = result[index];
  const other = result[index + direction];
  if (item !== undefined && other !== undefined) { result[index] = other; result[index + direction] = item; }
  return result;
}

/** Durable groups and atomic multi-reference workflows; no target recipe is changed. */
export function ReferenceLibraryManager({ disabled, controls = DEFAULT_MATCH_CONTROLS, onUpdated }: {
  disabled: boolean; controls?: ColorMatchControls; onUpdated: (references: ColorReference[]) => void;
}): JSX.Element {
  const [snapshot, setSnapshot] = useState<LibrarySnapshot | null>(null);
  const [group, setGroup] = useState('');
  const [name, setName] = useState('');
  const [destination, setDestination] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const [document, setDocument] = useState('');
  const [replaceExisting, setReplaceExisting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);
  const running = useRef(false);
  const updateCallback = useRef(onUpdated); updateCallback.current = onUpdated;

  const accept = (value: LibrarySnapshot) => {
    setSnapshot(value);
    setGroup(current => value.groups.includes(current) ? current : '');
    setDestination(current => value.groups.includes(current) ? current : '');
    setSelected(current => current.filter(id => value.references.some(reference => reference.id === id)));
    updateCallback.current(value.references);
    if (value.document !== null) setDocument(value.document);
  };
  useEffect(() => {
    alive.current = true;
    if (inTauri()) {
      void referenceLibraryWorkflow({ action: 'read' }).then(value => { if (alive.current) accept(value); })
        .catch(cause => { if (alive.current) setError(asIpcError(cause).message); });
    }
    return () => { alive.current = false; };
  }, []);

  const run = async (request: LibraryRequest) => {
    if (running.current || disabled || !inTauri()) return;
    running.current = true; setBusy(true); setError(null); setMessage(null);
    try {
      const value = await referenceLibraryWorkflow({ expectedRevision: snapshot?.revision, ...request });
      if (!alive.current) return;
      accept(value);
      setMessage(request.action === 'export' ? 'Descriptor library ready. It contains colour measurements and metadata; original photos and thumbnails are excluded.'
        : request.action === 'read' ? 'Reference library refreshed.'
          : value.changed ? 'Reference library saved. Photo edits and matching sessions are unchanged.' : 'The library already has these settings.');
    } catch (cause) { if (alive.current) setError(asIpcError(cause).message); }
    finally { running.current = false; if (alive.current) setBusy(false); }
  };
  const locked = disabled || busy || !inTauri();
  const groups = snapshot?.groups ?? [''];
  const members = snapshot?.references.filter(reference => (reference.group || '') === group) ?? [];
  const batchLocked = locked || !selected.length || !snapshot;
  const namedGroups = groups.filter(value => value !== '');
  const groupIndex = namedGroups.indexOf(group);
  const fileInput = async (file: File | undefined) => {
    if (!file) return;
    if (file.size > MAX_DOCUMENT) { setError('Portable reference libraries must be at most 1 MB.'); return; }
    try {
      const text = await file.text();
      if (alive.current) { setDocument(text); setMessage('Library document loaded. Review the import option, then import it.'); }
    } catch { if (alive.current) setError('The library file could not be read.'); }
  };
  const download = () => {
    if (!document) return;
    const url = URL.createObjectURL(new Blob([document], { type: 'application/json' }));
    const link = window.document.createElement('a'); link.href = url; link.download = 'aura-reference-library.json'; link.click();
    URL.revokeObjectURL(url);
  };

  return <details className="reference-library-manager"><summary>Organize groups and selected references</summary>
    <p className="lr-hint">Groups can be empty. Multi-reference changes are saved together. Removing a group moves its references to Ungrouped.</p>
    <button type="button" disabled={locked} onClick={() => void run({ action: 'read' })}>Refresh organized library</button>
    <fieldset disabled={locked || !snapshot}><legend>Reference groups</legend>
      <label>Manage reference group<select aria-label="Manage reference group" value={group} onChange={event => setGroup(event.target.value)}>
        {groups.map(value => <option key={value} value={value}>{groupLabel(value)}</option>)}
      </select></label>
      <label>New group name<input maxLength={80} value={name} onChange={event => setName(event.target.value)}/></label>
      <button type="button" disabled={!name.trim()} onClick={() => void run({ action: 'create_group', group: name })}>Create empty reference group</button>
      <button type="button" disabled={!group || !name.trim()} onClick={() => void run({ action: 'rename_group', group, name })}>Rename reference group</button>
      <button type="button" disabled={!group} onClick={() => void run({ action: 'delete_group', group })}>Delete group and keep references</button>
      <button type="button" disabled={groupIndex <= 0} onClick={() => void run({ action: 'reorder_groups', order: moved(namedGroups, groupIndex, -1) })}>Move group up</button>
      <button type="button" disabled={groupIndex < 0 || groupIndex >= namedGroups.length - 1} onClick={() => void run({ action: 'reorder_groups', order: moved(namedGroups, groupIndex, 1) })}>Move group down</button>
    </fieldset>
    <fieldset disabled={locked || !snapshot}><legend>{groupLabel(group)} references</legend>
      <button type="button" disabled={!members.length} onClick={() => setSelected(current => [...new Set([...current, ...members.map(reference => reference.id)])])}>Select references in this group</button>
      <button type="button" disabled={!selected.length} onClick={() => setSelected([])}>Clear reference selection</button>
      {!members.length && <p>This group is empty.</p>}
      <ol>{members.map((reference, index) => <li key={reference.id}>
        <label><input type="checkbox" aria-label={`Select reference ${reference.name}`} checked={selected.includes(reference.id)}
          onChange={event => setSelected(current => event.target.checked ? [...current, reference.id] : current.filter(id => id !== reference.id))}/>{reference.name}</label>
        <button type="button" aria-label={`Move reference ${reference.name} up`} disabled={index === 0}
          onClick={() => void run({ action: 'reorder_references', group, order: moved(members.map(item => item.id), index, -1) })}>Up</button>
        <button type="button" aria-label={`Move reference ${reference.name} down`} disabled={index === members.length - 1}
          onClick={() => void run({ action: 'reorder_references', group, order: moved(members.map(item => item.id), index, 1) })}>Down</button>
      </li>)}</ol>
    </fieldset>
    <fieldset disabled={batchLocked}><legend>Selected references ({selected.length})</legend>
      <label>Destination reference group<select aria-label="Destination reference group" value={destination} onChange={event => setDestination(event.target.value)}>
        {groups.map(value => <option key={value} value={value}>{groupLabel(value)}</option>)}
      </select></label>
      <button type="button" onClick={() => void run({ action: 'move', group: destination, referenceIds: selected })}>Move selected references</button>
      <button type="button" onClick={() => void run({ action: 'remove', referenceIds: selected })}>Remove selected references</button>
      <button type="button" onClick={() => void run({ action: 'favorite', favorite: true, referenceIds: selected })}>Favorite selected references</button>
      <button type="button" onClick={() => void run({ action: 'favorite', favorite: false, referenceIds: selected })}>Unfavorite selected references</button>
      <button type="button" onClick={() => void run({ action: 'defaults', controls, referenceIds: selected })}>Save current matching strengths for selected references</button>
      <button type="button" onClick={() => void run({ action: 'reset_defaults', referenceIds: selected })}>Reset selected matching defaults</button>
    </fieldset>
    <details><summary>Portable AURA reference library</summary>
      <p className="lr-hint">Shares measured colour descriptors and library metadata between compatible AURA catalogues. Does not include photo pixels, thumbnails, source paths or Evoto assets. Imported references can match colour without the original reference files.</p>
      <button type="button" disabled={locked || !snapshot} onClick={() => void run({ action: 'export', referenceIds: [] })}>Export entire descriptor library</button>
      <button type="button" disabled={batchLocked} onClick={() => void run({ action: 'export', referenceIds: selected })}>Export selected descriptors</button>
      <label>Load AURA library file<input type="file" accept=".json,application/json" disabled={locked} onChange={event => void fileInput(event.target.files?.[0])}/></label>
      <label>Portable library document<textarea aria-label="Portable library document" value={document} maxLength={MAX_DOCUMENT} disabled={locked} onChange={event => setDocument(event.target.value)}/></label>
      <label><input type="checkbox" checked={replaceExisting} disabled={locked} onChange={event => setReplaceExisting(event.target.checked)}/>Update metadata for identical existing descriptors</label>
      <p className="lr-hint">By default existing reference metadata is kept. Conflicting appearance data for the same identifier is always refused. Import merges libraries; it does not delete existing references.</p>
      <button type="button" disabled={locked || !snapshot || !document.trim()} onClick={() => void run({ action: 'import', document, replaceExisting })}>Import descriptor library</button>
      <button type="button" disabled={locked || !document} onClick={download}>Download descriptor JSON</button>
    </details>
    {message && <p role="status">{message}</p>}
    {error && <p role="alert">{error}</p>}
  </details>;
}
