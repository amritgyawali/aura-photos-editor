import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AutoRetouchSettings, PRESETS, SETTING_GROUPS, presetOf } from './AutoRetouchSettings';
import type { RecipeDto } from '../../ipc/types';
import { DEFAULT_RETOUCH_SETTINGS } from '../../ipc/nativeRetouch';
import { automaticLabel } from './NativeRetouchWorkspace';

describe('automatic retouch settings', () => {
  it('limits Acne only to blemishes and preserves eye and nose detail without beauty finishing',()=>{
    const run=vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run}/>);
    fireEvent.click(screen.getByRole('radio',{name:'Acne only · preserve detail'}));
    fireEvent.click(screen.getByRole('button',{name:'Auto retouch: Face + body skin'}));
    expect(run).toHaveBeenCalledWith(expect.objectContaining({eyes:false,teeth:false,refine:false,blemishes:true,scope:'face_and_body',
      settings:expect.objectContaining({protectEyeArea:true,protectNoseDetail:true,smoothing:0,toneEvenness:0,lightEvenness:0,shine:0,textureGraft:0,hairDetail:0,bodyBlemishes:1,bodySmoothing:0,bodyTone:0,matchBodyToFace:0,bodyShine:0,maxSpots:900})}));
  });
  it('offers face, body skin and face + body skin, and runs the chosen one', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    expect(screen.getByRole('radio', { name: 'Face' })).toBeTruthy();
    fireEvent.click(screen.getByRole('radio', { name: 'Body skin' }));
    expect(screen.getByText(/arms, hands and legs/)).toBeTruthy();
    expect(screen.getByText(/without a visible face/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Body skin' }));
    expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'body' }));
    fireEvent.click(screen.getByRole('radio', { name: 'Face + body skin' }));
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
    expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'face_and_body' }));
  });
  it('sends the chosen strength and features', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    fireEvent.click(screen.getByText('Strength and details'));
    fireEvent.change(screen.getByLabelText('Automatic retouch strength'), { target: { value: '60' } });
    expect(screen.getByText(/Strength: Subtle \(60%\)/)).toBeTruthy();
    fireEvent.click(screen.getByLabelText('Teeth'));
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
    expect(run).toHaveBeenCalledWith({ intensity: 0.6, blemishes: true, eyes: true, teeth: false, refine: true, scope: 'face', settings: DEFAULT_RETOUCH_SETTINGS, adaptive: true });
  });
  it('adapts to each face unless the photographer wants the settings used exactly', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    const adapt = screen.getByLabelText('Adapt to each face') as HTMLInputElement;
    expect(adapt.checked).toBe(true);
    expect(screen.getByText(/Each face is measured and gets its own amounts/)).toBeTruthy();
    fireEvent.click(adapt);
    expect(screen.getByText(/used exactly as set on every face/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
    expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ adaptive: false }));
  });
  it('turns face details off for body skin only and disables everything while busy', () => {
    const { rerender } = render(<AutoRetouchSettings disabled={false} onRun={vi.fn()} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Body skin' }));
    expect((screen.getByLabelText('Teeth') as HTMLInputElement).disabled).toBe(true);
    expect(screen.getByTitle(/Repairs measured spots/)).toBeTruthy();
    rerender(<AutoRetouchSettings disabled busy onRun={vi.fn()} />);
    expect((screen.getByRole('button', { name: 'Detecting and retouching…' }) as HTMLButtonElement).closest('fieldset')?.disabled).toBe(true);
  });
  it('labels automatic operations by face or body', () => {
    expect(automaticLabel('auto-portrait-v1-0-texture')).toBe(' · Auto (face 1)');
    expect(automaticLabel('auto-portrait-v1-1-body-tone')).toBe(' · Auto (body 2)');
    expect(automaticLabel('auto-scene-v1-sky')).toBe(' · Auto (scene)');
    expect(automaticLabel('manual-id')).toBe('');
    expect(automaticLabel('auto-portrait-v1-0-lines-neck')).toBe(' · Auto (body 1)');
    expect(automaticLabel('auto-portrait-v1-1-hair-detail')).toBe(' · Auto (hair 2)');
    expect(automaticLabel('auto-portrait-v1-0-fabric')).toBe(' · Auto (clothes 1)');
    expect(automaticLabel('auto-portrait-v1-backdrop')).toBe(' · Auto (backdrop)');
  });
  it('offers every fine control exactly once', () => {
    const keys = SETTING_GROUPS.flatMap(([, controls]) => controls.map(([key]) => key));
    expect(keys.length).toBe(61);
    expect(new Set(keys).size).toBe(61);
    expect([...keys].sort()).toEqual(Object.keys(DEFAULT_RETOUCH_SETTINGS).sort());
  });
  it('runs deep cleanup as one native pass and exposes dark-mark removal', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Deep acne cleanup' }));
    expect(screen.getByText(/freckles and the marks spots leave are healed. Moles and beauty marks are kept/)).toBeTruthy();
    fireEvent.click(screen.getByText('Blemishes'));
    expect(screen.getByLabelText('Most spots per face').getAttribute('max')).toBe('900');
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
    expect(run).toHaveBeenCalledWith(expect.objectContaining({ settings: expect.objectContaining({
      deepBlemishCleanup: true, removeDarkMarks: true, maxSpots: 512, keepFreckles: false,
    }) }));
  });
  it('applies a preset, then marks a hand change as custom and sends it', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Polished beauty' }));
    fireEvent.click(screen.getByText('Skin'));
    expect(screen.getByText('Skin smoothing: 85%')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Skin warmth'), { target: { value: '-40' } });
    expect(screen.getByText('Custom')).toBeTruthy();
    fireEvent.click(screen.getByText('Skin detection'));
    fireEvent.click(screen.getByLabelText('Main subject only'));
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
    const sent = run.mock.calls[0]?.[0];
    expect(sent?.settings).toEqual(expect.objectContaining({ smoothing: .85, contour: .5, skinWarmth: -0.4, mainSubjectOnly: true }));
    expect(PRESETS.every(([, , values]) => Object.keys(values).every(k => k in DEFAULT_RETOUCH_SETTINGS))).toBe(true);
  });
});

