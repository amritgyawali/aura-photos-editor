//! How one person should look across a whole wedding.
//!
//! Section 6.3, and it carries the phase's most quotable promise:
//!
//! > Guarantee: the same person's skin dE00 spread across the gallery <= 2.0 after correction - a
//! > measurable claim no competitor makes.
//!
//! A promise about a measurement has to be *measured*, so `gallery_skin_target.spread_after` is a
//! stored column and "the promise holds" is `SELECT MAX(spread_after)`. Phase 16 wrote this rule
//! for its skin guard and phase 22 for its identity drift; this is its third application and the
//! first at gallery scale.
//!
//! ## Four properties that are structural rather than promised
//!
//! **The target is that person's own frames.** There is no ideal-skin constant in this module, in
//! the contract, in `consistency.toml` or in migration 25, and the phase gate scans for one on
//! every run. Phase 15's argument, unchanged: a fixed target is how an editor lightens dark skin
//! while believing it is correcting a cast, and a code path with no constant to compare against
//! cannot do it. What exists instead is [`TargetBuilder`], which learns a person's appearance from
//! *this wedding* and corrects toward that.
//!
//! **The correction is capped by the light rather than by a global number.**
//! `SkinCorrection::cap_for_mood` reduces the cap as the frame's light becomes more intentional. A
//! candle-lit face may stay warm; it may not go magenta. That is section 6.3's own sentence as
//! arithmetic, and the cap falls to a fifth rather than to zero because a magenta cast on a
//! candle-lit face is still a magenta cast.
//!
//! **It is a residual on the phase 16 grade and phase 16's guard re-runs after it.** Phase 17's
//! rule - the shift happens before the guards, and every guard re-runs after it - in its fourth
//! application. A consistency correction that would move somebody's skin outside phase 16's hue and
//! chroma ceilings is a correction phase 16's guard withdraws.
//!
//! **Below `MIN_SKIN_FRAMES` an identity has no target at all.** Phase 15's argument for
//! `MIN_LOCUS_SAMPLES`: a target fitted to two frames is a target fitted to one lighting
//! condition, and a weak target is worse than none because it looks like evidence.
//!
//! ## The input port, and why this crate owns no fallback for it
//!
//! [`SkinField`] is how a per-frame, per-identity skin reading reaches this module. Phase 19's
//! rule applies - a phase that consumes another phase's output owns no fallback for it - and the
//! safe direction here is **none**: without a field the correction is not attempted and
//! `GalleryCode::SkinMaskAbsent` is recorded, which is a different row from
//! `GalleryCode::SkinTargetAbsent`. One says the product could not look; the other says it looked
//! and the person was not in enough well-lit frames. Phase 24's rule.
//!
//! In this build the field is always absent: [`crate::SKIN_FIELD_AVAILABLE`] is false because phase
//! 18's segmentation head is untrained and produces no identity-scoped skin region. Everything
//! below is exercised end to end against authored readings, which proves the arithmetic and says
//! nothing about a person. Condition C2 of the exit report.

use std::collections::BTreeMap;

use aura_core::contract::gallery::ImageId;
use aura_core::contract::gallery::{
    GalleryCode, SkinCorrection, SkinTarget, MIN_SKIN_FRAMES, SKIN_DE00_SPREAD_CEILING,
    SKIN_LUMA_CAP,
};
use aura_core::IdentityId;
use aura_raw::colour::de2000::{ciede2000, xyz_d65_to_lab};
use aura_raw::colour::illuminant::{uv_distance, uv_to_linear_srgb};
use aura_raw::colour::matrix::{apply, SRGB_TO_XYZ_D65};

use crate::stats;

/// One frame's reading of one person's skin.
///
/// Measured inside phase 18's identity-scoped `MaskKind::Skin` region, on the same 2048 px proxy
/// every phase from 09 onward decides on. Invariant 3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinReading {
    /// The photograph.
    pub image: ImageId,
    /// Whose skin.
    pub identity: IdentityId,
    /// The observed chromaticity, in CIE 1976 `u'v'`.
    pub uv: [f32; 2],
    /// The observed luminance, `0..1`.
    pub luma: f32,
    /// How much of the region the mask was sure about, `0..1`.
    ///
    /// The geometric mean of phase 18's two independent uncertainties, which is
    /// `Mask::allowance`. A reading behind a doubtful mask is a reading of the wall behind
    /// somebody's ear, and it is weighted accordingly rather than dropped: dropping it would make
    /// a person photographed only in hard light have no target at all.
    pub mask_quality: f32,
    /// How intentional the light on this frame is, `0..1`.
    ///
    /// What `SkinCorrection::cap_for_mood` is given.
    pub mood: f32,
}

