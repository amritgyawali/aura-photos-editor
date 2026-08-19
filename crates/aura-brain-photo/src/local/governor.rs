//! The per-image budget: what stops six defensible adjustments becoming one obvious one.
//!
//! PHASE-19 section 6.4:
//!
//! > A per-image perceptual budget (measured as mean absolute change in a perceptual space)
//! > prevents accumulation across operations; when the budget is exhausted, operations are
//! > scaled down in priority order (face lighting first, dodge/burn last).
//!
//! ## The reading of that sentence
//!
//! "Face lighting first, dodge/burn last" is the **priority** order, not the scaling order.
//! Face lighting has the first claim on the budget and dodge and burn the last, so what gets
//! given up when the allowance runs out is the shaping.
//! `docs/adr/ADR-0033-local-light-sculpting.md` section 5 records why that reading and not
//! the other: face lighting is the operation section 1 exists for, dodge and burn is both the
//! most decorative and the most artefact-prone, and a budget that protected the shaping and
//! gave up the lift would be spending the allowance on the part a photographer would not
//! miss.
//!
//! ## How the cost is measured
//!
//! Mean absolute change over the frame, in the perceptual space
//! [`crate::local::measure`] works in. For each operation that is the size of the change it
//! makes inside its region, times the fraction of the frame that region covers. A one-stop
//! lift on a face covering two per cent of the frame costs very little; the same lift on a
//! subject covering half of it costs a great deal, and that asymmetry is correct - the second
//! photograph *has* visibly changed.
//!
//! Absolute rather than signed, and summed rather than composed: two operations that cancel
//! each other out have still both changed the photograph, and pretending otherwise is how a
//! frame ends up with a lift and a reduction fighting inside the same skin.

use aura_core::contract::local::{
    BackgroundBalanceDelta, DodgeBurnMaps, FaceLightDelta, LocalOp, ShineReduction,
    SubjectEnhanceDelta, PERCEPTUAL_BUDGET,
};

use crate::local::measure::apply_ev;

/// How much of a clarity unit's change reads as a luminance change.
///
/// Clarity and texture are local-contrast operators: they move pixels apart around a local
/// mean without moving the mean. So their contribution to a *mean absolute* change is real but
/// much smaller than an exposure move of the same nominal size. A hundredth of a perceptual
/// unit per ten units of clarity is what the reference render measures on the
/// `crate::local::fixtures` faces.
pub const CLARITY_TO_LUMA: f32 = 0.001;

/// The same, for contrast.
pub const CONTRAST_TO_LUMA: f32 = 0.0008;

/// The same, for saturation. Saturation moves no luminance at all in a correct
/// implementation, but it is visible, so it is charged at a quarter of contrast's rate rather
/// than at nothing - an operation that costs nothing is an operation that runs everywhere.
pub const SATURATION_TO_LUMA: f32 = 0.0002;

/// What one operation would cost, and what it was allowed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Charge {
    /// The mean absolute perceptual change the operation would make at full strength.
    pub cost: f32,
    /// The fraction of that cost the governor allowed, `0..1`.
    pub allowed: f32,
}

/// The whole frame's spending.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ledger {
    /// One charge per operation, in [`LocalOp::PRIORITY`] order.
    pub charges: [Charge; LocalOp::COUNT],
    /// The fraction of the allowance actually spent, `0..1`.
    pub spent: f32,
    /// The allowance this frame had, in perceptual units.
    pub allowance: f32,
    /// True when something was given up.
    pub exhausted: bool,
}

impl Ledger {
    /// The scale factor one operation ended up with.
    #[must_use]
    pub fn allowed(&self, op: LocalOp) -> f32 {
        self.charges.get(op.rank()).map_or(0.0, |c| c.allowed)
    }
}

