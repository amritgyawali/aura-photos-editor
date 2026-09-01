//! The two set-level stages: camera matching and gallery consistency.
//!
//! Both are `Gallery` scope, so both checkpoint per stage: a solve over a whole wedding has no
//! half-finished state a resume could continue from, and a checkpoint claiming 400 of 1 units done
//! would be a progress bar lying about a solver.
//!
//! ## Why camera matching runs before consistency
//!
//! Phase 25 builds a tree of lighting nodes and normalises every frame toward its node's anchors.
//! Phase 26 removes what a *camera body* does to colour. If they ran the other way round, the
//! second shooter's Canon would be normalised into a Sony's node as though its warmer rendering
//! were a lighting change - and the node's anchors would then be a blend of two sensors, which is
//! a target neither camera can reach.
//!
//! Phase 26's own contract says it: match appearance, never parameters. The correction that
//! belongs to the device is removed first, and what is left is the room.

use crate::contract::autopilot::{CheckpointKind, ResourceNeeds, StageDecl, StageId, StageScope};

/// Phase 26. Two bodies, one visual result.
///
/// Depends on tone because the solver holds phase 15's skin locus as a hard constraint, and on the
/// cull because a fingerprint measured over rejected frames is a fingerprint of photographs nobody
/// is delivering.
pub const CAMERA_MATCH: StageDecl = StageDecl {
    id: StageId::CameraMatch,
    name: "camera_match",
    depends_on: &[StageId::Cull, StageId::Tone, StageId::Colour],
    scope: StageScope::Gallery,
    checkpoint: CheckpointKind::PerStage,
    optional: true,
    // Two fingerprints, a bounded pair search truncated at `MAX_PAIRS_PER_CAMERA`, and a
    // coordinate descent over seven identified parameters with a held-out split.
    est_ms_per_item: 12_000,
    resources: ResourceNeeds::cpu(1024, 4),
};

/// Phase 25. One wedding as one body of work.
///
/// Runs after every per-frame editing stage, because it is a residual from what those stages
/// decided and phase 25's own rule is that the layer underneath it is immutable with respect to
/// it: `normalise::solve` reads phases 15 and 16 and never reads its own output, which is what
/// makes running the pass twice a no-op.
pub const CONSISTENCY: StageDecl = StageDecl {
    id: StageId::Consistency,
    name: "consistency",
    depends_on: &[
        StageId::CameraMatch,
        StageId::Colour,
        StageId::Style,
        StageId::Story,
    ],
    scope: StageScope::Gallery,
    checkpoint: CheckpointKind::PerStage,
    optional: true,
    // The node tree, the change-point detector, the anchor ranking, the damped-then-bounded solve
    // and the outlier report, over a whole wedding.
    est_ms_per_item: 18_000,
    resources: ResourceNeeds::cpu(1536, 4),
};
