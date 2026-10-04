//! Skin: a broad prior that only decides where to *sample*, and a per-person model that does
//! the segmenting.
//!
//! # The prior is deliberately permissive, and it never segments anybody
//!
//! Skin chroma across the human range spans a hue of roughly 20 to 80 degrees in CIELAB and a
//! chroma from under 5 (the lightest and the darkest skin) to over 45 (a flushed face under
//! tungsten). A fixed region tight enough to be a useful segmenter is a region that fails
//! somebody at one end of that range, which is the failure every published fixed-ellipse skin
//! detector has been measured to have.
//!
//! So [`prior`] is wide, and it is consulted for exactly two things: whether a *detected* face
//! has a coherent skin-coloured centre at all (a cascade window on a wallpaper pattern does
//! not), and which pixels of that face to sample. Everything downstream of a face is a
//! distance from [`SkinModel`], which is fitted to that person's own cheeks and forehead.
//!
//! # A photograph without colour says so
//!
//! A black-and-white frame has no skin chroma to measure. `Canvas::is_colourful` is false
//! for it and every colour test in the crate then stands down rather than rejecting every face
//! for being grey.

/// The weight of a CIELAB colour under the broad skin prior, `0..=1`.
#[must_use]
pub fn prior(lab: [f32; 3]) -> f32 {
    let l = lab[0];
    let a = lab[1];
    let b = lab[2];
    let chroma = a.hypot(b);
    if !(4.0..=99.0).contains(&l) {
        return 0.0;
    }
    // The darkest and the lightest skin carry very little chroma, so the floor scales with how
    // close the lightness is to either end of the range.
    let extremity = ((30.0 - l) / 20.0).max((l - 85.0) / 10.0).clamp(0.0, 1.0);
    let floor = 5.0 - 3.0 * extremity;
    let chroma_w = ramp(chroma, floor - 2.0, floor + 1.5) * (1.0 - ramp(chroma, 52.0, 70.0));
    let hue = b.atan2(a).to_degrees();
    let hue_w = ramp(hue, 8.0, 22.0) * (1.0 - ramp(hue, 80.0, 96.0));
    // At very low chroma the hue is noise; let a near-neutral pixel through at half weight so
    // the darkest skin is sampled rather than skipped.
    let neutral = if chroma < floor + 2.0 { 0.5 } else { 0.0 };
    (chroma_w * hue_w).max(neutral * chroma_w.max(0.4) * ramp(hue, -20.0, 10.0).max(0.6))
}

/// A smooth step from 0 at `lo` to 1 at `hi`.
#[must_use]
pub fn ramp(x: f32, lo: f32, hi: f32) -> f32 {
    if hi <= lo {
        return if x >= hi { 1.0 } else { 0.0 };
    }
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One person's skin, measured from their own face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinModel {
    /// Mean CIELAB of the sampled skin.
    pub mean: [f32; 3],
    /// Inverse covariance of `a` and `b`, with a floor so a flat sample does not make the
    /// model a single colour.
    inv: [[f32; 2]; 2],
    /// The lightness band the sample spanned: 5th and 95th percentile.
    pub lightness: (f32, f32),
    /// Standard deviation of lightness across the sample.
    pub lightness_sd: f32,
    /// How many pixels the model was fitted on.
    pub samples: usize,
}

impl SkinModel {
    /// Fit a model to weighted samples, rejecting outliers twice.
    ///
    /// `None` when fewer than twelve samples carry weight: a model fitted on eleven pixels is
    /// arithmetic rather than a measurement of anybody.
    #[must_use]
    pub fn fit(samples: &[([f32; 3], f32)]) -> Option<Self> {
        let mut kept: Vec<([f32; 3], f32)> =
            samples.iter().copied().filter(|(_, w)| *w > 0.05).collect();
        if kept.len() < 12 {
            return None;
        }
        let mut model = Self::moments(&kept)?;
        for _ in 0..2 {
            let next: Vec<([f32; 3], f32)> = kept
                .iter()
                .copied()
                .filter(|(lab, _)| model.chroma_distance2(*lab) < 9.0)
                .collect();
            if next.len() < 12 {
                break;
            }
            kept = next;
            model = Self::moments(&kept)?;
        }
        Some(model)
    }

    fn moments(samples: &[([f32; 3], f32)]) -> Option<Self> {
        let total: f64 = samples.iter().map(|(_, w)| f64::from(*w)).sum();
        if total <= 1e-6 {
            return None;
        }
        let mut mean = [0.0_f64; 3];
        for (lab, w) in samples {
            for (m, v) in mean.iter_mut().zip(lab.iter()) {
                *m += f64::from(*v) * f64::from(*w);
            }
        }
        for m in &mut mean {
            *m /= total;
        }
        let (mut saa, mut sab, mut sbb, mut sll) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        for (lab, w) in samples {
            let w = f64::from(*w);
            let dl = f64::from(lab[0]) - mean[0];
            let da = f64::from(lab[1]) - mean[1];
            let db = f64::from(lab[2]) - mean[2];
            saa += w * da * da;
            sab += w * da * db;
            sbb += w * db * db;
            sll += w * dl * dl;
        }
        // A floor of three units of chroma: two photographs of one face differ by more than
        // that, and a model tighter than its own measurement noise rejects the person it was
        // fitted to.
        let floor = 9.0;
        let caa = saa / total + floor;
        let cbb = sbb / total + floor;
        let cab = sab / total;
        let det = caa * cbb - cab * cab;
        if det <= 1e-9 {
            return None;
        }
        let inv = [
            [(cbb / det) as f32, (-cab / det) as f32],
            [(-cab / det) as f32, (caa / det) as f32],
        ];
        let mut lights: Vec<f32> = samples.iter().map(|(lab, _)| lab[0]).collect();
        lights.sort_by(f32::total_cmp);
        let pick = |q: f32| -> f32 {
            let i = ((lights.len() as f32 - 1.0) * q).round() as usize;
            lights.get(i).copied().unwrap_or(mean[0] as f32)
        };
        Some(Self {
            mean: [mean[0] as f32, mean[1] as f32, mean[2] as f32],
            inv,
            lightness: (pick(0.05), pick(0.95)),
            lightness_sd: (sll / total).sqrt() as f32,
            samples: samples.len(),
        })
    }

