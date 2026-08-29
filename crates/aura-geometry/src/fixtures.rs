//! The synthetic ground truth every section 10.1 gate is measured against.
//!
//! ## What these prove, and what they do not
//!
//! Every frame here has its answer **painted into the pixels**: the tilt of a horizon, the bow of
//! a lens, the convergence of a pair of walls, the place a subject stands. A gate measured
//! against one of them proves the arithmetic recovers what was painted, which is the strongest
//! statement this repository can make about this phase - and it is not a statement about a
//! wedding.
//!
//! Section 9's DATA row asks for expert crop labels on two thousand frames and there are none
//! here. So the crop gates are statements about the *safety filter and the improvement margin*
//! rather than about whether a photographer would prefer AURA's rectangle, and
//! `docs/progress/PHASE-23-EXIT.md` carries that as a condition rather than a footnote.
//!
//! ## The protected regions are supplied rather than detected
//!
//! Phase 06's detector is a placeholder, so a fixture that needs a face in a particular place
//! puts one there. That is what makes the safety gates measurable at all, and it is also exactly
//! why they measure the filter rather than the wedding: on a real photograph in this build
//! `CropSafetyReport::considered` is zero.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{ProtectedContent, ProtectedRegion};
use aura_core::{PhotoId, SceneId};

use crate::decide::GeometryFrame;
use crate::profiles::LensExif;
use crate::straighten::Horizon;

/// The width of every fixture frame, in pixels.
///
/// Two hundred and forty by a hundred and sixty, which is 3:2 - the aspect most of a wedding is
/// shot at, and the one the resolution floor's worked example in the contract is written about.
/// Large enough that the edge band in [`crate::crop::EDGE_BAND`] is several pixels across and the
/// keystone bands are fifty rows deep; small enough that a test planning forty frames runs in
/// well under a second.
pub const WIDTH: usize = 240;

/// The height of every fixture frame, in pixels.
pub const HEIGHT: usize = 160;

/// A photo id for one fixture index.
#[must_use]
pub fn photo(index: u8) -> PhotoId {
    let text = format!("pht_00000000-0000-4000-8000-0000000023{index:02}");
    PhotoId::from_db(&text).unwrap_or_else(|_| {
        // Unreachable: the format above is a valid shape for every `u8`. A fallback rather than
        // an unwrap because this crate forbids both.
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000002300").unwrap_or_default()
    })
}

/// A flat plate with a little structure everywhere, so no term is undefined.
///
/// Not a constant field: every term of the crop objective is measured on a gradient, and a frame
/// with no gradient anywhere makes three of the four fall back on their empty-frame readings -
/// which would make a fixture that tested nothing look as though it passed.
#[must_use]
pub fn plate() -> Vec<f32> {
    let mut rgb = vec![0.0f32; WIDTH * HEIGHT * 3];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            // A slow diagonal ramp with a fine texture on it. The ramp gives the balance term
            // something to be even about and the texture gives the edge term a floor to measure
            // against.
            let ramp = 0.25 + 0.10 * (x as f32 / WIDTH as f32) + 0.05 * (y as f32 / HEIGHT as f32);
            let texture = if (x / 3 + y / 3) % 2 == 0 { 0.02 } else { -0.02 };
            for channel in 0..3 {
                if let Some(slot) = rgb.get_mut((y * WIDTH + x) * 3 + channel) {
                    *slot = (ramp + texture).clamp(0.0, 1.0);
                }
            }
        }
    }
    rgb
}

/// Paint a rectangle of high-contrast texture into a plate.
///
/// Texture rather than a flat block, because a flat block has gradient only at its boundary and
/// every measurement in this phase is made on the gradient. A subject painted as a flat block is
/// a subject the objective can see the outline of and not the middle.
pub fn paint(rgb: &mut [f32], area: Box2, bright: f32) {
    let x0 = ((area.x * WIDTH as f32).max(0.0) as usize).min(WIDTH);
    let y0 = ((area.y * HEIGHT as f32).max(0.0) as usize).min(HEIGHT);
    let x1 = (((area.x + area.w) * WIDTH as f32).max(0.0) as usize).min(WIDTH);
    let y1 = (((area.y + area.h) * HEIGHT as f32).max(0.0) as usize).min(HEIGHT);
    for y in y0..y1 {
        for x in x0..x1 {
            let value = if (x / 2 + y / 2) % 2 == 0 {
                bright
            } else {
                (bright - 0.55).max(0.02)
            };
            for channel in 0..3 {
                if let Some(slot) = rgb.get_mut((y * WIDTH + x) * 3 + channel) {
                    *slot = value;
                }
            }
        }
    }
}

