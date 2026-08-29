//! Which of three sources knows about this lens, and what that becomes.
//!
//! Section 6.1's order is "embedded correction data, then the bundled profile database keyed by
//! lens id and focal length, then geometric estimation from straight edges", and
//! [`crate::profiles::resolve_lens`] resolves the first two. This module owns the third, and the
//! two refusals that sit beside it: a vignette correction that would clip, and a chromatic
//! aberration correction nothing in the frame could verify.
//!
//! ## The estimator is a measurement and it refuses more often than it answers
//!
//! Section 2.1 asks for a manual-lens fallback that estimates distortion "from long straight
//! edges". [`estimate`] finds the strongest near-straight edge chains in the four outer bands of
//! the frame, fits a parabola to each, and turns the bow into a `k1`. It answers only when at
//! least [`MIN_AGREEING_CHAINS`] of them agree about the *sign*, because a single bowed line in a
//! photograph is far more often a bowed thing than a bowed lens - a garland, a bouquet, the top of
//! a mandap - and correcting a wedding's optics from one of those is a distortion nobody had.
//!
//! ## The maths, and why it is three lines rather than an optimiser
//!
//! A straight line in the world at signed perpendicular distance `d` from the optical centre
//! images, under `r_image = r (1 + k1 r^2)`, at
//!
//! ```text
//!   d(t) = d * (1 + k1 * (d^2 + t^2))
//! ```
//!
//! at lateral offset `t`, all in a radius normalised so the frame's corner is at one. So the
//! bow over a half-length `T` is `d(T) - d(0) = d * k1 * T^2`, and
//!
//! ```text
//!   k1 = (d(T) - d(0)) / (d * T^2)
//! ```
//!
//! which is a division rather than a fit. It is exactly the coefficient
//! [`aura_render::geometry::LensModel::k1`] wants, because the correction reads the source at the
//! distorted radius and that is the same map.
//!
//! ## What an estimate may never do
//!
//! It may not exceed [`MAX_ESTIMATED_K1`], it never produces a `k2` or a `k3`, and it never
//! produces a chromatic aberration or a vignette. A profile has three distortion terms because
//! somebody photographed a target at several radii; one photograph of a room has one usable
//! measurement in it, and a higher-order term fitted to it would be fitting the garland.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{GeometryCode, LensCorrection, LensSource, MAX_VIGNETTE};
use aura_render::geometry::{vignette_amount, LensModel};

use crate::profiles::LensMatch;

/// How many of the four bands must agree about the sign of the bow.
///
/// Three of four. Two agreeing is a coincidence a single long object in the frame can produce -
/// a table edge and its reflection, a pair of pillars - and four is a requirement that a frame
/// with one blown-out corner can never meet.
pub const MIN_AGREEING_CHAINS: usize = 3;

/// The largest distortion an estimate may claim.
///
/// Four hundredths, which is about the barrel of a 17 mm zoom and well inside what the class
/// ladder in the bundled table asks for. An estimate that wanted more than this has measured
/// something that is not a lens, and the honest response is to report the ceiling rather than the
/// number - so [`estimate`] refuses instead of clamping.
pub const MAX_ESTIMATED_K1: f32 = 0.04;

/// The smallest bow, as a share of the frame's half-diagonal, that is worth calling distortion.
///
/// A thousandth. Below it the measurement is the width of the edge chain rather than its
/// curvature, which is phase 22's lesson written down again: a threshold is a statement about the
/// instrument as well as about the world, and an edge located to the nearest pixel on a 2048 px
/// proxy cannot resolve a bow finer than about this.
pub const MIN_BOW: f32 = 0.001;

/// How far out a high-contrast edge must sit before a chromatic aberration correction can be
/// checked against it, as a normalised radius.
///
/// Half. Lateral chromatic aberration is zero at the optical centre by definition and grows with
/// radius, so a frame whose only strong edges are in the middle carries no evidence about it
/// either way - and applying a correction there is moving two channels by a fraction of a pixel
/// on the strength of a table.
pub const CA_MIN_RADIUS: f32 = 0.5;

/// The share of the frame's gradient energy that must sit beyond [`CA_MIN_RADIUS`].
///
/// A twentieth. Low, because the outer annulus is where a corrected fringe is visible and a
/// little evidence there is enough to justify a correction that is a fraction of a pixel.
pub const CA_MIN_ENERGY_SHARE: f32 = 0.05;

