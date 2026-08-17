import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';

import { DevelopPanel } from './DevelopPanel';
import type { HistoryDto, RecipeDto, RenderCapsDto, RenderDto } from '../../ipc/types';

function recipe(overrides: Partial<RecipeDto> = {}): RecipeDto {
  return {
    photoId: 'pht_1',
    body: '{}',
    recipeHash: 'a'.repeat(64),
    schema: 1,
    engine: 'aura-render/1.0.0',
    source: 'ai',
    confidence: 0.94,
    decisionId: 'dcn_1',
    userEditedFields: ['global.exposure'],
    params: [
      { path: 'global.exposure', value: 0.31, protected: true, stage: 'exposure' },
      { path: 'global.contrast', value: 11, protected: false, stage: 'contrast' },
      { path: 'global.shadows', value: 22, protected: false, stage: 'tone' },
    ],
    ...overrides,
  };
}

function rendered(overrides: Partial<RenderDto> = {}): RenderDto {
  return {
    width: 4,
    height: 2,
    rgbBase64: 'AAAA',
    colourSpace: 'srgb',
    icc: 'sRGB IEC61966-2.1',
    renderHash: 'b'.repeat(64),
    backend: 'cpu',
    stagesRun: ['exposure', 'output_transform'],
    notes: [],
    ms: 180,
    ...overrides,
  };
}

function history(overrides: Partial<HistoryDto> = {}): HistoryDto {
  return {
    photoId: 'pht_1',
    entries: [
      { seq: 1, atMs: 0, source: 'ai', changed: ['global.exposure'], label: 'AI: exposure' },
      { seq: 2, atMs: 1, source: 'user', changed: ['global.exposure'], label: 'Exposure' },
    ],
    snapshots: [],
    canUndo: true,
    canRedo: false,
    hasAiSuggestion: true,
    ...overrides,
  };
}

const caps: RenderCapsDto = {
  backend: 'cpu',
  maxTexture: 9459,
  precisionBits: 24,
  maxWorkingBytes: 1073741824,
  engine: 'aura-render/1.0.0+profiles.1',
  degradation: 'AURA-RENDER-8001',
  degradationMessage:
    'AURA is developing photographs using the processor rather than the graphics card. The result is the same; it takes longer.',
};

afterEach(cleanup);

describe('DevelopPanel', () => {
  it('marks a parameter the photographer set and says why', () => {
    render(
      <DevelopPanel
        recipe={recipe()}
        render={rendered()}
        history={history()}
        caps={caps}
        onSetParam={() => {}}
        onHistory={() => {}}
      />,
    );
    expect(screen.getByLabelText('You set this. AURA will not change it.')).toBeTruthy();
  });

  it('says which engine drew the photograph and what it is running without', () => {
    render(
      <DevelopPanel
        recipe={recipe()}
        render={rendered()}
        history={history()}
        caps={caps}
        onSetParam={() => {}}
        onHistory={() => {}}
      />,
    );
    expect(screen.getByTestId('develop-engine').textContent).toContain('processor');
    expect(screen.getByRole('status').textContent).toContain('graphics card');
  });

  it('renders a caveat for every stage that could not run', () => {
    render(
      <DevelopPanel
        recipe={recipe()}
        render={rendered({
          notes: [
            { stage: 'masks', reason: 'mask_generator_absent', detail: 'm1', isCaveat: true },
            { stage: 'dehaze', reason: 'not_requested', detail: null, isCaveat: false },
          ],
        })}
        history={history()}
        caps={caps}
        onSetParam={() => {}}
        onHistory={() => {}}
      />,
    );
    const list = screen.getByLabelText('What this render left out');
    expect(list.textContent).toContain('cannot work out that mask yet');
    // A stage nobody asked for is not a caveat and must not be listed: reporting it would
    // bury the ones that matter.
    expect(list.textContent).not.toContain('dehaze');
  });

  it('sends a parameter change with its dotted path', () => {
    const onSetParam = vi.fn();
    render(
      <DevelopPanel
        recipe={recipe()}
        render={rendered()}
        history={history()}
        caps={caps}
        onSetParam={onSetParam}
        onHistory={() => {}}
      />,
    );
    const control = screen.getByLabelText(/Contrast/);
    fireEvent.change(control, { target: { value: '25' } });
    expect(onSetParam).toHaveBeenCalledWith('global.contrast', 25);
  });

  it('offers reset to the AI suggestion only when something has suggested one', () => {
    const { rerender } = render(
      <DevelopPanel
        recipe={recipe()}
        render={rendered()}
        history={history({ hasAiSuggestion: false })}
        caps={caps}
        onSetParam={() => {}}
        onHistory={() => {}}
      />,
    );
    expect((screen.getByRole('button', { name: 'Reset to AI suggestion' }) as HTMLButtonElement).disabled).toBe(true);

    rerender(
      <DevelopPanel
        recipe={recipe()}
        render={rendered()}
        history={history({ hasAiSuggestion: true })}
        caps={caps}
        onSetParam={() => {}}
        onHistory={() => {}}
      />,
    );
    expect((screen.getByRole('button', { name: 'Reset to AI suggestion' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('says the original is never modified', () => {
    render(
      <DevelopPanel
        recipe={recipe()}
        render={rendered()}
        history={history()}
        caps={caps}
        onSetParam={() => {}}
        onHistory={() => {}}
      />,
    );
    expect(screen.getByText(/never changed/i)).toBeTruthy();
  });

  it('shows nothing that reads as a delivery decision', () => {
    // Phase 12 decides galleries and phase 30 delivers them. A develop panel that said
    // "keep" or "export" would be a second answer to a question another phase owns.
    const { container } = render(
      <DevelopPanel
        recipe={recipe()}
        render={rendered()}
        history={history()}
        caps={caps}
        onSetParam={() => {}}
        onHistory={() => {}}
      />,
    );
    const text = container.textContent?.toLowerCase() ?? '';
    for (const forbidden of ['keep', 'reject', 'deliver', 'export']) {
      expect(text).not.toContain(forbidden);
    }
  });
});
