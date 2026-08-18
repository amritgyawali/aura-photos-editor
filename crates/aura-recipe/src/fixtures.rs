//! Recipes with known answers, used by this crate's tests, by `aura-render`'s golden suite
//! and by the phase 14 gate.
//!
//! [`reference`] is **the document printed in PHASE-14 section 5**, field for field. It is
//! not an example of a recipe; it is the one the phase document froze, and a test that its
//! canonical form is stable is the closest this repository can get to a signature on the
//! schema.

use std::collections::BTreeMap;

use crate::contract::recipe::{
    Bw, Curve, EditSource, Geometry, Global, HslShift, ImageRef, Lens, Mask, MaskKind, MaskParams,
    Noise, Provenance, Recipe, Restoration, RetouchOp, Sharpen, ENGINE, SCHEMA_VERSION,
};

/// A content hash that is obviously synthetic and is still 64 hex characters.
pub const FIXTURE_HASH: &str = "a3f200000000000000000000000000000000000000000000000000000000beef";

/// The document from PHASE-14 section 5, exactly.
///
/// Section 5's JSON abbreviates the content hash to `a3f2...`; the full 64 characters here
/// are [`FIXTURE_HASH`] and every other value is verbatim.
///
/// Long by construction: it is a transcription of a frozen document, and breaking it into
/// helpers would put the schema in two places.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn reference() -> Recipe {
    let mut hsl = BTreeMap::new();
    hsl.insert("orange".to_string(), HslShift { h: -2, s: -6, l: 4 });

    Recipe {
        schema: SCHEMA_VERSION,
        engine: ENGINE.to_string(),
        image: ImageRef {
            content_hash: FIXTURE_HASH.to_string(),
            camera: "ILCE-7M4".to_string(),
            profile: "adobe_standard".to_string(),
        },
        global: Global {
            exposure: 0.31,
            contrast: 11,
            temperature: 4930,
            tint: 8,
            highlights: -31,
            shadows: 22,
            whites: 7,
            blacks: -9,
            clarity: 6,
            texture: 4,
            dehaze: 0,
            vibrance: 8,
            saturation: 0,
            curve: Curve {
                points: vec![[0, 0], [64, 58], [128, 132], [255, 255]],
            },
            hsl,
            sharpen: Sharpen {
                amount: 40,
                radius: 0.8,
                detail: 25,
                masking: 30,
            },
            noise: Noise {
                luminance: 22,
                colour: 30,
                detail: 50,
                model: "scene_aware_v1".to_string(),
            },
        },
        lens: Lens {
            distortion: true,
            vignette: 60,
            ca: true,
            profile: Some("FE 35mm F1.4 GM".to_string()),
        },
        geometry: Geometry {
            rotate: -0.6,
            crop: [0.02, 0.01, 0.97, 0.98],
            perspective: None,
        },
        masks: vec![
            Mask {
                id: "m1".to_string(),
                kind: MaskKind::Face,
                target: Some("identity:bride".to_string()),
                invert_of: None,
                feather: 0.35,
                params: MaskParams {
                    exposure: Some(0.18),
                    shadows: Some(8),
                    clarity: Some(-4),
                    temperature: Some(-60),
                    ..MaskParams::default()
                },
            },
            Mask {
                id: "m2".to_string(),
                kind: MaskKind::Background,
                target: None,
                invert_of: Some("subject".to_string()),
                feather: 0.5,
                params: MaskParams {
                    exposure: Some(-0.22),
                    saturation: Some(-6),
                    ..MaskParams::default()
                },
            },
        ],
        retouch: vec![RetouchOp {
            op: "skin_smooth".to_string(),
            strength: 0.35,
            protect_texture: 0.8,
            mask: Some("skin".to_string()),
        }],
        restoration: Restoration {
            denoise: "auto".to_string(),
            face_recovery: 20,
            deblur: 0,
        },
        bw: None,
        provenance: Provenance {
            scene: Some("indoor_ceremony".to_string()),
            style_profile: Some("amrit_v3".to_string()),
            confidence: 0.982,
            decision_id: Some("d_8f21".to_string()),
            source: EditSource::Ai,
            user_edited_fields: Vec::new(),
        },
        extra: BTreeMap::new(),
    }
}

/// The camera's own starting point: every control neutral.
///
/// This is what a photograph carries before any phase has decided anything, and it is what
/// "reset to original" restores. `EditSource::Default` and confidence 1.0, because a recipe
/// that changes nothing is a claim nobody can be wrong about.
#[must_use]
pub fn neutral(content_hash: &str, camera: &str) -> Recipe {
    Recipe {
        schema: SCHEMA_VERSION,
        engine: ENGINE.to_string(),
        image: ImageRef {
            content_hash: content_hash.to_string(),
            camera: camera.to_string(),
            profile: "reference".to_string(),
        },
        global: Global {
            exposure: 0.0,
            contrast: 0,
            temperature: 5500,
            tint: 0,
            highlights: 0,
            shadows: 0,
            whites: 0,
            blacks: 0,
            clarity: 0,
            texture: 0,
            dehaze: 0,
            vibrance: 0,
            saturation: 0,
            curve: Curve::identity(),
            hsl: BTreeMap::new(),
            sharpen: Sharpen::default(),
            noise: Noise::default(),
        },
        lens: Lens::default(),
        geometry: Geometry::default(),
        masks: Vec::new(),
        retouch: Vec::new(),
        restoration: Restoration::default(),
        bw: None,
        provenance: Provenance::default(),
        extra: BTreeMap::new(),
    }
}

/// A black-and-white conversion, for the sidecar and XMP round-trip tests.
#[must_use]
pub fn monochrome() -> Recipe {
    let mut recipe = reference();
    let mut mix = BTreeMap::new();
    mix.insert("red".to_string(), 20i16);
    mix.insert("orange".to_string(), 35i16);
    mix.insert("blue".to_string(), -30i16);
    recipe.bw = Some(Bw {
        mix,
        grade: Some("tri_x_400".to_string()),
    });
    recipe
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Validation;

    #[test]
    fn the_section_5_document_is_valid() {
        Validation::check(&reference()).expect("section 5's own recipe must validate");
    }

    #[test]
    fn the_neutral_recipe_is_valid_and_changes_nothing() {
        let neutral = neutral(FIXTURE_HASH, "ILCE-7M4");
        Validation::check(&neutral).expect("valid");
        assert!(neutral.global.curve.is_identity());
        assert!(neutral.geometry.is_identity());
        assert!(neutral.masks.is_empty());
        assert_eq!(neutral.provenance.source, EditSource::Default);
    }

    #[test]
    fn the_monochrome_recipe_is_valid() {
        Validation::check(&monochrome()).expect("valid");
    }
}
