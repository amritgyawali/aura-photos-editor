//! Synthetic portraits with a known answer.
//!
//! There is no consented face data in this repository, so every gate in this crate is
//! measured against a face *painted* here: a head with form shading, eye sockets, a lid line,
//! sclera, iris and pupil, brows, a nose shadow, lips and teeth, hair and a neck, on a plain
//! ground - at any of the ten Monk Skin Tone swatches, any size, any position and any tilt.
//!
//! The painter was tuned against OpenCV's own cascade until the shapes a cascade reads (an
//! eye band darker than the cheeks because of the socket and the lashes, a bright forehead) are
//! there for every tone. That matters: an earlier painter drew a large, bright sclera and no
//! socket, which inverts the eye band on the darkest skin, and it made the detector look
//! unfair for a reason that was the painter's rather than the detector's. What these fixtures
//! prove is the arithmetic. They are not evidence about anybody's real face.

use crate::plane::Plane;

/// The ten published Monk Skin Tone swatches, lightest first.
pub const MONK: [[u8; 3]; 10] = [
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

/// What to paint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortraitSpec {
    /// Frame width.
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Skin, encoded sRGB.
    pub skin: [u8; 3],
    /// Hair, encoded sRGB.
    pub hair: [u8; 3],
    /// The ground, encoded sRGB.
    pub ground: [u8; 3],
    /// Face centre, as a fraction of the frame.
    pub centre: [f32; 2],
    /// Face half-width as a fraction of the frame width; the half-height is 4/3 of it.
    pub radius: f32,
    /// Tilt of the head, radians, clockwise.
    pub roll: f32,
}

impl Default for PortraitSpec {
    fn default() -> Self {
        Self {
            width: 256,
            height: 256,
            skin: [200, 160, 135],
            hair: [40, 30, 25],
            ground: [90, 110, 130],
            centre: [0.5, 0.52],
            radius: 0.30,
            roll: 0.0,
        }
    }
}

/// A painted portrait and where everything in it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Portrait {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Interleaved encoded sRGB.
    pub rgb: Vec<u8>,
    /// Face centre, pixels.
    pub centre: [f32; 2],
    /// Image-left and image-right eye centres, pixels.
    pub eyes: [[f32; 2]; 2],
    /// Mouth centre, pixels.
    pub mouth: [f32; 2],
    /// Where the face (oval, features included) is.
    pub face: Plane,
    /// Where the visible teeth are.
    pub teeth: Plane,
    /// Where the lips are, teeth excluded.
    pub lips: Plane,
    /// Where the eye openings are.
    pub eye_openings: Plane,
    /// Where the hair is.
    pub hair: Plane,
}

impl Portrait {
    /// The pixels as interleaved linear sRGB, for a renderer test.
    #[must_use]
    pub fn linear_srgb(&self) -> Vec<f32> {
        self.rgb
            .iter()
            .map(|b| aura_raw::colour::curve::srgb_decode(f32::from(*b) / 255.0))
            .collect()
    }
}

