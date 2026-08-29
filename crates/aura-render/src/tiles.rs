//! Tiled execution: how a 100 MP export never needs to fit in memory at once.
//!
//! Section 6.3: *"Full-resolution export streams tiles so a 100 MP file never needs to fit
//! in VRAM."* The same argument holds for system memory on the reference path, and the same
//! code answers both.
//!
//! # The property that has to hold
//!
//! **A tiled render is bit-identical to a whole-frame one.** Not close - identical. A
//! photographer who exports a 24 MP file and a 100 MP file from the same recipe must not get
//! two different looks because one of them crossed a memory threshold.
//!
//! Three things make that true, and each of them is a decision somewhere else in this crate:
//!
//! 1. **Every spatial stage declares its reach**, in `graph::Stage::halo`, as the exact
//!    number of pixels it reads rather than an estimate. The tiler decodes the *sum* of
//!    those around each tile - stages compose, so their reaches add - and throws the halo
//!    away on assembly.
//! 2. **No stage takes a frame-wide reduction inside itself.** Sharpening's normaliser, noise
//!    reduction's edge keeper and dehaze's floor all come from `spatial::Stats`, measured
//!    once over the whole frame and passed into every tile. That refactor is the whole
//!    reason `Stats` exists.
//! 3. **The dither knows where it is.** `output::transform` takes the tile's origin, and the
//!    Bayer matrix is indexed by absolute pixel position, so the same pixel gets the same
//!    offset whichever tile it arrived in.
//!
//! `crates/aura-render/tests/tiling.rs` asserts the property directly, on a frame with
//! clarity, sharpening and noise reduction all enabled, at a tile size small enough to
//! produce a dozen tiles.
//!
//! # What is not streamed
//!
//! A **rotation**. Crop is a restriction of the tile grid and commutes with every stage in
//! front of it; a rotation resamples across tile boundaries and would need a halo that grows
//! with the frame's diagonal. A rotated full-resolution render therefore falls back to the
//! whole-frame path and says so, rather than being approximated. Phase 23 owns geometry and
//! is where a streaming rotation belongs.

use aura_core::AuraResult;
use aura_recipe::Recipe;

use crate::contract::render::{
    OutputSpec, RenderLevel, RenderNote, RenderPurpose, RenderedData, RenderedImage, SkipReason,
};
use crate::cpu::{CpuEngine, Frame, BYTES_PER_PIXEL};
use crate::graph::{self, Stage};
use crate::spatial;

/// The committed edge of one tile, in pixels.
///
/// 512, which is phase 02's decode tile size. Matching it is not a coincidence: a render
/// tile that straddles two decode tiles reads both, and aligning them halves the reads.
pub const TILE_SIZE: u32 = 512;

/// The halo a plan needs, in pixels.
#[must_use]
pub fn halo_for(plan: &graph::Plan) -> u32 {
    plan.halo()
}

/// How many tiles a frame of this size takes at this tile size.
#[must_use]
pub const fn tile_count(width: u32, height: u32, tile: u32) -> u32 {
    let across = width.div_ceil(tile);
    let down = height.div_ceil(tile);
    across * down
}

