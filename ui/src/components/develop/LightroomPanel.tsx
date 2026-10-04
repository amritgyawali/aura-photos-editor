import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { RecipeDto } from '../../ipc/types';
import type { EditProfile } from '../../ipc/client';
import { PointCurveEditor, type CurvePoint } from './PointCurveEditor';

/**
 * Every Lightroom Develop panel, on one photograph: Basic, Tone Curve (parametric, point and
 * RGB), Color Mixer, Black & White, Color Grading, Detail, Lens, Effects, Calibration and
 * Transform - plus Presets, Auto and Sync.
 *
 * Every control writes one recipe path through `setParam`, which records it as the
 * photographer's own setting: no automatic pass, profile or reference match changes it again.
 * A slider commits when it is released, so dragging does not queue a render per pixel.
 */

export type LightroomPanelProps = {
  recipe: RecipeDto | null;
  disabled: boolean;
  /** Width over height of the photograph, for the crop presets. */
  aspect: number | null;
  profiles: EditProfile[];
  onSetParam: (path: string, value: unknown, label: string) => void;
  onAuto: () => void;
  onApplyProfile: (profileId: string, strength: number) => void;
  onSync: (includeGeometry: boolean) => void;
  mode?: 'essentials' | 'advanced';
  syncControls?: ReactNode;
};

type Control = { path: string; label: string; min: number; max: number; step?: number; fallback: number; hint?: string };

const BANDS = ['red', 'orange', 'yellow', 'green', 'aqua', 'blue', 'purple', 'magenta'] as const;
const BAND_COLOURS: Record<string, string> = {
  red: '#e0473c', orange: '#ec8b34', yellow: '#e8d23d', green: '#58b04a',
  aqua: '#3fbfb4', blue: '#3f6fe0', purple: '#8a4fd8', magenta: '#d84fb4',
};

const c = (path: string, label: string, min: number, max: number, fallback = 0, step = 1, hint?: string): Control =>
  ({ path, label, min, max, fallback, step, hint });

const BASIC: Control[] = [
  c('global.temperature', 'Temp', 2000, 12000, 5500, 50, 'Kelvin, 5500 is neutral'),
  c('global.tint', 'Tint', -150, 150),
  c('global.exposure', 'Exposure', -5, 5, 0, 0.05),
  c('global.contrast', 'Contrast', -100, 100),
  c('global.highlights', 'Highlights', -100, 100),
  c('global.shadows', 'Shadows', -100, 100),
  c('global.whites', 'Whites', -100, 100),
  c('global.blacks', 'Blacks', -100, 100),
  c('global.texture', 'Texture', -100, 100),
  c('global.clarity', 'Clarity', -100, 100),
  c('global.dehaze', 'Dehaze', -100, 100),
  c('global.vibrance', 'Vibrance', -100, 100),
  c('global.saturation', 'Saturation', -100, 100),
];
const PARAMETRIC: Control[] = [
  c('global.parametric.highlights', 'Highlights', -100, 100),
  c('global.parametric.lights', 'Lights', -100, 100),
  c('global.parametric.darks', 'Darks', -100, 100),
  c('global.parametric.shadows', 'Shadows', -100, 100),
  c('global.parametric.shadow_split', 'Shadow split', 5, 85, 25),
  c('global.parametric.midtone_split', 'Midtone split', 10, 90, 50),
  c('global.parametric.highlight_split', 'Highlight split', 15, 95, 75),
];
const DETAIL: Control[] = [
  c('global.sharpen.amount', 'Sharpening', 0, 150),
  c('global.sharpen.radius', 'Radius', 0.5, 3, 1, 0.1),
  c('global.sharpen.detail', 'Detail', 0, 100, 25),
  c('global.sharpen.masking', 'Masking', 0, 100),
  c('global.noise.luminance', 'Noise reduction', 0, 100),
  c('global.noise.detail', 'NR detail', 0, 100, 50),
  c('global.noise.colour', 'Color noise', 0, 100),
];
const EFFECTS: Control[] = [
  c('global.effects.vignette.amount', 'Vignette', -100, 100),
  c('global.effects.vignette.midpoint', 'Midpoint', 0, 100, 50),
  c('global.effects.vignette.roundness', 'Roundness', -100, 100),
  c('global.effects.vignette.feather', 'Feather', 0, 100, 50),
  c('global.effects.vignette.highlights', 'Highlights', 0, 100),
  c('global.effects.grain.amount', 'Grain', 0, 100),
  c('global.effects.grain.size', 'Grain size', 0, 100, 25),
  c('global.effects.grain.roughness', 'Roughness', 0, 100, 50),
];
const CALIBRATION: Control[] = [
  c('global.calibration.shadows_tint', 'Shadows tint', -100, 100),
  c('global.calibration.red_hue', 'Red hue', -100, 100),
  c('global.calibration.red_saturation', 'Red saturation', -100, 100),
  c('global.calibration.green_hue', 'Green hue', -100, 100),
  c('global.calibration.green_saturation', 'Green saturation', -100, 100),
  c('global.calibration.blue_hue', 'Blue hue', -100, 100),
  c('global.calibration.blue_saturation', 'Blue saturation', -100, 100),
];
const WHEELS = [['shadows', 'Shadows'], ['midtones', 'Midtones'], ['highlights', 'Highlights'], ['global', 'Global']] as const;
const CURVES = [
  ['global.curve.points', 'Luminance', '#e8e3ee'],
  ['global.channel_curves.red.points', 'Red', '#e0473c'],
  ['global.channel_curves.green.points', 'Green', '#58b04a'],
  ['global.channel_curves.blue.points', 'Blue', '#3f6fe0'],
] as const;
const CROPS: [string, number | null][] = [['Original', null], ['1:1', 1], ['4:5', 4 / 5], ['3:2', 3 / 2], ['16:9', 16 / 9], ['9:16', 9 / 16]];