/// What the face lighting costs.
///
/// `area` is the fraction of the frame the faces cover in total.
#[must_use]
pub fn face_cost(deltas: &[FaceLightDelta], area: f32) -> f32 {
    if deltas.is_empty() {
        return 0.0;
    }
    let mean_change = deltas
        .iter()
        .map(|d| (d.luma_after - d.luma_before).abs())
        .sum::<f32>()
        / deltas.len() as f32;
    mean_change * area.clamp(0.0, 1.0)
}

/// What the subject enhancement costs.
#[must_use]
pub fn subject_cost(delta: &SubjectEnhanceDelta, area: f32) -> f32 {
    let change = f32::from(delta.clarity).abs() * CLARITY_TO_LUMA
        + f32::from(delta.texture).abs() * CLARITY_TO_LUMA
        + f32::from(delta.contrast).abs() * CONTRAST_TO_LUMA;
    change * area.clamp(0.0, 1.0)
}

/// What the background balance costs.
#[must_use]
pub fn background_cost(delta: &BackgroundBalanceDelta, area: f32) -> f32 {
    let luma = (apply_ev(delta.mean_luma_before.max(0.05), delta.exposure_ev)
        - delta.mean_luma_before.max(0.05))
    .abs();
    let extra = f32::from(delta.highlights).abs() * CONTRAST_TO_LUMA
        + f32::from(delta.saturation).abs() * SATURATION_TO_LUMA;
    (luma + extra) * area.clamp(0.0, 1.0)
}

/// What the shine reduction costs.
#[must_use]
pub fn shine_cost(shine: &ShineReduction) -> f32 {
    (shine.peak_before - shine.peak_after).abs() * shine.area_fraction.clamp(0.0, 1.0)
}

/// What the shaping costs, per band.
///
/// The grids are in units of 1/200 stop, so the mean absolute grid value converted back to a
/// luminance change at the face's own level is the cost. Charged against the faces' area, as
/// everything else is charged against its own region.
#[must_use]
pub fn shaping_cost(maps: &DodgeBurnMaps, area: f32, low_band: bool) -> f32 {
    if maps.faces.is_empty() {
        return 0.0;
    }
    let mut total = 0.0f32;
    for face in &maps.faces {
        let band = if low_band {
            &face.low_freq
        } else {
            &face.mid_freq
        };
        if band.is_empty() {
            continue;
        }
        let mean_ev =
            band.iter().map(|v| f32::from(*v).abs()).sum::<f32>() / band.len() as f32 / 200.0;
        // Around a face's own mid-tone, one stop is about 0.28 perceptual units.
        total += mean_ev * 0.28;
    }
    total / maps.faces.len() as f32 * area.clamp(0.0, 1.0)
}

