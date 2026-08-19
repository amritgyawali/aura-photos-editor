//! FROZEN CONTRACT. The spatial vocabulary every local decision in the rest of the product
//! speaks.
//!
//! PHASE-18 section 5 freezes [`Mask`], [`MaskKind`], [`MaskPayload`], [`MaskOp`] and
//! [`MaskService`] before any segmenter exists, and the ordering is the whole point: local
//! light sculpting (19), portrait retouch (20), micro retouch (21), restoration (22),
//! geometry (23), generative cleanup (24) and QC (27) are all written against this file, so
//! none of them can grow a dependency on *how* a region was found. A caller names a kind and
//! gets back an alpha it can multiply by.
//!
//! ## Why this file is here and not in `aura-core`
//!
//! Every judgement contract since phase 15 went into `aura-core`, and this one does not.
//! `aura-core` depends on no other workspace crate and a test asserts it; section 5 freezes
//! `fn upload_gpu(&self, mask: &Mask, level: RenderLevel) -> GpuMask`, and `RenderLevel` is
//! `aura-render`'s. The precedent is `SimilarityIndex` in `aura-index`, `RenderService` in
//! `aura-render` and `PreviewService` in `aura-preview`: a contract lives in the crate that
//! owns the kind of thing it describes. A mask is a spatial artefact tied to pixels and to
//! the renderer, not a judgement about a wedding.
//! See `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md` decision 1.
//!
//! ## The one thing to understand before reading the rest
//!
//! **A mask is evidence about a region; the phase that edits owns the edit.** Nothing in this
//! crate moves a pixel, and there is no field, method or command anywhere on this surface that
//! could. That is the same rule phases 05, 08, 09, 10 and 11 established, and it is load
//! bearing here for a reason that is new: a wrong mask is *silent*. A wrong exposure looks
//! wrong; a face mask that includes the wall behind somebody's ear looks fine until phase 20
//! brightens it. [`Mask::allowance`] exists so the region can say how much may be done with
//! it, and phases 19 to 24 multiply their own strength by it.
//!
//! ## The second thing: two numbers, because they fail independently
//!
//! [`Mask::confidence`] is how sure the class assignment is and [`Mask::edge_quality`] is how
//! well determined the boundary is. A face over a crowd can be confidently the right class
//! with a badly determined boundary; a dark suit against a dark background can have a crisp
//! boundary and no confidence about which side of it is clothing. Collapsing them into one
//! number loses which of the two a photographer is looking at. ADR-0037 decision 6.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;

use serde::{Deserialize, Serialize};

use aura_core::contract::error::{AuraError, AuraResult};
use aura_core::contract::ids::{IdentityId, MaskId};
use aura_render::contract::render::RenderLevel;

/// The identity of a photograph a mask belongs to.
///
/// PHASE-18 names this `ImageId`; the catalog calls it a `PhotoId`. One type, aliased rather
/// than duplicated, so no conversion can ever disagree.
pub type ImageId = aura_core::PhotoId;

// ---------------------------------------------------------------------------
// The numbers section 10.1 is measured against
// ---------------------------------------------------------------------------

/// The mean intersection-over-union a face or skin mask must reach.
///
/// The headline KPI's own number. Above it a mask boundary sits inside the width of the
/// feather a photographer would have drawn by hand anyway.
pub const FACE_SKIN_MIOU: f32 = 0.92;

/// The mean intersection-over-union a hair mask must reach.
///
/// Lower than [`FACE_SKIN_MIOU`] and deliberately so: the boundary of hair is a matter of
/// degree rather than a curve, which is why hair is one of the four classes stored as alpha
/// and refined by matting rather than as a run-length bitmap.
pub const HAIR_MIOU: f32 = 0.88;

/// The mean intersection-over-union a subject mask must reach.
pub const SUBJECT_MIOU: f32 = 0.90;

/// Every class of one photograph, in bytes. Section 11's third row.
///
/// One hundred and eighty kilobytes for twenty classes is nine kilobytes each, which is what
/// forces the split in [`MaskKind::stored_as`]: an all-alpha representation is 1.3 MB per
/// photograph and 1.3 GB for a thousand-image gallery, which is the failure mode section 12
/// names. The phase gate asserts this on a synthetic wedding rather than trusting it.
pub const PAYLOAD_BUDGET_BYTES: usize = 180 * 1024;

