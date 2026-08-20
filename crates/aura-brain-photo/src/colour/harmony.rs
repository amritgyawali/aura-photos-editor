//! What to change about a frame's colour, and why - before anything is expressed in bands.
//!
//! PHASE-16 section 6.2:
//!
//! > Solve small per-band adjustments with a harmony objective: reduce hue conflicts with the
//! > subject, tame the most saturated distractor, and keep total adjustment magnitude small
//! > (professional edits are subtle).
//!
//! This module owns the objective. [`crate::colour::hsl`] owns turning it into the eight
//! bands the recipe holds, and the split is not cosmetic: the objective is stated in
//! *perceptual* terms - degrees of hue, fractions of saturation, a named piece of content -
//! and the bands are a rendering detail of the document it ends up in. Phase 17 will replace
//! the targets these goals are measured against with a photographer's own, and it should not
//! have to know what a band is to do it.
//!
//! ## The five goals, and the one that is a refusal
//!
//! | Goal | What it does | When |
//! |---|---|---|
//! | greenery | pulls foliage from yellow-green toward the scene's target hue, and desaturates a little | outdoor frames with confident foliage |
//! | sky | deepens the sky's saturation toward the scene's target | frames with confident sky |
//! | wood | pulls warm venue tones toward each other | frames with confident wood |
//! | distractor | reduces the single most competing saturated region | when it passes the scene's ceiling |
//! | dress | **forbids** any saturation increase on a bright near-neutral region | whenever a dress is found |
//!
//! The dress goal is the only one that is a prohibition rather than a move, and it is the one
//! that matters most in a wedding gallery: a dress that has picked up a colour cast is the
//! failure a photographer notices second, after skin.
//!
//! ## What is not here
//!
//! Skin. [`ContentBand::Skin`] is read by [`crate::colour::content`] and never appears as a
//! goal; `ContentBand::is_protected` is asserted before any goal is built. The guarantee is
//! measured on the pixels by [`crate::colour::skin_guard`] regardless, so this is the cheap
//! half of a two-part defence rather than the defence itself.

use aura_core::contract::colour::{ColourCode, ContentBand, HslBand};
use aura_core::contract::integrity::CropRect;

use crate::colour::codec::{self, Explained};
use crate::colour::content::{hue_distance, Reading};
use crate::colour::intent::SceneIntent;

/// How far a hue must sit from the subject's before a saturated region counts as competing.
///
/// Forty degrees. Closer than that and a saturated region is part of the same colour story as
/// the subject - a red sari behind a bride in red - and reducing it is taking the photograph
/// apart rather than tidying it.
pub const CONFLICT_ANGLE_DEG: f32 = 40.0;

/// How far apart two saturated regions must be before they are called a conflict.
///
/// Ninety degrees. Two saturated colours a quarter turn apart are what "competing hues"
/// means, and it is the situation `ColourCode::HueConflictFound` names.
pub const CONFLICT_PAIR_ANGLE_DEG: f32 = 90.0;

/// The most a goal may ask a hue to rotate, in degrees.
///
/// Twelve. Beyond it foliage stops being foliage. It is a *goal* bound rather than a band
/// bound - `SceneIntent::max_band_shift` is the second, applied after the conversion to
/// recipe units - and having both is deliberate: one is about the content and one is about
/// the document.
pub const MAX_HUE_GOAL_DEG: f32 = 12.0;

/// The most a goal may ask a saturation to move, as a fraction of what it was.
pub const MAX_SATURATION_GOAL: f32 = 0.30;

/// One thing to change about one kind of content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Goal {
    /// What the goal is about.
    pub content: ContentBand,
    /// Which of the recipe's eight bands carries it.
    pub band: HslBand,
    /// How far to rotate that band's hue, in degrees. Positive is toward yellow-green.
    pub hue_deg: f32,
    /// How far to move its saturation, as a fraction of what was measured.
    pub saturation: f32,
    /// How far to move its luminance, as a fraction. Nearly always zero.
    pub luma: f32,
    /// How sure the content inference was, `0..1`. Scales the whole goal.
    pub confidence: f32,
    /// True when this goal forbids an increase rather than asking for a change.
    pub is_prohibition: bool,
    /// Where in the frame, for the reason's evidence crop.
    pub region: Option<CropRect>,
}

