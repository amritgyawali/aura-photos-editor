//! Bundled, offline person segmentation: face skin, body skin, hair and clothes. ADR-0082.
//!
//! The model is Google MediaPipe's *Selfie Multiclass* segmenter (Apache-2.0), converted to
//! ONNX by `ml/models/skin/convert_selfie_multiclass.py` and run on aura-infer. It labels each
//! pixel background, hair, body skin, face skin, clothes or other (accessories, glasses).
//!
//! It replaces a guess. Before this module, "the skin of this face" was a geometric oval from
//! five landmarks gated by colour similarity to one sampled patch, and "body skin" was a flood
//! fill of similar colour from the neck: both selected skin-coloured walls, missed shadowed skin
//! and stopped at a necklace. A learned person segmenter knows what a shoulder is.
//!
//! What this module adds on top of the network, all measured on the photograph itself:
//!
//! * **Person crops.** The network sees 256 x 256 pixels. A guest in a group photo is a few of
//!   them, so every detected face also gets a pass over a crop around its head and torso, and
//!   that pass is blended into the full-frame answer where it is more detailed.
//! * **The same person's own colour.** Each face's skin colour is measured from the pixels the
//!   network is most sure are its face skin, and the face matte is attenuated where a pixel is
//!   far darker (beard, brows, nostrils, pupils) or a different colour (lips, make-up, eyes)
//!   than *that person's* skin - never compared with an ideal tone.
//! * **Edges from the photograph.** Probabilities are refined with a guided filter whose guide
//!   is the image's own luminance, so a matte follows the jaw line and the hairline rather than
//!   the network's 256-pixel grid.
//! * **People, not classes.** Every face gets its own face matte, and connected regions of body
//!   skin, hair and clothes are assigned to the nearest face, so each person's strengths are
//!   measured from their own skin.
// Every index below is produced from a validated width x height grid; out-of-range coordinates
// are clamped before indexing.
#![allow(
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::needless_range_loop,
    clippy::doc_markdown
)]

use std::sync::{Mutex, OnceLock};

use aura_core::AuraResult;
use aura_infer::{
    contract::infer::{Precision, TensorView},
    onnx::Executable,
};
use serde::{Deserialize, Serialize};

use crate::portrait::PortraitFace;

#[path = "skin_orientation.rs"]
mod orientation;

pub const VERSION: &str = "mediapipe-selfie-multiclass-256-aura-v3";
pub const MODEL_HASH: &str = "10eee962bb85d9f5d0b292376f595ce70d810becfcef17feeb4762e62d8a7754";
const SIDE: usize = 256;
const CLASSES: usize = 6;
/// Longest edge of the working grid every matte is measured on.
pub const WORKING_EDGE: usize = 1024;
/// Longest edge of a stored matte, in cells. The renderer refines edges at full resolution.
pub const MATTE_EDGE: usize = 256;
static MODEL: &[u8] =
    include_bytes!("../../../assets/models/selfie_multiclass/selfie_multiclass_256x256.onnx");

/// The six labels, in the network's output order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Background = 0,
    Hair = 1,
    BodySkin = 2,
    FaceSkin = 3,
    Clothes = 4,
    Other = 5,
}

impl Class {
    pub const ALL: [Self; CLASSES] = [
        Self::Background,
        Self::Hair,
        Self::BodySkin,
        Self::FaceSkin,
        Self::Clothes,
        Self::Other,
    ];
}

/// What the caller wants measured, and how strictly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// 0 keeps every pixel the network calls skin; 1 also removes pixels far from the same
    /// person's own skin colour or brightness (beard, brows, lips, make-up).
    pub precision: f32,
    /// 0 hard edges, 1 very soft edges.
    pub softness: f32,
    /// At most this many person crops are segmented in addition to the whole frame.
    pub max_crops: usize,
    /// Keep pixels much darker than the person's skin (beard, stubble, brows) out of the face
    /// matte even at low precision.
    pub protect_dark_hair: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            precision: 0.5,
            softness: 0.35,
            max_crops: 6,
            protect_dark_hair: true,
        }
    }
}

/// A soft selection over part of the frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Matte {
    /// Normalized left, top, right, bottom of the grid in the oriented original.
    pub bounds: [f32; 4],
    pub width: usize,
    pub height: usize,
    /// Row-major coverage, 0 (not selected) to 255 (fully selected).
    pub alpha: Vec<u8>,
}

impl Matte {
    /// Mean coverage over the grid, 0..1.
    #[must_use]
    pub fn mean(&self) -> f32 {
        if self.alpha.is_empty() {
            return 0.0;
        }
        self.alpha.iter().map(|v| f32::from(*v)).sum::<f32>() / (self.alpha.len() as f32 * 255.0)
    }

    /// Selected area as a fraction of the whole frame.
    #[must_use]
    pub fn area(&self) -> f32 {
        let [l, t, r, b] = self.bounds;
        self.mean() * (r - l).max(0.0) * (b - t).max(0.0)
    }

    /// Coverage at a normalized frame position (nearest cell), 0..1.
    #[must_use]
    pub fn at(&self, x: f32, y: f32) -> f32 {
        let [l, t, r, b] = self.bounds;
        if x < l || y < t || x >= r || y >= b || self.width == 0 || self.height == 0 {
            return 0.0;
        }
        let gx = (((x - l) / (r - l)) * self.width as f32) as usize;
        let gy = (((y - t) / (b - t)) * self.height as f32) as usize;
        self.alpha
            .get(gy.min(self.height - 1) * self.width + gx.min(self.width - 1))
            .map_or(0.0, |v| f32::from(*v) / 255.0)
    }
}

