//! Borrowing real pixels from a sibling frame of the same moment. Section 6.3, and the method
//! this phase prefers above every other.
//!
//! A wedding is shot in bursts. The frame with the caterer's crate at the edge has a neighbour a
//! third of a second away in which the crate is out of shot, or the camera moved twelve pixels and
//! the wall behind it is visible. Those pixels are a **record of the room**. Copying them is not
//! generation at all, and the disclosure that says so is a different sentence from the one a fill
//! gets - which is why [`CleanupMethod::BorrowFrom`] carries its source in the type rather than in
//! a comment.
//!
//! ## What this module refuses, and why each refusal exists
//!
//! Four, and they are in the order they can be decided cheaply:
//!
//! 1. **Too few correspondences.** Fewer than [`MIN_CORRESPONDENCES`] ring patches matched, so
//!    there is no geometry to fit. Usually a featureless wall, where a homography is unidentified
//!    even though every patch "matched" - which is why a flat window scores zero in
//!    [`crate::pixels::ncc`] rather than one.
//! 2. **The fit does not hold.** The best homography leaves a median residual above
//!    [`MAX_RESIDUAL_PX`]. The two frames are of different things, or somebody walked through.
//! 3. **The object is in the sibling too.** The whole point of the search is a frame *without* the
//!    distraction, and a burst neighbour usually has it in very nearly the same place. This is the
//!    refusal that fires most often on a real wedding and the one a naive implementation forgets:
//!    it aligns perfectly, it borrows, and it replaces the exit sign with the exit sign.
//! 4. **Nothing to borrow.** The warp lands outside the sibling's own frame, so the pixels that
//!    would be copied are the sibling's clamped edge repeated - which is a smear, produced by the
//!    method that exists to avoid inventing anything.
//!
//! ## The fit is deterministic, and that is a requirement rather than a preference
//!
//! Invariant 4: the same inputs and the same versions produce a byte-identical recipe. A RANSAC
//! seeded from a clock produces a different homography on the second run and therefore a different
//! stored `CleanupDisclosure`, and a delivered file that cannot be re-created from its four values
//! fails phase 14's rule.
//!
//! So the robust fit is an **exhaustive** least-median-of-squares over every four-subset of at
//! most [`MAX_CONTROL_POINTS`] correspondences - 495 candidate homographies at twelve points, each
//! a 8x8 solve - rather than a sampled one. It is bounded, it is reproducible, and at this size it
//! is cheaper than seeding a generator.
//!
//! Least median rather than least squares for the reason phase 23 learned the hard way with its
//! distortion fit: **a trimmed mean rejects the correspondences with the largest residual, and on
//! a frame where somebody walked past, those are the correct ones.** The median is not moved by up
//! to half the points being wrong, and only after it has chosen does a least-squares refit run
//! over the inliers.
//!
//! [`CleanupMethod::BorrowFrom`]: aura_core::contract::cleanup::CleanupMethod::BorrowFrom

use aura_core::contract::cleanup::{Box2, CleanupCode, ImageId};

use crate::pixels::{self, Image, Rect};

/// How far outside the region the ring of control patches sits, as a share of the region's
/// larger side.
///
/// Far enough to be outside the distraction and its own soft edge, near enough that the
/// homography is fitted on the geometry immediately around the patch rather than on the far side
/// of the room. A homography fitted on the whole frame is the right transform for the frame and
/// the wrong one for a forty-pixel rectangle in the corner of it, because a real camera move
/// between two frames is not exactly planar.
pub const RING_OFFSET: f32 = 0.60;

/// The half-width of a control patch, in pixels.
pub const PATCH_RADIUS: isize = 6;

/// How far the block match searches, in pixels.
///
/// Generous, because two frames of a burst can differ by a hand-held camera's whole drift. The
/// cost is quadratic in this number and it is the reason the search runs on a proxy rather than at
/// full resolution.
pub const SEARCH_RADIUS: isize = 24;

/// The correlation a control patch needs before it is a correspondence at all.
pub const MIN_PATCH_NCC: f32 = 0.70;

/// The fewest correspondences a homography may be fitted from.
///
/// Six rather than four, so the least-median fit has something to be robust *about*. Four
/// correspondences and a four-point model is an interpolation with no residual and therefore no
/// evidence, and it would report a perfect fit on four points that all happened to match a
/// repeating tile pattern in the wrong place.
pub const MIN_CORRESPONDENCES: usize = 6;

