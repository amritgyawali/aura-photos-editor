//! Colour and tone transfer from an edited reference, fitted by distribution (ADR-0160).
//!
//! The slider fit this replaces searched bounded recipe parameters through the renderer, so it
//! could only move as far as those parameters reach: no hue rotation, a tone shape of five
//! sliders, a white balance capped at 800 K - and a split tone, a strong grade or a monochrome
//! reference ended farther from the look on a new photograph. This module measures the reference
//! and the photograph as distributions in `OKLab` and fits `aura_recipe::reference_grade`
//! directly:
//!
//! * **tone** - slope-limited lightness quantiles for corresponding scenes; unrelated colour
//!   scenes keep their tone rather than copying the relative area of bright/dark objects;
//! * **white balance and split toning** - each photograph's neutral axis at nine lightnesses,
//!   estimated from its most neutral pixels;
//! * **colourfulness** - chroma quantiles measured from the neutral axis, then the residual per
//!   hue sector and per lightness zone, each normalised so it cannot absorb a global change;
//! * **hue** - per-sector mean hue, bounded and shrunk by the support both photographs share.
//!
//! Corresponding scenes are iteratively re-measured through the renderer's evaluator. Across
//! unrelated scenes only supported casts, shared-colour saturation and bounded hue evidence
//! are used, with shrinkage toward identity. These are estimates of a look, not recovery of an
//! unknown editing operation. Native checks additionally limit clipping and skin changes.
use std::f32::consts::{PI, TAU};

use aura_core::AuraResult;
use aura_raw::codec::Rgb8;
use aura_raw::colour::curve::srgb_byte_to_linear;
use aura_recipe::reference_grade::{
    ReferenceGrade, AXIS_NODES, CHROMA_NODES, CHROMA_SPAN, MAX_CAST, MAX_CHROMA_OUT, SECTORS,
    TONE_NODES, ZONES,
};
use aura_render::reference_grade::{oklab_from_linear_srgb, Evaluator};
use serde::{Deserialize, Serialize};

use super::appearance::Region;
use super::AppState;

/// Bump when [`measure`] changes; a stored measurement of another version is re-taken.
pub(super) const STATS_VERSION: u32 = 3;
const STATS: &str = "colour_reference_grade_v1:";
const REFERENCE_SAMPLING: u32 = 1;
const LIGHT_QUANTILES: usize = 65;
const CHROMA_QUANTILES: usize = 33;
/// A photograph is fitted from this many pixels, sub-sampled on a grid so the distribution is
/// kept rather than averaged away.
pub(super) const TARGET_SAMPLES: usize = 40_000;
const REFERENCE_SAMPLES: usize = 65_536;
const MIN_SAMPLES: usize = 256;
const SAME_SCENE: f32 = 0.8;
/// Authored regularisation toward no edit when object/illumination correspondence is unknown.
/// This is a strength prior, not a calibrated probability of a correct match.
const UNRELATED_SHRINKAGE: f32 = 0.5;
const SECTOR_WIDTH: f32 = TAU / SECTORS as f32;
/// Lightness landmarks, as indices into the 65 lightness quantiles, for the coarse part of tone
/// fitting: 0.2, 5, 15, 30, 50, 70, 85, 95 and 99.8 per cent.
const LANDMARKS: [usize; 9] = [0, 3, 9, 19, 32, 45, 55, 61, 64];

fn at(values: &[f32], index: usize) -> f32 {
    values
        .get(index)
        .or_else(|| values.last())
        .copied()
        .unwrap_or(0.0)
}

fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One pixel of a photograph, as display light and as `OKLab`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Sample {
    linear: [f32; 3],
    lab: [f32; 3],
}

mod sampling;
pub(super) use sampling::samples;
mod neutral;
use neutral::neutral_axis;
mod exact_grade;
mod palette;
mod quantile;
pub(super) use exact_grade::fit_exact;
pub(super) mod tone_prior;
pub use tone_prior::ToneReading;

use quantile::QuantileMap;

/// What a photograph's colour and tone are, as the fit compares them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GradeStats {
    pub version: u32,
    /// Zero means legacy/unspecified pixels; one means native stratified reference import.
    #[serde(default)]
    pub reference_sampling: u32,
    pub samples: u32,
    pub lightness: Vec<f32>,
    pub axis: Vec<[f32; 2]>,
    pub axis_weight: Vec<f32>,
    pub chroma: Vec<f32>,
    pub zone_chroma: Vec<f32>,
    pub zone_weight: Vec<f32>,
    pub sector_share: Vec<f32>,
    pub sector_chroma: Vec<f32>,
    pub sector_hue: Vec<f32>,
    pub sector_light: Vec<f32>,
    pub achromatic: bool,
    /// Compact joint colour evidence. Older v3 measurements are refreshed from their preview.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette_samples: Vec<[f32; 3]>,
    #[serde(default)]
    pub palette_matching: bool,
}

impl GradeStats {
    pub(super) fn usable(&self) -> bool {
        self.version == STATS_VERSION
            && self.reference_sampling <= REFERENCE_SAMPLING
            && self.lightness.len() == LIGHT_QUANTILES
            && self.axis.len() == AXIS_NODES
            && self.axis_weight.len() == AXIS_NODES
            && self.chroma.len() == CHROMA_QUANTILES
            && self.zone_chroma.len() == ZONES
            && self.zone_weight.len() == ZONES
            && [
                &self.sector_share,
                &self.sector_chroma,
                &self.sector_hue,
                &self.sector_light,
            ]
            .iter()
            .all(|v| v.len() == SECTORS)
            && self
                .lightness
                .iter()
                .chain(self.axis.iter().flatten())
                .chain(&self.axis_weight)
                .chain(&self.chroma)
                .chain(&self.zone_chroma)
                .chain(&self.zone_weight)
                .chain(&self.sector_share)
                .chain(&self.sector_chroma)
                .chain(&self.sector_hue)
                .chain(&self.sector_light)
                .chain(self.palette_samples.iter().flatten())
                .all(|v| v.is_finite())
            && self.palette_samples.len() <= 1024
    }
}

fn weighted_quantile(pairs: &mut [(f32, f32)], q: f32) -> Option<f32> {
    // Neutral-axis tails request several ranks from the same array. Preserve that sorted
    // order and avoid sorting it again; unstable sorting needs no scratch allocation.
    if !pairs
        .iter()
        .zip(pairs.iter().skip(1))
        .all(|(a, b)| a.0.total_cmp(&b.0).is_le())
    {
        pairs.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    }
    let total: f32 = pairs.iter().map(|p| p.1).sum();
    if total <= 0.0 {
        return None;
    }
    let goal = q.clamp(0.0, 1.0) * total;
    let mut sum = 0.0;
    for &(value, weight) in pairs.iter() {
        sum += weight;
        if sum >= goal {
            return Some(value);
        }
    }
    pairs.last().map(|p| p.0)
}

