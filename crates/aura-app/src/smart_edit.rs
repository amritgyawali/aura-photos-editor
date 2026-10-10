//! One-click intelligent editing: measure the photograph, decide what it needs, and save the
//! result as a short series of ordinary history steps.
//!
//! The pass never owns a result. Every decision is written into the same recipe fields and
//! native retouch operations a person would use, each automatic stage is its own undoable
//! history step, and anything a person has already set is protected by
//! [`aura_recipe::schema::merge`]. ADR-0081.
//!
//! What is measured, in order:
//!
//! 1. **Light and colour.** Exposure, highlights, shadows and contrast from the tone
//!    histogram ([`crate::photo_enhance::correction`]); a gray-pixel white-balance estimate
//!    that ignores faces and only removes part of a cast, so a warm room stays warm; vibrance
//!    from how colourful the frame already is; clarity and dehaze from the scene type and the
//!    frame's own dark-channel floor; noise reduction from a measured noise level; sharpening
//!    tuned so skin is masked out on portraits.
//! 2. **Sky.** On outdoor frames with a bright sky, a feathered gradient that only touches
//!    bright pixels above the measured horizon.
//! 3. **Skin, blemishes, eyes, teeth and shine** for each detected face, from
//!    [`crate::portrait_auto`] and [`crate::portrait_features`].
// Pixel statistics intentionally convert between f32 and integer slider values.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]
// Pixel statistics use the conventional short names (r, g, b, n, s, e, w).
#![allow(
    clippy::many_single_char_names,
    clippy::items_after_statements,
    clippy::enum_variant_names
)]

use crate::{
    commands::IpcResult,
    contract::ipc::{DevelopImageInput, RecipeDto},
    portrait_auto::{self, Group, SceneSummary, StepSummary},
    portrait_features::{luma, Pixels},
    AppState,
};
use aura_core::{PhotoId, ProjectId};
use aura_preview::contract::service::{PreviewService, Priority};
use aura_recipe::{
    retouch_tools::{self, Edit, Gradient, LuminanceRange, Selection, Tool},
    schema, EditSource, Recipe,
};
use aura_render::{FrameSource, RenderLevel};
use aura_vision::portrait::PortraitFace;
use std::collections::BTreeMap;

/// The scene the photograph was measured to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneKind {
    Portrait,
    Group,
    Landscape,
    LowLight,
    /// Dark by nature with real light sources in it: a street at night, fireworks, a stage.
    Night,
    /// Bright by design with nothing truly dark: snow, a white studio, a backlit window.
    HighKey,
    General,
}

impl SceneKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Portrait => "portrait",
            Self::Group => "group portrait",
            Self::Landscape => "outdoor (sky or greenery)",
            Self::LowLight => "low light",
            Self::Night => "night scene with light sources",
            Self::HighKey => "high-key (bright by design)",
            Self::General => "general scene",
        }
    }
}

/// Global decisions for one photograph.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalPlan {
    pub kind: SceneKind,
    pub white_balance: Option<(u32, i16)>,
    pub vibrance: i16,
    pub clarity: i16,
    pub dehaze: i16,
    pub extra_shadows: i16,
    pub whites: i16,
    pub blacks: i16,
    /// Amount, radius, detail, masking.
    pub sharpen: (i16, f32, i16, i16),
    /// Luminance and colour noise reduction, when the measured noise needs any.
    pub noise: Option<(i16, i16)>,
    pub sky: Option<Edit>,
    pub decisions: Vec<String>,
}

/// Measurements of the display-referred frame, with faces excluded where it matters.
#[derive(Debug, Clone, Copy, Default)]
struct Measure {
    saturation: f32,
    sky: f32,
    horizon: f32,
    sky_luma: f32,
    /// Sky-coloured share of the bottom 30 %: a blue backdrop, not a sky, when high.
    sky_bottom: f32,
    foliage: f32,
    median: f32,
    high: f32,
    /// 2nd and 98th percentiles of display lightness: the tonal range actually used.
    p02: f32,
    p98: f32,
    dark_floor: f32,
    noise: f32,
    /// Share of the frame darker than 20 % display lightness.
    dark_share: f32,
    /// How far colours spread around the frame's average colour. Near zero with a visible
    /// average colour is a toned monochrome (sepia, cyanotype), not a colour cast.
    hue_spread: f32,
}

/// A tonal intent that a histogram would mistake for an exposure error.
fn intent(m: &Measure) -> Option<SceneKind> {
    if m.median < 0.16 && m.p98 > 0.8 && m.dark_share > 0.55 {
        Some(SceneKind::Night)
    } else if m.median > 0.72 && m.p98 - m.p02 > 0.12 {
        Some(SceneKind::HighKey)
    } else {
        None
    }
}

/// Highlights as evidence of underexposure. A finished photograph has something near white in
/// it - cloth, paper, a window, a specular - and a frame whose very brightest tones stop well
/// short of white, with nothing saying the darkness is the scene, was exposed too low. Returns
/// the lift in EV and where the brightest tones sit, or `None` when the frame already reaches
/// white. The lift is only ever part of the room there is, so it cannot clip what it measured.
fn highlight_lift(px: &Pixels<'_>) -> Option<(f32, f32)> {
    let (w, h) = (px.width, px.height);
    let mut display: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| aura_raw::colour::curve::srgb_encode(luma(px.linear(x, y))))
        .collect();
    let top = percentile(&mut display, 0.995);
    let room = (aura_raw::colour::curve::srgb_decode(0.95)
        / aura_raw::colour::curve::srgb_decode(top).max(0.005))
    .log2();
    (room > 0.5).then(|| ((room * 0.8).min(1.0), top))
}

fn big_faces(faces: &[PortraitFace]) -> usize {
    faces
        .iter()
        .filter(|f| {
            let [l, t, r, b] = f.bounds;
            (r - l) * (b - t) >= 0.008
        })
        .count()
}

/// Temper the histogram's tone correction the way an editor reads a frame before touching it.
///
/// A histogram pulls every median toward middle grey. Three things it cannot know: a night
/// scene and a high-key one are that way on purpose; a frame whose brightest tones already
/// reach white is exposed correctly however dark its midtones are, so those are lifted with
/// shadows rather than exposure; and darkening a frame in which nothing is near clipping only
/// turns its whites grey. Frames with people are left to [`face_exposure_cap`].
pub fn respect_intent(
    tone: &mut (f32, i16, i16, i16),
    px: &Pixels<'_>,
    faces: &[PortraitFace],
) -> Option<String> {
    if big_faces(faces) > 0 {
        return None;
    }
    let m = measure(px, faces);
    let before = *tone;
    let mut why = Vec::new();
    // Highlights decide exposure. Past the room the brightest tones leave, the rest of the
    // lift goes to the shadows, which is where a dark-looking but well-exposed frame needs it.
    // And, as with a colour cast, only part of the measured difference is removed: a frame
    // darker than middle grey is very often meant to be.
    let headroom = (0.97 / aura_raw::colour::curve::srgb_decode(m.p98).max(0.005))
        .log2()
        .max(0.0)
        + 0.1;
    let wanted = tone.0 * 0.7;
    if tone.0 > 0.0 && wanted.min(headroom) + 1e-3 < tone.0 {
        let excess = tone.0 - wanted.min(headroom);
        tone.0 = wanted.min(headroom);
        tone.2 = (tone.2 + (excess * 30.0).round() as i16).min(30);
        why.push(format!(
            "part of the lift is given to the shadows instead ({1:+}), which keeps the brightest tones ({0:.0}% before the edit) and the frame's mood",
            m.p98 * 100.0,
            tone.2
        ));
    }
    if tone.0 < -1e-3 && m.p98 < 0.95 {
        tone.0 = 0.0;
        why.push("nothing is near clipping, so darkening would only grey the whites".to_owned());
    }
    // Without people to confirm it, take less of the room, and less again in a frame that is
    // dark throughout, where dimness is as likely to be the place as the exposure.
    if let Some((lift, top)) = highlight_lift(px) {
        let lift = (lift * 0.75).min(if m.median < 0.22 { 0.5 } else { 1.0 });
        if lift > tone.0 + 1e-3 {
            tone.0 = lift;
            why.push(format!(
                "nothing in the frame comes near white (its brightest tones reach {:.0}%), so it is lifted as underexposed",
                top * 100.0
            ));
        }
    }
    // Intent has the last word: neither rule above may undo it.
    match intent(&m) {
        Some(SceneKind::Night) => {
            tone.0 = tone.0.min(0.15);
            tone.2 = tone.2.min(5);
            if before.0 > tone.0 + 1e-3 {
                why.push(
                    "the darkness is the scene, and brightening it would turn night into grey"
                        .to_owned(),
                );
            }
        }
        Some(SceneKind::HighKey) => {
            tone.0 = tone.0.max(0.0);
            tone.1 = tone.1.max(-8);
            if before.0 < -1e-3 || before.1 < tone.1 {
                why.push(
                    "the frame is bright by design, and darkening it would turn white into grey"
                        .to_owned(),
                );
            }
        }
        _ => {}
    }
    (!why.is_empty()).then(|| {
        format!(
            "Exposure {:+.2} EV (the histogram asked for {:+.2}): {}.",
            tone.0,
            before.0,
            why.join("; ")
        )
    })
}

