//! The two guards that bound a grade before the skin guard measures it: clipping, and
//! subtlety.
//!
//! PHASE-16 section 6.4:
//!
//! > Clipping guard re-solves any parameter set that introduces new clipping above the scene
//! > tolerance; the reason states which parameter was reduced.
//!
//! and section 12's first failure mode:
//!
//! > Over-graded, HDR-looking results - subtlety metric trained on professional edits,
//! > magnitude penalties in the solver, and expert audit gate.
//!
//! Both are here because both are *reductions*: neither can make a grade stronger, so running
//! them in either order gives the same answer, and running them before the skin guard means
//! the skin guard measures the grade that will actually be stored.
//!
//! ## What the clipping guard measures, and what it does not
//!
//! It replays the **luminance path** - tone, contrast, curve - over the frame's own 256-bin
//! luminance histogram, and counts how much of the frame lands at or above
//! [`CLIP_LEVEL`] afterwards. That is exact for those three stages, because all three are
//! functions of luminance alone.
//!
//! It does **not** replay the HSL luminance term, which is per-hue and cannot be evaluated
//! from a luminance histogram. That is a deliberate gap with a bound around it: a band's
//! luminance shift is capped by `SceneIntent::max_band_shift`, the largest value any shipped
//! row carries is 14 units, and the renderer scales `l` by half - so the most any band can
//! move a pixel's luminance is seven percent, on a pixel that has to be at that exact hue.
//! The tolerance the guard works to is a scene-level number an order of magnitude larger.
//!
//! ## Which parameter is reduced, and how that is decided
//!
//! **Measured, not assumed.** Three candidates - the white point, the contrast and the curve -
//! are each tried at [`REDUCTION`] of their current value, the clipping is re-measured for
//! each, and the one that helps most is the one that moves.
//!
//! A fixed order was the first design and it was wrong on the frames that matter: a dress
//! detail clips because of the *curve*, and a guard that spends every one of its attempts
//! reducing a white point that was not the cause reports four reductions and still clips.
//!
//! Highlights are never a candidate - they only ever pull clipping down.
//!
//! The reason names which one moved, which is section 6.4's own requirement and the
//! difference between a photographer being told "AURA reduced the contrast here because the
//! sky would have blown" and being told nothing.

use aura_core::contract::colour::{ColourCode, ToneCurve};
use aura_recipe::Curve;
use aura_render::tonemap::{self, CurveLut, Tone};

use crate::colour::codec::{self, Explained};
use crate::colour::curve;
use crate::colour::hsl::Solved as HslSolved;
use crate::colour::skin_guard::Grade;
use crate::colour::tone::ToneParams;

/// The luminance at or above which a pixel counts as clipped.
///
/// 0.995 rather than 1.0. A pixel at 0.997 has lost its detail as completely as one at 1.0,
/// and a threshold at exactly 1.0 measures a rounding rule instead of a photograph.
pub const CLIP_LEVEL: f32 = 0.995;

/// The luminance at or below which a pixel counts as crushed.
pub const CRUSH_LEVEL: f32 = 0.004;

/// How many times a grade is reduced before the guard gives up and returns what it has.
///
/// Six, which is two full passes over the three candidates. Each step takes a parameter to
/// [`REDUCTION`] of what it was, so six steps can take any one of them below a fiftieth -
/// indistinguishable from zero for everything this phase sets.
pub const MAX_RESOLVES: u8 = 6;

/// What fraction of itself a reduced parameter keeps.
///
/// 0.4 rather than 0.5. Halving needs seven steps to make a fifty-unit contrast invisible and
/// the guard has six; 0.4 needs four. The cost of the coarser step is that the guard
/// occasionally reduces slightly more than it had to, which is the right direction to be
/// wrong in when the alternative is a delivered frame with a blown dress.
pub const REDUCTION: f32 = 0.4;

/// How much of the frame's clipping is attributed to a parameter before it is reduced.
///
/// The guard reduces on any *new* clipping past the tolerance, not on total clipping: a frame
/// that arrived with a blown window keeps its blown window, and the guard's job is to stop the
/// grade making it worse. Section 10.1's gate is "no frame gains new clipping beyond its
/// scene tolerance", and the word is *gains*.
pub const ATTRIBUTION_EPSILON: f32 = 1e-4;

/// The clipping a grade leaves.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Clipping {
    /// The fraction of the frame at or above [`CLIP_LEVEL`].
    pub highlight: f32,
    /// The fraction at or below [`CRUSH_LEVEL`].
    pub shadow: f32,
}

