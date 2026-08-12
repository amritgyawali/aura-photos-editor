import { describe, expect, it } from 'vitest';

import {
  callSummary,
  disabledReasons,
  money,
  providerLabel,
  spendSentence,
} from './AiKeysPanel';
import type { CloudCallDto, CloudSpendDto, CloudStatusDto } from '../ipc/types';

const status = (overrides: Partial<CloudStatusDto> = {}): CloudStatusDto => ({
  provider: 'anthropic',
  endpoint: 'https://api.anthropic.com',
  keyPresent: true,
  keyFingerprint: 'sk-a...9xQz',
  keyStore: 'dpapi',
  offlineStudioMode: false,
  projectEnabled: true,
  blurFaces: false,
  transport: 'http',
  breakerReason: null,
  tierModels: ['claude-haiku-4-5', 'claude-sonnet-4-5', 'claude-opus-4-5'],
  ...overrides,
});

const spend = (overrides: Partial<CloudSpendDto> = {}): CloudSpendDto => ({
  capUsd: 5,
  spentUsd: 0.42,
  monthCapUsd: 30,
  monthSpentUsd: 2.1,
  calls: 21,
  downgrades: 0,
  fallbacks: 0,
  cacheHitRate: 0.72,
  stopped: false,
  ...overrides,
});

const call = (overrides: Partial<CloudCallDto> = {}): CloudCallDto => ({
  id: 'call-1',
  task: 'segment_naming',
  taskVersion: 1,
  model: 'claude-sonnet-4-5',
  source: 'cloud',
  fallbackReason: null,
  tokensIn: 2871,
  tokensOut: 118,
  costUsd: 0.0106,
  latencyMs: 1840,
  status: 'ok',
  retryCount: 0,
  promptHash: 'abc123',
  confidence: 0.86,
  decisionRef: 'seg-1',
  ...overrides,
});

describe('providerLabel', () => {
  it('names a self-hosted server in the words a photographer would use', () => {
    expect(providerLabel('compat')).toBe('My own server (OpenAI-compatible)');
    expect(providerLabel('anthropic')).toBe('Anthropic (Claude)');
  });

  it('falls back to the identifier rather than showing nothing', () => {
    expect(providerLabel('something-new')).toBe('something-new');
  });
});

describe('spendSentence', () => {
  it('is the sentence section 6.3 asks for', () => {
    expect(spendSentence(spend())).toBe('This wedding has used $0.42 of your $5.00 cap.');
  });

  it('says what happens next when the cap is reached', () => {
    const sentence = spendSentence(spend({ spentUsd: 5, stopped: true }));
    expect(sentence).toContain('limit has been reached');
    expect(sentence).toContain("AURA's own models");
  });

  it('does not pretend to know a figure it has not been given', () => {
    expect(spendSentence(null)).toContain('No AI spending recorded');
  });
});

describe('disabledReasons', () => {
  it('is empty when everything is on', () => {
    expect(disabledReasons(status())).toEqual([]);
  });

  it('lists every reason a call would not be made, in the order they are checked', () => {
    const reasons = disabledReasons(
      status({
        offlineStudioMode: true,
        projectEnabled: false,
        keyPresent: false,
        breakerReason: 'the provider stopped answering',
      }),
    );
    expect(reasons).toHaveLength(4);
    expect(reasons[0]).toContain('Offline studio mode');
    expect(reasons[1]).toContain('off for this job');
    expect(reasons[2]).toContain('No key');
    expect(reasons[3]).toContain('the provider stopped answering');
  });

  it('says so when the build itself cannot reach the network', () => {
    expect(disabledReasons(status({ transport: 'offline' })).join(' ')).toContain(
      'every decision is local',
    );
  });
});

describe('callSummary', () => {
  it('shows the model, the tokens and the cost for a billed call', () => {
    const summary = callSummary(call());
    expect(summary).toContain('claude-sonnet-4-5');
    expect(summary).toContain('2871+118 tokens');
    expect(summary).toContain('$0.01');
    expect(summary).not.toContain('repaired');
  });

  it('says when a repair was needed', () => {
    expect(callSummary(call({ retryCount: 1 }))).toContain('repaired once');
  });

  it('says why a decision was local rather than hiding it', () => {
    const summary = callSummary(
      call({ source: 'local_fallback', fallbackReason: 'budget_reached' }),
    );
    expect(summary).toContain("AURA's own models");
    expect(summary).toContain('budget_reached');
  });

  it('makes clear that a cache hit cost nothing', () => {
    expect(callSummary(call({ source: 'cache' }))).toContain('no charge');
  });
});

describe('money', () => {
  it('always shows two decimal places', () => {
    expect(money(0.4)).toBe('$0.40');
    expect(money(12)).toBe('$12.00');
  });
});
