//! FROZEN CONTRACT. The learning loop. PHASE-30 section 5.
//!
//! Every phase from 06 to 29 lets a photographer disagree with it. This one is what makes the
//! disagreement worth something: a correction is captured, attributed to the decision it corrected,
//! aggregated with the others, turned into an offset, measured on corrections it was not fitted on,
//! and adopted **by a person** or not at all.
//!
//! ## Why the shapes here are so defensive
//!
//! A learning loop is the one feature in this product that can make it worse over time while every
//! test stays green. Section 12's second row - "learning loop degrades quality over time" - is not a
//! hypothetical: a loop that fits on everything it sees will happily learn a photographer's
//! Tuesday-afternoon mood, a wedding shot through a marquee's yellow canvas, and the forty frames
//! somebody fixed by hand because the model was wrong about a single room. Then it applies that to
//! the next wedding, which arrives looking subtly wrong in a way nobody can point at.
//!
//! Five properties are structural rather than promised:
//!
//! **Nothing is adopted without a person.** [`LearningUpdate::adopted`] is set by
//! `LearnService::adopt`, which takes an update a human is looking at. There is no automatic
//! adoption path, no confidence above which one appears, and no setting that enables one. Section
//! 6.3: "the user adopts explicitly, and one click rolls back". A loop that could adopt on its own
//! would be a loop whose failure mode is silent.
//!
//! **Improvement is measured on corrections the fit never saw.** [`LearningUpdate::
//! expected_improvement`] is defined as the change on a **held-out** split, and
//! [`HeldOut::deterministic`] is how the split is chosen - by a hash of the correction's id, not by
//! a shuffle - so the same corrections produce the same split on every machine and re-running a fit
//! cannot quietly re-draw the line until the number looks good.
//!
//! **A correction that cannot be attributed is not learned from.** [`Correction::decision_id`] is
//! not an `Option`. A slider a photographer moved on a photograph AURA never made a decision about
//! is a preference with no baseline, and a residual measured from no baseline is an absolute edit
//! wearing a residual's shape - which is phase 17's condition C4 exactly, in the phase that would
//! propagate it to every future wedding.
//!
//! **A parameter is learnable or it is not, and the list is closed.**
//! [`Learnable`] has fifteen members and no `Other`. Section 6.3's update list is style deltas,
//! ranker weights and threshold offsets; anything outside that - a mask boundary, a retouch
//! ceiling, a crop safety rule, a cleanup permission - is a *guarantee* rather than a preference,
//! and a loop that could move one would be a loop that learns its way past a promise.
//!
//! **Contribution is per project, off, and recorded.** [`Consent`] carries what was agreed, when,
//! and by which build. Section 6.3's last line is "strictly opt-in per project with a clear consent
//! record", and a consent that is a boolean in a settings file is a consent nobody can produce
//! afterwards.
//!
//! ## The one thing a later phase can get wrong
//!
//! **A correction is evidence about a decision, never about a photograph.** [`Correction::
//! magnitude`] says how far somebody moved a value, and it is deliberately not a quality score. A
//! large correction on one frame is a photographer fixing one frame; the loop only ever acts on the
//! *central tendency* of a bucket with at least [`MIN_CORRECTIONS`] of them in it, and
//! [`Aggregate::outliers_dropped`] is what it threw away to get there.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult, ErrorCode, Recovery, Severity};
use crate::contract::ids::{DecisionId, IdentityId, ProfileId, ProjectId};
use crate::contract::ledger::DecisionKind;
use crate::contract::scene::{ImageId, SceneId, Timestamp};

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// The fewest corrections a bucket needs before the loop will act on it.
///
/// Twelve. Section 6.3 asks for "a minimum count before acting" and does not name one; twelve is
/// where a trimmed mean over a bucket stops being a description of two weddings. It is deliberately
/// a *count of corrections* rather than of weddings, because a photographer who fixes the same thing
/// twelve times across one wedding has told the product something twelve times.
pub const MIN_CORRECTIONS: u32 = 12;

/// The fewest weddings a bucket must draw from.
///
/// Two, so that one unusual wedding cannot on its own move a photographer's profile. Section 12's
/// "learning loop degrades quality over time" is mostly this failure: a marquee's yellow canvas,
/// learned from a single Saturday, applied to every wedding afterwards.
pub const MIN_PROJECTS: u32 = 2;

/// The share of a bucket's corrections held out of the fit.
///
/// A quarter. Small enough that a bucket at [`MIN_CORRECTIONS`] still fits on nine, large enough
/// that three held-out corrections is not a coin toss.
pub const HELD_OUT_SHARE: f32 = 0.25;

/// How far from the bucket's median a correction may sit before it is dropped as an outlier.
///
/// Three median absolute deviations. Section 6.3: "discard outliers".
pub const OUTLIER_MADS: f32 = 3.0;

/// The most of a bucket's own measured shift one update may take.
///
/// Half. Section 6.3's "incrementally": an update that moved the whole way would oscillate, because
/// the next wedding's corrections are measured against a baseline that has already moved.
pub const MAX_STEP_SHARE: f32 = 0.5;

