//! Authored ports, so a whole wedding can be run without a wedding.
//!
//! Section 10.1's chaos suite is twenty kills at random points, a sleep, an unplugged drive, a
//! full disk and a GPU reset. None of those is testable against a real pipeline on a machine with
//! no camera files, no GPU backend and no trained model - so what is tested here is the
//! *orchestrator*, against a runner whose behaviour this repository authored.
//!
//! **That proves the scheduling, the checkpointing, the resume, the isolation, the governor and
//! the summary. It proves nothing about how long a real wedding takes.** Condition C1 of the phase
//! 28 exit report, and the fixtures are in the shipped crate rather than in a test file so the
//! phase gate drives exactly the same ones the unit tests do.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use aura_core::contract::ids::ProjectId;
use aura_core::contract::ledger::{Autonomy, DecisionKind};
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use parking_lot::Mutex;

use crate::api::stage_inputs;
use crate::contract::autopilot::{
    AutonomyGate, MachineProbe, MachineState, RunWatch, SkipCause, StageId, StageOutcome,
    StageRequest, StageRunner,
};

/// How a fixture stage should behave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Behaviour {
    /// Finish every unit.
    Succeed,
    /// Fail this many times, then finish.
    FailTimes(u8),
    /// Fail every time.
    AlwaysFail,
    /// Report itself unavailable.
    Unavailable(SkipCause),
    /// Have nothing to work on.
    Empty,
    /// Cancel the run part-way through, after this many units.
    CancelAfter(u32),
}

/// A `StageRunner` that does what it was told to do.
#[derive(Debug)]
pub struct ScriptedRunner {
    units: u32,
    behaviours: Mutex<BTreeMap<StageId, Behaviour>>,
    attempts: Mutex<BTreeMap<StageId, u8>>,
    versions: Mutex<BTreeMap<StageId, String>>,
    ran: Mutex<Vec<StageId>>,
    units_done: AtomicU32,
    cancel: Mutex<Option<CancelToken>>,
}

impl ScriptedRunner {
    /// A runner where every stage succeeds over `units` units.
    #[must_use]
    pub fn new(units: u32) -> Self {
        Self {
            units,
            behaviours: Mutex::new(BTreeMap::new()),
            attempts: Mutex::new(BTreeMap::new()),
            versions: Mutex::new(BTreeMap::new()),
            ran: Mutex::new(Vec::new()),
            units_done: AtomicU32::new(0),
            cancel: Mutex::new(None),
        }
    }

    /// Give one stage a behaviour.
    #[must_use]
    pub fn with(self, stage: StageId, behaviour: Behaviour) -> Self {
        self.behaviours.lock().insert(stage, behaviour);
        self
    }

    /// Give one stage a version string, so moving it invalidates its checkpoint.
    pub fn set_version(&self, stage: StageId, version: &str) {
        self.versions.lock().insert(stage, version.to_string());
    }

    /// Hand the runner the token it should cancel when a `CancelAfter` fires.
    pub fn arm(&self, cancel: CancelToken) {
        *self.cancel.lock() = Some(cancel);
    }

    /// Which stages actually ran, in order.
    #[must_use]
    pub fn ran(&self) -> Vec<StageId> {
        self.ran.lock().clone()
    }

    /// How many units finished across the whole run.
    #[must_use]
    pub fn units_done(&self) -> u32 {
        self.units_done.load(Ordering::SeqCst)
    }

    fn behaviour(&self, stage: StageId) -> Behaviour {
        self.behaviours
            .lock()
            .get(&stage)
            .cloned()
            .unwrap_or(Behaviour::Succeed)
    }
}

impl StageRunner for ScriptedRunner {
    fn unit_count(&self, _project: ProjectId, stage: StageId) -> AuraResult<u32> {
        Ok(match self.behaviour(stage) {
            Behaviour::Empty => 0,
            _ => self.units,
        })
    }

    fn availability(&self, _project: ProjectId, stage: StageId) -> Option<SkipCause> {
        match self.behaviour(stage) {
            Behaviour::Unavailable(cause) => Some(cause),
            _ => None,
        }
    }

