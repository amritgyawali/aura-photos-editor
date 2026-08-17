//! What matters in this kind of photograph, and how much.
//!
//! PHASE-12 section 8's first step: "author `cull_weights.toml` ... with the photographer
//! consultant; PM signs off before code". This module is the loader, and the file it loads
//! is the **fifth** in the product to require a written reason per row - after
//! `prominence.toml`, `scene_profiles.toml`, `moment_profiles.toml`, `emotion_weights.toml`
//! and `composition_rules.toml`.
//!
//! It is also the one where a wrong row costs the most. A wrong prominence weight makes a
//! face rank oddly; a wrong emotion weight makes a browser lead with the wrong frame. A
//! wrong row here removes photographs from a wedding.
//!
//! ## Why the weights are exponents rather than coefficients
//!
//! Section 6.1: `keep_score = w_t·technical^a · w_e·emotion^b · w_c·composition^c` "in log
//! space", so that "a catastrophic technical failure cannot be rescued by emotion and vice
//! versa". A weighted **sum** does not have that property and is what every competitor
//! ships: a frame at technical 0.05 and emotion 0.95 sums to a respectable number and
//! multiplies to nothing.
//!
//! So the four numbers in each row are the weights of a geometric mean, and their absolute
//! size does not matter - only their ratio does. They are still written to sum to roughly
//! one, because a table of numbers that sum to one is a table a product manager can read.
//!
//! ## The one structural constraint the loader enforces
//!
//! [`SceneWeights::composition`] may not exceed [`SceneWeights::technical`]. ADR-0023
//! section 3 decided that taste breaks ties rather than overriding substance, and phase 11
//! capped what its own arithmetic would grant. This is where that decision is *spent*: a
//! row that weighted framing above whether the photograph worked would be the phase 11 cap
//! undone by a config file, one phase later, with a gallery rather than a review queue at
//! stake.
//!
//! A row that asks for it is **refused**, not clamped - for the reason phase 11's
//! `AESTHETIC_WEIGHT_CAP` is refused rather than clamped: a product manager who wrote it
//! meant something, and should find out that they cannot have it.
//!
//! ## Refusal is whole-file
//!
//! A malformed table raises `AURA-ML-5051` and **nothing is loaded**. Phases 09 and 11
//! halt for the same reason and this phase's version of it is the sharpest: a table that
//! loaded the ceremony rows and dropped the reception rows would cull half a wedding
//! against measured weights and half against neutral ones, and the gallery that came out
//! would be thin after dinner. That reads as a product opinion about receptions and
//! nothing at all like a configuration error.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::error::AuraError;
use aura_core::{CullMode, SceneId};
use serde::Deserialize;

use crate::errors;

/// The shipped table, embedded so a binary can never disagree with its own defaults.
const EMBEDDED: &str = include_str!("../config/cull_weights.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "cull_weights.toml";

/// The fewest characters a rationale may have.
///
/// Nine, the same as every other config file in this product, and a test asserts the
/// constants agree. A product decision nobody can explain is a magic number even when a
/// photographer made it.
pub const MIN_RATIONALE: usize = 9;

/// The most keepers any one moment may be allowed, before mode adjustment.
///
/// Section 6.2's "up to 3-5". Five, and the mode may not raise it further: a moment with
/// six keepers is not a moment, it is a burst that phase 08 should have split.
pub const MAX_KEEPERS_CAP: u32 = 5;

/// How much a missing scene row costs the confidence of every decision made on it.
///
/// Flat rather than per-row, exactly as phase 11's `UNRULED_CONFIDENCE_PENALTY` is, and
/// for the same reason: an unweighted scene is judged against a *documented* neutral row
/// rather than against a guess, which is a weaker claim and not a worse one.
pub const UNWEIGHTED_CONFIDENCE_PENALTY: f32 = 0.08;

/// What matters in one kind of photograph.
///
/// Every field is documented in `config/cull_weights.toml`, which is where a person
/// changing one reads about them. The doc comments here say what the *code* does with each
/// number, which is the other half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneWeights {
    /// How much whether the photograph worked matters. Phase 09.
    pub technical: f32,
    /// How much what is happening in it matters. Phase 10.
    pub emotion: f32,
    /// How much how it is framed matters. Phase 11.
    ///
    /// Never above [`SceneWeights::technical`]; see this module's header.
    pub composition: f32,
    /// How much who is in it matters. Phase 06.
    pub prominence: f32,
    /// Below this fused score a frame is not kept unless a guarantee holds it.
    ///
    /// Section 6.3's "threshold" - the bar a force-added coverage frame is *below* when it
    /// is marked `CoveredWeak`. Scene-conditioned because invariant 7 says every threshold
    /// is, and because a dance-floor frame and an altar frame do not have to clear the
    /// same bar to be worth delivering.
    pub floor: f32,
    /// The most frames this scene's moments may contribute, before mode adjustment.
    ///
    /// The upper end of section 6.2's `k`. The lower end is always one: every moment that
    /// has an eligible frame contributes at least one, which is what stops a chapter quota
    /// from deleting a moment entirely.
    pub max_keepers: u32,
}