/// A frame with nothing remarkable about it: no lens, no horizon, no faces.
///
/// The commonest kind of photograph in this build and the plan it produces is the commonest plan
/// in the product - `geometry_crop_kept_original`, `geometry_horizon_absent`,
/// `geometry_lens_profile_missing`.
#[must_use]
pub fn plain_frame(scene: SceneId) -> GeometryFrame {
    GeometryFrame {
        image_id: photo(0),
        scene,
        rgb: plate(),
        width: WIDTH,
        height: HEIGHT,
        full_width: 6000,
        full_height: 4000,
        lens: LensExif::default(),
        horizon: Horizon::default(),
        protected: Vec::new(),
        hint: None,
        user_edited: false,
    }
}

/// A frame whose subject sits well off to one side, with room to recompose.
///
/// The frame a crop *should* improve, and the one the improvement margin is calibrated against:
/// a search that cannot beat the original here is a search that will never fire, and one that
/// beats it by a lot is a search whose margin is doing no work.
#[must_use]
pub fn lopsided_frame(scene: SceneId) -> GeometryFrame {
    let mut rgb = plate();
    let subject = Box2 {
        x: 0.66,
        y: 0.28,
        w: 0.16,
        h: 0.44,
    };
    paint(&mut rgb, subject, 0.92);
    GeometryFrame {
        rgb,
        hint: Some(subject),
        ..plain_frame(scene)
    }
}

/// A frame with several faces scattered to the edges.
///
/// What the safety filter exists for. Every candidate rectangle tighter than about ninety per
/// cent cuts one of them, so a search that returns anything at all here has been through the
/// veto rather than around it.
#[must_use]
pub fn crowded_frame(scene: SceneId) -> GeometryFrame {
    let mut rgb = plate();
    let places = [
        (0.04f32, 0.12f32),
        (0.88, 0.15),
        (0.46, 0.06),
        (0.10, 0.78),
        (0.82, 0.80),
    ];
    let mut protected = Vec::with_capacity(places.len());
    for (index, (x, y)) in places.into_iter().enumerate() {
        let area = Box2 {
            x,
            y,
            w: 0.08,
            h: 0.12,
        };
        paint(&mut rgb, area, 0.88);
        protected.push(ProtectedRegion::anonymous(
            if index == 0 {
                ProtectedContent::PrimaryFace
            } else {
                ProtectedContent::Face
            },
            area,
        ));
    }
    GeometryFrame {
        rgb,
        protected,
        ..plain_frame(scene)
    }
}

/// A frame whose horizon is off level by a painted amount.
///
/// The tilt is in the pixels **and** in the [`Horizon`] this returns, because phase 11 is what
/// measures a horizon and this phase is what acts on one. A fixture that only painted the tilt
/// would be testing phase 11; a fixture that only declared it would be testing nothing.
#[must_use]
pub fn tilted_frame(scene: SceneId, degrees: f32, confidence: f32, intentional: bool) -> GeometryFrame {
    let mut rgb = plate();
    // A single long line at the painted angle, through the middle of the frame.
    let slope = degrees.to_radians().tan();
    for x in 0..WIDTH {
        let centred = x as f32 - WIDTH as f32 / 2.0;
        let y = HEIGHT as f32 / 2.0 + centred * slope * (WIDTH as f32 / HEIGHT as f32) / 1.5;
        for dy in -1i32..=1 {
            let py = y as i32 + dy;
            if py >= 0 && (py as usize) < HEIGHT {
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut((py as usize * WIDTH + x) * 3 + channel) {
                        *slot = 0.95;
                    }
                }
            }
        }
    }
    GeometryFrame {
        rgb,
        horizon: Horizon {
            tilt_deg: degrees,
            confidence,
            intentional,
            present: true,
        },
        ..plain_frame(scene)
    }
}

/// A frame whose upright lines converge toward the top by a painted amount.
///
/// `lean` is the share of the frame's width each side moves inward between the bottom and the
/// top, so `0.0` is a level camera and `0.10` is a camera pointed up at a building.
#[must_use]
pub fn architectural_frame(scene: SceneId, lean: f32) -> GeometryFrame {
    let mut rgb = plate();
    for y in 0..HEIGHT {
        let t = y as f32 / HEIGHT as f32;
        let inset = lean * (1.0 - t);
        for (side, base) in [(0usize, 0.18f32), (1, 0.82f32)] {
            let centre = if side == 0 { base + inset } else { base - inset };
            let x = (centre * WIDTH as f32) as isize;
            for dx in -2isize..=2 {
                let px = x + dx;
                if px >= 0 && (px as usize) < WIDTH {
                    for channel in 0..3 {
                        if let Some(slot) = rgb.get_mut((y * WIDTH + px as usize) * 3 + channel) {
                            *slot = 0.95;
                        }
                    }
                }
            }
        }
    }
    GeometryFrame {
        rgb,
        ..plain_frame(scene)
    }
}

