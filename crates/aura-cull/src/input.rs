//! Everything the engine is allowed to see, as plain data.
//!
//! This module is the reason section 10.1's determinism criterion is a unit test. The
//! engine takes a [`CullInput`] and returns a `SelectionResult`; it holds no connection,
//! no clock, no service handle and no cache. Two machines given the same [`CullInput`]
//! cannot produce different galleries, because there is nothing else for them to differ
//! about.
//!
//! It is also the boundary that keeps six phases' worth of vocabulary out of the passes.
//! [`gather`](crate::gather) is the only module that knows what an `IntegrityResult` or an
//! `ImageEmotion` is; by the time a candidate reaches [`fusion`](crate::fusion) it is four
//! numbers, a scene, a chapter and a timestamp. That flattening is deliberate: a pass that
//! could read `IntegrityFlags` would eventually make its own decision about what a flag
//! means, and phase 09's rule - one answer to "is this frame sharp" - would have a second
//! answer in the phase that matters most.
//!
//! ## Why a missing phase is `0.5` and a missing verdict is disqualifying
//!
//! [`Candidate::emotion`], [`Candidate::composition`] and [`Candidate::prominence`] carry
//! `KeepScore::NEUTRAL` when their phase has not run, because absence of evidence is not
//! evidence of absence and a geometric mean multiplied by zero would let a phase that
//! never looked at a frame veto it.
//!
//! [`Candidate::analysed`] is different, and it is the one hard requirement. A frame with
//! no phase 09 verdict is not scored at all: it becomes `CullCode::NotAnalysed`, it is
//! counted outside `CullOutline::eligible`, and it is never rejected - because "we have not
//! checked this yet" and "this is not good enough" must never look the same in a gallery.

use aura_core::contract::cull::ImageId;
use aura_core::contract::emotion::Interaction;
use aura_core::contract::scene::{ChapterId, SceneId, Timestamp};
use aura_core::{CullCode, IdentityId, KeepScore, MomentId};

/// How much of the frame the subject fills.
///
/// Section 2.1 asks for diversity "across time, framing (wide/medium/tight) and identity
/// mix", so framing has to be a value the diversity pass can compare. Three buckets rather
/// than a continuous number, because the constraint is "do not deliver six frames that all
/// look the same", and two frames whose subject areas differ by four per cent do look the
/// same.
///
/// Derived in [`gather`](crate::gather) from the largest face box relative to the frame.
/// A frame with no faces is [`Framing::Wide`] - not a fourth "unknown" bucket, because an
/// establishing shot of an empty venue genuinely is a wide frame and giving it its own
/// category would exempt it from the spread rule it most needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Framing {
    /// The subject is small in the frame, or there is no subject.
    #[default]
    Wide,
    /// Half length, a small group, a two-shot.
    Medium,
    /// A head-and-shoulders or closer.
    Tight,
}

impl Framing {
    /// Every framing, from widest to tightest.
    pub const ALL: [Self; 3] = [Self::Wide, Self::Medium, Self::Tight];

    /// Largest face area, as a fraction of the frame, at or above which a frame is tight.
    ///
    /// Six per cent of the frame is a face that fills roughly a quarter of the height on a
    /// 3:2 frame - a portrait rather than a two-shot. Authored, like every band in this
    /// phase that could not be measured from data that does not exist.
    pub const TIGHT_AT: f32 = 0.06;

    /// ...and at or above which it is medium.
    pub const MEDIUM_AT: f32 = 0.012;

    /// The bucket one face area falls in.
    #[must_use]
    pub fn from_face_area(area: f32) -> Self {
        if area >= Self::TIGHT_AT {
            Self::Tight
        } else if area >= Self::MEDIUM_AT {
            Self::Medium
        } else {
            Self::Wide
        }
    }

    /// The stable slug, for telemetry and the coverage panel.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Medium => "medium",
            Self::Tight => "tight",
        }
    }
}

/// One photograph, as the engine sees it.
///
/// Ordered by `(timeline_ts, image_id)` inside [`CullInput::candidates`], and every pass
/// relies on that order for its tie-breaks.
#[derive(Debug, Clone, PartialEq)]
// Five booleans, which is two more than `clippy::struct_excessive_bools` likes, and the
// lint is allowed here rather than obeyed. Each of the five is a fact about a *different*
// phase - phase 09 ran, phase 10 ran, phase 11 ran, phase 10 called this the peak, phase 10
// could not separate that peak - and the lint's remedy is to gather them into a sub-struct,
// which would put four phases' provenance behind one name and make the fusion read
// `candidate.phases.has_emotion`.
#[allow(clippy::struct_excessive_bools)]
pub struct Candidate {
    /// The photograph.
    pub image_id: ImageId,
    /// The moment it belongs to, when it belongs to one.
    pub moment_id: Option<MomentId>,
    /// The chapter it counts toward.
    pub chapter: ChapterId,
    /// What it is of. [`SceneId::Unknown`] means the neutral weight row is used.
    pub scene: SceneId,
    /// Timeline milliseconds. The clock-aligned time phase 01 computed, never the file's.
    pub timeline_ts: Timestamp,

