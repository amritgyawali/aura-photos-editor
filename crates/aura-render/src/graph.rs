//! The render graph: which stages run, in what order, and what a changed parameter costs.
//!
//! # The order is the contract
//!
//! [`Stage`]'s declaration order **is** the pipeline order, and
//! `crates/aura-render/tests/stage_order.rs` asserts the three orderings that matter:
//! highlight recovery before white balance, white balance before the camera matrix, and the
//! output transform last. ADR-0029 section 2 has the argument for each.
//!
//! # Invalidation is downstream-only
//!
//! Section 6.3: *"parameter changes invalidate only downstream stages"*. A photographer
//! moving the vibrance slider should not re-run the demosaic, the white balance or the tone
//! curve - and [`earliest_affected`] is how the interactive path knows. It maps a dotted
//! recipe path to the first stage that reads it, so a cache holding the working buffer after
//! stage *n* can resume from there.
//!
//! The mapping is exhaustive over the schema and
//! `every_recipe_path_maps_to_a_stage_or_is_deliberately_inert` is the test that keeps it
//! that way: a new parameter that no stage claims would silently never invalidate anything,
//! which is a slider that appears to do nothing until the cache is cleared.

use aura_recipe::{Recipe, ENGINE};

use crate::contract::render::{OutputSpec, RenderNote, RenderPurpose, SkipReason};

/// One stage of the pipeline, in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Reconstruct clipped channels. **Before white balance**, deliberately.
    HighlightRecovery,
    /// Temperature and tint, as channel multipliers on camera-native RGB.
    WhiteBalance,
    /// Camera native into linear Rec.2020 at D65. The end of the input half.
    CameraMatrix,
    /// Lens vignette correction.
    LensVignette,
    /// Lens distortion correction. Needs a lens profile.
    LensDistortion,
    /// Lateral chromatic aberration correction. Needs a lens profile.
    LensCa,
    /// Chroma and luminance noise reduction.
    NoiseReduction,
    /// Exposure, in stops.
    Exposure,
    /// Highlights, shadows, whites, blacks.
    Tone,
    /// The S-curve about middle grey.
    Contrast,
    /// The point curve.
    Curve,
    /// Per-band hue, saturation and luminance.
    Hsl,
    /// Coarse local contrast.
    Clarity,
    /// Fine local contrast.
    Texture,
    /// Haze removal.
    Dehaze,
    /// Vibrance and saturation.
    Vibrance,
    /// Black-and-white conversion with a per-band mix.
    Monochrome,
    /// Local masks and the parameters inside them.
    Masks,
    /// Retouch operators.
    Retouch,
    /// Denoise, face recovery, deblur.
    Restoration,
    /// Capture sharpening.
    Sharpen,
    /// Crop, rotation and perspective.
    Geometry,
    /// Tone map, gamut, transfer, quantise. **The only place tone is baked.**
    OutputTransform,
}

/// Every stage, in execution order. The array *is* the pipeline.
pub const ORDER: [Stage; 23] = [
    Stage::HighlightRecovery,
    Stage::WhiteBalance,
    Stage::CameraMatrix,
    Stage::LensVignette,
    Stage::LensDistortion,
    Stage::LensCa,
    Stage::NoiseReduction,
    Stage::Exposure,
    Stage::Tone,
    Stage::Contrast,
    Stage::Curve,
    Stage::Hsl,
    Stage::Clarity,
    Stage::Texture,
    Stage::Dehaze,
    Stage::Vibrance,
    Stage::Monochrome,
    Stage::Masks,
    Stage::Retouch,
    Stage::Restoration,
    Stage::Sharpen,
    Stage::Geometry,
    Stage::OutputTransform,
];

