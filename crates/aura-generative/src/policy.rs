//! `cleanup_policy.toml`: the caps, the denylist and the autonomy rules, per scene.
//!
//! Co-owned by PM and SEC - section 9 gives them the file jointly, which is the only file in the
//! product with two owners, because the numbers in it are a product decision and a safety
//! decision at the same time.
//!
//! ## The file may only make the product stricter
//!
//! The contract owns `AREA_CAP_DEFAULT`, `DENYLIST_OVERLAP_MAX` and `ZERO_TOUCH_CONFIDENCE`. This
//! loader refuses a row whose `area_cap` is above the first, whose `denylist_overlap_max` is above
//! the second, or whose `zero_touch_confidence` is below the third. A config file that could relax
//! a safety guarantee is a safety guarantee that lives in a text file somebody can edit, and
//! `docs/generative-policy.md` would then be a description of some defaults rather than a promise
//! about the product.
//!
//! This is phase 21's rule - a ceiling can be lowered by a studio and raised by nobody - and phase
//! 23's, restated for the phase where it matters most.
//!
//! ## Every row needs a written reason
//!
//! The sixth config file in the product to enforce it, after `emotion_weights.toml`,
//! `local_light.toml`, `scene_profiles.toml`, `crop_rules.toml` and `retouch_presets.toml`. A
//! threshold nobody can explain is a product decision nobody made, and here it is also a safety
//! decision nobody made.

use std::collections::BTreeMap;

use aura_core::contract::cleanup::{
    DistractionClass, AREA_CAP_DEFAULT, DENYLIST_OVERLAP_MAX, ZERO_TOUCH_CONFIDENCE,
};
use aura_core::contract::scene::SceneId;
use aura_core::AuraResult;
use serde::Deserialize;

use crate::errors;

/// What one scene permits.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenePolicy {
    /// The largest share of the frame an automated removal may cover here.
    pub area_cap: f32,
    /// The most a candidate may overlap a denylisted region, as a share of its own area.
    pub denylist_overlap_max: f32,
    /// The calibrated confidence a tier-1 removal needs before Zero-Touch may apply it.
    pub zero_touch_confidence: f32,
    /// Whether automated proposals happen at all in this scene.
    ///
    /// False in the scenes where nearly everything in frame is part of the story. The ceremony is
    /// the obvious one: a garland, a fire, a plate of offerings and a stack of chairs are all
    /// clutter to a detector and three of the four are the wedding.
    pub enabled: bool,
    /// Why this row says what it says. Refused when empty.
    pub reason: String,
}

impl ScenePolicy {
    /// The strictest row that is still a row: nothing is removed.
    #[must_use]
    pub fn off(reason: impl Into<String>) -> Self {
        Self {
            area_cap: 0.0,
            denylist_overlap_max: 0.0,
            zero_touch_confidence: 1.0,
            enabled: false,
            reason: reason.into(),
        }
    }
}

/// Every scene's row, plus the classes this build will never propose automatically.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    rows: BTreeMap<&'static str, ScenePolicy>,
    /// Which version of the file this is. Stored on every proposal so a change re-examines.
    pub version: u16,
}

#[derive(Debug, Deserialize)]
struct FileRoot {
    #[serde(default)]
    version: u16,
    #[serde(default)]
    scene: BTreeMap<String, FileRow>,
}

#[derive(Debug, Deserialize)]
struct FileRow {
    area_cap: f32,
    denylist_overlap_max: f32,
    zero_touch_confidence: f32,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    reason: String,
}

const fn yes() -> bool {
    true
}

impl Policy {
    /// Parse and validate a policy file.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5119` when the file does not parse, names an unknown scene, carries a row with no
    /// reason, or tries to widen any of the three bounds the contract owns.
    pub fn load_str(text: &str) -> AuraResult<Self> {
        let root: FileRoot = toml::from_str(text).map_err(|e| {
            errors::policy_refused(format!("cleanup_policy.toml did not parse: {e}"))
        })?;

        let mut rows = BTreeMap::new();
        for (name, row) in root.scene {
            // `from_str_or_unknown` maps anything it does not recognise onto `Unknown`, so a
            // typo would silently become the row that switches cleanup off - which is safe and is
            // also a file the loader claimed to have validated. Compare the slug back.
            let scene = SceneId::from_str_or_unknown(&name);
            if scene.as_str() != name {
                return Err(errors::policy_refused(format!(
                    "cleanup_policy.toml names unknown scene {name}"
                )));
            }
            if row.reason.trim().is_empty() {
                return Err(errors::policy_refused(format!(
                    "the row for {name} has no reason; a threshold nobody can explain is a \
                     product decision nobody made"
                )));
            }
            if row.area_cap > AREA_CAP_DEFAULT {
                return Err(errors::policy_refused(format!(
                    "the row for {name} raises area_cap to {:.3}; the contract owns {AREA_CAP_DEFAULT:.3} \
                     and this file may only lower it",
                    row.area_cap
                )));
            }
            if row.denylist_overlap_max > DENYLIST_OVERLAP_MAX {
                return Err(errors::policy_refused(format!(
                    "the row for {name} raises denylist_overlap_max to {:.4}; the contract owns \
                     {DENYLIST_OVERLAP_MAX:.4} and this file may only lower it",
                    row.denylist_overlap_max
                )));
            }
            if row.zero_touch_confidence < ZERO_TOUCH_CONFIDENCE {
                return Err(errors::policy_refused(format!(
                    "the row for {name} lowers zero_touch_confidence to {:.3}; the contract owns \
                     {ZERO_TOUCH_CONFIDENCE:.3} and this file may only raise it",
                    row.zero_touch_confidence
                )));
            }
            if !(0.0..=1.0).contains(&row.area_cap)
                || !(0.0..=1.0).contains(&row.denylist_overlap_max)
                || !(0.0..=1.0).contains(&row.zero_touch_confidence)
            {
                return Err(errors::policy_refused(format!(
                    "the row for {name} carries a value outside 0..1"
                )));
            }
            rows.insert(
                scene.as_str(),
                ScenePolicy {
                    area_cap: row.area_cap,
                    denylist_overlap_max: row.denylist_overlap_max,
                    zero_touch_confidence: row.zero_touch_confidence,
                    enabled: row.enabled,
                    reason: row.reason,
                },
            );
        }

        Ok(Self {
            rows,
            version: root.version,
        })
    }

