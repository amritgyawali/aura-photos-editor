//! Colour and tone transfer from an edited reference, fitted by distribution (ADR-0160).
//!
//! The slider fit this replaces searched bounded recipe parameters through the renderer, so it
//! could only move as far as those parameters reach: no hue rotation, a tone shape of five
//! sliders, a white balance capped at 800 K - and a split tone, a strong grade or a monochrome
//! reference ended farther from the look on a new photograph. This module measures the reference
//! and the photograph as distributions in `OKLab` and fits `aura_recipe::reference_grade`
//! directly:
//!
//! * **tone** - lightness quantiles mapped onto the reference's, blended toward a landmark-only
//!   curve when the two are different scenes, slope-limited so nothing posterises;
//! * **white balance and split toning** - each photograph's neutral axis at nine lightnesses,
//!   estimated from its most neutral pixels;
//! * **colourfulness** - chroma quantiles measured from the neutral axis, then the residual per
//!   hue sector and per lightness zone, each normalised so it cannot absorb a global change;
//! * **hue** - per-sector mean hue, bounded and shrunk by the support both photographs share.
//!
//! Every component is then re-measured on the transformed photograph, through the evaluator the
//! renderer executes, and corrected again until the statistics agree. Nothing is refused for
//! being far from the reference. A person's skin that would turn is limited instead.
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
const LIGHT_QUANTILES: usize = 65;
const CHROMA_QUANTILES: usize = 33;
/// A photograph is fitted from this many pixels, sub-sampled on a grid so the distribution is
/// kept rather than averaged away.
pub(super) const TARGET_SAMPLES: usize = 40_000;
const REFERENCE_SAMPLES: usize = 65_536;
const MIN_SAMPLES: usize = 256;
const ITERATIONS: usize = 4;
const SECTOR_WIDTH: f32 = TAU / SECTORS as f32;
/// Lightness landmarks, as indices into the 65 lightness quantiles, for the curve used between
/// different scenes: 0.2, 5, 15, 30, 50, 70, 85, 95 and 99.8 per cent.
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
use neutral::{measuring_axis, neutral_axis};
mod pairs;
pub(super) use pairs::{refine as refine_on_pairs, Pairs};

/// The 128 by 128 thumbnail a reference keeps (`persist_reference`), made the same way from a
/// photograph so the two line up pixel for pixel when they are the same frame.
pub(super) fn thumbnail(image: &Rgb8) -> Option<Rgb8> {
    let rgb = image::RgbImage::from_raw(image.width, image.height, image.data.clone())?;
    let small = image::imageops::thumbnail(&rgb, 128, 128);
    Some(Rgb8 {
        width: small.width(),
        height: small.height(),
        data: small.into_raw(),
    })
}

/// What a photograph's colour and tone are, as the fit compares them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GradeStats {
    pub version: u32,
    pub samples: u32,
    pub lightness: Vec<f32>,
    /// The shift-equivariant axis chroma is measured from, and the data under it.
    pub axis: Vec<[f32; 2]>,
    pub axis_weight: Vec<f32>,
    /// Where the greys are, and the evidence that they are greys.
    pub neutral: Vec<[f32; 2]>,
    pub neutral_weight: Vec<f32>,
    pub chroma: Vec<f32>,
    pub zone_chroma: Vec<f32>,
    pub zone_weight: Vec<f32>,
    pub sector_share: Vec<f32>,
    pub sector_chroma: Vec<f32>,
    pub sector_hue: Vec<f32>,
    pub sector_light: Vec<f32>,
    pub achromatic: bool,
}

