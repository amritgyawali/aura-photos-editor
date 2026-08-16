//! FROZEN CONTRACT. Which photographs are delivered, which are not, and why - together
//! with the guarantee that the wedding's story survives the decision.
//!
//! PHASE-12 section 5 freezes these shapes before any engine exists. The file is in
//! `aura-core` for the reason [`crate::contract::integrity`],
//! [`crate::contract::emotion`], [`crate::contract::moment`] and
//! [`crate::contract::composition`] are: the phases that consume a selection are 13 (the
//! explainability ledger), 14 (the develop engine, which only edits survivors), 27 (the QC
//! agent, which swaps in runner-ups), 28 (autopilot), 29 (curation) and 30 (delivery), and
//! none of them needs the quota solver, the diversity pass or the size regression.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **This is the phase where evidence becomes absence.** Every contract since phase 05 has
//! carried the same restraint in a different form - a distance is not a duplicate, a score
//! is not a verdict, an ordering is not a shortlist - and every one of them ended by
//! naming phase 12 as the phase that decides. It has arrived.
//!
//! So the restraint changes shape rather than disappearing. Three things are structural
//! here:
//!
//! * **Nothing in this contract deletes anything.** A rejection is a row with reasons and
//!   a pointer to what was kept instead. [`Rejected::kept_instead`] exists so that the
//!   answer to "why is this frame not in my gallery" is never "it scored 0.41".
//! * **The coverage guard cannot be turned off.** [`CullMode`] shifts thresholds and
//!   k-values and touches nothing the guard reads. Section 6.4: "even `Aggressive` cannot
//!   drop a must-have."
//! * **`Missing` means nobody shot it.** [`Coverage::Missing`] is only ever reported when
//!   no candidate existed at all. A rule that could have been satisfied and was not is a
//!   bug, not a state.
//!
//! ## Ten spellings differ from the phase document
//!
//! All ten are recorded in `docs/adr/ADR-0025-culling-coverage-and-gallery-sizing.md`
//! section 2.
//!
//! * **`ImageId` is `PhotoId`**, aliased rather than duplicated, for the eighth time.
//! * **`Reason` is [`CullReason`]**, carrying a typed [`CullCode`]. `Reason` is already
//!   taken by phase 09 in this same crate, and a free string cannot be counted,
//!   translated or asserted against.
//! * **[`Selected::moment_id`] is optional.** Phase 08 groups *groupable* frames; a frame
//!   with no embedding is in no moment, is still deliverable, and must not be given a
//!   fabricated moment id to satisfy a type.
//! * **[`MustHave`] is defined here**, as a closed twelve-variant vocabulary, because
//!   `coverage_rules.toml` indexes it and a typo in TOML must not silently disable a
//!   guarantee.
//! * **[`Coverage`] is defined here**, as section 6.3's three states.
//! * **[`CullMode`] is defined here**, as section 2.1's three autonomy modes. Zero-Touch
//!   is phase 28 and is deliberately not a fourth variant.
//! * **[`Rejected`] is not in section 5 at all**, and section 2.1 requires it.
//! * **[`CullOutline`] is not in section 5 either.** Phase 05's coverage rule, inherited
//!   for the eighth time.
//! * **[`CullService`] is not in section 5 either.** Six later phases consume a selection.
//! * **[`SelectionResult::deterministic_hash`] is over the *inputs and the config***,
//!   never over the output. It identifies the question, so two machines that computed
//!   different answers to the same question can be told apart from two machines that were
//!   asked different questions.
//!
//! ## Three version fields, and a fourth that is a digest
//!
//! [`KeepScore::calibration_ver`] invalidates every fused score, because the per-scene
//! calibration is what makes 0.8 mean the same thing in a ceremony and on a dance floor.
//! [`SelectionResult::analysis_ver`] invalidates the passes. [`SelectionResult::model_ver`]
//! is the *lowest* model version among the sub-scores that fed the fusion, because a
//! selection is only as current as the least current head underneath it.
//!
//! The fourth thing that invalidates a selection is not a version at all - it is the
//! content of `cull_weights.toml` and `coverage_rules.toml`, which a release can change
//! without touching a line of Rust. That is why the determinism hash covers them, and why
//! `AURA-ML-5048` is raised on a mismatch rather than on a version comparison alone.
//! Eighth phase, eighth version-drift code.
//!
//! Changing anything in this file requires an ADR.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, MomentId, ProjectId};
use crate::contract::scene::{ChapterId, SceneId};

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Modes
// ---------------------------------------------------------------------------

/// How much the photographer wants the engine to decide.
///
/// Section 2.1's three autonomy modes. `Zero-Touch` is named there too and is
/// deliberately **not** a fourth variant: section 2.2 puts it in phase 28, and phase 28's
/// behaviour is a policy about *when to run this engine unattended*, not a fourth setting
/// inside it.
///
/// **A mode shifts preferences and never touches guarantees.** It moves the score floor
/// and the per-moment keeper cap. It has no access to [`MustHave`], to the identity
/// minimums or to the coverage guard, and section 6.4 requires exactly that: "modes shift
/// thresholds and k-values, never the coverage rules - even `Aggressive` cannot drop a
/// must-have."
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CullMode {
    /// Keeps more and flags more. The default for a photographer who has not chosen.
    ///
    /// Default because the failure it prevents is the expensive one. An over-full gallery
    /// costs a photographer ten minutes with the slider; an over-culled one costs a frame
    /// nobody can get back.
    Conservative,
    /// The middle setting, and the one the size regression is calibrated against.
    #[default]
    Balanced,
    /// A tight gallery. Still cannot drop a must-have.
    Aggressive,
}

impl CullMode {
    /// Every mode, from most to least generous.
    pub const ALL: [Self; 3] = [Self::Conservative, Self::Balanced, Self::Aggressive];

    /// How many modes there are.
    pub const COUNT: usize = 3;

