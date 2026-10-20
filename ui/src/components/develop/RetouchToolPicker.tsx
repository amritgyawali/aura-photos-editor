import { useState } from 'react';
import { RETOUCH_TOOLS, type RetouchTool } from '../../ipc/nativeRetouch';

/** Search only tools that have an executable native path. */
export function RetouchToolPicker({ value, onChange }: { value: RetouchTool; onChange: (tool: RetouchTool) => void }): JSX.Element {
  const [query, setQuery] = useState('');
  const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const matches = RETOUCH_TOOLS.filter(tool => words.every(word => tool.join(' ').toLocaleLowerCase().includes(word)));
  const current = RETOUCH_TOOLS.find(tool => tool[0] === value);
  return <>
    <label>Find a retouch tool<input type="search" value={query} placeholder="Try fabric, shadow, hair, or glow"
      onChange={event => setQuery(event.target.value)} /></label>
    {query && <p role="status">{matches.length ? `${matches.length} matching tools` : 'No matching tools. Try a material or effect name.'}
      {' '}<button type="button" onClick={() => setQuery('')}>Clear tool search</button></p>}
    <label>Tool<select aria-label="Tool" value={value} onChange={event => onChange(event.target.value as RetouchTool)}>
      {current && !matches.some(tool => tool[0] === value) && <optgroup label="Current tool"><option value={value}>{current[1]}</option></optgroup>}
      {Array.from(new Set(matches.map(tool => tool[2]))).map(group => <optgroup label={group} key={group}>
        {matches.filter(tool => tool[2] === group).map(tool => <option value={tool[0]} key={tool[0]}>{tool[1]}</option>)}
      </optgroup>)}
    </select></label>
  </>;
}
