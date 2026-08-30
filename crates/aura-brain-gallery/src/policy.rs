//! `consistency.toml`: how far anything is allowed to move, and what each scene tolerates.
//!
//! Section 9 gives PM "approve `consistency.toml` damping and bounds; define the 'preserve mood'
//! rule", so every row here is a product decision with a written reason beside it. The loader's
//! job is to make sure a product decision cannot become a promise the product breaks.
//!
//! ## A ceiling can be lowered by a studio and raised by nobody
//!
//! Phase 21 wrote this rule and phase 22 inherited it. Every bound in this file is checked against
//! `Bound::ceiling`, which lives in the frozen contract, and a file that widens one is
//! **refused outright** rather than clamped. `AURA-ML-5129` is `run_blocking` and `halt`, which is
//! the same call phases 21 and 22 made: a file that tries to raise a ceiling is not a file with a
//! typo in it, and falling back on defaults would run the pass under settings nobody chose while a
//! studio believed their own were in force.
//!
//! The damping factor is the one number that is bounded on *both* sides. A damping of zero is a
//! pass that is switched off - which is a feature flag rather than a config value somebody sets by
//! accident - and a damping of one moves every frame onto its target exactly, which is section
//! 12's first failure mode written as a setting.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::gallery::{
    Bound, DAMPING_RANGE, DEFAULT_DAMPING, MAX_D_CCT_K, MAX_D_CONTRAST, MAX_D_EXPOSURE_EV,
    MAX_D_SATURATION, MAX_D_TINT, MIN_ANCHORS, OUTLIER_RESIDUAL, SPLIT_SIGMA,
};
use aura_core::{AuraError, SceneId};
use serde::Deserialize;

use crate::errors;

/// The table as it ships, compiled in.
///
/// A studio's own file replaces it wholesale rather than merging into it, because a merge makes
/// "what is this build using" a question with two answers.
pub const BUNDLED: &str = include_str!("../config/consistency.toml");

/// What one scene tolerates before a frame in it is called inconsistent.
///
/// Invariant 7: no threshold is global. A dance floor varies enormously frame to frame and a
/// family portrait does not, and one tolerance is right for exactly one of them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenePolicy {
    /// The scene.
    pub scene: SceneId,
    /// How much of the distance to the target a frame in this scene moves.
    pub damping: f32,
    /// How far a frame may sit from its node's temperature and still be consistent, in kelvin.
    pub cct_tol: f32,
    /// How far it may sit in tint units.
    pub tint_tol: f32,
    /// How far it may sit in subject luminance, `0..1`.
    pub luma_tol: f32,
    /// Whether the grade half - contrast, saturation, character - is harmonised in this scene.
    ///
    /// False where the *scene's own variation is the point*: a dance floor's contrast is a
    /// property of where the lights were pointing at that instant, and harmonising it makes four
    /// hundred frames of a party look like one long exposure.
    pub harmonise_grade: bool,
    /// Whether skin is corrected in this scene.
    ///
    /// The one axis that is on almost everywhere, because a person's skin is the thing a client
    /// notices drifting. It is off where the light is the subject.
    pub correct_skin: bool,
}

impl ScenePolicy {
    /// The row a scene with no policy of its own is judged by.
    ///
    /// The most careful settings in the table: the smallest damping the range allows, the widest
    /// tolerances, no grade harmonisation. An unknown scene is a scene nobody argued about, and
    /// the safe direction for an unargued scene is to do less. `AURA-ML-5126` says so out loud
    /// rather than letting a neutral row look like a decision.
    #[must_use]
    pub const fn neutral(scene: SceneId) -> Self {
        Self {
            scene,
            damping: DAMPING_RANGE.0,
            cct_tol: 250.0,
            tint_tol: 6.0,
            luma_tol: 0.06,
            harmonise_grade: false,
            correct_skin: true,
        }
    }
}

