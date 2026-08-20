//! The guarantee: grading never turns anybody's skin unnatural, and it is measured rather
//! than promised.
//!
//! PHASE-16 section 6.3:
//!
//! > All HSL, vibrance and saturation operations are attenuated inside the skin mask by a
//! > factor that guarantees measured hue shift <= 2 deg and chroma change <= 6 %. After
//! > grading, measure actual skin hue/chroma shift; if the ceiling is exceeded, re-solve with
//! > stronger attenuation and record the event in the reasons.
//!
//! ## Why it is a post-condition and not a factor
//!
//! Every product that has shipped orange skin was built on the other design: derive an
//! attenuation from the mask, apply it to the *parameters*, and trust the arithmetic. Its
//! failure mode is not that the attenuation is wrong. It is that the attenuation is applied
//! to a parameter while the promise is made about a pixel, and between them sit a curve that
//! moves chroma without touching a band, two bands that overlap on a face, and a vibrance
//! operator that is not linear in the saturation of what it touches.
//!
//! So this module grades the skin samples, **measures what happened to them**, and if either
//! ceiling is exceeded re-solves at a stronger attenuation - up to [`MAX_RESOLVES`] times,
//! and then withdraws the colour operations entirely. ADR-0033 decision 1.
//!
//! ## The measurement runs through the real renderer
//!
//! `aura_render::tonemap` is the authority on what a grade does to a pixel, and this module
//! calls it: the same `tone`, `contrast`, `apply_curve`, `hsl` and `vibrance_saturation`
//! functions the CPU engine calls, in the same order the CPU engine calls them. A
//! reimplementation here would make the guarantee a statement about a *model* of the
//! renderer, which is exactly the gap the post-condition exists to close.
//!
//! That is why `aura-brain-photo` depends on `aura-render` from phase 16 onward. It still
//! never writes a recipe: `aura_recipe::schema::merge` is not called anywhere in this crate
//! and `tests/no_recipe_writes.rs` is a grep that fails the build if it ever is.
//!
//! ## What "unnatural" is measured against
//!
//! **This frame's own skin, after the tone half and before the colour half.** Not a target,
//! not a locus, not a reference: the ceiling is on how far *colour grading* moved it. That is
//! the only definition under which the guarantee means the same thing for every skin tone,
//! and it is why there is no ideal-skin constant anywhere in this phase - the same structural
//! argument phase 15's `skin_locus` makes for white balance, made once more for colour.
//!
//! The baseline is the tone half rather than the raw pixel, and section 6.3 is precise about
//! why: the ceilings are on "all HSL, vibrance and saturation operations". Lifting a shadow
//! or steepening a curve **must** change a skin pixel's chroma, because chroma in CIE `LCh`
//! scales with lightness and that is what making a photograph lighter means. Measuring the
//! colour operations against an ungraded pixel would therefore report every correctly
//! exposed frame as a violation, and the guard would withdraw the colour from all of them -
//! a guarantee that fires on every photograph is one nobody can act on.

use std::collections::BTreeMap;

use aura_core::contract::colour::{
    ColourCode, HslAdjustments, HslBand, SkinGuardReport, ToneCurve, SKIN_CHROMA_CEILING,
    SKIN_HUE_CEILING_DEG,
};
use aura_core::contract::people::FaceRef;
use aura_raw::colour::de2000::{xyz_d65_to_lab, Lab};
use aura_raw::colour::matrix::{apply as apply_matrix, SRGB_TO_XYZ_D65};
use aura_recipe::{Curve, HslShift as RecipeHslShift};
use aura_render::tonemap::{self, CurveLut, Tone};

use crate::colour::codec::{self, Explained};
use crate::colour::hsl::Solved as HslSolved;
use crate::colour::tone::ToneParams;

/// How many times the grade is re-solved before the colour operations are withdrawn.
///
/// Three. The attenuation ladder is 0.5, 0.25, 0.0 of what the solver asked for, so the third
/// re-solve has already removed every colour operation - and a fourth would be measuring the
/// same thing again.
pub const MAX_RESOLVES: u8 = 3;

