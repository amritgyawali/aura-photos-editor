//! Building a decision that can be stored, and the two things that make it reproducible.
//!
//! A deciding phase produces a verdict, some reasons and a confidence. Between that and a
//! row in the ledger there are exactly two pieces of arithmetic, and both of them exist so
//! that `aura replay` can ask the same question again in a year:
//!
//! * **The canonical output encoding.** `outputs_json` is compared byte for byte by the
//!   replay, so two runs that decided the same thing must produce the same string. That
//!   means sorted keys, no whitespace and quantised floats - see [`canonical_json`].
//! * **The inputs hash.** Section 6.3: it "covers analysis outputs, config versions and
//!   model versions, so replay can assert determinism and detect drift after an upgrade".
//!   [`inputs_hash`] is the one place that is computed, so a phase cannot accidentally hash
//!   a different set of things than the replay compares.
//!
//! ## Why floats are quantised to a thousandth
//!
//! The same reason `aura_cull::explain::deterministic_hash` quantises: a sub-score that
//! arrives as `0.640_000_1` on one machine and `0.639_999_9` on another describes the same
//! photograph. A hash that called those different questions would report a determinism
//! failure on every cross-platform replay, and a support tool that cries wolf is a support
//! tool nobody runs.
//!
//! Three decimal places, because that is the precision at which every confidence, score and
//! weight in this product is rendered and argued about. A difference the panel cannot show
//! is a difference the ledger does not record.

use std::collections::BTreeMap;

use aura_core::contract::ids::DecisionId;
use aura_core::contract::ledger::{
    Autonomy, DecisionKind, DecisionSource, DecisionSubject, LedgerDecision, LedgerReason,
};
use aura_core::contract::scene::Timestamp;
use aura_core::ProjectId;

use crate::LEDGER_VER;

/// How many decimal places every float in a hash or a canonical output carries.
pub const QUANTISATION: f32 = 1_000.0;

/// One float, as the hash and the canonical encoding see it.
///
/// Clamped to `0..1` first, because every float this crate hashes is a score, a confidence
/// or a weight, and a weight outside that range is a producer bug that must not become a
/// distinct question.
#[must_use]
pub fn quantise(value: f32) -> i32 {
    (value.clamp(-1.0, 1.0) * QUANTISATION).round() as i32
}

/// Assemble a decision that the ledger will accept.
///
/// The builder exists to make the two things a caller cannot get wrong impossible to get
/// wrong: the reasons are ranked and bounded here rather than at each of six call sites,
/// and [`DecisionBuilder::build`] computes the inputs hash from what was actually declared
/// rather than from what the caller remembered to include.
///
/// It does **not** set the autonomy band or the calibrated confidence. Those are computed by
/// [`crate::api::Explain::record`] from the policy and the calibration set, because a
/// builder that could set its own band would be a caller that could grant itself autonomy -
/// which is the one thing section 6.4 exists to prevent.
#[derive(Debug, Clone)]
pub struct DecisionBuilder {
    project: ProjectId,
    kind: DecisionKind,
    subject: DecisionSubject,
    outputs: BTreeMap<String, String>,
    inputs: Vec<(String, String)>,
    reasons: Vec<LedgerReason>,
    raw_confidence: f32,
    source: DecisionSource,
    model_versions: BTreeMap<String, u16>,
    config_versions: BTreeMap<String, u16>,
    ms: u32,
    supersedes: Option<DecisionId>,
}

impl DecisionBuilder {
    /// A decision of one kind, about one subject, in one project.
    #[must_use]
    pub fn new(project: ProjectId, kind: DecisionKind, subject: DecisionSubject) -> Self {
        Self {
            project,
            kind,
            subject,
            outputs: BTreeMap::new(),
            inputs: Vec::new(),
            reasons: Vec::new(),
            raw_confidence: 0.0,
            source: DecisionSource::Local,
            model_versions: BTreeMap::new(),
            config_versions: BTreeMap::new(),
            ms: 0,
            supersedes: None,
        }
    }

