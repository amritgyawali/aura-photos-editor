//! The reference's exact colours (ADR-0182).
//!
//! The Natural goal copies the part of a reference that is an edit; this goal copies its colours.
//! The photograph's colour distribution in `OKLab` - lightness, hue and colourfulness together -
//! is moved onto the reference's. Lightness goes to the reference's lightness of the same rank,
//! which is its tone curve and never reverses. Colour goes by iterative distribution transfer
//! (Pitié, Kokaram and Dahyot, 2007) in the colour plane: along two directions at a time each
//! value is sent to the reference value of the same rank, and the directions turn between
//! rounds. It runs separately in four lightness bands blended smoothly, so the reference's
//! shadows give the photograph's shadows their colour and its highlights its highlights. Skin goes to the
//! reference's skin; a reference without people leaves a face its own colour. Skin is the skin the
//! people detector found in each photograph (ADR-0183): a warm reference has many skin-coloured
//! walls and curtains, and a face sent to their colours comes out yellow and dull. What comes out
//! has the reference's tone curve, its palette in its proportions and its split tone.
//!
//! Every map is learned from samples, so the whole transfer is a function of colour alone, and
//! `transfer::exact_grade` writes it into the stored reference grade the renderer already applies:
//! the tone map as its tone curve, the colour each lightness takes on average as its neutral axis,
//! and the rest as its palette displacement field. The strength sliders scale it, the native checks
//! refuse bands, and Undo restores the photograph exactly.
//!
//! A different scene is repainted in the reference's colours. That is what this goal is for, and
//! the panel says so beside it.
use aura_core::{progress::CancelToken, AuraResult};
use aura_raw::codec::Rgb8;
use aura_raw::colour::curve::srgb_byte_to_linear;
use aura_render::reference_grade::{oklab_from_linear_srgb, skin_likeness};
use serde::{Deserialize, Serialize};

use super::{appearance, check_cancel, AppState};

const KEY: &str = "colour_reference_exact_v1:";
/// Version 2 adds the colours of the reference's detected skin.
const VERSION: u32 = 2;
/// Reference colours kept for the transfer, from its full-resolution pixels.
const KEPT: usize = 8192;
/// Rounds of two colour-plane directions per lightness band.
const ROUNDS: usize = 12;
/// Points of each one-dimensional map.
const KNOTS: usize = 129;
/// No colour map segment is steeper than this or flatter than its inverse: a steeper one is a band.
const MAX_SLOPE: f32 = 2.5;
/// The tone map may be steeper: its curve is bounded again where it is written into the grade, and
/// the native check refuses any that tears a gradient.
const MAX_TONE_SLOPE: f32 = 4.0;

/// The reference's colours as stored at import, in encoded sRGB.
#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    version: u32,
    rgb: Vec<u8>,
    /// Colours inside its detected skin; empty when it shows no people, absent before version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skin: Option<Vec<u8>>,
}

/// What the reference gives the exact colours.
pub(super) struct ReferenceColours {
    pub colours: Vec<[f32; 3]>,
    /// Its detected skin's colours; `None` when they were not measured (imported before them).
    pub skin: Option<Vec<[f32; 3]>>,
    /// The share of its colours clipped at full scale.
    pub clipped: f32,
}

fn key(id: &str) -> AuraResult<String> {
    super::asset_key(id)?;
    Ok(format!("{KEY}{id}"))
}

fn rgb_of(pixel: &[u8]) -> [u8; 3] {
    match pixel {
        [r, g, b] => [*r, *g, *b],
        _ => [0; 3],
    }
}

fn pixel_lab(rgb: [u8; 3]) -> [f32; 3] {
    oklab_from_linear_srgb(rgb.map(srgb_byte_to_linear))
}

