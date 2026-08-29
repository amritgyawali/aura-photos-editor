//! How far this kind of photograph may be repaired, and what this camera's sensor actually does.
//!
//! Two tables in one module, because they are loaded together, refused together and versioned
//! together into one `profile_ver`. They answer different questions and both are needed before a
//! single frame can be planned.
//!
//! | Table | Question | File |
//! |---|---|---|
//! | [`RestoreProfiles`] | how far this *scene* may be repaired | `config/restore_profiles.toml` |
//! | [`NoiseTable`] | what this *sensor* does to a photon count | `config/noise_models/*.toml` |
//!
//! ## The refusal is whole-file
//!
//! As phases 15 to 21 all do. Half a profile table would denoise the ceremony against measured
//! ceilings and the reception against nothing, and that inconsistency is invisible in a delivered
//! gallery until somebody prints it. `AURA-ML-5111` is run-blocking.
//!
//! ## A studio may lower a ceiling and may never raise one - except one, which runs the other way
//!
//! [`RestoreProfiles::parse`] compares every ceiling against the constants
//! `aura_core::contract::restore` owns. The one exception is `skin_attenuation`, whose bound runs
//! *upward*: a studio may withhold more sharpening from skin than the contract requires and may
//! never withhold less, because "sharpening is explicitly attenuated on skin" is section 6.2 of
//! the phase document rather than a default somebody chose.
//!
//! ## The noise models are not measurements, and every one of them says so
//!
//! There are no camera files in this repository, so all twenty rows are derived from published
//! specifications - as phase 09's calibration table is. Every one carries `measured = false`, and
//! [`aura_core::contract::restore::NoiseModel::tier_ceiling`] turns that into a cap at
//! [`DenoiseTier::Standard`]. ADR-0047 section 3 has the argument for why the asymmetry runs that
//! way rather than the other.

use std::collections::BTreeMap;

use aura_core::contract::error::AuraError;
use aura_core::contract::restore::{
    DenoiseTier, NoiseModel, MAX_FACE_RECOVERY, MAX_SHARPEN_AMOUNT, SKIN_ATTENUATION,
};
use aura_core::SceneId;
use serde::Deserialize;

use crate::errors;

/// The scene-profile file, for the error messages.
pub const PROFILE_FILE: &str = "crates/aura-restore/config/restore_profiles.toml";

/// The scene-profile table compiled into the binary.
const EMBEDDED_PROFILES: &str = include_str!("../config/restore_profiles.toml");

/// Every camera noise model compiled into the binary, as `(slug, text)`.
///
/// Generated beside the files themselves so that a body added to the directory and forgotten
/// here is a build error rather than a silent fallback to the reference model.
const EMBEDDED_NOISE: &[(&str, &str)] = &[
    (
        "canon_eos_5d4",
        include_str!("../config/noise_models/canon_eos_5d4.toml"),
    ),
    (
        "canon_eos_r",
        include_str!("../config/noise_models/canon_eos_r.toml"),
    ),
    (
        "canon_eos_r5",
        include_str!("../config/noise_models/canon_eos_r5.toml"),
    ),
    (
        "canon_eos_r6",
        include_str!("../config/noise_models/canon_eos_r6.toml"),
    ),
    (
        "canon_eos_r6m2",
        include_str!("../config/noise_models/canon_eos_r6m2.toml"),
    ),
    (
        "fujifilm_gfx100s",
        include_str!("../config/noise_models/fujifilm_gfx100s.toml"),
    ),
    (
        "fujifilm_xh2s",
        include_str!("../config/noise_models/fujifilm_xh2s.toml"),
    ),
    (
        "fujifilm_xt4",
        include_str!("../config/noise_models/fujifilm_xt4.toml"),
    ),
    (
        "leica_sl2s",
        include_str!("../config/noise_models/leica_sl2s.toml"),
    ),
    (
        "nikon_d850",
        include_str!("../config/noise_models/nikon_d850.toml"),
    ),
    (
        "nikon_z6_2",
        include_str!("../config/noise_models/nikon_z6_2.toml"),
    ),
    (
        "nikon_z7_2",
        include_str!("../config/noise_models/nikon_z7_2.toml"),
    ),
    (
        "nikon_z8",
        include_str!("../config/noise_models/nikon_z8.toml"),
    ),
    (
        "panasonic_dc_s1r",
        include_str!("../config/noise_models/panasonic_dc_s1r.toml"),
    ),
    (
        "panasonic_dc_s5",
        include_str!("../config/noise_models/panasonic_dc_s5.toml"),
    ),
    (
        "sony_ilce_1",
        include_str!("../config/noise_models/sony_ilce_1.toml"),
    ),
    (
        "sony_ilce_7m3",
        include_str!("../config/noise_models/sony_ilce_7m3.toml"),
    ),
    (
        "sony_ilce_7m4",
        include_str!("../config/noise_models/sony_ilce_7m4.toml"),
    ),
    (
        "sony_ilce_7rm4",
        include_str!("../config/noise_models/sony_ilce_7rm4.toml"),
    ),
    (
        "sony_ilce_7sm3",
        include_str!("../config/noise_models/sony_ilce_7sm3.toml"),
    ),
];