/// A person's measured skin colour, in display-encoded values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinColour {
    /// Median encoded RGB of the most confident skin pixels.
    pub rgb: [f32; 3],
    /// Number of pixels the median was measured from.
    pub samples: usize,
    /// A representative pixel position, normalized: the confident skin pixel closest to the
    /// median colour. Usable as a renderer sample point.
    pub point: [f32; 2],
}

/// Everything segmented for one detected face.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub face: Option<Matte>,
    pub body: Option<Matte>,
    pub hair: Option<Matte>,
    pub clothes: Option<Matte>,
    pub face_colour: Option<SkinColour>,
    pub body_colour: Option<SkinColour>,
}

/// The result of one analysis.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    /// One entry per input face, in the same order.
    pub people: Vec<Person>,
    /// The frame's background, for backdrop finishing.
    pub background: Option<Matte>,
    /// Network passes run: one for the whole frame plus one per person crop.
    pub passes: usize,
    /// Fraction of the frame per class (argmax), in [`Class::ALL`] order.
    pub coverage: [f32; CLASSES],
}

fn invalid(message: impl Into<String>) -> aura_core::AuraError {
    let mut error = aura_core::errors::ml::parse_failed(message);
    error.user_message =
        "Skin detection could not finish. Try again or use the manual retouch tools.".into();
    error
}

fn graph() -> AuraResult<&'static Mutex<Executable>> {
    static GRAPH: OnceLock<Result<Mutex<Executable>, String>> = OnceLock::new();
    GRAPH
        .get_or_init(|| {
            if blake3::hash(MODEL).to_hex().as_str() != MODEL_HASH {
                return Err("Skin segmentation model checksum mismatch".into());
            }
            let model = aura_infer::onnx::parse(MODEL).map_err(|e| e.to_string())?;
            Executable::compile(&model, Precision::Fp32)
                .map(Mutex::new)
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|message| invalid(message.clone()))
}

/// Packed sRGB working pixels.
struct Image {
    w: usize,
    h: usize,
    rgb: Vec<u8>,
}

impl Image {
    fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y.min(self.h - 1) * self.w + x.min(self.w - 1)) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    /// Encoded luminance 0..1.
    fn luma(&self) -> Vec<f32> {
        self.rgb
            .chunks_exact(3)
            .map(|p| {
                (f32::from(p[0]) * 0.2126 + f32::from(p[1]) * 0.7152 + f32::from(p[2]) * 0.0722)
                    / 255.0
            })
            .collect()
    }
}

/// Box-downscale to at most `edge` on the long side; small images are copied.
fn working(rgb: &[u8], w: usize, h: usize, edge: usize) -> Image {
    let factor = w.max(h).div_ceil(edge).max(1);
    if factor == 1 {
        return Image {
            w,
            h,
            rgb: rgb.to_vec(),
        };
    }
    // Cover the complete source extent, including the final partial block. Integer
    // division used to crop the right/bottom edges and produce zero-height panoramas.
    let (ow, oh) = (w.div_ceil(factor), h.div_ceil(factor));
    let mut out = Vec::with_capacity(ow * oh * 3);
    for y in 0..oh {
        let (y0, y1) = (y * h / oh, (y + 1) * h / oh);
        for x in 0..ow {
            let (x0, x1) = (x * w / ow, (x + 1) * w / ow);
            let mut sum = [0_u64; 3];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = (yy * w + xx) * 3;
                    for c in 0..3 {
                        sum[c] += u64::from(rgb[i + c]);
                    }
                }
            }
            let n = ((x1 - x0) * (y1 - y0)) as u64;
            for s in sum {
                out.push(((s + n / 2) / n) as u8);
            }
        }
    }
    Image {
        w: ow,
        h: oh,
        rgb: out,
    }
}

