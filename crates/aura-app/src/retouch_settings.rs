//! The automatic retouch's fine controls in ten groups (ADR-0082), including opt-in deep
//! blemish cleanup (ADR-0085).
//!
//! Modelled on what professional portrait retouching tools expose (frequency-separated skin
//! smoothing that keeps pores, tone and light evening, measured blemish healing, dodge and
//! burn, eye vessels and brilliance, teeth, portrait volumes, body skin, fabric and backdrop),
//! with one difference that is a rule rather than a default: every setting *scales a measured
//! correction* or adds a bounded, editable operation. None of them compares anybody's skin with
//! an ideal colour, and there is no setting that reshapes a face or a body.
//!
//! Every value is optional on the wire (`#[serde(default)]`), so options saved before a setting
//! existed keep working and get its neutral value.
use serde::{Deserialize, Serialize};

/// Fine controls. Strengths are `0..1`; `0` switches the operation off. For corrections the
/// planner measures (smoothing, tone, light, lines, under-eyes, eyes, teeth, shine) `0.5` is
/// the measured strength, `1` twice that within each tool's bounds. Signed values are `-1..1`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    // -- Detection -------------------------------------------------------------------------
    /// Find face and body skin with the bundled person segmenter. Off falls back to landmark
    /// geometry and colour sampling (faster, less exact).
    pub ai_skin_detection: bool,
    /// Retouch only the largest face (and its body) instead of everybody.
    pub main_subject_only: bool,
    /// How strictly face skin must match the person's own skin colour (beard, brows, lips and
    /// make-up are left out more as this rises).
    pub mask_precision: f32,
    /// How soft the edges of the skin selection are.
    pub edge_softness: f32,
    /// Keep beard and stubble out of the smoothing, whatever the precision.
    pub protect_facial_hair: bool,
    /// Keep eyelids, inner corners and orbital shadows out of every automatic face step.
    /// Manual edits remain available; disabling this permits the dedicated eye controls.
    pub protect_eye_area: bool,
    /// Preserve nose bridge, nostril edges, pores and shading during automatic retouch.
    pub protect_nose_detail: bool,

    // -- Skin ------------------------------------------------------------------------------
    /// Mid-frequency smoothing between pores and facial form.
    pub smoothing: f32,
    /// How much fine texture (pores) survives smoothing; 1 keeps all of it.
    pub texture: f32,
    /// Size of what counts as unevenness: 0 fine, 1 broad.
    pub smoothing_size: f32,
    /// Blotchy colour evened toward the person's own skin.
    pub tone_evenness: f32,
    /// Uneven light (patchy shadows) evened within the skin.
    pub light_evenness: f32,
    /// Small luminance variations evened (micro dodge and burn).
    pub micro_dodge_burn: f32,
    /// Fine-texture refinement for visibly enlarged pores; never removes texture.
    pub pore_refine: f32,
    /// Oily shine and hot spots softened.
    pub shine: f32,
    /// Redness beside the nose evened toward the cheek.
    pub redness: f32,
    /// A soft lift of the skin's own highlights.
    pub glow: f32,
    /// Brighten (+) or deepen (-) the whole face skin. Neutral by default.
    pub skin_brightness: f32,
    /// Warmer (+) or cooler (-) skin. Neutral by default.
    pub skin_warmth: f32,
    /// Magenta (+) or green (-) skin tint. Neutral by default.
    pub skin_tint: f32,
    /// Real pore texture brought back to an even level after healing and smoothing: glints
    /// limited, and detail from the same face's clean skin added where it is missing. ADR-0090.
    pub texture_graft: f32,

    // -- Blemishes -------------------------------------------------------------------------
    /// How small a departure from the surrounding skin still counts as a blemish.
    pub blemish_sensitivity: f32,
    /// Search the complete segmented face at several spot sizes.
    pub deep_blemish_cleanup: bool,
    /// Include compact dark marks; may also remove freckles or beauty marks.
    pub remove_dark_marks: bool,
    /// Rebuild the tone under every compact mark from the clean skin around it, leaving the
    /// pores where they are (frequency healing). 0 is off. ADR-0090.
    pub frequency_heal: f32,
    /// At most this many spots healed per face (1..=900 in deep cleanup).
    pub max_spots: u16,
    /// Treat a field of many small marks as freckles and keep all of them.
    pub keep_freckles: bool,

    // -- Lines -----------------------------------------------------------------------------
    pub forehead_lines: f32,
    pub crows_feet: f32,
    pub smile_lines: f32,
    pub under_eye_lines: f32,
    pub neck_lines: f32,

    // -- Under eyes ------------------------------------------------------------------------
    pub dark_circles: f32,
    pub eye_bags: f32,

    // -- Eyes and brows --------------------------------------------------------------------
    /// Whites of the eyes brightened a little (never to paper white).
    pub eye_whitening: f32,
    /// Red vessels in the whites reduced.
    pub eye_vessels: f32,
    /// Iris and lash fine detail.
    pub iris_detail: f32,
    /// Iris brilliance: a small lift of the iris.
    pub iris_brightness: f32,
    /// Flash red-eye corrected when measured.
    pub red_eye: bool,
    /// Lash line definition.
    pub lash_definition: f32,
    /// Brow definition.
    pub brow_definition: f32,

    // -- Mouth -----------------------------------------------------------------------------
    pub teeth_whitening: f32,
    /// A natural rose tint on the lips.
    pub lip_colour: f32,
    /// Lip texture and edge definition.
    pub lip_definition: f32,

    // -- Portrait volumes and make-up ------------------------------------------------------
    /// Soft shadow under the cheekbones and along the jaw (burn).
    pub contour: f32,
    /// Soft light on the nose bridge, cheekbones, brow bone and chin (dodge).
    pub highlight: f32,
    /// A warm blush on the cheeks.
    pub blush: f32,
    /// Lift the whole face relative to its surroundings (fill light).
    pub face_light: f32,

    // -- Body ------------------------------------------------------------------------------
    pub body_smoothing: f32,
    pub body_tone: f32,
    /// Opt-in body skin colour matching to the same person's face. Neutral by default:
    /// tanning and differences between face and body are part of the original complexion.
    pub match_body_to_face: f32,
    pub body_shine: f32,
    /// Red hands, knuckles and elbows evened.
    pub body_redness: f32,
    /// Small spots on body skin healed.
    pub body_blemishes: f32,

    // -- Hair, clothes and backdrop --------------------------------------------------------
    pub hair_detail: f32,
    pub hair_shine: f32,
    /// Creases in clothing softened (seams and weave kept).
    pub fabric: f32,
    /// A plain backdrop smoothed (studio paper and walls).
    pub backdrop: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ai_skin_detection: true,
            main_subject_only: false,
            mask_precision: 0.5,
            edge_softness: 0.35,
            protect_facial_hair: true,
            protect_eye_area: true,
            protect_nose_detail: true,
            smoothing: 0.5,
            texture: 0.85,
            smoothing_size: 0.5,
            tone_evenness: 0.5,
            light_evenness: 0.5,
            micro_dodge_burn: 0.25,
            pore_refine: 0.0,
            shine: 0.5,
            redness: 0.5,
            glow: 0.0,
            skin_brightness: 0.0,
            skin_warmth: 0.0,
            skin_tint: 0.0,
            texture_graft: 0.0,
            blemish_sensitivity: 0.5,
            deep_blemish_cleanup: false,
            remove_dark_marks: false,
            frequency_heal: 0.0,
            max_spots: 12,
            keep_freckles: true,
            forehead_lines: 0.5,
            crows_feet: 0.5,
            smile_lines: 0.5,
            under_eye_lines: 0.25,
            neck_lines: 0.0,
            dark_circles: 0.5,
            eye_bags: 0.25,
            eye_whitening: 0.2,
            eye_vessels: 0.5,
            iris_detail: 0.5,
            iris_brightness: 0.0,
            red_eye: true,
            lash_definition: 0.0,
            brow_definition: 0.0,
            teeth_whitening: 0.5,
            lip_colour: 0.0,
            lip_definition: 0.0,
            contour: 0.0,
            highlight: 0.0,
            blush: 0.0,
            face_light: 0.0,
            body_smoothing: 0.5,
            body_tone: 0.5,
            match_body_to_face: 0.0,
            body_shine: 0.25,
            body_redness: 0.0,
            body_blemishes: 0.0,
            hair_detail: 0.0,
            hair_shine: 0.0,
            fabric: 0.0,
            backdrop: 0.0,
        }
    }
}

