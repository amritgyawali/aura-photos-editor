//! Sampled colour eligibility; shared by preview, saved coverage and rendering.
use aura_raw::colour::{
    de2000::{xyz_d65_to_lab, Lab},
    working_space::working_to_xyz,
};
use aura_recipe::retouch_tools::ColorRange;

pub(crate) struct Prepared {
    sample: Lab,
    tolerance: f64,
    softness: f64,
    lightness: f64,
}

fn lab(pixel: &[f32]) -> Lab {
    let channel = |index| f64::from(pixel.get(index).copied().unwrap_or(0.0));
    xyz_d65_to_lab(working_to_xyz([channel(0), channel(1), channel(2)]))
}

impl Prepared {
    pub(crate) fn new(range: &ColorRange, rgb: &[f32], w: usize, h: usize) -> Self {
        let x = ((range.sample[0] * w as f32) as usize).min(w.saturating_sub(1));
        let y = ((range.sample[1] * h as f32) as usize).min(h.saturating_sub(1));
        let index = (y * w + x) * 3;
        Self {
            sample: lab(rgb.get(index..index + 3).unwrap_or(&[])),
            tolerance: f64::from(range.tolerance),
            softness: f64::from(range.softness),
            lightness: f64::from(range.lightness),
        }
    }

    pub(crate) fn weight(&self, pixel: &[f32]) -> f32 {
        let value = lab(pixel);
        let distance = ((value.l - self.sample.l) * self.lightness)
            .hypot(value.a - self.sample.a)
            .hypot(value.b - self.sample.b);
        if distance <= self.tolerance {
            return 1.0;
        }
        if self.softness <= 0.0 {
            return 0.0;
        }
        let weight = (1.0 - (distance - self.tolerance) / self.softness).clamp(0.0, 1.0);
        (weight * weight * (3.0 - 2.0 * weight)) as f32
    }
}
