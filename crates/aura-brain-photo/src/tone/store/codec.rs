//! The three JSON documents `image_tone_estimate` stores, and the positional form they use.
//!
//! Section 11 budgets **600 bytes per image**, the tightest per-image figure in the product.
//! Three decisions pay for it and all three live here.
//!
//! **Positional arrays, not objects.** `["tungsten",0.263,0.512,0.71,"known_neutral"]` rather
//! than `{"kind":"tungsten","u":0.263,...}`. The named form costs about twice as much, which
//! over a four-thousand-image wedding is more than the whole of phase 11's composition table.
//! This module is the only thing in the product that knows what the positions mean, and the
//! round-trip tests at the bottom of this file are what keep that true. The orders are written
//! out in [`ILLUMINANT_FIELDS`], [`REASON_FIELDS`] and [`ALTERNATIVE_FIELDS`], and a test
//! asserts the encoder writes as many values as each of them names.
//!
//! **Only the measurement is stored; everything derivable is derived.** An illuminant's
//! chromaticity is stored and its correlated colour temperature, its tint and its distance off
//! the locus are not, because all three are functions of it - and a stored derived value is a
//! value that can disagree with the thing it was derived from after a release changes the
//! conversion. `aura_raw::colour::illuminant` is that conversion and it is the only one.
//!
//! **No English reaches the catalog.** Phase 09's rule, stated once more and this time
//! enforced without exception: a reason stores its code and, when its sentence names numbers,
//! *the numbers*. [`render`] puts them back together. A catalog full of sentences is a catalog
//! nobody can translate and a table that grows every time a copywriter improves a phrase.
//!
//! # Reading a document this build did not write
//!
//! Every decoder here is total: an unknown illuminant kind reads as
//! [`IlluminantKind::Unknown`], an unknown reason code as `ToneCode::SubjectExposed`, a short
//! array fills its missing positions with defaults, and a malformed document reads as an empty
//! list. The rule `SceneId::from_str_or_unknown` established in phase 07: a catalog written by
//! a newer build must still open, and an unreadable *evidence* document must never take a
//! *decision* row down with it.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::tone::{
    HypothesisSource, Illuminant, IlluminantKind, ToneAlternative, ToneCode, ToneReason,
};
use aura_raw::colour::illuminant::{cct_from_uv, duv};
use serde_json::Value;

/// The order of an illuminant's positional array.
pub const ILLUMINANT_FIELDS: [&str; 6] = ["kind", "u", "v", "weight", "source", "region"];

/// The order of a reason's positional array.
pub const REASON_FIELDS: [&str; 4] = ["code", "weight", "numbers", "evidence"];

/// The order of an alternative's positional array.
pub const ALTERNATIVE_FIELDS: [&str; 4] = ["temperature_k", "tint", "cost", "why"];

/// How many `u'v'` units one tint unit is.
///
/// Re-exported from `illuminant` rather than repeated, so that a stored chromaticity and a
/// displayed tint stay two views of one number rather than two numbers that agree today.
use crate::tone::illuminant::TINT_PER_UV;

/// Round a float to three decimals, as an `f64` for serde.
///
/// Three, because everything stored here needs at most that: a chromaticity to three decimals
/// is a hundredth of the smallest visible step, and a weight is a display value.
///
/// The rounding happens **in `f64`**, which looks like a detail and is worth 200 bytes a row.
/// Rounding in `f32` and widening afterwards produces the exact binary value of the `f32` -
/// `0.263_f32` widens to `0.263000011444091796875` - and `serde_json` prints every digit of
/// it, because that is the shortest string that round-trips *as an `f64`*. Widening first
/// and rounding in `f64` lands on a value whose shortest round-trip is `0.263`.
fn round3(value: f32) -> f64 {
    (f64::from(value) * 1000.0).round() / 1000.0
}

