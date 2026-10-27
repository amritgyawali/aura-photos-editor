import { useEffect, useRef, useState } from 'react';
import {
  ATTIRE_SLIDERS, BODY_SLIDERS, COLOUR_FEATURES, FACE_SLIDERS, FINISH_PRESETS, HAIR_SLIDERS, LIGHT_SLIDERS, MAKEUP_SLIDERS, MORE_BODY_SLIDERS,
  MORE_FACE_SLIDERS, NEUTRAL_ATTIRE, NEUTRAL_BODY, NEUTRAL_FACE, NEUTRAL_FINISH, NEUTRAL_HAIR, NEUTRAL_LIGHT, NEUTRAL_MAKEUP, NEUTRAL_SCENE,
  NEUTRAL_SKIN, NEUTRAL_SUBJECT, SCENE_SLIDERS, SKIN_SLIDERS, SUBJECT_SLIDERS,
  hexToRgb, rgbToHex, type Background, type BackgroundMode, type FeatureColours, type LightSuggestion, type LiquifyMode, type Slider, type StudioFinish,
} from '../../ipc/studioFinish';
import type { EvotoFace } from '../../ipc/evotoWorkflow';

/** What dragging on the photograph does while the Liquify tab has a brush chosen. */
export type LiquifyTool = { mode: LiquifyMode; size: number; strength: number } | null;

export type PortraitFinishPanelProps = {
  finish: StudioFinish;
  disabled: boolean;
  liquify: LiquifyTool;
  onSave: (finish: StudioFinish, label: string) => void;
  onLiquify: (tool: LiquifyTool) => void;
  /** Auto light: measure how each face is lit and propose a light. Absent where it cannot run. */
  onSuggestLight?: () => Promise<LightSuggestion>;
  /** The photograph the panel shows; a measurement made for another one is discarded. */
  subject?: string;
  /** The face picker: the faces in this photograph. Absent where it cannot run. */
  onFindFaces?: () => Promise<EvotoFace[]>;
  initialTab?: Tab;
  /** What the last save could not do on this photograph, shown above the tools. */
  notes?: string[];
};

type Tab = 'face' | 'body' | 'skin' | 'light' | 'liquify' | 'background' | 'makeup' | 'hair' | 'people' | 'faces';
const TABS: ReadonlyArray<readonly [Tab, string]> = [
  ['face', 'Face'], ['body', 'Body'], ['skin', 'Skin'], ['light', 'Light'], ['liquify', 'Liquify'], ['background', 'Background'],
  ['makeup', 'Makeup & colour'], ['hair', 'Hair & attire'], ['people', 'People & scene'], ['faces', 'Faces'],
];

/** A slider that only shapes another one: the eyeliner wing draws nothing without a liner, a
 * light's direction moves nothing until there is a light. While its parent is at zero it is
 * shown disabled with the reason, so it never looks like a slider that does nothing. */
const NEEDS: Readonly<Record<string, readonly [string, string]>> = {
  eyelinerWing: ['eyeliner', 'Set Eyeliner first: the wing extends the liner.'],
  relightDirection: ['relight', 'Set Relight first: this is where that light comes from.'],
  direction: ['key', 'Set Key light first: this is where that light comes from.'],
  height: ['key', 'Set Key light first: this is how high that light is.'],
};

/** The sentence for a slider that is waiting on its parent, or undefined when it can act. Only
 * within the group that holds the parent: the body's Height has no light to wait for. */
function waitingFor(key: string, values: object): string | undefined {
  const need = NEEDS[key];
  return need && need[0] in values && (values as Record<string, number>)[need[0]] === 0 ? need[1] : undefined;
}

/** One ADR-0150 group of sliders, each committing the whole group when released. */
function SliderGroup<K extends string>({ title, list, values, neutral, disabled, onCommit }: {
  title?: string; list: ReadonlyArray<Slider<K>>; values: Record<K, number>; neutral?: Partial<Record<K, number>>; disabled: boolean;
  onCommit: (key: K, value: number, label: string) => void;
}): JSX.Element {
  return <div className="portrait-finish-group" aria-label={title}>
    {title && <h4>{title}</h4>}
    {list.map(([key, label, min, max, hint]) => <div key={key} title={hint}>
      <ShapeSlider label={label} min={min} max={max} neutral={neutral?.[key] ?? 0} value={values[key] ?? 0} disabled={disabled}
        waiting={waitingFor(key, values)} onCommit={value => onCommit(key, value, label)} />
    </div>)}
  </div>;
}

