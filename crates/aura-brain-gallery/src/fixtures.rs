//! The synthetic galleries every section 10.1 gate is measured against.
//!
//! **Nothing here is a photograph.** There are no weddings in this repository, so a gate that
//! claims a drift was reduced has to be measured on a gallery whose drift was *authored*: the
//! temperature every frame was given, the transition that was put in the middle of it and the
//! person whose skin was made to wander are all known by construction, and the pass is measured
//! against the answer rather than against a plausible-looking result.
//!
//! That proves the tree, the change-point detector, the anchor ranking, the robust statistics, the
//! solver, the bounds, the idempotence, the skin arithmetic, the outlier threshold and the store.
//! It is **not** evidence that a photographer would call a real gallery consistent. That is
//! condition C1 of `docs/progress/PHASE-25-EXIT.md`, it is a Sev 2 trigger, and it is printed at
//! the end of every gate run rather than hidden in a helper.
//!
//! ## Why the drift is authored rather than sampled
//!
//! A random gallery is a gallery with no right answer, so a gate over one can only assert that a
//! number went down - which every solver that moves anything toward anything achieves. What section
//! 10.1 asks is that the spread reduce by 60 %, that a *transition* survive, and that a *bound* not
//! be exceeded, and all three of those are claims about a specific arrangement of frames. So the
//! arrangements are written down here and named after what they test.

use std::collections::BTreeMap;

use aura_core::contract::gallery::ImageId;
use aura_core::{IdentityId, SceneId, SegmentId};

use crate::skin_consistency::{SkinField, SkinReading};
use crate::tree::Frame;

/// One ordinary frame, confident and unremarkable, at 5,000 K and a mid subject luminance.
///
/// The baseline every other fixture perturbs. Its identity map has one person in it at a moderate
/// prominence, because a frame with nobody in it takes a different path through the anchor ranking
/// and a test that wanted the ordinary path would silently be testing the other one.
#[must_use]
pub fn frame_at(segment: SegmentId, timeline_ms: i64, scene: SceneId) -> Frame {
    let mut identities = BTreeMap::new();
    identities.insert(fixture_identity(), 0.7);
    Frame {
        image: ImageId::new(),
        segment,
        scene,
        timeline_ms,
        cct_k: Some(5000.0),
        tint: Some(0.0),
        exposure_ev: Some(0.0),
        subject_luma: Some(0.45),
        wb_conf: 0.85,
        exposure_conf: 0.80,
        mixed_light: false,
        intentional_light: false,
        mood: 0.0,
        contrast: Some(10.0),
        saturation: Some(4.0),
        signature: Some(
            crate::stats::GradeSignature::new(30.0, 8.0, 40.0, 6.0, 0.10, 0.05, 0.1, 0.02).values,
        ),
        identities,
        user_edited: false,
        enabled: true,
    }
}

/// The one person every fixture frame contains.
///
/// A fixed id parsed from its canonical text, so two runs of a fixture - and two fixtures in the
/// same test - produce the *same* person. A fixture that minted a fresh identity on every call
/// would make a skin target that never accumulated, because each frame would belong to somebody
/// else, and the test would pass by producing nothing at all.
///
/// Parsed rather than constructed because `aura-core` does not re-export `Uuid`, and this crate
/// takes no dependency on `uuid` for one constant.
#[must_use]
pub fn fixture_identity() -> IdentityId {
    IdentityId::from_db("idt_0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0")
        .unwrap_or_else(|_| IdentityId::from_db(FALLBACK_IDENTITY).unwrap_or_default())
}

/// Only reachable if the literal above stops parsing, which a unit test forbids.
const FALLBACK_IDENTITY: &str = "idt_00000000-0000-4000-8000-000000000000";

/// A chapter whose frames drifted, with the drift written down.
///
/// `spread_k` is the peak-to-peak temperature wander across the chapter, applied as a slow ramp
/// with a small oscillation on top - which is what a real drift looks like, because a photographer
/// turning slowly in a room produces a ramp and the camera's own auto white balance produces the
/// oscillation. A pure ramp would be indistinguishable from a sunset and would be split rather than
/// normalised, which is a different test.
#[must_use]
pub fn drifting_chapter(
    segment: SegmentId,
    scene: SceneId,
    count: usize,
    spread_k: f32,
) -> Vec<Frame> {
    (0..count)
        .map(|i| {
            let t = if count > 1 {
                i as f32 / (count - 1) as f32
            } else {
                0.0
            };
            let ramp = (t - 0.5) * spread_k;
            let wobble = ((i % 7) as f32 - 3.0) * (spread_k * 0.04);
            let mut frame = frame_at(segment, i as i64 * 4_000, scene);
            frame.cct_k = Some(5000.0 + ramp + wobble);
            frame.tint = Some(ramp * 0.01);
            frame.subject_luma = Some(0.45 + (t - 0.5) * 0.06);
            frame
        })
        .collect()
}

