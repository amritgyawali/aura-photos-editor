//! FROZEN CONTRACT. Why anything in this product happened, how sure it was, what it looked
//! at - and the append-only record that lets somebody ask again a year later.
//!
//! PHASE-13 section 5 freezes these shapes before any ledger exists. The file is in
//! `aura-core` for the reason every contract since phase 06 is: **every phase writes into
//! it**. Phase 12 decides galleries today, phase 14 will decide edits, phase 20 retouches,
//! phase 27 QC tickets, phase 29 albums and phase 30 exports. A decision model that lived
//! in `aura-explain` would be a decision model that six later crates had to depend on the
//! explainer to speak, and the first one that could not would invent its own.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **This is the phase where the product has to be able to prove what it did.** Every
//! phase since 05 has emitted evidence, and every one of them carried a confidence and a
//! list of reasons because invariant 2 says a decision without an explanation is a bug.
//! That was eight separate vocabularies, eight separate confidence scales and eight
//! separate ideas of what "sure" means. This contract is the one place they meet.
//!
//! Three things are structural here, in the sense that the type system rather than a
//! review comment enforces them:
//!
//! * **A decision cannot be recorded without a reason.** [`LedgerDecision::is_explained`]
//!   is checked by the store before the row is written and by a CHECK inside migration 13,
//!   so invariant 2 is a property of the catalog rather than of a habit. Section 10.1's
//!   coverage gate is 100 % and the only way to hold that is to make the zero-reason case
//!   impossible to persist.
//! * **A reason carries its evidence, and the evidence is never a pixel.** [`Evidence`] is
//!   a crop rectangle, a list of frame ids or a list of named parameter deltas. There is no
//!   variant that could hold an image, which is what makes section 10.1's "support bundle
//!   contains no pixels" a fact about the shape rather than a promise about the exporter.
//! * **Confidence is two numbers, always.** [`LedgerDecision::raw_confidence`] is what the
//!   deciding code believed and [`LedgerDecision::calibrated_confidence`] is what that
//!   belief is worth given how often it has been right. Autonomy reads the second one.
//!   Storing only the calibrated number would make a re-calibration unfalsifiable, and
//!   storing only the raw one would make [`Autonomy`] a guess.
//!
//! ## Five spellings differ from the phase document
//!
//! All five are recorded in `docs/adr/ADR-0027-decision-ledger-and-confidence.md` section 2.
//!
//! * **`Reason` is [`LedgerReason`]** and **`Decision` is [`LedgerDecision`]**. Both names
//!   are already taken in this crate - `Reason` by phase 09's technical reason and
//!   `Decision` by phase 12's keep-or-drop sum - and both are re-exported at the crate
//!   root, so a third meaning of either word would be a compile error in every file that
//!   imports two of them. Ninth phase, ninth rename with a reason.
//! * **`Source` is [`DecisionSource`]**, because phase 07 already froze a `Source` that
//!   says whether a scene label came from a model or a person.
//! * **`Reason::code` is a `String` rather than a `&'static str`.** Section 5 writes
//!   `&'static str`, which cannot survive a round trip through a catalog: the ledger is
//!   read back years later by a build whose `&'static` table has moved on. The discipline
//!   section 12 asks for - "a lint forbids constructing reasons outside the deciding
//!   module" - is kept a different way, and a stronger one: `aura_explain::reason::Catalog`
//!   holds every code every phase can emit, and a decision carrying a code that is not in
//!   it is **refused** rather than stored. `AURA-ML-5054`.
//! * **[`LedgerDecision::project`] is not in section 5.** A ledger row that cannot say
//!   which wedding it belongs to is a row that cannot be compacted, sliced into a support
//!   bundle, or deleted when the photographer deletes the project.
//! * **[`LedgerDecision::supersedes`] is not in section 5 either**, and section 6.3
//!   requires it: "corrections create new decisions that supersede old ones, preserving
//!   history". A pointer rather than a flag on the old row, because the old row is
//!   append-only and an `UPDATE` to mark it superseded is the one write this table does not
//!   allow.
//!
//! ## What this contract deliberately cannot express
//!
//! There is no field for a deletion, a file path, a client name or a face template, and no
//! variant of [`Evidence`] that could carry image bytes. That is not an omission: phase 13
//! is the phase whose output goes to a support engineer, and every one of those would be a
//! thing that left the machine the first time somebody sent in a bundle.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::composition::Box2;
use crate::contract::error::AuraResult;
use crate::contract::ids::{DecisionId, MomentId, ProjectId, SegmentId};
use crate::contract::scene::Timestamp;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// What kind of decision it was
// ---------------------------------------------------------------------------

