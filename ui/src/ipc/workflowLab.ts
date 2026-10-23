import { invoke } from './client';

export interface ReviewRecord { recipeHash: string; passed: string[]; reviewer: string; dueDate: string }
export interface ReviewIssue { id: string; photoId: string; recipeHash: string; text: string; point: [number, number] | null; resolved: boolean; dependsOn: string[] }
export interface ReviewComparison { left: string; right: string; leftHash: string; rightHash: string; winner: string; note: string }
export interface ReviewFilter { name: string; reviewer: string; state: string; overdueOnly: boolean; issuesOnly: boolean }
export interface WorkflowRow { photoId: string; changed: string[]; conflicts: string[] }
export interface WorkflowPhoto { photoId: string; recipeHash: string; status: string; openIssues: number; overdue: boolean; fingerprint: string; sections: Record<string, string> }
export interface WorkflowSnapshot {
  revision: string;
  queue: string[];
  criteria: string[];
  reviews: Record<string, ReviewRecord>;
  issues: ReviewIssue[];
  comparisons: ReviewComparison[];
  filters: ReviewFilter[];
  checkpoints: Array<{ id: string; name: string; at: string; count: number }>;
  plan: { kind: string; entries: WorkflowRow[] } | null;
  audit: Array<{ at: string; action: string; photos: string[] }>;
  photos: WorkflowPhoto[];
  rows: WorkflowRow[];
  selected: string[];
  document: string | null;
  message: string;
}
export type WorkflowAction = 'read' | 'audit' | 'fingerprints' | 'matrix' | 'checkpoint' | 'remove_checkpoint' | 'drift'
  | 'restore_plan' | 'rollback_plan' | 'delta_plan' | 'merge_plan' | 'apply_plan' | 'verify' | 'queue' | 'criteria'
  | 'signoff' | 'issue' | 'resolve_issue' | 'propagate_issue' | 'assign' | 'schedule' | 'compare' | 'undecided'
  | 'filter' | 'apply_filter' | 'handoff' | 'export' | 'import';
export interface WorkflowRequest {
  projectId: string;
  action: WorkflowAction;
  expectedRevision?: string;
  photoIds: string[];
  recipeHashes?: Record<string, string>;
  sections?: string[];
  name?: string;
  checkpointId?: string;
  sourcePhotoId?: string;
  issueId?: string;
  text?: string;
  reviewer?: string;
  dueDate?: string;
  reviewDate?: string;
  winner?: string;
  criterion?: string;
  passed?: boolean;
  dependsOn?: string[];
  point?: [number, number];
  filter?: ReviewFilter;
  document?: string;
}
export const workflowLab = { run: (input: WorkflowRequest) => invoke<WorkflowSnapshot>('workflow_lab', { input }) };
