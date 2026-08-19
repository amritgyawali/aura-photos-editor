//! The WGSL sources, compiled into the binary and checked against the reference.
//!
//! This build links no `wgpu` (ADR-0029 section 4), so nothing here is *executed*. It is
//! still load-bearing: `crates/aura-render/tests/shader_parity.rs` asserts that every stage
//! in `graph::ORDER` has an entry point, that every entry point names a stage, and that the
//! constants the two paths share - middle grey, the curve gamma, the tone band centres and
//! widths, the Bayer matrix, the luminance coefficients - appear in the WGSL with the same
//! values they have in Rust.
//!
//! That is the difference between shipping shaders and shipping decoration. A shader that
//! drifts from the reference fails the build today, months before a device exists to run it,
//! which is the only moment at which the drift is cheap to fix.

use crate::graph::{Stage, ORDER};

/// The input half: highlight recovery, white balance, the camera matrix, lens corrections.
pub const COLOUR: &str = include_str!("../shaders/colour.wgsl");

/// The creative point-wise half.
pub const TONE: &str = include_str!("../shaders/tone.wgsl");

/// The stages that read a neighbourhood.
pub const SPATIAL: &str = include_str!("../shaders/spatial.wgsl");

/// The output transform.
pub const OUTPUT: &str = include_str!("../shaders/output.wgsl");

/// PHASE-19. The luminosity mask and the mask-quality gate in front of it.
///
/// A **library** rather than a stage: a luminosity mask is what `stage_masks` multiplies a
/// generated mask's alpha by, and phases 20 to 22 will each want the same weighting from the
/// same place. It declares no `fn stage_` entry point, and `every_entry_point_names_a_stage`
/// is what keeps that true.
pub const LUMINOSITY_MASK: &str = include_str!("../shaders/luminosity_mask.wgsl");

/// PHASE-19. Frequency separation, three bands, of which two are returned.
pub const FREQ_SEP: &str = include_str!("../shaders/freq_sep.wgsl");

/// PHASE-19. Applying a local light plan.
pub const LOCAL_APPLY: &str = include_str!("../shaders/local_apply.wgsl");

/// Every source, with the file name it came from.
pub const SOURCES: [(&str, &str); 7] = [
    ("colour.wgsl", COLOUR),
    ("tone.wgsl", TONE),
    ("spatial.wgsl", SPATIAL),
    ("output.wgsl", OUTPUT),
    ("luminosity_mask.wgsl", LUMINOSITY_MASK),
    ("freq_sep.wgsl", FREQ_SEP),
    ("local_apply.wgsl", LOCAL_APPLY),
];

/// The entry point name for a stage. `exposure` becomes `stage_exposure`.
#[must_use]
pub fn entry_point(stage: Stage) -> String {
    format!("stage_{}", stage.as_str())
}

/// The source file a stage's entry point lives in, or `None` when it has none.
#[must_use]
pub fn source_for(stage: Stage) -> Option<(&'static str, &'static str)> {
    let needle = entry_point(stage);
    SOURCES
        .iter()
        .find(|(_, source)| source.contains(&format!("fn {needle}(")))
        .copied()
}

/// Constants that must read the same on both paths.
///
/// The list is the whole test. A constant that appears here and not in the WGSL, or with a
/// different value, is a parity failure that would otherwise show up as "the image is nearly
/// right" the first time a device runs.
#[must_use]
pub fn shared_constants() -> Vec<(&'static str, String)> {
    vec![
        ("MID_GREY", format!("{:.2}", crate::tonemap::MID_GREY)),
        ("CURVE_GAMMA", format!("{:.1}", crate::tonemap::CURVE_GAMMA)),
        ("KNEE", format!("{:.1}", aura_raw::colour::curve::KNEE)),
        ("luma.r", "0.262700".to_string()),
        ("luma.g", "0.677998".to_string()),
        ("luma.b", "0.059302".to_string()),
        // PHASE-19. The three constants the local light application shares with the
        // processor reference in `crate::local`. A shader that drifted from any of them
        // would change how far every face in the product gets lifted, and would do it
        // silently on the day a backend first ran.
        ("FACE_PIVOT", format!("{:.2}", crate::local::FACE_PIVOT)),
        (
            "SHADOWS_PER_EV",
            format!("{:.1}", crate::local::SHADOWS_PER_EV),
        ),
        (
            "HIGHLIGHTS_PER_EV",
            format!("{:.1}", crate::local::HIGHLIGHTS_PER_EV),
        ),
        (
            "SHAPING_UNIT_EV",
            format!("{:.3}", crate::local::SHAPING_UNIT_EV),
        ),
        (
            "MIN_MASK_CONFIDENCE",
            format!("{:.2}", aura_core::contract::local::MIN_MASK_CONFIDENCE),
        ),
        (
            "FULL_MASK_CONFIDENCE",
            format!("{:.2}", aura_core::contract::local::FULL_MASK_CONFIDENCE),
        ),
    ]
}

