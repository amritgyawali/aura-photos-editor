import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type {
  ConsentDto,
  DeliveryStatusDto,
  DiagnosticsDto,
  ExportStatusDto,
  LearnBucketDto,
  LearnComparisonDto,
  LearnStatusDto,
} from '../../ipc/types';
import {
  DeliveryView,
  DiagnosticsView,
  ExportView,
  LearningView,
  ManifestView,
  stateWord,
} from './Delivery';

/**
 * PHASE-30. Every test here is about a distinction the screen must not collapse.
 *
 * This phase has four of them, and every one is a pair of facts that would render identically if
 * somebody simplified the panel:
 *
 * - **not checked** and **the check failed**: a choice somebody made, and a drive to replace;
 * - **cannot reach a gallery** and **the upload found nothing to do**;
 * - **arrived wrong** and **did not arrive**;
 * - **you have not corrected enough yet** and **your corrections disagree**.
 *
 * The fifth is about the whole phase: nothing here has been trained on a real archive, and a
 * panel that rendered a synthetic improvement as a measured one would be telling somebody their
 * profile got better.
 */

function exportStatus(overrides: Partial<ExportStatusDto> = {}): ExportStatusDto {
  return {
    photos: 4000,
    selected: 700,
    requested: 80,
    written: 80,
    verified: 80,
    unverified: 0,
    corrupt: 0,
    renderFailed: 0,
    renamed: 0,
    sidecars: 0,
    bytes: 1_000_000,
    manifestSealed: true,
    ms: 1000,
    ...overrides,
  };
}

function deliveryStatus(overrides: Partial<DeliveryStatusDto> = {}): DeliveryStatusDto {
  return {
    files: 80,
    backups: 0,
    backedUp: 0,
    diverged: 0,
    providers: 0,
    uploaded: 0,
    outstanding: 0,
    refused: 0,
    resumes: 0,
    unmappedSets: 0,
    bytesSent: 0,
    networkAvailable: false,
    ...overrides,
  };
}

function learnStatus(overrides: Partial<LearnStatusDto> = {}): LearnStatusDto {
  return {
    corrections: 0,
    projects: 0,
    buckets: 0,
    actionableBuckets: 0,
    unattributed: 0,
    attributionRate: 0,
    updates: 0,
    adopted: 0,
    consentedProjects: 0,
    contributingProjects: 0,
    fittedOnRealCorrections: false,
    ...overrides,
  };
}

function noop() {
  /* a callback the test does not care about */
}

describe('the export view', () => {
  it('shows three denominators, because an album export is not a failed project export', () => {
    render(
      <ExportView
        status={exportStatus()}
        presets={[]}
        selected="album"
        destination="/out"
        verify
        names={null}
        running={false}
        onSelectPreset={noop}
        onDestination={noop}
        onVerify={noop}
        onPreviewNames={noop}
        onRun={noop}
      />,
    );
    expect(screen.getByTestId('requested').textContent).toBe('80');
    expect(screen.getByTestId('selected').textContent).toBe('700');
    expect(screen.getByTestId('photos').textContent).toBe('4000');
  });

  it('renders "not checked" and "the check failed" as different things', () => {
    // The first is a choice somebody made. The second is a drive that should be replaced.
    const { rerender } = render(
      <ExportView
        status={exportStatus({ verified: 60, unverified: 20 })}
        presets={[]}
        selected="gallery"
        destination="/out"
        verify={false}
        names={null}
        running={false}
        onSelectPreset={noop}
        onDestination={noop}
        onVerify={noop}
        onPreviewNames={noop}
        onRun={noop}
      />,
    );
    expect(screen.getByTestId('unverified').textContent).toBe('20');
    expect(screen.queryByTestId('corrupt')).toBeNull();

    rerender(
      <ExportView
        status={exportStatus({ verified: 79, corrupt: 1 })}
        presets={[]}
        selected="gallery"
        destination="/out"
        verify
        names={null}
        running={false}
        onSelectPreset={noop}
        onDestination={noop}
        onVerify={noop}
        onPreviewNames={noop}
        onRun={noop}
      />,
    );
    expect(screen.getByTestId('corrupt').textContent).toBe('1');
    expect(screen.queryByTestId('unverified')).toBeNull();
  });

  it('warns before the job when the read-back is off, not afterwards', () => {
    render(
      <ExportView
        status={null}
        presets={[]}
        selected="gallery"
        destination="/out"
        verify={false}
        names={null}
        running={false}
        onSelectPreset={noop}
        onDestination={noop}
        onVerify={noop}
        onPreviewNames={noop}
        onRun={noop}
      />,
    );
    expect(screen.getByTestId('verify-off')).toBeTruthy();
  });

  it('shows a preset with the argument for it', () => {
    // A preset nobody can explain is a preset nobody can argue with.
    render(
      <ExportView
        status={null}
        presets={[
          {
            name: 'album',
            format: 'tiff',
            quality: 100,
            colour: 'adobe_rgb',
            bitDepth: 16,
            resize: 'full',
            sharpen: 'print',
            naming: '{seq}',
            sidecar: false,
            reason: 'Sixteen bits in Adobe RGB so the lab profile is not the second quantisation.',
          },
        ]}
        selected="album"
        destination="/out"
        verify
        names={null}
        running={false}
        onSelectPreset={noop}
        onDestination={noop}
        onVerify={noop}
        onPreviewNames={noop}
        onRun={noop}
      />,
    );
    expect(screen.getByTestId('preset-reason').textContent).toContain('second quantisation');
  });

  it('will not run without a destination, and shows names without writing', () => {
    const onPreview = vi.fn();
    const onRun = vi.fn();
    render(
      <ExportView
        status={null}
        presets={[]}
        selected="gallery"
        destination="  "
        verify
        names={[
          {
            imageId: 'pht_1',
            set: 'gallery',
            path: 'gallery/DSC_0431.jpg',
            renamed: false,
            reasons: [],
          },
          {
            imageId: 'pht_2',
            set: 'gallery',
            path: 'gallery/DSC_0431_2.jpg',
            renamed: true,
            reasons: [
              { code: 'name_collision_resolved', text: 'A number was added.', fatal: false },
            ],
          },
        ]}
        running={false}
        onSelectPreset={noop}
        onDestination={noop}
        onVerify={noop}
        onPreviewNames={onPreview}
        onRun={onRun}
      />,
    );
    const run = screen.getByTestId('run') as HTMLButtonElement;
    expect(run.disabled).toBe(true);
    fireEvent.click(screen.getByTestId('preview-names'));
    expect(onPreview).toHaveBeenCalled();
    expect(screen.getByTestId('names').textContent).toContain('DSC_0431_2.jpg');
  });
});

