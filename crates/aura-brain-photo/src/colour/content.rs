//! What is in a frame, as far as its colours can say.
//!
//! PHASE-16 section 6.2 asks the HSL solver to work "by content, not by preset": cluster the
//! image into greenery, sky, skin, dress, wood and decor, measure each one's hue and
//! saturation, and adjust toward pleasing targets rather than applying a fixed look.
//!
//! ## The honest version of "segmentation cues"
//!
//! Section 6.2's word is *segmentation*. There is no segmentation model in this build and
//! phase 18 is where masks are made, so this module infers the bands from **hue, saturation,
//! luminance and position** instead - sky is blue, bright, unsaturated and above the middle;
//! greenery is green-yellow and saturated; a dress is bright and near-neutral; wood is a
//! warm mid-tone; decor is whatever is left and saturated.
//!
//! Every band therefore carries a [`BandReading::confidence`], every decision that acts on
//! one also carries `ColourCode::ContentInferred`, and the adjustment a band can receive is
//! *scaled by* that confidence. ADR-0033 decision 4 records why shipping this beats waiting:
//! phase 16 unlocks phases 17, 25 and 27, and a phase that ships nothing until a later phase
//! exists has not shipped.
//!
//! When phase 18's masks arrive, [`read`] gets a better input and nothing above it moves.
//!
//! ## Skin is read and never adjusted
//!
//! [`ContentBand::Skin`] is measured here because the panel has to be able to show what the
//! guard protected, and because [`Reading::skin_area`] is the denominator the skin guard's
//! own confidence uses. It has no target in `tone_intent.toml`, no branch in
//! [`crate::colour::hsl`], and `ContentBand::is_protected` is checked before any band is
//! written.

use aura_core::contract::colour::{BandReading, ContentBand};
use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::FaceRef;

/// How many pixels are skipped between samples.
///
/// Two, matching `tone::stats::STRIDE`. A content band is an area statistic and the answer
/// over a quarter of a 2048 px proxy is a quarter of a million samples, which is far more
/// than a hue histogram needs and cheap enough not to argue about.
pub const STRIDE: usize = 2;

/// Below this confidence a band is reported and never adjusted.
///
/// A third. Below it the inference is closer to a guess than to a reading, and section 12's
/// fourth failure mode - "HSL fights the photographer's taste" - is what an adjustment made
/// on a guess looks like from the other side of the screen.
pub const MIN_CONFIDENCE: f32 = 0.33;

/// The smallest fraction of the frame a band must cover to be reported at all.
///
/// One percent. Below that an adjustment aimed at the band is invisible and the hue estimate
/// over so few samples is noise.
pub const MIN_AREA: f32 = 0.01;

/// The saturation below which a bright region is called a dress rather than a colour.
pub const DRESS_SATURATION: f32 = 0.10;

/// The luminance a dress region must exceed.
pub const DRESS_LUMA: f32 = 0.55;

