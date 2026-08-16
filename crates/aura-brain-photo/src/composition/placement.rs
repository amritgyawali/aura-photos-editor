//! Where the subject sits in the frame, and how much of the frame is left over.
//!
//! PHASE-11 section 2.1: "headroom ratio, subject placement vs thirds/centre,
//! negative-space balance". Section 6.1 adds the conditioning: "headroom is compared to
//! scene-specific bands from `composition_rules.toml` (a `family_portrait` wants more
//! headroom than a `dance_floor` frame)."
//!
//! ## Four measurements, and only two of them are scored everywhere
//!
//! | Measure | What it is | Scored when |
//! |---|---|---|
//! | [`Placement::headroom`] | crown to frame top, as a fraction of height | there is a head |
//! | [`Placement::thirds_offset`] | subject to nearest power point, over the diagonal | `thirds_weight > 0` |
//! | [`Placement::balance`] | how evenly visual weight is spread | `balance_weight > 0` |
//! | [`Placement::negative_space`] | how much of the frame is quiet | never |
//!
//! The fourth is measured and reported and never scored, and that is deliberate rather
//! than an omission. A `details` flat-lay wants a lot of empty frame and a `dance_floor`
//! frame has none, and there is no direction in which "more" or "less" is better -
//! so it is carried for the aesthetic head, which can learn what it means per scene, and
//! for phase 23, which needs to know what it is allowed to crop away.
//!
//! ## The thirds are a *preference*, not a rule
//!
//! [`SceneRule::centre_preferred`] exists because a rule of thirds applied to a flat-lay,
//! a ring shot or a symmetrical altar frame flags every correct photograph in the scene.
//! When it is set, being near the centre raises `CENTRED` - which is not a defect - and
//! being far from *both* the centre and the power points is what raises `OFF_THIRDS`.
//!
//! ## Balance is measured from contrast energy, not from saliency
//!
//! Section 6.2 permits "a light saliency model" for the background analysis, and
//! [`super::background`] uses one. Balance deliberately does not. The thing that makes a
//! frame feel lopsided is where the *contrast* is, and contrast energy is a measurement
//! with no model in it - which means the number is the same on every machine, at every
//! model version, forever. Invariant 4 costs nothing here and buys a measure that never
//! needs re-validating.

use aura_core::contract::composition::{Box2, CropHint, HORIZON_ACT_AT};
use aura_vision::embed::descriptors::LumaPlane;

use crate::composition::keypoints::Pose;
use crate::composition::rules::SceneRule;

/// The four power points of the rule of thirds, in normalised coordinates.
pub const POWER_POINTS: [(f32, f32); 4] = [
    (1.0 / 3.0, 1.0 / 3.0),
    (2.0 / 3.0, 1.0 / 3.0),
    (1.0 / 3.0, 2.0 / 3.0),
    (2.0 / 3.0, 2.0 / 3.0),
];

/// The frame diagonal in normalised units. The denominator of every placement distance.
pub const DIAGONAL: f32 = std::f32::consts::SQRT_2;

/// How near a power point counts as on it, as a fraction of the diagonal.
///
/// Six per cent, which is about eighty-five pixels on a 2048 px proxy - roughly the width
/// of a face in a half-length portrait. Tighter than that and a photographer who placed a
/// subject by eye is told they missed.
pub const ON_POWER_POINT: f32 = 0.06;

/// How near the centre counts as centred, as a fraction of the diagonal.
pub const ON_CENTRE: f32 = 0.06;

/// Beyond this distance from both the centre and every power point, placement is called
/// off.
///
/// Eighteen per cent of the diagonal. Between `ON_POWER_POINT` and this the subject is
/// simply somewhere, which is where most subjects in most photographs are - and reporting
/// that as a violation is section 12's first failure mode in one line of code.
pub const OFF_PLACEMENT: f32 = 0.18;

/// How many bands the balance measure splits the frame into, per axis.
pub const BALANCE_BANDS: usize = 8;

