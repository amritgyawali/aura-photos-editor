//! The nine per-frame editing stages, and the mask stage that feeds six of them.
//!
//! Every one of them is `SelectedImages`, which is invariant 3's third tier: expensive work only
//! on survivors. Every one of them is optional, and that is the honest shape rather than a
//! generous one - a wedding delivered with no micro-retouch is a wedding, and a wedding whose
//! retouch stage failed at 90 % of a two-hour run must not be a wedding that failed.
//!
//! ## The order is phase 22's argument, not this phase's
//!
//! `Restoration` runs before `Geometry` because phase 22 sharpens and phase 23 must not sharpen
//! again; `Cleanup` runs after `Geometry` because phase 23's rule is that the corners it opens are
//! phase 24's to fill and phase 24 must not widen the crop to hide them. Neither of those is a
//! scheduling preference. They are two phases' recorded decisions, and the DAG is where they stop
//! being prose.

use crate::contract::autopilot::{CheckpointKind, ResourceNeeds, StageDecl, StageId, StageScope};

/// Phase 18. Twenty classes, mattes, allowances.
///
/// First of the editing half and the only one of the ten that decides nothing - it measures where
/// things are, and phases 19 to 24 each edit a region. It therefore has no decision kind and
/// consults no gate, and it is what six later stages skip on when it is absent.
pub const MASKS: StageDecl = StageDecl {
    id: StageId::Masks,
    name: "masks",
    depends_on: &[StageId::Cull, StageId::Faces],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 320,
    resources: ResourceNeeds::accelerated(2048, 1536, 4),
};

/// Phase 15. Illuminant, skin locus, exposure.
///
/// The first stage that decides. Everything downstream of it grades on top of its values, so it
/// depends on the cull and on nothing in the editing half.
pub const TONE: StageDecl = StageDecl {
    id: StageId::Tone,
    name: "tone",
    depends_on: &[StageId::Cull, StageId::Faces, StageId::Story],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 190,
    resources: ResourceNeeds::cpu(1024, 4),
};

/// Phase 16. Curve, bands, skin guard.
pub const COLOUR: StageDecl = StageDecl {
    id: StageId::Colour,
    name: "colour",
    depends_on: &[StageId::Tone],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    // The dominant term is the skin guard, which grades this frame's own skin pixels through the
    // real renderer and re-solves until they have not moved. Phase 16's guarantee is a
    // post-condition rather than an attenuation factor, and this is what that costs.
    est_ms_per_item: 260,
    resources: ResourceNeeds::cpu(1024, 4),
};

/// Phase 17. The photographer's own residual.
///
/// After tone and colour, never before. Phase 17's rule: a style is a residual and the baseline is
/// never re-derived, so the shift moves the *solved* parameters and then every guard re-runs.
pub const STYLE: StageDecl = StageDecl {
    id: StageId::Style,
    name: "style",
    depends_on: &[StageId::Colour],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    // Three map lookups and an addition, plus the guards re-running. Phase 17's regression fits
    // its slopes and discards them, which is what makes inference nearly free.
    est_ms_per_item: 40,
    resources: ResourceNeeds::cpu(768, 4),
};

/// Phase 19. Six local operations against one perceptual allowance.
pub const LOCAL_LIGHT: StageDecl = StageDecl {
    id: StageId::LocalLight,
    name: "local_light",
    depends_on: &[StageId::Style, StageId::Masks],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 210,
    resources: ResourceNeeds::accelerated(1536, 1024, 4),
};

/// Phase 20. Skin, under-eye, evening, blemishes.
///
/// Depends on local light because phase 19 wrote the rule: phase 20 must not re-smooth what phase
/// 19 has already evened, and `idx_local_evened` is the query that tells it. It also inherits
/// phase 19's per-image allowance rather than getting its own.
pub const RETOUCH: StageDecl = StageDecl {
    id: StageId::Retouch,
    name: "retouch",
    depends_on: &[StageId::LocalLight, StageId::Masks],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    // The texture guard applies the plan through the real renderer and re-solves at three quarters
    // strength up to three times. A frame that needs all three rounds is three renders.
    est_ms_per_item: 340,
    resources: ResourceNeeds::accelerated(2048, 1536, 4),
};

/// Phase 21. Hair, teeth, eyes, clothing, glare.
pub const MICRO: StageDecl = StageDecl {
    id: StageId::Micro,
    name: "micro",
    depends_on: &[StageId::Retouch],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 280,
    resources: ResourceNeeds::accelerated(2048, 1536, 4),
};

/// Phase 22. Denoise, sharpen, face recovery.
///
/// Phase 22 occupies two of phase 14's render stages rather than the one named after it -
/// denoising is a sensor-domain operation at index 6 and face recovery is at index 19 - but it is
/// one *decision*, so it is one stage here.
pub const RESTORATION: StageDecl = StageDecl {
    id: StageId::Restoration,
    name: "restoration",
    depends_on: &[StageId::Micro, StageId::Integrity],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 300,
    resources: ResourceNeeds::accelerated(2048, 1536, 4),
};

/// Phase 23. Lens, straighten, crop, variants.
///
/// After restoration, because phase 22 sharpens and phase 23 must not sharpen again.
pub const GEOMETRY: StageDecl = StageDecl {
    id: StageId::Geometry,
    name: "geometry",
    depends_on: &[StageId::Restoration, StageId::Composition],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 150,
    resources: ResourceNeeds::cpu(1024, 4),
};

/// Phase 24. Safe generative cleanup.
///
/// Last of the per-frame stages, because phase 23's rule is that the corners it opens are this
/// phase's to fill. On this build it proposes nothing on a real photograph - the distraction head
/// is untrained, so every candidate is `Unclassified` and the safety engine refuses all of it -
/// which is the correct behaviour for a build that cannot tell a bin from a gift.
pub const CLEANUP: StageDecl = StageDecl {
    id: StageId::Cleanup,
    name: "cleanup",
    depends_on: &[StageId::Geometry, StageId::Masks],
    scope: StageScope::SelectedImages,
    checkpoint: CheckpointKind::PerImage,
    optional: true,
    est_ms_per_item: 240,
    resources: ResourceNeeds::accelerated(2048, 1536, 4),
};
