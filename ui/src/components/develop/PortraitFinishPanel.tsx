import { useEffect, useRef, useState } from 'react';
import {
  BODY_SLIDERS, COLOUR_FEATURES, FACE_SLIDERS, FINISH_PRESETS, NEUTRAL_BODY, NEUTRAL_FACE, NEUTRAL_FINISH,
  hexToRgb, rgbToHex, type Background, type BackgroundMode, type FeatureColours, type LiquifyMode, type StudioFinish,
} from '../../ipc/studioFinish';

/** What dragging on the photograph does while the Liquify tab has a brush chosen. */
export type LiquifyTool = { mode: LiquifyMode; size: number; strength: number } | null;

export type PortraitFinishPanelProps = {
  finish: StudioFinish;
  disabled: boolean;
  liquify: LiquifyTool;
  onSave: (finish: StudioFinish, label: string) => void;
  onLiquify: (tool: LiquifyTool) => void;
};

type Tab = 'face' | 'body' | 'liquify' | 'background' | 'makeup';
const TABS: ReadonlyArray<readonly [Tab, string]> = [
  ['face', 'Face'], ['body', 'Body'], ['liquify', 'Liquify'], ['background', 'Background'], ['makeup', 'Makeup & colour'],
];

/** One shape slider: commits when released, so a drag does not queue a render per step. */
function ShapeSlider({ label, value, min = -100, max = 100, disabled, onCommit }: {
  label: string; value: number; min?: number; max?: number; disabled: boolean; onCommit: (value: number) => void;
}): JSX.Element {
  const [draft, setDraft] = useState(value);
  const committed = useRef(value);
  useEffect(() => { setDraft(value); committed.current = value; }, [value]);
  const commit = (next: number) => {
    const clamped = Math.min(max, Math.max(min, Math.round(next)));
    if (clamped !== committed.current) { committed.current = clamped; onCommit(clamped); }
  };
  return <label className="lr-slider" title="Double-click to reset">
    <span className="lr-slider-label" onDoubleClick={() => { setDraft(0); commit(0); }}>{label}</span>
    <input type="range" min={min} max={max} step={1} value={draft} disabled={disabled} aria-label={label}
      onChange={event => setDraft(Number(event.target.value))}
      onPointerUp={() => commit(draft)} onKeyUp={() => commit(draft)} onBlur={() => commit(draft)} />
    <span className="lr-number" aria-hidden="true">{draft}</span>
  </label>;
}

const LIQUIFY_MODES: ReadonlyArray<readonly [LiquifyMode, string, string]> = [
  ['push', 'Push', 'Drag to move what is under the brush along your stroke.'],
  ['bloat', 'Enlarge', 'Drag over something to make it larger.'],
  ['pinch', 'Shrink', 'Drag over something to make it smaller.'],
  ['restore', 'Restore', 'Paint over a liquify to take it back toward the photograph as taken.'],
];

const BACKGROUND_MODES: ReadonlyArray<readonly [BackgroundMode, string]> = [
  ['colour', 'Solid colour'], ['gradient', 'Gradient'], ['blur', 'Blur'], ['sky', 'Replace sky'],
];

/**
 * Evoto-style finishing: face and body reshaping, liquify, background replacement and makeup,
 * each saved as one undoable step. Nothing here is applied automatically. ADR-0108.
 */
