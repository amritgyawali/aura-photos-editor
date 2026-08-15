//! Is the *right* subject sharp.
//!
//! PHASE-09 section 6.1. The whole phase turns on the difference between "this
//! photograph has soft pixels in it" and "this photograph is soft": a blurred
//! background is craft and a blurred bride is a defect, and a global sharpness number
//! cannot tell them apart. Section 1 says photographers abandon culling tools the
//! moment one frame is thrown away that should have been kept, and a global sharpness
//! measure is the single most common way that happens.
//!
//! ## Four measures, and why each of them is here
//!
//! | Measure | What it is good at | Where it lies |
//! |---|---|---|
//! | [`laplacian_variance`] | cheap, monotonic under blur | says a lace veil is sharper than a cheek |
//! | [`tenengrad`] | robust to noise, weights strong edges | says a noisy frame is sharp |
//! | [`acutance`] | contrast-normalised, comparable between frames | needs an edge to measure |
//! | the focus head | shallow depth of field, veils, rim light, bokeh | is a placeholder in this build |
//!
//! None of them is trusted alone. [`measure_region`] takes the *contrast-normalised*
//! acutance as the headline number because it is the only one of the three classical
//! measures that is comparable between two different photographs, and carries the other
//! two so the panel and the eval harness can see them disagree.
//!
//! ## The region order is the argument
//!
//! Section 6.1: "compute sharpness on eye regions first (that is what viewers judge),
//! then face, then body, then background bands". [`RegionSet::for_face`] builds exactly
//! that, and the eye regions are why phase 09 needed the geometry amendment to
//! `FaceRef` recorded in ADR-0019 section 3 - a face box alone cannot say where an eye
//! is on a head turned three-quarters away.
//!
//! ## Normalisation is not optional
//!
//! Every number that leaves this module has been divided by
//! [`Calibration::expected_mtf50`]. Section 12's third failure mode is camera bias, and
//! section 10.1's last gate is a 0.05 tolerance between a 24 MP and a 61 MP body on the
//! same scene. Without the division the 61 MP body loses that comparison every time -
//! not because it is worse but because a bigger downsample averages more sensor pixels
//! into each proxy pixel. See [`crate::integrity::calibration`].

use std::sync::Arc;

use aura_core::contract::integrity::CropRect;
use aura_core::{AuraResult, Priority};
use aura_infer::contract::infer::{InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{FOCUS_CLASSES, FOCUS_CROP_SIDE, FOCUS_HEAD_MODEL};
use aura_vision::embed::descriptors::{box_resize, LumaPlane};

use crate::integrity::calibration::Calibration;

/// A rectangle in pixels, already clamped to the plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    /// Left edge, inclusive.
    pub x0: usize,
    /// Top edge, inclusive.
    pub y0: usize,
    /// Right edge, exclusive.
    pub x1: usize,
    /// Bottom edge, exclusive.
    pub y1: usize,
}

impl PixelRect {
    /// Convert a normalised rectangle, clamped so that no measurement can read off the
    /// end of a plane.
    ///
    /// A rectangle of fewer than three pixels on a side is returned empty rather than
    /// enlarged: a Laplacian needs a neighbourhood, and silently growing a caller's
    /// rectangle would measure pixels they did not ask about.
    #[must_use]
    pub fn from_norm(rect: CropRect, width: usize, height: usize) -> Option<Self> {
        let rect = rect.clamped();
        let scale = |value: f32, span: usize| -> usize {
            let raw = value * span as f32;
            (raw.round().max(0.0) as usize).min(span)
        };
        let x0 = scale(rect.x, width);
        let y0 = scale(rect.y, height);
        let x1 = scale(rect.x + rect.w, width);
        let y1 = scale(rect.y + rect.h, height);
        if x1.saturating_sub(x0) < 3 || y1.saturating_sub(y0) < 3 {
            return None;
        }
        Some(Self { x0, y0, x1, y1 })
    }

    /// How many pixels the rectangle covers.
    #[must_use]
    pub const fn area(&self) -> usize {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }
}

