import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DevelopWorkspace, provenance } from './DevelopWorkspace';

// `vi.mock` is hoisted above every `const` in the module, so the spies and the fixtures it
// closes over have to be hoisted with it. `vi.hoisted` is the supported way to say that.
const fixtures = vi.hoisted(() => {
  const RECIPE = {
    photoId: 'pho_1',
    body: '{}',
    recipeHash: 'abc',
    schema: 1,
    engine: 'aura-render/1',
    source: 'ai',
    confidence: 0.7,
    decisionId: null,
    userEditedFields: [] as string[],
    params: [],
  };
  const HISTORY = {
    photoId: 'pho_1',
    entries: [],
    snapshots: [] as string[],
    canUndo: false,
    canRedo: false,
    hasAiSuggestion: true,
  };
  const MATRIX = {
    allowed: [true, false, true, true, true],
    operators: ['flyaway', 'teeth', 'eyes', 'clothing', 'glare'],
    clothing: [true, false, false, false, false],
    clothingKinds: ['lint', 'strap', 'crease', 'stain', 'label'],
    clothingOptIn: [false, true, true, true, true],
    borrowing: false,
  };
  return {
    RECIPE,
    HISTORY,
    MATRIX,
    renderImage: vi.fn(),
    renderCaps: vi.fn().mockResolvedValue({
      backend: 'processor',
      maxTexture: 4096,
      precisionBits: 32,
      maxWorkingBytes: 1,
      engine: 'aura-render/1',
      degradation: null,
      degradationMessage: null,
    }),
    setMicroMatrix: vi.fn().mockResolvedValue({}),
    setFraming: vi.fn().mockResolvedValue({}),
  };
});

vi.mock('../../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (error: unknown) => ({ code: 'AURA-TEST-0001', message: String(error) }),
  develop: {
    imageRecipe: vi.fn(() => Promise.resolve(fixtures.RECIPE)),
    imageHistory: vi.fn(() => Promise.resolve(fixtures.HISTORY)),
    renderImage: (...args: unknown[]) => {
      fixtures.renderImage(...args);
      return Promise.resolve({
        width: 8,
        height: 8,
        rgbBase64: '',
        colourSpace: 'srgb',
        icc: '',
        renderHash: 'r',
        backend: 'processor',
        stagesRun: [],
        notes: [],
        ms: 12,
      });
    },
    renderCaps: fixtures.renderCaps,
    setParam: vi.fn().mockResolvedValue({}),
    historyStep: vi.fn().mockResolvedValue({}),
  },
  tone: {
    toneStatus: vi.fn().mockResolvedValue(null),
    imageTone: vi.fn().mockResolvedValue(null),
    acceptTone: vi.fn().mockResolvedValue({}),
    setToneOverride: vi.fn().mockResolvedValue({}),
  },
  colour: {
    colourStatus: vi.fn().mockResolvedValue(null),
    imageColour: vi.fn().mockResolvedValue(null),
    acceptColour: vi.fn().mockResolvedValue({}),
    selectColourVariant: vi.fn().mockResolvedValue({}),
    setColourOverride: vi.fn().mockResolvedValue({}),
  },
  local: {
    localStatus: vi.fn().mockResolvedValue(null),
    imageLocal: vi.fn().mockResolvedValue(null),
    acceptLocal: vi.fn().mockResolvedValue({}),
    setLocalStrength: vi.fn().mockResolvedValue({}),
  },
  retouch: {
    retouchStatus: vi.fn().mockResolvedValue(null),
    imageRetouch: vi.fn().mockResolvedValue(null),
    acceptRetouch: vi.fn().mockResolvedValue({}),
    setRetouch: vi.fn().mockResolvedValue({}),
    setProtection: vi.fn().mockResolvedValue({}),
  },
  micro: {
    microStatus: vi.fn().mockResolvedValue(null),
    imageMicro: vi.fn().mockResolvedValue(null),
    microMatrix: vi.fn(() => Promise.resolve(fixtures.MATRIX)),
    acceptMicro: vi.fn().mockResolvedValue({}),
    setMicroMatrix: fixtures.setMicroMatrix,
  },
  restore: {
    restoreStatus: vi.fn().mockResolvedValue(null),
    imageRestore: vi.fn().mockResolvedValue(null),
    acceptRestore: vi.fn().mockResolvedValue({}),
    setRestoreOverride: vi.fn().mockResolvedValue({}),
  },
  geometry: {
    geometryStatus: vi.fn().mockResolvedValue(null),
    imageGeometry: vi.fn().mockResolvedValue(null),
    acceptGeometry: vi.fn().mockResolvedValue({}),
    setFraming: fixtures.setFraming,
  },
}));

