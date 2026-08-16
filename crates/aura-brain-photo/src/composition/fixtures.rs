//! The synthetic ground truth every section 10.1 composition gate is measured against.
//!
//! There are no camera files in this repository - phase 02's first exit condition, still
//! open - and no labelled composition data. Every number in section 10.1 therefore has to
//! be measured against something whose answer is known by construction, and this module is
//! that something.
//!
//! **Read what that does and does not prove.** A horizon drawn at exactly 3.2 degrees has
//! a known angle, so an angle error of 0.05 degrees is a real result about the estimator.
//! A joint marker painted at a known place has a known position, so a limb-crop F1 of
//! 1.000 is a real result about the *audit* - the derived person box, the joint mapping,
//! the edge tests, the severity arithmetic and the scene weighting - and says nothing
//! whatever about the keypoint head's weights, which are a placeholder. Phase 06 drew
//! exactly this line with its `ReferenceDetector`, phase 09 with its eye markers and phase
//! 10 with its painted expressions; this is the fourth time and the honesty is the same.
//!
//! ## The horizon fixtures are anti-aliased, and that is the whole gate
//!
//! Section 10.1's first gate is 0.4 degrees of angle error. A hard-edged line drawn by
//! rounding pixel coordinates has a staircase on it, and the staircase has its own
//! orientation - so a gate measured against one would be measuring the rounding rather
//! than the estimator. [`Canvas::draw_horizon`] computes a signed distance to the true line
//! and shades the boundary pixels by it, which is what puts the real angle back into the
//! gradients.
//!
//! ## The joint markers are a colour no photograph carries
//!
//! Full red with zero green and zero blue. `ReferencePose::read` matches on exactly that,
//! and a marker convention an ordinary proxy pixel could satisfy would make every gate
//! here meaningless.
//!
//! ## One person per frame
//!
//! `ReferencePose::read` scans the whole frame for markers rather than the person box, so
//! a two-person fixture would mix their joints. Every builder here paints one figure. The
//! multi-person path is exercised by `aura-cli verify --phase 11` against the real head.

use aura_core::contract::composition::Box2;
use aura_core::contract::people::FaceRef;
use aura_core::SceneId;

use crate::composition::keypoints::Joint17;
use crate::fixtures::face_salted;

/// Side of every synthetic frame.
///
/// 512, matching `crate::fixtures::SIDE`. Every analyser in this module tree is
/// resolution-independent by construction - the horizon histogram normalises by total
/// weight, the balance grid by cell count, the crop audit by the person's own span - and a
/// 512 px fixture runs sixteen times faster than a 2048 px one in a suite that builds
/// several hundred of them.
pub const SIDE: usize = 512;

/// Radius of a painted joint marker, in pixels.
pub const MARKER_RADIUS: usize = 3;

/// A synthetic frame and what is true about it.
#[derive(Debug, Clone)]
pub struct Canvas {
    /// Interleaved 8-bit sRGB.
    pub rgb: Vec<u8>,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// The faces phase 06 would have found.
    pub faces: Vec<FaceRef>,
    /// What the builder knows and the analyser is being asked to recover.
    pub truth: Truth,
}

/// What a fixture's builder knows.
///
/// Five `bool`s, which the pedantic lint dislikes and which is the right shape here: each
/// one is an independent fact about a *different* measurement, and gathering them into a
/// sub-struct would put a level of nesting between a test and the thing it is asserting.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Truth {
    /// The angle the horizon was drawn at, in degrees, positive down-to-the-right.
    pub tilt_deg: f32,
    /// True when a horizon was drawn at all.
    pub has_horizon: bool,
    /// The joint the frame edge was built to cut, when it was built to cut one.
    pub cut_joint: Option<aura_core::contract::composition::Joint>,
    /// True when the cut was built to land on the joint rather than between joints.
    pub cut_at_joint: bool,
    /// True when the crown was built to be outside the frame.
    pub head_cropped: bool,
    /// True when a vertical structure was drawn above the head.
    pub head_merge: bool,
    /// True when a bright region was drawn in the background.
    pub bright_blob: bool,
    /// The scene the fixture is meant to be judged in.
    pub scene: SceneId,
}