impl SceneWeights {
    /// The sum of the four weights. Never zero; the loader refuses a row that would be.
    #[must_use]
    pub fn total(&self) -> f32 {
        self.technical + self.emotion + self.composition + self.prominence
    }
}

/// Which way a mode moves the preferences, and nothing else.
///
/// Three numbers, and what is *not* here is the point: there is no field for a coverage
/// rule, a must-have minimum or an identity threshold, because section 6.4 says modes
/// "shift thresholds and k-values, never the coverage rules". A type with nowhere to put a
/// guarantee cannot weaken one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModeShift {
    /// Added to every scene's floor. Negative keeps more.
    pub floor_delta: f32,
    /// Added to every scene's keeper cap, then clamped to [`MAX_KEEPERS_CAP`].
    pub keeper_delta: i32,
    /// Multiplies the predicted gallery size.
    pub target_scale: f32,
}

/// Every scene's weights, the three modes, and the digest of the file they came from.
#[derive(Debug, Clone)]
pub struct WeightTable {
    version: u16,
    neutral: SceneWeights,
    rows: BTreeMap<SceneId, SceneWeights>,
    modes: BTreeMap<CullMode, ModeShift>,
    digest: String,
}

impl WeightTable {
    /// The table this build ships.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5051` when the embedded table is malformed - which is a build failure
    /// rather than a runtime one, and is why the phase gate loads it first.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse(EMBEDDED, "embedded cull_weights.toml")
    }

    /// An installation override.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5051` when the file cannot be read or is malformed. **Nothing is loaded**
    /// and the caller keeps whatever table it had.
    pub fn load(path: &Path) -> Result<Self, AuraError> {
        let text = std::fs::read_to_string(path).map_err(|error| {
            errors::weights_refused(
                &path.display().to_string(),
                "file",
                &format!("could not be read: {error}"),
            )
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    /// Parse and validate a table. Every rule is checked here and nowhere else.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5051`, naming the file, the key and the rule, in the order somebody fixes
    /// them in.
    pub fn parse(text: &str, origin: &str) -> Result<Self, AuraError> {
        let file: FileShape = toml::from_str(text)
            .map_err(|error| errors::weights_refused(origin, "file", &format!("{error}")))?;
        if file.version == 0 {
            return Err(errors::weights_refused(
                origin,
                "version",
                "must be at least 1",
            ));
        }

        let neutral = check_row(origin, "neutral", &file.neutral)?;
        let mut rows = BTreeMap::new();
        for row in &file.scene {
            let scene = SceneId::from_str_or_unknown(&row.id);
            if scene == SceneId::Unknown {
                return Err(errors::weights_refused(
                    origin,
                    &row.id,
                    "is not a scene in the frozen taxonomy",
                ));
            }
            let weights = check_row(origin, &row.id, &row.row())?;
            if rows.insert(scene, weights).is_some() {
                return Err(errors::weights_refused(origin, &row.id, "appears twice"));
            }
        }

        let mut modes = BTreeMap::new();
        for row in &file.mode {
            let Some(mode) = CullMode::ALL.into_iter().find(|m| m.as_str() == row.id) else {
                return Err(errors::weights_refused(
                    origin,
                    &row.id,
                    "is not one of conservative, balanced or aggressive",
                ));
            };
            if row.rationale.trim().len() < MIN_RATIONALE {
                return Err(errors::weights_refused(
                    origin,
                    &row.id,
                    "needs a rationale of at least nine characters",
                ));
            }
            if !(-0.5..=0.5).contains(&row.floor_delta) {
                return Err(errors::weights_refused(
                    origin,
                    &row.id,
                    "floor_delta must be within -0.5..0.5",
                ));
            }
            if !(-2..=2).contains(&row.keeper_delta) {
                return Err(errors::weights_refused(
                    origin,
                    &row.id,
                    "keeper_delta must be within -2..2",
                ));
            }
            if !(0.5..=2.0).contains(&row.target_scale) {
                return Err(errors::weights_refused(
                    origin,
                    &row.id,
                    "target_scale must be within 0.5..2.0",
                ));
            }
            let shift = ModeShift {
                floor_delta: row.floor_delta,
                keeper_delta: row.keeper_delta,
                target_scale: row.target_scale,
            };
            if modes.insert(mode, shift).is_some() {
                return Err(errors::weights_refused(origin, &row.id, "appears twice"));
            }
        }
        for mode in CullMode::ALL {
            if !modes.contains_key(&mode) {
                return Err(errors::weights_refused(
                    origin,
                    mode.as_str(),
                    "has no row, and all three modes must be defined",
                ));
            }
        }

        Ok(Self {
            version: file.version,
            neutral,
            rows,
            modes,
            digest: blake3::hash(text.as_bytes()).to_hex().to_string(),
        })
    }

    /// Which table this is. Stored on the run and part of what `AURA-ML-5048` compares.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many scenes have their own row.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    /// The digest of the exact text this table was parsed from.
    ///
    /// Half of the config digest in `SelectionResult::deterministic_hash`. It is the
    /// content and not the version, because a release can change a weight without
    /// remembering to bump anything - and a stored gallery that cannot tell that happened
    /// is a stored gallery nobody can reproduce.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The documented neutral row, used for a scene with no weights of its own.
    #[must_use]
    pub const fn neutral(&self) -> SceneWeights {
        self.neutral
    }

    /// One scene's weights, and whether the neutral row was substituted.
    ///
    /// Returning the substitution rather than hiding it, so the caller can lower confidence
    /// and the outline can list the scene. Phase 11's `RuleTable::for_scene` shape, for its
    /// reason: a fallback nobody can see is a fallback nobody fixes.
    #[must_use]
    pub fn for_scene(&self, scene: SceneId) -> (SceneWeights, bool) {
        self.rows
            .get(&scene)
            .map_or((self.neutral, true), |row| (*row, false))
    }

    /// One mode's shift.
    ///
    /// Infallible: the loader refuses a table missing any of the three, so by the time a
    /// pass asks, all three exist. A `Result` here would be a `Result` nine call sites
    /// unwrapped.
    #[must_use]
    pub fn mode(&self, mode: CullMode) -> ModeShift {
        self.modes.get(&mode).copied().unwrap_or(ModeShift {
            floor_delta: 0.0,
            keeper_delta: 0,
            target_scale: 1.0,
        })
    }

    /// Scenes in the frozen taxonomy with no row of their own, by slug.
    ///
    /// What `CullOutline` would report and what the phase gate asserts is empty.
    #[must_use]
    pub fn unweighted(&self) -> Vec<String> {
        SceneId::ALL
            .into_iter()
            .filter(|scene| *scene != SceneId::Unknown)
            .filter(|scene| !self.rows.contains_key(scene))
            .map(|scene| scene.as_str().to_string())
            .collect()
    }
}

/// Check one row's ranges and its rationale.
fn check_row(origin: &str, key: &str, row: &RowShape) -> Result<SceneWeights, AuraError> {
    if row.rationale.trim().len() < MIN_RATIONALE {
        return Err(errors::weights_refused(
            origin,
            key,
            "needs a rationale of at least nine characters",
        ));
    }
    for (name, value) in [
        ("technical", row.technical),
        ("emotion", row.emotion),
        ("composition", row.composition),
        ("prominence", row.prominence),
    ] {
        if !(0.0..=1.0).contains(&value) {
            return Err(errors::weights_refused(
                origin,
                key,
                &format!("{name} must be within 0.0..1.0"),
            ));
        }
    }
    let weights = SceneWeights {
        technical: row.technical,
        emotion: row.emotion,
        composition: row.composition,
        prominence: row.prominence,
        floor: row.floor,
        max_keepers: row.max_keepers,
    };
    if weights.total() <= 0.0 {
        return Err(errors::weights_refused(
            origin,
            key,
            "has four zero weights, which would score every frame the same",
        ));
    }
    if row.composition > row.technical {
        return Err(errors::weights_refused(
            origin,
            key,
            "weights composition above technical; taste breaks ties, it does not override \
             whether the photograph worked",
        ));
    }
    if !(0.0..=0.9).contains(&row.floor) {
        return Err(errors::weights_refused(
            origin,
            key,
            "floor must be within 0.0..0.9",
        ));
    }
    if row.max_keepers == 0 || row.max_keepers > MAX_KEEPERS_CAP {
        return Err(errors::weights_refused(
            origin,
            key,
            "max_keepers must be within 1..5",
        ));
    }
    Ok(weights)
}

// -- the file's shape -------------------------------------------------------

#[derive(Debug, Deserialize)]
struct FileShape {
    version: u16,
    neutral: RowShape,
    #[serde(default)]
    scene: Vec<SceneShape>,
    #[serde(default)]
    mode: Vec<ModeShape>,
}

/// One `[[scene]]` row.
///
/// The six fields are repeated rather than `#[serde(flatten)]`ed onto [`RowShape`], and
/// the repetition is on purpose: `flatten` routes through `deserialize_any`, which turns a
/// `floor = 0` written without its decimal point into a type error whose message names
/// neither the field nor the row. A config file a product manager edits should fail with
/// "`dance_floor` floor must be within 0.0..0.9", not with a serde trace.
#[derive(Debug, Deserialize)]
struct SceneShape {
    id: String,
    technical: f32,
    emotion: f32,
    composition: f32,
    prominence: f32,
    floor: f32,
    max_keepers: u32,
    rationale: String,
}

impl SceneShape {
    fn row(&self) -> RowShape {
        RowShape {
            technical: self.technical,
            emotion: self.emotion,
            composition: self.composition,
            prominence: self.prominence,
            floor: self.floor,
            max_keepers: self.max_keepers,
            rationale: self.rationale.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RowShape {
    technical: f32,
    emotion: f32,
    composition: f32,
    prominence: f32,
    floor: f32,
    max_keepers: u32,
    rationale: String,
}

#[derive(Debug, Deserialize)]
struct ModeShape {
    id: String,
    floor_delta: f32,
    keeper_delta: i32,
    target_scale: f32,
    rationale: String,
}
