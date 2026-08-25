//! Candidate generation and the composition objective. PHASE-23 section 6.3.
//!
//! Section 9 gives MLL "define the crop objective and improvement margin". This module is that
//! objective, and the first thing to say about it is what it is **not**: it is not phase 11's
//! `composition_score`. That composite is measured from pixels - background edge energy,
//! colour competition, an aesthetic head - and re-measuring it over nine hundred candidate
//! rectangles is nine hundred decodes per photograph against a forty-millisecond budget.
//!
//! What is here instead is a four-term objective computed from **the evidence phase 11 already
//! published**: where the subject is, where the faces are, where the distractions are, and how
//! much room is above the topmost head. It is evaluated identically over the frame as shot and
//! over every candidate, which is what makes [`IMPROVEMENT_MARGIN`] a margin between two
//! comparable numbers rather than a comparison between a measurement and a proxy for one.
//!
//! ## A weighted geometric mean, so nothing rescues anything
//!
//! The house shape since phase 09's `technical_score`: a product of terms rather than a sum,
//! so a candidate that places the subject beautifully and puts a face two pixels from the edge
//! cannot average its way to a win. Phase 12's fusion makes the same argument in the same
//! words, and this phase needs it more than most - the terms are correlated, and a sum lets a
//! tighter crop buy placement with edge cleanliness on nearly every frame.
//!
//! ## The search is a grid, and it is bounded
//!
//! Nine scales by a five-by-five grid of positions, per aspect. Deterministic, exhaustive over
//! its own lattice, and about two hundred candidates - which is the right size when the *fine*
//! answer is nearly always "keep the original framing" and the coarse question is "is there a
//! clearly better rectangle at all". A gradient walk would find a local optimum half a per cent
//! better and would not be reproducible across two builds of the same compiler.

use aura_core::contract::geometry::{
    Aspect, CropPurpose, CropVariant, ProtectedKind, ProtectedRegion,
};
use aura_core::contract::integrity::CropRect;

use crate::safety::{self, SafetyInput};

/// How many scales the search tries, from the whole frame down to the resolution floor.
pub const SCALE_STEPS: usize = 9;

/// How many positions per axis at each scale.
///
/// Five, which is a centre, two thirds and two edges. An even number has no centre, and the
/// centre is the position most candidates want.
pub const POSITION_STEPS: usize = 5;

/// The four terms' weights in the geometric mean, in the order they are computed.
///
/// Placement first because it is what a photographer means by "better framed"; edge
/// cleanliness a close second because it is what they notice when it is wrong. Headroom is
/// deliberately the lightest: it is scene-conditioned and its band is wide, so a heavy weight
/// on it would make the objective chase a number that is right over a whole interval.
pub const WEIGHTS: [f32; 4] = [0.35, 0.30, 0.20, 0.15];

/// What a distraction left whole inside the crop costs the edge term, per unit area.
pub const INSIDE_COST: f32 = 3.0;

/// What a distraction the crop's border runs through costs, per unit area.
///
/// More than ten times [`INSIDE_COST`], and the ratio is the term rather than a tuning
/// constant: below about ten to one the placement a tighter rectangle buys outscores the
/// distraction it cut through, and the objective starts preferring to slice a bright window in
/// half over leaving it alone. That is the single most visible mistake an automatic crop can
/// make, and it was the objective's behaviour on its first run.
pub const STRADDLE_COST: f32 = 40.0;

/// Everything the objective reads. All rectangles are in the **corrected** frame.
#[derive(Debug, Clone, Copy)]
pub struct Objective<'a> {
    /// Faces, hands and key content.
    pub regions: &'a [ProtectedRegion],
    /// Bright blobs and edge intrusions from phase 11.
    pub distractions: &'a [CropRect],
    /// What the frame is about, when phase 11 said.
    pub subject: Option<CropRect>,
    /// The scene's headroom band, as a fraction of the crop's height.
    pub headroom: (f32, f32),
    /// Width over height of the frame.
    pub aspect: f32,
}