// ---------------------------------------------------------------------------
// The scene table
// ---------------------------------------------------------------------------

/// One scene: how far this kind of photograph may be repaired.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRow {
    /// The strongest denoise tier this scene permits.
    pub max_tier: DenoiseTier,
    /// Whether deconvolution sharpening may run here at all.
    pub sharpen: bool,
    /// Whether face recovery may run here at all.
    pub face_recovery: bool,
    /// Why. Never empty; the loader refuses a row without one.
    pub reason: String,
}

/// The whole scene table plus the two bound blocks and the ladder.
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreProfiles {
    version: u16,
    max_sharpen: f32,
    skin_attenuation: f32,
    max_face_recovery: f32,
    ladder: [f32; 3],
    prominence_raises_above: f32,
    output_raises_above: u32,
    scenes: BTreeMap<String, SceneRow>,
    neutral: SceneRow,
    unlisted: Vec<String>,
}

impl RestoreProfiles {
    /// The table compiled into this build.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5111` when the embedded table will not load, which is a build fault rather than
    /// an installation one and is therefore never expected in the field.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse(EMBEDDED_PROFILES, PROFILE_FILE)
    }

    /// Parse and validate a scene table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5111` naming the key and the rule it broke.
    #[allow(clippy::too_many_lines)]
    pub fn parse(text: &str, file: &str) -> Result<Self, AuraError> {
        let raw: RawProfiles = toml::from_str(text)
            .map_err(|error| errors::profile_refused(file, "table", &error.to_string()))?;

        if raw.version == 0 {
            return Err(errors::profile_refused(
                file,
                "version",
                "must be at least 1: it is written into every stored plan",
            ));
        }

        // --- the bounds the code owns ------------------------------------------------------
        if raw.bounds.max_sharpen > MAX_SHARPEN_AMOUNT + 1e-6 {
            return Err(errors::profile_refused(
                file,
                "bounds.max_sharpen",
                &format!("is above the contract's {MAX_SHARPEN_AMOUNT}, and a file may only lower a ceiling"),
            ));
        }
        if raw.bounds.max_face_recovery > MAX_FACE_RECOVERY + 1e-6 {
            return Err(errors::profile_refused(
                file,
                "bounds.max_face_recovery",
                &format!("is above the contract's {MAX_FACE_RECOVERY}, and a file may only lower a ceiling"),
            ));
        }
        // The one bound that runs the other way. See the module header.
        if raw.bounds.skin_attenuation < SKIN_ATTENUATION - 1e-6 {
            return Err(errors::profile_refused(
                file,
                "bounds.skin_attenuation",
                &format!(
                    "is below the contract's {SKIN_ATTENUATION}; a file may withhold more \
                     sharpening from skin and never less"
                ),
            ));
        }
        if raw.bounds.skin_attenuation > 1.0 {
            return Err(errors::profile_refused(
                file,
                "bounds.skin_attenuation",
                "is above 1.0, which would sharpen skin negatively",
            ));
        }
        if raw.bounds.reason.trim().is_empty() {
            return Err(errors::profile_refused(
                file,
                "bounds",
                "has no written reason, and every ceiling here is a product decision",
            ));
        }

        // --- the ladder --------------------------------------------------------------------
        let ladder = [raw.ladder.light, raw.ladder.standard, raw.ladder.strong];
        if !(ladder[0] < ladder[1] && ladder[1] < ladder[2]) {
            return Err(errors::profile_refused(
                file,
                "ladder",
                "is not strictly increasing, so two tiers would claim the same noise level",
            ));
        }
        if ladder[0] <= 0.0 {
            return Err(errors::profile_refused(
                file,
                "ladder.light",
                "is at or below zero, which would denoise a frame with no noise in it",
            ));
        }
        if raw.ladder.reason.trim().is_empty() {
            return Err(errors::profile_refused(
                file,
                "ladder",
                "has no written reason",
            ));
        }
        if raw.modifiers.reason.trim().is_empty() {
            return Err(errors::profile_refused(
                file,
                "modifiers",
                "has no written reason",
            ));
        }
        if !(0.0..=1.0).contains(&raw.modifiers.prominence_raises_above) {
            return Err(errors::profile_refused(
                file,
                "modifiers.prominence_raises_above",
                "is outside 0..1",
            ));
        }

        // --- the scenes --------------------------------------------------------------------
        let mut scenes = BTreeMap::new();
        for (name, row) in &raw.scene {
            let max_tier = DenoiseTier::parse(&row.max_tier).ok_or_else(|| {
                errors::profile_refused(
                    file,
                    &format!("scene.{name}.max_tier"),
                    "is not one of off, light, standard or strong",
                )
            })?;
            if row.reason.trim().is_empty() {
                return Err(errors::profile_refused(
                    file,
                    &format!("scene.{name}"),
                    "has no written reason, and a threshold nobody can explain is a product \
                     quietly deciding how much of somebody's wedding to smooth away",
                ));
            }
            scenes.insert(
                name.clone(),
                SceneRow {
                    max_tier,
                    sharpen: row.sharpen,
                    face_recovery: row.face_recovery,
                    reason: row.reason.trim().to_string(),
                },
            );
        }

        let neutral = scenes
            .get(SceneId::Unknown.as_str())
            .cloned()
            .ok_or_else(|| {
                errors::profile_refused(
                    file,
                    "scene.unknown",
                    "is missing, and it is the row every unclassified frame is planned against",
                )
            })?;

        // Which of the twenty-two scenes this file does not describe. Reported rather than
        // refused: a new scene added to the vocabulary should not stop a studio's build, and the
        // outline names it so somebody can add the row. Phase 20's rule.
        let unlisted = SceneId::ALL
            .iter()
            .filter(|scene| !scenes.contains_key(scene.as_str()))
            .map(|scene| scene.as_str().to_string())
            .collect();

        Ok(Self {
            version: raw.version,
            max_sharpen: raw.bounds.max_sharpen,
            skin_attenuation: raw.bounds.skin_attenuation,
            max_face_recovery: raw.bounds.max_face_recovery,
            ladder,
            prominence_raises_above: raw.modifiers.prominence_raises_above,
            output_raises_above: raw.modifiers.output_raises_above,
            scenes,
            neutral,
            unlisted,
        })
    }

    /// The table version, written into every stored plan.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// The row for one scene, or the neutral row.
    #[must_use]
    pub fn row(&self, scene: SceneId) -> &SceneRow {
        self.scenes.get(scene.as_str()).unwrap_or(&self.neutral)
    }

    /// True when this scene has a row of its own.
    #[must_use]
    pub fn has_row(&self, scene: SceneId) -> bool {
        self.scenes.contains_key(scene.as_str())
    }

    /// Scenes with no row, which are planned against the neutral one.
    #[must_use]
    pub fn unlisted(&self) -> &[String] {
        &self.unlisted
    }

    /// The largest deconvolution amount any scene may reach.
    #[must_use]
    pub const fn max_sharpen(&self) -> f32 {
        self.max_sharpen
    }

    /// The share of the sharpening amount withheld on skin.
    #[must_use]
    pub const fn skin_attenuation(&self) -> f32 {
        self.skin_attenuation
    }

    /// The largest face-recovery strength any scene may reach.
    #[must_use]
    pub const fn max_face_recovery(&self) -> f32 {
        self.max_face_recovery
    }

    /// The tier one relative sigma asks for, before any modifier or ceiling.
    ///
    /// **The whole of section 6.1's "evidence-based" selection.** `relative` is phase 09's
    /// `noise_sigma_rel`, which is 1.0 at exactly this scene's own tolerance - so a frame below
    /// the first rung is a frame whose noise a photographer has already decided is acceptable,
    /// and the tier is `Off` rather than a small number.
    #[must_use]
    pub fn tier_for(&self, relative: f32) -> DenoiseTier {
        if !relative.is_finite() || relative < self.ladder[0] {
            DenoiseTier::Off
        } else if relative < self.ladder[1] {
            DenoiseTier::Light
        } else if relative < self.ladder[2] {
            DenoiseTier::Standard
        } else {
            DenoiseTier::Strong
        }
    }

    /// True when this subject prominence justifies one step up.
    #[must_use]
    pub fn prominence_raises(&self, prominence: f32) -> bool {
        prominence >= self.prominence_raises_above
    }

    /// True when this delivery size justifies one step up.
    #[must_use]
    pub const fn output_raises(&self, long_edge: u32) -> bool {
        long_edge >= self.output_raises_above
    }
}

