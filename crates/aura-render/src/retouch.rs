//! The processor reference for PHASE-20's retouch stage.
//!
//! `aura-retouch` decides *what* should happen to somebody's skin; `aura_recipe::RetouchOp`
//! carries the decision; this module is what turns the decision into pixels. The three WGSL
//! files - `inpaint_patch.wgsl`, `freq_bands.wgsl` and `retouch_apply.wgsl` - are the same
//! arithmetic for a device, and `crates/aura-render/tests/shader_parity.rs` holds the two to the
//! same constants.
//!
//! ## Why the decision phase calls into the renderer
//!
//! Because section 6.3's texture guarantee is a **post-condition**, and phase 16 already
//! established what that costs: the skin guard measures what the renderer does to a skin pixel
//! rather than what a copy of it would do. `aura_retouch::texture_guard` therefore runs the
//! plan through [`apply`] and measures the band energy of the result. A second implementation
//! of patch synthesis inside the decision crate would make the stored `band_ratio` a statement
//! about a model of the renderer rather than about the renderer, and the number is the phase's
//! headline claim.
//!
//! ## The one idea
//!
//! **Every operator here reconstructs its output so that the skin keeps its own texture
//! energy.** Tone evening scales the mid band and leaves the high band untouched entirely, so
//! its texture ratio is exactly one. Healing replaces both bands of the mark with a donor patch
//! and then rescales the donor texture so the repaired patch carries the same high-band energy
//! the skin around it carries.
//!
//! Section 6.2 words are "blend only the low/mid bands while transplanting the original high
//! band back", and the second half needs care. A blemish is *not* only low and mid frequency: a
//! spot has an edge, and that edge is high-frequency content of the mark rather than of the
//! skin. Putting the literal original high band back therefore puts a third of the blemish back
//! with it - which this module did on its first implementation, and the unit test below is what
//! caught it. What the sentence has to mean, and what a healing brush has always done, is that
//! the repaired patch ends up with the texture *of that skin* rather than smooth. So the
//! texture comes from the donor, which is the same person under the same light, and its
//! amplitude is matched to the ring of skin around the mark.
//!
//! The consequence is worth stating plainly: outside a healed patch every pore is the
//! photograph own, and inside one the pores are that same face own pores borrowed from a few
//! millimetres away. What the texture guard measures - and what section 6.3 gates - is that the
//! energy is still there.
//!
//! ## Everything here is linear
//!
//! Invariant 8. The only `powf` in this module is inside [`gain`], where it produces a
//! *multiplier* from a stop count and never an encoded value.

use aura_core::contract::retouch::{RetouchOp, MAX_EVENING_MID};

use crate::bands;
use crate::local::luma;

/// How far away a donor patch is looked for, as a multiple of the blemish's own radius.
///
/// Two and a fifth. Close enough that the skin is the same person under the same light -
/// section 6.2's requirement - and far enough that the donor is outside the blemish and outside
/// its own halo.
pub const DONOR_DISTANCE: f32 = 2.2;

/// How many directions the donor search tries, evenly spaced.
///
/// Eight, starting at due east and going clockwise. Fixed and ordered, because the tie-break
/// between two equally good donors has to be the same on every machine - invariant 4.
pub const DONOR_DIRECTIONS: usize = 8;

/// The largest low-band difference, in linear luminance, a donor may have from its target.
///
/// Above this the donor is different skin: a different plane of the face, a different light or a
/// different person. Healing from it leaves a patch that is the right texture and the wrong
/// tone, which reads worse than the blemish did.
pub const DONOR_MAX_DELTA: f32 = 0.06;

/// How wide the feather at a healed patch's edge is, as a fraction of its radius.
///
/// A quarter. The transition has to be wider than the high band's own radius or the transplant
/// shows as a ring, and narrower than the patch or the centre never fully heals.
pub const PATCH_FEATHER: f32 = 0.25;

/// Where the transplant boundary sits, as a fraction of the radius of the blemish.
///
/// A third. Everything finer than this is texture and comes from the donor at the amplitude the
/// surrounding skin has; everything coarser is tone and comes from the donor directly. The
/// boundary is a fraction of the mark rather than a fixed number of samples so that a
/// five-pixel spot on a proxy and a fifty-pixel one at full resolution are separated at the
/// same perceptual scale - which is what makes the preview agree with the export.
pub const TRANSPLANT_FRACTION: f32 = 0.35;

