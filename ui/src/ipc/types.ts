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

// ---------------------------------------------------------------------------
// PHASE-10. The emotion surface: what a photograph is worth, and why.
//
// Frozen; see `docs/adr/ADR-0022-emotion-ipc-surface.md`. Seven commands, five of
// them reads, and the two that change anything are both the photographer telling
// the product it is wrong.
//
// **Nothing here keeps, delivers or builds a gallery.** `rankedByEmotion` returns
// an ordering, which is this phase's headline feature; section 2.2 puts the
// choosing in phase 12. An ordering looks even more like a shortlist than phase
// 09's score did, which is why the boundary is restated on the types.
// ---------------------------------------------------------------------------

export type EmotionReasonDto = {
  /** The stable slug. `docs/emotion-and-moments.md` documents every one. */
  code: string;
  text: string;
  /**
   * Positive for something the frame earned, negative for something it cost.
   *
   * The opposite sign convention from `IntegrityReasonDto`, and that is the two
   * phases rather than an inconsistency: a technical verdict explains penalties
   * and an emotion score explains what it found.
   */
  weight: number;
  /**
   * True when this is a note about the reading rather than about the photograph.
   *
   * Sent rather than derived from the slug, so the panel and the harness cannot
   * disagree about which reasons are caveats.
   */
  caveat: boolean;
  evidence?: CropRectDto | null;
};

export type FaceExpressionDto = {
  faceId: string;
  identityId?: string | null;
  /** The eight continuous channels, in `channelNames` order. */
  channels: number[];
  /** `unknown`, `camera`, `partner`, `officiant` or `away`. */
  gaze: string;
  confidence: number;
  /**
   * True when the tear reading is above the certainty gate.
   *
   * Sent rather than compared here against a threshold this file would then own.
   * Section 12's fourth failure mode is a false tear.
   */
  readsAsCrying: boolean;
  posedSmile: boolean;
  crop: CropRectDto;
};

export type InteractionDto = {
  kind: string;
  /** The photographer-facing label, for the chip. */
  title: string;
  strength: number;
  /** True for the four milestones a client buys a print of. */
  milestone: boolean;
};

export type EmotionDto = {
  photoId: string;
  faces: FaceExpressionDto[];
  /**
   * The names of the eight channels, in `FaceExpressionDto.channels` order.
   *
   * Sent once per reading rather than hard-coded here, so the order can never
   * drift between the model, the store and the bars a photographer looks at.
   */
  channelNames: string[];
  interactions: InteractionDto[];
  mutualGaze: boolean;
  peakProximity: number;
  reactionOf?: string | null;
  /**
   * The scene-weighted, calibrated composite.
   *
   * **Not a keep decision.** A frame at 0.22 may be the only photograph of the
   * ring exchange.
   */
  emotionScore: number;
  narrativeWeight: number;
  scene: string;
  reasons: EmotionReasonDto[];
  confidence: number;
  /** `local`, `cloud` or `user`. */
  source: string;
  modelVer: number;
  analysisVer: number;
  weightsVer: number;
};

export type MomentPeakDto = {
  momentId: string;
  photoId: string;
  index: number;
  frames: number;
  margin: number;
  /** `expression`, `kiss_apex`, `tear_release`, `bouquet_in_air`, `ring_slide` or `flat`. */
  kind: string;
  /**
   * True when the margin cleared the floor and the kind is not `flat`.
   *
   * A moment with no separated peak is a common and correct answer - a bracketed
   * detail shot has no apex - so the indicator draws "no clear best frame" rather
   * than pointing at a rounding error.
   */
  resolved: boolean;
  confidence: number;
  reasons: EmotionReasonDto[];
  userChosen: boolean;
};

export type ReactionLinkDto = {
  action: string;
  reaction: string;
  /** Signed: negative when the reaction frame is earlier than the action frame. */
  gapMs: number;
  bonus: number;
  confidence: number;
  reasons: EmotionReasonDto[];
};

export type RankedByEmotionDto = {
  photoId: string;
  emotionScore: number;
};

export type EmotionStatusDto = {
  photos: number;
  scored: number;
  /** Denominator: every photograph. */
  coverage: number;
  /**
   * Fraction of scored frames that carried at least one face.
   *
   * The second number, and the one that matters most when it is low: seven of the
   * nine ranker terms come from faces.
   */
  faceAware: number;
  moments: number;
  peaked: number;
  peakRate: number;
  links: number;
  /** How many frames carry each interaction, in `interactionNames` order. */
  interactionCounts: number[];
  interactionNames: string[];
  meanScore: number;
  meanMargin: number;
  preferences: number;
  modelVer: number;
  analysisVer: number;
  weightsVer: number;
};

export type RankedInput = {
  projectId: string;
  limit?: number;
};

export type PreferInput = {
  winnerId: string;
  loserId: string;
};

export type SetPeakInput = {
  momentId: string;
  photoId: string;
};

export type ScoreEmotionInput = {
  projectId: string;
  cancelId?: string;
};

export type EmotionPassDto = {
  scored: number;
  failed: number;
  faces: number;
  moments: number;
  peaked: number;
  links: number;
  meanScore: number;
  elapsedMs: number;
  cancelled: boolean;
};

export type EmotionEvent =
  | {
      kind: 'emotionScored';
      images: number;
      ms: number;
      meanScore: number;
      interactionHistogram: number[];
    }
  | { kind: 'emotionPeaks'; moments: number; meanMargin: number }
  | { kind: 'emotionReactions'; links: number; meanBonus: number }
  | { kind: 'emotionCloudUsed'; calls: number; costUsd: number };

// ---------------------------------------------------------------------------
// PHASE-11. How a photograph is framed, why, and where the evidence is.
//
// These shapes mirror `CompositionResult` rather than approximating it in the
// interface. In particular, the backend says which reasons are exonerations,
// which cuts are flagged, and whether a crop hint is actionable. The UI does not
// own second copies of those rules.
// ---------------------------------------------------------------------------

export type CompositionJointCutDto = {
  /** `neck`, `shoulder`, `elbow`, `wrist`, `hip`, `knee` or `ankle`. */
  joint: string;
  /** `top`, `right`, `bottom` or `left`. */
  edge: string;
  /** True when the edge lands on the joint rather than between joints. */
  atJoint: boolean;
  /** Scene-conditioned cost, from zero to one. */
  severity: number;
  /** The backend's threshold decision; never re-derived here. */
  flagged: boolean;
  area: CropRectDto;
};

export type CompositionReasonDto = {
  code: string;
  /** The exact photographer-facing sentence produced by the analyser. */
  text: string;
  /** Negative for a penalty; zero or positive for an exoneration. */
  weight: number;
  /** True when this reason withdraws a claim rather than making one. */
  exoneration: boolean;
  /** Null only for a frame-wide reason. */
  evidence: CropRectDto | null;
};

/** A target for phase 23. This phase never applies it. */
export type CompositionCropHintDto = {
  region: CropRectDto;
  safeMargin: number;
  straightenDeg: number | null;
  confidence: number;
  actionable: boolean;
};

/** Every field in the frozen `CompositionResult`. */
export type CompositionDto = {
  photoId: string;
  /** Degrees off level. Positive is clockwise. */
  tiltDeg: number;
  tiltIntentional: boolean;
  horizonConf: number;
  /** `none`, `gradient`, `vanishing_lines` or `gravity`. */
  horizonSource: string;
  /** Space above the subject as a fraction of frame height. */
  headroom: number;
  /** Distance from the nearest rule-of-thirds power point. */
  thirdsOffset: number;
  balance: number;
  negativeSpace: number;
  jointCuts: CompositionJointCutDto[];
  headCrop: boolean;
  edgeIntrusions: CropRectDto[];
  /** Scene-relative clutter; 1.0 is the scene's tolerance. */
  clutter: number;
  brightBlobs: CropRectDto[];
  headMerge: boolean;
  colourCompetition: number;
  aesthetic: number;
  compositionScore: number;
  cropSuggestionHint: CompositionCropHintDto | null;
  scene: string;
  relativeComposition: number;
  keypointSubjects: number;
  flags: string[];
  /** True when at least one flag is a framing violation. */
  hasViolation: boolean;
  reasons: CompositionReasonDto[];
  confidence: number;
  userReviewed: boolean;
  modelVer: number;
  analysisVer: number;
  rulesVer: number;
};

export type CompositionStatusDto = {
  photos: number;
  scored: number;
  /** Denominator: every photograph in the project. */
  coverage: number;
  /** Fraction of scored frames whose subjects had keypoints. */
  keypointAware: number;
  flagCounts: number[];
  flagNames: string[];
  meanScore: number;
  /** Denominator: frames with a measurable horizon. */
  meanAbsTilt: number;
  intentionalRatio: number;
  hinted: number;
  reviewed: number;
  /** An upper bound because one frame may carry several flags. */
  violatingAtMost: number;
  unruledScenes: string[];
  modelVer: number;
  analysisVer: number;
  rulesVer: number;
};

export type FlaggedCompositionInput = {
  projectId: string;
  flags: string[];
  limit?: number;
};

export type DismissCompositionFlagInput = {
  photoId: string;
  /** Exactly one violation flag slug. */
  flag: string;
};

export type AnalyseCompositionInput = {
  projectId: string;
  cancelId?: string;
};

export type CompositionPassDto = {
  scored: number;
  failed: number;
  keypointSubjects: number;
  cut: number;
  intentionalTilts: number;
  horizons: number;
  meanAbsTilt: number;
  flagCounts: number[];
  flagNames: string[];
  meanScore: number;
  hinted: number;
  unruledScenes: string[];
  elapsedMs: number;
  /** Completed rows remain saved when this is true. */
  cancelled: boolean;
};

// ---------------------------------------------------------------------------
// PHASE-12. What is being delivered, why, and what the photographer may say
// back.
//
// Every meaning on this surface is the backend's. `satisfied`, `protected`,
// `keep`, `veto` and `vetoed` are booleans the engine computed, not predicates
// the interface reimplemented - because a web view that decided for itself what
// `covered_weak` meant would be a web view that could tell a photographer their
// gallery was complete when it was not.
//
// There is deliberately no delete, move, export or upload on this surface.
// ---------------------------------------------------------------------------

export type CullReasonDto = {
  code: string;
  /** The exact sentence the engine produced. Render it; do not rebuild it. */
  text: string;
  /** Positive keeps, negative rejects. */
  weight: number;
  keep: boolean;
  /** True when it fired before any arithmetic: section 6.1's hard vetoes. */
  veto: boolean;
};

export type SelectedDto = {
  photoId: string;
  momentId: string | null;
  keepScore: number;
  confidence: number;
  reasons: CullReasonDto[];
  /** The best alternative from the same moment that is not itself delivered. */
  runnerUp: string | null;
  coverageRole: string | null;
  /** True when a guarantee holds this frame, so the size slider may not drop it. */
  protected: boolean;
};

export type RejectedDto = {
  photoId: string;
  momentId: string | null;
  /** Zero when a veto fired: a veto replaced the score rather than lowering it. */
  keepScore: number;
  /** Never empty. Invariant 2, and section 10.1's last criterion. */
  reasons: CullReasonDto[];
  keptInstead: string | null;
  wasPeak: boolean;
  vetoed: boolean;
};

export type CoverageRuleDto = {
  rule: string;
  title: string;
  /** `covered`, `covered_weak` or `missing`. */
  state: string;
  satisfied: boolean;
};

export type IdentityCoverageDto = {
  identityId: string;
  frames: number;
};

export type ChapterCountDto = {
  chapter: string;
  title: string;
  delivered: number;
  target: number;
};

export type CoverageReportDto = {
  mustHaves: CoverageRuleDto[];
  /** Includes the zeros: the zero is the number the panel exists to show. */
  identityCoverage: IdentityCoverageDto[];
  chapters: ChapterCountDto[];
  warnings: string[];
};

export type SelectionDto = {
  selected: SelectedDto[];
  rejected: RejectedDto[];
  coverage: CoverageReportDto;
  targetCount: number;
  /** May exceed `targetCount`: coverage runs last and a guarantee outranks a slider. */
  actualCount: number;
  mode: string;
  /** Hex, because JavaScript cannot hold a 64-bit integer exactly. */
  deterministicHash: string;
  modelVer: number;
  analysisVer: number;
  /** `0` is the unfitted identity calibration this build ships. */
  calibrationVer: number;
};