    /// The stored text, and the wire slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conservative => "conservative",
            Self::Balanced => "balanced",
            Self::Aggressive => "aggressive",
        }
    }

    /// The words a photographer reads in the mode switcher.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Conservative => "Conservative - keeps more, flags more",
            Self::Balanced => "Balanced",
            Self::Aggressive => "Aggressive - a tighter gallery",
        }
    }

    /// Parse the stored text. Unknown values read as [`CullMode::Balanced`].
    ///
    /// Reading an unknown mode as the middle one rather than as the tightest, for the
    /// reason every `from_str_or_*` in this crate gives and one more: a catalog written by
    /// a newer build that invented a fourth mode must not have its galleries re-culled
    /// aggressively by an older one.
    #[must_use]
    pub fn from_str_or_balanced(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == text)
            .unwrap_or(Self::Balanced)
    }
}

impl fmt::Display for CullMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The score
// ---------------------------------------------------------------------------

/// What one photograph is worth, and what the four things underneath it said.
///
/// Section 5's frozen shape. The four sub-scores are carried beside the fused one rather
/// than folded into it, for the reason every phase since 07 has carried its inputs: a
/// photographer who disagrees with a keeper needs to see *which* of the four disagreed
/// with them, and a support case needs to be able to tell a soft frame from an unloved
/// one.
///
/// **A score is not a decision.** [`Selected`] is the decision, and the two are separate
/// types because a frame can carry a high score and be rejected as a near-duplicate of a
/// higher one, or carry a low score and be kept because it is the only photograph of the
/// vows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KeepScore {
    /// The photograph.
    pub image_id: ImageId,
    /// Phase 09's `technical_score`, `0..1`. Zero when no verdict exists.
    pub technical: f32,
    /// Phase 10's `emotion_score`, `0..1`. Zero when no reading exists.
    pub emotion: f32,
    /// Phase 11's `composition_score`, `0..1`. Zero when no judgement exists.
    pub composition: f32,
    /// Phase 06's prominence-weighted subject presence, `0..1`.
    ///
    /// `ImageSubjects::subject_focus_score` scaled by the hierarchy weight of the most
    /// important identity in the frame, so that a sharp photograph of a vendor and a sharp
    /// photograph of the bride are not the same number. Neutral at `0.5` when no face pass
    /// has run - the value that neither rewards nor punishes a frame for a missing phase.
    pub prominence: f32,
    /// The fused, scene-weighted, calibrated keep score, `0..1`.
    ///
    /// A weighted **geometric** mean in log space, not a sum: section 6.1 requires that a
    /// catastrophic technical failure cannot be rescued by emotion, and only a product has
    /// that property. Zero means a hard veto fired, and [`Selected`] is never built from a
    /// zero.
    pub scene_weighted: f32,
    /// Which scene's weights were used.
    ///
    /// Invariant 7: a stored score that does not say which scene it assumed is not
    /// reproducible. [`SceneId::Unknown`] means the neutral weight row was used.
    pub scene: SceneId,
    /// Which per-scene calibration table mapped the fused number.
    ///
    /// `0` is the identity map, which is what ships: fitting isotonic regression needs
    /// labelled keeper/reject pairs from real weddings and there are none in this
    /// repository. A fitted table gets a non-zero version so the two can never be
    /// confused. ADR-0025 section 3.
    pub calibration_ver: u16,
}

impl KeepScore {
    /// The version of an unfitted, identity-map calibration.
    ///
    /// Named rather than written as `0` at four call sites, because "no calibration" and
    /// "calibration table zero" are the same integer and only one of them is a claim.
    pub const UNCALIBRATED: u16 = 0;

    /// The neutral value for a sub-score whose phase has not run.
    ///
    /// Half, not zero. A missing phase is an absence of evidence and zero is evidence of
    /// absence: a frame with no emotion reading multiplied by zero would be vetoed by a
    /// phase that never looked at it. The one exception is [`KeepScore::technical`], which
    /// is genuinely zero when there is no verdict, because the *analysis pass* refuses to
    /// select a frame nobody has checked and says so with [`CullCode::NotAnalysed`].
    pub const NEUTRAL: f32 = 0.5;

    /// True when a hard veto fired and no fusion happened.
    #[must_use]
    pub fn is_vetoed(&self) -> bool {
        self.scene_weighted <= 0.0
    }

