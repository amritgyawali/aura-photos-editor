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
use aura_render::{retouch_tools as retouch_tools_render, FrameSource, RenderLevel};
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

/// A photograph whose colour is its look: none at all, or one colour throughout. Nothing in an
/// automatic edit adds colour to it, enriches it or corrects it away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Monochrome {
    /// Black and white.
    Neutral,
    /// One colour throughout: sepia, selenium, a cyanotype.
    Toned,
}

impl Monochrome {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Neutral => "a black-and-white photograph",
            Self::Toned => "a toned monochrome",
        }
    }
}

/// Global decisions for one photograph.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalPlan {
    pub kind: SceneKind,
    /// Set when the frame is black and white or toned; [`crate::adaptive_look`] then adds no
    /// colour to it.
    pub monochrome: Option<Monochrome>,
    /// The lens's colour fringing as measured on the renderer's own frame, when there was one.
    /// [`apply_fringing`] writes a correction it confirmed. ADR-0125.
    pub fringing: Option<crate::lateral_ca::Fringing>,
    /// A purple-fringe removal, when rims were measured along bright edges. ADR-0125.
    pub defringe: Option<Edit>,
    pub white_balance: Option<(u32, i16)>,
    pub vibrance: i16,
    pub clarity: i16,
    pub dehaze: i16,
    pub extra_shadows: i16,
    pub whites: i16,
    pub blacks: i16,
    /// Black and white points for every channel's curve, for a frame veiled at both ends.
    pub levels: Option<[[u16; 2]; 2]>,
    /// Amount, radius, detail, masking.
    pub sharpen: (i16, f32, i16, i16),
    /// Luminance and colour noise reduction, when the measured noise needs any.
    pub noise: Option<(i16, i16)>,
    pub sky: Option<Edit>,
    /// Shadows and Highlights sized from the frame's regions. ADR-0126.
    pub range: crate::local_range::RangePlan,
    pub decisions: Vec<String>,
}

