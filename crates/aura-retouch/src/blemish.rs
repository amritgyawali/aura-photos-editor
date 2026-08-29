//! Finding the marks on a face, and deciding which of them are temporary.
//!
//! PHASE-20 section 6.1, first bullet: "find skin anomalies, then classify each as temporary or
//! permanent". This module does the first half and hands each candidate to [`crate::permanent`]
//! for the second.
//!
//! ## What a blemish is, measured
//!
//! A pimple, a spot, a patch of temporary redness or a small scratch is, in the pixels:
//!
//! * **compact** - a few samples across, not a region;
//! * **isolated** - the skin around it is even, which is what separates a spot from the shadow
//!   edge of a nostril;
//! * **mid-frequency** - larger than a pore and smaller than the modelling of the face, so it
//!   lands in the band [`aura_render::bands`] calls `mid`;
//! * **red** - it sits on the red side of the chromaticity of the skin around it, because what
//!   makes a spot visible is inflammation rather than darkness.
//!
//! The fourth is what carries most of the temporary-versus-permanent decision in a single
//! frame, and `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` section 7 records
//! why this is a measurement rather than a network: the shipped detector head is untrained, and
//! a phase that refused to consult its placeholder *and* had nothing underneath would ship a
//! retoucher that finds nothing. A difference-of-Gaussians with a colour test finds fewer marks
//! than a trained network would, and the ones it misses stay on the photograph - which is the
//! direction every uncertainty in this phase falls in.
//!
//! ## The threshold is relative to the skin it is on
//!
//! There is no absolute redness constant and no absolute contrast constant here. Both are
//! measured against the median and the deviation of *this face own skin*, which is phase 15
//! rule and the reason a detector tuned on one skin tone does not quietly stop working on
//! another. `docs/skin-fairness.md` says the same thing in the product voice.

use aura_core::contract::composition::Box2;
use aura_core::contract::retouch::{MAX_BLEMISH_FRACTION, PERMANENT_FLOOR, TEMPORARY_FLOOR};
use aura_render::bands;

/// How many deviations above the skin own mid-band spread a sample must sit to be a candidate.
///
/// Three. Two finds pore clusters and shadow edges on every face in a wedding; four finds only
/// the marks a photographer would have removed by hand anyway and misses the ones they would
/// have noticed on a print.
pub const MID_SIGMA: f32 = 3.0;

/// The smallest candidate, as a fraction of the shorter side of the face, worth removing.
///
/// A two-hundredth of a face - about two pixels on a face that fills a 2048 px proxy. Below this
/// the mark is at the scale of a pore, and a retoucher that removes pores is the failure this
/// whole phase is built to avoid.
pub const MIN_BLEMISH_FRACTION: f32 = 1.0 / 200.0;

/// The smallest mid-band luminance excursion, as a fraction of the luminance of the skin.
///
/// One per cent. The relative threshold alone is not enough: on a face with no marks at all the
/// median absolute deviation of the mid band is the deviation of *pores*, and three of those is
/// still a pore. An absolute floor is what makes "even skin produces nothing" true rather than
/// nearly true.
pub const MIN_CONTRAST: f32 = 0.01;

/// The smallest mid-band excursion in the red share of the chromaticity.
///
/// Six thousandths. **The detector reads colour as well as luminance, and this is why**: an
/// inflamed spot is often no darker or brighter than the skin it sits on - it is *redder*, and a
/// detector that only band-passed luminance would miss the most common blemish on a wedding
/// face while finding every shadow edge. That is not a hypothetical: this module was written
/// with a luminance-only threshold first and the fixture spot it exists to find went straight
/// past it.
pub const MIN_RED_CONTRAST: f32 = 0.006;

/// How much redder than the surrounding skin a mark must be to read as inflammation.
///
/// Measured as the difference in the red share of the chromaticity of the mark, against the
/// median of the face, and departure-weighted so the core of the mark carries the reading.
///
/// Two hundredths. An inflamed spot sits three to six hundredths above its own skin at the
/// centre and about two once the falloff is weighted in; a mole sits below it. The constant is
/// what the two are scored against, so it is calibrated at "clearly inflamed" rather than at
/// "unmistakable" - a threshold set at the unmistakable end leaves every ordinary spot in the
/// undecided band, which reads as a retoucher that does nothing.
pub const REDNESS_MARGIN: f32 = 0.02;

