//! What curation is handed about one photograph, and the port it arrives through.
//!
//! # Why a port rather than a dependency
//!
//! `aura-curate` depends on none of the deciding crates. Everything it knows about a photograph
//! comes through [`Field`], which `aura-app` implements out of the frozen services in `aura-core`.
//!
//! Phase 27 built the same indirection to stop `aura-brain-photo` depending on the crate that judges
//! it. The reason here is different and sharper: **`aura-cull` decides what is in the gallery, and
//! this crate curates what `aura-cull` chose.** If the two could see each other, the first person to
//! want "heroes should influence the cull" would have a straight line to a cycle in which the
//! gallery depends on the portfolio that depends on the gallery.
//!
//! # Why every reading is an `Option`
//!
//! Because on this build most of them are absent, and a selector handed a frame with no emotion
//! reading must **skip that term** rather than score it at zero. Phase 06's detector finds no faces,
//! so `largest_face`, `facing` and `identities` are empty almost everywhere; phase 10's expression
//! head is untrained; phase 15 has no skin loci to hand out. Every one of those produces a *narrower*
//! score with a lower confidence and a reason naming what was missing, never a confident zero.
//!
//! Phase 24 wrote the rule this follows: an absent input is ignorance, not permission.

use std::collections::BTreeMap;
use std::fmt;

use aura_core::contract::cull::{CoverageReport, MustHave};
use aura_core::contract::curate::{AspectVariant, ImageId, ShotScale};
use aura_core::contract::ids::{IdentityId, MomentId};
use aura_core::contract::scene::{ChapterId, SceneId};
use aura_core::{AuraResult, ProjectId};
use aura_index::contract::index::LumaStats;

/// Which way a frame's subjects are looking, in the reader's frame of reference.
///
/// **Measured, never predicted.** Phase 06 stores each face's box and its two eye centres; the
/// horizontal offset of the eye midpoint inside the box is a direct reading of head yaw, because a
/// head turned toward the viewer's right hides its far cheek and carries the eyes toward that side
/// of the visible head. Phase 10 measured gaze the same way rather than asking a model, and phase
/// 15's rule - ask the room, not the winner - is the same instinct.
///
/// [`Facing::Unknown`] is the honest and, on this build, universal answer: without eye landmarks
/// there is nothing to measure, and a spread whose facing is unknown scores zero on that term with
/// [`crate::spread::SpreadPair::facing_known`] false beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Facing {
    /// The subjects look toward the viewer's left.
    Left,
    /// The subjects look toward the viewer's right.
    Right,
    /// The subjects look at the camera, or straight past it.
    Frontal,
    /// Nothing stored could say. The default, because it claims the least.
    #[default]
    Unknown,
}

impl Facing {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Frontal => "frontal",
            Self::Unknown => "unknown",
        }
    }

    /// True when anything was measured at all.
    #[must_use]
    pub const fn is_known(self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// Measure a frame's facing from phase 06's face boxes and eye centres.
    ///
    /// `faces` is `(box centre x, box width, eye midpoint x, area fraction)`, all normalised to the
    /// frame. Faces with no landmarks are skipped rather than read as centred - phase 06 stores
    /// `[[0,0],[0,0]]` for "unknown", and a caller that averaged that in would be measuring the
    /// frame's top-left corner.
    ///
    /// Weighted by area, because a spread is about where the *subject* is looking and the guest in
    /// the background is not the subject.
    #[must_use]
    pub fn measure(faces: &[(f32, f32, f32, f32)]) -> Self {
        let mut weight = 0.0f32;
        let mut sum = 0.0f32;
        for (centre, width, eye_x, area) in faces {
            if *width <= f32::EPSILON || *area <= 0.0 {
                continue;
            }
            // Normalised offset of the eye midpoint inside the face box: -0.5 hard left, +0.5 hard
            // right. Clamped because a detector can put a landmark just outside its own box.
            let offset = ((eye_x - centre) / width).clamp(-0.5, 0.5);
            sum += offset * area;
            weight += area;
        }
        if weight <= f32::EPSILON {
            return Self::Unknown;
        }
        let mean = sum / weight;
        // A tenth of the face's width. Below that a head is frontal for the purposes of a spread:
        // the term exists to stop a subject looking off the outer edge, and a five-degree turn does
        // not do that.
        if mean > 0.10 {
            Self::Right
        } else if mean < -0.10 {
            Self::Left
        } else {
            Self::Frontal
        }
    }
}

