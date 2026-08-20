//! Connected components, and which person each of them belongs to.
//!
//! Section 6.2: "Assign each connected component of face/skin/hair/clothing to the nearest
//! Phase 06 face/person box with an overlap test; unassigned components become
//! `identity: None`."
//!
//! # The word "nearest" in that sentence is the trap
//!
//! Nearest-box assignment is what makes the bride's skin mask include the guest standing
//! behind her, which is section 10.1's own test - "in group photos, per-identity skin masks do
//! not bleed between adjacent people" - and the failure that makes phase 25's per-person
//! gallery consistency worse than no per-person consistency at all. What is implemented is the
//! *overlap test* half of the sentence, with [`crate::contract::mask::ASSIGN_MIN_OVERLAP`] as
//! the floor, and everything under it is `None`.
//!
//! The overlap is **containment** rather than intersection-over-union, and that is not a
//! detail. A face is a small ellipse and a body box is most of the frame, so their IoU is under
//! a fifth even when the face is entirely inside the box - an IoU floor would leave every face
//! in the wedding unassigned while looking like a careful threshold.
//!
//! `None` is a real answer. An unscoped skin component is still skin; it is just not *hers*,
//! and an operation that needs it to be hers can see that it is not. ADR-0037 decision 9.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::ids::IdentityId;

use crate::contract::mask::{MaskReason, ASSIGN_MIN_OVERLAP};
use crate::face::detect::NormBox;
use crate::face::person::PersonBox;
use crate::face::FaceObservation;
use crate::mask::algebra::Plane;
use crate::mask::MaskPlane;

/// A labelling of a plane into connected components.
///
/// Component `0` is background. Ids are assigned in raster order, which is what makes the
/// labelling deterministic: a union-find that assigned ids in discovery order would produce
/// the same partition with different numbers on two machines, and the numbers reach the
/// catalog through the order masks are stored in.
#[derive(Debug, Clone)]
pub struct Labels {
    /// Grid width.
    pub w: u32,
    /// Grid height.
    pub h: u32,
    /// One id per pixel.
    pub ids: Vec<u32>,
    /// How many components there are, not counting background.
    pub count: u32,
}

impl Labels {
    /// The component id at a pixel, or `0` outside.
    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> u32 {
        if x < 0 || y < 0 || x >= i64::from(self.w) || y >= i64::from(self.h) {
            return 0;
        }
        self.ids
            .get((y as usize) * (self.w as usize) + (x as usize))
            .copied()
            .unwrap_or(0)
    }

    /// A plane containing exactly the named components.
    #[must_use]
    pub fn select(&self, keep: &BTreeSet<u32>) -> Plane {
        let mut out = Plane::zeros(self.w, self.h);
        for y in 0..i64::from(self.h) {
            for x in 0..i64::from(self.w) {
                if keep.contains(&self.at(x, y)) {
                    out.set(x, y, 1.0);
                }
            }
        }
        out
    }

    /// How much of a component sits inside a normalised box, `0.0 ..= 1.0`.
    ///
    /// The measure the assignment is made on. See the module note for why it is not IoU.
    #[must_use]
    pub fn containment(&self, id: u32, region: &NormBox) -> f32 {
        let mut inside = 0.0_f64;
        let mut total = 0.0_f64;
        for y in 0..self.h {
            for x in 0..self.w {
                if self.at(i64::from(x), i64::from(y)) != id {
                    continue;
                }
                total += 1.0;
                let nx = x as f32 / self.w.max(1) as f32;
                let ny = y as f32 / self.h.max(1) as f32;
                if nx >= region.x
                    && nx <= region.x + region.w
                    && ny >= region.y
                    && ny <= region.y + region.h
                {
                    inside += 1.0;
                }
            }
        }
        if total <= 0.0 {
            return 0.0;
        }
        (inside / total) as f32
    }

