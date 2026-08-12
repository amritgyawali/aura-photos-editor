import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
  CreateProjectInput,
  ImageRowLite,
  IngestEvent,
  IpcError,
  JobHandle,
  ListImagesInput,
  ProblemRow,
  ProjectHandle,
  ProjectSummary,
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
};