impl GradeStats {
    fn usable(&self) -> bool {
        self.version == STATS_VERSION
            && self.lightness.len() == LIGHT_QUANTILES
            && self.axis.len() == AXIS_NODES
            && self.axis_weight.len() == AXIS_NODES
            && self.neutral.len() == AXIS_NODES
            && self.neutral_weight.len() == AXIS_NODES
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
                .chain(self.neutral.iter().flatten())
                .chain(&self.neutral_weight)
                .chain(&self.chroma)
                .chain(&self.zone_chroma)
                .chain(&self.zone_weight)
                .chain(&self.sector_share)
                .chain(&self.sector_chroma)
                .chain(&self.sector_hue)
                .chain(&self.sector_light)
                .all(|v| v.is_finite())
    }
}

fn weighted_quantile(pairs: &mut [(f32, f32)], q: f32) -> Option<f32> {
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
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
    values.sort_by(f32::total_cmp);
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
    let (axis, axis_weight) = measuring_axis(labs);
    let (neutral, neutral_weight) = neutral_axis(labs);
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
        samples: labs.len() as u32,
        lightness,
        axis,
        axis_weight,
        neutral,
        neutral_weight,
        chroma,
        zone_chroma,
        zone_weight,
        sector_share,
        sector_chroma,
        sector_hue,
        sector_light,
        achromatic,
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

/// How far two photographs' colour and tone are apart, in `OKLab` units times a hundred - close
/// to CIELAB ΔE in size. Four terms, so a report can say which part of a look is missing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Distance {
    pub tone: f32,
    pub cast: f32,
    pub chroma: f32,
    pub hue: f32,
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
    Distance {
        tone,
        cast,
        chroma,
        hue,
        total: (tone + cast + chroma + hue) / 4.0,
    }
}

/// How much of each part of a reference's appearance a photograph of a *different* scene takes.
///
/// One finished photograph confounds its edit with its content: a beige wall and a warm grade
/// measure the same, and so do a dark doorway and crushed blacks. Nothing in a single image
/// separates them, so the parts of a look are trusted in the order of how little content moves
/// them, and a photograph of the same scene takes all of every part:
///
/// * the black and white points - a lifted, faded or clipped end - are taken whole, because
///   nearly every photograph has something near black and something near white;
/// * the neutral axis - white balance and split toning - comes from the least coloured pixels
///   at each lightness, which content moves only when nothing there is neutral;
/// * colourfulness, the tonal shape between the ends, and per-hue character are what content
///   moves most: a flower market is more saturated than a church under the same edit.
///
/// These are authored priors chosen by measurement, not fitted constants. ADR-0160 records the
/// four sets they were compared on: two halves of a real wedding shoot under eight known looks,
/// and two halves of nineteen MIT-Adobe `FiveK` expert edits, each photograph matched to another's
/// finished version. Of four candidate sets these were the only ones that left a different
/// photograph nearer the right answer on every set (by 3 to 27 per cent) while still moving its
/// appearance a seventh to a third of the way to the reference. Taking more moved appearance
/// further and the answer away, and a set that was best on half a shoot was worst on its other
/// half. Where the reference's original is available the edit itself is learned instead
/// (`edit.rs`) and none of this applies.
const ENDS_TRUST: f32 = 0.2;
const TONE_TRUST: f32 = 0.1;
const CAST_TRUST: f32 = 0.3;
const CHROMA_TRUST: f32 = 0.4;
const HUE_TRUST: f32 = 0.15;

/// Per-part trust for a given scene similarity, from the priors above at a different scene to
/// all of the reference at the same one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Trust {
    /// The black and white points.
    pub ends: f32,
    /// The tonal shape between them.
    pub tone: f32,
    pub cast: f32,
    pub chroma: f32,
    pub hue: f32,
}

impl Trust {
    pub(super) fn for_content(content: f32) -> Self {
        let towards = |prior: f32| prior + (1.0 - prior) * content.clamp(0.0, 1.0);
        Self {
            ends: towards(ENDS_TRUST),
            tone: towards(TONE_TRUST),
            cast: towards(CAST_TRUST),
            chroma: towards(CHROMA_TRUST),
            hue: towards(HUE_TRUST),
        }
    }
}

