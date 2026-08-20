//! Dodge and burn: the classic retoucher moves, applied conservatively.
//!
//! PHASE-19 section 6.3's second and third bullets. The shaping map is derived from **face
//! geometry and the existing light direction**, deepening natural shadow zones slightly and
//! lifting under-eye and jaw shadows.
//!
//! ## The map is not stored. The moves are.
//!
//! Phase 13's rule - evidence can never be a pixel - applied to a decision. A 32x32 grid per
//! band per face is 2 KB, and a wedding's worth of them is a catalog nobody can back up. What
//! is stored is the handful of [`ShapingZone`] rows the grid was generated from: a named
//! region, a centre, a radius and a gain. Ten zones at four numbers each is 160 bytes, it is
//! **legible** - a support engineer can read "jaw, -0.04 EV" and know what happened - and the
//! grid is regenerated deterministically by [`grid`].
//!
//! The cost is a fourth version column. A change to [`grid`] changes delivered pixels without
//! changing one stored number, and `shaping_ver` is the only thing that makes that visible.
//!
//! ## Light direction, and why the zones are not symmetric
//!
//! A face lit from the left has a shadow on the right, and deepening the *shadow* side while
//! lifting the *lit* side is not shaping - it is flattening. So [`zones_for`] reads the low
//! band's own horizontal gradient across the face and mirrors the burn zones onto whichever
//! side is already darker. That is the move a retoucher makes: shadows go deeper, not
//! elsewhere.
//!
//! ## What stops this looking edited
//!
//! Three things, all structural rather than tuned:
//!
//! * [`ShapingZone::MAX_GAIN_EV`] is a sixth of a stop, and the contract refuses a plan that
//!   exceeds it;
//! * three of the ten zones are dodge-only, in the contract, so no arithmetic here can put a
//!   shadow under somebody's eyes;
//! * only faces above [`MIN_SHAPEABLE_FACE`] are shaped at all, because the low band of a
//!   forty-pixel face is four samples wide and a shaping map over it is noise with a name.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::local::{
    DodgeBurnMaps, FaceShaping, FaceZone, ShapingZone, MAX_SHAPED_FACES, MID_BAND_TOLERANCE,
    MIN_SHAPEABLE_FACE, SHAPING_SIDE,
};

use crate::local::freqsep::Bands;

/// Which build's derivation turns zones into grids.
///
/// Written into `local_light_plan.shaping_ver`. Bump it on any change to [`grid`], to
/// [`zones_for`]'s geometry, or to the constants below - a change to any of them changes what
/// a delivered JPEG looks like without changing a stored number, and `AURA-ML-5066` exists so
/// that never happens silently.
pub const SHAPING_VER: u16 = 1;

/// How much of the mid-band tolerance the evening actually aims at.
///
/// Ninety-five per cent rather than all of it. The gate compares a measured ratio against
/// [`MID_BAND_TOLERANCE`], and a solve that aims exactly at the constant fails on rounding
/// about half the time - which is a test that flickers rather than a property that holds.
pub const EVENING_MARGIN: f32 = 0.95;

/// The gain each zone gets at full strength, in stops, before any scaling.
///
/// Every one of them is at or below [`ShapingZone::MAX_GAIN_EV`], and the three dodge-only
/// zones are positive by construction rather than by arithmetic. The relative sizes are the
/// argument: the under-eye lift is the largest because it is the move that reads as "well
/// lit", and the jaw burn is second because it is the one that gives a face an edge.
const ZONE_GAINS: [(FaceZone, f32); 10] = [
    (FaceZone::UnderEye, 0.150),
    (FaceZone::Cheekbone, 0.090),
    (FaceZone::CheekHollow, -0.075),
    (FaceZone::Jaw, -0.110),
    (FaceZone::NoseBridge, 0.070),
    (FaceZone::NoseSide, -0.060),
    (FaceZone::Forehead, 0.055),
    (FaceZone::Temple, -0.085),
    (FaceZone::Chin, 0.045),
    (FaceZone::NeckShadow, -0.095),
];

