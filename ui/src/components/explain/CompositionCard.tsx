import { useEffect, useId, useRef, useState, type CSSProperties } from 'react';

import { asIpcError, composition as compositionApi } from '../../ipc/client';
import type {
  CompositionDto,
  CompositionJointCutDto,
  CompositionReasonDto,
  CropRectDto,
} from '../../ipc/types';
import { useThumbnails, type ThumbnailEntry } from '../../stores/thumbnailStore';

/**
 * PHASE-11's Explain card.
 *
 * The overlay draws geometry; the text beside it carries the judgement. Every
 * line and box has a different stroke pattern and a written legend, so red/green
 * perception is never required to tell a guide, a measured horizon, evidence, a
 * violation or a future crop hint apart.
 *
 * A crop hint is displayed as a suggestion only. This card cannot crop,
 * straighten, remove a distraction, keep or reject a photograph.
 */

const NON_VIOLATION_FLAGS = new Set(['intentional_tilt', 'centred', 'no_geometry']);
const CANCEL_CODES = new Set(['AURA-JOB-7001', 'AURA-ML-5011']);

type LoadState =
  | { kind: 'loading' }
  | { kind: 'empty' }
  | { kind: 'ready'; value: CompositionDto }
  | { kind: 'error'; message: string }
  | { kind: 'cancelled' };

/** How the fused score reads without making a keep/reject decision. */
export function scoreLabel(score: number): string {
  if (score >= 0.85) {
    return 'cleanly composed';
  }
  if (score >= 0.65) {
    return 'compositionally sound';
  }
  if (score >= 0.4) {
    return 'compositionally unsettled';
  }
  return 'strongly compromised';
}

/** Lead with the strongest exact explanation, not an unexplained decimal. */
export function headline(result: CompositionDto): string {
  const strongest = orderedReasons(result).find((reason) => reason.weight < 0);
  const label = scoreLabel(result.compositionScore);
  if (!strongest) {
    return `AURA reads this frame as ${label}. Nothing counted against its framing.`;
  }
  return `AURA reads this frame as ${label}, mostly because ${strongest.text}.`;
}

/** Missing data is not a clean verdict. */
export function notAnalysedSentence(): string {
  return 'AURA has not analysed this photograph\'s composition yet. That is not the same as finding its framing clean.';
}

/** A stopped read is neutral, and completed project rows remain durable. */
export function cancelledSentence(): string {
  return 'The composition reading stopped before this photograph finished. Everything already analysed remains saved; this photograph still has no judgement.';
}

/** Clamp model values before presenting percentages. */
export function percent(value: number): string {
  return `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`;
}

/** Explain why confidence fell using the backend's reasons and source fields. */
export function confidenceSentence(result: CompositionDto): string {
  const why: string[] = [];
  if (result.flags.includes('no_geometry')) {
    why.push('no face or body geometry was available');
  }
  if (result.reasons.some((reason) => reason.code === 'no_rule')) {
    why.push('this scene used neutral framing rules');
  }
  if (result.reasons.some((reason) => reason.code === 'aesthetic_unavailable')) {
    why.push('the learned aesthetic head was unavailable');
  }
  if (result.reasons.some((reason) => reason.code === 'keypoints_unavailable')) {
    why.push('body keypoints were unavailable');
  }
  if (
    result.horizonSource === 'none' ||
    result.reasons.some((reason) => reason.code === 'horizon_uncertain')
  ) {
    why.push('there was no reliable horizon');
  }
  if (result.scene === 'unknown' && !why.includes('this scene used neutral framing rules')) {
    why.push('this photograph has no scene label');
  }
  if (why.length === 0) {
    return `Confidence ${result.confidence.toFixed(2)}.`;
  }
  return `Confidence ${result.confidence.toFixed(2)}, lowered because ${why.join(', and ')}.`;
}

/** The measured horizon, including the important absence and intentional states. */
export function horizonSentence(result: CompositionDto): string {
  if (result.horizonSource === 'none') {
    return 'No reliable horizon was found, so the overlay makes no claim that the frame is level.';
  }
  const direction = result.tiltDeg < 0 ? 'counter-clockwise' : 'clockwise';
  const angle = Math.abs(result.tiltDeg).toFixed(1);
  if (result.tiltIntentional) {
    return `The measured horizon is ${angle} degrees ${direction}; AURA reads that tilt as deliberate.`;
  }
  return `The measured horizon is ${angle} degrees ${direction}, with ${percent(result.horizonConf)} confidence.`;
}

