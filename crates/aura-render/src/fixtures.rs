//! Frames with known answers.
//!
//! Every golden render, parity comparison and budget measurement in this crate runs over one
//! of these. They are **authored synthetic pixels**, not photographs, and the exit report
//! says so plainly: there are no camera files in this repository and no photographed
//! ColorChecker (phase 02's open exit conditions, ADR-0006), so the golden suite is a
//! determinism and regression gate rather than a claim about colour accuracy.
//!
//! What they *do* prove is everything the arithmetic can be wrong about on its own: that a
//! neutral stays neutral, that a tiled render equals a whole one, that two runs are
//! byte-identical, that a stage does what its slider says, and that a curve does not
//! overshoot.

use std::sync::Arc;

use aura_core::{AuraResult, PhotoId};

use crate::contract::render::RenderLevel;
use crate::cpu::{Frame, FrameSource};
use crate::graph::InputKind;

/// A frame source that always answers with the same frame.
#[derive(Debug)]
pub struct StaticSource {
    frame: Frame,
}

impl StaticSource {
    /// One frame, for every request.
    #[must_use]
    pub const fn new(frame: Frame) -> Self {
        Self { frame }
    }

    /// Behind an `Arc`, which is what `CpuEngine::new` takes.
    #[must_use]
    pub fn shared(frame: Frame) -> Arc<dyn FrameSource> {
        Arc::new(Self::new(frame))
    }
}

impl FrameSource for StaticSource {
    fn frame(&self, _image: &PhotoId, _level: RenderLevel) -> AuraResult<Frame> {
        Ok(self.frame.clone())
    }
}

/// A flat neutral frame at one linear value.
///
/// The most useful fixture in the crate: a neutral must stay neutral through every stage,
/// and a stage that tints one is a stage with a channel-order bug or a matrix that is not
/// white-normalised.
#[must_use]
pub fn grey_frame(width: u32, height: u32, value: f32) -> Frame {
    Frame::working(
        vec![value; (width as usize) * (height as usize) * 3],
        width,
        height,
        "Bench-01",
    )
}

/// A horizontal luminance ramp from near-black to above white.
///
/// Runs to 1.6 in linear light rather than stopping at 1.0, because the working space is
/// scene-referred: a specular highlight on a ring is brighter than white, and a stage that
/// clips at 1.0 loses it. Every tone test uses this frame to catch exactly that.
#[must_use]
pub fn ramp_frame(width: u32, height: u32) -> Frame {
    let mut rgb = vec![0.0f32; (width as usize) * (height as usize) * 3];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let value = 0.002 + (x as f32 / width.max(1) as f32) * 1.6;
            for channel in 0..3 {
                if let Some(slot) = rgb.get_mut((y * width as usize + x) * 3 + channel) {
                    *slot = value;
                }
            }
        }
    }
    Frame::working(rgb, width, height, "Bench-01")
}

/// A frame with detail at several scales and genuine colour.
///
/// Used by the tiling and parity tests, where a flat or monotone frame would pass by
/// accident: a seam on a tile boundary is invisible on a ramp and obvious on this.
#[must_use]
pub fn detail_frame(width: u32, height: u32) -> Frame {
    let mut rgb = vec![0.0f32; (width as usize) * (height as usize) * 3];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let fx = x as f32 / width.max(1) as f32;
            let fy = y as f32 / height.max(1) as f32;
            // Three scales, so a blur at any radius has something to remove.
            let coarse = (fx * std::f32::consts::TAU).sin() * 0.25 + 0.4;
            let medium = ((fx + fy) * 25.13).sin() * 0.08;
            let fine = ((x as f32) * 1.9 + (y as f32) * 2.7).sin() * 0.03;
            let base = (coarse + medium + fine).max(0.001);
            // Genuine colour, so a hue rotation or a saturation change is measurable.
            let values = [base * 1.15, base, base * 0.8 + fy * 0.1];
            for (channel, value) in values.iter().enumerate() {
                if let Some(slot) = rgb.get_mut((y * width as usize + x) * 3 + channel) {
                    *slot = value.max(0.0);
                }
            }
        }
    }
    Frame::working(rgb, width, height, "Bench-01")
}

/// A frame with a blown highlight in one channel and detail in the others.
///
/// Camera-native, because highlight recovery is a camera-native stage: this is the fixture
/// that proves recovery runs before white balance rather than after it.
#[must_use]
pub fn clipped_frame(width: u32, height: u32) -> Frame {
    let mut rgb = vec![0.0f32; (width as usize) * (height as usize) * 3];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let fx = x as f32 / width.max(1) as f32;
            // The left half is ordinary; the right half blows green first, then red.
            let (r, g, b) = if fx < 0.5 {
                (0.3 + fx, 0.28 + fx, 0.25 + fx)
            } else {
                (0.9 + fx * 0.6, 1.0, 0.8 + fx * 0.3)
            };
            for (channel, value) in [r, g.min(1.0), b].iter().enumerate() {
                if let Some(slot) = rgb.get_mut((y * width as usize + x) * 3 + channel) {
                    *slot = *value;
                }
            }
        }
    }
    Frame {
        rgb,
        width,
        height,
        kind: InputKind::CameraNative,
        clip: [1.0, 1.0, 1.0],
        camera: "Bench-01".to_string(),
        origin: (0, 0),
        full: (width, height),
    }
}