/// What one region measured.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RegionSharpness {
    /// Contrast-normalised mean gradient. The headline number.
    pub acutance: f32,
    /// Variance of the Laplacian, normalised by the region's own contrast.
    pub laplacian: f32,
    /// Mean squared Sobel gradient magnitude.
    pub tenengrad: f32,
    /// Difference between the region's 95th and 5th luminance percentiles.
    ///
    /// Carried because it is the denominator of the first two, and a region with almost
    /// no contrast - a wall, an overexposed veil - produces a meaningless ratio that a
    /// caller has to be able to notice rather than inherit.
    pub contrast: f32,
    /// How many pixels were measured.
    pub pixels: usize,
}

impl RegionSharpness {
    /// True when there was enough contrast for the numbers to mean anything.
    ///
    /// Three per cent of full scale. Below that the region is a flat surface and its
    /// acutance is a ratio of two small numbers, which is a way of producing confident
    /// nonsense.
    #[must_use]
    pub fn is_measurable(&self) -> bool {
        self.pixels > 0 && self.contrast >= 0.03
    }
}

/// Variance of the Laplacian over a region, normalised by contrast.
///
/// The classical measure, and the one everybody starts with. It is monotonic under blur,
/// which section 10.1's blur ladder asserts, and it is wrong about photographs in a
/// specific and predictable way: it responds to *texture*, so a lace veil scores higher
/// than an in-focus cheek and a noisy frame scores higher than a clean one.
///
/// Normalised by the square of the region's contrast so that two exposures of the same
/// subject agree. The raw variance would make a brightly lit face score four times a
/// dim one.
#[must_use]
pub fn laplacian_variance(plane: &LumaPlane, rect: PixelRect) -> f32 {
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut count = 0u32;
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };

    for y in (rect.y0 + 1)..rect.y1.saturating_sub(1) {
        for x in (rect.x0 + 1)..rect.x1.saturating_sub(1) {
            let response = at(x, y).mul_add(
                4.0,
                -(at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1)),
            );
            sum += f64::from(response);
            sum_sq += f64::from(response) * f64::from(response);
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    let mean = sum / f64::from(count);
    let variance = (sum_sq / f64::from(count)) - mean * mean;
    variance.max(0.0) as f32
}