/// How far below the eye the periorbital region reaches, as a multiple of the eye separation.
///
/// A fifth of the inter-ocular distance. The region a retoucher lightens is the shadow in the
/// tear trough and the orbital rim, not the cheek: taking it lower is the classic tell, because
/// it flattens the transition into the cheekbone that phase 19 has just shaped.
pub const UNDEREYE_DROP: f32 = 0.20;

/// How far below the surrounding skin a pixel must sit to count as a full shadow.
///
/// Thirty-five per cent of the luminance of the skin around the eye, which is about half a
/// stop. A tear trough on a well-lit face is a fifth to a third down; anything past half a stop
/// is a socket rather than a circle, and lifting one of those flattens the face.
pub const SHADOW_SPAN: f32 = 0.35;

/// How wide that region is, as a multiple of the eye separation.
pub const UNDEREYE_WIDTH: f32 = 0.34;

/// What the caller supplies alongside the operations.
///
/// Two things, and both of them are things the renderer must not invent. The skin weights come
/// from phase 18 and decide where an operator may act at all; the reference chromaticity is the
/// surrounding skin's own, measured by the caller over the frame rather than per operator, so
/// that two blemishes on one cheek are corrected toward the same colour.
#[derive(Debug, Clone)]
pub struct RetouchContext {
    /// Per-pixel skin coverage, `0..1`, `width * height` long.
    ///
    /// Empty means "no skin mask", and every operator then does nothing. That is the same
    /// gating phase 19 applies and for the same reason: an operator that falls back to a
    /// rectangle edits the wall behind somebody's ear.
    pub skin: Vec<f32>,
    /// Eye centres in pixels, left then right, for each face the plan corrected.
    ///
    /// Parallel to the [`RetouchOp::UnderEye`] operations in the plan, in the same order. A
    /// face with no landmarks is not in this list and its operation does nothing - phase 09's
    /// rule that an unknown landmark must never be read as the origin.
    pub eyes: Vec<[[f32; 2]; 2]>,
}

impl RetouchContext {
    /// A context with no mask and no landmarks: every operator becomes a no-op.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            skin: Vec::new(),
            eyes: Vec::new(),
        }
    }

    /// Skin coverage at one pixel, `0..1`.
    #[must_use]
    pub fn skin_at(&self, index: usize) -> f32 {
        self.skin.get(index).copied().unwrap_or(0.0)
    }

    /// True when there is a skin mask at all.
    #[must_use]
    pub fn has_skin(&self) -> bool {
        self.skin.iter().any(|w| *w > 0.0)
    }
}

/// What one call to [`apply`] did.
///
/// Returned rather than logged, because the decision phase turns it into reasons: an operation
/// the renderer could not perform is `RetouchCode::NoDonorPatch` in the plan, and an operation
/// nobody could tell had been skipped is the failure mode section 12 calls "preview/export
/// mismatch".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetouchApplied {
    /// Blemishes healed.
    pub healed: u32,
    /// Blemishes skipped because no donor patch was close enough.
    pub no_donor: u32,
    /// Under-eye regions corrected.
    pub under_eyes: u32,
    /// Evening operations applied.
    pub evened: u32,
    /// Operations skipped because there was no skin mask under them.
    pub unmasked: u32,
}

/// Apply a plan's operations to a linear RGB buffer, in place.
///
/// The order is fixed and is not the plan's order: blemishes first, then under-eye, then
/// evening. Healing before evening matters - a blemish still present when the mid band is
/// scaled is a blemish that gets *spread* rather than removed - and the fixed order is what
/// makes the same plan produce the same pixels on a proxy and at full resolution, which is
/// section 10.1's preview/export agreement.
///
/// `pixels` is interleaved linear RGB, `width * height * 3` long.
pub fn apply(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    ops: &[RetouchOp],
    context: &RetouchContext,
) -> RetouchApplied {
    let mut applied = RetouchApplied::default();
    if width == 0 || height == 0 || pixels.len() < width * height * 3 {
        return applied;
    }
    if !context.has_skin() {
        applied.unmasked = ops.len() as u32;
        return applied;
    }

    for op in ops {
        if let RetouchOp::Blemish { area, strength, .. } = op {
            match heal(pixels, width, height, *area, *strength, context) {
                Healed::Done => applied.healed += 1,
                Healed::NoDonor => applied.no_donor += 1,
            }
        }
    }

    let mut eye_index = 0usize;
    for op in ops {
        if let RetouchOp::UnderEye { luma, chroma, .. } = op {
            if let Some(eyes) = context.eyes.get(eye_index) {
                under_eye(pixels, width, height, *eyes, *luma, *chroma, context);
                applied.under_eyes += 1;
            }
            eye_index += 1;
        }
    }

    for op in ops {
        if let RetouchOp::ToneEvening { strength, .. } = op {
            even_tone(pixels, width, height, *strength, context);
            applied.evened += 1;
        }
    }

    applied
}

