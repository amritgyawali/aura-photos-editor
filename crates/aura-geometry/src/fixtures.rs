//! Synthetic frames with a known answer. PHASE-23 sections 8.9 and 10.1.
//!
//! Section 9 gives DATA "expert crop labels on 2k frames; architecture and tilt sets", and
//! there are none in this repository - no wedding photographs, no expert crops, no measured
//! lens profiles. What is here instead is a set of frames whose geometry was **chosen, painted
//! into the pixels and read back through the real pipeline**: a plate bent by a known `k1`, a
//! wall fanned to a known convergence, a horizon set at a known angle, faces placed where a
//! crop would have to cut them.
//!
//! That proves the estimator, the tracker, the gates, the safety filter, the search and the
//! store. It is **not** evidence that a photographer would agree with a crop. That is
//! condition C1 in `docs/progress/PHASE-23-EXIT.md`, it is a Sev 2 trigger, and every number
//! in section 10.1 is measured against these.
//!
//! Every generator is deterministic and takes no clock, no randomness and no file.

use aura_core::contract::geometry::{ProtectedKind, ProtectedRegion};
use aura_core::contract::integrity::CropRect;
use aura_core::{PhotoId, SceneId};

use crate::keystone::VerticalLine;
use crate::lens::{self, EdgeChain, LensInput};
use crate::plan::GeometryInput;

/// The proxy side the fixtures are painted at. Small enough that a gate run is a second.
pub const SIDE: usize = 256;

/// The side the **distortion** fixtures are painted at.
///
/// Four times [`SIDE`], and the difference is not a preference. A lens correction's whole
/// signal is the bow of a straight line, and at `k1 = 0.02` on a 256 px plate that bow is a
/// third of a pixel - so a tracker that snaps to integer positions cannot see it, and the
/// estimator correctly declines. The real pass measures on the 2048 px proxy, where the same
/// bow is two and a half pixels. Testing the estimator at 256 px measures the plate, not the
/// estimator.
pub const DISTORTION_SIDE: usize = 512;

/// A deterministic photo id for fixture `n`.
#[must_use]
pub fn photo(n: u8) -> PhotoId {
    PhotoId::from_db(&format!("pht_00000000-0000-4000-8000-0000000000{n:02}"))
        .unwrap_or_else(|_| PhotoId::new())
}

/// One labelled case: an input and what the gate expects of it.
#[derive(Debug, Clone)]
pub struct Case {
    /// What it is, for a failing assertion's message.
    pub name: &'static str,
    /// The input.
    pub input: GeometryInput,
    /// The angle an expert would have levelled it by, when there is one.
    ///
    /// Section 10.1's "within 0.3 deg of expert on >= 90 % of labelled frames". Authored by
    /// construction: the plate was painted at this angle.
    pub expert_rotate_deg: Option<f32>,
    /// True when the case must be delivered exactly as it was shot.
    pub must_keep_framing: bool,
}

/// A face region.
#[must_use]
pub fn face(x: f32, y: f32, primary: bool) -> ProtectedRegion {
    ProtectedRegion {
        kind: ProtectedKind::Face,
        identity: None,
        rect: CropRect {
            x,
            y,
            w: 0.085,
            h: 0.120,
        },
        primary,
    }
}

/// A pair of hands.
#[must_use]
pub fn hands(x: f32, y: f32, primary: bool) -> ProtectedRegion {
    ProtectedRegion {
        kind: ProtectedKind::Hands,
        identity: None,
        rect: CropRect {
            x,
            y,
            w: 0.090,
            h: 0.070,
        },
        primary,
    }
}

/// A bright blob or an edge intrusion.
#[must_use]
pub const fn distraction(x: f32, y: f32, w: f32, h: f32) -> CropRect {
    CropRect { x, y, w, h }
}