/// The six kinds of decision this product makes.
///
/// Section 5's list, exactly. Five of the six are phases that do not exist yet, and they
/// are in the vocabulary now on purpose: a closed enum that a later phase extends is an
/// enum every stored row has to be re-read against, and the whole point of freezing this
/// contract in phase 13 rather than in phase 27 is that phase 27 writes into a ledger it
/// did not have to design.
///
/// **Analysis is not on this list.** Phases 09, 10 and 11 measure; they do not decide, and
/// every one of their exit reports says so in the same words. Their output reaches the
/// Explain panel as evidence underneath a [`DecisionKind::Cull`] row rather than as rows of
/// its own, because a ledger with four hundred thousand "this frame is sharp" entries in it
/// is a ledger nobody can search and a size budget nobody can meet.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// Whether a photograph is delivered. Phase 12.
    #[default]
    Cull,
    /// How a photograph is developed. Phase 14 onward.
    Edit,
    /// What was done to a face or a blemish. Phases 20 and 21.
    Retouch,
    /// A quality ticket and its remedy. Phase 27.
    Qc,
    /// An album, a hero frame, a social crop. Phase 29.
    Curate,
    /// What was written out and where it went. Phase 30.
    Export,
}

impl DecisionKind {
    /// Every kind, in the order the pipeline reaches them.
    pub const ALL: [Self; 6] = [
        Self::Cull,
        Self::Edit,
        Self::Retouch,
        Self::Qc,
        Self::Curate,
        Self::Export,
    ];

    /// How many kinds there are.
    pub const COUNT: usize = 6;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cull => "cull",
            Self::Edit => "edit",
            Self::Retouch => "retouch",
            Self::Qc => "qc",
            Self::Curate => "curate",
            Self::Export => "export",
        }
    }

    /// The words a photographer reads on the Explain panel's tab.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Cull => "Selection",
            Self::Edit => "Edit",
            Self::Retouch => "Retouch",
            Self::Qc => "Quality check",
            Self::Curate => "Album",
            Self::Export => "Delivery",
        }
    }

    /// Parse the stored slug. Unknown text is `None`.
    ///
    /// `None` rather than a default, as [`crate::contract::cull::MustHave::parse`] is and
    /// for the same reason: a decision kind read as the wrong one would file a retouch under
    /// a cull, and the store refuses the row instead.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// True when acting on this decision is hard to take back.
    ///
    /// Section 6.4's risk multiplier, as a property of the kind rather than a number in a
    /// config file. A parameter edit is a row in a recipe and reverting it costs a click; a
    /// generative retouch, an album layout sent to a lab and an export that has reached a
    /// client's gallery are not symmetrical with their undo.
    ///
    /// **A cull is not on this list**, and that is worth reading twice: phase 12's whole
    /// design is that a rejection is a row and nothing on disk moves. If that ever stops
    /// being true, this is the line that has to change with it.
    #[must_use]
    pub const fn is_irreversible(self) -> bool {
        matches!(self, Self::Retouch | Self::Curate | Self::Export)
    }
}

impl fmt::Display for DecisionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// What it was about
// ---------------------------------------------------------------------------

/// The thing a decision was made about.
///
/// Section 5's four subjects. A sum rather than four nullable id columns, because a
/// decision is about exactly one thing and a shape that could express two would eventually
/// express two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum DecisionSubject {
    /// One photograph.
    Image(ImageId),
    /// One thing the photographer shot once. Phase 08.
    Moment(MomentId),
    /// One chapter of the story. Phase 07.
    Segment(SegmentId),
    /// The delivered gallery as a whole.
    Gallery(ProjectId),
}