/// How many updates of one profile are kept for rollback.
///
/// Ten. A photographer rolls back to the version before the one that went wrong, not to the version
/// from last spring, and an unbounded history is a table that grows with usage forever.
pub const ROLLBACK_DEPTH: u16 = 10;

/// The longest a diff-summary line may be.
pub const MAX_DIFF_LINE: usize = 160;

/// How many diff-summary lines an update may carry.
pub const MAX_DIFF_LINES: usize = 24;

/// The improvement an update must show on held-out corrections before it may be offered at all.
///
/// Two per cent. Below this the difference is inside the measurement's own noise, and offering it
/// would train a photographer to click adopt on nothing.
pub const MIN_OFFERABLE_IMPROVEMENT: f32 = 0.02;

// ---------------------------------------------------------------------------
// What may be learned
// ---------------------------------------------------------------------------

/// A value the loop is allowed to move.
///
/// **Closed, with no `Other`.** Section 6.3 names three families - phase 17's style deltas, phases
/// 10 and 11's ranker weights, phase 12's threshold offsets - and this is that list written out.
///
/// What is *not* here is the point. There is no member for a mask boundary, a retouch ceiling, a
/// crop safety margin, a cleanup permission, a skin guard, an identity-drift cap or a coverage
/// guarantee. Those are guarantees rather than preferences: a photographer who repeatedly widened a
/// retouch is a photographer whose next wedding must still get the texture floor, because the floor
/// is a promise `docs/retouch-ethics.md` makes about the product rather than a default somebody
/// chose. A loop that could move one would be a loop that learns its way past a promise, one
/// wedding at a time, with every gate green.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Learnable {
    // -- phase 17 style deltas ---------------------------------------------------------
    /// Exposure, in stops.
    #[default]
    Exposure,
    /// Colour temperature, in kelvin.
    TemperatureK,
    /// Tint.
    Tint,
    /// Contrast.
    Contrast,
    /// Highlight recovery.
    Highlights,
    /// Shadow lift.
    Shadows,
    /// White point.
    Whites,
    /// Black point.
    Blacks,
    /// Vibrance.
    Vibrance,
    /// Flat saturation.
    Saturation,

    // -- phases 10 and 11 ranker weights ------------------------------------------------
    /// How much the emotion ranker's expression term counts.
    EmotionWeight,
    /// How much composition counts in the hero and cull scores.
    CompositionWeight,

    // -- phase 12 threshold offsets -----------------------------------------------------
    /// How readily a frame is kept: an offset on the keep threshold, never on a veto.
    KeepThreshold,
    /// How large a gallery this photographer delivers, as a share of the predicted size.
    GallerySize,

    // -- phase 29 curation --------------------------------------------------------------
    /// How readily a frame is offered as a hero.
    HeroThreshold,
}

impl Learnable {
    /// Every learnable value.
    pub const ALL: [Self; 15] = [
        Self::Exposure,
        Self::TemperatureK,
        Self::Tint,
        Self::Contrast,
        Self::Highlights,
        Self::Shadows,
        Self::Whites,
        Self::Blacks,
        Self::Vibrance,
        Self::Saturation,
        Self::EmotionWeight,
        Self::CompositionWeight,
        Self::KeepThreshold,
        Self::GallerySize,
        Self::HeroThreshold,
    ];

    /// How many there are.
    pub const COUNT: usize = 15;

