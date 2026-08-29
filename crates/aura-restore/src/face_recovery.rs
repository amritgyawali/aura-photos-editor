//! Whether a face is inside the narrow band, and whether it is still the same person afterwards.
//!
//! Section 6.3, which is unusually specific and is worth quoting because the implementation is
//! the sentence:
//!
//! > Hard identity constraint: compute the Phase 06 face embedding before and after; if cosine
//! > distance exceeds a small threshold, reduce strength and retry, and if it still fails, skip
//! > and record the reason. This is the guarantee that the product never changes what someone
//! > looks like.
//!
//! ## The band is checked before any model is consulted
//!
//! [`SOFT_FACE_LO`] to [`SOFT_FACE_HI`]. Below the floor a face carries too little information to
//! constrain a prior, so what a prior returns *is* the prior - a plausible face, which is to say
//! somebody else's. The check is first rather than last, so an untrained head can never be the
//! thing that saves a frame from it.
//!
//! ## The constraint measures through the renderer, and it can only refuse
//!
//! [`enforce`] renders the plan through `aura_render::restore::apply` - the same code the
//! delivered JPEG goes through - crops the face, embeds both crops through the caller's
//! [`IdentityProbe`], and compares. Above [`MAX_IDENTITY_DRIFT`] the strength drops by
//! [`RESOLVE_STEP`] and it renders again, at most [`MAX_RESOLVES`] times. Still above, and the
//! face is **skipped** - removed from the plan entirely.
//!
//! There is deliberately no fourth outcome. Phase 16's skin guard re-solves a grade and phase
//! 20's texture guard re-solves and withdraws; this one re-solves and then refuses, because a
//! face that has drifted a little is a face that has drifted.
//!
//! ## The measurement is stored whether it passed or not
//!
//! `restore_face.identity_drift` is on every row that reached a render, which is what makes
//! section 10.1's "below threshold on 100 % of fixtures" a query rather than a sentence. Phase 16
//! established that a guarantee you cannot query is a guarantee you cannot find out you have
//! lost.
//!
//! ## Nothing in this build calls [`solve`] with a result
//!
//! [`FACE_RECOVERY_HEAD_TRAINED`] is false and [`solve`] returns `None` on every frame. There is
//! deliberately no measured fallback, unlike phase 20's blemish detector: the measurement that
//! would stand in for a face prior is unsharp masking on a face, and that is not a weaker version
//! of face recovery - it is a different operation with a worse result and the same name.
//! ADR-0047 section 6.

use std::fmt;

use aura_core::contract::composition::Box2;
use aura_core::contract::ids::IdentityId;
use aura_core::contract::restore::{
    RecoveredFace, RestoreCode, RestoreReason, MAX_FACE_RECOVERY, MAX_IDENTITY_DRIFT,
    MAX_RECOVERED_FACES, MAX_RESOLVES, RESOLVE_STEP, SOFT_FACE_HI, SOFT_FACE_LO,
};
use aura_render::restore::{self, RestoreContext, RestoreOps};

/// Whether a trained face-recovery head is registered and trusted in this build.
///
/// **False, and it is not a placeholder for a fallback.** See the module header and ADR-0047
/// section 6: the operation that would stand in for a face prior is a different operation. When a
/// trained head arrives, this becomes true, `solve` starts returning a strength, and nothing else
/// in this module changes - the constraint below it was written to hold a model rather than to
/// hold this.
pub const FACE_RECOVERY_HEAD_TRAINED: bool = false;

/// The smallest face, as a fraction of the frame's shorter side, worth considering.
///
/// A twentieth. Below this the face is a few dozen pixels across, the identity measurement is
/// being taken over a crop with almost no information in it, and a distance measured on one is a
/// distance that means nothing in either direction.
pub const MIN_FACE_FRACTION: f32 = 0.05;

