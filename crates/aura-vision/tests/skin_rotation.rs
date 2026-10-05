#![allow(clippy::disallowed_methods)]
//! Real-photo rotation regression. No ground-truth accuracy claim is made here.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use aura_vision::{portrait, skin};

fn point([x, y]: [f32; 2]) -> [f32; 2] {
    [1.0 - y, x]
}

#[test]
#[ignore = "requires AURA_SKIN_RGB=path to packed RGB and AURA_SKIN_SIZE=WxH"]
fn quarter_turns_preserve_face_and_body_selection() {
    let path = std::env::var("AURA_SKIN_RGB").unwrap();
    let size = std::env::var("AURA_SKIN_SIZE").unwrap();
    let (w, h) = size.split_once('x').unwrap();
    let (mut w, mut h): (usize, usize) = (w.parse().unwrap(), h.parse().unwrap());
    let mut rgb = std::fs::read(path).unwrap();
    let mut faces = portrait::detect(&rgb, w as u32, h as u32).unwrap();
    assert!(!faces.is_empty(), "fixture must have a detected face");
    let base = skin::analyse(&rgb, w as u32, h as u32, &faces, skin::Options::default()).unwrap();
    let mut failures = Vec::new();
    for turns in 1..=3 {
        let mut rotated = vec![0; rgb.len()];
        for y in 0..h {
            for x in 0..w {
                let from = (y * w + x) * 3;
                let to = (x * h + h - 1 - y) * 3;
                rotated[to..to + 3].copy_from_slice(&rgb[from..from + 3]);
            }
        }
        rgb = rotated;
        std::mem::swap(&mut w, &mut h);
        for face in &mut faces {
            face.landmarks = face.landmarks.map(point);
            let [l, t, r, b] = face.bounds;
            face.bounds = [1.0 - b, l, 1.0 - t, r];
        }
        let result =
            skin::analyse(&rgb, w as u32, h as u32, &faces, skin::Options::default()).unwrap();
        for (index, (a, b)) in base.people.iter().zip(&result.people).enumerate() {
            for (label, expected, actual) in
                [("face", &a.face, &b.face), ("body", &a.body, &b.body)]
            {
                let Some(expected) = expected else { continue };
                let mut intersection = 0;
                let mut total = 0;
                for y in 0..128 {
                    for x in 0..128 {
                        let original = [(x as f32 + 0.5) / 128.0, (y as f32 + 0.5) / 128.0];
                        let mut p = original;
                        for _ in 0..turns {
                            p = point(p);
                        }
                        let selected = expected.at(original[0], original[1]) >= 0.5;
                        let rotated_selected =
                            actual.as_ref().is_some_and(|m| m.at(p[0], p[1]) >= 0.5);
                        total += usize::from(selected) + usize::from(rotated_selected);
                        intersection += usize::from(selected && rotated_selected);
                    }
                }
                let dice = if total == 0 {
                    1.0
                } else {
                    2.0 * intersection as f32 / total as f32
                };
                println!("turns={turns} person={index} {label} rotation Dice={dice:.5}");
                if dice < 0.97 {
                    failures.push((turns, index, label, dice));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Rotation changed skin selection: {failures:?}"
    );
}
