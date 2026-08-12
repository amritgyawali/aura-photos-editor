//! Multi-camera clock alignment.
//!
//! At a real wedding two photographers shoot with three bodies, and at least one
//! body's clock is wrong: typically by exactly one hour after a daylight-saving
//! change, occasionally by years after a battery swap. Ordering the story
//! correctly depends on recovering those offsets from the frames themselves.
//!
//! The estimator is pure and deterministic: same timestamps in, same offsets out.

use std::collections::BTreeMap;

/// Every capture timestamp attributed to one body, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraSamples {
    /// The body serial, or a synthesised key when the maker records none.
    pub body_serial: String,
    /// Capture times in milliseconds since the epoch. Sorted by the estimator.
    pub times_ms: Vec<i64>,
}

/// What the estimator concluded about one body.
#[derive(Debug, Clone, PartialEq)]
pub struct OffsetEstimate {
    /// The body this estimate is for.
    pub body_serial: String,
    /// Milliseconds to add to this body's EXIF time to reach the reference clock.
    pub offset_ms: i64,
    /// How much of the evidence agreed, from 0.0 to 1.0.
    pub confidence: f64,
    /// True when the offset was applied; false when confidence was too low and
    /// the offset was forced to zero.
    pub applied: bool,
    /// Structured reason, rendered in the Explain panel.
    pub reason: String,
}

/// Histogram bucket. One second is fine enough to spot a daylight-saving hour
/// and coarse enough that shutter jitter does not split the peak.
const BUCKET_MS: i64 = 1_000;
/// Refinement window around the coarse mode.
const REFINE_MS: i64 = 2_000;
/// Below this, we refuse to guess and tell the photographer which frames to compare.
pub const MIN_CONFIDENCE: f64 = 0.6;
/// Frames sampled per body before pairing. 400 x 400 pairs is 160,000 integer
/// subtractions - microseconds - and a wedding's shape is already visible in a
/// sample that size.
const MAX_SAMPLES: usize = 400;

/// Estimate a clock offset for every body against the busiest one.
///
/// The body with the most frames is the reference and always gets offset zero.
/// For every other body we build a histogram of nearest-neighbour deltas, take
/// the mode as the coarse offset, then refine with the mean of the deltas inside
/// that bucket.
#[must_use]
pub fn estimate_offsets(cameras: &[CameraSamples]) -> Vec<OffsetEstimate> {
    let mut sorted: Vec<CameraSamples> = cameras
        .iter()
        .map(|c| {
            let mut times = c.times_ms.clone();
            times.sort_unstable();
            CameraSamples {
                body_serial: c.body_serial.clone(),
                times_ms: times,
            }
        })
        .collect();
    // DETERMINISM: frame count first, then serial, so ties never depend on input order.
    sorted.sort_by(|a, b| {
        b.times_ms
            .len()
            .cmp(&a.times_ms.len())
            .then_with(|| a.body_serial.cmp(&b.body_serial))
    });

    let Some(reference) = sorted.first().cloned() else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(sorted.len());
    out.push(OffsetEstimate {
        body_serial: reference.body_serial.clone(),
        offset_ms: 0,
        confidence: 1.0,
        applied: true,
        reason: format!(
            "reference body: most frames in the project ({})",
            reference.times_ms.len()
        ),
    });

    for camera in sorted.iter().skip(1) {
        out.push(estimate_one(&reference, camera));
    }
    out
}

/// Estimate one body's offset by pairing every sampled frame against every
/// sampled reference frame.
///
/// Nearest-neighbour matching cannot see an offset larger than the gap between
/// frames, and it aliases badly when two bodies shoot at a similar cadence: a
/// body one hour out looks like a body four seconds out. Pairing every sample
/// instead makes the true offset the tallest peak in the histogram no matter how
/// large it is, which is what recovers a daylight-saving hour or a battery-swap
/// year.
fn estimate_one(reference: &CameraSamples, camera: &CameraSamples) -> OffsetEstimate {
    let reference_sample = sample(&reference.times_ms);
    let camera_sample = sample(&camera.times_ms);

    if reference_sample.len() < 3 || camera_sample.len() < 3 {
        return OffsetEstimate {
            body_serial: camera.body_serial.clone(),
            offset_ms: 0,
            confidence: 0.0,
            applied: false,
            reason: "too few frames on one of the bodies to estimate an offset".to_string(),
        };
    }

    // DETERMINISM: BTreeMap keeps the histogram in bucket order, so a tie between
    // two equally populated buckets always resolves to the same one.
    let mut histogram: BTreeMap<i64, usize> = BTreeMap::new();
    for camera_time in &camera_sample {
        for reference_time in &reference_sample {
            let delta = reference_time - camera_time;
            *histogram.entry(delta.div_euclid(BUCKET_MS)).or_insert(0) += 1;
        }
    }

    let Some((bucket, count)) = histogram
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(bucket, count)| (*bucket, *count))
    else {
        return OffsetEstimate {
            body_serial: camera.body_serial.clone(),
            offset_ms: 0,
            confidence: 0.0,
            applied: false,
            reason: "no delta histogram could be built".to_string(),
        };
    };

    let coarse = bucket * BUCKET_MS;
    let refined: Vec<i64> = camera_sample
        .iter()
        .flat_map(|camera_time| {
            reference_sample
                .iter()
                .map(move |reference_time| reference_time - camera_time)
        })
        .filter(|delta| (*delta - coarse).abs() <= REFINE_MS)
        .collect();

    let offset_ms = if refined.is_empty() {
        coarse
    } else {
        let sum: i128 = refined.iter().map(|delta| i128::from(*delta)).sum();
        i64::try_from(sum / i128::try_from(refined.len()).unwrap_or(1)).unwrap_or(coarse)
    };

    // How many of the smaller body's frames found a partner at this offset. One
    // frame can only agree once, so the ratio is a real fraction of evidence.
    let matched = count.min(camera_sample.len().min(reference_sample.len()));
    #[allow(clippy::cast_precision_loss)]
    let confidence =
        (matched as f64 / camera_sample.len().min(reference_sample.len()) as f64).clamp(0.0, 1.0);
    let applied = confidence >= MIN_CONFIDENCE;

    OffsetEstimate {
        body_serial: camera.body_serial.clone(),
        offset_ms: if applied { offset_ms } else { 0 },
        confidence,
        applied,
        reason: if applied {
            format!(
                "{matched} of {} sampled frames agree within one second; offset {offset_ms} ms",
                camera_sample.len().min(reference_sample.len())
            )
        } else {
            format!(
                "only {matched} of {} sampled frames agree; refusing to guess, compare two \
                 frames manually",
                camera_sample.len().min(reference_sample.len())
            )
        },
    }
}

/// Evenly spaced sample of at most [`MAX_SAMPLES`] timestamps, preserving order.
fn sample(times: &[i64]) -> Vec<i64> {
    if times.len() <= MAX_SAMPLES {
        return times.to_vec();
    }
    let stride = times.len().div_ceil(MAX_SAMPLES);
    times.iter().step_by(stride).copied().collect()
}
