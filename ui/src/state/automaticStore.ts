import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type { OneClickStatusDto } from '../ipc/types';

type AutomaticState = {
  jobId: string | null;
  projectId: string | null;
  status: OneClickStatusDto | null;
  starting: boolean;
  error: string | null;
  adopt: (jobId: string, projectId: string) => void;
};

export const useAutomatic = create<AutomaticState>()(persist((set) => ({
  jobId: null, projectId: null, status: null, starting: false, error: null,
  adopt: (jobId, projectId) => set({ jobId, projectId, status: null, starting: false, error: null }),
}), {
  name: 'aura-automatic-run',
  storage: createJSONStorage(() => sessionStorage),
  partialize: ({ jobId, projectId, status }) => ({ jobId, projectId, status }),
}));

export const automaticBusy = (s: AutomaticState): boolean => s.starting ||
  (s.jobId !== null && (s.status === null || s.status.status === 'running' || s.status.status === 'cancelling'));
