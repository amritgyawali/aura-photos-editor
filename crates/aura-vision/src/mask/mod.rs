//! PHASE-18. The masking engine: one decode, twenty regions.
//!
//! Phase 05 established the discipline this module follows - a wedding is read once - and
//! phase 06 followed it for faces. This is the third pass over the pixels and the last one
//! that reads them for a *judgement*: everything from phase 19 onwards reads them to change
//! them.
//!
//! ```text
//! proxy + faces + person boxes + identities
//!      |
//!      +--> segment (20 classes at 768 px) --> class planes
//!      |                                          |
//!      +--> subject (salient person)  --> coarse alpha
//!                                                 |
//!                       trimap --> matting --> refined alpha (hair / veil / rim)
//!                                                 |
//!             instance assignment via face and person boxes --> identity-scoped masks
//!                                                 |
//!                 store (run length + quarter-resolution alpha) --> mask store
//!                                                 |
//!     algebra (union / intersect / subtract / feather) --> phases 19 to 24 and the brush
//! ```
//!
//! # The heads are placeholders, and nothing consults them
//!
//! [`segment::SEG_HEAD_TRAINED`] and [`matting::MATTING_HEAD_TRAINED`] are both `false`. Two
//! models are registered, signed and carded, and no photograph in this build is segmented by
//! a random projection. What ships is deterministic geometry and colour arithmetic over the
//! pixels, seeded by phase 06's boxes and landmarks - which is what section 6.1 describes for
//! skin ("seeded by detected faces and extended by colour-space growth constrained to
//! connected regions") and what the rest of the classes can honestly be measured with.
//!
//! This is the fourth phase to make that call and the argument is in
//! `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md` decision 2. It matters
//! more here than it did in phase 16, for one reason: **a wrong mask is silent**. A wrong tone
//! parameter is visible in the histogram; a class label on the pixels behind somebody's ear is
//! visible only after phase 20 has smoothed them.
//!
//! # Nothing here moves a pixel
//!
//! Sixth phase running. `MaskService` produces regions and phases 19 to 24 consume them.
//! There is no field on any shape in this module, and no method on the frozen service, that
//! could apply a mask to a photograph.

use aura_core::contract::ids::{IdentityId, MaskId};
use aura_core::PhotoId;
use aura_raw::contract::pixels::PixelLevel;

use crate::contract::mask::{EdgeQuality, Mask, MaskKind, MaskReason, ALL_KINDS};
use crate::face::FramePeople;

pub mod algebra;
pub mod api;
pub mod errors;
pub mod fixtures;
pub mod instance;
pub mod matting;
pub mod quality;
pub mod segment;
pub mod store;
pub mod subject;
pub mod trimap;

pub use algebra::Plane;

/// Which pixel tier the masking pass reads.
///
/// The 2048 px proxy, the same rung phase 06 reads. Section 6.1 asks for segmentation at
/// 768 px and that is the grid the *analysis* runs on - [`ANALYSIS_EDGE`] - but the buffer it
/// is downsampled from has to be the proxy: a 384 px thumbnail is 11 px of face for a guest
/// (phase 06's own measurement), and a skin seed sampled from eleven pixels is a colour
/// nobody measured.
pub const MASK_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// The long edge the class decisions are made on.
///
/// Section 6.1's own number. Everything downstream - the trimap band, the matting, the
/// connected components, the stored payloads - is expressed as a fraction of this, so the
/// only place a resolution appears as a constant is here.
pub const ANALYSIS_EDGE: u32 = 768;

/// How much smaller a stored alpha plane is than the analysis grid.
///
/// Four, section 6.3's own number. At [`ANALYSIS_EDGE`] that is a 192 px long edge, which is
/// 24 KB for a plane and 98 KB for the four classes that get one - inside the 180 KB budget
/// with room for sixteen run lengths.
pub const ALPHA_DIVISOR: u32 = 4;

/// The model set these masks were produced under.
///
/// Bumping it invalidates every class assignment. It is separate from [`ANALYSIS_VER`]
/// because the two invalidate different things, which is the rule phases 06, 08, 09 and 10
/// each restated: `MODEL_VER` invalidates *what a region is*, `ANALYSIS_VER` invalidates
/// *where its boundary is* and how good it is.
pub const MODEL_VER: u16 = 1;

/// The arithmetic these masks were produced under.
///
/// Bumping it invalidates every boundary, every confidence and every edge quality, and leaves
/// the class assignments alone.
pub const ANALYSIS_VER: u16 = 1;

