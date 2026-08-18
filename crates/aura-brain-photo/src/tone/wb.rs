//! Scoring an illuminant hypothesis by what it does to skin.
//!
//! PHASE-15 section 6.2's second bullet, which is the heart of this phase:
//!
//! > Score each hypothesis by how plausible it makes the *skin* of known identities: project
//! > skin patches into a chromaticity space and measure distance from that person's
//! > estimated locus (accumulated across the wedding, so a single bad frame cannot mislead).
//!
//! and section 6.3's third:
//!
//! > The skin locus constraint is a hard constraint in the solve, not a post-hoc adjustment.
//!
//! Those two sentences describe two different mechanisms and this module implements both.
//! The **score** is soft: a hypothesis that leaves skin a little off its locus is worse than
//! one that does not, and it can still win on other evidence. The **constraint** is hard: a
//! hypothesis that leaves a prominent person's skin *outside* their locus is removed from
//! consideration entirely, and no amount of neutral evidence brings it back.
//!
//! The difference matters because of what a soft-only version does. With a penalty alone, a
//! confident but wrong neutral - a beige wall, a cream cake under a gel - can outweigh the
//! skin term and take the whole frame somewhere that makes a person the wrong colour. A hard
//! constraint means the worst that a wrong neutral can do is choose badly *among* the
//! answers that keep skin plausible.
//!
//! ## What happens when there is no locus
//!
//! Nothing, and loudly. [`Constraints::is_empty`] is checked by the solver, which emits
//! `ToneCode::SkinLocusUnavailable`, drops the confidence and lets the frame reach the review
//! queue. It does **not** substitute a default skin colour, because a default skin colour is
//! the thing section 6.3 exists to prevent, and a phase that quietly used one when it had no
//! measurement would be exactly as biased as one that always used one - just harder to find.

use aura_core::contract::tone::SkinLocus;
use aura_core::IdentityId;
use aura_raw::colour::illuminant::{adapt, linear_srgb_to_uv, uv_distance};

use crate::tone::illuminant::Hypothesis;
use crate::tone::neutrals::NeutralPatch;

/// How much the skin term weighs in the cost.
///
/// The largest of the three, and deliberately so: section 1 says photographers judge an
/// editor on "correct exposure and believable skin colour", and the neutral term is evidence
/// *about* the light while this is evidence about the result.
pub const SKIN_WEIGHT: f32 = 0.55;

/// How much agreement with a detected neutral weighs.
pub const NEUTRAL_WEIGHT: f32 = 0.30;

/// How much the generator's own prior weighs.
///
/// The smallest, because a prior is what we believed before looking at this photograph and
/// the other two terms are what the photograph says.
pub const PRIOR_WEIGHT: f32 = 0.15;

/// The prominence below which a person's locus does not veto a hypothesis.
///
/// A guest at the edge of a group photograph should not be able to reject the white balance
/// that makes the couple correct. The hard constraint applies to people the photograph is
/// *about*; everybody else contributes to the soft score.
pub const VETO_PROMINENCE: f32 = 0.25;

/// The distance at which the soft skin term saturates, in `u'v'`.
///
/// Roughly the distance between correct and a full 1500 K error. Past it, one hypothesis is
/// not usefully worse than another - both are wrong - and letting the term keep growing would
/// let a single badly-sampled face dominate a frame's whole cost.
pub const SKIN_SATURATION: f32 = 0.060;

/// One person's skin in this frame, and where their skin is known to sit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraint {
    /// Whose skin.
    pub identity: IdentityId,
    /// Mean linear RGB as observed in this frame.
    pub observed: [f32; 3],
    /// Where that person's skin sits under a neutral light.
    pub locus: SkinLocus,
    /// How much this photograph is about them, `0..1`. Phase 06's prominence.
    pub prominence: f32,
}

impl Constraint {
    /// Distance from this person's locus centre under a candidate light, in `u'v'`.
    #[must_use]
    pub fn distance_under(&self, illuminant_uv: [f32; 2]) -> f32 {
        let reflectance = adapt(self.observed, illuminant_uv);
        self.locus.distance(linear_srgb_to_uv(reflectance))
    }

    /// True when this light would put this person outside their plausible region.
    #[must_use]
    pub fn violated_by(&self, illuminant_uv: [f32; 2]) -> bool {
        let reflectance = adapt(self.observed, illuminant_uv);
        !self.locus.contains(linear_srgb_to_uv(reflectance))
    }

    /// True when this person is prominent enough to veto.
    #[must_use]
    pub fn can_veto(&self) -> bool {
        self.prominence >= VETO_PROMINENCE && self.locus.is_usable()
    }
}

/// Every skin constraint this frame carries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Constraints {
    /// One per identity with a usable locus and a sampled patch, most prominent first.
    pub people: Vec<Constraint>,
}