/// Every stage that has no entry point. Empty, and a test keeps it that way.
#[must_use]
pub fn stages_without_a_shader() -> Vec<Stage> {
    ORDER
        .iter()
        .copied()
        .filter(|stage| source_for(*stage).is_none())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stage_has_an_entry_point() {
        let missing = stages_without_a_shader();
        assert!(
            missing.is_empty(),
            "stages with no shader: {:?}",
            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_entry_point_names_a_stage() {
        let known: Vec<String> = ORDER.iter().map(|s| entry_point(*s)).collect();
        for (file, source) in SOURCES {
            for line in source.lines() {
                let trimmed = line.trim();
                let Some(rest) = trimmed.strip_prefix("fn stage_") else {
                    continue;
                };
                let Some(name) = rest.split('(').next() else {
                    continue;
                };
                let full = format!("stage_{name}");
                assert!(
                    known.contains(&full),
                    "{file} declares {full}, which is not a stage"
                );
            }
        }
    }

    #[test]
    fn the_shared_constants_appear_in_the_shaders_with_the_same_values() {
        let all: String = SOURCES
            .iter()
            .map(|(_, s)| *s)
            .collect::<Vec<_>>()
            .join("\n");
        for (name, value) in shared_constants() {
            assert!(
                all.contains(&value),
                "{name} is {value} in Rust and does not appear in any shader"
            );
        }
    }

    #[test]
    fn the_output_transform_is_the_only_shader_that_encodes_a_transfer_function() {
        for (file, source) in SOURCES {
            let encodes = source.contains("fn encode_srgb") || source.contains("fn encode_gamma");
            assert_eq!(
                encodes,
                file == "output.wgsl",
                "{file} must not bake tone; only the output transform may"
            );
        }
    }

    #[test]
    fn no_shader_uses_an_atomic() {
        // Section 6.2: no atomics in colour maths. A reduction that arrived by way of an
        // atomic would make a render non-deterministic in a way no golden test catches
        // reliably, because it would usually produce the right answer.
        for (file, source) in SOURCES {
            assert!(
                !source.contains("atomic"),
                "{file} uses an atomic in colour maths"
            );
        }
    }

    #[test]
    fn every_shader_with_an_entry_point_declares_the_frame_uniform() {
        // Narrowed in PHASE-19, when the first shader *libraries* arrived. A file that
        // declares no `fn stage_` is not dispatched over the frame - it is a set of helper
        // functions another shader calls - so it has no frame to know the dimensions of. The
        // property being asserted is unchanged: anything that is dispatched must know how big
        // the thing it is dispatched over is.
        for (file, source) in SOURCES {
            if !source.contains("fn stage_") {
                continue;
            }
            assert!(source.contains("struct Frame"), "{file} has no Frame block");
        }
    }

    #[test]
    fn a_shader_library_declares_no_entry_point_and_is_still_checked() {
        // The three PHASE-19 files are libraries, and every other property in this module -
        // no atomics, no encoding, the shared constants - still applies to them. A library
        // that quietly acquired an entry point would be a stage nothing scheduled.
        for name in ["luminosity_mask.wgsl", "freq_sep.wgsl", "local_apply.wgsl"] {
            let source = SOURCES
                .iter()
                .find(|(file, _)| *file == name)
                .map(|(_, source)| *source);
            let source = source.unwrap_or_else(|| panic!("{name} is not in SOURCES"));
            assert!(
                !source.contains("fn stage_"),
                "{name} declares an entry point but nothing schedules it"
            );
        }
    }
}
