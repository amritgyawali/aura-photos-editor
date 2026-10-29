import { describe, expect, it } from 'vitest';

import { finishedPhotos } from './DeliveryPanel';

describe('the photos an export covers by default', () => {
  it('is the set the automatic edit just finished', () => {
    expect(finishedPhotos(['a', 'b'], ['a', 'b', 'c'])).toEqual({ ids: ['a', 'b'], label: 'The 2 photos you just edited' });
  });

  it("is the cull's best set when nothing was just edited", () => {
    expect(finishedPhotos(null, ['a', 'c'])).toEqual({ ids: ['a', 'c'], label: 'The 2 best photos from the cull' });
  });

  it('is every photo when the collection was never culled', () => {
    expect(finishedPhotos(null, null)).toBeNull();
    expect(finishedPhotos([], [])).toBeNull();
  });
});