/// The most correspondences the exhaustive fit considers.
///
/// Twelve gives 495 four-subsets. Sixteen would give 1,820 and buy very little: the points are
/// spread around one small ring, so beyond a dozen they stop being independent evidence about the
/// transform and start being independent evidence about the same two edges.
pub const MAX_CONTROL_POINTS: usize = 12;

/// The largest median reprojection residual a borrow may carry, in pixels.
pub const MAX_RESIDUAL_PX: f32 = 1.75;

/// How little the aligned sibling region may differ from the target region before the object is
/// judged to be in both, as a share of the surrounding texture's own spread.
///
/// See refusal 3 in the module header. **This is a difference rather than a correlation, and the
/// first implementation got that wrong in a way that passed exactly the case it existed to catch.**
///
/// A correlation is undefined over a flat window and [`crate::pixels::ncc`] returns zero for one,
/// which is the right answer to "do these two windows have the same structure" and the wrong answer
/// to "is this the same object". A gaffer-taped cable, an exit sign and a caterer's crate are all
/// close to flat, so a burst neighbour containing the identical object correlated at **zero** and
/// the borrow went ahead - replacing the exit sign with the exit sign, which is the single failure
/// this refusal is written for.
///
/// The scale is the ring's own luminance spread rather than an absolute, so the same number is
/// right on a proxy of a dim ceremony and on a bright lawn. Half, because a region that differs
/// from its sibling by less than half of what the surrounding texture varies by has not
/// meaningfully changed between the two frames.
pub const SAME_OBJECT_DIFFERENCE: f32 = 0.5;

/// The smallest luminance spread that counts as texture at all.
///
/// Below this the ring is a flat wall, its spread is measurement noise, and dividing by it would
/// turn any difference at all into a large ratio. Phase 22's rule: a threshold on a measurement is
/// a statement about the instrument, and this is where the instrument stops saying anything.
pub const MIN_RING_SPREAD: f32 = 0.004;

/// The share of the region's smaller side that is feathered at the seam.
///
/// The band sits **outside** the region rather than inside it - see [`crate::pixels::feather_out`]
/// for the defect that distinction fixes - so the borrowed pixels cover the whole object at full
/// weight and the transition happens on background that is genuinely in both frames.
pub const FEATHER_SHARE: f32 = 0.18;

/// A homography, row major, with `h[8]` normalised to one.
pub type Homography = [f32; 9];

/// What the fit found.
#[derive(Debug, Clone, PartialEq)]
pub struct Alignment {
    /// The transform taking a target position to a sibling position.
    pub matrix: Homography,
    /// How many correspondences the refit used.
    pub inliers: usize,
    /// How many were offered to it.
    pub offered: usize,
    /// The median reprojection residual over every correspondence, in pixels.
    pub residual_px: f32,
    /// The mean correlation of the inlying control patches, `0..1`.
    pub patch_ncc: f32,
}

impl Alignment {
    /// How much this alignment can be trusted, `0..1`.
    ///
    /// Three terms multiplied rather than averaged - the geometric shape phases 09, 11, 12 and 18
    /// all use, for the reason they use it: a zero in any one of them is a zero overall, and no
    /// term may rescue another. An alignment with twelve inliers, a perfect correlation and a
    /// three-pixel residual is not a good alignment that is slightly off; it is a wrong one that
    /// found a lot of agreement about being wrong, which is exactly what a repeating pattern
    /// produces.
    #[must_use]
    pub fn confidence(&self) -> f32 {
        if self.offered == 0 {
            return 0.0;
        }
        let support = (self.inliers as f32 / self.offered as f32).clamp(0.0, 1.0);
        let geometry = (1.0 - self.residual_px / MAX_RESIDUAL_PX).clamp(0.0, 1.0);
        let photometric =
            ((self.patch_ncc - MIN_PATCH_NCC) / (1.0 - MIN_PATCH_NCC)).clamp(0.0, 1.0);
        (support * geometry * photometric).clamp(0.0, 1.0)
    }
}

/// A completed borrow.
#[derive(Debug, Clone, PartialEq)]
pub struct Borrowed {
    /// The photograph the pixels came from.
    pub source: ImageId,
    /// The frame with the region replaced.
    pub result: Image,
    /// What the fit found.
    pub alignment: Alignment,
    /// The gain applied to the borrowed pixels to match this frame's exposure.
    pub gain: f32,
    /// The offset applied after the gain.
    pub offset: f32,
}

/// One control patch and where it was found in the sibling.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Correspondence {
    tx: f32,
    ty: f32,
    sx: f32,
    sy: f32,
    score: f32,
}

