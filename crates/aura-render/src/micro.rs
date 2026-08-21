//! The processor reference for PHASE-21's micro-retouch stage.
//!
//! `aura-retouch::micro` decides *what* small fixes a photograph should get;
//! `aura_core::contract::micro::MicroOp` carries the decision; this module is what turns the
//! decision into pixels. The two WGSL files - `micro_apply.wgsl` and `micro_borrow.wgsl` - are
//! the same arithmetic for a device, and `crates/aura-render/tests/shader_parity.rs` holds the
//! two to the same constants.
//!
//! ## Why the decision phase calls into the renderer
//!
//! Phase 16 established it for skin colour, phase 20 repeated it for skin texture and this is the
//! third: **a guarantee about a pixel is enforced on the pixel**. `aura_retouch::micro::guard`
//! runs the plan through [`apply`] and then measures the three quantities section 10.1 gates -
//! [`catchlight_peak`], [`edge_energy`] and [`teeth_excursion`] - on the buffer the renderer
//! actually produced. A second implementation of these operators inside the decision crate would
//! make the stored ratios statements about a model of the renderer.
//!
//! ## The one idea
//!
//! **Every operator here is a bounded reduction of something it measured in the same frame.**
//! There is no target value anywhere in this module. Flyaway reduction moves a strand's contrast
//! toward the background *behind that strand*; the teeth operator reduces a chromaticity's
//! distance to a locus centred on *this frame's* neutral; the sclera operator removes a share of
//! the redness *it measured*; the iris operator scales the local contrast that is already there.
//! A module with no constants to move a pixel toward cannot have a preferred appearance.
//!
//! ## Specular pixels are excluded by construction, not by a threshold applied afterwards
//!
//! [`SPECULAR_FLOOR`] is read at the top of both eye operators and the value is *left alone*
//! rather than scaled down. A catchlight is what makes an eye look alive, and an operator that
//! touched it a little and then had its damage measured would be an operator that sometimes
//! passed. The guard measures the peak afterwards anyway, because the exclusion is arithmetic
//! and arithmetic can be wrong.
//!
//! ## Everything here is linear
//!
//! Invariant 8. The only `powf` in this module is inside [`gain`], where it produces a
//! *multiplier* from a stop count and never an encoded value.

use std::collections::BTreeMap;

use aura_core::contract::composition::Box2;
use aura_core::contract::micro::{ClothingIssue, ColourLocus, GlareMethod, MicroOp, MicroRegion};
use aura_raw::colour::illuminant::linear_srgb_to_uv;

use crate::bands;

/// The luminance at or above which a pixel is treated as specular and left alone.
///
/// Nine tenths of the working space's diffuse white. Above this a pixel is a reflection of the
/// light source rather than of the subject: it is a catchlight in an eye, the wet highlight on a
/// lip, or the sheet on a pair of glasses. Every eye and teeth operator reads this first.
pub const SPECULAR_FLOOR: f32 = 0.90;

/// The luminance at or above which a pixel is treated as *clipped* by a glare sheet.
///
/// Higher than [`SPECULAR_FLOOR`], because the two answer different questions: the first is
/// "would touching this dull a highlight" and this is "does this pixel still record anything".
/// A pixel at 0.985 of the working white carries no recoverable detail, and
/// `MIN_SPECULAR_FRACTION` of a borrow region has to be at or above this before a borrow is
/// permitted at all.
pub const CLIPPED_FLOOR: f32 = 0.985;

/// The radius, as a fraction of the shorter side of a flyaway region, of the background estimate.
///
/// A twelfth. Wide enough that a strand of hair is entirely inside the blur kernel - which is
/// what makes the blurred value an estimate of the background *behind* the strand - and narrow
/// enough that it does not reach across the whole region into the hair mass.
pub const FLYAWAY_BACKGROUND_FRAC: f32 = 1.0 / 12.0;

/// The radius, as a fraction of the iris width, of the iris clarity high-pass.
///
/// A sixth. The structures a retoucher raises in an iris are the radial fibres and the collarette,
/// both a few per cent of the iris across; a wider kernel starts raising the limbal ring, which is
/// the edge that makes an over-clarified iris read as drawn.
pub const IRIS_DETAIL_FRAC: f32 = 1.0 / 6.0;

/// How far a donor patch is looked for when cleaning a garment, as a multiple of the mark's own
/// radius.
///
/// Two and a fifth, phase 20's number and for phase 20's reason: close enough that the fabric is
/// the same garment under the same light, far enough that the donor is outside the mark and its
/// halo.
pub const DONOR_DISTANCE: f32 = 2.2;

/// How many directions the donor search tries, evenly spaced.
pub const DONOR_DIRECTIONS: usize = 8;

/// The width of the feather at the edge of a cleaned or borrowed patch, as a fraction of its
/// radius.
pub const PATCH_FEATHER: f32 = 0.25;

/// Everything the operators need that the renderer must not invent.
///
/// Three things. The region weights come from phase 18 and decide where an operator may act at
/// all; the eye centres are phase 06's landmarks, parallel to the per-identity operations in the
/// plan; and the neutral chromaticity is phase 15's illuminant estimate, which is the origin the
/// teeth locus is expressed against. An absent neutral is not a default - the teeth operator's
/// colour half does nothing without one, because a locus with no origin describes nothing.
#[derive(Debug, Clone)]
pub struct MicroContext {
    /// Per-pixel coverage per region, `0..1`, each `width * height` long.
    ///
    /// A missing region is a region no operator may act through. That is the same gating phases
    /// 19 and 20 apply and for the same reason: an operator that falls back to a rectangle edits
    /// the wall behind somebody's ear.
    pub regions: BTreeMap<MicroRegion, Vec<f32>>,
    /// Eye centres in pixels, left then right, for each face the plan corrected.
    ///
    /// Parallel to the [`MicroOp::Teeth`] and [`MicroOp::Eyes`] operations in the plan, in that
    /// order. A face with no landmarks is not in this list and its operations do nothing - phase
    /// 09's rule that an unknown landmark must never be read as the origin.
    pub faces: Vec<FaceGeometry>,
    /// The frame's own neutral in CIE `u'v'`, from phase 15.
    ///
    /// `None` when no illuminant was estimated. The teeth colour move is then skipped entirely.
    pub neutral: Option<[f32; 2]>,
    /// The locus plausible teeth chromaticities sit in, relative to `neutral`.
    pub teeth_locus: ColourLocus,
    /// Aligned donor patches for the borrows in the plan, in operation order.
    ///
    /// Filled by `aura_retouch::micro::borrow`, which owns the alignment search. The renderer
    /// composites; it does not decide what may be composited.
    pub borrows: Vec<BorrowPatch>,
}