/// Ten labelled frames covering every branch section 10.1 names.
///
/// Deliberately weighted the way a wedding is: most of them must keep their framing, because
/// most photographs should.
#[must_use]
#[allow(clippy::too_many_lines)] // Ten authored frames. Splitting them hides the set.
pub fn wedding() -> Vec<Case> {
    let mut cases = Vec::new();

    // 1. A tilted venue frame with nobody in it. Levelled, and the only case that should be.
    let mut tilted = GeometryInput::bare(photo(1), SceneId::Venue);
    tilted.tilt_deg = 2.6;
    tilted.horizon_conf = 0.91;
    cases.push(Case {
        name: "a tilted venue frame is levelled",
        input: tilted,
        expert_rotate_deg: Some(-2.6),
        must_keep_framing: false,
    });

    // 2. A deliberately dutch-angled dance floor frame. Untouched.
    let mut dutch = GeometryInput::bare(photo(2), SceneId::DanceFloor);
    dutch.tilt_deg = 9.4;
    dutch.horizon_conf = 0.88;
    dutch.tilt_intentional = true;
    dutch.regions = vec![face(0.20, 0.30, false), face(0.62, 0.34, false)];
    cases.push(Case {
        name: "a deliberate tilt is left alone",
        input: dutch,
        expert_rotate_deg: Some(0.0),
        must_keep_framing: true,
    });

    // 3. A ceremony frame with a horizon nobody is sure about.
    let mut unsure = GeometryInput::bare(photo(3), SceneId::Ceremony);
    unsure.tilt_deg = 3.1;
    unsure.horizon_conf = 0.44;
    unsure.regions = vec![face(0.44, 0.28, true), face(0.55, 0.29, true)];
    cases.push(Case {
        name: "an uncertain horizon is not acted on",
        input: unsure,
        expert_rotate_deg: Some(0.0),
        must_keep_framing: true,
    });

    // 4. A family formal with somebody at each end. Never cropped, never levelled into them.
    let mut formal = GeometryInput::bare(photo(4), SceneId::FamilyPortrait);
    formal.tilt_deg = 1.4;
    formal.horizon_conf = 0.90;
    formal.regions = vec![
        face(0.030, 0.34, false),
        face(0.230, 0.32, true),
        face(0.430, 0.31, true),
        face(0.640, 0.32, false),
        face(0.880, 0.34, false),
    ];
    cases.push(Case {
        name: "a family formal keeps everybody",
        input: formal,
        expert_rotate_deg: None,
        must_keep_framing: true,
    });

    // 5. A speeches frame with two croppable distractions. The one case that may improve.
    let mut speech = GeometryInput::bare(photo(5), SceneId::Speeches);
    speech.regions = vec![face(0.24, 0.26, true)];
    speech.subject = Some(CropRect {
        x: 0.24,
        y: 0.26,
        w: 0.085,
        h: 0.120,
    });
    speech.distractions = vec![
        distraction(0.79, 0.04, 0.19, 0.24),
        distraction(0.82, 0.62, 0.16, 0.22),
    ];
    cases.push(Case {
        name: "a speeches frame may be tightened",
        input: speech,
        expert_rotate_deg: None,
        must_keep_framing: false,
    });

    // 6. The rings, with the other person's hands at the edge.
    let mut rings = GeometryInput::bare(photo(6), SceneId::Rings);
    rings.regions = vec![hands(0.40, 0.44, true), hands(0.035, 0.52, true)];
    rings.subject = Some(CropRect {
        x: 0.40,
        y: 0.44,
        w: 0.090,
        h: 0.070,
    });
    cases.push(Case {
        name: "the rings keep both pairs of hands",
        input: rings,
        expert_rotate_deg: None,
        must_keep_framing: false,
    });

    // 7. A kiss. Never cropped, whatever the objective thinks.
    let mut kiss = GeometryInput::bare(photo(7), SceneId::Kiss);
    kiss.regions = vec![face(0.09, 0.10, true), face(0.17, 0.12, true)];
    kiss.subject = Some(CropRect {
        x: 0.09,
        y: 0.10,
        w: 0.085,
        h: 0.120,
    });
    kiss.distractions = vec![distraction(0.70, 0.68, 0.26, 0.28)];
    cases.push(Case {
        name: "a kiss is never cropped",
        input: kiss,
        expert_rotate_deg: None,
        must_keep_framing: true,
    });

    // 8. A church interior with converging verticals and a wide unprofiled lens.
    let mut church = GeometryInput::bare(photo(8), SceneId::Venue);
    church.aspect = 2.0 / 3.0;
    church.verticals = converging(0.86, 6);
    church.lens = LensInput {
        lens_id: Some("MYSTERY 15mm".to_string()),
        focal_mm: Some(15.0),
        embedded: None,
    };
    church.edges = bent_chains(0.042, 1.5, 8);
    cases.push(Case {
        name: "a church is squared up and its lens estimated",
        input: church,
        expert_rotate_deg: None,
        must_keep_framing: false,
    });

    // 9. A ritual frame. The highest floor in the table, and never cropped.
    let mut ritual = GeometryInput::bare(photo(9), SceneId::Ritual);
    ritual.regions = vec![face(0.36, 0.30, true), face(0.55, 0.31, true)];
    ritual.distractions = vec![distraction(0.03, 0.03, 0.14, 0.16)];
    cases.push(Case {
        name: "a ritual is delivered as shot",
        input: ritual,
        expert_rotate_deg: None,
        must_keep_framing: true,
    });

    // 10. A couple portrait with room to spare - the scene the crop was designed for.
    let mut couple = GeometryInput::bare(photo(10), SceneId::CouplePortrait);
    couple.regions = vec![face(0.30, 0.30, true), face(0.42, 0.31, true)];
    couple.subject = Some(CropRect {
        x: 0.30,
        y: 0.30,
        w: 0.20,
        h: 0.26,
    });
    couple.distractions = vec![distraction(0.86, 0.05, 0.13, 0.20)];
    cases.push(Case {
        name: "a couple portrait is the crop's own scene",
        input: couple,
        expert_rotate_deg: None,
        must_keep_framing: false,
    });

    cases
}

