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

export type PeopleStatusDto = {
  photos: number;
  scanned: number;
  coverage: number;
  faces: number;
  votingFaces: number;
  identities: number;
  tiledFrames: number;
  coupleUnconfirmed: boolean;
  staleVersions: number[];
  erased: boolean;
  keyStore: string;
  weightsVer: number;
};

export type CompanionDto = {
  id: string;
  label: string | null;
  frames: number;
};

export type IdentityCardDto = {
  id: string;
  label: string | null;
  role: string;
  roleConfidence: number;
  roleReasons: string[];
  userLocked: boolean;
  importance: number;
  faces: number;
  votingFaces: number;
  frames: number;
  firstSeen: string | null;
  lastSeen: string | null;
  meanQuality: number;
  coverFaceId: string | null;
  variance: number;
  subCount: number;
  companions: CompanionDto[];
};

export type FaceBoxDto = {
  faceId: string;
  identityId: string | null;
  x: number;
  y: number;
  w: number;
  h: number;
  detScore: number;
  quality: number;
  blur: number;
  occlusion: number;
  yaw: number;
  pitch: number;
  roll: number;
  pxHeight: number;
  votes: boolean;
  foundBy: string;
  reasons: string[];
};

export type ProminenceEntryDto = {
  identityId: string;
  prominence: number;
};

export type ImageSubjectsDto = {
  photoId: string;
  faces: FaceBoxDto[];
  dominant: string | null;
  prominence: ProminenceEntryDto[];
  subjectFocusScore: number;
  peopleCount: number;
  weightsVer: number;
};

export type ScanFacesInput = {
  projectId: string;
};

export type ScanFacesDto = {
  scanned: number;
  faces: number;
  voting: number;
  gated: number;
  personBoxes: number;
  headless: number;
  failed: number;
  remaining: number;
  tiledFrames: number;
  tileRatio: number;
  elapsedMs: number;
  cancelled: boolean;
};

export type GroupPeopleInput = {
  projectId: string;
};

export type GroupPeopleDto = {
  identities: number;
  assigned: number;
  unassigned: number;
  threshold: number;
  refusedMerges: number;
  subClustered: number;
  couple: string[];
  coupleConfidence: number;
  coupleAmbiguous: boolean;
  sceneStarved: boolean;
  decisionsReplayed: number;
  decisionsOrphaned: number;
  coverage: number;
  elapsedMs: number;
};

export type MergeIdentitiesInput = {
  a: string;
  b: string;
};

export type SplitIdentityInput = {
  identityId: string;
  faceIds: string[];
};

export type SetIdentityRoleInput = {
  identityId: string;
  role: string;
};

export type RenameIdentityInput = {
  identityId: string;
  label: string | null;
};

export type SetIdentityImportanceInput = {
  identityId: string;
  importance: number;
};

export type IdentityHandleDto = {
  id: string;
};

export type EraseBiometricsInput = {
  projectId: string;
  confirm: string;
};

export type EraseBiometricsDto = {
  faces: number;
  identities: number;
  crops: number;
  keyRemoved: boolean;
};

export type CoverageGapDto = {
  fromMs: number;
  toMs: number;
  minutes: number;
};

export type IdentityTimelineDto = {
  identityId: string;
  firstMs: number | null;
  lastMs: number | null;
  spanMinutes: number;
  frames: number;
  gaps: CoverageGapDto[];
};

export type FaceCropDto = {
  faceId: string;
  dataUrl: string;
};

export type PeopleEvent =
  | { kind: 'scanProgress'; done: number; total: number; faces: number; tiled: boolean }
  | { kind: 'identitiesGrouped'; identities: number; facesUsed: number; threshold: number; ms: number }
  | {
      kind: 'roleInferred';
      identityId: string;
      role: string;
      confidence: number;
      evidenceKinds: number;
    }
  | { kind: 'userEdit'; action: string; identities: number };

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

// ---------------------------------------------------------------------------
// PHASE-07. The story surface. Generated from
// `crates/aura-app/src/contract/ipc.rs`; see
// `docs/adr/ADR-0016-story-ipc-surface.md`.
// ---------------------------------------------------------------------------

export type StoryOutlineInput = {
  projectId: string;
};

export type SceneScoreDto = {
  scene: string;
  score: number;
};

/**
 * What one photograph is of.
 *
 * `attributes` is a list of names rather than a bitfield, and `attributesMeasured`
 * is beside it because an empty list means "outdoors, no flash, daylight, nobody
 * around" - a description - while an unmeasured frame is not the same thing.
 */
export type SceneDto = {
  photoId: string;
  scene: string;
  sceneTitle: string;
  sceneConf: number;
  top3: SceneScoreDto[];
  attributes: string[];
  attributesMeasured: boolean;
  ritual: string | null;
  ritualConf: number;
  source: string;
  modelVer: number;
};

