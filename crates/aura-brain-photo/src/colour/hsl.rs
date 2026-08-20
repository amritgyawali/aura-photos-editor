//! The harmony objective, expressed in the eight bands the recipe holds.
//!
//! [`crate::colour::harmony`] decides *what* should change, in degrees of hue and fractions
//! of saturation. This module decides what the recipe should say, which is a different
//! question with three parts: the unit conversion, the caps, and the two prohibitions.
//!
//! ## The unit conversion, and why it is written down here
//!
//! `aura_render::tonemap::hsl` is the authority on what a band value does to a pixel, and it
//! says three things:
//!
//! * `h = 100` rotates a pixel one eighth of the wheel - **45 degrees**, one band's width -
//!   so a band can never rotate past its neighbour;
//! * `s = 100` doubles saturation, so a band value is a **percentage of what was there**;
//! * `l = 100` multiplies luminance by 1.5.
//!
//! Those three ratios are [`DEGREES_PER_UNIT`], [`SATURATION_PER_UNIT`] and
//! [`LUMA_PER_UNIT`], and they are the only place in this phase that knows them. A solver
//! that guessed them would produce goals that are right in degrees and wrong on the screen.
//!
//! ## The two prohibitions
//!
//! **A skin band is attenuated before anything else happens.** Orange carries most of a face
//! at every skin tone, red and yellow carry its edges, and this module scales any adjustment
//! landing on them by [`SKIN_BAND_ATTENUATION`] and [`SKIN_ADJACENT_ATTENUATION`]. It is the
//! cheap half of section 6.3's guarantee: it costs nothing, it catches the common case, and
//! it is **not** the guarantee - [`crate::colour::skin_guard`] measures the pixels afterwards
//! and can withdraw everything this module produced.
//!
//! **A dress band may not gain saturation.** A bright near-neutral region that picks up a
//! colour cast is the second thing a photographer notices after skin, and the prohibition is
//! a clamp rather than an attenuation because there is no amount of it that is correct.

use aura_core::contract::colour::{ColourCode, HslAdjustments, HslBand, HslShift};

use crate::colour::codec::{self, Explained};
use crate::colour::harmony::{Goal, Plan};
use crate::colour::intent::SceneIntent;

/// Degrees of hue rotation one recipe unit produces.
///
/// 0.45, because `h = 100` rotates one eighth of the wheel. Taken from
/// `aura_render::tonemap::hsl` rather than chosen.
pub const DEGREES_PER_UNIT: f32 = 45.0 / 100.0;

/// The fraction of a pixel's saturation one recipe unit adds.
pub const SATURATION_PER_UNIT: f32 = 1.0 / 100.0;

/// The fraction of a pixel's luminance one recipe unit adds.
pub const LUMA_PER_UNIT: f32 = 0.5 / 100.0;

/// How much of an adjustment survives landing on the skin band.
///
/// A quarter. Orange is where most of a face sits at every skin tone, so an adjustment aimed
/// at a wooden floor or a tan sari lands on people as well - and the difference between a
/// quarter of it and all of it is usually the difference between passing and failing the
/// measured ceiling on the first try.
pub const SKIN_BAND_ATTENUATION: f32 = 0.25;

/// How much survives landing on a band next to skin.
pub const SKIN_ADJACENT_ATTENUATION: f32 = 0.60;

/// How much of the scene's vibrance intent a frame that is already saturated receives.
///
/// Vibrance is defined as saturation weighted away from what is already saturated, so
/// applying the full intent to a frame full of colour is applying it to the only parts that
/// do not want it. The scaling is by the frame's own mean saturation, which is the measure
/// vibrance itself is built on.
pub const VIBRANCE_SATURATION_FLOOR: f32 = 0.55;

/// What the band solver produced.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Solved {
    /// The eight bands.
    pub hsl: HslAdjustments,
    /// Vibrance, in recipe units.
    pub vibrance: f32,
    /// Flat saturation, in recipe units. Nearly always zero.
    pub saturation: f32,
    /// The reasons this module added, in the order they were made.
    pub reasons: Vec<Explained>,
}

impl Solved {
    /// True when nothing at all was adjusted.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.hsl.is_neutral() && self.vibrance.abs() < 0.5 && self.saturation.abs() < 0.5
    }

    /// Everything scaled toward zero, for the skin guard's re-solve and the subtlety cap.
    #[must_use]
    pub fn attenuated(&self, factor: f32) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        Self {
            hsl: self.hsl.attenuated(factor),
            vibrance: self.vibrance * factor,
            saturation: self.saturation * factor,
            reasons: self.reasons.clone(),
        }
    }
}