impl Objective<'_> {
    /// Score one rectangle, `0..1`.
    #[must_use]
    pub fn score(&self, rect: CropRect) -> f32 {
        if rect.is_empty() {
            return 0.0;
        }
        let terms = [
            self.placement(rect),
            self.edge_cleanliness(rect),
            self.balance(rect),
            self.headroom_term(rect),
        ];
        geometric_mean(&terms, &WEIGHTS)
    }

    /// How close the subject sits to a power point of this rectangle.
    ///
    /// Both thirds intersections on the subject's own side, and the centre - because a centred
    /// subject is right in four of phase 11's scenes and scoring it as a failure would make
    /// the objective crop every ritual frame off-axis.
    fn placement(&self, rect: CropRect) -> f32 {
        let Some(subject) = self.subject.or_else(|| self.dominant_face()) else {
            // Nothing to place. Neutral rather than zero: a `details` flat-lay has no subject
            // and is not badly composed for it.
            return 0.75;
        };
        let cx = (subject.x + subject.w / 2.0 - rect.x) / rect.w.max(1e-6);
        let cy = (subject.y + subject.h / 2.0 - rect.y) / rect.h.max(1e-6);
        if !(0.0..=1.0).contains(&cx) || !(0.0..=1.0).contains(&cy) {
            return 0.0;
        }
        let targets = [
            (1.0 / 3.0, 1.0 / 3.0),
            (2.0 / 3.0, 1.0 / 3.0),
            (1.0 / 3.0, 2.0 / 3.0),
            (2.0 / 3.0, 2.0 / 3.0),
            (0.5, 0.5),
        ];
        // Normalised by the rectangle's own diagonal, so a tight crop is not rewarded merely
        // for making every distance smaller.
        let aspect = self.aspect * rect.w / rect.h.max(1e-6);
        let diagonal = (aspect * aspect + 1.0).sqrt();
        let nearest = targets
            .iter()
            .map(|(tx, ty)| {
                let dx = (cx - tx) * aspect;
                let dy = cy - ty;
                (dx * dx + dy * dy).sqrt()
            })
            .fold(f32::INFINITY, f32::min);
        (1.0 - (nearest / (diagonal * 0.35)).clamp(0.0, 1.0)).clamp(0.0, 1.0)
    }

    /// How much is crowding this rectangle's border.
    ///
    /// A distraction the crop *removes* costs nothing; one it cuts through, or leaves just
    /// inside the edge, costs. This is the term that makes a tighter frame worth taking when
    /// the tighter frame is the one that excludes the exit sign.
    fn edge_cleanliness(&self, rect: CropRect) -> f32 {
        let band = 0.06f32;
        let mut penalty = 0.0f32;
        for distraction in self.distractions {
            let overlap = intersection_area(*distraction, rect);
            if overlap <= 0.0 {
                continue; // Removed entirely. The reward for cropping it out.
            }
            let inner = CropRect {
                x: rect.x + rect.w * band,
                y: rect.y + rect.h * band,
                w: rect.w * (1.0 - 2.0 * band),
                h: rect.h * (1.0 - 2.0 * band),
            };
            let deep = intersection_area(*distraction, inner);
            // Fully inside costs a little; straddling the border costs **far** more, because a
            // half-cropped bright blob reads as a mistake rather than as a background. The
            // ratio between the two is the whole term: at anything under about ten to one the
            // placement a tighter crop buys outscores the distraction it cut through, and the
            // objective quietly prefers slicing a bright window in half to leaving it whole.
            let straddle = (overlap - deep).max(0.0);
            penalty += overlap * INSIDE_COST + straddle * STRADDLE_COST;
        }
        for region in self.regions.iter().filter(|r| r.kind == ProtectedKind::Face) {
            if !region.is_inside(rect, 0.04) && region.is_inside(rect, 0.0) {
                // Inside, but only just. Not a refusal - that is the filter's job - but a face
                // pressed against the edge is a frame nobody would have shot on purpose.
                penalty += 0.15;
            }
        }
        (1.0 - penalty).clamp(0.05, 1.0)
    }

    /// How evenly the visual weight sits about this rectangle's centre.
    fn balance(&self, rect: CropRect) -> f32 {
        let mut weight = 0.0f32;
        let mut moment = 0.0f32;
        for region in self.regions.iter().filter(|r| r.kind != ProtectedKind::Hands) {
            let area = region.rect.w * region.rect.h;
            if area <= 0.0 {
                continue;
            }
            let cx = (region.rect.x + region.rect.w / 2.0 - rect.x) / rect.w.max(1e-6);
            if !(0.0..=1.0).contains(&cx) {
                continue;
            }
            weight += area;
            moment += area * (cx - 0.5);
        }
        if weight <= 0.0 {
            return 0.80;
        }
        let offset = (moment / weight).abs();
        (1.0 - (offset / 0.30).clamp(0.0, 1.0)).clamp(0.05, 1.0)
    }

    /// How well the space above the topmost head sits inside the scene's band.
    fn headroom_term(&self, rect: CropRect) -> f32 {
        let Some(top) = self
            .regions
            .iter()
            .filter(|r| r.kind == ProtectedKind::Face)
            .map(|r| r.rect.y)
            .fold(None, |acc: Option<f32>, y| {
                Some(acc.map_or(y, |best| best.min(y)))
            })
        else {
            return 0.80; // No face; the concept does not apply.
        };
        let room = (top - rect.y) / rect.h.max(1e-6);
        if room < 0.0 {
            return 0.05; // The head is above the crop's top edge. The filter refuses it too.
        }
        let (lo, hi) = self.headroom;
        if (lo..=hi).contains(&room) {
            return 1.0;
        }
        let distance = if room < lo { lo - room } else { room - hi };
        (1.0 - (distance / 0.20).clamp(0.0, 1.0)).clamp(0.05, 1.0)
    }

    fn dominant_face(&self) -> Option<CropRect> {
        self.regions
            .iter()
            .filter(|r| r.kind == ProtectedKind::Face)
            .max_by(|a, b| {
                (a.rect.w * a.rect.h)
                    .partial_cmp(&(b.rect.w * b.rect.h))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|region| region.rect)
    }
}