/// The appearance the fit steers the photograph toward: its own measurement moved toward the
/// reference's by each part's trust. With full trust this is the reference's measurement.
pub(super) fn goal(
    reference: &GradeStats,
    start: &GradeStats,
    trust: Trust,
    content: f32,
) -> GradeStats {
    let mix = |own: f32, theirs: f32, t: f32| own + (theirs - own) * t;
    let last = LIGHT_QUANTILES - 1;
    let (t0, t1) = (at(&start.lightness, 0), at(&start.lightness, last));
    let (r0, r1) = (
        mix(t0, at(&reference.lightness, 0), trust.ends),
        mix(t1, at(&reference.lightness, last), trust.ends),
    );
    // The photograph's own tonal shape with the reference's black and white points.
    let levels = |x: f32| {
        if t1 - t0 > 1e-3 {
            r0 + (x - t0) * (r1 - r0) / (t1 - t0)
        } else {
            x
        }
    };
    let mut aim = reference.clone();
    aim.lightness = start
        .lightness
        .iter()
        .zip(&reference.lightness)
        .map(|(own, theirs)| mix(levels(*own), *theirs, trust.tone))
        .collect();
    // The same scene takes the reference's axis whole, lightness by lightness. A different scene
    // takes only a smooth shift between the two photographs' greys - one tint and one slope from
    // shadows to highlights, fitted where both show greys - so no single lightness can carry an
    // object's colour across, and every photograph of a batch is moved the same way.
    let shift = grey_shift(reference, start);
    aim.axis = (0..AXIS_NODES)
        .map(|k| {
            let own = start.axis.get(k).copied().unwrap_or([0.0; 2]);
            let theirs = reference.axis.get(k).copied().unwrap_or(own);
            let between = shift.get(k).copied().unwrap_or([0.0; 2]);
            let take = (1.0 - content) * trust.cast;
            [
                own[0] + content * (theirs[0] - own[0]) + take * between[0],
                own[1] + content * (theirs[1] - own[1]) + take * between[1],
            ]
        })
        .collect();
    if reference.achromatic {
        // A monochrome reference is unambiguous whatever the scene: greys stay grey.
        aim.axis.clone_from(&reference.axis);
    }
    aim.axis_weight = start
        .axis_weight
        .iter()
        .zip(&reference.axis_weight)
        .map(|(a, b)| a.min(*b))
        .collect();
    if !reference.achromatic {
        aim.chroma = start
            .chroma
            .iter()
            .zip(&reference.chroma)
            .map(|(own, theirs)| mix(*own, *theirs, trust.chroma))
            .collect();
        aim.zone_chroma = start
            .zone_chroma
            .iter()
            .zip(&reference.zone_chroma)
            .map(|(own, theirs)| mix(*own, *theirs, trust.chroma))
            .collect();
    }
    aim.zone_weight = start
        .zone_weight
        .iter()
        .zip(&reference.zone_weight)
        .map(|(a, b)| a.min(*b))
        .collect();
    for k in 0..SECTORS {
        let (own_share, their_share) = (at(&start.sector_share, k), at(&reference.sector_share, k));
        let (own_chroma, their_chroma) =
            (at(&start.sector_chroma, k), at(&reference.sector_chroma, k));
        let shared = own_share.min(their_share);
        let moved = if shared >= 0.003 && own_chroma > 1e-3 && their_chroma > 1e-3 {
            (
                (own_chroma.ln() + (their_chroma.ln() - own_chroma.ln()) * trust.chroma).exp(),
                at(&start.sector_hue, k)
                    + trust.hue * wrap(at(&reference.sector_hue, k) - at(&start.sector_hue, k)),
                mix(
                    at(&start.sector_light, k),
                    at(&reference.sector_light, k),
                    trust.hue,
                ),
            )
        } else {
            (
                own_chroma,
                at(&start.sector_hue, k),
                at(&start.sector_light, k),
            )
        };
        if let (Some(share), Some(chroma), Some(hue), Some(light)) = (
            aim.sector_share.get_mut(k),
            aim.sector_chroma.get_mut(k),
            aim.sector_hue.get_mut(k),
            aim.sector_light.get_mut(k),
        ) {
            *share = shared;
            (*chroma, *hue, *light) = moved;
        }
    }
    aim
}

