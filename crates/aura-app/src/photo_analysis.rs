//! Twenty read-only diagnostics of the saved, rendered photograph.
//!
//! These are bounded proxy measurements, not camera/sensor or face-model claims.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::too_many_lines
)]

use crate::{commands::IpcResult, AppState};
use aura_core::{AuraResult, PhotoId};
use aura_render::{CpuEngine, FrameSource, RenderLevel, RenderPurpose};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const EDGE: usize = 512;
mod advanced;
mod delivery;
mod quality;
const GW: usize = 16;
const GH: usize = 12;
const CELLS: usize = GW * GH;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub project_id: String,
    pub photo_id: String,
    pub recipe_hash: String,
}

/// All visualizations are finite, bounded arrays; row-major for density and map views.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub id: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub unit: String,
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
    pub labels: Vec<String>,
    pub low: f32,
    pub high: f32,
    pub samples: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub recipe_hash: String,
    pub width: u32,
    pub height: u32,
    pub diagnostics: Vec<Diagnostic>,
    pub notes: Vec<String>,
}

fn invalid(message: &str) -> aura_core::AuraError {
    let mut error = aura_recipe::errors::recipe_invalid("photo analysis", message);
    message.clone_into(&mut error.user_message);
    error
}

/// Analyze one saved revision without editing its recipe or imported original.
/// # Errors
/// Rejects foreign membership, stale revision, invalid pixels and failed rendering.
pub fn inspect(state: &AppState, input: &Input) -> IpcResult<Report> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    let hash = aura_recipe::recipe_hash(&recipe)?;
    if hash != input.recipe_hash {
        return Err(invalid("Photo changed. Run the inspection again.").into());
    }
    let source = Arc::new(crate::photo_frames::CatalogFrames::new(state.clone()));
    let level = RenderLevel::Screen(EDGE as u32, EDGE as u32);
    let frame = source.frame(&photo, level)?;
    let engine = CpuEngine::new(source, Arc::clone(state.clock()));
    let plan = aura_render::graph::plan(
        &recipe,
        RenderPurpose::Export,
        frame.kind,
        aura_render::Capabilities {
            mask_generators: true,
            retouch_operators: true,
            geometry_models: true,
            ..Default::default()
        },
    );
    let (rgb, width, height, notes) = engine.working_buffer(&frame, &recipe, &plan, level, None);
    let diagnostics = measure(&rgb, width as usize, height as usize)?;
    if aura_recipe::recipe_hash(&crate::develop_commands::load_or_neutral(state, photo)?)? != hash {
        return Err(invalid("Photo changed during inspection. Run it again.").into());
    }
    Ok(Report {
        recipe_hash: hash,
        width,
        height,
        diagnostics,
        notes: notes
            .iter()
            .take(16)
            .map(|note| note.reason.as_str().to_owned())
            .collect(),
    })
}

fn diagnostic(
    text: (&str, &str, &str),
    kind: &str,
    unit: &str,
    size: (usize, usize),
    values: Vec<f32>,
    labels: &[&str],
    samples: usize,
) -> Diagnostic {
    let (id, title, description) = text;
    let low = values.iter().copied().fold(f32::INFINITY, f32::min);
    let high = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    Diagnostic {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        kind: kind.into(),
        unit: unit.into(),
        width: size.0,
        height: size.1,
        values,
        labels: labels.iter().map(|s| (*s).into()).collect(),
        low,
        high,
        samples,
    }
}

fn divide(sum: &[f32], count: &[usize]) -> Vec<f32> {
    sum.iter()
        .zip(count)
        .map(|(s, n)| if *n == 0 { 0.0 } else { *s / *n as f32 })
        .collect()
}
fn mean(v: &[f32]) -> f32 {
    v.iter().sum::<f32>() / v.len().max(1) as f32
}
fn sat(p: [f32; 3]) -> f32 {
    let high = p.into_iter().fold(0.0_f32, f32::max);
    (high - p.into_iter().fold(f32::INFINITY, f32::min)) / high.max(1e-6)
}
fn ev(value: f32) -> f32 {
    (value.max(1e-6) / 0.18).log2()
}
fn correlation(pixels: &[[f32; 3]], a: usize, b: usize) -> f32 {
    let n = pixels.len() as f64;
    let ma = pixels.iter().map(|p| f64::from(p[a])).sum::<f64>() / n;
    let mb = pixels.iter().map(|p| f64::from(p[b])).sum::<f64>() / n;
    let (mut aa, mut bb, mut ab) = (0.0_f64, 0.0_f64, 0.0_f64);
    for p in pixels {
        let da = f64::from(p[a]) - ma;
        let db = f64::from(p[b]) - mb;
        aa += da * da;
        bb += db * db;
        ab += da * db;
    }
    if aa * bb <= 1e-20 {
        0.0
    } else {
        (ab / (aa * bb).sqrt()).clamp(-1.0, 1.0) as f32
    }
}

