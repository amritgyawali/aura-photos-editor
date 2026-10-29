//! Batch 1 of the photo pass: capture, light and colour faults (steps 1-25). ADR-0186.
//!
//! Every function reads the original's preview and returns a [`Finding`]; none writes a recipe.
//! Each acts only on what it measured, and most compare the photograph with itself - shadows
//! against its own midtones, a corner against its own centre - so a deliberate look, a warm room
//! or a dark scene is not mistaken for a fault. People are kept out of every step that would
//! change texture, by the scene's person zone.
// Planes are `w * h` long and every index is derived from a position inside them; statistics
// convert freely between pixel counts and f32.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

use super::scene::{blobs, grid_share, hue, opponent, saturation, Blob, Scene};
use super::{op_id, Change, Finding, StepDef, STEPS};
use crate::advanced_retouch::{base_edit, measure};
use aura_recipe::retouch_tools::{ColorRange, Edit, Gradient, LuminanceRange, Selection, Tool};
use aura_recipe::Recipe;

/// Plan every step of batch 1, in catalogue order.
#[must_use]
pub fn plan(s: &Scene<'_>, base: &Recipe) -> Vec<Finding> {
    if s.w < 64 || s.h < 64 {
        return STEPS
            .iter()
            .map(|_| Finding::not_applicable("The preview is too small to measure."))
            .collect();
    }
    let noise = Noise::measure(s);
    let edges = edge_width(s);
    let level = horizon(s, base, &STEPS[18]);
    let levelled = level.outcome == super::Outcome::Applied;
    vec![
        sensor_dust(s, &STEPS[0]),
        clipped_channels(s, &STEPS[1]),
        colour_noise(&noise, base),
        shadow_noise(&noise, base, &STEPS[3]),
        smooth_noise(s, &noise, &STEPS[4]),
        jpeg_compression(s, &STEPS[5]),
        banding(s, &STEPS[6]),
        moire(s, &STEPS[7]),
        vignetting(s, base),
        mixed_light(s),
        shadow_tint(s, base),
        shadow_cast(s, base),
        highlight_tint(s, base),
        regional_haze(s, base, &STEPS[13]),
        capture_sharpening(s, base, edges),
        camera_shake(s, edges),
        green_fringes(s, base, &STEPS[16]),
        saturated_lights(s, &STEPS[17]),
        level,
        pale_sky(s, &STEPS[19]),
        scene_texture(s, &noise, base),
        hard_corners(s, base, levelled),
        dark_foreground(s, base, &STEPS[22]),
        sharpen_mask(s, &noise, base),
        flicker(s),
    ]
}

/// A whole-frame operation: what "Select entire photo" selects, corners included.
fn whole(def: &StepDef, n: usize, tool: Tool, amount: f32) -> Edit {
    let mut e = base_edit(op_id(def, n), tool, [0.5, 0.5, 1.0, 1.0], amount);
    e.feather = 0.0;
    e
}

fn on(def: &StepDef, n: usize, tool: Tool, region: [f32; 4], amount: f32) -> Edit {
    base_edit(op_id(def, n), tool, region, amount)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn percent(v: f32) -> String {
    format!("{:.1}%", v * 100.0)
}

fn decode(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn median(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mid = values.len() / 2;
    values.select_nth_unstable_by(mid, f32::total_cmp);
    Some(values[mid])
}

// ---- Noise, measured once --------------------------------------------------------------------

/// Noise in display-encoded units, as a robust standard deviation of each pixel against its own
/// 3 x 3 neighbourhood on plain areas: a smooth gradient does not count, an edge is an outlier.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Noise {
    /// Luminance noise on plain midtones.
    pub midtones: f32,
    /// Luminance noise on plain shadows.
    pub shadows: f32,
    /// Colour noise (green-magenta and blue-yellow together) on plain midtones.
    pub colour: f32,
    /// Luminance noise on plain bright areas: sky, backdrop, wall.
    pub smooth: f32,
    pub midtone_samples: usize,
    pub shadow_samples: usize,
    pub smooth_samples: usize,
    /// Share of the frame that is plain shadow.
    pub shadow_share: f32,
}

impl Noise {
    #[must_use]
    pub fn measure(s: &Scene<'_>) -> Self {
        let (w, h) = (s.w, s.h);
        let mut gm = Vec::with_capacity(s.len());
        let mut by = Vec::with_capacity(s.len());
        for i in 0..s.len() {
            let [_, a, b] = opponent(s.rgb(i));
            gm.push(a);
            by.push(b);
        }
        let l3 = measure::box_mean(&s.luma, w, h, 1);
        let gm3 = measure::box_mean(&gm, w, h, 1);
        let by3 = measure::box_mean(&by, w, h, 1);
        let (mut mid, mut shadow, mut colour, mut smooth) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        // A pixel against the mean of nine including itself keeps 8/9 of its deviation.
        let k = 9.0_f32 / 8.0;
        for y in (1..h - 1).step_by(2) {
            for x in (1..w - 1).step_by(2) {
                let i = y * w + x;
                if s.wide_std[i] > 0.06 {
                    continue;
                }
                let l = s.luma[i];
                let d = (l - l3[i]).abs() * k;
                if (0.03..0.2).contains(&l) {
                    shadow.push(d);
                } else if (0.3..0.75).contains(&l) {
                    mid.push(d);
                    colour.push((gm[i] - gm3[i]).hypot(by[i] - by3[i]) * k);
                }
                if l >= 0.4 && s.wide_std[i] < 0.025 && s.people[i] < 0.5 {
                    smooth.push(d);
                }
            }
        }
        // The median absolute deviation of a normal distribution is 0.6745 of its sigma.
        let sigma = |v: &mut Vec<f32>| median(v).map_or(0.0, |m| m / 0.6745);
        let sampled = (w / 2) * (h / 2);
        Self {
            midtone_samples: mid.len(),
            shadow_samples: shadow.len(),
            smooth_samples: smooth.len(),
            shadow_share: shadow.len() as f32 / sampled.max(1) as f32,
            midtones: sigma(&mut mid),
            shadows: sigma(&mut shadow),
            // Two chroma components: the median of their joint magnitude is about 1.18 sigma.
            colour: median(&mut colour).map_or(0.0, |m| m / 1.1774),
            smooth: sigma(&mut smooth),
        }
    }
}

// ---- 1. Sensor dust --------------------------------------------------------------------------

/// Below this f-number a speck on the sensor is too far out of focus to see.
const DUST_APERTURE: f32 = 5.6;

fn sensor_dust(s: &Scene<'_>, def: &StepDef) -> Finding {
    match s.capture.aperture {
        Some(f) if f < DUST_APERTURE - 0.05 => {
            return Finding::not_applicable(format!(
                "Shot at f/{f:.1}: dust on the sensor is too far out of focus to show below f/{DUST_APERTURE}."
            ));
        }
        None => {
            return Finding::not_applicable(
                "The aperture is not recorded; dust is only searched for when it is known to be narrow enough to show.",
            );
        }
        Some(_) => {}
    }
    let region: Vec<f32> = (0..s.len())
        .map(|i| {
            f32::from(u8::from(
                s.people[i] < 0.5 && s.luma[i] >= 0.35 && s.wide_std[i] < 0.03,
            ))
        })
        .collect();
    let share = region.iter().sum::<f32>() / s.len() as f32;
    if share < 0.04 {
        return Finding::not_applicable(
            "No plain bright area (sky, backdrop, wall) large enough to show sensor dust.",
        );
    }
    let search = measure::marks(&s.linear(), &region, Some(&s.people), 0.00004, 16);
    let searched = format!(
        "Searched {} of the frame that is plain and bright",
        percent(share)
    );
    if search.pattern {
        return Finding::checked(format!(
            "{searched}: {}+ small irregularities are the surface's own texture, so none is dust.",
            search.found
        ));
    }
    // Dust is a faint shadow on the sensor; a strong mark on a plain sky is a bird or a kite.
    let faint: Vec<_> = search
        .repairs
        .iter()
        .filter(|r| r.departure <= 0.35 && r.kind != "thread")
        .collect();
    if faint.is_empty() {
        return Finding::checked(format!(
            "{searched}: {} found, none a faint round speck.",
            plural(search.found, "candidate", "candidates")
        ));
    }
    let ops = faint
        .iter()
        .enumerate()
        .map(|(n, rep)| {
            let mut e = on(
                def,
                n,
                Tool::Heal,
                rep.region,
                measure::repair_amount(rep.departure),
            );
            e.source = Some(rep.source);
            e.feather = 0.6;
            e
        })
        .collect::<Vec<_>>();
    Finding::applied(
        format!(
            "{searched}: {}.",
            plural(ops.len(), "speck of dust", "specks of dust")
        ),
        format!(
            "Healed {} from clean surface beside {}.",
            plural(ops.len(), "speck", "specks"),
            if ops.len() == 1 { "it" } else { "each" }
        ),
        Vec::new(),
        ops,
    )
}

// ---- 2. Clipped colour channels ---------------------------------------------------------------

/// One or two channels at the top and another well below it: the hue has been pushed toward
/// the next primary (an orange face under a red stage light turning yellow).
fn channel_clipped(p: [f32; 3]) -> bool {
    let max = p[0].max(p[1]).max(p[2]);
    let min = p[0].min(p[1]).min(p[2]);
    max >= 0.985 && min <= 0.85
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d)
}

fn mean_hue(hues: &[f32]) -> Option<f32> {
    if hues.is_empty() {
        return None;
    }
    let (sx, sy) = hues.iter().fold((0.0_f32, 0.0_f32), |(x, y), h| {
        let a = h * std::f32::consts::TAU;
        (x + a.cos(), y + a.sin())
    });
    Some((sy.atan2(sx) / std::f32::consts::TAU).rem_euclid(1.0))
}

