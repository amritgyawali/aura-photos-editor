//! The processor reference for PHASE-22's restoration stages.
//!
//! `aura-restore` decides *what* a photograph should be repaired with;
//! `aura_core::contract::restore` carries the decision; this module is what turns the decision
//! into pixels. The two WGSL files - `denoise_tile.wgsl` and `deconv.wgsl` - are the same
//! arithmetic for a device, and `crates/aura-render/tests/shader_parity.rs` holds the two to the
//! same constants.
//!
//! ## This module owns two stages, not one
//!
//! Phase 14 froze `graph::ORDER` with `NoiseReduction` at index 6 and `Sharpen` at index 20, and
//! PHASE-22 section 2.1 requires denoise before local retouch and sharpening last. Both are
//! satisfied by putting this phase's denoise at stage 6 and its deconvolution at stage 20, with
//! only face recovery at `Stage::Restoration` in between.
//! `docs/adr/ADR-0047-restoration-denoise-sharpen-and-identity.md` section 2 has the argument,
//! and `crates/aura-render/tests/restoration_order.rs` is section 10.1's render-graph test.
//!
//! ## Why the decision phase calls into the renderer
//!
//! Phase 16 established it for skin colour, phase 20 for skin texture, phase 21 for catchlights,
//! and this is the fourth: **a guarantee about a pixel is enforced on the pixel.**
//! `aura_restore::selfcheck` runs the plan through [`apply`] and measures
//! [`texture_retention`] and [`ringing`] on the buffer the renderer actually produced;
//! `aura_restore::face_recovery` measures the identity distance on a crop of the same buffer. A
//! second implementation of these operators inside the decision crate would make every stored
//! number a statement about a model of the renderer.
//!
//! ## The one idea
//!
//! **Every operator here is conditioned on something measured about this frame.** The denoiser
//! is conditioned on the sensor's own predicted sigma rather than on a strength slider, so the
//! same tier removes different amounts on two bodies at two ISOs; the deconvolution is
//! conditioned on a kernel estimated from this frame's own edges. A module whose amounts came
//! from constants would remove the same amount of noise from a frame that had none.
//!
//! ## Everything here is linear
//!
//! Invariant 8. There is no `powf` in this module outside [`edge_keep`], where it shapes a
//! weight and never an encoded value.

use std::collections::BTreeMap;

use aura_core::contract::restore::{RestoreRegion, SharpenSpec};

use crate::bands;
use crate::spatial;

/// How much wider the chroma radius is than the luminance radius.
///
/// Two and a half. Section 6.1: "Preserve chroma detail separately from luminance detail;
/// wedding fabrics and skin suffer most from chroma smearing." The two halves of that sentence
/// pull in opposite directions and this constant is where they are reconciled: chroma noise is
/// spatially *low-frequency*, so removing it needs a wide radius, and chroma *detail* in fabric
/// is at the same scale as the luminance detail beside it. A wide radius with a strong edge
/// guard removes the first without touching the second; a narrow radius removes neither.
pub const CHROMA_RADIUS_RATIO: f32 = 2.5;

/// The largest radius either half of the denoiser will use, in samples.
///
/// Twelve. Above this a box blur of three passes reaches 36 samples, which on a 2048 px proxy is
/// nearly two per cent of the frame - and a chroma artefact that survives a 36-sample blur is
/// not noise. The cap also bounds `Stage::NoiseReduction`'s halo, which the tiler sums.
pub const MAX_DENOISE_RADIUS: usize = 12;

/// How many multiples of the predicted sigma count as "certainly an edge".
///
/// Three. A luminance step of three sigma is a step the sensor could not have produced by noise,
/// so it is structure and the denoiser leaves it alone. Below one sigma every step is noise.
/// This is the whole of what "conditioned on the noise model" means at the pixel: the *decision*
/// about what to keep is made in units of the sensor's own uncertainty rather than in units of
/// the frame's contrast.
pub const EDGE_SIGMAS: f32 = 3.0;

/// The share of the frame's strongest edges the ringing measurement is taken over.
///
/// A fiftieth. Ringing appears at the strongest edges first and nowhere else, so a measurement
/// averaged over the whole frame is dominated by the flat regions that cannot ring and reports a
/// clean number for a visibly ringing photograph. Phase 20's rule about weighting a region's
/// reading by how far each sample departs from its background, in the form this phase needs.
pub const RINGING_EDGE_FRACTION: f32 = 0.02;

/// The neighbourhood the ringing measurement looks for an overshoot in, in samples.
pub const RINGING_RADIUS: usize = 3;

