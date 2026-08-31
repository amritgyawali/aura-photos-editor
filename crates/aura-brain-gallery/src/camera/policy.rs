//! `camera_match.toml`: how far a body may be moved, and how much of a photographer's habit
//! survives.
//!
//! Section 9 gives PM "decide default reference-camera policy and how much shooter style may be
//! normalised", so every row here is a product decision with a written reason beside it. The
//! loader's job is to make sure a product decision cannot become a promise the product breaks.
//!
//! ## A ceiling can be lowered by a studio and raised by nobody
//!
//! Phase 21 wrote this rule, phases 22 and 25 inherited it, and it matters more here than in any of
//! the three: a bound in this file governs **every photograph a body shot**, so a widened ceiling
//! is not a slightly bolder edit on one frame, it is a systematic shift across four thousand of
//! them that nobody will notice frame by frame. Every bound is checked against
//! `TransformBound::ceiling`, which lives in the frozen contract, and a file that widens one is
//! **refused outright** rather than clamped. `AURA-ML-5133` is `run_blocking` and `halt`.
//!
//! ## The shooter share is bounded on both sides
//!
//! A share of zero is a matching pass with the shooter half switched off, which is a feature flag
//! rather than a config value somebody sets by accident. A share of one erases a second shooter
//! entirely, which is section 12's second failure mode written as a setting - and it is the failure
//! this phase is most likely to be asked for by somebody who has not thought about it, because
//! "make them match completely" sounds like the goal until the gallery arrives with no second
//! photographer visible in it.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::camera::{
    TransformBound, MAX_CHANNEL_GAIN, MAX_CONTRAST_SHAPE, MAX_PAIR_GAP_MS, MAX_SHOOTER_EV,
    MAX_SHOOTER_SHARE, MAX_T_CCT_K, MAX_T_EXPOSURE_EV, MAX_T_SATURATION, MAX_T_TINT,
    MIN_BACKGROUND_AGREEMENT, MIN_MATCHED_PAIRS, MIN_PAIR_SIMILARITY, SKIN_UV_CAP,
};
use aura_core::{AuraError, SceneId};
use serde::Deserialize;

use super::errors;

/// The table as it ships, compiled in.
///
/// A studio's own file replaces it wholesale rather than merging into it, because a merge makes
/// "what is this build using" a question with two answers. Phase 25's decision, unchanged.
pub const BUNDLED: &str = include_str!("../../config/camera_match.toml");

/// What one scene allows a camera correction to do inside it.
///
/// Invariant 7: no threshold is global. A getting-ready room lit by one window and a dance floor
/// lit by six moving heads are not the same evidence, and a pair found in the second is worth much
/// less than a pair found in the first - so the pairing rule is scene-conditioned even though the
/// transform it feeds is not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenePolicy {
    /// The scene.
    pub scene: SceneId,
    /// Whether frames of this scene may be used as matched-pair evidence.
    ///
    /// False where the light *moves within the scene itself*: a dance floor's illuminant changes
    /// between two frames a second apart, so two bodies shooting it ninety seconds apart were not
    /// in the same light however similar their frames look. A pair from there teaches the solver
    /// that one brand is magenta.
    pub pairable: bool,
    /// How much the backgrounds of a pair in this scene must agree, `0..1`.
    ///
    /// At or above [`MIN_BACKGROUND_AGREEMENT`]; a scene may demand more agreement and never less.
    pub background_agreement: f32,
    /// Whether a shooter's exposure habit is corrected in this scene.
    ///
    /// False where the exposure *is* the photograph: a backlit portrait session and a sparkler exit
    /// are exposed the way they are on purpose, and a median offset measured there is a measurement
    /// of two people's taste rather than of one person's habit.
    pub correct_shooter: bool,
}

impl ScenePolicy {
    /// The row a scene with no policy of its own is judged by.
    ///
    /// The most careful settings in the table: pairs are allowed, agreement is demanded at the
    /// contract's own floor, and the shooter half is off. An unknown scene is a scene nobody argued
    /// about, and the safe direction for an unargued scene is to do less.
    #[must_use]
    pub const fn neutral(scene: SceneId) -> Self {
        Self {
            scene,
            pairable: true,
            background_agreement: MIN_BACKGROUND_AGREEMENT,
            correct_shooter: false,
        }
    }
}