// ---------------------------------------------------------------------------
// The camera table
// ---------------------------------------------------------------------------

/// Every camera's noise model, keyed by a normalised make and model.
#[derive(Debug, Clone, PartialEq)]
pub struct NoiseTable {
    version: u16,
    models: BTreeMap<String, NoiseModel>,
}

impl NoiseTable {
    /// The twenty bodies compiled into this build.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5111` when one of the embedded files will not load.
    pub fn embedded() -> Result<Self, AuraError> {
        let mut models = BTreeMap::new();
        let mut version = 0u16;
        for (slug, text) in EMBEDDED_NOISE {
            let file = format!("crates/aura-restore/config/noise_models/{slug}.toml");
            let (key, model) = Self::parse_one(text, &file)?;
            version = version.max(model.table_ver);
            if models.insert(key.clone(), model).is_some() {
                return Err(errors::profile_refused(
                    &file,
                    &key,
                    "is a second model for the same body, and two answers to how much noise a \
                     sensor makes is two different renders of the same photograph",
                ));
            }
        }
        Ok(Self { version, models })
    }

    /// Parse one camera file into its lookup key and its model.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5111` naming the key and the rule it broke.
    pub fn parse_one(text: &str, file: &str) -> Result<(String, NoiseModel), AuraError> {
        let raw: RawNoise = toml::from_str(text)
            .map_err(|error| errors::profile_refused(file, "table", &error.to_string()))?;

        if raw.version == 0 {
            return Err(errors::profile_refused(
                file,
                "version",
                "must be at least 1",
            ));
        }
        if raw.full_well_e <= 0.0 {
            return Err(errors::profile_refused(
                file,
                "full_well_e",
                "is at or below zero, and it is the divisor that turns electrons into the units \
                 the renderer works in",
            ));
        }
        if raw.read_noise_e <= 0.0 {
            return Err(errors::profile_refused(
                file,
                "read_noise_e",
                "is at or below zero, and a sensor with no read noise does not exist",
            ));
        }
        if raw.reason.trim().is_empty() {
            return Err(errors::profile_refused(file, "reason", "is empty"));
        }

        // The normalisation, and it is the only arithmetic in this loader. A signal of `y` in
        // linear working-space units is `y * full_well` electrons, whose variance is
        // `read_e^2 + y * full_well`; dividing through by `full_well^2` puts both terms into the
        // units the renderer measures in and gives `read = read_e / full_well` and
        // `shot = 1 / full_well`.
        let model = NoiseModel {
            camera: normalise(&raw.make, &raw.model),
            iso: raw.iso.max(1),
            read: raw.read_noise_e / raw.full_well_e,
            shot: 1.0 / raw.full_well_e,
            measured: raw.measured,
            table_ver: raw.version,
        };
        if let Some(problem) = model.problem() {
            return Err(errors::profile_refused(file, "model", &problem));
        }
        Ok((model.camera.clone(), model))
    }