/// Keep a reference's colours when it enters the library. `regions` are its analysed regions when
/// the caller has them; otherwise they are analysed here, once.
pub(super) fn remember(
    state: &AppState,
    id: &str,
    image: &Rgb8,
    regions: Option<&[appearance::Region]>,
) -> AuraResult<()> {
    let key = key(id)?;
    let count = image.data.len() / 3;
    if count == 0
        || super::read::<Stored>(state, &key)?.is_some_and(|stored| stored.version == VERSION)
    {
        return Ok(());
    }
    let mut rgb = Vec::with_capacity(KEPT.min(count) * 3);
    for i in super::calibration::sample_indices(count, KEPT) {
        if let Some(pixel) = image.data.get(3 * i..3 * i + 3) {
            rgb.extend_from_slice(pixel);
        }
    }
    let analysed;
    let regions = if let Some(regions) = regions {
        regions
    } else {
        analysed = appearance::analyse(image)?;
        &analysed
    };
    let skin = appearance::skin_colours(image, regions, KEPT)
        .into_iter()
        .flatten()
        .collect();
    super::store(
        state,
        &key,
        &Stored {
            version: VERSION,
            rgb,
            skin: Some(skin),
        },
    )
}

/// The reference's colours in `OKLab`, or `None` when none were kept (a reference imported before
/// this goal: its stored palette samples stand in).
pub(super) fn reference_colours(
    state: &AppState,
    id: &str,
) -> AuraResult<Option<ReferenceColours>> {
    if let Some(stored) = super::read::<Stored>(state, &key(id)?)? {
        if stored.version <= VERSION && stored.rgb.len() >= 3 * 256 {
            let pixels: Vec<[u8; 3]> = stored.rgb.chunks_exact(3).map(rgb_of).collect();
            let clipped = pixels.iter().filter(|p| p.contains(&255)).count() as f32
                / pixels.len().max(1) as f32;
            return Ok(Some(ReferenceColours {
                colours: pixels.into_iter().map(pixel_lab).collect(),
                skin: stored
                    .skin
                    .map(|skin| skin.chunks_exact(3).map(rgb_of).map(pixel_lab).collect()),
                clipped,
            }));
        }
    }
    Ok(super::transfer::recall(state, id)?
        .map(|stats| stats.palette_samples)
        .filter(|samples| samples.len() >= 256)
        .map(|colours| ReferenceColours {
            colours,
            skin: None,
            clipped: 0.0,
        }))
}

/// Colours inside a photograph's detected skin, in `OKLab`.
pub(super) fn skin_of(image: &Rgb8, regions: &[appearance::Region]) -> Vec<[f32; 3]> {
    appearance::skin_colours(image, regions, KEPT)
        .into_iter()
        .map(pixel_lab)
        .collect()
}

/// The detected skin of both photographs, which skin is transported between.
#[derive(Debug, Clone, Copy)]
pub(super) struct Skin<'a> {
    pub photograph: &'a [[f32; 3]],
    pub reference: &'a [[f32; 3]],
}

/// One direction's map: values at the photograph's quantiles, and where they go.
#[derive(Debug, Clone)]
struct Map {
    from: Vec<f32>,
    to: Vec<f32>,
}

impl Map {
    fn learn(source: Vec<f32>, target: Vec<f32>) -> Self {
        Self::learn_bounded(source, target, MAX_SLOPE)
    }