/// Mean squared Sobel gradient magnitude over a region.
///
/// More robust to noise than the Laplacian because it is a first derivative rather than
/// a second, and it weights strong edges heavily - which is what makes it good at
/// telling a well-focused eyelash from a slightly soft one and bad at frames whose only
/// strong edge is a blown highlight.
#[must_use]
pub fn tenengrad(plane: &LumaPlane, rect: PixelRect) -> f32 {
    let mut total = 0.0f64;
    let mut count = 0u32;
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };

    for y in (rect.y0 + 1)..rect.y1.saturating_sub(1) {
        for x in (rect.x0 + 1)..rect.x1.saturating_sub(1) {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            total += f64::from(gx.mul_add(gx, gy * gy));
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    (total / f64::from(count)) as f32
}

/// Contrast-normalised mean gradient. **The MTF50 estimate section 6.1 asks for.**
///
/// It is an estimate and the name matters, so this is stated plainly: a true MTF50 is
/// measured from the edge-spread function of a slanted edge on a chart, and there is no
/// chart in a wedding photograph. What this computes is the mean absolute gradient over
/// the region divided by the region's own dynamic range - the acutance - which is
/// proportional to MTF50 for a step edge and degrades gracefully when the region has no
/// clean edge in it.
///
/// The division by contrast is what makes the number comparable between a bright face
/// and a dim one, between two exposures of one scene, and - once
/// [`Calibration::expected_mtf50`] divides it again - between two camera bodies.
#[must_use]
pub fn acutance(plane: &LumaPlane, rect: PixelRect) -> f32 {
    let contrast = contrast_of(plane, rect);
    if contrast <= f32::EPSILON {
        return 0.0;
    }
    let mut total = 0.0f64;
    let mut count = 0u32;
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };

    for y in rect.y0..rect.y1.saturating_sub(1) {
        for x in rect.x0..rect.x1.saturating_sub(1) {
            let here = at(x, y);
            let dx = at(x + 1, y) - here;
            let dy = at(x, y + 1) - here;
            total += f64::from(dx.abs() + dy.abs());
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    ((total / f64::from(count)) as f32) / contrast
}

/// The region's 95th percentile minus its 5th, in `0..1`.
///
/// Percentiles rather than the full range, because one blown specular pixel would
/// otherwise set the contrast of a whole face to 1.0 and divide every sharpness number
/// in the region by it.
#[must_use]
pub fn contrast_of(plane: &LumaPlane, rect: PixelRect) -> f32 {
    let mut histogram = [0u32; 256];
    let mut count = 0u32;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let value = plane
                .values
                .get(y * plane.width + x)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let bin = (value * 255.0).round() as usize;
            if let Some(slot) = histogram.get_mut(bin.min(255)) {
                *slot += 1;
            }
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    let percentile = |fraction: f32| -> f32 {
        let target = ((fraction * count as f32).ceil() as u32).max(1);
        let mut seen = 0u32;
        for (bin, hits) in histogram.iter().enumerate() {
            seen += hits;
            if seen >= target {
                return bin as f32 / 255.0;
            }
        }
        1.0
    };
    (percentile(0.95) - percentile(0.05)).max(0.0)
}

/// Measure one region with all three classical measures.
#[must_use]
pub fn measure_region(plane: &LumaPlane, rect: PixelRect) -> RegionSharpness {
    let contrast = contrast_of(plane, rect);
    let laplacian = if contrast > f32::EPSILON {
        laplacian_variance(plane, rect) / (contrast * contrast)
    } else {
        0.0
    };
    RegionSharpness {
        acutance: acutance(plane, rect),
        laplacian,
        tenengrad: tenengrad(plane, rect),
        contrast,
        pixels: rect.area(),
    }
}

/// Map a raw acutance onto `0..1` against what this body should reach.
///
/// `1 - exp(-1.6 * ratio)`, where `ratio` is the measured acutance over
/// [`Calibration::expected_mtf50`]. Three properties, and all three are requirements
/// rather than preferences:
///
/// * **Strictly monotonic and never clamped.** Section 10.1's first gate is a blur
///   ladder with a monotonic response, and a curve that saturates at exactly 1.0 makes
///   the two sharpest rungs compare equal.
/// * **A frame that meets its body's expectation scores 0.80**, not 1.0. Meeting
///   expectation is "sharp", and leaving headroom above it is what lets a genuinely
///   exceptional frame be distinguished from an adequate one inside a moment.
/// * **Half the expectation scores 0.55, a quarter scores 0.33.** The curve is shallow
///   near the top and steep near the bottom, so the numbers that decide a flag are
///   spread out and the numbers that do not are compressed.
#[must_use]
pub fn normalise(raw_acutance: f32, calibration: &Calibration) -> f32 {
    if calibration.expected_mtf50 <= f32::EPSILON {
        return 0.0;
    }
    let ratio = (raw_acutance / calibration.expected_mtf50).max(0.0);
    1.0 - (-1.6 * ratio).exp()
}

/// The regions of one frame, in section 6.1's order of importance.
///
/// Built by [`RegionSet::for_face`] for a frame with a subject and by
/// [`RegionSet::subjectless`] for one without. The two are different enough that the
/// caller has to choose, which is deliberate: a detail shot of rings has no subject to
/// be soft, and a code path that silently substituted the frame centre for a face would
/// report `SUBJECT_SOFT` about a photograph with no subject in it.
#[derive(Debug, Clone, Default)]
pub struct RegionSet {
    /// The eye regions, when the geometry was available. What viewers judge.
    pub eyes: Vec<CropRect>,
    /// The face boxes, prominence-weighted by the caller.
    pub faces: Vec<CropRect>,
    /// Body boxes, when phase 06 predicted any.
    pub bodies: Vec<CropRect>,
    /// The far background band - the top of the frame away from any subject.
    pub far: Option<CropRect>,
    /// The near foreground band - the bottom of the frame away from any subject.
    pub near: Option<CropRect>,
}

impl RegionSet {
    /// How wide an eye region is, as a fraction of the face box's width.
    ///
    /// A third of the face width, centred on the eye landmark. Wide enough to contain
    /// the lashes and the lid edge, which is where the sharpness actually is, and
    /// narrow enough that a three-quarter turned head does not pull the far eye's box
    /// onto an ear.
    pub const EYE_SPAN: f32 = 0.33;

    /// Build the region set for a frame with faces.
    ///
    /// `faces` is `(box, eyes, has_eyes)` per face, in prominence order - the caller
    /// owns prominence, because it comes from phase 06 and this crate may not compute
    /// it.
    #[must_use]
    pub fn for_face(faces: &[(CropRect, [[f32; 2]; 2], bool)], bodies: &[CropRect]) -> Self {
        let mut eyes = Vec::new();
        for (bbox, points, has_eyes) in faces {
            if !has_eyes {
                continue;
            }
            let span = (bbox.w * Self::EYE_SPAN).max(0.01);
            for point in points {
                eyes.push(
                    CropRect {
                        x: point[0] - span / 2.0,
                        y: point[1] - span / 2.0,
                        w: span,
                        h: span,
                    }
                    .clamped(),
                );
            }
        }
        let boxes: Vec<CropRect> = faces.iter().map(|(bbox, _, _)| *bbox).collect();
        let highest = boxes.iter().map(|face| face.y).fold(1.0f32, f32::min);
        let lowest = boxes
            .iter()
            .map(|face| face.y + face.h)
            .fold(0.0f32, f32::max);
        Self {
            eyes,
            faces: boxes,
            bodies: bodies.to_vec(),
            far: band_above(highest),
            near: band_below(lowest),
        }
    }

    /// The region set for a frame with nothing phase 06 recognised.
    ///
    /// The central third is measured as "the subject", the outer bands as background,
    /// and the caller is expected to raise [`IntegrityFlags::NO_SUBJECT_DETECTED`] so
    /// that every later reader knows the sharpness figure is about the frame.
    ///
    /// [`IntegrityFlags::NO_SUBJECT_DETECTED`]: aura_core::contract::integrity::IntegrityFlags::NO_SUBJECT_DETECTED
    #[must_use]
    pub fn subjectless() -> Self {
        Self {
            eyes: Vec::new(),
            faces: vec![CropRect {
                x: 1.0 / 3.0,
                y: 1.0 / 3.0,
                w: 1.0 / 3.0,
                h: 1.0 / 3.0,
            }],
            bodies: Vec::new(),
            far: band_above(1.0 / 3.0),
            near: band_below(2.0 / 3.0),
        }
    }
}

/// The background band above the topmost subject, if there is room for one.
fn band_above(top: f32) -> Option<CropRect> {
    let height = top.clamp(0.0, 1.0);
    if height < 0.10 {
        return None;
    }
    Some(CropRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: height,
    })
}

/// The foreground band below the lowest subject, if there is room for one.
fn band_below(bottom: f32) -> Option<CropRect> {
    let start = bottom.clamp(0.0, 1.0);
    if start > 0.90 {
        return None;
    }
    Some(CropRect {
        x: 0.0,
        y: start,
        w: 1.0,
        h: 1.0 - start,
    })
}

/// Everything the focus analysis produced for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusResult {
    /// Prominence-weighted subject sharpness, normalised, `0..1`.
    pub subject: f32,
    /// Background sharpness, normalised, `0..1`.
    pub background: f32,
    /// Where the sharpest plane is, `-1..1`. Negative is front focus.
    pub offset: f32,
    /// The raw subject acutance, before normalisation.
    ///
    /// Stored nowhere and returned so the eval harness can assert monotonicity on a
    /// blur ladder without a calibration row in the way.
    pub raw_subject: f32,
    /// The raw background acutance.
    pub raw_background: f32,
    /// True when the frame had no subject and the centre was measured instead.
    pub subjectless: bool,
    /// True when the background is much softer than a sharp subject.
    ///
    /// Section 13's second criterion in one boolean: **a shallow-depth-of-field
    /// portrait with creamy bokeh is never called soft.** The structural half of that
    /// guarantee is that the subject figure is measured on the subject's own regions,
    /// so a soft background cannot lower it; this flag is the half that lets the panel
    /// say so out loud.
    pub shallow_depth_of_field: bool,
}

