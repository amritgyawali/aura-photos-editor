//! The scene profile registry: where invariant 7 stops being a slogan.
//!
//! > "No threshold is global; every threshold is a function of the detected scene and
//! > subject role."
//!
//! For six phases that has been a promise with nothing behind it, because there was no
//! scene. [`SceneProfileRegistry`] is the lookup that makes it true, and
//! `crates/aura-brain-wedding/config/scene_profiles.toml` is the table it reads.
//!
//! ## Three rules the loader enforces
//!
//! **1. A profile without a rationale does not load.** Section 12's third failure mode
//! is that this file becomes a dumping ground of magic numbers. The friction is
//! intentional: somebody who cannot write a sentence explaining why `dance_floor`
//! tolerates three times the ceremony's noise has not finished deciding it.
//!
//! **2. The three weights may not sum above 1.0.** They are a split of one composite
//! score, and a scene whose weights summed to 1.4 would score higher than every other
//! scene by construction - which would look like "the ceremony is better photographed
//! than the reception" in every report the product ever produces.
//!
//! **3. A missing scene is a substitution, not a refusal.** [`SceneProfileRegistry::get`]
//! is infallible and returns [`SceneProfile::neutral`] with `AURA-ML-5023` logged once.
//! A scene that cannot be graded takes a wedding out of the product; a scene graded
//! neutrally does not.
//!
//! Rules 1 and 2 halt. Rule 3 does not. That asymmetry is the design: refuse a config
//! file that would silently change every number, and never refuse a wedding.
//!
//! ## Three layers, in this order
//!
//! 1. the embedded baseline, which always loads;
//! 2. an installation override at `<catalog>/config/scene_profiles.toml`;
//! 3. per-project rows in `scene_profiles` with `user_edited = 1`, which are the
//!    photographer's own and are never overwritten by a reload.
//!
//! Layer 2 is a whole file and replaces layer 1 outright. Layer 3 is per scene. A
//! partial override file would be unreadable after the third tuning round, and a
//! per-scene database row is exactly what a settings panel writes.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use aura_core::contract::scene::{EditIntent, SceneId, SceneProfile};
use aura_core::AuraError;
use serde::Deserialize;

use crate::errors;