    /// Phase 09's `technical_score`.
    pub technical: f32,
    /// Phase 10's `emotion_score`, or [`KeepScore::NEUTRAL`].
    pub emotion: f32,
    /// Phase 11's `composition_score`, or [`KeepScore::NEUTRAL`].
    pub composition: f32,
    /// Phase 06's prominence-weighted subject presence, or [`KeepScore::NEUTRAL`].
    pub prominence: f32,

    /// True when a phase 09 verdict exists. The eligibility gate; see this module's header.
    pub analysed: bool,
    /// True when a phase 10 reading exists.
    pub has_emotion: bool,
    /// True when a phase 11 judgement exists.
    pub has_composition: bool,

    /// Why this frame never reaches fusion, when something does.
    ///
    /// One of the three `CullCode::VETOES`. Section 6.1's hard vetoes are decided in
    /// [`gather`](crate::gather) from the phase 09 verdict, because they are
    /// *measurements* - the subject is out of focus, the exposure is lost, the eyes are
    /// closed with no reason - and a pass that recomputed them from a composite would be
    /// inventing a second answer to a question phase 09 already answered.
    pub veto: Option<CullCode>,

    /// True when this frame is its moment's peak.
    pub is_peak: bool,
    /// True when the peak was too flat to name, so `is_peak` means little.
    ///
    /// Phase 10's `PeakKind::Flat` and `AURA-ML-5042`. A burst of fourteen near-identical
    /// frames genuinely has no apex, and protecting an arbitrary one of them as "the peak"
    /// would be this phase acting on a claim phase 10 refused to make.
    pub peak_flat: bool,

    /// Which near-identical group this frame is in, when phase 08 put it in one.
    ///
    /// An index into [`MomentInfo::duplicate_groups`]. Two frames sharing one are the same
    /// photograph for delivery purposes, and only one of them may be in the gallery.
    pub duplicate_group: Option<usize>,

    /// Who is in the frame, in phase 06's prominence order.
    pub identities: Vec<IdentityId>,
    /// How much of the frame the subject fills.
    pub framing: Framing,
    /// What the people are doing, strongest first. Phase 10's detections.
    ///
    /// Read by the coverage guard: a `rings` rule matches either the `rings` *scene* or a
    /// `ring_exchange` *interaction*, because a ring exchange inside a long ceremony
    /// segment is usually labelled `ceremony` and a rule that only matched the scene would
    /// report the rings missing at every wedding with a single-segment ceremony.
    pub interactions: Vec<Interaction>,

    /// How sure the evidence underneath is, `0..1`.
    ///
    /// The minimum of the confidences of the sub-scores that exist - not the mean, because
    /// a decision is as certain as its least certain input and averaging hides the one
    /// that is missing.
    pub confidence: f32,
    /// The lowest model version among the phases that contributed.
    pub model_ver: u16,
}

impl Candidate {
    /// True when this frame can be delivered at all.
    #[must_use]
    pub const fn is_eligible(&self) -> bool {
        self.analysed && self.veto.is_none()
    }

    /// The four sub-scores, in `KeepScore` order.
    #[must_use]
    pub const fn sub_scores(&self) -> [f32; 4] {
        [
            self.technical,
            self.emotion,
            self.composition,
            self.prominence,
        ]
    }

    /// A candidate with nothing known about it but its identity and its place in time.
    ///
    /// The shape a frame with no analysis takes, and the starting point every fixture
    /// builds from. Deliberately **not** `Default`: a candidate needs an id and a time,
    /// and a `Candidate::default()` with a zero timestamp would sort to the front of every
    /// gallery in a test that forgot to set one.
    #[must_use]
    pub fn unanalysed(image_id: ImageId, timeline_ts: Timestamp) -> Self {
        Self {
            image_id,
            moment_id: None,
            chapter: ChapterId::Other,
            scene: SceneId::Unknown,
            timeline_ts,
            technical: 0.0,
            emotion: KeepScore::NEUTRAL,
            composition: KeepScore::NEUTRAL,
            prominence: KeepScore::NEUTRAL,
            analysed: false,
            has_emotion: false,
            has_composition: false,
            veto: None,
            is_peak: false,
            peak_flat: false,
            duplicate_group: None,
            identities: Vec::new(),
            framing: Framing::Wide,
            interactions: Vec::new(),
            confidence: 0.0,
            model_ver: 0,
        }
    }
}

