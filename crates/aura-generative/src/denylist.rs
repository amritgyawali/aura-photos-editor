//! The semantic denylist: the intersection against phase 18's masks.
//!
//! Six kinds are never removed from around - faces, skin, hands, dress, rings and cake - and a
//! candidate that overlaps any of them by more than the policy's share of its own area is blocked.
//!
//! ## An absent mask blocks. It does not pass.
//!
//! This is the single most consequential line in the crate, and it inverts what every phase from
//! 19 to 23 does. Those phases *gated* an operation when an input was missing: a local light
//! adjustment with no mask ran at zero strength, a sharpen with no regions did not sharpen. The
//! safe direction there was less.
//!
//! Here the safe direction is none, and "gated to zero" and "blocked" would be indistinguishable
//! in a panel while meaning completely different things. One says the product checked and found
//! nothing to worry about. The other says it could not check. Only the first is a claim, and a
//! build with no segmenter must not be able to produce it.
//!
//! So [`Coverage::Absent`] returns [`Verdict::Unknown`], the safety engine turns that into
//! `CleanupCode::ProtectionUnknown` rather than `CleanupCode::OverlapsProtected`, and the two are
//! separate rows, separate reason codes and separate runbooks.
//!
//! ADR-0049 section 3.

use aura_core::contract::cleanup::Box2;

/// One of the six kinds a removal is never allowed to reach into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protected {
    /// A face, from phase 18's `Face` class or phase 06's boxes.
    Face,
    /// Any skin, which is wider than a face and catches an arm crossing the frame.
    Skin,
    /// Hands, including joined hands, which phase 23 also protects and for the same reason.
    Hands,
    /// A dress or a garment whose fabric a fill would smear.
    Dress,
    /// Rings, the smallest protected thing and the one a size cap would never catch.
    Rings,
    /// The cake, which is an object and is also the point of the photograph it is in.
    Cake,
}

impl Protected {
    /// Every kind, in a fixed order.
    pub const ALL: [Self; 6] = [
        Self::Face,
        Self::Skin,
        Self::Hands,
        Self::Dress,
        Self::Rings,
        Self::Cake,
    ];

    /// The phase 18 mask class slug this reads.
    #[must_use]
    pub const fn mask_kind(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Skin => "skin",
            Self::Hands => "hands",
            Self::Dress => "dress",
            Self::Rings => "rings",
            Self::Cake => "cake",
        }
    }

    /// The stored slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.mask_kind()
    }
}

/// What is known about one photograph's protected regions.
///
/// The distinction between [`Self::Known`] with an empty list and [`Self::Absent`] is the whole
/// point of this module: the first says the segmenter ran and found no protected content, the
/// second says it did not run.
#[derive(Debug, Clone, Default)]
pub enum Coverage {
    /// The masks arrived. The rectangles are the bounding boxes of each protected region.
    Known(Vec<(Protected, Box2)>),
    /// The masks did not arrive, so nothing can be shown to be safe.
    #[default]
    Absent,
}

impl Coverage {
    /// The masks arrived and found nothing protected.
    #[must_use]
    pub fn known_empty() -> Self {
        Self::Known(Vec::new())
    }

    /// True when the segmenter ran, whatever it found.
    #[must_use]
    pub const fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }
}

/// What the denylist has to say about one candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    /// The masks arrived and the candidate is clear of every protected region.
    Clear,
    /// The masks arrived and the candidate overlaps one, with the share of the candidate's own
    /// area that overlapped.
    Overlaps(Protected, f32),
    /// The masks did not arrive. Not a claim about the photograph.
    Unknown,
}

impl Verdict {
    /// True only for [`Self::Clear`]. Deliberately not `!matches!(self, Overlaps(..))`, so that
    /// adding a third failing variant later cannot silently become a pass.
    #[must_use]
    pub const fn permits_removal(self) -> bool {
        matches!(self, Self::Clear)
    }
}

/// The share of `candidate` that `other` covers, `0..1`.
///
/// The denominator is the **candidate's** area rather than the union or the protected region's,
/// because the question is "how much of what I am about to erase is her hand", and a hand that is
/// one per cent of a large mask can be a hundred per cent of a small candidate.
#[must_use]
pub fn overlap_fraction(candidate: &Box2, other: &Box2) -> f32 {
    let x0 = candidate.x.max(other.x);
    let y0 = candidate.y.max(other.y);
    let x1 = (candidate.x + candidate.w).min(other.x + other.w);
    let y1 = (candidate.y + candidate.h).min(other.y + other.h);
    if x1 <= x0 || y1 <= y0 {
        return 0.0;
    }
    let inter = (x1 - x0) * (y1 - y0);
    let area = candidate.w * candidate.h;
    if area <= 0.0 {
        return 1.0;
    }
    (inter / area).clamp(0.0, 1.0)
}