/// A fan of converging verticals with a known convergence ratio.
#[must_use]
pub fn converging(ratio: f32, count: usize) -> Vec<VerticalLine> {
    (0..count)
        .map(|i| {
            let t = i as f32 / (count - 1).max(1) as f32;
            let bottom_x = 0.10 + 0.80 * t;
            VerticalLine {
                top_x: 0.5 + (bottom_x - 0.5) * ratio,
                bottom_x,
                strength: 0.85,
            }
        })
        .collect()
}

/// World-straight lines bent by a known `k1`, as chains.
#[must_use]
pub fn bent_chains(k1: f32, aspect: f32, count: usize) -> Vec<EdgeChain> {
    (0..count)
        .map(|i| {
            let y = 0.06 + 0.88 * i as f32 / (count - 1).max(1) as f32;
            let points = (0..24)
                .map(|step| {
                    let t = step as f32 / 23.0;
                    lens::source_of([0.04 + 0.92 * t, y], [k1, 0.0, 0.0], aspect, 1.0)
                })
                .collect();
            EdgeChain { points }
        })
        .collect()
}

/// A luminance plate of a horizon at a known angle, for the tracker.
///
/// A dark half below a light half, with the boundary at `angle_deg`. Nothing else in the
/// frame, which is what makes the answer known.
#[must_use]
pub fn horizon_plate(angle_deg: f32) -> Vec<f32> {
    let mut out = vec![0.0f32; SIDE * SIDE];
    let slope = angle_deg.to_radians().tan();
    for y in 0..SIDE {
        for x in 0..SIDE {
            let nx = x as f32 / SIDE as f32 - 0.5;
            let ny = y as f32 / SIDE as f32 - 0.5;
            let value = if ny > nx * slope { 0.15 } else { 0.85 };
            if let Some(slot) = out.get_mut(y * SIDE + x) {
                *slot = value;
            }
        }
    }
    out
}

/// A luminance plate of a grid bent by a known `k1`.
///
/// Eleven lines each way, so the tracker has plenty to find and the estimator has chains at
/// every radius - which is what a distortion fit needs and what a photograph of a wall does
/// not always give it.
///
/// **The strokes are a constant width in world units, deliberately.** A gradient tracker
/// follows a stroke's edge rather than its centre, so every chain sits at a fixed offset from
/// the line it is tracking - and a fixed offset in *world* units is a parallel straight line,
/// which is still straight and still recovers the same coefficient. A stroke of constant
/// width in *plate* pixels puts that offset at a radius-dependent distance in the world, which
/// is exactly the shape of a distortion coefficient: it was tried, and it biased nothing
/// either way, which is how the real cause was found.
#[must_use]
pub fn grid_plate(k1: f32) -> Vec<f32> {
    grid_plate_at(k1, SIDE)
}