/** Headroom in words, preserving negative values that mean a cropped head. */
export function headroomSentence(result: CompositionDto): string {
  if (result.flags.includes('no_geometry')) {
    return 'Headroom was not measured because no subject geometry was available.';
  }
  if (result.headroom < 0) {
    return `The frame cuts about ${percent(Math.abs(result.headroom))} of a head above the top edge.`;
  }
  return `Headroom uses ${percent(result.headroom)} of the frame height.`;
}

/** Placement is reported against the actual thirds metric. */
export function placementSentence(result: CompositionDto): string {
  return `The main subject sits ${percent(result.thirdsOffset)} of the frame diagonal from the nearest rule-of-thirds power point; balance is ${percent(result.balance)}.`;
}

/** Background clutter is already scene-relative: one is the tolerance. */
export function backgroundSentence(result: CompositionDto): string {
  const clutter = Math.round(Math.max(0, result.clutter) * 100);
  const qualifier = result.clutter > 1 ? 'above' : 'within';
  return `Background clutter is ${clutter}% of this scene's tolerance (${qualifier} the limit); colour competition is ${percent(result.colourCompetition)}.`;
}

/** The crop hand-off says explicitly that it has not been applied. */
export function cropHintSentence(result: CompositionDto): string {
  const hint = result.cropSuggestionHint;
  if (hint === null) {
    return 'AURA has no safe crop suggestion for this frame.';
  }
  const straighten =
    hint.straightenDeg === null
      ? 'no straighten angle is claimed'
      : `a ${Math.abs(hint.straightenDeg).toFixed(1)} degree straighten is suggested`;
  if (!hint.actionable) {
    return `A crop region was recorded, but it offers no safe adjustment; ${straighten}. Nothing has been applied.`;
  }
  return `The dashed crop hint preserves the marked region with a ${percent(hint.safeMargin)} safe margin; ${straighten}. Nothing has been applied.`;
}

/** Do not call the deterministic reference value learned while the head is withheld. */
export function aestheticSentence(result: CompositionDto): string {
  const learned = !result.reasons.some((reason) => reason.code === 'aesthetic_unavailable');
  return `${learned ? 'learned' : 'reference'} aesthetic ${percent(result.aesthetic)}`;
}

/** Penalties first, followed by the context that withdrew claims. */
export function orderedReasons(result: CompositionDto): CompositionReasonDto[] {
  const penalties = result.reasons.filter((reason) => reason.weight < 0);
  const rest = result.reasons.filter((reason) => reason.weight >= 0);
  return [...penalties, ...rest];
}

/** Non-defect flags describe style or missing geometry and cannot be dismissed. */
export function dismissible(flag: string): boolean {
  return !NON_VIOLATION_FLAGS.has(flag);
}

/** Stable human-readable flag label, without owning any flag semantics. */
export function flagLabel(flag: string): string {
  return flag.replace(/_/g, ' ');
}

/** Describe one crop violation without relying on its box colour. */
export function jointCutSentence(cut: CompositionJointCutDto): string {
  const location = cut.atJoint ? `at the ${cut.joint}` : `near the ${cut.joint}, between joints`;
  const decision = cut.flagged ? 'flagged as a violation' : 'recorded below the flag threshold';
  return `The ${cut.edge} edge cuts ${location}; severity ${percent(cut.severity)}, ${decision}.`;
}

/** Percentage positioning shared by evidence and hint boxes outside the SVG. */
export function boxStyle(box: CropRectDto): CSSProperties {
  return {
    left: `${(box.x * 100).toFixed(2)}%`,
    top: `${(box.y * 100).toFixed(2)}%`,
    width: `${(box.w * 100).toFixed(2)}%`,
    height: `${(box.h * 100).toFixed(2)}%`,
  };
}

function svgBox(box: CropRectDto): { x: number; y: number; width: number; height: number } {
  return {
    x: box.x * 100,
    y: box.y * 100,
    width: box.w * 100,
    height: box.h * 100,
  };
}

/** A complete screen-reader summary of what the graphical overlay contains. */
export function overlayDescription(result: CompositionDto): string {
  const horizon =
    result.horizonSource === 'none'
      ? 'No measured horizon.'
      : `Measured horizon ${Math.abs(result.tiltDeg).toFixed(1)} degrees ${
          result.tiltDeg < 0 ? 'counter-clockwise' : 'clockwise'
        }.`;
  const hint = result.cropSuggestionHint ? 'A crop hint is outlined but not applied.' : 'No crop hint.';
  return `Rule-of-thirds guides. ${horizon} ${result.reasons.filter((reason) => reason.evidence !== null).length} reason evidence boxes, ${result.jointCuts.length} body-cut boxes, ${result.edgeIntrusions.length} edge intrusions and ${result.brightBlobs.length} bright distractions. ${hint}`;
}

