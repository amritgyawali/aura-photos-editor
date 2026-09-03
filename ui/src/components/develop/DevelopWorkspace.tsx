import { useCallback, useEffect, useMemo, useState } from 'react';

import {
  asIpcError,
  colour as colourApi,
  develop as developApi,
  geometry as geometryApi,
  local as localApi,
  micro as microApi,
  restore as restoreApi,
  retouch as retouchApi,
  tone as toneApi,
} from '../../ipc/client';
import type {
  ColourDto,
  ColourStatusDto,
  CurvePointDto,
  GeometryPlanDto,
  GeometryStatusDto,
  HistoryDto,
  HslShiftDto,
  LocalPlanDto,
  LocalStatusDto,
  MicroMatrixDto,
  MicroPlanDto,
  MicroStatusDto,
  ProtectedFeatureDto,
  RecipeDto,
  RenderCapsDto,
  RenderDto,
  RestorePlanDto,
  RestoreStatusDto,
  RetouchPlanDto,
  RetouchStatusDto,
  ToneDto,
  ToneStatusDto,
} from '../../ipc/types';
import { BasicPanel } from './BasicPanel';
import { CurveEditor } from './CurveEditor';
import { DevelopPanel } from './DevelopPanel';
import { GeometryPanel } from './GeometryPanel';
import { HslPanel } from './HslPanel';
import { LocalPanel } from './LocalPanel';
import { MaskPanel } from './MaskPanel';
import { MicroRetouchPanel } from './MicroRetouchPanel';
import { RestorePanel } from './RestorePanel';
import { RetouchPanel } from './RetouchPanel';
import { TonePanel } from './TonePanel';

/**
 * The container that wires the eleven develop views to the commands behind them.
 *
 * Every panel under `components/develop` is pure - values and callbacks in, nothing fetched -
 * which is what makes them testable without a Tauri window, and which is also why until now
 * none of them was reachable from `main.tsx`: a pure view needs somebody to mount it.
 * `PHASE-01-30-REVIEW.md` section 6.4 counted forty-two such files. This is the container for
 * the largest group of them, and it follows the split phase 25's `GalleryPanel` established.
 *
 * ## Why one container rather than eleven
 *
 * Because the panels are eleven readings of **one photograph**, and they share three things a
 * per-panel container would have to fetch eleven times over: the recipe, the render and the
 * project's coverage. More importantly they share an invalidation - a tone override rewrites the
 * recipe, which changes the render, which is what the retouch panel's before-and-after is drawn
 * against. Eleven containers each reloading their own slice would show a photographer a frame
 * whose exposure had moved and whose retouch preview had not.
 *
 * **Everything reloads after a write.** The same rule phases 25, 26 and 27 wrote for their own
 * containers, for the same reason: a panel that patched its own state locally would drift from
 * the catalog the first time a re-solve did something it did not predict. Phase 15's skin locus
 * and phase 16's skin guard both re-solve, so that is not a hypothetical.
 *
 * ## What this container is not allowed to do
 *
 * It never computes an edit. Every number it displays came from a service and every number it
 * sends is one a person typed. Phase 14's rule - `RenderService` is the only way to turn a recipe
 * into pixels - has a front-end half, which is that no view may derive one.
 */
export type DevelopWorkspaceProps = {
  /** The open wedding, or null. */
  projectId: string | null;
  /** The selected photograph, or null. */
  photoId: string | null;
  /** Surface an error to the app's banner. The same shape every other panel uses. */
  onError: (error: { code: string; message: string } | null) => void;
};

/** The sections, in the order the render graph runs them. */
const SECTIONS = [
  { id: 'basic', title: 'Basic' },
  { id: 'tone', title: 'Tone and colour' },
  { id: 'curve', title: 'Curve' },
  { id: 'hsl', title: 'Colour bands' },
  { id: 'masks', title: 'Regions' },
  { id: 'local', title: 'Local light' },
  { id: 'retouch', title: 'Retouch' },
  { id: 'micro', title: 'Small fixes' },
  { id: 'restore', title: 'Restoration' },
  { id: 'geometry', title: 'Geometry' },
  { id: 'params', title: 'All parameters' },
] as const;

type SectionId = (typeof SECTIONS)[number]['id'];