/// How much sharper the background must be before the frame is called back-focused.
///
/// A ratio of 1.35 in raw acutance. Deliberately not tight: a background band contains
/// different content from a face, and a wall with a picture rail on it will out-measure
/// a cheek at any aperture. Section 12's first failure mode is false rejection and this
/// is one of the three places in the phase where a loose threshold is the whole point.
pub const FOCUS_MISS_RATIO: f32 = 1.35;

/// How much softer the background must be to count as deliberate.
///
/// The reciprocal case, at a wider margin - 2.2 - because "the background is out of
/// focus" is a claim that exonerates the photograph and should need more evidence than
/// a claim that convicts it.
pub const BOKEH_RATIO: f32 = 2.2;

/// Measure a frame's focus.
///
/// `weights` is one prominence weight per face, in the same order as
/// [`RegionSet::faces`]. An empty slice weights every face equally, which is what a
/// frame with no identity assignment gets.
#[must_use]
pub fn analyse(
    plane: &LumaPlane,
    regions: &RegionSet,
    weights: &[f32],
    calibration: &Calibration,
) -> FocusResult {
    let measure = |rect: &CropRect| -> Option<RegionSharpness> {
        PixelRect::from_norm(*rect, plane.width, plane.height)
            .map(|pixels| measure_region(plane, pixels))
            .filter(RegionSharpness::is_measurable)
    };

    // Section 6.1's order: eyes first, then faces, then bodies. An eye region that is
    // measurable outranks the face box it sits in, because a face box averages a cheek
    // and a jaw into the number a viewer judges by an eyelash.
    let eye_values: Vec<f32> = regions
        .eyes
        .iter()
        .filter_map(|rect| measure(rect).map(|m| m.acutance))
        .collect();

    let face_values: Vec<(f32, f32)> = regions
        .faces
        .iter()
        .enumerate()
        .filter_map(|(index, rect)| {
            measure(rect).map(|m| {
                let weight = weights.get(index).copied().unwrap_or(1.0).max(0.0);
                (m.acutance, weight)
            })
        })
        .collect();

    let body_values: Vec<f32> = regions
        .bodies
        .iter()
        .filter_map(|rect| measure(rect).map(|m| m.acutance))
        .collect();

    let raw_subject = if !eye_values.is_empty() {
        mean(&eye_values)
    } else if !face_values.is_empty() {
        weighted_mean(&face_values)
    } else if !body_values.is_empty() {
        mean(&body_values)
    } else {
        0.0
    };

    let far = regions.far.as_ref().and_then(measure).map(|m| m.acutance);
    let near = regions.near.as_ref().and_then(measure).map(|m| m.acutance);
    let background_samples: Vec<f32> = [far, near].into_iter().flatten().collect();
    let raw_background = if background_samples.is_empty() {
        0.0
    } else {
        mean(&background_samples)
    };

    // The sign convention is section 5's: negative is front focus. The two bands are
    // asked separately rather than averaged, because averaging them would let a sharp
    // foreground and a sharp background cancel into "the subject is fine".
    let offset = focus_offset(raw_subject, far, near);

    let shallow =
        raw_subject > 0.0 && raw_background > 0.0 && raw_subject / raw_background >= BOKEH_RATIO;

    FocusResult {
        subject: normalise(raw_subject, calibration),
        background: normalise(raw_background, calibration),
        offset,
        raw_subject,
        raw_background,
        subjectless: regions.eyes.is_empty() && regions.bodies.is_empty() && weights.is_empty(),
        shallow_depth_of_field: shallow,
    }
}

