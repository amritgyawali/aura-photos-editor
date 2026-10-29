//! Tone to the photographer's own result: the light of every photograph is read as shot, the
//! lightness the photographer's finished photographs give that light is predicted, and the
//! six tone sliders are solved to reach it - in a model of the renderer first, then checked on
//! the render. ADR-0191.
//!
//! The prediction was learned from the photographer's finished JPEGs paired with the camera
//! files they came from (`crates/aura-app/tests/photographer_pairs.rs`,
//! `scripts/photographer-pairs.py`): 651 photographs, eight cameras, day and night, flash and
//! ambient light. Every quantity is CIE L* (0 black, 100 white) of the rendered sRGB, read the
//! same way here and in the scoring script.
// Pixel statistics convert between f32 and integer sliders on purpose.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::similar_names
)]

use aura_render::edge_aware::tone_pixel;
use aura_render::output::{display_srgb_to_working, map_one, working_to_output};
use aura_render::tonemap::{self, Tone};
use aura_render::OutputColour;

/// The percentiles of lightness a reading takes: the black point, the shadows, the middle, the
/// highlights and the white point.
pub const PERCENTILES: [f32; 5] = [0.02, 0.10, 0.50, 0.90, 0.98];

/// What a frame's light reads as.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// L* at [`PERCENTILES`].
    pub l: [f32; 5],
    /// The median L* of the skin in the middle of every face, when there is one.
    pub face: Option<f32>,
    /// Median chroma (CIE a*b* distance from grey).
    pub chroma: f32,
    /// Share of the frame at the top of a channel.
    pub clipped: f32,
}

fn decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn lab_f(t: f32) -> f32 {
    if t > 0.008_856 {
        t.cbrt()
    } else {
        7.787 * t + 16.0 / 116.0
    }
}

/// CIE L*, a*, b* (D65) of one sRGB colour, each channel `0 ..= 1`.
#[must_use]
pub fn lab(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(|v| decode(v.clamp(0.0, 1.0)));
    let x = (0.412_453 * r + 0.357_580 * g + 0.180_423 * b) / 0.950_456;
    let y = 0.212_671 * r + 0.715_160 * g + 0.072_169 * b;
    let z = (0.019_334 * r + 0.119_193 * g + 0.950_227 * b) / 1.088_754;
    let (fx, fy, fz) = (lab_f(x), lab_f(y), lab_f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIE L* alone, the cheaper half of [`lab`].
#[must_use]
pub fn lightness(rgb: [f32; 3]) -> f32 {
    let [r, g, b] = rgb.map(|v| decode(v.clamp(0.0, 1.0)));
    116.0 * lab_f(0.212_671 * r + 0.715_160 * g + 0.072_169 * b) - 16.0
}

/// Whether the point `(x, y)` (0 to 1) is the skin in the middle of a face box
/// `[left, top, right, bottom]`: not the hair above, the beard below or the background beside.
#[must_use]
pub fn in_skin(faces: &[[f32; 4]], x: f32, y: f32) -> bool {
    faces.iter().any(|&[l, t, r, b]| {
        let (w, h) = (r - l, b - t);
        x >= l + 0.2 * w && x <= l + 0.8 * w && y >= t + 0.25 * h && y <= t + 0.85 * h
    })
}

fn percentile_of(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted.get(i).copied().unwrap_or(0.0)
}

/// Reduce lightness values (and skin values, and chroma) to a reading.
fn reading_of(
    mut l: Vec<f32>,
    mut skin: Vec<f32>,
    mut chroma: Vec<f32>,
    clipped: f32,
) -> Option<Reading> {
    if l.len() < 100 {
        return None;
    }
    let n = l.len();
    l.sort_by(f32::total_cmp);
    skin.sort_by(f32::total_cmp);
    chroma.sort_by(f32::total_cmp);
    Some(Reading {
        l: PERCENTILES.map(|p| percentile_of(&l, p)),
        face: (skin.len() >= 30).then(|| percentile_of(&skin, 0.5)),
        chroma: percentile_of(&chroma, 0.5),
        clipped: clipped / n as f32,
    })
}

/// A pixel at least this light as shot (L*) is a bright background already: a curtain, a
/// wall, a window. Taking it to white loses next to nothing, and the photographer does, so its
/// clipping counts [`BRIGHT_CLIP_WEIGHT`] of a midtone's.
pub const BRIGHT: f32 = 85.0;
/// What clipping a pixel that was already bright as shot counts, against one for a midtone.
pub const BRIGHT_CLIP_WEIGHT: f32 = 0.15;
/// What a saturated colour at the top of one channel counts (a red sari, a pink canopy), against
/// a white burnt out in every channel: the colour keeps its hue and most of its texture.
pub const COLOUR_CLIP_WEIGHT: f32 = 0.05;

/// How much one clipped pixel counts: `top` and `bottom` are its highest and lowest channel
/// (0 to 1), `bright` whether it was already bright as shot.
fn clip_weight(top: f32, bottom: f32, bright: bool) -> f32 {
    if top < 0.995 {
        0.0
    } else if bright {
        BRIGHT_CLIP_WEIGHT
    } else if bottom < 0.9 {
        COLOUR_CLIP_WEIGHT
    } else {
        1.0
    }
}

/// Read an sRGB frame's light. `faces` are boxes `[left, top, right, bottom]`, 0 to 1. Clipping
/// is weighted by the frame itself: what is clipped in it was bright.
#[must_use]
pub fn read(rgb: &[u8], w: usize, h: usize, faces: &[[f32; 4]]) -> Option<Reading> {
    read_against(rgb, w, h, faces, rgb)
}

/// Read a frame's light, weighting its clipping by how light each pixel was in `shot`, a frame
/// of the same size as shot (see [`BRIGHT`]).
#[must_use]
pub fn read_against(
    rgb: &[u8],
    w: usize,
    h: usize,
    faces: &[[f32; 4]],
    shot: &[u8],
) -> Option<Reading> {
    let step = ((w * h) as f32 / 60_000.0).sqrt().ceil().max(1.0) as usize;
    let (mut l, mut skin, mut chroma, mut clipped) = (Vec::new(), Vec::new(), Vec::new(), 0.0);
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let i = (y * w + x) * 3;
            let Some(&[r, g, b]) = rgb.get(i..i + 3) else {
                continue;
            };
            let c = [r, g, b].map(|v| f32::from(v) / 255.0);
            let [lv, a, bv] = lab(c);
            l.push(lv);
            chroma.push(a.hypot(bv));
            if r.max(g).max(b) >= 254 {
                let was = match shot.get(i..i + 3) {
                    Some(&[a, b, c]) => lightness([a, b, c].map(|v| f32::from(v) / 255.0)),
                    _ => 100.0,
                };
                clipped += clip_weight(1.0, c[0].min(c[1]).min(c[2]), was >= BRIGHT);
            }
            if in_skin(
                faces,
                (x as f32 + 0.5) / w as f32,
                (y as f32 + 0.5) / h as f32,
            ) {
                skin.push(lv);
            }
        }
    }
    reading_of(l, skin, chroma, clipped)
}

