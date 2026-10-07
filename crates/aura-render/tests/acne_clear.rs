//! Acne clear on skin whose answer is known. ADR-0092.
//!
//! The frames are painted: a skin tone under a slope of light, pore-sized relief, a shadow
//! across one side, a crease, and marks whose size, colour and position this file chose - a
//! dense cluster among them, the case frequency healing (ADR-0090) leaves half done. That proves
//! the arithmetic - what is found, what is left alone, what is conserved - and says nothing
//! about a photograph of a person. `auto_retouch_photos.rs` in `aura-app` is the by-hand check
//! on real pixels.
// Tests assert by unwrapping; a panic here is a failed test, never a photographer's crash.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::float_cmp
)]

use aura_recipe::retouch_tools::{BrushMask, BrushStroke, Edit, Tool};
use aura_render::retouch_tools::{apply, frequency_heal_marks};
use std::collections::BTreeMap;

const W: usize = 256;

/// An acne clear over most of the frame, as the automatic pass plans it.
fn operation() -> Edit {
    Edit {
        id: "clear".into(),
        tool: Tool::AcneClear,
        enabled: true,
        region: [0.5, 0.5, 0.75, 0.75],
        source: None,
        amount: 1.0,
        feather: 0.0,
        // Four pixels on this frame: pores below it, marks above.
        radius: 4.0 / W as f32,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture_heal: false,
        sensitivity: Some(0.7),
        keep_dark_marks: false,
        texture: 0.25,
        tone: 1.0,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
        // A matte-less operation is a brush; the automatic pass always names a matte. The
        // tests that want the automatic behaviour give it one covering the frame.
        matte: Some("skin".into()),
        mask: None,
        skin: None,
    }
}

fn full_matte() -> BTreeMap<String, aura_recipe::retouch_tools::Matte> {
    let mut matte =
        aura_recipe::retouch_tools::Matte::encode([0.0, 0.0, 1.0, 1.0], 64, 64, &[255; 64 * 64]);
    matte.refine_edges = false;
    BTreeMap::from([("skin".to_owned(), matte)])
}

fn render(before: &[f32], edit: &Edit) -> Vec<f32> {
    let mut out = before.to_vec();
    aura_render::retouch_tools::apply_with_mattes(
        &mut out,
        W,
        W,
        std::slice::from_ref(edit),
        &full_matte(),
    );
    out
}

/// Pore relief: a fixed pattern of about three per cent, different at every pixel.
fn pore(x: usize, y: usize) -> f32 {
    let mut v = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    v ^= v >> 13;
    v = v.wrapping_mul(0xC2B2_AE3D);
    v ^= v >> 16;
    ((v % 2001) as f32 / 1000.0 - 1.0) * 0.03
}

/// How much a shadow darkens the right of the frame, with a soft edge at x = 190.
fn shadow(x: usize) -> f32 {
    let t = ((x as f32 - 184.0) / 12.0).clamp(0.0, 1.0);
    1.0 - 0.4 * t * t * (3.0 - 2.0 * t)
}

/// Skin of one tone under light that falls off across the frame, a shadow on its right, pores.
fn skin(tone: [f32; 3], exposure: f32) -> Vec<f32> {
    (0..W * W)
        .flat_map(|i| {
            let (x, y) = (i % W, i / W);
            let light = exposure * (1.0 - x as f32 * 0.0010 + y as f32 * 0.0004) * shadow(x);
            let relief = 1.0 + pore(x, y);
            tone.map(|v| v * light * relief)
        })
        .collect()
}

/// Multiply a soft-edged disk by `colour`.
fn mark(rgb: &mut [f32], centre: [usize; 2], radius: f32, colour: [f32; 3]) {
    for y in 0..W {
        for x in 0..W {
            let d = (x as f32 - centre[0] as f32).hypot(y as f32 - centre[1] as f32);
            let t = (radius + 0.5 - d).clamp(0.0, 1.0);
            for c in 0..3 {
                rgb[(y * W + x) * 3 + c] *= 1.0 + t * (colour[c] - 1.0);
            }
        }
    }
}

/// Redder and darker: an inflamed spot.
const RED: [f32; 3] = [0.86, 0.60, 0.62];
/// Browner and darker: a healed mark's pigment.
const BROWN: [f32; 3] = [0.74, 0.62, 0.48];
/// Darker only: a mole, a freckle, or a shadow.
const GREY: [f32; 3] = [0.6, 0.6, 0.6];

