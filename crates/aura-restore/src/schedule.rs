//! Where the heavy pixels are pushed, and when.
//!
//! Section 6.4: "Restoration never runs on the interactive path; it runs during export or as an
//! explicit background enhancement pass with progress and cancellation."
//!
//! ## The "never" is enforced by there being nowhere to say otherwise
//!
//! [`aura_core::contract::restore::RestoreWhen`] has two variants and no third, and the render
//! graph refuses independently: `graph::plan` marks `Stage::Restoration` `InteractivePath`
//! whenever `RenderPurpose::skip_heavy` is set. Two layers, and neither is a check somebody could
//! forget to write.
//!
//! ## The cloud arm is unreachable, and this module is where that is true
//!
//! [`where_to_run`] has no arm that produces [`RunWhere::Cloud`]. Section 7 of PHASE-22 says the
//! Cloud AI Gateway stays idle in this phase; section 2.1 lists an offload anyway; ADR-0047
//! section 7 resolves it in favour of section 7 and records why - an offload needs a provider that
//! can accept a 45 MP linear buffer, a measured cost, a cassette and a local GPU figure to be
//! faster than, and this build has none of the four.
//!
//! The variant exists because section 5 freezes it and a variant that is absent cannot be added
//! later without a contract change. `aura-restore` has no dependency that could reach a provider,
//! and `tests/no_network.rs` fails the build if one appears.

use aura_core::contract::restore::{RestoreCode, RestoreReason, RestoreWhen, RunWhere};

/// How many megapixels the processor path is willing to restore in one frame.
///
/// Forty-eight. Above this the reference path's wall clock stops being a wait and starts being a
/// hang: section 11's processor budget is 40 s for a 45 MP denoise, and that is with no
/// deconvolution and no face recovery on top. The pass works on 2048 px proxies, which is about
/// four megapixels, so this bound is about a caller that hands it something else.
pub const MAX_CPU_MEGAPIXELS: f32 = 48.0;

/// What this machine can do.
///
/// Deliberately not a hardware probe. Phase 03 owns hardware and `InferService` is the one way to
/// ask what a machine has; this is the two facts the scheduler needs, passed in by the caller
/// that already asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capacity {
    /// True when a device backend is linked and usable.
    ///
    /// False in this build: ADR-0029 section 4 links no `wgpu` backend, which is why four of
    /// section 11's five performance rows are waived.
    pub gpu: bool,
    /// True when the photographer has consented to sending pixels to a provider for this project.
    ///
    /// Read and never acted on in this build. It is here rather than absent so that the
    /// consent-first shape is visible in the signature: a future offload cannot be written that
    /// does not take it.
    pub cloud_consent: bool,
}

/// Where one frame's heavy pixels are pushed.
///
/// **There is no arm that returns [`RunWhere::Cloud`].** See the module header.
#[must_use]
pub fn where_to_run(capacity: Capacity, megapixels: f32) -> (RunWhere, Vec<RestoreReason>) {
    let mut reasons = vec![RestoreReason::plain(
        RestoreCode::ScheduledOffInteractive,
        0.1,
    )];
    if capacity.gpu {
        return (RunWhere::LocalGpu, reasons);
    }
    if megapixels > MAX_CPU_MEGAPIXELS {
        // Not a cloud fallback. A frame this large on the processor path is refused by the caller
        // - `decide` turns an empty plan into a stored no-op with a reason - because the
        // alternative is a wait a photographer would kill the application during.
        reasons.push(RestoreReason::plain(RestoreCode::RegionUnusable, -0.2));
    }
    (RunWhere::LocalCpu, reasons)
}

/// True when a frame of this size can be restored on the processor path at all.
#[must_use]
pub fn fits_on_cpu(megapixels: f32) -> bool {
    megapixels <= MAX_CPU_MEGAPIXELS
}

/// When one pass runs.
///
/// A pass triggered by an export runs at [`RestoreWhen::Export`]; anything else is the explicit
/// background enhancement pass. There is deliberately no third answer and no way for a caller to
/// ask for restoration while a slider is moving.
#[must_use]
pub const fn when_to_run(during_export: bool) -> RestoreWhen {
    if during_export {
        RestoreWhen::Export
    } else {
        RestoreWhen::Background
    }
}

#[cfg(test)]
// `-D warnings` on the command line beats the crate-level `cfg_attr(test, allow(..))`
// block, so a test that compares two floats it computed itself needs the allow here.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn nothing_in_this_build_schedules_a_frame_into_the_cloud() {
        // ADR-0047 section 7, as an exhaustive assertion rather than as an intention. Every
        // combination of the two capacity flags and a range of frame sizes.
        for gpu in [false, true] {
            for cloud_consent in [false, true] {
                for megapixels in [0.5_f32, 4.0, 24.0, 45.0, 100.0, 400.0] {
                    let (destination, _) =
                        where_to_run(Capacity { gpu, cloud_consent }, megapixels);
                    assert_ne!(
                        destination,
                        RunWhere::Cloud,
                        "gpu={gpu} consent={cloud_consent} mp={megapixels} reached the cloud"
                    );
                    assert!(!destination.leaves_the_device());
                }
            }
        }
    }

    #[test]
    fn a_device_backend_is_preferred_and_this_build_has_none() {
        let (with_gpu, _) = where_to_run(
            Capacity {
                gpu: true,
                cloud_consent: false,
            },
            45.0,
        );
        assert_eq!(with_gpu, RunWhere::LocalGpu);
        let (without, _) = where_to_run(Capacity::default(), 45.0);
        assert_eq!(without, RunWhere::LocalCpu);
    }

    #[test]
    fn every_scheduled_frame_says_it_was_kept_off_the_interactive_path() {
        let (_, reasons) = where_to_run(Capacity::default(), 4.0);
        assert!(reasons
            .iter()
            .any(|r| r.code == RestoreCode::ScheduledOffInteractive));
    }

    #[test]
    fn a_frame_too_large_for_the_processor_path_is_named_rather_than_offloaded() {
        let (destination, reasons) = where_to_run(Capacity::default(), MAX_CPU_MEGAPIXELS + 10.0);
        assert_eq!(destination, RunWhere::LocalCpu);
        assert!(
            reasons.len() > 1,
            "an oversized frame said nothing about it"
        );
        assert!(!fits_on_cpu(MAX_CPU_MEGAPIXELS + 0.1));
        assert!(fits_on_cpu(4.0));
    }

    #[test]
    fn there_are_exactly_two_occasions_and_neither_is_interactive() {
        assert_eq!(when_to_run(true), RestoreWhen::Export);
        assert_eq!(when_to_run(false), RestoreWhen::Background);
        assert_eq!(RestoreWhen::ALL.len(), 2);
    }
}
