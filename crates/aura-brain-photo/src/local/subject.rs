//! Making the subject visually dominant - and the rule that it is never done alone.
//!
//! PHASE-19 section 6.2, first bullet, which is the whole module in one sentence:
//!
//! > Never brighten the subject alone; always pair with a proportional background reduction
//! > so overall image luminance stays roughly constant - the eye reads the *relationship*,
//! > not absolute values.
//!
//! So this module and [`crate::local::background`] produce **one decision in two halves**,
//! and the halves are solved together in [`pair`] rather than composed afterwards. Composing
//! them afterwards is what produces a gallery that gets steadily brighter: every frame gets a
//! defensible subject lift, most of them get a defensible background reduction, and the ones
//! that do not drift up.
//!
//! ## What the subject half actually is
//!
//! Clarity, texture and a little contrast - not exposure. Section 2.1 calls it "a small
//! contrast/clarity/micro-contrast lift on the subject", and the reason it is not brightness
//! is that brightness on a subject is [`crate::local::face_light`]'s job and doing it twice
//! is how a face gets lifted a stop and a half by two operations that each thought they were
//! being conservative.

use aura_core::contract::local::{
    BackgroundBalanceDelta, SubjectEnhanceDelta, COMPETITION_CHROMA, COMPETITION_LUMA_RATIO,
    MAX_MEAN_LUMA_DRIFT,
};

use crate::local::measure::{apply_ev, RegionStats};

/// The most clarity a subject may gain, at full strength.
///
/// Twenty-two, on the `-100..=100` scale schema v1 uses. A retoucher's own local clarity on a
/// delivered wedding frame is smaller than most people expect, and clarity above about thirty
/// on skin starts to look like a sharpening halo around the jaw.
pub const MAX_CLARITY: f32 = 22.0;

/// The most texture a subject may gain, at full strength.
///
/// Lower than clarity. Texture is a high-frequency operator and skin is where high-frequency
/// operators are most obvious; phase 20 owns anything more than this, under explicit texture
/// protection.
pub const MAX_TEXTURE: f32 = 14.0;

/// The most contrast a subject may gain, at full strength.
pub const MAX_CONTRAST: f32 = 12.0;

/// The most a background may be pulled down, in stops, at full strength.
///
/// Two thirds of a stop. Past this the background stops being darker and starts being a
/// vignette, and a vignette that follows the shape of a person is the most obvious artefact
/// this phase could produce.
pub const MAX_BACKGROUND_EV: f32 = 0.67;

/// The most a background's saturation may be reduced, at full strength.
pub const MAX_BACKGROUND_SATURATION: f32 = 24.0;

/// The most a background's highlights may be reduced, at full strength.
pub const MAX_BACKGROUND_HIGHLIGHTS: f32 = 35.0;

/// What the frame said about the relationship between subject and background.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Competition {
    /// Background mean luminance over subject mean luminance.
    pub luma_ratio: f32,
    /// Mean chroma over the background.
    pub chroma_energy: f32,
    /// How many bright blobs phase 11 found behind the subject.
    pub bright_blobs: u8,
    /// The subject region.
    pub subject: RegionStats,
    /// The background region.
    pub background: RegionStats,
}

impl Competition {
    /// Measure the relationship. Nothing is decided here.
    #[must_use]
    pub fn measure(subject: RegionStats, background: RegionStats, bright_blobs: u8) -> Self {
        let luma_ratio = if subject.mean_luma <= 1e-4 {
            1.0
        } else {
            background.mean_luma / subject.mean_luma
        };
        Self {
            luma_ratio,
            chroma_energy: background.mean_chroma,
            bright_blobs,
            subject,
            background,
        }
    }

    /// True when something behind the subject is pulling the eye.
    ///
    /// Section 6.2 requires the trigger to be *measured*: "operations trigger only when a
    /// measured threshold is crossed". Three independent ways in, because a bright doorway, a
    /// saturated wall and a single hot lamp are three different problems and a single test
    /// would apply the wrong remedy to two of them.
    #[must_use]
    pub fn is_competing(&self) -> bool {
        self.luma_ratio > COMPETITION_LUMA_RATIO
            || self.chroma_energy > COMPETITION_CHROMA
            || self.bright_blobs > 0
    }

    /// How badly, `0..1`, for scaling the response.
    #[must_use]
    pub fn severity(&self) -> f32 {
        let luma = ((self.luma_ratio - COMPETITION_LUMA_RATIO) / 0.85).clamp(0.0, 1.0);
        let chroma = ((self.chroma_energy - COMPETITION_CHROMA) / 0.25).clamp(0.0, 1.0);
        let blobs = (f32::from(self.bright_blobs) / 3.0).clamp(0.0, 1.0);
        // The largest of the three rather than their sum. A frame with one very bright window
        // needs the full response; a frame with three mild problems does not need three times
        // it, and a sum would give it four.
        luma.max(chroma).max(blobs)
    }
}

