//! Every reason this product can give, in one registry, assembled from the vocabularies
//! themselves.
//!
//! Section 6.2: "every reason code has a documented meaning, a severity, and a localisable
//! sentence template - the reason-code reference is a public doc." This module is that
//! registry, `docs/reason-codes.md` is that public doc, and a test asserts the two agree
//! code for code.
//!
//! ## Why it is assembled rather than written out
//!
//! There are ninety-odd codes across five phases and there will be more. A registry typed
//! out by hand would be a second copy of five frozen enums, and the second copy is the one
//! that goes stale: a phase adds a code, nobody adds it here, and the first decision citing
//! it is refused in production by `AURA-ML-5054`.
//!
//! So [`Catalog::shipped`] walks `ReasonCode::ALL`, `EmotionCode::ALL`,
//! `CompositionCode::ALL` and `CullCode::ALL` and takes each code's slug and its own
//! `user_text` as the template. **The registry cannot disagree with the vocabulary, because
//! it is the vocabulary.** What this module adds is the two things the frozen enums do not
//! carry: which phase owns a code, and how to read it.
//!
//! ## Severity is a reading, not a judgement
//!
//! [`ReasonSeverity`] answers "how should the panel show this", and the four values exist
//! because the panel would otherwise have to decide, in TypeScript, whether
//! `eyes_closed_ok` is good news. It is derived from predicates the frozen contracts
//! already expose - `ReasonCode::is_exoneration`, `EmotionCode::is_caveat`,
//! `CompositionCode::is_exoneration`, `CullCode::is_keep` and `CullCode::is_veto` - so a
//! phase that changes its mind about a code changes it in one place.
//!
//! **[`ReasonSeverity::Caveat`] is the important one.** A caveat is a reason that says
//! *something was not measured*: no scene label, no faces, an unavailable head, an
//! uncalibrated body. Phases 09, 10, 11 and 12 all wrote the same rule into their exit
//! reports - an absent row is "not checked", not "clean" - and this is where that rule
//! becomes visible in a single panel across all four of them.

use std::collections::BTreeMap;
use std::fmt;

use aura_core::contract::colour::ColourCode;
use aura_core::contract::composition::CompositionCode;
use aura_core::contract::cull::CullCode;
use aura_core::contract::curate::CurateCode;
use aura_core::contract::emotion::EmotionCode;
use aura_core::contract::integrity::ReasonCode;
use aura_core::contract::ledger::DecisionKind;
use aura_core::contract::qc::QcCode;
use aura_core::contract::tone::ToneCode;

/// How a reason should be read.
///
/// Four values and not two, because "argued for" and "argued against" cannot express the
/// two states that matter most in this product: a reason that clears a photograph of a
/// suspicion, and a reason that admits nobody looked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ReasonSeverity {
    /// Something good about the photograph. It argued for the decision.
    Credit,
    /// Neutral, or an explicit clearing of a suspicion - `eyes_closed_ok`,
    /// `intentional_motion`, `centred_subject_ok`.
    ///
    /// The panel shows these in the product's ordinary voice rather than as a warning,
    /// which is the whole reason phases 09 and 11 have an `is_exoneration` predicate.
    #[default]
    Note,
    /// Something was not measured, so the decision was made with less than it wanted.
    ///
    /// Lowers confidence and is rendered as a caveat. Never rendered as a fault: "AURA did
    /// not check this" and "AURA checked this and it is bad" are opposite sentences and
    /// showing the first as the second is the failure every phase since 09 has guarded.
    Caveat,
    /// Something is wrong with the photograph. It argued against the decision.
    Fault,
}

impl ReasonSeverity {
    /// Every severity, best news first.
    pub const ALL: [Self; 4] = [Self::Credit, Self::Note, Self::Caveat, Self::Fault];

    /// The stable slug, sent on the wire so the panel styles by it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Credit => "credit",
            Self::Note => "note",
            Self::Caveat => "caveat",
            Self::Fault => "fault",
        }
    }
}