/// A chapter with a genuine lighting transition in the middle of it.
///
/// The first half is a warm room and the second is a flash-lit one, which is section 2.1's
/// "flash on/off" and the transition the change-point detector must not flatten. Both halves are
/// internally tight, so a detector that missed the boundary would produce one node with an enormous
/// spread and a target in between - which is exactly the failure the gate looks for.
#[must_use]
pub fn transitioning_chapter(segment: SegmentId, scene: SceneId, half: usize) -> Vec<Frame> {
    let mut frames = Vec::with_capacity(half * 2);
    for i in 0..half {
        let mut frame = frame_at(segment, i as i64 * 3_000, scene);
        frame.cct_k = Some(3050.0 + ((i % 5) as f32 - 2.0) * 12.0);
        frame.tint = Some(4.0);
        frame.subject_luma = Some(0.28);
        frames.push(frame);
    }
    for i in 0..half {
        let mut frame = frame_at(segment, (half + i) as i64 * 3_000, scene);
        frame.cct_k = Some(5450.0 + ((i % 5) as f32 - 2.0) * 12.0);
        frame.tint = Some(0.5);
        frame.subject_luma = Some(0.52);
        frames.push(frame);
    }
    frames
}

/// A chapter with one frame a long way from the rest, which no bound can reach.
///
/// The outlier gate's fixture. The stray frame is 2,600 K from its node, which is more than five
/// times the bound, so it stays an outlier however the damping is set - a stray that the bound
/// could reach would make the gate a test of the bound rather than of the detector.
#[must_use]
pub fn chapter_with_a_stray(segment: SegmentId, scene: SceneId, count: usize) -> Vec<Frame> {
    let mut frames = drifting_chapter(segment, scene, count, 120.0);
    if let Some(stray) = frames.last_mut() {
        stray.cct_k = Some(7600.0);
        stray.subject_luma = Some(0.22);
    }
    frames
}

/// A chapter lit by something intentional, which must come out of the pass untouched.
#[must_use]
pub fn intentional_chapter(segment: SegmentId, scene: SceneId, count: usize) -> Vec<Frame> {
    (0..count)
        .map(|i| {
            let mut frame = frame_at(segment, i as i64 * 2_000, scene);
            frame.cct_k = Some(2400.0 + (i % 9) as f32 * 40.0);
            frame.intentional_light = true;
            frame.mood = 0.9;
            frame
        })
        .collect()
}

/// A whole synthetic wedding: four chapters, one of each shape above.
///
/// Returns the frames in capture order, with each chapter in its own segment. The chapters are
/// separated by a gap in the timeline so a caller that ignores segments still sees four groups.
#[must_use]
pub fn wedding() -> Vec<Frame> {
    let mut all = Vec::new();
    let mut push = |mut chapter: Vec<Frame>, offset: i64| {
        for frame in &mut chapter {
            frame.timeline_ms += offset;
        }
        all.extend(chapter);
    };
    push(
        drifting_chapter(SegmentId::new(), SceneId::GettingReadyBride, 40, 600.0),
        0,
    );
    push(
        transitioning_chapter(SegmentId::new(), SceneId::Ceremony, 24),
        30 * 60_000,
    );
    push(
        intentional_chapter(SegmentId::new(), SceneId::FirstDance, 20),
        120 * 60_000,
    );
    push(
        chapter_with_a_stray(SegmentId::new(), SceneId::Speeches, 30),
        180 * 60_000,
    );
    all
}

/// A skin field with authored readings, for the tests the real one cannot run in this build.
///
/// [`crate::SKIN_FIELD_AVAILABLE`] is false, so nothing in the product produces a reading. This
/// produces one per frame for one identity, with a wander of `spread_uv` around a centre - which is
/// the drift section 6.3's gate is measured on.
///
/// The centre is **not** a constant standing in for "correct skin": it is an arbitrary point, the
/// gate measures the *spread* about whatever centre the readings themselves imply, and moving the
/// centre changes nothing the gate asserts. That is the property that makes the fixture legitimate
/// where a fixed reference in the product would not be.
#[derive(Debug, Clone)]
pub struct AuthoredSkin {
    readings: BTreeMap<String, Vec<SkinReading>>,
}

