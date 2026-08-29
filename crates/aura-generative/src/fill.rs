//! Classical content-aware fill: texture that is already in this photograph, moved. Section 6.3.
//!
//! The middle tier, and the one that does most of the work in a build with no diffusion model. It
//! is preferred over inpainting for a reason ADR-0049 section 4 states precisely and that is worth
//! repeating at the top of the file that implements it:
//!
//! > It **cannot hallucinate structure**. It copies patches from the surrounding texture, so its
//! > failure mode is a visible seam or a repeated tuft of grass - ugly, findable, and not a
//! > fabrication. A diffusion model asked to fill the same region can produce a beautiful railing
//! > that was never there. The first failure is caught by a photographer glancing at a thumbnail;
//! > the second is not caught at all.
//!
//! Everything in this module follows from that. There is no learned prior, no generator and no
//! completion of a partially visible object. There is a search for the patch of *this frame* that
//! best continues what is around the hole, and a refusal when no such patch exists.
//!
//! ## The algorithm, and why it fills patches rather than pixels
//!
//! Exemplar-based synthesis, onion-peeled from the boundary inward. Each step picks the unknown
//! position with the most known neighbours, searches the ring around the hole for the source patch
//! whose known samples match best, and copies that patch's whole footprint in.
//!
//! Filling one *pixel* at a time from a weighted neighbourhood average is simpler and produces a
//! blur - the classical diffusion inpaint, which on anything with texture reads as a thumbprint.
//! Copying a whole patch carries the texture's own statistics across with it, which is the entire
//! point: a lawn filled pixel-wise becomes a green smear, and a lawn filled patch-wise stays a
//! lawn that happens to repeat.
//!
//! The priority is known-neighbour count rather than Criminisi's isophote term. The isophote term
//! exists to propagate a linear structure *into* a hole - to continue a railing across it - and
//! that is precisely the behaviour [`crate::safety`] has already refused to allow here, because a
//! continued structure is a structure the fill decided on. Using the term would be building the
//! capability the safety engine blocks upstream, and it would make the failures worse when it did
//! run: a wrongly propagated edge looks deliberate.
//!
//! ## The refusal
//!
//! [`CleanupCode::TextureStructured`] when the surroundings are too directional to copy from. It is
//! measured with a structure tensor over the ring, which is the same instrument phase 09 uses to
//! tell a panned frame from a shaken one - one implementation of "which way does this texture go"
//! per product, arrived at independently and deliberately kept the same shape.
//!
//! ## Everything here is linear
//!
//! Invariant 8. A patch matched in an encoded space matches its surroundings on a screen and not
//! in the file, and the seam appears in the export.

use aura_core::contract::cleanup::{Box2, CleanupCode};

use crate::pixels::{self, Image, Rect};

/// The half-width of a synthesis patch, in pixels.
///
/// Five gives an 11x11 footprint. Small enough that the search finds a genuine match on ordinary
/// wedding backgrounds - carpet, grass, a painted wall, a tablecloth - and large enough to carry
/// texture rather than noise. At a half-width of two the copied content is indistinguishable from
/// a blur; at ten the search rarely finds anything that matches on all sides and the seams get
/// worse rather than better.
pub const PATCH_RADIUS: usize = 5;

/// How far outside the hole the source ring extends, as a multiple of the hole's larger side.
///
/// One and a half, so the source area is about eight times the hole. Bigger would find better
/// matches and would start borrowing from the other side of the subject, which is how a fill ends
/// up putting a piece of somebody's dress into a lawn.
pub const RING_REACH: f32 = 1.5;

/// The largest structure-tensor coherence a fill will work over.
///
/// Coherence is `(l1 - l2) / (l1 + l2)` on the ring's gradient tensor: zero for isotropic texture
/// like grass or carpet, approaching one for a single straight edge. Above this the surroundings
/// are a *direction* rather than a texture, and copying patches across a direction is how a fill
/// produces the warped line the self-check is looking for.
pub const MAX_COHERENCE: f32 = 0.72;

