import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
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
