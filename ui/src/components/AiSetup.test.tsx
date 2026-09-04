import { describe, expect, it } from 'vitest';

import {
  canCheck,
  canContinue,
  filterProviders,
  priceLine,
  providerWarnings,
  setupHeadline,
} from './AiSetup';
import type { AiModelDto, AiProviderDto, AiSetupStatusDto } from '../ipc/types';

const model = (overrides: Partial<AiModelDto> = {}): AiModelDto => ({
  tier: 'balanced',
  model: 'claude-sonnet-4-5',
  inputPerMtokUsd: 3,
  outputPerMtokUsd: 15,
  ...overrides,
});

const provider = (overrides: Partial<AiProviderDto> = {}): AiProviderDto => ({
  id: 'anthropic',
  label: 'Anthropic (Claude)',
  blurb: 'Strong at reading a scene and explaining why.',
  wire: 'anthropic',
  endpoint: 'https://api.anthropic.com',
  endpointEditable: false,
  requiresKey: true,
  keyHint: 'sk-ant-...',
  keysUrl: 'https://console.anthropic.com/settings/keys',
  images: true,
  models: [model()],
  ...overrides,
});

const status = (overrides: Partial<AiSetupStatusDto> = {}): AiSetupStatusDto => ({
  completed: false,
  skipped: false,
  provider: 'anthropic',
  endpoint: null,
  models: ['', '', ''],
  keyedProviders: [],
  schemes: ['http', 'https'],
  offlineStudioMode: false,
  ...overrides,
});

describe('filterProviders', () => {
  const catalogue = [
    provider(),
    provider({ id: 'groq', label: 'Groq', blurb: 'The fastest answers here by a wide margin.' }),
    provider({
      id: 'ollama',
      label: 'Ollama (on this computer)',
      blurb: 'Nothing leaves the machine and nothing is billed.',
    }),
  ];

  it('returns everything when nothing has been typed', () => {
    expect(filterProviders(catalogue, '   ')).toHaveLength(3);
  });

  it('matches a name', () => {
    expect(filterProviders(catalogue, 'groq').map((entry) => entry.id)).toEqual(['groq']);
  });

  // Somebody who knows they want "the fast one" should not have to already know
  // that the answer is called Groq.
  it('matches what a provider is good at, not only what it is called', () => {
    expect(filterProviders(catalogue, 'fastest').map((entry) => entry.id)).toEqual(['groq']);
    expect(filterProviders(catalogue, 'nothing is billed').map((entry) => entry.id)).toEqual([
      'ollama',
    ]);
  });

  it('is case insensitive', () => {
    expect(filterProviders(catalogue, 'OLLAMA')).toHaveLength(1);
  });
});

describe('priceLine', () => {
  it('publishes the price in the unit every vendor publishes it in', () => {
    expect(priceLine(model())).toBe('$3.00 in / $15.00 out per million tokens');
  });

  it('says a local model costs nothing rather than showing two zeroes', () => {
    expect(priceLine(model({ inputPerMtokUsd: 0, outputPerMtokUsd: 0 }))).toBe('no charge');
  });
});

describe('providerWarnings', () => {
  it('says nothing about a provider this build can reach that reads photographs', () => {
    expect(providerWarnings(provider(), ['http', 'https'])).toEqual([]);
  });

  // The whole point of putting the transport's schemes on the wire: a build with
  // no TLS would otherwise collect an Anthropic key it could never use.
  it('warns when this build cannot reach the address at all', () => {
    const notes = providerWarnings(provider(), ['http']);
    expect(notes.join(' ')).toContain('cannot reach https');
  });

  it('warns when the models cannot see a photograph', () => {
    const notes = providerWarnings(provider({ id: 'deepseek', images: false }), ['https']);
    expect(notes.join(' ')).toContain('do not look at photographs');
  });

  it('says a local server needs no key and does need to be running', () => {
    const notes = providerWarnings(
      provider({ id: 'ollama', requiresKey: false, endpoint: 'http://127.0.0.1:11434' }),
      ['http', 'https'],
    );
    expect(notes.join(' ')).toContain('No key is needed');
  });

  it('has nothing to say about a provider nobody has chosen', () => {
    expect(providerWarnings(null, ['http'])).toEqual([]);
  });
});

describe('canContinue', () => {
  it('refuses before a provider has been chosen', () => {
    expect(canContinue(null, 'sk-ant-x', [])).toBe(false);
  });

  it('needs a key for a provider that needs a key', () => {
    expect(canContinue(provider(), '   ', [])).toBe(false);
    expect(canContinue(provider(), 'sk-ant-x', [])).toBe(true);
  });

  // Switching back to a provider set up last month must not ask for the key
  // again: it is already in the operating system's credential store, and nothing
  // in AURA can read it back out to prefill a field with.
  it('is satisfied by a key this machine already has', () => {
    expect(canContinue(provider(), '', ['anthropic'])).toBe(true);
  });

  it('needs nothing at all for a local server', () => {
    expect(canContinue(provider({ requiresKey: false }), '', [])).toBe(true);
  });
});

describe('setupHeadline', () => {
  it('leads with the fact that the product works without any of this', () => {
    expect(setupHeadline(status())).toContain('without any of this');
  });

  it('remembers that somebody declined rather than pretending they never saw it', () => {
    expect(setupHeadline(status({ completed: true, skipped: true }))).toContain('skipped');
  });

  it('offers a change rather than an introduction once one is configured', () => {
    expect(setupHeadline(status({ completed: true }))).toContain('Change which AI provider');
  });
});

describe('canCheck', () => {
  it('will not probe a provider the gateway is not pointed at yet', () => {
    expect(canCheck(provider(), status({ provider: 'groq' }), ['anthropic'])).toBe(false);
  });

  it('probes a saved provider whose key is stored', () => {
    expect(canCheck(provider(), status({ provider: 'anthropic' }), ['anthropic'])).toBe(true);
  });

  it('will not probe a saved provider that has no key yet', () => {
    expect(canCheck(provider(), status({ provider: 'anthropic' }), [])).toBe(false);
  });

  // The most useful check on the screen: a local server that is not running.
  it('probes a saved local server that needs no key', () => {
    const local = provider({ id: 'ollama', requiresKey: false });
    expect(canCheck(local, status({ provider: 'ollama' }), [])).toBe(true);
  });
});
