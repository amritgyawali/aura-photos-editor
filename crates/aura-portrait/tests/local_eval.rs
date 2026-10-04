//! Evaluation on photographs that are not in this repository.
//!
//! There is no consented face data in this repository and there will not be, so the
//! evaluation against real photographs runs on a directory the person running it supplies:
//!
//! ```text
//! AURA_PORTRAIT_EVAL_DIR=/path/to/pgm-and-ppm cargo test -p aura-portrait --release \
//!     --test local_eval -- --ignored --nocapture
//! ```
//!
//! Every test here is `#[ignore]`d and prints rather than asserts: it is an instrument, not a
//! gate. `docs/portrait-retouch.md` records what it measured on the day the crate shipped.

use std::path::PathBuf;

use aura_portrait::cascade::{Cascade, GreyImage, ScanParams};

fn dir() -> Option<PathBuf> {
    std::env::var_os("AURA_PORTRAIT_EVAL_DIR").map(PathBuf::from)
}

/// Read a binary PGM (`P5`) or PPM (`P6`) with 8-bit samples.
fn read_pnm(path: &std::path::Path) -> Option<(u32, u32, u32, Vec<u8>)> {
    let bytes = std::fs::read(path).ok()?;
    let mut fields = Vec::new();
    let mut at = 0;
    while fields.len() < 4 {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) == Some(&b'#') {
            while bytes.get(at).is_some_and(|b| *b != b'\n') {
                at += 1;
            }
            continue;
        }
        let start = at;
        while bytes.get(at).is_some_and(|b| !b.is_ascii_whitespace()) {
            at += 1;
        }
        fields.push(String::from_utf8_lossy(bytes.get(start..at)?).to_string());
    }
    at += 1;
    let channels = match fields.first()?.as_str() {
        "P5" => 1,
        "P6" => 3,
        _ => return None,
    };
    let width: u32 = fields.get(1)?.parse().ok()?;
    let height: u32 = fields.get(2)?.parse().ok()?;
    Some((width, height, channels, bytes.get(at..)?.to_vec()))
}

fn files(extension: &str) -> Vec<PathBuf> {
    let Some(dir) = dir() else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == extension))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

#[test]
#[ignore = "needs AURA_PORTRAIT_EVAL_DIR"]
fn frontal_cascade_on_grey_frames() {
    let Some(cascade) = Cascade::frontal() else {
        panic!("frontal cascade did not parse");
    };
    for path in files("pgm") {
        let Some((width, height, 1, pixels)) = read_pnm(&path) else {
            continue;
        };
        let Some(image) = GreyImage::new(width, height, pixels) else {
            continue;
        };
        let mut found = cascade.detect(&image, ScanParams::default());
        found.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let boxes: Vec<String> = found
            .iter()
            .map(|d| format!("{},{},{},{},{}", d.x, d.y, d.w, d.h, d.neighbours))
            .collect();
        println!("{name} {}", boxes.join(" "));
    }
}

fn out_dir() -> Option<PathBuf> {
    std::env::var_os("AURA_PORTRAIT_EVAL_OUT").map(PathBuf::from)
}

fn write_ppm(path: &std::path::Path, width: u32, height: u32, rgb: &[u8]) {
    let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
    bytes.extend_from_slice(rgb);
    let _ = std::fs::write(path, bytes);
}

fn dot(rgb: &mut [u8], width: u32, height: u32, x: f32, y: f32, colour: [u8; 3], radius: i64) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let px = x as i64 + dx;
            let py = y as i64 + dy;
            if px < 0 || py < 0 || px >= i64::from(width) || py >= i64::from(height) {
                continue;
            }
            let at = (py as usize * width as usize + px as usize) * 3;
            if let Some(slot) = rgb.get_mut(at..at + 3) {
                slot.copy_from_slice(&colour);
            }
        }
    }
}

