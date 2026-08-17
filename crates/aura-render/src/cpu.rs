//! The CPU reference pipeline. **The ground truth every other backend is measured against.**
//!
//! Section 6.2: *"Every stage has a CPU reference implementation; CI compares GPU vs CPU
//! within a per-stage tolerance."* This is that implementation, and the direction of the
//! comparison is the point - when a GPU stage and this one disagree, this one is right until
//! somebody proves otherwise in an ADR.
//!
//! # What a render is
//!
//! A [`Frame`] in, a `RenderedImage` out, with a [`graph::Plan`] deciding what happens in
//! between. Nothing here opens a file: the pixels arrive through [`FrameSource`], which the
//! caller supplies. Invariant 1 is a property of the module's dependencies - there is no
//! path type in scope.
//!
//! # `f32` throughout
//!
//! ADR-0029 section 3. The working buffer is interleaved `f32` RGB in linear Rec.2020, and
//! it stays that way from the first stage to the output transform. A `f16` working buffer
//! would round twice per stage and eat the parity tolerance the gate is written against.

use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::errors::render::tiled_fallback;
use aura_core::{AuraError, AuraResult, PhotoId};
use aura_recipe::{MaskKind, Recipe};
use rayon::prelude::*;

use crate::colour::{apply_f32, recover_highlights, white_balance};
use crate::contract::render::{
    Backend, RenderCaps, RenderLevel, RenderNote, RenderPurpose, RenderRequest, RenderService,
    RenderedImage, SkipReason,
};
use crate::graph::{self, Capabilities, InputKind, Plan, Stage};
use crate::spatial;
use crate::tonemap::{self, CurveLut, Tone};

/// The working buffer's bytes per pixel: three `f32`.
pub const BYTES_PER_PIXEL: u64 = 12;

/// The default working-buffer ceiling before a render is streamed in tiles.
///
/// 1 GB. A 45 MP frame is 537 MB of interleaved `f32` and fits with room for the two
/// single-channel scratch planes the spatial stages need; a 100 MP export is 1.22 GB, does
/// not fit, and is streamed - which is section 6.3's requirement and `AURA-RENDER-8009`.
pub const DEFAULT_WORKING_BYTES: u64 = 1024 * 1024 * 1024;

/// Pixels in, and where they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Interleaved RGB, `width * height * 3` long.
    pub rgb: Vec<f32>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Camera native, or already in the working space.
    pub kind: InputKind,
    /// The per-channel clip point, for highlight recovery. Meaningless when `kind` is
    /// [`InputKind::Working`], and the graph does not run that stage then.
    pub clip: [f32; 3],
    /// The EXIF camera model, for the profile lookup.
    pub camera: String,
    /// Where this buffer's top-left pixel sits inside the whole frame.
    ///
    /// `(0, 0)` for a whole-frame render and the tile's own origin for a streamed one. Two
    /// stages are **position-dependent** - vignette correction measures a radius from the
    /// frame's centre, and a mask is drawn in frame coordinates - so a tile that did not know
    /// where it was would apply both as though it were the whole photograph. That is a seam
    /// on every tile boundary, and it is the defect this field exists to make impossible.
    pub origin: (u32, u32),
    /// The whole frame's size, which may be larger than this buffer's.
    pub full: (u32, u32),
}

impl Frame {
    /// A working-space frame, which is what phase 02's proxy delivers.
    #[must_use]
    pub fn working(rgb: Vec<f32>, width: u32, height: u32, camera: &str) -> Self {
        Self {
            rgb,
            width,
            height,
            kind: InputKind::Working,
            clip: [1.0; 3],
            camera: camera.to_string(),
            origin: (0, 0),
            full: (width, height),
        }
    }