/// What one band's samples added up to.
#[derive(Debug, Clone, Copy, Default)]
struct Accumulator {
    samples: u32,
    /// Sum of `cos(hue)` and `sin(hue)`, for a circular mean. A linear mean of hue angles
    /// puts the average of 350 and 10 degrees at 180, which is the opposite colour.
    cos_sum: f64,
    sin_sum: f64,
    sat_sum: f64,
    luma_sum: f64,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl Accumulator {
    fn new() -> Self {
        Self {
            min_x: 1.0,
            min_y: 1.0,
            max_x: 0.0,
            max_y: 0.0,
            ..Self::default()
        }
    }

    fn add(&mut self, hue_deg: f32, saturation: f32, luma: f32, x: f32, y: f32) {
        let radians = f64::from(hue_deg.to_radians());
        self.samples += 1;
        self.cos_sum += radians.cos();
        self.sin_sum += radians.sin();
        self.sat_sum += f64::from(saturation);
        self.luma_sum += f64::from(luma);
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    /// The circular mean hue, the mean saturation and the mean luminance.
    fn means(&self) -> (f32, f32, f32) {
        if self.samples == 0 {
            return (0.0, 0.0, 0.0);
        }
        let count = f64::from(self.samples);
        let hue = self
            .sin_sum
            .atan2(self.cos_sum)
            .to_degrees()
            .rem_euclid(360.0) as f32;
        (
            hue,
            (self.sat_sum / count) as f32,
            (self.luma_sum / count) as f32,
        )
    }

    /// How tightly the hues cluster, `0..1`.
    ///
    /// The resultant length of the circular mean. One means every sample had the same hue;
    /// zero means they are spread around the wheel, which is what a band assembled out of
    /// unrelated things looks like.
    fn cohesion(&self) -> f32 {
        if self.samples == 0 {
            return 0.0;
        }
        let count = f64::from(self.samples);
        let resultant = (self.cos_sum / count).hypot(self.sin_sum / count);
        resultant.clamp(0.0, 1.0) as f32
    }

    fn bounds(&self) -> CropRect {
        if self.samples == 0 {
            return CropRect::FULL;
        }
        CropRect {
            x: self.min_x,
            y: self.min_y,
            w: (self.max_x - self.min_x).max(0.01),
            h: (self.max_y - self.min_y).max(0.01),
        }
        .clamped()
    }
}

/// The most distracting saturated thing in a frame that is not the subject.
///
/// Section 2.1's fluorescent green exit sign, as a measurement. It is a *region* rather than
/// a band, because the whole argument of section 6.2 is that the answer to one distracting
/// colour is to reduce that colour and not to desaturate the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distractor {
    /// Its mean hue, in degrees.
    pub hue_deg: f32,
    /// Its mean saturation, `0..1`.
    pub saturation: f32,
    /// The fraction of the frame it covers.
    pub area: f32,
    /// Where it is, for the reason's evidence crop.
    pub region: CropRect,
    /// How far it sits from the subject's own hue, in degrees. Larger conflicts more.
    pub hue_distance_deg: f32,
}

/// What one frame's colours say about what is in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// One entry per band that met [`MIN_AREA`], largest area first.
    pub bands: Vec<BandReading>,
    /// The most distracting saturated region, when there is one.
    pub distractor: Option<Distractor>,
    /// The fraction of the frame the faces cover.
    pub skin_area: f32,
    /// The subject's own mean hue, in degrees, when there is a subject.
    ///
    /// The harmony objective is "reduce hue conflicts with the subject", so there has to be
    /// a subject hue to conflict with. A frame with no faces has none and the objective
    /// falls back to taming whatever is most saturated.
    pub subject_hue_deg: Option<f32>,
    /// How much of the frame was classified into any band at all, `0..1`.
    ///
    /// The coverage figure behind every band confidence. A frame that is 90 % unclassified
    /// is a frame whose colour statistics said very little.
    pub classified: f32,
}

impl Reading {
    /// One band's reading, when it was found.
    #[must_use]
    pub fn band(&self, band: ContentBand) -> Option<BandReading> {
        self.bands.iter().copied().find(|entry| entry.band == band)
    }

    /// True when a band was found and is confident enough to act on.
    #[must_use]
    pub fn actionable(&self, band: ContentBand) -> bool {
        !band.is_protected()
            && self
                .band(band)
                .is_some_and(|entry| entry.confidence >= MIN_CONFIDENCE)
    }

    /// Every band that was found but is not confident enough to act on.
    ///
    /// What `ColourCode::BandUnconfident` is emitted for. Reported rather than dropped,
    /// because "AURA saw greenery and was not sure enough to touch it" and "AURA saw no
    /// greenery" are different sentences and only one of them is about this photograph.
    #[must_use]
    pub fn unconfident(&self) -> Vec<ContentBand> {
        self.bands
            .iter()
            .filter(|entry| !entry.band.is_protected() && entry.confidence < MIN_CONFIDENCE)
            .map(|entry| entry.band)
            .collect()
    }
}