/// The frame readings the scene decisions rest on, for the real-photo harness.
#[must_use]
pub fn readings(px: &Pixels<'_>, faces: &[PortraitFace]) -> String {
    let m = measure(px, faces);
    format!(
        "median {:.2} p02 {:.2} p98 {:.2} dark share {:.2} saturation {:.2} hue spread {:.3} sky {:.2} foliage {:.2} noise {:.4}",
        m.median, m.p02, m.p98, m.dark_share, m.saturation, m.hue_spread, m.sky, m.foliage, m.noise
    )
}

fn in_faces(faces: &[PortraitFace], x: f32, y: f32) -> bool {
    faces.iter().any(|f| {
        let [l, t, r, b] = f.bounds;
        let (w, h) = (r - l, b - t);
        // Include hair and neck: a face's own colours are never evidence about the light.
        x >= l - w * 0.35 && x <= r + w * 0.35 && y >= t - h * 0.35 && y <= b + h * 0.9
    })
}

fn percentile(values: &mut [f32], p: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    let i = ((values.len() - 1) as f32 * p).round() as usize;
    values.get(i).copied().unwrap_or(0.0)
}

fn measure(px: &Pixels<'_>, faces: &[PortraitFace]) -> Measure {
    let (w, h) = (px.width, px.height);
    let step = ((w * h) as f32 / 250_000.0).sqrt().ceil().max(1.0) as usize;
    let mut sat = Vec::new();
    let mut lum = Vec::new();
    let mut floor = Vec::new();
    let mut sky = 0_usize;
    let mut sky_luma = 0.0;
    let mut top = 0_usize;
    let mut bottom = (0_usize, 0_usize);
    let mut foliage = 0_usize;
    let mut rows = vec![(0_usize, 0_usize); 20];
    let mut n = 0_usize;
    let mut dark = 0_usize;
    // Log-chroma sums for the spread of colour around the frame's average.
    let mut hue = [0.0_f64; 5];
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let fx = (x as f32 + 0.5) / w as f32;
            let fy = (y as f32 + 0.5) / h as f32;
            let p = px.encoded(x, y);
            let max = p[0].max(p[1]).max(p[2]);
            let min = p[0].min(p[1]).min(p[2]);
            let s = if max > 1e-4 { (max - min) / max } else { 0.0 };
            let l = p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722;
            lum.push(l);
            dark += usize::from(l < 0.2);
            // Dark pixels carry more quantisation than colour; they are not evidence of hue.
            if min > 0.15 && max < 0.96 {
                let (a, b) = (f64::from((p[0] / p[1]).ln()), f64::from((p[2] / p[1]).ln()));
                hue[0] += 1.0;
                hue[1] += a;
                hue[2] += b;
                hue[3] += a * a;
                hue[4] += b * b;
            }
            let face = in_faces(faces, fx, fy);
            if !face {
                sat.push(s);
            }
            n += 1;
            // Blue sky only: a bright white tabletop or wall is not evidence of sky.
            let is_sky = p[2] > p[0] * 1.08 && p[2] >= p[1] * 0.95 && l > 0.4 && s > 0.12;
            if fy < 0.6 {
                let band = ((fy / 0.03) as usize).min(19);
                if let Some(row) = rows.get_mut(band) {
                    row.0 += 1;
                    row.1 += usize::from(is_sky && !face);
                }
            }
            if fy < 0.45 {
                top += 1;
                if is_sky && !face {
                    sky += 1;
                    sky_luma += l;
                }
            }
            if fy > 0.7 {
                bottom.0 += 1;
                bottom.1 += usize::from(is_sky);
            }
            if !is_sky && !face {
                floor.push(min);
            }
            if p[1] > p[0] * 1.05 && p[1] > p[2] * 1.05 && s > 0.15 {
                foliage += 1;
            }
        }
    }
    let horizon = rows
        .iter()
        .enumerate()
        .take_while(|(_, (count, skies))| *count > 0 && *skies * 10 > *count * 4)
        .last()
        .map_or(0.0, |(band, _)| (band as f32 + 1.0) * 0.03);
    let mut s = Measure {
        saturation: if sat.is_empty() {
            0.0
        } else {
            sat.iter().sum::<f32>() / sat.len() as f32
        },
        sky: if top == 0 {
            0.0
        } else {
            sky as f32 / top as f32
        },
        horizon,
        sky_luma: if sky == 0 { 0.0 } else { sky_luma / sky as f32 },
        sky_bottom: if bottom.0 == 0 {
            0.0
        } else {
            bottom.1 as f32 / bottom.0 as f32
        },
        foliage: if n == 0 {
            0.0
        } else {
            foliage as f32 / n as f32
        },
        median: percentile(&mut lum, 0.5),
        high: percentile(&mut lum, 0.95),
        p02: percentile(&mut lum, 0.02),
        p98: percentile(&mut lum, 0.98),
        dark_floor: percentile(&mut floor, 0.05),
        noise: 0.0,
        dark_share: if n == 0 { 0.0 } else { dark as f32 / n as f32 },
        hue_spread: if hue[0] < 50.0 {
            1.0
        } else {
            let var = (hue[3] / hue[0] - (hue[1] / hue[0]).powi(2))
                + (hue[4] / hue[0] - (hue[2] / hue[0]).powi(2));
            var.max(0.0).sqrt() as f32
        },
    };
    s.noise = noise_sigma(px);
    s
}

/// Immerkaer's fast noise estimate over flat pixels, in display-encoded units.
pub(crate) fn noise_sigma(px: &Pixels<'_>) -> f32 {
    let (w, h) = (px.width, px.height);
    if w < 8 || h < 8 {
        return 0.0;
    }
    let l = |x: usize, y: usize| {
        let p = px.encoded(x, y);
        p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722
    };
    let step = ((w * h) as f32 / 400_000.0).sqrt().ceil().max(1.0) as usize;
    let mut responses = Vec::new();
    for y in (1..h - 1).step_by(step) {
        for x in (1..w - 1).step_by(step) {
            let c = l(x, y);
            let (n, s, e, west) = (l(x, y - 1), l(x, y + 1), l(x + 1, y), l(x - 1, y));
            let gradient = (e - west).abs() + (s - n).abs();
            // Edges and texture are signal, not noise.
            if gradient > 0.04 || !(0.03..=0.97).contains(&c) {
                continue;
            }
            let laplacian = l(x - 1, y - 1) - 2.0 * n + l(x + 1, y - 1) - 2.0 * west + 4.0 * c
                - 2.0 * e
                + l(x - 1, y + 1)
                - 2.0 * s
                + l(x + 1, y + 1);
            responses.push(laplacian.abs());
        }
    }
    if responses.len() < 200 {
        return 0.0;
    }
    // A median resists the few edges that slipped through the flatness test.
    let median = percentile(&mut responses, 0.5);
    median * (std::f32::consts::PI / 2.0).sqrt() / 6.0 * 1.4826
}

