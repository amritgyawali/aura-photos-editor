//! Repair and portrait: clipped highlight colour, local dehaze and the iris's limbal ring.
//! ADR-0134.
use super::{area, each, mix, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::ClippedHighlightRepair | Tool::LocalDehaze | Tool::LimbalRing
    )
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::ClippedHighlightRepair => clipped(rgb, frame, edit, coverage),
        Tool::LocalDehaze => dehaze(rgb, frame, edit, coverage),
        _ => limbal(rgb, frame, edit, coverage),
    }
}

/// How clipped a pixel is: 0 when no channel is near the top, 1 when one is at it.
fn clipping(p: [f32; 3]) -> f32 {
    smoothstep(0.92, 0.995, p[0].max(p[1]).max(p[2]))
}

fn clipped(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).max(3);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    // The colour of the unclipped surroundings, as each channel's share of luminance, weighted
    // by how unclipped each neighbour is: the colour the highlight would have had.
    let mut trust = vec![0.0_f32; r.len()];
    let mut ratio = [
        vec![0.0_f32; r.len()],
        vec![0.0_f32; r.len()],
        vec![0.0_f32; r.len()],
    ];
    for i in 0..r.len() {
        let p = snapshot.at((r.x0 + i % r.w) as isize, (r.y0 + i / r.w) as isize);
        let t = 1.0 - clipping(p);
        let l = luma(p).max(1e-4);
        trust[i] = t;
        for c in 0..3 {
            ratio[c][i] = p[c] / l * t;
        }
    }
    let weights = blur(&trust, r.w, r.h, radius);
    let shares = ratio.map(|plane| blur(&plane, r.w, r.h, radius));
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        let clip = clipping(old);
        if k <= 0.0 || clip <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        if weights[i] < 0.05 {
            return None;
        }
        // Keep the pixel's brightness, take its colour from the surroundings.
        let l = luma(old);
        let colour: [f32; 3] = std::array::from_fn(|c| shares[c][i] / weights[i] * l);
        Some(mix(old, colour, k * clip))
    });
}

/// The smallest value within `radius`, separably.
fn min_filter(plane: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let along = |src: &[f32], step: usize, len: usize, lines: usize, stride: usize| {
        let mut out = vec![0.0; src.len()];
        for line in 0..lines {
            for k in 0..len {
                let lo = k.saturating_sub(radius);
                let hi = (k + radius).min(len - 1);
                out[line * stride + k * step] = (lo..=hi)
                    .map(|j| src[line * stride + j * step])
                    .fold(f32::MAX, f32::min);
            }
        }
        out
    };
    let rows = along(plane, 1, w, h, w);
    along(&rows, w, h, w, 1)
}

fn dehaze(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let patch = (frame.short() * 0.01).round().clamp(2.0, 15.0) as usize;
    let Some(source) = area(coverage, frame, patch * 2) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    // The dark channel of the selection: a hazy area has no dark pixel anywhere near.
    let darkest: Vec<f32> = (0..r.len())
        .map(|i| {
            let p = snapshot.at((r.x0 + i % r.w) as isize, (r.y0 + i / r.w) as isize);
            p[0].min(p[1]).min(p[2])
        })
        .collect();
    let dark = blur(&min_filter(&darkest, r.w, r.h, patch), r.w, r.h, patch);
    // The haze colour: the brightest of the dark channel, measured on the selection itself.
    let mut sorted: Vec<f32> = dark.clone();
    sorted.sort_by(f32::total_cmp);
    let airlight = sorted[((sorted.len() - 1) as f32 * 0.995) as usize].max(0.05);
    let removal = 0.4 + edit.tone.clamp(0.0, 1.0) * 0.55;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        let transmission = (1.0 - removal * dark[i] / airlight).clamp(0.25, 1.0);
        let clear = old.map(|v| ((v - airlight) / transmission + airlight).max(0.0));
        Some(mix(old, clear, k))
    });
}

fn limbal(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let c = frame.centre(edit);
    let reach = frame.reach(edit);
    let width = 0.1 + edit.tone.clamp(0.0, 1.0) * 0.25;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let d = ((x as f32 + 0.5 - c[0]) / reach[0]).hypot((y as f32 + 0.5 - c[1]) / reach[1]);
        // A ring just inside the ellipse, soft on both sides.
        let ring =
            smoothstep(1.0 - width - 0.1, 1.0 - width * 0.5, d) * (1.0 - smoothstep(0.92, 1.0, d));
        if ring <= 0.0 {
            return None;
        }
        Some(mix(old, old.map(|v| v * 0.45), ring * k))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn a_clipped_highlight_takes_the_colour_of_its_surroundings() {
        let (w, h) = (40, 40);
        // Warm skin with a highlight blown in red and green but not blue: it reads yellow.
        let base = fill(w, h, |x, y| {
            if (17..23).contains(&x) && (17..23).contains(&y) {
                [1.0, 1.0, 0.75]
            } else {
                [0.6, 0.38, 0.28]
            }
        });
        let mut e = edit(Tool::ClippedHighlightRepair, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.15;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let p = px(&out, w, 20, 20);
        let ratio = |q: [f32; 3]| q[1] / q[0];
        assert!((ratio(p) - 0.38 / 0.6).abs() < 0.12, "{p:?}");
        assert!(
            (luma(p) - luma([1.0, 1.0, 0.75])).abs() < 0.02,
            "brightness kept"
        );
        assert_eq!(px(&out, w, 5, 20), px(&base, w, 5, 20));
    }

    #[test]
    fn local_dehaze_deepens_a_hazy_area() {
        let (w, h) = (60, 60);
        // A scene under a veil: every pixel lifted toward a grey haze.
        let base = fill(w, h, |x, y| {
            let scene = if (x / 6 + y / 6) % 2 == 0 { 0.05 } else { 0.4 };
            [0.5 * scene + 0.5 * 0.7; 3]
        });
        let mut e = edit(Tool::LocalDehaze, [0.5, 0.5, 0.6, 0.6]);
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let contrast = |rgb: &[f32]| (luma(px(rgb, w, 33, 30)) - luma(px(rgb, w, 27, 30))).abs();
        assert!(contrast(&out) > contrast(&base) * 1.3);
    }

    #[test]
    fn the_limbal_ring_darkens_the_iris_edge_and_not_its_centre() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |_, _| [0.2, 0.3, 0.35]);
        let mut e = edit(Tool::LimbalRing, [0.5, 0.5, 0.3, 0.3]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert_eq!(px(&out, w, 30, 30), px(&base, w, 30, 30));
        assert!(luma(px(&out, w, 46, 30)) < luma(px(&base, w, 46, 30)) * 0.8);
    }
}