/// What this frame says about its own optics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evidence {
    /// The share of the frame's gradient energy beyond [`CA_MIN_RADIUS`], `0..1`.
    pub outer_energy: f32,
    /// The brightest luminance in the frame's corners, `0..`.
    ///
    /// What the vignette refusal is measured against. Above one after the correction's gain is a
    /// corner the correction would clip, which turns a slightly dark corner into a flat white
    /// one - a worse defect than the one being fixed and an irreversible one.
    pub corner_peak: f32,
}

impl Default for Evidence {
    fn default() -> Self {
        Self {
            outer_energy: 0.0,
            corner_peak: 0.0,
        }
    }
}

impl Evidence {
    /// Measure a proxy.
    #[must_use]
    pub fn of_proxy(rgb: &[f32], width: usize, height: usize) -> Self {
        if width < 8 || height < 8 {
            return Self::default();
        }
        let luma = aura_render::spatial::luma_plane(rgb, width, height);
        let gradient = aura_render::spatial::gradient_plane(&luma, width, height);
        let cx = (width as f32 - 1.0) / 2.0;
        let cy = (height as f32 - 1.0) / 2.0;
        let max_r = cx.hypot(cy).max(1e-6);

        let mut total = 0.0f64;
        let mut outer = 0.0f64;
        let mut corner_peak = 0.0f32;
        for y in 0..height {
            for x in 0..width {
                let energy = f64::from(gradient.get(y * width + x).copied().unwrap_or(0.0));
                total += energy;
                let r = (x as f32 - cx).hypot(y as f32 - cy) / max_r;
                if r >= CA_MIN_RADIUS {
                    outer += energy;
                }
                if r >= 0.80 {
                    corner_peak = corner_peak.max(luma.get(y * width + x).copied().unwrap_or(0.0));
                }
            }
        }
        Self {
            outer_energy: if total <= 0.0 {
                0.0
            } else {
                (outer / total) as f32
            },
            corner_peak,
        }
    }
}

/// What the lens half of a plan decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// The correction, ready for the recipe.
    pub correction: LensCorrection,
    /// The codes that explain it, in the order the panel reads them.
    pub codes: Vec<GeometryCode>,
    /// The model the correction was made from, when there is one.
    ///
    /// Carried so the pass can report whether it came from a measured row - on this build, never.
    pub model: Option<LensModel>,
}

impl Decision {
    /// Nothing corrected.
    #[must_use]
    pub fn none(code: GeometryCode) -> Self {
        Self {
            correction: LensCorrection::none(),
            codes: vec![code],
            model: None,
        }
    }
}

/// Turn a resolved profile and this frame's own evidence into a correction.
///
/// The three refusals that live here are all about *this frame* rather than about the lens:
/// a chromatic aberration correction with nothing off-centre to verify it, a vignette correction
/// that would clip the corners it is lifting, and an estimate that could not be made.
#[must_use]
pub fn decide(matched: &LensMatch, evidence: Evidence, estimated: Option<f32>) -> Decision {
    match matched.source {
        LensSource::Embedded => {
            // The camera wrote its own numbers and phase 02 applied them on decode. There is
            // nothing for this phase to add, and `profile_id` stays `None` because there is no
            // row to name - a plan that invented one would be a plan claiming a provenance it
            // does not have.
            Decision {
                correction: LensCorrection {
                    distortion: true,
                    vignette: 0,
                    ca: true,
                    profile_id: None,
                    source: LensSource::Embedded,
                },
                codes: vec![GeometryCode::LensEmbedded],
                model: None,
            }
        }
        LensSource::Database => {
            let Some(model) = matched.model else {
                return Decision::none(GeometryCode::LensProfileMissing);
            };
            let mut codes = vec![GeometryCode::LensProfileMatched];

            let (ca, ca_code) = verifiable_ca(&model, evidence);
            if let Some(code) = ca_code {
                codes.push(code);
            }
            let (vignette, reduced) = safe_vignette(&model, evidence);
            if reduced {
                codes.push(GeometryCode::LensVignetteReduced);
            }

            Decision {
                correction: LensCorrection {
                    distortion: !model.is_identity(),
                    vignette,
                    ca,
                    profile_id: matched.profile_id.clone(),
                    source: LensSource::Database,
                }
                .clamped(),
                codes,
                model: Some(model),
            }
        }
        LensSource::Estimated | LensSource::None => {
            let Some(k1) = estimated else {
                return Decision::none(matched.code);
            };
            // An estimate corrects distortion and nothing else. There is no chromatic aberration
            // term and no vignette term, because one photograph of a room carries one usable
            // measurement and inventing the other two from it would be a correction nobody made.
            let model = LensModel {
                k1,
                ..LensModel::identity()
            };
            Decision {
                correction: LensCorrection {
                    distortion: true,
                    vignette: 0,
                    ca: false,
                    profile_id: None,
                    source: LensSource::Estimated,
                }
                .clamped(),
                codes: vec![GeometryCode::LensEstimated],
                model: Some(model),
            }
        }
    }
}

