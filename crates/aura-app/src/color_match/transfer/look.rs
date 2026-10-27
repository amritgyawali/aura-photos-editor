//! Follow a reference's look on a different photograph (ADR-0170).
//!
//! The conservative fit in the parent module shrinks every unrelated-scene estimate toward no
//! edit, because it is trying not to guess an editor's settings. Asked instead to make a photo
//! *look like* the reference, a literal distribution match is wrong in the other direction: a
//! reference that is mostly dark foliage darkens a bride's face to match its histogram. This fit
//! transfers what a look is made of and keeps what the scene is made of:
//!
//! * **cast and split tone** - the reference's neutral axis, in full;
//! * **tone shape** - the reference's spread of tones around its own middle grey, placed around
//!   *this* photograph's middle grey, so contrast, lifted blacks and rolled whites travel while
//!   the subject's exposure moves only a bounded share of the brightness difference;
//! * **colourfulness** - the chroma distribution, blended with the shared-colour ratio so an
//!   empty grey wall in one photograph does not desaturate the other;
//! * **hue** - per-sector colour, hue and lightness where both photographs hold that colour.
//!
//! Each person stays themselves. The fit is checked on this photograph's own skin samples and
//! its curves corrected until skin brightness moves at most [`SKIN_LIGHT_LIMIT`] and skin keeps
//! between [`SKIN_CHROMA_FLOOR`] and [`SKIN_CHROMA_CEILING`] of its colourfulness. Warmth along
//! the skin's own hue travels; the renderer's skin protection, which [`super::review_skin`]
//! raises when a face would turn, keeps it from turning green or grey.
use super::{
    at, chroma_curve, distance, measure, quantile_map, regularise_tone, shared_chroma_scale,
    transformed, update_sectors, Fit, GradeStats, ReferenceGrade, Sample, CHROMA_NODES,
    CHROMA_QUANTILES, CHROMA_SPAN, LIGHT_QUANTILES, MIN_SAMPLES, SECTORS, SECTOR_WIDTH, TONE_NODES,
};
use crate::color_match::appearance::Region;
use aura_core::AuraResult;
use aura_raw::codec::Rgb8;
use aura_raw::colour::curve::srgb_byte_to_linear;
use aura_render::reference_grade::{oklab_from_linear_srgb, Evaluator};

/// Largest change in a person's mean skin lightness (`OKLab` L) the look may make.
pub(in crate::color_match) const SKIN_LIGHT_LIMIT: f32 = 0.035;
/// Skin keeps at least this share of its colourfulness, and gains at most the ceiling.
pub(in crate::color_match) const SKIN_CHROMA_FLOOR: f32 = 0.85;
pub(in crate::color_match) const SKIN_CHROMA_CEILING: f32 = 1.3;
const MAX_SKIN_SAMPLES: usize = 4096;
/// Width of the tone-curve correction around skin lightness.
const SKIN_TONE_WIDTH: f32 = 0.18;

/// The photograph's pooled face and body skin, as fit samples.
pub(in crate::color_match) fn skin_samples(image: &Rgb8, regions: &[Region]) -> Vec<Sample> {
    let points: Vec<[f32; 2]> = regions
        .iter()
        .filter(|r| r.name == "face skin" || r.name == "body skin")
        .flat_map(|r| r.points.iter().copied())
        .collect();
    let stride = points.len().div_ceil(MAX_SKIN_SAMPLES).max(1);
    points
        .into_iter()
        .step_by(stride)
        .filter_map(|[x, y]| {
            let column =
                ((x * image.width as f32) as usize).min(image.width.saturating_sub(1) as usize);
            let row =
                ((y * image.height as f32) as usize).min(image.height.saturating_sub(1) as usize);
            let at = (row * image.width as usize + column) * 3;
            let pixel = <[u8; 3]>::try_from(image.data.get(at..at + 3)?).ok()?;
            let linear = pixel.map(srgb_byte_to_linear);
            Some(Sample {
                linear,
                lab: oklab_from_linear_srgb(linear),
            })
        })
        .collect()
}

