//! Which rectangles are worth considering, and how good each one is.
//!
//! Section 6.3: "Generate candidate crops by optimising a composition objective (subject
//! placement, balance, edge cleanliness, headroom) over translation/scale within a bounded
//! search."
//!
//! ## The objective is this phase's own, and it is not phase 11's composite
//!
//! Phase 11's `composition_score` is a judgement about a **photograph**. It fuses horizon,
//! headroom, thirds, balance, clutter, colour competition and a learned aesthetic reading, and
//! several of those terms do not change when the frame is cropped: the background is as cluttered
//! inside a tighter rectangle as it was outside it, and the aesthetic head never saw the
//! rectangle at all. Optimising over a rectangle with an objective that mostly does not depend on
//! the rectangle finds noise, and the noise would then be compared against
//! [`aura_core::contract::geometry::MIN_IMPROVEMENT`] as though it meant something.
//!
//! So [`objective`] is four terms that are **all** functions of the rectangle, each bounded in
//! `0..1`, combined as a **geometric** mean. Geometric for the reason phase 09's technical score,
//! phase 12's keep score and phase 18's mask allowance are: a rectangle that puts the subject
//! perfectly on a power point and cuts a bright doorway in half at the edge must not average out
//! as good. One term at zero is a rectangle at zero.
//!
//! It is comparable **between rectangles of one photograph** and deliberately not between
//! photographs: three of the four terms are normalised by the rectangle's own size and the
//! frame's own energy, so a 0.7 on a dance floor and a 0.7 on a detail shot are two different
//! statements. Nothing in this product ranks frames by it and no column stores it as a quality.
//!
//! ## What the search may not do
//!
//! It may not propose a rectangle that breaks a safety rule - [`crate::safety::check`] runs first
//! and an unsafe candidate is *dropped* rather than penalised - and it may not go tighter than
//! the scene's own zoom or the resolution floor, whichever binds first. It cannot grow: every
//! candidate is inside the frame it started in, because a crop can only ever remove.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    fit_aspect, AspectRatio, CropPurpose, CropVariant, GeometryCode,
};

use crate::profiles::{Placement, SceneRule};
use crate::safety::{self, Limits};

/// How many scales the search tries between the frame and the scene's tightest zoom.
///
/// Seven. The search is a grid rather than a descent because the objective is not convex - the
/// placement term has four maxima on a thirds row and the edge term is piecewise - and a
/// gradient method on it returns whichever local maximum it started next to. Seven scales by
/// nine offsets by two axes is a few hundred evaluations of four summed-area lookups, which is
/// microseconds, and it is *deterministic*: invariant 4 says the same inputs produce the same
/// recipe, and a search with a random restart in it does not.
pub const SCALE_STEPS: usize = 7;

/// How many translations the search tries along each axis at each scale.
///
/// Nine, which is a step of an eighth of the available travel. Finer than the eye can see on a
/// crop boundary and coarse enough that the whole search stays inside the 40 ms budget beside a
/// decode.
pub const OFFSET_STEPS: usize = 9;

/// The share of a rectangle's shorter side that counts as its edge band.
///
/// Six per cent. The band is where "edge cleanliness" is measured, and its width is the one
/// number in this module that had to be chosen rather than derived: too narrow and a bright
/// doorway two pixels further in is invisible, too wide and the term stops being about the edge
/// and becomes a second clutter measure - which phase 11 already has and which does not depend
/// on the rectangle.
pub const EDGE_BAND: f32 = 0.06;

/// The frame, measured once.
///
/// One decode, one luminance plane, one gradient plane and one summed-area table over it. Phase
/// 05's rule - descriptors are computed once - applied inside a phase: the search evaluates a few
/// hundred rectangles and every one of them asks for the energy inside a box, which is four
/// lookups against this table and would otherwise be a loop over a few million pixels.
#[derive(Debug, Clone)]
pub struct Measured {
    /// Proxy width in pixels.
    pub width: usize,
    /// Proxy height in pixels.
    pub height: usize,
    /// The frame's aspect, width over height.
    pub frame_aspect: f32,
    /// Summed-area table of the gradient magnitude, `(width + 1) * (height + 1)`, in `f64`.
    ///
    /// `f64` rather than `f32`, and this is not caution. A summed-area table over four million
    /// pixels accumulates the whole frame's energy in its last entry, and the difference of two
    /// large `f32`s that are close together is where the mantissa runs out - which would make the
    /// energy of a small box near the bottom right of the frame noise rather than a measurement.
    sat: Vec<f64>,
    /// The whole frame's gradient energy, for normalising.
    pub energy: f64,
}