/// Try to replace a region with real pixels from one sibling frame.
///
/// # Errors
///
/// One of four codes, each of which is stored on the proposal rather than logged: a caller that
/// could not say *why* a borrow was declined would fall through to a fill without recording that
/// the better method was tried, and the delivery report would then be unable to answer the one
/// question a photographer asks about a cleanup, which is where the pixels came from.
pub fn borrow(
    target: &Image,
    sibling: &Image,
    source: ImageId,
    region: &Box2,
) -> Result<Borrowed, CleanupCode> {
    if !target.is_well_formed() || !sibling.is_well_formed() {
        return Err(CleanupCode::NoAlignedSibling);
    }
    let rect = pixels::resolve(region, target.w, target.h).ok_or(CleanupCode::NoAlignedSibling)?;

    let points = control_points(target, &rect);
    let matched = match_points(target, sibling, &points);
    if matched.len() < MIN_CORRESPONDENCES {
        return Err(CleanupCode::NoAlignedSibling);
    }

    let alignment = fit(&matched).ok_or(CleanupCode::NoAlignedSibling)?;
    if alignment.residual_px > MAX_RESIDUAL_PX {
        return Err(CleanupCode::NoAlignedSibling);
    }

    // Refusal 4: the region maps off the sibling's own frame, so what would be copied is its
    // clamped edge repeated.
    if !lands_inside(&alignment.matrix, &rect, sibling) {
        return Err(CleanupCode::NoAlignedSibling);
    }

    // The warped sibling region, before any photometric correction, so the presence test compares
    // structure rather than exposure.
    //
    // Warped over the region **grown by the feather band**, because the band is outside the object
    // and the pixels that fill it have to come from somewhere.
    let band = ((rect.w.min(rect.h) as f32 * FEATHER_SHARE).round() as usize).max(1);
    let outer = rect.grown(band, target.w, target.h);
    let warped = warp_region(sibling, &alignment.matrix, &outer);

    // Refusal 3: the object is in the sibling too. See the module header and
    // `SAME_OBJECT_DIFFERENCE`.
    if !differs_enough(
        target,
        &warped,
        rect_ring_spread(target, &rect),
        &rect,
        &outer,
    ) {
        return Err(CleanupCode::NoAlignedSibling);
    }

    let (gain, offset) = photometry(target, sibling, &alignment.matrix, &rect);
    let result = composite(target, &warped, &rect, &outer, band, gain, offset);

    Ok(Borrowed {
        source,
        result,
        alignment,
        gain,
        offset,
    })
}

/// The luminance spread of the ring around a region.
///
/// The ring rather than the region, because the region is the object and the question is how much
/// this photograph's *background* varies. Phase 19's rule: the measurement reads the input.
fn rect_ring_spread(image: &Image, rect: &Rect) -> f32 {
    let ring = rect.grown(rect.w.max(rect.h) / 2 + 2, image.w, image.h);
    let mut n = 0.0f32;
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    for y in ring.y..ring.bottom() {
        for x in ring.x..ring.right() {
            if rect.contains(x, y) {
                continue;
            }
            let value = image.luma(x as isize, y as isize);
            sum += value;
            sum_sq += value * value;
            n += 1.0;
        }
    }
    if n < 8.0 {
        return MIN_RING_SPREAD;
    }
    let mean = sum / n;
    ((sum_sq / n - mean * mean).max(0.0))
        .sqrt()
        .max(MIN_RING_SPREAD)
}

/// True when the aligned sibling shows something different from what is in the region.
///
/// A mean absolute luminance difference, scaled by what the surroundings vary by. See
/// [`SAME_OBJECT_DIFFERENCE`] for why this is not a correlation.
fn differs_enough(target: &Image, warped: &Image, spread: f32, rect: &Rect, outer: &Rect) -> bool {
    let mut n = 0.0f32;
    let mut sum = 0.0f32;
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let a = target.luma(x as isize, y as isize);
            let b = warped.luma((x - outer.x) as isize, (y - outer.y) as isize);
            sum += (a - b).abs();
            n += 1.0;
        }
    }
    if n <= 0.0 {
        return false;
    }
    (sum / n) >= spread * SAME_OBJECT_DIFFERENCE
}