fn clipped_channels(s: &Scene<'_>, def: &StepDef) -> Finding {
    const CELL: usize = 16;
    let (share, gw, gh) = grid_share(s.w, s.h, CELL, |i| channel_clipped(s.rgb(i)));
    let marked: Vec<bool> = share.iter().map(|v| *v >= 0.3).collect();
    let least = 0.0003 * s.len() as f32;
    let mut ops = Vec::new();
    let (mut found, mut own_colour) = (0, 0);
    for blob in blobs(&marked, gw, gh) {
        if (blob.cells * CELL * CELL) as f32 * 0.3 < least || ops.len() >= 8 {
            continue;
        }
        found += 1;
        // The hue inside against the hue of the unclipped rim around it.
        let (mut inside, mut rim) = (Vec::new(), Vec::new());
        let x0 = blob.x0.saturating_sub(1) * CELL;
        let y0 = blob.y0.saturating_sub(1) * CELL;
        let x1 = ((blob.x1 + 1) * CELL).min(s.w);
        let y1 = ((blob.y1 + 1) * CELL).min(s.h);
        for y in (y0..y1).step_by(2) {
            for x in (x0..x1).step_by(2) {
                let p = s.px.encoded(x, y);
                let max = p[0].max(p[1]).max(p[2]);
                if let Some(h) = hue(p).filter(|_| saturation(p) > 0.08) {
                    if channel_clipped(p) {
                        inside.push(h);
                    } else if (0.55..0.9).contains(&max) {
                        rim.push(h);
                    }
                }
            }
        }
        let shifted = match (mean_hue(&inside), mean_hue(&rim)) {
            (Some(a), Some(b)) if rim.len() >= 20 => hue_distance(a, b) >= 0.03,
            _ => false,
        };
        if !shifted {
            own_colour += 1;
            continue;
        }
        let mut e = on(
            def,
            ops.len(),
            Tool::ClippedHighlightRepair,
            blob.ellipse(gw, gh, 1.0),
            0.8,
        );
        e.radius = 0.01;
        ops.push(e);
    }
    if ops.is_empty() {
        return Finding::checked(if found == 0 {
            "No area where one colour channel is burnt out and the others are not.".to_owned()
        } else {
            format!(
                "{} with a burnt-out channel, all the same colour as their surroundings: the colour is the object's.",
                plural(found, "area", "areas")
            )
        });
    }
    Finding::applied(
        format!(
            "{} where a burnt-out channel has shifted the hue away from the colour around it{}.",
            plural(ops.len(), "area", "areas"),
            if own_colour > 0 {
                format!(" ({own_colour} more keep their own colour)")
            } else {
                String::new()
            }
        ),
        "Each area takes the colour of its unclipped surroundings at its own brightness.",
        Vec::new(),
        ops,
    )
}

// ---- 3-5. Noise ---------------------------------------------------------------------------------

fn colour_noise(n: &Noise, base: &Recipe) -> Finding {
    if n.midtone_samples < 4000 {
        return Finding::not_applicable("Too few plain midtones to measure colour noise on.");
    }
    let current = base.global.noise.colour;
    // Calibrated on the preview: clean files read under 0.004, high-ISO ones 0.01 and more.
    let target = ((n.colour - 0.003) * 4000.0).round().clamp(0.0, 60.0) as i16;
    let reading = format!(
        "Colour noise {:.4} against luminance noise {:.4} on plain midtones.",
        n.colour, n.midtones
    );
    if target <= current + 4 {
        return Finding::checked(format!(
            "{reading} Colour noise reduction {current} already covers it."
        ));
    }
    Finding::applied(
        reading,
        format!("Colour noise reduction {current} -> {target}."),
        vec![Change::ColourNoise(target)],
        Vec::new(),
    )
}

/// How much the saved edit brightens the shadows: exposure, and the Shadows slider's lift.
fn shadow_gain(base: &Recipe) -> f32 {
    let g = &base.global;
    2.0_f32.powf(g.exposure) * (1.0 + f32::from(g.shadows.max(0)) / 100.0)
}

fn shadow_noise(n: &Noise, base: &Recipe, def: &StepDef) -> Finding {
    if n.shadow_share < 0.04 || n.shadow_samples < 2000 {
        return Finding::not_applicable("Not enough plain shadow for its noise to show.");
    }
    let gain = shadow_gain(base);
    let lifted = n.shadows * gain;
    let reading = format!(
        "Shadow noise {:.4} (x{gain:.2} after the edit's lift) against midtone noise {:.4}.",
        n.shadows, n.midtones
    );
    if lifted < 0.012 || n.shadows < 1.4 * n.midtones {
        return Finding::checked(format!("{reading} The shadows are as clean as the rest."));
    }
    let mut e = whole(def, 0, Tool::LocalDenoise, 0.7);
    // Luminance selections are evaluated after the edit's exposure: the shadows, by their own
    // brightness, up to about encoded 0.2.
    let ceiling = (decode(0.2) * 2.0_f32.powf(base.global.exposure) / 0.18).log2();
    e.selection = Some(Selection {
        inverted: false,
        gradient: None,
        luminance: Some(LuminanceRange {
            low: -16.0,
            high: ceiling,
            softness: 1.0,
        }),
        color: None,
    });
    e.tone = ((lifted - 0.008) / 0.03).clamp(0.1, 0.8);
    Finding::applied(
        reading,
        "Shadows denoised on their own, edges kept; midtones and highlights untouched.",
        Vec::new(),
        vec![e],
    )
}

fn smooth_noise(s: &Scene<'_>, n: &Noise, def: &StepDef) -> Finding {
    const CELL: usize = 32;
    if n.smooth_samples < 3000 {
        return Finding::not_applicable("No plain bright area (sky, backdrop) large enough.");
    }
    let reading = format!("Noise {:.4} on plain bright areas.", n.smooth);
    if n.smooth < 0.006 {
        return Finding::checked(format!("{reading} Clean."));
    }
    let (share, gw, gh) = grid_share(s.w, s.h, CELL, |i| {
        s.people[i] < 0.5 && s.luma[i] >= 0.4 && s.wide_std[i] < 0.025
    });
    let marked: Vec<bool> = share.iter().map(|v| *v >= 0.6).collect();
    let areas: Vec<Blob> = blobs(&marked, gw, gh)
        .into_iter()
        .filter(|b| b.cells >= 6)
        .take(3)
        .collect();
    if areas.is_empty() {
        return Finding::checked(format!(
            "{reading} No plain area is large enough to show it."
        ));
    }
    let ops = areas
        .iter()
        .enumerate()
        .map(|(k, b)| {
            let mut e = on(def, k, Tool::LocalDenoise, b.ellipse(gw, gh, 0.5), 0.75);
            e.tone = ((n.smooth - 0.004) / 0.02).clamp(0.1, 0.7);
            e
        })
        .collect::<Vec<_>>();
    Finding::applied(
        reading,
        format!(
            "Denoised {} with their edges kept.",
            plural(ops.len(), "plain area", "plain areas")
        ),
        Vec::new(),
        ops,
    )
}

// ---- 6. JPEG compression ------------------------------------------------------------------------

/// The IJG standard luminance quantisation table; order does not matter, only its sum.
const STANDARD_LUMA: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

/// The IJG quality a JPEG's luminance table corresponds to, read from the file's own header.
#[must_use]
pub fn jpeg_quality(bytes: &[u8]) -> Option<u8> {
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            return None;
        }
        let marker = bytes[i + 1];
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
        let body = bytes.get(i + 4..i + 2 + len)?;
        if marker == 0xDB {
            let mut j = 0;
            while j < body.len() {
                let (precision, table) = (body[j] >> 4, body[j] & 0x0F);
                let size = if precision == 0 { 64 } else { 128 };
                let values = body.get(j + 1..j + 1 + size)?;
                if table == 0 {
                    let sum: u32 = if precision == 0 {
                        values.iter().map(|v| u32::from(*v)).sum()
                    } else {
                        values
                            .chunks_exact(2)
                            .map(|c| u32::from(u16::from_be_bytes([c[0], c[1]])))
                            .sum()
                    };
                    let standard: u32 = STANDARD_LUMA.iter().map(|v| u32::from(*v)).sum();
                    let scale = sum as f32 * 100.0 / standard as f32;
                    let quality = if scale <= 100.0 {
                        (200.0 - scale) / 2.0
                    } else {
                        5000.0 / scale
                    };
                    return Some(quality.round().clamp(1.0, 100.0) as u8);
                }
                j += 1 + size;
            }
        }
        i += 2 + len;
    }
    None
}

/// [`jpeg_quality`] of a file on disk, reading only its header.
#[must_use]
pub fn jpeg_quality_of_file(path: &std::path::Path) -> Option<u8> {
    use std::io::Read;
    let mut head = Vec::with_capacity(1 << 18);
    std::fs::File::open(path)
        .ok()?
        .take(1 << 18)
        .read_to_end(&mut head)
        .ok()?;
    jpeg_quality(&head)
}

fn jpeg_compression(s: &Scene<'_>, def: &StepDef) -> Finding {
    if s.capture.kind.as_deref() != Some("jpeg") {
        return Finding::not_applicable("Not a JPEG file, so it was never compressed into blocks.");
    }
    let Some(q) = s.capture.jpeg_quality else {
        return Finding::checked(
            "A JPEG whose quantisation tables could not be read; left as it is.",
        );
    };
    if q >= 85 {
        return Finding::checked(format!(
            "JPEG quality {q}: compression blocks are not visible."
        ));
    }
    let mut e = whole(def, 0, Tool::JpegCleanup, 0.8);
    e.tone = (f32::from(85 - q) / 60.0).clamp(0.1, 1.0);
    Finding::applied(
        format!("JPEG quality {q}, read from the file's own quantisation tables."),
        "Block edges smoothed where they are steps of compression, not edges in the picture.",
        Vec::new(),
        vec![e],
    )
}

