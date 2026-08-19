//! How much local shaping each kind of photograph gets.
//!
//! PHASE-19 section 9 gives PM one task: "approve `local_light.toml` per-scene policy and
//! default strengths". This is the loader, and the file it loads is the seventh in the
//! product to require a written reason per row.
//!
//! ## Why the numbers in that file are smaller than they look like they should be
//!
//! Every other policy table in AURA decides something a photographer can see a number for.
//! This one decides how much of a photograph the product may change *without anybody being
//! able to tell*. Section 0's risk line is "subtlety is the whole point", and a phase like
//! this does not fail because one row is wrong - it fails because every row is a little too
//! high and the gallery quietly starts to look processed.
//!
//! So the editing rule here is the opposite of the usual one: a row that is too gentle costs
//! a photographer thirty seconds with a slider, and a row that is too strong costs them their
//! trust in every frame they did not check.
//!
//! ## Refusal is whole-file
//!
//! A malformed table raises `AURA-ML-5069` and **nothing is loaded**, exactly as phase 15's
//! exposure targets and phase 11's composition rules behave. Half a policy table would shape
//! the ceremony against measured strengths and the reception against nothing, and that
//! inconsistency is invisible in a delivered gallery.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::error::AuraError;
use aura_core::contract::local::LocalOp;
use aura_core::SceneId;
use serde::Deserialize;

use crate::errors;

/// The shipped table, embedded so a binary can never disagree with its own defaults.
const EMBEDDED: &str = include_str!("../../config/local_light.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "local_light.toml";

/// The fewest characters a rationale may have.
///
/// Nine, the same as `prominence`'s, `scene_profiles`', `moment_profiles`',
/// `emotion_weights`', `camera_calibration`'s, `composition_rules`' and
/// `exposure_targets`'. Seven files, one rule, and a test asserts the constants agree.
pub const MIN_RATIONALE: usize = 9;

/// How much of the confidence a missing policy row costs.
///
/// The same flat 0.08 phases 11 and 15 charge for a missing rule or target row, and for the
/// same reason: an unpolicied scene is shaped against a *documented neutral* row rather than
/// against a guess, which is a weaker claim and not a worse one.
pub const UNPOLICIED_CONFIDENCE_PENALTY: f32 = 0.08;

/// One scene's strengths, caps and budget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenePolicy {
    /// The six per-operation strengths, in [`LocalOp::PRIORITY`] order.
    pub strengths: [f32; LocalOp::COUNT],
    /// The fraction of [`aura_core::contract::local::PERCEPTUAL_BUDGET`] this scene may
    /// spend across all operations.
    pub budget: f32,
    /// A ceiling on the face lift for this scene, in stops, before the dynamic noise cap.
    pub max_face_lift_ev: f32,
    /// True when this row was written for this scene rather than inherited from the neutral
    /// row.
    ///
    /// Phase 11's `measured` and phase 15's under a new name, for the same reason: an
    /// inherited row is a weaker claim and the plan has to be able to say so.
    pub measured: bool,
}

impl ScenePolicy {
    /// The strength this scene allows one operation.
    #[must_use]
    pub fn strength(&self, op: LocalOp) -> f32 {
        self.strengths.get(op.rank()).copied().unwrap_or(0.0)
    }

    /// True when this scene switches an operation off entirely.
    ///
    /// A zero is a product decision rather than a small number: `details` has no faces to
    /// light and `dance_floor` has no form worth shaping, and both are expressed as an
    /// operation that does not run rather than as one that runs and does nothing.
    #[must_use]
    pub fn declines(&self, op: LocalOp) -> bool {
        self.strength(op) <= 0.0
    }
}

/// Every scene's policy, plus the neutral row.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyTable {
    version: u16,
    neutral: ScenePolicy,
    rows: BTreeMap<String, ScenePolicy>,
}

#[derive(Debug, Deserialize)]
struct PolicyFile {
    version: u16,
    neutral: Row,
    #[serde(default)]
    scene: Vec<SceneEntry>,
}

#[derive(Debug, Deserialize)]
struct SceneEntry {
    id: String,
    #[serde(flatten)]
    body: Row,
}

#[derive(Debug, Deserialize)]
struct Row {
    face_light: f32,
    subject_enhance: f32,
    background_balance: f32,
    shine_control: f32,
    dodge_burn_low: f32,
    dodge_burn_mid: f32,
    budget: f32,
    max_face_lift_ev: f32,
    rationale: String,
}

