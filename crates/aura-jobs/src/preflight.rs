//! What is checked before a two-hour run starts.
//!
//! Section 2.1: "fail fast with actionable messages before starting a two-hour job". Eight checks,
//! every one of which produces a sentence naming what to do rather than what is wrong.
//!
//! ## Why a check that cannot be performed warns rather than passes
//!
//! Phase 24's rule, at the point where it costs the most. A machine whose free disk cannot be read
//! is not a machine with enough disk; it is a machine nobody knows about, and the difference
//! between those two shows up two hours later. Every check here that has an unreadable input
//! produces [`PreflightVerdict::Warn`] with a sentence saying so, and none of them produces
//! `Pass`.
//!
//! ## Why `Block` is reserved for four of the eight
//!
//! A blocking pre-flight is the product refusing to start, and a product that refuses too eagerly
//! is a product people learn to click past. Four things block: a project that will not open, a
//! project with no photographs, a disk that cannot hold the output, and a missing model that a
//! *mandatory* stage needs. Everything else - hardware, cloud budget, calibration, battery - is a
//! warning, because every one of them leaves a run that still delivers something.
//!
//! The calibration row is the one worth reading twice. On this build it is always a warning and it
//! always fires, because phase 13's `calibration_ver` is 0 - and what it says is not "something is
//! broken" but "here is how much of this run you will be asked about".

use crate::contract::autopilot::{
    PreflightCheck, PreflightReport, PreflightRow, PreflightVerdict, StageId, DISK_HEADROOM,
};
use crate::stages;

/// Everything the pre-flight needs to know, gathered by the caller.
///
/// A struct of readings rather than a set of ports, because every one of these is something
/// `aura-app` already knows and nothing here needs to be mocked independently. The `Option`s are
/// the readings a machine may not expose.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    /// Whether the project's catalog opened and its schema is current.
    pub project_opens: bool,
    /// How many photographs are in it.
    pub images: u32,
    /// Free bytes on the volume it lives on, or `None` when unreadable.
    pub disk_free_bytes: Option<u64>,
    /// Bytes the run expects to write.
    pub estimated_output_bytes: u64,
    /// Whether a hardware plan could be made.
    pub hardware_ready: bool,
    /// What the hardware plan resolved to, for the message.
    pub hardware_detail: String,
    /// Models a mandatory stage needs that are not installed.
    pub missing_required_models: Vec<String>,
    /// Models an optional stage needs that are not installed.
    pub missing_optional_models: Vec<String>,
    /// The remaining cloud budget for this project in US dollars, or `None` when no stage in this
    /// run would make a call.
    pub cloud_budget_usd: Option<f32>,
    /// Whether this build's confidences are calibrated.
    pub calibrated: bool,
    /// Whether the machine is on battery.
    pub on_battery: bool,
    /// Whether the photographer said heavy stages may run on battery.
    pub allow_on_battery: bool,
    /// The stages this run will actually attempt, with the units each has.
    pub planned: Vec<(StageId, u32)>,
    /// How many of the enabled stages would be held by the autonomy gate.
    pub held_stages: u32,
    /// The disk headroom multiple this installation requires.
    pub disk_headroom: f32,
}

impl Default for Facts {
    fn default() -> Self {
        Self {
            project_opens: true,
            images: 0,
            disk_free_bytes: None,
            estimated_output_bytes: 0,
            hardware_ready: true,
            hardware_detail: String::new(),
            missing_required_models: Vec::new(),
            missing_optional_models: Vec::new(),
            cloud_budget_usd: None,
            calibrated: false,
            on_battery: false,
            allow_on_battery: false,
            planned: Vec::new(),
            held_stages: 0,
            disk_headroom: DISK_HEADROOM,
        }
    }
}

/// Run the eight checks.
#[must_use]
pub fn check(facts: &Facts) -> PreflightReport {
    let mut rows = Vec::with_capacity(PreflightCheck::ALL.len());

    rows.push(if facts.project_opens {
        row(
            PreflightCheck::ProjectIntegrity,
            PreflightVerdict::Pass,
            "This wedding opens and its catalog is up to date.",
        )
    } else {
        row(
            PreflightCheck::ProjectIntegrity,
            PreflightVerdict::Block,
            "AURA could not open this wedding's catalog. Open it once from the projects list, \
             which will migrate it, then start the run again.",
        )
    });

    rows.push(if facts.images == 0 {
        row(
            PreflightCheck::HasImages,
            PreflightVerdict::Block,
            "There are no photographs in this wedding yet. Import them first.",
        )
    } else {
        row(
            PreflightCheck::HasImages,
            PreflightVerdict::Pass,
            format!("{} photographs.", facts.images),
        )
    });

    rows.push(disk_row(facts));

    rows.push(if facts.hardware_ready {
        row(
            PreflightCheck::Hardware,
            PreflightVerdict::Pass,
            if facts.hardware_detail.is_empty() {
                "AURA can use this machine.".to_string()
            } else {
                facts.hardware_detail.clone()
            },
        )
    } else {
        row(
            PreflightCheck::Hardware,
            PreflightVerdict::Warn,
            "AURA could not work out what this machine can do, so it will run everything on the \
             processor. The run will finish; it will take considerably longer.",
        )
    });

    rows.push(models_row(facts));
    rows.push(budget_row(facts));
    rows.push(calibration_row(facts));
    rows.push(power_row(facts));

    let estimated_ms = facts
        .planned
        .iter()
        .map(|(stage, units)| {
            u64::from(stages::decl(*stage).est_ms_per_item).saturating_mul(u64::from(*units))
        })
        .sum();

    PreflightReport {
        rows,
        images: facts.images,
        estimated_output_bytes: facts.estimated_output_bytes,
        estimated_ms,
    }
}