/// Every candidate rectangle for one aspect, largest first.
///
/// Largest first so that ties resolve toward keeping more of the photograph, which is this
/// phase's whole disposition.
#[must_use]
pub fn candidates(aspect: Aspect, frame_aspect: f32, floor: f32) -> Vec<CropRect> {
    let target = aspect.ratio();
    let mut out = Vec::with_capacity(SCALE_STEPS * POSITION_STEPS * POSITION_STEPS);
    for step in 0..SCALE_STEPS {
        // From the whole frame down to a little under the floor; the filter refuses whatever
        // actually falls below it, so the lattice does not have to know the exact boundary.
        let t = step as f32 / (SCALE_STEPS - 1) as f32;
        let scale = 1.0 - t * (1.0 - floor * 0.95);
        let (w, h) = match target {
            None => (scale, scale),
            Some(ratio) => {
                // The largest `ratio`-shaped rectangle inside a `scale`-sized box.
                let want = ratio / frame_aspect; // in normalised frame units
                if want >= 1.0 {
                    (scale, scale / want)
                } else {
                    (scale * want, scale)
                }
            }
        };
        if w > 1.0 || h > 1.0 {
            continue;
        }
        for iy in 0..POSITION_STEPS {
            for ix in 0..POSITION_STEPS {
                let room_x = 1.0 - w;
                let room_y = 1.0 - h;
                let fx = if POSITION_STEPS > 1 {
                    ix as f32 / (POSITION_STEPS - 1) as f32
                } else {
                    0.5
                };
                let fy = if POSITION_STEPS > 1 {
                    iy as f32 / (POSITION_STEPS - 1) as f32
                } else {
                    0.5
                };
                out.push(CropRect {
                    x: room_x * fx,
                    y: room_y * fy,
                    w,
                    h,
                });
                if room_x <= 1e-6 && room_y <= 1e-6 {
                    break;
                }
            }
        }
    }
    out.dedup_by(|a, b| {
        (a.x - b.x).abs() < 1e-6
            && (a.y - b.y).abs() < 1e-6
            && (a.w - b.w).abs() < 1e-6
            && (a.h - b.h).abs() < 1e-6
    });
    out
}