impl Canvas {
    /// A quiet, low-texture frame with nothing in it.
    ///
    /// Deliberately *low* texture rather than the flat grey a first version used. A frame
    /// with no gradients anywhere produces no horizon votes at all, so
    /// `horizon::accumulate` returns `None` and every fixture built on it would test the
    /// unmeasured path rather than the measured one. A gentle diagonal weave gives the
    /// accumulator something to be uncertain about, which is what an ordinary photograph
    /// gives it.
    #[must_use]
    pub fn quiet() -> Self {
        let mut rgb = vec![0u8; SIDE * SIDE * 3];
        for y in 0..SIDE {
            for x in 0..SIDE {
                let weave = ((x / 7 + y / 11) % 2) as f32 * 6.0;
                let value = (118.0 + weave).clamp(0.0, 255.0) as u8;
                write(&mut rgb, x, y, [value, value, value]);
            }
        }
        Self {
            rgb,
            width: SIDE,
            height: SIDE,
            faces: Vec::new(),
            truth: Truth::default(),
        }
    }

    /// Draw an anti-aliased straight boundary across the frame.
    ///
    /// `degrees` is positive down-to-the-right, which is the convention
    /// `CompositionResult::tilt_deg` reports: the gradient of a boundary with slope
    /// `tan(theta)` points at `atan2(1, -tan(theta))`, and `horizon::analyse` turns that
    /// by ninety degrees into `theta`.
    ///
    /// The shading is a signed distance ramp one pixel wide, which is what carries the
    /// true angle into the Sobel gradients and makes a 0.4 degree gate meaningful.
    pub fn draw_horizon(&mut self, degrees: f32, above: u8, below: u8) {
        let slope = degrees.to_radians().tan();
        let centre_x = self.width as f32 / 2.0;
        let centre_y = self.height as f32 / 2.0;
        // The perpendicular scale, so the ramp is one pixel wide measured across the line
        // rather than one pixel of `y`.
        let normalise = 1.0 / (1.0 + slope * slope).sqrt();
        for y in 0..self.height {
            for x in 0..self.width {
                let on_line = centre_y + slope * (x as f32 - centre_x);
                let distance = (y as f32 - on_line) * normalise;
                let mix = (distance + 0.5).clamp(0.0, 1.0);
                let value = f32::from(above) + (f32::from(below) - f32::from(above)) * mix;
                let value = value.clamp(0.0, 255.0) as u8;
                write(&mut self.rgb, x, y, [value, value, value]);
            }
        }
        self.truth.tilt_deg = degrees;
        self.truth.has_horizon = true;
    }