/// How many Jacobi sweeps the seam correction runs.
///
/// A fixed count rather than a convergence test, for invariant 4: a loop that stops when a residual
/// falls below a tolerance produces a different number of sweeps on two machines whose float
/// rounding differs by an ulp, and therefore two different sets of pixels for the same photograph.
///
/// One hundred and twenty-eight is comfortably past visual convergence for a region bounded at 4 %
/// of a proxy: the field being solved is harmonic and its low-frequency part - which is all that
/// matters here - settles in a few dozen sweeps.
pub const SEAM_ITERATIONS: usize = 128;

/// The gradient energy below which the coherence figure is not evidence.
///
/// A flat wall has no gradient, so its tensor is numerically dominated by whatever noise is
/// present and its coherence is meaningless. Below this the region is uniform, which is the
/// easiest thing in the world to fill, and the coherence check is skipped rather than trusted.
///
/// Phase 22's lesson, in its general form: a threshold on a measurement is a statement about the
/// instrument as well as about the world, and this is the floor at which the instrument says
/// anything at all.
pub const MIN_COHERENCE_ENERGY: f32 = 1.0e-4;

/// What the fill measured about the region it worked on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureReport {
    /// Gradient directionality on the ring, `0..1`. Higher is more structured.
    pub coherence: f32,
    /// Mean squared gradient magnitude on the ring.
    pub energy: f32,
    /// How uniform the ring's luminance is, `0..1`. One is a flat wall.
    pub uniformity: f32,
}

impl TextureReport {
    /// True when the surroundings are safe to copy from.
    #[must_use]
    pub fn is_fillable(&self) -> bool {
        self.energy < MIN_COHERENCE_ENERGY || self.coherence <= MAX_COHERENCE
    }
}

/// A completed fill.
#[derive(Debug, Clone, PartialEq)]
pub struct Filled {
    /// The frame with the region replaced.
    pub result: Image,
    /// What the surroundings looked like.
    pub texture: TextureReport,
    /// How many patches were copied.
    pub patches: usize,
}

/// Measure the texture around a region without filling it.
///
/// Separate from [`fill`] because the source selector needs the answer *before* it decides which
/// method to use, and running the synthesis to find out whether the synthesis was allowed would be
/// the same shape of mistake as scoring a candidate before checking it was safe.
#[must_use]
pub fn measure(image: &Image, region: &Box2) -> Option<TextureReport> {
    let rect = pixels::resolve(region, image.w, image.h)?;
    Some(ring_texture(image, &rect))
}

/// Replace a region with texture copied from around it.
///
/// # Errors
///
/// [`CleanupCode::TextureStructured`] when the surroundings are too directional to copy from, and
/// [`CleanupCode::NoAlignedSibling`] is never returned here - a fill has no sibling. A region that
/// does not resolve onto the frame returns [`CleanupCode::TextureStructured`] as well, because the
/// alternative is a fill of a rectangle that is not on the photograph.
pub fn fill(image: &Image, region: &Box2) -> Result<Filled, CleanupCode> {
    if !image.is_well_formed() {
        return Err(CleanupCode::TextureStructured);
    }
    let rect = pixels::resolve(region, image.w, image.h).ok_or(CleanupCode::TextureStructured)?;
    let texture = ring_texture(image, &rect);
    if !texture.is_fillable() {
        return Err(CleanupCode::TextureStructured);
    }

    let reach = ((rect.w.max(rect.h) as f32) * RING_REACH).round() as usize + PATCH_RADIUS + 1;
    let window = rect.grown(reach, image.w, image.h);

    let mut result = image.clone();
    let mut known = vec![true; window.area()];
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            if let Some(slot) = index_of(&window, x, y).and_then(|i| known.get_mut(i)) {
                *slot = false;
            }
        }
    }

    let mut patches = 0usize;
    // Bounded rather than "until done": a hole of `area` pixels needs at most `area` patch copies
    // even if every copy commits exactly one new pixel, and a loop that could run forever on an
    // input nobody has thought of is not a loop that belongs in a background pass.
    let budget = rect.area() + 1;
    while let Some((tx, ty)) = next_target(&window, &known) {
        if patches >= budget {
            break;
        }
        let (sx, sy) = best_source(&result, &window, &known, tx, ty);
        copy_patch(&mut result, &mut known, &window, tx, ty, sx, sy);
        patches += 1;
    }

    // The seam correction. See `seam_correct`.
    seam_correct(&mut result, image, &rect);

    // **There is no feather here, deliberately.** The first version blended the outermost ring of
    // the filled region toward `image`, which inside the region is the object being removed - so
    // the code that existed to hide a seam left a rim of the distraction behind, and the
    // self-check caught it as a terminated gradient. Phase 18's resampler made the same shape of
    // mistake in the code that delivers a mask.
    //
    // A fill needs no feather anyway: `best_source` matches every patch against the *known* samples
    // around it, so each copied patch already agrees with its surroundings where it meets them.
    // What is left is the honest failure mode ADR-0049 section 4 names - a visible seam or a
    // repeated tuft of grass, which is findable and is not a fabrication.

    Ok(Filled {
        result,
        texture,
        patches,
    })
}

