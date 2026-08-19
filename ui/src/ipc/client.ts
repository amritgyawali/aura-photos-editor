import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
  AcceptColourInput,
  ColourDto,
  ColourPassDto,
  ColourReviewInput,
  ColourStatusDto,
  EstimateColourInput,
  SelectVariantInput,
  SetColourOverrideDto,
  SetColourOverrideInput,
  AcceptToneInput,
  EstimateToneInput,
  ReferenceFrameDto,
  ReferenceFramesInput,
  SetToneOverrideDto,
  SetToneOverrideInput,
  ToneDto,
  TonePassDto,
  ToneReviewInput,
  ToneStatusDto,
  DevelopImageInput,
  DevelopStatusDto,
  HistoryDto,
  HistoryStepInput,
  RecipeDto,
  RenderCapsDto,
  RenderDto,
  RenderImageInput,
  SetParamDto,
  SetParamInput,
  SnapshotInput,
  AnalyseCompositionInput,
  ExplainPanelDto,
  ExportBundleInput,
  LedgerDecisionDto,
  LedgerStatusDto,
  RecordDecisionsDto,
  RecordDecisionsInput,
  ReviewQueueInput,
  SupportBundleDto,
  CullPassDto,
  CullProjectInput,
  CullStatusDto,
  DecisionDto,
  OverrideDecisionInput,
  ResizeGalleryInput,
  SelectionDto,
  SetCullModeInput,
  CompositionDto,
  CompositionPassDto,
  CompositionStatusDto,
  DismissCompositionFlagInput,
  FlaggedCompositionInput,
  EmotionDto,
  EmotionEvent,
  EmotionPassDto,
  EmotionStatusDto,
  MomentPeakDto,
  PreferInput,
  RankedByEmotionDto,
  RankedInput,
  ReactionLinkDto,
  ScoreEmotionInput,
  SetPeakInput,
  CacheStatsDto,
  DescriptorsDto,
  EmbedProgressDto,
  EmbedProjectInput,
  FindSimilarInput,
  IndexEvent,
  IndexStatusDto,
  SimilarResultDto,
  EraseBiometricsDto,
  EraseBiometricsInput,
  FaceCropDto,
  GroupPeopleDto,
  GroupPeopleInput,
  IdentityCardDto,
  IdentityHandleDto,
  IdentityTimelineDto,
  ImageSubjectsDto,
  MergeIdentitiesInput,
  PeopleEvent,
  PeopleStatusDto,
  RenameIdentityInput,
  ScanFacesDto,
  ScanFacesInput,
  SetIdentityImportanceInput,
  SetIdentityRoleInput,
  SplitIdentityInput,
  ChapterHandleDto,
  ClassifyScenesInput,
  MergeChaptersInput,
  MoveBoundaryInput,
  SceneDto,
  SceneProfileDto,
  SetChapterInput,
  SplitChapterInput,
  DuplicateSetDto,
  GroupMomentsInput,
  LockMomentInput,
  MergeMomentsInput,
  MomentDto,
  MomentEditDto,
  MomentEvent,
  MomentHandleDto,
  MomentListDto,
  MomentStatusDto,
  MomentsInput,
  SetKeepHintInput,
  SplitMomentInput,
  StoryEvent,
  StoryOutlineDto,
  StoryStatusDto,
  CloudCacheStatsDto,
  CloudCallDto,
  CloudEvent,
  CloudSpendDto,
  CloudStatusDto,
  KeyCheckDto,
  SetAiKeyInput,
  SetCloudBudgetInput,
  SetCloudPrivacyInput,
  HardwarePlanDto,
  InferEvent,
  InferStatsDto,
  ModelStatusDto,
  SetExecutionProviderInput,
  WarmupReportDto,
  CreateProjectInput,
  GetPreviewInput,
  ImageRowLite,
  IngestEvent,
  IpcError,
  JobHandle,
  ListImagesInput,
  PrefetchInput,
  PreviewEvent,
  PreviewPayload,
  ProblemRow,
  ProjectHandle,
  ProjectSummary,
  SetCacheBudgetInput,
  SetCameraLabelInput,
  StartIngestInput,
  AnalyseIntegrityInput,
  DismissFlagInput,
  FlaggedInput,
  IntegrityDto,
  IntegrityEvent,
  IntegrityPassDto,
  IntegrityStatusDto,
  RankedFrameDto,
  WithinMomentInput,
  // PHASE-17.
  AdoptProfileInput,
  CompareProfilesInput,
  ExportProfileDto,
  ExportProfileInput,
  ImportProfileDto,
  ImportProfileInput,
  ProfileReportDto,
  ScanArchiveDto,
  ScanArchiveInput,
  SetProjectProfileInput,
  StyleComparisonDto,
  StylePairDto,
  StyleProfileDto,
  StyleStatusDto,
  TrainProfileDto,
  TrainProfileInput,
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