/// The lightness the photographer's finished photograph has, predicted from the light as
/// shot (a ridge regression on [`features`]; each row is `[p02, p10, p50, p90, p98, face]`),
/// learned from 651 pairs. The comment above each row is how far it lands from the
/// photographer's own photograph on photographs the fit did not see (ten-fold), and how far one
/// fixed value for every photograph would.
const MODEL: [[f32; 9]; 6] = [
    // p02: 4.5 L* (one fixed value: 6.8)
    [0.34, 54.01, 30.09, 6.17, -3.37, 0.46, 2.09, -7.96, 1.14],
    // p10: 6.1 L* (10.7)
    [3.85, 30.77, 82.43, 21.08, -3.81, -3.23, 5.78, -18.60, 3.46],
    // p50: 6.2 L* (12.1)
    [18.08, -8.99, 7.20, 108.74, 5.65, -3.45, 1.89, -31.17, 5.79],
    // p90: 5.3 L* (8.9)
    [44.76, 2.68, 4.87, 25.73, 47.41, -0.83, -0.33, -23.75, 2.07],
    // p98: 4.3 L* (7.2)
    [53.36, 5.27, 2.19, 13.43, 17.43, 28.18, -0.22, -16.93, 0.55],
    // face: 6.1 L* (8.4)
    [33.32, -4.81, 1.75, 11.86, -3.16, -6.25, 0.83, 68.46, 0.00],
];

/// The model's inputs: a constant, the five lightness percentiles and the face as fractions,
/// chroma over 30, and whether there is a face.
#[must_use]
pub fn features(shot: &Reading) -> [f32; 9] {
    let [p02, p10, p50, p90, p98] = shot.l.map(|v| v / 100.0);
    [
        1.0,
        p02,
        p10,
        p50,
        p90,
        p98,
        shot.chroma / 30.0,
        shot.face.map_or(p50, |f| f / 100.0),
        if shot.face.is_some() { 1.0 } else { 0.0 },
    ]
}

/// What kind of light the photograph is, for how far towards the prediction it goes. The pairs
/// it was learned from are nearly all of people; a frame without people is dark, bright or
/// lit by its own light sources on purpose far more often, and is moved less.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// People in it: the prediction in full.
    People,
    /// No people, ordinary light.
    Scene,
    /// No people, dark by nature with light sources in it: a street at night, a stage.
    Night,
    /// No people, low-key: dark throughout on purpose.
    LowKey,
    /// No people, high-key: bright throughout on purpose (snow, a white studio).
    HighKey,
}

impl Intent {
    #[must_use]
    pub fn of(shot: &Reading) -> Self {
        let [_, p10, p50, _, p98] = shot.l;
        if shot.face.is_some() {
            Self::People
        } else if p50 < 22.0 && p98 > 80.0 {
            Self::Night
        } else if p50 < 25.0 {
            Self::LowKey
        } else if p10 > 55.0 {
            Self::HighKey
        } else {
            Self::Scene
        }
    }

    /// How far from the light as shot towards the prediction.
    #[must_use]
    pub const fn share(self) -> f32 {
        match self {
            Self::People => 1.0,
            Self::Scene => 0.8,
            Self::HighKey => 0.6,
            Self::LowKey => 0.5,
            Self::Night => 0.35,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::People => "people",
            Self::Scene => "a scene without people",
            Self::Night => "a night scene lit by its own lights",
            Self::LowKey => "a dark, low-key scene",
            Self::HighKey => "a bright, high-key scene",
        }
    }
}

