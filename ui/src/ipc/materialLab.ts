import { invoke } from './client';
import type { RecipeDto } from './types';
export const MATERIAL_TOOLS = [
  ['directional_smooth', 'Directional hair smoothing', 'Smooth along the strand direction while protecting strong edges.'],
  ['silver_hair', 'Silver hair darkening', 'Darken bright, low-chroma strands with local highlight protection.'],
  ['fabric_bands', 'Fabric frequency balance', 'Reduce coarse fabric unevenness while controlling fine weave retention.'],
  ['luma_moire', 'Luminance moiré notch', 'Reduce a repeating brightness pattern along a chosen direction and spacing.'],
  ['deband', 'Flat-area debanding', 'Soften small tonal steps while protecting stronger contrast.'],
  ['chroma_median', 'Median chroma repair', 'Remove isolated colour variation while retaining luminance detail.'],
  ['impulse_repair', 'Isolated pixel repair', 'Replace isolated bright or dark impulses surrounded by consistent pixels.'],
  ['line_scratch', 'Line scratch repair', 'Repair thin horizontal or vertical scratches from agreeing pixels across the line.'],
  ['destripe', 'Row and column stripe repair', 'Subtract measured repeating row or column brightness bias.'],
  ['checkerboard', 'Checkerboard suppression', 'Remove measured two-pixel parity offsets in brightness.'],
  ['ringing', 'Edge ringing suppression', 'Limit overshoot near contrast edges using neighborhood quartiles.'],
  ['specular_chroma', 'Specular colour recovery', 'Borrow surrounding diffuse colour for near-neutral bright reflections.'],
  ['chroma_mottle', 'Colour mottle removal', 'Suppress intermediate-scale colour blotches while retaining fine detail.'],
  ['illumination_plane', 'Backdrop gradient leveling', 'Fit and subtract a bounded smooth lighting slope from the selected area.'],
  ['channel_detail', 'Cross-channel detail reconstruction', 'Rebuild inconsistent channel detail from the shared median residual.'],
] as const;
export type MaterialTool = typeof MATERIAL_TOOLS[number][0];
export type MaterialOperation = { id: string; tool: MaterialTool; maskId: string; enabled: boolean; amount: number; radius: number; angle: number; threshold: number; detail: number; coarse: number };
export function materialsOf(recipe: RecipeDto | null): MaterialOperation[] {
  try { const value = (JSON.parse(recipe?.body ?? '{}') as { material_lab_v1?: MaterialOperation[] }).material_lab_v1; return Array.isArray(value) ? value : []; } catch { return []; }
}
export const materialLab = {
  save: (projectId: string, photoId: string, recipeHash: string, operations: MaterialOperation[]) => invoke<{ recipe: RecipeDto }>('save_material_lab', { input: { projectId, photoId, recipeHash, operations } }),
};