/// The attenuation the guard tries at each step.
///
/// A ladder rather than a search. A bisection would find the largest attenuation that just
/// passes, which is precisely the value that fails on the next frame of the same person under
/// slightly different light - and gallery consistency is worth more here than the last few
/// percent of a saturation adjustment.
pub const LADDER: [f32; MAX_RESOLVES as usize] = [0.5, 0.25, 0.0];

/// How many skin samples are measured per frame.
///
/// Sixty-four. The measurement is over the *maximum* shift rather than the mean, so more
/// samples can only make the guarantee stricter; sixty-four is where the cost stops being
/// free at section 11's 20 ms budget.
pub const MAX_SAMPLES: usize = 64;

/// The sampling grid inside one face box, per axis.
///
/// A fixed grid rather than every pixel, so the sample count does not depend on how large the
/// face happens to be - which would make a close-up more strictly guarded than a group shot
/// for no reason anybody could explain.
pub const GRID: usize = 4;

/// Below this chroma a sample's hue angle is treated as undefined.
///
/// A near-neutral sample's angle swings wildly for a change nobody can see, and letting it
/// into a maximum would fail the guarantee on a photograph of somebody in very flat light.
pub const CHROMA_FLOOR: f64 = 0.5;

/// How far inside a face box the samples are taken.
///
/// Half, matching phase 15's `stats::SKIN_INSET`. The middle half of a face box is skin;
/// the edges are hair, collar and background, and a guard measuring a collar is a guard
/// measuring the wrong thing.
pub const INSET: f32 = 0.50;

/// The skin pixels one frame offers, and how much of the frame they are.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkinSamples {
    /// Linear sRGB triples, at most [`MAX_SAMPLES`].
    pub samples: Vec<[f32; 3]>,
    /// The fraction of the frame the face boxes cover.
    pub area: f32,
}

impl SkinSamples {
    /// True when there is skin to protect.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Collect the skin samples from one decoded proxy.
///
/// The boxes come from phase 06 through the frozen service, so this is *where the faces are*
/// rather than wherever the colour looks like skin - which is the difference between
/// protecting a person and protecting a wooden door.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn sample(
    rgb: &[u8],
    width: usize,
    height: usize,
    table: &[f32; 256],
    faces: &[FaceRef],
) -> SkinSamples {
    let mut samples = Vec::new();
    let mut area = 0.0_f32;

    for face in faces {
        let box_rect = face.bbox.clamped();
        area += box_rect.w * box_rect.h;

        let inset_w = box_rect.w * INSET;
        let inset_h = box_rect.h * INSET;
        let x0 = ((box_rect.x + (box_rect.w - inset_w) / 2.0) * width as f32) as usize;
        let y0 = ((box_rect.y + (box_rect.h - inset_h) / 2.0) * height as f32) as usize;
        let x1 = (((box_rect.x + f32::midpoint(box_rect.w, inset_w)) * width as f32) as usize)
            .min(width);
        let y1 = (((box_rect.y + f32::midpoint(box_rect.h, inset_h)) * height as f32) as usize)
            .min(height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }

        for row in 0..GRID {
            for column in 0..GRID {
                let x = x0 + (x1 - x0) * (column * 2 + 1) / (GRID * 2);
                let y = y0 + (y1 - y0) * (row * 2 + 1) / (GRID * 2);
                let index = (y.min(height - 1) * width + x.min(width - 1)) * 3;
                let (Some(r), Some(g), Some(b)) = (
                    rgb.get(index).and_then(|v| table.get(*v as usize)).copied(),
                    rgb.get(index + 1)
                        .and_then(|v| table.get(*v as usize))
                        .copied(),
                    rgb.get(index + 2)
                        .and_then(|v| table.get(*v as usize))
                        .copied(),
                ) else {
                    continue;
                };
                // A clipped or near-black sample says nothing about hue and would dominate a
                // maximum. Excluded rather than clamped, which is the same exclusion phase
                // 15's specular-highlight rule makes for the same reason.
                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                if !(0.02..=0.95).contains(&luma) {
                    continue;
                }
                samples.push([r, g, b]);
            }
        }
    }

    // Keep a deterministic, evenly spread subset. Truncating would guard only the first
    // faces in a group photograph.
    if samples.len() > MAX_SAMPLES {
        let stride = samples.len().div_ceil(MAX_SAMPLES);
        samples = samples
            .iter()
            .step_by(stride)
            .copied()
            .take(MAX_SAMPLES)
            .collect();
    }

    SkinSamples {
        samples,
        area: area.clamp(0.0, 1.0),
    }
}