/// Where the tone is going.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Targets {
    pub l: [f32; 5],
    pub face: Option<f32>,
    pub intent: Intent,
}

/// The photographer's lightness for this light, blended by the scene's intent and kept inside
/// bounds no photograph should leave.
#[must_use]
pub fn targets(shot: &Reading) -> Targets {
    let x = features(shot);
    let predict = |row: &[f32; 9]| row.iter().zip(x).map(|(w, v)| w * v).sum::<f32>();
    let intent = Intent::of(shot);
    let share = intent.share();
    let mut l = [0.0; 5];
    for (i, slot) in l.iter_mut().enumerate() {
        let shot_l = shot.l.get(i).copied().unwrap_or(0.0);
        // Whatever the light, no more than 30 L* brighter or 15 darker than it was shot: the
        // model was learned inside that range and is not trusted outside it.
        let wanted = MODEL
            .get(i)
            .map_or(shot_l, predict)
            .clamp(shot_l - 15.0, shot_l + 30.0);
        *slot = shot_l + share * (wanted - shot_l);
    }
    // A high-key frame is not darkened, a low-key one not flattened into grey.
    if intent == Intent::HighKey {
        for (slot, s) in l.iter_mut().zip(shot.l) {
            *slot = slot.max(s);
        }
    }
    // Monotone, and a white point short of paper white.
    if let Some(white) = l.get_mut(4) {
        *white = white.clamp(60.0, 97.0);
    }
    for i in (0..4).rev() {
        let next = l.get(i + 1).copied().unwrap_or(100.0);
        if let Some(v) = l.get_mut(i) {
            *v = v.min(next - 2.0).max(0.0);
        }
    }
    let face = shot.face.map(|f| {
        let wanted = MODEL.get(5).map_or(f, predict);
        // Skin is never taken into the dark or burnt to white, whatever the light.
        wanted.clamp(f.min(48.0), 74.0)
    });
    Targets { l, face, intent }
}

/// The six sliders the tone is solved with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sliders {
    pub exposure: f32,
    pub contrast: i16,
    pub highlights: i16,
    pub shadows: i16,
    pub whites: i16,
    pub blacks: i16,
}

impl Sliders {
    pub const ZERO: Self = Self {
        exposure: 0.0,
        contrast: 0,
        highlights: 0,
        shadows: 0,
        whites: 0,
        blacks: 0,
    };

    /// The sliders of a recipe.
    #[must_use]
    pub fn of(g: &aura_recipe::Global) -> Self {
        Self {
            exposure: g.exposure,
            contrast: g.contrast,
            highlights: g.highlights,
            shadows: g.shadows,
            whites: g.whites,
            blacks: g.blacks,
        }
    }

    /// Write into a recipe.
    pub fn write(self, g: &mut aura_recipe::Global) {
        g.exposure = (self.exposure * 100.0).round() / 100.0;
        g.contrast = self.contrast;
        g.highlights = self.highlights;
        g.shadows = self.shadows;
        g.whites = self.whites;
        g.blacks = self.blacks;
    }

    fn get(self, i: usize) -> f32 {
        match i {
            0 => self.exposure,
            1 => f32::from(self.contrast),
            2 => f32::from(self.highlights),
            3 => f32::from(self.shadows),
            4 => f32::from(self.whites),
            _ => f32::from(self.blacks),
        }
    }

    fn set(&mut self, i: usize, v: f32) {
        let s = v.round().clamp(-100.0, 100.0) as i16;
        match i {
            0 => self.exposure = (v * 100.0).round() / 100.0,
            1 => self.contrast = s,
            2 => self.highlights = s,
            3 => self.shadows = s,
            4 => self.whites = s,
            _ => self.blacks = s,
        }
    }
}

/// The range each slider is solved in, `(low, high, first step)`: wide enough for any light,
/// narrow enough that no slider alone can wreck a photograph.
const RANGE: [(f32, f32, f32); 6] = [
    (-1.5, 2.0, 0.4),
    (-15.0, 20.0, 8.0),
    (-85.0, 25.0, 25.0),
    (-20.0, 60.0, 20.0),
    (-60.0, 40.0, 20.0),
    (-30.0, 15.0, 10.0),
];

/// The tone-neutral frame, sampled: each sample's working-space colour (before exposure and
/// tone), the log2 luminance of its region (what local Shadows and Highlights read), and
/// whether it is skin.
#[derive(Debug, Clone, Default)]
pub struct Samples {
    working: Vec<[f32; 3]>,
    region: Vec<f32>,
    skin: Vec<bool>,
    /// At least [`BRIGHT`] as shot.
    bright: Vec<bool>,
}

impl Samples {
    #[must_use]
    pub fn len(&self) -> usize {
        self.working.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.working.is_empty()
    }
}

