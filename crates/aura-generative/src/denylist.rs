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
//! ## A kind phase 18 has no word for is a third absence, and it is also `Unknown`
//!
//! Phase 18's `MaskKind` vocabulary has twenty classes and **three of the six kinds here are not
//! among them**. `Face`, `Skin` and `Dress` map exactly; `Hands` maps onto `Skin`, which is a
//! superset and therefore protects more than asked; and `Rings` and `Cake` have no class at all.
//!
//! That is a second way for the evidence to be missing, one level up from the first, and it has
//! the same answer. A [`Coverage`] built from a segmenter that cannot name a ring is not evidence
//! that a candidate is clear of one, and marking it `Known` would be exactly the mistake the
//! paragraph above rejects, made in a place where it is much harder to see. So `Coverage` carries
//! *which* kinds it could resolve, [`Coverage::partial`] is how a caller says so, and a candidate
//! that clears every resolved kind while some kind was unresolvable comes back
//! [`Verdict::Unknown`] rather than [`Verdict::Clear`].
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

    /// How many there are, for the fixed-width resolved set on [`Coverage`].
    pub const COUNT: usize = Self::ALL.len();

    /// Position in [`Self::ALL`], which is the index into a resolved set.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Face => 0,
            Self::Skin => 1,
            Self::Hands => 2,
            Self::Dress => 3,
            Self::Rings => 4,
            Self::Cake => 5,
        }
    }

    /// The phase 18 `MaskKind` slug that can stand in for this kind, when one can.
    ///
    /// `Hands` reads `skin`, which is a **superset**: every hand is skin, so a candidate clear of
    /// all skin is clear of every hand, and the substitution can only refuse more than asked.
    /// That is the safe direction and it is the only substitution made here.
    ///
    /// `Rings` and `Cake` return `None`, because phase 18 has no word for either. A caller that
    /// treated `None` as "nothing to intersect" would have re-created the mistake this module
    /// exists to prevent; [`Coverage::partial`] is the shape that stops it.
    #[must_use]
    pub const fn phase18_kind(self) -> Option<&'static str> {
        match self {
            Self::Face => Some("face"),
            Self::Skin | Self::Hands => Some("skin"),
            Self::Dress => Some("dress"),
            Self::Rings | Self::Cake => None,
        }
    }

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
/// Three states rather than two, and the difference between them is the whole point of this
/// module:
///
/// * [`Self::Known`] with an empty list - the segmenter ran, could name every protected kind, and
///   found none of them in this photograph.
/// * [`Self::Known`] with `resolved` short of every kind - the segmenter ran and there is at least
///   one kind it has no word for, so what it found is not the whole answer.
/// * [`Self::Absent`] - it did not run.
///
/// Only the first is a claim about the photograph.
#[derive(Debug, Clone, Default)]
pub enum Coverage {
    /// The masks arrived. The rectangles are the bounding boxes of each protected region found.
    Known {
        /// Where each protected region is.
        regions: Vec<(Protected, Box2)>,
        /// Which kinds the segmenter could look for at all, indexed by [`Protected::index`].
        ///
        /// A `false` here is not "did not find one". It is "could not have found one", and
        /// [`judge`] turns it into [`Verdict::Unknown`] rather than into [`Verdict::Clear`].
        resolved: [bool; Protected::COUNT],
    },
    /// The masks did not arrive, so nothing can be shown to be safe.
    #[default]
    Absent,
}

impl Coverage {
    /// The masks arrived, every kind was resolvable, and these are the regions found.
    ///
    /// The constructor the fixtures and the gates use, because a fixture author knows what is in
    /// their own frame. A pass over a real photograph goes through [`Self::partial`].
    #[must_use]
    pub fn known(regions: Vec<(Protected, Box2)>) -> Self {
        Self::Known {
            regions,
            resolved: [true; Protected::COUNT],
        }
    }

    /// The masks arrived, and these are the kinds the segmenter could look for.
    #[must_use]
    pub fn partial(regions: Vec<(Protected, Box2)>, resolved: [bool; Protected::COUNT]) -> Self {
        Self::Known { regions, resolved }
    }

    /// The masks arrived, every kind was resolvable, and nothing protected is in the frame.
    #[must_use]
    pub fn known_empty() -> Self {
        Self::known(Vec::new())
    }