/// Run the network on a square region of the working image. `region` is in working pixels
/// (left, top, side) and may extend past the frame, where the input is black. Returns the six
/// class probabilities per model pixel, class-major.
fn infer_region(image: &Image, left: f32, top: f32, side: f32) -> AuraResult<Vec<f32>> {
    let mut input = vec![-1.0_f32; 3 * SIDE * SIDE];
    let step = side / SIDE as f32;
    // Average a small grid of samples per model pixel: an area filter, so a large region is
    // not aliased into the 256-pixel input.
    let taps = (step.ceil() as usize).clamp(1, 4);
    for my in 0..SIDE {
        for mx in 0..SIDE {
            let mut sum = [0.0_f32; 3];
            let mut n = 0.0_f32;
            for ty in 0..taps {
                for tx in 0..taps {
                    let fx = left + (mx as f32 + (tx as f32 + 0.5) / taps as f32) * step;
                    let fy = top + (my as f32 + (ty as f32 + 0.5) / taps as f32) * step;
                    if fx < 0.0 || fy < 0.0 || fx >= image.w as f32 || fy >= image.h as f32 {
                        continue;
                    }
                    let p = image.pixel(fx as usize, fy as usize);
                    for c in 0..3 {
                        sum[c] += f32::from(p[c]);
                    }
                    n += 1.0;
                }
            }
            let total = (taps * taps) as f32;
            for c in 0..3 {
                // Outside the frame counts as black (-1), matching the letterbox.
                let mean = (sum[c] + (total - n) * 0.0) / total;
                input[c * SIDE * SIDE + my * SIDE + mx] = mean / 127.5 - 1.0;
            }
        }
    }
    let graph = graph()?
        .lock()
        .map_err(|_| invalid("Skin segmentation lock failed"))?;
    let outputs = graph.run(&[TensorView::new(vec![1, 3, SIDE, SIDE], &input)?])?;
    let logits = outputs
        .first()
        .ok_or_else(|| invalid("Missing skin segmentation output"))?;
    if logits.data.len() != CLASSES * SIDE * SIDE {
        return Err(invalid("Unexpected skin segmentation output shape"));
    }
    let mut probs = logits.data.clone();
    for i in 0..SIDE * SIDE {
        let max = (0..CLASSES).fold(f32::NEG_INFINITY, |m, k| m.max(probs[k * SIDE * SIDE + i]));
        let mut total = 0.0;
        for k in 0..CLASSES {
            let e = (probs[k * SIDE * SIDE + i] - max).exp();
            probs[k * SIDE * SIDE + i] = e;
            total += e;
        }
        for k in 0..CLASSES {
            probs[k * SIDE * SIDE + i] /= total.max(1e-12);
        }
    }
    Ok(probs)
}

/// Six probability planes over the working grid.
struct Field {
    planes: Vec<Vec<f32>>,
}

impl Field {
    fn plane(&self, class: Class) -> &[f32] {
        &self.planes[class as usize]
    }
}

/// Bilinear sample of one model-space class plane at model coordinates.
fn sample(probs: &[f32], class: usize, mx: f32, my: f32) -> f32 {
    let fx = mx.clamp(0.0, (SIDE - 1) as f32);
    let fy = my.clamp(0.0, (SIDE - 1) as f32);
    let (x0, y0) = (fx as usize, fy as usize);
    let (x1, y1) = ((x0 + 1).min(SIDE - 1), (y0 + 1).min(SIDE - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let base = class * SIDE * SIDE;
    let at = |x: usize, y: usize| probs[base + y * SIDE + x];
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// The whole frame, then a crop around each of the largest faces blended in where it adds
/// detail. Faces are normalized; crops are skipped for people who already fill the frame.
fn segment(image: &Image, faces: &[PortraitFace], max_crops: usize) -> AuraResult<(Field, usize)> {
    let (w, h) = (image.w, image.h);
    let side = w.max(h) as f32;
    let left = (w as f32 - side) * 0.5;
    let top = (h as f32 - side) * 0.5;
    let probs = infer_region(image, left, top, side)?;
    let scale = SIDE as f32 / side;
    let mut planes = vec![vec![0.0_f32; w * h]; CLASSES];
    for y in 0..h {
        for x in 0..w {
            let mx = (x as f32 + 0.5 - left) * scale - 0.5;
            let my = (y as f32 + 0.5 - top) * scale - 0.5;
            for (k, plane) in planes.iter_mut().enumerate() {
                plane[y * w + x] = sample(&probs, k, mx, my);
            }
        }
    }
    let mut passes = 1;
    // Largest faces first: they matter most, and a crowd should not cost a crop per guest.
    let mut order: Vec<usize> = (0..faces.len()).collect();
    let height_of = |f: &PortraitFace| f.bounds[3] - f.bounds[1];
    order.sort_by(|a, b| height_of(&faces[*b]).total_cmp(&height_of(&faces[*a])));
    for &index in order.iter().take(max_crops) {
        let [l, t, r, b] = faces[index].bounds;
        let (fw, fh) = ((r - l) * w as f32, (b - t) * h as f32);
        // Head and torso: four and a half face heights square, the head in the upper third.
        let crop = (fh.max(fw) * 4.6).max(48.0);
        // A face already 40 model pixels tall in the whole-frame pass gains little from a crop.
        if crop > side * 0.8 || fh.max(fw) / side * SIDE as f32 >= 40.0 {
            continue;
        }
        let cx = (l + r) * 0.5 * w as f32;
        let crop_left = cx - crop * 0.5;
        let crop_top = t * h as f32 - fh * 0.6;
        let probs = infer_region(image, crop_left, crop_top, crop)?;
        passes += 1;
        let scale = SIDE as f32 / crop;
        let x0 = crop_left.max(0.0) as usize;
        let y0 = crop_top.max(0.0) as usize;
        let x1 = ((crop_left + crop).ceil().max(0.0) as usize).min(w);
        let y1 = ((crop_top + crop).ceil().max(0.0) as usize).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let mx = (x as f32 + 0.5 - crop_left) * scale - 0.5;
                let my = (y as f32 + 0.5 - crop_top) * scale - 0.5;
                // Full weight in the middle of the crop, fading over its outer eighth, so the
                // seam between the two answers is invisible.
                let edge = mx
                    .min(my)
                    .min(SIDE as f32 - 1.0 - mx)
                    .min(SIDE as f32 - 1.0 - my);
                let weight = (edge / (SIDE as f32 / 8.0)).clamp(0.0, 1.0);
                if weight <= 0.0 {
                    continue;
                }
                for (k, plane) in planes.iter_mut().enumerate() {
                    let v = sample(&probs, k, mx, my);
                    let slot = &mut plane[y * w + x];
                    *slot = *slot * (1.0 - weight) + v * weight;
                }
            }
        }
    }
    Ok((Field { planes }, passes))
}

/// Mean over a (2r+1)^2 window clipped at the borders, via an integral image.
fn box_mean(values: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let stride = w + 1;
    let mut sum = vec![0.0_f64; stride * (h + 1)];
    for y in 0..h {
        let mut row = 0.0_f64;
        for x in 0..w {
            row += f64::from(values[y * w + x]);
            sum[(y + 1) * stride + x + 1] = sum[y * stride + x + 1] + row;
        }
    }
    let mut out = vec![0.0_f32; w * h];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
            let total = sum[y1 * stride + x1] - sum[y0 * stride + x1] - sum[y1 * stride + x0]
                + sum[y0 * stride + x0];
            out[y * w + x] = (total / ((y1 - y0) * (x1 - x0)) as f64) as f32;
        }
    }
    out
}

