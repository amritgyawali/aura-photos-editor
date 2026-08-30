import { useMemo } from 'react';

import type {
  GalleryDeltaDto,
  GalleryOutlierDto,
  GalleryStatusDto,
  SceneNodeDto,
} from '../../ipc/types';
import { AnchorPicker } from './AnchorPicker';
import { OutlierList } from './OutlierList';
import { TimelineStrips } from './TimelineStrips';

/**
 * PHASE-25. The gallery consistency view: a whole wedding as one body of work.
 *
 * Section 9's SFE deliverable, and the first panel in this product whose subject is a wedding
 * rather than a photograph.
 *
 * Five rules:
 *
 * 1. **Both denominators are in the header, and the second is the one that matters when it is low.**
 *    A project at 100 % coverage with 20 % anchored has had almost nothing done to it: an
 *    unanchored node produces a zero delta for every frame in it, and a zero delta is still a row.
 *    A panel that led with coverage alone would render a wedding nobody could judge as a wedding
 *    that needed no work - which is the exact failure this whole phase exists to make visible.
 *
 * 2. **The spread is shown before and after, in its own units.** "77 % closer" alone cannot tell
 *    500 K → 115 K from 20 K → 4.6 K, and only the first is worth telling a photographer about.
 *
 * 3. **Nothing about skin is claimed while `skinFieldAvailable` is false.** Phase 18's segmenter is
 *    a placeholder, so no photograph in this build has an identity-scoped skin region and nothing
 *    about anybody's skin was measured. The panel says that in a sentence rather than showing a
 *    zero that reads as "no problems found". This is the single most damaging thing the product
 *    could say wrongly, because it is a promise about people.
 *
 * 4. **A node is a lighting group, not a chapter.** The tree is read-only here: renaming, splitting
 *    and merging *chapters* is the story panel's job, and a second editable tree would be a second
 *    answer to what a wedding's structure is. What a photographer changes here is which frames a
 *    node is matched to.
 *
 * 5. **Running the pass is one button and it says what it did.** No progress bar, because there is
 *    no partial state a reader could make sense of - a node half-solved against one target and half
 *    against another has a target that describes neither.
 *
 * Pure: rows and callbacks in, no fetching, no pixels.
 */
export type ConsistencyViewProps = {
  /** The project header. */
  status: GalleryStatusDto | null;
  /** The node tree, in capture order. */
  nodes: SceneNodeDto[];
  /** Which node is open. */
  selectedNodeId: string | null;
  /** The open node's deltas, in capture order. */
  deltas: GalleryDeltaDto[];
  /** The project's outlier queue, worst first. */
  outliers: GalleryOutlierDto[];
  /** Which photograph is selected. */
  selectedPhotoId?: string | null;
  /** Open a node. */
  onSelectNode: (nodeId: string) => void;
  /** Open a photograph. */
  onSelectPhoto?: (photoId: string) => void;
  /** Run the consistency pass over the project. */
  onRunPass: () => void;
  /** Pin or reject one anchor. */
  onPin: (photoId: string, pinned: boolean) => void;
  /** True while the pass is running. */
  busy?: boolean;
};

