import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { nativeRetouch, type NativeRetouchEdit } from '../../ipc/nativeRetouch';
import type { RenderDto } from '../../ipc/types';

/** At most one native draft render at a time. Superseded drafts never reach the screen. */
export function useRetouchDraftPreview(projectId: string, photoId: string,
  draft: NativeRetouchEdit, replaceId: string | null, enabled: boolean, revision: number) {
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const [state, setState] = useState<{ image: RenderDto | null; pending: boolean; error: string | null }>({
    image: null, pending: false, error: null,
  });
  useEffect(() => {
    let current = true;
    setState({ image: null, pending: enabled, error: null });
    if (!enabled) return;
    const timer = window.setTimeout(() => {
      queue.current = queue.current.catch(() => undefined).then(async () => {
        if (!current) return;
        try {
          const image = await nativeRetouch.draftPreview(projectId, photoId, draft, replaceId);
          if (current) setState({ image, pending: false, error: null });
        } catch (error) {
          if (current) setState({ image: null, pending: false, error: asIpcError(error).message });
        }
      });
    }, 350);
    return () => { current = false; window.clearTimeout(timer); };
  }, [projectId, photoId, draft, replaceId, enabled, revision]);
  return state;
}