/// Whether a heal found somewhere to borrow from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Healed {
    Done,
    NoDonor,
}

/// Remove one blemish by patch synthesis with a high-band transplant.
///
/// Section 6.2 healing-brush equivalent, in four steps:
///
/// 1. find a donor patch of the same skin under the same light, by searching eight directions
///    at [`DONOR_DISTANCE`] and taking the one whose mean luminance is closest;
/// 2. split both patches at the **transplant boundary** - see [`TRANSPLANT_FRACTION`];
/// 3. compose the donor tone with the donor texture, shifting the tone so it matches the skin in
///    the ring around the mark - which is what stops a healed patch reading as a light spot -
///    and scaling the texture so its energy matches the ring as well;
/// 4. blend that over the target with a radial feather, scaled by the skin coverage and the
///    strength of the operation.
///
/// **The texture is the donor, energy-matched, and not the original**, for the reason the module
/// header gives: the high band of a blemish is the edge of the blemish, and putting it back puts
/// the mark back. Both statistics are measured on the *ring* rather than over the whole window,
/// because the mark is inside the window and would otherwise set the target it is being
/// measured against.
fn heal(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    area: aura_core::contract::composition::Box2,
    strength: f32,
    context: &RetouchContext,
) -> Healed {
    let (x0, y0, w, h) = to_pixels(area, width, height);
    if w == 0 || h == 0 || strength <= 0.0 {
        return Healed::Done;
    }
    let radius = ((w.max(h) as f32) * 0.5).max(1.0);
    let transplant = ((radius * TRANSPLANT_FRACTION).round() as usize).max(bands::MIN_RADIUS);
    let margin = transplant * 3;

    let frame = Region {
        x: 0,
        y: 0,
        w: width,
        h: height,
    };
    let window = Region { x: x0, y: y0, w, h };
    let Some(target) = Patch::read(pixels, frame, window, margin) else {
        return Healed::Done;
    };

    let mut best: Option<(f32, Patch)> = None;
    for direction in 0..DONOR_DIRECTIONS {
        let angle = std::f32::consts::TAU * (direction as f32) / (DONOR_DIRECTIONS as f32);
        let dx = (angle.cos() * radius * DONOR_DISTANCE).round() as i64;
        let dy = (angle.sin() * radius * DONOR_DISTANCE).round() as i64;
        let sx = x0 as i64 + dx;
        let sy = y0 as i64 + dy;
        if sx < 0 || sy < 0 {
            continue;
        }
        let donor_window = Region {
            x: sx as usize,
            y: sy as usize,
            w,
            h,
        };
        let Some(donor) = Patch::read(pixels, frame, donor_window, margin) else {
            continue;
        };
        if donor.w != target.w || donor.h != target.h {
            // A donor clipped by the frame edge cannot be composed sample for sample, and
            // resampling one would invent texture. Skip it and try the next direction.
            continue;
        }
        if donor.skin_coverage(context, width) < 0.5 {
            continue;
        }
        let delta = (donor.mean_luma() - target.mean_luma()).abs();
        if delta > DONOR_MAX_DELTA {
            continue;
        }
        // Strictly less than, so the first direction wins a tie. Determinism, invariant 4.
        if best.as_ref().is_none_or(|(score, _)| delta < *score) {
            best = Some((delta, donor));
        }
    }

    let Some((_, donor)) = best else {
        return Healed::NoDonor;
    };

    for channel in 0..3 {
        let target_plane = target.plane(channel);
        let donor_plane = donor.plane(channel);
        let target_smooth = bands::blur(&target_plane, target.w, target.h, transplant);
        let donor_smooth = bands::blur(&donor_plane, target.w, target.h, transplant);

        // The shift and the texture scale are both measured on the ring *outside* the mark.
        // Measured over the whole window they would include the blemish, and the mark would
        // then be setting the tone and the texture it is being corrected toward.
        let shift = ring_mean(&target_smooth, target.w, target.h)
            - ring_mean(&donor_smooth, target.w, target.h);
        let target_texture = ring_texture(&target_plane, &target_smooth, target.w, target.h);
        let donor_texture = ring_texture(&donor_plane, &donor_smooth, target.w, target.h);
        // Clamped, because a donor patch that happens to be unusually smooth would otherwise
        // have its texture multiplied into noise to hit the number.
        let texture_scale = if donor_texture > 1e-6 {
            (target_texture / donor_texture).clamp(0.5, 2.0)
        } else {
            1.0
        };

        for row in 0..target.h {
            for col in 0..target.w {
                let index = row * target.w + col;
                let composed = donor_smooth.get(index).copied().unwrap_or(0.0)
                    + shift
                    + (donor_plane.get(index).copied().unwrap_or(0.0)
                        - donor_smooth.get(index).copied().unwrap_or(0.0))
                        * texture_scale;

                let fx = target.x + col;
                let fy = target.y + row;
                if fx >= width || fy >= height {
                    continue;
                }
                let pixel = fy * width + fx;
                let weight = feather(col, row, target.w, target.h)
                    * strength.clamp(0.0, 1.0)
                    * context.skin_at(pixel);
                if weight <= 0.0 {
                    continue;
                }
                let slot = pixel * 3 + channel;
                if let Some(value) = pixels.get_mut(slot) {
                    *value = (*value * (1.0 - weight) + composed.max(0.0) * weight).max(0.0);
                }
            }
        }
    }

    Healed::Done
}