    /// The stored slug and the wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exposure => "exposure",
            Self::TemperatureK => "temperature_k",
            Self::Tint => "tint",
            Self::Contrast => "contrast",
            Self::Highlights => "highlights",
            Self::Shadows => "shadows",
            Self::Whites => "whites",
            Self::Blacks => "blacks",
            Self::Vibrance => "vibrance",
            Self::Saturation => "saturation",
            Self::EmotionWeight => "emotion_weight",
            Self::CompositionWeight => "composition_weight",
            Self::KeepThreshold => "keep_threshold",
            Self::GallerySize => "gallery_size",
            Self::HeroThreshold => "hero_threshold",
        }
    }

    /// Which decision kind a correction to this value comes from.
    #[must_use]
    pub const fn decision_kind(self) -> DecisionKind {
        match self {
            Self::Exposure
            | Self::TemperatureK
            | Self::Tint
            | Self::Contrast
            | Self::Highlights
            | Self::Shadows
            | Self::Whites
            | Self::Blacks
            | Self::Vibrance
            | Self::Saturation => DecisionKind::Edit,
            Self::EmotionWeight
            | Self::CompositionWeight
            | Self::KeepThreshold
            | Self::GallerySize => DecisionKind::Cull,
            Self::HeroThreshold => DecisionKind::Curate,
        }
    }

    /// The largest offset this value may ever carry, in its own units.
    ///
    /// **The contract owns every bound.** A studio's config may lower one of these and nothing may
    /// raise one, which is the shape phases 21 and 22 established for ceilings. A learning loop with
    /// an unbounded step is a learning loop that walks off after four weddings.
    #[must_use]
    // The arms are written one family at a time and two of them happen to share a number today.
    // Collapsing them would put `GallerySize` beside `EmotionWeight` in a match whose grouping is
    // the argument - a ranker weight and a gallery-size share are bounded at 0.20 for completely
    // different reasons, and the day one of them moves the merge has to be undone first.
    #[allow(clippy::match_same_arms)]
    pub fn ceiling(self) -> f32 {
        match self {
            Self::Exposure => 0.60,
            Self::TemperatureK => 600.0,
            Self::Tint => 8.0,
            Self::Contrast
            | Self::Highlights
            | Self::Shadows
            | Self::Whites
            | Self::Blacks
            | Self::Vibrance
            | Self::Saturation => 0.25,
            Self::EmotionWeight | Self::CompositionWeight => 0.20,
            Self::KeepThreshold | Self::HeroThreshold => 0.10,
            Self::GallerySize => 0.20,
        }
    }

    /// Parse the stored slug.
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11002` when the slug names nothing this build can learn.
    pub fn parse(slug: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|l| l.as_str() == slug)
            .ok_or_else(|| {
                AuraError::new(
                    ErrorCode("AURA-LRN-11002"),
                    Severity::Degraded,
                    Recovery::Fallback,
                    format!("`{slug}` is not something this build learns"),
                    "AURA found a learned setting it does not recognise, which usually means this \
                     profile was trained by a newer version.",
                )
            })
    }
}

impl fmt::Display for Learnable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// A correction - section 5, verbatim
// ---------------------------------------------------------------------------

/// One thing a photographer changed, attributed to the decision it changed.
///
/// **`decision_id` is not an `Option`.** A correction with no decision behind it is a preference
/// with no baseline, and a residual measured from no baseline is an absolute edit wearing a
/// residual's shape - phase 17's condition C4, in the phase that would carry it into every future
/// wedding. `capture` refuses one rather than storing it with a placeholder.
///
/// **`before_json` and `after_json` are the decision's own encoding**, not a rendering of it. What
/// makes a correction re-readable in two years is that it stores what the ledger stored, and the
/// ledger's canonical encoding is phase 13's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    /// The decision this corrected. Phase 13's ledger row.
    pub decision_id: DecisionId,
    /// What kind of decision it was.
    pub kind: DecisionKind,
    /// The decision's own value, canonically encoded.
    pub before_json: String,
    /// What the photographer replaced it with.
    pub after_json: String,
    /// What the photograph is of. The bucket's first coordinate.
    pub scene: SceneId,
    /// Whose face this is about, when the correction is about a person.
    #[serde(default)]
    pub identity: Option<IdentityId>,
    /// How far the value moved, in the value's own units, signed.
    ///
    /// **Not a quality score.** A large magnitude is a photographer fixing one frame; the loop only
    /// ever reads the central tendency of a bucket.
    pub magnitude: f32,
    /// When, epoch milliseconds.
    pub created_at: Timestamp,
}

/// Everything about a correction that is not in section 5's frozen shape.
///
/// Kept beside `Correction` rather than inside it, because section 5 is copied verbatim and this is
/// what the store needs in order to bucket, hold out and attribute. ADR-0061 section 5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionContext {
    /// The wedding it came from. The [`MIN_PROJECTS`] guard counts these.
    pub project: ProjectId,
    /// The photograph.
    pub image: ImageId,
    /// Which value moved.
    pub learnable: Learnable,
    /// Whether this correction is in the held-out split.
    pub held_out: bool,
    /// Whether an update has already consumed it.
    pub consumed_by: Option<u16>,
}

impl Correction {
    /// Whether this correction is worth recording at all.
    ///
    /// A zero-magnitude correction is a photographer opening a panel and closing it; recording one
    /// would put a thousand rows of nothing into every bucket's denominator and drag every median
    /// toward zero.
    #[must_use]
    pub fn is_material(&self, learnable: Learnable) -> bool {
        // A thousandth of the value's own ceiling. Scaled rather than absolute, because 0.001 is
        // meaningless for a slider in [-1, 1] and nothing at all for a temperature in kelvin.
        self.magnitude.abs() > learnable.ceiling() * 0.01
    }
}

// ---------------------------------------------------------------------------
// Buckets and aggregation
// ---------------------------------------------------------------------------

/// Where a correction is counted.
///
/// Section 6.3: "group corrections by (decision kind, scene bucket, identity role)". The identity
/// half is a *role* rather than a person, because a profile that learned "brighten this specific
/// bride" would be a profile that is wrong on every subsequent wedding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CorrectionBucket {
    /// What was decided.
    pub kind: DecisionKind,
    /// What the photograph is of.
    pub scene: SceneId,
    /// Which value.
    pub learnable: Learnable,
    /// Whether the correction was about somebody close to the couple.
    pub subject_close: bool,
}

impl CorrectionBucket {
    /// The stored key, and the key the panel groups by.
    #[must_use]
    pub fn key(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.kind.as_str(),
            self.scene.as_str(),
            self.learnable.as_str(),
            u8::from(self.subject_close)
        )
    }
}

/// What one bucket of corrections says, once the outliers are gone.
///
/// **The central tendency is a trimmed median, not a mean.** A mean over a bucket that contains one
/// photographer's rescue of a single badly-lit room is a mean that has that room in it. The median
/// is what section 6.3's "robust central tendencies" means, and `outliers_dropped` is what the panel
/// shows so that a photographer can see the loop ignored their four extreme fixes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aggregate {
    /// Which bucket.
    pub bucket: CorrectionBucket,
    /// How many corrections went in, before trimming.
    pub corrections: u32,
    /// How many weddings they came from.
    pub projects: u32,
    /// How many were dropped as outliers.
    pub outliers_dropped: u32,
    /// The trimmed median magnitude, in the value's own units.
    pub central: f32,
    /// The median absolute deviation, which is how sure the bucket is of itself.
    pub dispersion: f32,
    /// How many were held out of the fit.
    pub held_out: u32,
    /// Whether this bucket met [`MIN_CORRECTIONS`] and [`MIN_PROJECTS`].
    pub actionable: bool,
}

impl Aggregate {
    /// The offset this bucket proposes, bounded by [`MAX_STEP_SHARE`] and the value's ceiling.
    ///
    /// **Two bounds and not one.** The share bound stops an update taking the whole of a measured
    /// shift, which would oscillate; the ceiling stops a sequence of updates from walking off, which
    /// the share bound alone cannot - half of half of half still reaches the ceiling eventually if
    /// nothing catches it.
    #[must_use]
    pub fn proposed_offset(&self) -> f32 {
        if !self.actionable {
            return 0.0;
        }
        let ceiling = self.bucket.learnable.ceiling();
        (self.central * MAX_STEP_SHARE).clamp(-ceiling, ceiling)
    }
}

/// How the held-out split was drawn.
///
/// **Deterministic, by a hash of the correction's decision id.** Not a shuffle, not a timestamp cut,
/// not "the last quarter". A shuffle re-draws the line on every fit, which means a fit that
/// disappoints can be re-run until the number is good - and nothing about that would look wrong. A
/// timestamp cut holds out the most recent corrections, which are exactly the ones a photographer's
/// current taste is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HeldOut {
    /// Whether the split is reproducible from the corrections alone.
    pub deterministic: bool,
    /// Corrections in the fit.
    pub fitted: u32,
    /// Corrections kept back.
    pub held: u32,
}

// ---------------------------------------------------------------------------
// An update - section 5, verbatim
// ---------------------------------------------------------------------------

/// One proposed change to a profile, measured and waiting for a person.
///
/// **`adopted` is set by `LearnService::adopt` and by nothing else.** There is no code path in this
/// product that flips it, no confidence above which it flips itself, and no setting that enables
/// one. Section 6.3, and section 10.1's "no learning update is adopted without explicit user
/// action" is a test over exactly this field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningUpdate {
    /// The profile this changes.
    pub profile_id: ProfileId,
    /// The version it is measured against.
    pub from_version: u16,
    /// The version it would produce.
    pub to_version: u16,
    /// How many corrections it was fitted on. **Excludes the held-out split.**
    pub corrections_used: u32,
    /// The improvement on held-out corrections, `0..1`, as a share of the error it started with.
    pub expected_improvement: f32,
    /// One line per changed value, in the photographer's own words.
    pub diff_summary: Vec<String>,
    /// Whether a person accepted it.
    pub adopted: bool,
}

impl LearningUpdate {
    /// Whether this update may be shown to a photographer at all.
    ///
    /// Below [`MIN_OFFERABLE_IMPROVEMENT`] the difference is inside the measurement's own noise, and
    /// offering it would train somebody to click adopt on nothing.
    #[must_use]
    pub fn is_offerable(&self) -> bool {
        self.expected_improvement >= MIN_OFFERABLE_IMPROVEMENT
            && self.corrections_used >= MIN_CORRECTIONS
            && !self.diff_summary.is_empty()
    }

    /// Check the update's own bounds.
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11003` when the version does not advance, the summary is too long, or a line is.
    pub fn validate(&self) -> AuraResult<()> {
        if self.to_version <= self.from_version {
            return Err(bad_update(format!(
                "update goes from version {} to {}",
                self.from_version, self.to_version
            )));
        }
        if self.diff_summary.len() > MAX_DIFF_LINES {
            return Err(bad_update(format!(
                "{} summary lines, more than {MAX_DIFF_LINES}",
                self.diff_summary.len()
            )));
        }
        if self
            .diff_summary
            .iter()
            .any(|l| l.chars().count() > MAX_DIFF_LINE)
        {
            return Err(bad_update("a summary line is too long".to_owned()));
        }
        Ok(())
    }
}

