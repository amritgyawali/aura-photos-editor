import { invoke } from './client';
import type { FramingContext } from '../components/develop/framingMath';
import type { RenderDto } from './types';
const input = (projectId: string, photoId: string, recipeHash: string) => ({ input: { projectId, photoId, recipeHash } });
export const framingWorkflows = {
  context: (projectId: string, photoId: string, recipeHash: string) => invoke<FramingContext>('framing_context', input(projectId, photoId, recipeHash)),
  colorOnly: (projectId: string, photoId: string, recipeHash: string) => invoke<RenderDto>('color_only_preview', input(projectId, photoId, recipeHash)),
};