/**
 * Where this edit came from, and how much of it a person owns, as one sentence.
 *
 * The five `RecipeDto.source` values are slugs - `ai`, `user`, `qc`, `preset`, `default` - and
 * the first version of this rendered them straight into the prose, so a photograph nobody had
 * edited read "This edit came from default". A slug in a sentence is a slug a photographer has
 * to decode, and `default` in particular says the opposite of what it means: it is the camera's
 * own starting point rather than a decision anything made.
 */
export function provenance(recipe: RecipeDto): string {
  const owned = recipe.userEditedFields.length;
  const mine =
    owned === 0
      ? 'Nothing on it has been set by hand.'
      : `${owned} of its settings ${owned === 1 ? 'is yours' : 'are yours'} and will not be overwritten.`;
  switch (recipe.source) {
    case 'ai':
      return `AURA suggested this edit. ${mine}`;
    case 'user':
      return `This edit is yours. ${mine}`;
    case 'qc':
      return `This edit was changed by a quality-control fix. ${mine}`;
    case 'preset':
      return `This edit came from a preset. ${mine}`;
    // `default` is the camera's own starting point, which is not an edit anybody made.
    default:
      return `Nothing has edited this photograph yet - this is the camera's own starting point. ${mine}`;
  }
}

function toBanner(error: unknown): { code: string; message: string } {
  const ipc = asIpcError(error);
  return {
    code: ipc?.code ?? 'AURA-RENDER-8000',
    message: ipc?.message ?? 'The develop panel could not be read.',
  };
}

