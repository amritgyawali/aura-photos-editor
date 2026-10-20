//! Portrait and scene tools: catchlight, eye glow, lip gloss, freckles, floor reflection and
//! drop shadow.
use super::{area, each, hash, mix, seed_of, smoothstep, weight, Frame, Snapshot};
use crate::bands::blur;
use crate::retouch_mask::Coverage;
use crate::retouch_planes::{luma, Rect};
use aura_recipe::retouch_tools::{Edit, Tool};

pub(super) fn supports(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Catchlight
            | Tool::EyeGlowFix
            | Tool::LipGloss
            | Tool::Freckles
            | Tool::FloorReflection
            | Tool::DropShadow
    )
}

pub(super) fn apply(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    match edit.tool {
        Tool::Catchlight => catchlight(rgb, frame, edit, coverage),
        Tool::EyeGlowFix => eye_glow(rgb, frame, edit, coverage),
        Tool::LipGloss => gloss(rgb, frame, edit, coverage),
        Tool::Freckles => freckles(rgb, frame, edit, coverage),
        Tool::FloorReflection => reflection(rgb, frame, edit, coverage),
        _ => shadow(rgb, frame, edit, coverage),
    }
}

fn catchlight(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    // As bright as the brightest thing already in the eye, and never dimmer than a clean
    // white: a catchlight is a reflection of the light, the brightest point in the eye.
    let mut peak = 0.0_f32;
    for y in rect.y0..rect.y0 + rect.h {
        for x in rect.x0..rect.x0 + rect.w {
            let i = (y * frame.width + x) * 3;
            peak = peak.max(luma([rgb[i], rgb[i + 1], rgb[i + 2]]));
        }
    }
    let level = peak.max(0.85);
    let light = edit
        .target_color
        .map_or([1.0; 3], crate::retouch_color::to_working)
        .map(|c| c * level);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        (k > 0.0).then(|| mix(old, light, k))
    });
}

fn eye_glow(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let pupil = 0.02 + (1.0 - edit.tone.clamp(0.0, 1.0)) * 0.06;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let top = old[0].max(old[1]).max(old[2]);
        let low = old[0].min(old[1]).min(old[2]);
        let saturation = if top > 1e-5 { (top - low) / top } else { 0.0 };
        // Glow: bright and very strongly coloured, in any hue - red in people, green or gold
        // in animals. Skin is coloured too, but in linear light it stays below about 0.65 on
        // this measure, and a tool placed slightly off the pupil must not darken it.
        let glow = smoothstep(0.12, 0.45, luma(old)).max(smoothstep(0.25, 0.6, top))
            * smoothstep(0.66, 0.82, saturation);
        if glow <= 0.0 {
            return None;
        }
        // Keep the eye's own shading by scaling rather than painting a flat disc.
        let scale = pupil / luma(old).max(1e-4);
        let dark = [luma(old) * scale; 3];
        Some(mix(old, dark, k * glow))
    });
}

fn gloss(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let radius = frame.radius_px(edit).max(2);
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let Some(source) = area(coverage, frame, radius * 3) else {
        return;
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let r = snapshot.rect();
    let lum = snapshot.luma_plane();
    // Pore-sized glints are texture, not a gloss highlight: compare a lightly smoothed lip with
    // its surroundings, so only highlights a third of the chosen size or larger are lifted.
    let lit = blur(&lum, r.w, r.h, (radius / 3).max(1));
    let soft = blur(&lum, r.w, r.h, radius);
    let lift = 0.4 + edit.tone.clamp(0.0, 1.0);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        let i = snapshot.index(x, y);
        // Where the lip is already lighter than around it: the highlights it has.
        let spec = smoothstep(0.04, 0.2, (lit[i] - soft[i]) / soft[i].max(1e-4));
        if spec <= 0.0 {
            return None;
        }
        let t = k * spec;
        let l = luma(old);
        let brighter = old.map(|v| v * (1.0 + lift * t));
        // A gloss highlight is the colour of the light, so it whitens as well as brightens.
        Some(mix(brighter, [l * (1.0 + lift * t); 3], t * 0.35))
    });
}