/// Render a frame one tile at a time, assembling the output raster.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the recipe cannot be canonicalised for its hash.
#[allow(clippy::too_many_lines)]
pub fn render_streamed(
    engine: &CpuEngine,
    frame: &Frame,
    recipe: &Recipe,
    level: RenderLevel,
    purpose: RenderPurpose,
    output: &OutputSpec,
    budget_bytes: u64,
) -> AuraResult<RenderedImage> {
    let clamped = recipe.clamped();
    // The engine's own capabilities, not the defaults. A tile plan built against a different
    // capability set from the one `render_frame` will use is a streamed render that silently
    // omits a stage the whole-frame path applies - which is the same photograph rendered two
    // ways depending on how large it is.
    let plan = graph::plan(&clamped, purpose, frame.kind, engine.stage_capabilities());

    // A rotation is not streamable. Neither is either lens resample: PHASE-23's distortion and
    // chromatic aberration models are written against the *frame's* radius, and the displacement
    // at a corner is a percent or two of the half-diagonal - tens of pixels at export size. No
    // fixed halo covers that, and one that looked as though it did would be a seam at every tile
    // boundary rather than an error anybody could see in a test. Say so and render whole.
    let lens_resample = plan.stages.contains(&Stage::LensDistortion)
        || plan.stages.contains(&Stage::LensCa);
    if clamped.geometry.rotate.abs() > f32::EPSILON
        || clamped.geometry.perspective.is_some()
        || lens_resample
    {
        let mut whole = engine.render_frame(frame, &clamped, level, purpose, output)?;
        whole.notes.push(RenderNote {
            stage: Stage::Geometry.as_str().to_string(),
            reason: SkipReason::NotRequested,
            detail: Some(if lens_resample {
                "a lens-corrected frame is rendered whole rather than streamed".to_string()
            } else {
                "a rotated frame is rendered whole rather than streamed".to_string()
            }),
        });
        return Ok(whole);
    }

    let stats = spatial::measure(&frame.rgb, frame.width as usize, frame.height as usize);

    // The geometry stage must not run inside a tile: the crop *is* the tile grid.
    let mut tile_recipe = clamped.clone();
    tile_recipe.geometry = aura_recipe::Geometry::default();
    let tile_plan = graph::plan(&tile_recipe, purpose, frame.kind, engine.stage_capabilities());
    let halo = tile_plan.halo();

    // The crop restricts the tile grid rather than being a stage of its own. Crop commutes
    // with every point-wise and local stage in front of it, so this is the same pixels in a
    // different order - and it is what stops a 100 MP frame being fully rendered so that
    // 30 % of it can be thrown away.
    let crop = clamped.geometry.crop;
    let left = (crop.first().copied().unwrap_or(0.0) * frame.width as f32).round() as u32;
    let top = (crop.get(1).copied().unwrap_or(0.0) * frame.height as f32).round() as u32;
    let right = (crop.get(2).copied().unwrap_or(1.0) * frame.width as f32).round() as u32;
    let bottom = (crop.get(3).copied().unwrap_or(1.0) * frame.height as f32).round() as u32;
    let out_width = right.saturating_sub(left).max(1);
    let out_height = bottom.saturating_sub(top).max(1);

    let pixels = out_width as usize * out_height as usize;
    let mut eight = if output.bit_depth == 8 {
        vec![0u8; pixels * 3]
    } else {
        Vec::new()
    };
    let mut sixteen = if output.bit_depth == 8 {
        Vec::new()
    } else {
        vec![0u16; pixels * 3]
    };

    let mut y = top;
    while y < bottom {
        let mut x = left;
        let tile_h = TILE_SIZE.min(bottom - y);
        while x < right {
            let tile_w = TILE_SIZE.min(right - x);

            let (sub, sub_w, sub_h, ox, oy) = extract(frame, x, y, tile_w, tile_h, halo);
            let sub_frame = Frame {
                rgb: sub,
                width: sub_w,
                height: sub_h,
                kind: frame.kind,
                clip: frame.clip,
                camera: frame.camera.clone(),
                // Including the halo: the sub-buffer starts at `x - ox`, not at `x`.
                origin: (x - ox, y - oy),
                full: (frame.width, frame.height),
            };

            let (rendered, rw, _rh, _notes) = engine.working_buffer(
                &sub_frame,
                &tile_recipe,
                &tile_plan,
                RenderLevel::Full,
                Some(stats),
            );

            // Commit the middle of the tile, which is the part the halo protected.
            let committed = crop_out(&rendered, rw, ox, oy, tile_w, tile_h);
            let quantised = crate::output::transform(
                &committed,
                tile_w,
                tile_h,
                output.colour_space,
                output.bit_depth,
                // The dither is indexed in **output raster** coordinates, which is what the
                // whole-frame path uses too. Indexing it in source coordinates would give a
                // cropped export a different dither phase from an uncropped one.
                (x - left, y - top),
            );

            blit(
                &quantised,
                &mut eight,
                &mut sixteen,
                out_width,
                x - left,
                y - top,
                tile_w,
                tile_h,
            );

            x += tile_w;
        }
        y += tile_h;
    }

    let assembled = if output.bit_depth == 8 {
        RenderedData::Eight(eight)
    } else {
        RenderedData::Sixteen(sixteen)
    };

    let mut notes = plan.notes.clone();
    notes.push(RenderNote {
        stage: Stage::OutputTransform.as_str().to_string(),
        reason: SkipReason::NotRequested,
        detail: Some(format!(
            "streamed in {} tiles of {TILE_SIZE} px to stay inside {budget_bytes} bytes",
            tile_count(out_width, out_height, TILE_SIZE)
        )),
    });
    notes.sort_by(|a, b| a.stage.cmp(&b.stage).then(a.reason.cmp(&b.reason)));

    Ok(RenderedImage {
        width: out_width,
        height: out_height,
        data: assembled,
        colour_space: output.colour_space,
        render_hash: graph::render_hash(&clamped.image.content_hash, &clamped, output)?,
        backend: crate::contract::render::Backend::Cpu.as_str().to_string(),
        notes,
        stages_run: plan.slugs(),
        ms: 0,
        cache_hit: false,
    })
}