/// Below this allowance the aggressive operations are disabled outright.
///
/// Skin smoothing and generative cleanup, named in section 6.4. Everything else is scaled
/// rather than refused, because a cliff at a threshold is what silently leaves half a gallery
/// unedited. ADR-0037 decision 6.
pub const AGGRESSIVE_FLOOR: f32 = 0.45;

/// How much of a connected component must sit inside a person's box before it is theirs.
///
/// **Containment, not intersection-over-union.** IoU is the wrong measure for this question and
/// gets it wrong in the common case: somebody's face is a small ellipse and their body box is
/// most of the frame, so the IoU of the two is under a fifth even when the face is entirely
/// inside the box. What decides whether a region belongs to a person is what fraction of the
/// *region* is inside them, which is what this is.
///
/// A half. Below it the component is [`Mask::identity`] `None` rather than assigned to the
/// nearest box, which is what stops the bride's skin mask from swallowing the guest behind her.
/// ADR-0037 decision 9.
pub const ASSIGN_MIN_OVERLAP: f32 = 0.5;

/// The longest edge an overlay may cross the IPC surface at. ADR-0038 decision 1.
pub const OVERLAY_MAX_EDGE: u32 = 512;

// ---------------------------------------------------------------------------
// The vocabulary
// ---------------------------------------------------------------------------

/// What a mask is a mask *of*.
///
/// A closed set of twenty. Section 2.1 names fourteen semantic classes - skin, face, eyes,
/// sclera, iris, teeth, lips, eyebrows, hair, facial hair, clothing, dress, background, sky -
/// then adds subject/background separation and five environment masks, of which sky was
/// already in the fourteen. Fourteen plus six is twenty.
///
/// **The closed set is the point.** A kind that no later phase knows how to interpret is a
/// mask that silently does nothing, and a local exposure lift that silently does nothing is
/// the hardest class of bug to see in a delivered gallery. Phase 14's `aura_recipe::MaskKind`
/// makes the same argument about the *recipe's* eight-entry vocabulary; the two enums are
/// deliberately different, because that one names what a mask is **drawn from** (including
/// two gradients that are not semantic at all) and this one names what a region **is**.
/// [`MaskKind::recipe_slug`] is the mapping, by string, in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskKind {
    /// Every pixel of visible human skin. Seeded by faces, grown in a colour space.
    Skin,
    /// The face region proper, from phase 06's box.
    Face,
    /// Both eye regions, including the lids.
    Eyes,
    /// The whites of the eyes.
    Sclera,
    /// The coloured part of the eyes.
    Iris,
    /// Visible teeth.
    Teeth,
    /// The lips.
    Lips,
    /// The eyebrows.
    Eyebrows,
    /// Head hair, including the soft boundary matting refines.
    Hair,
    /// Beard and moustache.
    FacialHair,
    /// Worn fabric that is not a bridal dress.
    Clothing,
    /// A bridal dress or a veil.
    Dress,
    /// Everything that is not the subject.
    Background,
    /// Sky.
    Sky,
    /// The salient person or people, matted at the edge.
    Subject,
    /// Foliage, grass and flowers.
    Greenery,
    /// Water.
    Water,
    /// The floor, the ground or a stage.
    Floor,
    /// A window or another bright light source in frame.
    Window,
    /// The zone phase 16's skin guard must never move. Skin and face, dilated.
    SkinSafe,
}

/// Every kind, in a fixed order. Iteration order is a contract: it decides the order masks
/// are stored, reported and composited in, and a set-based order would make two runs of the
/// same photograph produce two payloads.
pub const ALL_KINDS: [MaskKind; 20] = [
    MaskKind::Skin,
    MaskKind::Face,
    MaskKind::Eyes,
    MaskKind::Sclera,
    MaskKind::Iris,
    MaskKind::Teeth,
    MaskKind::Lips,
    MaskKind::Eyebrows,
    MaskKind::Hair,
    MaskKind::FacialHair,
    MaskKind::Clothing,
    MaskKind::Dress,
    MaskKind::Background,
    MaskKind::Sky,
    MaskKind::Subject,
    MaskKind::Greenery,
    MaskKind::Water,
    MaskKind::Floor,
    MaskKind::Window,
    MaskKind::SkinSafe,
];