/** Where a key light is put by one click, seen from the camera. */
const LIGHT_POSITIONS: ReadonlyArray<readonly [string, number, number]> = [
  ['From the left', -55, 30], ['From the front', 0, 40], ['From the right', 55, 30], ['Rim from behind', -150, 20],
];

/** One shape slider: commits when released, so a drag does not queue a render per step. */
function ShapeSlider({ label, value, min = -100, max = 100, neutral = 0, disabled, waiting, onCommit }: {
  label: string; value: number; min?: number; max?: number; neutral?: number; disabled: boolean;
  /** Why this slider cannot act yet; it is disabled and says so. */
  waiting?: string;
  onCommit: (value: number) => void;
}): JSX.Element {
  const [draft, setDraft] = useState(value);
  const committed = useRef(value);
  useEffect(() => { setDraft(value); committed.current = value; }, [value]);
  const commit = (next: number) => {
    const clamped = Math.min(max, Math.max(min, Math.round(next)));
    if (clamped !== committed.current) { committed.current = clamped; onCommit(clamped); }
  };
  const off = disabled || waiting !== undefined;
  return <>
    <label className="lr-slider" title={waiting ?? 'Double-click to reset'}>
      <span className="lr-slider-label" onDoubleClick={() => { if (!off) { setDraft(neutral); commit(neutral); } }}>{label}</span>
      <input type="range" min={min} max={max} step={1} value={draft} disabled={off} aria-label={label}
        onChange={event => setDraft(Number(event.target.value))}
        onPointerUp={() => commit(draft)} onKeyUp={() => commit(draft)} onBlur={() => { if (!off) commit(draft); }} />
      <span className="lr-number" aria-hidden="true">{draft}</span>
    </label>
    {waiting && <p className="lr-hint">{waiting}</p>}
  </>;
}

const LIQUIFY_MODES: ReadonlyArray<readonly [LiquifyMode, string, string]> = [
  ['push', 'Push', 'Drag to move what is under the brush along your stroke.'],
  ['bloat', 'Enlarge', 'Drag over something to make it larger.'],
  ['pinch', 'Shrink', 'Drag over something to make it smaller.'],
  ['restore', 'Restore', 'Paint over a liquify to take it back toward the photograph as taken.'],
];

const BACKGROUND_MODES: ReadonlyArray<readonly [BackgroundMode, string]> = [
  ['colour', 'Solid colour'], ['keep_shadows', 'Colour, keep shadows'], ['gradient', 'Gradient'], ['blur', 'Blur'],
  ['sky', 'Replace sky'], ['cloud_sky', 'Sky with clouds'],
];

/**
 * Evoto-style finishing: face and body reshaping, portrait light, liquify, background replacement
 * and makeup, each saved as one undoable step. Nothing here is applied automatically; Auto light
 * runs only when pressed. ADR-0108, ADR-0127.
 */
