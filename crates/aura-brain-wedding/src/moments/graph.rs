//! The similarity graph, its scene-aware thresholds, and the union-find over it.
//!
//! PHASE-08 section 6.2. Three things happen here and they are in this order for a
//! reason:
//!
//! 1. **Candidate edges** come from a time-windowed sweep, never from all pairs. A
//!    4,000-frame wedding has eight million pairs and about sixty thousand candidates;
//!    section 11's six-second budget is only reachable because the second number is the
//!    one that gets scored.
//! 2. **Each candidate is scored** by section 6.2's four-term sum, and compared against
//!    a threshold that comes from the scene - the mechanism invariant 7 asks for, in
//!    the one place in this phase where a threshold is applied.
//! 3. **Union-find** turns the surviving edges into groups, and an over-large group is
//!    re-clustered at a stricter threshold rather than accepted.
//!
//! ## The edge score, and why the embedding cannot carry a pair alone
//!
//! ```text
//! score = ( 0.55 x (1 - embed_dist)      visual similarity
//!         + 0.20 x (1 - dhash_norm)      pixel-level near-identity
//!         + 0.15 x identity_overlap      the same people are in both
//!         + 0.10 x same_camera )         the same body took both
//!       x proximity                      how tight this pair is for this cadence
//!       - scene_penalty                  the two frames are of different things
//! ```
//!
//! Section 6.2's four weights, unchanged. The multiplier and the penalty are additions
//! recorded in ADR-0017 section 3; see [`PROXIMITY_DECAY`] for the failure that made
//! the first one necessary and what the evaluation harness found without it.
//!
//! The property worth stating is what the first weight is *not* big enough to do: 0.55
//! is below every threshold in `moment_profiles.toml`, so two frames that are visually
//! identical but share no people and came off different bodies do **not** group on
//! appearance alone. That is deliberate and it is the direction section 12's first
//! failure mode says to fail in - a merge hides a keeper, and the photographer never
//! learns which one.
//!
//! ## Cross-camera edges, and the pass that is not this one
//!
//! A candidate pair from two different bodies is scored without the `same_camera`
//! term, so it needs materially stronger appearance and identity evidence to clear the
//! same threshold. Two shooters standing beside each other during the kiss produce
//! exactly that, and they group here.
//!
//! Two shooters at opposite ends of the aisle do not, and they are not meant to at this
//! level: their *frames* are not the same photograph. [`super::merge`] joins their
//! *moments* on temporal overlap and medoid proximity, which is section 6.2's last
//! paragraph, and it keeps the per-camera sub-groups separable so a bad merge is
//! recoverable. Two mechanisms, two kinds of evidence, and section 12's third failure
//! mode is why the second one is conservative.
//!
//! ## What is not decided here
//!
//! Nothing about a photograph's fate. A group is a set of frames that belong together;
//! which of them a client sees is phase 12's, and this module has no opinion.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use aura_core::contract::moment::{ImageId, Timestamp};
use aura_core::{AuraError, IdentityId, SceneId};
use serde::Deserialize;

use crate::errors;
use crate::moments::cadence::{Cadence, Drive, Shot};