/// What the guards concluded.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// The grade that passed.
    pub grade: Grade,
    /// The clipping it leaves.
    pub after: Clipping,
    /// The clipping the frame arrived with.
    pub before: Clipping,
    /// The reasons, in the order they were made.
    pub reasons: Vec<Explained>,
    /// How many times a parameter was reduced.
    pub resolves: u8,
    /// True when the subtlety cap scaled the whole grade back.
    pub capped: bool,
}

/// Measure what a grade does to a frame's clipping.
///
/// The histogram is phase 15's - one decode, one set of statistics - and the replay uses the
/// renderer's own `tone`, `contrast` and `apply_curve`, so there is no second answer to what
/// a parameter does.
#[must_use]
pub fn clipping_of(histogram: &[u32; 256], counted: u32, grade: &Grade) -> Clipping {
    if counted == 0 {
        return Clipping::default();
    }
    let lut = CurveLut::build(&Curve {
        points: grade.curve.recipe_points(),
    });
    let tone = Tone {
        highlights: grade.tone.highlights / 100.0,
        shadows: grade.tone.shadows / 100.0,
        whites: grade.tone.whites / 100.0,
        blacks: grade.tone.blacks / 100.0,
    };
    let contrast_amount = grade.tone.contrast / 100.0;

    let mut clipped = 0u64;
    let mut crushed = 0u64;
    for (bin, hits) in histogram.iter().enumerate() {
        if *hits == 0 {
            continue;
        }
        // A neutral pixel at the bin's luminance. The three stages replayed here are
        // functions of luminance alone, so a grey sample carries the same answer as any
        // colour at the same luminance would.
        let level = bin as f32 / 255.0;
        let mut value = [level, level, level];
        value = tonemap::tone(value, tone);
        value = tonemap::contrast(value, contrast_amount);
        value = tonemap::apply_curve(value, &lut);
        let out = value.first().copied().unwrap_or(0.0);
        if out >= CLIP_LEVEL {
            clipped += u64::from(*hits);
        } else if out <= CRUSH_LEVEL {
            crushed += u64::from(*hits);
        }
    }
    Clipping {
        highlight: clipped as f32 / counted as f32,
        shadow: crushed as f32 / counted as f32,
    }
}

/// The subtlety metric: the root-mean-square of every normalised parameter this phase sets.
///
/// Five terms - the tone parameters, the curve's largest excursion, the HSL block, the
/// vibrance and the saturation - so a grade that is moderate in all five scores lower than one
/// that is extreme in one, which is what "professionally subtle" means when it is written
/// down. Section 12's first failure mode is the thing this number exists to make measurable.
#[must_use]
pub fn subtlety(grade: &Grade) -> f32 {
    let terms = [
        grade.tone.magnitude(),
        grade.curve.magnitude(),
        grade.colour.hsl.magnitude(),
        (grade.colour.vibrance / 100.0).abs(),
        (grade.colour.saturation / 100.0).abs(),
    ];
    let squares: f32 = terms.iter().map(|term| term * term).sum();
    (squares / terms.len() as f32).sqrt()
}

/// Apply the subtlety cap and then the clipping guard.
///
/// `before` is the frame's own clipping with no grade at all, measured by the caller from the
/// same histogram, so the comparison the gate makes is `after - before` rather than `after`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn enforce(
    grade: &Grade,
    histogram: &[u32; 256],
    counted: u32,
    tolerance: f32,
    subtlety_cap: f32,
) -> Resolved {
    let mut reasons: Vec<Explained> = Vec::new();
    let identity = Grade {
        tone: ToneParams::default(),
        curve: ToneCurve::identity(),
        colour: HslSolved::default(),
    };
    let before = clipping_of(histogram, counted, &identity);

    // -- the subtlety cap ------------------------------------------------
    let measured = subtlety(grade);
    let mut candidate = grade.clone();
    let capped = measured > subtlety_cap && subtlety_cap > 0.0;
    if capped {
        let factor = (subtlety_cap / measured).clamp(0.0, 1.0);
        candidate = Grade {
            tone: candidate.tone.scaled(factor),
            curve: curve::scaled(&candidate.curve, factor),
            colour: candidate.colour.attenuated(factor),
        };
        reasons.push(codec::numbered(
            ColourCode::SubtletyCapped,
            format!(
                "the whole grade was scaled back to {:.0}% of what was worked out, because \
                 everything together would have looked processed",
                factor * 100.0
            ),
            -0.05,
            vec![factor, measured, subtlety_cap],
        ));
    }

    // -- the clipping guard ----------------------------------------------
    let mut after = clipping_of(histogram, counted, &candidate);
    let mut resolves = 0_u8;
    while added(after, before) > tolerance + ATTRIBUTION_EPSILON && resolves < MAX_RESOLVES {
        let Some((reduced, which)) = reduce(&candidate, histogram, counted) else {
            break;
        };
        candidate = reduced;
        resolves += 1;
        let previous = after;
        after = clipping_of(histogram, counted, &candidate);
        reasons.push(codec::numbered(
            ColourCode::ClippingGuardResolved,
            format!(
                "the {which} was reduced, because the grade would have clipped {:.1}% of this \
                 frame and this kind of photograph accepts {:.1}%",
                added(previous, before) * 100.0,
                tolerance * 100.0
            ),
            -0.04,
            vec![added(previous, before), tolerance],
        ));
    }

    // The fitted curve was the last thing reduced and it may now be the identity, in which
    // case the frame carries the withdrawal rather than an invisible curve.
    if candidate.curve.is_identity() && !grade.curve.is_identity() {
        reasons.push(codec::plain(ColourCode::CurveFlattenedForClipping, -0.04));
    }

    Resolved {
        grade: candidate,
        after,
        before,
        reasons,
        resolves,
        capped,
    }
}

