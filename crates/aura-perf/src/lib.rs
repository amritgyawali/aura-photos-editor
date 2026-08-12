#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! Budgets are assertions, not hopes.
//!
//! Every performance budget in a phase document becomes a value in `perf/budgets.toml`
//! and a test that fails when the measurement exceeds it.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::clock::Clock;
use serde::{Deserialize, Serialize};

/// One measured stage: wall clock and the unit count it covered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measurement {
    /// Stage name, matching the key in `perf/budgets.toml`.
    pub stage: String,
    /// Wall-clock milliseconds.
    pub elapsed_ms: u64,
    /// Units processed, for example files indexed.
    pub units: u64,
}

impl Measurement {
    /// Units per second, or zero when nothing was measured.
    #[must_use]
    pub fn units_per_second(&self) -> f64 {
        if self.elapsed_ms == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.units as f64 * 1000.0 / self.elapsed_ms as f64
        }
    }
}

/// A running stage timer that reads the clock exactly twice.
#[derive(Debug)]
pub struct StageTimer<'a> {
    stage: String,
    started_ms: u64,
    clock: &'a dyn Clock,
}

impl<'a> StageTimer<'a> {
    /// Start timing `stage`.
    #[must_use]
    pub fn start(stage: impl Into<String>, clock: &'a dyn Clock) -> Self {
        Self {
            stage: stage.into(),
            started_ms: clock.monotonic_ms(),
            clock,
        }
    }

    /// Stop timing and record how many units the stage covered.
    #[must_use]
    pub fn finish(self, units: u64) -> Measurement {
        Measurement {
            elapsed_ms: self.clock.monotonic_ms().saturating_sub(self.started_ms),
            stage: self.stage,
            units,
        }
    }
}

/// One budget line: the ceiling a stage must stay under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Maximum wall clock for the whole stage.
    pub max_elapsed_ms: u64,
    /// Maximum milliseconds per unit, or zero when the stage has no per-unit budget.
    #[serde(default)]
    pub max_ms_per_unit: u64,
}

/// One size ceiling. Bytes are budgets too: a preview cache that quietly grows
/// fills the drive the wedding is being edited from, and peak resident memory
/// decides whether a 16 GB laptop can proxy a whole wedding at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizeBudget {
    /// Maximum bytes.
    pub max_bytes: u64,
}

/// One count ceiling.
///
/// Added in phase 04. Not everything worth budgeting is a duration or a size:
/// "at most 75 cloud calls for a 3,000 image wedding" is the budget that keeps
/// the cost budget true, and a run that met its millisecond targets by making
/// four hundred calls would have passed every other kind of check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountBudget {
    /// Maximum number of whatever is being counted.
    pub max_count: u64,
}

/// One money ceiling, in hundredths of a US cent.
///
/// An integer unit, because this file is read by people as well as by tests and
/// a float in a budget invites an argument about rounding. 15,000 is USD 1.50.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostBudget {
    /// Maximum spend, in hundredths of a US cent.
    pub max_usd_hundredths_of_a_cent: u64,
}

impl CostBudget {
    /// The ceiling in US dollars.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn max_usd(self) -> f64 {
        // A budget in dollars never approaches 2^53 hundredths of a cent.
        self.max_usd_hundredths_of_a_cent as f64 / 10_000.0
    }
}

/// The parsed contents of `perf/budgets.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Budgets {
    /// Budgets keyed by stage name. `BTreeMap` so reports are ordered.
    #[serde(default)]
    pub stage: BTreeMap<String, Budget>,
    /// Size ceilings keyed by name.
    #[serde(default)]
    pub size: BTreeMap<String, SizeBudget>,
    /// Count ceilings keyed by name.
    #[serde(default)]
    pub count: BTreeMap<String, CountBudget>,
    /// Money ceilings keyed by name.
    #[serde(default)]
    pub cost: BTreeMap<String, CostBudget>,
}

impl Budgets {
    /// Load budgets from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns the parse or read failure as a string; callers turn it into a test failure.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("read budgets: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("parse budgets: {e}"))
    }

    /// Check one measurement against its budget.
    ///
    /// Returns `Ok(())` when the stage has no budget: an unmeasured stage is not a
    /// failure, an over-budget one is.
    ///
    /// # Errors
    ///
    /// Returns a human-readable breach description.
    pub fn check(&self, measurement: &Measurement) -> Result<(), String> {
        let Some(budget) = self.stage.get(&measurement.stage) else {
            return Ok(());
        };
        if measurement.elapsed_ms > budget.max_elapsed_ms {
            return Err(format!(
                "{} took {} ms, budget {} ms",
                measurement.stage, measurement.elapsed_ms, budget.max_elapsed_ms
            ));
        }
        if budget.max_ms_per_unit > 0 && measurement.units > 0 {
            let per_unit = measurement.elapsed_ms / measurement.units.max(1);
            if per_unit > budget.max_ms_per_unit {
                return Err(format!(
                    "{} took {} ms per unit, budget {} ms",
                    measurement.stage, per_unit, budget.max_ms_per_unit
                ));
            }
        }
        Ok(())
    }

    /// Check a measured size against its ceiling.
    ///
    /// Returns `Ok(())` when the name has no budget, for the same reason
    /// [`Budgets::check`] does: an unmeasured thing is not a failure.
    ///
    /// # Errors
    ///
    /// Returns a human-readable breach description.
    pub fn check_size(&self, name: &str, measured_bytes: u64) -> Result<(), String> {
        let Some(budget) = self.size.get(name) else {
            return Ok(());
        };
        if measured_bytes > budget.max_bytes {
            return Err(format!(
                "{name} used {measured_bytes} bytes, budget {} bytes",
                budget.max_bytes
            ));
        }
        Ok(())
    }

    /// Check a measured count against its ceiling.
    ///
    /// # Errors
    ///
    /// Returns a human-readable breach description.
    pub fn check_count(&self, name: &str, measured: u64) -> Result<(), String> {
        let Some(budget) = self.count.get(name) else {
            return Ok(());
        };
        if measured > budget.max_count {
            return Err(format!(
                "{name} was {measured}, budget {}",
                budget.max_count
            ));
        }
        Ok(())
    }

    /// Check a measured spend against its ceiling.
    ///
    /// # Errors
    ///
    /// Returns a human-readable breach description.
    pub fn check_cost(&self, name: &str, measured_usd: f64) -> Result<(), String> {
        let Some(budget) = self.cost.get(name) else {
            return Ok(());
        };
        if measured_usd > budget.max_usd() {
            return Err(format!(
                "{name} cost ${measured_usd:.4}, budget ${:.4}",
                budget.max_usd()
            ));
        }
        Ok(())
    }
}