impl Measured {
    /// Measure one proxy.
    ///
    /// The gradient plane rather than the luminance plane, because every term of the objective is
    /// about *structure*: where the detail is, whether it is balanced, and whether something
    /// enters at an edge. A flat wall and a flat sky have the same energy and neither of them
    /// wants to be in the frame.
    #[must_use]
    pub fn of_proxy(rgb: &[f32], width: usize, height: usize) -> Self {
        let frame_aspect = if height == 0 {
            1.5
        } else {
            width as f32 / height as f32
        };
        if width == 0 || height == 0 {
            return Self {
                width,
                height,
                frame_aspect,
                sat: vec![0.0],
                energy: 0.0,
            };
        }
        let luma = aura_render::spatial::luma_plane(rgb, width, height);
        let gradient = aura_render::spatial::gradient_plane(&luma, width, height);

        let stride = width + 1;
        let mut sat = vec![0.0f64; stride * (height + 1)];
        for y in 0..height {
            let mut row_sum = 0.0f64;
            for x in 0..width {
                row_sum += f64::from(gradient.get(y * width + x).copied().unwrap_or(0.0));
                let above = sat.get(y * stride + x + 1).copied().unwrap_or(0.0);
                if let Some(slot) = sat.get_mut((y + 1) * stride + x + 1) {
                    *slot = above + row_sum;
                }
            }
        }
        let energy = sat.last().copied().unwrap_or(0.0);
        Self {
            width,
            height,
            frame_aspect,
            sat,
            energy,
        }
    }

    /// The gradient energy inside a normalised rectangle.
    #[must_use]
    pub fn energy_in(&self, rect: Box2) -> f64 {
        if self.width == 0 || self.height == 0 {
            return 0.0;
        }
        let stride = self.width + 1;
        let to_x = |v: f32| ((v * self.width as f32).round().max(0.0) as usize).min(self.width);
        let to_y = |v: f32| ((v * self.height as f32).round().max(0.0) as usize).min(self.height);
        let x0 = to_x(rect.x);
        let y0 = to_y(rect.y);
        let x1 = to_x(rect.x + rect.w).max(x0);
        let y1 = to_y(rect.y + rect.h).max(y0);
        let at = |x: usize, y: usize| self.sat.get(y * stride + x).copied().unwrap_or(0.0);
        (at(x1, y1) + at(x0, y0) - at(x1, y0) - at(x0, y1)).max(0.0)
    }

    /// Where the frame's structure sits, as a normalised point.
    ///
    /// The fallback subject when nothing else named one. It is an energy centroid rather than a
    /// detection, so on a photograph of a room it lands on whatever is most detailed - which is
    /// usually the thing the photograph is of and is sometimes a chandelier. That is why the
    /// scenes where it would be the only input are the scenes `crop_rules.toml` switches cropping
    /// off for.
    #[must_use]
    pub fn centroid(&self) -> (f32, f32) {
        if self.energy <= 0.0 {
            return (0.5, 0.5);
        }
        // Bisect on each axis for the median of the energy, which is the centroid a summed-area
        // table can answer in logarithmic time and is robust to one very bright corner in a way
        // a first moment is not.
        let axis = |horizontal: bool| -> f32 {
            let (mut low, mut high) = (0.0f32, 1.0f32);
            for _ in 0..20 {
                let mid = f32::midpoint(low, high);
                let half = if horizontal {
                    Box2 {
                        x: 0.0,
                        y: 0.0,
                        w: mid,
                        h: 1.0,
                    }
                } else {
                    Box2 {
                        x: 0.0,
                        y: 0.0,
                        w: 1.0,
                        h: mid,
                    }
                };
                if self.energy_in(half) * 2.0 < self.energy {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            f32::midpoint(low, high)
        };
        (axis(true), axis(false))
    }
}

/// The four terms, and what they multiply to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// How well the subject sits where this scene wants it, `0..1`.
    pub placement: f32,
    /// How evenly the structure is distributed about the rectangle's centre, `0..1`.
    pub balance: f32,
    /// How little is happening right at the rectangle's edge, `0..1`.
    pub edge: f32,
    /// How close the space above the subject is to what this scene wants, `0..1`.
    pub headroom: f32,
    /// The geometric mean of the four, `0..1`.
    pub total: f32,
}

impl Score {
    /// A score of nothing, which is what a degenerate rectangle gets.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            placement: 0.0,
            balance: 0.0,
            edge: 0.0,
            headroom: 0.0,
            total: 0.0,
        }
    }

    /// The geometric mean of four bounded terms.
    #[must_use]
    fn fuse(placement: f32, balance: f32, edge: f32, headroom: f32) -> Self {
        let clamp = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        let (placement, balance, edge, headroom) =
            (clamp(placement), clamp(balance), clamp(edge), clamp(headroom));
        let total = (placement * balance * edge * headroom).max(0.0).powf(0.25);
        Self {
            placement,
            balance,
            edge,
            headroom,
            total,
        }
    }
}