/// Lift and de-tint one periorbital region.
///
/// Two capped moves and nothing else.
///
/// **Both are measured against the skin around the eye rather than against a constant**, and
/// that is phase 15 rule inherited: there is no ideal under-eye luminance and no ideal
/// under-eye colour anywhere in this module, in the contract or in the schema, because a fixed
/// target is how an editor lightens dark skin while believing it is correcting a shadow. What
/// is corrected is the *separation* between the tear trough and the cheek beside it, which is
/// what a dark circle actually is.
///
/// The luminance weight is therefore a shadow depth relative to the ring around the region,
/// not [`crate::local::luminosity_weight`]. Phase 19 pivot is right for lifting a whole face
/// toward a scene band and wrong here: an under-eye shadow on well-lit skin sits near that
/// pivot, so the weight would be zero on exactly the frames this operation exists for.
fn under_eye(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    eyes: [[f32; 2]; 2],
    luma_ev: f32,
    chroma: f32,
    context: &RetouchContext,
) {
    let separation = (eyes[1][0] - eyes[0][0]).hypot(eyes[1][1] - eyes[0][1]);
    if separation <= f32::EPSILON {
        return;
    }
    let half_w = separation * UNDEREYE_WIDTH * 0.5;
    let drop = separation * UNDEREYE_DROP;

    for eye in eyes {
        let cx = eye[0];
        let cy = eye[1] + drop * 0.5;
        let x0 = (cx - half_w).floor().max(0.0) as usize;
        let x1 = (cx + half_w).ceil().min(width as f32) as usize;
        let y0 = eye[1].floor().max(0.0) as usize;
        let y1 = (eye[1] + drop).ceil().min(height as f32) as usize;

        // The skin around the region: its own chromaticity and its own luminance. Both halves
        // of the correction are measured against these.
        let inner = Region {
            x: x0,
            y: y0,
            w: x1.saturating_sub(x0),
            h: y1.saturating_sub(y0),
        };
        let frame = Region {
            x: 0,
            y: 0,
            w: width,
            h: height,
        };
        let Some(reference) = ring_skin(pixels, frame, inner, context) else {
            continue;
        };

        for y in y0..y1 {
            for x in x0..x1 {
                let pixel = y * width + x;
                let coverage = context.skin_at(pixel);
                if coverage <= 0.0 {
                    continue;
                }
                let ex = (x as f32 - cx) / half_w.max(1.0);
                let ey = (y as f32 - cy) / (drop * 0.5).max(1.0);
                let radial = 1.0 - (ex * ex + ey * ey).sqrt();
                if radial <= 0.0 {
                    continue;
                }
                let weight = smooth(radial) * coverage;

                let slot = pixel * 3;
                let Some(rgb) = pixels.get(slot..slot + 3) else {
                    continue;
                };
                let mut colour = triple(rgb);

                // How much darker than the surrounding skin this pixel is, as a fraction of a
                // full stop below it. One at a stop down or more, zero at the same luminance.
                let own = luma(colour).max(1e-6);
                let depth = ((reference.luma - own) / (reference.luma * SHADOW_SPAN).max(1e-6))
                    .clamp(0.0, 1.0);
                let lift = gain(luma_ev * weight * smooth(depth));
                for channel in &mut colour {
                    *channel *= lift;
                }

                let lifted = luma(colour).max(1e-6);
                for (channel, target) in colour.iter_mut().zip(reference.chroma.iter()) {
                    let own_chroma = *channel / lifted;
                    *channel =
                        lifted * (own_chroma + (target - own_chroma) * chroma.abs() * weight);
                }

                if let Some(out) = pixels.get_mut(slot..slot + 3) {
                    for (slot, value) in out.iter_mut().zip(colour.iter()) {
                        *slot = value.max(0.0);
                    }
                }
            }
        }
    }
}