    fn learn_bounded(mut source: Vec<f32>, mut target: Vec<f32>, max_slope: f32) -> Self {
        source.sort_by(f32::total_cmp);
        target.sort_by(f32::total_cmp);
        let at = |values: &[f32], q: f32| {
            let position = q * (values.len().saturating_sub(1)) as f32;
            let low = position.floor() as usize;
            let t = position - low as f32;
            let a = values.get(low).copied().unwrap_or(0.0);
            let b = values.get(low + 1).copied().unwrap_or(a);
            a + t * (b - a)
        };
        let mut from = Vec::with_capacity(KNOTS);
        let mut to = Vec::with_capacity(KNOTS);
        for k in 0..KNOTS {
            let q = k as f32 / (KNOTS - 1) as f32;
            from.push(at(&source, q));
            to.push(at(&target, q));
        }
        // Equal source values (a flat region) cannot map to two places; and the slope of every
        // segment is bounded, so no narrow range of the photograph is stretched into a band.
        for k in 1..KNOTS {
            let (lo, hi) = (from.get(k - 1).copied(), from.get(k).copied());
            if let (Some(lo), Some(hi)) = (lo, hi) {
                if hi <= lo + 1e-6 {
                    if let Some(v) = from.get_mut(k) {
                        *v = lo + 1e-6;
                    }
                }
            }
        }
        for k in 1..KNOTS {
            let step =
                from.get(k).copied().unwrap_or(0.0) - from.get(k - 1).copied().unwrap_or(0.0);
            let previous = to.get(k - 1).copied().unwrap_or(0.0);
            if let Some(v) = to.get_mut(k) {
                let rise = (*v - previous).clamp(step / max_slope, step * max_slope);
                *v = previous + rise;
            }
        }
        // Bounding a slope moves everything after it; the shift with the least squared error
        // puts the bounded map back over the reference's quantiles as a whole.
        let wanted: Vec<f32> = (0..KNOTS)
            .map(|k| at(&target, k as f32 / (KNOTS - 1) as f32))
            .collect();
        let shift = wanted.iter().zip(&to).map(|(w, t)| w - t).sum::<f32>() / KNOTS as f32;
        for v in &mut to {
            *v += shift;
        }
        Self { from, to }
    }

    fn apply(&self, x: f32) -> f32 {
        let (first, last) = (
            self.from.first().copied().unwrap_or(0.0),
            self.from.last().copied().unwrap_or(0.0),
        );
        if x <= first {
            return self.to.first().copied().unwrap_or(x) + (x - first);
        }
        if x >= last {
            return self.to.last().copied().unwrap_or(x) + (x - last);
        }
        let upper = self.from.partition_point(|&v| v < x).clamp(1, KNOTS - 1);
        let (x0, x1) = (
            self.from.get(upper - 1).copied().unwrap_or(0.0),
            self.from.get(upper).copied().unwrap_or(1.0),
        );
        let (y0, y1) = (
            self.to.get(upper - 1).copied().unwrap_or(0.0),
            self.to.get(upper).copied().unwrap_or(1.0),
        );
        let t = ((x - x0) / (x1 - x0).max(1e-9)).clamp(0.0, 1.0);
        y0 + t * (y1 - y0)
    }
}

/// Lightness bands the colour is transported in, by rank: shadows go to the reference's shadows
/// and highlights to its highlights, so a split tone survives.
const BANDS: [f32; 4] = [0.125, 0.375, 0.625, 0.875];
/// A band or skin population needs this many samples of its own to be transported.
const MIN_POPULATION: usize = 96;
/// Below this share of skin-coloured pixels the reference has no people to take skin from.
const MIN_SKIN_SHARE: f32 = 0.02;

fn quantile(sorted: &[f32], q: f32) -> f32 {
    let position = q.clamp(0.0, 1.0) * sorted.len().saturating_sub(1) as f32;
    let low = position.floor() as usize;
    let t = position - low as f32;
    let a = sorted.get(low).copied().unwrap_or(0.5);
    let b = sorted.get(low + 1).copied().unwrap_or(a);
    a + t * (b - a)
}

/// Soft membership of `lightness` in each band, from that image's band centres: one band below
/// the first centre and above the last, a smooth blend of two between neighbours, so no colour
/// edge appears where one band hands over to the next.
fn memberships(centres: &[f32; 4], lightness: f32) -> [f32; 4] {
    let mut weights = [0.0_f32; 4];
    let first = centres.first().copied().unwrap_or(0.0);
    let last = centres.last().copied().unwrap_or(1.0);
    if lightness <= first {
        if let Some(w) = weights.first_mut() {
            *w = 1.0;
        }
        return weights;
    }
    if lightness >= last {
        if let Some(w) = weights.last_mut() {
            *w = 1.0;
        }
        return weights;
    }
    for (k, pair) in centres.windows(2).enumerate() {
        let (lo, hi) = match pair {
            [lo, hi] => (*lo, *hi),
            _ => continue,
        };
        if (lo..=hi).contains(&lightness) {
            let t = ((lightness - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0);
            let s = t * t * (3.0 - 2.0 * t);
            if let Some(w) = weights.get_mut(k) {
                *w = 1.0 - s;
            }
            if let Some(w) = weights.get_mut(k + 1) {
                *w = s;
            }
            break;
        }
    }
    weights
}

fn nearest(centres: &[f32; 4], lightness: f32) -> usize {
    centres
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (lightness - **a).abs().total_cmp(&(lightness - **b).abs()))
        .map_or(0, |(k, _)| k)
}