export type CullStatusDto = {
  photos: number;
  eligible: number;
  selected: number;
  /** Denominator: every photograph. The most consequential number in this phase. */
  coverage: number;
  emotionAware: number;
  compositionAware: number;
  grouped: number;
  covered: number;
  coveredWeak: number;
  missing: number;
  userKept: number;
  userRejected: number;
  mode: string;
  deterministicHash: string;
  modelVer: number;
  analysisVer: number;
  calibrationVer: number;
};

export type DecisionDto = {
  kept: boolean;
  selected: SelectedDto | null;
  rejected: RejectedDto | null;
};

export type CullProjectInput = {
  projectId: string;
  /** Absent keeps the stored mode. */
  mode?: string;
  /** Absent asks the size model to predict one. */
  target?: number;
  cancelId?: string;
};

export type ResizeGalleryInput = {
  projectId: string;
  target: number;
};

export type SetCullModeInput = {
  projectId: string;
  mode: string;
};

export type OverrideDecisionInput = {
  photoId: string;
  /** `keep`, `reject` or `clear`. */
  action: string;
};

export type CullPassDto = {
  photos: number;
  eligible: number;
  selected: number;
  vetoCounts: number[];
  vetoNames: string[];
  swaps: number;
  coverageAdded: number;
  diversityDropped: number;
  sizeAdded: number;
  sizeTrimmed: number;
  peaksRejected: number;
  coverageWeak: number;
  coverageMissing: number;
  unweightedScenes: string[];
  elapsedMs: number;
};

/**
 * PHASE-13. Why anything happened, how sure it was, and what it looked at.
 *
 * Every shape below carries the backend's own reading of a code - its severity,
 * its domain, whether a band was raised - so that no component has to keep a
 * second copy of a vocabulary that spans five phases. A view that decided for
 * itself whether `keypoints_unavailable` is bad news would be a view that could
 * tell a photographer their photograph is badly framed because AURA did not look
 * at it.
 */

export type EvidenceCropDto = {
  x: number;
  y: number;
  w: number;
  h: number;
};

export type ParamDeltaDto = {
  name: string;
  value: number;
};

export type LedgerReasonDto = {
  code: string;
  text: string;
  weight: number;
  /** `credit`, `note`, `caveat` or `fault`. */
  severity: string;
  /** `technical`, `emotion`, `composition`, `selection` or `ledger`. */
  domain: string;
  /** `none`, `crop`, `frames` or `params`. */
  evidenceKind: string;
  crop: EvidenceCropDto | null;
  frames: string[];
  params: ParamDeltaDto[];
};

export type LedgerDecisionDto = {
  decisionId: string;
  kind: string;
  kindTitle: string;
  subjectKind: string;
  subjectId: string;
  rawConfidence: number;
  calibratedConfidence: number;
  calibrationVer: number;
  calibrated: boolean;
  autonomy: string;
  autonomyTitle: string;
  autonomyText: string;
  needsReview: boolean;
  source: string;
  reasons: LedgerReasonDto[];
  outputsJson: string;
  /** Hex, because JavaScript cannot hold a u64 exactly. */
  inputsHash: string;
  modelVersions: [string, number][];
  configVersions: [string, number][];
  ms: number;
  createdAt: number;
  supersedes: string | null;
};

export type ExplainTabDto = {
  id: string;
  title: string;
  available: boolean;
  /** Why there is nothing here. Rendered instead of an empty tab. */
  unavailableReason: string | null;
  reasons: LedgerReasonDto[];
  score: number | null;
  confidence: number | null;
};

export type AlternativeDto = {
  photoId: string;
  keepScore: number;
  technical: number;
  emotion: number;
  composition: number;
  prominence: number;
  delivered: boolean;
};

export type ExplainPanelDto = {
  photoId: string;
  tabs: ExplainTabDto[];
  decision: LedgerDecisionDto | null;
  headline: string | null;
  summary: string;
  summaryFromCloud: boolean;
  alternatives: AlternativeDto[];
};

export type LedgerStatusDto = {
  decisions: number;
  explained: number;
  explanationCoverage: number;
  current: number;
  superseded: number;
  byKind: number[];
  kindNames: string[];
  byAutonomy: number[];
  autonomyNames: string[];
  bySource: number[];
  sourceNames: string[];
  calibrated: number;
  calibrationVer: number;
  evidenced: number;
  reasons: number;
  bytes: number;
};

export type SupportBundleDto = {
  json: string;
  decisions: number;
  anonymised: number;
  safe: boolean;
};

export type ReviewQueueInput = {
  projectId: string;
  /** `auto`, `auto_zero_touch`, `suggest` or `require_review`. */
  band?: string | null;
  limit?: number | null;
};

export type RecordDecisionsInput = {
  projectId: string;
};

export type RecordDecisionsDto = {
  recorded: number;
  refused: number;
  byAutonomy: number[];
  autonomyNames: string[];
  uncalibrated: boolean;
  elapsedMs: number;
};

export type ExportBundleInput = {
  projectId: string;
  limit?: number | null;
};

// ---------------------------------------------------------------------------
// PHASE-14. The develop surface.
//
// Nine commands. The UI may read an edit, change a parameter, walk the history, snapshot,
// reset, render a proxy and ask what the renderer can do. There is no command here that
// names a destination, and none that can overwrite a parameter a person set - the second is
// enforced in `aura_recipe::schema::merge`, not on this wire, so a caller cannot route
// around it.
//
// See docs/adr/ADR-0030-develop-ipc-surface.md.
// ---------------------------------------------------------------------------

/** One parameter of an edit, as the develop panel renders it. */
export type DevelopParamDto = {
  /** The dotted path, e.g. `global.exposure`. The identity of the control. */
  path: string;
  /** The current value. A JSON scalar so one shape carries floats and integers. */
  value: unknown;
  /** True when a person set this and no automated pass may change it. */
  protected: boolean;
  /** Which render stage re-runs when this moves. `null` when it is inert. */
  stage: string | null;
};

/** One photograph's edit. */
export type RecipeDto = {
  photoId: string;
  /** The canonical JSON, exactly as hashed and stored. */
  body: string;
  recipeHash: string;
  schema: number;
  engine: string;
  /** `ai`, `user`, `qc`, `preset` or `default`. */
  source: string;
  confidence: number;
  decisionId: string | null;
  userEditedFields: string[];
  params: DevelopParamDto[];
};

/** One stage that did not run, and why. */
export type RenderNoteDto = {
  stage: string;
  reason: string;
  detail: string | null;
  /** True when this is worth showing the photographer. */
  isCaveat: boolean;
};

/** A rendered proxy, ready for a data URL. */
export type RenderDto = {
  width: number;
  height: number;
  /** Base64 of interleaved 8-bit RGB. There is no path: this phase writes no image file. */
  rgbBase64: string;
  colourSpace: string;
  icc: string;
  renderHash: string;
  backend: string;
  stagesRun: string[];
  notes: RenderNoteDto[];
  ms: number;
};

/** What this machine's renderer can do. */
export type RenderCapsDto = {
  backend: string;
  maxTexture: number;
  precisionBits: number;
  maxWorkingBytes: number;
  engine: string;
  degradation: string | null;
  degradationMessage: string | null;
};

/** One step in a photograph's edit history. */
export type HistoryEntryDto = {
  seq: number;
  atMs: number;
  source: string;
  changed: string[];
  label: string;
};

/** A photograph's history, as the panel renders it. */
export type HistoryDto = {
  photoId: string;
  entries: HistoryEntryDto[];
  snapshots: string[];
  canUndo: boolean;
  canRedo: boolean;
  hasAiSuggestion: boolean;
};

/** How much of a wedding has an edit. The denominator is every photograph. */
export type DevelopStatusDto = {
  images: number;
  withRecipe: number;
  fromAi: number;
  fromUser: number;
  touchedByHand: number;
  sidecarBehind: number;
};

export type DevelopImageInput = {
  photoId: string;
};

export type SetParamInput = {
  projectId: string;
  photoId: string;
  path: string;
  value: unknown;
  label?: string | null;
};

export type SetParamDto = {
  recipe: RecipeDto;
  changed: string[];
  /** The first stage that has to re-run, or `null` when nothing does. */
  invalidatedFrom: string | null;
};

export type RenderImageInput = {
  photoId: string;
  /** `proxy2048`, `screen` or `full`. */
  level?: string | null;
  screen?: [number, number] | null;
  /** `srgb`, `adobe_rgb` or `display_p3`. */
  colourSpace?: string | null;
  /** `interactive`, `analysis` or `export`. */
  purpose?: string | null;
};

export type HistoryStepInput = {
  projectId: string;
  photoId: string;
  /** `undo`, `redo`, `reset_original` or `reset_ai`. */
  action: string;
};

export type SnapshotInput = {
  projectId: string;
  photoId: string;
  name: string;
  /** `take` or `restore`. */
  action: string;
};

// ---------------------------------------------------------------------------
// PHASE-15. Exposure and white balance.
// ---------------------------------------------------------------------------

/** One light the solver found in a frame. */
export type IlluminantDto = {
  /** `daylight`, `tungsten`, `fluorescent`, `led`, `flash`, `candle`, `shade`,
   * `cloudy`, `mixed_discharge`, `coloured` or `unknown`. */
  kind: string;
  cctK: number;
  tint: number;
  /** How much of the frame this light accounts for, `0..1`. */
  weight: number;
  /** How far off neutral the light itself is, `0..1`. */
  chroma: number;
  /** `camera_as_shot`, `grey_world`, `white_patch`, `learned` or `known_neutral`. */
  source: string;
  /** Where it dominates, or `null` for a light that fills the frame. */
  region: CropRectDto | null;
};

/** A runner-up white balance and what it cost. */
export type ToneAlternativeDto = {
  exposureEv: number;
  temperatureK: number;
  tint: number;
  /** Lower is better. The winner's cost is the floor. */
  cost: number;
  why: string;
};

/** One thing that moved an exposure or a white balance. */
export type ToneReasonDto = {
  code: string;
  text: string;
  /** Negative is doubt. */
  weight: number;
  evidence: CropRectDto | null;
};

/** One photograph's exposure and white-balance decision. */
export type ToneDto = {
  photoId: string;
  exposureEv: number;
  exposureConf: number;
  temperatureK: number;
  tint: number;
  wbConf: number;
  /** The geometric mean of the two confidences. */
  confidence: number;
  illuminants: IlluminantDto[];
  mixedLight: boolean;
  dominantOnSubject: number | null;
  subjectLumaBefore: number;
  subjectLumaTarget: number;
  skinDe00Estimate: number;
  alternatives: ToneAlternativeDto[];
  reasons: ToneReasonDto[];
  scene: string;
  /** True when a face anchored the exposure. */
  faceAnchored: boolean;
  backlit: boolean;
  colouredLight: boolean;
  clippingAdded: number;
  constrainedIdentities: number;
  /** True when the photographer set these by hand. The three numbers above stay AURA's own. */
  userEdited: boolean;
  userExposureEv: number | null;
  userTemperatureK: number | null;
  userTint: number | null;
  reviewed: boolean;
  needsReview: boolean;
  modelVer: number;
  analysisVer: number;
  targetsVer: number;
};

/** What a project's tone pass covered and found. */
export type ToneStatusDto = {
  photos: number;
  estimated: number;
  /** Fraction estimated; the denominator is every photograph. */
  coverage: number;
  /** Fraction of estimated frames whose exposure was anchored on a face. */
  faceAnchored: number;
  /** Fraction whose white balance was bounded by a skin locus. */
  skinConstrained: number;
  mixedLight: number;
  colouredLight: number;
  needsReview: number;
  userEdited: number;
  meanEv: number;
  meanCct: number;
  illuminantCounts: number[];
  illuminantNames: string[];
  segmentsAnchored: number;
  segments: number;
  loci: number;
  untargetedScenes: string[];
  modelVer: number;
  analysisVer: number;
  targetsVer: number;
};

