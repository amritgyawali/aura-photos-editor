//! Why, in a sentence a photographer can argue with - and the hash that makes the
//! argument reproducible.
//!
//! Two jobs live here, and they are the same job at two scales.
//!
//! **Per frame**: invariant 2, and section 2.1's "rejection reasons for every rejected
//! frame". Every decision in both directions carries at least one reason, at most four, in
//! an order that puts the one a photographer would ask about first.
//!
//! **Per run**: section 6.4's "`deterministic_hash` over inputs+config recorded in the
//! ledger so a support case can be reproduced exactly".
//!
//! ## The hash covers the question, never the answer
//!
//! This is the decision worth reading twice. A hash of the *output* would tell you that two
//! galleries differ, which you already knew because you were looking at them. A hash of the
//! *inputs and the configuration* tells you something you cannot otherwise find out: whether
//! the two machines were asked the same question.
//!
//! Two runs with the same hash and different galleries is a determinism bug, and it is the
//! only thing that can be. Two runs with different hashes were given different scores,
//! different config, a different mode or a different target - and the support case is
//! *which*, not *why*.
//!
//! ## Why the config digest is inside it
//!
//! Because a release can change a threshold in `cull_weights.toml` without touching a line
//! of Rust and without bumping any version number a human has to remember. Three version
//! columns cover the code; the digest covers the two files, and together they are the four
//! things `AURA-ML-5048` compares.
//!
//! ## Why reasons store a code and not a sentence
//!
//! Phase 09's decision, applied a third time. A stored sentence is copy that a release can
//! improve, a catalog full of English cannot be translated, and four thousand rows of
//! "another frame of the same moment is stronger" is a measurable amount of a photographer's
//! disk. A reason whose text differs from its code's own - because it names a rule, a rival
//! frame or a count - stores the text as well, and only then.

use std::collections::BTreeMap;

use aura_core::contract::cull::ImageId;
use aura_core::{CullCode, CullMode, CullReason, KeepScore, MustHave};

use crate::input::CullInput;
use crate::rules::RuleTable;
use crate::weights::WeightTable;

/// The digest of both configuration tables, as one hex string.
///
/// Order matters and is fixed: weights then rules. A digest that depended on which table
/// was loaded first would be a digest that changed for no reason.
#[must_use]
pub fn config_digest(weights: &WeightTable, rules: &RuleTable) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(weights.digest().as_bytes());
    hasher.update(rules.digest().as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// The hash of one question.
///
/// Covers every input that can change a gallery: each candidate's identity, its four
/// sub-scores, its scene, its chapter, its moment, its veto and its timestamp; the mode;
/// the target; the analysis version; and the digest of both configuration files.
///
/// Floats are hashed through [`f32::to_bits`] after being quantised to a thousandth. The
/// quantisation is deliberate: a sub-score that arrives as `0.640_000_1` on one machine and
/// `0.639_999_9` on another describes the same photograph, and a hash that called those two
/// different questions would report a determinism failure on every cross-platform run.
#[must_use]
pub fn deterministic_hash(
    input: &CullInput,
    scores: &BTreeMap<ImageId, KeepScore>,
    config: &str,
    mode: CullMode,
    target: u32,
    analysis_ver: u16,
) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"aura-cull-v1");
    hasher.update(config.as_bytes());
    hasher.update(mode.as_str().as_bytes());
    hasher.update(&target.to_le_bytes());
    hasher.update(&analysis_ver.to_le_bytes());
    hasher.update(&input.model_ver.to_le_bytes());

    // `input.candidates` is sorted by (timeline_ts, image_id) before this is called, so
    // the walk order is a property of the data rather than of the map implementation.
    for candidate in &input.candidates {
        hasher.update(candidate.image_id.to_db().as_bytes());
        hasher.update(&candidate.timeline_ts.to_le_bytes());
        hasher.update(candidate.scene.as_str().as_bytes());
        hasher.update(candidate.chapter.as_str().as_bytes());
        hasher.update(&[u8::from(candidate.analysed), u8::from(candidate.is_peak)]);
        hasher.update(candidate.veto.map_or("none", CullCode::as_str).as_bytes());
        for value in candidate.sub_scores() {
            hasher.update(&quantise(value).to_le_bytes());
        }
        if let Some(score) = scores.get(&candidate.image_id) {
            hasher.update(&quantise(score.scene_weighted).to_le_bytes());
        }
    }
    for moment in &input.moments {
        hasher.update(moment.id.to_db().as_bytes());
        hasher.update(&quantise(moment.diversity).to_le_bytes());
        hasher.update(&quantise(moment.significance).to_le_bytes());
    }
    for identity in &input.identities {
        hasher.update(identity.id.to_db().as_bytes());
        hasher.update(&identity.appearances.to_le_bytes());
        hasher.update(&[u8::from(identity.primary), u8::from(identity.close_family)]);
    }

    let digest = hasher.finalize();
    let bytes = digest.as_bytes();
    let mut first = [0u8; 8];
    first.copy_from_slice(bytes.get(..8).unwrap_or(&[0; 8]));
    u64::from_le_bytes(first)
}