impl DecisionSubject {
    /// The stable slug for which kind of subject this is.
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::Image(_) => "image",
            Self::Moment(_) => "moment",
            Self::Segment(_) => "segment",
            Self::Gallery(_) => "gallery",
        }
    }

    /// The canonical text form of the id, as the catalog stores it.
    #[must_use]
    pub fn id_str(&self) -> String {
        match self {
            Self::Image(id) => id.to_db(),
            Self::Moment(id) => id.to_db(),
            Self::Segment(id) => id.to_db(),
            Self::Gallery(id) => id.to_db(),
        }
    }

    /// Rebuild a subject from the two stored columns.
    ///
    /// `None` when the kind is unknown or the id does not parse as that kind - which is the
    /// same refusal [`DecisionKind::parse`] makes and for the same reason.
    #[must_use]
    pub fn parse(kind: &str, id: &str) -> Option<Self> {
        match kind {
            "image" => ImageId::from_db(id).ok().map(Self::Image),
            "moment" => MomentId::from_db(id).ok().map(Self::Moment),
            "segment" => SegmentId::from_db(id).ok().map(Self::Segment),
            "gallery" => ProjectId::from_db(id).ok().map(Self::Gallery),
            _ => None,
        }
    }

    /// The photograph this is about, when it is about one.
    #[must_use]
    pub const fn image(&self) -> Option<ImageId> {
        match self {
            Self::Image(id) => Some(*id),
            _ => None,
        }
    }
}

impl fmt::Display for DecisionSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind_str(), self.id_str())
    }
}

// ---------------------------------------------------------------------------
// What it looked at
// ---------------------------------------------------------------------------

/// What a reason can point at, so the photographer can check it.
///
/// Section 5's four variants, and the reason the phase exists in one type: an explanation
/// that says "the eyes are soft" is an assertion, and an explanation that shows the crop of
/// the eye is an argument. Section 6.2 calls the second one "the single most trust-building
/// screen in the product".
///
/// **No variant can hold a pixel.** A crop is four numbers in `0..1`, frames are ids, and
/// parameters are names and floats. The panel renders a crop by asking the preview service
/// for the region; the ledger stores the rectangle. That is what makes the support bundle
/// pixel-free by construction rather than by filtering, and section 10.1 tests it anyway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Evidence {
    /// Nothing to point at. The honest answer for a reason about the whole frame.
    #[default]
    None,
    /// A normalised region of the photograph.
    Crop(Box2),
    /// Other photographs - the runner-up, the reference frames, the rest of the burst.
    Frames(Vec<ImageId>),
    /// Named numbers and how far they moved. `("temperature_k", -610.0)`.
    ///
    /// Section 6.2: "the Edit tab shows literal parameter deltas". A pair rather than a
    /// sentence, because a sentence cannot be re-checked against what the develop engine
    /// actually executed and a pair can.
    Params(Vec<(String, f32)>),
}

impl Evidence {
    /// The most frames or parameters one piece of evidence may carry.
    ///
    /// Eight, because the panel shows them and a list longer than that is a list nobody
    /// reads - and because section 11 budgets six megabytes of ledger per thousand images
    /// and evidence is the part of a row that grows without bound if nothing stops it.
    pub const MAX_ITEMS: usize = 8;

    /// The stable slug for which kind of evidence this is.
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Crop(_) => "crop",
            Self::Frames(_) => "frames",
            Self::Params(_) => "params",
        }
    }

    /// True when there is nothing to show.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::None)
    }

    /// The region, when this points at one.
    #[must_use]
    pub const fn crop(&self) -> Option<Box2> {
        match self {
            Self::Crop(area) => Some(*area),
            _ => None,
        }
    }

    /// The frames, when this points at some.
    #[must_use]
    pub fn frames(&self) -> &[ImageId] {
        match self {
            Self::Frames(ids) => ids,
            _ => &[],
        }
    }

    /// The parameters, when this carries some.
    #[must_use]
    pub fn params(&self) -> &[(String, f32)] {
        match self {
            Self::Params(pairs) => pairs,
            _ => &[],
        }
    }

    /// The same evidence with its list cut to [`Evidence::MAX_ITEMS`].
    ///
    /// A method rather than a check in the store, because truncating at the boundary would
    /// mean the panel and the bundle disagreed about what the decision looked at.
    #[must_use]
    pub fn bounded(self) -> Self {
        match self {
            Self::Frames(mut ids) => {
                ids.truncate(Self::MAX_ITEMS);
                Self::Frames(ids)
            }
            Self::Params(mut pairs) => {
                pairs.truncate(Self::MAX_ITEMS);
                Self::Params(pairs)
            }
            other => other,
        }
    }
}