/// Where the control patches sit: a ring outside the region, at twelve fixed angles.
///
/// Fixed angles rather than corner detection, because a corner detector would place every point on
/// the distraction's own edges - which are the highest-contrast structure anywhere near it, and are
/// exactly the pixels that do not exist in a sibling frame showing clean background.
fn control_points(image: &Image, rect: &Rect) -> Vec<(f32, f32)> {
    let cx = rect.x as f32 + rect.w as f32 * 0.5;
    let cy = rect.y as f32 + rect.h as f32 * 0.5;
    let rx = rect.w as f32 * (0.5 + RING_OFFSET);
    let ry = rect.h as f32 * (0.5 + RING_OFFSET);
    let mut out = Vec::with_capacity(MAX_CONTROL_POINTS);
    for index in 0..MAX_CONTROL_POINTS {
        let angle = std::f32::consts::TAU * (index as f32) / (MAX_CONTROL_POINTS as f32);
        let x = cx + rx * angle.cos();
        let y = cy + ry * angle.sin();
        // A patch whose window would leave the frame is dropped rather than clamped: clamping
        // would put two control points at the same place, and two identical rows make the eight
        // by eight solve singular.
        let pad = PATCH_RADIUS as f32;
        if x - pad >= 0.0 && y - pad >= 0.0 && x + pad < image.w as f32 && y + pad < image.h as f32
        {
            out.push((x, y));
        }
    }
    out
}

/// Block-match every control patch into the sibling.
fn match_points(target: &Image, sibling: &Image, points: &[(f32, f32)]) -> Vec<Correspondence> {
    let mut out = Vec::with_capacity(points.len());
    for (tx, ty) in points.iter().copied() {
        let ix = tx.round() as isize;
        let iy = ty.round() as isize;
        let mut best = (f32::MIN, 0isize, 0isize);
        for dy in -SEARCH_RADIUS..=SEARCH_RADIUS {
            for dx in -SEARCH_RADIUS..=SEARCH_RADIUS {
                let sx = ix + dx;
                let sy = iy + dy;
                if sx < 0 || sy < 0 || sx >= sibling.w as isize || sy >= sibling.h as isize {
                    continue;
                }
                let score = pixels::ncc(target, ix, iy, sibling, sx, sy, PATCH_RADIUS);
                // Strictly greater, so a tie keeps the smaller displacement - which is the one
                // nearer the identity transform and is deterministic under the fixed scan order.
                if score > best.0 {
                    best = (score, sx, sy);
                }
            }
        }
        if best.0 >= MIN_PATCH_NCC {
            out.push(Correspondence {
                tx,
                ty,
                sx: best.1 as f32,
                sy: best.2 as f32,
                score: best.0,
            });
        }
    }
    out
}

/// The exhaustive least-median-of-squares fit, then a least-squares refit over the inliers.
fn fit(points: &[Correspondence]) -> Option<Alignment> {
    let n = points.len();
    if n < MIN_CORRESPONDENCES {
        return None;
    }

    let mut best: Option<(f32, Homography)> = None;
    for a in 0..n {
        for b in (a + 1)..n {
            for c in (b + 1)..n {
                for d in (c + 1)..n {
                    // `get` rather than indexing, so `clippy::indexing_slicing` stays denied in
                    // this crate. The four indices are provably inside `0..n`, and the whole point
                    // of the deny is that a reader should not have to prove that.
                    let (Some(pa), Some(pb), Some(pc), Some(pd)) =
                        (points.get(a), points.get(b), points.get(c), points.get(d))
                    else {
                        continue;
                    };
                    let subset = [*pa, *pb, *pc, *pd];
                    let Some(matrix) = homography_from_four(&subset) else {
                        continue;
                    };
                    let median = median_residual(&matrix, points);
                    match best {
                        // Strictly less, so the first subset in the fixed enumeration order wins a
                        // tie. Invariant 4.
                        Some((current, _)) if current <= median => {}
                        _ => best = Some((median, matrix)),
                    }
                }
            }
        }
    }

    let (median, coarse) = best?;
    // The inlier band is a multiple of the median rather than a fixed pixel count, which is what
    // makes the same code correct on a 2048 px proxy and on a 45 MP frame.
    let band = (median * 2.5).max(1.0);
    let inliers: Vec<Correspondence> = points
        .iter()
        .copied()
        .filter(|point| residual(&coarse, point) <= band)
        .collect();
    if inliers.len() < 4 {
        return None;
    }
    let matrix = homography_least_squares(&inliers).unwrap_or(coarse);
    let residual_px = median_residual(&matrix, &inliers);
    let patch_ncc = inliers.iter().map(|point| point.score).sum::<f32>() / inliers.len() as f32;

    Some(Alignment {
        matrix,
        inliers: inliers.len(),
        offered: n,
        residual_px,
        patch_ncc,
    })
}