fn quantiles(mut values: Vec<f32>, count: usize, top: f32, bottom: f32) -> Vec<f32> {
    values.sort_unstable_by(f32::total_cmp);
    let last = values.len().saturating_sub(1) as f32;
    (0..count)
        .map(|i| {
            let q = bottom + (top - bottom) * i as f32 / (count - 1) as f32;
            let x = q * last;
            let lo = x.floor() as usize;
            let t = x - lo as f32;
            at(&values, lo) + (at(&values, lo + 1) - at(&values, lo)) * t
        })
        .collect()
}

fn axis_at(axis: &[[f32; 2]], lightness: f32) -> [f32; 2] {
    let position = lightness.clamp(0.0, 1.0) * (AXIS_NODES - 1) as f32;
    let index = (position as usize).min(AXIS_NODES - 2);
    let fraction = position - index as f32;
    let left = axis.get(index).copied().unwrap_or([0.0; 2]);
    let right = axis.get(index + 1).copied().unwrap_or(left);
    [
        left[0] + (right[0] - left[0]) * fraction,
        left[1] + (right[1] - left[1]) * fraction,
    ]
}

/// Measure one photograph. The reference is measured once, when it is imported; a photograph
/// being matched is measured on every iteration of the fit.
pub(super) fn measure(labs: &[[f32; 3]]) -> GradeStats {
    let n = labs.len().max(1) as f32;
    let lightness = quantiles(
        labs.iter().map(|l| l[0]).collect(),
        LIGHT_QUANTILES,
        0.998,
        0.002,
    );
    let (axis, axis_weight) = neutral_axis(labs);
    // Chroma and hue measured from the neutral axis, so a cast is never read as saturation.
    let relative: Vec<(f32, f32, f32)> = labs
        .iter()
        .map(|l| {
            let c = axis_at(&axis, l[0]);
            let (a, b) = (l[1] - c[0], l[2] - c[1]);
            (l[0], a.hypot(b), b.atan2(a))
        })
        .collect();
    let chroma = quantiles(
        relative.iter().map(|r| r.1).collect(),
        CHROMA_QUANTILES,
        0.995,
        0.0,
    );
    let mut zone_chroma = vec![0.0; ZONES];
    let mut zone_weight = vec![0.0; ZONES];
    for (z, (median, weight)) in zone_chroma
        .iter_mut()
        .zip(zone_weight.iter_mut())
        .enumerate()
    {
        let centre = z as f32 / (ZONES - 1) as f32;
        let mut pairs: Vec<(f32, f32)> = relative
            .iter()
            .filter_map(|r| {
                let w = 1.0 - (r.0 - centre).abs() * (ZONES - 1) as f32;
                (w > 0.0).then_some((r.1, w))
            })
            .collect();
        *weight = pairs.iter().map(|p| p.1).sum::<f32>() / n;
        *median = weighted_quantile(&mut pairs, 0.5).unwrap_or(0.0);
    }
    let mut sector_share = vec![0.0; SECTORS];
    let mut sector_chroma = vec![0.0; SECTORS];
    let mut sector_hue = vec![0.0; SECTORS];
    let mut sector_light = vec![0.0; SECTORS];
    for (k, (((share, chroma_k), hue_k), light_k)) in sector_share
        .iter_mut()
        .zip(sector_chroma.iter_mut())
        .zip(sector_hue.iter_mut())
        .zip(sector_light.iter_mut())
        .enumerate()
    {
        let centre = k as f32 * SECTOR_WIDTH;
        let members: Vec<(f32, f32, f32, f32)> = relative
            .iter()
            .filter_map(|&(l, c, h)| {
                let offset = wrap(h - centre);
                let w = (1.0 - offset.abs() / SECTOR_WIDTH) * smoothstep(0.012, 0.05, c);
                (w > 0.0).then_some((l, c, offset, w))
            })
            .collect();
        let mass: f32 = members.iter().map(|m| m.3).sum();
        *share = mass / n;
        if mass <= 0.0 {
            continue;
        }
        let mut pairs: Vec<(f32, f32)> = members.iter().map(|m| (m.1, m.3)).collect();
        *chroma_k = weighted_quantile(&mut pairs, 0.5).unwrap_or(0.0);
        let mut pairs: Vec<(f32, f32)> = members.iter().map(|m| (m.0, m.3)).collect();
        *light_k = weighted_quantile(&mut pairs, 0.5).unwrap_or(0.0);
        let (s, c) = members.iter().fold((0.0, 0.0), |(s, c), m| {
            (s + m.2.sin() * m.3, c + m.2.cos() * m.3)
        });
        *hue_k = s.atan2(c);
    }
    // A constant object colour is not monochrome, and a small selective-colour detail
    // must not be lost in a high quantile. Check absolute chroma on every sample.
    let achromatic = !labs.is_empty() && labs.iter().all(|l| l[1].hypot(l[2]) < 0.004);
    GradeStats {
        version: STATS_VERSION,
        reference_sampling: 0,
        samples: labs.len() as u32,
        lightness,
        axis,
        axis_weight,
        chroma,
        zone_chroma,
        zone_weight,
        sector_share,
        sector_chroma,
        sector_hue,
        sector_light,
        achromatic,
        palette_samples: palette::compact(labs),
        palette_matching: false,
    }
}

/// Measure a rendered photograph the way the fit measured it.
pub(super) fn measure_image(image: &Rgb8) -> GradeStats {
    let labs: Vec<[f32; 3]> = samples(image, TARGET_SAMPLES)
        .iter()
        .map(|s| s.lab)
        .collect();
    measure(&labs)
}

pub(super) fn sample_labs(image: &Rgb8, limit: usize) -> Vec<[f32; 3]> {
    samples(image, limit)
        .iter()
        .map(|sample| sample.lab)
        .collect()
}

/// How far two photographs' colour and tone are apart, in `OKLab` units times a hundred - close
/// to CIELAB Î”E in size. Four terms, so a report can say which part of a look is missing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Distance {
    pub tone: f32,
    pub cast: f32,
    pub chroma: f32,
    pub hue: f32,
    #[serde(default)]
    pub palette: f32,
    pub total: f32,
}