export type EstimateToneInput = {
  projectId: string;
  cancelId?: string | null;
};

/** What one tone pass did. */
export type TonePassDto = {
  estimated: number;
  failed: number;
  faceAnchored: number;
  mixedLight: number;
  colouredLight: number;
  lowConfidence: number;
  loci: number;
  segmentsAnchored: number;
  meanEv: number;
  meanCct: number;
  untargetedScenes: string[];
  recipesWritten: number;
  /** Paths the merge refused because a person had set them. */
  recipesProtected: number;
  elapsedMs: number;
  cancelled: boolean;
};

export type ToneReviewInput = {
  projectId: string;
  limit?: number | null;
};

export type AcceptToneInput = {
  photoId: string;
};

export type SetToneOverrideInput = {
  projectId: string;
  photoId: string;
  exposureEv?: number | null;
  temperatureK?: number | null;
  tint?: number | null;
};

/** What recording an override did, on both sides. */
export type SetToneOverrideDto = {
  estimate: ToneDto;
  recipe: RecipeDto;
  changed: string[];
  /** The dotted paths a person now owns. */
  protected: string[];
};

/** One of a segment's anchors for gallery consistency. */
export type ReferenceFrameDto = {
  photoId: string;
  segmentId: string;
  rank: number;
  wbConf: number;
  temperatureK: number;
  tint: number;
  subjectLuma: number;
  quality: number;
};

export type ReferenceFramesInput = {
  segmentId: string;
};

// ---------------------------------------------------------------------------
// PHASE-16. Tone curves, HSL and skin protection.
// ---------------------------------------------------------------------------

/**
 * One control point of a tone curve, in the recipe's 0-255 units.
 *
 * A curve is always monotone: `x` strictly increases, `y` never decreases, and the first and
 * last points sit at 0 and 255. The backend refuses to build one that is not, so a panel does
 * not have to check - but a curve editor that lets a photographer drag a point **does**, and
 * `setColourOverride` will refuse a set that breaks the rule rather than clamping it.
 */
export type CurvePointDto = {
  x: number;
  y: number;
};

/** One hue band's shift, in the recipe's units. */
export type HslShiftDto = {
  /** `red`, `orange`, `yellow`, `green`, `aqua`, `blue`, `purple` or `magenta`. */
  band: string;
  h: number;
  s: number;
  l: number;
};

/** What was found of one kind of content in one frame. */
export type BandReadingDto = {
  /** `greenery`, `sky`, `dress`, `wood`, `decor` or `skin`. */
  band: string;
  area: number;
  hueDeg: number;
  saturation: number;
  luma: number;
  /**
   * How sure the inference is, `0..1`.
   *
   * The bands are inferred from colour statistics rather than segmented, so a low confidence
   * is a real answer and the panel should say so: "AURA saw greenery and was not sure enough
   * to touch it" and "AURA saw no greenery" are different sentences.
   */
  confidence: number;
};

/**
 * What grading actually did to the skin in one frame.
 *
 * `measured` false is **not** a perfect score. A frame with nobody in it has no skin to
 * protect and no measurement to report, and rendering the two the same way would turn a
 * coverage gap into a guarantee.
 */
export type SkinGuardDto = {
  maskArea: number;
  maxHueShiftDeg: number;
  maxChromaChange: number;
  attenuation: number;
  resolves: number;
  measured: boolean;
  withinCeilings: boolean;
};

/** One thing that moved a grade. */
export type ColourReasonDto = {
  code: string;
  text: string;
  /** How much confidence it cost. Negative is doubt. */
  weight: number;
  evidence?: CropRectDto | null;
};

/**
 * A complete alternative grade.
 *
 * Whole parameter sets, never deltas: every one has been through the clipping guard and the
 * skin guard, which is what makes switching safe rather than only fast.
 */
export type ColourVariantDto = {
  /** `flatter`, `punchier` or `warmer`. */
  kind: string;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  vibrance: number;
  saturation: number;
  curve: CurvePointDto[];
  hsl: HslShiftDto[];
  skinGuard: SkinGuardDto;
};

/** One photograph's tone and colour decision. */
export type ColourDto = {
  photoId: string;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  vibrance: number;
  saturation: number;
  curve: CurvePointDto[];
  /** All eight bands, in the recipe's order, including the neutral ones. */
  hsl: HslShiftDto[];
  bands: BandReadingDto[];
  skinGuard: SkinGuardDto;
  clippingBefore: number;
  clippingAfter: number;
  clippingAdded: number;
  alternatives: ColourVariantDto[];
  reasons: ColourReasonDto[];
  confidence: number;
  /** The total adjustment magnitude, `0..1`. Lower is subtler. */
  subtlety: number;
  scene: string;
  /** True when there was skin in the frame and the guarantee was checked on it. */
  skinMeasured: boolean;
  userEdited: boolean;
  reviewed: boolean;
  needsReview: boolean;
  modelVer: number;
  analysisVer: number;
  intentVer: number;
};

/** What the Develop panel's project header shows. */
export type ColourStatusDto = {
  photos: number;
  decided: number;
  coverage: number;
  skinMeasured: number;
  skinGuardTriggered: number;
  clipGuardResolved: number;
  subtletyCapped: number;
  needsReview: number;
  userEdited: number;
  meanContrast: number;
  meanShadowLift: number;
  meanSubtlety: number;
  /** The one number that falsifies this phase's headline guarantee. */
  worstSkinHueShift: number;
  guaranteeHeld: boolean;
  untargetedScenes: string[];
  modelVer: number;
  analysisVer: number;
  intentVer: number;
};

export type EstimateColourInput = {
  projectId: string;
  cancelId?: string | null;
};

/** What one grading pass did. */
export type ColourPassDto = {
  decided: number;
  failed: number;
  skinMeasured: number;
  skinGuardTriggered: number;
  skinGuardWithdrew: number;
  clipGuardResolved: number;
  subtletyCapped: number;
  lowConfidence: number;
  meanContrast: number;
  meanShadowLift: number;
  meanSubtlety: number;
  worstSkinHueShift: number;
  untargetedScenes: string[];
  recipesWritten: number;
  recipesProtected: number;
  elapsedMs: number;
  cancelled: boolean;
};

export type ColourReviewInput = {
  projectId: string;
  limit?: number | null;
};

export type AcceptColourInput = {
  photoId: string;
};

/** Promote one stored alternative to the primary grade. */
export type SelectVariantInput = {
  projectId: string;
  photoId: string;
  /** `flatter`, `punchier` or `warmer`. */
  kind: string;
};

/**
 * Record what the photographer set instead.
 *
 * Every field is optional and independent. The curve and the HSL block are whole-or-nothing,
 * because a curve is not a set of independent numbers.
 */
export type SetColourOverrideInput = {
  projectId: string;
  photoId: string;
  contrast?: number | null;
  highlights?: number | null;
  shadows?: number | null;
  whites?: number | null;
  blacks?: number | null;
  vibrance?: number | null;
  saturation?: number | null;
  curve?: CurvePointDto[] | null;
  hsl?: HslShiftDto[] | null;
};

/** What recording a colour override did, on both sides. */
export type SetColourOverrideDto = {
  decision: ColourDto;
  recipe: RecipeDto;
  changed: string[];
  /** The dotted paths a person now owns. */
  protected: string[];
};

// ---------------------------------------------------------------------------
// PHASE-17. Style learning: scene-conditional personal AI profiles.
// ---------------------------------------------------------------------------

/** One leaf of the style tree, as the matrix draws it. */
export type StyleBucketDto = {
  /** `group/lighting`, the catalog's own key. */
  key: string;
  group: string;
  lighting: string;
  title: string;
  samples: number;
  heldOut: number;
  /**
   * The measured style-match error in dE00, or `null` when nothing was held out.
   *
   * **Never zero for "not measured".** A bucket trained on eleven pairs and evaluated on none
   * has no measurement, and zero would render as a perfect match - which is the one thing a
   * report about accuracy must not do where it knows least.
   */
  matchDe00: number | null;
  /** `bucket`, `group`, `global` or `factory`. */
  level: string;
  weak: boolean;
};

/** One profile, as the list shows it. */
export type StyleProfileDto = {
  profileId: string;
  name: string;
  version: number;
  /** `candidate`, `adopted` or `retired`. */
  status: string;
  trainedPairs: number;
  /** `0..1`, for the meter section 12 asks for instead of a ready state. */
  strength: number;
  overallDe00: number;
  taughtBuckets: number;
  usable: boolean;
  engineVer: string;
  trainedAt: number;
};

/** The honest report a photographer reads before adopting. */
export type ProfileReportDto = {
  profile: StyleProfileDto;
  perBucket: StyleBucketDto[];
  weakBuckets: string[];
  recommendation: string;
  acceptedPairs: number;
  /** On the wire beside the acceptance, so a report cannot claim a hundred percent. */
  rejectedPairs: number;
  acceptance: number;
  metCeiling: boolean;
};

/** Point the scanner at folders of the photographer's own work. */
export type ScanArchiveInput = {
  name: string;
  /** Paths in, and nothing but names out. */
  roots: string[];
};

/** What one archive scan found, before anything is fitted. */
export type ScanArchiveDto = {
  originals: number;
  finals: number;
  matched: number;
  unmatchedOriginals: number;
  /** The one worth reporting: the RAWs are missing, elsewhere, or undecodable. */
  unmatchedFinals: number;
  byMethod: [string, number][];
  weakestMethod: string;
  enough: boolean;
};

/** One original-and-final pair. **No pixels.** */
export type StylePairDto = {
  original: string;
  finalImage: string;
  matchedBy: string;
  extractedFrom: string;
  bucket: string;
  residualDe00: number;
  accepted: boolean;
  rejection: string | null;
};

export type TrainProfileInput = {
  name: string;
  cancelId?: string | null;
};

/** What one training run did. The profile it produced is a **candidate**. */
export type TrainProfileDto = {
  profile: StyleProfileDto | null;
  matched: number;
  accepted: number;
  rejected: number;
  reused: number;
  fromXmp: number;
  buckets: number;
  overallDe00: number;
  /** The same figure for the unstyled baseline, so the improvement is visible. */
  baselineDe00: number;
  elapsedMs: number;
  cancelled: boolean;
};

export type AdoptProfileInput = {
  profileId: string;
};

export type StyleReasonDto = {
  code: string;
  text: string;
  weight: number;
};

/** One leaf's three answers, for the side-by-side before adoption. */
export type StyleComparisonDto = {
  bucket: string;
  title: string;
  /** Exposure, temperature, contrast and vibrance. */
  baseline: number[];
  current: number[];
  candidate: number[];
  level: string;
  confidence: number;
  reasons: StyleReasonDto[];
};

export type CompareProfilesInput = {
  projectId: string;
  candidateId: string;
  limit?: number | null;
};

export type ExportProfileInput = {
  profileId: string;
  path: string;
};

export type ExportProfileDto = {
  path: string;
  bytes: number;
  fingerprint: string;
};

export type ImportProfileInput = {
  path: string;
};

/** What importing a profile produced. */
export type ImportProfileDto = {
  profile: StyleProfileDto;
  fingerprint: string;
  /**
   * True when the document is unchanged since it was signed.
   *
   * **Not `verified`.** With the public key inside the bundle this proves integrity and not
   * provenance: there is no key distribution in this product and nothing to check a key
   * against. The panel says "unchanged since signing" and never says "verified".
   */
  unchangedSinceSigning: boolean;
};

export type SetProjectProfileInput = {
  projectId: string;
  /** One of phase 07's nine chapter slugs, or `null` for the project default. */
  chapter?: string | null;
  /** The profile, or `null` to clear the selection. */
  profileId?: string | null;
};

/** What the style panel's project header shows. */
export type StyleStatusDto = {
  profiles: number;
  active: string | null;
  activeName: string;
  activeVersion: number;
  trainedPairs: number;
  strength: number;
  overallDe00: number;
  chapterOverrides: string[];
  /**
   * How many leaves resolve at each level.
   *
   * The number that matters when it is skewed: a wedding whose frames all resolve at `global`
   * has had its scene conditioning do nothing, which is the quiet version of "one global
   * style" - the exact thing this phase exists to beat.
   */
  levelCounts: [string, number][];
  bucketRatio: number;
  analysisVer: number;
};