/// How far the reference's greys sit from the photograph's, as one tint plus one slope across
/// lightness, by weighted least squares over the lightnesses where both photographs show greys,
/// and scaled down when there are few of them. Zero when there are none.
fn grey_shift(reference: &GradeStats, start: &GradeStats) -> Vec<[f32; 2]> {
    let mut sums = [0.0_f32; 7]; // w, wx, wxx, wa, wb, wxa, wxb
    for k in 0..AXIS_NODES {
        let w = at(&start.neutral_weight, k)
            .min(at(&reference.neutral_weight, k))
            .clamp(0.0, 1.0);
        if w <= 0.0 {
            continue;
        }
        let x = k as f32 / (AXIS_NODES - 1) as f32 - 0.5;
        let own = start.neutral.get(k).copied().unwrap_or([0.0; 2]);
        let theirs = reference.neutral.get(k).copied().unwrap_or(own);
        let (da, db) = (theirs[0] - own[0], theirs[1] - own[1]);
        for (slot, term) in
            sums.iter_mut()
                .zip([w, w * x, w * x * x, w * da, w * db, w * x * da, w * x * db])
        {
            *slot += term;
        }
    }
    let [w, wx, wxx, wa, wb, wxa, wxb] = sums;
    if w <= 1e-3 {
        return vec![[0.0; 2]; AXIS_NODES];
    }
    let mean_x = wx / w;
    let spread = wxx / w - mean_x * mean_x;
    let (mean_a, mean_b) = (wa / w, wb / w);
    // A slope needs evidence spread across lightness: a quarter of the range at least.
    let (slope_a, slope_b) = if spread > 0.25 * 0.25 / 4.0 {
        (
            ((wxa / w - mean_x * mean_a) / spread).clamp(-0.1, 0.1),
            ((wxb / w - mean_x * mean_b) / spread).clamp(-0.1, 0.1),
        )
    } else {
        (0.0, 0.0)
    };
    // Two lightnesses' worth of full evidence is full confidence.
    let confidence = (w / 2.0).min(1.0);
    (0..AXIS_NODES)
        .map(|k| {
            let x = k as f32 / (AXIS_NODES - 1) as f32 - 0.5 - mean_x;
            [
                confidence * (mean_a + slope_a * x),
                confidence * (mean_b + slope_b * x),
            ]
        })
        .collect()
}