fn freckles(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let size = (edit.radius * frame.short()).max(1.5);
    let cell = (size * 3.0).ceil() as usize;
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    let density = edit.tone.clamp(0.0, 1.0) * 0.7;
    let seed = seed_of(edit);
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        if k <= 0.0 {
            return None;
        }
        // Each freckle lives in one grid cell; a pixel looks at its own cell and its eight
        // neighbours, so a freckle near a cell edge is drawn whole.
        let (cx, cy) = ((x / cell) as i64, (y / cell) as i64);
        let mut spot = 0.0_f32;
        for gy in cy - 1..=cy + 1 {
            for gx in cx - 1..=cx + 1 {
                if hash(gx, gy, seed) >= density {
                    continue;
                }
                let fx = (gx as f32 + hash(gx, gy, seed ^ 1)) * cell as f32;
                let fy = (gy as f32 + hash(gx, gy, seed ^ 2)) * cell as f32;
                let r = size * (0.35 + 0.45 * hash(gx, gy, seed ^ 3));
                let d = (x as f32 + 0.5 - fx).hypot(y as f32 + 0.5 - fy) / r;
                let shade = 0.6 + 0.4 * hash(gx, gy, seed ^ 4);
                spot = spot.max((1.0 - d * d).max(0.0).powi(2) * shade);
            }
        }
        if spot <= 0.0 {
            return None;
        }
        // Freckles are melanin: they darken blue most and red least.
        let t = spot * k * 0.6;
        Some([
            old[0] * (1.0 - t * 0.55),
            old[1] * (1.0 - t * 0.75),
            old[2] * (1.0 - t),
        ])
    });
}

fn reflection(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let Some(rect) = area(coverage, frame, 0) else {
        return;
    };
    // The mirror line is the selection's top edge; what is above it is reflected into it.
    let axis = rect.y0;
    if axis == 0 {
        return;
    }
    let depth = rect.h.min(axis);
    let source = Rect {
        x0: rect.x0,
        y0: axis - depth,
        w: rect.w,
        h: depth,
    };
    let snapshot = Snapshot::take(rgb, frame, source);
    let fade = (0.1 + edit.tone.clamp(0.0, 1.0) * 0.9) * rect.h as f32;
    each(rgb, frame, rect, |x, y, old| {
        let k = weight(edit, coverage, frame, x, y);
        let below = y - axis;
        if k <= 0.0 || below >= depth {
            return None;
        }
        let mirrored = snapshot.at(x as isize, (axis - 1 - below) as isize);
        let t = k * (-(below as f32) / fade).exp();
        Some(mix(old, mirrored, t))
    });
}