impl GlobalPlan {
    /// The Highlights and Shadows this plan writes, given the histogram's
    /// `(exposure, highlights, shadows, contrast)`: the stronger of the histogram's answer and
    /// the regions' for each, never their sum. ADR-0126.
    #[must_use]
    pub fn highlights_shadows(&self, tone: (f32, i16, i16, i16)) -> (i16, i16) {
        self.range
            .merged(tone.1, (tone.2 + self.extra_shadows).clamp(-100, 100))
    }
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

/// Display lightness the brightest tones of a correctly exposed frame reach.
const NEAR_WHITE: f32 = 0.85;

/// Where the darkest tones of a frame stop being black, and where they are a veil for certain.
///
/// A soft or matte finish lifts the blacks too, to about 16% on the finished photographs
/// measured; a veil lifts them past 20%. The two overlap, so the end points are given back
/// only from where a finish is unlikely, and in full only where it is very unlikely.
const VEIL_FROM: f32 = 0.15;
const VEIL_FULL: f32 = 0.25;
/// A frame whose very brightest tones reach this is using the top of its range.
const VEIL_CEILING: f32 = 0.96;
/// How much of a measured veil is taken off. Part of any softness is the photograph's own.
const VEIL_REMOVED: f32 = 0.85;

/// The darkest and brightest tones a frame actually contains, as display lightness: the
/// 0.5th and 99.8th percentiles, so that one dead pixel or one specular is not an end point.
///
/// Not the 2nd and 98th, which say how much of a frame is dark or bright. A frame with a grey
/// backdrop and one white shirt reaches white; its 98th percentile does not.
fn extremes(px: &Pixels<'_>) -> (f32, f32) {
    let (w, h) = (px.width, px.height);
    let step = ((w * h) as f32 / 250_000.0).sqrt().ceil().max(1.0) as usize;
    let mut display: Vec<f32> = (0..h)
        .step_by(step)
        .flat_map(|y| (0..w).step_by(step).map(move |x| (x, y)))
        .map(|(x, y)| aura_raw::colour::curve::srgb_encode(luma(px.linear(x, y))))
        .collect();
    (
        percentile(&mut display, 0.005),
        percentile(&mut display, 0.998),
    )
}

/// The tone curve's end points for a frame that uses neither end of its range.
///
/// Haze, flare, a flat picture profile and a faded print lift the blacks and lower the whites
/// together, and a slider that scales one band of tones cannot take a pedestal off: the
/// frame has to be given its end points back. One end alone is a different thing and is left
/// to the frame: no black is a soft or high-key look, and no white is a subject without
/// anything white in it.
///
/// `darkest` and `brightest` are display lightness. The points are in the curve's own domain.
fn levels(darkest: f32, brightest: f32) -> Option<[[u16; 2]; 2]> {
    if brightest >= VEIL_CEILING || brightest - darkest <= 0.1 {
        return None;
    }
    let veil = ((darkest - VEIL_FROM) / (VEIL_FULL - VEIL_FROM)).clamp(0.0, 1.0) * VEIL_REMOVED;
    if veil < 0.3 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let point = |display: f32| {
        (aura_render::tonemap::curve_domain_encode(aura_raw::colour::curve::srgb_decode(display))
            * 255.0)
            .round()
            .clamp(0.0, 255.0) as u16
    };
    let (black, white) = (point(darkest * veil), point(1.0 - (1.0 - brightest) * veil));
    (white > black + 32).then_some([[black, 0], [white, 255]])
}

/// Give a frame its end points back, on each channel alike.
///
/// A veil is light added to every channel, so it washes colour out as well as contrast. The
/// luminance curve holds colour as it finds it and would leave the frame grey with deep
/// blacks; the same curve on red, green and blue takes the pedestal off each, which is what
/// puts the colour back, and using one curve for all three leaves the white balance alone.
pub(crate) fn set_levels(global: &mut aura_recipe::Global, points: [[u16; 2]; 2]) {
    let curve = levels_curve(points);
    global.channel_curves.red.points.clone_from(&curve);
    global.channel_curves.green.points.clone_from(&curve);
    global.channel_curves.blue.points = curve;
}

/// How many points the straight run between the end points is drawn with.
const LEVELS_STEPS: u16 = 8;

/// The tone curve that puts black and white at these end points.
///
/// A recipe's curve spans the whole range, so the ends are held flat outside them, and the
/// renderer draws a smooth monotone curve through whatever points it is given. Through the
/// four corners alone that curve is an S between the end points, which adds a contrast
/// nobody asked for on top of the range that was. The points along the straight run are
/// what keep it straight, and leave only a short toe and shoulder at the ends.
fn levels_curve([[black, _], [white, _]]: [[u16; 2]; 2]) -> Vec<[u16; 2]> {
    let mut points = vec![[0, 0]];
    if black > 0 {
        points.push([black, 0]);
    }
    let span = white.saturating_sub(black);
    for step in 1..LEVELS_STEPS {
        let x = black + span * step / LEVELS_STEPS;
        if points.last().is_some_and(|last| x > last[0]) && x < white {
            points.push([x, 255 * step / LEVELS_STEPS]);
        }
    }
    if white < 255 {
        points.push([white, 255]);
    }
    points.push([255, 255]);
    points
}

/// Whether [`levels`] is the remedy for this frame at all. A night or low-light frame keeps
/// its dark floor and a high-key one has no black on purpose; with people in the frame those
/// readings belong to the backdrop and say nothing.
fn veil_is_a_fault(m: &Measure, people: bool) -> bool {
    people || !(intent(m).is_some() || (m.median < 0.22 && m.high < 0.7))
}

/// A veiled frame's histogram measures the veil. Its median sits near the middle whatever the
/// exposure was, and its narrow range asks for contrast the end points are about to give
/// back, so the histogram's tone correction is dropped and the end points do the work.
pub(crate) fn veiled(px: &Pixels<'_>, faces: &[PortraitFace]) -> bool {
    let m = measure(px, faces);
    let (darkest, brightest) = extremes(px);
    veil_is_a_fault(&m, big_faces(faces) > 0) && levels(darkest, brightest).is_some()
}

/// The most a high-key frame's highlights are taken down by, with or without people in it.
const HIGH_KEY_HIGHLIGHTS: i16 = -8;

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
    (room > 0.5).then(|| ((room * 0.9).min(1.0), top))
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
    // When the very brightest tones already come near white the exposure is right, however
    // dark the midtones are: the darkness is the photograph's, and the histogram's lift goes
    // to the shadows instead, lightly. A dark photograph two thirds of a stop over reads the
    // same as a mid-toned one exposed correctly, so lifting it only takes it further away.
    let exposed = extremes(px).1 >= NEAR_WHITE;
    let headroom = if exposed { 0.0 } else { headroom };
    // And the shadows get a lighter hand there too: it is the same darkness.
    let most_moved = if exposed { 10 } else { 30 };
    let wanted = tone.0 * 0.7;
    if tone.0 > 0.0 && wanted.min(headroom) + 1e-3 < tone.0 {
        let excess = tone.0 - wanted.min(headroom);
        tone.0 = wanted.min(headroom);
        tone.2 = (tone.2 + ((excess * 30.0).round() as i16).min(most_moved)).min(30);
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
            tone.1 = tone.1.max(HIGH_KEY_HIGHLIGHTS);
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

/// Keep the whites of a frame that is bright by design when there are people in it too.
///
/// [`respect_intent`] leaves frames with people to [`face_exposure_cap`], and that answers
/// for exposure alone. Highlights is a separate lever, and the histogram pulls it down
/// whenever the brightest part of the frame is bright - which in front of a white studio wall
/// is the wall. The wall is not the people and it is not overexposed: taking it down turns a
/// white backdrop grey, and nothing about the faces asked for it.
pub fn keep_high_key(
    tone: &mut (f32, i16, i16, i16),
    px: &Pixels<'_>,
    faces: &[PortraitFace],
) -> Option<String> {
    if big_faces(faces) == 0 || tone.1 >= HIGH_KEY_HIGHLIGHTS {
        return None;
    }
    if intent(&measure(px, faces)) != Some(SceneKind::HighKey) {
        return None;
    }
    let asked = tone.1;
    tone.1 = HIGH_KEY_HIGHLIGHTS;
    Some(format!(
        "Highlights {HIGH_KEY_HIGHLIGHTS:+} (the histogram asked for {asked:+}): the frame is bright by design, and taking its white background down would turn it grey."
    ))
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
/// The size of colour, in log chroma, that a finished photograph carries by choice.
///
/// Measured on six published photographs as they were delivered: every one of them read
/// between 0.05 and 0.12 to all three estimates, and none was a mistake.
const CAST_BY_CHOICE: f32 = 0.10;

/// The contrast a camera file is given when nothing else asks for more.
///
/// A camera file rendered neutrally is flatter than any camera's own JPEG of the same frame,
/// because every camera and every raw converter lays a base tone curve over the sensor's
/// response; a finished photograph already carries one and is left alone. The value is a
/// moderate base curve chosen beforehand, not fitted: it was then checked against real
/// camera files and an expert's finished versions, and no research data set it.
const CAMERA_FILE_CONTRAST: i16 = 15;

/// Give a camera file the base contrast every rendering of one carries, and say so.
fn camera_file_contrast(tone: &mut (f32, i16, i16, i16), developed: bool) -> Option<String> {
    if developed || tone.3 >= CAMERA_FILE_CONTRAST {
        return None;
    }
    tone.3 = CAMERA_FILE_CONTRAST;
    Some(format!(
        "Contrast +{CAMERA_FILE_CONTRAST}: a camera file rendered neutrally is flatter than the camera's own picture; this is the base curve every rendering of one carries."
    ))
}

/// The colour left to a camera file's own white balance: a trace, not a choice.
const CAMERA_CAST_FLOOR: f32 = 0.04;

fn white_balance(frame: &aura_render::Frame, faces: &[PortraitFace], developed: bool) -> Neutral {
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
    // A third, independent reading: the brightest unclipped tones. Whites, paper, cloth and
    // speculars take the colour of the light and nothing else, so when they are neutral the
    // light is, whatever the scene's own colours add up to - a finished photograph with warm
    // wood or foliage in it reads as a cast to the other two. And when they show the same cast,
    // more of it can be removed with confidence.
    let whites = white_patch(&samples);
    let mut estimate = ((centre.0 + edge.0) * 0.5, (centre.1 + edge.1) * 0.5);
    let mut settled = false;
    if (centre.0 - edge.0).hypot(centre.1 - edge.1) > tolerance {
        // Mixed light: the neutral areas and the edges each follow a different source. In a
        // finished photograph that mixture is what somebody kept. From a camera file nobody
        // has chosen yet, and when the whites side with one of the two, two witnesses of three
        // agree about the light the subject was in.
        let sided = whites.filter(|_| !developed).and_then(|patch| {
            [centre, edge]
                .into_iter()
                .filter(|witness| (witness.0 - patch.0).hypot(witness.1 - patch.1) <= 0.1)
                .min_by(|a, b| {
                    let d = |w: &(f32, f32)| (w.0 - patch.0).hypot(w.1 - patch.1);
                    d(a).total_cmp(&d(b))
                })
                .map(|witness| ((witness.0 + patch.0) * 0.5, (witness.1 + patch.1) * 0.5))
        });
        let Some(sided) = sided.filter(|_| coverage >= 0.03) else {
            return Neutral::Unsure {
                reason: "the neutral areas and the edges disagree about the light (a coloured backdrop or mixed light)",
            };
        };
        estimate = sided;
        settled = true;
    }
    // How much colour a photograph is taken to carry by choice. A camera file's white balance
    // is the camera's guess and nobody's choice yet, so only a trace of colour is left to it.
    let floor = if developed {
        CAST_BY_CHOICE
    } else {
        CAMERA_CAST_FLOOR
    };
    let confirmed = settled
        || match whites {
            Some(patch)
                if patch.0.hypot(patch.1) < 0.07 && estimate.0.hypot(estimate.1) >= 0.04 =>
            {
                return Neutral::Unsure {
                reason: "the brightest tones in the frame are neutral, so the light is; the colour elsewhere belongs to the scene",
            };
            }
            // A handful of near-neutral pixels agreeing with each other is one coloured object,
            // not three witnesses; the whites only add confidence where the neutral area is real.
            // A frame filled by people in front of a bright backdrop has little left to measure
            // once the faces and the near-clipped tones are set aside, so a smaller area will do
            // when the cast is strong.
            Some(patch) => {
                let cast = estimate.0.hypot(estimate.1);
                coverage
                    >= if cast > CAST_BY_CHOICE * 2.0 || !developed {
                        0.03
                    } else {
                        0.1
                    }
                    && (estimate.0 - patch.0).hypot(estimate.1 - patch.1) <= 0.1
            }
            None => false,
        };
    let estimate = match whites {
        Some(patch) if confirmed && !settled => (
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
    // The gray-pixel and gray-edge estimates agree with each other on any frame dominated by
    // one colour: a pink studio backdrop, a painted wall, golden-hour light. Only the whites
    // tell a cast on everything from a colourful scene, so without them the colour is kept -
    // a finished photograph's white balance was somebody's choice, and moving it on two
    // estimates that one coloured surface can fool turns a pink portrait grey.
    if !confirmed {
        return Neutral::Unsure {
            reason: "the frame's whites do not show the same cast, so the colour is the scene's (a coloured backdrop or wall, or the light's mood) rather than a cast",
        };
    }
    // Take off the part of a cast a photograph would not carry on purpose, and leave the
    // rest. A finished photograph is rarely neutral: a warm backdrop, a cool grey studio and
    // evening light all read as a cast of about this size to every estimate there is, and
    // removing them moves a picture somebody balanced. Past that size the colour is the
    // light's, and what is left after the correction is as much of it as a choice would be.
    if cast <= floor {
        return Neutral::Unsure {
            reason: "the colour is faint enough to be the photograph's own (a warm backdrop, a cool studio grey, the hour) rather than a cast",
        };
    }
    // A colourful finished frame fools all three witnesses the same way: pink saris and a pale
    // pink wall, a crowd in yellow and orange, marigold decor. Its brightest near-neutral
    // tones are pale versions of those colours, so the whites "confirm" a cast that is the
    // scene. A real cast tints everything, so it does not line up with what fills the frame.
    if developed && scene_colour_explains(frame, faces, estimate) {
        return Neutral::Unsure {
            reason: "the cast points the same way as the colours that fill the frame (clothing, decor, a painted wall), so it is the scene's colour rather than the light's",
        };
    }
    let strength = ((cast - floor) / cast).min(0.85);
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

/// Share of a frame (faces aside) that strongly coloured things must cover before their
/// colour is taken as the reason every estimate reads a cast.
const SCENE_COLOUR_SHARE: f32 = 0.25;

/// True when a cast estimate `(ln r/g, ln b/g)` points the same way as the saturated colours
/// that fill the frame, outside the faces: the scene's colour, not the light's.
fn scene_colour_explains(
    frame: &aura_render::Frame,
    faces: &[PortraitFace],
    estimate: (f32, f32),
) -> bool {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let cast = estimate.0.hypot(estimate.1);
    if w < 4 || h < 4 || cast <= 1e-6 {
        return false;
    }
    let step = ((w * h) as f32 / 100_000.0).sqrt().ceil().max(1.0) as usize;
    let (mut total, mut coloured) = (0_usize, 0_usize);
    let mut sum = (0.0_f64, 0.0_f64);
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            if in_faces(
                faces,
                (x as f32 + 0.5) / w as f32,
                (y as f32 + 0.5) / h as f32,
            ) {
                continue;
            }
            let i = (y * w + x) * 3;
            let (Some(&r), Some(&g), Some(&b)) =
                (frame.rgb.get(i), frame.rgb.get(i + 1), frame.rgb.get(i + 2))
            else {
                continue;
            };
            if !(r.is_finite() && g.is_finite() && b.is_finite()) {
                continue;
            }
            total += 1;
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            if max < 0.02 || min < 0.002 || max > 0.95 || (max - min) / max < 0.35 {
                continue;
            }
            coloured += 1;
            sum.0 += f64::from((r / g).ln());
            sum.1 += f64::from((b / g).ln());
        }
    }
    if total == 0 || coloured == 0 || (coloured as f32 / total as f32) < SCENE_COLOUR_SHARE {
        return false;
    }
    let dominant = (
        (sum.0 / coloured as f64) as f32,
        (sum.1 / coloured as f64) as f32,
    );
    let length = dominant.0.hypot(dominant.1);
    length > 1e-6 && (estimate.0 * dominant.0 + estimate.1 * dominant.1) / (cast * length) > 0.7
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
    analyse_from(px, frame, faces, exposure, true)
}

/// [`analyse`], told whether the pixels were already developed (a JPEG, a PNG, an edited
/// file) or come straight from a camera file, whose white balance nobody has chosen yet.
#[must_use]
pub fn analyse_from(
    px: &Pixels<'_>,
    frame: Option<&aura_render::Frame>,
    faces: &[PortraitFace],
    exposure: f32,
    developed: bool,
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
    let monochrome = if m.saturation <= 0.02 {
        Some(Monochrome::Neutral)
    } else if toned {
        Some(Monochrome::Toned)
    } else {
        None
    };
    // Vibrance: less for already colourful frames, never on skin-heavy portraits past +10.
    let base: f32 = match kind {
        // Vibrance moves skin more than anything else in a portrait; stay very light.
        SceneKind::Portrait | SceneKind::Group => 4.0,
        SceneKind::Landscape => 18.0,
        SceneKind::LowLight | SceneKind::Night => 6.0,
        SceneKind::HighKey => 8.0,
        SceneKind::General => 12.0,
    };
    // A veil washes colour out with contrast, and taking it off puts both back: the low
    // saturation measured through it is not a reason to add more.
    let hazy =
        kind == SceneKind::Landscape && m.dark_floor > 0.13 && (m.high - m.dark_floor) < 0.75;
    let through_a_veil = !hazy && exposure.abs() < 1e-3 && veiled(px, faces);
    let colour = if toned || m.saturation > 0.45 {
        0.0
    } else if m.saturation > 0.35 {
        0.5
    } else if m.saturation < 0.15 && m.saturation > 0.02 && !through_a_veil {
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
    let dehaze = if hazy {
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
    let (darkest, brightest) = extremes(px);
    let (darkest, brightest) = (after(darkest), after(brightest));
    let levels = (dehaze == 0 && veil_is_a_fault(&m, big_faces > 0))
        .then(|| levels(darkest, brightest))
        .flatten();
    let blacks =
        if matches!(kind, SceneKind::LowLight | SceneKind::Night) || p02 <= 0.08 || !has_range {
            0
        } else if levels.is_some() {
            // The curve's black point takes the whole pedestal off; a slider on top doubles it.
            0
        } else if kind == SceneKind::HighKey {
            // A high-key frame has no black in it on purpose; anchor it only lightly.
            -((p02 - 0.04) * 150.0).clamp(0.0, 8.0) as i16
        } else if dehaze > 0 {
            // Dehaze already deepens the floor; setting the black point as well doubles it.
            -((p02 - 0.04) * 75.0).clamp(0.0, 15.0) as i16
        } else if big_faces > 0 {
            // A lifted floor behind people is most often a soft finish: anchor it lightly.
            -((p02 - 0.04) * 150.0).clamp(0.0, 12.0) as i16
        } else {
            -((p02 - 0.04) * 150.0).clamp(0.0, 25.0) as i16
        };
    // The 98th percentile says how much of a frame is bright; its very brightest tones say
    // whether anything is. A grey studio backdrop with a white shirt in front of it reaches
    // white and is finished, and stretching its top brightens skin to fill a range nothing
    // said it should fill. Only a frame in which nothing at all comes near white is stretched.
    // Nor is a low-key portrait, whose people are lit brighter than the dark around them.
    let colour_at_top = saturated_top_share(px) > SATURATED_TOP_SHARE;
    let whites = if levels.is_some()
        || p98 >= 0.9
        || brightest >= 0.9
        || !has_range
        || colour_at_top
        || people_lit_above_the_scene(px, faces)
    {
        0
    } else {
        ((0.95 - p98) * 150.0).clamp(0.0, 20.0) as i16
    };
    if colour_at_top && whites == 0 && p98 < 0.9 && brightest < 0.9 {
        decisions.push("Whites kept: a saturated colour near the top of its channel fills part of the frame, and stretching the whites would clip it.".into());
    }
    if levels.is_some() {
        decisions.push(format!(
            "Black and white points set: the frame's tones run only from {:.0}% to {:.0}% of the range, veiled at both ends as haze, flare or a flat picture profile leaves it, so its end points are given back.",
            darkest * 100.0,
            brightest * 100.0
        ));
    } else if blacks != 0 || whites != 0 {
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
    // Shadows and Highlights act on regions in the renderer, so they are sized from the
    // frame's regions too: how much of it is deep shadow, how much is bright with detail
    // left, and how much is clipped. ADR-0126.
    let range = crate::local_range::plan(px, exposure, kind);
    if let Some(sentence) = range.sentence() {
        decisions.push(sentence);
    }
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
            || white_balance(f, faces, developed),
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
                "White balance {kelvin} K / tint {tint:+}: two independent estimates agreed on a cast{}; removed about {:.0}% of it, the part beyond what a photograph carries by choice, measured on {:.0}% neutral areas with faces ignored.",
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
                repair_passes: None,
        target_color: None,
                sensitivity: None,
                keep_dark_marks: false,
                mole_edge: None,
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
    // Lateral chromatic aberration: measured in the working space the renderer corrects it in,
    // so only on the renderer's own frame.
    let fringing = frame
        .filter(|f| f.kind == aura_render::InputKind::Working)
        .map(|f| crate::lateral_ca::decide(&f.rgb, f.width as usize, f.height as usize));
    if let Some(found) = &fringing {
        decisions.push(found.sentence());
    }
    let defringe = frame
        .filter(|f| f.kind == aura_render::InputKind::Working)
        .and_then(|f| {
            let (w, h) = (f.width as usize, f.height as usize);
            let edit = defringe_edit();
            let rims = retouch_tools_render::defringe_rims(&f.rgb, w, h, &edit);
            let enough = (DEFRINGE_PER_MILLION * (w * h) as f32 / 1e6).max(DEFRINGE_FEWEST);
            if (rims as f32) < enough {
                if rims > 0 {
                    decisions.push(format!(
                        "Purple fringes: {rims} pixels along bright edges read as a lens's rim, too few to correct."
                    ));
                }
                return None;
            }
            decisions.push(format!(
                "Purple fringes removed: {rims} pixels on the dark side of bright edges ({:.2}% of the frame) take the colour of their calm surroundings; purple things and the glow of purple lights are kept.",
                rims as f32 / (w * h) as f32 * 100.0
            ));
            Some(edit)
        });
    GlobalPlan {
        kind,
        monochrome,
        fringing,
        defringe,
        white_balance,
        vibrance,
        clarity,
        dehaze,
        extra_shadows,
        whites,
        blacks,
        levels,
        sharpen,
        noise,
        sky,
        range,
        decisions,
    }
}

/// Keep a brightening exposure from washing out the people in the frame.
///
/// Two relative checks, never a target brightness for skin: a subject that is already
/// clearly brighter than the rest of the scene is a low-key portrait and keeps its mood, and
/// no face may be pushed into clipping.
pub(crate) fn face_exposure_cap(
    exposure: f32,
    px: &Pixels<'_>,
    faces: &[PortraitFace],
) -> (f32, Option<String>) {
    if faces.is_empty() {
        return (exposure, None);
    }
    // A detected face identifies the subject, not its intended brightness. Dark complexion
    // and low-key lighting must not be treated as proof of underexposure. ADR-0096.
    let requested = exposure;
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
    if hot > HOT_SKIN {
        let down = -(0.2 + (hot - HOT_SKIN) * 2.0).min(0.7);
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
    // And when the people are not darker than the frame around them, the frame's darkness is
    // not theirs: at most a small lift, however far the median sits from middle grey.
    let surroundings = face_median >= frame_median;
    let exposure = if surroundings {
        exposure.min(0.25)
    } else {
        exposure
    };
    // The surroundings rule answers a histogram that wants middle grey. Highlights are a
    // different witness: when nothing in the frame, the people included, comes near white, the
    // whole frame is low, and a night scene is the one exception.
    let night = intent(&measure(px, faces)) == Some(SceneKind::Night);
    // When the people are brighter than the dark around them, the dark is no reason to lift
    // them: a dark complexion in a dark room is not underexposed. A white shirt, a page or a
    // wall that is brighter than the people and still nowhere near white is a different
    // witness, and says the whole frame was exposed too low.
    // That witness is believed only when it is far short of white, as a stop of underexposure
    // leaves it: a pale blue shirt in a moody portrait sits a little short of white by choice.
    let witnessed = |lift: f32| {
        !surroundings
            || (lift >= WHITE_OBJECT_LIFT
                && (white_object_short_of_white(px, faces, face_median)
                    || brightest_is_not_the_people(px, faces)))
    };
    let anchor =
        highlight_lift(px).filter(|(lift, _)| !night && witnessed(*lift) && *lift > exposure);
    // With people in the frame, how bright the rest of it is says nothing about how they were
    // exposed: a histogram asks for a lift in front of every dark wall and a cut in front of
    // every white one. Exposure moves on evidence only - skin at white, above, or nothing in
    // the frame reaching white - and a capped histogram request is still a guess, applied to
    // every finished portrait that happens to have dark surroundings.
    if anchor.is_none() && requested.abs() > 1e-3 {
        return (
            0.0,
            Some(format!(
                "Exposure kept (the histogram asked for {requested:+.2} EV): with people in the frame, the brightness of everything else is not evidence about them, and nothing in the frame says they are under- or overexposed."
            )),
        );
    }
    let exposure = anchor.map_or(exposure, |(lift, _)| lift);
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
    let (exposure, _, _, contrast) = tone;
    let (highlights, shadows) = plan.highlights_shadows(tone);
    g.exposure = exposure;
    g.highlights = highlights;
    g.shadows = shadows;
    g.contrast = contrast;
    if let Some((kelvin, tint)) = plan.white_balance {
        g.temperature = kelvin;
        g.tint = tint;
    }
    g.whites = plan.whites;
    g.blacks = plan.blacks;
    if let Some(points) = plan.levels {
        set_levels(g, points);
    }
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
    apply_fringing(recipe, plan);
}

/// The automatic purple-fringe removal: a scene operation a person can weaken, disable or
/// remove like any other. Green rims are left to a person, because the edge of every leaf
/// against a bright sky is green too. ADR-0125.
pub const DEFRINGE_ID: &str = "auto-scene-v1-defringe";
/// How far from a bright area a rim may reach, as a share of the shorter side.
const DEFRINGE_REACH: f32 = 0.003;
/// Rim pixels per million of the frame, and in all, before the removal is proposed.
const DEFRINGE_PER_MILLION: f32 = 40.0;
const DEFRINGE_FEWEST: f32 = 80.0;

fn defringe_edit() -> Edit {
    Edit {
        id: DEFRINGE_ID.to_owned(),
        tool: Tool::Defringe,
        enabled: true,
        // The whole photograph, corners included: what "Select entire photo" selects.
        region: [0.5, 0.5, 1.0, 1.0],
        source: None,
        amount: 1.0,
        feather: 0.0,
        radius: DEFRINGE_REACH,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture: 1.0,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        repair_passes: None,
        target_color: None,
        sensitivity: None,
        keep_dark_marks: false,
        mole_edge: None,
        tone: 0.5,
        warmth: 1.0,
        tint: 0.0,
        mask: None,
        skin: None,
        matte: None,
        selection: None,
    }
}

/// Write a confirmed fringing correction into the lens block: the renderer's `ca_red` and
/// `ca_blue`, with nothing else about the lens changed. Chromatic scales a lens profile or a
/// person already set are kept; distortion coefficients already there are kept beside the new
/// scales. Returns whether anything was written. ADR-0125.
pub(crate) fn apply_fringing(recipe: &mut Recipe, plan: &GlobalPlan) -> bool {
    let Some([red, blue]) = plan
        .fringing
        .as_ref()
        .and_then(crate::lateral_ca::Fringing::scales)
    else {
        return false;
    };
    let lens = &mut recipe.lens;
    match lens.coefficients.as_mut() {
        None => {
            lens.coefficients = Some(aura_recipe::LensCoefficients {
                k1: 0.0,
                k2: 0.0,
                k3: 0.0,
                ca_red: red,
                ca_blue: blue,
            });
        }
        Some(c)
            if (c.ca_red - 1.0).abs() < f32::EPSILON && (c.ca_blue - 1.0).abs() < f32::EPSILON =>
        {
            c.ca_red = red;
            c.ca_blue = blue;
        }
        Some(_) => return false,
    }
    lens.ca = true;
    true
}

/// Hold a proposed exposure, highlight, shadow and contrast move to what the photograph
/// shows: skin neither clipped nor lifted past its headroom, a white wall not mistaken for
/// overexposed people, a frame that is dark or bright by design left that way.
///
/// The same three rules whoever proposed the move. AURA's own histogram reading goes through
/// them, and so does a vision model's answer: a model reads a bright median as overexposure
/// as readily as a histogram does, and cloud proposes while deterministic code decides.
pub(crate) fn guard_tone(
    tone: &mut (f32, i16, i16, i16),
    px: &Pixels<'_>,
    faces: &[PortraitFace],
) -> Option<String> {
    let (capped, note) = face_exposure_cap(tone.0, px, faces);
    tone.0 = capped;
    let note = note.or(respect_intent(tone, px, faces));
    let (overexposed, bright) = highlights_on_evidence(tone, px, faces);
    // Skin at the ceiling is overexposure, not a bright frame made on purpose.
    let highlights = bright.or_else(|| {
        if overexposed {
            None
        } else {
            keep_high_key(tone, px, faces)
        }
    });
    let clipping = colour_clip_cap(tone, px);
    let parts: Vec<String> = [note, highlights, clipping].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Share of the frame a brightening may newly push to the top of a colour channel. A dark frame
/// may give up a little more of a saturated colour for the lift it needs.
const LIFT_CLIP_SHARE: f32 = 0.02;
const DARK_LIFT_CLIP_SHARE: f32 = 0.08;
/// Share of the frame that saturated colour near the top of its channel must cover before the
/// whites are not stretched: their brightness says "room", their reddest channel says none.
pub(crate) const SATURATED_TOP_SHARE: f32 = 0.05;

/// The share of a frame that is a saturated colour already near the top of a channel: a pink
/// backdrop, an orange tent, a red sari in good light.
pub(crate) fn saturated_top_share(px: &Pixels<'_>) -> f32 {
    let (w, h) = (px.width, px.height);
    let step = ((w * h) as f32 / 40_000.0).sqrt().ceil().max(1.0) as usize;
    let (mut n, mut hit) = (0_usize, 0_usize);
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let p = px.encoded(x, y);
            let max = p[0].max(p[1]).max(p[2]);
            let min = p[0].min(p[1]).min(p[2]);
            n += 1;
            hit += usize::from(max > 0.88 && (max - min) / max > 0.35);
        }
    }
    if n == 0 {
        0.0
    } else {
        hit as f32 / n as f32
    }
}

/// Hold an exposure lift where it would push a saturated area - a pink backdrop, an orange
/// tent, a red sari - to the top of a colour channel. Brightness alone does not see this: a
/// saturated colour clips in one channel long before its luminance nears white.
fn colour_clip_cap(tone: &mut (f32, i16, i16, i16), px: &Pixels<'_>) -> Option<String> {
    if tone.0 <= 0.0 {
        return None;
    }
    let (w, h) = (px.width, px.height);
    let step = ((w * h) as f32 / 40_000.0).sqrt().ceil().max(1.0) as usize;
    // Each sample goes through the renderer's own input and output transforms, so the
    // highlight roll-off a lift gets is counted rather than modelled as a hard clip.
    let space = aura_render::OutputColour::Srgb;
    let matrix = aura_render::output::working_to_output(space);
    let mut working = Vec::new();
    let mut lum = Vec::new();
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let e = px.encoded(x, y);
            working.push(aura_render::output::display_srgb_to_working(e));
            lum.push(e[0] * 0.2126 + e[1] * 0.7152 + e[2] * 0.0722);
        }
    }
    if working.is_empty() {
        return None;
    }
    let clipped = |ev: f32| {
        let gain = ev.exp2();
        working
            .iter()
            .filter(|p| {
                let out = aura_render::output::map_one(p.map(|v| v * gain), matrix, space);
                out[0].max(out[1]).max(out[2]) >= 0.995
            })
            .count() as f32
            / working.len() as f32
    };
    let base = clipped(0.0);
    let asked = tone.0;
    let allowed = if percentile(&mut lum, 0.5) < 0.3 {
        DARK_LIFT_CLIP_SHARE
    } else {
        LIFT_CLIP_SHARE
    };
    if clipped(asked) - base <= allowed {
        return None;
    }
    let mut ev = asked;
    while ev > 0.0 && clipped(ev) - base > allowed {
        ev = (ev - 0.05).max(0.0);
    }
    tone.0 = ev;
    Some(format!(
        "Exposure held to {ev:+.2} EV (asked {asked:+.2}): more would push {:.0}% of the frame, a saturated colour such as a backdrop or a garment, to the top of a colour channel.",
        (clipped(asked) - base) * 100.0
    ))
}

/// The share of skin near white above which the people in a frame are overexposed.
///
/// Two thirds of a stop over put 2.5% to 9.5% of the skin there on the frames measured, and
/// of thirty-two photographs exposed as intended none had more than 4.1%: a black-and-white
/// portrait lit hard. It was 12% until the histogram stopped being allowed to darken people.
const HOT_SKIN: f32 = 0.06;
/// The share of a frame without people that must sit at white before its highlights are
/// taken as lost rather than as bright.
const BLOWN_SHARE: f32 = 0.05;

/// True when the people in a frame are clearly brighter than the scene around them: a
/// low-key portrait, whose darkness is a choice. The same reading [`face_exposure_cap`] makes.
fn people_lit_above_the_scene(px: &Pixels<'_>, faces: &[PortraitFace]) -> bool {
    if big_faces(faces) == 0 {
        return false;
    }
    let (w, h) = (px.width, px.height);
    let (mut face, mut frame) = (Vec::new(), Vec::new());
    for y in 0..h {
        for x in 0..w {
            let q = px.encoded(x, y);
            frame.push(q[0] * 0.2126 + q[1] * 0.7152 + q[2] * 0.0722);
            let (fx, fy) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
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
        return false;
    }
    let frame_median = percentile(&mut frame, 0.5);
    let face_median = aura_raw::colour::curve::srgb_encode(percentile(&mut face, 0.5));
    face_median > frame_median * 1.25
}

/// The lift below which the frame's brightest tones falling short of white is not taken as
/// evidence on its own: about four fifths of a stop of room.
const WHITE_OBJECT_LIFT: f32 = 0.72;

/// True when something neutral and brighter than the people - cloth, paper, a wall - covers
/// at least half a percent of the frame outside the faces. Its colour is what makes it a
/// witness: skin of every tone carries colour, and a neutral surface that is lighter than the
/// skin beside it should be near white in a photograph exposed for that skin.
fn white_object_short_of_white(px: &Pixels<'_>, faces: &[PortraitFace], face_median: f32) -> bool {
    let (w, h) = (px.width, px.height);
    let step = ((w * h) as f32 / 120_000.0).sqrt().ceil().max(1.0) as usize;
    let (mut total, mut white) = (0_usize, 0_usize);
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            total += 1;
            if in_faces(
                faces,
                (x as f32 + 0.5) / w as f32,
                (y as f32 + 0.5) / h as f32,
            ) {
                continue;
            }
            let q = px.encoded(x, y);
            let (max, min) = (q[0].max(q[1]).max(q[2]), q[0].min(q[1]).min(q[2]));
            let display = q[0] * 0.2126 + q[1] * 0.7152 + q[2] * 0.0722;
            white +=
                usize::from(max > 0.05 && (max - min) / max < 0.12 && display > face_median * 1.15);
        }
    }
    total > 0 && white as f32 / total as f32 >= 0.005
}

/// True when the brightest tones of the frame are not the people: a shirt, a blanket, a page or
/// a window, not skin. Those should come near white in a frame exposed for the people in it,
/// so when they do not, the frame was exposed too low. When the brightest thing in the frame
/// is the people's own skin, its distance from white says nothing - a dark complexion in a
/// dark room is not underexposed - and no lift is made on it.
fn brightest_is_not_the_people(px: &Pixels<'_>, faces: &[PortraitFace]) -> bool {
    let (w, h) = (px.width, px.height);
    let step = ((w * h) as f32 / 120_000.0).sqrt().ceil().max(1.0) as usize;
    let mut tones: Vec<(f32, bool)> = Vec::new();
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let people = in_faces(
                faces,
                (x as f32 + 0.5) / w as f32,
                (y as f32 + 0.5) / h as f32,
            );
            tones.push((luma(px.linear(x, y)), people));
        }
    }
    tones.sort_by(|a, b| b.0.total_cmp(&a.0));
    let Some(top) = tones.get(..(tones.len() / 100).max(1)) else {
        return false;
    };
    top.iter().filter(|(_, people)| !people).count() as f32 / top.len() as f32 >= 0.7
}

/// How much of the detected faces' skin is near white.
///
/// A clipped colour channel on skin was tried as a more sensitive witness and dropped. On the
/// photographs it was set on, skin as delivered had none and the same frames two thirds of a
/// stop over had 4% to 30%; on twenty-two photographs it had never seen, bright commercial
/// portraits exposed exactly as intended had 3% to 14%, warm light raised it further, and no
/// threshold separated the two. Skin near white in luminance is rarer and is kept.
struct Skin {
    /// Brighter than 85% in linear light.
    hot: f32,
}

impl Skin {
    fn of(px: &Pixels<'_>, faces: &[PortraitFace]) -> Option<Self> {
        let (w, h) = (px.width, px.height);
        let (mut skin, mut hot) = (0_usize, 0_usize);
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                if faces.iter().any(|f| {
                    let [l0, t0, r0, b0] = f.bounds;
                    let (mw, mh) = ((r0 - l0) * 0.2, (b0 - t0) * 0.2);
                    fx > l0 + mw && fx < r0 - mw && fy > t0 + mh && fy < b0 - mh
                }) {
                    skin += 1;
                    hot += usize::from(luma(px.linear(x, y)) > 0.85);
                }
            }
        }
        (skin >= 16).then(|| Self {
            hot: hot as f32 / skin as f32,
        })
    }