    /// Pixels in the frame.
    #[must_use]
    pub const fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// Where a renderer gets its pixels.
///
/// A port, deliberately **not** frozen, exactly as `aura_infer::Backend` is: the production
/// implementation reads phase 02's preview service, the golden suite reads authored
/// mosaics, and neither is a contract change. ADR-0029 section 4 makes the same argument
/// about `GpuBackend`.
pub trait FrameSource: Send + Sync + std::fmt::Debug {
    /// The pixels for one photograph at one level.
    ///
    /// # Errors
    ///
    /// Whatever the underlying decode raises. `aura-render` adds nothing to it.
    fn frame(&self, image: &PhotoId, level: RenderLevel) -> AuraResult<Frame>;
}

/// The reference engine.
#[derive(Debug)]
pub struct CpuEngine {
    source: Arc<dyn FrameSource>,
    clock: Arc<dyn Clock>,
    caps: Capabilities,
    working_bytes: u64,
}

impl CpuEngine {
    /// An engine over a frame source.
    #[must_use]
    pub fn new(source: Arc<dyn FrameSource>, clock: Arc<dyn Clock>) -> Self {
        Self {
            source,
            clock,
            caps: Capabilities::default(),
            working_bytes: DEFAULT_WORKING_BYTES,
        }
    }

    /// Override the working-buffer ceiling. Used by the tiling tests and by the phase 03
    /// hardware plan.
    #[must_use]
    pub const fn with_working_bytes(mut self, bytes: u64) -> Self {
        self.working_bytes = bytes;
        self
    }

    /// Declare which later phases' capabilities are present.
    #[must_use]
    pub const fn with_capabilities(mut self, caps: Capabilities) -> Self {
        self.caps = caps;
        self
    }

    /// Render a frame that is already in hand, without touching the source.
    ///
    /// The parity harness, the tiler and the golden suite all go through here, which is what
    /// makes them comparisons of the *pipeline* rather than of the decode in front of it.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002` when the recipe cannot be canonicalised for its hash.
    pub fn render_frame(
        &self,
        frame: &Frame,
        recipe: &Recipe,
        level: RenderLevel,
        purpose: RenderPurpose,
        output: &crate::contract::render::OutputSpec,
    ) -> AuraResult<RenderedImage> {
        let started = self.clock.monotonic_us();
        let clamped = recipe.clamped();
        let plan = graph::plan(&clamped, purpose, frame.kind, self.caps);

        let (rgb, width, height, mut notes) =
            self.working_buffer(frame, &clamped, &plan, level, None);

        let quantised = crate::output::transform(
            &rgb,
            width,
            height,
            output.colour_space,
            output.bit_depth,
            (0, 0),
        );

        notes.extend(plan.notes.iter().cloned());
        notes.sort_by(|a, b| a.stage.cmp(&b.stage).then(a.reason.cmp(&b.reason)));
        notes.dedup();

        let hash = graph::render_hash(&clamped.image.content_hash, &clamped, output)?;
        let elapsed = self.clock.monotonic_us().saturating_sub(started);

        Ok(RenderedImage {
            width,
            height,
            data: quantised,
            colour_space: output.colour_space,
            render_hash: hash,
            backend: Backend::Cpu.as_str().to_string(),
            notes,
            stages_run: plan.slugs(),
            ms: u32::try_from(elapsed / 1_000).unwrap_or(u32::MAX),
            cache_hit: false,
        })
    }