/// The outcome of the white-balance measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Neutral {
    /// Both estimates agree on a cast; correct part of it.
    Correct {
        kelvin: u32,
        tint: i16,
        cast: f32,
        coverage: f32,
        /// The share of the measured cast that is removed.
        strength: f32,
        /// The frame's brightest tones show the same cast.
        confirmed: bool,
    },
    /// Both estimates agree the light is already neutral.
    Neutral { coverage: f32 },
    /// Too few neutral areas, or the two estimates disagree (a coloured backdrop).
    Unsure { reason: &'static str },
}

/// Two independent illuminant estimates on the renderer's own input pixels, faces excluded:
/// near-neutral pixels (gray pixels) and the average colour of edges (gray edge). A large
/// coloured backdrop moves the first and barely the second, so a correction is only made
/// when both agree.
fn white_balance(frame: &aura_render::Frame, faces: &[PortraitFace]) -> Neutral {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if w < 4 || h < 4 {
        return Neutral::Unsure {
            reason: "the frame is too small to measure",
        };
    }
    let step = ((w * h) as f32 / 200_000.0).sqrt().ceil().max(1.0) as usize;
    let at = |x: usize, y: usize| -> Option<[f32; 3]> {
        let i = (y * w + x) * 3;
        let p = frame.rgb.get(i..i + 3)?;
        Some([p.first().copied()?, p.get(1).copied()?, p.get(2).copied()?])
    };
    let mut samples = Vec::new();
    let mut edges = [0.0_f64; 3];
    let mut total = 0_usize;
    for y in (0..h - 1).step_by(step) {
        for x in (0..w - 1).step_by(step) {
            total += 1;
            if in_faces(
                faces,
                (x as f32 + 0.5) / w as f32,
                (y as f32 + 0.5) / h as f32,
            ) {
                continue;
            }
            let (Some(p), Some(right), Some(down)) = (at(x, y), at(x + 1, y), at(x, y + 1)) else {
                continue;
            };
            if p.iter()
                .chain(&right)
                .chain(&down)
                .any(|v| !v.is_finite() || *v < 0.004 || *v > 0.9)
            {
                continue;
            }
            for (edge, ((a, r), d)) in edges.iter_mut().zip(p.iter().zip(&right).zip(&down)) {
                *edge += f64::from((a - r).abs() + (a - d).abs());
            }
            let (r, g, b) = (p[0], p[1], p[2]);
            if r < 0.01 || g < 0.01 || b < 0.01 {
                continue;
            }
            let (lr, lb) = ((r / g).ln(), (b / g).ln());
            if lr.abs() < 0.8 && lb.abs() < 0.8 {
                samples.push((lr, lb, luma([r, g, b])));
            }
        }
    }
    if total == 0 || edges.iter().any(|e| *e <= 1e-6) {
        return Neutral::Unsure {
            reason: "the frame has no usable detail",
        };
    }
    let edge = (
        (edges[0] / edges[1]).ln() as f32,
        (edges[2] / edges[1]).ln() as f32,
    );
    // Seed the gray-pixel search from the edge estimate: near-neutral pixels that agree with
    // the edges are surfaces lit by the scene's light, while a large coloured backdrop sits
    // far from it and is ignored.
    let mut centre = edge;
    let mut used = 0;
    for radius in [0.2_f32, 0.12, 0.08] {
        let mut sum = (0.0, 0.0);
        let mut weight = 0.0;
        used = 0;
        for (lr, lb, l) in &samples {
            if (lr - centre.0).hypot(lb - centre.1) <= radius {
                let k = l.sqrt();
                sum.0 += lr * k;
                sum.1 += lb * k;
                weight += k;
                used += 1;
            }
        }
        if weight <= 0.0 {
            return Neutral::Unsure {
                reason: "not enough reliable neutral areas",
            };
        }
        centre = (sum.0 / weight, sum.1 / weight);
    }
    let coverage = used as f32 / total as f32;
    if coverage < 0.015 {
        return Neutral::Unsure {
            reason: "not enough reliable neutral areas",
        };
    }
    // With people in frame the edges include warm skin-adjacent detail, so allow a little
    // more disagreement there; without people, stay strict.
    let tolerance = if faces.is_empty() { 0.1 } else { 0.15 };
    if (centre.0 - edge.0).hypot(centre.1 - edge.1) > tolerance {
        return Neutral::Unsure {
            reason: "the neutral areas and the edges disagree about the light (a coloured backdrop or mixed light)",
        };
    }
    let estimate = ((centre.0 + edge.0) * 0.5, (centre.1 + edge.1) * 0.5);
    // A third, independent reading: the brightest unclipped tones. Whites, paper, cloth and
    // speculars take the colour of the light and nothing else, so when they are neutral the
    // light is, whatever the scene's own colours add up to - a finished photograph with warm
    // wood or foliage in it reads as a cast to the other two. And when they show the same cast,
    // more of it can be removed with confidence.
    let whites = white_patch(&samples);
    let confirmed = match whites {
        Some(patch) if patch.0.hypot(patch.1) < 0.07 && estimate.0.hypot(estimate.1) >= 0.04 => {
            return Neutral::Unsure {
                reason: "the brightest tones in the frame are neutral, so the light is; the colour elsewhere belongs to the scene",
            };
        }
        // A handful of near-neutral pixels agreeing with each other is one coloured object,
        // not three witnesses; the whites only add confidence where the neutral area is real.
        Some(patch) => coverage >= 0.1 && (estimate.0 - patch.0).hypot(estimate.1 - patch.1) <= 0.1,
        None => false,
    };
    let estimate = match whites {
        Some(patch) if confirmed => (
            (centre.0 + edge.0 + patch.0) / 3.0,
            (centre.1 + edge.1 + patch.1) / 3.0,
        ),
        _ => estimate,
    };
    let cast = estimate.0.hypot(estimate.1);
    if cast < 0.04 {
        return Neutral::Neutral { coverage };
    }
    // Without people, a strong colour is usually the light itself - a sunset, blue hour or
    // stage lighting - and removing it removes the photograph's reason for being.
    if faces.is_empty() && cast > 0.2 {
        return Neutral::Unsure {
            reason: "a strong colour with no people in frame reads as the light's mood (sunset, blue hour or stage light)",
        };
    }
    // Keep part of the light's character: correct mild casts more than strong ones, so a
    // tungsten reception still reads as warm evening light rather than a studio.
    let strength = match (confirmed, cast > 0.25) {
        (true, true) => 0.7,
        (true, false) => 0.85,
        (false, true) => 0.5,
        (false, false) => 0.65,
    };
    let virtual_gray = [
        0.18 * (estimate.0 * strength).exp(),
        0.18,
        0.18 * (estimate.1 * strength).exp(),
    ];
    match crate::studio_tools::neutral_white_balance(virtual_gray) {
        Ok((k, t)) => Neutral::Correct {
            kelvin: k.clamp(2800, 11_000),
            tint: t.clamp(-40, 40),
            cast,
            coverage,
            strength,
            confirmed,
        },
        Err(_) => Neutral::Unsure {
            reason: "the measured cast is outside the supported range",
        },
    }
}

