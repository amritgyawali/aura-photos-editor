import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
  CacheStatsDto,
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
};