/// Map `x` through the quantile function: where `x` sits among `from`, the same place in `to`.
/// Runs of equal quantiles - a clipped sky, a black border - map to the mean of their run.
fn quantile_map(from: &[f32], to: &[f32], x: f32, ceiling: f32, slopes: (f32, f32)) -> f32 {
    let last = from.len().min(to.len()).saturating_sub(1);
    if last == 0 {
        return x;
    }
    let edge = (from.len() / 16).clamp(1, last);
    let slope = |df: f32, dt: f32| {
        if df > 1e-4 {
            (dt / df).clamp(slopes.0, slopes.1)
        } else {
            1.0
        }
    };
    let (f0, fl) = (at(from, 0), at(from, last));
    if x <= f0 {
        let s = slope(at(from, edge) - f0, at(to, edge) - at(to, 0));
        return (at(to, 0) - (f0 - x) * s).max(0.0);
    }
    if x >= fl {
        let s = slope(
            fl - at(from, last - edge),
            at(to, last) - at(to, last - edge),
        );
        return (at(to, last) + (x - fl) * s).min(ceiling);
    }
    let run_mean = |index: usize| {
        let value = at(from, index);
        let start = from.partition_point(|v| *v < value);
        let end = from.partition_point(|v| *v <= value).max(start + 1);
        let span = start..end.min(last + 1);
        let count = span.len().max(1) as f32;
        span.map(|i| at(to, i)).sum::<f32>() / count
    };
    let j = from
        .partition_point(|v| *v <= x)
        .saturating_sub(1)
        .min(last - 1);
    let (fa, fb) = (at(from, j), at(from, j + 1));
    let (ta, tb) = (run_mean(j), run_mean(j + 1));
    if fb - fa <= 1e-7 {
        return ta;
    }
    ta + (tb - ta) * ((x - fa) / (fb - fa)).clamp(0.0, 1.0)
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
    let last = TONE_NODES - 1;
    let h = 1.0 / last as f32;
    let mut y: Vec<f32> = (0..TONE_NODES)
        .map(|i| {
            let x = i as f32 * h;
            at(&nodes, i).clamp(
                (x - MAX_TONE_DEVIATION).max(0.0),
                (x + MAX_TONE_DEVIATION).min(1.0),
            )
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
    let (lo, hi) = (MIN_TONE_SLOPE * h, MAX_TONE_SLOPE * h);
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

/// Compose a chroma correction onto the current chroma curve.
///
/// The curve is kept in the domain a grade actually works in - a gain on colourfulness that
/// changes gently with how colourful a pixel already is. Quantiles alone would accept a curve that
/// crushes faint colour and keeps strong colour, because the per-hue and per-lightness gains can
/// compensate in the statistics while every pixel ends somewhere else. Near-neutral pixels gain at
/// most three times their chroma plus a trace, so the noise in a grey wall never becomes colour.
fn chroma_curve(from: &[f32], to: &[f32], current: &[f32]) -> Vec<f32> {
    let step = CHROMA_SPAN / (CHROMA_NODES - 1) as f32;
    let mut log_gain: Vec<f32> = (1..CHROMA_NODES)
        .map(|j| {
            let input = j as f32 * step;
            let mapped = quantile_map(from, to, at(current, j), MAX_CHROMA_OUT, (0.3, 2.0));
            (mapped.max(1e-5) / input).clamp(0.25, 3.0).ln()
        })
        .collect();
    for _ in 0..2 {
        let previous = log_gain.clone();
        let last = previous.len().saturating_sub(1);
        log_gain = (0..previous.len())
            .map(|i| {
                let left = at(&previous, i.saturating_sub(1));
                let right = at(&previous, (i + 1).min(last));
                0.25 * left + 0.5 * at(&previous, i) + 0.25 * right
            })
            .collect();
    }
    // Neighbouring nodes may differ by at most a quarter in gain.
    for i in 1..log_gain.len() {
        let previous = at(&log_gain, i - 1);
        if let Some(value) = log_gain.get_mut(i) {
            *value = value.clamp(previous - 0.22, previous + 0.22);
        }
    }
    let mut floor = 0.0_f32;
    std::iter::once(0.0)
        .chain(log_gain.iter().enumerate().map(|(i, g)| {
            let input = (i + 1) as f32 * step;
            let value = (input * g.exp())
                .min(3.0 * input + 0.005)
                .clamp(floor, MAX_CHROMA_OUT);
            floor = value;
            value
        }))
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
    // Half steps, smoothed across neighbouring zones: the zones overlap, and a full step lets two
    // neighbours trade colourfulness back and forth between iterations.
    let steps: Vec<f32> = terms.iter().map(|(l, r, _)| 0.5 * l * (r - mean)).collect();
    let last = steps.len().saturating_sub(1);
    for (z, value) in grade.zone_gain.iter_mut().enumerate() {
        let smoothed = 0.25 * at(&steps, z.saturating_sub(1))
            + 0.5 * at(&steps, z)
            + 0.25 * at(&steps, (z + 1).min(last));
        *value *= smoothed.exp();
    }
}

fn transformed(grade: &ReferenceGrade, samples: &[Sample]) -> Vec<[f32; 3]> {
    let evaluator = Evaluator::new(grade);
    samples
        .iter()
        .map(|s| oklab_from_linear_srgb(evaluator.linear_srgb(s.linear)))
        .collect()
}

/// How far `grade` takes the photograph toward the reference, measured on its samples.
pub(super) fn predicted(
    reference: &GradeStats,
    grade: &ReferenceGrade,
    target: &[Sample],
) -> (Distance, Distance) {
    let labs: Vec<[f32; 3]> = target.iter().map(|s| s.lab).collect();
    (
        distance(reference, &measure(&labs)),
        distance(reference, &measure(&transformed(grade, target))),
    )
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
    if out.validate().is_err() {
        out = out.bounded();
    }
    out
}

/// What the fit found and how close it came, before the renderer confirms it.
#[derive(Debug, Clone)]
pub(super) struct Fit {
    pub grade: ReferenceGrade,
    /// Distance from the reference before and after - how much of its look was taken.
    pub before: Distance,
    pub after: Distance,
    /// Distance from the goal the trust priors set - how well the fit reached what it aimed at.
    pub goal_after: Distance,
    pub iterations: usize,
    pub content: f32,
}

/// How alike two photographs' scenes are, from their structural similarity: the same framing
/// under another edit scores near one, a different photograph near zero. Unknown is different.
pub(super) fn content_of(similarity: Option<f32>) -> f32 {
    similarity.map_or(0.0, |s| ((s - 0.3) / 0.6).clamp(0.0, 1.0))
}

/// Fit the transform that makes `target` measure like `reference`.
///
/// `similarity` is the structural similarity of the two photographs when known. The same scene
/// takes the reference whole, quantile for quantile; a different scene takes each part of the
/// look by its [`Trust`], because there the reference also describes different things.
pub(super) fn fit_with_checkpoint(
    reference: &GradeStats,
    target: &[Sample],
    similarity: Option<f32>,
    checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<Fit> {
    let content = content_of(similarity);
    fit_with(
        reference,
        target,
        content,
        Trust::for_content(content),
        checkpoint,
    )
}

#[allow(clippy::too_many_lines)]
pub(super) fn fit_with(
    reference: &GradeStats,
    target: &[Sample],
    content: f32,
    trust: Trust,
    mut checkpoint: impl FnMut() -> AuraResult<()>,
) -> AuraResult<Fit> {
    checkpoint()?;
    let labs: Vec<[f32; 3]> = target.iter().map(|s| s.lab).collect();
    let start = measure(&labs);
    let before = distance(reference, &start);
    let aim = goal(reference, &start, trust, content);
    let aim_before = distance(&aim, &start);
    let identity = ReferenceGrade::identity();
    let unchanged = Fit {
        grade: identity.clone(),
        before,
        after: before,
        goal_after: aim_before,
        iterations: 0,
        content,
    };
    if target.len() < MIN_SAMPLES
        || (reference.samples as usize) < MIN_SAMPLES
        || before.total <= 1e-4
        || aim_before.total <= 1e-4
    {
        return Ok(unchanged);
    }
    let detail = 0.35 + 0.65 * content;
    let mono = reference.achromatic;
    let mut grade = identity.clone();
    grade.cast_in.clone_from(&start.axis);
    grade.cast_out = if mono {
        vec![[0.0; 2]; AXIS_NODES]
    } else {
        aim.axis.clone()
    };
    grade.tone = tone_curve(&start.lightness, &aim.lightness, detail);
    if mono {
        grade.chroma = vec![0.0; CHROMA_NODES];
    }
    grade = grade.bounded();
    let mut best = (aim_before, identity.clone());
    let consider =
        |candidate: &ReferenceGrade, now: &GradeStats, best: &mut (Distance, ReferenceGrade)| {
            let score = distance(&aim, now);
            // Keep intermediate answers: coupled cast and chroma updates can overshoot later, and
            // scaling the last answer cannot recover an earlier, better one. An answer that would
            // leave the photograph farther from the reference's appearance is never kept.
            if score.total.is_finite()
                && score.total + 1e-4 < best.0.total
                && distance(reference, now).total <= before.total + 1e-4
            {
                *best = (score, candidate.clone());
            }
        };
    for _ in 0..ITERATIONS {
        checkpoint()?;
        let now = measure(&transformed(&grade, target));
        consider(&grade, &now, &mut best);
        let correction = tone_curve(&now.lightness, &aim.lightness, detail);
        grade.tone = regularise_tone(
            grade
                .tone
                .iter()
                .map(|v| nodes_at(&correction, 1.0, *v))
                .collect(),
        );
        if !mono {
            for (out, (r, a)) in grade
                .cast_out
                .iter_mut()
                .zip(aim.axis.iter().zip(&now.axis))
            {
                out[0] += 0.8 * (r[0] - a[0]);
                out[1] += 0.8 * (r[1] - a[1]);
            }
            grade = grade.bounded();
            checkpoint()?;
            let now = measure(&transformed(&grade, target));
            grade.chroma = chroma_curve(&now.chroma, &aim.chroma, &grade.chroma);
            checkpoint()?;
            let now = measure(&transformed(&grade, target));
            update_sectors(&mut grade, &aim, &now, 1.0);
            grade = grade.bounded();
            checkpoint()?;
            let now = measure(&transformed(&grade, target));
            update_zones(&mut grade, &aim, &now);
            // A lightness zone's colourfulness is the most content-bound measurement of all; it
            // may follow the reference by about half again either way and no further.
            for gain in &mut grade.zone_gain {
                *gain = gain.clamp(0.6, 1.6);
            }
        }
        grade = grade.bounded();
    }
    // When every component is constrained the best answer may be part of the way, and a short
    // search over the whole transform finds it. The fit never ends farther from its goal than
    // the photograph started.
    for t in [1.0, 0.75, 0.5, 0.25] {
        checkpoint()?;
        let candidate = toward(&identity, &grade, t);
        let now = measure(&transformed(&candidate, target));
        consider(&candidate, &now, &mut best);
    }
    let (goal_after, grade) = best;
    let after = distance(reference, &measure(&transformed(&grade, target)));
    Ok(Fit {
        grade,
        before,
        after,
        goal_after,
        iterations: ITERATIONS,
        content,
    })
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
pub(super) fn remember(state: &AppState, id: &str, image: &Rgb8) -> AuraResult<()> {
    let key = stats_key(id)?;
    if super::read::<GradeStats>(state, &key)?.is_some_and(|s| s.usable()) {
        return Ok(());
    }
    let labs: Vec<[f32; 3]> = samples(image, REFERENCE_SAMPLES)
        .iter()
        .map(|s| s.lab)
        .collect();
    if labs.len() < MIN_SAMPLES {
        return Ok(());
    }
    super::store(state, &key, &measure(&labs))
}

/// The stored measurement of a reference, when it has one this build can use.
#[allow(clippy::similar_names)] // state and stats
pub(super) fn recall(state: &AppState, id: &str) -> AuraResult<Option<GradeStats>> {
    let stats = super::read::<GradeStats>(state, &stats_key(id)?)?;
    if stats.as_ref().is_some_and(|s| !s.usable()) {
        return Err(super::invalid(
            "This reference needs fresh colour measurements. Upload its edited photo again, then analyse the match.",
        ));
    }
    Ok(stats)
}

#[cfg(test)]
fn fit(reference: &GradeStats, target: &[Sample], similarity: Option<f32>) -> Fit {
    fit_with_checkpoint(reference, target, similarity, || Ok(())).unwrap()
}

#[cfg(test)]
mod robustness;
#[cfg(test)]
mod tests;