/// Calm mid-frequency unevenness across the skin, leaving the high band alone.
///
/// The mid band is scaled by `1 - strength * MAX_EVENING_MID` and the low and high bands are
/// put back untouched. Because the reconstruction is exact - `low + mid + high` is the input,
/// which `bands::tests::the_three_bands_sum_back_to_the_input` asserts - a pixel outside the
/// mask or at zero strength comes back bit-identical, and the texture ratio of an evening-only
/// plan is exactly one.
fn even_tone(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    strength: f32,
    context: &RetouchContext,
) {
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.0 {
        return;
    }
    let scale = 1.0 - strength * MAX_EVENING_MID;

    for channel in 0..3 {
        let mut plane = Vec::with_capacity(width * height);
        for pixel in 0..width * height {
            plane.push(pixels.get(pixel * 3 + channel).copied().unwrap_or(0.0));
        }
        let decomposed = bands::separate(&plane, width, height);
        for pixel in 0..width * height {
            let coverage = context.skin_at(pixel);
            if coverage <= 0.0 {
                continue;
            }
            let mid = decomposed.mid.get(pixel).copied().unwrap_or(0.0);
            let evened = decomposed.low.get(pixel).copied().unwrap_or(0.0)
                + mid * scale
                + decomposed.high.get(pixel).copied().unwrap_or(0.0);
            let original = plane.get(pixel).copied().unwrap_or(0.0);
            let blended = original + (evened - original) * coverage;
            if let Some(slot) = pixels.get_mut(pixel * 3 + channel) {
                *slot = blended.max(0.0);
            }
        }
    }
}

/// The luminance plane of a buffer, for the texture guard's own measurement.
///
/// Luminance rather than three channels, because the band ratio is a statement about *texture*
/// and texture is achromatic: a blemish removal that changed only the red channel would still
/// have flattened the pores, and measuring per channel would let a chroma-only operation report
/// a texture loss it did not cause.
#[must_use]
pub fn luma_plane(pixels: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut plane = Vec::with_capacity(width * height);
    for pixel in 0..width * height {
        let slot = pixel * 3;
        let rgb = pixels.get(slot..slot + 3).map_or([0.0f32; 3], triple);
        plane.push(luma(rgb));
    }
    plane
}

/// A rectangular window of the frame, with a margin for the band decomposition.
#[derive(Debug, Clone)]
struct Patch {
    /// Frame x of the window's first column.
    x: usize,
    /// Frame y of the window's first row.
    y: usize,
    /// Window width.
    w: usize,
    /// Window height.
    h: usize,
    /// Interleaved linear RGB of the window.
    rgb: Vec<f32>,
}

impl Patch {
    fn read(pixels: &[f32], frame: Region, window: Region, margin: usize) -> Option<Self> {
        let width = frame.w;
        let height = frame.h;
        let x = window.x.saturating_sub(margin);
        let y = window.y.saturating_sub(margin);
        let w = (window.w + margin * 2).min(width.saturating_sub(x));
        let h = (window.h + margin * 2).min(height.saturating_sub(y));
        if w == 0 || h == 0 {
            return None;
        }
        let mut rgb = Vec::with_capacity(w * h * 3);
        for row in 0..h {
            for col in 0..w {
                let slot = ((y + row) * width + (x + col)) * 3;
                match pixels.get(slot..slot + 3) {
                    Some(value) => rgb.extend_from_slice(value),
                    None => rgb.extend_from_slice(&[0.0, 0.0, 0.0]),
                }
            }
        }
        Some(Self { x, y, w, h, rgb })
    }

    fn plane(&self, channel: usize) -> Vec<f32> {
        let mut plane = Vec::with_capacity(self.w * self.h);
        for pixel in 0..self.w * self.h {
            plane.push(self.rgb.get(pixel * 3 + channel).copied().unwrap_or(0.0));
        }
        plane
    }

    fn mean_luma(&self) -> f32 {
        if self.rgb.is_empty() {
            return 0.0;
        }
        let mut total = 0.0f64;
        for pixel in 0..self.w * self.h {
            let slot = pixel * 3;
            let rgb = self.rgb.get(slot..slot + 3).map_or([0.0f32; 3], triple);
            total += f64::from(luma(rgb));
        }
        (total / (self.w * self.h) as f64) as f32
    }

