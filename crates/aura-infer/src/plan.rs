//! Reading and writing `hardware_plan.json`.
//!
//! The plan is a cache of measurements, never a source of truth. Everything in it
//! can be recomputed by measuring the machine again, which decides how every
//! failure here is handled: a plan that will not parse is replaced rather than
//! repaired, and a plan from a future schema is discarded rather than guessed at.
//!
//! Writes are atomic - a temporary file beside the target, then a rename - so a
//! machine that loses power mid-write starts next time with the old plan or the
//! new one, never with half of either.

use std::fs;
use std::path::{Path, PathBuf};

use aura_core::AuraResult;

use crate::contract::infer::{ExecutionProvider, HardwarePlan, SetAsideProvider};
use crate::errors::plan_unreadable;

/// The file name inside the application data directory.
pub const PLAN_FILE: &str = "hardware_plan.json";

/// The probe's reference model, written beside the plan.
pub const PROBE_MODEL_FILE: &str = "hardware_probe.onnx";

/// Where the plan and its probe model live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPaths {
    /// Directory holding both files.
    pub directory: PathBuf,
}

impl PlanPaths {
    /// Point at a directory.
    #[must_use]
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.to_path_buf(),
        }
    }

    /// The plan file.
    #[must_use]
    pub fn plan(&self) -> PathBuf {
        self.directory.join(PLAN_FILE)
    }

    /// The probe's reference model.
    #[must_use]
    pub fn probe_model(&self) -> PathBuf {
        self.directory.join(PROBE_MODEL_FILE)
    }
}

/// Read a plan.
///
/// `Ok(None)` means there is no plan yet - the first run on this machine, which
/// is not an error. An unreadable or future-schema plan is `AURA-GPU-4005`, and
/// the caller re-probes.
///
/// # Errors
///
/// `AURA-GPU-4005` when a plan exists but cannot be used.
pub fn load(paths: &PlanPaths) -> AuraResult<Option<HardwarePlan>> {
    let path = paths.plan();
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)
        .map_err(|err| plan_unreadable(format!("plan could not be read: {err}")))?;
    let plan: HardwarePlan = serde_json::from_str(&text)
        .map_err(|err| plan_unreadable(format!("plan could not be parsed: {err}")))?;
    if plan.schema != HardwarePlan::SCHEMA {
        return Err(plan_unreadable(format!(
            "plan schema {} is not the {} this build writes",
            plan.schema,
            HardwarePlan::SCHEMA
        )));
    }
    if plan.ep_order.is_empty() {
        return Err(plan_unreadable(
            "plan names no execution providers".to_string(),
        ));
    }
    Ok(Some(plan))
}

/// Write a plan atomically.
///
/// # Errors
///
/// `AURA-GPU-4005` when the directory cannot be created or the file cannot be
/// written or renamed.
pub fn save(paths: &PlanPaths, plan: &HardwarePlan) -> AuraResult<()> {
    fs::create_dir_all(&paths.directory)
        .map_err(|err| plan_unreadable(format!("plan directory could not be created: {err}")))?;
    let rendered = serde_json::to_string_pretty(plan)
        .map_err(|err| plan_unreadable(format!("plan could not be rendered: {err}")))?;

    let target = paths.plan();
    let temporary = paths.directory.join(format!("{PLAN_FILE}.tmp"));
    fs::write(&temporary, rendered.as_bytes())
        .map_err(|err| plan_unreadable(format!("plan could not be written: {err}")))?;

    // Windows refuses a rename onto an existing file, so the old plan goes first.
    // The window between the two is why the plan is a cache: losing it costs one
    // re-probe, and a torn plan is impossible because the content only ever
    // arrives by rename.
    if target.exists() {
        drop(fs::remove_file(&target));
    }
    fs::rename(&temporary, &target).map_err(|err| {
        drop(fs::remove_file(&temporary));
        plan_unreadable(format!("plan could not be renamed into place: {err}"))
    })?;
    Ok(())
}

/// Set a provider aside on this machine, with a reason and a timestamp.
pub fn set_aside(plan: &mut HardwarePlan, ep: ExecutionProvider, reason: &str, at: &str) {
    if plan.is_set_aside(ep) {
        return;
    }
    plan.set_aside.push(SetAsideProvider {
        ep,
        reason: reason.to_string(),
        at: at.to_string(),
    });
    plan.ep_order.retain(|candidate| *candidate != ep);
    if plan.ep_order.is_empty() {
        plan.ep_order.push(ExecutionProvider::Cpu);
    }
}

/// Clear every set-aside entry, as "Re-check hardware" does.
pub fn clear_set_aside(plan: &mut HardwarePlan) {
    plan.set_aside.clear();
}

/// Remember the batch size that worked for a model on this machine.
pub fn remember_batch(plan: &mut HardwarePlan, model: &str, batch: u16) {
    plan.remembered_batch
        .insert(model.to_string(), batch.max(1));
}