    /// Fill one normalised rectangle with a flat colour.
    pub fn fill(&mut self, area: Box2, colour: [u8; 3]) {
        let (x0, y0, x1, y1) = pixels(area, self.width, self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                write(&mut self.rgb, x, y, colour);
            }
        }
    }

    /// Fill one normalised rectangle with a textured subject.
    pub fn fill_textured(&mut self, area: Box2) {
        let (x0, y0, x1, y1) = pixels(area, self.width, self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let fine = ((x / 4 + y / 4) % 2) as f32 * 26.0;
                let coarse = ((x / 16 + y / 16) % 2) as f32 * 18.0;
                let value = (110.0 + fine + coarse).clamp(0.0, 255.0) as u8;
                write(&mut self.rgb, x, y, [value, value, value]);
            }
        }
    }

    /// Paint one joint marker at a normalised position.
    ///
    /// Silently does nothing when the position is outside the frame, which is a real case
    /// rather than an error: a joint the frame cut is a joint with no pixels, and the crop
    /// audit's answer for it comes from the derived person box rather than from a marker.
    pub fn mark(&mut self, x: f32, y: f32) {
        let cx = (x * self.width as f32).round();
        let cy = (y * self.height as f32).round();
        if cx < 0.0 || cy < 0.0 {
            return;
        }
        let cx = cx as usize;
        let cy = cy as usize;
        for dy in 0..=MARKER_RADIUS * 2 {
            for dx in 0..=MARKER_RADIUS * 2 {
                let px = (cx + dx).saturating_sub(MARKER_RADIUS);
                let py = (cy + dy).saturating_sub(MARKER_RADIUS);
                if px >= self.width || py >= self.height {
                    continue;
                }
                write(&mut self.rgb, px, py, [255, 0, 0]);
            }
        }
    }

    /// Register a face box, which is what the person box is derived from.
    pub fn add_face(&mut self, bbox: Box2, salt: i32) {
        self.fill_textured(bbox);
        self.faces.push(face_salted(bbox, salt));
    }

    /// Paint a standing figure whose joints are at known normalised positions.
    ///
    /// The seventeen markers are painted **in `Joint17::ALL` order down the frame**, which
    /// is what `ReferencePose::read` recovers: it sorts markers by `y` and then by `x`. Any
    /// joint whose position falls outside the frame is skipped, and the reader simply finds
    /// fewer markers - which is the honest representation of a joint the frame cut.
    pub fn paint_figure(&mut self, joints: &[(Joint17, f32, f32)]) {
        for (_, x, y) in joints {
            self.mark(*x, *y);
        }
    }
}

/// A figure whose joints run from `head_y` to `feet_y` down the centre of the frame.
///
/// The proportions are the ones [`crate::composition::keypoints::person_box`] assumes, so
/// a fixture built here and a person box derived from its face agree - which is what makes
/// the crop audit's severity arithmetic measurable rather than merely exercised.
#[must_use]
pub fn standing_joints(centre_x: f32, head_y: f32, feet_y: f32) -> Vec<(Joint17, f32, f32)> {
    let span = feet_y - head_y;
    let at = |fraction: f32| head_y + span * fraction;
    let narrow = 0.035f32;
    let wide = 0.075f32;
    vec![
        (Joint17::Nose, centre_x, at(0.03)),
        (Joint17::LeftEye, centre_x - 0.018, at(0.02)),
        (Joint17::RightEye, centre_x + 0.018, at(0.02)),
        (Joint17::LeftEar, centre_x - 0.034, at(0.04)),
        (Joint17::RightEar, centre_x + 0.034, at(0.04)),
        (Joint17::LeftShoulder, centre_x - wide, at(0.16)),
        (Joint17::RightShoulder, centre_x + wide, at(0.16)),
        (Joint17::LeftElbow, centre_x - wide, at(0.32)),
        (Joint17::RightElbow, centre_x + wide, at(0.32)),
        (Joint17::LeftWrist, centre_x - narrow, at(0.46)),
        (Joint17::RightWrist, centre_x + narrow, at(0.46)),
        (Joint17::LeftHip, centre_x - narrow, at(0.52)),
        (Joint17::RightHip, centre_x + narrow, at(0.52)),
        (Joint17::LeftKnee, centre_x - narrow, at(0.74)),
        (Joint17::RightKnee, centre_x + narrow, at(0.74)),
        (Joint17::LeftAnkle, centre_x - narrow, at(0.96)),
        (Joint17::RightAnkle, centre_x + narrow, at(0.96)),
    ]
}

// ---------------------------------------------------------------------------
// The builders
// ---------------------------------------------------------------------------

/// A frame with a horizon at a known angle and nothing else.
///
/// The gate: section 10.1's "horizon angle error <= 0.4 deg on labelled architecture and
/// seascape-style fixtures".
#[must_use]
pub fn tilted(degrees: f32) -> Canvas {
    let mut canvas = Canvas::quiet();
    canvas.draw_horizon(degrees, 60, 190);
    canvas.truth.scene = SceneId::Venue;
    canvas
}

