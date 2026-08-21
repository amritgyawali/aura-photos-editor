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

/// PHASE-18. Guided upsampling and feathering of a stored mask.
///
/// Two files rather than one because they are two jobs with two lifetimes: a mask is upsampled
/// **once per render session** and re-uploaded only when the mask changes, and it is composited
/// on **every parameter change**. Section 6.3's "GPU path uploads masks as textures once per
/// render session and reuses them across parameter changes" is that split, and a single file
/// would invite a single pipeline that redid the expensive half on every slider move.
pub const MASK_UPSAMPLE: &str = include_str!("../shaders/mask_upsample.wgsl");

/// PHASE-18. Compositing an edited buffer back through a mask, in linear light.
pub const MASK_COMPOSITE: &str = include_str!("../shaders/mask_composite.wgsl");

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

/// PHASE-20. Three-band frequency separation, of which the third is measured rather than moved.
///
/// A **library**, like [`LUMINOSITY_MASK`]. Phase 19 `freq_sep.wgsl` returns two bands and
/// cannot reach the third by construction; this one returns all three, because the texture
/// guarantee is a ratio of high-band energies and a band nobody can measure is a guarantee
/// nobody can enforce.
pub const FREQ_BANDS: &str = include_str!("../shaders/freq_bands.wgsl");

/// PHASE-20. Patch synthesis: one temporary mark removed, with the texture of the skin kept.
pub const INPAINT_PATCH: &str = include_str!("../shaders/inpaint_patch.wgsl");

/// PHASE-20. The retouch stage: under-eye correction and tone evening through a skin mask.
///
/// This carries `stage_retouch`. Phase 14 left a pass-through of that name in `spatial.wgsl` so
/// that `every_stage_has_an_entry_point` could pass before an operator existed; phase 20
/// retired it, because two entry points with one name and one of them doing nothing is the
/// drift this test exists to catch.
pub const RETOUCH_APPLY: &str = include_str!("../shaders/retouch_apply.wgsl");

/// PHASE-21. The micro-retouch operators: flyaway, teeth, sclera and iris, through phase 18's
/// regions.
///
/// A library rather than a stage, for the reason [`INPAINT_PATCH`] and [`FREQ_BANDS`] are: the
/// micro operations run inside the retouch stage phase 20 already owns, one dispatch per
/// operation, and a second entry in `graph::ORDER` would be a change to a frozen contract for no
/// behavioural reason.
pub const MICRO_APPLY: &str = include_str!("../shaders/micro_apply.wgsl");

/// PHASE-21. Glare: the conservative reduction, and the composite of an aligned sibling patch.
///
/// It composites and does not decide what may be composited - the specular test, the area cap
/// and the alignment search all happen in `aura_retouch::micro::borrow` first, and the patch
/// arrives as pixels rather than as a source and a transform so that the decision cannot be
/// re-made here by accident.
pub const MICRO_BORROW: &str = include_str!("../shaders/micro_borrow.wgsl");

