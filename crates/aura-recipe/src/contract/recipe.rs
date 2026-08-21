//! FROZEN CONTRACT. The edit recipe, schema v1.
//!
//! PHASE-14 section 5 prints this document as JSON and calls it "frozen; every AI phase
//! writes this". Every field below either appears in that JSON or is named in
//! `docs/adr/ADR-0029-render-pipeline.md`. Changing anything here requires an ADR and a
//! re-lock of `contracts.lock`.
//!
//! # The three rules encoded in the types rather than in prose
//!
//! * **A recipe describes an edit; it does not perform one.** There is no path in this
//!   module, no file handle, and no way to express "write to disk". Invariant 1 - never
//!   mutate a RAW - is a property of a shape that cannot name a destination.
//! * **Provenance is not optional.** [`Provenance`] carries `confidence`, `source` and
//!   `decision_id`, so a recipe that came from an automated pass can always be traced back
//!   to the ledger row that produced it. Invariant 2.
//! * **A field a human touched is listed by path.** [`Provenance::user_edited_fields`] is
//!   the enforcement point of section 6.4, and `aura_recipe::schema::merge` is the only
//!   function that may add to it or read it. See ADR-0029 section 6.
//!
//! # Ranges
//!
//! Every numeric parameter states its range in its doc comment and is clamped - never
//! rejected - by [`Recipe::clamped`]. A slider that arrives out of range is a caller bug,
//! and refusing to render a wedding because one number was 101 instead of 100 is the wrong
//! trade. What *is* refused, with `AURA-RENDER-8002`, is a recipe whose shape is wrong: a
//! curve with two points, a crop with a negative width, a mask with no id.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The schema version this build writes. Bumping it requires a migration in
/// `aura_recipe::migrate` and an entry in that module's table.
pub const SCHEMA_VERSION: u16 = 1;

/// The engine string written into every recipe and hashed into every render.
///
/// Changing it invalidates every stored `render_hash`, which is the point: a renderer that
/// produces different pixels must not claim the old ones.
pub const ENGINE: &str = "aura-render/1.0.0";

/// The eight hue bands the HSL panel operates on, in a fixed order.
///
/// Fixed rather than a free-form map because the canonical serialisation sorts keys and a
/// typo'd band name would otherwise be silently carried through a hash unchanged.
pub const HSL_BANDS: [&str; 8] = [
    "red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta",
];

/// One complete non-destructive edit.
///
/// Serialises to exactly the document in PHASE-14 section 5. Field order in the struct is
/// irrelevant to the wire form: `aura_recipe::hash::canonical` sorts keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Schema version. `1` for everything this build writes.
    pub schema: u16,
    /// The renderer that produced this document, e.g. `aura-render/1.0.0`.
    pub engine: String,
    /// Which image this edit belongs to, and which camera profile renders it.
    pub image: ImageRef,
    /// Every parameter that applies to the whole frame.
    pub global: Global,
    /// Lens corrections.
    pub lens: Lens,
    /// Crop, rotation and perspective.
    pub geometry: Geometry,
    /// Local masks, each with its own parameter block. Phase 18 fills these.
    #[serde(default)]
    pub masks: Vec<Mask>,
    /// Retouch operations, in application order. Phases 20 and 21 fill these.
    #[serde(default)]
    pub retouch: Vec<RetouchOp>,
    /// Restoration settings. Phase 22 fills these.
    pub restoration: Restoration,
    /// Black-and-white conversion, or `None` for a colour render.
    #[serde(default)]
    pub bw: Option<Bw>,
    /// Where this edit came from and how sure it was.
    pub provenance: Provenance,
    /// Anything a newer schema wrote that this build does not understand.
    ///
    /// Preserved verbatim so that opening a project in an older build does not destroy work
    /// done in a newer one. ADR-0029 section 7.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Which image an edit belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRef {
    /// BLAKE3 of the RAW's bytes, lower-case hex. Phase 01's content address.
    pub content_hash: String,
    /// EXIF camera model, e.g. `ILCE-7M4`. Chooses the calibration matrix.
    pub camera: String,
    /// Camera profile name, e.g. `adobe_standard`. Stored so renders reproduce.
    pub profile: String,
}