/// The whole table, plus the bounds every scene shares.
#[derive(Debug, Clone, PartialEq)]
pub struct Consistency {
    /// Which table this is. Travels with every row this pass writes.
    pub version: u16,
    /// The five ceilings, in `Bound::ALL` order. Each at or below the contract's own.
    bounds: [f32; Bound::COUNT],
    /// How many anchors a node aims for, between `MIN_ANCHORS` and `MAX_ANCHORS`.
    pub target_anchors: usize,
    /// The residual, as a fraction of each bound, past which a frame is an outlier.
    pub outlier_residual: f32,
    /// How many within-run spreads a step must exceed before it is a change point.
    pub split_sigma: f32,
    /// The scenes, by id.
    scenes: BTreeMap<SceneId, ScenePolicy>,
}

impl Default for Consistency {
    fn default() -> Self {
        Self::load(BUNDLED).unwrap_or_else(|_| Self {
            version: 0,
            bounds: [
                MAX_D_CCT_K,
                MAX_D_TINT,
                MAX_D_EXPOSURE_EV,
                MAX_D_CONTRAST,
                MAX_D_SATURATION,
            ],
            target_anchors: MIN_ANCHORS,
            outlier_residual: OUTLIER_RESIDUAL,
            split_sigma: SPLIT_SIGMA,
            scenes: BTreeMap::new(),
        })
    }
}

impl Consistency {
    /// Load and validate a table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5129` when the file will not parse, when a bound is wider than the contract's own
    /// ceiling, when a damping factor is outside [`DAMPING_RANGE`], or when a tolerance is
    /// negative.
    pub fn load(text: &str) -> Result<Self, AuraError> {
        let raw: RawTable =
            toml::from_str(text).map_err(|err| errors::policy_refused(err.to_string()))?;

        let mut bounds = [
            MAX_D_CCT_K,
            MAX_D_TINT,
            MAX_D_EXPOSURE_EV,
            MAX_D_CONTRAST,
            MAX_D_SATURATION,
        ];
        for (index, bound) in Bound::ALL.into_iter().enumerate() {
            let requested = match bound {
                Bound::Cct => raw.bounds.max_d_cct_k,
                Bound::Tint => raw.bounds.max_d_tint,
                Bound::Exposure => raw.bounds.max_d_exposure_ev,
                Bound::Contrast => raw.bounds.max_d_contrast,
                Bound::Saturation => raw.bounds.max_d_saturation,
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

        let target_anchors = raw.anchors.target.unwrap_or(4);
        if !(MIN_ANCHORS..=aura_core::contract::gallery::MAX_ANCHORS).contains(&target_anchors) {
            return Err(errors::policy_refused(format!(
                "anchors.target is {target_anchors}, outside {MIN_ANCHORS}..={}",
                aura_core::contract::gallery::MAX_ANCHORS
            )));
        }

        let outlier_residual = raw.outliers.residual.unwrap_or(OUTLIER_RESIDUAL);
        if !(0.05..=1.0).contains(&outlier_residual) {
            return Err(errors::policy_refused(format!(
                "outliers.residual is {outlier_residual}, outside 0.05..=1.0"
            )));
        }

        let split_sigma = raw.changepoint.sigma.unwrap_or(SPLIT_SIGMA);
        if !(1.0..=10.0).contains(&split_sigma) {
            return Err(errors::policy_refused(format!(
                "changepoint.sigma is {split_sigma}, outside 1.0..=10.0"
            )));
        }

        let mut scenes = BTreeMap::new();
        for row in raw.scene {
            let scene = SceneId::from_str_or_unknown(&row.scene);
            if scene == SceneId::Unknown && row.scene != SceneId::Unknown.as_str() {
                return Err(errors::policy_refused(format!(
                    "unknown scene '{}' in consistency.toml",
                    row.scene
                )));
            }
            let damping = row.damping.unwrap_or(DEFAULT_DAMPING);
            if !(DAMPING_RANGE.0..=DAMPING_RANGE.1).contains(&damping) {
                return Err(errors::policy_refused(format!(
                    "scene {} damping is {damping}, outside {:?}",
                    row.scene, DAMPING_RANGE
                )));
            }
            let policy = ScenePolicy {
                scene,
                damping,
                cct_tol: positive(row.cct_tol, 200.0, "cct_tol", &row.scene)?,
                tint_tol: positive(row.tint_tol, 4.0, "tint_tol", &row.scene)?,
                luma_tol: positive(row.luma_tol, 0.05, "luma_tol", &row.scene)?,
                harmonise_grade: row.harmonise_grade.unwrap_or(true),
                correct_skin: row.correct_skin.unwrap_or(true),
            };
            scenes.insert(scene, policy);
        }

        Ok(Self {
            version: raw.version,
            bounds,
            target_anchors,
            outlier_residual,
            split_sigma,
            scenes,
        })
    }

    /// Load from a path, falling back on [`BUNDLED`] when the file is not there.
    ///
    /// A *missing* file is the ordinary case - most installations never write one - and a
    /// *present but wrong* file is refused. The two are deliberately different.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5129` when the file exists and will not validate.
    pub fn load_or_bundled(path: &Path) -> Result<Self, AuraError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::load(&text),
            Err(_) => Self::load(BUNDLED),
        }
    }

