//! Defringe: the purple and green rims along high-contrast edges, measured. ADR-0125.
//!
//! Axial chromatic aberration and sensor bloom leave a coloured rim on the dark side of a bright
//! edge - purple or magenta in front of the plane of focus, green behind it: a branch against a
//! bright sky, the edge of a backlit veil, a dark suit beside a white shirt. Unlike the lateral
//! kind it is not a shift a resample can undo, so a raw converter removes it by colour, and the
//! risk is the same as a raw converter's: a purple dress, a magenta flower or the glow of a purple
//! uplight is purple too. Three measurements decide what counts as a rim, and one choice makes a
//! wrong one cheap:
//!
//! - **Beside a bright area.** A power mean of the luminance around the pixel leans toward the
//!   brightest of the neighbourhood; a rim is a pixel several times darker than that. A pixel in
//!   the middle of a colour, or on the bright side of an edge, is not one.
//! - **In the fringe's colour, and not in the colour of its calm surroundings.** A rim is a few
//!   pixels wide. The surroundings away from the edge are read as well, and where they are that
//!   same colour - a purple dress, the inside of a flower - the colour is the object's and is
//!   kept.
//! - **Beside a neutral light.** Lens fringes form round white and near-white highlights. When
//!   the bright area beside the pixel is itself purple or green - an LED uplight, a neon sign -
//!   the colour round it is its glow and is kept.
//! - **Moved to its surroundings' colour, never to grey.** A rim pixel takes the colour of the
//!   calm, unfringed pixels around it, at its own luminance. So a pixel that was selected but is
//!   really the object's colour moves to the object's colour: nearly nowhere.
//!
//! `warmth` is how much of a purple rim is removed and `tint` how much of a green one, each
//! `0..=1`; `radius` is how far from the bright area a rim may reach; `amount` scales both.
//! Nothing is generated and no luminance moves.

// Every plane covers one bounded rectangle and every index is derived from it.
#![allow(clippy::indexing_slicing)]

use crate::retouch_mask::Coverage;
use crate::retouch_planes::{blur, luma, smoothstep, Rect};
use aura_recipe::retouch_tools::Edit;
use rayon::prelude::*;

/// Hue bands, in degrees of the working space's hexcone hue: violet to magenta, and green.
const PURPLE: [f32; 2] = [255.0, 325.0];
const GREEN: [f32; 2] = [80.0, 150.0];
/// How far either side of a band its membership fades out, in degrees.
const FALLOFF: f32 = 15.0;
/// Below the first chroma (max minus min over max) a pixel has no colour to correct; from the
/// second it is fully coloured.
const CHROMA: [f32; 2] = [0.06, 0.20];
/// The nearby bright area over the pixel's own light: from the first ratio a pixel is beside a
/// high-contrast edge, fully from the second.
const CONTRAST: [f32; 2] = [2.0, 5.0];
/// Calm surroundings that are this much of the fringe's colour are an object in it:
/// protection starts at the first share and is complete at the second.
const CONTENT: [f32; 2] = [0.15, 0.40];
/// A change smaller than this in chromaticity (each channel over luminance) is not counted as
/// a rim removed when the operation is measured.
const NOTICEABLE: f32 = 0.12;

/// Hexcone hue in degrees and chroma over the largest channel.
fn hue(p: [f32; 3]) -> (f32, f32) {
    let max = p[0].max(p[1]).max(p[2]);
    let min = p[0].min(p[1]).min(p[2]);
    let c = max - min;
    if max <= 1e-6 || c <= 1e-7 {
        return (0.0, 0.0);
    }
    let h = if p[0] >= p[1] && p[0] >= p[2] {
        60.0 * ((p[1] - p[2]) / c).rem_euclid(6.0)
    } else if p[1] >= p[2] {
        60.0 * ((p[2] - p[0]) / c + 2.0)
    } else {
        60.0 * ((p[0] - p[1]) / c + 4.0)
    };
    (h, c / max)
}

