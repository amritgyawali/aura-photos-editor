//! What a photograph of this kind should be exposed to, and how far its colour may move.
//!
//! PHASE-15 section 8's step 1 belongs to DATA and step 6 to SRML; this module is the one
//! section 9 gives to PM: "approve `exposure_targets.toml` and the 'preserve mood' policy
//! with the consultant". It is the loader, and the file it loads is the sixth in the product
//! to require a written reason per row.
//!
//! ## Why this file is not `camera_calibration.toml`, again
//!
//! Phase 11's `rules` module drew this distinction and it is sharper here. There is no
//! experiment that establishes how bright a dance floor should be. There is a *convention*,
//! it differs between photographers, and section 12's fourth failure mode is
//! "auto-exposure fights the photographer's intent". What the file records is a product
//! decision per scene, and the loader's job is to make sure nobody ships one without saying
//! why.
//!
//! ## The two fields that are policy rather than tolerance
//!
//! [`Mood`] and [`SceneTarget::preserve_coloured_light`] are the two rows a rule engine
//! cannot argue its way out of, and both are section 2.1's:
//!
//! * a dark dance-floor frame is a decision and a dark family portrait is a mistake;
//! * a purple dance floor should stay purple.
//!
//! Everything else in a row is a bound. These two are the product's opinion, and they are
//! the reason the file needs a name against it.
//!
//! ## Refusal is whole-file
//!
//! A malformed table raises `AURA-ML-5063` and **nothing is loaded**. Phase 09's calibration
//! loader and phase 11's rule loader halt for the same reason, and in this phase the
//! consequence is the most visible one yet: a table that loaded the ceremony rows and
//! dropped the reception rows would expose half a wedding against measured bands and half
//! against neutral ones, and the gallery's brightness would change at a chapter boundary.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::error::AuraError;
use aura_core::SceneId;
use serde::Deserialize;

use crate::errors;

/// The shipped table, embedded so a binary can never disagree with its own defaults.
const EMBEDDED: &str = include_str!("../../config/exposure_targets.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "exposure_targets.toml";

/// The fewest characters a rationale may have.
///
/// Nine, the same as `prominence`'s, `scene_profiles`', `moment_profiles`',
/// `emotion_weights`', `camera_calibration`'s and `composition_rules`'. Six files, one rule,
/// and a test asserts the constants agree.
pub const MIN_RATIONALE: usize = 9;

/// How much of the confidence a missing target row costs.
///
/// The same flat 0.08 phase 11 charges for a missing rule row, and for the same reason: an
/// untargeted scene is judged against a *documented neutral* band rather than against a
/// guess, which is a weaker claim and not a worse one.
pub const UNTARGETED_CONFIDENCE_PENALTY: f32 = 0.08;

/// What to do when the subject is below the band.
///
/// Section 2.1's preserve-mood policy, as three named answers rather than as a boolean.
/// Three because "always lift" and "never lift" are both wrong somewhere: a family portrait
/// is never deliberately dark, a dance floor usually is, and a candid is whichever the
/// photographer meant - so the middle answer has to exist and has to be the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mood {
    /// Always bring the subject into the band. Nothing here is deliberately dark.
    Lift,
    /// Bring it into the band unless a constraint stops it. The default.
    #[default]
    Neutral,
    /// A dark frame here is probably a decision. Lift only to the bottom of the band and
    /// record [`aura_core::ToneCode::MoodPreserved`].
    Preserve,
}

impl Mood {
    /// Every mood, in the order the file documents them.
    pub const ALL: [Self; 3] = [Self::Lift, Self::Neutral, Self::Preserve];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lift => "lift",
            Self::Neutral => "neutral",
            Self::Preserve => "preserve",
        }
    }

    /// Parse the file's text. Unknown values are refused by the loader rather than
    /// defaulted, which is why this returns an option.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mood| mood.as_str() == text)
    }
}

/// One scene's exposure and white-balance bands.
///
/// Every field is documented in `config/exposure_targets.toml`, which is where a person
/// changing one reads about them. The doc comments here say what the *code* does with each
/// number, which is the other half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneTarget {
    /// Bottom of the band the prominence-weighted face luminance should land in.
    pub subject_luma_min: f32,
    /// Top of that band.
    pub subject_luma_max: f32,
    /// How much of the frame this scene will let become newly clipped to reach the band.
    pub clip_tolerance: f32,
    /// Multiplies phase 09's measured shadow headroom for this body.
    pub shadow_lift_scale: f32,
    /// What to do when the subject is below the band.
    pub mood: Mood,
    /// True when a saturated illuminant here is a creative choice.
    pub preserve_coloured_light: bool,
    /// The lowest colour temperature the correction will pull toward, in kelvin.
    pub tungsten_floor_k: f32,
    /// How far a solution may move from the camera's own as-shot value, in kelvin.
    pub max_correction_k: f32,
    /// Fraction of the frame this scene will let blow when the subject is backlit.
    pub backlight_allowance: f32,
    /// How much this scene expects to contain a known neutral, `0..1`.
    pub neutral_prior: f32,
    /// False when this row is the neutral fallback rather than a scene's own.
    ///
    /// Read by the analyser to decide whether to emit [`aura_core::ToneCode::NoTargetRow`]
    /// and to subtract [`UNTARGETED_CONFIDENCE_PENALTY`]. Not in the file; the loader sets
    /// it.
    pub measured: bool,
}