/// The working bytes a whole-frame render of this size would need.
#[must_use]
pub const fn working_bytes_for(width: u32, height: u32) -> u64 {
    (width as u64) * (height as u64) * BYTES_PER_PIXEL
}

/// Cut a tile plus its halo out of a frame, clamped to the frame's edges.
///
/// Returns the sub-buffer, its size, and where the committed region sits inside it.
fn extract(
    frame: &Frame,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    halo: u32,
) -> (Vec<f32>, u32, u32, u32, u32) {
    let x0 = x.saturating_sub(halo);
    let y0 = y.saturating_sub(halo);
    let x1 = (x + w + halo).min(frame.width);
    let y1 = (y + h + halo).min(frame.height);
    let sub_w = x1.saturating_sub(x0).max(1);
    let sub_h = y1.saturating_sub(y0).max(1);

    let mut out = vec![0.0f32; sub_w as usize * sub_h as usize * 3];
    for row in 0..sub_h {
        let src = ((y0 + row) as usize * frame.width as usize + x0 as usize) * 3;
        let dst = (row as usize * sub_w as usize) * 3;
        let len = sub_w as usize * 3;
        if let (Some(source), Some(target)) =
            (frame.rgb.get(src..src + len), out.get_mut(dst..dst + len))
        {
            target.copy_from_slice(source);
        }
    }
    (out, sub_w, sub_h, x - x0, y - y0)
}

