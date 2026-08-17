//! Canonical serialisation and the recipe hash.
//!
//! Section 6.4: *"Canonical JSON serialisation (sorted keys, fixed float formatting) so
//! hashes are stable across platforms."* Three rules, and the third is the one that earns
//! its keep.
//!
//! 1. **Keys sorted byte-wise.** `serde_json`'s map is a `BTreeMap` in this build, so this
//!    is already true; it is re-imposed here anyway, because it must not become false when
//!    somebody enables `preserve_order` for an unrelated reason three phases from now.
//! 2. **No insignificant whitespace.** One byte sequence per document.
//! 3. **Every non-integral number formatted as `{:.6}`.** `serde_json` writes the shortest
//!    string that round-trips, which is correct and stable *today*. Six fixed decimals is
//!    stable against a change to that algorithm, against a different serialiser on the far
//!    side of an XMP round trip, and against the difference between the number a person
//!    typed and the same number after one `f32` round trip. The cost is that a parameter is
//!    quantised to a millionth, four orders of magnitude finer than any control in the
//!    product.
//!
//! `docs/adr/ADR-0029-render-pipeline.md` section 5 records the reasoning and the four
//! inputs of `render_hash`.

use std::fmt::Write as _;

use aura_core::errors::render::recipe_invalid;
use aura_core::AuraResult;
use serde_json::Value;

use crate::contract::recipe::Recipe;

/// Decimal places every non-integral number is written with.
///
/// Six, because the finest control in the product is exposure at a hundredth of a stop and
/// the finest derived quantity is a normalised crop edge; both are representable four
/// orders of magnitude coarser than this.
pub const FLOAT_PLACES: usize = 6;

/// Serialise a recipe into its one canonical byte sequence.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the document cannot be represented as JSON at all, which in
/// practice means a non-finite float reached a field. A `NaN` exposure is not a value that
/// can be hashed, rendered or argued with, and it is refused here rather than silently
/// turned into `null` the way `serde_json` would.
pub fn canonical(recipe: &Recipe) -> AuraResult<String> {
    // Checked before serialisation rather than after, because `serde_json` turns a
    // non-finite float into `null` without complaining, and a `null` exposure would then
    // parse back as a missing field and render as zero. A silent zero is the worst of the
    // three possible outcomes; a refusal is the best.
    if let Some(field) = non_finite_field(recipe) {
        return Err(recipe_invalid(field, "not a finite number"));
    }
    let value = serde_json::to_value(recipe)
        .map_err(|err| recipe_invalid("<document>", &err.to_string()))?;
    let mut out = String::with_capacity(2048);
    write_value(&value, &mut out)?;
    Ok(out)
}

/// The first float field in the document that is `NaN` or infinite, if any.
///
/// Enumerated by hand rather than discovered by reflection: the list is short, it is part
/// of the frozen schema, and a new float field that is not added here will be caught by
/// `every_float_field_is_finite_checked` in `schema.rs`.
#[must_use]
pub fn non_finite_field(recipe: &Recipe) -> Option<&'static str> {
    let g = &recipe.global;
    let checks: [(&'static str, f32); 3] = [
        ("global.exposure", g.exposure),
        ("global.sharpen.radius", g.sharpen.radius),
        ("geometry.rotate", recipe.geometry.rotate),
    ];
    for (name, value) in checks {
        if !value.is_finite() {
            return Some(name);
        }
    }
    if recipe.geometry.crop.iter().any(|v| !v.is_finite()) {
        return Some("geometry.crop");
    }
    if let Some(p) = &recipe.geometry.perspective {
        if !p.vertical.is_finite()
            || !p.horizontal.is_finite()
            || !p.rotate.is_finite()
            || !p.scale.is_finite()
        {
            return Some("geometry.perspective");
        }
    }
    for mask in &recipe.masks {
        if !mask.feather.is_finite() {
            return Some("masks.feather");
        }
        if mask.params.exposure.is_some_and(|v| !v.is_finite()) {
            return Some("masks.params.exposure");
        }
    }
    for op in &recipe.retouch {
        if !op.strength.is_finite() || !op.protect_texture.is_finite() {
            return Some("retouch.strength");
        }
    }
    if !recipe.provenance.confidence.is_finite() {
        return Some("provenance.confidence");
    }
    None
}

/// BLAKE3 over the canonical bytes, lower-case hex.
///
/// This is the recipe's identity. Two recipes with the same hash render the same pixels
/// through the same engine, which is what makes the render cache safe and what makes
/// `MergeReport::changed` checkable.
///
/// # Errors
///
/// Propagates [`canonical`].
pub fn recipe_hash(recipe: &Recipe) -> AuraResult<String> {
    Ok(blake3::hash(canonical(recipe)?.as_bytes())
        .to_hex()
        .to_string())
}

