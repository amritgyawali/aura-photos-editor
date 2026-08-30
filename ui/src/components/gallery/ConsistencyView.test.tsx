import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ConsistencyView } from './ConsistencyView';
import { OutlierList } from './OutlierList';
import { TimelineStrips, kelvinSwatch } from './TimelineStrips';
import type {
  GalleryDeltaDto,
  GalleryOutlierDto,
  GalleryStatusDto,
  SceneNodeDto,
} from '../../ipc/types';

/**
 * PHASE-25. The five things this panel must get right, and three of them are about what AURA
 * *could not* do rather than about what it did.
 */

function status(over: Partial<GalleryStatusDto> = {}): GalleryStatusDto {
  return {
    photos: 400,
    normalised: 400,
    coverage: 1,
    nodes: 10,
    anchoredNodes: 10,
    splitNodes: 2,
    pinnedAnchors: 0,
    bounded: 6,
    moodPreserved: 20,
    userEdited: 0,
    outliers: 3,
    skinTargeted: 0,
    identities: 12,
    spreadBeforeCct: 500,
    spreadAfterCct: 115,
    spreadBeforeEv: 0.2,
    spreadAfterEv: 0.05,
    worstSkinSpread: 0,
    untargetedScenes: [],
    skinFieldAvailable: false,
    policyVer: 1,
    ...over,
  };
}

function node(over: Partial<SceneNodeDto> = {}): SceneNodeDto {
  return {
    nodeId: 'nod_1',
    parentId: null,
    segmentId: 'seg_1',
    label: 'Ceremony',
    scene: 'ceremony',
    imageCount: 2,
    anchors: ['pht_a'],
    target: {
      cctK: 5000,
      cctTol: 150,
      tint: 0,
      tintTol: 4,
      subjectLuma: 0.45,
      lumaTol: 0.05,
      contrast: 10,
      saturation: 4,
      anchorCount: 4,
      cohesion: 0.9,
    },
    reasons: [],
    ...over,
  };
}

function delta(over: Partial<GalleryDeltaDto> = {}): GalleryDeltaDto {
  return {
    photoId: 'pht_a',
    nodeId: 'nod_1',
    dExposure: 0.05,
    dCct: -120,
    dTint: 0,
    dContrast: 0,
    dSaturation: 0,
    fromExposureEv: 0,
    fromCctK: 5200,
    fromTint: 0,
    damping: 0.8,
    boundedBy: null,
    magnitude: 0.27,
    skinIdentity: null,
    skinDe00Before: null,
    skinDe00After: null,
    confidence: 0.8,
    reasons: [{ code: 'warmth_normalised', text: 'The warmth was brought into line.', withdraws: false }],
    userEdited: false,
    ...over,
  };
}

function outlier(over: Partial<GalleryOutlierDto> = {}): GalleryOutlierDto {
  return {
    photoId: 'pht_z',
    nodeId: 'nod_1',
    description: '+310 K warmer than the anchors, skin cast 4.2 dE00',
    residualCct: 310,
    residualTint: 0,
    residualExposure: 0,
    residualSkinDe00: 4.2,
    deviation: 0.7,
    reasons: [
      {
        code: 'outlier_after_normalisation',
        text: 'This frame is still noticeably different.',
        withdraws: false,
      },
    ],
    ...over,
  };
}

function view(over: Partial<Parameters<typeof ConsistencyView>[0]> = {}) {
  return (
    <ConsistencyView
      status={status()}
      nodes={[node()]}
      selectedNodeId="nod_1"
      deltas={[delta(), delta({ photoId: 'pht_b', fromCctK: 4800, dCct: 140 })]}
      outliers={[outlier()]}
      onSelectNode={vi.fn()}
      onRunPass={vi.fn()}
      onPin={vi.fn()}
      {...over}
    />
  );
}

