//! How much one photograph is about one person.
//!
//! Every phase from 09 onwards needs a number that says "sharpness on the bride's
//! face outranks sharpness on a stranger's elbow", and this module is where that
//! number comes from. Two things it produces:
//!
//! * **Prominence**, per identity per frame, in `0..1`. Section 6.4's formula.
//! * **[`ImageSubjects::subject_focus_score`]**, one number per frame: the
//!   prominence-weighted sharpness of the faces present.
//!
//! The second one is the important one, and it is worth saying why plainly. A
//! naive pipeline scores a photograph's sharpness globally, which means a frame
//! that is tack sharp on the champagne tower and soft on the bride's face scores
//! well and gets delivered. `subject_focus_score` inverts that: it asks how sharp
//! the photograph is *where it matters*, weighted by who is in it. Phases 09 and 12
//! use it in place of global sharpness, and that substitution is most of what
//! separates this product's culling from a competitor's.
//!
//! ## The weights are a file, and the file has a version
//!
//! Section 6.4 requires that the weights live in a versioned configuration file so
//! QA can tune them without a rebuild, and that every score record which version
//! produced it. So [`ProminenceWeights`] is loaded from
//! `crates/aura-vision/config/prominence.toml`, embedded as the baseline, and
//! overridable per installation. [`ImageSubjects::weights_ver`] carries the version
//! into the catalog and into the explainability ledger.
//!
//! A malformed override is a warning and a fall back to the embedded baseline. It
//! is never a refusal to open a project: a tuning file is a tuning file, and a
//! photographer whose wedding will not open because a QA engineer left a trailing
//! comma is a photographer with a support ticket and a deadline.
//!
//! ## Scene conditioning, and what it means before phase 07
//!
//! Invariant 7 - "no threshold is global; every threshold is a function of the
//! detected scene and subject role" - is satisfied by the scene tables in that
//! file. Phase 07 has not run, so every frame currently resolves to `[base]`, and
//! [`ProminenceWeights::for_scene`] returning the baseline for an unknown scene is
//! the mechanism by which that is *temporary* rather than a design decision. The
//! scene tables are written and tested now so that phase 07 turns them on by
//! supplying a label, with no change here.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::people::{FaceRef, ImageSubjects, Role, SubjectHierarchy};
use aura_core::IdentityId;
use serde::Deserialize;

/// The embedded baseline weights. The file that ships with the build.
const EMBEDDED: &str = include_str!("../../config/prominence.toml");

/// Where an installation may put an override, relative to the catalog directory.
pub const OVERRIDE_RELATIVE_PATH: &str = "config/prominence.toml";

/// The five weights of section 6.4's formula.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct WeightSet {
    /// Weight on `sqrt(area_frac)`.
    pub area: f32,
    /// Weight on centrality.
    pub centrality: f32,
    /// Weight on the face's own sharpness.
    pub sharpness: f32,
    /// Weight on gaze towards the camera.
    pub gaze: f32,
    /// Weight on the identity's role weight.
    pub role: f32,
}

impl WeightSet {
    /// Sum of the five, for the normalisation.
    #[must_use]
    pub fn total(&self) -> f32 {
        self.area + self.centrality + self.sharpness + self.gaze + self.role
    }
}

impl Default for WeightSet {
    /// The baseline from the shipped file, hard-coded as the last resort.
    ///
    /// Reachable only if the embedded file itself fails to parse, which would be a
    /// build-time mistake caught by a test. It exists so that this module has no
    /// path on which it cannot produce a score.
    fn default() -> Self {
        Self {
            area: 0.28,
            centrality: 0.20,
            sharpness: 0.22,
            gaze: 0.10,
            role: 0.20,
        }
    }
}

/// The whole weight configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProminenceWeights {
    /// The version stamped onto every score computed with these weights.
    pub version: u16,
    /// Weights for a frame whose scene is unknown or has no table.
    pub base: WeightSet,
    /// Role weights, by the stable text of [`Role`].
    #[serde(default)]
    pub roles: BTreeMap<String, f32>,
    /// Per-scene overrides.
    #[serde(default)]
    pub scenes: BTreeMap<String, WeightSet>,
}

impl Default for ProminenceWeights {
    fn default() -> Self {
        Self::embedded()
    }
}