#[allow(clippy::many_single_char_names)] // The renderer's own interpolation, letter for letter.
fn axis_at(nodes: &[[f32; 2]], lightness: f32) -> [f32; 2] {
    let last = nodes.len().saturating_sub(1);
    if last == 0 {
        return nodes.first().copied().unwrap_or([0.0; 2]);
    }
    let x = lightness.clamp(0.0, 1.0) * last as f32;
    let i = (x as usize).min(last - 1);
    let t = x - i as f32;
    let a = nodes.get(i).copied().unwrap_or([0.0; 2]);
    let b = nodes.get(i + 1).copied().unwrap_or(a);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// What the grade does to skin: mean lightness change, chroma ratio, mean input lightness, and
/// how the skin is spread over the renderer's hue sectors.
pub(in crate::color_match) struct SkinEffect {
    pub light: f32,
    pub ratio: f32,
    pub base: f32,
    pub weight: Vec<f32>,
}

pub(in crate::color_match) fn skin_effect(grade: &ReferenceGrade, skin: &[Sample]) -> SkinEffect {
    let evaluator = Evaluator::new(grade);
    let (mut light, mut base, mut chroma_in, mut chroma_out) = (0.0, 0.0, 0.0, 0.0);
    let mut weight = vec![0.0_f32; SECTORS];
    for s in skin {
        let out = oklab_from_linear_srgb(evaluator.linear_srgb(s.linear));
        light += out[0] - s.lab[0];
        base += s.lab[0];
        chroma_in += s.lab[1].hypot(s.lab[2]);
        chroma_out += out[1].hypot(out[2]);
        let source = axis_at(&grade.cast_in, s.lab[0]);
        let hue = (s.lab[2] - source[1]).atan2(s.lab[1] - source[0]);
        let p = (hue / SECTOR_WIDTH).rem_euclid(SECTORS as f32);
        let i = (p as usize).min(SECTORS - 1);
        let t = p - i as f32;
        if let Some(w) = weight.get_mut(i) {
            *w += 1.0 - t;
        }
        if let Some(w) = weight.get_mut((i + 1) % SECTORS) {
            *w += t;
        }
    }
    let n = skin.len().max(1) as f32;
    SkinEffect {
        light: light / n,
        ratio: if chroma_in > 1e-6 {
            chroma_out / chroma_in
        } else {
            1.0
        },
        base: base / n,
        weight,
    }
}

/// A linear channel at or above this reads as 255 in an 8-bit sRGB render.
const CLIPPED: f32 = 0.995;
/// New clipping the fit may add on its own samples before it pulls the look back. Below the
/// full-resolution check's one per cent, so the check confirms rather than cuts.
const CLIP_ALLOWANCE: f32 = 0.004;

/// Share of samples with any channel clipped; of those, the share clipped in all three (a
/// white, rather than a colour pushed to the edge of the display); and how the newly clipped
/// colours spread over the renderer's hue sectors.
fn clipping(grade: &ReferenceGrade, target: &[Sample]) -> (f32, f32, Vec<f32>) {
    let evaluator = Evaluator::new(grade);
    let (mut any, mut white) = (0_usize, 0_usize);
    let mut sectors = vec![0.0_f32; SECTORS];
    for s in target {
        let out = evaluator.linear_srgb(s.linear);
        if out.iter().any(|v| *v >= CLIPPED) {
            any += 1;
            if out.iter().all(|v| *v >= CLIPPED) {
                white += 1;
            } else if !s.linear.iter().any(|v| *v >= CLIPPED) {
                let source = axis_at(&grade.cast_in, s.lab[0]);
                let hue = (s.lab[2] - source[1]).atan2(s.lab[1] - source[0]);
                let p = (hue / SECTOR_WIDTH).rem_euclid(SECTORS as f32);
                let i = (p as usize).min(SECTORS - 1);
                let t = p - i as f32;
                if let Some(w) = sectors.get_mut(i) {
                    *w += 1.0 - t;
                }
                if let Some(w) = sectors.get_mut((i + 1) % SECTORS) {
                    *w += t;
                }
            }
        }
    }
    let n = target.len().max(1) as f32;
    (
        any as f32 / n,
        if any == 0 {
            0.0
        } else {
            white as f32 / any as f32
        },
        sectors,
    )
}

fn original_clipping(target: &[Sample]) -> f32 {
    let n = target.len().max(1) as f32;
    target
        .iter()
        .filter(|s| s.linear.iter().any(|v| *v >= CLIPPED))
        .count() as f32
        / n
}

/// Keep the look from blowing highlights: when it clips more of this photograph than the
/// photograph already had, pull the top of the tone curve back toward the original where the
/// new clipping is white, and calm colour in the bright zones where colours hit the edge.
fn keep_highlights(grade: &mut ReferenceGrade, target: &[Sample]) {
    let allowed = original_clipping(target) + CLIP_ALLOWANCE;
    for _ in 0..16 {
        let (clipped, white, sectors) = clipping(grade, target);
        if clipped <= allowed {
            return;
        }
        ease_highlights(grade, white, &sectors);
    }
}

/// One round of easing the look where it clips. `white` is the share of the new clipping
/// that is white rather than a colour pushed to the edge of the display; `sectors` is how the
/// newly clipped colours spread over the hue sectors (all zero when unknown).
pub(in crate::color_match) fn ease_highlights(
    grade: &mut ReferenceGrade,
    white: f32,
    sectors: &[f32],
) {
    // Bright pixels clip when the curve lifts them: ease the top of the tone curve back toward
    // the original, harder for whites.
    let pull = if white >= 0.5 { 0.66 } else { 0.8 };
    let nodes: Vec<f32> = grade
        .tone
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = i as f32 / (TONE_NODES - 1) as f32;
            let weight = ((x - 0.5) / 0.5).clamp(0.0, 1.0);
            let toward = if *v > x { x + (v - x) * pull } else { *v };
            v + weight * (toward - v)
        })
        .collect();
    grade.tone = regularise_tone(nodes);
    if white < 0.5 {
        // The overall colourfulness curve, where it adds colour.
        let last = grade.chroma.len().saturating_sub(1).max(1);
        for (j, value) in grade.chroma.iter_mut().enumerate() {
            let input = CHROMA_SPAN * j as f32 / last as f32;
            if *value > input {
                *value = input + (*value - input) * 0.85;
            }
        }
        let last = grade.zone_gain.len().saturating_sub(1).max(1);
        for (z, gain) in grade.zone_gain.iter_mut().enumerate() {
            if z as f32 / last as f32 >= 0.5 {
                *gain *= 0.88;
            }
        }
        for gain in &mut grade.hue_gain {
            if *gain > 1.0 {
                *gain = 1.0 + (*gain - 1.0) * 0.8;
            }
        }
        // The hues that are clipping lose colour and any lift they were given, in proportion
        // to how much of the clipping they hold.
        let heaviest = sectors.iter().copied().fold(0.0_f32, f32::max);
        if heaviest > 0.0 {
            for ((gain, light), w) in grade
                .hue_gain
                .iter_mut()
                .zip(grade.hue_light.iter_mut())
                .zip(sectors)
            {
                let share = w / heaviest;
                *gain *= 1.0 - 0.15 * share;
                if *light > 0.0 {
                    *light *= 1.0 - 0.4 * share;
                }
            }
        }
    }
    // A tint in the highlights pushes one channel past white even where colour is calm, and
    // near-white sky is where that happens first: ease the target axis toward neutral at the top.
    let last = grade.cast_out.len().saturating_sub(1).max(1);
    for (j, out) in grade.cast_out.iter_mut().enumerate() {
        let weight = ((j as f32 / last as f32 - 0.55) / 0.45).clamp(0.0, 1.0);
        let keep = 1.0 - 0.25 * weight;
        out[0] *= keep;
        out[1] *= keep;
    }
    *grade = grade.clone().bounded();
}