// ---------------------------------------------------------------------------
// PHASE-18. Local mask AI: the regions every later phase edits inside.
// ---------------------------------------------------------------------------

/**
 * What the mask panel's project header shows.
 *
 * **`selected` and `masked` are two numbers rather than a ratio**, and this is the first status
 * shape in the product where that matters. The denominator is *selected* frames, not every
 * photograph: a mask over a rejected frame is not a gap, it is a frame nobody asked about. A
 * project where the cull has not run sends `selected: 0` rather than a coverage figure computed
 * against a denominator that does not exist.
 */
export type MaskStatusDto = {
  selected: number;
  masked: number;
  masks: number;
  userEdited: number;
  lowQuality: number;
  meanConfidence: number;
  meanEdgeQuality: number;
  payloadBytes: number;
  /** Mean stored bytes per masked frame. What the 180 KB budget bounds. */
  bytesPerImage: number;
  modelVer: number;
  analysisVer: number;
  /** False in this build. The learned segmentation head is registered and never consulted. */
  headTrained: boolean;
};

/** One reason a region is the way it is, with the sentence the panel renders. */
export type MaskReasonDto = {
  code: string;
  text: string;
};

/**
 * One region of one photograph.
 *
 * `confidence` and `edgeQuality` are never collapsed into one number: they fail independently
 * and are fixed by different things, so the panel shows two bars and names which of the two is
 * limiting what may be done.
 */
export type MaskDto = {
  id: string;
  imageId: string;
  /** The class slug, from the frozen twenty. */
  kind: string;
  identityId: string | null;
  identityName: string | null;
  /** `rle` or `alpha8`. */
  form: string;
  width: number;
  height: number;
  bytes: number;
  feather: number;
  confidence: number;
  edgeQuality: number;
  /** `matted`, `soft`, `binary` or `unknown`. */
  edge: string;
  /**
   * The strength ceiling phases 19 to 24 multiply by.
   *
   * Computed in Rust and sent. The panel could derive it from the two quality numbers and must
   * not: two implementations of a gating rule is two answers to "may this mask carry skin
   * smoothing", and the one written here is the one nobody tests against a fixture.
   */
  allowance: number;
  allowsAggressive: boolean;
  reasons: MaskReasonDto[];
  userEdited: boolean;
  modelVer: number;
};

/**
 * A region as a plane the panel can draw.
 *
 * Quarter-resolution eight-bit alpha, base64, capped on the long edge. **There is no field here
 * that could hold a photograph** - this is derived geometry about a region, and the pixels of
 * the frame reach the panel through the preview surface and nowhere else.
 */
export type MaskOverlayDto = {
  id: string;
  width: number;
  height: number;
  alphaBase64: string;
  level: string;
};

/** Ask for a photograph's regions, producing any that are missing. */
export type EnsureMasksInput = {
  /**
   * The project the photograph is in.
   *
   * Needed because producing a region reads pixels, and the preview cache is opened per project.
   * Every other command on this surface is a query over one table and takes no project.
   */
  projectId: string;
  imageId: string;
  /** Which classes. Empty means all twenty. */
  kinds?: string[];
};

/**
 * One step of a mask composition, in postfix order.
 *
 * A whole edit arrives as one command rather than as a stream of brush points: a per-point
 * command would be a command per animation frame, and it would make undo a replay of two
 * hundred rows.
 */
export type MaskOpDto = {
  /**
   * `source`, `plane`, `union`, `intersect`, `subtract`, `invert`, `feather`, `grow` or
   * `shrink`.
   */
  op: string;
  maskId?: string | null;
  width?: number | null;
  height?: number | null;
  alphaBase64?: string | null;
  amount?: number | null;
  radius?: number | null;
};

/**
 * Apply a composition to one region and keep the result.
 *
 * Sets `userEdited`, and there is no argument here that clears it. The one thing that clears it
 * is `regenerateMask`, which is a separate deliberate act.
 */
export type EditMaskInput = {
  maskId: string;
  ops: MaskOpDto[];
  feather?: number | null;
};

/** What one operation may do through one region. */
export type MaskAllowanceDto = {
  maskId: string;
  operation: string;
  /** Multiply by it; do not compare against it. */
  ceiling: number;
  permitted: boolean;
  reasons: MaskReasonDto[];
};

// ---------------------------------------------------------------------------
// PHASE-19. Local light sculpting.
// ---------------------------------------------------------------------------

/** One face, and what the light on it was moved by. */
export type FaceLightDto = {
  identityId?: string | null;
  exposureEv: number;
  shadows: number;
  /** Never positive. A face is never lifted by pushing its highlights up. */
  highlights: number;
  lumaBefore: number;
  lumaTarget: number;
  lumaAfter: number;
  /**
   * The largest lift this frame's noise would have tolerated, in stops.
   *
   * What the panel shows when a lift stopped short: "AURA lifted her face 0.4 EV
   * and would have lifted it 0.9" is a sentence a photographer can act on.
   */
  noiseCapEv: number;
  maskScale: number;
};

/** One shaping move, as a retoucher would name it. */
export type ShapingZoneDto = {
  zone: string;
  cx: number;
  cy: number;
  radius: number;
  /** Positive lifts, negative deepens. Bounded to a sixth of a stop. */
  gainEv: number;
};

/** One reason the local work came out the way it did. */
export type LocalReasonDto = {
  code: string;
  text: string;
  weight: number;
  operation?: string | null;
  /** True when the code withdraws a claim rather than making one. */
  withdrawal: boolean;
  evidence?: CropRectDto | null;
};

/** One operation that did not run at full strength, and the mask that stopped it. */
export type GateDto = {
  operation: string;
  maskKind: string;
};

/** Everything phase 19 decided about the light inside one photograph. */
export type LocalPlanDto = {
  photoId: string;
  /** The strength each operation ran at, in priority order. */
  strengths: number[];
  /** The operations' stable slugs, in the same order. */
  operations: string[];
  faces: FaceLightDto[];
  subjectClarity: number;
  subjectTexture: number;
  subjectContrast: number;
  /** Zero or negative. This phase calms a background and never enriches one. */
  backgroundEv: number;
  backgroundSaturation: number;
  competitionRatio: number;
  chromaEnergy: number;
  meanLumaBefore: number;
  meanLumaAfter: number;
  shineRegions: number;
  shineEv: number;
  shineBoxes: CropRectDto[];
  /** The shaping moves, by face ordinal. */
  shaping: ShapingZoneDto[][];
  faceSpread: number;
  groupFair: boolean;
  budgetUsed: number;
  gated: GateDto[];
  reasons: LocalReasonDto[];
  confidence: number;
  scene: string;
  userEdited: boolean;
  reviewed: boolean;
  needsReview: boolean;
  modelVer: number;
  analysisVer: number;
  policyVer: number;
  shapingVer: number;
};

/** What a project's local light pass covered and found. */
export type LocalStatusDto = {
  photos: number;
  planned: number;
  coverage: number;
  /**
   * Fraction of planned frames where at least one operation actually ran.
   *
   * The number that matters when it is low: because this phase's work is meant to
   * be invisible, a wedding at 100 % coverage and 4 % acted-on looks exactly like a
   * wedding that was worked on.
   */
  actedOn: number;
  maskCovered: number;
  opCounts: number[];
  opNames: string[];
  gatedCounts: number[];
  gatedNames: string[];
  meanBudgetUsed: number;
  shineReduced: number;
  meanShineEv: number;
  groupSolved: number;
  needsReview: number;
  userEdited: number;
  unpoliciedScenes: string[];
  modelVer: number;
  analysisVer: number;
  policyVer: number;
  shapingVer: number;
};

export type SculptLocalInput = {
  projectId: string;
  /** Empty means every photograph with no current plan. */
  photoIds?: string[];
  cancelId?: string | null;
};

/** What one local light pass did. */
export type LocalPassDto = {
  planned: number;
  failed: number;
  actedOn: number;
  opCounts: number[];
  gated: number;
  fullyMasked: number;
  groupSolved: number;
  shineReduced: number;
  lowConfidence: number;
  meanBudgetUsed: number;
  unpoliciedScenes: string[];
  recipesWritten: number;
  /** Paths the merge refused because a person had set them. */
  recipesProtected: number;
  elapsedMs: number;
  cancelled: boolean;
};

export type LocalReviewInput = {
  projectId: string;
  limit?: number | null;
};

export type AcceptLocalInput = {
  photoId: string;
};

export type SetLocalStrengthInput = {
  projectId: string;
  photoId: string;
  operation: string;
  strength: number;
};

/** What recording a strength override did, on both sides. */
export type SetLocalStrengthDto = {
  plan: LocalPlanDto;
  recipe: RecipeDto;
  changed: string[];
  /** The dotted paths a person now owns. */
  protected: string[];
};

// ---------------------------------------------------------------------------
// PHASE-20. Portrait retouch.
// ---------------------------------------------------------------------------

/** One thing that was done to somebody's skin. */
export type RetouchOpDto = {
  /** `blemish`, `under_eye`, `tone_evening` or `shine_reduce`. */
  kind: string;
  strength: number;
  /** Present for a blemish; absent for the operations that act through a mask or a region. */
  area?: CropRectDto | null;
  /** `patch` or `learned`, for a blemish. */
  method?: string | null;
  identityId?: string | null;
  /** Bounded at 0.25 stops by the contract. */
  lumaEv: number;
  /** Bounded at 0.12 by the contract. */
  chroma: number;
};

/**
 * Something about a person AURA will not remove.
 *
 * `area` is **face-normalised**: the origin sits between the eyes, x runs along the eye-to-eye
 * line and the unit is the inter-ocular distance, so `x` and `y` may be negative. That is what
 * lets one row protect the same mole in four hundred photographs.
 */
export type ProtectedFeatureDto = {
  identityId: string;
  /** `mole`, `freckle`, `birthmark`, `scar`, `tattoo` or `dimple`. */
  kind: string;
  area: CropRectDto;
  confidence: number;
  /** `cross_frame`, `classifier` or `user`, in ascending order of authority. */
  source: string;
  frames: number;
  spanMinutes: number;
  firstSeenPhoto: string;
  /** True when nothing may clear it. A tattoo is rendered without a control, not with a disabled one. */
  absolute: boolean;
};

/** What the retouch did to the skin's own texture, measured through the renderer. */
export type TextureReportDto = {
  /** High-band skin energy after over the same energy before. One is a retouch that cost nothing. */
  bandRatio: number;
  floor: number;
  passed: boolean;
  /** Below 256 the ratio is arithmetic rather than evidence, and the panel says so. */
  measuredOn: number;
  resolves: number;
  /** True when the retouch was withdrawn because the floor could not be met. */
  withdrawn: boolean;
};

/** One reason the retouch came out the way it did. */
export type RetouchReasonDto = {
  code: string;
  text: string;
  weight: number;
  /** Half the codes in this phase are withdrawals, which is why the panel groups by this. */
  withdrawal: boolean;
  evidence?: CropRectDto | null;
};

/** One person's gallery-wide retouch strength. */
export type IdentityStrengthDto = {
  identityId: string;
  strength: number;
};

/** Everything phase 20 decided about one photograph's skin. */
export type RetouchPlanDto = {
  photoId: string;
  ops: RetouchOpDto[];
  identityStrengths: IdentityStrengthDto[];
  protected: ProtectedFeatureDto[];
  texture: TextureReportDto;
  /** `off`, `light`, `natural` or `polished`. */
  preset: string;
  reasons: RetouchReasonDto[];
  confidence: number;
  scene: string;
  /** Phase 19's shared allowance, not a second one. */
  budgetUsed: number;
  userEdited: boolean;
  reviewed: boolean;
  needsReview: boolean;
  modelVer: number;
  analysisVer: number;
  presetVer: number;
};