/// The two sides a photographer compares before adopting.
///
/// Section 6.3's "show an A/B comparison". Both sides are measured on the **same** held-out
/// corrections, which is the only comparison that means anything: a current profile measured on one
/// split and a candidate measured on another is two numbers about two different questions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbComparison {
    /// The profile.
    pub profile_id: ProfileId,
    /// The version in use.
    pub current_version: u16,
    /// The version on offer.
    pub candidate_version: u16,
    /// Mean absolute residual of the current profile on the held-out corrections.
    pub current_error: f32,
    /// Mean absolute residual of the candidate on the **same** corrections.
    pub candidate_error: f32,
    /// How many corrections both were measured on.
    pub held_out: u32,
    /// Per value: what it was, what it would be, and how many corrections say so.
    pub rows: Vec<AbRow>,
}

impl AbComparison {
    /// The improvement, `0..1`, as a share of the error the current profile has.
    ///
    /// Zero when the candidate is no better, and never negative: a candidate that is *worse* is
    /// reported as no improvement and refused by [`LearningUpdate::is_offerable`], because a
    /// negative improvement rendered in a panel is a number a photographer would reasonably read as
    /// a magnitude.
    #[must_use]
    pub fn improvement(&self) -> f32 {
        if self.current_error <= f32::EPSILON {
            return 0.0;
        }
        ((self.current_error - self.candidate_error) / self.current_error).clamp(0.0, 1.0)
    }
}

