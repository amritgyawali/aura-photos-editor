import { act, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { autopilot, cull, delivery, develop } from '../../ipc/client';
import { useStore } from '../../state/store';
import { useWorkflow } from '../../state/workflowStore';
import { WorkflowGuide } from './WorkflowGuide';

vi.mock('../../ipc/client', async (importOriginal) => {
  const original = await importOriginal<typeof import('../../ipc/client')>();
  return {
    ...original,
    inTauri: () => true,
    api: { ...original.api },
    autopilot: { autopilotSummary: vi.fn() },
    cull: { cullStatus: vi.fn() },
    develop: { developStatus: vi.fn() },
    delivery: { exportStatus: vi.fn(), exportManifest: vi.fn() },
  };
});

const cullStatus = {
  photos: 4000,
  eligible: 3800,
  selected: 412,
  coverage: 0.95,
  emotionAware: 0.9,
  compositionAware: 0.9,
  grouped: 3000,
  covered: 300,
  coveredWeak: 12,
  missing: 2,
  userKept: 4,
  userRejected: 1,
  mode: 'balanced',
  deterministicHash: 'h',
  modelVer: 1,
  analysisVer: 1,
  calibrationVer: 0,
};

const developStatus = {
  images: 4000,
  withRecipe: 412,
  fromAi: 380,
  fromUser: 32,
  touchedByHand: 32,
  sidecarBehind: 0,
};

const exportStatus = {
  photos: 4000,
  selected: 412,
  requested: 0,
  written: 0,
  verified: 0,
  unverified: 0,
  corrupt: 0,
  renderFailed: 0,
  renamed: 0,
  sidecars: 0,
  bytes: 0,
  manifestSealed: false,
  ms: 0,
};

function openProject(): void {
  act(() => {
    useStore.getState().setActiveProject('p-1');
  });
}

describe('WorkflowGuide', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkflow.getState().resetEvidence();
    useStore.getState().replaceRows([]);
    vi.mocked(autopilot.autopilotSummary).mockResolvedValue(null);
    vi.mocked(cull.cullStatus).mockResolvedValue(cullStatus);
    vi.mocked(develop.developStatus).mockResolvedValue(developStatus);
    vi.mocked(delivery.exportStatus).mockResolvedValue(exportStatus);
    vi.mocked(delivery.exportManifest).mockResolvedValue(null);
    openProject();
  });

  it('draws the five steps once a wedding is open', async () => {
    render(<WorkflowGuide currentStep="import" onGo={() => undefined} refreshToken={0} />);
    for (const title of ['Import', 'Analyze', 'Cull', 'Edit', 'Export']) {
      await waitFor(() =>
        expect(screen.getByRole('button', { name: new RegExp(title) })).toBeDefined(),
      );
    }
  });

  it('marks Analyze as never-run from the summary the panels already answer with', async () => {
    render(<WorkflowGuide currentStep="import" onGo={() => undefined} refreshToken={0} />);
    await waitFor(() => {
      const analyze = screen.getByRole('button', { name: /Analyze/ });
      expect(analyze.textContent).toContain('A pass has never run.');
    });
    expect(autopilot.autopilotSummary).toHaveBeenCalledWith('p-1');
  });

  it('reports a finished run with frames needing review as needing the photographer', async () => {
    vi.mocked(autopilot.autopilotSummary).mockResolvedValue({
      runId: 'r-1',
      status: 'completed',
      statusTitle: 'Done',
      selected: 412,
      exported: 0,
      needsReview: 37,
      degradedStages: [],
      spendUsd: 0,
      totalMs: 0,
      outputPath: '',
      stageTimings: [],
    } as Awaited<ReturnType<typeof autopilot.autopilotSummary>>);
    render(<WorkflowGuide currentStep="import" onGo={() => undefined} refreshToken={0} />);
    await waitFor(() => {
      const analyze = screen.getByRole('button', { name: /Analyze/ });
      expect(analyze.textContent).toContain('37 frames want a look.');
      expect(analyze.textContent).toContain('Needs you');
    });
  });

  it('hides the bar when a status read fails, rather than painting stale dots', async () => {
    // A catalog that will not answer makes every dot a guess, and the review's rule
    // for guidance is that ignorance renders as absent, not as "Waiting".
    vi.mocked(cull.cullStatus).mockRejectedValue(new Error('boom'));
    render(<WorkflowGuide currentStep="import" onGo={() => undefined} refreshToken={0} />);
    await waitFor(() => expect(screen.queryByRole('button', { name: /Cull/ })).toBeNull());
  });
});
