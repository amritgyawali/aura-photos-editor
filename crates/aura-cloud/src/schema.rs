//! A JSON Schema validator, deliberately small.
//!
//! A general draft 2020-12 implementation is a large dependency with a large
//! attack surface, and this crate needs perhaps a tenth of it. What it needs
//! instead is three properties a general validator does not give:
//!
//! **The schema itself is checked at parse time.** [`Schema::parse`] refuses a
//! schema containing a keyword it does not implement, rather than ignoring it. A
//! validator that silently skips `$ref` would report a malformed response as
//! valid, and the response would then be deserialised into a struct that does not
//! match it. Failing loudly at parse time turns that into a compile-and-test
//! failure for the person who wrote the schema.
//!
//! **The error message is written for the model.** It is sent verbatim in the one
//! repair retry, so it names the JSON Pointer, the rule and what was found -
//! `/confidence: expected number in [0, 1], found 1.4` - because "validation
//! failed" is not something a model can act on.
//!
//! **Errors are ordered and complete.** Every failure is reported, sorted by
//! pointer, so a response with three problems is repaired in one attempt rather
//! than three. Determinism invariant 4 covers the ordering: the same bad response
//! must produce byte-identical repair text on every machine.
//!
//! The supported subset is exactly what the tasks in this product use:
//! `type` (including union arrays), `required`, `properties`,
//! `additionalProperties`, `items`, `enum`, `const`, `minimum`, `maximum`,
//! `minLength`, `maxLength`, `minItems` and `maxItems`. Annotations that do not
//! constrain anything - `title`, `description`, `$schema`, `$id`, `examples`,
//! `default` - are accepted and ignored.

use std::collections::BTreeMap;
use std::fmt;

use aura_core::errors::cloud::schema_invalid;
use aura_core::AuraError;
use serde_json::{Map, Value};

/// The value substituted for an enum member the schema does not know.
pub const UNKNOWN: &str = "unknown";

/// Keywords that describe rather than constrain. Accepted, never enforced.
const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "$comment",
    "title",
    "description",
    "examples",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
];

/// One reason a document failed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchemaError {
    /// RFC 6901 JSON Pointer to the offending value; the empty string is the root.
    pub pointer: String,
    /// A sentence naming the rule and what was found.
    pub message: String,
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pointer.is_empty() {
            write!(f, "(root): {}", self.message)
        } else {
            write!(f, "{}: {}", self.pointer, self.message)
        }
    }
}

/// The scalar kinds a `type` keyword may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Object,
    Array,
    String,
    Number,
    Integer,
    Boolean,
    Null,
}

impl Kind {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "object" => Some(Self::Object),
            "array" => Some(Self::Array),
            "string" => Some(Self::String),
            "number" => Some(Self::Number),
            "integer" => Some(Self::Integer),
            "boolean" => Some(Self::Boolean),
            "null" => Some(Self::Null),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Array => "array",
            Self::String => "string",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Null => "null",
        }
    }

    fn matches(self, value: &Value) -> bool {
        match (self, value) {
            (Self::Object, Value::Object(_))
            | (Self::Array, Value::Array(_))
            | (Self::String, Value::String(_))
            | (Self::Boolean, Value::Bool(_))
            | (Self::Null, Value::Null)
            | (Self::Number, Value::Number(_)) => true,
            (Self::Integer, Value::Number(number)) => {
                number.is_i64()
                    || number.is_u64()
                    || number.as_f64().is_some_and(f64::fract_is_zero)
            }
            _ => false,
        }
    }
}

/// `f64::fract() == 0.0` without tripping the float-comparison lint, which is
/// banned for good reasons that do not apply to "is this a whole number".
trait FractIsZero {
    fn fract_is_zero(self) -> bool;
}

impl FractIsZero for f64 {
    fn fract_is_zero(self) -> bool {
        self.is_finite() && (self - self.trunc()).abs() < f64::EPSILON
    }
}

/// What `additionalProperties` says about names the schema does not list.
#[derive(Debug, Clone)]
enum Additional {
    /// `true`, or absent. Anything goes.
    Allowed,
    /// `false`. Unlisted names are an error, and [`Schema::prune`] removes them.
    Forbidden,
    /// A schema every unlisted value must satisfy.
    Constrained(Box<Schema>),
}