/// One row of the A/B comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbRow {
    /// Which value.
    pub learnable: Learnable,
    /// Which scene it is conditioned on.
    pub scene: SceneId,
    /// The offset in the current profile.
    pub current: f32,
    /// The offset the candidate proposes.
    pub candidate: f32,
    /// How many corrections the candidate's number came from.
    pub corrections: u32,
    /// The sentence a photographer reads.
    pub summary: String,
}

// ---------------------------------------------------------------------------
// Consent
// ---------------------------------------------------------------------------

/// What a photographer agreed to, when, and in which build.
///
/// **Per project and off.** Section 6.3's last line, and the record exists because a consent that
/// is a boolean in a settings file is a consent nobody can produce a year later when somebody asks
/// what was agreed to.
///
/// `local_learning` is separate from `dataset_contribution` and both default to off. They are
/// different questions: one asks whether this machine may learn from this wedding, the other whether
/// anonymised evidence may leave it. Collapsing them into one switch would make the second one
/// happen by accident.
///
/// Four bools rather than a bitmask or a set, deliberately. Each is a separate question a person
/// answered separately, and the whole point of the shape is that they cannot be set together by
/// accident. Folding them into an integer would make "what did they agree to" an argument about how
/// to decode it, in the one struct where that question has to be answerable a year later.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consent {
    /// The wedding.
    pub project: ProjectId,
    /// May this machine learn from this wedding's corrections?
    pub local_learning: bool,
    /// May anonymised corrections from it contribute to the shared dataset?
    pub dataset_contribution: bool,
    /// May crash reports be sent?
    pub crash_reports: bool,
    /// May aggregate telemetry be sent?
    pub telemetry: bool,
    /// When the answers were last set, epoch milliseconds.
    pub decided_at: Timestamp,
    /// Which build asked. A consent given to one version's wording is a consent to that wording.
    pub app_version: String,
}

impl Consent {
    /// Everything off, which is what a project starts with.
    #[must_use]
    pub fn none(project: ProjectId, app_version: impl Into<String>) -> Self {
        Self {
            project,
            local_learning: false,
            dataset_contribution: false,
            crash_reports: false,
            telemetry: false,
            decided_at: 0,
            app_version: app_version.into(),
        }
    }

    /// Whether anything at all may leave this machine because of this project.
    ///
    /// One predicate rather than three checks at three call sites, because the failure this guards
    /// is a fourth call site that forgot one of them.
    #[must_use]
    pub const fn anything_leaves(&self) -> bool {
        self.dataset_contribution || self.crash_reports || self.telemetry
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why the learning loop did or did not do something.
///
/// Invariant 2 in the phase whose decisions are about the product itself. A photographer looking at
/// "no update available" has to be able to find out whether that means "you have not corrected
/// anything", "you have corrected plenty and it made no difference", or "one of your weddings
/// disagrees with the other four" - and those are three completely different sentences.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum LearnCode {
    /// The correction was recorded against its decision.
    #[default]
    CorrectionCaptured,
    /// The correction moved nothing worth recording.
    CorrectionImmaterial,
    /// There is no ledger decision behind this change, so it was not recorded.
    NoDecisionToAttributeTo,
    /// The value changed is not one this product learns.
    NotLearnable,
    /// This project's consent does not allow local learning.
    LearningNotConsented,
    /// The bucket has fewer than [`MIN_CORRECTIONS`] corrections.
    TooFewCorrections,
    /// The bucket draws on fewer than [`MIN_PROJECTS`] weddings.
    TooFewWeddings,
    /// A correction sat more than [`OUTLIER_MADS`] deviations from its bucket's median.
    OutlierDropped,
    /// The bucket agreed with itself, so the offset it proposes is confident.
    BucketConsistent,
    /// The bucket's corrections disagree with each other, so the offset was damped.
    BucketDispersed,
    /// The proposed offset was cut to [`MAX_STEP_SHARE`] of the measured shift.
    StepBounded,
    /// The proposed offset reached the value's own ceiling.
    CeilingBinding,
    /// The candidate did better than the current profile on corrections it never saw.
    HeldOutImproved,
    /// The candidate did no better on corrections it never saw, so nothing is offered.
    HeldOutNoImprovement,
    /// The candidate did **worse** on held-out corrections. Not offered, and said out loud.
    HeldOutRegressed,
    /// A person adopted the update.
    AdoptedByUser,
    /// A person rolled the profile back.
    RolledBack,
    /// The rollback restored the previous profile byte for byte.
    RollbackExact,
    /// There is no earlier version to roll back to.
    NothingToRollBackTo,
    /// Anonymised contribution is off for this project, which is the default.
    ContributionOff,
}

impl LearnCode {
    /// Every code.
    pub const ALL: [Self; 20] = [
        Self::CorrectionCaptured,
        Self::CorrectionImmaterial,
        Self::NoDecisionToAttributeTo,
        Self::NotLearnable,
        Self::LearningNotConsented,
        Self::TooFewCorrections,
        Self::TooFewWeddings,
        Self::OutlierDropped,
        Self::BucketConsistent,
        Self::BucketDispersed,
        Self::StepBounded,
        Self::CeilingBinding,
        Self::HeldOutImproved,
        Self::HeldOutNoImprovement,
        Self::HeldOutRegressed,
        Self::AdoptedByUser,
        Self::RolledBack,
        Self::RollbackExact,
        Self::NothingToRollBackTo,
        Self::ContributionOff,
    ];