/// Whether a chromatic aberration correction can be checked against anything in this frame.
fn verifiable_ca(model: &LensModel, evidence: Evidence) -> (bool, Option<GeometryCode>) {
    if model.ca_red.abs() < 1e-9 && model.ca_blue.abs() < 1e-9 {
        return (false, None);
    }
    if evidence.outer_energy < CA_MIN_ENERGY_SHARE {
        return (false, Some(GeometryCode::LensCaUnverifiable));
    }
    (true, Some(GeometryCode::LensCaCorrected))
}

/// How much of a profile's vignette correction this frame can take without clipping.
///
/// The gain the renderer applies at the corner is `1 + 0.6 * amount * r^2`, which at `r = 1` is
/// `1 + 0.6 * amount`. A corner already near white cannot take all of it, and the share that
/// fits is solved rather than stepped down to - a correction that stopped one step short of
/// clipping would leave a visible ring where the steps met.
fn safe_vignette(model: &LensModel, evidence: Evidence) -> (u8, bool) {
    let full = vignette_amount(model, 1.0);
    if full == 0 {
        return (0, false);
    }
    let peak = evidence.corner_peak;
    if peak <= 0.0 {
        return (full, false);
    }
    // The largest `amount` in `0..1` with `peak * (1 + 0.6 * amount) <= 1`.
    let headroom = ((1.0 / peak - 1.0) / 0.6).clamp(0.0, 1.0);
    let wanted = f32::from(full) / 100.0;
    if wanted <= headroom + 1e-4 {
        return (full, false);
    }
    // Floor rather than round. Rounding half a per cent upward is half a per cent of clipping,
    // and the whole reason this branch exists is that the corner had no room left.
    let reduced = (headroom * 100.0).floor().clamp(0.0, f32::from(MAX_VIGNETTE)) as u8;
    (reduced, true)
}

/// Estimate `k1` from the long straight edges in a frame.
///
/// `None` when fewer than [`MIN_AGREEING_CHAINS`] bands produced a usable chain, when they
/// disagreed about the sign, or when the answer exceeded [`MAX_ESTIMATED_K1`]. Refusing rather
/// than clamping in the last case, because a number at a ceiling is indistinguishable from a
/// number that was measured there.
#[must_use]
pub fn estimate(rgb: &[f32], width: usize, height: usize) -> Option<f32> {
    if width < 32 || height < 32 {
        return None;
    }
    let luma = aura_render::spatial::luma_plane(rgb, width, height);
    let (gx, gy) = aura_render::spatial::sobel_planes(&luma, width, height);

    let mut samples: Vec<f32> = Vec::new();
    // Four bands: the top and bottom thirds carry near-horizontal lines and the left and right
    // thirds near-vertical ones. Outer bands only, because the bow this measures grows with the
    // square of the lateral offset and a chain through the middle of the frame has almost none
    // of it - which is the same reason `CA_MIN_RADIUS` exists.
    for (horizontal, near) in [(true, true), (true, false), (false, true), (false, false)] {
        if let Some(k1) = chain_k1(&gx, &gy, width, height, horizontal, near) {
            samples.push(k1);
        }
    }
    if samples.len() < MIN_AGREEING_CHAINS {
        return None;
    }
    let positive = samples.iter().filter(|k| **k > 0.0).count();
    let agreeing = positive.max(samples.len() - positive);
    if agreeing < MIN_AGREEING_CHAINS {
        return None;
    }
    let sign = if positive >= samples.len() - positive {
        1.0
    } else {
        -1.0
    };
    let mut agreed: Vec<f32> = samples
        .into_iter()
        .filter(|k| k.signum() == sign)
        .collect();
    agreed.sort_by(f32::total_cmp);
    // The median rather than the mean: one band that latched onto a bouquet rather than a wall
    // should not move the answer, and with three or four samples the median is the only estimator
    // that guarantees it cannot.
    let k1 = agreed.get(agreed.len() / 2).copied()?;
    (k1.abs() <= MAX_ESTIMATED_K1).then_some(k1)
}

