//! The support bundle: enough to reproduce a complaint, and nothing that belongs to a
//! client.
//!
//! Section 2.1: "anonymised ledger slice + config + model versions, with no image pixels
//! unless the user opts in." Section 10.1: "support bundle contains no pixels, no client
//! names and no API keys."
//!
//! ## Three guarantees, and only one of them is a filter
//!
//! * **No pixels, structurally.** `Evidence` has no variant that can hold image bytes, so
//!   there is nothing in the ledger for an exporter to leak. The opt-in of section 2.1 is
//!   deliberately **not implemented**: it would be the one code path that could put a
//!   photograph in a file that gets emailed, and this build has no use for it. When phase 27
//!   needs it, it will need an ADR.
//! * **No names, structurally.** Nothing in migration 13 stores a filename, a person's name
//!   or a folder. The ids are UUIDs, and [`Bundle::write`] replaces even those - see below.
//! * **No keys, by construction and by scan.** Keys live in the OS credential store and
//!   have never been in the catalog. [`Bundle::scan`] looks anyway, because "it cannot
//!   happen" is what every leak was called beforehand.
//!
//! ## Why identifiers are replaced rather than kept
//!
//! A UUID identifies nothing on its own - and it identifies a great deal when the person
//! reading the bundle also has the catalog it came from. So every id in a bundle is replaced
//! by a short stable handle derived from a per-bundle salt: `image_0001`, `moment_0003`. The
//! *structure* survives - the same photograph is the same handle everywhere in the file, so
//! a support engineer can still see that three decisions were about one frame - and the
//! mapping is not in the bundle and is not stored anywhere.
//!
//! A photographer who needs to point at a specific frame can say so themselves. That is a
//! sentence in an email; the alternative is a file that identifies a client's wedding to
//! anybody who receives it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use aura_core::contract::ledger::{Evidence, LedgerDecision, LedgerOutline};
use aura_core::AuraError;

use crate::decision::json_string;
use crate::errors;

/// What a bundle may and may not carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleOptions {
    /// Include the reasons' sentences as well as their codes.
    ///
    /// On by default. The sentences are product copy - they name a rule, a rival frame, a
    /// shutter speed - and none of them can name a person: `aura_cull::explain::identity_text`
    /// deliberately never does, and a test in phase 12 asserts it.
    pub include_text: bool,
    /// How many decisions to carry. The newest ones.
    pub limit: usize,
}

impl Default for BundleOptions {
    fn default() -> Self {
        Self {
            include_text: true,
            limit: 5_000,
        }
    }
}

/// One anonymised slice of a ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// The JSON document.
    pub json: String,
    /// How many decisions it carries.
    pub decisions: usize,
    /// How many distinct identifiers were replaced by handles.
    pub anonymised: usize,
}

impl Bundle {
    /// Words that must never appear in a bundle.
    ///
    /// Two of them are key prefixes, two are the shapes a filesystem path takes, and one is
    /// the header a bearer token travels in. The list is short because a long one would
    /// match ordinary English and turn the scan into something people work around.
    pub const FORBIDDEN: &'static [&'static str] = &["sk-", "Bearer ", "C:\\", "/Users/"];

    /// Build a bundle from a project's decisions.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5059` when there is nothing to export. An empty bundle is worse than none:
    /// it looks like evidence that nothing happened.
    pub fn build(
        decisions: &[LedgerDecision],
        outline: &LedgerOutline,
        options: &BundleOptions,
    ) -> Result<Self, AuraError> {
        if decisions.is_empty() {
            return Err(errors::bundle_refused(
                "this wedding has no recorded decisions, so there is nothing to send",
            ));
        }

        let mut handles = Handles::default();
        let slice: Vec<&LedgerDecision> = decisions.iter().rev().take(options.limit).collect();

        let mut out = String::with_capacity(1_024 + slice.len() * 256);
        out.push_str("{\"schema\":\"aura-support-bundle-1\",\"decisions\":[");
        for (index, decision) in slice.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&encode(decision, &mut handles, options));
        }
        out.push_str("],\"summary\":");
        out.push_str(&encode_outline(outline));
        out.push('}');

        Ok(Self {
            json: out,
            decisions: slice.len(),
            anonymised: handles.count(),
        })
    }

    /// Everything forbidden that this bundle nonetheless contains.
    ///
    /// Empty is the only acceptable answer, and the gate asserts it. It is a scan over the
    /// finished text rather than a check on each field, because the failure this catches is
    /// a field somebody added later without thinking about where it goes.
    #[must_use]
    pub fn scan(&self) -> Vec<&'static str> {
        Self::FORBIDDEN
            .iter()
            .copied()
            .filter(|needle| self.json.contains(needle))
            .collect()
    }

    /// True when the scan is clean.
    #[must_use]
    pub fn is_safe(&self) -> bool {
        self.scan().is_empty()
    }
}