    fn skin_coverage(&self, context: &RetouchContext, width: usize) -> f32 {
        if self.w == 0 || self.h == 0 {
            return 0.0;
        }
        let mut total = 0.0f64;
        for row in 0..self.h {
            for col in 0..self.w {
                total += f64::from(context.skin_at((self.y + row) * width + (self.x + col)));
            }
        }
        (total / (self.w * self.h) as f64) as f32
    }
}

/// A radial feather over a patch, one in the middle and zero at the edge.
fn feather(col: usize, row: usize, w: usize, h: usize) -> f32 {
    if w == 0 || h == 0 {
        return 0.0;
    }
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    let rx = (cx).max(1.0);
    let ry = (cy).max(1.0);
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

/// A rectangle of the frame, in pixels.
///
/// Exists so the two functions that would otherwise take eight arguments take three. A window
/// is one thing rather than four numbers, and a caller that swapped `y0` and `x1` in an
/// eight-argument call would compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Region {
    /// The right edge, exclusive.
    const fn right(self) -> usize {
        self.x + self.w
    }

    /// The bottom edge, exclusive.
    const fn bottom(self) -> usize {
        self.y + self.h
    }
}

/// The first three values of a slice, as a triple.
///
/// Written with an iterator rather than three indexes because this crate denies
/// `clippy::indexing_slicing` in library code: a slice that is shorter than it should be is a
/// bug, and the right shape for one is a black pixel rather than a panic in a render.
fn triple(values: &[f32]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for (slot, value) in out.iter_mut().zip(values.iter()) {
        *slot = *value;
    }
    out
}

/// Smoothstep, so no operator has a hard edge anywhere in this module.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A linear multiplier from a count of stops.
///
/// The only `powf` in this module, and it produces a *gain* rather than an encoded value -
/// which is the distinction `crates/aura-render/tests/colour_discipline.rs` cares about.
fn gain(stops: f32) -> f32 {
    2.0f32.powf(stops)
}