/// How a kind's payload is stored.
///
/// Declared once, on the kind, so it cannot drift per call site. ADR-0037 decision 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Storage {
    /// A run-length bitmap. The boundary is hard and the payload is tiny.
    Rle,
    /// Eight-bit alpha at a quarter of the analysis resolution. The boundary is soft.
    Alpha,
}

impl MaskKind {
    /// Stable text used on the wire, in the catalog and in telemetry. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skin => "skin",
            Self::Face => "face",
            Self::Eyes => "eyes",
            Self::Sclera => "sclera",
            Self::Iris => "iris",
            Self::Teeth => "teeth",
            Self::Lips => "lips",
            Self::Eyebrows => "eyebrows",
            Self::Hair => "hair",
            Self::FacialHair => "facial_hair",
            Self::Clothing => "clothing",
            Self::Dress => "dress",
            Self::Background => "background",
            Self::Sky => "sky",
            Self::Subject => "subject",
            Self::Greenery => "greenery",
            Self::Water => "water",
            Self::Floor => "floor",
            Self::Window => "window",
            Self::SkinSafe => "skin_safe",
        }
    }

    /// Parse the stable text. `None` for anything this build does not know.
    ///
    /// Not `FromStr`: that trait's error type would have to be a real error, and "this build
    /// does not know that class" is not a failure - it is a catalog written by a newer build,
    /// and the right answer is to skip the row rather than to refuse the photograph.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        ALL_KINDS.iter().copied().find(|k| k.as_str() == text)
    }

    /// How this kind's payload is stored.
    ///
    /// Alpha for the four whose boundary is perceptually load bearing - subject, hair, face
    /// and skin - and a run length for the rest. A caller that needs a soft edge from an RLE
    /// kind gets a hard one and [`EdgeQuality::Binary`] in the report, rather than a silent
    /// lie about how soft the boundary is.
    #[must_use]
    pub const fn stored_as(self) -> Storage {
        match self {
            Self::Subject | Self::Hair | Self::Face | Self::Skin => Storage::Alpha,
            _ => Storage::Rle,
        }
    }

    /// True when this kind belongs to a person and can therefore be identity scoped.
    #[must_use]
    pub const fn is_person(self) -> bool {
        matches!(
            self,
            Self::Skin
                | Self::Face
                | Self::Eyes
                | Self::Sclera
                | Self::Iris
                | Self::Teeth
                | Self::Lips
                | Self::Eyebrows
                | Self::Hair
                | Self::FacialHair
                | Self::Clothing
                | Self::Dress
                | Self::Subject
        )
    }

    /// The `aura_recipe::MaskKind` slug this kind resolves a recipe mask for, if any.
    ///
    /// By string rather than by type, because `aura-vision` does not depend on `aura-recipe`
    /// and should not: a recipe is a document about an edit and this crate produces regions.
    /// The mapping is here, once, so the eight-entry recipe vocabulary and the twenty-entry
    /// semantic one meet in exactly one place.
    #[must_use]
    pub const fn recipe_slug(self) -> Option<&'static str> {
        match self {
            Self::Face => Some("face"),
            Self::Subject => Some("subject"),
            Self::Background => Some("background"),
            Self::Sky => Some("sky"),
            Self::Skin => Some("skin"),
            _ => None,
        }
    }
}

impl fmt::Display for MaskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The payload
// ---------------------------------------------------------------------------