/** What a project's retouch pass covered and found. */
export type RetouchStatusDto = {
  photos: number;
  planned: number;
  coverage: number;
  actedOn: number;
  maskCovered: number;
  blemishesRemoved: number;
  /** The answer to "why is that mark still there". */
  anomaliesLeft: number;
  protectedCounts: number[];
  protectedKinds: string[];
  textureResolved: number;
  textureWithdrawn: number;
  meanBandRatio: number;
  meanStrength: number;
  /** Zero while strength is a gallery constant. */
  maxIdentitySpread: number;
  presetCounts: number[];
  presetNames: string[];
  needsReview: number;
  userEdited: number;
  unpresetScenes: string[];
  modelVer: number;
  analysisVer: number;
  presetVer: number;
};

/** What one retouch pass did. */
export type RetouchPassDto = {
  planned: number;
  failed: number;
  actedOn: number;
  maskCovered: number;
  blemishes: number;
  textureResolved: number;
  textureWithdrawn: number;
  protected: number;
  lowConfidence: number;
  meanBandRatio: number;
  unpresetScenes: string[];
  recipesWritten: number;
  recipesProtected: number;
  elapsedMs: number;
  cancelled: boolean;
};

export type RetouchPassInput = {
  projectId: string;
  photoIds?: string[];
  /** `off`, `light`, `natural` or `polished`. Absent means Natural. */
  preset?: string | null;
  cancelId?: string | null;
};

export type RetouchReviewInput = {
  projectId: string;
  limit?: number | null;
};

export type AcceptRetouchInput = {
  photoId: string;
};

/**
 * Record what the photographer set instead.
 *
 * A strength is **gallery-wide**: setting one person's strength on one frame and not on the rest
 * is how a gallery ends up with a bride whose skin changes character between the ceremony and the
 * reception.
 */
export type SetRetouchInput = {
  projectId: string;
  photoId: string;
  preset?: string | null;
  identityId?: string | null;
  strength?: number | null;
};

export type SetRetouchDto = {
  plan: RetouchPlanDto;
  recipe: RecipeDto;
  changed: string[];
  /** The dotted paths a person now owns. */
  protected: string[];
};

/**
 * Add or clear one protected feature.
 *
 * `area` arrives in **frame** coordinates, as the panel drew it, and the backend projects it onto
 * the face. Clearing an absolute feature - a tattoo - is refused with `AURA-ML-5097`.
 */
export type SetProtectionInput = {
  projectId: string;
  identityId: string;
  photoId: string;
  kind: string;
  area: CropRectDto;
  protect: boolean;
};

// ---------------------------------------------------------------------------
// PHASE-21. The micro-retouch surface.
// ---------------------------------------------------------------------------

/**
 * One small fix, as the panel draws it.
 *
 * The five operators flattened into one shape: `kind` says which, and only the fields that
 * operator uses are non-zero.
 */
export type MicroOpDto = {
  /** `flyaway`, `teeth`, `eyes`, `clothing` or `glare`. */
  kind: string;
  /** How strongly it ran, as a fraction of that operator's own ceiling. */
  strength: number;
  /** Where it acted, for the three operators that name a rectangle. */
  region?: CropRectDto | null;
  /** Whose face, for the two that name a person. */
  identityId?: string | null;
  /** Teeth luminance lift in stops. Bounded at 0.20. */
  lumaEv: number;
  /** Teeth yellow reduction, as a share of the measured excess. Bounded at 0.35. */
  yellowReduce: number;
  /** Sclera redness reduction, as a share of the measured excess. Bounded at 0.30. */
  sclera: number;
  /** Iris local contrast gain. Bounded at 0.25. */
  irisClarity: number;
  /** `lint`, `thread`, `stain`, `strap` or `crease`, for a clothing operation. */
  clothingKind?: string | null;
  /** `reduce` or `borrow`, for a glare operation. */
  method?: string | null;
  /**
   * **The disclosure.** The photograph these pixels came from, for a borrow.
   *
   * Never absent when `method` is `borrow`. The panel must render such a region with a visible
   * marker rather than as an ordinary edit - see `docs/retouch-ethics.md` section 5.
   */
  borrowedFrom?: string | null;
  /** How well the two regions aligned, for a borrow. */
  alignment: number;
};

/** What the naturalness guard measured on the rendered result. */
export type NaturalnessReportDto = {
  /** Peak iris luminance after over before. Held at or above 0.98. */
  catchlightRatio: number;
  /** Hair-region edge energy after over before. Held at or above 0.94. */
  hairEnergyRatio: number;
  /** How much further outside the locus the plan pushed the teeth. Held below 0.003. */
  teethExcursion: number;
  /**
   * How many pixels the three measurements were taken over, summed.
   *
   * Show the ratios to three decimals only when this is large enough to mean something.
   */
  measuredOn: number;
  /** How many times a family gave up strength to reach its bound. */
  resolves: number;
  /** Which families were withdrawn, aligned with `families`. */
  withdrawn: boolean[];
  /** The family names, so the panel never hard-codes the order. */
  families: string[];
};

/** One reason the plan came out the way it did. */
export type MicroReasonDto = {
  code: string;
  text: string;
  weight: number;
  /** True when the code withdraws a claim rather than making one. */
  doubt: boolean;
  evidence?: CropRectDto | null;
};

/** One photograph's micro-retouch plan. */
export type MicroPlanDto = {
  photoId: string;
  ops: MicroOpDto[];
  naturalness: NaturalnessReportDto;
  /** Which operations the matrix permitted, aligned with `operators`. */
  allowed: boolean[];
  operators: string[];
  reasons: MicroReasonDto[];
  confidence: number;
  scene: string;
  budgetUsed: number;
  /** **The disclosure, per frame.** Every photograph this plan borrowed pixels from. */
  borrowedFrom: string[];
  userEdited: boolean;
  reviewed: boolean;
  modelVer: number;
  analysisVer: number;
  matrixVer: number;
};

/** What the Micro-Retouch panel's project header shows. */
export type MicroStatusDto = {
  photos: number;
  planned: number;
  /** Fraction of the project with a plan. The denominator is every photograph. */
  coverage: number;
  actedOn: number;
  /** Photographs where the regions this phase needs arrived from phase 18. */
  regionCovered: number;
  /** How many operations of each kind ran, aligned with `operators`. */
  opCounts: number[];
  operators: string[];
  /** **How many frames in this gallery composited pixels from another.** */
  borrows: number;
  /** How many families were withdrawn, aligned with `families`. */
  withdrawnCounts: number[];
  families: string[];
  resolved: number;
  meanCatchlightRatio: number;
  meanHairEnergyRatio: number;
  needsReview: number;
  userEdited: number;
  unlistedScenes: string[];
  modelVer: number;
  analysisVer: number;
  matrixVer: number;
};

/**
 * Which operations a project permits.
 *
 * **There is no strength field and no ceiling field, and there never will be.** A studio chooses
 * which small fixes run; how far each may go is bounded by the contract.
 */
export type MicroMatrixDto = {
  allowed: boolean[];
  operators: string[];
  clothing: boolean[];
  clothingKinds: string[];
  /** Which issues are opt-in only and start switched off. */
  clothingOptIn: boolean[];
  /** Whether cross-frame borrowing is permitted at all. */
  borrowing: boolean;
};

/** Record which operations a project permits. Absent fields are left alone. */
export type SetMicroMatrixInput = {
  projectId: string;
  allowed?: boolean[] | null;
  clothing?: boolean[] | null;
  borrowing?: boolean | null;
};

/** Run the resumable micro-retouch pass. */
export type MicroPassInput = {
  projectId: string;
  priority?: string | null;
  /** Switch the whole stage off for this run. */
  enabled?: boolean | null;
};

/** What one pass did. */
export type MicroPassDto = {
  planned: number;
  failed: number;
  actedOn: number;
  regionCovered: number;
  ops: number[];
  borrows: number;
  meanAlignment: number;
  withdrawn: number[];
  resolved: number;
  lowConfidence: number;
  unlistedScenes: string[];
  elapsedMs: number;
  cancelled: boolean;
};

/** Ask for the frames worth a photographer's attention. */
export type MicroReviewInput = {
  projectId: string;
  limit?: number | null;
};

/** Record that a photographer has looked at one plan and agrees. */
export type AcceptMicroInput = {
  photoId: string;
};

/**
 * One frame that composited pixels from another, and where they came from.
 *
 * **The disclosure list.** The panel, the delivery report and the QC agent all read this, so no
 * two of them can disagree about what was composited.
 */
export type MicroCompositeDto = {
  photoId: string;
  sourcePhotoIds: string[];
};

// ---------------------------------------------------------------------------
// PHASE-22. Restoration.
// ---------------------------------------------------------------------------

/**
 * What happened to one face the restoration pass considered.
 *
 * **Every face gets one of these, whether it was recovered or not.** Two thirds of this phase's
 * reason codes are refusals, and a panel that only listed what happened would make a careful
 * product look like a careless one.
 *
 * `identityDrift` is present whether the face was kept or skipped, so the panel can show a
 * measured distance beside the sentence rather than a bare refusal.
 */
export type RestoreFaceDto = {
  identityId: string | null;
  area: CropRectDto;
  sharpness: number;
  strength: number;
  identityDrift: number;
  resolves: number;
  skipped: boolean;
  skippedBecause: string | null;
};

/**
 * What the artefact self-check measured on the rendered result.
 *
 * Three numbers rather than one score: smearing is fixed by lowering the denoise tier, ringing by
 * reducing the sharpen amount, and drift by the identity constraint. A photographer whose
 * complaint is that an edge looks crunchy needs the ringing figure rather than a score that
 * averaged it with something else.
 */
export type ArtefactReportDto = {
  textureRetention: number;
  ringing: number;
  identityDrift: number;
  measuredOn: number;
  resolves: number;
  denoiseReduced: boolean;
  sharpenReduced: boolean;
  faceSkipped: boolean;
};

/** One reason a restoration came out the way it did. */
export type RestoreReasonDto = {
  code: string;
  text: string;
  /** `denoise`, `sharpen`, `face_recovery` or `plan`. The panel groups by this. */
  subject: string;
  weight: number;
  restraint: boolean;
  area: CropRectDto | null;
};

/** One photograph's restoration plan. */
export type RestorePlanDto = {
  photoId: string;
  denoise: string;
  denoiseLuminance: number | null;
  denoiseColour: number | null;
  denoiseSigma: number | null;
  denoiseCamera: string | null;
  denoiseMeasured: boolean;
  sharpenKernel: number | null;
  sharpenAmount: number;
  sharpenSkinAttenuation: number;
  sharpenCoverage: number;
  faceRecovery: number;
  faces: RestoreFaceDto[];
  facesRecovered: number;
  facesSkippedIdentity: number;
  selfcheck: ArtefactReportDto | null;
  runWhere: string;
  runWhen: string;
  regionCovered: boolean;
  reasons: RestoreReasonDto[];
  confidence: number;
  scene: string;
  userEdited: boolean;
  reviewed: boolean;
};

/** One reason sharpening was refused, and how often. */
export type RestoreRefusalDto = {
  code: string;
  text: string;
  count: number;
};

/** What the Restore panel's project header shows. */
export type RestoreStatusDto = {
  photos: number;
  planned: number;
  coverage: number;
  actedOn: number;
  regionCovered: number;
  tiers: number[];
  tierNames: string[];
  sharpened: number;
  sharpenRefusals: RestoreRefusalDto[];
  facesRecovered: number;
  facesSkippedIdentity: number;
  worstIdentityDrift: number;
  meanTextureRetention: number;
  meanRinging: number;
  reduced: number;
  needsReview: number;
  userEdited: number;
  /** Camera bodies denoised against a synthetic noise model. Every body in this build. */
  unmeasuredCameras: string[];
  unlistedScenes: string[];
  versions: number[];
};

/** Ask for the frames worth a photographer's attention. */
export type RestoreReviewInput = {
  projectId: string;
  limit?: number | null;
};

/** Record that a photographer has looked at one plan and agrees. */
export type AcceptRestoreInput = {
  photoId: string;
};

/**
 * Record what a photographer chose for one photograph.
 *
 * **A tier and two switches, and no other number.** The line is between which of four and how far
 * each goes: a tier is on the wire and the three denoise amounts are not, because they are what
 * the tier becomes under one sensor at one ISO.
 */
