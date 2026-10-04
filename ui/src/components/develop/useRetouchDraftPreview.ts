import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { nativeRetouch, type NativeRetouchEdit } from '../../ipc/nativeRetouch';
import type { RenderDto } from '../../ipc/types';

/** At most one native draft render at a time. Superseded drafts never reach the screen. */
export function useRetouchDraftPreview(projectId: string, photoId: string,
  draft: NativeRetouchEdit, replaceId: string | null, enabled: boolean, revision: number, selection = false) {
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const [state, setState] = useState<{ image: Pick<RenderDto, 'width'|'height'|'rgbBase64'> | null; pending: boolean; error: string | null; selection: boolean }>({
    image: null, pending: false, error: null, selection,
  });
  useEffect(() => {
    let current = true;
    setState({ image: null, pending: enabled, error: null, selection });
    if (!enabled) return;
    const timer = window.setTimeout(() => {
      queue.current = queue.current.catch(() => undefined).then(async () => {
        if (!current) return;
        try {
          const image = await (selection ? nativeRetouch.selectionPreview : nativeRetouch.draftPreview)(projectId, photoId, draft, replaceId);
          if (current) setState({ image, pending: false, error: null, selection });
        } catch (error) {
          if (current) setState({ image: null, pending: false, error: asIpcError(error).message, selection });
        }
      });
    }, 350);
    return () => { current = false; window.clearTimeout(timer); };
  }, [projectId, photoId, draft, replaceId, enabled, revision, selection]);
  return state.selection === selection ? state : { image: null, pending: enabled, error: null };
}