/// The stored form of a region.
///
/// Two variants and no third. A compressed run length for a hard boundary, eight-bit alpha
/// for a soft one, and nothing that could hold a photograph: there is no `Png`, no `Jpeg` and
/// no path, which is what makes "a mask store contains no imagery" a property of this enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "form")]
pub enum MaskPayload {
    /// A run-length encoded bitmap at `w` by `h`, starting with a run of zeroes.
    Rle {
        /// Bitmap width in pixels.
        w: u32,
        /// Bitmap height in pixels.
        h: u32,
        /// Alternating run lengths as unsigned LEB128, first run is `off`.
        runs: Vec<u8>,
    },
    /// An eight-bit alpha plane at `w` by `h`, row major.
    Alpha8 {
        /// Plane width in pixels.
        w: u32,
        /// Plane height in pixels.
        h: u32,
        /// `w * h` alpha bytes.
        alpha: Vec<u8>,
    },
}

impl MaskPayload {
    /// The plane's dimensions, whichever form it is in.
    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Rle { w, h, .. } | Self::Alpha8 { w, h, .. } => (*w, *h),
        }
    }

    /// The stored byte length. What the budget in [`PAYLOAD_BUDGET_BYTES`] counts.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Rle { runs, .. } => runs.len(),
            Self::Alpha8 { alpha, .. } => alpha.len(),
        }
    }

    /// Which storage form this is.
    #[must_use]
    pub const fn storage(&self) -> Storage {
        match self {
            Self::Rle { .. } => Storage::Rle,
            Self::Alpha8 { .. } => Storage::Alpha,
        }
    }

    /// True when the region is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let (w, h) = self.dimensions();
        w == 0 || h == 0 || self.byte_len() == 0
    }
}

/// How well determined a mask's boundary is, as a word rather than a number.
///
/// The number is [`Mask::edge_quality`]; this is what the panel says. Four bands, because a
/// photographer asking "why can I not smooth this" needs to know whether the answer is "the
/// veil is backlit" or "this class has no soft boundary at all".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeQuality {
    /// Matted, and the matte agreed with the guide. Hair and veils look right at 100 % zoom.
    Matted,
    /// Refined but uncertain - backlight, motion, or a low-contrast boundary.
    Soft,
    /// Stored as a run length, so the boundary is a hard curve by construction.
    Binary,
    /// The boundary could not be determined at all. The mask is still a region; it is not an
    /// edge anybody should feather against.
    Unknown,
}

impl EdgeQuality {
    /// Stable text for the wire and the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Matted => "matted",
            Self::Soft => "soft",
            Self::Binary => "binary",
            Self::Unknown => "unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a mask is the way it is.
///
/// Invariant 2: every AI decision carries a confidence and reasons. Section 5 of the phase
/// document prints a `Mask` with a confidence and an edge quality and no reasons, and this
/// field is the one addition to that shape - recorded in ADR-0037 rather than assumed, the
/// same way phase 09 amended `FaceRef`. A mask that cannot say why it is uncertain is a mask
/// whose uncertainty a photographer cannot act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskReason {
    /// Seeded from a phase 06 face box.
    SeededByFace,
    /// Grown from a seed through connected pixels of similar colour.
    ColourGrown,
    /// Derived from another mask by the algebra rather than measured.
    Derived,
    /// The boundary was refined by matting inside a trimap band.
    Matted,
    /// No face was detected, so every person-bearing class fell back to whole-frame priors.
    NoFaces,
    /// The subject and the background are too close in colour to separate confidently.
    LowContrastBoundary,
    /// The frame is soft or motion blurred, so no boundary here is well determined.
    FrameNotSharp,
    /// The region reaches the frame edge, so part of it is not in the photograph.
    ClippedByFrame,
    /// The region was too small to matte and kept its coarse boundary.
    TooSmallToMatte,
    /// More than one identity overlaps this component, so it is not scoped to any of them.
    AmbiguousIdentity,
    /// The photographer edited this mask by hand and it is not regenerated.
    UserEdited,
    /// The learned segmentation head is not trained in this build and was not consulted.
    HeadUntrained,
}