pub(super) fn distance(reference: &GradeStats, actual: &GradeStats) -> Distance {
    let mean_abs = |a: &[f32], b: &[f32]| {
        a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len().max(1) as f32
    };
    let tone = 100.0 * mean_abs(&reference.lightness, &actual.lightness);
    let (mut cast, mut mass) = (0.0, 0.0);
    for ((r, a), (wr, wa)) in reference
        .axis
        .iter()
        .zip(&actual.axis)
        .zip(reference.axis_weight.iter().zip(&actual.axis_weight))
    {
        let w = 0.1 + wr.min(*wa);
        cast += w * (r[0] - a[0]).hypot(r[1] - a[1]);
        mass += w;
    }
    let cast = 100.0 * cast / f32::max(mass, 1e-6);
    let chroma = 100.0 * mean_abs(&reference.chroma, &actual.chroma);
    let (mut hue, mut support) = (0.0, 0.0);
    for k in 0..SECTORS {
        let s = at(&reference.sector_share, k).min(at(&actual.sector_share, k));
        let (rc, ac) = (
            at(&reference.sector_chroma, k),
            at(&actual.sector_chroma, k),
        );
        let turn = wrap(at(&reference.sector_hue, k) - at(&actual.sector_hue, k)).abs();
        hue += s * ((rc - ac).abs() + 0.5 * (rc + ac) * turn);
        support += s;
    }
    let hue = if support > 1e-4 {
        100.0 * hue / support
    } else {
        0.0
    };
    let palette = if reference.palette_matching {
        palette::distance(&reference.palette_samples, &actual.palette_samples)
    } else {
        0.0
    };
    Distance {
        tone,
        cast,
        chroma,
        hue,
        palette,
        total: if reference.palette_matching {
            (tone + cast + chroma + hue + 4.0 * palette) / 8.0
        } else {
            (tone + cast + chroma + hue) / 4.0
        },
    }
}

/// Map `x` through the quantile function: where `x` sits among `from`, the same place in `to`.
/// Runs of equal quantiles - a clipped sky, a black border - map to the mean of their run.
fn quantile_map(from: &[f32], to: &[f32], x: f32, ceiling: f32, slopes: (f32, f32)) -> f32 {
    QuantileMap::new(from, to, ceiling, slopes).apply(x)
}

/// Evaluate a curve stored as evenly spaced nodes over `0..=span`, linearly.
fn nodes_at(nodes: &[f32], span: f32, x: f32) -> f32 {
    let last = nodes.len().saturating_sub(1);
    if last == 0 {
        return x;
    }
    let p = (x / span).clamp(0.0, 1.0) * last as f32;
    let i = (p as usize).min(last - 1);
    at(nodes, i) + (at(nodes, i + 1) - at(nodes, i)) * (p - i as f32)
}

const MIN_TONE_SLOPE: f32 = 0.05;
const MAX_TONE_SLOPE: f32 = 4.0;
const MAX_TONE_DEVIATION: f32 = 0.35;

/// Keep a tone curve smooth, rising, within a bounded distance of the identity, and with no
/// segment flat enough to posterise or steep enough to tear a gradient.
fn regularise_tone(nodes: Vec<f32>) -> Vec<f32> {
    regularise_tone_within(nodes, MAX_TONE_DEVIATION, MAX_TONE_SLOPE)
}

/// [`regularise_tone`] with stated bounds on the distance from the identity and on the slope.
fn regularise_tone_within(nodes: Vec<f32>, deviation: f32, max_slope: f32) -> Vec<f32> {
    let last = TONE_NODES - 1;
    let h = 1.0 / last as f32;
    let mut y: Vec<f32> = (0..TONE_NODES)
        .map(|i| {
            let x = i as f32 * h;
            at(&nodes, i).clamp((x - deviation).max(0.0), (x + deviation).min(1.0))
        })
        .collect();
    let mut floor = 0.0_f32;
    for v in &mut y {
        *v = v.max(floor);
        floor = *v;
    }
    let mut start = at(&y, 0);
    let mut end = at(&y, last);
    if end - start < MIN_TONE_SLOPE {
        end = (start + MIN_TONE_SLOPE).min(1.0);
        start = end - MIN_TONE_SLOPE;
    }
    let total = end - start;
    let raw: Vec<f32> = y.iter().zip(y.iter().skip(1)).map(|(a, b)| b - a).collect();
    // One [1, 2, 1] pass over the increments removes quantile noise and keeps their sum.
    let mut inc: Vec<f32> = (0..last)
        .map(|i| {
            let prev = if i == 0 { at(&raw, 0) } else { at(&raw, i - 1) };
            let next = if i + 1 == last {
                at(&raw, i)
            } else {
                at(&raw, i + 1)
            };
            0.25 * prev + 0.5 * at(&raw, i) + 0.25 * next
        })
        .collect();
    let (lo, hi) = (MIN_TONE_SLOPE * h, max_slope * h);
    for _ in 0..12 {
        for v in &mut inc {
            *v = v.clamp(lo, hi);
        }
        let diff = total - inc.iter().sum::<f32>();
        if diff.abs() < 1e-7 {
            break;
        }
        let slack: Vec<f32> = inc
            .iter()
            .map(|v| if diff > 0.0 { hi - v } else { v - lo })
            .collect();
        let room: f32 = slack.iter().sum();
        if room <= 1e-9 {
            break;
        }
        for (v, s) in inc.iter_mut().zip(&slack) {
            *v += diff * s / room;
        }
    }
    let mut out = Vec::with_capacity(TONE_NODES);
    let mut value = start;
    out.push(value);
    for v in inc {
        value = (value + v).clamp(0.0, 1.0);
        out.push(value);
    }
    out
}

fn tone_curve(from: &[f32], to: &[f32], detail: f32) -> Vec<f32> {
    let pick = |values: &[f32]| LANDMARKS.iter().map(|&i| at(values, i)).collect::<Vec<_>>();
    let (land_from, land_to) = (pick(from), pick(to));
    let nodes = (0..TONE_NODES)
        .map(|i| {
            let x = i as f32 / (TONE_NODES - 1) as f32;
            let full = quantile_map(from, to, x, 1.0, (0.3, 1.5));
            let landmark = quantile_map(&land_from, &land_to, x, 1.0, (0.3, 1.5));
            detail * full + (1.0 - detail) * landmark
        })
        .collect();
    regularise_tone(nodes)
}

/// Compose a chroma correction onto the current chroma curve. Near-neutral pixels can gain at
/// most three times their chroma plus a trace, so the noise in a grey wall never becomes colour.
fn chroma_curve(from: &[f32], to: &[f32], current: &[f32]) -> Vec<f32> {
    let step = CHROMA_SPAN / (CHROMA_NODES - 1) as f32;
    let mut floor = 0.0_f32;
    (0..CHROMA_NODES)
        .map(|j| {
            if j == 0 {
                return 0.0;
            }
            let input = j as f32 * step;
            let mapped = quantile_map(from, to, at(current, j), MAX_CHROMA_OUT, (0.3, 2.0));
            let value = mapped
                .min(3.0 * input + 0.005)
                .min(floor + 3.0 * step)
                .clamp(floor, MAX_CHROMA_OUT);
            floor = value;
            value
        })
        .collect()
}

fn smooth_circular(values: &[f32]) -> Vec<f32> {
    let n = values.len();
    (0..n)
        .map(|k| {
            0.2 * at(values, (k + n - 1) % n.max(1))
                + 0.6 * at(values, k)
                + 0.2 * at(values, (k + 1) % n.max(1))
        })
        .collect()
}