    /// The ceiling for one axis, in that axis's units.
    ///
    /// Never wider than `Bound::ceiling`, which `load` guarantees, so a caller may use this and
    /// the contract's own interchangeably when the table is the bundled one.
    #[must_use]
    pub fn bound(&self, bound: Bound) -> f32 {
        let index = Bound::ALL
            .iter()
            .position(|candidate| *candidate == bound)
            .unwrap_or(0);
        self.bounds.get(index).copied().unwrap_or(bound.ceiling())
    }

    /// One scene's policy, or the neutral row.
    ///
    /// Infallible, like `StoryService::profile`: a scene that cannot be looked up takes a wedding
    /// out of the product and a careful default does not. The caller that wants to know it
    /// happened reads [`Consistency::has`] and raises `AURA-ML-5126`.
    #[must_use]
    pub fn scene(&self, scene: SceneId) -> ScenePolicy {
        self.scenes
            .get(&scene)
            .copied()
            .unwrap_or_else(|| ScenePolicy::neutral(scene))
    }

    /// True when the table has a row for this scene.
    #[must_use]
    pub fn has(&self, scene: SceneId) -> bool {
        self.scenes.contains_key(&scene)
    }

    /// How many scenes carry a row.
    #[must_use]
    pub fn len(&self) -> usize {
        self.scenes.len()
    }

    /// True when nothing was loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }

    /// Every scene the table does not cover, as stored slugs, in `SceneId::ALL` order.
    ///
    /// What `GalleryOutline::untargeted_scenes` carries. Phase 15 and 16 both publish this and the
    /// reason is the same: a wedding graded under the neutral row everywhere is a wedding nobody
    /// argued about, and it should be visible rather than inferred from an outcome.
    #[must_use]
    pub fn untargeted(&self) -> Vec<String> {
        SceneId::ALL
            .into_iter()
            .filter(|scene| !self.scenes.contains_key(scene))
            .map(|scene| scene.as_str().to_string())
            .collect()
    }
}