impl MaskReason {
    /// Stable text for the wire, the catalog and the ledger.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SeededByFace => "seeded_by_face",
            Self::ColourGrown => "colour_grown",
            Self::Derived => "derived",
            Self::Matted => "matted",
            Self::NoFaces => "no_faces",
            Self::LowContrastBoundary => "low_contrast_boundary",
            Self::FrameNotSharp => "frame_not_sharp",
            Self::ClippedByFrame => "clipped_by_frame",
            Self::TooSmallToMatte => "too_small_to_matte",
            Self::AmbiguousIdentity => "ambiguous_identity",
            Self::UserEdited => "user_edited",
            Self::HeadUntrained => "head_untrained",
        }
    }

    /// Parse the stable text. See [`MaskKind::parse`] for why this is not `FromStr`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        const ALL: [MaskReason; 12] = [
            MaskReason::SeededByFace,
            MaskReason::ColourGrown,
            MaskReason::Derived,
            MaskReason::Matted,
            MaskReason::NoFaces,
            MaskReason::LowContrastBoundary,
            MaskReason::FrameNotSharp,
            MaskReason::ClippedByFrame,
            MaskReason::TooSmallToMatte,
            MaskReason::AmbiguousIdentity,
            MaskReason::UserEdited,
            MaskReason::HeadUntrained,
        ];
        ALL.iter().copied().find(|r| r.as_str() == text)
    }

    /// One sentence, in the product's own voice, for the panel and the Explain surface.
    #[must_use]
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::SeededByFace => "Found from a face AURA detected in this photograph.",
            Self::ColourGrown => "Grown outwards from that seed through pixels of the same colour.",
            Self::Derived => "Worked out from another mask rather than measured directly.",
            Self::Matted => "The edge was refined so hair and fabric fade out the way they do.",
            Self::NoFaces => "No face was found here, so this covers the whole frame instead.",
            Self::LowContrastBoundary => {
                "The subject and what is behind them are close in colour, so the edge is a guess."
            }
            Self::FrameNotSharp => "This photograph is soft, so no edge in it is well defined.",
            Self::ClippedByFrame => "Part of this region runs off the edge of the photograph.",
            Self::TooSmallToMatte => "This region was too small to refine, so its edge is coarse.",
            Self::AmbiguousIdentity => {
                "More than one person overlaps here, so this is not tied to any of them."
            }
            Self::UserEdited => "You changed this mask, so AURA leaves it exactly as you left it.",
            Self::HeadUntrained => {
                "AURA's learned segmentation is not trained in this build, so this was measured."
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The mask
// ---------------------------------------------------------------------------

/// One region of one photograph.
///
/// Section 5's shape, with `reasons` added for invariant 2 and `edge` added because
/// [`Mask::edge_quality`] is a number and the panel needs the word. ADR-0037 records both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mask {
    /// This mask's id.
    pub id: MaskId,
    /// The photograph.
    pub image_id: ImageId,
    /// What the region is.
    pub kind: MaskKind,
    /// Which person it belongs to, when it belongs to one.
    ///
    /// `None` is a real answer and not a missing one: a skin component that overlaps no face
    /// box by more than [`ASSIGN_MIN_IOU`] is skin that is not scoped, which is different
    /// from skin that is scoped to whoever happens to be nearest. ADR-0037 decision 9.
    pub identity: Option<IdentityId>,
    /// The stored region.
    pub payload: MaskPayload,
    /// Edge softness applied on top of the payload, `0.0 ..= 1.0`.
    pub feather: f32,
    /// How sure the class assignment is, `0.0 ..= 1.0`.
    pub confidence: f32,
    /// How well determined the boundary is, `0.0 ..= 1.0`.
    pub edge_quality: f32,
    /// The word for [`Mask::edge_quality`].
    pub edge: EdgeQuality,
    /// Why this mask is the way it is. Never empty.
    pub reasons: Vec<MaskReason>,
    /// True when a photographer brushed it. Automation never regenerates one of these.
    pub user_edited: bool,
    /// The model set this mask was produced under.
    pub model_ver: u16,
}