// ---- 7. Banding --------------------------------------------------------------------------------

fn banding(s: &Scene<'_>, def: &StepDef) -> Finding {
    const CELL: usize = 32;
    if !s.capture.eight_bit() {
        return Finding::not_applicable(
            "A camera raw file is developed at more than eight bits, so its gradients cannot band.",
        );
    }
    let smooth = |i: usize| s.people[i] < 0.5 && s.wide_std[i] < 0.03 && s.fine_std[i] < 0.006;
    let code = |i: usize| (s.luma[i] * 255.0).round() as i32;
    let (mut steps, mut length) = (0_usize, 0_usize);
    for y in (0..s.h).step_by(4) {
        let row = y * s.w;
        let mut x = 0;
        // Runs of one code value, and what separates each from the next.
        let mut previous: Option<(i32, usize)> = None;
        while x < s.w {
            if !smooth(row + x) {
                previous = None;
                x += 1;
                continue;
            }
            let c = code(row + x);
            let start = x;
            while x < s.w && smooth(row + x) && code(row + x) == c {
                x += 1;
            }
            let run = x - start;
            length += run;
            if let Some((pc, prun)) = previous {
                if prun >= 5 && run >= 5 && (1..=2).contains(&(c - pc).abs()) {
                    steps += 1;
                }
            }
            previous = Some((c, run));
        }
    }
    let density = steps as f32 / (length as f32 / 100.0).max(1.0);
    let reading = format!(
        "{} between flat runs over {length} px of smooth gradient ({density:.2} per 100 px).",
        plural(steps, "one-level step", "one-level steps")
    );
    if steps < 200 || density < 1.0 {
        return Finding::checked(format!("{reading} No visible banding."));
    }
    let (share, gw, gh) = grid_share(s.w, s.h, CELL, smooth);
    let marked: Vec<bool> = share.iter().map(|v| *v >= 0.5).collect();
    let Some(area) = blobs(&marked, gw, gh).into_iter().next() else {
        return Finding::checked(format!("{reading} No single area is large enough."));
    };
    let mut e = on(def, 0, Tool::Deband, area.ellipse(gw, gh, 1.0), 0.8);
    e.radius = 0.01;
    Finding::applied(
        reading,
        "The banded gradient smoothed with a fine dither; edges kept.",
        Vec::new(),
        vec![e],
    )
}

// ---- 8. Moiré ----------------------------------------------------------------------------------

fn moire(s: &Scene<'_>, def: &StepDef) -> Finding {
    const CELL: usize = 24;
    let (w, h) = (s.w, s.h);
    let (gw, gh) = (w.div_ceil(CELL), h.div_ceil(CELL));
    let in_face = |x: usize, y: usize| {
        let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
        s.faces.iter().any(|f| {
            let [l, t, r, b] = f.bounds;
            u >= l && u <= r && v >= t && v <= b
        })
    };
    // Clothes: below a face and within a shoulder's width of it. On held-out photographs every
    // area this found anywhere else was real fine colour - pinwheels, flowers, windmill sails,
    // hair against a window - and fabric is where a camera's moiré forms.
    let on_clothes = |x: usize, y: usize| {
        let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
        s.faces.iter().any(|f| {
            let [l, t, r, b] = f.bounds;
            let fw = r - l;
            u >= l - 1.2 * fw && u <= r + 1.2 * fw && v > b + 0.1 * (b - t)
        })
    };
    if s.no_people() {
        return Finding::not_applicable("Nobody in the frame: moiré is looked for on clothes.");
    }
    let mut marked = vec![false; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let (x0, y0) = (gx * CELL + 1, gy * CELL + 1);
            let (x1, y1) = (((gx + 1) * CELL).min(w - 1), ((gy + 1) * CELL).min(h - 1));
            let (cx, cy) = (x0 + CELL / 2, y0 + CELL / 2);
            if x1 <= x0 + 4 || y1 <= y0 + 4 || in_face(cx, cy) || !on_clothes(cx, cy) {
                continue;
            }
            let (mut chroma, mut luma, mut sat, mut flips, mut pairs, mut n) =
                (0.0_f32, 0.0_f32, 0.0_f32, 0_usize, 0_usize, 0_usize);
            for y in y0..y1 {
                let mut last: Option<f32> = None;
                for x in x0..x1 {
                    let o = opponent(s.px.encoded(x, y));
                    let near = [
                        opponent(s.px.encoded(x - 1, y)),
                        opponent(s.px.encoded(x + 1, y)),
                        opponent(s.px.encoded(x, y - 1)),
                        opponent(s.px.encoded(x, y + 1)),
                    ];
                    let m = |c: usize| near.iter().map(|p| p[c]).sum::<f32>() / 4.0;
                    let dby = o[2] - m(2);
                    chroma += (o[1] - m(1)).hypot(dby);
                    luma += (o[0] - m(0)).abs();
                    sat += saturation(s.px.encoded(x, y));
                    n += 1;
                    if let Some(prev) = last {
                        if prev.abs() > 0.004 && dby.abs() > 0.004 {
                            pairs += 1;
                            if prev.signum() != dby.signum() {
                                flips += 1;
                            }
                        }
                    }
                    last = Some(dby);
                }
            }
            let n = n.max(1) as f32;
            let (chroma, luma, sat) = (chroma / n, luma / n, sat / n);
            let alternating = pairs > 40 && flips as f32 / pairs as f32 >= 0.6;
            marked[gy * gw + gx] =
                chroma >= 0.02 && chroma >= 1.3 * luma && alternating && sat < 0.35;
        }
    }
    let areas: Vec<Blob> = blobs(&marked, gw, gh)
        .into_iter()
        .filter(|b| b.cells >= 3)
        .take(4)
        .collect();
    if areas.is_empty() {
        return Finding::checked(
            "No clothing where colour alternates at the finest scale while brightness does not.",
        );
    }
    let ops = areas
        .iter()
        .enumerate()
        .map(|(k, b)| on(def, k, Tool::FabricMoire, b.ellipse(gw, gh, 0.5), 0.7))
        .collect::<Vec<_>>();
    Finding::applied(
        format!(
            "False colour that alternates pixel to pixel without a matching brightness pattern in {}.",
            plural(ops.len(), "area", "areas")
        ),
        "The false colour evened out; the pattern's brightness kept.",
        Vec::new(),
        ops,
    )
}

// ---- 9. Lens vignetting ------------------------------------------------------------------------

fn vignetting(s: &Scene<'_>, base: &Recipe) -> Finding {
    if base.lens.vignette != 0 || base.lens.profile.is_some() {
        return Finding::not_applicable(
            "A lens profile or a vignette setting already corrects the corners.",
        );
    }
    let (cx, cy) = (s.w as f32 / 2.0, s.h as f32 / 2.0);
    let max_r = cx.hypot(cy);
    let (mut centre, mut corners) = (Vec::new(), [Vec::new(), Vec::new(), Vec::new(), Vec::new()]);
    for y in (0..s.h).step_by(3) {
        for x in (0..s.w).step_by(3) {
            let i = y * s.w + x;
            let l = s.luma[i];
            if s.people[i] > 0.5 || s.wide_std[i] > 0.04 || !(0.06..0.95).contains(&l) {
                continue;
            }
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let r = dx.hypot(dy) / max_r;
            if r < 0.4 {
                centre.push(decode(l));
            } else if r > 0.8 {
                let q = usize::from(dx > 0.0) + 2 * usize::from(dy > 0.0);
                corners[q].push(decode(l));
            }
        }
    }
    if centre.len() < 400 || corners.iter().any(|c| c.len() < 150) {
        return Finding::not_applicable(
            "Not enough plain area in the centre and all four corners to measure fall-off.",
        );
    }
    let Some(mid) = median(&mut centre) else {
        return Finding::not_applicable("No centre reading.");
    };
    let ratios: Vec<f32> = corners
        .iter_mut()
        .filter_map(|c| median(c))
        .map(|v| v / mid.max(1e-4))
        .collect();
    let (lo, hi) = ratios
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), r| (a.min(*r), b.max(*r)));
    let mean = ratios.iter().sum::<f32>() / ratios.len().max(1) as f32;
    let reading = format!(
        "Plain corners at {} of the centre's brightness (from {} to {}).",
        percent(mean),
        percent(lo),
        percent(hi)
    );
    // Shared by all four corners and darker by at least a quarter stop: the lens, not the scene.
    if hi > 0.84 || hi / lo.max(1e-4) > 1.25 {
        return Finding::checked(format!("{reading} No fall-off all four corners share."));
    }
    // The renderer multiplies by 1 + 0.6 a r^2: half the measured fall-off is corrected.
    let full = (1.0 / mean - 1.0) / 0.6;
    let amount = (full * 50.0).round().clamp(5.0, 60.0) as i16;
    Finding::applied(
        reading,
        format!("Lens vignette correction {amount}, half of the measured fall-off."),
        vec![Change::LensVignette(amount)],
        Vec::new(),
    )
}

// ---- 10. Mixed light --------------------------------------------------------------------------

