//! Run in a separate process with AURA_DISABLE_GPU=1; no environment is mutated in tests.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]
use aura_core::clock::FixedClock;
use aura_render::{
    output, Backend, CpuEngine, Frame, FrameSource, OutputSpec, RenderLevel, RenderPurpose,
    RenderService,
};
use std::sync::Arc;

#[derive(Debug)]
struct Source(Frame);
impl FrameSource for Source {
    fn frame(&self, _: &aura_core::PhotoId, _: RenderLevel) -> aura_core::AuraResult<Frame> {
        Ok(self.0.clone())
    }
}

#[test]
#[ignore = "run with AURA_DISABLE_GPU=1 in a separate process"]
fn the_explicit_disable_switch_uses_identical_cpu_pixels_and_cache_identity() {
    assert_eq!(std::env::var("AURA_DISABLE_GPU").as_deref(), Ok("1"));
    let frame = Frame::working(vec![0.18; 1024 * 1024 * 3], 1024, 1024, "fixture");
    let recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
    let source = Arc::new(Source(frame.clone()));
    let reference = CpuEngine::new(source.clone(), Arc::new(FixedClock::default()));
    let automatic = CpuEngine::new(source, Arc::new(FixedClock::default())).with_gpu();
    let original = reference
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Interactive,
            &OutputSpec::default(),
        )
        .unwrap();
    let fallback = automatic
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Interactive,
            &OutputSpec::default(),
        )
        .unwrap();
    assert_eq!(fallback.data, original.data);
    assert_eq!(fallback.render_hash, original.render_hash);
    assert_eq!(fallback.backend, "cpu");
    assert_eq!(automatic.capabilities().backend, Backend::Cpu);
    assert!(automatic.degradation().is_some());
    assert!(output::gpu_identity().is_none());
    assert!(output::gpu_unavailable_reason()
        .unwrap()
        .contains("disabled"));
}