impl Mask {
    /// The strength ceiling phases 19 to 24 multiply their own strength by.
    ///
    /// The **geometric** mean of the two quality numbers, so neither can rescue the other -
    /// the same fusion phase 12 uses on its four sub-scores and for the same reason. A face
    /// mask that is certainly a face with an undetermined boundary is not a mask that may
    /// carry skin smoothing, and an arithmetic mean would say it was.
    ///
    /// A hand-edited mask returns 1.0. A photographer who has drawn the region is the
    /// authority on it, and gating their own brush stroke by AURA's confidence in a mask
    /// they replaced is the product arguing with its user.
    #[must_use]
    pub fn allowance(&self) -> f32 {
        if self.user_edited {
            return 1.0;
        }
        let c = self.confidence.clamp(0.0, 1.0);
        let e = self.edge_quality.clamp(0.0, 1.0);
        (c * e).max(0.0).sqrt()
    }

    /// True when this mask may carry an aggressive operation - skin smoothing, generative
    /// cleanup. Section 6.4.
    #[must_use]
    pub fn allows_aggressive(&self) -> bool {
        self.allowance() >= AGGRESSIVE_FLOOR
    }

    /// The stored size of this mask, in bytes.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.payload.byte_len()
    }
}

// ---------------------------------------------------------------------------
// Algebra
// ---------------------------------------------------------------------------

/// One step of a mask composition.
///
/// The operand of a binary op is another mask that is already in the set being composed;
/// `Source` is what puts one there. A composition is therefore a little stack program with no
/// loops, which is what makes it total: see [`MaskService::compose`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum MaskOp {
    /// Push a stored mask onto the stack.
    Source {
        /// Which mask.
        id: MaskId,
    },
    /// Push an explicit plane onto the stack. What a brush stroke arrives as.
    Plane {
        /// The stroke, at whatever resolution the caller drew it.
        payload: MaskPayload,
        /// What the result should be called.
        kind: MaskKind,
    },
    /// Replace the top two with their union.
    Union,
    /// Replace the top two with their intersection.
    Intersect,
    /// Replace the top two with the second minus the first.
    Subtract,
    /// Invert the top.
    Invert,
    /// Feather the top by a `0.0 ..= 1.0` amount.
    Feather {
        /// How much.
        amount: f32,
    },
    /// Grow the top by a radius in analysis pixels.
    Grow {
        /// How far.
        radius: u32,
    },
    /// Shrink the top by a radius in analysis pixels.
    Shrink {
        /// How far.
        radius: u32,
    },
}

impl MaskOp {
    /// Stable text for the wire.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Source { .. } => "source",
            Self::Plane { .. } => "plane",
            Self::Union => "union",
            Self::Intersect => "intersect",
            Self::Subtract => "subtract",
            Self::Invert => "invert",
            Self::Feather { .. } => "feather",
            Self::Grow { .. } => "grow",
            Self::Shrink { .. } => "shrink",
        }
    }
}

// ---------------------------------------------------------------------------
// The resolved plane
// ---------------------------------------------------------------------------

/// A mask resolved to a render level and ready to multiply by.
///
/// Named `GpuMask` because section 5 names it that and because that is what it becomes the
/// moment a `wgpu` backend exists. **This build links none** (ADR-0029 section 4), so what
/// the field holds today is a host-side plane. The name is kept rather than corrected so the
/// signature does not move when the backend arrives, and this paragraph is here so nobody
/// reads the type and concludes there is a texture.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuMask {
    /// Which mask this came from.
    pub id: MaskId,
    /// The level it was resolved at.
    pub level: RenderLevel,
    /// Plane width.
    pub width: u32,
    /// Plane height.
    pub height: u32,
    /// `width * height` alpha values in `0.0 ..= 1.0`, row major.
    pub alpha: Vec<f32>,
    /// The strength ceiling the mask carried, copied so a consumer holding only the plane
    /// still cannot exceed it.
    pub allowance: f32,
}