/// What the renderer is told about the frame it is restoring.
///
/// Three things. The region weights come from phase 18 and decide where the deconvolution may
/// act at all; the face boxes are phase 06's, and bound where face recovery may act; and the
/// predicted sigma is phase 22's own, from the camera's noise model at this frame's ISO.
///
/// An absent sigma is not a default. The denoiser does nothing without one, because a denoiser
/// with no idea how much noise to expect is a blur.
#[derive(Debug, Clone)]
pub struct RestoreContext {
    /// Per-pixel coverage per region, `0..1`, each `width * height` long.
    ///
    /// A missing region is a region the deconvolution may not act through. Same gating as phases
    /// 19, 20 and 21, and here it is a refusal rather than an attenuation - ADR-0047 section 4.
    pub regions: BTreeMap<RestoreRegion, Vec<f32>>,
    /// The predicted noise sigma at diffuse white, in linear working-space units.
    ///
    /// `None` when no noise model could be resolved. The denoiser then does nothing.
    pub sigma: Option<f32>,
    /// Face boxes in pixels, as `(x, y, w, h)`, for the faces the plan recovered.
    pub faces: Vec<(usize, usize, usize, usize)>,
}

impl Default for RestoreContext {
    fn default() -> Self {
        Self::empty()
    }
}

impl RestoreContext {
    /// A context with no regions, no sigma and no faces: every operator becomes a no-op.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            regions: BTreeMap::new(),
            sigma: None,
            faces: Vec::new(),
        }
    }

    /// Coverage of one region at one pixel, `0..1`.
    #[must_use]
    pub fn at(&self, region: RestoreRegion, index: usize) -> f32 {
        self.regions
            .get(&region)
            .and_then(|plane| plane.get(index))
            .copied()
            .unwrap_or(0.0)
    }

    /// True when a region arrived at all.
    #[must_use]
    pub fn has(&self, region: RestoreRegion) -> bool {
        self.regions
            .get(&region)
            .is_some_and(|plane| plane.iter().any(|w| *w > 0.0))
    }

    /// The per-pixel weight the deconvolution acts through, `0..1`.
    ///
    /// Sky and out-of-focus background are zero, skin is `1 - skin_attenuation`, and everything
    /// else is one. Built once per frame rather than per pixel, because the alternative is three
    /// map lookups inside the innermost loop of the most expensive stage in the product.
    #[must_use]
    pub fn sharpen_weights(&self, pixels: usize, skin_attenuation: f32) -> Vec<f32> {
        let mut weights = vec![1.0f32; pixels];
        let skin_keep = (1.0 - skin_attenuation).clamp(0.0, 1.0);
        for index in 0..pixels {
            let mut weight = 1.0f32;
            for region in RestoreRegion::ALL {
                if !region.excluded_from_sharpen() {
                    continue;
                }
                weight = weight.min(1.0 - self.at(region, index).clamp(0.0, 1.0));
            }
            let skin = self.at(RestoreRegion::Skin, index).clamp(0.0, 1.0);
            weight *= skin.mul_add(skin_keep - 1.0, 1.0);
            if let Some(slot) = weights.get_mut(index) {
                *slot = weight.clamp(0.0, 1.0);
            }
        }
        weights
    }
}

/// What one call to [`apply`] did.
///
/// Returned rather than logged, because the decision phase turns it into reasons: an operation
/// the renderer could not perform is a code in the plan, and an operation nobody could tell had
/// been skipped is the failure mode section 12 calls a preview/export mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RestoreApplied {
    /// True when the denoiser ran.
    pub denoised: bool,
    /// True when the deconvolution ran.
    pub sharpened: bool,
    /// Faces the recovery operator acted on.
    pub faces_recovered: u32,
    /// Operations skipped because the region or the sigma they needed was not present.
    pub unconditioned: u32,
}

/// One frame's restoration, as the renderer receives it.
///
/// The three operations, flattened. `aura-restore` builds this from a `RestorePlan`; the
/// renderer never sees the plan, for the reason it never sees a `MicroPlan`: this crate applies
/// decisions and does not make them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RestoreOps {
    /// Luminance reduction, `0..1`. Zero means no luminance pass.
    pub luminance: f32,
    /// Chroma reduction, `0..1`. Zero means no chroma pass.
    pub colour: f32,
    /// How much fine detail is protected against the luminance pass, `0..1`.
    pub detail: f32,
    /// The deconvolution, when there is one.
    pub sharpen: Option<SharpenSpec>,
    /// Face-recovery strength, `0..1`. Zero means no recovery.
    pub face_recovery: f32,
}

impl RestoreOps {
    /// True when nothing here changes a pixel.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.luminance <= 0.0
            && self.colour <= 0.0
            && self.sharpen.is_none()
            && self.face_recovery <= 0.0
    }
}

