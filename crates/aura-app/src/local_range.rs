//! Shadows and Highlights sized from the photograph's regions. ADR-0126.
//!
//! The renderer's Shadows and Highlights act on regions since ADR-0126: a lifted shadow keeps
//! its texture and a recovered sky keeps its white clouds white. That makes them safe to use
//! with a firmer hand than a per-pixel curve allowed - but only where the photograph asks for
//! it, and the question has to be put to the same regions the sliders act on. A histogram
//! cannot tell a dark interior from a black cat, and it cannot tell a bright sky with detail
//! from a blown one.
//!
//! So the frame is measured through the renderer's own tone base
//! ([`aura_render::edge_aware::regions`]) and the two sliders are sized from what it reports:
//!
//! - **Shadows** grow with how much of the frame is in deep-shadow regions and how deep they
//!   are. Regions that are black - a studio backdrop, a graphic, a night sky - carry nothing to
//!   lift and are left black.
//! - **Highlights** grow with how much of the frame is in bright regions that still carry
//!   detail and how close to white they come. Clipped regions are not counted: there is
//!   nothing in them to recover, and the renderer protects them anyway.
//!
//! The scene has the last word. A night scene keeps its darkness and a high-key frame keeps its
//! white, for the reasons [`crate::smart_edit::respect_intent`] gives.
#![allow(
    // Pixel counts and shares become floating point and slider values come back as integers;
    // every one of them is bounded long before a cast could lose anything.
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use aura_render::edge_aware::{self, Regions};

use crate::portrait_features::Pixels;
use crate::smart_edit::SceneKind;

/// Long edge of the frame the regions are measured on: the renderer's own base grid.
pub const MEASURE_EDGE: usize = edge_aware::BASE_EDGE;

/// The Shadows a minority of shadow regions may ask for: dark corners, a dark background,
/// black fur. Enough to open them up; not so much that a dark background stops being one.
pub const MINORITY_SHADOWS: f32 = 35.0;
/// The further Shadows a frame that is *mostly* shadow may ask for: an interior exposed for
/// its windows, a backlit scene exposed for the sky.
pub const MAJORITY_SHADOWS: f32 = 35.0;
/// The most Shadows the regions alone may ask for.
pub const MOST_SHADOWS: f32 = MINORITY_SHADOWS + MAJORITY_SHADOWS;
/// The most Highlights recovery the regions alone may ask for.
pub const MOST_HIGHLIGHTS: f32 = 40.0;

/// Shadow regions covering less of the frame than this ask for nothing: a few dark corners
/// are contrast, not a problem.
pub const FEWEST_SHADOWS: f32 = 0.03;
/// Shadow regions covering this share of the frame ask for the whole minority amount.
pub const FULL_SHADOWS: f32 = 0.25;
/// A frame begins to count as mostly shadow when its shadow regions cover this share, and
/// counts fully at [`MOSTLY_SHADOW_FULL`].
pub const MOSTLY_SHADOW_FROM: f32 = 0.3;
/// See [`MOSTLY_SHADOW_FROM`].
pub const MOSTLY_SHADOW_FULL: f32 = 0.7;
/// Shadow regions this many stops below middle grey begin to ask for a stronger lift, and
/// regions two stops further down ask for all of it.
pub const SHADOW_DEPTH_FROM: f32 = -3.0;
/// Regions darker than this are black by design and carry nothing to lift.
pub const BLACK_BY_DESIGN: f32 = -8.5;

/// Bright regions with detail covering this share of the frame ask for the full recovery.
pub const FULL_HIGHLIGHTS: f32 = 0.15;
/// Bright regions covering more than this share are the photograph's own brightness - a
/// white building, a bright room - rather than a sky or a window above it, and ask for less:
/// pulling the subject itself down only turns it grey. At [`DOMINANT_HIGHLIGHTS_FULL`] they
/// ask for [`DOMINANT_HIGHLIGHTS_KEEP`] of the recovery.
pub const DOMINANT_HIGHLIGHTS_FROM: f32 = 0.3;
/// See [`DOMINANT_HIGHLIGHTS_FROM`].
pub const DOMINANT_HIGHLIGHTS_FULL: f32 = 0.6;
/// See [`DOMINANT_HIGHLIGHTS_FROM`].
pub const DOMINANT_HIGHLIGHTS_KEEP: f32 = 0.4;
/// The brightest regions (97th percentile) begin to ask for recovery this many stops above
/// middle grey - about sRGB 88% - and ask for all of it 1.2 stops further up.
pub const HIGHLIGHT_FROM: f32 = 1.8;

/// The most Highlights a high-key frame is given, as in [`crate::smart_edit`].
const HIGH_KEY_HIGHLIGHTS: i16 = -8;