    /// The weakest of the four sub-scores, and which one it was.
    ///
    /// What the panel leads with when a photographer asks why a frame scored badly. The
    /// order is the order [`CullCode`] documents them in, so a tie resolves to the same
    /// answer on every machine.
    #[must_use]
    pub fn weakest(&self) -> (&'static str, f32) {
        let mut worst = ("technical", self.technical);
        for candidate in [
            ("emotion", self.emotion),
            ("composition", self.composition),
            ("prominence", self.prominence),
        ] {
            if candidate.1 < worst.1 {
                worst = candidate;
            }
        }
        worst
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a photograph is in the gallery, or why it is not.
///
/// A closed vocabulary, for the reason phase 09's `ReasonCode` and phase 11's
/// `CompositionCode` are closed: `docs/how-aura-culls.md` is generated against
/// [`CullCode::ALL`] and a test asserts every variant appears there, telemetry counts them
/// (`cull.veto {reason_code, count}`), and a stored row carries the code rather than the
/// sentence so that a release can improve the copy without rewriting a catalog.
///
/// **The list is split in three and the split is meaningful.** A *keep* code says what
/// earned the frame its place. A *reject* code says what it lost to, and every one of them
/// names something a photographer can look at. A *veto* code is a reject that never
/// reached the arithmetic at all, and those three are the only rejections in this phase
/// that are about a single frame in isolation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CullCode {
    // --- keeps ------------------------------------------------------------
    /// The strongest frame of its moment. The ordinary reason a photograph is delivered.
    #[default]
    MomentWinner,
    /// The moment's peak frame from phase 10.
    ///
    /// Section 6.2: the peak "is either selected or explicitly rejected with a reason -
    /// never silently dropped". This is the selected half of that guarantee.
    PeakFrame,
    /// Held because a must-have rule would otherwise be empty.
    ///
    /// The hard promotion of section 6.1, and its text names the rule: "the only frame of
    /// the ring exchange".
    CoverageProtected,
    /// Held so that somebody who should be in the gallery is in the gallery.
    ///
    /// Section 6.3's identity minimums - the rule that prevents "my aunt isn't in the
    /// gallery".
    IdentityCoverage,
    /// Kept because it shows something the other keepers of this moment do not.
    ///
    /// Section 2.1's diversity constraint working in the generous direction: a different
    /// framing, a different part of the sequence or a different set of faces.
    DiversitySpread,
    /// Kept to fill this chapter's target count.
    ChapterQuota,
    /// Kept because the gallery is smaller than the target and this was the best frame
    /// left.
    SizeTarget,
    /// The only candidate that existed for this moment or rule.
    OnlyCandidate,
    /// The photographer said keep it.
    ///
    /// Unbeatable, and re-applied onto every fresh selection rather than excluded from it.
    /// ADR-0025 section 8.
    UserKept,

    // --- vetoes -----------------------------------------------------------
    /// The subject is not in focus. A veto: it never reached fusion.
    VetoOutOfFocus,
    /// The exposure cannot be brought back. A veto.
    VetoExposureLost,
    /// The primary subject's eyes are closed with no reason for them to be. A veto.
    ///
    /// **Only in a posed scene**, and only when phase 09 did not already exonerate the
    /// blink through phase 10's tears path. A veto that fired on a kiss or a prayer would
    /// be this phase's most embarrassing failure.
    VetoEyesClosed,

    // --- rejects ----------------------------------------------------------
    /// Another frame of the same moment was stronger.
    LostMomentRank,
    /// Effectively the same photograph as one that was kept.
    NearDuplicate,
    /// This chapter's target was already met by stronger frames.
    ChapterFull,
    /// Below the score floor for this scene and mode.
    BelowFloor,
    /// The gallery already carries enough frames like this one.
    ///
    /// The diversity cap working in the restrictive direction - section 2.1's "avoid 40
    /// near-identical dance frames".
    DiversityCap,
    /// Dropped to reach the target gallery size.
    SizeTrim,
    /// The photographer said reject it.
    UserRejected,

    // --- explanations -----------------------------------------------------
    /// No phase 09 verdict exists, so the frame was not eligible.
    ///
    /// **An explanation, not a judgement.** "Not checked" is not "not good enough", and a
    /// gallery missing four hundred frames because a preview pass was interrupted must say
    /// which of the two happened. `AURA-ML-5050`.
    NotAnalysed,
    /// The moment's peak frame was rejected, and this says so out loud.
    ///
    /// Section 6.2 forbids dropping a peak silently. When one loses, it loses *with this
    /// code attached* so that the browser can show it and the gate can count it.
    PeakRejected,
    /// The frame is the runner-up its moment's winner points at.
    ///
    /// Carried on the rejection so that phase 27 can find the replacement without
    /// re-running anything.
    RunnerUp,
    /// No scene label, so neutral weights were used. Lowers confidence.
    NoScene,
    /// No moment, so it was ranked on its own. Lowers confidence.
    NoMoment,
}

impl CullCode {
    /// Every code, in the order `docs/how-aura-culls.md` documents them.
    pub const ALL: [Self; 24] = [
        Self::MomentWinner,
        Self::PeakFrame,
        Self::CoverageProtected,
        Self::IdentityCoverage,
        Self::DiversitySpread,
        Self::ChapterQuota,
        Self::SizeTarget,
        Self::OnlyCandidate,
        Self::UserKept,
        Self::VetoOutOfFocus,
        Self::VetoExposureLost,
        Self::VetoEyesClosed,
        Self::LostMomentRank,
        Self::NearDuplicate,
        Self::ChapterFull,
        Self::BelowFloor,
        Self::DiversityCap,
        Self::SizeTrim,
        Self::UserRejected,
        Self::NotAnalysed,
        Self::PeakRejected,
        Self::RunnerUp,
        Self::NoScene,
        Self::NoMoment,
    ];

    /// How many codes there are.
    pub const COUNT: usize = 24;