/// Sample a tone-neutral sRGB render (exposure, contrast and the four tone sliders at zero).
#[must_use]
pub fn samples(rgb: &[u8], w: usize, h: usize, faces: &[[f32; 4]]) -> Samples {
    if w == 0 || h == 0 || rgb.len() < w * h * 3 {
        return Samples::default();
    }
    // Region luminance on a coarse grid, the scale local Shadows and Highlights work at.
    let cells = 28_usize;
    let (gw, gh) = if w >= h {
        (cells, (cells * h / w).max(1))
    } else {
        ((cells * w / h).max(1), cells)
    };
    let mut sum = vec![0.0_f64; gw * gh];
    let mut count = vec![0_u32; gw * gh];
    let coarse = ((w * h) as f32 / 90_000.0).sqrt().ceil().max(1.0) as usize;
    for y in (0..h).step_by(coarse) {
        for x in (0..w).step_by(coarse) {
            let i = (y * w + x) * 3;
            let Some(&[r, g, b]) = rgb.get(i..i + 3) else {
                continue;
            };
            let lin = display_srgb_to_working([r, g, b].map(|v| f32::from(v) / 255.0));
            let cell = (y * gh / h).min(gh - 1) * gw + (x * gw / w).min(gw - 1);
            if let (Some(s), Some(c)) = (sum.get_mut(cell), count.get_mut(cell)) {
                *s += f64::from(aura_render::colour::luma(lin).max(1e-6));
                *c += 1;
            }
        }
    }
    let mean: Vec<f32> = sum
        .iter()
        .zip(&count)
        .map(|(s, c)| {
            if *c > 0 {
                (*s / f64::from(*c)) as f32
            } else {
                0.18
            }
        })
        .collect();
    // A 3x3 box over the cells: the region, not the cell edge.
    let at = |cx: usize, cy: usize| -> f32 {
        let mut total = 0.0;
        let mut n = 0.0;
        for dy in 0..3 {
            for dx in 0..3 {
                let (x, y) = ((cx + dx).checked_sub(1), (cy + dy).checked_sub(1));
                if let (Some(x), Some(y)) = (x, y) {
                    if x < gw && y < gh {
                        total += mean.get(y * gw + x).copied().unwrap_or(0.18);
                        n += 1.0;
                    }
                }
            }
        }
        if n > 0.0 {
            total / n
        } else {
            0.18
        }
    };
    let step = ((w * h) as f32 / 8_000.0).sqrt().ceil().max(1.0) as usize;
    let mut out = Samples::default();
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let i = (y * w + x) * 3;
            let Some(&[r, g, b]) = rgb.get(i..i + 3) else {
                continue;
            };
            let display = [r, g, b].map(|v| f32::from(v) / 255.0);
            out.working.push(display_srgb_to_working(display));
            out.bright.push(lightness(display) >= BRIGHT);
            let (cx, cy) = ((x * gw / w).min(gw - 1), (y * gh / h).min(gh - 1));
            out.region.push(at(cx, cy).max(1e-6).log2());
            out.skin.push(in_skin(
                faces,
                (x as f32 + 0.5) / w as f32,
                (y as f32 + 0.5) / h as f32,
            ));
        }
    }
    out
}

/// What the renderer makes of the samples with these sliders: exposure, local Shadows and
/// Highlights on each region, Whites and Blacks on each pixel, contrast, then the output
/// transform - the order `aura_render::cpu` applies them in.
#[must_use]
pub fn simulate(samples: &Samples, s: Sliders) -> Option<Reading> {
    let matrix = working_to_output(OutputColour::Srgb);
    let point = Tone {
        highlights: 0.0,
        shadows: 0.0,
        whites: f32::from(s.whites) / 100.0,
        blacks: f32::from(s.blacks) / 100.0,
    };
    let (sh, hi) = (
        f32::from(s.shadows) / 100.0,
        f32::from(s.highlights) / 100.0,
    );
    let contrast = f32::from(s.contrast) / 100.0;
    let n = samples.working.len();
    let (mut l, mut skin, mut clipped) = (Vec::with_capacity(n), Vec::new(), 0.0);
    for (((w, region), is_skin), bright) in samples
        .working
        .iter()
        .zip(&samples.region)
        .zip(&samples.skin)
        .zip(&samples.bright)
    {
        let mut v = tonemap::exposure(*w, s.exposure);
        if sh != 0.0 || hi != 0.0 {
            v = tone_pixel(v, region + s.exposure, sh, hi);
        }
        v = tonemap::tone(v, point);
        v = tonemap::contrast(v, contrast);
        let out = map_one(v, matrix, OutputColour::Srgb);
        let lv = lightness(out);
        l.push(lv);
        clipped += clip_weight(
            out[0].max(out[1]).max(out[2]),
            out[0].min(out[1]).min(out[2]),
            *bright,
        );
        if *is_skin {
            skin.push(lv);
        }
    }
    // Chroma is not solved for; the model leaves it out to stay fast.
    reading_of(l, skin, Vec::new(), clipped)
}

/// What a slider at full travel costs, in squared L*: enough that two sliders never fight
/// each other for a fraction of an L*.
const SLIDER_COST: f32 = 14.0;