/// Paint one portrait.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn portrait(spec: &PortraitSpec) -> Portrait {
    let w = spec.width as f32;
    let h = spec.height as f32;
    let cx = spec.centre[0] * w;
    let cy = spec.centre[1] * h;
    let fw = spec.radius * w;
    let fh = fw * 4.0 / 3.0;
    let (rs, rc) = spec.roll.sin_cos();
    // Face-frame offsets to frame pixels.
    let place = |dx: f32, dy: f32| -> [f32; 2] { [cx + dx * rc - dy * rs, cy + dx * rs + dy * rc] };
    let skin = spec.skin.map(f32::from);
    let hair = spec.hair.map(f32::from);
    let ex = fw * 0.42;
    let ey = -fh * 0.12;
    let my = fh * 0.48;

    let mut img = vec![spec.ground.map(f32::from); (spec.width * spec.height) as usize];
    let mut face_plane = Plane::zeros(spec.width, spec.height);
    let mut teeth_plane = Plane::zeros(spec.width, spec.height);
    let mut lips_plane = Plane::zeros(spec.width, spec.height);
    let mut eye_plane = Plane::zeros(spec.width, spec.height);
    let mut hair_plane = Plane::zeros(spec.width, spec.height);

    for py in 0..spec.height {
        for px in 0..spec.width {
            let x = px as f32 + 0.5;
            let y = py as f32 + 0.5;
            // Into the face's own frame.
            let gx = x - cx;
            let gy = y - cy;
            let u = gx * rc + gy * rs;
            let v = -gx * rs + gy * rc;
            let ell = |ox: f32, oy: f32, rx: f32, ry: f32, soft: f32| -> f32 {
                let d = (((u - ox) / rx).powi(2) + ((v - oy) / ry).powi(2)).sqrt();
                ((1.0 - d) * rx.min(ry) / soft + 0.5).clamp(0.0, 1.0)
            };
            let index = (py * spec.width + px) as usize;
            let Some(pixel) = img.get_mut(index) else {
                continue;
            };
            let mut p = *pixel;
            let paint = |p: &mut [f32; 3], m: f32, c: [f32; 3]| {
                for (slot, value) in p.iter_mut().zip(c) {
                    *slot = *slot * (1.0 - m) + value * m;
                }
            };
            let shade = |p: &mut [f32; 3], m: f32, k: f32| {
                for slot in p.iter_mut() {
                    *slot *= 1.0 - m * (1.0 - k);
                }
            };

            let cap = ell(0.0, -fh * 0.25, fw * 1.18, fh, 2.0);
            paint(&mut p, cap, hair);
            let mut hair_weight = cap;
            paint(&mut p, ell(0.0, fh * 1.05, fw * 0.45, fh * 0.5, 2.0), skin);
            let face = ell(0.0, 0.0, fw, fh, 2.0);
            paint(&mut p, face, skin);
            hair_weight *= 1.0 - face;
            let r = ((u / fw).powi(2) + (v / fh).powi(2)).sqrt().min(1.0);
            shade(&mut p, face * r * r, 0.7);
            let glow = face
                * 0.10
                * (-((u / (fw * 0.5)).powi(2) + ((v + fh * 0.35) / (fh * 0.25)).powi(2))).exp();
            for (slot, s) in p.iter_mut().zip(skin) {
                *slot += glow * s;
            }
            let fringe = ell(0.0, -fh * 0.95, fw * 0.95, fh * 0.22, 2.0);
            paint(&mut p, fringe, hair);
            hair_weight = hair_weight.max(fringe);
            let mut opening = 0.0_f32;
            for side in [-1.0_f32, 1.0] {
                let sx = side * ex;
                shade(
                    &mut p,
                    ell(sx, ey - fh * 0.02, fw * 0.30, fh * 0.13, 6.0),
                    0.55,
                );
                let brow = [
                    (skin[0] * 0.35).min(40.0),
                    (skin[1] * 0.35).min(30.0),
                    (skin[2] * 0.35).min(25.0),
                ];
                paint(
                    &mut p,
                    ell(sx, ey - fh * 0.22, fw * 0.26, fh * 0.045, 2.0),
                    brow,
                );
                let sclera = ell(sx, ey, fw * 0.16, fh * 0.05, 2.0);
                opening = opening.max(sclera);
                paint(&mut p, sclera, [215.0, 205.0, 195.0]);
                paint(
                    &mut p,
                    ell(sx, ey, fw * 0.065, fh * 0.05, 2.0),
                    [45.0, 30.0, 20.0],
                );
                paint(
                    &mut p,
                    ell(sx, ey, fw * 0.03, fh * 0.03, 2.0),
                    [8.0, 8.0, 8.0],
                );
                paint(
                    &mut p,
                    ell(sx, ey - fh * 0.045, fw * 0.17, fh * 0.014, 1.0),
                    [15.0, 12.0, 10.0],
                );
            }
            shade(&mut p, ell(0.0, fh * 0.20, fw * 0.14, fh * 0.06, 2.0), 0.65);
            let lip_colour = [(skin[0] * 1.05).min(255.0), skin[1] * 0.62, skin[2] * 0.65];
            let lips = ell(0.0, my, fw * 0.40, fh * 0.11, 2.0);
            paint(&mut p, lips, lip_colour);
            let teeth = ell(0.0, my, fw * 0.30, fh * 0.045, 2.0);
            paint(&mut p, teeth, [230.0, 222.0, 200.0]);
            paint(
                &mut p,
                ell(0.0, my + fh * 0.03, fw * 0.28, fh * 0.012, 1.0),
                [20.0, 10.0, 10.0],
            );

            *pixel = p.map(|c| c.clamp(0.0, 255.0));
            let (ix, iy) = (i64::from(px), i64::from(py));
            face_plane.set(ix, iy, face);
            teeth_plane.set(ix, iy, teeth);
            lips_plane.set(ix, iy, (lips - teeth).max(0.0));
            eye_plane.set(ix, iy, opening);
            hair_plane.set(ix, iy, hair_weight);
        }
    }

    let rgb = img
        .iter()
        .flat_map(|p| p.map(|c| c.round().clamp(0.0, 255.0) as u8))
        .collect();
    Portrait {
        width: spec.width,
        height: spec.height,
        rgb,
        centre: [cx, cy],
        eyes: [place(-ex, ey), place(ex, ey)],
        mouth: place(0.0, my),
        face: face_plane,
        teeth: teeth_plane,
        lips: lips_plane,
        eye_openings: eye_plane,
        hair: hair_plane,
    }
}

/// Intersection over union of two soft planes, thresholded at a half.
#[must_use]
pub fn iou(a: &Plane, b: &Plane) -> f32 {
    let mut inter = 0_usize;
    let mut union = 0_usize;
    for (x, y) in a.values.iter().zip(b.values.iter()) {
        let p = *x >= 0.5;
        let q = *y >= 0.5;
        if p && q {
            inter += 1;
        }
        if p || q {
            union += 1;
        }
    }
    if union == 0 {
        1.0
    } else {
        inter as f32 / union as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_painter_puts_the_features_where_it_says() {
        let p = portrait(&PortraitSpec::default());
        assert_eq!(p.rgb.len(), 256 * 256 * 3);
        let [l, r] = p.eyes;
        assert!(l[0] < p.centre[0] && r[0] > p.centre[0]);
        assert!(p.mouth[1] > p.centre[1]);
        assert!(p.teeth.at(p.mouth[0] as i64, p.mouth[1] as i64) > 0.9);
        assert!(p.eye_openings.at(l[0] as i64, l[1] as i64) > 0.9);
        assert!(p.face.coverage() > 0.15);
        assert!(p.hair.coverage() > 0.03);
    }

    #[test]
    fn a_tilted_portrait_tilts_its_landmarks() {
        let p = portrait(&PortraitSpec {
            roll: 0.3,
            ..PortraitSpec::default()
        });
        let [l, r] = p.eyes;
        let roll = (r[1] - l[1]).atan2(r[0] - l[0]);
        assert!((roll - 0.3).abs() < 1e-3);
    }
}