/// Follow one edge chain across a band and return the `k1` its bow implies.
///
/// `horizontal` picks a near-horizontal chain (tracked in `y` as a function of `x`) or a
/// near-vertical one; `near` picks the band closer to the origin of that axis.
fn chain_k1(
    gx: &[f32],
    gy: &[f32],
    width: usize,
    height: usize,
    horizontal: bool,
    near: bool,
) -> Option<f32> {
    // Work in the axis the chain runs along (`long`) and the axis it is tracked on (`across`).
    let (long, across) = if horizontal {
        (width, height)
    } else {
        (height, width)
    };
    if long < 24 || across < 24 {
        return None;
    }
    let band = across / 3;
    let (lo, hi) = if near { (1, band) } else { (across - band, across - 1) };

    // Twenty-one stations along the chain, which is enough to fit a parabola robustly and few
    // enough that each one can search its whole band.
    const STATIONS: usize = 21;
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(STATIONS);
    let mut tracked: Option<usize> = None;
    // Middle outward: the chain is anchored where the frame is most likely to have it and
    // followed toward the ends, so a chain that leaves the band takes the stations after it
    // rather than the whole measurement.
    let order: Vec<usize> = {
        let middle = STATIONS / 2;
        let mut out = vec![middle];
        for step in 1..=middle {
            if middle + step < STATIONS {
                out.push(middle + step);
            }
            if middle >= step {
                out.push(middle - step);
            }
        }
        out
    };
    for station in order {
        let l = (station * (long - 1)) / (STATIONS - 1);
        let window = tracked.map_or((lo, hi), |t| {
            (t.saturating_sub(4).max(lo), (t + 4).min(hi))
        });
        let mut best = (0.0f32, 0usize);
        for a in window.0..=window.1 {
            let (x, y) = if horizontal { (l, a) } else { (a, l) };
            let (Some(&ex), Some(&ey)) = (gx.get(y * width + x), gy.get(y * width + x)) else {
                continue;
            };
            // Perpendicular to the chain: a near-horizontal edge has a mostly vertical gradient.
            let (along, perpendicular) = if horizontal { (ex, ey) } else { (ey, ex) };
            if perpendicular.abs() < along.abs() * 2.0 {
                continue;
            }
            let strength = perpendicular.abs();
            if strength > best.0 {
                best = (strength, a);
            }
        }
        if best.0 <= 0.02 {
            continue;
        }
        tracked = Some(best.1);
        points.push((l as f32, best.1 as f32));
    }
    if points.len() < STATIONS - 6 {
        return None;
    }

    // Into the normalised radius the distortion model is written in: centred, and divided by the
    // frame's own half-diagonal so that the corner is at one.
    let cx = (width as f32 - 1.0) / 2.0;
    let cy = (height as f32 - 1.0) / 2.0;
    let max_r = cx.hypot(cy).max(1e-6);
    let mut mapped: Vec<(f32, f32)> = points
        .into_iter()
        .map(|(l, a)| {
            let (x, y) = if horizontal { (l, a) } else { (a, l) };
            ((x - cx) / max_r, (y - cy) / max_r)
        })
        .collect();
    // Along the chain, so the middle of the list is the middle of the chain.
    mapped.sort_by(|left, right| {
        if horizontal {
            left.0.total_cmp(&right.0)
        } else {
            left.1.total_cmp(&right.1)
        }
    });

    let value = |p: (f32, f32)| if horizontal { p.1 } else { p.0 };
    let offset = |p: (f32, f32)| if horizontal { p.0 } else { p.1 };

    let first = *mapped.first()?;
    let last = *mapped.last()?;
    let middle = *mapped.get(mapped.len() / 2)?;

    // `d` is the chain's perpendicular distance from the centre at its own midpoint, and `T` is
    // its half-length. Both in half-diagonal units, which is what makes the division below a
    // `k1` rather than a number in pixels.
    let d = value(middle);
    let half_length = (offset(last) - offset(first)).abs() / 2.0;
    if d.abs() < 0.10 || half_length < 0.20 {
        // A chain through the middle of the frame, or a short one. Neither carries enough bow to
        // divide by; reporting a number from one is dividing noise by a small number.
        return None;
    }
    // The bow, measured at both ends and averaged, which cancels any residual tilt in the chain.
    let bow = ((value(first) - d) + (value(last) - d)) / 2.0;
    if bow.abs() < MIN_BOW {
        return None;
    }
    let k1 = bow / (d * half_length * half_length);
    k1.is_finite().then_some(k1)
}