/// Two orthonormal directions in the colour plane, turned by round.
fn plane(round: usize) -> [[f32; 2]; 2] {
    // The golden angle spreads successive rounds evenly around the plane.
    let angle = round as f32 * 2.399_963_2;
    let (s, c) = angle.sin_cos();
    [[c, s], [-s, c]]
}

fn dot2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A colour-plane transfer: rounds of two directions and their maps.
#[derive(Debug, Clone)]
struct Colour {
    rounds: Vec<([[f32; 2]; 2], [Map; 2])>,
}

impl Colour {
    fn identity() -> Self {
        Self { rounds: Vec::new() }
    }

    fn learn(
        photograph: &[[f32; 2]],
        reference: &[[f32; 2]],
        cancel: &CancelToken,
    ) -> AuraResult<Self> {
        if photograph.len() < MIN_POPULATION || reference.len() < MIN_POPULATION {
            return Ok(Self::identity());
        }
        let mut current = photograph.to_vec();
        let mut rounds = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            check_cancel(cancel)?;
            let axes = plane(round);
            let maps = axes.map(|axis| {
                Map::learn(
                    current.iter().map(|p| dot2(*p, axis)).collect(),
                    reference.iter().map(|p| dot2(*p, axis)).collect(),
                )
            });
            for p in &mut current {
                *p = colour_step(&axes, &maps, *p);
            }
            rounds.push((axes, maps));
        }
        Ok(Self { rounds })
    }

    fn apply(&self, ab: [f32; 2]) -> [f32; 2] {
        self.rounds
            .iter()
            .fold(ab, |p, (axes, maps)| colour_step(axes, maps, p))
    }
}

fn colour_step(axes: &[[f32; 2]; 2], maps: &[Map; 2], p: [f32; 2]) -> [f32; 2] {
    let mut out = p;
    for (axis, map) in axes.iter().zip(maps) {
        let value = dot2(p, *axis);
        let moved = map.apply(value) - value;
        for (o, a) in out.iter_mut().zip(axis) {
            *o += moved * a;
        }
    }
    out
}

/// One lightness band's colour. With people in both, skin goes to the reference's skin and
/// everything else to everything else; with people only in the photograph, a face keeps its own
/// colour; without people in the photograph, everything goes to everything (`skin` is `None`).
#[derive(Debug, Clone)]
struct Band {
    skin: Option<Colour>,
    rest: Colour,
}

/// The whole transfer.
#[derive(Debug, Clone)]
pub(super) struct Transport {
    /// Lightness to the reference's lightness of the same rank: the tone curve, which never
    /// reverses, because it depends on lightness alone.
    tone: Map,
    centres: [f32; 4],
    bands: Vec<Band>,
    /// The colours of the photograph's detected skin, when skin goes to the reference's detected
    /// skin: only colours like them take the skin map.
    skin: Option<Cluster>,
}

fn is_skin(lab: [f32; 3]) -> bool {
    skin_likeness(lab) > 0.5
}

/// Where a population sits in the colour plane: its mean and the inverse of its covariance.
#[derive(Debug, Clone)]
struct Cluster {
    mean: [f32; 2],
    inverse: [[f32; 2]; 2],
}

/// Colours within this many standard deviations of the detected skin take the skin map in full,
/// and none beyond `CLUSTER_OUTER`; in between the share falls smoothly, so no edge appears.
const CLUSTER_INNER: f32 = 2.5;
const CLUSTER_OUTER: f32 = 4.0;
/// The narrowest the skin cluster is taken to be in the colour plane: a uniformly lit face is
/// tight, and its shadows and highlights still belong to it.
const CLUSTER_FLOOR: f32 = 0.012;