/// The reprojection error of one correspondence, in pixels.
fn residual(matrix: &Homography, point: &Correspondence) -> f32 {
    let (x, y) = apply(matrix, point.tx, point.ty);
    (x - point.sx).hypot(y - point.sy)
}

/// The median reprojection error over a set.
fn median_residual(matrix: &Homography, points: &[Correspondence]) -> f32 {
    let mut errors: Vec<f32> = points.iter().map(|p| residual(matrix, p)).collect();
    errors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    match errors.len() {
        0 => f32::MAX,
        n => errors.get(n / 2).copied().unwrap_or(f32::MAX),
    }
}

/// Map one position through a homography.
#[must_use]
pub fn apply(matrix: &Homography, x: f32, y: f32) -> (f32, f32) {
    let w = matrix[6] * x + matrix[7] * y + matrix[8];
    if w.abs() < 1e-9 {
        return (x, y);
    }
    (
        (matrix[0] * x + matrix[1] * y + matrix[2]) / w,
        (matrix[3] * x + matrix[4] * y + matrix[5]) / w,
    )
}

/// The exact homography through four correspondences, by direct linear transform.
fn homography_from_four(points: &[Correspondence; 4]) -> Option<Homography> {
    let mut a = [[0.0f32; 8]; 8];
    let mut b = [0.0f32; 8];
    for (index, point) in points.iter().enumerate() {
        let row = index * 2;
        let (x, y, u, v) = (point.tx, point.ty, point.sx, point.sy);
        if let Some(slot) = a.get_mut(row) {
            *slot = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y];
        }
        if let Some(slot) = b.get_mut(row) {
            *slot = u;
        }
        if let Some(slot) = a.get_mut(row + 1) {
            *slot = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y];
        }
        if let Some(slot) = b.get_mut(row + 1) {
            *slot = v;
        }
    }
    let solved = solve8(&mut a, &mut b)?;
    Some([
        solved[0], solved[1], solved[2], solved[3], solved[4], solved[5], solved[6], solved[7], 1.0,
    ])
}

/// The least-squares homography over any number of correspondences, by normal equations.
///
/// Normal equations rather than a QR factorisation because the design matrix is eight columns and
/// well conditioned once the coordinates are what they are here - a few dozen pixels around one
/// small rectangle. It is also what makes this a plain 8x8 solve, which is the same routine the
/// four-point case uses, so there is one elimination in this file rather than two.
fn homography_least_squares(points: &[Correspondence]) -> Option<Homography> {
    let mut ata = [[0.0f32; 8]; 8];
    let mut atb = [0.0f32; 8];
    for point in points {
        let (x, y, u, v) = (point.tx, point.ty, point.sx, point.sy);
        let rows = [
            ([x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y], u),
            ([0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y], v),
        ];
        for (row, rhs) in rows {
            for i in 0..8 {
                let ri = row.get(i).copied().unwrap_or(0.0);
                if let Some(slot) = atb.get_mut(i) {
                    *slot += ri * rhs;
                }
                for j in 0..8 {
                    let rj = row.get(j).copied().unwrap_or(0.0);
                    if let Some(target) = ata.get_mut(i).and_then(|r| r.get_mut(j)) {
                        *target += ri * rj;
                    }
                }
            }
        }
    }
    let solved = solve8(&mut ata, &mut atb)?;
    Some([
        solved[0], solved[1], solved[2], solved[3], solved[4], solved[5], solved[6], solved[7], 1.0,
    ])
}

/// Gaussian elimination with partial pivoting on an 8x8 system.
fn solve8(a: &mut [[f32; 8]; 8], b: &mut [f32; 8]) -> Option<[f32; 8]> {
    for column in 0..8 {
        let mut pivot = column;
        let mut best = 0.0f32;
        for row in column..8 {
            let value = a.get(row).and_then(|r| r.get(column)).copied()?.abs();
            if value > best {
                best = value;
                pivot = row;
            }
        }
        if best < 1e-9 {
            return None;
        }
        a.swap(column, pivot);
        b.swap(column, pivot);
        let head = a.get(column).copied()?;
        let head_value = head.get(column).copied()?;
        for row in (column + 1)..8 {
            let factor = a.get(row).and_then(|r| r.get(column)).copied()? / head_value;
            if factor == 0.0 {
                continue;
            }
            for k in column..8 {
                let subtract = factor * head.get(k).copied()?;
                if let Some(slot) = a.get_mut(row).and_then(|r| r.get_mut(k)) {
                    *slot -= subtract;
                }
            }
            let head_rhs = b.get(column).copied()?;
            if let Some(slot) = b.get_mut(row) {
                *slot -= factor * head_rhs;
            }
        }
    }
    let mut out = [0.0f32; 8];
    for row in (0..8).rev() {
        let mut sum = b.get(row).copied()?;
        for column in (row + 1)..8 {
            sum -= a.get(row).and_then(|r| r.get(column)).copied()? * out.get(column).copied()?;
        }
        let diagonal = a.get(row).and_then(|r| r.get(row)).copied()?;
        if diagonal.abs() < 1e-9 {
            return None;
        }
        if let Some(slot) = out.get_mut(row) {
            *slot = sum / diagonal;
        }
    }
    Some(out)
}

