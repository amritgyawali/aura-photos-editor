import { useEffect, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { nativeRetouch, type SelectionPreview } from '../../ipc/nativeRetouch';

/** Coverage belongs to a photo, recipe revision and operation. Never show an old response
 * over a new photo or stack, including while a new request is still in flight. */
export function useSavedRetouchCoverage(projectId: string, photoId: string, revision: string,
  operationId: string | null, enabled: boolean) {
  const key = JSON.stringify([projectId, photoId, revision, operationId]);
  const [result, setResult] = useState<{key: string; image?: SelectionPreview; error?: string} | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    setResult(null);
    void nativeRetouch.savedSelection(projectId, photoId, operationId)
      .then(image => { if (active) setResult({key, image}); })
      .catch(cause => { if (active) setResult({key, error: asIpcError(cause).message}); });
    return () => { active = false; };
  }, [projectId, photoId, key, operationId, enabled]);
  const current = enabled && result?.key === key ? result : null;
  return { image: current?.image ?? null, error: current?.error ?? null, pending: enabled && !current };
}
