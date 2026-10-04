//! Portrait regions and portrait retouching, executed inside the render.
//!
//! # Why the renderer parses the photograph itself
//!
//! Phase 14's rule is that a delivered file can be re-created from four values: the RAW's
//! content hash, the canonical recipe, the engine string and the output spec. A face mask
//! looked up in the catalog at render time would be a fifth. So the regions a recipe names -
//! `face`, `skin`, `teeth`, `hair`, `background` - are *re-derived from the pixels* by
//! `aura_portrait`, a pure function of the frame, and [`PARSE_VER`] is folded into the engine
//! string so a change to that function is a change to the engine. A photographer's own face
//! box travels in the recipe as a mask whose `target` is `hint:x,y,w,h`, which keeps that
//! decision inside the four values too.
//!
//! The parse runs on the frame as it arrived, before any slider, so moving an exposure or a
//! temperature does not move a mask - and the result is cached by the frame's own content,
//! so the second slider move costs nothing.
//!
//! # The operators
//!
//! Fourteen, named by [`OPERATORS`], each of which reads one region by default and applies at
//! `strength` in `0..=1`. They share three rules, and the rules are the product:
//!
//! 1. **Texture is a separate band and survives.** Smoothing reduces the mid band - blotches,
//!    uneven tone - and keeps the fine band where pores live, scaled by `protect_texture`.
//!    A skin smoothed to plastic is a retouch nobody asked for.
//! 2. **Nobody's skin tone moves.** Evening pulls each pixel's chroma toward the *local
//!    average of that person's own skin*, not toward a target; there is no constant anywhere in
//!    this module a person's skin could be compared against. Brightness operators on skin are
//!    fills - shadows lift, highlights do not - never a lightening of the tone itself.
//! 3. **Nothing reshapes and nothing is invented.** Every operator changes a pixel's tone or
//!    colour in place. There is no warp, no liquify and no generated pixel.
//!
//! Every mask is a normalised convolution - a blur of skin over a blur of the skin mask - so
//! the dark of an eye or a nostril never bleeds into the cheek beside it.
//!
//! Everything here is linear. Luminance is processed as its logarithm so a band split behaves
//! the same in a shadow as in a highlight, and every colour is re-applied as a ratio to it.

use std::sync::{Arc, Mutex, PoisonError};

use aura_portrait::canvas::{Canvas, Primaries};
use aura_portrait::face::FaceHint;
use aura_portrait::plane::blur_values;
use aura_portrait::{PortraitMap, Region, ANALYSIS_EDGE, PARSE_VER};
use aura_recipe::{Mask, MaskKind, Recipe, RetouchOp};
use rayon::prelude::*;

use crate::colour::luma;
use crate::contract::render::{RenderNote, SkipReason};
use crate::cpu::Frame;
use crate::graph::Stage;

/// Every portrait operator this renderer executes, in the order the panel offers them.
pub const OPERATORS: [&str; 14] = [
    "skin_smooth",
    "skin_even",
    "blemish_clear",
    "under_eye_lift",
    "shine_control",
    "face_light",
    "eye_brighten",
    "iris_enhance",
    "sclera_whiten",
    "brow_define",
    "teeth_whiten",
    "lip_enhance",
    "hair_define",
    "background_blur",
];

/// True when this renderer executes an operator of this name.
#[must_use]
pub fn is_operator(name: &str) -> bool {
    OPERATORS.contains(&name)
}

/// The region an operator works inside unless the recipe says otherwise.
#[must_use]
pub fn default_region(op: &str) -> Option<Region> {
    Some(match op {
        "skin_smooth" | "skin_even" | "blemish_clear" => Region::Skin,
        "under_eye_lift" => Region::UnderEyes,
        "shine_control" | "face_light" => Region::Face,
        "eye_brighten" => Region::Eyes,
        "iris_enhance" => Region::Iris,
        "sclera_whiten" => Region::Sclera,
        "brow_define" => Region::Eyebrows,
        "teeth_whiten" => Region::Teeth,
        "lip_enhance" => Region::Lips,
        "hair_define" => Region::Hair,
        "background_blur" => Region::Background,
        _ => return None,
    })
}

