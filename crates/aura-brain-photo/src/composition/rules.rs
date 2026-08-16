//! What good framing means in this kind of photograph.
//!
//! PHASE-11 section 8's first step: "author `composition_rules.toml` with the
//! photographer consultant". This module is the loader, and the file it loads is the
//! fourth in the product to require a written reason per row.
//!
//! ## Why this file is not `camera_calibration.toml`
//!
//! Its sibling in the same directory is a table of measurements. Nothing here is. There
//! is no chart that establishes how much headroom a Hindu ceremony wants, no experiment
//! that decides whether a cut wrist matters on a dance floor, and no way to be right
//! about either in general. What the file records is a **product decision per scene**,
//! and the loader's job is to make sure nobody can ship one without saying why.
//!
//! That difference changes what the failure modes are. A wrong calibration row makes one
//! camera look bad; a wrong rule row makes one *kind of photograph* look bad, and section
//! 12's first failure mode - "rigid rules punish creative work" - is what a product with
//! several of them looks like from outside.
//!
//! ## The three permissions, and why they are per scene
//!
//! [`SceneRule::allows_dutch`], [`SceneRule::head_crop_allowed`] and
//! [`SceneRule::centre_preferred`] are the three fields that turn a violation into an
//! exoneration. All three are section 6.1's, and all three are the difference between
//! this phase and a rule engine:
//!
//! * a tilted dance-floor frame is style, and a tilted altar frame is a mistake;
//! * a cropped crown in a couple portrait is a beauty crop, and the same crop in a
//!   family group is a reshoot;
//! * a centred flat-lay is correct, and a centred candid is usually an accident.
//!
//! A global rule cannot express any of those three, which is invariant 7 - no threshold
//! is global - arriving at composition.
//!
//! ## Refusal is whole-file
//!
//! A malformed table raises `AURA-ML-5046` and **nothing is loaded**. Phase 09's
//! calibration loader halts for the same reason and it is worth restating in this
//! phase's terms: a table that loaded the ceremony rows and dropped the reception rows
//! would judge half a wedding against measured bands and half against neutral ones, and
//! the review queue that came out would be sorted by time of day. That reads as "the
//! product hates the reception" and not at all like a configuration error.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::error::AuraError;
use aura_core::SceneId;
use serde::Deserialize;

use crate::errors;

/// The shipped table, embedded so a binary can never disagree with its own defaults.
const EMBEDDED: &str = include_str!("../../config/composition_rules.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "composition_rules.toml";

/// The fewest characters a rationale may have.
///
/// Nine, the same as `prominence`'s, `scene_profiles`' , `moment_profiles`',
/// `emotion_weights`' and `camera_calibration`'s. Five files, one rule, and a test
/// asserts the constants agree - a product decision nobody can explain is a magic number
/// even when a photographer made it.
pub const MIN_RATIONALE: usize = 9;

/// The most a scene row may let the learned head move the score.
///
/// Section 6.3: "aesthetic score is capped in influence ... because taste should break
/// ties, not override substance." This is the cap on what the *file* may ask for;
/// `super::score::AESTHETIC_CAP` is the cap on what the arithmetic will grant, and the
/// two are deliberately separate. A file that asked for 0.9 would be refused rather than
/// silently clamped, because a product manager who wrote 0.9 meant something and should
/// find out that they cannot have it.
pub const AESTHETIC_WEIGHT_CAP: f32 = 0.50;

/// How much of the confidence a missing rule row costs.
///
/// Flat rather than per-row, which is the difference from `AURA-ML-5037`'s
/// `Calibration::confidence_penalty`: an unmeasured camera body is judged against a
/// *guess*, and an unruled scene is judged against a documented neutral row. The second
/// is a weaker claim, not a worse one.
pub const UNRULED_CONFIDENCE_PENALTY: f32 = 0.08;

/// One scene's framing rules.
///
/// Every field is documented in `config/composition_rules.toml`, which is where a person
/// changing one reads about them. The doc comments here say what the *code* does with
/// each number, which is the other half.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SceneRule {
    /// Least headroom this scene wants, as a fraction of frame height.
    pub headroom_min: f32,
    /// Most headroom this scene wants.
    pub headroom_max: f32,
    /// What being outside the band costs the composite.
    pub headroom_penalty: f32,
    /// Above this many degrees the horizon is called tilted.
    pub tilt_tolerance_deg: f32,
    /// True when a large tilt here may read as a decision.
    ///
    /// The fourth of section 6.1's four conditions for an intentional tilt. A permission,
    /// not a verdict.
    pub allows_dutch: bool,
    /// True when cutting into the top of a head is normal here.
    pub head_crop_allowed: bool,
    /// Multiplies every limb cut's severity in this scene.
    pub joint_cut_weight: f32,
    /// Background edge density this scene carries without complaint.
    ///
    /// The denominator of `CompositionResult::clutter`, which is why that field is a
    /// relative number.
    pub clutter_tolerance: f32,
    /// True when the middle of the frame is the right place for the subject.
    ///
    /// Turns `OFF_THIRDS` into `CENTRED` and stops a rule of thirds being applied to a
    /// flat-lay.
    pub centre_preferred: bool,
    /// How much placement moves the score here.
    pub thirds_weight: f32,
    /// How much balance moves the score here.
    pub balance_weight: f32,
    /// How much the learned head moves the score here.
    pub aesthetic_weight: f32,
    /// False when this row is the neutral fallback rather than a scene's own.
    ///
    /// Read by the analyser to decide whether to emit `CompositionCode::NoRule` and to
    /// subtract [`UNRULED_CONFIDENCE_PENALTY`]. Not in the file; the loader sets it.
    pub measured: bool,
}

