//! The checklist and the resource budgets, from a file a product manager owns.
//!
//! `autopilot.toml` decides which stages a photographer wants run and how hard the governor is
//! allowed to push a machine. It decides nothing about the graph: [`crate::stages`] owns what
//! depends on what, and this file cannot reach it.
//!
//! ## The one thing this loader exists to refuse
//!
//! **A file that widens a bound the code owns.** Four numbers are safety limits rather than
//! preferences - the two thermal ceilings, the video-memory share and the battery floor - and a
//! studio that could raise them by editing a file could cook a laptop by editing a file. The
//! loader compares against the constants in [`crate::contract::autopilot`] and returns
//! `AURA-JOB-7008` rather than clamping quietly, because a clamped value is a studio believing it
//! configured something it did not.
//!
//! Phase 21 wrote this rule for retouch ceilings, phase 22 for sharpening, phase 24 for cleanup
//! and phase 27 for QC thresholds. This is its fifth application and the first where the thing
//! being protected is the photographer's hardware rather than their photographs.

use std::collections::BTreeMap;

use aura_core::{AuraError, AuraResult};
use serde::Deserialize;

use crate::contract::autopilot::{
    StageId, BATTERY_FLOOR, DISK_HEADROOM, THERMAL_PAUSE_C, THERMAL_REDUCE_C, VRAM_CEILING,
};
use crate::errors;

/// The table this build ships, embedded so a binary cannot disagree with itself.
const EMBEDDED: &str = include_str!("../config/autopilot.toml");

/// Where the file may be overridden per installation.
pub const OVERRIDE_RELATIVE_PATH: &str = "autopilot.toml";

/// How hard a run may push the machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budgets {
    /// The share of video memory a run may occupy.
    pub vram_ceiling: f32,
    /// Degrees Celsius above which the governor reduces concurrency.
    pub thermal_reduce_c: f32,
    /// Degrees Celsius above which the governor pauses.
    pub thermal_pause_c: f32,
    /// Battery share below which heavy stages do not start on battery power.
    pub battery_floor: f32,
    /// Free disk required, as a multiple of the estimated output.
    pub disk_headroom: f32,
    /// How many stages may run at once when nothing is under pressure.
    pub max_parallel_stages: u16,
    /// Photographs per batch for the stages that checkpoint per batch.
    pub batch_size: u16,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            vram_ceiling: VRAM_CEILING,
            thermal_reduce_c: THERMAL_REDUCE_C,
            thermal_pause_c: THERMAL_PAUSE_C,
            battery_floor: BATTERY_FLOOR,
            disk_headroom: DISK_HEADROOM,
            max_parallel_stages: 2,
            batch_size: 16,
        }
    }
}

/// The whole loaded policy.
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    version: i64,
    defaults: BTreeMap<StageId, bool>,
    budgets: Budgets,
}

impl Policy {
    /// The table this build ships.
    ///
    /// # Errors
    ///
    /// `AURA-JOB-7008` when the embedded file is not valid, which is a build defect rather than an
    /// installation one and is why the phase gate parses it on every run.
    pub fn embedded() -> AuraResult<Self> {
        Self::parse(EMBEDDED)
    }

    /// Parse a table.
    ///
    /// # Errors
    ///
    /// `AURA-JOB-7008` when the file will not parse, names a stage that does not exist, names a
    /// mandatory stage, or widens one of the four bounds the code owns.
    pub fn parse(text: &str) -> AuraResult<Self> {
        let file: PolicyFile =
            toml::from_str(text).map_err(|err| errors::policy_refused(err.to_string()))?;

        let mut defaults = BTreeMap::new();
        for row in &file.stage {
            let Some(stage) = StageId::parse(&row.id) else {
                return Err(errors::policy_refused(format!(
                    "the checklist names `{}`, which is not a stage",
                    row.id
                )));
            };
            if !crate::stages::decl(stage).optional {
                return Err(errors::policy_refused(format!(
                    "the checklist offers to switch off `{}`, which a wedding cannot be \
                     delivered without",
                    row.id
                )));
            }
            if row.reason.trim().is_empty() {
                return Err(errors::policy_refused(format!(
                    "the checklist row for `{}` has no written reason",
                    row.id
                )));
            }
            if defaults.insert(stage, row.default_on).is_some() {
                return Err(errors::policy_refused(format!(
                    "the checklist names `{}` twice",
                    row.id
                )));
            }
        }

        let budgets = Budgets {
            vram_ceiling: file.resources.vram_ceiling,
            thermal_reduce_c: file.resources.thermal_reduce_c,
            thermal_pause_c: file.resources.thermal_pause_c,
            battery_floor: file.resources.battery_floor,
            disk_headroom: file.resources.disk_headroom,
            max_parallel_stages: file.resources.max_parallel_stages,
            batch_size: file.resources.batch_size,
        };
        check_budgets(&budgets)?;

        Ok(Self {
            version: file.version,
            defaults,
            budgets,
        })
    }

