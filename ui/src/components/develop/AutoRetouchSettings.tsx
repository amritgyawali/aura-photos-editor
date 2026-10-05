import { useState } from 'react';
import type { RecipeDto } from '../../ipc/types';
import { readRetouchPreferences } from '../../ipc/retouchPreferences';
import {
  DEFAULT_RETOUCH_SETTINGS,
  type AutoRetouchOptions, type RetouchScope, type RetouchSettings,
} from '../../ipc/nativeRetouch';

const SCOPES: [RetouchScope, string, string][] = [
  ['face', 'Face', 'Skin, blemishes, lines, eyes and teeth on each detected face.'],
  ['body', 'Body skin', 'Neck, shoulders, chest and arms below each face. The face is left as it is.'],
  ['face_and_body', 'Face + body skin', 'Everything on the face, plus matching body skin.'],
];

const FEATURES: [keyof Pick<AutoRetouchOptions, 'blemishes' | 'refine' | 'eyes' | 'teeth'>, string, string][] = [
  ['blemishes', 'Heal blemishes', 'Repairs measured spots with nearby skin texture. Dark marks are kept unless Remove dark marks is enabled.'],
  ['refine', 'Soften lines and redness', 'Fine lines, smile lines and redness beside the nose, only where they measure stronger than the cheek.'],
  ['eyes', 'Eyes', 'Iris detail, redness in the whites, flash red-eye and under-eye shadows, only where measured.'],
  ['teeth', 'Teeth', 'Reduce a measured yellow cast on visible teeth.'],
];