const SINGLE: [[usize; 2]; 3] = [[40, 50], [120, 40], [60, 200]];
/// Twelve marks whose edges touch or overlap: every one of them has another beside it, so
/// no ring around any of them is clean skin.
const CLUSTER: [[usize; 2]; 12] = [
    [112, 118],
    [119, 116],
    [126, 119],
    [133, 117],
    [115, 125],
    [122, 124],
    [129, 126],
    [136, 124],
    [113, 132],
    [120, 131],
    [127, 133],
    [134, 131],
];
const CREASE_Y: usize = 90;

fn marked(tone: [f32; 3], exposure: f32) -> Vec<f32> {
    let mut rgb = skin(tone, exposure);
    for (n, centre) in SINGLE.iter().enumerate() {
        mark(&mut rgb, *centre, 4.0, if n == 1 { BROWN } else { RED });
    }
    for centre in CLUSTER {
        mark(&mut rgb, centre, 3.5, RED);
    }
    // A long crease, darker and no redder.
    for x in 20..180 {
        for y in CREASE_Y..CREASE_Y + 2 {
            for c in 0..3 {
                rgb[(y * W + x) * 3 + c] *= 0.72;
            }
        }
    }
    rgb
}

/// Mean log red-over-green and log luminance over a disk, minus the same over its ring.
fn departure(rgb: &[f32], centre: [usize; 2], radius: f32) -> (f32, f32) {
    let read = |x: usize, y: usize| {
        let i = (y * W + x) * 3;
        let p = [rgb[i], rgb[i + 1], rgb[i + 2]];
        (
            (p[0] / p[1]).ln(),
            (p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593).ln(),
        )
    };
    let (mut inside, mut ring) = ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0));
    let reach = (radius * 3.5).ceil() as usize;
    for y in centre[1] - reach..centre[1] + reach {
        for x in centre[0] - reach..centre[0] + reach {
            let d = (x as f32 - centre[0] as f32).hypot(y as f32 - centre[1] as f32);
            let (red, lum) = read(x, y);
            if d <= radius * 0.7 {
                inside = (inside.0 + red, inside.1 + lum, inside.2 + 1.0);
            } else if (radius * 2.5..radius * 3.3).contains(&d) {
                ring = (ring.0 + red, ring.1 + lum, ring.2 + 1.0);
            }
        }
    }
    (
        inside.0 / inside.2 - ring.0 / ring.2,
        ring.1 / ring.2 - inside.1 / inside.2,
    )
}

/// The cluster as one disk around its centre, against clean skin around it.
fn cluster_departure(rgb: &[f32]) -> (f32, f32) {
    let read = |x: usize, y: usize| {
        let i = (y * W + x) * 3;
        (rgb[i] / rgb[i + 1]).ln()
    };
    let mut on_marks = (0.0, 0.0);
    for centre in CLUSTER {
        on_marks = (on_marks.0 + read(centre[0], centre[1]), on_marks.1 + 1.0);
    }
    let mut around = (0.0, 0.0);
    for x in (95..160).step_by(3) {
        for y in [100_usize, 160] {
            around = (around.0 + read(x, y), around.1 + 1.0);
        }
    }
    (on_marks.0 / on_marks.1 - around.0 / around.1, 0.0)
}

#[test]
fn every_mark_in_a_dense_cluster_is_rebuilt() {
    let before = marked([0.42, 0.30, 0.22], 1.0);
    let after = render(&before, &operation());
    let (red_before, _) = cluster_departure(&before);
    let (red_after, _) = cluster_departure(&after);
    assert!(
        red_before > 0.2,
        "fixture: the cluster is red ({red_before})"
    );
    assert!(
        red_after < red_before * 0.1,
        "the cluster keeps {red_after} of {red_before} log redness"
    );
}

#[test]
fn isolated_red_and_brown_marks_are_rebuilt_in_colour_and_tone() {
    for tone in [[0.62, 0.48, 0.40], [0.42, 0.30, 0.22], [0.16, 0.10, 0.07]] {
        for exposure in [0.1, 1.0, 2.5] {
            let before = marked(tone, exposure);
            let after = render(&before, &operation());
            for centre in SINGLE {
                let (red0, dark0) = departure(&before, centre, 4.0);
                let (red1, dark1) = departure(&after, centre, 4.0);
                assert!(dark0 > 0.1, "fixture mark at {centre:?} is dark ({dark0})");
                assert!(
                    dark1.abs() < dark0 * 0.2 && red1.abs() < red0.abs() * 0.2 + 0.01,
                    "tone {tone:?} x{exposure}: mark at {centre:?} went from {red0:.3}/{dark0:.3} to {red1:.3}/{dark1:.3}"
                );
            }
        }
    }
}