/// Turn one frame's objective into the eight bands and the two global saturation controls.
///
/// `mean_saturation` is the frame's own, used only to scale the vibrance intent.
#[must_use]
pub fn solve(plan: &Plan, intent: &SceneIntent, mean_saturation: f32) -> Solved {
    let mut hsl = HslAdjustments::default();
    let prohibited = plan.prohibited();

    for goal in &plan.goals {
        if goal.is_prohibition {
            continue;
        }
        let shift = shift_for(goal, intent);
        if shift.is_neutral() {
            continue;
        }
        // Goals accumulate rather than replace: two content bands can land on one recipe
        // band - foliage and a green sign both sit in green - and taking the last one would
        // make the answer depend on the order the goals were formed in.
        let existing = hsl.get(goal.band);
        hsl.set(
            goal.band,
            HslShift {
                h: existing.h + shift.h,
                s: existing.s + shift.s,
                l: existing.l + shift.l,
            },
        );
    }

    // The dress prohibition, applied after accumulation so it cannot be undone by a goal
    // formed later.
    for band in prohibited {
        let shift = hsl.get(band);
        hsl.set(
            band,
            HslShift {
                h: 0.0,
                s: shift.s.min(0.0),
                l: shift.l.min(0.0),
            },
        );
    }

    // The per-band cap, applied last, so that accumulation cannot exceed what the scene
    // allows on any single band however many goals landed on it.
    let cap = intent.max_band_shift.max(0.0);
    for band in HslBand::ALL {
        let shift = hsl.get(band);
        hsl.set(
            band,
            HslShift {
                h: shift.h.clamp(-cap, cap),
                s: shift.s.clamp(-cap, cap),
                l: shift.l.clamp(-cap, cap),
            },
        );
    }

    // Vibrance and flat saturation. Every shipped scene row sets saturation to zero and the
    // loader has a test that says so, but the field is carried rather than assumed away:
    // phase 17 replaces these numbers with a photographer's own, and a photographer who wants
    // flat saturation should get it.
    let headroom =
        ((VIBRANCE_SATURATION_FLOOR - mean_saturation) / VIBRANCE_SATURATION_FLOOR).clamp(0.0, 1.0);
    let vibrance = intent.vibrance * headroom;
    let saturation = intent.saturation;

    let mut reasons = plan.reasons.clone();
    if hsl.is_neutral() && vibrance.abs() < 0.5 && saturation.abs() < 0.5 {
        reasons.push(codec::plain(ColourCode::HslNeutral, 0.0));
    } else {
        // Every frame this phase adjusts a band on says that the bands were inferred from
        // colour statistics rather than segmented. ADR-0033 decision 4: the caveat travels
        // with the adjustment, so a photographer reading the panel knows how much to trust
        // it, and it stops being emitted the day phase 18's masks arrive.
        reasons.push(codec::plain(ColourCode::ContentInferred, -0.04));
    }

    Solved {
        hsl,
        vibrance,
        saturation,
        reasons,
    }
}