/// The objective for one frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    /// The goals, in the order they were formed.
    pub goals: Vec<Goal>,
    /// The reasons, in the order they were formed.
    pub reasons: Vec<Explained>,
}

impl Plan {
    /// True when nothing needs changing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.goals.iter().all(|goal| {
            goal.is_prohibition
                || (goal.hue_deg.abs() < 0.5
                    && goal.saturation.abs() < 0.01
                    && goal.luma.abs() < 0.01)
        })
    }

    /// The bands this plan forbids a saturation increase on.
    #[must_use]
    pub fn prohibited(&self) -> Vec<HslBand> {
        self.goals
            .iter()
            .filter(|goal| goal.is_prohibition)
            .map(|goal| goal.band)
            .collect()
    }
}

/// Form the objective for one frame.
///
/// Deterministic and side-effect free. Every goal is scaled by the confidence of the content
/// reading it came from, so a band AURA is half sure about receives half the move - the
/// scaling and the [`crate::colour::content::MIN_CONFIDENCE`] gate together are what make
/// section 12's fourth failure mode, "HSL fights the photographer's taste", expensive to
/// reach by accident.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn plan(reading: &Reading, intent: &SceneIntent) -> Plan {
    let mut plan = Plan::default();

    // -- greenery --------------------------------------------------------
    //
    // Section 6.2's named case: "outdoor foliage is usually too yellow-green and too
    // saturated". Both halves are corrected, and both are bounded: the hue by
    // `MAX_HUE_GOAL_DEG` and the saturation by the scene's own target.
    if reading.actionable(ContentBand::Greenery) {
        if let Some(band) = reading.band(ContentBand::Greenery) {
            let hue_delta = signed_hue_delta(band.hue_deg, intent.greenery_hue_deg)
                .clamp(-MAX_HUE_GOAL_DEG, MAX_HUE_GOAL_DEG);
            let saturation_delta = relative_delta(band.saturation, intent.greenery_saturation);
            if hue_delta.abs() >= 0.5 || saturation_delta.abs() >= 0.01 {
                plan.goals.push(Goal {
                    content: ContentBand::Greenery,
                    band: HslBand::of_hue(band.hue_deg),
                    hue_deg: hue_delta,
                    saturation: saturation_delta,
                    luma: 0.0,
                    confidence: band.confidence,
                    is_prohibition: false,
                    region: None,
                });
                plan.reasons.push(codec::numbered(
                    ColourCode::GreeneryTamed,
                    format!(
                        "the foliage was recorded at {:.0} degrees and this kind of photograph \
                         wants it near {:.0}, so it was brought back and calmed down a little",
                        band.hue_deg, intent.greenery_hue_deg
                    ),
                    0.0,
                    vec![band.hue_deg, intent.greenery_hue_deg, saturation_delta],
                ));
            }
        }
    }

    // -- sky -------------------------------------------------------------
    //
    // Saturation only. A sky whose hue has been rotated is the most obviously edited thing in
    // a photograph, and there is no scene at a wedding where it is the right answer.
    if reading.actionable(ContentBand::Sky) {
        if let Some(band) = reading.band(ContentBand::Sky) {
            let saturation_delta = relative_delta(band.saturation, intent.sky_saturation);
            if saturation_delta.abs() >= 0.01 {
                plan.goals.push(Goal {
                    content: ContentBand::Sky,
                    band: HslBand::of_hue(band.hue_deg),
                    hue_deg: 0.0,
                    saturation: saturation_delta,
                    luma: 0.0,
                    confidence: band.confidence,
                    is_prohibition: false,
                    region: None,
                });
                plan.reasons
                    .push(codec::plain(ColourCode::SkyDeepened, 0.0));
            }
        }
    }

    // -- wood ------------------------------------------------------------
    //
    // The gentlest goal in the module. Warm venue surfaces are pulled a few degrees toward
    // each other so a room lit by two warm sources does not read as two rooms; nothing is
    // desaturated, because a warm wooden hall is what somebody chose.
    if reading.actionable(ContentBand::Wood) {
        if let Some(band) = reading.band(ContentBand::Wood) {
            let hue_delta = signed_hue_delta(band.hue_deg, WOOD_TARGET_HUE_DEG).clamp(-6.0, 6.0);
            if hue_delta.abs() >= 0.5 {
                plan.goals.push(Goal {
                    content: ContentBand::Wood,
                    band: HslBand::of_hue(band.hue_deg),
                    hue_deg: hue_delta,
                    saturation: 0.0,
                    luma: 0.0,
                    confidence: band.confidence,
                    is_prohibition: false,
                    region: None,
                });
                plan.reasons.push(codec::plain(ColourCode::WoodWarmed, 0.0));
            }
        }
    }

    // -- the distractor --------------------------------------------------
    //
    // Section 2.1's fluorescent green exit sign. Three conditions, all of them necessary: it
    // has to be more saturated than the scene accepts, it has to sit far enough from the
    // subject's own hue to be competing rather than harmonising, and it has to be a band the
    // content reading was confident about.
    if let Some(distractor) = reading.distractor {
        let over = distractor.saturation - intent.distractor_ceiling;
        let competing = distractor.hue_distance_deg >= CONFLICT_ANGLE_DEG;
        let band = HslBand::of_hue(distractor.hue_deg);
        let confidence = reading
            .bands
            .iter()
            .find(|entry| !entry.band.is_protected() && HslBand::of_hue(entry.hue_deg) == band)
            .map_or(0.0, |entry| entry.confidence);
        if over > 0.0 && competing && confidence >= crate::colour::content::MIN_CONFIDENCE {
            // Proportional to how far past the ceiling it is, not to how saturated it is. A
            // frame whose most saturated thing is only just over the line gets a nudge; the
            // exit sign gets the whole allowance.
            let reduction = -(over / (1.0 - intent.distractor_ceiling).max(0.05)).clamp(0.0, 1.0)
                * MAX_SATURATION_GOAL;
            plan.goals.push(Goal {
                content: ContentBand::Decor,
                band,
                hue_deg: 0.0,
                saturation: reduction,
                luma: 0.0,
                confidence,
                is_prohibition: false,
                region: Some(distractor.region),
            });
            plan.reasons.push(codec::at(
                ColourCode::DistractorTamed,
                format!(
                    "a colour covering {:.0}% of the frame was the most distracting thing in it, \
                     so that colour was reduced rather than the whole frame being desaturated",
                    distractor.area * 100.0
                ),
                0.0,
                vec![distractor.area, distractor.saturation, reduction],
                distractor.region,
            ));

            // A second saturated region a quarter turn from the first is what "competing
            // hues" means. It is recorded rather than acted on twice: taming both is how a
            // frame ends up desaturated, which is the outcome section 6.2 exists to avoid.
            if has_competing_pair(reading, distractor.hue_deg) {
                plan.reasons
                    .push(codec::plain(ColourCode::HueConflictFound, -0.02));
            }
        }
    }

    // -- the dress, as a prohibition -------------------------------------
    if let Some(band) = reading.band(ContentBand::Dress) {
        if band.area >= crate::colour::content::MIN_AREA {
            plan.goals.push(Goal {
                content: ContentBand::Dress,
                band: HslBand::of_hue(band.hue_deg),
                hue_deg: 0.0,
                saturation: 0.0,
                luma: 0.0,
                confidence: band.confidence,
                is_prohibition: true,
                region: None,
            });
            plan.reasons
                .push(codec::plain(ColourCode::DressProtected, 0.0));
        }
    }

    // -- what was seen and not acted on ----------------------------------
    //
    // "AURA saw greenery and was not sure enough to touch it" and "AURA saw no greenery" are
    // different sentences and only one of them is about this photograph.
    if !reading.unconfident().is_empty() {
        plan.reasons
            .push(codec::plain(ColourCode::BandUnconfident, -0.03));
    }

    plan
}