/// The baseline that ships with the build.
const EMBEDDED: &str = include_str!("../../config/scene_profiles.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "scene_profiles.toml";

/// One profile, as authored.
#[derive(Debug, Clone, Deserialize)]
struct ProfileFile {
    keeper_rate: [f32; 2],
    max_acceptable_noise: f32,
    max_acceptable_blur: f32,
    subject_focus_weight: f32,
    emotion_weight: f32,
    composition_weight: f32,
    #[serde(default = "neutral_intent")]
    editing_intent: String,
    #[serde(default)]
    must_cover: bool,
    rationale: String,
}

fn neutral_intent() -> String {
    "neutral".to_string()
}

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
struct RegistryFile {
    version: u16,
    #[serde(default)]
    scenes: BTreeMap<String, ProfileFile>,
}

/// The fewest characters a rationale may have.
///
/// Nine, matching the `CHECK (length(rationale) > 8)` in migration 7. Two places
/// because the loader must refuse before the database is reached and the database must
/// refuse a row written by anything else; a test asserts the two agree.
pub const MIN_RATIONALE: usize = 9;

/// Scene tolerances and weights, with their rationales.
#[derive(Debug)]
pub struct SceneProfileRegistry {
    profiles: BTreeMap<SceneId, SceneProfile>,
    rationales: BTreeMap<SceneId, String>,
    version: u16,
    /// Which scenes have already had `AURA-ML-5023` logged for them, as a bitmask.
    ///
    /// A bitmask and an atomic because [`SceneProfileRegistry::get`] is called from
    /// `StoryService::profile`, which is `&self` and shared across threads, and because
    /// a warning logged four thousand times is a warning nobody reads. Twenty-three
    /// scenes fit in a `u64` with room to spare.
    warned: AtomicU64,
}

impl SceneProfileRegistry {
    /// Load the shipped baseline.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5024` when the embedded file is malformed, which would be a build bug
    /// rather than a deployment one - and is exactly why it is a test in
    /// `crates/aura-brain-wedding/tests/profiles.rs` rather than only a runtime check.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse("scene_profiles.toml (embedded)", EMBEDDED)
    }

    /// The shipped baseline, or an empty registry.
    ///
    /// Every lookup then substitutes neutral and logs, which is a wedding that still
    /// finishes. Used only where a refusal cannot be surfaced to anybody - a debug
    /// panel, a `Default` impl - and never on the classification path, which calls
    /// [`SceneProfileRegistry::embedded`] and propagates the refusal.
    #[must_use]
    pub fn embedded_or_empty() -> Self {
        match Self::embedded() {
            Ok(loaded) => loaded,
            Err(err) => {
                tracing::error!(
                    target: "scene.profiles",
                    code = %err.code,
                    "the shipped scene profiles would not load; every scene will be graded neutrally"
                );
                Self {
                    profiles: BTreeMap::new(),
                    rationales: BTreeMap::new(),
                    version: 0,
                    warned: AtomicU64::new(0),
                }
            }
        }
    }

    /// Load an installation override, or fall back to the baseline.
    ///
    /// Returns the registry and, when an override was present and refused, the reason -
    /// so a caller can show it in the Problems list. A malformed override is **not**
    /// fatal here, unlike a malformed embedded file: the baseline is known good and
    /// falling back to it keeps the product open, which is the same call
    /// `aura_vision::face::prominence` makes for its weight file.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5024` only when the embedded baseline itself will not load.
    pub fn load_or_embedded(directory: &Path) -> Result<(Self, Option<AuraError>), AuraError> {
        let path = directory.join(OVERRIDE_FILE);
        if !path.exists() {
            return Ok((Self::embedded()?, None));
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            let refusal = errors::config_refused(
                &path.display().to_string(),
                "file",
                "could not be read; the shipped profiles are in use",
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
    /// `AURA-ML-5024`, naming the file, the scene and the rule.
    pub fn parse(name: &str, text: &str) -> Result<Self, AuraError> {
        let parsed: RegistryFile = toml::from_str(text).map_err(|err| {
            errors::config_refused(name, "file", &format!("is not valid TOML: {err}"))
        })?;

        let mut profiles = BTreeMap::new();
        let mut rationales = BTreeMap::new();

        for (key, authored) in parsed.scenes {
            let scene = SceneId::from_str_or_unknown(&key);
            if scene == SceneId::Unknown && key != "unknown" {
                return Err(errors::config_refused(
                    name,
                    &key,
                    "is not one of the 22 scenes in `SceneId`",
                ));
            }
            if scene == SceneId::Unknown {
                return Err(errors::config_refused(
                    name,
                    &key,
                    "is the abstention, not a scene; it has no profile and never will",
                ));
            }

            let profile = Self::validate(name, scene, &authored)?;
            rationales.insert(scene, authored.rationale.trim().to_string());
            profiles.insert(scene, profile);
        }

        Ok(Self {
            profiles,
            rationales,
            version: parsed.version,
            warned: AtomicU64::new(0),
        })
    }

    /// Every rule a profile must satisfy, in the order somebody would fix them.
    fn validate(
        file: &str,
        scene: SceneId,
        authored: &ProfileFile,
    ) -> Result<SceneProfile, AuraError> {
        let key = scene.as_str();

        // Rule 1, and it is first because it is the one that matters.
        if authored.rationale.trim().len() < MIN_RATIONALE {
            return Err(errors::config_refused(
                file,
                key,
                "has no rationale; a threshold nobody can explain does not load. Say why a \
                 photographer would agree with this number, not what the number is",
            ));
        }

        let (keeper_min, keeper_max) = (
            authored.keeper_rate.first().copied().unwrap_or(0.0),
            authored.keeper_rate.get(1).copied().unwrap_or(0.0),
        );
        if !(0.0..=1.0).contains(&keeper_min) || !(0.0..=1.0).contains(&keeper_max) {
            return Err(errors::config_refused(
                file,
                key,
                "has a keeper_rate outside 0..1",
            ));
        }
        if keeper_min > keeper_max {
            return Err(errors::config_refused(
                file,
                key,
                "has keeper_rate min above max",
            ));
        }
        for (label, value) in [
            ("max_acceptable_noise", authored.max_acceptable_noise),
            ("max_acceptable_blur", authored.max_acceptable_blur),
            ("subject_focus_weight", authored.subject_focus_weight),
            ("emotion_weight", authored.emotion_weight),
            ("composition_weight", authored.composition_weight),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return Err(errors::config_refused(
                    file,
                    key,
                    &format!("has {label} = {value}, outside 0..1"),
                ));
            }
        }

        // Rule 2. The tolerance is 1e-3 rather than exact, because these are authored
        // decimals and 0.35 + 0.30 + 0.35 is not 1.0 in binary floating point.
        let sum =
            authored.subject_focus_weight + authored.emotion_weight + authored.composition_weight;
        if sum > 1.0 + 1e-3 {
            return Err(errors::config_refused(
                file,
                key,
                &format!(
                    "has weights summing to {sum:.3}; they are a split of one composite score and \
                     a scene that sums above 1.0 outscores every other scene by construction"
                ),
            ));
        }

        let intent = authored.editing_intent.trim();
        if EditIntent::from_str_or_neutral(intent).as_str() != intent {
            return Err(errors::config_refused(
                file,
                key,
                &format!(
                    "has editing_intent = \"{intent}\"; use airy, neutral, warm, moody or punchy"
                ),
            ));
        }

        Ok(SceneProfile {
            scene,
            keeper_rate: (keeper_min, keeper_max),
            max_acceptable_noise: authored.max_acceptable_noise,
            max_acceptable_blur: authored.max_acceptable_blur,
            subject_focus_weight: authored.subject_focus_weight,
            emotion_weight: authored.emotion_weight,
            composition_weight: authored.composition_weight,
            editing_intent: EditIntent::from_str_or_neutral(intent),
            must_cover: authored.must_cover,
        })
    }

    /// The file's version, written into `scene_profiles.profile_ver`.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many scenes are described.
    #[must_use]
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// True when nothing loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// The tolerances one scene is judged by. Never fails; see the module header.
    #[must_use]
    pub fn get(&self, scene: SceneId) -> SceneProfile {
        if let Some(found) = self.profiles.get(&scene) {
            return found.clone();
        }
        self.warn_once(scene);
        SceneProfile::neutral(scene)
    }

    /// Why one scene's numbers are what they are, when it has a profile.
    ///
    /// On the IPC surface, because "why is my dance floor being judged this way" is a
    /// question a photographer asks and invariant 2 says has to have an answer.
    #[must_use]
    pub fn rationale(&self, scene: SceneId) -> Option<&str> {
        self.rationales.get(&scene).map(String::as_str)
    }

    /// Every scene that has a profile, in [`SceneId::ALL`] order.
    #[must_use]
    pub fn scenes(&self) -> Vec<SceneId> {
        SceneId::ALL
            .into_iter()
            .filter(|scene| self.profiles.contains_key(scene))
            .collect()
    }

    /// The scenes phase 12's coverage guarantee applies to.
    #[must_use]
    pub fn must_cover(&self) -> Vec<SceneId> {
        self.scenes()
            .into_iter()
            .filter(|scene| self.get(*scene).must_cover)
            .collect()
    }

    /// Replace one scene's profile, for a per-project override read from the catalog.
    pub fn set(&mut self, profile: SceneProfile, rationale: String) {
        self.rationales.insert(profile.scene, rationale);
        self.profiles.insert(profile.scene, profile);
    }

    /// Log `AURA-ML-5023` the first time a scene is missing, and never again.
    fn warn_once(&self, scene: SceneId) {
        let bit = 1u64 << (scene.as_index() % 64);
        let seen = self.warned.fetch_or(bit, Ordering::Relaxed);
        if seen & bit != 0 {
            return;
        }
        let missing = errors::profile_missing(scene.as_str());
        tracing::warn!(
            target: "scene.profiles",
            code = %missing.code,
            scene = scene.as_str(),
            "{}", missing.detail
        );
    }
}

impl Default for SceneProfileRegistry {
    fn default() -> Self {
        Self::embedded_or_empty()
    }
}