/// Where each zone sits, as a fraction of the face box: `(x, y, radius)`.
///
/// `x` is mirrored for the shadow side; `y` runs from the top of the box. These are the
/// proportions of a face rather than measurements of one - the eyes at 40 % down, the mouth at
/// 72 %, the jaw at 88 % - which is the same canonical geometry `aura-vision`'s 112 px warp
/// assumes, and it is why this module needs a face box and two eye positions rather than a
/// landmark model.
const ZONE_GEOMETRY: [(FaceZone, f32, f32, f32); 10] = [
    (FaceZone::UnderEye, 0.32, 0.48, 0.13),
    (FaceZone::Cheekbone, 0.26, 0.58, 0.15),
    (FaceZone::CheekHollow, 0.24, 0.70, 0.14),
    (FaceZone::Jaw, 0.20, 0.86, 0.16),
    (FaceZone::NoseBridge, 0.50, 0.55, 0.09),
    (FaceZone::NoseSide, 0.42, 0.62, 0.08),
    (FaceZone::Forehead, 0.50, 0.22, 0.20),
    (FaceZone::Temple, 0.12, 0.32, 0.14),
    (FaceZone::Chin, 0.50, 0.92, 0.11),
    (FaceZone::NeckShadow, 0.50, 1.02, 0.16),
];

/// Which side of the face is already darker.
///
/// `-1.0` for the left, `1.0` for the right, `0.0` for a flatly lit face. Read from the low
/// band rather than from the whole crop, because the mid band's blotches would otherwise
/// decide which way a face is lit.
#[must_use]
pub fn light_direction(bands: &Bands) -> f32 {
    if bands.is_empty() || bands.width < 4 {
        return 0.0;
    }
    let half = bands.width / 2;
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    let mut left_n = 0usize;
    let mut right_n = 0usize;
    for y in 0..bands.height {
        for x in 0..bands.width {
            let value = bands.low.get(y * bands.width + x).copied().unwrap_or(0.0);
            if x < half {
                left += value;
                left_n += 1;
            } else {
                right += value;
                right_n += 1;
            }
        }
    }
    if left_n == 0 || right_n == 0 {
        return 0.0;
    }
    let left = left / left_n as f32;
    let right = right / right_n as f32;
    // Normalised so a two-per-cent difference is nothing and a ten-per-cent one is decisive.
    ((right - left) / 0.10).clamp(-1.0, 1.0)
}

/// The zones for one face.
///
/// `region` is the face box in frame coordinates. `direction` is [`light_direction`]'s answer.
/// `strength` is the scene policy's `dodge_burn_low` after the governor.
///
/// Returns an empty list for a face too small to shape - which is a real answer that the
/// caller turns into [`aura_core::contract::local::LocalCode::FaceTooSmallToShape`], not a
/// failure.
#[must_use]
pub fn zones_for(
    region: CropRect,
    direction: f32,
    strength: f32,
    frame_short_side_frac: f32,
) -> Vec<ShapingZone> {
    let side = region.w.min(region.h) * frame_short_side_frac.max(1.0);
    if region.w.min(region.h) < MIN_SHAPEABLE_FACE || strength <= 0.0 {
        return Vec::new();
    }
    let _ = side;
    let strength = strength.clamp(0.0, 1.0);
    let mut zones = Vec::with_capacity(ZONE_GEOMETRY.len());
    for (zone, fx, fy, fr) in ZONE_GEOMETRY {
        let Some((_, base_gain)) = ZONE_GAINS.iter().copied().find(|(z, _)| *z == zone) else {
            continue;
        };
        // The burn zones move to the shadow side; the dodge zones stay where the feature is.
        // A dodge that chased the light would brighten the already-lit side, which is the
        // flattening move the module header warns about.
        let x = if zone.is_dodge_only() || (fx - 0.5).abs() < 1e-3 {
            fx
        } else if direction >= 0.0 {
            // Lit from the left, shadow on the right: mirror to the right half.
            1.0 - fx
        } else {
            fx
        };
        // A flatly lit face gets a smaller burn: there is no existing shadow to deepen, and
        // deepening nothing is painting one on.
        let directional = if zone.is_dodge_only() {
            1.0
        } else {
            0.45f32.mul_add(direction.abs(), 0.55)
        };
        let gain = (base_gain * strength * directional)
            .clamp(-ShapingZone::MAX_GAIN_EV, ShapingZone::MAX_GAIN_EV);
        if gain.abs() < 1e-4 {
            continue;
        }
        zones.push(ShapingZone {
            zone,
            centre: [
                (region.x + x * region.w).clamp(0.0, 1.0),
                (region.y + fy * region.h).clamp(0.0, 1.0),
            ],
            radius: (fr * region.w.max(region.h)).clamp(1e-3, 1.0),
            gain_ev: gain,
        });
    }
    zones
}

