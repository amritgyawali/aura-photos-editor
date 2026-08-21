//! Where a specular sheet has destroyed the record over somebody's eyes.
//!
//! PHASE-21 section 6.3:
//!
//! > Glasses glare: detect specular sheets overlapping the eye region; if a sibling frame from
//! > the same moment has the same face without glare and closely matching geometry, borrow that
//! > region with alignment and frequency blending; otherwise reduce highlight intensity
//! > conservatively.
//!
//! ## A sheet is not a highlight
//!
//! A catchlight is a small bright point and this phase protects it. A glare sheet is a *large
//! connected area of near-clipped pixels lying across the eye region*, and the difference is
//! area and shape rather than brightness. [`detect`] looks only inside the eye region grown by
//! [`EYE_MARGIN`] - which is where a lens sits - and requires a component covering at least
//! [`MIN_SHEET_FRACTION`] of that region before it calls anything a sheet.
//!
//! ## The number that decides whether a sheet may be borrowed over
//!
//! [`Sheet::clipped_fraction`] is how much of the sheet is past
//! `aura_render::micro::CLIPPED_FLOOR` - past which the sensor recorded nothing. ADR-0043
//! section 4 turns that into the rule this phase is bounded by:
//!
//! > You may only borrow pixels that carry no information.
//!
//! A sheet at or above [`aura_core::contract::micro::MIN_SPECULAR_FRACTION`] clipped is a hole in
//! the photograph and may be reconstructed from a sibling. A softer sheen below it still carries
//! an eye, so it is *reduced* using this frame's own pixels and nothing is composited. The
//! decision is [`Sheet::may_borrow`] and it is made here rather than in [`super::borrow`],
//! because it is a statement about what the photograph contains rather than about what another
//! photograph could supply.
//!
//! ## Everything here is linear
//!
//! Invariant 8.

use aura_core::contract::composition::Box2;
use aura_core::contract::micro::{MAX_BORROW_AREA, MAX_GLARE_REDUCE, MIN_SPECULAR_FRACTION};
use aura_core::contract::people::FaceRef;
use aura_render::micro::{CLIPPED_FLOOR, SPECULAR_FLOOR};

use crate::texture_guard::Frame;

/// How far outside the eye region a lens reaches, as a multiple of the inter-ocular distance.
///
/// A third. Spectacle lenses are wider and taller than the eyes behind them, and a search
/// confined to the eye alpha finds the middle of a sheet and not its edges - which produces a
/// reduction with a visible boundary halfway across the lens.
pub const EYE_MARGIN: f32 = 0.33;

/// The smallest share of the searched region a component must cover to be a sheet.
///
/// Below this it is a catchlight, a highlight on a frame's metal, or a reflection in an iris -
/// all things this phase protects rather than reduces.
///
/// One and a half per cent, and the number is low for a reason that is worth stating because it
/// is not obvious. It has to coexist with
/// [`aura_core::contract::micro::MAX_BORROW_AREA`], which bounds a borrow as a fraction of the
/// **frame**, while this bounds a sheet as a fraction of the **lens box**. On a portrait the lens
/// box is a large share of the frame, so a floor of six per cent of it would already exceed the
/// borrow cap - and cross-frame repair would be structurally unreachable on exactly the
/// photographs it exists for. A catchlight is a few tenths of a per cent of a lens box, so one
/// and a half per cent still separates the two by an order of magnitude.
pub const MIN_SHEET_FRACTION: f32 = 0.015;

/// The fewest samples a component needs before it is a sheet at all.
///
/// An absolute floor beside the relative one, because on a small face the lens box is a hundred
/// pixels and one and a half per cent of it is a single sample. A component that small is noise
/// whatever share of its region it covers.
pub const MIN_SHEET_PIXELS: usize = 24;

/// The largest share of the searched region a sheet may cover and still be worked on.
///
/// Above this the entire lens is white, there is no local reference for a reduction, and there is
/// nothing in this frame to blend a borrow into. A photograph like that needs a different frame
/// rather than a repair.
pub const MAX_SHEET_FRACTION: f32 = 0.72;

/// One specular sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// Where it is, normalised to the frame.
    pub region: Box2,
    /// Which face it sits on, by index into the frame's face list.
    pub face: usize,
    /// What share of the sheet is past [`CLIPPED_FLOOR`], `0..1`.
    pub clipped_fraction: f32,
    /// What share of the searched region the sheet covers, `0..1`.
    pub coverage: f32,
    /// How much brighter the sheet is than the eye region around it.
    pub excess: f32,
}

