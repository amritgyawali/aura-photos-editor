//! Replay the tone solve (ADR-0191) on saved renders as shot, printing what holds it back.
//! Ignored. `AURA_TONE_DIR` holds `<name>-shot.png` and `faces.json` (pixel boxes `[x, y, w, h]`
//! per name, from `scripts/photographer-pairs.py`).
#![allow(clippy::expect_used, clippy::print_stderr, clippy::disallowed_methods)]
use aura_app::tone_target as tt;

#[test]
#[ignore = "needs AURA_TONE_DIR"]
fn replay_the_tone_solve() {
    let Some(dir) = std::env::var_os("AURA_TONE_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let faces: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("faces.json")).expect("faces"))
            .expect("json");
    let only: Vec<String> = std::env::var("AURA_TONE_ONLY")
        .map(|v| v.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    for (name, boxes) in faces.as_object().expect("object") {
        if !only.is_empty() && !only.contains(name) {
            continue;
        }
        let Ok(img) = image::open(dir.join(format!("{name}-shot.png"))) else {
            continue;
        };
        let img = img.to_rgb8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let rgb = img.into_raw();
        let boxes: Vec<[f32; 4]> = boxes
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|b| {
                        let v: Vec<f32> = b
                            .as_array()?
                            .iter()
                            .filter_map(|x| x.as_f64().map(|x| x as f32))
                            .collect();
                        let [x, y, bw, bh] = v.as_slice() else {
                            return None;
                        };
                        Some([
                            x / w as f32,
                            y / h as f32,
                            (x + bw) / w as f32,
                            (y + bh) / h as f32,
                        ])
                    })
                    .collect()
            })
            .unwrap_or_default();
        let Some(shot) = tt::read(&rgb, w, h, &boxes) else {
            continue;
        };
        let goal = tt::targets(&shot);
        let samples = tt::samples(&rgb, w, h, &boxes);
        let clip_allowed = (shot.clipped + 0.003).max(0.005);
        let started = std::time::Instant::now();
        let Some((s, p)) = tt::solve(&samples, &goal, [false; 6], clip_allowed) else {
            continue;
        };
        let ms = started.elapsed().as_millis();
        if std::env::var_os("AURA_TONE_SWEEP").is_some() {
            for ev in [0.0_f32, 0.5, 1.0, 1.5] {
                let trial = tt::Sliders {
                    exposure: ev,
                    ..tt::Sliders::ZERO
                };
                if let Some(r) = tt::simulate(&samples, trial) {
                    eprintln!(
                        "   ev {ev:+.1}: skin {:?} mid {:.0} p98 {:.0} clip {:.4} cost {:.0}",
                        r.face.map(|v| v.round()),
                        r.l[2],
                        r.l[4],
                        r.clipped,
                        tt::cost(&r, &goal, trial, clip_allowed)
                    );
                }
            }
        }
        eprintln!(
            "{name}: skin {:?} -> {:?} (goal {:?}) mid {:.0} -> {:.0} ({:.0}) p98 {:.0} -> {:.0} ({:.0}) clip {:.3} -> {:.3} (allowed {:.3}) | ev {:+.2} hi {} sh {} wh {} bl {} co {} | cost {:.0} | {ms} ms",
            shot.face.map(|v| v.round()),
            p.face.map(|v| v.round()),
            goal.face.map(|v| v.round()),
            shot.l[2], p.l[2], goal.l[2], shot.l[4], p.l[4], goal.l[4],
            shot.clipped, p.clipped, clip_allowed,
            s.exposure, s.highlights, s.shadows, s.whites, s.blacks, s.contrast,
            tt::cost(&p, &goal, s, clip_allowed)
        );
    }
}