impl SceneRule {
    /// True when `headroom` sits inside the band.
    ///
    /// Inclusive at both ends, because a band whose edges are violations is a band that
    /// flags the frame a photographer framed exactly to the rule.
    #[must_use]
    pub fn headroom_ok(&self, headroom: f32) -> bool {
        headroom >= self.headroom_min && headroom <= self.headroom_max
    }

    /// The middle of the headroom band. What a crop hint aims for.
    #[must_use]
    pub fn headroom_target(&self) -> f32 {
        f32::midpoint(self.headroom_min, self.headroom_max)
    }
}

/// Every scene this build has rules for, and the row for the ones it does not.
#[derive(Debug, Clone)]
pub struct RuleTable {
    version: u16,
    neutral: SceneRule,
    rows: BTreeMap<String, SceneRule>,
}

impl RuleTable {
    /// Load the shipped table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5046` when the embedded file is malformed, which would be a build bug
    /// rather than a deployment one - and is why `tests/composition_rules.rs` loads it
    /// too.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse("composition_rules.toml (embedded)", EMBEDDED)
    }

    /// Load an installation override, or fall back to the shipped table.
    ///
    /// A malformed override is not fatal; a malformed baseline is. The same asymmetry the
    /// scene, moment, emotion and calibration registries have, for the same reason: the
    /// baseline is known good and falling back to it keeps the product open.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5046` only when the embedded baseline itself will not load.
    pub fn load_or_embedded(directory: &Path) -> Result<(Self, Option<AuraError>), AuraError> {
        let path = directory.join(OVERRIDE_FILE);
        if !path.exists() {
            return Ok((Self::embedded()?, None));
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            let refusal = errors::rules_refused(
                &path.display().to_string(),
                "file",
                "could not be read; the shipped framing rules are in use",
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
    /// `AURA-ML-5046`, naming the file, the key and the rule.
    pub fn parse(name: &str, text: &str) -> Result<Self, AuraError> {
        let parsed: RuleFile = toml::from_str(text).map_err(|err| {
            errors::rules_refused(name, "file", &format!("is not valid TOML: {err}"))
        })?;

        if parsed.version == 0 {
            return Err(errors::rules_refused(
                name,
                "version",
                "must be at least 1; it is written into every judgement and 0 means unversioned",
            ));
        }

        let mut neutral = validate(name, "neutral", &parsed.neutral)?;
        neutral.measured = false;

        let mut rows = BTreeMap::new();
        for entry in &parsed.scene {
            let label = format!("scene.{}", entry.id);
            if SceneId::from_str_or_unknown(&entry.id) == SceneId::Unknown && entry.id != "unknown"
            {
                return Err(errors::rules_refused(
                    name,
                    &label,
                    "names a scene that is not in the frozen taxonomy; adding a scene is a \
                     phase 07 contract change and needs an ADR",
                ));
            }
            let mut row = validate(name, &label, &entry.body)?;
            row.measured = true;
            if rows.insert(entry.id.clone(), row).is_some() {
                return Err(errors::rules_refused(
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

    /// Which version of the table this is. Written into every judgement.
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
    /// [`SceneId::Unknown`] always takes the neutral row, and it takes it with
    /// `measured = false`, so an unclassified frame and a frame whose scene has no row
    /// are reported the same way. They are the same thing from the panel's point of view:
    /// a judgement made against bands nobody wrote for this photograph.
    #[must_use]
    pub fn get(&self, scene: SceneId) -> SceneRule {
        if scene == SceneId::Unknown {
            return self.neutral;
        }
        self.rows
            .get(scene.as_str())
            .copied()
            .unwrap_or(self.neutral)
    }

    /// The neutral row as written.
    #[must_use]
    pub const fn neutral(&self) -> SceneRule {
        self.neutral
    }

    /// Every scene with a row of its own, as slugs.
    #[must_use]
    pub fn scenes(&self) -> Vec<String> {
        self.rows.keys().cloned().collect()
    }

    /// The scenes in the frozen taxonomy that have no row here.
    ///
    /// What `AURA-ML-5047` is raised for, and what `CompositionOutline::unruled_scenes`
    /// reports. `unknown` is not counted: it has no row by design.
    #[must_use]
    pub fn unruled(&self) -> Vec<String> {
        SceneId::ALL
            .into_iter()
            .filter(|scene| *scene != SceneId::Unknown)
            .map(|scene| scene.as_str().to_string())
            .filter(|slug| !self.rows.contains_key(slug))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RuleFile {
    version: u16,
    neutral: RuleBody,
    #[serde(default)]
    scene: Vec<SceneEntry>,
}

#[derive(Debug, Deserialize)]
struct SceneEntry {
    id: String,
    #[serde(flatten)]
    body: RuleBody,
}

#[derive(Debug, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
struct RuleBody {
    headroom_min: f32,
    headroom_max: f32,
    headroom_penalty: f32,
    tilt_tolerance_deg: f32,
    allows_dutch: bool,
    head_crop_allowed: bool,
    joint_cut_weight: f32,
    clutter_tolerance: f32,
    centre_preferred: bool,
    thirds_weight: f32,
    balance_weight: f32,
    aesthetic_weight: f32,
    rationale: String,
}

/// Check one row and turn it into a [`SceneRule`].
///
/// Every bound below is in the file's own header, and the two are checked against each
/// other by `tests/composition_rules.rs`: a documented range nobody enforces is a comment,
/// and an enforced range nobody documented is a trap.
fn validate(file: &str, key: &str, body: &RuleBody) -> Result<SceneRule, AuraError> {
    let refuse =
        |field: &str, rule: &str| errors::rules_refused(file, &format!("{key}.{field}"), rule);

    if body.rationale.trim().len() < MIN_RATIONALE {
        return Err(refuse(
            "rationale",
            "is missing or too short; every framing rule needs a written reason, because a \
             threshold nobody can explain is one nobody can argue with",
        ));
    }
    if !(0.0..=0.60).contains(&body.headroom_min) || !(0.0..=0.60).contains(&body.headroom_max) {
        return Err(refuse(
            "headroom_min/headroom_max",
            "must both be within 0.0..0.60 of the frame height",
        ));
    }
    if body.headroom_max <= body.headroom_min {
        return Err(refuse(
            "headroom_max",
            "must be above headroom_min; a band of zero width flags every photograph in the \
             scene",
        ));
    }
    if !(0.0..=0.40).contains(&body.headroom_penalty) {
        return Err(refuse("headroom_penalty", "must be within 0.0..0.40"));
    }
    if !(0.1..=12.0).contains(&body.tilt_tolerance_deg) {
        return Err(refuse(
            "tilt_tolerance_deg",
            "must be within 0.1..12.0; below a tenth of a degree nothing is level and above \
             twelve nothing is tilted",
        ));
    }
    if !(0.0..=1.5).contains(&body.joint_cut_weight) {
        return Err(refuse("joint_cut_weight", "must be within 0.0..1.5"));
    }
    if !(0.02..=0.60).contains(&body.clutter_tolerance) {
        return Err(refuse(
            "clutter_tolerance",
            "must be within 0.02..0.60; it is a divisor and a value near zero makes every \
             background infinitely cluttered",
        ));
    }
    if !(0.0..=1.0).contains(&body.thirds_weight) || !(0.0..=1.0).contains(&body.balance_weight) {
        return Err(refuse(
            "thirds_weight/balance_weight",
            "must both be within 0.0..1.0",
        ));
    }
    if !(0.0..=AESTHETIC_WEIGHT_CAP).contains(&body.aesthetic_weight) {
        return Err(refuse(
            "aesthetic_weight",
            "must be within 0.0..0.50; section 6.3 caps how much taste may move a score, and a \
             file that asks for more is refused rather than clamped so that whoever wrote it \
             finds out",
        ));
    }

    Ok(SceneRule {
        headroom_min: body.headroom_min,
        headroom_max: body.headroom_max,
        headroom_penalty: body.headroom_penalty,
        tilt_tolerance_deg: body.tilt_tolerance_deg,
        allows_dutch: body.allows_dutch,
        head_crop_allowed: body.head_crop_allowed,
        joint_cut_weight: body.joint_cut_weight,
        clutter_tolerance: body.clutter_tolerance,
        centre_preferred: body.centre_preferred,
        thirds_weight: body.thirds_weight,
        balance_weight: body.balance_weight,
        aesthetic_weight: body.aesthetic_weight,
        measured: true,
    })
}