/// The colour of the brightest unclipped near-neutral tones, as `(ln r/g, ln b/g)`. `None`
/// when there are too few of them, they are not bright, or they do not agree with each other
/// (coloured lights rather than white things).
fn white_patch(samples: &[(f32, f32, f32)]) -> Option<(f32, f32)> {
    if samples.len() < 400 {
        return None;
    }
    let mut lumas: Vec<f32> = samples.iter().map(|sample| sample.2).collect();
    let floor = percentile(&mut lumas, 0.97);
    if floor <= 0.25 {
        return None;
    }
    let (mut red, mut blue): (Vec<f32>, Vec<f32>) = samples
        .iter()
        .filter(|sample| sample.2 >= floor)
        .map(|sample| (sample.0, sample.1))
        .unzip();
    if red.len() < 30 {
        return None;
    }
    let deviation = |values: &[f32]| {
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
    };
    if deviation(&red).hypot(deviation(&blue)) >= 0.16 {
        return None;
    }
    Some((percentile(&mut red, 0.5), percentile(&mut blue, 0.5)))
}

/// Decide the global adjustments for one photograph.
#[must_use]
pub fn analyse(
    px: &Pixels<'_>,
    frame: Option<&aura_render::Frame>,
    faces: &[PortraitFace],
    exposure: f32,
) -> GlobalPlan {
    let m = measure(px, faces);
    let big_faces = big_faces(faces);
    let kind = if big_faces >= 3 {
        SceneKind::Group
    } else if big_faces >= 1 {
        SceneKind::Portrait
    } else if let Some(kind) = intent(&m) {
        kind
    } else if m.median < 0.22 && m.high < 0.7 {
        SceneKind::LowLight
    } else if (m.sky > 0.25 && m.sky_bottom < 0.1) || m.foliage > 0.3 {
        SceneKind::Landscape
    } else {
        SceneKind::General
    };
    let mut decisions = vec![format!(
        "Scene measured as {} ({} face{} large enough to retouch).",
        kind.label(),
        big_faces,
        if big_faces == 1 { "" } else { "s" }
    )];
    // One colour throughout, and visibly not grey: a toned black-and-white print.
    let toned = m.hue_spread < 0.08 && m.saturation > 0.02 && m.saturation < 0.5;
    // Vibrance: less for already colourful frames, never on skin-heavy portraits past +10.
    let base: f32 = match kind {
        // Vibrance moves skin more than anything else in a portrait; stay very light.
        SceneKind::Portrait | SceneKind::Group => 4.0,
        SceneKind::Landscape => 18.0,
        SceneKind::LowLight | SceneKind::Night => 6.0,
        SceneKind::HighKey => 8.0,
        SceneKind::General => 12.0,
    };
    let colour = if toned || m.saturation > 0.45 {
        0.0
    } else if m.saturation > 0.35 {
        0.5
    } else if m.saturation < 0.15 && m.saturation > 0.02 {
        1.4
    } else if m.saturation <= 0.02 {
        0.0
    } else {
        1.0
    };
    let vibrance = (base * colour).round().clamp(0.0, 20.0) as i16;
    decisions.push(if vibrance > 0 {
        format!(
            "Vibrance +{vibrance}: average saturation measured {:.0}%.",
            m.saturation * 100.0
        )
    } else {
        format!(
            "Vibrance unchanged: the frame is {} ({:.0}% average saturation).",
            if m.saturation <= 0.02 {
                "monochrome"
            } else if toned {
                "a toned monochrome, and its single colour is the look"
            } else {
                "already colourful"
            },
            m.saturation * 100.0
        )
    });
    let clarity = match kind {
        SceneKind::Landscape => 12,
        SceneKind::General => 8,
        _ => 0,
    };
    if clarity > 0 {
        decisions.push(format!(
            "Clarity +{clarity} for structure; no faces to protect."
        ));
    }
    // Haze is an outdoor property; a bright studio backdrop has a high floor too.
    let dehaze =
        if kind == SceneKind::Landscape && m.dark_floor > 0.13 && (m.high - m.dark_floor) < 0.75 {
            let amount = ((m.dark_floor - 0.1) * 150.0).clamp(5.0, 25.0) as i16;
            decisions.push(format!(
                "Dehaze +{amount}: the darkest non-sky tones sit at {:.0}% (a veil of haze).",
                m.dark_floor * 100.0
            ));
            amount
        } else {
            0
        };
    // A flat frame that never reaches black or white: set the end points, like the
    // Shift-double-click on Whites and Blacks. Low-light frames keep their dark floor.
    // Judge the range after the exposure this pass applies, or a brightened frame would be
    // stretched twice.
    let after = |v: f32| {
        aura_raw::colour::curve::srgb_encode(
            (aura_raw::colour::curve::srgb_decode(v) * exposure.exp2()).min(1.0),
        )
    };
    let (p02, p98) = (after(m.p02), after(m.p98));
    let has_range = p98 - p02 > 0.1;
    let blacks =
        if matches!(kind, SceneKind::LowLight | SceneKind::Night) || p02 <= 0.08 || !has_range {
            0
        } else if kind == SceneKind::HighKey {
            // A high-key frame has no black in it on purpose; anchor it only lightly.
            -((p02 - 0.04) * 150.0).clamp(0.0, 8.0) as i16
        } else if dehaze > 0 {
            // Dehaze already deepens the floor; setting the black point as well doubles it.
            -((p02 - 0.04) * 75.0).clamp(0.0, 15.0) as i16
        } else {
            -((p02 - 0.04) * 150.0).clamp(0.0, 25.0) as i16
        };
    let whites = if p98 >= 0.9 || !has_range {
        0
    } else {
        ((0.95 - p98) * 150.0).clamp(0.0, 20.0) as i16
    };
    if blacks != 0 || whites != 0 {
        decisions.push(format!(
            "Tonal range stretched (whites {whites:+}, blacks {blacks:+}): the frame used only {:.0}%-{:.0}% of the range.",
            p02 * 100.0,
            p98 * 100.0
        ));
    }
    let extra_shadows = if kind == SceneKind::LowLight {
        decisions.push("Shadows lifted a further +10 for a low-light frame.".into());
        10
    } else {
        0
    };
    let noise = if m.noise > 0.008 {
        let luminance = ((m.noise - 0.006) * 2000.0).clamp(8.0, 45.0) as i16;
        let colour = (luminance + 10).clamp(15, 50);
        decisions.push(format!(
            "Noise reduction {luminance}/{colour}: measured noise {:.2}% of full scale.",
            m.noise * 100.0
        ));
        Some((luminance, colour))
    } else {
        None
    };
    let sharpen = match kind {
        SceneKind::Portrait | SceneKind::Group => (20, 1.0, 25, 70),
        SceneKind::Landscape => (35, 1.0, 30, 40),
        SceneKind::LowLight | SceneKind::Night => (15, 1.0, 20, 75),
        SceneKind::HighKey => (25, 1.0, 25, 60),
        SceneKind::General => (30, 1.0, 25, 50),
    };
    let sharpen = if noise.is_some() {
        (sharpen.0 / 2, sharpen.1, sharpen.2, sharpen.3.max(70))
    } else {
        sharpen
    };
    decisions.push(format!(
        "Sharpening {} with {}% edge masking{}.",
        sharpen.0,
        sharpen.3,
        if matches!(kind, SceneKind::Portrait | SceneKind::Group) {
            " so skin texture is not exaggerated"
        } else {
            ""
        }
    ));
    let kept_light = if toned {
        Some("the frame is a toned monochrome; its colour is the look, not a cast")
    } else if kind == SceneKind::Night {
        Some("at night the light sources are the colour of the scene")
    } else {
        None
    };
    let white_balance = match frame.map(|f| {
        kept_light.map_or_else(
            || white_balance(f, faces),
            |reason| Neutral::Unsure { reason },
        )
    }) {
        Some(Neutral::Correct {
            kelvin,
            tint,
            coverage,
            strength,
            confirmed,
            ..
        }) => {
            decisions.push(format!(
                "White balance {kelvin} K / tint {tint:+}: two independent estimates agreed on a cast{}; removed about {:.0}% of it, measured on {:.0}% neutral areas with faces ignored.",
                if confirmed {
                    ", and the frame's brightest tones show the same one"
                } else {
                    ""
                },
                strength * 100.0,
                coverage * 100.0
            ));
            Some((kelvin, tint))
        }
        Some(Neutral::Neutral { coverage }) => {
            decisions.push(format!(
                "White balance kept: neutral areas ({:.0}% of the frame) are already neutral.",
                coverage * 100.0
            ));
            None
        }
        Some(Neutral::Unsure { reason }) => {
            decisions.push(format!("White balance kept: {reason}."));
            None
        }
        None => None,
    };
    let sky = (kind == SceneKind::Landscape
        && m.sky > 0.2
        && m.sky_bottom < 0.1
        && m.sky_luma > 0.55
        && m.horizon > 0.08)
        .then(|| {
            decisions.push(format!(
                "Sky: balanced the bright sky above {:.0}% of the frame height with a feathered gradient.",
                m.horizon * 100.0
            ));
            Edit {
                id: format!("{}sky", portrait_auto::SCENE_PREFIX),
                tool: Tool::Burn,
                enabled: true,
                region: [0.5, 0.5, 0.5, 0.5],
                source: None,
                amount: 0.3,
                feather: 0.7,
                radius: 0.002,
                source_scale: 1.0,
                preserve_microtexture: false,
                texture: 1.0,
                texture_heal: false,
                clean_ring_fit: false,
                curved_heal: false,
                heal_samples: Vec::new(),
                texture_sources: Vec::new(),
                sensitivity: None,
                keep_dark_marks: false,
                tone: 0.5,
                warmth: 0.0,
                tint: 0.0,
                mask: None,
                skin: None,
                matte: None,
                selection: Some(Selection {
                    inverted: false,
                    gradient: Some(Gradient {
                        start: [0.5, m.horizon.min(0.95)],
                        end: [0.5, 0.0],
                    }),
                    luminance: Some(LuminanceRange {
                        low: 0.5,
                        high: 16.0,
                        softness: 1.0,
                    }),
                }),
            }
        });
    GlobalPlan {
        kind,
        white_balance,
        vibrance,
        clarity,
        dehaze,
        extra_shadows,
        whites,
        blacks,
        sharpen,
        noise,
        sky,
        decisions,
    }
}