/// The ladder section 10.1's first gate is measured over.
///
/// Both signs and both magnitudes, because the sign convention is the single easiest thing
/// in the phase to invert and a one-sided ladder would not catch it.
#[must_use]
pub fn tilt_ladder() -> Vec<Canvas> {
    [-8.0f32, -3.2, -0.8, 0.0, 0.8, 3.2, 8.0]
        .into_iter()
        .map(tilted)
        .collect()
}

/// A large tilt with a centred subject and no horizon, in a scene that permits it.
///
/// The gate: "intentional dutch angles are not flagged as tilt errors (dedicated fixture
/// set)". Every one of section 6.1's four conditions is arranged to hold, and
/// [`accidental_dutch`] is the same frame with one of them broken.
#[must_use]
pub fn intentional_dutch() -> Canvas {
    let mut canvas = Canvas::quiet();
    // A diagonal weave rather than a boundary: plenty of edge energy, no dominant
    // direction, so the horizon confidence lands below `horizon::STRONG_HORIZON`.
    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let a = ((x + y) / 5 % 2) as f32 * 22.0;
            let b = ((x * 2 + y * 3) / 9 % 2) as f32 * 18.0;
            let c = ((x * 3 + y) / 7 % 2) as f32 * 14.0;
            let value = (96.0 + a + b + c).clamp(0.0, 255.0) as u8;
            write(&mut canvas.rgb, x, y, [value, value, value]);
        }
    }
    canvas.add_face(
        Box2 {
            x: 0.42,
            y: 0.40,
            w: 0.16,
            h: 0.18,
        },
        11,
    );
    canvas.truth.scene = SceneId::DanceFloor;
    canvas
}

/// The same tilt with a *strong* horizon, in a scene that does not permit a dutch angle.
///
/// The negative half of the intentional-tilt gate. A fixture set that only proves the
/// product does not flag a dutch angle proves nothing: a build that flagged nothing at all
/// would pass it.
#[must_use]
pub fn accidental_dutch() -> Canvas {
    let mut canvas = Canvas::quiet();
    canvas.draw_horizon(9.0, 55, 195);
    canvas.add_face(
        Box2 {
            x: 0.42,
            y: 0.40,
            w: 0.16,
            h: 0.18,
        },
        12,
    );
    canvas.truth.scene = SceneId::FamilyPortrait;
    canvas
}

/// A figure whose wrists sit within a joint band of the bottom edge, with the body
/// crossing it.
///
/// The gate: "limb/joint crop detection F1 >= 0.90".
#[must_use]
pub fn cut_at_wrist() -> Canvas {
    figure_cut(0.46, aura_core::contract::composition::Joint::Wrist, true)
}

/// A figure cut between joints.
#[must_use]
pub fn cut_mid_limb() -> Canvas {
    figure_cut(0.62, aura_core::contract::composition::Joint::Knee, false)
}

/// A figure whose wrists are near the bottom edge with the body entirely inside it.
///
/// The false-positive half of the crop-audit gate, and the one that matters: a wrist an
/// inch from the bottom of the frame with the whole arm inside it is a tight crop, not a
/// cut. A build that reported this would fire on every well-framed half-length portrait in
/// a wedding.
#[must_use]
pub fn tight_but_uncut() -> Canvas {
    let mut canvas = Canvas::quiet();
    // A small, high face gives a person box that ends inside the frame.
    let face = Box2 {
        x: 0.44,
        y: 0.06,
        w: 0.07,
        h: 0.07,
    };
    canvas.add_face(face, 21);
    canvas.paint_figure(&standing_joints(0.475, 0.06, 0.55));
    canvas.truth.scene = SceneId::FamilyPortrait;
    canvas
}