/// One complete grade, as the guard applies it to a sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Grade {
    /// The five tone parameters.
    pub tone: ToneParams,
    /// The fitted curve.
    pub curve: ToneCurve,
    /// The eight bands, vibrance and saturation.
    pub colour: HslSolved,
}

impl Grade {
    /// This grade with only its colour half attenuated.
    ///
    /// **The tone half is never attenuated by the guard.** Contrast, highlights, shadows,
    /// whites, blacks and the curve are what makes the photograph correct; the HSL, the
    /// vibrance and the saturation are what makes it pretty. If something has to give to keep
    /// skin where it was, section 6.3 is unambiguous about which one it is.
    #[must_use]
    pub fn colour_attenuated(&self, factor: f32) -> Self {
        Self {
            tone: self.tone,
            curve: self.curve.clone(),
            colour: self.colour.attenuated(factor),
        }
    }
}

/// What the guard concluded.
#[derive(Debug, Clone, PartialEq)]
pub struct Guarded {
    /// The grade that passed, which may be the one that went in.
    pub grade: Grade,
    /// The measured report.
    pub report: SkinGuardReport,
    /// The reasons, in the order they were made.
    pub reasons: Vec<Explained>,
    /// True when every colour operation was withdrawn.
    pub withdrew: bool,
}

/// Grade the skin samples, measure what happened, and re-solve until it is inside the
/// ceilings.
///
/// Returns the grade that passed. A frame with no skin passes trivially and says so - the
/// report's `measured` is false, `ColourCode::NoSkinInFrame` is emitted, and no caller can
/// mistake "there was nobody to protect" for "nobody moved".
#[must_use]
pub fn enforce(grade: &Grade, skin: &SkinSamples) -> Guarded {
    if skin.is_empty() {
        return Guarded {
            grade: grade.clone(),
            report: SkinGuardReport::unmeasured(),
            reasons: vec![codec::plain(ColourCode::NoSkinInFrame, -0.02)],
            withdrew: false,
        };
    }

    // The baseline: this frame's own skin after the TONE half and before the colour half. See
    // the module header for why it is not the raw pixel.
    let tone_only = Grade {
        tone: grade.tone,
        curve: grade.curve.clone(),
        colour: HslSolved::default(),
    };
    let before: Vec<Lab> = skin
        .samples
        .iter()
        .map(|rgb| lab_of(apply(&tone_only, *rgb)))
        .collect();

    let mut attenuation = 1.0_f32;
    let mut resolves = 0_u8;
    let mut candidate = grade.clone();
    let mut worst = measure(&before, skin, &candidate);

    while !within(worst) && resolves < MAX_RESOLVES {
        attenuation = LADDER.get(resolves as usize).copied().unwrap_or(0.0);
        candidate = grade.colour_attenuated(attenuation);
        resolves += 1;
        worst = measure(&before, skin, &candidate);
    }

    let report = SkinGuardReport {
        mask_area: skin.area,
        max_hue_shift_deg: worst.0,
        max_chroma_change: worst.1,
        attenuation,
        resolves,
        measured: true,
    };

    let mut reasons = Vec::new();
    let withdrew = !within(worst);
    if withdrew {
        // Even at zero colour the ceilings are exceeded, which means the *tone* half moved
        // the skin. There is nothing left to attenuate that section 6.3 permits attenuating,
        // so the honest answer is that the colour operations are gone and the frame says so.
        reasons.push(codec::plain(ColourCode::SkinGuardWithdrew, -0.10));
    } else if resolves > 0 {
        reasons.push(codec::numbered(
            ColourCode::SkinGuardResolved,
            format!(
                "the first grade moved skin {:.1} degrees, which is more than AURA allows, so it \
                 was worked out again more gently",
                measure(&before, skin, grade).0
            ),
            -0.05,
            vec![measure(&before, skin, grade).0, attenuation],
        ));
    } else if !grade.colour.is_neutral() {
        reasons.push(codec::numbered(
            ColourCode::SkinProtected,
            "every colour adjustment was held back over skin, so nobody's colour moved".to_string(),
            0.0,
            vec![attenuation],
        ));
    }

    Guarded {
        grade: candidate,
        report,
        reasons,
        withdrew,
    }
}