/// The index of a frame position inside a window's own row-major buffer.
fn index_of(window: &Rect, x: usize, y: usize) -> Option<usize> {
    if !window.contains(x, y) {
        return None;
    }
    Some((y - window.y) * window.w + (x - window.x))
}

/// The unknown position with the most known neighbours, ties broken by scan order.
fn next_target(window: &Rect, known: &[bool]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize, usize)> = None;
    for y in window.y..window.bottom() {
        for x in window.x..window.right() {
            if index_of(window, x, y)
                .and_then(|i| known.get(i))
                .copied()
                .unwrap_or(true)
            {
                continue;
            }
            let mut count = 0usize;
            for dy in -(PATCH_RADIUS as isize)..=(PATCH_RADIUS as isize) {
                for dx in -(PATCH_RADIUS as isize)..=(PATCH_RADIUS as isize) {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx < 0 || ny < 0 {
                        continue;
                    }
                    if index_of(window, nx as usize, ny as usize)
                        .and_then(|i| known.get(i))
                        .copied()
                        .unwrap_or(false)
                    {
                        count += 1;
                    }
                }
            }
            // Strictly greater keeps the first position in the fixed scan order on a tie, which is
            // what makes two runs of the same photograph produce the same pixels. Invariant 4.
            match best {
                Some((current, _, _)) if current >= count => {}
                _ => best = Some((count, x, y)),
            }
        }
    }
    best.map(|(_, x, y)| (x, y))
}

/// The source patch centre whose known samples best match the target's known samples.
///
/// Sum of squared differences over the samples that are known at *both* ends, normalised by how
/// many those were. Normalising matters: without it the search prefers a source that overlaps only
/// two known target samples, because two agreements are cheaper than forty.
fn best_source(
    image: &Image,
    window: &Rect,
    known: &[bool],
    tx: usize,
    ty: usize,
) -> (usize, usize) {
    let radius = PATCH_RADIUS as isize;
    let mut best = (f32::MAX, tx, ty);
    for sy in (window.y + PATCH_RADIUS)..window.bottom().saturating_sub(PATCH_RADIUS) {
        for sx in (window.x + PATCH_RADIUS)..window.right().saturating_sub(PATCH_RADIUS) {
            // A source patch must be entirely known, or it would propagate the hole.
            if !patch_is_known(window, known, sx, sy) {
                continue;
            }
            let mut sum = 0.0f32;
            let mut count = 0.0f32;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let nx = tx as isize + dx;
                    let ny = ty as isize + dy;
                    if nx < 0 || ny < 0 {
                        continue;
                    }
                    let is_known = index_of(window, nx as usize, ny as usize)
                        .and_then(|i| known.get(i))
                        .copied()
                        .unwrap_or(false);
                    if !is_known {
                        continue;
                    }
                    let a = image.at(nx, ny);
                    let b = image.at(sx as isize + dx, sy as isize + dy);
                    for channel in 0..3 {
                        let d = a.get(channel).copied().unwrap_or(0.0)
                            - b.get(channel).copied().unwrap_or(0.0);
                        sum += d * d;
                    }
                    count += 1.0;
                }
            }
            if count < 1.0 {
                continue;
            }
            let score = sum / count;
            if score < best.0 {
                best = (score, sx, sy);
            }
        }
    }
    (best.1, best.2)
}