export type SetRestoreOverrideInput = {
  photoId: string;
  denoise?: string | null;
  sharpen?: boolean | null;
  faceRecovery?: boolean | null;
};

/** Run the resumable restoration pass. */
export type RestorePassInput = {
  projectId: string;
  /** `export` or `background`. There is no interactive value. */
  when?: string | null;
  priority?: string | null;
  outputLongEdge?: number | null;
  enabled?: boolean | null;
};

/** What one pass did. */
export type RestorePassDto = {
  planned: number;
  failed: number;
  actedOn: number;
  regionCovered: number;
  tiers: number[];
  sharpened: number;
  facesRecovered: number;
  facesSkippedIdentity: number;
  reduced: number;
  lowConfidence: number;
  unmeasuredCameras: string[];
  unlistedScenes: string[];
  elapsedMs: number;
  cancelled: boolean;
};

/**
 * One frame whose face recovery was declined to keep somebody looking like themselves.
 *
 * **The guarantee's own list.** The panel, the delivery report and the QC agent all read it.
 */
export type RestoreIdentityRefusalDto = {
  photoId: string;
  worstDrift: number;
  faces: number;
};

// ---------------------------------------------------------------------------
// PHASE-23 - geometry
//
// No shape here can hold a pixel, and none returns the lens profile table. What the panel
// gets is rectangles, an angle, a set of coefficients and reason codes - plus `lensSynthetic`,
// which says whether anybody actually measured the lens.
// ---------------------------------------------------------------------------

export type CropVariantDto = {
  /** original | primary | album | social | wide */
  purpose: string;
  title: string;
  /** original | 4:5 | 5:4 | 1:1 | 16:9 */
  aspect: string;
  rect: CropRectDto;
  score: number;
  safe: boolean;
};

export type CropSafetyDto = {
  facesIntact: boolean;
  resolutionOk: boolean;
  contentKept: boolean;
  /** **Zero is "nothing was checked", never "nothing was cut".** */
  facesChecked: number;
  /** Zero on every photograph in this build: there is no pose estimate. */
  handsChecked: number;
  /** True when at least one region was actually checked. Ask this before saying "safe". */
  isEvidence: boolean;
  /** face, hands, resolution, content. */
  refused: number[];
  refusedNames: string[];
};

export type GeometryReasonDto = {
  code: string;
  text: string;
  weight: number;
  /** True when this describes something AURA declined to do. Eleven of the twenty-four do. */
  restraint: boolean;
  evidence?: CropRectDto | null;
};

export type GeometryPlanDto = {
  photoId: string;
  scene: string;
  /** none | embedded | profile | estimated */
  lensSource: string;
  lensId?: string | null;
  lensProfile?: string | null;
  /** True when the profile was fabricated rather than measured. On this build, always. */
  lensSynthetic: boolean;
  distortion: number[];
  vignette: number;
  ca: number[];
  rotateDeg: number;
  rotateConf: number;
  keystoneVertical?: number | null;
  keystoneHorizontal?: number | null;
  keystoneStretch?: number | null;
  keystoneVerticals: number;
  /** Never empty. Index zero is always the frame as shot. */
  crops: CropVariantDto[];
  primaryCrop: number;
  keptOriginal: boolean;
  safety: CropSafetyDto;
  reasons: GeometryReasonDto[];
  confidence: number;
  profileVer: number;
  analysisVer: number;
  rulesVer: number;
  userEdited: boolean;
};

export type GeometryStatusDto = {
  photos: number;
  planned: number;
  coverage: number;
  /** **Above 0.70 is the passing direction.** More restraint is a better result. */
  keptOriginal: number;
  profileCovered: number;
  levelled: number;
  meanRotateDeg: number;
  keystoned: number;
  variantCounts: number[];
  variantNames: string[];
  refusedCounts: number[];
  refusedNames: string[];
  missingProfiles: string[];
  unpoliciedScenes: string[];
  needsReview: number;
  userEdited: number;
  profilesSynthetic: boolean;
  profilesKnown: number;
  profileVer: number;
  analysisVer: number;
  rulesVer: number;
};

export type PlanGeometryInput = {
  projectId: string;
  limit?: number | null;
};

export type GeometryPassDto = {
  planned: number;
  failed: number;
  keptOriginal: number;
  recipesWritten: number;
  /** Paths the merge refused because a person had set them. */
  recipesProtected: number;
  elapsedMs: number;
  cancelled: boolean;
};

export type GeometryReviewInput = {
  projectId: string;
  limit?: number | null;
};

/**
 * Record the framing the photographer chose.
 *
 * **Reverting is this command with the whole frame and zero degrees**, not a separate one:
 * a revert implemented as clearing the row is a revert the next pass undoes.
 */
export type SetFramingInput = {
  projectId: string;
  photoId: string;
  rect: CropRectDto;
  rotateDeg: number;
  aspect: string;
};

export type SetFramingDto = {
  plan: GeometryPlanDto;
  recipe: RecipeDto;
  changed: string[];
  /** The dotted paths a person now owns. */
  protected: string[];
};

export type AcceptGeometryInput = {
  photoId: string;
};

// =================================================================================================
// PHASE-24. Distraction cleanup. ADR-0050.
//
// Nine calls. Four read, one runs the pass, three record a decision, and one is the manual tool.
//
// There is no strength on this surface, no size, and no field a description of what should be
// generated could go in. `docs/generative-policy.md` promises AURA never generates from a
// description, and the way that promise is kept is that no type here could carry one.
// =================================================================================================

/** One reason, with the pixels behind it where there are any. */
export type CleanupReasonDto = {
  code: string;
  text: string;
  weight: number;
  /** True when this code records something AURA declined to do. More than half of them do. */
  isRefusal: boolean;
  evidence: CropRectDto | null;
};

/** One proposed removal. */
export type CleanupProposalDto = {
  proposalId: string;
  photoId: string;
  region: CropRectDto;
  /** `unclassified` on every frame in this build; there is no trained detector. */
  class: string;
  classText: string;
  areaFrac: number;
  salience: number;
  /** `borrow`, `fill` or `inpaint`. */
  method: string;
  /** Set only for `borrow`: a removal that moved real pixels says where they came from. */
  borrowedFrom: string | null;
  /** Set only for `inpaint`. Never set in this build. */
  model: string | null;
  confidence: number;
  artefactScore: number;
  autonomy: string;
  scene: string;
  reasons: CleanupReasonDto[];
  /** `true` accepted, `false` rejected, `null` undecided. */
  accepted: boolean | null;
  applied: boolean;
  /** False everywhere in this build: nothing is calibrated, so every band is raised to review. */
  mayApplyUnattended: boolean;
  versions: number[];
};

/** One candidate the safety engine refused. */
export type CleanupBlockedDto = {
  region: CropRectDto;
  /** One of `size_cap`, `denylist`, `identity_protect`, `structure_span`, `confidence`. */
  check: string;
  code: string;
  text: string;
};

/** One removal that happened, for the delivery report. */
export type CleanupDisclosureDto = {
  proposalId: string;
  photoId: string;
  method: string;
  borrowedFrom: string | null;
  model: string | null;
  region: CropRectDto;
  acceptedByUser: boolean;
  artefactScore: number;
};

/** What the Cleanup panel's project header shows. */
export type CleanupStatusDto = {
  photos: number;
  examined: number;
  /** The denominator is every photograph. */
  coverage: number;
  withProposals: number;
  applied: number;
  /** By check, in `SafetyCheck::ALL` order. */
  blocked: number[];
  checkNames: string[];
  borrowed: number;
  filled: number;
  /** Zero in this build. */
  inpainted: number;
  reverted: number;
  /**
   * The number to read first. At zero, every candidate was refused for want of evidence rather
   * than for want of safety, and the blocked histogram says nothing about the photographs.
   */
  maskCovered: number;
  detectorTrained: boolean;
  inpaintAvailable: boolean;
};

export type CleanupPassInput = {
  projectId: string;
  /** Empty runs every pending photograph; a list runs phase 12's keepers only. */
  photoIds?: string[];
};

export type CleanupPassDto = {
  examined: number;
  withProposals: number;
  proposals: number;
  blocked: number[];
  reverted: number;
  judged: number;
  declined: number;
  failed: number;
  cancelled: boolean;
  elapsedMs: number;
};

export type DecideCleanupInput = {
  photoId: string;
  proposalId: string;
  /** Yes or no. There is no third thing a person can say here. */
  accept: boolean;
};

export type DisableCleanupInput = {
  photoId: string;
  disabled: boolean;
};

/**
 * Ask for one region to be removed by hand.
 *
 * It still runs the whole safety engine. A person choosing a rectangle is a reason to skip the
 * detector, not a reason to skip the filter, and the one thing it can never do - whatever anybody
 * confirms - is remove a person.
 */
export type ManualRemoveInput = {
  photoId: string;
  region: CropRectDto;
  /** The command refuses without it. Section 2.2 asks for explicit confirmation. */
  confirmed: boolean;
};

export type ManualRemoveDto = {
  proposal: CleanupProposalDto | null;
  /** Which check refused it, when one did. A refusal is a result rather than a failure. */
  blocked: CleanupBlockedDto | null;
};

// ---------------------------------------------------------------------------
// PHASE-25 - gallery consistency
// ---------------------------------------------------------------------------
//
// Two things about these shapes that a panel has to get right, both of them recorded in ADR-0052.
//
// **Both denominators are here.** `nodes` and `anchoredNodes`. A project at 100 % coverage with
// 20 % anchored has had almost nothing done to it, because an unanchored node produces a zero delta
// for every frame in it and a zero delta is still a row.
//
// **`target: null` is not a neutral target.** It means AURA could not judge that part of the
// wedding. Rendering it as zeroes turns "we could not tell" into "it needed nothing".

/** One reason a frame moved, or did not. */
export type GalleryReasonDto = {
  /** The stable slug a filter matches on. Never localised. */
  code: string;
  /** The sentence a photographer reads. */
  text: string;
  /** True when this code says AURA declined to act. */
  withdraws: boolean;
};

/** What a node's anchors say it should look like. */
export type NodeTargetDto = {
  cctK: number;
  cctTol: number;
  tint: number;
  tintTol: number;
  subjectLuma: number;
  lumaTol: number;
  contrast: number;
  saturation: number;
  anchorCount: number;
  /** How much the anchors agree, 0..1. */
  cohesion: number;
};

/** One lighting group inside one chapter. */
export type SceneNodeDto = {
  nodeId: string;
  parentId: string | null;
  segmentId: string;
  /** "Ceremony (2 of 3)". */
  label: string;
  scene: string;
  imageCount: number;
  anchors: string[];
  /** Null when the node could not be anchored. **Not** a neutral target. */
  target: NodeTargetDto | null;
  reasons: GalleryReasonDto[];
};

/**
 * How far one frame moves toward its node.
 *
 * Every `d` field is a residual on top of phases 15 and 16; the three `from` fields say what it is
 * a residual from, so a strip can draw an arrow between them.
 */
export type GalleryDeltaDto = {
  photoId: string;
  nodeId: string;
  dExposure: number;
  dCct: number;
  dTint: number;
  dContrast: number;
  dSaturation: number;
  fromExposureEv: number;
  fromCctK: number;
  fromTint: number;
  damping: number;
  /** `cct`, `tint`, `exposure`, `contrast` or `saturation`. */
  boundedBy: string | null;
  /** How much of the bounds this movement used, 0..1. */
  magnitude: number;
  skinIdentity: string | null;
  skinDe00Before: number | null;
  skinDe00After: number | null;
  confidence: number;
  reasons: GalleryReasonDto[];
  userEdited: boolean;
};

/** A frame that is still out of line after normalising. */
export type GalleryOutlierDto = {
  photoId: string;
  nodeId: string;
  /** "+310 K warmer than the anchors, skin cast 4.2 dE00", assembled from the residuals. */
  description: string;
  residualCct: number;
  residualTint: number;
  residualExposure: number;
  residualSkinDe00: number;
  deviation: number;
  reasons: GalleryReasonDto[];
};