/// What the objective is evaluated against.
#[derive(Debug, Clone)]
pub struct Objective<'a> {
    /// The frame, measured.
    pub frame: &'a Measured,
    /// What the photograph is of, in normalised frame coordinates.
    ///
    /// The union of the protected regions when there are any, phase 11's crop hint when there is
    /// one, and the energy centroid otherwise. The caller decides which, because only the caller
    /// knows which of the three it had - and a term that silently fell back would make a
    /// placement score over a frame with no subject in it indistinguishable from one over a frame
    /// with a subject exactly where it should be.
    pub subject: Box2,
    /// Where this scene wants the subject.
    pub placement: Placement,
    /// The share of the frame's height this scene wants above the subject.
    pub headroom_target: f32,
}

/// Score one rectangle.
///
/// Every term is a function of `rect`. A term that was not would be a constant added to every
/// candidate, which changes no ordering and does change whether the winner clears
/// [`aura_core::contract::geometry::MIN_IMPROVEMENT`] - so a constant term is worse than no term.
#[must_use]
pub fn objective(objective: &Objective<'_>, rect: Box2) -> Score {
    if rect.w <= 1e-4 || rect.h <= 1e-4 {
        return Score::zero();
    }
    Score::fuse(
        placement_term(objective, rect),
        balance_term(objective.frame, rect),
        edge_term(objective.frame, rect),
        headroom_term(objective, rect),
    )
}

/// How close the subject's centre is to the nearest place this scene wants it.
///
/// Measured **inside the rectangle**: the subject's position is re-normalised to the rectangle's
/// own coordinates, which is the whole point - moving the rectangle is what moves the subject
/// relative to the frame that is delivered.
///
/// The *nearest* of the placement's targets rather than a named one, because a subject on the
/// left third and a subject on the right third are equally well placed, and a target that named
/// one of the two would punish half of every wedding for being a mirror image of the other half.
fn placement_term(objective: &Objective<'_>, rect: Box2) -> f32 {
    let cx = objective.subject.x + objective.subject.w / 2.0;
    let cy = objective.subject.y + objective.subject.h / 2.0;
    let u = (cx - rect.x) / rect.w;
    let v = (cy - rect.y) / rect.h;
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        // The subject is outside the rectangle. Not a safety failure - the safety filter owns
        // that and runs first - but a rectangle that does not contain what the photograph is of
        // is a rectangle with no composition to score.
        return 0.0;
    }
    let nearest = objective
        .placement
        .targets()
        .into_iter()
        .map(|(tx, ty)| (u - tx).hypot(v - ty))
        .fold(f32::MAX, f32::min);
    // Normalised by the half-diagonal of a unit square, which is the furthest any point can be
    // from any target inside it. `FRAC_1_SQRT_2` is that half-diagonal exactly - the constant
    // rather than a rounded literal, so the term does not quietly change if somebody types more
    // digits later.
    (1.0 - nearest / core::f32::consts::FRAC_1_SQRT_2).clamp(0.0, 1.0)
}

/// How evenly the structure inside the rectangle sits about its own centre.
///
/// Two evennesses - left against right, top against bottom - and their geometric mean. A frame
/// with everything on one side scores near zero on that axis, and a rectangle that is balanced
/// horizontally and empty in its top half does not average out as balanced.
fn balance_term(frame: &Measured, rect: Box2) -> f32 {
    let total = frame.energy_in(rect);
    if total <= 0.0 {
        // A rectangle with no structure in it at all. Half rather than zero: an empty sky is not
        // unbalanced, it is empty, and scoring it at zero would make the geometric mean refuse
        // every crop of a high-key detail shot.
        return 0.5;
    }
    let left = frame.energy_in(Box2 {
        w: rect.w / 2.0,
        ..rect
    });
    let top = frame.energy_in(Box2 {
        h: rect.h / 2.0,
        ..rect
    });
    let evenness = |half: f64| {
        let share = (half / total).clamp(0.0, 1.0);
        (1.0 - (share - 0.5).abs() * 2.0) as f32
    };
    (evenness(left) * evenness(top)).max(0.0).sqrt()
}

