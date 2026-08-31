//! How each body actually rendered *this* wedding.
//!
//! Section 8 step 2. Section 2.1 asks for "per body/profile colour response measured from the
//! wedding's own frames - skin chromaticity, white-point behaviour, saturation response, contrast
//! character, highlight roll-off", and the phrase that decides the design is **from the wedding's
//! own frames**: a fingerprint is not a lookup, and a body this build has never seen is
//! fingerprinted as well as a body it has.
//!
//! ## Nothing here opens a photograph
//!
//! Every number comes from a reading another phase already stored - phase 05's descriptors, phase
//! 15's illuminant and subject luminance, phase 16's grade, phase 25's per-identity skin work.
//! Invariant 3 read at its strongest: a fingerprint pass over a four-thousand-frame wedding that
//! decoded four thousand RAWs would spend twenty minutes producing nine numbers that were already
//! in the catalog, and section 11 budgets eighteen seconds for this step and the pairing together.
//!
//! ## Every statistic is robust, and the reason is specific rather than general
//!
//! A wedding contains a handful of frames from every body that are wrong: the accidental shutter
//! press into a light, the frame where somebody's phone screen filled a third of the sensor, the
//! one where phase 15's illuminant search picked the wrong hypothesis. A mean over those is a
//! fingerprint that says a Canon is green. So the chromaticities are component-wise medians, the
//! scalars are trimmed means, and the agreement between the samples is folded into the confidence
//! rather than thrown away - a body whose frames disagree with each other about where it puts skin
//! has not been fingerprinted, whatever the sample count says.

use std::collections::BTreeMap;

use aura_core::contract::camera::{
    Brand, CameraCode, CameraFingerprint, CameraReason, FlashState, MIN_FINGERPRINT_SAMPLES,
};
use aura_core::contract::gallery::ImageId;
use aura_core::contract::ids::NodeId;
use aura_core::contract::moment::CameraId;
use aura_core::SceneId;

use crate::stats;

use super::ANALYSIS_VER;

/// How many coarse hue bins a background summary keeps.
///
/// Twelve, from phase 05's eight hue bins crossed with its eight saturation bins: coarse enough
/// that two bodies' different rendering of the same wall lands in the same bin, fine enough that a
/// marquee ceiling and a stained-glass window do not. The comparison this feeds is "were these two
/// frames in the same room", not "are these the same colour".
pub const HUE_BINS: usize = 12;

/// How many tonal quarters a saturation or contrast response is measured across.
pub const RESPONSE_QUARTERS: usize = 4;

/// What a frame's surroundings looked like, cheaply, for pair verification.
///
/// Assembled from phase 05's stored descriptors and from nothing else. Migration 5's own header
/// says "the histogram and the luminance percentiles are what phases 25 and 26 use for gallery
/// colour consistency and multi-camera matching", so this is the consumer that comment was written
/// for.
///
/// **It deliberately describes the whole frame rather than a segmented background.** Phase 18's
/// masks would give a cleaner background, and using them would make this phase's evidence depend on
/// a segmentation head that is untrained in this build - so a pair that could be verified from
/// phase 05's descriptors would become a pair that could not be verified at all. The whole-frame
/// summary is dominated by the surroundings on any frame that is not a tight portrait, and the
/// tight portraits are excluded from pairing by the similarity floor rather than by a mask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackgroundStats {
    /// A coarse hue histogram, normalised to sum to one.
    pub hue: [f32; HUE_BINS],
    /// Mean, first percentile, median and ninety-ninth percentile luminance.
    pub luma: [f32; 4],
    /// Mean gradient magnitude, `0..1`. How busy the frame is.
    pub edge_energy: f32,
    /// Mean saturation in each of four tonal quarters.
    pub sat_by_quarter: [f32; RESPONSE_QUARTERS],
    /// The spread of value inside each tonal quarter: the frame's own contrast character.
    pub contrast_by_quarter: [f32; RESPONSE_QUARTERS],
    /// How gently the highlights roll off, `0..1`. One is a hard clip.
    pub highlight_rolloff: f32,
}

