//! Turning a list of reasons into a paragraph, without adding anything to it.
//!
//! Section 6.2 is the whole specification of this module in one sentence: "the language
//! model may only *rephrase* them into prose, never add facts." Two things enforce it.
//!
//! **The offline path cannot invent, because it does not write.** [`Summariser::template`]
//! assembles a paragraph from the reasons' own sentences with connective words between
//! them. Every clause in the output is a clause a deciding phase produced. This is the path
//! that always exists - invariant 6 - and it is the default.
//!
//! **The cloud path is checked afterwards.** [`Grounding::check`] takes the structured
//! input and the generated paragraph and refuses any number in the paragraph that is not in
//! the input. That is a blunt instrument and it is deliberately blunt: a model that adds
//! "and the shutter speed was 1/60" to an explanation has invented a measurement, and there
//! is no way to tell a plausible invention from a true one except by having said it first.
//!
//! ## Why numbers and not claims
//!
//! Because a claim is not checkable and a number is. A summariser that says "the frame is
//! slightly soft" when the reason said "the subject is softer than this camera should
//! manage" has rephrased. One that says "0.42 EV" when nothing in the input mentioned 0.42
//! has fabricated, and the automated check in section 10.1 - "NL summaries contain no
//! numbers or claims absent from the structured input" - is enforceable on exactly the
//! first half of that sentence.
//!
//! The second half is enforced differently and structurally: the cloud task is given the
//! reason list and nothing else, no images, and its output is one paragraph and an optional
//! headline. There is no field for a new reason, so a fabricated *claim* has nowhere to go
//! that the panel would render as evidence.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use aura_core::contract::ledger::LedgerDecision;

/// The longest summary the panel shows. Section 7's schema caps the cloud answer here too.
pub const MAX_SUMMARY_CHARS: usize = 600;

/// The longest headline.
pub const MAX_HEADLINE_CHARS: usize = 90;

/// A paragraph and its optional headline.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Summary {
    /// Two to four sentences, in the photographer's vocabulary.
    pub summary: String,
    /// A short line above it, when there is one worth having.
    pub headline: Option<String>,
    /// True when a language model wrote it rather than the template.
    pub from_cloud: bool,
}

impl Summary {
    /// True when the text is inside the caps the schema and the panel share.
    #[must_use]
    pub fn is_bounded(&self) -> bool {
        self.summary.chars().count() <= MAX_SUMMARY_CHARS
            && self
                .headline
                .as_ref()
                .is_none_or(|line| line.chars().count() <= MAX_HEADLINE_CHARS)
    }
}

/// Assembles a decision's reasons into prose.
#[derive(Debug, Clone, Copy, Default)]
pub struct Summariser;

impl Summariser {
    /// The deterministic paragraph. Always available, offline, in every build.
    ///
    /// Reads the strongest three reasons, in the decision's own ranking, and joins them
    /// with connectives chosen by how many there are. The connectives are the only words in
    /// the output this module contributes, and they carry no facts: "and", "but also",
    /// "on top of that".
    ///
    /// The lead sentence names what was decided, because a paragraph of reasons with no
    /// verdict at the top is a paragraph a photographer has to read twice.
    #[must_use]
    pub fn template(decision: &LedgerDecision) -> Summary {
        let reasons = decision.top_reasons(3);
        let verdict = verdict_line(decision);
        let mut summary = String::with_capacity(320);
        summary.push_str(&verdict);

        for (index, reason) in reasons.iter().enumerate() {
            let sentence = reason.text.trim();
            if sentence.is_empty() {
                continue;
            }
            match index {
                0 => summary.push_str(" The main reason is that "),
                1 => summary.push_str(" There is also this: "),
                _ => summary.push_str(" And on top of that, "),
            }
            summary.push_str(sentence);
            summary.push('.');
        }

        if !decision.is_calibrated() {
            summary.push_str(
                " AURA has not yet learned how often it is right about this kind of decision, \
                 so read the confidence as a rough guide.",
            );
        }

        let mut text = summary.trim().to_string();
        if text.chars().count() > MAX_SUMMARY_CHARS {
            text = text.chars().take(MAX_SUMMARY_CHARS).collect();
        }

        Summary {
            summary: text,
            headline: Some(headline(decision)),
            from_cloud: false,
        }
    }