/** What the Consistency panel's project header shows. */
export type GalleryStatusDto = {
  photos: number;
  normalised: number;
  /** The first denominator. */
  coverage: number;
  nodes: number;
  /** The second denominator, and the one that matters when it is low. */
  anchoredNodes: number;
  splitNodes: number;
  pinnedAnchors: number;
  bounded: number;
  moodPreserved: number;
  userEdited: number;
  outliers: number;
  skinTargeted: number;
  identities: number;
  spreadBeforeCct: number;
  spreadAfterCct: number;
  spreadBeforeEv: number;
  spreadAfterEv: number;
  worstSkinSpread: number;
  untargetedScenes: string[];
  /**
   * False on this build. Phase 18's segmenter is a placeholder, so no photograph has an
   * identity-scoped skin region and nothing about anybody's skin was measured.
   *
   * A panel must never render "everybody's skin is consistent" while this is false.
   */
  skinFieldAvailable: boolean;
  policyVer: number;
};

export type GalleryPassInput = {
  projectId: string;
};

/** What one consistency pass did. */
export type GalleryPassDto = {
  nodes: number;
  anchored: number;
  split: number;
  normalised: number;
  outliers: number;
  skinTargets: number;
  spreadBeforeCct: number;
  spreadAfterCct: number;
  spreadBeforeEv: number;
  spreadAfterEv: number;
  decisionsKept: number;
  cancelled: boolean;
  elapsedMs: number;
};

/** Pin or reject one photograph as an anchor of its node. */
export type PinAnchorInput = {
  nodeId: string;
  photoId: string;
  /** True to pin, false to reject. There is no third thing a person can say here. */
  pinned: boolean;
};

/**
 * What the photographer set instead, on one frame.
 *
 * Every value is bounded by the frozen contract and one outside its bound is **refused rather than
 * clamped**. There is no strength field and no way to raise a bound.
 */
export type GalleryOverrideInput = {
  photoId: string;
  /** Kelvin, within 450. */
  dCct?: number;
  /** Tint units, within 12. */
  dTint?: number;
  /** Stops, within 0.35. */
  dExposure?: number;
  /** Recipe units, within 8. */
  dContrast?: number;
  /** Recipe units, within 6. */
  dSaturation?: number;
};

export type DisableGalleryInput = {
  photoId: string;
  disabled: boolean;
};

// ---------------------------------------------------------------------------
// Multi-camera and second-shooter matching. PHASE-26, ADR-0054.
// ---------------------------------------------------------------------------
//
// The rule for every panel that renders these types: **read the evidence before the number.**
// `dCct: -300` from `source: 'matched_pairs'` with `evidencePairs: 34` and `dCct: -300` from
// `source: 'brand_baseline'` with `evidencePairs: 0` are the same arithmetic and completely
// different claims, and only the second needs a photographer to look at it.
//
// And never render a measurement claim while `baselinesMeasured` is false. Every bundled brand
// baseline in this build was fabricated rather than measured from a photographed target.

/** One reason a camera was matched the way it was. */
export type CameraReasonDto = {
  /** The stable slug a filter matches on. Never localised. */
  code: string;
  /** The sentence a photographer reads. Rendered from the code, never stored. */
  text: string;
  /** True when this code says AURA declined to correct, or corrected on less evidence. */
  withdraws: boolean;
};

/** How one body rendered this wedding, in one flash state. */
export type CameraFingerprintDto = {
  cameraId: string;
  /** `ambient` or `flash`. */
  flash: string;
  brand: string;
  /** Where this body puts skin, in CIE 1976 u'v'. */
  skinChroma: [number, number];
  /** Where it puts a neutral. */
  whitePoint: [number, number];
  highlightRolloff: number;
  subjectLuma: number;
  samples: number;
  confidence: number;
  reasons: CameraReasonDto[];
};

/** What one body needs to look like the reference. */
export type CameraTransformDto = {
  cameraId: string;
  /** `ambient` or `flash`. */
  flash: string;
  referenceId: string;
  /** Kelvin, within 900. */
  dCct: number;
  /** Tint units, within 20. */
  dTint: number;
  /** Stops, within 0.6. */
  dExposure: number;
  /** Recipe units, within 12. */
  dSaturation: number;
  channelGain: [number, number, number];
  contrastShape: [number, number, number];
  skinDe00Before: number;
  /** The number the headline promise is measured on. */
  skinDe00After: number;
  skinCapped: boolean;
  skinLocusValid: boolean;
  /** `matched_pairs`, `blended` or `brand_baseline`. Read this before any number above. */
  source: string;
  /** The share of the solved answer in the blend, `0..1`. */
  blend: number;
  evidencePairs: number;
  heldoutPairs: number;
  heldoutBefore: number;
  heldoutAfter: number;
  /** `null` is the third state: there was nothing to check the correction against. */
  heldoutImproved: boolean | null;
  boundedBy: string | null;
  magnitude: number;
  confidence: number;
  enabled: boolean;
  userEdited: boolean;
  reasons: CameraReasonDto[];
};

/** Two photographs, from two bodies, of the same conditions. */
export type MatchedPairDto = {
  pairId: string;
  leftPhotoId: string;
  rightPhotoId: string;
  flash: string;
  gapMs: number;
  subjectSimilarity: number;
  /** How much the **backgrounds** agree. The number that decides whether a pair is evidence. */
  backgroundAgreement: number;
  verified: boolean;
  heldOut: boolean;
};

/** How differently one photographer exposes, in one kind of photograph. */
export type ShooterBiasDto = {
  shooter: string;
  cameraId: string;
  scene: string;
  /** The systematic offset that was measured, in stops. */
  measuredEv: number;
  /** The part of it that is applied, in stops, opposite in sign. Always smaller. */
  appliedEv: number;
  frames: number;
  capped: boolean;
  reasons: CameraReasonDto[];
};

/** One body's report, in a photographer's own words. */
export type CameraReportDto = {
  cameraId: string;
  flash: string;
  shooter: string | null;
  isReference: boolean;
  /** The one line a collapsed row shows. Reads the reasons, never the magnitude. */
  headline: string;
  evidence: string;
  corrections: string[];
  withdrawals: string[];
  skinDe00After: number;
  meetsPromise: boolean;
  magnitude: number;
  confidence: number;
};

/** What the Camera Match panel's project header shows. */
export type CameraStatusDto = {
  photos: number;
  matched: number;
  coverage: number;
  cameras: number;
  fingerprinted: number;
  /** The number that matters when it is low: how many corrections rest on this wedding's own. */
  solvedFromPairs: number;
  blended: number;
  baselineOnly: number;
  pairs: number;
  pairsRejected: number;
  heldoutPairs: number;
  flashSeparated: number;
  shootersMeasured: number;
  shootersCapped: number;
  disabled: number;
  userEdited: number;
  skinDe00Before: number;
  skinDe00After: number;
  worstSkinDe00: number;
  referenceId: string | null;
  referenceSource: string;
  unknownBrands: string[];
  /** **False in this build.** Every bundled brand baseline was fabricated. */
  baselinesMeasured: boolean;
  /** **False in this build.** No photograph carries an identity-scoped skin region. */
  skinFieldAvailable: boolean;
  policyVer: number;
};

/** Ask for a project's cameras to be matched. */
export type CameraPassInput = {
  projectId: string;
};

/** What one matching pass did. */
export type CameraPassDto = {
  cameras: number;
  referenceId: string | null;
  referenceSource: string;
  pairs: number;
  pairsRejected: number;
  heldoutPairs: number;
  solved: number;
  blended: number;
  baselineOnly: number;
  heldoutFailures: number;
  distanceBefore: number;
  distanceAfter: number;
  signatureBefore: number;
  signatureAfter: number;
  worstSkinDe00: number;
  shootersMeasured: number;
  shootersCapped: number;
  /** One paragraph, in the product's own words. */
  summary: string;
};

/** Choose the body everything else is matched to. */
export type SetCameraReferenceInput = {
  projectId: string;
  cameraId: string;
};

/** Switch matching off for one body, or back on. Both flash states move together. */
export type DisableCameraInput = {
  projectId: string;
  cameraId: string;
  disabled: boolean;
};

/**
 * What the photographer set instead, for one body in one flash state.
 *
 * Four optional movements, every one bounded by the frozen contract and refused rather than clamped
 * when it is outside. There is no strength field and no way to raise a bound.
 */
export type CameraOverrideInput = {
  projectId: string;
  cameraId: string;
  /** `ambient` or `flash`. */
  flash: string;
  dCct?: number | null;
  dTint?: number | null;
  dExposure?: number | null;
  dSaturation?: number | null;
};

// ---------------------------------------------------------------------------
// PHASE-27 - quality control
// ---------------------------------------------------------------------------
//
// The first surface in this product whose primary object is a **problem**. Every earlier panel
// answers "what did AURA decide about this photograph"; this one answers "what does AURA think is
// wrong with what it decided", so the reader arrives sceptical and every number that would let
// them check a finding travels beside the sentence.
//
// Read `QcStatusDto.completeness` first. A QC panel is the only place in this product where an
// empty result is genuinely ambiguous - zero findings means either "AURA looked at everything and
// it is fine" or "AURA could not look" - and in this build the second is the common case, because
// phase 06's detector finds no faces and phase 18's segmenter is untrained. `inspectionsSkipped`
// is on the wire beside it, and `detectorTrained` is false.

/** What the QC panel's project header shows. */
export type QcStatusDto = {
  /** Photographs in the delivered gallery. Phase 18's denominator, not the project's. */
  selected: number;
  /** Photographs the pass reached. */
  checked: number;
  /** Fraction of the gallery inspected, 0..1. */
  coverage: number;
  /** Inspections that ran. */
  inspections: number;
  /**
   * Inspections that could not run because something they needed was absent.
   *
   * The number that makes the rest honest. A category with zero findings and four hundred skips
   * is not a clean category.
   */
  inspectionsSkipped: number;
  /** Fraction of attempted inspections that actually ran, 0..1. */
  completeness: number;
  /** Findings still wanting somebody's attention. */
  open: number;
  /** Findings a photographer agreed with. */
  accepted: number;
  /** Findings a photographer rejected. The false-ticket numerator. */
  dismissed: number;
  /**
   * Fraction of reviewed findings a photographer disagreed with, 0..1.
   *
   * The denominator is findings somebody looked at, not findings that exist: a queue nobody has
   * opened has no disagreement rate, and reporting one as zero would read as unanimous agreement.
   */
  falseTicketRate: number;
  /** Frames replaced by a better alternative. */
  replaced: number;
  /** Remediation rounds run. */
  rounds: number;
  /** Planner calls made, out of forty. */
  plannerCalls: number;
  /** Findings in each category, in QcCategory::ALL order. */
  byCategory: number[];
  /** Findings in each status, in TicketStatus::ALL order. */
  byStatus: number[];
  /** Bytes this phase occupies for the project. */
  bytes: number;
  /** Which thresholds table. */
  thresholdsVer: number;
  /** Which arithmetic. */
  analysisVer: number;
  /**
   * False in this build. No defect-detection model ships and every check is a measurement against
   * another phase's stored number.
   */
  detectorTrained: boolean;
};

/** One finding, as a photographer reads it. */
export type QcTicketDto = {
  /** The finding. */
  ticketId: string;
  /** The photograph. */
  imageId: string;
  /** Which inspection. */
  category: string;
  /** What exactly. */
  code: string;
  /** The senior retoucher's note, rendered from the numbers rather than stored. */
  diagnosis: string;
  /** How far from acceptable. */
  deviation: number;
  /** What acceptable was. */
  threshold: number;
  /** What both are measured in. */
  unit: string;
  /** How far past the threshold, as a multiple of it. What the queue sorts on. */
  severity: number;
  /** What should be done, as one of the five remedy slugs. */
  remedyKind: string;
  /** What it acts on. */
  remedyTarget: string;
  /** How much the deviation should fall if it is applied. */
  expectedGain: number;
  /** How sure, 0..1. */
  confidence: number;
  /** What the product is allowed to do about it. */
  autonomy: string;
  /** True when the confidence and the band both permit acting without a person. */
  mayActUnattended: boolean;
  /** Which round it is on, out of two. */
  round: number;
  /** Where it stands. */
  status: string;
  /** What happened to it, when something has. */
  outcomeCode?: string | null;
  /** The scene, for the panel's own grouping. */
  scene: string;
  /** What to look at: none | crop | frames | anchors | params. */
  evidenceKind: string;
  /** The frames the finding was measured against, when it names any. */
  evidenceFrames: string[];
  /** The region of this frame, as [x, y, w, h] normalised, when it names one. */
  evidenceCrop?: number[] | null;
  /** The reasons, strongest first, as sentences. */
  reasons: string[];
};