/// The best safe candidate for one aspect, with the refusal histogram.
///
/// `None` when every candidate was refused, which is the honest answer for a dance floor frame
/// with a limb at every edge.
#[must_use]
pub fn best(
    aspect: Aspect,
    purpose: CropPurpose,
    objective: &Objective<'_>,
    input: &SafetyInput<'_>,
) -> (Option<CropVariant>, [u32; 4]) {
    let all = candidates(aspect, objective.aspect, input.resolution_floor);
    let (safe, refused) = safety::filter(all, input);
    let mut best: Option<CropVariant> = None;
    for rect in safe {
        let score = objective.score(rect);
        let better = match best {
            None => true,
            // A strict improvement, so the largest-first ordering decides ties and the search
            // keeps more of the photograph when two rectangles score the same.
            Some(current) => score > current.score + 1e-6,
        };
        if better {
            best = Some(CropVariant {
                aspect,
                rect,
                purpose,
                score,
                safe: true,
            });
        }
    }
    (best, refused)
}

fn geometric_mean(terms: &[f32], weights: &[f32]) -> f32 {
    let mut total = 0.0f32;
    let mut sum = 0.0f32;
    for (term, weight) in terms.iter().zip(weights.iter()) {
        total += weight * term.clamp(1e-3, 1.0).ln();
        sum += weight;
    }
    if sum <= 0.0 {
        return 0.0;
    }
    (total / sum).exp().clamp(0.0, 1.0)
}

