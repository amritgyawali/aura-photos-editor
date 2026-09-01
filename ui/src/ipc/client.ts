import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
  // PHASE-28.
  AutopilotEventDto,
  AutopilotPreflightDto,
  AutopilotProgressDto,
  AutopilotSettingsInput,
  AutopilotStageDto,
  AutopilotStartInput,
  AutopilotStatusDto,
  AutopilotSummaryDto,
  // PHASE-17.,
  // PHASE-19.,
  // PHASE-20.,
  // PHASE-21.,
  // PHASE-22.,
  // PHASE-23.,
  AcceptColourInput,
  AcceptGeometryInput,
  AcceptLocalInput,
  AcceptMicroInput,
  AcceptRestoreInput,
  AcceptRetouchInput,
  AcceptToneInput,
  AdoptProfileInput,
  AnalyseCompositionInput,
  AnalyseIntegrityInput,
  CacheStatsDto,
  CameraFingerprintDto,
  CameraOverrideInput,
  CameraPassDto,
  CameraPassInput,
  CameraReasonDto,
  CameraReportDto,
  CameraStatusDto,
  CameraTransformDto,
  ChapterHandleDto,
  ClassifyScenesInput,
  CleanupBlockedDto,
  CleanupDisclosureDto,
  CleanupPassDto,
  CleanupPassInput,
  CleanupProposalDto,
  CleanupReasonDto,
  CleanupStatusDto,
  CloudCacheStatsDto,
  CloudCallDto,
  CloudEvent,
  CloudSpendDto,
  CloudStatusDto,
  ColourDto,
  ColourPassDto,
  ColourReviewInput,
  ColourStatusDto,
  CompareProfilesInput,
  CompositionDto,
  CompositionPassDto,
  CompositionStatusDto,
  CreateProjectInput,
  CullPassDto,
  CullProjectInput,
  CullStatusDto,
  DecideCleanupInput,
  DecisionDto,
  DescriptorsDto,
  DevelopImageInput,
  DevelopStatusDto,
  DisableCameraInput,
  DisableCleanupInput,
  DisableGalleryInput,
  DismissCompositionFlagInput,
  DismissFlagInput,
  DuplicateSetDto,
  EditMaskInput,
  EmbedProgressDto,
  EmbedProjectInput,
  EmotionDto,
  EmotionEvent,
  EmotionPassDto,
  EmotionStatusDto,
  EnsureMasksInput,
  EraseBiometricsDto,
  EraseBiometricsInput,
  EstimateColourInput,
  EstimateToneInput,
  ExplainPanelDto,
  ExportBundleInput,
  ExportProfileDto,
  ExportProfileInput,
  FaceCropDto,
  FindSimilarInput,
  FlaggedCompositionInput,
  FlaggedInput,
  GalleryDeltaDto,
  GalleryOutlierDto,
  GalleryOverrideInput,
  GalleryPassDto,
  GalleryPassInput,
  GalleryReasonDto,
  GalleryStatusDto,
  GeometryPassDto,
  GeometryPlanDto,
  GeometryReviewInput,
  GeometryStatusDto,
  GetPreviewInput,
  GroupMomentsInput,
  GroupPeopleDto,
  GroupPeopleInput,
  HardwarePlanDto,
  HistoryDto,
  HistoryStepInput,
  IdentityCardDto,
  IdentityHandleDto,
  IdentityTimelineDto,
  ImageRowLite,
  ImageSubjectsDto,
  ImportProfileDto,
  ImportProfileInput,
  IndexEvent,
  IndexStatusDto,
  InferEvent,
  InferStatsDto,
  IngestEvent,
  IntegrityDto,
  IntegrityEvent,
  IntegrityPassDto,
  IntegrityStatusDto,
  IpcError,
  JobHandle,
  KeyCheckDto,
  LedgerDecisionDto,
  LedgerStatusDto,
  ListImagesInput,
  LocalPassDto,
  LocalPlanDto,
  LocalReviewInput,
  LocalStatusDto,
  LockMomentInput,
  ManualRemoveDto,
  ManualRemoveInput,
  MaskAllowanceDto,
  MaskDto,
  MaskOverlayDto,
  MaskStatusDto,
  MatchedPairDto,
  MergeChaptersInput,
  MergeIdentitiesInput,
  MergeMomentsInput,
  MicroCompositeDto,
  MicroMatrixDto,
  MicroPassDto,
  MicroPassInput,
  MicroPlanDto,
  MicroReasonDto,
  MicroReviewInput,
  MicroStatusDto,
  ModelStatusDto,
  MomentDto,
  MomentEditDto,
  MomentEvent,
  MomentHandleDto,
  MomentListDto,
  MomentPeakDto,
  MomentStatusDto,
  MomentsInput,
  MoveBoundaryInput,
  OverrideDecisionInput,
  PeopleEvent,
  PeopleStatusDto,
  PinAnchorInput,
  PlanGeometryInput,
  PreferInput,
  PrefetchInput,
  PreviewEvent,
  PreviewPayload,
  ProblemRow,
  ProfileReportDto,
  ProjectHandle,
  ProjectSummary,
  ProtectedFeatureDto,
  QcDecideBulkInput,
  QcDecideInput,
  QcGroupDto,
  QcPassInput,
  QcReportDto,
  QcRoundDto,
  QcStatusDto,
  QcTicketDto,
  RankedByEmotionDto,
  RankedFrameDto,
  RankedInput,
  ReactionLinkDto,
  RecipeDto,
  RecordDecisionsDto,
  RecordDecisionsInput,
  ReferenceFrameDto,
  ReferenceFramesInput,
  RenameIdentityInput,
  RenderCapsDto,
  RenderDto,
  RenderImageInput,
  ResizeGalleryInput,
  RestoreIdentityRefusalDto,
  RestorePassDto,
  RestorePassInput,
  RestorePlanDto,
  RestoreReasonDto,
  RestoreReviewInput,
  RestoreStatusDto,
  RetouchPassDto,
  RetouchPassInput,
  RetouchPlanDto,
  RetouchReviewInput,
  RetouchStatusDto,
  ReviewQueueInput,
  ScanArchiveDto,
  ScanArchiveInput,
  ScanFacesDto,
  ScanFacesInput,
  SceneDto,
  SceneNodeDto,
  SceneProfileDto,
  ScoreEmotionInput,
  SculptLocalInput,
  SelectVariantInput,
  SelectionDto,
  SetAiKeyInput,
  SetCacheBudgetInput,
  SetCameraLabelInput,
  SetCameraReferenceInput,
  SetChapterInput,
  SetCloudBudgetInput,
  SetCloudPrivacyInput,
  SetColourOverrideDto,
  SetColourOverrideInput,
  SetCullModeInput,
  SetExecutionProviderInput,
  SetFramingDto,
  SetFramingInput,
  SetIdentityImportanceInput,
  SetIdentityRoleInput,
  SetKeepHintInput,
  SetLocalStrengthDto,
  SetLocalStrengthInput,
  SetMicroMatrixInput,
  SetParamDto,
  SetParamInput,
  SetPeakInput,
  SetProjectProfileInput,
  SetProtectionInput,
  SetRestoreOverrideInput,
  SetRetouchDto,
  SetRetouchInput,
  SetToneOverrideDto,
  SetToneOverrideInput,
  ShooterBiasDto,
  SimilarResultDto,
  SnapshotInput,
  SplitChapterInput,
  SplitIdentityInput,
  SplitMomentInput,
  StartIngestInput,
  StoryEvent,
  StoryOutlineDto,
  StoryStatusDto,
  StyleComparisonDto,
  StylePairDto,
  StyleProfileDto,
  StyleStatusDto,
  SupportBundleDto,
  ToneDto,
  TonePassDto,
  ToneReviewInput,
  ToneStatusDto,
  TrainProfileDto,
  TrainProfileInput,
  WarmupReportDto,
  WithinMomentInput,
} from './types';

/** True when the shell is present. Storybook-style dev runs fall back to stubs. */
export const inTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Narrow an unknown rejection into the typed error the backend promises. */
export const asIpcError = (error: unknown): IpcError => {
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'message' in error &&
    'runbookUrl' in error
  ) {
    return error as IpcError;
  }
  return {
    code: 'AURA-DB-3006',
    message: 'AURA hit an unexpected problem and stopped safely.',
    runbookUrl: 'https://aura.app/e/AURA-DB-3006',
    retryable: false,
  };
};