/// A parsed schema in the supported subset.
#[derive(Debug, Clone)]
pub struct Schema {
    types: Vec<Kind>,
    required: Vec<String>,
    properties: BTreeMap<String, Schema>,
    additional: Additional,
    items: Option<Box<Schema>>,
    enumeration: Option<Vec<Value>>,
    constant: Option<Value>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    min_length: Option<usize>,
    max_length: Option<usize>,
    min_items: Option<usize>,
    max_items: Option<usize>,
}

impl Schema {
    /// Parse a JSON Schema, refusing anything outside the supported subset.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6005` when the text is not JSON, is not an object, or uses a
    /// keyword this validator does not implement. That last case is a programming
    /// error in a task, caught by that task's own tests, never by a photographer.
    pub fn parse(text: &str) -> Result<Self, AuraError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|err| schema_invalid("schema", format!("schema is not JSON: {err}")))?;
        Self::from_value(&value, "")
    }

    fn from_value(value: &Value, pointer: &str) -> Result<Self, AuraError> {
        let object = value.as_object().ok_or_else(|| {
            schema_invalid(
                "schema",
                format!("schema at {} is not an object", display_pointer(pointer)),
            )
        })?;

        let mut schema = Self {
            types: Vec::new(),
            required: Vec::new(),
            properties: BTreeMap::new(),
            additional: Additional::Allowed,
            items: None,
            enumeration: None,
            constant: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            min_items: None,
            max_items: None,
        };

        for (keyword, setting) in object {
            if ANNOTATIONS.contains(&keyword.as_str()) {
                continue;
            }
            match keyword.as_str() {
                "type" => schema.types = parse_types(setting, pointer)?,
                "required" => {
                    schema.required = setting
                        .as_array()
                        .ok_or_else(|| unsupported(pointer, "required must be an array"))?
                        .iter()
                        .filter_map(|name| name.as_str().map(ToString::to_string))
                        .collect();
                }
                "properties" => {
                    let members = setting
                        .as_object()
                        .ok_or_else(|| unsupported(pointer, "properties must be an object"))?;
                    for (name, child) in members {
                        let child_pointer = format!("{pointer}/{name}");
                        schema
                            .properties
                            .insert(name.clone(), Self::from_value(child, &child_pointer)?);
                    }
                }
                "additionalProperties" => {
                    schema.additional = match setting {
                        Value::Bool(true) => Additional::Allowed,
                        Value::Bool(false) => Additional::Forbidden,
                        other => Additional::Constrained(Box::new(Self::from_value(
                            other,
                            &format!("{pointer}/*"),
                        )?)),
                    };
                }
                "items" => {
                    schema.items = Some(Box::new(Self::from_value(
                        setting,
                        &format!("{pointer}/0"),
                    )?));
                }
                "enum" => {
                    schema.enumeration = Some(
                        setting
                            .as_array()
                            .ok_or_else(|| unsupported(pointer, "enum must be an array"))?
                            .clone(),
                    );
                }
                "const" => schema.constant = Some(setting.clone()),
                "minimum" => schema.minimum = setting.as_f64(),
                "maximum" => schema.maximum = setting.as_f64(),
                "minLength" => schema.min_length = as_usize(setting),
                "maxLength" => schema.max_length = as_usize(setting),
                "minItems" => schema.min_items = as_usize(setting),
                "maxItems" => schema.max_items = as_usize(setting),
                other => {
                    return Err(unsupported(
                        pointer,
                        &format!("this validator does not implement `{other}`"),
                    ))
                }
            }
        }
        Ok(schema)
    }

    /// Check a document. An empty result means it is valid.
    ///
    /// Errors come back sorted by pointer so the repair message is identical on
    /// every machine for the same bad response.
    #[must_use]
    pub fn validate(&self, value: &Value) -> Vec<SchemaError> {
        let mut errors = Vec::new();
        self.check(value, "", &mut errors);
        errors.sort();
        errors
    }

    /// A single sentence naming every problem, for the repair retry.
    #[must_use]
    pub fn explain(&self, value: &Value) -> Option<String> {
        let errors = self.validate(value);
        if errors.is_empty() {
            return None;
        }
        Some(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    /// Drop every property the schema does not define, where it says to.
    ///
    /// Section 6.2: "Any field the schema does not define is dropped". A model
    /// that adds a helpful extra field should not cost the user a repair round
    /// trip, so pruning happens before validation. Returns how many fields were
    /// dropped, which the gateway records in the audit row - a task whose
    /// responses are routinely pruned has a prompt that is asking for the wrong
    /// thing.
    pub fn prune(&self, value: &mut Value) -> usize {
        let mut dropped = 0usize;
        match value {
            Value::Object(map) => {
                if matches!(self.additional, Additional::Forbidden) {
                    let unwanted: Vec<String> = map
                        .keys()
                        .filter(|name| !self.properties.contains_key(name.as_str()))
                        .cloned()
                        .collect();
                    dropped += unwanted.len();
                    for name in unwanted {
                        map.remove(&name);
                    }
                }
                for (name, child) in map.iter_mut() {
                    if let Some(schema) = self.properties.get(name) {
                        dropped += schema.prune(child);
                    }
                }
            }
            Value::Array(items) => {
                if let Some(schema) = &self.items {
                    for item in items.iter_mut() {
                        dropped += schema.prune(item);
                    }
                }
            }
            _ => {}
        }
        dropped
    }

    /// Replace enum members the schema does not know with [`UNKNOWN`].
    ///
    /// Only where the schema's own enum offers `unknown` as a member: a schema
    /// that does not is one whose task would rather fail than guess. Returns how
    /// many values were coerced, so the gateway can apply the confidence penalty
    /// from section 6.2 once per coercion.
    pub fn coerce_unknown_enums(&self, value: &mut Value) -> usize {
        let mut coerced = 0usize;
        if let (Some(members), Value::String(text)) = (&self.enumeration, &*value) {
            let known = members.iter().any(|member| member.as_str() == Some(text));
            let offers_unknown = members
                .iter()
                .any(|member| member.as_str() == Some(UNKNOWN));
            if !known && offers_unknown {
                *value = Value::String(UNKNOWN.to_string());
                return 1;
            }
        }
        match value {
            Value::Object(map) => {
                for (name, child) in map.iter_mut() {
                    if let Some(schema) = self.properties.get(name) {
                        coerced += schema.coerce_unknown_enums(child);
                    }
                }
            }
            Value::Array(items) => {
                if let Some(schema) = &self.items {
                    for item in items.iter_mut() {
                        coerced += schema.coerce_unknown_enums(item);
                    }
                }
            }
            _ => {}
        }
        coerced
    }

    fn check(&self, value: &Value, pointer: &str, errors: &mut Vec<SchemaError>) {
        if !self.types.is_empty() && !self.types.iter().any(|kind| kind.matches(value)) {
            errors.push(SchemaError {
                pointer: pointer.to_string(),
                message: format!(
                    "expected {}, found {}",
                    self.types
                        .iter()
                        .map(|kind| kind.as_str())
                        .collect::<Vec<_>>()
                        .join(" or "),
                    describe(value)
                ),
            });
            // No point checking `minimum` on something that is not a number.
            return;
        }

        if let Some(members) = &self.enumeration {
            if !members.contains(value) {
                errors.push(SchemaError {
                    pointer: pointer.to_string(),
                    message: format!(
                        "expected one of [{}], found {}",
                        members.iter().map(render).collect::<Vec<_>>().join(", "),
                        render(value)
                    ),
                });
            }
        }

        if let Some(expected) = &self.constant {
            if expected != value {
                errors.push(SchemaError {
                    pointer: pointer.to_string(),
                    message: format!("expected {}, found {}", render(expected), render(value)),
                });
            }
        }

        match value {
            Value::Number(number) => self.check_number(number.as_f64(), pointer, errors),
            Value::String(text) => self.check_string(text, pointer, errors),
            Value::Array(items) => self.check_array(items, pointer, errors),
            Value::Object(map) => self.check_object(map, pointer, errors),
            Value::Bool(_) | Value::Null => {}
        }
    }

    fn check_number(&self, found: Option<f64>, pointer: &str, errors: &mut Vec<SchemaError>) {
        let Some(found) = found else { return };
        if let Some(minimum) = self.minimum {
            if found < minimum {
                errors.push(SchemaError {
                    pointer: pointer.to_string(),
                    message: format!("expected at least {minimum}, found {found}"),
                });
            }
        }
        if let Some(maximum) = self.maximum {
            if found > maximum {
                errors.push(SchemaError {
                    pointer: pointer.to_string(),
                    message: format!("expected at most {maximum}, found {found}"),
                });
            }
        }
    }

    fn check_string(&self, text: &str, pointer: &str, errors: &mut Vec<SchemaError>) {
        let length = text.chars().count();
        if let Some(minimum) = self.min_length {
            if length < minimum {
                errors.push(SchemaError {
                    pointer: pointer.to_string(),
                    message: format!("expected at least {minimum} characters, found {length}"),
                });
            }
        }
        if let Some(maximum) = self.max_length {
            if length > maximum {
                errors.push(SchemaError {
                    pointer: pointer.to_string(),
                    message: format!("expected at most {maximum} characters, found {length}"),
                });
            }
        }
    }

    fn check_array(&self, items: &[Value], pointer: &str, errors: &mut Vec<SchemaError>) {
        if let Some(minimum) = self.min_items {
            if items.len() < minimum {
                errors.push(SchemaError {
                    pointer: pointer.to_string(),
                    message: format!("expected at least {minimum} items, found {}", items.len()),
                });
            }
        }
        if let Some(maximum) = self.max_items {
            if items.len() > maximum {
                errors.push(SchemaError {
                    pointer: pointer.to_string(),
                    message: format!("expected at most {maximum} items, found {}", items.len()),
                });
            }
        }
        if let Some(schema) = &self.items {
            for (index, item) in items.iter().enumerate() {
                schema.check(item, &format!("{pointer}/{index}"), errors);
            }
        }
    }

    fn check_object(&self, map: &Map<String, Value>, pointer: &str, errors: &mut Vec<SchemaError>) {
        for name in &self.required {
            if !map.contains_key(name) {
                errors.push(SchemaError {
                    pointer: format!("{pointer}/{name}"),
                    message: "required, but missing".to_string(),
                });
            }
        }
        for (name, child) in map {
            let child_pointer = format!("{pointer}/{name}");
            match self.properties.get(name) {
                Some(schema) => schema.check(child, &child_pointer, errors),
                None => match &self.additional {
                    Additional::Allowed => {}
                    Additional::Forbidden => errors.push(SchemaError {
                        pointer: child_pointer,
                        message: "not defined by the schema, and extra fields are not allowed"
                            .to_string(),
                    }),
                    Additional::Constrained(schema) => schema.check(child, &child_pointer, errors),
                },
            }
        }
    }
}

fn parse_types(setting: &Value, pointer: &str) -> Result<Vec<Kind>, AuraError> {
    let names: Vec<&str> = match setting {
        Value::String(name) => vec![name.as_str()],
        Value::Array(members) => members.iter().filter_map(Value::as_str).collect(),
        _ => return Err(unsupported(pointer, "type must be a string or an array")),
    };
    names
        .into_iter()
        .map(|name| {
            Kind::parse(name).ok_or_else(|| unsupported(pointer, &format!("unknown type `{name}`")))
        })
        .collect()
}

fn as_usize(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|n| usize::try_from(n).ok())
}

fn unsupported(pointer: &str, message: &str) -> AuraError {
    schema_invalid("schema", format!("{}: {message}", display_pointer(pointer)))
}

fn display_pointer(pointer: &str) -> &str {
    if pointer.is_empty() {
        "(root)"
    } else {
        pointer
    }
}

/// What kind of thing was actually found, for the error message.
fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// A short rendering of a value for an error message, never the whole document.
fn render(value: &Value) -> String {
    let text = match value {
        Value::String(text) => format!("\"{text}\""),
        other => other.to_string(),
    };
    if text.chars().count() <= 40 {
        return text;
    }
    let head: String = text.chars().take(37).collect();
    format!("{head}...")
}
