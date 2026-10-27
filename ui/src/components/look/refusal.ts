import { asIpcError } from '../../ipc/client';

/** A sentence this workspace wrote for the photographer: why a match was not saved, or what to
 *  do next. It is shown as written. Anything else (a backend error, a bug) goes through
 *  `asIpcError`, which never shows a raw exception to the photographer. */
export class Refusal extends Error {}

export const reasonOf = (cause: unknown): string =>
  cause instanceof Refusal ? cause.message : asIpcError(cause).message;