/// Per-hue residuals, normalised by their shared-support mean so a sector cannot absorb a change
/// the chroma curve already made, and shrunk by how much of both photographs the sector holds.
fn update_sectors(
    grade: &mut ReferenceGrade,
    reference: &GradeStats,
    actual: &GradeStats,
    trust: f32,
) {
    let mut lambda = vec![0.0_f32; SECTORS];
    let mut log_ratio = vec![0.0_f32; SECTORS];
    let (mut mean, mut mass) = (0.0, 0.0);
    for k in 0..SECTORS {
        let support = at(&reference.sector_share, k).min(at(&actual.sector_share, k));
        let (rc, ac) = (
            at(&reference.sector_chroma, k),
            at(&actual.sector_chroma, k),
        );
        if support < 0.003 || ac < 1e-3 {
            continue;
        }
        let l = support / (support + 0.02);
        let r = (rc.max(1e-4) / ac).ln();
        if let (Some(ls), Some(rs)) = (lambda.get_mut(k), log_ratio.get_mut(k)) {
            *ls = l;
            *rs = r;
        }
        mean += l * support * r;
        mass += l * support;
    }
    let mean = if mass > 1e-9 { mean / mass } else { 0.0 };
    let gain: Vec<f32> = (0..SECTORS)
        .map(|k| 0.8 * at(&lambda, k) * (at(&log_ratio, k) - mean))
        .collect();
    let turn: Vec<f32> = (0..SECTORS)
        .map(|k| {
            0.8 * trust
                * at(&lambda, k)
                * wrap(at(&reference.sector_hue, k) - at(&actual.sector_hue, k))
        })
        .collect();
    let lift: Vec<f32> = (0..SECTORS)
        .map(|k| {
            0.6 * trust
                * at(&lambda, k)
                * (at(&reference.sector_light, k) - at(&actual.sector_light, k))
        })
        .collect();
    let (gain, turn, lift) = (
        smooth_circular(&gain),
        smooth_circular(&turn),
        smooth_circular(&lift),
    );
    for (k, value) in grade.hue_gain.iter_mut().enumerate() {
        *value *= at(&gain, k).exp();
    }
    for (k, value) in grade.hue_shift.iter_mut().enumerate() {
        *value += at(&turn, k);
    }
    for (k, value) in grade.hue_light.iter_mut().enumerate() {
        *value += at(&lift, k);
    }
}

fn update_zones(grade: &mut ReferenceGrade, reference: &GradeStats, actual: &GradeStats) {
    let mut terms = Vec::with_capacity(ZONES);
    for z in 0..ZONES {
        let support = at(&reference.zone_weight, z).min(at(&actual.zone_weight, z));
        let (rc, ac) = (at(&reference.zone_chroma, z), at(&actual.zone_chroma, z));
        if support < 0.01 || ac < 1e-3 {
            terms.push((0.0, 0.0, support));
            continue;
        }
        terms.push((
            support / (support + 0.03),
            (rc.max(1e-4) / ac).ln(),
            support,
        ));
    }
    let mass: f32 = terms.iter().map(|t| t.0 * t.2).sum();
    let mean = if mass > 1e-9 {
        terms.iter().map(|t| t.0 * t.2 * t.1).sum::<f32>() / mass
    } else {
        0.0
    };
    for (value, (l, r, _)) in grade.zone_gain.iter_mut().zip(terms) {
        *value *= (0.8 * l * (r - mean)).exp();
    }
}

/// Match colourfulness using colours both scenes contain, rather than matching the fraction
/// of the frame occupied by coloured objects. A neutral wall must not desaturate green trees.
fn shared_chroma_scale(reference: &GradeStats, actual: &GradeStats) -> f32 {
    let mut ratios: Vec<_> = (0..SECTORS)
        .filter_map(|k| {
            let support = at(&reference.sector_share, k).min(at(&actual.sector_share, k));
            let (rc, ac) = (
                at(&reference.sector_chroma, k),
                at(&actual.sector_chroma, k),
            );
            (support >= 0.005 && rc > 0.01 && ac > 0.01)
                .then_some(((rc / ac).ln().clamp(-0.7, 0.7), support))
        })
        .collect();
    weighted_quantile(&mut ratios, 0.5).unwrap_or(0.0).exp()
}

/// Shared hue evidence may guide a different scene, but object brightness and colourfulness
/// within a sector must not be copied along with it. Apply one bounded update, without chasing
/// the changing histogram through repeated reassignments of pixels to hue sectors.
fn update_shared_hues(grade: &mut ReferenceGrade, reference: &GradeStats, actual: &GradeStats) {
    let turns: Vec<_> = (0..SECTORS)
        .map(|k| {
            let support = at(&reference.sector_share, k).min(at(&actual.sector_share, k));
            if support < 0.005 {
                return 0.0;
            }
            0.64 * support / (support + 0.02)
                * wrap(at(&reference.sector_hue, k) - at(&actual.sector_hue, k))
        })
        .collect();
    for (shift, turn) in grade.hue_shift.iter_mut().zip(smooth_circular(&turns)) {
        *shift += turn;
    }
}

fn transformed(grade: &ReferenceGrade, samples: &[Sample]) -> Vec<[f32; 3]> {
    let evaluator = Evaluator::new(grade);
    samples
        .iter()
        .map(|s| oklab_from_linear_srgb(evaluator.linear_srgb(s.linear)))
        .collect()
}

/// The same transform at a fraction of its strength.
pub(super) fn toward(from: &ReferenceGrade, to: &ReferenceGrade, t: f32) -> ReferenceGrade {
    mix(from, to, &Scales::uniform(t))
}

/// How far each component moves from one transform toward another.
#[derive(Debug, Clone, Copy)]
pub(super) struct Scales {
    /// Tone at blacks, shadows, midtones, highlights and whites.
    pub tone: [f32; 5],
    pub temperature: f32,
    pub tint: f32,
    pub chroma: f32,
    pub saturation: f32,
    pub hue: f32,
    pub hue_light: f32,
    pub skin: f32,
}

impl Scales {
    pub(super) fn uniform(t: f32) -> Self {
        Self {
            tone: [t; 5],
            temperature: t,
            tint: t,
            chroma: t,
            saturation: t,
            hue: t,
            hue_light: t,
            skin: t.min(1.0),
        }
    }
}

// Strength endpoints must restore the stored transform exactly.
#[allow(clippy::float_cmp)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    if t == 0.0 {
        a
    } else if t == 1.0 {
        b
    } else {
        a + (b - a) * t
    }
}

// Preserve the same exact strength endpoints in multiplicative components.
#[allow(clippy::float_cmp)]
fn log_lerp(a: f32, b: f32, t: f32) -> f32 {
    if a > 1e-4 && b > 1e-4 && !(t == 0.0) && !(t == 1.0) {
        (a.ln() + (b.ln() - a.ln()) * t).exp()
    } else {
        lerp(a, b, t)
    }
}