/// How far a reading is from the targets, in squared L*: the skin first when there are people,
/// the middle otherwise, then the ends of the range; plus what the sliders cost and a wall
/// against clipping more than the frame already does.
#[must_use]
pub fn cost(r: &Reading, t: &Targets, s: Sliders, clip_allowed: f32) -> f32 {
    let people = t.face.is_some() && r.face.is_some();
    let weights = if people {
        [0.5, 0.8, 0.8, 1.0, 1.0]
    } else {
        [0.6, 1.0, 2.0, 1.0, 1.0]
    };
    let mut e: f32 =
        r.l.iter()
            .zip(t.l)
            .zip(weights)
            .map(|((a, b), w)| w * (a - b) * (a - b))
            .sum();
    if let (Some(face), Some(goal)) = (r.face, t.face) {
        e += SKIN_WEIGHT * (face - goal) * (face - goal);
        // Skin pushed to the ceiling reads as overexposure whatever the target said.
        if face > 80.0 {
            e += 40.0 * (face - 80.0) * (face - 80.0);
        }
    }
    if r.clipped > clip_allowed {
        // Soft: the photographer lets a window or a sky go to white for the people in front of
        // it (more than 1.2 % of the frame on one photograph in twenty), never a face (above).
        let over = (r.clipped - clip_allowed) * 1000.0;
        e += CLIP_COST * over * over;
    }
    // Every slider costs a little, so nothing moves that the light does not ask for.
    let moved = (s.exposure / 1.5).powi(2)
        + [s.contrast, s.highlights, s.shadows, s.whites, s.blacks]
            .iter()
            .map(|v| (f32::from(*v) / 100.0).powi(2))
            .sum::<f32>();
    e + SLIDER_COST * moved
}

/// How much more skin counts than one percentile of the frame: the photographer exposes for
/// the people first.
const SKIN_WEIGHT: f32 = 5.0;

/// What each 0.1 % of the frame clipped past the allowance costs, squared, in squared L*: a
/// whole percent more costs as much as skin 10 L* from its target.
const CLIP_COST: f32 = 3.0;

/// Moves of two sliders together, `(slider, sign)` pairs: exposure up with the highlights or
/// whites down keeps the top of the range while the middle rises - the move one slider at a
/// time never finds, because either half alone makes things worse.
const PAIRS: [[(usize, f32); 2]; 4] = [
    [(0, 1.0), (2, -1.0)],
    [(0, 1.0), (4, -1.0)],
    [(0, -1.0), (3, 1.0)],
    [(3, 1.0), (5, -1.0)],
];

/// Solve the free sliders (those not in `fixed`) against the targets in the model: coordinate
/// descent from the light as shot with paired moves, the steps halving each round.
#[must_use]
pub fn solve(
    samples: &Samples,
    t: &Targets,
    fixed: [bool; 6],
    clip_allowed: f32,
) -> Option<(Sliders, Reading)> {
    let mut best = Sliders::ZERO;
    let mut best_reading = simulate(samples, best)?;
    let mut best_cost = cost(&best_reading, t, best, clip_allowed);
    let free = |i: usize| !fixed.get(i).copied().unwrap_or(true);
    let mut steps: [f32; 6] = RANGE.map(|r| r.2);
    let try_move = |best: &mut Sliders,
                    best_reading: &mut Reading,
                    best_cost: &mut f32,
                    moves: &[(usize, f32)],
                    steps: &[f32; 6]|
     -> bool {
        let mut trial = *best;
        for &(i, dir) in moves {
            let (low, high, _) = RANGE.get(i).copied().unwrap_or((0.0, 0.0, 0.0));
            let step = steps.get(i).copied().unwrap_or(0.0);
            trial.set(i, (best.get(i) + dir * step).clamp(low, high));
        }
        if trial == *best {
            return false;
        }
        let Some(reading) = simulate(samples, trial) else {
            return false;
        };
        let c = cost(&reading, t, trial, clip_allowed);
        if c < *best_cost - 1e-3 {
            (*best, *best_reading, *best_cost) = (trial, reading, c);
            true
        } else {
            false
        }
    };
    for round in 0..6 {
        // Exposure first, alone: it carries most of the move and everything follows it.
        let order: &[usize] = if round == 0 {
            &[0]
        } else {
            &[0, 2, 4, 3, 5, 1]
        };
        for &i in order {
            if !free(i) {
                continue;
            }
            while try_move(
                &mut best,
                &mut best_reading,
                &mut best_cost,
                &[(i, -1.0)],
                &steps,
            ) || try_move(
                &mut best,
                &mut best_reading,
                &mut best_cost,
                &[(i, 1.0)],
                &steps,
            ) {}
        }
        if round > 0 {
            for pair in PAIRS {
                if pair.iter().all(|(i, _)| free(*i)) {
                    let back = pair.map(|(i, d)| (i, -d));
                    while try_move(&mut best, &mut best_reading, &mut best_cost, &pair, &steps)
                        || try_move(&mut best, &mut best_reading, &mut best_cost, &back, &steps)
                    {
                    }
                }
            }
            for (s, r) in steps.iter_mut().zip(RANGE) {
                *s = (*s * 0.5).max(if r.2 < 1.0 { 0.02 } else { 1.0 });
            }
        }
    }
    Some((best, best_reading))
}

/// How far a reading is from the targets, without what the sliders cost: the measure two
/// renders are compared by.
#[must_use]
pub fn distance(r: &Reading, t: &Targets, clip_allowed: f32) -> f32 {
    cost(r, t, Sliders::ZERO, clip_allowed)
}

/// An sRGB frame: bytes, width, height.
pub type Frame = (Vec<u8>, usize, usize);