describe('the manifest view', () => {
  it('says a wedding has not been delivered rather than showing an empty manifest', () => {
    // Null is not an empty manifest, and the two are different answers.
    render(<ManifestView manifest={null} files={[]} />);
    expect(screen.getByTestId('no-manifest').textContent).toContain('not been delivered');
  });

  it('names what was removed, because a removal nobody can audit is not disclosed', () => {
    render(
      <ManifestView
        manifest={{
          projectId: 'prj_1',
          createdAt: 0,
          files: 1,
          bytes: 1000,
          sets: [['gallery', 1]],
          qcReportPath: null,
          cleanupDisclosures: [['pht_1', 'an exit sign was removed from the background']],
          engineVersions: [],
          fullyHashed: true,
        }}
        files={[]}
      />,
    );
    expect(screen.getByTestId('disclosures').textContent).toContain('exit sign');
  });
});

describe('the delivery view', () => {
  it('says this build cannot reach a gallery, before the provider list', () => {
    // Otherwise a photographer configures a provider, presses upload, sees nothing happen and
    // concludes their credentials are wrong.
    render(
      <DeliveryView
        status={deliveryStatus()}
        providers={[]}
        items={[]}
        backupPath=""
        onBackupPath={noop}
        onBackup={noop}
        onUpload={noop}
      />,
    );
    expect(screen.getByTestId('no-network').textContent).toContain('cannot reach');
  });

  it('renders a diverged backup as a fault a photographer has to act on', () => {
    render(
      <DeliveryView
        status={deliveryStatus({ diverged: 1, backedUp: 40 })}
        providers={[]}
        items={[]}
        backupPath=""
        onBackupPath={noop}
        onBackup={noop}
        onUpload={noop}
      />,
    );
    expect(screen.getByTestId('diverged-warning').textContent).toContain('Check that drive');
  });

  it('offers upload only for a provider with a sign-in', () => {
    const onUpload = vi.fn();
    render(
      <DeliveryView
        status={deliveryStatus()}
        providers={[
          { id: 'a', label: 'A gallery', hasCredential: false, mayPublish: false },
          { id: 'b', label: 'B gallery', hasCredential: true, mayPublish: false },
        ]}
        items={[]}
        backupPath=""
        onBackupPath={noop}
        onBackup={noop}
        onUpload={onUpload}
      />,
    );
    const buttons = screen.getAllByText('Upload');
    expect(buttons).toHaveLength(1);
    fireEvent.click(buttons[0]);
    expect(onUpload).toHaveBeenCalledWith('b');
  });

  it('reads "arrived wrong" and "did not arrive" differently', () => {
    // Only one of the two is worth re-sending immediately, and a photographer reading the list
    // should be able to tell which.
    expect(stateWord('corrupt')).toContain('arrived wrong');
    expect(stateWord('failed')).toContain('did not arrive');
    expect(stateWord('verified')).toContain('checked');
    expect(stateWord('corrupt')).not.toBe(stateWord('failed'));
  });
});