/// How much clipping a grade added.
#[must_use]
pub fn added(after: Clipping, before: Clipping) -> f32 {
    (after.highlight - before.highlight).max(0.0)
}

/// Reduce whichever parameter is most responsible for the clipping, and name it.
///
/// Each candidate is tried and measured; the one that removes the most clipping wins. Returns
/// `None` when no candidate helps, which is how the loop ends on a frame whose clipping was
/// there before the grade.
fn reduce(grade: &Grade, histogram: &[u32; 256], counted: u32) -> Option<(Grade, &'static str)> {
    let mut candidates: Vec<(Grade, &'static str)> = Vec::with_capacity(3);
    if grade.tone.whites.abs() >= 0.5 {
        let mut out = grade.clone();
        out.tone.whites *= REDUCTION;
        candidates.push((out, "white point"));
    }
    if grade.tone.contrast.abs() >= 0.5 {
        let mut out = grade.clone();
        out.tone.contrast *= REDUCTION;
        candidates.push((out, "contrast"));
    }
    if !grade.curve.is_identity() {
        let mut out = grade.clone();
        out.curve = curve::scaled(&grade.curve, REDUCTION);
        candidates.push((out, "tone curve"));
    }

    if candidates.is_empty() {
        return None;
    }
    let current = clipping_of(histogram, counted, grade).highlight;
    let mut best: Option<(f32, Grade, &'static str)> = None;
    for (candidate, name) in &candidates {
        let after = clipping_of(histogram, counted, candidate).highlight;
        let gained = current - after;
        if gained <= 0.0 {
            continue;
        }
        if best.as_ref().is_none_or(|(top, _, _)| gained > *top) {
            best = Some((gained, candidate.clone(), name));
        }
    }
    // Nothing helped *this* step, which is not the same as nothing helping. A frame whose
    // bright region is one histogram bin wide flips all of it at once: a single reduction can
    // move the clipping by exactly zero and the next one by sixty percent. Falling back to the
    // largest candidate keeps the loop making progress; the loop itself still stops as soon as
    // the frame is inside its tolerance, and `MAX_RESOLVES` bounds the futile case.
    best.map(|(_, candidate, name)| (candidate, name))
        .or_else(|| {
            candidates
                .into_iter()
                .max_by(|left, right| magnitude(&left.0).total_cmp(&magnitude(&right.0)))
        })
}

/// How much of a grade is left, for the fallback ordering.
fn magnitude(grade: &Grade) -> f32 {
    grade.tone.whites.abs() + grade.tone.contrast.abs() + grade.curve.magnitude() * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::colour::{CurvePoint, HslAdjustments, HslBand, HslShift};

    fn histogram_of(levels: &[(usize, u32)]) -> ([u32; 256], u32) {
        let mut histogram = [0u32; 256];
        let mut counted = 0;
        for (bin, hits) in levels {
            if let Some(slot) = histogram.get_mut(*bin) {
                *slot += hits;
            }
            counted += hits;
        }
        (histogram, counted)
    }

    fn grade(contrast: f32, whites: f32) -> Grade {
        Grade {
            tone: ToneParams {
                contrast,
                whites,
                ..ToneParams::default()
            },
            curve: ToneCurve::identity(),
            colour: HslSolved::default(),
        }
    }

    #[test]
    fn a_grade_that_would_blow_a_bright_frame_is_reduced_and_says_which_parameter() {
        // Half the frame just under white, and a grade that lifts it over. The guard has to
        // pull it back and name the parameter it pulled.
        let (histogram, counted) = histogram_of(&[(40, 500), (248, 500)]);
        let resolved = enforce(&grade(40.0, 40.0), &histogram, counted, 0.004, 1.0);
        assert!(resolved.resolves > 0, "{resolved:?}");
        assert!(resolved
            .reasons
            .iter()
            .any(|(reason, _)| reason.code == ColourCode::ClippingGuardResolved));
        assert!(resolved.grade.tone.whites.abs() < 40.0);
    }

    #[test]
    fn a_frame_that_arrived_clipped_keeps_its_clipping_and_is_not_re_solved_for_it() {
        // Section 10.1's gate is about clipping a frame GAINS. A sparkler that was blown
        // before the grade is not the grade's fault, and re-solving for it would darken
        // every face in the frame.
        let (histogram, counted) = histogram_of(&[(255, 300), (100, 700)]);
        let resolved = enforce(&grade(6.0, 0.0), &histogram, counted, 0.004, 1.0);
        assert_eq!(resolved.resolves, 0, "{resolved:?}");
        assert!(resolved.before.highlight > 0.25);
    }

    #[test]
    fn the_subtlety_cap_scales_everything_including_the_curve() {
        let mut hsl = HslAdjustments::default();
        hsl.set(
            HslBand::Green,
            HslShift {
                h: 40.0,
                s: -40.0,
                l: 0.0,
            },
        );
        let strong = Grade {
            tone: ToneParams {
                contrast: 48.0,
                highlights: -50.0,
                shadows: 50.0,
                whites: 30.0,
                blacks: -30.0,
            },
            curve: ToneCurve::new(vec![
                CurvePoint::new(0, 0),
                CurvePoint::new(64, 30),
                CurvePoint::new(192, 220),
                CurvePoint::new(255, 255),
            ])
            .unwrap_or_else(|_| unreachable!("those four points are monotone")),
            colour: HslSolved {
                hsl,
                vibrance: 40.0,
                saturation: 20.0,
                reasons: Vec::new(),
            },
        };
        let (histogram, counted) = histogram_of(&[(120, 1000)]);
        let resolved = enforce(&strong, &histogram, counted, 0.05, 0.20);
        assert!(resolved.capped);
        assert!(subtlety(&resolved.grade) <= 0.205, "{:?}", resolved.grade);
        assert!(resolved.grade.curve.magnitude() < strong.curve.magnitude());
        assert!(resolved.grade.colour.vibrance.abs() < 40.0);
    }

    #[test]
    fn a_subtle_grade_is_left_exactly_alone() {
        let gentle = grade(8.0, 2.0);
        let (histogram, counted) = histogram_of(&[(120, 1000)]);
        let resolved = enforce(&gentle, &histogram, counted, 0.05, 0.30);
        assert!(!resolved.capped);
        assert_eq!(resolved.resolves, 0);
        assert_eq!(resolved.grade.tone, gentle.tone);
    }

    #[test]
    fn the_guards_only_ever_reduce() {
        // The property that makes the order of the three guards irrelevant, and that lets the
        // skin guard run last and measure what will actually be stored.
        for contrast in [-40.0_f32, 0.0, 40.0] {
            for whites in [-30.0_f32, 0.0, 30.0] {
                let input = grade(contrast, whites);
                let (histogram, counted) = histogram_of(&[(20, 400), (200, 300), (250, 300)]);
                let resolved = enforce(&input, &histogram, counted, 0.004, 0.25);
                assert!(resolved.grade.tone.contrast.abs() <= input.tone.contrast.abs() + 1e-3);
                assert!(resolved.grade.tone.whites.abs() <= input.tone.whites.abs() + 1e-3);
            }
        }
    }

    #[test]
    fn the_subtlety_metric_prefers_five_small_moves_to_one_huge_one() {
        let spread = Grade {
            tone: ToneParams {
                contrast: 20.0,
                highlights: -20.0,
                shadows: 20.0,
                whites: 20.0,
                blacks: -20.0,
            },
            curve: ToneCurve::identity(),
            colour: HslSolved::default(),
        };
        let extreme = Grade {
            tone: ToneParams {
                contrast: 100.0,
                ..ToneParams::default()
            },
            curve: ToneCurve::identity(),
            colour: HslSolved::default(),
        };
        assert!(subtlety(&spread) < subtlety(&extreme));
    }
}