async function mount(projectId: string | null, photoId: string | null) {
  const result = render(
    <DevelopWorkspace projectId={projectId} photoId={photoId} onError={() => undefined} />,
  );
  // Two awaits: the container fetches the recipe, then the plans, then the render.
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return result;
}

describe('DevelopWorkspace', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('says what to do rather than rendering empty controls with no wedding open', async () => {
    await mount(null, null);
    expect(screen.getByText(/Open a wedding/)).toBeDefined();
  });

  it('says what to do rather than rendering empty controls with no photograph selected', async () => {
    await mount('prj_1', null);
    expect(screen.getByText(/Select a photograph/)).toBeDefined();
  });

  it('offers every stage of the edit', async () => {
    await mount('prj_1', 'pho_1');
    for (const section of [
      'Basic',
      'Tone and colour',
      'Curve',
      'Colour bands',
      'Regions',
      'Local light',
      'Retouch',
      'Small fixes',
      'Restoration',
      'Geometry',
      'All parameters',
    ]) {
      expect(screen.getByRole('button', { name: section })).toBeDefined();
    }
  });

  it('renders the proxy once rather than once per panel', async () => {
    // Eleven per-panel containers would each have asked for their own. On the processor path
    // that is about 210 ms a call and this build links no GPU backend.
    await mount('prj_1', 'pho_1');
    expect(fixtures.renderImage).toHaveBeenCalledTimes(1);
  });

  it('says whose the edit is, and how much of it a person owns', async () => {
    await mount('prj_1', 'pho_1');
    expect(screen.getByText(/AURA suggested this edit/)).toBeDefined();
    expect(screen.getByText(/Nothing on it has been set by hand/)).toBeDefined();
  });

  it('does not ask the machine what its renderer can do until that tab is opened', async () => {
    // `render_caps` probes the hardware. Probing it to draw a tab nobody opened is work for a
    // panel that may never be looked at.
    const { getByRole } = await mount('prj_1', 'pho_1');
    expect(fixtures.renderCaps).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.click(getByRole('button', { name: 'All parameters' }));
      await Promise.resolve();
    });
    expect(fixtures.renderCaps).toHaveBeenCalledTimes(1);
  });
});

describe('provenance', () => {
  function recipe(source: string, owned: string[] = []) {
    return { ...fixtures.RECIPE, source, userEditedFields: owned };
  }

  it('never renders a slug into the sentence', () => {
    // The five `RecipeDto.source` values are slugs. The first version of the header rendered
    // them straight through, so a photograph nobody had edited read "This edit came from
    // default" - which is both a slug and the opposite of what it means.
    //
    // The assertion is against slug *shapes* rather than against the strings themselves,
    // because `preset` is an ordinary English word and the sentence for that source contains
    // it legitimately. A test that forbade the word outright could not be met by a correct
    // implementation, which is the same trap the exit reports record for phases 19, 21, 22, 25
    // and 29 - a threshold nothing can meet is a bug in the threshold.
    for (const source of ['ai', 'user', 'qc', 'preset', 'default', 'something_new']) {
      const sentence = provenance(recipe(source));
      expect(sentence).not.toMatch(/\b(ai|qc|default|something_new)\b/);
      expect(sentence).not.toMatch(/_/);
      expect(sentence.endsWith('.')).toBe(true);
      // Prose, not a fragment: a capital at the front and more than a label's worth of it.
      expect(sentence[0]).toBe(sentence[0]?.toUpperCase());
      expect(sentence.length).toBeGreaterThan(30);
    }
  });

  it('says the camera made it rather than naming a source, when nothing has edited it', () => {
    expect(provenance(recipe('default'))).toContain("camera's own starting point");
  });

  it('counts what a person owns, and agrees with itself about number', () => {
    expect(provenance(recipe('ai'))).toContain('Nothing on it has been set by hand.');
    expect(provenance(recipe('ai', ['exposure']))).toContain('1 of its settings is yours');
    expect(provenance(recipe('ai', ['exposure', 'tint']))).toContain('2 of its settings are yours');
  });

  it('promises the protection every source shares', () => {
    for (const source of ['ai', 'user', 'qc', 'preset', 'default']) {
      expect(provenance(recipe(source, ['exposure']))).toContain('will not be overwritten');
    }
  });
});