/// A figure whose crown is above the top of the frame.
///
/// The gate: "top-of-head crop correctly context-dependent". Judged in
/// `family_portrait` it is a violation; judged in `couple_portrait` it is
/// `CompositionCode::DeliberateCrop`, because section 6.1 says top-of-head crops "can be
/// deliberate" there.
#[must_use]
pub fn head_cropped() -> Canvas {
    let mut canvas = Canvas::quiet();
    let face = Box2 {
        x: 0.38,
        y: 0.010,
        w: 0.24,
        h: 0.20,
    };
    canvas.add_face(face, 31);
    // The nose and ears close enough to the top that `Pose::crown` lands outside it.
    canvas.mark(0.50, 0.030);
    canvas.mark(0.455, 0.055);
    canvas.mark(0.545, 0.055);
    canvas.truth.head_cropped = true;
    canvas.truth.scene = SceneId::FamilyPortrait;
    canvas
}

/// A pole growing out of somebody's head.
///
/// The gate: "head-merge detection finds >= 85 % of labelled cases with < 10 % false
/// positives". [`clean_behind_head`] is the negative half.
#[must_use]
pub fn head_merge() -> Canvas {
    let mut canvas = Canvas::quiet();
    let face = Box2 {
        x: 0.44,
        y: 0.34,
        w: 0.13,
        h: 0.15,
    };
    canvas.add_face(face, 41);
    // A dark vertical bar running from the top of the frame down to the crown.
    canvas.fill(
        Box2 {
            x: 0.492,
            y: 0.0,
            w: 0.016,
            h: 0.35,
        },
        [28, 28, 28],
    );
    canvas.truth.head_merge = true;
    canvas.truth.scene = SceneId::CouplePortrait;
    canvas
}

/// The same head with a quiet background above it.
#[must_use]
pub fn clean_behind_head() -> Canvas {
    let mut canvas = Canvas::quiet();
    canvas.add_face(
        Box2 {
            x: 0.44,
            y: 0.34,
            w: 0.13,
            h: 0.15,
        },
        42,
    );
    canvas.truth.scene = SceneId::CouplePortrait;
    canvas
}

/// A window brighter than the subject, off to one side.
#[must_use]
pub fn bright_blob() -> Canvas {
    let mut canvas = Canvas::quiet();
    canvas.add_face(
        Box2 {
            x: 0.30,
            y: 0.40,
            w: 0.14,
            h: 0.16,
        },
        51,
    );
    canvas.fill(
        Box2 {
            x: 0.72,
            y: 0.18,
            w: 0.18,
            h: 0.26,
        },
        [246, 246, 244],
    );
    canvas.truth.bright_blob = true;
    canvas.truth.scene = SceneId::CouplePortrait;
    canvas
}

/// A saturated red sign behind a neutral subject.
///
/// Section 6.2's worked example, in reverse: "a saturated red exit sign behind a white
/// dress is flagged".
#[must_use]
pub fn colour_clash() -> Canvas {
    let mut canvas = Canvas::quiet();
    canvas.add_face(
        Box2 {
            x: 0.40,
            y: 0.38,
            w: 0.18,
            h: 0.20,
        },
        61,
    );
    canvas.fill(
        Box2 {
            x: 0.72,
            y: 0.20,
            w: 0.20,
            h: 0.22,
        },
        [214, 22, 22],
    );
    canvas.truth.scene = SceneId::CouplePortrait;
    canvas
}

/// A centred, textured arrangement with no people in it.
///
/// The `details` flat-lay of section 2.1, and what `centre_preferred` exists for: a rule of
/// thirds applied here flags a correctly framed photograph.
#[must_use]
pub fn flat_lay() -> Canvas {
    let mut canvas = Canvas::quiet();
    canvas.fill_textured(Box2 {
        x: 0.36,
        y: 0.36,
        w: 0.28,
        h: 0.28,
    });
    canvas.truth.scene = SceneId::Details;
    canvas
}

