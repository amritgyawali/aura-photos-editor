//! Body-skin spot healing over a full-length figure: the skin is a few separate patches inside
//! a large body rectangle, and the work should follow the skin rather than the rectangle.
//! ADR-0124.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::collections::BTreeMap;

use aura_recipe::retouch_tools::{BrushMask, BrushStroke, Edit, Matte, Tool};

/// Skin patches of a standing figure, normalised to its body box: a V of chest under the face,
/// two bare arms ending in hands, and two lower legs under a skirt.
const PATCHES: [[f32; 4]; 5] = [
    [0.40, 0.10, 0.60, 0.20],
    [0.04, 0.22, 0.14, 0.60],
    [0.86, 0.22, 0.96, 0.60],
    [0.30, 0.76, 0.42, 0.98],
    [0.58, 0.76, 0.70, 0.98],
];

/// The spacing of the spots, across and down, on a frame `w` wide.
fn lattice(w: usize) -> (usize, usize) {
    (w * 97 / 3000, w * 89 / 3000)
}

/// A `w x h` frame with a figure in `body` (normalised): pored skin with inflamed spots on a
/// lattice inside the patches, a blue dress and a grey wall elsewhere. Returns the linear
/// pixels and the body matte at the segmenter's 256-cell resolution.
fn figure(w: usize, h: usize, body: [f32; 4]) -> (Vec<f32>, Matte) {
    let [l, t, r, b] = body;
    let in_patch = |u: f32, v: f32| {
        PATCHES
            .iter()
            .any(|[a, c, d, e]| u >= *a && u < *d && v >= *c && v < *e)
    };
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let mut n = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
            n ^= n >> 13;
            n = n.wrapping_mul(0xC2B2_AE3D);
            n ^= n >> 16;
            let grain = 1.0 + ((n >> 8) as f32 / 16_777_216.0 - 0.5) * 0.05;
            let u = ((x as f32 + 0.5) / w as f32 - l) / (r - l);
            let v = ((y as f32 + 0.5) / h as f32 - t) / (b - t);
            let inside = (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v);
            let p = if inside && in_patch(u, v) {
                let (px, py) = lattice(w);
                let d =
                    ((x % px) as f32 - (px / 2) as f32).hypot((y % py) as f32 - (py / 2) as f32);
                let core = 4.0 * w as f32 / 3000.0;
                let spot = (1.0 - (d - core).max(0.0) / (core * 0.75)).clamp(0.0, 1.0);
                [
                    0.40 * grain * (1.0 + 0.05 * spot),
                    0.29 * grain * (1.0 - 0.22 * spot),
                    0.23 * grain * (1.0 - 0.22 * spot),
                ]
            } else if inside {
                [0.05 * grain, 0.07 * grain, 0.20 * grain]
            } else {
                [0.30 * grain, 0.30 * grain, 0.29 * grain]
            };
            rgb.extend(p);
        }
    }
    let mh = 256_usize;
    let mw = ((r - l) * w as f32 / ((b - t) * h as f32) * mh as f32).round() as usize;
    let alpha: Vec<u8> = (0..mw * mh)
        .map(|i| {
            let u = ((i % mw) as f32 + 0.5) / mw as f32;
            let v = ((i / mw) as f32 + 0.5) / mh as f32;
            if in_patch(u, v) {
                255
            } else {
                0
            }
        })
        .collect();
    (rgb, Matte::encode(body, mw as u32, mh as u32, &alpha))
}

/// The body spot heal the automatic planner writes (`body_cleanup::spots`, deep cleanup).
fn body_heal(body: [f32; 4], radius_px: f32, w: usize, h: usize) -> Edit {
    let [l, t, r, b] = body;
    let short = w.min(h) as f32;
    let strokes = (0..12)
        .map(|k| {
            let y = t + (b - t) * k as f32 / 11.0;
            BrushStroke {
                erase: false,
                radius: ((r - l) * w as f32 * 0.5 / short).min(0.25),
                opacity: 1.0,
                points: vec![[l, y, 1.0], [r, y, 1.0]],
            }
        })
        .collect();
    Edit {
        id: "auto-portrait-v1-0-body-spots".into(),
        tool: Tool::FrequencyHeal,
        enabled: true,
        region: [(l + r) * 0.5, (t + b) * 0.5, (r - l) * 0.5, (b - t) * 0.5],
        source: None,
        amount: 1.0,
        feather: 0.6,
        radius: radius_px / short,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        repair_passes: None,
        target_color: None,
        sensitivity: Some(0.5),
        keep_dark_marks: false,
        mole_edge: Some(0.0),
        texture: 0.65,
        tone: 1.0,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
        matte: Some("body".into()),
        mask: Some(BrushMask { strokes }),
        skin: None,
    }
}