/// Guided filter (He, Sun and Tang): `src` smoothed so its edges follow `guide`'s.
fn guided(guide: &[f32], src: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Vec<f32> {
    let mean_i = box_mean(guide, w, h, r);
    let mean_p = box_mean(src, w, h, r);
    let ip: Vec<f32> = guide.iter().zip(src).map(|(a, b)| a * b).collect();
    let ii: Vec<f32> = guide.iter().map(|a| a * a).collect();
    let corr_ip = box_mean(&ip, w, h, r);
    let corr_ii = box_mean(&ii, w, h, r);
    let mut a = vec![0.0_f32; w * h];
    let mut b = vec![0.0_f32; w * h];
    for i in 0..w * h {
        let var = (corr_ii[i] - mean_i[i] * mean_i[i]).max(0.0);
        let cov = corr_ip[i] - mean_i[i] * mean_p[i];
        a[i] = cov / (var + eps);
        b[i] = mean_p[i] - a[i] * mean_i[i];
    }
    let mean_a = box_mean(&a, w, h, r);
    let mean_b = box_mean(&b, w, h, r);
    (0..w * h)
        .map(|i| (mean_a[i] * guide[i] + mean_b[i]).clamp(0.0, 1.0))
        .collect()
}

/// 8-connected components of `mask`; returns a label per pixel (0 = background) and a count.
fn components(mask: &[bool], w: usize, h: usize) -> (Vec<u32>, usize) {
    let mut labels = vec![0_u32; w * h];
    let mut next = 0_u32;
    let mut stack = Vec::new();
    for start in 0..w * h {
        if !mask[start] || labels[start] != 0 {
            continue;
        }
        next += 1;
        labels[start] = next;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            for dy in [-1_i64, 0, 1] {
                for dx in [-1_i64, 0, 1] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let n = ny as usize * w + nx as usize;
                    if mask[n] && labels[n] == 0 {
                        labels[n] = next;
                        stack.push(n);
                    }
                }
            }
        }
    }
    (labels, next as usize)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn median(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mid = values.len() / 2;
    values.select_nth_unstable_by(mid, f32::total_cmp);
    values[mid]
}

/// A robust colour model of one person's skin: median and spread of chromaticity and
/// brightness over the pixels the network is most sure about.
#[derive(Clone, Copy)]
struct ColourModel {
    rg: [f32; 2],
    spread: [f32; 2],
    luma: f32,
    rgb: [f32; 3],
    samples: usize,
    point: [f32; 2],
}

fn chromaticity(p: [u8; 3]) -> ([f32; 2], f32) {
    let [r, g, b] = p.map(f32::from);
    let total = (r + g + b).max(1.0);
    (
        [r / total, g / total],
        (r * 0.2126 + g * 0.7152 + b * 0.0722) / 255.0,
    )
}

fn colour_model(image: &Image, confident: &[usize]) -> Option<ColourModel> {
    if confident.len() < 30 {
        return None;
    }
    let stride = (confident.len() / 4000).max(1);
    let picked: Vec<usize> = confident.iter().step_by(stride).copied().collect();
    let values: Vec<([f32; 2], f32, [u8; 3])> = picked
        .iter()
        .map(|&i| {
            let p = image.pixel(i % image.w, i / image.w);
            let (rg, l) = chromaticity(p);
            (rg, l, p)
        })
        // Clipped highlights and black say nothing about skin colour.
        .filter(|(_, l, p)| *l > 0.04 && p.iter().all(|v| *v < 250))
        .collect();
    if values.len() < 20 {
        return None;
    }
    let rg = [
        median(values.iter().map(|v| v.0[0]).collect()),
        median(values.iter().map(|v| v.0[1]).collect()),
    ];
    let spread = [
        (median(values.iter().map(|v| (v.0[0] - rg[0]).abs()).collect()) * 1.4826).max(0.008),
        (median(values.iter().map(|v| (v.0[1] - rg[1]).abs()).collect()) * 1.4826).max(0.006),
    ];
    let luma = median(values.iter().map(|v| v.1).collect()).max(0.02);
    let rgb = [0, 1, 2].map(|c| median(values.iter().map(|v| f32::from(v.2[c]) / 255.0).collect()));
    // The confident pixel closest to the median colour, as a representative sample point.
    let best = picked.iter().copied().min_by(|&a, &b| {
        let score = |i: usize| {
            let (c, l) = chromaticity(image.pixel(i % image.w, i / image.w));
            ((c[0] - rg[0]) / spread[0]).abs()
                + ((c[1] - rg[1]) / spread[1]).abs()
                + ((l - luma) / (luma * 0.1)).abs()
        };
        score(a).total_cmp(&score(b))
    })?;
    Some(ColourModel {
        rg,
        spread,
        luma,
        rgb,
        samples: values.len(),
        point: [
            ((best % image.w) as f32 + 0.5) / image.w as f32,
            ((best / image.w) as f32 + 0.5) / image.h as f32,
        ],
    })
}