/// The texture energy of the samples a radial feather does not reach.
///
/// Mean absolute residual over the ring, the same measure [`crate::bands::Bands3::high_energy`]
/// uses, so the number a healed patch is matched to and the number the texture guard checks are
/// the same quantity.
fn ring_texture(values: &[f32], smooth_values: &[f32], w: usize, h: usize) -> f32 {
    let mut total = 0.0f64;
    let mut count = 0usize;
    for row in 0..h {
        for col in 0..w {
            if feather(col, row, w, h) > 0.0 {
                continue;
            }
            let index = row * w + col;
            let value = values.get(index).copied().unwrap_or(0.0);
            let smoothed = smooth_values.get(index).copied().unwrap_or(0.0);
            total += f64::from((value - smoothed).abs());
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    (total / count as f64) as f32
}

/// The mean of the samples a radial feather does not reach.
///
/// The ring around a healed patch: the skin the result has to match. Falls back to the whole
/// window when the feather covers everything, which happens only on a patch two samples across.
fn ring_mean(values: &[f32], w: usize, h: usize) -> f32 {
    let mut total = 0.0f64;
    let mut count = 0usize;
    for row in 0..h {
        for col in 0..w {
            if feather(col, row, w, h) > 0.0 {
                continue;
            }
            total += f64::from(values.get(row * w + col).copied().unwrap_or(0.0));
            count += 1;
        }
    }
    if count == 0 {
        return mean(values);
    }
    (total / count as f64) as f32
}

fn mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let sum: f64 = values.iter().map(|v| f64::from(*v)).sum();
    (sum / values.len() as f64) as f32
}

/// What the skin just outside a rectangle looks like.
///
/// `None` when the ring carries no skin at all, which makes the whole under-eye correction do
/// nothing rather than correct toward whatever the background happens to be. That is the
/// conservative direction and it is the same one every gate in this phase falls in.
#[derive(Debug, Clone, Copy)]
struct RingSkin {
    /// Mean chromaticity: each channel over the luminance of the pixel it came from.
    chroma: [f32; 3],
    /// Mean linear luminance.
    luma: f32,
}

fn ring_skin(
    pixels: &[f32],
    frame: Region,
    inner: Region,
    context: &RetouchContext,
) -> Option<RingSkin> {
    let width = frame.w;
    let (x0, y0) = (inner.x, inner.y);
    let (x1, y1) = (inner.right(), inner.bottom());
    let pad = (inner.w / 3).max(2);
    let rx0 = x0.saturating_sub(pad);
    let ry0 = y0.saturating_sub(pad);
    let rx1 = (x1 + pad).min(frame.w);
    let ry1 = (y1 + pad).min(frame.h);

    let mut total = [0.0f64; 3];
    let mut total_luma = 0.0f64;
    let mut weight = 0.0f64;
    for y in ry0..ry1 {
        for x in rx0..rx1 {
            let inside = x >= x0 && x < x1 && y >= y0 && y < y1;
            if inside {
                continue;
            }
            let pixel = y * width + x;
            let coverage = f64::from(context.skin_at(pixel));
            if coverage <= 0.0 {
                continue;
            }
            let slot = pixel * 3;
            let Some(rgb) = pixels.get(slot..slot + 3) else {
                continue;
            };
            let colour = triple(rgb);
            let l = f64::from(luma(colour).max(1e-6));
            for (slot, value) in total.iter_mut().zip(colour.iter()) {
                *slot += f64::from(*value) / l * coverage;
            }
            total_luma += l * coverage;
            weight += coverage;
        }
    }
    if weight <= f64::EPSILON {
        return None;
    }
    let mut chroma = [0.0f32; 3];
    for (slot, value) in chroma.iter_mut().zip(total.iter()) {
        *slot = (value / weight) as f32;
    }
    Some(RingSkin {
        chroma,
        luma: (total_luma / weight) as f32,
    })
}

/// A normalised rectangle in pixels, clamped to the frame.
fn to_pixels(
    area: aura_core::contract::composition::Box2,
    width: usize,
    height: usize,
) -> (usize, usize, usize, usize) {
    let area = area.clamped();
    let x = (area.x * width as f32).floor().max(0.0) as usize;
    let y = (area.y * height as f32).floor().max(0.0) as usize;
    let w = (area.w * width as f32).ceil().max(1.0) as usize;
    let h = (area.h * height as f32).ceil().max(1.0) as usize;
    (
        x.min(width.saturating_sub(1)),
        y.min(height.saturating_sub(1)),
        w.min(width.saturating_sub(x)),
        h.min(height.saturating_sub(y)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::composition::Box2;
    use aura_core::contract::retouch::{FreqBand, InpaintMethod};

    /// A synthetic cheek: even skin with pore-scale texture painted into it.
    fn cheek(width: usize, height: usize) -> Vec<f32> {
        let mut pixels = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let pore = if (x + y) % 2 == 0 { 0.012 } else { -0.012 };
                let base = 0.34 + pore;
                pixels.extend_from_slice(&[base * 1.18, base, base * 0.82]);
            }
        }
        pixels
    }

    fn paint_blemish(pixels: &mut [f32], width: usize, cx: usize, cy: usize, radius: f32) {
        let height = pixels.len() / (width * 3);
        for y in 0..height {
            for x in 0..width {
                let d = ((x as f32 - cx as f32).powi(2) + (y as f32 - cy as f32).powi(2)).sqrt();
                if d > radius {
                    continue;
                }
                let amount = 1.0 - d / radius;
                let slot = (y * width + x) * 3;
                pixels[slot] += 0.09 * amount;
                pixels[slot + 1] -= 0.02 * amount;
                pixels[slot + 2] -= 0.02 * amount;
            }
        }
    }

    fn all_skin(width: usize, height: usize) -> RetouchContext {
        RetouchContext {
            skin: vec![1.0; width * height],
            eyes: Vec::new(),
        }
    }

    #[test]
    fn healing_removes_the_colour_and_keeps_the_texture() {
        let (w, h) = (96, 96);
        let clean = cheek(w, h);
        let mut pixels = clean.clone();
        paint_blemish(&mut pixels, w, 48, 48, 5.0);

        let before = bands::separate(&luma_plane(&pixels, w, h), w, h).high_energy();
        let redness_before = pixels[(48 * w + 48) * 3] - pixels[(48 * w + 48) * 3 + 1];

        let ops = vec![RetouchOp::Blemish {
            area: Box2 {
                x: 42.0 / w as f32,
                y: 42.0 / h as f32,
                w: 12.0 / w as f32,
                h: 12.0 / h as f32,
            },
            method: InpaintMethod::Patch,
            strength: 1.0,
        }];
        let applied = apply(&mut pixels, w, h, &ops, &all_skin(w, h));
        assert_eq!(applied.healed, 1);

        let redness_after = pixels[(48 * w + 48) * 3] - pixels[(48 * w + 48) * 3 + 1];
        assert!(
            redness_after < redness_before * 0.5,
            "the mark survived: {redness_before} -> {redness_after}"
        );

        let after = bands::separate(&luma_plane(&pixels, w, h), w, h).high_energy();
        assert!(
            after / before >= 0.90,
            "texture ratio {} is below the floor",
            after / before
        );
    }

    #[test]
    fn evening_leaves_the_high_band_exactly_where_it_was() {
        // The strongest statement this module makes: tone evening reconstructs from `low + mid *
        // k + high`, so the high band comes back untouched however hard the operation runs.
        let (w, h) = (64, 64);
        let mut pixels = cheek(w, h);
        // A blotch: a broad, low-contrast lift over a quarter of the region.
        for y in 10..40 {
            for x in 10..40 {
                let slot = (y * w + x) * 3;
                for channel in 0..3 {
                    pixels[slot + channel] += 0.03;
                }
            }
        }
        let before = bands::separate(&luma_plane(&pixels, w, h), w, h);
        let mid_before = before.mid_energy();
        let high_before = before.high_energy();

        let ops = vec![RetouchOp::ToneEvening {
            mask: aura_core::MaskId::from_db("msk_00000000-0000-4000-8000-000000000020")
                .expect("a mask id"),
            strength: 1.0,
            band: FreqBand::Mid,
        }];
        apply(&mut pixels, w, h, &ops, &all_skin(w, h));

        let after = bands::separate(&luma_plane(&pixels, w, h), w, h);
        assert!(
            after.mid_energy() < mid_before,
            "evening did not calm the blotch"
        );
        assert!(
            (after.high_energy() / high_before) > 0.97,
            "evening cost texture: {} -> {}",
            high_before,
            after.high_energy()
        );
    }

    #[test]
    fn nothing_happens_without_a_skin_mask() {
        let (w, h) = (48, 48);
        let mut pixels = cheek(w, h);
        let before = pixels.clone();
        let ops = vec![RetouchOp::Blemish {
            area: Box2 {
                x: 0.4,
                y: 0.4,
                w: 0.1,
                h: 0.1,
            },
            method: InpaintMethod::Patch,
            strength: 1.0,
        }];
        let applied = apply(&mut pixels, w, h, &ops, &RetouchContext::empty());
        assert_eq!(applied.unmasked, 1);
        assert_eq!(pixels, before);
    }

    #[test]
    fn an_under_eye_lift_is_bounded_and_acts_on_the_shadow() {
        let (w, h) = (96, 96);
        let mut pixels = cheek(w, h);
        // A darker band under the left eye.
        for y in 40..52 {
            for x in 24..48 {
                let slot = (y * w + x) * 3;
                for channel in 0..3 {
                    pixels[slot + channel] *= 0.6;
                }
            }
        }
        let shadow_before = pixels[(42 * w + 36) * 3 + 1];
        let cheek_before = pixels[(80 * w + 36) * 3 + 1];

        let mut context = all_skin(w, h);
        context.eyes = vec![[[36.0, 38.0], [66.0, 38.0]]];
        let ops = vec![RetouchOp::UnderEye {
            identity: aura_core::IdentityId::from_db("idt_00000000-0000-4000-8000-000000000020")
                .expect("an identity"),
            luma: 0.25,
            chroma: 0.10,
        }];
        apply(&mut pixels, w, h, &ops, &context);

        let shadow_after = pixels[(42 * w + 36) * 3 + 1];
        let cheek_after = pixels[(80 * w + 36) * 3 + 1];
        assert!(shadow_after > shadow_before, "the shadow was not lifted");
        assert!(
            shadow_after / shadow_before <= 2.0f32.powf(0.25) + 1e-3,
            "the lift exceeded its cap"
        );
        assert!(
            (cheek_after - cheek_before).abs() < 1e-6,
            "the correction reached the cheek"
        );
    }

    #[test]
    fn application_is_deterministic() {
        let (w, h) = (64, 64);
        let mut a = cheek(w, h);
        paint_blemish(&mut a, w, 32, 32, 4.0);
        let mut b = a.clone();
        let ops = vec![RetouchOp::Blemish {
            area: Box2 {
                x: 0.44,
                y: 0.44,
                w: 0.12,
                h: 0.12,
            },
            method: InpaintMethod::Patch,
            strength: 0.8,
        }];
        apply(&mut a, w, h, &ops, &all_skin(w, h));
        apply(&mut b, w, h, &ops, &all_skin(w, h));
        assert_eq!(a, b);
    }
}