export function PortraitFinishPanel({ finish, disabled, liquify, onSave, onLiquify }: PortraitFinishPanelProps): JSX.Element {
  const [tab, setTab] = useState<Tab>('face');
  const strokes = finish.liquify ?? [];
  const background: Background = finish.background ?? { mode: 'colour', colour: [1, 1, 1], colour2: [0.85, 0.85, 0.85], amount: 0, feather: 30 };
  const setBackground = (next: Background | null, label: string) => onSave({ ...finish, background: next }, label);
  const setTint = (key: keyof FeatureColours, colour: [number, number, number], amount: number, label: string) =>
    onSave({ ...finish, colours: { ...finish.colours, [key]: amount > 0 ? { colour, amount } : null } }, label);

  return <div className="portrait-finish" aria-label="Portrait finishing">
    <p className="lr-hint">Reshape, background and makeup tools in the style of Evoto. Every change is one step in your history, and none of them is ever applied automatically.</p>
    <div className="lr-crops" aria-label="Finishing presets">{FINISH_PRESETS.map(preset =>
      <button type="button" key={preset.id} title={preset.hint} disabled={disabled} onClick={() => onSave(preset.apply(finish), preset.name)}>{preset.name}</button>)}
    </div>
    <div className="lr-tabs" role="tablist">{TABS.map(([key, label]) =>
      <button type="button" role="tab" key={key} aria-selected={tab === key} onClick={() => { setTab(key); if (key !== 'liquify' && liquify) onLiquify(null); }}>{label}</button>)}
    </div>

    {tab === 'face' && <div role="tabpanel" aria-label="Face shape">
      <p className="lr-hint">Applies to every face AURA finds. Draw a missed face in Portrait retouch first.</p>
      {FACE_SLIDERS.map(([key, label]) => <ShapeSlider key={key} label={label} value={finish.face[key]} disabled={disabled}
        onCommit={value => onSave({ ...finish, face: { ...finish.face, [key]: value } }, `Face: ${label}`)} />)}
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, face: NEUTRAL_FACE }, 'Face shape reset')}>Reset face</button>
    </div>}

    {tab === 'body' && <div role="tabpanel" aria-label="Body shape">
      <p className="lr-hint">Applies to the main person. The background bends a little with a strong change, as in any reshaping tool.</p>
      {BODY_SLIDERS.map(([key, label]) => <ShapeSlider key={key} label={label} value={finish.body[key]} disabled={disabled}
        onCommit={value => onSave({ ...finish, body: { ...finish.body, [key]: value } }, `Body: ${label}`)} />)}
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, body: NEUTRAL_BODY }, 'Body shape reset')}>Reset body</button>
    </div>}

    {tab === 'liquify' && <div role="tabpanel" aria-label="Liquify">
      <div className="lr-crops">{LIQUIFY_MODES.map(([mode, label, hint]) =>
        <button type="button" key={mode} title={hint} aria-pressed={liquify?.mode === mode} disabled={disabled}
          onClick={() => onLiquify(liquify?.mode === mode ? null : { mode, size: liquify?.size ?? 0.06, strength: liquify?.strength ?? 0.7 })}>{label}</button>)}
      </div>
      {liquify ? <>
        <p className="lr-hint">{LIQUIFY_MODES.find(([mode]) => mode === liquify.mode)?.[2]} Drag on the photograph.</p>
        <label className="lr-slider"><span className="lr-slider-label">Brush size</span>
          <input type="range" min={0.01} max={0.3} step={0.005} value={liquify.size} aria-label="Liquify brush size"
            onChange={event => onLiquify({ ...liquify, size: Number(event.target.value) })} /></label>
        <label className="lr-slider"><span className="lr-slider-label">Strength</span>
          <input type="range" min={0.05} max={1} step={0.05} value={liquify.strength} aria-label="Liquify strength"
            onChange={event => onLiquify({ ...liquify, strength: Number(event.target.value) })} /></label>
      </> : <p className="lr-hint">Choose a brush, then drag on the photograph.</p>}
      <p>{strokes.length} stroke{strokes.length === 1 ? '' : 's'}</p>
      <button type="button" disabled={disabled || strokes.length === 0} onClick={() => onSave({ ...finish, liquify: strokes.slice(0, -1) }, 'Liquify: undo stroke')}>Remove last stroke</button>
      <button type="button" disabled={disabled || strokes.length === 0} onClick={() => onSave({ ...finish, liquify: [] }, 'Liquify reset')}>Clear liquify</button>
    </div>}

    {tab === 'background' && <div role="tabpanel" aria-label="Background">
      <p className="lr-hint">The people are kept and everything behind them is replaced. Replace sky changes only open sky.</p>
      <div className="lr-crops">{BACKGROUND_MODES.map(([mode, label]) =>
        <button type="button" key={mode} aria-pressed={finish.background?.mode === mode} disabled={disabled}
          onClick={() => setBackground({ ...background, mode, amount: background.amount > 0 ? background.amount : 100 }, `Background: ${label}`)}>{label}</button>)}
      </div>
      {finish.background && <>
        {finish.background.mode !== 'blur' && <label className="lr-toggle">{finish.background.mode === 'colour' ? 'Colour' : 'Top colour'}
          <input type="color" aria-label="Background colour" value={rgbToHex(background.colour)} disabled={disabled}
            onChange={event => setBackground({ ...background, colour: hexToRgb(event.target.value) }, 'Background colour')} /></label>}
        {(finish.background.mode === 'gradient' || finish.background.mode === 'sky') && <label className="lr-toggle">{finish.background.mode === 'sky' ? 'Horizon colour' : 'Bottom colour'}
          <input type="color" aria-label="Second background colour" value={rgbToHex(background.colour2)} disabled={disabled}
            onChange={event => setBackground({ ...background, colour2: hexToRgb(event.target.value) }, 'Background colour')} /></label>}
        <ShapeSlider label={finish.background.mode === 'blur' ? 'Blur amount' : 'Amount'} min={0} max={100} value={background.amount} disabled={disabled}
          onCommit={amount => setBackground({ ...background, amount }, 'Background amount')} />
        <ShapeSlider label="Edge softness" min={0} max={100} value={background.feather} disabled={disabled}
          onCommit={feather => setBackground({ ...background, feather }, 'Background edge')} />
        <button type="button" disabled={disabled} onClick={() => setBackground(null, 'Background restored')}>Keep original background</button>
      </>}
    </div>}

    {tab === 'makeup' && <div role="tabpanel" aria-label="Makeup and colour">
      <p className="lr-hint">Each colour keeps the light and shade underneath it. Skin tone is never changed here.</p>
      {COLOUR_FEATURES.map(([key, label, fallback]) => {
        const tint = finish.colours[key];
        const colour = tint?.colour ?? hexToRgb(fallback);
        return <div key={key} className="portrait-finish-colour">
          <label className="lr-toggle">{label}
            <input type="color" aria-label={`${label} colour`} value={rgbToHex(colour)} disabled={disabled}
              onChange={event => setTint(key, hexToRgb(event.target.value), tint?.amount ?? 50, label)} /></label>
          <ShapeSlider label={`${label} amount`} min={0} max={100} value={tint?.amount ?? 0} disabled={disabled}
            onCommit={amount => setTint(key, colour, amount, label)} />
        </div>;
      })}
    </div>}

    <button type="button" className="portrait-finish-reset" disabled={disabled} onClick={() => { onLiquify(null); onSave(NEUTRAL_FINISH, 'Portrait finishing reset'); }}>Reset all finishing</button>
  </div>;
}
