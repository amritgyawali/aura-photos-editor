//! CIEDE2000, the yardstick the golden suite measures colour drift with.
//!
//! Pixel-wise RGB differences are not perceptual: a two-code shift in a dark
//! blue is invisible, the same shift in a skin midtone is a complaint from the
//! photographer. Every colour tolerance in this project is therefore stated in
//! dE2000, and this module is the single implementation of it.
//!
//! Reference: Sharma, Wu and Dalal, *The CIEDE2000 Color-Difference Formula*
//! (2005). The test module checks the published worked pairs from that paper.

use crate::colour::matrix::{Vec3, WHITE_D65};

/// CIE L\*a\*b\* under a stated white point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lab {
    /// Lightness, 0 to 100.
    pub l: f64,
    /// Green-red axis.
    pub a: f64,
    /// Blue-yellow axis.
    pub b: f64,
}

fn f_lab(t: f64) -> f64 {
    const DELTA: f64 = 6.0 / 29.0;
    if t > DELTA * DELTA * DELTA {
        t.cbrt()
    } else {
        t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

/// CIE XYZ to L\*a\*b\* under the given white point.
#[must_use]
pub fn xyz_to_lab(xyz: Vec3, white: Vec3) -> Lab {
    let ratio = |index: usize| -> f64 {
        let value = xyz.get(index).copied().unwrap_or(0.0);
        let reference = white.get(index).copied().unwrap_or(1.0);
        if reference.abs() < 1e-12 {
            0.0
        } else {
            f_lab(value / reference)
        }
    };
    let fx = ratio(0);
    let fy = ratio(1);
    let fz = ratio(2);
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// CIE XYZ under D65 to L\*a\*b\*, the common case in this pipeline.
#[must_use]
pub fn xyz_d65_to_lab(xyz: Vec3) -> Lab {
    xyz_to_lab(xyz, WHITE_D65)
}

/// The CIEDE2000 difference between two colours.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn ciede2000(reference: Lab, sample: Lab) -> f64 {
    let (l1, a1, b1) = (reference.l, reference.a, reference.b);
    let (l2, a2, b2) = (sample.l, sample.a, sample.b);

    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let c_bar = 0.5 * (c1 + c2);
    let c_bar7 = c_bar.powi(7);
    let g = 0.5 * (1.0 - (c_bar7 / (c_bar7 + 25.0f64.powi(7))).sqrt());

    let a1p = (1.0 + g) * a1;
    let a2p = (1.0 + g) * a2;
    let c1p = (a1p * a1p + b1 * b1).sqrt();
    let c2p = (a2p * a2p + b2 * b2).sqrt();

    let h1p = hue_angle(b1, a1p);
    let h2p = hue_angle(b2, a2p);

    let dlp = l2 - l1;
    let dcp = c2p - c1p;
    let dhp = if c1p * c2p < 1e-12 {
        0.0
    } else {
        let raw = h2p - h1p;
        if raw > 180.0 {
            raw - 360.0
        } else if raw < -180.0 {
            raw + 360.0
        } else {
            raw
        }
    };
    let dhp_big = 2.0 * (c1p * c2p).sqrt() * (dhp.to_radians() / 2.0).sin();

    let lp_bar = 0.5 * (l1 + l2);
    let cp_bar = 0.5 * (c1p + c2p);
    let hp_bar = mean_hue(h1p, h2p, c1p, c2p);

    let t = 1.0 - 0.17 * ((hp_bar - 30.0).to_radians()).cos()
        + 0.24 * ((2.0 * hp_bar).to_radians()).cos()
        + 0.32 * ((3.0 * hp_bar + 6.0).to_radians()).cos()
        - 0.20 * ((4.0 * hp_bar - 63.0).to_radians()).cos();

    let d_theta = 30.0 * (-(((hp_bar - 275.0) / 25.0).powi(2))).exp();
    let cp_bar7 = cp_bar.powi(7);
    let rc = 2.0 * (cp_bar7 / (cp_bar7 + 25.0f64.powi(7))).sqrt();
    let sl = 1.0 + (0.015 * (lp_bar - 50.0).powi(2)) / (20.0 + (lp_bar - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cp_bar;
    let sh = 1.0 + 0.015 * cp_bar * t;
    let rt = -((2.0 * d_theta).to_radians()).sin() * rc;

    let term_l = dlp / sl;
    let term_c = dcp / sc;
    let term_h = dhp_big / sh;

    (term_l * term_l + term_c * term_c + term_h * term_h + rt * term_c * term_h).sqrt()
}

fn hue_angle(b: f64, ap: f64) -> f64 {
    if b.abs() < 1e-12 && ap.abs() < 1e-12 {
        return 0.0;
    }
    let degrees = b.atan2(ap).to_degrees();
    if degrees < 0.0 {
        degrees + 360.0
    } else {
        degrees
    }
}

fn mean_hue(h1p: f64, h2p: f64, c1p: f64, c2p: f64) -> f64 {
    if c1p * c2p < 1e-12 {
        return h1p + h2p;
    }
    let difference = (h1p - h2p).abs();
    let sum = h1p + h2p;
    if difference <= 180.0 {
        0.5 * sum
    } else if sum < 360.0 {
        0.5 * (sum + 360.0)
    } else {
        0.5 * (sum - 360.0)
    }
}

/// Mean and maximum dE2000 over paired colours, the shape every golden
/// assertion in the project reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeltaESummary {
    /// Arithmetic mean over all pairs.
    pub mean: f64,
    /// Worst single pair.
    pub max: f64,
    /// How many pairs were compared.
    pub count: usize,
}

/// Summarise the difference between two equal-length colour lists.
#[must_use]
pub fn summarise(reference: &[Lab], sample: &[Lab]) -> DeltaESummary {
    let mut total = 0.0;
    let mut worst = 0.0f64;
    let mut count = 0usize;
    for (left, right) in reference.iter().zip(sample.iter()) {
        let delta = ciede2000(*left, *right);
        total += delta;
        worst = worst.max(delta);
        count += 1;
    }
    DeltaESummary {
        mean: if count == 0 {
            0.0
        } else {
            total / count as f64
        },
        max: worst,
        count,
    }
}
