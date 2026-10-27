//! Measured contour warnings and conservative acceptance of QC corrections.
//!
//! This is an independent image measurement, not a learned defect classifier. A clean
//! result means only that the named tests passed at the recorded render dimensions.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use aura_vision::portrait::PortraitFace;
use serde::{Deserialize, Serialize};

mod regions;
pub use regions::{Region, RegionReading, REGIONS};

/// One face and the neck immediately below it; no claim of dense anatomical parsing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContourReading {
    pub face: usize,
    pub sampled_pairs: usize,
    pub new_seam_pairs: usize,
    pub source_edge_pairs: usize,
    pub weakened_edge_pairs: usize,
    pub review: bool,
    #[serde(default)]
    pub regions: Vec<RegionReading>,
}

impl ContourReading {
    #[must_use]
    pub fn needs_review(&self) -> bool {
        self.review || self.regions.iter().any(|r| r.review)
    }
    /// Old aggregate-only reports do not certify local contour coverage.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.regions.len() == REGIONS.len()
            && REGIONS.iter().all(|region| {
                self.regions
                    .iter()
                    .filter(|r| r.region == *region && r.measured && r.sampled_pairs >= 64)
                    .count()
                    == 1
            })
    }

    #[must_use]
    pub fn non_regressing(&self, old: &Self) -> bool {
        self.face == old.face
            && self.complete()
            && old.complete()
            && (!self.needs_review() || old.needs_review())
            && self.new_seam_pairs <= old.new_seam_pairs.saturating_add(3)
            && self.weakened_edge_pairs <= old.weakened_edge_pairs.saturating_add(3)
            && self.regions.iter().all(|r| {
                old.regions
                    .iter()
                    .find(|o| o.region == r.region)
                    .is_some_and(|old| r.non_regressing(old))
            })
    }
}

/// Compare local RGB differences, which reject a constant colour offset. Sampling covers
/// the face plus 25% of its height below the detector box. It detects abrupt new seams and
/// strongly erased source edges; it cannot identify every halo, blemish or identity change.
#[must_use]
pub fn contour(
    before: &[u8],
    after: &[u8],
    w: usize,
    h: usize,
    face: &PortraitFace,
    index: usize,
) -> Option<ContourReading> {
    let n = w.checked_mul(h)?.checked_mul(3)?;
    if w < 8
        || h < 8
        || before.len() != n
        || after.len() != n
        || !face
            .bounds
            .iter()
            .chain(face.landmarks.iter().flatten())
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    {
        return None;
    }
    let [l, t, r, b] = face.bounds;
    if l >= r || t >= b {
        return None;
    }
    let eyes = face.landmarks;
    let iod = ((eyes[0][0] - eyes[1][0]) * w as f32).hypot((eyes[0][1] - eyes[1][1]) * h as f32);
    if iod < 20.0 {
        return None;
    }
    let mut regions = regions::Sampler::new(face, w, h, iod)?;
    let gap = (iod * 0.015).round().clamp(1.0, 4.0) as usize;
    let step = (iod / 100.0).floor().max(1.0) as usize;
    let x0 = (l * w as f32) as usize;
    let x1 = ((r * w as f32).ceil() as usize).min(w.saturating_sub(gap));
    let y0 = (t * h as f32) as usize;
    let y1 =
        (((b + (b - t) * 0.25).min(1.0) * h as f32).ceil() as usize).min(h.saturating_sub(gap));
    let mut result = ContourReading {
        face: index,
        sampled_pairs: 0,
        new_seam_pairs: 0,
        source_edge_pairs: 0,
        weakened_edge_pairs: 0,
        review: false,
        regions: Vec::new(),
    };
    let delta = |pixels: &[u8], i: usize, j: usize| -> [f32; 3] {
        std::array::from_fn(|c| (f32::from(pixels[j + c]) - f32::from(pixels[i + c])) / 255.0)
    };
    let norm = |p: [f32; 3]| p.iter().map(|v| v.abs()).sum::<f32>() / 3.0;
    for y in (y0..y1).step_by(step) {
        for x in (x0..x1).step_by(step) {
            let i = (y * w + x) * 3;
            for (xx, yy) in [(x + gap, y), (x, y + gap)] {
                let j = (yy * w + xx) * 3;
                let a = delta(before, i, j);
                let b = delta(after, i, j);
                let (old, new) = (norm(a), norm(b));
                let departure = norm(std::array::from_fn(|c| b[c] - a[c]));
                regions.pair(
                    (x + xx) as f32 * 0.5,
                    (y + yy) as f32 * 0.5,
                    old,
                    new,
                    departure,
                );
                result.sampled_pairs += 1;
                result.new_seam_pairs += usize::from(old < 0.025 && new > 0.06 && departure > 0.04);
                if old > 0.05 {
                    result.source_edge_pairs += 1;
                    result.weakened_edge_pairs += usize::from(new < old * 0.45 && departure > 0.03);
                }
            }
        }
    }
    if result.sampled_pairs < 200 {
        return None;
    }
    // Pilot thresholds, deliberately reported as review warnings rather than proof of a
    // specific anatomical error. Validate on labelled real defects before auto-remediation.
    result.review = result.new_seam_pairs >= ((iod * 0.15).ceil() as usize).max(12)
        || (result.source_edge_pairs >= 20
            && result.weakened_edge_pairs >= 12
            && result.weakened_edge_pairs as f32 / result.source_edge_pairs as f32 > 0.15);
    result.regions = regions.finish();
    result.review |= result.regions.iter().any(|r| r.review);
    Some(result)
}

