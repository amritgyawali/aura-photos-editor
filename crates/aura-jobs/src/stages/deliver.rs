//! The last three stages: QC, curation and export.
//!
//! Two of the three are not built in this release, and they are declared anyway. That is
//! deliberate and it is phase 27's rule applied to a whole stage: a DAG that omitted the stages
//! this build cannot run would be a DAG whose run summary said a wedding was finished, when what
//! actually happened is that nobody wrote a file.
//!
//! `SkipCause::PhaseNotBuilt` is what they report, `RunStatus::CompletedDegraded` is what a run
//! carrying them ends as, and `RunSummary::degraded_stages` names them. Phases 29 and 30 change
//! two `availability` answers in `aura-app` and change nothing here.

use crate::contract::autopilot::{CheckpointKind, ResourceNeeds, StageDecl, StageId, StageScope};

/// Phase 27. Ten inspections and the bounded re-edit loop.
///
/// Depends on consistency, and that dependency is the whole reason this stage is here rather than
/// earlier: phase 27's own contract says a caller that ran QC before phase 25's normalisation
/// would be inspecting frames that are about to move, and every consistency ticket it wrote would
/// be about work that had not happened yet.
///
/// `Gallery` scope because the pass is one sweep over the delivered set with a set context, and
/// `PerStage` because the re-edit loop's bound is per *ticket* rather than per frame - a resume
/// that continued a half-finished QC pass would have to reconstruct which tickets had already
/// consumed a round, and the pass is 50 ms per thousand frames, so replaying it is cheaper than
/// storing that.
pub const QC: StageDecl = StageDecl {
    id: StageId::Qc,
    name: "qc",
    depends_on: &[StageId::Consistency, StageId::Cleanup, StageId::Geometry],
    scope: StageScope::Gallery,
    checkpoint: CheckpointKind::PerStage,
    optional: true,
    // Measured at 10 ms for 200 frames in phase 27's own budget, plus the re-edit loop, whose real
    // cost is the deciding phase's re-solve rather than this stage's arithmetic.
    est_ms_per_item: 25_000,
    resources: ResourceNeeds::cpu(1024, 4),
};

/// Phase 29. Albums, heroes, black and white, social crops.
///
/// Not built in this release. Section 2.2 of the phase document puts curation outputs in phase 29
/// and says this phase runs it "as an optional stage" - so the stage exists, its dependencies are
/// declared, and `StageRunner::availability` answers `PhaseNotBuilt` until `aura-curate` does.
pub const CURATION: StageDecl = StageDecl {
    id: StageId::Curation,
    name: "curation",
    depends_on: &[StageId::Qc],
    scope: StageScope::Gallery,
    checkpoint: CheckpointKind::PerStage,
    optional: true,
    est_ms_per_item: 30_000,
    resources: ResourceNeeds::cpu(1536, 4),
};

/// Phase 30. JPEG, TIFF, XMP, and where they went.
///
/// Not built in this release. The terminal stage of section 3's DAG, and the only one whose
/// failure a photographer would describe as the product not working - which is exactly why it is
/// declared here with `PerStage` checkpointing and why `RunSummary::exported` is zero and
/// `RunSummary::output_path` is the project's own directory on this build, rather than either
/// being quietly omitted.
///
/// `PerStage` and not `PerImage`, even though it writes one file per photograph. Section 10.1
/// requires that cancellation leaves no partial exports, and the way that is guaranteed is that
/// the stage commits its checkpoint only once every file is written and renamed - phase 03's
/// verify-then-rename shape, at gallery scale. A per-image checkpoint would make a cancelled
/// export a directory half full of files a photographer might send.
pub const EXPORT: StageDecl = StageDecl {
    id: StageId::Export,
    name: "export",
    depends_on: &[StageId::Qc, StageId::Curation],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerStage,
    optional: true,
    est_ms_per_item: 850,
    resources: ResourceNeeds::accelerated(2048, 2048, 8),
};