    /// One field of what was decided. A string value, quoted in the JSON.
    #[must_use]
    pub fn output(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.outputs.insert(key.into(), json_string(&value.into()));
        self
    }

    /// One field of what was decided, as a number quantised to a thousandth.
    #[must_use]
    pub fn output_num(mut self, key: impl Into<String>, value: f32) -> Self {
        self.outputs.insert(
            key.into(),
            format!(
                "{:.3}",
                f64::from(quantise(value)) / f64::from(QUANTISATION)
            ),
        );
        self
    }

    /// One field of what was decided, as a boolean.
    #[must_use]
    pub fn output_bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.outputs.insert(key.into(), value.to_string());
        self
    }

    /// One thing that fed the decision, as a name and its stable text form.
    ///
    /// Everything declared here is in the inputs hash, in declaration order. Order matters
    /// and is not sorted on purpose: an input list is a description of *how* a decision was
    /// reached, and two phases that fed the same values in a different order asked different
    /// questions.
    #[must_use]
    pub fn input(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.inputs.push((name.into(), value.into()));
        self
    }

    /// One numeric input, quantised.
    #[must_use]
    pub fn input_num(mut self, name: impl Into<String>, value: f32) -> Self {
        self.inputs.push((name.into(), quantise(value).to_string()));
        self
    }

    /// One reason. Ranked and bounded at [`LedgerDecision::MAX_REASONS`] on build.
    #[must_use]
    pub fn reason(mut self, reason: LedgerReason) -> Self {
        self.reasons.push(reason);
        self
    }

    /// Several reasons at once.
    #[must_use]
    pub fn reasons(mut self, reasons: impl IntoIterator<Item = LedgerReason>) -> Self {
        self.reasons.extend(reasons);
        self
    }

    /// What the deciding code believed, `0..1`.
    #[must_use]
    pub fn confidence(mut self, raw: f32) -> Self {
        self.raw_confidence = raw.clamp(0.0, 1.0);
        self
    }

    /// Where the decision came from. Local unless said otherwise.
    #[must_use]
    pub fn source(mut self, source: DecisionSource) -> Self {
        self.source = source;
        self
    }

    /// One model underneath the decision.
    #[must_use]
    pub fn model(mut self, name: impl Into<String>, version: u16) -> Self {
        self.model_versions.insert(name.into(), version);
        self
    }

    /// One configuration table underneath the decision.
    #[must_use]
    pub fn config(mut self, name: impl Into<String>, version: u16) -> Self {
        self.config_versions.insert(name.into(), version);
        self
    }

    /// How long the deciding code took.
    #[must_use]
    pub const fn ms(mut self, ms: u32) -> Self {
        self.ms = ms;
        self
    }

    /// The decision this one replaces.
    #[must_use]
    pub const fn supersedes(mut self, earlier: DecisionId) -> Self {
        self.supersedes = Some(earlier);
        self
    }

    /// Finish, at a given time.
    ///
    /// The clock is an argument rather than a call, for the rule the whole workspace
    /// follows: an ambient wall-clock read is banned outside `main.rs`, and a decision whose id or
    /// timestamp came from an ambient clock would be a decision no test could pin.
    ///
    /// `calibrated_confidence` is set equal to the raw value and `autonomy` to
    /// [`Autonomy::RequireReview`]. Both are recomputed by the ledger before the row is
    /// written; the defaults here are the strict ones so that a decision recorded by
    /// something that bypassed the ledger would ask for review rather than act.
    #[must_use]
    pub fn build(self, id: DecisionId, created_at: Timestamp) -> LedgerDecision {
        let inputs_hash = inputs_hash(&self.inputs, &self.model_versions, &self.config_versions);
        let reasons = rank(self.reasons, LedgerDecision::MAX_REASONS);
        LedgerDecision {
            id,
            project: self.project,
            kind: self.kind,
            subject: self.subject,
            inputs_hash,
            outputs_json: canonical_json(&self.outputs),
            reasons,
            raw_confidence: self.raw_confidence,
            calibrated_confidence: self.raw_confidence,
            autonomy: Autonomy::RequireReview,
            source: self.source,
            model_versions: self.model_versions.into_iter().collect(),
            config_versions: self.config_versions.into_iter().collect(),
            calibration_ver: LedgerDecision::UNCALIBRATED,
            ms: self.ms,
            created_at,
            supersedes: self.supersedes,
        }
    }
}