export const api = {
  createProject: (input: CreateProjectInput): Promise<ProjectHandle> =>
    invoke<ProjectHandle>('create_project', { input }),

  listProjects: (): Promise<ProjectSummary[]> => invoke<ProjectSummary[]>('list_projects'),

  startIngest: (input: StartIngestInput): Promise<JobHandle> =>
    invoke<JobHandle>('start_ingest', { input }),

  cancelJob: (jobId: string): Promise<boolean> => invoke<boolean>('cancel_job', { jobId }),

  listImages: (input: ListImagesInput): Promise<ImageRowLite[]> =>
    invoke<ImageRowLite[]>('list_images', { input }),

  setCameraLabel: (input: SetCameraLabelInput): Promise<void> =>
    invoke<void>('set_camera_label', { input }),

  listProblems: (projectId: string): Promise<ProblemRow[]> =>
    invoke<ProblemRow[]>('list_problems', { projectId }),

  onIngestEvent: (handler: (event: IngestEvent) => void): Promise<() => void> =>
    listen<IngestEvent>('ingest', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),

  getPreview: (input: GetPreviewInput): Promise<PreviewPayload> =>
    invoke<PreviewPayload>('get_preview', { input }),

  prefetchPreviews: (input: PrefetchInput): Promise<number> =>
    invoke<number>('prefetch_previews', { input }),

  cancelPreviews: (projectId: string, photoIds: string[]): Promise<number> =>
    invoke<number>('cancel_previews', { projectId, photoIds }),

  previewStats: (projectId: string): Promise<CacheStatsDto> =>
    invoke<CacheStatsDto>('preview_stats', { projectId }),

  setCacheBudget: (input: SetCacheBudgetInput): Promise<CacheStatsDto> =>
    invoke<CacheStatsDto>('set_cache_budget', { input }),

  purgeCache: (projectId: string): Promise<CacheStatsDto> =>
    invoke<CacheStatsDto>('purge_cache', { projectId }),

  onPreviewEvent: (handler: (event: PreviewEvent) => void): Promise<() => void> =>
    listen<PreviewEvent>('preview', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),

  hardwarePlan: (): Promise<HardwarePlanDto> => invoke<HardwarePlanDto>('hardware_plan'),

  recheckHardware: (): Promise<HardwarePlanDto> => invoke<HardwarePlanDto>('recheck_hardware'),

  setExecutionProvider: (input: SetExecutionProviderInput): Promise<HardwarePlanDto> =>
    invoke<HardwarePlanDto>('set_execution_provider', { input }),

  listModels: (): Promise<ModelStatusDto[]> => invoke<ModelStatusDto[]>('list_models'),

  warmupModels: (): Promise<WarmupReportDto> => invoke<WarmupReportDto>('warmup_models'),

  inferStats: (): Promise<InferStatsDto> => invoke<InferStatsDto>('infer_stats'),

  onInferEvent: (handler: (event: InferEvent) => void): Promise<() => void> =>
    listen<InferEvent>('infer', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),

  cloudStatus: (): Promise<CloudStatusDto> => invoke<CloudStatusDto>('cloud_status'),

  /**
   * The one command that carries the key, and it carries it one way only.
   * There is deliberately no `getAiKey`.
   */
  setAiKey: (input: SetAiKeyInput): Promise<CloudStatusDto> =>
    invoke<CloudStatusDto>('set_ai_key', { input }),

  clearAiKey: (provider: string): Promise<CloudStatusDto> =>
    invoke<CloudStatusDto>('clear_ai_key', { provider }),

  checkAiKey: (): Promise<KeyCheckDto> => invoke<KeyCheckDto>('check_ai_key'),

  setCloudBudget: (input: SetCloudBudgetInput): Promise<CloudSpendDto> =>
    invoke<CloudSpendDto>('set_cloud_budget', { input }),

  setCloudPrivacy: (input: SetCloudPrivacyInput): Promise<CloudStatusDto> =>
    invoke<CloudStatusDto>('set_cloud_privacy', { input }),

  cloudSpend: (projectId: string): Promise<CloudSpendDto> =>
    invoke<CloudSpendDto>('cloud_spend', { projectId }),

  cloudCalls: (projectId: string, limit: number): Promise<CloudCallDto[]> =>
    invoke<CloudCallDto[]>('cloud_calls', { projectId, limit }),

  cloudCacheStats: (): Promise<CloudCacheStatsDto> =>
    invoke<CloudCacheStatsDto>('cloud_cache_stats'),

  purgeCloudCache: (task: string, taskVersion: number): Promise<number> =>
    invoke<number>('purge_cloud_cache', { task, taskVersion }),

  onCloudEvent: (handler: (event: CloudEvent) => void): Promise<() => void> =>
    listen<CloudEvent>('cloud', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),

  findSimilar: (input: FindSimilarInput): Promise<SimilarResultDto> =>
    invoke<SimilarResultDto>('find_similar', { input }),

  indexStatus: (projectId: string): Promise<IndexStatusDto> =>
    invoke<IndexStatusDto>('index_status', { projectId }),

  buildIndex: (projectId: string): Promise<IndexStatusDto> =>
    invoke<IndexStatusDto>('build_index', { projectId }),

  embedProject: (input: EmbedProjectInput): Promise<EmbedProgressDto> =>
    invoke<EmbedProgressDto>('embed_project', { input }),

  imageDescriptors: (projectId: string, photoId: string): Promise<DescriptorsDto> =>
    invoke<DescriptorsDto>('image_descriptors', { projectId, photoId }),

  onIndexEvent: (handler: (event: IndexEvent) => void): Promise<() => void> =>
    listen<IndexEvent>('index', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),

  peopleStatus: (projectId: string): Promise<PeopleStatusDto> =>
    invoke<PeopleStatusDto>('people_status', { projectId }),

  listIdentities: (projectId: string): Promise<IdentityCardDto[]> =>
    invoke<IdentityCardDto[]>('list_identities', { projectId }),

  imageSubjects: (photoId: string): Promise<ImageSubjectsDto> =>
    invoke<ImageSubjectsDto>('image_subjects', { photoId }),

  scanFaces: (input: ScanFacesInput): Promise<ScanFacesDto> =>
    invoke<ScanFacesDto>('scan_faces', { input }),

  groupPeople: (input: GroupPeopleInput): Promise<GroupPeopleDto> =>
    invoke<GroupPeopleDto>('group_people', { input }),

  mergeIdentities: (input: MergeIdentitiesInput): Promise<IdentityHandleDto> =>
    invoke<IdentityHandleDto>('merge_identities', { input }),

  splitIdentity: (input: SplitIdentityInput): Promise<IdentityHandleDto> =>
    invoke<IdentityHandleDto>('split_identity', { input }),

  /** The "this is the bride" button. Sets `userLocked`; automation may not undo it. */
  setIdentityRole: (input: SetIdentityRoleInput): Promise<void> =>
    invoke<void>('set_identity_role', { input }),

  renameIdentity: (input: RenameIdentityInput): Promise<void> =>
    invoke<void>('rename_identity', { input }),

  setIdentityImportance: (input: SetIdentityImportanceInput): Promise<void> =>
    invoke<void>('set_identity_importance', { input }),

  identityTimelines: (projectId: string): Promise<IdentityTimelineDto[]> =>
    invoke<IdentityTimelineDto[]>('identity_timelines', { projectId }),

  /**
   * The only route from the sealed biometric store to a screen. One crop per call,
   * decoded in Rust; there is deliberately no command that returns a template.
   */
  identityCover: (projectId: string, faceId: string): Promise<FaceCropDto> =>
    invoke<FaceCropDto>('identity_cover', { projectId, faceId }),

  /**
   * Irreversible. `confirm` must equal `projectId`, and the backend refuses otherwise -
   * the check is there rather than only in the dialog because this is the one operation
   * in the product with no undo.
   */
  eraseBiometrics: (input: EraseBiometricsInput): Promise<EraseBiometricsDto> =>
    invoke<EraseBiometricsDto>('erase_biometrics', { input }),

  onPeopleEvent: (handler: (event: PeopleEvent) => void): Promise<() => void> =>
    listen<PeopleEvent>('people', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),

  // -- PHASE-07: the story surface -----------------------------------------

  /** What the timeline opens with. One read; section 11 budgets 200 ms. */
  storyOutline: (projectId: string): Promise<StoryOutlineDto> =>
    invoke<StoryOutlineDto>('story_outline', { projectId }),

  storyStatus: (projectId: string): Promise<StoryStatusDto> =>
    invoke<StoryStatusDto>('story_status', { projectId }),

  /** What one photograph is of, for the Explain panel. Null when unclassified. */
  imageScene: (photoId: string): Promise<SceneDto | null> =>
    invoke<SceneDto | null>('image_scene', { photoId }),

  /**
   * Every scene's tolerances, with the sentence explaining them. The rationale is
   * on the wire because "why is my dance floor being judged this way" is a
   * question a photographer asks, and invariant 2 says it has to have an answer.
   */
  sceneProfiles: (projectId: string): Promise<SceneProfileDto[]> =>
    invoke<SceneProfileDto[]>('scene_profiles', { projectId }),

  classifyScenes: (input: ClassifyScenesInput): Promise<StoryStatusDto> =>
    invoke<StoryStatusDto>('classify_scenes', { input }),

  segmentStory: (projectId: string): Promise<StoryOutlineDto> =>
    invoke<StoryOutlineDto>('segment_story', { projectId }),

  /** Rename a chapter. Sets `userLocked`; re-analysis may not undo it. */
  setChapter: (input: SetChapterInput): Promise<ChapterHandleDto> =>
    invoke<ChapterHandleDto>('set_chapter', { input }),

  /** Move a boundary. Locks BOTH chapters either side of it - a boundary is shared. */
  moveChapterBoundary: (input: MoveBoundaryInput): Promise<ChapterHandleDto> =>
    invoke<ChapterHandleDto>('move_chapter_boundary', { input }),

  splitChapter: (input: SplitChapterInput): Promise<ChapterHandleDto> =>
    invoke<ChapterHandleDto>('split_chapter', { input }),

  mergeChapters: (input: MergeChaptersInput): Promise<ChapterHandleDto> =>
    invoke<ChapterHandleDto>('merge_chapters', { input }),

  onStoryEvent: (handler: (event: StoryEvent) => void): Promise<() => void> =>
    listen<StoryEvent>('story', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),

  // -- PHASE-08: the moments surface ---------------------------------------

  /**
   * Every moment in a wedding, or one chapter's. The grid virtualises this list
   * exactly as it virtualises frames.
   */
  listMoments: (input: MomentsInput): Promise<MomentListDto> =>
    invoke<MomentListDto>('list_moments', { input }),

  momentStatus: (projectId: string): Promise<MomentStatusDto> =>
    invoke<MomentStatusDto>('moment_status', { projectId }),

  /** The moment one photograph is in. Null when it has not been grouped. */
  momentOfImage: (photoId: string): Promise<MomentDto | null> =>
    invoke<MomentDto | null>('moment_of_image', { photoId }),

  /**
   * The duplicate sets inside one moment - only the ones that cap the gallery.
   * The alternatives phase 12 chooses between are the moment's frames grouped by
   * `burstIx`; a variant set is not stored because it constrains nothing.
   */
  momentDuplicates: (momentId: string): Promise<DuplicateSetDto[]> =>
    invoke<DuplicateSetDto[]>('moment_duplicates', { momentId }),

  /** Regroup a wedding. Preserves every moment the photographer has locked. */
  groupMoments: (input: GroupMomentsInput): Promise<MomentStatusDto> =>
    invoke<MomentStatusDto>('group_moments', { input }),

  /** Break a moment in two. Locks BOTH halves - a split is one statement about two. */
  splitMoment: (input: SplitMomentInput): Promise<MomentHandleDto> =>
    invoke<MomentHandleDto>('split_moment', { input }),

  mergeMoments: (input: MergeMomentsInput): Promise<MomentHandleDto> =>
    invoke<MomentHandleDto>('merge_moments', { input }),

  /** Pin a grouping against re-analysis, or release it. */
  lockMoment: (input: LockMomentInput): Promise<MomentHandleDto> =>
    invoke<MomentHandleDto>('lock_moment', { input }),

  /**
   * "Keep this one." Moves where phase 12 starts from; culls nothing, and cannot -
   * every other frame in the set stays exactly as eligible as it was.
   */
  setKeepHint: (input: SetKeepHintInput): Promise<MomentHandleDto> =>
    invoke<MomentHandleDto>('set_keep_hint', { input }),

  undoMomentEdit: (projectId: string): Promise<MomentEditDto> =>
    invoke<MomentEditDto>('undo_moment_edit', { projectId }),

  onMomentEvent: (handler: (event: MomentEvent) => void): Promise<() => void> =>
    listen<MomentEvent>('moments', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),

  // -------------------------------------------------------------------------
  // PHASE-09. Six commands, and exactly one of them changes anything.
  // -------------------------------------------------------------------------

  /** What the Integrity panel's header shows, including what was not checked. */
  integrityStatus: (projectId: string): Promise<IntegrityStatusDto> =>
    invoke<IntegrityStatusDto>('integrity_status', { projectId }),

  /**
   * One photograph's verdict.
   *
   * `null` means **nobody has looked** - which is not the same as "nothing is
   * wrong", and the card must not draw it that way.
   */
  imageIntegrity: (photoId: string): Promise<IntegrityDto | null> =>
    invoke<IntegrityDto | null>('image_integrity', { photoId }),

  /** The frames carrying any of these marks, worst technical score first. */
  flaggedImages: (input: FlaggedInput): Promise<string[]> =>
    invoke<string[]>('flagged_images', { input }),

  /**
   * One moment's frames ranked by subject sharpness.
   *
   * Evidence phase 12 asked for by name. It says which of six frames is
   * sharpest and nothing about which of them a client sees.
   */
  withinMoment: (input: WithinMomentInput): Promise<RankedFrameDto[]> =>
    invoke<RankedFrameDto[]>('within_moment', { input }),

  /**
   * "This mark is wrong." Clears one flag and records that the photographer
   * said so; a re-analysis re-applies the dismissal rather than reverting it.
   */
  dismissFlag: (input: DismissFlagInput): Promise<IntegrityDto> =>
    invoke<IntegrityDto>('dismiss_flag', { input }),

  /** Check every frame that has no current verdict. Resumable and cancellable. */
  analyseIntegrity: (input: AnalyseIntegrityInput): Promise<IntegrityPassDto> =>
    invoke<IntegrityPassDto>('analyse_integrity', { input }),

  onIntegrityEvent: (handler: (event: IntegrityEvent) => void): Promise<() => void> =>
    listen<IntegrityEvent>('integrity', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),
};