impl Stage {
    /// Stable slug for telemetry, notes and the shader file name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HighlightRecovery => "highlight_recovery",
            Self::WhiteBalance => "white_balance",
            Self::CameraMatrix => "camera_matrix",
            Self::LensVignette => "lens_vignette",
            Self::LensDistortion => "lens_distortion",
            Self::LensCa => "lens_ca",
            Self::NoiseReduction => "noise_reduction",
            Self::Exposure => "exposure",
            Self::Tone => "tone",
            Self::Contrast => "contrast",
            Self::Curve => "curve",
            Self::Hsl => "hsl",
            Self::Clarity => "clarity",
            Self::Texture => "texture",
            Self::Dehaze => "dehaze",
            Self::Vibrance => "vibrance",
            Self::Monochrome => "monochrome",
            Self::Masks => "masks",
            Self::Retouch => "retouch",
            Self::Restoration => "restoration",
            Self::Sharpen => "sharpen",
            Self::Geometry => "geometry",
            Self::OutputTransform => "output_transform",
        }
    }

    /// Position in the pipeline, from zero.
    #[must_use]
    pub fn index(self) -> usize {
        ORDER.iter().position(|s| *s == self).unwrap_or(usize::MAX)
    }

    /// Pixels of neighbourhood this stage reads on each side.
    ///
    /// Zero for a point operation. The tiler sums these across the enabled stages and
    /// decodes that much halo around every tile, which is what makes a tiled render
    /// bit-identical to a whole-frame one. See `Plan::halo` for why it is a sum.
    #[must_use]
    pub const fn halo(self) -> u32 {
        match self {
            // A three-pass box blur of radius r reads 3r away. These are that reach, not an
            // estimate of it: the chroma pass runs at r <= 4 and the luminance pass at
            // r <= 3, plus one pixel for the Sobel.
            Self::NoiseReduction => 16,
            // r = 16.
            Self::Clarity => 48,
            // Twelve for both, arrived at from different radii: texture blurs at r = 4, and
            // sharpening at r <= 3 plus one pixel for the Sobel. They share an arm because
            // the numbers are equal, not because the stages are.
            Self::Texture | Self::Sharpen => 12,
            // r = 8 on the dark channel.
            Self::Dehaze => 24,
            Self::LensDistortion | Self::LensCa | Self::Geometry => 8,
            _ => 0,
        }
    }

    /// True when this stage reads only the pixel it writes.
    #[must_use]
    pub const fn is_point_wise(self) -> bool {
        self.halo() == 0
    }

    /// True when the interactive path may skip it until the photographer zooms in.
    ///
    /// Section 6.3 names restoration and heavy retouch. Clarity and dehaze are here too:
    /// both are wide-radius blurs whose cost dominates a 2048 px proxy re-render and whose
    /// effect at that size is a few pixels of local contrast.
    #[must_use]
    pub const fn is_heavy(self) -> bool {
        matches!(self, Self::Restoration | Self::Retouch | Self::Dehaze)
    }
}

/// What will run, and what will not.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Stages to run, in order.
    pub stages: Vec<Stage>,
    /// Stages that will not run, and why.
    pub notes: Vec<RenderNote>,
}

impl Plan {
    /// The halo the tiler needs for this plan.
    ///
    /// The **sum** of the enabled stages' reaches, not the maximum. Stages compose: a pixel
    /// committed after sharpening needs correct values twelve pixels away *after clarity*,
    /// and those need correct values forty-eight pixels away in the input. Taking the
    /// maximum would be right for one spatial stage and wrong for two, and the failure would
    /// be a faint seam on a tile boundary that only appears on large exports.
    #[must_use]
    pub fn halo(&self) -> u32 {
        self.stages.iter().map(|s| s.halo()).sum()
    }

    /// True when this plan carries a note the photographer should see.
    #[must_use]
    pub fn has_caveats(&self) -> bool {
        self.notes.iter().any(|n| n.reason.is_a_caveat())
    }

    /// The stage slugs, for telemetry's `stages_run`.
    #[must_use]
    pub fn slugs(&self) -> Vec<String> {
        self.stages.iter().map(|s| s.as_str().to_string()).collect()
    }
}

/// Where the pixels came from, which decides whether the input half runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// Camera-native RGB: highlight recovery, white balance and the matrix all run.
    CameraNative,
    /// Already in the working space - which is what phase 02's proxy delivers. The input
    /// half is skipped with `SkipReason::UpstreamAlready`, and white balance runs anyway
    /// because a temperature slider still has to do something.
    Working,
}

