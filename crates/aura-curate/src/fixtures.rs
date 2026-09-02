//! Synthetic weddings, and the field that serves them.
//!
//! **Everything in this module is authored.** Section 9's DATA row asks for sixty real album
//! sequences, hero sets and B&W selections collected with permission, and this repository has none.
//! So every gate in `tests/eval/curate_eval.rs` and every check in the phase 29 gate is measured
//! against galleries whose right answer this file put there.
//!
//! That proves the arithmetic, the constraints, the refusals, the ordering and the store. It is not
//! evidence that a photographer would agree with a hero, a sequence or a monochrome suggestion, and
//! the exit report says so as condition C1.
//!
//! # What "the right answer is known by construction" means here
//!
//! [`wedding`] builds a gallery whose chapters, moments, scenes, scores and descriptors were chosen
//! rather than measured, so a test can assert things like "the frame carrying `MustHave::Rings` is
//! in the album" without inspecting the album's internals. [`planted`] goes further: it plants a
//! known set of frames as the ones a photographer would pick, so the agreement gates have something
//! to be measured against - and that number is a measurement of this file, not of a photographer.

use std::collections::BTreeMap;
use std::sync::Mutex;

use aura_core::contract::cull::{Coverage, CoverageReport, MustHave};
use aura_core::contract::curate::{AspectVariant, ImageId};
use aura_core::contract::ids::{IdentityId, MomentId};
use aura_core::contract::scene::{ChapterId, SceneId};
use aura_core::{AuraResult, ProjectId};
use aura_index::contract::index::LumaStats;

use crate::read::{Descriptor, FaceRead, Field, Frame};

/// The shape of one synthetic wedding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    /// How many frames the gallery holds.
    pub frames: u32,
    /// Whether phase 06 found faces. False reproduces this build; true exercises the mechanisms
    /// that face detection unlocks - shot scale, facing, and the skin rule.
    pub faces: bool,
    /// Whether phase 15 measured a skin locus for the identities in it.
    pub loci: bool,
    /// Whether phase 05's descriptors are present.
    pub descriptors: bool,
}

impl Shape {
    /// This build: a gallery with readings but no faces and no loci.
    ///
    /// The honest default, and the one the phase gate runs on. Phase 06's detector finds no faces,
    /// so phase 15 has no loci, so shot scale is `Unknown` and facing is `Unknown` on every frame.
    #[must_use]
    pub const fn as_shipped(frames: u32) -> Self {
        Self {
            frames,
            faces: false,
            loci: false,
            descriptors: true,
        }
    }

    /// A gallery with everything measured. What the mechanisms are tested against.
    #[must_use]
    pub const fn complete(frames: u32) -> Self {
        Self {
            frames,
            faces: true,
            loci: true,
            descriptors: true,
        }
    }
}

/// One synthetic wedding, and everything a pass needs to know about it.
#[derive(Debug, Clone)]
pub struct Wedding {
    /// The project.
    pub project: ProjectId,
    /// The gallery, in timeline order.
    pub frames: Vec<Frame>,
    /// The identities in it.
    pub identities: Vec<IdentityId>,
    /// Which band each identity's measured skin locus falls in.
    pub loci: BTreeMap<IdentityId, u8>,
    /// Phase 12's own coverage report over the gallery.
    pub coverage: CoverageReport,
    /// The rituals phase 07 named.
    pub rituals: Vec<String>,
}