    fn run(
        &self,
        request: &StageRequest,
        progress: &RunWatch,
        cancel: &CancelToken,
    ) -> AuraResult<StageOutcome> {
        self.ran.lock().push(request.stage);
        match self.behaviour(request.stage) {
            // Unreachable through the orchestrator, which asks `availability` first. Answered
            // rather than panicked on, because a fixture that panicked here would be a fixture
            // that could not be used to test a caller which forgot to ask.
            Behaviour::Unavailable(cause) => Ok(StageOutcome::Skipped(cause)),
            Behaviour::Empty => Ok(StageOutcome::Skipped(SkipCause::NoInput)),
            Behaviour::AlwaysFail => Ok(StageOutcome::Failed {
                code: "AURA-JOB-7005".to_string(),
                detail: "the fixture was told to fail".to_string(),
            }),
            Behaviour::FailTimes(times) => {
                let mut attempts = self.attempts.lock();
                let seen = attempts.entry(request.stage).or_insert(0);
                *seen = seen.saturating_add(1);
                if *seen <= times {
                    return Ok(StageOutcome::Failed {
                        code: "AURA-JOB-7005".to_string(),
                        detail: format!("attempt {seen} of a scripted failure"),
                    });
                }
                drop(attempts);
                self.finish(request, progress, cancel, None)
            }
            Behaviour::CancelAfter(after) => self.finish(request, progress, cancel, Some(after)),
            Behaviour::Succeed => self.finish(request, progress, cancel, None),
        }
    }

    fn inputs_hash(&self, _project: ProjectId, stage: StageId) -> AuraResult<String> {
        let versions = self.versions.lock();
        let version = versions
            .get(&stage)
            .cloned()
            .unwrap_or_else(|| "1".to_string());
        Ok(stage_inputs(
            stage,
            &[("fixture_ver", &version)],
            self.units,
        ))
    }
}

impl ScriptedRunner {
    fn finish(
        &self,
        request: &StageRequest,
        progress: &RunWatch,
        cancel: &CancelToken,
        cancel_after: Option<u32>,
    ) -> AuraResult<StageOutcome> {
        let mut done = request.resume_from;
        while done < self.units {
            if cancel.is_cancelled() {
                return Ok(StageOutcome::Partial {
                    items: done,
                    failed: 0,
                    detail: "cancelled".to_string(),
                });
            }
            done += 1;
            self.units_done.fetch_add(1, Ordering::SeqCst);
            progress.update(|value| value.items_done = done);
            if cancel_after == Some(done) {
                if let Some(token) = self.cancel.lock().as_ref() {
                    token.cancel();
                }
            }
        }
        Ok(StageOutcome::Completed { items: done })
    }
}

/// A gate that answers with one band for everything.
#[derive(Debug, Clone, Copy)]
pub struct FixedGate {
    band: Autonomy,
    calibrated: bool,
}

impl FixedGate {
    /// A gate answering `band`, reporting whether the build is calibrated.
    #[must_use]
    pub const fn new(band: Autonomy, calibrated: bool) -> Self {
        Self { band, calibrated }
    }

    /// The gate this build actually has: uncalibrated, so every band is raised one step.
    ///
    /// `Auto` raised once is `AutoZeroTouch`, which acts in Zero-Touch and holds otherwise. That
    /// is the honest shape of this release and it is what the phase gate runs against.
    #[must_use]
    pub const fn uncalibrated() -> Self {
        Self::new(Autonomy::AutoZeroTouch, false)
    }
}

impl AutonomyGate for FixedGate {
    fn band(&self, _project: ProjectId, _kind: DecisionKind) -> AuraResult<Autonomy> {
        Ok(self.band)
    }

    fn calibrated(&self, _project: ProjectId) -> bool {
        self.calibrated
    }
}

/// A probe that reports whatever it was given.
#[derive(Debug, Clone)]
pub struct FixedProbe {
    state: MachineState,
}

impl FixedProbe {
    /// A probe reporting this state.
    #[must_use]
    pub const fn new(state: MachineState) -> Self {
        Self { state }
    }

    /// A machine with nothing to report, which is what a desktop with no sensors looks like.
    #[must_use]
    pub fn quiet() -> Self {
        Self::new(MachineState::default())
    }

    /// A machine whose disk cannot hold the run.
    #[must_use]
    pub fn full_disk() -> Self {
        Self::new(MachineState {
            disk_free_bytes: Some(1),
            disk_needed_bytes: Some(1_000_000),
            ..MachineState::default()
        })
    }

    /// A machine that is hot enough for the governor to slow the run down.
    #[must_use]
    pub fn hot() -> Self {
        Self::new(MachineState {
            temperature_c: Some(90.0),
            ..MachineState::default()
        })
    }
}

impl MachineProbe for FixedProbe {
    fn sample(&self) -> MachineState {
        self.state
    }
}

/// The three ports, wired to fixtures.
#[must_use]
pub fn ports(runner: Arc<ScriptedRunner>, gate: FixedGate, probe: FixedProbe) -> crate::api::Ports {
    crate::api::Ports {
        runner,
        gate: Arc::new(gate),
        probe: Arc::new(probe),
    }
}