/// Where the sharpest plane is relative to the subject.
///
/// **An approximation, and its limits are worth stating.** Without a depth map, "in
/// front of" and "behind" are inferred from where in the frame the sharp region is: the
/// band below the subject is usually nearer the camera and the band above it is usually
/// further away. That holds for a standing portrait and fails for a subject shot from
/// above, which is why the magnitude is bounded and the flag it feeds needs
/// [`FOCUS_MISS_RATIO`] before it fires at all.
///
/// Returns zero when neither band is measurable, which is "not determinable" and not
/// "on the subject" - the caller distinguishes them by whether a flag was raised.
#[must_use]
pub fn focus_offset(subject: f32, far: Option<f32>, near: Option<f32>) -> f32 {
    if subject <= f32::EPSILON {
        return 0.0;
    }
    let signed = |band: Option<f32>, sign: f32| -> f32 {
        band.map_or(0.0, |value| {
            let ratio = value / subject;
            if ratio < FOCUS_MISS_RATIO {
                0.0
            } else {
                sign * ((ratio - FOCUS_MISS_RATIO) / (ratio + 1.0)).clamp(0.0, 1.0)
            }
        })
    };
    let back = signed(far, 1.0);
    let front = signed(near, -1.0);
    // Whichever band is further from the subject wins outright rather than being summed
    // with the other. A frame that is sharp in front *and* behind is a frame whose
    // subject is soft, and that is `SUBJECT_SOFT` rather than a focus miss in either
    // direction.
    if back.abs() >= front.abs() {
        back
    } else {
        front
    }
}