    /// The three codes that bypass fusion entirely. Section 6.1's hard vetoes.
    pub const VETOES: [Self; 3] = [
        Self::VetoOutOfFocus,
        Self::VetoExposureLost,
        Self::VetoEyesClosed,
    ];

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MomentWinner => "moment_winner",
            Self::PeakFrame => "peak_frame",
            Self::CoverageProtected => "coverage_protected",
            Self::IdentityCoverage => "identity_coverage",
            Self::DiversitySpread => "diversity_spread",
            Self::ChapterQuota => "chapter_quota",
            Self::SizeTarget => "size_target",
            Self::OnlyCandidate => "only_candidate",
            Self::UserKept => "user_kept",
            Self::VetoOutOfFocus => "veto_out_of_focus",
            Self::VetoExposureLost => "veto_exposure_lost",
            Self::VetoEyesClosed => "veto_eyes_closed",
            Self::LostMomentRank => "lost_moment_rank",
            Self::NearDuplicate => "near_duplicate",
            Self::ChapterFull => "chapter_full",
            Self::BelowFloor => "below_floor",
            Self::DiversityCap => "diversity_cap",
            Self::SizeTrim => "size_trim",
            Self::UserRejected => "user_rejected",
            Self::NotAnalysed => "not_analysed",
            Self::PeakRejected => "peak_rejected",
            Self::RunnerUp => "runner_up",
            Self::NoScene => "no_scene",
            Self::NoMoment => "no_moment",
        }
    }

    /// Parse the stored slug. Unknown text is [`CullCode::MomentWinner`].
    #[must_use]
    pub fn from_str_or_winner(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::MomentWinner)
    }

    /// The sentence this code says when nothing more specific is known.
    ///
    /// **Stored rows carry the code, not the sentence.** A reason whose text differs from
    /// this - because it names a rule, a joint, a moment or a rival frame - stores its text
    /// as well, and only then. Phase 09's decision, applied a third time.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::MomentWinner => "the strongest frame of this moment",
            Self::PeakFrame => "the moment of this sequence where the most was happening",
            Self::CoverageProtected => {
                "part of the wedding's story that would otherwise be missing \
                 from the gallery"
            }
            Self::IdentityCoverage => {
                "somebody who should be in the gallery is in very few other \
                 frames"
            }
            Self::DiversitySpread => "it shows something the other keepers from this moment do not",
            Self::ChapterQuota => "this part of the day still had room in the gallery",
            Self::SizeTarget => "the best frame left when the gallery was still short of its size",
            Self::OnlyCandidate => "the only frame there was",
            Self::UserKept => "you asked for this one to be kept",
            Self::VetoOutOfFocus => "the subject is not in focus",
            Self::VetoExposureLost => "the exposure cannot be brought back in editing",
            Self::VetoEyesClosed => {
                "the main subject's eyes are closed and nothing in the moment \
                 explains it"
            }
            Self::LostMomentRank => "another frame of the same moment is stronger",
            Self::NearDuplicate => "effectively the same photograph as one already in the gallery",
            Self::ChapterFull => "this part of the day was already well covered by stronger frames",
            Self::BelowFloor => "it is weaker than this kind of photograph usually needs to be",
            Self::DiversityCap => "the gallery already carries several frames very like this one",
            Self::SizeTrim => "it was the weakest frame left when the gallery reached its size",
            Self::UserRejected => "you asked for this one to be left out",
            Self::NotAnalysed => {
                "AURA has not checked this photograph yet, so it has not been \
                 considered - this is not a judgement about it"
            }
            Self::PeakRejected => {
                "this was the strongest instant of its moment, and a different \
                 frame of the same instant was delivered instead"
            }
            Self::RunnerUp => "the closest alternative to the frame that was delivered",
            Self::NoScene => {
                "AURA does not know what kind of photograph this is, so it was \
                 judged cautiously"
            }
            Self::NoMoment => {
                "this photograph was not grouped with any others, so it was \
                 judged on its own"
            }
        }
    }

    /// True when this code puts a photograph in the gallery.
    #[must_use]
    pub const fn is_keep(self) -> bool {
        matches!(
            self,
            Self::MomentWinner
                | Self::PeakFrame
                | Self::CoverageProtected
                | Self::IdentityCoverage
                | Self::DiversitySpread
                | Self::ChapterQuota
                | Self::SizeTarget
                | Self::OnlyCandidate
                | Self::UserKept
        )
    }

    /// True when this code fired before fusion.
    #[must_use]
    pub const fn is_veto(self) -> bool {
        matches!(
            self,
            Self::VetoOutOfFocus | Self::VetoExposureLost | Self::VetoEyesClosed
        )
    }

    /// True when this code protects a frame the gallery would otherwise lose.
    ///
    /// The two hard promotions. They are the codes a re-selection may never drop, and the
    /// codes the gate counts.
    #[must_use]
    pub const fn is_protection(self) -> bool {
        matches!(self, Self::CoverageProtected | Self::IdentityCoverage)
    }
}

impl fmt::Display for CullCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing that put a photograph in the gallery, or kept it out.
///
/// Invariant 2, and section 2.1's "rejection reasons for every rejected frame". The
/// weight is signed for phase 11's reason: the panel sorts by it, and a sort that has to
/// consult two fields gets written wrong once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CullReason {
    /// Which reason this is.
    pub code: CullCode,
    /// The sentence a photographer reads.
    ///
    /// Written in the product's voice and, wherever it can be, naming the specific thing:
    /// "the only frame of the ring exchange", not "coverage rule 3 triggered".
    pub text: String,
    /// How much this moved the decision. Positive keeps, negative rejects.
    ///
    /// A veto is `-1.0` exactly, because a veto did not move a score - it replaced one.
    pub weight: f32,
}

impl CullReason {
    /// A reason with the code's own sentence.
    #[must_use]
    pub fn plain(code: CullCode, weight: f32) -> Self {
        Self {
            code,
            text: code.user_text().to_string(),
            weight,
        }
    }

    /// A reason with a sentence that names something specific.
    #[must_use]
    pub fn spoken(code: CullCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
        }
    }

    /// True when the stored row can omit the text and rebuild it from the code.
    ///
    /// The size decision migration 12 is built on, expressed once so the store and the
    /// codec cannot disagree about it.
    #[must_use]
    pub fn text_is_default(&self) -> bool {
        self.text == self.code.user_text()
    }
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// The parts of a wedding a gallery may not be missing.
///
/// Section 2.1's list, exactly: rings, vows, kiss, first look, entrances, cake, first
/// dance, family formals, each close-family member, a venue establishing shot and the
/// exit. Eleven named there; twelve here, because "entrances" is two different moments -
/// the processional at the ceremony and the couple's entrance into the reception - and a
/// single rule that either one could satisfy would let a gallery ship with no ceremony
/// entrance because the DJ announced them at dinner.
///
/// **A closed vocabulary because `coverage_rules.toml` indexes it.** A must-have written
/// as a string in a config file is a must-have that a typo disables silently, and section
/// 12's first failure mode is losing the customer forever.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum MustHave {
    /// The staged first sight of one partner by the other.
    FirstLook,
    /// The processional into the ceremony.
    CeremonyEntrance,
    /// Spoken vows.
    #[default]
    Vows,
    /// The ring exchange.
    Rings,
    /// The kiss.
    Kiss,
    /// Posed family groups.
    FamilyFormals,
    /// The couple's entrance into the reception.
    ReceptionEntrance,
    /// The cake.
    Cake,
    /// The first dance.
    FirstDance,
    /// The place itself, without the people.
    VenueEstablishing,
    /// The departure.
    Exit,
    /// Every close-family member, at least the configured number of times.
    ///
    /// The one rule that is about *people* rather than about a point in the day, which is
    /// why it is reported once with an aggregate state and detailed per identity in
    /// [`CoverageReport::identity_coverage`].
    CloseFamily,
}

