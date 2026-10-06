import { useState } from 'react';
import { freshRetouch, RETOUCH_TOOLS, type NativeRetouchEdit } from '../../ipc/nativeRetouch';

const KEY = 'aura.retouch.presets.v1';
const FIELDS = ['tool', 'amount', 'feather', 'radius', 'texture', 'tone', 'warmth', 'tint', 'skin', 'preserveMicrotexture', 'textureHeal', 'sensitivity', 'keepDarkMarks'] as const;
type Settings = Pick<NativeRetouchEdit, typeof FIELDS[number]>;
type Preset = { name: string; settings: Settings };

export function parseRetouchPresets(value: string | null): Preset[] {
  if (!value) return [];
  const parsed = JSON.parse(value);
  if (parsed.version !== 1 || !Array.isArray(parsed.items) || parsed.items.length > 24) throw new Error('Saved retouch presets have an unsupported format.');
  const names = new Set<string>();
  return parsed.items.map((item: Preset) => {
    const s = item.settings;
    if (typeof item.name !== 'string' || !item.name.trim() || item.name.length > 40 || names.has(item.name.toLowerCase())
      || !s || !RETOUCH_TOOLS.some(tool => tool[0] === s.tool)
      || ![s.amount, s.feather, s.radius, s.texture, s.tone, s.warmth, s.tint].every(Number.isFinite)
      || s.amount < 0 || s.amount > 1 || s.feather < 0 || s.feather > 1 || s.radius < .0005 || s.radius > .05
      || s.texture < 0 || s.texture > 2 || s.tone < 0 || s.tone > 1 || Math.abs(s.warmth) > 1 || Math.abs(s.tint) > 1
      || (s.textureHeal !== undefined && typeof s.textureHeal !== 'boolean')
      || (s.preserveMicrotexture !== undefined && typeof s.preserveMicrotexture !== 'boolean')
      || (s.keepDarkMarks !== undefined && typeof s.keepDarkMarks !== 'boolean')
      || (s.sensitivity != null && (!Number.isFinite(s.sensitivity) || s.sensitivity < 0 || s.sensitivity > 1))
      || (s.skin && (!Number.isFinite(s.skin.tolerance) || s.skin.tolerance < .015 || s.skin.tolerance > .3
        || !Number.isFinite(s.skin.edgeProtection) || s.skin.edgeProtection < 0 || s.skin.edgeProtection > 1))) {
      throw new Error('Saved retouch presets contain invalid settings.');
    }
    names.add(item.name.toLowerCase());
    return { name: item.name, settings: settingsOnly(s) };
  });
}

function settingsOnly(draft: Settings): Settings {
  const result = Object.fromEntries(FIELDS.map(key => [key, draft[key]])) as Settings;
  result.textureHeal = draft.textureHeal ?? false;
  result.preserveMicrotexture = draft.preserveMicrotexture ?? false;
  result.keepDarkMarks = draft.keepDarkMarks ?? false;
  result.sensitivity = draft.sensitivity ?? null;
  if (result.skin) result.skin = { tolerance: result.skin.tolerance, edgeProtection: result.skin.edgeProtection };
  return result;
}

export function RetouchPresets({ draft, disabled, onApply }: {
  draft: NativeRetouchEdit; disabled: boolean; onApply: (settings: Settings) => void;
}) {
  const [initial] = useState(() => {
    try { return { items: parseRetouchPresets(localStorage.getItem(KEY)), error: '' }; }
    catch (error) { return { items: [] as Preset[], error: error instanceof Error ? error.message : 'Cannot read local presets.' }; }
  });
  const [items, setItems] = useState(initial.items);
  const [error, setError] = useState(initial.error);
  const [name, setName] = useState('');
  const [selected, setSelected] = useState('');
  const persist = (next: Preset[]) => {
    try {
      const encoded = JSON.stringify({ version: 1, items: next });
      parseRetouchPresets(encoded);
      localStorage.setItem(KEY, encoded);
      setItems(next); setError(''); return true;
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Cannot save local presets.'); return false; }
  };
  return <details className="retouch-presets"><summary>My tool presets</summary>
    <p className="lr-hint">Save settings for another photo. Selections and source points stay with their photo.</p>
    {error && <p role="alert">{error}</p>}
    <label>Preset name<input maxLength={40} value={name} disabled={disabled} onChange={event => setName(event.target.value)}/></label>
    <button type="button" disabled={disabled || !name.trim() || items.length >= 24} onClick={() => {
      if (items.some(p => p.name.toLowerCase() === name.trim().toLowerCase())) { setError('Choose a different preset name.'); return; }
      if (persist([...items, { name: name.trim(), settings: settingsOnly(draft) }])) { setSelected(name.trim()); setName(''); }
    }}>Save tool preset</button>
    <label>Saved preset<select aria-label="Saved preset" value={selected} disabled={disabled} onChange={event => setSelected(event.target.value)}>
      <option value="">Choose a preset</option>{items.map(item => <option key={item.name} value={item.name}>{item.name}</option>)}
    </select></label>
    <button type="button" disabled={disabled || !selected} onClick={() => {
      const item = items.find(p => p.name === selected); if (item) onApply(item.settings);
    }}>Load tool preset</button>
    <button type="button" disabled={disabled || !selected} onClick={() => {
      if (persist(items.filter(p => p.name !== selected))) setSelected('');
    }}>Delete preset</button>
    <button type="button" disabled={disabled} onClick={() => onApply(settingsOnly({ ...freshRetouch(), tool: draft.tool,
      warmth: draft.tool === 'makeup' ? .2 : 0, tint: draft.tool === 'makeup' ? .2 : 0 }))}>Reset tool settings</button>
  </details>;
}