/// Everything that applies to the whole frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Global {
    /// Exposure in stops. `-5.0 ..= 5.0`.
    pub exposure: f32,
    /// Contrast. `-100 ..= 100`.
    pub contrast: i16,
    /// Correlated colour temperature in kelvin. `2000 ..= 50000`.
    pub temperature: u32,
    /// Green-magenta tint. `-150 ..= 150`.
    pub tint: i16,
    /// Highlight recovery. `-100 ..= 100`.
    pub highlights: i16,
    /// Shadow lift. `-100 ..= 100`.
    pub shadows: i16,
    /// White point. `-100 ..= 100`.
    pub whites: i16,
    /// Black point. `-100 ..= 100`.
    pub blacks: i16,
    /// Local contrast at a coarse radius. `-100 ..= 100`.
    pub clarity: i16,
    /// Local contrast at a fine radius. `-100 ..= 100`.
    pub texture: i16,
    /// Haze removal. `-100 ..= 100`.
    pub dehaze: i16,
    /// Saturation weighted away from already-saturated colours. `-100 ..= 100`.
    pub vibrance: i16,
    /// Flat saturation. `-100 ..= 100`.
    pub saturation: i16,
    /// The point curve.
    pub curve: Curve,
    /// Per-band hue, saturation and luminance shifts. Keys are [`HSL_BANDS`].
    #[serde(default)]
    pub hsl: BTreeMap<String, HslShift>,
    /// Capture sharpening.
    pub sharpen: Sharpen,
    /// Noise reduction.
    pub noise: Noise,
}

/// The point curve, in 0-255 input and output units.
///
/// Two rules: the first point's x is 0, the last point's x is 255, and x is strictly
/// increasing. A curve that breaks any of them is refused by [`Recipe::validate`] rather
/// than clamped, because there is no correct way to guess what a caller meant by a curve
/// that goes backwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Curve {
    /// `[input, output]` pairs, at least two, x strictly increasing.
    pub points: Vec<[u16; 2]>,
}

impl Curve {
    /// The identity curve: black to black, white to white.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            points: vec![[0, 0], [255, 255]],
        }
    }

    /// True when this curve changes nothing.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.points == [[0, 0], [255, 255]]
    }
}

/// One hue band's shift. Each is `-100 ..= 100`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HslShift {
    /// Hue rotation within the band.
    pub h: i16,
    /// Saturation within the band.
    pub s: i16,
    /// Luminance within the band.
    pub l: i16,
}

impl HslShift {
    /// True when this band is untouched.
    #[must_use]
    pub const fn is_neutral(&self) -> bool {
        self.h == 0 && self.s == 0 && self.l == 0
    }
}

/// Capture sharpening, in Adobe's four-control vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sharpen {
    /// Strength. `0 ..= 150`.
    pub amount: i16,
    /// Radius in pixels. `0.5 ..= 3.0`.
    pub radius: f32,
    /// How much fine detail is protected from haloing. `0 ..= 100`.
    pub detail: i16,
    /// Edge mask strength; higher spares flat areas. `0 ..= 100`.
    pub masking: i16,
}

impl Default for Sharpen {
    fn default() -> Self {
        Self {
            amount: 0,
            radius: 1.0,
            detail: 25,
            masking: 0,
        }
    }
}

/// Noise reduction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Noise {
    /// Luminance noise reduction. `0 ..= 100`.
    pub luminance: i16,
    /// Chroma noise reduction. `0 ..= 100`.
    pub colour: i16,
    /// Detail preserved against the luminance pass. `0 ..= 100`.
    pub detail: i16,
    /// Which model produced these numbers, e.g. `scene_aware_v1`.
    pub model: String,
}

impl Default for Noise {
    /// **Truly neutral**, unlike Lightroom's default of 25 colour noise reduction.
    ///
    /// A recipe at its defaults must change nothing: it is what "reset to original" restores
    /// and what a photograph carries before any phase has decided anything. A default that
    /// quietly applied chroma noise reduction would make a neutral render differ from the
    /// camera's own pixels, and every claim in this phase about a neutral render would be a
    /// claim about a lightly denoised one instead. Phases 15 to 17 decide the value.
    fn default() -> Self {
        Self {
            luminance: 0,
            colour: 0,
            detail: 50,
            model: "reference_v1".to_string(),
        }
    }
}

/// Lens corrections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lens {
    /// Apply the distortion model for the profile.
    pub distortion: bool,
    /// Vignette correction. `0 ..= 100`, where 100 is full correction.
    pub vignette: i16,
    /// Apply lateral chromatic aberration correction.
    pub ca: bool,
    /// The lens profile name, or `None` when the lens is unknown.
    #[serde(default)]
    pub profile: Option<String>,
}

impl Default for Lens {
    /// No corrections. A lens whose profile is unknown corrects nothing rather than
    /// guessing, which is `SkipReason::LensProfileAbsent` and phase 23's job to fill.
    fn default() -> Self {
        Self {
            distortion: false,
            vignette: 0,
            ca: false,
            profile: None,
        }
    }
}

