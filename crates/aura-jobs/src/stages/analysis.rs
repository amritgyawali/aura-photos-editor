//! The eight measuring stages: ingest through composition.
//!
//! None of these decides anything about a photograph, so none of them has a decision kind and none
//! of them consults the autonomy gate. Phase 13's rule - analysis is not a decision - as a
//! scheduling fact.
//!
//! Three of the eight are mandatory. `Ingest` because there is no wedding without it, `Previews`
//! because every later stage reads a proxy rather than a RAW, and `Embed` because phase 05's
//! vector is underneath the grouping, the story, the consistency tree and the cull. A wedding can
//! be delivered with no face detection and no emotion reading; it cannot be delivered with no
//! photographs in the catalog.

use crate::contract::autopilot::{CheckpointKind, ResourceNeeds, StageDecl, StageId, StageScope};

/// Phase 01. Discover, hash, read EXIF, journal.
///
/// The only stage with no dependencies, and the only one whose unit count is not known from the
/// catalog before it starts: the walker discovers files. `unit_count` returns the project's
/// current photograph count, which is zero on a first run - and a zero that means "nothing to work
/// on" is exactly what `SkipCause::NoInput` is for, so the runner reports the *file* count it is
/// about to walk rather than the photograph count it has.
pub const INGEST: StageDecl = StageDecl {
    id: StageId::Ingest,
    name: "ingest",
    depends_on: &[],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerImage,
    optional: false,
    // Hash plus EXIF plus a catalog row, on a 30 MB RAW off a fast card reader.
    est_ms_per_item: 45,
    resources: ResourceNeeds::cpu(512, 4),
};

/// Phase 02. Embedded previews and 2048 px proxies.
///
/// `PerBatch` rather than `PerImage` because the decode path is parallel over output rows and the
/// cache writes are content-addressed: a batch is what the scheduler hands the pool, and a
/// checkpoint per image would commit a transaction for every proxy on a four-thousand-frame
/// wedding.
pub const PREVIEWS: StageDecl = StageDecl {
    id: StageId::Previews,
    name: "previews",
    depends_on: &[StageId::Ingest],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerBatch,
    optional: false,
    // A tier-2 decode to 2048 px. The dominant cost of the whole analysis half.
    est_ms_per_item: 380,
    resources: ResourceNeeds::cpu(2048, 8),
};

/// Phase 05. One embedding and five descriptors from one decode.
pub const EMBED: StageDecl = StageDecl {
    id: StageId::Embed,
    name: "embed",
    depends_on: &[StageId::Previews],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerBatch,
    optional: false,
    est_ms_per_item: 60,
    resources: ResourceNeeds::accelerated(1024, 1024, 4),
};

/// Phase 06. Detection, alignment, clustering, roles.
///
/// Optional, and it is the most consequential `true` in this table. A wedding whose face detector
/// could not start still culls, grades and delivers - it does it worse, because seven of phase
/// 10's nine ranker features come from faces and phase 20 has nobody to retouch - and the run
/// summary says so rather than the gallery quietly being about nothing.
pub const FACES: StageDecl = StageDecl {
    id: StageId::Faces,
    name: "faces",
    depends_on: &[StageId::Previews],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerBatch,
    optional: true,
    est_ms_per_item: 140,
    resources: ResourceNeeds::accelerated(1536, 1024, 4),
};

/// Phase 07. Scenes, chapters, rites.
///
/// Depends on faces as well as embeddings, and that is not an accident of convenience: phase 07
/// half-closed phase 06's condition C3 by feeding scene labels back into the co-occurrence graph,
/// and running the story before the faces would leave `RoleOutcome::scene_starved` true on a
/// wedding that had every input it needed.
pub const STORY: StageDecl = StageDecl {
    id: StageId::Story,
    name: "story",
    depends_on: &[StageId::Embed, StageId::Faces],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerBatch,
    optional: true,
    est_ms_per_item: 25,
    resources: ResourceNeeds::accelerated(768, 512, 2),
};

/// Phase 08. Moments, bursts and duplicates.
///
/// Arithmetic over phase 05's vectors and phase 01's timestamps, so it is cheap and CPU-only. It
/// depends on the story because its thresholds are scene-conditioned - invariant 7 - and a
/// grouping run before the classification would compare every frame against the neutral profile.
pub const MOMENTS: StageDecl = StageDecl {
    id: StageId::Moments,
    name: "moments",
    depends_on: &[StageId::Embed, StageId::Story],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerStage,
    optional: true,
    est_ms_per_item: 8,
    resources: ResourceNeeds::cpu(768, 4),
};

/// Phase 09. Sharpness, motion, exposure, noise, eyes.
///
/// Depends on faces for the subject regions and on the story for the scene tolerances. Phase 09's
/// own `subject_aware` number is what a run reports when the first is missing: a wedding judged on
/// frame-wide sharpness is a wedding judged on the background.
pub const INTEGRITY: StageDecl = StageDecl {
    id: StageId::Integrity,
    name: "integrity",
    depends_on: &[StageId::Previews, StageId::Faces, StageId::Story],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 95,
    resources: ResourceNeeds::accelerated(1024, 768, 4),
};

/// Phase 10. Expression, interaction, peaks, ranking.
///
/// Depends on integrity as well as faces, because phase 10 closed phase 09's condition C4 in the
/// other direction: `IntegrityPass::with_emotion` fills the tears input. The dependency here is
/// the one that runs forwards - emotion reads phase 09's stored eye states - and the backwards
/// half is a re-measure phase 09 does on its own `ANALYSIS_VER` bump rather than a stage.
pub const EMOTION: StageDecl = StageDecl {
    id: StageId::Emotion,
    name: "emotion",
    depends_on: &[StageId::Faces, StageId::Integrity, StageId::Moments],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 70,
    resources: ResourceNeeds::accelerated(1024, 768, 4),
};

/// Phase 11. Horizon, cuts, headroom, balance, aesthetics.
pub const COMPOSITION: StageDecl = StageDecl {
    id: StageId::Composition,
    name: "composition",
    depends_on: &[StageId::Previews, StageId::Faces, StageId::Story],
    scope: StageScope::AllImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 85,
    resources: ResourceNeeds::accelerated(1024, 768, 4),
};