export function PortraitFinishPanel({ finish, disabled, liquify, onSave, onLiquify, onSuggestLight, subject, onFindFaces, initialTab = 'face', notes = [] }: PortraitFinishPanelProps): JSX.Element {
  const [tab, setTab] = useState<Tab>(initialTab);
  useEffect(() => { setTab(initialTab); }, [initialTab]);
  const [faces, setFaces] = useState<EvotoFace[] | null>(null);
  const [findingFaces, setFindingFaces] = useState(false);
  useEffect(() => { setFaces(null); setFindingFaces(false); }, [subject]);
  const findFaces = () => {
    if (!onFindFaces) return;
    const asked = subject;
    setFindingFaces(true);
    onFindFaces().then(found => { if (current.current === asked) setFaces(found); })
      .catch(() => { if (current.current === asked) setFaces([]); })
      .finally(() => { if (current.current === asked) setFindingFaces(false); });
  };
  const chosen = finish.faces ?? [];
  const isChosen = (face: EvotoFace) => chosen.some(p => Math.hypot(p[0] - face.centre[0], p[1] - face.centre[1]) < (face.bounds[2] - face.bounds[0]) * 0.5);
  const toggleFace = (face: EvotoFace) => {
    const next = isChosen(face)
      ? chosen.filter(p => Math.hypot(p[0] - face.centre[0], p[1] - face.centre[1]) >= (face.bounds[2] - face.bounds[0]) * 0.5)
      : [...chosen, face.centre];
    onSave({ ...finish, faces: next }, next.length ? 'Faces: chosen' : 'Faces: every face');
  };
  const [measuring, setMeasuring] = useState(false);
  const [measured, setMeasured] = useState<{ reasons: string[]; failed: boolean } | null>(null);
  const current = useRef(subject);
  // The finish as it is when a measurement comes back, so an edit made meanwhile is kept.
  const latest = useRef(finish);
  latest.current = finish;
  useEffect(() => { current.current = subject; setMeasured(null); setMeasuring(false); }, [subject]);
  const light = finish.light ?? NEUTRAL_LIGHT;
  const autoLight = () => {
    if (!onSuggestLight) return;
    const asked = subject;
    setMeasuring(true);
    onSuggestLight().then(suggestion => {
      if (current.current !== asked) return;
      setMeasured({ reasons: suggestion.reasons, failed: false });
      // Glow is a look rather than a correction, so whatever the photographer set is kept.
      const now = latest.current;
      if (suggestion.faces > 0) onSave({ ...now, light: { ...suggestion.light, glow: (now.light ?? NEUTRAL_LIGHT).glow } }, 'Auto light');
    }).catch(() => {
      if (current.current === asked) setMeasured({ reasons: ['The light on this photograph could not be measured. Try again once the preview has loaded.'], failed: true });
    }).finally(() => { if (current.current === asked) setMeasuring(false); });
  };
  const strokes = finish.liquify ?? [];
  const background: Background = finish.background ?? { mode: 'colour', colour: [1, 1, 1], colour2: [0.85, 0.85, 0.85], amount: 0, feather: 30 };
  const setBackground = (next: Background | null, label: string) => onSave({ ...finish, background: next }, label);
  const setTint = (key: keyof FeatureColours, colour: [number, number, number], amount: number, label: string) =>
    onSave({ ...finish, colours: { ...finish.colours, [key]: amount > 0 ? { colour, amount } : null } }, label);

  return <div className="portrait-finish" aria-label="Portrait finishing">
    <p className="lr-hint">Reshape, light, background and makeup tools in the style of Evoto. Every change is one step in your history, and none of them is ever applied automatically.</p>
    {notes.length > 0 && <div className="portrait-finish-notes" aria-live="polite">{notes.map(note => <p key={note} className="lr-notice">{note}</p>)}</div>}
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
      {MORE_FACE_SLIDERS.map(([title, list]) => <SliderGroup key={title} title={title} list={list} values={finish.face} disabled={disabled}
        onCommit={(key, value, label) => onSave({ ...finish, face: { ...finish.face, [key]: value } }, `Face: ${label}`)} />)}
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, face: NEUTRAL_FACE }, 'Face shape reset')}>Reset face</button>
    </div>}

    {tab === 'body' && <div role="tabpanel" aria-label="Body shape">
      <p className="lr-hint">Applies to the main person. The background bends a little with a strong change, as in any reshaping tool.</p>
      {BODY_SLIDERS.map(([key, label]) => <ShapeSlider key={key} label={label} value={finish.body[key]} disabled={disabled}
        onCommit={value => onSave({ ...finish, body: { ...finish.body, [key]: value } }, `Body: ${label}`)} />)}
      <SliderGroup title="More body shape" list={MORE_BODY_SLIDERS} values={finish.body} disabled={disabled}
        onCommit={(key, value, label) => onSave({ ...finish, body: { ...finish.body, [key]: value } }, `Body: ${label}`)} />
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, body: NEUTRAL_BODY }, 'Body shape reset')}>Reset body</button>
    </div>}

    {tab === 'light' && <div role="tabpanel" aria-label="Portrait light">
      <p className="lr-hint">A virtual light on every face AURA finds, shaped by the head: a key light, a fill for the shadow side, catchlights in eyes without them, and a soft glow. Skin tone is kept; only brightness moves.</p>
      {onSuggestLight && <button type="button" disabled={disabled || measuring} onClick={autoLight}>{measuring ? 'Measuring the light...' : 'Auto light'}</button>}
      {measured && <ul className={measured.failed ? 'lr-notice' : 'lr-hint'} aria-label="What Auto light found">{measured.reasons.map(reason => <li key={reason}>{reason}</li>)}</ul>}
      <div className="lr-crops" aria-label="Light position">{LIGHT_POSITIONS.map(([label, direction, height]) =>
        <button type="button" key={label} disabled={disabled} aria-pressed={light.key > 0 && light.direction === direction && light.height === height}
          onClick={() => onSave({ ...finish, light: { ...light, direction, height, key: light.key > 0 ? light.key : 35 } }, `Light: ${label.toLowerCase()}`)}>{label}</button>)}
      </div>
      {LIGHT_SLIDERS.map(([key, label, min, max]) => <ShapeSlider key={key} label={label} min={min} max={max} neutral={NEUTRAL_LIGHT[key]}
        value={light[key]} disabled={disabled} waiting={waitingFor(key, light)}
        onCommit={value => onSave({ ...finish, light: { ...light, [key]: value } }, `Light: ${label}`)} />)}
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, light: NEUTRAL_LIGHT }, 'Portrait light reset')}>Reset light</button>
    </div>}

    {tab === 'liquify' && <div role="tabpanel" aria-label="Liquify">
      <p className="lr-hint">Background repair restores the backdrop distorted by AURA's saved shape and liquify edits. It keeps the detected subject and softens the boundary. For an imported flattened edit, use a clean original; its missing background cannot be recovered exactly.</p>
      <ShapeSlider label="Background repair" min={0} max={100} value={finish.backgroundRepair ?? 0} disabled={disabled}
        onCommit={backgroundRepair => onSave({ ...finish, backgroundRepair }, 'Liquify background repair')} />
      <button type="button" disabled={disabled || (finish.backgroundRepair ?? 0) === 100}
        onClick={() => onSave({ ...finish, backgroundRepair: 100 }, 'Liquify background repair')}>Repair background automatically</button>
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
        {finish.background.mode !== 'blur' && <label className="lr-toggle">{finish.background.mode === 'colour' || finish.background.mode === 'keep_shadows' ? 'Colour' : 'Top colour'}
          <input type="color" aria-label="Background colour" value={rgbToHex(background.colour)} disabled={disabled}
            onChange={event => setBackground({ ...background, colour: hexToRgb(event.target.value) }, 'Background colour')} /></label>}
        {(finish.background.mode === 'gradient' || finish.background.mode === 'sky' || finish.background.mode === 'cloud_sky') && <label className="lr-toggle">{finish.background.mode === 'gradient' ? 'Bottom colour' : 'Horizon colour'}
          <input type="color" aria-label="Second background colour" value={rgbToHex(background.colour2)} disabled={disabled}
            onChange={event => setBackground({ ...background, colour2: hexToRgb(event.target.value) }, 'Background colour')} /></label>}
        <ShapeSlider label={finish.background.mode === 'blur' ? 'Blur amount' : 'Amount'} min={0} max={100} value={background.amount} disabled={disabled}
          onCommit={amount => setBackground({ ...background, amount }, 'Background amount')} />
        <ShapeSlider label="Edge softness" min={0} max={100} value={background.feather} disabled={disabled}
          onCommit={feather => setBackground({ ...background, feather }, 'Background edge')} />
        <button type="button" disabled={disabled} onClick={() => setBackground(null, 'Background restored')}>Keep original background</button>
      </>}
    </div>}

    {tab === 'skin' && <div role="tabpanel" aria-label="Skin finishing">
      <p className="lr-hint">Measured on every face AURA finds, against that person's own skin. Choose faces in the Faces tab to limit them.</p>
      <SliderGroup list={SKIN_SLIDERS} values={finish.skin} disabled={disabled}
        onCommit={(key, value, label) => onSave({ ...finish, skin: { ...finish.skin, [key]: value } }, `Skin: ${label}`)} />
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, skin: NEUTRAL_SKIN }, 'Skin finishing reset')}>Reset skin</button>
    </div>}

    {tab === 'hair' && <div role="tabpanel" aria-label="Hair and attire">
      <SliderGroup title="Hair and body" list={HAIR_SLIDERS} values={finish.hair} disabled={disabled}
        onCommit={(key, value, label) => onSave({ ...finish, hair: { ...finish.hair, [key]: value } }, `Hair: ${label}`)} />
      <SliderGroup title="Clothing and jewellery" list={ATTIRE_SLIDERS} values={finish.attire} disabled={disabled}
        onCommit={(key, value, label) => onSave({ ...finish, attire: { ...finish.attire, [key]: value } }, `Attire: ${label}`)} />
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, hair: NEUTRAL_HAIR, attire: NEUTRAL_ATTIRE }, 'Hair and attire reset')}>Reset hair and attire</button>
    </div>}

    {tab === 'people' && <div role="tabpanel" aria-label="People and scene">
      <SliderGroup title="People" list={SUBJECT_SLIDERS} values={finish.subject} neutral={{ relightDirection: -45 }} disabled={disabled}
        onCommit={(key, value, label) => onSave({ ...finish, subject: { ...finish.subject, [key]: value } }, `People: ${label}`)} />
      <SliderGroup title="Background" list={SCENE_SLIDERS} values={finish.scene} disabled={disabled}
        onCommit={(key, value, label) => onSave({ ...finish, scene: { ...finish.scene, [key]: value } }, `Background: ${label}`)} />
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, subject: NEUTRAL_SUBJECT, scene: NEUTRAL_SCENE }, 'People and scene reset')}>Reset people and scene</button>
    </div>}

    {tab === 'faces' && <div role="tabpanel" aria-label="Choose faces">
      <p className="lr-hint">Face, skin and makeup changes apply to every face unless you choose some here. Body, people and background changes apply to the whole photograph.</p>
      {onFindFaces && <button type="button" disabled={disabled || findingFaces} onClick={findFaces}>{findingFaces ? 'Finding faces...' : 'Find faces'}</button>}
      {faces && faces.length === 0 && <p role="status">No face was found. Draw a missed face in Portrait retouch.</p>}
      {faces && faces.length > 0 && <ul className="portrait-finish-faces" aria-label="Faces in this photograph">{faces.map((face, i) => <li key={`${face.centre[0]}:${face.centre[1]}`}>
        <label className="lr-toggle"><input type="checkbox" checked={isChosen(face)} disabled={disabled} onChange={() => toggleFace(face)} />
          {`Face ${i + 1} - ${face.centre[0] < 0.4 ? 'left' : face.centre[0] > 0.6 ? 'right' : 'centre'}`}
          {face.closed ? ' (eyes closed)' : ''}</label>
      </li>)}</ul>}
      <p>{chosen.length ? `${chosen.length} face${chosen.length === 1 ? '' : 's'} chosen.` : 'Every face.'}</p>
      <button type="button" disabled={disabled || chosen.length === 0} onClick={() => onSave({ ...finish, faces: [] }, 'Faces: every face')}>Use every face</button>
    </div>}

    {tab === 'makeup' && <div role="tabpanel" aria-label="Makeup and colour">
      <p className="lr-hint">Each colour keeps the light and shade underneath it. Skin tone is never changed here.</p>
      <SliderGroup title="Makeup" list={MAKEUP_SLIDERS} values={finish.makeup} disabled={disabled}
        onCommit={(key, value, label) => onSave({ ...finish, makeup: { ...finish.makeup, [key]: value } }, `Makeup: ${label}`)} />
      <button type="button" disabled={disabled} onClick={() => onSave({ ...finish, makeup: NEUTRAL_MAKEUP }, 'Makeup reset')}>Reset makeup</button>
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