fn mixed_light(s: &Scene<'_>) -> Finding {
    const GRID: usize = 8;
    let (cw, ch) = (s.w.div_ceil(GRID), s.h.div_ceil(GRID));
    let mut tiles: Vec<Vec<(f32, usize)>> = vec![Vec::new(); GRID * GRID];
    for y in (0..s.h).step_by(2) {
        for x in (0..s.w).step_by(2) {
            let i = y * s.w + x;
            let p = s.rgb(i);
            let l = s.luma[i];
            if s.people[i] > 0.5 || !(0.3..0.92).contains(&l) || saturation(p) > 0.18 {
                continue;
            }
            // Open sky is blue because it is sky, not because a surface is lit blue.
            if s.wide_std[i] < 0.03 && p[2] > p[0] && l > 0.45 {
                continue;
            }
            let (r, b) = (decode(p[0]).max(1e-4), decode(p[2]).max(1e-4));
            tiles[(y / ch) * GRID + x / cw].push(((b / r).ln(), i));
        }
    }
    let mut cells: Vec<(usize, f32)> = tiles
        .iter_mut()
        .enumerate()
        .filter(|(_, t)| t.len() >= 40)
        .filter_map(|(k, t)| {
            let mut v: Vec<f32> = t.iter().map(|p| p.0).collect();
            median(&mut v).map(|m| (k, m))
        })
        .collect();
    if cells.len() < 16 {
        return Finding::not_applicable(
            "Too few neutral surfaces to compare the light across the frame.",
        );
    }
    cells.sort_by(|a, b| a.1.total_cmp(&b.1));
    // The split of the sorted readings into two groups with the least spread inside each.
    let values: Vec<f32> = cells.iter().map(|c| c.1).collect();
    let spread = |v: &[f32]| {
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
    };
    let mut best = None;
    for k in 3..values.len() - 3 {
        let (a, b) = values.split_at(k);
        let cost = spread(a) * a.len() as f32 + spread(b) * b.len() as f32;
        if best.is_none_or(|(c, _)| cost < c) {
            best = Some((cost, k));
        }
    }
    let Some((_, k)) = best else {
        return Finding::checked("The neutral surfaces share one light.");
    };
    let (cool, warm) = values.split_at(k);
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let gap = mean(warm) - mean(cool);
    let (minority, majority) = if cool.len() <= warm.len() {
        (&cells[..k], warm)
    } else {
        (&cells[k..], cool)
    };
    let reading = format!(
        "Neutral surfaces in {} parts of the frame, {:.2} apart in blue against red.",
        cells.len(),
        gap
    );
    let tight = spread(cool).max(spread(warm)) <= gap / 3.0;
    let share = minority.len() as f32 / cells.len() as f32;
    if gap < 0.3 || !tight || !(0.1..=0.45).contains(&share) {
        return Finding::checked(format!(
            "{reading} One light, or a gradual change across the frame."
        ));
    }
    let mut marked = vec![false; GRID * GRID];
    for (cell, _) in minority {
        marked[*cell] = true;
    }
    let Some(area) = blobs(&marked, GRID, GRID).into_iter().next() else {
        return Finding::checked(reading);
    };
    if (area.cells as f32) < 0.7 * minority.len() as f32 {
        return Finding::checked(format!(
            "{reading} The other light is scattered, not one area."
        ));
    }
    // The sample: the brightest neutral pixel of the minority whose colour is its median.
    let target = mean(if cool.len() <= warm.len() { cool } else { warm });
    let source = minority
        .iter()
        .flat_map(|(cell, _)| tiles[*cell].iter())
        .filter(|(t, _)| (t - target).abs() < 0.05)
        .max_by(|a, b| s.luma[a.1].total_cmp(&s.luma[b.1]))
        .map(|(_, i)| [(i % s.w) as f32 / s.w as f32, (i / s.w) as f32 / s.h as f32]);
    let Some(source) = source else {
        return Finding::checked(reading);
    };
    let _ = majority;
    let [cx, cy, ..] = area.ellipse(GRID, GRID, 0.3);
    let place = |v: f32, low: &'static str, high: &'static str| {
        if v < 0.35 {
            low
        } else if v > 0.65 {
            high
        } else {
            "middle"
        }
    };
    // Reported, not changed: on real photographs a sunset band, open shade under a canopy and a
    // lamp-lit corner all measure the same way, and only the first two are the light the
    // photographer wanted. Local white balance sampled at this point evens it by hand.
    Finding::checked(format!(
        "{reading} {} of the frame ({} {}) is lit by a different light. Not changed automatically - a sunset or open shade measures the same way; to even it, use Local white balance and sample the neutral surface at {:.0}% across, {:.0}% down.",
        percent(share),
        place(cy, "top", "bottom"),
        place(cx, "left", "right"),
        source[0] * 100.0,
        source[1] * 100.0
    ))
}

// ---- 11-13. Casts by tone ---------------------------------------------------------------------

/// Mean colour of the near-neutral pixels of one tone band, as each channel over luminance.
fn band_colour(s: &Scene<'_>, low: f32, high: f32) -> Option<([f32; 3], f32)> {
    let (mut sum, mut n) = ([0.0_f32; 3], 0_usize);
    let mut total = 0_usize;
    for y in (0..s.h).step_by(2) {
        for x in (0..s.w).step_by(2) {
            total += 1;
            let i = y * s.w + x;
            let l = s.luma[i];
            let p = s.rgb(i);
            if s.people[i] > 0.5 || !(low..high).contains(&l) || saturation(p) > 0.2 {
                continue;
            }
            for c in 0..3 {
                sum[c] += p[c] / l.max(1e-3);
            }
            n += 1;
        }
    }
    let share = n as f32 / total.max(1) as f32;
    (share >= 0.015).then(|| (sum.map(|v| v / n as f32), share))
}

/// The renderer's grade-wheel offset for a hue, per channel (see `creative::tint_offset`).
fn wheel_offset(hue_degrees: f32) -> [f32; 3] {
    let h = (hue_degrees / 60.0).rem_euclid(6.0);
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let rgb = match h as u8 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    };
    let l = (rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722).max(1e-4);
    rgb.map(|v| v / l - 1.0)
}

/// The wheel hue and saturation that cancel `share` of `cast` (each channel's excess over
/// luminance).
fn wheel_against(cast: [f32; 3], share: f32) -> (i16, i16) {
    let want = cast.map(|v| -v * share);
    let mut best = (0, 0.0_f32, f32::MAX);
    for hue in (0..360).step_by(5) {
        let o = wheel_offset(hue as f32);
        let oo: f32 = o.iter().map(|v| v * v).sum();
        let s = (o.iter().zip(want).map(|(a, b)| a * b).sum::<f32>() / oo.max(1e-6)).max(0.0);
        let miss: f32 = (0..3).map(|c| (want[c] - o[c] * s).powi(2)).sum();
        if miss < best.2 {
            best = (hue, s, miss);
        }
    }
    (
        best.0 as i16,
        (best.1 * 100.0).round().clamp(0.0, 25.0) as i16,
    )
}

/// Green over the mean of red and blue, as a share: positive is green, negative magenta.
fn green_excess(c: [f32; 3]) -> f32 {
    c[1] - 0.5 * (c[0] + c[2])
}

fn shadow_tint(s: &Scene<'_>, base: &Recipe) -> Finding {
    if base.global.calibration.shadows_tint != 0 {
        return Finding::checked("Shadow tint is already set.");
    }
    let (Some((shadows, _)), Some((mids, _))) =
        (band_colour(s, 0.05, 0.25), band_colour(s, 0.35, 0.65))
    else {
        return Finding::not_applicable("Too few neutral shadows and midtones to compare.");
    };
    let d = green_excess(shadows) - green_excess(mids);
    let reading = format!(
        "Neutral shadows are {:+.1}% greener than neutral midtones.",
        d * 100.0
    );
    // Below three per cent the difference is the scene's own light (a red carpet's bounce, a
    // coloured uplight), not the camera's.
    if d.abs() < 0.03 {
        return Finding::checked(format!("{reading} Within what the light itself gives."));
    }
    // The renderer lowers green by 0.25 t in the deepest shadows; most of the excess is removed.
    let tint = (d * 280.0).round().clamp(-40.0, 40.0) as i16;
    Finding::applied(
        reading,
        format!("Calibration shadow tint {tint:+}."),
        vec![Change::ShadowsTint(tint)],
        Vec::new(),
    )
}

/// The cast of one tone band relative to the midtones, with green-magenta left to the tint.
fn cast_against_mids(band: [f32; 3], mids: [f32; 3], keep_green: bool) -> [f32; 3] {
    let mut cast: [f32; 3] = std::array::from_fn(|c| band[c] - mids[c]);
    if !keep_green {
        let g = green_excess(cast);
        // Remove the component along green-magenta, [-0.5, 1, -0.5] / |..|^2.
        let k = g / 1.5;
        cast = [cast[0] + 0.5 * k, cast[1] - k, cast[2] + 0.5 * k];
    }
    cast
}

fn shadow_cast(s: &Scene<'_>, base: &Recipe) -> Finding {
    if base.global.colour_grade.shadows.saturation != 0 {
        return Finding::checked("The shadows already carry a colour grade.");
    }
    let (Some((shadows, _)), Some((mids, _))) =
        (band_colour(s, 0.05, 0.25), band_colour(s, 0.35, 0.65))
    else {
        return Finding::not_applicable("Too few neutral shadows and midtones to compare.");
    };
    let cast = cast_against_mids(shadows, mids, false);
    let by = cast[2] - 0.5 * (cast[0] + cast[1]);
    let reading = format!(
        "Neutral shadows are {:+.1}% bluer than neutral midtones.",
        by * 100.0
    );
    if by.abs() < 0.02 {
        return Finding::checked(format!("{reading} The same colour."));
    }
    let (hue, saturation) = wheel_against(cast, 0.7);
    if saturation < 1 {
        return Finding::checked(format!("{reading} Too slight to grade."));
    }
    Finding::applied(
        reading,
        format!("Shadows wheel {hue}° at {saturation}, against the cast."),
        vec![Change::ShadowsWheel { hue, saturation }],
        Vec::new(),
    )
}