/** The rule-of-thirds, horizon, evidence, violation and crop-hint overlay. */
type OverlayPreview = Pick<ThumbnailEntry, 'dataUrl' | 'width' | 'height'>;

export function CompositionOverlay({
  result,
  preview,
}: {
  result: CompositionDto;
  preview?: OverlayPreview;
}): JSX.Element {
  const titleId = useId();
  const descriptionId = useId();
  const evidence = result.reasons.filter(
    (reason): reason is CompositionReasonDto & { evidence: CropRectDto } =>
      reason.evidence !== null,
  );

  return (
    <figure className="composition-card__figure">
      <div
        className="composition-card__canvas"
        style={{
          aspectRatio:
            preview && preview.width > 0 && preview.height > 0
              ? `${preview.width} / ${preview.height}`
              : '3 / 2',
        }}
      >
        {preview ? (
          <img
            className="composition-card__preview"
            src={preview.dataUrl}
            alt=""
            width={preview.width}
            height={preview.height}
          />
        ) : (
          <div className="composition-card__preview-placeholder" aria-hidden="true" />
        )}
        <svg
          className="composition-card__overlay"
          viewBox="0 0 100 100"
          preserveAspectRatio="none"
          role="img"
          aria-labelledby={`${titleId} ${descriptionId}`}
          data-testid="composition-overlay"
        >
          <title id={titleId}>Composition guides and evidence</title>
          <desc id={descriptionId}>{overlayDescription(result)}</desc>

        <g
          data-overlay-kind="thirds"
          aria-label="Four dashed rule-of-thirds guides"
          fill="none"
          stroke="currentColor"
          strokeWidth="0.45"
          strokeDasharray="2 2"
          opacity="0.65"
        >
          <line x1="33.333" y1="0" x2="33.333" y2="100" />
          <line x1="66.667" y1="0" x2="66.667" y2="100" />
          <line x1="0" y1="33.333" x2="100" y2="33.333" />
          <line x1="0" y1="66.667" x2="100" y2="66.667" />
        </g>

        {result.horizonSource !== 'none' ? (
          <g data-overlay-kind="horizon" aria-label={horizonSentence(result)}>
            <line
              x1="0"
              y1="50"
              x2="100"
              y2="50"
              transform={`rotate(${result.tiltDeg} 50 50)`}
              fill="none"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeDasharray={result.horizonConf >= 0.6 ? undefined : '5 3'}
              vectorEffect="non-scaling-stroke"
            />
          </g>
        ) : null}

        <g data-overlay-kind="reason-evidence" aria-label="Reason evidence boxes">
          {evidence.map((reason, index) => (
            <rect
              key={`${reason.code}-${index}`}
              {...svgBox(reason.evidence)}
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
              strokeDasharray="4 2"
              vectorEffect="non-scaling-stroke"
              data-reason-code={reason.code}
            >
              <title>{reason.text}</title>
            </rect>
          ))}
        </g>

        <g data-overlay-kind="joint-cuts" aria-label="Body-cut boxes">
          {result.jointCuts.map((cut, index) => (
            <rect
              key={`${cut.joint}-${cut.edge}-${index}`}
              {...svgBox(cut.area)}
              fill="none"
              stroke="currentColor"
              strokeWidth={cut.flagged ? 1.8 : 0.8}
              strokeDasharray={cut.atJoint ? '1 1' : '6 2'}
              vectorEffect="non-scaling-stroke"
              data-flagged={cut.flagged ? 'yes' : 'no'}
            >
              <title>{jointCutSentence(cut)}</title>
            </rect>
          ))}
        </g>

        <g data-overlay-kind="edge-intrusions" aria-label="Edge intrusion boxes">
          {result.edgeIntrusions.map((box, index) => (
            <rect
              key={`intrusion-${index}`}
              {...svgBox(box)}
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeDasharray="7 2"
              vectorEffect="non-scaling-stroke"
            >
              <title>Object entering at the frame edge</title>
            </rect>
          ))}
        </g>

        <g data-overlay-kind="bright-blobs" aria-label="Bright distraction boxes">
          {result.brightBlobs.map((box, index) => (
            <rect
              key={`bright-${index}`}
              {...svgBox(box)}
              rx="2"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeDasharray="2 4"
              vectorEffect="non-scaling-stroke"
            >
              <title>Bright background distraction</title>
            </rect>
          ))}
        </g>

          {result.cropSuggestionHint ? (
            <g data-overlay-kind="crop-hint" aria-label={cropHintSentence(result)}>
              <rect
                {...svgBox(result.cropSuggestionHint.region)}
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeDasharray="8 3 2 3"
                vectorEffect="non-scaling-stroke"
                data-actionable={result.cropSuggestionHint.actionable ? 'yes' : 'no'}
              >
                <title>{cropHintSentence(result)}</title>
              </rect>
            </g>
          ) : null}
        </svg>
      </div>

      <figcaption>
        <ul className="composition-card__legend" aria-label="Overlay key">
          <li data-pattern="short-dash">Short dashes: rule-of-thirds guides</li>
          <li data-pattern="solid">Solid line: measured horizon</li>
          <li data-pattern="medium-dash">Medium dashes: reason evidence</li>
          <li data-pattern="cut">Tight or long dashes: body and edge violations</li>
          <li data-pattern="crop-hint">Dash-dot outline: crop hint, not an applied crop</li>
        </ul>
      </figcaption>
    </figure>
  );
}