impl AuthoredSkin {
    /// Build a field over a set of frames, with one identity whose skin wanders.
    #[must_use]
    pub fn new(frames: &[Frame], centre_uv: [f32; 2], centre_luma: f32, spread_uv: f32) -> Self {
        let identity = fixture_identity();
        let mut readings: BTreeMap<String, Vec<SkinReading>> = BTreeMap::new();
        for (i, frame) in frames.iter().enumerate() {
            let phase = (i % 11) as f32 / 10.0 - 0.5;
            readings.insert(
                frame.image.to_db(),
                vec![SkinReading {
                    image: frame.image,
                    identity,
                    uv: [
                        centre_uv[0] + phase * spread_uv,
                        centre_uv[1] + phase * spread_uv * 0.5,
                    ],
                    luma: (centre_luma + phase * 0.05).clamp(0.02, 0.75),
                    mask_quality: 0.85,
                    mood: frame.mood,
                }],
            );
        }
        Self { readings }
    }

    /// The identity every reading belongs to.
    #[must_use]
    pub fn identity() -> IdentityId {
        fixture_identity()
    }
}

impl SkinField for AuthoredSkin {
    fn readings(&self, image: ImageId) -> Vec<SkinReading> {
        self.readings
            .get(&image.to_db())
            .cloned()
            .unwrap_or_default()
    }
}

/// A field that never has a reading, which is what this build's real one does.
///
/// Named for what it *is* rather than for what it lacks, because a test that used
/// `Option<&dyn SkinField>` and passed `None` would be testing a different code path from the one
/// the product runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSkinField;

impl SkinField for NoSkinField {
    fn readings(&self, _image: ImageId) -> Vec<SkinReading> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_identity_parses_and_is_the_same_every_call() {
        let a = fixture_identity();
        assert_eq!(a, fixture_identity());
        assert_eq!(a.to_db(), "idt_0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0");
    }

    #[test]
    fn a_drifting_chapter_actually_drifts_by_what_it_was_asked_for() {
        let frames = drifting_chapter(SegmentId::new(), SceneId::Ceremony, 40, 600.0);
        let ccts: Vec<f32> = frames.iter().filter_map(|f| f.cct_k).collect();
        let lo = ccts.iter().copied().fold(f32::MAX, f32::min);
        let hi = ccts.iter().copied().fold(f32::MIN, f32::max);
        assert!((hi - lo) > 500.0, "authored spread was {}", hi - lo);
    }

    #[test]
    fn a_transitioning_chapter_has_two_tight_halves() {
        let frames = transitioning_chapter(SegmentId::new(), SceneId::Ceremony, 20);
        let first: Vec<f32> = frames[..20].iter().filter_map(|f| f.cct_k).collect();
        let second: Vec<f32> = frames[20..].iter().filter_map(|f| f.cct_k).collect();
        assert!(crate::stats::mean_abs_deviation(&first) < 30.0);
        assert!(crate::stats::mean_abs_deviation(&second) < 30.0);
        let gap =
            (crate::stats::median(&second).unwrap() - crate::stats::median(&first).unwrap()).abs();
        assert!(gap > 2_000.0, "the transition is {gap} K");
    }

    #[test]
    fn the_wedding_has_four_chapters_in_capture_order() {
        let frames = wedding();
        let segments: Vec<SegmentId> = {
            let mut seen: Vec<SegmentId> = Vec::new();
            for frame in &frames {
                if !seen.contains(&frame.segment) {
                    seen.push(frame.segment);
                }
            }
            seen
        };
        assert_eq!(segments.len(), 4);
        assert!(frames.len() > 100);
    }

    #[test]
    fn the_authored_skin_field_answers_for_the_frames_it_was_built_over_and_no_others() {
        let frames = drifting_chapter(SegmentId::new(), SceneId::Ceremony, 10, 100.0);
        let field = AuthoredSkin::new(&frames, [0.24, 0.50], 0.45, 0.01);
        assert_eq!(field.readings(frames[0].image).len(), 1);
        assert!(field.readings(ImageId::new()).is_empty());
        assert!(NoSkinField.readings(frames[0].image).is_empty());
    }

    #[test]
    fn the_same_identity_appears_on_every_authored_frame() {
        let frames = drifting_chapter(SegmentId::new(), SceneId::Ceremony, 8, 100.0);
        let field = AuthoredSkin::new(&frames, [0.24, 0.50], 0.45, 0.01);
        for frame in &frames {
            assert_eq!(
                field.readings(frame.image)[0].identity,
                AuthoredSkin::identity()
            );
        }
    }
}
