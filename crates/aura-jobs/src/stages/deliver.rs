//! The last three stages: QC, curation and export.
//!
//! All three are built. Two of them were not when this file was written, and they were declared
//! anyway - which is phase 27's rule applied to a whole stage: a DAG that omitted the stages this
//! build could not run would be a DAG whose run summary said a wedding was finished, when what
//! actually happened is that nobody wrote a file.
//!
//! That prediction held exactly. Phase 29 and phase 30 each changed one `availability` answer in
//! `aura-app` and changed nothing here, and `AppRunner::availability` is now empty. What survives
//! is the machinery it was built for: `SkipCause` is still what a stage that cannot run reports,
//! `RunStatus::CompletedDegraded` is still what a run carrying one ends as, and
//! `RunSummary::degraded_stages` still names it - and export uses all three, on a wedding nobody
//! has set an export up for.

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
/// Built since phase 29. Section 2.2 of the phase document puts curation outputs in phase 29 and
/// says this phase runs it "as an optional stage", and the arm in `AppRunner::run_stage` is one
/// call into `curate_project` - the shape section 4 asks for.
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
/// Built since phase 30, which closes phase 28's condition C7: every stage in this DAG now exists,
/// and a completed run writes files. The terminal stage of section 3's DAG, and the only one whose
/// failure a photographer would describe as the product not working.
///
/// **It is the one stage that can decline on the wedding rather than on the release.** An export
/// needs a destination, a naming template, a size and a quality, and all four are decisions a
/// photographer makes about this client in the export panel. A run repeats the export this wedding
/// has already been given, over whatever is selected now; a wedding that has never been exported
/// skips with `SkipCause::NoInput` and the run finishes degraded with this stage named. The
/// autopilot inventing a folder would be the scheduler making a decision, which is the one thing
/// `crates/aura-jobs/tests/no_decisions.rs` exists to prevent.
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