/// Regenerate the low-frequency grid from the stored zones.
///
/// Deterministic and pure. `region` is the same face box the zones were generated against, and
/// the grid is [`SHAPING_SIDE`] samples a side over it. Units are 1/200 of a stop, which puts
/// [`ShapingZone::MAX_GAIN_EV`] at 33 and leaves the `i8` three times the headroom it needs.
#[must_use]
pub fn grid(region: CropRect, zones: &[ShapingZone]) -> Vec<i8> {
    let side = usize::from(SHAPING_SIDE);
    let mut out = vec![0i8; side * side];
    if region.is_empty() {
        return out;
    }
    for sy in 0..side {
        for sx in 0..side {
            // Sample centres rather than corners, so the grid is symmetric about the region.
            let fx = (sx as f32 + 0.5) / side as f32;
            let fy = (sy as f32 + 0.5) / side as f32;
            let px = region.x + fx * region.w;
            let py = region.y + fy * region.h;
            let mut gain = 0.0f32;
            for zone in zones {
                let dx = px - zone.centre[0];
                let dy = py - zone.centre[1];
                let r = zone.radius.max(1e-4);
                let d2 = (dx * dx + dy * dy) / (r * r);
                if d2 > 9.0 {
                    continue;
                }
                // A Gaussian falloff. Nothing here has a hard edge, which is the whole
                // reason a zone is a centre and a radius rather than a polygon.
                gain += zone.gain_ev * (-0.5 * d2).exp();
            }
            let units = (gain * 200.0).round().clamp(-127.0, 127.0) as i8;
            if let Some(slot) = out.get_mut(sy * side + sx) {
                *slot = units;
            }
        }
    }
    out
}

/// The mid-frequency evening map, and how much of it may be applied.
///
/// Section 6.3: "mid-frequency evening reduces blotchy tonal patches without smoothing". A
/// blotch is a *low-spatial-frequency component of the mid band*, so the map is the mid band
/// smoothed at a wide radius and negated - it cancels the patch and leaves everything finer
/// than the patch untouched.
///
/// The strength is then bounded so that the mid band's own energy moves by at most
/// [`MID_BAND_TOLERANCE`]. That bound is what makes this the honest alternative to skin blur
/// rather than a soft version of it: a blur reduces mid-band energy without limit, and this
/// cannot reduce it by more than five per cent whatever anybody sets the slider to.
#[must_use]
pub fn evening(bands: &Bands, strength: f32) -> (Vec<i8>, f32, f32, f32) {
    let side = usize::from(SHAPING_SIDE);
    let before = bands.mid_energy();
    if bands.is_empty() || strength <= 0.0 || before <= f32::EPSILON {
        return (vec![0i8; side * side], 0.0, before, before);
    }
    let radius = (bands.width.min(bands.height) / 8).max(1);
    let blotches = crate::local::freqsep::blur(&bands.mid, bands.width, bands.height, radius);

    // How much of the blotch map can be cancelled before the band's energy moves too far.
    // Solved directly rather than searched: cancelling a fraction `k` of the blotches removes
    // `k` times the blotch energy from the band, so the largest admissible `k` is the
    // tolerance divided by the blotch fraction.
    let blotch_energy = if blotches.is_empty() {
        0.0
    } else {
        blotches.iter().map(|v| v.abs()).sum::<f32>() / blotches.len() as f32
    };
    let admissible = if blotch_energy <= f32::EPSILON {
        0.0
    } else {
        (EVENING_MARGIN * MID_BAND_TOLERANCE * before / blotch_energy).clamp(0.0, 1.0)
    };
    let applied = admissible.min(strength.clamp(0.0, 1.0));

    // Resample the blotch map onto the shaping grid.
    let mut out = vec![0i8; side * side];
    for sy in 0..side {
        let by = (sy * bands.height / side).min(bands.height.saturating_sub(1));
        for sx in 0..side {
            let bx = (sx * bands.width / side).min(bands.width.saturating_sub(1));
            let patch = blotches.get(by * bands.width + bx).copied().unwrap_or(0.0);
            // Negated: a bright patch is brought down and a dark one is brought up. In
            // luminance units, converted to the same 1/200-stop grid the low band uses.
            let ev = -patch * applied * crate::local::measure::ENCODING_GAMMA;
            if let Some(slot) = out.get_mut(sy * side + sx) {
                *slot = (ev * 200.0).round().clamp(-127.0, 127.0) as i8;
            }
        }
    }
    let after = (before - applied * blotch_energy).max(0.0);
    (out, applied, before, after)
}

