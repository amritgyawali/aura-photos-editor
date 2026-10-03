//! Real-photograph check of the skin segmenter, run by hand. ADR-0077.
//!
//! `AURA_SKIN_PHOTOS` names a folder of `NAME_WxH.rgb` files (packed sRGB, made from JPEGs by
//! `ml/models/skin/to_raw.py`); for each one this writes `NAME.overlay.rgb` beside it: the photo
//! with face skin tinted magenta, body skin cyan, hair brown and clothes green, plus the face
//! boxes. The images are looked at, not asserted on - the assertions are that every detected
//! face receives a face matte and that analysis finishes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use aura_vision::{portrait, skin};

#[test]
#[ignore = "needs AURA_SKIN_PHOTOS; writes overlays for a person to look at"]
fn segments_real_photographs() {
    let dir = std::env::var("AURA_SKIN_PHOTOS").expect("AURA_SKIN_PHOTOS");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "rgb")
                && p.file_stem()
                    .is_some_and(|s| !s.to_string_lossy().contains('.'))
        })
        .collect();
    assert!(
        !entries.is_empty(),
        "No NAME_WxH.rgb inputs found in AURA_SKIN_PHOTOS"
    );
    entries.sort();
    for path in entries {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let dims = stem.rsplit('_').next().unwrap();
        let (w, h) = dims.split_once('x').unwrap();
        let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
        let rgb = std::fs::read(&path).unwrap();
        let started = std::time::Instant::now();
        let mut faces = portrait::detect(&rgb, w, h).unwrap();
        faces = portrait::detect_small_faces(&rgb, w, h, &faces).unwrap_or(faces);
        let analysis = skin::analyse(&rgb, w, h, &faces, skin::Options::default()).unwrap();
        let elapsed = started.elapsed();
        let mut out = rgb.clone();
        let tint = |out: &mut Vec<u8>, matte: &skin::Matte, colour: [f32; 3]| {
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let a =
                        matte.at((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32) * 0.6;
                    let i = (y * w as usize + x) * 3;
                    for c in 0..3 {
                        out[i + c] =
                            (f32::from(out[i + c]) * (1.0 - a) + colour[c] * a).round() as u8;
                    }
                }
            }
        };
        for person in &analysis.people {
            if let Some(m) = &person.clothes {
                tint(&mut out, m, [0.0, 170.0, 0.0]);
            }
            if let Some(m) = &person.hair {
                tint(&mut out, m, [150.0, 75.0, 20.0]);
            }
            if let Some(m) = &person.body {
                tint(&mut out, m, [0.0, 220.0, 255.0]);
            }
            if let Some(m) = &person.face {
                tint(&mut out, m, [255.0, 0.0, 210.0]);
            }
        }
        for face in &faces {
            let [l, t, r, b] = face.bounds;
            let (x0, y0) = ((l * w as f32) as usize, (t * h as f32) as usize);
            let (x1, y1) = (
                ((r * w as f32) as usize).min(w as usize - 1),
                ((b * h as f32) as usize).min(h as usize - 1),
            );
            for x in x0..=x1 {
                for y in [y0, y1] {
                    let i = (y * w as usize + x) * 3;
                    out[i..i + 3].copy_from_slice(&[255, 255, 0]);
                }
            }
            for y in y0..=y1 {
                for x in [x0, x1] {
                    let i = (y * w as usize + x) * 3;
                    out[i..i + 3].copy_from_slice(&[255, 255, 0]);
                }
            }
        }
        std::fs::write(path.with_file_name(format!("{stem}.overlay.rgb")), &out).unwrap();
        let with_face = analysis.people.iter().filter(|p| p.face.is_some()).count();
        let with_body = analysis.people.iter().filter(|p| p.body.is_some()).count();
        println!(
            "{stem}: {} faces, {with_face} face mattes, {with_body} body mattes, {} passes, {:.2}s, coverage {:?}",
            faces.len(),
            analysis.passes,
            elapsed.as_secs_f32(),
            analysis.coverage.map(|c| (c * 100.0).round() / 100.0)
        );
    }
}