impl BackgroundStats {
    /// Build a summary from phase 05's eight-by-eight-by-eight HSV histogram and luminance stats.
    ///
    /// The histogram is indexed `h * 64 + s * 8 + v`, which is phase 05's own layout.
    #[must_use]
    pub fn from_descriptors(hsv_hist: &[u8; 512], luma: [f32; 4], edge_energy: f32) -> Self {
        let total: f32 = hsv_hist.iter().map(|bin| f32::from(*bin)).sum();
        let inv = if total > 0.0 { 1.0 / total } else { 0.0 };

        let mut hue = [0.0_f32; HUE_BINS];
        let mut sat_sum = [0.0_f32; RESPONSE_QUARTERS];
        let mut sat_weight = [0.0_f32; RESPONSE_QUARTERS];
        let mut value_mean = [0.0_f32; RESPONSE_QUARTERS];
        let mut value_sq = [0.0_f32; RESPONSE_QUARTERS];

        for h in 0..8_usize {
            for s in 0..8_usize {
                for v in 0..8_usize {
                    let Some(count) = hsv_hist.get(h * 64 + s * 8 + v) else {
                        continue;
                    };
                    let weight = f32::from(*count) * inv;
                    if weight <= 0.0 {
                        continue;
                    }
                    // Eight hue bins into twelve: each source bin contributes to the one and a half
                    // destination bins it overlaps, so a wall that lands on a boundary in one
                    // body's rendering and just inside it in another's still matches.
                    #[allow(clippy::cast_precision_loss)]
                    let centre = (h as f32 + 0.5) / 8.0 * HUE_BINS as f32;
                    let lower = centre.floor();
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let lo = (lower as usize) % HUE_BINS;
                    let hi = (lo + 1) % HUE_BINS;
                    let frac = centre - lower;
                    if let Some(slot) = hue.get_mut(lo) {
                        *slot += weight * (1.0 - frac);
                    }
                    if let Some(slot) = hue.get_mut(hi) {
                        *slot += weight * frac;
                    }

                    let quarter = (v * RESPONSE_QUARTERS) / 8;
                    #[allow(clippy::cast_precision_loss)]
                    let saturation = (s as f32 + 0.5) / 8.0;
                    #[allow(clippy::cast_precision_loss)]
                    let value = (v as f32 + 0.5) / 8.0;
                    if let (Some(sum), Some(w)) =
                        (sat_sum.get_mut(quarter), sat_weight.get_mut(quarter))
                    {
                        *sum += saturation * weight;
                        *w += weight;
                    }
                    if let (Some(mean), Some(sq)) =
                        (value_mean.get_mut(quarter), value_sq.get_mut(quarter))
                    {
                        *mean += value * weight;
                        *sq += value * value * weight;
                    }
                }
            }
        }

        let mut sat_by_quarter = [0.0_f32; RESPONSE_QUARTERS];
        let mut contrast_by_quarter = [0.0_f32; RESPONSE_QUARTERS];
        for q in 0..RESPONSE_QUARTERS {
            let w = sat_weight.get(q).copied().unwrap_or(0.0);
            if w <= f32::EPSILON {
                continue;
            }
            if let Some(slot) = sat_by_quarter.get_mut(q) {
                *slot = sat_sum.get(q).copied().unwrap_or(0.0) / w;
            }
            let mean = value_mean.get(q).copied().unwrap_or(0.0) / w;
            let sq = value_sq.get(q).copied().unwrap_or(0.0) / w;
            if let Some(slot) = contrast_by_quarter.get_mut(q) {
                // Standard deviation of value inside the quarter, scaled so a full-range quarter
                // reads as one. A *spread* rather than a range, because a range is decided by two
                // pixels and every wedding has two pixels of a specular highlight in it.
                *slot = (sq - mean * mean).max(0.0).sqrt() * 4.0;
            }
        }

        Self {
            hue,
            luma,
            edge_energy: edge_energy.clamp(0.0, 1.0),
            sat_by_quarter,
            contrast_by_quarter,
            highlight_rolloff: rolloff_from(luma),
        }
    }