/// Format one number the way the canonical form requires.
///
/// Integral values keep their integer spelling - `1`, not `1.000000` - because the schema's
/// integer fields are integers and writing them as floats would make the JSON harder to
/// read for no gain. Everything else takes exactly [`FLOAT_PLACES`] decimals, and negative
/// zero is written as zero because `-0.0 == 0.0` and a hash must not disagree with `==`.
#[must_use]
pub fn number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        // An integral float is an integer in the document. `-0.0` folds into `0` here too.
        let as_int = value as i64;
        return as_int.to_string();
    }
    let mut text = format!("{value:.FLOAT_PLACES$}");
    if text.starts_with("-0.") && text[1..].chars().all(|c| c == '0' || c == '.') {
        text.remove(0);
    }
    text
}

fn write_value(value: &Value, out: &mut String) -> AuraResult<()> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = write!(out, "{i}");
            } else if let Some(u) = n.as_u64() {
                let _ = write!(out, "{u}");
            } else if let Some(f) = n.as_f64() {
                out.push_str(&number(f));
            } else {
                return Err(recipe_invalid("<number>", "not representable"));
            }
        }
        Value::String(s) => write_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            // Re-imposed rather than relied on: see rule 1 in the module documentation.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                let Some(child) = map.get(*key) else {
                    return Err(recipe_invalid(key, "key vanished between sort and read"));
                };
                write_value(child, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn keys_are_sorted_and_whitespace_is_absent() {
        let text = canonical(&fixtures::reference()).expect("canonical");
        // Whitespace *inside* a string value is legitimate - the reference recipe's lens is
        // called "FE 35mm F1.4 GM". What must not appear is whitespace between tokens, which
        // is what a pretty-printer adds and what would make the bytes depend on a formatter.
        assert!(!text.contains(": "));
        assert!(!text.contains(", "));
        assert!(!text.contains('\n'));
        assert!(!text.contains('\t'));
        // `engine` sorts before `geometry` sorts before `global` sorts before `image`.
        let engine = text.find("\"engine\"").expect("engine");
        let geometry = text.find("\"geometry\"").expect("geometry");
        let global = text.find("\"global\"").expect("global");
        let image = text.find("\"image\"").expect("image");
        assert!(engine < geometry && geometry < global && global < image);
    }

    #[test]
    fn floats_take_exactly_six_decimals() {
        assert_eq!(number(0.31), "0.310000");
        assert_eq!(number(0.982), "0.982000");
        assert_eq!(number(-0.6), "-0.600000");
        // Integral values keep their integer spelling.
        assert_eq!(number(4930.0), "4930");
        assert_eq!(number(0.0), "0");
        // Negative zero is zero, because `-0.0 == 0.0` and the hash must agree with `==`.
        assert_eq!(number(-0.0), "0");
    }

    #[test]
    fn the_same_recipe_hashes_the_same_twice() {
        let recipe = fixtures::reference();
        assert_eq!(
            recipe_hash(&recipe).expect("first"),
            recipe_hash(&recipe).expect("second")
        );
    }

    #[test]
    fn a_millionth_of_a_stop_does_not_change_the_hash_and_a_hundredth_does() {
        let base = fixtures::reference();
        let mut finer = base.clone();
        finer.global.exposure += 1e-9;
        assert_eq!(
            recipe_hash(&base).expect("base"),
            recipe_hash(&finer).expect("finer"),
            "quantisation to a millionth is the documented behaviour"
        );

        let mut coarser = base.clone();
        coarser.global.exposure += 0.01;
        assert_ne!(
            recipe_hash(&base).expect("base"),
            recipe_hash(&coarser).expect("coarser")
        );
    }

    #[test]
    fn a_round_trip_through_json_preserves_the_hash() {
        let recipe = fixtures::reference();
        let text = canonical(&recipe).expect("canonical");
        let parsed: Recipe = serde_json::from_str(&text).expect("parse");
        assert_eq!(
            recipe_hash(&recipe).expect("original"),
            recipe_hash(&parsed).expect("parsed")
        );
    }

    #[test]
    fn a_non_finite_number_is_refused_rather_than_written_as_null() {
        let mut recipe = fixtures::reference();
        recipe.global.exposure = f32::NAN;
        let err = canonical(&recipe).expect_err("NaN must not serialise");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8002");
    }
}