impl ColourModel {
    /// 1 for this person's skin, falling to 0 for clearly different colour or much darker
    /// pixels. `strictness` 0..1.
    fn likeness(&self, p: [u8; 3], strictness: f32, dark: f32) -> f32 {
        let (c, l) = chromaticity(p);
        let z = (((c[0] - self.rg[0]) / self.spread[0]).powi(2)
            + ((c[1] - self.rg[1]) / self.spread[1]).powi(2))
        .sqrt();
        let near = 3.0 + 3.0 * (1.0 - strictness);
        let colour = 1.0 - smoothstep(near, near + 3.0, z);
        let ratio = l / self.luma;
        let floor = 0.22 + 0.26 * dark;
        let light = smoothstep(floor, floor + 0.15, ratio);
        colour * light
    }

    fn report(&self) -> SkinColour {
        SkinColour {
            rgb: self.rgb,
            samples: self.samples,
            point: self.point,
        }
    }
}

/// Crop a working-grid plane to its support and store it as a matte of at most
/// [`MATTE_EDGE`] cells on the long side.
fn to_matte(plane: &[f32], w: usize, h: usize, floor: f32) -> Option<Matte> {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if plane[y * w + x] > floor {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    // A margin of two pixels keeps the soft edge inside the grid.
    let (x0, y0) = (x0.saturating_sub(2), y0.saturating_sub(2));
    let (x1, y1) = ((x1 + 2).min(w), (y1 + 2).min(h));
    let (bw, bh) = (x1 - x0, y1 - y0);
    let factor = bw.max(bh).div_ceil(MATTE_EDGE).max(1);
    let (mw, mh) = (bw.div_ceil(factor), bh.div_ceil(factor));
    let mut alpha = Vec::with_capacity(mw * mh);
    for gy in 0..mh {
        for gx in 0..mw {
            let mut sum = 0.0;
            let mut n = 0.0;
            for y in y0 + gy * factor..(y0 + (gy + 1) * factor).min(y1) {
                for x in x0 + gx * factor..(x0 + (gx + 1) * factor).min(x1) {
                    sum += plane[y * w + x];
                    n += 1.0;
                }
            }
            let v = if n > 0.0 { sum / n } else { 0.0 };
            alpha.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    // The grid covers whole cells, so its right/bottom edge may pass the cropped bounds.
    let bounds = [
        x0 as f32 / w as f32,
        y0 as f32 / h as f32,
        ((x0 + mw * factor) as f32 / w as f32).min(1.0),
        ((y0 + mh * factor) as f32 / h as f32).min(1.0),
    ];
    Some(Matte {
        bounds,
        width: mw,
        height: mh,
        alpha,
    })
}

// A component can contain touching people. Keep detached hands with the nearest
// person, but partition a shared component between all faces it reaches. Choosing
// one owner for the entire component retouched somebody else's arms in group photos.
fn assign_regions(
    plane: &[f32],
    w: usize,
    h: usize,
    boxes: &[[f32; 4]],
    reach: f32,
) -> Vec<Vec<bool>> {
    let mask: Vec<bool> = plane.iter().map(|p| *p > 0.5).collect();
    let (labels, count) = components(&mask, w, h);
    let distance = |i: usize, k: usize| {
        let [l, t, r, b] = boxes[k];
        let x = (i % w) as f32 + 0.5;
        let y = (i / w) as f32 + 0.5;
        (x - (l + r) * 0.5).hypot(y - (t + b) * 0.5) / (r - l).max(b - t).max(1.0)
    };
    let mut nearest = vec![vec![f32::INFINITY; boxes.len()]; count + 1];
    for (i, &label) in labels.iter().enumerate().filter(|(_, label)| **label != 0) {
        for k in 0..boxes.len() {
            nearest[label as usize][k] = nearest[label as usize][k].min(distance(i, k));
        }
    }
    let candidates: Vec<Vec<usize>> = nearest
        .iter()
        .map(|distances| {
            let nearby: Vec<usize> = distances
                .iter()
                .enumerate()
                .filter(|(_, d)| **d <= 1.5_f32.min(reach))
                .map(|(k, _)| k)
                .collect();
            if !nearby.is_empty() {
                return nearby;
            }
            distances
                .iter()
                .enumerate()
                .filter(|(_, d)| **d <= reach)
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(k, _)| vec![k])
                .unwrap_or_default()
        })
        .collect();
    let mut out = vec![vec![false; w * h]; boxes.len()];
    for (i, &label) in labels.iter().enumerate().filter(|(_, label)| **label != 0) {
        if let Some(&k) = candidates[label as usize]
            .iter()
            .min_by(|&&a, &&b| distance(i, a).total_cmp(&distance(i, b)))
        {
            out[k][i] = true;
        }
    }
    out
}

/// Segment the people in a photograph and return a face, body, hair and clothes matte for
/// each detected face (in input order), plus the background.
///
/// `rgb` is packed, oriented sRGB; any size is accepted and is box-reduced to at most
/// [`WORKING_EDGE`] pixels on its long side first. Faces are normalized to the same frame.
/// # Errors
/// Invalid pixels, a damaged bundled model, or an inference failure.
pub fn analyse(
    rgb: &[u8],
    width: u32,
    height: u32,
    faces: &[PortraitFace],
    options: Options,
) -> AuraResult<Analysis> {
    let (w, h) = (width as usize, height as usize);
    if w < 8 || h < 8 || w.checked_mul(h).and_then(|n| n.checked_mul(3)) != Some(rgb.len()) {
        return Err(invalid("Invalid skin analysis pixels"));
    }
    if faces.iter().any(|face| {
        let [l, t, r, b] = face.bounds;
        face.bounds
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || l >= r
            || t >= b
            || face.landmarks.iter().flatten().any(|v| !v.is_finite())
            || !face.confidence.is_finite()
    }) {
        return Err(invalid("Invalid skin analysis face geometry"));
    }
    let image = working(rgb, w, h, WORKING_EDGE);
    let turns = orientation::upright_turns(faces, w, h);
    if turns == 0 {
        return analyse_upright(&image, faces, options);
    }
    // Rotate only the bounded analysis proxy. Stored selections and sample points
    // must return to original coordinates before the planner or renderer sees them.
    let image = orientation::image(&image, turns);
    let faces: Vec<_> = faces.iter().map(|f| orientation::face(f, turns)).collect();
    let mut result = analyse_upright(&image, &faces, options)?;
    orientation::restore(&mut result, (4 - turns) % 4);
    Ok(result)
}

fn analyse_upright(
    image: &Image,
    faces: &[PortraitFace],
    options: Options,
) -> AuraResult<Analysis> {
    let precision = if options.precision.is_finite() {
        options.precision.clamp(0.0, 1.0)
    } else {
        0.5
    };
    let softness = if options.softness.is_finite() {
        options.softness.clamp(0.0, 1.0)
    } else {
        0.35
    };
    // Beard and brows are much darker than skin; protecting them sets a floor on the
    // brightness test only, so colour precision stays the photographer's choice.
    let dark = if options.protect_dark_hair {
        precision.max(0.5)
    } else {
        precision
    };
    let (w, h) = (image.w, image.h);
    let (field, passes) = segment(image, faces, options.max_crops)?;
    let luma = image.luma();
    // The network's grid is w / 256 working pixels; the refinement radius follows it, and the
    // photographer's softness widens it.
    let cell = (w.max(h) as f32 / SIDE as f32).max(1.0);
    let radius = ((cell * (0.8 + softness * 2.2)).round() as usize).max(1);
    let eps = 0.0004 + softness * 0.004;

    let mut coverage = [0.0_f32; CLASSES];
    let mut argmax = vec![0_u8; w * h];
    for i in 0..w * h {
        let mut best = 0;
        for k in 1..CLASSES {
            if field.planes[k][i] > field.planes[best][i] {
                best = k;
            }
        }
        argmax[i] = best as u8;
        coverage[best] += 1.0;
    }
    for c in &mut coverage {
        *c /= (w * h) as f32;
    }

    // Faces in working pixels.
    let boxes: Vec<[f32; 4]> = faces
        .iter()
        .map(|f| {
            let [l, t, r, b] = f.bounds;
            [l * w as f32, t * h as f32, r * w as f32, b * h as f32]
        })
        .collect();
    let centre = |k: usize| {
        let [l, t, r, b] = boxes[k];
        [(l + r) * 0.5, (t + b) * 0.5, (r - l).max(b - t).max(1.0)]
    };
    // The face a pixel belongs to: the nearest face centre, in units of that face's size.
    let owner = |x: f32, y: f32| -> Option<(usize, f32)> {
        (0..boxes.len())
            .map(|k| {
                let [cx, cy, size] = centre(k);
                (k, (x - cx).hypot(y - cy) / size)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    };

    let body_regions = assign_regions(field.plane(Class::BodySkin), w, h, &boxes, 7.0);
    let hair_regions = assign_regions(field.plane(Class::Hair), w, h, &boxes, 2.5);
    let clothes_regions = assign_regions(field.plane(Class::Clothes), w, h, &boxes, 6.0);

    // Grow a hard region by a few pixels so the soft edge around it survives.
    let grow = |region: &[bool], by: usize| -> Vec<f32> {
        let as_f: Vec<f32> = region.iter().map(|v| f32::from(u8::from(*v))).collect();
        box_mean(&as_f, w, h, by)
            .into_iter()
            .map(|v| if v > 0.0 { 1.0 } else { 0.0 })
            .collect()
    };
    let refine = |plane: Vec<f32>| -> Vec<f32> {
        guided(&luma, &plane, w, h, radius, eps)
            .into_iter()
            .map(|v| if v < 0.03 { 0.0 } else { v })
            .collect()
    };

    let mut people = Vec::with_capacity(faces.len());
    for k in 0..boxes.len() {
        let [l, t, r, b] = boxes[k];
        let (fw, fh) = (r - l, b - t);
        // Forehead and jaw reach past the detector's box.
        let ex = [l - fw * 0.3, t - fh * 0.55, r + fw * 0.3, b + fh * 0.25];
        let face_plane = field.plane(Class::FaceSkin);
        let mut face = vec![0.0_f32; w * h];
        let mut confident = Vec::new();
        let (x0, y0) = (ex[0].max(0.0) as usize, ex[1].max(0.0) as usize);
        let (x1, y1) = (
            (ex[2].ceil().max(0.0) as usize).min(w),
            (ex[3].ceil().max(0.0) as usize).min(h),
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * w + x;
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                if owner(px, py).map(|(o, _)| o) != Some(k) {
                    continue;
                }
                face[i] = face_plane[i];
                let inner = px > l + fw * 0.1 && px < r - fw * 0.1 && py > t && py < b;
                if inner && face_plane[i] > 0.85 && argmax[i] == Class::FaceSkin as u8 {
                    confident.push(i);
                }
            }
        }
        let face_model = colour_model(image, &confident);
        let gate: Option<Vec<f32>> = face_model.as_ref().map(|model| {
            (0..w * h)
                .map(|i| {
                    if face[i] > 0.0 {
                        model.likeness(image.pixel(i % w, i / w), precision, dark)
                    } else {
                        0.0
                    }
                })
                .collect()
        });
        if let Some(gate) = &gate {
            for (v, g) in face.iter_mut().zip(gate) {
                *v *= g;
            }
        }
        // Refine the edges, then gate again: the guided filter would otherwise fill thin
        // structures such as lashes, brow hairs and stubble back in.
        let mut refined = refine(face);
        if let Some(gate) = &gate {
            for (v, g) in refined.iter_mut().zip(gate) {
                *v *= g;
            }
        }
        let face_matte = to_matte(&refined, w, h, 0.03);

        let body_support = grow(&body_regions[k], radius + 1);
        let body_plane = field.plane(Class::BodySkin);
        let body_confident: Vec<usize> = (0..w * h)
            .filter(|&i| body_regions[k][i] && body_plane[i] > 0.85)
            .collect();
        let body_model = colour_model(image, &body_confident).or(face_model);
        let mut body: Vec<f32> = (0..w * h)
            .map(|i| body_plane[i] * body_support[i])
            .collect();
        if let Some(model) = &body_model {
            // Body skin varies more than a face (hands, elbows, light), so it is judged
            // half as strictly.
            for (i, v) in body.iter_mut().enumerate() {
                if *v > 0.0 {
                    *v *=
                        model.likeness(image.pixel(i % w, i / w), precision * 0.5, precision * 0.5);
                }
            }
        }
        let body_matte = to_matte(&refine(body), w, h, 0.03);

        let hair_support = grow(&hair_regions[k], radius + 1);
        let hair: Vec<f32> = field
            .plane(Class::Hair)
            .iter()
            .zip(&hair_support)
            .map(|(p, s)| p * s)
            .collect();
        let clothes_support = grow(&clothes_regions[k], radius + 1);
        let clothes: Vec<f32> = field
            .plane(Class::Clothes)
            .iter()
            .zip(&clothes_support)
            .map(|(p, s)| p * s)
            .collect();
        people.push(Person {
            face: face_matte,
            body: body_matte,
            hair: to_matte(&refine(hair), w, h, 0.05),
            clothes: to_matte(&refine(clothes), w, h, 0.05),
            face_colour: face_model.map(|m| m.report()),
            body_colour: body_model.map(|m| m.report()),
        });
    }
    let background = to_matte(&refine(field.plane(Class::Background).to_vec()), w, h, 0.05);
    Ok(Analysis {
        people,
        background,
        passes,
        coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touching_people_do_not_share_one_body_owner() {
        let (w, h) = (100, 100);
        let mut plane = vec![0.0; w * h];
        for y in 30..90 {
            for x in 15..85 {
                plane[y * w + x] = 1.0;
            }
        }
        let boxes = [[15.0, 10.0, 35.0, 35.0], [65.0, 10.0, 85.0, 35.0]];
        let regions = assign_regions(&plane, w, h, &boxes, 7.0);
        assert!(regions[0][60 * w + 25]);
        assert!(!regions[0][60 * w + 75]);
        assert!(regions[1][60 * w + 75]);
        for i in 0..w * h {
            assert!(!(regions[0][i] && regions[1][i]));
            assert_eq!(regions[0][i] || regions[1][i], plane[i] > 0.5);
        }
    }

    #[test]
    fn downscaling_keeps_frame_edges_and_thin_panoramas() {
        let mut rgb = vec![0; 2051 * 9 * 3];
        for y in 0..9 {
            rgb[(y * 2051 + 2050) * 3] = 255;
        }
        let image = working(&rgb, 2051, 9, WORKING_EDGE);
        assert!(image.w > 0 && image.h > 0);
        assert!(image.pixel(image.w - 1, image.h - 1)[0] > 0);
        let panorama = working(&vec![120; 10000 * 8 * 3], 10000, 8, WORKING_EDGE);
        assert_eq!(panorama.h, 1);
        assert_eq!(panorama.rgb.len(), panorama.w * panorama.h * 3);
    }

    #[test]
    fn interpreter_matches_onnxruntime_on_a_fixed_pattern() {
        // Reference logits from onnxruntime 1.28 on the converted graph, for the input
        // ((x * 7 + y * 13 + c * 29) % 256) / 127.5 - 1. See ml/models/skin/.
        let mut input = vec![0.0_f32; 3 * SIDE * SIDE];
        for c in 0..3 {
            for y in 0..SIDE {
                for x in 0..SIDE {
                    input[c * SIDE * SIDE + y * SIDE + x] =
                        ((x * 7 + y * 13 + c * 29) % 256) as f32 / 127.5 - 1.0;
                }
            }
        }
        let graph = graph().unwrap().lock().unwrap();
        let out = graph
            .run(&[TensorView::new(vec![1, 3, SIDE, SIDE], &input).unwrap()])
            .unwrap();
        let logits = &out[0].data;
        let at = |k: usize, y: usize, x: usize| logits[k * SIDE * SIDE + y * SIDE + x];
        let expected: [((usize, usize), [f32; 6]); 6] = [
            (
                (0, 0),
                [-0.34041, -1.1908, -1.20016, -0.39858, 1.38323, 1.75401],
            ),
            (
                (17, 200),
                [3.08875, -1.20341, -1.15113, -0.95646, -1.05553, -1.17624],
            ),
            (
                (128, 128),
                [2.39985, -0.47882, -1.9398, -1.67277, -1.06828, 1.36294],
            ),
            (
                (255, 255),
                [2.58382, -0.83618, -0.85399, -0.65047, -1.08774, -1.17018],
            ),
            (
                (90, 33),
                [1.78222, -1.02035, -1.18887, -1.45089, -0.61991, 1.68414],
            ),
            (
                (200, 60),
                [2.73554, -0.9869, -1.84279, -1.63339, -1.05802, 1.20959],
            ),
        ];
        for ((y, x), want) in expected {
            for (k, v) in want.iter().enumerate() {
                assert!(
                    (at(k, y, x) - v).abs() < 2e-3,
                    "class {k} at ({y},{x}): {} against {v}",
                    at(k, y, x)
                );
            }
        }
        let means: Vec<f32> = (0..CLASSES)
            .map(|k| {
                logits[k * SIDE * SIDE..(k + 1) * SIDE * SIDE]
                    .iter()
                    .sum::<f32>()
                    / (SIDE * SIDE) as f32
            })
            .collect();
        for (got, want) in means
            .iter()
            .zip([2.53609, -0.85203, -1.25716, -1.31767, -1.09787, 0.38493])
        {
            assert!((got - want).abs() < 1e-3, "{got} against {want}");
        }
    }

    #[test]
    fn guided_refinement_keeps_a_step_on_a_step_edge() {
        let (w, h) = (40, 10);
        let guide: Vec<f32> = (0..w * h)
            .map(|i| if i % w < 20 { 0.2 } else { 0.8 })
            .collect();
        // A blurry selection whose edge sits on the guide's edge.
        let src: Vec<f32> = (0..w * h)
            .map(|i| ((i % w) as f32 - 15.0).clamp(0.0, 10.0) / 10.0)
            .collect();
        let out = guided(&guide, &src, w, h, 3, 1e-4);
        assert!(out[5 * w + 17] < 0.35, "{}", out[5 * w + 17]);
        assert!(out[5 * w + 22] > 0.65, "{}", out[5 * w + 22]);
    }

    #[test]
    fn mattes_crop_to_support_and_cap_their_resolution() {
        let (w, h) = (600, 400);
        let mut plane = vec![0.0_f32; w * h];
        for y in 100..300 {
            for x in 50..550 {
                plane[y * w + x] = 1.0;
            }
        }
        let matte = to_matte(&plane, w, h, 0.05).unwrap();
        assert!(matte.width <= MATTE_EDGE && matte.height <= MATTE_EDGE);
        assert!((matte.bounds[0] - 48.0 / 600.0).abs() < 0.01);
        assert!(matte.at(0.5, 0.5) > 0.99);
        assert!(matte.at(0.02, 0.5) < 0.01);
        assert!((matte.area() - 500.0 * 200.0 / (600.0 * 400.0)).abs() < 0.02);
        assert!(to_matte(&vec![0.0; w * h], w, h, 0.05).is_none());
    }

    #[test]
    fn blank_frames_have_no_people_and_invalid_buffers_are_refused() {
        assert!(analyse(&[0; 10], 4, 4, &[], Options::default()).is_err());
        let mut face = PortraitFace {
            bounds: [0.1, 0.1, 0.9, 0.9],
            landmarks: [[0.5, 0.5]; 5],
            confidence: 0.9,
        };
        face.landmarks[0][0] = f32::NAN;
        assert!(analyse(&[128; 8 * 8 * 3], 8, 8, &[face.clone()], Options::default()).is_err());
        face.landmarks[0][0] = 0.5;
        face.bounds = [0.8, 0.1, 0.2, 0.9];
        assert!(analyse(&[128; 8 * 8 * 3], 8, 8, &[face], Options::default()).is_err());
        let analysis = analyse(&vec![128; 64 * 48 * 3], 64, 48, &[], Options::default()).unwrap();
        assert!(analysis.people.is_empty());
        assert_eq!(analysis.passes, 1);
    }
}
