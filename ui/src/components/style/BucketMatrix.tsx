/**
 * PHASE-17. The style tree as a heat map: eight scene groups down, ten kinds of light across.
 *
 * Three things this component is careful about, and all three are the same care:
 *
 * 1. **A bucket with no held-out pairs shows "not measured", never a number.** `matchDe00` is
 *    `null` for a leaf that was trained and not evaluated, and rendering that as zero would
 *    make the report look best exactly where it knows least.
 * 2. **A leaf the photographer taught and a leaf that borrowed from its parent look
 *    different.** `level` is on the wire for this, and a matrix that coloured them the same
 *    would be telling somebody they have a dance-floor style when they have a wedding-wide one.
 * 3. **An empty cell is empty, not bad.** Seventy of eighty leaves are empty for a photographer
 *    who shoots one kind of wedding, and their profile is fine.
 */
import { useMemo } from 'react';

import type { StyleBucketDto } from '../../ipc/types';

/** The eight scene groups, in wedding order. Mirrors `SceneGroup::ALL`. */
export const GROUPS = [
  'preparation',
  'details',
  'ceremony',
  'portraits',
  'reception',
  'dance',
  'candid',
  'other',
] as const;

/** The ten kinds of light. Mirrors `LightingBucket::ALL`. */
export const LIGHTING = [
  'unknown',
  'daylight',
  'golden_hour',
  'shade',
  'overcast',
  'tungsten',
  'artificial',
  'flash',
  'candle',
  'stage',
] as const;

const GROUP_TITLES: Record<string, string> = {
  preparation: 'Preparation',
  details: 'Details',
  ceremony: 'Ceremony',
  portraits: 'Portraits',
  reception: 'Reception',
  dance: 'Dance',
  candid: 'Candid',
  other: 'Other',
};

const LIGHT_TITLES: Record<string, string> = {
  unknown: 'Unknown',
  daylight: 'Daylight',
  golden_hour: 'Golden',
  shade: 'Shade',
  overcast: 'Overcast',
  tungsten: 'Tungsten',
  artificial: 'LED',
  flash: 'Flash',
  candle: 'Candle',
  stage: 'Stage',
};

export type BucketMatrixProps = {
  buckets: StyleBucketDto[];
  onSelect?: (key: string) => void;
};

/** How a cell should read: taught, borrowed, or nothing at all. */
export function cellState(bucket: StyleBucketDto | undefined): 'empty' | 'taught' | 'borrowed' {
  if (!bucket || bucket.samples === 0) return 'empty';
  return bucket.level === 'bucket' ? 'taught' : 'borrowed';
}

/** What a cell says about its accuracy. Never a number it does not have. */
export function cellAccuracy(bucket: StyleBucketDto | undefined): string {
  if (!bucket || bucket.samples === 0) return '';
  if (bucket.matchDe00 === null || bucket.matchDe00 === undefined) return 'not measured yet';
  return `${bucket.matchDe00.toFixed(1)} dE00`;
}

export function BucketMatrix({ buckets, onSelect }: BucketMatrixProps) {
  const byKey = useMemo(() => {
    const map = new Map<string, StyleBucketDto>();
    for (const bucket of buckets) map.set(bucket.key, bucket);
    return map;
  }, [buckets]);

  return (
    <section className="bucket-matrix" aria-label="What AURA has learned, by scene and light">
      <table>
        <caption>
          Where your style is strong, and where AURA is still borrowing from the rest of your
          work
        </caption>
        <thead>
          <tr>
            <th scope="col">Scene</th>
            {LIGHTING.map((light) => (
              <th key={light} scope="col">
                {LIGHT_TITLES[light] ?? light}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {GROUPS.map((group) => (
            <tr key={group}>
              <th scope="row">{GROUP_TITLES[group] ?? group}</th>
              {LIGHTING.map((light) => {
                const key = `${group}/${light}`;
                const bucket = byKey.get(key);
                const state = cellState(bucket);
                return (
                  <td
                    key={key}
                    className={`bucket-cell bucket-${state}`}
                    title={
                      state === 'empty'
                        ? 'no photographs of this kind yet'
                        : `${bucket?.samples ?? 0} photographs, ${cellAccuracy(bucket)}`
                    }
                    onClick={onSelect ? () => onSelect(key) : undefined}
                  >
                    {state === 'empty' ? (
                      <span className="bucket-none">—</span>
                    ) : (
                      <>
                        <span className="bucket-count">{bucket?.samples ?? 0}</span>
                        <span className="bucket-accuracy">{cellAccuracy(bucket)}</span>
                      </>
                    )}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <p className="matrix-legend">
        A filled cell is a kind of photograph AURA has learned from your own work. A pale cell is
        one where it has some of your photographs but not enough, so it uses your overall look
        instead. An empty cell means you have not given it any — which is perfectly normal, and
        does not make the profile weaker at what you do shoot.
      </p>
    </section>
  );
}