/// The face boxes a photographer drew, carried as masks whose target is `hint:x,y,w,h`.
#[must_use]
pub fn hints(recipe: &Recipe) -> Vec<FaceHint> {
    recipe
        .masks
        .iter()
        .filter_map(|m| m.target.as_deref().and_then(FaceHint::parse))
        .collect()
}

/// True for a mask that only carries a face box and adjusts nothing.
#[must_use]
pub fn is_hint(mask: &Mask) -> bool {
    mask.target
        .as_deref()
        .is_some_and(|t| t.starts_with("hint:"))
}

/// The portrait region a generated mask resolves to, or `None` when it is not one this
/// renderer can draw - a brush stroke, a geometric gradient, a hint.
#[must_use]
pub fn mask_region(mask: &Mask) -> Option<Region> {
    if is_hint(mask) {
        return None;
    }
    if let Some(target) = mask.target.as_deref() {
        if let Some(region) = Region::parse(target) {
            return Some(region);
        }
    }
    kind_region(mask.kind)
}

fn kind_region(kind: MaskKind) -> Option<Region> {
    match kind {
        MaskKind::Face => Some(Region::Face),
        MaskKind::Subject => Some(Region::Body),
        MaskKind::Background => Some(Region::Background),
        MaskKind::Sky => Some(Region::Sky),
        MaskKind::Skin => Some(Region::Skin),
        MaskKind::Linear | MaskKind::Radial | MaskKind::Brush => None,
    }
}

/// True when rendering this recipe needs a parse of the photograph.
#[must_use]
pub fn wants_parse(recipe: &Recipe) -> bool {
    recipe.retouch.iter().any(|op| is_operator(&op.op))
        || recipe.masks.iter().any(|m| mask_region(m).is_some())
}

/// The parses this process has already made, newest last.
///
/// Keyed by the content of the analysis canvas and the hints, so the cache cannot serve one
/// photograph's faces for another and does not care which preview tier asked. Six entries:
/// a photographer moving between a handful of frames in a session.
static CACHE: Mutex<Vec<(blake3::Hash, Arc<PortraitMap>)>> = Mutex::new(Vec::new());

/// How many parses are kept.
const CACHE_ENTRIES: usize = 6;

/// Parse a whole frame, or return the parse already made of the same pixels.
///
/// `None` for a tile: a face cut in half by a tile boundary is not a face, and the streamed
/// path renders a portrait recipe whole for that reason.
#[must_use]
pub fn parse(frame: &Frame, hints: &[FaceHint]) -> Option<Arc<PortraitMap>> {
    if frame.origin != (0, 0) || frame.full != (frame.width, frame.height) {
        return None;
    }
    let canvas = Canvas::from_linear(
        &frame.rgb,
        frame.width,
        frame.height,
        Primaries::Rec2020,
        ANALYSIS_EDGE,
    )?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&PARSE_VER.to_le_bytes());
    hasher.update(&canvas.width.to_le_bytes());
    hasher.update(&canvas.height.to_le_bytes());
    for pixel in &canvas.srgb {
        for channel in pixel {
            hasher.update(&channel.to_le_bytes());
        }
    }
    for hint in hints {
        hasher.update(hint.to_target().as_bytes());
    }
    let key = hasher.finalize();
    {
        let cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, map)) = cache.iter().find(|(k, _)| *k == key) {
            return Some(Arc::clone(map));
        }
    }
    let map = Arc::new(aura_portrait::analyse(&canvas, hints));
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if !cache.iter().any(|(k, _)| *k == key) {
        cache.push((key, Arc::clone(&map)));
        if cache.len() > CACHE_ENTRIES {
            cache.remove(0);
        }
    }
    Some(map)
}