fn highlight_tint(s: &Scene<'_>, base: &Recipe) -> Finding {
    if base.global.colour_grade.highlights.saturation != 0 {
        return Finding::checked("The highlights already carry a colour grade.");
    }
    let (Some((highs, _)), Some((mids, _))) =
        (band_colour(s, 0.75, 0.95), band_colour(s, 0.35, 0.65))
    else {
        return Finding::not_applicable("Too few neutral highlights and midtones to compare.");
    };
    let cast = cast_against_mids(highs, mids, true);
    let size = cast.iter().map(|v| v * v).sum::<f32>().sqrt();
    let reading = format!(
        "Neutral highlights differ from neutral midtones by {} (red {:+.1}%, green {:+.1}%, blue {:+.1}%).",
        percent(size),
        cast[0] * 100.0,
        cast[1] * 100.0,
        cast[2] * 100.0
    );
    // Sunlit highlights are warmer than skylit midtones by nature; only a strong difference is a
    // tint, and only a third of it is taken out.
    if size < 0.08 {
        return Finding::checked(format!("{reading} Within what the light itself gives."));
    }
    let (hue, saturation) = wheel_against(cast, 0.35);
    if saturation < 1 {
        return Finding::checked(format!("{reading} Too slight to grade."));
    }
    Finding::applied(
        reading,
        format!("Highlights wheel {hue}° at {saturation}, a third of the way against the tint."),
        vec![Change::HighlightsWheel { hue, saturation }],
        Vec::new(),
    )
}

// ---- 14. Regional haze --------------------------------------------------------------------------

fn regional_haze(s: &Scene<'_>, base: &Recipe, def: &StepDef) -> Finding {
    const CELL: usize = 64;
    // Behind a person a lens at a wide aperture leaves the background soft, bright and low in
    // contrast - which is what haze looks like to a measurement. Measured on real wedding
    // portraits, every "hazy region" behind people was a soft background.
    if !s.no_people() {
        return Finding::not_applicable(
            "People are in the frame: a soft background behind them is depth of field, not haze.",
        );
    }
    let (gw, gh) = (s.w.div_ceil(CELL), s.h.div_ceil(CELL));
    let mut dark = vec![None; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let mut v = Vec::new();
            let mut lum = Vec::new();
            let mut sat = 0.0_f32;
            let (mut sky, mut people, mut n) = (0, 0, 0);
            for y in (gy * CELL..((gy + 1) * CELL).min(s.h)).step_by(2) {
                for x in (gx * CELL..((gx + 1) * CELL).min(s.w)).step_by(2) {
                    let i = y * s.w + x;
                    n += 1;
                    if s.people[i] > 0.5 {
                        people += 1;
                        continue;
                    }
                    // The dark-channel prior does not hold for sky.
                    if s.luma[i] > 0.6 && s.wide_std[i] < 0.02 {
                        sky += 1;
                        continue;
                    }
                    let p = s.rgb(i);
                    v.push(decode(p[0].min(p[1]).min(p[2])));
                    lum.push(s.luma[i]);
                    sat += saturation(p);
                }
            }
            if people * 10 < n && sky * 2 < n && v.len() >= 50 {
                v.sort_by(f32::total_cmp);
                lum.sort_by(f32::total_cmp);
                let spread = lum[lum.len() * 9 / 10] - lum[lum.len() / 10];
                // Haze lifts the darkest tones and flattens what is above them; white flowers
                // and pale curtains lift the darkest tones and keep their contrast and colour.
                let flat = spread < 0.25 && sat / v.len() as f32 <= 0.25;
                dark[gy * gw + gx] = Some((v[v.len() / 10], flat));
            }
        }
    }
    let mut known: Vec<f32> = dark.iter().flatten().map(|d| d.0).collect();
    if known.len() < 12 {
        return Finding::not_applicable(
            "Too little of the frame is scenery (not sky, not people).",
        );
    }
    let Some(typical) = median(&mut known) else {
        return Finding::not_applicable("No reading.");
    };
    let floor = (typical + 0.08).max(0.1);
    let marked: Vec<bool> = dark
        .iter()
        .map(|d| d.is_some_and(|(v, flat)| v >= floor && flat))
        .collect();
    let hazy = marked.iter().filter(|m| **m).count();
    let share = hazy as f32 / known.len() as f32;
    let reading = format!(
        "Darkest tones lift to {floor:.2} or more in {} of the scenery (typical {typical:.2}).",
        percent(share)
    );
    if !(0.08..=0.6).contains(&share) {
        return Finding::checked(format!(
            "{reading} {}",
            if share > 0.6 {
                "Haze over most of the frame is Dehaze's to decide."
            } else {
                "No hazy region."
            }
        ));
    }
    if base.global.dehaze >= 25 {
        return Finding::checked(format!(
            "{reading} Dehaze {} already clears it.",
            base.global.dehaze
        ));
    }
    let Some(area) = blobs(&marked, gw, gh).into_iter().next() else {
        return Finding::checked(reading);
    };
    if (area.cells as f32) < 0.7 * hazy as f32 {
        return Finding::checked(format!("{reading} The haze is scattered, not one region."));
    }
    let mut e = on(def, 0, Tool::LocalDehaze, area.ellipse(gw, gh, 0.5), 0.6);
    e.tone = 0.4;
    Finding::applied(
        reading,
        "Local dehaze on the hazy region only; people left as they are.",
        Vec::new(),
        vec![e],
    )
    .within(super::OUTSIDE_PEOPLE, s.outside_people())
}

// ---- 15-16. Sharpness --------------------------------------------------------------------------

/// The median 10-90% rise width, in preview pixels, of the strongest edges.
fn edge_width(s: &Scene<'_>) -> Option<f32> {
    let (w, h) = (s.w, s.h);
    let at = |x: f32, y: f32| {
        let (xi, yi) = (x.floor(), y.floor());
        if xi < 0.0 || yi < 0.0 || xi as usize + 1 >= w || yi as usize + 1 >= h {
            return None;
        }
        let (fx, fy) = (x - xi, y - yi);
        let i = yi as usize * w + xi as usize;
        let top = s.luma[i] * (1.0 - fx) + s.luma[i + 1] * fx;
        let bottom = s.luma[i + w] * (1.0 - fx) + s.luma[i + w + 1] * fx;
        Some(top * (1.0 - fy) + bottom * fy)
    };
    let mut strong = Vec::new();
    for y in (8..h - 8).step_by(2) {
        for x in (8..w - 8).step_by(2) {
            let i = y * w + x;
            let gx = (s.luma[i + 1] - s.luma[i - 1]) / 2.0;
            let gy = (s.luma[i + w] - s.luma[i - w]) / 2.0;
            strong.push((gx.hypot(gy), x, y, gx, gy));
        }
    }
    let keep = (strong.len() / 300).clamp(50, 4000);
    if strong.len() <= keep {
        return None;
    }
    strong.select_nth_unstable_by(keep, |a, b| b.0.total_cmp(&a.0));
    let mut widths = Vec::new();
    for &(g, x, y, gx, gy) in strong.iter().take(keep) {
        if g < 0.02 {
            continue;
        }
        let (ux, uy) = (gx / g, gy / g);
        let profile: Option<Vec<f32>> = (-12..=12)
            .map(|k| {
                at(
                    x as f32 + ux * k as f32 * 0.5,
                    y as f32 + uy * k as f32 * 0.5,
                )
            })
            .collect();
        let Some(p) = profile else { continue };
        let (lo, hi) = (p[0], p[p.len() - 1]);
        if hi - lo < 0.15 {
            continue;
        }
        // Where the profile crosses a level, between samples.
        let level = |t: f32| {
            let target = lo + t * (hi - lo);
            let k = p.iter().position(|v| *v >= target)?;
            if k == 0 {
                return Some(0.0);
            }
            let (a, b) = (p[k - 1], p[k]);
            Some(k as f32 - 1.0 + (target - a) / (b - a).max(1e-6))
        };
        if let (Some(a), Some(b)) = (level(0.1), level(0.9)) {
            widths.push((b - a) * 0.5);
        }
    }
    (widths.len() >= 30).then(|| median(&mut widths)).flatten()
}

fn capture_sharpening(s: &Scene<'_>, base: &Recipe, edges: Option<f32>) -> Finding {
    let sharpen = &base.global.sharpen;
    if sharpen.amount == 0 {
        return Finding::not_applicable("Sharpening is off, so it has no radius to match.");
    }
    let Some(width) = edges else {
        return Finding::not_applicable(
            "Too few strong edges to measure how soft the lens drew them.",
        );
    };
    let target: f32 = match width {
        w if w <= 1.6 => 0.8,
        w if w <= 2.4 => 1.0,
        w if w <= 3.5 => 1.3,
        _ => 1.6,
    };
    let reading = s.capture.width.map_or_else(
        || format!("The strongest edges rise over {width:.2} preview px."),
        |_| {
            format!(
                "The strongest edges rise over {width:.2} preview px (about {:.0} px of the original).",
                width * s.scale()
            )
        },
    );
    if (target - sharpen.radius).abs() < 0.15 {
        return Finding::checked(format!(
            "{reading} Radius {:.1} already matches.",
            sharpen.radius
        ));
    }
    Finding::applied(
        reading,
        format!(
            "Sharpening radius {:.1} -> {target:.1}, matched to the edges.",
            sharpen.radius
        ),
        vec![Change::SharpenRadius(target)],
        Vec::new(),
    )
}