/// True when every corner of the region maps inside the sibling's own frame.
fn lands_inside(matrix: &Homography, rect: &Rect, sibling: &Image) -> bool {
    let corners = [
        (rect.x as f32, rect.y as f32),
        (rect.right() as f32 - 1.0, rect.y as f32),
        (rect.x as f32, rect.bottom() as f32 - 1.0),
        (rect.right() as f32 - 1.0, rect.bottom() as f32 - 1.0),
    ];
    corners.into_iter().all(|(x, y)| {
        let (sx, sy) = apply(matrix, x, y);
        sx >= 0.0 && sy >= 0.0 && sx <= sibling.w as f32 - 1.0 && sy <= sibling.h as f32 - 1.0
    })
}

/// The sibling's pixels for one region, resampled onto the region's own grid.
fn warp_region(sibling: &Image, matrix: &Homography, rect: &Rect) -> Image {
    let mut out = Image::black(rect.w, rect.h);
    for y in 0..rect.h {
        for x in 0..rect.w {
            let (sx, sy) = apply(matrix, (rect.x + x) as f32, (rect.y + y) as f32);
            out.put(x, y, sibling.sample(sx, sy));
        }
    }
    out
}

/// The gain and offset that take the sibling's exposure onto this frame's, measured on the ring.
///
/// Measured on the **ring** rather than on the region, because the region is the one place the two
/// frames are supposed to differ. Fitting the photometry inside it would take the brightness of the
/// exit sign and apply it to the wall that replaces it, which is a rectangle of subtly wrong wall -
/// the failure this whole method exists to avoid, arrived at from the other direction.
fn photometry(target: &Image, sibling: &Image, matrix: &Homography, rect: &Rect) -> (f32, f32) {
    let ring = rect.grown(rect.w.max(rect.h) / 2 + 2, target.w, target.h);
    let mut n = 0.0f32;
    let mut sum_t = 0.0f32;
    let mut sum_s = 0.0f32;
    let mut sum_ss = 0.0f32;
    let mut sum_ts = 0.0f32;
    for y in ring.y..ring.bottom() {
        for x in ring.x..ring.right() {
            if rect.contains(x, y) {
                continue;
            }
            let (sx, sy) = apply(matrix, x as f32, y as f32);
            if sx < 0.0 || sy < 0.0 || sx > sibling.w as f32 - 1.0 || sy > sibling.h as f32 - 1.0 {
                continue;
            }
            let t = target.luma(x as isize, y as isize);
            let s = pixels::luminance(sibling.sample(sx, sy));
            n += 1.0;
            sum_t += t;
            sum_s += s;
            sum_ss += s * s;
            sum_ts += t * s;
        }
    }
    if n < 16.0 {
        return (1.0, 0.0);
    }
    let denominator = n * sum_ss - sum_s * sum_s;
    if denominator.abs() < 1e-9 {
        return (1.0, 0.0);
    }
    let gain = (n * sum_ts - sum_s * sum_t) / denominator;
    let offset = (sum_t - gain * sum_s) / n;
    // A gain far from one is not an exposure difference, it is a bad correspondence set. Clamping
    // rather than refusing, because the seam feather absorbs a small error and the alignment
    // confidence has already been computed from the geometry.
    (gain.clamp(0.5, 2.0), offset.clamp(-0.25, 0.25))
}

