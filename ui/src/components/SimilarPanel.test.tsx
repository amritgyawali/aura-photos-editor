import { describe, expect, it } from 'vitest';

import type {
  DescriptorsDto,
  EmbedProgressDto,
  IndexStatusDto,
  SimilarNeighbourDto,
} from '../ipc/types';
import {
  TIME_WINDOWS,
  coverageSentence,
  descriptorLines,
  duplicateLabel,
  filterableSentence,
  passSentence,
  similarityLabel,
} from './SimilarPanel';

function neighbour(overrides: Partial<SimilarNeighbourDto> = {}): SimilarNeighbourDto {
  return {
    photoId: 'pht_00000000-0000-0000-0000-000000000001',
    distance: 0.0432,
    similarity: 0.9568,
    dhashDistance: 12,
    nearDuplicate: false,
    ...overrides,
  };
}

function status(overrides: Partial<IndexStatusDto> = {}): IndexStatusDto {
  return {
    vectors: 4000,
    photos: 4000,
    coverage: 1,
    filterable: 4000,
    modelVer: 100,
    staleModelVersions: [],
    buildMs: 312,
    fromSnapshot: false,
    ...overrides,
  };
}

function progress(overrides: Partial<EmbedProgressDto> = {}): EmbedProgressDto {
  return {
    embedded: 4000,
    failed: 0,
    remaining: 0,
    elapsedMs: 156_000,
    batches: 125,
    cancelled: false,
    ...overrides,
  };
}

function descriptors(overrides: Partial<DescriptorsDto> = {}): DescriptorsDto {
  return {
    photoId: 'pht_00000000-0000-0000-0000-000000000001',
    dhashHex: 'a5a5f00d0badc0de',
    lumaMean: 0.4213,
    lumaP1: 0.0235,
    lumaP50: 0.4039,
    lumaP99: 0.9451,
    clipLo: 0.0012,
    clipHi: 0.0342,
    edgeEnergy: 0.1123,
    palette: ['#c8b4a0', '#282830', '#785a46', '#0a0a0c', '#fafafa'],
    modelVer: 100,
    pixelSource: 'tier 1 embedded',
    ...overrides,
  };
}

describe('SimilarPanel', () => {
  it('shows both the human number and the distance the next phase will threshold', () => {
    const label = similarityLabel(neighbour());
    expect(label).toContain('96% alike');
    expect(label).toContain('0.0432');
  });

  it('never shows a similarity outside nought to a hundred per cent', () => {
    expect(similarityLabel(neighbour({ similarity: 0, distance: 1.0 }))).toContain('0% alike');
    expect(similarityLabel(neighbour({ similarity: 1, distance: 0 }))).toContain('100% alike');
  });

  it('calls a zero-bit hash match identical, and says so in those words', () => {
    expect(duplicateLabel(neighbour({ dhashDistance: 0, nearDuplicate: true }))).toBe(
      'identical to the hash',
    );
  });

  it('reports a near duplicate with its bit count rather than a verdict', () => {
    const label = duplicateLabel(neighbour({ dhashDistance: 3, nearDuplicate: true }));
    expect(label).toContain('near duplicate');
    expect(label).toContain('3 bits');
    expect(label).not.toContain('delete');
  });

  it('reports a distinct frame as a bit distance', () => {
    expect(duplicateLabel(neighbour({ dhashDistance: 27 }))).toBe('27 bits apart');
  });

  it('explains an empty index rather than showing an empty list', () => {
    const sentence = coverageSentence(status({ vectors: 0, coverage: 0 }));
    expect(sentence).toContain('have been analysed');
    expect(sentence).toContain('4000');
  });

  it('explains an empty project', () => {
    expect(coverageSentence(status({ photos: 0, vectors: 0, coverage: 0 }))).toContain(
      'no photographs',
    );
  });

  it('says how long the index took to build, or that it was loaded', () => {
    expect(coverageSentence(status())).toContain('built in 312 ms');
    expect(coverageSentence(status({ fromSnapshot: true }))).toContain('snapshot');
  });

  it('mentions a background re-read when older versions are still present', () => {
    const sentence = coverageSentence(status({ staleModelVersions: [90] }));
    expect(sentence).toContain('re-read in the background');
  });

  it('stays quiet about filterability when every frame is filterable', () => {
    expect(filterableSentence(status())).toBeNull();
  });

  it('says how many frames a time filter cannot reach', () => {
    const sentence = filterableSentence(status({ filterable: 3990 }));
    expect(sentence).toContain('10 photograph(s)');
    expect(sentence).toContain('no capture time');
  });

  it('reports a clean pass without inventing failures', () => {
    const sentence = passSentence(progress());
    expect(sentence).toContain('analysed 4000');
    expect(sentence).not.toContain('could not be read');
    expect(sentence).toContain('156.0 s');
  });

  it('reports failures and remaining work when there are any', () => {
    const sentence = passSentence(progress({ embedded: 3990, failed: 10, remaining: 25 }));
    expect(sentence).toContain('10 could not be read');
    expect(sentence).toContain('25 still to do');
  });

  it('says when a pass was stopped early', () => {
    expect(passSentence(progress({ cancelled: true }))).toContain('stopped early');
  });

  it('reads out every descriptor a later phase will use', () => {
    const lines = descriptorLines(descriptors());
    expect(lines.join('\n')).toContain('a5a5f00d0badc0de');
    expect(lines.join('\n')).toContain('luminance mean 0.421');
    expect(lines.join('\n')).toContain('edge energy 0.1123');
    expect(lines.join('\n')).toContain('tier 1 embedded');
  });

  it('reports clipping as a percentage, because that is how it is read', () => {
    const lines = descriptorLines(descriptors({ clipLo: 0.25, clipHi: 0.0 }));
    expect(lines.join('\n')).toContain('25.0% dark');
    expect(lines.join('\n')).toContain('0.0% bright');
  });

  it('offers the whole wedding as well as burst-sized windows', () => {
    expect(TIME_WINDOWS[0]?.seconds).toBeNull();
    expect(TIME_WINDOWS.map((window) => window.seconds)).toContain(5);
    expect(TIME_WINDOWS.every((window) => window.label.length > 0)).toBe(true);
  });
});