/// Membership of hue `h` in a band, with a smooth falloff either side.
fn band(h: f32, [low, high]: [f32; 2]) -> f32 {
    let distance = if h < low {
        low - h
    } else if h > high {
        h - high
    } else {
        0.0
    };
    1.0 - smoothstep(0.0, FALLOFF, distance)
}

/// How much a colour is a fringe's colour, weighted by how much colour it has at all.
fn fringe_colour(p: [f32; 3], purple: f32, green: f32) -> f32 {
    let (h, s) = hue(p);
    let coloured = smoothstep(CHROMA[0], CHROMA[1], s);
    ((purple * band(h, PURPLE)).max(green * band(h, GREEN)) * coloured).clamp(0.0, 1.0)
}

/// The coarse grid the neighbourhood readings are taken on: every reading but the pixel's own
/// colour and light is a mean over `reach` pixels or more, so it is measured on cells a third of
/// that across and read back bilinearly. A 24-megapixel export would otherwise blur a dozen
/// full-size planes for an operation that changes a few thousand pixels.
#[derive(Debug, Clone, Copy)]
struct Grid {
    /// Pixels a cell stands for, on each side.
    f: usize,
    w: usize,
    h: usize,
}

impl Grid {
    fn new(rect: Rect, reach: usize) -> Self {
        let f = (reach / 3).clamp(1, 8);
        Self {
            f,
            w: rect.w.div_ceil(f),
            h: rect.h.div_ceil(f),
        }
    }

    /// Every cell's mean of `N` readings over the pixels it covers; `at` reads one pixel of
    /// the rectangle by its index there. Cells are independent, so they are shared across cores.
    fn pool<const N: usize>(
        self,
        rect: Rect,
        at: impl Fn(usize) -> [f32; N] + Sync,
    ) -> [Vec<f32>; N] {
        let cells: Vec<[f32; N]> = (0..self.w * self.h)
            .into_par_iter()
            .map(|cell| {
                let (cx, cy) = (cell % self.w, cell / self.w);
                let (x0, y0) = (cx * self.f, cy * self.f);
                let (x1, y1) = ((x0 + self.f).min(rect.w), (y0 + self.f).min(rect.h));
                let mut sum = [0.0_f32; N];
                for y in y0..y1 {
                    for x in x0..x1 {
                        for (total, v) in sum.iter_mut().zip(at(y * rect.w + x)) {
                            *total += v;
                        }
                    }
                }
                let n = ((x1 - x0) * (y1 - y0)).max(1) as f32;
                sum.map(|v| v / n)
            })
            .collect();
        std::array::from_fn(|k| cells.iter().map(|c| c[k]).collect())
    }