/// The one route to a per-frame skin reading.
///
/// A port rather than a function, so a caller supplies real mattes in the product and authored
/// readings in a test, and this crate owns neither. The same shape phase 19's `MaskField` and
/// phase 22's `RestoreField` take.
pub trait SkinField: std::fmt::Debug + Send + Sync {
    /// Every identity's skin reading on one photograph, or an empty slice when none could be
    /// measured.
    ///
    /// An empty answer is `GalleryCode::SkinMaskAbsent` and never a zero reading. A chromaticity of
    /// `[0.0, 0.0]` is a real colour, and a correction toward a target from it would be enormous.
    fn readings(&self, image: ImageId) -> Vec<SkinReading>;
}

/// The luminance band a reading has to sit in to contribute to a target.
///
/// Phase 15's `CONTRIBUTING_LUMA`, restated rather than imported, because this crate does not
/// depend on `aura-brain-photo` and the number means the same thing for the same reason: a face at
/// 2 % linear luminance carries about three bits of chroma and one at 75 % has begun to clip, and
/// neither says what colour anybody's skin is.
pub const CONTRIBUTING_LUMA: (f32, f32) = (0.02, 0.75);

/// The mask quality a reading needs before it contributes to a target.
///
/// Contributing to a *target* is a higher bar than being corrected, because a target is what every
/// other frame of that person is moved toward. Phase 18's `AGGRESSIVE_FLOOR` is 0.45 for an
/// operation on one frame; this is above it.
pub const CONTRIBUTING_MASK_QUALITY: f32 = 0.60;

/// Accumulates one project's skin readings, by identity.
///
/// A `BTreeMap` rather than a hash map, for the reason every collection in this product is ordered:
/// the iteration order reaches a report, a log line and a determinism test.
#[derive(Debug, Clone, Default)]
pub struct TargetBuilder {
    readings: BTreeMap<IdentityId, Vec<SkinReading>>,
}

impl TargetBuilder {
    /// A new, empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Offer one reading. Rejected silently when it is not well-lit enough to contribute.
    pub fn add(&mut self, reading: SkinReading) {
        if reading.luma < CONTRIBUTING_LUMA.0 || reading.luma > CONTRIBUTING_LUMA.1 {
            return;
        }
        if reading.mask_quality < CONTRIBUTING_MASK_QUALITY {
            return;
        }
        // A frame whose light is intentional says what a candle does to somebody, not what their
        // skin is. Section 6.3's "best-lit frames" excludes it in the same breath as it excludes
        // an underexposed one.
        if reading.mood > 0.5 {
            return;
        }
        self.readings
            .entry(reading.identity)
            .or_default()
            .push(reading);
    }

    /// How many usable readings one identity has.
    #[must_use]
    pub fn count(&self, identity: IdentityId) -> usize {
        self.readings.get(&identity).map_or(0, Vec::len)
    }

    /// Every identity that offered a reading, usable or not.
    #[must_use]
    pub fn identities(&self) -> Vec<IdentityId> {
        self.readings.keys().copied().collect()
    }

    /// Finish, producing one target per identity that has enough evidence.
    ///
    /// An identity below [`MIN_SKIN_FRAMES`] produces **no target**, which is
    /// `GalleryCode::SkinTargetAbsent` rather than a target nobody should trust.
    #[must_use]
    pub fn finish(&self, analysis_ver: u16) -> BTreeMap<IdentityId, SkinTarget> {
        let mut out = BTreeMap::new();
        for (identity, readings) in &self.readings {
            if readings.len() < MIN_SKIN_FRAMES as usize {
                continue;
            }
            let uvs: Vec<[f32; 2]> = readings.iter().map(|r| r.uv).collect();
            let lumas: Vec<f32> = readings.iter().map(|r| r.luma).collect();
            let (Some(uv), Some(luma)) = (stats::median_uv(&uvs), stats::median(&lumas)) else {
                continue;
            };
            let spread_before = spread_of(readings, uv, luma);
            out.insert(
                *identity,
                SkinTarget {
                    identity: *identity,
                    uv,
                    luma,
                    frames: readings.len().min(u32::MAX as usize) as u32,
                    spread_before,
                    // Filled in by `measure_after` once the corrections are solved. Zero here would
                    // be a claim the promise already holds, which nothing has yet measured.
                    spread_after: spread_before,
                    analysis_ver,
                },
            );
        }
        out
    }
}

