import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';

import { AbCompare, format, levelLabel } from './AbCompare';
import { BucketMatrix, cellAccuracy, cellState } from './BucketMatrix';
import { ProfileReport, signatureLabel, strengthLabel } from './ProfileReport';
import { scanHeadline, trainHeadline } from './TeachMyAi';
import type {
  ImportProfileDto,
  ProfileReportDto,
  ScanArchiveDto,
  StyleBucketDto,
  StyleComparisonDto,
  StyleProfileDto,
  TrainProfileDto,
} from '../../ipc/types';

function bucket(overrides: Partial<StyleBucketDto> = {}): StyleBucketDto {
  return {
    key: 'portraits/daylight',
    group: 'portraits',
    lighting: 'daylight',
    title: 'Portraits - Daylight',
    samples: 40,
    heldOut: 8,
    matchDe00: 1.4,
    level: 'bucket',
    weak: false,
    ...overrides,
  };
}

function profile(overrides: Partial<StyleProfileDto> = {}): StyleProfileDto {
  return {
    profileId: 'prf_1',
    name: 'light and airy',
    version: 2,
    status: 'candidate',
    trainedPairs: 812,
    strength: 0.71,
    overallDe00: 1.9,
    taughtBuckets: 9,
    usable: true,
    engineVer: 'aura-render/1.0.0',
    trainedAt: 1_700_000_000_000,
    ...overrides,
  };
}

function report(overrides: Partial<ProfileReportDto> = {}): ProfileReportDto {
  return {
    profile: profile(),
    perBucket: [bucket()],
    weakBuckets: ['dance/flash'],
    recommendation: 'add one wedding with dance in flash light',
    acceptedPairs: 812,
    rejectedPairs: 96,
    acceptance: 0.894,
    metCeiling: true,
    ...overrides,
  };
}

afterEach(cleanup);

describe('BucketMatrix', () => {
  it('renders a bucket with no held-out pairs as "not measured", never as a number', () => {
    expect(cellAccuracy(bucket({ matchDe00: null }))).toBe('not measured yet');
    expect(cellAccuracy(bucket({ matchDe00: null }))).not.toMatch(/0/);
  });

  it('distinguishes a taught leaf from one that borrowed from its parent', () => {
    expect(cellState(bucket({ level: 'bucket' }))).toBe('taught');
    expect(cellState(bucket({ level: 'group' }))).toBe('borrowed');
    expect(cellState(bucket({ samples: 0 }))).toBe('empty');
    expect(cellState(undefined)).toBe('empty');
  });

  it('says an empty cell is normal rather than a fault', () => {
    render(<BucketMatrix buckets={[bucket()]} />);
    expect(screen.getByText(/perfectly normal/i)).toBeTruthy();
  });

  it('draws all eighty leaves', () => {
    const { container } = render(<BucketMatrix buckets={[bucket()]} />);
    expect(container.querySelectorAll('td.bucket-cell').length).toBe(80);
  });
});

describe('ProfileReport', () => {
  it('leads with a measurement rather than with a ready state', () => {
    render(<ProfileReport report={report()} />);
    expect(screen.getByText(/1\.9 dE00/)).toBeTruthy();
    expect(screen.queryByText(/\bready\b/i)).toBeNull();
  });

  it('shows how many pairs were left out beside how many were used', () => {
    render(<ProfileReport report={report()} />);
    expect(screen.getByText(/812 used, 96 left out/)).toBeTruthy();
  });

  it('never says a bundle is verified', () => {
    const imported: ImportProfileDto = {
      profile: profile(),
      fingerprint: 'aaaa-bbbb-cccc-dddd',
      unchangedSinceSigning: true,
    };
    expect(signatureLabel(imported)).toContain('unchanged since signing');
    expect(signatureLabel(imported).toLowerCase()).not.toContain('verified');

    render(<ProfileReport report={report()} imported={imported} />);
    expect(screen.queryByText(/verified/i)).toBeNull();
  });

  it('renders the strength as a number and a word', () => {
    expect(strengthLabel(0.8)).toBe('80% — strong');
    expect(strengthLabel(0.5)).toBe('50% — usable');
    expect(strengthLabel(0.1)).toBe('10% — still learning');
    expect(strengthLabel(0)).toBe('0% — nothing learned yet');
  });

  it('says the skin check runs after the style rather than before it', () => {
    render(<ProfileReport report={report()} />);
    expect(screen.getByText(/skin check runs after your style/i)).toBeTruthy();
  });

  it('offers the wizard rather than a zero score when nothing has been trained', () => {
    render(<ProfileReport report={null} />);
    expect(screen.getByText(/No profile has been trained yet/i)).toBeTruthy();
  });
});