/// How little is happening in the band just inside the rectangle's edge.
///
/// The band's energy *density* against the rectangle's own, so a busy photograph and a quiet one
/// are judged against themselves rather than against each other. A ratio of one is a frame whose
/// edge is as detailed as its middle, which is what a doorway, a stray arm or a bright window at
/// the boundary produces.
///
/// The term is `1 / ratio` above one and flat below it, because an edge *quieter* than the middle
/// is not better than one exactly as quiet - it is a vignette, and rewarding it would push every
/// crop outward onto the darkest border it could find.
fn edge_term(frame: &Measured, rect: Box2) -> f32 {
    let band = (rect.w.min(rect.h) * EDGE_BAND).max(1e-4);
    let inner = Box2 {
        x: rect.x + band,
        y: rect.y + band,
        w: (rect.w - band * 2.0).max(1e-4),
        h: (rect.h - band * 2.0).max(1e-4),
    };
    let total = frame.energy_in(rect);
    if total <= 0.0 {
        return 1.0;
    }
    let inner_energy = frame.energy_in(inner);
    let band_energy = (total - inner_energy).max(0.0);
    let band_area = f64::from((rect.w * rect.h - inner.w * inner.h).max(1e-6));
    let inner_area = f64::from(inner.w * inner.h);
    if band_area <= 0.0 || inner_area <= 0.0 {
        return 1.0;
    }
    let ratio = (band_energy / band_area) / (inner_energy / inner_area).max(1e-9);
    if ratio <= 1.0 {
        1.0
    } else {
        (1.0 / ratio) as f32
    }
}

/// How close the space above the subject is to what this scene asks for.
///
/// Measured as a share of the **rectangle's** height rather than the frame's, because that is
/// what a viewer sees. The tolerance is the target itself: a scene asking for ten per cent
/// headroom scores zero at zero and at twenty, and one at exactly ten. That is wide enough not to
/// fight the placement term and narrow enough that a crop which puts somebody's head against the
/// top of the frame cannot win.
fn headroom_term(objective: &Objective<'_>, rect: Box2) -> f32 {
    let target = objective.headroom_target.clamp(0.01, 0.45);
    let gap = (objective.subject.y - rect.y) / rect.h;
    if !gap.is_finite() {
        return 0.0;
    }
    (1.0 - (gap - target).abs() / target).clamp(0.0, 1.0)
}

/// A candidate the search produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The rectangle.
    pub rect: Box2,
    /// Its four terms.
    pub score: Score,
}

/// What the search is allowed to consider.
#[derive(Debug, Clone)]
pub struct Search<'a> {
    /// The objective.
    pub objective: Objective<'a>,
    /// The regions that may not be cut.
    pub protected: &'a [aura_core::contract::geometry::ProtectedRegion],
    /// The safety limits.
    pub limits: Limits,
    /// The scene's own rule.
    pub rule: SceneRule,
    /// The rectangle every candidate must sit inside.
    ///
    /// The whole frame for an unrotated photograph, and the rotation-induced crop for a
    /// straightened one - which is what makes "a rotation costs a crop, and the crop is computed
    /// before the rotation is agreed to" true rather than aspirational.
    pub bounds: Box2,
}