#[test]
fn shadows_creases_and_clean_skin_keep_their_exact_values() {
    let before = marked([0.42, 0.30, 0.22], 1.0);
    let after = render(&before, &operation());
    let changed = |x: usize, y: usize| {
        let i = (y * W + x) * 3;
        before[i..i + 3] != after[i..i + 3]
    };
    // The crease and the skin either side of it, away from any mark.
    for x in 25..175 {
        for y in CREASE_Y - 3..CREASE_Y + 5 {
            if SINGLE
                .iter()
                .chain(&CLUSTER)
                .any(|c| (x as f32 - c[0] as f32).hypot(y as f32 - c[1] as f32) < 16.0)
            {
                continue;
            }
            assert!(!changed(x, y), "crease pixel {x},{y} moved");
        }
    }
    // The shadow, its edge included.
    for x in 170..250 {
        for y in 20..240 {
            assert!(!changed(x, y), "shadow pixel {x},{y} moved");
        }
    }
}

#[test]
fn a_dark_mark_is_kept_when_asked_and_a_mole_needs_skin_on_every_side() {
    let mut before = skin([0.42, 0.30, 0.22], 1.0);
    mark(&mut before, [80, 80], 4.0, GREY);
    let mut keep = operation();
    keep.keep_dark_marks = true;
    assert_eq!(render(&before, &keep), before, "a mole was touched");
    let (_, dark0) = departure(&before, [80, 80], 4.0);
    let (_, dark1) = departure(&render(&before, &operation()), [80, 80], 4.0);
    assert!(
        dark1 < dark0 * 0.2,
        "an enclosed dark mark stays {dark1} of {dark0}"
    );
}

#[test]
fn nothing_outside_the_selection_changes() {
    let before = marked([0.42, 0.30, 0.22], 1.0);
    let mut edit = operation();
    edit.matte = None;
    edit.region = [0.3, 0.3, 0.2, 0.2];
    let mut after = before.clone();
    apply(&mut after, W, W, std::slice::from_ref(&edit));
    for y in 0..W {
        for x in 0..W {
            let dx = (x as f32 + 0.5) / W as f32 - 0.3;
            let dy = (y as f32 + 0.5) / W as f32 - 0.3;
            if dx.hypot(dy) > 0.2 {
                let i = (y * W + x) * 3;
                assert_eq!(&after[i..i + 3], &before[i..i + 3], "pixel {x},{y}");
            }
        }
    }
}

#[test]
fn a_brush_clears_what_it_is_painted_over_bumps_included() {
    let mut before = skin([0.42, 0.30, 0.22], 1.0);
    // A skin-coloured bump: brighter, no redder - the automatic pass leaves it.
    mark(&mut before, [100, 100], 3.5, [1.22, 1.22, 1.22]);
    mark(&mut before, [150, 100], 4.0, RED);
    let automatic = render(&before, &operation());
    let (_, bump_auto) = departure(&automatic, [100, 100], 3.5);
    assert!(
        bump_auto < -0.1,
        "the automatic pass left the bump ({bump_auto})"
    );
    let mut brush = operation();
    brush.matte = None;
    brush.feather = 0.35;
    brush.mask = Some(BrushMask {
        strokes: vec![BrushStroke {
            erase: false,
            radius: 14.0 / W as f32,
            opacity: 1.0,
            points: vec![[100.5 / W as f32, 100.5 / W as f32, 1.0]],
        }],
    });
    let mut painted = before.clone();
    apply(&mut painted, W, W, std::slice::from_ref(&brush));
    let (_, bump) = departure(&painted, [100, 100], 3.5);
    assert!(bump.abs() < 0.04, "the painted bump keeps {bump}");
    // The red mark was not painted and is left as it was.
    let i = (100 * W + 150) * 3;
    assert_eq!(&painted[i..i + 3], &before[i..i + 3]);
}