describe('ConsistencyView', () => {
  it('shows both denominators, not just coverage', () => {
    render(view({ status: status({ anchoredNodes: 2 }) }));
    expect(screen.getByText(/Photographs matched/)).toBeTruthy();
    expect(screen.getByText(/Parts anchored/)).toBeTruthy();
    expect(screen.getByText(/2 of 10/)).toBeTruthy();
  });

  it('says loudly when parts of the wedding could not be anchored', () => {
    render(view({ status: status({ anchoredNodes: 2 }) }));
    const warning = screen.getByText(/nothing in them was matched to anything/i);
    expect(warning).toBeTruthy();
  });

  it('does not warn when every part is anchored', () => {
    render(view());
    expect(screen.queryByText(/nothing in them was matched to anything/i)).toBeNull();
  });

  it('shows the spread before and after in its own units, not only a percentage', () => {
    render(view());
    expect(screen.getByText(/500 K → 115 K/)).toBeTruthy();
    expect(screen.getByText(/0\.20 EV → 0\.05 EV/)).toBeTruthy();
  });

  it('never claims anything about skin while the skin field is unavailable', () => {
    render(view());
    expect(
      screen.getByText(/cannot yet tell which pixels are a person's skin/i),
    ).toBeTruthy();
    expect(screen.queryByText(/Skin matched for/)).toBeNull();
  });

  it('reports the skin spread once the field is available', () => {
    render(
      view({
        status: status({ skinFieldAvailable: true, skinTargeted: 9, worstSkinSpread: 1.4 }),
      }),
    );
    expect(screen.getByText(/Skin matched for 9 of 12 people/)).toBeTruthy();
  });

  it('marks an unanchored node in the tree', () => {
    render(view({ nodes: [node({ target: null })] }));
    expect(screen.getAllByText('not anchored').length).toBeGreaterThan(0);
  });

  it('marks a node a change point split', () => {
    render(view({ nodes: [node({ parentId: 'nod_0' })] }));
    expect(screen.getByText('split')).toBeTruthy();
  });
});

describe('TimelineStrips', () => {
  it('draws a before row and an after row for each of the two channels', () => {
    render(<TimelineStrips deltas={[delta()]} target={node().target} />);
    // Two strip pairs - warmth and brightness - so two of each label. Asserting on the count
    // rather than on a single match is the point: a pair that lost one of its rows would still
    // find "Before" somewhere.
    expect(screen.getAllByText('Before')).toHaveLength(2);
    expect(screen.getAllByText('After')).toHaveLength(2);
    expect(screen.getByText('Warmth')).toBeTruthy();
    expect(screen.getByText('Brightness')).toBeTruthy();
  });

  it('shows the tolerance the node is judged by', () => {
    render(<TimelineStrips deltas={[delta()]} target={node().target} />);
    expect(screen.getByText(/consistent within ±150 K here/)).toBeTruthy();
  });

  it('says an unanchored node was not matched rather than showing an empty strip', () => {
    render(<TimelineStrips deltas={[delta()]} target={null} />);
    expect(screen.getByText(/nothing here was matched to anything/i)).toBeTruthy();
  });

  it('maps a warm value and a cool value to different colours', () => {
    expect(kelvinSwatch(7500)).not.toEqual(kelvinSwatch(2500));
    expect(kelvinSwatch(5000)).toEqual(kelvinSwatch(5000));
  });
});

describe('OutlierList', () => {
  it('renders the sentence the wire supplied rather than assembling its own', () => {
    render(<OutlierList outliers={[outlier()]} />);
    expect(
      screen.getByText('+310 K warmer than the anchors, skin cast 4.2 dE00'),
    ).toBeTruthy();
  });

  it('says what to do about a node whose references disagree', () => {
    render(
      <OutlierList
        outliers={[
          outlier({
            reasons: [
              { code: 'anchors_disagree', text: 'The references disagree.', withdraws: false },
            ],
          }),
        ]}
      />,
    );
    expect(screen.getByText(/Pin one you trust/i)).toBeTruthy();
  });

  it('says nothing drifted rather than showing an empty list', () => {
    render(<OutlierList outliers={[]} />);
    expect(screen.getByText(/came into line/i)).toBeTruthy();
  });
});