/// One thing found on a face, before anything has decided what to do about it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// Where it is, in frame coordinates.
    pub area: Box2,
    /// How likely it is to be temporary, `0..1`.
    ///
    /// Above [`TEMPORARY_FLOOR`] it may be removed. Below [`PERMANENT_FLOOR`] it is offered to
    /// the protect set. Between the two nothing happens and the plan says
    /// [`aura_core::contract::retouch::RetouchCode::AnomalyUncertain`] - which is most marks on
    /// most faces, deliberately.
    pub temporary: f32,
    /// How sure the detector is that this is a mark at all, `0..1`.
    pub confidence: f32,
    /// How much redder than the surrounding skin it is.
    ///
    /// Positive is redder. A negative value is a *darker, less saturated* mark, which is what a
    /// mole looks like and is the main single-frame evidence for permanence.
    pub redness: f32,
    /// How large it is, as a fraction of the shorter side of the frame.
    pub size_frac: f32,
    /// True when it is larger than [`MAX_BLEMISH_FRACTION`] of the face.
    ///
    /// Kept as a flag rather than filtered out, because a candidate this phase declines to touch
    /// is something a photographer is told about rather than something that vanishes.
    pub too_large: bool,
}

impl Candidate {
    /// True when this may be removed.
    #[must_use]
    pub fn is_removable(&self) -> bool {
        !self.too_large && self.temporary >= TEMPORARY_FLOOR
    }

    /// True when this should be offered to the protect set.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        1.0 - self.temporary >= PERMANENT_FLOOR
    }
}

/// One face, as this module reads it: a rectangle of linear RGB and where it sits in the frame.
#[derive(Debug, Clone)]
pub struct FaceCrop {
    /// Interleaved linear RGB, `width * height * 3`.
    pub rgb: Vec<f32>,
    /// Width in samples.
    pub width: usize,
    /// Height in samples.
    pub height: usize,
    /// Skin coverage per sample, `0..1`, from phase 18.
    pub skin: Vec<f32>,
    /// Where the crop sits in the frame, normalised.
    pub bounds: Box2,
}

impl FaceCrop {
    /// The three channels at one sample, or black when the index is past the end.
    ///
    /// Written without indexing because this crate denies `clippy::indexing_slicing` in library
    /// code: a crop shorter than its own dimensions is a bug, and the right shape for one is a
    /// black sample rather than a panic in the middle of a wedding.
    #[must_use]
    pub fn rgb_at(&self, index: usize) -> [f32; 3] {
        let slot = index * 3;
        let mut out = [0.0f32; 3];
        if let Some(values) = self.rgb.get(slot..slot + 3) {
            for (channel, value) in out.iter_mut().zip(values.iter()) {
                *channel = *value;
            }
        }
        out
    }

    /// Linear luminance at one sample.
    #[must_use]
    pub fn luma_at(&self, index: usize) -> f32 {
        let rgb = self.rgb_at(index);
        0.262_700f32.mul_add(rgb[0], 0.677_998f32.mul_add(rgb[1], 0.059_302 * rgb[2]))
    }

    /// The red share of the chromaticity at one sample.
    #[must_use]
    pub fn redness_at(&self, index: usize) -> f32 {
        let rgb = self.rgb_at(index);
        let total = rgb[0] + rgb[1] + rgb[2];
        if total <= 1e-6 {
            return 0.0;
        }
        rgb[0] / total
    }

    /// Skin coverage at one sample.
    #[must_use]
    pub fn skin_at(&self, index: usize) -> f32 {
        self.skin.get(index).copied().unwrap_or(0.0)
    }

    /// True when there is enough skin here to measure anything against.
    #[must_use]
    pub fn has_skin(&self) -> bool {
        self.skin.iter().any(|w| *w > 0.0)
    }
}