/// One face's contribution to a frame's readings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceRead {
    /// Who it is, when phase 06 assigned it to anybody.
    pub identity: Option<IdentityId>,
    /// Fraction of the frame the face covers, `0..1`.
    pub area_frac: f32,
    /// The horizontal centre of the face box, normalised.
    pub centre_x: f32,
    /// The width of the face box, normalised.
    pub width: f32,
    /// The horizontal midpoint of the two eye centres, normalised. `None` when phase 06 produced
    /// no usable landmarks, which is not the same as a midpoint at zero.
    pub eye_mid_x: Option<f32>,
}

/// Phase 05's stored descriptors for one frame.
///
/// Read, never recomputed. Phase 05's rule: descriptors are computed once, and a phase that
/// recomputes one of them is opening a file that did not need opening.
#[derive(Debug, Clone, PartialEq)]
pub struct Descriptor {
    /// The 8x8x8 HSV histogram, each bin scaled so the fullest is 255.
    pub hsv_hist: Vec<u8>,
    /// Luminance statistics.
    pub luma: LumaStats,
    /// Mean gradient magnitude, `0..1`.
    pub edge_energy: f32,
}

/// Everything curation knows about one photograph in the gallery.
///
/// Assembled by `aura-app` from the frozen services, one struct per **selected** frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// The photograph.
    pub image_id: ImageId,
    /// Its position in the gallery's timeline order, `0` first.
    pub order: u32,
    /// Phase 07's scene, when it has one.
    pub scene: Option<SceneId>,
    /// Phase 07's chapter, when it has one.
    pub chapter: Option<ChapterId>,
    /// Phase 08's moment, when it is in one.
    pub moment: Option<MomentId>,
    /// Phase 12's fused keep score.
    pub keep_score: f32,
    /// Which must-have guarantees this frame can satisfy, from phase 12's own rule table.
    ///
    /// Supplied by the port rather than derived here, because there is no second coverage
    /// vocabulary and no second rule table in this product. Phase 12 owns the mapping from a scene
    /// to a guarantee; this crate does subset arithmetic over the answer.
    pub satisfies: Vec<MustHave>,
    /// Who is in it, from phase 06.
    pub identities: Vec<IdentityId>,
    /// Every face phase 06 found, for the scale and facing measurements.
    pub faces: Vec<FaceRead>,
    /// Phase 09's technical score. **Required for a hero**, because it is the veto.
    pub technical: Option<f32>,
    /// Phase 09's relative noise, for the grain term.
    pub noise_sigma_rel: Option<f32>,
    /// Phase 09's subject sharpness, for the legibility term.
    pub subject_sharpness: Option<f32>,
    /// Phase 10's emotion score.
    pub emotion: Option<f32>,
    /// Phase 10's narrative weight - zero unless a `MomentSignificance` call was made and returned.
    pub narrative: Option<f32>,
    /// The strength of the strongest interaction phase 10 detected, for the gesture term.
    pub interaction: Option<f32>,
    /// Phase 11's composition score.
    pub composition: Option<f32>,
    /// Phase 11's negative space, for the legibility term.
    pub negative_space: Option<f32>,
    /// Phase 11's background clutter, for the legibility term.
    pub clutter: Option<f32>,
    /// Phase 05's descriptors.
    pub descriptor: Option<Descriptor>,
    /// The frame's colour temperature after phase 25's normalisation, in kelvin.
    ///
    /// Phase 15's estimate plus phase 25's delta, resolved by the port. One number rather than two,
    /// because a spread cares what the frame will *look like* and not how it got there.
    pub warmth_k: Option<f32>,
    /// The aspect variants phase 23 found a safe crop for.
    ///
    /// [`AspectVariant::Original`] is always present. Anything else is a crop `GeometryService`
    /// said was safe, and a social set never asks for one that is not here.
    pub aspects: Vec<AspectVariant>,
}

impl Frame {
    /// A frame with nothing measured. What a fixture starts from, and what a photograph nobody
    /// analysed produces.
    #[must_use]
    pub fn bare(image_id: ImageId, order: u32) -> Self {
        Self {
            image_id,
            order,
            scene: None,
            chapter: None,
            moment: None,
            keep_score: 0.5,
            satisfies: Vec::new(),
            identities: Vec::new(),
            faces: Vec::new(),
            technical: None,
            noise_sigma_rel: None,
            subject_sharpness: None,
            emotion: None,
            narrative: None,
            interaction: None,
            composition: None,
            negative_space: None,
            clutter: None,
            descriptor: None,
            warmth_k: None,
            aspects: vec![AspectVariant::Original],
        }
    }