/// Apply a frame's restoration to a linear RGB buffer, in place.
///
/// The order is fixed and it is the frozen render graph's own: **denoise, then face recovery,
/// then deconvolution**. It is not arbitrary.
///
/// - **Denoise first**, because every operation after it treats what is in the buffer as signal.
///   A recovery or a deconvolution over undenoised pixels amplifies grain into structure, and
///   the structure is then indistinguishable from detail to everything downstream.
/// - **Face recovery next**, because it must run on the face that will be delivered and because
///   the identity measurement has to be taken after the pixels it is about have settled.
/// - **Deconvolution last**, because its amount is capped by what the denoiser left, and because
///   sharpening anything before the last pixel operation means sharpening it twice.
///
/// `pixels` is interleaved linear RGB, `width * height * 3` long.
pub fn apply(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    ops: &RestoreOps,
    context: &RestoreContext,
) -> RestoreApplied {
    let mut applied = RestoreApplied::default();
    if width == 0 || height == 0 || pixels.len() < width * height * 3 {
        return applied;
    }

    if ops.luminance > 0.0 || ops.colour > 0.0 {
        match context.sigma {
            Some(sigma) if sigma > 0.0 => {
                denoise(pixels, width, height, ops, sigma);
                applied.denoised = true;
            }
            // A denoiser with no idea how much noise to expect is a blur. Section 6.1 asks for
            // conditioning and this is the branch where it is missing.
            _ => applied.unconditioned += 1,
        }
    }

    if ops.face_recovery > 0.0 {
        if context.faces.is_empty() {
            applied.unconditioned += 1;
        } else {
            for face in &context.faces {
                recover_face(pixels, width, height, *face, ops.face_recovery);
                applied.faces_recovered += 1;
            }
        }
    }

    if let Some(spec) = &ops.sharpen {
        if spec.amount > 0.0 && spec.mask.from_regions && context.has(RestoreRegion::Subject) {
            let weights = context.sharpen_weights(width * height, spec.skin_attenuation);
            deconvolve(pixels, width, height, spec, &weights);
            applied.sharpened = true;
        } else {
            applied.unconditioned += 1;
        }
    }

    applied
}

// ---------------------------------------------------------------------------
// Denoise
// ---------------------------------------------------------------------------

/// The noise-model-conditioned denoiser.
///
/// Two passes over separated planes, and the thing that makes it this phase's rather than phase
/// 14's is that both read `sigma`.
///
/// **Luminance** is an edge-preserving blend: each pixel is moved toward its blurred value by an
/// amount that falls off with how far its own local step exceeds the sensor's uncertainty. A step
/// of [`EDGE_SIGMAS`] or more is structure and is left where it is; a step under one sigma is
/// noise and is blended fully. The decision is therefore made in units of what the *sensor*
/// could have produced rather than in units of the frame's contrast, which is why the same tier
/// removes visibly different amounts from a clean ISO 100 frame and a dance floor.
///
/// **Chroma** is blurred at [`CHROMA_RADIUS_RATIO`] times the radius and with the same edge
/// guard, because chroma noise is spatially low-frequency and needs the width, while chroma
/// detail in lace sits at the same scale as its luminance detail and needs the guard.
fn denoise(pixels: &mut [f32], width: usize, height: usize, ops: &RestoreOps, sigma: f32) {
    let plane = spatial::luma_plane(pixels, width, height);
    let count = width * height;

    if ops.colour > 0.0 {
        let radius = radius_for(ops.colour * CHROMA_RADIUS_RATIO);
        let mut chroma_r = Vec::with_capacity(count);
        let mut chroma_b = Vec::with_capacity(count);
        for index in 0..count {
            let base = index * 3;
            let l = plane.get(index).copied().unwrap_or(0.0);
            chroma_r.push(pixels.get(base).copied().unwrap_or(0.0) - l);
            chroma_b.push(pixels.get(base + 2).copied().unwrap_or(0.0) - l);
        }
        let blurred_r = bands::blur(&chroma_r, width, height, radius);
        let blurred_b = bands::blur(&chroma_b, width, height, radius);
        // The chroma guard reads the *chroma* difference against the sigma, not the luminance
        // one: a red-to-green step at constant luminance is exactly what chroma noise looks
        // like, and exactly what a coloured thread looks like. The sensor's own sigma is the
        // only thing that separates them.
        for index in 0..count {
            let base = index * 3;
            let l = plane.get(index).copied().unwrap_or(0.0);
            let cr = chroma_r.get(index).copied().unwrap_or(0.0);
            let cb = chroma_b.get(index).copied().unwrap_or(0.0);
            let br = blurred_r.get(index).copied().unwrap_or(cr);
            let bb = blurred_b.get(index).copied().unwrap_or(cb);
            let step = ((cr - br).abs()).max((cb - bb).abs());
            let keep = edge_keep(step, sigma, ops.detail);
            let mix = ops.colour * (1.0 - keep);
            let r = l + cr + (br - cr) * mix;
            let b = l + cb + (bb - cb) * mix;
            let g = pixels.get(base + 1).copied().unwrap_or(0.0);
            let corrected = set_luma([r, g, b], l);
            for (offset, value) in corrected.iter().enumerate() {
                if let Some(slot) = pixels.get_mut(base + offset) {
                    *slot = *value;
                }
            }
        }
    }

    if ops.luminance > 0.0 {
        let radius = radius_for(ops.luminance);
        let blurred = bands::blur(&plane, width, height, radius);
        for index in 0..count {
            let base = index * 3;
            let value = plane.get(index).copied().unwrap_or(0.0);
            let low = blurred.get(index).copied().unwrap_or(value);
            let keep = edge_keep((value - low).abs(), sigma, ops.detail);
            let mix = ops.luminance * (1.0 - keep);
            let target = value + (low - value) * mix;
            let current = [
                pixels.get(base).copied().unwrap_or(0.0),
                pixels.get(base + 1).copied().unwrap_or(0.0),
                pixels.get(base + 2).copied().unwrap_or(0.0),
            ];
            let out = set_luma(current, target.max(0.0));
            for (offset, value) in out.iter().enumerate() {
                if let Some(slot) = pixels.get_mut(base + offset) {
                    *slot = *value;
                }
            }
        }
    }
}