impl fmt::Display for ReasonSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One code, as the registry knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredReason {
    /// The stable slug. What is stored, counted and translated against.
    pub code: &'static str,
    /// Which phase's vocabulary it came from, as a short name the reference groups by.
    pub domain: &'static str,
    /// The decision kind this code can appear under.
    pub kind: DecisionKind,
    /// How to read it.
    pub severity: ReasonSeverity,
    /// The sentence when nothing more specific is known.
    ///
    /// Localisable, and *the* template: a stored reason whose text equals this does not
    /// store its text at all, so changing the wording here changes every historical row's
    /// rendering at once. That is the intended behaviour and the reason phase 09 decided to
    /// store codes rather than sentences.
    pub template: &'static str,
}

/// Every reason code this build documents.
///
/// Ordered by `(domain, code)` so that the reference doc, the panel's grouping and the
/// gate's iteration all walk it identically. A `BTreeMap` rather than a `HashMap` for the
/// determinism rule every crate in this workspace follows.
#[derive(Debug, Clone)]
pub struct Catalog {
    by_code: BTreeMap<&'static str, RegisteredReason>,
}

impl Catalog {
    /// The registry this build ships.
    ///
    /// Assembled from the four frozen vocabularies plus the codes phase 13 owns itself. See
    /// this module's header for why it is assembled rather than written.
    #[must_use]
    // Eight vocabularies, each assembled the same way from its phase's own frozen enum. Splitting
    // this would put the loop and the domain it fills in different functions, which is the one
    // thing a registry that must not go stale should not do.
    #[allow(clippy::too_many_lines)]
    pub fn shipped() -> Self {
        let mut by_code = BTreeMap::new();

        for code in ReasonCode::ALL {
            by_code.insert(
                code.as_str(),
                RegisteredReason {
                    code: code.as_str(),
                    domain: TECHNICAL,
                    kind: DecisionKind::Cull,
                    severity: technical_severity(code),
                    template: code.user_text(),
                },
            );
        }
        for code in EmotionCode::ALL {
            by_code.insert(
                code.as_str(),
                RegisteredReason {
                    code: code.as_str(),
                    domain: EMOTION,
                    kind: DecisionKind::Cull,
                    severity: if code.is_caveat() {
                        ReasonSeverity::Caveat
                    } else {
                        ReasonSeverity::Credit
                    },
                    template: code.user_text(),
                },
            );
        }
        for code in CompositionCode::ALL {
            by_code.insert(
                code.as_str(),
                RegisteredReason {
                    code: code.as_str(),
                    domain: COMPOSITION,
                    kind: DecisionKind::Cull,
                    severity: composition_severity(code),
                    template: code.user_text(),
                },
            );
        }
        for code in CullCode::ALL {
            by_code.insert(
                code.as_str(),
                RegisteredReason {
                    code: code.as_str(),
                    domain: SELECTION,
                    kind: DecisionKind::Cull,
                    severity: selection_severity(code),
                    template: code.user_text(),
                },
            );
        }
        // PHASE-30. `DecisionKind` has had six members since phase 13 and this registry had
        // vocabulary for exactly one of them, so a develop, curation or QC decision recorded
        // through `ExplainService` was refused with `AURA-ML-5054` - which nothing noticed,
        // because nothing had recorded one yet.
        //
        // The learning loop is what noticed: `Learnable::Exposure` maps to `DecisionKind::Edit`,
        // and a correction can only be attributed to a decision that exists. Ten of the fifteen
        // learnable values were unattributable in this build, silently, and the loop would have
        // reported an empty correction table on a wedding somebody had spent an evening
        // correcting.
        //
        // The four vocabularies are assembled the same way the first four are - from the phases'
        // own frozen enums - so they cannot go stale either.
        for code in ToneCode::ALL {
            by_code.insert(
                code.as_str(),
                RegisteredReason {
                    code: code.as_str(),
                    domain: DEVELOP,
                    kind: DecisionKind::Edit,
                    severity: ReasonSeverity::Note,
                    template: code.user_text(),
                },
            );
        }
        for code in ColourCode::ALL {
            by_code.insert(
                code.as_str(),
                RegisteredReason {
                    code: code.as_str(),
                    domain: DEVELOP,
                    kind: DecisionKind::Edit,
                    severity: ReasonSeverity::Note,
                    template: code.user_text(),
                },
            );
        }
        for code in CurateCode::ALL {
            by_code.insert(
                code.as_str(),
                RegisteredReason {
                    code: code.as_str(),
                    domain: CURATION,
                    kind: DecisionKind::Curate,
                    severity: ReasonSeverity::Note,
                    template: code.user_text(),
                },
            );
        }
        for code in QcCode::ALL {
            by_code.insert(
                code.as_str(),
                RegisteredReason {
                    code: code.as_str(),
                    domain: QUALITY,
                    kind: DecisionKind::Qc,
                    // A QC finding is a fault by construction - that is what a ticket is - so
                    // every one of these is a caveat rather than a credit. Phase 27's own
                    // vocabulary carries no positive code.
                    severity: ReasonSeverity::Caveat,
                    template: code.user_text(),
                },
            );
        }

        for entry in LEDGER_CODES {
            by_code.insert(entry.code, entry.clone());
        }

        Self { by_code }
    }

