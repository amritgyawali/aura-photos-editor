//! The thresholds a check is measured against, and the bounds the code keeps for itself.
//! PHASE-27 section 6.1.
//!
//! ## The one thing that makes a threshold file safe
//!
//! Section 6.1: "thresholds live in `qc_thresholds.toml` per scene class, because a dance floor
//! tolerates more than a family formal." That is a product decision, so the file is a product
//! manager's to argue with - and a QC agent whose thresholds a studio can *widen* is a QC agent a
//! studio can switch off by editing a number.
//!
//! So the direction is asymmetric and the asymmetry is enforced here: **the file may tighten a
//! bound and may never widen one.** Every per-scene threshold has a ceiling in this module, and a
//! row asking for a larger one is `AURA-ML-5140` and the pass halts. The three loop parameters run
//! the other way - `min_gain_share` has a *floor*, `max_collateral` a ceiling, `replace_confidence`
//! a floor - because for each of those, "safer" is a different direction.
//!
//! This is phase 21's rule in its fourth application, after phases 22 and 24: a ceiling a studio can
//! lower and nobody can raise is what makes a written promise a property of the product rather than
//! a description of its defaults.
//!
//! ## Why the pass halts rather than falling back
//!
//! Phases 24, 25 and 26 all made this choice for their own policy tables and this phase has the
//! strongest version of the reason. A QC pass running on thresholds nobody chose would produce a
//! report a photographer *trusts*, and a set of remedies applied to their delivered gallery, out of
//! a file that failed to load. Refusing to run is legible; running on defaults is not.

use std::collections::BTreeMap;

use aura_core::contract::error::AuraResult;
use aura_core::contract::qc::{
    MAX_COLLATERAL, MAX_ROUNDS, MAX_TICKETS_PER_IMAGE, MIN_GAIN_SHARE, REPLACE_CONFIDENCE_FLOOR,
    REPLACE_MARGIN,
};
use aura_core::contract::scene::SceneId;
use serde::Deserialize;

use crate::errors::policy_refused;

/// Which thresholds table produced a stored finding.
///
/// Bumped whenever a shipped row moves. Separate from `ANALYSIS_VER` because the two invalidate
/// different things: this invalidates every `threshold` and therefore every severity ordering,
/// without invalidating the deviations themselves. `AURA-ML-5141` is raised on either.
pub const THRESHOLDS_VER: u16 = 1;

// ---------------------------------------------------------------------------
// The ceilings the code owns
// ---------------------------------------------------------------------------

/// The largest colour drift from a lighting group's own target that any scene may tolerate, as a
/// multiple of the node's own tolerance.
///
/// Phase 25 already computes a per-node tolerance from that node's own frames, so this is a
/// multiplier rather than an absolute: a threshold in kelvin would be wrong in every node whose
/// light was more or less variable than average, which is all of them.
///
/// Two. A frame twice its group's own tolerance out is a frame that does not look like it was shot
/// in the same room.
pub const MAX_CONSISTENCY_SIGMA: f32 = 2.0;

/// The largest grade-character difference from a node's anchors any scene may tolerate.
///
/// The eight-number signature phase 25 and phase 26 both measure over, as a Euclidean distance.
pub const MAX_SIGNATURE_DISTANCE: f32 = 0.30;

/// The largest per-person skin difference from their own gallery target, in dE00.
///
/// Phase 25's own promise is 2.0 dE00 across a gallery, so a QC threshold above that would be a
/// check that cannot fail on a gallery phase 25 already passed. Set at 3.0 because this measures
/// the *delivered* frame after phases 16, 20, 21 and 26 have all moved it, and a check identical to
/// the upstream guarantee would fire on rounding.
pub const MAX_SKIN_DE00: f32 = 3.0;

/// The largest skin hue shift phase 16's guard may have measured before it is a finding, in degrees.
pub const MAX_SKIN_HUE_DEG: f32 = 6.0;

/// The largest skin chroma change before it is a finding.
pub const MAX_SKIN_CHROMA: f32 = 0.10;

/// The largest departure from a scene's own subject-luminance band, in EV.
pub const MAX_EXPOSURE_EV: f32 = 1.00;

/// The largest share of the frame the edit may newly clip, in either direction.
pub const MAX_CLIPPING_ADDED: f32 = 0.05;