    /// The table version, folded into every stored plan's `profile_ver`.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many bodies this table describes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// True when this table describes no body at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// The model for one body, or the reference model.
    ///
    /// **Never `None`.** Phase 14's rule for camera profiles, inherited: every real body renders
    /// through the reference profile and says so. Here every body with no row of its own denoises
    /// through the reference model, `measured` is false either way in this build, and the plan
    /// records `restore_reference_noise_model` so a photographer can tell the two apart.
    #[must_use]
    pub fn model_for(&self, make: &str, model: &str) -> NoiseModel {
        self.models
            .get(&normalise(make, model))
            .cloned()
            .unwrap_or_else(NoiseModel::reference)
    }

    /// True when this table has a row of its own for one body.
    #[must_use]
    pub fn has_row(&self, make: &str, model: &str) -> bool {
        self.models.contains_key(&normalise(make, model))
    }

    /// Every body with a row, in key order. For the gate and the panel.
    #[must_use]
    pub fn bodies(&self) -> Vec<&NoiseModel> {
        self.models.values().collect()
    }
}

/// One lookup key from a make and a model.
///
/// Manufacturers are inconsistent about whitespace and about whether the make repeats inside the
/// model, so both sides are normalised the same way and in exactly one place - phase 09's rule,
/// and phase 09's function shape.
#[must_use]
pub fn normalise(make: &str, model: &str) -> String {
    let make = make.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    let model = model.strip_prefix(&make).map_or(model.as_str(), str::trim);
    let mut key = String::with_capacity(make.len() + model.len() + 1);
    key.push_str(&make);
    key.push(' ');
    key.push_str(model);
    key.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// The wire shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawProfiles {
    version: u16,
    bounds: RawBounds,
    ladder: RawLadder,
    modifiers: RawModifiers,
    scene: BTreeMap<String, RawScene>,
}