    /// How fully the evidence says the people are overexposed, 0 to 1 and beyond.
    fn overexposure(&self) -> f32 {
        self.hot / HOT_SKIN
    }
}

/// Take highlights down only as far as something says they were lost.
///
/// The histogram asks for highlight recovery whenever the brightest twentieth of a frame is
/// bright, and in a finished photograph that is a white blanket, a dress, a studio wall: on a
/// file that is already display-referred there is no detail behind them to bring back, and
/// lowering them only greys what was white. With people in the frame the witness is their
/// skin; without, it is how much of the frame sits at white, and an open sky is left to the
/// histogram because a sky does hold detail short of white.
///
/// Returns whether the people are overexposed, and the note when the request was reduced.
#[allow(clippy::cast_possible_truncation)]
fn highlights_on_evidence(
    tone: &mut (f32, i16, i16, i16),
    px: &Pixels<'_>,
    faces: &[PortraitFace],
) -> (bool, Option<String>) {
    let asked = tone.1;
    let (share, of, overexposed) = if big_faces(faces) > 0 {
        let Some(skin) = Skin::of(px, faces) else {
            return (false, None);
        };
        let evidence = skin.overexposure();
        (
            evidence,
            "of the skin on the detected faces is",
            evidence >= 1.0,
        )
    } else {
        let m = measure(px, faces);
        if m.sky > 0.1 {
            return (false, None);
        }
        let (w, h) = (px.width, px.height);
        let white = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|(x, y)| px.encoded(*x, *y).iter().all(|v| *v >= 0.97))
            .count() as f32
            / (w * h).max(1) as f32;
        (white / BLOWN_SHARE, "of the frame is", false)
    };
    if asked >= 0 || share >= 1.0 {
        return (overexposed, None);
    }
    tone.1 = (f32::from(asked) * share.clamp(0.0, 1.0)).round() as i16;
    if tone.1 == asked {
        return (overexposed, None);
    }
    let limit = if big_faces(faces) > 0 {
        HOT_SKIN
    } else {
        BLOWN_SHARE
    };
    (
        overexposed,
        Some(format!(
            "Highlights {:+} (the histogram asked for {asked:+}): under {:.1}% {of} at or near white, so the bright tones are the scene's own and lowering them would grey them.",
            tone.1,
            (share * limit * 100.0).max(0.1)
        )),
    )
}