    /// Run the plan over a frame, returning the working buffer in linear working space.
    ///
    /// `stats` is the frame-wide measurement three stages need. `None` measures this buffer,
    /// which is right for a whole-frame render and wrong for a tile - so the tiler measures
    /// once and passes the same value into every tile. See `spatial::Stats`.
    ///
    /// Public because the tiler, the parity harness and the golden suite all need the
    /// working buffer without an output transform on the end of it.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn working_buffer(
        &self,
        frame: &Frame,
        recipe: &Recipe,
        plan: &Plan,
        level: RenderLevel,
        stats: Option<spatial::Stats>,
    ) -> (Vec<f32>, u32, u32, Vec<RenderNote>) {
        let mut notes = Vec::new();
        let g = &recipe.global;

        // Resample first when the caller asked for a smaller rung. Everything downstream is
        // then cheaper, and the two spatial radii are expressed in output pixels - which is
        // what makes a clarity setting look the same on a proxy and on an export.
        let (mut rgb, mut width, mut height) = fit(frame, level);
        // Where this buffer sits in the whole photograph, after any resample. The two
        // position-dependent stages read it; every other stage is either point-wise or
        // protected by the tiler's halo.
        let position = spatial::Position::of(frame, width, height);
        let stats =
            stats.unwrap_or_else(|| spatial::measure(&rgb, width as usize, height as usize));

        let profile = crate::profiles::resolve(&frame.camera);
        if let Some(warning) = &profile.warning {
            let _ = warning;
            notes.push(RenderNote {
                stage: Stage::CameraMatrix.as_str().to_string(),
                reason: SkipReason::CameraProfileAbsent,
                detail: Some(frame.camera.clone()),
            });
        }

        let multipliers = white_balance(g.temperature as f32, f32::from(g.tint));
        let curve = CurveLut::build(&g.curve);
        let tone = Tone {
            highlights: f32::from(g.highlights) / 100.0,
            shadows: f32::from(g.shadows) / 100.0,
            whites: f32::from(g.whites) / 100.0,
            blacks: f32::from(g.blacks) / 100.0,
        };
        let recovery = (-f32::from(g.highlights) / 100.0).max(0.0);

        // ---- the point-wise input half, fused into one pass over the pixels -------------
        let wants_recovery = plan.stages.contains(&Stage::HighlightRecovery);
        let wants_wb = plan.stages.contains(&Stage::WhiteBalance);
        let wants_matrix = plan.stages.contains(&Stage::CameraMatrix);
        if wants_recovery || wants_wb || wants_matrix {
            let clip = frame.clip;
            let matrix = profile.camera_to_working;
            rgb.par_chunks_mut(3).for_each(|pixel| {
                let mut value = [
                    pixel.first().copied().unwrap_or(0.0),
                    pixel.get(1).copied().unwrap_or(0.0),
                    pixel.get(2).copied().unwrap_or(0.0),
                ];
                if wants_recovery {
                    value = recover_highlights(value, clip, recovery);
                }
                if wants_wb {
                    for (slot, m) in value.iter_mut().zip(multipliers.iter()) {
                        *slot *= m;
                    }
                }
                if wants_matrix {
                    value = apply_f32(matrix, value);
                }
                for (slot, out) in pixel.iter_mut().zip(value.iter()) {
                    *slot = *out;
                }
            });
        }

        // ---- lens ----------------------------------------------------------------------
        if plan.stages.contains(&Stage::LensVignette) {
            spatial::correct_vignette(
                &mut rgb,
                width as usize,
                height as usize,
                f32::from(recipe.lens.vignette) / 100.0,
                position,
            );
        }

        // ---- noise ---------------------------------------------------------------------
        if plan.stages.contains(&Stage::NoiseReduction) {
            spatial::denoise(
                &mut rgb,
                width as usize,
                height as usize,
                f32::from(g.noise.luminance) / 100.0,
                f32::from(g.noise.colour) / 100.0,
                f32::from(g.noise.detail) / 100.0,
                &stats,
            );
        }

        // ---- the point-wise creative half, fused into one pass -------------------------
        let wants_exposure = plan.stages.contains(&Stage::Exposure);
        let wants_tone = plan.stages.contains(&Stage::Tone);
        let wants_contrast = plan.stages.contains(&Stage::Contrast);
        let wants_curve = plan.stages.contains(&Stage::Curve);
        let wants_hsl = plan.stages.contains(&Stage::Hsl);
        let wants_vibrance = plan.stages.contains(&Stage::Vibrance);
        let wants_bw = plan.stages.contains(&Stage::Monochrome);

        if wants_exposure
            || wants_tone
            || wants_contrast
            || wants_curve
            || wants_hsl
            || wants_vibrance
            || wants_bw
        {
            let vibrance = f32::from(g.vibrance) / 100.0;
            let saturation = f32::from(g.saturation) / 100.0;
            let contrast_amount = f32::from(g.contrast) / 100.0;
            rgb.par_chunks_mut(3).for_each(|pixel| {
                let mut value = [
                    pixel.first().copied().unwrap_or(0.0),
                    pixel.get(1).copied().unwrap_or(0.0),
                    pixel.get(2).copied().unwrap_or(0.0),
                ];
                if wants_exposure {
                    value = tonemap::exposure(value, g.exposure);
                }
                if wants_tone {
                    value = tonemap::tone(value, tone);
                }
                if wants_contrast {
                    value = tonemap::contrast(value, contrast_amount);
                }
                if wants_curve {
                    value = tonemap::apply_curve(value, &curve);
                }
                if wants_hsl {
                    value = tonemap::hsl(value, &g.hsl);
                }
                if wants_vibrance {
                    value = tonemap::vibrance_saturation(value, vibrance, saturation);
                }
                if wants_bw {
                    if let Some(bw) = &recipe.bw {
                        value = tonemap::monochrome(value, bw);
                    }
                }
                for (slot, out) in pixel.iter_mut().zip(value.iter()) {
                    *slot = *out;
                }
            });
        }

        // ---- local contrast ------------------------------------------------------------
        if plan.stages.contains(&Stage::Clarity) {
            spatial::local_contrast(
                &mut rgb,
                width as usize,
                height as usize,
                16,
                f32::from(g.clarity) / 100.0,
            );
        }
        if plan.stages.contains(&Stage::Texture) {
            spatial::local_contrast(
                &mut rgb,
                width as usize,
                height as usize,
                4,
                f32::from(g.texture) / 100.0,
            );
        }
        if plan.stages.contains(&Stage::Dehaze) {
            spatial::dehaze(
                &mut rgb,
                width as usize,
                height as usize,
                f32::from(g.dehaze) / 100.0,
                &stats,
            );
        }

        // ---- masks ---------------------------------------------------------------------
        if plan.stages.contains(&Stage::Masks) {
            apply_masks(&mut rgb, width, height, recipe, position);
        }

        // ---- sharpening ----------------------------------------------------------------
        if plan.stages.contains(&Stage::Sharpen) {
            spatial::unsharp(
                &mut rgb,
                width as usize,
                height as usize,
                g.sharpen.radius,
                f32::from(g.sharpen.amount) / 100.0,
                f32::from(g.sharpen.detail) / 100.0,
                f32::from(g.sharpen.masking) / 100.0,
                &stats,
            );
        }

        // ---- geometry ------------------------------------------------------------------
        if plan.stages.contains(&Stage::Geometry) {
            let (out, w, h) = spatial::crop_rotate(
                &rgb,
                width as usize,
                height as usize,
                recipe.geometry.crop,
                recipe.geometry.rotate,
            );
            rgb = out;
            width = w as u32;
            height = h as u32;
        }

        (rgb, width, height, notes)
    }

