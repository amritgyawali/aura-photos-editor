//! What a photograph of this kind should look like, and how far its colour may move.
//!
//! PHASE-16 section 9 gives PM one task: "approve `tone_intent.toml` per scene with the
//! consultant". This is the loader, and the file it loads is the seventh in the product to
//! require a written reason per row.
//!
//! ## Why this is not `exposure_targets.toml`
//!
//! Phase 15's file decides how *bright* a photograph should be, and the argument for a band
//! is at least about faces being visible. This file decides what a photograph should *look
//! like*, which has no such anchor: there is no experiment that establishes how much
//! contrast a Hindu ceremony wants. There is a convention, it differs between photographers,
//! and phase 17 is where a photographer's own answer replaces every number here.
//!
//! So the loader's job is narrower than phase 15's and more important: make sure nobody can
//! ship a row without saying why, and make sure no row can ask for a grade the product's own
//! ceilings forbid. [`SUBTLETY_CEILING`] and [`CLIP_CEILING`] are checked here, per row,
//! against the frozen contract - a scene cannot buy its way past a guarantee by editing a
//! config file.
//!
//! ## Refusal is whole-file
//!
//! A malformed table raises `AURA-ML-5070` and **nothing is loaded**. Phase 09's calibration
//! loader, phase 11's rule loader and phase 15's target loader all halt for the same reason,
//! and here the consequence is that a wedding would be graded to two different conventions
//! with the change falling on a chapter boundary.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::colour::{CLIP_CEILING, SUBTLETY_CEILING};
use aura_core::contract::error::AuraError;
use aura_core::SceneId;
use serde::Deserialize;

use crate::errors;

/// The shipped table, embedded so a binary can never disagree with its own defaults.
const EMBEDDED: &str = include_str!("../../config/tone_intent.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "tone_intent.toml";

/// The fewest characters a rationale may have.
///
/// Nine, the same as every other config file in the product. Seven files, one rule, and a
/// test asserts the constants agree.
pub const MIN_RATIONALE: usize = 9;

/// How much of the confidence a missing intent row costs.
///
/// The same flat 0.08 phases 11 and 15 charge, and for the same reason: an untargeted scene
/// is graded against a *documented neutral* intent rather than against a guess, which is a
/// weaker claim and not a worse one.
pub const UNTARGETED_CONFIDENCE_PENALTY: f32 = 0.08;

/// One scene's grading intent.
///
/// Every field is documented in `config/tone_intent.toml`, which is where a person changing
/// one reads about them. The doc comments here say what the *code* does with each number,
/// which is the other half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneIntent {
    /// The scene's baseline contrast, in the recipe's units.
    pub contrast: f32,
    /// The interquartile luminance spread the curve is fitted to give the subject.
    pub subject_contrast: f32,
    /// How far this scene wants its shadows opened, before the noise guard.
    pub shadow_lift: f32,
    /// How hard this scene pulls its highlights down. Negative recovers.
    pub highlight_recovery: f32,
    /// Vibrance, weighted away from already-saturated colours.
    pub vibrance: f32,
    /// Flat saturation. Nearly always zero.
    pub saturation: f32,
    /// How much of the frame this scene will let become newly clipped by the grade.
    pub clip_tolerance: f32,
    /// Where this scene wants foliage to sit on the hue wheel, in degrees.
    pub greenery_hue_deg: f32,
    /// Where this scene wants foliage's saturation, `0..1`.
    pub greenery_saturation: f32,
    /// Where this scene wants sky's saturation, `0..1`.
    pub sky_saturation: f32,
    /// How saturated a non-subject colour may be before the harmony solver tames it.
    pub distractor_ceiling: f32,
    /// The largest adjustment any single HSL band may receive, in recipe units.
    pub max_band_shift: f32,
    /// The ceiling on the whole grade's magnitude.
    pub subtlety_cap: f32,
    /// False when this row is the neutral fallback rather than a scene's own.
    ///
    /// Read by the analyser to decide whether to emit `ColourCode::NoIntentRow` and to
    /// subtract [`UNTARGETED_CONFIDENCE_PENALTY`]. Not in the file; the loader sets it.
    pub measured: bool,
}