/// A colour-target frame: twenty-four flat patches in a fixed order.
///
/// The shape of a ColorChecker without being one. The values are authored rather than
/// measured, so the suite proves that a patch renders *the same way every time and in the
/// same relationship to its neighbours* - which is the regression the golden gate is for -
/// and proves nothing about how a real chart would render. That distinction is condition C2
/// in the phase 14 exit report.
#[must_use]
pub fn patch_frame(patch: u32) -> Frame {
    // Colour values, not mathematical constants: 0.318 here is a patch, not 1/pi.
    #[allow(clippy::approx_constant)]
    const PATCHES: [[f32; 3]; 24] = [
        [0.111, 0.076, 0.058],
        [0.400, 0.318, 0.256],
        [0.183, 0.186, 0.271],
        [0.108, 0.134, 0.064],
        [0.269, 0.244, 0.410],
        [0.350, 0.560, 0.480],
        [0.520, 0.230, 0.049],
        [0.121, 0.109, 0.330],
        [0.400, 0.104, 0.129],
        [0.092, 0.049, 0.116],
        [0.352, 0.480, 0.083],
        [0.583, 0.352, 0.045],
        [0.056, 0.055, 0.253],
        [0.098, 0.290, 0.083],
        [0.312, 0.043, 0.052],
        [0.700, 0.550, 0.038],
        [0.372, 0.101, 0.253],
        [0.021, 0.239, 0.324],
        [0.880, 0.880, 0.880],
        [0.580, 0.580, 0.580],
        [0.360, 0.360, 0.360],
        [0.190, 0.190, 0.190],
        [0.090, 0.090, 0.090],
        [0.031, 0.031, 0.031],
    ];

    let across = 6u32;
    let width = across * patch;
    let height = 4 * patch;
    let mut rgb = vec![0.0f32; (width as usize) * (height as usize) * 3];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let index = (y / patch as usize) * across as usize + (x / patch as usize);
            let colour = PATCHES.get(index).copied().unwrap_or([0.0; 3]);
            for (channel, value) in colour.iter().enumerate() {
                if let Some(slot) = rgb.get_mut((y * width as usize + x) * 3 + channel) {
                    *slot = *value;
                }
            }
        }
    }
    Frame::working(rgb, width, height, "Bench-01")
}

/// The eight synthetic bench bodies, as frames of the same scene.
///
/// Section 10.1 asks for golden renders across twelve camera bodies. Eight is what
/// `aura_raw::colour::profile` ships and what can be measured exactly; the gap between eight
/// synthetic bodies and twelve real ones is condition C2 in the phase 14 exit report rather
/// than a number quietly rounded up.
/// Camera-native, not working-space, and that is the whole point: a body's profile is
/// applied by `Stage::CameraMatrix`, which the graph correctly skips for pixels that already
/// arrived in the working space. Eight working-space frames would render eight identical
/// files and the cross-body suite would prove nothing.
#[must_use]
pub fn bench_bodies(width: u32, height: u32) -> Vec<Frame> {
    crate::profiles::known_bodies()
        .keys()
        .map(|model| {
            let base = detail_frame(width, height);
            Frame {
                rgb: base.rgb,
                width: base.width,
                height: base.height,
                kind: InputKind::CameraNative,
                clip: [1.6, 1.0, 1.5],
                camera: model.clone(),
                origin: (0, 0),
                full: (base.width, base.height),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ramp_runs_past_white() {
        let frame = ramp_frame(64, 4);
        let brightest = frame.rgb.iter().copied().fold(0.0f32, f32::max);
        assert!(brightest > 1.0, "a scene-referred ramp must exceed 1.0");
    }

    #[test]
    fn every_fixture_is_the_length_it_claims() {
        for frame in [
            grey_frame(8, 6, 0.2),
            ramp_frame(8, 6),
            detail_frame(8, 6),
            clipped_frame(8, 6),
        ] {
            assert_eq!(
                frame.rgb.len(),
                frame.width as usize * frame.height as usize * 3
            );
        }
    }

    #[test]
    fn the_patch_frame_has_twenty_four_distinct_patches() {
        let frame = patch_frame(4);
        assert_eq!((frame.width, frame.height), (24, 16));
        let mut seen = std::collections::BTreeSet::new();
        for y in (2..frame.height as usize).step_by(4) {
            for x in (2..frame.width as usize).step_by(4) {
                let base = (y * frame.width as usize + x) * 3;
                let key = frame
                    .rgb
                    .get(base..base + 3)
                    .map(|v| format!("{v:.3?}"))
                    .unwrap_or_default();
                seen.insert(key);
            }
        }
        assert_eq!(seen.len(), 24);
    }

    #[test]
    fn the_bench_bodies_are_eight_camera_native_frames_that_each_name_themselves() {
        let frames = bench_bodies(8, 8);
        assert_eq!(frames.len(), 8);
        for frame in frames {
            assert!(frame.camera.starts_with("Bench-"));
            assert_eq!(
                frame.kind,
                InputKind::CameraNative,
                "a working-space frame skips the camera matrix and renders identically"
            );
        }
    }

    #[test]
    fn the_clipped_frame_is_camera_native() {
        assert_eq!(clipped_frame(8, 8).kind, InputKind::CameraNative);
    }
}