impl MustHave {
    /// Every must-have, in the order of the wedding day.
    ///
    /// [`MustHave::CloseFamily`] is last because it is not a point in the day.
    pub const ALL: [Self; 12] = [
        Self::FirstLook,
        Self::CeremonyEntrance,
        Self::Vows,
        Self::Rings,
        Self::Kiss,
        Self::FamilyFormals,
        Self::ReceptionEntrance,
        Self::Cake,
        Self::FirstDance,
        Self::VenueEstablishing,
        Self::Exit,
        Self::CloseFamily,
    ];

    /// How many rules there are.
    pub const COUNT: usize = 12;

    /// The stable slug, in the catalog, on the wire and as the key in
    /// `coverage_rules.toml`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstLook => "first_look",
            Self::CeremonyEntrance => "ceremony_entrance",
            Self::Vows => "vows",
            Self::Rings => "rings",
            Self::Kiss => "kiss",
            Self::FamilyFormals => "family_formals",
            Self::ReceptionEntrance => "reception_entrance",
            Self::Cake => "cake",
            Self::FirstDance => "first_dance",
            Self::VenueEstablishing => "venue_establishing",
            Self::Exit => "exit",
            Self::CloseFamily => "close_family",
        }
    }

    /// The words a photographer reads in the coverage panel.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::FirstLook => "The first look",
            Self::CeremonyEntrance => "Walking in",
            Self::Vows => "The vows",
            Self::Rings => "The rings",
            Self::Kiss => "The kiss",
            Self::FamilyFormals => "Family groups",
            Self::ReceptionEntrance => "Entering the reception",
            Self::Cake => "The cake",
            Self::FirstDance => "The first dance",
            Self::VenueEstablishing => "The venue",
            Self::Exit => "The exit",
            Self::CloseFamily => "Close family",
        }
    }

    /// Parse the stored slug. Unknown text is `None`.
    ///
    /// `None` rather than a default, unlike every other `from_str` in this crate, and the
    /// difference is deliberate: an unknown scene read as `Unknown` costs a label, and an
    /// unknown must-have read as `Vows` would silently enforce the wrong guarantee. The
    /// config loader refuses the file instead.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|rule| rule.as_str() == text)
    }

    /// True when this rule is about people rather than about a point in the day.
    #[must_use]
    pub const fn is_identity_rule(self) -> bool {
        matches!(self, Self::CloseFamily)
    }
}

impl fmt::Display for MustHave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How well one must-have rule came out.
///
/// Section 6.3's three states. The difference between the last two is the whole point of
/// the phase: `CoveredWeak` is the engine saying "I found something and I am not confident
/// it is good", and `Missing` is the engine saying "there was nothing to find".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    /// At least the required number of frames, all of them above the floor.
    Covered,
    /// The required number was reached only by force-adding frames below the floor.
    ///
    /// Section 6.3: "force-add the best available candidates even if below threshold,
    /// marking them `CoveredWeak` with a visible warning". The frames *are* in the
    /// gallery; the warning is about their quality, not about their presence.
    CoveredWeak,
    /// No candidate existed at all.
    ///
    /// **The photographer never shot it, or the analysis never reached it.** Section 6.3:
    /// "the product must be clear that it cannot invent coverage." This is the default
    /// because an unevaluated rule has found nothing, and a rule that reports `Covered`
    /// before it has run would be the worst possible default in this file.
    #[default]
    Missing,
}

impl Coverage {
    /// Every state, best first.
    pub const ALL: [Self; 3] = [Self::Covered, Self::CoveredWeak, Self::Missing];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Covered => "covered",
            Self::CoveredWeak => "covered_weak",
            Self::Missing => "missing",
        }
    }

    /// Parse the stored slug. Unknown text is [`Coverage::Missing`].
    ///
    /// The pessimistic direction on purpose: an unreadable coverage state shown as a gap
    /// sends somebody to look, and one shown as covered does not.
    #[must_use]
    pub fn from_str_or_missing(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == text)
            .unwrap_or(Self::Missing)
    }

    /// True when the gallery contains the rule's frames, however weakly.
    #[must_use]
    pub const fn is_satisfied(self) -> bool {
        matches!(self, Self::Covered | Self::CoveredWeak)
    }
}

impl fmt::Display for Coverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the gallery guarantees, and where it could not.
///
/// Section 5's frozen shape. The warnings are free text on purpose - they name specific
/// identities, rules and counts, and a closed vocabulary that could express "Priya appears
/// once and close family should appear three times" would be a sentence template with four
/// holes in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoverageReport {
    /// Every must-have and how it came out, in [`MustHave::ALL`] order.
    pub must_haves: Vec<(MustHave, Coverage)>,
    /// How many frames each identity appears in, within the gallery.
    ///
    /// Every identity the project knows about, including the ones at zero - because the
    /// zero is the number the panel exists to show.
    pub identity_coverage: Vec<(IdentityId, u32)>,
    /// Per chapter: how many frames were delivered, and how many were targeted.
    pub chapter_counts: Vec<(ChapterId, u32, u32)>,
    /// Everything a photographer should read before delivering.
    pub warnings: Vec<String>,
}

impl CoverageReport {
    /// The rules that were not satisfied at all.
    #[must_use]
    pub fn missing(&self) -> Vec<MustHave> {
        self.must_haves
            .iter()
            .filter(|(_, state)| *state == Coverage::Missing)
            .map(|(rule, _)| *rule)
            .collect()
    }

    /// The rules that were satisfied only by force-adding weak frames.
    #[must_use]
    pub fn weak(&self) -> Vec<MustHave> {
        self.must_haves
            .iter()
            .filter(|(_, state)| *state == Coverage::CoveredWeak)
            .map(|(rule, _)| *rule)
            .collect()
    }

    /// True when every rule that had a candidate was satisfied.
    ///
    /// **This is the gate**, and it is deliberately not "every rule is `Covered`". Section
    /// 10.1: "zero missed must-haves on all fixtures **whenever candidates exist**;
    /// `Missing` reported honestly when they do not." A wedding with no cake cannot be
    /// made to have one, and a build that failed its own gate over that would be a build
    /// that lies about the cake.
    #[must_use]
    pub fn satisfied_where_possible(&self, had_candidates: &[MustHave]) -> bool {
        self.must_haves
            .iter()
            .filter(|(rule, _)| had_candidates.contains(rule))
            .all(|(_, state)| state.is_satisfied())
    }