/// A region on the render grid, `width * height` weights.
#[must_use]
pub fn resolve(map: &PortraitMap, region: Region, width: u32, height: u32) -> Vec<f32> {
    map.resolve(region, width, height).values
}

/// The interocular distance of the faces in render pixels: the unit every spatial radius here
/// is sized in. The median face, so one guest in the corner does not set the radius for the
/// bride.
fn face_unit(map: &PortraitMap, width: u32) -> f32 {
    let scale = width as f32 / map.width.max(1) as f32;
    let mut units: Vec<f32> = map.faces.iter().map(|f| f.interocular() * scale).collect();
    units.sort_by(f32::total_cmp);
    units
        .get(units.len() / 2)
        .copied()
        .unwrap_or(width as f32 * 0.08)
        .max(4.0)
}

/// Apply every generated mask a recipe carries. Hints are skipped - they adjust nothing.
pub fn apply_masks(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    recipe: &Recipe,
    map: &PortraitMap,
) -> Vec<RenderNote> {
    let mut notes = Vec::new();
    let long = width.max(height) as f32;
    let weight_of = |mask: &Mask| -> Option<Vec<f32>> {
        let region = mask_region(mask)?;
        let mut plane = resolve(map, region, width, height);
        let radius = (mask.feather.clamp(0.0, 1.0) * 0.01 * long).round() as usize;
        if radius > 0 {
            plane = blur_values(&plane, width as usize, height as usize, radius);
        }
        Some(plane)
    };
    for mask in &recipe.masks {
        if is_hint(mask) || mask.params.is_empty() {
            continue;
        }
        let Some(region) = mask_region(mask) else {
            continue;
        };
        if region.needs_a_face() && map.faces.is_empty() {
            notes.push(no_face(Stage::Masks, &mask.id));
            continue;
        }
        let weights = match mask.invert_of.as_deref() {
            Some(other) => {
                let base = recipe
                    .masks
                    .iter()
                    .find(|m| m.id == other)
                    .and_then(weight_of)
                    .or_else(|| {
                        kind_from_str(other)
                            .and_then(kind_region)
                            .map(|r| resolve(map, r, width, height))
                    });
                match base {
                    Some(plane) => plane.iter().map(|v| 1.0 - v.clamp(0.0, 1.0)).collect(),
                    None => continue,
                }
            }
            None => match weight_of(mask) {
                Some(plane) => plane,
                None => continue,
            },
        };
        let params = mask.params;
        rgb.par_chunks_mut(3)
            .zip(weights.par_iter())
            .for_each(|(pixel, weight)| {
                if *weight <= 1e-4 {
                    return;
                }
                let value = [
                    pixel.first().copied().unwrap_or(0.0),
                    pixel.get(1).copied().unwrap_or(0.0),
                    pixel.get(2).copied().unwrap_or(0.0),
                ];
                let out = crate::cpu::apply_mask_params(value, &params, weight.clamp(0.0, 1.0));
                for (slot, v) in pixel.iter_mut().zip(out.iter()) {
                    *slot = *v;
                }
            });
    }
    notes
}

fn kind_from_str(text: &str) -> Option<MaskKind> {
    Some(match text {
        "face" => MaskKind::Face,
        "subject" => MaskKind::Subject,
        "background" => MaskKind::Background,
        "sky" => MaskKind::Sky,
        "skin" => MaskKind::Skin,
        _ => return None,
    })
}

fn no_face(stage: Stage, what: &str) -> RenderNote {
    RenderNote {
        stage: stage.as_str().to_string(),
        reason: SkipReason::MaskGeneratorAbsent,
        detail: Some(format!("{what}: no face was found in this photograph")),
    }
}