/**
 * PHASE-10. What a photograph is worth.
 *
 * Seven commands. Five read; `preferFrame` and `setMomentPeak` are the
 * photographer telling the product it is wrong. **None of them keeps, delivers
 * or builds a gallery** - section 2.2 puts the choosing in phase 12.
 */
export const emotion = {
  /** The panel header: coverage, face-awareness, peaks, links and versions. */
  emotionStatus: (projectId: string): Promise<EmotionStatusDto> =>
    invoke<EmotionStatusDto>('emotion_status', { projectId }),

  /**
   * One photograph's reading.
   *
   * `null` means **nobody has looked** - which is not the same as "nothing
   * happened here", and the card must not draw it that way.
   */
  imageEmotion: (photoId: string): Promise<EmotionDto | null> =>
    invoke<EmotionDto | null>('image_emotion', { photoId }),

  /**
   * One moment's peak.
   *
   * `null` means the moment has not been scored. A moment that *was* scored and
   * had no apex comes back with `resolved: false`, which is a different and
   * common answer.
   */
  momentPeak: (momentId: string): Promise<MomentPeakDto | null> =>
    invoke<MomentPeakDto | null>('moment_peak', { momentId }),

  /** Every frame that reacts to this one, earliest first. */
  reactionsOf: (photoId: string): Promise<ReactionLinkDto[]> =>
    invoke<ReactionLinkDto[]>('reactions_of', { photoId }),

  /**
   * A project's frames ordered by emotion score, strongest first.
   *
   * **An ordering, not a selection.**
   */
  rankedByEmotion: (input: RankedInput): Promise<RankedByEmotionDto[]> =>
    invoke<RankedByEmotionDto[]>('ranked_by_emotion', { input }),

  /**
   * "I would deliver this one." Recorded for phase 30's learning loop and
   * applied to nothing today: a ranker that refitted itself mid-cull would
   * reorder the grid under the photographer's hands.
   */
  preferFrame: (input: PreferInput): Promise<void> => invoke<void>('prefer_frame', { input }),

  /**
   * "This frame is the one." Unbeatable: a re-analysis re-applies the choice
   * rather than reverting it.
   */
  setMomentPeak: (input: SetPeakInput): Promise<MomentPeakDto> =>
    invoke<MomentPeakDto>('set_moment_peak', { input }),

  /** Score every frame that has no current reading. Resumable and cancellable. */
  scoreEmotion: (input: ScoreEmotionInput): Promise<EmotionPassDto> =>
    invoke<EmotionPassDto>('score_emotion', { input }),

  onEmotionEvent: (handler: (event: EmotionEvent) => void): Promise<() => void> =>
    listen<EmotionEvent>('emotion', (message) => handler(message.payload)).then(
      (unlisten) => () => {
        unlisten();
      },
    ),
};

/**
 * PHASE-11. How a photograph is framed.
 *
 * Five commands: three reads, one dismissal and the resumable analysis pass.
 * None of them applies the crop hint, straightens pixels, or makes a delivery
 * decision.
 */
export const composition = {
  /** Coverage, keypoint awareness, flags, tilt telemetry and stored versions. */
  compositionStatus: (projectId: string): Promise<CompositionStatusDto> =>
    invoke<CompositionStatusDto>('composition_status', { projectId }),

  /** One frame's complete judgement. Null means nobody has analysed it. */
  imageComposition: (photoId: string): Promise<CompositionDto | null> =>
    invoke<CompositionDto | null>('image_composition', { photoId }),

  /** A review queue ordered from the lowest composition score upward. */
  flaggedComposition: (input: FlaggedCompositionInput): Promise<string[]> =>
    invoke<string[]>('flagged_composition', { input }),

  /** Clear one violation and remember the photographer's disagreement. */
  dismissCompositionFlag: (input: DismissCompositionFlagInput): Promise<CompositionDto> =>
    invoke<CompositionDto>('dismiss_composition_flag', { input }),

  /** Judge pending frames. Completed rows survive cancellation and the next run resumes. */
  analyseComposition: (input: AnalyseCompositionInput): Promise<CompositionPassDto> =>
    invoke<CompositionPassDto>('analyse_composition', { input }),
};

