//! The cull. One stage, and the hinge of the whole DAG.
//!
//! Every stage above it works on `AllImages` and every stage below it works on `SelectedImages`.
//! That is what makes invariant 3's three-tier compute a property of the graph rather than a
//! convention each phase remembers: expensive work reaches survivors because the survivors are
//! what the scope resolves to, not because each editing stage checks a flag.

use crate::contract::autopilot::{CheckpointKind, ResourceNeeds, StageDecl, StageId, StageScope};

/// Phase 12. The gallery.
///
/// Mandatory. A wedding whose cull did not run is not a degraded wedding, it is four thousand
/// unsorted files - and every stage after it would then work on every frame, which is the two-hour
/// budget spent editing photographs nobody is delivering.
///
/// `Gallery` scope and `PerStage` checkpointing, because the cull is a solve over the whole
/// project: quotas are allocated across chapters, the coverage guard runs twice over the result,
/// and there is no half-finished gallery a resume could continue from. A kill at 60 % of the cull
/// replays the cull, which takes seconds.
///
/// It depends on every analyser that has an opinion, and on none of them absolutely: phase 12
/// fuses four sub-scores as a geometric mean and its own `CullOutline::coverage` is what says how
/// much of the wedding carried a verdict. A cull that ran with no emotion readings is a cull that
/// says so.
pub const CULL: StageDecl = StageDecl {
    id: StageId::Cull,
    name: "cull",
    depends_on: &[
        StageId::Integrity,
        StageId::Emotion,
        StageId::Composition,
        StageId::Moments,
        StageId::Story,
    ],
    scope: StageScope::Gallery,
    checkpoint: CheckpointKind::PerStage,
    optional: false,
    // The whole solve over a 3,000-frame wedding, not per photograph: `Gallery` scope means the
    // unit count is one, so this number is the stage's entire budget. Section 11's inherited gate
    // is "analysis + cull < 8 min", of which the cull's own share is seconds.
    est_ms_per_item: 20_000,
    resources: ResourceNeeds::cpu(1024, 4),
};