fn camera_shake(s: &Scene<'_>, width: Option<f32>) -> Finding {
    let slow = match (s.capture.shutter, s.capture.focal_mm) {
        (Some(t), Some(f)) => Some((t, f, t > 1.0 / f)),
        _ => None,
    };
    let edges = width.map_or_else(
        || "Too few strong edges to judge sharpness.".to_owned(),
        |w| format!("The strongest edges rise over {w:.1} preview px."),
    );
    let exposure = slow.map_or_else(String::new, |(t, f, _)| {
        format!(" Shutter 1/{:.0} s at {f:.0} mm.", 1.0 / t.max(1e-6))
    });
    let soft = width.is_some_and(|w| w > 2.4);
    let finding = match (soft, slow.is_some_and(|(.., risk)| risk)) {
        (true, true) => "Soft, and slower than one over the focal length: camera shake. Sharpening cannot undo it; this frame is flagged for review.",
        (true, false) => "Soft though the shutter was fast enough: missed focus or a soft lens. Flagged for review.",
        (false, true) => "Slower than one over the focal length, but the edges are sharp: held steady.",
        (false, false) => "Sharp.",
    };
    Finding::checked(format!("{edges}{exposure} {finding}"))
}

// ---- 17. Green and blue fringes -----------------------------------------------------------------

/// Rim pixels per million of the frame, and in all, before a removal is proposed - the same
/// evidence Auto enhance needs for purple fringes.
const FRINGE_PER_MILLION: f32 = 40.0;
const FRINGE_FEWEST: f32 = 80.0;

fn green_fringes(s: &Scene<'_>, base: &Recipe, def: &StepDef) -> Finding {
    // A lens's axial colour leaves purple rims in front of the plane of focus and green ones
    // behind it. Without the purple ones Auto enhance confirmed, green beside something bright
    // was, on real photographs, leaves and garlands beside white flowers - kept.
    let purple = aura_recipe::retouch_tools::read(base).is_ok_and(|ops| {
        ops.iter()
            .any(|e| e.id == crate::smart_edit::DEFRINGE_ID && e.enabled)
    });
    if !purple {
        return Finding::checked(
            "Auto enhance found no purple fringes, so this lens left no axial colour to pair green rims with; green beside bright things is kept as it is.",
        );
    }
    let Some(f) = s
        .frame
        .as_ref()
        .filter(|f| f.kind == aura_render::InputKind::Working)
    else {
        return Finding::not_applicable(
            "The renderer's own frame was not available to measure fringes on.",
        );
    };
    let mut e = whole(def, 0, Tool::Defringe, 1.0);
    e.radius = 0.003;
    e.warmth = 0.0;
    e.tint = 1.0;
    let (w, h) = (f.width as usize, f.height as usize);
    // Green garlands, teal silk and leaves in a bouquet are green things on people, and a lens's
    // rim is never needed on them to show the fault: it is counted, and removed, elsewhere only.
    let mut rgb = f.rgb.clone();
    if !s.no_people() {
        for y in 0..h {
            let py = (y * s.h / h.max(1)).min(s.h - 1);
            for x in 0..w {
                let px = (x * s.w / w.max(1)).min(s.w - 1);
                if s.people[py * s.w + px] > 0.5 {
                    rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(0.0);
                }
            }
        }
    }
    let rims = aura_render::retouch_tools::defringe_rims(&rgb, w, h, &e);
    let enough = (FRINGE_PER_MILLION * (w * h) as f32 / 1e6).max(FRINGE_FEWEST);
    let reading = format!(
        "{} on the dark side of bright edges read as a lens's green rim.",
        plural(rims, "pixel", "pixels")
    );
    if (rims as f32) < enough {
        return Finding::checked(format!("{reading} Too few to correct."));
    }
    Finding::applied(
        reading,
        "Green rims take the colour of their calm surroundings; green things, green lights and people kept.",
        Vec::new(),
        vec![e],
    )
    .within(super::OUTSIDE_PEOPLE, s.outside_people())
}

// ---- 18. Over-saturated light colours -----------------------------------------------------------

fn saturated_lights(s: &Scene<'_>, def: &StepDef) -> Finding {
    const CELL: usize = 16;
    let burnt = |i: usize| {
        let p = s.rgb(i);
        s.people[i] < 0.5 && saturation(p) >= 0.9 && p[0].max(p[1]).max(p[2]) >= 0.985
    };
    let (share, gw, gh) = grid_share(s.w, s.h, CELL, burnt);
    let marked: Vec<bool> = share.iter().map(|v| *v >= 0.25).collect();
    let least = 0.0004 * s.len() as f32;
    let areas: Vec<Blob> = blobs(&marked, gw, gh)
        .into_iter()
        .filter(|b| (b.cells * CELL * CELL) as f32 * 0.25 >= least)
        .take(6)
        .collect();
    if areas.is_empty() {
        return Finding::checked("No colour is burnt out at full saturation.");
    }
    let mut ops = Vec::new();
    for b in &areas {
        // The most typical burnt pixel of the area is the colour the selection keys on.
        let (x0, y0) = (b.x0 * CELL, b.y0 * CELL);
        let (x1, y1) = ((b.x1 * CELL).min(s.w), (b.y1 * CELL).min(s.h));
        let sample = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .find(|&(x, y)| burnt(y * s.w + x) && burnt(y * s.w + (x + 1).min(s.w - 1)));
        let Some((x, y)) = sample else { continue };
        let mut e = on(
            def,
            ops.len(),
            Tool::ColorSaturation,
            b.ellipse(gw, gh, 1.0),
            0.8,
        );
        e.warmth = -0.25;
        e.selection = Some(Selection {
            inverted: false,
            gradient: None,
            luminance: None,
            color: Some(ColorRange {
                sample: [(x as f32 + 0.5) / s.w as f32, (y as f32 + 0.5) / s.h as f32],
                tolerance: 18.0,
                softness: 12.0,
                lightness: 0.3,
            }),
        });
        ops.push(e);
    }
    if ops.is_empty() {
        return Finding::checked("No area large enough to treat.");
    }
    Finding::applied(
        format!(
            "{} of stage or LED colour burnt out at full saturation.",
            plural(ops.len(), "area", "areas")
        ),
        "That colour's saturation lowered a quarter inside each area, so its detail shows again.",
        Vec::new(),
        ops,
    )
}

// ---- 19. Level horizon --------------------------------------------------------------------------

fn horizon(s: &Scene<'_>, base: &Recipe, _def: &StepDef) -> Finding {
    if !base.geometry.is_identity() {
        return Finding::checked("The photograph already has a crop or rotation; it is kept.");
    }
    match measure::tilt(&s.px, Some(&s.people)) {
        measure::Tilt::Tilted {
            degrees,
            lines,
            agreement,
        } => {
            let aspect = s.w as f32 / s.h.max(1) as f32;
            let rect = aura_geometry::straighten::inscribed(-degrees, aspect);
            Finding::applied(
                format!(
                    "{lines} long straight lines agree ({:.0}% of their length) on a {degrees:+.1}° tilt.",
                    agreement * 100.0
                ),
                format!("Rotated {:+.1}° and cropped only enough to hide the corners.", -degrees),
                vec![Change::Level {
                    rotate: -degrees,
                    crop: [rect.x, rect.y, rect.x + rect.w, rect.y + rect.h],
                }],
                Vec::new(),
            )
        }
        measure::Tilt::Level { lines, degrees } => Finding::checked(format!(
            "{lines} straight lines agree the frame is level ({degrees:+.2}°)."
        )),
        measure::Tilt::Unsure(why) => Finding::checked(format!("Left as shot: {why}.")),
    }
}

// ---- 20. Pale sky ------------------------------------------------------------------------------

fn pale_sky(s: &Scene<'_>, def: &StepDef) -> Finding {
    const CELL: usize = 32;
    let top = (s.h as f32 * 0.45) as usize;
    let sky = |i: usize| {
        let p = s.rgb(i);
        let l = s.luma[i];
        i / s.w < top
            && s.people[i] < 0.5
            && (0.5..0.97).contains(&l)
            && s.wide_std[i] < 0.02
            && (saturation(p) < 0.06 || hue(p).is_some_and(|h| (0.45..0.72).contains(&h)))
    };
    let (mut n, mut blue, mut sat) = (0_usize, 0_usize, 0.0_f32);
    for y in (0..top).step_by(2) {
        for x in (0..s.w).step_by(2) {
            let i = y * s.w + x;
            if sky(i) {
                n += 1;
                let p = s.rgb(i);
                let sv = saturation(p);
                sat += sv;
                if sv >= 0.04 && hue(p).is_some_and(|h| (0.5..0.7).contains(&h)) {
                    blue += 1;
                }
            }
        }
    }
    let share = n as f32 / ((s.w / 2) * (s.h / 2)).max(1) as f32;
    if share < 0.08 {
        return Finding::not_applicable("No open sky in the top of the frame.");
    }
    let (blue_share, mean_sat) = (blue as f32 / n as f32, sat / n as f32);
    let reading = format!(
        "Sky over {} of the frame: {} of it blue, saturation {:.2}.",
        percent(share),
        percent(blue_share),
        mean_sat
    );
    if blue_share < 0.6 {
        return Finding::checked(format!(
            "{reading} Overcast or white: it is kept as it was."
        ));
    }
    if mean_sat > 0.16 {
        return Finding::checked(format!("{reading} Already a clear blue."));
    }
    let (cells, gw, gh) = grid_share(s.w, s.h, CELL, sky);
    let marked: Vec<bool> = cells.iter().map(|v| *v >= 0.5).collect();
    let Some(area) = blobs(&marked, gw, gh).into_iter().next() else {
        return Finding::checked(reading);
    };
    let mut e = on(def, 0, Tool::SkyBlue, area.ellipse(gw, gh, 0.5), 0.6);
    e.tone = 0.3;
    Finding::applied(
        reading,
        "A washed-out blue sky deepened gently; clouds, anything warm and people left alone.",
        Vec::new(),
        vec![e],
    )
    .within(super::OUTSIDE_PEOPLE, s.outside_people())
}