/// Local energy below which a cell counts as empty frame.
pub const QUIET_ENERGY: f32 = 0.04;

/// How the headroom compares to the scene's band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeadroomVerdict {
    /// There was no head to measure. The default, because it claims the least.
    #[default]
    NotMeasured,
    /// Inside the band.
    InBand,
    /// Below the band's minimum.
    Tight,
    /// Above the band's maximum.
    Excessive,
}

/// Where everything is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Space above the highest crown, as a fraction of frame height. Negative when a head
    /// is cut.
    pub headroom: f32,
    /// How that compares to this scene's band.
    pub headroom_verdict: HeadroomVerdict,
    /// Distance from the subject to the nearest power point, over the diagonal.
    pub thirds_offset: f32,
    /// Distance from the subject to the frame centre, over the diagonal.
    pub centre_offset: f32,
    /// True when the subject is on a power point.
    pub on_power_point: bool,
    /// True when the subject is at the centre and this scene wants it there.
    pub centred: bool,
    /// True when the subject is near neither.
    pub off_placement: bool,
    /// How evenly visual weight is spread, `0..1`.
    pub balance: f32,
    /// How much of the frame is quiet, `0..1`.
    pub negative_space: f32,
    /// Where the subject is, normalised, when there is one.
    pub subject_centre: Option<(f32, f32)>,
    /// The box everything of interest sits inside, when there is one.
    pub subject_box: Option<Box2>,
}

impl Placement {
    /// A frame with no subject in it.
    ///
    /// The two whole-frame measures still have values, because a photograph of a room has
    /// a balance and an amount of empty space whether or not anybody is in it.
    #[must_use]
    pub fn without_subject(balance: f32, negative_space: f32) -> Self {
        Self {
            headroom: 0.0,
            headroom_verdict: HeadroomVerdict::NotMeasured,
            thirds_offset: 0.0,
            centre_offset: 0.0,
            on_power_point: false,
            centred: false,
            off_placement: false,
            balance,
            negative_space,
            subject_centre: None,
            subject_box: None,
        }
    }
}

/// Measure placement, balance and empty space.
///
/// `head_crop_depth` comes from [`super::crop_audit`] and is how a cut head becomes a
/// negative headroom rather than a zero one: zero would mean "the crown is exactly at the
/// frame edge", which is a different photograph.
#[must_use]
pub fn analyse(
    plane: &LumaPlane,
    poses: &[Pose],
    faces: &[Box2],
    rule: &SceneRule,
    head_crop_depth: f32,
) -> Placement {
    let balance = balance(plane);
    let negative_space = negative_space(plane);

    let Some(subject) = subject_box(poses, faces) else {
        return Placement::without_subject(balance, negative_space);
    };
    let centre = (subject.x + subject.w / 2.0, subject.y + subject.h / 2.0);

    let headroom = if head_crop_depth > 0.0 {
        -head_crop_depth
    } else {
        crown_y(poses, faces).unwrap_or(subject.y).max(0.0)
    };
    let headroom_verdict = if head_crop_depth > 0.0 {
        // A cut head is reported by the crop audit, not twice. `Tight` here would put two
        // sentences about one edge in a six-slot panel.
        HeadroomVerdict::Tight
    } else if rule.headroom_ok(headroom) {
        HeadroomVerdict::InBand
    } else if headroom < rule.headroom_min {
        HeadroomVerdict::Tight
    } else {
        HeadroomVerdict::Excessive
    };

    let thirds_offset = POWER_POINTS
        .into_iter()
        .map(|(x, y)| (centre.0 - x).hypot(centre.1 - y) / DIAGONAL)
        .fold(f32::MAX, f32::min);
    let centre_offset = (centre.0 - 0.5).hypot(centre.1 - 0.5) / DIAGONAL;

    let on_power_point = thirds_offset <= ON_POWER_POINT;
    let centred = rule.centre_preferred && centre_offset <= ON_CENTRE;
    // Off placement needs to be far from *both* anchors, and it is not claimed at all in a
    // scene that prefers the centre - there, being off the centre is the whole finding and
    // `CENTRED` simply does not fire.
    let off_placement = !on_power_point
        && !centred
        && thirds_offset > OFF_PLACEMENT
        && centre_offset > OFF_PLACEMENT;

    Placement {
        headroom,
        headroom_verdict,
        thirds_offset,
        centre_offset,
        on_power_point,
        centred,
        off_placement,
        balance,
        negative_space,
        subject_centre: Some(centre),
        subject_box: Some(subject),
    }
}

