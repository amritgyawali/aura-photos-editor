//! How much care a photograph of this kind, and a person of this role, is given.
//!
//! PHASE-20 section 4 asks for `config/retouch_presets.toml` and section 9 gives PM two tasks:
//! own the preset definitions and approve Natural as the default. This module loads that file,
//! refuses it when it is wrong, and is the only place either question is answered.
//!
//! ## The refusal is whole-file
//!
//! As phases 15 to 19 all do, and the reason is sharper here: half a preset table would retouch
//! the ceremony against measured strengths and the reception against nothing, and that
//! inconsistency is invisible in a delivered gallery. `AURA-ML-5093` is run-blocking.
//!
//! ## The floors are bounded by the code, not by the file
//!
//! [`PresetTable::parse`] refuses a texture floor below
//! [`aura_core::contract::retouch::RetouchPreset::floor`] for its own preset. Section 6.3 says
//! "never below 0.80 even in Polished", `docs/retouch.md` repeats it to a photographer, and a
//! claim a text file can retract is not a claim.

use std::collections::BTreeMap;

use aura_core::contract::error::AuraError;
use aura_core::contract::people::Role;
use aura_core::contract::retouch::RetouchPreset;
use aura_core::SceneId;
use serde::Deserialize;

use crate::errors;

/// The file this table is loaded from, for the error messages.
pub const FILE: &str = "crates/aura-retouch/config/retouch_presets.toml";

/// The table compiled into the binary.
const EMBEDDED: &str = include_str!("../config/retouch_presets.toml");

/// One preset: how hard it works and how much texture it insists on keeping.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetRow {
    /// The base strength every identity strength is scaled by, `0..1`.
    pub base_strength: f32,
    /// The high-band energy ratio this preset holds a retouch to.
    pub texture_floor: f32,
    /// How much of the strength blemish removal gets, `0..1`.
    pub blemish: f32,
    /// How much of it under-eye correction gets, `0..1`.
    pub undereye: f32,
    /// How much of it tone evening gets, `0..1`.
    pub evening: f32,
    /// Why this row reads the way it does. Never empty; the loader refuses one that is.
    pub reason: String,
}

/// One scene: which operations may run on this kind of photograph, and how far.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRow {
    /// The ceiling on what any operation may do here, `0..1`. Zero means no retouch at all.
    pub limit: f32,
    /// Whether under-eye correction runs on this kind of photograph.
    pub allow_undereye: bool,
    /// Whether tone evening runs on it.
    pub allow_evening: bool,
    /// Why. Never empty.
    pub reason: String,
}

/// The whole table.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetTable {
    version: u16,
    presets: BTreeMap<String, PresetRow>,
    scenes: BTreeMap<String, SceneRow>,
    roles: BTreeMap<String, f32>,
    neutral: SceneRow,
    unpreset: Vec<String>,
}