/// Read one decoded proxy's content bands.
///
/// `faces` comes from phase 06 through the frozen service, so the skin band is *where the
/// faces are* rather than wherever the hue happens to look like skin - which is the
/// difference between protecting a person and protecting a wooden door.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn read(
    rgb: &[u8],
    width: usize,
    height: usize,
    table: &[f32; 256],
    faces: &[FaceRef],
) -> Reading {
    let mut accumulators = [Accumulator::new(); ContentBand::COUNT];
    let mut counted: u32 = 0;
    let mut classified: u32 = 0;

    // The face boxes, in normalised coordinates, so the test is a rectangle containment
    // rather than a per-pixel mask. A face box is generous by construction - phase 06 pads
    // it - and a generous skin region is the conservative direction for a guard.
    let boxes: Vec<CropRect> = faces.iter().map(|face| face.bbox.clamped()).collect();

    for y in (0..height).step_by(STRIDE) {
        let ny = if height <= 1 {
            0.0
        } else {
            y as f32 / (height - 1) as f32
        };
        for x in (0..width).step_by(STRIDE) {
            let index = (y * width + x) * 3;
            let (Some(r), Some(g), Some(b)) = (
                rgb.get(index).and_then(|v| table.get(*v as usize)).copied(),
                rgb.get(index + 1)
                    .and_then(|v| table.get(*v as usize))
                    .copied(),
                rgb.get(index + 2)
                    .and_then(|v| table.get(*v as usize))
                    .copied(),
            ) else {
                continue;
            };
            counted += 1;
            let nx = if width <= 1 {
                0.0
            } else {
                x as f32 / (width - 1) as f32
            };
            let (hue, saturation, luma) = hsl_of(r, g, b);

            let band = if boxes.iter().any(|area| contains(area, nx, ny)) {
                Some(ContentBand::Skin)
            } else {
                classify(hue, saturation, luma, ny)
            };
            let Some(band) = band else { continue };
            classified += 1;
            if let Some(slot) = accumulators.get_mut(band as usize) {
                slot.add(hue, saturation, luma, nx, ny);
            }
        }
    }

    let total = f32::from(u8::from(counted > 0)) * counted.max(1) as f32;
    let classified_fraction = if counted == 0 {
        0.0
    } else {
        classified as f32 / counted as f32
    };

    let mut bands: Vec<BandReading> = Vec::new();
    for band in ContentBand::ALL {
        let Some(accumulator) = accumulators.get(band as usize) else {
            continue;
        };
        if accumulator.samples == 0 {
            continue;
        }
        let area = accumulator.samples as f32 / total.max(1.0);
        if area < MIN_AREA {
            continue;
        }
        let (hue, saturation, luma) = accumulator.means();
        bands.push(BandReading {
            band,
            area,
            hue_deg: hue,
            saturation,
            luma,
            confidence: confidence_of(band, area, accumulator.cohesion(), saturation),
        });
    }
    bands.sort_by(|a, b| b.area.total_cmp(&a.area).then(a.band.cmp(&b.band)));

    let skin_area = bands
        .iter()
        .find(|entry| entry.band == ContentBand::Skin)
        .map_or(0.0, |entry| entry.area);
    let subject_hue = bands
        .iter()
        .find(|entry| entry.band == ContentBand::Skin)
        .map(|entry| entry.hue_deg);

    let distractor = find_distractor(&bands, &accumulators, subject_hue);

    Reading {
        bands,
        distractor,
        skin_area,
        subject_hue_deg: subject_hue,
        classified: classified_fraction,
    }
}

