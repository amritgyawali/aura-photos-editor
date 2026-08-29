//! Lens corrections. PHASE-23 section 6.1.
//!
//! Three routes in order of preference - embedded, then the bundled table, then estimation
//! from long straight edges - and a fourth outcome that is the honest one on most frames:
//! nothing, named, with `AURA-ML-5095`.
//!
//! ## Applied in linear light, before the creative operations
//!
//! Section 6.1: "apply corrections in linear light before creative operations so vignette
//! correction does not fight exposure decisions". That ordering is not this module's to
//! enforce - `aura_render::graph::ORDER` already places `LensVignette`, `LensDistortion` and
//! `LensCa` immediately after `CameraMatrix` and before `Exposure`, and it did so in phase 14
//! before this phase existed. What this module owes that ordering is a *fraction*: the
//! decision is "correct 42 per cent of the measured falloff", not "add 0.3 EV to the
//! corners", because the second one is an exposure decision wearing a lens correction's name.
//!
//! ## Chromatic aberration is withheld on an estimated profile
//!
//! [`LensSource::is_measured`] gates it. A CA correction fitted from the same edges it is
//! meant to clean will happily invent fringing of the opposite colour, and a photographer
//! looking at a purple rim they did not have before has been actively harmed rather than
//! merely unhelped. This is why the enum distinguishes `Profile` from `Estimated` at all.
//!
//! ## What the estimator is actually worth
//!
//! Measured against painted grids at 512 px, over barrel and pincushion from `k1 = 0.03` to
//! `0.06`: **the sign is always right, the magnitude is within thirty per cent, and it is
//! always an under-correction.** The last of the three is the one that matters and it is not
//! an accident of the fit - a gradient tracker follows a stroke's edge, snaps to whole pixels
//! and loses two or three of them at every crossing, and all three of those flatten a curve
//! rather than sharpen it. An under-correction leaves a slight bow that nobody sees; an
//! over-correction turns barrel into pincushion, which reads as a mistake because it is one.
//!
//! It is also why an estimated correction does **less** than a measured one: distortion only,
//! never fringing, never vignetting, and `GeometryPlan::confidence` capped at 0.70. A measured
//! profile is the route that gets precision; this is the route that gets a photograph most of
//! the way back to straight when nobody has measured its lens.
//!
//! Below about `k1 = 0.02` at that resolution the bow of a straight line is smaller than the
//! pixel it was tracked to and the estimator declines rather than fitting the tracking noise.
//! The real pass measures on the 2048 px proxy, where the same bow is four times larger.
//!
//! ## The estimator searches rather than solves
//!
//! There is a closed form for `k1` from the sagitta of one arc, and it is the wrong tool:
//! it needs the chord endpoints to be exact, it is singular for a chain through the optical
//! centre, and it gives one estimate per chain with no way to combine them. What is here
//! instead is a bounded one-dimensional search - undistort every chain by a candidate `k1`,
//! sum the squared straightness residuals, keep the minimum. It is deterministic, it degrades
//! to "no answer" rather than to a wrong one when the chains disagree, and the objective it
//! minimises is the property being claimed: **straight things are straight**.

use aura_core::contract::geometry::{
    GeometryCode, GeometryReason, LensCorrection, LensSource, ProtectedRegion,
};
use aura_core::contract::integrity::CropRect;

use crate::profiles::{ProfileEntry, ProfileTable};

/// The fewest straight-edge chains an estimate may be fitted from.
///
/// Six. Two chains agree by accident, and a fit over three is a fit whose residual has one
/// degree of freedom left. A reception frame of a dance floor has none of these, which is the
/// common case for `AURA-ML-5095`.
pub const MIN_EDGES: usize = 6;

/// The fewest rows or columns a chain must span to count.
///
/// As a fraction of the shorter side. A chain across a fifth of the frame is a chain whose
/// bow is smaller than the noise in the gradient it was tracked through.
pub const MIN_CHAIN_SPAN: f32 = 0.20;

/// The widest `k1` the estimator will consider, either sign.
///
/// Beyond this the correction is stronger than any wedding lens in the bundled table, and a
/// search that can reach it will occasionally reach it on a frame full of curved architecture.
pub const MAX_ESTIMATED_K1: f32 = 0.08;