    /// How many there are.
    pub const COUNT: usize = 20;

    /// The stored slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CorrectionCaptured => "correction_captured",
            Self::CorrectionImmaterial => "correction_immaterial",
            Self::NoDecisionToAttributeTo => "no_decision_to_attribute_to",
            Self::NotLearnable => "not_learnable",
            Self::LearningNotConsented => "learning_not_consented",
            Self::TooFewCorrections => "too_few_corrections",
            Self::TooFewWeddings => "too_few_weddings",
            Self::OutlierDropped => "outlier_dropped",
            Self::BucketConsistent => "bucket_consistent",
            Self::BucketDispersed => "bucket_dispersed",
            Self::StepBounded => "step_bounded",
            Self::CeilingBinding => "ceiling_binding",
            Self::HeldOutImproved => "held_out_improved",
            Self::HeldOutNoImprovement => "held_out_no_improvement",
            Self::HeldOutRegressed => "held_out_regressed",
            Self::AdoptedByUser => "adopted_by_user",
            Self::RolledBack => "rolled_back",
            Self::RollbackExact => "rollback_exact",
            Self::NothingToRollBackTo => "nothing_to_roll_back_to",
            Self::ContributionOff => "contribution_off",
        }
    }

    /// The sentence a photographer reads.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::CorrectionCaptured => "AURA noticed this change and will learn from it.",
            Self::CorrectionImmaterial => "This change was too small to learn anything from.",
            Self::NoDecisionToAttributeTo => {
                "AURA had not made a decision here, so there is nothing for this change to be a \
                 correction of."
            }
            Self::NotLearnable => "This is not one of the settings AURA learns.",
            Self::LearningNotConsented => "Learning is switched off for this wedding.",
            Self::TooFewCorrections => {
                "Not enough corrections yet. AURA waits until it has seen the same thing several \
                 times."
            }
            Self::TooFewWeddings => {
                "These corrections all come from one wedding, so AURA is waiting to see whether it \
                 happens again."
            }
            Self::OutlierDropped => "One correction was far from the rest and was left out.",
            Self::BucketConsistent => "Your corrections here agree with each other.",
            Self::BucketDispersed => {
                "Your corrections here vary a lot, so AURA has moved less than they suggest."
            }
            Self::StepBounded => "AURA moved part of the way, not all of it.",
            Self::CeilingBinding => "This has moved as far as AURA will ever move it.",
            Self::HeldOutImproved => {
                "Tested against corrections it had not seen, the new version did better."
            }
            Self::HeldOutNoImprovement => {
                "Tested against corrections it had not seen, the new version was no better, so \
                 there is nothing to adopt."
            }
            Self::HeldOutRegressed => {
                "Tested against corrections it had not seen, the new version did worse. It has not \
                 been offered."
            }
            Self::AdoptedByUser => "You adopted this update.",
            Self::RolledBack => "You rolled this profile back.",
            Self::RollbackExact => "The previous version was restored exactly.",
            Self::NothingToRollBackTo => "There is no earlier version of this profile.",
            Self::ContributionOff => {
                "Nothing from this wedding is shared. That is the default and you have not changed \
                 it."
            }
        }
    }

    /// Parse the stored slug.
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11001` when the slug is not one this build knows.
    pub fn parse(slug: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == slug)
            .ok_or_else(|| {
                AuraError::new(
                    ErrorCode("AURA-LRN-11001"),
                    Severity::Degraded,
                    Recovery::Fallback,
                    format!("unknown learning reason code `{slug}`"),
                    "AURA found a learning note it does not recognise, which usually means this \
                     profile was trained by a newer version.",
                )
            })
    }
}

impl fmt::Display for LearnCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason, with the measured half of its sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnReason {
    /// Which code.
    pub code: LearnCode,
    /// The measured half, when there is one.
    #[serde(default)]
    pub detail: Option<String>,
}

impl LearnReason {
    /// A reason with no detail.
    #[must_use]
    pub const fn plain(code: LearnCode) -> Self {
        Self { code, detail: None }
    }

    /// A reason with its measured half.
    #[must_use]
    pub fn with(code: LearnCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
        }
    }

    /// The whole sentence.
    #[must_use]
    pub fn sentence(&self) -> String {
        match &self.detail {
            Some(d) => format!("{} ({d})", self.code.user_text()),
            None => self.code.user_text().to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What the loop has seen and what it has done about it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LearnOutline {
    /// Corrections captured, all time.
    pub corrections: u32,
    /// Weddings they came from.
    pub projects: u32,
    /// Buckets with at least one correction in them.
    pub buckets: u32,
    /// Buckets that met [`MIN_CORRECTIONS`] and [`MIN_PROJECTS`].
    pub actionable_buckets: u32,
    /// Corrections that could not be attributed to a decision and were refused.
    pub unattributed: u32,
    /// Corrections dropped as outliers by the last aggregation.
    pub outliers: u32,
    /// Updates computed.
    pub updates: u32,
    /// Updates a person adopted.
    pub adopted: u32,
    /// Rollbacks a person asked for.
    pub rollbacks: u32,
    /// Projects whose consent allows local learning.
    pub consented_projects: u32,
    /// Projects whose consent allows anonymised contribution.
    pub contributing_projects: u32,
    /// Milliseconds the last aggregation took. Section 11's budget is 90 s per wedding.
    pub last_update_ms: u64,
}

impl LearnOutline {
    /// The share of captured corrections that could be attributed, `0..1`.
    ///
    /// The number that says whether the loop is *receiving* anything. A build where every override
    /// arrives without a decision behind it has a full correction table and an empty loop, and the
    /// two look identical from the outline's other fields.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn attribution_rate(&self) -> f32 {
        let total = self.corrections + self.unattributed;
        if total == 0 {
            return 0.0;
        }
        f64::from(self.corrections) as f32 / f64::from(total) as f32
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to learn from a photographer.
///
/// **Twenty-eighth service of its kind and the last.** Its subject is the product's own future
/// behaviour, which is why every method that changes anything takes an explicit act by a person:
/// `adopt` and `roll_back` are the only two that move a profile, and there is no third.
///
/// No later phase may keep its own correction table, its own aggregation or its own idea of what an
/// improvement is. Two answers to "what has this photographer taught us" is two profiles that
/// disagree about the same wedding.
pub trait LearnService: Send + Sync + fmt::Debug {
    /// What the loop has seen.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn outline(&self) -> AuraResult<LearnOutline>;

    /// Record one correction, attributed to the decision it corrected.
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11004` when there is no ledger decision behind it, when the value is not
    /// [`Learnable`], or when the project has not consented to local learning.
    fn capture(
        &self,
        correction: &Correction,
        context: &CorrectionContext,
    ) -> AuraResult<Vec<LearnReason>>;

    /// Every bucket, aggregated.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn aggregates(&self, profile: ProfileId) -> AuraResult<Vec<Aggregate>>;

    /// Compute a candidate update, measured on held-out corrections. **Adopts nothing.**
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11003` when the profile is unknown or the fit cannot be measured.
    fn compute(&self, profile: ProfileId) -> AuraResult<Option<LearningUpdate>>;

    /// The two sides a photographer compares, both measured on the same held-out corrections.
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11003` when there is no candidate to compare against.
    fn compare(&self, profile: ProfileId) -> AuraResult<Option<AbComparison>>;

    /// Adopt a candidate. **The only way `adopted` becomes true.**
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11003` when there is no candidate, or when the candidate is not offerable.
    fn adopt(&self, profile: ProfileId) -> AuraResult<LearningUpdate>;

    /// Roll a profile back to its previous version, exactly.
    ///
    /// # Errors
    ///
    /// `AURA-LRN-11005` when there is no earlier version, or when the stored bytes do not restore.
    fn roll_back(&self, profile: ProfileId) -> AuraResult<(u16, Vec<LearnReason>)>;

    /// What a project has consented to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    fn consent(&self, project: ProjectId) -> AuraResult<Consent>;

    /// Record what a project consents to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3005` when the row cannot be written.
    fn set_consent(&self, consent: &Consent) -> AuraResult<()>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bad_update(detail: String) -> AuraError {
    AuraError::new(
        ErrorCode("AURA-LRN-11003"),
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "AURA could not work out an update from your corrections. Nothing about your photographs \
         or your profile has changed.",
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::assertions_on_constants,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::disallowed_methods,
    clippy::panic
)]
mod tests {
    use super::*;