type Kind = 'unit' | 'signed' | 'toggle' | 'count';
type Control = [keyof RetouchSettings, string, Kind, string];
/** Every fine control, grouped as a retoucher would look for them. */
export const SETTING_GROUPS: [string, Control[]][] = [
  ['Skin detection', [
    ['aiSkinDetection', 'AI skin detection', 'toggle', 'Find face and body skin with the bundled person segmenter. Off uses landmarks and sampled colour.'],
    ['mainSubjectOnly', 'Main subject only', 'toggle', 'Retouch only the largest face and its body.'],
    ['maskPrecision', 'Skin mask precision', 'unit', 'Higher keeps beard, brows, lips and make-up further out of the skin selection.'],
    ['edgeSoftness', 'Mask edge softness', 'unit', 'How gradually the retouch fades out at the edge of the skin.'],
    ['protectFacialHair', 'Protect beard and stubble', 'toggle', 'Never smooth facial hair.'],
  ]],
  ['Skin', [
    ['smoothing', 'Skin smoothing', 'unit', 'Mid-scale unevenness between pores and facial form. 50% is the measured amount.'],
    ['texture', 'Keep pore texture', 'unit', 'Retains original fine skin detail separately from larger uneven texture. Review at full size.'],
    ['smoothingSize', 'Smoothing size', 'unit', 'Small evens fine unevenness, large evens broad patches.'],
    ['toneEvenness', 'Even skin tone', 'unit', 'Blotchy colour evened toward the same person’s own skin.'],
    ['lightEvenness', 'Even skin light', 'unit', 'Patchy light within the skin evened.'],
    ['microDodgeBurn', 'Micro dodge & burn', 'unit', 'Small light and dark variations evened, as a retoucher would by hand.'],
    ['poreRefine', 'Refine pores', 'unit', 'Softens enlarged pores a little; texture stays.'],
    ['shine', 'Reduce shine', 'unit', 'Oily shine and hot spots softened.'],
    ['redness', 'Reduce redness', 'unit', 'Redness beside the nose evened toward the cheek.'],
    ['glow', 'Skin glow', 'unit', 'A soft lift of the skin’s own highlights.'],
    ['skinBrightness', 'Skin brightness', 'signed', 'Brighten or deepen the face skin. Neutral by default.'],
    ['skinWarmth', 'Skin warmth', 'signed', 'Warmer or cooler skin. Neutral by default.'],
    ['skinTint', 'Skin tint', 'signed', 'Magenta or green skin tint. Neutral by default.'],
  ]],
  ['Blemishes', [
    ['deepBlemishCleanup', 'Deep blemish cleanup', 'toggle', 'Search the complete segmented face at multiple spot sizes. Eyes, brows, lips and creases remain protected.'],
    ['removeDarkMarks', 'Remove dark marks', 'toggle', 'Include compact dark spots in deep cleanup. This may also remove freckles or beauty marks; review the result.'],
    ['blemishSensitivity', 'Blemish sensitivity', 'unit', 'How small a departure still counts as a spot.'],
    ['maxSpots', 'Most spots per face', 'count', 'A face with more is left for you to judge.'],
    ['keepFreckles', 'Keep freckles', 'toggle', 'A field of many small marks is treated as freckles and kept.'],
  ]],
  ['Lines & wrinkles', [
    ['foreheadLines', 'Forehead lines', 'unit', 'Softened, never erased.'],
    ['crowsFeet', 'Crow’s feet', 'unit', 'Lines beside the eyes.'],
    ['smileLines', 'Smile lines', 'unit', 'The fold from nose to mouth, lifted where darker than the cheek.'],
    ['underEyeLines', 'Under-eye lines', 'unit', 'Fine lines below the eyes.'],
    ['neckLines', 'Neck lines', 'unit', 'Horizontal lines on the neck (needs body skin).'],
  ]],
  ['Under eyes', [
    ['darkCircles', 'Dark circles', 'unit', 'Lifted only where darker than the same person’s cheek.'],
    ['eyeBags', 'Eye bags', 'unit', 'The puffy band below the shadow, evened.'],
  ]],
  ['Eyes & brows', [
    ['eyeWhitening', 'Whiten eyes', 'unit', 'A small lift of the whites, never paper white.'],
    ['eyeVessels', 'Eye vessels', 'unit', 'Red vessels in the whites reduced.'],
    ['irisDetail', 'Iris detail', 'unit', 'Fine iris and lash detail.'],
    ['irisBrightness', 'Iris brilliance', 'unit', 'A soft lift of the iris.'],
    ['redEye', 'Fix red-eye', 'toggle', 'Flash red-eye corrected when measured.'],
    ['lashDefinition', 'Lash definition', 'unit', 'Detail and a little depth along the lash line.'],
    ['browDefinition', 'Brow definition', 'unit', 'Brow hair detail; the shape is never changed.'],
  ]],
  ['Mouth', [
    ['teethWhitening', 'Whiten teeth', 'unit', 'Reduces a measured yellow cast.'],
    ['lipColour', 'Lip colour', 'unit', 'A natural rose tint, never on the teeth.'],
    ['lipDefinition', 'Lip definition', 'unit', 'Lip texture and edge detail.'],
  ]],
  ['Portrait volumes & make-up', [
    ['contour', 'Contour', 'unit', 'Soft shadow under the cheekbones and along the jaw.'],
    ['highlight', 'Highlight', 'unit', 'Soft light on the nose bridge, cheekbones, brow bone and chin.'],
    ['blush', 'Blush', 'unit', 'A warm blush on the cheeks.'],
    ['faceLight', 'Face fill light', 'unit', 'Lifts the whole face relative to its surroundings.'],
  ]],
  ['Body', [
    ['bodySmoothing', 'Body smoothing', 'unit', 'Neck, shoulders, arms and hands.'],
    ['bodyTone', 'Even body tone', 'unit', 'Blotchy body skin evened.'],
    ['matchBodyToFace', 'Match body to face', 'unit', 'Body skin colour moved toward the same person’s face.'],
    ['bodyShine', 'Body shine', 'unit', 'Shine on shoulders and arms softened.'],
    ['bodyRedness', 'Red hands & elbows', 'unit', 'Redness on hands, knuckles and elbows evened.'],
    ['bodyBlemishes', 'Body blemishes', 'unit', 'Small spots on body skin healed.'],
  ]],
  ['Hair, clothes & backdrop', [
    ['hairDetail', 'Hair detail', 'unit', 'Strand definition inside the segmented hair.'],
    ['hairShine', 'Hair shine', 'unit', 'Lifts the hair’s own highlights.'],
    ['fabric', 'Fabric creases', 'unit', 'Creases in clothing softened; seams and weave kept.'],
    ['backdrop', 'Clean backdrop', 'unit', 'Smooths a plain studio backdrop; textured backgrounds are left alone.'],
  ]],
];