/// Embed one aligned face crop, the way phase 06 does.
///
/// A port rather than a dependency, the shape phases 19, 21 and this phase all use for something
/// another phase owns. `aura-restore` must not keep its own face recogniser - phase 06's rule,
/// and the reason it matters here is sharper than usual: an identity constraint measured with a
/// *different* embedding from the one the product groups people with is a constraint that is
/// protecting something other than the thing a photographer would notice.
///
/// Returning `None` means the embedding could not be produced, and the caller then does not
/// recover the face at all. A guarantee that cannot be measured is a guarantee that cannot be
/// kept.
pub trait IdentityProbe: Send + Sync + fmt::Debug {
    /// Embed one crop of interleaved linear RGB.
    fn embed(&self, rgb: &[f32], width: usize, height: usize) -> Option<Vec<f32>>;
}

/// One face this phase is considering.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceCandidate {
    /// Whose face, when phase 06 has assigned one.
    pub identity: Option<IdentityId>,
    /// Where it is, in frame coordinates.
    pub bounds: Box2,
    /// The measured sharpness of the face region, `0..1`.
    pub sharpness: f32,
}

/// What the strength solver decided for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryChoice {
    /// The plan-wide strength, when there is one.
    pub strength: Option<f32>,
    /// One record per face considered.
    pub faces: Vec<RecoveredFace>,
    /// Why.
    pub reasons: Vec<RestoreReason>,
}

/// Which faces are inside the band, and what strength they would get.
///
/// **Returns `None` for the strength on every frame in this build**, because
/// [`FACE_RECOVERY_HEAD_TRAINED`] is false. Every face still gets a record, so a photographer can
/// see that AURA looked and why it did nothing - phase 20's rule that what was left alone is
/// shown as prominently as what was done.
#[must_use]
pub fn solve(candidates: &[FaceCandidate], scene_allows: bool, ceiling: f32) -> RecoveryChoice {
    let mut reasons = Vec::new();
    let mut faces = Vec::new();

    if candidates.is_empty() {
        reasons.push(RestoreReason::plain(RestoreCode::NoFaces, 0.0));
        return RecoveryChoice {
            strength: None,
            faces,
            reasons,
        };
    }

    let mut in_band = 0usize;
    for candidate in candidates.iter().take(MAX_RECOVERED_FACES) {
        // The band, checked before anything else. See the module header.
        let too_small = candidate.bounds.w.min(candidate.bounds.h) < MIN_FACE_FRACTION;
        let code = if candidate.sharpness > SOFT_FACE_HI {
            Some(RestoreCode::FaceSharpEnough)
        } else if candidate.sharpness < SOFT_FACE_LO || too_small {
            Some(RestoreCode::FaceTooBlurred)
        } else if !scene_allows {
            Some(RestoreCode::FaceSharpEnough)
        } else if !FACE_RECOVERY_HEAD_TRAINED {
            Some(RestoreCode::RecoveryHeadUntrained)
        } else {
            None
        };

        if let Some(code) = code {
            faces.push(RecoveredFace {
                identity: candidate.identity,
                bounds: candidate.bounds,
                sharpness: candidate.sharpness,
                strength: 0.0,
                identity_drift: 0.0,
                resolves: 0,
                skipped: true,
                skipped_because: Some(code),
            });
        } else {
            in_band += 1;
            faces.push(RecoveredFace {
                identity: candidate.identity,
                bounds: candidate.bounds,
                sharpness: candidate.sharpness,
                // The opening strength, before the constraint has seen a pixel. It rises the
                // softer the face is inside the band, because a face at the sharp end needs
                // almost nothing - and it is capped by the contract and by the profile file,
                // whichever is lower.
                strength: opening_strength(candidate.sharpness, ceiling),
                identity_drift: 0.0,
                resolves: 0,
                skipped: false,
                skipped_because: None,
            });
        }
    }

    // One reason per distinct outcome rather than one per face: a group shot where twelve faces
    // were all sharp enough is one sentence, not twelve.
    let mut seen: Vec<RestoreCode> = Vec::new();
    for face in &faces {
        if let Some(code) = face.skipped_because {
            if !seen.contains(&code) {
                seen.push(code);
                reasons.push(RestoreReason::plain(code, -0.3));
            }
        }
    }
    if in_band > 0 {
        reasons.push(RestoreReason::plain(RestoreCode::FaceRecovered, 0.5));
    }

    let strength = faces
        .iter()
        .filter(|face| !face.skipped)
        .map(|face| face.strength)
        .fold(0.0_f32, f32::max);
    RecoveryChoice {
        strength: (strength > 0.0).then_some(strength),
        faces,
        reasons,
    }
}