/**
 * PHASE-12. What is being delivered.
 *
 * Seven commands: three reads, three that change the decision, and one that
 * runs the cull. None of them deletes, moves, exports or uploads a photograph -
 * phase 14 edits the survivors, phase 27 swaps in runner-ups, phase 29 builds
 * albums and phase 30 delivers.
 *
 * `gallery` returns `null` when the wedding has never been culled. That is
 * "nobody has decided", not "deliver nothing", and no caller may render the two
 * the same way.
 */
export const cull = {
  /** Coverage, guarantee counts, overrides, mode and stored versions. */
  cullStatus: (projectId: string): Promise<CullStatusDto> =>
    invoke<CullStatusDto>('cull_status', { projectId }),

  /** The stored gallery. Null means nobody has culled this wedding yet. */
  gallery: (projectId: string): Promise<SelectionDto | null> =>
    invoke<SelectionDto | null>('gallery', { projectId }),

  /** What was decided about one photograph, in either direction, with reasons. */
  imageDecision: (photoId: string): Promise<DecisionDto | null> =>
    invoke<DecisionDto | null>('image_decision', { photoId }),

  /** Run or re-run the cull. Reads stored analysis; opens no image file. */
  cullProject: (input: CullProjectInput): Promise<CullPassDto> =>
    invoke<CullPassDto>('cull_project', { input }),

  /**
   * Move the size slider. The result may exceed the requested target, because
   * the coverage guard runs last and a guarantee outranks a slider.
   */
  resizeGallery: (input: ResizeGalleryInput): Promise<SelectionDto> =>
    invoke<SelectionDto>('resize_gallery', { input }),

  /** Switch autonomy mode. Cannot drop a must-have, whichever mode is chosen. */
  setCullMode: (input: SetCullModeInput): Promise<SelectionDto> =>
    invoke<SelectionDto>('set_cull_mode', { input }),

  /**
   * Keep or remove one photograph by hand, or withdraw an earlier choice.
   *
   * Unbeatable and re-applied onto every fresh selection. A removal can leave a
   * guarantee short: the coverage report then degrades that rule and says so.
   */
  overrideDecision: (input: OverrideDecisionInput): Promise<DecisionDto> =>
    invoke<DecisionDto>('override_decision', { input }),
};

/**
 * PHASE-13. The record of what AURA did, and why.
 *
 * Eight commands. Six read, `recordDecisions` writes the stored gallery's
 * decisions into the ledger, and `exportSupportBundle` produces an anonymised
 * file a photographer can send.
 *
 * **Nothing here changes a decision.** A photographer who disagrees with the
 * panel changes the decision itself through `cull.overrideDecision`, and the
 * ledger then records a new decision that supersedes the old one. The ledger is
 * append-only: there is no update and no delete on this surface.
 */
export const explain = {
  /** Everything the Explain panel draws for one photograph. */
  explainImage: (photoId: string): Promise<ExplainPanelDto> =>
    invoke<ExplainPanelDto>('explain_image', { photoId }),

  /** Every decision ever recorded about one photograph, newest first. */
  decisionHistory: (photoId: string): Promise<LedgerDecisionDto[]> =>
    invoke<LedgerDecisionDto[]>('decision_history', { photoId }),

  /** One decision by its id - what a support case quotes down a telephone. */
  decisionById: (decisionId: string): Promise<LedgerDecisionDto | null> =>
    invoke<LedgerDecisionDto | null>('decision_by_id', { decisionId }),

  /** Counts, coverage, calibration version and size for one wedding's ledger. */
  ledgerStatus: (projectId: string): Promise<LedgerStatusDto> =>
    invoke<LedgerStatusDto>('ledger_status', { projectId }),

  /** The decisions waiting for a person, newest first. */
  reviewQueue: (input: ReviewQueueInput): Promise<LedgerDecisionDto[]> =>
    invoke<LedgerDecisionDto[]>('review_queue', { input }),

  /**
   * Record the stored gallery's decisions. Append-only, so running it twice
   * records two rounds of decisions rather than overwriting the first.
   */
  recordDecisions: (input: RecordDecisionsInput): Promise<RecordDecisionsDto> =>
    invoke<RecordDecisionsDto>('record_decisions', { input }),

  /**
   * An anonymised slice of the ledger. No pixels, no names, no keys - every
   * identifier is replaced by a handle before the file exists.
   */
  exportSupportBundle: (input: ExportBundleInput): Promise<SupportBundleDto> =>
    invoke<SupportBundleDto>('export_support_bundle', { input }),

  /** Apply the compaction policy. Keeps the newest decision per subject and
   * every decision the photographer made themselves. */
  compactLedger: (projectId: string): Promise<number> =>
    invoke<number>('compact_ledger', { projectId }),
};

/**
 * PHASE-14. The develop surface.
 *
 * Nine calls. None of them names a destination and none can overwrite a parameter a person
 * set: `setParam` sends a person's own change, and every automated pass goes through the
 * same merge in Rust with an automated source and is refused there.
 */
export const develop = {
  /** One photograph's edit, or the camera's own starting point when it has none. */
  imageRecipe: (input: DevelopImageInput): Promise<RecipeDto> =>
    invoke<RecipeDto>('image_recipe', { input }),

  /** Change one parameter, as a person. Marks the path protected from then on. */
  setParam: (input: SetParamInput): Promise<SetParamDto> =>
    invoke<SetParamDto>('set_param', { input }),

  /** Undo, redo, or one of the two resets. */
  historyStep: (input: HistoryStepInput): Promise<SetParamDto> =>
    invoke<SetParamDto>('history_step', { input }),

  /** One photograph's history, its snapshots, and what is available. */
  imageHistory: (input: DevelopImageInput): Promise<HistoryDto> =>
    invoke<HistoryDto>('image_history', { input }),

  /** Take or restore a named snapshot. */
  snapshot: (input: SnapshotInput): Promise<HistoryDto> =>
    invoke<HistoryDto>('snapshot', { input }),

  /** Render a proxy. The pixels come back inline; there is no file. */
  renderImage: (input: RenderImageInput): Promise<RenderDto> =>
    invoke<RenderDto>('render_image', { input }),

  /** What this machine's renderer can do, and what it is running without. */
  renderCaps: (): Promise<RenderCapsDto> => invoke<RenderCapsDto>('render_caps', {}),

  /** How much of a wedding has an edit. The denominator is every photograph. */
  developStatus: (projectId: string): Promise<DevelopStatusDto> =>
    invoke<DevelopStatusDto>('develop_status', { projectId }),
};

/**
 * PHASE-15. The exposure and white-balance surface.
 *
 * Seven calls. Four read, one runs the pass, and two record what the photographer decided.
 * None of them returns a skin locus: what a named person's skin looks like stays behind the
 * service, and the panel gets counts instead (ADR-0032 section 4).
 */
export const tone = {
  /** How much of a wedding has an exposure decision, and how much of it came from a face. */
  toneStatus: (projectId: string): Promise<ToneStatusDto> =>
    invoke<ToneStatusDto>('tone_status', { projectId }),

  /** One photograph's decision, or `null` when nobody has estimated it. */
  imageTone: (photoId: string): Promise<ToneDto | null> =>
    invoke<ToneDto | null>('image_tone', { photoId }),

  /** The frames whose white balance is worth a look, weakest first. A queue, not a cull. */
  toneReviewQueue: (input: ToneReviewInput): Promise<string[]> =>
    invoke<string[]>('tone_review_queue', { input }),

  /** One chapter's anchors, best first. What phase 25 will normalise toward. */
  referenceFrames: (input: ReferenceFramesInput): Promise<ReferenceFrameDto[]> =>
    invoke<ReferenceFrameDto[]>('reference_frames', { input }),

  /** Record that the photographer looked and agrees. Does not set `userEdited`. */
  acceptTone: (input: AcceptToneInput): Promise<ToneDto> =>
    invoke<ToneDto>('accept_tone', { input }),

  /** Record what they set instead, and write it into the recipe as a person. */
  setToneOverride: (input: SetToneOverrideInput): Promise<SetToneOverrideDto> =>
    invoke<SetToneOverrideDto>('set_tone_override', { input }),

  /** Run the resumable pass, then carry what it decided into the recipes. */
  estimateTone: (input: EstimateToneInput): Promise<TonePassDto> =>
    invoke<TonePassDto>('estimate_tone', { input }),
};

/**
 * PHASE-16. Tone curves, HSL and skin protection.
 *
 * Seven commands. Three read, one runs the pass, three record what the photographer decided.
 *
 * Nothing here can reach a white balance: phase 15 owns the temperature and the tint, and the
 * "warmer" alternative is a shift of the warm hue bands rather than a change of light.
 */