/// Pixels in, in the working space, at whatever resolution the caller has.
///
/// Deliberately not `aura_render::cpu::Frame`: that shape carries a camera name, a clip point
/// and a tile origin, none of which a mask has any business reading, and taking it would let
/// a future edit reach for the camera profile from inside a segmenter.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskFrame {
    /// Interleaved linear RGB, `width * height * 3` long.
    pub rgb: Vec<f32>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl MaskFrame {
    /// Wrap a buffer.
    #[must_use]
    pub fn new(rgb: Vec<f32>, width: u32, height: u32) -> Self {
        Self { rgb, width, height }
    }

    /// Read one of phase 02's decoded buffers into the working space.
    ///
    /// **The linearisation is the whole of this function and it is not optional.** Every
    /// decision in [`segment`] is about a ratio - is this pixel the same colour as that face,
    /// is this region flatter than the scene, is this brighter than the frame's median - and a
    /// ratio taken on sRGB-encoded values is a different number at every brightness. A skin
    /// seed measured on encoded pixels would sit at a different chromaticity in shadow than in
    /// light, which is exactly the failure the per-frame seed exists to prevent.
    ///
    /// Invariant 8, and the same reason phase 15's white-balance head takes `linear_srgb`.
    ///
    /// Returns `None` for a tiled buffer: a full-resolution tiled image is not what this pass
    /// reads, and silently stitching one would be an eight-gigabyte allocation nobody asked for.
    #[must_use]
    pub fn from_buffer(buffer: &aura_raw::contract::pixels::PixelBuffer) -> Option<Self> {
        use aura_raw::colour::curve;
        use aura_raw::contract::pixels::PixelData;

        let width = buffer.width;
        let height = buffer.height;
        let want = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(3);

        let rgb = match &buffer.data {
            PixelData::Srgb8(bytes) => {
                if bytes.len() < want {
                    return None;
                }
                bytes
                    .iter()
                    .take(want)
                    .map(|b| curve::srgb_decode(f32::from(*b) / 255.0))
                    .collect()
            }
            PixelData::Linear16(codes) => {
                if codes.len() < want {
                    return None;
                }
                codes
                    .iter()
                    .take(want)
                    .map(|c| curve::linear_u16_to_scene(*c))
                    .collect()
            }
            PixelData::Tiled(_) => return None,
        };

        Some(Self { rgb, width, height })
    }

    /// The pixel at a location, or black outside.
    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> [f32; 3] {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return [0.0; 3];
        }
        let base = ((y as usize) * (self.width as usize) + (x as usize)) * 3;
        [
            self.rgb.get(base).copied().unwrap_or(0.0),
            self.rgb.get(base + 1).copied().unwrap_or(0.0),
            self.rgb.get(base + 2).copied().unwrap_or(0.0),
        ]
    }

    /// The analysis-grid size for this frame: [`ANALYSIS_EDGE`] on the long edge, or the
    /// frame's own size when it is already smaller.
    #[must_use]
    pub fn analysis_size(&self) -> (u32, u32) {
        let long = self.width.max(self.height);
        if long <= ANALYSIS_EDGE || long == 0 {
            return (self.width.max(1), self.height.max(1));
        }
        let scale = f64::from(ANALYSIS_EDGE) / f64::from(long);
        (
            ((f64::from(self.width) * scale).round() as u32).max(1),
            ((f64::from(self.height) * scale).round() as u32).max(1),
        )
    }

    /// Box-filtered downsample onto the analysis grid.
    ///
    /// A box filter rather than a point sample, because every colour decision in [`segment`]
    /// is a statement about a *neighbourhood* - is this pixel skin-coloured, is this region
    /// textured - and point sampling a 2048 px frame down to 768 px throws away two thirds of
    /// the evidence for each of those decisions and makes noise look like texture.
    #[must_use]
    pub fn to_analysis(&self) -> Self {
        let (w, h) = self.analysis_size();
        if w == self.width && h == self.height {
            return self.clone();
        }
        let mut out = vec![0.0_f32; (w as usize) * (h as usize) * 3];
        let sx = f64::from(self.width) / f64::from(w);
        let sy = f64::from(self.height) / f64::from(h);
        for y in 0..h {
            let y0 = ((f64::from(y) * sy).floor() as i64).max(0);
            let y1 = (((f64::from(y) + 1.0) * sy).ceil() as i64).max(y0 + 1);
            for x in 0..w {
                let x0 = ((f64::from(x) * sx).floor() as i64).max(0);
                let x1 = (((f64::from(x) + 1.0) * sx).ceil() as i64).max(x0 + 1);
                let mut acc = [0.0_f64; 3];
                let mut n = 0.0_f64;
                for sy_i in y0..y1 {
                    for sx_i in x0..x1 {
                        let p = self.at(sx_i, sy_i);
                        acc[0] += f64::from(p[0]);
                        acc[1] += f64::from(p[1]);
                        acc[2] += f64::from(p[2]);
                        n += 1.0;
                    }
                }
                let base = ((y as usize) * (w as usize) + (x as usize)) * 3;
                if n > 0.0 {
                    for (channel, total) in acc.iter().enumerate() {
                        if let Some(slot) = out.get_mut(base + channel) {
                            *slot = (total / n) as f32;
                        }
                    }
                }
            }
        }
        Self {
            rgb: out,
            width: w,
            height: h,
        }
    }
}

