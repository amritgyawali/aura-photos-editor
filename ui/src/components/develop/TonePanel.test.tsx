import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import { CurveEditor, isMonotone } from './CurveEditor';
import { HslPanel } from './HslPanel';
import { TonePanel } from './TonePanel';
import type { ColourDto, ColourStatusDto, HslShiftDto, RecipeDto } from '../../ipc/types';

function recipe(overrides: Partial<RecipeDto> = {}): RecipeDto {
  return {
    photoId: 'pht_1',
    body: '{}',
    recipeHash: 'a'.repeat(64),
    schema: 1,
    engine: 'aura-render/1.0.0',
    source: 'ai',
    confidence: 0.9,
    decisionId: null,
    userEditedFields: [],
    params: [],
    ...overrides,
  };
}

function colour(overrides: Partial<ColourDto> = {}): ColourDto {
  return {
    photoId: 'pht_1',
    contrast: 14,
    highlights: -12,
    shadows: 10,
    whites: 4,
    blacks: -6,
    vibrance: 6,
    saturation: 0,
    curve: [
      { x: 0, y: 0 },
      { x: 64, y: 54 },
      { x: 192, y: 202 },
      { x: 255, y: 255 },
    ],
    hsl: [
      { band: 'red', h: 0, s: 0, l: 0 },
      { band: 'orange', h: 0, s: 0, l: 0 },
      { band: 'yellow', h: 0, s: 0, l: 0 },
      { band: 'green', h: 6, s: -8, l: 0 },
      { band: 'aqua', h: 0, s: 0, l: 0 },
      { band: 'blue', h: 0, s: 0, l: 0 },
      { band: 'purple', h: 0, s: 0, l: 0 },
      { band: 'magenta', h: 0, s: 0, l: 0 },
    ],
    bands: [
      {
        band: 'greenery',
        area: 0.42,
        hueDeg: 98,
        saturation: 0.55,
        luma: 0.3,
        confidence: 0.86,
      },
      { band: 'skin', area: 0.09, hueDeg: 30, saturation: 0.32, luma: 0.45, confidence: 1 },
    ],
    skinGuard: {
      maskArea: 0.09,
      maxHueShiftDeg: 0.4,
      maxChromaChange: 0.01,
      attenuation: 1,
      resolves: 0,
      measured: true,
      withinCeilings: true,
    },
    clippingBefore: 0.001,
    clippingAfter: 0.001,
    clippingAdded: 0,
    alternatives: [
      {
        kind: 'punchier',
        contrast: 26,
        highlights: -12,
        shadows: 10,
        whites: 4,
        blacks: -14,
        vibrance: 6,
        saturation: 0,
        curve: [
          { x: 0, y: 0 },
          { x: 64, y: 44 },
          { x: 192, y: 212 },
          { x: 255, y: 255 },
        ],
        hsl: [],
        skinGuard: {
          maskArea: 0.09,
          maxHueShiftDeg: 0.6,
          maxChromaChange: 0.02,
          attenuation: 1,
          resolves: 0,
          measured: true,
          withinCeilings: true,
        },
      },
    ],
    reasons: [
      {
        code: 'greenery_tamed',
        text: 'the foliage was recorded at 98 degrees and this kind of photograph wants it near 128',
        weight: 0,
        evidence: null,
      },
    ],
    confidence: 0.78,
    subtlety: 0.19,
    scene: 'couple_portrait',
    skinMeasured: true,
    userEdited: false,
    reviewed: false,
    needsReview: false,
    modelVer: 100,
    analysisVer: 1,
    intentVer: 1,
    ...overrides,
  };
}

function status(overrides: Partial<ColourStatusDto> = {}): ColourStatusDto {
  return {
    photos: 100,
    decided: 100,
    coverage: 1,
    skinMeasured: 62,
    skinGuardTriggered: 3,
    clipGuardResolved: 1,
    subtletyCapped: 0,
    needsReview: 2,
    userEdited: 0,
    meanContrast: 12,
    meanShadowLift: 9,
    meanSubtlety: 0.18,
    worstSkinHueShift: 1.2,
    guaranteeHeld: true,
    untargetedScenes: [],
    modelVer: 100,
    analysisVer: 1,
    intentVer: 1,
    ...overrides,
  };
}