/// The best rectangle at one aspect, or `None` when every candidate was refused.
///
/// **Every candidate is checked for safety before its score is looked at**, and an unsafe one is
/// dropped rather than penalised. The returned codes are the union of what was refused, so a
/// caller can say *why* there is no square crop of a photograph rather than only that there is
/// not one.
#[must_use]
pub fn search(search: &Search<'_>, aspect: AspectRatio) -> (Option<Candidate>, Vec<GeometryCode>) {
    let mut refusals: Vec<GeometryCode> = Vec::new();
    let mut best: Option<Candidate> = None;
    if search.bounds.w <= 1e-4 || search.bounds.h <= 1e-4 {
        return (None, refusals);
    }

    let frame_aspect = search.limits.floored().frame_aspect;
    let tightest = search
        .rule
        .max_zoom
        .max(search.limits.floored().min_long_edge)
        .clamp(0.05, 1.0);

    for step in 0..SCALE_STEPS {
        // From the bounds outward-in. `SCALE_STEPS - 1` divisions so the first scale is exactly
        // the bounds and the last is exactly the scene's tightest zoom - a ladder that reached
        // neither end would be a search that could not return the frame it was given.
        let t = step as f32 / (SCALE_STEPS - 1).max(1) as f32;
        let scale = 1.0 + t * (tightest - 1.0);

        for oy in 0..OFFSET_STEPS {
            for ox in 0..OFFSET_STEPS {
                let centre = (
                    offset_at(ox, search.bounds.x, search.bounds.w),
                    offset_at(oy, search.bounds.y, search.bounds.h),
                );
                let rect = candidate_rect(search, aspect, frame_aspect, scale, centre);
                if rect.w <= 1e-4 || rect.h <= 1e-4 {
                    continue;
                }
                let outcome = safety::check(rect, search.protected, search.limits);
                if !outcome.codes.is_empty() {
                    for code in outcome.codes {
                        if !refusals.contains(&code) {
                            refusals.push(code);
                        }
                    }
                    continue;
                }
                let score = objective(&search.objective, rect);
                let better = match &best {
                    None => true,
                    // Strictly better, and ties go to the earlier candidate - which is the
                    // larger, less-offset one, because the ladder runs outward-in. Determinism,
                    // invariant 4: a tie broken by iteration order is a tie broken the same way
                    // on every machine.
                    Some(current) => score.total > current.score.total + 1e-6,
                };
                if better {
                    best = Some(Candidate { rect, score });
                }
            }
        }
    }
    (best, refusals)
}

/// The `n`th offset along an axis, in normalised frame coordinates.
fn offset_at(n: usize, origin: f32, extent: f32) -> f32 {
    let t = n as f32 / (OFFSET_STEPS - 1).max(1) as f32;
    origin + extent * t
}

/// One candidate rectangle: the aspect fitted inside the bounds at a scale, centred on a point.
fn candidate_rect(
    search: &Search<'_>,
    aspect: AspectRatio,
    frame_aspect: f32,
    scale: f32,
    centre: (f32, f32),
) -> Box2 {
    let scaled = Box2 {
        x: search.bounds.x,
        y: search.bounds.y,
        w: search.bounds.w * scale,
        h: search.bounds.h * scale,
    };
    match aspect.ratio() {
        // The frame's own shape. Straightening keeps the shape and so does a plain tightening;
        // an "original" crop that quietly changed the aspect would deliver a 1.63:1 photograph
        // from a 3:2 one, which is `rotation_crop`'s argument applied to the search.
        None => {
            // `.max(origin)` for the reason `fit_aspect` needs it: at the first rung of the
            // scale ladder the rectangle *is* the bounds, and `bounds.x + bounds.w - w` lands a
            // few ulps under `bounds.x`, which is a `clamp` whose minimum exceeds its maximum.
            let x = (centre.0 - scaled.w / 2.0).clamp(
                search.bounds.x,
                (search.bounds.x + search.bounds.w - scaled.w).max(search.bounds.x),
            );
            let y = (centre.1 - scaled.h / 2.0).clamp(
                search.bounds.y,
                (search.bounds.y + search.bounds.h - scaled.h).max(search.bounds.y),
            );
            Box2 { x, y, ..scaled }
        }
        Some(ratio) => fit_aspect(scaled_bounds(search.bounds, scale), frame_aspect, ratio, centre),
    }
}

/// The bounds shrunk by a scale, still anchored so that a shifted rectangle can reach the edges.
fn scaled_bounds(bounds: Box2, scale: f32) -> Box2 {
    Box2 {
        x: bounds.x,
        y: bounds.y,
        w: bounds.w * scale,
        h: bounds.h * scale,
    }
}