/// New clipping between two renders of the same frame: the share of pixels clipped after but
/// not before, the share of those that are white, and their spread over the hue sectors.
pub(in crate::color_match) fn new_clipping(
    grade: &ReferenceGrade,
    before: &Rgb8,
    after: &Rgb8,
) -> (f32, f32, Vec<f32>) {
    let (mut fresh, mut white) = (0_usize, 0_usize);
    let mut sectors = vec![0.0_f32; SECTORS];
    for (b, a) in before.data.chunks_exact(3).zip(after.data.chunks_exact(3)) {
        if !a.contains(&255) || b.contains(&255) {
            continue;
        }
        fresh += 1;
        if a.iter().all(|v| *v == 255) {
            white += 1;
            continue;
        }
        let Ok(pixel) = <[u8; 3]>::try_from(b) else {
            continue;
        };
        let lab = oklab_from_linear_srgb(pixel.map(srgb_byte_to_linear));
        let source = axis_at(&grade.cast_in, lab[0]);
        let hue = (lab[2] - source[1]).atan2(lab[1] - source[0]);
        let p = (hue / SECTOR_WIDTH).rem_euclid(SECTORS as f32);
        let i = (p as usize).min(SECTORS - 1);
        let t = p - i as f32;
        if let Some(w) = sectors.get_mut(i) {
            *w += 1.0 - t;
        }
        if let Some(w) = sectors.get_mut((i + 1) % SECTORS) {
            *w += t;
        }
    }
    let n = (before.data.len() / 3).max(1) as f32;
    (
        fresh as f32 / n,
        if fresh == 0 {
            0.0
        } else {
            white as f32 / fresh as f32
        },
        sectors,
    )
}

/// Skin to the look guard, for a caller that has re-rendered and eased highlights.
pub(in crate::color_match) fn settle_skin(
    grade: &mut ReferenceGrade,
    image: &Rgb8,
    regions: &[Region],
) -> bool {
    satisfy_skin(grade, image, regions)
}