/// The most shadow headroom a delivered frame may have given up.
///
/// Stated as a *deficit* rather than as a floor, so that every row in this table reads the same
/// way: **a larger number is more permissive**, and the loader's one rule - the file may lower a
/// value and may never raise it - therefore always means "a studio may make AURA fussier".
///
/// Two thresholds in this table were originally written the other way round, as a required
/// minimum. Both had to be reformulated, because a mixed table is a table where a studio tightening
/// one row and loosening another cannot tell which they did.
pub const MAX_SHADOW_DEFICIT: f32 = 0.35;

/// The reference floor a delivered frame's subject-against-background sharpness is measured from.
///
/// Code-owned and not in the file. What a scene row tunes is how far *below* this a frame may sit
/// ([`MAX_SHARPNESS_SLACK`]), which keeps the direction consistent with every other row: a bigger
/// slack tolerates softer frames.
///
/// Putting the floor itself in the file would invert one row's meaning against the other eighteen -
/// raising it would demand sharper frames, which is stricter, while the loader only permits values
/// to fall. A studio would then loosen this check by editing it in the direction that tightens
/// everything else.
pub const SHARPNESS_REFERENCE_FLOOR: f32 = 0.55;

/// The most softness, below [`SHARPNESS_REFERENCE_FLOOR`], a scene may tolerate.
pub const MAX_SHARPNESS_SLACK: f32 = 0.45;

/// The most ringing a sharpened frame may carry, `0..1`.
pub const MAX_RINGING: f32 = 0.12;

/// The most texture a denoised frame may have lost, `0..1`.
pub const MAX_TEXTURE_LOSS: f32 = 0.30;

/// The furthest a recovered face may sit from its own identity, `0..1`.
///
/// Phase 22 holds this at its own ceiling on every face it recovers, so a QC finding here means
/// that guard did not run rather than that it failed. Both are worth a ticket and the reason codes
/// differ.
pub const MAX_IDENTITY_DRIFT: f32 = 0.08;

/// How far below its own floor a retouched frame's texture band may sit before it is a finding.
pub const MAX_TEXTURE_FLOOR_MISS: f32 = 0.15;

/// The most a micro-retouch may have moved a catchlight, a hairline or a set of teeth.
pub const MAX_NATURALNESS_EXCURSION: f32 = 0.25;

/// The most of phase 19's per-image perceptual allowance a delivered frame may have spent.
///
/// Above 1.0 by construction: the allowance is already a budget phase 19 enforces, so a value at or
/// below one could never fire and a check that can never fire is a check that is not running.
pub const MAX_ALLOWANCE_OVERSPEND: f32 = 0.20;

/// The most an operation may exceed what its region supports, `0..1`.
pub const MAX_MASK_OVERREACH: f32 = 0.25;

/// The largest artefact score a generative removal may leave, `0..1`.
pub const MAX_CLEANUP_ARTEFACT: f32 = 0.20;

/// The largest difference-hash distance at which two delivered frames are called duplicates.
///
/// Phase 08 owns duplicate policy and this is deliberately *tighter* than its own near-duplicate
/// band: the question here is not "are these the same shot" - phase 08 answered that - but "did two
/// frames a client will scroll past in sequence turn out identical". Phase 05's rule: a distance is
/// evidence and the deciding phase owns the threshold, so this one is this phase's own and does not
/// pretend to be phase 08's.
pub const MAX_DUPLICATE_HAMMING: u32 = 6;

/// The largest crop shortfall against a purpose's resolution floor, as a fraction of the long edge.
pub const MAX_CROP_SHORTFALL: f32 = 0.30;

// ---------------------------------------------------------------------------
// One scene's row
// ---------------------------------------------------------------------------