export type ChapterDto = {
  segmentId: string;
  ordinal: number;
  chapter: string;
  label: string | null;
  title: string;
  startMs: number;
  endMs: number;
  durationMinutes: number;
  dominantScene: string;
  confidence: number;
  keyFrame: string;
  imageCount: number;
  reasons: string[];
  userLocked: boolean;
  needsReview: boolean;
};

export type StoryOutlineDto = {
  chapters: ChapterDto[];
  coverage: number;
  needsReview: string[];
  sceneVer: number;
  taxonomyVer: number;
};

export type ImageSceneInput = {
  photoId: string;
};

export type SetChapterInput = {
  segmentId: string;
  chapter: string;
  label: string | null;
};

export type MoveBoundaryInput = {
  segmentId: string;
  newEndMs: number;
};

export type SplitChapterInput = {
  segmentId: string;
  photoId: string;
};

export type MergeChaptersInput = {
  segmentIdA: string;
  segmentIdB: string;
};

export type ChapterHandleDto = {
  id: string;
};

export type SceneProfileDto = {
  scene: string;
  title: string;
  keeperMin: number;
  keeperMax: number;
  maxAcceptableNoise: number;
  maxAcceptableBlur: number;
  subjectFocusWeight: number;
  emotionWeight: number;
  compositionWeight: number;
  editingIntent: string;
  mustCover: boolean;
  rationale: string;
};

export type ClassifyScenesInput = {
  projectId: string;
};

export type StoryStatusDto = {
  photos: number;
  classified: number;
  coverage: number;
  chapters: number;
  needsReview: number;
  locked: number;
  penalty: number;
  gapsOnly: boolean;
  sceneVer: number;
  taxonomyVer: number;
  ritualsKnown: number;
};

export type StoryEvent =
  | { kind: 'sceneClassified'; images: number; ms: number; meanConf: number; lowConfCount: number }
  | { kind: 'storySegmented'; segments: number; boundaryPenalty: number; chapters: number }
  | {
      kind: 'storyUserEdit';
      action: string;
      segment: string;
      fromLabel: string | null;
      toLabel: string | null;
    }
  | { kind: 'sceneCloudUsed'; segments: number; calls: number; costUsd: number };

// -- PHASE-08: the moments surface -------------------------------------------
//
// Nine commands, eight types and one event. Nothing here can reject a
// photograph: five commands change a grouping, one moves a hint, three are
// reads. Section 2.2 puts every question about a photograph's fate in phase 12.

export type MomentsInput = {
  projectId: string;
  segmentId: string | null;
};

export type MomentFrameDto = {
  photoId: string;
  position: number;
  burstIx: number;
  suppressed: boolean;
};

export type MomentDto = {
  momentId: string;
  segmentId: string | null;
  cover: string;
  frames: MomentFrameDto[];
  frameCount: number;
  burstCount: number;
  cameraCount: number;
  startMs: number;
  endMs: number;
  durationS: number;
  diversity: number;
  suggestedKeepers: number;
  confidence: number;
  reasons: string[];
  userLocked: boolean;
  duplicateSets: number;
};

export type MomentListDto = {
  moments: MomentDto[];
  coverage: number;
  embedVer: number;
  groupVer: number;
  profileVer: number;
};

export type MomentOfImageInput = {
  photoId: string;
};

export type DuplicateSetDto = {
  kind: string;
  photoIds: string[];
  keepHint: string;
  confidence: number;
  reasons: string[];
  userChosen: boolean;
  capsGallery: boolean;
};

export type GroupMomentsInput = {
  projectId: string;
};

export type MomentStatusDto = {
  photos: number;
  groupable: number;
  grouped: number;
  coverage: number;
  moments: number;
  locked: number;
  meanSize: number;
  bursts: number;
  duplicates: [number, number, number];
  medianIntervalMs: number;
  implausible: boolean;
  embedVer: number;
  groupVer: number;
  profileVer: number;
};

export type SplitMomentInput = {
  momentId: string;
  photoId: string;
};

export type MergeMomentsInput = {
  momentIdA: string;
  momentIdB: string;
};

export type LockMomentInput = {
  momentId: string;
  locked: boolean;
};

export type SetKeepHintInput = {
  momentId: string;
  photoId: string;
};

export type MomentHandleDto = {
  id: string;
};

export type MomentEditDto = {
  action: string;
  momentId: string;
  otherId: string | null;
  photoId: string | null;
  momentSize: number;
};

export type MomentEvent =
  | { kind: 'momentsBuilt'; images: number; moments: number; bursts: number; meanSize: number; ms: number }
  | { kind: 'duplicatesFound'; identical: number; nearIdentical: number; variant: number }
  | { kind: 'momentsUserEdit'; action: string; momentSize: number };

