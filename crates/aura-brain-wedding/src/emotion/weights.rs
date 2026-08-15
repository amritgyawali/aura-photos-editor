//! `emotion_weights.toml`: the scene and tradition weighting, the ranker's coefficients
//! and the per-scene calibration, in one versioned file.
//!
//! ## Why one file and not three
//!
//! Because all three invalidate exactly the same thing - `ImageEmotion::emotion_score` -
//! and nothing else. Three files would mean three version columns on
//! `image_interaction` that always move together, which is phase 09's third inherited
//! rule ("three version columns, because they invalidate three different things") read
//! in the direction that removes a column rather than adding one. ADR-0021 section 4.
//!
//! ## The eight refusals, and why a half-loaded table halts
//!
//! Every other failure in this phase degrades. This one does not, for `AURA-ML-5024`'s
//! and `AURA-ML-5031`'s reason with one extra clause: this file is where the product
//! decides that a composed Hindu ceremony is not an empty gallery. A **half-loaded**
//! weight table applies the tradition-aware weights to some scenes and the shipped
//! defaults to others, silently, in precisely the direction section 12's first failure
//! mode warns about. So the loader refuses the document and leaves the previous table in
//! place.
//!
//! The asymmetry `SceneProfileRegistry` and `MomentProfileRegistry` both use applies
//! here too: a malformed **override** falls back to the shipped baseline and is reported;
//! a malformed **baseline** is a broken build and halts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use aura_core::contract::emotion::FaceExpression;
use aura_core::{AuraError, SceneId};
use serde::Deserialize;

use crate::errors;

/// The shipped baseline, compiled in so a binary can never disagree with itself.
const EMBEDDED: &str = include_str!("../../config/emotion_weights.toml");

/// What an installation override is called, under `<catalog>/config/`.
const OVERRIDE_FILE: &str = "emotion_weights.toml";

/// The traditions a weight table may name.
///
/// The eight the ritual head is conditioned on, which is the list of things that can
/// actually be selected at run time. A tradition weighted here that the head cannot emit
/// is a weight nobody will ever notice is wrong, so the loader refuses it.
///
/// **Five of the eight carry a row in the shipped file, and the other three do not.**
/// `sikh` has no taxonomy in `config/rituals/`, so nobody here has established what its
/// rites look like or how composed a couple are during them - and a multiplier invented
/// without that is not a finding, it is a guess with a version number. `mixed` and
/// `unclear` are abstentions rather than traditions. All three fall back to the neutral
/// case, which is `[default]` set from the balanced fixture set rather than from the
/// Christian one. `tests/config.rs` asserts the five that are there are exactly the five
/// with taxonomies.
pub const KNOWN_TRADITIONS: [&str; 8] = crate::scene::ritual::TRADITIONS;

/// How far a channel or term weight may go.
///
/// Zero disables a channel, which is a real thing to want - see
/// `[scene.family_portrait]`'s `genuineness` at 0.15 and the argument for why it is
/// nearly zero. Two is twice normal, which is as far as any argument in the shipped file
/// goes.
pub const WEIGHT_RANGE: (f32, f32) = (0.0, 2.0);

/// How far a Bradley-Terry coefficient may go.
///
/// The utility is a linear form over nine features in `0..1`, so a coefficient past
/// eight makes the logistic saturate and every frame in a wedding scores 0.00 or 1.00 -
/// a ranking that carries no information and looks like a working one.
pub const COEFFICIENT_RANGE: (f32, f32) = (-8.0, 8.0);

/// The shortest rationale the loader will accept.
///
/// Nine characters, the same floor `scene_profiles.toml` and `moment_profiles.toml` use.
/// It is not a length check so much as a friction check: somebody who cannot write a
/// sentence saying why a photographer would agree with a number has not finished
/// deciding it.
pub const MIN_RATIONALE: usize = 9;

// ---------------------------------------------------------------------------
// The shapes
// ---------------------------------------------------------------------------

/// The weights one scene is judged by.
///
/// Eight channel weights and four term weights. Every field is populated: a scene table
/// in the file may set any subset, and the rest are inherited from `[default]` at load
/// time rather than at lookup time, so nothing downstream has to know which came from
/// where.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneWeights {
    /// The eight channel weights, in [`FaceExpression::channels`] order.
    pub channels: [f32; FaceExpression::CHANNELS],
    /// How much a detected interaction is worth here.
    pub interaction: f32,
    /// How much being the strongest frame of its moment is worth.
    pub peak: f32,
    /// How much being somebody's reaction is worth.
    pub reaction: f32,
    /// How much two people looking at each other is worth.
    pub mutual_gaze: f32,
}