/// A pair for the cap gate: a beautifully arranged frame that cuts a neck, and a plain one
/// that does not.
///
/// Section 6.3's requirement, as a fixture: taste must not lift the first above the second
/// at any value of `aesthetic_weight`. The first element is the beautiful, cut frame.
#[must_use]
pub fn beautiful_but_cut() -> (Canvas, Canvas) {
    let mut cut = Canvas::quiet();
    let face = Box2 {
        x: 0.30,
        y: 0.10,
        w: 0.18,
        h: 0.20,
    };
    cut.add_face(face, 71);
    // Shoulders at the very bottom of the frame, with the body crossing it.
    cut.paint_figure(&standing_joints(0.39, 0.10, 1.34));
    cut.truth.cut_joint = Some(aura_core::contract::composition::Joint::Neck);
    cut.truth.cut_at_joint = true;
    cut.truth.scene = SceneId::CouplePortrait;

    let mut plain = Canvas::quiet();
    plain.add_face(
        Box2 {
            x: 0.44,
            y: 0.30,
            w: 0.12,
            h: 0.13,
        },
        72,
    );
    plain.paint_figure(&standing_joints(0.50, 0.30, 0.62));
    plain.truth.scene = SceneId::CouplePortrait;

    (cut, plain)
}

/// Pairs of frames with a known better half, for the agreement gate.
///
/// Eight pairs, each isolating one measurement, and the *better* frame is always the second
/// element. Eight authored comparisons is not four thousand photographer comparisons -
/// that is condition C2 of the phase 11 exit report and the harness says so where the
/// number is reported.
#[must_use]
pub fn composition_pairs() -> Vec<(Canvas, Canvas)> {
    vec![
        (tilted(7.0), tilted(0.2)),
        (accidental_dutch(), tilted(0.4)),
        (head_merge(), clean_behind_head()),
        (bright_blob(), clean_behind_head()),
        (colour_clash(), clean_behind_head()),
        (cut_at_wrist(), tight_but_uncut()),
        (cut_mid_limb(), tight_but_uncut()),
        (head_cropped(), tight_but_uncut()),
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A standing figure whose feet run past the bottom edge, with a named joint at
/// `wrist_fraction` of the way down and the frame cutting near it.
fn figure_cut(
    joint_fraction: f32,
    joint: aura_core::contract::composition::Joint,
    at_joint: bool,
) -> Canvas {
    let mut canvas = Canvas::quiet();
    let face = Box2 {
        x: 0.42,
        y: 0.10,
        w: 0.14,
        h: 0.16,
    };
    canvas.add_face(face, 81);
    // The figure runs from the face down past the bottom of the frame, so the derived
    // person box crosses the bottom edge - which is what makes a cut a cut rather than a
    // joint near an edge.
    let head_y = 0.10;
    let feet_y = head_y + (0.97 - head_y) / joint_fraction;
    canvas.paint_figure(&standing_joints(0.49, head_y, feet_y));
    canvas.truth.cut_joint = Some(joint);
    canvas.truth.cut_at_joint = at_joint;
    canvas.truth.scene = SceneId::FamilyPortrait;
    canvas
}

/// One normalised rectangle in pixels, clamped.
fn pixels(area: Box2, width: usize, height: usize) -> (usize, usize, usize, usize) {
    let area = area.clamped();
    let x0 = (area.x * width as f32).round().max(0.0) as usize;
    let y0 = (area.y * height as f32).round().max(0.0) as usize;
    let x1 = (((area.x + area.w) * width as f32).round().max(0.0) as usize).min(width);
    let y1 = (((area.y + area.h) * height as f32).round().max(0.0) as usize).min(height);
    (x0.min(x1), y0.min(y1), x1, y1)
}

/// Write one pixel.
fn write(rgb: &mut [u8], x: usize, y: usize, colour: [u8; 3]) {
    let base = (y * SIDE + x) * 3;
    for (offset, value) in colour.into_iter().enumerate() {
        if let Some(slot) = rgb.get_mut(base + offset) {
            *slot = value;
        }
    }
}