/// Judge one candidate against what is known about the photograph.
///
/// Returns the **worst** overlap rather than the first, so a stored reason names the thing a
/// photographer would most object to rather than whichever mask happened to be first in the list.
#[must_use]
pub fn judge(candidate: &Box2, coverage: &Coverage, max_overlap: f32) -> Verdict {
    let regions = match coverage {
        Coverage::Absent => return Verdict::Unknown,
        Coverage::Known(regions) => regions,
    };

    let mut worst: Option<(Protected, f32)> = None;
    for (kind, region) in regions {
        let share = overlap_fraction(candidate, region);
        if share <= max_overlap {
            continue;
        }
        match worst {
            Some((_, best)) if best >= share => {}
            _ => worst = Some((*kind, share)),
        }
    }

    match worst {
        Some((kind, share)) => Verdict::Overlaps(kind, share),
        None => Verdict::Clear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Box2 {
        Box2 { x, y, w, h }
    }

    #[test]
    fn an_absent_mask_is_unknown_and_unknown_does_not_permit_removal() {
        let verdict = judge(&rect(0.1, 0.1, 0.05, 0.05), &Coverage::Absent, 0.01);
        assert_eq!(verdict, Verdict::Unknown);
        assert!(!verdict.permits_removal());
    }

    #[test]
    fn a_mask_that_ran_and_found_nothing_is_clear_and_clear_does_permit_removal() {
        let verdict = judge(&rect(0.1, 0.1, 0.05, 0.05), &Coverage::known_empty(), 0.01);
        assert_eq!(verdict, Verdict::Clear);
        assert!(verdict.permits_removal());
    }

    #[test]
    fn the_two_absences_are_different_verdicts() {
        // The property this module exists for: "no protected content found" and "could not look"
        // must never be the same answer.
        let looked = judge(&rect(0.1, 0.1, 0.05, 0.05), &Coverage::known_empty(), 0.01);
        let did_not = judge(&rect(0.1, 0.1, 0.05, 0.05), &Coverage::Absent, 0.01);
        assert_ne!(looked, did_not);
    }

    #[test]
    fn overlap_is_measured_against_the_candidates_own_area() {
        // A small candidate wholly inside a large protected region overlaps by 1.0, not by the
        // tiny share of the region it covers.
        let candidate = rect(0.50, 0.50, 0.02, 0.02);
        let hand = rect(0.40, 0.40, 0.30, 0.30);
        let share = overlap_fraction(&candidate, &hand);
        assert!((share - 1.0).abs() < 1e-4, "share was {share}");
    }

    #[test]
    fn a_touch_below_the_threshold_is_clear_and_above_it_blocks() {
        let candidate = rect(0.50, 0.50, 0.10, 0.10);

        // Clearly below: a 0.0005-wide sliver of a 0.10-wide box is 0.5 % of it. Deliberately not
        // *at* 1 %, because a fixture sitting on its own threshold tests the arithmetic of f32
        // rather than the rule - the same trap phases 19, 21 and 22 each hit from the other side.
        let sleeve = rect(0.5995, 0.50, 0.20, 0.10);
        assert_eq!(
            judge(
                &candidate,
                &Coverage::Known(vec![(Protected::Skin, sleeve)]),
                0.01
            ),
            Verdict::Clear
        );

        // Clearly above: half the candidate.
        let arm = rect(0.55, 0.50, 0.20, 0.10);
        match judge(
            &candidate,
            &Coverage::Known(vec![(Protected::Skin, arm)]),
            0.01,
        ) {
            Verdict::Overlaps(Protected::Skin, share) => assert!(share > 0.4),
            other => panic!("expected an overlap, got {other:?}"),
        }
    }

    #[test]
    fn the_worst_overlap_is_reported_rather_than_the_first() {
        let candidate = rect(0.40, 0.40, 0.20, 0.20);
        let coverage = Coverage::Known(vec![
            (Protected::Cake, rect(0.55, 0.55, 0.10, 0.10)),
            (Protected::Face, rect(0.30, 0.30, 0.25, 0.25)),
        ]);
        match judge(&candidate, &coverage, 0.01) {
            Verdict::Overlaps(kind, _) => assert_eq!(kind, Protected::Face),
            other => panic!("expected an overlap, got {other:?}"),
        }
    }

    #[test]
    fn a_disjoint_region_does_not_overlap() {
        let candidate = rect(0.05, 0.05, 0.05, 0.05);
        let face = rect(0.60, 0.60, 0.20, 0.20);
        assert!(overlap_fraction(&candidate, &face) < f32::EPSILON);
    }
}