impl SceneWeights {
    /// The compiled-in fallback, used only when the embedded file itself is missing.
    ///
    /// Never reached in a shipped build - the loader halts instead - and it exists so
    /// that `Default` is honest rather than so that anything depends on it.
    pub const NEUTRAL: Self = Self {
        channels: [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        interaction: 1.0,
        peak: 1.0,
        reaction: 1.0,
        mutual_gaze: 1.0,
    };

    /// One channel's weight, by position in [`FaceExpression::channels`].
    #[must_use]
    pub fn channel(&self, index: usize) -> f32 {
        self.channels.get(index).copied().unwrap_or(1.0)
    }

    /// The smile weight, slot 0.
    #[must_use]
    pub fn smile(&self) -> f32 {
        self.channel(0)
    }

    /// The genuineness weight, slot 1.
    #[must_use]
    pub fn genuineness(&self) -> f32 {
        self.channel(1)
    }

    /// The laughter weight, slot 2.
    #[must_use]
    pub fn laughter(&self) -> f32 {
        self.channel(2)
    }

    /// The tears weight, slot 3.
    #[must_use]
    pub fn tears(&self) -> f32 {
        self.channel(3)
    }

    /// The composure weight, slot 6.
    ///
    /// The single most consequential number in the file. See
    /// `crates/aura-brain-wedding/config/emotion_weights.toml`'s header.
    #[must_use]
    pub fn composed(&self) -> f32 {
        self.channel(6)
    }

    /// The discomfort weight, slot 7. A magnitude; the term is subtracted.
    #[must_use]
    pub fn discomfort(&self) -> f32 {
        self.channel(7)
    }
}

impl Default for SceneWeights {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// Multipliers one tradition applies on top of a scene's weights.
///
/// Only three channels are settable. Not because the others are the same everywhere, but
/// because nobody has evidence about the others - and a weight invented without evidence
/// is worse than no weight, because it looks like a finding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraditionWeights {
    /// Multiplier on `smile`.
    pub smile: f32,
    /// Multiplier on `laughter`.
    pub laughter: f32,
    /// Multiplier on `composed`.
    pub composed: f32,
}

impl TraditionWeights {
    /// No correction. What a wedding with no detected tradition gets.
    pub const NONE: Self = Self {
        smile: 1.0,
        laughter: 1.0,
        composed: 1.0,
    };
}

impl Default for TraditionWeights {
    fn default() -> Self {
        Self::NONE
    }
}

/// The Bradley-Terry utility, as nine coefficients and a bias.
///
/// Section 6.4: `P(a beats b) = sigmoid(u(a) - u(b))` with `u(x) = w . f(x)`, so the
/// utility *is* the score up to the calibration. The feature vector these multiply is
/// documented in [`crate::emotion::score`], and every one of the nine is in `0..1` before
/// its coefficient is applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ranker {
    /// The intercept. Negative, so a frame with nothing in it scores low rather than at
    /// a half.
    pub bias: f32,
    /// Weight on the scene-weighted expression mix over the frame's faces.
    pub subject_expression: f32,
    /// Weight on prominence-weighted genuineness.
    pub genuineness: f32,
    /// Weight on the strongest interaction.
    pub interaction: f32,
    /// Weight on peak proximity.
    pub peak: f32,
    /// Weight on the reaction bonus.
    pub reaction: f32,
    /// Weight on mutual gaze.
    pub mutual_gaze: f32,
    /// Weight on the composure term.
    pub composure: f32,
    /// Weight on the tears term.
    pub tears: f32,
    /// Weight on the group term.
    pub group: f32,
}

impl Ranker {
    /// How many features the utility reads.
    pub const FEATURES: usize = 9;

    /// The coefficients in feature order, for the dot product and for the eval harness's
    /// ablations.
    #[must_use]
    pub const fn coefficients(&self) -> [f32; Self::FEATURES] {
        [
            self.subject_expression,
            self.genuineness,
            self.interaction,
            self.peak,
            self.reaction,
            self.mutual_gaze,
            self.composure,
            self.tears,
            self.group,
        ]
    }

    /// The feature names, in [`Ranker::coefficients`] order.
    pub const FEATURE_NAMES: [&'static str; Self::FEATURES] = [
        "subject_expression",
        "genuineness",
        "interaction",
        "peak",
        "reaction",
        "mutual_gaze",
        "composure",
        "tears",
        "group",
    ];