/// How much of a local step is kept, `0..1`, given the sensor's own uncertainty.
///
/// Zero at a step the sensor could easily have produced by noise, one at
/// [`EDGE_SIGMAS`] and above. `detail` sharpens the transition rather than moving it: a
/// photographer asking for more detail is asking to be more suspicious that a step is real, not
/// to redefine what the sensor can do.
#[must_use]
pub fn edge_keep(step: f32, sigma: f32, detail: f32) -> f32 {
    if sigma <= 0.0 {
        return 1.0;
    }
    let ratio = (step / (sigma * EDGE_SIGMAS)).clamp(0.0, 1.0);
    ratio.powf(1.0 / (0.35 + detail.clamp(0.0, 1.0) * 1.3))
}

/// The blur radius one amount asks for, in samples.
fn radius_for(amount: f32) -> usize {
    let radius = 1.0 + amount.clamp(0.0, 4.0) * 3.0;
    (radius.round() as usize).clamp(1, MAX_DENOISE_RADIUS)
}

/// Set a pixel's luminance while keeping its chromaticity.
fn set_luma(rgb: [f32; 3], target: f32) -> [f32; 3] {
    let current = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    if current <= 1e-6 {
        return [target.max(0.0); 3];
    }
    let gain = (target / current).max(0.0);
    [rgb[0] * gain, rgb[1] * gain, rgb[2] * gain]
}

// ---------------------------------------------------------------------------
// Deconvolution
// ---------------------------------------------------------------------------