    /// How much two frames' surroundings agree, `0..1`.
    ///
    /// The number section 6.1's verification turns on. Three terms, multiplied rather than averaged
    /// - a **product**, so no term can rescue another, which is the shape phase 12 established for
    ///   its keep score and phase 25 for its anchor ranking:
    ///
    /// * histogram intersection over the twelve hue bins: were these two frames looking at the same
    ///   things,
    /// * luminance agreement over the four percentiles: was the room the same brightness,
    /// * edge agreement: was the frame as busy.
    ///
    /// **It is deliberately insensitive to the difference this phase is trying to measure.** Two
    /// bodies rendering one wall put its hue in the same coarse bin and its luminance within a few
    /// per cent; two bodies in two different rooms do neither. A metric fine enough to see a brand
    /// difference here would reject every pair in the wedding.
    #[must_use]
    pub fn agreement(&self, other: &Self) -> f32 {
        let hue: f32 = self
            .hue
            .iter()
            .zip(other.hue.iter())
            .map(|(a, b)| a.min(*b))
            .sum();

        let luma_error: f32 = self
            .luma
            .iter()
            .zip(other.luma.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / 4.0;
        let luma = (1.0 - luma_error * 3.0).clamp(0.0, 1.0);

        let edge_error = (self.edge_energy - other.edge_energy).abs();
        let edge = (1.0 - edge_error * 2.5).clamp(0.0, 1.0);

        (hue.clamp(0.0, 1.0) * luma * edge).clamp(0.0, 1.0)
    }
}

/// How gently the highlights roll off, from four luminance percentiles.
///
/// One minus the headroom between the median and the ninety-ninth percentile, scaled: a frame whose
/// p99 sits far above its median has runway left, and one whose p99 is nearly on top of its median
/// has been clipped into. The clipped fraction is not used because phase 05 stores it separately
/// and a frame with a specular highlight in it is not a frame that rolls off hard.
fn rolloff_from(luma: [f32; 4]) -> f32 {
    let median = luma.get(2).copied().unwrap_or(0.5);
    let p99 = luma.get(3).copied().unwrap_or(1.0);
    let headroom = (p99 - median).clamp(0.0, 1.0);
    (1.0 - headroom * 2.0).clamp(0.0, 1.0)
}

/// Everything the matching pass knows about one photograph.
///
/// Assembled by the caller from phase 05's descriptors, phase 07's scene, phase 15's tone estimate,
/// phase 16's colour decision and phase 25's tree, through the frozen services and through nothing
/// else. This crate never asks a second time.
///
/// **Every field that comes from another phase is optional or carries its own confidence**, and the
/// absences are what the reason codes are made of. A frame with no tone estimate is not a frame at
/// 5,500 K. Phase 25's `Frame` established the shape and this is its sibling: the two are separate
/// types because they answer different questions - that one is about a photograph's place in a
/// gallery and this one is about the body that took it - and folding them together would put a
/// camera id on four thousand rows that do not need one.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraFrame {
    /// The photograph.
    pub image: ImageId,
    /// The body that took it.
    pub camera: CameraId,
    /// That body's manufacturer, for the baseline lookup only.
    pub brand: Brand,
    /// The shooter label the catalog carries for that body.
    pub shooter: String,
    /// Which of the body's two colour behaviours this frame belongs to.
    pub flash: FlashState,
    /// The scene node phase 25 placed it in, when it placed it in one.
    ///
    /// `None` excludes the frame from pairing and not from fingerprinting: a body's colour response
    /// is measured over everything it shot, and a pair needs two frames provably in the same light.
    pub node: Option<NodeId>,
    /// What it is of.
    pub scene: SceneId,
    /// When it was taken, in milliseconds on the project's own aligned timeline.
    ///
    /// Phase 08's `sub_sec_ms` already folded in by the caller. Pairing reads this as a distance and
    /// a whole-second resolution would make two frames of a 10 fps burst indistinguishable, which
    /// would let a pair form between two frames of the *same* body's burst if the camera ids were
    /// ever wrong.
    pub timeline_ms: i64,
    /// Phase 15's solved temperature, in kelvin.
    pub cct_k: Option<f32>,
    /// Phase 15's solved tint.
    pub tint: Option<f32>,
    /// Phase 15's solved exposure offset, in stops.
    pub exposure_ev: Option<f32>,
    /// The subject luminance after that exposure, `0..1`.
    pub subject_luma: Option<f32>,
    /// Phase 15's white-balance confidence, `0..1`.
    pub wb_conf: f32,
    /// Where phase 15 put the illuminant, in CIE 1976 `u'v'`.
    pub white_uv: Option<[f32; 2]>,
    /// Where this frame's skin sits, in CIE 1976 `u'v'`.
    ///
    /// From phase 25's per-identity skin readings, which in this build exist only for authored
    /// fixtures - `SKIN_FIELD_AVAILABLE` is false, so no real photograph carries one. A frame
    /// without it contributes to every part of a fingerprint except the skin chromaticity.
    pub skin_uv: Option<[f32; 2]>,
    /// How bright that skin is, `0..1`.
    pub skin_luma: Option<f32>,
    /// Phase 16's contrast, in the recipe's units.
    pub contrast: Option<f32>,
    /// Phase 16's saturation.
    pub saturation: Option<f32>,
    /// Phase 16's colour character, as the eight numbers a distance is measured over.
    pub signature: Option<[f32; 8]>,
    /// Phase 05's embedding, for the pairing pre-filter.
    pub embedding: Option<Vec<f32>>,
    /// Phase 05's descriptors, summarised.
    pub background: Option<BackgroundStats>,
}