    #[test]
    fn every_code_and_learnable_has_a_distinct_slug() {
        let mut slugs: Vec<&str> = LearnCode::ALL.iter().map(|c| c.as_str()).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), LearnCode::COUNT);
        for code in LearnCode::ALL {
            assert!(!code.user_text().is_empty());
            assert_eq!(LearnCode::parse(code.as_str()).unwrap(), code);
        }

        let mut names: Vec<&str> = Learnable::ALL.iter().map(|l| l.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Learnable::COUNT);
        for l in Learnable::ALL {
            assert_eq!(Learnable::parse(l.as_str()).unwrap(), l);
            assert!(l.ceiling() > 0.0);
        }
    }

    #[test]
    fn nothing_a_guard_owns_is_learnable() {
        // The closed list is the guarantee. If a later phase adds a member here, this test is
        // where it has to argue for it - and these five words are the ones that must never
        // appear.
        for l in Learnable::ALL {
            let name = l.as_str();
            for forbidden in [
                "texture", "identity", "skin", "crop", "cleanup", "mask", "coverage", "tattoo",
            ] {
                assert!(
                    !name.contains(forbidden),
                    "{name} looks like a guarantee rather than a preference"
                );
            }
        }
    }

    #[test]
    fn an_offset_is_bounded_twice() {
        let bucket = CorrectionBucket {
            kind: DecisionKind::Edit,
            scene: SceneId::Unknown,
            learnable: Learnable::Exposure,
            subject_close: false,
        };
        // A bucket that measured a full stop proposes half of it, and the ceiling holds.
        let agg = Aggregate {
            bucket,
            corrections: 40,
            projects: 4,
            outliers_dropped: 2,
            central: 1.0,
            dispersion: 0.1,
            held_out: 10,
            actionable: true,
        };
        assert!((agg.proposed_offset() - 0.5).abs() < 1e-6);

        // A bucket that measured four stops is cut to the ceiling rather than to half of four.
        let agg = Aggregate {
            central: 4.0,
            ..agg
        };
        assert!((agg.proposed_offset() - Learnable::Exposure.ceiling()).abs() < 1e-6);

        // A bucket that is not actionable proposes nothing at all.
        let agg = Aggregate {
            actionable: false,
            ..agg
        };
        assert_eq!(agg.proposed_offset(), 0.0);
    }

    #[test]
    fn a_worse_candidate_reports_no_improvement_rather_than_a_negative_one() {
        let ab = AbComparison {
            profile_id: ProfileId::new(),
            current_version: 3,
            candidate_version: 4,
            current_error: 0.20,
            candidate_error: 0.30,
            held_out: 12,
            rows: Vec::new(),
        };
        assert_eq!(ab.improvement(), 0.0);

        let ab = AbComparison {
            candidate_error: 0.10,
            ..ab
        };
        assert!((ab.improvement() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn an_update_below_the_noise_floor_is_not_offerable() {
        let mut u = LearningUpdate {
            profile_id: ProfileId::new(),
            from_version: 1,
            to_version: 2,
            corrections_used: 40,
            expected_improvement: 0.01,
            diff_summary: vec!["Exposure, indoor ceremony: +0.08 EV".to_owned()],
            adopted: false,
        };
        assert!(!u.is_offerable());
        u.expected_improvement = 0.16;
        assert!(u.is_offerable());
        assert!(u.validate().is_ok());

        // Twelve corrections is the floor; eleven is not enough however good the number looks.
        u.corrections_used = MIN_CORRECTIONS - 1;
        assert!(!u.is_offerable());
    }

    #[test]
    fn an_update_that_does_not_advance_a_version_is_refused() {
        let u = LearningUpdate {
            profile_id: ProfileId::new(),
            from_version: 4,
            to_version: 4,
            corrections_used: 40,
            expected_improvement: 0.2,
            diff_summary: Vec::new(),
            adopted: false,
        };
        assert!(u.validate().is_err());
    }

    #[test]
    fn consent_starts_at_nothing_and_says_so_in_one_predicate() {
        let c = Consent::none(ProjectId::new(), "0.1.0");
        assert!(!c.local_learning);
        assert!(!c.dataset_contribution);
        assert!(!c.anything_leaves());
    }

    #[test]
    fn an_immaterial_correction_is_not_worth_recording() {
        let c = Correction {
            decision_id: DecisionId::new(),
            kind: DecisionKind::Edit,
            before_json: "{}".to_owned(),
            after_json: "{}".to_owned(),
            scene: SceneId::Unknown,
            identity: None,
            magnitude: 0.0001,
            created_at: 0,
        };
        assert!(!c.is_material(Learnable::Exposure));
        let c = Correction {
            magnitude: 0.2,
            ..c
        };
        assert!(c.is_material(Learnable::Exposure));
    }

    #[test]
    fn a_learnable_names_the_decision_kind_it_comes_from() {
        assert_eq!(Learnable::Exposure.decision_kind(), DecisionKind::Edit);
        assert_eq!(Learnable::KeepThreshold.decision_kind(), DecisionKind::Cull);
        assert_eq!(
            Learnable::HeroThreshold.decision_kind(),
            DecisionKind::Curate
        );
    }
}