    /// The warning a render that had to be streamed raises.
    #[must_use]
    fn budget_warning(&self, pixels: u64) -> Option<AuraError> {
        let needed = pixels.saturating_mul(BYTES_PER_PIXEL);
        if needed > self.working_bytes {
            Some(tiled_fallback(pixels, self.working_bytes))
        } else {
            None
        }
    }
}

/// Resample a frame to the requested rung.
fn fit(frame: &Frame, level: RenderLevel) -> (Vec<f32>, u32, u32) {
    let Some(edge) = level.long_edge() else {
        return (frame.rgb.clone(), frame.width, frame.height);
    };
    let long = frame.width.max(frame.height);
    if long <= edge || long == 0 {
        return (frame.rgb.clone(), frame.width, frame.height);
    }
    let scale = f64::from(edge) / f64::from(long);
    let out_w = ((f64::from(frame.width) * scale).round() as u32).max(1);
    let out_h = ((f64::from(frame.height) * scale).round() as u32).max(1);
    let resampled = spatial::resample(
        &frame.rgb,
        frame.width as usize,
        frame.height as usize,
        out_w as usize,
        out_h as usize,
    );
    (resampled, out_w, out_h)
}

/// Apply the geometric masks a recipe carries.
///
/// **Only `Linear` and `Radial`.** Every other kind needs a generator phase 18 ships, and
/// the graph has already recorded a note for each one; this function would silently do
/// nothing for them, which is exactly the failure the note exists to prevent.
///
/// The geometry of the two that do run is the documented default - a linear mask runs top to
/// bottom, a radial mask is a centred ellipse over 70 % of the frame - because schema v1
/// carries a mask's `feather` and its parameters but not its placement. Phase 18 adds the
/// placement, and it is an added field rather than a changed one.
fn apply_masks(
    rgb: &mut [f32],
    width: u32,
    _height: u32,
    recipe: &Recipe,
    position: spatial::Position,
) {
    let geometric: Vec<_> = recipe
        .masks
        .iter()
        .filter(|m| matches!(m.kind, MaskKind::Linear | MaskKind::Radial))
        .collect();
    if geometric.is_empty() {
        return;
    }
    let w = position.full_width.max(1) as f32;
    let h = position.full_height.max(1) as f32;

    rgb.par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            // Frame coordinates, not tile coordinates. A mask is drawn on the photograph.
            let x = ((index % width.max(1) as usize) as f32 + position.x as f32) / w;
            let y = ((index / width.max(1) as usize) as f32 + position.y as f32) / h;

            let mut value = [
                pixel.first().copied().unwrap_or(0.0),
                pixel.get(1).copied().unwrap_or(0.0),
                pixel.get(2).copied().unwrap_or(0.0),
            ];
            for mask in &geometric {
                let raw = if mask.kind == MaskKind::Linear {
                    y
                } else {
                    let dx = (x - 0.5) / 0.35;
                    let dy = (y - 0.5) / 0.35;
                    1.0 - dx.hypot(dy).min(1.0)
                };
                let feather = mask.feather.clamp(0.01, 1.0);
                let weight = smoothstep(raw, 0.5 - feather / 2.0, 0.5 + feather / 2.0);
                if weight <= 0.0 {
                    continue;
                }
                value = apply_mask_params(value, &mask.params, weight);
            }
            for (slot, out) in pixel.iter_mut().zip(value.iter()) {
                *slot = *out;
            }
        });
}