/// The baseline threshold table that ships with the build.
const EMBEDDED: &str = include_str!("../../config/moment_profiles.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "moment_profiles.toml";

/// The fewest characters a rationale may have.
///
/// Nine, the same as `scene::profile::MIN_RATIONALE`. Two files, one rule, and a test
/// asserts the two constants agree.
pub const MIN_RATIONALE: usize = 9;

/// Section 6.2's weight on visual similarity.
pub const W_EMBED: f32 = 0.55;
/// Section 6.2's weight on difference-hash proximity.
pub const W_DHASH: f32 = 0.20;
/// Section 6.2's weight on shared identities.
pub const W_IDENTITY: f32 = 0.15;
/// Section 6.2's weight on a shared camera body.
pub const W_CAMERA: f32 = 0.10;

/// How much of a pair's score a full window's separation costs it.
///
/// **Section 2.1 lists time proximity first among the grouping signals and section
/// 6.2's four-term score has no time term at all.** This closes that gap, and it is
/// recorded as a deviation in ADR-0017 section 3 rather than slipped in.
///
/// The four weights are untouched: the sum is multiplied by
///
/// ```text
/// proximity = 1 - PROXIMITY_DECAY x (gap / window)
/// ```
///
/// so a pair at the very edge of its window keeps 80 % of whatever visual evidence it
/// has, and a pair inside a burst keeps essentially all of it. The ablation section 9
/// asks MLR for still reports the four terms exactly as the phase document writes them.
///
/// ### Why it is needed, in one paragraph
///
/// Without it, the window is a *gate* and nothing more: two frames either are or are
/// not candidates, and among candidates a pair 100 ms apart and a pair 8 s apart are
/// judged identically. A ceremony shot at one frame every eight seconds then chains
/// into a single moment for as long as the photographer keeps shooting, because every
/// consecutive pair is inside the eight-second clamp and every consecutive pair looks
/// alike - the altar has not moved. `slow_ceremony` in
/// `aura_brain_wedding::fixtures::bursts` is that case, and the evaluation harness
/// found it: one moment where a photographer counts six.
///
/// The clamp is what makes this discriminating rather than arbitrary. A burst's window
/// is pinned at the 0.7 s floor while its gaps are 100 ms, so a burst pair sits at
/// `gap/window` around 0.14; slow shooting is pinned at the 8 s ceiling while its gaps
/// are 8 s, so a slow pair sits at 1.0. The ratio measures *how tight this pair is
/// relative to how tight this photographer's shooting could get*, which is exactly what
/// "time proximity (adaptive)" means.
pub const PROXIMITY_DECAY: f32 = 0.20;

/// Subtracted from a pair whose two frames were classified as different scenes.
///
/// Section 6.2's "scene mismatch penalty". Small, because the scene labels are
/// themselves smoothed posteriors and a boundary frame legitimately gets the label of
/// the chapter it is heading into - the penalty is meant to break ties at a chapter
/// edge, not to override the visual evidence.
///
/// Applied only when both frames carry a known label. An unclassified wedding pays no
/// penalty at all, which is what lets phase 08 run before phase 07 on a fresh import.
pub const SCENE_MISMATCH_PENALTY: f32 = 0.06;

/// Added to the threshold when an over-large group is re-clustered.
///
/// Section 6.2: "split over-large groups (> 60 frames) by re-clustering with a stricter
/// threshold". One step is usually enough; the pass repeats up to
/// [`MAX_RECLUSTER_ROUNDS`] times and then splits on time, so the cap is a guarantee
/// rather than an aspiration.
pub const STRICTER_STEP: f32 = 0.08;

/// How many times an over-large group may be re-clustered before it is cut on time.
///
/// Three. Beyond that the frames really are all alike - a static camera on a tripod
/// during a long speech - and raising the threshold further would produce singletons
/// rather than moments, which is the opposite failure.
pub const MAX_RECLUSTER_ROUNDS: usize = 3;

/// The most forward neighbours one frame proposes as candidates.
///
/// The linearity guarantee section 9's PERF task asks for. A 10 fps burst inside an
/// 8-second window is eighty frames, and every one of them is examined; beyond that the
/// pairs are between frames the union-find will connect transitively anyway, so the cap
/// costs nothing and bounds the worst case - a photographer who left the shutter down
/// for a minute - at `frames x 64` rather than at `frames^2`.
pub const MAX_NEIGHBOURS: usize = 64;

/// The band of mean moment sizes that does not raise `AURA-ML-5030`.
///
/// Section 0 wants 3,000 files to become 700-1,100 moments, which is 2.7 to 4.3 frames
/// each. This band is much wider - `1.0` to `12.0` - on purpose: a photographer who
/// shoots one deliberate frame per pose genuinely has one frame per moment, and a
/// sports-style shooter genuinely has twelve. What the code can honestly say is that
/// something outside this band is worth telling somebody about.
pub const PLAUSIBLE_MEAN_SIZE: (f32, f32) = (1.0, 12.0);

/// Which grouping implementation produced a moment.
///
/// Written into `moments.group_ver`. Bump it on any change to the scoring, the
/// candidate generation, the union-find or the re-cluster pass - anything that would
/// make this build and the previous one disagree about the same wedding.
pub const GROUP_VER: u16 = 1;

// ---------------------------------------------------------------------------
// The threshold table
// ---------------------------------------------------------------------------

/// The three grouping knobs one scene is judged by.
///
/// Deliberately **not** part of the frozen `SceneProfile`: ten later phases read that
/// type and none of them uses a grouping threshold. See ADR-0017 section 4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MomentProfile {
    /// The score two frames must reach to be the same moment, `0..1`. Higher is
    /// stricter.
    pub edge_threshold: f32,
    /// Multiplier on the adaptive cadence window.
    pub window_scale: f32,
    /// Frames, before the over-large split pass re-clusters.
    pub max_group: usize,
}