    /// The file's version, recorded on every run it configured.
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// Whether a stage is on when nobody has configured anything.
    ///
    /// A stage with no row is on, which is exactly the four mandatory ones - the loader refuses a
    /// row for any of them, so there is no way for this to answer `false` about a stage a wedding
    /// cannot be delivered without.
    #[must_use]
    pub fn default_on(&self, stage: StageId) -> bool {
        self.defaults.get(&stage).copied().unwrap_or(true)
    }

    /// The resource budgets.
    #[must_use]
    pub const fn budgets(&self) -> Budgets {
        self.budgets
    }
}

/// Refuse a file that widens a bound the code owns.
///
/// Four comparisons, one direction each, and the directions are not all the same - which is the
/// part somebody re-deriving this will get wrong. A *ceiling* may only be lowered and a *floor*
/// may only be raised, because both of those mean "more cautious", and phase 22's
/// `skin_attenuation` established that a bound running the other way is a real thing rather than a
/// typo.
fn check_budgets(budgets: &Budgets) -> AuraResult<()> {
    if budgets.vram_ceiling > VRAM_CEILING {
        return Err(widened("vram_ceiling", budgets.vram_ceiling, VRAM_CEILING));
    }
    if budgets.thermal_reduce_c > THERMAL_REDUCE_C {
        return Err(widened(
            "thermal_reduce_c",
            budgets.thermal_reduce_c,
            THERMAL_REDUCE_C,
        ));
    }
    if budgets.thermal_pause_c > THERMAL_PAUSE_C {
        return Err(widened(
            "thermal_pause_c",
            budgets.thermal_pause_c,
            THERMAL_PAUSE_C,
        ));
    }
    if budgets.battery_floor < BATTERY_FLOOR {
        return Err(errors::policy_refused(format!(
            "battery_floor is {:.2} and the code's floor is {BATTERY_FLOOR:.2}; a studio may \
             raise it and may not lower it",
            budgets.battery_floor
        )));
    }
    if budgets.disk_headroom < DISK_HEADROOM {
        return Err(errors::policy_refused(format!(
            "disk_headroom is {:.2} and the code's floor is {DISK_HEADROOM:.2}; a studio may \
             raise it and may not lower it",
            budgets.disk_headroom
        )));
    }
    if budgets.thermal_pause_c <= budgets.thermal_reduce_c {
        return Err(errors::policy_refused(format!(
            "thermal_pause_c is {:.1} and thermal_reduce_c is {:.1}; a machine has to be told to \
             slow down before it is told to stop",
            budgets.thermal_pause_c, budgets.thermal_reduce_c
        )));
    }
    if budgets.max_parallel_stages == 0 {
        return Err(errors::policy_refused(
            "max_parallel_stages is 0, which is a run that never starts a stage",
        ));
    }
    if budgets.batch_size == 0 {
        return Err(errors::policy_refused(
            "batch_size is 0, which is a batch that never contains a photograph",
        ));
    }
    Ok(())
}

fn widened(field: &str, found: f32, ceiling: f32) -> AuraError {
    errors::policy_refused(format!(
        "{field} is {found:.2} and the code's ceiling is {ceiling:.2}; a studio may lower it and \
         may not raise it"
    ))
}

#[derive(Debug, Deserialize)]
struct PolicyFile {
    version: i64,
    #[serde(default)]
    stage: Vec<StageRow>,
    #[serde(default)]
    resources: ResourceRow,
}

