//! Engraved looks: emboss and crosshatch. ADR-0135.
use super::{area, each, mix, smoothstep, weight, Frame, Snapshot};
use crate::retouch_mask::Coverage;
use crate::retouch_planes::luma;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(tool, Tool::Emboss | Tool::Crosshatch)
}

fn encode(v: f32) -> f32 {
    v.clamp(0.0, 1.0).powf(1.0 / 2.2)
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, 2) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let keep = edit.tone.clamp(0.0, 1.0);
    let spacing = (edit.radius * frame.short()).clamp(3.0, 40.0);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let (xi, yi) = (x as isize, y as isize);
        let new = if edit.tool == Tool::Emboss {
            // Light from the top left: the difference across the diagonal.
            let a = encode(luma(snapshot.at(xi - 1, yi - 1)));
            let b = encode(luma(snapshot.at(xi + 1, yi + 1)));
            let relief = (0.5 + (b - a) * 2.5).clamp(0.0, 1.0);
            let grey = relief.powf(2.2);
            let l = luma(old).max(1e-5);
            mix([grey; 3], old.map(|v| v * grey / l), keep)
        } else {
            let e = encode(luma(old));
            let (px, py) = (x as f32, y as f32);
            // Three hatch directions that join in as the tone darkens.
            // A line is the outer fifth of each period, soft-edged: thin ink on paper. The first
            // version inked three quarters of every period, so dark areas went solid black.
            let line = |u: f32| {
                let t = (u / spacing).rem_euclid(1.0);
                smoothstep(0.36, 0.44, (t - 0.5).abs())
            };
            let mut ink = 0.0_f32;
            if e < 0.75 {
                ink = ink.max(line(px + py) * smoothstep(0.75, 0.6, e));
            }
            if e < 0.5 {
                ink = ink.max(line(px - py) * smoothstep(0.5, 0.35, e));
            }
            if e < 0.28 {
                ink = ink.max(line(px * 1.414) * smoothstep(0.28, 0.15, e));
            }
            let paper = [0.85, 0.83, 0.78];
            mix(paper, [0.02; 3], ink)
        };
        Some(mix(old, new, k))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn emboss_turns_flat_areas_grey_and_lights_one_side_of_a_step() {
        let (w, h) = (40, 40);
        let base = fill(w, h, |x, _| if x < 20 { [0.1; 3] } else { [0.6; 3] });
        let mut e = edit(Tool::Emboss, [0.5, 0.5, 0.8, 0.8]);
        e.tone = 0.0;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let flat = px(&out, w, 8, 20);
        assert!((flat[0] - 0.5_f32.powf(2.2)).abs() < 0.01, "{flat:?}");
        assert!(luma(px(&out, w, 20, 20)) > luma(flat) + 0.1);
    }

    #[test]
    fn crosshatch_gets_denser_in_the_dark() {
        let (w, h) = (80, 40);
        let base = fill(w, h, |x, _| if x < 40 { [0.03; 3] } else { [0.5; 3] });
        let mut e = edit(Tool::Crosshatch, [0.5, 0.5, 0.8, 0.8]);
        e.radius = 0.15;
        e.feather = 0.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let ink = |x0: usize| {
            (x0..x0 + 20)
                .flat_map(|x| (10..30).map(move |y| (x, y)))
                .filter(|&(x, y)| px(&out, w, x, y)[0] < 0.2)
                .count()
        };
        assert!(ink(8) > ink(50) * 2, "{} vs {}", ink(8), ink(50));
        // Even the darkest tone is hatched, not filled: paper shows between the lines.
        assert!(
            ink(8) < 20 * 20 * 3 / 4,
            "{} of 400 inked in the dark",
            ink(8)
        );
    }
}