// ---------------------------------------------------------------------------
// PHASE-09. The integrity surface: what is technically wrong with a photograph,
// where it is wrong, and what the photographer may say back.
//
// Frozen alongside `crates/aura-app/src/contract/ipc.rs`; see
// `docs/adr/ADR-0020-integrity-ipc-surface.md`.
//
// **Nothing in these shapes is a decision about delivery.** `technicalScore` is
// a measurement and `flags` are measurements; a view that sorted a delivery by
// either would be making phase 12's decision three phases early.
// ---------------------------------------------------------------------------

/** One rectangle in a photograph, normalised to the frame. */
export type CropRectDto = {
  x: number;
  y: number;
  w: number;
  h: number;
};

/** One thing that moved a frame's score, with the pixels that prove it. */
export type IntegrityReasonDto = {
  /** The stable slug. `docs/frame-integrity.md` documents every one. */
  code: string;
  /** The sentence a photographer reads. */
  text: string;
  /** Negative for a penalty, zero or positive for an exoneration. */
  weight: number;
  /**
   * True when this reason withdraws a claim rather than making one.
   *
   * Sent by the backend rather than derived here from a list of codes, because
   * which reasons are the good news is exactly the thing an interface must not
   * work out for itself.
   */
  exoneration: boolean;
  /** The crop to show, or null when the reason is about the whole frame. */
  evidence: CropRectDto | null;
};

/** One face's eyes, as the card lists them. */
export type EyeStateDto = {
  faceId: string;
  identityId: string | null;
  /** `open`, `squint`, `closed`, `looking_down` or `occluded`. */
  state: string;
  confidence: number;
  /** True when the scene, the expression or the partner justifies a closure. */
  intentional: boolean;
  /** True when this face's eyes decide anything about the frame. */
  gates: boolean;
  areaFrac: number;
  crop: CropRectDto;
};

/** One photograph's technical verdict. */
export type IntegrityDto = {
  photoId: string;
  subjectSharpness: number;
  bgSharpness: number;
  /** Negative is front focus, positive is back focus. */
  focusOffset: number;
  /** One is the sharpest of its moment; 0.5 means it has no siblings. */
  relativeSharpness: number;
  /** `none`, `camera_shake`, `subject_motion` or `intentional`. */
  motion: string;
  motionSeverity: number;
  /** `good`, `recoverable`, `marginal` or `lost`. */
  exposure: string;
  clipHi: number;
  clipLo: number;
  /** Stops from a correct exposure. Negative is under. */
  evOffset: number;
  /** Noise relative to what this scene tolerates. **1.0 is the tolerance.** */
  noiseSigmaRel: number;
  closedEyeRatio: number;
  /** The denominator of the ratio above. Zero of zero is not zero of six. */
  gatingFaces: number;
  technicalScore: number;
  scene: string;
  /** The set flags, as slugs. */
  flags: string[];
  /** True when at least one flag describes a defect. */
  hasDefect: boolean;
  reasons: IntegrityReasonDto[];
  eyes: EyeStateDto[];
  confidence: number;
  userReviewed: boolean;
  modelVer: number;
  analysisVer: number;
  calibVer: number;
};

/** What the Integrity panel's header shows. */
export type IntegrityStatusDto = {
  photos: number;
  scored: number;
  /** Denominator: **every photograph**, unlike the moments view's. */
  coverage: number;
  /** Fraction of scored frames that had a subject to be judged against. */
  subjectAware: number;
  reviewed: number;
  /** How many frames carry each flag, in the same order as `flagNames`. */
  flagCounts: number[];
  flagNames: string[];
  meanScore: number;
  /** An upper bound: one frame can be soft *and* noisy. */
  defectiveAtMost: number;
  uncalibrated: string[];
  modelVer: number;
  analysisVer: number;
  calibVer: number;
};

export type FlaggedInput = {
  projectId: string;
  flags: string[];
  limit?: number;
};

export type WithinMomentInput = {
  momentId: string;
};

export type RankedFrameDto = {
  photoId: string;
  relativeSharpness: number;
};

export type DismissFlagInput = {
  photoId: string;
  /** Exactly one flag slug. */
  flag: string;
};

export type AnalyseIntegrityInput = {
  projectId: string;
  cancelId?: string;
};

export type IntegrityPassDto = {
  scored: number;
  failed: number;
  faces: number;
  closed: number;
  closedOk: number;
  meanScore: number;
  uncalibrated: string[];
  elapsedMs: number;
  cancelled: boolean;
};

export type IntegrityEvent =
  | {
      kind: 'integrityScored';
      images: number;
      ms: number;
      meanScore: number;
      flagHistogram: number[];
    }
  | { kind: 'integrityEyes'; faces: number; closed: number; closedOk: number; squint: number }
  | { kind: 'integrityCameraUncalibrated'; make: string; model: string };
