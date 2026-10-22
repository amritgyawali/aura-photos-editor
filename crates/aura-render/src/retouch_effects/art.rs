//! Artistic looks: watercolour, stained glass, cartoon, neon edges and thermal. ADR-0134.
use super::{area, each, hash, mix, opponent, seed_of, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Watercolor | Tool::StainedGlass | Tool::Cartoon | Tool::NeonEdges | Tool::Thermal
    )
}

fn encode(v: f32) -> f32 {
    v.clamp(0.0, 1.0).powf(1.0 / 2.2)
}

fn decode(e: f32) -> f32 {
    e.clamp(0.0, 1.0).powf(2.2)
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::StainedGlass => glass(rgb, frame, edit, coverage),
        Tool::Thermal => thermal(rgb, frame, edit, coverage),
        _ => painted(rgb, frame, edit, coverage),
    }
}

/// The selection with a margin, its three channels and luminance, smoothed at `radius`, and
/// the luminance edge strength at each pixel.
struct Prepared {
    snapshot: Snapshot,
    soft: [Vec<f32>; 3],
    edge: Vec<f32>,
}

fn prepare(rgb: &[f32], frame: Frame, coverage: &Coverage, radius: usize) -> Option<Prepared> {
    let source = area(coverage, frame, radius * 3 + 2)?;
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let soft = [0, 1, 2].map(|c| blur(&snapshot.plane(c), r.w, r.h, radius));
    // Edges between colours count as much as edges between brightnesses: a red sari against a
    // blue wall is an edge a painter draws even where the two are equally light.
    let calm: Vec<Vec<f32>> = (0..3)
        .map(|c| {
            let plane: Vec<f32> = snapshot.plane(c).iter().map(|v| encode(*v)).collect();
            blur(&plane, r.w, r.h, 1)
        })
        .collect();
    let edge = (0..r.len())
        .map(|i| {
            let (x, y) = (i % r.w, i / r.w);
            calm.iter()
                .map(|plane| {
                    let at = |dx: isize, dy: isize| {
                        let xx = (x as isize + dx).clamp(0, r.w as isize - 1) as usize;
                        let yy = (y as isize + dy).clamp(0, r.h as isize - 1) as usize;
                        plane[yy * r.w + xx]
                    };
                    (at(1, 0) - at(-1, 0)).hypot(at(0, 1) - at(0, -1))
                })
                .fold(0.0_f32, f32::max)
        })
        .collect();
    Some(Prepared {
        snapshot,
        soft,
        edge,
    })
}

fn painted(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).clamp(1, 24);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(p) = prepare(rgb, frame, coverage, radius) else {
        return;
    };
    let tone = edit.tone.clamp(0.0, 1.0);
    let seed = seed_of(edit);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = p.snapshot.index(x, y);
        let soft = [p.soft[0][i], p.soft[1][i], p.soft[2][i]];
        let edge = p.edge[i];
        let new = match edit.tool {
            Tool::Watercolor => {
                // Washes: the smoothed colour, lightened and a little less saturated.
                let [l, cb, cr] = opponent(soft);
                let wash = super::from_opponent([l, cb * 0.85, cr * 0.85])
                    .map(|v| decode(encode(v) * 0.85 + 0.15));
                // Pigment pools where a wash meets another.
                let pooled = 1.0 - 0.5 * smoothstep(0.03, 0.15, edge);
                // Paper grain, fixed to the paper.
                let grain = 1.0 + (hash(x as i64 / 2, y as i64 / 2, seed) - 0.5) * 0.08;
                wash.map(|v| v * pooled * grain)
            }
            Tool::Cartoon => {
                let levels = 2.0 + (tone * 8.0).round();
                let l = luma(soft).max(1e-5);
                let stepped = decode((encode(l) * levels).round() / levels);
                let flat = soft.map(|v| v * stepped / l);
                let ink = smoothstep(0.08, 0.2, edge);
                mix(flat, [0.01; 3], ink)
            }
            _ => {
                // Neon: an edge glows in its own colour, brightened; the rest is darkened.
                let glow = smoothstep(0.04, 0.18, edge);
                let l = luma(old).max(1e-4);
                let vivid = old.map(|v| (v / l).powf(1.5) * 0.6);
                let background = old.map(|v| v * (1.0 - (0.5 + 0.45 * tone)));
                mix(background, vivid, glow)
            }
        };
        Some(mix(old, new, k))
    });
}