fn smoothstep(x: f32, edge0: f32, edge1: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sparse parameter block, scaled by a mask weight.
///
/// Each parameter is applied at `weight` of its full travel rather than the whole stage
/// being blended. Blending the whole stage would make two overlapping masks interact
/// non-linearly with each other's tone curves; scaling the parameters makes them add, which
/// is what a photographer expects and what makes an inverted pair cover the frame exactly.
fn apply_mask_params(rgb: [f32; 3], params: &aura_recipe::MaskParams, weight: f32) -> [f32; 3] {
    let mut value = rgb;
    if let Some(exposure) = params.exposure {
        value = tonemap::exposure(value, exposure * weight);
    }
    if let Some(contrast) = params.contrast {
        value = tonemap::contrast(value, f32::from(contrast) / 100.0 * weight);
    }
    let tone = Tone {
        highlights: f32::from(params.highlights.unwrap_or(0)) / 100.0 * weight,
        shadows: f32::from(params.shadows.unwrap_or(0)) / 100.0 * weight,
        whites: f32::from(params.whites.unwrap_or(0)) / 100.0 * weight,
        blacks: f32::from(params.blacks.unwrap_or(0)) / 100.0 * weight,
    };
    if !tone.is_identity() {
        value = tonemap::tone(value, tone);
    }
    if let Some(saturation) = params.saturation {
        value = tonemap::saturate(value, 1.0 + f32::from(saturation) / 100.0 * weight);
    }
    if let Some(kelvin) = params.temperature {
        // A mask's temperature is an *offset* from the global setting, so the multipliers
        // are the ratio of the two rather than the absolute value.
        let base = white_balance(crate::colour::REFERENCE_KELVIN, 0.0);
        let shifted = white_balance(crate::colour::REFERENCE_KELVIN + kelvin as f32, 0.0);
        for (slot, (b, s)) in value.iter_mut().zip(base.iter().zip(shifted.iter())) {
            let ratio = if *b > 1e-6 { s / b } else { 1.0 };
            *slot *= 1.0 + (ratio - 1.0) * weight;
        }
    }
    value
}

impl RenderService for CpuEngine {
    fn render(&self, req: RenderRequest) -> AuraResult<RenderedImage> {
        aura_recipe::schema::Validation::check(&req.recipe)?;
        let frame = self.source.frame(&req.image_id, req.level)?;

        let mut image = if let Some(warning) = self.budget_warning(frame.pixels()) {
            let mut rendered = crate::tiles::render_streamed(
                self,
                &frame,
                &req.recipe,
                req.level,
                req.purpose,
                &req.output,
                self.working_bytes,
            )?;
            rendered.notes.push(RenderNote {
                stage: Stage::OutputTransform.as_str().to_string(),
                reason: SkipReason::NotRequested,
                detail: Some(warning.detail.clone()),
            });
            rendered
        } else {
            self.render_frame(&frame, &req.recipe, req.level, req.purpose, &req.output)?
        };

        image.notes.dedup();
        Ok(image)
    }

    fn render_hash(&self, req: &RenderRequest) -> AuraResult<String> {
        let clamped = req.recipe.clamped();
        graph::render_hash(&clamped.image.content_hash, &clamped, &req.output)
    }

    fn capabilities(&self) -> RenderCaps {
        RenderCaps {
            backend: Backend::Cpu,
            // The reference path has no texture limit; the number is the working-buffer
            // ceiling expressed as a square edge, so a caller sizing a request against it
            // gets an honest answer rather than a hardware constant that does not apply.
            max_texture: ((self.working_bytes / BYTES_PER_PIXEL) as f64).sqrt() as u32,
            precision_bits: 24,
            max_working_bytes: self.working_bytes,
            engine: graph::engine_string(),
        }
    }

    fn degradation(&self) -> Option<AuraError> {
        Some(aura_core::errors::render::no_gpu_backend(
            "this build links no wgpu backend; see ADR-0029 section 4",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::render::{OutputColour, OutputSpec, RenderedData};
    use crate::fixtures;
    use aura_core::clock::FixedClock;
    use aura_recipe::fixtures as recipes;

    fn engine(source: Arc<dyn FrameSource>) -> CpuEngine {
        CpuEngine::new(source, FixedClock::at(time::OffsetDateTime::UNIX_EPOCH))
    }

    #[test]
    fn a_neutral_recipe_renders_the_frame_it_was_given() {
        let frame = fixtures::grey_frame(16, 16, 0.18);
        let source = Arc::new(fixtures::StaticSource::new(frame.clone()));
        let engine = engine(source);
        let recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");

        let out = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("render");

        assert_eq!((out.width, out.height), (16, 16));
        assert_eq!(out.stages_run, vec!["output_transform".to_string()]);
        let RenderedData::Eight(bytes) = &out.data else {
            panic!("eight-bit output requested");
        };
        // A neutral grey stays neutral, and stays grey.
        let first = bytes.first().copied().unwrap_or(0);
        for triple in bytes.chunks(3) {
            for channel in triple {
                assert!(
                    channel.abs_diff(first) <= 1,
                    "a neutral render tinted or banded: {channel} vs {first}"
                );
            }
        }
    }

    #[test]
    fn exposure_moves_the_output_the_way_it_should() {
        let frame = fixtures::grey_frame(8, 8, 0.18);
        let engine = engine(Arc::new(fixtures::StaticSource::new(frame.clone())));
        let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");

        let base = mean_byte(&engine, &frame, &recipe);
        recipe.global.exposure = 1.0;
        let brighter = mean_byte(&engine, &frame, &recipe);
        recipe.global.exposure = -1.0;
        let darker = mean_byte(&engine, &frame, &recipe);

        assert!(brighter > base, "{brighter} should exceed {base}");
        assert!(darker < base, "{darker} should be under {base}");
    }

    fn mean_byte(engine: &CpuEngine, frame: &Frame, recipe: &Recipe) -> f64 {
        let out = engine
            .render_frame(
                frame,
                recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("render");
        let RenderedData::Eight(bytes) = &out.data else {
            return 0.0;
        };
        bytes.iter().map(|b| f64::from(*b)).sum::<f64>() / bytes.len().max(1) as f64
    }

    #[test]
    fn the_same_recipe_renders_byte_identically_twice() {
        let frame = fixtures::ramp_frame(64, 48);
        let engine = engine(Arc::new(fixtures::StaticSource::new(frame.clone())));
        let recipe = recipes::reference();

        let a = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("a");
        let b = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("b");
        assert_eq!(a.data, b.data, "invariant 4");
        assert_eq!(a.render_hash, b.render_hash);
    }

    #[test]
    fn a_proxy_render_lands_on_the_requested_long_edge() {
        let frame = fixtures::ramp_frame(4000, 3000);
        let engine = engine(Arc::new(fixtures::StaticSource::new(frame.clone())));
        let recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
        let out = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Proxy2048,
                RenderPurpose::Interactive,
                &OutputSpec::default(),
            )
            .expect("render");
        assert_eq!(out.width, 2048);
        assert_eq!(out.height, 1536);
    }

    #[test]
    fn a_render_never_touches_the_recipe_it_was_given() {
        let frame = fixtures::grey_frame(8, 8, 0.5);
        let engine = engine(Arc::new(fixtures::StaticSource::new(frame.clone())));
        let recipe = recipes::reference();
        let before = aura_recipe::recipe_hash(&recipe).expect("hash");
        let _ = engine.render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        );
        assert_eq!(aura_recipe::recipe_hash(&recipe).expect("hash"), before);
    }

    #[test]
    fn the_engine_reports_the_missing_gpu_backend_once() {
        let engine = engine(Arc::new(fixtures::StaticSource::new(fixtures::grey_frame(
            4, 4, 0.2,
        ))));
        assert_eq!(engine.capabilities().backend, Backend::Cpu);
        assert_eq!(
            engine.degradation().map(|e| e.code.to_string()),
            Some("AURA-RENDER-8001".to_string())
        );
    }

    #[test]
    fn an_invalid_recipe_is_refused_before_any_pixel_moves() {
        let frame = fixtures::grey_frame(4, 4, 0.2);
        let engine = engine(Arc::new(fixtures::StaticSource::new(frame)));
        let mut recipe = recipes::reference();
        recipe.geometry.crop = [0.9, 0.0, 0.1, 1.0];
        let err = engine
            .render(RenderRequest {
                image_id: PhotoId::new(),
                recipe,
                level: RenderLevel::Full,
                output: OutputSpec::default(),
                purpose: RenderPurpose::Export,
            })
            .expect_err("must refuse");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8002");
    }

    #[test]
    fn a_sixteen_bit_export_carries_sixteen_bit_samples() {
        let frame = fixtures::ramp_frame(16, 16);
        let engine = engine(Arc::new(fixtures::StaticSource::new(frame.clone())));
        let out = engine
            .render_frame(
                &frame,
                &recipes::neutral(recipes::FIXTURE_HASH, "Bench-01"),
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec {
                    colour_space: OutputColour::AdobeRgb,
                    bit_depth: 16,
                    icc: None,
                },
            )
            .expect("render");
        assert_eq!(out.data.bit_depth(), 16);
        assert_eq!(out.colour_space, OutputColour::AdobeRgb);
    }

    #[test]
    fn a_mask_the_build_cannot_generate_is_noted_rather_than_ignored() {
        let frame = fixtures::ramp_frame(16, 16);
        let engine = engine(Arc::new(fixtures::StaticSource::new(frame.clone())));
        let out = engine
            .render_frame(
                &frame,
                &recipes::reference(),
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("render");
        assert!(out
            .notes
            .iter()
            .any(|n| n.reason == SkipReason::MaskGeneratorAbsent));
    }
}