/** One recipe value, by path, with a fallback for a block the recipe leaves absent. */
export function paramValue(recipe: RecipeDto | null, path: string): unknown {
  return recipe?.params.find(p => p.path === path)?.value;
}

function isProtected(recipe: RecipeDto | null, path: string): boolean {
  return Boolean(recipe?.params.find(p => p.path === path)?.protected);
}

function Slider({ control, recipe, disabled, onSetParam, background }: {
  control: Control; recipe: RecipeDto | null; disabled: boolean;
  onSetParam: LightroomPanelProps['onSetParam']; background?: string;
}): JSX.Element {
  const stored = Number(paramValue(recipe, control.path) ?? control.fallback);
  const [draft, setDraft] = useState(stored);
  const [numberText, setNumberText] = useState(String(stored));
  const committed = useRef(stored);
  useEffect(() => {
    setDraft(stored); setNumberText(String(stored)); committed.current = stored;
  }, [stored, recipe]);
  const change = (value: number) => { setDraft(value); setNumberText(String(value)); };
  const commit = (value: number) => {
    if (disabled || !Number.isFinite(value)) {
      change(stored);
      return;
    }
    const clamped = Math.min(control.max, Math.max(control.min, value));
    change(clamped);
    if (Math.abs(clamped - committed.current) > 1e-9) {
      committed.current = clamped;
      onSetParam(control.path, clamped, control.label);
    }
  };
  const commitNumber = () => commit(numberText.trim() === '' ? NaN : Number(numberText));
  return <label className="lr-slider" title={control.hint ?? 'Double-click to reset'}>
    <span className="lr-slider-label" onDoubleClick={() => commit(control.fallback)}>
      {control.label}{isProtected(recipe, control.path) && <small title="Your setting - automatic edits leave it alone"> ●</small>}
    </span>
    <input type="range" min={control.min} max={control.max} step={control.step ?? 1} value={draft} disabled={disabled}
      aria-label={control.label}
      style={background ? { background } : undefined}
      onChange={event => change(Number(event.target.value))}
      onPointerUp={() => commit(draft)} onKeyUp={() => commit(draft)} onBlur={() => commit(draft)} />
    <input className="lr-number" type="number" min={control.min} max={control.max} step={control.step ?? 1}
      value={numberText} disabled={disabled}
      onChange={event => setNumberText(event.target.value)} onBlur={commitNumber}
      onKeyDown={event => {
        if (event.key === 'Enter') commitNumber();
        if (event.key === 'Escape') change(stored);
      }} aria-label={`${control.label} value`} />
  </label>;
}