it('adds adjustable pore refinement to face and body cleanup without enabling eye edits', () => {
  const run = vi.fn();
  render(<AutoRetouchSettings disabled={false} onRun={run} />);
  fireEvent.click(screen.getByRole('radio', { name: 'Skin cleanup · refine pores' }));
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
  expect(run).toHaveBeenCalledWith(expect.objectContaining({ scope: 'face_and_body', eyes: false, teeth: false,
    settings: expect.objectContaining({ frequencyHeal: 1, bodyBlemishes: 1, smoothing: .3, texture: .85,
      poreRefine: .3, bodySmoothing: .25, protectEyeArea: true, protectNoseDetail: true }) }));
  fireEvent.click(screen.getByRole('radio', { name: 'Acne only · preserve detail' }));
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
  expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ settings: expect.objectContaining({ smoothing: 0, poreRefine: 0, bodySmoothing: 0 }) }));
});

it('runs deep cleanup with explicit dark-mark removal and retained fine texture', () => {
  const run = vi.fn();
  render(<AutoRetouchSettings disabled={false} onRun={run} />);
  fireEvent.click(screen.getByRole('radio', { name: 'Deep acne cleanup' }));
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
  expect(run).toHaveBeenCalledWith(expect.objectContaining({ settings: expect.objectContaining({
    deepBlemishCleanup: true, removeDarkMarks: true, maxSpots: 512, texture: .85,
  }) }));
});