/// The whole table: the seven ceilings, the evidence thresholds and the scenes.
#[derive(Debug, Clone, PartialEq)]
pub struct Matching {
    /// Which table this is. Travels with every row this pass writes.
    pub version: u16,
    /// The seven ceilings, in `TransformBound::ALL` order. Each at or below the contract's own.
    bounds: [f32; TransformBound::COUNT],
    /// How many verified pairs before a solved transform is trusted on its own.
    pub min_pairs: u32,
    /// The widest gap between two frames of a pair, in milliseconds.
    pub max_gap_ms: i64,
    /// The least embedding similarity a candidate pair needs.
    pub min_similarity: f32,
    /// The most of a measured shooter habit that may be corrected, `0..1`.
    pub shooter_share: f32,
    /// The most a shooter-habit correction may contribute, in stops.
    pub shooter_ev: f32,
    /// The scenes, by id.
    scenes: BTreeMap<SceneId, ScenePolicy>,
}

impl Default for Matching {
    fn default() -> Self {
        Self::load(BUNDLED).unwrap_or_else(|_| Self {
            version: 0,
            bounds: [
                MAX_T_CCT_K,
                MAX_T_TINT,
                MAX_T_EXPOSURE_EV,
                MAX_CHANNEL_GAIN,
                MAX_T_SATURATION,
                MAX_CONTRAST_SHAPE,
                SKIN_UV_CAP,
            ],
            min_pairs: MIN_MATCHED_PAIRS,
            max_gap_ms: MAX_PAIR_GAP_MS,
            min_similarity: MIN_PAIR_SIMILARITY,
            shooter_share: MAX_SHOOTER_SHARE,
            shooter_ev: MAX_SHOOTER_EV,
            scenes: BTreeMap::new(),
        })
    }
}

impl Matching {
    /// Load and validate a table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5133` when the file will not parse, when a bound is wider than the contract's own
    /// ceiling, when an evidence threshold is looser than the contract's, when the shooter share is
    /// outside `0.05..=` [`MAX_SHOOTER_SHARE`], or when a scene is not one of the twenty-two.
    pub fn load(text: &str) -> Result<Self, AuraError> {
        let raw: RawTable =
            toml::from_str(text).map_err(|err| errors::policy_refused(err.to_string()))?;

        let mut bounds = [
            MAX_T_CCT_K,
            MAX_T_TINT,
            MAX_T_EXPOSURE_EV,
            MAX_CHANNEL_GAIN,
            MAX_T_SATURATION,
            MAX_CONTRAST_SHAPE,
            SKIN_UV_CAP,
        ];
        for (index, bound) in TransformBound::ALL.into_iter().enumerate() {
            let requested = match bound {
                TransformBound::Cct => raw.bounds.max_cct_k,
                TransformBound::Tint => raw.bounds.max_tint,
                TransformBound::Exposure => raw.bounds.max_exposure_ev,
                TransformBound::ChannelGain => raw.bounds.max_channel_gain,
                TransformBound::Saturation => raw.bounds.max_saturation,
                TransformBound::ContrastShape => raw.bounds.max_contrast_shape,
                TransformBound::Skin => raw.bounds.max_skin_uv,
            };
            let Some(requested) = requested else { continue };
            if !requested.is_finite() || requested <= 0.0 {
                return Err(errors::policy_refused(format!(
                    "bound {bound} must be a positive finite number, found {requested}"
                )));
            }
            if requested > bound.ceiling() {
                return Err(errors::policy_refused(format!(
                    "bound {bound} is {requested}, wider than the contract ceiling {}; a studio \
                     may lower a ceiling and may not raise one",
                    bound.ceiling()
                )));
            }
            if let Some(slot) = bounds.get_mut(index) {
                *slot = requested;
            }
        }

        // Evidence thresholds move in the *opposite* direction from bounds: a studio may demand
        // more evidence and may never demand less, because "fewer pairs are enough" is a way of
        // widening every bound at once without touching one.
        let min_pairs = raw.evidence.min_pairs.unwrap_or(MIN_MATCHED_PAIRS);
        if min_pairs < MIN_MATCHED_PAIRS {
            return Err(errors::policy_refused(format!(
                "evidence.min_pairs is {min_pairs}, below the contract floor {MIN_MATCHED_PAIRS}; \
                 a studio may demand more evidence and may not demand less"
            )));
        }
        let max_gap_ms = raw.evidence.max_gap_ms.unwrap_or(MAX_PAIR_GAP_MS);
        if max_gap_ms <= 0 || max_gap_ms > MAX_PAIR_GAP_MS {
            return Err(errors::policy_refused(format!(
                "evidence.max_gap_ms is {max_gap_ms}, outside 1..={MAX_PAIR_GAP_MS}"
            )));
        }
        let min_similarity = raw.evidence.min_similarity.unwrap_or(MIN_PAIR_SIMILARITY);
        if !(MIN_PAIR_SIMILARITY..=1.0).contains(&min_similarity) {
            return Err(errors::policy_refused(format!(
                "evidence.min_similarity is {min_similarity}, outside \
                 {MIN_PAIR_SIMILARITY}..=1.0"
            )));
        }

        let shooter_share = raw.shooter.share.unwrap_or(MAX_SHOOTER_SHARE);
        if !(0.05..=MAX_SHOOTER_SHARE).contains(&shooter_share) {
            return Err(errors::policy_refused(format!(
                "shooter.share is {shooter_share}, outside 0.05..={MAX_SHOOTER_SHARE}; a share of \
                 zero is a feature flag and a share above the ceiling erases a photographer"
            )));
        }
        let shooter_ev = raw.shooter.max_ev.unwrap_or(MAX_SHOOTER_EV);
        if !(0.0..=MAX_SHOOTER_EV).contains(&shooter_ev) || !shooter_ev.is_finite() {
            return Err(errors::policy_refused(format!(
                "shooter.max_ev is {shooter_ev}, outside 0.0..={MAX_SHOOTER_EV}"
            )));
        }

        let mut scenes = BTreeMap::new();
        for row in raw.scene {
            let scene = SceneId::from_str_or_unknown(&row.scene);
            if scene == SceneId::Unknown && row.scene != SceneId::Unknown.as_str() {
                return Err(errors::policy_refused(format!(
                    "unknown scene '{}' in camera_match.toml",
                    row.scene
                )));
            }
            let agreement = row.background_agreement.unwrap_or(MIN_BACKGROUND_AGREEMENT);
            if !(MIN_BACKGROUND_AGREEMENT..=1.0).contains(&agreement) {
                return Err(errors::policy_refused(format!(
                    "scene {} background_agreement is {agreement}, outside \
                     {MIN_BACKGROUND_AGREEMENT}..=1.0; a scene may demand more agreement and not \
                     less",
                    row.scene
                )));
            }
            scenes.insert(
                scene,
                ScenePolicy {
                    scene,
                    pairable: row.pairable.unwrap_or(true),
                    background_agreement: agreement,
                    correct_shooter: row.correct_shooter.unwrap_or(true),
                },
            );
        }

        Ok(Self {
            version: raw.version,
            bounds,
            min_pairs,
            max_gap_ms,
            min_similarity,
            shooter_share,
            shooter_ev,
            scenes,
        })
    }

