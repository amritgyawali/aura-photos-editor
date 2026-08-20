//! The five tone parameters: contrast, highlights, shadows, whites and blacks.
//!
//! PHASE-16 section 6.1 asks for "a small MLP over features: histogram percentiles, subject
//! luminance, scene class, dynamic range, flash flag, noise level". The head exists, it is
//! registered, it is signed, and **it is never consulted** - [`TONE_HEAD_TRAINED`] is false
//! for the reason every model since phase 05 has been untrained: there is no corpus of RAW
//! files paired with expert edits in this repository, and section 8's step 1 is a DATA task
//! that needs a wedding archive nobody here has.
//!
//! What ships is the solver below. ADR-0033 decision 5 records why that is a decision rather
//! than a fallback: a random projection of a histogram blended at any weight is a random
//! contribution at that weight, and it would be indistinguishable in the panel from a
//! learned one. The solver is what section 10.1's gates measure and it is the thing the head
//! will one day have to beat.
//!
//! ## What each parameter is solved from
//!
//! | Parameter | Solved from | Bounded by |
//! |---|---|---|
//! | `blacks` | where the darkest real tone sits against the scene's black point | how much is already crushed |
//! | `whites` | where the brightest real tone sits | the scene's clipping tolerance |
//! | `highlights` | how much is clipped, and how bright the top decile is | the scene's own recovery intent |
//! | `shadows` | the scene's lift intent, scaled by how deep the shadows actually are | **phase 09's noise estimate** |
//! | `contrast` | the gap between the subject's measured spread and the scene's intent | the subtlety cap |
//!
//! The noise bound is the one that is not negotiable. `IntegrityService` owns what a lift
//! costs in noise and this module *takes* the answer rather than forming one: a second
//! opinion on how far a body can be pushed is two exposure decisions that disagree, which is
//! the rule phase 15 wrote for the same number.

use aura_core::contract::colour::ColourCode;

use crate::colour::codec::{self, Explained};
use crate::colour::intent::SceneIntent;

/// Whether the pinned tone weights have passed the section 10.1 gate.
///
/// A compile-time release assertion rather than an inference-success check, exactly as phase
/// 11's `KEYPOINT_HEAD_TRAINED` and phase 15's `WB_HEAD_TRAINED` are. The registered fixture
/// is a valid ONNX graph and executes successfully; executing successfully does not make its
/// output an estimate of anything. While this is false the head is never run at all.
pub const TONE_HEAD_TRAINED: bool = false;

/// The largest contrast this phase will set, in recipe units.
///
/// Fifty. Beyond it the grade stops being a tone decision and becomes a look, and phase 17 is
/// where a look belongs.
pub const MAX_CONTRAST: f32 = 50.0;

/// The largest highlight or shadow move this phase will set.
pub const MAX_RECOVERY: f32 = 60.0;

/// The largest white or black point move this phase will set.
///
/// Forty, and lower than the recovery bound on purpose: whites and blacks are *endpoints*,
/// and an endpoint moved forty units has already redefined what the photograph's darkest
/// tone is.
pub const MAX_ENDPOINT: f32 = 40.0;

/// How many units of shadow lift one stop of phase 09's measured headroom buys.
///
/// Twelve. A body with two stops of headroom at this ISO can carry a lift of about
/// twenty-four units before the shadows start showing the noise the sensor recorded, which
/// is roughly where a photographer stops by hand. The constant is the only place the
/// conversion happens, so changing it changes every scene at once and bumps `ANALYSIS_VER`.
pub const LIFT_PER_HEADROOM_EV: f32 = 12.0;

/// Where a scene wants its darkest real tone to sit, in perceptual units.
///
/// Two percent rather than zero. A photograph whose first percentile is at absolute black
/// has lost shadow detail that no later phase can recover, and a photograph whose first
/// percentile is at ten percent looks washed out. Two is where a print's black point lands.
pub const BLACK_POINT_TARGET: f32 = 0.02;