/// The two halves, solved together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paired {
    /// The subject half.
    pub subject: SubjectEnhanceDelta,
    /// The background half.
    pub background: BackgroundBalanceDelta,
    /// True when the pair was scaled back to hold the frame's mean luminance.
    pub held_mean_luma: bool,
    /// True when the background's colour, rather than its brightness, was what competed.
    pub chroma_led: bool,
}

/// Solve the paired operation for one frame.
///
/// `subject_strength` and `background_strength` are the scene policy's own numbers after the
/// governor. `mask_scale` is the subject and background masks' combined quality; a poor mask
/// produces a gentle edit rather than an artefact, which is section 6.4's rule and is applied
/// here as a multiplier that can reach zero.
///
/// `frame_mean` is the frame's mean perceptual luminance before any local work. It is passed
/// in rather than measured because the same number bounds the face lighting and the shine
/// reduction, and three modules measuring it separately is three chances to measure it after
/// something has already moved.
#[must_use]
pub fn pair(
    competition: &Competition,
    subject_strength: f32,
    background_strength: f32,
    mask_scale: f32,
    frame_mean: f32,
) -> Option<Paired> {
    if !competition.is_competing() {
        return None;
    }
    let mask = mask_scale.clamp(0.0, 1.0);
    if mask <= 0.0 {
        return None;
    }
    let severity = competition.severity();
    let chroma_led = competition.chroma_energy > COMPETITION_CHROMA
        && competition.luma_ratio <= COMPETITION_LUMA_RATIO;

    let subject_scale = subject_strength.clamp(0.0, 1.0) * mask * severity;
    let background_scale = background_strength.clamp(0.0, 1.0) * mask * severity;

    // The background half. Luminance and chroma respond to their own triggers rather than to
    // the combined severity: a saturated but correctly exposed wall should be desaturated and
    // not darkened, and darkening it is how a red sari behind a couple becomes brown.
    let luma_trigger =
        competition.luma_ratio > COMPETITION_LUMA_RATIO || competition.bright_blobs > 0;
    let chroma_trigger = competition.chroma_energy > COMPETITION_CHROMA;

    let mut background_ev = if luma_trigger {
        -(MAX_BACKGROUND_EV * background_scale)
    } else {
        0.0
    };
    let mut saturation = if chroma_trigger {
        -(MAX_BACKGROUND_SATURATION * background_scale)
    } else {
        0.0
    };
    let mut highlights = if competition.background.p95_luma > 0.80 && luma_trigger {
        -(MAX_BACKGROUND_HIGHLIGHTS * background_scale)
    } else {
        0.0
    };

    // The subject half. Never exposure - see the module header.
    let mut clarity = MAX_CLARITY * subject_scale;
    let mut texture = MAX_TEXTURE * subject_scale;
    let mut contrast = MAX_CONTRAST * subject_scale;

    // The pairing itself. The frame's mean luminance is the subject's contribution plus the
    // background's; clarity, texture and contrast are ratio operations and move a *mean* by
    // very little, so what has to be held is the background's exposure.
    let mut held = false;
    let mut mean_after = mean_luma_after(competition, frame_mean, background_ev);
    if (mean_after - frame_mean).abs() > MAX_MEAN_LUMA_DRIFT {
        held = true;
        // Scale the whole pair rather than only the background: reducing the background less
        // while enhancing the subject the same amount is exactly the un-paired operation this
        // module exists to prevent.
        for _ in 0..8 {
            if (mean_after - frame_mean).abs() <= MAX_MEAN_LUMA_DRIFT {
                break;
            }
            background_ev *= 0.75;
            saturation *= 0.75;
            highlights *= 0.75;
            clarity *= 0.75;
            texture *= 0.75;
            contrast *= 0.75;
            mean_after = mean_luma_after(competition, frame_mean, background_ev);
        }
    }

    let subject = SubjectEnhanceDelta {
        clarity: clarity.round().clamp(0.0, 100.0) as i16,
        texture: texture.round().clamp(0.0, 100.0) as i16,
        contrast: contrast.round().clamp(-100.0, 100.0) as i16,
        paired_background_ev: background_ev,
        competition_ratio: competition.luma_ratio,
        mask_scale: mask,
    };
    let background = BackgroundBalanceDelta {
        exposure_ev: background_ev,
        highlights: highlights.round().clamp(-100.0, 0.0) as i16,
        saturation: saturation.round().clamp(-100.0, 0.0) as i16,
        // The background's feather is the widest in the phase. Section 6.2 asks for a guided
        // filter "so the background reduction does not trace a visible outline", and until
        // phase 18 supplies an edge-aware matte the honest substitute is a very soft edge.
        feather: 0.80,
        competition_ratio: competition.luma_ratio,
        chroma_energy: competition.chroma_energy,
        bright_blobs: competition.bright_blobs,
        mean_luma_before: frame_mean,
        mean_luma_after: mean_after,
        mask_scale: mask,
    };

    if subject.is_noop() && background.is_noop() {
        return None;
    }
    Some(Paired {
        subject,
        background,
        held_mean_luma: held,
        chroma_led,
    })
}

