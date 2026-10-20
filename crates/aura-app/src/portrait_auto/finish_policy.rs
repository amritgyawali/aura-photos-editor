//! Frame eligibility and a shared budget for overlapping surface treatments.
//! These are planning constraints, not a claim of competitor quality equivalence.
use super::{Edit, Settings, Tool};
use aura_vision::portrait::PortraitFace;

/// Count every detected face, including small background faces. Selecting the main
/// subject must not turn a group photograph into an eligible isolated portrait.
pub(crate) fn advanced_allowed(faces: &[PortraitFace], settings: &Settings) -> bool {
    if !settings.portrait_only_advanced {
        return true;
    }
    let [face] = faces else { return false };
    let area = (face.bounds[2] - face.bounds[0]) * (face.bounds[3] - face.bounds[1]);
    face.confidence.is_finite() && face.confidence >= 0.8 && area.is_finite() && area >= 0.008
}

pub(crate) fn is_advanced(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Frequency
            | Tool::FrequencyHeal
            | Tool::MicroDodgeBurn
            | Tool::PortraitDodgeBurn
            | Tool::Dodge
            | Tool::Burn
    )
}

/// Group spot cleanup uses supported compact donor repairs, without a broad
/// whole-mask frequency-healing pass.
pub(crate) fn settings(mut chosen: Settings, allowed: bool) -> Settings {
    if !allowed {
        chosen.frequency_heal = 0.0;
        chosen.light_evenness = 0.0;
        chosen.micro_dodge_burn = 0.0;
        chosen.pore_refine = 0.0;
        chosen.glow = 0.0;
        chosen.skin_brightness = 0.0;
        chosen.face_light = 0.0;
        chosen.contour = 0.0;
        chosen.highlight = 0.0;
        chosen.hair_detail = 0.0;
    }
    chosen
}

/// The broad skin pass and the continuous repair finish overlap. Limit the
/// second pass to the unused part of an 85% surface-treatment budget, rather
/// than applying both at full requested strength. Healing itself is untouched.
pub(crate) fn balance_surface(skin: &[Edit], finishing: &mut Vec<Edit>) -> bool {
    let primary = skin
        .iter()
        .filter(|e| e.enabled && e.tool == Tool::SkinSmooth)
        .map(|e| e.amount)
        .fold(0.0_f32, f32::max);
    let remaining = (0.85 - primary).max(0.0);
    let mut adjusted = false;
    for edit in finishing
        .iter_mut()
        .filter(|e| e.tool == Tool::Frequency && e.id.ends_with("-surface-finish"))
    {
        if edit.amount > remaining {
            edit.amount = remaining;
            adjusted = true;
        }
    }
    finishing.retain(|e| e.amount > 0.0);
    adjusted
}

#[cfg(test)]
mod tests {
    use super::*;
    fn face(confidence: f32) -> PortraitFace {
        PortraitFace {
            bounds: [0.2, 0.1, 0.8, 0.9],
            confidence,
            landmarks: [
                [0.35, 0.35],
                [0.65, 0.35],
                [0.5, 0.5],
                [0.4, 0.65],
                [0.6, 0.65],
            ],
        }
    }
    #[test]
    fn only_one_confident_subject_is_eligible_even_when_main_subject_only_is_chosen() {
        let s = Settings {
            main_subject_only: true,
            portrait_only_advanced: true,
            ..Settings::default()
        };
        assert!(advanced_allowed(&[face(0.99)], &s));
        assert!(!advanced_allowed(&[face(0.99), face(0.99)], &s));
        assert!(!advanced_allowed(&[], &s));
        assert!(!advanced_allowed(&[face(0.7)], &s));
        assert!(!advanced_allowed(&[face(f32::NAN)], &s));
        assert!(advanced_allowed(
            &[],
            &Settings {
                portrait_only_advanced: false,
                ..s
            }
        ));
    }
    #[test]
    fn group_policy_keeps_spot_healing_and_under_eye_body_controls() {
        let s = Settings {
            frequency_heal: 1.0,
            body_blemishes: 0.8,
            dark_circles: 0.75,
            pore_refine: 0.9,
            face_light: 1.0,
            ..Settings::default()
        };
        let used = settings(s, false);
        assert_eq!(
            (used.frequency_heal, used.body_blemishes, used.dark_circles),
            (0.0, 0.8, 0.75)
        );
        assert_eq!(
            (
                used.pore_refine,
                used.face_light,
                used.micro_dodge_burn,
                used.light_evenness
            ),
            (0.0, 0.0, 0.0, 0.0)
        );
        assert_eq!(settings(s, true), s);
        assert!(is_advanced(Tool::FrequencyHeal));
        assert!(!is_advanced(Tool::PatchHeal));
        assert!(!is_advanced(Tool::SkinSmooth));
    }
}
