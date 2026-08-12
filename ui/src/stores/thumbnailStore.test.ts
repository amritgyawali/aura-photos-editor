import { beforeEach, describe, expect, it } from 'vitest';

import type { PreviewPayload } from '../ipc/types';
import { MAX_ENTRIES, useThumbnails } from './thumbnailStore';

function payload(photoId: string, tier = 1): PreviewPayload {
  return {
    photoId,
    tier,
    width: 512,
    height: 341,
    source: tier >= 2 ? 'decoded' : 'embedded',
    dataUrl: `data:image/jpeg;base64,${photoId}`,
  };
}

describe('thumbnailStore', () => {
  beforeEach(() => {
    useThumbnails.getState().clear();
  });

  it('stores and returns a decoded preview', () => {
    useThumbnails.getState().put('pht_1', payload('pht_1'));
    const entry = useThumbnails.getState().get('pht_1');
    expect(entry?.dataUrl).toContain('pht_1');
    expect(entry?.tier).toBe(1);
  });

  it('upgrades a thumbnail to a proxy in place', () => {
    useThumbnails.getState().put('pht_1', payload('pht_1', 1));
    useThumbnails.getState().put('pht_1', payload('pht_1', 2));
    expect(useThumbnails.getState().get('pht_1')?.tier).toBe(2);
  });

  it('never downgrades a cell that already has better pixels', () => {
    useThumbnails.getState().put('pht_1', payload('pht_1', 2));
    useThumbnails.getState().put('pht_1', payload('pht_1', 1));
    expect(useThumbnails.getState().get('pht_1')?.tier).toBe(2);
  });

  it('keeps the number of decoded bitmaps under the ceiling', () => {
    for (let index = 0; index < MAX_ENTRIES + 50; index += 1) {
      useThumbnails.getState().put(`pht_${index}`, payload(`pht_${index}`));
    }
    expect(useThumbnails.getState().entries.size).toBeLessThanOrEqual(MAX_ENTRIES);
  });

  it('evicts the least recently used entry first', () => {
    for (let index = 0; index < MAX_ENTRIES; index += 1) {
      useThumbnails.getState().put(`pht_${index}`, payload(`pht_${index}`));
    }
    // Touch the oldest entry, then overflow by one.
    expect(useThumbnails.getState().get('pht_0')).toBeDefined();
    useThumbnails.getState().put('pht_new', payload('pht_new'));

    expect(useThumbnails.getState().entries.has('pht_0')).toBe(true);
    expect(useThumbnails.getState().entries.has('pht_1')).toBe(false);
  });

  it('records a failure so the cell stops asking', () => {
    useThumbnails.getState().markFailed('pht_bad', 'This photo could not be opened.');
    expect(useThumbnails.getState().failed.get('pht_bad')).toContain('could not be opened');
    expect(useThumbnails.getState().entries.has('pht_bad')).toBe(false);
  });

  it('clears a failure when pixels finally arrive', () => {
    useThumbnails.getState().markFailed('pht_1', 'transient');
    useThumbnails.getState().put('pht_1', payload('pht_1'));
    expect(useThumbnails.getState().failed.has('pht_1')).toBe(false);
    expect(useThumbnails.getState().entries.has('pht_1')).toBe(true);
  });

  it('drops everything when the project changes', () => {
    useThumbnails.getState().put('pht_1', payload('pht_1'));
    useThumbnails.getState().clear();
    expect(useThumbnails.getState().entries.size).toBe(0);
    expect(useThumbnails.getState().inFlight.size).toBe(0);
  });
});