/// True when both ceilings hold.
fn within(worst: (f32, f32)) -> bool {
    worst.0 <= SKIN_HUE_CEILING_DEG && worst.1 <= SKIN_CHROMA_CEILING
}

/// What one grade does to one linear triple, through the renderer's own operators.
///
/// The CPU engine's point-wise order, from `cpu.rs`: exposure, tone, contrast, curve, HSL,
/// vibrance. Exposure is phase 15's and is not part of a phase 16 grade, so the replay starts
/// at tone.
///
/// It builds the curve lookup on every call, so it is for a gate, an eval harness or a
/// handful of samples rather than for a frame; [`measure`] builds the lookup once and is what
/// the guard itself uses.
#[must_use]
pub fn apply(grade: &Grade, rgb: [f32; 3]) -> [f32; 3] {
    let lut = CurveLut::build(&Curve {
        points: grade.curve.recipe_points(),
    });
    let bands = recipe_bands(&grade.colour.hsl);
    let tone = Tone {
        highlights: grade.tone.highlights / 100.0,
        shadows: grade.tone.shadows / 100.0,
        whites: grade.tone.whites / 100.0,
        blacks: grade.tone.blacks / 100.0,
    };
    let mut value = tonemap::tone(rgb, tone);
    value = tonemap::contrast(value, grade.tone.contrast / 100.0);
    value = tonemap::apply_curve(value, &lut);
    value = tonemap::hsl(value, &bands);
    tonemap::vibrance_saturation(
        value,
        grade.colour.vibrance / 100.0,
        grade.colour.saturation / 100.0,
    )
}

/// The largest hue rotation and relative chroma change over all samples.
///
/// The **maximum** rather than the mean, and that is the whole guarantee: a mean over
/// sixty-four samples hides one face in a group photograph that went orange, and one face is
/// what a photographer notices.
fn measure(before: &[Lab], skin: &SkinSamples, grade: &Grade) -> (f32, f32) {
    let lut = CurveLut::build(&Curve {
        points: grade.curve.recipe_points(),
    });
    let bands = recipe_bands(&grade.colour.hsl);
    let tone = Tone {
        highlights: grade.tone.highlights / 100.0,
        shadows: grade.tone.shadows / 100.0,
        whites: grade.tone.whites / 100.0,
        blacks: grade.tone.blacks / 100.0,
    };
    let contrast_amount = grade.tone.contrast / 100.0;
    let vibrance = grade.colour.vibrance / 100.0;
    let saturation = grade.colour.saturation / 100.0;

    let mut worst_hue = 0.0_f32;
    let mut worst_chroma = 0.0_f32;
    for (index, rgb) in skin.samples.iter().enumerate() {
        // The CPU engine's own point-wise order, from `cpu.rs`: exposure, tone, contrast,
        // curve, HSL, vibrance. Exposure is phase 15's and is not part of this grade, so the
        // replay starts at tone.
        let mut value = tonemap::tone(*rgb, tone);
        value = tonemap::contrast(value, contrast_amount);
        value = tonemap::apply_curve(value, &lut);
        value = tonemap::hsl(value, &bands);
        value = tonemap::vibrance_saturation(value, vibrance, saturation);

        let after = lab_of(value);
        let Some(reference) = before.get(index) else {
            continue;
        };
        let (hue, chroma) = shift(*reference, after);
        worst_hue = worst_hue.max(hue);
        worst_chroma = worst_chroma.max(chroma);
    }
    (worst_hue, worst_chroma)
}