impl CameraFrame {
    /// True when there is enough here to contribute to a fingerprint.
    #[must_use]
    pub fn is_measurable(&self) -> bool {
        self.cct_k.is_some() && self.white_uv.is_some() && self.background.is_some()
    }

    /// True when this frame could be half of a matched pair.
    ///
    /// The node is what makes it evidence: two frames in different nodes were shot under different
    /// light by construction, whatever their subjects look like.
    #[must_use]
    pub fn is_pairable(&self) -> bool {
        self.node.is_some()
            && self.background.is_some()
            && self.embedding.is_some()
            && self.is_measurable()
    }
}

/// The key a fingerprint and a transform are both stored under.
pub type Key = (CameraId, FlashState);

/// Group a project's frames by body and flash state, in a deterministic order.
///
/// A `BTreeMap` rather than a hash map for the reason every collection in this product is ordered:
/// the iteration order reaches a solver and a determinism test. Invariant 4.
#[must_use]
pub fn group(frames: &[CameraFrame]) -> BTreeMap<Key, Vec<usize>> {
    let mut groups: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (index, frame) in frames.iter().enumerate() {
        groups
            .entry((frame.camera.clone(), frame.flash))
            .or_default()
            .push(index);
    }
    groups
}

/// Measure one body's colour response in one flash state.
///
/// `None` when fewer than [`MIN_FINGERPRINT_SAMPLES`] of the frames are measurable, which is
/// [`CameraCode::FingerprintAbsent`] and is a body matched from its brand baseline alone. A weak
/// fingerprint is worse than none because it looks like evidence - phase 15's argument for
/// `MIN_LOCUS_SAMPLES`, unchanged.
#[must_use]
pub fn measure(
    camera: &CameraId,
    flash: FlashState,
    brand: Brand,
    frames: &[&CameraFrame],
    flash_split: bool,
) -> Option<CameraFingerprint> {
    let usable: Vec<&&CameraFrame> = frames.iter().filter(|f| f.is_measurable()).collect();
    let count = u32::try_from(usable.len()).unwrap_or(u32::MAX);
    if count < MIN_FINGERPRINT_SAMPLES {
        return None;
    }

    let white_points: Vec<[f32; 2]> = usable.iter().filter_map(|f| f.white_uv).collect();
    let white_point = stats::median_uv(&white_points)?;

    // Skin is the one axis that may be missing entirely: `SKIN_FIELD_AVAILABLE` is false in this
    // build, so on a real photograph this list is empty. The fingerprint then falls back on the
    // white point, which is a *stated* substitution rather than a silent one - a body's neutral and
    // a body's skin are correlated and are not the same thing, and `CameraCode::FingerprintThin`
    // beside it is what says the fingerprint rests on less than it wanted to.
    let skin_points: Vec<[f32; 2]> = usable.iter().filter_map(|f| f.skin_uv).collect();
    let skin_measured = skin_points.len() >= MIN_FINGERPRINT_SAMPLES as usize;
    let skin_chroma = if skin_measured {
        stats::median_uv(&skin_points).unwrap_or(white_point)
    } else {
        white_point
    };

    let subject_luma = stats::median(
        &usable
            .iter()
            .filter_map(|f| f.subject_luma)
            .collect::<Vec<_>>(),
    )
    .unwrap_or(0.5)
    .clamp(0.0, 1.0);

    let backgrounds: Vec<&BackgroundStats> = usable
        .iter()
        .filter_map(|f| f.background.as_ref())
        .collect();
    let sat_response = quarter_median(&backgrounds, |b| b.sat_by_quarter);
    let contrast_response = quarter_median(&backgrounds, |b| b.contrast_by_quarter);
    let highlight_rolloff = stats::median(
        &backgrounds
            .iter()
            .map(|b| b.highlight_rolloff)
            .collect::<Vec<_>>(),
    )
    .unwrap_or(0.5)
    .clamp(0.0, 1.0);

    let grade_signature = signature_median(&usable);

    // Three terms, multiplied. A fingerprint measured from two hundred frames the product was
    // unsure about is not worth more than one measured from twenty it was sure about, and a body
    // whose own frames disagree about where it puts a neutral has not been fingerprinted at all
    // whatever the count says.
    let sample_term = CameraFingerprint::sample_weight(count);
    let confidence_term = stats::median(
        &usable
            .iter()
            .map(|f| f.wb_conf.clamp(0.0, 1.0))
            .collect::<Vec<_>>(),
    )
    .unwrap_or(0.0);
    let spread = uv_spread(&white_points, white_point);
    // A quarter of the white-point scale: past that the samples are describing several rooms
    // rather than one body, which is what a whole wedding's white points do when a body was used
    // indoors and out. It reduces confidence rather than refusing, because a body used everywhere
    // is the ordinary case and its *median* is still the right answer.
    let agreement_term = (1.0 - spread / 0.03).clamp(0.0, 1.0);
    let confidence = (sample_term * confidence_term * agreement_term).clamp(0.0, 1.0);

    let mut reasons = vec![CameraReason::of(CameraCode::Fingerprinted)];
    if sample_term < 1.0 || !skin_measured {
        reasons.push(CameraReason::of(CameraCode::FingerprintThin));
    }
    if flash_split {
        reasons.push(CameraReason::of(CameraCode::FlashSeparated));
    }
    reasons.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.code.cmp(&b.code))
    });

    Some(CameraFingerprint {
        camera_id: camera.clone(),
        flash,
        skin_chroma,
        white_point,
        sat_response,
        contrast_response,
        highlight_rolloff,
        samples: count,
        confidence,
        brand,
        grade_signature,
        subject_luma,
        reasons,
        analysis_ver: ANALYSIS_VER,
    })
}

