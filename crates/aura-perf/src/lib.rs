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
//!
//! ## One budget, two kinds of machine
//!
//! A timing budget is a statement about *this code on that hardware*, and the two hosts this
//! product is tested on are three to five times apart. Every figure in `perf/budgets.toml` is
//! measured on a development machine, and a shared CI runner cannot meet them - phase 14's
//! proxy render was measured at 210 ms in release on a developer's laptop and at 497, 669 and
//! 1,123 ms on three GitHub runners.
//!
//! There are two wrong ways to resolve that. Raising every figure to the slowest runner
//! destroys the guardrail on the machine anybody actually develops on; leaving them alone
//! means CI is permanently red and stops being read.
//!
//! So a timing budget is multiplied by [`host_scale`], read once from `AURA_PERF_HOST_SCALE`,
//! which CI sets and a developer does not. The budget file keeps the developer-machine
//! numbers, and the assertion stays real on both: at the scale CI sets, a genuine regression
//! still fails.
//!
//! **It applies to timings only.** [`Budgets::check_size`], [`Budgets::check_count`] and
//! [`Budgets::check_cost`] ignore it, because a byte is a byte, a call is a call and a dollar
//! is a dollar on any machine. A slow runner is not a reason to store more.
//!
//! ## Why the scale is an argument and not only a variable
//!
//! [`Budgets::check`] is a thin wrapper over [`Budgets::check_at_scale`] at whatever the host
//! says. The split exists because the first version read the environment inside `check`, and
//! that quietly made every test of what a budget *means* depend on what CI had set: a case
//! asserting that 900 ms breaches a 400 ms budget stopped breaching the moment the runner
//! exported a scale of four, and the suite went red for a reason that had nothing to do with
//! the code under test. A measurement's verdict depends on the host; the *rule* does not.
//! Tests of the rule pass their own scale, and nothing in the crate but [`host_scale`] reads
//! the process environment.

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

/// The environment variable that relaxes timing budgets for slower hosts.
///
/// Unset - the developer machine - is a scale of one and the budget file's own figures apply.
pub const HOST_SCALE_VAR: &str = "AURA_PERF_HOST_SCALE";

/// The largest relaxation that will be honoured.
///
/// Eight. Past this the variable has stopped being a hardware allowance and started being a
/// way to make a red build green, and a budget that can be switched off from the environment
/// is not a budget. A larger value is clamped here rather than refused, because a CI change
/// that typed an extra zero should degrade to the ceiling rather than fail every perf suite.
pub const MAX_HOST_SCALE: u64 = 8;

/// How much slower this host is allowed to be than the machine the budgets were measured on.
///
/// One unless `AURA_PERF_HOST_SCALE` names an integer between 1 and [`MAX_HOST_SCALE`].
/// Anything unparseable is one: a typo must tighten the assertion rather than loosen it.
#[must_use]
pub fn host_scale() -> u64 {
    scale_from(std::env::var(HOST_SCALE_VAR).ok().as_deref())
}

/// [`host_scale`] without the environment, so the parsing rules can be tested without a
/// process-wide variable that leaks into whatever test runs next.
#[must_use]
pub fn scale_from(raw: Option<&str>) -> u64 {
    raw.and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_HOST_SCALE)
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
        self.check_at_scale(measurement, host_scale())
    }

    /// [`Budgets::check`] against an explicit host scale rather than the environment.
    ///
    /// What a budget *means* is not a property of the machine reading it, so the tests that
    /// assert the meaning call this and are unaffected by what CI sets. `scale` is clamped to
    /// `1..=`[`MAX_HOST_SCALE`] here as well as in [`host_scale`]: a caller cannot switch a
    /// budget off by passing a large number any more than a CI file can by setting one.
    ///
    /// # Errors
    ///
    /// Returns a human-readable breach description.
    pub fn check_at_scale(&self, measurement: &Measurement, scale: u64) -> Result<(), String> {
        let Some(budget) = self.stage.get(&measurement.stage) else {
            return Ok(());
        };
        let scale = scale.clamp(1, MAX_HOST_SCALE);
        // The message names the budget file's own figure *and* what this host was allowed, so
        // a failing CI log says which of the two it breached without anybody going to look up
        // the variable.
        let note = |allowed: u64, stated: u64| {
            if scale == 1 {
                format!("budget {stated} ms")
            } else {
                format!("budget {allowed} ms ({stated} ms at host scale {scale})")
            }
        };
        let elapsed_ceiling = budget.max_elapsed_ms.saturating_mul(scale);
        if measurement.elapsed_ms > elapsed_ceiling {
            return Err(format!(
                "{} took {} ms, {}",
                measurement.stage,
                measurement.elapsed_ms,
                note(elapsed_ceiling, budget.max_elapsed_ms)
            ));
        }
        if budget.max_ms_per_unit > 0 && measurement.units > 0 {
            let per_unit = measurement.elapsed_ms / measurement.units.max(1);
            let unit_ceiling = budget.max_ms_per_unit.saturating_mul(scale);
            if per_unit > unit_ceiling {
                return Err(format!(
                    "{} took {} ms per unit, {}",
                    measurement.stage,
                    per_unit,
                    note(unit_ceiling, budget.max_ms_per_unit)
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