/// The thresholds for one scene class.
///
/// Invariant 7, in its twenty-second application: no threshold in this product is global. A dance
/// floor at ISO 12800 tolerates softness a family formal does not, and a ceremony tolerates a colour
/// drift a detail shot does not, and one number for both is wrong twice.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneThresholds {
    /// How many of its lighting group's own tolerances a frame may sit out.
    pub consistency_sigma: f32,
    /// How far this frame's grade character may sit from its anchors'.
    pub signature_distance: f32,
    /// How far a person's skin may sit from their own gallery target, in dE00.
    pub skin_de00: f32,
    /// How far the grade may have moved skin's hue, in degrees.
    pub skin_hue_deg: f32,
    /// How far it may have moved skin's chroma.
    pub skin_chroma: f32,
    /// How far the finished frame may sit from its scene band, in EV.
    pub exposure_ev: f32,
    /// How much of the frame the edit may newly clip.
    pub clipping_added: f32,
    /// How little shadow headroom the finished frame may be left with.
    pub shadow_deficit: f32,
    /// How far below [`SHARPNESS_REFERENCE_FLOOR`] the subject may sit before it is a finding.
    pub sharpness_slack: f32,
    /// How much ringing sharpening may have left.
    pub ringing: f32,
    /// How much texture denoising may have taken.
    pub texture_loss: f32,
    /// How far a recovered face may have moved from its own identity.
    pub identity_drift: f32,
    /// How far below its floor the retouched texture band may sit.
    pub texture_floor_miss: f32,
    /// How far a micro-retouch may have moved a catchlight, hairline or teeth.
    pub naturalness_excursion: f32,
    /// How much of the per-image local allowance may be overspent.
    pub allowance_overspend: f32,
    /// How far an operation may exceed what its region supports.
    pub mask_overreach: f32,
    /// How visible a generative removal may be.
    pub cleanup_artefact: f32,
    /// How close two delivered frames may be, in difference-hash distance.
    pub duplicate_hamming: u32,
    /// How far a crop may fall below its purpose's resolution floor.
    pub crop_shortfall: f32,
    /// Why this row is what it is. Required, and the loader refuses a row without one.
    ///
    /// Fourth config file in the product to demand it, after phases 10, 12 and 25. A threshold
    /// nobody can explain is a threshold nobody can argue with, and this table is where the product
    /// decides how fussy it is about somebody's wedding.
    #[serde(skip)]
    pub has_reason: bool,
}

impl SceneThresholds {
    /// The reference row: every ceiling at the code's own maximum.
    ///
    /// What a scene the file does not name falls back to, and what every unit test in this crate is
    /// written against. It is the *most permissive* legal row, deliberately: a scene nobody has
    /// thought about should produce the fewest tickets, because a ticket the product cannot justify
    /// is worse than a finding it did not make.
    #[must_use]
    pub const fn reference() -> Self {
        Self {
            consistency_sigma: MAX_CONSISTENCY_SIGMA,
            signature_distance: MAX_SIGNATURE_DISTANCE,
            skin_de00: MAX_SKIN_DE00,
            skin_hue_deg: MAX_SKIN_HUE_DEG,
            skin_chroma: MAX_SKIN_CHROMA,
            exposure_ev: MAX_EXPOSURE_EV,
            clipping_added: MAX_CLIPPING_ADDED,
            shadow_deficit: MAX_SHADOW_DEFICIT,
            sharpness_slack: MAX_SHARPNESS_SLACK,
            ringing: MAX_RINGING,
            texture_loss: MAX_TEXTURE_LOSS,
            identity_drift: MAX_IDENTITY_DRIFT,
            texture_floor_miss: MAX_TEXTURE_FLOOR_MISS,
            naturalness_excursion: MAX_NATURALNESS_EXCURSION,
            allowance_overspend: MAX_ALLOWANCE_OVERSPEND,
            mask_overreach: MAX_MASK_OVERREACH,
            cleanup_artefact: MAX_CLEANUP_ARTEFACT,
            duplicate_hamming: MAX_DUPLICATE_HAMMING,
            crop_shortfall: MAX_CROP_SHORTFALL,
            has_reason: true,
        }
    }