/// Shadows and Highlights the regions ask for.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RangePlan {
    /// The Shadows lift the regions ask for, `0 ..= 70`.
    pub shadows: i16,
    /// The Highlights recovery the regions ask for, `-40 ..= 0`.
    pub highlights: i16,
    /// What they were sized from.
    pub regions: Regions,
}

impl RangePlan {
    /// True when the regions asked for nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.shadows == 0 && self.highlights == 0
    }

    /// The Highlights and Shadows a frame ends with, given what the histogram asked for.
    ///
    /// Each slider takes the stronger of the two answers rather than their sum: the histogram
    /// and the regions are two readings of the same light, and adding them would correct it
    /// twice.
    #[must_use]
    pub fn merged(&self, highlights: i16, shadows: i16) -> (i16, i16) {
        (highlights.min(self.highlights), shadows.max(self.shadows))
    }

    /// The decision in words, when there is one.
    #[must_use]
    pub fn sentence(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let r = &self.regions;
        let mut parts = Vec::new();
        if self.shadows > 0 {
            parts.push(format!(
                "Shadows {:+}: {:.0}% of the frame is in shadow regions about {:.1} stops below middle grey",
                self.shadows,
                r.dark_share * 100.0,
                -r.dark_depth
            ));
        }
        if self.highlights < 0 {
            parts.push(format!(
                "Highlights {:+}: {:.0}% is in bright regions that still carry detail, reaching {:.1} stops above it",
                self.highlights,
                r.bright_share * 100.0,
                r.high
            ));
        }
        Some(format!(
            "Sized from the frame's regions - {}. Both act on whole regions, so the texture inside them is kept{}.",
            parts.join("; "),
            if r.clipped_share > 0.01 {
                format!(
                    ", and the {:.0}% that is clipped is left white",
                    r.clipped_share * 100.0
                )
            } else {
                String::new()
            }
        ))
    }
}

/// A frame for the measurement: the preview reduced to [`MEASURE_EDGE`] in linear light and
/// brought into the working space the way the renderer brings a display-referred file in.
#[must_use]
pub fn working_frame(px: &Pixels<'_>) -> (Vec<f32>, usize, usize) {
    let (width, height) = (px.width, px.height);
    let long = width.max(height).max(1);
    let scale = (MEASURE_EDGE as f64 / long as f64).min(1.0);
    let w = ((width as f64 * scale).round() as usize).max(1);
    let h = ((height as f64 * scale).round() as usize).max(1);
    let bounds = |i: usize, out: usize, full: usize| -> (usize, usize) {
        let start = i * full / out;
        (start, ((i + 1) * full / out).max(start + 1).min(full))
    };
    let mut out = Vec::with_capacity(w * h * 3);
    for oy in 0..h {
        let (y0, y1) = bounds(oy, h, height);
        for ox in 0..w {
            let (x0, x1) = bounds(ox, w, width);
            let mut sum = [0.0f32; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    for (s, v) in sum.iter_mut().zip(px.linear(x, y)) {
                        *s += v;
                    }
                }
            }
            let count = ((y1 - y0) * (x1 - x0)).max(1) as f32;
            let encoded = sum.map(|s| aura_render::output::encode_srgb(s / count));
            out.extend(aura_render::output::display_srgb_to_working(encoded));
        }
    }
    (out, w, h)
}

/// Size Shadows and Highlights from the frame's regions, after `exposure`.
#[must_use]
pub fn plan(px: &Pixels<'_>, exposure: f32, kind: SceneKind) -> RangePlan {
    let (rgb, w, h) = working_frame(px);
    plan_from(edge_aware::regions(&rgb, w, h, exposure), kind)
}