// ---- 21. Scene texture -------------------------------------------------------------------------

fn scene_texture(s: &Scene<'_>, n: &Noise, base: &Recipe) -> Finding {
    if !s.no_people() {
        return Finding::not_applicable(
            "People are in the frame; their texture is retouch's to decide.",
        );
    }
    if base.global.texture != 0 {
        return Finding::checked(format!("Texture is already {}.", base.global.texture));
    }
    let (mut sum, mut count) = (0.0_f32, 0_usize);
    for i in (0..s.len()).step_by(3) {
        if s.wide_std[i] > 0.03 {
            sum += s.fine_std[i];
            count += 1;
        }
    }
    if count < 2000 {
        return Finding::not_applicable("Almost no textured area to bring out.");
    }
    let detail = sum / count as f32;
    let reading = format!(
        "Fine detail {detail:.3} on textured areas, noise {:.4}.",
        n.midtones
    );
    if n.midtones > 0.008 {
        return Finding::checked(format!(
            "{reading} Too noisy: texture would bring out the noise."
        ));
    }
    let amount = if detail < 0.03 {
        12
    } else if detail < 0.05 {
        8
    } else {
        return Finding::checked(format!("{reading} Already crisp."));
    };
    Finding::applied(
        reading,
        format!("Texture +{amount}."),
        vec![Change::Texture(amount)],
        Vec::new(),
    )
}

// ---- 22. Hard corner shading -------------------------------------------------------------------

fn hard_corners(s: &Scene<'_>, base: &Recipe, levelled: bool) -> Finding {
    if levelled {
        return Finding::checked("The level crop already removed the corners.");
    }
    if !base.geometry.is_identity() {
        return Finding::checked("The photograph already has a crop; it is kept.");
    }
    let diag = (s.w as f32).hypot(s.h as f32);
    let mean_at = |t: f32, corner: usize| {
        // A small disc on the diagonal, `t` of the way in from the corner.
        let (sx, sy) = (
            if corner.is_multiple_of(2) { 1.0 } else { -1.0 },
            if corner < 2 { 1.0 } else { -1.0 },
        );
        let (ox, oy) = (
            if corner.is_multiple_of(2) {
                0.0
            } else {
                s.w as f32 - 1.0
            },
            if corner < 2 { 0.0 } else { s.h as f32 - 1.0 },
        );
        let d = t * diag / std::f32::consts::SQRT_2;
        let (cx, cy) = (ox + sx * d, oy + sy * d);
        let r = (diag * 0.006).max(2.0);
        let (mut sum, mut n) = (0.0, 0);
        for y in (cy - r).max(0.0) as usize..((cy + r) as usize).min(s.h) {
            for x in (cx - r).max(0.0) as usize..((cx + r) as usize).min(s.w) {
                sum += s.luma[y * s.w + x];
                n += 1;
            }
        }
        (n > 0).then(|| sum / n as f32)
    };
    // A hood or a filter shades the corners and leaves the middle of each side lit; a black
    // backdrop is black along the sides as well.
    let edge_mid = |x: usize, y: usize| {
        let r = ((diag * 0.006) as usize).max(2);
        let (mut sum, mut n) = (0.0, 0);
        for yy in y.saturating_sub(r)..(y + r).min(s.h) {
            for xx in x.saturating_sub(r)..(x + r).min(s.w) {
                sum += s.luma[yy * s.w + xx];
                n += 1;
            }
        }
        sum / n.max(1) as f32
    };
    let (mx, my) = (s.w / 2, s.h / 2);
    let inset = (diag * 0.01) as usize;
    let sides_lit = [
        edge_mid(mx, inset),
        edge_mid(mx, s.h - 1 - inset),
        edge_mid(inset, my),
        edge_mid(s.w - 1 - inset, my),
    ]
    .iter()
    .all(|v| *v > 0.1);
    let mut depth: Vec<f32> = Vec::new();
    for corner in 0..4 {
        let (Some(at), Some(inward)) = (mean_at(0.01, corner), mean_at(0.12, corner)) else {
            continue;
        };
        if at < 0.03 && inward > 0.15 {
            // How far in the black reaches along the diagonal.
            let reach = (1..=40)
                .map(|k| k as f32 * 0.0025)
                .find(|t| mean_at(*t, corner).is_some_and(|v| v > 0.5 * inward))
                .unwrap_or(0.1);
            depth.push(reach);
        }
    }
    if depth.len() < 3 || !sides_lit {
        return Finding::checked(if depth.len() >= 2 && !sides_lit {
            "Dark corners, but the sides are dark too: a dark backdrop, not a lens hood."
        } else {
            "No corner is shaded black by a lens hood or filter."
        });
    }
    let reach = depth.iter().copied().fold(0.0_f32, f32::max);
    // The diagonal reach as an equal inset on every side, keeping the frame's shape.
    let inset = (reach * 0.75).clamp(0.01, 0.08);
    Finding::applied(
        format!(
            "{} shaded hard to black, reaching {} of the diagonal: a lens hood or a stacked filter.",
            plural(depth.len(), "corner", "corners"),
            percent(reach)
        ),
        format!("Cropped {} in from each side to remove them.", percent(inset)),
        vec![Change::Crop([inset, inset, 1.0 - inset, 1.0 - inset])],
        Vec::new(),
    )
}

// ---- 23. Dark foreground ------------------------------------------------------------------------

fn dark_foreground(s: &Scene<'_>, base: &Recipe, def: &StepDef) -> Finding {
    if !s.no_people() {
        return Finding::not_applicable("People are in the frame; their light is not graduated.");
    }
    let row_mean = |y: usize| s.luma[y * s.w..(y + 1) * s.w].iter().sum::<f32>() / s.w as f32;
    let band = |a: f32, b: f32| {
        let (y0, y1) = ((s.h as f32 * a) as usize, (s.h as f32 * b) as usize);
        (y0..y1).map(row_mean).sum::<f32>() / (y1 - y0).max(1) as f32
    };
    let (top, bottom) = (band(0.0, 0.33), band(0.67, 1.0));
    let reading =
        format!("Top third at {top:.2}, bottom third at {bottom:.2} (display brightness).");
    if top - bottom < 0.25 || bottom >= 0.28 || top < 0.5 {
        return Finding::checked(format!(
            "{reading} The foreground is not dark against the sky."
        ));
    }
    // The horizon: where the rows cross half way between the two, searched in the middle.
    let half = f32::midpoint(top, bottom);
    let line = ((s.h as f32 * 0.3) as usize..(s.h as f32 * 0.8) as usize)
        .find(|y| row_mean(*y) < half)
        .map_or(0.55, |y| y as f32 / s.h as f32);
    let mut stops = ((0.30 - bottom) * 3.0).clamp(0.2, 0.7);
    if base.global.shadows >= 40 {
        stops /= 2.0;
    }
    let mut e = whole(
        def,
        0,
        Tool::Dodge,
        ((stops.exp2() - 1.0) / (0.75_f32.exp2() - 1.0)).clamp(0.1, 0.95),
    );
    // The lift is for the dark ground: lamps, lit windows and white boats below the horizon keep
    // their brightness instead of clipping. Luminance is read after the edit's exposure.
    let ceiling = (decode(0.4) * 2.0_f32.powf(base.global.exposure) / 0.18).log2();
    e.selection = Some(Selection {
        inverted: false,
        gradient: Some(Gradient {
            start: [0.5, line],
            end: [0.5, (line + 0.25).min(1.0)],
        }),
        luminance: Some(LuminanceRange {
            low: -16.0,
            high: ceiling,
            softness: 1.5,
        }),
        color: None,
    });
    Finding::applied(
        reading,
        format!(
            "The dark foreground below the horizon at {} of the height lifted {stops:.1} stop, graduated; its bright parts keep their brightness.",
            percent(line)
        ),
        Vec::new(),
        vec![e],
    )
}

// ---- 24. Sharpening mask for noisy files -------------------------------------------------------

fn sharpen_mask(s: &Scene<'_>, n: &Noise, base: &Recipe) -> Finding {
    let sharpen = &base.global.sharpen;
    if sharpen.amount == 0 {
        return Finding::not_applicable("Sharpening is off.");
    }
    let iso = s.capture.iso.unwrap_or(0);
    let target = if iso >= 3200 || n.midtones >= 0.008 {
        60
    } else if iso >= 1600 || n.midtones >= 0.005 {
        40
    } else {
        0
    };
    let reading = format!(
        "{}noise {:.4} on plain midtones.",
        if iso > 0 {
            format!("ISO {iso}, ")
        } else {
            String::new()
        },
        n.midtones
    );
    if target == 0 {
        return Finding::checked(format!("{reading} Clean enough to sharpen everywhere."));
    }
    if sharpen.masking >= target {
        return Finding::checked(format!(
            "{reading} Masking {} already keeps it off flat areas.",
            sharpen.masking
        ));
    }
    Finding::applied(
        reading,
        format!(
            "Sharpening masking {} -> {target}: edges sharpened, flat noisy areas left alone.",
            sharpen.masking
        ),
        vec![Change::SharpenMasking(target)],
        Vec::new(),
    )
}