impl SceneTarget {
    /// The middle of the luminance band. What the solve aims for.
    #[must_use]
    pub fn luma_target(&self) -> f32 {
        f32::midpoint(self.subject_luma_min, self.subject_luma_max)
    }

    /// True when a subject luminance is already inside the band.
    ///
    /// Inclusive at both ends, for phase 11's reason: a band whose edges are violations is a
    /// band that moves the exposure of a photograph that was exposed exactly to the rule.
    #[must_use]
    pub fn in_band(&self, luma: f32) -> bool {
        luma >= self.subject_luma_min && luma <= self.subject_luma_max
    }

    /// Where inside the band the solve should aim, given the mood.
    ///
    /// [`Mood::Preserve`] aims at the **bottom** of the band rather than at the middle,
    /// which is the whole of "moody dance floors stay moody" in one line: the frame is
    /// brought up to the point where the faces are readable and no further.
    #[must_use]
    pub fn aim_for(&self, mood: Mood) -> f32 {
        match mood {
            Mood::Preserve => self.subject_luma_min,
            Mood::Lift | Mood::Neutral => self.luma_target(),
        }
    }
}

/// Every scene this build has targets for, and the row for the ones it does not.
#[derive(Debug, Clone)]
pub struct TargetTable {
    version: u16,
    neutral: SceneTarget,
    rows: BTreeMap<String, SceneTarget>,
}