/// True when every sample of a patch is known.
fn patch_is_known(window: &Rect, known: &[bool], cx: usize, cy: usize) -> bool {
    let radius = PATCH_RADIUS as isize;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let x = cx as isize + dx;
            let y = cy as isize + dy;
            if x < 0 || y < 0 {
                return false;
            }
            if !index_of(window, x as usize, y as usize)
                .and_then(|i| known.get(i))
                .copied()
                .unwrap_or(false)
            {
                return false;
            }
        }
    }
    true
}

/// Copy the unknown samples of one patch from a source, and mark them known.
fn copy_patch(
    image: &mut Image,
    known: &mut [bool],
    window: &Rect,
    tx: usize,
    ty: usize,
    sx: usize,
    sy: usize,
) {
    let radius = PATCH_RADIUS as isize;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let x = tx as isize + dx;
            let y = ty as isize + dy;
            if x < 0 || y < 0 {
                continue;
            }
            let (x, y) = (x as usize, y as usize);
            let Some(index) = index_of(window, x, y) else {
                continue;
            };
            if known.get(index).copied().unwrap_or(true) {
                continue;
            }
            let value = image.at(sx as isize + dx, sy as isize + dy);
            image.put(x, y, value);
            if let Some(slot) = known.get_mut(index) {
                *slot = true;
            }
        }
    }
}

/// Add a smooth field to the filled region so its boundary continues what is around it exactly.
///
/// Exemplar synthesis matches *texture* and does not match *tone*: the best patch of a smoothly
/// shaded wall is usually taken from a slightly different part of the shading, so the region comes
/// back correct in every local detail and a tenth of a stop out overall. On a busy background that
/// is invisible. On the smooth ones - a painted wall, a sky, a tablecloth - it is a rectangle,
/// and it is exactly what [`crate::selfcheck`] reports as a terminated gradient.
///
/// The fix is the classical one and it is important that it is **only a tone shift**: measure the
/// step across the seam, solve for the harmonic field inside the region that takes those steps as
/// its boundary values, and add it. A harmonic field has no local extrema of its own, so it cannot
/// introduce an edge, a texture or a structure. It can only slide the patch onto its surroundings.
///
/// That distinction is what keeps this inside section 6.3's "classical fill cannot invent
/// structure": every high-frequency sample in the region still came from somewhere else in this
/// photograph, and what was added is the smoothest possible correction to its level.
fn seam_correct(result: &mut Image, original: &Image, rect: &Rect) {
    let w = rect.w;
    let h = rect.h;
    if w < 3 || h < 3 {
        return;
    }

    for channel in 0..3 {
        // The boundary condition: at each pixel of the region's own outer ring, the step between
        // what the fill put there and what its neighbour outside the region actually is.
        let mut field = vec![0.0f32; w * h];
        let mut fixed = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                let on_edge = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
                if !on_edge {
                    continue;
                }
                let px = rect.x + x;
                let py = rect.y + y;
                let mut sum = 0.0f32;
                let mut count = 0.0f32;
                for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                    let nx = px as isize + dx;
                    let ny = py as isize + dy;
                    if nx < 0 || ny < 0 {
                        continue;
                    }
                    // Only neighbours *outside* the region carry evidence; the ones inside it are
                    // the fill talking to itself.
                    if rect.contains(nx as usize, ny as usize) {
                        continue;
                    }
                    sum += original
                        .at(nx, ny)
                        .get(channel)
                        .copied()
                        .unwrap_or(0.0);
                    count += 1.0;
                }
                if count <= 0.0 {
                    continue;
                }
                let here = result
                    .at(px as isize, py as isize)
                    .get(channel)
                    .copied()
                    .unwrap_or(0.0);
                if let Some(slot) = field.get_mut(y * w + x) {
                    *slot = sum / count - here;
                }
                if let Some(slot) = fixed.get_mut(y * w + x) {
                    *slot = true;
                }
            }
        }

        // Jacobi sweeps toward the harmonic interior. Jacobi rather than Gauss-Seidel because it
        // does not depend on the traversal order, so the answer does not change if somebody
        // parallelises the loop later. Invariant 4.
        let mut next = field.clone();
        for _ in 0..SEAM_ITERATIONS {
            for y in 1..h.saturating_sub(1) {
                for x in 1..w.saturating_sub(1) {
                    if fixed.get(y * w + x).copied().unwrap_or(false) {
                        continue;
                    }
                    let mut sum = 0.0f32;
                    for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                        let nx = (x as isize + dx) as usize;
                        let ny = (y as isize + dy) as usize;
                        sum += field.get(ny * w + nx).copied().unwrap_or(0.0);
                    }
                    if let Some(slot) = next.get_mut(y * w + x) {
                        *slot = sum * 0.25;
                    }
                }
            }
            std::mem::swap(&mut field, &mut next);
        }

        for y in 0..h {
            for x in 0..w {
                let correction = field.get(y * w + x).copied().unwrap_or(0.0);
                let mut value = result.at((rect.x + x) as isize, (rect.y + y) as isize);
                if let Some(slot) = value.get_mut(channel) {
                    *slot = (*slot + correction).max(0.0);
                }
                result.put(rect.x + x, rect.y + y, value);
            }
        }
    }
}

