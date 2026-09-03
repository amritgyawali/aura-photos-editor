//! The three autonomy modes, and the one thing they are structurally unable to do.
//!
//! Section 2.1 asks for `Conservative`, `Balanced` and `Aggressive`. Section 6.4 says what
//! separates them from everything else in this crate: "modes shift thresholds and
//! k-values, **never the coverage rules** - even `Aggressive` cannot drop a must-have."
//!
//! This module is how that sentence stops being a promise.
//!
//! [`Tuning`] is what a mode produces, and it has four fields: a score floor, a keeper cap,
//! a target scale and a confidence flag. There is no field for a must-have, an identity
//! minimum, a rule id or a coverage state, and this module does not import
//! [`RuleTable`](crate::rules::RuleTable) at all. A future change that wanted an
//! `Aggressive` mode which "just skips the venue shot" would have to add a field, import
//! the rule table and get both past review - which is exactly the amount of friction the
//! decision deserves.
//!
//! ## Why the shifts are asymmetric
//!
//! `Conservative` moves the floor down by 0.08 and `Aggressive` moves it up by 0.07. The
//! asymmetry is in `cull_weights.toml` rather than here, but the reason belongs with the
//! modes: the two failures are not equally expensive. An over-full gallery costs a
//! photographer ten minutes with the slider. An over-culled one costs a frame nobody can
//! get back.
//!
//! ## Why `Conservative` also flags more
//!
//! Section 2.1 says it "keeps more, flags more", which is two behaviours and only one of
//! them is a threshold. The second is [`Tuning::flag_below`]: the confidence under which a
//! keeper is offered for review. Keeping more frames necessarily means keeping less
//! certain ones, and a mode that quietly filled a gallery with decisions it was unsure of
//! would be worse than one that culled hard and said so.

use aura_core::{CullMode, SceneId};

use crate::weights::{SceneWeights, WeightTable, MAX_KEEPERS_CAP};

/// One scene's thresholds after a mode has moved them.
///
/// Four numbers, and what is absent is the point. See this module's header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    /// The fused score below which a frame is not kept unless a guarantee holds it.
    pub floor: f32,
    /// The most frames one moment of this scene may contribute.
    pub max_keepers: u32,
    /// Confidence below which a keeper is offered for review.
    pub flag_below: f32,
    /// True when this scene had no weight row and the neutral one was substituted.
    pub unweighted: bool,
}

/// The confidence at which `Balanced` offers a keeper for review.
///
/// Below this a decision is uncertain enough that a photographer should look. It is not a
/// rejection and nothing acts on it: `Selected::confidence` is already on every keeper,
/// and this is only the line the panel draws.
pub const FLAG_BELOW_BALANCED: f32 = 0.45;

/// How much further `Conservative` raises that line.
///
/// Section 2.1's "flags more", as one number. A mode that keeps less certain frames has to
/// say which ones they are.
pub const FLAG_BELOW_CONSERVATIVE_DELTA: f32 = 0.15;

/// How much `Aggressive` lowers it.
///
/// A photographer who asked for a tight gallery has said they trust the engine, and a
/// tight gallery covered in review flags is a tight gallery that saved nobody any time.
pub const FLAG_BELOW_AGGRESSIVE_DELTA: f32 = -0.10;

/// Apply a mode to one scene's weights.
///
/// The one function in this crate that turns a preference into a number, and the only
/// place a `CullMode` is read at all.
#[must_use]
pub fn tune(table: &WeightTable, mode: CullMode, scene: SceneId) -> Tuning {
    let (weights, unweighted) = table.for_scene(scene);
    tune_row(table, mode, weights, unweighted)
}

/// Apply a mode to a row that has already been looked up.
///
/// Separate from [`tune`] because the moment pass has the row in hand and looking it up
/// again per frame would be a `BTreeMap` probe per photograph in a wedding.
#[must_use]
pub fn tune_row(
    table: &WeightTable,
    mode: CullMode,
    weights: SceneWeights,
    unweighted: bool,
) -> Tuning {
    let shift = table.mode(mode);
    let floor = (weights.floor + shift.floor_delta).clamp(0.0, 0.95);
    let keepers = i64::from(weights.max_keepers) + i64::from(shift.keeper_delta);
    let max_keepers = keepers.clamp(1, i64::from(MAX_KEEPERS_CAP));
    Tuning {
        floor,
        // Always at least one. A mode may not reduce a moment to zero keepers: that is
        // the difference between culling a moment and deleting it, and phase 08's whole
        // grouping exists so that every moment is represented.
        max_keepers: u32::try_from(max_keepers).unwrap_or(1),
        flag_below: (FLAG_BELOW_BALANCED + flag_delta(mode)).clamp(0.0, 1.0),
        unweighted,
    }
}

/// How much the target gallery size moves in this mode.
///
/// Read by [`sizing`](crate::sizing) and by nothing else. It is a scale rather than a
/// count because the prediction it multiplies is itself a function of the wedding, and a
/// mode that added a fixed two hundred frames would double a small wedding and barely
/// touch a large one.
#[must_use]
pub fn target_scale(table: &WeightTable, mode: CullMode) -> f32 {
    table.mode(mode).target_scale
}