/// One goal as a band shift, in recipe units, with the skin attenuation applied.
fn shift_for(goal: &Goal, intent: &SceneIntent) -> HslShift {
    let confidence = goal.confidence.clamp(0.0, 1.0);
    let attenuation = match goal.band {
        band if band.is_skin_band() => SKIN_BAND_ATTENUATION,
        band if band.is_skin_adjacent() => SKIN_ADJACENT_ATTENUATION,
        _ => 1.0,
    };
    let scale = confidence * attenuation;
    let cap = intent.band_cap(confidence);
    HslShift {
        h: (goal.hue_deg / DEGREES_PER_UNIT * scale).clamp(-cap, cap),
        s: (goal.saturation / SATURATION_PER_UNIT * scale).clamp(-cap, cap),
        l: (goal.luma / LUMA_PER_UNIT * scale).clamp(-cap, cap),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::harmony::Goal;
    use crate::colour::intent::IntentTable;
    use aura_core::contract::colour::ContentBand;
    use aura_core::SceneId;

    fn intent_for(scene: SceneId) -> SceneIntent {
        IntentTable::embedded().map_or_else(
            |_| unreachable!("the shipped intents are embedded in the binary"),
            |table| table.get(scene),
        )
    }

    fn goal(band: HslBand, hue: f32, saturation: f32) -> Goal {
        Goal {
            content: ContentBand::Greenery,
            band,
            hue_deg: hue,
            saturation,
            luma: 0.0,
            confidence: 1.0,
            is_prohibition: false,
            region: None,
        }
    }

    #[test]
    fn a_hue_goal_becomes_the_units_the_renderer_reads() {
        // Five degrees at `DEGREES_PER_UNIT` is about 11 units, inside a venue's cap of 14 -
        // so this test measures the conversion rather than the clamp. A twelve-degree goal
        // would be 27 units and would arrive capped, which is the next test.
        let plan = Plan {
            goals: vec![goal(HslBand::Green, 5.0, 0.0)],
            reasons: Vec::new(),
        };
        let solved = solve(&plan, &intent_for(SceneId::Venue), 0.3);
        let shift = solved.hsl.get(HslBand::Green);
        assert!((shift.h - 5.0 / DEGREES_PER_UNIT).abs() < 0.01, "{shift:?}");
    }

    #[test]
    fn a_scenes_cap_bounds_every_band() {
        let plan = Plan {
            goals: vec![goal(HslBand::Green, 12.0, -0.30)],
            reasons: Vec::new(),
        };
        let intent = intent_for(SceneId::Ceremony);
        let solved = solve(&plan, &intent, 0.3);
        for band in HslBand::ALL {
            let shift = solved.hsl.get(band);
            assert!(shift.h.abs() <= intent.max_band_shift + 1e-3);
            assert!(shift.s.abs() <= intent.max_band_shift + 1e-3);
        }
    }

    #[test]
    fn the_skin_band_keeps_a_quarter_of_whatever_lands_on_it() {
        // The cheap half of the guarantee. An adjustment aimed at a tan surface lands on
        // faces, and it arrives at a quarter strength before the guard has measured anything.
        // A ten-percent saturation goal is ten units, inside the venue cap of fourteen, so
        // what this measures is the attenuation rather than the clamp.
        let plan = Plan {
            goals: vec![goal(HslBand::Orange, 0.0, -0.10)],
            reasons: Vec::new(),
        };
        let solved = solve(&plan, &intent_for(SceneId::Venue), 0.3);
        let orange = solved.hsl.get(HslBand::Orange).s;
        let plan_green = Plan {
            goals: vec![goal(HslBand::Green, 0.0, -0.10)],
            reasons: Vec::new(),
        };
        let green = solve(&plan_green, &intent_for(SceneId::Venue), 0.3)
            .hsl
            .get(HslBand::Green)
            .s;
        assert!(
            (orange / green - SKIN_BAND_ATTENUATION).abs() < 0.01,
            "orange {orange} against green {green}"
        );
    }

    #[test]
    fn a_dress_band_can_lose_saturation_and_never_gain_it() {
        let mut prohibition = goal(HslBand::Yellow, 0.0, 0.0);
        prohibition.content = ContentBand::Dress;
        prohibition.is_prohibition = true;
        let plan = Plan {
            goals: vec![goal(HslBand::Yellow, 8.0, 0.25), prohibition],
            reasons: Vec::new(),
        };
        let solved = solve(&plan, &intent_for(SceneId::GettingReadyBride), 0.2);
        let shift = solved.hsl.get(HslBand::Yellow);
        assert!(shift.s <= 0.0, "a dress band gained saturation: {shift:?}");
        assert!(shift.h.abs() < 1e-6, "a dress band was rotated: {shift:?}");
    }

    #[test]
    fn two_goals_on_one_band_accumulate_rather_than_replace() {
        let plan = Plan {
            goals: vec![
                goal(HslBand::Green, 4.0, 0.0),
                goal(HslBand::Green, 4.0, 0.0),
            ],
            reasons: Vec::new(),
        };
        let one = solve(
            &Plan {
                goals: vec![goal(HslBand::Green, 4.0, 0.0)],
                reasons: Vec::new(),
            },
            &intent_for(SceneId::Venue),
            0.3,
        );
        let two = solve(&plan, &intent_for(SceneId::Venue), 0.3);
        assert!(two.hsl.get(HslBand::Green).h > one.hsl.get(HslBand::Green).h);
    }

    #[test]
    fn a_saturated_frame_gets_less_vibrance_than_a_pale_one() {
        let plan = Plan::default();
        let pale = solve(&plan, &intent_for(SceneId::Details), 0.05).vibrance;
        let saturated = solve(&plan, &intent_for(SceneId::Details), 0.60).vibrance;
        assert!(pale > saturated);
        assert!(saturated.abs() < 1e-6);
    }

    #[test]
    fn an_adjusted_frame_always_says_the_content_was_inferred() {
        let plan = Plan {
            goals: vec![goal(HslBand::Green, 8.0, -0.10)],
            reasons: Vec::new(),
        };
        let solved = solve(&plan, &intent_for(SceneId::Venue), 0.3);
        assert!(solved
            .reasons
            .iter()
            .any(|(reason, _)| reason.code == ColourCode::ContentInferred));
    }
}