/// The chapters of a synthetic wedding, with how much of the day each takes and what is in it.
const PLAN: [(ChapterId, SceneId, Option<MustHave>, f32); 10] = [
    (
        ChapterId::GettingReady,
        SceneId::GettingReadyBride,
        None,
        0.10,
    ),
    (ChapterId::Details, SceneId::Details, None, 0.06),
    (
        ChapterId::Ceremony,
        SceneId::CeremonyEntrance,
        Some(MustHave::CeremonyEntrance),
        0.08,
    ),
    (
        ChapterId::Ceremony,
        SceneId::Vows,
        Some(MustHave::Vows),
        0.08,
    ),
    (
        ChapterId::Ceremony,
        SceneId::Rings,
        Some(MustHave::Rings),
        0.04,
    ),
    (
        ChapterId::Ceremony,
        SceneId::Kiss,
        Some(MustHave::Kiss),
        0.04,
    ),
    (
        ChapterId::Portraits,
        SceneId::FamilyPortrait,
        Some(MustHave::FamilyFormals),
        0.14,
    ),
    (ChapterId::Reception, SceneId::Speeches, None, 0.18),
    (
        ChapterId::Dance,
        SceneId::FirstDance,
        Some(MustHave::FirstDance),
        0.20,
    ),
    (ChapterId::Exit, SceneId::Exit, Some(MustHave::Exit), 0.08),
];

/// Build one synthetic wedding.
///
/// Deterministic in `seed`: the same seed produces the same gallery, so a gate that fails can be
/// reproduced exactly. No random number generator - the scores are a small integer hash of the
/// frame's index, which is reproducible on every platform in a way a seeded PRNG is not guaranteed
/// to be across versions of its crate.
#[must_use]
pub fn wedding(shape: Shape, seed: u64) -> Wedding {
    let project = ProjectId::new();
    let identities: Vec<IdentityId> = (0..4).map(|_| IdentityId::new()).collect();
    let mut frames = Vec::with_capacity(shape.frames as usize);
    let mut satisfied: Vec<MustHave> = Vec::new();

    let mut order = 0u32;
    for (chapter, scene, rule, share) in PLAN {
        let count = ((shape.frames as f32) * share).round().max(1.0) as u32;
        let moment = MomentId::new();
        for ix in 0..count {
            let noise = mix(seed, u64::from(order));
            let mut frame = Frame::bare(ImageId::new(), order);
            frame.scene = Some(scene);
            frame.chapter = Some(chapter);
            // Every eighth frame starts a new moment, so a chapter has several.
            frame.moment = Some(if ix.is_multiple_of(8) {
                MomentId::new()
            } else {
                moment
            });
            frame.keep_score = unit(noise, 0);
            frame.technical = Some(0.55 + 0.45 * unit(noise, 1));
            frame.emotion = Some(unit(noise, 2));
            frame.composition = Some(unit(noise, 3));
            frame.narrative = Some(unit(noise, 4));
            frame.interaction = Some(unit(noise, 5));
            frame.noise_sigma_rel = Some(0.6 * unit(noise, 6));
            frame.subject_sharpness = Some(0.4 + 0.6 * unit(noise, 7));
            frame.negative_space = Some(unit(noise, 8));
            frame.clutter = Some(unit(noise, 9));
            frame.warmth_k = Some(3200.0 + 3000.0 * unit(noise, 10));
            if let Some(rule) = rule {
                frame.satisfies = vec![rule];
                if !satisfied.contains(&rule) {
                    satisfied.push(rule);
                }
            }
            // Two identities per frame, rotating, so close-family coverage has something to work on.
            let a = identities.get((order as usize) % identities.len()).copied();
            let b = identities
                .get((order as usize + 1) % identities.len())
                .copied();
            frame.identities = [a, b].into_iter().flatten().collect();

            if shape.faces {
                // A face whose size and eye offset vary with the frame, so shot scale and facing are
                // both measurable and both vary.
                let area = 0.004 + 0.10 * unit(noise, 11);
                let offset = (unit(noise, 12) - 0.5) * 0.2;
                frame.faces = vec![FaceRead {
                    identity: a,
                    area_frac: area,
                    centre_x: 0.5,
                    width: 0.2,
                    eye_mid_x: Some(0.5 + offset),
                }];
            }
            if shape.descriptors {
                frame.descriptor = Some(descriptor(noise));
            }
            // Every third frame has a square crop; every fifth has a 4:5.
            frame.aspects = vec![AspectVariant::Original];
            if order.is_multiple_of(3) {
                frame.aspects.push(AspectVariant::Square);
            }
            if order.is_multiple_of(5) {
                frame.aspects.push(AspectVariant::FourFive);
            }
            frames.push(frame);
            order += 1;
        }
    }

    let loci = if shape.loci {
        identities
            .iter()
            .enumerate()
            .map(|(ix, id)| (*id, ((ix + 1) % 8) as u8))
            .collect()
    } else {
        BTreeMap::new()
    };

    let coverage = CoverageReport {
        must_haves: MustHave::ALL
            .iter()
            .map(|rule| {
                let state = if satisfied.contains(rule) {
                    Coverage::Covered
                } else {
                    Coverage::Missing
                };
                (*rule, state)
            })
            .collect(),
        identity_coverage: identities.iter().map(|id| (*id, 20)).collect(),
        chapter_counts: Vec::new(),
        warnings: Vec::new(),
    };

    Wedding {
        project,
        frames,
        identities,
        loci,
        coverage,
        rituals: vec!["saptapadi".to_string(), "mangalsutra".to_string()],
    }
}

