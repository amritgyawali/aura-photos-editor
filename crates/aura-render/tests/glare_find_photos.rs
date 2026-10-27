//! Lens-reflection detection over a folder of real photographs, run by hand.
//!
//! `AURA_FIND_DIR` names a folder of `NAME_WxH.rgb` files (packed 8-bit sRGB). Each is parsed,
//! searched for reflections, and summarised in `findings.json` in the same folder: the faces
//! found and every proposed glare selection with its confidence, for a person to check.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::print_stdout
)]

use aura_render::Frame;

fn working(rgb: &[u8]) -> Vec<f32> {
    rgb.chunks_exact(3)
        .flat_map(|p| {
            aura_render::output::display_srgb_to_working(
                [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0),
            )
        })
        .collect()
}

#[test]
#[ignore = "needs AURA_FIND_DIR"]
fn reflections_are_proposed_on_real_photographs() {
    let dir = std::env::var("AURA_FIND_DIR").expect("AURA_FIND_DIR");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rgb"))
        .collect();
    entries.sort();
    let mut report = Vec::new();
    let (mut faces_total, mut with_findings) = (0usize, 0usize);
    for path in entries {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let (name, size) = stem.rsplit_once('_').unwrap();
        let (w, h) = size.split_once('x').unwrap();
        let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
        let bytes = std::fs::read(&path).unwrap();
        let frame = Frame::working(working(&bytes), w, h, "photo");
        let Some(map) = aura_render::portrait::parse(&frame, &[]) else {
            continue;
        };
        let found =
            aura_render::glare_find::find_reflections(&frame.rgb, w as usize, h as usize, &map);
        faces_total += map.faces.len();
        if !found.is_empty() {
            with_findings += 1;
        }
        let eyes: Vec<_> = map
            .faces
            .iter()
            .map(|f| {
                let (sx, sy) = (w as f32 / map.width as f32, h as f32 / map.height as f32);
                serde_json::json!({
                    "confidence": f.confidence,
                    "left": [f.left_eye[0] * sx / w as f32, f.left_eye[1] * sy / h as f32],
                    "right": [f.right_eye[0] * sx / w as f32, f.right_eye[1] * sy / h as f32],
                })
            })
            .collect();
        let looked: Vec<_> = aura_render::glare_find::inspect(&frame.rgb, w as usize, h as usize, &map)
            .into_iter()
            .map(|look| serde_json::json!({
                "face": look.face, "window": look.window, "threshold": look.threshold,
                "skin": look.skin, "eyePx": look.eye_px, "frame": look.frame,
                "blobs": look.blobs.iter().filter(|b| b.pixels >= 12).map(|b| serde_json::json!({
                    "bounds": b.bounds, "pixels": b.pixels, "outside": b.outside, "chroma": b.chroma,
                    "crisp": b.crisp, "strength": b.strength, "roughness": b.roughness, "confidence": b.confidence,
                    "verdict": b.verdict,
                })).collect::<Vec<_>>(),
            }))
            .collect();
        report.push(serde_json::json!({
            "inspection": looked,
            "name": name,
            "file": path.file_name().unwrap().to_string_lossy(),
            "faces": eyes,
            "findings": found.iter().map(|r| serde_json::json!({
                "region": r.region, "confidence": r.confidence, "face": r.face, "pixels": r.pixels
            })).collect::<Vec<_>>(),
        }));
    }
    println!(
        "{} photographs, {faces_total} faces, {with_findings} with a proposed reflection",
        report.len()
    );
    std::fs::write(
        std::path::Path::new(&dir).join("findings.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}