/// Apply every portrait operator in a recipe, in the recipe's order.
///
/// Operators this renderer does not know are not applied here; the graph has already noted
/// each one as absent, which is the difference between a skipped operator and a silent one.
pub fn apply_retouch(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    recipe: &Recipe,
    map: &PortraitMap,
) -> Vec<RenderNote> {
    let mut notes = Vec::new();
    let unit = face_unit(map, width);
    for op in &recipe.retouch {
        if !is_operator(&op.op) {
            continue;
        }
        let Some(region) = op
            .mask
            .as_deref()
            .and_then(|m| {
                recipe
                    .masks
                    .iter()
                    .find(|mask| mask.id == m)
                    .and_then(mask_region)
                    .or_else(|| kind_from_str(m).and_then(kind_region))
            })
            .or_else(|| default_region(&op.op))
        else {
            continue;
        };
        if region.needs_a_face() && map.faces.is_empty() {
            notes.push(no_face(Stage::Retouch, &op.op));
            continue;
        }
        let mask = resolve(map, region, width, height);
        if mask.iter().all(|v| *v <= 1e-3) {
            continue;
        }
        let mut image = Image {
            rgb,
            width: width as usize,
            height: height as usize,
        };
        let context = Context {
            map,
            width,
            height,
            unit,
        };
        apply_one(&mut image, op, &mask, &context);
    }
    notes
}

/// The buffer an operator writes.
struct Image<'a> {
    rgb: &'a mut [f32],
    width: usize,
    height: usize,
}

/// What an operator may read besides its own mask.
struct Context<'a> {
    map: &'a PortraitMap,
    width: u32,
    height: u32,
    unit: f32,
}

fn apply_one(image: &mut Image<'_>, op: &RetouchOp, mask: &[f32], cx: &Context<'_>) {
    let s = op.strength.clamp(0.0, 1.0);
    if s <= 0.0 {
        return;
    }
    let p = op.protect_texture.clamp(0.0, 1.0);
    match op.op.as_str() {
        "skin_smooth" => skin_smooth(image, mask, s, p, cx.unit),
        "skin_even" => skin_even(image, mask, s, cx.unit),
        "blemish_clear" => blemish_clear(image, mask, s, cx.unit),
        "under_eye_lift" => {
            let reference = resolve(cx.map, Region::Skin, cx.width, cx.height);
            under_eye_lift(image, mask, &reference, s, cx.unit);
        }
        "shine_control" => {
            let skin = resolve(cx.map, Region::Skin, cx.width, cx.height);
            let face_skin: Vec<f32> = mask.iter().zip(skin.iter()).map(|(a, b)| a * b).collect();
            shine_control(image, &face_skin, s, cx.unit);
        }
        "face_light" => face_light(image, mask, s),
        "eye_brighten" => eye_brighten(image, mask, s, cx.unit),
        "iris_enhance" => iris_enhance(image, mask, s, cx.unit),
        "sclera_whiten" => sclera_whiten(image, mask, s),
        "brow_define" => brow_define(image, mask, s, cx.unit),
        "teeth_whiten" => teeth_whiten(image, mask, s),
        "lip_enhance" => lip_enhance(image, mask, s, cx.unit),
        "hair_define" => hair_define(image, mask, s, cx.unit),
        "background_blur" => background_blur(image, mask, s, cx.unit),
        _ => {}
    }
}

// --- shared arithmetic ---------------------------------------------------------------------

const FLOOR: f32 = 1e-5;

fn pixel(rgb: &[f32], i: usize) -> [f32; 3] {
    [
        rgb.get(i * 3).copied().unwrap_or(0.0),
        rgb.get(i * 3 + 1).copied().unwrap_or(0.0),
        rgb.get(i * 3 + 2).copied().unwrap_or(0.0),
    ]
}

fn write(rgb: &mut [f32], i: usize, value: [f32; 3]) {
    if let Some(slot) = rgb.get_mut(i * 3..i * 3 + 3) {
        for (s, v) in slot.iter_mut().zip(value) {
            *s = if v.is_finite() { v.max(0.0) } else { 0.0 };
        }
    }
}