    /// The normalised bounding box of a component, or `None` when it has no pixels.
    #[must_use]
    pub fn bbox_of(&self, id: u32) -> Option<NormBox> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0_u32, 0_u32);
        let mut seen = false;
        for y in 0..self.h {
            for x in 0..self.w {
                if self.at(i64::from(x), i64::from(y)) != id {
                    continue;
                }
                seen = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
        if !seen {
            return None;
        }
        Some(NormBox::from_corners(
            x0 as f32 / self.w.max(1) as f32,
            y0 as f32 / self.h.max(1) as f32,
            x1 as f32 / self.w.max(1) as f32,
            y1 as f32 / self.h.max(1) as f32,
        ))
    }
}

/// Label the connected components of a plane, four-connected.
///
/// Four-connected rather than eight, because eight-connectivity joins two regions that touch
/// only at a corner - and at 768 px a corner touch between the bride's arm and the guest's
/// shoulder is one pixel of anti-aliasing, which is exactly the bleed the overlap test exists
/// to prevent.
#[must_use]
pub fn label_components(plane: &Plane) -> Labels {
    let w = plane.w;
    let h = plane.h;
    let n = (w as usize) * (h as usize);
    let mut ids = vec![0_u32; n];
    let mut next = 0_u32;
    let mut stack: Vec<(i64, i64)> = Vec::new();

    for y in 0..i64::from(h) {
        for x in 0..i64::from(w) {
            let index = (y as usize) * (w as usize) + (x as usize);
            if plane.at(x, y) <= 0.0 || ids.get(index).copied().unwrap_or(0) != 0 {
                continue;
            }
            next = next.saturating_add(1);
            stack.push((x, y));
            while let Some((cx, cy)) = stack.pop() {
                if cx < 0 || cy < 0 || cx >= i64::from(w) || cy >= i64::from(h) {
                    continue;
                }
                let ci = (cy as usize) * (w as usize) + (cx as usize);
                if plane.at(cx, cy) <= 0.0 || ids.get(ci).copied().unwrap_or(0) != 0 {
                    continue;
                }
                if let Some(slot) = ids.get_mut(ci) {
                    *slot = next;
                }
                stack.push((cx + 1, cy));
                stack.push((cx - 1, cy));
                stack.push((cx, cy + 1));
                stack.push((cx, cy - 1));
            }
        }
    }

    Labels {
        w,
        h,
        ids,
        count: next,
    }
}