fn disk_row(facts: &Facts) -> PreflightRow {
    let Some(free) = facts.disk_free_bytes else {
        return row(
            PreflightCheck::DiskSpace,
            PreflightVerdict::Warn,
            "AURA could not read how much room is left on this disk, so it cannot promise the \
             run will fit. Everything is checkpointed, so a full disk stops the run rather than \
             losing work.",
        );
    };
    #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
    let needed = (facts.estimated_output_bytes as f32 * facts.disk_headroom) as u64;
    if free >= needed {
        return row(
            PreflightCheck::DiskSpace,
            PreflightVerdict::Pass,
            format!(
                "{} free, and the run needs about {}.",
                gigabytes(free),
                gigabytes(needed)
            ),
        );
    }
    row(
        PreflightCheck::DiskSpace,
        PreflightVerdict::Block,
        format!(
            "This run needs about {} and there is {} free. Free up {} and start again.",
            gigabytes(needed),
            gigabytes(free),
            gigabytes(needed.saturating_sub(free))
        ),
    )
}

fn models_row(facts: &Facts) -> PreflightRow {
    if !facts.missing_required_models.is_empty() {
        return row(
            PreflightCheck::Models,
            PreflightVerdict::Block,
            format!(
                "A step this wedding cannot be delivered without needs a model that is not \
                 installed: {}. Install the model pack from Settings and start again.",
                facts.missing_required_models.join(", ")
            ),
        );
    }
    if !facts.missing_optional_models.is_empty() {
        return row(
            PreflightCheck::Models,
            PreflightVerdict::Warn,
            format!(
                "These steps will be skipped because their models are not installed: {}. \
                 Everything else runs.",
                facts.missing_optional_models.join(", ")
            ),
        );
    }
    row(
        PreflightCheck::Models,
        PreflightVerdict::Pass,
        "Every model this run needs is installed and verified.",
    )
}

fn budget_row(facts: &Facts) -> PreflightRow {
    match facts.cloud_budget_usd {
        None => row(
            PreflightCheck::CloudBudget,
            PreflightVerdict::Pass,
            "Nothing in this run needs the internet.",
        ),
        Some(budget) if budget <= 0.0 => row(
            PreflightCheck::CloudBudget,
            PreflightVerdict::Warn,
            "This wedding's AI budget is spent, so the steps that would have asked a model will \
             use their offline answers instead. The run finishes either way.",
        ),
        Some(budget) => row(
            PreflightCheck::CloudBudget,
            PreflightVerdict::Pass,
            format!("${budget:.2} left in this wedding's AI budget."),
        ),
    }
}

/// The row this build always shows, and the honest one.
///
/// Phase 13's condition C2 is that nothing in this build is calibrated, so `uncalibrated_raises`
/// moves every decision one band toward review. The consequence for a photographer pressing a
/// Zero-Touch button is concrete and is worth stating before the run rather than after it: AURA
/// will do the work and ask about more of it than it eventually will.
fn calibration_row(facts: &Facts) -> PreflightRow {
    if facts.calibrated {
        return row(
            PreflightCheck::Calibration,
            PreflightVerdict::Pass,
            "AURA has learned how often it is right, so it will only ask you about the \
             decisions it is genuinely unsure of.",
        );
    }
    if facts.held_stages == 0 {
        return row(
            PreflightCheck::Calibration,
            PreflightVerdict::Warn,
            "AURA has not yet learned how often it is right, so it is being careful: it will do \
             the work and put more of it in the review queue than it will once it has learned.",
        );
    }
    row(
        PreflightCheck::Calibration,
        PreflightVerdict::Warn,
        format!(
            "AURA has not yet learned how often it is right, so it is being careful. {} of the \
             steps you asked for will wait for you rather than run on their own; everything else \
             runs and goes in the review queue.",
            facts.held_stages
        ),
    )
}

