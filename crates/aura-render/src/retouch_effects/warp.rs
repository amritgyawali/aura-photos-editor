//! Liquify: push, bloat, pucker and twirl inside the selection. Each output pixel samples the
//! frame as it was at a displaced position. The displacement is the selection's coverage times
//! a falloff that reaches zero at the region's ellipse, so a warp never tears at its own edge
//! even with a hard-edged selection.
use super::{area, each, Frame, Snapshot};
use crate::retouch_mask::Coverage;
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::LiquifyPush | Tool::LiquifyBloat | Tool::LiquifyPucker | Tool::LiquifyTwirl
    )
}

/// How far a bloat or pucker moves a sample at full strength, as a share of its distance from
/// the centre.
const SCALE: f32 = 0.45;

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let centre = frame.centre(edit);
    let reach = frame.reach(edit);
    let longest = reach[0].max(reach[1]);
    let strength = edit.amount.clamp(0.0, 1.0);
    // The furthest any sample can be read from the pixel it lands on.
    let travel = match edit.tool {
        Tool::LiquifyPush => strength * reach[0].hypot(reach[1]),
        Tool::LiquifyPucker | Tool::LiquifyBloat => SCALE * strength * longest,
        _ => std::f32::consts::PI * longest,
    };
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, travel.ceil() as usize + 2) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    each(rgb, frame, rect, |x, y, _| {
        let cover = coverage.at(x, y, frame.width, frame.height);
        if cover <= 0.0 {
            return None;
        }
        let px = x as f32 + 0.5;
        let py = y as f32 + 0.5;
        let dx = px - centre[0];
        let dy = py - centre[1];
        let d = (dx / reach[0]).hypot(dy / reach[1]);
        if d >= 1.0 {
            return None;
        }
        let falloff = (1.0 - d * d).powi(2) * cover;
        let (sx, sy) = match edit.tool {
            Tool::LiquifyPush => (
                px - edit.warmth * reach[0] * strength * falloff,
                py - edit.tint * reach[1] * strength * falloff,
            ),
            Tool::LiquifyBloat => {
                let k = 1.0 - SCALE * strength * falloff;
                (centre[0] + dx * k, centre[1] + dy * k)
            }
            Tool::LiquifyPucker => {
                let k = 1.0 + SCALE * strength * falloff;
                (centre[0] + dx * k, centre[1] + dy * k)
            }
            _ => {
                let angle = -edit.warmth * std::f32::consts::PI * strength * falloff;
                let (s, c) = angle.sin_cos();
                (centre[0] + dx * c - dy * s, centre[1] + dx * s + dy * c)
            }
        };
        Some(snapshot.bilinear(sx, sy))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use aura_recipe::retouch_tools::Tool;

    /// A dark disc on a light ground: how many pixels are dark, and where their centroid is.
    fn disc(w: usize, h: usize, r: f32) -> Vec<f32> {
        fill(w, h, |x, y| {
            let d = (x as f32 + 0.5 - w as f32 / 2.0).hypot(y as f32 + 0.5 - h as f32 / 2.0);
            if d < r {
                [0.02, 0.02, 0.02]
            } else {
                [0.6, 0.6, 0.6]
            }
        })
    }

    fn dark(rgb: &[f32], w: usize) -> (usize, f32, f32) {
        let mut n = 0;
        let (mut sx, mut sy) = (0.0, 0.0);
        for (i, p) in rgb.chunks_exact(3).enumerate() {
            if p[0] < 0.3 {
                n += 1;
                sx += (i % w) as f32;
                sy += (i / w) as f32;
            }
        }
        (n, sx / n.max(1) as f32, sy / n.max(1) as f32)
    }

    #[test]
    fn bloat_enlarges_and_pucker_shrinks_what_is_at_the_centre() {
        let (w, h) = (80, 80);
        let before = dark(&disc(w, h, 10.0), w).0;
        let mut bloated = disc(w, h, 10.0);
        let mut e = edit(Tool::LiquifyBloat, [0.5, 0.5, 0.4, 0.4]);
        e.amount = 0.8;
        run(&mut bloated, w, h, &e);
        let mut puckered = disc(w, h, 10.0);
        e.tool = Tool::LiquifyPucker;
        run(&mut puckered, w, h, &e);
        let grown = dark(&bloated, w).0;
        let shrunk = dark(&puckered, w).0;
        assert!(grown as f32 > before as f32 * 1.3, "{before} -> {grown}");
        assert!(
            (shrunk as f32) < before as f32 * 0.8,
            "{before} -> {shrunk}"
        );
    }

    #[test]
    fn push_moves_content_the_way_it_is_pushed() {
        let (w, h) = (80, 80);
        let mut pushed = disc(w, h, 8.0);
        let mut e = edit(Tool::LiquifyPush, [0.5, 0.5, 0.4, 0.4]);
        e.warmth = 0.5;
        e.tint = -0.3;
        e.amount = 0.6;
        run(&mut pushed, w, h, &e);
        let (_, cx, cy) = dark(&pushed, w);
        assert!(cx > 41.0, "pushed right: {cx}");
        assert!(cy < 39.0, "pushed up: {cy}");
    }

    #[test]
    fn twirl_keeps_a_centred_disc_where_it_was() {
        let (w, h) = (80, 80);
        let mut twirled = disc(w, h, 10.0);
        let mut e = edit(Tool::LiquifyTwirl, [0.5, 0.5, 0.4, 0.4]);
        e.warmth = 0.7;
        run(&mut twirled, w, h, &e);
        let before = dark(&disc(w, h, 10.0), w);
        let after = dark(&twirled, w);
        assert!((after.0 as f32 - before.0 as f32).abs() < before.0 as f32 * 0.08);
        assert!((after.1 - before.1).abs() < 1.0 && (after.2 - before.2).abs() < 1.0);
    }

    #[test]
    fn a_hard_edged_warp_has_no_seam_at_its_ellipse() {
        let (w, h) = (80, 80);
        let mut e = edit(Tool::LiquifyBloat, [0.5, 0.5, 0.3, 0.3]);
        e.feather = 0.0;
        let before = checker(w, h, 3);
        let mut after = before.clone();
        run(&mut after, w, h, &e);
        // Just inside the ellipse the falloff is near zero, so the pixel barely moves.
        let x = (0.5 * w as f32 + 0.29 * w as f32) as usize;
        let y = h / 2;
        let a = px(&before, w, x, y);
        let b = px(&after, w, x, y);
        assert!((a[0] - b[0]).abs() < 0.12, "{a:?} {b:?}");
    }
}