/// The box every subject sits inside.
///
/// The union of the located person boxes when there are any, and of the face boxes
/// otherwise. A pose with zero located points is a failed keypoint reading, not evidence
/// that its extrapolated full-body box is the subject; using one would drag a face-only
/// portrait's centre towards the lower edge. The union rather than the largest, because a
/// family portrait's subject is the group and a crop hint built around one member of it
/// would cut the rest out.
#[must_use]
pub fn subject_box(poses: &[Pose], faces: &[Box2]) -> Option<Box2> {
    let located: Vec<Box2> = poses
        .iter()
        .filter(|pose| pose.located() > 0)
        .map(|pose| pose.person)
        .collect();
    let boxes: Vec<Box2> = if located.is_empty() {
        faces.to_vec()
    } else {
        located
    };
    let mut iter = boxes.into_iter();
    let first = iter.next()?;
    let (mut x0, mut y0) = (first.x, first.y);
    let (mut x1, mut y1) = (first.x + first.w, first.y + first.h);
    for area in iter {
        x0 = x0.min(area.x);
        y0 = y0.min(area.y);
        x1 = x1.max(area.x + area.w);
        y1 = y1.max(area.y + area.h);
    }
    Some(
        Box2 {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
        .clamped(),
    )
}

/// The highest crown in the frame.
///
/// From [`Pose::crown`] when a nose was located, and from the top of the highest face box
/// otherwise. The fallback is conservative in the direction that matters: a face box top
/// is *below* the real crown, so the headroom it produces is larger than the truth and a
/// tight crop is under-reported rather than invented.
#[must_use]
pub fn crown_y(poses: &[Pose], faces: &[Box2]) -> Option<f32> {
    let from_poses = poses
        .iter()
        .filter_map(Pose::crown)
        .fold(None::<f32>, |best, y| Some(best.map_or(y, |b| b.min(y))));
    if from_poses.is_some() {
        return from_poses;
    }
    faces
        .iter()
        .map(|face| face.y)
        .fold(None::<f32>, |best, y| Some(best.map_or(y, |b| b.min(y))))
}

/// How evenly the frame's contrast energy is spread, `0..1`.
///
/// Horizontal lopsidedness is weighted at 0.7 and vertical at 0.3, because a frame with
/// everything on the left reads as a mistake and a frame with everything at the bottom
/// reads as a landscape. Both are measured; only one of them is usually a complaint.
#[must_use]
pub fn balance(plane: &LumaPlane) -> f32 {
    let energy = energy_grid(plane);
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    let mut top = 0.0f32;
    let mut bottom = 0.0f32;
    for (index, cell) in energy.iter().enumerate() {
        let column = index % BALANCE_BANDS;
        let row = index / BALANCE_BANDS;
        if column < BALANCE_BANDS / 2 {
            left += cell;
        } else {
            right += cell;
        }
        if row < BALANCE_BANDS / 2 {
            top += cell;
        } else {
            bottom += cell;
        }
    }
    let horizontal = asymmetry(left, right);
    let vertical = asymmetry(top, bottom);
    (1.0 - (horizontal * 0.7 + vertical * 0.3)).clamp(0.0, 1.0)
}

/// How much of the frame carries almost no contrast, `0..1`.
#[must_use]
pub fn negative_space(plane: &LumaPlane) -> f32 {
    let energy = energy_grid(plane);
    if energy.is_empty() {
        return 0.0;
    }
    let quiet = energy.iter().filter(|cell| **cell < QUIET_ENERGY).count();
    quiet as f32 / energy.len() as f32
}

/// What phase 23 is handed.
///
/// Three things, and the third is the one that is easiest to get wrong:
///
/// * **the region**: the subject box grown by a margin, so the crop keeps the people;
/// * **the safe margin**: how far in a crop may go before it starts cutting something,
///   which is **zero** when the frame already cuts a limb - cropping further would turn a
///   mid-limb cut into a joint cut, and a hint that invited that would be worse than no
///   hint;
/// * **the straighten**: `None` below [`HORIZON_ACT_AT`], because section 6.1 says never
///   straighten below 0.6 confidence and an absent value says that where a zero would
///   claim the frame is level.
#[must_use]
pub fn crop_hint(
    placement: &Placement,
    tilt_deg: f32,
    horizon_conf: f32,
    confidence: f32,
    already_cut: bool,
) -> Option<CropHint> {
    let region = placement.subject_box.map(|area| {
        // A tenth of the subject's own size on each side. Enough that a crop optimised
        // toward this hint keeps hands and hair; not so much that it is the whole frame.
        area.expanded(0.20)
    })?;

    let straighten = if horizon_conf >= HORIZON_ACT_AT && tilt_deg.abs() > 0.1 {
        Some(-tilt_deg)
    } else {
        None
    };

    // How much frame there is between the region and the edges, on the tightest side.
    let margin = [
        region.x,
        region.y,
        1.0 - (region.x + region.w),
        1.0 - (region.y + region.h),
    ]
    .into_iter()
    .fold(f32::MAX, f32::min)
    .max(0.0);
    let safe_margin = if already_cut { 0.0 } else { margin };

    // A portrait-class caller needs the preservation objective even when there is no safe
    // room to tighten it today. `CropHint::is_actionable` remains the separate answer the
    // UI and phase 23 use for offering an action; dropping the whole value here would lose
    // the subject region and fail section 13's "all portrait-class frames" handoff.
    Some(CropHint {
        region,
        safe_margin,
        straighten_deg: straighten,
        confidence,
    })
}

/// Mean absolute gradient in each cell of an 8x8 grid.
fn energy_grid(plane: &LumaPlane) -> Vec<f32> {
    if plane.width < BALANCE_BANDS * 2 || plane.height < BALANCE_BANDS * 2 {
        return Vec::new();
    }
    let mut sums = vec![0.0f32; BALANCE_BANDS * BALANCE_BANDS];
    let mut counts = vec![0u32; BALANCE_BANDS * BALANCE_BANDS];
    let at = |x: usize, y: usize| -> f32 {
        plane
            .values
            .get(y * plane.width + x)
            .copied()
            .unwrap_or(0.0)
    };
    for y in 0..plane.height - 1 {
        let row = (y * BALANCE_BANDS) / plane.height;
        for x in 0..plane.width - 1 {
            let column = (x * BALANCE_BANDS) / plane.width;
            let here = at(x, y);
            let gradient = (at(x + 1, y) - here).abs() + (at(x, y + 1) - here).abs();
            let index = row * BALANCE_BANDS + column;
            if let (Some(sum), Some(count)) = (sums.get_mut(index), counts.get_mut(index)) {
                *sum += gradient;
                *count += 1;
            }
        }
    }
    sums.iter()
        .zip(&counts)
        .map(|(sum, count)| {
            if *count == 0 {
                0.0
            } else {
                sum / *count as f32
            }
        })
        .collect()
}

/// `|a - b| / (a + b)`, guarded, in `0..1`.
fn asymmetry(a: f32, b: f32) -> f32 {
    let total = a + b;
    if total <= f32::EPSILON {
        return 0.0;
    }
    ((a - b).abs() / total).clamp(0.0, 1.0)
}