fn intersection_area(a: CropRect, b: CropRect) -> f32 {
    let x = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let y = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    if x <= 0.0 || y <= 0.0 {
        0.0
    } else {
        x * y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASPECT: f32 = 1.5;

    fn face(x: f32, y: f32, size: f32) -> ProtectedRegion {
        ProtectedRegion {
            kind: ProtectedKind::Face,
            identity: None,
            rect: CropRect {
                x,
                y,
                w: size,
                h: size * ASPECT,
            },
            primary: true,
        }
    }

    fn objective<'a>(
        regions: &'a [ProtectedRegion],
        distractions: &'a [CropRect],
    ) -> Objective<'a> {
        Objective {
            regions,
            distractions,
            subject: None,
            headroom: (0.05, 0.20),
            aspect: ASPECT,
        }
    }

    #[test]
    fn one_bad_term_cannot_be_averaged_away() {
        // Placement perfect, edge cleanliness destroyed. A weighted sum would score about
        // 0.75; the geometric mean must not.
        let sum = 0.35 * 1.0 + 0.30 * 0.05 + 0.20 * 1.0 + 0.15 * 1.0;
        let mean = geometric_mean(&[1.0, 0.05, 1.0, 1.0], &WEIGHTS);
        assert!(sum > 0.70, "the sum baseline changed: {sum}");
        assert!(mean < 0.45, "the geometric mean rescued a broken term: {mean}");
    }

    #[test]
    fn a_crop_that_removes_a_distraction_scores_better_than_one_that_keeps_it() {
        let regions = [face(0.30, 0.25, 0.10)];
        // A bright blob in the top-right corner.
        let distractions = [CropRect {
            x: 0.80,
            y: 0.02,
            w: 0.16,
            h: 0.16,
        }];
        let obj = objective(&regions, &distractions);
        let keeps = CropRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let removes = CropRect {
            x: 0.0,
            y: 0.0,
            w: 0.74,
            h: 0.90,
        };
        assert!(
            obj.score(removes) > obj.score(keeps),
            "{} vs {}",
            obj.score(removes),
            obj.score(keeps)
        );
    }

    #[test]
    fn a_crop_that_straddles_a_distraction_scores_worse_than_one_that_keeps_it_whole() {
        let regions = [face(0.30, 0.30, 0.10)];
        let distractions = [CropRect {
            x: 0.60,
            y: 0.30,
            w: 0.20,
            h: 0.20,
        }];
        let obj = objective(&regions, &distractions);
        let whole = CropRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let straddles = CropRect {
            x: 0.0,
            y: 0.0,
            w: 0.70,
            h: 1.0,
        };
        assert!(obj.score(whole) > obj.score(straddles));
    }

    #[test]
    fn a_subject_on_a_power_point_beats_one_hard_against_the_edge() {
        let regions = [face(0.30, 0.28, 0.10)];
        let none: [CropRect; 0] = [];
        let obj = objective(&regions, &none);
        let placed = CropRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let shoved = CropRect {
            x: 0.24,
            y: 0.18,
            w: 0.70,
            h: 0.70,
        };
        assert!(obj.score(placed) > obj.score(shoved));
    }

    #[test]
    fn a_centred_subject_is_not_scored_as_a_failure() {
        let regions = [face(0.45, 0.42, 0.10)];
        let none: [CropRect; 0] = [];
        let obj = objective(&regions, &none);
        let full = CropRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        assert!(
            obj.score(full) > 0.55,
            "a centred subject scored {}",
            obj.score(full)
        );
    }

    #[test]
    fn a_frame_with_nothing_in_it_is_neutral_rather_than_zero() {
        let none_r: [ProtectedRegion; 0] = [];
        let none_d: [CropRect; 0] = [];
        let obj = objective(&none_r, &none_d);
        let score = obj.score(CropRect::FULL);
        assert!((0.6..=0.9).contains(&score), "{score}");
    }

    #[test]
    fn the_candidate_lattice_is_bounded_and_deterministic() {
        let first = candidates(Aspect::Original, ASPECT, 0.60);
        let again = candidates(Aspect::Original, ASPECT, 0.60);
        assert_eq!(first, again);
        assert!(
            first.len() <= SCALE_STEPS * POSITION_STEPS * POSITION_STEPS,
            "{}",
            first.len()
        );
        assert!(first.len() > 50, "{}", first.len());
        // Largest first.
        let areas: Vec<f32> = first.iter().map(|r| r.w * r.h).collect();
        assert!(areas.first().copied().unwrap_or(0.0) >= areas.last().copied().unwrap_or(1.0));
    }

    #[test]
    fn every_candidate_for_an_aspect_has_that_aspect() {
        for aspect in [
            Aspect::FourFive,
            Aspect::FiveFour,
            Aspect::Square,
            Aspect::SixteenNine,
        ] {
            let want = aspect.ratio().unwrap_or(1.0);
            for rect in candidates(aspect, ASPECT, 0.60) {
                let got = rect.w * ASPECT / rect.h;
                assert!(
                    (got - want).abs() < 1e-3,
                    "{aspect}: wanted {want}, got {got}"
                );
                assert!(rect.x >= -1e-6 && rect.x + rect.w <= 1.0 + 1e-6);
                assert!(rect.y >= -1e-6 && rect.y + rect.h <= 1.0 + 1e-6);
            }
        }
    }

    #[test]
    fn a_frame_with_a_face_at_every_edge_yields_no_candidate() {
        let regions = [
            face(0.01, 0.40, 0.06),
            face(0.93, 0.40, 0.06),
            face(0.45, 0.01, 0.06),
        ];
        let none: [CropRect; 0] = [];
        let obj = objective(&regions, &none);
        let input = SafetyInput {
            regions: &regions,
            aspect: ASPECT,
            resolution_floor: 0.60,
        };
        let (best, refused) = best(Aspect::Square, CropPurpose::Social, &obj, &input);
        assert!(best.is_none(), "a square crop survived a frame full of edges");
        assert!(refused.iter().sum::<u32>() > 0);
    }
}
