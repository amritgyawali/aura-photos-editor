//! A painted portrait with a parse that agrees with it by construction, for the unit tests of
//! every ADR-0150 adjustment. One face, eyes 40 px apart, on a 240 x 240 frame whose analysis
//! grid is the frame itself, so a region's weight at a pixel is exactly what was painted there.
use aura_portrait::{FaceGeometry, FaceSource, Plane, PortraitMap, Region, RegionStat};

pub(crate) const W: usize = 240;
pub(crate) const H: usize = 240;
pub(crate) const LEFT_EYE: [f32; 2] = [100.0, 100.0];
pub(crate) const RIGHT_EYE: [f32; 2] = [140.0, 100.0];
pub(crate) const NOSE: [f32; 2] = [120.0, 128.0];
pub(crate) const MOUTH: [f32; 2] = [120.0, 150.0];

pub(crate) const BACKGROUND: [f32; 3] = [0.30, 0.34, 0.40];
pub(crate) const SKIN: [f32; 3] = [0.42, 0.28, 0.20];
pub(crate) const LIPS: [f32; 3] = [0.40, 0.12, 0.12];
pub(crate) const HAIR: [f32; 3] = [0.06, 0.04, 0.03];
pub(crate) const CLOTH: [f32; 3] = [0.55, 0.04, 0.05];
pub(crate) const BROW: [f32; 3] = [0.08, 0.06, 0.05];
pub(crate) const SCLERA: [f32; 3] = [0.70, 0.68, 0.66];
pub(crate) const IRIS: [f32; 3] = [0.08, 0.05, 0.03];
pub(crate) const TEETH: [f32; 3] = [0.62, 0.58, 0.48];

fn inside(x: f32, y: f32, c: [f32; 2], r: [f32; 2]) -> bool {
    let u = (x - c[0]) / r[0];
    let v = (y - c[1]) / r[1];
    u * u + v * v <= 1.0
}

/// Which painted part a pixel centre is in, outermost first.
pub(crate) fn part(x: usize, y: usize) -> Region {
    let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
    let face = inside(x, y, [120.0, 120.0], [50.0, 66.0]);
    for eye in [LEFT_EYE, RIGHT_EYE] {
        if inside(x, y, eye, [4.5, 4.5]) {
            return Region::Iris;
        }
        if inside(x, y, eye, [11.0, 5.0]) {
            return Region::Sclera;
        }
        if inside(x, y, [eye[0], eye[1] - 13.0], [13.0, 3.5]) {
            return Region::Eyebrows;
        }
    }
    if inside(x, y, MOUTH, [12.0, 2.0]) {
        return Region::Teeth;
    }
    if inside(x, y, MOUTH, [17.0, 6.5]) {
        return Region::Lips;
    }
    if face {
        return Region::Face;
    }
    if y < 120.0 && inside(x, y, [120.0, 95.0], [62.0, 78.0]) {
        return Region::Hair;
    }
    if (102.0..=138.0).contains(&x) && (180.0..205.0).contains(&y) {
        return Region::Neck;
    }
    if y >= 205.0 && (50.0..=190.0).contains(&x) {
        return Region::Clothing;
    }
    Region::Background
}

pub(crate) fn colour_of(region: Region) -> [f32; 3] {
    match region {
        Region::Iris => IRIS,
        Region::Sclera => SCLERA,
        Region::Eyebrows => BROW,
        Region::Teeth => TEETH,
        Region::Lips => LIPS,
        Region::Face | Region::Neck => SKIN,
        Region::Hair => HAIR,
        Region::Clothing => CLOTH,
        _ => BACKGROUND,
    }
}

/// The painted frame. The top rim of each eye opening is lashes: part of the eye as the parse
/// reports it, and as dark as the brows.
pub(crate) fn frame() -> Vec<f32> {
    (0..H)
        .flat_map(|y| {
            (0..W).map(move |x| {
                let p = part(x, y);
                let lash = matches!(p, Region::Sclera | Region::Iris)
                    && (y as f32 + 0.5) < LEFT_EYE[1] - 3.5;
                if lash {
                    BROW
                } else {
                    colour_of(p)
                }
            })
        })
        .flatten()
        .collect()
}

/// True when the painted part at a pixel belongs to `region` as the parse reports it.
pub(crate) fn belongs(region: Region, x: usize, y: usize) -> bool {
    let p = part(x, y);
    match region {
        Region::Skin => matches!(p, Region::Face | Region::Neck),
        Region::Face => matches!(
            p,
            Region::Face
                | Region::Iris
                | Region::Sclera
                | Region::Eyebrows
                | Region::Lips
                | Region::Teeth
        ),
        Region::Eyes => matches!(p, Region::Iris | Region::Sclera),
        Region::Mouth => matches!(p, Region::Lips | Region::Teeth),
        Region::Nose => {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            p == Region::Face && inside(fx, fy, [120.0, 118.0], [9.0, 16.0])
        }
        Region::UnderEyes => {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            p == Region::Face
                && (inside(fx, fy, [LEFT_EYE[0], 110.0], [11.0, 4.0])
                    || inside(fx, fy, [RIGHT_EYE[0], 110.0], [11.0, 4.0]))
        }
        Region::Body => p != Region::Background,
        Region::BodySkin | Region::FacialHair | Region::Sky => false,
        other => p == other,
    }
}

fn plane(region: Region) -> Plane {
    let values = (0..H)
        .flat_map(|y| (0..W).map(move |x| if belongs(region, x, y) { 1.0 } else { 0.0 }))
        .collect();
    Plane::from_values(W as u32, H as u32, values)
}

/// The one face.
pub(crate) fn face() -> FaceGeometry {
    FaceGeometry {
        center: [120.0, 120.0],
        size: 110.0,
        roll: 0.0,
        left_eye: LEFT_EYE,
        right_eye: RIGHT_EYE,
        nose: NOSE,
        mouth: MOUTH,
        mouth_width: 34.0,
        eyes_measured: 2,
        eye_hits: 2,
        mouth_measured: true,
        confidence: 0.95,
        neighbours: 12,
        source: FaceSource::Frontal,
        skin_fraction: 0.9,
    }
}

/// The parse of [`frame`].
pub(crate) fn map() -> PortraitMap {
    let planes: Vec<(Region, Plane)> = aura_portrait::ALL_REGIONS
        .iter()
        .map(|r| (*r, plane(*r)))
        .collect();
    let stats = planes
        .iter()
        .map(|(r, p)| RegionStat {
            region: *r,
            coverage: p.values.iter().sum::<f32>() / (W * H) as f32,
            confidence: 0.9,
        })
        .collect();
    PortraitMap {
        width: W as u32,
        height: H as u32,
        faces: vec![face()],
        planes,
        stats,
        colourful: true,
    }
}

/// The pixel at `(x, y)`.
pub(crate) fn px(rgb: &[f32], x: usize, y: usize) -> [f32; 3] {
    let i = (y * W + x) * 3;
    [rgb[i], rgb[i + 1], rgb[i + 2]]
}

/// Pixels of `region` (or outside it, with `outside`) that moved by more than `tolerance`.
pub(crate) fn moved(
    before: &[f32],
    after: &[f32],
    region: Region,
    outside: bool,
    tolerance: f32,
) -> usize {
    (0..H)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .filter(|&(x, y)| belongs(region, x, y) != outside)
        .filter(|&(x, y)| {
            let (a, b) = (px(before, x, y), px(after, x, y));
            (0..3).any(|c| (a[c] - b[c]).abs() > tolerance)
        })
        .count()
}