/** One category's findings, worst first. */
export type QcGroupDto = {
  /** Which inspection. */
  category: string;
  /** The worst severity in the group, which is what orders the groups themselves. */
  worst: number;
  /** The findings. */
  tickets: QcTicketDto[];
};

/** One remediation attempt, and whether it worked. */
export type QcRoundDto = {
  /** Which round, one or two. */
  round: number;
  /** What was tried. */
  remedyKind: string;
  /** What it acted on. */
  remedyTarget: string;
  /** The deviation before. */
  deviationBefore: number;
  /** And after. */
  deviationAfter: number;
  /** What was predicted. */
  expectedGain: number;
  /** The share of that prediction actually realised. The number the loop decided on. */
  realisedShare: number;
  /** The worst movement in another check, as a share of that check's own threshold. */
  collateral: number;
  /** Which check took it. */
  collateralCategory?: string | null;
  /** Whether the change survived. */
  kept: boolean;
  /** What happened, as a code. */
  outcome: string;
  /** How long, in milliseconds. */
  ms: number;
};

/** One frame swapped for another. */
export type QcReplacementDto = {
  /** The finding that caused it. */
  ticketId: string;
  /** The frame that was in the gallery. */
  replaced: string;
  /** The frame that is in it now. */
  replacement: string;
  /** Which metric decided it. */
  category: string;
  /** What the replaced frame measured. */
  metricBefore: number;
  /**
   * What the replacement measures.
   *
   * Both, never the difference: a photographer looking at a swap wants to know what each frame
   * measured, and a stored subtraction cannot be read back as two numbers.
   */
  metricAfter: number;
  /** How sure. Never below 0.85 on an automatic swap. */
  confidence: number;
  /** True on every stored swap: coverage was re-validated and held. */
  coverageHeld: boolean;
  /** One sentence about why. */
  note: string;
};

/** One category's tally in the report. */
export type QcTallyDto = {
  /** Which inspection. */
  category: string;
  /** Findings opened. */
  found: number;
  /** Findings a remedy fixed and re-inspection confirmed. */
  fixed: number;
  /** Findings handed to a person. */
  escalated: number;
  /** Frames this check could not run on. */
  skipped: number;
};

/** What one QC pass did. */
export type QcReportDto = {
  /** Photographs inspected. */
  images: number;
  /** Photographs the pass did not reach before its time ran out. */
  imagesUnreached: number;
  /** True when it reached every frame it was asked to. */
  complete: boolean;
  /** Inspections that ran. */
  checksRun: number;
  /** Inspections that could not. */
  skipped: number;
  /** One row per category. */
  byCategory: QcTallyDto[];
  /** Findings opened, across every category. */
  found: number;
  /** Remedies applied and kept. */
  fixed: number;
  /** Remedies applied and put back. */
  reverted: number;
  /** Findings handed to a person. */
  escalated: number;
  /** Every swap, with its before and after. */
  replacements: QcReplacementDto[];
  /** Planner calls made. */
  plannerCalls: number;
  /** True when the planner was reached at all. */
  cloudUsed: boolean;
  /** How long, in milliseconds. */
  durationMs: number;
  /** Which thresholds table. */
  thresholdsVer: number;
  /** Which arithmetic. */
  analysisVer: number;
};

/** Ask for a QC pass. */
export type QcPassInput = {
  /** The project. */
  projectId: string;
  /**
   * True to apply remedies the autonomy bands permit; false to inspect and report only.
   *
   * The safe default is false, and phase 28 is the caller that sets it true. A pass that only
   * inspects changes nothing, which is what makes it the thing to run before a delivery.
   */
  remediate: boolean;
};

/**
 * What a photographer decided about one finding.
 *
 * `status` may only be `accepted` or `dismissed`. Automation owns `open`, `fixed`, `reverted` and
 * `escalated`, which are a record of what happened rather than an opinion about it.
 */
export type QcDecideInput = {
  /** The finding. */
  ticketId: string;
  /** `accepted` or `dismissed`. */
  status: string;
  /**
   * Apply the proposed remedy now, whatever the autonomy band said.
   *
   * A photographer overruling a review requirement upward is the one direction that is safe: they
   * have looked. There is no field that overrules it downward.
   */
  applyRemedy: boolean;
  /** One sentence, kept for the studio's record. At most 280 characters. */
  note?: string | null;
};

/**
 * What a photographer decided about many findings at once.
 *
 * There is no `applyRemedy` here, and that is the decision rather than an omission. Agreeing that
 * forty findings are real is a statement about the findings; instructing AURA to act on forty
 * frames unattended is a statement about the remedies, and the two are different judgements made
 * with different amounts of attention. ADR-0056 section 5.
 */
export type QcDecideBulkInput = {
  /** The project, for the audit trail. */
  projectId: string;
  /** The findings. */
  ticketIds: string[];
  /** `accepted` or `dismissed`. */
  status: string;
  /** One sentence, applied to all of them. */
  note?: string | null;
};

// ---------------------------------------------------------------------------
// PHASE-28. The zero-touch autopilot.
// ---------------------------------------------------------------------------
//
// The primary object here is a **run**, which is the first subject on this surface that a
// photographer starts and then walks away from. Every earlier panel answers a question about a
// photograph that is on the screen; this one has to answer, to somebody who has come back two
// hours later: what did you do, what did you not do, and why.
//
// That is why `AutopilotStageDto` carries `skipCause` and `skipText` beside the outcome, why the
// summary carries `degradedStages` as a list rather than a count, and why `calibrated` is on the
// wire at all.

/** What the Autopilot panel's header shows. */
export type AutopilotStatusDto = {
  /** Runs this wedding has had. */
  runs: number;
  /** The newest run's id, when there is one. */
  latestRun: string | null;
  /** Its status slug: `running`, `completed`, `completed_degraded`, `cancelled` or `failed`. */
  status: string | null;
  /** Stages the photographer asked for. */
  stagesEnabled: number;
  /** Stages that did their work. */
  stagesCompleted: number;
  /** Stages that could not. */
  stagesDegraded: number;
  /** `stagesCompleted / stagesEnabled`, or zero when nothing has run. */
  completeness: number;
  /** Whether the newest run was unattended. */
  zeroTouch: boolean;
  /**
   * Whether this build's confidences have been calibrated.
   *
   * False on every build shipped so far, and the most consequential field on this shape: it is why
   * the panel says AURA is being careful, rather than leaving a photographer to wonder why
   * Zero-Touch queued four hundred frames.
   */
  calibrated: boolean;
  /** How many times the governor asked the run to slow down. */
  resourceEvents: number;
  /** Bytes migration 28 holds for this wedding. */
  bytes: number;
  /** The checklist file's version. */
  policyVer: number;
  /** The orchestrator's own version. */
  orchestratorVer: number;
};

/** One pre-flight row. */
export type AutopilotPreflightRowDto = {
  /** Which check. */
  check: string;
  /** The words a photographer reads. */
  title: string;
  /** `pass`, `warn` or `block`. */
  verdict: string;
  /** What to do about it. Never empty. */
  detail: string;
};

/** Everything the pre-flight found. */
export type AutopilotPreflightDto = {
  /** The strongest verdict in the report. */
  verdict: string;
  /** Whether the run may start. */
  permitsStart: boolean;
  /** How many photographs the run would work on. */
  images: number;
  /** Bytes the run expects to write. */
  estimatedOutputBytes: number;
  /** The whole run's estimate, from the declared per-item estimates. */
  estimatedMs: number;
  /** Every row, in check order. */
  rows: AutopilotPreflightRowDto[];
};

/** What the run in flight is doing right now. */
export type AutopilotProgressDto = {
  /** The run. */
  runId: string;
  /** Its status slug. */
  status: string;
  /** The stage slug. */
  stage: string;
  /** The words a photographer reads. */
  stageTitle: string;
  /** Its position in the plan, from zero. */
  stageIndex: number;
  /** How many stages are in the plan. */
  stageTotal: number;
  /** Units finished in this stage. */
  itemsDone: number;
  /** Units this stage has to do. */
  itemsTotal: number;
  /** Seconds left for the whole run. */
  etaS: number;
  /** Units per second, measured over this stage. */
  throughputPerS: number;
  /** What the run has spent on cloud calls. */
  spendUsd: number;
  /** Anything worth saying that is not a failure. */
  warnings: string[];
  /** The photograph being worked on, for the thumbnail. */
  currentImage: string | null;
  /** Whether a stop has been asked for. */
  cancelled: boolean;
};

/** One stage of one run. */
export type AutopilotStageDto = {
  /** The stage slug. */
  stage: string;
  /** The words a photographer reads. */
  title: string;
  /** `completed`, `partial`, `skipped`, `failed` or `running`. */
  outcome: string;
  /**
   * Why it did not run, when it did not.
   *
   * Present exactly when `outcome` is `skipped`, and separate from it because "skipped" and
   * "skipped because you turned it off" are the difference between a degraded run and a complete
   * one.
   */
  skipCause: string | null;
  /** The same, in the photographer's own words. */
  skipText: string | null;
  /** What the autonomy gate said: `act`, `act_and_review` or `hold`. */
  verdict: string;
  /** Units finished. */
  itemsDone: number;
  /** Units it had to do. */
  itemsTotal: number;
  /** Milliseconds of wall clock. */
  elapsedMs: number;
  /** How many attempts it took. */
  attempts: number;
  /** The reason codes attached to it. */
  reasons: string[];
};

/** What a finished run did. */
export type AutopilotSummaryDto = {
  /** The run. */
  runId: string;
  /** Its status slug. */
  status: string;
  /** The words a photographer reads. */
  statusTitle: string;
  /** How many photographs were selected. */
  selected: number;
  /** How many files were written. */
  exported: number;
  /** How many frames a person is being asked to look at. */
  needsReview: number;
  /** Total wall clock across every stage. */
  totalMs: number;
  /** What the run spent on cloud calls. */
  spendUsd: number;
  /** Where the delivered files are. */
  outputPath: string;
  /** How long each stage took, in execution order. */
  stageTimings: [string, number][];
  /** Every stage that did not do what it was meant to, with the reason. */
  degradedStages: [string, string][];
};

/** One thing the governor noticed, and what it did. */
export type AutopilotEventDto = {
  /** `vram`, `ram`, `thermal`, `battery`, `disk`, `quiet` or `device_lost`. */
  kind: string;
  /** `reduce`, `pause` or `stop`. Never `proceed`. */
  action: string;
  /** The sentence a photographer reads. */
  actionText: string;
  /** The reading, in the kind's own units. */
  reading: number;
  /** What it was compared against. */
  threshold: number;
  /** The stage that was running. */
  stage: string;
};

/** Start a run. */
export type AutopilotStartInput = {
  /** The wedding. */
  projectId: string;
  /** Stage slugs the photographer switched off. */
  disabled: string[];
  /**
   * Whether the run may act unattended where phase 13's bands allow.
   *
   * The only autonomy field on this surface, and a boolean rather than a level: what it unlocks is
   * decided by the bands, not here. A field that could name a band would be a field that routed
   * around phase 13.
   */
  zeroTouch: boolean;
  /** Whether heavy stages may run on battery power. */
  allowOnBattery: boolean;
  /** Whether the run yields to foreground work. */
  quietMode: boolean;
};

/** Record what the photographer chose in the checklist. */
export type AutopilotSettingsInput = {
  /** The wedding. */
  projectId: string;
  /** Stage slugs to switch off. */
  disabled: string[];
  /** Whether the run may act unattended. */
  zeroTouch: boolean;
  /** Whether heavy stages may run on battery power. */
  allowOnBattery: boolean;
  /** Whether the run yields to foreground work. */
  quietMode: boolean;
};