impl TargetTable {
    /// Load the shipped table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5063` when the embedded file is malformed, which would be a build bug rather
    /// than a deployment one - and is why `tests/exposure_targets.rs` loads it too.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse("exposure_targets.toml (embedded)", EMBEDDED)
    }

    /// Load an installation override, or fall back to the shipped table.
    ///
    /// A malformed override is not fatal; a malformed baseline is. The same asymmetry every
    /// registry in the product has, for the same reason: the baseline is known good and
    /// falling back to it keeps the product open.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5063` only when the embedded baseline itself will not load.
    pub fn load_or_embedded(directory: &Path) -> Result<(Self, Option<AuraError>), AuraError> {
        let path = directory.join(OVERRIDE_FILE);
        if !path.exists() {
            return Ok((Self::embedded()?, None));
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            let refusal = errors::targets_refused(
                &path.display().to_string(),
                "file",
                "could not be read; the shipped exposure targets are in use",
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
    /// `AURA-ML-5063`, naming the file, the key and the rule.
    pub fn parse(name: &str, text: &str) -> Result<Self, AuraError> {
        let parsed: TargetFile = toml::from_str(text).map_err(|err| {
            errors::targets_refused(name, "file", &format!("is not valid TOML: {err}"))
        })?;

        if parsed.version == 0 {
            return Err(errors::targets_refused(
                name,
                "version",
                "must be at least 1; it is written into every estimate and 0 means unversioned",
            ));
        }

        let mut neutral = validate(name, "neutral", &parsed.neutral)?;
        neutral.measured = false;

        let mut rows = BTreeMap::new();
        for entry in &parsed.scene {
            let label = format!("scene.{}", entry.id);
            if SceneId::from_str_or_unknown(&entry.id) == SceneId::Unknown && entry.id != "unknown"
            {
                return Err(errors::targets_refused(
                    name,
                    &label,
                    "names a scene that is not in the frozen taxonomy; adding a scene is a \
                     phase 07 contract change and needs an ADR",
                ));
            }
            let mut row = validate(name, &label, &entry.body)?;
            row.measured = true;
            if rows.insert(entry.id.clone(), row).is_some() {
                return Err(errors::targets_refused(
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

    /// Which version of the table this is. Written into every estimate.
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
    /// are the same thing from the panel's point of view: an exposure decided against bands
    /// nobody wrote for this photograph.
    #[must_use]
    pub fn get(&self, scene: SceneId) -> SceneTarget {
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
    pub const fn neutral(&self) -> SceneTarget {
        self.neutral
    }

    /// Every scene with a row of its own, as slugs.
    #[must_use]
    pub fn scenes(&self) -> Vec<String> {
        self.rows.keys().cloned().collect()
    }

    /// The scenes in the frozen taxonomy that have no row here.
    ///
    /// What `AURA-ML-5064` is raised for, and what `ToneOutline::untargeted_scenes` reports.
    /// `unknown` is not counted: it has no row by design.
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
struct TargetFile {
    version: u16,
    neutral: TargetBody,
    #[serde(default)]
    scene: Vec<SceneEntry>,
}

#[derive(Debug, Deserialize)]
struct SceneEntry {
    id: String,
    #[serde(flatten)]
    body: TargetBody,
}

#[derive(Debug, Deserialize)]
struct TargetBody {
    subject_luma_min: f32,
    subject_luma_max: f32,
    clip_tolerance: f32,
    shadow_lift_scale: f32,
    mood: String,
    preserve_coloured_light: bool,
    tungsten_floor_k: f32,
    max_correction_k: f32,
    backlight_allowance: f32,
    neutral_prior: f32,
    rationale: String,
}

/// Check one row and turn it into a [`SceneTarget`].
///
/// Every bound below is in the file's own header, and the two are checked against each other
/// by `tests/exposure_targets.rs`: a documented range nobody enforces is a comment, and an
/// enforced range nobody documented is a trap.
fn validate(file: &str, key: &str, body: &TargetBody) -> Result<SceneTarget, AuraError> {
    let refuse =
        |field: &str, rule: &str| errors::targets_refused(file, &format!("{key}.{field}"), rule);

    if body.rationale.trim().len() < MIN_RATIONALE {
        return Err(refuse(
            "rationale",
            "is missing or too short; every exposure decision needs a written reason, because a \
             band nobody can explain is one nobody can argue with - and this is the most visible \
             decision in the product",
        ));
    }
    if !(0.05..=0.95).contains(&body.subject_luma_min)
        || !(0.05..=0.95).contains(&body.subject_luma_max)
    {
        return Err(refuse(
            "subject_luma_min/subject_luma_max",
            "must both be within 0.05..0.95",
        ));
    }
    if body.subject_luma_max <= body.subject_luma_min {
        return Err(refuse(
            "subject_luma_max",
            "must be above subject_luma_min; a band of zero width moves the exposure of every \
             photograph in the scene",
        ));
    }
    if !(0.0..=0.20).contains(&body.clip_tolerance) {
        return Err(refuse(
            "clip_tolerance",
            "must be within 0.0..0.20; above a fifth of the frame the exposure is no longer \
             being constrained by clipping at all",
        ));
    }
    if !(0.4..=1.6).contains(&body.shadow_lift_scale) {
        return Err(refuse(
            "shadow_lift_scale",
            "must be within 0.4..1.6; it multiplies a measured headroom and a value outside \
             that either ignores phase 09 or refuses every lift",
        ));
    }
    let Some(mood) = Mood::parse(body.mood.trim()) else {
        return Err(refuse(
            "mood",
            "must be one of lift, neutral or preserve; it decides whether a dark photograph in \
             this scene is a mistake or a decision, and there is no sensible default for that",
        ));
    };
    if !(1800.0..=5500.0).contains(&body.tungsten_floor_k) {
        return Err(refuse(
            "tungsten_floor_k",
            "must be within 1800..5500; below that no light at a wedding is that warm, and above \
             it the floor would start pushing daylight frames warm",
        ));
    }
    if !(200.0..=8000.0).contains(&body.max_correction_k) {
        return Err(refuse(
            "max_correction_k",
            "must be within 200..8000; below 200 K nothing can be corrected and above 8000 K \
             nothing is bounded",
        ));
    }
    if !(0.0..=0.50).contains(&body.backlight_allowance) {
        return Err(refuse(
            "backlight_allowance",
            "must be within 0.0..0.50; above half the frame the photograph is a silhouette and \
             the rule stops describing a backlit subject",
        ));
    }
    if !(0.0..=1.0).contains(&body.neutral_prior) {
        return Err(refuse("neutral_prior", "must be within 0.0..1.0"));
    }

    Ok(SceneTarget {
        subject_luma_min: body.subject_luma_min,
        subject_luma_max: body.subject_luma_max,
        clip_tolerance: body.clip_tolerance,
        shadow_lift_scale: body.shadow_lift_scale,
        mood,
        preserve_coloured_light: body.preserve_coloured_light,
        tungsten_floor_k: body.tungsten_floor_k,
        max_correction_k: body.max_correction_k,
        backlight_allowance: body.backlight_allowance,
        neutral_prior: body.neutral_prior,
        measured: true,
    })
}