/// Component-wise interpolation between two transforms. Zero returns `from` exactly, so a
/// strength of zero restores a photograph's previous look bit for bit.
pub(super) fn mix(from: &ReferenceGrade, to: &ReferenceGrade, scales: &Scales) -> ReferenceGrade {
    let tone_scale = |x: f32| {
        // Blacks, shadows, midtones, highlights, whites: a partition of unity over lightness.
        let p = x.clamp(0.0, 1.0) * 4.0;
        let i = (p as usize).min(3);
        let t = p - i as f32;
        scales.tone.get(i).copied().unwrap_or(1.0) * (1.0 - t)
            + scales.tone.get(i + 1).copied().unwrap_or(1.0) * t
    };
    let mut out = to.clone();
    for (i, (v, (a, b))) in out
        .tone
        .iter_mut()
        .zip(from.tone.iter().zip(&to.tone))
        .enumerate()
    {
        *v = lerp(*a, *b, tone_scale(i as f32 / (TONE_NODES - 1) as f32));
    }
    for (slot, (a, b)) in out.cast_in.iter_mut().chain(out.cast_out.iter_mut()).zip(
        from.cast_in
            .iter()
            .chain(&from.cast_out)
            .zip(to.cast_in.iter().chain(&to.cast_out)),
    ) {
        *slot = [
            lerp(a[0], b[0], scales.tint),
            lerp(a[1], b[1], scales.temperature),
        ];
    }
    for (v, (a, b)) in out
        .chroma
        .iter_mut()
        .zip(from.chroma.iter().zip(&to.chroma))
    {
        *v = lerp(*a, *b, scales.chroma);
    }
    for (v, (a, b)) in out.hue_gain.iter_mut().chain(out.zone_gain.iter_mut()).zip(
        from.hue_gain
            .iter()
            .chain(&from.zone_gain)
            .zip(to.hue_gain.iter().chain(&to.zone_gain)),
    ) {
        *v = log_lerp(*a, *b, scales.saturation);
    }
    for (v, (a, b)) in out
        .hue_shift
        .iter_mut()
        .zip(from.hue_shift.iter().zip(&to.hue_shift))
    {
        *v = lerp(*a, *b, scales.hue);
    }
    for (v, (a, b)) in out
        .hue_light
        .iter_mut()
        .zip(from.hue_light.iter().zip(&to.hue_light))
    {
        *v = lerp(*a, *b, scales.hue_light);
    }
    out.skin = lerp(from.skin, to.skin, scales.skin);
    out.skin_fidelity = lerp(from.skin_fidelity, to.skin_fidelity, scales.skin);
    out.palette = palette::mix(from.palette.as_ref(), to.palette.as_ref(), scales);
    if out.validate().is_err() {
        out = out.bounded();
    }
    out
}

/// What the fit found and how close it came, before the renderer confirms it.
#[derive(Debug, Clone)]
pub(super) struct Fit {
    pub grade: ReferenceGrade,
    pub before: Distance,
    pub after: Distance,
    pub iterations: usize,
    pub content: f32,
    /// The scene-prior tone estimate, when the reference's scene is not this photograph's.
    pub tone: Option<ToneReading>,
    /// How far the fit stretched colour differences before it was limited, when it had to be.
    pub stretch_limited_from: Option<f32>,
}

/// The evidence available for a fit. Similarity is structural correspondence, not confidence.
struct FitContext<'a> {
    reference: &'a GradeStats,
    target: &'a [Sample],
    content: f32,
    monochrome: bool,
    toned: bool,
    iterations: usize,
    palette: bool,
}

impl FitContext<'_> {
    fn corresponding(&self) -> bool {
        self.content >= SAME_SCENE || self.palette
    }

    fn initial(&self, start: &GradeStats) -> ReferenceGrade {
        let mut grade = ReferenceGrade::identity();
        grade.skin_fidelity = if self.palette && !self.monochrome {
            1.0
        } else {
            0.0
        };
        grade.cast_in.clone_from(&start.axis);
        if !self.monochrome {
            grade.cast_out.clone_from(&self.reference.axis);
        }
        if self.corresponding() || self.monochrome || self.toned {
            let tone = tone_curve(
                &start.lightness,
                &self.reference.lightness,
                0.35 + 0.65 * self.content,
            );
            let amount = if self.corresponding() { 1.0 } else { 0.1 };
            for (v, &fitted) in grade.tone.iter_mut().zip(&tone) {
                *v += amount * (fitted - *v);
            }
        }
        if self.monochrome {
            grade.chroma.fill(0.0);
        }
        grade.bounded()
    }

    fn refine(
        &self,
        grade: &mut ReferenceGrade,
        best: &mut (Distance, ReferenceGrade),
        checkpoint: &mut impl FnMut() -> AuraResult<()>,
    ) -> AuraResult<usize> {
        let iterations = if self.corresponding() || self.toned {
            self.iterations
        } else {
            1
        };
        for _ in 0..iterations {
            checkpoint()?;
            let now = measure(&transformed(grade, self.target));
            let score = distance(self.reference, &now);
            if self.corresponding() {
                if score.total.is_finite() && score.total + 1e-4 < best.0.total {
                    *best = (score, grade.clone());
                }
                let correction = tone_curve(
                    &now.lightness,
                    &self.reference.lightness,
                    0.35 + 0.65 * self.content,
                );
                grade.tone = regularise_tone(
                    grade
                        .tone
                        .iter()
                        .map(|v| nodes_at(&correction, 1.0, *v))
                        .collect(),
                );
            }
            if self.monochrome {
                continue;
            }
            if self.corresponding() {
                for (out, (r, a)) in grade
                    .cast_out
                    .iter_mut()
                    .zip(self.reference.axis.iter().zip(&now.axis))
                {
                    out[0] += 0.8 * (r[0] - a[0]);
                    out[1] += 0.8 * (r[1] - a[1]);
                }
            }
            *grade = grade.clone().bounded();
            checkpoint()?;
            let now = measure(&transformed(grade, self.target));
            if self.corresponding() || self.toned {
                grade.chroma = chroma_curve(&now.chroma, &self.reference.chroma, &grade.chroma);
            } else {
                let scale = shared_chroma_scale(self.reference, &now).powf(0.25);
                grade.chroma.iter_mut().for_each(|v| *v *= scale);
            }
            checkpoint()?;
            let now = measure(&transformed(grade, self.target));
            if self.corresponding() {
                update_sectors(grade, self.reference, &now, 0.55 + 0.45 * self.content);
            } else if !self.toned {
                update_shared_hues(grade, self.reference, &now);
            }
            *grade = grade.clone().bounded();
            checkpoint()?;
            if self.corresponding() {
                let now = measure(&transformed(grade, self.target));
                update_zones(grade, self.reference, &now);
            }
            *grade = grade.clone().bounded();
        }
        Ok(iterations)
    }
}