it('returns the spot limit to the ordinary range when deep cleanup is disabled', () => {
  const run = vi.fn();
  render(<AutoRetouchSettings disabled={false} onRun={run} />);
  fireEvent.click(screen.getByRole('radio', { name: 'Deep acne cleanup' }));
  fireEvent.click(screen.getByLabelText('Deep blemish cleanup'));
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
  expect(run).toHaveBeenCalledWith(expect.objectContaining({ settings: expect.objectContaining({ deepBlemishCleanup: false, maxSpots: 24 }) }));
});

it('offers a professional preset: acne clear first, real texture back last', () => {
  const run = vi.fn();
  render(<AutoRetouchSettings disabled={false} onRun={run} />);
  fireEvent.click(screen.getByRole('radio', { name: 'Professional retouch' }));
  expect(screen.getByText(/Frequency healing rebuilds the tone under each mark first/)).toBeTruthy();
  expect(screen.getByText(/restores the original pores within the selected skin/)).toBeTruthy();
  fireEvent.click(screen.getByText('Blemishes'));
  expect(screen.getByText('Frequency healing: 100%')).toBeTruthy();
  fireEvent.click(screen.getByText('Skin'));
  expect(screen.getByText('Restore skin texture: 80%')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
  expect(run).toHaveBeenCalledWith(expect.objectContaining({ settings: expect.objectContaining({
    deepBlemishCleanup: true, removeDarkMarks: true, frequencyHeal: 1, textureGraft: .8, microDodgeBurn: .55, texture: .9, smoothing: .35,
  }) }));
});

it('keeps frequency healing and the texture graft off unless a preset or a person turns them on', () => {
  expect(DEFAULT_RETOUCH_SETTINGS.frequencyHeal).toBe(0);
  expect(DEFAULT_RETOUCH_SETTINGS.textureGraft).toBe(0);
  for (const [id, , values] of PRESETS) {
    if (id === 'pro' || id === 'acne_only' || id === 'skin_cleanup' || id === 'detailed_skin' || id === 'portrait_finish' || id === 'smooth_finish' || id === 'body_finish') continue;
    expect(values.frequencyHeal ?? 0).toBe(0);
    expect(values.textureGraft ?? 0).toBe(0);
  }
  const run = vi.fn();
  render(<AutoRetouchSettings disabled={false} onRun={run} />);
  expect(screen.queryByText(/Frequency healing rebuilds the tone/)).toBeNull();
  fireEvent.click(screen.getByText('Blemishes'));
  fireEvent.change(screen.getByLabelText('Frequency healing'), { target: { value: '60' } });
  expect(screen.getByText('Custom')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
  expect(run).toHaveBeenCalledWith(expect.objectContaining({ settings: expect.objectContaining({ frequencyHeal: .6, textureGraft: 0 }) }));
});

it('restores the complete finishing scope after acne-only without enabling iris, teeth or shape changes', () => {
  const run = vi.fn();
  render(<AutoRetouchSettings disabled={false} onRun={run} />);
  fireEvent.click(screen.getByRole('radio', { name: 'Acne only · preserve detail' }));
  fireEvent.click(screen.getByRole('radio', { name: 'Portrait skin finish' }));
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
  const options = run.mock.calls[0]?.[0];
  expect(options).toEqual(expect.objectContaining({ scope: 'face_and_body', eyes: true, refine: true, teeth: false,
    settings: expect.objectContaining({ cleanupPasses: 4, maxSpots: 900, darkCircles: .75, toneEvenness: .6,
      redness: .5, protectEyeArea: true, protectNoseDetail: true, texture: .9, bodyTone: .35,
      irisDetail: 0, eyeWhitening: 0, redEye: false, teethWhitening: 0, contour: 0, highlight: 0 }) }));
  fireEvent.click(screen.getByRole('radio', { name: 'Detailed skin repair and review' }));
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
  expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ eyes: false, refine: false,
    settings: expect.objectContaining({ darkCircles: 0, toneEvenness: 0 }) }));
});