#[derive(Debug, Deserialize)]
struct StageRow {
    id: String,
    default_on: bool,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct ResourceRow {
    vram_ceiling: f32,
    thermal_reduce_c: f32,
    thermal_pause_c: f32,
    battery_floor: f32,
    disk_headroom: f32,
    max_parallel_stages: u16,
    batch_size: u16,
}

impl Default for ResourceRow {
    fn default() -> Self {
        let budgets = Budgets::default();
        Self {
            vram_ceiling: budgets.vram_ceiling,
            thermal_reduce_c: budgets.thermal_reduce_c,
            thermal_pause_c: budgets.thermal_pause_c,
            battery_floor: budgets.battery_floor,
            disk_headroom: budgets.disk_headroom,
            max_parallel_stages: budgets.max_parallel_stages,
            batch_size: budgets.batch_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_loads() {
        let policy = Policy::embedded().expect("the shipped autopilot.toml must load");
        assert_eq!(policy.version(), 1);
    }

    #[test]
    fn micro_and_cleanup_are_the_two_stages_that_are_off_by_default() {
        let policy = Policy::embedded().expect("policy");
        let off: Vec<StageId> = StageId::ALL
            .into_iter()
            .filter(|stage| !policy.default_on(*stage))
            .collect();
        assert_eq!(off, vec![StageId::Micro, StageId::Cleanup]);
    }

    #[test]
    fn every_optional_stage_has_a_row_with_a_reason() {
        // A checklist row a photographer can see and nobody wrote a reason for is a switch nobody
        // can defend. The loader refuses an empty reason; this asserts the shipped file has one
        // for every stage it offers.
        let policy = Policy::embedded().expect("policy");
        for stage in StageId::ALL {
            if crate::stages::decl(stage).optional {
                assert!(
                    policy.defaults.contains_key(&stage),
                    "{stage} is optional and the checklist does not mention it"
                );
            }
        }
    }

    #[test]
    fn a_file_that_raises_the_thermal_ceiling_is_refused() {
        let text = format!(
            "version = 1\n[resources]\nthermal_reduce_c = {}\n",
            THERMAL_REDUCE_C + 5.0
        );
        let err = Policy::parse(&text).expect_err("a widened thermal ceiling must be refused");
        assert_eq!(err.code.0, "AURA-JOB-7008");
    }

    #[test]
    fn a_file_that_raises_the_vram_share_is_refused() {
        let text = "version = 1\n[resources]\nvram_ceiling = 0.95\n";
        let err = Policy::parse(text).expect_err("a widened vram share must be refused");
        assert_eq!(err.code.0, "AURA-JOB-7008");
    }

    #[test]
    fn a_file_that_lowers_the_battery_floor_is_refused() {
        let text = "version = 1\n[resources]\nbattery_floor = 0.05\n";
        let err = Policy::parse(text).expect_err("a lowered battery floor must be refused");
        assert_eq!(err.code.0, "AURA-JOB-7008");
    }

    #[test]
    fn tightening_every_bound_is_allowed() {
        let text = "version = 3\n[resources]\nvram_ceiling = 0.5\nthermal_reduce_c = 70.0\n\
                    thermal_pause_c = 80.0\nbattery_floor = 0.6\ndisk_headroom = 2.5\n";
        let policy = Policy::parse(text).expect("a stricter file must be accepted");
        assert_eq!(policy.version(), 3);
        assert!((policy.budgets().vram_ceiling - 0.5).abs() < f32::EPSILON);
        assert!((policy.budgets().battery_floor - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn a_file_that_offers_to_switch_off_the_cull_is_refused() {
        let text = "version = 1\n[[stage]]\nid = \"cull\"\ndefault_on = false\nreason = \"x\"\n";
        let err = Policy::parse(text).expect_err("a mandatory stage must not be switchable");
        assert_eq!(err.code.0, "AURA-JOB-7008");
    }

    #[test]
    fn a_file_naming_a_stage_that_does_not_exist_is_refused() {
        let text = "version = 1\n[[stage]]\nid = \"denoise\"\ndefault_on = true\nreason = \"x\"\n";
        let err = Policy::parse(text).expect_err("an unknown stage must be refused");
        assert_eq!(err.code.0, "AURA-JOB-7008");
    }

    #[test]
    fn a_row_with_no_reason_is_refused() {
        let text = "version = 1\n[[stage]]\nid = \"micro\"\ndefault_on = false\nreason = \"  \"\n";
        let err = Policy::parse(text).expect_err("a row with no reason must be refused");
        assert_eq!(err.code.0, "AURA-JOB-7008");
    }

    #[test]
    fn a_pause_ceiling_below_the_reduce_ceiling_is_refused() {
        let text = "version = 1\n[resources]\nthermal_reduce_c = 80.0\nthermal_pause_c = 75.0\n";
        let err = Policy::parse(text).expect_err("an inverted thermal pair must be refused");
        assert_eq!(err.code.0, "AURA-JOB-7008");
    }
}