/// A wedding with a known set of frames planted as the ones a photographer would pick.
///
/// The planted frames are given the strongest readings in the gallery, so a correct selector finds
/// them. **This measures the selector against this file's own opinion**, which is the only kind of
/// agreement gate a repository with no consented archive can run - and it is not the study section
/// 10.1 asks for. `docs/progress/PHASE-29-EXIT.md` condition C1.
#[must_use]
pub fn planted(shape: Shape, seed: u64, count: usize) -> (Wedding, Vec<ImageId>) {
    let mut wedding = wedding(shape, seed);
    let mut planted = Vec::new();
    // Spread the plants across chapters, so a correct selector has to satisfy the diversity
    // constraints to find them all.
    let stride = (wedding.frames.len() / count.max(1)).max(1);
    for ix in 0..count {
        let position = ix * stride;
        let Some(frame) = wedding.frames.get_mut(position) else {
            break;
        };
        frame.technical = Some(0.98);
        frame.emotion = Some(0.99);
        frame.composition = Some(0.97);
        frame.narrative = Some(0.95);
        frame.keep_score = 0.98;
        // Its own moment, so the moment constraint does not exclude it.
        frame.moment = Some(MomentId::new());
        planted.push(frame.image_id);
    }
    (wedding, planted)
}

/// A descriptor whose colour and tone vary with the frame.
fn descriptor(noise: u64) -> Descriptor {
    let mut hist = vec![0u8; 512];
    let hue = (noise % 8) as usize;
    let sat = 4 + ((noise >> 3) % 4) as usize;
    let dark = ((noise >> 6) % 3) as usize;
    let light = 5 + ((noise >> 8) % 3) as usize;
    for (bin, weight) in [(dark, 255u8), (light, 200u8)] {
        let ix = hue * 64 + sat * 8 + bin;
        if let Some(slot) = hist.get_mut(ix) {
            *slot = weight;
        }
    }
    let mean = 0.25 + 0.5 * unit(noise, 13);
    Descriptor {
        hsv_hist: hist,
        luma: LumaStats {
            mean,
            p1: 0.02,
            p50: mean,
            p99: 0.97,
            clip_lo: 0.0,
            clip_hi: 0.0,
        },
        edge_energy: 0.1 + 0.3 * unit(noise, 14),
    }
}