/// Number of named settings in [`Settings`], for documentation and the UI's own check.
pub const COUNT: usize = 58;

fn unit(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

fn signed(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

impl Settings {
    /// Clamp everything a caller might send out of range.
    #[must_use]
    pub fn sanitised(self) -> Self {
        let d = Self::default();
        Self {
            ai_skin_detection: self.ai_skin_detection,
            main_subject_only: self.main_subject_only,
            mask_precision: unit(self.mask_precision, d.mask_precision),
            edge_softness: unit(self.edge_softness, d.edge_softness),
            protect_facial_hair: self.protect_facial_hair,
            protect_eye_area: self.protect_eye_area,
            protect_nose_detail: self.protect_nose_detail,
            smoothing: unit(self.smoothing, d.smoothing),
            texture: unit(self.texture, d.texture),
            smoothing_size: unit(self.smoothing_size, d.smoothing_size),
            tone_evenness: unit(self.tone_evenness, d.tone_evenness),
            light_evenness: unit(self.light_evenness, d.light_evenness),
            micro_dodge_burn: unit(self.micro_dodge_burn, 0.0),
            pore_refine: unit(self.pore_refine, 0.0),
            shine: unit(self.shine, d.shine),
            redness: unit(self.redness, d.redness),
            glow: unit(self.glow, 0.0),
            skin_brightness: signed(self.skin_brightness),
            skin_warmth: signed(self.skin_warmth),
            skin_tint: signed(self.skin_tint),
            texture_graft: unit(self.texture_graft, 0.0),
            blemish_sensitivity: unit(self.blemish_sensitivity, d.blemish_sensitivity),
            deep_blemish_cleanup: self.deep_blemish_cleanup,
            remove_dark_marks: self.remove_dark_marks,
            frequency_heal: unit(self.frequency_heal, 0.0),
            max_spots: self
                .max_spots
                .clamp(1, if self.deep_blemish_cleanup { 900 } else { 24 }),
            keep_freckles: self.keep_freckles,
            forehead_lines: unit(self.forehead_lines, d.forehead_lines),
            crows_feet: unit(self.crows_feet, d.crows_feet),
            smile_lines: unit(self.smile_lines, d.smile_lines),
            under_eye_lines: unit(self.under_eye_lines, 0.0),
            neck_lines: unit(self.neck_lines, 0.0),
            dark_circles: unit(self.dark_circles, d.dark_circles),
            eye_bags: unit(self.eye_bags, 0.0),
            eye_whitening: unit(self.eye_whitening, 0.0),
            eye_vessels: unit(self.eye_vessels, d.eye_vessels),
            iris_detail: unit(self.iris_detail, d.iris_detail),
            iris_brightness: unit(self.iris_brightness, 0.0),
            red_eye: self.red_eye,
            lash_definition: unit(self.lash_definition, 0.0),
            brow_definition: unit(self.brow_definition, 0.0),
            teeth_whitening: unit(self.teeth_whitening, d.teeth_whitening),
            lip_colour: unit(self.lip_colour, 0.0),
            lip_definition: unit(self.lip_definition, 0.0),
            contour: unit(self.contour, 0.0),
            highlight: unit(self.highlight, 0.0),
            blush: unit(self.blush, 0.0),
            face_light: unit(self.face_light, 0.0),
            body_smoothing: unit(self.body_smoothing, d.body_smoothing),
            body_tone: unit(self.body_tone, d.body_tone),
            match_body_to_face: unit(self.match_body_to_face, 0.0),
            body_shine: unit(self.body_shine, 0.0),
            body_redness: unit(self.body_redness, 0.0),
            body_blemishes: unit(self.body_blemishes, 0.0),
            hair_detail: unit(self.hair_detail, 0.0),
            hair_shine: unit(self.hair_shine, 0.0),
            fabric: unit(self.fabric, 0.0),
            backdrop: unit(self.backdrop, 0.0),
        }
    }
}

/// A measured-correction strength as a multiplier: `0.5` is the measured strength.
#[must_use]
pub fn gain(v: f32) -> f32 {
    (v * 2.0).clamp(0.0, 2.0)
}

/// Scale a detection threshold by a sensitivity: `0.5` keeps it, `1` lowers it by 40 %,
/// `0` raises it by 60 %.
#[must_use]
pub fn threshold(base: f32, sensitivity: f32) -> f32 {
    base * (1.6 - 1.2 * sensitivity.clamp(0.0, 1.0)).max(0.4)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn all_settings_round_trip_and_old_options_get_neutral_values() {
        let value = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(value.as_object().unwrap().len(), COUNT);
        // The wire names the UI sends (ui/src/ipc/nativeRetouch.ts).
        for key in [
            "aiSkinDetection",
            "maskPrecision",
            "crowsFeet",
            "lipColour",
            "matchBodyToFace",
            "maxSpots",
            "backdrop",
        ] {
            assert!(value.get(key).is_some(), "{key}");
        }
        let parsed: Settings =
            serde_json::from_value(serde_json::json!({ "smoothing": 0.8 })).unwrap();
        assert!((parsed.smoothing - 0.8).abs() < 1e-6);
        assert_eq!(parsed.texture, Settings::default().texture);
        assert_eq!(parsed.skin_warmth, 0.0);
        assert!(!parsed.deep_blemish_cleanup && !parsed.remove_dark_marks);
        // Frequency healing and the texture graft are opt-in: options saved before they
        // existed plan exactly what they planned then.
        assert_eq!(parsed.frequency_heal, 0.0);
        assert_eq!(parsed.texture_graft, 0.0);
        assert!(parsed.protect_eye_area && parsed.protect_nose_detail);
    }

    #[test]
    fn sanitising_bounds_every_value() {
        let wild = Settings {
            smoothing: 7.0,
            skin_warmth: -3.0,
            glow: f32::NAN,
            max_spots: 200,
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(wild.smoothing, 1.0);
        assert_eq!(wild.skin_warmth, -1.0);
        assert_eq!(wild.glow, 0.0);
        assert_eq!(wild.max_spots, 24);
        let deep = Settings {
            deep_blemish_cleanup: true,
            max_spots: 1200,
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(deep.max_spots, 900);
        assert_eq!(gain(0.5), 1.0);
        assert!(threshold(1.0, 1.0) < threshold(1.0, 0.5));
        assert!(threshold(1.0, 0.0) > 1.0);
    }
}