fn glass(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let cell = (edit.radius * frame.short()).clamp(4.0, 200.0);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let seed = seed_of(edit);
    let snapshot = Snapshot::take(rgb, frame, rect);
    // One seed per grid cell, jittered; a pixel belongs to its nearest seed.
    let site = |gx: i64, gy: i64| -> [f32; 2] {
        [
            (gx as f32 + 0.15 + 0.7 * hash(gx, gy, seed)) * cell,
            (gy as f32 + 0.15 + 0.7 * hash(gx, gy, seed ^ 1)) * cell,
        ]
    };
    let nearest = |px: f32, py: f32| -> ((i64, i64), f32, f32) {
        let (gx, gy) = ((px / cell).floor() as i64, (py / cell).floor() as i64);
        let mut best = ((gx, gy), f32::MAX, f32::MAX);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let s = site(gx + dx, gy + dy);
                let d = (px - s[0]).hypot(py - s[1]);
                if d < best.1 {
                    best = ((gx + dx, gy + dy), d, best.1);
                } else if d < best.2 {
                    best.2 = d;
                }
            }
        }
        best
    };
    // Each cell's colour: the photograph at its seed, averaged over a small patch.
    let colour = |id: (i64, i64)| -> [f32; 3] {
        let s = site(id.0, id.1);
        let mut sum = [0.0; 3];
        for (dx, dy) in [
            (0.0, 0.0),
            (0.25, 0.0),
            (-0.25, 0.0),
            (0.0, 0.25),
            (0.0, -0.25),
        ] {
            let p = snapshot.bilinear(s[0] + dx * cell, s[1] + dy * cell);
            for c in 0..3 {
                sum[c] += p[c] / 5.0;
            }
        }
        sum
    };
    let lead = (cell * 0.06).max(1.0);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (id, d1, d2) = nearest(x as f32 + 0.5, y as f32 + 0.5);
        // Leading where the two nearest seeds are almost equally near.
        let leading = 1.0 - smoothstep(lead * 0.5, lead * 1.5, d2 - d1);
        let pane = colour(id).map(|v| v * 1.1);
        Some(mix(old, mix(pane, [0.02; 3], leading), k))
    });
}

fn thermal(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    const PALETTE: [[f32; 3]; 6] = [
        [0.0, 0.0, 0.02],
        [0.05, 0.0, 0.35],
        [0.5, 0.0, 0.5],
        [0.95, 0.2, 0.05],
        [1.0, 0.75, 0.0],
        [1.0, 1.0, 0.85],
    ];
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let t = encode(luma(old)) * (PALETTE.len() - 1) as f32;
        let i = (t.floor() as usize).min(PALETTE.len() - 2);
        let f = t - i as f32;
        let colour =
            std::array::from_fn(|c| PALETTE[i][c] + (PALETTE[i + 1][c] - PALETTE[i][c]) * f);
        Some(mix(old, colour, k))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn cartoon_draws_ink_on_edges_and_flattens_areas() {
        let (w, h) = (60, 40);
        let base = fill(w, h, |x, y| {
            let n = super::super::hash(x as i64, y as i64, 5) * 0.06;
            if x < 30 {
                [0.5 + n, 0.3, 0.2]
            } else {
                [0.1, 0.2 + n, 0.5]
            }
        });
        let mut e = edit(Tool::Cartoon, [0.5, 0.5, 0.8, 0.8]);
        e.radius = 0.03;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 30, 20)) < 0.08, "ink at the edge");
        let row: Vec<f32> = (5..20).map(|x| px(&out, w, x, 20)[0]).collect();
        let spread = row.iter().copied().fold(0.0_f32, f32::max)
            - row.iter().copied().fold(1.0_f32, f32::min);
        assert!(spread < 0.05, "flat area: {spread}");
    }

    #[test]
    fn stained_glass_makes_panes_with_leading() {
        let (w, h) = (80, 80);
        let base = fill(w, h, |x, y| [x as f32 / 80.0, y as f32 / 80.0, 0.4]);
        let mut e = edit(Tool::StainedGlass, [0.5, 0.5, 0.6, 0.6]);
        e.radius = 0.15;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let dark = out
            .chunks_exact(3)
            .filter(|p| luma([p[0], p[1], p[2]]) < 0.05)
            .count();
        assert!(dark > 30 && dark < w * h / 3, "{dark}");
    }

    #[test]
    fn thermal_maps_dark_to_blue_black_and_bright_to_white_yellow() {
        let (w, h) = (20, 10);
        let base = fill(w, h, |x, _| if x < 10 { [0.003; 3] } else { [0.9; 3] });
        let mut e = edit(Tool::Thermal, [0.5, 0.5, 0.8, 0.8]);
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let cold = px(&out, w, 5, 5);
        let hot = px(&out, w, 15, 5);
        assert!(cold[2] >= cold[0] && luma(cold) < 0.1, "{cold:?}");
        assert!(hot[0] > 0.9 && hot[1] > 0.7, "{hot:?}");
    }

    #[test]
    fn neon_edges_glow_and_the_rest_darkens() {
        let (w, h) = (60, 40);
        let base = fill(w, h, |x, _| {
            if x < 30 {
                [0.4, 0.1, 0.1]
            } else {
                [0.1, 0.1, 0.4]
            }
        });
        let mut e = edit(Tool::NeonEdges, [0.5, 0.5, 0.8, 0.8]);
        e.feather = 0.0;
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 10, 20)) < luma(px(&base, w, 10, 20)) * 0.2);
        assert!(luma(px(&out, w, 30, 20)) > luma(px(&out, w, 10, 20)) * 3.0);
    }

    #[test]
    fn watercolour_lightens_and_softens() {
        let (w, h) = (60, 40);
        let base = checker(w, h, 3);
        let mut e = edit(Tool::Watercolor, [0.5, 0.5, 0.8, 0.8]);
        e.radius = 0.05;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let mean = |rgb: &[f32]| rgb.iter().sum::<f32>() / rgb.len() as f32;
        assert!(mean(&out) > mean(&base) * 0.9);
        let contrast = |rgb: &[f32]| (px(rgb, w, 30, 20)[0] - px(rgb, w, 33, 20)[0]).abs();
        assert!(contrast(&out) < contrast(&base) * 0.5);
    }
}
