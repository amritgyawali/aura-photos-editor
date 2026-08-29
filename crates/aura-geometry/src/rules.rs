//! The crop rules table. PHASE-23 sections 6.3 and 9.
//!
//! Invariant 7 - no threshold is global - applied to the one decision in this phase that
//! removes something from a photograph. A `ritual` frame and a `details` flat-lay want
//! opposite behaviour from the same solver, and the difference is data with a written reason
//! rather than a branch somebody added.
//!
//! ## The loader only tightens
//!
//! `resolution_floor` may be raised above the contract's [`RESOLUTION_FLOOR`] and never
//! lowered; `improvement_margin` may be raised above [`IMPROVEMENT_MARGIN`] and never
//! lowered; and there is no field in the schema that could permit cutting a face. **A config
//! file that could relax a safety guarantee is a safety guarantee that lives in a text file
//! somebody can edit**, and this is the third table in the product to be written that way -
//! `cull_weights.toml` refuses a row that weights framing above whether the photograph
//! worked, and `local_light.toml` refuses one that shapes form harder than it lights faces.
//!
//! ## A missing row is not a missing file
//!
//! A scene with no row falls back to [`SceneRule::conservative`] - deliver as shot - and
//! raises `AURA-ML-5094`, a warning. A file that will not load raises `AURA-ML-5093`, which
//! is run-blocking. The difference is the whole reason there are two codes: one missing scene
//! should not stop a wedding being finished, and cropping every wedding to a default nobody
//! approved is worse than cropping nothing.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::error::AuraResult;
use aura_core::contract::geometry::{CropPurpose, IMPROVEMENT_MARGIN, RESOLUTION_FLOOR};
use aura_core::SceneId;
use serde::Deserialize;

use crate::errors;

/// Which version of this table produced a plan.
///
/// Bumping it invalidates every stored crop, because every rectangle was checked against the
/// margins in this file. `AURA-ML-5090` is raised when a comparison would cross it.
pub const RULES_VER: u16 = 1;

/// Where the shipped table lives, relative to this crate.
pub const RULES_PATH: &str = "config/crop_rules.toml";

/// What one scene allows.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRule {
    /// Whether a tighter primary framing may be proposed at all.
    pub crop: bool,
    /// How much better a proposed primary crop must score than the frame as shot.
    pub improvement_margin: f32,
    /// The smallest fraction of the original long edge a crop may keep.
    pub resolution_floor: f32,
    /// The bottom of the headroom band, as a fraction of the crop's height.
    pub headroom_min: f32,
    /// The top of the headroom band.
    pub headroom_max: f32,
    /// Which aspect variants this scene generates.
    pub variants: Vec<CropPurpose>,
    /// Why. Required on every row.
    pub reason: String,
}

impl SceneRule {
    /// The fallback for a scene with no row: deliver as shot.
    ///
    /// Not an average of the other rows and not a permissive default. The absence of a
    /// decision is a decision to leave the photograph alone, which is what makes
    /// `AURA-ML-5094` a warning rather than a failure.
    #[must_use]
    pub fn conservative() -> Self {
        Self {
            crop: false,
            improvement_margin: IMPROVEMENT_MARGIN,
            resolution_floor: RESOLUTION_FLOOR,
            headroom_min: 0.04,
            headroom_max: 0.18,
            variants: Vec::new(),
            reason: "no row for this scene, so the frame is delivered as shot".to_string(),
        }
    }
}

