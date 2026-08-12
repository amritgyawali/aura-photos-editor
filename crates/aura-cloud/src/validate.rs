//! Turning a model's text into a typed value, or a precise complaint.
//!
//! Four things happen between "the provider replied" and "the caller has a
//! struct", and the order matters:
//!
//! 1. **Extract.** Find the JSON inside whatever else came back. Every vendor's
//!    JSON mode occasionally emits a markdown fence or a sentence of preamble,
//!    and burning a repair round trip on a fence is a waste of the user's money
//!    when the JSON underneath is perfectly good.
//! 2. **Prune.** Drop fields the schema does not define, where it says to.
//!    Section 6.2.
//! 3. **Coerce.** Map unrecognised enum members onto `unknown`, counting them so
//!    the gateway can apply the confidence penalty. Section 6.2 again.
//! 4. **Validate, then deserialise, then validate again.** The schema proves the
//!    shape, `serde` produces the value, and [`Validate`] proves the meaning.
//!
//! Steps 2 and 3 before step 4 is the deliberate part. A validator that ran first
//! would reject a response that pruning was about to make valid, and the user
//! would pay for a repair that changed nothing.

use aura_core::errors::cloud::schema_invalid;
use aura_core::AuraResult;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::contract::cloud::Validate;
use crate::schema::Schema;

/// A response that survived the whole chain, and what it took to get there.
#[derive(Debug, Clone, PartialEq)]
pub struct Validated<T> {
    /// The typed answer.
    pub value: T,
    /// Fields the schema did not define, dropped.
    pub pruned: usize,
    /// Enum members the schema did not know, mapped to `unknown`.
    pub coerced: usize,
}

/// Run the whole chain over one response body.
///
/// # Errors
///
/// `AURA-CLOUD-6005`, with a `detail` that is written to be shown to the model
/// on the repair attempt. Every failing rule is named, not just the first,
/// because one repair round trip is all there is.
pub fn parse<T: DeserializeOwned + Validate>(
    task: &str,
    schema: &Schema,
    text: &str,
) -> AuraResult<Validated<T>> {
    let extracted = extract_json(text).ok_or_else(|| {
        schema_invalid(
            task,
            format!(
                "the response contained no JSON object; it began {}",
                crate::provider::truncate(text.trim(), 120)
            ),
        )
    })?;

    let mut document: Value = serde_json::from_str(extracted).map_err(|err| {
        schema_invalid(
            task,
            format!(
                "the response is not valid JSON ({err}); it began {}",
                crate::provider::truncate(extracted, 120)
            ),
        )
    })?;

    let pruned = schema.prune(&mut document);
    let coerced = schema.coerce_unknown_enums(&mut document);

    if let Some(complaint) = schema.explain(&document) {
        return Err(schema_invalid(task, complaint));
    }

    let value: T = serde_json::from_value(document).map_err(|err| {
        // Reaching here means the schema and the struct disagree, which is a
        // programming error in the task rather than a bad answer from a model.
        // It is still reported as a schema failure so the pipeline degrades
        // rather than stopping, and the detail says whose fault it is.
        schema_invalid(
            task,
            format!("the response matched the schema but not the type ({err}); the task's schema and its Output struct disagree"),
        )
    })?;

    value
        .validate()
        .map_err(|complaint| schema_invalid(task, complaint))?;

    Ok(Validated {
        value,
        pruned,
        coerced,
    })
}

/// Find the JSON object inside a model's reply.
///
/// Handles a bare object, a fenced block with or without a language tag, and
/// prose either side. Returns `None` when there is no balanced object at all.
#[must_use]
pub fn extract_json(text: &str) -> Option<&str> {
    let trimmed = text.trim();

    // A fenced block, with or without ```json. Take what is inside the fence and
    // recurse, because the fence may still wrap prose around the object.
    if let Some(rest) = trimmed.strip_prefix("```") {
        let body = rest.split_once('\n').map_or(rest, |(_, body)| body);
        let inner = body.rsplit_once("```").map_or(body, |(head, _)| head);
        return balanced_object(inner);
    }

    balanced_object(trimmed)
}

/// The first balanced `{...}` in the text, respecting strings and escapes.
fn balanced_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|byte| *byte == b'{')?;

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, byte) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(start..=offset);
                }
            }
            _ => {}
        }
    }
    None
}