fn flag_delta(mode: CullMode) -> f32 {
    match mode {
        CullMode::Conservative => FLAG_BELOW_CONSERVATIVE_DELTA,
        CullMode::Balanced => 0.0,
        CullMode::Aggressive => FLAG_BELOW_AGGRESSIVE_DELTA,
    }
}

#[cfg(test)]
mod tests {
    use aura_core::{CullMode, SceneId};

    use super::{
        target_scale, tune, FLAG_BELOW_AGGRESSIVE_DELTA, FLAG_BELOW_BALANCED,
        FLAG_BELOW_CONSERVATIVE_DELTA,
    };
    use crate::weights::{WeightTable, MAX_KEEPERS_CAP};

    fn table() -> WeightTable {
        WeightTable::embedded().expect("the shipped weight table has to load")
    }

    #[test]
    fn the_three_modes_order_the_floor_the_way_they_are_described() {
        // "Keeps more" and "keeps fewer" are the whole of what a mode is, and the floor is
        // where that happens. Conservative below balanced below aggressive, on every scene.
        let table = table();
        for scene in [SceneId::Vows, SceneId::DanceFloor, SceneId::Unknown] {
            let conservative = tune(&table, CullMode::Conservative, scene).floor;
            let balanced = tune(&table, CullMode::Balanced, scene).floor;
            let aggressive = tune(&table, CullMode::Aggressive, scene).floor;
            assert!(
                conservative < balanced && balanced < aggressive,
                "{scene:?}: {conservative} {balanced} {aggressive}"
            );
        }
    }

    #[test]
    fn the_asymmetry_favours_keeping_a_frame_nobody_can_get_back() {
        // "An over-full gallery costs a photographer ten minutes with the slider. An
        // over-culled one costs a frame nobody can get back." The two shifts are therefore not
        // the same size, and the larger one is the one that keeps more.
        let table = table();
        let balanced = tune(&table, CullMode::Balanced, SceneId::Vows).floor;
        let down = balanced - tune(&table, CullMode::Conservative, SceneId::Vows).floor;
        let up = tune(&table, CullMode::Aggressive, SceneId::Vows).floor - balanced;
        assert!(
            down > up,
            "conservative moved {down}, aggressive moved {up}"
        );
    }

    #[test]
    fn no_mode_can_reduce_a_moment_to_no_keepers() {
        // The difference between culling a moment and deleting it. Phase 08's whole grouping
        // exists so that every moment is represented.
        let table = table();
        for mode in [
            CullMode::Conservative,
            CullMode::Balanced,
            CullMode::Aggressive,
        ] {
            for scene in SceneId::ALL {
                let tuning = tune(&table, mode, scene);
                assert!(tuning.max_keepers >= 1, "{mode:?}/{scene:?}");
                assert!(tuning.max_keepers <= MAX_KEEPERS_CAP, "{mode:?}/{scene:?}");
            }
        }
    }

    #[test]
    fn conservative_flags_more_and_aggressive_flags_less() {
        // Section 2.1's "keeps more, flags more" is two behaviours, and only one of them is a
        // threshold. A mode that quietly filled a gallery with decisions it was unsure of
        // would be worse than one that culled hard and said so.
        let table = table();
        let conservative = tune(&table, CullMode::Conservative, SceneId::Vows).flag_below;
        let balanced = tune(&table, CullMode::Balanced, SceneId::Vows).flag_below;
        let aggressive = tune(&table, CullMode::Aggressive, SceneId::Vows).flag_below;
        assert!((balanced - FLAG_BELOW_BALANCED).abs() < f32::EPSILON);
        assert!(
            (conservative - (FLAG_BELOW_BALANCED + FLAG_BELOW_CONSERVATIVE_DELTA)).abs() < 1e-6
        );
        assert!((aggressive - (FLAG_BELOW_BALANCED + FLAG_BELOW_AGGRESSIVE_DELTA)).abs() < 1e-6);
    }

    #[test]
    fn the_mode_scale_orders_the_gallery_sizes_the_same_way() {
        let table = table();
        assert!(
            target_scale(&table, CullMode::Conservative) > target_scale(&table, CullMode::Balanced)
                && target_scale(&table, CullMode::Balanced)
                    > target_scale(&table, CullMode::Aggressive)
        );
    }

    #[test]
    fn a_scene_with_no_row_of_its_own_says_so_rather_than_borrowing_silently() {
        // An unweighted scene is judged against a *documented* neutral row rather than a
        // guess, which is a weaker claim and not a worse one - but only because the tuning
        // carries the fact, which is what pays for the confidence penalty downstream.
        let table = table();
        let tuning = tune(&table, CullMode::Balanced, SceneId::Unknown);
        assert!(tuning.unweighted);
        assert!(!tune(&table, CullMode::Balanced, SceneId::Vows).unweighted);
    }

    #[test]
    fn a_tuning_has_no_field_a_coverage_rule_could_be_expressed_in() {
        // Section 6.4: "modes shift thresholds and k-values, **never the coverage rules** -
        // even `Aggressive` cannot drop a must-have." This module does not import the rule
        // table at all, and the four fields below are all a mode can produce. The assertion is
        // a compile-time one: the destructuring fails to build if a fifth field appears, which
        // is the review friction the decision deserves.
        let table = table();
        let super::Tuning {
            floor: _,
            max_keepers: _,
            flag_below: _,
            unweighted: _,
        } = tune(&table, CullMode::Aggressive, SceneId::Vows);
    }
}
