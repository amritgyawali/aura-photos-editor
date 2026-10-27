import { invoke } from '@tauri-apps/api/core';

/** One piece of work in progress. ADR-0123. */
export type ActivityItem = {
  id: number;
  /** `folder`, `export`, `edit`, `retouch`, `ai`, `import`. */
  kind: string;
  label: string;
  /** The photograph or step it is on now. */
  detail: string;
  done: number;
  total: number;
  /** How long it has been running. */
  seconds: number;
};

/** Everything the application is doing at one moment. */
export type ActivityStatus = {
  items: ActivityItem[];
  imageWorkersBusy: number;
  imageWorkers: number;
};

/** Read it. Not written to the audit log: the status bar asks every second. */
export const activityStatus = (): Promise<ActivityStatus> => invoke<ActivityStatus>('activity_status');