/// Correct the look on this person's skin: a local tone-curve correction around the skin's
/// lightness, and a colour gain on the hue sectors the skin occupies.
fn keep_skin(grade: &mut ReferenceGrade, skin: &[Sample]) {
    if skin.len() < 32 {
        return;
    }
    for _ in 0..6 {
        let effect = skin_effect(grade, skin);
        let excess = effect.light - effect.light.clamp(-SKIN_LIGHT_LIMIT, SKIN_LIGHT_LIMIT);
        let wanted = effect.ratio.clamp(SKIN_CHROMA_FLOOR, SKIN_CHROMA_CEILING);
        if excess.abs() < 0.004 && (wanted - effect.ratio).abs() < 0.02 {
            return;
        }
        if excess.abs() >= 0.004 {
            let nodes: Vec<f32> = grade
                .tone
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let x = i as f32 / (TONE_NODES - 1) as f32;
                    v - excess * (-((x - effect.base) / SKIN_TONE_WIDTH).powi(2)).exp()
                })
                .collect();
            grade.tone = regularise_tone(nodes);
        }
        if (wanted - effect.ratio).abs() >= 0.02 {
            let heaviest = effect
                .weight
                .iter()
                .copied()
                .fold(0.0_f32, f32::max)
                .max(1e-6);
            let factor = (wanted / effect.ratio.max(1e-3)).clamp(0.5, 2.0);
            for (gain, w) in grade.hue_gain.iter_mut().zip(&effect.weight) {
                *gain *= factor.powf(w / heaviest);
            }
        }
        *grade = grade.clone().bounded();
    }
}

/// How strongly each part of the reference is followed. `default()` is the shipped look fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::color_match) struct LookOptions {
    /// Share of the reference's tone shape (spread around middle grey) taken.
    pub tone_shape: f32,
    /// Share of the difference in middle grey taken, before the bound below.
    pub brightness: f32,
    /// Largest move of this photograph's middle grey, in `OKLab` lightness.
    pub max_brightness: f32,
    /// Share of the reference's chroma distribution taken (the rest is the shared-colour ratio).
    pub chroma: f32,
    /// Trust given to per-hue differences.
    pub hue: f32,
    /// Share of the difference in overall tint (the average colour of everything that is not
    /// strongly coloured, per shadows, midtones and highlights) taken per iteration.
    pub tint: f32,
    /// Colours more saturated than this are objects (a red sari, a green lawn), not the grade,
    /// when reading the overall tint.
    pub tint_chroma: f32,
    /// How far colours the reference does not hold are turned toward its nearest dominant
    /// colour (greens toward gold under a golden look), as a share of the turn.
    pub palette: f32,
    pub iterations: usize,
}

impl Default for LookOptions {
    fn default() -> Self {
        Self {
            tone_shape: 0.7,
            brightness: 0.35,
            max_brightness: 0.06,
            chroma: 0.5,
            hue: 0.6,
            tint: 0.6,
            tint_chroma: 0.12,
            palette: 0.7,
            iterations: 3,
        }
        .tuned()
    }
}

impl LookOptions {
    /// Experiments only: `AURA_LOOK_OPTIONS="tone_shape=0.9,tint_chroma=0.2"` in a test build.
    #[cfg(test)]
    fn tuned(mut self) -> Self {
        let Ok(spec) = std::env::var("AURA_LOOK_OPTIONS") else {
            return self;
        };
        for pair in spec.split(',') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            let Ok(value) = value.trim().parse::<f32>() else {
                continue;
            };
            match key.trim() {
                "tone_shape" => self.tone_shape = value,
                "brightness" => self.brightness = value,
                "max_brightness" => self.max_brightness = value,
                "chroma" => self.chroma = value,
                "hue" => self.hue = value,
                "tint" => self.tint = value,
                "tint_chroma" => self.tint_chroma = value,
                "palette" => self.palette = value,
                _ => {}
            }
        }
        self
    }

    #[cfg(not(test))]
    const fn tuned(self) -> Self {
        self
    }
}

const MIDDLE: usize = LIGHT_QUANTILES / 2;
/// A reference colour family this large is part of its palette.
const DOMINANT_SHARE: f32 = 0.04;
/// A photo's colour family is "missing" from the reference below this share of its own.
const MISSING_RATIO: f32 = 0.35;
/// The largest re-hue, radians (about 34 degrees), and the farthest palette colour considered.
const MAX_PULL: f32 = 0.6;
const FARTHEST_PULL: f32 = 1.6;
/// Colour families at least this colourful keep their hue: a wedding red, a jewel tone. Foliage
/// and backdrops sit below it.
const CALM_CHROMA: (f32, f32) = (0.05, 0.1);