    /// How many identities appear fewer than `minimum` times.
    #[must_use]
    pub fn under_covered_identities(&self, minimum: u32) -> usize {
        self.identity_coverage
            .iter()
            .filter(|(_, count)| *count < minimum)
            .count()
    }
}

// ---------------------------------------------------------------------------
// The decision
// ---------------------------------------------------------------------------

/// One photograph that is in the gallery.
///
/// Section 5's frozen shape, with [`Selected::moment_id`] made optional; see this module's
/// header.
#[derive(Debug, Clone, PartialEq)]
pub struct Selected {
    /// The photograph.
    pub image_id: ImageId,
    /// The moment it came from, when it is in one.
    pub moment_id: Option<MomentId>,
    /// Its fused score, `0..1`. Repeated here so a caller reading a gallery does not need
    /// a second lookup per frame.
    pub keep_score: f32,
    /// How sure this decision is, `0..1`. Invariant 2.
    ///
    /// Lowered by a missing scene label, a missing moment, a low-confidence sub-score and
    /// a forced weak promotion. A frame kept at confidence 0.3 is a frame the panel should
    /// offer for review, and section 2.1's Conservative mode "flags more" by lowering the
    /// bar at which it does.
    pub confidence: f32,
    /// Why it is here, strongest first. At most [`Selected::MAX_REASONS`]. Invariant 2.
    pub reasons: Vec<CullReason>,
    /// The best alternative in the same moment, when there is one.
    ///
    /// Section 2.2 puts the replacing in phase 27; this is the pointer it consumes. `None`
    /// when the moment held one frame - which is honest, and is not the same as "there was
    /// nothing as good".
    pub runner_up: Option<ImageId>,
    /// Which guarantee protects this frame, when one does.
    ///
    /// Set for exactly the frames whose reasons include [`CullCode::CoverageProtected`] or
    /// [`CullCode::IdentityCoverage`], and it is a separate field rather than a search
    /// through the reasons because the sizing pass reads it on every frame it considers
    /// dropping.
    pub coverage_role: Option<MustHave>,
}

impl Selected {
    /// The most reasons one keeper carries.
    ///
    /// Four, one fewer than the six phases 09, 10 and 11 allow, because a keeper with six
    /// reasons is a keeper nobody believes. The ranking that picks the four is part of the
    /// explanation.
    pub const MAX_REASONS: usize = 4;

    /// True when a coverage guarantee is holding this frame in the gallery.
    ///
    /// What the sizing pass checks before dropping anything. Section 6.4: shrinking never
    /// breaks a must-have.
    #[must_use]
    pub const fn is_protected(&self) -> bool {
        self.coverage_role.is_some()
    }

    /// True when the photographer put this frame here by hand.
    #[must_use]
    pub fn is_user_kept(&self) -> bool {
        self.reasons
            .iter()
            .any(|reason| reason.code == CullCode::UserKept)
    }
}

/// One photograph that is not in the gallery, and what it lost to.
///
/// Not in section 5; section 2.1 requires it. The pointer matters as much as the reason:
/// "another frame of the same moment is stronger" is an explanation, and "*this* frame is
/// stronger" is an explanation a photographer can check in one click.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejected {
    /// The photograph.
    pub image_id: ImageId,
    /// The moment it came from, when it is in one.
    pub moment_id: Option<MomentId>,
    /// Its fused score, `0..1`. Zero when a veto fired.
    pub keep_score: f32,
    /// Why it is not here, strongest first. At most [`Rejected::MAX_REASONS`]. Invariant 2.
    ///
    /// Never empty. Section 10.1: "every rejected frame has at least one human-readable
    /// reason", and `SelectionResult::is_explained` is how the gate checks it.
    pub reasons: Vec<CullReason>,
    /// The frame that is in the gallery instead, when there is a specific one.
    ///
    /// `None` for a veto - a frame nobody would have delivered lost to nothing - and for a
    /// size trim from a chapter where several frames were dropped together.
    pub kept_instead: Option<ImageId>,
}

impl Rejected {
    /// The most reasons one rejection carries.
    pub const MAX_REASONS: usize = 4;

    /// True when this frame never reached the arithmetic.
    #[must_use]
    pub fn is_vetoed(&self) -> bool {
        self.reasons.iter().any(|reason| reason.code.is_veto())
    }

    /// True when this frame was its moment's peak.
    ///
    /// Section 6.2's guarantee, made checkable: a peak that was dropped carries
    /// [`CullCode::PeakRejected`] and a peak that was dropped *silently* is a bug the gate
    /// catches.
    #[must_use]
    pub fn was_peak(&self) -> bool {
        self.reasons
            .iter()
            .any(|reason| reason.code == CullCode::PeakRejected)
    }
}

/// Everything one run of the culling engine decided.
///
/// Section 5's frozen shape, plus the three provenance fields this module's header
/// explains.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SelectionResult {
    /// The gallery, ordered by timeline time.
    ///
    /// Timeline order rather than score order, because a gallery is a story and a gallery
    /// sorted by score is a highlight reel. Phase 29 does highlight reels.
    pub selected: Vec<Selected>,
    /// Everything else, ordered by timeline time.
    pub rejected: Vec<Rejected>,
    /// What the gallery guarantees.
    pub coverage: CoverageReport,
    /// How many frames were aimed for.
    pub target_count: u32,
    /// How many were delivered.
    ///
    /// Equal to `selected.len()`. It differs from [`SelectionResult::target_count`]
    /// whenever coverage forced frames in or there were not enough candidates, and the
    /// panel shows both because the difference is information.
    pub actual_count: u32,
    /// Which autonomy mode produced it.
    pub mode: CullMode,
    /// A hash over the inputs and the configuration, never over the output.
    ///
    /// Section 6.4: "a `deterministic_hash` over inputs+config recorded in the ledger so a
    /// support case can be reproduced exactly." It identifies the *question*: two machines
    /// with the same hash and different galleries is a determinism bug, and two machines
    /// with different hashes were asked different things.
    pub deterministic_hash: u64,
    /// The lowest model version among the sub-scores that fed the fusion.
    ///
    /// A selection is only as current as the least current head underneath it.
    pub model_ver: u16,
    /// Which build's passes produced it.
    pub analysis_ver: u16,
    /// Which per-scene calibration mapped the fused scores.
    pub calibration_ver: u16,
}