/// Write the borrowed pixels into a copy of the target, feathered on the band outside the object.
fn composite(
    target: &Image,
    warped: &Image,
    rect: &Rect,
    outer: &Rect,
    band: usize,
    gain: f32,
    offset: f32,
) -> Image {
    let mut out = target.clone();
    for y in outer.y..outer.bottom() {
        for x in outer.x..outer.right() {
            let weight = pixels::feather_out(rect, band, x, y);
            if weight <= 0.0 {
                continue;
            }
            let source = warped.at((x - outer.x) as isize, (y - outer.y) as isize);
            let original = target.at(x as isize, y as isize);
            let mut blended = [0.0f32; 3];
            for (channel, slot) in blended.iter_mut().enumerate() {
                let corrected = (source
                    .get(channel)
                    .copied()
                    .unwrap_or(0.0)
                    .mul_add(gain, offset))
                .max(0.0);
                *slot = original.get(channel).copied().unwrap_or(0.0) * (1.0 - weight)
                    + corrected * weight;
            }
            out.put(x, y, blended);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::PhotoId;

    fn textured(w: usize, h: usize, shift: f32) -> Image {
        // A deterministic non-repeating texture: two incommensurate frequencies plus a ramp, so a
        // block match has something to lock onto and a wrong lock is not rewarded.
        let mut image = Image::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let fx = x as f32 + shift;
                let fy = y as f32;
                let v = 0.35
                    + 0.12 * (fx * 0.21).sin()
                    + 0.09 * (fy * 0.13).cos()
                    + 0.05 * ((fx + fy) * 0.07).sin()
                    + 0.0004 * fx;
                image.put(x, y, [v, v * 0.92, v * 0.83]);
            }
        }
        image
    }

    fn paint(image: &mut Image, rect: Rect, value: [f32; 3]) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                image.put(x, y, value);
            }
        }
    }

    fn region(rect: Rect, w: usize, h: usize) -> Box2 {
        Box2 {
            x: rect.x as f32 / w as f32,
            y: rect.y as f32 / h as f32,
            w: rect.w as f32 / w as f32,
            h: rect.h as f32 / h as f32,
        }
    }

    fn some_id() -> ImageId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000024").unwrap_or_default()
    }

    const BLOCK: Rect = Rect {
        x: 60,
        y: 60,
        w: 20,
        h: 20,
    };

    #[test]
    fn a_sibling_showing_clean_background_replaces_the_object_with_real_pixels() {
        let clean = textured(160, 160, 0.0);
        let mut target = clean.clone();
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);

        let borrowed = borrow(
            &target,
            &clean,
            some_id(),
            &region(BLOCK, target.w, target.h),
        )
        .expect("a clean sibling must be borrowable");

        // The centre of the block is background again rather than red.
        let centre = borrowed.result.at(70, 70);
        let wanted = clean.at(70, 70);
        assert!(
            (centre[0] - wanted[0]).abs() < 0.05,
            "centre was {centre:?}, wanted about {wanted:?}"
        );
        assert!(borrowed.alignment.confidence() > 0.5);
    }

    #[test]
    fn a_sibling_with_the_same_object_in_it_is_refused() {
        // The refusal a naive implementation forgets: a burst neighbour usually has the
        // distraction in almost the same place, and borrowing from it replaces the exit sign with
        // the exit sign.
        let clean = textured(160, 160, 0.0);
        let mut target = clean.clone();
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let mut sibling = clean;
        paint(&mut sibling, BLOCK, [0.95, 0.05, 0.05]);

        let outcome = borrow(
            &target,
            &sibling,
            some_id(),
            &region(BLOCK, target.w, target.h),
        );
        assert_eq!(outcome.err(), Some(CleanupCode::NoAlignedSibling));
    }

    #[test]
    fn a_flat_object_present_in_both_frames_is_still_refused() {
        // The regression for the defect `SAME_OBJECT_DIFFERENCE` documents: a correlation over a
        // flat block is zero, so the first implementation read "identical object" as "completely
        // different" and borrowed. Most wedding distractions are close to flat.
        let clean = textured(160, 160, 0.0);
        let mut target = clean.clone();
        paint(&mut target, BLOCK, [0.30, 0.30, 0.30]);
        let mut sibling = clean;
        paint(&mut sibling, BLOCK, [0.30, 0.30, 0.30]);
        assert_eq!(
            borrow(&target, &sibling, some_id(), &region(BLOCK, 160, 160)).err(),
            Some(CleanupCode::NoAlignedSibling)
        );
    }

    #[test]
    fn a_featureless_pair_is_refused_rather_than_borrowed_confidently() {
        let flat = Image {
            w: 160,
            h: 160,
            rgb: vec![0.4; 160 * 160 * 3],
        };
        let mut target = flat.clone();
        paint(&mut target, BLOCK, [0.9, 0.1, 0.1]);
        let outcome = borrow(&target, &flat, some_id(), &region(BLOCK, 160, 160));
        assert_eq!(outcome.err(), Some(CleanupCode::NoAlignedSibling));
    }

    #[test]
    fn a_translated_sibling_is_aligned_and_borrowed() {
        // The sibling is the same wall photographed after the camera drifted, which is what a
        // burst actually looks like.
        let clean = textured(200, 200, 0.0);
        let shifted = textured(200, 200, 7.0);
        let mut target = clean;
        paint(&mut target, BLOCK, [0.9, 0.1, 0.1]);

        let borrowed = borrow(&target, &shifted, some_id(), &region(BLOCK, 200, 200))
            .expect("a drifted sibling must still align");
        assert!(borrowed.alignment.residual_px <= MAX_RESIDUAL_PX);
        assert!(borrowed.alignment.inliers >= 4);
    }

    #[test]
    fn the_fit_is_deterministic() {
        let clean = textured(160, 160, 0.0);
        let mut target = clean.clone();
        paint(&mut target, BLOCK, [0.9, 0.1, 0.1]);
        let first = borrow(&target, &clean, some_id(), &region(BLOCK, 160, 160)).expect("borrows");
        let second = borrow(&target, &clean, some_id(), &region(BLOCK, 160, 160)).expect("borrows");
        assert_eq!(first.alignment, second.alignment);
        assert_eq!(first.result, second.result);
    }

    #[test]
    fn an_identity_homography_maps_a_point_to_itself() {
        let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let (x, y) = apply(&identity, 12.5, 40.25);
        assert!((x - 12.5).abs() < 1e-5 && (y - 40.25).abs() < 1e-5);
    }

    #[test]
    fn four_correspondences_recover_the_translation_that_made_them() {
        let points = [
            Correspondence {
                tx: 10.0,
                ty: 10.0,
                sx: 17.0,
                sy: 13.0,
                score: 1.0,
            },
            Correspondence {
                tx: 90.0,
                ty: 10.0,
                sx: 97.0,
                sy: 13.0,
                score: 1.0,
            },
            Correspondence {
                tx: 10.0,
                ty: 90.0,
                sx: 17.0,
                sy: 93.0,
                score: 1.0,
            },
            Correspondence {
                tx: 90.0,
                ty: 90.0,
                sx: 97.0,
                sy: 93.0,
                score: 1.0,
            },
        ];
        let matrix = homography_from_four(&points).expect("four points define a homography");
        let (x, y) = apply(&matrix, 50.0, 50.0);
        assert!((x - 57.0).abs() < 1e-2, "x was {x}");
        assert!((y - 53.0).abs() < 1e-2, "y was {y}");
    }

    #[test]
    fn the_median_fit_ignores_a_walker_the_least_squares_fit_would_follow() {
        // Phase 23's lesson in this phase's shape: eight correspondences agree on a translation
        // of seven pixels and two of them - somebody walking through the ring - are forty pixels
        // out. A trimmed-mean fit would be dragged; the median is not.
        let mut points: Vec<Correspondence> = (0..8)
            .map(|index| {
                let angle = std::f32::consts::TAU * index as f32 / 8.0;
                let tx = 100.0 + 40.0 * angle.cos();
                let ty = 100.0 + 40.0 * angle.sin();
                Correspondence {
                    tx,
                    ty,
                    sx: tx + 7.0,
                    sy: ty,
                    score: 0.95,
                }
            })
            .collect();
        if let Some(bad) = points.get_mut(0) {
            bad.sx += 40.0;
        }
        if let Some(bad) = points.get_mut(1) {
            bad.sy -= 35.0;
        }
        let alignment = fit(&points).expect("six good correspondences are enough");
        let (x, y) = apply(&alignment.matrix, 100.0, 100.0);
        assert!((x - 107.0).abs() < 1.0, "x was {x}");
        assert!((y - 100.0).abs() < 1.0, "y was {y}");
        assert!(alignment.inliers >= 6 && alignment.inliers <= 8);
    }

    #[test]
    fn confidence_is_geometric_so_one_bad_term_is_fatal() {
        let good = Alignment {
            matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            inliers: 12,
            offered: 12,
            residual_px: 0.1,
            patch_ncc: 0.99,
        };
        assert!(good.confidence() > 0.9);
        let sloppy = Alignment {
            residual_px: MAX_RESIDUAL_PX,
            ..good.clone()
        };
        assert!(
            sloppy.confidence() < 1e-6,
            "a residual at the ceiling must not be rescued by perfect support"
        );
    }
}