afterEach(cleanup);

describe('TonePanel', () => {
  it('renders an ungraded frame as ungraded rather than as zeroes', () => {
    // The rule that stops a gap in coverage looking like a decision.
    render(
      <TonePanel
        colour={null}
        recipe={recipe()}
        status={null}
        onOverride={vi.fn()}
        onAccept={vi.fn()}
        onSelectVariant={vi.fn()}
        onResetToAi={vi.fn()}
      />,
    );
    expect(screen.getByTestId('tone-ungraded')).toBeTruthy();
    expect(screen.queryByLabelText(/Contrast/)).toBeNull();
  });

  it('states the skin guarantee as a measurement', () => {
    render(
      <TonePanel
        colour={colour()}
        recipe={recipe()}
        status={status()}
        onOverride={vi.fn()}
        onAccept={vi.fn()}
        onSelectVariant={vi.fn()}
        onResetToAi={vi.fn()}
      />,
    );
    const guard = screen.getByTestId('tone-skin-guard').textContent ?? '';
    expect(guard).toContain('0.4 degrees');
    expect(guard).toContain('limit is 2 degrees');
  });

  it('says the guarantee did not apply when there was nobody in the frame', () => {
    // Not a perfect score. The two sentences are different on purpose.
    render(
      <TonePanel
        colour={colour({
          skinMeasured: false,
          skinGuard: {
            maskArea: 0,
            maxHueShiftDeg: 0,
            maxChromaChange: 0,
            attenuation: 1,
            resolves: 0,
            measured: false,
            withinCeilings: true,
          },
        })}
        recipe={recipe()}
        status={status()}
        onOverride={vi.fn()}
        onAccept={vi.fn()}
        onSelectVariant={vi.fn()}
        onResetToAi={vi.fn()}
      />,
    );
    const guard = screen.getByTestId('tone-skin-guard').textContent ?? '';
    expect(guard).toContain('nobody in this frame');
    expect(guard).not.toContain('0.0 degrees');
  });

  it('carries the protected dot on a value a person set', () => {
    render(
      <TonePanel
        colour={colour({ userEdited: true })}
        recipe={recipe({ userEditedFields: ['global.contrast'] })}
        status={status()}
        onOverride={vi.fn()}
        onAccept={vi.fn()}
        onSelectVariant={vi.fn()}
        onResetToAi={vi.fn()}
      />,
    );
    expect(screen.getAllByTitle('You set this. AURA will not change it.').length).toBe(1);
  });

  it('offers the alternatives and promotes the one that was clicked', () => {
    const onSelectVariant = vi.fn();
    render(
      <TonePanel
        colour={colour()}
        recipe={recipe()}
        status={status()}
        onOverride={vi.fn()}
        onAccept={vi.fn()}
        onSelectVariant={onSelectVariant}
        onResetToAi={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText('Punchier'));
    expect(onSelectVariant).toHaveBeenCalledWith('punchier');
  });

  it('has no control that could reach a white balance', () => {
    // Section 2.2's boundary, checked in the place a photographer would look for it.
    const { container } = render(
      <TonePanel
        colour={colour()}
        recipe={recipe()}
        status={status()}
        onOverride={vi.fn()}
        onAccept={vi.fn()}
        onSelectVariant={vi.fn()}
        onResetToAi={vi.fn()}
      />,
    );
    const text = container.textContent ?? '';
    expect(text).not.toContain('Temperature');
    expect(text).not.toContain('Tint');
    expect(text).not.toContain('Exposure');
  });

  it('says how much of the wedding had its guarantee checked', () => {
    render(
      <TonePanel
        colour={colour()}
        recipe={recipe()}
        status={status({ skinMeasured: 0 })}
        onOverride={vi.fn()}
        onAccept={vi.fn()}
        onSelectVariant={vi.fn()}
        onResetToAi={vi.fn()}
      />,
    );
    expect(screen.getByTestId('tone-coverage').textContent).toContain(
      'has not been checked on any of them',
    );
  });
});

describe('CurveEditor', () => {
  it('draws the stored control points', () => {
    render(<CurveEditor colour={colour()} onSelectVariant={vi.fn()} />);
    const path = screen.getByTestId('curve-path').getAttribute('d') ?? '';
    expect(path.startsWith('M 0.00')).toBe(true);
    expect(screen.getByTestId('curve-fitted-note')).toBeTruthy();
  });

  it('says plainly when a frame needed no curve', () => {
    render(
      <CurveEditor
        colour={colour({
          curve: [
            { x: 0, y: 0 },
            { x: 255, y: 255 },
          ],
        })}
        onSelectVariant={vi.fn()}
      />,
    );
    expect(screen.getByTestId('curve-identity-note')).toBeTruthy();
  });

  it('refuses a set of points that goes backwards', () => {
    // The same three rules `ToneCurve::new` enforces, checked here so a photographer dragging
    // a point finds out at the moment they do it.
    expect(
      isMonotone([
        { x: 0, y: 0 },
        { x: 64, y: 54 },
        { x: 255, y: 255 },
      ]),
    ).toBe(true);
    expect(
      isMonotone([
        { x: 0, y: 0 },
        { x: 128, y: 200 },
        { x: 64, y: 20 },
        { x: 255, y: 255 },
      ]),
    ).toBe(false);
    expect(
      isMonotone([
        { x: 0, y: 0 },
        { x: 128, y: 200 },
        { x: 200, y: 100 },
        { x: 255, y: 255 },
      ]),
    ).toBe(false);
    expect(
      isMonotone([
        { x: 10, y: 0 },
        { x: 255, y: 255 },
      ]),
    ).toBe(false);
  });

  it('previews an alternative without promoting it', () => {
    const onSelectVariant = vi.fn();
    render(
      <CurveEditor colour={colour()} previewKind="punchier" onSelectVariant={onSelectVariant} />,
    );
    expect(onSelectVariant).not.toHaveBeenCalled();
    expect(screen.getByTestId('curve-path')).toBeTruthy();
  });
});

describe('HslPanel', () => {
  it('shows what was found and how sure it was', () => {
    render(<HslPanel colour={colour()} recipe={recipe()} onHslChange={vi.fn()} />);
    const greenery = screen.getByTestId('hsl-content-greenery').textContent ?? '';
    expect(greenery).toContain('42%');
    expect(greenery).toContain('86% sure');
  });

  it('shows skin and gives it no slider', () => {
    render(<HslPanel colour={colour()} recipe={recipe()} onHslChange={vi.fn()} />);
    expect(screen.getByTestId('hsl-skin-area').textContent).toContain('never adjusted');
    expect(screen.queryByTestId('hsl-band-skin')).toBeNull();
  });

  it('says an unconfident band was left alone rather than hiding it', () => {
    render(
      <HslPanel
        colour={colour({
          bands: [
            {
              band: 'decor',
              area: 0.05,
              hueDeg: 300,
              saturation: 0.6,
              luma: 0.4,
              confidence: 0.2,
            },
          ],
        })}
        recipe={recipe()}
        onHslChange={vi.fn()}
      />,
    );
    expect(screen.getByTestId('hsl-content-decor').textContent).toContain(
      'not confidently enough to adjust',
    );
  });

  it('sends all eight bands when one is changed', () => {
    // Whole-or-nothing, matching the override contract: eight bands are one decision.
    const onHslChange = vi.fn();
    render(<HslPanel colour={colour()} recipe={recipe()} onHslChange={onHslChange} />);
    fireEvent.change(screen.getByLabelText('green s'), { target: { value: '-12' } });
    expect(onHslChange).toHaveBeenCalledTimes(1);
    const sent = onHslChange.mock.calls[0]?.[0] as HslShiftDto[];
    expect(sent).toHaveLength(8);
    expect(sent.find((entry) => entry.band === 'green')?.s).toBe(-12);
  });

  it('always carries the inferred-content caveat', () => {
    render(<HslPanel colour={colour()} recipe={recipe()} onHslChange={vi.fn()} />);
    expect(screen.getByTestId('hsl-caveat').textContent).toContain('rather than outlining');
  });
});