impl Cluster {
    fn of(colours: &[[f32; 3]]) -> Option<Self> {
        if colours.len() < MIN_POPULATION {
            return None;
        }
        let n = colours.len() as f32;
        let mean = [
            colours.iter().map(|p| p[1]).sum::<f32>() / n,
            colours.iter().map(|p| p[2]).sum::<f32>() / n,
        ];
        let (mut aa, mut ab, mut bb) = (0.0_f32, 0.0_f32, 0.0_f32);
        for p in colours {
            let (da, db) = (p[1] - mean[0], p[2] - mean[1]);
            aa += da * da;
            ab += da * db;
            bb += db * db;
        }
        let floor = CLUSTER_FLOOR * CLUSTER_FLOOR;
        let (aa, ab, bb) = (aa / n + floor, ab / n, bb / n + floor);
        let determinant = aa * bb - ab * ab;
        (determinant > 0.0).then(|| Self {
            mean,
            inverse: [
                [bb / determinant, -ab / determinant],
                [-ab / determinant, aa / determinant],
            ],
        })
    }

    /// How much a colour belongs to the cluster, from one to none.
    fn membership(&self, lab: [f32; 3]) -> f32 {
        let (da, db) = (lab[1] - self.mean[0], lab[2] - self.mean[1]);
        let [[aa, ab], [ba, bb]] = self.inverse;
        let distance = (da * (aa * da + ab * db) + db * (ba * da + bb * db))
            .max(0.0)
            .sqrt();
        let share = ((distance - CLUSTER_INNER) / (CLUSTER_OUTER - CLUSTER_INNER)).clamp(0.0, 1.0);
        1.0 - share * share * (3.0 - 2.0 * share)
    }
}

impl Transport {
    /// Learn the transfer of `photograph`'s colours onto `reference`'s. With `skin`, the people
    /// detector's skin decides who has people and which colours skin goes to; without it (a
    /// reference imported before its skin was measured), skin is recognised by its colour.
    pub(super) fn learn(
        photograph: &[[f32; 3]],
        reference: &[[f32; 3]],
        skin: Option<Skin<'_>>,
        cancel: &CancelToken,
    ) -> AuraResult<Self> {
        let tone = Map::learn_bounded(
            photograph.iter().map(|p| p[0]).collect(),
            reference.iter().map(|p| p[0]).collect(),
            MAX_TONE_SLOPE,
        );
        let sorted = |set: &[[f32; 3]]| {
            let mut l: Vec<f32> = set.iter().map(|p| p[0]).collect();
            l.sort_by(f32::total_cmp);
            l
        };
        let (own, theirs) = (sorted(photograph), sorted(reference));
        let centres = BANDS.map(|q| quantile(&own, q));
        let reference_centres = BANDS.map(|q| quantile(&theirs, q));
        let share = |set: &[[f32; 3]]| {
            set.iter().filter(|p| is_skin(**p)).count() as f32 / set.len().max(1) as f32
        };
        // Detected skin decides only for a photograph whose people show skin colour: with no one
        // found, or a black-and-white portrait whose faces are grey, skin is recognised by its
        // colour as before, so a scene is not mapped onto the reference's colours without its skin.
        let skin = skin.filter(|skin| {
            skin.photograph.len() >= MIN_POPULATION
                && skin.photograph.iter().filter(|p| is_skin(**p)).count() * 2
                    >= skin.photograph.len()
        });
        let (own_people, reference_people) = match skin {
            Some(skin) => (
                skin.photograph.len() >= MIN_POPULATION,
                skin.reference.len() >= MIN_POPULATION,
            ),
            None => (
                share(photograph) >= MIN_SKIN_SHARE,
                share(reference) >= MIN_SKIN_SHARE,
            ),
        };
        // Detected skin goes to detected skin as one population: a face spans few lightness
        // bands, and the reference's skin may sit in none of the ones the photograph's does.
        let detected = match skin {
            Some(skin) if own_people && reference_people => {
                let ab = |set: &[[f32; 3]]| -> Vec<[f32; 2]> {
                    set.iter().map(|p| [p[1], p[2]]).collect()
                };
                Some(Colour::learn(
                    &ab(skin.photograph),
                    &ab(skin.reference),
                    cancel,
                )?)
            }
            _ => None,
        };
        let mut bands = Vec::with_capacity(BANDS.len());
        for band in 0..BANDS.len() {
            check_cancel(cancel)?;
            let mine: Vec<&[f32; 3]> = photograph
                .iter()
                .filter(|p| nearest(&centres, p[0]) == band)
                .collect();
            let yours: Vec<&[f32; 3]> = reference
                .iter()
                .filter(|p| nearest(&reference_centres, p[0]) == band)
                .collect();
            // `None` takes every colour of the set; `Some(skin)` only skin or only the rest.
            let ab = |set: &[&[f32; 3]], skin: Option<bool>| -> Vec<[f32; 2]> {
                set.iter()
                    .filter(|p| skin.is_none_or(|s| is_skin(***p) == s))
                    .map(|p| [p[1], p[2]])
                    .collect()
            };
            let band = match (own_people, reference_people) {
                (true, true) => Band {
                    skin: Some(match &detected {
                        Some(colour) => colour.clone(),
                        None => {
                            Colour::learn(&ab(&mine, Some(true)), &ab(&yours, Some(true)), cancel)?
                        }
                    }),
                    rest: Colour::learn(&ab(&mine, Some(false)), &ab(&yours, Some(false)), cancel)?,
                },
                // A reference without people has no skin to give: a face keeps its own colour
                // rather than taking the rank of a leaf or a wall.
                (true, false) => Band {
                    skin: Some(Colour::identity()),
                    rest: Colour::learn(&ab(&mine, Some(false)), &ab(&yours, None), cancel)?,
                },
                (false, _) => Band {
                    skin: None,
                    rest: Colour::learn(&ab(&mine, None), &ab(&yours, None), cancel)?,
                },
            };
            bands.push(band);
        }
        Ok(Self {
            tone,
            centres,
            bands,
            skin: detected
                .is_some()
                .then(|| skin.and_then(|skin| Cluster::of(skin.photograph)))
                .flatten(),
        })
    }

