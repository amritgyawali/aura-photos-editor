//! What the machine has to say about how fast the run may go.
//!
//! Section 6.2. Seven readings, one action, and the action can only make the product do less.
//!
//! ## Why a component whose failure modes all point one way has no unsafe failure mode
//!
//! Every input here is an `Option` and every `None` contributes [`GovernorAction::Proceed`] -
//! which sounds like the dangerous default and is not, because `Proceed` is the *absence* of a
//! restriction rather than a licence. There is no reading that could make the product raise its
//! concurrency, enlarge a batch or skip a check, so a sensor that is broken, absent or lying
//! cannot cause anything worse than the run going at the speed it would have gone at anyway.
//!
//! Phase 24 built its cloud judgement this way: `Decline | Stand | Unavailable` with no `Approve`,
//! so an unreachable provider and a cautious model leave the photograph in the same state. This is
//! the same shape applied to hardware, and it is why the governor is safe to run with no
//! telemetry at all on a machine that exposes none.
//!
//! ## Why the readings combine with `max` rather than by voting
//!
//! Because they are about different things. A hot machine and a full disk are not two votes about
//! one question, they are two independent reasons to slow down, and the correct response to both
//! is the stronger of the two. `GovernorAction`'s ordering is the strength of the response and
//! that is the whole of the combination rule.

use crate::contract::autopilot::{
    GovernorAction, MachineState, ResourceEvent, ResourceKind, StageId,
};
use crate::policy::Budgets;

/// What the photographer asked for that the governor has to honour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunMode {
    /// Whether heavy stages may run on battery power.
    pub allow_on_battery: bool,
    /// Whether the run yields to foreground work.
    pub quiet_mode: bool,
}

impl Default for RunMode {
    fn default() -> Self {
        Self {
            allow_on_battery: false,
            quiet_mode: true,
        }
    }
}

/// The decision, and everything that went into it.
#[derive(Debug, Clone, PartialEq)]
pub struct Ruling {
    /// The strongest action any reading called for.
    pub action: GovernorAction,
    /// One event per reading that called for more than [`GovernorAction::Proceed`].
    pub events: Vec<ResourceEvent>,
    /// Stages that may run at once, after the action is applied.
    pub parallel_stages: u16,
    /// Photographs per batch, after the action is applied.
    pub batch_size: u16,
}

/// Record one reading that asked for something, and hand back what it asked for.
///
/// A free function rather than a closure so the borrow of `events` ends at the call: the ruling
/// takes seven readings in sequence and a closure holding the vector would make each one a
/// borrow-checker argument rather than a governor argument.
///
/// A [`GovernorAction::Proceed`] records nothing. Seven rows saying "the temperature was fine"
/// on every poll of every stage of a two-hour run is a table nobody can read, and the one thing
/// `resource_events` exists for is a photographer asking why their run took four hours.
fn consider(
    events: &mut Vec<ResourceEvent>,
    stage: StageId,
    kind: ResourceKind,
    candidate: GovernorAction,
    reading: f32,
    threshold: f32,
) -> GovernorAction {
    if candidate == GovernorAction::Proceed {
        return GovernorAction::Proceed;
    }
    events.push(ResourceEvent {
        kind,
        action: candidate,
        reading,
        threshold,
        stage,
    });
    candidate
}

/// The machine's own opinion about how fast this run may go.
#[derive(Debug, Clone, Copy)]
pub struct Governor {
    budgets: Budgets,
    mode: RunMode,
}

impl Governor {
    /// A governor with these budgets and this run's mode.
    #[must_use]
    pub const fn new(budgets: Budgets, mode: RunMode) -> Self {
        Self { budgets, mode }
    }