impl Sheet {
    /// True when this sheet has destroyed the record and may be reconstructed from a sibling.
    ///
    /// **The rule the whole borrowing feature is bounded by.** Two conditions, and both are about
    /// this photograph rather than about the sibling: the region carries no information, and it is
    /// small enough that reconstructing it is a repair rather than a composite portrait.
    #[must_use]
    pub fn may_borrow(&self) -> bool {
        self.clipped_fraction >= MIN_SPECULAR_FRACTION
            && self.region.w * self.region.h <= MAX_BORROW_AREA + 1e-9
    }
}

/// The conservative reduction strength for a sheet that must be repaired from this frame alone.
///
/// Proportional to how far above the surrounding eye region the sheet sits, so a faint sheen is
/// barely touched and a strong one is pulled down as far as the ceiling allows. Never above
/// [`MAX_GLARE_REDUCE`], because a sheet reduced to nothing is a lens that has stopped reflecting
/// the room - which reads as a hole rather than as a repair.
#[must_use]
pub fn reduce_strength(sheet: &Sheet, strength: f32) -> f32 {
    let wanted = (sheet.excess * 2.0).clamp(0.0, 1.0) * strength.clamp(0.0, 1.0);
    wanted.clamp(0.0, MAX_GLARE_REDUCE)
}

/// Find the specular sheets over one frame's faces.
///
/// `eyes` is the per-pixel eye coverage from phase 18. Faces with no landmarks are skipped -
/// phase 09's rule that an unknown landmark must never be read as the origin - and a frame with
/// no eye region produces nothing, because there is no geometric fallback in this phase.
///
/// Ordered by coverage, largest first, then by position. Invariant 4.
#[must_use]
pub fn detect(frame: &Frame, eyes: &[f32], faces: &[FaceRef]) -> Vec<Sheet> {
    let (width, height) = (frame.width, frame.height);
    if width == 0 || height == 0 || eyes.len() < width * height {
        return Vec::new();
    }

    let luminance = luma_plane(frame);
    let mut out: Vec<Sheet> = Vec::new();

    for (face_index, face) in faces.iter().enumerate() {
        if !face.has_eyes() {
            continue;
        }
        let search = lens_box(face);
        let x0 = (search.x * width as f32).floor().max(0.0) as usize;
        let y0 = (search.y * height as f32).floor().max(0.0) as usize;
        let x1 = (((search.x + search.w) * width as f32).ceil() as usize).min(width);
        let y1 = (((search.y + search.h) * height as f32).ceil() as usize).min(height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }

        // The reference: the non-specular eye-region luminance this face's sheet is measured
        // against. This frame's own pixels, never a constant.
        let mut reference = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let index = y * width + x;
                if eyes.get(index).copied().unwrap_or(0.0) < 0.5 {
                    continue;
                }
                let value = luminance.get(index).copied().unwrap_or(0.0);
                if value >= SPECULAR_FLOOR {
                    continue;
                }
                reference.push(value);
            }
        }
        if reference.is_empty() {
            continue;
        }
        reference.sort_by(f32::total_cmp);
        let base = reference
            .get(reference.len() / 2)
            .copied()
            .unwrap_or_default();

        let searched = (x1 - x0) * (y1 - y0);
        let mut seen = vec![false; searched];
        for y in y0..y1 {
            for x in x0..x1 {
                let local = (y - y0) * (x1 - x0) + (x - x0);
                if seen.get(local).copied().unwrap_or(true) {
                    continue;
                }
                if luminance.get(y * width + x).copied().unwrap_or(0.0) < SPECULAR_FLOOR {
                    if let Some(slot) = seen.get_mut(local) {
                        *slot = true;
                    }
                    continue;
                }
                let component = grow(
                    local,
                    &luminance,
                    &mut seen,
                    (x0, y0, x1 - x0, y1 - y0),
                    width,
                );
                let coverage = component.count as f32 / searched as f32;
                if component.count < MIN_SHEET_PIXELS
                    || !(MIN_SHEET_FRACTION..=MAX_SHEET_FRACTION).contains(&coverage)
                {
                    continue;
                }

                let region = Box2 {
                    x: (x0 + component.x0) as f32 / width as f32,
                    y: (y0 + component.y0) as f32 / height as f32,
                    w: (component.x1 - component.x0 + 1) as f32 / width as f32,
                    h: (component.y1 - component.y0 + 1) as f32 / height as f32,
                };
                out.push(Sheet {
                    region,
                    face: face_index,
                    clipped_fraction: if component.count == 0 {
                        0.0
                    } else {
                        component.clipped as f32 / component.count as f32
                    },
                    coverage,
                    excess: (component.mean - base).max(0.0),
                });
            }
        }
    }

    out.sort_by(|a, b| {
        b.coverage
            .total_cmp(&a.coverage)
            .then(a.region.y.total_cmp(&b.region.y))
            .then(a.region.x.total_cmp(&b.region.x))
    });
    out
}