/// Find the marks on one face.
///
/// Deterministic in every respect: the scan order is row-major, components are grown in a fixed
/// order and the returned list is sorted by position. Invariant 4.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn detect(crop: &FaceCrop) -> Vec<Candidate> {
    if crop.width == 0 || crop.height == 0 || !crop.has_skin() {
        return Vec::new();
    }

    let mut luma = Vec::with_capacity(crop.width * crop.height);
    let mut red = Vec::with_capacity(crop.width * crop.height);
    for index in 0..crop.width * crop.height {
        luma.push(crop.luma_at(index));
        red.push(crop.redness_at(index));
    }
    let decomposed = bands::separate(&luma, crop.width, crop.height);
    let chromatic = bands::separate(&red, crop.width, crop.height);

    // The spread of this face own mid band, over its own skin. Median absolute deviation rather
    // than a standard deviation, because a face with one large mark on it would otherwise raise
    // its own threshold until the mark no longer crossed it.
    let mut mid_samples: Vec<f32> = (0..crop.width * crop.height)
        .filter(|index| crop.skin_at(*index) > 0.5)
        .map(|index| decomposed.mid.get(index).copied().unwrap_or(0.0).abs())
        .collect();
    if mid_samples.len() < 64 {
        return Vec::new();
    }
    let spread = median(&mut mid_samples).max(1e-5);
    let mut skin_luma: Vec<f32> = (0..crop.width * crop.height)
        .filter(|index| crop.skin_at(*index) > 0.5)
        .map(|index| crop.luma_at(index))
        .collect();
    let level = median(&mut skin_luma).max(1e-4);
    let threshold = (spread * MID_SIGMA).max(level * MIN_CONTRAST);

    let mut red_samples: Vec<f32> = (0..crop.width * crop.height)
        .filter(|index| crop.skin_at(*index) > 0.5)
        .map(|index| chromatic.mid.get(index).copied().unwrap_or(0.0).abs())
        .collect();
    let red_threshold = (median(&mut red_samples).max(1e-6) * MID_SIGMA).max(MIN_RED_CONTRAST);

    let mut red_samples: Vec<f32> = (0..crop.width * crop.height)
        .filter(|index| crop.skin_at(*index) > 0.5)
        .map(|index| crop.redness_at(index))
        .collect();
    let median_redness = median(&mut red_samples);

    // **One sign at a time.** A mark is an excursion of one sign - a spot in shadow is
    // negative, an inflamed one is positive - and around it the mid band swings the *other* way,
    // because that is what a band-pass does at an edge. Growing components over `|mid|` merges
    // the mark with its own shoulder, and the shoulder is ordinary skin: including it in the
    // colour measurement dilutes exactly the signal that separates a spot from a mole. The first
    // implementation did that and read a clearly inflamed spot as undecided.
    let mut out = Vec::new();
    let face_side = crop.width.min(crop.height) as f32;
    for sign in [1.0f32, -1.0f32] {
        let mut marked = vec![false; crop.width * crop.height];
        for index in 0..crop.width * crop.height {
            if crop.skin_at(index) <= 0.5 {
                continue;
            }
            let mid = decomposed.mid.get(index).copied().unwrap_or(0.0);
            let chroma = chromatic.mid.get(index).copied().unwrap_or(0.0);
            // Either signal, and both are read with the same sign convention: a redder-than-skin
            // excursion and a brighter-than-skin one are both positive.
            if mid * sign >= threshold || chroma * sign >= red_threshold {
                if let Some(slot) = marked.get_mut(index) {
                    *slot = true;
                }
            }
        }

        let mut visited = vec![false; marked.len()];
        for start in 0..marked.len() {
            if !marked.get(start).copied().unwrap_or(false)
                || visited.get(start).copied().unwrap_or(true)
            {
                continue;
            }
            let component = grow(&marked, &mut visited, start, crop.width, crop.height);
            if component.samples.is_empty() {
                continue;
            }
            let w = (component.x1 - component.x0 + 1) as f32;
            let h = (component.y1 - component.y0 + 1) as f32;
            let side = w.max(h);
            let fraction = side / face_side.max(1.0);
            if fraction < MIN_BLEMISH_FRACTION {
                continue;
            }

            // **Weighted by how far each sample departs from the skin.** A component includes the
            // falloff of the mark as well as its core, and a sample in the falloff is half skin: a
            // plain mean over the component therefore reads a clearly inflamed spot as half
            // inflamed, and the stronger the mark the more falloff it has and the *worse* the
            // reading gets. That is not hypothetical either - this module measured a plain mean
            // first, and painting the fixture spot brighter made its temporary probability go down.
            let mut redness = 0.0f32;
            let mut amplitude = 0.0f32;
            let mut weight = 0.0f32;
            for index in &component.samples {
                let luma_mid = decomposed.mid.get(*index).copied().unwrap_or(0.0).abs();
                let chroma_mid = chromatic.mid.get(*index).copied().unwrap_or(0.0).abs();
                // The two signals in one unit: each expressed as a multiple of its own floor, so a
                // mark that is purely chromatic and one that is purely tonal weigh the same.
                let departure =
                    luma_mid / (level * MIN_CONTRAST).max(1e-6) + chroma_mid / MIN_RED_CONTRAST;
                redness += crop.redness_at(*index) * departure;
                amplitude +=
                    luma_mid.max(chroma_mid * level / MIN_RED_CONTRAST * MIN_CONTRAST) * departure;
                weight += departure;
            }
            let weight = weight.max(1e-6);
            let redness = redness / weight - median_redness;
            let amplitude = amplitude / weight;

            // How compact it is: a round mark fills most of its own box, a shadow edge does not.
            let compactness = (component.samples.len() as f32 / (w * h).max(1.0)).clamp(0.0, 1.0);

            out.push(Candidate {
                area: Box2 {
                    x: crop.bounds.x + crop.bounds.w * (component.x0 as f32 / crop.width as f32),
                    y: crop.bounds.y + crop.bounds.h * (component.y0 as f32 / crop.height as f32),
                    w: crop.bounds.w * (w / crop.width as f32),
                    h: crop.bounds.h * (h / crop.height as f32),
                }
                .clamped(),
                temporary: temporary_probability(redness, compactness),
                confidence: confidence(amplitude, threshold, compactness),
                redness,
                size_frac: fraction * crop.bounds.w.max(crop.bounds.h),
                too_large: fraction > MAX_BLEMISH_FRACTION,
            });
        }
    }

    out.sort_by(|a, b| {
        a.area
            .y
            .total_cmp(&b.area.y)
            .then(a.area.x.total_cmp(&b.area.x))
    });
    out
}