impl SelectionResult {
    /// How many frames were considered.
    #[must_use]
    pub fn considered(&self) -> usize {
        self.selected.len() + self.rejected.len()
    }

    /// What fraction of the considered frames were delivered, `0..1`.
    ///
    /// Section 6.4 expects 22-38 % on a typical wedding. A number far outside that band is
    /// not wrong - a wedding of four hundred frames is mostly keepers - but it is the
    /// number the sizing telemetry watches.
    ///
    /// Per-thousandth integer arithmetic then one lossless widening, which is the shape
    /// every ratio in this crate uses. `aura-core` does not relax
    /// `clippy::cast_precision_loss` the way the ML crates do.
    #[must_use]
    pub fn keeper_rate(&self) -> f32 {
        ratio(self.selected.len(), self.considered())
    }

    /// True when every decision in both directions carries a reason.
    ///
    /// Invariant 2 and section 10.1's last criterion, as one call. The gate asserts it and
    /// so does a unit test on every fixture, because a decision without an explanation is
    /// a bug and this is the phase where it would be an expensive one.
    #[must_use]
    pub fn is_explained(&self) -> bool {
        self.selected.iter().all(|keep| !keep.reasons.is_empty())
            && self.rejected.iter().all(|drop| !drop.reasons.is_empty())
    }

    /// The frames a coverage guarantee is holding in the gallery.
    #[must_use]
    pub fn protected(&self) -> Vec<&Selected> {
        self.selected
            .iter()
            .filter(|keep| keep.is_protected())
            .collect()
    }

    /// How many rejections carry each code, in [`CullCode::ALL`] order.
    ///
    /// Section 11's `cull.veto {reason_code, count}` event, computed once. Counts *every*
    /// reason on every rejection rather than the first, so the sum exceeds the number of
    /// rejected frames and the histogram is documented as an upper bound.
    #[must_use]
    pub fn reject_histogram(&self) -> [u32; CullCode::COUNT] {
        let mut counts = [0u32; CullCode::COUNT];
        for drop in &self.rejected {
            for reason in &drop.reasons {
                if let Some(slot) = CullCode::ALL
                    .iter()
                    .position(|code| *code == reason.code)
                    .and_then(|index| counts.get_mut(index))
                {
                    *slot = slot.saturating_add(1);
                }
            }
        }
        counts
    }

    /// The selection as a lookup, for a caller that asks about one photograph at a time.
    #[must_use]
    pub fn by_image(&self) -> BTreeMap<ImageId, &Selected> {
        self.selected
            .iter()
            .map(|keep| (keep.image_id, keep))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Coverage of the pass itself
// ---------------------------------------------------------------------------

/// What a project's cull covered, and what it was drawn over.
///
/// Not in section 5. Phase 05's rule, inherited for the eighth time: report coverage when
/// you report a result, and say what the denominator is.
///
/// This phase's denominators are the most consequential in the product, because a cull
/// drawn over a partially analysed wedding is a gallery with a hole in it that looks
/// exactly like a gallery with a decision in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CullOutline {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs that were eligible - that is, that carried a phase 09 verdict.
    ///
    /// The denominator of [`CullOutline::coverage`]. A frame with no verdict is not
    /// rejected; it is [`CullCode::NotAnalysed`], and the difference is the whole reason
    /// this field exists.
    pub eligible: u32,
    /// Photographs in the gallery.
    pub selected: u32,
    /// Fraction of the project that was eligible to be culled, `0..1`.
    ///
    /// **The number that matters most in this phase.** A wedding at 60 % coverage has had
    /// 40 % of its frames excluded by an interrupted analysis rather than by a decision,
    /// and a photographer who delivers that gallery has delivered six hours of a wedding.
    pub coverage: f32,
    /// Fraction of *eligible* frames that also carried an emotion reading, `0..1`.
    ///
    /// The second denominator, and the one that says how much of the fusion was real. A
    /// wedding at 100 % coverage and 3 % emotion-aware has been culled on technical
    /// quality and framing alone.
    pub emotion_aware: f32,
    /// Fraction of *eligible* frames that also carried a composition judgement, `0..1`.
    pub composition_aware: f32,
    /// Fraction of *eligible* frames that belonged to a moment, `0..1`.
    ///
    /// The third, and the one the moment pass depends on. Frames outside a moment are
    /// ranked on their own, which is a weaker decision and is reported as one.
    pub grouped: f32,
    /// How many must-haves came out `Covered`.
    pub covered: u32,
    /// How many came out `CoveredWeak`.
    pub covered_weak: u32,
    /// How many came out `Missing`.
    pub missing: u32,
    /// Frames the photographer forced into the gallery.
    pub user_kept: u32,
    /// Frames the photographer forced out of it.
    pub user_rejected: u32,
    /// Which mode produced the stored selection.
    pub mode: CullMode,
    /// The stored selection's determinism hash, or zero when there is none.
    pub deterministic_hash: u64,
    /// The lowest model version underneath the stored scores.
    pub model_ver: u16,
    /// Which build's passes produced them.
    pub analysis_ver: u16,
    /// Which calibration mapped them.
    pub calibration_ver: u16,
}

impl CullOutline {
    /// True when no selection has been stored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.selected == 0 && self.eligible == 0
    }