/// The strength a face of one sharpness opens at, before the constraint.
#[must_use]
pub fn opening_strength(sharpness: f32, ceiling: f32) -> f32 {
    let span = (SOFT_FACE_HI - SOFT_FACE_LO).max(1e-6);
    let softness = ((SOFT_FACE_HI - sharpness) / span).clamp(0.0, 1.0);
    (softness * ceiling.min(MAX_FACE_RECOVERY)).clamp(0.0, MAX_FACE_RECOVERY)
}

/// What the identity constraint did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnforceReport {
    /// Faces that were skipped because the embedding still moved too far.
    pub skipped_for_identity: usize,
    /// Faces whose strength was reduced to bring the embedding back.
    pub reduced: usize,
    /// How many reductions happened in total.
    pub resolves: u8,
    /// The largest movement over the faces that were kept, `0..1`.
    pub worst_kept_drift: f32,
}

/// Hold every face to the identity ceiling, reducing and then refusing.
///
/// `pixels` is the frame **before** this phase's operations, interleaved linear RGB. It is
/// re-rendered from that baseline on every attempt rather than being edited in place, because a
/// second pass over an already-recovered buffer measures the second recovery against the first
/// rather than against the photograph.
///
/// Returns the faces as they stand after the constraint, and mutates `faces` in place.
pub fn enforce(
    pixels: &[f32],
    width: usize,
    height: usize,
    faces: &mut [RecoveredFace],
    context: &RestoreContext,
    probe: &dyn IdentityProbe,
) -> EnforceReport {
    let mut report = EnforceReport::default();
    if width == 0 || height == 0 || pixels.len() < width * height * 3 {
        return report;
    }

    for face in faces.iter_mut() {
        if face.skipped || face.strength <= 0.0 {
            continue;
        }
        let Some(box_px) = to_pixels(face.bounds, width, height) else {
            face.skipped = true;
            face.strength = 0.0;
            face.skipped_because = Some(RestoreCode::FaceTooBlurred);
            continue;
        };

        let (before_crop, cw, ch) = restore::face_crop(pixels, width, height, box_px);
        let Some(before) = probe.embed(&before_crop, cw, ch) else {
            // A guarantee that cannot be measured is a guarantee that cannot be kept.
            face.skipped = true;
            face.strength = 0.0;
            face.skipped_because = Some(RestoreCode::RecoveryHeadUntrained);
            continue;
        };

        let mut strength = face.strength;
        let mut resolves = 0u8;
        let mut drift;
        loop {
            let mut attempt = pixels.to_vec();
            let ops = RestoreOps {
                face_recovery: strength,
                ..RestoreOps::default()
            };
            // Only this face, so the measurement is about this face rather than about whatever
            // the other three in the frame did.
            let single = RestoreContext {
                regions: context.regions.clone(),
                sigma: context.sigma,
                faces: vec![box_px],
            };
            restore::apply(&mut attempt, width, height, &ops, &single);
            let (after_crop, aw, ah) = restore::face_crop(&attempt, width, height, box_px);
            drift = match probe.embed(&after_crop, aw, ah) {
                Some(after) => cosine_distance(&before, &after),
                None => 1.0,
            };

            if drift <= MAX_IDENTITY_DRIFT {
                break;
            }
            if resolves >= MAX_RESOLVES {
                break;
            }
            strength *= RESOLVE_STEP;
            resolves += 1;
            report.resolves = report.resolves.saturating_add(1);
        }

        face.resolves = resolves;
        face.identity_drift = drift.clamp(0.0, 1.0);
        if drift > MAX_IDENTITY_DRIFT {
            // The only refusal in the product that fires on a *measurement of a person*. See the
            // module header: there is no fourth outcome.
            face.skipped = true;
            face.strength = 0.0;
            face.skipped_because = Some(RestoreCode::IdentityDriftSkipped);
            report.skipped_for_identity += 1;
        } else {
            if resolves > 0 {
                report.reduced += 1;
            }
            face.strength = strength;
            report.worst_kept_drift = report.worst_kept_drift.max(face.identity_drift);
        }
    }
    report
}