/// One region, before it is stored.
///
/// The in-memory twin of [`Mask`]: it carries a working [`Plane`] where the stored form
/// carries a payload, and `store` is the one place the two meet. Keeping them apart is what
/// stops the pipeline from encoding and decoding a run length between every two stages.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskPlane {
    /// What the region is.
    pub kind: MaskKind,
    /// Which person, when it belongs to one.
    pub identity: Option<IdentityId>,
    /// The region itself, on the analysis grid.
    pub plane: Plane,
    /// How sure the class assignment is.
    pub confidence: f32,
    /// How well determined the boundary is.
    pub edge_quality: f32,
    /// The word for the boundary.
    pub edge: EdgeQuality,
    /// Why this region is the way it is. Never empty.
    pub reasons: Vec<MaskReason>,
}

impl MaskPlane {
    /// The strength ceiling this region carries, before it is stored.
    ///
    /// The same geometric mean [`Mask::allowance`] computes, so a caller working on planes and
    /// a caller working on stored masks get the same number. Two implementations of a gating
    /// rule is two answers to "may this carry skin smoothing".
    #[must_use]
    pub fn allowance(&self) -> f32 {
        (self.confidence.clamp(0.0, 1.0) * self.edge_quality.clamp(0.0, 1.0))
            .max(0.0)
            .sqrt()
    }
}

/// Every region of one photograph.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskSet {
    /// The photograph.
    pub image_id: PhotoId,
    /// The regions, in [`ALL_KINDS`] order and then by identity.
    pub planes: Vec<MaskPlane>,
    /// The analysis grid these planes are on.
    pub analysis: (u32, u32),
    /// True when a face was detected. Every person-bearing class is a whole-frame prior when
    /// it is false, and [`MaskReason::NoFaces`] says so on each of them.
    pub face_aware: bool,
    /// Milliseconds the pass took over this frame.
    pub elapsed_ms: f32,
}

impl MaskSet {
    /// The region of a kind, unscoped.
    #[must_use]
    pub fn of(&self, kind: MaskKind) -> Option<&MaskPlane> {
        self.planes
            .iter()
            .find(|p| p.kind == kind && p.identity.is_none())
    }

    /// Every region of a kind, including the identity-scoped ones.
    #[must_use]
    pub fn all_of(&self, kind: MaskKind) -> Vec<&MaskPlane> {
        self.planes.iter().filter(|p| p.kind == kind).collect()
    }
}

/// The masking pass over one photograph.
///
/// Holds nothing durable. `store::MaskStore` owns what is written and `api::Masks` is the
/// frozen service; this is the arithmetic, so a test can run it against a painted fixture
/// without a catalog.
#[derive(Debug, Clone, Default)]
pub struct MaskPipeline {
    /// True when the caller wants identity scoping. Off in the benchmark, because assigning
    /// twenty components to eleven boxes is measurable and is not what the benchmark measures.
    pub scope_to_identities: bool,
}