/// What the frame's mean perceptual luminance becomes after the background moves.
///
/// The background's own area is what weights it. A subject filling half the frame and a
/// subject filling a twentieth of it produce very different drifts from the same background
/// reduction, and a fixed weight would let the second one through and stop the first.
fn mean_luma_after(competition: &Competition, frame_mean: f32, background_ev: f32) -> f32 {
    let area = competition.background.area.clamp(0.0, 1.0);
    let before = competition.background.mean_luma;
    let after = apply_ev(before, background_ev);
    (frame_mean + (after - before) * area).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(luma: f32, chroma: f32, area: f32) -> RegionStats {
        RegionStats {
            mean_luma: luma,
            mean_chroma: chroma,
            area,
            p95_luma: (luma + 0.2).min(1.0),
        }
    }

    #[test]
    fn a_calm_background_triggers_nothing() {
        let calm = Competition::measure(region(0.50, 0.05, 0.25), region(0.45, 0.05, 0.75), 0);
        assert!(!calm.is_competing());
        assert!(pair(&calm, 1.0, 1.0, 1.0, 0.46).is_none());
    }

    #[test]
    fn a_bright_window_behind_the_subject_is_brought_down() {
        let hot = Competition::measure(region(0.40, 0.05, 0.20), region(0.80, 0.04, 0.30), 1);
        assert!(hot.is_competing());
        let paired = pair(&hot, 1.0, 1.0, 1.0, 0.52).expect("a competing frame acts");
        assert!(paired.background.exposure_ev < 0.0);
        assert!(paired.subject.clarity > 0);
    }

    #[test]
    fn the_two_halves_always_travel_together() {
        let hot = Competition::measure(region(0.40, 0.05, 0.20), region(0.80, 0.04, 0.30), 1);
        let paired = pair(&hot, 1.0, 1.0, 1.0, 0.52).expect("acts");
        assert!(
            (paired.subject.paired_background_ev - paired.background.exposure_ev).abs() < 1e-6,
            "the halves disagree about the same number"
        );
    }

    #[test]
    fn the_frames_mean_luminance_stays_within_three_per_cent() {
        // Section 10.1's own acceptance criterion, on a frame built to break it: a background
        // covering most of the photograph, far brighter than the subject.
        let hot = Competition::measure(region(0.25, 0.05, 0.08), region(0.85, 0.05, 0.92), 3);
        let paired = pair(&hot, 1.0, 1.0, 1.0, 0.80).expect("acts");
        assert!(
            paired.background.luma_drift() <= MAX_MEAN_LUMA_DRIFT + 1e-4,
            "the pairing moved the mean by {:.4}",
            paired.background.luma_drift()
        );
        assert!(paired.held_mean_luma, "and did not record that it held it");
    }

    #[test]
    fn a_saturated_but_correctly_exposed_background_is_desaturated_not_darkened() {
        let loud = Competition::measure(region(0.50, 0.06, 0.30), region(0.48, 0.35, 0.70), 0);
        assert!(loud.is_competing());
        let paired = pair(&loud, 1.0, 1.0, 1.0, 0.49).expect("acts");
        assert!(paired.chroma_led);
        assert!(paired.background.saturation < 0);
        assert_eq!(
            paired.background.exposure_ev, 0.0,
            "a red sari behind a couple must not become brown"
        );
    }

    #[test]
    fn a_hopeless_mask_produces_nothing_at_all() {
        let hot = Competition::measure(region(0.40, 0.05, 0.20), region(0.80, 0.04, 0.30), 1);
        assert!(pair(&hot, 1.0, 1.0, 0.0, 0.52).is_none());
    }

    #[test]
    fn a_weak_mask_produces_a_gentler_edit() {
        let hot = Competition::measure(region(0.40, 0.05, 0.20), region(0.80, 0.04, 0.30), 1);
        let strong = pair(&hot, 1.0, 1.0, 1.0, 0.52).expect("acts");
        let weak = pair(&hot, 1.0, 1.0, 0.3, 0.52).expect("acts");
        assert!(weak.background.exposure_ev > strong.background.exposure_ev);
        assert!(weak.subject.clarity < strong.subject.clarity);
    }

    #[test]
    fn severity_takes_the_worst_trigger_rather_than_their_sum() {
        let one_bad = Competition::measure(region(0.30, 0.05, 0.20), region(0.95, 0.02, 0.80), 0);
        let three_mild =
            Competition::measure(region(0.45, 0.05, 0.20), region(0.55, 0.13, 0.80), 1);
        assert!(one_bad.severity() > three_mild.severity());
        assert!(three_mild.severity() <= 1.0);
    }
}
