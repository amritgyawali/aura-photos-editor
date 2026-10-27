//! Known-answer check of lens-reflection removal on a real photograph, run by hand.
//!
//! `AURA_GLARE_PHOTO` names a `NAME_WxH.rgb` file (packed 8-bit sRGB) and `AURA_GLARE_REGION`
//! an ellipse `cx,cy,rx,ry` in frame fractions over an eye. A reflection with a known shape and
//! colour - a soft sheet with a diagonal streak, neutral, green-coated and purple-coated - is
//! added to the real pixels, the Glare tool removes it, and the error against the clean
//! photograph is printed before and after. The reflection is synthetic; the eye, skin, lashes
//! and frame texture under it are the photograph's own.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::print_stdout
)]

use aura_recipe::retouch_tools::{Edit, Tool};

fn working(rgb: &[u8]) -> Vec<f32> {
    rgb.chunks_exact(3)
        .flat_map(|p| {
            aura_render::output::display_srgb_to_working(
                [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0),
            )
        })
        .collect()
}

fn edit(region: [f32; 4]) -> Edit {
    let mut e: Edit = serde_json::from_value(serde_json::json!({
        "id": "glare", "tool": "glare", "enabled": true, "region": region, "source": null,
        "amount": 1.0, "feather": 0.3, "radius": 0.003, "texture": 1.0, "tone": 0.5,
        "warmth": 0.0, "tint": 0.0
    }))
    .unwrap();
    e.tool = Tool::Glare;
    e
}