    /// Every threshold on this row, as `(name, value, code ceiling)`.
    ///
    /// One list rather than nineteen comparisons, so that adding a threshold cannot silently ship
    /// without a bound: the loader iterates this, and a field added to the struct and forgotten
    /// here is a field with no ceiling - which is why the test below asserts the length.
    #[must_use]
    pub fn bounded(&self) -> [(&'static str, f32, f32); 19] {
        [
            (
                "consistency_sigma",
                self.consistency_sigma,
                MAX_CONSISTENCY_SIGMA,
            ),
            (
                "signature_distance",
                self.signature_distance,
                MAX_SIGNATURE_DISTANCE,
            ),
            ("skin_de00", self.skin_de00, MAX_SKIN_DE00),
            ("skin_hue_deg", self.skin_hue_deg, MAX_SKIN_HUE_DEG),
            ("skin_chroma", self.skin_chroma, MAX_SKIN_CHROMA),
            ("exposure_ev", self.exposure_ev, MAX_EXPOSURE_EV),
            ("clipping_added", self.clipping_added, MAX_CLIPPING_ADDED),
            ("shadow_deficit", self.shadow_deficit, MAX_SHADOW_DEFICIT),
            ("sharpness_slack", self.sharpness_slack, MAX_SHARPNESS_SLACK),
            ("ringing", self.ringing, MAX_RINGING),
            ("texture_loss", self.texture_loss, MAX_TEXTURE_LOSS),
            ("identity_drift", self.identity_drift, MAX_IDENTITY_DRIFT),
            (
                "texture_floor_miss",
                self.texture_floor_miss,
                MAX_TEXTURE_FLOOR_MISS,
            ),
            (
                "naturalness_excursion",
                self.naturalness_excursion,
                MAX_NATURALNESS_EXCURSION,
            ),
            (
                "allowance_overspend",
                self.allowance_overspend,
                MAX_ALLOWANCE_OVERSPEND,
            ),
            ("mask_overreach", self.mask_overreach, MAX_MASK_OVERREACH),
            (
                "cleanup_artefact",
                self.cleanup_artefact,
                MAX_CLEANUP_ARTEFACT,
            ),
            (
                "duplicate_hamming",
                self.duplicate_hamming as f32,
                MAX_DUPLICATE_HAMMING as f32,
            ),
            ("crop_shortfall", self.crop_shortfall, MAX_CROP_SHORTFALL),
        ]
    }
}

// ---------------------------------------------------------------------------
// The loop's own parameters
// ---------------------------------------------------------------------------

/// The three numbers the re-edit loop runs on, plus its two bounds.
///
/// Not per scene. A remedy either realised its predicted gain or it did not, and that is arithmetic
/// rather than an aesthetic judgement - a dance floor does not want a *different* definition of
/// "this correction worked".
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopPolicy {
    /// The share of a predicted gain a remedy must realise. Floored by [`MIN_GAIN_SHARE`].
    pub min_gain_share: f32,
    /// How far a remedy may worsen another check. Capped by [`MAX_COLLATERAL`].
    pub max_collateral: f32,
    /// The confidence a replacement needs. Floored by [`REPLACE_CONFIDENCE_FLOOR`].
    pub replace_confidence: f32,
    /// How much better a runner-up must be. Floored by [`REPLACE_MARGIN`].
    pub replace_margin: f32,
    /// Rounds per image. Capped by [`MAX_ROUNDS`].
    pub max_rounds: u8,
    /// Tickets per image. Capped by [`MAX_TICKETS_PER_IMAGE`].
    pub max_tickets_per_image: u8,
}

impl LoopPolicy {
    /// The shipped defaults: every parameter at the contract's own bound.
    #[must_use]
    pub const fn reference() -> Self {
        Self {
            min_gain_share: MIN_GAIN_SHARE,
            max_collateral: MAX_COLLATERAL,
            replace_confidence: REPLACE_CONFIDENCE_FLOOR,
            replace_margin: REPLACE_MARGIN,
            max_rounds: MAX_ROUNDS,
            max_tickets_per_image: MAX_TICKETS_PER_IMAGE as u8,
        }
    }