    /// What to do about the machine right now.
    ///
    /// Seven independent readings, folded with `max`, because `GovernorAction` is ordered by how
    /// much it holds the product back and the strictest reading wins. Each reading is its own
    /// method for the reason section 6.2 gives them one row each: they fail independently, they
    /// are fixed by different things, and a reader chasing "why did this run slow down" wants one
    /// of them rather than all seven.
    #[must_use]
    pub fn rule(&self, machine: &MachineState, stage: StageId) -> Ruling {
        let mut events = Vec::new();
        let mut action = GovernorAction::Proceed;

        action = action.max(self.vram(&mut events, machine, stage));
        action = action.max(Self::ram(&mut events, machine, stage));
        action = action.max(self.thermal(&mut events, machine, stage));
        action = action.max(self.battery(&mut events, machine, stage));
        action = action.max(self.disk(&mut events, machine, stage));
        action = action.max(self.quiet(&mut events, machine, stage));
        action = action.max(Self::device(&mut events, machine, stage));

        let (parallel_stages, batch_size) = self.apply(action);
        Ruling {
            action,
            events,
            parallel_stages,
            batch_size,
        }
    }

    /// Video memory. Over the ceiling reduces; a device that has stopped answering is a different
    /// reading and is `Self::device`.
    fn vram(
        &self,
        events: &mut Vec<ResourceEvent>,
        machine: &MachineState,
        stage: StageId,
    ) -> GovernorAction {
        let Some(used) = machine.vram_used else {
            return GovernorAction::Proceed;
        };
        let candidate = if used > self.budgets.vram_ceiling {
            GovernorAction::Reduce
        } else {
            GovernorAction::Proceed
        };
        consider(
            events,
            stage,
            ResourceKind::Vram,
            candidate,
            used,
            self.budgets.vram_ceiling,
        )
    }

    /// Host memory. Pauses rather than reduces past 0.95, because a machine that is swapping is a
    /// machine where reducing the batch size does not help fast enough.
    ///
    /// An associated function rather than a method, and the same for `Self::device`: these two
    /// readings are the ones a studio cannot tune, so they hold no budget and take no `self`.
    fn ram(
        events: &mut Vec<ResourceEvent>,
        machine: &MachineState,
        stage: StageId,
    ) -> GovernorAction {
        let Some(used) = machine.ram_used else {
            return GovernorAction::Proceed;
        };
        let candidate = if used > 0.95 {
            GovernorAction::Pause
        } else if used > 0.85 {
            GovernorAction::Reduce
        } else {
            GovernorAction::Proceed
        };
        consider(events, stage, ResourceKind::Ram, candidate, used, 0.85)
    }

    /// Temperature.
    fn thermal(
        &self,
        events: &mut Vec<ResourceEvent>,
        machine: &MachineState,
        stage: StageId,
    ) -> GovernorAction {
        let Some(temperature) = machine.temperature_c else {
            return GovernorAction::Proceed;
        };
        let candidate = if temperature >= self.budgets.thermal_pause_c {
            GovernorAction::Pause
        } else if temperature >= self.budgets.thermal_reduce_c {
            GovernorAction::Reduce
        } else {
            GovernorAction::Proceed
        };
        consider(
            events,
            stage,
            ResourceKind::Thermal,
            candidate,
            temperature,
            self.budgets.thermal_reduce_c,
        )
    }

    /// Battery. Only when actually on battery: a laptop plugged in reports a charge and it is not
    /// a reason to do anything.
    fn battery(
        &self,
        events: &mut Vec<ResourceEvent>,
        machine: &MachineState,
        stage: StageId,
    ) -> GovernorAction {
        if !machine.on_battery || self.mode.allow_on_battery {
            return GovernorAction::Proceed;
        }
        let charge = machine.battery.unwrap_or(0.0);
        let candidate = if charge < self.budgets.battery_floor {
            GovernorAction::Pause
        } else {
            GovernorAction::Reduce
        };
        consider(
            events,
            stage,
            ResourceKind::Battery,
            candidate,
            charge,
            self.budgets.battery_floor,
        )
    }