export function DevelopWorkspace({
  projectId,
  photoId,
  onError,
}: DevelopWorkspaceProps): JSX.Element {
  const [open, setOpen] = useState<SectionId>('basic');
  const [comparing, setComparing] = useState(false);
  const [busy, setBusy] = useState(false);

  const [recipe, setRecipe] = useState<RecipeDto | null>(null);
  const [render, setRender] = useState<RenderDto | null>(null);
  const [history, setHistory] = useState<HistoryDto | null>(null);
  const [caps, setCaps] = useState<RenderCapsDto | null>(null);

  const [toneStatus, setToneStatus] = useState<ToneStatusDto | null>(null);
  const [tone, setTone] = useState<ToneDto | null>(null);
  const [colourStatus, setColourStatus] = useState<ColourStatusDto | null>(null);
  const [colour, setColour] = useState<ColourDto | null>(null);
  const [localStatus, setLocalStatus] = useState<LocalStatusDto | null>(null);
  const [localPlan, setLocalPlan] = useState<LocalPlanDto | null>(null);
  const [retouchStatus, setRetouchStatus] = useState<RetouchStatusDto | null>(null);
  const [retouchPlan, setRetouchPlan] = useState<RetouchPlanDto | null>(null);
  const [microStatus, setMicroStatus] = useState<MicroStatusDto | null>(null);
  const [microPlan, setMicroPlan] = useState<MicroPlanDto | null>(null);
  const [matrix, setMatrix] = useState<MicroMatrixDto | null>(null);
  const [restoreStatus, setRestoreStatus] = useState<RestoreStatusDto | null>(null);
  const [restorePlan, setRestorePlan] = useState<RestorePlanDto | null>(null);
  const [geometryStatus, setGeometryStatus] = useState<GeometryStatusDto | null>(null);
  const [geometryPlan, setGeometryPlan] = useState<GeometryPlanDto | null>(null);

  const fail = useCallback(
    (error: unknown) => {
      onError(toBanner(error));
    },
    [onError],
  );

  /** The project's coverage. Cheap, and it does not change when the selection does. */
  const reloadProject = useCallback(async () => {
    if (!projectId) {
      setToneStatus(null);
      setColourStatus(null);
      setLocalStatus(null);
      setRetouchStatus(null);
      setMicroStatus(null);
      setMatrix(null);
      setRestoreStatus(null);
      setGeometryStatus(null);
      return;
    }
    try {
      const [nextTone, nextColour, nextLocal, nextRetouch, nextMicro, nextMatrix] =
        await Promise.all([
          toneApi.toneStatus(projectId),
          colourApi.colourStatus(projectId),
          localApi.localStatus(projectId),
          retouchApi.retouchStatus(projectId),
          microApi.microStatus(projectId),
          microApi.microMatrix(projectId),
        ]);
      setToneStatus(nextTone);
      setColourStatus(nextColour);
      setLocalStatus(nextLocal);
      setRetouchStatus(nextRetouch);
      setMicroStatus(nextMicro);
      setMatrix(nextMatrix);

      const [nextRestore, nextGeometry] = await Promise.all([
        restoreApi.restoreStatus(projectId),
        geometryApi.geometryStatus(projectId),
      ]);
      setRestoreStatus(nextRestore);
      setGeometryStatus(nextGeometry);
    } catch (error) {
      fail(error);
    }
  }, [fail, projectId]);

  /**
   * Everything about the open photograph.
   *
   * The render is asked for last and at `interactive` purpose, because it is the one call that
   * costs real time - about 210 ms on the processor path, and this build links no GPU backend
   * (ADR-0029 section 4). Asking for it first would leave every panel blank while it ran.
   */
  const reloadPhoto = useCallback(async () => {
    if (!photoId) {
      setRecipe(null);
      setRender(null);
      setHistory(null);
      setTone(null);
      setColour(null);
      setLocalPlan(null);
      setRetouchPlan(null);
      setMicroPlan(null);
      setRestorePlan(null);
      setGeometryPlan(null);
      return;
    }
    try {
      const [nextRecipe, nextHistory, nextTone, nextColour] = await Promise.all([
        developApi.imageRecipe({ photoId }),
        developApi.imageHistory({ photoId }),
        toneApi.imageTone(photoId),
        colourApi.imageColour(photoId),
      ]);
      setRecipe(nextRecipe);
      setHistory(nextHistory);
      setTone(nextTone);
      setColour(nextColour);

      const [nextLocal, nextRetouch, nextMicro, nextRestore, nextGeometry] = await Promise.all([
        localApi.imageLocal(photoId),
        retouchApi.imageRetouch(photoId),
        microApi.imageMicro(photoId),
        restoreApi.imageRestore(photoId),
        geometryApi.imageGeometry(photoId),
      ]);
      setLocalPlan(nextLocal);
      setRetouchPlan(nextRetouch);
      setMicroPlan(nextMicro);
      setRestorePlan(nextRestore);
      setGeometryPlan(nextGeometry);

      setRender(
        await developApi.renderImage({ photoId, level: 'proxy2048', purpose: 'interactive' }),
      );
    } catch (error) {
      fail(error);
    }
  }, [fail, photoId]);

  useEffect(() => {
    void reloadProject();
  }, [reloadProject]);

  useEffect(() => {
    void reloadPhoto();
  }, [reloadPhoto]);

  /** One write, then a full reload. Never a local patch - see the note at the top. */
  const write = useCallback(
    async (action: () => Promise<unknown>) => {
      setBusy(true);
      try {
        await action();
        onError(null);
        await reloadPhoto();
        await reloadProject();
      } catch (error) {
        fail(error);
      } finally {
        setBusy(false);
      }
    },
    [fail, onError, reloadPhoto, reloadProject],
  );

  const caveats = useMemo(
    () => (render?.notes ?? []).filter((note) => note.isCaveat),
    [render?.notes],
  );

  if (!projectId) {
    return (
      <section className="develop-workspace">
        <p className="empty">Open a wedding to edit its photographs.</p>
      </section>
    );
  }

  if (!photoId) {
    return (
      <section className="develop-workspace">
        <p className="empty">Select a photograph to develop it.</p>
      </section>
    );
  }

  if (!recipe || !history) {
    return (
      <section className="develop-workspace" aria-busy="true">
        <p>Loading the edit…</p>
      </section>
    );
  }

  const project = projectId;
  const photo = photoId;

  return (
    <section className="develop-workspace" aria-label="Develop">
      <header className="develop-workspace__header">
        <h2>Develop</h2>
        <p className="develop-workspace__source">{provenance(recipe)}</p>
        {caveats.length > 0 ? (
          <ul className="develop-workspace__caveats">
            {caveats.map((note) => (
              <li key={`${note.stage}-${note.reason}`}>
                {note.stage}: {note.detail ?? note.reason}
              </li>
            ))}
          </ul>
        ) : null}
        {busy ? <p role="status">Applying…</p> : null}
      </header>

      <nav className="develop-workspace__tabs" aria-label="Develop sections">
        {SECTIONS.map((section) => (
          <button
            key={section.id}
            type="button"
            aria-pressed={open === section.id}
            className={open === section.id ? 'is-open' : undefined}
            onClick={() => setOpen(section.id)}
          >
            {section.title}
          </button>
        ))}
      </nav>

      <div className="develop-workspace__body">
        {open === 'basic' ? (
          <BasicPanel
            tone={tone}
            recipe={recipe}
            status={toneStatus}
            onOverride={(values) =>
              void write(() =>
                toneApi.setToneOverride({
                  projectId: project,
                  photoId: photo,
                  exposureEv: values.exposureEv ?? null,
                  temperatureK: values.temperatureK ?? null,
                  tint: values.tint ?? null,
                }),
              )
            }
            onAccept={() => void write(() => toneApi.acceptTone({ photoId: photo }))}
            onResetToAi={() =>
              void write(() =>
                developApi.historyStep({
                  projectId: project,
                  photoId: photo,
                  action: 'reset_ai',
                }),
              )
            }
          />
        ) : null}

        {open === 'tone' ? (
          <TonePanel
            colour={colour}
            recipe={recipe}
            status={colourStatus}
            onOverride={(values) =>
              void write(() =>
                colourApi.setColourOverride({
                  projectId: project,
                  photoId: photo,
                  contrast: values.contrast ?? null,
                  highlights: values.highlights ?? null,
                  shadows: values.shadows ?? null,
                  whites: values.whites ?? null,
                  blacks: values.blacks ?? null,
                  vibrance: values.vibrance ?? null,
                  saturation: values.saturation ?? null,
                }),
              )
            }
            onAccept={() => void write(() => colourApi.acceptColour({ photoId: photo }))}
            onSelectVariant={(kind) =>
              void write(() =>
                colourApi.selectColourVariant({ projectId: project, photoId: photo, kind }),
              )
            }
            onResetToAi={() =>
              void write(() =>
                developApi.historyStep({
                  projectId: project,
                  photoId: photo,
                  action: 'reset_ai',
                }),
              )
            }
          />
        ) : null}

        {open === 'curve' ? (
          <CurveEditor
            colour={colour}
            onSelectVariant={(kind) =>
              void write(() =>
                colourApi.selectColourVariant({ projectId: project, photoId: photo, kind }),
              )
            }
            onCurveChange={(points: CurvePointDto[]) =>
              void write(() =>
                colourApi.setColourOverride({
                  projectId: project,
                  photoId: photo,
                  curve: points,
                }),
              )
            }
          />
        ) : null}

        {open === 'hsl' ? (
          <HslPanel
            colour={colour}
            recipe={recipe}
            onHslChange={(bands: HslShiftDto[]) =>
              void write(() =>
                colourApi.setColourOverride({
                  projectId: project,
                  photoId: photo,
                  hsl: bands,
                }),
              )
            }
          />
        ) : null}

        {open === 'masks' ? <MaskPanel projectId={project} imageId={photo} /> : null}

        {open === 'local' ? (
          <LocalPanel
            status={localStatus}
            plan={localPlan}
            comparing={comparing}
            onCompare={setComparing}
            onSetStrength={(operation, strength) =>
              void write(() =>
                localApi.setLocalStrength({
                  projectId: project,
                  photoId: photo,
                  operation,
                  strength,
                }),
              )
            }
            onAccept={() => void write(() => localApi.acceptLocal({ photoId: photo }))}
          />
        ) : null}

        {open === 'retouch' ? (
          <RetouchPanel
            status={retouchStatus}
            plan={retouchPlan}
            comparing={comparing}
            onCompare={setComparing}
            onSetPreset={(preset) =>
              void write(() =>
                retouchApi.setRetouch({ projectId: project, photoId: photo, preset }),
              )
            }
            onSetStrength={(identityId, strength) =>
              void write(() =>
                retouchApi.setRetouch({
                  projectId: project,
                  photoId: photo,
                  identityId,
                  strength,
                }),
              )
            }
            // A protected feature is cleared, never weakened - phase 20's rule. An absolute
            // one is not offered at all, which the panel enforces by not drawing the control.
            onClearProtection={(feature: ProtectedFeatureDto) =>
              void write(() =>
                retouchApi.setProtection({
                  projectId: project,
                  identityId: feature.identityId,
                  photoId: photo,
                  kind: feature.kind,
                  area: feature.area,
                  protect: false,
                }),
              )
            }
            onAccept={() => void write(() => retouchApi.acceptRetouch({ photoId: photo }))}
          />
        ) : null}

        {open === 'micro' ? (
          <MicroRetouchPanel
            status={microStatus}
            plan={microPlan}
            matrix={matrix}
            comparing={comparing}
            onCompare={setComparing}
            // The matrix is whole-or-nothing on the wire: the panel names one operator and
            // this sends the whole vector, because a partial matrix is a matrix whose other
            // entries the backend would have to guess.
            onToggleOperation={(operator, allowed) => {
              if (!matrix) {
                return;
              }
              const index = matrix.operators.indexOf(operator);
              if (index < 0) {
                return;
              }
              const next = matrix.allowed.map((was, at) => (at === index ? allowed : was));
              void write(() => microApi.setMicroMatrix({ projectId: project, allowed: next }));
            }}
            onToggleClothing={(kind, allowed) => {
              if (!matrix) {
                return;
              }
              const index = matrix.clothingKinds.indexOf(kind);
              if (index < 0) {
                return;
              }
              const next = matrix.clothing.map((was, at) => (at === index ? allowed : was));
              void write(() => microApi.setMicroMatrix({ projectId: project, clothing: next }));
            }}
            onToggleBorrowing={(allowed) =>
              void write(() => microApi.setMicroMatrix({ projectId: project, borrowing: allowed }))
            }
            onAccept={() => void write(() => microApi.acceptMicro({ photoId: photo }))}
          />
        ) : null}

        {open === 'restore' ? (
          <RestorePanel
            status={restoreStatus}
            plan={restorePlan}
            onOverride={(input) =>
              void write(() =>
                restoreApi.setRestoreOverride({
                  photoId: photo,
                  denoise: input.denoise ?? null,
                  sharpen: input.sharpen ?? null,
                  faceRecovery: input.faceRecovery ?? null,
                }),
              )
            }
            onAccept={() => void write(() => restoreApi.acceptRestore({ photoId: photo }))}
          />
        ) : null}

        {open === 'geometry' ? (
          <GeometryPanel
            status={geometryStatus}
            plan={geometryPlan}
            comparing={comparing}
            onCompare={setComparing}
            // Choosing a variant sends that variant's own rectangle. The index is not on the
            // wire because a stored index is a reference into a list a re-plan can reorder,
            // and phase 23's rule is that a delivered frame is the rectangle the safety filter
            // passed rather than a pointer to one.
            onSelectCrop={(index) => {
              const variant = geometryPlan?.crops[index];
              if (!variant || !geometryPlan) {
                return;
              }
              void write(() =>
                geometryApi.setFraming({
                  projectId: project,
                  photoId: photo,
                  rect: variant.rect,
                  rotateDeg: geometryPlan.rotateDeg,
                  aspect: variant.aspect,
                }),
              );
            }}
            onSetFraming={(rect, rotateDeg) =>
              void write(() =>
                geometryApi.setFraming({
                  projectId: project,
                  photoId: photo,
                  rect,
                  rotateDeg,
                  aspect: geometryPlan?.crops[geometryPlan.primaryCrop]?.aspect ?? 'original',
                }),
              )
            }
            onAccept={() => void write(() => geometryApi.acceptGeometry({ photoId: photo }))}
          />
        ) : null}

        {open === 'params' && caps ? (
          <DevelopPanel
            recipe={recipe}
            render={render}
            history={history}
            caps={caps}
            onSetParam={(path, value) =>
              void write(() =>
                developApi.setParam({ projectId: project, photoId: photo, path, value }),
              )
            }
            onHistory={(action) =>
              void write(() =>
                developApi.historyStep({ projectId: project, photoId: photo, action }),
              )
            }
          />
        ) : null}

        {open === 'params' && !caps ? (
          <CapsLoader onLoaded={setCaps} onFailed={fail} />
        ) : null}
      </div>
    </section>
  );
}

/**
 * The renderer's capabilities, fetched on first use rather than on mount.
 *
 * `render_caps` probes the machine, and probing it to draw a panel nobody has opened is work
 * done for a tab that may never be looked at. It is a component rather than an effect so that
 * opening the tab is what triggers the call.
 */
function CapsLoader({
  onLoaded,
  onFailed,
}: {
  onLoaded: (caps: RenderCapsDto) => void;
  onFailed: (error: unknown) => void;
}): JSX.Element {
  useEffect(() => {
    void developApi
      .renderCaps()
      .then(onLoaded)
      .catch((error: unknown) => onFailed(error));
  }, [onFailed, onLoaded]);
  return <p aria-busy="true">Asking this machine what its renderer can do…</p>;
}