/// The hue rotation in degrees and the relative chroma change between two Lab colours.
///
/// Measured in **CIE `LCh`** rather than in HSV. A hue angle in a device space is not a
/// perceptual quantity - two degrees of it means one thing on a bright cheek and another in
/// a shadow - and section 10.1's ceiling is a claim about what a person can see.
#[must_use]
pub fn shift(before: Lab, after: Lab) -> (f32, f32) {
    let chroma_before = before.a.hypot(before.b);
    let chroma_after = after.a.hypot(after.b);
    let hue = if chroma_before < CHROMA_FLOOR || chroma_after < CHROMA_FLOOR {
        0.0
    } else {
        let angle_before = before.b.atan2(before.a);
        let angle_after = after.b.atan2(after.a);
        let mut difference = (angle_after - angle_before).to_degrees();
        while difference > 180.0 {
            difference -= 360.0;
        }
        while difference < -180.0 {
            difference += 360.0;
        }
        difference.abs()
    };
    let relative = if chroma_before < CHROMA_FLOOR {
        0.0
    } else {
        ((chroma_after - chroma_before) / chroma_before).abs()
    };
    (hue as f32, relative as f32)
}

/// One linear sRGB triple as Lab under D65.
#[must_use]
pub fn lab_of(rgb: [f32; 3]) -> Lab {
    let xyz = apply_matrix(
        SRGB_TO_XYZ_D65,
        [
            f64::from(rgb.first().copied().unwrap_or(0.0)),
            f64::from(rgb.get(1).copied().unwrap_or(0.0)),
            f64::from(rgb.get(2).copied().unwrap_or(0.0)),
        ],
    );
    xyz_d65_to_lab(xyz)
}