/// Log luminance of every pixel.
fn log_luma(image: &Image<'_>) -> Vec<f32> {
    image
        .rgb
        .par_chunks(3)
        .map(|p| {
            luma([
                p.first().copied().unwrap_or(0.0),
                p.get(1).copied().unwrap_or(0.0),
                p.get(2).copied().unwrap_or(0.0),
            ])
            .max(FLOOR)
            .ln()
        })
        .collect()
}

fn blur(values: &[f32], image: &Image<'_>, radius: f32) -> Vec<f32> {
    blur_values(
        values,
        image.width,
        image.height,
        radius.round().max(1.0) as usize,
    )
}

/// A normalised convolution: the average of `values` over the pixels `mask` covers, so the
/// dark of an eye or a nostril never bleeds into the skin beside it.
fn masked_blur(values: &[f32], mask: &[f32], image: &Image<'_>, radius: f32) -> Vec<f32> {
    let weighted: Vec<f32> = values
        .iter()
        .zip(mask.iter())
        .map(|(v, m)| v * m.max(0.0))
        .collect();
    let top = blur(&weighted, image, radius);
    let bottom = blur(mask, image, radius);
    top.iter()
        .zip(bottom.iter())
        .zip(values.iter())
        .map(|((t, b), v)| if *b > 1e-3 { t / b } else { *v })
        .collect()
}

/// Rewrite every masked pixel's luminance to `exp(new_log)`, holding its colour.
fn set_log_luma(image: &mut Image<'_>, old: &[f32], new: &[f32], mask: &[f32]) {
    let n = image.width * image.height;
    for i in 0..n {
        let m = mask.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        if m <= 1e-4 {
            continue;
        }
        let before = old.get(i).copied().unwrap_or(0.0);
        let after = new.get(i).copied().unwrap_or(before);
        let ratio = ((after - before) * m).exp();
        let p = pixel(image.rgb, i);
        write(image.rgb, i, [p[0] * ratio, p[1] * ratio, p[2] * ratio]);
    }
}

/// Each pixel's colour as a ratio to its luminance: three planes.
fn chroma_ratios(image: &Image<'_>) -> [Vec<f32>; 3] {
    let n = image.width * image.height;
    let mut out = [vec![0.0; n], vec![0.0; n], vec![0.0; n]];
    for i in 0..n {
        let p = pixel(image.rgb, i);
        let y = luma(p).max(FLOOR);
        for (plane, value) in out.iter_mut().zip(p) {
            if let Some(slot) = plane.get_mut(i) {
                *slot = value / y;
            }
        }
    }
    out
}

/// Pull each masked pixel's colour ratios toward `target` by `amount` of the mask, holding
/// its luminance.
fn pull_chroma(image: &mut Image<'_>, target: &[Vec<f32>; 3], amount: &[f32]) {
    let n = image.width * image.height;
    for i in 0..n {
        let k = amount.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        if k <= 1e-4 {
            continue;
        }
        let p = pixel(image.rgb, i);
        let y = luma(p).max(FLOOR);
        let mut out = [0.0_f32; 3];
        for ((slot, value), plane) in out.iter_mut().zip(p).zip(target.iter()) {
            let ratio = value / y;
            let goal = plane.get(i).copied().unwrap_or(ratio);
            *slot = (ratio + (goal - ratio) * k) * y;
        }
        // The pull is in ratio space, which moves luminance slightly; put it back exactly.
        let moved = luma(out).max(FLOOR);
        let fix = y / moved;
        write(image.rgb, i, [out[0] * fix, out[1] * fix, out[2] * fix]);
    }
}

fn ramp(x: f32, lo: f32, hi: f32) -> f32 {
    aura_portrait::skin::ramp(x, lo, hi)
}

// --- the operators ---------------------------------------------------------------------------