impl ProminenceWeights {
    /// The weights that ship with this build.
    ///
    /// # Panics
    ///
    /// Never. A malformed embedded file falls back to [`WeightSet::default`] and
    /// version zero, which a test refuses - so the failure is caught in CI rather
    /// than at a wedding.
    #[must_use]
    pub fn embedded() -> Self {
        toml::from_str(EMBEDDED).unwrap_or(Self {
            version: 0,
            base: WeightSet::default(),
            roles: BTreeMap::new(),
            scenes: BTreeMap::new(),
        })
    }

    /// Load an override, or fall back to the embedded baseline.
    ///
    /// Returns the weights and, when an override was rejected, the reason - so the
    /// caller can log it against a code rather than swallowing it. Invariant 9
    /// applies to configuration too.
    #[must_use]
    pub fn load_or_embedded(path: &Path) -> (Self, Option<String>) {
        if !path.exists() {
            return (Self::embedded(), None);
        }
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Self>(&text) {
                Ok(loaded) if loaded.version > 0 => (loaded, None),
                Ok(_) => (
                    Self::embedded(),
                    Some(format!(
                        "{} has version 0; a weight file must carry a version so a score can \
                         record which weights produced it",
                        path.display()
                    )),
                ),
                Err(err) => (
                    Self::embedded(),
                    Some(format!("{} could not be parsed: {err}", path.display())),
                ),
            },
            Err(err) => (
                Self::embedded(),
                Some(format!("{} could not be read: {err}", path.display())),
            ),
        }
    }

    /// The weights for one scene, or the baseline when the scene has no table.
    #[must_use]
    pub fn for_scene(&self, scene: Option<&str>) -> WeightSet {
        scene
            .and_then(|label| self.scenes.get(label))
            .copied()
            .unwrap_or(self.base)
    }

    /// The weight of one role, before the photographer's importance multiplier.
    ///
    /// An unknown role gets the `unknown` entry, and an absent `unknown` entry gets
    /// 0.2 - low enough that an unrecognised role never outranks a known one, high
    /// enough that it is not invisible.
    #[must_use]
    pub fn role_weight(&self, role: Role) -> f32 {
        self.roles
            .get(role.as_str())
            .or_else(|| self.roles.get(Role::Unknown.as_str()))
            .copied()
            .unwrap_or(0.2)
            .clamp(0.0, 1.0)
    }
}

/// One face's inputs to the prominence formula.
///
/// Separate from [`FaceRef`] because prominence needs two things the subject
/// reference deliberately does not carry: gaze, which is a pose measurement, and
/// the identity's role and importance, which are properties of the *project* rather
/// than of the face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProminenceInput {
    /// Fraction of the frame the face covers, `0..1`.
    pub area_frac: f32,
    /// Centrality, `0..1`.
    pub centrality: f32,
    /// The face's own sharpness, `0..1`.
    pub sharpness: f32,
    /// How much the subject is looking at the lens, `0..1`.
    pub gaze: f32,
    /// The identity's role.
    pub role: Role,
    /// The photographer's importance slider for that identity, `0..1`. The default
    /// is 0.5, which is why the multiplier below is `2 * importance` - so an
    /// untouched slider is exactly a no-op.
    pub importance: f32,
}

/// Gaze towards the camera, from a pose estimate.
///
/// One minus the frontality penalty. It is a proxy and it is labelled as one: a
/// person facing the lens with their eyes closed scores the same as one looking
/// into it, because eye state is phase 09's and expression is phase 10's. Those
/// phases consume these crops and will replace this term with a measurement.
#[must_use]
pub fn gaze_from_pose(pose: crate::face::align::Pose) -> f32 {
    (1.0 - pose.frontality_penalty()).clamp(0.0, 1.0)
}

/// Section 6.4's formula, normalised by the weight sum.
///
/// The importance slider multiplies the role term only, and that is deliberate: a
/// photographer who drags the bride's importance up is saying "this person matters
/// more", not "faces should be bigger". Multiplying the whole score would make an
/// unimportant person's sharp central close-up lose to an important person's blurred
/// edge crop, which is not what the control means.
#[must_use]
pub fn prominence(
    input: &ProminenceInput,
    weights: &ProminenceWeights,
    scene: Option<&str>,
) -> f32 {
    let set = weights.for_scene(scene);
    let total = set.total();
    if total <= f32::EPSILON {
        return 0.0;
    }

    let role_term =
        weights.role_weight(input.role) * (input.importance.clamp(0.0, 1.0) * 2.0).clamp(0.0, 2.0);

    let sum = set.area * input.area_frac.clamp(0.0, 1.0).sqrt()
        + set.centrality * input.centrality.clamp(0.0, 1.0)
        + set.sharpness * input.sharpness.clamp(0.0, 1.0)
        + set.gaze * input.gaze.clamp(0.0, 1.0)
        + set.role * role_term;

    (sum / total).clamp(0.0, 1.0)
}

