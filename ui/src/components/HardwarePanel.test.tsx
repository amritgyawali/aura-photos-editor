import { describe, expect, it } from 'vitest';

import type { ModelStatusDto } from '../ipc/types';
import { PROVIDER_CHOICES, modelSummary, providerLabel } from './HardwarePanel';

function model(overrides: Partial<ModelStatusDto> = {}): ModelStatusDto {
  return {
    name: 'aura_tiny_embedding',
    version: '1.0.0',
    task: 'embedding',
    activeVersion: null,
    pendingVersion: null,
    rejectedVersions: [],
    modelCard: 'docs/model-cards/aura_tiny_embedding.md',
    workingSetMb: 8,
    fileCount: 3,
    int8Forbidden: false,
    ...overrides,
  };
}

describe('HardwarePanel', () => {
  it('names every provider in a photographer’s words', () => {
    for (const ep of PROVIDER_CHOICES) {
      expect(providerLabel(ep)).not.toBe(ep);
    }
    expect(providerLabel('cpu')).toBe('Processor');
  });

  it('shows an unknown provider verbatim rather than swallowing it', () => {
    expect(providerLabel('rocm')).toBe('rocm');
  });

  it('says a model is pinned but unused before anything runs', () => {
    expect(modelSummary(model())).toContain('1.0.0 pinned');
  });

  it('says a newly installed version is not yet proved', () => {
    expect(modelSummary(model({ activeVersion: '1.0.0', pendingVersion: '1.1.0' }))).toContain(
      'not yet proved',
    );
  });

  it('names the version that rolled back, because support starts there', () => {
    const summary = modelSummary(
      model({ activeVersion: '1.0.0', rejectedVersions: ['1.1.0'] }),
    );
    expect(summary).toContain('1.0.0 in use');
    expect(summary).toContain('1.1.0 rolled back');
  });
});