/// Where one face's landmarks are, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceGeometry {
    /// Left eye centre.
    pub left_eye: [f32; 2],
    /// Right eye centre.
    pub right_eye: [f32; 2],
    /// The face box in frame coordinates, for the teeth region's own bound.
    pub bbox: Box2,
}

/// One aligned donor region, ready to composite.
///
/// The renderer is handed pixels rather than a source image and a transform, so that the
/// alignment search - which is a *decision* about whether a borrow is permissible at all - stays
/// in the phase that owns the refusal.
#[derive(Debug, Clone, PartialEq)]
pub struct BorrowPatch {
    /// Frame x of the patch's first column.
    pub x: usize,
    /// Frame y of the patch's first row.
    pub y: usize,
    /// Patch width.
    pub w: usize,
    /// Patch height.
    pub h: usize,
    /// Interleaved linear RGB, `w * h * 3` long, already aligned to the target region.
    pub rgb: Vec<f32>,
}

impl Default for MicroContext {
    fn default() -> Self {
        Self::empty()
    }
}

impl MicroContext {
    /// A context with no regions and no landmarks: every operator becomes a no-op.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            regions: BTreeMap::new(),
            faces: Vec::new(),
            neutral: None,
            teeth_locus: ColourLocus::OPEN,
            borrows: Vec::new(),
        }
    }

    /// Coverage of one region at one pixel, `0..1`.
    #[must_use]
    pub fn at(&self, region: MicroRegion, index: usize) -> f32 {
        self.regions
            .get(&region)
            .and_then(|plane| plane.get(index))
            .copied()
            .unwrap_or(0.0)
    }

    /// True when a region arrived at all.
    #[must_use]
    pub fn has(&self, region: MicroRegion) -> bool {
        self.regions
            .get(&region)
            .is_some_and(|plane| plane.iter().any(|w| *w > 0.0))
    }
}

/// What one call to [`apply`] did.
///
/// Returned rather than logged, because the decision phase turns it into reasons: an operation
/// the renderer could not perform is a code in the plan, and an operation nobody could tell had
/// been skipped is the failure mode section 12 calls a preview/export mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MicroApplied {
    /// Flyaway regions attenuated.
    pub flyaways: u32,
    /// Teeth corrections applied.
    pub teeth: u32,
    /// Teeth corrections whose colour half was skipped for want of an illuminant.
    pub teeth_luma_only: u32,
    /// Eye corrections applied.
    pub eyes: u32,
    /// Garment marks cleaned.
    pub clothing: u32,
    /// Garment marks skipped because no donor patch was close enough.
    pub no_donor: u32,
    /// Glare sheets reduced from this frame.
    pub glare_reduced: u32,
    /// Glare sheets reconstructed from a sibling frame.
    pub borrowed: u32,
    /// Operations skipped because the region they needed was not present.
    pub unregioned: u32,
}

/// Apply a plan's operations to a linear RGB buffer, in place.
///
/// The order is fixed and is not the plan's order: glare, then flyaway, then clothing, then
/// teeth, then eyes. It is not arbitrary.
///
/// - **Glare first**, because a specular sheet over an eye would otherwise set the specular
///   exclusion the eye operators read, and an eye under a repaired sheet is an eye that can then
///   be corrected normally.
/// - **Flyaway and clothing next**, because both replace content and the two that follow measure
///   statistics over a region.
/// - **Teeth and eyes last**, so their measurements read a frame nothing else is still going to
///   change.
///
/// The fixed order is what makes the same plan produce the same pixels on a proxy and at full
/// resolution, which is section 10.1's preview/export agreement.
///
/// `pixels` is interleaved linear RGB, `width * height * 3` long.
pub fn apply(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    ops: &[MicroOp],
    context: &MicroContext,
) -> MicroApplied {
    let mut applied = MicroApplied::default();
    if width == 0 || height == 0 || pixels.len() < width * height * 3 {
        return applied;
    }

    let mut borrow_index = 0usize;
    for op in ops {
        if let MicroOp::Glare { region, method } = op {
            match method {
                GlareMethod::Reduce { strength } => {
                    if context.has(MicroRegion::Eyes) || context.has(MicroRegion::Face) {
                        reduce_glare(pixels, width, height, *region, *strength);
                        applied.glare_reduced += 1;
                    } else {
                        applied.unregioned += 1;
                    }
                }
                GlareMethod::BorrowFrom { .. } => {
                    if let Some(patch) = context.borrows.get(borrow_index) {
                        composite(pixels, width, height, patch);
                        applied.borrowed += 1;
                    } else {
                        applied.unregioned += 1;
                    }
                    borrow_index += 1;
                }
            }
        }
    }

    for op in ops {
        if let MicroOp::Flyaway { region, strength } = op {
            if context.has(MicroRegion::Hair) {
                calm_flyaways(pixels, width, height, *region, *strength, context);
                applied.flyaways += 1;
            } else {
                applied.unregioned += 1;
            }
        }
    }

    for op in ops {
        if let MicroOp::Clothing {
            region,
            kind,
            strength,
        } = op
        {
            let plane = garment_plane(*kind);
            if context.has(plane) {
                if clean_garment(pixels, width, height, *region, *strength, plane, context) {
                    applied.clothing += 1;
                } else {
                    applied.no_donor += 1;
                }
            } else {
                applied.unregioned += 1;
            }
        }
    }

    let mut face_index = 0usize;
    for op in ops {
        if let MicroOp::Teeth {
            luma,
            yellow_reduce,
            ..
        } = op
        {
            if context.has(MicroRegion::Teeth) && context.faces.get(face_index).is_some() {
                let coloured = correct_teeth(pixels, width, height, *luma, *yellow_reduce, context);
                applied.teeth += 1;
                if !coloured {
                    applied.teeth_luma_only += 1;
                }
            } else {
                applied.unregioned += 1;
            }
            face_index += 1;
        }
    }

    for op in ops {
        if let MicroOp::Eyes {
            sclera,
            iris_clarity,
            ..
        } = op
        {
            if context.has(MicroRegion::Sclera) || context.has(MicroRegion::Iris) {
                clear_sclera(pixels, width, height, *sclera, context);
                clarify_iris(pixels, width, height, *iris_clarity, context);
                applied.eyes += 1;
            } else {
                applied.unregioned += 1;
            }
        }
    }

    applied
}

// ---------------------------------------------------------------------------
// Hair
// ---------------------------------------------------------------------------