impl MomentProfile {
    /// The profile a scene with no row gets.
    ///
    /// Not a refusal, for `SceneProfile::neutral`'s reason: a scene with no grouping
    /// profile must still group, or one missing config row takes a wedding out of the
    /// product. Overwritten by the file's `[defaults]` block whenever one loads, which
    /// it always does in a shipped build.
    pub const NEUTRAL: Self = Self {
        edge_threshold: 0.68,
        window_scale: 1.0,
        max_group: 60,
    };
}

/// One authored profile.
#[derive(Debug, Clone, Deserialize)]
struct ProfileFile {
    edge_threshold: f32,
    window_scale: f32,
    max_group: usize,
    rationale: String,
}

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
struct RegistryFile {
    version: u16,
    defaults: ProfileFile,
    #[serde(default)]
    scenes: BTreeMap<String, ProfileFile>,
}

/// Scene-conditioned grouping thresholds, with their rationales.
#[derive(Debug, Clone)]
pub struct MomentProfileRegistry {
    default: MomentProfile,
    default_rationale: String,
    scenes: BTreeMap<SceneId, MomentProfile>,
    rationales: BTreeMap<SceneId, String>,
    version: u16,
}

impl Default for MomentProfileRegistry {
    fn default() -> Self {
        Self {
            default: MomentProfile::NEUTRAL,
            default_rationale: "compiled-in neutral".to_string(),
            scenes: BTreeMap::new(),
            rationales: BTreeMap::new(),
            version: 0,
        }
    }
}