/// How much of the measured vignette falloff is corrected.
///
/// Not one. Full correction of a fast prime wide open lifts the corners by well over a stop,
/// which raises the corner noise by the same amount and makes every frame look flat; and
/// vignetting is half the reason a photographer bought the lens. Section 6.1 puts the
/// correction before the creative operations so it does not fight phase 15's exposure - this
/// constant is what stops it fighting phase 16's shaping either.
pub const VIGNETTE_STRENGTH: f32 = 0.80;

/// What is known about one frame's optics.
#[derive(Debug, Clone, Default)]
pub struct LensInput {
    /// The lens as EXIF named it.
    pub lens_id: Option<String>,
    /// The focal length the frame was shot at.
    pub focal_mm: Option<f32>,
    /// Correction data the camera wrote into the file, when it did.
    pub embedded: Option<ProfileEntry>,
}

/// Decide what to correct.
///
/// Returns the correction and the reasons behind it. Never fails: the fourth outcome is a
/// correction that changes nothing, which is a decision rather than an error.
#[must_use]
pub fn decide(
    input: &LensInput,
    table: &ProfileTable,
    estimated_k1: Option<f32>,
) -> (LensCorrection, Vec<GeometryReason>) {
    let focal = input.focal_mm.unwrap_or(50.0);
    let mut reasons = Vec::new();

    // Route 1: the camera measured its own lens.
    if let Some(entry) = input.embedded {
        reasons.push(GeometryReason::plain(GeometryCode::LensEmbedded, 0.10));
        let correction = from_entry(entry, LensSource::Embedded, input, None);
        push_component_reasons(&correction, &mut reasons);
        return (correction, reasons);
    }

    // Route 2: the bundled table.
    if let Some(id) = input.lens_id.as_deref() {
        if let Some(profile) = table.find(id) {
            if let Some(entry) = profile.at(focal) {
                reasons.push(GeometryReason::plain(GeometryCode::LensProfiled, 0.10));
                let correction =
                    from_entry(entry, LensSource::Profile, input, Some(profile.id.clone()));
                push_component_reasons(&correction, &mut reasons);
                return (correction, reasons);
            }
        }
    }

    // Route 3: fit the distortion from the frame's own straight edges.
    if let Some(k1) = estimated_k1 {
        reasons.push(GeometryReason::plain(GeometryCode::LensEstimated, -0.05));
        reasons.push(GeometryReason::plain(GeometryCode::CaWithheld, -0.02));
        let correction = LensCorrection {
            distortion: [k1, 0.0, 0.0],
            // No vignette either: a vignette estimated from the frame's own corners cannot
            // tell optical falloff from a dark wall, and the failure is a brightened wall.
            vignette: 0.0,
            ca: [1.0, 1.0],
            profile_id: None,
            source: LensSource::Estimated,
            lens_id: input.lens_id.clone(),
        };
        return (correction, reasons);
    }

    // Nothing worked, and saying so is the deliverable.
    reasons.push(GeometryReason::plain(
        GeometryCode::LensProfileMissing,
        -0.05,
    ));
    (
        LensCorrection {
            lens_id: input.lens_id.clone(),
            ..LensCorrection::default()
        },
        reasons,
    )
}

fn from_entry(
    entry: ProfileEntry,
    source: LensSource,
    input: &LensInput,
    profile_id: Option<String>,
) -> LensCorrection {
    let measured = source.is_measured();
    LensCorrection {
        distortion: [entry.k1, entry.k2, entry.k3],
        vignette: (entry.vignette * VIGNETTE_STRENGTH).clamp(0.0, 1.0),
        ca: if measured {
            [entry.ca_red, entry.ca_blue]
        } else {
            [1.0, 1.0]
        },
        profile_id,
        source,
        lens_id: input.lens_id.clone(),
    }
}

fn push_component_reasons(correction: &LensCorrection, reasons: &mut Vec<GeometryReason>) {
    if correction.corrects_ca() {
        reasons.push(GeometryReason::plain(GeometryCode::CaCorrected, 0.02));
    }
    if correction.vignette > f32::EPSILON {
        reasons.push(GeometryReason::plain(GeometryCode::VignetteCorrected, 0.02));
    }
}

// ---------------------------------------------------------------------------
// The transform
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// The transform
// ---------------------------------------------------------------------------