    /// True when anything at all was measured about this frame.
    ///
    /// The `curated` counter's predicate. A frame with no scene, no readings and no descriptors is a
    /// gap in curation's coverage rather than a frame that scored badly, and
    /// `CurationOutline::coverage` is how a caller finds out that an album was drawn over a third of
    /// a gallery.
    #[must_use]
    pub fn is_readable(&self) -> bool {
        self.technical.is_some()
            || self.emotion.is_some()
            || self.composition.is_some()
            || self.descriptor.is_some()
    }

    /// The chapter, or [`ChapterId::Other`] for a frame phase 07 never placed.
    #[must_use]
    pub fn chapter_or_other(&self) -> ChapterId {
        self.chapter.unwrap_or(ChapterId::Other)
    }

    /// The largest face's share of the frame, when there is a face.
    #[must_use]
    pub fn largest_face(&self) -> Option<f32> {
        self.faces
            .iter()
            .map(|f| f.area_frac)
            .fold(None, |best: Option<f32>, area| {
                Some(best.map_or(area, |b| b.max(area)))
            })
    }

    /// Which way the subjects are looking.
    #[must_use]
    pub fn facing(&self) -> Facing {
        let readings: Vec<(f32, f32, f32, f32)> = self
            .faces
            .iter()
            .filter_map(|f| {
                f.eye_mid_x
                    .map(|eye| (f.centre_x, f.width, eye, f.area_frac))
            })
            .collect();
        Facing::measure(&readings)
    }

    /// How close the photographer was, as far as anything stored can say.
    ///
    /// Two sources, in this order: the largest face's share of the frame, and the scene label where
    /// the scene's scale is known **by definition**. Never a guess: a ceremony is not "medium"
    /// because guessing would put a number in the rhythm score that came from a label rather than
    /// from a photograph, and ADR-0059 section 8 records that.
    ///
    /// The two thresholds are the classic portrait distances expressed as area: a head-and-shoulders
    /// frame puts a face over about 6 % of the picture, and a full-length figure in a room puts it
    /// under about 1.5 %.
    #[must_use]
    pub fn scale(&self) -> ShotScale {
        if let Some(area) = self.largest_face() {
            if area >= 0.06 {
                return ShotScale::Tight;
            }
            if area >= 0.015 {
                return ShotScale::Medium;
            }
            return ShotScale::Wide;
        }
        self.scene
            .map_or(ShotScale::Unknown, aura_core::contract::curate::scale_of_scene)
    }

    /// Mean luminance, `0..1`, when phase 05 measured it. The frame's tonal weight.
    #[must_use]
    pub fn tonal_weight(&self) -> Option<f32> {
        self.descriptor.as_ref().map(|d| d.luma.mean)
    }
}

/// The one way curation learns anything about a project.
///
/// Implemented by `aura-app` out of the frozen services. Every method is a read; nothing on this
/// port can change a photograph, and there is deliberately no `write` of any kind.
pub trait Field: Send + Sync + fmt::Debug {
    /// The **selected** gallery, in timeline order.
    ///
    /// Phase 12's `SelectionResult::selected` and nothing else. Curation's subject is a finished
    /// gallery: a rejected frame is not a curation gap, it is a frame nobody asked about.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn frames(&self, project: ProjectId) -> AuraResult<Vec<Frame>>;

    /// How many photographs the project holds, selected or not.
    ///
    /// On the outline beside `selected` so that a project whose cull has not run is visibly
    /// different from one whose gallery is genuinely small.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the count cannot be read.
    fn photo_count(&self, project: ProjectId) -> AuraResult<u32>;

    /// Phase 12's own coverage report over the gallery.
    ///
    /// The album's report is computed as a **subset** of this one: a rule the gallery misses is a
    /// rule the album necessarily misses, because the product cannot invent coverage. Phase 12's
    /// rule, and the reason this is read rather than recomputed.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the report cannot be read.
    fn gallery_coverage(&self, project: ProjectId) -> AuraResult<CoverageReport>;

    /// Which of the eight hue bands each identity's **measured** skin locus falls in.
    ///
    /// From phase 15's `ToneService::skin_loci`, converted from `u'v'` to a hue. An identity with no
    /// usable locus is absent from the map, which is not the same as an identity in the red band -
    /// and the difference is the whole of ADR-0059 section 5.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the loci cannot be read.
    fn skin_bands(&self, project: ProjectId) -> AuraResult<BTreeMap<IdentityId, u8>>;