type Preset = [string, string, Partial<RetouchSettings>, number?];
/** Starting points modelled on common retouching looks; every control stays adjustable. */
export const PRESETS: Preset[] = [
  ['natural', 'Natural', {}],
  ['acne', 'Deep acne cleanup', { deepBlemishCleanup: true, removeDarkMarks: true, keepFreckles: false, maxSpots: 220, blemishSensitivity: .9, smoothing: .8, texture: .85, toneEvenness: .7, microDodgeBurn: .65, poreRefine: .3, shine: .85, hairDetail: .4, hairShine: .2 }],
  ['subtle', 'Subtle', { smoothing: .35, toneEvenness: .4, microDodgeBurn: .15, eyeWhitening: .1, underEyeLines: .15 }, .8],
  ['soft', 'Soft glow', { smoothing: .75, texture: .4, glow: .4, eyeWhitening: .4, darkCircles: .7, blush: .2 }],
  ['beauty', 'Polished beauty', { smoothing: .85, toneEvenness: .75, microDodgeBurn: .6, poreRefine: .4, contour: .5, highlight: .5, lipColour: .35, lashDefinition: .5, browDefinition: .4, irisBrightness: .4, eyeWhitening: .4, hairDetail: .4, hairShine: .3 }],
  ['bridal', 'Bridal', { smoothing: .65, glow: .3, eyeWhitening: .35, irisBrightness: .3, lashDefinition: .3, lipColour: .25, blush: .2, highlight: .3, teethWhitening: .6, bodySmoothing: .6, matchBodyToFace: .5, fabric: .4 }],
  ['groom', 'Groom & men', { smoothing: .35, texture: .8, protectFacialHair: true, darkCircles: .6, eyeBags: .5, shine: .8, eyeWhitening: .15 }],
  ['editorial', 'Editorial', { smoothing: .6, texture: .6, microDodgeBurn: .6, contour: .6, highlight: .5, browDefinition: .4, lashDefinition: .4, hairDetail: .4 }],
  ['body', 'Body focus', { bodySmoothing: .7, bodyTone: .7, matchBodyToFace: .6, bodyRedness: .5, bodyBlemishes: .5, bodyShine: .5, neckLines: .4 }],
  ['studio', 'Studio clean', { fabric: .6, backdrop: .6, hairShine: .3, shine: .7 }],
];

const percent = (v: number) => `${Math.round(v * 100)}%`;