/// Which capabilities this build has. Every one of them is false today except the geometry
/// half that phase 14 itself ships.
///
/// Four booleans rather than a flag set: each names a *phase*,  reads them one at a
/// time, and a caller constructing one should have to say which of the four it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Capabilities {
    /// A mask generator for face, subject, background, sky, skin and brush. Phase 18.
    pub mask_generators: bool,
    /// Retouch operators. Phases 20 and 21.
    pub retouch_operators: bool,
    /// Restoration models. Phase 22.
    pub restoration: bool,
    /// Perspective correction and lens distortion models. Phase 23.
    pub geometry_models: bool,
}

impl Default for Capabilities {
    /// What phase 14 ships: none of the four.
    fn default() -> Self {
        Self {
            mask_generators: false,
            retouch_operators: false,
            restoration: false,
            geometry_models: false,
        }
    }
}

/// Decide what runs.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn plan(recipe: &Recipe, purpose: RenderPurpose, input: InputKind, caps: Capabilities) -> Plan {
    let mut stages = Vec::with_capacity(ORDER.len());
    let mut notes = Vec::new();
    let g = &recipe.global;

    // A macro rather than a closure: a closure capturing both vectors mutably would stop
    // the two later blocks that push into them directly from compiling, and splitting the
    // function to avoid that would put the pipeline order in two places.
    macro_rules! push {
        ($stage:expr, $wanted:expr, $reason:expr, $detail:expr $(,)?) => {
            if $wanted {
                stages.push($stage);
            } else {
                notes.push(RenderNote {
                    stage: $stage.as_str().to_string(),
                    reason: $reason,
                    detail: $detail,
                });
            }
        };
    }

    // --- the input half -----------------------------------------------------------------
    //
    // Pushed in ORDER: recovery, then white balance, then the matrix. Not in the order the
    // two input kinds happen to make convenient - a plan is a *filter* of ORDER and never a
    // re-sort, and `a_plan_never_reorders_the_stages_it_selects` is the test that says so.
    let native = input == InputKind::CameraNative;
    let upstream = if native {
        SkipReason::NotRequested
    } else {
        SkipReason::UpstreamAlready
    };
    push!(
        Stage::HighlightRecovery,
        native && g.highlights < 0,
        upstream,
        None,
    );
    push!(
        Stage::WhiteBalance,
        g.temperature != 5500 || g.tint != 0,
        SkipReason::NotRequested,
        None,
    );
    push!(Stage::CameraMatrix, native, upstream, None);

    // --- lens ---------------------------------------------------------------------------
    push!(
        Stage::LensVignette,
        recipe.lens.vignette != 0,
        SkipReason::NotRequested,
        None,
    );
    let lens_profile = recipe.lens.profile.clone();
    push!(
        Stage::LensDistortion,
        recipe.lens.distortion && caps.geometry_models,
        if recipe.lens.distortion {
            SkipReason::LensProfileAbsent
        } else {
            SkipReason::NotRequested
        },
        lens_profile.clone(),
    );
    push!(
        Stage::LensCa,
        recipe.lens.ca && caps.geometry_models,
        if recipe.lens.ca {
            SkipReason::LensProfileAbsent
        } else {
            SkipReason::NotRequested
        },
        lens_profile,
    );

    // --- the creative half --------------------------------------------------------------
    push!(
        Stage::NoiseReduction,
        g.noise.luminance > 0 || g.noise.colour > 0,
        SkipReason::NotRequested,
        None,
    );
    push!(
        Stage::Exposure,
        g.exposure != 0.0,
        SkipReason::NotRequested,
        None,
    );
    push!(
        Stage::Tone,
        g.highlights != 0 || g.shadows != 0 || g.whites != 0 || g.blacks != 0,
        SkipReason::NotRequested,
        None,
    );
    push!(
        Stage::Contrast,
        g.contrast != 0,
        SkipReason::NotRequested,
        None
    );
    push!(
        Stage::Curve,
        !g.curve.is_identity(),
        SkipReason::NotRequested,
        None,
    );
    push!(
        Stage::Hsl,
        g.hsl.values().any(|s| !s.is_neutral()),
        SkipReason::NotRequested,
        None,
    );

    let skip_heavy = purpose.may_skip_heavy_stages();
    push!(
        Stage::Clarity,
        g.clarity != 0,
        SkipReason::NotRequested,
        None,
    );
    push!(
        Stage::Texture,
        g.texture != 0,
        SkipReason::NotRequested,
        None
    );
    push!(
        Stage::Dehaze,
        g.dehaze != 0 && !skip_heavy,
        if g.dehaze != 0 {
            SkipReason::InteractivePath
        } else {
            SkipReason::NotRequested
        },
        None,
    );
    push!(
        Stage::Vibrance,
        g.vibrance != 0 || g.saturation != 0,
        SkipReason::NotRequested,
        None,
    );
    push!(
        Stage::Monochrome,
        recipe.bw.is_some(),
        SkipReason::NotRequested,
        None,
    );

    // --- local, retouch, restoration ----------------------------------------------------
    let geometric_masks = recipe
        .masks
        .iter()
        .filter(|m| {
            matches!(
                m.kind,
                aura_recipe::MaskKind::Linear | aura_recipe::MaskKind::Radial
            )
        })
        .count();
    let generated_masks: Vec<String> = recipe
        .masks
        .iter()
        .filter(|m| {
            !matches!(
                m.kind,
                aura_recipe::MaskKind::Linear | aura_recipe::MaskKind::Radial
            )
        })
        .map(|m| m.id.clone())
        .collect();

    if geometric_masks > 0 {
        stages.push(Stage::Masks);
    }
    if !generated_masks.is_empty() && !caps.mask_generators {
        notes.push(RenderNote {
            stage: Stage::Masks.as_str().to_string(),
            reason: SkipReason::MaskGeneratorAbsent,
            detail: Some(generated_masks.join(",")),
        });
    } else if geometric_masks == 0 && generated_masks.is_empty() {
        notes.push(RenderNote {
            stage: Stage::Masks.as_str().to_string(),
            reason: SkipReason::NotRequested,
            detail: None,
        });
    }

    push!(
        Stage::Retouch,
        !recipe.retouch.is_empty() && caps.retouch_operators && !skip_heavy,
        if recipe.retouch.is_empty() {
            SkipReason::NotRequested
        } else if !caps.retouch_operators {
            SkipReason::OperatorAbsent
        } else {
            SkipReason::InteractivePath
        },
        recipe.retouch.first().map(|op| op.op.clone()),
    );

    // PHASE-22, ADR-0045 section 2. `restoration.denoise` deliberately does **not** appear here.
    // Denoising is a sensor-domain operation and runs at `Stage::NoiseReduction`, index 6, thirteen
    // stages earlier - which is what satisfies section 2.1's "denoise before local retouch and
    // sharpening" without moving anything in `ORDER`. The field carries the *tier name* for audit
    // and the panel; the numbers that make it happen are `global.noise.*`. A tier alone enabling
    // this stage would report a stage that ran and changed nothing.
    let wants_restoration = recipe.restoration.face_recovery > 0 || recipe.restoration.deblur > 0;
    push!(
        Stage::Restoration,
        wants_restoration && caps.restoration && !skip_heavy,
        if !wants_restoration {
            SkipReason::NotRequested
        } else if !caps.restoration {
            SkipReason::RestorationAbsent
        } else {
            SkipReason::InteractivePath
        },
        None,
    );

    push!(
        Stage::Sharpen,
        g.sharpen.amount > 0,
        SkipReason::NotRequested,
        None,
    );

    // --- geometry and output ------------------------------------------------------------
    let wants_geometry = !recipe.geometry.is_identity();
    if recipe.geometry.perspective.is_some() && !caps.geometry_models {
        notes.push(RenderNote {
            stage: Stage::Geometry.as_str().to_string(),
            reason: SkipReason::GeometryAbsent,
            detail: Some("perspective".to_string()),
        });
    }
    push!(
        Stage::Geometry,
        wants_geometry,
        SkipReason::NotRequested,
        None
    );
    stages.push(Stage::OutputTransform);

    notes.sort_by(|a, b| a.stage.cmp(&b.stage).then(a.reason.cmp(&b.reason)));
    Plan { stages, notes }
}