/// The previews and the people one photograph's light is judged from.
pub(crate) struct Seen {
    thumb: std::sync::Arc<aura_raw::PixelBuffer>,
    proxy: Option<std::sync::Arc<aura_raw::PixelBuffer>>,
    faces: Vec<PortraitFace>,
}

impl Seen {
    /// Decode the previews and find the faces, as Auto enhance does.
    ///
    /// # Errors
    /// A preview that cannot be produced or is not sRGB, or a failed detection.
    pub(crate) fn of(
        state: &AppState,
        photo: PhotoId,
        project_id: &str,
    ) -> aura_core::AuraResult<Self> {
        let previews = state.previews(project_id)?;
        let thumb = previews.get(
            photo,
            aura_raw::PixelLevel::Thumb(512),
            Priority::Interactive,
        )?;
        let rgb = thumb.as_srgb8().ok_or_else(|| {
            aura_core::errors::render::recipe_invalid("photo", "An sRGB preview is required")
        })?;
        let proxy = previews
            .get(
                photo,
                aura_raw::PixelLevel::Proxy2048,
                Priority::Interactive,
            )
            .ok();
        let found = aura_vision::portrait::detect(rgb, thumb.width, thumb.height)?;
        let faces = match proxy
            .as_ref()
            .and_then(|p| p.as_srgb8().map(|pixels| (pixels, p.width, p.height)))
        {
            Some((pixels, w, h)) => {
                aura_vision::portrait::detect_small_faces(pixels, w, h, &found).unwrap_or(found)
            }
            None => found,
        };
        Ok(Self {
            thumb,
            proxy,
            faces,
        })
    }