/** Choose what the automatic retouch works on and how strongly, then run it. */
export function AutoRetouchSettings({ disabled, busy = false, onRun, recipe }: { disabled: boolean; busy?: boolean; onRun: (options: AutoRetouchOptions) => void; recipe?: RecipeDto | null }) {
  const [options, setOptions] = useState<AutoRetouchOptions>(() => readRetouchPreferences(recipe));
  const [preset, setPreset] = useState(() => options.intensity === 1 &&
    (Object.keys(DEFAULT_RETOUCH_SETTINGS) as (keyof RetouchSettings)[]).every(key =>
      options.settings?.[key] === DEFAULT_RETOUCH_SETTINGS[key]) ? 'natural' : 'custom');
  const settings = options.settings ?? DEFAULT_RETOUCH_SETTINGS;
  const label = options.intensity < 0.8 ? 'Subtle' : options.intensity > 1.2 ? 'Polished' : 'Natural';
  const faceFeatures = options.scope !== 'body';
  const scope = SCOPES.find(([value]) => value === options.scope) ?? SCOPES[0];
  const change = (key: keyof RetouchSettings, value: number | boolean) => {
    setPreset('custom');
    setOptions({ ...options, settings: { ...settings, [key]: value,
      ...(key === 'deepBlemishCleanup' && value === false ? { maxSpots: Math.min(24, settings.maxSpots) } : {}),
    } });
  };
  const choose = ([id, , values, intensity]: Preset) => {
    setPreset(id);
    setOptions({ ...options, intensity: intensity ?? 1, settings: { ...DEFAULT_RETOUCH_SETTINGS, ...values } });
  };
  return <fieldset className="auto-retouch-settings" disabled={disabled}>
    <legend>Automatic retouch</legend>
    <div role="radiogroup" aria-label="What to retouch" className="retouch-scope">
      {SCOPES.map(([value, name, hint]) => <label key={value} className="retouch-toggle" title={hint}>
        <input type="radio" name="auto-retouch-scope" value={value} checked={options.scope === value}
          onChange={() => setOptions({ ...options, scope: value })} />{name}
      </label>)}
    </div>
    <div role="radiogroup" aria-label="Retouch preset" className="retouch-presets">
      {PRESETS.map(p => <label key={p[0]} className="retouch-toggle">
        <input type="radio" name="auto-retouch-preset" value={p[0]} checked={preset === p[0]} onChange={() => choose(p)} />{p[1]}
      </label>)}
      {preset === 'custom' && <span className="lr-hint">Custom</span>}
    </div>
    <p className="lr-hint">{scope?.[2]} Skin is found by AI segmentation and measured against the same person's own skin; every result becomes an ordinary operation you can adjust, disable or remove below.</p>
    <button type="button" className="retouch-primary" disabled={disabled || busy} onClick={() => onRun(options)}>{busy ? 'Detecting and retouching…' : `Auto retouch: ${scope?.[1] ?? 'Face'}`}</button>
    <details>
      <summary>Strength and details</summary>
      <label>Strength: {label} ({Math.round(options.intensity * 100)}%)
        <input type="range" min={25} max={150} step={5} value={Math.round(options.intensity * 100)}
          aria-label="Automatic retouch strength" onChange={event => setOptions({ ...options, intensity: Number(event.target.value) / 100 })} />
      </label>
      {FEATURES.map(([key, name, hint]) => <label key={key} className="retouch-toggle" title={hint}>
        <input type="checkbox" checked={options[key]} disabled={!faceFeatures} onChange={event => setOptions({ ...options, [key]: event.target.checked })} />{name}
      </label>)}
      {!faceFeatures && <p className="lr-hint">Face details are not used while only body skin is selected.</p>}
    </details>
    {SETTING_GROUPS.map(([group, controls]) => <details key={group} className="retouch-setting-group">
      <summary>{group}</summary>
      {controls.map(([key, name, kind, hint]) => {
        const value = settings[key];
        if (kind === 'toggle') {
          return <label key={key} className="retouch-toggle" title={hint}>
            <input type="checkbox" checked={Boolean(value)} onChange={event => change(key, event.target.checked)} />{name}
          </label>;
        }
        const n = Number(value);
        const [min, max, scale] = kind === 'signed' ? [-100, 100, 100] : kind === 'count' ? [1, settings.deepBlemishCleanup ? 220 : 24, 1] : [0, 100, 100];
        const shown = kind === 'count' ? String(n) : kind === 'signed' ? `${n > 0 ? '+' : ''}${Math.round(n * 100)}` : percent(n);
        return <label key={key} title={hint}>{name}: {shown}
          <input type="range" min={min} max={max} step={1} value={Math.round(n * scale)} aria-label={name}
            onChange={event => change(key, Number(event.target.value) / scale)} />
        </label>;
      })}
    </details>)}
    <p className="lr-hint">These choices belong to this photo only. Running it again replaces the automatic operations; operations you added yourself are kept. Undo restores the previous version.</p>
  </fieldset>;
}
