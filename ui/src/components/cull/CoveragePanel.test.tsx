import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { CoverageReportDto, CullStatusDto, DecisionDto } from '../../ipc/types';

import {
  analysisSentence,
  CoveragePanel,
  COVERAGE_WARN_BELOW,
  signalSentence,
  stateExplanation,
  stateLabel,
} from './CoveragePanel';
import { canOverride, headline, NOT_ANALYSED, RejectReasons } from './RejectReasons';
import { floorFor, overshootSentence } from './SizeSlider';

function status(overrides: Partial<CullStatusDto> = {}): CullStatusDto {
  return {
    photos: 1000,
    eligible: 1000,
    selected: 300,
    coverage: 1,
    emotionAware: 1,
    compositionAware: 1,
    grouped: 1,
    covered: 12,
    coveredWeak: 0,
    missing: 0,
    userKept: 0,
    userRejected: 0,
    mode: 'balanced',
    deterministicHash: '00000000deadbeef',
    modelVer: 1,
    analysisVer: 1,
    calibrationVer: 0,
    ...overrides,
  };
}

function coverage(overrides: Partial<CoverageReportDto> = {}): CoverageReportDto {
  return {
    mustHaves: [
      { rule: 'vows', title: 'The vows', state: 'covered', satisfied: true },
      { rule: 'rings', title: 'The rings', state: 'covered_weak', satisfied: true },
      { rule: 'cake', title: 'The cake', state: 'missing', satisfied: false },
    ],
    identityCoverage: [],
    chapters: [
      { chapter: 'ceremony', title: 'Ceremony', delivered: 40, target: 38 },
      { chapter: 'exit', title: 'Exit', delivered: 0, target: 0 },
    ],
    warnings: ['The cake: no candidates found. AURA cannot add what was not photographed.'],
    ...overrides,
  };
}

describe('the three coverage states are words, never a colour alone', () => {
  it('labels each state', () => {
    expect(stateLabel('covered')).toBe('covered');
    expect(stateLabel('covered_weak')).toBe('weakly covered');
    expect(stateLabel('missing')).toBe('missing');
  });

  it('says that missing means nobody photographed it', () => {
    expect(stateExplanation('missing')).toContain('cannot invent');
  });

  it('says that weakly covered is in the gallery', () => {
    expect(stateExplanation('covered_weak')).toContain('in the gallery');
  });

  it('renders every rule with its state as text', () => {
    render(<CoveragePanel status={status()} coverage={coverage()} />);
    expect(screen.getByText('The vows')).toBeDefined();
    expect(screen.getByText('missing')).toBeDefined();
    expect(screen.getByText('weakly covered')).toBeDefined();
  });

  it('never claims everything is covered when a rule is missing', () => {
    render(<CoveragePanel status={status()} coverage={coverage()} />);
    expect(screen.queryByText(/Every part of the wedding/)).toBeNull();
    expect(screen.getByText(/1 of 3 are missing/)).toBeDefined();
  });
});

describe('the header says what the gallery was chosen from', () => {
  it('is quiet at full coverage', () => {
    expect(analysisSentence(status())).toContain('1000 of 1000');
  });

  it('says how much of the wedding was left out when analysis is short', () => {
    const partial = status({ eligible: 600, coverage: 0.6 });
    expect(analysisSentence(partial)).toContain('60%');
    expect(analysisSentence(partial)).toContain('not been checked');
  });

  it('warns below the threshold and not above it', () => {
    expect(COVERAGE_WARN_BELOW).toBeGreaterThan(0.9);
  });

  it('says how much of the fusion was real', () => {
    const thin = status({ emotionAware: 0.03 });
    expect(signalSentence(thin)).toContain('expression on 3%');
  });
});

describe('the size slider is honest about a gallery it cannot shrink', () => {
  it('says how many extra frames a guarantee forced', () => {
    expect(overshootSentence(500, 540)).toContain('40 more');
    expect(overshootSentence(500, 540)).toContain('guarantees');
  });

  it('offers a floor that is not zero', () => {
    expect(floorFor(1000)).toBe(50);
    expect(floorFor(3)).toBeGreaterThanOrEqual(1);
  });
});

describe('a decision is explained in both directions', () => {
  const kept: DecisionDto = {
    kept: true,
    selected: {
      photoId: 'pht_a',
      momentId: 'mom_a',
      keepScore: 0.8,
      confidence: 0.7,
      reasons: [
        {
          code: 'moment_winner',
          text: 'the strongest frame of this moment',
          weight: 0.8,
          keep: true,
          veto: false,
        },
      ],
      runnerUp: 'pht_b',
      coverageRole: null,
      protected: false,
    },
    rejected: null,
  };

  const unchecked: DecisionDto = {
    kept: false,
    selected: null,
    rejected: {
      photoId: 'pht_c',
      momentId: null,
      keepScore: 0,
      reasons: [
        {
          code: NOT_ANALYSED,
          text: 'AURA has not checked this photograph yet',
          weight: -0.1,
          keep: false,
          veto: false,
        },
      ],
      keptInstead: null,
      wasPeak: false,
      vetoed: false,
    },
  };

  it('leads a keeper with the reason rather than a number', () => {
    expect(headline(kept)).toContain('the strongest frame of this moment');
    expect(headline(kept)).not.toMatch(/0\.\d/);
  });

  it('says that an unchecked photograph was not judged', () => {
    expect(headline(unchecked)).toContain('not a judgement');
  });

  it('offers no override on a photograph nobody looked at', () => {
    expect(canOverride(unchecked)).toBe(false);
    expect(canOverride(kept)).toBe(true);
    render(<RejectReasons decision={unchecked} />);
    expect(screen.queryByText('Keep this one')).toBeNull();
    expect(screen.getByText(/Run the analysis/)).toBeDefined();
  });

  it('offers the alternative to compare', () => {
    render(<RejectReasons decision={kept} />);
    expect(screen.getByText('Compare with the alternative')).toBeDefined();
  });

  it('renders nothing that could delete a photograph', () => {
    const { container } = render(<RejectReasons decision={kept} />);
    expect(container.textContent).not.toMatch(/delete|trash|remove from disk/i);
  });
});