/// Mean redness share over the spot centres inside one patch, before and after.
fn spot_red(rgb: &[f32], w: usize, h: usize, body: [f32; 4], patch: [f32; 4]) -> (f32, usize) {
    let [l, t, r, b] = body;
    let [pl, pt, pr, pb] = patch;
    let (x0, x1) = (
        ((l + (r - l) * pl) * w as f32) as usize + 8,
        ((l + (r - l) * pr) * w as f32) as usize - 8,
    );
    let (y0, y1) = (
        ((t + (b - t) * pt) * h as f32) as usize + 8,
        ((t + (b - t) * pb) * h as f32) as usize - 8,
    );
    let (px, py) = lattice(w);
    let (mut sum, mut n) = (0.0, 0);
    for y in (y0..y1).filter(|y| y % py == py / 2) {
        for x in (x0..x1).filter(|x| x % px == px / 2) {
            let i = (y * w + x) * 3;
            sum += (rgb[i] - (rgb[i + 1] + rgb[i + 2]) * 0.5) / (rgb[i] + rgb[i + 1] + rgb[i + 2]);
            n += 1;
        }
    }
    (sum / n.max(1) as f32, n)
}

/// Every separate patch of a full-length figure's body skin is healed, and nothing away from
/// the skin is touched.
#[test]
fn every_patch_of_a_full_length_figure_is_healed_and_nothing_else_is_touched() {
    let (w, h) = (1500_usize, 2250_usize);
    let body = [0.30, 0.09, 0.70, 0.98];
    let (rgb, matte) = figure(w, h, body);
    let mattes = BTreeMap::from([("body".to_owned(), matte)]);
    let mut out = rgb.clone();
    aura_render::retouch_tools::apply_with_mattes(
        &mut out,
        w,
        h,
        &[body_heal(body, 1.5, w, h)],
        &mattes,
    );
    let skin = (0.40 - (0.29 + 0.23) * 0.5) / (0.40 + 0.29 + 0.23);
    for patch in PATCHES {
        let (before, n) = spot_red(&rgb, w, h, body, patch);
        let (after, _) = spot_red(&out, w, h, body, patch);
        assert!(n >= 1, "no spot centre measured in {patch:?}");
        let left = (after - skin) / (before - skin);
        assert!(left < 0.5, "{patch:?}: {left:.2} of the redness left");
    }
    // Forty pixels from any patch, every pixel is exactly as it was.
    let [l, t, r, b] = body;
    let near = |x: usize, y: usize| {
        PATCHES.iter().any(|[a, c, d, e]| {
            let (px0, px1) = ((l + (r - l) * a) * w as f32, (l + (r - l) * d) * w as f32);
            let (py0, py1) = ((t + (b - t) * c) * h as f32, (t + (b - t) * e) * h as f32);
            let (x, y) = (x as f32, y as f32);
            x > px0 - 40.0 && x < px1 + 40.0 && y > py0 - 40.0 && y < py1 + 40.0
        })
    };
    for y in 0..h {
        for x in 0..w {
            if !near(x, y) {
                let i = (y * w + x) * 3;
                assert_eq!(&out[i..i + 3], &rgb[i..i + 3], "{x},{y} changed");
            }
        }
    }
}

/// Timing, by hand:
/// `cargo test --release -p aura-render --test body_skin_islands -- --ignored --nocapture`.
#[test]
#[ignore = "timing, by hand"]
fn timing() {
    let (w, h) = (3000_usize, 4500_usize);
    let body = [0.30, 0.09, 0.70, 0.98];
    let (rgb, matte) = figure(w, h, body);
    let edit = body_heal(body, 3.0, w, h);
    let mattes = BTreeMap::from([("body".to_owned(), matte)]);
    let mut out = rgb.clone();
    let started = std::time::Instant::now();
    aura_render::retouch_tools::apply_with_mattes(&mut out, w, h, &[edit], &mattes);
    println!("body heal over {w}x{h}: {:?}", started.elapsed());
    for (label, ms) in aura_render::perf::take() {
        println!("  {label}: {ms} ms");
    }
}