#[derive(Debug, Deserialize)]
struct RawBounds {
    max_sharpen: f32,
    skin_attenuation: f32,
    max_face_recovery: f32,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawLadder {
    light: f32,
    standard: f32,
    strong: f32,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawModifiers {
    prominence_raises_above: f32,
    output_raises_above: u32,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawScene {
    max_tier: String,
    sharpen: bool,
    face_recovery: bool,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawNoise {
    version: u16,
    make: String,
    model: String,
    measured: bool,
    iso: u32,
    read_noise_e: f32,
    full_well_e: f32,
    reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_profile_table_loads_and_covers_every_scene() {
        let profiles = RestoreProfiles::embedded().expect("the embedded profile table loads");
        assert!(profiles.version() >= 1);
        assert!(
            profiles.unlisted().is_empty(),
            "scenes with no row: {:?}",
            profiles.unlisted()
        );
        for scene in SceneId::ALL {
            let row = profiles.row(scene);
            assert!(!row.reason.is_empty(), "{scene} has no written reason");
        }
    }

    #[test]
    fn the_neutral_row_is_the_most_conservative_row_in_the_file() {
        // A photograph AURA cannot describe is a photograph it should barely touch, and every
        // other row in the file is an argument for doing more than the neutral one.
        let profiles = RestoreProfiles::embedded().expect("the embedded profile table loads");
        let neutral = profiles.row(SceneId::Unknown);
        assert!(!neutral.sharpen);
        assert!(!neutral.face_recovery);
        for scene in SceneId::ALL {
            let row = profiles.row(scene);
            assert!(
                row.max_tier.rank() >= neutral.max_tier.rank(),
                "{scene} is more conservative than the neutral row"
            );
        }
    }

    #[test]
    fn the_ladder_starts_at_the_scene_tolerance() {
        // Phase 09's `noise_sigma_rel` is 1.0 at exactly the scene's own tolerance, so a frame
        // below the first rung has no more noise than the scene carries well. Denoising it would
        // be removing noise a photographer had already accepted.
        let profiles = RestoreProfiles::embedded().expect("the embedded profile table loads");
        assert_eq!(profiles.tier_for(0.99), DenoiseTier::Off);
        assert_eq!(profiles.tier_for(1.20), DenoiseTier::Light);
        assert_eq!(profiles.tier_for(2.00), DenoiseTier::Standard);
        assert_eq!(profiles.tier_for(4.00), DenoiseTier::Strong);
        assert_eq!(profiles.tier_for(f32::NAN), DenoiseTier::Off);
    }

    #[test]
    fn a_file_that_raises_a_ceiling_is_refused_and_one_that_lowers_it_is_not() {
        let text = include_str!("../config/restore_profiles.toml");
        assert!(RestoreProfiles::parse(text, PROFILE_FILE).is_ok());

        let raised = text.replace("max_sharpen      = 0.50", "max_sharpen      = 0.90");
        let error = RestoreProfiles::parse(&raised, PROFILE_FILE)
            .expect_err("a raised sharpening ceiling is refused");
        assert_eq!(error.code.0, "AURA-ML-5111");

        let lowered = text.replace("max_sharpen      = 0.50", "max_sharpen      = 0.20");
        assert!(
            RestoreProfiles::parse(&lowered, PROFILE_FILE).is_ok(),
            "a studio may lower a ceiling"
        );

        // The one bound that runs the other way.
        let weakened = text.replace("skin_attenuation = 0.80", "skin_attenuation = 0.10");
        assert!(
            RestoreProfiles::parse(&weakened, PROFILE_FILE).is_err(),
            "a file weakened the skin attenuation and was accepted"
        );
        let strengthened = text.replace("skin_attenuation = 0.80", "skin_attenuation = 0.95");
        assert!(RestoreProfiles::parse(&strengthened, PROFILE_FILE).is_ok());
    }

    #[test]
    fn a_row_without_a_written_reason_is_refused() {
        let text = include_str!("../config/restore_profiles.toml");
        let stripped = text.replace(
            "[scene.dance_floor]\nmax_tier      = \"strong\"\nsharpen       = false\nface_recovery = false\nreason = \"\"\"",
            "[scene.dance_floor]\nmax_tier      = \"strong\"\nsharpen       = false\nface_recovery = false\nreason = \"\"\"\"\"\"\nunused = \"\"\"",
        );
        // The replacement above is only meaningful if it changed something; if the file is
        // reformatted the test should fail loudly rather than pass vacuously.
        assert_ne!(stripped, text, "the fixture edit matched nothing");
        assert!(RestoreProfiles::parse(&stripped, PROFILE_FILE).is_err());
    }

    #[test]
    fn every_shipped_noise_model_loads_and_none_of_them_is_measured() {
        let table = NoiseTable::embedded().expect("the embedded noise models load");
        assert_eq!(table.len(), 20, "section 8 step 1 asks for twenty bodies");
        for model in table.bodies() {
            assert!(
                !model.measured,
                "{} claims to be measured, and there are no camera files in this repository",
                model.camera
            );
            // Which means none of them may reach the strongest tier. ADR-0047 section 3.
            assert_eq!(model.tier_ceiling(), DenoiseTier::Standard);
            assert!(model.problem().is_none(), "{:?}", model.problem());
        }
    }

    #[test]
    fn a_body_with_no_row_gets_the_reference_model_rather_than_nothing() {
        let table = NoiseTable::embedded().expect("the embedded noise models load");
        assert!(table.has_row("SONY", "ILCE-7M3"));
        assert!(!table.has_row("Hasselblad", "X2D"));
        let fallback = table.model_for("Hasselblad", "X2D");
        assert_eq!(fallback.camera, "reference");
        assert!(!fallback.measured);
    }

    #[test]
    fn the_key_survives_the_ways_manufacturers_spell_a_body() {
        assert_eq!(
            normalise("SONY", "ILCE-7M3"),
            normalise("sony ", " ilce-7m3")
        );
        assert_eq!(
            normalise("NIKON CORPORATION", "NIKON Z 8"),
            normalise("nikon corporation", "  NIKON   Z 8 ")
        );
        // The make repeated inside the model is the common case and is stripped.
        assert_eq!(
            normalise("FUJIFILM", "FUJIFILM X-T4"),
            normalise("FUJIFILM", "X-T4")
        );
    }

    #[test]
    fn a_larger_well_predicts_less_noise_at_the_same_iso() {
        // The property that makes conditioning worth doing at all: two bodies with almost the
        // same read noise behave completely differently at ISO 12800 because their wells differ
        // by a factor of four. A denoiser with one strength for both is wrong for both.
        let table = NoiseTable::embedded().expect("the embedded noise models load");
        let big_wells = table.model_for("SONY", "ILCE-7SM3");
        let small_wells = table.model_for("SONY", "ILCE-7RM4");
        let big = big_wells.sigma_at(0.2, 12_800);
        let small = small_wells.sigma_at(0.2, 12_800);
        assert!(
            small > big * 1.5,
            "the 61 MP body predicted {small} against the 12 MP body's {big}"
        );
    }
}
