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
use aura_core::contract::curate::{AspectVariant, ImageId, MAX_HEROES_PER_CHAPTER};
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
    let project = project_id(seed);
    let identities: Vec<IdentityId> = (0..4).map(|ix| identity_id(seed, ix)).collect();
    let mut frames = Vec::with_capacity(shape.frames as usize);
    let mut satisfied: Vec<MustHave> = Vec::new();

    let mut order = 0u32;
    for (chapter, scene, rule, share) in PLAN {
        let count = ((shape.frames as f32) * share).round().max(1.0) as u32;
        let moment = moment_id(seed, u64::from(order) | 0x1000_0000);
        for ix in 0..count {
            let noise = mix(seed, u64::from(order));
            let mut frame = Frame::bare(image_id(seed, u64::from(order)), order);
            frame.scene = Some(scene);
            frame.chapter = Some(chapter);
            // Every eighth frame starts a new moment, so a chapter has several.
            frame.moment = Some(if ix.is_multiple_of(8) {
                moment_id(seed, u64::from(order))
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
                frame.descriptor = Some(descriptor(noise, chapter_base_luma(chapter)));
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

    // Plants are allocated across the chapters in proportion to how much of the day each one is,
    // capped below the hero quota, and placed at an even stride *inside* each chapter.
    //
    // Two corrections, both forced by the hero agreement gate, and both the same lesson in different
    // clothes - phase 25's, for the sixth and seventh time in this repository.
    //
    // The first version planted at a fixed stride through the whole gallery, which follows chapter
    // size and therefore put exactly `MAX_HEROES_PER_CHAPTER` plants into each of the three longest
    // chapters. A plant set that fills a quota is one where a single ordinary frame winning a single
    // round costs a plant permanently, and the gate measures the tie rather than the selector.
    //
    // The second was an even round robin, which fixed the quota and broke the spacing: three plants
    // in the shortest chapter sit close enough together to be near-duplicates of each other, and
    // uniqueness is eighteen per cent of the portfolio blend. A photographer's own top twenty is not
    // three frames from one corner of a short chapter; it is spread in proportion to where the day
    // actually was.
    let mut by_chapter: BTreeMap<ChapterId, Vec<usize>> = BTreeMap::new();
    for (ix, frame) in wedding.frames.iter().enumerate() {
        by_chapter
            .entry(frame.chapter_or_other())
            .or_default()
            .push(ix);
    }
    let cap = (MAX_HEROES_PER_CHAPTER.saturating_sub(1)).max(1) as usize;
    let total: usize = by_chapter.values().map(Vec::len).sum();
    let mut quota: BTreeMap<ChapterId, usize> = BTreeMap::new();
    for (chapter, frames_in) in &by_chapter {
        let proportional = frames_in.len() * count / total.max(1);
        quota.insert(*chapter, proportional.min(cap).min(frames_in.len()));
    }
    // Hand the remainder to whichever chapter is furthest below its cap, longest first, so an
    // allocation that rounded down does not silently plant fewer than asked for.
    while quota.values().sum::<usize>() < count {
        let mut best: Option<ChapterId> = None;
        let mut best_room = 0usize;
        for (chapter, frames_in) in &by_chapter {
            let taken = quota.get(chapter).copied().unwrap_or_default();
            let room = cap.min(frames_in.len()).saturating_sub(taken);
            if room > best_room {
                best_room = room;
                best = Some(*chapter);
            }
        }
        let Some(chapter) = best else { break };
        *quota.entry(chapter).or_default() += 1;
    }

    let mut positions: Vec<usize> = Vec::new();
    for (chapter, frames_in) in &by_chapter {
        let take = quota.get(chapter).copied().unwrap_or_default();
        if take == 0 {
            continue;
        }
        // Centred strides: `len/2n`, `3len/2n`, ... so the plants are as far from each other and
        // from the chapter's edges as the chapter allows.
        let span = frames_in.len();
        for slot in 0..take {
            let offset = (2 * slot + 1) * span / (2 * take);
            if let Some(position) = frames_in.get(offset.min(span.saturating_sub(1))) {
                positions.push(*position);
            }
        }
    }
    positions.sort_unstable();
    positions.truncate(count);

    for position in positions {
        let Some(frame) = wedding.frames.get_mut(position) else {
            break;
        };
        frame.technical = Some(0.98);
        frame.emotion = Some(0.99);
        frame.composition = Some(0.97);
        frame.narrative = Some(0.95);
        frame.keep_score = 0.98;
        // Its own moment, so the moment constraint does not exclude it.
        frame.moment = Some(moment_id(seed, 0x8000_0000 | position as u64));
        planted.push(frame.image_id);
    }
    (wedding, planted)
}

/// The mean luminance a chapter sits around, after phase 25 has normalised the gallery.
///
/// Per chapter rather than per frame, and this is a correction the phase gate forced. The first
/// fixture drew each frame's mean luminance uniformly across the whole range, which models a gallery
/// **nobody normalised** - and no curation pass ever sees one of those, because phase 25 runs first.
/// The consequence was that half of every candidate pairing exceeded the tonal ceiling and the gate
/// reported an album of single pages.
///
/// Phase 25's lesson, restated: when a gate cannot be met, work out whether the fixture, the
/// threshold or the code is the thing that does not match reality. Here it was the fixture. A real
/// wedding varies enormously in brightness **between** a dark church and a sunset portrait and
/// modestly **within** either - and spreads only ever pair inside one chapter.
const fn chapter_base_luma(chapter: ChapterId) -> f32 {
    match chapter {
        ChapterId::GettingReady => 0.62,
        ChapterId::Details => 0.54,
        ChapterId::Ceremony => 0.34,
        ChapterId::Rituals => 0.36,
        ChapterId::Portraits => 0.66,
        ChapterId::Reception => 0.42,
        ChapterId::Dance => 0.22,
        ChapterId::Exit => 0.28,
        ChapterId::Other => 0.45,
    }
}

/// A descriptor whose colour and tone vary with the frame.
///
/// `base` is the chapter's own tone; a frame varies around it by at most a tenth of the range, which
/// is roughly what phase 25 leaves behind inside one lighting node.
fn descriptor(noise: u64, base: f32) -> Descriptor {
    let mut hist = vec![0u8; 512];
    // Four hues rather than one. The first fixture put a frame's whole population in a single hue
    // bin, and the monochrome gate is what found it: a one-hue frame has **nothing to separate**,
    // so every mix and every preset scored zero on it and three quarters of the gallery measured
    // nothing at all. A wedding photograph is skin, fabric, foliage and sky in one frame - which is
    // the entire reason a per-band mix exists - and a fixture with one hue in it is a fixture that
    // cannot tell a working solver from a solver that returns neutral.
    //
    // The four are spread around the wheel rather than adjacent, because two neighbouring hue bins
    // land in overlapping bands and would leave the same gap.
    let first = (noise % 8) as usize;
    let hues = [first, (first + 2) % 8, (first + 4) % 8, (first + 5) % 8];
    // Descending weights, so a frame has a dominant colour and three lesser ones rather than four
    // equal quarters - which is what `hue_carried` and `dominant` are written against.
    let weights = [255u8, 170, 110, 60];
    let sat = 4 + ((noise >> 3) % 4) as usize;
    let dark = ((noise >> 6) % 3) as usize;
    let light = 5 + ((noise >> 8) % 3) as usize;
    for (slot_ix, hue) in hues.into_iter().enumerate() {
        let Some(top) = weights.get(slot_ix) else {
            continue;
        };
        // Each hue sits at its own brightness, so the bands are separable in principle and a mix
        // has something to do. The two bins per hue are its shadow and its highlight.
        let step = ((noise >> (16 + slot_ix * 3)) % 3) as usize;
        for (bin, share) in [
            (dark.saturating_add(step).min(7), *top),
            (light.saturating_sub(step).max(dark), top / 2 + 20),
        ] {
            let ix = hue * 64 + sat * 8 + bin;
            if let Some(slot) = hist.get_mut(ix) {
                *slot = slot.saturating_add(share);
            }
        }
    }
    let mean = (base + 0.20f32.mul_add(unit(noise, 13), -0.10)).clamp(0.05, 0.95);
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
                // Otherwise it falls off with how far apart in the day the two frames were shot,
                // plus a small stable jitter.
                //
                // The first version was a hash of the two ids and nothing else, which makes
                // similarity **independent of everything else about a frame** - and the hero gate is
                // what found it. Uniqueness is eighteen per cent of the portfolio blend, so a
                // planted frame that drew an unlucky hash lost its place to an ordinary frame that
                // drew a lucky one, and the agreement gate read 0.60 while measuring a coin toss.
                //
                // A wedding does not work like that. Two frames from the same minute look alike
                // whether or not the grouper put them in one moment, and a frame from the vows and a
                // frame from the first dance do not - which is exactly why a photographer's own top
                // twenty is spread across the day. Modelling that makes the uniqueness term mean
                // what the product says it means, and it gives the album's near-duplicate refusal
                // consecutive frames to refuse rather than an arbitrary scattering.
                //
                // Combined commutatively, because a similarity that disagrees with itself when the
                // arguments are swapped is not a distance - and the near-duplicate constraint asks
                // it both ways, once when the pair is laid out and once when a swap is considered.
                // The first version passed the two folds straight into `mix`, which is not
                // symmetric in its arguments, and `similarity_is_symmetric` is what found it.
                let order_of = |id: ImageId| -> Option<u32> {
                    self.wedding
                        .frames
                        .iter()
                        .find(|f| f.image_id == id)
                        .map(|f| f.order)
                };
                let (Some(x), Some(y)) = (order_of(from), order_of(*other)) else {
                    return None;
                };
                let gap = (f64::from(x) - f64::from(y)).abs() as f32;
                let (a, b) = (fold(&from.to_db()), fold(&other.to_db()));
                let noise = mix(a.wrapping_add(b), a ^ b);
                let falloff = 0.82 * (-gap / 45.0).exp();
                Some((0.04 + falloff + 0.10 * unit(noise, 0)).clamp(0.0, 0.99))
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

/// A deterministic id of the same kind as `sample`, keyed on the fixture's seed.
///
/// **The fixture minted random ids until the hero gate caught it.** `ImageId::new()` is a v7 UUID,
/// which is time-ordered in its high bits and *random* in its low ones - so two runs of the same
/// seed produced galleries that agreed about every score and disagreed about every identifier. That
/// is invisible until something reads an id: the similarity jitter does, and so does every
/// tie-break in this crate that falls back on `image_id`. The agreement gate moved by fifteen
/// points between two runs of an unchanged build, which is the worst kind of red line - one nobody
/// can reproduce.
///
/// `the_same_seed_produces_the_same_wedding` had been passing throughout, because it compared
/// scores and chapters rather than identifiers. It compares ids now.
///
/// The index goes in the high bytes so the ids still sort in gallery order, which is the one
/// property of a v7 UUID this crate relies on.
fn id_text(sample: &str, seed: u64, tag: u64, index: u64) -> String {
    let prefix = sample.split('_').next().unwrap_or("id");
    let hi = mix(seed ^ tag, index);
    let lo = mix(hi, index.wrapping_add(tag));
    format!(
        "{prefix}_{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        index as u32,
        ((hi >> 48) & 0xFFFF) as u16,
        ((hi >> 32) & 0xFFFF) as u16,
        ((hi >> 16) & 0xFFFF) as u16,
        lo & 0xFFFF_FFFF_FFFF
    )
}

/// The `index`-th image of the wedding `seed` describes.
fn image_id(seed: u64, index: u64) -> ImageId {
    ImageId::from_db(&id_text(&ImageId::new().to_db(), seed, 0x1_1111, index))
        .unwrap_or_else(|_| ImageId::new())
}

/// The `index`-th moment of the wedding `seed` describes.
fn moment_id(seed: u64, index: u64) -> MomentId {
    MomentId::from_db(&id_text(&MomentId::new().to_db(), seed, 0x2_2222, index))
        .unwrap_or_else(|_| MomentId::new())
}

/// The `index`-th person in the wedding `seed` describes.
fn identity_id(seed: u64, index: u64) -> IdentityId {
    IdentityId::from_db(&id_text(&IdentityId::new().to_db(), seed, 0x3_3333, index))
        .unwrap_or_else(|_| IdentityId::new())
}

/// The project the wedding `seed` describes.
fn project_id(seed: u64) -> ProjectId {
    ProjectId::from_db(&id_text(&ProjectId::new().to_db(), seed, 0x4_4444, 0))
        .unwrap_or_else(|_| ProjectId::new())
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
    fn a_chapter_is_tonally_consistent_and_two_chapters_are_not() {
        // The correction the phase gate forced: a curation pass only ever sees a gallery phase 25
        // has normalised, and pairs only ever form inside one chapter.
        let w = wedding(Shape::as_shipped(400), 4);
        let mut by_chapter: BTreeMap<ChapterId, Vec<f32>> = BTreeMap::new();
        for frame in &w.frames {
            if let Some(luma) = frame.tonal_weight() {
                by_chapter
                    .entry(frame.chapter_or_other())
                    .or_default()
                    .push(luma);
            }
        }
        let mut means = Vec::new();
        for (chapter, lumas) in &by_chapter {
            let lo = lumas.iter().copied().fold(f32::MAX, f32::min);
            let hi = lumas.iter().copied().fold(f32::MIN, f32::max);
            assert!(
                hi - lo <= aura_core::contract::curate::MAX_PAIR_TONAL_GAP,
                "{chapter:?} spans {:.2}, which is wider than a normalised chapter",
                hi - lo
            );
            let sum: f32 = lumas.iter().copied().sum();
            means.push(sum / lumas.len() as f32);
        }
        let lo = means.iter().copied().fold(f32::MAX, f32::min);
        let hi = means.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            hi - lo > aura_core::contract::curate::MAX_PAIR_TONAL_GAP,
            "the chapters are all the same brightness ({lo:.2} to {hi:.2}), which no wedding is"
        );
    }

    #[test]
    fn the_same_seed_produces_the_same_wedding() {
        let a = wedding(Shape::as_shipped(200), 7);
        let b = wedding(Shape::as_shipped(200), 7);
        assert_eq!(a.frames.len(), b.frames.len());
        for (x, y) in a.frames.iter().zip(&b.frames) {
            // The identifiers as well as the scores. Comparing only the scores is what let a
            // fixture that minted random ids look deterministic for four phases of this crate's
            // development - see `id_text`.
            assert_eq!(x.image_id, y.image_id);
            assert_eq!(x.moment, y.moment);
            assert_eq!(x.keep_score, y.keep_score);
            assert_eq!(x.technical, y.technical);
            assert_eq!(x.chapter, y.chapter);
        }
        assert_eq!(a.project, b.project, "the project id is not reproducible");
        assert_eq!(
            a.identities, b.identities,
            "the people are not reproducible"
        );
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