/// Attenuate a stray strand's contrast against the background behind it.
///
/// Section 6.1's "reduce rather than remove", as arithmetic. Four properties, and each is a
/// failure this operator would otherwise have:
///
/// 1. **The background estimate is a blur of the frame at a radius wider than a strand.** A
///    strand is one or two pixels across and the kernel is a twelfth of the region, so the
///    blurred value at a strand is very nearly the background it sits on. That is what makes
///    "contrast against the background" measurable without a second image.
/// 2. **The move is toward the background and stops short of it.** `strength` is capped at
///    `MAX_FLYAWAY_STRENGTH` in the contract, so a strand always keeps a fraction of its own
///    contrast and the hairline still reads as hair.
/// 3. **Nothing inside the hair mass moves.** The per-pixel weight is `1 - hair`, so a pixel the
///    generator is sure is hair has weight zero. The mass is what a bald patch is made of.
/// 4. **The weight reads the *input*.** Phase 19 found this trap the hard way: a weight
///    evaluated on a partially-edited value is not linear in its own strength, and past about
///    half coverage the correction overtakes and produces a bright rim. The blur is taken once,
///    from the buffer as it arrived, and every weight below reads it.
fn calm_flyaways(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    region: Box2,
    strength: f32,
    context: &MicroContext,
) {
    let window = to_pixels(region, width, height);
    if window.w == 0 || window.h == 0 || strength <= 0.0 {
        return;
    }
    let radius = bands::radius(window.w.min(window.h) as f32, FLYAWAY_BACKGROUND_FRAC);

    // The window plus a margin, so the blur at the window's edge is not made of zeros.
    let margin = radius * 2;
    let x0 = window.x.saturating_sub(margin);
    let y0 = window.y.saturating_sub(margin);
    let w = (window.w + margin * 2).min(width.saturating_sub(x0));
    let h = (window.h + margin * 2).min(height.saturating_sub(y0));
    if w == 0 || h == 0 {
        return;
    }

    for channel in 0..3 {
        let mut plane = Vec::with_capacity(w * h);
        for row in 0..h {
            for col in 0..w {
                let slot = ((y0 + row) * width + (x0 + col)) * 3 + channel;
                plane.push(pixels.get(slot).copied().unwrap_or(0.0));
            }
        }
        let background = bands::blur(&plane, w, h, radius);

        for row in 0..h {
            for col in 0..w {
                let fx = x0 + col;
                let fy = y0 + row;
                if fx < window.x || fx >= window.right() || fy < window.y || fy >= window.bottom() {
                    continue;
                }
                let index = fy * width + fx;
                // Outside the hair mass, and no further than the region says.
                let outside = 1.0 - context.at(MicroRegion::Hair, index).clamp(0.0, 1.0);
                if outside <= 0.0 {
                    continue;
                }
                let local = row * w + col;
                let value = plane.get(local).copied().unwrap_or(0.0);
                let base = background.get(local).copied().unwrap_or(value);
                let feathered = feather(fx - window.x, fy - window.y, window.w, window.h);
                let move_by = (value - base) * strength * outside * feathered;
                if let Some(slot) = pixels.get_mut(index * 3 + channel) {
                    *slot = (value - move_by).max(0.0);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Teeth
// ---------------------------------------------------------------------------

/// Even the teeth's luminance and reduce their distance to the locus.
///
/// Returns true when the colour half ran, which is false exactly when the frame has no
/// illuminant estimate. Two halves, and both are bounded reductions:
///
/// - **Luminance.** A gain from a stop count, applied through the teeth coverage, and *clamped
///   against the brightest non-specular skin on the face* - which is the guarantee section 6.2
///   makes as a comparison against the subject rather than against a number. Specular pixels are
///   excluded, so a wet highlight on an incisor is not lifted with the rest.
/// - **Colour.** Each pixel's chromaticity offset from the frame's neutral is measured, its
///   excess outside the locus is computed, and a bounded share of that excess is removed by
///   moving toward the locus boundary. A pixel already inside is untouched. The move is applied
///   as a scale on the two chromatic channels at constant luminance, so nothing here can change
///   how bright a tooth is by way of its colour.
fn correct_teeth(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    luma_ev: f32,
    yellow_reduce: f32,
    context: &MicroContext,
) -> bool {
    let Some(plane) = context.regions.get(&MicroRegion::Teeth) else {
        return false;
    };
    let ceiling = brightest_skin(pixels, width, height, context);
    let lift = gain(luma_ev.max(0.0));
    let neutral = context.neutral;

    for index in 0..width * height {
        let coverage = plane.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        if coverage <= 0.0 {
            continue;
        }
        let slot = index * 3;
        let Some(rgb) = pixels.get(slot..slot + 3).map(triple) else {
            continue;
        };
        let before = luma(rgb);
        if before >= SPECULAR_FLOOR {
            // A highlight on an incisor. Left exactly alone, for the reason the module header
            // gives about catchlights: a reflection of the light is not a property of the tooth.
            continue;
        }

        // --- luminance -------------------------------------------------------------------
        let wanted = before * lift;
        // The comparison against the subject. `ceiling` is the brightest non-specular skin on
        // this face in this frame; teeth that would go past it stop there.
        let target = if ceiling > 0.0 {
            wanted.min(ceiling)
        } else {
            wanted
        };
        let scale = if before > 1e-6 { target / before } else { 1.0 };
        let mut out = [rgb[0] * scale, rgb[1] * scale, rgb[2] * scale];

        // --- colour ----------------------------------------------------------------------
        if let Some(white) = neutral {
            if yellow_reduce > 0.0 {
                out = pull_toward_locus(out, white, context.teeth_locus, yellow_reduce);
            }
        }

        for channel in 0..3 {
            let value = rgb.get(channel).copied().unwrap_or(0.0);
            let moved = out.get(channel).copied().unwrap_or(value);
            if let Some(target) = pixels.get_mut(slot + channel) {
                *target = (value + (moved - value) * coverage).max(0.0);
            }
        }
    }
    neutral.is_some()
}

/// Move one colour a bounded share of the way from outside a locus to its boundary.
///
/// **There is no path through this function that moves a chromaticity toward the locus centre.**
/// A colour already inside comes back unchanged; one outside travels `share` of its own excess
/// and no further. That is what makes the locus a bound rather than a target, and it is the whole
/// of ADR-0043 section 3 as three lines of arithmetic.
///
/// The move is at constant luminance: the corrected chromaticity is turned back into a linear
/// colour and re-normalised onto the input's own `Y`.
fn pull_toward_locus(rgb: [f32; 3], neutral: [f32; 2], locus: ColourLocus, share: f32) -> [f32; 3] {
    let before = luma(rgb);
    if before <= 1e-6 {
        return rgb;
    }
    let uv = linear_srgb_to_uv(rgb);
    let du = uv[0] - neutral[0];
    let dv = uv[1] - neutral[1];
    let excess = locus.excess(du, dv);
    if excess <= 0.0 {
        return rgb;
    }
    // Direction from the locus centre to this chromaticity, in the plane of offsets.
    let cx = du - locus.du;
    let cy = dv - locus.dv;
    let distance = (cx * cx + cy * cy).sqrt();
    if distance <= 1e-9 {
        return rgb;
    }
    let travel = excess * share.clamp(0.0, 1.0);
    let scale = (distance - travel) / distance;
    let corrected = [
        neutral[0] + locus.du + cx * scale,
        neutral[1] + locus.dv + cy * scale,
    ];
    let unit = aura_raw::colour::illuminant::uv_to_linear_srgb(corrected);
    let unit_luma = luma(unit);
    if unit_luma <= 1e-6 {
        return rgb;
    }
    [
        unit[0] / unit_luma * before,
        unit[1] / unit_luma * before,
        unit[2] / unit_luma * before,
    ]
}

/// The brightest non-specular skin luminance on the frame, or zero when there is no skin.
///
/// The number the teeth lift is clamped against. Measured rather than assumed, and measured
/// *excluding* specular pixels, because the brightest pixel on a forehead under flash is a
/// reflection of the flash and clamping against it would be clamping against nothing.
fn brightest_skin(pixels: &[f32], width: usize, height: usize, context: &MicroContext) -> f32 {
    let Some(plane) = context.regions.get(&MicroRegion::Skin) else {
        return 0.0;
    };
    let mut best = 0.0f32;
    for index in 0..width * height {
        if plane.get(index).copied().unwrap_or(0.0) < 0.5 {
            continue;
        }
        let slot = index * 3;
        let Some(rgb) = pixels.get(slot..slot + 3).map(triple) else {
            continue;
        };
        let value = luma(rgb);
        if value < SPECULAR_FLOOR && value > best {
            best = value;
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Eyes
// ---------------------------------------------------------------------------

/// Take a bounded share of the measured redness out of the whites of the eyes.
///
/// **Chroma only.** The corrected colour is re-normalised onto the input's own luminance before
/// it is written, so there is no value of `share` at which this operator brightens a sclera.
/// Section 6.2 asks for exactly that and `MicroOp::Eyes` has nowhere to put a luminance term, so
/// the promise is kept in two places.
///
/// Redness is measured as the chromaticity's excess outside the sclera's own locus - the same
/// shape the teeth correction uses, and for the same reason: a sclera under tungsten is warm
/// because the room is, and an absolute target would fight the white balance.
fn clear_sclera(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    share: f32,
    context: &MicroContext,
) {
    let Some(plane) = context.regions.get(&MicroRegion::Sclera) else {
        return;
    };
    let Some(neutral) = context.neutral else {
        return;
    };
    if share <= 0.0 {
        return;
    }
    // The sclera's own locus is tighter than the teeth's and is centred nearer the neutral.
    // Expressed here rather than loaded, because it is a property of what a sclera *is* and the
    // config file's copy is what a studio may tighten.
    let locus = ColourLocus {
        du: 0.001,
        dv: 0.002,
        radius: 0.022,
    };

    for index in 0..width * height {
        let coverage = plane.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        if coverage <= 0.0 {
            continue;
        }
        let slot = index * 3;
        let Some(rgb) = pixels.get(slot..slot + 3).map(triple) else {
            continue;
        };
        if luma(rgb) >= SPECULAR_FLOOR {
            // A catchlight. Excluded by construction - see the module header.
            continue;
        }
        let corrected = pull_toward_locus(rgb, neutral, locus, share);
        for channel in 0..3 {
            let value = rgb.get(channel).copied().unwrap_or(0.0);
            let moved = corrected.get(channel).copied().unwrap_or(value);
            if let Some(target) = pixels.get_mut(slot + channel) {
                *target = (value + (moved - value) * coverage).max(0.0);
            }
        }
    }
}

/// Raise the local contrast already present in an iris.
///
/// A high-pass at [`IRIS_DETAIL_FRAC`] of the iris width, added back at a bounded gain, on
/// luminance only. Three things it is not:
///
/// - **Not a colour change.** The gain is applied as a multiplier on all three channels equally,
///   so a blue iris stays exactly as blue as it was. There is no representable eye-colour change
///   in this phase.
/// - **Not a sharpen.** The kernel is a sixth of the iris rather than a pixel or two, so what
///   comes up is the fibre structure rather than the sensor's noise.
/// - **Not applied to catchlights.** Same exclusion as everywhere else in this module.
fn clarify_iris(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    gain_amount: f32,
    context: &MicroContext,
) {
    let Some(plane) = context.regions.get(&MicroRegion::Iris) else {
        return;
    };
    if gain_amount <= 0.0 {
        return;
    }
    let Some(window) = bounds_of(plane, width, height) else {
        return;
    };
    let radius = bands::radius(window.w.min(window.h) as f32, IRIS_DETAIL_FRAC);

    let mut luminance = Vec::with_capacity(width * height);
    for index in 0..width * height {
        let slot = index * 3;
        luminance.push(pixels.get(slot..slot + 3).map_or(0.0, |v| luma(triple(v))));
    }
    let smoothed = bands::blur(&luminance, width, height, radius);

    for index in 0..width * height {
        let coverage = plane.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        if coverage <= 0.0 {
            continue;
        }
        let value = luminance.get(index).copied().unwrap_or(0.0);
        if value >= SPECULAR_FLOOR {
            continue;
        }
        let base = smoothed.get(index).copied().unwrap_or(value);
        let detail = value - base;
        let wanted = (value + detail * gain_amount * coverage).max(0.0);
        if value <= 1e-6 {
            continue;
        }
        let scale = wanted / value;
        let slot = index * 3;
        for channel in 0..3 {
            if let Some(target) = pixels.get_mut(slot + channel) {
                *target = (*target * scale).max(0.0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Clothing
// ---------------------------------------------------------------------------

/// Which region a clothing operation acts through.
///
/// All five kinds go through `Clothing`, and the argument is kept rather than dropped because the
/// question this function answers is a real one that could have a different answer: a bra strap
/// and a crease are on worn fabric by definition of what they are, while lint, threads and stains
/// happen on a dress as readily as on a jacket. The clothing region is the larger of the two on
/// nearly every frame, so one plane serves all five - and if that ever stops being true, this is
/// the one place it changes.
const fn garment_plane(_kind: ClothingIssue) -> MicroRegion {
    MicroRegion::Clothing
}

/// Clean one small mark off a garment by patch synthesis.
///
/// Phase 20's healing shape, one region up and simplified in exactly one way: **there is no
/// high-band transplant.** Phase 20 needed one because skin texture is the guarantee it makes;
/// here the guarantee about fabric is that it keeps its own weave, and the way to keep a weave is
/// to copy it wholesale from four millimetres away rather than to decompose and recombine it. The
/// donor patch is used as it is, tone-shifted onto the ring around the mark.
///
/// Returns false when no donor was close enough, which the caller turns into a reason rather than
/// a silent no-op.
fn clean_garment(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    region: Box2,
    strength: f32,
    plane: MicroRegion,
    context: &MicroContext,
) -> bool {
    let window = to_pixels(region, width, height);
    if window.w == 0 || window.h == 0 || strength <= 0.0 {
        return true;
    }
    let radius = ((window.w.max(window.h) as f32) * 0.5).max(1.0);
    let step = radius * DONOR_DISTANCE;

    let target = read(pixels, width, height, window);
    let Some(target) = target else {
        return true;
    };
    let target_mean = mean_luma(&target, window.w, window.h);

    let mut best: Option<(f32, Vec<f32>)> = None;
    for direction in 0..DONOR_DIRECTIONS {
        let angle = std::f32::consts::TAU * direction as f32 / DONOR_DIRECTIONS as f32;
        let dx = (angle.cos() * step).round() as i64;
        let dy = (angle.sin() * step).round() as i64;
        let x = window.x as i64 + dx;
        let y = window.y as i64 + dy;
        if x < 0 || y < 0 {
            continue;
        }
        let donor = Region {
            x: x as usize,
            y: y as usize,
            w: window.w,
            h: window.h,
        };
        if donor.right() > width || donor.bottom() > height {
            continue;
        }
        // The donor must be the same garment: every pixel of it inside the clothing region.
        if !fully_inside(plane, donor, width, context) {
            continue;
        }
        let Some(candidate) = read(pixels, width, height, donor) else {
            continue;
        };
        let delta = (mean_luma(&candidate, window.w, window.h) - target_mean).abs();
        // Fixed iteration order and a strict comparison, so the tie-break between two equally
        // good donors is the same on every machine. Invariant 4.
        if best.as_ref().is_none_or(|(score, _)| delta < *score) {
            best = Some((delta, candidate));
        }
    }

    let Some((_, donor)) = best else {
        return false;
    };
    let donor_mean = mean_luma(&donor, window.w, window.h);
    let shift = target_mean - donor_mean;

    for row in 0..window.h {
        for col in 0..window.w {
            let index = (window.y + row) * width + (window.x + col);
            let coverage = context.at(plane, index).clamp(0.0, 1.0);
            let weight = feather(col, row, window.w, window.h) * coverage * strength;
            if weight <= 0.0 {
                continue;
            }
            let local = (row * window.w + col) * 3;
            for channel in 0..3 {
                let donor_value = donor.get(local + channel).copied().unwrap_or(0.0) + shift;
                if let Some(slot) = pixels.get_mut(index * 3 + channel) {
                    *slot = (*slot + (donor_value - *slot) * weight).max(0.0);
                }
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Glare
// ---------------------------------------------------------------------------

/// Reduce a specular sheet using only this frame.
///
/// The conservative path, and it is deliberately unable to reconstruct anything: what it does is
/// pull the sheet's luminance down toward the region's own non-clipped surround, which makes the
/// reflection less dominant and leaves whatever detail survived underneath. A sheet that has
/// clipped completely comes out grey rather than resolved, which is honest - the alternative is
/// inventing an eye, and that is the operation `docs/retouch-ethics.md` refuses.
fn reduce_glare(pixels: &mut [f32], width: usize, height: usize, region: Box2, strength: f32) {
    let window = to_pixels(region, width, height);
    if window.w == 0 || window.h == 0 || strength <= 0.0 {
        return;
    }
    // The surround: the ring of pixels the feather does not reach, excluding anything clipped.
    let mut total = 0.0f64;
    let mut count = 0usize;
    for row in 0..window.h {
        for col in 0..window.w {
            if feather(col, row, window.w, window.h) > 0.0 {
                continue;
            }
            let slot = ((window.y + row) * width + (window.x + col)) * 3;
            let Some(rgb) = pixels.get(slot..slot + 3).map(triple) else {
                continue;
            };
            let value = luma(rgb);
            if value >= CLIPPED_FLOOR {
                continue;
            }
            total += f64::from(value);
            count += 1;
        }
    }
    if count == 0 {
        return;
    }
    let surround = (total / count as f64) as f32;

    for row in 0..window.h {
        for col in 0..window.w {
            let weight = feather(col, row, window.w, window.h) * strength;
            if weight <= 0.0 {
                continue;
            }
            let index = (window.y + row) * width + (window.x + col);
            let slot = index * 3;
            let Some(rgb) = pixels.get(slot..slot + 3).map(triple) else {
                continue;
            };
            let value = luma(rgb);
            if value <= surround {
                continue;
            }
            let wanted = value + (surround - value) * weight;
            if value <= 1e-6 {
                continue;
            }
            let scale = wanted / value;
            for channel in 0..3 {
                if let Some(target) = pixels.get_mut(slot + channel) {
                    *target = (*target * scale).max(0.0);
                }
            }
        }
    }
}

/// Composite one aligned donor patch over the frame.
///
/// Feathered, and nothing else: the alignment, the specular test and the refusal all happened in
/// `aura_retouch::micro::borrow` before this was called. The renderer composites; it does not
/// decide what may be composited, and keeping that split is what makes the refusal auditable in
/// one place.
fn composite(pixels: &mut [f32], width: usize, height: usize, patch: &BorrowPatch) {
    if patch.w == 0 || patch.h == 0 || patch.rgb.len() < patch.w * patch.h * 3 {
        return;
    }
    for row in 0..patch.h {
        for col in 0..patch.w {
            let fx = patch.x + col;
            let fy = patch.y + row;
            if fx >= width || fy >= height {
                continue;
            }
            let weight = feather(col, row, patch.w, patch.h);
            if weight <= 0.0 {
                continue;
            }
            let local = (row * patch.w + col) * 3;
            let slot = (fy * width + fx) * 3;
            for channel in 0..3 {
                let donor = patch.rgb.get(local + channel).copied().unwrap_or(0.0);
                if let Some(target) = pixels.get_mut(slot + channel) {
                    *target += (donor - *target) * weight;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The three measurements the guard makes
// ---------------------------------------------------------------------------

/// The peak luminance inside the iris regions, and how many pixels it was found over.
///
/// Section 10.1's specular-pixel test. The *peak* rather than the mean, because a catchlight is
/// one small very bright thing and a mean over an iris would move by a hundredth if it were
/// erased entirely.
#[must_use]
pub fn catchlight_peak(pixels: &[f32], width: usize, height: usize, iris: &[f32]) -> (f32, u32) {
    let mut peak = 0.0f32;
    let mut count = 0u32;
    for index in 0..width * height {
        if iris.get(index).copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        count += 1;
        let slot = index * 3;
        let Some(rgb) = pixels.get(slot..slot + 3).map(triple) else {
            continue;
        };
        peak = peak.max(luma(rgb));
    }
    (peak, count)
}

/// The mean absolute local residual inside a region, and how many pixels it was found over.
///
/// What "hairline damage" is, measured: the residual of the luminance against a blur of itself,
/// over the hair region. A bald patch flattens it and nothing else in this module can. The same
/// quantity [`crate::bands::Bands3::high_energy`] uses, so the number the hair guard checks and
/// the number phase 20's texture guard checks are the same kind of thing.
#[must_use]
pub fn edge_energy(pixels: &[f32], width: usize, height: usize, region: &[f32]) -> (f32, u32) {
    if width == 0 || height == 0 {
        return (0.0, 0);
    }
    let mut luminance = Vec::with_capacity(width * height);
    for index in 0..width * height {
        let slot = index * 3;
        luminance.push(pixels.get(slot..slot + 3).map_or(0.0, |v| luma(triple(v))));
    }
    let radius = bands::radius(width.min(height) as f32, bands::HIGH_RADIUS_FRAC);
    let smoothed = bands::blur(&luminance, width, height, radius);

    let mut total = 0.0f64;
    let mut weight = 0.0f64;
    let mut count = 0u32;
    for index in 0..width * height {
        let coverage = region.get(index).copied().unwrap_or(0.0);
        if coverage <= 0.0 {
            continue;
        }
        count += 1;
        let value = luminance.get(index).copied().unwrap_or(0.0);
        let base = smoothed.get(index).copied().unwrap_or(value);
        total += f64::from((value - base).abs()) * f64::from(coverage);
        weight += f64::from(coverage);
    }
    if weight <= f64::EPSILON {
        return (0.0, count);
    }
    ((total / weight) as f32, count)
}

/// The largest distance any teeth pixel sits outside the locus, and how many were measured.
///
/// Section 10.1's "luminance and chroma stay inside the natural locus", after the fact. The
/// *largest* rather than the mean, because the guarantee is about no tooth rather than about
/// teeth on average, and one pixel driven past the boundary by a resample is exactly the kind of
/// thing a mean hides.
#[must_use]
pub fn teeth_excursion(
    pixels: &[f32],
    width: usize,
    height: usize,
    teeth: &[f32],
    neutral: Option<[f32; 2]>,
    locus: ColourLocus,
) -> (f32, u32) {
    let Some(white) = neutral else {
        // No origin, no measurement - and no correction was made either, so the honest answer is
        // zero excursion over zero samples rather than a number nobody can interpret.
        return (0.0, 0);
    };
    let mut worst = 0.0f32;
    let mut count = 0u32;
    for index in 0..width * height {
        if teeth.get(index).copied().unwrap_or(0.0) < 0.5 {
            continue;
        }
        let slot = index * 3;
        let Some(rgb) = pixels.get(slot..slot + 3).map(triple) else {
            continue;
        };
        if luma(rgb) >= SPECULAR_FLOOR {
            continue;
        }
        count += 1;
        let uv = linear_srgb_to_uv(rgb);
        worst = worst.max(locus.excess(uv[0] - white[0], uv[1] - white[1]));
    }
    (worst, count)
}

/// What fraction of a region is clipped past [`CLIPPED_FLOOR`].
///
/// **The number a borrow is permitted by.** ADR-0043 section 4: you may only borrow pixels that
/// carry no information, and this is how much of a region carries none. Lives here rather than in
/// the decision crate because it is a statement about the pixels the renderer works on, and
/// because the guard measures the same quantity after the composite.
#[must_use]
pub fn clipped_fraction(pixels: &[f32], width: usize, height: usize, region: Box2) -> f32 {
    let window = to_pixels(region, width, height);
    if window.w == 0 || window.h == 0 {
        return 0.0;
    }
    let mut clipped = 0usize;
    let mut total = 0usize;
    for row in 0..window.h {
        for col in 0..window.w {
            let slot = ((window.y + row) * width + (window.x + col)) * 3;
            let Some(rgb) = pixels.get(slot..slot + 3).map(triple) else {
                continue;
            };
            total += 1;
            if luma(rgb) >= CLIPPED_FLOOR {
                clipped += 1;
            }
        }
    }
    if total == 0 {
        return 0.0;
    }
    clipped as f32 / total as f32
}

// ---------------------------------------------------------------------------
// Shared arithmetic
// ---------------------------------------------------------------------------

/// A rectangle of the frame, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Region {
    const fn right(self) -> usize {
        self.x + self.w
    }

    const fn bottom(self) -> usize {
        self.y + self.h
    }
}

/// A normalised rectangle as a pixel window, clamped to the frame.
fn to_pixels(area: Box2, width: usize, height: usize) -> Region {
    let clamped = area.clamped();
    let x = (clamped.x * width as f32).floor().max(0.0) as usize;
    let y = (clamped.y * height as f32).floor().max(0.0) as usize;
    let w = ((clamped.w * width as f32).ceil() as usize)
        .max(1)
        .min(width.saturating_sub(x));
    let h = ((clamped.h * height as f32).ceil() as usize)
        .max(1)
        .min(height.saturating_sub(y));
    Region { x, y, w, h }
}

/// The bounding box of a region's non-zero samples.
fn bounds_of(plane: &[f32], width: usize, height: usize) -> Option<Region> {
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0usize, 0usize);
    for y in 0..height {
        for x in 0..width {
            if plane.get(y * width + x).copied().unwrap_or(0.0) <= 0.0 {
                continue;
            }
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 > x1 || y0 > y1 {
        return None;
    }
    Some(Region {
        x: x0,
        y: y0,
        w: x1 - x0 + 1,
        h: y1 - y0 + 1,
    })
}

/// True when every pixel of a window sits inside a region.
fn fully_inside(region: MicroRegion, window: Region, width: usize, context: &MicroContext) -> bool {
    for row in 0..window.h {
        for col in 0..window.w {
            if context.at(region, (window.y + row) * width + (window.x + col)) < 0.5 {
                return false;
            }
        }
    }
    true
}

/// Copy one window out of the frame as interleaved linear RGB.
fn read(pixels: &[f32], width: usize, height: usize, window: Region) -> Option<Vec<f32>> {
    if window.right() > width || window.bottom() > height || window.w == 0 || window.h == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(window.w * window.h * 3);
    for row in 0..window.h {
        for col in 0..window.w {
            let slot = ((window.y + row) * width + (window.x + col)) * 3;
            match pixels.get(slot..slot + 3) {
                Some(value) => out.extend_from_slice(value),
                None => out.extend_from_slice(&[0.0, 0.0, 0.0]),
            }
        }
    }
    Some(out)
}

/// Mean luminance of an interleaved window.
fn mean_luma(rgb: &[f32], w: usize, h: usize) -> f32 {
    if w == 0 || h == 0 {
        return 0.0;
    }
    let mut total = 0.0f64;
    for pixel in 0..w * h {
        let slot = pixel * 3;
        total += f64::from(rgb.get(slot..slot + 3).map_or(0.0, |v| luma(triple(v))));
    }
    (total / (w * h) as f64) as f32
}

/// A radial feather over a window, one in the middle and zero at the edge.
fn feather(col: usize, row: usize, w: usize, h: usize) -> f32 {
    if w == 0 || h == 0 {
        return 0.0;
    }
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    let rx = cx.max(1.0);
    let ry = cy.max(1.0);
    let dx = (col as f32 - cx) / rx;
    let dy = (row as f32 - cy) / ry;
    let distance = (dx * dx + dy * dy).sqrt();
    let inner = 1.0 - PATCH_FEATHER;
    if distance <= inner {
        return 1.0;
    }
    if distance >= 1.0 {
        return 0.0;
    }
    smooth((1.0 - distance) / PATCH_FEATHER.max(1e-6))
}

/// Smoothstep, so no operator here has a hard edge anywhere.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A linear multiplier from a count of stops.
///
/// The only `powf` in this module, and it produces a *gain* rather than an encoded value - the
/// distinction `crates/aura-render/tests/colour_discipline.rs` cares about.
fn gain(stops: f32) -> f32 {
    2.0f32.powf(stops)
}

/// Rec.709 luminance of a linear triple.
fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// The first three values of a slice, as a triple.
fn triple(values: &[f32]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for (slot, value) in out.iter_mut().zip(values.iter()) {
        *slot = *value;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::IdentityId;

    fn frame(width: usize, height: usize, value: f32) -> Vec<f32> {
        vec![value; width * height * 3]
    }

    fn full(width: usize, height: usize) -> Vec<f32> {
        vec![1.0; width * height]
    }

    #[test]
    fn a_flyaway_loses_contrast_and_the_hair_mass_does_not_move() {
        let (w, h) = (64usize, 64usize);
        let mut pixels = frame(w, h, 0.20);
        // One bright strand down the middle of the window, and a block of "hair mass" beside it.
        for y in 16..48 {
            for slot in 0..3 {
                if let Some(p) = pixels.get_mut((y * w + 32) * 3 + slot) {
                    *p = 0.60;
                }
                if let Some(p) = pixels.get_mut((y * w + 10) * 3 + slot) {
                    *p = 0.60;
                }
            }
        }
        let mut hair = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..20 {
                if let Some(slot) = hair.get_mut(y * w + x) {
                    *slot = 1.0;
                }
            }
        }
        let mut context = MicroContext::empty();
        context.regions.insert(MicroRegion::Hair, hair);

        let before_strand = pixels[(32 * w + 32) * 3];
        let before_mass = pixels[(32 * w + 10) * 3];
        let ops = [MicroOp::Flyaway {
            region: Box2 {
                x: 24.0 / 64.0,
                y: 16.0 / 64.0,
                w: 16.0 / 64.0,
                h: 32.0 / 64.0,
            },
            strength: 0.6,
        }];
        let applied = apply(&mut pixels, w, h, &ops, &context);
        assert_eq!(applied.flyaways, 1);

        let after_strand = pixels[(32 * w + 32) * 3];
        let after_mass = pixels[(32 * w + 10) * 3];
        assert!(
            after_strand < before_strand,
            "the strand kept its contrast: {before_strand} -> {after_strand}"
        );
        assert!(
            after_strand > 0.20,
            "the strand was erased rather than calmed: {after_strand}"
        );
        assert!(
            (after_mass - before_mass).abs() < 1e-6,
            "the hair mass moved: {before_mass} -> {after_mass}"
        );
    }

    #[test]
    fn teeth_are_never_lifted_past_the_brightest_skin_on_the_face() {
        let (w, h) = (32usize, 32usize);
        // Teeth at 0.40, skin at 0.42. A 0.20 EV lift wants 0.46, and the clamp says 0.42.
        let mut pixels = frame(w, h, 0.42);
        let mut teeth = vec![0.0f32; w * h];
        let mut skin = vec![0.0f32; w * h];
        for index in 0..w * h {
            let x = index % w;
            if x < 8 {
                if let Some(slot) = teeth.get_mut(index) {
                    *slot = 1.0;
                }
                for channel in 0..3 {
                    if let Some(p) = pixels.get_mut(index * 3 + channel) {
                        *p = 0.40;
                    }
                }
            } else if let Some(slot) = skin.get_mut(index) {
                *slot = 1.0;
            }
        }
        let mut context = MicroContext::empty();
        context.regions.insert(MicroRegion::Teeth, teeth);
        context.regions.insert(MicroRegion::Skin, skin);
        context.faces.push(FaceGeometry {
            left_eye: [8.0, 8.0],
            right_eye: [24.0, 8.0],
            bbox: Box2 {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
        });

        let ops = [MicroOp::Teeth {
            identity: IdentityId::new(),
            luma: 0.20,
            yellow_reduce: 0.0,
        }];
        apply(&mut pixels, w, h, &ops, &context);
        let tooth = pixels[(4 * w + 2) * 3 + 1];
        assert!(
            tooth <= 0.4201,
            "teeth were lifted past the brightest skin: {tooth}"
        );
        assert!(tooth > 0.40, "teeth were not lifted at all: {tooth}");
    }

    #[test]
    fn a_chromaticity_inside_the_locus_is_left_exactly_alone() {
        let neutral = [0.1978f32, 0.4683f32];
        let locus = ColourLocus {
            du: 0.004,
            dv: 0.008,
            radius: 0.030,
        };
        // A neutral grey is inside a locus centred a few thousandths off neutral with a radius
        // of thirty thousandths.
        let grey = [0.5f32, 0.5, 0.5];
        let out = pull_toward_locus(grey, neutral, locus, 0.35);
        for (before, after) in grey.iter().zip(out.iter()) {
            assert!((before - after).abs() < 1e-5, "{before} -> {after}");
        }
    }

    #[test]
    fn a_yellow_tooth_moves_a_bounded_share_and_never_past_the_boundary() {
        let neutral = [0.1978f32, 0.4683f32];
        let locus = ColourLocus {
            du: 0.004,
            dv: 0.008,
            radius: 0.010,
        };
        let yellow = [0.62f32, 0.55, 0.30];
        let before = linear_srgb_to_uv(yellow);
        let excess_before = locus.excess(before[0] - neutral[0], before[1] - neutral[1]);
        assert!(excess_before > 0.0, "the fixture is not outside the locus");

        let out = pull_toward_locus(yellow, neutral, locus, 0.35);
        let after = linear_srgb_to_uv(out);
        let excess_after = locus.excess(after[0] - neutral[0], after[1] - neutral[1]);
        assert!(
            excess_after < excess_before,
            "{excess_before} -> {excess_after}"
        );
        assert!(
            excess_after > 0.0,
            "a 35 % reduction reached the boundary, which is a full correction"
        );
        // And the luminance did not move: this is a chroma-only operation.
        assert!((luma(out) - luma(yellow)).abs() < 1e-4);
    }

    #[test]
    fn a_catchlight_survives_the_eye_operators() {
        let (w, h) = (24usize, 24usize);
        let mut pixels = frame(w, h, 0.30);
        // A catchlight: four pixels at the top of the iris, well above the specular floor.
        for (x, y) in [(10usize, 8usize), (11, 8), (10, 9), (11, 9)] {
            for channel in 0..3 {
                if let Some(p) = pixels.get_mut((y * w + x) * 3 + channel) {
                    *p = 0.98;
                }
            }
        }
        let mut iris = vec![0.0f32; w * h];
        let mut sclera = vec![0.0f32; w * h];
        for y in 6..16 {
            for x in 6..18 {
                if let Some(slot) = iris.get_mut(y * w + x) {
                    *slot = 1.0;
                }
            }
        }
        for y in 6..16 {
            for x in 0..6 {
                if let Some(slot) = sclera.get_mut(y * w + x) {
                    *slot = 1.0;
                }
            }
        }
        let mut context = MicroContext::empty();
        context.regions.insert(MicroRegion::Iris, iris.clone());
        context.regions.insert(MicroRegion::Sclera, sclera);
        context.neutral = Some([0.1978, 0.4683]);

        let (before, _) = catchlight_peak(&pixels, w, h, &iris);
        let ops = [MicroOp::Eyes {
            identity: IdentityId::new(),
            sclera: 0.30,
            iris_clarity: 0.25,
        }];
        apply(&mut pixels, w, h, &ops, &context);
        let (after, count) = catchlight_peak(&pixels, w, h, &iris);
        assert!(count > 0);
        assert!(
            after >= before - 1e-6,
            "a catchlight was dulled: {before} -> {after}"
        );
    }

    #[test]
    fn glare_reduction_pulls_a_sheet_toward_its_surround_and_stops() {
        let (w, h) = (32usize, 32usize);
        let mut pixels = frame(w, h, 0.25);
        for y in 12..20 {
            for x in 12..20 {
                for channel in 0..3 {
                    if let Some(p) = pixels.get_mut((y * w + x) * 3 + channel) {
                        *p = 1.0;
                    }
                }
            }
        }
        let mut context = MicroContext::empty();
        context.regions.insert(MicroRegion::Eyes, full(w, h));

        let before = pixels[(16 * w + 16) * 3];
        let ops = [MicroOp::Glare {
            region: Box2 {
                x: 10.0 / 32.0,
                y: 10.0 / 32.0,
                w: 12.0 / 32.0,
                h: 12.0 / 32.0,
            },
            method: GlareMethod::Reduce { strength: 0.70 },
        }];
        let applied = apply(&mut pixels, w, h, &ops, &context);
        assert_eq!(applied.glare_reduced, 1);
        let after = pixels[(16 * w + 16) * 3];
        assert!(after < before, "{before} -> {after}");
        assert!(
            after > 0.25,
            "a conservative reduction reached the surround: {after}"
        );
    }

    #[test]
    fn a_borrow_composites_the_patch_it_was_handed_and_nothing_else() {
        let (w, h) = (32usize, 32usize);
        let mut pixels = frame(w, h, 1.0);
        let mut context = MicroContext::empty();
        context.regions.insert(MicroRegion::Eyes, full(w, h));
        context.borrows.push(BorrowPatch {
            x: 12,
            y: 12,
            w: 8,
            h: 8,
            rgb: vec![0.30; 8 * 8 * 3],
        });
        let ops = [MicroOp::Glare {
            region: Box2 {
                x: 12.0 / 32.0,
                y: 12.0 / 32.0,
                w: 8.0 / 32.0,
                h: 8.0 / 32.0,
            },
            method: GlareMethod::BorrowFrom {
                source: aura_core::PhotoId::new(),
                alignment: 0.95,
            },
        }];
        let applied = apply(&mut pixels, w, h, &ops, &context);
        assert_eq!(applied.borrowed, 1);
        assert!(pixels[(15 * w + 15) * 3] < 0.9, "the centre did not change");
        assert!(
            (pixels[(2 * w + 2) * 3] - 1.0).abs() < 1e-6,
            "a pixel outside the patch changed"
        );
    }

    #[test]
    fn the_clipped_fraction_is_what_a_borrow_is_permitted_by() {
        let (w, h) = (16usize, 16usize);
        let mut pixels = frame(w, h, 0.30);
        for y in 4..12 {
            for x in 4..12 {
                for channel in 0..3 {
                    if let Some(p) = pixels.get_mut((y * w + x) * 3 + channel) {
                        *p = 1.0;
                    }
                }
            }
        }
        let region = Box2 {
            x: 4.0 / 16.0,
            y: 4.0 / 16.0,
            w: 8.0 / 16.0,
            h: 8.0 / 16.0,
        };
        assert!(clipped_fraction(&pixels, w, h, region) > 0.99);
        let elsewhere = Box2 {
            x: 0.0,
            y: 0.0,
            w: 4.0 / 16.0,
            h: 4.0 / 16.0,
        };
        assert!(clipped_fraction(&pixels, w, h, elsewhere) < 0.01);
    }

    #[test]
    fn nothing_happens_when_the_region_did_not_arrive() {
        let (w, h) = (16usize, 16usize);
        let mut pixels = frame(w, h, 0.4);
        let before = pixels.clone();
        let ops = [
            MicroOp::Flyaway {
                region: Box2 {
                    x: 0.2,
                    y: 0.2,
                    w: 0.05,
                    h: 0.05,
                },
                strength: 0.6,
            },
            MicroOp::Teeth {
                identity: IdentityId::new(),
                luma: 0.2,
                yellow_reduce: 0.3,
            },
        ];
        let applied = apply(&mut pixels, w, h, &ops, &MicroContext::empty());
        assert_eq!(applied.unregioned, 2);
        assert_eq!(pixels, before);
    }
}