/// A frame shot on a lens the bundled table has a row for.
#[must_use]
pub fn profiled_frame(scene: SceneId, lens: &str, focal_mm: f32) -> GeometryFrame {
    let mut rgb = plate();
    // High-contrast structure out at the corners, so the chromatic aberration correction has
    // something to be verified against. Without it the plan is `geometry_lens_ca_unverifiable`,
    // which is correct and is not what a lens fixture is trying to test.
    for area in [
        Box2 {
            x: 0.02,
            y: 0.02,
            w: 0.16,
            h: 0.20,
        },
        Box2 {
            x: 0.82,
            y: 0.78,
            w: 0.16,
            h: 0.20,
        },
    ] {
        paint(&mut rgb, area, 0.95);
    }
    GeometryFrame {
        rgb,
        lens: LensExif {
            name: lens.to_string(),
            focal_mm: Some(focal_mm),
            embedded: false,
        },
        ..plain_frame(scene)
    }
}

/// A frame whose camera wrote its own corrections into the file.
#[must_use]
pub fn embedded_frame(scene: SceneId) -> GeometryFrame {
    GeometryFrame {
        lens: LensExif {
            name: "some lens the camera knows about".to_string(),
            focal_mm: Some(50.0),
            embedded: true,
        },
        ..plain_frame(scene)
    }
}

/// A small synthetic wedding, for the pass-level gates.
///
/// Twenty-four frames, and **the mixture is the measurement**. Section 10.1's conservatism gate -
/// "most frames (>= 70 %) keep their original framing" - is a statement about a wedding, so it is
/// only meaningful over a set of frames weighted the way a wedding is weighted. A fixture made of
/// the cases this phase finds interesting would measure how often the interesting cases fire,
/// which is a different number that happens to look like the same one.
///
/// So the proportions below are chosen to resemble what a photographer actually delivers rather
/// than what a solver wants to be given:
///
/// | Kind | Frames | Why that many |
/// |---|---|---|
/// | plain | 15 | Most of a wedding is level, shot on an unremarkable lens, framed by somebody who meant it. |
/// | crowded | 4 | Faces at the edges - the formals and the dance floor. The safety filter's denominator. |
/// | lopsided | 3 | Frames with real room to recompose. About one in eight, which is generous. |
/// | tilted | 1 | Off level *and* confidently measured *and* inside the band. Rare: most tilt is either negligible or deliberate. |
/// | architectural | 1 | Converging verticals with no people in front of them. Rarer still at a wedding. |
///
/// A wedding where the last three rows were much larger would be a wedding where something had
/// gone wrong on the day, and a fixture that pretended otherwise would turn a passing gate into a
/// claim nobody should make.
#[must_use]
pub fn wedding() -> Vec<GeometryFrame> {
    /// What each frame is, in the order the day happens.
    const RECIPE: [(SceneId, Kind); 24] = [
        (SceneId::GettingReadyBride, Kind::Lopsided),
        (SceneId::GettingReadyBride, Kind::Plain),
        (SceneId::Details, Kind::Plain),
        (SceneId::Details, Kind::Plain),
        (SceneId::CeremonyEntrance, Kind::Architectural),
        (SceneId::Ceremony, Kind::Crowded),
        (SceneId::Ceremony, Kind::Plain),
        (SceneId::Ceremony, Kind::Plain),
        (SceneId::Ceremony, Kind::Plain),
        (SceneId::Vows, Kind::Plain),
        (SceneId::Rings, Kind::Plain),
        (SceneId::FamilyPortrait, Kind::Crowded),
        (SceneId::FamilyPortrait, Kind::Crowded),
        (SceneId::FamilyPortrait, Kind::Plain),
        (SceneId::CouplePortrait, Kind::Lopsided),
        (SceneId::CouplePortrait, Kind::Plain),
        (SceneId::CouplePortrait, Kind::Plain),
        (SceneId::Speeches, Kind::Plain),
        (SceneId::Speeches, Kind::Tilted),
        (SceneId::FirstDance, Kind::Plain),
        (SceneId::DanceFloor, Kind::Crowded),
        (SceneId::DanceFloor, Kind::Plain),
        (SceneId::DanceFloor, Kind::Plain),
        (SceneId::Candid, Kind::Lopsided),
    ];

    RECIPE
        .into_iter()
        .enumerate()
        .map(|(index, (scene, kind))| {
            let mut frame = match kind {
                Kind::Plain => plain_frame(scene),
                Kind::Lopsided => lopsided_frame(scene),
                Kind::Crowded => crowded_frame(scene),
                Kind::Tilted => tilted_frame(scene, 2.4, 0.82, false),
                Kind::Architectural => architectural_frame(scene, 0.09),
            };
            frame.image_id = photo(u8::try_from(index).unwrap_or(0));
            frame
        })
        .collect()
}

