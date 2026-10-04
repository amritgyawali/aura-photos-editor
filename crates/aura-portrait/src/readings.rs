//! What a portrait measures as, before anybody decides what to do about it.
//!
//! An automatic retouch is sized from these numbers rather than from a preset alone: a face
//! with even skin gets less smoothing than a face with blotches, closed lips get no teeth
//! whitening, and white sclera get no whitening at all. Every reading is relative to the same
//! person's own face - the under-eye depth is a difference from *their* cheek, the shine is a
//! share of *their* skin - so no reading compares anybody against a constant.
//!
//! A reading that cannot be measured is `None`, not zero. Zero teeth yellowness and "the mouth
//! is closed" are different facts and lead to different decisions.

use crate::canvas::Canvas;
use crate::parse::PortraitMap;
use crate::plane::Plane;
use crate::regions::Region;

/// The measurements an automatic retouch is sized from.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PortraitReadings {
    /// Faces found.
    pub faces: usize,
    /// Standard deviation of the mid band of facial skin, in CIELAB lightness: blotches and
    /// uneven texture. Pores are below this band and do not count.
    pub skin_roughness: Option<f32>,
    /// Standard deviation of redness (CIELAB `a`) across facial skin: uneven tone.
    pub tone_variation: Option<f32>,
    /// How much darker the skin under the eyes is than the face around it, in lightness units.
    pub under_eye_depth: Option<f32>,
    /// Share of facial skin that is specular shine.
    pub shine_share: Option<f32>,
    /// Mean CIELAB `b` of visible teeth: how yellow they are. `None` when the mouth is closed.
    pub teeth_yellowness: Option<f32>,
    /// Mean CIELAB `a` of the whites of the eyes: how red they are. `None` when not visible.
    pub sclera_redness: Option<f32>,
    /// Share of the frame that is hair.
    pub hair_coverage: f32,
    /// Mean CIELAB lightness of facial skin.
    pub face_lightness: Option<f32>,
    /// Median CIELAB lightness of the whole frame.
    pub frame_lightness: f32,
}

/// Weighted mean and standard deviation of one value over a plane, counting pixels at or above
/// `cut`. `None` when fewer than `min` pixels qualify.
fn stats(plane: &Plane, cut: f32, min: usize, value: impl Fn(usize) -> f32) -> Option<(f32, f32)> {
    let mut n = 0.0_f64;
    let mut sum = 0.0_f64;
    let mut sq = 0.0_f64;
    let mut count = 0;
    for (i, w) in plane.values.iter().enumerate() {
        if *w < cut {
            continue;
        }
        let v = f64::from(value(i));
        let w = f64::from(*w);
        n += w;
        sum += w * v;
        sq += w * v * v;
        count += 1;
    }
    if count < min || n <= 0.0 {
        return None;
    }
    let mean = sum / n;
    let var = (sq / n - mean * mean).max(0.0);
    Some((mean as f32, var.sqrt() as f32))
}

/// Measure a parsed portrait.
#[must_use]
pub fn read(canvas: &Canvas, map: &PortraitMap) -> PortraitReadings {
    let mut lights: Vec<f32> = canvas.lab.iter().map(|lab| lab[0]).collect();
    lights.sort_by(f32::total_cmp);
    let frame_lightness = lights.get(lights.len() / 2).copied().unwrap_or(50.0);
    let mut out = PortraitReadings {
        faces: map.faces.len(),
        frame_lightness,
        hair_coverage: map.stat(Region::Hair).map_or(0.0, |s| s.coverage),
        ..PortraitReadings::default()
    };
    if map.faces.is_empty() {
        return out;
    }
    let get = |r: Region| {
        map.plane(r)
            .cloned()
            .unwrap_or_else(|| Plane::zeros(map.width, map.height))
    };
    let lab = |i: usize| canvas.lab.get(i).copied().unwrap_or([0.0; 3]);
    let face_skin = get(Region::Skin).mul(&get(Region::Face));
    let face_stats = stats(&face_skin, 0.6, 64, |i| lab(i)[0]);
    out.face_lightness = face_stats.map(|s| s.0);
    out.tone_variation = stats(&face_skin, 0.6, 64, |i| lab(i)[1]).map(|s| s.1);

    // The mid band: lightness smoothed at a pore's scale, less lightness smoothed at a blotch's.
    let unit = {
        let mut units: Vec<f32> = map
            .faces
            .iter()
            .map(crate::face::FaceGeometry::interocular)
            .collect();
        units.sort_by(f32::total_cmp);
        units.get(units.len() / 2).copied().unwrap_or(20.0)
    };
    let lightness = Plane::from_values(
        canvas.width,
        canvas.height,
        canvas.lab.iter().map(|l| l[0]).collect(),
    );
    let masked = |radius: f32| -> Plane {
        let r = radius.round().max(1.0) as u32;
        let top = lightness.mul(&face_skin).blur(r);
        let bottom = face_skin.blur(r);
        top.zip(&bottom, |t, b| if b > 1e-3 { t / b } else { 0.0 })
    };
    let fine = masked(unit * 0.025);
    let coarse = masked(unit * 0.22);
    let band = fine.zip(&coarse, |f, c| f - c);
    out.skin_roughness = stats(&face_skin, 0.8, 64, |i| {
        band.values.get(i).copied().unwrap_or(0.0)
    })
    .map(|s| s.1);

    if let Some((face_l, face_sd)) = face_stats {
        out.under_eye_depth =
            stats(&get(Region::UnderEyes), 0.5, 16, |i| lab(i)[0]).map(|(l, _)| face_l - l);
        let mut shiny = 0.0_f64;
        let mut total = 0.0_f64;
        for (i, w) in face_skin.values.iter().enumerate() {
            if *w < 0.6 {
                continue;
            }
            total += 1.0;
            let p = lab(i);
            if p[0] > face_l + 2.0 * face_sd.max(2.0) && p[1].hypot(p[2]) < 12.0 {
                shiny += 1.0;
            }
        }
        out.shine_share = (total > 0.0).then(|| (shiny / total) as f32);
    }
    out.teeth_yellowness = stats(&get(Region::Teeth), 0.6, 12, |i| lab(i)[2]).map(|s| s.0);
    out.sclera_redness = stats(&get(Region::Sclera), 0.6, 8, |i| lab(i)[1]).map(|s| s.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{portrait, PortraitSpec};

    #[test]
    fn a_painted_portrait_reads_open_teeth_and_white_eyes() {
        let p = portrait(&PortraitSpec::default());
        let canvas = Canvas::from_srgb8(&p.rgb, p.width, p.height, 1024).unwrap();
        let map = crate::analyse(&canvas, &[]);
        let r = read(&canvas, &map);
        assert_eq!(r.faces, 1);
        let teeth = r.teeth_yellowness.expect("the painted mouth is open");
        assert!(teeth > 3.0, "painted teeth are slightly yellow: {teeth}");
        assert!(r.skin_roughness.is_some() && r.face_lightness.is_some());
        assert!(r.under_eye_depth.unwrap_or(0.0) > -5.0);
    }

    #[test]
    fn a_frame_without_a_face_reads_nothing_about_a_face() {
        let canvas = Canvas::from_srgb8(&vec![100; 3 * 64 * 48], 64, 48, 1024).unwrap();
        let map = crate::analyse(&canvas, &[]);
        let r = read(&canvas, &map);
        assert_eq!(r.faces, 0);
        assert!(r.skin_roughness.is_none() && r.teeth_yellowness.is_none());
    }
}