/// The whole table.
#[derive(Debug, Clone)]
pub struct CropRules {
    rows: BTreeMap<SceneId, SceneRule>,
    defaults: SceneRule,
    version: u16,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    #[serde(default)]
    version: u16,
    defaults: RawRow,
    #[serde(default)]
    scene: Vec<RawScene>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRow {
    #[serde(default)]
    crop: bool,
    improvement_margin: Option<f32>,
    resolution_floor: Option<f32>,
    headroom_min: Option<f32>,
    headroom_max: Option<f32>,
    #[serde(default)]
    variants: Option<Vec<String>>,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawScene {
    id: String,
    #[serde(flatten)]
    row: RawRow,
}

impl CropRules {
    /// The table this build ships.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` when the shipped file does not load.
    pub fn shipped() -> AuraResult<Self> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(RULES_PATH);
        Self::load(&path)
    }

    /// Load a table from a path.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` when the file cannot be read, does not parse, names an unknown scene,
    /// carries a row with no reason, or tries to loosen a safety rule.
    pub fn load(path: &Path) -> AuraResult<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| errors::rules_refused(format!("{}: {err}", path.display())))?;
        Self::parse(&text)
    }

    /// Parse a table from text.
    ///
    /// # Errors
    ///
    /// As [`CropRules::load`].
    pub fn parse(text: &str) -> AuraResult<Self> {
        let raw: RawFile =
            toml::from_str(text).map_err(|err| errors::rules_refused(err.to_string()))?;
        let defaults = convert(&raw.defaults, "defaults")?;
        let mut rows = BTreeMap::new();
        for scene in &raw.scene {
            let id = SceneId::ALL
                .into_iter()
                .find(|candidate| candidate.as_str() == scene.id)
                .ok_or_else(|| errors::rules_refused(format!("unknown scene '{}'", scene.id)))?;
            let rule = convert(&scene.row, &scene.id)?;
            if rows.insert(id, rule).is_some() {
                return Err(errors::rules_refused(format!(
                    "scene '{}' appears twice",
                    scene.id
                )));
            }
        }
        Ok(Self {
            rows,
            defaults,
            version: if raw.version > 0 {
                raw.version
            } else {
                RULES_VER
            },
        })
    }

    /// The rule for one scene.
    ///
    /// The second element is `false` when the scene had no row and the conservative fallback
    /// was used, which is what the caller reports as `AURA-ML-5094` and stores in
    /// `geometry_plan.rules_row`.
    #[must_use]
    pub fn for_scene(&self, scene: SceneId) -> (SceneRule, bool) {
        match self.rows.get(&scene) {
            Some(rule) => (rule.clone(), true),
            None => (SceneRule::conservative(), false),
        }
    }

    /// The defaults row, for the panel.
    #[must_use]
    pub const fn defaults(&self) -> &SceneRule {
        &self.defaults
    }

    /// Which version this is.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Scenes with no row, in vocabulary order.
    #[must_use]
    pub fn unpolicied(&self) -> Vec<SceneId> {
        SceneId::ALL
            .into_iter()
            .filter(|scene| !self.rows.contains_key(scene))
            .collect()
    }

    /// How many scenes have a row.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when no scene has a row.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn convert(raw: &RawRow, id: &str) -> AuraResult<SceneRule> {
    if raw.reason.trim().is_empty() {
        return Err(errors::rules_refused(format!(
            "row '{id}' has no reason - section 9 gives PM this file to approve, and a \
             threshold nobody can explain is a product decision nobody made"
        )));
    }
    let margin = raw.improvement_margin.unwrap_or(IMPROVEMENT_MARGIN);
    if margin < IMPROVEMENT_MARGIN {
        return Err(errors::rules_refused(format!(
            "row '{id}' sets improvement_margin {margin:.3} below the contract's \
             {IMPROVEMENT_MARGIN:.3} - this file may only tighten a safety rule"
        )));
    }
    let floor = raw.resolution_floor.unwrap_or(RESOLUTION_FLOOR);
    if floor < RESOLUTION_FLOOR {
        return Err(errors::rules_refused(format!(
            "row '{id}' sets resolution_floor {floor:.3} below the contract's \
             {RESOLUTION_FLOOR:.3} - this file may only tighten a safety rule"
        )));
    }
    if floor > 1.0 || margin > 1.0 {
        return Err(errors::rules_refused(format!(
            "row '{id}' sets a fraction above one"
        )));
    }
    let headroom_min = raw.headroom_min.unwrap_or(0.04);
    let headroom_max = raw.headroom_max.unwrap_or(0.18);
    if headroom_min < 0.0 || headroom_max > 0.5 || headroom_min > headroom_max {
        return Err(errors::rules_refused(format!(
            "row '{id}' has a headroom band outside 0..0.5 or inverted"
        )));
    }
    let mut variants = Vec::new();
    for name in raw.variants.clone().unwrap_or_default() {
        let purpose = CropPurpose::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == name)
            .ok_or_else(|| {
                errors::rules_refused(format!("row '{id}': unknown variant '{name}'"))
            })?;
        if matches!(purpose, CropPurpose::Original | CropPurpose::Primary) {
            return Err(errors::rules_refused(format!(
                "row '{id}': '{name}' is not an aspect variant - the original and the primary \
                 are on every plan by construction"
            )));
        }
        if !variants.contains(&purpose) {
            variants.push(purpose);
        }
    }
    variants.sort_unstable();
    Ok(SceneRule {
        crop: raw.crop,
        improvement_margin: margin,
        resolution_floor: floor,
        headroom_min,
        headroom_max,
        variants,
        reason: raw.reason.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> CropRules {
        CropRules::shipped().expect("the shipped table loads")
    }

    #[test]
    fn every_scene_in_the_vocabulary_has_a_row_with_a_reason() {
        let rules = shipped();
        assert!(
            rules.unpolicied().is_empty(),
            "scenes with no row: {:?}",
            rules.unpolicied()
        );
        for scene in SceneId::ALL {
            let (rule, present) = rules.for_scene(scene);
            assert!(present, "{scene} has no row");
            assert!(!rule.reason.is_empty(), "{scene} has no reason");
        }
    }

    #[test]
    fn the_table_is_conservative_by_design() {
        // Section 10.1: most frames keep their original framing. The mechanism starts here -
        // a majority of scenes may not be cropped at all.
        let rules = shipped();
        let croppable = SceneId::ALL
            .into_iter()
            .filter(|scene| rules.for_scene(*scene).0.crop)
            .count();
        assert!(
            croppable * 2 <= SceneId::ALL.len(),
            "{croppable} of {} scenes allow a crop, which is not conservative",
            SceneId::ALL.len()
        );
    }

    #[test]
    fn the_scenes_that_must_never_crop_never_crop() {
        let rules = shipped();
        // Four rows a future edit must not quietly flip. `kiss` is the photograph the wedding
        // is bought for, `ritual` may contain something nobody here can identify, and the two
        // group rows have somebody at each end who is as important as the couple.
        for scene in [
            SceneId::Kiss,
            SceneId::Ritual,
            SceneId::FamilyPortrait,
            SceneId::GroupPortrait,
            SceneId::DanceFloor,
            SceneId::Unknown,
        ] {
            assert!(!rules.for_scene(scene).0.crop, "{scene} may not be cropped");
        }
    }

    #[test]
    fn a_row_may_not_loosen_a_safety_rule() {
        let base = "[defaults]\nreason = \"d\"\n";
        let loose_floor = format!(
            "{base}[[scene]]\nid = \"candid\"\ncrop = true\nresolution_floor = 0.40\n\
             reason = \"too loose\"\n"
        );
        let err = CropRules::parse(&loose_floor).expect_err("a loosened floor is refused");
        assert_eq!(err.code.0, "AURA-ML-5093");

        let loose_margin = format!(
            "{base}[[scene]]\nid = \"candid\"\ncrop = true\nimprovement_margin = 0.01\n\
             reason = \"too loose\"\n"
        );
        assert!(CropRules::parse(&loose_margin).is_err());
    }

    #[test]
    fn a_row_without_a_reason_is_refused() {
        let text = "[defaults]\nreason = \"d\"\n[[scene]]\nid = \"candid\"\ncrop = true\n";
        let err = CropRules::parse(text).expect_err("no reason is refused");
        assert_eq!(err.code.0, "AURA-ML-5093");
    }

    #[test]
    fn an_unknown_scene_and_a_duplicate_scene_are_both_refused() {
        let base = "[defaults]\nreason = \"d\"\n";
        assert!(CropRules::parse(&format!(
            "{base}[[scene]]\nid = \"reception_afterparty\"\nreason = \"r\"\n"
        ))
        .is_err());
        assert!(CropRules::parse(&format!(
            "{base}[[scene]]\nid = \"candid\"\nreason = \"r\"\n\
             [[scene]]\nid = \"candid\"\nreason = \"r\"\n"
        ))
        .is_err());
    }

    #[test]
    fn the_original_and_the_primary_are_not_variants_anybody_may_ask_for() {
        let text = "[defaults]\nreason = \"d\"\n[[scene]]\nid = \"candid\"\n\
                    variants = [\"primary\"]\nreason = \"r\"\n";
        assert!(CropRules::parse(text).is_err());
    }

    #[test]
    fn an_unrecognised_scene_falls_back_to_delivering_as_shot() {
        let rules = CropRules::parse("[defaults]\nreason = \"d\"\n").expect("an empty table");
        let (rule, present) = rules.for_scene(SceneId::Candid);
        assert!(!present);
        assert!(!rule.crop);
        assert!(rule.variants.is_empty());
        assert_eq!(rules.unpolicied().len(), SceneId::ALL.len());
    }
}