    /// The utility of one feature vector.
    #[must_use]
    pub fn utility(&self, features: &[f32; Self::FEATURES]) -> f32 {
        let mut sum = self.bias;
        for (weight, value) in self.coefficients().into_iter().zip(features.iter()) {
            sum += weight * value;
        }
        sum
    }

    /// The utility squashed into `0..1`.
    ///
    /// The logistic is exactly the Bradley-Terry link: `P(a beats b)` is
    /// `sigmoid(u(a) - u(b))`, so `sigmoid(u(x))` is the probability this frame beats a
    /// frame of zero utility - which is a meaningful scale rather than an arbitrary
    /// squash.
    #[must_use]
    pub fn score(&self, features: &[f32; Self::FEATURES]) -> f32 {
        let utility = self.utility(features);
        (1.0 / (1.0 + (-utility).exp())).clamp(0.0, 1.0)
    }
}

impl Default for Ranker {
    /// A ranker that reads only the expression term.
    ///
    /// The compiled-in fallback, never reached in a shipped build. Deliberately *not*
    /// all-ones: a default that weighted nine terms equally would produce a plausible
    /// ordering from a file that failed to load, which is the silent failure the loader
    /// halts to prevent.
    fn default() -> Self {
        Self {
            bias: -2.0,
            subject_expression: 3.0,
            genuineness: 0.0,
            interaction: 0.0,
            peak: 0.0,
            reaction: 0.0,
            mutual_gaze: 0.0,
            composure: 0.0,
            tears: 0.0,
            group: 0.0,
        }
    }
}

/// A monotone piecewise-linear map, per scene.
///
/// Section 6.4's isotonic layer. It ships as the identity everywhere - condition C4 of
/// the phase 10 exit report - and the machinery is real: [`Calibration::new`] refuses a
/// non-monotone knot sequence, because a calibration that reorders frames is not a
/// calibration.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    knots: Vec<(f32, f32)>,
}

impl Calibration {
    /// The map that changes nothing.
    #[must_use]
    pub fn identity() -> Self {
        Self { knots: Vec::new() }
    }

    /// Build from knots, refusing anything that is not a monotone map on `0..1`.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` when the two arrays differ in length, when either is shorter than
    /// two, when either leaves `0..1`, or when either is not non-decreasing.
    pub fn new(file: &str, key: &str, x: &[f32], y: &[f32]) -> Result<Self, AuraError> {
        if x.len() != y.len() {
            return Err(errors::emotion_config_refused(
                file,
                key,
                "has calibration arrays of different lengths",
            ));
        }
        if x.len() < 2 {
            return Err(errors::emotion_config_refused(
                file,
                key,
                "has fewer than two calibration knots; a map needs at least an endpoint each",
            ));
        }
        let mut knots = Vec::with_capacity(x.len());
        let mut last = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (input, output) in x.iter().zip(y.iter()) {
            if !(0.0..=1.0).contains(input) || !(0.0..=1.0).contains(output) {
                return Err(errors::emotion_config_refused(
                    file,
                    key,
                    "has a calibration knot outside 0..1",
                ));
            }
            if *input < last.0 || *output < last.1 {
                return Err(errors::emotion_config_refused(
                    file,
                    key,
                    "has a calibration knot sequence that is not monotone; a map that reorders \
                     frames is not a calibration",
                ));
            }
            last = (*input, *output);
            knots.push((*input, *output));
        }
        Ok(Self { knots })
    }

    /// Map one score through the calibration.
    ///
    /// Linear between knots, flat outside them. An empty knot list is the identity.
    #[must_use]
    pub fn apply(&self, value: f32) -> f32 {
        let value = value.clamp(0.0, 1.0);
        if self.knots.is_empty() {
            return value;
        }
        let mut previous = match self.knots.first() {
            Some(first) => *first,
            None => return value,
        };
        if value <= previous.0 {
            return previous.1;
        }
        for knot in self.knots.iter().skip(1) {
            if value <= knot.0 {
                let span = knot.0 - previous.0;
                if span <= f32::EPSILON {
                    return knot.1;
                }
                let t = (value - previous.0) / span;
                return (previous.1 + t * (knot.1 - previous.1)).clamp(0.0, 1.0);
            }
            previous = *knot;
        }
        previous.1
    }