/// Where a scene wants its brightest real tone to sit.
///
/// Ninety-six percent, not a hundred: the last four percent is the headroom a specular
/// highlight needs, and a white point placed at 1.0 turns every reflection into a flat disc.
pub const WHITE_POINT_TARGET: f32 = 0.96;

/// The five parameters, in the recipe's own units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ToneParams {
    /// Contrast.
    pub contrast: f32,
    /// Highlight recovery. Negative pulls highlights down.
    pub highlights: f32,
    /// Shadow lift. Positive opens shadows.
    pub shadows: f32,
    /// White point.
    pub whites: f32,
    /// Black point.
    pub blacks: f32,
}

impl ToneParams {
    /// Every parameter clamped into this module's own bounds.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            contrast: self.contrast.clamp(-MAX_CONTRAST, MAX_CONTRAST),
            highlights: self.highlights.clamp(-MAX_RECOVERY, MAX_RECOVERY),
            shadows: self.shadows.clamp(-MAX_RECOVERY, MAX_RECOVERY),
            whites: self.whites.clamp(-MAX_ENDPOINT, MAX_ENDPOINT),
            blacks: self.blacks.clamp(-MAX_ENDPOINT, MAX_ENDPOINT),
        }
    }

    /// Every parameter scaled toward zero.
    #[must_use]
    pub fn scaled(self, factor: f32) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        Self {
            contrast: self.contrast * factor,
            highlights: self.highlights * factor,
            shadows: self.shadows * factor,
            whites: self.whites * factor,
            blacks: self.blacks * factor,
        }
    }

    /// The root-mean-square magnitude, normalised to `0..1`.
    ///
    /// The tone half of the subtlety metric. Root-mean-square rather than a sum, so a frame
    /// that needed one large correction is not scored the same as a frame that has been
    /// pushed in five directions at once - the second is what "over-graded" looks like and
    /// the first is what "fixed" looks like.
    #[must_use]
    pub fn magnitude(self) -> f32 {
        let squares = self.contrast.powi(2)
            + self.highlights.powi(2)
            + self.shadows.powi(2)
            + self.whites.powi(2)
            + self.blacks.powi(2);
        (squares / 5.0).sqrt() / 100.0
    }
}

/// What one frame's histogram and subject say, in the units the solver reasons in.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Measurements {
    /// First percentile of the perceptual luminance.
    pub p01: f32,
    /// Fifth percentile.
    pub p05: f32,
    /// Median.
    pub p50: f32,
    /// Ninety-fifth percentile.
    pub p95: f32,
    /// Ninety-ninth percentile.
    pub p99: f32,
    /// The interquartile spread of the subject region's luminance.
    ///
    /// Interquartile rather than a standard deviation, because a face with a specular
    /// highlight on the nose has a long tail and a standard deviation reads it as contrast
    /// that is not there.
    pub subject_spread: f32,
    /// The subject region's median luminance.
    pub subject_luma: f32,
    /// True when a face defined the subject region.
    pub subject_is_face: bool,
    /// The fraction of the frame already clipped at the top.
    pub clipped: f32,
    /// The fraction already crushed at the bottom.
    pub crushed: f32,
    /// Phase 09's measured shadow headroom for this body at this ISO, in stops.
    pub shadow_headroom_ev: f32,
}

impl Measurements {
    /// Percentiles from a 256-bin luminance histogram.
    ///
    /// The histogram is phase 15's - `tone::stats::FrameStats::luma_histogram` - because it
    /// is already computed on the one decode this crate makes of the proxy. A second pass to
    /// build a second histogram would be the "recompute a descriptor" mistake phase 05's
    /// fifth rule names.
    #[must_use]
    pub fn from_histogram(histogram: &[u32; 256], counted: u32) -> Self {
        let percentile = |fraction: f32| -> f32 {
            if counted == 0 {
                return 0.0;
            }
            let target = (counted as f32 * fraction.clamp(0.0, 1.0)) as u32;
            let mut seen = 0u32;
            for (bin, hits) in histogram.iter().enumerate() {
                seen += *hits;
                if seen > target {
                    return bin as f32 / 255.0;
                }
            }
            1.0
        };
        Self {
            p01: percentile(0.01),
            p05: percentile(0.05),
            p50: percentile(0.50),
            p95: percentile(0.95),
            p99: percentile(0.99),
            ..Self::default()
        }
    }