    /// Load from a path, falling back on [`BUNDLED`] when the file is not there.
    ///
    /// A *missing* file is the ordinary case - most installations never write one - and a *present
    /// but wrong* file is refused. The two are deliberately different.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5133` when the file exists and will not validate.
    pub fn load_or_bundled(path: &Path) -> Result<Self, AuraError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::load(&text),
            Err(_) => Self::load(BUNDLED),
        }
    }

    /// The ceiling for one axis, in that axis's units.
    ///
    /// Never wider than `TransformBound::ceiling`, which [`Matching::load`] guarantees.
    #[must_use]
    pub fn bound(&self, bound: TransformBound) -> f32 {
        let index = TransformBound::ALL
            .iter()
            .position(|candidate| *candidate == bound)
            .unwrap_or(0);
        self.bounds
            .get(index)
            .copied()
            .unwrap_or_else(|| bound.ceiling())
    }

    /// One scene's policy, or the neutral row.
    ///
    /// Infallible, like `StoryService::profile`: a scene that cannot be looked up takes a wedding's
    /// most careful settings rather than stopping a pass. [`Matching::untargeted`] is how a caller
    /// finds out it happened.
    #[must_use]
    pub fn scene(&self, scene: SceneId) -> ScenePolicy {
        self.scenes
            .get(&scene)
            .copied()
            .unwrap_or(ScenePolicy::neutral(scene))
    }

    /// Every scene with no row of its own, as slugs, in `SceneId::ALL` order.
    ///
    /// What the outline reports. Phase 25's shape, and the reason is the same: a scene nobody
    /// argued about being run on defaults is a fact a product manager should see rather than
    /// discover.
    #[must_use]
    pub fn untargeted(&self) -> Vec<String> {
        SceneId::ALL
            .into_iter()
            .filter(|scene| !self.scenes.contains_key(scene))
            .map(|scene| scene.as_str().to_string())
            .collect()
    }

    /// How many scenes carry a row.
    #[must_use]
    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }

    /// The correction one measured habit justifies under **this** table, in stops.
    ///
    /// The contract's `ShooterBias::correction_for` under the bundled table and tighter under a
    /// studio's own. It is here as well as in the contract because a studio that lowers the share
    /// has to change what the solver does and not only what the ceiling says.
    ///
    /// The sign opposes the habit, exactly as the contract's does: a shooter who works darker gets
    /// a positive correction. See `ShooterBias::applied_ev`.
    #[must_use]
    pub fn shooter_correction(&self, measured_ev: f32) -> f32 {
        use aura_core::contract::camera::SHOOTER_DEADBAND_EV;
        if !measured_ev.is_finite() || measured_ev.abs() < SHOOTER_DEADBAND_EV {
            return 0.0;
        }
        (-measured_ev * self.shooter_share).clamp(-self.shooter_ev, self.shooter_ev)
    }
}

