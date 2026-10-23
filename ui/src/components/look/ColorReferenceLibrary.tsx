import { useEffect, useState } from 'react';
import type { ColorMatchControls, ColorReference, ReferenceMetadata } from '../../ipc/colorMatch';

export function ColorReferenceLibrary({ references, selected, disabled, controls, onSelect, onManage }: {
  references: ColorReference[]; selected: string; disabled: boolean; controls: ColorMatchControls;
  onSelect: (reference: ColorReference) => void;
  onManage: (reference: ColorReference, metadata: ReferenceMetadata, remove?: boolean) => void;
}): JSX.Element {
  const [query, setQuery] = useState('');
  const [group, setGroup] = useState('');
  const [favorites, setFavorites] = useState(false);
  const [name, setName] = useState('');
  const [category, setCategory] = useState('');
  const current = references.find(item => item.id === selected);
  useEffect(() => { setName(current?.name ?? ''); setCategory(current?.group || 'Ungrouped'); }, [current?.id, current?.name, current?.group]);
  const shown = references.filter(item => (!favorites || item.favorite) &&
    (!group || (item.group || 'Ungrouped') === group) &&
    `${item.name} ${item.group ?? ''}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const groups = [...new Set(references.map(item => item.group || 'Ungrouped'))].sort();
  return <details className="color-reference-library"><summary>Reference library ({references.length})</summary>
    <label>Find reference<input type="search" value={query} onChange={event => setQuery(event.target.value)}/></label>
    <label>Reference group<select aria-label="Reference group" value={group} onChange={event => setGroup(event.target.value)}>
      <option value="">All groups</option>{groups.map(value => <option key={value}>{value}</option>)}
    </select></label>
    <label><input type="checkbox" checked={favorites} onChange={event => setFavorites(event.target.checked)}/>Favorite references only</label>
    <div className="color-reference-grid">{shown.map(item => <button type="button" key={item.id} disabled={disabled}
      aria-pressed={item.id === selected} onClick={() => onSelect(item)}>
      {item.thumbnail && <img src={item.thumbnail} alt="" width="96" height="96" loading="lazy"/>}
      <span>{item.favorite ? '★ ' : ''}{item.name}</span><small>{item.group || 'Ungrouped'}</small>
    </button>)}</div>
    {!shown.length && <p>No references match these filters.</p>}
    {current && <fieldset disabled={disabled}><legend>Edit selected reference</legend>
      <label>Reference name<input maxLength={120} value={name} onChange={event => setName(event.target.value)}/></label>
      <label>Save in group<input maxLength={80} value={category} onChange={event => setCategory(event.target.value)}/></label>
      <button type="button" disabled={!name.trim() || !category.trim()} onClick={() => onManage(current, { name, group: category })}>Save reference details</button>
      <button type="button" onClick={() => onManage(current, { favorite: !current.favorite })}>{current.favorite ? 'Unfavorite reference' : 'Favorite reference'}</button>
      <button type="button" onClick={() => onManage(current, { defaultControls: controls })}>Remember matching strengths for this reference</button>
      <button type="button" onClick={() => onManage(current, {}, true)}>Remove reference from library</button>
      <p className="lr-hint">Removing a library entry keeps its original file and all saved photo edits.</p>
    </fieldset>}
  </details>;
}