/// [`plan`], from regions already measured.
#[must_use]
pub fn plan_from(regions: Regions, kind: SceneKind) -> RangePlan {
    let r = regions;
    let shadows = if r.dark_share < FEWEST_SHADOWS || r.dark_depth < BLACK_BY_DESIGN {
        0.0
    } else {
        let minority = (r.dark_share / FULL_SHADOWS).clamp(0.0, 1.0);
        let majority = ((r.dark_share - MOSTLY_SHADOW_FROM)
            / (MOSTLY_SHADOW_FULL - MOSTLY_SHADOW_FROM))
            .clamp(0.0, 1.0);
        let depth = ((SHADOW_DEPTH_FROM - r.dark_depth) / 2.0).clamp(0.0, 1.0);
        (MINORITY_SHADOWS * minority + MAJORITY_SHADOWS * majority) * (0.35 + 0.65 * depth)
    };
    let highlights = {
        let need = ((r.high - HIGHLIGHT_FROM) / 1.2).clamp(0.0, 1.0);
        let area = (r.bright_share / FULL_HIGHLIGHTS).clamp(0.0, 1.0);
        let dominant = ((r.bright_share - DOMINANT_HIGHLIGHTS_FROM)
            / (DOMINANT_HIGHLIGHTS_FULL - DOMINANT_HIGHLIGHTS_FROM))
            .clamp(0.0, 1.0);
        let keep = 1.0 - (1.0 - DOMINANT_HIGHLIGHTS_KEEP) * dominant;
        -(MOST_HIGHLIGHTS * need * area * keep)
    };
    let (mut shadows, mut highlights) = (shadows.round() as i16, highlights.round() as i16);
    match kind {
        // The darkness is the scene: lifting its regions turns night into grey.
        SceneKind::Night => shadows = 0,
        // Bright by design: its white is the photograph, and it has little shadow to lift.
        SceneKind::HighKey => {
            highlights = highlights.max(HIGH_KEY_HIGHLIGHTS);
            shadows /= 2;
        }
        _ => {}
    }
    RangePlan {
        shadows,
        highlights,
        regions: r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regions(dark_share: f32, dark_depth: f32, bright_share: f32, high: f32) -> Regions {
        Regions {
            low: dark_depth - 1.0,
            median: 0.0,
            high,
            dark_share,
            dark_depth,
            bright_share,
            clipped_share: 0.0,
        }
    }

    #[test]
    fn a_dark_interior_with_bright_windows_gets_both_sliders() {
        let plan = plan_from(regions(0.8, -4.9, 0.12, 2.9), SceneKind::LowLight);
        assert!(plan.shadows >= 55, "{plan:?}");
        assert!(plan.highlights <= -25, "{plan:?}");
        assert!(plan.sentence().is_some());
    }

    #[test]
    fn an_ordinary_frame_asks_for_nothing() {
        let plan = plan_from(regions(0.02, -3.2, 0.1, 1.5), SceneKind::General);
        assert!(plan.is_empty(), "{plan:?}");
        assert!(plan.sentence().is_none());
    }

    #[test]
    fn a_dark_background_is_opened_lightly_and_a_dark_frame_firmly() {
        let background = plan_from(regions(0.2, -4.5, 0.1, 1.5), SceneKind::General);
        let interior = plan_from(regions(0.85, -4.5, 0.02, 1.5), SceneKind::General);
        assert!(
            background.shadows > 0 && background.shadows <= 30,
            "{background:?}"
        );
        assert!(interior.shadows > background.shadows * 2, "{interior:?}");
    }

    #[test]
    fn a_bright_sky_is_recovered_more_than_a_bright_subject() {
        let sky = plan_from(regions(0.05, -3.5, 0.2, 2.8), SceneKind::Landscape);
        let facade = plan_from(regions(0.05, -3.5, 0.6, 2.8), SceneKind::General);
        assert!(sky.highlights < facade.highlights, "{sky:?} vs {facade:?}");
        assert!(facade.highlights < 0);
    }

    #[test]
    fn black_by_design_is_left_black() {
        let plan = plan_from(regions(0.4, -12.0, 0.0, 1.0), SceneKind::General);
        assert_eq!(plan.shadows, 0);
    }

    #[test]
    fn night_keeps_its_darkness_and_high_key_its_white() {
        let night = plan_from(regions(0.7, -5.0, 0.05, 3.0), SceneKind::Night);
        assert_eq!(night.shadows, 0);
        let bright = plan_from(regions(0.1, -4.0, 0.6, 3.0), SceneKind::HighKey);
        assert!(bright.highlights >= HIGH_KEY_HIGHLIGHTS);
    }

    #[test]
    fn the_stronger_answer_wins_and_they_never_add() {
        let plan = RangePlan {
            shadows: 30,
            highlights: -20,
            regions: Regions::default(),
        };
        assert_eq!(plan.merged(-25, 10), (-25, 30));
        assert_eq!(plan.merged(0, 40), (-20, 40));
    }

    #[test]
    fn a_preview_is_measured_in_the_working_space_at_the_base_grid_size() {
        let (w, h) = (1200u32, 800u32);
        let bytes: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let v = if (i % w) < w / 2 { 20 } else { 200 };
                [v, v, v]
            })
            .collect();
        let px = Pixels::new(&bytes, w, h).expect("pixels");
        let (rgb, sw, sh) = working_frame(&px);
        assert_eq!((sw, sh), (MEASURE_EDGE, 341));
        assert_eq!(rgb.len(), sw * sh * 3);
        // The dark half is deep shadow; the bright half is not.
        let plan = plan(&px, 0.0, SceneKind::General);
        assert!(plan.regions.dark_share > 0.4, "{plan:?}");
    }
}