/// A small integer hash. Deterministic on every platform, unlike a seeded PRNG across crate
/// versions - invariant 4 is about the product and a fixture that drifts is a gate that drifts.
fn mix(seed: u64, index: u64) -> u64 {
    let mut x = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(index.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// One `0..1` value from a hash, at a given bit offset.
fn unit(noise: u64, slot: u32) -> f32 {
    let bits = (noise >> (slot % 48)) & 0xFFFF;
    bits as f32 / 65535.0
}

/// A [`Field`] over one synthetic wedding.
///
/// Similarity is a deterministic function of the two ids rather than a stored table, so a gallery of
/// any size has a similarity for every pair without a fixture having to enumerate them. Frames in
/// the same moment are made deliberately alike, which is what gives the near-duplicate constraint
/// something to refuse.
#[derive(Debug)]
pub struct FixtureField {
    wedding: Wedding,
    /// When true, every similarity is `None`: the uniqueness term is unmeasurable, which is what a
    /// project with no phase 05 index looks like.
    blind: bool,
    /// How many similarity calls have been made. The gate reads it to check the pass does not scale
    /// quadratically with the gallery.
    calls: Mutex<u64>,
}

impl FixtureField {
    /// A field over one wedding.
    #[must_use]
    pub fn new(wedding: Wedding) -> Self {
        Self {
            wedding,
            blind: false,
            calls: Mutex::new(0),
        }
    }

    /// A field that cannot measure similarity at all.
    #[must_use]
    pub fn blind(wedding: Wedding) -> Self {
        Self {
            wedding,
            blind: true,
            calls: Mutex::new(0),
        }
    }

    /// The wedding underneath.
    #[must_use]
    pub const fn wedding(&self) -> &Wedding {
        &self.wedding
    }

    /// How many similarity readings have been asked for.
    #[must_use]
    pub fn calls(&self) -> u64 {
        self.calls.lock().map_or(0, |c| *c)
    }
}

impl Field for FixtureField {
    fn frames(&self, _project: ProjectId) -> AuraResult<Vec<Frame>> {
        Ok(self.wedding.frames.clone())
    }

    fn photo_count(&self, _project: ProjectId) -> AuraResult<u32> {
        // A gallery is a selection: the project holds more photographs than the gallery does.
        Ok(self.wedding.frames.len() as u32 * 3)
    }

    fn gallery_coverage(&self, _project: ProjectId) -> AuraResult<CoverageReport> {
        Ok(self.wedding.coverage.clone())
    }

    fn skin_bands(&self, _project: ProjectId) -> AuraResult<BTreeMap<IdentityId, u8>> {
        Ok(self.wedding.loci.clone())
    }

    fn similarity(&self, from: ImageId, others: &[ImageId]) -> Vec<Option<f32>> {
        if let Ok(mut calls) = self.calls.lock() {
            *calls += others.len() as u64;
        }
        if self.blind {
            return vec![None; others.len()];
        }
        let moment_of = |id: ImageId| -> Option<MomentId> {
            self.wedding
                .frames
                .iter()
                .find(|f| f.image_id == id)
                .and_then(|f| f.moment)
        };
        let from_moment = moment_of(from);
        others
            .iter()
            .map(|other| {
                if from == *other {
                    return Some(1.0);
                }
                // Two frames of one shot look alike; anything else is a stable pseudo-distance.
                if from_moment.is_some() && from_moment == moment_of(*other) {
                    return Some(0.95);
                }
                // Combined commutatively, because a similarity that disagrees with itself when the
                // arguments are swapped is not a distance - and the near-duplicate constraint asks
                // it both ways, once when the pair is laid out and once when a swap is considered.
                // The first version passed the two folds straight into `mix`, which is not
                // symmetric in its arguments, and `similarity_is_symmetric` is what found it.
                let (a, b) = (fold(&from.to_db()), fold(&other.to_db()));
                let noise = mix(a.wrapping_add(b), a ^ b);
                Some(0.1 + 0.7 * unit(noise, 0))
            })
            .collect()
    }

    fn rituals(&self, _project: ProjectId) -> AuraResult<Vec<String>> {
        Ok(self.wedding.rituals.clone())
    }

    fn close_family(&self, _project: ProjectId) -> AuraResult<(Vec<IdentityId>, u32)> {
        Ok((self.wedding.identities.clone(), 2))
    }
}

/// Fold a string into a hash seed, symmetrically enough that `similarity(a, b)` and
/// `similarity(b, a)` agree.
fn fold(text: &str) -> u64 {
    text.bytes()
        .fold(0u64, |acc, b| acc.wrapping_add(u64::from(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_produces_the_same_wedding() {
        let a = wedding(Shape::as_shipped(200), 7);
        let b = wedding(Shape::as_shipped(200), 7);
        assert_eq!(a.frames.len(), b.frames.len());
        for (x, y) in a.frames.iter().zip(&b.frames) {
            assert_eq!(x.keep_score, y.keep_score);
            assert_eq!(x.technical, y.technical);
            assert_eq!(x.chapter, y.chapter);
        }
    }

    #[test]
    fn the_as_shipped_shape_reproduces_this_build() {
        let w = wedding(Shape::as_shipped(120), 1);
        assert!(w.loci.is_empty(), "no phase 15 loci");
        for frame in &w.frames {
            assert!(frame.faces.is_empty(), "no phase 06 faces");
            assert_eq!(frame.facing(), crate::read::Facing::Unknown);
        }
        // Which makes shot scale unmeasurable on everything except the two scenes whose scale is
        // known by definition.
        let measured = w.frames.iter().filter(|f| f.scale().is_known()).count();
        assert!(
            measured < w.frames.len() / 2,
            "{measured} of {} frames measurable",
            w.frames.len()
        );
    }

    #[test]
    fn the_complete_shape_makes_the_mechanisms_reachable() {
        let w = wedding(Shape::complete(120), 1);
        assert_eq!(w.loci.len(), w.identities.len());
        assert!(w.frames.iter().all(|f| !f.faces.is_empty()));
        let facing = w.frames.iter().filter(|f| f.facing().is_known()).count();
        assert_eq!(facing, w.frames.len());
    }

    #[test]
    fn the_gallery_covers_the_rules_the_plan_puts_in_it() {
        let w = wedding(Shape::as_shipped(200), 3);
        let covered: Vec<MustHave> = w
            .coverage
            .must_haves
            .iter()
            .filter(|(_, s)| s.is_satisfied())
            .map(|(r, _)| *r)
            .collect();
        assert!(covered.contains(&MustHave::Rings));
        assert!(covered.contains(&MustHave::Kiss));
        // And honestly misses the ones nobody shot.
        assert!(!covered.contains(&MustHave::Cake));
    }

    #[test]
    fn similarity_is_symmetric_and_frames_of_one_shot_are_alike() {
        let w = wedding(Shape::as_shipped(80), 5);
        let field = FixtureField::new(w.clone());
        // Frames 1 and 2 share a moment; frame 0 starts one of its own.
        let a = w.frames[1].image_id;
        let b = w.frames[2].image_id;
        let ab = field.similarity(a, &[b])[0];
        let ba = field.similarity(b, &[a])[0];
        assert_eq!(ab, ba, "an asymmetric similarity is not a distance");
        assert!(ab.unwrap() > 0.9, "two frames of one shot look alike");

        // And a pair from different moments is symmetric too, which is the case the first
        // implementation got wrong.
        let far = w.frames[40].image_id;
        assert_eq!(
            field.similarity(a, &[far])[0],
            field.similarity(far, &[a])[0]
        );
    }

    #[test]
    fn a_blind_field_reports_nothing_rather_than_zero() {
        let w = wedding(Shape::as_shipped(20), 2);
        let field = FixtureField::blind(w.clone());
        let readings = field.similarity(w.frames[0].image_id, &[w.frames[5].image_id]);
        assert_eq!(readings, vec![None]);
    }

    #[test]
    fn planted_frames_are_the_strongest_in_the_gallery() {
        let (w, planted) = planted(Shape::as_shipped(200), 11, 20);
        assert_eq!(planted.len(), 20);
        for id in &planted {
            let frame = w
                .frames
                .iter()
                .find(|f| f.image_id == *id)
                .expect("planted");
            assert!(frame.technical.unwrap() > 0.9);
            assert!(frame.emotion.unwrap() > 0.9);
        }
    }
}