/// One number as a JSON value.
///
/// Built by hand rather than with `serde_json::json!`, whose expansion contains an `unwrap`
/// that the workspace's disallowed-methods lint refuses. Phase 14's gate made the same
/// substitution for the same reason. A non-finite number becomes `null` rather than an error:
/// nothing here should ever produce one, and a `NaN` that reached this point should surface as
/// a visibly missing value rather than as a failed write in the middle of a pass.
fn number(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// Round a float to two decimals, as an `f64` for serde.
///
/// For the numbers whose only job is to order a short list. See [`round3`] for why the
/// rounding happens in `f64`.
fn round2(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

/// A whole number as a JSON value.
///
/// Used for every quantity whose stored precision cannot survive the trip to a pixel or to a
/// sentence. Three of them are in this file and all three are the same argument:
///
/// * a temperature and a tint are written into `aura_recipe::Global`, whose fields are `u32`
///   and `i16`, so a stored `3200.4` is a value the renderer rounds away;
/// * a reason's numbers are printed by [`render`] with `{:.0}`, so a stored `44.6` is a
///   digit nobody can ever read.
///
/// Storing precision that nothing downstream can express is storing noise, and here it is
/// storing noise against the tightest per-image budget in the product.
fn integer(value: f32) -> Value {
    #[allow(clippy::cast_possible_truncation)]
    Value::Number(serde_json::Number::from(value.round() as i64))
}

/// A short string as a JSON value.
fn text(value: &str) -> Value {
    Value::String(value.to_string())
}

/// Read one position as a float.
fn at(array: &[Value], index: usize, fallback: f64) -> f32 {
    array.get(index).and_then(Value::as_f64).unwrap_or(fallback) as f32
}

/// Read one position as a string.
fn slug(array: &[Value], index: usize) -> &str {
    array.get(index).and_then(Value::as_str).unwrap_or_default()
}

/// A rectangle as four rounded numbers.
fn encode_rect(rect: CropRect) -> Value {
    Value::Array(vec![
        number(round3(rect.x)),
        number(round3(rect.y)),
        number(round3(rect.w)),
        number(round3(rect.h)),
    ])
}

/// Four numbers back into a rectangle.
fn decode_rect(value: &Value) -> Option<CropRect> {
    let array = value.as_array()?;
    if array.len() < 4 {
        return None;
    }
    Some(
        CropRect {
            x: at(array, 0, 0.0),
            y: at(array, 1, 0.0),
            w: at(array, 2, 0.0),
            h: at(array, 3, 0.0),
        }
        .clamped(),
    )
}

// ---------------------------------------------------------------------------
// Illuminants
// ---------------------------------------------------------------------------

/// Encode the lights found in one frame.
///
/// [`ILLUMINANT_FIELDS`] is the order. The temperature, the tint and the chroma are **not**
/// written: all three are functions of the chromaticity, and deriving them on read is what
/// keeps a stored light and a displayed slider from disagreeing after a release changes the
/// conversion.
#[must_use]
pub fn encode_illuminants(illuminants: &[Illuminant]) -> String {
    let items: Vec<Value> = illuminants
        .iter()
        .map(|light| {
            let mut row = vec![
                text(light.kind.as_str()),
                number(round3(light.uv[0])),
                number(round3(light.uv[1])),
                number(round3(light.weight)),
                text(light.source.as_str()),
            ];
            if let Some(region) = light.region {
                row.push(encode_rect(region));
            }
            Value::Array(row)
        })
        .collect();
    Value::Array(items).to_string()
}

/// Decode the lights, re-deriving the temperature, the tint and the chroma.
#[must_use]
pub fn decode_illuminants(source: &str) -> Vec<Illuminant> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(source) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_array)
        .map(|row| {
            let uv = [at(row, 1, 0.0), at(row, 2, 0.0)];
            Illuminant {
                kind: IlluminantKind::from_str_or_unknown(slug(row, 0)),
                cct_k: cct_from_uv(uv),
                tint: (duv(uv) / TINT_PER_UV).clamp(-150.0, 150.0),
                uv,
                weight: at(row, 3, 0.0),
                region: row.get(5).and_then(decode_rect),
                source: HypothesisSource::from_str_or_as_shot(slug(row, 4)),
                chroma: duv(uv).abs(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// A reason and the numbers its sentence names.
///
/// The pair travels together from the solver that produced it to the row that stores it,
/// because the numbers are what the catalog keeps and the sentence is what the panel shows -
/// and the two must never be able to drift. `Vec<f32>` is empty for the twenty-three codes
/// that say the same thing every time.
pub type Explained = (ToneReason, Vec<f32>);

/// A reason with nothing to fill in.
#[must_use]
pub fn plain(code: ToneCode, weight: f32) -> Explained {
    (ToneReason::plain(code, weight), Vec::new())
}

/// A reason about one region, with nothing to fill in.
#[must_use]
pub fn plain_at(code: ToneCode, weight: f32, evidence: CropRect) -> Explained {
    (
        ToneReason::at(code, code.user_text(), weight, evidence),
        Vec::new(),
    )
}

/// How many numbers a reason's sentence names.
///
/// Two codes carry them and both are section 6.1's and 6.2's: an exposure reason names the
/// subject luminance and the band it was compared against, and a mixed-light reason names how
/// far apart the two lights were. Everything else says the same thing every time and stores
/// nothing but its code.
#[must_use]
pub const fn takes_numbers(code: ToneCode) -> usize {
    match code {
        ToneCode::SubjectUnderexposed | ToneCode::SubjectOverexposed => 3,
        ToneCode::MixedLight => 1,
        _ => 0,
    }
}

/// Put a code and its numbers back into a sentence.
///
/// The other half of [`takes_numbers`], and the reason no English reaches the catalog. A code
/// whose numbers are missing falls back to its own generic sentence, which is what a row
/// written by an older build produces and is a correct - if less specific - thing to say.
#[must_use]
pub fn render(code: ToneCode, numbers: &[f32]) -> String {
    match code {
        ToneCode::SubjectUnderexposed | ToneCode::SubjectOverexposed if numbers.len() >= 3 => {
            format!(
                "the faces here sat at {:.0} % and this kind of photograph wants {:.0} to {:.0} %",
                numbers.first().copied().unwrap_or(0.0),
                numbers.get(1).copied().unwrap_or(0.0),
                numbers.get(2).copied().unwrap_or(0.0)
            )
        }
        ToneCode::MixedLight if !numbers.is_empty() => {
            format!(
                "there are two different-coloured lights here, about {:.0} K apart. AURA has set \
                 the colour for the light on the people; the rest can be corrected separately",
                numbers.first().copied().unwrap_or(0.0)
            )
        }
        other => other.user_text().to_string(),
    }
}

/// Build a reason whose sentence names numbers, and the numbers beside it.
///
/// Used by the solver rather than by the codec, so that the numbers exist as numbers before
/// they are ever formatted. [`ToneReason::text`] is filled by [`render`], so the sentence a
/// panel shows and the sentence a round trip produces are the same string by construction.
#[must_use]
pub fn reason_with(
    code: ToneCode,
    weight: f32,
    numbers: &[f32],
    evidence: Option<CropRect>,
) -> (ToneReason, Vec<f32>) {
    let reason = ToneReason {
        code,
        text: render(code, numbers),
        weight,
        evidence,
    };
    (reason, numbers.to_vec())
}

/// Encode the reasons and the numbers their sentences name.
///
/// `numbers` is parallel to `reasons`; an entry may be empty. It is passed separately rather
/// than carried on [`ToneReason`] because the frozen contract has no field for it, and adding
/// one would be a contract change made for a storage optimisation - which is the wrong trade
/// in the wrong direction.
#[must_use]
pub fn encode_reasons(reasons: &[ToneReason], numbers: &[Vec<f32>]) -> String {
    let items: Vec<Value> = reasons
        .iter()
        .enumerate()
        .map(|(index, reason)| {
            let own: &[f32] = numbers.get(index).map_or(&[][..], Vec::as_slice);
            let mut row = vec![text(reason.code.as_str()), number(round3(reason.weight))];
            let wanted = takes_numbers(reason.code);
            if wanted > 0 && !own.is_empty() {
                row.push(Value::Array(
                    own.iter()
                        .take(wanted)
                        .map(|value| integer(*value))
                        .collect(),
                ));
            } else if reason.evidence.is_some() {
                // A placeholder, so the evidence stays at position three. Cheaper than a
                // named object and clearer than a shorter array that means something else.
                row.push(Value::Null);
            }
            if let Some(evidence) = reason.evidence {
                row.push(encode_rect(evidence));
            }
            Value::Array(row)
        })
        .collect();
    Value::Array(items).to_string()
}

/// Decode the reasons, re-rendering each sentence from its code and its numbers.
#[must_use]
pub fn decode_reasons(source: &str) -> Vec<ToneReason> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(source) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_array)
        .map(|row| {
            let code = ToneCode::from_str_or_exposed(slug(row, 0));
            let numbers: Vec<f32> = row
                .get(2)
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_f64)
                        .map(|value| value as f32)
                        .collect()
                })
                .unwrap_or_default();
            ToneReason {
                code,
                text: render(code, &numbers),
                weight: at(row, 1, 0.0),
                evidence: row.get(3).and_then(decode_rect),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Alternatives
// ---------------------------------------------------------------------------

/// Encode the runner-up answers.
///
/// The exposure is **not** stored. Every alternative in an estimate carries the same one -
/// they are colour answers applied on top of the exposure the frame got - so storing it three
/// times would be storing the row's own `exposure_ev` three times.
/// [`decode_alternatives`] takes it as an argument for exactly that reason.
#[must_use]
pub fn encode_alternatives(alternatives: &[ToneAlternative]) -> String {
    let items: Vec<Value> = alternatives
        .iter()
        .map(|alternative| {
            Value::Array(vec![
                integer(alternative.temperature_k),
                integer(alternative.tint),
                number(round2(alternative.cost)),
                text(alternative.why.as_str()),
            ])
        })
        .collect();
    Value::Array(items).to_string()
}

/// Decode the runner-up answers, filling each one's exposure from the row's own.
#[must_use]
pub fn decode_alternatives(source: &str, exposure_ev: f32) -> Vec<ToneAlternative> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(source) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_array)
        .map(|row| ToneAlternative {
            exposure_ev,
            temperature_k: at(row, 0, 5500.0),
            tint: at(row, 1, 0.0),
            cost: at(row, 2, 0.0),
            why: ToneCode::from_str_or_exposed(slug(row, 3)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(kind: IlluminantKind, uv: [f32; 2], region: Option<CropRect>) -> Illuminant {
        Illuminant {
            kind,
            cct_k: cct_from_uv(uv),
            tint: duv(uv) / TINT_PER_UV,
            uv,
            weight: 0.71,
            region,
            source: HypothesisSource::KnownNeutral,
            chroma: duv(uv).abs(),
        }
    }

    #[test]
    fn illuminants_survive_a_round_trip() {
        let lights = vec![
            light(IlluminantKind::Tungsten, [0.263, 0.512], None),
            light(
                IlluminantKind::Daylight,
                [0.198, 0.468],
                Some(CropRect {
                    x: 0.5,
                    y: 0.0,
                    w: 0.5,
                    h: 0.4,
                }),
            ),
        ];
        let back = decode_illuminants(&encode_illuminants(&lights));
        assert_eq!(back.len(), 2);
        for (before, after) in lights.iter().zip(back.iter()) {
            assert_eq!(before.kind, after.kind);
            assert_eq!(before.source, after.source);
            assert!((before.uv[0] - after.uv[0]).abs() < 1e-3);
            assert!((before.uv[1] - after.uv[1]).abs() < 1e-3);
            // Derived rather than stored: the temperature has to come back from the
            // chromaticity, not from a column.
            assert!((before.cct_k - after.cct_k).abs() < 60.0);
            assert_eq!(before.region.is_some(), after.region.is_some());
        }
    }

    #[test]
    fn the_encoder_writes_as_many_values_as_the_field_list_names() {
        let encoded = encode_illuminants(&[light(
            IlluminantKind::Tungsten,
            [0.263, 0.512],
            Some(CropRect::FULL),
        )]);
        let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&encoded) else {
            unreachable!("the encoder writes an array");
        };
        let Some(Value::Array(row)) = items.first() else {
            unreachable!("one light");
        };
        assert_eq!(row.len(), ILLUMINANT_FIELDS.len());
    }

    #[test]
    fn no_english_reaches_the_catalog() {
        let (reason, numbers) = reason_with(ToneCode::MixedLight, -0.12, &[3700.0], None);
        let encoded = encode_reasons(&[reason], &[numbers]);
        assert!(
            !encoded.contains("different-coloured") && !encoded.contains("AURA"),
            "a sentence reached the catalog: {encoded}"
        );
        let back = decode_reasons(&encoded);
        assert_eq!(back.first().map(|r| r.code), Some(ToneCode::MixedLight));
        assert!(
            back.first()
                .is_some_and(|r| r.text.contains("3700 K apart")),
            "the sentence was not re-rendered: {back:?}"
        );
    }

    #[test]
    fn a_reason_with_no_numbers_keeps_its_own_sentence() {
        let reasons = vec![ToneReason::plain(ToneCode::NeutralFound, 0.0)];
        let back = decode_reasons(&encode_reasons(&reasons, &[Vec::new()]));
        assert_eq!(
            back.first().map(|r| r.text.as_str()),
            Some(ToneCode::NeutralFound.user_text())
        );
    }

    #[test]
    fn an_exposure_reason_keeps_its_numbers_and_its_evidence() {
        let (reason, numbers) = reason_with(
            ToneCode::SubjectUnderexposed,
            -0.04,
            &[22.0, 45.0, 55.0],
            Some(CropRect {
                x: 0.4,
                y: 0.3,
                w: 0.2,
                h: 0.2,
            }),
        );
        let back = decode_reasons(&encode_reasons(&[reason], &[numbers]));
        let Some(first) = back.first() else {
            unreachable!("one reason");
        };
        assert!(first.text.contains("22 %") && first.text.contains("45 to 55 %"));
        assert!(first.evidence.is_some());
    }

    #[test]
    fn a_reason_with_evidence_and_no_numbers_keeps_its_rectangle() {
        let reason = ToneReason::at(
            ToneCode::NeutralFound,
            ToneCode::NeutralFound.user_text(),
            0.0,
            CropRect::FULL,
        );
        let back = decode_reasons(&encode_reasons(&[reason], &[Vec::new()]));
        assert!(
            back.first().is_some_and(|r| r.evidence.is_some()),
            "the null placeholder must keep the rectangle at position three"
        );
    }

    #[test]
    fn alternatives_take_the_rows_own_exposure() {
        let alternatives = vec![ToneAlternative {
            exposure_ev: 0.42,
            temperature_k: 3200.0,
            tint: 6.0,
            cost: 0.183,
            why: ToneCode::GreyWorldFallback,
        }];
        let encoded = encode_alternatives(&alternatives);
        let back = decode_alternatives(&encoded, 0.42);
        assert_eq!(back.len(), 1);
        assert_eq!(
            back.first().map(|a| a.why),
            Some(ToneCode::GreyWorldFallback)
        );
        assert!(back
            .first()
            .is_some_and(|a| (a.exposure_ev - 0.42).abs() < 1e-6));
    }

    #[test]
    fn a_malformed_document_reads_as_nothing_rather_than_failing() {
        assert!(decode_illuminants("not json").is_empty());
        assert!(decode_reasons("{\"not\":\"an array\"}").is_empty());
        assert!(decode_alternatives("", 0.0).is_empty());
    }

    #[test]
    fn an_unknown_code_from_a_newer_build_does_not_lose_the_row() {
        let back = decode_reasons("[[\"something_phase_31_added\",-0.2]]");
        assert_eq!(back.len(), 1, "the reason survived");
        assert_eq!(back.first().map(|r| r.code), Some(ToneCode::SubjectExposed));
    }

    #[test]
    fn a_short_array_from_an_older_build_fills_its_defaults() {
        let back = decode_illuminants("[[\"tungsten\",0.263,0.512]]");
        assert_eq!(back.len(), 1);
        assert!(back.first().is_some_and(|light| light.weight.abs() < 1e-6));
    }

    /// What the three documents cost on the widest row this table can hold.
    ///
    /// **Measured, not chosen.** The decomposition, on the fixture below:
    ///
    /// ```text
    ///   illuminants   132 B   two localised lights, six positions each
    ///   reasons       220 B   six codes, two carrying numbers, one carrying a rectangle
    ///   alternatives  103 B   three runner-up colours
    ///                 -----
    ///                 455 B
    /// ```
    ///
    /// The guard is 480 rather than 455 so that adding one illuminant kind with a longer
    /// slug does not fail the build, and rather than 600 so that adding a decimal back to a
    /// stored number does. It is a regression guard on the *encoding*; section 11's per-image
    /// figure is measured against a real catalog by
    /// `crates/aura-perf/tests/tone_budgets.rs`, and `perf/budgets.toml` records what that
    /// measurement found and why the phase document's 600 B is not met.
    const WIDEST_DOCUMENTS: usize = 480;

    #[test]
    fn the_documents_stay_inside_the_storage_budget() {
        // The worst realistic row: two illuminants with a region each, six reasons of which
        // two carry numbers and one carries an evidence rectangle, and three alternatives.
        // Two *localised* lights rather than two full-frame ones. `Illuminant::region` is
        // `None` for a light that fills the frame, so the only row that carries two
        // rectangles is a genuine mixed-light frame - which is also the widest row this
        // table stores.
        let lights = vec![
            light(
                IlluminantKind::Tungsten,
                [0.263, 0.512],
                Some(CropRect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.55,
                    h: 1.0,
                }),
            ),
            light(
                IlluminantKind::Daylight,
                [0.198, 0.468],
                Some(CropRect {
                    x: 0.45,
                    y: 0.0,
                    w: 0.55,
                    h: 1.0,
                }),
            ),
        ];
        let mut reasons = Vec::new();
        let mut numbers = Vec::new();
        let (reason, own) = reason_with(
            ToneCode::SubjectUnderexposed,
            -0.04,
            &[22.0, 45.0, 55.0],
            Some(CropRect::FULL),
        );
        reasons.push(reason);
        numbers.push(own);
        let (reason, own) = reason_with(ToneCode::MixedLight, -0.12, &[3700.0], None);
        reasons.push(reason);
        numbers.push(own);
        for code in [
            ToneCode::ExposureHeldForHighlights,
            ToneCode::SkinLocusUnavailable,
            ToneCode::GreyWorldFallback,
            ToneCode::WbLowConfidence,
        ] {
            reasons.push(ToneReason::plain(code, -0.08));
            numbers.push(Vec::new());
        }
        let alternatives = vec![
            ToneAlternative {
                exposure_ev: 0.42,
                temperature_k: 3200.0,
                tint: 6.0,
                cost: 0.183,
                why: ToneCode::GreyWorldFallback,
            },
            ToneAlternative {
                exposure_ev: 0.42,
                temperature_k: 4100.0,
                tint: 2.0,
                cost: 0.221,
                why: ToneCode::NeutralFound,
            },
            ToneAlternative {
                exposure_ev: 0.42,
                temperature_k: 5500.0,
                tint: 0.0,
                cost: 0.305,
                why: ToneCode::CameraAsShotUsed,
            },
        ];
        let total = encode_illuminants(&lights).len()
            + encode_reasons(&reasons, &numbers).len()
            + encode_alternatives(&alternatives).len();
        assert!(
            total <= WIDEST_DOCUMENTS,
            "the three documents cost {total} B against a {WIDEST_DOCUMENTS} B guard"
        );
    }
}