    /// [`guard_tone`] on this photograph. `None` when the move stands as proposed.
    pub(crate) fn guard(&self, tone: &mut (f32, i16, i16, i16)) -> Option<String> {
        let rgb = self.thumb.as_srgb8()?;
        let px = Pixels::new(rgb, self.thumb.width, self.thumb.height)?;
        guard_tone(tone, &px, &self.faces)
    }

    /// AURA's own measured light and colour, guarded, written into a copy of `base`, with the
    /// decisions in words. Nothing is saved and no retouch is planned: this is the light half
    /// of Auto enhance alone, for a caller that has no better proposal.
    ///
    /// # Errors
    /// Pixels that cannot be measured.
    pub(crate) fn measured_light(
        &self,
        state: &AppState,
        photo: PhotoId,
        base: &Recipe,
    ) -> aura_core::AuraResult<(Recipe, Vec<String>)> {
        let invalid =
            || aura_core::errors::render::recipe_invalid("photo", "An sRGB preview is required");
        let rgb = self.thumb.as_srgb8().ok_or_else(invalid)?;
        let small = Pixels::new(rgb, self.thumb.width, self.thumb.height).ok_or_else(invalid)?;
        let mut tone = crate::photo_enhance::correction(rgb)?;
        if veiled(&small, &self.faces) {
            tone = (0.0, 0, 0, 0);
        }
        let note = guard_tone(&mut tone, &small, &self.faces);
        let exposure = effective_exposure(base, tone.0, true);
        let frame = crate::photo_frames::CatalogFrames::new(state.clone())
            .frame(&photo, RenderLevel::Proxy2048)
            .ok();
        let large = self
            .proxy
            .as_ref()
            .and_then(|p| Pixels::new(p.as_srgb8()?, p.width, p.height));
        let developed = self.thumb.source != aura_raw::PixelSource::Demosaiced;
        let mut plan = analyse_from(
            large.as_ref().unwrap_or(&small),
            frame.as_ref(),
            &self.faces,
            exposure,
            developed,
        );
        if let Some(note) = camera_file_contrast(&mut tone, developed) {
            plan.decisions.push(note);
        }
        if let Some(note) = note {
            plan.decisions.insert(1.min(plan.decisions.len()), note);
        }
        let look = crate::adaptive_look::enabled().then(|| {
            crate::adaptive_look::plan(
                large.as_ref().unwrap_or(&small),
                &self.faces,
                plan.kind,
                plan.monochrome,
                tone.3,
            )
        });
        if let Some(look) = &look {
            if let Some(contrast) = look.contrast {
                tone.3 = contrast;
            }
            plan.vibrance = (plan.vibrance + look.vibrance).clamp(0, 100);
            plan.decisions.extend(look.decisions.iter().cloned());
        }
        let mut proposal = base.clone();
        apply_global(&mut proposal, tone, &plan);
        if let Some(look) = &look {
            crate::adaptive_look::apply(look, &mut proposal.global);
        }
        Ok((proposal, plan.decisions))
    }
}