    /// The tone map alone: where a lightness goes.
    pub(super) fn tone_at(&self, lightness: f32) -> f32 {
        self.tone.apply(lightness)
    }

    /// A colour through the transfer.
    pub(super) fn apply(&self, lab: [f32; 3]) -> [f32; 3] {
        let weights = memberships(&self.centres, lab[0]);
        // Detected skin's map reaches colours like that skin, not every skin-coloured curtain.
        let skin = skin_likeness(lab) * self.skin.as_ref().map_or(1.0, |c| c.membership(lab));
        let (mut a, mut b) = (0.0_f32, 0.0_f32);
        for (band, w) in self.bands.iter().zip(weights) {
            if w <= 0.0 {
                continue;
            }
            let rest = band.rest.apply([lab[1], lab[2]]);
            let face = band
                .skin
                .as_ref()
                .map_or(rest, |s| s.apply([lab[1], lab[2]]));
            a += w * (skin * face[0] + (1.0 - skin) * rest[0]);
            b += w * (skin * face[1] + (1.0 - skin) * rest[1]);
        }
        [self.tone.apply(lab[0]), a, b]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(n: usize, centre: [f32; 3], spread: [f32; 3], seed: u64) -> Vec<[f32; 3]> {
        let mut state = seed;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 33) as f32 / (1_u64 << 31) as f32) * 2.0 - 1.0
        };
        (0..n)
            .map(|_| [0, 1, 2].map(|c| centre[c] + spread[c] * next()))
            .collect()
    }

    fn moments(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
        let n = points.len() as f32;
        let mean = [0, 1, 2].map(|c| points.iter().map(|p| p[c]).sum::<f32>() / n);
        let spread = [0, 1, 2]
            .map(|c| (points.iter().map(|p| (p[c] - mean[c]).powi(2)).sum::<f32>() / n).sqrt());
        (mean, spread)
    }

    #[test]
    fn a_photograph_takes_the_reference_colour_distribution() {
        // A cool, flat photograph and a warm, contrasty reference.
        let photograph = cloud(6000, [0.55, -0.02, -0.03], [0.12, 0.02, 0.02], 7);
        let reference = cloud(6000, [0.62, 0.03, 0.05], [0.25, 0.04, 0.05], 11);
        let transport =
            Transport::learn(&photograph, &reference, None, &CancelToken::new()).unwrap();
        let moved: Vec<_> = photograph.iter().map(|p| transport.apply(*p)).collect();
        let ((mean, spread), (want_mean, want_spread)) = (moments(&moved), moments(&reference));
        for c in 0..3 {
            assert!(
                (mean[c] - want_mean[c]).abs() < 0.006,
                "mean {c}: {mean:?} vs {want_mean:?}"
            );
            assert!(
                (spread[c] - want_spread[c]).abs() < 0.12 * want_spread[c] + 0.002,
                "spread {c}: {spread:?} vs {want_spread:?}"
            );
        }
    }

    #[test]
    fn a_photograph_already_like_the_reference_barely_moves() {
        let photograph = cloud(6000, [0.5, 0.01, 0.02], [0.2, 0.03, 0.03], 3);
        let reference = cloud(6000, [0.5, 0.01, 0.02], [0.2, 0.03, 0.03], 5);
        let transport =
            Transport::learn(&photograph, &reference, None, &CancelToken::new()).unwrap();
        let worst = photograph
            .iter()
            .map(|p| {
                let q = transport.apply(*p);
                (0..3).map(|c| (q[c] - p[c]).powi(2)).sum::<f32>().sqrt()
            })
            .fold(0.0_f32, f32::max);
        assert!(worst < 0.03, "{worst}");
    }

    #[test]
    fn tone_never_reverses() {
        let photograph = cloud(6000, [0.5, 0.0, 0.01], [0.3, 0.03, 0.03], 13);
        let reference = cloud(6000, [0.45, 0.02, -0.02], [0.4, 0.06, 0.05], 17);
        let transport =
            Transport::learn(&photograph, &reference, None, &CancelToken::new()).unwrap();
        let mut last = f32::MIN;
        for i in 0..=200 {
            let l = transport.apply([i as f32 / 200.0, 0.01, 0.01])[0];
            assert!(l >= last - 1e-6, "lightness {i}: {l} after {last}");
            last = l;
        }
    }

    #[test]
    fn a_face_keeps_its_colour_when_the_reference_has_no_people() {
        // A portrait (skin-coloured face among cool greens) and a reference of blue water only.
        let mut photograph = cloud(4000, [0.6, -0.04, 0.02], [0.2, 0.02, 0.02], 19);
        let face = cloud(1500, [0.68, 0.035, 0.045], [0.04, 0.008, 0.008], 23);
        photograph.extend(&face);
        let reference = cloud(6000, [0.5, -0.03, -0.08], [0.25, 0.02, 0.03], 29);
        assert!(reference.iter().all(|p| !is_skin(*p)));
        let transport =
            Transport::learn(&photograph, &reference, None, &CancelToken::new()).unwrap();
        let moved: Vec<_> = face.iter().map(|p| transport.apply(*p)).collect();
        let ((before, _), (after, _)) = (moments(&face), moments(&moved));
        // Its colour stays (its lightness may follow the reference's tone curve).
        assert!(
            (after[1] - before[1]).abs() < 0.01,
            "{before:?} -> {after:?}"
        );
        assert!(
            (after[2] - before[2]).abs() < 0.01,
            "{before:?} -> {after:?}"
        );
    }

    #[test]
    fn detected_skin_takes_the_reference_skin_not_its_skin_coloured_walls() {
        // A daylight portrait, and a warm reference whose many yellow walls are skin-coloured
        // but whose detected skin is a deep orange.
        let mut photograph = cloud(4000, [0.45, 0.12, 0.04], [0.2, 0.03, 0.03], 31);
        let face = cloud(1500, [0.66, 0.035, 0.045], [0.05, 0.01, 0.01], 37);
        photograph.extend(&face);
        let walls = cloud(5000, [0.6, 0.04, 0.10], [0.15, 0.01, 0.01], 41);
        let skin = cloud(1000, [0.55, 0.13, 0.11], [0.08, 0.01, 0.01], 43);
        assert!(walls.iter().filter(|p| is_skin(**p)).count() > 2500);
        let mut reference = walls;
        reference.extend(&skin);
        let transport = Transport::learn(
            &photograph,
            &reference,
            Some(Skin {
                photograph: &face,
                reference: &skin,
            }),
            &CancelToken::new(),
        )
        .unwrap();
        let moved: Vec<_> = face.iter().map(|p| transport.apply(*p)).collect();
        let ((after, _), (want, _)) = (moments(&moved), moments(&skin));
        let hue = |p: [f32; 3]| p[2].atan2(p[1]).to_degrees();
        assert!(
            (hue(after) - hue(want)).abs() < 6.0,
            "face hue {:.1} vs reference skin {:.1}",
            hue(after),
            hue(want)
        );
        let chroma = |p: [f32; 3]| p[1].hypot(p[2]);
        assert!(
            chroma(after) > 0.8 * chroma(want),
            "face chroma {:.3} vs reference skin {:.3}",
            chroma(after),
            chroma(want)
        );
    }

    #[test]
    fn grey_detected_skin_leaves_skin_to_its_colour() {
        // A black-and-white portrait: the detector finds a face, but it is grey. The transfer is
        // the one it would be without detected skin.
        let photograph = cloud(6000, [0.45, 0.0, 0.0], [0.3, 0.004, 0.004], 61);
        let face = cloud(800, [0.6, 0.0, 0.0], [0.08, 0.003, 0.003], 67);
        let mut reference = cloud(5000, [0.7, -0.06, 0.0], [0.2, 0.02, 0.02], 71);
        reference.extend(cloud(1500, [0.6, 0.06, 0.08], [0.1, 0.01, 0.01], 73));
        let skin = Skin {
            photograph: &face,
            reference: &reference[5000..],
        };
        let detected =
            Transport::learn(&photograph, &reference, Some(skin), &CancelToken::new()).unwrap();
        let colour = Transport::learn(&photograph, &reference, None, &CancelToken::new()).unwrap();
        for p in photograph.iter().step_by(97) {
            assert_eq!(detected.apply(*p), colour.apply(*p), "{p:?}");
        }
    }

    #[test]
    fn a_face_keeps_its_colour_when_the_reference_shows_no_people() {
        let mut photograph = cloud(4000, [0.6, -0.04, 0.02], [0.2, 0.02, 0.02], 47);
        let face = cloud(1500, [0.68, 0.035, 0.045], [0.04, 0.008, 0.008], 53);
        photograph.extend(&face);
        // Warm sand: skin-coloured, but nobody in it.
        let reference = cloud(6000, [0.6, 0.03, 0.09], [0.2, 0.02, 0.02], 59);
        let transport = Transport::learn(
            &photograph,
            &reference,
            Some(Skin {
                photograph: &face,
                reference: &[],
            }),
            &CancelToken::new(),
        )
        .unwrap();
        let moved: Vec<_> = face.iter().map(|p| transport.apply(*p)).collect();
        let ((before, _), (after, _)) = (moments(&face), moments(&moved));
        assert!(
            (after[1] - before[1]).abs() < 0.01,
            "{before:?} -> {after:?}"
        );
        assert!(
            (after[2] - before[2]).abs() < 0.01,
            "{before:?} -> {after:?}"
        );
    }

    #[test]
    fn no_map_is_steep_enough_to_band() {
        // Every photograph value in one narrow range, the reference spread wide.
        let map = Map::learn(
            (0..2000).map(|i| 0.5 + 0.0001 * (i % 10) as f32).collect(),
            (0..2000).map(|i| i as f32 / 2000.0).collect(),
        );
        for k in 1..KNOTS {
            let dx = map.from[k] - map.from[k - 1];
            let dy = map.to[k] - map.to[k - 1];
            assert!(dy <= dx * MAX_SLOPE + 1e-6, "segment {k}: {dy} over {dx}");
        }
    }
}