/// The dE00 spread of a set of readings about a centre.
///
/// A mean absolute deviation in dE00 rather than a standard deviation, for the reason
/// [`crate::stats::mean_abs_deviation`] gives: what a person sees is how far apart two frames look,
/// and squaring makes the worst frame dominate a number that is supposed to describe the set.
#[must_use]
pub fn spread_of(readings: &[SkinReading], uv: [f32; 2], luma: f32) -> f32 {
    if readings.len() < 2 {
        return 0.0;
    }
    let errors: Vec<f32> = readings
        .iter()
        .map(|reading| de00_between(reading.uv, reading.luma, uv, luma))
        .collect();
    errors.iter().sum::<f32>() / errors.len() as f32
}

/// The dE00 between two skin appearances, each a chromaticity and a luminance.
///
/// The chromaticity is turned back into a linear RGB at the given luminance and then into Lab
/// through `aura_raw::colour`, which is the one implementation of that arithmetic in the workspace.
/// Two copies of a colour conversion is two answers to what a person's skin looks like - phase 23's
/// argument for putting the lens polynomial in `aura-raw`.
#[must_use]
pub fn de00_between(uv_a: [f32; 2], luma_a: f32, uv_b: [f32; 2], luma_b: f32) -> f32 {
    let lab = |uv: [f32; 2], luma: f32| {
        let rgb = uv_to_linear_srgb(uv);
        let sum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        let scale = if sum > 1e-6 { luma / sum } else { 0.0 };
        let xyz = apply(
            SRGB_TO_XYZ_D65,
            [
                f64::from(rgb[0] * scale),
                f64::from(rgb[1] * scale),
                f64::from(rgb[2] * scale),
            ],
        );
        xyz_d65_to_lab(xyz)
    };
    ciede2000(lab(uv_a, luma_a), lab(uv_b, luma_b)) as f32
}

/// Plan one frame's skin correction against an identity's gallery target.
///
/// Returns `None` when the reading is already at the target, which is a correction of nothing
/// rather than a correction of zero: a stored row with six zeroes would say the product moved
/// somebody's skin by no distance, and `GalleryCode::AlreadyConsistent` is the honest answer.
#[must_use]
pub fn correct(reading: &SkinReading, target: &SkinTarget) -> Option<SkinCorrection> {
    if !target.is_usable() {
        return None;
    }
    let de00_before = de00_between(reading.uv, reading.luma, target.uv, target.luma);
    if de00_before < 0.25 {
        // Under a quarter of a dE00 nothing in this product can resolve the difference and nothing
        // a person can see is at stake. Phase 22's rule about instruments.
        return None;
    }

    let cap = SkinCorrection::cap_for_mood(reading.mood);
    let want_u = target.uv[0] - reading.uv[0];
    let want_v = target.uv[1] - reading.uv[1];
    let distance = uv_distance(reading.uv, target.uv);
    let (d_u, d_v, capped) = if distance <= cap || distance <= f32::EPSILON {
        (want_u, want_v, false)
    } else {
        let scale = cap / distance;
        (want_u * scale, want_v * scale, true)
    };

    let want_luma = target.luma - reading.luma;
    let d_luma = want_luma.clamp(-SKIN_LUMA_CAP, SKIN_LUMA_CAP);
    let luma_capped = (want_luma - d_luma).abs() > f32::EPSILON;

    let after_uv = [reading.uv[0] + d_u, reading.uv[1] + d_v];
    let after_luma = reading.luma + d_luma;
    let de00_after = de00_between(after_uv, after_luma, target.uv, target.luma);

    Some(SkinCorrection {
        identity: target.identity,
        d_uv: [d_u, d_v],
        d_luma,
        de00_before,
        de00_after,
        cap,
        capped: capped || luma_capped,
    })
}