impl Constraints {
    /// True when nothing constrains the solve.
    ///
    /// The condition `AURA-ML-5065` is raised on.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
    }

    /// How many people can veto a hypothesis.
    #[must_use]
    pub fn vetoing(&self) -> usize {
        self.people.iter().filter(|p| p.can_veto()).count()
    }

    /// True when any prominent person's skin would be implausible under this light.
    ///
    /// **The hard constraint.** Section 6.3.
    #[must_use]
    pub fn rejects(&self, illuminant_uv: [f32; 2]) -> bool {
        self.people
            .iter()
            .filter(|person| person.can_veto())
            .any(|person| person.violated_by(illuminant_uv))
    }

    /// The prominence-weighted mean distance from the loci under this light, `0..1`.
    ///
    /// The soft term. Everybody contributes, prominent or not: a guest whose skin goes green
    /// is still a guest whose skin goes green, and their opinion should be heard even when it
    /// cannot veto.
    #[must_use]
    pub fn error_under(&self, illuminant_uv: [f32; 2]) -> f32 {
        if self.people.is_empty() {
            return 0.0;
        }
        let mut total = 0.0f64;
        let mut weight_total = 0.0f64;
        for person in &self.people {
            let weight = f64::from(person.prominence.max(0.05));
            let distance = person.distance_under(illuminant_uv);
            // Only the part *outside* the locus counts. Inside it, every position is equally
            // plausible by definition - that is what a locus is - and charging for distance
            // from its exact centre would make the solve chase a mean that was never a
            // target.
            let outside = (distance - person.locus.radius).max(0.0);
            total += f64::from((outside / SKIN_SATURATION).clamp(0.0, 1.0)) * weight;
            weight_total += weight;
        }
        if weight_total <= 0.0 {
            return 0.0;
        }
        (total / weight_total) as f32
    }
}

/// What one hypothesis costs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// The fused cost. Lower is better.
    pub cost: f32,
    /// The skin term, `0..1`.
    pub skin: f32,
    /// The neutral term, `0..1`.
    pub neutral: f32,
    /// True when a prominent person's locus rejected this light outright.
    pub vetoed: bool,
}

impl Score {
    /// The cost a vetoed hypothesis carries.
    ///
    /// Above any reachable cost, so a vetoed hypothesis sorts last even in a frame where
    /// every survivor is bad. Not infinity: an infinity in a sort key is a `total_cmp` that
    /// still works and a mean that does not, and the alternatives list reports these costs.
    pub const VETOED: f32 = 10.0;
}

/// Score one hypothesis.
///
/// `patches` is the frame's detected neutrals; a hypothesis is rewarded for agreeing with
/// them, weighted by how strong each patch is. The [`crate::tone::illuminant::Hypothesis`]
/// that *came from* a patch is not given a free ride here - it is scored against all of them,
/// so a frame with two disagreeing neutral candidates does not simply pick whichever was
/// found first.
#[must_use]
pub fn score(
    hypothesis: &Hypothesis,
    constraints: &Constraints,
    patches: &[NeutralPatch],
) -> Score {
    let vetoed = constraints.rejects(hypothesis.uv);
    let skin = constraints.error_under(hypothesis.uv);
    let neutral = neutral_error(hypothesis.uv, patches);
    let prior = 1.0 - hypothesis.prior.clamp(0.0, 1.0);

    let cost = if vetoed {
        Score::VETOED
    } else {
        SKIN_WEIGHT * skin + NEUTRAL_WEIGHT * neutral + PRIOR_WEIGHT * prior
    };
    Score {
        cost,
        skin,
        neutral,
        vetoed,
    }
}

/// How far this light is from what the detected neutrals say, `0..1`.
///
/// Zero when there are no neutrals, which is the right answer rather than a missing one: a
/// frame with no neutral evidence should have the neutral term contribute nothing, not
/// contribute a penalty that is the same for every hypothesis and therefore changes no
/// ordering while making every cost look worse.
fn neutral_error(uv: [f32; 2], patches: &[NeutralPatch]) -> f32 {
    if patches.is_empty() {
        return 0.0;
    }
    let mut total = 0.0f64;
    let mut weight_total = 0.0f64;
    for patch in patches {
        let weight = f64::from(patch.strength().max(0.02));
        total += f64::from((uv_distance(uv, patch.uv) / SKIN_SATURATION).clamp(0.0, 1.0)) * weight;
        weight_total += weight;
    }
    if weight_total <= 0.0 {
        return 0.0;
    }
    (total / weight_total) as f32
}

/// The estimated skin error a chosen light leaves, in dE00.
///
/// What `ToneEstimate::skin_de00_estimate` carries. Measured against each person's own locus
/// rather than against a chart, so it is a statement about **consistency across this
/// wedding** and not about accuracy - which is exactly what the product can honestly compute
/// without an expert reference, and exactly what phase 25 needs to normalise a gallery.
///
/// The conversion from a `u'v'` distance to dE00 is a linearisation around the region skin
/// occupies. It is an estimate of an estimate and it is labelled as one everywhere it
/// surfaces; the eval harness computes the real thing with `aura_raw::colour::de2000`
/// against reference patches, which is what section 10.1's gate is measured with.
#[must_use]
pub fn skin_de00_estimate(constraints: &Constraints, illuminant_uv: [f32; 2]) -> f32 {
    /// dE00 per unit of `u'v'` distance, near skin.
    ///
    /// About 160. A chromaticity error of 0.006 - a small but visible cast on a face - is
    /// roughly one dE00 unit at the lightness skin sits at, and the relationship is close
    /// enough to linear across the range this is ever asked about.
    const DE00_PER_UV: f32 = 160.0;

    if constraints.people.is_empty() {
        return 0.0;
    }
    let mut total = 0.0f64;
    let mut weight_total = 0.0f64;
    for person in &constraints.people {
        let weight = f64::from(person.prominence.max(0.05));
        total += f64::from(person.distance_under(illuminant_uv)) * weight;
        weight_total += weight;
    }
    if weight_total <= 0.0 {
        return 0.0;
    }
    ((total / weight_total) as f32 * DE00_PER_UV).clamp(0.0, 100.0)
}