it('offers a smoother portrait finish that softens lines and under-eye bags and still leaves eyes, teeth and shape alone', () => {
  const run = vi.fn();
  render(<AutoRetouchSettings disabled={false} onRun={run} />);
  fireEvent.click(screen.getByRole('radio', { name: 'Smooth portrait finish' }));
  expect(screen.getByText(/more of the smoothing a polished headshot carries/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
  expect(run).toHaveBeenCalledWith(expect.objectContaining({ scope: 'face_and_body', eyes: true, refine: true, teeth: false,
    settings: expect.objectContaining({ cleanupPasses: 4, darkCircles: .75, toneEvenness: .6, smoothing: .5, texture: .75,
      foreheadLines: .6, crowsFeet: .6, smileLines: .35, underEyeLines: .9, eyeBags: .75, protectEyeArea: false,
      protectNoseDetail: true, irisDetail: 0, eyeWhitening: 0, redEye: false, teethWhitening: 0, contour: 0, highlight: 0 }) }));
  // The finish it is built on is unchanged by it.
  fireEvent.click(screen.getByRole('radio', { name: 'Portrait skin finish' }));
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
  expect(run).toHaveBeenLastCalledWith(expect.objectContaining({
    settings: expect.objectContaining({ smoothing: .05, microDodgeBurn: 0, lightEvenness: .1, poreRefine: .8, texture: .9, foreheadLines: 0, eyeBags: 0, protectEyeArea: true }) }));
});

it('keeps moles by default and clears stretch marks only when a preset or a person asks', () => {
  expect(DEFAULT_RETOUCH_SETTINGS.keepMoles).toBe(true);
  expect(DEFAULT_RETOUCH_SETTINGS.stretchMarks).toBe(0);
  for (const [id, , values] of PRESETS) {
    expect(values.keepMoles ?? true).toBe(true);
    if (id !== 'body_finish') expect(values.stretchMarks ?? 0).toBe(0);
  }
  const run = vi.fn();
  render(<AutoRetouchSettings disabled={false} onRun={run} />);
  fireEvent.click(screen.getByRole('radio', { name: 'Body skin finish · stretch marks' }));
  expect(screen.getByText(/Moles and beauty marks are kept/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
  expect(run).toHaveBeenCalledWith(expect.objectContaining({ scope: 'face_and_body', teeth: false,
    settings: expect.objectContaining({ stretchMarks: 1, keepMoles: true, removeDarkMarks: true, bodyBlemishes: 1 }) }));
  fireEvent.click(screen.getByText('Blemishes'));
  fireEvent.click(screen.getByLabelText('Keep moles'));
  expect(screen.getByText(/moles and beauty marks can all be removed/)).toBeTruthy();
});

it('shows the preset a saved retouch was made with instead of Custom with nothing chosen', () => {
  // Every preset is recognised from its own values, and a changed value is Custom.
  for (const [id, , values, intensity] of PRESETS) {
    const options = { intensity: intensity ?? 1, blemishes: true, eyes: true, teeth: true, refine: true,
      scope: 'face' as const, settings: { ...DEFAULT_RETOUCH_SETTINGS, ...values } };
    expect(PRESETS.find(([other]) => other === presetOf(options))?.[2]).toEqual(values);
    expect(presetOf({ ...options, settings: { ...options.settings, glow: .37 } })).toBe('custom');
    expect(id).toBeTruthy();
  }
  const body = PRESETS.find(([id]) => id === 'body_finish')!;
  const recipe = { body: JSON.stringify({ studio_portrait_auto_v1: { options: {
    intensity: 1, scope: 'face_and_body', blemishes: true, eyes: true, teeth: false, refine: true,
    settings: { ...DEFAULT_RETOUCH_SETTINGS, ...body[2] } } } }) } as RecipeDto;
  render(<AutoRetouchSettings disabled={false} onRun={vi.fn()} recipe={recipe} />);
  expect((screen.getByRole('radio', { name: body[1] }) as HTMLInputElement).checked).toBe(true);
  expect(screen.queryByText('Custom')).toBeNull();
});
