//! The WGSL sources, held to the Rust reference.
//!
//! This build links no `wgpu`, so nothing here is executed. What is checked is that the
//! shaders **cannot drift** while a backend does not exist to notice: every stage has an
//! entry point, every entry point names a stage, the shared constants agree, and no shader
//! bakes tone or takes an atomic.
//!
//! The value of this file is entirely in its timing. Shipping shaders that nobody compiles
//! is how a GPU backend arrives a year later against a reference that has moved twice.

use aura_render::graph::{Stage, ORDER};
use aura_render::shaders::{self, SOURCES};

#[test]
fn every_stage_has_an_entry_point() {
    let missing: Vec<&str> = shaders::stages_without_a_shader()
        .iter()
        .map(|s| s.as_str())
        .collect();
    assert!(missing.is_empty(), "stages with no shader: {missing:?}");
}

#[test]
fn every_entry_point_names_a_stage() {
    let known: Vec<String> = ORDER.iter().map(|s| shaders::entry_point(*s)).collect();
    for (file, source) in SOURCES {
        for line in source.lines() {
            let Some(rest) = line.trim().strip_prefix("fn stage_") else {
                continue;
            };
            let Some(name) = rest.split('(').next() else {
                continue;
            };
            let full = format!("stage_{name}");
            assert!(
                known.contains(&full),
                "{file} declares {full}, which is not a stage in graph::ORDER"
            );
        }
    }
}

#[test]
fn the_shared_constants_agree_between_the_two_paths() {
    let all: String = SOURCES
        .iter()
        .map(|(_, s)| *s)
        .collect::<Vec<_>>()
        .join("\n");
    for (name, value) in shaders::shared_constants() {
        assert!(
            all.contains(&value),
            "{name} is {value} on the processor path and does not appear in any shader"
        );
    }
}

#[test]
fn only_the_output_shader_bakes_tone() {
    for (file, source) in SOURCES {
        let encodes = source.contains("fn encode_srgb") || source.contains("fn encode_gamma");
        assert_eq!(
            encodes,
            file == "output.wgsl",
            "{file}: only the output transform may apply a transfer function"
        );
    }
}

#[test]
fn no_shader_uses_an_atomic_or_a_workgroup_reduction() {
    for (file, source) in SOURCES {
        assert!(!source.contains("atomic"), "{file} uses an atomic");
        assert!(
            !source.contains("workgroupBarrier"),
            "{file} synchronises inside a workgroup, which is a reduction by another name"
        );
    }
}

#[test]
fn the_shaders_declare_f32_storage_rather_than_f16() {
    // ADR-0029 section 3: `f16` may be a texture format, never the arithmetic. A shader that
    // declared `f16` locals would round twice per stage and eat the parity tolerance.
    // The *word* f16 appears in colour.wgsl's header, which is where the decision is
    // explained. What must not appear is f16 as a type or as an enabled extension.
    for (file, source) in SOURCES {
        for usage in ["<f16>", ": f16", "f16(", "enable f16", "shader-f16"] {
            assert!(
                !source.contains(usage),
                "{file} computes in f16 ({usage}); see ADR-0029 section 3"
            );
        }
    }
}

#[test]
fn the_geometry_shader_uses_pixel_centres() {
    // The half-pixel bug, caught once on the processor path and pinned on both. A crop that
    // resampled by half a pixel would soften every straightened frame.
    let (_, spatial) = SOURCES
        .iter()
        .find(|(name, _)| *name == "spatial.wgsl")
        .copied()
        .expect("spatial.wgsl");
    assert!(
        spatial.contains("(left + right - 1.0)"),
        "the geometry shader must centre on pixel centres, not edges"
    );
}

#[test]
fn each_stage_entry_point_is_in_the_file_its_subject_belongs_to() {
    for stage in ORDER {
        let (file, _) = shaders::source_for(stage).expect("every stage has a shader");
        let expected = match stage {
            Stage::HighlightRecovery
            | Stage::WhiteBalance
            | Stage::CameraMatrix
            | Stage::LensVignette
            | Stage::LensDistortion
            | Stage::LensCa => "colour.wgsl",
            Stage::OutputTransform => "output.wgsl",
            Stage::Exposure
            | Stage::Tone
            | Stage::Contrast
            | Stage::Curve
            | Stage::Hsl
            | Stage::Vibrance
            | Stage::Monochrome => "tone.wgsl",
            // PHASE-20. The retouch operators are their own file, and the pass-through phase 14
            // left in `spatial.wgsl` is gone. A stage whose entry point sits beside the sharpen
            // kernel is a stage nobody finds when they go looking for the retoucher.
            Stage::Retouch => "retouch_apply.wgsl",
            _ => "spatial.wgsl",
        };
        assert_eq!(file, expected, "{} is in the wrong file", stage.as_str());
    }
}