/// How likely a mark is to be temporary, from its colour and its shape.
///
/// Redness dominates and compactness modifies. An inflamed spot is redder than the skin it sits
/// on; a mole is *less* red and usually darker; a scratch is red and elongated. Half is the
/// answer when nothing distinguishes it, and half is below [`TEMPORARY_FLOOR`] - so an
/// undistinguished mark is left alone, which is the whole ethical posture of the phase written
/// as an arithmetic default.
#[must_use]
pub fn temporary_probability(redness: f32, compactness: f32) -> f32 {
    let colour = ((redness / REDNESS_MARGIN) * 0.5).clamp(-1.0, 1.0);
    let shape = (compactness - 0.55) * 0.4;
    (0.5 + colour * 0.42 + shape).clamp(0.0, 1.0)
}

/// How sure the detector is that this is a mark at all.
#[must_use]
fn confidence(amplitude: f32, threshold: f32, compactness: f32) -> f32 {
    let strength = (amplitude / threshold.max(1e-6) - 1.0).clamp(0.0, 1.0);
    (0.45 + strength * 0.35 + compactness * 0.2).clamp(0.0, 1.0)
}

/// One connected component of marked samples.
struct Component {
    samples: Vec<usize>,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

/// Four-connected flood fill, iterative and in a fixed order.
fn grow(
    marked: &[bool],
    visited: &mut [bool],
    start: usize,
    width: usize,
    height: usize,
) -> Component {
    let mut stack = vec![start];
    let mut samples = Vec::new();
    let (mut x0, mut y0) = (width, height);
    let (mut x1, mut y1) = (0usize, 0usize);

    while let Some(index) = stack.pop() {
        if visited.get(index).copied().unwrap_or(true) {
            continue;
        }
        if !marked.get(index).copied().unwrap_or(false) {
            continue;
        }
        if let Some(slot) = visited.get_mut(index) {
            *slot = true;
        }
        samples.push(index);

        let x = index % width;
        let y = index / width;
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);

        if x > 0 {
            stack.push(index - 1);
        }
        if x + 1 < width {
            stack.push(index + 1);
        }
        if y > 0 {
            stack.push(index - width);
        }
        if y + 1 < height {
            stack.push(index + width);
        }
    }

    samples.sort_unstable();
    Component {
        samples,
        x0,
        y0,
        x1,
        y1,
    }
}

/// The median of a slice, which the caller is allowed to have reordered.
fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn a_red_spot_is_found_and_reads_as_temporary() {
        let crop = fixtures::face_with_blemish();
        let found = detect(&crop);
        assert!(!found.is_empty(), "the spot was not found");
        let spot = found
            .iter()
            .max_by(|a, b| a.confidence.total_cmp(&b.confidence))
            .expect("a candidate");
        assert!(
            spot.temporary >= TEMPORARY_FLOOR,
            "a red inflamed spot read as {:.2} temporary",
            spot.temporary
        );
        assert!(spot.is_removable());
    }

    #[test]
    fn a_dark_mole_does_not_read_as_temporary() {
        let crop = fixtures::face_with_mole();
        let found = detect(&crop);
        assert!(!found.is_empty(), "the mole was not found at all");
        for candidate in &found {
            assert!(
                !candidate.is_removable(),
                "a mole read as removable at {:.2}",
                candidate.temporary
            );
        }
    }

    #[test]
    fn even_skin_produces_nothing() {
        let crop = fixtures::even_face();
        assert!(detect(&crop).is_empty());
    }

    #[test]
    fn a_face_with_no_skin_mask_produces_nothing() {
        let mut crop = fixtures::face_with_blemish();
        crop.skin = vec![0.0; crop.skin.len()];
        assert!(detect(&crop).is_empty());
    }

    #[test]
    fn detection_is_deterministic() {
        let crop = fixtures::face_with_blemish();
        assert_eq!(detect(&crop), detect(&crop));
    }

    #[test]
    fn an_undistinguished_mark_is_left_alone() {
        // Neither red nor round: the default has to land below the removal floor, because
        // section 6.1 says removing a mole is a far worse error than leaving a pimple.
        assert!(temporary_probability(0.0, 0.55) < TEMPORARY_FLOOR);
    }
}
