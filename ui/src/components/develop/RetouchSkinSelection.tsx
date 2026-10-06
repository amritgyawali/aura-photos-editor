import type { NativeRetouchEdit } from '../../ipc/nativeRetouch';

/** Reuse the exact detected matte, including its feature exclusions and sample. */
export function RetouchSkinSelection({ draft, edits, onChange }: {
  draft: NativeRetouchEdit;
  edits: NativeRetouchEdit[];
  onChange: (patch: Partial<NativeRetouchEdit>) => void;
}) {
  const choices = edits.filter((edit, index) => edit.matte
    && /-(face|body|skin|surface)(-feature-safe)?$/.test(edit.matte)
    && edits.findIndex(other => other.matte === edit.matte) === index);
  return <details open><summary>Detected skin selection</summary>
    <label>Use detected skin<select disabled={choices.length === 0} value={choices.some(e => e.matte === draft.matte) ? draft.matte! : ''}
      onChange={event => {
        const edit = choices.find(e => e.matte === event.target.value);
        if (edit) onChange({ matte: edit.matte, mask: edit.mask, region: [...edit.region],
          source: edit.source ? [...edit.source] : null, skin: null, selection: null, feather: edit.feather });
      }}>
      <option value="" disabled>Choose face or body skin</option>
      {choices.map(edit => {
        const match = /-(\d+)-(face|body|skin|surface)(-feature-safe)?$/.exec(edit.matte!);
        return <option key={edit.matte} value={edit.matte!}>
          {match?.[2] === 'body' ? 'Body skin' : match?.[2] === 'surface' ? 'Blemish cleanup skin' : 'Face skin'} · Person {Number(match?.[1] ?? 0) + 1}{match?.[3] ? ' · protected details' : ''}
        </option>;
      })}
    </select></label>
    {draft.matte ? <>
      <p className="lr-hint">AI mask active. Brush, ellipse and brightness limits only refine this detected region. Preview selection mask to inspect coverage.</p>
      <button type="button" onClick={() => onChange({ matte: null })}>Remove AI mask restriction</button>
    </> : <p className="lr-hint">{choices.length ? 'Choose a detected region to keep this tool on skin.' : 'Run Automatic retouch with Face + body skin to detect reusable skin selections.'}</p>}
  </details>;
}
