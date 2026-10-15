//! A model refines measured colour; it cannot invent a large cast correction.
// Recipe temperatures are bounded integers; conversion to reciprocal temperature is intentional.
#![allow(clippy::cast_precision_loss)]
use aura_cloud::photo_adjustment::PhotoAdjustment;
use aura_recipe::Global;

// Conservative product limits, not calibrated perceptual-quality thresholds. Reciprocal
// temperature gives the same illuminant-change budget in warm and cool lighting.
const TEMPERATURE_STEP_MIRED: f32 = 8.0;
const TINT_STEP: i16 = 3;
const COLOUR_STEP: i16 = 5;

pub(super) fn apply(
    asked: &PhotoAdjustment,
    measured: &Global,
    proposal: &mut Global,
) -> Vec<String> {
    let mut notes = Vec::new();
    let own_mired = 1_000_000.0 / measured.temperature as f32;
    let asked_mired = 1_000_000.0 / asked.temperature as f32;
    let confirmed = own_mired
        + (asked_mired - own_mired).clamp(-TEMPERATURE_STEP_MIRED, TEMPERATURE_STEP_MIRED);
    // Both source values have already passed recipe/provider validation. The reciprocal
    // step is additionally bounded to the recipe's legal temperature domain.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let temperature = (1_000_000.0 / confirmed).round().clamp(2000.0, 50_000.0) as u32;
    proposal.temperature = temperature;
    proposal.tint = asked.tint.clamp(
        measured.tint.saturating_sub(TINT_STEP),
        measured.tint.saturating_add(TINT_STEP),
    );
    if proposal.temperature != asked.temperature || proposal.tint != asked.tint {
        notes.push(format!(
            "White balance limited to {} K / tint {:+} around the measured {} K / {:+}; the AI requested {} K / {:+}. This preserves the scene's light and skin colour.",
            proposal.temperature, proposal.tint, measured.temperature, measured.tint, asked.temperature, asked.tint
        ));
    }
    proposal.vibrance = asked.vibrance.clamp(
        measured.vibrance.saturating_sub(COLOUR_STEP),
        measured.vibrance.saturating_add(COLOUR_STEP),
    );
    proposal.saturation = asked.saturation.clamp(-COLOUR_STEP, COLOUR_STEP);
    if proposal.vibrance != asked.vibrance || proposal.saturation != asked.saturation {
        notes.push(format!(
            "Colour limited to vibrance {:+} / saturation {:+} around measured vibrance {:+}; the AI requested {:+} / {:+}.",
            proposal.vibrance, proposal.saturation, measured.vibrance, asked.vibrance, asked.saturation
        ));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> Global {
        aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test").global
    }

    fn request(temperature: u32, tint: i16, vibrance: i16, saturation: i16) -> PhotoAdjustment {
        PhotoAdjustment {
            exposure: 0.0,
            contrast: 0,
            highlights: 0,
            shadows: 0,
            temperature,
            tint,
            vibrance,
            saturation,
            preset: "natural".into(),
            confidence: 0.9,
            reasons: vec!["Vision proposal".into()],
        }
    }

    #[test]
    fn extreme_model_colour_cannot_recolour_neutral_portraits_or_warm_scenes() {
        for temperature in [4000, 5500, 7500, 12_000] {
            let measured = Global {
                temperature,
                tint: -8,
                vibrance: 2,
                ..neutral()
            };
            for asked in [request(4000, 30, 20, 10), request(7500, -30, -10, -15)] {
                let mut proposal = neutral();
                let notes = apply(&asked, &measured, &mut proposal);
                let difference = (1_000_000.0 / proposal.temperature as f32
                    - 1_000_000.0 / temperature as f32)
                    .abs();
                assert!(
                    difference < 8.03,
                    "white balance changed by {difference} mired"
                );
                assert!((proposal.tint - measured.tint).abs() <= 3);
                assert!((proposal.vibrance - measured.vibrance).abs() <= 5);
                assert!(proposal.saturation.abs() <= 5);
                assert!(!notes.is_empty());
            }
        }
    }

    #[test]
    fn supported_model_refinements_are_kept_and_do_not_accumulate() {
        let measured = Global {
            temperature: 6200,
            tint: 4,
            vibrance: 12,
            ..neutral()
        };
        let asked = request(6250, 5, 14, 2);
        let mut proposal = neutral();
        assert!(apply(&asked, &measured, &mut proposal).is_empty());
        assert_eq!(
            (
                proposal.temperature,
                proposal.tint,
                proposal.vibrance,
                proposal.saturation
            ),
            (6250, 5, 14, 2)
        );
        let first = proposal.clone();
        assert!(apply(&asked, &measured, &mut proposal).is_empty());
        assert_eq!(proposal, first);
    }
}