fn mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f32>() / values.len() as f32
}

fn weighted_mean(values: &[(f32, f32)]) -> f32 {
    let total: f32 = values.iter().map(|(_, weight)| *weight).sum();
    if total <= f32::EPSILON {
        return mean(&values.iter().map(|(value, _)| *value).collect::<Vec<_>>());
    }
    values
        .iter()
        .map(|(value, weight)| value * weight)
        .sum::<f32>()
        / total
}

// ---------------------------------------------------------------------------
// The learned head
// ---------------------------------------------------------------------------

/// What the focus head thinks one crop is.
///
/// Three mutually exclusive interpretations, in the head's output order. The third is
/// the one that earns the model its place: a classical measure cannot distinguish "the
/// photographer missed focus" from "the photographer opened the lens to f/1.4 and this
/// is the background", because both are low-frequency regions. Section 13's second
/// acceptance criterion is that a shallow-depth-of-field portrait with creamy bokeh is
/// never called soft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusClass {
    /// In focus.
    #[default]
    Sharp,
    /// Softer than it should be.
    Soft,
    /// Deliberately out of focus.
    Bokeh,
}

impl FocusClass {
    /// Every class, in the head's output order. Renumbering relabels a trained model.
    pub const ALL: [Self; 3] = [Self::Sharp, Self::Soft, Self::Bokeh];

    /// The stable slug, for logs and the debug panel.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sharp => "sharp",
            Self::Soft => "soft",
            Self::Bokeh => "bokeh",
        }
    }
}

/// Registry name of the focus head.
pub const FOCUS_MODEL: &str = FOCUS_HEAD_MODEL;

/// Pinned version of the focus head.
pub const FOCUS_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The version stamped on every `image_integrity.model_ver` alongside the eye head's.
///
/// The two heads ship as one pack and move together, so one number invalidates both.
/// They are separately *pinned* - `ModelRef` names each independently, and either can be
/// rolled back on its own by phase 03's machinery - but a stored verdict that recorded
/// two version numbers would need two columns and would answer a question nobody asks:
/// no consumer of `IntegrityResult` cares which of the two heads moved, only that the
/// numbers are not comparable across the move.
pub const MODEL_VER: u16 =
    FOCUS_MODEL_VERSION.major * 100 + FOCUS_MODEL_VERSION.minor * 10 + FOCUS_MODEL_VERSION.patch;