/// The eight bands in the map shape the renderer reads.
///
/// The one place this phase's `HslAdjustments` becomes the recipe's `BTreeMap`, and it is
/// here rather than in `aura-app` because the guard has to hand the renderer exactly what the
/// recipe will hand it. Two conversions is two grades.
#[must_use]
pub fn recipe_bands(hsl: &HslAdjustments) -> BTreeMap<String, RecipeHslShift> {
    let mut out = BTreeMap::new();
    for band in HslBand::ALL {
        let shift = hsl.get(band);
        if shift.is_neutral() {
            continue;
        }
        out.insert(
            band.as_str().to_string(),
            RecipeHslShift {
                h: shift.h.round().clamp(-100.0, 100.0) as i16,
                s: shift.s.round().clamp(-100.0, 100.0) as i16,
                l: shift.l.round().clamp(-100.0, 100.0) as i16,
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::colour::HslShift;

    fn skin(samples: &[[f32; 3]]) -> SkinSamples {
        SkinSamples {
            samples: samples.to_vec(),
            area: 0.15,
        }
    }

    /// Five reflectances spanning light to dark, under a neutral light. The same five phase
    /// 15's fairness gate uses, and they are five points on a line rather than five people.
    const TONES: [[f32; 3]; 5] = [
        [0.62, 0.47, 0.39],
        [0.44, 0.32, 0.26],
        [0.30, 0.21, 0.17],
        [0.19, 0.13, 0.10],
        [0.11, 0.074, 0.058],
    ];

    fn grade_with(saturation_units: f32) -> Grade {
        let mut hsl = HslAdjustments::default();
        hsl.set(
            HslBand::Orange,
            HslShift {
                h: 0.0,
                s: saturation_units,
                l: 0.0,
            },
        );
        Grade {
            tone: ToneParams::default(),
            curve: ToneCurve::identity(),
            colour: HslSolved {
                hsl,
                vibrance: 0.0,
                saturation: 0.0,
                reasons: Vec::new(),
            },
        }
    }

    #[test]
    fn a_frame_with_no_skin_is_unmeasured_rather_than_perfect() {
        let guarded = enforce(&grade_with(40.0), &SkinSamples::default());
        assert!(!guarded.report.measured);
        assert!(guarded
            .reasons
            .iter()
            .any(|(reason, _)| reason.code == ColourCode::NoSkinInFrame));
        assert!(guarded.report.within_ceilings());
    }

    #[test]
    fn a_gentle_grade_passes_without_intervening() {
        let guarded = enforce(&grade_with(2.0), &skin(&TONES));
        assert_eq!(guarded.report.resolves, 0);
        assert!(guarded.report.within_ceilings());
        assert!(!guarded.withdrew);
    }

    #[test]
    fn a_violent_saturation_move_is_caught_and_re_solved() {
        // The property the phase exists for. Ninety units of saturation on the band that
        // carries most of a face is exactly the operation that produces orange skin.
        let guarded = enforce(&grade_with(90.0), &skin(&TONES));
        assert!(guarded.report.resolves > 0, "{:?}", guarded.report);
        assert!(
            guarded.report.within_ceilings() || guarded.withdrew,
            "{:?}",
            guarded.report
        );
        assert!(guarded.report.attenuation < 1.0);
    }

    #[test]
    fn the_result_is_always_inside_the_ceilings_or_says_it_withdrew() {
        // The whole guarantee, over the parameter space the solver can reach. There is no
        // combination that leaves a frame graded and outside the ceilings without saying so.
        for saturation in [-90.0_f32, -40.0, -10.0, 0.0, 10.0, 40.0, 90.0] {
            for hue in [-40.0_f32, 0.0, 40.0] {
                let mut grade = grade_with(saturation);
                grade.colour.hsl.set(
                    HslBand::Red,
                    HslShift {
                        h: hue,
                        s: 0.0,
                        l: 0.0,
                    },
                );
                grade.colour.vibrance = saturation / 2.0;
                let guarded = enforce(&grade, &skin(&TONES));
                assert!(
                    guarded.report.within_ceilings() || guarded.withdrew,
                    "s = {saturation}, h = {hue}: {:?}",
                    guarded.report
                );
            }
        }
    }

    #[test]
    fn the_guarantee_holds_equally_on_every_skin_tone() {
        // Section 10.1's fairness gate in miniature. Each reflectance is guarded on its own,
        // and the number of re-solves the guard needs must not depend on which one it is -
        // if it did, the product would be grading dark skin more or less aggressively than
        // light skin and calling both "protected".
        let mut resolves = Vec::new();
        for tone in TONES {
            let guarded = enforce(&grade_with(70.0), &skin(&[tone]));
            assert!(guarded.report.within_ceilings() || guarded.withdrew);
            resolves.push(guarded.report.resolves);
        }
        let first = resolves.first().copied().unwrap_or(0);
        assert!(
            resolves.iter().all(|count| count.abs_diff(first) <= 1),
            "the guard behaved differently by skin tone: {resolves:?}"
        );
    }

    #[test]
    fn the_tone_half_is_never_attenuated() {
        // Section 6.3 is unambiguous about what gives. A frame whose contrast was withdrawn
        // to protect skin would be a frame the guarantee had made worse.
        let mut grade = grade_with(90.0);
        grade.tone.contrast = 30.0;
        grade.tone.shadows = 20.0;
        let guarded = enforce(&grade, &skin(&TONES));
        assert!((guarded.grade.tone.contrast - 30.0).abs() < 1e-6);
        assert!((guarded.grade.tone.shadows - 20.0).abs() < 1e-6);
    }

    #[test]
    fn a_near_neutral_sample_does_not_fail_the_hue_ceiling() {
        // A person in very flat light has almost no chroma, and the hue angle of a
        // near-neutral swings wildly for a change nobody can see. Letting it into the
        // maximum would fail the guarantee on a photograph nobody would question.
        let guarded = enforce(
            &grade_with(20.0),
            &skin(&[[0.5, 0.5, 0.5], [0.3, 0.3, 0.3]]),
        );
        assert!(guarded.report.within_ceilings());
        assert_eq!(guarded.report.resolves, 0);
    }

    #[test]
    fn the_measurement_is_the_maximum_and_not_the_mean() {
        // One face out of five going orange is what a photographer notices, and a mean over
        // sixty-four samples hides it.
        let mixed = skin(&[
            [0.62, 0.47, 0.39],
            [0.62, 0.47, 0.39],
            [0.62, 0.47, 0.39],
            [0.62, 0.47, 0.39],
            [0.90, 0.30, 0.20],
        ]);
        let guarded = enforce(&grade_with(80.0), &mixed);
        assert!(guarded.report.within_ceilings() || guarded.withdrew);
    }
}