fn positive(value: Option<f32>, default: f32, name: &str, scene: &str) -> Result<f32, AuraError> {
    let value = value.unwrap_or(default);
    if !value.is_finite() || value < 0.0 {
        return Err(errors::policy_refused(format!(
            "scene {scene} {name} is {value}, which must be finite and not negative"
        )));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// The file's shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawTable {
    version: u16,
    #[serde(default)]
    bounds: RawBounds,
    #[serde(default)]
    anchors: RawAnchors,
    #[serde(default)]
    outliers: RawOutliers,
    #[serde(default)]
    changepoint: RawChangepoint,
    #[serde(default)]
    scene: Vec<RawScene>,
}

#[derive(Debug, Default, Deserialize)]
struct RawBounds {
    max_d_cct_k: Option<f32>,
    max_d_tint: Option<f32>,
    max_d_exposure_ev: Option<f32>,
    max_d_contrast: Option<f32>,
    max_d_saturation: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawAnchors {
    target: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct RawOutliers {
    residual: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawChangepoint {
    sigma: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct RawScene {
    scene: String,
    damping: Option<f32>,
    cct_tol: Option<f32>,
    tint_tol: Option<f32>,
    luma_tol: Option<f32>,
    harmonise_grade: Option<bool>,
    correct_skin: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_table_loads_and_covers_every_scene() {
        let table = Consistency::load(BUNDLED).expect("bundled table loads");
        assert!(table.version >= 1);
        assert!(
            table.untargeted().is_empty(),
            "every scene needs an argued-over row: {:?}",
            table.untargeted()
        );
        assert_eq!(table.len(), SceneId::ALL.len());
    }

    #[test]
    fn every_bundled_bound_is_at_or_below_the_contract_ceiling() {
        let table = Consistency::load(BUNDLED).expect("loads");
        for bound in Bound::ALL {
            assert!(
                table.bound(bound) <= bound.ceiling(),
                "{bound} is wider than the contract"
            );
        }
    }

    #[test]
    fn a_file_that_widens_a_bound_is_refused_rather_than_clamped() {
        let text = format!(
            "version = 2\n[bounds]\nmax_d_cct_k = {}\n",
            MAX_D_CCT_K * 2.0
        );
        let err = Consistency::load(&text).expect_err("a widened bound is refused");
        assert_eq!(err.code, errors::GALLERY_POLICY_REFUSED);
        assert!(err.detail.contains("may lower a ceiling"), "{}", err.detail);
    }

    #[test]
    fn a_file_that_narrows_a_bound_is_accepted() {
        let text = "version = 3\n[bounds]\nmax_d_cct_k = 200.0\n";
        let table = Consistency::load(text).expect("a narrowed bound is a studio's business");
        assert!((table.bound(Bound::Cct) - 200.0).abs() < 1e-6);
    }

    #[test]
    fn damping_is_bounded_on_both_sides() {
        for damping in [0.0_f32, 1.0] {
            let text =
                format!("version = 1\n[[scene]]\nscene = \"ceremony\"\ndamping = {damping}\n");
            let err = Consistency::load(&text).expect_err("{damping} is outside the range");
            assert_eq!(err.code, errors::GALLERY_POLICY_REFUSED);
        }
    }

    #[test]
    fn an_unknown_scene_name_is_refused_rather_than_folded_into_unknown() {
        let text = "version = 1\n[[scene]]\nscene = \"reception_afterparty\"\n";
        let err = Consistency::load(text).expect_err("a typo is not a scene");
        assert!(err.detail.contains("unknown scene"), "{}", err.detail);
    }

    #[test]
    fn a_missing_scene_falls_back_on_the_most_careful_row() {
        let table = Consistency::load("version = 1\n").expect("an empty table is legal");
        let neutral = table.scene(SceneId::DanceFloor);
        assert!(!table.has(SceneId::DanceFloor));
        assert_eq!(neutral.damping, DAMPING_RANGE.0);
        assert!(!neutral.harmonise_grade);
    }

    #[test]
    fn the_dance_floor_does_not_have_its_grade_harmonised() {
        let table = Consistency::load(BUNDLED).expect("loads");
        assert!(
            !table.scene(SceneId::DanceFloor).harmonise_grade,
            "a dance floor's contrast is where the lights were pointing"
        );
        assert!(
            table.scene(SceneId::FamilyPortrait).harmonise_grade,
            "a family portrait session is one look"
        );
    }
}