impl PresetTable {
    /// The table compiled into this build.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` when the embedded table will not load, which is a build fault rather than
    /// an installation one and is therefore never expected to happen in the field.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse(EMBEDDED, FILE)
    }

    /// Parse and validate a table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` naming the key and the rule it broke.
    #[allow(clippy::too_many_lines)]
    pub fn parse(text: &str, file: &str) -> Result<Self, AuraError> {
        let raw: RawTable = toml::from_str(text)
            .map_err(|error| errors::presets_refused(file, "table", &error.to_string()))?;

        if raw.version == 0 {
            return Err(errors::presets_refused(
                file,
                "version",
                "must be at least 1: it is written into every stored plan",
            ));
        }

        let mut presets = BTreeMap::new();
        for preset in RetouchPreset::ALL {
            let key = preset.as_str();
            let row = raw.preset.get(key).ok_or_else(|| {
                errors::presets_refused(file, key, "is missing, and every preset must have a row")
            })?;
            check_unit(file, key, "base_strength", row.base_strength)?;
            check_unit(file, key, "blemish", row.blemish)?;
            check_unit(file, key, "undereye", row.undereye)?;
            check_unit(file, key, "evening", row.evening)?;
            if row.reason.trim().is_empty() {
                return Err(errors::presets_refused(
                    file,
                    key,
                    "has no written reason, and every threshold here is a product decision",
                ));
            }
            // The bound the code owns. See the module header.
            if row.texture_floor + 1e-6 < preset.floor() {
                return Err(errors::presets_refused(
                    file,
                    key,
                    &format!(
                        "sets a texture floor of {:.3}, below the {:.2} this preset may never go \
                         under",
                        row.texture_floor,
                        preset.floor()
                    ),
                ));
            }
            if row.texture_floor > 1.0 {
                return Err(errors::presets_refused(
                    file,
                    key,
                    "sets a texture floor above 1.0, which no retouch could ever meet",
                ));
            }
            presets.insert(
                key.to_string(),
                PresetRow {
                    base_strength: row.base_strength,
                    texture_floor: row.texture_floor,
                    blemish: row.blemish,
                    undereye: row.undereye,
                    evening: row.evening,
                    reason: row.reason.clone(),
                },
            );
        }

        let mut roles = BTreeMap::new();
        for (key, value) in &raw.role {
            if key == "reason" {
                continue;
            }
            let Some(weight) = value.as_float().map(|v| v as f32) else {
                continue;
            };
            check_unit(file, "role", key, weight)?;
            if Role::from_str_or_unknown(key) == Role::Unknown && key != "unknown" {
                return Err(errors::presets_refused(
                    file,
                    key,
                    "is not a role this build knows",
                ));
            }
            roles.insert(key.clone(), weight);
        }
        if roles.is_empty() {
            return Err(errors::presets_refused(
                file,
                "role",
                "carries no weights, so every person in every wedding would be retouched the same",
            ));
        }

        let mut scenes = BTreeMap::new();
        let mut neutral = None;
        for (key, row) in &raw.scene {
            check_unit(file, key, "limit", row.limit)?;
            if row.reason.trim().is_empty() {
                return Err(errors::presets_refused(
                    file,
                    key,
                    "has no written reason, and every threshold here is a product decision",
                ));
            }
            let parsed = SceneRow {
                limit: row.limit,
                allow_undereye: row.allow_undereye,
                allow_evening: row.allow_evening,
                reason: row.reason.clone(),
            };
            if key == "neutral" {
                neutral = Some(parsed);
                continue;
            }
            // A scene name this build does not know is a typo, and a typo in this file is a kind
            // of photograph that silently falls back to the neutral row for ever.
            if SceneId::from_str_or_unknown(key) == SceneId::Unknown && key != "unknown" {
                return Err(errors::presets_refused(
                    file,
                    key,
                    "is not a scene in the phase 07 vocabulary",
                ));
            }
            scenes.insert(key.clone(), parsed);
        }

        let neutral = neutral.ok_or_else(|| {
            errors::presets_refused(
                file,
                "scene.neutral",
                "is missing, and it is what an unknown kind of photograph falls back to",
            )
        })?;

        let unpreset = SceneId::ALL
            .iter()
            .filter(|scene| !scenes.contains_key(scene.as_str()))
            .map(|scene| scene.as_str().to_string())
            .collect();

        Ok(Self {
            version: raw.version,
            presets,
            scenes,
            roles,
            neutral,
            unpreset,
        })
    }

    /// Which table this is. Written into every stored plan.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// One preset row.
    ///
    /// Falls back to Natural rather than failing, because [`PresetTable::parse`] has already
    /// refused a table missing a preset - so this can only be reached by a caller holding a
    /// preset from a newer build.
    #[must_use]
    pub fn preset(&self, preset: RetouchPreset) -> &PresetRow {
        self.presets
            .get(preset.as_str())
            .or_else(|| self.presets.get(RetouchPreset::Natural.as_str()))
            .unwrap_or_else(|| Self::any_preset())
    }

    fn any_preset() -> &'static PresetRow {
        // Unreachable in practice: `parse` refuses an empty table. Written without a panic
        // because hard rule 8 has no exception for unreachable code.
        static FALLBACK: std::sync::OnceLock<PresetRow> = std::sync::OnceLock::new();
        FALLBACK.get_or_init(|| PresetRow {
            base_strength: 0.0,
            texture_floor: 1.0,
            blemish: 0.0,
            undereye: 0.0,
            evening: 0.0,
            reason: "an empty table retouches nothing".to_string(),
        })
    }

    /// One scene row, and whether the scene had one of its own.
    ///
    /// `false` means the neutral row was used, which the caller turns into `AURA-ML-5094` and
    /// [`aura_core::contract::retouch::RetouchCode::SceneLimited`].
    #[must_use]
    pub fn scene(&self, scene: SceneId) -> (&SceneRow, bool) {
        match self.scenes.get(scene.as_str()) {
            Some(row) => (row, true),
            None => (&self.neutral, false),
        }
    }

    /// The weight for one role, `0..1`.
    ///
    /// An unknown role gets the `unknown` weight rather than one, which is the conservative
    /// direction: on a build with no face recognition every person lands there.
    #[must_use]
    pub fn role(&self, role: Role) -> f32 {
        self.roles
            .get(role.as_str())
            .or_else(|| self.roles.get("unknown"))
            .copied()
            .unwrap_or(0.3)
    }

    /// Scenes with no row of their own, by slug.
    #[must_use]
    pub fn unpreset(&self) -> Vec<String> {
        self.unpreset.clone()
    }
}