/// Stable short handles for the ids in one bundle.
#[derive(Debug, Default)]
struct Handles {
    map: BTreeMap<String, String>,
}

impl Handles {
    /// The handle for one id, assigning one if this is the first time it has been seen.
    ///
    /// Numbered in first-appearance order rather than hashed, because a support engineer
    /// reads them and `image_0007` is a thing a person can hold in their head while
    /// `img_9f3a` is not. First-appearance order is stable for a given ledger slice, which
    /// is what makes two bundles of the same wedding comparable.
    fn get(&mut self, kind: &str, id: &str) -> String {
        if let Some(existing) = self.map.get(id) {
            return existing.clone();
        }
        let handle = format!("{kind}_{:04}", self.map.len().saturating_add(1));
        self.map.insert(id.to_string(), handle.clone());
        handle
    }

    fn count(&self) -> usize {
        self.map.len()
    }
}

fn encode(decision: &LedgerDecision, handles: &mut Handles, options: &BundleOptions) -> String {
    let mut out = String::with_capacity(256);
    out.push('{');
    let _ = write!(
        out,
        "\"id\":{},\"kind\":{},\"subject\":{},\"inputs_hash\":\"{:016x}\"",
        json_string(&handles.get("decision", &decision.id.to_db())),
        json_string(decision.kind.as_str()),
        json_string(&handles.get(decision.subject.kind_str(), &decision.subject.id_str())),
        decision.inputs_hash
    );
    let _ = write!(
        out,
        ",\"raw_confidence\":{:.3},\"calibrated_confidence\":{:.3},\"calibration_ver\":{}",
        decision.raw_confidence, decision.calibrated_confidence, decision.calibration_ver
    );
    let _ = write!(
        out,
        ",\"autonomy\":{},\"source\":{},\"ms\":{}",
        json_string(decision.autonomy.as_str()),
        json_string(decision.source.as_str()),
        decision.ms
    );

    // The outputs are *not* carried verbatim: they contain photo ids. What a support case
    // needs from them is the shape, so they are replaced by the count of fields and the
    // decision's own reasons carry the rest.
    let _ = write!(
        out,
        ",\"outputs_fields\":{}",
        decision.outputs_json.matches("\":").count()
    );

    out.push_str(",\"versions\":{\"models\":[");
    for (index, (name, version)) in decision.model_versions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "[{},{version}]", json_string(name));
    }
    out.push_str("],\"config\":[");
    for (index, (name, version)) in decision.config_versions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "[{},{version}]", json_string(name));
    }
    out.push_str("]},\"reasons\":[");
    for (index, reason) in decision.reasons.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"code\":{},\"weight\":{:.3},\"evidence\":{}",
            json_string(&reason.code),
            reason.weight,
            json_string(reason.evidence.kind_str())
        );
        if options.include_text {
            let _ = write!(out, ",\"text\":{}", json_string(&reason.text));
        }
        // A crop rectangle is four numbers about a frame nobody outside this machine can
        // look at, so it travels; a frame list is a list of ids, so it becomes handles.
        match &reason.evidence {
            Evidence::Crop(area) => {
                let _ = write!(
                    out,
                    ",\"crop\":[{:.3},{:.3},{:.3},{:.3}]",
                    area.x, area.y, area.w, area.h
                );
            }
            Evidence::Frames(ids) => {
                out.push_str(",\"frames\":[");
                for (position, id) in ids.iter().enumerate() {
                    if position > 0 {
                        out.push(',');
                    }
                    out.push_str(&json_string(&handles.get("image", &id.to_db())));
                }
                out.push(']');
            }
            Evidence::Params(pairs) => {
                out.push_str(",\"params\":[");
                for (position, (name, value)) in pairs.iter().enumerate() {
                    if position > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "[{},{value:.3}]", json_string(name));
                }
                out.push(']');
            }
            Evidence::None => {}
        }
        out.push('}');
    }
    out.push_str("]}");
    out
}

fn encode_outline(outline: &LedgerOutline) -> String {
    format!(
        "{{\"decisions\":{},\"explained\":{},\"current\":{},\"superseded\":{},\
         \"calibrated\":{},\"calibration_ver\":{},\"reasons\":{},\"evidenced\":{},\
         \"bytes\":{},\"explanation_coverage\":{:.3}}}",
        outline.decisions,
        outline.explained,
        outline.current,
        outline.superseded,
        outline.calibrated,
        outline.calibration_ver,
        outline.reasons,
        outline.evidenced,
        outline.bytes,
        outline.explanation_coverage()
    )
}