    /// The structured facts a cloud summariser is given.
    ///
    /// Codes, weights and numbers - never an image, never a filename, never an identity.
    /// This is also exactly what [`Grounding::check`] is measured against, so the prompt and
    /// the check cannot describe different inputs.
    #[must_use]
    pub fn facts(decision: &LedgerDecision) -> Vec<String> {
        let mut facts = Vec::with_capacity(decision.reasons.len() + 2);
        facts.push(format!(
            "decision: {} ; confidence {:.2} ; autonomy {}",
            decision.kind.as_str(),
            decision.calibrated_confidence,
            decision.autonomy.as_str()
        ));
        for reason in &decision.reasons {
            let mut line = format!(
                "reason {} (weight {:.2}): {}",
                reason.code, reason.weight, reason.text
            );
            for (name, value) in reason.evidence.params() {
                // Writing into a String cannot fail; the result is discarded deliberately.
                let _ = write!(line, " ; {name} {value:+.2}");
            }
            facts.push(line);
        }
        facts
    }
}

/// The sentence that says what was decided.
fn verdict_line(decision: &LedgerDecision) -> String {
    let kept = decision.outputs_json.contains("\"kept\":true");
    match (decision.kind, kept) {
        (aura_core::DecisionKind::Cull, true) => "This photograph is in the gallery.".to_string(),
        (aura_core::DecisionKind::Cull, false) => {
            "This photograph is not in the gallery.".to_string()
        }
        (kind, _) => format!("AURA made a {} decision about this.", kind.title()),
    }
}

/// The short line above the paragraph.
fn headline(decision: &LedgerDecision) -> String {
    let line = decision
        .top_reasons(1)
        .first()
        .map_or_else(String::new, |reason| reason.text.clone());
    let mut headline = if line.is_empty() {
        decision.kind.title().to_string()
    } else {
        let mut chars: Vec<char> = line.chars().collect();
        if let Some(first) = chars.first_mut() {
            *first = first.to_ascii_uppercase();
        }
        chars.into_iter().collect()
    };
    if headline.chars().count() > MAX_HEADLINE_CHARS {
        headline = headline
            .chars()
            .take(MAX_HEADLINE_CHARS.saturating_sub(1))
            .collect::<String>();
        headline.push('…');
    }
    headline
}

/// Whether a paragraph says only what it was given.
///
/// Section 10.1's automated grounding check.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Grounding {
    /// Numbers in the paragraph that were not in the input.
    pub ungrounded: Vec<String>,
}

impl Grounding {
    /// Check one paragraph against the facts it was given.
    ///
    /// Numeric tokens are compared as text after normalisation - a leading sign and a
    /// trailing unit are stripped, and trailing zeroes after a decimal point are removed -
    /// so "0.40", "+0.4" and "0.4 EV" all match a fact containing 0.4. That is deliberately
    /// generous: the purpose is to catch a model that invented a measurement, not one that
    /// rounded a supplied one, and a check that failed on rounding would be a check somebody
    /// turned off.
    #[must_use]
    pub fn check(summary: &str, facts: &[String]) -> Self {
        let supplied: BTreeSet<String> = facts.iter().flat_map(|fact| numbers_in(fact)).collect();
        let ungrounded = numbers_in(summary)
            .into_iter()
            .filter(|number| !supplied.contains(number))
            .collect();
        Self { ungrounded }
    }

    /// True when nothing was invented.
    #[must_use]
    pub fn is_grounded(&self) -> bool {
        self.ungrounded.is_empty()
    }
}

/// Every number in a sentence, normalised.
///
/// Public because the eval harness asserts against it directly, and a check whose helper is
/// private is a check whose helper has no test of its own.
#[must_use]
pub fn numbers_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || (ch == '.' && !current.is_empty()) {
            current.push(ch);
        } else if !current.is_empty() {
            out.push(normalise(&current));
            current.clear();
        }
    }
    if !current.is_empty() {
        out.push(normalise(&current));
    }
    out.retain(|number| !number.is_empty());
    out
}

/// One number, with trailing zeroes and a trailing point removed.
fn normalise(raw: &str) -> String {
    let trimmed = raw.trim_matches('.');
    if !trimmed.contains('.') {
        return trimmed.to_string();
    }
    let cleaned = trimmed.trim_end_matches('0').trim_end_matches('.');
    if cleaned.is_empty() {
        "0".to_string()
    } else {
        cleaned.to_string()
    }
}