/// The component-wise median of a four-number response across a body's frames.
fn quarter_median(
    backgrounds: &[&BackgroundStats],
    pick: impl Fn(&BackgroundStats) -> [f32; RESPONSE_QUARTERS],
) -> [f32; RESPONSE_QUARTERS] {
    let mut out = [0.0_f32; RESPONSE_QUARTERS];
    for q in 0..RESPONSE_QUARTERS {
        let values: Vec<f32> = backgrounds
            .iter()
            .filter_map(|b| pick(b).get(q).copied())
            .collect();
        if let Some(slot) = out.get_mut(q) {
            *slot = stats::median(&values).unwrap_or(0.0);
        }
    }
    out
}

/// The component-wise median of a body's eight-number grade character.
fn signature_median(frames: &[&&CameraFrame]) -> [f32; 8] {
    let mut out = [0.0_f32; 8];
    for index in 0..8 {
        let values: Vec<f32> = frames
            .iter()
            .filter_map(|f| f.signature.and_then(|sig| sig.get(index).copied()))
            .collect();
        if let Some(slot) = out.get_mut(index) {
            *slot = stats::median(&values).unwrap_or(0.0);
        }
    }
    out
}

/// The median distance of a set of chromaticities from their own centre, in `u'v'`.
fn uv_spread(points: &[[f32; 2]], centre: [f32; 2]) -> f32 {
    let distances: Vec<f32> = points
        .iter()
        .map(|p| {
            let du = p.first().copied().unwrap_or(0.0) - centre.first().copied().unwrap_or(0.0);
            let dv = p.get(1).copied().unwrap_or(0.0) - centre.get(1).copied().unwrap_or(0.0);
            (du * du + dv * dv).sqrt()
        })
        .collect();
    stats::median(&distances).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(camera: &str, white: [f32; 2], skin: Option<[f32; 2]>) -> CameraFrame {
        CameraFrame {
            image: ImageId::new(),
            camera: CameraId::new(camera),
            brand: Brand::Canon,
            shooter: "primary".to_string(),
            flash: FlashState::Ambient,
            node: Some(NodeId::new()),
            scene: SceneId::Ceremony,
            timeline_ms: 0,
            cct_k: Some(5200.0),
            tint: Some(0.0),
            exposure_ev: Some(0.0),
            subject_luma: Some(0.45),
            wb_conf: 0.8,
            white_uv: Some(white),
            skin_uv: skin,
            skin_luma: Some(0.5),
            contrast: Some(8.0),
            saturation: Some(4.0),
            signature: Some([0.1; 8]),
            embedding: Some(vec![0.5; 8]),
            background: Some(BackgroundStats::from_descriptors(
                &[4; 512],
                [0.4, 0.05, 0.38, 0.92],
                0.2,
            )),
        }
    }

    #[test]
    fn a_body_with_too_few_frames_is_not_fingerprinted() {
        let frames: Vec<CameraFrame> = (0..(MIN_FINGERPRINT_SAMPLES as usize - 1))
            .map(|_| frame("cam_a", [0.20, 0.47], None))
            .collect();
        let refs: Vec<&CameraFrame> = frames.iter().collect();
        assert!(
            measure(
                &CameraId::new("cam_a"),
                FlashState::Ambient,
                Brand::Canon,
                &refs,
                false
            )
            .is_none(),
            "a weak fingerprint is worse than none because it looks like evidence"
        );
    }

    #[test]
    fn one_wild_frame_does_not_move_a_fingerprint() {
        // The whole argument for medians over means. Twenty ordinary frames plus one accidental
        // shutter press into a stage light; a mean would report this body as magenta.
        let mut frames: Vec<CameraFrame> = (0..20)
            .map(|_| frame("cam_a", [0.2000, 0.4700], None))
            .collect();
        frames.push(frame("cam_a", [0.3400, 0.3100], None));
        let refs: Vec<&CameraFrame> = frames.iter().collect();
        let print = measure(
            &CameraId::new("cam_a"),
            FlashState::Ambient,
            Brand::Canon,
            &refs,
            false,
        )
        .expect("enough samples");
        assert!(
            (print.white_point[0] - 0.2000).abs() < 1e-3,
            "{:?}",
            print.white_point
        );
        assert!((print.white_point[1] - 0.4700).abs() < 1e-3);
    }

    #[test]
    fn disagreeing_frames_lower_the_confidence_without_refusing() {
        let tight: Vec<CameraFrame> = (0..30)
            .map(|_| frame("cam_a", [0.20, 0.47], None))
            .collect();
        let loose: Vec<CameraFrame> = (0..30)
            .enumerate()
            .map(|(i, _)| {
                let offset = if i % 2 == 0 { 0.04 } else { -0.04 };
                frame("cam_a", [0.20 + offset, 0.47], None)
            })
            .collect();
        let measure_of = |set: &[CameraFrame]| {
            let refs: Vec<&CameraFrame> = set.iter().collect();
            measure(
                &CameraId::new("cam_a"),
                FlashState::Ambient,
                Brand::Canon,
                &refs,
                false,
            )
            .expect("enough samples")
            .confidence
        };
        assert!(measure_of(&tight) > measure_of(&loose));
    }

    #[test]
    fn an_absent_skin_field_is_stated_rather_than_silently_substituted() {
        let frames: Vec<CameraFrame> = (0..20)
            .map(|_| frame("cam_a", [0.20, 0.47], None))
            .collect();
        let refs: Vec<&CameraFrame> = frames.iter().collect();
        let print = measure(
            &CameraId::new("cam_a"),
            FlashState::Ambient,
            Brand::Canon,
            &refs,
            false,
        )
        .expect("enough samples");
        assert!(print
            .reasons
            .iter()
            .any(|r| r.code == CameraCode::FingerprintThin));
        assert_eq!(print.skin_chroma, print.white_point);
    }

    #[test]
    fn two_frames_of_the_same_room_agree_and_two_rooms_do_not() {
        let same = BackgroundStats::from_descriptors(&[4; 512], [0.40, 0.05, 0.38, 0.92], 0.20);
        let nearly = BackgroundStats::from_descriptors(&[4; 512], [0.41, 0.06, 0.39, 0.93], 0.22);
        assert!(same.agreement(&nearly) > 0.7, "{}", same.agreement(&nearly));

        let mut other_hist = [0_u8; 512];
        for (index, slot) in other_hist.iter_mut().enumerate() {
            *slot = if index < 64 { 200 } else { 0 };
        }
        let elsewhere =
            BackgroundStats::from_descriptors(&other_hist, [0.12, 0.01, 0.09, 0.40], 0.75);
        assert!(
            same.agreement(&elsewhere) < 0.3,
            "{}",
            same.agreement(&elsewhere)
        );
    }

    #[test]
    fn agreement_is_a_product_so_no_term_rescues_another() {
        let base = BackgroundStats::from_descriptors(&[4; 512], [0.40, 0.05, 0.38, 0.92], 0.20);
        // Identical hues and identical edges, wildly different brightness. An average would call
        // this two thirds agreement; a product calls it nothing, which is correct - the two frames
        // were not in the same light.
        let dark = BackgroundStats::from_descriptors(&[4; 512], [0.02, 0.0, 0.01, 0.10], 0.20);
        assert!(base.agreement(&dark) < 0.05, "{}", base.agreement(&dark));
    }

    #[test]
    fn grouping_is_deterministic_and_splits_the_flash_state() {
        let mut frames = vec![
            frame("cam_b", [0.2, 0.47], None),
            frame("cam_a", [0.2, 0.47], None),
        ];
        frames[0].flash = FlashState::Flash;
        let groups = group(&frames);
        let keys: Vec<String> = groups
            .keys()
            .map(|(camera, flash)| format!("{camera}/{flash}"))
            .collect();
        assert_eq!(keys, vec!["cam_a/ambient", "cam_b/flash"]);
    }

    #[test]
    fn the_rolloff_reads_a_clipped_frame_as_hard_and_an_open_one_as_gentle() {
        let clipped = rolloff_from([0.5, 0.1, 0.5, 0.55]);
        let open = rolloff_from([0.4, 0.05, 0.35, 0.95]);
        assert!(clipped > open);
        assert!((0.0..=1.0).contains(&clipped));
        assert!((0.0..=1.0).contains(&open));
    }
}