/// Hue in degrees, saturation and luminance from a linear triple.
///
/// The classification runs in **linear** light, invariant 8, even though it is a statistic
/// rather than a transform: a hue measured after a transfer curve depends on how bright the
/// pixel was, and a foliage classifier that changes its mind in the shadows is a classifier
/// that produces a different answer for the same tree at two exposures.
#[must_use]
pub fn hsl_of(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if delta <= 1e-6 || max <= 1e-6 {
        return (0.0, 0.0, luma.clamp(0.0, 1.0));
    }
    let hue = if (max - r).abs() < 1e-9 {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-9 {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } * 60.0;
    (hue.rem_euclid(360.0), delta / max, luma.clamp(0.0, 1.0))
}

/// Which band one sample belongs to, or none.
///
/// The order matters and is deliberate: dress is tested before wood because a warm-lit white
/// tablecloth is both, and calling it a dress is the answer that protects it. Sky is tested
/// before decor because a deep blue-hour sky is saturated enough to be mistaken for
/// uplighting, and a sky that has been tamed as a distractor is the mistake nobody forgives.
#[must_use]
fn classify(hue: f32, saturation: f32, luma: f32, ny: f32) -> Option<ContentBand> {
    if saturation < DRESS_SATURATION {
        return if luma >= DRESS_LUMA {
            Some(ContentBand::Dress)
        } else {
            None
        };
    }
    if (180.0..=265.0).contains(&hue) && saturation <= 0.70 && luma >= 0.18 && ny <= 0.60 {
        return Some(ContentBand::Sky);
    }
    if (65.0..=175.0).contains(&hue) && saturation >= 0.12 {
        return Some(ContentBand::Greenery);
    }
    if (15.0..=65.0).contains(&hue) && (0.10..=0.60).contains(&saturation) && luma <= 0.62 {
        return Some(ContentBand::Wood);
    }
    if saturation >= 0.40 {
        return Some(ContentBand::Decor);
    }
    None
}

/// How much to trust one band's reading.
///
/// Three terms, multiplied so no one of them can rescue the others - the same geometric
/// discipline phase 09's technical score and phase 12's keep score use:
///
/// 1. **Area.** A band covering a fiftieth of the frame is a guess; one covering a fifth is
///    a reading. Saturating at 20 %.
/// 2. **Cohesion, squared.** How tightly the band's hues cluster. A "greenery" band assembled
///    out of a hedge and a green exit sign has low cohesion and must not be adjusted as one
///    thing - so this is the term that is weighted twice. It is the only one of the three
///    that answers "are these samples the same object", and every failure mode of an inferred
///    band is a failure of that question rather than of area or of saturation.
/// 3. **Separation.** How far the band's saturation is from the threshold that admitted it.
///    A sample set that only just qualified is one a different frame would have classified
///    differently.
#[must_use]
fn confidence_of(band: ContentBand, area: f32, cohesion: f32, saturation: f32) -> f32 {
    let area_term = (area / 0.20).clamp(0.0, 1.0);
    let separation = match band {
        // A dress is defined by the *absence* of saturation, so its separation runs the
        // other way: the less saturated it is, the more certain the call.
        ContentBand::Dress => ((DRESS_SATURATION - saturation) / DRESS_SATURATION).clamp(0.0, 1.0),
        // Skin is not inferred from colour at all - it is where phase 06 says the faces are -
        // so there is nothing to be uncertain about beyond how much of it there is.
        ContentBand::Skin => 1.0,
        _ => ((saturation - 0.10) / 0.30).clamp(0.0, 1.0),
    };
    let coherence = cohesion.clamp(0.0, 1.0).powi(2);
    (area_term * coherence * separation.max(0.05))
        .powf(1.0 / 3.0)
        .clamp(0.0, 1.0)
}

/// The most distracting saturated region, or none.
///
/// Scored as `saturation * area_weight * hue_distance`, and every term earns its place:
/// a saturated thing that covers two pixels is not distracting, a large pale thing is not
/// either, and a saturated thing the same hue as the subject is *harmony* rather than
/// competition - which is the whole difference between section 6.2's objective and a
/// saturation slider.
///
/// **Sky and dress can never be the distractor.** A sky is the reason for the photograph
/// often enough that taming it is never the right default, and a dress is what the guard
/// exists to keep neutral.
#[must_use]
fn find_distractor(
    bands: &[BandReading],
    accumulators: &[Accumulator; ContentBand::COUNT],
    subject_hue: Option<f32>,
) -> Option<Distractor> {
    let mut best: Option<(f32, Distractor)> = None;
    for reading in bands {
        if matches!(
            reading.band,
            ContentBand::Skin | ContentBand::Sky | ContentBand::Dress
        ) {
            continue;
        }
        let distance = subject_hue.map_or(180.0, |hue| hue_distance(reading.hue_deg, hue));
        let area_weight = (reading.area / 0.10).clamp(0.0, 1.0);
        let score = reading.saturation * area_weight * (distance / 180.0);
        let region = accumulators
            .get(reading.band as usize)
            .map_or(CropRect::FULL, Accumulator::bounds);
        let candidate = Distractor {
            hue_deg: reading.hue_deg,
            saturation: reading.saturation,
            area: reading.area,
            region,
            hue_distance_deg: distance,
        };
        if best.as_ref().is_none_or(|(current, _)| score > *current) {
            best = Some((score, candidate));
        }
    }
    best.map(|(_, distractor)| distractor)
}

/// The shortest angle between two hues, in degrees, `0..180`.
#[must_use]
pub fn hue_distance(a: f32, b: f32) -> f32 {
    let difference = (a - b).rem_euclid(360.0);
    if difference > 180.0 {
        360.0 - difference
    } else {
        difference
    }
}

/// True when a normalised point is inside a rectangle.
fn contains(area: &CropRect, x: f32, y: f32) -> bool {
    x >= area.x && x <= area.x + area.w && y >= area.y && y <= area.y + area.h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_circular_mean_does_not_average_red_into_cyan() {
        // The bug this accumulator exists to avoid. A linear mean of 350 and 10 degrees is
        // 180 - the opposite colour - and a "red" band reported as cyan would send the
        // solver to adjust the wrong band entirely.
        let mut accumulator = Accumulator::new();
        accumulator.add(350.0, 0.5, 0.5, 0.0, 0.0);
        accumulator.add(10.0, 0.5, 0.5, 1.0, 1.0);
        let (hue, _, _) = accumulator.means();
        assert!(
            !(1.0..=359.0).contains(&hue),
            "circular mean landed at {hue}"
        );
    }

    #[test]
    fn a_spread_of_hues_has_low_cohesion() {
        let mut accumulator = Accumulator::new();
        for step in 0..8 {
            accumulator.add(step as f32 * 45.0, 0.5, 0.5, 0.5, 0.5);
        }
        assert!(accumulator.cohesion() < 0.05);
    }

    #[test]
    fn a_bright_neutral_is_a_dress_and_a_dark_one_is_nothing() {
        assert_eq!(classify(30.0, 0.02, 0.80, 0.5), Some(ContentBand::Dress));
        assert_eq!(classify(30.0, 0.02, 0.20, 0.5), None);
    }

    #[test]
    fn a_deep_sky_is_never_read_as_decor() {
        // The one misclassification that would be visibly wrong in the delivered gallery.
        assert_eq!(classify(220.0, 0.55, 0.45, 0.20), Some(ContentBand::Sky));
    }

    #[test]
    fn foliage_is_greenery_at_the_hues_a_camera_actually_records() {
        // Cameras record foliage between about 70 and 110 degrees - yellow-green - which is
        // exactly why section 6.2 calls it the classic wedding problem.
        for hue in [70.0_f32, 90.0, 110.0, 140.0] {
            assert_eq!(
                classify(hue, 0.35, 0.30, 0.7),
                Some(ContentBand::Greenery),
                "hue {hue} was not read as foliage"
            );
        }
    }

    #[test]
    fn confidence_needs_all_three_terms() {
        // Geometric, so a huge area cannot rescue an incoherent band.
        let coherent = confidence_of(ContentBand::Greenery, 0.30, 0.95, 0.40);
        let incoherent = confidence_of(ContentBand::Greenery, 0.30, 0.05, 0.40);
        assert!(coherent > 0.8, "a clean band should be trusted: {coherent}");
        assert!(
            incoherent < MIN_CONFIDENCE,
            "an incoherent band must not be actionable: {incoherent}"
        );
    }
}