/// The structure tensor of the ring around a region.
///
/// The ring rather than the region, because the region is the thing being removed and its own
/// gradients are the distraction's edges. Measuring there would report every candidate as highly
/// structured, which is the same class of mistake as phase 19's weight read off a partly-edited
/// pixel: the measurement must read the input, not the thing under discussion.
fn ring_texture(image: &Image, rect: &Rect) -> TextureReport {
    let reach = (rect.w.max(rect.h) / 2).max(4);
    let ring = rect.grown(reach, image.w, image.h);

    let mut jxx = 0.0f64;
    let mut jyy = 0.0f64;
    let mut jxy = 0.0f64;
    let mut energy = 0.0f64;
    let mut samples = 0.0f64;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;

    for y in ring.y..ring.bottom() {
        for x in ring.x..ring.right() {
            if rect.contains(x, y) {
                continue;
            }
            let gx = image.luma(x as isize + 1, y as isize) - image.luma(x as isize - 1, y as isize);
            let gy = image.luma(x as isize, y as isize + 1) - image.luma(x as isize, y as isize - 1);
            jxx += f64::from(gx * gx);
            jyy += f64::from(gy * gy);
            jxy += f64::from(gx * gy);
            energy += f64::from(gx * gx + gy * gy);
            let value = f64::from(image.luma(x as isize, y as isize));
            sum += value;
            sum_sq += value * value;
            samples += 1.0;
        }
    }

    if samples < 1.0 {
        return TextureReport {
            coherence: 1.0,
            energy: 0.0,
            uniformity: 0.0,
        };
    }

    jxx /= samples;
    jyy /= samples;
    jxy /= samples;
    let trace = jxx + jyy;
    let diff = ((jxx - jyy) * (jxx - jyy) + 4.0 * jxy * jxy).sqrt();
    let coherence = if trace > 1e-12 { diff / trace } else { 0.0 };

    let mean = sum / samples;
    let variance = (sum_sq / samples - mean * mean).max(0.0);
    // A spread of a tenth of a stop reads as uniform; anything with real texture does not.
    let uniformity = (1.0 - variance.sqrt() * 20.0).clamp(0.0, 1.0);

    TextureReport {
        coherence: coherence.clamp(0.0, 1.0) as f32,
        energy: (energy / samples) as f32,
        uniformity: uniformity as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grass(w: usize, h: usize) -> Image {
        // Isotropic texture: two sinusoids at right angles at the same amplitude, so the structure
        // tensor has no dominant direction. A lawn, arithmetically.
        let mut image = Image::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = 0.30
                    + 0.06 * ((x as f32) * 0.9).sin()
                    + 0.06 * ((y as f32) * 0.9).sin()
                    + 0.03 * (((x + y) as f32) * 0.5).cos();
                image.put(x, y, [v * 0.6, v, v * 0.5]);
            }
        }
        image
    }

    fn railing(w: usize, h: usize) -> Image {
        // A single dominant direction: horizontal bars. The thing this module refuses.
        let mut image = Image::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = if (y / 6) % 2 == 0 { 0.65 } else { 0.18 };
                image.put(x, y, [v, v, v]);
            }
        }
        image
    }

    fn hole(x: usize, y: usize, w: usize, h: usize, fw: usize, fh: usize) -> Box2 {
        Box2 {
            x: x as f32 / fw as f32,
            y: y as f32 / fh as f32,
            w: w as f32 / fw as f32,
            h: h as f32 / fh as f32,
        }
    }

    fn paint(image: &mut Image, rect: Rect, value: [f32; 3]) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                image.put(x, y, value);
            }
        }
    }

    /// The lowest and highest luminance anywhere in a frame.
    fn luma_range(image: &Image) -> (f32, f32) {
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for y in 0..image.h {
            for x in 0..image.w {
                let value = image.luma(x as isize, y as isize);
                low = low.min(value);
                high = high.max(value);
            }
        }
        (low, high)
    }

    #[test]
    fn a_bottle_on_grass_is_filled_with_grass() {
        let clean = grass(120, 120);
        let mut frame = clean.clone();
        paint(
            &mut frame,
            Rect {
                x: 50,
                y: 50,
                w: 14,
                h: 14,
            },
            [0.9, 0.9, 0.95],
        );

        let filled = fill(&frame, &hole(50, 50, 14, 14, 120, 120)).expect("grass is fillable");

        // The bright bottle is gone: **every** filled sample sits inside the luminance range the
        // clean grass occupies, rather than at the 0.9 the bottle was painted at. Compared against
        // the range rather than against one arbitrary pixel, because grass varies and picking a
        // single reference sample tests where that sample happened to fall.
        let (low, high) = luma_range(&clean);
        for y in 50..64 {
            for x in 50..64 {
                let value = pixels::luminance(filled.result.at(x, y));
                assert!(
                    value >= low - 0.02 && value <= high + 0.02,
                    "filled sample {value} at {x},{y} is outside the grass range {low}..{high}"
                );
            }
        }
        assert!(filled.patches > 0);
    }

    #[test]
    fn a_railing_is_refused_rather_than_warped() {
        let mut frame = railing(120, 120);
        paint(
            &mut frame,
            Rect {
                x: 50,
                y: 50,
                w: 14,
                h: 14,
            },
            [0.95, 0.1, 0.1],
        );
        assert_eq!(
            fill(&frame, &hole(50, 50, 14, 14, 120, 120)).err(),
            Some(CleanupCode::TextureStructured)
        );
    }

    #[test]
    fn a_flat_wall_is_fillable_even_though_its_coherence_is_meaningless() {
        // Phase 22's lesson: a threshold on a measurement says something about the instrument. A
        // wall has no gradient, so its coherence is noise, and reading that noise as structure
        // would refuse the easiest fill there is.
        let mut frame = Image {
            w: 120,
            h: 120,
            rgb: vec![0.42; 120 * 120 * 3],
        };
        paint(
            &mut frame,
            Rect {
                x: 50,
                y: 50,
                w: 12,
                h: 12,
            },
            [0.95, 0.1, 0.1],
        );
        let filled = fill(&frame, &hole(50, 50, 12, 12, 120, 120)).expect("a wall is fillable");
        let centre = filled.result.at(56, 56);
        assert!((centre[0] - 0.42).abs() < 0.02, "centre was {centre:?}");
        assert!(filled.texture.uniformity > 0.9);
    }

    #[test]
    fn the_fill_is_deterministic() {
        let mut frame = grass(100, 100);
        paint(
            &mut frame,
            Rect {
                x: 40,
                y: 40,
                w: 12,
                h: 12,
            },
            [0.9, 0.9, 0.9],
        );
        let first = fill(&frame, &hole(40, 40, 12, 12, 100, 100)).expect("fills");
        let second = fill(&frame, &hole(40, 40, 12, 12, 100, 100)).expect("fills");
        assert_eq!(first.result, second.result);
        assert_eq!(first.patches, second.patches);
    }

    #[test]
    fn nothing_outside_the_region_is_touched() {
        // The property a content-aware fill is most likely to break quietly, because the search
        // window is much larger than the hole.
        let mut frame = grass(100, 100);
        paint(
            &mut frame,
            Rect {
                x: 40,
                y: 40,
                w: 12,
                h: 12,
            },
            [0.9, 0.9, 0.9],
        );
        let filled = fill(&frame, &hole(40, 40, 12, 12, 100, 100)).expect("fills");
        for y in 0..100 {
            for x in 0..100 {
                if x >= 40 && x < 52 && y >= 40 && y < 52 {
                    continue;
                }
                assert_eq!(
                    filled.result.at(x as isize, y as isize),
                    frame.at(x as isize, y as isize),
                    "the fill wrote outside its own region at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn no_rim_of_the_object_is_left_at_the_boundary() {
        // The regression for the defect `feather_out` documents: the first version blended the
        // outermost ring of the filled region back toward the original, which inside the region is
        // the object. The symptom was a red rim around a fill that was otherwise correct.
        let clean = grass(120, 120);
        let mut frame = clean.clone();
        paint(
            &mut frame,
            Rect {
                x: 50,
                y: 50,
                w: 14,
                h: 14,
            },
            [0.95, 0.10, 0.10],
        );
        let filled = fill(&frame, &hole(50, 50, 14, 14, 120, 120)).expect("grass is fillable");
        for y in 50..64 {
            for x in 50..64 {
                let sample = filled.result.at(x, y);
                assert!(
                    sample[0] <= sample[1] + 0.05,
                    "a red rim survived at {x},{y}: {sample:?}"
                );
            }
        }
    }

    #[test]
    fn a_fill_on_a_smoothly_shaded_wall_leaves_no_step_at_its_own_boundary() {
        // What `seam_correct` is for. An exemplar match on a smooth gradient is correct in texture
        // and a fraction out in tone, and the result is a rectangle - which is the terminated
        // gradient the self-check reports.
        let mut frame = Image::black(120, 120);
        for y in 0..120 {
            for x in 0..120 {
                let v = 0.25 + 0.0025 * x as f32 + 0.0015 * y as f32;
                frame.put(x, y, [v, v, v]);
            }
        }
        let mut with_object = frame.clone();
        paint(
            &mut with_object,
            Rect {
                x: 50,
                y: 50,
                w: 14,
                h: 14,
            },
            [0.95, 0.10, 0.10],
        );
        let filled =
            fill(&with_object, &hole(50, 50, 14, 14, 120, 120)).expect("a wall is fillable");

        // Every step across the seam is inside what the frame's own gradient does anywhere.
        for y in 50..64 {
            let inside = filled.result.luma(50, y);
            let outside = filled.result.luma(49, y);
            assert!(
                (inside - outside).abs() < 0.01,
                "a step of {} at the left seam, row {y}",
                (inside - outside).abs()
            );
        }
    }

    #[test]
    fn measuring_does_not_need_to_fill() {
        let frame = railing(80, 80);
        let report = measure(&frame, &hole(30, 30, 10, 10, 80, 80)).expect("resolves");
        assert!(!report.is_fillable());
        assert!(report.coherence > MAX_COHERENCE, "{}", report.coherence);
    }

    #[test]
    fn the_texture_is_measured_on_the_ring_rather_than_on_the_object() {
        // Phase 19's rule: the measurement must read the input. A hard-edged red block has an
        // enormously coherent gradient field of its own, and measuring inside it would refuse
        // every candidate in the wedding.
        let mut frame = grass(100, 100);
        paint(
            &mut frame,
            Rect {
                x: 40,
                y: 40,
                w: 16,
                h: 16,
            },
            [0.95, 0.02, 0.02],
        );
        let report = measure(&frame, &hole(40, 40, 16, 16, 100, 100)).expect("resolves");
        assert!(
            report.is_fillable(),
            "coherence {} came from the object rather than its surroundings",
            report.coherence
        );
    }
}
