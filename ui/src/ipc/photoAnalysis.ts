import { invoke } from './client';

export interface PhotoDiagnostic {
  id: string;
  title: string;
  description: string;
  kind: 'map' | 'density' | 'bars';
  unit: string;
  width: number;
  height: number;
  values: number[];
  labels: string[];
  low: number;
  high: number;
  samples: number;
}
export interface PhotoAnalysisReport {
  recipeHash: string;
  width: number;
  height: number;
  diagnostics: PhotoDiagnostic[];
  notes: string[];
}
export const photoAnalysis = {
  inspect: (projectId: string, photoId: string, recipeHash: string) =>
    invoke<PhotoAnalysisReport>('inspect_photo_diagnostics', { input: { projectId, photoId, recipeHash } }),
};