    /// True when this map changes nothing at all.
    ///
    /// Read by the exit report's evidence and by the eval harness, which asserts that
    /// today it is true for every scene.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.knots.is_empty()
            || self
                .knots
                .iter()
                .all(|(input, output)| (input - output).abs() <= f32::EPSILON)
    }
}

impl Default for Calibration {
    fn default() -> Self {
        Self::identity()
    }
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// The whole table, loaded and validated.
#[derive(Debug, Clone)]
pub struct EmotionWeights {
    default: SceneWeights,
    scenes: BTreeMap<SceneId, SceneWeights>,
    traditions: BTreeMap<String, TraditionWeights>,
    calibrations: BTreeMap<SceneId, Calibration>,
    ranker: Ranker,
    version: u16,
}

impl Default for EmotionWeights {
    fn default() -> Self {
        Self {
            default: SceneWeights::NEUTRAL,
            scenes: BTreeMap::new(),
            traditions: BTreeMap::new(),
            calibrations: BTreeMap::new(),
            ranker: Ranker::default(),
            version: 0,
        }
    }
}

impl EmotionWeights {
    /// Load the shipped baseline.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` when the embedded file is malformed, which would be a build bug
    /// rather than a deployment one - and is why `tests/config.rs` loads it too.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse("emotion_weights.toml (embedded)", EMBEDDED)
    }

