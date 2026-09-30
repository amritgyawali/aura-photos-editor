import { useState } from 'react';
import { useStore } from '../../state/store';

export const SYNC_GROUPS = [
  ['tone', 'Exposure and tone'], ['white_balance', 'White balance'], ['curves', 'Tone curves'],
  ['color', 'Color and monochrome'], ['detail', 'Sharpening and noise'], ['effects', 'Grain and vignette'],
  ['calibration', 'Calibration'], ['lens', 'Lens corrections'], ['geometry', 'Crop and straighten'],
] as const;

export function SyncSettingsPanel({ projectId, photoId, disabled, onSync }: {
  projectId: string; photoId: string; disabled: boolean;
  onSync: (targets: string[], groups: string[]) => void;
}): JSX.Element {
  const selection = useStore(state => state.selection);
  const activeProject = useStore(state => state.activeProjectId);
  const [scope, setScope] = useState<'all' | 'selected'>('all');
  const [groups, setGroups] = useState<string[]>(SYNC_GROUPS.filter(([id]) => id !== 'geometry').map(([id]) => id));
  const targets = activeProject === projectId ? [...selection].filter(id => id !== photoId) : [];
  return <details className="lr-section"><summary>Synchronize settings</summary><div className="lr-section-body">
    <p className="lr-hint">Choose what to copy. Changed settings replace existing values and become protected manual edits. Masks, retouching and lens profiles stay with their own photo.</p>
    <label>Copy to<select value={scope} disabled={disabled} onChange={event => setScope(event.target.value as 'all' | 'selected')}>
      <option value="all">All other photos in this collection</option>
      <option value="selected">Selected library photos ({targets.length})</option>
    </select></label>
    {SYNC_GROUPS.map(([id, label]) => <label className="lr-toggle" key={id}><input type="checkbox" disabled={disabled}
      checked={groups.includes(id)} onChange={event => setGroups(old => event.target.checked ? [...old, id] : old.filter(value => value !== id))} />{label}</label>)}
    <button type="button" disabled={disabled || groups.length === 0 || (scope === 'selected' && targets.length === 0)}
      onClick={() => { if (scope === 'selected' && targets.length === 0) return; onSync(scope === 'all' ? [] : targets, groups); }}>Apply selected settings</button>
  </div></details>;
}
