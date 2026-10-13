//! Match a finished version of the same photo through AURA's actual renderer.
//! Reference pixels guide bounded tone/color settings; they never enter the export or recipe.
use std::io::Read;
use std::path::Path;
use std::sync::PoisonError;

use aura_core::progress::CancelToken;
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_raw::codec::Rgb8;
use aura_recipe::{schema, EditSource, Recipe};
use aura_render::{Frame, FrameSource, OutputSpec, RenderLevel, RenderPurpose, RenderedData};
use serde::{Deserialize, Serialize};

use crate::{commands::IpcResult, AppState};

const EDGE: u32 = 384;
const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_PIXELS: u32 = 16_000_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedReferenceInput {
    pub photo_id: String,
    pub reference_path: String,
    pub cancel_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedReferenceReport {
    pub changed: usize,
    pub probes: usize,
    /// Mean absolute RGB byte error at export dimensions, not a quality percentage.
    pub before_error: f64,
    pub after_error: f64,
    pub exact_match: bool,
    pub matching_pixels: u64,
    pub total_pixels: u64,
    pub protected_fields: usize,
    pub reference_hash: String,
    pub width: u32,
    pub height: u32,
    pub subject_errors_before: Vec<f64>,
    pub subject_errors_after: Vec<f64>,
}

fn refused(message: &str) -> aura_core::AuraError {
    let mut error = aura_core::errors::ml::look_reference_refused(message);
    error.user_message = message.to_string();
    error
}

fn check_cancel(cancel: &CancelToken) -> AuraResult<()> {
    if cancel.is_cancelled() {
        return Err(refused("Reference matching stopped. No edit was saved."));
    }
    Ok(())
}

fn decode_reference(path: &Path) -> AuraResult<(Rgb8, String)> {
    // Bound the read itself, including a file that grows after opening.
    let file =
        std::fs::File::open(path).map_err(|_| refused("Cannot open the reference photo."))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| refused("Cannot read the reference photo."))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(refused(
            "Choose a reference JPEG or PNG smaller than 64 MB.",
        ));
    }
    let limits = aura_raw::timeout::DecodeLimits {
        max_pixels: u64::from(MAX_PIXELS),
        max_alloc_bytes: u64::from(MAX_PIXELS) * 4,
        ..aura_raw::timeout::DecodeLimits::tier1()
    };
    let image = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        aura_raw::codec::decode_png(&bytes, limits)?
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        aura_raw::codec::decode_jpeg(&bytes, limits)?
    } else {
        return Err(refused("Choose a JPEG or PNG export of this same photo."));
    };
    Ok((image, blake3::hash(&bytes).to_hex().to_string()))
}

#[derive(Debug, Clone, Copy)]
struct Score {
    error: f64,
    matching: u64,
    pixels: u64,
}