// ---------------------------------------------------------------------------
// Why
// ---------------------------------------------------------------------------

/// One thing that moved a decision, in the words of the code that made it.
///
/// Section 5's frozen shape, with `code` as a `String`; see this module's header for why,
/// and `docs/reason-codes.md` for the reference every code appears in.
///
/// **A reason is produced by the deciding code and never by the explainer.** Section 6.2:
/// "reasons are always produced by the deciding code with real evidence; the language model
/// may only *rephrase* them into prose, never add facts." The enforcement is in
/// `aura_explain::reason::Catalog`: a code that is not in the shipped registry is refused
/// at record time, so a reason invented anywhere - by a summariser, by a UI, by a future
/// phase in a hurry - never reaches the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LedgerReason {
    /// The stable slug. Documented in `docs/reason-codes.md`, and in the registry.
    pub code: String,
    /// The sentence a photographer reads.
    ///
    /// Localisable: the code is what is stored and counted, and the sentence is what a
    /// release can improve. A reason whose text is the registry's own template does not
    /// need to store it, and the store leaves the column empty in that case.
    pub text: String,
    /// How much this moved the decision. Positive toward it, negative against.
    pub weight: f32,
    /// What the photographer can look at to check it.
    pub evidence: Evidence,
}

impl LedgerReason {
    /// A reason with no evidence to show.
    #[must_use]
    pub fn plain(code: impl Into<String>, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code: code.into(),
            text: text.into(),
            weight,
            evidence: Evidence::None,
        }
    }

    /// A reason pointing at a region of the photograph.
    #[must_use]
    pub fn at(code: impl Into<String>, text: impl Into<String>, weight: f32, area: Box2) -> Self {
        Self {
            code: code.into(),
            text: text.into(),
            weight,
            evidence: Evidence::Crop(area.clamped()),
        }
    }

    /// A reason pointing at other photographs.
    #[must_use]
    pub fn about(
        code: impl Into<String>,
        text: impl Into<String>,
        weight: f32,
        frames: Vec<ImageId>,
    ) -> Self {
        Self {
            code: code.into(),
            text: text.into(),
            weight,
            evidence: Evidence::Frames(frames).bounded(),
        }
    }

    /// A reason carrying named parameter deltas.
    #[must_use]
    pub fn with_params(
        code: impl Into<String>,
        text: impl Into<String>,
        weight: f32,
        params: Vec<(String, f32)>,
    ) -> Self {
        Self {
            code: code.into(),
            text: text.into(),
            weight,
            evidence: Evidence::Params(params).bounded(),
        }
    }

    /// True when this reason argued for the decision rather than against it.
    #[must_use]
    pub fn is_for(&self) -> bool {
        self.weight >= 0.0
    }
}

// ---------------------------------------------------------------------------
// How much the product is allowed to act on it
// ---------------------------------------------------------------------------

/// What the product may do with a decision at the confidence it has.
///
/// Section 6.4's four bands. **This is a safety mechanism and not a label**: it is the only
/// thing standing between a calibrated number and phase 28 acting on it unattended, which
/// is why the bands live in a PM-owned config file, why the multipliers are structural, and
/// why the default when anything is unknown is the strictest band rather than the loosest.
///
/// The ordering is deliberate and is the one thing in this type that other code depends on:
/// `Auto < AutoZeroTouch < Suggest < RequireReview`, from most autonomous to least, so
/// "raise one band" is `stricter()` and "at least this much review" is `max`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    /// Applied without asking, in every mode.
    Auto,
    /// Applied without asking only when the photographer has turned Zero-Touch on.
    AutoZeroTouch,
    /// Applied, and put in the review queue so somebody looks.
    Suggest,
    /// Not applied until somebody says so.
    ///
    /// The default, because an unknown band must be the one that asks. A decision whose
    /// autonomy could not be worked out is exactly the decision nobody should have acted on
    /// silently.
    #[default]
    RequireReview,
}