/// Keep a brightening exposure from washing out the people in the frame.
///
/// Two relative checks, never a target brightness for skin: a subject that is already
/// clearly brighter than the rest of the scene is a low-key portrait and keeps its mood, and
/// no face may be pushed into clipping.
fn face_exposure_cap(
    exposure: f32,
    px: &Pixels<'_>,
    faces: &[PortraitFace],
) -> (f32, Option<String>) {
    if faces.is_empty() {
        return (exposure, None);
    }
    // The histogram correction keeps uniformly dark frames within 0.25 EV because it cannot
    // tell a silhouette from underexposure. A detected face says it is not a silhouette.
    let requested = exposure;
    let (w, h) = (px.width, px.height);
    let mut all: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| luma(px.linear(x, y)))
        .collect();
    let median = percentile(&mut all, 0.5);
    let high = aura_raw::colour::curve::srgb_encode(percentile(&mut all, 0.95));
    let raised =
        (high < 0.6 && exposure >= 0.0).then(|| (0.18 / median.max(0.002)).log2().clamp(0.0, 1.5));
    let (w, h) = (px.width, px.height);
    let mut face = Vec::new();
    let mut frame = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let q = px.encoded(x, y);
            let l = q[0] * 0.2126 + q[1] * 0.7152 + q[2] * 0.0722;
            let (fx, fy) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            frame.push(l);
            if faces.iter().any(|f| {
                let [l0, t0, r0, b0] = f.bounds;
                let (mw, mh) = ((r0 - l0) * 0.2, (b0 - t0) * 0.2);
                fx > l0 + mw && fx < r0 - mw && fy > t0 + mh && fy < b0 - mh
            }) {
                face.push(luma(px.linear(x, y)));
            }
        }
    }
    if face.len() < 16 {
        return (exposure, None);
    }
    let frame_median = percentile(&mut frame, 0.5);
    let frame_high = percentile(&mut frame, 0.95);
    let face_high = percentile(&mut face, 0.98);
    let face_median = aura_raw::colour::curve::srgb_encode(percentile(&mut face, 0.5));
    // Skin at or near clipping is overexposure whatever the histogram or the background says:
    // there is no detail left in it to keep. This is a ceiling, never a target brightness.
    let hot = face.iter().filter(|value| **value > 0.85).count() as f32 / face.len() as f32;
    if hot > 0.12 {
        let down = -(0.2 + (hot - 0.12) * 2.0).min(0.7);
        if down < exposure {
            return (
                down,
                Some(format!(
                    "Exposure {down:+.2} EV: {:.0}% of the skin on the detected faces is at or near clipping, which is overexposure whatever the rest of the frame looks like.",
                    hot * 100.0
                )),
            );
        }
    }
    // A dark frame around a face is evidence of underexposure only when the face itself is
    // dark. Dark hair, dark clothes and a grey wall around a well-lit face are not, and
    // brightening them would lighten somebody's skin for no photographic reason.
    let exposure = match raised {
        Some(raised) if face_median < 0.3 => exposure.max(raised),
        _ => exposure,
    };
    // And when the people are not darker than the frame around them, the frame's darkness is
    // not theirs: at most a small lift, however far the median sits from middle grey.
    let surroundings = exposure > 0.25 && face_median >= frame_median;
    let exposure = if surroundings { 0.25 } else { exposure };
    // The surroundings rule answers a histogram that wants middle grey. Highlights are a
    // different witness: when nothing in the frame, the people included, comes near white, the
    // whole frame is low, and a night scene is the one exception.
    let night = intent(&measure(px, faces)) == Some(SceneKind::Night);
    let anchor = highlight_lift(px).filter(|(lift, _)| !night && *lift > exposure);
    let exposure = anchor.map_or(exposure, |(lift, _)| lift);
    // Nor is a white wall evidence that the people in front of it are overexposed.
    if exposure < 0.0 && face_median < 0.6 {
        return (
            0.0,
            Some(format!(
                "Exposure kept (the histogram asked for {exposure:+.2} EV): the bright background is not the people, who are not overexposed; Highlights handles the background."
            )),
        );
    }
    if exposure <= 0.0 {
        return (exposure, None);
    }
    let headroom = (0.9 / face_high.max(1e-3)).log2().max(0.0);
    let mut capped = exposure.min(headroom);
    // Low key: the frame already reaches bright tones and the people are its brightest part,
    // so the darkness around them is a choice. An underexposed frame never reaches bright.
    let low_key = frame_high > 0.75 && face_median > frame_median * 1.25;
    if low_key {
        capped = capped.min(0.15);
    }
    if let Some((_, top)) = anchor {
        return (
            capped,
            Some(format!(
                "Exposure {capped:+.2} EV (the histogram asked for {requested:+.2}): nothing in the frame, the people included, comes near white (its brightest tones reach {:.0}%), which is underexposure rather than a mood. The lift stops before any face would clip.",
                top * 100.0
            )),
        );
    }
    if capped > requested + 1e-3 {
        return (
            capped,
            Some(format!(
                "Exposure raised to {capped:+.2} EV: the frame is dark throughout, and the detected faces show it is underexposed rather than a silhouette."
            )),
        );
    }
    if capped + 1e-3 < exposure {
        (
            capped,
            Some(format!(
                "Exposure limited to {capped:+.2} EV (the histogram asked for {exposure:+.2}): {}.",
                if low_key {
                    "the people are already brighter than the scene around them, so the darker mood is kept"
                } else {
                    "brightening further would clip highlights on a face"
                }
            )),
        )
    } else if surroundings {
        (
            exposure,
            Some(format!(
                "Exposure limited to {exposure:+.2} EV (the histogram asked for {requested:+.2}): the people are not darker than the frame around them, so its dark hair, clothes or background are not a reason to lighten them."
            )),
        )
    } else {
        (exposure, None)
    }
}