    /// The shipped table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5119` when the embedded file is malformed, which is a build error rather than a
    /// deployment one and is why it is checked by the phase gate.
    pub fn shipped() -> AuraResult<Self> {
        Self::load_str(include_str!("../config/cleanup_policy.toml"))
    }

    /// The row for one scene.
    ///
    /// `None` rather than a neutral default, so a caller must decide what an unknown scene means
    /// and the decision is visible. Invariant 7: a default cap applied to a scene nobody wrote a
    /// row for is a global threshold wearing a scene's name.
    #[must_use]
    pub fn scene(&self, scene: SceneId) -> Option<&ScenePolicy> {
        self.rows.get(scene.as_str())
    }

    /// How many rows the table carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when the table carries no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// True when this class may ever be proposed automatically, in any scene.
    ///
    /// A method on the policy rather than a column in the file, because it is not a threshold a
    /// studio tunes: `BackgroundPerson` is a person and `Unclassified` cannot be shown to be
    /// irrelevant. There is no field in the file that could turn either on, which is what makes
    /// the policy document's claim about them a promise rather than a default.
    #[must_use]
    pub fn class_may_be_automatic(class: DistractionClass) -> bool {
        class.story_safe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
version = 1
[scene.getting_ready_bride]
area_cap = 0.03
denylist_overlap_max = 0.01
zero_touch_confidence = 0.97
reason = "dressing rooms are full of clutter that is nobody's memory"
"#;

    #[test]
    fn a_row_that_lowers_a_bound_is_accepted() {
        let policy = Policy::load_str(GOOD).expect("loads");
        let row = policy.scene(SceneId::GettingReadyBride).expect("row");
        assert!(row.area_cap < AREA_CAP_DEFAULT);
        assert!(row.enabled);
    }

    #[test]
    fn a_row_that_raises_the_area_cap_is_refused() {
        let text = GOOD.replace("area_cap = 0.03", "area_cap = 0.20");
        let err = Policy::load_str(&text).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5119");
        assert!(err.detail.contains("may only lower"));
    }

    #[test]
    fn a_row_that_lowers_the_zero_touch_confidence_is_refused() {
        let text = GOOD.replace(
            "zero_touch_confidence = 0.97",
            "zero_touch_confidence = 0.50",
        );
        let err = Policy::load_str(&text).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5119");
        assert!(err.detail.contains("may only raise"));
    }

    #[test]
    fn a_row_with_no_reason_is_refused() {
        let text = GOOD.replace(
            r#"reason = "dressing rooms are full of clutter that is nobody's memory""#,
            r#"reason = """#,
        );
        let err = Policy::load_str(&text).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5119");
        assert!(err.detail.contains("no reason"));
    }

    #[test]
    fn an_unknown_scene_is_refused() {
        let text = GOOD.replace("[scene.getting_ready_bride]", "[scene.afterparty_karaoke]");
        let err = Policy::load_str(&text).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5119");
    }

    #[test]
    fn the_shipped_table_loads_and_covers_every_scene() {
        let policy = Policy::shipped().expect("the shipped table must load");
        for scene in SceneId::ALL {
            assert!(
                policy.scene(scene).is_some(),
                "{} has no cleanup policy row",
                scene.as_str()
            );
        }
    }

    #[test]
    fn a_person_and_an_unknown_object_are_never_automatic_in_any_scene() {
        assert!(!Policy::class_may_be_automatic(
            DistractionClass::BackgroundPerson
        ));
        assert!(!Policy::class_may_be_automatic(
            DistractionClass::Unclassified
        ));
        assert!(Policy::class_may_be_automatic(DistractionClass::Bin));
    }
}
