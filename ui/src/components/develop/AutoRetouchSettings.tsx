import { useState } from 'react';
import type { RecipeDto } from '../../ipc/types';
import { readRetouchPreferences } from '../../ipc/retouchPreferences';
import {
  DEFAULT_RETOUCH_SETTINGS,
  type AutoRetouchOptions, type RetouchScope, type RetouchSettings,
} from '../../ipc/nativeRetouch';

const SCOPES: [RetouchScope, string, string][] = [
  ['face', 'Face', 'Skin, blemishes, lines, eyes and teeth on each detected face.'],
  ['body', 'Body skin', 'All detected visible body skin, including neck, shoulders, arms, hands and legs. Body-only photos can be healed without a visible face.'],
  ['face_and_body', 'Face + body skin', 'All detected visible face and body skin, including forehead, cheeks, nose skin, neck, arms, hands and legs. Inspect the saved selection to check coverage.'],
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
    ['protectNoseDetail', 'Preserve nose detail', 'toggle', 'Protects nose shape and shading from broad finishing. Measured local spot repair treats nose skin while excluding nostril openings and the underside crease.'],
    ['protectEyeArea', 'Protect eyes and surrounding skin', 'toggle', 'Protects eyelids and inner corners from automatic retouch. Measured dark-circle correction can treat skin below the lashes; other automatic tools avoid the surrounding eye area. Manual retouch is available.'],
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
    ['textureGraft', 'Restore skin texture', 'unit', 'Runs last: puts this face’s own pores back where healing and smoothing removed them, in the same place, with glints and deep pits limited. Healed blemishes borrow pores from clean skin nearby. Nothing is generated. Off at 0%.'],
  ]],
  ['Blemishes', [
    ['deepBlemishCleanup', 'Deep blemish cleanup', 'toggle', 'Search the complete segmented face at multiple spot sizes. Eyes, brows, lips and creases remain protected.'],
    ['removeDarkMarks', 'Remove dark marks', 'toggle', 'Include compact dark spots in deep cleanup. This may also remove freckles or beauty marks; review the result.'],
    ['frequencyHeal', 'Frequency healing', 'unit', 'Runs first: rebuilds the tone under measured compact marks from nearby clean skin while retaining pore texture. Local donor repairs follow for remaining detected spots. Off at 0%.'],
    ['blemishSensitivity', 'Blemish sensitivity', 'unit', 'How small a departure still counts as a spot.'],
    ['maxSpots', 'Most spots per face', 'count', 'Raise this for dense acne. Larger passes take longer; remaining measured candidates are reported for review.'],
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
const ACNE_ONLY: Partial<RetouchSettings> = { deepBlemishCleanup: true, removeDarkMarks: true, keepFreckles: false, maxSpots: 900, blemishSensitivity: .85, frequencyHeal: 1, textureGraft: 0, smoothing: 0, toneEvenness: 0, lightEvenness: 0, microDodgeBurn: 0, shine: 0, redness: 0, foreheadLines: 0, crowsFeet: 0, smileLines: 0, underEyeLines: 0, darkCircles: 0, eyeBags: 0, eyeWhitening: 0, eyeVessels: 0, irisDetail: 0, redEye: false, teethWhitening: 0, bodySmoothing: 0, bodyTone: 0, matchBodyToFace: 0, bodyShine: 0, bodyRedness: 0, bodyBlemishes: 1, protectEyeArea: true, protectNoseDetail: true };
/** Starting points modelled on common retouching looks; every control stays adjustable. */
export const PRESETS: Preset[] = [
  ['natural', 'Natural', {}],
  ['acne_only', 'Acne only · preserve detail', ACNE_ONLY],
  ['skin_cleanup', 'Skin cleanup · refine pores', { ...ACNE_ONLY, smoothing: .3, texture: .85, poreRefine: .3, redness: .15, bodySmoothing: .25 }],
  ['acne', 'Deep acne cleanup', { deepBlemishCleanup: true, removeDarkMarks: true, keepFreckles: false, maxSpots: 512, blemishSensitivity: .9, smoothing: .8, texture: .85, toneEvenness: .7, microDodgeBurn: .65, poreRefine: .3, shine: .85, hairDetail: .4, hairShine: .2 }],
  ['pro', 'Professional retouch', { deepBlemishCleanup: true, removeDarkMarks: true, keepFreckles: false, maxSpots: 512, blemishSensitivity: .8, frequencyHeal: 1, textureGraft: .8, smoothing: .35, texture: .9, toneEvenness: .6, microDodgeBurn: .55, shine: .7, hairDetail: .4, hairShine: .2 }],
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
export function AutoRetouchSettings({ disabled, busy = false, onRun, onRunCollection, recipe }: { disabled: boolean; busy?: boolean; onRun: (options: AutoRetouchOptions) => void; onRunCollection?: (options: AutoRetouchOptions) => void; recipe?: RecipeDto | null }) {
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
      ...(key === 'deepBlemishCleanup' && !value ? { maxSpots: Math.min(settings.maxSpots, 24), removeDarkMarks: false } : {}),
    } });
  };
  const choose = ([id, , values, intensity]: Preset) => {
    setPreset(id);
    setOptions({ ...options, intensity: intensity ?? 1, settings: { ...DEFAULT_RETOUCH_SETTINGS, ...values },
      ...(['acne_only', 'skin_cleanup'].includes(id) ? { eyes: false, teeth: false, refine: false, blemishes: true, scope: 'face_and_body' as const } : {}),
    });
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
    <label className="retouch-toggle" title="Measures each face (skin texture, colour evenness, light, shine, marks, facial hair, size in the frame, noise) and tunes the settings below for it. Your settings stay the style; each photo gets its own amounts.">
      <input type="checkbox" checked={options.adaptive !== false} onChange={event => setOptions({ ...options, adaptive: event.target.checked })} />Adapt to each face
    </label>
    <p className="lr-hint">{options.adaptive !== false ? 'Each face is measured and gets its own amounts; the report lists what was changed and why.' : 'The settings below are used exactly as set on every face.'}</p>
    <p className="lr-hint">{scope?.[2]} Skin is found by AI segmentation and measured against the same person's own skin; every result becomes an ordinary operation you can adjust, disable or remove below.</p>
    {settings.deepBlemishCleanup && <p className="lr-hint">Deep cleanup searches the full detected face. {settings.removeDarkMarks ? 'Dark-mark removal is on and can also remove freckles or beauty marks. Review the before/after.' : 'Dark marks are protected.'}</p>}
    {(settings.frequencyHeal > 0 || settings.textureGraft > 0) && <p className="lr-hint">{settings.frequencyHeal > 0 ? 'Frequency healing rebuilds the tone under each mark first and keeps the pores. ' : ''}{settings.textureGraft > 0 ? 'Texture restore runs last and restores the original pores within the selected skin. Protected details stay unchanged.' : ''}</p>}
    {settings.protectEyeArea && options.scope !== 'body' && <p className="lr-hint">Eye protection is on. Eyelids and inner corners stay protected. Measured dark-circle correction can treat skin below the lashes; other automatic tools avoid the surrounding eye area. Use Show retouched areas to inspect the saved selection.</p>}
    {settings.protectNoseDetail && options.scope !== 'body' && <p className="lr-hint">Local spot repair includes nose skin. Nostril openings and the underside crease stay protected; broad healing and finishing preserve the wings and nose shading.</p>}
    <button type="button" className="retouch-primary" disabled={disabled || busy} onClick={() => onRun(options)}>{busy ? 'Detecting and retouching…' : `Auto retouch: ${scope?.[1] ?? 'Face'}`}</button>
    {onRunCollection && <><button type="button" disabled={disabled || busy} onClick={()=>onRunCollection(options)}>Apply cleanup settings to this collection</button>
      <p className="lr-hint">Uses these settings on each photo, with a fresh skin selection and repairs for that photo. Manual edits and exposure remain saved.</p></>}
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
        const [min, max, scale] = kind === 'signed' ? [-100, 100, 100] : kind === 'count' ? [1, settings.deepBlemishCleanup ? 900 : 24, 1] : [0, 100, 100];
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