/// Take the committed region out of a rendered tile.
fn crop_out(rgb: &[f32], width: u32, ox: u32, oy: u32, w: u32, h: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; w as usize * h as usize * 3];
    for row in 0..h {
        let src = ((oy + row) as usize * width as usize + ox as usize) * 3;
        let dst = (row as usize * w as usize) * 3;
        let len = w as usize * 3;
        if let (Some(source), Some(target)) = (rgb.get(src..src + len), out.get_mut(dst..dst + len))
        {
            target.copy_from_slice(source);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn blit(
    quantised: &RenderedData,
    eight: &mut [u8],
    sixteen: &mut [u16],
    out_width: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) {
    for row in 0..h {
        let src = (row as usize * w as usize) * 3;
        let dst = ((y + row) as usize * out_width as usize + x as usize) * 3;
        let len = w as usize * 3;
        match quantised {
            RenderedData::Eight(bytes) => {
                if let (Some(source), Some(target)) =
                    (bytes.get(src..src + len), eight.get_mut(dst..dst + len))
                {
                    target.copy_from_slice(source);
                }
            }
            RenderedData::Sixteen(words) => {
                if let (Some(source), Some(target)) =
                    (words.get(src..src + len), sixteen.get_mut(dst..dst + len))
                {
                    target.copy_from_slice(source);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use aura_core::clock::FixedClock;
    use aura_recipe::fixtures as recipes;
    use std::sync::Arc;

    #[test]
    fn the_tile_count_is_a_ceiling_division() {
        assert_eq!(tile_count(1024, 1024, 512), 4);
        assert_eq!(tile_count(1025, 512, 512), 3);
        assert_eq!(tile_count(1, 1, 512), 1);
    }

    #[test]
    fn a_whole_frame_render_of_a_45_megapixel_file_fits_the_default_budget() {
        assert!(working_bytes_for(8192, 5464) < crate::cpu::DEFAULT_WORKING_BYTES);
    }

    #[test]
    fn a_100_megapixel_render_does_not() {
        assert!(working_bytes_for(11648, 8736) > crate::cpu::DEFAULT_WORKING_BYTES);
    }

    #[test]
    fn a_streamed_render_equals_a_whole_frame_render() {
        let frame = fixtures::detail_frame(300, 220);
        let engine = CpuEngine::new(
            Arc::new(fixtures::StaticSource::new(frame.clone())),
            FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );

        let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
        recipe.global.clarity = 40;
        recipe.global.texture = 25;
        recipe.global.sharpen.amount = 70;
        recipe.global.noise.luminance = 30;
        recipe.global.noise.colour = 40;
        recipe.global.exposure = 0.4;
        recipe.global.contrast = 20;

        let whole = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("whole");

        let streamed = render_streamed(
            &engine,
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
            1024,
        )
        .expect("streamed");

        assert_eq!(
            (whole.width, whole.height),
            (streamed.width, streamed.height)
        );
        assert_eq!(
            whole.data, streamed.data,
            "a tiled render must be identical to a whole one"
        );
    }

    #[test]
    fn a_streamed_render_with_a_crop_equals_a_whole_one() {
        let frame = fixtures::detail_frame(256, 256);
        let engine = CpuEngine::new(
            Arc::new(fixtures::StaticSource::new(frame.clone())),
            FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );
        let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
        recipe.global.sharpen.amount = 50;
        recipe.geometry.crop = [0.25, 0.25, 0.75, 0.75];

        let whole = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &OutputSpec::default(),
            )
            .expect("whole");
        let streamed = render_streamed(
            &engine,
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
            1024,
        )
        .expect("streamed");

        assert_eq!(
            (whole.width, whole.height),
            (streamed.width, streamed.height)
        );
        assert_eq!(whole.data, streamed.data);
    }

    #[test]
    fn a_rotated_frame_falls_back_to_the_whole_path_and_says_so() {
        let frame = fixtures::detail_frame(128, 128);
        let engine = CpuEngine::new(
            Arc::new(fixtures::StaticSource::new(frame.clone())),
            FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );
        let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
        recipe.geometry.rotate = 3.0;

        let out = render_streamed(
            &engine,
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
            1024,
        )
        .expect("streamed");

        assert!(out.notes.iter().any(|n| n
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("rendered whole"))));
    }

    #[test]
    fn a_sixteen_bit_streamed_render_equals_a_whole_one() {
        let frame = fixtures::detail_frame(200, 140);
        let engine = CpuEngine::new(
            Arc::new(fixtures::StaticSource::new(frame.clone())),
            FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );
        let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
        recipe.global.clarity = 30;
        let spec = OutputSpec {
            bit_depth: 16,
            ..OutputSpec::default()
        };

        let whole = engine
            .render_frame(
                &frame,
                &recipe,
                RenderLevel::Full,
                RenderPurpose::Export,
                &spec,
            )
            .expect("whole");
        let streamed = render_streamed(
            &engine,
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &spec,
            1024,
        )
        .expect("streamed");
        assert_eq!(whole.data, streamed.data);
    }
}