// ---------------------------------------------------------------------------
// The file's own shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawTable {
    version: u16,
    #[serde(default)]
    bounds: RawBounds,
    #[serde(default)]
    evidence: RawEvidence,
    #[serde(default)]
    shooter: RawShooter,
    #[serde(default)]
    scene: Vec<RawScene>,
}

#[derive(Debug, Default, Deserialize)]
struct RawBounds {
    max_cct_k: Option<f32>,
    max_tint: Option<f32>,
    max_exposure_ev: Option<f32>,
    max_channel_gain: Option<f32>,
    max_saturation: Option<f32>,
    max_contrast_shape: Option<f32>,
    max_skin_uv: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawEvidence {
    min_pairs: Option<u32>,
    max_gap_ms: Option<i64>,
    min_similarity: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawShooter {
    share: Option<f32>,
    max_ev: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct RawScene {
    scene: String,
    pairable: Option<bool>,
    background_agreement: Option<f32>,
    correct_shooter: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_table_loads_and_covers_every_scene() {
        let table = Matching::load(BUNDLED).expect("bundled table");
        assert!(table.version >= 1);
        assert!(
            table.untargeted().is_empty(),
            "every scene needs an argued-over row: {:?}",
            table.untargeted()
        );
        assert_eq!(table.scene_count(), SceneId::ALL.len());
    }

    #[test]
    fn every_bundled_bound_is_at_or_below_the_contract_ceiling() {
        let table = Matching::load(BUNDLED).expect("bundled table");
        for bound in TransformBound::ALL {
            assert!(
                table.bound(bound) <= bound.ceiling(),
                "{bound} is wider than the contract"
            );
        }
    }

    #[test]
    fn a_widened_bound_is_refused_rather_than_clamped() {
        let text = format!(
            "version = 1\n[bounds]\nmax_channel_gain = {}\n",
            MAX_CHANNEL_GAIN * 2.0
        );
        let err = Matching::load(&text).expect_err("a widened ceiling must be refused");
        assert_eq!(err.code, errors::CAMERA_POLICY_REFUSED);
    }

    #[test]
    fn a_loosened_evidence_threshold_is_refused_because_it_widens_every_bound_at_once() {
        let text = format!(
            "version = 1\n[evidence]\nmin_pairs = {}\n",
            MIN_MATCHED_PAIRS - 1
        );
        let err = Matching::load(&text).expect_err("less evidence must be refused");
        assert_eq!(err.code, errors::CAMERA_POLICY_REFUSED);
        // The other direction is a studio being more careful, which is always allowed.
        let stricter = format!(
            "version = 1\n[evidence]\nmin_pairs = {}\n",
            MIN_MATCHED_PAIRS + 8
        );
        assert!(Matching::load(&stricter).is_ok());
    }

    #[test]
    fn a_shooter_share_of_zero_or_one_is_refused() {
        for share in ["0.0", "1.0"] {
            let text = format!("version = 1\n[shooter]\nshare = {share}\n");
            let err = Matching::load(&text).expect_err("share {share} must be refused");
            assert_eq!(err.code, errors::CAMERA_POLICY_REFUSED);
        }
    }

    #[test]
    fn an_unknown_scene_is_refused_rather_than_silently_ignored() {
        let text = "version = 1\n[[scene]]\nscene = \"karaoke\"\n";
        let err = Matching::load(text).expect_err("an unknown scene must be refused");
        assert_eq!(err.code, errors::CAMERA_POLICY_REFUSED);
    }

    #[test]
    fn the_shooter_correction_is_a_share_and_never_the_whole_habit() {
        let table = Matching::load(BUNDLED).expect("bundled table");
        let applied = table.shooter_correction(0.5);
        assert!(applied.abs() < 0.5);
        assert!(
            applied < 0.0,
            "a shooter who works brighter is brought down"
        );
        assert_eq!(table.shooter_correction(0.01), 0.0);
    }

    #[test]
    fn the_dance_floor_is_not_pairable_and_the_ceremony_is() {
        let table = Matching::load(BUNDLED).expect("bundled table");
        assert!(!table.scene(SceneId::DanceFloor).pairable);
        assert!(table.scene(SceneId::Ceremony).pairable);
    }
}
