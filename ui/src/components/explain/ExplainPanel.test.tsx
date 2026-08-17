import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type {
  AlternativeDto,
  ExplainPanelDto,
  ExplainTabDto,
  LedgerDecisionDto,
  LedgerReasonDto,
} from '../../ipc/types';

import { AlternativeCompare, comparisonSentence, scoreCell } from './AlternativeCompare';
import { boxStyle, cropDescription, withContext } from './EvidenceCrop';
import {
  calibrationCaveat,
  DEFAULT_REASONS,
  ExplainPanel,
  orderedReasons,
  percent,
  strongestWeight,
  tabPlaceholder,
} from './ExplainPanel';
import { barWidth, evidenceLabel, isCaveat, severityLabel } from './ReasonRow';

vi.mock('../../ipc/client', async () => {
  const actual = await vi.importActual<typeof import('../../ipc/client')>('../../ipc/client');
  return {
    ...actual,
    explain: { explainImage: vi.fn() },
  };
});

const { explain } = await import('../../ipc/client');

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const photoA = 'pht_00000000-0000-7000-8000-000000000001';
const photoB = 'pht_00000000-0000-7000-8000-000000000002';

const reason = (over: Partial<LedgerReasonDto> = {}): LedgerReasonDto => ({
  code: 'moment_winner',
  text: 'the strongest frame of this moment',
  weight: 0.6,
  severity: 'credit',
  domain: 'selection',
  evidenceKind: 'none',
  crop: null,
  frames: [],
  params: [],
  ...over,
});

const tab = (over: Partial<ExplainTabDto> = {}): ExplainTabDto => ({
  id: 'selection',
  title: 'Selection',
  available: true,
  unavailableReason: null,
  reasons: [reason()],
  score: 0.81,
  confidence: 0.74,
  ...over,
});

const decision = (over: Partial<LedgerDecisionDto> = {}): LedgerDecisionDto => ({
  decisionId: 'dcn_00000000-0000-7000-8000-00000000000a',
  kind: 'cull',
  kindTitle: 'Selection',
  subjectKind: 'image',
  subjectId: photoA,
  rawConfidence: 0.81,
  calibratedConfidence: 0.81,
  calibrationVer: 0,
  calibrated: false,
  autonomy: 'suggest',
  autonomyTitle: 'Applied - worth a look',
  autonomyText: 'AURA has done this and put it in the review queue',
  needsReview: true,
  source: 'local',
  reasons: [reason()],
  outputsJson: '{"kept":true}',
  inputsHash: '0123456789abcdef',
  modelVersions: [['cull_subscores', 1]],
  configVersions: [['autonomy_bands', 1]],
  ms: 4,
  createdAt: 1_770_000_000_000,
  supersedes: null,
  ...over,
});

const panel = (over: Partial<ExplainPanelDto> = {}): ExplainPanelDto => ({
  photoId: photoA,
  tabs: [
    tab(),
    tab({
      id: 'edit',
      title: 'Edit',
      available: false,
      unavailableReason: 'AURA has not developed this photograph.',
      reasons: [],
      score: null,
      confidence: null,
    }),
  ],
  decision: decision(),
  headline: 'The strongest frame of this moment',
  summary: 'This photograph is in the gallery.',
  summaryFromCloud: false,
  alternatives: [],
  ...over,
});

const alternative = (over: Partial<AlternativeDto> = {}): AlternativeDto => ({
  photoId: photoA,
  keepScore: 0.81,
  technical: 0.77,
  emotion: 0.64,
  composition: 0.7,
  prominence: 0.5,
  delivered: true,
  ...over,
});

describe('reason rendering', () => {
  it('never renders a caveat as a fault', () => {
    const caveat = reason({ code: 'keypoints_unavailable', severity: 'caveat' });
    expect(isCaveat(caveat)).toBe(true);
    expect(severityLabel('caveat')).toBe('not checked');
    expect(severityLabel('fault')).toBe('counted against it');
    expect(severityLabel('caveat')).not.toBe(severityLabel('fault'));
  });

  it('scales the weight bar against the strongest reason on the panel', () => {
    expect(barWidth(0.6, 0.6)).toBe(100);
    expect(barWidth(0.3, 0.6)).toBe(50);
    expect(barWidth(-0.6, 0.6)).toBe(100);
    expect(barWidth(0.9, 0.6)).toBe(100);
  });

  it('labels the three kinds of evidence and stays quiet when there is none', () => {
    expect(evidenceLabel(reason())).toBeNull();
    expect(evidenceLabel(reason({ evidenceKind: 'crop' }))).toBe('shows where');
    expect(evidenceLabel(reason({ evidenceKind: 'frames', frames: [photoB] }))).toBe(
      'compare with the other frame',
    );
    expect(
      evidenceLabel(
        reason({ evidenceKind: 'params', params: [{ name: 'exposure_ev', value: 0.42 }] }),
      ),
    ).toBe('exposure_ev +0.42');
  });

  it('orders reasons by weight with the code as the tie-break', () => {
    const ordered = orderedReasons([
      reason({ code: 'b_code', weight: 0.4 }),
      reason({ code: 'a_code', weight: 0.4 }),
      reason({ code: 'strong', weight: -0.9 }),
    ]);
    expect(ordered.map((entry) => entry.code)).toEqual(['strong', 'a_code', 'b_code']);
    expect(strongestWeight(ordered)).toBeCloseTo(0.9);
  });
});