export const colour = {
  /** How much of a wedding has a grade, and how much of the skin guarantee was checked. */
  colourStatus: (projectId: string): Promise<ColourStatusDto> =>
    invoke<ColourStatusDto>('colour_status', { projectId }),

  /** One photograph's grade, or `null` when nobody has graded it. */
  imageColour: (photoId: string): Promise<ColourDto | null> =>
    invoke<ColourDto | null>('image_colour', { photoId }),

  /** The frames whose grade is worth a look, weakest first. A queue, not a cull. */
  colourReviewQueue: (input: ColourReviewInput): Promise<string[]> =>
    invoke<string[]>('colour_review_queue', { input }),

  /** Record that the photographer looked and agrees. Does not set `userEdited`. */
  acceptColour: (input: AcceptColourInput): Promise<ColourDto> =>
    invoke<ColourDto>('accept_colour', { input }),

  /** Record what they set instead, and write it into the recipe as a person. */
  setColourOverride: (input: SetColourOverrideInput): Promise<SetColourOverrideDto> =>
    invoke<SetColourOverrideDto>('set_colour_override', { input }),

  /**
   * Promote one stored alternative to the primary grade.
   *
   * Safe because every variant has already been through the clipping guard and the skin
   * guard, so the promoted set is one somebody checked.
   */
  selectColourVariant: (input: SelectVariantInput): Promise<SetColourOverrideDto> =>
    invoke<SetColourOverrideDto>('select_colour_variant', { input }),

  /** Run the resumable pass, then carry what it decided into the recipes. */
  estimateColour: (input: EstimateColourInput): Promise<ColourPassDto> =>
    invoke<ColourPassDto>('estimate_colour', { input }),
};

/**
 * PHASE-17. Style learning: scene-conditional personal AI profiles.
 *
 * Eleven commands. Four read, two look at an archive, two move a profile through its
 * lifecycle, two carry it between machines, and one chooses which profile a project uses.
 *
 * **Nothing here returns imagery.** Paths go in and names, numbers and verdicts come out,
 * which is what makes "AURA never uploads your archive" a property of the shapes rather than a
 * promise about the code.
 */
export const style = {

  /** What this project knows about style. */
  styleStatus: (projectId: string): Promise<StyleStatusDto> =>
    invoke<StyleStatusDto>('style_status', { projectId }),

  /** Every profile, newest first. */
  listProfiles: (): Promise<StyleProfileDto[]> =>
    invoke<StyleProfileDto[]>('list_profiles', {}),

  /** One profile's honest report, or `null` when nobody has trained it. */
  profileReport: (profileId: string): Promise<ProfileReportDto | null> =>
    invoke<ProfileReportDto | null>('profile_report', { profileId }),

  /** The pairs behind one profile - accepted **and** rejected. */
  profilePairs: (name: string, limit?: number): Promise<StylePairDto[]> =>
    invoke<StylePairDto[]>('profile_pairs', { name, limit: limit ?? null }),

  /** Look at what is in a folder, before anything is fitted. Opens nothing. */
  scanArchive: (input: ScanArchiveInput): Promise<ScanArchiveDto> =>
    invoke<ScanArchiveDto>('scan_archive', { input }),

  /** Train a profile. The result is a **candidate**; adoption is a separate act. */
  trainProfile: (input: TrainProfileInput): Promise<TrainProfileDto> =>
    invoke<TrainProfileDto>('train_profile', { input }),

  /** Adopt one profile: it becomes what the product edits with. */
  adoptProfile: (input: AdoptProfileInput): Promise<StyleProfileDto> =>
    invoke<StyleProfileDto>('adopt_profile', { input }),

  /** The side-by-side of the baseline, the adopted profile and a candidate. No pixels. */
  compareProfiles: (input: CompareProfilesInput): Promise<StyleComparisonDto[]> =>
    invoke<StyleComparisonDto[]>('compare_profiles', { input }),

  /** Write a signed, portable profile. */
  exportProfile: (input: ExportProfileInput): Promise<ExportProfileDto> =>
    invoke<ExportProfileDto>('export_profile', { input }),

  /** Read a signed profile bundle. A tampered one is refused with `AURA-ML-5076`. */
  importProfile: (input: ImportProfileInput): Promise<ImportProfileDto> =>
    invoke<ImportProfileDto>('import_profile', { input }),

  /** Choose which profile a project, or one chapter of it, uses. */
  setProjectProfile: (input: SetProjectProfileInput): Promise<StyleStatusDto> =>
    invoke<StyleStatusDto>('set_project_profile', { input }),
};


// ---------------------------------------------------------------------------
// PHASE-18. Local mask AI.
//
// Eight commands and no ninth. There is no `applyMask` here: section 2.2 of the phase document
// puts every *use* of a mask in phases 19 to 24, and what this surface hands out is a region and
// a strength ceiling.
// ---------------------------------------------------------------------------

export const maskApi = {
  /** What the mask panel's project header shows. Two numbers, never a ratio. */
  maskStatus: (projectId: string): Promise<MaskStatusDto> =>
    invoke<MaskStatusDto>('mask_status', { projectId }),

  /**
   * Every region stored for one photograph, in the frozen class order.
   *
   * An empty list means nobody has masked this frame yet. It is not the same as a frame with no
   * regions in it, and the panel renders the two differently.
   */
  imageMasks: (imageId: string): Promise<MaskDto[]> =>
    invoke<MaskDto[]>('image_masks', { imageId }),

  /** Produce the named regions if they are not already stored. Idempotent. */
  ensureMasks: (input: EnsureMasksInput): Promise<MaskDto[]> =>
    invoke<MaskDto[]>('ensure_masks', { input }),

  /** One region as a plane to draw over a preview. Quarter resolution, capped. */
  maskOverlay: (maskId: string): Promise<MaskOverlayDto> =>
    invoke<MaskOverlayDto>('mask_overlay', { maskId }),

  /** What one operation may do through one region, and why it may not do more. */
  maskAllowance: (maskId: string, operation: string): Promise<MaskAllowanceDto> =>
    invoke<MaskAllowanceDto>('mask_allowance', { maskId, operation }),

  /** Apply a composition and keep the result as the photographer's own. Sets `userEdited`. */
  editMask: (input: EditMaskInput): Promise<MaskDto> =>
    invoke<MaskDto>('edit_mask', { input }),

  /** Drop a photographer's edit so the next pass regenerates the region. */
  regenerateMask: (maskId: string): Promise<boolean> =>
    invoke<boolean>('regenerate_mask', { maskId }),

  /** The twenty class slugs, in the frozen iteration order. */
  maskKinds: (): Promise<string[]> => invoke<string[]>('mask_kinds', {}),
};

/**
 * The previews a project could not build, with the reason each one was quarantined.
 *
 * Phase 02's surface, and the last command in the product to get a typed wrapper: the shell
 * has answered to it since phase 02 and nothing had ever called it. A preview that failed is
 * not an image that is missing - it is an image the grid must render as a problem rather than
 * as an empty cell, which is invariant 9 at the level a photographer sees.
 */
export const previewProblems = {
  previewProblems: (projectId: string): Promise<Array<[string, string]>> =>
    invoke<Array<[string, string]>>('preview_problems', { projectId }),
};

/**
 * PHASE-19. Local light sculpting.
 *
 * Six calls. Three read, one runs the pass, and two record what the photographer decided.
 * None of them returns a mask: phase 18 owns masks, phase 19 reads them, and nothing on this
 * surface can return an alpha, a matte or a grid (ADR-0040 section 4). What the panel gets
 * instead is the reasons' own evidence rectangles and the shaping zones by name.
 */
export const local = {
  /** How much of a wedding has a local light plan, and how much of it actually did anything. */
  localStatus: (projectId: string): Promise<LocalStatusDto> =>
    invoke<LocalStatusDto>('local_status', { projectId }),

  /** One photograph's plan, or `null` when nobody has planned it. */
  imageLocal: (photoId: string): Promise<LocalPlanDto | null> =>
    invoke<LocalPlanDto | null>('image_local', { photoId }),

  /** The frames whose local work is worth a look, weakest first. A queue, not a cull. */
  localReviewQueue: (input: LocalReviewInput): Promise<string[]> =>
    invoke<string[]>('local_review_queue', { input }),

  /** Record that the photographer looked and agrees. Does not set `userEdited`. */
  acceptLocal: (input: AcceptLocalInput): Promise<LocalPlanDto> =>
    invoke<LocalPlanDto>('accept_local', { input }),

  /** Record one operation's own strength, and write it into the recipe as a person. */
  setLocalStrength: (input: SetLocalStrengthInput): Promise<SetLocalStrengthDto> =>
    invoke<SetLocalStrengthDto>('set_local_strength', { input }),

  /**
   * Run the resumable pass over the selected photographs, then carry what it decided into
   * the recipes.
   *
   * `photoIds` is the normal path: invariant 3, and section 11's own budget is written about
   * a thousand selected images rather than about a wedding.
   */
  sculptLocal: (input: SculptLocalInput): Promise<LocalPassDto> =>
    invoke<LocalPassDto>('sculpt_local', { input }),
};


/**
 * PHASE-20. Portrait retouch.
 *
 * Eight calls. Three read, one runs the pass, and four record what the photographer decided.
 *
 * **There is no strength argument anywhere on this surface.** Retouch strength is one stored
 * number per identity per project, computed from gallery statistics, and the frame decides
 * which operations run rather than how hard they run - so a per-frame strength is not a field
 * a caller forgot, it is a field the contract refuses to have (ADR-0043).
 */