impl MomentProfileRegistry {
    /// Load the shipped baseline.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5031` when the embedded file is malformed, which would be a build bug
    /// rather than a deployment one - and is why `tests/config.rs` loads it too.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse("moment_profiles.toml (embedded)", EMBEDDED)
    }

    /// Load an installation override, or fall back to the baseline.
    ///
    /// A malformed override is not fatal; a malformed baseline is. The same asymmetry
    /// `SceneProfileRegistry::load_or_embedded` has, for the same reason: the baseline
    /// is known good, and falling back to it keeps the product open.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5031` only when the embedded baseline itself will not load.
    pub fn load_or_embedded(directory: &Path) -> Result<(Self, Option<AuraError>), AuraError> {
        let path = directory.join(OVERRIDE_FILE);
        if !path.exists() {
            return Ok((Self::embedded()?, None));
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            let refusal = errors::moment_config_refused(
                &path.display().to_string(),
                "file",
                "could not be read; the shipped thresholds are in use",
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
    /// `AURA-ML-5031`, naming the file, the key and the rule.
    pub fn parse(name: &str, text: &str) -> Result<Self, AuraError> {
        let parsed: RegistryFile = toml::from_str(text).map_err(|err| {
            errors::moment_config_refused(name, "file", &format!("is not valid TOML: {err}"))
        })?;

        let default = validate(name, "defaults", &parsed.defaults)?;
        let mut scenes = BTreeMap::new();
        let mut rationales = BTreeMap::new();

        for (key, authored) in parsed.scenes {
            let scene = SceneId::from_str_or_unknown(&key);
            if scene == SceneId::Unknown {
                return Err(errors::moment_config_refused(
                    name,
                    &key,
                    "is not one of the 22 scenes in `SceneId`; the abstention takes the defaults",
                ));
            }
            scenes.insert(scene, validate(name, &key, &authored)?);
            rationales.insert(scene, authored.rationale.trim().to_string());
        }

        Ok(Self {
            default,
            default_rationale: parsed.defaults.rationale.trim().to_string(),
            scenes,
            rationales,
            version: parsed.version,
        })
    }

    /// The profile for a scene. Infallible; an unlisted scene takes the defaults.
    ///
    /// No `AURA-ML-5023`-style warning here, unlike the scene registry, because an
    /// unlisted scene is the *expected* case: the file names ten scenes and takes the
    /// default for the other twelve, deliberately, so that a reader can see which
    /// numbers were actually argued over.
    #[must_use]
    pub fn get(&self, scene: SceneId) -> MomentProfile {
        self.scenes.get(&scene).copied().unwrap_or(self.default)
    }

    /// The sentence explaining a scene's numbers, or the default's.
    #[must_use]
    pub fn rationale(&self, scene: SceneId) -> &str {
        self.rationales
            .get(&scene)
            .map_or(self.default_rationale.as_str(), String::as_str)
    }

    /// The table's version, written into `moments.profile_ver`.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many scenes carry an override.
    #[must_use]
    pub fn overrides(&self) -> usize {
        self.scenes.len()
    }

    /// Every scene that carries an override, in `SceneId::ALL` order.
    #[must_use]
    pub fn overridden(&self) -> Vec<SceneId> {
        self.scenes.keys().copied().collect()
    }
}

/// Every rule a moment profile must satisfy, in the order somebody would fix them.
fn validate(file: &str, key: &str, authored: &ProfileFile) -> Result<MomentProfile, AuraError> {
    if authored.rationale.trim().len() < MIN_RATIONALE {
        return Err(errors::moment_config_refused(
            file,
            key,
            "has no rationale; a threshold nobody can explain does not load. Say why a \
             photographer would agree with this number, not what the number is",
        ));
    }
    if !(0.0..=1.0).contains(&authored.edge_threshold) {
        return Err(errors::moment_config_refused(
            file,
            key,
            "has an edge_threshold outside 0..1",
        ));
    }
    // The maximum score two frames can reach without sharing an identity is
    // `W_EMBED + W_DHASH + W_CAMERA`. A threshold above that would mean no two frames
    // of a detail shot could ever group, however identical - which is a table that
    // silently disables grouping for a scene, and is exactly the kind of half-broken
    // configuration `AURA-ML-5031` exists to refuse rather than to survive.
    let reachable_without_faces = W_EMBED + W_DHASH + W_CAMERA;
    if authored.edge_threshold > reachable_without_faces {
        return Err(errors::moment_config_refused(
            file,
            key,
            &format!(
                "has edge_threshold = {:.2}, above the {reachable_without_faces:.2} two frames \
                 with no shared faces can reach; nothing in this scene would ever group",
                authored.edge_threshold
            ),
        ));
    }
    if !(0.25..=4.0).contains(&authored.window_scale) {
        return Err(errors::moment_config_refused(
            file,
            key,
            "has a window_scale outside 0.25..4.0",
        ));
    }
    if authored.max_group < 2 || authored.max_group > 500 {
        return Err(errors::moment_config_refused(
            file,
            key,
            "has a max_group outside 2..500",
        ));
    }
    Ok(MomentProfile {
        edge_threshold: authored.edge_threshold,
        window_scale: authored.window_scale,
        max_group: authored.max_group,
    })
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// Everything the grouping pass knows about one photograph.
///
/// Assembled once per project from the catalog - the embedding and hash from phase 05,
/// the identities from phase 06's assignment, the scene from phase 07 - and then read
/// many times. Nothing here is computed from pixels: this phase opens no image files,
/// which is what section 11's budget assumes and what makes it a pass over rows.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The photograph.
    pub image: ImageId,
    /// Timeline time, milliseconds since the Unix epoch.
    pub ts: Timestamp,
    /// Dense camera handle from [`super::cadence::CameraTable`].
    pub camera: u32,
    /// What the camera said about the drive.
    pub drive: Drive,
    /// The L2-normalised phase 05 embedding, widened to single precision once.
    pub embedding: Vec<f32>,
    /// The 64-bit difference hash.
    pub dhash: u64,
    /// Who `PeopleService` assigned to this frame. Empty when no face pass has run.
    pub identities: BTreeSet<IdentityId>,
    /// What phase 07 said this frame is of.
    pub scene: SceneId,
    /// True when phase 07 has classified this frame at all.
    ///
    /// Distinct from `scene == Unknown`, which is a classifier that ran and abstained.
    /// The scene-mismatch penalty needs to tell those apart: an abstention is not
    /// evidence that two frames are different things.
    pub classified: bool,
    /// Mean gradient magnitude from phase 05's descriptors, `0..1`.
    ///
    /// Half of `keep_hint`'s "technically strongest frame by edge energy and subject
    /// focus". Not a quality score - phase 09 owns that - and used only to break ties
    /// inside a set of frames that are already known to be near identical.
    pub edge_energy: f32,
    /// Phase 06's prominence-weighted face sharpness, `0..1`. Zero when no face pass
    /// has run, which sends `keep_hint` to edge energy alone and says so in a reason.
    pub subject_focus: f32,
}

impl Frame {
    /// The shot this frame is, for the cadence estimator.
    #[must_use]
    pub fn shot(&self) -> Shot {
        Shot {
            image: self.image,
            ts: self.ts,
            camera: self.camera,
            drive: self.drive,
        }
    }
}

/// One scored candidate pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    /// Index of the earlier frame.
    pub left: usize,
    /// Index of the later frame.
    pub right: usize,
    /// Section 6.2's four-term score, after any scene-mismatch penalty. `0..1`.
    pub score: f32,
    /// Cosine distance between the two embeddings, `0..2`.
    pub embed_dist: f32,
    /// Hamming distance between the two difference hashes, `0..=64`.
    pub dhash_dist: u32,
    /// True when the camera itself said the two frames were one release.
    pub drive_evidence: bool,
    /// True when the two frames came off different bodies.
    pub cross_camera: bool,
}

/// The four terms of section 6.2's score, for one pair.
///
/// Returned rather than summed inline so that the ablation section 9 asks MLR for -
/// "ablate each signal's contribution" - is a call rather than a rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeTerms {
    /// `W_EMBED x (1 - embed_dist)`, clamped at zero.
    pub embed: f32,
    /// `W_DHASH x (1 - hamming/64)`.
    pub dhash: f32,
    /// `W_IDENTITY x` the Jaccard overlap of the two identity sets.
    pub identity: f32,
    /// `W_CAMERA`, or zero across bodies.
    pub camera: f32,
    /// The scene-mismatch penalty, as a negative number or zero.
    pub scene_penalty: f32,
    /// The time-proximity multiplier, `1 - PROXIMITY_DECAY x (gap / window)`.
    ///
    /// One when the gap is unknown, which is what [`score_pair`] returns: the four
    /// weighted terms are a property of the pair, and the proximity is a property of
    /// the pair *and* the local cadence, which only [`candidates`] knows.
    pub proximity: f32,
}