    /// One code, if this build documents it.
    #[must_use]
    pub fn get(&self, code: &str) -> Option<&RegisteredReason> {
        self.by_code.get(code)
    }

    /// True when this build documents the code.
    #[must_use]
    pub fn contains(&self, code: &str) -> bool {
        self.by_code.contains_key(code)
    }

    /// The sentence for a code, or an empty string when it is unknown.
    ///
    /// An empty string rather than a placeholder: a caller that reaches for a template it
    /// does not have must produce nothing rather than "unknown reason", which is a sentence
    /// a photographer would read as an explanation.
    #[must_use]
    pub fn template(&self, code: &str) -> &str {
        self.by_code.get(code).map_or("", |entry| entry.template)
    }

    /// How a code should be read, defaulting to a note.
    #[must_use]
    pub fn severity(&self, code: &str) -> ReasonSeverity {
        self.by_code
            .get(code)
            .map_or(ReasonSeverity::Note, |entry| entry.severity)
    }

    /// Every code, in `(domain, code)` order.
    #[must_use]
    pub fn all(&self) -> Vec<&RegisteredReason> {
        let mut entries: Vec<&RegisteredReason> = self.by_code.values().collect();
        entries.sort_by(|left, right| {
            left.domain
                .cmp(right.domain)
                .then_with(|| left.code.cmp(right.code))
        });
        entries
    }

    /// Every code in one domain, in code order.
    #[must_use]
    pub fn domain(&self, domain: &str) -> Vec<&RegisteredReason> {
        self.all()
            .into_iter()
            .filter(|entry| entry.domain == domain)
            .collect()
    }

    /// How many codes there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_code.len()
    }

    /// True when the registry is empty, which it never is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_code.is_empty()
    }
}

impl Default for Catalog {
    fn default() -> Self {
        Self::shipped()
    }
}

/// Phase 09's vocabulary: whether the frame worked.
pub const TECHNICAL: &str = "technical";
/// Phase 10's: what the frame is worth.
pub const EMOTION: &str = "emotion";
/// Phase 11's: how it is framed.
pub const COMPOSITION: &str = "composition";
/// Phase 12's: whether it is delivered.
pub const SELECTION: &str = "selection";
/// Phase 13's own: things about the record rather than about the photograph.
pub const LEDGER: &str = "ledger";
/// Phases 15 and 16's: how a photograph was developed. PHASE-30.
pub const DEVELOP: &str = "develop";
/// Phase 29's: what a photographer should show. PHASE-30.
pub const CURATION: &str = "curation";
/// Phase 27's: what the product found wrong with its own work. PHASE-30.
pub const QUALITY: &str = "quality";

