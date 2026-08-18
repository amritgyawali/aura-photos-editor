//! The accelerator port. **This build links no `wgpu`.**
//!
//! ADR-0029 section 4 has the argument, and it is the same one ADR-0004 made about LibRaw
//! and ADR-0007 about ONNX Runtime: the build machine has no Windows SDK, a backend that
//! cannot be run cannot be tested, and an untested renderer is worse than an absent one
//! because it looks like a shipped feature.
//!
//! # What ships instead
//!
//! * [`GpuBackend`] - the port a `wgpu` implementation fills. **Deliberately not frozen**,
//!   exactly as `aura_infer::Backend` is: adding a backend adds a file and touches no
//!   caller, no contract and no ADR.
//! * [`Absent`] - the implementation that says no, once, with a typed code. Invariant 9: the
//!   absence of a GPU has a defined reduced behaviour, and the reduced behaviour is the same
//!   code path with a different engine underneath it.
//! * `crate::shaders` - the WGSL sources, compiled into the binary and checked against the
//!   Rust reference by `crates/aura-render/tests/shader_parity.rs`. A shader that drifts
//!   from the reference fails the build *today*, before a GPU exists to run it.
//!
//! # What the first real backend has to do
//!
//! Pass `crate::parity::compare` on every stage in `graph::ORDER`, at `parity::TOLERANCE`,
//! on the fixture set. Nothing else about it is prescribed here. `AURA-RENDER-8010`'s
//! runbook says what to do when a stage fails, and the first line of it is "do not widen the
//! tolerance".

use aura_core::errors::render::no_gpu_backend;
use aura_core::{AuraError, AuraResult};

use crate::contract::render::Backend;
use crate::graph::Stage;

/// An accelerator that can run one stage of the pipeline.
///
/// One stage at a time rather than a whole plan, because that is the granularity the parity
/// gate compares at: a backend that could only be tested end to end would fail as "the image
/// is wrong" rather than as "the tone stage is wrong".
pub trait GpuBackend: Send + Sync + std::fmt::Debug {
    /// Which backend this is.
    fn kind(&self) -> Backend;

    /// True when this backend implements a stage. A backend may implement some and defer the
    /// rest to the reference path; a mixed plan is slower than a full one and correct.
    fn supports(&self, stage: Stage) -> bool;

    /// Run one stage over an interleaved `f32` RGB buffer, in place.
    ///
    /// # Errors
    ///
    /// Whatever the device raises. A backend that fails mid-plan leaves the caller to fall
    /// back to the reference path for the remaining stages.
    fn run(&self, stage: Stage, rgb: &mut [f32], width: u32, height: u32) -> AuraResult<()>;

    /// The largest edge this device will render in one pass.
    fn max_texture(&self) -> u32;
}

/// The backend this build ships: none.
#[derive(Debug, Default, Clone, Copy)]
pub struct Absent;

impl Absent {
    /// The degradation to report, once per session.
    #[must_use]
    pub fn degradation() -> AuraError {
        no_gpu_backend("this build links no wgpu backend; see ADR-0029 section 4")
    }
}

impl GpuBackend for Absent {
    fn kind(&self) -> Backend {
        Backend::Cpu
    }

    fn supports(&self, _stage: Stage) -> bool {
        false
    }

    fn run(&self, stage: Stage, _rgb: &mut [f32], _width: u32, _height: u32) -> AuraResult<()> {
        Err(no_gpu_backend(format!(
            "stage {} has no accelerator in this build",
            stage.as_str()
        )))
    }

    fn max_texture(&self) -> u32 {
        0
    }
}

/// The backend a caller gets on this machine.
///
/// A function rather than a constant so the day a `wgpu` backend lands it probes here and
/// every caller keeps working.
#[must_use]
pub fn detect() -> Absent {
    Absent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_absent_backend_supports_nothing_and_says_why() {
        let backend = detect();
        assert_eq!(backend.kind(), Backend::Cpu);
        for stage in crate::graph::ORDER {
            assert!(!backend.supports(stage));
        }
        let mut buffer = [0.0f32; 3];
        let err = backend
            .run(Stage::Exposure, &mut buffer, 1, 1)
            .expect_err("must refuse");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8001");
    }

    #[test]
    fn the_degradation_is_a_degradation_and_not_a_failure() {
        let err = Absent::degradation();
        assert_eq!(err.severity, aura_core::Severity::Degraded);
        assert_eq!(err.recovery, aura_core::Recovery::Fallback);
    }
}