impl Autonomy {
    /// Every band, from most autonomous to least.
    pub const ALL: [Self; 4] = [
        Self::Auto,
        Self::AutoZeroTouch,
        Self::Suggest,
        Self::RequireReview,
    ];

    /// How many bands there are.
    pub const COUNT: usize = 4;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::AutoZeroTouch => "auto_zero_touch",
            Self::Suggest => "suggest",
            Self::RequireReview => "require_review",
        }
    }

    /// The words a photographer reads on the confidence badge.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Auto => "Applied",
            Self::AutoZeroTouch => "Applied in Zero-Touch",
            Self::Suggest => "Applied - worth a look",
            Self::RequireReview => "Waiting for you",
        }
    }

    /// What the badge says when a photographer asks what the band means.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Auto => {
                "AURA is confident enough in this to do it without asking, and you can \
                 change it at any time"
            }
            Self::AutoZeroTouch => {
                "AURA will do this on its own only when you have turned Zero-Touch on; \
                 otherwise it waits"
            }
            Self::Suggest => {
                "AURA has done this and put it in the review queue, because it is not \
                 certain enough to do it quietly"
            }
            Self::RequireReview => {
                "AURA has not done this. It is not confident enough, so it is asking you"
            }
        }
    }

    /// Parse the stored slug. Unknown text is [`Autonomy::RequireReview`].
    ///
    /// The strict direction on purpose, and it is the most important `from_str_or_*` in this
    /// crate: an unreadable band that defaulted to `Auto` would be a build that acted on a
    /// row it could not understand.
    #[must_use]
    pub fn from_str_or_review(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|band| band.as_str() == text)
            .unwrap_or(Self::RequireReview)
    }

    /// True when the product acts on this without being asked in the current mode.
    ///
    /// `zero_touch` is phase 28's switch, and it is an argument rather than a global because
    /// this crate has no state and phase 28 does not exist yet.
    #[must_use]
    pub const fn acts(self, zero_touch: bool) -> bool {
        match self {
            Self::Auto => true,
            Self::AutoZeroTouch => zero_touch,
            Self::Suggest | Self::RequireReview => false,
        }
    }

    /// True when the decision is in a review queue.
    #[must_use]
    pub const fn needs_review(self) -> bool {
        matches!(self, Self::Suggest | Self::RequireReview)
    }

    /// One band more cautious, saturating at [`Autonomy::RequireReview`].
    ///
    /// Section 6.4's two risk multipliers are both this function: an irreversible action and
    /// a decision touching a must-have moment are each raised one band, because the cost of
    /// being wrong is higher and the confidence has not changed.
    #[must_use]
    pub const fn stricter(self) -> Self {
        match self {
            Self::Auto => Self::AutoZeroTouch,
            Self::AutoZeroTouch => Self::Suggest,
            Self::Suggest | Self::RequireReview => Self::RequireReview,
        }
    }
}

impl fmt::Display for Autonomy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Who decided
// ---------------------------------------------------------------------------

/// Where a decision came from.
///
/// Section 6.1: "cloud-sourced decisions are calibrated separately from local ones, because
/// their error profile differs". This enum is what makes that possible, and it is why the
/// calibration key is a pair rather than a kind.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    /// Deterministic code on this machine, from local models. The overwhelming majority.
    #[default]
    Local,
    /// A cloud model proposed it and deterministic code accepted it.
    ///
    /// Never "a cloud model did it": phase 04's rule is that cloud proposes and
    /// deterministic code decides, and a source that said otherwise would be a source that
    /// let an audit trail claim a model executed something.
    Cloud,
    /// The photographer said so. Unbeatable, and calibrated against nothing.
    User,
}

impl DecisionSource {
    /// Every source.
    pub const ALL: [Self; 3] = [Self::Local, Self::Cloud, Self::User];

    /// How many sources there are.
    pub const COUNT: usize = 3;

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
            Self::User => "user",
        }
    }

    /// Parse the stored slug. Unknown text is [`DecisionSource::Local`].
    #[must_use]
    pub fn from_str_or_local(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == text)
            .unwrap_or(Self::Local)
    }

    /// True when a calibration model may be fitted for this source.
    ///
    /// A photographer's own decision is right by definition: there is no outcome to be
    /// wrong about, so fitting a curve to it would be fitting a curve to the word "yes".
    #[must_use]
    pub const fn is_calibratable(self) -> bool {
        matches!(self, Self::Local | Self::Cloud)
    }
}