#[test]
fn a_brush_over_a_spot_beside_a_brow_leaves_the_brow() {
    let mut before = skin([0.42, 0.30, 0.22], 1.0);
    // A brow: a band of dark brown hair, far darker than the skin.
    for y in 96..108 {
        for x in 60..200 {
            let i = (y * W + x) * 3;
            for (c, k) in [0.32_f32, 0.28, 0.24].into_iter().enumerate() {
                before[i + c] *= k;
            }
        }
    }
    mark(&mut before, [190, 116], 4.0, RED);
    let mut brush = operation();
    brush.matte = None;
    brush.feather = 0.35;
    brush.mask = Some(BrushMask {
        strokes: vec![BrushStroke {
            erase: false,
            radius: 16.0 / W as f32,
            opacity: 1.0,
            points: vec![[190.5 / W as f32, 110.5 / W as f32, 1.0]],
        }],
    });
    let mut painted = before.clone();
    apply(&mut painted, W, W, std::slice::from_ref(&brush));
    // The brow hair under the brush keeps its value; the red spot below it is cleared.
    for x in 180..200 {
        for y in 98..106 {
            let i = (y * W + x) * 3;
            let moved = (painted[i] / before[i]).ln().abs();
            assert!(moved < 0.1, "brow pixel {x},{y} moved by {moved}");
        }
    }
    let (red, _) = departure(&painted, [190, 116], 4.0);
    assert!(red < 0.05, "the spot beside the brow keeps {red}");
}

#[test]
fn pores_under_a_rebuilt_mark_stay_and_redness_evening_moves_colour_only() {
    let before = marked([0.42, 0.30, 0.22], 1.0);
    let after = render(&before, &operation());
    // Relief inside a rebuilt mark: the pixel-to-pixel variation of luminance.
    let relief = |rgb: &[f32], c: [usize; 2]| {
        let mut sum = 0.0;
        for y in c[1] - 2..c[1] + 2 {
            for x in c[0] - 2..c[0] + 2 {
                let l = |x: usize, y: usize| {
                    let i = (y * W + x) * 3;
                    (rgb[i] * 0.2627 + rgb[i + 1] * 0.678 + rgb[i + 2] * 0.0593).ln()
                };
                sum += (l(x + 1, y) - l(x, y)).abs();
            }
        }
        sum / 16.0
    };
    let clean = relief(&before, [80, 150]);
    for centre in SINGLE {
        let kept = relief(&after, centre);
        assert!(
            kept > clean * 0.5,
            "mark at {centre:?} lost its pores: {kept} against clean {clean}"
        );
    }
    // A flat blotch too broad to be a mark: with evening on, its colour goes part of the way
    // and the light falling on it does not change.
    // Wider than the largest group acne clear rebuilds (45 squared mark radii).
    let mut blotch = skin([0.42, 0.30, 0.22], 1.0);
    mark(&mut blotch, [90, 90], 18.0, [1.0, 0.93, 0.94]);
    let mut even = operation();
    even.preserve_microtexture = true;
    let evened = render(&blotch, &even);
    let (red0, dark0) = departure(&blotch, [90, 90], 8.0);
    let (red1, dark1) = departure(&evened, [90, 90], 8.0);
    assert!(red1 < red0 * 0.7, "blotch redness {red0} -> {red1}");
    assert!(
        dark1 <= dark0 + 1e-3,
        "evening darkened the blotch: {dark0} -> {dark1}"
    );
}

#[test]
fn the_mark_plane_shows_exactly_where_marks_were_rebuilt() {
    let before = marked([0.42, 0.30, 0.22], 1.0);
    let marks = frequency_heal_marks(&before, W, W, &operation(), &full_matte());
    for centre in SINGLE.iter().chain(&CLUSTER) {
        assert!(marks[centre[1] * W + centre[0]] > 0.9, "{centre:?}");
    }
    assert_eq!(marks[CREASE_Y * W + 60], 0.0);
    assert_eq!(marks[200 * W + 220], 0.0);
}

#[test]
#[ignore = "timing: run by hand with --release"]
fn a_face_sized_field_takes_seconds_not_minutes() {
    const N: usize = 1200;
    let rgb: Vec<f32> = (0..N * N)
        .flat_map(|i| {
            let (x, y) = (i % N, i / N);
            let relief = 1.0 + pore(x, y);
            [0.42 * relief, 0.30 * relief, 0.22 * relief]
        })
        .collect();
    let mut edit = operation();
    edit.radius = 11.0 / N as f32;
    edit.preserve_microtexture = true;
    edit.matte = None;
    edit.region = [0.5, 0.5, 0.7, 0.7];
    let mut out = rgb.clone();
    let started = std::time::Instant::now();
    apply(&mut out, N, N, std::slice::from_ref(&edit));
    println!(
        "acne clear over {N}x{N}: {:.2}s",
        started.elapsed().as_secs_f32()
    );
}