fn subject_bounds(recipe: &Recipe) -> AuraResult<Vec<[f64; 4]>> {
    let Some(record) = recipe.extra.get("studio_portrait_auto_v1") else {
        return Ok(Vec::new());
    };
    let Some(faces) = record.get("faces").and_then(serde_json::Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut bounds = Vec::new();
    for face in faces {
        let rect: [f64; 4] = serde_json::from_value(
            face.get("bounds")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        )
        .map_err(|_| {
            refused("The saved portrait has invalid face bounds. Run portrait analysis again.")
        })?;
        if !rect
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            || rect.first() >= rect.get(2)
            || rect.get(1) >= rect.get(3)
        {
            return Err(refused(
                "The saved portrait has invalid face bounds. Run portrait analysis again.",
            ));
        }
        bounds.push(rect);
    }
    Ok(bounds)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn subject_errors(image: &Rgb8, target: &Rgb8, bounds: &[[f64; 4]]) -> AuraResult<Vec<f64>> {
    // Validate the shared dimensions/lengths before addressing any region.
    score(&image.data, target, image.width, image.height)?;
    bounds
        .iter()
        .map(|rect| {
            let [left, top, right, bottom] = *rect;
            let x0 = (left * f64::from(image.width)).floor() as u32;
            let y0 = (top * f64::from(image.height)).floor() as u32;
            let x1 = (right * f64::from(image.width)).floor() as u32;
            let y1 = (bottom * f64::from(image.height)).floor() as u32;
            let mut difference = 0_u64;
            let mut samples = 0_u64;
            for row in y0..y1 {
                for column in x0..x1 {
                    let index =
                        (u64::from(row) * u64::from(image.width) + u64::from(column)) as usize * 3;
                    for channel in 0..3 {
                        let source = image
                            .data
                            .get(index + channel)
                            .ok_or_else(|| refused("Invalid subject pixels."))?;
                        let reference = target
                            .data
                            .get(index + channel)
                            .ok_or_else(|| refused("Invalid reference pixels."))?;
                        difference += u64::from(source.abs_diff(*reference));
                        samples += 1;
                    }
                }
            }
            if samples == 0 {
                return Err(refused("A saved face region contains no pixels."));
            }
            Ok(difference as f64 / samples as f64)
        })
        .collect()
}

fn subjects_preserved(before: &[f64], after: &[f64]) -> bool {
    before.len() == after.len()
        && before
            .iter()
            .zip(after)
            .all(|(a, b)| b.is_finite() && *b <= *a + 0.000_001)
}

#[allow(clippy::cast_precision_loss)]
fn merit(whole: f64, subjects: &[f64]) -> f64 {
    if subjects.is_empty() {
        whole
    } else {
        whole * 0.2 + subjects.iter().sum::<f64>() / subjects.len() as f64 * 0.8
    }
}

#[allow(clippy::cast_precision_loss)]
fn score(rgb: &[u8], target: &Rgb8, width: u32, height: u32) -> AuraResult<Score> {
    let expected = u64::from(width) * u64::from(height) * 3;
    if width != target.width
        || height != target.height
        || expected == 0
        || rgb.len() as u64 != expected
        || target.data.len() != rgb.len()
    {
        return Err(refused(
            "The reference must have the same dimensions and crop as this photo.",
        ));
    }
    let difference: u64 = rgb
        .iter()
        .zip(&target.data)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    let matching = rgb
        .chunks_exact(3)
        .zip(target.data.chunks_exact(3))
        .filter(|(a, b)| a == b)
        .count() as u64;
    Ok(Score {
        error: difference as f64 / expected as f64,
        matching,
        pixels: expected / 3,
    })
}

/// Signed luminance gradients distinguish alignment from a color/brightness change.
fn edge_alignment(source: &[u8], target: &[u8], width: u32, height: u32) -> f64 {
    let columns = width as usize;
    let rows = height as usize;
    if source.len() != columns * rows * 3 || source.len() != target.len() || columns < 2 || rows < 2
    {
        return 0.0;
    }
    let luma = |rgb: &[u8], i: usize| -> f64 {
        f64::from(rgb.get(i * 3).copied().unwrap_or(0)) * 0.2126
            + f64::from(rgb.get(i * 3 + 1).copied().unwrap_or(0)) * 0.7152
            + f64::from(rgb.get(i * 3 + 2).copied().unwrap_or(0)) * 0.0722
    };
    let (mut dot, mut norm_a, mut norm_b) = (0.0, 0.0, 0.0);
    for y in 0..rows - 1 {
        for x in 0..columns - 1 {
            let i = y * columns + x;
            for neighbour in [i + 1, i + columns] {
                let ga = luma(source, neighbour) - luma(source, i);
                let gb = luma(target, neighbour) - luma(target, i);
                dot += ga * gb;
                norm_a += ga * ga;
                norm_b += gb * gb;
            }
        }
    }
    if norm_a * norm_b < 1.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b).sqrt()
}

const AXES: usize = 9;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn perturb(base: &Recipe, best: &Recipe, axis: usize, step: f32) -> AuraResult<Recipe> {
    let mut next = best.clone();
    let g = &mut next.global;
    let b = &base.global;
    match axis {
        0 => {
            g.exposure = (g.exposure + step * 0.3)
                .clamp((b.exposure - 0.75).max(-5.0), (b.exposure + 0.75).min(5.0));
        }
        1 => {
            g.temperature = (g.temperature as f32 + step * 300.0).round().clamp(
                (b.temperature as f32 - 1200.0).max(2000.0),
                (b.temperature as f32 + 1200.0).min(50000.0),
            ) as u32;
        }
        2 => {
            g.tint = (f32::from(g.tint) + step * 6.0).round().clamp(
                f32::from((b.tint - 24).max(-150)),
                f32::from((b.tint + 24).min(150)),
            ) as i16;
        }
        3..=8 => {
            let (value, original) = match axis {
                3 => (&mut g.contrast, b.contrast),
                4 => (&mut g.highlights, b.highlights),
                5 => (&mut g.shadows, b.shadows),
                6 => (&mut g.whites, b.whites),
                7 => (&mut g.blacks, b.blacks),
                _ => (&mut g.saturation, b.saturation),
            };
            *value = (f32::from(*value) + step * if axis == 8 { 8.0 } else { 18.0 })
                .round()
                .clamp(
                    f32::from((original - 36).max(-100)),
                    f32::from((original + 36).min(100)),
                ) as i16;
        }
        _ => return Err(refused("Unknown reference parameter.")),
    }
    Ok(schema::merge(base, &next, EditSource::Ai)?.0)
}

fn fit<F>(
    base: &Recipe,
    before: f64,
    cancel: &CancelToken,
    mut evaluate: F,
) -> AuraResult<(Recipe, usize)>
where
    F: FnMut(&Recipe) -> AuraResult<f64>,
{
    if before <= 0.000_001 {
        return Ok((base.clone(), 0));
    }
    let mut best = base.clone();
    let mut distance = before;
    let mut probes = 0;
    // Deterministic coordinate descent; <=72 renders, no unbounded/stochastic search.
    // The last sweep can resolve exposure changes below the old 0.033-stop step.
    for scale in [1.0, 0.33, 0.11, 0.035] {
        for axis in 0..AXES {
            check_cancel(cancel)?;
            let centre = best.clone();
            for sign in [-1.0, 1.0] {
                let proposal = perturb(base, &centre, axis, sign * scale)?;
                if proposal.global == centre.global {
                    continue;
                }
                schema::Validation::check(&proposal)?;
                check_cancel(cancel)?;
                let error = evaluate(&proposal)?;
                probes += 1;
                if error.is_finite() && error + 0.000_001 < distance {
                    distance = error;
                    best = proposal;
                }
            }
        }
    }
    Ok((best, probes))
}

fn rendered(engine: &aura_render::CpuEngine, frame: &Frame, recipe: &Recipe) -> AuraResult<Rgb8> {
    let image = engine.render_frame(
        frame,
        recipe,
        RenderLevel::Full,
        RenderPurpose::Analysis,
        &OutputSpec::default(),
    )?;
    let RenderedData::Eight(rgb) = image.data else {
        return Err(refused("Reference matching requires an 8-bit sRGB render."));
    };
    Ok(Rgb8 {
        width: image.width,
        height: image.height,
        data: rgb,
    })
}

/// Match a finished reference and save one reversible edit after full-resolution measurement.
/// # Errors
/// Invalid/unaligned/oversized reference, cancelled work, missing saved edit, or concurrent edit.
pub fn match_paired_reference(
    state: &AppState,
    input: &PairedReferenceInput,
) -> IpcResult<PairedReferenceReport> {
    uuid::Uuid::parse_str(&input.cancel_id)
        .map_err(|_| refused("Invalid reference matching job."))?;
    let cancel = CancelToken::new();
    state.register_job(&input.cancel_id, cancel.clone());
    let result = run(state, input, &cancel);
    state.finish_job(&input.cancel_id);
    Ok(result?)
}

#[allow(clippy::too_many_lines)] // Keep measurement and its single conditional save together.
fn run(
    state: &AppState,
    input: &PairedReferenceInput,
    cancel: &CancelToken,
) -> AuraResult<PairedReferenceReport> {
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| refused("Invalid photo."))?;
    let (project, width, height): (String, Option<i64>, Option<i64>) =
        state.catalog().read(|conn| {
            conn.query_row(
                "SELECT project_id, width_px, height_px FROM photo WHERE photo_id=?1",
                [&input.photo_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("paired reference photo", &e))
        })?;
    let known_dimensions = matches!((width, height), (Some(w), Some(h)) if w > 0 && h > 0);
    if matches!((width, height), (Some(w), Some(h)) if w > 0 && h > 0 && w.checked_mul(h).is_none_or(|p| p > i64::from(MAX_PIXELS)))
    {
        return Err(refused(
            "Paired reference matching supports photos up to 16 megapixels.",
        ));
    }
    let current = state
        .recipe_store()
        .load(&photo)?
        .ok_or_else(|| refused("Save an edit before matching its finished reference."))?;
    let expected_hash = aura_recipe::hash::recipe_hash(&current)?;
    let bounds = subject_bounds(&current)?;
    // Includes source/target decode and all renders. Live previews defer while this lane is busy.
    let _work = state
        .image_work_gate()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    check_cancel(cancel)?;
    let (reference, reference_hash) = decode_reference(Path::new(&input.reference_path))?;
    if !known_dimensions {
        // PNG imports can lack EXIF dimensions even though their pixels decode correctly.
        // Prove the bound with a limited original decode before requesting the full frame.
        use aura_preview::source::{CatalogSource, PreviewSource};
        let source = CatalogSource::new(std::sync::Arc::clone(state.catalog()), project.clone())
            .locate(photo)?;
        let (bounded, hash) = decode_reference(&source.path)?;
        if hash != source.content_hash {
            return Err(refused(
                "The original file changed since import. Reimport it before matching.",
            ));
        }
        drop(bounded);
    }
    let full =
        crate::photo_frames::CatalogFrames::new(state.clone()).frame(&photo, RenderLevel::Full)?;
    if full.pixels() > u64::from(MAX_PIXELS) {
        return Err(refused(
            "This photo exceeds the 16 megapixel paired reference limit.",
        ));
    }
    if (full.width, full.height) != (reference.width, reference.height) {
        return Err(refused(
            "The reference must have the same dimensions and crop as this photo.",
        ));
    }
    let target_small = aura_raw::thumb::scale_to(&reference, EDGE);
    let scaled = aura_raw::demosaic::resize(
        &aura_raw::demosaic::RgbF32 {
            width: full.width,
            height: full.height,
            data: full.rgb.clone(),
        },
        target_small.width,
        target_small.height,
    );
    let small = Frame::working(scaled.data, scaled.width, scaled.height, &full.camera);
    let engine = state.render()?;
    let baseline = rendered(&engine, &small, &current)?;
    let initial = score(
        &baseline.data,
        &target_small,
        baseline.width,
        baseline.height,
    )?;
    let initial_subjects = subject_errors(&baseline, &target_small, &bounds)?;
    if edge_alignment(
        &baseline.data,
        &target_small.data,
        baseline.width,
        baseline.height,
    ) < 0.80
    {
        return Err(refused(
            "Choose the finished version of this same photo, with the same crop and orientation.",
        ));
    }
    check_cancel(cancel)?;
    let before_full = rendered(&engine, &full, &current)?;
    let before = score(
        &before_full.data,
        &reference,
        before_full.width,
        before_full.height,
    )?;
    let before_subjects = subject_errors(&before_full, &reference, &bounds)?;
    drop(before_full);
    let (mut best, probes) = fit(
        &current,
        merit(initial.error, &initial_subjects),
        cancel,
        |proposal| {
            let result = rendered(&engine, &small, proposal)?;
            let error = score(&result.data, &target_small, result.width, result.height)?.error;
            let subjects = subject_errors(&result, &target_small, &bounds)?;
            Ok(if subjects_preserved(&initial_subjects, &subjects) {
                merit(error, &subjects)
            } else {
                f64::INFINITY
            })
        },
    )?;
    check_cancel(cancel)?;
    let final_frame = rendered(&engine, &full, &best)?;
    let mut after = score(
        &final_frame.data,
        &reference,
        final_frame.width,
        final_frame.height,
    )?;
    let mut after_subjects = subject_errors(&final_frame, &reference, &bounds)?;
    // A preview improvement can reverse at export resolution. Those proposals are never saved.
    if after.error + 0.000_001 >= before.error
        || !subjects_preserved(&before_subjects, &after_subjects)
    {
        best = current.clone();
        after = before;
        after_subjects.clone_from(&before_subjects);
    }
    let (_, merge) = schema::merge(&current, &best, EditSource::Ai)?;
    check_cancel(cancel)?;
    if !merge.changed.is_empty() {
        best.provenance.source = EditSource::Ai;
        best.provenance.confidence = 0.4;
        let project = ProjectId::from_db(&project).map_err(|_| refused("Invalid collection."))?;
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &best,
            &merge.changed,
            &format!(
                "Paired reference {}: full-resolution RGB error {:.4} to {:.4}; exact equality {}",
                reference_hash.get(..12).unwrap_or(&reference_hash),
                before.error,
                after.error,
                after.matching == after.pixels
            ),
            &expected_hash,
        )?;
    }
    Ok(PairedReferenceReport {
        changed: merge.changed.len(),
        probes,
        before_error: before.error,
        after_error: after.error,
        exact_match: after.matching == after.pixels,
        matching_pixels: after.matching,
        total_pixels: after.pixels,
        protected_fields: current.provenance.user_edited_fields.len(),
        reference_hash,
        width: reference.width,
        height: reference.height,
        subject_errors_before: before_subjects,
        subject_errors_after: after_subjects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn better_background_error_cannot_hide_a_worse_face() {
        let target = Rgb8 {
            width: 8,
            height: 2,
            data: vec![100; 48],
        };
        let mut before = target.clone();
        let mut after = target.clone();
        for row in 0..2 {
            for column in 0..8 {
                for channel in 0..3 {
                    let index = (row * 8 + column) * 3 + channel;
                    if column >= 4 {
                        before.data[index] = 150;
                    }
                    if column < 2 {
                        after.data[index] = 102;
                    }
                }
            }
        }
        let bounds = [[0.0, 0.0, 0.25, 1.0], [0.25, 0.0, 0.5, 1.0]];
        let baseline = subject_errors(&before, &target, &bounds).unwrap();
        let candidate = subject_errors(&after, &target, &bounds).unwrap();
        assert!(
            score(&after.data, &target, 8, 2).unwrap().error
                < score(&before.data, &target, 8, 2).unwrap().error
        );
        assert!(!subjects_preserved(&baseline, &candidate));
        assert!(subjects_preserved(&baseline, &baseline));
    }
    #[test]
    fn a_png_without_exif_dimensions_is_decoded_with_its_real_bounds_and_hash() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original.png");
        let pixels = vec![80; 2 * 3 * 3];
        {
            let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), 2, 3);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&pixels)
                .unwrap();
        }
        let (image, hash) = decode_reference(&path).unwrap();
        assert_eq!((image.width, image.height), (2, 3));
        assert_eq!(image.data, pixels);
        assert_eq!(
            hash,
            blake3::hash(&std::fs::read(path).unwrap())
                .to_hex()
                .to_string()
        );
    }
    #[test]
    fn equality_is_strict_and_dimensions_are_never_aligned_silently() {
        let target = Rgb8 {
            width: 2,
            height: 1,
            data: vec![10, 20, 30, 40, 50, 60],
        };
        let same = score(&target.data, &target, 2, 1).unwrap();
        assert_eq!(same.error, 0.0);
        assert_eq!(same.matching, 2);
        let different = score(&[10, 20, 31, 40, 50, 60], &target, 2, 1).unwrap();
        assert_eq!(different.matching, 1);
        assert!(different.error > 0.0);
        assert!(score(&target.data, &target, 1, 2).is_err());
        assert!(score(&target.data[..3], &target, 2, 1).is_err());
    }
    #[test]
    fn a_different_scene_is_not_a_paired_reference() {
        let a: Vec<u8> = (0..36)
            .flat_map(|i| [if i % 6 < 3 { 20 } else { 180 }; 3])
            .collect();
        let shifted: Vec<u8> = a.iter().map(|v| v + 10).collect();
        let rotated: Vec<u8> = (0..36)
            .flat_map(|i| [if i / 6 < 3 { 20 } else { 180 }; 3])
            .collect();
        assert!(edge_alignment(&a, &shifted, 6, 6) > 0.99);
        assert!(edge_alignment(&a, &rotated, 6, 6) < 0.80);
    }
    #[test]
    fn bounded_fit_preserves_manual_fields_and_the_entire_retouch_stack() {
        let mut base = aura_recipe::fixtures::reference();
        base.provenance.user_edited_fields =
            vec!["global.temperature".into(), "global.shadows".into()];
        let cancel = CancelToken::new();
        let (best, probes) = fit(&base, 100.0, &cancel, |r| {
            Ok(f64::from((r.global.exposure - base.global.exposure - 0.4).abs()) * 100.0)
        })
        .unwrap();
        assert!(best.global.exposure > base.global.exposure);
        assert_eq!(best.global.temperature, base.global.temperature);
        assert_eq!(best.global.shadows, base.global.shadows);
        assert_eq!(best.retouch, base.retouch);
        assert_eq!(best.extra, base.extra);
        assert_eq!(best.masks, base.masks);
        assert_eq!(best.geometry, base.geometry);
        assert!(probes <= 72);
        assert_eq!(fit(&base, 0.0, &cancel, |_| Ok(0.0)).unwrap().0, base);
        cancel.cancel();
        assert!(fit(&base, 100.0, &cancel, |_| panic!(
            "must not render after cancellation"
        ))
        .is_err());
    }

    #[test]
    fn actual_native_renders_get_closer_without_replacing_reference_pixels() {
        #[derive(Debug)]
        struct Source(Frame);
        impl FrameSource for Source {
            fn frame(&self, _: &PhotoId, _: RenderLevel) -> AuraResult<Frame> {
                Ok(self.0.clone())
            }
        }
        let bytes: Vec<u8> = (0..256_u16)
            .flat_map(|i| {
                let v = u8::try_from(30 + i % 160).unwrap();
                [v, v.saturating_add(10), v.saturating_sub(8)]
            })
            .collect();
        let frame = Frame::working(crate::photo_frames::srgb8_to_working(&bytes), 16, 16, "");
        let engine = aura_render::CpuEngine::new(
            std::sync::Arc::new(Source(frame.clone())),
            aura_core::clock::FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );
        let base = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "");
        let mut desired = base.clone();
        desired.global.exposure = 0.3;
        desired.global.contrast = 6;
        let target = rendered(&engine, &frame, &desired).unwrap();
        let original = rendered(&engine, &frame, &base).unwrap();
        let before = score(&original.data, &target, 16, 16).unwrap().error;
        let (best, _) = fit(&base, before, &CancelToken::new(), |r| {
            let image = rendered(&engine, &frame, r)?;
            Ok(score(&image.data, &target, 16, 16)?.error)
        })
        .unwrap();
        let result = rendered(&engine, &frame, &best).unwrap();
        assert!(score(&result.data, &target, 16, 16).unwrap().error < before);
        assert_eq!(best.extra, base.extra);
        assert_eq!(best.geometry, base.geometry);
        assert_eq!(best.retouch, base.retouch);

        // A small real exposure correction sits below the old 0.033-stop
        // final step. Locked colour controls must not disguise that failure.
        let mut fine_base = base;
        fine_base.provenance.user_edited_fields = [
            "temperature",
            "tint",
            "contrast",
            "highlights",
            "shadows",
            "whites",
            "blacks",
            "saturation",
        ]
        .map(|field| format!("global.{field}"))
        .to_vec();
        let mut fine_desired = fine_base.clone();
        fine_desired.global.exposure = 0.012;
        let fine_target = rendered(&engine, &frame, &fine_desired).unwrap();
        let fine_before = score(&original.data, &fine_target, 16, 16).unwrap().error;
        let (fine_best, _) = fit(&fine_base, fine_before, &CancelToken::new(), |r| {
            let image = rendered(&engine, &frame, r)?;
            Ok(score(&image.data, &fine_target, 16, 16)?.error)
        })
        .unwrap();
        assert!(fine_best.global.exposure > 0.0 && fine_best.global.exposure < 0.02);
        let fine_result = rendered(&engine, &frame, &fine_best).unwrap();
        assert!(
            score(&fine_result.data, &fine_target, 16, 16)
                .unwrap()
                .error
                < fine_before
        );
        assert_eq!(
            fine_best.provenance.user_edited_fields,
            fine_base.provenance.user_edited_fields
        );
        assert_eq!(fine_best.retouch, fine_base.retouch);
    }
}
