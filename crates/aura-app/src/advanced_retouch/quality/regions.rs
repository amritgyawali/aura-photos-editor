//! Landmark-oriented local contour measurements. Ellipses are approximate inspection
//! zones, not semantic skin masks. They must never be used to paint or select skin.
#![allow(clippy::indexing_slicing)]
use aura_vision::portrait::PortraitFace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    EyeImageLeft,
    EyeImageRight,
    Nose,
    Mouth,
    ChinNeck,
}

pub const REGIONS: [Region; 5] = [
    Region::EyeImageLeft,
    Region::EyeImageRight,
    Region::Nose,
    Region::Mouth,
    Region::ChinNeck,
];

impl Region {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::EyeImageLeft => "image-left eye and under-eye",
            Self::EyeImageRight => "image-right eye and under-eye",
            Self::Nose => "nose",
            Self::Mouth => "mouth",
            Self::ChinNeck => "chin and adjacent neck",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionReading {
    pub region: Region,
    pub sampled_pairs: usize,
    pub new_seam_pairs: usize,
    pub source_edge_pairs: usize,
    pub weakened_edge_pairs: usize,
    pub measured: bool,
    pub review: bool,
}

impl RegionReading {
    fn new(region: Region) -> Self {
        Self {
            region,
            sampled_pairs: 0,
            new_seam_pairs: 0,
            source_edge_pairs: 0,
            weakened_edge_pairs: 0,
            measured: false,
            review: false,
        }
    }

    #[must_use]
    pub fn non_regressing(&self, old: &Self) -> bool {
        self.region == old.region
            && self.measured
            && old.measured
            && (!self.review || old.review)
            && self.new_seam_pairs <= old.new_seam_pairs.saturating_add(3)
            && self.weakened_edge_pairs <= old.weakened_edge_pairs.saturating_add(3)
    }
}

pub(super) struct Sampler {
    centres: [[f32; 2]; 5],
    radii: [[f32; 2]; 5],
    u: [f32; 2],
    v: [f32; 2],
    iod: f32,
    readings: [RegionReading; 5],
}

impl Sampler {
    pub(super) fn new(face: &PortraitFace, w: usize, h: usize, iod: f32) -> Option<Self> {
        let p = face.landmarks.map(|p| [p[0] * w as f32, p[1] * h as f32]);
        let (left, right) = if p[0][0] <= p[1][0] {
            (p[0], p[1])
        } else {
            (p[1], p[0])
        };
        let u = [(right[0] - left[0]) / iod, (right[1] - left[1]) / iod];
        let mouth = [(p[3][0] + p[4][0]) * 0.5, (p[3][1] + p[4][1]) * 0.5];
        let mid = [(left[0] + right[0]) * 0.5, (left[1] + right[1]) * 0.5];
        let mut v = [-u[1], u[0]];
        let dot = |a: [f32; 2], b: [f32; 2]| a[0] * b[0] + a[1] * b[1];
        if dot([mouth[0] - mid[0], mouth[1] - mid[1]], v) < 0.0 {
            v = v.map(|v| -v);
        }
        let depth = |p: [f32; 2]| dot([p[0] - mid[0], p[1] - mid[1]], v) / iod;
        if !face.confidence.is_finite()
            || face.confidence < 0.5
            || depth(p[2]) < 0.1
            || depth(mouth) < depth(p[2]) + 0.05
        {
            return None;
        }
        let down = |p: [f32; 2], d: f32| [p[0] + v[0] * d * iod, p[1] + v[1] * d * iod];
        Some(Self {
            centres: [
                down(left, 0.18),
                down(right, 0.18),
                p[2],
                mouth,
                down(mouth, 0.50),
            ],
            radii: [
                [0.38, 0.30],
                [0.38, 0.30],
                [0.30, 0.34],
                [0.48, 0.22],
                [0.70, 0.38],
            ],
            u,
            v,
            iod,
            readings: REGIONS.map(RegionReading::new),
        })
    }

    pub(super) fn pair(&mut self, x: f32, y: f32, old: f32, new: f32, departure: f32) {
        for i in 0..5 {
            let dx = (x - self.centres[i][0]) / self.iod;
            let dy = (y - self.centres[i][1]) / self.iod;
            let a = (dx * self.u[0] + dy * self.u[1]) / self.radii[i][0];
            let b = (dx * self.v[0] + dy * self.v[1]) / self.radii[i][1];
            if a * a + b * b > 1.0 {
                continue;
            }
            let reading = &mut self.readings[i];
            reading.sampled_pairs += 1;
            reading.new_seam_pairs += usize::from(old < 0.025 && new > 0.06 && departure > 0.04);
            if old > 0.05 {
                reading.source_edge_pairs += 1;
                reading.weakened_edge_pairs += usize::from(new < old * 0.45 && departure > 0.03);
            }
        }
    }

    pub(super) fn finish(mut self) -> Vec<RegionReading> {
        for reading in &mut self.readings {
            reading.measured = reading.sampled_pairs >= 64;
            // Pilot review limits; a local concentration cannot be diluted by thousands
            // of untouched eyebrow/hair edges elsewhere on the face.
            reading.review = reading.measured
                && (reading.new_seam_pairs >= ((self.iod * 0.05).ceil() as usize).max(8)
                    || (reading.source_edge_pairs >= 12
                        && reading.weakened_edge_pairs >= 8
                        && reading.weakened_edge_pairs as f32 / reading.source_edge_pairs as f32
                            > 0.25));
        }
        self.readings.into()
    }
}