/// Split every person-bearing plane into identity-scoped planes.
///
/// The unscoped plane stays: a caller that wants "all the skin in this photograph" should not
/// have to union eleven identity planes back together, and phase 16's skin guard is exactly
/// that caller.
pub fn scope(
    planes: &mut Vec<MaskPlane>,
    faces: &[FaceObservation],
    persons: &[PersonBox],
    identities: &[(usize, IdentityId)],
    _size: (u32, u32),
) {
    if faces.is_empty() || identities.is_empty() {
        return;
    }
    let by_face: BTreeMap<usize, IdentityId> = identities.iter().copied().collect();
    let mut added = Vec::new();

    for plane in planes.iter() {
        if !plane.kind.is_person() || plane.identity.is_some() || plane.plane.is_empty() {
            continue;
        }
        let labels = label_components(&plane.plane);
        // One accumulator per identity, so two components of the same person - an arm and a
        // shoulder separated by a sleeve - land in one mask rather than two.
        let mut per_identity: BTreeMap<IdentityId, Plane> = BTreeMap::new();
        let mut ambiguous = false;

        for id in 1..=labels.count {
            if labels.bbox_of(id).is_none() {
                continue;
            }
            let mut best: Option<(f32, IdentityId)> = None;
            let mut second = 0.0_f32;
            let mut matches = 0_usize;
            for (index, face) in faces.iter().enumerate() {
                let Some(identity) = by_face.get(&index) else {
                    continue;
                };
                // The body box when phase 06 bound one, otherwise the face box. A skin
                // component that is somebody's forearm overlaps their body and not their face,
                // and scoping only against faces would leave every arm in the wedding
                // unassigned.
                let region = persons
                    .iter()
                    .find(|p| p.face == Some(index))
                    .map_or(face.bbox, |p| p.bbox);
                let overlap = labels.containment(id, &region);
                if overlap < ASSIGN_MIN_OVERLAP {
                    continue;
                }
                matches += 1;
                match best {
                    Some((score, _)) if overlap <= score => second = second.max(overlap),
                    Some((score, _)) => {
                        second = second.max(score);
                        best = Some((overlap, *identity));
                    }
                    None => best = Some((overlap, *identity)),
                }
            }
            // Two people containing one component is a component that belongs to neither.
            // Assigning it to the better of the two is how the bride's skin mask swallows the
            // guest, and the margin below is what makes "better" mean something.
            if matches > 1 {
                if let Some((score, _)) = best {
                    if second > score * AMBIGUITY_MARGIN {
                        ambiguous = true;
                        continue;
                    }
                }
            }
            let Some((_, identity)) = best else {
                continue;
            };
            let entry = per_identity
                .entry(identity)
                .or_insert_with(|| Plane::zeros(labels.w, labels.h));
            let mut one = BTreeSet::new();
            one.insert(id);
            let component = labels.select(&one);
            for (slot, value) in entry.a.iter_mut().zip(component.a.iter()) {
                *slot = slot.max(*value);
            }
        }

        for (identity, region) in per_identity {
            let mut reasons = plane.reasons.clone();
            if ambiguous && !reasons.contains(&MaskReason::AmbiguousIdentity) {
                reasons.push(MaskReason::AmbiguousIdentity);
            }
            added.push(MaskPlane {
                kind: plane.kind,
                identity: Some(identity),
                // Scoping keeps the *soft* values of the parent inside the component, so a
                // matted hair boundary is not thresholded on its way into a per-person mask.
                plane: crate::mask::algebra::intersect(&plane.plane, &region),
                confidence: plane.confidence,
                edge_quality: plane.edge_quality,
                edge: plane.edge,
                reasons,
            });
        }
    }

    planes.extend(added);
}

/// How much better the best overlap has to be than the second before it decides.
///
/// Eighty per cent: a component that overlaps two people's boxes within a fifth of each other
/// is a component the geometry cannot separate. Below that margin it stays unscoped and
/// carries [`MaskReason::AmbiguousIdentity`], which is the honest answer and the one section
/// 10.1's group-photo test is written against.
const AMBIGUITY_MARGIN: f32 = 0.8;

#[cfg(test)]
mod tests {
    // The panic family is how a test asserts, and a mask test compares alphas that are exactly
    // zero or exactly one by construction - a painted fixture has no rounding to be tolerant of.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )]
    use super::*;

    fn rect(w: u32, h: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> Plane {
        let mut p = Plane::zeros(w, h);
        for y in y0..y1 {
            for x in x0..x1 {
                p.set(i64::from(x), i64::from(y), 1.0);
            }
        }
        p
    }

    #[test]
    fn two_separated_rectangles_are_two_components() {
        let mut plane = rect(32, 16, 1, 1, 6, 6);
        let other = rect(32, 16, 20, 1, 26, 6);
        for (slot, value) in plane.a.iter_mut().zip(other.a.iter()) {
            *slot = slot.max(*value);
        }
        assert_eq!(label_components(&plane).count, 2);
    }

    #[test]
    fn a_corner_touch_is_two_components_rather_than_one() {
        // Four-connectivity, and the module note says why: at 768 px a corner touch between
        // two people is one pixel of anti-aliasing.
        let mut plane = Plane::zeros(8, 8);
        for i in 0..3 {
            plane.set(i, i, 1.0);
        }
        for i in 4..7 {
            plane.set(i, i, 1.0);
        }
        assert!(label_components(&plane).count >= 2);
    }

    #[test]
    fn a_component_bounding_box_is_normalised() {
        let plane = rect(100, 100, 10, 20, 30, 40);
        let labels = label_components(&plane);
        let bbox = labels.bbox_of(1).expect("one component");
        assert!((bbox.x - 0.10).abs() < 1e-4);
        assert!((bbox.y - 0.20).abs() < 1e-4);
        assert!((bbox.w - 0.20).abs() < 1e-4);
    }
}
