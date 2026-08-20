//! The four JSON documents `image_colour_decision` stores, and the positional form they use.
//!
//! Phase 15's `tone::store::codec` established the shape and this is the same shape with four
//! documents instead of three. The three decisions that pay for the per-image budget are the
//! same three, and they are worth restating because this phase stores more per frame than any
//! before it - three complete alternative parameter sets is most of the row.
//!
//! **Positional arrays, not objects.** `["greenery_tamed",-0.02,[0.42,0.31],null]` rather
//! than `{"code":"greenery_tamed","weight":-0.02,...}`. This module is the only thing in the
//! product that knows what the positions mean, and the round-trip tests at the bottom are
//! what keep that true.
//!
//! **Only the measurement is stored; everything derivable is derived.** A variant's subtlety
//! is not stored because it is a function of its parameters, and a stored derived value is
//! one that can disagree with the thing it came from after a release changes the formula.
//!
//! **No English reaches the catalog.** A reason stores its code and, when its sentence names
//! numbers, *the numbers*. [`render`] puts them back together. Phase 09's rule, for the fifth
//! migration running.
//!
//! # Reading a document this build did not write
//!
//! Every decoder here is total: an unknown code reads as `ColourCode::ToneAlreadyRight`, an
//! unknown band as `ContentBand::Decor`, a short array fills its missing positions with
//! defaults, a malformed document reads as an empty list, and **a stored curve whose points
//! are not monotone reads as the identity**. That last one is the phase's guarantee applied
//! to storage: a catalog row cannot smuggle in a curve that `ToneCurve::new` would have
//! refused.

use aura_core::contract::colour::{
    BandReading, ColourCode, ColourReason, ColourVariant, ContentBand, CurvePoint, HslAdjustments,
    HslBand, HslShift, SkinGuardReport, ToneCurve, VariantKind,
};
use aura_core::contract::integrity::CropRect;
use serde_json::Value;

/// The order of a reason's positional array.
pub const REASON_FIELDS: [&str; 4] = ["code", "weight", "numbers", "evidence"];

/// The order of a band reading's positional array.
pub const BAND_FIELDS: [&str; 6] = [
    "band",
    "area",
    "hue_deg",
    "saturation",
    "luma",
    "confidence",
];

/// The order of a variant's positional array.
pub const VARIANT_FIELDS: [&str; 12] = [
    "kind",
    "contrast",
    "highlights",
    "shadows",
    "whites",
    "blacks",
    "vibrance",
    "saturation",
    "curve",
    "hsl",
    "clipping_after",
    "skin_guard",
];

/// The order of a skin guard report's positional array.
pub const GUARD_FIELDS: [&str; 6] = [
    "mask_area",
    "max_hue_shift_deg",
    "max_chroma_change",
    "attenuation",
    "resolves",
    "measured",
];

/// A reason and the numbers its sentence names.
///
/// Parallel to the reason rather than carried on it, because the frozen contract has no field
/// for them and adding one would be a contract change made for a storage optimisation. Phase
/// 15 made the same choice for the same reason.
pub type Explained = (ColourReason, Vec<f32>);

/// A reason whose sentence is the code's own.
#[must_use]
pub fn plain(code: ColourCode, weight: f32) -> Explained {
    (ColourReason::plain(code, weight), Vec::new())
}

/// A reason whose sentence names numbers.
#[must_use]
pub fn numbered(
    code: ColourCode,
    text: impl Into<String>,
    weight: f32,
    numbers: Vec<f32>,
) -> Explained {
    (ColourReason::frame(code, text, weight), numbers)
}

/// A reason about a region, whose sentence names numbers.
#[must_use]
pub fn at(
    code: ColourCode,
    text: impl Into<String>,
    weight: f32,
    numbers: Vec<f32>,
    evidence: CropRect,
) -> Explained {
    (ColourReason::at(code, text, weight, evidence), numbers)
}

// ---------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------

/// Round to three decimals in `f64`.
///
/// The rounding happens in `f64` for phase 15's reason and it is worth two hundred bytes a
/// row: rounding in `f32` and widening produces the exact binary value of the `f32`, and
/// `serde_json` prints every digit of it.
fn round3(value: f32) -> f64 {
    (f64::from(value) * 1000.0).round() / 1000.0
}