/// Turn one region of the luminance plane into the head's input tensor.
///
/// A box-filter resample to 64 px, the same resampler phase 05 uses for every downscale
/// in the product - and for the reason its doc comment gives: bilinear sampling of a
/// six-times downscale reads four source pixels out of thirty-six and calls the answer a
/// resize. **For a model whose entire job is to judge sharpness, an aliasing resampler
/// would be a catastrophe**: it would introduce exactly the high-frequency artefacts the
/// head is being asked to detect.
#[must_use]
pub fn preprocess_region(plane: &LumaPlane, rect: PixelRect) -> Vec<f32> {
    let width = rect.x1 - rect.x0;
    let height = rect.y1 - rect.y0;
    let mut cropped = Vec::with_capacity(width * height);
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            cropped.push(
                plane
                    .values
                    .get(y * plane.width + x)
                    .copied()
                    .unwrap_or(0.0),
            );
        }
    }
    box_resize(&cropped, width, height, FOCUS_CROP_SIDE, FOCUS_CROP_SIDE)
}

/// The focus head.
#[derive(Debug, Clone)]
pub struct FocusHead {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl FocusHead {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(FOCUS_MODEL, FOCUS_MODEL_VERSION),
        }
    }

    /// The model this runner is pinned to.
    #[must_use]
    pub const fn model(&self) -> ModelRef {
        self.model
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.infer.plan().selected_ep().as_str()
    }

    /// Load the head before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Classify a batch of regions.
    ///
    /// One call for every region of one frame, rather than one call per region: a frame
    /// with three faces asks about three eye pairs, three faces and two bands, and eight
    /// separate calls would throw away phase 03's batch scheduler for the sake of a
    /// simpler loop.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest,
    /// `AURA-ML-5011` when the work was cancelled, and whatever the loader raised.
    pub fn run_batch(
        &self,
        regions: &[Vec<f32>],
        prio: Priority,
    ) -> AuraResult<Vec<(FocusClass, f32)>> {
        if regions.is_empty() {
            return Ok(Vec::new());
        }
        let mut inputs = Vec::with_capacity(regions.len());
        for region in regions {
            inputs.push(vec![TensorView::new(
                vec![1, 1, FOCUS_CROP_SIDE, FOCUS_CROP_SIDE],
                region,
            )?]);
        }
        let results = self.infer.run_batch(self.model, inputs, prio)?;

        let mut out = Vec::with_capacity(results.len());
        for result in results {
            let Some(tensor) = result.outputs.first() else {
                return Err(aura_core::errors::ml::shape_mismatch(
                    "one focus tensor",
                    "no outputs",
                ));
            };
            out.push(decode_focus(&tensor.data));
        }
        Ok(out)
    }
}

/// Turn the head's three probabilities into a class and a confidence.
#[must_use]
pub fn decode_focus(probabilities: &[f32]) -> (FocusClass, f32) {
    if probabilities.len() < FOCUS_CLASSES {
        // A manifest disagreement. `Sharp` is the safe reading: it raises no flag.
        return (FocusClass::Sharp, 0.0);
    }
    let mut best = (FocusClass::Sharp, 0.0f32);
    for (index, class) in FocusClass::ALL.into_iter().enumerate() {
        let value = probabilities.get(index).copied().unwrap_or(0.0);
        if value > best.1 {
            best = (class, value);
        }
    }
    best
}

/// How confident the head must be before its opinion is allowed to overrule the
/// classical measures.
///
/// 0.70, and it only ever overrules in **one direction**: a confident `Bokeh` withdraws a
/// softness claim, and a confident `Soft` does not create one. Section 12's first failure
/// mode is false rejection, and a placeholder model that gained the power to convict a
/// frame would be the fastest possible route to it. When the head is trained, lifting
/// that asymmetry is an ADR and a re-validation rather than a constant change.
pub const HEAD_OVERRULES_AT: f32 = 0.70;

/// Let the head withdraw a softness claim, and never let it make one.
///
/// Returns the focus result with `shallow_depth_of_field` set when the head is confident
/// the subject region is deliberate bokeh. Everything else is left exactly as the
/// classical measures found it.
#[must_use]
pub fn apply_head(mut result: FocusResult, subject: Option<(FocusClass, f32)>) -> FocusResult {
    if let Some((FocusClass::Bokeh, confidence)) = subject {
        if confidence >= HEAD_OVERRULES_AT {
            result.shallow_depth_of_field = true;
        }
    }
    result
}