    /// True when the segmenter ran, whatever it found and whatever it could not look for.
    #[must_use]
    pub const fn is_known(&self) -> bool {
        matches!(self, Self::Known { .. })
    }

    /// True when the segmenter ran **and** could look for every one of the six kinds.
    ///
    /// The number `CleanupOutline::mask_covered` counts, because it is the one that says whether a
    /// project's blocked histogram is about photographs or about a missing vocabulary.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        match self {
            Self::Known { resolved, .. } => resolved.iter().all(|ok| *ok),
            Self::Absent => false,
        }
    }

    /// The kinds this coverage could not look for, in [`Protected::ALL`] order.
    #[must_use]
    pub fn unresolved(&self) -> Vec<Protected> {
        match self {
            Self::Known { resolved, .. } => Protected::ALL
                .into_iter()
                .filter(|kind| !resolved.get(kind.index()).copied().unwrap_or(false))
                .collect(),
            Self::Absent => Protected::ALL.to_vec(),
        }
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
    let (regions, resolved) = match coverage {
        Coverage::Absent => return Verdict::Unknown,
        Coverage::Known { regions, resolved } => (regions, resolved),
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

    // A found overlap outranks an unresolvable kind, because it is the stronger statement: one
    // says the product looked and found somebody, the other says part of the question was not
    // askable. A panel showing the second when the first is true would be telling a photographer
    // less than the product knows.
    if let Some((kind, share)) = worst {
        return Verdict::Overlaps(kind, share);
    }
    if !resolved.iter().all(|ok| *ok) {
        return Verdict::Unknown;
    }
    Verdict::Clear
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
                &Coverage::known(vec![(Protected::Skin, sleeve)]),
                0.01
            ),
            Verdict::Clear
        );

        // Clearly above: half the candidate.
        let arm = rect(0.55, 0.50, 0.20, 0.10);
        match judge(
            &candidate,
            &Coverage::known(vec![(Protected::Skin, arm)]),
            0.01,
        ) {
            Verdict::Overlaps(Protected::Skin, share) => assert!(share > 0.4),
            other => panic!("expected an overlap, got {other:?}"),
        }
    }

    #[test]
    fn the_worst_overlap_is_reported_rather_than_the_first() {
        let candidate = rect(0.40, 0.40, 0.20, 0.20);
        let coverage = Coverage::known(vec![
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

    #[test]
    fn a_kind_the_segmenter_has_no_word_for_is_unknown_rather_than_clear() {
        // Phase 18 can name a face, skin and a dress. It has no class for a ring, so a coverage
        // built from it is not evidence that a candidate is clear of one - even when everything
        // it *could* look for came back empty.
        let mut resolved = [true; Protected::COUNT];
        resolved[Protected::Rings.index()] = false;
        let coverage = Coverage::partial(Vec::new(), resolved);
        assert_eq!(
            judge(&rect(0.02, 0.90, 0.04, 0.04), &coverage, 0.01),
            Verdict::Unknown
        );
        assert!(coverage.is_known());
        assert!(!coverage.is_complete());
        assert_eq!(coverage.unresolved(), vec![Protected::Rings]);
    }

    #[test]
    fn a_found_overlap_outranks_an_unresolvable_kind() {
        let mut resolved = [true; Protected::COUNT];
        resolved[Protected::Cake.index()] = false;
        let coverage = Coverage::partial(
            vec![(Protected::Face, rect(0.40, 0.40, 0.25, 0.25))],
            resolved,
        );
        match judge(&rect(0.45, 0.45, 0.10, 0.10), &coverage, 0.01) {
            Verdict::Overlaps(Protected::Face, _) => {}
            other => panic!("a found face must outrank an unaskable question, got {other:?}"),
        }
    }

    #[test]
    fn hands_read_skin_and_rings_and_cake_read_nothing() {
        assert_eq!(Protected::Hands.phase18_kind(), Some("skin"));
        assert_eq!(Protected::Skin.phase18_kind(), Some("skin"));
        assert_eq!(Protected::Rings.phase18_kind(), None);
        assert_eq!(Protected::Cake.phase18_kind(), None);
        // The index is the position in ALL and nothing else may reorder it: it is what a stored
        // resolved set is written against.
        for (position, kind) in Protected::ALL.into_iter().enumerate() {
            assert_eq!(kind.index(), position);
        }
    }
}