    /// Cosine similarity between one frame and several others, through phase 05's index.
    ///
    /// `None` for a pair either of whose vectors is missing, which is a **skipped** uniqueness term
    /// rather than a similarity of zero. Batched because the hero selector asks one candidate about
    /// every chosen hero at once, and the spread pairer asks one frame about its neighbours.
    ///
    /// There is no similarity implementation in this crate. Phase 05's rule: `SimilarityIndex` is
    /// the only way to ask what looks like something.
    fn similarity(&self, from: ImageId, others: &[ImageId]) -> Vec<Option<f32>>;

    /// Every ritual phase 07 named in this project, as the words a caption may use.
    ///
    /// The tradition-specific half of [`crate::caption::Vocabulary`]. A wedding with a saptapadi may
    /// have a caption that says `saptapadi`; one without may not.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rites cannot be read.
    fn rituals(&self, project: ProjectId) -> AuraResult<Vec<String>>;

    /// The identities phase 12's coverage rules treat as close family, and how many album frames
    /// each of them needs.
    ///
    /// The number comes from `coverage_rules.toml`, which is phase 12's file and stays phase 12's
    /// file. There is no second close-family rule in this product.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the identities cannot be read.
    fn close_family(&self, project: ProjectId) -> AuraResult<(Vec<IdentityId>, u32)>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(centre: f32, width: f32, eye: Option<f32>, area: f32) -> FaceRead {
        FaceRead {
            identity: None,
            area_frac: area,
            centre_x: centre,
            width,
            eye_mid_x: eye,
            }
    }

    #[test]
    fn facing_is_measured_from_the_eye_offset_and_is_unknown_without_landmarks() {
        // Eyes right of the box centre: the head is turned toward the viewer's right.
        let right = Facing::measure(&[(0.5, 0.2, 0.54, 0.05)]);
        assert_eq!(right, Facing::Right);

        let left = Facing::measure(&[(0.5, 0.2, 0.46, 0.05)]);
        assert_eq!(left, Facing::Left);

        let frontal = Facing::measure(&[(0.5, 0.2, 0.505, 0.05)]);
        assert_eq!(frontal, Facing::Frontal);

        assert_eq!(Facing::measure(&[]), Facing::Unknown);
    }

    #[test]
    fn a_face_with_no_landmarks_is_skipped_rather_than_read_as_centred() {
        // Phase 06 stores `[[0,0],[0,0]]` for "unknown". A caller that averaged it in would be
        // measuring the frame's top-left corner, which for a face at x = 0.5 reads as a hard left
        // turn.
        let mut frame = Frame::bare(ImageId::new(), 0);
        frame.faces = vec![face(0.5, 0.2, None, 0.05)];
        assert_eq!(frame.facing(), Facing::Unknown);

        frame.faces.push(face(0.5, 0.2, Some(0.55), 0.05));
        assert_eq!(frame.facing(), Facing::Right);
    }

    #[test]
    fn facing_is_weighted_by_face_area_so_the_guest_in_the_background_does_not_decide() {
        let subject = face(0.5, 0.25, Some(0.56), 0.09);
        let guest = face(0.1, 0.05, Some(0.085), 0.002);
        let frame = {
            let mut f = Frame::bare(ImageId::new(), 0);
            f.faces = vec![subject, guest];
            f
        };
        assert_eq!(frame.facing(), Facing::Right);
    }

    #[test]
    fn shot_scale_prefers_a_measured_face_and_falls_back_to_a_scene_it_can_be_sure_of() {
        let mut frame = Frame::bare(ImageId::new(), 0);
        assert_eq!(frame.scale(), ShotScale::Unknown);

        frame.scene = Some(SceneId::Details);
        assert_eq!(frame.scale(), ShotScale::Tight);

        // A ceremony's scale is not known from its label.
        frame.scene = Some(SceneId::Ceremony);
        assert_eq!(frame.scale(), ShotScale::Unknown);

        frame.faces = vec![face(0.5, 0.3, Some(0.5), 0.10)];
        assert_eq!(frame.scale(), ShotScale::Tight);

        frame.faces = vec![face(0.5, 0.1, Some(0.5), 0.03)];
        assert_eq!(frame.scale(), ShotScale::Medium);

        frame.faces = vec![face(0.5, 0.03, Some(0.5), 0.004)];
        assert_eq!(frame.scale(), ShotScale::Wide);
    }

    #[test]
    fn a_frame_nobody_analysed_is_not_readable() {
        let mut frame = Frame::bare(ImageId::new(), 0);
        assert!(!frame.is_readable());
        frame.technical = Some(0.8);
        assert!(frame.is_readable());
    }
}