export const retouch = {
  /** How much of a wedding has a retouch plan, and how much of it actually did anything. */
  retouchStatus: (projectId: string): Promise<RetouchStatusDto> =>
    invoke<RetouchStatusDto>('retouch_status', { projectId }),

  /** One photograph's plan, or `null` when nobody has planned it. */
  imageRetouch: (photoId: string): Promise<RetouchPlanDto | null> =>
    invoke<RetouchPlanDto | null>('image_retouch', { photoId }),

  /**
   * What this product will never remove from one person's face.
   *
   * The rectangles are **face-normalised** - origin between the eyes, x along the eye-to-eye
   * line, unit the inter-ocular distance - which is what lets one row protect the same mole in
   * four hundred photographs. Drawing one on a frame projects it through that frame's landmarks.
   */
  protectedFeatures: (projectId: string, identityId: string): Promise<ProtectedFeatureDto[]> =>
    invoke<ProtectedFeatureDto[]>('protected_features', { projectId, identityId }),

  /** The frames whose retouch is worth a look, weakest first. A queue, not a cull. */
  retouchReviewQueue: (input: RetouchReviewInput): Promise<string[]> =>
    invoke<string[]>('retouch_review_queue', { input }),

  /** Record that the photographer looked and agrees. Does not set `userEdited`. */
  acceptRetouch: (input: AcceptRetouchInput): Promise<RetouchPlanDto> =>
    invoke<RetouchPlanDto>('accept_retouch', { input }),

  /** Record the preset the photographer chose, and write it into the recipe as a person. */
  setRetouch: (input: SetRetouchInput): Promise<SetRetouchDto> =>
    invoke<SetRetouchDto>('set_retouch', { input }),

  /**
   * Protect or unprotect one feature.
   *
   * A tattoo cannot be unprotected. The refusal is `AURA-ML-5097` and it lives in the type, in
   * the service and in a database trigger, because a promise enforced in one layer lasts until
   * somebody writes a second caller.
   */
  setProtection: (input: SetProtectionInput): Promise<ProtectedFeatureDto[]> =>
    invoke<ProtectedFeatureDto[]>('set_protection', { input }),

  /** Run the resumable retouch pass, then carry what it decided into the recipes. */
  retouchPass: (input: RetouchPassInput): Promise<RetouchPassDto> =>
    invoke<RetouchPassDto>('retouch_pass', { input }),
};

/**
 * PHASE-21. The micro-retouch suite: hair, teeth, eyes, clothing and glare.
 *
 * Nine calls. Four read, one runs the pass, three record a decision, and one assembles the
 * panel's legend from the frozen enum.
 *
 * **`microComposites` is the disclosure surface.** A glare repair may borrow pixels from a
 * sibling frame, and that is the only place in this product where two photographs are
 * composited. Every borrow is listed here with its source frame, because a composite a
 * photographer cannot find is a composite they cannot disclose (ADR-0045).
 */
export const micro = {
  /** How much of a wedding has a micro-retouch plan, and how much of it did anything. */
  microStatus: (projectId: string): Promise<MicroStatusDto> =>
    invoke<MicroStatusDto>('micro_status', { projectId }),

  /** One photograph's plan, or `null` when nobody has planned it. */
  imageMicro: (photoId: string): Promise<MicroPlanDto | null> =>
    invoke<MicroPlanDto | null>('image_micro', { photoId }),

  /** Every frame in the project that carries pixels borrowed from another frame. */
  microComposites: (projectId: string): Promise<MicroCompositeDto[]> =>
    invoke<MicroCompositeDto[]>('micro_composites', { projectId }),

  /** The frames whose small fixes are worth a look, weakest first. A queue, not a cull. */
  microReviewQueue: (input: MicroReviewInput): Promise<string[]> =>
    invoke<string[]>('micro_review_queue', { input }),

  /** Which of the five operations this studio has opted into, and their ceilings. */
  microMatrix: (projectId: string): Promise<MicroMatrixDto> =>
    invoke<MicroMatrixDto>('micro_matrix', { projectId }),

  /**
   * Change the opt-in matrix.
   *
   * A studio may switch an operation off and may lower a ceiling. Nothing here can raise one:
   * the contract owns every bound and the config file may only tighten it, which is what makes
   * `docs/retouch-ethics.md` a promise about the product rather than about its defaults.
   */
  setMicroMatrix: (input: SetMicroMatrixInput): Promise<MicroMatrixDto> =>
    invoke<MicroMatrixDto>('set_micro_matrix', { input }),

  /** Record that the photographer looked and agrees. Does not set `userEdited`. */
  acceptMicro: (input: AcceptMicroInput): Promise<void> =>
    invoke<void>('accept_micro', { input }),

  /** Run the resumable pass, then carry what it decided into the recipes. */
  microPass: (input: MicroPassInput): Promise<MicroPassDto> =>
    invoke<MicroPassDto>('micro_pass', { input }),

  /** The panel's legend, assembled from the frozen enum rather than from a stored table. */
  microReasonCodes: (): Promise<MicroReasonDto[]> =>
    invoke<MicroReasonDto[]>('micro_reason_codes', {}),
};

/**
 * PHASE-22. Restoration: denoise, selective sharpening and face recovery.
 *
 * Eight calls. Four read, one runs the pass, two record a decision, and one assembles the
 * legend.
 *
 * **`restoreIdentityRefusals` is the guarantee made visible.** Face recovery is held to a
 * cosine-distance ceiling measured through the real renderer, and a face that would have moved
 * past it is skipped rather than recovered. Those skips are rows, not silence, because here the
 * refusal is the product working (ADR-0047).
 */
export const restore = {
  /** How much of a wedding has a restoration plan, and how much of it did anything. */
  restoreStatus: (projectId: string): Promise<RestoreStatusDto> =>
    invoke<RestoreStatusDto>('restore_status', { projectId }),

  /** One photograph's plan, or `null` when nobody has planned it. */
  imageRestore: (photoId: string): Promise<RestorePlanDto | null> =>
    invoke<RestorePlanDto | null>('image_restore', { photoId }),

  /** Every face this product declined to recover, and how far it would have moved. */
  restoreIdentityRefusals: (input: RestoreReviewInput): Promise<RestoreIdentityRefusalDto[]> =>
    invoke<RestoreIdentityRefusalDto[]>('restore_identity_refusals', { input }),

  /** The frames whose repairs are worth a look, weakest first. A queue, not a cull. */
  restoreReviewQueue: (input: RestoreReviewInput): Promise<string[]> =>
    invoke<string[]>('restore_review_queue', { input }),

  /** Record that the photographer looked and agrees. Does not set `userEdited`. */
  acceptRestore: (input: AcceptRestoreInput): Promise<void> =>
    invoke<void>('accept_restore', { input }),

  /** Record the photographer's own tier or amount, and write it into the recipe as a person. */
  setRestoreOverride: (input: SetRestoreOverrideInput): Promise<RestorePlanDto> =>
    invoke<RestorePlanDto>('set_restore_override', { input }),

  /** Run the resumable pass, then carry what it decided into the recipes. */
  restorePass: (input: RestorePassInput): Promise<RestorePassDto> =>
    invoke<RestorePassDto>('restore_pass', { input }),

  /** The panel's legend, assembled from the frozen enum rather than from a stored table. */
  restoreReasonCodes: (): Promise<RestoreReasonDto[]> =>
    invoke<RestoreReasonDto[]>('restore_reason_codes', {}),
};

/**
 * PHASE-23. Geometry: lens corrections, straightening and the crop.
 *
 * Six calls. Three read, one runs the pass, and two record what the photographer decided.
 *
 * **There is no revert call, and that is deliberate.** `GeometryPlanDto.crops[0]` is always the
 * original framing - the contract's constructor puts it there and nothing can take it away - so
 * "the original framing is one click away" is a property of the shape rather than a button
 * somebody has to remember to wire. Reverting is `setFraming` with the whole frame at zero
 * degrees (ADR-0041).
 */
export const geometry = {
  /** How much of a wedding is planned, and what share of it was delivered exactly as shot. */
  geometryStatus: (projectId: string): Promise<GeometryStatusDto> =>
    invoke<GeometryStatusDto>('geometry_status', { projectId }),

  /**
   * One photograph's plan, or `null` when nobody has planned it.
   *
   * The aspect variants an album or a feed needs are `crops`, on this plan. They are rectangles
   * in one recipe rather than files on a disk, which is what "without duplicating files" means.
   */
  imageGeometry: (photoId: string): Promise<GeometryPlanDto | null> =>
    invoke<GeometryPlanDto | null>('image_geometry', { photoId }),

  /** The frames whose framing is worth a look, weakest first. A queue, not a cull. */
  geometryReviewQueue: (input: GeometryReviewInput): Promise<string[]> =>
    invoke<string[]>('geometry_review_queue', { input }),

  /** Record that the photographer looked and agrees. Does not set `userEdited`. */
  acceptGeometry: (input: AcceptGeometryInput): Promise<GeometryPlanDto> =>
    invoke<GeometryPlanDto>('accept_geometry', { input }),

  /**
   * Record the framing the photographer chose, and write it into the recipe as a person.
   *
   * A person may crop one photograph of their own through a face, because it is their
   * photograph and they are looking at it. There is no field anywhere on this surface that says
   * cutting faces is acceptable *in general* - that is the setting which would crop the next
   * four hundred frames through people.
   */
  setFraming: (input: SetFramingInput): Promise<SetFramingDto> =>
    invoke<SetFramingDto>('set_framing', { input }),

  /** Run the resumable pass, then carry what it decided into the recipes. */
  planGeometry: (input: PlanGeometryInput): Promise<GeometryPassDto> =>
    invoke<GeometryPassDto>('plan_geometry', { input }),
};

