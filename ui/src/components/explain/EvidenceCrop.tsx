import type { CSSProperties } from 'react';

import type { EvidenceCropDto } from '../../ipc/types';

/**
 * PHASE-13. The region a reason points at, drawn over the photograph.
 *
 * Section 6.2: an explanation that says "the eyes are soft" is an assertion, and
 * one that shows the crop of the eye is an argument.
 *
 * Two things this component does not do, both deliberate:
 *
 * * **It does not fetch pixels of its own.** It draws a rectangle over whatever
 *   thumbnail its parent already has. The ledger stores four numbers; nothing in
 *   phase 13 opens an image file, which is what makes a support bundle pixel-free
 *   by construction rather than by filtering.
 * * **It does not crop anything.** A rectangle on an overlay is evidence. Phase 23
 *   owns geometry; this is a box drawn on a preview.
 */

/** Grow a tight box so the photographer can see what surrounds it. */
export function withContext(crop: EvidenceCropDto, fraction = 0.6): EvidenceCropDto {
  const growW = (crop.w * fraction) / 2;
  const growH = (crop.h * fraction) / 2;
  const x = Math.max(0, crop.x - growW);
  const y = Math.max(0, crop.y - growH);
  return {
    x,
    y,
    w: Math.min(1 - x, crop.w + growW * 2),
    h: Math.min(1 - y, crop.h + growH * 2),
  };
}

/** The rectangle as a percentage box over the preview. */
export function boxStyle(crop: EvidenceCropDto): CSSProperties {
  const grown = withContext(crop);
  return {
    left: `${(grown.x * 100).toFixed(2)}%`,
    top: `${(grown.y * 100).toFixed(2)}%`,
    width: `${(grown.w * 100).toFixed(2)}%`,
    height: `${(grown.h * 100).toFixed(2)}%`,
  };
}

/** What a screen reader is told about the box, in plain words. */
export function cropDescription(crop: EvidenceCropDto, what: string): string {
  const horizontal = crop.x + crop.w / 2 < 0.4 ? 'left' : crop.x + crop.w / 2 > 0.6 ? 'right' : 'centre';
  const vertical = crop.y + crop.h / 2 < 0.4 ? 'upper' : crop.y + crop.h / 2 > 0.6 ? 'lower' : 'middle';
  return `${what}, ${vertical} ${horizontal} of the frame`;
}

export type EvidenceCropProps = {
  crop: EvidenceCropDto;
  /** The sentence this rectangle belongs to. */
  caption: string;
  /** An already-loaded preview, when the parent has one. */
  src?: string | null;
};

export function EvidenceCrop({ crop, caption, src }: EvidenceCropProps) {
  return (
    <figure className="evidence-crop">
      <div className="evidence-frame">
        {src ? (
          <img src={src} alt="" className="evidence-image" />
        ) : (
          <div className="evidence-placeholder" aria-hidden="true" />
        )}
        <span className="evidence-box" style={boxStyle(crop)} />
      </div>
      <figcaption>{cropDescription(crop, caption)}</figcaption>
    </figure>
  );
}
