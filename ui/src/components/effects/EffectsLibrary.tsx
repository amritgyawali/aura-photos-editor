import { useDeferredValue, useMemo, useState } from 'react';
import { isSampledSkinTool, RETOUCH_TOOLS, type RetouchTool } from '../../ipc/nativeRetouch';
import { EFFECT_COLOR, EFFECT_PARAMS, EFFECT_SHORTCUTS, EFFECT_SOURCE, isEffectTool, isWarpTool } from '../../ipc/retouchEffects';
import { useEffectRequest, type EffectPatch } from '../../state/effectRequest';
import './effects-library.css';

type Entry = { id: RetouchTool; name: string; group: string; description: string };
const ENTRIES: Entry[] = RETOUCH_TOOLS.map(([id, name, group, description]) => ({ id, name, group, description }));
const BY_ID = new Map(ENTRIES.map(entry => [entry.id, entry]));
/** The groups in the order the tools first name them. */
export const LIBRARY_GROUPS = [...new Set(ENTRIES.map(entry => entry.group))];

/** What a tool asks of the photographer before it can run, said on its card. */
export function badgesOf(tool: RetouchTool): string[] {
  const badges: string[] = [];
  const source = isEffectTool(tool) ? EFFECT_SOURCE[tool] : undefined;
  if (['clone', 'color_match'].includes(tool) || source?.required) badges.push('Pick a point');
  else if (source) badges.push('Optional point');
  if (isSampledSkinTool(tool)) badges.push('Sample skin');
  if (isWarpTool(tool)) badges.push('Moves pixels');
  if (isEffectTool(tool) && EFFECT_COLOR[tool]) badges.push('Choose a colour');
  if (isEffectTool(tool)) {
    const sliders = EFFECT_PARAMS[tool].length + 1;
    badges.push(`${sliders} slider${sliders === 1 ? '' : 's'}`);
  }
  return badges;
}

const matches = (entry: Entry, words: string[]) => {
  const text = `${entry.name} ${entry.group} ${entry.description}`.toLowerCase();
  return words.every(word => text.includes(word));
};

type View = 'all' | 'favourites' | 'recent' | string;

/**
 * Every retouch tool and effect in one place, searchable and grouped, each opening Retouch on the
 * photograph in front of you with that tool chosen. ADR-0135. Choosing here changes nothing: the
 * tool still needs its selection, and nothing is applied until Apply.
 */
export function EffectsLibrary({ photoName, disabled = false, onOpen }: {
  /** The photograph a tool would open on; without one, the library is for browsing. */
  photoName: string | null; disabled?: boolean;
  /** Called after a tool has been asked for, to show the photograph in Retouch. */
  onOpen: () => void;
}) {
  const [query, setQuery] = useState('');
  const deferred = useDeferredValue(query);
  const [view, setView] = useState<View>('all');
  const ask = useEffectRequest(state => state.ask);
  const recent = useEffectRequest(state => state.recent);
  const favourites = useEffectRequest(state => state.favourites);
  const toggleFavourite = useEffectRequest(state => state.toggleFavourite);
  const canUse = Boolean(photoName) && !disabled;

  const shown = useMemo(() => {
    const words = deferred.toLowerCase().split(/\s+/).filter(Boolean);
    const pool = view === 'favourites' ? favourites.map(id => BY_ID.get(id)).filter((e): e is Entry => Boolean(e))
      : view === 'recent' ? recent.map(id => BY_ID.get(id)).filter((e): e is Entry => Boolean(e))
        : view === 'all' ? ENTRIES : ENTRIES.filter(entry => entry.group === view);
    return pool.filter(entry => matches(entry, words));
  }, [deferred, view, favourites, recent]);
  // Favourites and recent keep their own order; everything else is shown in its groups.
  const grouped = view === 'favourites' || view === 'recent' ? [['', shown] as const]
    : LIBRARY_GROUPS.map(group => [group, shown.filter(entry => entry.group === group)] as const).filter(([, list]) => list.length > 0);

  const use = (tool: RetouchTool, patch?: EffectPatch) => {
    if (!canUse) return;
    ask(tool, patch);
    onOpen();
  };

  return <section className="effects-library" aria-label="Effects library">
    <header className="workspace-heading"><div><span className="eyebrow">{ENTRIES.length} TOOLS · ONE SELECTION EACH</span><h1>Effects & retouch tools.</h1>
      <p className="effects-lede">Find a tool by what it does, then open it on {photoName ? <strong>{photoName}</strong> : 'a photograph'}. Each one works inside the area you select, is saved with the photo, and can be undone.</p></div></header>
    {!photoName && <p className="effects-notice" role="status">Open a collection and choose a photo in Auto edit to use a tool. You can browse everything here meanwhile.</p>}

    <div className="effects-search">
      <label><span>Search tools</span><input type="search" value={query} placeholder="Try “glow”, “eyes”, “sky” or “film”" onChange={event => setQuery(event.target.value)} /></label>
      <p aria-live="polite">{shown.length === ENTRIES.length ? `All ${ENTRIES.length} tools` : `${shown.length} of ${ENTRIES.length} tools`}</p>
    </div>
    <nav className="effects-chips" aria-label="Tool groups">
      {(['all', 'favourites', 'recent', ...LIBRARY_GROUPS] as View[]).map(id => {
        const label = id === 'all' ? 'All' : id === 'favourites' ? `Favourites (${favourites.length})` : id === 'recent' ? 'Recently used' : id;
        return <button key={id} type="button" aria-pressed={view === id} onClick={() => setView(id)}>{label}</button>;
      })}
    </nav>

    {view === 'all' && !deferred.trim() && <section className="effects-section" aria-label="Quick jobs">
      <h2>Quick jobs</h2>
      <div className="effects-shortcuts">
        {EFFECT_SHORTCUTS.map(shortcut => <button key={shortcut.label} type="button" disabled={!canUse} onClick={() => use(shortcut.tool, shortcut.patch)}>
          <strong>{shortcut.label}</strong><span>{BY_ID.get(shortcut.tool)?.name}</span></button>)}
      </div>
    </section>}

    {shown.length === 0 && <p className="effects-empty">{view === 'favourites' ? 'No favourites yet - star a tool to keep it here.'
      : view === 'recent' ? 'Tools you open from here will be listed here.' : 'No tool matches that search.'}</p>}

    {grouped.map(([group, list]) => <section key={group || view} className="effects-section" aria-label={group || (view === 'favourites' ? 'Favourites' : 'Recently used')}>
      {group && <h2>{group} <span>{list.length}</span></h2>}
      <ul className="effects-grid">
        {list.map(entry => {
          const favourite = favourites.includes(entry.id);
          return <li key={entry.id} className="effect-card">
            <div className="effect-card-head"><h3>{entry.name}</h3>
              <button type="button" className="effect-star" aria-pressed={favourite} aria-label={`${favourite ? 'Remove' : 'Add'} ${entry.name} ${favourite ? 'from' : 'to'} favourites`}
                onClick={() => toggleFavourite(entry.id)}>{favourite ? '★' : '☆'}</button></div>
            <p>{entry.description}</p>
            <ul className="effect-badges" aria-label="What it needs">{badgesOf(entry.id).map(badge => <li key={badge}>{badge}</li>)}</ul>
            <button type="button" disabled={!canUse} onClick={() => use(entry.id)}>Use on this photo</button>
          </li>;
        })}
      </ul>
    </section>)}
  </section>;
}