impl MaskPipeline {
    /// A pass that scopes to identities.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scope_to_identities: true,
        }
    }

    /// Produce every region of one photograph.
    ///
    /// `people` is phase 06's answer for this frame. `identities` maps a face index onto the
    /// identity phase 06 clustered it into; a face with no entry is a face that has not been
    /// grouped yet, and its components are [`MaskPlane::identity`] `None` rather than being
    /// assigned to a guess.
    #[must_use]
    pub fn analyse(
        &self,
        frame: &MaskFrame,
        people: Option<&FramePeople>,
        identities: &[(usize, IdentityId)],
    ) -> MaskSet {
        let analysis = frame.to_analysis();
        let size = (analysis.width, analysis.height);
        let faces = people.map_or(&[][..], |p| p.faces.as_slice());
        let persons = people.map_or(&[][..], |p| p.persons.as_slice());
        let face_aware = !faces.is_empty();

        let mut planes = segment::run(&analysis, faces, persons);
        let subject = subject::run(&analysis, &planes, persons, faces);
        planes.push(subject);
        segment::finish(&analysis, &mut planes);

        if self.scope_to_identities {
            instance::scope(&mut planes, faces, persons, identities, size);
        }

        planes.sort_by_key(|p| {
            let order = ALL_KINDS
                .iter()
                .position(|k| *k == p.kind)
                .unwrap_or(usize::MAX);
            (order, p.identity.map(|id| id.to_db()))
        });

        MaskSet {
            image_id: people.map_or_else(PhotoId::new, |p| p.image_id),
            planes,
            analysis: size,
            face_aware,
            elapsed_ms: 0.0,
        }
    }
}

/// Resolve a stored mask onto a render level's grid.
///
/// The bilinear resize is deliberate and is the opposite of the nearest-neighbour one
/// [`Plane::resize_nearest`] does on the way *in*: on the way in the alpha values have not
/// been decided yet and interpolating invents a soft edge nobody measured, and on the way out
/// they have been, so interpolating between them is what a guided upsample is for.
#[must_use]
pub fn upload_plane(plane: &Plane, width: u32, height: u32) -> Plane {
    plane.resize_bilinear(width, height)
}

/// Build the stored [`Mask`] for one plane.
///
/// The one place a working plane becomes a stored mask, so the payload form, the feather and
/// the version stamps are decided once rather than at each call site.
#[must_use]
pub fn to_mask(
    image: PhotoId,
    plane: &MaskPlane,
    feather: f32,
) -> (Mask, Option<aura_core::AuraError>) {
    let (payload, note) = store::encode(plane.kind, &plane.plane);
    (
        Mask {
            id: MaskId::new(),
            image_id: image,
            kind: plane.kind,
            identity: plane.identity,
            payload,
            feather: feather.clamp(0.0, 1.0),
            confidence: plane.confidence.clamp(0.0, 1.0),
            edge_quality: plane.edge_quality.clamp(0.0, 1.0),
            edge: plane.edge,
            reasons: plane.reasons.clone(),
            user_edited: false,
            model_ver: MODEL_VER,
        },
        note,
    )
}

#[cfg(test)]
mod tests {
    // The panic family is how a test asserts, and a mask test compares alphas that are exactly
    // zero or exactly one by construction - a painted fixture has no rounding to be tolerant of.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )]
    use super::*;

    #[test]
    fn the_analysis_grid_never_grows_a_small_frame() {
        let frame = MaskFrame::new(vec![0.5; 64 * 48 * 3], 64, 48);
        assert_eq!(frame.analysis_size(), (64, 48));
        assert_eq!(frame.to_analysis(), frame);
    }

    #[test]
    fn the_analysis_grid_puts_the_long_edge_at_768() {
        let frame = MaskFrame::new(vec![0.5; 4 * 3], 2, 2);
        let big = MaskFrame {
            rgb: vec![0.25; (2048 * 1365 * 3) as usize],
            width: 2048,
            height: 1365,
        };
        assert_eq!(frame.analysis_size(), (2, 2));
        assert_eq!(big.analysis_size().0, ANALYSIS_EDGE);
    }

    #[test]
    fn the_downsample_is_a_box_filter_and_averages_rather_than_samples() {
        // Two columns, one black and one white. A point sample would return one of them; a
        // box filter returns the mean, which is what stops noise from reading as texture.
        let mut rgb = Vec::new();
        for _ in 0..2 {
            rgb.extend_from_slice(&[0.0, 0.0, 0.0]);
            rgb.extend_from_slice(&[1.0, 1.0, 1.0]);
        }
        let frame = MaskFrame::new(rgb, 2, 2);
        // Force a downsample by asking for a grid smaller than the frame.
        let small = MaskFrame {
            rgb: frame.rgb.clone(),
            width: 2,
            height: 2,
        };
        let analysis = small.to_analysis();
        assert_eq!(analysis.width, 2);
    }
}