/// Fit an estimated look from an edited reference. Same-scene structure permits distribution
/// refinement; unrelated scenes use conservative shared-colour evidence. Neither a lower
/// distribution distance nor the content score proves recovery of the original editing operation.
#[cfg(test)]
pub(super) fn fit_with_checkpoint(
    reference: &GradeStats,
    target: &[Sample],
    similarity: Option<f32>,
    checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<Fit> {
    fit_with_quality_checkpoint(
        reference,
        target,
        similarity,
        super::MatchQuality::Precise,
        checkpoint,
    )
}

#[cfg(test)]
pub(super) fn fit_with_quality_checkpoint(
    reference: &GradeStats,
    target: &[Sample],
    similarity: Option<f32>,
    quality: super::MatchQuality,
    checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<Fit> {
    fit_with_intent_checkpoint(
        reference,
        target,
        similarity,
        quality,
        super::MatchIntent::Natural,
        checkpoint,
    )
}

pub(super) fn fit_with_intent_checkpoint(
    reference: &GradeStats,
    target: &[Sample],
    similarity: Option<f32>,
    quality: super::MatchQuality,
    intent: super::MatchIntent,
    checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<Fit> {
    fit_tones(
        reference, target, similarity, quality, intent, true, checkpoint,
    )
}

/// The fit, with or without the scene-prior tone; only the looks harness turns it off, to
/// measure against the fit as it was before ADR-0180.
fn fit_tones(
    reference: &GradeStats,
    target: &[Sample],
    similarity: Option<f32>,
    quality: super::MatchQuality,
    intent: super::MatchIntent,
    scene_tone: bool,
    mut checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<Fit> {
    checkpoint()?;
    let mut reference = reference.clone();
    reference.palette_matching = intent == super::MatchIntent::Palette;
    let reference = &reference;
    let labs: Vec<[f32; 3]> = target.iter().map(|s| s.lab).collect();
    let start = measure(&labs);
    let before = distance(reference, &start);
    let content = similarity.map_or(0.0, |s| ((s - 0.3) / 0.6).clamp(0.0, 1.0));
    let identity = ReferenceGrade::identity();
    if target.len() < MIN_SAMPLES
        || (reference.samples as usize) < MIN_SAMPLES
        || before.total <= 1e-4
    {
        return Ok(Fit {
            grade: identity,
            before,
            after: before,
            iterations: 0,
            content,
            tone: None,
            stretch_limited_from: None,
        });
    }
    // A colour reference cannot restore colours absent from a genuinely grey target.
    let keep_target_grey = start.achromatic && at(&reference.chroma, CHROMA_QUANTILES - 2) > 0.02;
    let fitter = FitContext {
        reference,
        target,
        content,
        monochrome: reference.achromatic || keep_target_grey,
        toned: at(&reference.chroma, CHROMA_QUANTILES - 2) < 0.012,
        iterations: quality.iterations(),
        palette: intent == super::MatchIntent::Palette,
    };
    let mut grade = fitter.initial(&start);
    let mut best = (before, identity.clone());
    let iterations = fitter.refine(&mut grade, &mut best, &mut checkpoint)?;
    let scene_amount = if fitter.corresponding() || fitter.monochrome || fitter.toned {
        1.0
    } else {
        UNRELATED_SHRINKAGE
    };
    // Identity is always an incumbent. This guards only the measured objective; on unrelated
    // scenes it cannot establish fidelity to an unseen intended grade.
    for t in [1.0, 0.75, 0.5, 0.25] {
        checkpoint()?;
        let candidate = toward(&identity, &grade, t * scene_amount);
        let d = distance(reference, &measure(&transformed(&candidate, target)));
        if d.total.is_finite() && d.total + 1e-4 < best.0.total {
            best = (d, candidate);
        }
    }
    if fitter.palette && !fitter.monochrome && reference.palette_samples.len() >= MIN_SAMPLES {
        best = palette::refine(reference, target, best, quality, &mut checkpoint)?;
    }
    // Across different scenes the reference's lightness histogram is mostly its scene, so
    // matching it is wrong and ignoring it loses every matte, airy, moody or contrasty look. The
    // scene prior estimates the part of it that is an edit, and how far this photograph itself
    // sits from an ordinary finished one, and the tone moves by the difference (ADR-0180). It is
    // a posterior mean already, so the distribution score does not get to undo it.
    let mut tone = None;
    if !fitter.corresponding() && !fitter.palette && scene_tone {
        checkpoint()?;
        if let Some((curve, reading)) =
            tone_prior::for_target(&reference.lightness, &start.lightness, TONE_NODES)
        {
            best.1.tone = regularise_tone(curve);
            best.1 = best.1.clone().bounded();
            best.0 = distance(reference, &measure(&transformed(&best.1, target)));
            tone = Some(reading);
        }
    }
    // Bound how steeply the transform stretches nearby colours, at the colours this photograph
    // contains: a steep spot is a band in a sky or a patch on a floor (ADR-0181).
    // The same picture edited is a real edit to recover, and real edits can be steeper; the limit
    // applies where the scenes differ.
    checkpoint()?;
    let (limited, stretch_limited_from) = if fitter.content >= SAME_SCENE {
        (best.1, None)
    } else {
        limit_stretch(best.1, target)
    };
    best.1 = limited;
    if stretch_limited_from.is_some() {
        best.0 = distance(reference, &measure(&transformed(&best.1, target)));
    }
    Ok(Fit {
        grade: best.1,
        before,
        after: best.0,
        iterations,
        content,
        tone,
        stretch_limited_from,
    })
}

#[cfg(test)]
fn fit(reference: &GradeStats, target: &[Sample], similarity: Option<f32>) -> Fit {
    fit_with_checkpoint(reference, target, similarity, || Ok(())).unwrap()
}

/// The Natural fit without the scene-prior tone, as it was before ADR-0180.
#[cfg(test)]
fn fit_without_scene_tone(
    reference: &GradeStats,
    target: &[Sample],
    similarity: Option<f32>,
) -> Fit {
    fit_tones(
        reference,
        target,
        similarity,
        super::MatchQuality::Precise,
        super::MatchIntent::Natural,
        false,
        || Ok(()),
    )
    .unwrap()
}

/// How much a transform stretches small colour differences at the colours a photograph contains:
/// the 99th percentile, over up to 4,096 of its samples, of the largest local gain in six `OKLab`
/// directions. A look stretches by a bounded, smooth amount everywhere - a contrast curve by its
/// slope; a transform that would band a sky or blotch a floor has a spike, where colours a hair
/// apart land far apart. Measured on the function, not on noisy pixels, so a strong look on a
/// noisy shadow is not mistaken for a band (ADR-0181).
pub(super) fn stretch(grade: &ReferenceGrade, samples: &[Sample]) -> f32 {
    const STEP: f32 = 0.006;
    let evaluator = Evaluator::new(grade);
    let stride = (samples.len() / 4096).max(1);
    let mut gains: Vec<f32> = samples
        .iter()
        .step_by(stride)
        .filter_map(|sample| {
            let from = oklab_from_linear_srgb(evaluator.linear_srgb(sample.linear));
            let mut worst = 0.0_f32;
            for axis in 0..3 {
                for sign in [-1.0_f32, 1.0] {
                    let mut lab = sample.lab;
                    if let Some(v) = lab.get_mut(axis) {
                        *v += sign * STEP;
                    }
                    let linear = aura_render::reference_grade::linear_srgb_from_oklab(lab);
                    if linear.iter().any(|v| !(0.0..=1.0).contains(v)) {
                        continue;
                    }
                    let moved = oklab_from_linear_srgb(linear);
                    let input = (0..3)
                        .map(|c| {
                            (moved.get(c).copied().unwrap_or(0.0)
                                - sample.lab.get(c).copied().unwrap_or(0.0))
                            .powi(2)
                        })
                        .sum::<f32>()
                        .sqrt();
                    if input < 1e-4 {
                        continue;
                    }
                    let to = oklab_from_linear_srgb(evaluator.linear_srgb(linear));
                    let output = (0..3)
                        .map(|c| {
                            (to.get(c).copied().unwrap_or(0.0)
                                - from.get(c).copied().unwrap_or(0.0))
                            .powi(2)
                        })
                        .sum::<f32>()
                        .sqrt();
                    worst = worst.max(output / input);
                }
            }
            (worst > 0.0).then_some(worst)
        })
        .collect();
    if gains.is_empty() {
        return 1.0;
    }
    gains.sort_by(f32::total_cmp);
    gains
        .get(((gains.len() - 1) as f32 * 0.99).round() as usize)
        .copied()
        .unwrap_or(1.0)
}

/// The most a fitted transform may stretch small colour differences. Real looks learned from
/// their own originals stretch the assessment photographs by at most 3.2 (a strong contrast and
/// saturation look) and Natural fits by about 1.1; unconstrained palette fits reached 4 to 42 and
/// banded skies and blotched floors. Limits of 4, 2.5 and 2 left new bands in 10, 3 and 2 of 20
/// palette results, and the tightest was also the closest to the true looks (ADR-0181), so only an
/// explicitly requested palette transfer is ever shortened by it.
pub(super) const MAX_STRETCH: f32 = 2.0;

/// Bring a transform within [`MAX_STRETCH`] by shrinking whichever component - tone, chroma,
/// saturation, hue or cast - is making it steep, a quarter at a time, rather than weakening the
/// whole look. Returns the stretch it started from when it had to.
pub(super) fn limit_stretch(
    grade: ReferenceGrade,
    samples: &[Sample],
) -> (ReferenceGrade, Option<f32>) {
    limit_stretch_to(grade, samples, MAX_STRETCH)
}

/// [`limit_stretch`] to a stated limit.
fn limit_stretch_to(
    grade: ReferenceGrade,
    samples: &[Sample],
    limit: f32,
) -> (ReferenceGrade, Option<f32>) {
    let stride = (samples.len() / 2048).max(1);
    let probe: Vec<Sample> = samples.iter().step_by(stride).copied().collect();
    let first = stretch(&grade, &probe);
    if first <= limit {
        return (grade, None);
    }
    (
        shorten_until(&grade, first, limit, |g| stretch(g, &probe)),
        Some(first),
    )
}

/// Shrink whichever component of a look - tone, chroma, saturation, hue or cast - lowers
/// `measure` most, a quarter at a time, until it is within `limit`; when no single component is
/// responsible, the whole look is shortened. `first` is `measure` of `grade`.
fn shorten_until(
    grade: &ReferenceGrade,
    first: f32,
    limit: f32,
    measure: impl Fn(&ReferenceGrade) -> f32,
) -> ReferenceGrade {
    let identity = ReferenceGrade::identity();
    let shrink = |scales: &Scales, component: usize| {
        let mut next = *scales;
        match component {
            0 => next.tone = next.tone.map(|t| t * 0.75),
            1 => next.chroma *= 0.75,
            2 => next.saturation *= 0.75,
            3 => {
                next.hue *= 0.75;
                next.hue_light *= 0.75;
            }
            _ => {
                next.temperature *= 0.75;
                next.tint *= 0.75;
            }
        }
        next
    };
    let mut scales = Scales::uniform(1.0);
    let (mut best, mut current) = (grade.clone(), first);
    for _ in 0..32 {
        if current <= limit {
            break;
        }
        let choice = (0..5)
            .map(|component| {
                let trial = shrink(&scales, component);
                let candidate = mix(&identity, grade, &trial);
                (measure(&candidate), trial, candidate)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match choice {
            Some((value, trial, candidate)) if value < current - 1e-3 * limit.max(1e-3) => {
                (scales, best, current) = (trial, candidate, value);
            }
            // No single component is responsible: shorten the whole look.
            _ => {
                scales = (0..5).fold(scales, |s, component| shrink(&s, component));
                best = mix(&identity, grade, &scales);
                current = measure(&best);
            }
        }
    }
    best
}

/// The share of this photograph's own colours a transform newly clips: a channel reaching full
/// scale (255 in the delivered file) where it did not before.
pub(super) fn new_clipping(grade: &ReferenceGrade, samples: &[Sample]) -> f32 {
    let full = aura_raw::colour::curve::srgb_decode(254.5 / 255.0);
    let evaluator = Evaluator::new(grade);
    let stride = (samples.len() / 8192).max(1);
    let (mut clipped, mut total) = (0_usize, 0_usize);
    for sample in samples.iter().step_by(stride) {
        total += 1;
        let before = sample.linear.iter().copied().fold(0.0_f32, f32::max);
        if before >= full {
            continue;
        }
        let after = evaluator
            .linear_srgb(sample.linear)
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        if after >= full {
            clipped += 1;
        }
    }
    clipped as f32 / total.max(1) as f32
}

/// The most new clipping a fitted look may cause: half of what the native check allows, so a
/// strength between the steps the analysis tried does not land on the check's edge.
pub(super) const MAX_NEW_CLIPPING: f32 = 0.005;

/// Hold back only the highlights of a look that would clip them. Near white there is no room for
/// colour - a cool cast on a bright backdrop drives its blue channel to full scale at the same
/// lightness - and the native check refuses the clipping, after which the whole look used to be
/// shortened until the photograph barely changed. Instead the cast, colourfulness and tone of the
/// top of the range move a quarter of the way back toward the photograph's own at a time, more
/// the brighter they are, until the new clipping is within `limit`; skin, midtones
/// and shadows keep the whole look. Returns the clipping it started from when it had to.
///
/// `limit` is a share of what this transform alone does to the photograph's colours; the native
/// check, which renders the whole pipeline, calibrates it against [`MAX_NEW_CLIPPING`].
pub(super) fn limit_highlights_to(
    grade: ReferenceGrade,
    samples: &[Sample],
    limit: f32,
) -> (ReferenceGrade, Option<f32>) {
    let first = new_clipping(&grade, samples);
    if first <= limit {
        return (grade, None);
    }
    let weight = |lightness: f32| smoothstep(0.55, 1.0, lightness);
    // Each step moves the highlights toward the photograph's own, so it is kept even when the
    // clipping has not fallen yet: clipping is a step function of the cast and stays flat until
    // a channel comes back under full scale.
    let (mut best, mut current) = (grade, first);
    for _ in 0..24 {
        if current <= limit {
            break;
        }
        let mut next = best.clone();
        for (k, (out, input)) in next.cast_out.iter_mut().zip(&best.cast_in).enumerate() {
            let w = 0.25 * weight(k as f32 / (AXIS_NODES - 1) as f32);
            out[0] += w * (input[0] - out[0]);
            out[1] += w * (input[1] - out[1]);
        }
        for (z, gain) in next.zone_gain.iter_mut().enumerate() {
            let w = 0.25 * weight(z as f32 / (ZONES - 1) as f32);
            *gain += w * (1.0 - *gain);
        }
        for (i, v) in next.tone.iter_mut().enumerate() {
            let x = i as f32 / (TONE_NODES - 1) as f32;
            *v += 0.25 * weight(x) * (x - *v);
        }
        next.tone = regularise_tone(next.tone);
        next = next.bounded();
        current = new_clipping(&next, samples);
        best = next;
    }
    // What is left is not in the highlights - a cast pushing bright, colourful pixels such as a
    // pastel dress past full scale - so shorten whichever part of the look does it.
    if current > limit {
        best = shorten_until(&best, current, limit, |g| new_clipping(g, samples));
    }
    (best, Some(first))
}

/// What the skin review found.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinReview {
    pub people: usize,
    pub protection: f32,
    pub worst_hue_shift_degrees: f32,
    pub worst_chroma_ratio: f32,
}

/// Limit how far the transform turns each person's skin, measured on that person.
///
/// Every person is checked separately, as the slider fit's guard does, because an unchanged
/// average can hide one face turning green in a group. A face that turns more than nine degrees
/// of hue, or gains or loses colour beyond half again, raises the protection the renderer applies
/// to skin-coloured pixels; nothing is refused.
pub(super) fn review_skin(
    grade: &mut ReferenceGrade,
    image: &Rgb8,
    regions: &[Region],
    monochrome: bool,
) -> SkinReview {
    let people: Vec<&Region> = regions
        .iter()
        .filter(|r| r.name.starts_with("guard:person-") && r.name.ends_with("skin"))
        .collect();
    let mut review = SkinReview {
        people: people.len(),
        protection: 0.5,
        worst_hue_shift_degrees: 0.0,
        worst_chroma_ratio: 1.0,
    };
    if monochrome {
        grade.skin = 0.0;
        review.protection = 0.0;
        return review;
    }
    let pixel = |x: f32, y: f32| {
        let column =
            ((x * image.width as f32) as usize).min(image.width.saturating_sub(1) as usize);
        let row = ((y * image.height as f32) as usize).min(image.height.saturating_sub(1) as usize);
        let at = (row * image.width as usize + column) * 3;
        image
            .data
            .get(at..at + 3)
            .and_then(|p| <[u8; 3]>::try_from(p).ok())
            .map(|p| p.map(srgb_byte_to_linear))
    };
    for level in [0.5, 1.0] {
        grade.skin = level;
        review.protection = level;
        let evaluator = Evaluator::new(grade);
        let (mut worst_turn, mut worst_ratio) = (0.0_f32, 1.0_f32);
        for person in &people {
            let (mut before, mut after, mut n) = ([0.0_f32; 2], [0.0_f32; 2], 0.0_f32);
            for &[x, y] in &person.points {
                let Some(linear) = pixel(x, y) else { continue };
                let a = oklab_from_linear_srgb(linear);
                let b = oklab_from_linear_srgb(evaluator.linear_srgb(linear));
                before = [before[0] + a[1], before[1] + a[2]];
                after = [after[0] + b[1], after[1] + b[2]];
                n += 1.0;
            }
            let (cb, ca) = (
                before[0].hypot(before[1]) / n.max(1.0),
                after[0].hypot(after[1]) / n.max(1.0),
            );
            if n < 16.0 || cb < 0.01 {
                continue;
            }
            let turn = wrap(after[1].atan2(after[0]) - before[1].atan2(before[0]))
                .abs()
                .to_degrees();
            let ratio = ca / cb;
            worst_turn = worst_turn.max(turn);
            if (ratio - 1.0).abs() > (worst_ratio - 1.0).abs() {
                worst_ratio = ratio;
            }
        }
        review.worst_hue_shift_degrees = worst_turn;
        review.worst_chroma_ratio = worst_ratio;
        if worst_turn <= 9.0 && (0.6..=1.45).contains(&worst_ratio) {
            break;
        }
    }
    review
}

fn stats_key(id: &str) -> AuraResult<String> {
    super::asset_key(id)?;
    Ok(format!("{STATS}{id}"))
}

/// Measure a reference once, when it enters the library, and keep the measurement by content.
/// A reference imported before this engine is measured the next time it is imported or captured.
pub(super) fn remember(state: &AppState, id: &str, image: &Rgb8) -> AuraResult<bool> {
    let key = stats_key(id)?;
    if super::read::<GradeStats>(state, &key)?.is_some_and(|s| {
        s.usable()
            && s.palette_samples.len() >= MIN_SAMPLES
            && s.reference_sampling == REFERENCE_SAMPLING
    }) {
        return Ok(false);
    }
    let labs: Vec<[f32; 3]> = samples(image, REFERENCE_SAMPLES)
        .iter()
        .map(|s| s.lab)
        .collect();
    if labs.len() < MIN_SAMPLES {
        return Ok(false);
    }
    let mut measured = measure(&labs);
    measured.reference_sampling = REFERENCE_SAMPLING;
    super::store(state, &key, &measured)?;
    Ok(true)
}

/// The stored measurement of a reference, when it has one this build can use.
pub(super) fn recall(state: &AppState, id: &str) -> AuraResult<Option<GradeStats>> {
    let measurements = super::read::<GradeStats>(state, &stats_key(id)?)?;
    if measurements.as_ref().is_some_and(|s| !s.usable()) {
        return Err(super::invalid(
            "This reference needs fresh colour measurements. Upload its edited photo again, then analyse the match.",
        ));
    }
    Ok(measurements)
}

#[cfg(test)]
mod robustness;
#[cfg(test)]
mod tests;