/// Crop, rotation and perspective.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    /// Rotation in degrees, positive clockwise. `-45.0 ..= 45.0`.
    pub rotate: f32,
    /// `[left, top, right, bottom]` in normalised `0..1` image coordinates.
    ///
    /// `right > left` and `bottom > top`, both refused rather than clamped.
    pub crop: [f32; 4],
    /// Perspective correction, or `None`. Phase 23 fills this.
    #[serde(default)]
    pub perspective: Option<Perspective>,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            rotate: 0.0,
            crop: [0.0, 0.0, 1.0, 1.0],
            perspective: None,
        }
    }
}

impl Geometry {
    /// True when nothing is cropped, rotated or corrected.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.perspective.is_none()
            && self.rotate.abs() < f32::EPSILON
            && self.crop[0].abs() < f32::EPSILON
            && self.crop[1].abs() < f32::EPSILON
            && (self.crop[2] - 1.0).abs() < f32::EPSILON
            && (self.crop[3] - 1.0).abs() < f32::EPSILON
    }
}

/// Perspective correction. Phase 23 owns the values; the slot is frozen here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Perspective {
    /// Vertical keystone. `-100.0 ..= 100.0`.
    pub vertical: f32,
    /// Horizontal keystone. `-100.0 ..= 100.0`.
    pub horizontal: f32,
    /// Rotation applied after keystone, in degrees. `-10.0 ..= 10.0`.
    pub rotate: f32,
    /// Scale applied to hide the corners the correction opened. `0.5 ..= 2.0`.
    pub scale: f32,
}

/// What a mask is drawn from.
///
/// A closed set, because a mask kind that no renderer knows how to evaluate is a mask that
/// silently does nothing - and a local exposure lift that silently does nothing is the
/// hardest class of bug to see in a delivered gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskKind {
    /// A face region, resolved through `PeopleService`.
    Face,
    /// A whole-person region.
    Subject,
    /// Everything that is not the subject.
    Background,
    /// Sky.
    Sky,
    /// Skin, for retouch operations.
    Skin,
    /// A linear gradient.
    Linear,
    /// A radial gradient.
    Radial,
    /// A hand-painted brush mask, stored by phase 18 in its own table.
    Brush,
}

impl MaskKind {
    /// Stable text used on the wire and in the sidecar. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Subject => "subject",
            Self::Background => "background",
            Self::Sky => "sky",
            Self::Skin => "skin",
            Self::Linear => "linear",
            Self::Radial => "radial",
            Self::Brush => "brush",
        }
    }
}

/// One local mask and the parameters that apply inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mask {
    /// Stable within a recipe, e.g. `m1`. Referenced by [`RetouchOp::mask`].
    pub id: String,
    /// What the mask is drawn from.
    pub kind: MaskKind,
    /// What it resolves against, e.g. `identity:bride`. Interpretation is the mask
    /// generator's, not the renderer's.
    #[serde(default)]
    pub target: Option<String>,
    /// The id of another mask this one is the complement of.
    #[serde(default)]
    pub invert_of: Option<String>,
    /// Edge softness. `0.0 ..= 1.0`.
    pub feather: f32,
    /// The parameters that apply inside the mask. Only the fields that are present are
    /// applied, which is why this is a sparse block rather than a whole [`Global`].
    pub params: MaskParams,
}

/// The sparse parameter block a mask carries.
///
/// Every field is `Option` and absent means "not touched here", not "zero". A `Some(0.0)`
/// exposure inside a mask is a deliberate instruction to hold that region at zero while the
/// global exposure moves; `None` lets the global through. Those are different edits.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MaskParams {
    /// Exposure in stops, inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure: Option<f32>,
    /// Contrast inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contrast: Option<i16>,
    /// Highlights inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlights: Option<i16>,
    /// Shadows inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadows: Option<i16>,
    /// Whites inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whites: Option<i16>,
    /// Blacks inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blacks: Option<i16>,
    /// Clarity inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clarity: Option<i16>,
    /// Texture inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<i16>,
    /// Saturation inside the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saturation: Option<i16>,
    /// A temperature offset in kelvin, relative to the global setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<i32>,
    /// A tint offset, relative to the global setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<i16>,
}

impl MaskParams {
    /// True when this mask changes nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.exposure.is_none()
            && self.contrast.is_none()
            && self.highlights.is_none()
            && self.shadows.is_none()
            && self.whites.is_none()
            && self.blacks.is_none()
            && self.clarity.is_none()
            && self.texture.is_none()
            && self.saturation.is_none()
            && self.temperature.is_none()
            && self.tint.is_none()
    }
}