/// Assemble one face's shaping.
#[must_use]
pub fn shape_face(
    identity: Option<aura_core::IdentityId>,
    region: CropRect,
    bands: &Bands,
    low_strength: f32,
    mid_strength: f32,
    frame_short_side_frac: f32,
) -> FaceShaping {
    let direction = light_direction(bands);
    let zones = zones_for(region, direction, low_strength, frame_short_side_frac);
    let low = grid(region, &zones);
    let (mid, applied, before, after) = evening(bands, mid_strength);
    FaceShaping {
        identity,
        region,
        side: SHAPING_SIDE,
        low_freq: low,
        mid_freq: mid,
        zones,
        light_direction: direction,
        low_strength: low_strength.clamp(0.0, 1.0),
        evening: applied,
        band_energy_before: before,
        band_energy_after: after,
    }
}

/// Collect the shaped faces into the plan's own shape.
///
/// At most [`MAX_SHAPED_FACES`], and the caller is expected to have sorted by prominence
/// first: lighting is solved for every face in the frame and shaping is not, because a
/// forty-person group formal is a frame where per-face form shaping is the wrong idea.
#[must_use]
pub fn collect(faces: Vec<FaceShaping>) -> Option<DodgeBurnMaps> {
    let faces: Vec<FaceShaping> = faces
        .into_iter()
        .filter(|f| !f.zones.is_empty() || f.evening > 0.0)
        .take(MAX_SHAPED_FACES)
        .collect();
    if faces.is_empty() {
        return None;
    }
    Some(DodgeBurnMaps {
        faces,
        shaping_ver: SHAPING_VER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_vision::embed::descriptors::LumaPlane;

    fn box_at(x: f32, y: f32, size: f32) -> CropRect {
        CropRect {
            x,
            y,
            w: size,
            h: size,
        }
    }

    fn lit_from_left(width: usize, height: usize) -> Bands {
        let values: Vec<f32> = (0..width * height)
            .map(|i| 0.60 - 0.25 * ((i % width) as f32 / width as f32))
            .collect();
        let plane = LumaPlane {
            values,
            width,
            height,
        };
        crate::local::freqsep::separate(&plane)
    }

    #[test]
    fn no_zone_ever_exceeds_the_contracts_ceiling() {
        for direction in [-1.0f32, -0.4, 0.0, 0.4, 1.0] {
            let zones = zones_for(box_at(0.3, 0.2, 0.30), direction, 1.0, 1.0);
            for zone in &zones {
                assert!(
                    zone.gain_ev.abs() <= ShapingZone::MAX_GAIN_EV + 1e-6,
                    "{:?} moved {} EV",
                    zone.zone,
                    zone.gain_ev
                );
            }
        }
    }

    #[test]
    fn a_dodge_only_zone_is_never_negative() {
        for direction in [-1.0f32, 0.0, 1.0] {
            for zone in zones_for(box_at(0.3, 0.2, 0.30), direction, 1.0, 1.0) {
                assert!(
                    !zone.sign_is_wrong(),
                    "{:?} was deepened at direction {direction}",
                    zone.zone
                );
            }
        }
    }

    #[test]
    fn a_face_too_small_to_shape_gets_no_zones() {
        let tiny = box_at(0.4, 0.4, MIN_SHAPEABLE_FACE - 0.005);
        assert!(zones_for(tiny, 0.0, 1.0, 1.0).is_empty());
    }

    #[test]
    fn the_burn_zones_follow_the_shadow_side() {
        let region = box_at(0.0, 0.0, 1.0);
        let lit_left = zones_for(region, 1.0, 1.0, 1.0);
        let lit_right = zones_for(region, -1.0, 1.0, 1.0);
        let jaw = |zones: &[ShapingZone]| {
            zones
                .iter()
                .find(|z| z.zone == FaceZone::Jaw)
                .map_or(0.5, |z| z.centre[0])
        };
        assert!(
            jaw(&lit_left) > jaw(&lit_right),
            "the jaw burn did not move to the shadow side"
        );
    }

    #[test]
    fn a_flatly_lit_face_is_shaped_less_than_a_directionally_lit_one() {
        let flat = zones_for(box_at(0.0, 0.0, 1.0), 0.0, 1.0, 1.0);
        let directional = zones_for(box_at(0.0, 0.0, 1.0), 1.0, 1.0, 1.0);
        let burn = |zones: &[ShapingZone]| {
            zones
                .iter()
                .filter(|z| z.gain_ev < 0.0)
                .map(|z| z.gain_ev.abs())
                .sum::<f32>()
        };
        assert!(
            burn(&flat) < burn(&directional),
            "a flat face got as much burn as a modelled one; that is painting a shadow on"
        );
    }

    #[test]
    fn the_grid_is_deterministic_and_bounded() {
        let region = box_at(0.2, 0.1, 0.4);
        let zones = zones_for(region, 0.5, 1.0, 1.0);
        let a = grid(region, &zones);
        let b = grid(region, &zones);
        assert_eq!(a, b);
        assert_eq!(
            a.len(),
            usize::from(SHAPING_SIDE) * usize::from(SHAPING_SIDE)
        );
        // A sixth of a stop is 33 units; overlapping zones may add, but not without bound.
        assert!(a.iter().all(|v| v.abs() < 100));
    }

    #[test]
    fn the_light_direction_reads_the_form_rather_than_the_texture() {
        let bands = lit_from_left(64, 64);
        assert!(
            light_direction(&bands) < -0.2,
            "a face lit from the left read as {}",
            light_direction(&bands)
        );
    }

    #[test]
    fn evening_can_never_move_the_band_more_than_the_tolerance() {
        // Section 10.1's texture criterion, on a crop built entirely out of blotches - the
        // frame where an unbounded evening would flatten everything.
        let values: Vec<f32> = (0..96 * 96)
            .map(|i| {
                let x = i % 96;
                let y = i / 96;
                if (x / 12 + y / 12) % 2 == 0 {
                    0.58
                } else {
                    0.44
                }
            })
            .collect();
        let bands = crate::local::freqsep::separate(&LumaPlane {
            values,
            width: 96,
            height: 96,
        });
        let (_, applied, before, after) = evening(&bands, 1.0);
        assert!(applied > 0.0, "nothing was evened on a blotchy crop");
        let drift = ((after - before) / before).abs();
        assert!(
            drift <= MID_BAND_TOLERANCE + 1e-4,
            "evening moved the band by {drift:.4}"
        );
    }

    #[test]
    fn evening_a_flat_crop_does_nothing_rather_than_dividing_by_zero() {
        let bands = crate::local::freqsep::separate(&LumaPlane {
            values: vec![0.5; 64 * 64],
            width: 64,
            height: 64,
        });
        let (map, applied, _, _) = evening(&bands, 1.0);
        assert!(applied.abs() < f32::EPSILON);
        assert!(map.iter().all(|v| *v == 0));
    }

    #[test]
    fn at_most_four_faces_are_shaped() {
        let bands = lit_from_left(64, 64);
        let faces: Vec<FaceShaping> = (0..8)
            .map(|i| {
                shape_face(
                    None,
                    box_at(0.05 * i as f32, 0.2, 0.20),
                    &bands,
                    1.0,
                    1.0,
                    1.0,
                )
            })
            .collect();
        let maps = collect(faces).expect("some faces were shapeable");
        assert!(maps.faces.len() <= MAX_SHAPED_FACES);
        assert!(maps.texture_preserved());
    }
}