/**
 * PHASE-24. Distraction cleanup: the exit sign, the gaffer tape and the caterer's crate.
 *
 * Nine calls. Four read, one runs the pass, three record a decision, and one is the manual tool.
 *
 * **Read `cleanupStatus().maskCovered` before anything else.** Phase 18's mask vocabulary has no
 * class for a ring or a cake, so it is zero on every build so far - which means every candidate
 * was refused because AURA could not show the region was clear of people, not because it looked
 * and found somebody. The two are different rows, different reason codes and different runbooks,
 * and a panel that rendered them the same way would let a build with no segmenter look like one
 * that examined every photograph and found them all clear.
 *
 * **Nothing here can apply a removal unattended.** Section 6.4 permits a borrow or a fill at
 * calibrated confidence 0.97 in Zero-Touch; nothing in this build is calibrated, so phase 13's
 * `uncalibrated_raises` moves every band one further toward review and `mayApplyUnattended` is
 * false on every proposal.
 */
export const cleanup = {
  /** How much of a wedding has been examined, and what came of it. */
  cleanupStatus: (projectId: string): Promise<CleanupStatusDto> =>
    invoke<CleanupStatusDto>('cleanup_status', { projectId }),

  /** One photograph's proposals, strongest first. */
  imageCleanup: (photoId: string): Promise<CleanupProposalDto[]> =>
    invoke<CleanupProposalDto[]>('image_cleanup', { photoId }),

  /**
   * Every candidate the safety engine refused on one photograph.
   *
   * A separate call because the refused set is usually larger than the proposed one. It is on the
   * surface at all because teaching a photographer what AURA will never do is most of the trust
   * this feature needs.
   */
  cleanupBlocked: (photoId: string): Promise<CleanupBlockedDto[]> =>
    invoke<CleanupBlockedDto[]>('cleanup_blocked', { photoId }),

  /** Everything removed from this project, for the delivery report. */
  cleanupDisclosures: (projectId: string): Promise<CleanupDisclosureDto[]> =>
    invoke<CleanupDisclosureDto[]>('cleanup_disclosures', { projectId }),

  /** Run the resumable pass. Killing it costs nothing; the work remaining is a query. */
  cleanupPass: (input: CleanupPassInput): Promise<CleanupPassDto> =>
    invoke<CleanupPassDto>('cleanup_pass', { input }),

  /** Accept or reject one proposal. Accepting does not apply it. */
  decideCleanup: (input: DecideCleanupInput): Promise<void> =>
    invoke<void>('decide_cleanup', { input }),

  /** Leave one photograph alone entirely. Excluded from every later pass. */
  disableCleanup: (input: DisableCleanupInput): Promise<void> =>
    invoke<void>('disable_cleanup', { input }),

  /** Remove one region the photographer drew. Still runs all five safety checks. */
  manualRemove: (input: ManualRemoveInput): Promise<ManualRemoveDto> =>
    invoke<ManualRemoveDto>('manual_remove', { input }),

  /** The panel's legend, assembled from the frozen enum rather than from a stored table. */
  cleanupReasonCodes: (): Promise<CleanupReasonDto[]> =>
    invoke<CleanupReasonDto[]>('cleanup_reason_codes', {})
};

/**
 * PHASE-25. The gallery consistency surface: nine commands whose subject is a wedding rather than a
 * photograph.
 *
 * **Read `galleryStatus().anchoredNodes` beside `nodes`, always.** A project at 100 % coverage with
 * 20 % anchored has had almost nothing done to it: an unanchored node produces a zero delta for
 * every frame in it, and a zero delta is still a row. A panel that led with coverage alone would
 * render a wedding nobody could judge as a wedding that needed no work.
 *
 * And **never render a skin claim while `skinFieldAvailable` is false.** Phase 18's segmenter is a
 * placeholder, so nothing about anybody's skin was measured on this build.
 */
export const gallery = {
  /** What the Consistency panel's project header shows. Two denominators; read both. */
  galleryStatus: (projectId: string): Promise<GalleryStatusDto> =>
    invoke<GalleryStatusDto>('gallery_status', { projectId }),

  /** The node tree, in capture order of each node's first frame. */
  galleryNodes: (projectId: string): Promise<SceneNodeDto[]> =>
    invoke<SceneNodeDto[]>('gallery_nodes', { projectId }),

  /**
   * One node's deltas, in capture order. What a timeline strip draws.
   *
   * A separate call from `galleryNodes` because a wedding has forty nodes and four thousand
   * frames, and a header that pulled every delta would pull the whole gallery to draw a summary.
   */
  galleryNodeStrip: (nodeId: string): Promise<GalleryDeltaDto[]> =>
    invoke<GalleryDeltaDto[]>('gallery_node_strip', { nodeId }),

  /** One photograph's delta. `null` is a gap in coverage, never a zero delta. */
  imageGallery: (photoId: string): Promise<GalleryDeltaDto | null> =>
    invoke<GalleryDeltaDto | null>('image_gallery', { photoId }),

  /** Every frame still out of line, worst first. What phase 27's QC queue reads. */
  galleryOutliers: (projectId: string, limit: number): Promise<GalleryOutlierDto[]> =>
    invoke<GalleryOutlierDto[]>('gallery_outliers', { projectId, limit }),

  /**
   * Run the consistency pass.
   *
   * Runs to completion and returns what it did rather than a job id: a node half-solved against one
   * target and half against another has a target that describes neither, so there is no partial
   * state to poll that would not be a lie about what the catalog holds.
   */
  galleryPass: (input: GalleryPassInput): Promise<GalleryPassDto> =>
    invoke<GalleryPassDto>('gallery_pass', { input }),

  /** Pin or reject one anchor. Both survive every later pass. */
  pinGalleryAnchor: (input: PinAnchorInput): Promise<void> =>
    invoke<void>('pin_gallery_anchor', { input }),

  /**
   * Record what the photographer set instead, on one frame.
   *
   * Records the disagreement; it does not move a pixel. A value outside its bound is refused rather
   * than clamped.
   */
  setGalleryOverride: (input: GalleryOverrideInput): Promise<void> =>
    invoke<void>('set_gallery_override', { input }),

  /** Leave one photograph out of the gallery match entirely. */
  disableGallery: (input: DisableGalleryInput): Promise<void> =>
    invoke<void>('disable_gallery', { input }),

  /** The panel's legend, assembled from the frozen enum rather than from a stored table. */
  galleryReasonCodes: (): Promise<GalleryReasonDto[]> =>
    invoke<GalleryReasonDto[]>('gallery_reason_codes', {}),
};

/**
 * Multi-camera and second-shooter matching. PHASE-26, ADR-0054.
 *
 * Eleven commands whose subject is a **camera body** rather than a photograph or a wedding. Two
 * rules for anything that renders them, and both are about not overstating what was measured.
 *
 * **Read `source` before any number.** A body corrected by 300 K from thirty-four verified pairs of
 * its own ceremony and a body corrected by 300 K from a bundled brand setting are the same
 * arithmetic and completely different claims. `cameraReports()` is the same facts as sentences and
 * it already leads with the evidence; a panel that renders `cameraTransforms()` directly has to do
 * the same ordering itself.
 *
 * **Never render a measurement claim while `baselinesMeasured` is false.** It is false in this
 * build: every file in `assets/camera_baselines/` was chosen to be plausible rather than measured
 * from a photographed colour target. A body whose `source` is `brand_baseline` on this build has
 * been corrected by a fabricated number, and the panel must say so rather than presenting it as a
 * laboratory result.
 *
 * And `heldoutImproved` has **three** states. `null` means there were too few spare pairs to check
 * the correction against, which is not the same as a check that passed and not the same as one that
 * failed.
 */