/// Turn the colours this photograph holds and the reference does not toward the reference's
/// nearest dominant colour: under a golden look, foliage greens go olive and gold instead of
/// grey. Strongly saturated objects keep their hue (a red sari stays red), and the amount
/// falls as the reference holds more of the colour itself. Applied once, from the photograph's
/// own colours, so it cannot compound.
fn pull_palette(grade: &mut ReferenceGrade, reference: &GradeStats, own: &GradeStats, share: f32) {
    if share <= 0.0 {
        return;
    }
    let dominant: Vec<f32> = (0..SECTORS)
        .filter(|&j| at(&reference.sector_share, j) >= DOMINANT_SHARE)
        .map(|j| j as f32 * SECTOR_WIDTH + at(&reference.sector_hue, j))
        .collect();
    if dominant.is_empty() {
        return;
    }
    for k in 0..SECTORS {
        let mine = at(&own.sector_share, k);
        let theirs = at(&reference.sector_share, k);
        if mine < 0.02 || theirs >= MISSING_RATIO * mine {
            continue;
        }
        let hue = k as f32 * SECTOR_WIDTH + at(&own.sector_hue, k);
        let Some(delta) = dominant
            .iter()
            .map(|d| super::wrap(d - hue))
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
        else {
            continue;
        };
        if delta.abs() > FARTHEST_PULL {
            continue;
        }
        let missing = 1.0 - theirs / (MISSING_RATIO * mine);
        let calm = 1.0 - super::smoothstep(CALM_CHROMA.0, CALM_CHROMA.1, at(&own.sector_chroma, k));
        if let Some(shift) = grade.hue_shift.get_mut(k) {
            *shift += share * missing * calm * delta.clamp(-MAX_PULL, MAX_PULL);
        }
    }
}
/// Centres of the shadow, midtone and highlight zones the overall tint is read in.
const TINT_ZONES: [f32; 3] = [0.25, 0.55, 0.85];
/// Largest correction of the neutral axis from overall tint in one iteration.
const TINT_STEP: f32 = 0.03;

/// The reference as `OKLab` colours, from its stored 32 x 32 sRGB thumbnail.
pub(in crate::color_match) fn palette(rgb: &[u8]) -> Vec<[f32; 3]> {
    rgb.chunks_exact(3)
        .filter_map(|p| <[u8; 3]>::try_from(p).ok())
        .map(|p| oklab_from_linear_srgb(p.map(srgb_byte_to_linear)))
        .collect()
}

/// Mean `[a, b]` of the not-strongly-coloured pixels in each tint zone, with its weight.
fn zone_tints(labs: &[[f32; 3]], ceiling: f32) -> [([f32; 2], f32); 3] {
    let mut out = [([0.0_f32; 2], 0.0_f32); 3];
    for lab in labs {
        if lab[1].hypot(lab[2]) > ceiling {
            continue;
        }
        for (zone, centre) in out.iter_mut().zip(TINT_ZONES) {
            let w = (1.0 - (lab[0] - centre).abs() / 0.3).max(0.0);
            zone.0[0] += w * lab[1];
            zone.0[1] += w * lab[2];
            zone.1 += w;
        }
    }
    for zone in &mut out {
        if zone.1 > 1e-6 {
            zone.0 = [zone.0[0] / zone.1, zone.0[1] / zone.1];
        }
    }
    out
}

/// Move the neutral axis so the photograph's overall tint goes toward the reference's.
fn follow_tint(
    grade: &mut ReferenceGrade,
    palette: &[[f32; 3]],
    target: &[Sample],
    share: f32,
    ceiling: f32,
) {
    if palette.is_empty() || share <= 0.0 {
        return;
    }
    let wanted = zone_tints(palette, ceiling);
    let now = zone_tints(&transformed(grade, target), ceiling);
    let last = grade.cast_out.len().saturating_sub(1).max(1);
    for (j, out) in grade.cast_out.iter_mut().enumerate() {
        let x = j as f32 / last as f32;
        let (mut d, mut mass) = ([0.0_f32; 2], 0.0_f32);
        for (k, ((r, rw), (n, nw))) in wanted.iter().zip(&now).enumerate() {
            if *rw < 1.0 || *nw < 1.0 {
                continue;
            }
            let centre = TINT_ZONES.get(k).copied().unwrap_or(0.5);
            let w = (1.0 - (x - centre).abs() / 0.35).max(0.0);
            d[0] += w * (r[0] - n[0]);
            d[1] += w * (r[1] - n[1]);
            mass += w;
        }
        if mass > 1e-6 {
            let step = [share * d[0] / mass, share * d[1] / mass];
            let size = step[0].hypot(step[1]);
            let scale = if size > TINT_STEP {
                TINT_STEP / size
            } else {
                1.0
            };
            out[0] += step[0] * scale;
            out[1] += step[1] * scale;
        }
    }
}

/// Target lightness quantiles carrying the reference's tone shape around this photo's middle.
fn anchored_tone(own: &[f32], reference: &[f32], options: &LookOptions) -> Vec<f32> {
    let (own_mid, reference_mid) = (at(own, MIDDLE), at(reference, MIDDLE));
    let shift = (options.brightness * (reference_mid - own_mid))
        .clamp(-options.max_brightness, options.max_brightness);
    let wanted: Vec<f32> = (0..LIGHT_QUANTILES)
        .map(|i| {
            let shaped = own_mid + (at(reference, i) - reference_mid);
            let mixed = at(own, i) + options.tone_shape * (shaped - at(own, i));
            (mixed + shift).clamp(0.0, 1.0)
        })
        .collect();
    // Quantiles must rise for the map; keep each at least its predecessor.
    let mut floor = 0.0_f32;
    wanted
        .into_iter()
        .map(|v| {
            floor = v.max(floor);
            floor
        })
        .collect()
}