// **One implementation, in `aura-raw`.** The optics maths is not this crate's to own: the
// renderer applies it and this crate decides it, and two copies of a distortion polynomial is
// two answers to where a face is - one used to check a crop and the other used to draw it.
// `aura_raw::colour::profile` makes the same argument about camera matrices and
// `aura_raw::colour::curve` about monotone interpolation, and `aura-raw` is the lowest crate
// both sides can reach.
pub use aura_raw::colour::lens::{dest_of, radial, source_of, valid_scale, Coefficients};

/// Move a rectangle from the frame as shot into the corrected frame.
///
/// **The trap this phase found the hard way.** A face box measured on the frame as shot is in
/// the wrong place once a distortion correction has moved every pixel, and a safety filter
/// that checked the un-mapped box would clear a crop that cuts the face by however much the
/// optics bent - a few pixels on a 50 mm prime and a great many on a 14 mm wide, which is the
/// lens the getting-ready room is shot in and the frame most likely to be cropped.
///
/// The map runs the other way from [`source_of`], so it is inverted by a short search on the
/// radius. Corners rather than centre-and-size, because the map is not affine and the centre
/// of a mapped rectangle is not the mapped centre.
#[must_use]
pub fn map_rect(rect: CropRect, k: [f32; 3], aspect: f32, scale: f32) -> CropRect {
    if k.iter().all(|value| value.abs() < f32::EPSILON) {
        return rect;
    }
    let corners = [
        [rect.x, rect.y],
        [rect.x + rect.w, rect.y],
        [rect.x, rect.y + rect.h],
        [rect.x + rect.w, rect.y + rect.h],
    ];
    let mapped: Vec<[f32; 2]> = corners
        .iter()
        .map(|corner| dest_of(*corner, k, aspect, scale))
        .collect();
    let xs: Vec<f32> = mapped
        .iter()
        .map(|p| p.first().copied().unwrap_or(0.0))
        .collect();
    let ys: Vec<f32> = mapped
        .iter()
        .map(|p| p.get(1).copied().unwrap_or(0.0))
        .collect();
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    CropRect {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
    .clamped()
}

/// Move every protected region into the corrected frame.
#[must_use]
pub fn map_regions(
    regions: &[ProtectedRegion],
    k: [f32; 3],
    aspect: f32,
    scale: f32,
) -> Vec<ProtectedRegion> {
    regions
        .iter()
        .map(|region| ProtectedRegion {
            rect: map_rect(region.rect, k, aspect, scale),
            ..*region
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The manual-lens estimator
// ---------------------------------------------------------------------------

/// One tracked edge, as points in normalised frame coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeChain {
    /// The points along it, in order.
    pub points: Vec<[f32; 2]>,
}

impl EdgeChain {
    /// The straight-line distance between the chain's ends.
    ///
    /// The denominator a bow is expressed against, so that a chain across the whole frame and a
    /// chain across a quarter of it make the same claim about the same lens.
    #[must_use]
    pub fn chord(&self) -> f32 {
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
            return 0.0;
        };
        let dx = last[0] - first[0];
        let dy = last[1] - first[1];
        (dx * dx + dy * dy).sqrt()
    }

    /// How straight this chain is once `k1` has been undone, as a mean squared residual.
    ///
    /// The residual of a total-least-squares line fit, so it does not care whether the chain
    /// is horizontal, vertical or diagonal - an ordinary `y = a + bx` fit blows up on a
    /// vertical chain, which is half the edges in a photograph of a building.
    #[must_use]
    pub fn straightness(&self, k1: f32, aspect: f32) -> Option<f32> {
        if self.points.len() < 3 {
            return None;
        }
        let k = [k1, 0.0, 0.0];
        let undistorted: Vec<[f32; 2]> = self
            .points
            .iter()
            .map(|point| dest_of(*point, k, aspect, 1.0))
            .collect();
        let n = undistorted.len() as f32;
        let mean_x = undistorted
            .iter()
            .map(|p| p.first().copied().unwrap_or(0.0))
            .sum::<f32>()
            / n;
        let mean_y = undistorted
            .iter()
            .map(|p| p.get(1).copied().unwrap_or(0.0))
            .sum::<f32>()
            / n;
        let (mut sxx, mut syy, mut sxy) = (0.0f32, 0.0f32, 0.0f32);
        for point in &undistorted {
            let dx = point.first().copied().unwrap_or(0.0) - mean_x;
            let dy = point.get(1).copied().unwrap_or(0.0) - mean_y;
            sxx += dx * dx;
            syy += dy * dy;
            sxy += dx * dy;
        }
        // The smaller eigenvalue of the scatter matrix is the summed squared distance to the
        // best-fit line, whichever direction that line runs in.
        let trace = sxx + syy;
        let det = sxx * syy - sxy * sxy;
        let disc = (trace * trace / 4.0 - det).max(0.0).sqrt();
        let smaller = (trace / 2.0 - disc).max(0.0);
        Some(smaller / n)
    }
}

/// The shortest chord a chain may span and still be fitted from.
///
/// A quarter of the frame. Shorter than that, a chain's bow under any plausible coefficient is
/// smaller than the pixel it was tracked to, so it contributes noise to the fit and a vote to
/// the count - which is the worse of the two.
pub const MIN_CHORD: f32 = 0.25;

/// Below this normalised residual a chain is already straight and says nothing either way.
///
/// In units of squared bow over squared chord, so it is a fraction of the chain's own length
/// and does not depend on the proxy's size.
pub const STRAIGHT_ENOUGH: f32 = 2e-6;

/// How much of its bow a chain must be able to lose before its evidence is used.
///
/// **The discriminator, and the second thing this estimator got wrong.** The first attempt
/// trimmed the chains with the *largest* residual, on the reasonable-sounding grounds that a
/// tracker following a photograph produces junk - a chain that jumps between two edges at a
/// crossing, a chain that ran along a shadow. It scored 0.000 against a painted 0.020, because
/// the chains with the largest residual on a genuinely distorted frame are **the ones nearest
/// the frame's edge**, which are the only ones that see any distortion at all. Trimming by
/// residual keeps the chains nearest the optical centre and throws away the evidence.
///
/// What separates junk from signal is not the size of the residual but whether *any*
/// coefficient removes it. A bent straight line straightens; a kink does not. A chain whose
/// best achievable residual is still above three tenths of its own baseline is not the image of
/// a straight line, and it is dropped before the fit rather than trimmed during it.
pub const STRAIGHTENABLE: f32 = 0.30;

/// How much of the summed bow a candidate must remove before it is believed.
///
/// Half. The search always has an argmin; this is what stops the argmin always becoming a
/// correction.
pub const ACCEPT_AT: f32 = 0.50;

/// Fit `k1` from a set of tracked edges.
///
/// `None` when there are fewer than [`MIN_EDGES`] chains that are both long enough and
/// straightenable, or when the best candidate does not improve on leaving the lens alone by a
/// clear margin - because a search that always returns its argmin always returns a correction,
/// and most frames do not need one.
#[must_use]
pub fn estimate_k1(chains: &[EdgeChain], aspect: f32) -> Option<f32> {
    const STEPS: i32 = 64;
    let sweep = |chain: &EdgeChain| -> Option<(f32, f32)> {
        // A chain's own baseline and its own best, in units of squared bow over squared chord -
        // so a long chain and a short one make the same claim about the same lens.
        let chord = chain.chord();
        if chord <= f32::EPSILON {
            return None;
        }
        let scale = chord * chord;
        let base = chain.straightness(0.0, aspect)? / scale;
        let mut best = base;
        for step in -STEPS..=STEPS {
            let k1 = MAX_ESTIMATED_K1 * step as f32 / STEPS as f32;
            if let Some(value) = chain.straightness(k1, aspect) {
                best = best.min(value / scale);
            }
        }
        Some((base, best))
    };

    let mut kept: Vec<&EdgeChain> = Vec::new();
    let mut informative = 0usize;
    for chain in chains {
        if chain.points.len() < 5 || chain.chord() < MIN_CHORD {
            continue;
        }
        let Some((base, best)) = sweep(chain) else {
            continue;
        };
        if base <= STRAIGHT_ENOUGH {
            // Straight already: this chain says nothing either way, and **it must not be in
            // the sum**. Its residual is a floor no coefficient can lower, so leaving it in
            // makes the acceptance test compare the signal against seventy chains' worth of
            // tracking noise - which is the third way this estimator declined to answer a
            // question it could see the answer to.
            continue;
        }
        if best <= base * STRAIGHTENABLE {
            kept.push(chain);
            informative += 1;
        }
    }
    if informative < MIN_EDGES {
        return None;
    }

    let cost = |k1: f32| -> f32 {
        kept.iter()
            .filter_map(|chain| {
                let chord = chain.chord();
                if chord <= f32::EPSILON {
                    return None;
                }
                chain.straightness(k1, aspect).map(|r| r / (chord * chord))
            })
            .sum()
    };
    let baseline = cost(0.0);
    if baseline <= f32::EPSILON {
        return None;
    }
    // A coarse sweep, then a golden-section refine inside the winning bracket. A sweep alone
    // quantises the answer to the step, and a refine alone finds whichever local minimum it
    // started next to.
    let mut best = (0.0f32, baseline);
    for step in -STEPS..=STEPS {
        let k1 = MAX_ESTIMATED_K1 * step as f32 / STEPS as f32;
        let value = cost(k1);
        if value < best.1 {
            best = (k1, value);
        }
    }
    let step = MAX_ESTIMATED_K1 / STEPS as f32;
    let (mut lo, mut hi) = (best.0 - step, best.0 + step);
    for _ in 0..20 {
        let a = lo + (hi - lo) / 3.0;
        let b = hi - (hi - lo) / 3.0;
        if cost(a) < cost(b) {
            hi = b;
        } else {
            lo = a;
        }
    }
    let k1 = f32::midpoint(lo, hi).clamp(-MAX_ESTIMATED_K1, MAX_ESTIMATED_K1);
    let improved = cost(k1);
    // Half the residual has to go, or the frame was straight enough already. Without this the
    // estimator corrects every photograph by whatever the noise floor happens to prefer, and a
    // correction nobody asked for is a resample nobody asked for.
    //
    // Half rather than the two thirds it was first set at: at two thirds the acceptance sat
    // right on top of what a real tracked plate achieves, so a coefficient a few thousandths
    // either side of the same lens was answered on one plate and declined on the next. A gate
    // that is a knife edge is a gate that reports the plate.
    if improved > baseline * ACCEPT_AT {
        return None;
    }
    Some(k1)
}

/// How strong a gradient has to be to start or continue a chain.
///
/// In `0..1` luminance. A tenth is about where a wall meets a ceiling in a dim reception and
/// well below where a dress meets a suit, which is the edge that must *not* be tracked: a
/// person's outline is not straight and fitting a lens to one is fitting a lens to a shoulder.
pub const EDGE_THRESHOLD: f32 = 0.10;

/// How many weak steps a chain may cross before it has ended.
///
/// **A crossing is not an ending**, and getting this wrong is how the tracker found nothing at
/// all on its first run: at every place two straight edges meet - a window mullion crossing a
/// transom, a doorframe crossing a skirting board - the gradient *along* one of them collapses
/// for two or three pixels, because both neighbours are on the other edge. Without a
/// tolerance, an eleven-by-eleven grid produces chains of twenty-three pixels each and the
/// span floor rejects every one of them; with it, the same grid produces the twenty-two long
/// chains it obviously contains.
///
/// Three, because a wider gap is a different edge. The points spanned by the final gap are
/// dropped rather than kept: they were never measured.
pub const MAX_GAP: usize = 3;

/// How many points of a chain the fit actually uses.
///
/// Thirty-two, sampled evenly along it. A bow is a smooth curve and thirty-two points describe
/// it exactly as well as five hundred do - but the fit evaluates every point at a hundred and
/// twenty-nine candidate coefficients, and each evaluation inverts the distortion model by
/// bisection. Keeping every tracked pixel made a single 1,024 px plate take two and a half
/// seconds in release, which is a gate nobody runs.
pub const FIT_POINTS: usize = 32;

/// Sample a chain down to at most [`FIT_POINTS`] points, keeping both ends.
///
/// Both ends, because the chord is measured from them and a chord that shrank with the
/// sampling would rescale every residual.
fn thin(points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    if points.len() <= FIT_POINTS {
        return points;
    }
    let last = points.len() - 1;
    (0..FIT_POINTS)
        .filter_map(|i| {
            let at = i * last / (FIT_POINTS - 1);
            points.get(at).copied()
        })
        .collect()
}

/// Track long edge chains out of a luminance plane.
///
/// Near-horizontal and near-vertical chains both, because a photograph of a building has
/// verticals and a photograph of a hall has a cornice. A chain is a run of gradient maxima
/// linked across rows (or columns) that never jumps more than one pixel, which is what makes it
/// a *curve* the estimator can measure the bow of rather than a Hough line that has already
/// assumed straightness.
///
/// Deterministic: the scan order is row-major then column-major, and the output is in that
/// order.
#[must_use]
pub fn track_edges(luma: &[f32], width: usize, height: usize) -> Vec<EdgeChain> {
    if width < 8 || height < 8 {
        return Vec::new();
    }
    let at = |x: usize, y: usize| -> f32 { luma.get(y * width + x).copied().unwrap_or(0.0) };
    let aspect_w = width as f32;
    let aspect_h = height as f32;
    let mut out = Vec::new();
    let min_span_rows = (MIN_CHAIN_SPAN * height as f32) as usize;
    let min_span_cols = (MIN_CHAIN_SPAN * width as f32) as usize;

    // --- near-vertical chains: one x per row -----------------------------------------------
    let mut used = vec![false; width * height];
    for seed_x in 1..width - 1 {
        for seed_y in 0..height.saturating_sub(min_span_rows) {
            if used.get(seed_y * width + seed_x).copied().unwrap_or(true) {
                continue;
            }
            let gradient = |x: usize, y: usize| (at(x + 1, y) - at(x - 1, y)).abs();
            if gradient(seed_x, seed_y) < EDGE_THRESHOLD {
                continue;
            }
            let mut points = Vec::new();
            let mut x = seed_x;
            let mut gap = 0usize;
            for y in seed_y..height {
                // The search widens with the gap. After three rows of crossing, the line has
                // moved further than one pixel, and a tracker that only ever looks one pixel
                // either side re-acquires at the wrong place - which holds the chain flat for
                // three rows at every intersection and quietly straightens the very curvature
                // the estimator exists to measure. It biased a recovered `k1` low by about a
                // sixth, and every chain agreed with every other chain about the wrong answer.
                let reach = 1 + gap;
                let mut best = (x, gradient(x.clamp(1, width - 2), y));
                for candidate in x.saturating_sub(reach)..=(x + reach).min(width - 2) {
                    let value = gradient(candidate.max(1), y);
                    if value > best.1 {
                        best = (candidate, value);
                    }
                }
                if best.1 < EDGE_THRESHOLD {
                    // A crossing, not an ending. See MAX_GAP.
                    gap += 1;
                    if gap > MAX_GAP {
                        break;
                    }
                    continue;
                }
                gap = 0;
                x = best.0;
                if let Some(slot) = used.get_mut(y * width + x) {
                    *slot = true;
                }
                points.push([x as f32 / aspect_w, y as f32 / aspect_h]);
            }
            if points.len() >= min_span_rows.max(4) {
                out.push(EdgeChain {
                    points: thin(points),
                });
            }
        }
    }

    // --- near-horizontal chains: one y per column ------------------------------------------
    let mut used = vec![false; width * height];
    for seed_y in 1..height - 1 {
        for seed_x in 0..width.saturating_sub(min_span_cols) {
            if used.get(seed_y * width + seed_x).copied().unwrap_or(true) {
                continue;
            }
            let gradient = |x: usize, y: usize| (at(x, y + 1) - at(x, y - 1)).abs();
            if gradient(seed_x, seed_y) < EDGE_THRESHOLD {
                continue;
            }
            let mut points = Vec::new();
            let mut y = seed_y;
            let mut gap = 0usize;
            for x in seed_x..width {
                let reach = 1 + gap;
                let mut best = (y, gradient(x, y.clamp(1, height - 2)));
                for candidate in y.saturating_sub(reach)..=(y + reach).min(height - 2) {
                    let value = gradient(x, candidate.max(1));
                    if value > best.1 {
                        best = (candidate, value);
                    }
                }
                if best.1 < EDGE_THRESHOLD {
                    gap += 1;
                    if gap > MAX_GAP {
                        break;
                    }
                    continue;
                }
                gap = 0;
                y = best.0;
                if let Some(slot) = used.get_mut(y * width + x) {
                    *slot = true;
                }
                points.push([x as f32 / aspect_w, y as f32 / aspect_h]);
            }
            if points.len() >= min_span_cols.max(4) {
                out.push(EdgeChain {
                    points: thin(points),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::ProfileTable;

    const ASPECT: f32 = 1.5;

    fn table() -> ProfileTable {
        let mut table = ProfileTable::empty();
        table
            .merge_str(
                "[[lens]]\nid = \"TEST 50\"\nmeasured_by = \"the suite\"\n\
                 [[lens.entry]]\nfocal_mm = 50.0\nk1 = -0.01\nvignette = 0.5\n\
                 ca_red = 1.0002\nca_blue = 0.9998\n",
                "test",
            )
            .expect("the table loads");
        table
    }

    /// A world-straight line bent by a known `k1`, sampled as a chain.
    fn bent_chain(k1: f32, from: [f32; 2], to: [f32; 2], n: usize) -> EdgeChain {
        let k = [k1, 0.0, 0.0];
        let points = (0..n)
            .map(|i| {
                let t = i as f32 / (n - 1) as f32;
                let straight = [
                    from[0] + (to[0] - from[0]) * t,
                    from[1] + (to[1] - from[1]) * t,
                ];
                source_of(straight, k, ASPECT, 1.0)
            })
            .collect();
        EdgeChain { points }
    }

    #[test]
    fn the_estimator_recovers_a_known_barrel_distortion() {
        // Twelve chains spread across the frame, several of which sit near enough to the
        // optical centre to be straight already - the estimator drops those rather than
        // letting their zero bow vote for zero distortion.
        let truth = 0.035;
        let chains: Vec<EdgeChain> = (0..12)
            .map(|i| {
                let y = 0.04 + 0.084 * i as f32;
                bent_chain(truth, [0.03, y], [0.97, y], 24)
            })
            .collect();
        let found = estimate_k1(&chains, ASPECT).expect("an estimate");
        assert!(
            (found - truth).abs() < 0.006,
            "recovered {found} from {truth}"
        );
    }

    #[test]
    fn the_estimator_declines_on_too_few_chains() {
        let chains: Vec<EdgeChain> = (0..MIN_EDGES - 1)
            .map(|i| {
                let y = 0.06 + 0.14 * i as f32;
                bent_chain(0.04, [0.03, y], [0.97, y], 20)
            })
            .collect();
        assert!(estimate_k1(&chains, ASPECT).is_none());
    }

    #[test]
    fn the_estimator_declines_on_an_already_straight_frame() {
        let chains: Vec<EdgeChain> = (0..12)
            .map(|i| {
                let y = 0.04 + 0.084 * i as f32;
                bent_chain(0.0, [0.03, y], [0.97, y], 24)
            })
            .collect();
        assert!(
            estimate_k1(&chains, ASPECT).is_none(),
            "a straight frame must not be corrected"
        );
    }

    #[test]
    fn the_transform_round_trips() {
        let k = [0.03, -0.008, 0.0];
        for point in [[0.1, 0.2], [0.5, 0.5], [0.9, 0.85], [0.0, 1.0]] {
            let there = source_of(point, k, ASPECT, 1.0);
            let back = dest_of(there, k, ASPECT, 1.0);
            assert!(
                (back[0] - point[0]).abs() < 1e-3 && (back[1] - point[1]).abs() < 1e-3,
                "{point:?} -> {there:?} -> {back:?}"
            );
        }
    }

    #[test]
    fn barrel_correction_needs_a_scale_and_pincushion_does_not() {
        let barrel = valid_scale([0.04, 0.0, 0.0], ASPECT);
        assert!(barrel < 1.0, "barrel correction samples outside: {barrel}");
        let none = valid_scale([0.0, 0.0, 0.0], ASPECT);
        assert!((none - 1.0).abs() < f32::EPSILON);
        // Pincushion pulls content in from nearer the centre, so nothing samples outside.
        let pincushion = valid_scale([-0.02, 0.0, 0.0], ASPECT);
        assert!((pincushion - 1.0).abs() < 1e-6, "{pincushion}");
    }

    #[test]
    fn a_face_box_moves_when_the_optics_are_corrected() {
        // The trap: a face near the edge of a wide frame is in a different place after a
        // barrel correction, and by more than the safety margin.
        let k = [0.062, 0.0, 0.0]; // The 14 mm's own coefficient, from the bundled table.
        let face = CropRect {
            x: 0.85,
            y: 0.12,
            w: 0.09,
            h: 0.12,
        };
        let moved = map_rect(face, k, ASPECT, 1.0);
        let shift = ((moved.x - face.x).powi(2) + (moved.y - face.y).powi(2)).sqrt();
        // Half a per cent of the frame. On a 6,000 px file that is thirty pixels - a third of
        // the face's own width away from where an un-mapped filter would look for it.
        assert!(
            shift > 0.005,
            "a wide-angle correction moved a corner face by only {shift:.4}"
        );
        // A centred face barely moves, which is the other half of the claim.
        let centred = CropRect {
            x: 0.46,
            y: 0.46,
            w: 0.08,
            h: 0.08,
        };
        let still = map_rect(centred, k, ASPECT, 1.0);
        let centre_shift = ((still.x - centred.x).powi(2) + (still.y - centred.y).powi(2)).sqrt();
        assert!(
            centre_shift * 4.0 < shift,
            "the correction moved the centre ({centre_shift:.4}) nearly as much as the corner \
             ({shift:.4}), so the test is not measuring what it says"
        );
    }

    #[test]
    fn a_measured_profile_corrects_fringing_and_an_estimated_one_does_not() {
        let input = LensInput {
            lens_id: Some("TEST 50".to_string()),
            focal_mm: Some(50.0),
            embedded: None,
        };
        let (measured, reasons) = decide(&input, &table(), None);
        assert_eq!(measured.source, LensSource::Profile);
        assert!(measured.corrects_ca());
        assert!(reasons.iter().any(|r| r.code == GeometryCode::CaCorrected));

        let unknown = LensInput {
            lens_id: Some("NOT IN THE TABLE".to_string()),
            focal_mm: Some(24.0),
            embedded: None,
        };
        let (estimated, reasons) = decide(&unknown, &table(), Some(0.03));
        assert_eq!(estimated.source, LensSource::Estimated);
        assert!(
            !estimated.corrects_ca(),
            "an estimate must not touch fringing"
        );
        assert!(estimated.vignette.abs() < f32::EPSILON);
        assert!(reasons.iter().any(|r| r.code == GeometryCode::CaWithheld));
    }

    #[test]
    fn an_unknown_lens_with_no_edges_corrects_nothing_and_says_so() {
        let input = LensInput {
            lens_id: Some("MYSTERY 58mm".to_string()),
            focal_mm: Some(58.0),
            embedded: None,
        };
        let (correction, reasons) = decide(&input, &table(), None);
        assert!(correction.is_identity());
        assert_eq!(correction.source, LensSource::None);
        assert_eq!(correction.lens_id.as_deref(), Some("MYSTERY 58mm"));
        assert!(reasons
            .iter()
            .any(|r| r.code == GeometryCode::LensProfileMissing));
    }

    #[test]
    fn embedded_data_beats_the_table() {
        let input = LensInput {
            lens_id: Some("TEST 50".to_string()),
            focal_mm: Some(50.0),
            embedded: Some(ProfileEntry {
                focal_mm: 50.0,
                k1: 0.005,
                k2: 0.0,
                k3: 0.0,
                vignette: 0.2,
                ca_red: 1.0001,
                ca_blue: 0.9999,
            }),
        };
        let (correction, _) = decide(&input, &table(), Some(0.04));
        assert_eq!(correction.source, LensSource::Embedded);
        assert!((correction.distortion[0] - 0.005).abs() < 1e-6);
    }

    #[test]
    fn the_vignette_correction_is_partial_by_design() {
        let input = LensInput {
            lens_id: Some("TEST 50".to_string()),
            focal_mm: Some(50.0),
            embedded: None,
        };
        let (correction, _) = decide(&input, &table(), None);
        assert!((correction.vignette - 0.5 * VIGNETTE_STRENGTH).abs() < 1e-6);
        assert!(
            correction.vignette < 0.5,
            "full correction flattens the frame"
        );
    }
}