/// One retouch operation. Phases 20 to 22 add operators; the shape is frozen here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetouchOp {
    /// The operator name, e.g. `skin_smooth`.
    pub op: String,
    /// How strongly it is applied. `0.0 ..= 1.0`.
    pub strength: f32,
    /// How much original texture is protected. `0.0 ..= 1.0`.
    #[serde(default)]
    pub protect_texture: f32,
    /// The mask id or mask kind it applies within.
    #[serde(default)]
    pub mask: Option<String>,
    /// The photograph these pixels were borrowed from, for a cross-frame repair.
    ///
    /// **Added by PHASE-21; see ADR-0044 section 3.** Optional and absent on every operation any
    /// earlier build wrote, so the change is additive in both directions: an older reader ignores
    /// it and a newer reader defaults it to `None`.
    ///
    /// It is in the *recipe* rather than only in the catalog because phase 14's rule is that a
    /// delivered file can be re-created from the RAW hash, the canonical recipe, the engine string
    /// and the output spec. A composite whose source appears in none of those four is a delivered
    /// file that cannot be re-created and, more to the point, cannot be audited. It is also the
    /// third of the five places `docs/retouch-ethics.md` section 5 promises a borrow is disclosed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borrowed_from: Option<String>,
}

/// Restoration settings. Phase 22 owns the values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restoration {
    /// `auto`, `off`, or a named model.
    pub denoise: String,
    /// Face detail recovery. `0 ..= 100`, stored as an integer per-cent.
    pub face_recovery: i16,
    /// Deblur strength. `0 ..= 100`.
    pub deblur: i16,
}

impl Default for Restoration {
    fn default() -> Self {
        Self {
            denoise: "off".to_string(),
            face_recovery: 0,
            deblur: 0,
        }
    }
}

/// Black-and-white conversion.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Bw {
    /// Per-band luminance weights, keys from [`HSL_BANDS`], each `-100 ..= 100`.
    #[serde(default)]
    pub mix: BTreeMap<String, i16>,
    /// A named film-style grade applied after the mix, or `None`.
    #[serde(default)]
    pub grade: Option<String>,
}

/// Who made this edit.
///
/// Not a boolean, because "the product did it", "the product did it and a person accepted
/// it" and "a person did it" are three different answers to a support question, and phase
/// 27 needs to tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditSource {
    /// An automated pass wrote it. Phases 15 to 26.
    Ai,
    /// A person wrote it.
    User,
    /// The QC agent wrote it. Phase 27.
    Qc,
    /// A style profile was applied wholesale. Phase 17.
    Preset,
    /// Nothing has been decided yet; this is the camera's own starting point.
    Default,
}

impl EditSource {
    /// Stable text used on the wire, in the sidecar and in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::User => "user",
            Self::Qc => "qc",
            Self::Preset => "preset",
            Self::Default => "default",
        }
    }

    /// True when an automated pass produced this and section 6.4's protection applies.
    ///
    /// `Preset` is automated: applying a style profile is something the product did, and a
    /// preset that silently overwrote a slider a photographer set would be the same bug
    /// with a friendlier name.
    #[must_use]
    pub const fn is_automated(self) -> bool {
        matches!(self, Self::Ai | Self::Qc | Self::Preset)
    }
}

/// Where this edit came from and how sure it was.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// The scene this edit was decided for, e.g. `indoor_ceremony`.
    #[serde(default)]
    pub scene: Option<String>,
    /// The style profile applied, e.g. `amrit_v3`. Phase 17 fills this.
    #[serde(default)]
    pub style_profile: Option<String>,
    /// Calibrated confidence, `0.0 ..= 1.0`. Invariant 2.
    pub confidence: f32,
    /// The ledger row that recorded this decision, or `None` for a user edit.
    #[serde(default)]
    pub decision_id: Option<String>,
    /// Who made it.
    pub source: EditSource,
    /// Dotted paths a person has touched, sorted, e.g. `global.exposure`.
    ///
    /// **The enforcement point of section 6.4.** `aura_recipe::schema::merge` is the only
    /// function permitted to add to this list, and no automated pass may write a path that
    /// appears in it. ADR-0029 section 6.
    #[serde(default)]
    pub user_edited_fields: Vec<String>,
}

impl Default for Provenance {
    fn default() -> Self {
        Self {
            scene: None,
            style_profile: None,
            confidence: 1.0,
            decision_id: None,
            source: EditSource::Default,
            user_edited_fields: Vec::new(),
        }
    }
}
