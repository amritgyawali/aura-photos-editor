import type { ColorMatchIntent } from '../../ipc/colorMatch';

/** Engines whose saved session is current for the Palette goal as well as Natural: a look
 *  learned from the reference's own original serves either goal. */
export const LEARNED_ENGINE = 'reference-original-calibrated-v1';
export const PALETTE_ENGINES: readonly string[] = ['reference-palette-transport-v8', LEARNED_ENGINE];
/** The reference's exact colours transported onto the photo (ADR-0182). */
export const EXACT_ENGINE = 'reference-exact-colours-v1';

/** Whether a saved session's engine answers this matching goal. */
export const engineServes = (intent: ColorMatchIntent, engine: string | undefined): boolean =>
  intent === 'exact' ? engine === EXACT_ENGINE
    : intent === 'palette' ? PALETTE_ENGINES.includes(engine ?? '')
      : engine !== EXACT_ENGINE;