impl EdgeTerms {
    /// The four terms, scaled by proximity, plus the scene penalty. `0..1`.
    ///
    /// The penalty is applied **after** the multiplier rather than inside it, because a
    /// scene mismatch is evidence about the two frames and not about the cadence:
    /// scaling it would make a mismatch cost less at the edge of the window, which is
    /// backwards.
    #[must_use]
    pub fn total(&self) -> f32 {
        let weighted = self.embed + self.dhash + self.identity + self.camera;
        (weighted * self.proximity + self.scene_penalty).clamp(0.0, 1.0)
    }
}

/// The proximity multiplier for a gap inside a window.
///
/// Clamped at both ends: a gap of zero keeps everything, and a gap at or beyond the
/// window keeps `1 - PROXIMITY_DECAY`. A window of zero - which the cadence estimator
/// cannot produce, since [`super::cadence::MIN_WINDOW_MS`] is 700 - returns the floor
/// rather than dividing by zero.
#[must_use]
pub fn proximity_factor(gap_ms: i64, window_ms: i64) -> f32 {
    if window_ms <= 0 {
        return 1.0 - PROXIMITY_DECAY;
    }
    let ratio = (gap_ms.max(0) as f32 / window_ms as f32).clamp(0.0, 1.0);
    1.0 - PROXIMITY_DECAY * ratio
}