// ---- 25. Flicker bands -------------------------------------------------------------------------

fn flicker(s: &Scene<'_>) -> Finding {
    let mut rows = Vec::with_capacity(s.h);
    for y in 0..s.h {
        let (mut sum, mut n) = (0.0_f32, 0_usize);
        for x in (0..s.w).step_by(2) {
            let i = y * s.w + x;
            if s.people[i] < 0.5 && s.wide_std[i] < 0.05 && (0.1..0.9).contains(&s.luma[i]) {
                sum += s.luma[i] / s.wide_mean[i].max(0.02);
                n += 1;
            }
        }
        rows.push((n * 2 >= s.w / 3).then(|| sum / n as f32));
    }
    let usable = rows.iter().filter(|r| r.is_some()).count();
    if usable < s.h / 2 {
        return Finding::not_applicable("Too few plain rows to look for flicker across.");
    }
    // Each row against its neighbours 15 rows either way, so the scene's own light drops out.
    let detrended: Vec<f32> = (0..s.h)
        .map(|y| {
            let Some(v) = rows[y] else { return 0.0 };
            let near: Vec<f32> = rows[y.saturating_sub(15)..(y + 16).min(s.h)]
                .iter()
                .flatten()
                .copied()
                .collect();
            v - near.iter().sum::<f32>() / near.len().max(1) as f32
        })
        .collect();
    let energy = detrended.iter().map(|v| v * v).sum::<f32>();
    let rms = (energy / s.h as f32).sqrt();
    let mut best = (0_usize, 0.0_f32);
    for lag in 6..(s.h / 4).min(120) {
        let c: f32 = (0..s.h - lag)
            .map(|y| detrended[y] * detrended[y + lag])
            .sum();
        let r = c / energy.max(1e-9);
        if r > best.1 {
            best = (lag, r);
        }
    }
    if best.1 >= 0.5 && rms >= 0.004 {
        return Finding::checked(format!(
            "Rows brighten and darken every {} px of the preview ({} of the light): flicker from LED or fluorescent light with a fast or electronic shutter. It cannot be removed reliably; flagged for review.",
            best.0,
            percent(rms)
        ));
    }
    Finding::checked("No periodic brightening across rows.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photo_pass::scene::Capture;
    use crate::portrait_features::Pixels;

    fn scene_of(bytes: &[u8], w: u32, h: u32, capture: Capture) -> Scene<'_> {
        Scene::new(Pixels::new(bytes, w, h).unwrap(), Vec::new(), capture, None)
    }

    fn paint(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Vec<u8> {
        let mut out = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                out.extend(f(x, y).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
            }
        }
        out
    }

    fn neutral() -> Recipe {
        aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t")
    }

    /// Deterministic noise in -1..1.
    fn noise(x: usize, y: usize, seed: u32) -> f32 {
        let mut v =
            (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663) ^ seed;
        v ^= v >> 13;
        v = v.wrapping_mul(0x5bd1_e995);
        v ^= v >> 15;
        (v & 0xFFFF) as f32 / 32_767.5 - 1.0
    }

    #[test]
    fn jpeg_quality_reads_the_luminance_table() {
        // A DQT for quality 50 is the standard table itself.
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00, 67, 0x00];
        bytes.extend(STANDARD_LUMA.iter().map(|v| *v as u8));
        bytes.extend([0xFF, 0xDA, 0x00, 0x02]);
        assert_eq!(jpeg_quality(&bytes), Some(50));
        // Quality 90 halves... scale 20: each entry a fifth.
        let mut fine = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00, 67, 0x00];
        fine.extend(
            STANDARD_LUMA
                .iter()
                .map(|v| ((u32::from(*v) * 20 + 50) / 100).max(1) as u8),
        );
        assert!(jpeg_quality(&fine).is_some_and(|q| (88..=92).contains(&q)));
        assert_eq!(jpeg_quality(b"not a jpeg"), None);
    }

    #[test]
    fn colour_noise_is_measured_and_raised_only_when_it_shows() {
        let (w, h) = (256, 256);
        let clean = paint(w, h, |_, _| [0.5, 0.5, 0.5]);
        let noisy = paint(w, h, |x, y| {
            let n = 0.06 * noise(x, y, 1);
            [0.5 + n, 0.5 - n * 0.5, 0.5 + 0.06 * noise(x, y, 2)]
        });
        let s = scene_of(&clean, 256, 256, Capture::default());
        let n = Noise::measure(&s);
        assert_eq!(
            colour_noise(&n, &neutral()).outcome,
            super::super::Outcome::Unchanged
        );
        let s = scene_of(&noisy, 256, 256, Capture::default());
        let n = Noise::measure(&s);
        assert!(n.colour > 0.01, "{n:?}");
        let f = colour_noise(&n, &neutral());
        assert_eq!(f.outcome, super::super::Outcome::Applied, "{f:?}");
    }

    #[test]
    fn banding_is_found_in_a_stepped_eight_bit_gradient_and_never_in_raw() {
        let (w, h) = (256, 160);
        // A gradient that steps one code value every 12 px.
        let banded = paint(w, h, |x, _| {
            let v = (100.0 + (x / 12) as f32) / 255.0;
            [v, v, v]
        });
        let jpeg = Capture {
            kind: Some("jpeg".into()),
            ..Capture::default()
        };
        let s = scene_of(&banded, 256, 160, jpeg);
        let f = banding(&s, &STEPS[6]);
        assert_eq!(f.outcome, super::super::Outcome::Applied, "{f:?}");
        assert_eq!(f.ops[0].id, "pass-07-banding-0");
        let raw = Capture {
            kind: Some("raw".into()),
            ..Capture::default()
        };
        let s = scene_of(&banded, 256, 160, raw);
        assert_eq!(
            banding(&s, &STEPS[6]).outcome,
            super::super::Outcome::NotApplicable
        );
    }

    #[test]
    fn vignetting_needs_all_four_corners_to_fall_off_together() {
        let (w, h) = (300, 200);
        let falloff = |x: usize, y: usize| {
            let (dx, dy) = (x as f32 - 150.0, y as f32 - 100.0);
            let r2 = (dx * dx + dy * dy) / (150.0_f32.powi(2) + 100.0_f32.powi(2));
            let v = 0.6 / (1.0 + 0.6 * 0.8 * r2);
            [v, v, v]
        };
        let lens = paint(w, h, falloff);
        let s = scene_of(&lens, 300, 200, Capture::default());
        let f = vignetting(&s, &neutral());
        assert_eq!(f.outcome, super::super::Outcome::Applied, "{f:?}");
        // A dark bottom (the ground) is the scene, not the lens.
        let ground = paint(w, h, |x, y| {
            if y > 120 {
                [0.2; 3]
            } else {
                falloff(x, y).map(|v| v.max(0.55))
            }
        });
        let s = scene_of(&ground, 300, 200, Capture::default());
        assert_ne!(
            vignetting(&s, &neutral()).outcome,
            super::super::Outcome::Applied
        );
    }

    #[test]
    fn green_shadows_get_a_magenta_shadow_tint() {
        let (w, h) = (240, 240);
        let tinted = paint(w, h, |x, _| {
            let v = 0.08 + 0.85 * x as f32 / 240.0;
            // Shadows (left) a little green; midtones neutral.
            let g = if v < 0.25 { v * 1.08 } else { v };
            [v, g, v]
        });
        let s = scene_of(&tinted, 240, 240, Capture::default());
        let f = shadow_tint(&s, &neutral());
        assert_eq!(f.outcome, super::super::Outcome::Applied, "{f:?}");
        assert!(matches!(f.sliders[0], Change::ShadowsTint(t) if t > 0));
    }

    #[test]
    fn a_wheel_is_chosen_against_the_cast() {
        // Blue shadows want a yellow-orange wheel.
        let (hue, sat) = wheel_against([-0.03, -0.03, 0.12], 0.7);
        assert!((30..=90).contains(&hue), "{hue}");
        assert!(sat > 0);
    }

    #[test]
    fn dark_foreground_under_a_bright_sky_gets_a_graduated_lift() {
        let (w, h) = (200, 150);
        let img = paint(w, h, |_, y| {
            if y < 75 {
                [0.75, 0.8, 0.9]
            } else {
                [0.12, 0.13, 0.1]
            }
        });
        let s = scene_of(&img, 200, 150, Capture::default());
        let f = dark_foreground(&s, &neutral(), &STEPS[22]);
        assert_eq!(f.outcome, super::super::Outcome::Applied, "{f:?}");
        let g = f.ops[0]
            .selection
            .as_ref()
            .and_then(|s| s.gradient.clone())
            .unwrap();
        assert!(g.start[1] > 0.4 && g.start[1] < 0.6, "{g:?}");
    }

    #[test]
    fn a_plain_grey_frame_needs_nothing() {
        let (w, h) = (256, 192);
        let grey = paint(w, h, |x, y| {
            let v = 0.45 + 0.004 * noise(x, y, 9);
            [v, v, v]
        });
        let s = scene_of(
            &grey,
            256,
            192,
            Capture {
                kind: Some("raw".into()),
                ..Capture::default()
            },
        );
        let mut base = neutral();
        base.global.sharpen.amount = 40;
        base.global.sharpen.radius = 1.0;
        let found = plan(&s, &base);
        assert_eq!(found.len(), 25);
        for (def, f) in STEPS.iter().zip(&found) {
            assert_ne!(
                f.outcome,
                super::super::Outcome::Applied,
                "{}: {f:?}",
                def.title
            );
        }
    }
}