function Section({ title, children, open = false }: { title: string; children: React.ReactNode; open?: boolean }): JSX.Element {
  return <details className="lr-section" open={open}><summary>{title}</summary><div className="lr-section-body">{children}</div></details>;
}

const HUE_GRADIENT = 'linear-gradient(90deg, #e0473c, #e8d23d, #58b04a, #3fbfb4, #3f6fe0, #d84fb4, #e0473c)';

export function LightroomPanel({ recipe, disabled, aspect, profiles, onSetParam, onAuto, onApplyProfile, onSync, syncControls, mode = 'advanced' }: LightroomPanelProps): JSX.Element {
  const [mixer, setMixer] = useState<'h' | 's' | 'l'>('s');
  const [curve, setCurve] = useState(0);
  const [preset, setPreset] = useState('');
  const [presetStrength, setPresetStrength] = useState(100);
  const [syncGeometry, setSyncGeometry] = useState(false);
  const bw = paramValue(recipe, 'bw');
  const isBw = recipe ? recipe.params.some(p => p.path.startsWith('bw.') || (p.path === 'bw' && p.value !== null)) : false;
  const curveInfo = CURVES[curve] ?? CURVES[0];
  const curvePoints = useMemo(() => {
    const value = paramValue(recipe, curveInfo[0]);
    return Array.isArray(value) ? value as CurvePoint[] : [[0, 0], [255, 255]] as CurvePoint[];
  }, [recipe, curveInfo]);
  const slider = (control: Control, background?: string) =>
    <Slider key={control.path} control={control} recipe={recipe} disabled={disabled} onSetParam={onSetParam} background={background} />;

  const crop = (ratio: number | null) => {
    if (ratio === null || !aspect) { onSetParam('geometry.crop', [0, 0, 1, 1], 'Crop'); return; }
    // The largest centred rectangle of this ratio inside the frame, in normalised edges.
    const width = ratio >= aspect ? 1 : ratio / aspect;
    const height = ratio >= aspect ? aspect / ratio : 1;
    const left = (1 - width) / 2;
    const top = (1 - height) / 2;
    onSetParam('geometry.crop', [left, top, left + width, top + height].map(v => Math.round(v * 10000) / 10000), `Crop ${ratio.toFixed(2)}`);
  };

  return <div className="lr-panel" aria-label="Develop">
    <div className="lr-quick">
      <button type="button" className="is-primary" disabled={disabled} onClick={onAuto}>Auto enhance photo</button>
      <select aria-label="Preset" value={preset} disabled={disabled || profiles.length === 0} onChange={event => setPreset(event.target.value)}>
        <option value="">Presets…</option>
        {profiles.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
      </select>
      <input type="range" min={0} max={150} value={presetStrength} aria-label="Preset strength" disabled={disabled || !preset}
        onChange={event => setPresetStrength(Number(event.target.value))} />
      <button type="button" disabled={disabled || !preset} onClick={() => onApplyProfile(preset, presetStrength / 100)}>Apply {presetStrength}%</button>
    </div>

    <Section title={mode === 'essentials' ? 'Light & color' : 'Basic'} open>{BASIC.filter(control => mode === 'advanced' ||
      ['global.exposure', 'global.contrast', 'global.highlights', 'global.shadows', 'global.temperature', 'global.vibrance'].includes(control.path)).map(control => slider(control,
      control.path === 'global.temperature' ? 'linear-gradient(90deg, #4f7fe0, #e8e3d8, #e0a040)'
        : control.path === 'global.tint' ? 'linear-gradient(90deg, #58b04a, #e8e3d8, #d84fb4)' : undefined))}</Section>

    {mode === 'advanced' && <><Section title="Tone Curve">
      <div className="lr-tabs" role="tablist">{CURVES.map(([, label], index) =>
        <button key={label} type="button" role="tab" aria-selected={curve === index} onClick={() => setCurve(index)}>{label}</button>)}</div>
      <PointCurveEditor key={curveInfo[0]} points={curvePoints} colour={curveInfo[2]} disabled={disabled}
        onCommit={points => onSetParam(curveInfo[0], points, `${curveInfo[1]} curve`)} />
      <p className="lr-hint">Click to add a point, drag to move, double-click to remove.</p>
      {PARAMETRIC.map(control => slider(control))}
    </Section>

    <Section title="Color Mixer">
      <div className="lr-tabs" role="tablist">{([['h', 'Hue'], ['s', 'Saturation'], ['l', 'Luminance']] as const).map(([key, label]) =>
        <button key={key} type="button" role="tab" aria-selected={mixer === key} onClick={() => setMixer(key)}>{label}</button>)}</div>
      {BANDS.map(band => slider(c(`global.hsl.${band}.${mixer}`, band[0]?.toUpperCase() + band.slice(1), -100, 100),
        `linear-gradient(90deg, #3a3640, ${BAND_COLOURS[band]})`))}
    </Section>

    <Section title="Black & White">
      <label className="lr-toggle"><input type="checkbox" checked={isBw} disabled={disabled}
        onChange={event => onSetParam('bw', event.target.checked ? (bw && typeof bw === 'object' ? bw : { mix: {}, grade: null }) : null, event.target.checked ? 'Black & white' : 'Colour')} /> Convert to black & white</label>
      {isBw && BANDS.map(band => slider(c(`bw.mix.${band}`, band[0]?.toUpperCase() + band.slice(1), -100, 100),
        `linear-gradient(90deg, #111, ${BAND_COLOURS[band]}, #fff)`))}
    </Section>

    <Section title="Color Grading">
      {WHEELS.map(([key, label]) => <fieldset key={key} className="lr-wheel"><legend>{label}</legend>
        {slider(c(`global.colour_grade.${key}.hue`, 'Hue', 0, 359), HUE_GRADIENT)}
        {slider(c(`global.colour_grade.${key}.saturation`, 'Saturation', 0, 100))}
        {slider(c(`global.colour_grade.${key}.luminance`, 'Luminance', -100, 100))}
      </fieldset>)}
      {slider(c('global.colour_grade.blending', 'Blending', 0, 100, 50))}
      {slider(c('global.colour_grade.balance', 'Balance', -100, 100))}
    </Section>

    <Section title="Detail">{DETAIL.map(control => slider(control))}</Section>

    <Section title="Lens Corrections">
      {slider(c('lens.vignette', 'Vignetting correction', 0, 100))}
      <label className="lr-toggle"><input type="checkbox" disabled={disabled} checked={paramValue(recipe, 'lens.distortion') === true}
        onChange={event => onSetParam('lens.distortion', event.target.checked, 'Distortion correction')} /> Profile distortion correction</label>
      <label className="lr-toggle"><input type="checkbox" disabled={disabled} checked={paramValue(recipe, 'lens.ca') === true}
        onChange={event => onSetParam('lens.ca', event.target.checked, 'Chromatic aberration')} /> Remove chromatic aberration</label>
    </Section>
    </>}

    <Section title="Transform & Crop">
      {slider(c('geometry.rotate', 'Straighten', -45, 45, 0, 0.1))}
      <div className="lr-crops">{CROPS.map(([label, ratio]) =>
        <button key={label} type="button" disabled={disabled || (ratio !== null && !aspect)} onClick={() => crop(ratio)}>{label}</button>)}</div>
    </Section>

    {mode === 'advanced' && <><Section title="Effects">{EFFECTS.map(control => slider(control))}</Section>

    <Section title="Calibration">{CALIBRATION.map(control => slider(control))}</Section></>}

    {syncControls ?? <div className="lr-sync">
      <label className="lr-toggle"><input type="checkbox" checked={syncGeometry} onChange={event => setSyncGeometry(event.target.checked)} /> Include crop</label>
      <button type="button" disabled={disabled} onClick={() => onSync(syncGeometry)}>Sync settings to all photos</button>
    </div>}
  </div>;
}
