// FROZEN CONTRACT. Mirrors crates/aura-app/src/contract/ipc.rs.
// Both files are digested in contracts.lock; changing one without the other
// fails CI, and a shape change needs an ADR.

export type CreateProjectInput = {
  name: string;
  coupleNames: string | null;
  eventDate: string | null;
};

export type ProjectHandle = {
  id: string;
};

export type ProjectSummary = {
  id: string;
  name: string;
  eventDate: string | null;
  photoCount: number;
};

export type StartIngestInput = {
  projectId: string;
  roots: string[];
};

export type JobHandle = {
  jobId: string;
};

export type ListImagesInput = {
  projectId: string;
  offset: number;
  limit: number;
  orderBy: string | null;
};

export type ImageRowLite = {
  id: string;
  fileName: string;
  timelineTs: string | null;
  cameraId: string | null;
  width: number;
  height: number;
  status: string;
};

export type SetCameraLabelInput = {
  cameraId: string;
  shooterLabel: string;
  clockOffsetMs: number;
};

export type ProblemRow = {
  path: string;
  code: string;
  message: string;
};

export type GetPreviewInput = {
  projectId: string;
  photoId: string;
  level: string;
  priority: string;
};

export type PreviewPayload = {
  photoId: string;
  tier: number;
  width: number;
  height: number;
  source: string;
  dataUrl: string;
};

export type PrefetchInput = {
  projectId: string;
  photoIds: string[];
  level: string;
};

export type CacheStatsDto = {
  bytesUsed: number;
  budgetBytes: number;
  entries: number;
  hits: number;
  misses: number;
  evictions: number;
  hitRate: number;
};

export type SetCacheBudgetInput = {
  projectId: string;
  budgetBytes: number;
};

export type PreviewEvent =
  | { kind: 'ready'; photoId: string; tier: number }
  | { kind: 'failed'; photoId: string; code: string; message: string }
  | { kind: 'cacheStats'; bytesUsed: number; budgetBytes: number; hitRate: number };

export type IpcError = {
  code: string;
  message: string;
  runbookUrl: string;
  retryable: boolean;
};

export type IngestEvent =
  | { kind: 'discovered'; totalHint: number }
  | { kind: 'progress'; done: number; total: number; current: string }
  | { kind: 'batch'; rows: ImageRowLite[] }
  | { kind: 'warning'; code: string; message: string }
  | { kind: 'finished'; inserted: number; skipped: number; failed: number; elapsedMs: number };