/// Turn a candidate into a stored variant.
#[must_use]
pub fn into_variant(candidate: &Candidate, aspect: AspectRatio, safe: bool) -> CropVariant {
    CropVariant {
        aspect,
        rect: candidate.rect,
        purpose: if aspect == AspectRatio::Original {
            CropPurpose::Primary
        } else {
            aspect.purpose()
        },
        score: candidate.score.total,
        safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::geometry::{ProtectedContent, ProtectedRegion};
    use aura_core::SceneId;

    /// A frame whose structure is a bright block somewhere in it, painted into the pixels.
    fn frame_with_block(width: usize, height: usize, block: Box2) -> Vec<f32> {
        let mut rgb = vec![0.10f32; width * height * 3];
        let x0 = (block.x * width as f32) as usize;
        let y0 = (block.y * height as f32) as usize;
        let x1 = ((block.x + block.w) * width as f32) as usize;
        let y1 = ((block.y + block.h) * height as f32) as usize;
        for y in y0..y1.min(height) {
            for x in x0..x1.min(width) {
                // A chequer rather than a flat block: a flat block has gradient only at its
                // boundary, and every term here is measured on the gradient.
                let value = if (x / 2 + y / 2) % 2 == 0 { 0.85 } else { 0.20 };
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut((y * width + x) * 3 + channel) {
                        *slot = value;
                    }
                }
            }
        }
        rgb
    }

    fn measured(block: Box2) -> Measured {
        let (w, h) = (192usize, 128usize);
        Measured::of_proxy(&frame_with_block(w, h, block), w, h)
    }

    #[test]
    fn the_energy_of_the_whole_frame_is_the_sum_of_its_quadrants() {
        let frame = measured(Box2 {
            x: 0.55,
            y: 0.2,
            w: 0.3,
            h: 0.3,
        });
        let quadrants: f64 = [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5), (0.5, 0.5)]
            .into_iter()
            .map(|(x, y)| {
                frame.energy_in(Box2 {
                    x,
                    y,
                    w: 0.5,
                    h: 0.5,
                })
            })
            .sum();
        assert!(
            (quadrants - frame.energy).abs() / frame.energy.max(1e-9) < 1e-6,
            "{quadrants} != {}",
            frame.energy
        );
    }

    #[test]
    fn the_centroid_finds_where_the_structure_is() {
        let frame = measured(Box2 {
            x: 0.60,
            y: 0.10,
            w: 0.25,
            h: 0.25,
        });
        let (cx, cy) = frame.centroid();
        assert!(cx > 0.55 && cx < 0.90, "{cx}");
        assert!(cy > 0.05 && cy < 0.40, "{cy}");
    }

    #[test]
    fn a_subject_on_a_power_point_scores_above_one_in_the_corner() {
        let frame = measured(Box2 {
            x: 0.3,
            y: 0.3,
            w: 0.2,
            h: 0.2,
        });
        let subject = Box2 {
            x: 0.30,
            y: 0.30,
            w: 0.06,
            h: 0.06,
        };
        let objective_ = Objective {
            frame: &frame,
            subject,
            placement: Placement::Thirds,
            headroom_target: 0.10,
        };
        // A rectangle that puts the subject exactly on the top-left power point.
        let on_point = Box2 {
            x: 0.33 - 1.0 / 3.0 * 0.6,
            y: 0.33 - 1.0 / 3.0 * 0.6,
            w: 0.6,
            h: 0.6,
        };
        let corner = Box2 {
            x: 0.30,
            y: 0.30,
            w: 0.6,
            h: 0.6,
        };
        assert!(
            placement_term(&objective_, on_point) > placement_term(&objective_, corner),
            "a power point must beat a corner"
        );
    }

    #[test]
    fn one_term_at_zero_is_a_rectangle_at_zero() {
        // The geometric mean, as a test. A rectangle that does not contain the subject scores
        // zero on placement, and nothing the other three terms can do rescues it.
        let frame = measured(Box2 {
            x: 0.1,
            y: 0.1,
            w: 0.2,
            h: 0.2,
        });
        let objective_ = Objective {
            frame: &frame,
            subject: Box2 {
                x: 0.05,
                y: 0.05,
                w: 0.05,
                h: 0.05,
            },
            placement: Placement::Thirds,
            headroom_target: 0.10,
        };
        let elsewhere = Box2 {
            x: 0.5,
            y: 0.5,
            w: 0.4,
            h: 0.4,
        };
        assert!(objective(&objective_, elsewhere).total.abs() < 1e-6);
    }

    #[test]
    fn a_busy_edge_scores_below_a_quiet_one() {
        // Structure hard against the left edge of one rectangle and in the middle of another.
        let frame = measured(Box2 {
            x: 0.0,
            y: 0.30,
            w: 0.06,
            h: 0.40,
        });
        let touching = Box2 {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        let clear = Box2 {
            x: 0.30,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        assert!(
            edge_term(&frame, touching) < edge_term(&frame, clear),
            "{} !< {}",
            edge_term(&frame, touching),
            edge_term(&frame, clear)
        );
    }

    #[test]
    fn the_search_never_returns_a_rectangle_that_cuts_a_face() {
        // Section 10.1's hard gate, at the level of the search rather than of the pass: faces
        // scattered across the frame, and every candidate that would cut one is dropped.
        let frame = measured(Box2 {
            x: 0.3,
            y: 0.3,
            w: 0.4,
            h: 0.4,
        });
        let protected: Vec<ProtectedRegion> = [(0.05, 0.10), (0.85, 0.12), (0.45, 0.80)]
            .into_iter()
            .map(|(x, y)| {
                ProtectedRegion::anonymous(
                    ProtectedContent::Face,
                    Box2 {
                        x,
                        y,
                        w: 0.08,
                        h: 0.08,
                    },
                )
            })
            .collect();
        let rule = SceneRule {
            scene: SceneId::Candid,
            crop: true,
            min_improvement: 0.06,
            max_zoom: 0.62,
            headroom: 0.10,
            placement: Placement::Thirds,
        };
        let s = Search {
            objective: Objective {
                frame: &frame,
                subject: Box2 {
                    x: 0.3,
                    y: 0.3,
                    w: 0.4,
                    h: 0.4,
                },
                placement: Placement::Thirds,
                headroom_target: 0.10,
            },
            protected: &protected,
            limits: Limits::default(),
            rule,
            bounds: Box2::FULL,
        };
        let (best, refusals) = search(&s, AspectRatio::Original);
        if let Some(candidate) = &best {
            for region in &protected {
                assert!(
                    safety::inside(region, candidate.rect, Limits::default().margin),
                    "the winner cuts a face at {:?}",
                    region.area
                );
            }
        }
        assert!(
            refusals.contains(&GeometryCode::CropCutsFace),
            "no candidate was refused for cutting a face, so the test proved nothing"
        );
    }

    #[test]
    fn the_search_is_deterministic() {
        let frame = measured(Box2 {
            x: 0.55,
            y: 0.25,
            w: 0.2,
            h: 0.3,
        });
        let rule = SceneRule {
            scene: SceneId::Candid,
            crop: true,
            min_improvement: 0.06,
            max_zoom: 0.70,
            headroom: 0.10,
            placement: Placement::Thirds,
        };
        let s = Search {
            objective: Objective {
                frame: &frame,
                subject: Box2 {
                    x: 0.55,
                    y: 0.25,
                    w: 0.2,
                    h: 0.3,
                },
                placement: Placement::Thirds,
                headroom_target: 0.10,
            },
            protected: &[],
            limits: Limits::default(),
            rule,
            bounds: Box2::FULL,
        };
        let first = search(&s, AspectRatio::Original).0;
        let second = search(&s, AspectRatio::Original).0;
        assert_eq!(first, second);
    }

    #[test]
    fn an_aspect_variant_comes_back_at_that_aspect() {
        let frame = measured(Box2 {
            x: 0.4,
            y: 0.3,
            w: 0.2,
            h: 0.3,
        });
        let rule = SceneRule {
            scene: SceneId::CouplePortrait,
            crop: true,
            min_improvement: 0.06,
            max_zoom: 0.75,
            headroom: 0.12,
            placement: Placement::Thirds,
        };
        let s = Search {
            objective: Objective {
                frame: &frame,
                subject: Box2 {
                    x: 0.4,
                    y: 0.3,
                    w: 0.2,
                    h: 0.3,
                },
                placement: Placement::Thirds,
                headroom_target: 0.12,
            },
            protected: &[],
            limits: Limits {
                frame_aspect: frame.frame_aspect,
                ..Limits::default()
            },
            rule,
            bounds: Box2::FULL,
        };
        for aspect in AspectRatio::VARIANTS {
            let (best, _) = search(&s, aspect);
            let candidate = best.unwrap_or_else(|| panic!("{aspect} produced nothing"));
            let pixel_aspect =
                (candidate.rect.w * frame.frame_aspect) / candidate.rect.h.max(1e-6);
            let wanted = aspect.ratio().unwrap_or(frame.frame_aspect);
            assert!(
                (pixel_aspect - wanted).abs() < 0.02,
                "{aspect}: {pixel_aspect} != {wanted}"
            );
        }
    }
}