    /// Refuse anything that would make the loop less careful than the contract permits.
    ///
    /// Three of the five run one way and two the other, which is the whole reason this is a
    /// hand-written function rather than a loop over a table: "safer" is not a single direction.
    /// Raising `min_gain_share` keeps fewer remedies and is safe; lowering it keeps remedies that
    /// did not work. Lowering `max_collateral` reverts more and is safe; raising it keeps remedies
    /// that broke something else.
    fn check(&self) -> Result<(), String> {
        if !(self.min_gain_share.is_finite() && self.min_gain_share >= MIN_GAIN_SHARE) {
            return Err(format!(
                "min_gain_share = {} is below the contract's floor of {MIN_GAIN_SHARE}; a smaller \
                 share keeps remedies that did not work",
                self.min_gain_share
            ));
        }
        if !(self.max_collateral.is_finite()
            && self.max_collateral >= 0.0
            && self.max_collateral <= MAX_COLLATERAL)
        {
            return Err(format!(
                "max_collateral = {} is outside 0..{MAX_COLLATERAL}; a larger tolerance keeps \
                 remedies that broke another check",
                self.max_collateral
            ));
        }
        if !(self.replace_confidence.is_finite()
            && self.replace_confidence >= REPLACE_CONFIDENCE_FLOOR
            && self.replace_confidence <= 1.0)
        {
            return Err(format!(
                "replace_confidence = {} is outside {REPLACE_CONFIDENCE_FLOOR}..1.0; a replacement \
                 is the one remedy whose mistake a photographer cannot see",
                self.replace_confidence
            ));
        }
        if !(self.replace_margin.is_finite() && self.replace_margin >= REPLACE_MARGIN) {
            return Err(format!(
                "replace_margin = {} is below the contract's floor of {REPLACE_MARGIN}; a smaller \
                 margin swaps frames on measurement noise",
                self.replace_margin
            ));
        }
        if self.max_rounds > MAX_ROUNDS {
            return Err(format!(
                "max_rounds = {} is above the contract's ceiling of {MAX_ROUNDS}",
                self.max_rounds
            ));
        }
        if usize::from(self.max_tickets_per_image) > MAX_TICKETS_PER_IMAGE {
            return Err(format!(
                "max_tickets_per_image = {} is above the contract's ceiling of \
                 {MAX_TICKETS_PER_IMAGE}",
                self.max_tickets_per_image
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------

/// Everything the checks and the loop are measured against.
#[derive(Debug, Clone, PartialEq)]
pub struct Thresholds {
    scenes: BTreeMap<SceneId, SceneThresholds>,
    fallback: SceneThresholds,
    loop_policy: LoopPolicy,
    version: u16,
}

impl Thresholds {
    /// The reference table: every scene at the code's own ceiling, and the shipped loop policy.
    ///
    /// What every unit test in this crate runs against, and what a build with no config file falls
    /// back to. It is not what ships - `crates/aura-qc/config/qc_thresholds.toml` is, and it is
    /// tighter in nineteen places.
    #[must_use]
    pub fn reference() -> Self {
        Self {
            scenes: BTreeMap::new(),
            fallback: SceneThresholds::reference(),
            loop_policy: LoopPolicy::reference(),
            version: THRESHOLDS_VER,
        }
    }

    /// The thresholds for one scene, falling back on the reference row.
    ///
    /// A scene the file does not name gets the *most permissive* legal row rather than an error,
    /// for the reason `SceneId::from_str_or_unknown` exists: a catalog written by a newer build must
    /// still open, and a scene this build has no opinion about should produce the fewest tickets.
    #[must_use]
    pub fn scene(&self, scene: SceneId) -> SceneThresholds {
        self.scenes.get(&scene).copied().unwrap_or(self.fallback)
    }

    /// The loop's parameters.
    #[must_use]
    pub const fn loop_policy(&self) -> LoopPolicy {
        self.loop_policy
    }

    /// Which table this is.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many scenes the file names.
    #[must_use]
    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }

    /// Parse and validate a thresholds table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5140` when the TOML does not parse, when a scene slug is unknown, when a scene row
    /// is duplicated, when a row has no `reason`, when a threshold is above the code's own ceiling,
    /// or when a loop parameter would make the loop less careful than the contract permits.
    pub fn parse(text: &str) -> AuraResult<Self> {
        let file: File = toml::from_str(text)
            .map_err(|err| policy_refused(format!("qc_thresholds.toml did not parse: {err}")))?;

        file.loop_policy.check().map_err(policy_refused)?;

        let mut scenes = BTreeMap::new();
        for row in file.scene {
            let scene = SceneId::ALL
                .into_iter()
                .find(|kind| kind.as_str() == row.id)
                .ok_or_else(|| {
                    policy_refused(format!(
                        "qc_thresholds.toml names an unknown scene '{}'; the twenty-three slugs \
                         are the ones `SceneId::as_str` renders",
                        row.id
                    ))
                })?;
            if row.reason.trim().is_empty() {
                return Err(policy_refused(format!(
                    "the '{}' row has no reason. Every row in this table is a product decision \
                     about how fussy AURA is with somebody's wedding, and a threshold nobody can \
                     explain is a threshold nobody can argue with",
                    row.id
                )));
            }
            let mut values = row.thresholds;
            values.has_reason = true;
            for (name, value, ceiling) in values.bounded() {
                if !value.is_finite() || value < 0.0 {
                    return Err(policy_refused(format!(
                        "the '{}' row's {name} = {value} is not a usable number",
                        row.id
                    )));
                }
                if value > ceiling {
                    return Err(policy_refused(format!(
                        "the '{}' row asks for {name} = {value}, above the {ceiling} this build \
                         permits. This file may make AURA fussier and may never make it more \
                         permissive",
                        row.id
                    )));
                }
            }
            if scenes.insert(scene, values).is_some() {
                return Err(policy_refused(format!(
                    "the '{}' row appears twice; the second would silently win",
                    row.id
                )));
            }
        }

        Ok(Self {
            scenes,
            fallback: SceneThresholds::reference(),
            loop_policy: file.loop_policy,
            version: file.version,
        })
    }

    /// The table this build ships, compiled in.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5140` when the shipped file does not satisfy its own loader, which is a build
    /// failure rather than a runtime one - and is exactly what the phase gate checks.
    pub fn shipped() -> AuraResult<Self> {
        Self::parse(include_str!("../config/qc_thresholds.toml"))
    }
}

// ---------------------------------------------------------------------------
// The file's own shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u16,
    #[serde(rename = "loop")]
    loop_policy: LoopPolicy,
    #[serde(default)]
    scene: Vec<SceneRow>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneRow {
    id: String,
    reason: String,
    #[serde(flatten)]
    thresholds: SceneThresholds,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_satisfies_its_own_loader() {
        let table = Thresholds::shipped().expect("the shipped thresholds table must load");
        assert_eq!(table.version(), THRESHOLDS_VER);
        // Every scene the classifier can emit has a row, plus the abstention. A scene with no row
        // falls back on the most permissive legal one, and a table that relied on that would be a
        // table whose per-scene reasoning had quietly stopped being per scene.
        assert_eq!(table.scene_count(), SceneId::ALL.len());
    }

    #[test]
    fn every_threshold_has_a_ceiling_and_the_list_is_complete() {
        // The count is asserted so that a nineteenth-plus field added to `SceneThresholds` and
        // forgotten in `bounded()` fails here rather than shipping with no bound.
        assert_eq!(SceneThresholds::reference().bounded().len(), 19);
    }

    #[test]
    fn the_file_may_tighten_a_threshold() {
        let text = r#"
version = 1
[loop]
min_gain_share = 0.5
max_collateral = 0.10
replace_confidence = 0.85
replace_margin = 0.35
max_rounds = 2
max_tickets_per_image = 8
[[scene]]
id = "ceremony"
reason = "a ceremony is the one part of the day nobody can reshoot"
consistency_sigma = 1.0
signature_distance = 0.10
skin_de00 = 1.5
skin_hue_deg = 3.0
skin_chroma = 0.05
exposure_ev = 0.4
clipping_added = 0.01
shadow_deficit = 0.2
sharpness_slack = 0.3
ringing = 0.05
texture_loss = 0.15
identity_drift = 0.04
texture_floor_miss = 0.08
naturalness_excursion = 0.12
allowance_overspend = 0.10
mask_overreach = 0.12
cleanup_artefact = 0.10
duplicate_hamming = 3
crop_shortfall = 0.15
"#;
        let table = Thresholds::parse(text).expect("a tightened row is legal");
        assert_eq!(table.scene(SceneId::Ceremony).skin_de00, 1.5);
        // And an unnamed scene falls back on the reference row rather than on the ceremony's.
        assert_eq!(
            table.scene(SceneId::DanceFloor).skin_de00,
            SceneThresholds::reference().skin_de00
        );
    }

    fn one_scene(overrides: &str) -> String {
        let mut row = String::from(
            r#"
version = 1
[loop]
min_gain_share = 0.5
max_collateral = 0.10
replace_confidence = 0.85
replace_margin = 0.35
max_rounds = 2
max_tickets_per_image = 8
[[scene]]
id = "ceremony"
reason = "because"
consistency_sigma = 1.0
signature_distance = 0.10
skin_de00 = 1.5
skin_hue_deg = 3.0
skin_chroma = 0.05
exposure_ev = 0.4
clipping_added = 0.01
shadow_deficit = 0.2
sharpness_slack = 0.3
ringing = 0.05
texture_loss = 0.15
identity_drift = 0.04
texture_floor_miss = 0.08
naturalness_excursion = 0.12
allowance_overspend = 0.10
mask_overreach = 0.12
cleanup_artefact = 0.10
duplicate_hamming = 3
crop_shortfall = 0.15
"#,
        );
        row.push_str(overrides);
        row
    }

    #[test]
    fn the_file_may_never_widen_a_threshold() {
        // Replace `skin_de00 = 1.5` with something above the code's ceiling.
        let text = one_scene("").replace("skin_de00 = 1.5", "skin_de00 = 9.0");
        let err = Thresholds::parse(&text).expect_err("a widened threshold is refused");
        assert_eq!(err.code.0, "AURA-ML-5140");
        assert!(err.detail.contains("skin_de00"));
        assert!(err.detail.contains("never make it more permissive"));
    }

    #[test]
    fn a_row_without_a_reason_is_refused() {
        let text = one_scene("").replace(r#"reason = "because""#, r#"reason = "  ""#);
        let err = Thresholds::parse(&text).expect_err("a row with no reason is refused");
        assert!(err.detail.contains("no reason"));
    }

    #[test]
    fn an_unknown_scene_is_refused_rather_than_ignored() {
        let text = one_scene("").replace(r#"id = "ceremony""#, r#"id = "afterparty""#);
        let err = Thresholds::parse(&text).expect_err("an unknown scene is refused");
        assert!(err.detail.contains("afterparty"));
    }

    #[test]
    fn a_duplicated_scene_row_is_refused_because_the_second_would_win_silently() {
        let mut text = one_scene("");
        let block = text.clone();
        let second = block
            .split_once("[[scene]]")
            .map(|(_, rest)| format!("[[scene]]{rest}"))
            .unwrap_or_default();
        text.push_str(&second);
        let err = Thresholds::parse(&text).expect_err("a duplicated row is refused");
        assert!(err.detail.contains("twice"));
    }

    #[test]
    fn a_looser_loop_is_refused_in_both_directions() {
        let lower_gain = one_scene("").replace("min_gain_share = 0.5", "min_gain_share = 0.1");
        let err = Thresholds::parse(&lower_gain).expect_err("a smaller gain share is refused");
        assert!(err.detail.contains("did not work"));

        let more_collateral =
            one_scene("").replace("max_collateral = 0.10", "max_collateral = 0.9");
        let err = Thresholds::parse(&more_collateral).expect_err("more collateral is refused");
        assert!(err.detail.contains("broke another check"));

        let weaker_replace =
            one_scene("").replace("replace_confidence = 0.85", "replace_confidence = 0.5");
        let err = Thresholds::parse(&weaker_replace).expect_err("a weaker replacement is refused");
        assert!(err.detail.contains("cannot see"));

        let more_rounds = one_scene("").replace("max_rounds = 2", "max_rounds = 5");
        let err = Thresholds::parse(&more_rounds).expect_err("more rounds are refused");
        assert!(err.detail.contains("max_rounds"));
    }

    #[test]
    fn a_tighter_loop_is_permitted() {
        let stricter = one_scene("")
            .replace("min_gain_share = 0.5", "min_gain_share = 0.8")
            .replace("max_collateral = 0.10", "max_collateral = 0.02")
            .replace("replace_confidence = 0.85", "replace_confidence = 0.95")
            .replace("max_rounds = 2", "max_rounds = 1");
        let table = Thresholds::parse(&stricter).expect("a stricter loop is legal");
        assert_eq!(table.loop_policy().max_rounds, 1);
        assert_eq!(table.loop_policy().min_gain_share, 0.8);
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_ignored() {
        let text = one_scene("").replace(
            "crop_shortfall = 0.15",
            "crop_shortfall = 0.15\nfussiness = 3.0",
        );
        let err = Thresholds::parse(&text).expect_err("an unknown key is refused");
        // `deny_unknown_fields`, because a typo in a threshold name would otherwise leave that
        // threshold at the permissive default while the studio believed they had tightened it.
        assert_eq!(err.code.0, "AURA-ML-5140");
    }
}