/// Score one pair of frames.
///
/// Public because the eval harness ablates it and the debug panel explains it.
#[must_use]
pub fn score_pair(left: &Frame, right: &Frame) -> EdgeTerms {
    let embed_dist =
        aura_index::contract::index::cosine_distance(&left.embedding, &right.embedding);
    let dhash_dist = (left.dhash ^ right.dhash).count_ones();

    let shared = left.identities.intersection(&right.identities).count();
    let union = left.identities.union(&right.identities).count();
    // Jaccard, and zero when neither frame has an assigned identity. Zero rather than
    // one: "no faces in either frame" is not evidence that two frames are the same
    // moment, and treating an unscanned wedding as full agreement would make phase 08's
    // result depend on whether phase 06 had run - which is precisely what the
    // `classified`/`identities` split exists to avoid.
    let identity_overlap = if union == 0 {
        0.0
    } else {
        shared as f32 / union as f32
    };

    let scene_penalty = if left.classified
        && right.classified
        && left.scene != right.scene
        && left.scene != SceneId::Unknown
        && right.scene != SceneId::Unknown
    {
        -SCENE_MISMATCH_PENALTY
    } else {
        0.0
    };

    EdgeTerms {
        embed: W_EMBED * (1.0 - embed_dist).clamp(0.0, 1.0),
        dhash: W_DHASH * (1.0 - dhash_dist as f32 / 64.0),
        identity: W_IDENTITY * identity_overlap,
        camera: if left.camera == right.camera {
            W_CAMERA
        } else {
            0.0
        },
        scene_penalty,
        // Unknown here; `candidates` fills it in from the local cadence.
        proximity: 1.0,
    }
}

