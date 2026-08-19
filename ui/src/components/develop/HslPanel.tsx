import { useMemo } from 'react';

import type { ColourDto, HslShiftDto, RecipeDto } from '../../ipc/types';

/**
 * PHASE-16. The HSL panel: eight bands, what AURA found in the frame, and the protected-skin
 * indicator.
 *
 * Section 9 gives SFE "HSL panel with protected-skin indicator". Four rules this component
 * keeps:
 *
 * 1. **The bands say what they were inferred from.** Every reading carries a confidence,
 *    because the content is worked out from colour statistics rather than outlined - and a
 *    photographer deciding whether to trust "greenery" needs to know that.
 * 2. **Skin is shown and cannot be adjusted.** There is no slider for it, and the indicator
 *    reports the measurement rather than a reassurance.
 * 3. **A band with nothing behind it is visibly empty.** A slider at zero next to a band AURA
 *    found nothing of is a different thing from a slider at zero next to a band it found and
 *    left alone, and the two get different sentences.
 * 4. **A person's band carries the protected dot**, the same mark phase 14 uses everywhere.
 */

/** The eight bands, in the recipe's order. */
const ORDER = [
  'red',
  'orange',
  'yellow',
  'green',
  'aqua',
  'blue',
  'purple',
  'magenta',
] as const;

/** Plain words for a content band. */
function contentName(band: string): string {
  switch (band) {
    case 'greenery':
      return 'greenery';
    case 'sky':
      return 'sky';
    case 'dress':
      return 'bright near-white areas';
    case 'wood':
      return 'wood and warm surfaces';
    case 'decor':
      return 'decor';
    case 'skin':
      return 'skin';
    default:
      return band;
  }
}

export type HslPanelProps = {
  /** The photograph's grade, or `null` when nobody has graded it. */
  colour: ColourDto | null;
  /** The current edit, for the protected mark. */
  recipe: RecipeDto;
  /** Record a hand-set band. Whole-or-nothing: the caller sends all eight. */
  onHslChange: (bands: HslShiftDto[]) => void;
};

export function HslPanel({ colour, recipe, onHslChange }: HslPanelProps) {
  const protectedHsl = useMemo(
    () => recipe.userEditedFields.includes('global.hsl'),
    [recipe.userEditedFields],
  );

  if (!colour) {
    return (
      <section className="hsl-panel" aria-label="Colour by band">
        <h3>Colour</h3>
        <p data-testid="hsl-ungraded">
          AURA has not worked out the colour for this photograph yet.
        </p>
      </section>
    );
  }

  const byBand = new Map(colour.hsl.map((shift) => [shift.band, shift]));
  const skin = colour.bands.find((reading) => reading.band === 'skin');
  const adjustable = colour.bands.filter((reading) => reading.band !== 'skin');

  return (
    <section className="hsl-panel" aria-label="Colour by band">
      <h3>
        Colour
        {protectedHsl ? (
          <span
            className="develop-protected"
            title="You set these. AURA will not change them."
            aria-label="You set these. AURA will not change them."
          >
            ●
          </span>
        ) : null}
      </h3>

      {/*
        The guarantee, stated where a photographer is about to move a saturation slider. It is
        a measurement rather than a reassurance, and it says plainly when there was nobody to
        protect.
      */}
      <p className="hsl-skin" data-testid="hsl-skin-indicator">
        {colour.skinGuard.measured
          ? `Skin is protected: every adjustment below was held back over it, and skin moved ${colour.skinGuard.maxHueShiftDeg.toFixed(
              1,
            )} degrees.`
          : 'There is nobody in this frame, so nothing here is being held back over skin.'}
      </p>

      {skin ? (
        <p className="hsl-skin-area" data-testid="hsl-skin-area">
          Skin covers {Math.round(skin.area * 100)}% of the frame and has no slider - it is
          never adjusted.
        </p>
      ) : null}

      <ul className="hsl-content" aria-label="What AURA found in this frame">
        {adjustable.length === 0 ? (
          <li data-testid="hsl-no-content">
            AURA could not pick out anything in this frame by colour, so it left the colours
            alone.
          </li>
        ) : (
          adjustable.map((reading) => (
            <li key={reading.band} data-testid={`hsl-content-${reading.band}`}>
              {contentName(reading.band)}, {Math.round(reading.area * 100)}% of the frame
              {reading.confidence < 0.33
                ? ' - not confidently enough to adjust'
                : ` - worked out from its colour, ${Math.round(reading.confidence * 100)}% sure`}
            </li>
          ))
        )}
      </ul>

      <ol className="hsl-bands">
        {ORDER.map((band) => {
          const shift = byBand.get(band) ?? { band, h: 0, s: 0, l: 0 };
          const neutral = shift.h === 0 && shift.s === 0 && shift.l === 0;
          return (
            <li className="hsl-band" key={band} data-testid={`hsl-band-${band}`}>
              <span className="hsl-band-name">{band}</span>
              {(['h', 's', 'l'] as const).map((axis) => (
                <input
                  key={axis}
                  aria-label={`${band} ${axis}`}
                  type="number"
                  step="1"
                  value={String(Math.round(shift[axis]))}
                  onChange={(event) => {
                    const next = ORDER.map((name) => {
                      const current = byBand.get(name) ?? { band: name, h: 0, s: 0, l: 0 };
                      return name === band
                        ? { ...current, [axis]: Number(event.target.value) }
                        : current;
                    });
                    onHslChange(next);
                  }}
                />
              ))}
              {neutral ? <span className="hsl-band-neutral">unchanged</span> : null}
            </li>
          );
        })}
      </ol>

      <p className="hsl-caveat" data-testid="hsl-caveat">
        AURA works out what is in a photograph from its colours rather than outlining it, so
        these adjustments are deliberately small.
      </p>
    </section>
  );
}