    /// Disk. The only reading that can stop a run, because it is the only one that does not clear
    /// on its own: a hot machine cools and a busy foreground goes away, and a full disk stays full
    /// until somebody does something about it. Continuing would be writing until the write fails,
    /// which is the failure this whole phase exists to avoid at 90 % of a run.
    fn disk(
        &self,
        events: &mut Vec<ResourceEvent>,
        machine: &MachineState,
        stage: StageId,
    ) -> GovernorAction {
        let (Some(free), Some(needed)) = (machine.disk_free_bytes, machine.disk_needed_bytes)
        else {
            return GovernorAction::Proceed;
        };
        #[allow(clippy::cast_precision_loss)]
        let ratio = if needed == 0 {
            f32::INFINITY
        } else {
            free as f32 / needed as f32
        };
        let candidate = if ratio < 1.0 {
            GovernorAction::Stop
        } else if ratio < self.budgets.disk_headroom {
            GovernorAction::Reduce
        } else {
            GovernorAction::Proceed
        };
        consider(
            events,
            stage,
            ResourceKind::Disk,
            candidate,
            ratio,
            self.budgets.disk_headroom,
        )
    }

    /// The photographer is working. Section 6.2's quiet mode: yield rather than compete, so an
    /// overnight run is not required.
    fn quiet(
        &self,
        events: &mut Vec<ResourceEvent>,
        machine: &MachineState,
        stage: StageId,
    ) -> GovernorAction {
        if !self.mode.quiet_mode || !machine.foreground_busy {
            return GovernorAction::Proceed;
        }
        consider(
            events,
            stage,
            ResourceKind::Quiet,
            GovernorAction::Reduce,
            1.0,
            0.0,
        )
    }

    /// A lost device reduces rather than stops: section 6.3 asks for the run to continue on the
    /// CPU where feasible, and that is a *smaller* run rather than no run. The stage that wanted
    /// the GPU decides whether it can; this only says the run may keep going.
    fn device(
        events: &mut Vec<ResourceEvent>,
        machine: &MachineState,
        stage: StageId,
    ) -> GovernorAction {
        if !machine.device_lost {
            return GovernorAction::Proceed;
        }
        consider(
            events,
            stage,
            ResourceKind::DeviceLost,
            GovernorAction::Reduce,
            1.0,
            0.0,
        )
    }