/// Frequency-separated smoothing: the mid band goes, the pores stay.
fn skin_smooth(image: &mut Image<'_>, mask: &[f32], s: f32, protect: f32, unit: f32) {
    let l = log_luma(image);
    let low = masked_blur(&l, mask, image, unit * 0.22);
    let small = masked_blur(&l, mask, image, (unit * 0.025).max(1.0));
    let keep_mid = 1.0 - 0.85 * s;
    let keep_fine = 1.0 - 0.45 * s * (1.0 - protect);
    let new: Vec<f32> = l
        .iter()
        .zip(low.iter().zip(small.iter()))
        .map(|(v, (lo, sm))| {
            let target = lo + (sm - lo) * keep_mid + (v - sm) * keep_fine;
            v + (target - v).clamp(-0.35, 0.35)
        })
        .collect();
    set_log_luma(image, &l, &new, mask);
}

/// Even the tone: every pixel's colour moves toward the local average of the same person's
/// skin, never toward a target.
fn skin_even(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    let ratios = chroma_ratios(image);
    let radius = unit * 0.35;
    let target = [
        masked_blur(&ratios[0], mask, image, radius),
        masked_blur(&ratios[1], mask, image, radius),
        masked_blur(&ratios[2], mask, image, radius),
    ];
    let amount: Vec<f32> = mask.iter().map(|m| m * 0.7 * s).collect();
    pull_chroma(image, &target, &amount);
}

/// Small dark or red marks, healed toward the skin around them. A very dark, compact mark is
/// left alone: that is a mole or a nostril, and it is part of how somebody looks.
fn blemish_clear(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    let l = log_luma(image);
    let ratios = chroma_ratios(image);
    let r_core = (unit * 0.015).max(1.0);
    let r_ring = (unit * 0.07).max(2.0);
    let core = masked_blur(&l, mask, image, r_core);
    let ring = masked_blur(&l, mask, image, r_ring);
    let red_core = masked_blur(&ratios[0], mask, image, r_core);
    let red_ring = masked_blur(&ratios[0], mask, image, r_ring);
    let n = image.width * image.height;
    let mut weight = vec![0.0_f32; n];
    for (i, slot) in weight.iter_mut().enumerate() {
        let m = mask.get(i).copied().unwrap_or(0.0);
        if m <= 1e-3 {
            continue;
        }
        let spot = core.get(i).copied().unwrap_or(0.0) - ring.get(i).copied().unwrap_or(0.0);
        let red = red_core.get(i).copied().unwrap_or(0.0) - red_ring.get(i).copied().unwrap_or(0.0);
        let mark = ramp(-spot, 0.04, 0.12).max(ramp(red, 0.04, 0.12));
        let mole = ramp(-spot, 0.35, 0.55);
        *slot = m * mark * (1.0 - mole);
    }
    let weight = blur(&weight, image, r_core);
    let new: Vec<f32> = (0..n)
        .map(|i| {
            let v = l.get(i).copied().unwrap_or(0.0);
            let w = weight.get(i).copied().unwrap_or(0.0) * s;
            v + w * (ring.get(i).copied().unwrap_or(v) - core.get(i).copied().unwrap_or(v))
        })
        .collect();
    let full: Vec<f32> = vec![1.0; n];
    set_log_luma(image, &l, &new, &full);
    let target = [
        masked_blur(&ratios[0], mask, image, r_ring),
        masked_blur(&ratios[1], mask, image, r_ring),
        masked_blur(&ratios[2], mask, image, r_ring),
    ];
    let amount: Vec<f32> = weight.iter().map(|w| w * s).collect();
    pull_chroma(image, &target, &amount);
}