/// Every source, with the file name it came from.
pub const SOURCES: [(&str, &str); 14] = [
    ("colour.wgsl", COLOUR),
    ("tone.wgsl", TONE),
    ("spatial.wgsl", SPATIAL),
    ("output.wgsl", OUTPUT),
    ("mask_upsample.wgsl", MASK_UPSAMPLE),
    ("mask_composite.wgsl", MASK_COMPOSITE),
    ("luminosity_mask.wgsl", LUMINOSITY_MASK),
    ("freq_sep.wgsl", FREQ_SEP),
    ("local_apply.wgsl", LOCAL_APPLY),
    ("freq_bands.wgsl", FREQ_BANDS),
    ("inpaint_patch.wgsl", INPAINT_PATCH),
    ("retouch_apply.wgsl", RETOUCH_APPLY),
    ("micro_apply.wgsl", MICRO_APPLY),
    ("micro_borrow.wgsl", MICRO_BORROW),
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
        // PHASE-18. The two mask constants that exist on both sides. The feather fraction
        // decides how wide a soft edge is at every render level and the matting epsilon
        // decides how much a change in the guide has to matter before the matte follows it;
        // either of them drifting is a boundary that looks right on one path and not the other.
        (
            "FEATHER_MAX_FRACTION",
            format!("{MASK_FEATHER_MAX_FRACTION:.2}"),
        ),
        ("MASK_EPSILON", format!("{MASK_EPSILON:e}")),
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
        // PHASE-20. The six constants the retouch operators share with the processor
        // reference in `crate::retouch`. Two of them decide whether a mark is removed at all -
        // the donor search radius and the transplant boundary - and the other four decide how
        // far an under-eye correction and a tone evening may go. A shader that drifted from
        // any of them would retouch a wedding differently from the preview the photographer
        // approved, and would do it on the day a backend first ran.
        (
            "DONOR_DISTANCE",
            format!("{:.2}", crate::retouch::DONOR_DISTANCE),
        ),
        (
            "DONOR_MAX_DELTA",
            format!("{:.2}", crate::retouch::DONOR_MAX_DELTA),
        ),
        (
            "TRANSPLANT_FRACTION",
            format!("{:.2}", crate::retouch::TRANSPLANT_FRACTION),
        ),
        (
            "PATCH_FEATHER",
            format!("{:.2}", crate::retouch::PATCH_FEATHER),
        ),
        (
            "UNDEREYE_DROP",
            format!("{:.2}", crate::retouch::UNDEREYE_DROP),
        ),
        (
            "UNDEREYE_WIDTH",
            format!("{:.2}", crate::retouch::UNDEREYE_WIDTH),
        ),
        ("SHADOW_SPAN", format!("{:.2}", crate::retouch::SHADOW_SPAN)),
        (
            "MAX_EVENING_MID",
            format!("{:.2}", aura_core::contract::retouch::MAX_EVENING_MID),
        ),
        // PHASE-21. The eight constants the micro operators share with the processor reference
        // in `crate::micro`. Two of them decide whether an operation happens at all - the
        // specular floor, which is what keeps a catchlight out of every operator, and the
        // clipped floor, which is half of what permits a borrow - and the other six are the
        // ceilings the contract owns. A shader that drifted from any of them would deliver a
        // gallery that differs from the preview a photographer approved, on the day a backend
        // first runs.
        (
            "MICRO_SPECULAR_FLOOR",
            format!("{:.2}", crate::micro::SPECULAR_FLOOR),
        ),
        (
            "MICRO_CLIPPED_FLOOR",
            format!("{:.3}", crate::micro::CLIPPED_FLOOR),
        ),
        (
            "MAX_TEETH_LUMA_EV",
            format!("{:.2}", aura_core::contract::micro::MAX_TEETH_LUMA_EV),
        ),
        (
            "MAX_TEETH_YELLOW",
            format!("{:.2}", aura_core::contract::micro::MAX_TEETH_YELLOW),
        ),
        (
            "MAX_SCLERA",
            format!("{:.2}", aura_core::contract::micro::MAX_SCLERA),
        ),
        (
            "MAX_IRIS_CLARITY",
            format!("{:.2}", aura_core::contract::micro::MAX_IRIS_CLARITY),
        ),
        (
            "MAX_FLYAWAY_STRENGTH",
            format!("{:.2}", aura_core::contract::micro::MAX_FLYAWAY_STRENGTH),
        ),
        (
            "MIN_SPECULAR_FRACTION",
            format!("{:.2}", aura_core::contract::micro::MIN_SPECULAR_FRACTION),
        ),
        (
            "MIN_ALIGNMENT",
            format!("{:.2}", aura_core::contract::micro::MIN_ALIGNMENT),
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

/// The largest feather, as a fraction of the plane's short edge.
///
/// Mirrors `aura_vision::mask::algebra::FEATHER_MAX_FRACTION`. It is duplicated rather than
/// imported because `aura-render` does not depend on `aura-vision` and must not - the edge runs
/// the other way - and the parity test is what stops the duplicate from drifting.
pub const MASK_FEATHER_MAX_FRACTION: f32 = 0.08;

/// The guided filter's regularisation. Mirrors `aura_vision::mask::matting::EPSILON`.
pub const MASK_EPSILON: f32 = 1e-4;

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
    fn every_dispatched_shader_declares_the_frame_uniform() {
        // Every shader that is *dispatched* has to be told the size of the buffer it is
        // walking, because a compute dispatch is rounded up to whole workgroups and an
        // invocation that did not know where the frame ended would write past it.
        //
        // PHASE-18 widened this from `struct Frame` to "a uniform block carrying a width and a
        // height". The two mask shaders take *two* grids - a stored plane and the grid it is
        // composited onto - so a block called `Frame` would have had to mean one of them, and
        // naming which one in a struct called `Frame` is how the wrong one gets used.
        //
        // PHASE-19 narrowed the *set*, when the first shader libraries arrived. A file with no
        // `@compute` entry point is never dispatched over anything - it is helper functions
        // another shader calls - so it has no frame to know the dimensions of. The property is
        // unchanged: anything dispatched must know how big the thing it walks is. The
        // discriminator is `@compute` rather than `fn stage_`, because `mask_upsample` and
        // `mask_composite` are dispatched and name no stage in `graph::ORDER`.
        for (file, source) in SOURCES {
            if !source.contains("@compute") {
                continue;
            }
            assert!(
                source.contains("width: u32") && source.contains("height: u32"),
                "{file} declares no frame dimensions"
            );
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