    /// Squared Mahalanobis distance of a colour's chroma from this person's skin.
    ///
    /// The chroma is first scaled toward the model's lightness, because a cheek turning away
    /// from the light loses chroma roughly in proportion to the light it loses, and a shadowed
    /// cheek is still a cheek.
    #[must_use]
    pub fn chroma_distance2(&self, lab: [f32; 3]) -> f32 {
        let shade = ((self.mean[0] + 8.0) / (lab[0] + 8.0))
            .clamp(0.8, 1.35)
            .sqrt();
        let da = lab[1] * shade - self.mean[1];
        let db = lab[2] * shade - self.mean[2];
        da * (self.inv[0][0] * da + self.inv[0][1] * db)
            + db * (self.inv[1][0] * da + self.inv[1][1] * db)
    }

    /// How much a colour looks like this person's skin, `0..=1`.
    #[must_use]
    pub fn likelihood(&self, lab: [f32; 3]) -> f32 {
        let d2 = self.chroma_distance2(lab);
        let chroma = (-0.5 * d2 / 2.2).exp();
        let lo = self.lightness.0 - 18.0 - self.lightness_sd;
        let hi = self.lightness.1 + 14.0 + self.lightness_sd * 0.5;
        let light = ramp(lab[0], lo - 10.0, lo) * (1.0 - ramp(lab[0], hi, hi + 8.0));
        chroma * light
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::linear_srgb_to_lab;
    use aura_raw::colour::curve::srgb_decode;

    fn lab8(rgb: [u8; 3]) -> [f32; 3] {
        linear_srgb_to_lab([
            srgb_decode(f32::from(rgb[0]) / 255.0),
            srgb_decode(f32::from(rgb[1]) / 255.0),
            srgb_decode(f32::from(rgb[2]) / 255.0),
        ])
    }

    /// The ten published Monk Skin Tone swatches.
    const MONK: [[u8; 3]; 10] = [
        [246, 237, 228],
        [243, 231, 219],
        [247, 234, 208],
        [234, 218, 186],
        [215, 189, 150],
        [160, 126, 86],
        [130, 92, 67],
        [96, 65, 52],
        [58, 49, 42],
        [41, 36, 32],
    ];

    #[test]
    fn the_prior_admits_every_monk_swatch() {
        for (i, swatch) in MONK.iter().enumerate() {
            let w = prior(lab8(*swatch));
            assert!(w >= 0.3, "MST {} scored {w}", i + 1);
        }
    }

    #[test]
    fn the_prior_rejects_sky_grass_and_saturated_red() {
        assert!(prior(lab8([90, 140, 220])) < 0.05);
        assert!(prior(lab8([60, 150, 50])) < 0.05);
        assert!(prior(lab8([230, 20, 30])) < 0.2);
    }

    #[test]
    fn a_model_fitted_to_one_person_prefers_that_person() {
        for (i, swatch) in MONK.iter().enumerate() {
            let base = lab8(*swatch);
            let samples: Vec<([f32; 3], f32)> = (0..64)
                .map(|k| {
                    let j = (k % 8) as f32 - 3.5;
                    ([base[0] + j, base[1] + j * 0.2, base[2] - j * 0.2], 1.0)
                })
                .collect();
            let model = SkinModel::fit(&samples).unwrap();
            assert!(model.likelihood(base) > 0.8, "MST {}", i + 1);
            // A clearly different colour at the same lightness is not this person's skin.
            let other = [base[0], base[1] - 25.0, base[2] - 30.0];
            assert!(model.likelihood(other) < 0.05, "MST {}", i + 1);
        }
    }

    #[test]
    fn a_shadowed_cheek_is_still_the_same_skin() {
        let base = lab8([200, 150, 125]);
        let samples: Vec<([f32; 3], f32)> = (0..40).map(|_| (base, 1.0)).collect();
        let model = SkinModel::fit(&samples).unwrap();
        let shadow = lab8([140, 102, 84]);
        assert!(
            model.likelihood(shadow) > 0.3,
            "{}",
            model.likelihood(shadow)
        );
    }

    #[test]
    fn too_few_samples_is_no_model() {
        assert!(SkinModel::fit(&[([50.0, 10.0, 15.0], 1.0); 5]).is_none());
    }
}