/// The first stage a changed parameter affects, or `None` when nothing reads it.
///
/// Section 6.3's invalidation rule. A cache holding the working buffer after each stage can
/// resume from the index this returns; `None` means the change is inert for rendering -
/// which is true of provenance, of the image identity, and of nothing else.
#[must_use]
pub fn earliest_affected(paths: &[String]) -> Option<Stage> {
    paths
        .iter()
        .filter_map(|p| stage_for(p))
        .min_by_key(|s| s.index())
}

/// Which stage first reads a dotted recipe path.
#[must_use]
#[allow(clippy::match_same_arms)]
pub fn stage_for(path: &str) -> Option<Stage> {
    match path {
        "global.temperature" | "global.tint" => Some(Stage::WhiteBalance),
        "global.highlights" => Some(Stage::HighlightRecovery),
        "lens.vignette" => Some(Stage::LensVignette),
        "lens.distortion" => Some(Stage::LensDistortion),
        "lens.ca" => Some(Stage::LensCa),
        "lens.profile" => Some(Stage::LensDistortion),
        "global.noise.luminance"
        | "global.noise.colour"
        | "global.noise.detail"
        | "global.noise.model" => Some(Stage::NoiseReduction),
        "global.exposure" => Some(Stage::Exposure),
        "global.shadows" | "global.whites" | "global.blacks" => Some(Stage::Tone),
        "global.contrast" => Some(Stage::Contrast),
        "global.curve.points" => Some(Stage::Curve),
        "global.clarity" => Some(Stage::Clarity),
        "global.texture" => Some(Stage::Texture),
        "global.dehaze" => Some(Stage::Dehaze),
        "global.vibrance" | "global.saturation" => Some(Stage::Vibrance),
        "masks" => Some(Stage::Masks),
        "retouch" => Some(Stage::Retouch),
        "global.sharpen.amount"
        | "global.sharpen.radius"
        | "global.sharpen.detail"
        | "global.sharpen.masking" => Some(Stage::Sharpen),
        "geometry.rotate" | "geometry.crop" => Some(Stage::Geometry),
        "image.camera" | "image.profile" => Some(Stage::CameraMatrix),
        _ => {
            if path.starts_with("global.hsl") {
                Some(Stage::Hsl)
            } else if path.starts_with("bw") {
                Some(Stage::Monochrome)
            } else if path == "restoration.denoise" {
                // PHASE-22, ADR-0045 section 2. The tier decides `global.noise.*`, which
                // `Stage::NoiseReduction` reads at index 6, so a cache told the change is valid
                // from index 19 would serve the buffer it had already denoised under the previous
                // tier. Routing it to the earlier stage is conservative and correct; the previous
                // routing was a latent invalidation bug that nothing had hit because nothing wrote
                // the field.
                Some(Stage::NoiseReduction)
            } else if path.starts_with("restoration") {
                Some(Stage::Restoration)
            } else if path.starts_with("geometry.perspective") {
                Some(Stage::Geometry)
            } else {
                // `schema`, `engine`, `image.content_hash` and everything under
                // `provenance` are inert for rendering. Every one of them is deliberate;
                // see the exhaustiveness test.
                None
            }
        }
    }
}