describe('AbCompare', () => {
  const row: StyleComparisonDto = {
    bucket: 'portraits/daylight',
    title: 'Portraits - Daylight',
    baseline: [0, 0, 0, 0],
    current: [0.1, 100, 4, 2],
    candidate: [0.3, -200, -6, 5],
    level: 'bucket',
    confidence: 0.82,
    reasons: [],
  };

  it('formats every axis in its own units with a sign', () => {
    expect(format('exposure', 0.3)).toBe('+0.30 EV');
    expect(format('temperature', -200)).toBe('-200 K');
    expect(format('contrast', 6)).toBe('+6');
  });

  it('names where a borrowed answer came from', () => {
    expect(levelLabel('bucket')).toMatch(/your own photographs/);
    expect(levelLabel('group')).toMatch(/borrowed/);
    expect(levelLabel('global')).toMatch(/borrowed/);
    expect(levelLabel('factory')).toMatch(/no style/);
  });

  it('renders no image and asks for none', () => {
    const { container } = render(<AbCompare rows={[row]} />);
    expect(container.querySelectorAll('img').length).toBe(0);
    expect(container.querySelectorAll('canvas').length).toBe(0);
  });

  it('says plainly when a candidate would change nothing in this wedding', () => {
    render(<AbCompare rows={[]} />);
    expect(screen.getByText(/edited exactly as it is now/i)).toBeTruthy();
  });
});

describe('TeachMyAi', () => {
  function scan(overrides: Partial<ScanArchiveDto> = {}): ScanArchiveDto {
    return {
      originals: 3200,
      finals: 900,
      matched: 880,
      unmatchedOriginals: 2320,
      unmatchedFinals: 20,
      byMethod: [['filename_stem', 880]],
      weakestMethod: 'filename_stem',
      enough: true,
      ...overrides,
    };
  }

  it('names the commonest failure before any training is done', () => {
    expect(scanHeadline(scan({ originals: 0 }))).toMatch(/No camera originals/);
    expect(scanHeadline(scan({ finals: 0 }))).toMatch(/No finished photographs/);
    expect(scanHeadline(scan({ matched: 10, unmatchedFinals: 890 }))).toMatch(/another disk/);
  });

  it('reports what was left out alongside what was used', () => {
    const result: TrainProfileDto = {
      profile: profile(),
      matched: 908,
      accepted: 812,
      rejected: 96,
      reused: 0,
      fromXmp: 812,
      buckets: 9,
      overallDe00: 1.9,
      baselineDe00: 4.1,
      elapsedMs: 900_000,
      cancelled: false,
    };
    expect(trainHeadline(result)).toMatch(/812 photographs used, 96 left out/);
    expect(trainHeadline(result)).toMatch(/2\.2 closer/);
  });

  it('says a cancelled run keeps its work', () => {
    const cancelled: TrainProfileDto = {
      profile: null,
      matched: 400,
      accepted: 300,
      rejected: 10,
      reused: 0,
      fromXmp: 0,
      buckets: 0,
      overallDe00: 0,
      baselineDe00: 0,
      elapsedMs: 12_000,
      cancelled: true,
    };
    expect(trainHeadline(cancelled)).toMatch(/picks up where this left off/);
  });
});