/// Keep the automatic retouch of a crowded photograph inside the recipe's selection budget
/// ([`retouch_tools::MAX_MATTES`]), alongside any selections a person's own operations use.
///
/// Every automatic operation belongs to one face (`auto-portrait-v1-{face}-...`). Faces are kept
/// largest first while their selections fit; a face that does not fit loses all of its
/// operations, so nobody is left half retouched. Returns how many faces were kept and a note
/// for the report, or `None` when everything fits.
fn fit_matte_budget(
    groups: &mut BTreeMap<Group, Vec<Edit>>,
    base: &Recipe,
    faces: &[PortraitFace],
) -> Option<(usize, String)> {
    use std::collections::BTreeSet;
    let face_of = |id: &str| -> Option<usize> {
        id.strip_prefix(portrait_auto::PREFIX)?
            .split('-')
            .next()?
            .parse()
            .ok()
    };
    let own: BTreeSet<String> = retouch_tools::read(base)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| !e.id.starts_with(portrait_auto::PREFIX))
        .filter_map(|e| e.matte)
        .collect();
    let mut by_face: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    let mut shared: BTreeSet<String> = own;
    for edit in groups.values().flatten() {
        if let Some(matte) = &edit.matte {
            match face_of(&edit.id) {
                Some(face) => {
                    by_face.entry(face).or_default().insert(matte.clone());
                }
                None => {
                    shared.insert(matte.clone());
                }
            }
        }
    }
    let total: BTreeSet<&String> = shared.iter().chain(by_face.values().flatten()).collect();
    if total.len() <= retouch_tools::MAX_MATTES {
        return None;
    }
    let area = |face: usize| {
        faces.get(face).map_or(0.0, |f| {
            let [l, t, r, b] = f.bounds;
            ((r - l) * (b - t)).max(0.0)
        })
    };
    let mut order: Vec<usize> = by_face.keys().copied().collect();
    order.sort_by(|a, b| area(*b).total_cmp(&area(*a)).then(a.cmp(b)));
    let mut used: BTreeSet<String> = shared;
    let mut kept: BTreeSet<usize> = BTreeSet::new();
    for face in order {
        let Some(mattes) = by_face.get(&face) else {
            continue;
        };
        let more = mattes.iter().filter(|m| !used.contains(*m)).count();
        if used.len() + more <= retouch_tools::MAX_MATTES {
            used.extend(mattes.iter().cloned());
            kept.insert(face);
        }
    }
    let dropped = by_face.len() - kept.len();
    for edits in groups.values_mut() {
        edits.retain(|e| face_of(&e.id).is_none_or(|face| kept.contains(&face)));
    }
    Some((
        kept.len(),
        format!(
            "This photograph has {} faces to retouch and one edit holds at most {} skin selections, so the {} largest faces were retouched in full and the {dropped} smallest were left as they are.",
            by_face.len(),
            retouch_tools::MAX_MATTES,
            kept.len()
        ),
    ))
}