describe('the learning view', () => {
  it('says nothing here was trained on a real archive', () => {
    render(
      <LearningView
        status={learnStatus()}
        buckets={[]}
        comparison={null}
        consent={null}
        onAdopt={noop}
        onRollBack={noop}
        onConsent={noop}
      />,
    );
    expect(screen.getByTestId('not-fitted').textContent).toContain('real photographer');
  });

  it('distinguishes "not enough yet" from "nothing to learn"', () => {
    render(
      <LearningView
        status={learnStatus({ corrections: 8, projects: 1, buckets: 1, actionableBuckets: 0 })}
        buckets={[]}
        comparison={null}
        consent={null}
        onAdopt={noop}
        onRollBack={noop}
        onConsent={noop}
      />,
    );
    expect(screen.getByTestId('waiting').textContent).toContain('more than one wedding');
  });

  it('shows what the loop ignored rather than letting it look like nothing moved', () => {
    const bucket: LearnBucketDto = {
      learnable: 'exposure',
      label: 'Exposure',
      scene: 'ceremony_indoor',
      subjectClose: false,
      corrections: 44,
      projects: 3,
      outliersDropped: 4,
      central: 0.2,
      dispersion: 0.02,
      heldOut: 11,
      actionable: true,
      proposedOffset: 0.1,
    };
    render(
      <LearningView
        status={learnStatus({ corrections: 44 })}
        buckets={[bucket]}
        comparison={null}
        consent={null}
        onAdopt={noop}
        onRollBack={noop}
        onConsent={noop}
      />,
    );
    expect(screen.getByTestId('buckets').textContent).toContain('4 left out as unusual');
  });

  it('will not let a candidate that is not offerable be adopted', () => {
    const comparison: LearnComparisonDto = {
      profileId: 'prf_1',
      currentVersion: 3,
      candidateVersion: 4,
      currentError: 0.2,
      candidateError: 0.199,
      heldOut: 12,
      improvement: 0.005,
      offerable: false,
      rows: [],
      reasons: [
        { code: 'held_out_no_improvement', text: 'No better, so there is nothing to adopt.', fatal: false },
      ],
    };
    const onAdopt = vi.fn();
    render(
      <LearningView
        status={learnStatus()}
        buckets={[]}
        comparison={comparison}
        consent={null}
        onAdopt={onAdopt}
        onRollBack={noop}
        onConsent={noop}
      />,
    );
    const adopt = screen.getByTestId('adopt') as HTMLButtonElement;
    expect(adopt.disabled).toBe(true);
    expect(screen.getByTestId('comparison').textContent).toContain('nothing to adopt');
  });

  it('keeps the two consents separate and says when nothing leaves', () => {
    // "May this machine learn" and "may evidence leave it" are different questions, and
    // collapsing them is how the second happens by accident.
    const consent: ConsentDto = {
      projectId: 'prj_1',
      localLearning: true,
      datasetContribution: false,
      crashReports: false,
      telemetry: false,
      decidedAt: 0,
      appVersion: '0.1.0',
      anythingLeaves: false,
    };
    const onConsent = vi.fn();
    render(
      <LearningView
        status={learnStatus()}
        buckets={[]}
        comparison={null}
        consent={consent}
        onAdopt={noop}
        onRollBack={noop}
        onConsent={onConsent}
      />,
    );
    expect(screen.getByTestId('nothing-leaves')).toBeTruthy();
    fireEvent.click(screen.getByTestId('consent-dataset'));
    expect(onConsent).toHaveBeenCalledWith({ ...consent, datasetContribution: true });
  });
});

describe('the diagnostics view', () => {
  it('leads with what this machine cannot do', () => {
    const report: DiagnosticsDto = {
      appVersion: '0.1.0',
      schemaVersion: 30,
      renderBackend: 'cpu',
      renderDegradation: 'AURA is developing photographs using the processor.',
      modelSet: 'abcdef0123456789',
      stagesOff: ['cleanup'],
      networkTransport: false,
      trainedModels: false,
      providers: [],
      recentErrors: [],
    };
    render(<DiagnosticsView report={report} />);
    const caveats = screen.getByTestId('caveats').textContent ?? '';
    expect(caveats).toContain('processor');
    expect(caveats).toContain('cannot upload');
    expect(caveats).toContain('placeholder');
    expect(caveats).toContain('cleanup is switched off');
    expect(screen.getByTestId('schema').textContent).toContain('30');
  });
});