/// Re-measure an identity's spread with the corrections applied, and record it on the target.
///
/// Section 6.3's promise is about the *corrected* gallery, so the number that proves it has to be
/// measured after the corrections are planned rather than predicted from the caps. The readings and
/// the corrections are paired by photograph; a reading with no correction contributes unchanged,
/// which is what makes a target whose frames were all inside tolerance report the spread it
/// actually has rather than zero.
pub fn measure_after(
    target: &mut SkinTarget,
    readings: &[SkinReading],
    corrections: &BTreeMap<ImageId, SkinCorrection>,
) {
    let mut moved: Vec<SkinReading> = Vec::with_capacity(readings.len());
    for reading in readings {
        let mut after = *reading;
        if let Some(correction) = corrections.get(&reading.image) {
            after.uv = [
                reading.uv[0] + correction.d_uv[0],
                reading.uv[1] + correction.d_uv[1],
            ];
            after.luma = (reading.luma + correction.d_luma).clamp(0.0, 1.0);
        }
        moved.push(after);
    }
    target.spread_after = spread_of(&moved, target.uv, target.luma);
}

/// Which code a frame's skin half records.
///
/// Three answers, and keeping them apart is the whole of phase 24's rule. `SkinMaskAbsent` says the
/// product could not look. `SkinTargetAbsent` says it looked and this person was not in enough
/// well-lit frames. `SkinNormalised` says it looked, knew, and moved something.
#[must_use]
pub fn code_for(
    had_reading: bool,
    had_target: bool,
    corrected: bool,
    over_spread: bool,
) -> GalleryCode {
    if !had_reading {
        GalleryCode::SkinMaskAbsent
    } else if !had_target {
        GalleryCode::SkinTargetAbsent
    } else if over_spread {
        GalleryCode::SkinOutlier
    } else if corrected {
        GalleryCode::SkinNormalised
    } else {
        GalleryCode::AlreadyConsistent
    }
}