/// Selections must use the exposure that survives the recipe merge.
pub(crate) fn effective_exposure(base: &Recipe, proposed: f32, global: bool) -> f32 {
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
    let busy = crate::activity::begin(
        state,
        if global { "edit" } else { "retouch" },
        format!(
            "{} {}",
            if global {
                "Auto edit:"
            } else {
                "Auto retouch:"
            },
            crate::activity::photo_name(state, photo_id)
        ),
    );
    let _total = crate::perf::step(state, "edit.total");
    busy.detail("Waiting for a free image worker");
    // Residual planning renders working skin buffers directly. Share the same
    // gate as previews and exports so they cannot inflate its transient peak.
    let waited = crate::perf::step(state, "edit.wait_for_worker");
    let _image_work = state.image_work_gate().acquire();
    drop(waited);
    busy.detail("Measuring light, colour and faces");
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
    let thumb = {
        let _t = crate::perf::step(state, "edit.preview_thumb");
        previews.get(
            photo,
            aura_raw::PixelLevel::Thumb(512),
            Priority::Interactive,
        )?
    };
    let Some(rgb) = thumb.as_srgb8() else {
        return Err(invalid("An sRGB preview is required").into());
    };
    // Fine features (spots, eyes, teeth, noise) need more pixels than the thumbnail has.
    let proxy = {
        let _t = crate::perf::step(state, "edit.preview_proxy");
        previews
            .get(
                photo,
                aura_raw::PixelLevel::Proxy2048,
                Priority::Interactive,
            )
            .ok()
    };
    let detail = proxy
        .as_ref()
        .and_then(|p| p.as_srgb8().map(|pixels| (pixels, p.width, p.height)));
    let mut tone = crate::photo_enhance::correction(rgb)?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    // A later Auto enhance repeats whatever finishing the photographer last chose.
    let options = chosen.unwrap_or_else(|| {
        if crate::internal_ai::has_custom_skin_preferences(&base) {
            base.extra
                .get(portrait_auto::KEY)
                .and_then(|report| report.get("options"))
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_else(|| crate::internal_ai::fresh_skin_options(None))
        } else {
            crate::internal_ai::fresh_skin_options(None)
        }
    });
    let disabled = std::env::var_os("AURA_DISABLE_AUTO_PORTRAIT").is_some_and(|v| v == "1");
    let faces = if disabled {
        Vec::new()
    } else {
        let found = {
            let _t = crate::perf::step(state, "edit.faces_detect");
            aura_vision::portrait::detect(rgb, thumb.width, thumb.height)?
        };
        // Group photos and full-length portraits: look again, tile by tile, on the proxy.
        let _t = crate::perf::step(state, "edit.faces_small");
        match detail {
            Some((pixels, w, h)) => {
                aura_vision::portrait::detect_small_faces(pixels, w, h, &found).unwrap_or(found)
            }
            None => found,
        }
    };
    busy.detail(format!("Planning retouch for {} face(s)", faces.len()));
    let mut exposure_note = None;
    if global {
        if let Some(px) = Pixels::new(rgb, thumb.width, thumb.height) {
            if veiled(&px, &faces) {
                tone = (0.0, 0, 0, 0);
            }
            exposure_note = guard_tone(&mut tone, &px, &faces);
        }
    }
    // Luminance selections are evaluated after the global exposure the same pass applies.
    let exposure = effective_exposure(&base, tone.0, global);
    let planning = crate::perf::step(state, "edit.retouch_plan");
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
    drop(planning);
    let retouch_owned = chosen.is_some()
        || base.provenance.user_edited_fields.iter().any(|path| {
            path == retouch_tools::KEY || path.starts_with(&format!("{}.", retouch_tools::KEY))
        });
    let mut report = portrait.report.clone();
    // A chosen face-only preference must survive even when no face/operations
    // are found. Operation provenance alone cannot represent that choice.
    report.explicit_options = chosen.is_some().then_some(true).or_else(|| {
        base.extra
            .get(portrait_auto::KEY)
            .and_then(|value| value.get("explicitOptions"))
            .and_then(serde_json::Value::as_bool)
    });
    let mut groups = portrait.groups.clone();
    // A crowded group can need more skin selections than one recipe holds. Retouch the
    // largest faces in full rather than refusing the whole photograph.
    if let Some(note) = fit_matte_budget(&mut groups, &base, &report.faces) {
        report.retouched_faces = report.retouched_faces.min(note.0);
        report.message = format!("{} {}", report.message, note.1);
    }

    let mut global_plan = None;
    let mut global_look: Option<crate::adaptive_look::Look> = None;
    if global {
        let frame = crate::photo_frames::CatalogFrames::new(state.clone())
            .frame(&photo, RenderLevel::Proxy2048)
            .ok();
        let px = detail
            .and_then(|(d, w, h)| Pixels::new(d, w, h))
            .or_else(|| Pixels::new(rgb, thumb.width, thumb.height));
        if let Some(px) = px {
            let developed = thumb.source != aura_raw::PixelSource::Demosaiced;
            let analysing = crate::perf::step(state, "edit.light_colour_plan");
            let mut plan = analyse_from(&px, frame.as_ref(), &faces, exposure, developed);
            drop(analysing);
            if let Some(note) = camera_file_contrast(&mut tone, developed) {
                plan.decisions.push(note);
            }
            // The finishing look, measured on this frame after its corrections are decided.
            if crate::adaptive_look::enabled() {
                let look = {
                    let _t = crate::perf::step(state, "edit.look_plan");
                    crate::adaptive_look::plan(&px, &faces, plan.kind, plan.monochrome, tone.3)
                };
                if let Some(contrast) = look.contrast {
                    tone.3 = contrast;
                }
                plan.vibrance = (plan.vibrance + look.vibrance).clamp(0, 100);
                plan.decisions.extend(look.decisions.iter().cloned());
                global_look = Some(look);
            }
            if let Some(note) = exposure_note.take() {
                plan.decisions.insert(1, note);
            }
            groups.insert(
                Group::Scene,
                plan.sky
                    .clone()
                    .into_iter()
                    .chain(plan.defringe.clone())
                    .collect(),
            );
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
        let (exposure, _, _, contrast) = tone;
        let (highlights, shadows) = plan.highlights_shadows(tone);
        steps.push(Step {
            group: None,
            title: "Light & colour".into(),
            detail: format!(
                "{}: exposure {exposure:+.2} EV, highlights {highlights}, shadows {}, whites {:+}, blacks {:+}, contrast {contrast:+}, vibrance {:+}{}",
                plan.kind.label(),
                shadows,
                plan.whites,
                plan.blacks,
                plan.vibrance,
                plan.white_balance
                    .map_or(String::new(), |(k, t)| format!(", white balance {k} K / {t:+}")),
            ) + &global_look.as_ref().filter(|l| !l.is_empty()).map_or(String::new(), |l| {
                format!(
                    "; look: curve {:+}/{:+}/{:+}, {} colour band(s), split tone {}/{}",
                    l.parametric.shadows,
                    l.parametric.darks,
                    l.parametric.lights,
                    l.hsl.len(),
                    l.grade.highlights.saturation,
                    l.grade.shadows.saturation
                )
            }),
            operations: 0,
        });
        let scene: Vec<(&str, &str)> = [
            plan.sky.as_ref().map(|_| ("Sky balance", "Feathered gradient over bright sky pixels only.")),
            plan.defringe.as_ref().map(|_| ("Purple fringes", "Purple rims on the dark side of bright edges take the colour of their surroundings.")),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !scene.is_empty() {
            steps.push(Step {
                group: Some(Group::Scene),
                title: scene
                    .iter()
                    .map(|(t, _)| *t)
                    .collect::<Vec<_>>()
                    .join(" & "),
                detail: scene.iter().map(|(_, d)| *d).collect::<Vec<_>>().join(" "),
                operations: scene.len(),
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
        busy.progress(i as u64, total as u64);
        busy.detail(format!("Step {} of {total}: {}", i + 1, step.title));
        let _t = crate::perf::step(state, "edit.save_step");
        let mut proposal = current.clone();
        if i == 0 {
            if let Some(plan) = &global_plan {
                apply_global(&mut proposal, tone, plan);
            }
            if let Some(look) = &global_look {
                crate::adaptive_look::apply(look, &mut proposal.global);
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
                let mattes = {
                    let _t = crate::perf::step(state, "edit.mattes");
                    portrait_auto::mattes_for(&current, &portrait)?
                };
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

    /// A working frame with a bright neutral window in a dark room; round its edge, `rim`
    /// pixels of `fringe`, and the rest of the room `room`.
    fn room_with_window(rim: usize, fringe: [f32; 3], room: [f32; 3]) -> aura_render::Frame {
        let (w, h) = (300_usize, 200_usize);
        let (x0, x1, y0, y1) = (w / 3, 2 * w / 3, h / 3, 2 * h / 3);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let inside = (x0..x1).contains(&x) && (y0..y1).contains(&y);
                let out_by = [
                    x0.saturating_sub(x),
                    x.saturating_sub(x1 - 1),
                    y0.saturating_sub(y),
                    y.saturating_sub(y1 - 1),
                ]
                .into_iter()
                .max()
                .unwrap_or(0);
                rgb.extend(if inside {
                    [0.9, 0.9, 0.9]
                } else if out_by <= rim {
                    fringe
                } else {
                    room
                });
            }
        }
        aura_render::Frame::working(rgb, w as u32, h as u32, "")
    }

    fn planned_for(frame: &aura_render::Frame) -> GlobalPlan {
        let bytes: Vec<u8> = frame
            .rgb
            .iter()
            .map(|v| {
                (aura_raw::colour::curve::srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u8
            })
            .collect();
        let px = Pixels::new(&bytes, frame.width, frame.height).unwrap();
        analyse(&px, Some(frame), &[], 0.0)
    }

    #[test]
    fn purple_rims_round_a_bright_window_are_proposed_for_removal() {
        let plan = planned_for(&room_with_window(2, [0.07, 0.03, 0.09], [0.04, 0.04, 0.04]));
        let op = plan.defringe.clone().expect("a defringe");
        assert_eq!(op.id, DEFRINGE_ID);
        assert!(op.id.starts_with(portrait_auto::SCENE_PREFIX));
        assert!(op.warmth > 0.0 && op.tint == 0.0, "purple only: {op:?}");
        assert_eq!(
            op.region,
            [0.5, 0.5, 1.0, 1.0],
            "the corners are selected too"
        );
        retouch_tools::validate(std::slice::from_ref(&op)).unwrap();
        assert!(plan
            .decisions
            .iter()
            .any(|d| d.starts_with("Purple fringes removed")));
    }

    #[test]
    fn the_planned_defringe_removes_the_rim_in_the_rendered_photograph() {
        use aura_render::{CpuEngine, Frame, FrameSource, OutputSpec, RenderPurpose, RenderedData};
        #[derive(Debug)]
        struct Source(Frame);
        impl FrameSource for Source {
            fn frame(&self, _: &PhotoId, _: RenderLevel) -> aura_core::AuraResult<Frame> {
                Ok(self.0.clone())
            }
        }
        let frame = room_with_window(2, [0.07, 0.03, 0.09], [0.04, 0.04, 0.04]);
        let op = planned_for(&frame).defringe.expect("a defringe");
        let base = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "");
        let mut recipe = base.clone();
        retouch_tools::write_with_mattes(&mut recipe, &[op], &BTreeMap::new()).unwrap();
        let engine = CpuEngine::new(
            std::sync::Arc::new(Source(frame.clone())),
            aura_core::clock::FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );
        let render = |r: &Recipe| -> Vec<u8> {
            let image = engine
                .render_frame(
                    &frame,
                    r,
                    RenderLevel::Full,
                    RenderPurpose::Analysis,
                    &OutputSpec::default(),
                )
                .unwrap();
            let RenderedData::Eight(data) = image.data else {
                panic!("an 8-bit render")
            };
            data
        };
        let spread =
            |p: &[u8]| i32::from(*p.iter().max().unwrap()) - i32::from(*p.iter().min().unwrap());
        // On the rim, just outside the window's left edge.
        let at = (100 * 300 + 99) * 3;
        let (as_shot, cleaned) = (render(&base), render(&recipe));
        assert!(
            spread(&as_shot[at..at + 3]) > 20,
            "{:?}",
            &as_shot[at..at + 3]
        );
        assert!(
            spread(&cleaned[at..at + 3]) <= 3,
            "{:?}",
            &cleaned[at..at + 3]
        );
        // Away from the edge the room is exactly as it was.
        let room = (20 * 300 + 20) * 3;
        assert_eq!(&cleaned[room..room + 3], &as_shot[room..room + 3]);
    }

    #[test]
    fn a_purple_room_or_a_clean_edge_proposes_nothing() {
        let purple = [0.07, 0.03, 0.09];
        assert!(planned_for(&room_with_window(0, purple, purple))
            .defringe
            .is_none());
        let grey = [0.04, 0.04, 0.04];
        assert!(planned_for(&room_with_window(0, grey, grey))
            .defringe
            .is_none());
    }

    fn pixels(rgb: &[u8], w: u32, h: u32) -> Pixels<'_> {
        Pixels::new(rgb, w, h).unwrap()
    }

    /// A textured frame: `base` everywhere, `white` over the top tenth when given, each sample
    /// varied by a fixed pattern so the frame has edges, all multiplied by `light`.
    fn lit_frame(base: [f32; 3], white: Option<f32>, light: [f32; 3]) -> aura_render::Frame {
        let (w, h) = (96_usize, 96_usize);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let texture = 0.85 + 0.3 * (((x * 7 + y * 13) % 11) as f32 / 10.0);
                let surface = match white {
                    Some(v) if y < h / 10 => [v; 3],
                    _ => base,
                };
                for c in 0..3 {
                    rgb.push(surface[c] * texture * light[c]);
                }
            }
        }
        aura_render::Frame::working(rgb, w as u32, h as u32, "test")
    }

    #[test]
    fn a_lift_that_would_clip_a_saturated_backdrop_is_held_back() {
        // Two thirds of the frame a saturated pink whose red channel is near the top; the rest
        // a dim grey that wants lifting.
        let (w, h) = (90_usize, 60_usize);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for _ in 0..h {
            for x in 0..w {
                rgb.extend_from_slice(if x < 60 {
                    &[225, 90, 170]
                } else {
                    &[70, 70, 70]
                });
            }
        }
        let px = pixels(&rgb, w as u32, h as u32);
        let mut tone = (0.6, 0, 0, 0);
        let note = colour_clip_cap(&mut tone, &px).expect("held");
        // The pink's red channel (225) reaches the top of the output at about +0.4 EV once
        // the renderer's highlight roll-off is counted.
        assert!(tone.0 > 0.2 && tone.0 < 0.5, "{tone:?} {note}");
        // Nothing saturated near the top: the lift stands.
        let grey = vec![80_u8; w * h * 3];
        let mut lift = (0.6, 0, 0, 0);
        assert!(colour_clip_cap(&mut lift, &pixels(&grey, w as u32, h as u32)).is_none());
        assert!((lift.0 - 0.6).abs() < 1e-6);
    }

    #[test]
    fn a_crowded_group_keeps_its_largest_faces_inside_the_selection_budget() {
        // 20 faces, 5 selections each: 100 against a budget of 64. Face i is (i + 1) wide.
        let faces: Vec<PortraitFace> = (0..20)
            .map(|i| PortraitFace {
                bounds: [0.0, 0.0, 0.01 * (i as f32 + 1.0), 0.05],
                landmarks: [[0.0; 2]; 5],
                confidence: 0.9,
            })
            .collect();
        let mut edits = Vec::new();
        for face in 0..20 {
            for part in 0..5 {
                let mut e = crate::advanced_retouch::base_edit(
                    format!("{}{face}-op{part}", portrait_auto::PREFIX),
                    Tool::Dodge,
                    [0.5, 0.5, 0.1, 0.1],
                    0.2,
                );
                e.matte = Some(format!("{}{face}-matte{part}", portrait_auto::PREFIX));
                edits.push(e);
            }
        }
        let mut groups = BTreeMap::from([(Group::Skin, edits)]);
        let base = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t");
        let (kept, note) = fit_matte_budget(&mut groups, &base, &faces).expect("over budget");
        assert_eq!(kept, 12, "{note}");
        let mattes: std::collections::BTreeSet<_> = groups
            .values()
            .flatten()
            .filter_map(|e| e.matte.clone())
            .collect();
        assert!(mattes.len() <= retouch_tools::MAX_MATTES);
        // The kept faces are the twelve largest, and each is kept whole.
        assert!(groups.values().flatten().all(|e| {
            let face: usize =
                e.id.strip_prefix(portrait_auto::PREFIX)
                    .unwrap()
                    .split('-')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
            face >= 8
        }));
        assert_eq!(groups.values().flatten().count(), 60);
        // A photograph inside the budget is not touched.
        let mut small = BTreeMap::from([(Group::Skin, groups[&Group::Skin][..10].to_vec())]);
        assert!(fit_matte_budget(&mut small, &base, &faces).is_none());
    }

    #[test]
    fn a_cast_that_lines_up_with_the_colours_filling_the_frame_is_the_scene() {
        // A room full of pink saris: the "cast" every estimate reads points at pink.
        let pink = lit_frame([0.55, 0.18, 0.3], None, [1.0, 1.0, 1.0]);
        assert!(scene_colour_explains(&pink, &[], (0.25, 0.1)));
        // A green cast on the same frame is not explained by its colours.
        assert!(!scene_colour_explains(&pink, &[], (-0.2, -0.05)));
        // A grey room has nothing to explain a cast with: the light is the cause.
        let grey = lit_frame([0.3, 0.3, 0.3], Some(0.6), [1.1, 1.0, 0.85]);
        assert!(!scene_colour_explains(&grey, &[], (0.2, -0.2)));
    }

    #[test]
    fn a_cast_the_whites_confirm_is_corrected() {
        let frame = lit_frame([0.3, 0.3, 0.3], Some(0.6), [0.9, 1.0, 1.1]);
        assert!(
            matches!(
                white_balance(&frame, &[], true),
                Neutral::Correct {
                    confirmed: true,
                    ..
                }
            ),
            "{:?}",
            white_balance(&frame, &[], true)
        );
    }

    #[test]
    fn a_colour_the_whites_do_not_confirm_is_the_scenes_and_is_kept() {
        // A dim rose backdrop and nothing white: both estimates read rose, mildly enough that
        // the no-people mood rule does not decide it, and nothing says the light is.
        let frame = lit_frame([0.24, 0.2, 0.21], None, [1.0, 1.0, 1.0]);
        assert!(
            matches!(
                white_balance(&frame, &[], true),
                Neutral::Unsure { reason } if reason.contains("whites do not show")
            ),
            "{:?}",
            white_balance(&frame, &[], true)
        );
    }

    #[test]
    fn a_veiled_frame_gets_its_end_points_back_and_a_soft_finish_does_not() {
        // A flat picture profile: nothing below 28% or above 90%.
        let points = levels(0.28, 0.9).expect("a veil at both ends");
        assert!(points[0][0] > 40 && points[1][0] < 245, "{points:?}");
        // A matte finish lifts its blacks to about 16%; that is the photograph's own.
        assert_eq!(levels(0.16, 0.9), None);
        // A frame that reaches white has no veil over its top, whatever its blacks do.
        assert_eq!(levels(0.3, 0.97), None);
        // The curve the points are drawn as is a valid recipe curve and a straight run.
        let curve = levels_curve(points);
        let mut recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t");
        recipe.global.channel_curves.red.points.clone_from(&curve);
        schema::Validation::check(&recipe).expect("a valid curve");
        let (x0, x1) = (f32::from(points[0][0]), f32::from(points[1][0]));
        for pair in curve.windows(2).skip(2).take(curve.len() - 4) {
            let [a, b] = [pair[0], pair[1]];
            let slope = f32::from(b[1] - a[1]) / f32::from(b[0] - a[0]);
            assert!((slope - 255.0 / (x1 - x0)).abs() < 0.2, "{curve:?}");
        }
    }

    #[test]
    fn a_faint_cast_is_the_photographs_own_and_a_strong_one_is_reduced_not_removed() {
        // About 5% either way: a cool studio, an evening room. Kept.
        let faint = lit_frame([0.3, 0.3, 0.3], Some(0.6), [0.95, 1.0, 1.05]);
        assert!(
            matches!(
                white_balance(&faint, &[], true),
                Neutral::Unsure { reason } if reason.contains("faint")
            ),
            "{:?}",
            white_balance(&faint, &[], true)
        );
        // From a camera file the same faint cast is the camera's guess, not a choice: corrected.
        assert!(
            matches!(white_balance(&faint, &[], false), Neutral::Correct { .. }),
            "{:?}",
            white_balance(&faint, &[], false)
        );
        // A strong cast the whites confirm: corrected, but only the part beyond a choice.
        let strong = lit_frame([0.3, 0.3, 0.3], Some(0.6), [0.9, 1.0, 1.12]);
        match white_balance(&strong, &[], true) {
            Neutral::Correct { strength, cast, .. } => {
                assert!((strength - (cast - CAST_BY_CHOICE) / cast).abs() < 1e-4);
                assert!(strength < 0.85);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn skin_at_white_is_overexposed_and_bright_or_warm_skin_is_not() {
        let face = PortraitFace {
            bounds: [0.2, 0.2, 0.8, 0.8],
            landmarks: [
                [0.35, 0.4],
                [0.65, 0.4],
                [0.5, 0.5],
                [0.4, 0.65],
                [0.6, 0.65],
            ],
            confidence: 0.95,
        };
        let frame = |skin: [u8; 3]| {
            // A warm-lit grey wall unless the skin is blown, when the wall is blown with it.
            let wall = if skin[1] >= 230 { 255 } else { 200 };
            let mut rgb = vec![wall; 100 * 100 * 3];
            for y in 20..80 {
                for x in 20..80 {
                    rgb[(y * 100 + x) * 3..(y * 100 + x + 1) * 3].copy_from_slice(&skin);
                }
            }
            rgb
        };
        // Bright, warm skin with every channel recorded: not evidence of anything.
        let bright = frame([235, 200, 180]);
        let px = pixels(&bright, 100, 100);
        let (used, _) = face_exposure_cap(0.0, &px, std::slice::from_ref(&face));
        assert!(used.abs() < 1e-6, "{used}");
        let mut tone = (0.0, -25, 0, 0);
        let (overexposed, _) = highlights_on_evidence(&mut tone, &px, std::slice::from_ref(&face));
        assert!(!overexposed);
        assert_eq!(tone.1, 0, "a bright backdrop is not lost highlight detail");
        // Warm light with the red at the ceiling: not overexposure either.
        let warm = frame([255, 205, 170]);
        let px = pixels(&warm, 100, 100);
        let (used, note) = face_exposure_cap(0.0, &px, std::slice::from_ref(&face));
        assert!(used.abs() < 1e-6, "{used} {note:?}");
        // Skin at white: overexposed, brought down, and its highlights recovered.
        let blown = frame([252, 248, 244]);
        let px = pixels(&blown, 100, 100);
        let (used, note) = face_exposure_cap(0.0, &px, std::slice::from_ref(&face));
        assert!(used <= -0.2, "{used} {note:?}");
        let mut tone = (0.0, -25, 0, 0);
        let (overexposed, _) = highlights_on_evidence(&mut tone, &px, std::slice::from_ref(&face));
        assert!(overexposed);
        assert_eq!(tone.1, -25);
    }

    #[test]
    fn dark_complexions_on_dark_backdrops_do_not_trigger_a_white_anchor_lift() {
        let face = PortraitFace {
            bounds: [0.2, 0.2, 0.8, 0.8],
            landmarks: [
                [0.35, 0.4],
                [0.65, 0.4],
                [0.5, 0.5],
                [0.4, 0.65],
                [0.6, 0.65],
            ],
            confidence: 0.95,
        };
        for skin in [60_u8, 90, 140, 180] {
            let mut rgb = vec![skin / 3; 100 * 100 * 3];
            for y in 20..80 {
                for x in 20..80 {
                    rgb[(y * 100 + x) * 3..(y * 100 + x + 1) * 3].fill(skin);
                }
            }
            let px = pixels(&rgb, 100, 100);
            for asked in [0.0, 0.15, 0.75] {
                let (used, _) = face_exposure_cap(asked, &px, std::slice::from_ref(&face));
                assert!(
                    used <= asked.min(0.25) + 1e-5,
                    "skin {skin}, asked {asked}, used {used}"
                );
            }
        }
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
        let people = [face];
        assert!(respect_intent(&mut tone, &px, &people).is_none());
        assert!(tone.0 < 0.0);
        // The white behind that person is still white by design: highlights are not taken
        // far down for it, and a frame that is not high-key keeps what the histogram asked.
        let mut tone = (0.0, -20, 0, 0);
        let note = keep_high_key(&mut tone, &px, &people).expect("a note");
        assert_eq!(tone, (0.0, -8, 0, 0));
        assert!(note.contains("bright by design"), "{note}");
        let mut gentle = (0.0, -5, 0, 0);
        assert!(keep_high_key(&mut gentle, &px, &people).is_none());
        let mut tone = (0.0, -20, 0, 0);
        assert!(keep_high_key(&mut tone, &px, &[]).is_none());
        let grey = vec![120_u8; w * h * 3];
        let px = pixels(&grey, w as u32, h as u32);
        assert!(keep_high_key(&mut tone, &px, &people).is_none());
        assert_eq!(tone.1, -20);
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
            repair_passes: None,
            target_color: None,
            sensitivity: None,
            keep_dark_marks: false,
            mole_edge: None,
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