/// The rectangle a spectacle lens could occupy on one face.
///
/// Built from the two eye landmarks rather than from the face box, because a lens is positioned
/// by the eyes and a face box includes a chin.
#[must_use]
pub fn lens_box(face: &FaceRef) -> Box2 {
    let left = face.eyes[0];
    let right = face.eyes[1];
    let separation = ((right[0] - left[0]).powi(2) + (right[1] - left[1]).powi(2))
        .sqrt()
        .max(1e-4);
    let margin = separation * EYE_MARGIN;
    let x0 = left[0].min(right[0]) - margin;
    let x1 = left[0].max(right[0]) + margin;
    let y0 = left[1].min(right[1]) - margin;
    let y1 = left[1].max(right[1]) + margin;
    Box2 {
        x: x0,
        y: y0,
        w: (x1 - x0).max(1e-4),
        h: (y1 - y0).max(1e-4),
    }
    .clamped()
}

#[derive(Debug, Clone, Copy)]
struct Component {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    count: usize,
    clipped: usize,
    mean: f32,
}

/// Grow one specular component inside the searched window.
///
/// Coordinates in the returned component are **window-local**; the caller adds the window origin.
fn grow(
    seed: usize,
    luminance: &[f32],
    seen: &mut [bool],
    window: (usize, usize, usize, usize),
    stride: usize,
) -> Component {
    let (ox, oy, w, h) = window;
    let mut stack = vec![seed];
    if let Some(slot) = seen.get_mut(seed) {
        *slot = true;
    }
    let (mut x0, mut y0) = (seed % w, seed / w);
    let (mut x1, mut y1) = (x0, y0);
    let mut count = 0usize;
    let mut clipped = 0usize;
    let mut total = 0.0f64;

    while let Some(local) = stack.pop() {
        let (x, y) = (local % w, local / w);
        let value = luminance
            .get((oy + y) * stride + ox + x)
            .copied()
            .unwrap_or(0.0);
        if value < SPECULAR_FLOOR {
            continue;
        }
        count += 1;
        total += f64::from(value);
        if value >= CLIPPED_FLOOR {
            clipped += 1;
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);

        // The four neighbours, in unsigned coordinates: each one is this pixel with exactly one
        // axis moved, and a step that would leave the frame is simply not offered.
        let mut neighbours = [None; 4];
        if x + 1 < w {
            neighbours[0] = Some(y * w + x + 1);
        }
        if x > 0 {
            neighbours[1] = Some(y * w + x - 1);
        }
        if y + 1 < h {
            neighbours[2] = Some((y + 1) * w + x);
        }
        if y > 0 {
            neighbours[3] = Some((y - 1) * w + x);
        }
        for neighbour in neighbours.into_iter().flatten() {
            if seen.get(neighbour).copied().unwrap_or(true) {
                continue;
            }
            if let Some(slot) = seen.get_mut(neighbour) {
                *slot = true;
            }
            stack.push(neighbour);
        }
    }

    Component {
        x0,
        y0,
        x1,
        y1,
        count,
        clipped,
        mean: if count == 0 {
            0.0
        } else {
            (total / count as f64) as f32
        },
    }
}