impl GpuMask {
    /// The alpha at a pixel, or `0.0` outside the plane.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> f32 {
        if x >= self.width || y >= self.height {
            return 0.0;
        }
        let index = (y as usize) * (self.width as usize) + (x as usize);
        self.alpha.get(index).copied().unwrap_or(0.0)
    }
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What the mask panel's project header shows.
///
/// **Two numbers rather than a ratio.** `selected` is how many frames phase 12 kept and
/// `masked` is how many of those have masks; the denominator is *selected* frames rather than
/// every photograph, because a mask over a rejected frame is not a gap - it is a frame nobody
/// asked about. Every outline since phase 09 has used every photograph as its denominator and
/// this one does not, which is exactly why both numbers are on the wire. ADR-0037 decision 8,
/// and phase 08's rule: say what the denominator is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaskOutline {
    /// Frames phase 12 kept.
    pub selected: u64,
    /// How many of those carry masks at the current model version.
    pub masked: u64,
    /// How many masks exist in total.
    pub masks: u64,
    /// How many of those a photographer has edited.
    pub user_edited: u64,
    /// How many are below [`AGGRESSIVE_FLOOR`].
    pub low_quality: u64,
    /// Mean confidence over every mask.
    pub mean_confidence: f32,
    /// Mean edge quality over every mask.
    pub mean_edge_quality: f32,
    /// Total stored bytes.
    pub payload_bytes: u64,
    /// The model set the stored masks were produced under.
    pub model_ver: u16,
    /// The analysis version the stored masks were produced under.
    pub analysis_ver: u16,
    /// True when the learned segmentation head is trained in this build. It is not.
    pub head_trained: bool,
}

impl MaskOutline {
    /// The fraction of selected frames that carry masks, or `None` when nothing is selected.
    ///
    /// `None` rather than zero. A project where phase 12 has not run has no denominator, and
    /// a coverage of 0 % would read as a failure rather than as a question nobody has asked
    /// yet.
    #[must_use]
    pub fn coverage(&self) -> Option<f32> {
        if self.selected == 0 {
            return None;
        }
        Some((self.masked as f32) / (self.selected as f32))
    }

    /// Mean stored bytes per masked frame, which is what [`PAYLOAD_BUDGET_BYTES`] bounds.
    #[must_use]
    pub fn bytes_per_image(&self) -> f32 {
        if self.masked == 0 {
            return 0.0;
        }
        (self.payload_bytes as f32) / (self.masked as f32)
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what is where in a photograph.
///
/// Fourteenth service of its kind. The rule the thirteen before it established applies
/// unchanged and matters more here than in any of them: **no phase may keep its own
/// segmenter**. Phases 19 to 24 each edit a region, and two answers to "where is her face"
/// is a gallery in which the light sculpting and the retouching disagree about the same
/// pixels - which reads as neither, and is unfixable afterwards because nothing records which
/// answer each stage used.
pub trait MaskService: Send + Sync {
    /// Every mask stored for a photograph, in [`ALL_KINDS`] order.
    fn masks(&self, image: ImageId) -> Vec<Mask>;

    /// Produce the named kinds for a photograph if they are not already stored.
    ///
    /// Idempotent and resumable: a kind that already exists at the current model and analysis
    /// versions is returned rather than recomputed, which is what makes a killed run cost
    /// nothing on restart. A kind a photographer has edited is returned untouched.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5078` when a photograph's pixels cannot be read, `AURA-ML-5079` when the run
    /// is refused outright, `AURA-DB-3006` when the store cannot be written.
    fn ensure(&self, image: ImageId, kinds: &[MaskKind]) -> Result<Vec<Mask>, AuraError>;

    /// Run a composition and return the mask it produces.
    ///
    /// **Total, deliberately.** Section 5 prints this returning a `Mask` rather than a
    /// `Result`, and rather than amending the signature the implementation is made total: an
    /// empty program, an operand that is not stored, and an underflowed stack all produce the
    /// *empty mask* - zero everywhere, confidence zero, [`EdgeQuality::Unknown`] - which is
    /// the identity for union and the annihilator for intersection. A composition that
    /// silently produced a full-frame mask instead would be an edit applied to the whole
    /// photograph, which is the one outcome worse than no edit at all.
    fn compose(&self, ops: &[MaskOp]) -> Mask;

    /// Resolve a mask to a render level.
    fn upload_gpu(&self, mask: &Mask, level: RenderLevel) -> GpuMask;

    /// What the panel's header shows for a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be read.
    fn outline(&self, project: aura_core::ProjectId) -> AuraResult<MaskOutline>;
}