export function ConsistencyView({
  status,
  nodes,
  selectedNodeId,
  deltas,
  outliers,
  selectedPhotoId,
  onSelectNode,
  onSelectPhoto,
  onRunPass,
  onPin,
  busy = false,
}: ConsistencyViewProps): JSX.Element {
  const node = useMemo(
    () => nodes.find((candidate) => candidate.nodeId === selectedNodeId) ?? null,
    [nodes, selectedNodeId],
  );

  if (status === null) {
    return (
      <section className="consistency-view consistency-view--empty">
        <p>Open a wedding to see how consistently it reads.</p>
      </section>
    );
  }

  const anchoredShare = status.nodes > 0 ? status.anchoredNodes / status.nodes : 0;
  const cctReduction =
    status.spreadBeforeCct > 0 ? 1 - status.spreadAfterCct / status.spreadBeforeCct : 0;
  const evReduction =
    status.spreadBeforeEv > 0 ? 1 - status.spreadAfterEv / status.spreadBeforeEv : 0;

  return (
    <section className="consistency-view" aria-label="Gallery consistency">
      <header className="consistency-view__header">
        <h3>Gallery consistency</h3>
        <button type="button" onClick={onRunPass} disabled={busy}>
          {busy ? 'Matching…' : 'Match this wedding'}
        </button>
      </header>

      <dl className="consistency-view__stats">
        <div>
          <dt>Photographs matched</dt>
          <dd>
            {status.normalised} of {status.photos}
            {status.photos > 0 ? ` (${Math.round(status.coverage * 100)} %)` : ''}
          </dd>
        </div>
        <div>
          <dt>Parts anchored</dt>
          <dd className={anchoredShare < 0.5 ? 'consistency-view__stat--low' : undefined}>
            {status.anchoredNodes} of {status.nodes}
            {status.nodes > 0 ? ` (${Math.round(anchoredShare * 100)} %)` : ''}
          </dd>
        </div>
        <div>
          <dt>Warmth spread</dt>
          <dd>
            {Math.round(status.spreadBeforeCct)} K → {Math.round(status.spreadAfterCct)} K
            {status.spreadBeforeCct > 0 ? ` (${Math.round(cctReduction * 100)} % closer)` : ''}
          </dd>
        </div>
        <div>
          <dt>Brightness spread</dt>
          <dd>
            {status.spreadBeforeEv.toFixed(2)} EV → {status.spreadAfterEv.toFixed(2)} EV
            {status.spreadBeforeEv > 0 ? ` (${Math.round(evReduction * 100)} % closer)` : ''}
          </dd>
        </div>
        <div>
          <dt>Left alone on purpose</dt>
          <dd>
            {status.moodPreserved} for their light, {status.userEdited} set by you
          </dd>
        </div>
        <div>
          <dt>Still out of line</dt>
          <dd>{status.outliers}</dd>
        </div>
      </dl>

      {status.anchoredNodes < status.nodes ? (
        <p className="consistency-view__caveat" role="status">
          {status.nodes - status.anchoredNodes} part
          {status.nodes - status.anchoredNodes === 1 ? '' : 's'} of this wedding had too few frames
          AURA was confident about, so <strong>nothing in them was matched to anything</strong>.
          Pinning a reference frame fixes one at a time.
        </p>
      ) : null}

      {status.skinFieldAvailable ? (
        <p className="consistency-view__skin">
          Skin matched for {status.skinTargeted} of {status.identities} people, worst spread{' '}
          {status.worstSkinSpread.toFixed(2)} dE00.
        </p>
      ) : (
        <p className="consistency-view__skin consistency-view__skin--unavailable" role="status">
          AURA cannot yet tell which pixels are a person's skin, so it has not adjusted anybody's
          skin in this wedding and has measured nothing about it.
        </p>
      )}

      {status.untargetedScenes.length > 0 ? (
        <p className="consistency-view__caveat">
          {status.untargetedScenes.length} kind
          {status.untargetedScenes.length === 1 ? '' : 's'} of photograph have no matching guidance
          recorded yet, so AURA used its most careful settings on them.
        </p>
      ) : null}

      <div className="consistency-view__body">
        <nav className="consistency-view__tree" aria-label="Parts of the wedding">
          <ol>
            {nodes.map((candidate) => (
              <li key={candidate.nodeId}>
                <button
                  type="button"
                  className={
                    candidate.nodeId === selectedNodeId
                      ? 'node-row node-row--selected'
                      : 'node-row'
                  }
                  onClick={() => onSelectNode(candidate.nodeId)}
                >
                  <span className="node-row__label">{candidate.label}</span>
                  <span className="node-row__count">{candidate.imageCount}</span>
                  {candidate.target === null ? (
                    <span className="node-row__flag" title="Not anchored - nothing here was matched">
                      not anchored
                    </span>
                  ) : null}
                  {candidate.parentId ? (
                    <span className="node-row__flag" title="Split because the light changed">
                      split
                    </span>
                  ) : null}
                </button>
              </li>
            ))}
          </ol>
        </nav>

        <div className="consistency-view__detail">
          {node ? (
            <>
              <TimelineStrips
                deltas={deltas}
                target={node.target}
                selectedPhotoId={selectedPhotoId ?? null}
                onSelect={onSelectPhoto}
              />
              <AnchorPicker
                node={node}
                deltas={deltas}
                onPin={onPin}
                onSelect={onSelectPhoto}
                selectedPhotoId={selectedPhotoId ?? null}
                busy={busy}
              />
            </>
          ) : (
            <p>Choose a part of the wedding to see how it was matched.</p>
          )}
        </div>
      </div>

      <OutlierList
        outliers={outliers}
        selectedPhotoId={selectedPhotoId ?? null}
        onSelect={onSelectPhoto}
        onOpenNode={onSelectNode}
      />
    </section>
  );
}