/// The tone as solved and checked.
#[derive(Debug, Clone)]
pub struct Shaped {
    /// The light as shot.
    pub shot: Reading,
    pub goal: Targets,
    /// The free sliders (a fixed one is zero here: it was in the render already).
    pub sliders: Sliders,
    /// The render of those sliders and what it reads.
    pub frame: Frame,
    pub reading: Reading,
    pub clip_allowed: f32,
}

/// An earlier solve is kept while its render's distance is within this factor of what the
/// model says a new solve would reach...
const INCUMBENT_MARGIN: f32 = 1.15;
/// ...plus this, in squared L*.
const INCUMBENT_SLACK: f32 = 15.0;

/// The sliders with the fixed ones at zero: they are in the tone-neutral render already.
fn without_fixed(s: Sliders, fixed: [bool; 6]) -> Sliders {
    let mut out = s;
    for (i, f) in fixed.iter().enumerate() {
        if *f {
            out.set(i, 0.0);
        }
    }
    out
}

/// Renders the solved tone is checked on, at most: the solve, then corrections of what the
/// model of the renderer got wrong.
pub const RENDERS: usize = 3;

/// The whole tone: read the tone-neutral `frame` (every free slider at zero), predict the
/// photographer's lightness, solve in the model (keeping `incumbent`, the tone an earlier run
/// solved, while it is as good), then check on `render` (which renders the free
/// sliders given, the fixed ones as the recipe has them) and correct the model by what the
/// render shows, at most [`RENDERS`] times. The closest render wins.
pub fn shape(
    frame: &Frame,
    faces: &[[f32; 4]],
    fixed: [bool; 6],
    incumbent: Option<Sliders>,
    mut render: impl FnMut(Sliders) -> Option<Frame>,
) -> Option<Shaped> {
    let shot = read(&frame.0, frame.1, frame.2, faces)?;
    let goal = targets(&shot);
    let samples = samples(&frame.0, frame.1, frame.2, faces);
    // Clip no more than the camera already did and a little more before it starts to cost: a
    // glint, a lamp.
    let clip_allowed = (shot.clipped + 0.003).max(0.005);
    let (mut sliders, mut predicted) = solve(&samples, &goal, fixed, clip_allowed)?;
    // The tone an earlier full edit solved stays while its render is as close to the targets
    // as the model says a new solve would come: two near-equal mixes of sliders must not swap
    // places on every run. It is judged on the render, because it was corrected on one.
    let mut best: Option<(Sliders, Frame, Reading)> = None;
    if let Some(kept) = incumbent.map(|s| without_fixed(s, fixed)) {
        let new = distance(&predicted, &goal, clip_allowed);
        if let Some(rendered) = render(kept) {
            if let Some(reading) =
                read_against(&rendered.0, rendered.1, rendered.2, faces, &frame.0)
            {
                if distance(&reading, &goal, clip_allowed)
                    <= new * INCUMBENT_MARGIN + INCUMBENT_SLACK
                {
                    return Some(Shaped {
                        shot,
                        goal,
                        sliders: kept,
                        frame: rendered,
                        reading,
                        clip_allowed,
                    });
                }
                best = Some((kept, rendered, reading));
            }
        }
    }
    let mut aim = goal;
    for _ in 0..RENDERS {
        let Some(rendered) = render(sliders) else {
            break;
        };
        let Some(reading) = read_against(&rendered.0, rendered.1, rendered.2, faces, &frame.0)
        else {
            break;
        };
        let better = best.as_ref().is_none_or(|(_, _, r)| {
            distance(&reading, &goal, clip_allowed) < distance(r, &goal, clip_allowed)
        });
        let close = miss(&reading, &goal).abs() < 1.5;
        if better {
            best = Some((sliders, rendered, reading));
        }
        if close {
            break;
        }
        // The render is the truth: move the aim by what the model got wrong, solve again.
        aim = corrected(&aim, &predicted, &reading);
        let Some((next, next_predicted)) = solve(&samples, &aim, fixed, clip_allowed) else {
            break;
        };
        if next == sliders {
            break;
        }
        (sliders, predicted) = (next, next_predicted);
    }
    let (sliders, frame, reading) = best?;
    Some(Shaped {
        shot,
        goal,
        sliders,
        frame,
        reading,
        clip_allowed,
    })
}

/// The photographer's median chroma for this light, from the same features as the tone: 4.5
/// from the photographer's on photographs the fit did not see (one fixed value: 8.7; the render
/// as shot: 5.8, and the photographer's colour is 1.27 times AURA's as shot at the median).
const CHROMA_MODEL: [f32; 9] = [
    14.96, -11.52, 12.78, -13.68, 5.87, -10.39, 33.81, -2.91, -2.87,
];

/// The chroma the photographer's photograph has, blended by intent like the tone. Nothing is
/// coloured that has no colour: a black-and-white or one-colour frame keeps what it has.
#[must_use]
pub fn chroma_target(shot: &Reading, now: f32) -> Option<f32> {
    if shot.chroma < 4.0 {
        return None;
    }
    let x = features(shot);
    let wanted: f32 = CHROMA_MODEL.iter().zip(x).map(|(w, v)| w * v).sum();
    let share = Intent::of(shot).share();
    Some((now + share * (wanted - now)).clamp(now * 0.85, now * 1.6))
}