/// One face, with everything needed to place it in a frame's subject ranking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubjectFace {
    /// The subject reference that goes into [`ImageSubjects::faces`].
    pub reference: FaceRef,
    /// Its prominence inputs.
    pub input: ProminenceInput,
}

/// Build one frame's subject picture.
///
/// Per-identity prominence is the **maximum** over that identity's faces in the
/// frame, not the sum and not the mean. A frame that caught the bride twice - once
/// in a mirror, once directly - is not twice as much about her, and the mean would
/// let a poor reflection drag down a perfect direct look.
///
/// [`ImageSubjects::subject_focus_score`] is the prominence-weighted mean of the
/// faces' sharpness. When there are no faces it is zero, and zero is the honest
/// answer: a photograph of a cake has no subject focus, and a caller that wants to
/// know whether the cake is sharp should read the frame's edge energy from phase 05
/// instead.
#[must_use]
pub fn image_subjects(
    faces: &[SubjectFace],
    hierarchy: &SubjectHierarchy,
    scene: Option<&str>,
    people_count: u32,
    weights: &ProminenceWeights,
) -> ImageSubjects {
    let mut subjects = ImageSubjects {
        faces: faces.iter().map(|face| face.reference).collect(),
        dominant: None,
        prominence: BTreeMap::new(),
        subject_focus_score: 0.0,
        people_count,
        weights_ver: weights.version,
    };

    let mut weighted_sharpness = 0.0f32;
    let mut weight_total = 0.0f32;

    for face in faces {
        let score = prominence(&face.input, weights, scene);
        weighted_sharpness += score * face.reference.sharpness.clamp(0.0, 1.0);
        weight_total += score;

        if let Some(identity) = face.reference.identity_id {
            let slot = subjects.prominence.entry(identity).or_insert(0.0);
            if score > *slot {
                *slot = score;
            }
        }
    }

    if weight_total > f32::EPSILON {
        subjects.subject_focus_score = (weighted_sharpness / weight_total).clamp(0.0, 1.0);
    }

    // The dominant identity, with two tie-breaks that both matter. The project
    // hierarchy first, so a frame where the bride and a guest are equally prominent
    // is about the bride; then the identity id, so two guests at identical
    // prominence resolve the same way on every machine.
    let mut best: Option<(IdentityId, f32, f32)> = None;
    for (identity, score) in &subjects.prominence {
        let candidate = (*identity, *score, hierarchy.weight_of(*identity));
        let better = match best {
            None => true,
            Some((best_id, best_score, best_weight)) => {
                (candidate.1, candidate.2, std::cmp::Reverse(candidate.0))
                    > (best_score, best_weight, std::cmp::Reverse(best_id))
            }
        };
        if better {
            best = Some(candidate);
        }
    }
    subjects.dominant = best.map(|(identity, _, _)| identity);

    subjects
}

/// Build a project's subject hierarchy from per-identity roles and importances.
///
/// The weights map is **total** over the identities supplied, for the reason given
/// on `SubjectHierarchy::weights`: a sparse map forces every caller to invent a
/// default, and nine callers will invent four defaults.
#[must_use]
pub fn subject_hierarchy(
    identities: &[(IdentityId, Role, f32)],
    coverage: f32,
    couple_unconfirmed: bool,
    weights: &ProminenceWeights,
) -> SubjectHierarchy {
    let mut hierarchy = SubjectHierarchy {
        primary: Vec::new(),
        secondary: Vec::new(),
        weights: BTreeMap::new(),
        couple_unconfirmed,
        coverage: coverage.clamp(0.0, 1.0),
    };

    for (identity, role, importance) in identities {
        let weight =
            (weights.role_weight(*role) * (importance.clamp(0.0, 1.0) * 2.0)).clamp(0.0, 1.0);
        hierarchy.weights.insert(*identity, weight);

        if role.is_couple() {
            hierarchy.primary.push(*identity);
        } else if role.is_family() || matches!(role, Role::Vip) || *importance > 0.75 {
            hierarchy.secondary.push(*identity);
        }
    }

    hierarchy.primary.sort_unstable();
    hierarchy.secondary.sort_unstable();
    hierarchy
}