/// The glare softening shipped before this tool removed veils (`glare.2`), for comparison:
/// a highlight knee at 1.25 times the tone of four samples just outside the ellipse.
fn previous(rgb: &mut [f32], w: usize, h: usize, region: [f32; 4], amount: f32, feather: f32) {
    let luma = |p: [f32; 3]| 0.2627 * p[0] + 0.678 * p[1] + 0.0593 * p[2];
    let at = |x: f32, y: f32, rgb: &[f32]| {
        let x = ((x * w as f32) as usize).min(w - 1);
        let y = ((y * h as f32) as usize).min(h - 1);
        let i = (y * w + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    };
    let mut ring = [0.0_f32; 3];
    for (dx, dy) in [(1.15, 0.0), (-1.15, 0.0), (0.0, 1.15), (0.0, -1.15)] {
        let v = at(region[0] + dx * region[2], region[1] + dy * region[3], rgb);
        for c in 0..3 {
            ring[c] += v[c] / 4.0;
        }
    }
    let source = rgb.to_vec();
    for y in 0..h {
        for x in 0..w {
            let dx = ((x as f32 + 0.5) / w as f32 - region[0]) / region[2];
            let dy = ((y as f32 + 0.5) / h as f32 - region[1]) / region[3];
            let d = dx.hypot(dy);
            if d >= 1.0 {
                continue;
            }
            let t = ((1.0 - d) / feather).clamp(0.0, 1.0);
            let a = t * t * (3.0 - 2.0 * t) * amount;
            let i = (y * w + x) * 3;
            let pixel = [source[i], source[i + 1], source[i + 2]];
            let light = luma(pixel).max(0.0);
            let knee = (luma(ring).max(0.0) * 1.25).max(0.08);
            let excess = (light - knee).max(0.0);
            let compressed = excess / (1.0 + 3.0 * excess / (knee + 0.1));
            let darkest = pixel.into_iter().fold(f32::INFINITY, f32::min).max(0.0);
            let veil = (excess - compressed).min(light * 0.35).min(darkest * 0.75);
            for c in 0..3 {
                rgb[i + c] = pixel[c] + a * (-veil);
            }
        }
    }
}

#[test]
#[ignore = "needs AURA_GLARE_PHOTO and AURA_GLARE_REGION"]
fn a_known_reflection_is_removed_from_a_real_eye() {
    let path = std::env::var("AURA_GLARE_PHOTO").expect("AURA_GLARE_PHOTO");
    let region: Vec<f32> = std::env::var("AURA_GLARE_REGION")
        .expect("AURA_GLARE_REGION")
        .split(',')
        .map(|v| v.trim().parse().expect("number"))
        .collect();
    let region = [region[0], region[1], region[2], region[3]];
    let stem = std::path::Path::new(&path)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let size = stem.rsplit('_').next().unwrap();
    let (w, h) = size.split_once('x').unwrap();
    let (w, h): (usize, usize) = (w.parse().unwrap(), h.parse().unwrap());
    let clean = working(&std::fs::read(&path).unwrap());
    assert_eq!(clean.len(), w * h * 3);
    for (name, tint) in [
        ("neutral", [1.0, 1.0, 1.0]),
        ("green coating", [0.75, 1.0, 0.7]),
        ("purple coating", [1.0, 0.7, 1.0]),
    ] {
        let mut veiled = clean.clone();
        let mut veil = vec![0.0_f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let dx = ((x as f32 + 0.5) / w as f32 - region[0]) / (region[2] * 0.75);
                let dy = ((y as f32 + 0.5) / h as f32 - region[1]) / (region[3] * 0.75);
                let d = dx.hypot(dy);
                if d >= 1.0 {
                    continue;
                }
                let t = ((1.0 - d) / 0.4).clamp(0.0, 1.0);
                let sheet = t * t * (3.0 - 2.0 * t);
                let streak = (-((dx - dy) * 3.0).powi(2)).exp();
                let v = 0.30 * sheet * (0.55 + 0.45 * streak);
                veil[y * w + x] = v;
                for c in 0..3 {
                    veiled[(y * w + x) * 3 + c] += v * tint[c];
                }
            }
        }
        let mut restored = veiled.clone();
        aura_render::retouch_tools::apply(&mut restored, w, h, &[edit(region)]);
        let error = |pixels: &[f32]| {
            let (mut sum, mut n) = (0.0_f64, 0usize);
            for (i, v) in veil.iter().enumerate() {
                if *v > 0.01 {
                    for c in 0..3 {
                        let e = f64::from(pixels[i * 3 + c] - clean[i * 3 + c]);
                        sum += e * e;
                    }
                    n += 3;
                }
            }
            (sum / n.max(1) as f64).sqrt()
        };
        let (before, after) = (error(&veiled), error(&restored));
        let mut old = veiled.clone();
        previous(&mut old, w, h, region, 1.0, 0.3);
        println!(
            "  previous glare softening at full strength: {before:.4} -> {:.4} ({:.0}% removed)",
            error(&old),
            100.0 * (1.0 - error(&old) / before)
        );
        if let Ok(dir) = std::env::var("AURA_GLARE_DUMP") {
            // Share of the known veil removed, per pixel (grey 128 = none, 255 = all), plus the
            // clean, veiled and restored darkest channel, for a person to look at.
            let mut map = Vec::with_capacity(w * h * 4);
            for i in 0..w * h {
                let v = veil[i];
                let share = if v > 0.01 {
                    ((veiled[i * 3 + 1] - restored[i * 3 + 1]) / (v * tint[1])).clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                map.push((128.0 + 127.0 * share) as u8);
                for img in [&clean, &veiled, &restored] {
                    let d = img[i * 3].min(img[i * 3 + 1]).min(img[i * 3 + 2]);
                    map.push((d.clamp(0.0, 1.0).sqrt() * 255.0) as u8);
                }
            }
            std::fs::write(format!("{dir}/glare-{}.bin", name.replace(' ', "-")), map).unwrap();
        }
        // Pixels outside the reflection's own extent but inside the selection: did it harm them?
        let (mut harm, mut m) = (0.0_f64, 0usize);
        for y in 0..h {
            for x in 0..w {
                let dx = ((x as f32 + 0.5) / w as f32 - region[0]) / region[2];
                let dy = ((y as f32 + 0.5) / h as f32 - region[1]) / region[3];
                let i = y * w + x;
                if dx.hypot(dy) < 1.0 && veil[i] == 0.0 {
                    for c in 0..3 {
                        let e = f64::from(restored[i * 3 + c] - clean[i * 3 + c]);
                        harm += e * e;
                    }
                    m += 3;
                }
            }
        }
        let harm = (harm / m.max(1) as f64).sqrt();
        // Signed residual of the darkest channel, split by what is under the reflection.
        let split = |dark: bool| {
            let (mut sum, mut n) = (0.0_f64, 0usize);
            for (i, v) in veil.iter().enumerate() {
                let c = clean[i * 3].min(clean[i * 3 + 1]).min(clean[i * 3 + 2]);
                if *v > 0.05 && (c < 0.12) == dark {
                    let r = restored[i * 3]
                        .min(restored[i * 3 + 1])
                        .min(restored[i * 3 + 2]);
                    sum += f64::from(r - c) / f64::from(*v);
                    n += 1;
                }
            }
            (sum / n.max(1) as f64, n)
        };
        let (eye, eye_n) = split(true);
        let (skin, skin_n) = split(false);
        println!(
            "  veil left, as a share of the veil: dark content {:+.2} ({eye_n} px), light content {:+.2} ({skin_n} px)",
            eye, skin
        );
        println!(
            "{name}: reflection error {before:.4} -> {after:.4} ({:.0}% removed); unveiled selection error {harm:.4}",
            100.0 * (1.0 - after / before)
        );
        // Measured on four eyes of two people (2026-10-25): 11 % to 85 % removed, against 3 % to
        // 41 % for the previous softening. The hard case is an eye socket much darker than the
        // brow and cheek around the lens: there the reflection does not rise above its
        // surroundings and only part of it is found.
        assert!(after < before * 0.95, "{name}: {before} -> {after}");
        assert!(
            after < error(&old),
            "{name}: no better than the previous softening"
        );
    }
}