fn tone_nodes(own: &[f32], wanted: &[f32]) -> Vec<f32> {
    let nodes = (0..TONE_NODES)
        .map(|i| {
            let x = i as f32 / (TONE_NODES - 1) as f32;
            quantile_map(own, wanted, x, 1.0, (0.3, 2.0))
        })
        .collect();
    regularise_tone(nodes)
}

/// Fit a transform that makes `target` take on the reference's look.
pub(in crate::color_match) fn fit_look(
    reference: &GradeStats,
    target: &[Sample],
    skin: &[Sample],
    palette: &[[f32; 3]],
    options: &LookOptions,
    mut checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<Fit> {
    checkpoint()?;
    let labs: Vec<[f32; 3]> = target.iter().map(|s| s.lab).collect();
    let start = measure(&labs);
    let before = distance(reference, &start);
    let identity = ReferenceGrade::identity();
    if target.len() < MIN_SAMPLES || (reference.samples as usize) < MIN_SAMPLES {
        return Ok(Fit {
            grade: identity,
            before,
            after: before,
            iterations: 0,
            content: 0.0,
        });
    }
    let keep_target_grey = start.achromatic && at(&reference.chroma, CHROMA_QUANTILES - 2) > 0.02;
    let monochrome = reference.achromatic || keep_target_grey;
    let mut grade = identity.clone();
    grade.cast_in.clone_from(&start.axis);
    if !monochrome {
        grade.cast_out.clone_from(&reference.axis);
    }
    grade.tone = tone_nodes(
        &start.lightness,
        &anchored_tone(&start.lightness, &reference.lightness, options),
    );
    if monochrome {
        grade.chroma.fill(0.0);
    }
    grade = grade.bounded();
    for _ in 0..options.iterations {
        checkpoint()?;
        if monochrome {
            break;
        }
        let now = measure(&transformed(&grade, target));
        for (out, (r, a)) in grade
            .cast_out
            .iter_mut()
            .zip(reference.axis.iter().zip(&now.axis))
        {
            out[0] += 0.8 * (r[0] - a[0]);
            out[1] += 0.8 * (r[1] - a[1]);
        }
        follow_tint(
            &mut grade,
            palette,
            target,
            options.tint,
            options.tint_chroma,
        );
        grade = grade.bounded();
        let now = measure(&transformed(&grade, target));
        let matched = chroma_curve(&now.chroma, &reference.chroma, &grade.chroma);
        let shared = shared_chroma_scale(reference, &now);
        for j in 0..CHROMA_NODES {
            let (Some(current), Some(fitted)) = (grade.chroma.get(j).copied(), matched.get(j))
            else {
                continue;
            };
            let blended = options.chroma * fitted + (1.0 - options.chroma) * current * shared;
            if let Some(v) = grade.chroma.get_mut(j) {
                *v = blended;
            }
        }
        // Keep the chroma curve rising from zero.
        let mut floor = 0.0_f32;
        for v in &mut grade.chroma {
            *v = v.max(floor);
            floor = *v;
        }
        if let Some(first) = grade.chroma.first_mut() {
            *first = 0.0;
        }
        grade = grade.bounded();
        let now = measure(&transformed(&grade, target));
        update_sectors(&mut grade, reference, &now, options.hue);
        grade = grade.bounded();
    }
    checkpoint()?;
    if !monochrome {
        pull_palette(&mut grade, reference, &start, options.palette);
        grade = grade.bounded();
        // The renderer's own default skin protection, which the skin correction measures through.
        grade.skin = 0.5;
        keep_highlights(&mut grade, target);
        keep_skin(&mut grade, skin);
        // The skin correction can lift a bright face; check the highlights once more.
        keep_highlights(&mut grade, target);
    }
    let after = distance(reference, &measure(&transformed(&grade, target)));
    Ok(Fit {
        grade,
        before,
        after,
        iterations: options.iterations,
        content: 0.0,
    })
}

/// The look guard's limits, tightened so a fit that passes here passes the full-resolution
/// check, which reads a larger picture through the whole renderer.
const GUARD_TONE: f32 = 0.045;
const GUARD_CHROMA: (f32, f32) = (0.8, 1.35);
const GUARD_MID_TURN: f32 = 12.0;
const GUARD_HIGH_TURN: f32 = 16.0;

/// The picture as the grade renders it, at the picture's own size.
fn through(grade: &ReferenceGrade, image: &Rgb8) -> Rgb8 {
    let evaluator = Evaluator::new(grade);
    let mut rgb = Vec::with_capacity(image.data.len());
    for p in image.data.chunks_exact(3) {
        let Ok(p) = <[u8; 3]>::try_from(p) else {
            continue;
        };
        let out = evaluator.linear_srgb(p.map(srgb_byte_to_linear));
        rgb.extend(out.map(|v| {
            (aura_raw::colour::curve::srgb_encode(v) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        }));
    }
    Rgb8 {
        width: image.width,
        height: image.height,
        data: rgb,
    }
}

fn hue_turn(p: aura_core::contract::look::ZoneTint, q: aura_core::contract::look::ZoneTint) -> f32 {
    if p.a.hypot(p.b) < 3.0 || q.a.hypot(q.b) < 3.0 {
        return 0.0;
    }
    let d = q.b.atan2(q.a) - p.b.atan2(p.a);
    ((d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI)
        .abs()
        .to_degrees()
}

/// What to change for the worst skin region, if any region is outside the tightened guard.
enum SkinFix {
    Chroma {
        region: usize,
        factor: f32,
        lightness: f32,
    },
    Tone {
        lightness: f32,
        excess: f32,
    },
    Turn,
}

fn worst_skin(before: &[Region], after: &[Region]) -> Option<SkinFix> {
    let mut worst: Option<(f32, SkinFix)> = None;
    let mut consider = |severity: f32, fix: SkinFix| {
        if severity > 0.0 && worst.as_ref().is_none_or(|(s, _)| severity > *s) {
            worst = Some((severity, fix));
        }
    };
    for (index, a) in before
        .iter()
        .enumerate()
        .filter(|(_, r)| r.name.ends_with("skin"))
    {
        let Some(b) = after.iter().find(|b| b.name == a.name) else {
            continue;
        };
        let (x, y) = (&a.appearance, &b.appearance);
        if x.chroma_p50 > 1e-4 {
            let ratio = y.chroma_p50 / x.chroma_p50;
            let wanted = ratio.clamp(GUARD_CHROMA.0, GUARD_CHROMA.1);
            consider(
                (wanted / ratio).ln().abs() * 4.0,
                SkinFix::Chroma {
                    region: index,
                    factor: wanted / ratio.max(1e-3),
                    lightness: x.tone.p50,
                },
            );
        }
        let tone = y.tone.p50 - x.tone.p50;
        consider(
            (tone.abs() - GUARD_TONE) * 20.0,
            SkinFix::Tone {
                lightness: x.tone.p50,
                excess: tone - tone.clamp(-GUARD_TONE, GUARD_TONE),
            },
        );
        let turn = (hue_turn(x.mid, y.mid) - GUARD_MID_TURN)
            .max(hue_turn(x.high, y.high) - GUARD_HIGH_TURN);
        consider(turn / 10.0, SkinFix::Turn);
    }
    worst.map(|(_, fix)| fix)
}

/// Sector weights of one region's pixels, measured from the photograph's own neutral axis.
#[allow(clippy::many_single_char_names)] // r, g, b and the sector's p, i, t
fn region_sectors(grade: &ReferenceGrade, image: &Rgb8, region: &Region) -> Vec<f32> {
    let mut weight = vec![0.0_f32; SECTORS];
    for &[x, y] in &region.points {
        let column =
            ((x * image.width as f32) as usize).min(image.width.saturating_sub(1) as usize);
        let row = ((y * image.height as f32) as usize).min(image.height.saturating_sub(1) as usize);
        let at = (row * image.width as usize + column) * 3;
        let Some(&[r, g, b]) = image
            .data
            .get(at..at + 3)
            .and_then(|p| <&[u8; 3]>::try_from(p).ok())
        else {
            continue;
        };
        let lab = oklab_from_linear_srgb([r, g, b].map(srgb_byte_to_linear));
        let source = axis_at(&grade.cast_in, lab[0]);
        let hue = (lab[2] - source[1]).atan2(lab[1] - source[0]);
        let p = (hue / SECTOR_WIDTH).rem_euclid(SECTORS as f32);
        let i = (p as usize).min(SECTORS - 1);
        let t = p - i as f32;
        if let Some(w) = weight.get_mut(i) {
            *w += 1.0 - t;
        }
        if let Some(w) = weight.get_mut((i + 1) % SECTORS) {
            *w += t;
        }
    }
    weight
}

/// Correct the look until every person's skin passes the look guard, read with the guard's own
/// instrument on the 512 px picture: colourfulness through the hue sectors that person's skin
/// occupies, brightness through a local tone correction, and a hue turn through the renderer's
/// skin protection. Returns whether it got there.
fn satisfy_skin(grade: &mut ReferenceGrade, image: &Rgb8, regions: &[Region]) -> bool {
    let before = crate::color_match::appearance::readings(image, regions);
    for _ in 0..20 {
        let after = crate::color_match::appearance::readings(&through(grade, image), regions);
        let Some(fix) = worst_skin(&before, &after) else {
            return true;
        };
        match fix {
            SkinFix::Chroma {
                region,
                factor,
                lightness,
            } => {
                let Some(region) = regions.get(region) else {
                    return false;
                };
                if factor > 1.0 {
                    // Skin losing colour under a cool or neutralising look: the balance shift
                    // runs against the skin's own hue. Ease it back at the skin's lightness, so
                    // the look stays in the shadows, highlights and background.
                    let last = grade.cast_out.len().saturating_sub(1).max(1);
                    for (j, (out, source)) in
                        grade.cast_out.iter_mut().zip(&grade.cast_in).enumerate()
                    {
                        let x = j as f32 / last as f32;
                        let weight = 0.35 * (-((x - lightness) / 0.22).powi(2)).exp();
                        out[0] += weight * (source[0] - out[0]);
                        out[1] += weight * (source[1] - out[1]);
                    }
                }
                let weight = region_sectors(grade, image, region);
                let heaviest = weight.iter().copied().fold(0.0_f32, f32::max).max(1e-6);
                let step = factor.clamp(0.6, 1.6);
                for (gain, w) in grade.hue_gain.iter_mut().zip(&weight) {
                    *gain *= step.powf(w / heaviest);
                }
            }
            SkinFix::Tone { lightness, excess } => {
                let nodes: Vec<f32> = grade
                    .tone
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let x = i as f32 / (TONE_NODES - 1) as f32;
                        v - excess * (-((x - lightness) / SKIN_TONE_WIDTH).powi(2)).exp()
                    })
                    .collect();
                grade.tone = regularise_tone(nodes);
            }
            SkinFix::Turn => {
                if grade.skin >= 1.0 {
                    return false;
                }
                grade.skin = (grade.skin + 0.25).min(1.0);
            }
        }
        *grade = grade.clone().bounded();
    }
    false
}

/// The last step of a look fit, once the photograph's regions are known: skin to the look
/// guard, highlights back inside, and skin again in case the highlight pass moved it.
pub(in crate::color_match) fn settle(
    grade: &mut ReferenceGrade,
    target: &[Sample],
    image: &Rgb8,
    regions: &[Region],
) -> bool {
    satisfy_skin(grade, image, regions);
    keep_highlights(grade, target);
    satisfy_skin(grade, image, regions)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `OKLab` samples at one lightness, chroma and hue (degrees), repeated.
    fn patch(l: f32, chroma: f32, hue: f32, count: usize) -> Vec<[f32; 3]> {
        let h = hue.to_radians();
        vec![[l, chroma * h.cos(), chroma * h.sin()]; count]
    }

    #[test]
    fn a_golden_palette_turns_muted_greens_and_leaves_a_saturated_red() {
        // Reference: warm golds and browns, nothing green or red.
        let mut reference = patch(0.6, 0.09, 75.0, 800);
        reference.extend(patch(0.35, 0.06, 60.0, 400));
        reference.extend(patch(0.5, 0.0, 0.0, 300));
        // Photo: muted foliage greens, a saturated red garment, some neutrals.
        let mut own = patch(0.55, 0.06, 140.0, 600);
        own.extend(patch(0.45, 0.17, 25.0, 400));
        own.extend(patch(0.5, 0.0, 0.0, 300));
        let (reference, own) = (measure(&reference), measure(&own));
        let mut grade = ReferenceGrade::identity();
        grade.cast_in.clone_from(&own.axis);
        pull_palette(&mut grade, &reference, &own, 1.0);
        let sector = |hue: f32| ((hue.to_radians() / SECTOR_WIDTH).round() as usize) % SECTORS;
        let green = grade.hue_shift[sector(140.0)];
        let red = grade.hue_shift[sector(25.0)];
        assert!(green < -0.2, "greens turn toward gold (lower hue): {green}");
        assert!(green >= -MAX_PULL - 1e-6, "bounded: {green}");
        assert!(red.abs() < 0.02, "a saturated red keeps its hue: {red}");
    }

    #[test]
    fn colours_the_reference_holds_are_not_pulled() {
        let reference = measure(&patch(0.55, 0.06, 140.0, 800));
        let own = measure(&patch(0.55, 0.06, 140.0, 800));
        let mut grade = ReferenceGrade::identity();
        pull_palette(&mut grade, &reference, &own, 1.0);
        assert!(grade.hue_shift.iter().all(|v| v.abs() < 1e-6));
    }

    #[test]
    fn new_clipping_counts_only_pixels_that_were_not_clipped_before() {
        let image = |pixels: &[[u8; 3]]| Rgb8 {
            width: pixels.len() as u32,
            height: 1,
            data: pixels.iter().flatten().copied().collect(),
        };
        let before = image(&[
            [255, 255, 255],
            [200, 100, 90],
            [240, 240, 240],
            [10, 10, 10],
        ]);
        let after = image(&[
            [255, 255, 255],
            [255, 120, 100],
            [255, 255, 255],
            [12, 12, 12],
        ]);
        let (fresh, white, sectors) = new_clipping(&ReferenceGrade::identity(), &before, &after);
        assert!(
            (fresh - 0.5).abs() < 1e-6,
            "two of four newly clipped: {fresh}"
        );
        assert!((white - 0.5).abs() < 1e-6, "one of them white: {white}");
        assert!(
            sectors.iter().sum::<f32>() > 0.99,
            "the coloured one is placed in a sector"
        );
    }
}