fn apply_global(recipe: &mut Recipe, tone: (f32, i16, i16, i16), plan: &GlobalPlan) {
    let g = &mut recipe.global;
    let (exposure, highlights, shadows, contrast) = tone;
    g.exposure = exposure;
    g.highlights = highlights;
    g.shadows = (shadows + plan.extra_shadows).clamp(-100, 100);
    g.contrast = contrast;
    if let Some((kelvin, tint)) = plan.white_balance {
        g.temperature = kelvin;
        g.tint = tint;
    }
    g.whites = plan.whites;
    g.blacks = plan.blacks;
    g.vibrance = plan.vibrance;
    g.clarity = plan.clarity;
    g.dehaze = plan.dehaze;
    let (amount, radius, detail, masking) = plan.sharpen;
    g.sharpen.amount = amount;
    g.sharpen.radius = radius;
    g.sharpen.detail = detail;
    g.sharpen.masking = masking;
    if let Some((luminance, colour)) = plan.noise {
        g.noise.luminance = luminance;
        g.noise.colour = colour;
        g.noise.detail = 50;
    }
}

/// Selections must use the exposure that survives the recipe merge.
fn effective_exposure(base: &Recipe, proposed: f32, global: bool) -> f32 {
    if global
        && !base
            .provenance
            .user_edited_fields
            .iter()
            .any(|field| field == "global.exposure" || field == "global")
    {
        proposed
    } else {
        base.global.exposure
    }
}

fn saved_changed(base: &Recipe, merged: &Recipe) -> aura_core::AuraResult<bool> {
    Ok(aura_recipe::recipe_hash(base)? != aura_recipe::recipe_hash(merged)?)
}

fn count(edits: &BTreeMap<Group, Vec<Edit>>, group: Group) -> usize {
    edits.get(&group).map_or(0, Vec::len)
}

/// Measure, plan and save the automatic edit as a series of undoable steps.
///
/// With `global` false only portrait retouch runs; exposure, colour and the look are left
/// exactly as they are.
/// # Errors
/// A typed decode, analysis or recipe storage error.
pub fn run(state: &AppState, input: &DevelopImageInput, global: bool) -> IpcResult<RecipeDto> {
    run_with(state, &input.photo_id, global, None)
}

/// A photographer's request to re-run automatic retouch with their own choices.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoRetouchInput {
    pub project_id: String,
    pub photo_id: String,
    /// Also measure light and colour, as Auto enhance does.
    #[serde(default)]
    pub global: bool,
    pub options: crate::portrait_features::Options,
}

/// Re-run automatic editing with chosen finishing options and intensity.
/// # Errors
/// Membership, decode, analysis or storage failures.
pub fn auto_retouch(state: &AppState, input: &AutoRetouchInput) -> IpcResult<RecipeDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    run_with(
        state,
        &input.photo_id,
        input.global,
        Some(input.options.sanitised()),
    )
}