/// Lift the shadow under the eyes toward the level of the cheek below it.
fn under_eye_lift(image: &mut Image<'_>, mask: &[f32], skin: &[f32], s: f32, unit: f32) {
    let l = log_luma(image);
    let local = masked_blur(&l, mask, image, unit * 0.08);
    let around = masked_blur(&l, skin, image, unit * 0.45);
    let new: Vec<f32> = l
        .iter()
        .zip(local.iter().zip(around.iter()))
        .map(|(v, (lo, ar))| v + (ar - lo).clamp(0.0, 0.5) * 0.75 * s)
        .collect();
    set_log_luma(image, &l, &new, mask);
    let ratios = chroma_ratios(image);
    let target = [
        masked_blur(&ratios[0], skin, image, unit * 0.45),
        masked_blur(&ratios[1], skin, image, unit * 0.45),
        masked_blur(&ratios[2], skin, image, unit * 0.45),
    ];
    let amount: Vec<f32> = mask.iter().map(|m| m * 0.4 * s).collect();
    pull_chroma(image, &target, &amount);
}

/// Compress specular shine on skin back toward the skin around it.
fn shine_control(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    let l = log_luma(image);
    let local = masked_blur(&l, mask, image, unit * 0.18);
    let excess: Vec<f32> = l
        .iter()
        .zip(local.iter())
        .map(|(v, lo)| (v - (lo + 0.12)).max(0.0))
        .collect();
    let new: Vec<f32> = l
        .iter()
        .zip(excess.iter())
        .map(|(v, e)| v - e * 0.75 * s)
        .collect();
    set_log_luma(image, &l, &new, mask);
    let ratios = chroma_ratios(image);
    let target = [
        masked_blur(&ratios[0], mask, image, unit * 0.18),
        masked_blur(&ratios[1], mask, image, unit * 0.18),
        masked_blur(&ratios[2], mask, image, unit * 0.18),
    ];
    let amount: Vec<f32> = mask
        .iter()
        .zip(excess.iter())
        .map(|(m, e)| m * ramp(*e, 0.0, 0.3) * 0.6 * s)
        .collect();
    pull_chroma(image, &target, &amount);
}

/// A fill light on the face: shadows and midtones lift, highlights do not.
fn face_light(image: &mut Image<'_>, mask: &[f32], s: f32) {
    let l = log_luma(image);
    let stop = std::f32::consts::LN_2;
    let new: Vec<f32> = l
        .iter()
        .map(|v| {
            let y = v.exp();
            v + 0.45 * s * stop * (1.0 - ramp(y, 0.25, 0.7))
        })
        .collect();
    set_log_luma(image, &l, &new, mask);
}

/// Unsharp the log luminance inside a mask by `amount` at `radius`.
fn local_contrast(image: &mut Image<'_>, mask: &[f32], radius: f32, amount: f32, lift: f32) {
    let l = log_luma(image);
    let base = blur(&l, image, radius);
    let new: Vec<f32> = l
        .iter()
        .zip(base.iter())
        .map(|(v, b)| v + (v - b) * amount + lift)
        .collect();
    set_log_luma(image, &l, &new, mask);
}

fn eye_brighten(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    let l = log_luma(image);
    let stop = std::f32::consts::LN_2;
    let new: Vec<f32> = l
        .iter()
        .map(|v| v + 0.35 * s * stop * (1.0 - ramp(v.exp(), 0.5, 0.85)))
        .collect();
    set_log_luma(image, &l, &new, mask);
    local_contrast(image, mask, (unit * 0.04).max(1.0), 0.25 * s, 0.0);
}

fn iris_enhance(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    local_contrast(
        image,
        mask,
        (unit * 0.03).max(1.0),
        0.8 * s,
        0.15 * s * std::f32::consts::LN_2,
    );
    saturate(image, mask, 0.35 * s);
}

/// Scale every masked pixel's saturation by `1 + amount * mask`.
fn saturate(image: &mut Image<'_>, mask: &[f32], amount: f32) {
    let n = image.width * image.height;
    for i in 0..n {
        let m = mask.get(i).copied().unwrap_or(0.0);
        if m <= 1e-4 {
            continue;
        }
        let p = pixel(image.rgb, i);
        write(image.rgb, i, crate::tonemap::saturate(p, 1.0 + amount * m));
    }
}