/// Where in the frame a lens reason points, for the panel.
///
/// The whole frame for a distortion or a vignette, and the corners for a chromatic aberration -
/// which is where a fringe is and where a photographer will look to check.
#[must_use]
pub fn evidence_box(code: GeometryCode) -> Option<Box2> {
    match code {
        GeometryCode::LensCaCorrected | GeometryCode::LensCaUnverifiable => Some(Box2 {
            x: 0.0,
            y: 0.0,
            w: 0.22,
            h: 0.22,
        }),
        GeometryCode::LensVignetteReduced => Some(Box2 {
            x: 0.78,
            y: 0.78,
            w: 0.22,
            h: 0.22,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::LensMatch;

    /// A grid of straight lines, run through a known distortion so that the bow is painted into
    /// the pixels rather than asserted about them.
    fn distorted_grid(width: usize, height: usize, k1: f32) -> Vec<f32> {
        let model = LensModel {
            k1,
            ..LensModel::identity()
        };
        let cx = (width as f32 - 1.0) / 2.0;
        let cy = (height as f32 - 1.0) / 2.0;
        let max_r = cx.hypot(cy).max(1e-6);
        let mut rgb = vec![0.12f32; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                // Where this image pixel came from in the undistorted world. The imaging model
                // is `r_image = r_world (1 + k1 r_world^2)`, so painting the world through it is
                // the inverse - and the estimator, which reads the image, must recover `k1`.
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let r = (dx * dx + dy * dy).sqrt() / max_r;
                // One step of fixed-point inversion is plenty at these magnitudes.
                let mut world = r;
                for _ in 0..24 {
                    world = if model.source_radius(world) <= 0.0 {
                        r
                    } else {
                        world * r / model.source_radius(world).max(1e-6)
                    };
                }
                let gain = if r < 1e-6 { 1.0 } else { world / r };
                let wx = cx + dx * gain;
                let wy = cy + dy * gain;
                // Straight lines in the world: a grid every eighth of the frame.
                let line = |v: f32, span: f32| {
                    let period = span / 8.0;
                    let phase = (v / period).fract().abs();
                    phase < 0.06 || phase > 0.94
                };
                let on = line(wx, width as f32) || line(wy, height as f32);
                let value = if on { 0.92 } else { 0.12 };
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut((y * width + x) * 3 + channel) {
                        *slot = value;
                    }
                }
            }
        }
        rgb
    }

    #[test]
    fn a_frame_with_no_straight_edges_gives_no_estimate() {
        let noise: Vec<f32> = (0..256 * 256 * 3)
            .map(|i| ((i * 7919) % 251) as f32 / 251.0)
            .collect();
        assert!(estimate(&noise, 256, 256).is_none());
        let flat = vec![0.4f32; 256 * 256 * 3];
        assert!(estimate(&flat, 256, 256).is_none());
    }

    #[test]
    fn the_estimator_recovers_the_sign_of_a_distortion_painted_into_the_pixels() {
        // The sign is what the correction gets wrong catastrophically: correcting barrel as
        // pincushion doubles the defect. The magnitude is checked separately and loosely,
        // because a chain located to the nearest pixel on a small fixture cannot do better.
        let barrel = estimate(&distorted_grid(320, 240, -0.030), 320, 240);
        let pincushion = estimate(&distorted_grid(320, 240, 0.030), 320, 240);
        if let Some(k1) = barrel {
            assert!(k1 < 0.0, "barrel was estimated as {k1}");
        }
        if let Some(k1) = pincushion {
            assert!(k1 > 0.0, "pincushion was estimated as {k1}");
        }
        assert!(
            barrel.is_some() || pincushion.is_some(),
            "neither direction produced an estimate, so the test proved nothing"
        );
    }

    #[test]
    fn an_undistorted_grid_is_left_alone() {
        // The failure that matters most: inventing a correction for a lens that is behaving. A
        // straight grid must not produce a distortion large enough to act on.
        if let Some(k1) = estimate(&distorted_grid(320, 240, 0.0), 320, 240) {
            assert!(k1.abs() < 0.006, "a straight grid estimated {k1}");
        }
    }

    #[test]
    fn embedded_data_wins_and_names_no_profile() {
        let decision = decide(
            &LensMatch {
                profile_id: None,
                model: None,
                source: LensSource::Embedded,
                code: GeometryCode::LensEmbedded,
            },
            Evidence::default(),
            None,
        );
        assert_eq!(decision.correction.source, LensSource::Embedded);
        assert!(decision.correction.profile_id.is_none());
        assert_eq!(decision.codes, vec![GeometryCode::LensEmbedded]);
    }

    #[test]
    fn a_chromatic_aberration_correction_needs_something_off_centre_to_check_it() {
        let db = aura_render::geometry::database();
        let matched = crate::profiles::resolve_lens(
            &crate::profiles::LensExif {
                name: "EF16-35mm f/2.8L III USM".into(),
                focal_mm: Some(16.0),
                embedded: false,
            },
            db,
        );
        assert_eq!(matched.source, LensSource::Database);

        let blank = decide(
            &matched,
            Evidence {
                outer_energy: 0.0,
                corner_peak: 0.3,
            },
            None,
        );
        assert!(!blank.correction.ca);
        assert!(blank.codes.contains(&GeometryCode::LensCaUnverifiable));

        let detailed = decide(
            &matched,
            Evidence {
                outer_energy: 0.4,
                corner_peak: 0.3,
            },
            None,
        );
        assert!(detailed.correction.ca);
        assert!(detailed.codes.contains(&GeometryCode::LensCaCorrected));
    }

    #[test]
    fn a_vignette_correction_that_would_clip_is_reduced_rather_than_applied() {
        let model = LensModel {
            vignette: 100,
            ..LensModel::identity()
        };
        // A corner already at 0.95 has almost no headroom: 0.95 * 1.6 clips hard.
        let (reduced, was_reduced) = safe_vignette(
            &model,
            Evidence {
                outer_energy: 0.2,
                corner_peak: 0.95,
            },
        );
        assert!(was_reduced);
        assert!(reduced < 100);
        assert!(
            0.95 * (1.0 + 0.6 * f32::from(reduced) / 100.0) <= 1.001,
            "the reduced amount still clips"
        );

        // A dark corner takes the whole correction.
        let (full, was_reduced) = safe_vignette(
            &model,
            Evidence {
                outer_energy: 0.2,
                corner_peak: 0.2,
            },
        );
        assert!(!was_reduced);
        assert_eq!(full, 100);
    }

    #[test]
    fn an_estimate_corrects_distortion_and_nothing_else() {
        let decision = decide(
            &LensMatch::missing(GeometryCode::LensProfileMissing),
            Evidence {
                outer_energy: 0.9,
                corner_peak: 0.1,
            },
            Some(-0.02),
        );
        assert_eq!(decision.correction.source, LensSource::Estimated);
        assert!(decision.correction.distortion);
        assert!(!decision.correction.ca);
        assert_eq!(decision.correction.vignette, 0);
        assert!(decision.correction.profile_id.is_none());
    }

    #[test]
    fn no_profile_and_no_estimate_corrects_nothing() {
        let decision = decide(
            &LensMatch::missing(GeometryCode::LensProfileMissing),
            Evidence::default(),
            None,
        );
        assert!(decision.correction.is_identity());
        assert_eq!(decision.correction.source, LensSource::None);
        assert_eq!(decision.codes, vec![GeometryCode::LensProfileMissing]);
    }
}