impl fmt::Display for DecisionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The decision
// ---------------------------------------------------------------------------

/// One recorded decision, and everything needed to argue with it or reproduce it.
///
/// Section 5's frozen shape, plus [`LedgerDecision::project`] and
/// [`LedgerDecision::supersedes`]; see this module's header.
///
/// ## Why `inputs_hash` is here and what it covers
///
/// Section 6.3: "`inputs_hash` covers analysis outputs, config versions and model versions,
/// so replay can assert determinism and detect drift after an upgrade." It is the hash of
/// the *question*, exactly as `SelectionResult::deterministic_hash` is, and the two answer
/// different halves of the same support case: a replay whose inputs hash matches and whose
/// outputs differ is a determinism bug, and one whose inputs hash has moved is an upgrade.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerDecision {
    /// This decision's identity. Assigned once, never reused.
    pub id: DecisionId,
    /// Which wedding it belongs to.
    pub project: ProjectId,
    /// What kind of decision it was.
    pub kind: DecisionKind,
    /// What it was about.
    pub subject: DecisionSubject,
    /// A hash over everything that fed it: the analysis, the config and the model versions.
    pub inputs_hash: u64,
    /// What was decided, as compact JSON.
    ///
    /// JSON rather than a typed column, because six phases with six different outputs write
    /// here and a column per phase would be a table nobody could add to. It is what
    /// [`crate::contract::ledger::ExplainService`]'s replay compares, so it must be
    /// canonical: sorted keys, no whitespace, floats quantised by the producer.
    pub outputs_json: String,
    /// Why, strongest first. Never empty. At most [`LedgerDecision::MAX_REASONS`].
    pub reasons: Vec<LedgerReason>,
    /// What the deciding code believed, `0..1`.
    pub raw_confidence: f32,
    /// What that belief is worth, `0..1`, after the per-type calibration.
    ///
    /// Equal to [`LedgerDecision::raw_confidence`] when the calibration for this
    /// `(kind, source)` is the identity map - which is what ships, at
    /// `calibration_ver = 0`. The two fields exist separately so that a later fit can be
    /// checked against what the code originally said.
    pub calibrated_confidence: f32,
    /// What the product was allowed to do with it.
    pub autonomy: Autonomy,
    /// Where it came from.
    pub source: DecisionSource,
    /// Every model underneath it, as `(name, version)`, sorted by name.
    pub model_versions: Vec<(String, u16)>,
    /// Every config table underneath it, as `(name, version)`, sorted by name.
    pub config_versions: Vec<(String, u16)>,
    /// Which calibration table mapped the confidence. `0` is the unfitted identity map.
    pub calibration_ver: u16,
    /// How long the deciding code took, in milliseconds.
    pub ms: u32,
    /// When it was recorded.
    pub created_at: Timestamp,
    /// The decision this one replaces, when it replaces one.
    ///
    /// Section 6.3. The old row stays exactly as it was written: a correction is a new row
    /// pointing backwards, never an update pointing forwards, because the second one is how
    /// an append-only table quietly stops being append-only.
    pub supersedes: Option<DecisionId>,
}

impl LedgerDecision {
    /// The most reasons one decision carries.
    ///
    /// Six, matching phases 09, 10 and 11 rather than phase 12's four, because this row can
    /// hold a whole pipeline's argument about one frame. Section 12's fourth failure mode is
    /// explanation overload, and the answer there is the panel showing the top three by
    /// weight with "show all" - not a smaller ledger.
    pub const MAX_REASONS: usize = 6;

    /// True when this decision can be recorded at all.
    ///
    /// Invariant 2, as one call. The store checks it, migration 13 checks it again with a
    /// CHECK constraint, and section 10.1's coverage gate is 100 % because between those two
    /// there is no way to persist a decision without an explanation.
    #[must_use]
    pub fn is_explained(&self) -> bool {
        !self.reasons.is_empty()
            && self
                .reasons
                .iter()
                .all(|reason| !reason.text.trim().is_empty())
    }