/// One moment, as the engine sees it.
///
/// The frames are not carried here - they are on the candidates, which is the only place
/// they can be kept in one order. What is here is what the moment pass needs *about* the
/// group: how varied it is, what may not be delivered twice, and how significant it is.
#[derive(Debug, Clone, PartialEq)]
pub struct MomentInfo {
    /// The moment.
    pub id: MomentId,
    /// How many frames it holds.
    pub frames: u32,
    /// Mean pairwise embedding distance within the moment, `0..1`.
    ///
    /// Phase 08's `Moment::diversity`, and the single input that decides `k`. Section 6.2:
    /// "k = 1 for low-diversity moments, up to 3-5 for high-diversity, high-significance
    /// moments". Zero means the photographer bracketed a static subject and every frame
    /// says the same thing.
    pub diversity: f32,
    /// How much of the wedding's story this moment carries, `0..1`.
    ///
    /// Phase 10's peak strength combined with the scene's own weight. It is the second
    /// half of section 6.2's `k` rule - a varied dance sequence earns more keepers than a
    /// varied set of frames of the car park.
    pub significance: f32,
    /// Sets of frames that are the same photograph. Indices are [`Candidate::duplicate_group`].
    ///
    /// From phase 08's `Moment::duplicate_sets`, `Identical` and `NearIdentical` only.
    /// Phase 08 deliberately does not produce a `Variant` set, so everything here really is
    /// one photograph twice.
    pub duplicate_groups: Vec<Vec<ImageId>>,
    /// True when the photographer split, merged or pinned this moment.
    ///
    /// Carried so the moment pass can leave a pinned grouping alone. It does **not** stop
    /// the engine from choosing inside it: pinning a moment is a statement about which
    /// frames belong together, not about which of them is delivered.
    pub user_locked: bool,
}

/// One identity, as the coverage guard sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityInfo {
    /// The identity.
    pub id: IdentityId,
    /// Its weight in the subject hierarchy, `0..1`.
    pub weight: f32,
    /// True when this is one of the couple.
    pub primary: bool,
    /// True when this is close family or somebody the photographer marked important.
    ///
    /// Read from `SubjectHierarchy::secondary` rather than from a per-identity `Role`,
    /// because `PeopleService` does not expose one and amending a frozen phase 06 contract
    /// to add a getter would be a large change for a small convenience. ADR-0025 section 5.
    pub close_family: bool,
    /// How many photographs in the project they appear in.
    ///
    /// The recurrence signal behind section 6.3's "every recurring guest >= 1". A guest in
    /// four frames wants one of them in the gallery; a guest in one frame was walking past.
    pub appearances: u32,
}

/// Everything one run of the engine is given.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CullInput {
    /// Every photograph in the project, ordered by `(timeline_ts, image_id)`.
    ///
    /// Including the unanalysed ones. They are in the input rather than filtered out by
    /// the gatherer so that `CullOutline::coverage` has a denominator, and so that the
    /// panel can say how many frames were never considered.
    pub candidates: Vec<Candidate>,
    /// Every moment, in timeline order.
    pub moments: Vec<MomentInfo>,
    /// Every identity the project knows about, ordered by id.
    pub identities: Vec<IdentityInfo>,
    /// How long the wedding ran, in hours.
    ///
    /// A feature of the size regression. Computed from the first and last timeline stamps
    /// rather than from a venue booking, which is the only source that exists.
    pub hours: f32,
    /// True when phase 06 identified the couple by evidence and nobody confirmed it.
    ///
    /// Surfaced as a coverage warning, because if the primary identities are wrong then so
    /// is every decision the identity rules made. `SubjectHierarchy::couple_unconfirmed`.
    pub couple_unconfirmed: bool,
    /// The lowest model version among every phase that contributed a sub-score.
    pub model_ver: u16,
}

impl CullInput {
    /// How many frames carry a phase 09 verdict and no veto.
    #[must_use]
    pub fn eligible(&self) -> usize {
        self.candidates
            .iter()
            .filter(|candidate| candidate.analysed)
            .count()
    }

    /// How many frames nobody has analysed.
    #[must_use]
    pub fn unanalysed(&self) -> usize {
        self.candidates.len() - self.eligible()
    }

    /// The moment a frame belongs to, if any.
    #[must_use]
    pub fn moment(&self, id: MomentId) -> Option<&MomentInfo> {
        self.moments.iter().find(|moment| moment.id == id)
    }

    /// The identity record for one id, if the project knows it.
    #[must_use]
    pub fn identity(&self, id: IdentityId) -> Option<&IdentityInfo> {
        self.identities.iter().find(|entry| entry.id == id)
    }

    /// Put the candidates into the order every pass assumes.
    ///
    /// `(timeline_ts, image_id)`, and the second key is not decoration: two frames from
    /// two cameras can share a millisecond, and a sort that left them in whatever order
    /// the catalog returned would make the gallery depend on `SQLite`'s row order. Section
    /// 10.1's byte-identical criterion dies quietly there if it dies anywhere.
    pub fn sort(&mut self) {
        self.candidates.sort_by(
            |left, right| match left.timeline_ts.cmp(&right.timeline_ts) {
                std::cmp::Ordering::Equal => left.image_id.cmp(&right.image_id),
                other => other,
            },
        );
        self.moments.sort_by_key(|moment| moment.id);
        self.identities.sort_by_key(|identity| identity.id);
    }
}
