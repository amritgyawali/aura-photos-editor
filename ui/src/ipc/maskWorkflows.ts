import { invoke } from '@tauri-apps/api/core';
import type { LocalMasksDto } from './localMasks';

export type MaskWorkflowAction = 'duplicate' | 'invert' | 'union' | 'intersection' | 'subtraction' | 'freeze' | 'feather' | 'expand' | 'contract' | 'edge_contrast' | 'remove_islands' | 'fill_holes';
export type MaskWorkflowRequest = { action: MaskWorkflowAction; otherMaskId?: string; value?: number };
export const maskWorkflows = {
  apply: (projectId: string, photoId: string, maskId: string, recipeHash: string, request: MaskWorkflowRequest, cancelId: string) =>
    invoke<LocalMasksDto>('mask_workflow', { input: { projectId, photoId, maskId, recipeHash, cancelId, action: request.action, otherMaskId: request.otherMaskId ?? null, value: request.value ?? 0 } }),
};