fn line(rgb: &mut [u8], width: u32, height: u32, a: [f32; 2], b: [f32; 2], colour: [u8; 3]) {
    let steps = ((b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil() as usize).max(1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        dot(
            rgb,
            width,
            height,
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            colour,
            0,
        );
    }
}

#[test]
#[ignore = "needs AURA_PORTRAIT_EVAL_DIR"]
fn faces_on_colour_frames() {
    let mut total = 0;
    for path in files("ppm") {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with("full_") {
            continue;
        }
        let Some((width, height, 3, mut rgb)) = read_pnm(&path) else {
            continue;
        };
        let Some(canvas) = aura_portrait::Canvas::from_srgb8(&rgb, width, height, 1024) else {
            continue;
        };
        let started = std::time::Instant::now();
        let faces = aura_portrait::face::detect(&canvas, &[]);
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        total += faces.len();
        let k = width as f32 / canvas.width as f32;
        let summary: Vec<String> = faces
            .iter()
            .map(|f| {
                format!(
                    "{}@{:.0},{:.0} s{:.0} r{:.0} n{} e{} m{} sk{:.2} c{:.2}",
                    f.source.as_str(),
                    f.center[0] * k,
                    f.center[1] * k,
                    f.size * k,
                    f.roll.to_degrees(),
                    f.neighbours,
                    f.eyes_measured,
                    u8::from(f.mouth_measured),
                    f.skin_fraction,
                    f.confidence
                )
            })
            .collect();
        println!("{name} ({ms:.0} ms) {}", summary.join(" | "));
        if let Some(out) = out_dir() {
            for f in &faces {
                let colour = match f.source {
                    aura_portrait::FaceSource::Frontal => [0, 255, 0],
                    aura_portrait::FaceSource::Tilted => [255, 255, 0],
                    aura_portrait::FaceSource::Profile => [0, 160, 255],
                    aura_portrait::FaceSource::Equalised => [255, 0, 255],
                    aura_portrait::FaceSource::Hint => [255, 255, 255],
                };
                let corners = [
                    f.at(0.0, 0.0),
                    f.at(1.0, 0.0),
                    f.at(1.0, 1.0),
                    f.at(0.0, 1.0),
                ];
                for i in 0..4 {
                    let a = corners[i];
                    let b = corners[(i + 1) % 4];
                    line(
                        &mut rgb,
                        width,
                        height,
                        [a[0] * k, a[1] * k],
                        [b[0] * k, b[1] * k],
                        colour,
                    );
                }
                for e in [f.left_eye, f.right_eye] {
                    dot(&mut rgb, width, height, e[0] * k, e[1] * k, [255, 0, 0], 2);
                }
                dot(
                    &mut rgb,
                    width,
                    height,
                    f.nose[0] * k,
                    f.nose[1] * k,
                    [0, 0, 255],
                    1,
                );
                let (s, c) = f.roll.sin_cos();
                let half = f.mouth_width * 0.5;
                line(
                    &mut rgb,
                    width,
                    height,
                    [(f.mouth[0] - c * half) * k, (f.mouth[1] - s * half) * k],
                    [(f.mouth[0] + c * half) * k, (f.mouth[1] + s * half) * k],
                    [255, 120, 0],
                );
            }
            write_ppm(&out.join(format!("{name}_faces.ppm")), width, height, &rgb);
        }
    }
    println!("total faces {total}");
}

#[test]
#[ignore = "needs AURA_PORTRAIT_EVAL_DIR"]
fn timing_breakdown() {
    let Some(dir) = dir() else { return };
    let Some((width, height, 1, pixels)) = read_pnm(&dir.join("2008_004176.pgm")) else {
        return;
    };
    let image = GreyImage::new(width, height, pixels).unwrap_or_else(|| panic!("image"));
    let frontal = Cascade::frontal().unwrap_or_else(|| panic!("cascade"));
    for label in ["frontal x1", "frontal x2"] {
        let t = std::time::Instant::now();
        let n = frontal.detect(&image, ScanParams::default()).len();
        println!(
            "{label}: {n} in {:.1} ms",
            t.elapsed().as_secs_f32() * 1000.0
        );
    }
    let t = std::time::Instant::now();
    for _ in 0..25 {
        let _ = image.resize(width * 9 / 10, height * 9 / 10);
    }
    println!("25 resizes: {:.1} ms", t.elapsed().as_secs_f32() * 1000.0);
    let profile = Cascade::profile().unwrap_or_else(|| panic!("cascade"));
    let t = std::time::Instant::now();
    let n = profile
        .detect(
            &image,
            ScanParams {
                min_size: 28,
                ..ScanParams::default()
            },
        )
        .len();
    println!(
        "profile: {n} in {:.1} ms",
        t.elapsed().as_secs_f32() * 1000.0
    );
    let raw = frontal.detect_raw(&image, ScanParams::default()).len();
    println!("raw windows {raw}");
}

#[test]
#[ignore = "writes painted fixtures to AURA_PORTRAIT_EVAL_OUT"]
fn dump_painted_fixtures() {
    let Some(out) = out_dir() else { return };
    for (i, tone) in aura_portrait::fixtures::MONK.iter().enumerate() {
        let p = aura_portrait::fixtures::portrait(&aura_portrait::fixtures::PortraitSpec {
            skin: *tone,
            ..aura_portrait::fixtures::PortraitSpec::default()
        });
        write_ppm(
            &out.join(format!("rust_mst{}.ppm", i + 1)),
            p.width,
            p.height,
            &p.rgb,
        );
    }
}

fn overlay(rgb: &mut [u8], width: u32, height: u32, plane: &aura_portrait::Plane, colour: [u8; 3]) {
    let resized = plane.resize(width, height);
    for (i, w) in resized.values.iter().enumerate() {
        let a = w.clamp(0.0, 1.0) * 0.6;
        if let Some(px) = rgb.get_mut(i * 3..i * 3 + 3) {
            for (c, target) in px.iter_mut().zip(colour) {
                *c = (f32::from(*c) * (1.0 - a) + f32::from(target) * a).round() as u8;
            }
        }
    }
}

#[test]
#[ignore = "needs AURA_PORTRAIT_EVAL_DIR"]
fn regions_on_colour_frames() {
    use aura_portrait::Region;
    let Some(out) = out_dir() else { return };
    for path in files("ppm") {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with("full_") {
            continue;
        }
        let Some((width, height, 3, rgb)) = read_pnm(&path) else {
            continue;
        };
        let Some(canvas) = aura_portrait::Canvas::from_srgb8(&rgb, width, height, 1024) else {
            continue;
        };
        let started = std::time::Instant::now();
        let map = aura_portrait::analyse(&canvas, &[]);
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        let stats: Vec<String> = map
            .stats
            .iter()
            .filter(|s| s.coverage > 0.0005)
            .map(|s| {
                format!(
                    "{}:{:.1}%/{:.2}",
                    s.region.as_str(),
                    s.coverage * 100.0,
                    s.confidence
                )
            })
            .collect();
        println!(
            "{name} ({ms:.0} ms, {} faces) {}",
            map.faces.len(),
            stats.join(" ")
        );
        let get = |r: Region| {
            map.plane(r)
                .cloned()
                .unwrap_or_else(|| aura_portrait::Plane::zeros(1, 1))
        };
        let mut people = rgb.clone();
        overlay(
            &mut people,
            width,
            height,
            &get(Region::Background),
            [40, 40, 160],
        );
        overlay(
            &mut people,
            width,
            height,
            &get(Region::Sky),
            [120, 200, 255],
        );
        overlay(
            &mut people,
            width,
            height,
            &get(Region::Clothing),
            [60, 200, 60],
        );
        overlay(
            &mut people,
            width,
            height,
            &get(Region::Hair),
            [200, 120, 0],
        );
        overlay(
            &mut people,
            width,
            height,
            &get(Region::Skin),
            [255, 60, 160],
        );
        write_ppm(
            &out.join(format!("{name}_people.ppm")),
            width,
            height,
            &people,
        );
        let mut face = rgb.clone();
        overlay(
            &mut face,
            width,
            height,
            &get(Region::Eyebrows),
            [140, 70, 20],
        );
        overlay(
            &mut face,
            width,
            height,
            &get(Region::UnderEyes),
            [255, 0, 255],
        );
        overlay(&mut face, width, height, &get(Region::Nose), [0, 200, 0]);
        overlay(
            &mut face,
            width,
            height,
            &get(Region::Sclera),
            [255, 255, 255],
        );
        overlay(&mut face, width, height, &get(Region::Iris), [0, 220, 255]);
        overlay(&mut face, width, height, &get(Region::Lips), [255, 0, 0]);
        overlay(&mut face, width, height, &get(Region::Teeth), [255, 255, 0]);
        overlay(
            &mut face,
            width,
            height,
            &get(Region::FacialHair),
            [255, 128, 0],
        );
        write_ppm(
            &out.join(format!("{name}_features.ppm")),
            width,
            height,
            &face,
        );
    }
}

#[test]
#[ignore = "needs AURA_PORTRAIT_EVAL_DIR"]
fn readings_on_colour_frames() {
    for path in files("ppm") {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with("full_") {
            continue;
        }
        let Some((width, height, 3, rgb)) = read_pnm(&path) else {
            continue;
        };
        let Some(canvas) = aura_portrait::Canvas::from_srgb8(&rgb, width, height, 1024) else {
            continue;
        };
        let map = aura_portrait::analyse(&canvas, &[]);
        let r = aura_portrait::readings::read(&canvas, &map);
        println!(
            "{name:16} faces {} rough {:?} tone {:?} undereye {:?} shine {:?} teeth {:?} sclera {:?} hair {:.3} face {:?} frame {:.0}",
            r.faces,
            r.skin_roughness.map(|v| (v * 10.0).round() / 10.0),
            r.tone_variation.map(|v| (v * 10.0).round() / 10.0),
            r.under_eye_depth.map(|v| (v * 10.0).round() / 10.0),
            r.shine_share.map(|v| (v * 1000.0).round() / 1000.0),
            r.teeth_yellowness.map(|v| (v * 10.0).round() / 10.0),
            r.sclera_redness.map(|v| (v * 10.0).round() / 10.0),
            r.hair_coverage,
            r.face_lightness.map(|v| v.round()),
            r.frame_lightness
        );
    }
}

#[test]
#[ignore = "writes overlay planes for a UI preview to AURA_PORTRAIT_EVAL_OUT"]
fn dump_overlays_for_ui_preview() {
    let (Some(dir), Some(out)) = (dir(), out_dir()) else {
        return;
    };
    let Some((width, height, 3, rgb)) = read_pnm(&dir.join("obama2.ppm")) else {
        return;
    };
    let Some(canvas) = aura_portrait::Canvas::from_srgb8(&rgb, width, height, 1024) else {
        return;
    };
    let map = aura_portrait::analyse(&canvas, &[]);
    let (ow, oh) = aura_portrait::canvas::fit(map.width, map.height, 320);
    let mut index = String::new();
    for stat in &map.stats {
        if stat.coverage <= 1e-4 {
            continue;
        }
        let plane = map.resolve(stat.region, ow, oh);
        let bytes: Vec<u8> = plane
            .values
            .iter()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();
        let _ = std::fs::write(out.join(format!("ov_{}.bin", stat.region.as_str())), bytes);
        index.push_str(&format!(
            "{} {} {} {}\n",
            stat.region.as_str(),
            stat.region.label().replace(' ', "_"),
            stat.coverage,
            stat.confidence
        ));
    }
    let w = map.width as f32;
    let h = map.height as f32;
    for f in &map.faces {
        let b = f.bbox();
        index.push_str(&format!(
            "face {} {} {} {} {} {} {} {} {} {} {} {}\n",
            b[0] / w,
            b[1] / h,
            b[2] / w,
            b[3] / h,
            f.left_eye[0] / w,
            f.left_eye[1] / h,
            f.right_eye[0] / w,
            f.right_eye[1] / h,
            f.nose[0] / w,
            f.nose[1] / h,
            f.mouth[0] / w,
            f.mouth[1] / h
        ));
    }
    index.push_str(&format!("size {ow} {oh}\n"));
    let _ = std::fs::write(out.join("ov_index.txt"), index);
}
