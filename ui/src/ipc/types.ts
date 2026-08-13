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

export type ProviderNoteDto = {
  ep: string;
  reason: string;
};

export type ProbeScoreDto = {
  ep: string;
  medianMs: number;
};

export type HardwarePlanDto = {
  gpu: string | null;
  epOrder: string[];
  selectedEp: string;
  overrideEp: string | null;
  unavailable: ProviderNoteDto[];
  setAside: ProviderNoteDto[];
  vramBudgetMb: number;
  cpuThreads: number;
  probeScoresMs: ProbeScoreDto[];
  probedAt: string;
  probed: boolean;
};

export type ModelStatusDto = {
  name: string;
  version: string;
  task: string;
  activeVersion: string | null;
  pendingVersion: string | null;
  rejectedVersions: string[];
  modelCard: string;
  workingSetMb: number;
  fileCount: number;
  int8Forbidden: boolean;
};

export type WarmupReportDto = {
  loaded: number;
  elapsedMs: number;
  epUsed: string;
};

export type InferStatsDto = {
  residentSessions: number;
  poolHits: number;
  poolLoads: number;
  requests: number;
  downshifts: number;
  meanOverheadMs: number;
  peakMemoryMb: number;
};

export type SetExecutionProviderInput = {
  ep: string;
};

export type InferEvent =
  | { kind: 'warmupProgress'; done: number; total: number; model: string }
  | { kind: 'planChanged'; selectedEp: string }
  | { kind: 'modelRejected'; name: string; code: string; message: string };

export type CloudStatusDto = {
  provider: string;
  endpoint: string;
  keyPresent: boolean;
  keyFingerprint: string;
  keyStore: string;
  offlineStudioMode: boolean;
  projectEnabled: boolean;
  blurFaces: boolean;
  transport: string;
  breakerReason: string | null;
  tierModels: string[];
};

export type SetAiKeyInput = {
  provider: string;
  key: string;
  endpoint: string | null;
};

export type KeyCheckDto = {
  ok: boolean;
  model: string;
  message: string;
};

export type SetCloudBudgetInput = {
  projectId: string;
  capUsd: number;
  monthCapUsd: number;
  hardStop: boolean;
};

export type SetCloudPrivacyInput = {
  projectId: string;
  enabled: boolean;
  offlineStudioMode: boolean;
  blurFaces: boolean;
};

export type CloudSpendDto = {
  capUsd: number;
  spentUsd: number;
  monthCapUsd: number;
  monthSpentUsd: number;
  calls: number;
  downgrades: number;
  fallbacks: number;
  cacheHitRate: number;
  stopped: boolean;
};

export type CloudCallDto = {
  id: string;
  task: string;
  taskVersion: number;
  model: string;
  source: string;
  fallbackReason: string | null;
  tokensIn: number;
  tokensOut: number;
  costUsd: number;
  latencyMs: number;
  status: string;
  retryCount: number;
  promptHash: string;
  confidence: number;
  decisionRef: string | null;
};

export type CloudCacheStatsDto = {
  entries: number;
  bytes: number;
  hits: number;
};

export type CloudEvent =
  | { kind: 'call'; task: string; model: string; costUsd: number; latencyMs: number; status: string }
  | { kind: 'fallback'; task: string; reason: string }
  | { kind: 'budgetStop'; capUsd: number; spentUsd: number }
  | { kind: 'cache'; hitRate: number; entries: number; bytes: number };

export type FindSimilarInput = {
  projectId: string;
  photoId: string;
  k: number;
  timeWindowS: number | null;
  cameraId: string | null;
  exclude: string[];
};

export type SimilarNeighbourDto = {
  photoId: string;
  distance: number;
  similarity: number;
  dhashDistance: number;
  nearDuplicate: boolean;
};

export type SimilarResultDto = {
  photoId: string;
  neighbours: SimilarNeighbourDto[];
  elapsedMs: number;
  filterKind: string;
};

export type IndexStatusDto = {
  vectors: number;
  photos: number;
  coverage: number;
  filterable: number;
  modelVer: number;
  staleModelVersions: number[];
  buildMs: number;
  fromSnapshot: boolean;
};

export type EmbedProjectInput = {
  projectId: string;
};

export type EmbedProgressDto = {
  embedded: number;
  failed: number;
  remaining: number;
  elapsedMs: number;
  batches: number;
  cancelled: boolean;
};

export type DescriptorsDto = {
  photoId: string;
  dhashHex: string;
  lumaMean: number;
  lumaP1: number;
  lumaP50: number;
  lumaP99: number;
  clipLo: number;
  clipHi: number;
  edgeEnergy: number;
  palette: string[];
  modelVer: number;
  pixelSource: string;
};

export type IndexEvent =
  | { kind: 'embedProgress'; done: number; total: number; batchSize: number; ep: string }
  | { kind: 'indexBuilt'; vectors: number; ms: number; snapshotUsed: boolean }
  | { kind: 'queryTimed'; k: number; ms: number; filterKind: string };

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