/// Median chroma of the frame and of its skin, and the share of it pushed to the edge of the
/// colour space, after a further `vibrance` and `saturation` (`-1 ..= 1`) on the samples.
#[must_use]
pub fn simulate_colour(
    samples: &Samples,
    vibrance: f32,
    saturation: f32,
) -> Option<(f32, Option<f32>, f32)> {
    let matrix = working_to_output(OutputColour::Srgb);
    let (mut all, mut skin, mut clipped) = (Vec::new(), Vec::new(), 0_usize);
    for (w, is_skin) in samples.working.iter().zip(&samples.skin) {
        let v = tonemap::vibrance_saturation(*w, vibrance, saturation);
        let out = map_one(v, matrix, OutputColour::Srgb);
        let [_, a, b] = lab(out);
        let c = a.hypot(b);
        all.push(c);
        if *is_skin {
            skin.push(c);
        }
        let (max, min) = (
            out[0].max(out[1]).max(out[2]),
            out[0].min(out[1]).min(out[2]),
        );
        clipped += usize::from(max >= 0.995 && max - min > 0.2);
    }
    if all.len() < 100 {
        return None;
    }
    let n = all.len();
    all.sort_by(f32::total_cmp);
    skin.sort_by(f32::total_cmp);
    Some((
        percentile_of(&all, 0.5),
        (skin.len() >= 30).then(|| percentile_of(&skin, 0.5)),
        clipped as f32 / n as f32,
    ))
}

/// Skin is never made more colourful than this (CIE chroma): the photographer's skin sits at 26
/// on average, and beyond the mid thirties a face reads as sunburnt or orange.
pub const SKIN_CHROMA_CEILING: f32 = 34.0;

/// The further vibrance and saturation (slider units) that bring the frame's chroma to
/// `target`: vibrance first, which spares what is already colourful, then saturation only if
/// vibrance alone is not enough; never past the skin ceiling or into new colour clipping.
#[must_use]
pub fn solve_colour(samples: &Samples, target: f32, vibrance_room: i16) -> Option<(i16, i16, f32)> {
    let (_, skin0, clip0) = simulate_colour(samples, 0.0, 0.0)?;
    let skin_cap = skin0.map(|s| s.max(SKIN_CHROMA_CEILING));
    let allowed = clip0 + 0.004;
    let cost = |c: f32, skin: Option<f32>, clip: f32, v: i16, s: i16| {
        let mut e = (c - target) * (c - target);
        if let (Some(k), Some(cap)) = (skin, skin_cap) {
            if k > cap {
                e += 20.0 * (k - cap) * (k - cap);
            }
        }
        if clip > allowed {
            let over = (clip - allowed) * 1000.0;
            e += 10.0 * over * over;
        }
        e + 0.002 * f32::from(v) * f32::from(v) + 0.01 * f32::from(s) * f32::from(s)
    };
    let mut best = (0_i16, 0_i16, f32::MAX, 0.0_f32);
    for v in (-20..=vibrance_room.max(0)).step_by(5) {
        for s in (-15_i16..=15).step_by(5) {
            let Some((c, skin, clip)) =
                simulate_colour(samples, f32::from(v) / 100.0, f32::from(s) / 100.0)
            else {
                continue;
            };
            let e = cost(c, skin, clip, v, s);
            if e < best.2 {
                best = (v, s, e, c);
            }
        }
    }
    (best.2 < f32::MAX).then_some((best.0, best.1, best.3))
}

/// Move the targets by what the render showed the model got wrong, so the next solve lands
/// where the render will be.
#[must_use]
pub fn corrected(t: &Targets, predicted: &Reading, rendered: &Reading) -> Targets {
    let mut out = *t;
    for ((slot, p), r) in out.l.iter_mut().zip(predicted.l).zip(rendered.l) {
        *slot -= r - p;
    }
    if let (Some(goal), Some(p), Some(r)) = (out.face.as_mut(), predicted.face, rendered.face) {
        *goal -= r - p;
    }
    out
}