/// Decide what each operation may keep.
///
/// `costs` are the six full-strength costs in [`LocalOp::PRIORITY`] order. `scene_budget` is
/// the scene policy's own fraction of [`PERCEPTUAL_BUDGET`].
///
/// The allocation walks the priority order and gives each operation what is left. An
/// operation that cannot have all of what it asked for gets a fraction of it rather than
/// nothing - a half-strength lift is a defensible edit, and an operation that was silently
/// dropped is the failure mode section 12's third row names.
#[must_use]
pub fn allocate(costs: [f32; LocalOp::COUNT], scene_budget: f32) -> Ledger {
    let allowance = (PERCEPTUAL_BUDGET * scene_budget.clamp(0.0, 1.0)).max(0.0);
    let mut ledger = Ledger {
        charges: [Charge::default(); LocalOp::COUNT],
        spent: 0.0,
        allowance,
        exhausted: false,
    };
    let mut remaining = allowance;
    let mut used = 0.0f32;
    for (index, op) in LocalOp::PRIORITY.iter().enumerate() {
        let cost = costs.get(index).copied().unwrap_or(0.0).max(0.0);
        let allowed = if cost <= f32::EPSILON {
            // An operation that costs nothing is not "given the whole budget"; it did
            // nothing, and its scale is one so that a caller multiplying by it changes
            // nothing either.
            1.0
        } else if cost <= remaining {
            1.0
        } else {
            ledger.exhausted = true;
            (remaining / cost).clamp(0.0, 1.0)
        };
        let spent = cost * allowed;
        remaining = (remaining - spent).max(0.0);
        used += spent;
        if let Some(charge) = ledger.charges.get_mut(index) {
            *charge = Charge { cost, allowed };
        }
        let _ = op;
    }
    ledger.spent = if allowance <= f32::EPSILON {
        0.0
    } else {
        (used / allowance).clamp(0.0, 1.0)
    };
    ledger
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_frame_spends_nothing() {
        let ledger = allocate([0.0; LocalOp::COUNT], 1.0);
        assert_eq!(ledger.spent, 0.0);
        assert!(!ledger.exhausted);
        for op in LocalOp::PRIORITY {
            assert_eq!(ledger.allowed(op), 1.0);
        }
    }

    #[test]
    fn a_frame_inside_its_budget_keeps_everything() {
        let small = PERCEPTUAL_BUDGET / 100.0;
        let ledger = allocate([small; LocalOp::COUNT], 1.0);
        assert!(!ledger.exhausted);
        for op in LocalOp::PRIORITY {
            assert_eq!(ledger.allowed(op), 1.0);
        }
        assert!(ledger.spent < 0.10);
    }

    #[test]
    fn face_lighting_is_paid_first_and_shaping_last() {
        // Six operations that each want the whole allowance. Face lighting gets all of it and
        // dodge and burn gets none, which is section 6.4 read as the module header argues.
        let ledger = allocate([PERCEPTUAL_BUDGET; LocalOp::COUNT], 1.0);
        assert_eq!(ledger.allowed(LocalOp::FaceLight), 1.0);
        assert_eq!(ledger.allowed(LocalOp::DodgeBurnMid), 0.0);
        assert!(ledger.exhausted);
    }

    #[test]
    fn an_operation_that_cannot_have_it_all_gets_a_fraction_rather_than_nothing() {
        let half = PERCEPTUAL_BUDGET / 2.0;
        let mut costs = [0.0f32; LocalOp::COUNT];
        costs[LocalOp::FaceLight.rank()] = half;
        costs[LocalOp::SubjectEnhance.rank()] = PERCEPTUAL_BUDGET;
        let ledger = allocate(costs, 1.0);
        let allowed = ledger.allowed(LocalOp::SubjectEnhance);
        assert!(
            allowed > 0.0 && allowed < 1.0,
            "the subject half was dropped rather than scaled: {allowed}"
        );
    }

    #[test]
    fn a_tighter_scene_budget_exhausts_sooner() {
        // Six operations that together fit inside the full allowance and do not fit inside a
        // third of it. The generous frame keeps its shaping; the tight one gives it up.
        let costs = [PERCEPTUAL_BUDGET / 8.0; LocalOp::COUNT];
        let generous = allocate(costs, 1.0);
        let tight = allocate(costs, 0.30);
        assert!(tight.exhausted);
        assert!(tight.allowed(LocalOp::DodgeBurnLow) < generous.allowed(LocalOp::DodgeBurnLow));
    }

    #[test]
    fn spending_is_never_reported_above_the_allowance() {
        let ledger = allocate([PERCEPTUAL_BUDGET * 10.0; LocalOp::COUNT], 1.0);
        assert!(ledger.spent <= 1.0);
    }

    #[test]
    fn two_operations_that_cancel_still_both_cost() {
        // Absolute rather than signed. A lift and a matching reduction have both changed the
        // photograph, and a governor that netted them would let a frame carry twice the work
        // it was allowed.
        let lift = FaceLightDelta {
            luma_before: 0.40,
            luma_after: 0.50,
            ..FaceLightDelta::none(0.40)
        };
        let drop = FaceLightDelta {
            luma_before: 0.50,
            luma_after: 0.40,
            ..FaceLightDelta::none(0.50)
        };
        assert!(face_cost(&[lift, drop], 1.0) > 0.09);
    }
}