impl PolicyTable {
    /// The shipped table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5069` when the embedded file is malformed, which would be a build bug rather
    /// than a deployment one - and is why `tests/local_policy.rs` loads it too.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse("local_light.toml (embedded)", EMBEDDED)
    }

    /// Load an installation override, or fall back to the shipped table.
    ///
    /// A malformed override is not fatal; a malformed baseline is. The same asymmetry every
    /// registry in the product has.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5069` only when the embedded baseline itself will not load.
    pub fn load_or_embedded(directory: &Path) -> Result<(Self, Option<AuraError>), AuraError> {
        let path = directory.join(OVERRIDE_FILE);
        if !path.exists() {
            return Ok((Self::embedded()?, None));
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            let refusal = errors::policy_refused(
                &path.display().to_string(),
                "file",
                "could not be read; the shipped local light policy is in use",
            );
            return Ok((Self::embedded()?, Some(refusal)));
        };
        match Self::parse(&path.display().to_string(), &text) {
            Ok(loaded) => Ok((loaded, None)),
            Err(refusal) => Ok((Self::embedded()?, Some(refusal))),
        }
    }

    /// Parse and validate one document.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5069`, naming the file, the key and the rule.
    pub fn parse(name: &str, text: &str) -> Result<Self, AuraError> {
        let parsed: PolicyFile = toml::from_str(text).map_err(|err| {
            errors::policy_refused(name, "file", &format!("is not valid TOML: {err}"))
        })?;

        if parsed.version == 0 {
            return Err(errors::policy_refused(
                name,
                "version",
                "must be at least 1; it is written into every plan and 0 means unversioned",
            ));
        }

        let mut neutral = validate(name, "neutral", &parsed.neutral)?;
        neutral.measured = false;

        let mut rows = BTreeMap::new();
        for entry in &parsed.scene {
            let label = format!("scene.{}", entry.id);
            if SceneId::from_str_or_unknown(&entry.id) == SceneId::Unknown && entry.id != "unknown"
            {
                return Err(errors::policy_refused(
                    name,
                    &label,
                    "names a scene that is not in the frozen taxonomy; adding a scene is a \
                     phase 07 contract change and needs an ADR",
                ));
            }
            let mut row = validate(name, &label, &entry.body)?;
            row.measured = true;
            if rows.insert(entry.id.clone(), row).is_some() {
                return Err(errors::policy_refused(
                    name,
                    &label,
                    "appears twice; two rows for one scene would make the answer depend on file \
                     order",
                ));
            }
        }

        Ok(Self {
            version: parsed.version,
            neutral,
            rows,
        })
    }

    /// Which version of the table this is. Written into every plan.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many scenes are named.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    /// The row for one scene, or the neutral row.
    ///
    /// [`SceneId::Unknown`] always takes the neutral row with `measured = false`, so an
    /// unclassified frame and a frame whose scene has no row are reported the same way. They
    /// are the same thing from the panel's point of view: a photograph shaped against
    /// strengths nobody wrote for it.
    #[must_use]
    pub fn get(&self, scene: SceneId) -> ScenePolicy {
        if scene == SceneId::Unknown {
            return self.neutral;
        }
        self.rows
            .get(scene.as_str())
            .copied()
            .unwrap_or(self.neutral)
    }

    /// The scenes this table has a row for.
    #[must_use]
    pub fn scenes(&self) -> Vec<String> {
        self.rows.keys().cloned().collect()
    }

    /// The scenes in the frozen taxonomy that this table has no row for.
    ///
    /// What `LocalOutline::unpolicied_scenes` reports, so a support engineer finds out in one
    /// query rather than by comparing two lists by eye.
    #[must_use]
    pub fn unpolicied(&self) -> Vec<String> {
        SceneId::ALL
            .into_iter()
            .filter(|scene| *scene != SceneId::Unknown)
            .filter(|scene| !self.rows.contains_key(scene.as_str()))
            .map(|scene| scene.as_str().to_string())
            .collect()
    }
}

/// Check one row against every documented range.
fn validate(file: &str, key: &str, row: &Row) -> Result<ScenePolicy, AuraError> {
    let strength = |name: &str, value: f32| -> Result<f32, AuraError> {
        if !(0.0..=1.0).contains(&value) {
            return Err(errors::policy_refused(
                file,
                &format!("{key}.{name}"),
                "must be between 0.0 and 1.0",
            ));
        }
        Ok(value)
    };

    let strengths = [
        strength("face_light", row.face_light)?,
        strength("subject_enhance", row.subject_enhance)?,
        strength("background_balance", row.background_balance)?,
        strength("shine_control", row.shine_control)?,
        strength("dodge_burn_low", row.dodge_burn_low)?,
        strength("dodge_burn_mid", row.dodge_burn_mid)?,
    ];

    if !(0.10..=1.0).contains(&row.budget) {
        return Err(errors::policy_refused(
            file,
            &format!("{key}.budget"),
            "must be between 0.10 and 1.0; a scene with no budget at all would run every \
             operation and then discard all of it, which is slower than switching them off",
        ));
    }
    if !(0.0..=aura_core::contract::local::MAX_FACE_LIFT_EV).contains(&row.max_face_lift_ev) {
        return Err(errors::policy_refused(
            file,
            &format!("{key}.max_face_lift_ev"),
            "must be between 0.0 and MAX_FACE_LIFT_EV (1.20 stops)",
        ));
    }

    // The rule that makes this a product decision rather than a tuning file. Seventh table,
    // same floor, same argument: a strength nobody can explain is a change nobody approved.
    if row.rationale.trim().len() < MIN_RATIONALE {
        return Err(errors::policy_refused(
            file,
            &format!("{key}.rationale"),
            "must say why a photographer would agree; a policy row with no written reason is a \
             product decision nobody made",
        ));
    }

    // A scene that shapes form harder than it lights faces has its priorities backwards, and
    // the priority order is not something a config file may contradict. Section 6.4 makes
    // face lighting the first claim on the budget; a row where the decorative operation
    // outranks it would spend the allowance on the part a photographer would not miss.
    let at = |op: LocalOp| strengths.get(op.rank()).copied().unwrap_or(0.0);
    let face = at(LocalOp::FaceLight);
    let shaping = at(LocalOp::DodgeBurnLow);
    if shaping > face {
        return Err(errors::policy_refused(
            file,
            &format!("{key}.dodge_burn_low"),
            "may not exceed `face_light`; section 6.4 gives face lighting the first claim on \
             the budget and a row cannot reverse the priority order",
        ));
    }

    Ok(ScenePolicy {
        strengths,
        budget: row.budget,
        max_face_lift_ev: row.max_face_lift_ev,
        measured: true,
    })
}