fn power_row(facts: &Facts) -> PreflightRow {
    if !facts.on_battery {
        return row(PreflightCheck::Power, PreflightVerdict::Pass, "Plugged in.");
    }
    if facts.allow_on_battery {
        return row(
            PreflightCheck::Power,
            PreflightVerdict::Warn,
            "This machine is on battery and you have asked AURA to run anyway. It will, and it \
             will use the battery quickly.",
        );
    }
    row(
        PreflightCheck::Power,
        PreflightVerdict::Warn,
        "This machine is on battery, so the heavy steps will wait until it is plugged in. \
         Everything is saved as it goes, so plugging in later picks up where it left off.",
    )
}

fn row(
    check: PreflightCheck,
    verdict: PreflightVerdict,
    detail: impl Into<String>,
) -> PreflightRow {
    PreflightRow {
        check,
        verdict,
        detail: detail.into(),
    }
}

#[allow(clippy::cast_precision_loss)]
fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> Facts {
        Facts {
            images: 3_000,
            disk_free_bytes: Some(500_000_000_000),
            estimated_output_bytes: 30_000_000_000,
            hardware_detail: "NVIDIA GPU, 8 GB".into(),
            calibrated: true,
            ..Facts::default()
        }
    }

    #[test]
    fn a_healthy_project_passes_everything() {
        let report = check(&healthy());
        assert_eq!(report.verdict(), PreflightVerdict::Pass);
        assert!(report.permits_start());
        assert_eq!(report.rows.len(), PreflightCheck::ALL.len());
    }

    #[test]
    fn every_check_appears_exactly_once_and_in_order() {
        let report = check(&healthy());
        let checks: Vec<PreflightCheck> = report.rows.iter().map(|row| row.check).collect();
        assert_eq!(checks, PreflightCheck::ALL.to_vec());
    }

    #[test]
    fn every_row_says_something() {
        // The whole point of section 2.1's "actionable messages": a verdict with no sentence sends
        // a photographer to a runbook to find out how many gigabytes they need.
        for facts in [healthy(), Facts::default()] {
            for row in check(&facts).rows {
                assert!(
                    !row.detail.trim().is_empty(),
                    "{:?} says nothing",
                    row.check
                );
            }
        }
    }

    #[test]
    fn a_project_with_no_photographs_blocks() {
        let facts = Facts {
            images: 0,
            ..healthy()
        };
        assert!(!check(&facts).permits_start());
    }

    #[test]
    fn a_disk_that_cannot_hold_the_output_blocks_and_says_how_much_to_free() {
        let facts = Facts {
            disk_free_bytes: Some(10_000_000_000),
            estimated_output_bytes: 30_000_000_000,
            ..healthy()
        };
        let report = check(&facts);
        assert!(!report.permits_start());
        let disk = report
            .rows
            .iter()
            .find(|row| row.check == PreflightCheck::DiskSpace)
            .expect("a disk row");
        assert_eq!(disk.verdict, PreflightVerdict::Block);
        assert!(disk.detail.contains("Free up"), "{}", disk.detail);
    }

    #[test]
    fn an_unreadable_disk_warns_rather_than_passing() {
        // Phase 24's rule where it costs the most: a machine whose free space cannot be read is
        // not a machine with enough space.
        let facts = Facts {
            disk_free_bytes: None,
            ..healthy()
        };
        let report = check(&facts);
        let disk = report
            .rows
            .iter()
            .find(|row| row.check == PreflightCheck::DiskSpace)
            .expect("a disk row");
        assert_eq!(disk.verdict, PreflightVerdict::Warn);
        assert!(report.permits_start());
    }

    #[test]
    fn a_missing_model_blocks_only_when_a_mandatory_stage_needs_it() {
        let optional = Facts {
            missing_optional_models: vec!["face_detect".into()],
            ..healthy()
        };
        assert!(check(&optional).permits_start());

        let required = Facts {
            missing_required_models: vec!["wedding_embedding".into()],
            ..healthy()
        };
        assert!(!check(&required).permits_start());
    }

    #[test]
    fn an_uncalibrated_build_always_warns_and_says_how_many_steps_will_wait() {
        let facts = Facts {
            calibrated: false,
            held_stages: 4,
            ..healthy()
        };
        let report = check(&facts);
        let row = report
            .rows
            .iter()
            .find(|row| row.check == PreflightCheck::Calibration)
            .expect("a calibration row");
        assert_eq!(row.verdict, PreflightVerdict::Warn);
        assert!(row.detail.contains('4'), "{}", row.detail);
        assert!(report.permits_start());
    }

    #[test]
    fn being_on_battery_warns_and_never_blocks() {
        let facts = Facts {
            on_battery: true,
            ..healthy()
        };
        let report = check(&facts);
        assert_eq!(report.verdict(), PreflightVerdict::Warn);
        assert!(report.permits_start());
    }

    #[test]
    fn the_estimate_counts_only_the_stages_that_will_run() {
        let facts = Facts {
            planned: vec![(StageId::Previews, 100)],
            ..healthy()
        };
        assert_eq!(check(&facts).estimated_ms, 100 * 380);
    }
}