/// The render hash: BLAKE3 over the four things that decide the pixels.
///
/// ADR-0029 section 5, in this order: the RAW's content hash, the recipe's canonical JSON,
/// the engine string, and the output spec's canonical form. The profile table version is
/// folded into the engine string by [`engine_string`], because a matrix change produces
/// different pixels from the same recipe.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the recipe cannot be canonicalised.
pub fn render_hash(
    content_hash: &str,
    recipe: &Recipe,
    output: &OutputSpec,
) -> aura_core::AuraResult<String> {
    let canonical = aura_recipe::canonical(recipe)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(content_hash.as_bytes());
    hasher.update(b"\x00");
    hasher.update(canonical.as_bytes());
    hasher.update(b"\x00");
    hasher.update(engine_string().as_bytes());
    hasher.update(b"\x00");
    hasher.update(output.canonical().as_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

/// The engine identity that goes into every render hash.
///
/// `aura-render/1.0.0+profiles.1`. The profile version is part of the engine rather than a
/// fifth input, because from a caller's point of view a changed colour matrix *is* a
/// different renderer.
#[must_use]
pub fn engine_string() -> String {
    format!("{}+profiles.{}", ENGINE, crate::profiles::PROFILES_VER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::fixtures;

    #[test]
    fn the_order_array_holds_every_stage_exactly_once() {
        let mut seen = ORDER.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), ORDER.len(), "a stage appears twice in ORDER");
    }

    #[test]
    fn highlight_recovery_runs_before_white_balance() {
        assert!(
            Stage::HighlightRecovery.index() < Stage::WhiteBalance.index(),
            "a clipped channel reconstructed after a white-balance multiplier is a magenta veil"
        );
    }

    #[test]
    fn white_balance_runs_before_the_camera_matrix() {
        assert!(Stage::WhiteBalance.index() < Stage::CameraMatrix.index());
    }

    #[test]
    fn the_output_transform_is_last() {
        assert_eq!(ORDER.last(), Some(&Stage::OutputTransform));
    }

    #[test]
    fn a_neutral_recipe_runs_only_the_output_transform() {
        let recipe = fixtures::neutral(fixtures::FIXTURE_HASH, "Bench-01");
        let plan = plan(
            &recipe,
            RenderPurpose::Export,
            InputKind::Working,
            Capabilities::default(),
        );
        assert_eq!(plan.stages, vec![Stage::OutputTransform]);
        assert_eq!(plan.halo(), 0);
    }

    #[test]
    fn the_reference_recipe_runs_the_stages_it_asks_for() {
        let recipe = fixtures::reference();
        let plan = plan(
            &recipe,
            RenderPurpose::Export,
            InputKind::Working,
            Capabilities::default(),
        );
        for stage in [
            Stage::WhiteBalance,
            Stage::Exposure,
            Stage::Tone,
            Stage::Contrast,
            Stage::Curve,
            Stage::Hsl,
            Stage::Clarity,
            Stage::Texture,
            Stage::Vibrance,
            Stage::Sharpen,
            Stage::Geometry,
            Stage::OutputTransform,
        ] {
            assert!(plan.stages.contains(&stage), "{stage:?} should run");
        }
    }

    #[test]
    fn an_absent_mask_generator_is_a_note_rather_than_a_silence() {
        let plan = plan(
            &fixtures::reference(),
            RenderPurpose::Export,
            InputKind::Working,
            Capabilities::default(),
        );
        let note = plan
            .notes
            .iter()
            .find(|n| n.reason == SkipReason::MaskGeneratorAbsent)
            .expect("the face mask cannot be generated in this build");
        assert!(note.detail.as_deref().is_some_and(|d| d.contains("m1")));
        assert!(plan.has_caveats());
    }

    #[test]
    fn the_interactive_path_skips_heavy_stages_and_the_export_path_never_does() {
        let mut recipe = fixtures::reference();
        recipe.global.dehaze = 40;
        let interactive = plan(
            &recipe,
            RenderPurpose::Interactive,
            InputKind::Working,
            Capabilities::default(),
        );
        assert!(!interactive.stages.contains(&Stage::Dehaze));
        assert!(interactive
            .notes
            .iter()
            .any(|n| n.reason == SkipReason::InteractivePath));

        for purpose in [RenderPurpose::Export, RenderPurpose::Analysis] {
            let full = plan(
                &recipe,
                purpose,
                InputKind::Working,
                Capabilities::default(),
            );
            assert!(
                full.stages.contains(&Stage::Dehaze),
                "{purpose:?} must not skip"
            );
        }
    }

    #[test]
    fn working_space_input_skips_the_camera_matrix_without_calling_it_a_caveat() {
        let plan = plan(
            &fixtures::reference(),
            RenderPurpose::Export,
            InputKind::Working,
            Capabilities::default(),
        );
        let note = plan
            .notes
            .iter()
            .find(|n| n.stage == "camera_matrix")
            .expect("noted");
        assert_eq!(note.reason, SkipReason::UpstreamAlready);
        assert!(!note.reason.is_a_caveat());
    }

    #[test]
    fn the_halo_is_the_sum_of_the_enabled_stages() {
        let mut recipe = fixtures::neutral(fixtures::FIXTURE_HASH, "Bench-01");
        recipe.global.clarity = 30;
        recipe.global.sharpen.amount = 40;
        let plan = plan(
            &recipe,
            RenderPurpose::Export,
            InputKind::Working,
            Capabilities::default(),
        );
        assert_eq!(
            plan.halo(),
            Stage::Clarity.halo() + Stage::Sharpen.halo(),
            "stages compose, so their reaches add"
        );
    }

    #[test]
    fn moving_a_late_slider_invalidates_only_from_that_stage() {
        let vibrance = earliest_affected(&["global.saturation".to_string()]).expect("stage");
        assert_eq!(vibrance, Stage::Vibrance);
        assert!(vibrance.index() > Stage::Exposure.index());

        let both = earliest_affected(&[
            "global.saturation".to_string(),
            "global.exposure".to_string(),
        ])
        .expect("stage");
        assert_eq!(both, Stage::Exposure, "the earliest wins");
    }

    #[test]
    fn provenance_is_inert_for_rendering() {
        assert!(earliest_affected(&["provenance.confidence".to_string()]).is_none());
        assert!(earliest_affected(&["provenance.user_edited_fields".to_string()]).is_none());
        assert!(earliest_affected(&["image.content_hash".to_string()]).is_none());
    }

    #[test]
    fn every_recipe_path_maps_to_a_stage_or_is_deliberately_inert() {
        // The inert list is exhaustive and is the whole point of the test: a new parameter
        // that falls through to `None` without being named here is a slider that appears to
        // do nothing until somebody clears the cache.
        const INERT: [&str; 5] = [
            "schema",
            "engine",
            "image.content_hash",
            "image.camera",
            "image.profile",
        ];
        for path in aura_recipe::schema::paths(&fixtures::reference()) {
            if path.starts_with("provenance") {
                assert!(stage_for(&path).is_none(), "{path} should be inert");
                continue;
            }
            if INERT.contains(&path.as_str()) {
                continue;
            }
            assert!(
                stage_for(&path).is_some(),
                "{path} is read by no stage; add it to `stage_for` or to INERT"
            );
        }
    }

    #[test]
    fn the_render_hash_changes_with_each_of_its_four_inputs() {
        let recipe = fixtures::reference();
        let spec = OutputSpec::default();
        let base = render_hash(fixtures::FIXTURE_HASH, &recipe, &spec).expect("hash");

        let other_image = render_hash(&"b".repeat(64), &recipe, &spec).expect("hash");
        assert_ne!(base, other_image);

        let mut edited = recipe.clone();
        edited.global.exposure += 0.5;
        assert_ne!(
            base,
            render_hash(fixtures::FIXTURE_HASH, &edited, &spec).expect("hash")
        );

        let sixteen = OutputSpec {
            bit_depth: 16,
            ..OutputSpec::default()
        };
        assert_ne!(
            base,
            render_hash(fixtures::FIXTURE_HASH, &recipe, &sixteen).expect("hash")
        );
    }

    #[test]
    fn the_engine_string_carries_the_profile_version() {
        assert!(engine_string().ends_with(&format!("+profiles.{}", crate::profiles::PROFILES_VER)));
    }

    #[test]
    fn the_same_inputs_hash_the_same_twice() {
        let recipe = fixtures::reference();
        let spec = OutputSpec::default();
        assert_eq!(
            render_hash(fixtures::FIXTURE_HASH, &recipe, &spec).expect("a"),
            render_hash(fixtures::FIXTURE_HASH, &recipe, &spec).expect("b")
        );
    }
}