    /// A smooth plane on this grid, at pixel `(x, y)` of the rectangle, between cell centres.
    fn sample(self, plane: &[f32], x: usize, y: usize) -> f32 {
        let place = |v: usize, cells: usize| -> (usize, usize, f32) {
            let u = ((v as f32 + 0.5) / self.f as f32 - 0.5).clamp(0.0, (cells - 1) as f32);
            let lo = u.floor() as usize;
            (lo, (lo + 1).min(cells - 1), u - lo as f32)
        };
        let (x0, x1, tx) = place(x, self.w);
        let (y0, y1, ty) = place(y, self.h);
        let top = plane[y0 * self.w + x0] * (1.0 - tx) + plane[y0 * self.w + x1] * tx;
        let bottom = plane[y1 * self.w + x0] * (1.0 - tx) + plane[y1 * self.w + x1] * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// What a defringe would do: every rim pixel of the frame it would move, with the share of
/// the change (before the operation's amount and selection) and the colour it moves to.
pub(crate) struct Fringes {
    pub changes: Vec<(usize, f32, [f32; 3])>,
}

/// Find the rims within `bounds` (grown by what the measurements read), `reach` pixels from a
/// bright area. `None` when nothing in it is a rim.
#[allow(clippy::too_many_lines)]
pub(crate) fn find(
    rgb: &[f32],
    w: usize,
    h: usize,
    bounds: [usize; 4],
    reach: usize,
    purple: f32,
    green: f32,
) -> Option<Fringes> {
    if purple <= 0.0 && green <= 0.0 {
        return None;
    }
    let reach = reach.max(1);
    let rect = Rect::around(bounds, reach * 6 + 2, w, h)?;
    let frame_index = |i: usize| (rect.y0 + i / rect.w) * w + rect.x0 + i % rect.w;
    let pixel = |i: usize| -> [f32; 3] {
        let at = frame_index(i) * 3;
        [rgb[at].max(0.0), rgb[at + 1].max(0.0), rgb[at + 2].max(0.0)]
    };
    let light_of = |p: [f32; 3]| luma(p).max(0.0);
    let grid = Grid::new(rect, reach);
    let near = (reach as f32 / grid.f as f32).round().max(1.0) as usize;
    let wide = (2.0 * reach as f32 / grid.f as f32).round().max(1.0) as usize;

    // The brightest of the neighbourhood, by a fourth-power mean, and its colour.
    let [fourth, red, green_light, blue] = grid.pool(rect, |i| {
        let p = pixel(i);
        let l = light_of(p);
        let q = (l * l) * (l * l);
        let k = q / l.max(1e-6);
        [q, k * p[0], k * p[1], k * p[2]]
    });
    let fourth = blur(&fourth, grid.w, grid.h, near);
    let light = [red, green_light, blue].map(|plane| blur(&plane, grid.w, grid.h, near));
    let edge_at = |i: usize, l: f32| -> f32 {
        let bright = grid
            .sample(&fourth, i % rect.w, i / rect.w)
            .max(0.0)
            .powf(0.25);
        smoothstep(CONTRAST[0], CONTRAST[1], bright / (l + 0.002))
    };

    // The calm surroundings, away from any edge, read from the rim's own side: a rim is the
    // dark side of its edge, so every calm pixel counts by how dark it is (an inverse square of
    // its light). Read evenly, the bright side - a window, a sky - outweighs a purple wall's
    // calm interior and the wall's own edge reads as a rim.
    let [calm, dark_calm, dark_member, clean_lum, clean_r, clean_g, clean_b] =
        grid.pool(rect, |i| {
            let p = pixel(i);
            let l = light_of(p);
            let member = fringe_colour(p, purple, green);
            let calm = 1.0 - edge_at(i, l);
            let dark = calm * (l + 0.01).powi(-2);
            let clean = dark * (1.0 - member);
            [
                calm,
                dark,
                dark * member,
                clean * l,
                clean * p[0],
                clean * p[1],
                clean * p[2],
            ]
        });
    let wide_blur = |plane: &[f32]| blur(plane, grid.w, grid.h, wide);
    let (calm, dark_calm, dark_member, clean_lum) = (
        wide_blur(&calm),
        wide_blur(&dark_calm),
        wide_blur(&dark_member),
        wide_blur(&clean_lum),
    );
    let clean = [clean_r, clean_g, clean_b].map(|plane| wide_blur(&plane));

    let changes: Vec<(usize, f32, [f32; 3])> = (0..rect.h)
        .into_par_iter()
        .flat_map_iter(|row| {
            let mut found = Vec::new();
            for column in 0..rect.w {
                let i = row * rect.w + column;
                let p = pixel(i);
                let member = fringe_colour(p, purple, green);
                if member <= 0.0 {
                    continue;
                }
                let l = light_of(p);
                let edge = edge_at(i, l);
                if edge <= 0.0 {
                    continue;
                }
                let at = |plane: &[f32]| grid.sample(plane, column, row);
                let dark_total = at(&dark_calm);
                let content = if dark_total > 1e-6 {
                    at(&dark_member) / dark_total
                } else {
                    0.0
                };
                let bright_total = at(&fourth);
                let source = if bright_total > 1e-12 {
                    fringe_colour(
                        light.each_ref().map(|c| at(c) / bright_total),
                        purple,
                        green,
                    )
                } else {
                    0.0
                };
                let weight = edge
                    * member
                    * (1.0 - smoothstep(CONTENT[0], CONTENT[1], content))
                    * (1.0 - source);
                if weight <= 0.0 {
                    continue;
                }
                // The colour of the calm, unfringed pixels on the rim's side, at this pixel's
                // own luminance - trusted as far as such pixels are as dark as the rim. A twig
                // against a bright sky has no calm side of its own, and goes grey rather than
                // sky blue.
                let calm_total = at(&calm);
                let similar = if calm_total > 1e-6 {
                    dark_total / calm_total * (l + 0.01).powi(2)
                } else {
                    0.0
                };
                let trust = smoothstep(0.05, 0.25, similar);
                let lum_total = at(&clean_lum);
                let ratio = if lum_total > 1e-6 {
                    clean.each_ref().map(|c| at(c) / lum_total)
                } else {
                    [1.0; 3]
                };
                found.push((
                    frame_index(i),
                    weight,
                    ratio.map(|r| ((1.0 + trust * (r - 1.0)) * l).max(0.0)),
                ));
            }
            found
        })
        .collect();
    (!changes.is_empty()).then_some(Fringes { changes })
}

/// Remove the rims inside the selection.
pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    let reach = (edit.radius * w.min(h) as f32).round().max(1.0) as usize;
    let (purple, green) = (edit.warmth.clamp(0.0, 1.0), edit.tint.clamp(0.0, 1.0));
    if edit.amount <= 0.0 {
        return;
    }
    let Some(found) = find(rgb, w, h, coverage.bounds, reach, purple, green) else {
        return;
    };
    let writes: Vec<(usize, [f32; 3])> = found
        .changes
        .par_iter()
        .filter_map(|(at, weight, new)| {
            let share = weight * edit.amount * coverage.at(at % w, at / w, w, h);
            (share > 0.0).then(|| {
                let old = [rgb[at * 3], rgb[at * 3 + 1], rgb[at * 3 + 2]];
                (
                    *at,
                    std::array::from_fn(|c| old[c] + share * (new[c] - old[c])),
                )
            })
        })
        .collect();
    for (at, value) in writes {
        rgb[at * 3..at * 3 + 3].copy_from_slice(&value);
    }
}

/// How many pixels of the whole frame a defringe would visibly change: moved at least half
/// way, by a noticeable change of colour. Measured exactly as [`apply`] decides, before the
/// operation's amount.
pub(crate) fn rims(
    rgb: &[f32],
    w: usize,
    h: usize,
    reach: usize,
    purple: f32,
    green: f32,
) -> usize {
    let Some(found) = find(rgb, w, h, [0, 0, w, h], reach, purple, green) else {
        return 0;
    };
    found
        .changes
        .par_iter()
        .filter(|(at, weight, new)| {
            let old = [rgb[at * 3], rgb[at * 3 + 1], rgb[at * 3 + 2]];
            let (lo, ln) = (luma(old).max(1e-6), luma(*new).max(1e-6));
            let change = (0..3)
                .map(|c| (old[c] / lo - new[c] / ln).abs())
                .fold(0.0_f32, f32::max);
            *weight >= 0.5 && change >= NOTICEABLE
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dark scene with a bright neutral window, and round its edge `rim` pixels of colour
    /// `fringe` on the dark side, as a lens leaves them.
    fn window(w: usize, h: usize, rim: usize, fringe: Option<[f32; 3]>) -> Vec<f32> {
        let (x0, x1, y0, y1) = (w / 3, 2 * w / 3, h / 3, 2 * h / 3);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let inside = (x0..x1).contains(&x) && (y0..y1).contains(&y);
                let outside_by = [
                    x0.saturating_sub(x),
                    x.saturating_sub(x1 - 1),
                    y0.saturating_sub(y),
                    y.saturating_sub(y1 - 1),
                ]
                .into_iter()
                .max()
                .unwrap_or(0);
                let p = if inside {
                    [0.9, 0.9, 0.9]
                } else if outside_by <= rim {
                    fringe.unwrap_or([0.04, 0.04, 0.04])
                } else {
                    [0.04, 0.04, 0.04]
                };
                rgb.extend(p);
            }
        }
        rgb
    }

    fn edit(purple: f32, green: f32) -> Edit {
        let mut e: Edit = serde_json::from_value(serde_json::json!({
            "id": "defringe", "tool": "defringe", "enabled": true,
            "region": [0.5, 0.5, 0.5, 0.5], "source": null, "amount": 1.0, "feather": 0.0,
            "radius": 0.02, "texture": 1.0, "tone": 0.5, "warmth": 0.0, "tint": 0.0
        }))
        .unwrap();
        e.warmth = purple;
        e.tint = green;
        e
    }

    fn apply_to(rgb: &[f32], w: usize, h: usize, e: &Edit) -> Vec<f32> {
        let mut out = rgb.to_vec();
        let coverage = Coverage::new(e, w, h);
        apply(&mut out, w, h, e, &coverage);
        out
    }

    fn chroma(p: &[f32]) -> f32 {
        hue([p[0], p[1], p[2]]).1
    }

    #[test]
    fn a_purple_rim_beside_a_bright_window_is_removed_and_its_light_kept() {
        let (w, h) = (180, 120);
        let rgb = window(w, h, 2, Some([0.07, 0.03, 0.09]));
        let out = apply_to(&rgb, w, h, &edit(1.0, 0.0));
        // On the rim, just outside the window's left edge.
        let at = ((h / 2) * w + w / 3 - 1) * 3;
        assert!(chroma(&rgb[at..at + 3]) > 0.5);
        assert!(chroma(&out[at..at + 3]) < 0.1, "{:?}", &out[at..at + 3]);
        let (before, after) = (
            luma([rgb[at], rgb[at + 1], rgb[at + 2]]),
            luma([out[at], out[at + 1], out[at + 2]]),
        );
        assert!(
            (before - after).abs() < 1e-4,
            "luminance moved: {before} -> {after}"
        );
        assert!(rims(&rgb, w, h, 4, 1.0, 0.0) > 100);
        assert_eq!(rims(&out, w, h, 4, 1.0, 0.0), 0);
    }

    #[test]
    fn green_is_removed_only_when_asked() {
        let (w, h) = (180, 120);
        let rgb = window(w, h, 2, Some([0.03, 0.08, 0.035]));
        let at = ((h / 2) * w + w / 3 - 1) * 3;
        let purple_only = apply_to(&rgb, w, h, &edit(1.0, 0.0));
        assert_eq!(&purple_only[at..at + 3], &rgb[at..at + 3]);
        let both = apply_to(&rgb, w, h, &edit(1.0, 1.0));
        assert!(chroma(&both[at..at + 3]) < 0.1, "{:?}", &both[at..at + 3]);
    }

    #[test]
    fn a_purple_object_beside_a_bright_window_keeps_its_colour() {
        // A purple wall round the window: purple in its calm interior, so purple at its edge
        // is the wall's.
        let (w, h) = (180, 120);
        let wall = [0.07, 0.03, 0.09];
        let rgb: Vec<f32> = window(w, h, 0, None)
            .chunks_exact(3)
            .flat_map(|p| if p[0] > 0.5 { [p[0], p[1], p[2]] } else { wall })
            .collect();
        let out = apply_to(&rgb, w, h, &edit(1.0, 1.0));
        let worst = rgb
            .iter()
            .zip(&out)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(worst < 0.004, "a purple wall changed by {worst}");
        assert_eq!(rims(&rgb, w, h, 4, 1.0, 1.0), 0);
    }

    #[test]
    fn the_glow_round_a_purple_light_is_kept() {
        // A purple uplight: the bright area is purple itself, and so is the glow round it.
        let (w, h) = (180, 120);
        let rgb: Vec<f32> = window(w, h, 3, Some([0.07, 0.03, 0.09]))
            .chunks_exact(3)
            .flat_map(|p| {
                if p[0] > 0.5 {
                    [0.8, 0.3, 0.9]
                } else {
                    [p[0], p[1], p[2]]
                }
            })
            .collect();
        let out = apply_to(&rgb, w, h, &edit(1.0, 0.0));
        let at = ((h / 2) * w + w / 3 - 2) * 3;
        assert!(chroma(&out[at..at + 3]) > 0.4, "{:?}", &out[at..at + 3]);
    }

    #[test]
    fn a_thin_twig_against_a_bright_sky_goes_grey_not_blue() {
        // A twig three pixels wide is all rim: there is no calm twig to take a colour from, and
        // the sky beside it is not the twig's colour.
        let (w, h) = (180, 120);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for _ in 0..h {
            for x in 0..w {
                rgb.extend(if (89..92).contains(&x) {
                    [0.07, 0.03, 0.09]
                } else {
                    [0.35, 0.5, 0.75]
                });
            }
        }
        let out = apply_to(&rgb, w, h, &edit(1.0, 0.0));
        let at = ((h / 2) * w + 90) * 3;
        assert!(chroma(&out[at..at + 3]) < 0.12, "{:?}", &out[at..at + 3]);
        // The sky is not a rim and is left exactly as it was.
        let sky = ((h / 2) * w + 40) * 3;
        assert_eq!(&out[sky..sky + 3], &rgb[sky..sky + 3]);
    }

    /// The same three decisions on a large frame, where the neighbourhood is read on a grid
    /// four times coarser than the pixels: a rim is removed, a purple wall and a purple
    /// light's glow are kept.
    #[test]
    fn on_a_coarse_grid_rims_go_and_purple_things_stay() {
        let (w, h) = (720, 480);
        let mut e = edit(1.0, 0.0);
        e.radius = 0.025;
        let reach = (e.radius * h as f32).round() as usize;
        assert_eq!(Grid::new(Rect { x0: 0, y0: 0, w, h }, reach).f, 4);
        let at = ((h / 2) * w + w / 3 - 3) * 3;

        let rimmed = window(w, h, 6, Some([0.07, 0.03, 0.09]));
        let out = apply_to(&rimmed, w, h, &e);
        assert!(chroma(&out[at..at + 3]) < 0.1, "{:?}", &out[at..at + 3]);

        let wall = [0.07, 0.03, 0.09];
        let walled: Vec<f32> = window(w, h, 0, None)
            .chunks_exact(3)
            .flat_map(|p| if p[0] > 0.5 { [p[0], p[1], p[2]] } else { wall })
            .collect();
        let out = apply_to(&walled, w, h, &e);
        let worst = walled
            .iter()
            .zip(&out)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(worst < 0.004, "a purple wall changed by {worst}");

        let lit: Vec<f32> = rimmed
            .chunks_exact(3)
            .flat_map(|p| {
                if p[0] > 0.5 {
                    [0.8, 0.3, 0.9]
                } else {
                    [p[0], p[1], p[2]]
                }
            })
            .collect();
        let out = apply_to(&lit, w, h, &e);
        assert!(chroma(&out[at..at + 3]) > 0.4, "{:?}", &out[at..at + 3]);
    }

    /// A whole 24-megapixel frame with rims, as an export renders it. By hand:
    /// `cargo test --release -p aura-render --lib -- retouch_defringe::tests::timing --ignored --nocapture`.
    #[test]
    #[ignore = "timing, by hand"]
    #[allow(clippy::disallowed_methods, clippy::print_stdout)]
    fn timing() {
        let (w, h) = (6000, 4000);
        let rgb = window(w, h, 4, Some([0.07, 0.03, 0.09]));
        let mut e = edit(1.0, 0.0);
        e.radius = 0.003;
        let started = std::time::Instant::now();
        let out = apply_to(&rgb, w, h, &e);
        println!("defringe over {w}x{h}: {:?}", started.elapsed());
        assert_ne!(out, rgb);
    }

    #[test]
    fn a_neutral_edge_and_skin_beside_it_are_untouched() {
        let (w, h) = (180, 120);
        let neutral = window(w, h, 0, None);
        assert_eq!(apply_to(&neutral, w, h, &edit(1.0, 1.0)), neutral);
        // Skin beside a white collar: warm, not purple or green.
        let skin = window(w, h, 3, Some([0.30, 0.18, 0.12]));
        assert_eq!(apply_to(&skin, w, h, &edit(1.0, 1.0)), skin);
    }
}
