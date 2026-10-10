import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { nativeRetouch, type NativeRetouchEdit } from '../../ipc/nativeRetouch';
import type { RenderDto } from '../../ipc/types';

/** How long the brush rests before an unsaved draft is shown at full quality. */
export const FULL_QUALITY_AFTER_MS = 1200;

/** At most one native draft render at a time. Superseded drafts never reach the screen. */
export function useRetouchDraftPreview(projectId: string, photoId: string,
  draft: NativeRetouchEdit, replaceId: string | null, enabled: boolean, revision: number, selection = false) {
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const [state, setState] = useState<{ image: Pick<RenderDto, 'width'|'height'|'rgbBase64'> | null; pending: boolean; error: string | null; selection: boolean }>({
    image: null, pending: false, error: null, selection,
  });
  useEffect(() => {
    let current = true;
    let upgrade: number | undefined;
    setState({ image: null, pending: enabled, error: null, selection });
    if (!enabled) return;
    const timer = window.setTimeout(() => {
      queue.current = queue.current.catch(() => undefined).then(async () => {
        if (!current) return;
        try {
          // A live draft follows the brush, so it is the quick screen-sized render first.
          const image = selection ? await nativeRetouch.selectionPreview(projectId, photoId, draft, replaceId)
            : await nativeRetouch.draftPreview(projectId, photoId, draft, replaceId, 'fast');
          if (!current) return;
          setState({ image, pending: false, error: null, selection });
          // Once the brush has rested, the same draft at full quality. Only the draft itself is
          // rendered again - the saved retouch under it is kept (ADR-0098) - and a stroke that
          // moves on first cancels it.
          if (!selection) {
            upgrade = window.setTimeout(() => {
              queue.current = queue.current.catch(() => undefined).then(async () => {
                if (!current) return;
                try {
                  const full = await nativeRetouch.draftPreview(projectId, photoId, draft, replaceId, 'full');
                  if (current) setState({ image: full, pending: false, error: null, selection });
                } catch { /* The quick draft stays on screen. */ }
              });
            }, FULL_QUALITY_AFTER_MS);
          }
        } catch (error) {
          if (current) setState({ image: null, pending: false, error: asIpcError(error).message, selection });
        }
      });
    }, 350);
    return () => { current = false; window.clearTimeout(timer); window.clearTimeout(upgrade); };
  }, [projectId, photoId, draft, replaceId, enabled, revision, selection]);
  return state.selection === selection ? state : { image: null, pending: enabled, error: null };
}