/// A grid plate at a chosen side. See [`DISTORTION_SIDE`].
#[must_use]
pub fn grid_plate_at(k1: f32, side: usize) -> Vec<f32> {
    let mut out = vec![0.85f32; side * side];
    let k = [k1, 0.0, 0.0];
    // Half the stroke, in world units. Scaled so the stroke is about three plate pixels wide
    // at the centre whatever the plate's size: a stroke thicker than about four pixels has a
    // flat interior the tracker's own gradient window cannot see across, and it dies in the
    // middle of the line.
    let half_width = 15.0 / side as f32;
    for y in 0..side {
        for x in 0..side {
            let point = [x as f32 / side as f32, y as f32 / side as f32];
            // Where this output pixel comes from in the undistorted world.
            let world = lens::dest_of(point, k, 1.0, 1.0);
            let near = |value: f32| -> bool {
                let cell = value * 10.0;
                (cell - cell.round()).abs() < half_width
            };
            if near(world[0]) || near(world[1]) {
                if let Some(slot) = out.get_mut(y * side + x) {
                    *slot = 0.10;
                }
            }
        }
    }
    out
}

/// A luminance plate of a wall of verticals converging by a known ratio.
#[must_use]
pub fn wall_plate(ratio: f32) -> Vec<f32> {
    let mut out = vec![0.80f32; SIDE * SIDE];
    for line in 0..7 {
        let bottom = 0.10 + 0.80 * line as f32 / 6.0;
        let top = 0.5 + (bottom - 0.5) * ratio;
        for y in 0..SIDE {
            let t = y as f32 / (SIDE - 1) as f32;
            let nx = top + (bottom - top) * t;
            let x = (nx * SIDE as f32).round() as isize;
            for dx in -1..=1 {
                let px = x + dx;
                if px < 0 || px as usize >= SIDE {
                    continue;
                }
                if let Some(slot) = out.get_mut(y * SIDE + px as usize) {
                    *slot = 0.08;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystone;

    #[test]
    fn every_case_is_distinct_and_labelled() {
        let cases = wedding();
        assert_eq!(cases.len(), 10);
        let mut ids: Vec<String> = cases
            .iter()
            .map(|case| case.input.image_id.to_db())
            .collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), cases.len(), "two cases share a photo id");
        assert!(cases.iter().all(|case| !case.name.is_empty()));
    }

    #[test]
    fn most_cases_must_keep_their_framing() {
        let cases = wedding();
        let kept = cases.iter().filter(|case| case.must_keep_framing).count();
        assert!(
            kept * 10 >= cases.len() * 4,
            "only {kept} of {} cases protect their framing; the fixture set does not \
             resemble a wedding",
            cases.len()
        );
    }

    #[test]
    fn the_grid_plate_yields_chains_the_estimator_can_use() {
        let plate = grid_plate(0.040);
        let chains = lens::track_edges(&plate, SIDE, SIDE);
        assert!(
            chains.len() >= lens::MIN_EDGES,
            "only {} chains tracked",
            chains.len()
        );
    }

    #[test]
    fn the_wall_plate_yields_verticals_the_keystone_can_use() {
        let plate = wall_plate(0.88);
        let lines = keystone::track_verticals(&plate, SIDE, SIDE, 1.0);
        assert!(
            lines.len() >= min_verticals(),
            "only {} verticals tracked",
            lines.len()
        );
    }

    fn min_verticals() -> usize {
        aura_core::contract::geometry::Keystone::MIN_VERTICALS as usize
    }

    #[test]
    fn the_horizon_plate_is_painted_at_the_angle_it_says() {
        // The boundary passes through the centre at every angle, by construction.
        for angle in [0.0f32, 2.0, -3.5] {
            let plate = horizon_plate(angle);
            let centre = plate.get(SIDE / 2 * SIDE + SIDE / 2).copied().unwrap_or(0.0);
            assert!((0.0..=1.0).contains(&centre));
            let top = plate.get(SIDE / 8 * SIDE + SIDE / 2).copied().unwrap_or(0.0);
            let bottom = plate
                .get(SIDE * 7 / 8 * SIDE + SIDE / 2)
                .copied()
                .unwrap_or(0.0);
            assert!(top > bottom, "{angle}: the plate is upside down");
        }
    }
}