/// How far the render is from the targets, as one number of L*: the skin when there are
/// people, the middle otherwise.
#[must_use]
pub fn miss(r: &Reading, t: &Targets) -> f32 {
    match (r.face, t.face) {
        (Some(face), Some(goal)) => face - goal,
        _ => r.l.get(2).copied().unwrap_or(0.0) - t.l.get(2).copied().unwrap_or(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&f(x, y));
            }
        }
        v
    }

    #[test]
    fn lab_reads_grey_white_and_black() {
        assert!((lab([0.5; 3])[0] - 53.39).abs() < 0.1);
        assert!((lab([1.0; 3])[0] - 100.0).abs() < 0.1);
        assert!(lab([0.0; 3])[0].abs() < 0.1);
        assert!(lab([0.5; 3])[1].abs() < 0.1 && lab([0.5; 3])[2].abs() < 0.1);
    }

    #[test]
    fn a_dark_portrait_is_brightened_towards_the_photographers_skin() {
        // A dim room: skin at L* 38, the frame mostly darker.
        let (w, h) = (240, 160);
        let faces = [[0.4, 0.2, 0.6, 0.6]];
        let rgb = frame(w, h, |x, y| {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            if in_skin(&faces, fx, fy) {
                [118, 84, 66]
            } else {
                let v = (30.0 + 120.0 * fx) as u8;
                [v, v, v.saturating_sub(4)]
            }
        });
        let shot = read(&rgb, w, h, &faces).unwrap();
        assert!(shot.face.unwrap() < 42.0);
        let t = targets(&shot);
        assert_eq!(t.intent, Intent::People);
        assert!(t.face.unwrap() > shot.face.unwrap() + 8.0, "{t:?} {shot:?}");
        let s = samples(&rgb, w, h, &faces);
        let (sliders, predicted) = solve(&s, &t, [false; 6], 0.01).unwrap();
        assert!(sliders.exposure > 0.3, "{sliders:?}");
        assert!(
            (predicted.face.unwrap() - t.face.unwrap()).abs() < 4.0,
            "{predicted:?} {t:?}"
        );
        assert!(predicted.clipped <= 0.011);
    }

    #[test]
    fn a_night_scene_without_people_is_moved_least() {
        let (w, h) = (240, 160);
        let rgb = frame(w, h, |x, y| {
            if (x / 20 + y / 20) % 7 == 0 {
                [250, 230, 180]
            } else {
                [18, 20, 28]
            }
        });
        let shot = read(&rgb, w, h, &[]).unwrap();
        let t = targets(&shot);
        assert_eq!(t.intent, Intent::Night);
        assert!(t.l[2] - shot.l[2] < 20.0, "{t:?} {shot:?}");
    }

    #[test]
    fn a_high_key_frame_is_not_darkened() {
        let (w, h) = (200, 140);
        let rgb = frame(w, h, |x, _| {
            let v = (200 + (x % 50)) as u8;
            [v, v, v]
        });
        let shot = read(&rgb, w, h, &[]).unwrap();
        let t = targets(&shot);
        assert_eq!(t.intent, Intent::HighKey);
        for (a, b) in t.l.iter().zip(shot.l).take(4) {
            assert!(*a >= b - 2.01, "{t:?} {shot:?}");
        }
    }

    #[test]
    fn a_fixed_slider_is_never_moved() {
        let (w, h) = (160, 120);
        let rgb = frame(w, h, |x, _| {
            let v = (20 + x / 2) as u8;
            [v, v, v]
        });
        let shot = read(&rgb, w, h, &[]).unwrap();
        let t = targets(&shot);
        let s = samples(&rgb, w, h, &[]);
        let (sliders, _) = solve(&s, &t, [true, false, false, false, false, false], 0.01).unwrap();
        assert!(sliders.exposure.abs() < 1e-6);
    }

    #[test]
    fn the_model_matches_the_renderer_on_exposure() {
        // One stop on a mid grey: the simulation agrees with the renderer's own transform.
        let (w, h) = (100, 100);
        let rgb = frame(w, h, |_, _| [100, 100, 100]);
        let s = samples(&rgb, w, h, &[]);
        let zero = simulate(&s, Sliders::ZERO).unwrap();
        assert!(
            (zero.l[2] - lab([100.0 / 255.0; 3])[0]).abs() < 1.0,
            "{zero:?}"
        );
        let up = simulate(
            &s,
            Sliders {
                exposure: 1.0,
                ..Sliders::ZERO
            },
        )
        .unwrap();
        assert!(up.l[2] > zero.l[2] + 10.0);
    }

    #[test]
    fn a_dull_frame_gains_colour_and_skin_stays_below_its_ceiling() {
        let (w, h) = (200, 140);
        let faces = [[0.4, 0.2, 0.6, 0.7]];
        let rgb = frame(w, h, |x, y| {
            if in_skin(&faces, x as f32 / w as f32, y as f32 / h as f32) {
                [170, 130, 110]
            } else if x < 100 {
                [150, 95, 90]
            } else {
                [90, 110, 140]
            }
        });
        let s = samples(&rgb, w, h, &faces);
        let (c0, _, _) = simulate_colour(&s, 0.0, 0.0).unwrap();
        let (v, sat, c) = solve_colour(&s, c0 * 1.25, 40).unwrap();
        assert!(v > 0 || sat > 0, "{v} {sat}");
        assert!(c > c0 * 1.1, "{c} {c0}");
        let (_, skin, _) =
            simulate_colour(&s, f32::from(v) / 100.0, f32::from(sat) / 100.0).unwrap();
        assert!(skin.unwrap() <= SKIN_CHROMA_CEILING + 1.0);
    }

    #[test]
    fn black_and_white_is_not_coloured() {
        let shot = Reading {
            l: [5.0, 20.0, 50.0, 80.0, 90.0],
            face: None,
            chroma: 1.0,
            clipped: 0.0,
        };
        assert!(chroma_target(&shot, 1.0).is_none());
    }

    #[test]
    fn a_correction_moves_the_targets_by_the_models_miss() {
        let t = Targets {
            l: [5.0, 20.0, 50.0, 80.0, 90.0],
            face: Some(60.0),
            intent: Intent::People,
        };
        let p = Reading {
            l: [5.0, 20.0, 50.0, 80.0, 90.0],
            face: Some(60.0),
            chroma: 10.0,
            clipped: 0.0,
        };
        let r = Reading {
            face: Some(63.0),
            ..p
        };
        let c = corrected(&t, &p, &r);
        assert!((c.face.unwrap() - 57.0).abs() < 1e-4);
        assert!((miss(&r, &t) - 3.0).abs() < 1e-4);
    }
}