/// One face box in frame coordinates, as pixels.
///
/// `None` when the box is degenerate or falls outside the frame, which is a face nothing can be
/// measured on rather than a face at the origin.
#[must_use]
pub fn to_pixels(
    bounds: Box2,
    width: usize,
    height: usize,
) -> Option<(usize, usize, usize, usize)> {
    if !(bounds.w > 0.0 && bounds.h > 0.0) {
        return None;
    }
    let x = (bounds.x * width as f32).round().max(0.0) as usize;
    let y = (bounds.y * height as f32).round().max(0.0) as usize;
    let w = (bounds.w * width as f32).round().max(0.0) as usize;
    let h = (bounds.h * height as f32).round().max(0.0) as usize;
    if x >= width || y >= height || w < 4 || h < 4 {
        return None;
    }
    Some((x, y, w.min(width - x), h.min(height - y)))
}

/// Cosine distance between two embeddings, `0..2` clamped to `0..1`.
///
/// The measure phase 06 groups identities with. A zero-norm vector on either side is a distance
/// of one - "as different as it gets" - rather than zero, because a probe that returned nothing
/// useful must not read as a perfect match.
#[must_use]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 1.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += f64::from(*x) * f64::from(*y);
        norm_a += f64::from(*x) * f64::from(*x);
        norm_b += f64::from(*y) * f64::from(*y);
    }
    if norm_a <= f64::EPSILON || norm_b <= f64::EPSILON {
        return 1.0;
    }
    let similarity = dot / (norm_a.sqrt() * norm_b.sqrt());
    ((1.0 - similarity) as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
// `-D warnings` on the command line beats the crate-level `cfg_attr(test, allow(..))`
// block, so a test that compares two floats it computed itself needs the allow here.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    /// A probe whose vector **rotates** with the crop own high-band energy.
    ///
    /// See `crate::fixtures::BandProbe` for why the response is an angle rather than a scaled
    /// component: cosine distance is a function of direction, so a probe that multiplies one
    /// element by a large gain reports a *smaller* identity change the more sensitive it claims
    /// to be.
    #[derive(Debug)]
    struct BandProbe {
        /// Radians of rotation per unit of high-band energy.
        gain: f32,
    }

    impl IdentityProbe for BandProbe {
        fn embed(&self, rgb: &[f32], width: usize, height: usize) -> Option<Vec<f32>> {
            if width == 0 || height == 0 {
                return None;
            }
            let plane = aura_render::spatial::luma_plane(rgb, width, height);
            let bands = aura_render::bands::separate(&plane, width, height);
            let angle = bands.high_energy() * self.gain;
            Some(vec![angle.cos(), angle.sin()])
        }
    }

    #[derive(Debug)]
    struct BlindProbe;

    impl IdentityProbe for BlindProbe {
        fn embed(&self, _rgb: &[f32], _width: usize, _height: usize) -> Option<Vec<f32>> {
            None
        }
    }

    fn textured(width: usize, height: usize) -> Vec<f32> {
        let mut pixels = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let base = 0.35 + 0.2 * ((x / 3 + y / 3) % 2) as f32;
                let fine = if (x + y) % 2 == 0 { 0.04 } else { -0.04 };
                let value = (base + fine).clamp(0.0, 1.0);
                pixels.extend_from_slice(&[value, value * 0.95, value * 0.9]);
            }
        }
        pixels
    }

    fn candidate(sharpness: f32) -> FaceCandidate {
        FaceCandidate {
            identity: None,
            bounds: Box2 {
                x: 0.2,
                y: 0.2,
                w: 0.5,
                h: 0.5,
            },
            sharpness,
        }
    }

    #[test]
    fn no_face_in_this_build_is_recovered_and_every_one_says_why() {
        // `FACE_RECOVERY_HEAD_TRAINED` is false, so every candidate is skipped - and skipped with
        // a code rather than silently, which is the difference between a product that declined
        // and a product that missed.
        let choice = solve(&[candidate(0.55)], true, MAX_FACE_RECOVERY);
        assert!(choice.strength.is_none());
        assert_eq!(choice.faces.len(), 1);
        assert!(choice.faces[0].skipped);
        assert_eq!(
            choice.faces[0].skipped_because,
            Some(RestoreCode::RecoveryHeadUntrained)
        );
        assert!(choice.faces[0].problem().is_none());
    }

    #[test]
    fn a_frame_with_no_faces_says_so_rather_than_saying_nothing() {
        let choice = solve(&[], true, MAX_FACE_RECOVERY);
        assert!(choice.strength.is_none());
        assert!(choice.faces.is_empty());
        assert_eq!(choice.reasons[0].code, RestoreCode::NoFaces);
    }

    #[test]
    fn the_band_is_checked_before_the_head_is() {
        // The ordering matters: a face outside the band must report *why it was outside the
        // band* rather than reporting that a head is untrained, because the first is permanent
        // and the second is a property of this build.
        let sharp = solve(&[candidate(SOFT_FACE_HI + 0.1)], true, MAX_FACE_RECOVERY);
        assert_eq!(
            sharp.faces[0].skipped_because,
            Some(RestoreCode::FaceSharpEnough)
        );
        let blurred = solve(&[candidate(SOFT_FACE_LO - 0.1)], true, MAX_FACE_RECOVERY);
        assert_eq!(
            blurred.faces[0].skipped_because,
            Some(RestoreCode::FaceTooBlurred)
        );
    }

    #[test]
    fn a_tiny_face_is_treated_as_unrecoverable() {
        let mut small = candidate(0.55);
        small.bounds.w = 0.02;
        small.bounds.h = 0.02;
        let choice = solve(&[small], true, MAX_FACE_RECOVERY);
        assert_eq!(
            choice.faces[0].skipped_because,
            Some(RestoreCode::FaceTooBlurred)
        );
    }

    #[test]
    fn the_opening_strength_rises_with_softness_and_never_passes_the_cap() {
        let soft = opening_strength(SOFT_FACE_LO, MAX_FACE_RECOVERY);
        let nearly_sharp = opening_strength(SOFT_FACE_HI - 0.01, MAX_FACE_RECOVERY);
        assert!(soft > nearly_sharp);
        assert!(soft <= MAX_FACE_RECOVERY);
        assert_eq!(opening_strength(SOFT_FACE_HI, MAX_FACE_RECOVERY), 0.0);
        // A profile file may lower the ceiling and the opening strength follows it.
        assert!(opening_strength(SOFT_FACE_LO, 0.1) <= 0.1);
    }

    #[test]
    fn a_gentle_operator_keeps_the_face_and_records_what_it_measured() {
        // The constraint has to be exercised by something that really does move the pixels;
        // `BandProbe` with a small gain is a face the operator barely disturbs.
        let (width, height) = (64, 64);
        let pixels = textured(width, height);
        let mut faces = vec![RecoveredFace {
            identity: None,
            bounds: Box2 {
                x: 0.2,
                y: 0.2,
                w: 0.5,
                h: 0.5,
            },
            sharpness: 0.55,
            strength: 0.30,
            identity_drift: 0.0,
            resolves: 0,
            skipped: false,
            skipped_because: None,
        }];
        let report = enforce(
            &pixels,
            width,
            height,
            &mut faces,
            &RestoreContext::empty(),
            &BandProbe { gain: 2.0 },
        );
        assert!(!faces[0].skipped, "{:?}", faces[0]);
        assert!(faces[0].identity_drift <= MAX_IDENTITY_DRIFT);
        assert_eq!(report.skipped_for_identity, 0);
        assert!(faces[0].problem().is_none(), "{:?}", faces[0].problem());
    }

    #[test]
    fn a_face_that_keeps_drifting_is_skipped_rather_than_delivered_weaker() {
        // **The guarantee.** A probe that responds violently to the high band makes every
        // strength look like an identity change, and the only correct outcome is a skipped face -
        // not a face at a quarter strength that still moved.
        let (width, height) = (64, 64);
        let pixels = textured(width, height);
        let mut faces = vec![RecoveredFace {
            identity: None,
            bounds: Box2 {
                x: 0.1,
                y: 0.1,
                w: 0.7,
                h: 0.7,
            },
            sharpness: 0.45,
            strength: MAX_FACE_RECOVERY,
            identity_drift: 0.0,
            resolves: 0,
            skipped: false,
            skipped_because: None,
        }];
        let report = enforce(
            &pixels,
            width,
            height,
            &mut faces,
            &RestoreContext::empty(),
            &BandProbe { gain: 100.0 },
        );
        assert!(faces[0].skipped, "a drifting face was delivered");
        assert_eq!(faces[0].strength, 0.0);
        assert_eq!(
            faces[0].skipped_because,
            Some(RestoreCode::IdentityDriftSkipped)
        );
        assert_eq!(faces[0].resolves, MAX_RESOLVES);
        assert_eq!(report.skipped_for_identity, 1);
        // And the measured distance survives on the row, which is what makes the gate a query.
        assert!(faces[0].identity_drift > MAX_IDENTITY_DRIFT);
        assert!(faces[0].problem().is_none(), "{:?}", faces[0].problem());
    }

    #[test]
    fn a_face_that_cannot_be_embedded_is_not_recovered() {
        // A guarantee that cannot be measured is a guarantee that cannot be kept.
        let (width, height) = (48, 48);
        let pixels = textured(width, height);
        let mut faces = vec![RecoveredFace {
            identity: None,
            bounds: Box2 {
                x: 0.2,
                y: 0.2,
                w: 0.5,
                h: 0.5,
            },
            sharpness: 0.55,
            strength: 0.30,
            identity_drift: 0.0,
            resolves: 0,
            skipped: false,
            skipped_because: None,
        }];
        enforce(
            &pixels,
            width,
            height,
            &mut faces,
            &RestoreContext::empty(),
            &BlindProbe,
        );
        assert!(faces[0].skipped);
        assert_eq!(
            faces[0].skipped_because,
            Some(RestoreCode::RecoveryHeadUntrained)
        );
    }

    #[test]
    fn the_distance_is_one_when_a_vector_says_nothing() {
        assert_eq!(cosine_distance(&[], &[]), 1.0);
        assert_eq!(cosine_distance(&[0.0, 0.0], &[1.0, 1.0]), 1.0);
        assert_eq!(cosine_distance(&[1.0, 0.0], &[1.0, 0.0, 0.0]), 1.0);
        assert!(cosine_distance(&[1.0, 2.0], &[1.0, 2.0]) < 1e-6);
        assert!(cosine_distance(&[1.0, 0.0], &[0.0, 1.0]) > 0.9);
    }

    #[test]
    fn a_degenerate_face_box_maps_to_nothing_rather_than_to_the_origin() {
        let zero = Box2 {
            x: 0.1,
            y: 0.1,
            w: 0.0,
            h: 0.2,
        };
        assert!(to_pixels(zero, 100, 100).is_none());
        let outside = Box2 {
            x: 1.5,
            y: 0.1,
            w: 0.2,
            h: 0.2,
        };
        assert!(to_pixels(outside, 100, 100).is_none());
        let ok = Box2 {
            x: 0.25,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        };
        assert_eq!(to_pixels(ok, 100, 100), Some((25, 25, 50, 50)));
    }
}