    /// The frame's dynamic range in perceptual units, `0..1`.
    #[must_use]
    pub fn range(&self) -> f32 {
        (self.p99 - self.p01).clamp(0.0, 1.0)
    }
}

/// What the solver decided, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Solved {
    /// The five parameters.
    pub params: ToneParams,
    /// The reasons, in the order they were made.
    pub reasons: Vec<Explained>,
    /// True when the noise budget bound the shadow lift.
    pub noise_capped: bool,
    /// The shadow lift the scene asked for, before the noise budget.
    ///
    /// Kept so the panel and the gate can say *how much* was withheld rather than only that
    /// something was.
    pub requested_lift: f32,
}

/// Solve the five parameters for one frame.
///
/// Deterministic, total and side-effect free: the same measurements and the same intent give
/// the same five numbers, which is invariant 4 for this module.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn solve(measured: &Measurements, intent: &SceneIntent) -> Solved {
    let mut reasons: Vec<Explained> = Vec::new();

    // -- blacks ----------------------------------------------------------
    //
    // Where the darkest real tone sits against the scene's black point. A frame whose first
    // percentile is well above the target is flat and wants its blacks deepened; one below
    // it has already lost shadow detail and wants them lifted. The move is scaled by 200
    // because the parameter's range is ±100 and the measurement's is 0..1: half a unit of
    // black point per percentile point is what matches the renderer's own response.
    let black_error = measured.p01 - BLACK_POINT_TARGET;
    let mut blacks = -black_error * 200.0;
    if measured.crushed > 0.02 {
        // Already crushed. Deepening further destroys what is left, so the move is only ever
        // allowed upward from here - the same asymmetry phase 09's focus head has, where a
        // measurement can withdraw a claim and never make one.
        blacks = blacks.max(0.0);
    }
    let blacks = blacks.clamp(-MAX_ENDPOINT, MAX_ENDPOINT);
    if blacks.abs() >= 1.0 {
        reasons.push(codec::numbered(
            ColourCode::BlacksSet,
            format!(
                "the darkest tones sat at {:.0}% and this kind of photograph places them near \
                 {:.0}%",
                measured.p01 * 100.0,
                BLACK_POINT_TARGET * 100.0
            ),
            0.0,
            vec![measured.p01, blacks],
        ));
    }

    // -- whites ----------------------------------------------------------
    let white_error = WHITE_POINT_TARGET - measured.p99;
    let mut whites = white_error * 200.0;
    if measured.clipped > intent.clip_tolerance {
        // Already clipped past what the scene accepts. The white point may only come down.
        whites = whites.min(0.0);
    }
    let whites = whites.clamp(-MAX_ENDPOINT, MAX_ENDPOINT);
    if whites.abs() >= 1.0 {
        reasons.push(codec::numbered(
            ColourCode::WhitesSet,
            format!(
                "the brightest real tones sat at {:.0}% and this kind of photograph places them \
                 near {:.0}%",
                measured.p99 * 100.0,
                WHITE_POINT_TARGET * 100.0
            ),
            0.0,
            vec![measured.p99, whites],
        ));
    }

    // -- highlights ------------------------------------------------------
    //
    // The scene's own recovery intent, scaled by how much of the top of the histogram is
    // actually at risk. A frame with nothing above the ninety-fifth percentile has no
    // highlights to recover and gets none of the intent, which is what stops every dark
    // reception frame carrying a pointless negative highlight value.
    let at_risk = ((measured.p95 - 0.75) / 0.25).clamp(0.0, 1.0);
    let clipping_pressure = (measured.clipped / 0.02).clamp(0.0, 1.0);
    let highlights =
        (intent.highlight_recovery * at_risk.max(clipping_pressure)).clamp(-MAX_RECOVERY, 0.0);
    if highlights <= -1.0 {
        reasons.push(codec::numbered(
            ColourCode::HighlightsRecovered,
            format!(
                "{:.1}% of this frame was at or near the top of its range, so the brightest \
                 areas were pulled back",
                measured.clipped * 100.0
            ),
            0.0,
            vec![measured.clipped, highlights],
        ));
    }

    // -- shadows, and the one bound that is not negotiable ---------------
    //
    // The scene asks; phase 09's measured headroom decides. The requested lift is scaled by
    // how deep the shadows actually are - a frame whose fifth percentile is already at 40 %
    // has nothing to open - and then capped by what the sensor will carry at this ISO.
    let depth = ((0.30 - measured.p05) / 0.30).clamp(0.0, 1.0);
    let requested_lift = intent.shadow_lift * depth;
    let noise_cap = (measured.shadow_headroom_ev.max(0.0) * LIFT_PER_HEADROOM_EV).max(0.0);
    let shadows = requested_lift
        .min(noise_cap)
        .clamp(-MAX_RECOVERY, MAX_RECOVERY);
    let noise_capped = requested_lift > noise_cap + 0.5;
    if noise_capped {
        reasons.push(codec::numbered(
            ColourCode::ShadowLiftHeldForNoise,
            format!(
                "the shadows were opened {shadows:.0} rather than {requested_lift:.0}, because \
                 this camera at this ISO has about {:.1} stops of clean shadow left",
                measured.shadow_headroom_ev
            ),
            -0.06,
            vec![shadows, requested_lift, measured.shadow_headroom_ev],
        ));
    } else if shadows >= 1.0 {
        reasons.push(codec::numbered(
            ColourCode::ShadowsLifted,
            format!(
                "the darkest fifth of this frame sat at {:.0}%, so the shadows were opened",
                measured.p05 * 100.0
            ),
            0.0,
            vec![measured.p05, shadows],
        ));
    }

    // -- contrast --------------------------------------------------------
    //
    // The gap between what the subject's separation measures and what the scene wants. This
    // is the only parameter solved against a *subject* measurement rather than a frame one,
    // and it is phase 15's argument moved from brightness to contrast: a dark hall with a
    // well-lit bride is not a low-contrast photograph, and a frame-wide spread would say it
    // was.
    let spread_error = intent.subject_contrast - measured.subject_spread;
    let baseline = intent.contrast;
    let correction = spread_error * 160.0;
    let contrast = (baseline + correction).clamp(-MAX_CONTRAST, MAX_CONTRAST);
    if contrast >= 1.0 {
        reasons.push(codec::numbered(
            ColourCode::ContrastAdded,
            if measured.subject_is_face {
                format!(
                    "the faces here separated by {:.0}% and this kind of photograph wants about \
                     {:.0}%, so contrast was added",
                    measured.subject_spread * 100.0,
                    intent.subject_contrast * 100.0
                )
            } else {
                format!(
                    "this frame separated by {:.0}% and this kind of photograph wants about \
                     {:.0}%, so contrast was added",
                    measured.subject_spread * 100.0,
                    intent.subject_contrast * 100.0
                )
            },
            0.0,
            vec![measured.subject_spread, intent.subject_contrast, contrast],
        ));
    } else if contrast <= -1.0 {
        reasons.push(codec::numbered(
            ColourCode::ContrastReduced,
            format!(
                "the light here was already harder than this kind of photograph carries - a \
                 {:.0}% separation against a {:.0}% intent",
                measured.subject_spread * 100.0,
                intent.subject_contrast * 100.0
            ),
            0.0,
            vec![measured.subject_spread, intent.subject_contrast, contrast],
        ));
    }

    let params = ToneParams {
        contrast,
        highlights,
        shadows,
        whites,
        blacks,
    }
    .clamped();

    // Invariant 2 and migration 16's `reason_count` CHECK. A frame that needed nothing still
    // has to say so, and "this was already right" is a real answer rather than an empty one.
    if reasons.is_empty() {
        reasons.push(codec::plain(ColourCode::ToneAlreadyRight, 0.0));
    }

    Solved {
        params,
        reasons,
        noise_capped,
        requested_lift,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::intent::IntentTable;
    use aura_core::SceneId;

    fn intent_for(scene: SceneId) -> SceneIntent {
        IntentTable::embedded().map_or_else(
            |_| unreachable!("the shipped intents are embedded in the binary"),
            |table| table.get(scene),
        )
    }

    fn flat_frame() -> Measurements {
        Measurements {
            p01: 0.18,
            p05: 0.24,
            p50: 0.45,
            p95: 0.72,
            p99: 0.78,
            subject_spread: 0.14,
            subject_luma: 0.45,
            subject_is_face: true,
            clipped: 0.0,
            crushed: 0.0,
            shadow_headroom_ev: 2.0,
        }
    }

    #[test]
    fn a_flat_frame_gains_contrast_and_a_black_point() {
        let solved = solve(&flat_frame(), &intent_for(SceneId::CouplePortrait));
        assert!(solved.params.contrast > 10.0, "{:?}", solved.params);
        assert!(solved.params.blacks < -5.0, "{:?}", solved.params);
        assert!(solved.params.whites > 5.0, "{:?}", solved.params);
    }

    #[test]
    fn a_frame_that_is_already_right_says_so_and_moves_almost_nothing() {
        let intent = intent_for(SceneId::Ceremony);
        let measured = Measurements {
            p01: BLACK_POINT_TARGET,
            p05: 0.32,
            p50: 0.48,
            p95: 0.70,
            p99: WHITE_POINT_TARGET,
            subject_spread: intent.subject_contrast,
            subject_luma: 0.48,
            subject_is_face: true,
            clipped: 0.0,
            crushed: 0.0,
            shadow_headroom_ev: 2.0,
        };
        let solved = solve(&measured, &intent);
        assert!(solved.params.magnitude() < 0.14, "{:?}", solved.params);
    }

    #[test]
    fn the_noise_budget_caps_the_lift_and_names_itself() {
        // The bound that is not negotiable. A speeches frame asks for the largest lift in the
        // file; a body with a tenth of a stop of headroom gets almost none of it.
        let mut measured = flat_frame();
        measured.p05 = 0.05;
        measured.shadow_headroom_ev = 0.1;
        let solved = solve(&measured, &intent_for(SceneId::Speeches));
        assert!(solved.noise_capped);
        assert!(solved.params.shadows <= 1.3, "{:?}", solved.params);
        assert!(solved
            .reasons
            .iter()
            .any(|(reason, _)| reason.code == ColourCode::ShadowLiftHeldForNoise));
    }

    #[test]
    fn an_already_crushed_frame_is_never_crushed_further() {
        // The asymmetry that stops the solver destroying what it cannot recover.
        let mut measured = flat_frame();
        measured.p01 = 0.30;
        measured.crushed = 0.10;
        let solved = solve(&measured, &intent_for(SceneId::DanceFloor));
        assert!(solved.params.blacks >= 0.0, "{:?}", solved.params);
    }

    #[test]
    fn an_already_clipped_frame_never_gains_a_higher_white_point() {
        let mut measured = flat_frame();
        measured.p99 = 0.5;
        measured.clipped = 0.10;
        let solved = solve(&measured, &intent_for(SceneId::FamilyPortrait));
        assert!(solved.params.whites <= 0.0, "{:?}", solved.params);
    }

    #[test]
    fn a_dark_reception_does_not_get_a_highlight_recovery_it_has_no_use_for() {
        let mut measured = flat_frame();
        measured.p95 = 0.30;
        measured.p99 = 0.40;
        measured.clipped = 0.0;
        let solved = solve(&measured, &intent_for(SceneId::Speeches));
        assert!(solved.params.highlights.abs() < 1.0, "{:?}", solved.params);
    }

    #[test]
    fn the_head_is_never_consulted_in_this_build() {
        // A constant assertion on purpose: the day somebody flips `TONE_HEAD_TRAINED` this
        // test fails and points at the exit report's condition C1, which has to close with it.
        assert_eq!(
            usize::from(TONE_HEAD_TRAINED),
            0,
            "if this head is ever trained, the exit report's condition C1 has to close with it"
        );
    }
}