/// Where wood is pulled toward, in degrees.
///
/// Thirty-five. A warm neutral timber, between the orange of tungsten-lit pine and the red of
/// stained oak. It is a convention rather than a measurement, which is why it is a named
/// constant with this sentence against it rather than a number inside the function.
pub const WOOD_TARGET_HUE_DEG: f32 = 35.0;

/// The shortest signed rotation from `from` to `to`, in degrees.
#[must_use]
pub fn signed_hue_delta(from: f32, to: f32) -> f32 {
    let raw = (to - from).rem_euclid(360.0);
    if raw > 180.0 {
        raw - 360.0
    } else {
        raw
    }
}

/// How far a measured saturation is from a target, as a fraction of the measurement.
///
/// Relative rather than absolute, because a tenth of saturation is a large change on a pale
/// sky and a small one on a sari, and a solver that treated them the same would flatten one
/// and do nothing to the other.
#[must_use]
pub fn relative_delta(measured: f32, target: f32) -> f32 {
    if measured <= 0.01 {
        return 0.0;
    }
    ((target - measured) / measured).clamp(-MAX_SATURATION_GOAL, MAX_SATURATION_GOAL)
}

/// True when a second saturated region sits a quarter turn from the first.
fn has_competing_pair(reading: &Reading, hue: f32) -> bool {
    reading.bands.iter().any(|entry| {
        !entry.band.is_protected()
            && entry.band != ContentBand::Sky
            && entry.saturation >= 0.35
            && hue_distance(entry.hue_deg, hue) >= CONFLICT_PAIR_ANGLE_DEG
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::content::Distractor;
    use crate::colour::intent::IntentTable;
    use aura_core::contract::colour::BandReading;
    use aura_core::SceneId;

    fn intent_for(scene: SceneId) -> SceneIntent {
        IntentTable::embedded().map_or_else(
            |_| unreachable!("the shipped intents are embedded in the binary"),
            |table| table.get(scene),
        )
    }

    fn reading_with(bands: Vec<BandReading>, distractor: Option<Distractor>) -> Reading {
        Reading {
            skin_area: bands
                .iter()
                .find(|b| b.band == ContentBand::Skin)
                .map_or(0.0, |b| b.area),
            subject_hue_deg: bands
                .iter()
                .find(|b| b.band == ContentBand::Skin)
                .map(|b| b.hue_deg),
            classified: 0.8,
            bands,
            distractor,
        }
    }

    fn band(band: ContentBand, hue: f32, saturation: f32, area: f32) -> BandReading {
        BandReading {
            band,
            area,
            hue_deg: hue,
            saturation,
            luma: 0.4,
            confidence: 0.85,
        }
    }

    #[test]
    fn yellow_green_foliage_is_pulled_toward_the_scenes_target() {
        // The classic wedding problem, as a test. A camera records foliage near 95 degrees
        // and an outdoor portrait wants it near 128.
        let reading = reading_with(vec![band(ContentBand::Greenery, 95.0, 0.55, 0.30)], None);
        let plan = plan(&reading, &intent_for(SceneId::CouplePortrait));
        let Some(goal) = plan
            .goals
            .iter()
            .find(|goal| goal.content == ContentBand::Greenery)
            .copied()
        else {
            unreachable!("a confident greenery band must produce a goal")
        };
        assert!(goal.hue_deg > 0.0, "foliage should move toward green");
        assert!(goal.saturation < 0.0, "foliage should be calmed down");
        assert!(goal.hue_deg <= MAX_HUE_GOAL_DEG);
    }

    #[test]
    fn an_unconfident_band_is_reported_and_never_adjusted() {
        let mut foliage = band(ContentBand::Greenery, 95.0, 0.55, 0.30);
        foliage.confidence = 0.10;
        let plan = plan(
            &reading_with(vec![foliage], None),
            &intent_for(SceneId::Venue),
        );
        assert!(plan
            .goals
            .iter()
            .all(|goal| goal.content != ContentBand::Greenery));
        assert!(plan
            .reasons
            .iter()
            .any(|(reason, _)| reason.code == ColourCode::BandUnconfident));
    }

    #[test]
    fn the_exit_sign_is_tamed_and_the_frame_is_not() {
        // Section 2.1's own example. One saturated green region far from the subject's hue,
        // over the scene's ceiling: the goal reduces THAT band and touches nothing else.
        let reading = reading_with(
            vec![
                band(ContentBand::Skin, 30.0, 0.35, 0.10),
                band(ContentBand::Decor, 140.0, 0.95, 0.12),
            ],
            Some(Distractor {
                hue_deg: 140.0,
                saturation: 0.95,
                area: 0.12,
                region: CropRect::FULL,
                hue_distance_deg: 110.0,
            }),
        );
        let plan = plan(&reading, &intent_for(SceneId::Vows));
        let tamed: Vec<&Goal> = plan
            .goals
            .iter()
            .filter(|goal| goal.saturation < 0.0)
            .collect();
        assert_eq!(tamed.len(), 1, "exactly one band should be tamed");
        assert!(plan
            .reasons
            .iter()
            .any(|(reason, _)| reason.code == ColourCode::DistractorTamed));
    }

    #[test]
    fn a_saturated_region_that_matches_the_subject_is_harmony_and_is_left_alone() {
        // A red sari behind a bride in red. Reducing it is taking the photograph apart.
        let reading = reading_with(
            vec![
                band(ContentBand::Skin, 25.0, 0.35, 0.10),
                band(ContentBand::Decor, 20.0, 0.95, 0.20),
            ],
            Some(Distractor {
                hue_deg: 20.0,
                saturation: 0.95,
                area: 0.20,
                region: CropRect::FULL,
                hue_distance_deg: 5.0,
            }),
        );
        let plan = plan(&reading, &intent_for(SceneId::Ritual));
        assert!(plan
            .reasons
            .iter()
            .all(|(reason, _)| reason.code != ColourCode::DistractorTamed));
    }

    #[test]
    fn a_dance_floor_leaves_its_own_colour_alone() {
        // Phase 15 decided not to neutralise a purple dance floor. This asserts phase 16
        // cannot do it by another route: the scene's ceiling is high enough that a saturated
        // uplighter never becomes a distractor.
        let reading = reading_with(
            vec![
                band(ContentBand::Skin, 28.0, 0.30, 0.06),
                band(ContentBand::Decor, 285.0, 0.90, 0.40),
            ],
            Some(Distractor {
                hue_deg: 285.0,
                saturation: 0.90,
                area: 0.40,
                region: CropRect::FULL,
                hue_distance_deg: 150.0,
            }),
        );
        let plan = plan(&reading, &intent_for(SceneId::DanceFloor));
        assert!(plan
            .reasons
            .iter()
            .all(|(reason, _)| reason.code != ColourCode::DistractorTamed));
    }

    #[test]
    fn a_dress_produces_a_prohibition_rather_than_a_move() {
        let reading = reading_with(vec![band(ContentBand::Dress, 40.0, 0.04, 0.22)], None);
        let plan = plan(&reading, &intent_for(SceneId::GettingReadyBride));
        let Some(goal) = plan
            .goals
            .iter()
            .find(|goal| goal.content == ContentBand::Dress)
            .copied()
        else {
            unreachable!("a dress band must produce a prohibition")
        };
        assert!(goal.is_prohibition);
        assert!(goal.saturation.abs() < 1e-6);
        assert!(!plan.prohibited().is_empty());
    }

    #[test]
    fn skin_is_never_a_goal() {
        // The structural half of the guarantee. Skin is read and never planned against, on
        // every scene in the taxonomy.
        let reading = reading_with(vec![band(ContentBand::Skin, 30.0, 0.40, 0.35)], None);
        for scene in SceneId::ALL {
            let plan = plan(&reading, &intent_for(scene));
            assert!(
                plan.goals
                    .iter()
                    .all(|goal| goal.content != ContentBand::Skin),
                "{scene:?} planned against skin"
            );
        }
    }
}
