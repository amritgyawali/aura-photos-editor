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