/** What can be injected by a parent or a test. */
export type CompositionCardProps = {
  photoId: string;
  /** `undefined` fetches, `null` is explicitly unanalysed. */
  result?: CompositionDto | null;
  fetchComposition?: (photoId: string) => Promise<CompositionDto | null>;
  dismissCompositionFlag?: (input: { photoId: string; flag: string }) => Promise<CompositionDto>;
};

function stateFor(result: CompositionDto | null): LoadState {
  return result === null ? { kind: 'empty' } : { kind: 'ready', value: result };
}

/** Per-image composition readout with an accessible graphical overlay. */
export function CompositionCard({
  photoId,
  result,
  fetchComposition = compositionApi.imageComposition,
  dismissCompositionFlag = compositionApi.dismissCompositionFlag,
}: CompositionCardProps): JSX.Element {
  const preview = useThumbnails((thumbnailState) => thumbnailState.entries.get(photoId));
  const [state, setState] = useState<LoadState>(() =>
    result === undefined ? { kind: 'loading' } : stateFor(result),
  );
  const [dismissing, setDismissing] = useState<string | null>(null);
  const activePhoto = useRef(photoId);

  useEffect(() => {
    activePhoto.current = photoId;
    let active = true;
    if (result !== undefined) {
      setState(stateFor(result));
      return () => {
        active = false;
      };
    }

    setState({ kind: 'loading' });
    void fetchComposition(photoId)
      .then((next) => {
        if (active) {
          setState(stateFor(next));
        }
      })
      .catch((raised: unknown) => {
        if (!active) {
          return;
        }
        const error = asIpcError(raised);
        setState(
          CANCEL_CODES.has(error.code)
            ? { kind: 'cancelled' }
            : { kind: 'error', message: error.message },
        );
      });

    // Tauri invokes cannot be aborted in flight. Ignoring a settled request after
    // cleanup is its cancellation-equivalent: a slow response for photograph A may
    // never replace the result already shown for photograph B.
    return () => {
      active = false;
    };
  }, [fetchComposition, photoId, result]);

  const dismiss = async (flag: string): Promise<void> => {
    const requestedPhoto = photoId;
    setDismissing(flag);
    try {
      const next = await dismissCompositionFlag({ photoId: requestedPhoto, flag });
      if (activePhoto.current === requestedPhoto) {
        setState({ kind: 'ready', value: next });
      }
    } catch (raised) {
      if (activePhoto.current === requestedPhoto) {
        const error = asIpcError(raised);
        setState(
          CANCEL_CODES.has(error.code)
            ? { kind: 'cancelled' }
            : { kind: 'error', message: error.message },
        );
      }
    } finally {
      if (activePhoto.current === requestedPhoto) {
        setDismissing(null);
      }
    }
  };

  if (state.kind === 'loading') {
    return (
      <section
        className="composition-card composition-card--loading"
        aria-label="composition"
        aria-busy="true"
      >
        <p role="status">Reading this photograph&rsquo;s composition&hellip;</p>
      </section>
    );
  }

  if (state.kind === 'error') {
    return (
      <section className="composition-card composition-card--error" aria-label="composition">
        <p role="alert">{state.message}</p>
      </section>
    );
  }

  if (state.kind === 'cancelled') {
    return (
      <section className="composition-card composition-card--cancelled" aria-label="composition">
        <p role="status">{cancelledSentence()}</p>
      </section>
    );
  }

  if (state.kind === 'empty') {
    return (
      <section className="composition-card composition-card--empty" aria-label="composition">
        <p>{notAnalysedSentence()}</p>
      </section>
    );
  }

  const composition = state.value;
  return (
    <section
      className="composition-card"
      aria-label="composition"
      data-photo-id={composition.photoId}
    >
      <header className="composition-card__header">
        <h2>{headline(composition)}</h2>
        <p className="composition-card__score">
          Composition {percent(composition.compositionScore)}; {aestheticSentence(composition)}.
        </p>
        <p className="composition-card__confidence">{confidenceSentence(composition)}</p>
      </header>

      <CompositionOverlay result={composition} preview={preview} />

      <dl className="composition-card__measures">
        <dt>Horizon</dt>
        <dd>{horizonSentence(composition)}</dd>
        <dt>Headroom</dt>
        <dd>{headroomSentence(composition)}</dd>
        <dt>Placement and balance</dt>
        <dd>{placementSentence(composition)}</dd>
        <dt>Negative space</dt>
        <dd>{percent(composition.negativeSpace)} of the frame.</dd>
        <dt>Background</dt>
        <dd>{backgroundSentence(composition)}</dd>
        <dt>Crop suggestion</dt>
        <dd>{cropHintSentence(composition)}</dd>
      </dl>

      {composition.jointCuts.length > 0 ? (
        <section className="composition-card__cuts" aria-label="body crop audit">
          <h3>Body crop audit</h3>
          <ul>
            {composition.jointCuts.map((cut, index) => (
              <li
                key={`${cut.joint}-${cut.edge}-${index}`}
                data-flagged={cut.flagged ? 'yes' : 'no'}
              >
                <strong>{cut.flagged ? 'Violation: ' : 'Below flag threshold: '}</strong>
                {jointCutSentence(cut)}
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <section className="composition-card__reasons" aria-label="exact composition explanations">
        <h3>Why AURA read it this way</h3>
        <ul>
          {orderedReasons(composition).map((reason, index) => (
            <li
              key={`${reason.code}-${index}`}
              data-code={reason.code}
              data-reason-kind={reason.exoneration ? 'exoneration' : 'penalty'}
            >
              <strong>
                {reason.exoneration ? 'Context — no penalty: ' : 'Counts against the score: '}
              </strong>
              <span>{reason.text}</span>
              {reason.evidence ? (
                <span className="composition-card__evidence-note">
                  {' '}
                  Evidence box shown on the overlay.
                </span>
              ) : null}
            </li>
          ))}
        </ul>
      </section>

      {composition.flags.length > 0 ? (
        <section className="composition-card__marks" aria-label="composition marks">
          <h3>Marks</h3>
          <ul>
            {composition.flags.map((flag) => {
              const canDismiss = dismissible(flag);
              return (
                <li key={flag} data-mark-kind={canDismiss ? 'violation' : 'context'}>
                  <span>
                    {canDismiss ? 'Violation' : 'Context, not a fault'}: {flagLabel(flag)}
                  </span>
                  {canDismiss ? (
                    <button
                      type="button"
                      disabled={dismissing !== null}
                      onClick={() => void dismiss(flag)}
                    >
                      {dismissing === flag ? 'Removing mark…' : `“${flagLabel(flag)}” is wrong`}
                    </button>
                  ) : null}
                </li>
              );
            })}
          </ul>
          <p>
            Removing a mark remembers that you disagreed. It does not crop, straighten, keep or
            reject this photograph.
          </p>
        </section>
      ) : null}

      <footer className="composition-card__footer">
        <p>
          Judged as <strong>{composition.scene}</strong>; within-moment composition{' '}
          {percent(composition.relativeComposition)}; {composition.keypointSubjects} keypoint-aware{' '}
          {composition.keypointSubjects === 1 ? 'subject' : 'subjects'}.
        </p>
        <p>
          heads {composition.modelVer} · analysis {composition.analysisVer} · rules{' '}
          {composition.rulesVer}
          {composition.userReviewed ? ' · photographer reviewed' : ''}
        </p>
      </footer>
    </section>
  );
}