/// A correction must improve a failed metric, retain every available reading and avoid
/// worsening either texture loss or colour drift. Missing and non-finite input never passes.
#[must_use]
pub fn correction_improves(
    before: (Option<f32>, Option<f32>),
    after: (Option<f32>, Option<f32>),
    texture_floor: f32,
    shift_ceiling: f32,
) -> bool {
    let finite = |v: Option<f32>| v.filter(|v| v.is_finite());
    let (a, b, c, d) = (
        finite(before.0),
        finite(before.1),
        finite(after.0),
        finite(after.1),
    );
    if (a.is_some() && c.is_none()) || (b.is_some() && d.is_none()) {
        return false;
    }
    let texture_ok = a
        .zip(c)
        .is_none_or(|(a, c)| (texture_floor - c).max(0.0) <= (texture_floor - a).max(0.0) + 1e-5);
    let colour_ok = b.zip(d).is_none_or(|(b, d)| d <= b + 1e-5);
    let improved = a
        .zip(c)
        .is_some_and(|(a, c)| a < texture_floor && c > a + 1e-4)
        || b.zip(d)
            .is_some_and(|(b, d)| b > shift_ceiling && d < b - 1e-5);
    texture_ok && colour_ok && improved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> PortraitFace {
        PortraitFace {
            bounds: [0.1, 0.1, 0.9, 0.8],
            landmarks: [
                [0.3, 0.3],
                [0.7, 0.3],
                [0.5, 0.5],
                [0.35, 0.65],
                [0.65, 0.65],
            ],
            confidence: 0.99,
        }
    }

    #[test]
    fn additive_colour_change_is_not_a_seam_but_a_painted_patch_is() {
        let (w, h) = (160, 160);
        let before = vec![100; w * h * 3];
        let mut after = vec![110; w * h * 3];
        assert!(!contour(&before, &after, w, h, &face(), 0).unwrap().review);
        for y in 75..95 {
            for x in 60..100 {
                after[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(160);
            }
        }
        let reading = contour(&before, &after, w, h, &face(), 0).unwrap();
        assert!(reading.review && reading.new_seam_pairs > 30, "{reading:?}");
    }

    #[test]
    fn erased_source_edges_and_invalid_input_are_not_clean() {
        let (w, h) = (160, 160);
        let mut before = vec![100; w * h * 3];
        for y in 55..120 {
            for x in 75..85 {
                before[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(60);
            }
        }
        let after = vec![100; w * h * 3];
        assert!(contour(&before, &after, w, h, &face(), 0).unwrap().review);
        assert!(contour(&[], &after, w, h, &face(), 0).is_none());
        let mut small = face();
        small.landmarks[1] = small.landmarks[0];
        assert!(contour(&before, &after, w, h, &small, 0).is_none());
    }

    #[test]
    fn a_damaged_eye_cannot_hide_among_untouched_edges_elsewhere() {
        let (w, h) = (240, 240);
        let mut before = vec![100_u8; w * h * 3];
        // Lots of intact strong detail elsewhere dilutes a whole-face edge fraction.
        for y in 140..190 {
            for x in 30..210 {
                before[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(if (x / 3) % 2 == 0 {
                    40
                } else {
                    100
                });
            }
        }
        for y in 86..105 {
            for x in 68..74 {
                before[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(40);
            }
        }
        let mut after = before.clone();
        for y in 86..105 {
            for x in 68..74 {
                after[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(100);
            }
        }
        let reading = contour(&before, &after, w, h, &face(), 0).unwrap();
        assert!(reading.complete());
        assert!(reading.weakened_edge_pairs as f32 / (reading.source_edge_pairs as f32) < 0.15);
        assert!(reading
            .regions
            .iter()
            .any(|r| r.region == Region::EyeImageLeft && r.review));
        assert!(reading.needs_review());
        let clean = contour(&before, &before, w, h, &face(), 0).unwrap();
        assert!(!reading.non_regressing(&clean));
        assert!(clean.non_regressing(&reading));
    }

    #[test]
    fn local_coverage_is_required_and_follows_a_rotated_face() {
        let (w, h) = (240, 240);
        let before = vec![100_u8; w * h * 3];
        let mut after = before.clone();
        for y in 86..105 {
            for x in 65..75 {
                after[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(150);
            }
        }
        let reading = contour(&before, &after, w, h, &face(), 0).unwrap();
        let mut turned = vec![0; after.len()];
        for y in 0..h {
            for x in 0..w {
                let j = (x * w + (h - 1 - y)) * 3;
                turned[j..j + 3].copy_from_slice(&after[(y * w + x) * 3..(y * w + x) * 3 + 3]);
            }
        }
        let mut geometry = face();
        geometry.bounds = [0.2, 0.1, 0.9, 0.9];
        geometry.landmarks = geometry.landmarks.map(|[x, y]| [1.0 - y, x]);
        let rotated = contour(&before, &turned, w, h, &geometry, 0).unwrap();
        assert!(rotated.needs_review());
        assert_eq!(
            rotated.regions.iter().filter(|r| r.review).count(),
            reading.regions.iter().filter(|r| r.review).count()
        );
        let mut missing = reading;
        missing.regions.pop();
        assert!(!missing.complete());
        missing.regions.push(missing.regions[0].clone());
        assert!(
            !missing.complete(),
            "duplicate region must not fill a missing measurement"
        );
        geometry.confidence = f32::NAN;
        assert!(contour(&before, &turned, w, h, &geometry, 0).is_none());
    }

    #[test]
    fn a_qc_repair_must_improve_without_collateral_regression() {
        let old = (Some(0.4), Some(0.02));
        assert!(correction_improves(
            old,
            (Some(0.7), Some(0.01)),
            0.6,
            0.012
        ));
        assert!(!correction_improves(old, old, 0.6, 0.012));
        assert!(!correction_improves(
            old,
            (Some(0.7), Some(0.03)),
            0.6,
            0.012
        ));
        assert!(!correction_improves(old, (None, Some(0.01)), 0.6, 0.012));
        assert!(!correction_improves(
            old,
            (Some(f32::NAN), Some(0.01)),
            0.6,
            0.012
        ));
    }
}