describe('evidence crops', () => {
  it('grows a tight box for context and stays inside the frame', () => {
    const grown = withContext({ x: 0.02, y: 0.02, w: 0.1, h: 0.1 });
    expect(grown.x).toBeGreaterThanOrEqual(0);
    expect(grown.y).toBeGreaterThanOrEqual(0);
    expect(grown.x + grown.w).toBeLessThanOrEqual(1.0001);
    expect(grown.y + grown.h).toBeLessThanOrEqual(1.0001);
  });

  it('describes a region in words rather than in coordinates', () => {
    expect(cropDescription({ x: 0.05, y: 0.05, w: 0.1, h: 0.1 }, 'the closed eye')).toContain(
      'upper left',
    );
    const style = boxStyle({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 });
    expect(style.left).toMatch(/%$/);
  });
});

describe('the alternative comparison', () => {
  it('reads a missing measurement as missing rather than as zero', () => {
    expect(scoreCell(0)).toBe('not measured');
    expect(scoreCell(0.77)).toBe('0.77');
  });

  it('says so when two frames were within a hundredth', () => {
    const sentence = comparisonSentence([
      alternative({ keepScore: 0.8 }),
      alternative({ photoId: photoB, keepScore: 0.795, delivered: false }),
    ]);
    expect(sentence).toContain('tie-break');
  });

  it('offers no way to swap the two frames', () => {
    render(
      <AlternativeCompare
        alternatives={[alternative(), alternative({ photoId: photoB, delivered: false })]}
      />,
    );
    for (const button of screen.getAllByRole('button')) {
      const label = (button.textContent ?? '').toLowerCase();
      expect(label).not.toContain('swap');
      expect(label).not.toContain('replace');
      expect(label).not.toContain('use this');
    }
  });

  it('is honest when a moment held one frame', () => {
    render(<AlternativeCompare alternatives={[]} />);
    expect(screen.getByText(/not the same as there having been nothing as good/i)).toBeTruthy();
  });
});

describe('the panel', () => {
  it('shows the top three reasons and offers the rest', async () => {
    const many = Array.from({ length: 5 }, (_, index) =>
      reason({ code: `code_${index}`, weight: 0.5 - index * 0.05, text: `reason ${index}` }),
    );
    vi.mocked(explain.explainImage).mockResolvedValue(panel({ tabs: [tab({ reasons: many })] }));

    render(<ExplainPanel photoId={photoA} />);
    await waitFor(() => expect(screen.getByText('reason 0')).toBeTruthy());
    expect(screen.queryByText('reason 4')).toBeNull();
    expect(DEFAULT_REASONS).toBe(3);

    fireEvent.click(screen.getByRole('button', { name: /show all 5/i }));
    await waitFor(() => expect(screen.getByText('reason 4')).toBeTruthy());
  });

  it('says why a tab is empty instead of rendering nothing', async () => {
    vi.mocked(explain.explainImage).mockResolvedValue(panel());
    render(<ExplainPanel photoId={photoA} />);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Edit' })).toBeTruthy());

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    await waitFor(() =>
      expect(screen.getByText(/has not developed this photograph/i)).toBeTruthy(),
    );
    expect(
      tabPlaceholder({
        id: 'qc',
        title: 'Quality check',
        available: false,
        unavailableReason: null,
        reasons: [],
        score: null,
        confidence: null,
      }),
    ).toBe('There is nothing recorded here.');
  });

  it('says the confidence is uncalibrated rather than showing a percentage it cannot stand behind', async () => {
    vi.mocked(explain.explainImage).mockResolvedValue(panel());
    render(<ExplainPanel photoId={photoA} />);
    await waitFor(() =>
      expect(screen.getByText(/has not yet learned how often it is right/i)).toBeTruthy(),
    );
    expect(calibrationCaveat(panel({ decision: decision({ calibrated: true }) }))).toBeNull();
    expect(percent(0.815)).toBe('82%');
  });

  it('carries no control that could change a decision', async () => {
    vi.mocked(explain.explainImage).mockResolvedValue(panel());
    render(<ExplainPanel photoId={photoA} />);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Selection' })).toBeTruthy());

    for (const button of screen.getAllByRole('button')) {
      const label = (button.textContent ?? '').toLowerCase();
      for (const forbidden of ['keep', 'reject', 'delete', 'apply', 'deliver', 'cull']) {
        expect(label).not.toContain(forbidden);
      }
    }
  });

  it('shows the decision id and the question hash for a support case', async () => {
    vi.mocked(explain.explainImage).mockResolvedValue(panel());
    render(<ExplainPanel photoId={photoA} />);
    await waitFor(() => expect(screen.getByText(/0123456789abcdef/)).toBeTruthy());
  });
});