/// True when a corrected frame is still outside the gallery promise.
#[must_use]
pub fn is_skin_outlier(correction: &SkinCorrection) -> bool {
    correction.de00_after > SKIN_DE00_SPREAD_CEILING
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(identity: IdentityId, uv: [f32; 2], luma: f32) -> SkinReading {
        SkinReading {
            image: ImageId::new(),
            identity,
            uv,
            luma,
            mask_quality: 0.85,
            mood: 0.0,
        }
    }

    #[test]
    fn an_identity_with_four_readings_has_no_target() {
        let identity = IdentityId::new();
        let mut builder = TargetBuilder::new();
        for _ in 0..4 {
            builder.add(reading(identity, [0.24, 0.50], 0.45));
        }
        assert!(
            builder.finish(1).is_empty(),
            "a weak target looks like evidence"
        );
    }

    #[test]
    fn an_identity_with_five_readings_has_one() {
        let identity = IdentityId::new();
        let mut builder = TargetBuilder::new();
        for i in 0..5 {
            builder.add(reading(identity, [0.24 + i as f32 * 0.001, 0.50], 0.45));
        }
        let targets = builder.finish(1);
        let target = targets.get(&identity).expect("five is enough");
        assert!(target.is_usable());
        assert_eq!(target.frames, 5);
        assert!(target.uv[0] > 0.24 && target.uv[0] < 0.25);
    }

    #[test]
    fn a_badly_lit_or_badly_masked_reading_does_not_contribute() {
        let identity = IdentityId::new();
        let mut builder = TargetBuilder::new();
        let mut dark = reading(identity, [0.24, 0.50], 0.005);
        builder.add(dark);
        dark.luma = 0.95;
        builder.add(dark);
        let mut doubtful = reading(identity, [0.24, 0.50], 0.45);
        doubtful.mask_quality = 0.3;
        builder.add(doubtful);
        let mut candle = reading(identity, [0.24, 0.50], 0.45);
        candle.mood = 0.9;
        builder.add(candle);
        assert_eq!(builder.count(identity), 0);
    }

    #[test]
    fn a_target_is_the_middle_of_that_persons_own_frames_and_never_a_constant() {
        // Two people whose skin sits in two different places both get their own target, and the
        // targets are as far apart as the people are. A fixed reference would collapse them.
        let dark = IdentityId::new();
        let light = IdentityId::new();
        let mut builder = TargetBuilder::new();
        for _ in 0..6 {
            builder.add(reading(dark, [0.255, 0.512], 0.18));
            builder.add(reading(light, [0.228, 0.498], 0.62));
        }
        let targets = builder.finish(1);
        let a = targets[&dark];
        let b = targets[&light];
        assert!((a.luma - 0.18).abs() < 0.01);
        assert!((b.luma - 0.62).abs() < 0.01);
        assert!(uv_distance(a.uv, b.uv) > 0.01);
    }

    #[test]
    fn one_outlying_frame_does_not_move_a_target() {
        let identity = IdentityId::new();
        let mut builder = TargetBuilder::new();
        for _ in 0..6 {
            builder.add(reading(identity, [0.240, 0.500], 0.45));
        }
        builder.add(reading(identity, [0.320, 0.560], 0.45));
        let targets = builder.finish(1);
        let target = targets[&identity];
        assert!(
            (target.uv[0] - 0.240).abs() < 0.005,
            "the median moved to {}",
            target.uv[0]
        );
    }

    #[test]
    fn a_correction_closes_the_gap_and_reports_both_ends_of_it() {
        let identity = IdentityId::new();
        let mut builder = TargetBuilder::new();
        for _ in 0..6 {
            builder.add(reading(identity, [0.240, 0.500], 0.45));
        }
        let target = builder.finish(1)[&identity];
        let drifted = reading(identity, [0.248, 0.506], 0.45);
        let correction = correct(&drifted, &target).expect("a drifted frame is corrected");
        assert!(correction.de00_after < correction.de00_before);
        assert!(correction.closed() > 0.5);
    }

    #[test]
    fn a_candle_lit_face_may_stay_warm_but_the_cap_is_not_zero() {
        let identity = IdentityId::new();
        let mut builder = TargetBuilder::new();
        for _ in 0..6 {
            builder.add(reading(identity, [0.240, 0.500], 0.45));
        }
        let target = builder.finish(1)[&identity];
        let mut candle = reading(identity, [0.290, 0.520], 0.45);
        candle.mood = 1.0;
        let mut ordinary = candle;
        ordinary.mood = 0.0;

        let a = correct(&candle, &target).expect("still corrected");
        let b = correct(&ordinary, &target).expect("corrected");
        let moved_candle = a.d_uv[0].hypot(a.d_uv[1]);
        let moved_ordinary = b.d_uv[0].hypot(b.d_uv[1]);
        assert!(moved_candle < moved_ordinary, "mood reduces the cap");
        assert!(moved_candle > 0.0, "mood does not switch the promise off");
        assert!(a.capped);
    }

    #[test]
    fn a_frame_already_at_the_target_gets_no_correction_rather_than_a_zero_one() {
        let identity = IdentityId::new();
        let mut builder = TargetBuilder::new();
        for _ in 0..6 {
            builder.add(reading(identity, [0.240, 0.500], 0.45));
        }
        let target = builder.finish(1)[&identity];
        assert!(correct(&reading(identity, [0.240, 0.500], 0.45), &target).is_none());
    }

    #[test]
    fn the_promise_is_measured_after_the_corrections_rather_than_predicted() {
        let identity = IdentityId::new();
        let mut builder = TargetBuilder::new();
        let readings: Vec<SkinReading> = (0..8)
            .map(|i| reading(identity, [0.240 + i as f32 * 0.004, 0.500], 0.45))
            .collect();
        for r in &readings {
            builder.add(*r);
        }
        let mut target = builder.finish(1)[&identity];
        let before = target.spread_before;

        let corrections: BTreeMap<ImageId, SkinCorrection> = readings
            .iter()
            .filter_map(|r| correct(r, &target).map(|c| (r.image, c)))
            .collect();
        measure_after(&mut target, &readings, &corrections);
        assert!(
            target.spread_after < before,
            "spread went from {before} to {}",
            target.spread_after
        );
        assert!(target.meets_promise(), "{}", target.spread_after);
    }

    #[test]
    fn an_absent_field_and_an_absent_target_are_different_codes() {
        assert_eq!(
            code_for(false, false, false, false),
            GalleryCode::SkinMaskAbsent
        );
        assert_eq!(
            code_for(true, false, false, false),
            GalleryCode::SkinTargetAbsent
        );
        assert_eq!(
            code_for(true, true, true, false),
            GalleryCode::SkinNormalised
        );
        assert_eq!(code_for(true, true, true, true), GalleryCode::SkinOutlier);
    }

    #[test]
    fn de00_between_two_identical_appearances_is_zero() {
        let d = de00_between([0.24, 0.50], 0.45, [0.24, 0.50], 0.45);
        assert!(d < 1e-3, "{d}");
    }
}