    /// True when a person has to look at this before anything happens.
    #[must_use]
    pub const fn needs_review(&self) -> bool {
        self.autonomy.needs_review()
    }

    /// The strongest reasons, by absolute weight, up to `limit`.
    ///
    /// The panel's default view. Ties break on the code so two equally weighted reasons come
    /// out in the same order on every machine - the same rule `aura_cull::explain::rank`
    /// uses, for the same determinism reason.
    #[must_use]
    pub fn top_reasons(&self, limit: usize) -> Vec<&LedgerReason> {
        let mut ordered: Vec<&LedgerReason> = self.reasons.iter().collect();
        ordered.sort_by(|left, right| {
            right
                .weight
                .abs()
                .total_cmp(&left.weight.abs())
                .then_with(|| left.code.cmp(&right.code))
        });
        ordered.truncate(limit);
        ordered
    }

    /// How many reasons carry something to look at.
    #[must_use]
    pub fn evidenced(&self) -> usize {
        self.reasons
            .iter()
            .filter(|reason| !reason.evidence.is_empty())
            .count()
    }

    /// True when the calibration that produced [`Self::calibrated_confidence`] was fitted.
    ///
    /// `false` at `calibration_ver = 0`, which is what ships. `AURA-ML-5058` is raised on it
    /// once per run rather than per decision, because an unfitted map is a property of the
    /// build and not of any one photograph.
    #[must_use]
    pub const fn is_calibrated(&self) -> bool {
        self.calibration_ver != Self::UNCALIBRATED
    }

    /// The version of an unfitted, identity-map calibration.
    ///
    /// Named rather than written as `0`, exactly as `KeepScore::UNCALIBRATED` is: "no
    /// calibration" and "calibration table zero" are the same integer and only one of them
    /// is a claim.
    pub const UNCALIBRATED: u16 = 0;
}

/// Anything that can say what it decided.
///
/// Section 5's trait. Implemented by the deciding code - phase 12's engine today - so that
/// the ledger never has to know how a decision was reached, only what it was.
///
/// One method and no `record`: recording is the ledger's job, and a trait that could both
/// produce and persist a decision would be a trait every deciding phase had to hold a
/// database handle to satisfy.
pub trait Explainable {
    /// This decision, in the unified model.
    fn decision(&self) -> LedgerDecision;
}

// ---------------------------------------------------------------------------
// Coverage of the ledger itself
// ---------------------------------------------------------------------------

/// What a project's ledger holds, and what it does not.
///
/// Phase 05's rule, inherited for the ninth time: report coverage when you report a result,
/// and say what the denominator is. This phase's denominator is unusual and worth stating
/// plainly - it is *decisions*, not photographs. A wedding with three thousand photographs
/// and four hundred ledger rows is not 13 % covered; it is a wedding where four hundred
/// decisions were made, and [`LedgerOutline::explained`] over [`LedgerOutline::decisions`]
/// is the number section 10.1 gates at 100 %.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LedgerOutline {
    /// Decisions recorded.
    pub decisions: u32,
    /// Decisions carrying at least one reason with a sentence. The gate is `== decisions`.
    pub explained: u32,
    /// Decisions still in force - that is, not superseded by a later one.
    pub current: u32,
    /// Decisions the photographer's own corrections superseded.
    pub superseded: u32,
    /// How many decisions of each kind, in [`DecisionKind::ALL`] order.
    pub by_kind: [u32; DecisionKind::COUNT],
    /// How many in each autonomy band, in [`Autonomy::ALL`] order.
    pub by_autonomy: [u32; Autonomy::COUNT],
    /// How many from each source, in [`DecisionSource::ALL`] order.
    pub by_source: [u32; DecisionSource::COUNT],
    /// Decisions whose confidence went through a fitted calibration.
    pub calibrated: u32,
    /// Which calibration set produced them. `0` is the unfitted identity map.
    pub calibration_ver: u16,
    /// Reasons carrying something to look at.
    pub evidenced: u32,
    /// Reasons in total.
    pub reasons: u32,
    /// Bytes the ledger occupies for this project.
    ///
    /// Section 11 budgets six megabytes per thousand images for a full pipeline. Measured
    /// rather than estimated, because a budget nobody measures is a budget that is met.
    pub bytes: u64,
}