/// Neutralise redness in the whites of the eyes and lift them a little, never to paper white.
fn sclera_whiten(image: &mut Image<'_>, mask: &[f32], s: f32) {
    let n = image.width * image.height;
    for i in 0..n {
        let m = mask.get(i).copied().unwrap_or(0.0);
        if m <= 1e-4 {
            continue;
        }
        let p = pixel(image.rgb, i);
        let toned = crate::tonemap::saturate(p, 1.0 - 0.6 * s * m);
        let y = luma(toned).max(FLOOR);
        let lifted = (y * (0.25 * s * m).exp2()).min(y.max(0.8));
        let k = lifted / y;
        write(image.rgb, i, [toned[0] * k, toned[1] * k, toned[2] * k]);
    }
}

/// Darken and define the brows slightly.
fn brow_define(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    local_contrast(
        image,
        mask,
        (unit * 0.03).max(1.0),
        0.4 * s,
        -0.12 * s * std::f32::consts::LN_2,
    );
}

/// Take the yellow out of teeth and brighten them a little - bounded so a smile stays a smile.
fn teeth_whiten(image: &mut Image<'_>, mask: &[f32], s: f32) {
    let n = image.width * image.height;
    for i in 0..n {
        let m = mask.get(i).copied().unwrap_or(0.0);
        if m <= 1e-4 {
            continue;
        }
        let p = pixel(image.rgb, i);
        let toned = crate::tonemap::saturate(p, 1.0 - 0.55 * s * m);
        let y = luma(toned).max(FLOOR);
        let lifted = (y * (0.3 * s * m).exp2()).min(y.max(0.75));
        let k = lifted / y;
        write(image.rgb, i, [toned[0] * k, toned[1] * k, toned[2] * k]);
    }
}

fn lip_enhance(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    saturate(image, mask, 0.3 * s);
    local_contrast(image, mask, (unit * 0.05).max(1.0), 0.15 * s, 0.0);
}

fn hair_define(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    local_contrast(image, mask, (unit * 0.06).max(1.0), 0.6 * s, 0.0);
}

/// A shallow-focus background: the ground is blurred with the person excluded from the blur,
/// so their colour never bleeds into it.
fn background_blur(image: &mut Image<'_>, mask: &[f32], s: f32, unit: f32) {
    let long = image.width.max(image.height) as f32;
    let radius = (s * 0.012 * long).clamp(1.0, 48.0);
    // Held back from the person's edge by a few pixels: a sharp sliver of background beside a
    // shoulder reads as depth of field, a softened curl or ear reads as a mistake.
    let reach = (unit * 0.05).max(2.0);
    let eroded: Vec<f32> = blur(mask, image, reach)
        .iter()
        .zip(mask.iter())
        .map(|(b, m)| m.min(ramp(*b, 0.8, 0.98)))
        .collect();
    let mask = eroded.as_slice();
    let n = image.width * image.height;
    let mut channels = [vec![0.0_f32; n], vec![0.0_f32; n], vec![0.0_f32; n]];
    for i in 0..n {
        let p = pixel(image.rgb, i);
        for (plane, value) in channels.iter_mut().zip(p) {
            if let Some(slot) = plane.get_mut(i) {
                *slot = value;
            }
        }
    }
    let blurred = [
        masked_blur(&channels[0], mask, image, radius),
        masked_blur(&channels[1], mask, image, radius),
        masked_blur(&channels[2], mask, image, radius),
    ];
    for i in 0..n {
        let m = mask.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        if m <= 1e-4 {
            continue;
        }
        let p = pixel(image.rgb, i);
        let b = [
            blurred[0].get(i).copied().unwrap_or(p[0]),
            blurred[1].get(i).copied().unwrap_or(p[1]),
            blurred[2].get(i).copied().unwrap_or(p[2]),
        ];
        write(
            image.rgb,
            i,
            [
                p[0] + (b[0] - p[0]) * m,
                p[1] + (b[1] - p[1]) * m,
                p[2] + (b[2] - p[2]) * m,
            ],
        );
    }
}