    /// What the concurrency becomes under an action.
    ///
    /// Halved to a floor of one, never to zero: a batch size of zero is a stage that never
    /// finishes, and the correct expression of "do not start anything" is `Pause`, which the
    /// runner honours by waiting rather than by shrinking a number to nothing.
    #[must_use]
    const fn apply(&self, action: GovernorAction) -> (u16, u16) {
        match action {
            GovernorAction::Proceed => (self.budgets.max_parallel_stages, self.budgets.batch_size),
            GovernorAction::Reduce => (
                if self.budgets.max_parallel_stages > 1 {
                    self.budgets.max_parallel_stages / 2
                } else {
                    1
                },
                if self.budgets.batch_size > 1 {
                    self.budgets.batch_size / 2
                } else {
                    1
                },
            ),
            GovernorAction::Pause | GovernorAction::Stop => (1, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn governor() -> Governor {
        Governor::new(Budgets::default(), RunMode::default())
    }

    #[test]
    fn a_machine_with_no_readings_at_all_proceeds() {
        // Every sensor absent. The point is not that this is optimistic - it is that there is
        // nothing a governor can safely do with no information except leave the run alone, and
        // `Proceed` is the absence of a restriction rather than a licence.
        let ruling = governor().rule(&MachineState::default(), StageId::Embed);
        assert_eq!(ruling.action, GovernorAction::Proceed);
        assert!(ruling.events.is_empty());
    }

    #[test]
    fn a_hot_machine_reduces_and_a_very_hot_one_pauses() {
        let mut state = MachineState {
            temperature_c: Some(88.0),
            ..MachineState::default()
        };
        assert_eq!(
            governor().rule(&state, StageId::Embed).action,
            GovernorAction::Reduce
        );
        state.temperature_c = Some(97.0);
        assert_eq!(
            governor().rule(&state, StageId::Embed).action,
            GovernorAction::Pause
        );
    }

    #[test]
    fn a_full_disk_is_the_only_reading_that_stops_a_run() {
        let state = MachineState {
            disk_free_bytes: Some(500),
            disk_needed_bytes: Some(1_000),
            ..MachineState::default()
        };
        let ruling = governor().rule(&state, StageId::Export);
        assert_eq!(ruling.action, GovernorAction::Stop);

        // Nothing else reaches `Stop`, however extreme.
        let extremes = [
            MachineState {
                temperature_c: Some(150.0),
                ..MachineState::default()
            },
            MachineState {
                vram_used: Some(1.0),
                ..MachineState::default()
            },
            MachineState {
                ram_used: Some(1.0),
                ..MachineState::default()
            },
            MachineState {
                on_battery: true,
                battery: Some(0.01),
                ..MachineState::default()
            },
            MachineState {
                device_lost: true,
                ..MachineState::default()
            },
        ];
        for state in extremes {
            let action = governor().rule(&state, StageId::Embed).action;
            assert_ne!(action, GovernorAction::Stop, "{state:?} reached Stop");
        }
    }

    #[test]
    fn the_strongest_reading_wins_rather_than_the_last_one() {
        let state = MachineState {
            temperature_c: Some(97.0),
            foreground_busy: true,
            ..MachineState::default()
        };
        // Quiet mode is considered after the thermal reading and only asks for `Reduce`; the
        // ruling must still be `Pause`.
        assert_eq!(
            governor().rule(&state, StageId::Embed).action,
            GovernorAction::Pause
        );
    }

    #[test]
    fn every_reading_that_asked_for_something_produces_an_event() {
        let state = MachineState {
            temperature_c: Some(88.0),
            vram_used: Some(0.9),
            foreground_busy: true,
            ..MachineState::default()
        };
        let ruling = governor().rule(&state, StageId::Retouch);
        let kinds: Vec<ResourceKind> = ruling.events.iter().map(|event| event.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ResourceKind::Vram,
                ResourceKind::Thermal,
                ResourceKind::Quiet
            ]
        );
        for event in &ruling.events {
            assert_eq!(event.stage, StageId::Retouch);
        }
    }

    #[test]
    fn a_plugged_in_laptop_reporting_a_charge_is_not_a_reason_to_do_anything() {
        let state = MachineState {
            battery: Some(0.05),
            on_battery: false,
            ..MachineState::default()
        };
        assert_eq!(
            governor().rule(&state, StageId::Embed).action,
            GovernorAction::Proceed
        );
    }

    #[test]
    fn a_photographer_who_said_yes_to_battery_is_not_asked_again() {
        let state = MachineState {
            battery: Some(0.10),
            on_battery: true,
            ..MachineState::default()
        };
        let permissive = Governor::new(
            Budgets::default(),
            RunMode {
                allow_on_battery: true,
                quiet_mode: true,
            },
        );
        assert_eq!(
            permissive.rule(&state, StageId::Embed).action,
            GovernorAction::Proceed
        );
        assert_eq!(
            governor().rule(&state, StageId::Embed).action,
            GovernorAction::Pause
        );
    }

    #[test]
    fn reducing_halves_the_batch_and_never_reaches_zero() {
        let tiny = Budgets {
            batch_size: 1,
            max_parallel_stages: 1,
            ..Budgets::default()
        };
        let governor = Governor::new(tiny, RunMode::default());
        let state = MachineState {
            temperature_c: Some(88.0),
            ..MachineState::default()
        };
        let ruling = governor.rule(&state, StageId::Embed);
        assert_eq!(ruling.action, GovernorAction::Reduce);
        assert_eq!(ruling.batch_size, 1);
        assert_eq!(ruling.parallel_stages, 1);
    }

    #[test]
    fn a_lost_device_reduces_rather_than_failing_the_run() {
        // Section 6.3: a driver reset continues on the CPU where feasible, with an honest ETA
        // update. A governor that stopped here would turn a recoverable driver reset into a
        // two-hour run that has to be restarted.
        let state = MachineState {
            device_lost: true,
            ..MachineState::default()
        };
        assert_eq!(
            governor().rule(&state, StageId::Embed).action,
            GovernorAction::Reduce
        );
    }
}