fn shadow(rgb: &mut [f32], frame: Frame, edit: &Edit, coverage: &Coverage) {
    let reach = frame.reach(edit);
    let offset = [
        (edit.warmth * reach[0]).round() as isize,
        (edit.tint * reach[1]).round() as isize,
    ];
    let softness = frame.radius_px(edit).max(1);
    let margin = offset[0].unsigned_abs().max(offset[1].unsigned_abs()) + softness * 3;
    // The shadow lands on everything around the selection, so the area is the selection
    // grown by how far the shadow is cast and blurred.
    let Some(rect) = area(coverage, frame, margin) else {
        return;
    };
    // The selection's shape, moved by the offset, softened.
    let shape: Vec<f32> = (0..rect.len())
        .map(|i| {
            let x = (rect.x0 + i % rect.w) as isize - offset[0];
            let y = (rect.y0 + i / rect.w) as isize - offset[1];
            if x < 0 || y < 0 || x >= frame.width as isize || y >= frame.height as isize {
                0.0
            } else {
                coverage.at(x as usize, y as usize, frame.width, frame.height)
            }
        })
        .collect();
    let cast = blur(&shape, rect.w, rect.h, softness);
    let density = 0.35 + edit.tone.clamp(0.0, 1.0) * 0.5;
    let strength = edit.amount.clamp(0.0, 1.0);
    each(rgb, frame, rect, |x, y, old| {
        let i = (y - rect.y0) * rect.w + (x - rect.x0);
        // Never on the subject itself.
        let open = 1.0 - coverage.at(x, y, frame.width, frame.height);
        let t = cast[i] * open * strength * density;
        (t > 0.0).then(|| old.map(|v| v * (1.0 - t)))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::super::testing::*;
    use crate::retouch_planes::luma;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn catchlight_brightens_the_centre_of_the_selection() {
        let (w, h) = (40, 40);
        let base = fill(w, h, |_, _| [0.04, 0.03, 0.03]);
        let mut e = edit(Tool::Catchlight, [0.5, 0.5, 0.05, 0.05]);
        e.feather = 0.5;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 20, 20)) > 0.6);
        assert_eq!(px(&out, w, 5, 5), px(&base, w, 5, 5));
    }

    #[test]
    fn eye_glow_darkens_a_glowing_pupil_but_not_a_dark_one() {
        let (w, h) = (30, 30);
        let base = fill(w, h, |x, y| {
            if y < 10 {
                [0.45, 0.25, 0.17] // bright skin, in linear light
            } else if x < 15 {
                [0.1, 0.8, 0.15] // green flash glow
            } else {
                [0.03, 0.03, 0.03]
            }
        });
        let e = edit(Tool::EyeGlowFix, [0.5, 0.5, 0.6, 0.6]);
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 7, 15)) < 0.1);
        assert_eq!(px(&out, w, 22, 15), px(&base, w, 22, 15));
        assert_eq!(
            px(&out, w, 15, 5),
            px(&base, w, 15, 5),
            "skin is not a glowing pupil"
        );
    }

    #[test]
    fn freckles_darken_some_skin_and_leave_most_of_it() {
        let (w, h) = (80, 80);
        let base = fill(w, h, |_, _| [0.6, 0.42, 0.33]);
        let mut e = edit(Tool::Freckles, [0.5, 0.5, 0.45, 0.45]);
        e.radius = 0.02;
        e.tone = 0.5;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let darker = out.chunks_exact(3).filter(|p| p[2] < 0.32).count();
        assert!(darker > 20 && darker < w * h / 2, "{darker}");
    }

    #[test]
    fn floor_reflection_mirrors_what_stands_above_it() {
        let (w, h) = (40, 40);
        // A red post standing on a grey floor that starts at row 20.
        let base = fill(w, h, |x, y| {
            if y < 20 && (18..22).contains(&x) {
                [0.8, 0.1, 0.1]
            } else {
                [0.3, 0.3, 0.3]
            }
        });
        let mut e = edit(Tool::FloorReflection, [0.5, 0.75, 0.6, 0.25]);
        e.feather = 0.0;
        e.tone = 1.0;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        let under = px(&out, w, 20, 22);
        assert!(
            under[0] > under[1] + 0.2,
            "the post shows in the floor: {under:?}"
        );
        assert_eq!(px(&out, w, 20, 10), px(&base, w, 20, 10));
    }

    #[test]
    fn drop_shadow_falls_beside_the_subject_and_not_on_it() {
        let (w, h) = (60, 60);
        let base = fill(w, h, |_, _| [0.5, 0.5, 0.5]);
        let mut e = edit(Tool::DropShadow, [0.4, 0.4, 0.1, 0.1]);
        e.feather = 0.0;
        e.warmth = 1.0;
        e.tint = 1.0;
        e.radius = 0.02;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        // Offset down and right by the region's radius.
        assert!(px(&out, w, 32, 32)[0] < 0.45);
        assert_eq!(px(&out, w, 24, 24), px(&base, w, 24, 24));
        assert_eq!(px(&out, w, 10, 50), px(&base, w, 10, 50));
    }

    #[test]
    fn lip_gloss_lifts_the_highlights_the_lip_has() {
        let (w, h) = (40, 40);
        let base = fill(w, h, |x, y| {
            if (18..22).contains(&x) && (18..20).contains(&y) {
                [0.5, 0.25, 0.25]
            } else {
                [0.35, 0.15, 0.15]
            }
        });
        let mut e = edit(Tool::LipGloss, [0.5, 0.5, 0.4, 0.4]);
        e.radius = 0.1;
        let mut out = base.clone();
        run(&mut out, w, h, &e);
        assert!(luma(px(&out, w, 20, 19)) > luma(px(&base, w, 20, 19)) * 1.2);
    }
}