/// One float, as the integer the hash sees.
fn quantise(value: f32) -> i32 {
    (value.clamp(0.0, 1.0) * 1_000.0 + 0.5) as i32
}

/// Put reasons in the order a photographer reads them and cut the list to `limit`.
///
/// Strongest first by absolute weight, with the code's position in `CullCode::ALL` as the
/// tie-break so that two equally weighted reasons come out in the same order every time.
/// The truncation is part of the explanation: a panel that lists nine reasons is a panel
/// nobody reads, and the choice of which four survive is a decision this module owns rather
/// than leaving to a `Vec::truncate` at a call site.
#[must_use]
pub fn rank(mut reasons: Vec<CullReason>, limit: usize) -> Vec<CullReason> {
    reasons.sort_by(|left, right| {
        right
            .weight
            .abs()
            .total_cmp(&left.weight.abs())
            .then_with(|| position_of(left.code).cmp(&position_of(right.code)))
    });
    reasons.truncate(limit);
    reasons
}

fn position_of(code: CullCode) -> usize {
    CullCode::ALL
        .iter()
        .position(|candidate| *candidate == code)
        .unwrap_or(CullCode::COUNT)
}

/// The sentence a coverage promotion says.
///
/// Section 6.1: "the reason says so honestly ('only frame of the ring exchange')". This is
/// that sentence, and the honesty is in the branch: with alternatives it says the part of
/// the day, and without them it says there was only one.
#[must_use]
pub fn coverage_text(rule: MustHave, alternatives: u32) -> String {
    if alternatives <= 1 {
        return format!(
            "the only photograph of {} - it is in the gallery because that part of the \
             wedding would otherwise be missing",
            spoken(rule)
        );
    }
    format!(
        "one of the photographs of {} that make sure that part of the wedding is in the \
         gallery",
        spoken(rule)
    )
}

/// The sentence an identity promotion says.
///
/// Deliberately never names the person. The identity is in the catalog and the panel can
/// show a face; a stored *sentence* naming somebody would be a stored sentence that outlives
/// a merge, a split or a photographer correcting who they are.
#[must_use]
pub fn identity_text(appearances: u32, required: u32) -> String {
    format!(
        "somebody who should be in the gallery appears in {appearances} of its \
         photographs, and this kind of guest should appear in at least {required}"
    )
}

/// The sentence a near-duplicate rejection says.
#[must_use]
pub fn duplicate_text() -> String {
    "effectively the same photograph as one already in the gallery - AURA kept the \
     stronger of the two"
        .to_string()
}

/// The sentence a lost moment rank says, naming what beat it.
#[must_use]
pub fn lost_text(rank: usize, of: usize) -> String {
    format!(
        "another frame of the same moment is stronger; this one ranked {} of {of}",
        rank.saturating_add(1)
    )
}

/// The part of the day a rule is about, in the middle of a sentence.
fn spoken(rule: MustHave) -> &'static str {
    match rule {
        MustHave::FirstLook => "the first look",
        MustHave::CeremonyEntrance => "walking in to the ceremony",
        MustHave::Vows => "the vows",
        MustHave::Rings => "the ring exchange",
        MustHave::Kiss => "the kiss",
        MustHave::FamilyFormals => "the family groups",
        MustHave::ReceptionEntrance => "entering the reception",
        MustHave::Cake => "the cake",
        MustHave::FirstDance => "the first dance",
        MustHave::VenueEstablishing => "the venue",
        MustHave::Exit => "the exit",
        MustHave::CloseFamily => "close family",
    }
}