export const camera = {
  /** The project header: how many cameras, on what evidence, and what is left over. */
  cameraStatus: (projectId: string): Promise<CameraStatusDto> =>
    invoke<CameraStatusDto>('camera_status', { projectId }),

  /** Every body's correction, by body and then by flash state. */
  cameraTransforms: (projectId: string): Promise<CameraTransformDto[]> =>
    invoke<CameraTransformDto[]>('camera_transforms', { projectId }),

  /** Every body's measured colour response, by body and then by flash state. */
  cameraFingerprints: (projectId: string): Promise<CameraFingerprintDto[]> =>
    invoke<CameraFingerprintDto[]>('camera_fingerprints', { projectId }),

  /**
   * The per-camera report, worst evidence first.
   *
   * What section 13's third acceptance criterion asks for: what was corrected, and on what
   * evidence, in sentences a photographer reads rather than numbers they interpret.
   */
  cameraReports: (projectId: string): Promise<CameraReportDto[]> =>
    invoke<CameraReportDto[]>('camera_reports', { projectId }),

  /**
   * The matched pairs behind one body's correction, best first.
   *
   * **Rejected pairs come back too.** "Both cameras shot the whole ceremony and AURA still used a
   * brand baseline" is answered by a list of candidates whose backgrounds disagreed, and by nothing
   * else.
   */
  cameraPairs: (projectId: string, cameraId: string, limit: number): Promise<MatchedPairDto[]> =>
    invoke<MatchedPairDto[]>('camera_pairs', { projectId, cameraId, limit }),

  /** Every measured exposure habit, per photographer and per kind of photograph. */
  cameraShooterBias: (projectId: string): Promise<ShooterBiasDto[]> =>
    invoke<ShooterBiasDto[]>('camera_shooter_bias', { projectId }),

  /**
   * Match a project's cameras to each other.
   *
   * Runs to completion rather than returning a job id: a project half solved against one reference
   * and half against another has been matched to nothing.
   */
  cameraPass: (input: CameraPassInput): Promise<CameraPassDto> =>
    invoke<CameraPassDto>('camera_pass', { input }),

  /** Choose the body everything else is matched to, and re-solve against it. Durable. */
  setCameraReference: (input: SetCameraReferenceInput): Promise<void> =>
    invoke<void>('set_camera_reference', { input }),

  /** Leave one camera out of matching entirely. Both flash states move together. */
  disableCamera: (input: DisableCameraInput): Promise<void> =>
    invoke<void>('disable_camera', { input }),

  /**
   * Record what the photographer set instead, for one body in one flash state.
   *
   * Records the disagreement; it does not move a pixel. A value outside its bound is refused rather
   * than clamped.
   */
  setCameraOverride: (input: CameraOverrideInput): Promise<void> =>
    invoke<void>('set_camera_override', { input }),

  /** The panel's legend, assembled from the frozen enum rather than from a stored table. */
  cameraReasonCodes: (): Promise<CameraReasonDto[]> =>
    invoke<CameraReasonDto[]>('camera_reason_codes', {}),
};

/**
 * PHASE-27. Quality control: nine commands whose subject is a **problem**.
 *
 * Every earlier surface in this file answers "what did AURA decide about this photograph". This
 * one answers "what does AURA think is wrong with what it decided", and the reader arrives
 * sceptical - so `deviation`, `threshold`, `unit` and `severity` travel beside every sentence,
 * never the sentence alone.
 *
 * **Read `completeness` before you believe a clean report.** This is the only surface in the
 * product where an empty result is genuinely ambiguous: zero findings means either that AURA
 * looked at everything and it is fine, or that AURA could not look. In this build the second is
 * the common case - phase 06's detector finds no faces, phase 18's segmenter is untrained, phase
 * 22's face recovery never runs - so most checks skip, and a panel that rendered a skip as a pass
 * would be telling a photographer their gallery had been inspected when it had not.
 *
 * **`qcDecideBulk` records verdicts and never authorises remedies.** Agreeing that forty findings
 * are real is a statement about the findings; instructing AURA to act on forty frames unattended
 * is a statement about the remedies, and the two are different judgements made with different
 * amounts of attention. Per-ticket authorisation lives in `qcDecide`, next to a before and after.
 * ADR-0056 section 5.
 */
export const qc = {
  /** The project header: what was checked, what could not be, and what is outstanding. */
  qcStatus: (projectId: string): Promise<QcStatusDto> =>
    invoke<QcStatusDto>('qc_status', { projectId }),

  /** What the most recent pass did, or `null` when none has run. */
  qcReport: (projectId: string): Promise<QcReportDto | null> =>
    invoke<QcReportDto | null>('qc_report', { projectId }),

  /**
   * The same report as Markdown, for a studio's records.
   *
   * Rendered in Rust rather than here, so the archived report and the queue a photographer reads
   * say the same thing.
   */
  qcReportMarkdown: (projectId: string): Promise<string | null> =>
    invoke<string | null>('qc_report_markdown', { projectId }),

  /**
   * The escalation queue, worst first.
   *
   * Ordered by severity as a **ratio** rather than by raw deviation: 0.4 dE00 over a 0.2 ceiling
   * and 400 K over a 200 K one are the same amount of wrong, and sorting on the raw number would
   * put every colour-temperature finding above every skin finding for ever.
   */
  qcQueue: (projectId: string, category: string | null, limit: number): Promise<QcTicketDto[]> =>
    invoke<QcTicketDto[]>('qc_queue', { projectId, category, limit }),

  /**
   * The same queue, grouped by category, with the groups ordered by their worst member.
   *
   * Which is how a photographer works: eleven soft frames are one decision, not eleven.
   */
  qcQueueGrouped: (projectId: string, limit: number): Promise<QcGroupDto[]> =>
    invoke<QcGroupDto[]>('qc_queue_grouped', { projectId, limit }),

  /** Every finding on one photograph, worst first. */
  qcTickets: (imageId: string): Promise<QcTicketDto[]> =>
    invoke<QcTicketDto[]>('qc_tickets', { imageId }),

  /**
   * What was tried on one finding, and whether it worked.
   *
   * `realisedShare` is the number the loop decided on: a remedy that delivered less than half of
   * what it promised was put back, and the row says so rather than leaving a reader to infer it
   * from two deviations.
   */
  qcRounds: (projectId: string, ticketId: string): Promise<QcRoundDto[]> =>
    invoke<QcRoundDto[]>('qc_rounds', { projectId, ticketId }),

  /**
   * Inspect the delivered gallery, and remediate when asked to.
   *
   * `remediate: false` changes nothing, which is what makes it the thing to run before a delivery.
   * Runs to completion rather than returning a job id: a gallery half inspected under one
   * thresholds table and half under another has been checked against nothing.
   */
  qcRun: (input: QcPassInput): Promise<QcReportDto> =>
    invoke<QcReportDto>('qc_run', { input }),

  /**
   * Record what a photographer decided about one finding.
   *
   * `applyRemedy` overrules a review requirement upward - they have looked. Nothing here overrules
   * it downward.
   */
  qcDecide: (projectId: string, input: QcDecideInput): Promise<void> =>
    invoke<void>('qc_decide', { projectId, input }),

  /**
   * Record the same verdict on many findings, and authorise nothing.
   *
   * Returns how many rows moved. See this module's header for why there is no bulk `applyRemedy`.
   */
  qcDecideBulk: (input: QcDecideBulkInput): Promise<number> =>
    invoke<number>('qc_decide_bulk', { input }),

};

/**
 * PHASE-28. One button: EDIT COMPLETE WEDDING.
 *
 * Nine commands. `autopilotStart` returns as soon as the run is planned and the work continues on
 * a worker thread, because a command that returned when the wedding finished would hold this
 * surface for two hours. `autopilotProgress` is what the panel polls while it runs.
 *
 * There is no command here that runs one stage on its own, and that is a decision rather than an
 * omission: a surface that could run the retouch without the cull could edit four thousand frames
 * nobody is delivering. Every individual pass already has its own command from its own phase, and
 * those are where a photographer re-runs one step. ADR-0058 section 5.
 */
export const autopilot = {
  /** What the Autopilot panel's header shows. */
  autopilotStatus: (projectId: string): Promise<AutopilotStatusDto> =>
    invoke<AutopilotStatusDto>('autopilot_status', { projectId }),

  /**
   * What would happen if the run started now.
   *
   * Eight checks. Four can block - a wedding that will not open, a wedding with no photographs, a
   * disk that cannot hold the output, and a missing model a mandatory stage needs - and every row
   * carries a sentence saying what to do about it.
   */
  autopilotPreflight: (projectId: string): Promise<AutopilotPreflightDto> =>
    invoke<AutopilotPreflightDto>('autopilot_preflight', { projectId }),

  /**
   * Start or continue this wedding's run.
   *
   * Pressing this on a wedding that was stopped continues that run rather than starting a new one:
   * a checkpoint is keyed on the run, so a fresh id would repeat every finished stage.
   */
  autopilotStart: (input: AutopilotStartInput): Promise<AutopilotProgressDto> =>
    invoke<AutopilotProgressDto>('autopilot_start', { input }),

  /** What the run in flight is doing right now, or null when nothing is running. */
  autopilotProgress: (projectId: string): Promise<AutopilotProgressDto | null> =>
    invoke<AutopilotProgressDto | null>('autopilot_progress', { projectId }),

  /**
   * Stop this wedding's run.
   *
   * The token is polled between units, never inside a write, so a stopped run leaves the catalog
   * exactly as consistent as a finished one and picks up where it left off.
   */
  autopilotCancel: (projectId: string): Promise<boolean> =>
    invoke<boolean>('autopilot_cancel', { projectId }),

  /** Every stage of the newest run, with what happened to it. */
  autopilotStages: (projectId: string): Promise<AutopilotStageDto[]> =>
    invoke<AutopilotStageDto[]>('autopilot_stages', { projectId }),

  /** What the newest finished run did. */
  autopilotSummary: (projectId: string): Promise<AutopilotSummaryDto | null> =>
    invoke<AutopilotSummaryDto | null>('autopilot_summary', { projectId }),

  /** Everything the governor did during the newest run. */
  autopilotEvents: (projectId: string): Promise<AutopilotEventDto[]> =>
    invoke<AutopilotEventDto[]>('autopilot_events', { projectId }),

  /** Record what the photographer chose in the checklist. */
  autopilotSetSettings: (input: AutopilotSettingsInput): Promise<void> =>
    invoke<void>('autopilot_set_settings', { input }),
};