/// Which fixture a frame of the synthetic wedding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Level, unremarkable, framed on purpose. Most of a wedding.
    Plain,
    /// A subject well off to one side, with room to recompose.
    Lopsided,
    /// Faces scattered to the edges.
    Crowded,
    /// Off level, confidently, inside the band this phase acts on.
    Tilted,
    /// Converging verticals with nobody in front of them.
    Architectural,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_frame_carries_the_pixels_it_claims_to() {
        for frame in [
            plain_frame(SceneId::Candid),
            lopsided_frame(SceneId::Candid),
            crowded_frame(SceneId::DanceFloor),
            tilted_frame(SceneId::Candid, 3.0, 0.9, false),
            architectural_frame(SceneId::Venue, 0.10),
            profiled_frame(SceneId::Candid, "EF24-70mm f/2.8L II USM", 35.0),
            embedded_frame(SceneId::Candid),
        ] {
            assert_eq!(frame.rgb.len(), WIDTH * HEIGHT * 3);
            assert!(frame.rgb.iter().all(|v| (0.0..=1.0).contains(v)));
        }
    }

    #[test]
    fn the_fixture_wedding_is_the_size_and_shape_it_says_it_is() {
        let wedding = wedding();
        assert_eq!(wedding.len(), 24);
        // Distinct ids, or a store would collapse them and every pass-level gate would be
        // measured over one frame.
        let ids: std::collections::BTreeSet<_> = wedding.iter().map(|f| f.image_id).collect();
        assert_eq!(ids.len(), wedding.len());
        // And it is mostly the scenes a wedding is mostly made of.
        let ceremony = wedding
            .iter()
            .filter(|f| {
                matches!(
                    f.scene,
                    SceneId::Ceremony | SceneId::Vows | SceneId::Rings | SceneId::CeremonyEntrance
                )
            })
            .count();
        assert!(ceremony >= 6, "{ceremony}");
        // The mixture is the measurement: a fixture wedding where most frames were interesting
        // would turn section 10.1's conservatism gate into a claim about the solver rather than
        // about a wedding. Fifteen of the twenty-four have nothing remarkable about them.
        let plain = wedding
            .iter()
            .filter(|f| f.protected.is_empty() && !f.horizon.present && f.hint.is_none())
            .count();
        assert!(plain >= 15, "only {plain} frames are unremarkable");
    }

    #[test]
    fn a_tilted_fixture_has_its_tilt_in_the_pixels_as_well_as_in_the_declaration() {
        // The line is painted, so the frame's own structure is asymmetric about the horizontal.
        // A fixture that only declared its tilt would let a broken projection pass every gate.
        let level = tilted_frame(SceneId::Candid, 0.0, 0.9, false);
        let tilted = tilted_frame(SceneId::Candid, 6.0, 0.9, false);
        assert_ne!(level.rgb, tilted.rgb);
        assert!((tilted.horizon.tilt_deg - 6.0).abs() < 1e-6);
    }

    #[test]
    fn an_architectural_fixture_converges_and_a_level_one_does_not() {
        let luma = |frame: &GeometryFrame| {
            aura_render::spatial::luma_plane(&frame.rgb, WIDTH, HEIGHT)
        };
        let leaning = architectural_frame(SceneId::Venue, 0.10);
        let level = architectural_frame(SceneId::Venue, 0.0);
        let (gx, gy) = aura_render::spatial::sobel_planes(&luma(&leaning), WIDTH, HEIGHT);
        let measured = crate::keystone::measure(&gx, &gy, WIDTH, HEIGHT, &[]);
        let (gx, gy) = aura_render::spatial::sobel_planes(&luma(&level), WIDTH, HEIGHT);
        let flat = crate::keystone::measure(&gx, &gy, WIDTH, HEIGHT, &[]);
        assert!(
            measured.convergence > flat.convergence + 0.2,
            "{} !> {}",
            measured.convergence,
            flat.convergence
        );
    }
}