    /// Load an installation override, or fall back to the baseline.
    ///
    /// A malformed override is not fatal; a malformed baseline is. The same asymmetry
    /// `SceneProfileRegistry::load_or_embedded` and `MomentProfileRegistry` have, and
    /// for the same reason: the baseline is known good, and falling back to it keeps the
    /// product open.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` only when the embedded baseline itself will not load.
    pub fn load_or_embedded(directory: &Path) -> Result<(Self, Option<AuraError>), AuraError> {
        let path = directory.join(OVERRIDE_FILE);
        if !path.exists() {
            return Ok((Self::embedded()?, None));
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            let refusal = errors::emotion_config_refused(
                &path.display().to_string(),
                "file",
                "could not be read; the shipped weights are in use",
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
    /// `AURA-ML-5039` on any of the eight rules. Nothing is partially loaded: the
    /// document is validated into local maps and only assigned at the end.
    pub fn parse(file: &str, text: &str) -> Result<Self, AuraError> {
        let parsed: Document = toml::from_str(text).map_err(|e| {
            errors::emotion_config_refused(file, "file", &format!("is not valid TOML: {e}"))
        })?;

        let default = scene_weights(file, "default", &parsed.default, &SceneWeights::NEUTRAL)?;

        let mut scenes = BTreeMap::new();
        for (slug, table) in &parsed.scene {
            let scene = SceneId::from_str_or_unknown(slug);
            if scene == SceneId::Unknown && slug != "unknown" {
                return Err(errors::emotion_config_refused(
                    file,
                    &format!("scene.{slug}"),
                    "names a scene this build does not know",
                ));
            }
            scenes.insert(
                scene,
                scene_weights(file, &format!("scene.{slug}"), table, &default)?,
            );
        }

        let known: BTreeSet<&str> = KNOWN_TRADITIONS.into_iter().collect();
        let mut traditions = BTreeMap::new();
        for (slug, table) in &parsed.tradition {
            if !known.contains(slug.as_str()) {
                return Err(errors::emotion_config_refused(
                    file,
                    &format!("tradition.{slug}"),
                    "names a tradition with no ritual taxonomy in config/rituals/",
                ));
            }
            rationale(
                file,
                &format!("tradition.{slug}"),
                table.rationale.as_deref(),
            )?;
            let smile = weight(file, &format!("tradition.{slug}.smile"), table.smile, 1.0)?;
            let laughter = weight(
                file,
                &format!("tradition.{slug}.laughter"),
                table.laughter,
                1.0,
            )?;
            let composed = weight(
                file,
                &format!("tradition.{slug}.composed"),
                table.composed,
                1.0,
            )?;
            traditions.insert(
                slug.clone(),
                TraditionWeights {
                    smile,
                    laughter,
                    composed,
                },
            );
        }

        let mut calibrations = BTreeMap::new();
        for (slug, table) in &parsed.calibration {
            let scene = SceneId::from_str_or_unknown(slug);
            if scene == SceneId::Unknown && slug != "unknown" {
                return Err(errors::emotion_config_refused(
                    file,
                    &format!("calibration.{slug}"),
                    "names a scene this build does not know",
                ));
            }
            let key = format!("calibration.{slug}");
            rationale(file, &key, table.rationale.as_deref())?;
            calibrations.insert(scene, Calibration::new(file, &key, &table.x, &table.y)?);
        }

        let ranker = ranker(file, &parsed.ranker)?;

        Ok(Self {
            default,
            scenes,
            traditions,
            calibrations,
            ranker,
            version: parsed.version,
        })
    }

    /// The table's version, written into `image_interaction.weights_ver`.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// The weights one scene is judged by, with the tradition's multipliers applied.
    ///
    /// The two are combined here rather than by the caller, so that no scoring path can
    /// read a scene's weights and forget the tradition - which would be a silent
    /// reversion to exactly the bias section 12 names first.
    #[must_use]
    pub fn for_scene(&self, scene: SceneId, tradition: Option<&str>) -> SceneWeights {
        let mut weights = self.scenes.get(&scene).copied().unwrap_or(self.default);
        let Some(multipliers) = tradition.and_then(|slug| self.traditions.get(slug)) else {
            return weights;
        };
        // Slots 0, 2 and 6: smile, laughter, composed. The three the file may set.
        for (slot, multiplier) in [
            (0usize, multipliers.smile),
            (2, multipliers.laughter),
            (6, multipliers.composed),
        ] {
            if let Some(channel) = weights.channels.get_mut(slot) {
                *channel = (*channel * multiplier).clamp(WEIGHT_RANGE.0, WEIGHT_RANGE.1);
            }
        }
        weights
    }

    /// The default weights, for a scene with no row.
    #[must_use]
    pub const fn default_weights(&self) -> SceneWeights {
        self.default
    }

    /// The ranker in force.
    #[must_use]
    pub const fn ranker(&self) -> Ranker {
        self.ranker
    }

    /// One scene's calibration, or the identity.
    #[must_use]
    pub fn calibration(&self, scene: SceneId) -> Calibration {
        self.calibrations
            .get(&scene)
            .cloned()
            .unwrap_or_else(Calibration::identity)
    }

    /// True when every calibration in the table is the identity map.
    ///
    /// Read by the eval harness and by the phase gate: condition C4 says the isotonic
    /// layer ships unfitted, and this is how that claim is checked rather than asserted.
    #[must_use]
    pub fn calibration_is_identity(&self) -> bool {
        self.calibrations.values().all(Calibration::is_identity)
    }

    /// How many scenes carry their own row.
    #[must_use]
    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }

    /// How many traditions carry multipliers.
    #[must_use]
    pub fn tradition_count(&self) -> usize {
        self.traditions.len()
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Document {
    version: u16,
    default: SceneTable,
    #[serde(default)]
    scene: BTreeMap<String, SceneTable>,
    #[serde(default)]
    tradition: BTreeMap<String, TraditionTable>,
    #[serde(default)]
    calibration: BTreeMap<String, CalibrationTable>,
    ranker: RankerTable,
}

#[derive(Debug, Deserialize)]
struct SceneTable {
    rationale: Option<String>,
    smile: Option<f32>,
    genuineness: Option<f32>,
    laughter: Option<f32>,
    tears: Option<f32>,
    surprise: Option<f32>,
    tenderness: Option<f32>,
    composed: Option<f32>,
    discomfort: Option<f32>,
    interaction: Option<f32>,
    peak: Option<f32>,
    reaction: Option<f32>,
    mutual_gaze: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct TraditionTable {
    rationale: Option<String>,
    smile: Option<f32>,
    laughter: Option<f32>,
    composed: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct CalibrationTable {
    rationale: Option<String>,
    x: Vec<f32>,
    y: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct RankerTable {
    rationale: Option<String>,
    bias: f32,
    subject_expression: f32,
    genuineness: f32,
    interaction: f32,
    peak: f32,
    reaction: f32,
    mutual_gaze: f32,
    composure: f32,
    tears: f32,
    group: f32,
}

/// Rule 1, and the one with the most friction and the most value.
fn rationale(file: &str, key: &str, text: Option<&str>) -> Result<(), AuraError> {
    match text {
        Some(value) if value.trim().len() >= MIN_RATIONALE => Ok(()),
        _ => Err(errors::emotion_config_refused(
            file,
            key,
            "has no rationale; a weight nobody can explain is a magic number",
        )),
    }
}

/// Rule 3: a weight inside `WEIGHT_RANGE`, or the inherited value.
fn weight(file: &str, key: &str, value: Option<f32>, inherited: f32) -> Result<f32, AuraError> {
    let Some(value) = value else {
        return Ok(inherited);
    };
    if !value.is_finite() || value < WEIGHT_RANGE.0 || value > WEIGHT_RANGE.1 {
        return Err(errors::emotion_config_refused(
            file,
            key,
            &format!(
                "has a weight outside {}..{}",
                WEIGHT_RANGE.0, WEIGHT_RANGE.1
            ),
        ));
    }
    Ok(value)
}

/// One scene table, inheriting from `parent` for anything it does not set.
///
/// Inheritance is resolved **here**, at load time, rather than at lookup time. A lookup
/// that walked a parent chain would let a caller ask for one field and silently get the
/// default for it, and the scoring path has nine such lookups per frame.
fn scene_weights(
    file: &str,
    key: &str,
    table: &SceneTable,
    parent: &SceneWeights,
) -> Result<SceneWeights, AuraError> {
    rationale(file, key, table.rationale.as_deref())?;
    let channels = [
        weight(
            file,
            &format!("{key}.smile"),
            table.smile,
            parent.channel(0),
        )?,
        weight(
            file,
            &format!("{key}.genuineness"),
            table.genuineness,
            parent.channel(1),
        )?,
        weight(
            file,
            &format!("{key}.laughter"),
            table.laughter,
            parent.channel(2),
        )?,
        weight(
            file,
            &format!("{key}.tears"),
            table.tears,
            parent.channel(3),
        )?,
        weight(
            file,
            &format!("{key}.surprise"),
            table.surprise,
            parent.channel(4),
        )?,
        weight(
            file,
            &format!("{key}.tenderness"),
            table.tenderness,
            parent.channel(5),
        )?,
        weight(
            file,
            &format!("{key}.composed"),
            table.composed,
            parent.channel(6),
        )?,
        weight(
            file,
            &format!("{key}.discomfort"),
            table.discomfort,
            parent.channel(7),
        )?,
    ];

    // Rule 4. Every positive channel at zero means a scene that can never score above
    // its bias, which reads to a photographer as "the product dislikes this kind of
    // photograph" rather than as a config mistake. Slot 7 is excluded because
    // `discomfort` is subtracted: a scene that does not penalise awkwardness is a
    // legitimate thing to want.
    if channels
        .iter()
        .take(FaceExpression::CHANNELS - 1)
        .all(|value| *value <= f32::EPSILON)
    {
        return Err(errors::emotion_config_refused(
            file,
            key,
            "has every expression channel at zero; nothing in this scene could ever score",
        ));
    }

    Ok(SceneWeights {
        channels,
        interaction: weight(
            file,
            &format!("{key}.interaction"),
            table.interaction,
            parent.interaction,
        )?,
        peak: weight(file, &format!("{key}.peak"), table.peak, parent.peak)?,
        reaction: weight(
            file,
            &format!("{key}.reaction"),
            table.reaction,
            parent.reaction,
        )?,
        mutual_gaze: weight(
            file,
            &format!("{key}.mutual_gaze"),
            table.mutual_gaze,
            parent.mutual_gaze,
        )?,
    })
}

/// Rule 7: every coefficient inside `COEFFICIENT_RANGE`.
fn ranker(file: &str, table: &RankerTable) -> Result<Ranker, AuraError> {
    rationale(file, "ranker", table.rationale.as_deref())?;
    let check = |key: &str, value: f32| -> Result<f32, AuraError> {
        if !value.is_finite() || value < COEFFICIENT_RANGE.0 || value > COEFFICIENT_RANGE.1 {
            return Err(errors::emotion_config_refused(
                file,
                &format!("ranker.{key}"),
                &format!(
                    "has a coefficient outside {}..{}; past this the logistic saturates and every \
                     frame scores 0 or 1",
                    COEFFICIENT_RANGE.0, COEFFICIENT_RANGE.1
                ),
            ));
        }
        Ok(value)
    };

    Ok(Ranker {
        bias: check("bias", table.bias)?,
        subject_expression: check("subject_expression", table.subject_expression)?,
        genuineness: check("genuineness", table.genuineness)?,
        interaction: check("interaction", table.interaction)?,
        peak: check("peak", table.peak)?,
        reaction: check("reaction", table.reaction)?,
        mutual_gaze: check("mutual_gaze", table.mutual_gaze)?,
        composure: check("composure", table.composure)?,
        tears: check("tears", table.tears)?,
        group: check("group", table.group)?,
    })
}