/// The hash of one question.
///
/// Covers the declared inputs in declaration order, then every model and every config table
/// in name order. `LEDGER_VER` is mixed in first so that a change to this encoding produces
/// a different hash rather than a silent collision with the old one.
///
/// Returns the low eight bytes of a BLAKE3 digest, exactly as
/// `aura_cull::explain::deterministic_hash` does, so the two are comparable in a support
/// case and identical in cost.
#[must_use]
pub fn inputs_hash(
    inputs: &[(String, String)],
    models: &BTreeMap<String, u16>,
    configs: &BTreeMap<String, u16>,
) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"aura-explain-v1");
    hasher.update(&LEDGER_VER.to_le_bytes());
    for (name, value) in inputs {
        hasher.update(name.as_bytes());
        hasher.update(b"=");
        hasher.update(value.as_bytes());
        hasher.update(b";");
    }
    hasher.update(b"|models|");
    for (name, version) in models {
        hasher.update(name.as_bytes());
        hasher.update(&version.to_le_bytes());
    }
    hasher.update(b"|config|");
    for (name, version) in configs {
        hasher.update(name.as_bytes());
        hasher.update(&version.to_le_bytes());
    }
    let digest = hasher.finalize();
    let mut first = [0u8; 8];
    first.copy_from_slice(digest.as_bytes().get(..8).unwrap_or(&[0; 8]));
    u64::from_le_bytes(first)
}

/// A canonical JSON object from already-encoded values.
///
/// Sorted keys, no whitespace, no trailing comma. The values are written by the builder,
/// which is what keeps numbers quantised and strings escaped; this function only assembles.
///
/// Hand-written rather than `serde_json::to_string`, and it is worth saying why: `serde_json`
/// would serialise an `f32` at full precision, which is exactly the thing the replay must not
/// compare. Assembling from pre-encoded values makes the quantisation impossible to bypass.
#[must_use]
pub fn canonical_json(fields: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(64 + fields.len() * 24);
    out.push('{');
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&json_string(key));
        out.push(':');
        out.push_str(value);
    }
    out.push('}');
    out
}

/// One JSON string literal, with the six escapes the format requires.
#[must_use]
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // A control character, as the four-hex-digit escape JSON requires. The two
            // nibbles are written by hand rather than through `format!`, so that encoding a
            // string stays one pass with no intermediate allocation - this runs once per
            // field of every decision in the ledger.
            c if (c as u32) < 0x20 => {
                out.push_str("\\u00");
                let byte = c as u32;
                out.push(*HEX.get(((byte >> 4) & 0xf) as usize).unwrap_or(&'0'));
                out.push(*HEX.get((byte & 0xf) as usize).unwrap_or(&'0'));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The sixteen hexadecimal digits, for the control-character escape above.
const HEX: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

/// Put reasons in the order a photographer reads them and cut the list to `limit`.
///
/// Strongest first by absolute weight, ties broken on the code. The same rule
/// `aura_cull::explain::rank` uses and for the same reason: a panel that listed nine reasons
/// is a panel nobody reads, and which ones survive is a decision that must not depend on
/// the order a producer happened to push them in.
#[must_use]
pub fn rank(mut reasons: Vec<LedgerReason>, limit: usize) -> Vec<LedgerReason> {
    reasons.sort_by(|left, right| {
        right
            .weight
            .abs()
            .total_cmp(&left.weight.abs())
            .then_with(|| left.code.cmp(&right.code))
    });
    reasons.truncate(limit);
    reasons
}