/// The codes phase 13 owns.
///
/// Five, and every one of them is about the *decision* rather than about the photograph -
/// which is exactly why they cannot come from one of the four analysis vocabularies. A
/// decision that was made without a calibration, or that a photographer replaced, is a fact
/// about the product's own history.
pub const LEDGER_CODES: &[RegisteredReason] = &[
    RegisteredReason {
        code: "user_override",
        domain: LEDGER,
        kind: DecisionKind::Cull,
        severity: ReasonSeverity::Note,
        template: "you decided this one yourself, and AURA has kept your decision",
    },
    RegisteredReason {
        code: "supersedes_earlier",
        domain: LEDGER,
        kind: DecisionKind::Cull,
        severity: ReasonSeverity::Note,
        template: "this replaces an earlier decision about the same thing, which is still on \
                   record",
    },
    RegisteredReason {
        code: "uncalibrated_confidence",
        domain: LEDGER,
        kind: DecisionKind::Cull,
        severity: ReasonSeverity::Caveat,
        template: "AURA has not yet learned how often it is right about this kind of \
                   decision, so read the confidence as a rough guide",
    },
    RegisteredReason {
        code: "raised_for_must_have",
        domain: LEDGER,
        kind: DecisionKind::Cull,
        severity: ReasonSeverity::Caveat,
        template: "this touches a part of the day the gallery may not be missing, so AURA \
                   asked for a closer look than the confidence alone would need",
    },
    RegisteredReason {
        code: "raised_for_irreversible",
        domain: LEDGER,
        kind: DecisionKind::Retouch,
        severity: ReasonSeverity::Caveat,
        template: "this is not easy to undo, so AURA asked for a closer look than the \
                   confidence alone would need",
    },
];

/// How to read one of phase 09's codes.
///
/// Three groups. The exonerations are notes because they clear a suspicion; the four that
/// admit an absence are caveats; everything else is a fault, because phase 09's vocabulary
/// is a list of things that can be wrong with a frame.
fn technical_severity(code: ReasonCode) -> ReasonSeverity {
    if matches!(code, ReasonCode::Clean) {
        return ReasonSeverity::Credit;
    }
    if matches!(
        code,
        ReasonCode::NoSubject | ReasonCode::Uncalibrated | ReasonCode::MixedLight
    ) {
        return ReasonSeverity::Caveat;
    }
    if code.is_exoneration() {
        return ReasonSeverity::Note;
    }
    ReasonSeverity::Fault
}

/// How to read one of phase 11's codes.
///
/// The caveats are tested **before** the exonerations and not after, because phase 11 puts
/// them inside `is_exoneration`: `keypoints_unavailable` does clear a photograph of a
/// suspicion, and it clears it by admitting that nothing was measured. Both facts are true
/// and only one of them belongs on a panel that a photographer reads as reassurance.
fn composition_severity(code: CompositionCode) -> ReasonSeverity {
    if matches!(
        code,
        CompositionCode::NoGeometry
            | CompositionCode::KeypointsUnavailable
            | CompositionCode::NoRule
            | CompositionCode::AestheticUnavailable
            | CompositionCode::HorizonUncertain
    ) {
        return ReasonSeverity::Caveat;
    }
    if matches!(code, CompositionCode::Clean) {
        return ReasonSeverity::Credit;
    }
    if code.is_exoneration() {
        return ReasonSeverity::Note;
    }
    ReasonSeverity::Fault
}

/// How to read one of phase 12's codes.
///
/// A keep is a credit and a veto is a fault; the three explanation codes are caveats; every
/// other rejection is a **note**, and that is the deliberate part. "Another frame of the
/// same moment is stronger" says nothing is wrong with this photograph - it says a better
/// one exists - and rendering it as a fault would tell a photographer that four fifths of
/// their wedding is defective.
fn selection_severity(code: CullCode) -> ReasonSeverity {
    if code.is_keep() {
        return ReasonSeverity::Credit;
    }
    if code.is_veto() {
        return ReasonSeverity::Fault;
    }
    if matches!(
        code,
        CullCode::NotAnalysed | CullCode::NoScene | CullCode::NoMoment
    ) {
        return ReasonSeverity::Caveat;
    }
    ReasonSeverity::Note
}