impl LedgerOutline {
    /// True when nothing has been recorded.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.decisions == 0
    }

    /// Fraction of decisions carrying an explanation, `0..1`. The section 10.1 gate.
    #[must_use]
    pub fn explanation_coverage(&self) -> f32 {
        ratio(self.explained, self.decisions)
    }

    /// Fraction of reasons carrying evidence, `0..1`.
    ///
    /// Not a gate, and deliberately not: a reason like "you asked for this one to be kept"
    /// has nothing to point at, and a build that forced evidence onto it would be a build
    /// that invented a crop.
    #[must_use]
    pub fn evidence_coverage(&self) -> f32 {
        ratio(self.evidenced, self.reasons)
    }

    /// Bytes per thousand decisions, for the size budget.
    #[must_use]
    pub fn bytes_per_thousand(&self) -> u64 {
        if self.decisions == 0 {
            return 0;
        }
        self.bytes
            .saturating_mul(1_000)
            .checked_div(u64::from(self.decisions))
            .unwrap_or(u64::MAX)
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to record a decision, and the one way to read one back.
///
/// Frozen. Implemented by `aura_explain::Explain`.
///
/// The rule phase 05 wrote for `SimilarityIndex` and every phase since has repeated, a
/// ninth time and in the direction that matters most for the ones still to come: **no phase
/// may keep its own record of what it decided.** Phase 27 writes QC decisions here, phase 28
/// reads the autonomy bands from here, and phase 30's learning loop reads the whole table.
/// Two ledgers is two answers to "what did the product do", and the one thing a support case
/// cannot survive is a product that disagrees with itself about its own history.
///
/// **There is no update and no delete.** Section 6.3: append-only, corrections supersede.
/// [`ExplainService::compact`] is the only method that removes anything, it is bounded by a
/// documented policy, and it can never remove a decision a photographer overrode.
pub trait ExplainService: Send + Sync + fmt::Debug {
    /// Record one decision and return its id.
    ///
    /// Calibrates the confidence, resolves the autonomy band and writes the row, in that
    /// order. The `calibrated_confidence` and `autonomy` fields of the argument are
    /// **recomputed** rather than trusted: a caller that could set its own band would be a
    /// caller that could grant itself autonomy.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5054` when the decision carries no reason or a reason whose code is not in
    /// the registry, and `AURA-DB-3006` when the row cannot be written.
    fn record(&self, decision: &LedgerDecision) -> AuraResult<DecisionId>;

    /// One decision, by id.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    fn decision(&self, id: DecisionId) -> AuraResult<Option<LedgerDecision>>;

    /// Every decision about one subject, newest first.
    ///
    /// Includes superseded rows: the history is the point, and a caller that wants only the
    /// current one asks for the first element.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    fn history(&self, subject: DecisionSubject) -> AuraResult<Vec<LedgerDecision>>;

    /// The newest decision of one kind about one subject.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    fn latest(
        &self,
        subject: DecisionSubject,
        kind: DecisionKind,
    ) -> AuraResult<Option<LedgerDecision>>;

    /// What a project's ledger holds.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<LedgerOutline>;

    /// Apply the compaction policy to one project, returning how many rows went.
    ///
    /// Section 6.3: "compaction keeps the newest decision per subject plus all user
    /// overrides". Everything it removes is a superseded row that a newer row already points
    /// past, which is why this is the only method on an append-only table that deletes.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the ledger cannot be compacted.
    fn compact(&self, project: ProjectId) -> AuraResult<u32>;
}

/// `part / whole` as a fraction in `0..1`, with zero for an empty whole.
///
/// Per-thousandth integer arithmetic then one lossless widening, the shape every ratio in
/// this crate uses; `aura-core` does not relax `clippy::cast_precision_loss`.
fn ratio(part: u32, whole: u32) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    let thousandths = u64::from(part)
        .saturating_mul(1_000)
        .checked_div(u64::from(whole))
        .unwrap_or(0)
        .min(1_000);
    f32::from(u16::try_from(thousandths).unwrap_or(1_000)) / 1_000.0
}
