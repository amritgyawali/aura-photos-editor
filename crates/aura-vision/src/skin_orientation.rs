//! Quarter-turn normalization shared by the skin model and its saved selections.
use super::{Analysis, Image, Matte, PortraitFace};

/// Use the largest confident face's eye-to-mouth direction in pixel space.
/// People with different orientations in one frame still share this model view.
pub(super) fn upright_turns(faces: &[PortraitFace], w: usize, h: usize) -> u8 {
    let candidate = faces
        .iter()
        .filter_map(|f| {
            let [a, b, _, c, d] = f.landmarks;
            let dx = (c[0] + d[0] - a[0] - b[0]) * w as f32;
            let dy = (c[1] + d[1] - a[1] - b[1]) * h as f32;
            if dx.hypot(dy) < 1.0 {
                return None;
            }
            let turns = if dx.abs() > dy.abs() {
                if dx > 0.0 {
                    1
                } else {
                    3
                }
            } else if dy < 0.0 {
                2
            } else {
                0
            };
            let [l, t, r, b] = f.bounds;
            Some(((r - l) * (b - t) * f.confidence.clamp(0.0, 1.0), turns))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0));
    candidate.map_or(0, |(_, turns)| turns)
}

fn point(mut p: [f32; 2], turns: u8) -> [f32; 2] {
    for _ in 0..turns {
        p = [1.0 - p[1], p[0]];
    }
    p
}

fn bounds(mut b: [f32; 4], turns: u8) -> [f32; 4] {
    for _ in 0..turns {
        b = [1.0 - b[3], b[0], 1.0 - b[1], b[2]];
    }
    b
}

pub(super) fn face(f: &PortraitFace, turns: u8) -> PortraitFace {
    PortraitFace {
        bounds: bounds(f.bounds, turns),
        landmarks: f.landmarks.map(|p| point(p, turns)),
        confidence: f.confidence,
    }
}

// Source and destination grids have equal validated lengths, and every source
// pixel maps bijectively to one destination pixel, including non-square images.
fn rotate(
    pixels: &[u8],
    w: usize,
    h: usize,
    channels: usize,
    turns: u8,
) -> (Vec<u8>, usize, usize) {
    let (ow, oh) = if turns % 2 == 1 { (h, w) } else { (w, h) };
    let mut out = vec![0; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let (ox, oy) = match turns {
                1 => (h - 1 - y, x),
                2 => (w - 1 - x, h - 1 - y),
                3 => (y, w - 1 - x),
                _ => (x, y),
            };
            let src = (y * w + x) * channels;
            let dst = (oy * ow + ox) * channels;
            out[dst..dst + channels].copy_from_slice(&pixels[src..src + channels]);
        }
    }
    (out, ow, oh)
}

pub(super) fn image(image: &Image, turns: u8) -> Image {
    let (rgb, w, h) = rotate(&image.rgb, image.w, image.h, 3, turns);
    Image { w, h, rgb }
}

fn matte(m: &mut Matte, turns: u8) {
    let (alpha, width, height) = rotate(&m.alpha, m.width, m.height, 1, turns);
    m.alpha = alpha;
    m.width = width;
    m.height = height;
    m.bounds = bounds(m.bounds, turns);
}

pub(super) fn restore(result: &mut Analysis, turns: u8) {
    for person in &mut result.people {
        for m in [
            &mut person.face,
            &mut person.body,
            &mut person.hair,
            &mut person.clothes,
        ]
        .into_iter()
        .flatten()
        {
            matte(m, turns);
        }
        for colour in [&mut person.face_colour, &mut person.body_colour]
            .into_iter()
            .flatten()
        {
            colour.point = point(colour.point, turns);
        }
    }
    if let Some(m) = &mut result.background {
        matte(m, turns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_square_pixels_and_mattes_round_trip_in_every_orientation() {
        let pixels: Vec<u8> = (0..18).collect();
        for turns in 1..=3 {
            let (rotated, w, h) = rotate(&pixels, 3, 2, 3, turns);
            let (restored, w, h) = rotate(&rotated, w, h, 3, 4 - turns);
            assert_eq!((w, h), (3, 2));
            assert_eq!(restored, pixels);
            let original = Matte {
                bounds: [0.125, 0.25, 0.5, 0.75],
                width: 3,
                height: 2,
                alpha: vec![0, 50, 100, 150, 200, 255],
            };
            let mut rotated = original.clone();
            matte(&mut rotated, turns);
            for y in 0..2 {
                for x in 0..3 {
                    let p = [
                        0.125 + (x as f32 + 0.5) * 0.375 / 3.0,
                        0.25 + (y as f32 + 0.5) * 0.5 / 2.0,
                    ];
                    let q = point(p, turns);
                    assert_eq!(original.at(p[0], p[1]), rotated.at(q[0], q[1]));
                }
            }
            matte(&mut rotated, 4 - turns);
            assert_eq!(rotated, original);
        }
    }

    #[test]
    fn eye_to_mouth_direction_handles_quarter_turns_without_assuming_eye_order() {
        let base = PortraitFace {
            bounds: [0.1, 0.1, 0.8, 0.9],
            landmarks: [
                [0.3, 0.3],
                [0.6, 0.3],
                [0.45, 0.45],
                [0.35, 0.6],
                [0.55, 0.6],
            ],
            confidence: 0.99,
        };
        for turns in 0..4 {
            let f = face(&base, turns);
            let (w, h) = if turns % 2 == 0 {
                (400, 600)
            } else {
                (600, 400)
            };
            assert_eq!(upright_turns(&[f], w, h), (4 - turns) % 4);
        }
        assert_eq!(upright_turns(&[], 400, 600), 0);
    }
}