fn luma_plane(frame: &Frame) -> Vec<f32> {
    let mut out = Vec::with_capacity(frame.width * frame.height);
    for index in 0..frame.width * frame.height {
        let slot = index * 3;
        out.push(frame.rgb.get(slot..slot + 3).map_or(0.0, |rgb| {
            0.2126 * rgb.first().copied().unwrap_or(0.0)
                + 0.7152 * rgb.get(1).copied().unwrap_or(0.0)
                + 0.0722 * rgb.get(2).copied().unwrap_or(0.0)
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::integrity::CropRect;
    use aura_core::contract::people::FaceRef;
    use aura_core::FaceId;

    fn face() -> FaceRef {
        FaceRef {
            face_id: FaceId::new(),
            identity_id: None,
            bbox: CropRect {
                x: 0.25,
                y: 0.20,
                w: 0.50,
                h: 0.60,
            },
            eyes: [[0.40, 0.40], [0.60, 0.40]],
            area_frac: 0.30,
            centrality: 0.9,
            sharpness: 0.8,
            quality: 0.8,
            votes: true,
        }
    }

    /// A frame whose eye region is mid-grey, with a bright patch of a chosen brightness across
    /// part of the lens box.
    ///
    /// 256 px a side, because `MAX_BORROW_AREA` is a fraction of the frame: a sheet that is both
    /// large enough to be a sheet and small enough to borrow over cannot be expressed on a 100 px
    /// fixture, and a fixture that cannot express the real constraint tests itself.
    fn with_sheet(level: f32) -> (Frame, Vec<f32>) {
        let (width, height) = (256usize, 256usize);
        let mut rgb = vec![0.30f32; width * height * 3];
        let mut eyes = vec![0.0f32; width * height];
        for y in 88..112 {
            for x in 88..168 {
                if let Some(slot) = eyes.get_mut(y * width + x) {
                    *slot = 1.0;
                }
            }
        }
        // Ten by eight: 80 samples. Above `MIN_SHEET_PIXELS` and above
        // `MIN_SHEET_FRACTION` of the lens box, and 80/65536 of the frame - inside
        // `MAX_BORROW_AREA`.
        for y in 94..102 {
            for x in 96..106 {
                let index = y * width + x;
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut(index * 3 + channel) {
                        *slot = level;
                    }
                }
            }
        }
        (Frame { rgb, width, height }, eyes)
    }

    #[test]
    fn a_blown_sheet_is_found_and_may_be_borrowed_over() {
        let (frame, eyes) = with_sheet(1.20);
        let sheets = detect(&frame, &eyes, &[face()]);
        assert!(!sheets.is_empty(), "no sheet was found");
        let sheet = sheets.first().expect("a sheet");
        assert!(
            sheet.clipped_fraction >= MIN_SPECULAR_FRACTION,
            "the sheet was not read as clipped: {sheet:?}"
        );
        assert!(sheet.may_borrow(), "a blown sheet was refused: {sheet:?}");
    }

    #[test]
    fn a_soft_sheen_is_found_and_may_never_be_borrowed_over() {
        // Above the specular floor so it is detected, but below the clipped floor so it still
        // carries an eye. This is the case ADR-0043 section 4 exists to separate.
        let (frame, eyes) = with_sheet(0.93);
        let sheets = detect(&frame, &eyes, &[face()]);
        assert!(!sheets.is_empty(), "no sheet was found");
        for sheet in &sheets {
            assert!(
                !sheet.may_borrow(),
                "a sheet that still carries information was allowed to be borrowed over: {sheet:?}"
            );
            assert!(
                reduce_strength(sheet, 1.0) > 0.0,
                "and it was not reduced either"
            );
        }
    }

    #[test]
    fn a_catchlight_is_too_small_to_be_a_sheet() {
        let (mut frame, eyes) = with_sheet(0.30);
        for y in 98..100 {
            for x in 100..102 {
                let index = y * frame.width + x;
                for channel in 0..3 {
                    if let Some(slot) = frame.rgb.get_mut(index * 3 + channel) {
                        *slot = 1.5;
                    }
                }
            }
        }
        assert!(detect(&frame, &eyes, &[face()]).is_empty());
    }

    #[test]
    fn a_face_with_no_landmarks_is_skipped_rather_than_measured_at_the_origin() {
        let (frame, eyes) = with_sheet(1.20);
        let mut blind = face();
        blind.eyes = [[0.0, 0.0], [0.0, 0.0]];
        assert!(detect(&frame, &eyes, &[blind]).is_empty());
    }

    #[test]
    fn no_eye_region_is_no_sheets() {
        let (frame, _) = with_sheet(1.20);
        assert!(detect(&frame, &[], &[face()]).is_empty());
    }

    #[test]
    fn the_reduction_strength_can_never_exceed_the_ceiling() {
        let (frame, eyes) = with_sheet(1.60);
        for sheet in detect(&frame, &eyes, &[face()]) {
            assert!(reduce_strength(&sheet, 1.0) <= MAX_GLARE_REDUCE + 1e-6);
        }
    }
}