/// Build the candidate edges of one project.
///
/// `frames` must be sorted by `(ts, image)`; the caller does that once when it loads
/// them, because that is also the order every other pass in the product walks a
/// wedding in.
///
/// The sweep is forward-only and bounded twice: by the adaptive window, and by
/// [`MAX_NEIGHBOURS`]. Both bounds are on the *candidate* count rather than on the
/// result, so the cost is predictable from the frame count and the cadence rather than
/// from how similar the wedding happens to be.
#[must_use]
pub fn candidates(
    frames: &[Frame],
    cadence: &Cadence,
    profiles: &MomentProfileRegistry,
) -> Vec<Edge> {
    let mut edges = Vec::new();

    for (index, left) in frames.iter().enumerate() {
        let profile = profiles.get(left.scene);
        let window = (cadence.window_ms(left.camera, left.ts) as f32 * profile.window_scale) as i64;
        let reach = window.clamp(0, super::cadence::HARD_GAP_MS);

        // `enumerate` before `skip`, so `position` is the absolute index the union-find
        // works in rather than an offset from `index`.
        for (position, right) in frames
            .iter()
            .enumerate()
            .skip(index + 1)
            .take(MAX_NEIGHBOURS)
        {
            let gap = right.ts - left.ts;
            if gap > reach {
                break;
            }
            // The hard gap is checked again rather than trusted to `reach`, because a
            // profile's `window_scale` could in principle push a window past it and
            // section 6.1's rule is unconditional.
            if Cadence::is_hard_gap(gap) {
                break;
            }

            let mut terms = score_pair(left, right);
            terms.proximity = proximity_factor(gap, reach);
            let score = terms.total();
            let threshold = profile
                .edge_threshold
                .max(profiles.get(right.scene).edge_threshold);
            let drive_evidence = Cadence::drive_pair(&left.shot(), &right.shot());

            // Drive evidence is section 6.1's "strong evidence rather than inferring",
            // and this is where it is stronger than the threshold: two frames the body
            // itself recorded as one continuous release are one burst, and no scene's
            // threshold overrules the camera about what the camera did. The score is
            // still carried, so a reviewer can see how close it was.
            if score >= threshold || drive_evidence {
                edges.push(Edge {
                    left: index,
                    right: position,
                    score,
                    embed_dist: aura_index::contract::index::cosine_distance(
                        &left.embedding,
                        &right.embedding,
                    ),
                    dhash_dist: (left.dhash ^ right.dhash).count_ones(),
                    drive_evidence,
                    cross_camera: left.camera != right.camera,
                });
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Union-find
// ---------------------------------------------------------------------------

/// Disjoint sets over frame indices, with union by size and path halving.
///
/// Deterministic: ties are broken by the smaller index, so two machines that see the
/// same edges in the same order produce the same partition. Invariant 4.
#[derive(Debug, Clone)]
pub struct UnionFind {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl UnionFind {
    /// A partition of `n` singletons.
    #[must_use]
    pub fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    /// The representative of `index`.
    pub fn find(&mut self, index: usize) -> usize {
        let mut current = index;
        while self.parent.get(current).copied().unwrap_or(current) != current {
            let grandparent = self
                .parent
                .get(self.parent.get(current).copied().unwrap_or(current))
                .copied()
                .unwrap_or(current);
            if let Some(slot) = self.parent.get_mut(current) {
                *slot = grandparent;
            }
            current = grandparent;
        }
        current
    }

    /// Join two sets. Returns false when they were already one.
    pub fn union(&mut self, a: usize, b: usize) -> bool {
        let (mut left, mut right) = (self.find(a), self.find(b));
        if left == right {
            return false;
        }
        let (left_size, right_size) = (
            self.size.get(left).copied().unwrap_or(1),
            self.size.get(right).copied().unwrap_or(1),
        );
        // Union by size, and by the smaller index when the sizes are equal. The second
        // half is what makes this deterministic rather than merely balanced.
        if right_size > left_size || (right_size == left_size && right < left) {
            std::mem::swap(&mut left, &mut right);
        }
        if let Some(slot) = self.parent.get_mut(right) {
            *slot = left;
        }
        if let Some(slot) = self.size.get_mut(left) {
            *slot = left_size + right_size;
        }
        true
    }

    /// The groups, each sorted ascending, ordered by their first member.
    #[must_use]
    pub fn groups(mut self) -> Vec<Vec<usize>> {
        let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for index in 0..self.parent.len() {
            by_root.entry(self.find(index)).or_default().push(index);
        }
        let mut out: Vec<Vec<usize>> = by_root.into_values().collect();
        out.sort_by_key(|group| group.first().copied().unwrap_or(usize::MAX));
        out
    }
}

/// Turn scored edges into groups, splitting any that come out too large.
///
/// The over-large pass is section 6.2's, and it is a re-cluster rather than a cut:
/// raising the threshold on the same edges keeps the tight sub-structure of a long
/// dance sequence and drops the weak links between its parts, which a time-based cut
/// would not. Only after [`MAX_RECLUSTER_ROUNDS`] does it fall back to cutting at the
/// widest internal gap, so the size cap is a guarantee.
#[must_use]
pub fn group(
    frames: &[Frame],
    edges: &[Edge],
    profiles: &MomentProfileRegistry,
) -> Vec<Vec<usize>> {
    let mut sets = UnionFind::new(frames.len());
    for edge in edges {
        sets.union(edge.left, edge.right);
    }

    let mut out = Vec::new();
    for candidate in sets.groups() {
        let cap = candidate
            .iter()
            .filter_map(|index| frames.get(*index))
            .map(|frame| profiles.get(frame.scene).max_group)
            .min()
            .unwrap_or(MomentProfile::NEUTRAL.max_group);
        split_oversized(&candidate, edges, cap, 0, &mut out);
    }
    out.sort_by_key(|group| group.first().copied().unwrap_or(usize::MAX));
    out
}

/// Re-cluster one group until it fits the cap, then cut it if it still does not.
fn split_oversized(
    group: &[usize],
    edges: &[Edge],
    cap: usize,
    round: usize,
    out: &mut Vec<Vec<usize>>,
) {
    if group.len() <= cap {
        out.push(group.to_vec());
        return;
    }
    if round >= MAX_RECLUSTER_ROUNDS {
        cut_on_widest_gap(group, cap, out);
        return;
    }

    // Re-cluster: the same edges, a stricter bar. The bar is measured from the group's
    // own weakest surviving edge rather than from the profile, because a group that
    // reached this size did so through edges that all cleared the profile already.
    let members: BTreeSet<usize> = group.iter().copied().collect();
    let inner: Vec<&Edge> = edges
        .iter()
        .filter(|edge| members.contains(&edge.left) && members.contains(&edge.right))
        .collect();
    let weakest = inner
        .iter()
        .map(|edge| edge.score)
        .fold(f32::INFINITY, f32::min);
    let raised = if weakest.is_finite() {
        weakest + STRICTER_STEP
    } else {
        1.0
    };

    let mut sets = UnionFind::new(group.len());
    let position: BTreeMap<usize, usize> = group
        .iter()
        .enumerate()
        .map(|(local, global)| (*global, local))
        .collect();
    let mut joined = 0usize;
    for edge in &inner {
        // Drive evidence survives every re-cluster round. The camera recorded that the
        // shutter was held down; no threshold this pass invents is better evidence than
        // that, and breaking a real burst apart is the failure a photographer notices
        // first.
        if edge.score < raised && !edge.drive_evidence {
            continue;
        }
        if let (Some(left), Some(right)) = (position.get(&edge.left), position.get(&edge.right)) {
            if sets.union(*left, *right) {
                joined += 1;
            }
        }
    }
    // Nothing was separated, so another round would do nothing either.
    if joined + 1 >= group.len() {
        cut_on_widest_gap(group, cap, out);
        return;
    }

    for local_group in sets.groups() {
        let mapped: Vec<usize> = local_group
            .iter()
            .filter_map(|local| group.get(*local).copied())
            .collect();
        if mapped.len() == group.len() {
            cut_on_widest_gap(group, cap, out);
            return;
        }
        split_oversized(&mapped, edges, cap, round + 1, out);
    }
}

/// Cut a group into cap-sized runs at its widest internal gaps.
///
/// The last resort, and it is honest about what it is: a group that survived three
/// re-clusterings really is a run of frames that all look alike, and the only remaining
/// question is where a photographer would least mind the break. The widest gap is that
/// place.
fn cut_on_widest_gap(group: &[usize], cap: usize, out: &mut Vec<Vec<usize>>) {
    if group.len() <= cap {
        out.push(group.to_vec());
        return;
    }
    let pieces = group.len().div_ceil(cap.max(1));
    let each = group.len().div_ceil(pieces.max(1));
    for chunk in group.chunks(each.max(1)) {
        out.push(chunk.to_vec());
    }
}