    /// Fraction of eligible frames that are in the gallery, `0..1`.
    #[must_use]
    pub fn keeper_rate(&self) -> f32 {
        ratio(
            usize::try_from(self.selected).unwrap_or(usize::MAX),
            usize::try_from(self.eligible).unwrap_or(usize::MAX),
        )
    }

    /// True when the three version fields match this build's.
    ///
    /// The check `AURA-ML-5048` is raised on.
    #[must_use]
    pub const fn matches_versions(&self, model: u16, analysis: u16, calibration: u16) -> bool {
        self.model_ver == model
            && self.analysis_ver == analysis
            && self.calibration_ver == calibration
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what is being delivered.
///
/// Frozen. Implemented by `aura_cull::Cull`, and the only route from any later phase to a
/// keeper, a rejection, a runner-up or a coverage report.
///
/// The rule phase 05 wrote for `SimilarityIndex`, phase 06 for `PeopleService`, phase 07
/// for `StoryService`, phase 08 for `MomentService`, phase 09 for `IntegrityService`,
/// phase 10 for `EmotionService` and phase 11 for `CompositionService`, an eighth time and
/// with the highest stakes it has had: **no phase may keep its own idea of what is
/// delivered.** Phase 14 edits survivors, phase 27 swaps in runner-ups, phase 29 builds
/// albums out of keepers and phase 30 uploads them. Two answers to "what is in this
/// gallery" is a delivery that does not match the album that does not match the invoice.
pub trait CullService: Send + Sync + fmt::Debug {
    /// What a project's cull covered and guaranteed.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the selection cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<CullOutline>;

    /// The stored selection, or `None` when the project has not been culled.
    ///
    /// `None` means **nobody has decided**, and it is not an empty gallery. Every consumer
    /// of this trait is expected to render the difference; phase 30 in particular must
    /// never read it as "deliver nothing".
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the selection cannot be read.
    fn selection(&self, project: ProjectId) -> AuraResult<Option<SelectionResult>>;

    /// What was decided about one photograph, and why.
    ///
    /// `Ok(None)` when the photograph is in a project that has not been culled. A
    /// photograph that was considered and rejected returns `Some` with a rejection, which
    /// is the case the Explain panel exists for.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the decision cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<Decision>>;

    /// Re-run the allocation passes at a new target size.
    ///
    /// Section 6.4: "the size slider re-runs only the allocation passes (not analysis), so
    /// it feels instant; the coverage guard always runs last so shrinking never breaks
    /// must-haves." Section 11 budgets two seconds for it.
    ///
    /// `target` is the number of frames asked for. The result's `actual_count` may exceed
    /// it, because coverage runs last and a guarantee outranks a slider.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5049` when the project has no stored scores to re-allocate, and whatever
    /// the store raised.
    fn resize(&self, project: ProjectId, target: u32) -> Result<SelectionResult, AuraError>;

    /// Re-run the allocation passes in a different autonomy mode.
    ///
    /// The same cost as [`CullService::resize`] and the same guarantee: a mode change
    /// cannot break a must-have.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5049` when the project has no stored scores to re-allocate.
    fn set_mode(&self, project: ProjectId, mode: CullMode) -> Result<SelectionResult, AuraError>;

    /// Record that the photographer wants a frame kept, or dropped.
    ///
    /// Unbeatable, and **re-applied** onto every fresh selection rather than excluded from
    /// the input: the check is inside the statement a re-selection would overwrite the row
    /// with, exactly as `identities.user_locked`, `segments.user_locked`,
    /// `moments.user_locked`, `image_integrity.user_reviewed` and
    /// `image_composition.user_reviewed` are. Sixth phase, sixth unbeatable decision.
    ///
    /// A forced keep counts toward the chapter quota and toward coverage. A forced reject
    /// **cannot** break a must-have: if the rejected frame was a rule's only candidate the
    /// rule degrades to [`Coverage::Missing`] with a warning that names the override.
    /// ADR-0025 section 8.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5049` when the photograph is unknown or its project has not been culled.
    fn override_decision(&self, image: ImageId, keep: bool) -> Result<(), AuraError>;

    /// Withdraw an override, returning the frame to the engine's decision.
    ///
    /// Separate from [`CullService::override_decision`] rather than a third value on it,
    /// because "I have no opinion" and "I want this rejected" are different statements and
    /// a tri-state boolean is how they get conflated.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5049` when the photograph is unknown or carries no override.
    fn clear_override(&self, image: ImageId) -> Result<(), AuraError>;
}

/// What was decided about one photograph.
///
/// A sum rather than two nullable fields, because a photograph is in the gallery or it is
/// not, and a shape that could express both at once is a shape that will one day express
/// both at once.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// It is in the gallery.
    Keep(Box<Selected>),
    /// It is not.
    Drop(Box<Rejected>),
}

impl Decision {
    /// The photograph this is about.
    #[must_use]
    pub fn image_id(&self) -> ImageId {
        match self {
            Self::Keep(keep) => keep.image_id,
            Self::Drop(drop) => drop.image_id,
        }
    }

    /// True when the photograph is in the gallery.
    #[must_use]
    pub const fn is_keep(&self) -> bool {
        matches!(self, Self::Keep(_))
    }

    /// The reasons behind it, whichever way it went.
    #[must_use]
    pub fn reasons(&self) -> &[CullReason] {
        match self {
            Self::Keep(keep) => &keep.reasons,
            Self::Drop(drop) => &drop.reasons,
        }
    }
}

/// `part / whole` as a fraction in `0..1`, with zero for an empty whole.
///
/// Per-thousandth integer arithmetic then one lossless widening, which is the shape
/// `MomentOutline::mean_size` established and which every ratio in this crate uses.
/// `aura-core` does not relax `clippy::cast_precision_loss`: it is the crate every other
/// crate reads, and a silent `usize as f32` here would be a rounding decision that six
/// later phases inherited without knowing.
fn ratio(part: usize, whole: usize) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    let thousandths = part
        .saturating_mul(1_000)
        .checked_div(whole)
        .unwrap_or(0)
        .min(1_000);
    f32::from(u16::try_from(thousandths).unwrap_or(1_000)) / 1_000.0
}