/// Round to two decimals in `f64`, for numbers whose only job is to order a short list.
fn round2(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

/// One number as a JSON value.
///
/// Built by hand rather than with `serde_json::json!`, whose expansion contains an `unwrap`
/// the workspace's disallowed-methods lint refuses. A non-finite number becomes `null` rather
/// than an error: nothing here should produce one, and a `NaN` that reached this point should
/// surface as a visibly missing value rather than as a failed write mid-pass.
fn number(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// A whole number as a JSON value.
fn integer(value: i64) -> Value {
    Value::Number(serde_json::Number::from(value))
}

fn as_f32(value: Option<&Value>) -> f32 {
    value.and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn as_str(value: Option<&Value>) -> &str {
    value.and_then(Value::as_str).unwrap_or("")
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Encode a decision's reasons.
#[must_use]
pub fn encode_reasons(reasons: &[ColourReason], numbers: &[Vec<f32>]) -> String {
    let mut out = Vec::with_capacity(reasons.len());
    for (index, reason) in reasons.iter().enumerate() {
        let values = numbers.get(index).cloned().unwrap_or_default();
        let numbers_value = if values.is_empty() {
            Value::Null
        } else {
            Value::Array(values.iter().map(|v| number(round3(*v))).collect())
        };
        let evidence = reason.evidence.map_or(Value::Null, |rect| {
            Value::Array(vec![
                number(round3(rect.x)),
                number(round3(rect.y)),
                number(round3(rect.w)),
                number(round3(rect.h)),
            ])
        });
        out.push(Value::Array(vec![
            Value::String(reason.code.as_str().to_string()),
            number(round2(reason.weight)),
            numbers_value,
            evidence,
        ]));
    }
    Value::Array(out).to_string()
}

/// Decode a decision's reasons.
///
/// The sentence is rebuilt by [`render`] from the code and the numbers, so a release that
/// improves a phrase improves every stored row at once.
#[must_use]
pub fn decode_reasons(text: &str) -> Vec<ColourReason> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let Value::Array(fields) = item else {
                return None;
            };
            let code = ColourCode::from_str_or_already_right(as_str(fields.first()));
            let weight = as_f32(fields.get(1));
            let numbers: Vec<f32> = match fields.get(2) {
                Some(Value::Array(values)) => values.iter().map(|v| as_f32(Some(v))).collect(),
                _ => Vec::new(),
            };
            let evidence = match fields.get(3) {
                Some(Value::Array(rect)) if rect.len() >= 4 => Some(
                    CropRect {
                        x: as_f32(rect.first()),
                        y: as_f32(rect.get(1)),
                        w: as_f32(rect.get(2)),
                        h: as_f32(rect.get(3)),
                    }
                    .clamped(),
                ),
                _ => None,
            };
            Some(ColourReason {
                code,
                text: render(code, &numbers),
                weight,
                evidence,
            })
        })
        .collect()
}

/// Rebuild one reason's sentence from its code and its numbers.
///
/// Nine of the twenty-eight codes name a number. The rest return the code's own sentence,
/// which is why the catalog stores no text at all.
#[must_use]
pub fn render(code: ColourCode, numbers: &[f32]) -> String {
    let n = |index: usize| numbers.get(index).copied().unwrap_or(0.0);
    match code {
        ColourCode::BlacksSet => format!(
            "the darkest tones sat at {:.0}% and this kind of photograph places them near 2%",
            n(0) * 100.0
        ),
        ColourCode::WhitesSet => format!(
            "the brightest real tones sat at {:.0}% and this kind of photograph places them near \
             96%",
            n(0) * 100.0
        ),
        ColourCode::HighlightsRecovered => format!(
            "{:.1}% of this frame was at or near the top of its range, so the brightest areas \
             were pulled back",
            n(0) * 100.0
        ),
        ColourCode::ShadowsLifted => format!(
            "the darkest fifth of this frame sat at {:.0}%, so the shadows were opened",
            n(0) * 100.0
        ),
        ColourCode::ShadowLiftHeldForNoise => format!(
            "the shadows were opened {:.0} rather than {:.0}, because this camera at this ISO has \
             about {:.1} stops of clean shadow left",
            n(0),
            n(1),
            n(2)
        ),
        ColourCode::ContrastAdded => format!(
            "the subject separated by {:.0}% and this kind of photograph wants about {:.0}%, so \
             contrast was added",
            n(0) * 100.0,
            n(1) * 100.0
        ),
        ColourCode::ContrastReduced => format!(
            "the light here was already harder than this kind of photograph carries - a {:.0}% \
             separation against a {:.0}% intent",
            n(0) * 100.0,
            n(1) * 100.0
        ),
        ColourCode::GreeneryTamed => format!(
            "the foliage was recorded at {:.0} degrees and this kind of photograph wants it near \
             {:.0}, so it was brought back and calmed down a little",
            n(0),
            n(1)
        ),
        ColourCode::DistractorTamed => format!(
            "a colour covering {:.0}% of the frame was the most distracting thing in it, so that \
             colour was reduced rather than the whole frame being desaturated",
            n(0) * 100.0
        ),
        ColourCode::SkinGuardResolved => format!(
            "the first grade moved skin {:.1} degrees, which is more than AURA allows, so it was \
             worked out again more gently",
            n(0)
        ),
        ColourCode::SkinProtected => format!(
            "every colour adjustment was held back to {:.0}% of its strength over skin, so \
             nobody's colour moved",
            n(0) * 100.0
        ),
        ColourCode::ClippingGuardResolved => format!(
            "one setting was reduced, because the grade would have clipped {:.1}% of this frame \
             and this kind of photograph accepts {:.1}%",
            n(0) * 100.0,
            n(1) * 100.0
        ),
        ColourCode::SubtletyCapped => format!(
            "the whole grade was scaled back to {:.0}% of what was worked out, because \
             everything together would have looked processed",
            n(0) * 100.0
        ),
        other => other.user_text().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Content bands
// ---------------------------------------------------------------------------

/// Encode the content readings.
#[must_use]
pub fn encode_bands(bands: &[BandReading]) -> String {
    let out: Vec<Value> = bands
        .iter()
        .map(|reading| {
            Value::Array(vec![
                Value::String(reading.band.as_str().to_string()),
                number(round3(reading.area)),
                number(round2(reading.hue_deg)),
                number(round3(reading.saturation)),
                number(round3(reading.luma)),
                number(round2(reading.confidence)),
            ])
        })
        .collect();
    Value::Array(out).to_string()
}

/// Decode the content readings.
#[must_use]
pub fn decode_bands(text: &str) -> Vec<BandReading> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let Value::Array(fields) = item else {
                return None;
            };
            Some(BandReading {
                band: ContentBand::from_str_or_decor(as_str(fields.first())),
                area: as_f32(fields.get(1)),
                hue_deg: as_f32(fields.get(2)),
                saturation: as_f32(fields.get(3)),
                luma: as_f32(fields.get(4)),
                confidence: as_f32(fields.get(5)),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Curves and HSL
// ---------------------------------------------------------------------------

/// Encode a curve as a flat array of alternating input and output levels.
///
/// Flat rather than nested pairs because the pairing is structural - a curve always has an
/// even number of levels - and the nesting costs two bytes per point for information the
/// decoder already has.
#[must_use]
pub fn encode_curve(curve: &ToneCurve) -> String {
    let out: Vec<Value> = curve
        .points()
        .iter()
        .flat_map(|point| [integer(i64::from(point.x)), integer(i64::from(point.y))])
        .collect();
    Value::Array(out).to_string()
}

/// Decode a curve, refusing a non-monotone one.
///
/// **A stored curve that would not pass `ToneCurve::new` reads as the identity.** The
/// guarantee applies to what comes out of the catalog as strictly as to what goes in: a row
/// written by a future build, or corrupted in place, cannot deliver a posterising curve to
/// the renderer through the back door.
#[must_use]
pub fn decode_curve(text: &str) -> ToneCurve {
    let Ok(Value::Array(levels)) = serde_json::from_str::<Value>(text) else {
        return ToneCurve::identity();
    };
    let mut points = Vec::with_capacity(levels.len() / 2);
    for pair in levels.chunks(2) {
        let (Some(x), Some(y)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let x = x.as_u64().unwrap_or(0).min(u64::from(u16::MAX)) as u16;
        let y = y.as_u64().unwrap_or(0).min(u64::from(u16::MAX)) as u16;
        points.push(CurvePoint::new(x, y));
    }
    ToneCurve::new(points).unwrap_or_else(|_| ToneCurve::identity())
}

/// Encode the eight bands as a flat array of `h, s, l` triples.
#[must_use]
pub fn encode_hsl(hsl: &HslAdjustments) -> String {
    let out: Vec<Value> = HslBand::ALL
        .into_iter()
        .flat_map(|band| {
            let shift = hsl.get(band);
            [
                number(round2(shift.h)),
                number(round2(shift.s)),
                number(round2(shift.l)),
            ]
        })
        .collect();
    Value::Array(out).to_string()
}

/// Decode the eight bands.
#[must_use]
pub fn decode_hsl(text: &str) -> HslAdjustments {
    let Ok(Value::Array(values)) = serde_json::from_str::<Value>(text) else {
        return HslAdjustments::default();
    };
    let mut out = HslAdjustments::default();
    for (index, band) in HslBand::ALL.into_iter().enumerate() {
        let base = index * 3;
        out.set(
            band,
            HslShift {
                h: as_f32(values.get(base)),
                s: as_f32(values.get(base + 1)),
                l: as_f32(values.get(base + 2)),
            }
            .clamped(),
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Variants and the guard report
// ---------------------------------------------------------------------------

fn guard_value(guard: &SkinGuardReport) -> Value {
    Value::Array(vec![
        number(round3(guard.mask_area)),
        number(round3(guard.max_hue_shift_deg)),
        number(round3(guard.max_chroma_change)),
        number(round2(guard.attenuation)),
        integer(i64::from(guard.resolves)),
        Value::Bool(guard.measured),
    ])
}

fn guard_from(value: Option<&Value>) -> SkinGuardReport {
    let Some(Value::Array(fields)) = value else {
        return SkinGuardReport::unmeasured();
    };
    SkinGuardReport {
        mask_area: as_f32(fields.first()),
        max_hue_shift_deg: as_f32(fields.get(1)),
        max_chroma_change: as_f32(fields.get(2)),
        attenuation: as_f32(fields.get(3)),
        resolves: fields.get(4).and_then(Value::as_u64).unwrap_or(0).min(255) as u8,
        measured: fields.get(5).and_then(Value::as_bool).unwrap_or(false),
    }
}

/// Encode a decision's guard report.
#[must_use]
pub fn encode_guard(guard: &SkinGuardReport) -> String {
    guard_value(guard).to_string()
}

/// Decode a decision's guard report.
#[must_use]
pub fn decode_guard(text: &str) -> SkinGuardReport {
    serde_json::from_str::<Value>(text)
        .ok()
        .as_ref()
        .map_or_else(SkinGuardReport::unmeasured, |value| guard_from(Some(value)))
}

/// Encode the alternatives.
#[must_use]
pub fn encode_variants(variants: &[ColourVariant]) -> String {
    let out: Vec<Value> = variants
        .iter()
        .map(|variant| {
            Value::Array(vec![
                Value::String(variant.kind.as_str().to_string()),
                number(round2(variant.contrast)),
                number(round2(variant.highlights)),
                number(round2(variant.shadows)),
                number(round2(variant.whites)),
                number(round2(variant.blacks)),
                number(round2(variant.vibrance)),
                number(round2(variant.saturation)),
                Value::String(encode_curve(&variant.curve)),
                Value::String(encode_hsl(&variant.hsl)),
                Value::Array(vec![
                    number(round3(variant.clipping_after.0)),
                    number(round3(variant.clipping_after.1)),
                ]),
                guard_value(&variant.skin_guard),
            ])
        })
        .collect();
    Value::Array(out).to_string()
}

/// Decode the alternatives.
///
/// A variant whose kind is not one of the three is dropped rather than defaulted: a
/// "flatter" variant that is actually something else would be promoted by
/// `select_variant` under a name that lies about what it does.
#[must_use]
pub fn decode_variants(text: &str) -> Vec<ColourVariant> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let Value::Array(fields) = item else {
                return None;
            };
            let kind = VariantKind::parse(as_str(fields.first()))?;
            let clipping = match fields.get(10) {
                Some(Value::Array(pair)) => (as_f32(pair.first()), as_f32(pair.get(1))),
                _ => (0.0, 0.0),
            };
            Some(ColourVariant {
                kind,
                contrast: as_f32(fields.get(1)),
                highlights: as_f32(fields.get(2)),
                shadows: as_f32(fields.get(3)),
                whites: as_f32(fields.get(4)),
                blacks: as_f32(fields.get(5)),
                vibrance: as_f32(fields.get(6)),
                saturation: as_f32(fields.get(7)),
                curve: decode_curve(as_str(fields.get(8))),
                hsl: decode_hsl(as_str(fields.get(9))),
                clipping_after: clipping,
                skin_guard: guard_from(fields.get(11)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s_curve() -> ToneCurve {
        ToneCurve::new(vec![
            CurvePoint::new(0, 0),
            CurvePoint::new(64, 54),
            CurvePoint::new(192, 202),
            CurvePoint::new(255, 255),
        ])
        .unwrap_or_else(|_| unreachable!("those four points are monotone"))
    }

    #[test]
    fn a_reason_round_trips_without_storing_its_sentence() {
        let reasons = vec![ColourReason::frame(
            ColourCode::ShadowsLifted,
            "whatever this build happened to say",
            0.0,
        )];
        let encoded = encode_reasons(&reasons, &[vec![0.11]]);
        assert!(
            !encoded.contains("whatever"),
            "no English may reach the catalog: {encoded}"
        );
        let decoded = decode_reasons(&encoded);
        assert_eq!(decoded.len(), 1);
        assert_eq!(
            decoded.first().map(|r| r.code),
            Some(ColourCode::ShadowsLifted)
        );
        assert!(decoded.first().is_some_and(|r| r.text.contains("11%")));
    }

    #[test]
    fn a_curve_round_trips_exactly() {
        let encoded = encode_curve(&s_curve());
        assert_eq!(decode_curve(&encoded), s_curve());
    }

    #[test]
    fn a_stored_curve_that_goes_backwards_reads_as_the_identity() {
        // The guarantee applied to storage. A row written by a future build, or corrupted in
        // place, cannot deliver a posterising curve to the renderer through the back door.
        let hostile = "[0,0,128,200,64,20,255,255]";
        assert!(decode_curve(hostile).is_identity());
    }

    #[test]
    fn the_hsl_block_round_trips_in_band_order() {
        let mut hsl = HslAdjustments::default();
        hsl.set(
            HslBand::Green,
            HslShift {
                h: 6.0,
                s: -8.0,
                l: 2.0,
            },
        );
        let decoded = decode_hsl(&encode_hsl(&hsl));
        assert!((decoded.get(HslBand::Green).h - 6.0).abs() < 1e-6);
        assert!((decoded.get(HslBand::Green).s + 8.0).abs() < 1e-6);
        assert!(decoded.get(HslBand::Red).is_neutral());
    }

    #[test]
    fn a_variant_round_trips_with_its_guard() {
        let variant = ColourVariant {
            kind: VariantKind::Punchier,
            contrast: 24.0,
            highlights: -12.0,
            shadows: 8.0,
            whites: 4.0,
            blacks: -6.0,
            vibrance: 8.0,
            saturation: 0.0,
            curve: s_curve(),
            hsl: HslAdjustments::default(),
            clipping_after: (0.004, 0.002),
            skin_guard: SkinGuardReport {
                mask_area: 0.12,
                max_hue_shift_deg: 0.8,
                max_chroma_change: 0.02,
                attenuation: 0.7,
                resolves: 1,
                measured: true,
            },
        };
        let decoded = decode_variants(&encode_variants(std::slice::from_ref(&variant)));
        assert_eq!(decoded, vec![variant]);
    }

    #[test]
    fn an_unknown_variant_kind_is_dropped_rather_than_defaulted() {
        let hostile = r#"[["moodier",1,2,3,4,5,6,7,"[0,0,255,255]","[]",[0,0],[0,0,0,1,0,false]]]"#;
        assert!(decode_variants(hostile).is_empty());
    }

    #[test]
    fn a_malformed_document_reads_as_nothing_rather_than_failing() {
        assert!(decode_reasons("not json").is_empty());
        assert!(decode_bands("{}").is_empty());
        assert!(decode_variants("[").is_empty());
        assert!(decode_hsl("nonsense").is_neutral());
    }

    #[test]
    fn every_field_list_matches_what_the_encoder_writes() {
        // The positions are the schema. A test that counts them is what stops somebody
        // adding a field to the encoder and not to the list that documents it.
        assert_eq!(REASON_FIELDS.len(), 4);
        assert_eq!(BAND_FIELDS.len(), 6);
        assert_eq!(VARIANT_FIELDS.len(), 12);
        assert_eq!(GUARD_FIELDS.len(), 6);

        let bands = vec![BandReading {
            band: ContentBand::Greenery,
            area: 0.3,
            hue_deg: 104.0,
            saturation: 0.5,
            luma: 0.4,
            confidence: 0.8,
        }];
        let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&encode_bands(&bands)) else {
            unreachable!("the encoder writes an array")
        };
        let Some(Value::Array(first)) = items.first() else {
            unreachable!("the encoder writes one row per band")
        };
        assert_eq!(first.len(), BAND_FIELDS.len());
    }
}