impl SceneIntent {
    /// The largest shift one band may take, given how confident the content inference was.
    ///
    /// The band cap is scaled by the confidence rather than gated on it, so a band AURA is
    /// half sure about gets half the adjustment rather than none. The gate exists as well -
    /// `content::MIN_CONFIDENCE` - and it is below the point where the scaling has already
    /// made the move invisible.
    #[must_use]
    pub fn band_cap(&self, confidence: f32) -> f32 {
        self.max_band_shift * confidence.clamp(0.0, 1.0)
    }
}

/// Every scene this build has intents for, and the row for the ones it does not.
#[derive(Debug, Clone)]
pub struct IntentTable {
    version: u16,
    neutral: SceneIntent,
    rows: BTreeMap<String, SceneIntent>,
}

impl IntentTable {
    /// Load the shipped table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5070` when the embedded file is malformed, which would be a build bug rather
    /// than a deployment one - and is why `tests/tone_intent.rs` loads it too.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse("tone_intent.toml (embedded)", EMBEDDED)
    }

    /// Load an installation override, or fall back to the shipped table.
    ///
    /// A malformed override is not fatal; a malformed baseline is. The same asymmetry every
    /// registry in the product has, for the same reason: the baseline is known good and
    /// falling back to it keeps the product open.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5070` only when the embedded baseline itself will not load.
    pub fn load_or_embedded(directory: &Path) -> Result<(Self, Option<AuraError>), AuraError> {
        let path = directory.join(OVERRIDE_FILE);
        if !path.exists() {
            return Ok((Self::embedded()?, None));
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            let refusal = errors::intent_refused(
                &path.display().to_string(),
                "file",
                "could not be read; the shipped tone intents are in use",
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
    /// `AURA-ML-5070`, naming the file, the key and the rule.
    pub fn parse(name: &str, text: &str) -> Result<Self, AuraError> {
        let parsed: IntentFile = toml::from_str(text).map_err(|err| {
            errors::intent_refused(name, "file", &format!("is not valid TOML: {err}"))
        })?;

        if parsed.version == 0 {
            return Err(errors::intent_refused(
                name,
                "version",
                "must be at least 1; it is written into every decision and 0 means unversioned",
            ));
        }

        let mut neutral = validate(name, "neutral", &parsed.neutral)?;
        neutral.measured = false;

        let mut rows = BTreeMap::new();
        for entry in &parsed.scene {
            let label = format!("scene.{}", entry.id);
            if SceneId::from_str_or_unknown(&entry.id) == SceneId::Unknown && entry.id != "unknown"
            {
                return Err(errors::intent_refused(
                    name,
                    &label,
                    "names a scene that is not in the frozen taxonomy; adding a scene is a \
                     phase 07 contract change and needs an ADR",
                ));
            }
            let mut row = validate(name, &label, &entry.body)?;
            row.measured = true;
            if rows.insert(entry.id.clone(), row).is_some() {
                return Err(errors::intent_refused(
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

    /// Which version of the table this is. Written into every decision.
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
    /// unclassified frame and a frame whose scene has no row are reported the same way.
    #[must_use]
    pub fn get(&self, scene: SceneId) -> SceneIntent {
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
    pub const fn neutral(&self) -> SceneIntent {
        self.neutral
    }

    /// Every scene with a row of its own, as slugs.
    #[must_use]
    pub fn scenes(&self) -> Vec<String> {
        self.rows.keys().cloned().collect()
    }

    /// The scenes in the frozen taxonomy that have no row here.
    ///
    /// What `ColourOutline::untargeted_scenes` reports. `unknown` is not counted: it has no
    /// row by design.
    #[must_use]
    pub fn untargeted(&self) -> Vec<String> {
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
struct IntentFile {
    version: u16,
    neutral: IntentBody,
    #[serde(default)]
    scene: Vec<SceneEntry>,
}

#[derive(Debug, Deserialize)]
struct SceneEntry {
    id: String,
    #[serde(flatten)]
    body: IntentBody,
}

#[derive(Debug, Deserialize)]
struct IntentBody {
    contrast: f32,
    subject_contrast: f32,
    shadow_lift: f32,
    highlight_recovery: f32,
    vibrance: f32,
    saturation: f32,
    clip_tolerance: f32,
    greenery_hue_deg: f32,
    greenery_saturation: f32,
    sky_saturation: f32,
    distractor_ceiling: f32,
    max_band_shift: f32,
    subtlety_cap: f32,
    rationale: String,
}

/// Check one row and turn it into a [`SceneIntent`].
///
/// Every bound below is in the file's own header, and `tests/tone_intent.rs` checks the two
/// against each other: a documented range nobody enforces is a comment, and an enforced
/// range nobody documented is a trap.
#[allow(clippy::too_many_lines)]
fn validate(file: &str, key: &str, body: &IntentBody) -> Result<SceneIntent, AuraError> {
    let refuse =
        |field: &str, rule: &str| errors::intent_refused(file, &format!("{key}.{field}"), rule);

    if body.rationale.trim().len() < MIN_RATIONALE {
        return Err(refuse(
            "rationale",
            "is missing or too short; every grading decision needs a written reason, because \
             there is no experiment that establishes how much contrast a ceremony wants and a \
             number with no argument behind it is one nobody can change",
        ));
    }
    for (name, value) in [
        ("contrast", body.contrast),
        ("shadow_lift", body.shadow_lift),
        ("highlight_recovery", body.highlight_recovery),
        ("vibrance", body.vibrance),
        ("saturation", body.saturation),
    ] {
        if !(-100.0..=100.0).contains(&value) {
            return Err(refuse(
                name,
                "must be within -100..100, the recipe's own range",
            ));
        }
    }
    if !(0.05..=0.60).contains(&body.subject_contrast) {
        return Err(refuse(
            "subject_contrast",
            "must be within 0.05..0.60; below that the curve is asked to flatten a face into \
             one tone and above it to separate one into two",
        ));
    }
    if !(0.0..=0.20).contains(&body.clip_tolerance) {
        return Err(refuse(
            "clip_tolerance",
            "must be within 0.0..0.20; above a fifth of the frame the grade is no longer being \
             constrained by clipping at all",
        ));
    }
    if !(90.0..=160.0).contains(&body.greenery_hue_deg) {
        return Err(refuse(
            "greenery_hue_deg",
            "must be within 90..160; below 90 the target is yellow and above 160 it is cyan, \
             and neither is what foliage looks like",
        ));
    }
    for (name, value) in [
        ("greenery_saturation", body.greenery_saturation),
        ("sky_saturation", body.sky_saturation),
        ("distractor_ceiling", body.distractor_ceiling),
    ] {
        if !(0.0..=1.0).contains(&value) {
            return Err(refuse(name, "must be within 0.0..1.0"));
        }
    }
    if !(0.0..=50.0).contains(&body.max_band_shift) {
        return Err(refuse(
            "max_band_shift",
            "must be within 0.0..50.0; a band that can move fifty units can recolour a dress, \
             and a solver that can recolour a dress is one nobody can predict",
        ));
    }
    // The two guarantees a config file may not buy its way past. Both are frozen contract
    // constants, checked here per row rather than clamped, because a row that asked for more
    // than the product allows is a row somebody wrote believing it would be honoured.
    if !(0.05..=SUBTLETY_CEILING).contains(&body.subtlety_cap) {
        return Err(refuse(
            "subtlety_cap",
            "must be within 0.05..SUBTLETY_CEILING; a scene cannot raise the ceiling that stops \
             the product looking filtered, and a scene that asks to is one nobody would have \
             noticed shipping",
        ));
    }
    if body.clip_tolerance > 0.20 {
        return Err(refuse("clip_tolerance", "must be within 0.0..0.20"));
    }

    Ok(SceneIntent {
        contrast: body.contrast,
        subject_contrast: body.subject_contrast,
        shadow_lift: body.shadow_lift,
        highlight_recovery: body.highlight_recovery,
        vibrance: body.vibrance,
        saturation: body.saturation,
        clip_tolerance: body.clip_tolerance,
        greenery_hue_deg: body.greenery_hue_deg,
        greenery_saturation: body.greenery_saturation,
        sky_saturation: body.sky_saturation,
        distractor_ceiling: body.distractor_ceiling,
        max_band_shift: body.max_band_shift,
        subtlety_cap: body.subtlety_cap,
        measured: true,
    })
}

/// The scene-level clipping tolerance, never above the product's own ceiling.
///
/// A free function rather than a method so the guard and the gate call the same thing. The
/// minimum is taken rather than the row's value, which is the second half of "a config file
/// cannot buy its way past a guarantee": the loader refuses a row above the ceiling, and this
/// makes the ceiling hold even if a row somehow got through.
#[must_use]
pub fn clip_tolerance(intent: &SceneIntent) -> f32 {
    intent.clip_tolerance.min(CLIP_CEILING.max(0.0)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped table, or the neutral row when it will not load - which is a failure the
    /// first test reports rather than a panic every other test inherits.
    fn shipped() -> IntentTable {
        IntentTable::embedded().unwrap_or_else(|_| IntentTable {
            version: 0,
            neutral: SceneIntent {
                contrast: 0.0,
                subject_contrast: 0.26,
                shadow_lift: 0.0,
                highlight_recovery: 0.0,
                vibrance: 0.0,
                saturation: 0.0,
                clip_tolerance: 0.0,
                greenery_hue_deg: 120.0,
                greenery_saturation: 0.4,
                sky_saturation: 0.4,
                distractor_ceiling: 1.0,
                max_band_shift: 0.0,
                subtlety_cap: 0.05,
                measured: false,
            },
            rows: BTreeMap::new(),
        })
    }

    #[test]
    fn the_shipped_table_loads_and_covers_every_scene() {
        let table = shipped();
        assert!(table.version() >= 1);
        assert_eq!(
            table.untargeted(),
            Vec::<String>::new(),
            "every scene in the frozen taxonomy needs an intent row"
        );
        assert_eq!(table.rows(), SceneId::ALL.len() - 1);
    }

    #[test]
    fn a_row_without_a_reason_is_refused() {
        let text = r#"
version = 1
[neutral]
contrast = 8
subject_contrast = 0.26
shadow_lift = 8
highlight_recovery = -10
vibrance = 6
saturation = 0
clip_tolerance = 0.008
greenery_hue_deg = 120
greenery_saturation = 0.42
sky_saturation = 0.46
distractor_ceiling = 0.80
max_band_shift = 8
subtlety_cap = 0.22
rationale = "short"
"#;
        let Err(err) = IntentTable::parse("test", text) else {
            unreachable!("a row with a bare rationale must be refused")
        };
        assert_eq!(err.code.0, "AURA-ML-5070");
        assert!(err.detail.contains("rationale"));
    }

    #[test]
    fn a_scene_cannot_raise_the_subtlety_ceiling() {
        // The property this loader exists for. A row that asks to grade harder than the
        // product allows is refused rather than clamped, because clamping it would let
        // somebody ship a number they believed was being honoured.
        let table = shipped();
        for scene in SceneId::ALL {
            let row = table.get(scene);
            assert!(
                row.subtlety_cap <= SUBTLETY_CEILING,
                "{scene:?} asks for more than the ceiling"
            );
        }
    }

    #[test]
    fn every_scene_prefers_vibrance_to_flat_saturation() {
        // A product decision expressed as a test. Flat saturation is what makes a sari look
        // like a screenshot; vibrance weights away from what is already saturated. A row
        // that reached for saturation instead would be a change somebody should have to
        // argue for.
        let table = shipped();
        for scene in SceneId::ALL {
            let row = table.get(scene);
            assert!(
                row.saturation.abs() <= f32::EPSILON,
                "{scene:?} sets flat saturation"
            );
        }
    }

    #[test]
    fn a_ceremony_is_graded_more_gently_than_a_couple_portrait() {
        // The one cross-row property worth asserting: the frames a couple keeps are the ones
        // this phase is least allowed to stylise, and the portraits are the ones it is most
        // expected to. A file that inverted this would ship a produced-looking ceremony.
        let table = shipped();
        let ceremony = table.get(SceneId::Ceremony);
        let portrait = table.get(SceneId::CouplePortrait);
        assert!(ceremony.subtlety_cap < portrait.subtlety_cap);
        assert!(ceremony.contrast < portrait.contrast);
        assert!(ceremony.max_band_shift < portrait.max_band_shift);
    }

    #[test]
    fn the_two_coloured_light_scenes_barely_touch_a_band() {
        // Phase 15 decided not to neutralise a purple dance floor. This asserts that phase
        // 16 cannot do it by another route: the band cap on both rows is at the bottom of
        // the file's range and the distractor ceiling is at the top.
        let table = shipped();
        for scene in [SceneId::FirstDance, SceneId::DanceFloor] {
            let row = table.get(scene);
            assert!(
                row.max_band_shift <= 6.0,
                "{scene:?} can move a band too far"
            );
            assert!(
                row.distractor_ceiling >= 0.90,
                "{scene:?} tames too eagerly"
            );
        }
    }
}