/// Measure at most 512×512 edited working-space pixels. Nonfinite input is refused.
/// # Errors
/// Empty, oversized, incomplete and nonfinite frames have no trustworthy report.
pub fn measure(rgb: &[f32], width: usize, height: usize) -> AuraResult<Vec<Diagnostic>> {
    let expected = width.checked_mul(height).and_then(|v| v.checked_mul(3));
    if width == 0
        || height == 0
        || width > EDGE
        || height > EDGE
        || expected != Some(rgb.len())
        || rgb.iter().any(|v| !v.is_finite() || v.abs() > 1e6)
    {
        return Err(invalid("Photo has no usable bounded pixels"));
    }
    let pixels: Vec<[f32; 3]> = rgb.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    let n = pixels.len();
    let luma: Vec<f32> = pixels
        .iter()
        .map(|p| p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593)
        .collect();
    let domain: Vec<[f32; 3]> = pixels
        .iter()
        .map(|p| p.map(|v| aura_render::tonemap::curve_domain_encode(v).clamp(0.0, 1.0)))
        .collect();
    let mut counts = vec![0_usize; CELLS];
    let mut light = vec![0.0; CELLS];
    let mut square = vec![0.0; CELLS];
    let mut stops = vec![0.0; CELLS];
    let mut clip = vec![0.0; CELLS];
    let mut focus = vec![0.0; CELLS];
    let mut residual = vec![0.0; CELLS];
    let mut noise_count = vec![0_usize; CELLS];
    let mut parade = vec![0.0; 96 * 48];
    let mut waveform = vec![0.0; 96 * 48];
    let mut vector = vec![0.0; 48 * 48];
    let mut zones = vec![0.0; 15];
    let mut orientation = vec![0.0; 12];
    let mut gamut = vec![0.0; 3];
    let mut neutral = [0.0_f32; 3];
    let mut neutral_n = 0;
    let mut chroma = [0.0_f32; 3];
    let mut chroma_n = [0_usize; 3];
    let mut block_sum = [0.0_f32; 8];
    let mut block_n = [0_usize; 8];
    let mut rows = vec![0.0; height];
    let mut columns = vec![0.0; width];
    let mut registration = [0.0_f32; 2];
    let mut registration_n = 0;
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let cell = y * GH / height * GW + x * GW / width;
            let p = pixels[i];
            let q = domain[i];
            let lum = luma[i];
            counts[cell] += 1;
            light[cell] += lum;
            square[cell] += lum * lum;
            stops[cell] += ev(lum);
            rows[y] += lum / width as f32;
            columns[x] += lum / height as f32;
            clip[cell] += if p.iter().any(|v| *v <= 0.0 || *v >= 1.0) {
                100.0
            } else {
                0.0
            };
            zones[(ev(lum).floor() as i32 + 7).clamp(0, 14) as usize] += 1.0;
            for (c, value) in q.iter().enumerate() {
                let col = c * 32 + x * 32 / width;
                let row = 47 - (*value * 47.0).round() as usize;
                parade[row * 96 + col] += 1.0;
            }
            let encoded_light = aura_render::tonemap::curve_domain_encode(lum).clamp(0.0, 1.0);
            waveform[(47 - (encoded_light * 47.0).round() as usize) * 96 + x * 96 / width] += 1.0;
            let u = ((q[2] - q[1]) * 0.5 + 0.5).clamp(0.0, 1.0);
            let v = ((q[0] - q[1]) * 0.5 + 0.5).clamp(0.0, 1.0);
            vector[(47 - (v * 47.0).round() as usize) * 48 + (u * 47.0).round() as usize] += 1.0;
            if lum > 0.02 && lum < 0.8 && sat(p) < 0.12 {
                for c in 0..3 {
                    neutral[c] += p[c] / lum;
                }
                neutral_n += 1;
            }
            let region = if lum < 0.09 {
                0
            } else if lum < 0.72 {
                1
            } else {
                2
            };
            chroma[region] += sat(p).clamp(0.0, 1.0) * 100.0;
            chroma_n[region] += 1;
            let srgb = aura_raw::colour::working_space::working_to_linear_srgb(p.map(f64::from));
            if srgb.iter().any(|v| *v < 0.0) {
                gamut[1] += 1.0;
            } else if srgb.iter().any(|v| *v > 1.0) {
                gamut[2] += 1.0;
            } else {
                gamut[0] += 1.0;
            }
            if x > 0 {
                let d = (lum - luma[i - 1]).abs();
                block_sum[x % 8] += d;
                block_n[x % 8] += 1;
            }
            if y > 0 {
                let d = (lum - luma[i - width]).abs();
                block_sum[y % 8] += d;
                block_n[y % 8] += 1;
            }
            if x > 0 && y > 0 && x + 1 < width && y + 1 < height {
                let dx = (luma[i + 1] - luma[i - 1]) * 0.5;
                let dy = (luma[i + width] - luma[i - width]) * 0.5;
                let gradient = dx.hypot(dy);
                focus[cell] += gradient;
                let bin = (((dy.atan2(dx) + std::f32::consts::PI) / (2.0 * std::f32::consts::PI)
                    * 12.0)
                    .floor() as usize)
                    .min(11);
                orientation[bin] += gradient;
                if gradient < lum.abs() * 0.1 + 0.002 {
                    residual[cell] += (lum
                        - (luma[i - 1] + luma[i + 1] + luma[i - width] + luma[i + width]) * 0.25)
                        .abs();
                    noise_count[cell] += 1;
                }
                // Compare edge centroids in a five-pixel strip, rather than treating
                // differently saturated objects as an optical registration measurement.
                if x >= 2 && x + 2 < width && gradient > 0.02 && sat(p) < 0.15 {
                    let mut mass = [0.0_f32; 3];
                    let mut center = [0.0_f32; 3];
                    for offset in 0..4 {
                        for c in 0..3 {
                            let d = (pixels[i + offset - 1][c] - pixels[i + offset - 2][c]).abs();
                            mass[c] += d;
                            center[c] += d * (offset as f32 - 1.5);
                        }
                    }
                    if mass.iter().all(|m| *m > 0.03) {
                        registration[0] += center[0] / mass[0] - center[1] / mass[1];
                        registration[1] += center[2] / mass[2] - center[1] / mass[1];
                        registration_n += 1;
                    }
                }
            }
        }
    }
    let averages = divide(&light, &counts);
    let contrast: Vec<f32> = square
        .iter()
        .zip(&averages)
        .zip(&counts)
        .map(|((s, m), c)| {
            if *c == 0 {
                0.0
            } else {
                (s / *c as f32 - m * m).max(0.0).sqrt() / m.abs().max(0.001)
            }
        })
        .collect();
    let mut sorted = luma.clone();
    sorted.sort_by(f32::total_cmp);
    let percentile = |q: f32| sorted[((n - 1) as f32 * q).round() as usize];
    let range = [0.01, 0.05, 0.5, 0.95, 0.99].map(|q| ev(percentile(q)));
    let mut dynamic = range.to_vec();
    dynamic.push((range[4] - range[0]).max(0.0));
    let mut bands = Vec::with_capacity(64);
    for profile in [&rows, &columns] {
        for bin in 0..32 {
            let start = bin * profile.len() / 32;
            let end = ((bin + 1) * profile.len() / 32)
                .max(start + 1)
                .min(profile.len());
            let mut sum = 0.0;
            let mut count = 0;
            for j in start.max(1)..end.saturating_sub(1) {
                sum += (profile[j + 1] - 2.0 * profile[j] + profile[j - 1]).abs();
                count += 1;
            }
            bands.push(if count == 0 { 0.0 } else { sum / count as f32 });
        }
    }
    let mut detail = Vec::new();
    for radius in [1, 2, 4, 8, 16] {
        let mut sum = 0.0;
        let mut count = 0;
        for y in radius..height.saturating_sub(radius) {
            for x in radius..width.saturating_sub(radius) {
                let i = y * width + x;
                sum += (luma[i]
                    - (luma[i - radius]
                        + luma[i + radius]
                        + luma[i - radius * width]
                        + luma[i + radius * width])
                        * 0.25)
                    .abs();
                count += 1;
            }
        }
        detail.push(if count == 0 { 0.0 } else { sum / count as f32 });
    }
    let tile_ev = divide(&stops, &counts);
    let center_mean = mean(&tile_ev);
    let uniformity = tile_ev.iter().map(|v| *v - center_mean).collect();
    let normal = |v: &[f32]| v.iter().map(|x| *x / n as f32 * 100.0).collect::<Vec<_>>();
    let mut output = vec![
        diagnostic(("rgb-parade", "RGB parade", "Red, green and blue signals from left to right; curve-domain Rec.2020."), "density", "samples", (96, 48), parade, &[], n),
        diagnostic(("luma-waveform", "Luminance waveform", "Horizontal photo position versus curve-domain luminance, black below and white above."), "density", "samples", (96, 48), waveform, &[], n),
        diagnostic(("chroma-vectorscope", "Chroma vectorscope", "Blue minus green horizontally and red minus green vertically; neutral at centre."), "density", "samples", (48, 48), vector, &[], n),
        diagnostic(("exposure-map", "Exposure zone map", "Average exposure relative to 18% working gray in each photo region."), "map", "EV", (GW, GH), tile_ev, &[], n),
        diagnostic(("range-map", "Working range excursions", "Share of regional pixels with a channel at or below zero or at or above nominal linear white; not output clipping."), "map", "%", (GW, GH), divide(&clip, &counts), &[], n),
        diagnostic(("focus-map", "Spatial edge detail", "Average luminance gradient locates detail and soft regions; inspect subject detail before sharpening."), "map", "signal", (GW, GH), divide(&focus, &counts), &[], n),
        diagnostic(("noise-map", "Flat-area residual", "High-frequency residual in low-gradient areas. Texture can contribute; this is not a sensor noise measurement."), "map", "signal", (GW, GH), divide(&residual, &noise_count), &[], noise_count.iter().sum()),
        diagnostic(("neutral-cast", "Near-neutral colour balance", "Mean RGB gain in low-chroma midtones; one is balanced. Coloured objects can bias this sample."), "bars", "ratio", (3, 1), if neutral_n == 0 { vec![0.0; 3] } else { neutral.map(|v| v / neutral_n as f32).to_vec() }, &["Red", "Green", "Blue"], neutral_n),
        diagnostic(("local-contrast", "Local contrast map", "Regional luminance standard deviation divided by mean; high values indicate tonal structure."), "map", "ratio", (GW, GH), contrast, &[], n),
        diagnostic(("dynamic-range", "Rendered tonal range", "Luminance percentiles in stops from 18% gray; span measures the 1st to 99th percentile, not sensor dynamic range."), "bars", "EV", (6, 1), dynamic, &["1%", "5%", "50%", "95%", "99%", "Span"], n),
        diagnostic(("exposure-zones", "Exposure zone distribution", "Photo area in one-stop bands relative to 18% gray; outer bands include the tails."), "bars", "%", (15, 1), normal(&zones), &["<-6", "-6", "-5", "-4", "-3", "-2", "-1", "0", "+1", "+2", "+3", "+4", "+5", "+6", ">=7"], n),
        diagnostic(("srgb-occupancy", "sRGB working gamut occupancy", "Working colours mapped to linear sRGB before display tone mapping: negative channels take precedence over above-white channels."), "bars", "%", (3, 1), normal(&gamut), &["In range", "Negative", "Above white"], n),
        diagnostic(("stripe-profile", "Row and column banding profile", "Second differences of row means followed by column means; spikes locate possible stripes. Scene structure can also produce spikes."), "bars", "signal", (64, 1), bands, &[], n),
        diagnostic(("block-period", "Eight-pixel block boundary profile", "Mean adjacent luminance differences by modulo-eight boundary position; a recurring spike can indicate compression blocks at this preview scale."), "bars", "signal", (8, 1), divide(&block_sum, &block_n), &["0", "1", "2", "3", "4", "5", "6", "7"], block_n.iter().sum()),
        diagnostic(("edge-directions", "Edge direction balance", "Gradient energy across twelve directions; use for composition and directional detail review."), "bars", "energy", (12, 1), orientation, &["-180", "-150", "-120", "-90", "-60", "-30", "0", "30", "60", "90", "120", "150"], n),
        diagnostic(("illumination-map", "Relative illumination map", "Regional exposure offset from the frame average. Subject reflectance and lighting both affect this reading."), "map", "EV", (GW, GH), uniformity, &[], n),
        diagnostic(("edge-registration", "Neutral-edge channel alignment", "Mean horizontal red and blue edge centroid offsets against green on near-neutral edges. A proxy diagnostic, not a lens calibration."), "bars", "px", (2, 1), if registration_n == 0 { vec![0.0; 2] } else { registration.map(|v| v / registration_n as f32).to_vec() }, &["Red–green", "Blue–green"], registration_n),
        diagnostic(("channel-correlation", "Channel correlation", "Pearson correlation of channel pairs; zero also represents an undefined constant-channel correlation."), "bars", "ratio", (3, 1), vec![correlation(&pixels, 0, 1), correlation(&pixels, 0, 2), correlation(&pixels, 1, 2)], &["R/G", "R/B", "G/B"], n),
        diagnostic(("detail-scales", "Detail scale spectrum", "Average luminance residual at five neighbourhood radii in preview pixels; compare fine and broad structure."), "bars", "signal", (5, 1), detail, &["1 px", "2 px", "4 px", "8 px", "16 px"], n),
        diagnostic(("highlight-chroma", "Highlight chroma retention", "Mean relative chroma in shadows, midtones and highlights. A low highlight value may also reflect naturally neutral highlights."), "bars", "%", (3, 1), divide(&chroma, &chroma_n), &["Shadows", "Midtones", "Highlights"], n),
    ];
    output.extend(quality::measure(&pixels, &luma, width, height));
    output.extend(delivery::measure(&pixels, &luma, width, height));
    output.extend(advanced::measure(&pixels, &luma, width, height));
    // Restrict wire precision and prevent numerical noise from becoming a nonfinite payload.
    for tool in &mut output {
        for value in &mut tool.values {
            *value = (*value * 100_000.0).round() / 100_000.0;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn find<'a>(tools: &'a [Diagnostic], id: &str) -> &'a Diagnostic {
        tools.iter().find(|v| v.id == id).expect("diagnostic")
    }
    fn image(w: usize, h: usize, pixel: impl Fn(usize, usize) -> [f32; 3]) -> Vec<f32> {
        (0..h)
            .flat_map(|y| {
                (0..w).flat_map({
                    let pixel = &pixel;
                    move |x| pixel(x, y)
                })
            })
            .collect()
    }
    #[test]
    fn neutral_frame_has_seventy_distinct_finite_bounded_reports() {
        let rgb = vec![0.18; 64 * 48 * 3];
        let original = rgb.clone();
        let tools = measure(&rgb, 64, 48).expect("report");
        assert_eq!(tools.len(), 95);
        assert_eq!(rgb, original);
        let mut names: Vec<_> = tools.iter().map(|t| &t.id).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 95);
        for t in &tools {
            assert_eq!(t.values.len(), t.width * t.height);
            assert!(t.values.len() <= 96 * 48);
            assert!(t.values.iter().all(|v| v.is_finite()));
        }
        assert!(find(&tools, "exposure-map")
            .values
            .iter()
            .all(|v| v.abs() < 0.001));
        assert!(find(&tools, "noise-map")
            .values
            .iter()
            .all(|v| v.abs() < 0.0001));
        assert_eq!(find(&tools, "neutral-cast").values, vec![1.0; 3]);
        assert_eq!(find(&tools, "range-map").values, vec![0.0; CELLS]);
        assert!(serde_json::to_vec(&tools).expect("payload").len() < 150_000);
    }
    #[test]
    fn exposure_and_correlation_follow_a_known_ramp() {
        let tools = measure(
            &image(96, 64, |x, _| {
                let v = 0.01 + x as f32 / 96.0;
                [v; 3]
            }),
            96,
            64,
        )
        .expect("ramp");
        let exposure = &find(&tools, "exposure-map").values;
        assert!(exposure[15] > exposure[0]);
        assert!(find(&tools, "channel-correlation")
            .values
            .iter()
            .all(|v| *v > 0.999));
        let dynamic = &find(&tools, "dynamic-range").values;
        assert!(dynamic[5] > 4.0);
        assert!((find(&tools, "exposure-zones").values.iter().sum::<f32>() - 100.0).abs() < 0.001);
    }
    #[test]
    fn noisy_flat_regions_and_periodic_blocks_are_measurable() {
        let noisy = image(96, 64, |x, y| {
            let v = 0.3
                + if (x * 13 + y * 7) % 5 < 2 {
                    0.005
                } else {
                    -0.005
                };
            [v; 3]
        });
        let tools = measure(&noisy, 96, 64).expect("noise");
        assert!(mean(&find(&tools, "noise-map").values) > 0.001);
        let blocks = image(96, 64, |x, _| [if (x / 8) % 2 == 0 { 0.2 } else { 0.4 }; 3]);
        let tools = measure(&blocks, 96, 64).expect("blocks");
        let values = &find(&tools, "block-period").values;
        assert!(values[0] > 0.05);
        assert!(values[1..].iter().all(|v| *v < 0.001));
    }
    #[test]
    fn rejects_bad_pixels_and_handles_tiny_or_black_photos() {
        assert!(measure(&[], 0, 0).is_err());
        assert!(measure(&[f32::NAN; 3], 1, 1).is_err());
        assert!(measure(&[0.0; 3], 513, 1).is_err());
        assert!(measure(&[0.0; 2], 1, 1).is_err());
        for t in measure(&[0.0; 3], 1, 1).expect("black") {
            assert!(t.values.iter().all(|v| v.is_finite()));
        }
    }
}