fn run_with(
    state: &AppState,
    photo_id: &str,
    global: bool,
    chosen: Option<crate::portrait_features::Options>,
) -> IpcResult<RecipeDto> {
    let input = DevelopImageInput {
        photo_id: photo_id.to_owned(),
    };
    let invalid = |message: &str| aura_core::errors::render::recipe_invalid("photo", message);
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo identifier"))?;
    let project_id: String = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [&input.photo_id],
            |row| row.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("enhance photo", &e))
    })?;
    let project =
        ProjectId::from_db(&project_id).map_err(|_| invalid("Invalid project identifier"))?;
    let previews = state.previews(&project_id)?;
    let thumb = previews.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    )?;
    let Some(rgb) = thumb.as_srgb8() else {
        return Err(invalid("An sRGB preview is required").into());
    };
    // Fine features (spots, eyes, teeth, noise) need more pixels than the thumbnail has.
    let proxy = previews
        .get(
            photo,
            aura_raw::PixelLevel::Proxy2048,
            Priority::Interactive,
        )
        .ok();
    let detail = proxy
        .as_ref()
        .and_then(|p| p.as_srgb8().map(|pixels| (pixels, p.width, p.height)));
    let mut tone = crate::photo_enhance::correction(rgb)?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    // A later Auto enhance repeats whatever finishing the photographer last chose.
    let options = chosen.unwrap_or_else(|| {
        base.extra
            .get(portrait_auto::KEY)
            .and_then(|report| report.get("options"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    });
    let disabled = std::env::var_os("AURA_DISABLE_AUTO_PORTRAIT").is_some_and(|v| v == "1");
    let faces = if disabled {
        Vec::new()
    } else {
        let found = aura_vision::portrait::detect(rgb, thumb.width, thumb.height)?;
        // Group photos and full-length portraits: look again, tile by tile, on the proxy.
        match detail {
            Some((pixels, w, h)) => {
                aura_vision::portrait::detect_small_faces(pixels, w, h, &found).unwrap_or(found)
            }
            None => found,
        }
    };
    let mut exposure_note = None;
    if global {
        if let Some(px) = Pixels::new(rgb, thumb.width, thumb.height) {
            let (capped, note) = face_exposure_cap(tone.0, &px, &faces);
            tone.0 = capped;
            exposure_note = note.or(respect_intent(&mut tone, &px, &faces));
        }
    }
    // Luminance selections are evaluated after the global exposure the same pass applies.
    let exposure = effective_exposure(&base, tone.0, global);
    let portrait = portrait_auto::plan_with_faces(
        &base,
        rgb,
        thumb.width,
        thumb.height,
        detail,
        exposure,
        Some(faces.clone()),
        &options,
        // Every caller is a person pressing a button; automatic operations are replaced and
        // operations they added themselves are always kept.
        true,
    )?;
    let retouch_owned = chosen.is_some()
        || base.provenance.user_edited_fields.iter().any(|path| {
            path == retouch_tools::KEY || path.starts_with(&format!("{}.", retouch_tools::KEY))
        });
    let mut report = portrait.report.clone();
    let mut groups = portrait.groups.clone();

    let mut global_plan = None;
    if global {
        let frame = crate::photo_frames::CatalogFrames::new(state.clone())
            .frame(&photo, RenderLevel::Proxy2048)
            .ok();
        let px = detail
            .and_then(|(d, w, h)| Pixels::new(d, w, h))
            .or_else(|| Pixels::new(rgb, thumb.width, thumb.height));
        if let Some(px) = px {
            let mut plan = analyse(&px, frame.as_ref(), &faces, exposure);
            if let Some(note) = exposure_note.take() {
                plan.decisions.insert(1, note);
            }
            groups.insert(Group::Scene, plan.sky.clone().into_iter().collect());
            report.scene = Some(SceneSummary {
                kind: plan.kind.label().into(),
                decisions: plan.decisions.clone(),
            });
            global_plan = Some(plan);
        }
    }
    let retouch_protected = report.status == "protected";
    if retouch_protected {
        groups.clear();
    }

    // Plan every step before saving any, so the report can describe all of them.
    struct Step {
        group: Option<Group>,
        title: String,
        detail: String,
        operations: usize,
    }
    let mut steps = Vec::new();
    if let Some(plan) = &global_plan {
        let (exposure, highlights, shadows, contrast) = tone;
        steps.push(Step {
            group: None,
            title: "Light & colour".into(),
            detail: format!(
                "{}: exposure {exposure:+.2} EV, highlights {highlights}, shadows {}, whites {:+}, blacks {:+}, contrast {contrast:+}, vibrance {:+}{}",
                plan.kind.label(),
                (shadows + plan.extra_shadows).clamp(-100, 100),
                plan.whites,
                plan.blacks,
                plan.vibrance,
                plan.white_balance
                    .map_or(String::new(), |(k, t)| format!(", white balance {k} K / {t:+}")),
            ),
            operations: 0,
        });
        if plan.sky.is_some() {
            steps.push(Step {
                group: Some(Group::Scene),
                title: "Sky balance".into(),
                detail: "Feathered gradient over bright sky pixels only.".into(),
                operations: 1,
            });
        }
    } else {
        steps.push(Step {
            group: None,
            title: "Portrait analysis".into(),
            detail: format!("{} face(s) detected.", report.detected_faces),
            operations: 0,
        });
    }
    let spots: usize = report.assessments.iter().map(|a| a.spots_healed).sum();
    let kept: usize = report.assessments.iter().map(|a| a.marks_kept).sum();
    for (group, title, detail) in [
        (
            Group::Skin,
            match options.scope {
                crate::portrait_features::Scope::Face => "Skin",
                crate::portrait_features::Scope::Body => "Body skin",
                crate::portrait_features::Scope::FaceAndBody => "Face & body skin",
            },
            match options.scope {
                crate::portrait_features::Scope::Face => format!(
                    "Texture, tone evening and local light on {} face(s).",
                    report.retouched_faces
                ),
                crate::portrait_features::Scope::Body => format!(
                    "Texture and tone evening on visible body skin for {} person(s); faces unchanged.",
                    report.retouched_faces
                ),
                crate::portrait_features::Scope::FaceAndBody => format!(
                    "Face texture, tone and light, plus body skin texture and tone, for {} person(s).",
                    report.retouched_faces
                ),
            },
        ),
        (
            Group::Blemishes,
            "Blemishes",
            format!(
                "Applied {spots} measured spot repair(s); kept {kept} possible permanent mark(s). Review the result at full size."
            ),
        ),
        (
            Group::Refine,
            "Fine lines & redness",
            "Targeted softening measured against this person's own cheek texture and colour.".into(),
        ),
        (
            Group::Eyes,
            "Eyes",
            "Iris detail, sclera redness, red-eye and under-eye shadows where measured.".into(),
        ),
        (
            Group::Finishing,
            "Teeth & shine",
            "Teeth yellow cast and skin shine where measured.".into(),
        ),
    ] {
        if count(&groups, group) > 0 {
            steps.push(Step {
                group: Some(group),
                title: title.into(),
                detail,
                operations: count(&groups, group),
            });
        }
    }
    report.steps = steps
        .iter()
        .enumerate()
        .map(|(i, s)| StepSummary {
            step: i + 1,
            title: s.title.clone(),
            detail: s.detail.clone(),
            operations: s.operations,
        })
        .collect();

    let total = steps.len();
    let mut current = base.clone();
    for (i, step) in steps.iter().enumerate() {
        let mut proposal = current.clone();
        if i == 0 {
            if let Some(plan) = &global_plan {
                apply_global(&mut proposal, tone, plan);
            }
            portrait_auto::write_report(&mut proposal, &report)?;
        }
        // The last step reaches every group, so automatic work that is no longer planned
        // (for example teeth that an earlier pass whitened) is removed by the same pass.
        let upto = if i + 1 == total {
            Group::Finishing
        } else {
            step.group.unwrap_or(Group::Scene)
        };
        // Light and colour always merge as an automatic proposal, so a slider a person moved
        // is never overwritten. The retouch stack merges as the photographer's own edit when
        // they asked for it or have already edited it: pressing Auto again on a hand-edited
        // stack replaces only the automatic operations and keeps theirs.
        proposal.provenance.source = EditSource::Ai;
        proposal.provenance.confidence = 0.35;
        let (mut merged, mut changes) = schema::merge(&current, &proposal, EditSource::Ai)?;
        if !groups.is_empty() {
            let now = retouch_tools::read(&current)?;
            let stack = portrait_auto::staged(&now, &groups, upto);
            retouch_tools::validate(&stack)?;
            if !stack.is_empty() || merged.extra.contains_key(retouch_tools::KEY) {
                let mut with_stack = merged.clone();
                // Mattes for the planned operations, plus any already stored for the
                // photographer's own operations.
                let mattes = portrait_auto::mattes_for(&current, &portrait)?;
                retouch_tools::write_with_mattes(&mut with_stack, &stack, &mattes)?;
                let source = if retouch_owned {
                    EditSource::User
                } else {
                    EditSource::Ai
                };
                with_stack.provenance.source = source;
                let (stacked, more) = schema::merge(&merged, &with_stack, source)?;
                merged = stacked;
                changes.changed.extend(more.changed);
                changes.changed.sort_unstable();
                changes.changed.dedup();
            }
        }
        schema::Validation::check(&merged)?;
        if changes.changed.is_empty() || !saved_changed(&current, &merged)? {
            continue;
        }
        state.recipe_store().save(
            &project,
            &photo,
            &merged,
            &changes.changed,
            &format!(
                "{} {}/{total} · {}: {}",
                if chosen.is_some() {
                    "Auto retouch (your settings)"
                } else if global {
                    "Auto edit"
                } else {
                    "Auto portrait"
                },
                i + 1,
                step.title,
                step.detail
            ),
        )?;
        current = merged;
    }
    Ok(crate::develop_commands::recipe_dto(
        &input.photo_id,
        &current,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(rgb: &[u8], w: u32, h: u32) -> Pixels<'_> {
        Pixels::new(rgb, w, h).unwrap()
    }

    #[test]
    fn selections_follow_saved_exposure_when_manual_or_retouch_only() {
        let mut base = aura_recipe::fixtures::neutral("test", "test");
        base.global.exposure = -0.75;
        assert!((effective_exposure(&base, 1.0, true) - 1.0).abs() < f32::EPSILON);
        assert!((effective_exposure(&base, 1.0, false) + 0.75).abs() < f32::EPSILON);
        base.provenance
            .user_edited_fields
            .push("global.exposure".into());
        assert!((effective_exposure(&base, 1.0, true) + 0.75).abs() < f32::EPSILON);
        base.provenance.user_edited_fields = vec!["global".into()];
        assert!((effective_exposure(&base, 1.0, true) + 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn bright_studio_and_product_frames_are_not_mistaken_for_haze() {
        for colour in [[225, 220, 215], [180, 170, 160], [128, 128, 128]] {
            let rgb = colour.repeat(100 * 100);
            let plan = analyse(&pixels(&rgb, 100, 100), None, &[], 0.0);
            assert_eq!(plan.kind, SceneKind::General);
            assert_eq!(plan.dehaze, 0);
        }
    }

    #[test]
    fn a_blue_sky_landscape_gets_structure_colour_and_a_sky_gradient() {
        let (w, h) = (200_u32, 150_u32);
        let mut rgb = Vec::new();
        for y in 0..h {
            for _ in 0..w {
                rgb.extend(if y < 60 {
                    [150, 180, 225]
                } else {
                    [90, 120, 70]
                });
            }
        }
        let plan = analyse(&pixels(&rgb, w, h), None, &[], 0.0);
        assert_eq!(plan.kind, SceneKind::Landscape);
        assert!(plan.vibrance > 0 && plan.clarity > 0);
        let sky = plan.sky.expect("a bright blue sky");
        retouch_tools::validate(std::slice::from_ref(&sky)).unwrap();
        let g = sky.selection.unwrap().gradient.unwrap();
        assert!(g.start[1] > 0.3 && g.start[1] < 0.5, "{g:?}");
    }

    #[test]
    fn grey_and_dark_frames_are_not_given_colour_or_structure() {
        let gray = vec![128_u8; 100 * 100 * 3];
        let plan = analyse(&pixels(&gray, 100, 100), None, &[], 0.0);
        assert_eq!(plan.vibrance, 0);
        assert!(plan.sky.is_none());
        let dark = vec![25_u8; 100 * 100 * 3];
        let plan = analyse(&pixels(&dark, 100, 100), None, &[], 0.0);
        assert_eq!(plan.kind, SceneKind::LowLight);
        assert_eq!(plan.clarity, 0);
        assert!(plan.extra_shadows > 0);
    }

    #[test]
    fn night_and_high_key_frames_keep_their_intent() {
        let (w, h) = (120_usize, 100_usize);
        // A dark street with a row of lamps.
        let mut night = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let lamp = y > 20 && y < 36 && x % 12 < 3;
                night.extend(if lamp { [255, 230, 170] } else { [14, 16, 26] });
            }
        }
        let px = pixels(&night, w as u32, h as u32);
        let plan = analyse(&px, None, &[], 0.0);
        assert_eq!(plan.kind, SceneKind::Night, "{:?}", plan.decisions);
        assert_eq!((plan.blacks, plan.clarity, plan.dehaze), (0, 0, 0));
        let mut tone = (0.75, 0, 20, 0);
        let note = respect_intent(&mut tone, &px, &[]).expect("a note");
        assert!(tone.0 <= 0.15 && tone.2 <= 5, "{tone:?}");
        assert!(note.contains("night"));
        // Snow under a pale sky, with a few mid-grey rocks.
        let mut snow = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let rock = (x * 7 + y * 3) % 23 == 0;
                snow.extend(if rock {
                    [110, 112, 118]
                } else if y < 40 {
                    [215, 222, 232]
                } else {
                    [238, 240, 244]
                });
            }
        }
        let px = pixels(&snow, w as u32, h as u32);
        let plan = analyse(&px, None, &[], 0.0);
        assert_eq!(plan.kind, SceneKind::HighKey, "{:?}", plan.decisions);
        assert!(plan.blacks >= -8);
        let mut tone = (-0.25, -18, 0, 0);
        assert!(respect_intent(&mut tone, &px, &[]).is_some());
        assert!(tone.0 >= 0.0 && tone.1 >= -8);
        // A person in the frame hands the decision to the face-aware exposure cap instead.
        let face = PortraitFace {
            bounds: [0.3, 0.2, 0.7, 0.8],
            landmarks: [
                [0.4, 0.4],
                [0.6, 0.4],
                [0.5, 0.5],
                [0.45, 0.65],
                [0.55, 0.65],
            ],
            confidence: 0.9,
        };
        let mut tone = (-0.25, 0, 0, 0);
        assert!(respect_intent(&mut tone, &px, &[face]).is_none());
        assert!(tone.0 < 0.0);
    }

    #[test]
    fn highlights_decide_exposure_and_shadows_take_the_rest() {
        let (w, h) = (100_usize, 100_usize);
        // Dark midtones, but a tenth of the frame is already white: correctly exposed.
        let mut contrasty = Vec::new();
        for i in 0..w * h {
            contrasty.extend(if i % 10 == 0 {
                [250_u8; 3]
            } else {
                [70 + (i % 40) as u8; 3]
            });
        }
        let px = pixels(&contrasty, w as u32, h as u32);
        let mut tone = (0.75, 0, 10, 0);
        let note = respect_intent(&mut tone, &px, &[]).expect("a note");
        assert!(tone.0 <= 0.15, "{tone:?}");
        assert!(tone.2 > 10 && tone.2 <= 30, "{tone:?}");
        assert!(note.contains("shadows"), "{note}");
        // The same midtones with nothing bright: underexposed, and exposure is the right tool.
        let dim: Vec<u8> = (0..w * h).flat_map(|i| [70 + (i % 40) as u8; 3]).collect();
        let mut tone = (0.75, 0, 10, 0);
        respect_intent(&mut tone, &pixels(&dim, w as u32, h as u32), &[]);
        assert!(tone.0 > 0.5 && tone.2 < 20, "{tone:?}");
        // Bright but unclipped: never darkened.
        let bright: Vec<u8> = (0..w * h).flat_map(|i| [150 + (i % 60) as u8; 3]).collect();
        let mut tone = (-0.25, 0, 0, 0);
        assert!(respect_intent(&mut tone, &pixels(&bright, w as u32, h as u32), &[]).is_some());
        assert!(tone.0.abs() < 1e-6);
    }

    #[test]
    fn a_sepia_print_keeps_its_tone() {
        let (w, h) = (100_usize, 100_usize);
        let mut sepia = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = 40.0 + ((x * 3 + y * 5) % 160) as f32;
                sepia.extend([v as u8, (v * 0.86) as u8, (v * 0.68) as u8]);
            }
        }
        let px = pixels(&sepia, w as u32, h as u32);
        let m = measure(&px, &[]);
        assert!(m.hue_spread < 0.08, "{}", m.hue_spread);
        let plan = analyse(&px, None, &[], 0.0);
        assert_eq!(plan.vibrance, 0, "{:?}", plan.decisions);
        assert!(plan
            .decisions
            .iter()
            .any(|d| d.contains("toned monochrome")));
        // A colourful frame is not a toned one.
        let mut colourful = Vec::new();
        for i in 0..w * h {
            colourful.extend([[200, 90, 80], [80, 160, 90], [70, 100, 190]][i % 3]);
        }
        assert!(measure(&pixels(&colourful, w as u32, h as u32), &[]).hue_spread > 0.2);
    }

    #[test]
    fn measured_noise_turns_on_noise_reduction_and_calms_sharpening() {
        let (w, h) = (160_usize, 160_usize);
        let mut state = 12345_u32;
        let mut noisy = Vec::with_capacity(w * h * 3);
        for _ in 0..w * h {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let n = ((state >> 24).cast_signed() - 128) / 20;
            let v = (128 + n).clamp(0, 255) as u8;
            noisy.extend([v, v, v]);
        }
        let plan = analyse(&pixels(&noisy, w as u32, h as u32), None, &[], 0.0);
        assert!(plan.noise.is_some(), "{:?}", plan.decisions);
        let clean = vec![128_u8; w * h * 3];
        let calm = analyse(&pixels(&clean, w as u32, h as u32), None, &[], 0.0);
        assert!(calm.noise.is_none());
        assert!(plan.sharpen.0 < calm.sharpen.0);
    }

    #[test]
    fn staged_stacks_keep_manual_work_first_and_are_stable_on_a_repeat() {
        let edit = |id: &str| Edit {
            id: id.into(),
            tool: Tool::Dodge,
            enabled: true,
            region: [0.5, 0.5, 0.1, 0.1],
            source: None,
            amount: 0.5,
            feather: 0.5,
            radius: 0.002,
            source_scale: 1.0,
            preserve_microtexture: false,
            texture: 1.0,
            texture_heal: false,
            clean_ring_fit: false,
            curved_heal: false,
            heal_samples: Vec::new(),
            texture_sources: Vec::new(),
            sensitivity: None,
            keep_dark_marks: false,
            tone: 0.5,
            warmth: 0.0,
            tint: 0.0,
            mask: None,
            skin: None,
            selection: None,
            matte: None,
        };
        let manual = edit("manual");
        let skin = edit("auto-portrait-v1-0-texture");
        let spot = edit("auto-portrait-v1-0-spot-0");
        let current = vec![skin.clone(), manual.clone(), spot.clone()];
        let mut planned = BTreeMap::new();
        planned.insert(Group::Skin, vec![skin.clone()]);
        planned.insert(Group::Blemishes, vec![spot.clone()]);
        let after_skin = portrait_auto::staged(&current, &planned, Group::Skin);
        assert_eq!(after_skin, vec![manual.clone(), skin.clone(), spot.clone()]);
        assert_eq!(
            portrait_auto::staged(&after_skin, &planned, Group::Finishing),
            after_skin
        );
        planned.insert(Group::Blemishes, Vec::new());
        assert_eq!(
            portrait_auto::staged(&after_skin, &planned, Group::Finishing),
            vec![manual, skin]
        );
    }
}