/// A value that must be a fraction.
fn check_unit(file: &str, key: &str, field: &str, value: f32) -> Result<(), AuraError> {
    if !(0.0..=1.0).contains(&value) {
        return Err(errors::presets_refused(
            file,
            key,
            &format!("`{field}` is {value:.3}, which is outside 0..1"),
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RawTable {
    version: u16,
    preset: BTreeMap<String, RawPreset>,
    role: BTreeMap<String, toml::Value>,
    scene: BTreeMap<String, RawScene>,
}

#[derive(Debug, Deserialize)]
struct RawPreset {
    base_strength: f32,
    texture_floor: f32,
    blemish: f32,
    undereye: f32,
    evening: f32,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawScene {
    limit: f32,
    allow_undereye: bool,
    allow_evening: bool,
    reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_table_loads_and_covers_every_scene() {
        let table = PresetTable::embedded().expect("the embedded table");
        assert_eq!(table.version(), 1);
        assert!(
            table.unpreset().is_empty(),
            "scenes with no row: {:?}",
            table.unpreset()
        );
    }

    #[test]
    fn every_preset_row_keeps_its_own_floor() {
        let table = PresetTable::embedded().expect("the embedded table");
        for preset in RetouchPreset::ALL {
            let row = table.preset(preset);
            assert!(
                row.texture_floor >= preset.floor(),
                "{} sets {} below its floor {}",
                preset.as_str(),
                row.texture_floor,
                preset.floor()
            );
        }
    }

    #[test]
    fn the_default_is_natural_and_it_holds_the_phase_floor() {
        let table = PresetTable::embedded().expect("the embedded table");
        assert_eq!(RetouchPreset::default(), RetouchPreset::Natural);
        let natural = table.preset(RetouchPreset::Natural);
        assert!((natural.texture_floor - 0.90).abs() < 1e-6);
    }

    #[test]
    fn a_floor_below_the_bound_is_refused() {
        let text = EMBEDDED.replace("texture_floor = 0.84", "texture_floor = 0.50");
        let error = PresetTable::parse(&text, FILE).expect_err("refused");
        assert_eq!(error.code.0, "AURA-ML-5093");
        assert!(error.detail.contains("0.80"));
    }

    #[test]
    fn a_row_with_no_reason_is_refused() {
        let text = EMBEDDED.replace(
            r#"reason = "Rooms and buildings. No skin, no retouch.""#,
            r#"reason = """#,
        );
        let error = PresetTable::parse(&text, FILE).expect_err("refused");
        assert_eq!(error.code.0, "AURA-ML-5093");
    }

    #[test]
    fn an_unknown_scene_name_is_refused_rather_than_ignored() {
        let text = EMBEDDED.replace("[scene.cake]", "[scene.cake_cutting]");
        let error = PresetTable::parse(&text, FILE).expect_err("refused");
        assert_eq!(error.code.0, "AURA-ML-5093");
        assert!(error.detail.contains("cake_cutting"));
    }

    #[test]
    fn the_scenes_with_no_skin_in_them_retouch_nothing() {
        let table = PresetTable::embedded().expect("the embedded table");
        for scene in [SceneId::Details, SceneId::Venue] {
            let (row, found) = table.scene(scene);
            assert!(found, "{} has no row", scene.as_str());
            assert!(row.limit <= 0.0, "{} retouches something", scene.as_str());
        }
    }

    #[test]
    fn a_ritual_frame_is_never_evened() {
        // Turmeric, sindoor, mehndi and paint are the reason the photograph exists, and
        // mid-frequency evening is exactly the operation that would calm them into a patch.
        let table = PresetTable::embedded().expect("the embedded table");
        let (row, found) = table.scene(SceneId::Ritual);
        assert!(found);
        assert!(!row.allow_evening);
    }

    #[test]
    fn a_guest_is_retouched_less_than_the_couple() {
        let table = PresetTable::embedded().expect("the embedded table");
        assert!(table.role(Role::Guest) < table.role(Role::Couple));
        assert!(table.role(Role::Vendor) < table.role(Role::Guest));
        assert!(table.role(Role::Unknown) <= table.role(Role::Guest));
    }
}