/// Richardson-Lucy deconvolution with edge-aware damping, through a weight plane.
///
/// Section 6.2: "Use Richardson-Lucy-style deconvolution with a small iteration count and
/// edge-aware damping to avoid ringing". Three things about this implementation are decisions
/// rather than details.
///
/// **It runs on luminance only.** Deconvolving three channels independently produces coloured
/// fringes at every edge, because the three converge at different rates on the same structure.
/// The chromaticity is carried through unchanged, which is also what makes the operator
/// composable with the chroma pass above it.
///
/// **The damping reads the *input* rather than the partially deconvolved value.** Phase 19's
/// defect, and the general rule it wrote: a weight evaluated on an already-edited value is not
/// linear in its own strength, and the failure mode is an operator that is stronger at the edge
/// of its mask than at the centre. `guard` here is computed once, from the original luminance.
///
/// **The result is blended toward the original by `weight * amount`.** Which is what makes the
/// mask a mask: a pixel at weight zero is bit-identical to its input, so an excluded sky is
/// genuinely untouched rather than deconvolved and then mostly blended back.
fn deconvolve(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    spec: &SharpenSpec,
    weights: &[f32],
) {
    let count = width * height;
    let observed = spatial::luma_plane(pixels, width, height);
    let radius = bands::radius(spec.kernel_sigma * 2.0, 1.0).max(1);

    // The damping guard, computed once from the input. See the doc comment.
    let smooth = bands::blur(&observed, width, height, radius);
    let mut guard = Vec::with_capacity(count);
    for index in 0..count {
        let value = observed.get(index).copied().unwrap_or(0.0);
        let low = smooth.get(index).copied().unwrap_or(value);
        // A pixel whose own step is large is a pixel on a strong edge, and a strong edge is
        // where ringing appears. It is damped rather than excluded, because a strong edge is
        // also the only place there is anything to recover.
        guard.push(1.0 / (1.0 + (value - low).abs() * 12.0));
    }

    let mut estimate = observed.clone();
    for _ in 0..spec.iterations.max(1) {
        let reblurred = bands::blur(&estimate, width, height, radius);
        let mut ratio = Vec::with_capacity(count);
        for index in 0..count {
            let o = observed.get(index).copied().unwrap_or(0.0);
            let r = reblurred.get(index).copied().unwrap_or(o);
            ratio.push(if r > 1e-6 {
                (o / r).clamp(0.25, 4.0)
            } else {
                1.0
            });
        }
        let correction = bands::blur(&ratio, width, height, radius);
        for index in 0..count {
            let current = estimate.get(index).copied().unwrap_or(0.0);
            let c = correction.get(index).copied().unwrap_or(1.0);
            let damped = 1.0 + (c - 1.0) * guard.get(index).copied().unwrap_or(1.0);
            if let Some(slot) = estimate.get_mut(index) {
                *slot = (current * damped).max(0.0);
            }
        }
    }

    for index in 0..count {
        let base = index * 3;
        let original = observed.get(index).copied().unwrap_or(0.0);
        let recovered = estimate.get(index).copied().unwrap_or(original);
        let weight = weights.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let mix = weight * spec.amount;
        if mix <= 0.0 {
            continue;
        }
        let target = original + (recovered - original) * mix;
        let current = [
            pixels.get(base).copied().unwrap_or(0.0),
            pixels.get(base + 1).copied().unwrap_or(0.0),
            pixels.get(base + 2).copied().unwrap_or(0.0),
        ];
        let out = set_luma(current, target.max(0.0));
        for (offset, value) in out.iter().enumerate() {
            if let Some(slot) = pixels.get_mut(base + offset) {
                *slot = *value;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Face recovery
// ---------------------------------------------------------------------------

/// Recover fine detail inside one face box, blended at high frequencies.
///
/// **This is not the face-prior model section 6.3 specifies**, and it is not a stand-in for one.
/// `FACE_RECOVERY_HEAD_TRAINED` is false in this build and `aura_restore::face_recovery::solve`
/// returns `None` on every frame, so nothing calls this with a non-zero strength outside the
/// fixtures. It exists so that the identity constraint, the self-check and the store are
/// exercised end to end by something that really does move a face's pixels - which is the only
/// way to know they would catch a model that moved them too far.
/// ADR-0047 section 6 records why there is deliberately no measured fallback for this operation
/// in the product.
fn recover_face(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    face: (usize, usize, usize, usize),
    strength: f32,
) {
    let (fx, fy, fw, fh) = face;
    if fw == 0 || fh == 0 || fx >= width || fy >= height {
        return;
    }
    let w = fw.min(width - fx);
    let h = fh.min(height - fy);
    if w < 4 || h < 4 {
        return;
    }

    let mut crop = Vec::with_capacity(w * h);
    for row in 0..h {
        for column in 0..w {
            let index = (fy + row) * width + (fx + column);
            let base = index * 3;
            let r = pixels.get(base).copied().unwrap_or(0.0);
            let g = pixels.get(base + 1).copied().unwrap_or(0.0);
            let b = pixels.get(base + 2).copied().unwrap_or(0.0);
            crop.push(0.2126 * r + 0.7152 * g + 0.0722 * b);
        }
    }

    // Section 6.3: "blend with the original at high frequencies to keep skin realistic". The
    // low and mid bands are carried through untouched, so nothing here can move a feature - only
    // the band that carries pores and fine lines is scaled, and it is scaled by a strength the
    // contract caps at 0.4.
    let separated = bands::separate(&crop, w, h);
    for row in 0..h {
        for column in 0..w {
            let local = row * w + column;
            let high = separated.high.get(local).copied().unwrap_or(0.0);
            let base_value = crop.get(local).copied().unwrap_or(0.0);
            let target = base_value + high * strength;
            let index = (fy + row) * width + (fx + column);
            let base = index * 3;
            let current = [
                pixels.get(base).copied().unwrap_or(0.0),
                pixels.get(base + 1).copied().unwrap_or(0.0),
                pixels.get(base + 2).copied().unwrap_or(0.0),
            ];
            let out = set_luma(current, target.max(0.0));
            for (offset, value) in out.iter().enumerate() {
                if let Some(slot) = pixels.get_mut(base + offset) {
                    *slot = *value;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The measurements the self-check is built on
// ---------------------------------------------------------------------------

/// High-band energy over a weighted region, and how many samples contributed.
///
/// The numerator and denominator of [`ArtefactReport::texture_retention`]'s ratio, measured on
/// the buffer the renderer produced. Weighted rather than boxed for phase 20's reason: a
/// rectangle around a dress contains a background, and a ratio measured over a box is dominated
/// by the pixels nothing touched.
///
/// [`ArtefactReport::texture_retention`]: aura_core::contract::restore::ArtefactReport::texture_retention
#[must_use]
pub fn texture_energy(pixels: &[f32], width: usize, height: usize, weights: &[f32]) -> (f32, u32) {
    if width == 0 || height == 0 {
        return (0.0, 0);
    }
    let plane = spatial::luma_plane(pixels, width, height);
    let separated = bands::separate(&plane, width, height);
    separated.high_energy_masked(weights)
}

/// The texture retention ratio between two renders of the same frame.
///
/// One is a restoration that cost no texture. Below one it cost some, and
/// `MIN_TEXTURE_RETENTION` is where "some" becomes "the lace".
#[must_use]
pub fn texture_retention(
    before: &[f32],
    after: &[f32],
    width: usize,
    height: usize,
    weights: &[f32],
) -> (f32, u32) {
    let (before_energy, counted) = texture_energy(before, width, height, weights);
    let (after_energy, _) = texture_energy(after, width, height, weights);
    if before_energy <= 1e-7 {
        // A region with no texture in it before cannot have lost any, and dividing by it would
        // report a violation on a frame of a plain wall. Phase 21's rule: a report that measured
        // nothing may not claim a violation.
        return (1.0, counted);
    }
    ((after_energy / before_energy).clamp(0.0, 4.0), counted)
}

/// Mean edge overshoot introduced between two renders, and how many samples it was measured on.
///
/// **What a ringing measurement has to be, and what it must not be.** The naive version -
/// compare the gradient before and after - measures the *size of the sharpening*, because every
/// sharpening increases the step at an edge; that is what sharpening is. Phase 19 made exactly
/// this mistake with its halo test and ADR-0039 section 7 records it.
///
/// What ringing *is*, is a pixel pushed **beyond the range its own neighbourhood had before the
/// operation**: a bright fringe outside the bright side of an edge and a dark one outside the
/// dark side. So this measures, for each of the strongest [`RINGING_EDGE_FRACTION`] of edge
/// pixels, how far the result sits outside the local minimum and maximum the *input* had within
/// [`RINGING_RADIUS`], and averages that excursion. A sharpening that only steepened an edge
/// scores zero however hard it pushed.
#[must_use]
pub fn ringing(before: &[f32], after: &[f32], width: usize, height: usize) -> (f32, u32) {
    if width < 3 || height < 3 {
        return (0.0, 0);
    }
    let source = spatial::luma_plane(before, width, height);
    let result = spatial::luma_plane(after, width, height);
    let gradient = spatial::gradient_plane(&source, width, height);

    // The threshold is a quantile of this frame's own gradients rather than a constant, so a
    // low-contrast frame is measured on its own strongest edges rather than on nothing.
    let mut sorted: Vec<f32> = gradient.iter().copied().filter(|g| *g > 0.0).collect();
    if sorted.is_empty() {
        return (0.0, 0);
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let keep = ((sorted.len() as f32) * (1.0 - RINGING_EDGE_FRACTION)).round() as usize;
    let threshold = sorted
        .get(keep.min(sorted.len() - 1))
        .copied()
        .unwrap_or(f32::MAX);

    let mut total = 0.0f64;
    let mut counted = 0u32;
    let radius = RINGING_RADIUS as isize;
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if gradient.get(index).copied().unwrap_or(0.0) < threshold {
                continue;
            }
            let mut low = f32::MAX;
            let mut high = f32::MIN;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let sx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                    let sy = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                    let value = source.get(sy * width + sx).copied().unwrap_or(0.0);
                    low = low.min(value);
                    high = high.max(value);
                }
            }
            let value = result.get(index).copied().unwrap_or(0.0);
            let excursion = (value - high).max(low - value).max(0.0);
            total += f64::from(excursion);
            counted += 1;
        }
    }
    if counted == 0 {
        return (0.0, 0);
    }
    ((total / f64::from(counted)) as f32, counted)
}

/// A crop of one face, as interleaved linear RGB, for the identity measurement.
///
/// Returned rather than measured here, because the embedding belongs to phase 06 and this crate
/// depends on no vision crate. `aura_restore::face_recovery` hands both crops to the frozen
/// `PeopleService`.
#[must_use]
pub fn face_crop(
    pixels: &[f32],
    width: usize,
    height: usize,
    face: (usize, usize, usize, usize),
) -> (Vec<f32>, usize, usize) {
    let (fx, fy, fw, fh) = face;
    if fw == 0 || fh == 0 || fx >= width || fy >= height {
        return (Vec::new(), 0, 0);
    }
    let w = fw.min(width - fx);
    let h = fh.min(height - fy);
    let mut crop = Vec::with_capacity(w * h * 3);
    for row in 0..h {
        for column in 0..w {
            let base = ((fy + row) * width + (fx + column)) * 3;
            for offset in 0..3 {
                crop.push(pixels.get(base + offset).copied().unwrap_or(0.0));
            }
        }
    }
    (crop, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(width: usize, height: usize) -> Vec<f32> {
        let mut pixels = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let value = if x < width / 2 { 0.15 } else { 0.65 };
                let _ = y;
                pixels.extend_from_slice(&[value, value, value]);
            }
        }
        pixels
    }

    fn speckled(width: usize, height: usize, amplitude: f32) -> Vec<f32> {
        let mut pixels = ramp(width, height);
        for index in 0..width * height {
            // Deterministic pseudo-noise: invariant 4 forbids a seedless generator, and a fixed
            // pattern is what a regression test wants anyway.
            let sign = if (index * 7 + index / width).is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            for offset in 0..3 {
                if let Some(slot) = pixels.get_mut(index * 3 + offset) {
                    *slot = (*slot + sign * amplitude).max(0.0);
                }
            }
        }
        pixels
    }

    #[test]
    fn the_denoiser_does_nothing_without_a_sigma() {
        let mut pixels = speckled(32, 32, 0.02);
        let before = pixels.clone();
        let ops = RestoreOps {
            luminance: 0.5,
            colour: 0.5,
            detail: 0.5,
            ..RestoreOps::default()
        };
        let applied = apply(&mut pixels, 32, 32, &ops, &RestoreContext::empty());
        assert!(!applied.denoised);
        assert_eq!(applied.unconditioned, 1);
        assert_eq!(
            pixels, before,
            "a denoiser with no noise model changed pixels"
        );
    }

    #[test]
    fn the_denoiser_removes_noise_and_keeps_the_edge() {
        let clean = ramp(48, 48);
        let mut pixels = speckled(48, 48, 0.02);
        let context = RestoreContext {
            sigma: Some(0.02),
            ..RestoreContext::empty()
        };
        let ops = RestoreOps {
            luminance: 0.8,
            colour: 0.8,
            detail: 0.3,
            ..RestoreOps::default()
        };
        let applied = apply(&mut pixels, 48, 48, &ops, &context);
        assert!(applied.denoised);

        // Closer to the clean plate than the noisy input was, away from the edge.
        let error = |a: &[f32]| -> f32 {
            let mut total = 0.0f64;
            let mut counted = 0u32;
            for y in 0..48 {
                for x in 0..48 {
                    if (x as i32 - 24).abs() < 4 {
                        continue;
                    }
                    let index = (y * 48 + x) * 3;
                    total += f64::from(
                        (a.get(index).copied().unwrap_or(0.0)
                            - clean.get(index).copied().unwrap_or(0.0))
                        .abs(),
                    );
                    counted += 1;
                }
            }
            (total / f64::from(counted.max(1))) as f32
        };
        let noisy = speckled(48, 48, 0.02);
        assert!(
            error(&pixels) < error(&noisy) * 0.6,
            "denoised error {} against noisy {}",
            error(&pixels),
            error(&noisy)
        );

        // And the edge is still an edge: the step across the middle survives.
        let row = 24 * 48;
        let left = pixels.get((row + 20) * 3).copied().unwrap_or(0.0);
        let right = pixels.get((row + 28) * 3).copied().unwrap_or(0.0);
        assert!(
            right - left > 0.35,
            "the edge was blurred away: {left} {right}"
        );
    }

    #[test]
    fn a_sharpen_with_no_regions_does_nothing() {
        let mut pixels = ramp(32, 32);
        let before = pixels.clone();
        let mut excluded = [false; RestoreRegion::COUNT];
        for (index, region) in RestoreRegion::ALL.iter().enumerate() {
            if region.excluded_from_sharpen() {
                excluded[index] = true;
            }
        }
        let ops = RestoreOps {
            sharpen: Some(SharpenSpec {
                kernel_sigma: 1.0,
                amount: 0.5,
                mask: aura_core::contract::restore::SharpenMask {
                    excluded,
                    coverage: 1.0,
                    from_regions: false,
                },
                skin_attenuation: 0.8,
                iterations: 2,
            }),
            ..RestoreOps::default()
        };
        let applied = apply(&mut pixels, 32, 32, &ops, &RestoreContext::empty());
        assert!(!applied.sharpened);
        assert_eq!(pixels, before);
    }

    #[test]
    fn an_excluded_region_is_bit_identical_afterwards() {
        // What makes the mask a mask. A pixel at weight zero must be untouched rather than
        // deconvolved and mostly blended back, because "mostly" is where a crunchy sky comes
        // from.
        let width = 48;
        let height = 16;
        let mut pixels = ramp(width, height);
        let before = pixels.clone();

        let mut subject = vec![0.0f32; width * height];
        let mut sky = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let index = y * width + x;
                if y < height / 2 {
                    subject[index] = 1.0;
                } else {
                    sky[index] = 1.0;
                }
            }
        }
        let mut regions = BTreeMap::new();
        regions.insert(RestoreRegion::Subject, subject);
        regions.insert(RestoreRegion::Sky, sky);
        let context = RestoreContext {
            regions,
            sigma: None,
            faces: Vec::new(),
        };

        let mut excluded = [false; RestoreRegion::COUNT];
        for (index, region) in RestoreRegion::ALL.iter().enumerate() {
            if region.excluded_from_sharpen() {
                excluded[index] = true;
            }
        }
        let ops = RestoreOps {
            sharpen: Some(SharpenSpec {
                kernel_sigma: 1.2,
                amount: 0.5,
                mask: aura_core::contract::restore::SharpenMask {
                    excluded,
                    coverage: 0.5,
                    from_regions: true,
                },
                skin_attenuation: 0.8,
                iterations: 3,
            }),
            ..RestoreOps::default()
        };
        let applied = apply(&mut pixels, width, height, &ops, &context);
        assert!(applied.sharpened);

        for y in height / 2..height {
            for x in 0..width {
                let base = (y * width + x) * 3;
                assert_eq!(
                    pixels.get(base),
                    before.get(base),
                    "the sky moved at ({x}, {y})"
                );
            }
        }
        let touched =
            (0..width * height / 2).any(|index| pixels.get(index * 3) != before.get(index * 3));
        assert!(touched, "the subject was not sharpened either");
    }

    #[test]
    fn ringing_scores_zero_for_a_steeper_edge_and_more_for_an_overshoot() {
        // The measurement's whole point, as a test. Phase 19's halo defect was a metric that
        // scored the size of the edit; this one has to score only the excursion.
        let width = 32;
        let height = 8;
        let before = ramp(width, height);

        let mut steeper = before.clone();
        for y in 0..height {
            for x in 0..width {
                let base = (y * width + x) * 3;
                let value = if x < width / 2 { 0.15 } else { 0.65 };
                for offset in 0..3 {
                    if let Some(slot) = steeper.get_mut(base + offset) {
                        *slot = value;
                    }
                }
            }
        }
        let (clean, _) = ringing(&before, &steeper, width, height);
        assert!(clean < 1e-5, "a steeper edge scored {clean}");

        let mut ringed = before.clone();
        for y in 0..height {
            for x in 0..width {
                let base = (y * width + x) * 3;
                let overshoot = if x == width / 2 {
                    0.85
                } else if x + 1 == width / 2 {
                    0.02
                } else {
                    continue;
                };
                for offset in 0..3 {
                    if let Some(slot) = ringed.get_mut(base + offset) {
                        *slot = overshoot;
                    }
                }
            }
        }
        let (rung, counted) = ringing(&before, &ringed, width, height);
        assert!(counted > 0);
        assert!(
            rung > clean,
            "an overshoot scored {rung}, no worse than {clean}"
        );
    }

    #[test]
    fn texture_retention_is_one_when_nothing_moved_and_below_one_after_a_blur() {
        let width = 32;
        let height = 32;
        let noisy = speckled(width, height, 0.03);
        let weights = vec![1.0f32; width * height];

        let (same, counted) = texture_retention(&noisy, &noisy, width, height, &weights);
        assert!((same - 1.0).abs() < 1e-4, "{same}");
        assert!(counted > 0);

        let mut smoothed = noisy.clone();
        let context = RestoreContext {
            sigma: Some(0.03),
            ..RestoreContext::empty()
        };
        let ops = RestoreOps {
            luminance: 1.0,
            colour: 1.0,
            detail: 0.0,
            ..RestoreOps::default()
        };
        apply(&mut smoothed, width, height, &ops, &context);
        let (after, _) = texture_retention(&noisy, &smoothed, width, height, &weights);
        assert!(
            after < 1.0,
            "a strong denoise kept all its texture: {after}"
        );
    }

    #[test]
    fn a_flat_region_reports_full_retention_rather_than_a_violation() {
        let width = 8;
        let height = 8;
        let flat = vec![0.4f32; width * height * 3];
        let weights = vec![1.0f32; width * height];
        let (ratio, _) = texture_retention(&flat, &flat, width, height, &weights);
        assert!((ratio - 1.0).abs() < 1e-6);
    }
}
