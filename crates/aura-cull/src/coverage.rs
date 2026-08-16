//! The anti-catastrophe layer.
//!
//! PHASE-12 section 6.3. Everything else in this crate is a preference; this is the part
//! that cannot be traded away. Section 1 says why: "a single lost 'must-have' frame loses a
//! customer forever."
//!
//! ## Three states, and the third one is a promise about honesty
//!
//! * `Covered` - the rule has its frames and they all cleared the scene's floor.
//! * `CoveredWeak` - it has its frames because the guard **forced** some in from below the
//!   floor, or from the vetoed pile. They are in the gallery; the warning is about their
//!   quality, not their presence.
//! * `Missing` - **no candidate existed at all**. Section 6.3: "the product must be clear
//!   that it cannot invent coverage."
//!
//! `Missing` is never the outcome of a decision. If a candidate existed and the gallery
//! does not contain it, that is a bug and the phase gate fails on it.
//!
//! ## A veto excludes a frame from candidacy, not from a guarantee
//!
//! The rule that surprises everyone who reads this module. If the only photograph of the
//! ring exchange is out of focus, the guard adds it, marks the rule `CoveredWeak` and
//! writes a warning naming the veto.
//!
//! A blurred photograph of the rings beats no photograph of the rings. A product that
//! dropped it silently because a threshold said so is the failure section 12 opens with,
//! and it would be a *defensible* failure at the level of the single frame - which is
//! precisely what makes it dangerous.
//!
//! ## Identity coverage is the feature nobody asks for and everybody notices
//!
//! Section 6.3: "every close-family identity gets >= 3 frames, every recurring guest >= 1;
//! this is the feature that prevents 'my aunt isn't in the gallery' complaints."
//!
//! It is expensive in gallery slots and it is worth every one of them, because the failure
//! it prevents is the only kind a couple discovers *after* delivery, by looking, with
//! their family.
//!
//! ## What the guard is not allowed to do
//!
//! It cannot reject anything. It only ever adds. That asymmetry is what makes running it
//! twice - once after the chapter pass and once after sizing - safe and idempotent: the
//! second run finds the first run's additions already present and does nothing.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::cull::ImageId;
use aura_core::{Coverage, CoverageReport, IdentityId, KeepScore, MustHave};

use crate::input::{Candidate, CullInput};
use crate::rules::{CoverageRule, Prefer, RuleTable};

/// The coverage guard.
///
/// Borrows the rule table rather than owning it, because the table is loaded once per
/// process and the guard runs twice per selection.
#[derive(Debug)]
pub struct Guard<'a> {
    rules: &'a RuleTable,
}

/// What one run of the guard did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GuardOutcome {
    /// Frames the guard forced into the gallery, and which guarantee holds each one.
    pub added: BTreeMap<ImageId, MustHave>,
    /// How every rule came out, in `MustHave::ALL` order.
    pub states: Vec<(MustHave, Coverage)>,
    /// How many frames of the gallery each identity appears in.
    pub identity_coverage: Vec<(IdentityId, u32)>,
    /// How many candidates each rule had at all. The number behind `Missing`.
    pub candidates: BTreeMap<MustHave, u32>,
    /// How many frames each rule ended up with.
    pub found: BTreeMap<MustHave, u32>,
    /// Everything a photographer should read before delivering.
    pub warnings: Vec<String>,
    /// The same warnings, indexed by the rule they are about.
    ///
    /// The flat list is what the coverage panel shows and what the contract carries; this
    /// is what `coverage_report.warning` stores per row. Both, rather than one derived from
    /// the other, because two of the warnings - the unconfirmed couple and an identity
    /// shortfall - belong to the run rather than to any rule, and a flat list reconstructed
    /// from a per-rule map would silently lose them.
    pub rule_warnings: BTreeMap<MustHave, String>,
    /// Rules that found nothing, for `AURA-ML-5053`.
    pub missing: Vec<MustHave>,
}

impl<'a> Guard<'a> {
    /// A guard over one rule table.
    #[must_use]
    pub const fn new(rules: &'a RuleTable) -> Self {
        Self { rules }
    }

    /// Force every guarantee, adding to `kept` in place.
    ///
    /// Idempotent. Running it a second time over a gallery it has already fixed adds
    /// nothing, which is what makes section 6.4's "the coverage guard always runs last"
    /// cheap enough to actually do.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn enforce(
        &self,
        input: &CullInput,
        scores: &BTreeMap<ImageId, KeepScore>,
        floors: &BTreeMap<ImageId, f32>,
        overrides: &BTreeMap<ImageId, bool>,
        kept: &mut BTreeSet<ImageId>,
    ) -> GuardOutcome {
        let mut outcome = GuardOutcome::default();
        let by_id: BTreeMap<ImageId, &Candidate> = input
            .candidates
            .iter()
            .map(|candidate| (candidate.image_id, candidate))
            .collect();

        for rule in self.rules.rules() {
            if rule.id.is_identity_rule() {
                continue;
            }
            let state = enforce_rule(rule, input, scores, floors, overrides, kept, &mut outcome);
            outcome.states.push((rule.id, state));
        }

        let close_family =
            self.enforce_identities(input, scores, floors, overrides, &by_id, kept, &mut outcome);
        outcome.states.push((MustHave::CloseFamily, close_family));

        // Report every identity, including the ones at zero. The zero is the number the
        // panel exists to show.
        let mut coverage = Vec::with_capacity(input.identities.len());
        for identity in &input.identities {
            let count = kept
                .iter()
                .filter_map(|image| by_id.get(image))
                .filter(|candidate| candidate.identities.contains(&identity.id))
                .count();
            coverage.push((identity.id, u32::try_from(count).unwrap_or(u32::MAX)));
        }
        outcome.identity_coverage = coverage;

        if input.couple_unconfirmed {
            outcome.warnings.push(
                "AURA worked out who the couple are from the photographs and nobody has \
                 confirmed it. If that is wrong, the rules about who must appear in the \
                 gallery were applied to the wrong people."
                    .to_string(),
            );
        }

        outcome.states.sort_by_key(|(rule, _)| position_of(*rule));
        outcome.missing = outcome
            .states
            .iter()
            .filter(|(_, state)| *state == Coverage::Missing)
            .map(|(rule, _)| *rule)
            .collect();
        outcome
    }

    /// Turn a finished run into the report the contract carries.
    #[must_use]
    pub fn report(outcome: &GuardOutcome) -> CoverageReport {
        CoverageReport {
            must_haves: outcome.states.clone(),
            identity_coverage: outcome.identity_coverage.clone(),
            chapter_counts: Vec::new(),
            warnings: outcome.warnings.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn enforce_identities(
        &self,
        input: &CullInput,
        scores: &BTreeMap<ImageId, KeepScore>,
        floors: &BTreeMap<ImageId, f32>,
        overrides: &BTreeMap<ImageId, bool>,
        by_id: &BTreeMap<ImageId, &Candidate>,
        kept: &mut BTreeSet<ImageId>,
        outcome: &mut GuardOutcome,
    ) -> Coverage {
        let policy = self.rules.identity();
        let mut family = 0u32;
        let mut satisfied = 0u32;
        let mut weak = false;

        for identity in &input.identities {
            let required = if identity.close_family || identity.primary {
                policy.close_family_min
            } else if identity.appearances >= policy.guest_recurrence {
                policy.guest_min
            } else {
                0
            };
            if required == 0 {
                continue;
            }
            if identity.close_family || identity.primary {
                family = family.saturating_add(1);
            }

            let mut have: u32 = kept
                .iter()
                .filter_map(|image| by_id.get(image))
                .filter(|candidate| candidate.identities.contains(&identity.id))
                .count()
                .try_into()
                .unwrap_or(u32::MAX);

            let mut available: Vec<&Candidate> = input
                .candidates
                .iter()
                .filter(|candidate| candidate.analysed)
                .filter(|candidate| candidate.identities.contains(&identity.id))
                .filter(|candidate| !kept.contains(&candidate.image_id))
                .filter(|candidate| overrides.get(&candidate.image_id) != Some(&false))
                .collect();
            order(&mut available, Prefer::Score, scores);

            for candidate in available {
                if have >= required {
                    break;
                }
                let score = scores
                    .get(&candidate.image_id)
                    .map_or(0.0, |keep| keep.scene_weighted);
                let floor = floors.get(&candidate.image_id).copied().unwrap_or(0.0);
                if score < floor || candidate.veto.is_some() {
                    weak = true;
                }
                kept.insert(candidate.image_id);
                outcome
                    .added
                    .insert(candidate.image_id, MustHave::CloseFamily);
                have = have.saturating_add(1);
            }

            if have >= required {
                if identity.close_family || identity.primary {
                    satisfied = satisfied.saturating_add(1);
                }
            } else {
                weak = true;
                outcome.warnings.push(format!(
                    "One person appears in {have} of the gallery's photographs, and this \
                     kind of guest normally appears in at least {required}. There were no \
                     other photographs of them to add."
                ));
            }
        }

        outcome.candidates.insert(MustHave::CloseFamily, family);
        outcome.found.insert(MustHave::CloseFamily, satisfied);
        if family == 0 {
            // No close family was identified at all. That is a phase 06 gap rather than a
            // culling decision, and reporting it as `Missing` sends somebody to look at
            // the right phase.
            outcome.warnings.push(
                "AURA has not identified any close family in this wedding, so it could not \
                 guarantee that each of them is in the gallery."
                    .to_string(),
            );
            return Coverage::Missing;
        }
        if satisfied == 0 {
            return Coverage::Missing;
        }
        if weak || satisfied < family {
            return Coverage::CoveredWeak;
        }
        Coverage::Covered
    }
}

/// Force one guarantee, adding to `kept` in place.
///
/// A free function rather than a method: it reads the rule it is handed and nothing else
/// about the guard, and a `&self` it never touches is a `&self` that invites somebody to
/// start touching it.
#[allow(clippy::too_many_arguments)]
fn enforce_rule(
    rule: &CoverageRule,
    input: &CullInput,
    scores: &BTreeMap<ImageId, KeepScore>,
    floors: &BTreeMap<ImageId, f32>,
    overrides: &BTreeMap<ImageId, bool>,
    kept: &mut BTreeSet<ImageId>,
    outcome: &mut GuardOutcome,
) -> Coverage {
    let mut candidates: Vec<&Candidate> = input
        .candidates
        .iter()
        .filter(|candidate| candidate.analysed)
        .filter(|candidate| rule.matches(candidate.scene, &candidate.interactions))
        .collect();
    outcome
        .candidates
        .insert(rule.id, u32::try_from(candidates.len()).unwrap_or(u32::MAX));
    if candidates.is_empty() {
        outcome.found.insert(rule.id, 0);
        note(
            outcome,
            rule.id,
            format!(
                "{}: no candidates found. AURA cannot add what was not photographed.",
                rule.id.title()
            ),
        );
        return Coverage::Missing;
    }

    order(&mut candidates, rule.prefer, scores);

    let mut have: u32 = candidates
        .iter()
        .filter(|candidate| kept.contains(&candidate.image_id))
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let mut weak = false;
    let mut forced_names: Vec<String> = Vec::new();

    for candidate in &candidates {
        if have >= rule.min {
            break;
        }
        if kept.contains(&candidate.image_id) {
            continue;
        }
        // A frame the photographer explicitly removed is never forced back in. The
        // rule degrades instead, with a warning that names the override - because
        // silently overruling the photographer and silently losing the vows are both
        // failures and only one of them is recoverable by telling them.
        if overrides.get(&candidate.image_id) == Some(&false) {
            continue;
        }
        let score = scores
            .get(&candidate.image_id)
            .map_or(0.0, |keep| keep.scene_weighted);
        let floor = floors.get(&candidate.image_id).copied().unwrap_or(0.0);
        if score < floor || candidate.veto.is_some() {
            weak = true;
            forced_names.push(describe_weak(candidate, score, floor));
        }
        kept.insert(candidate.image_id);
        outcome.added.insert(candidate.image_id, rule.id);
        have = have.saturating_add(1);
    }

    outcome.found.insert(rule.id, have);
    if have == 0 {
        outcome.warnings.push(format!(
            "{}: every photograph of this was removed by hand, so it is not in the \
             gallery.",
            rule.id.title()
        ));
        return Coverage::Missing;
    }
    if have < rule.min {
        weak = true;
        outcome.warnings.push(format!(
            "{}: {have} of the {} frames a gallery normally carries. There were no \
             others to add.",
            rule.id.title(),
            rule.min
        ));
    }
    if weak {
        if !forced_names.is_empty() {
            note(
                outcome,
                rule.id,
                format!(
                    "{}: kept anyway, because it is the only coverage there is - {}.",
                    rule.id.title(),
                    forced_names.join("; ")
                ),
            );
        }
        return Coverage::CoveredWeak;
    }
    Coverage::Covered
}

/// Put a rule's candidates into the order it prefers.
///
/// Every branch ends on the image id, so two frames that tie on the preferred key are
/// still ordered the same way on every machine.
fn order(candidates: &mut [&Candidate], prefer: Prefer, scores: &BTreeMap<ImageId, KeepScore>) {
    let score_of = |candidate: &Candidate| {
        scores
            .get(&candidate.image_id)
            .map_or(0.0, |keep| keep.scene_weighted)
    };
    match prefer {
        Prefer::Peak => candidates.sort_by(|left, right| {
            right
                .is_peak
                .cmp(&left.is_peak)
                .then_with(|| score_of(right).total_cmp(&score_of(left)))
                .then_with(|| left.image_id.cmp(&right.image_id))
        }),
        Prefer::Score => candidates.sort_by(|left, right| {
            score_of(right)
                .total_cmp(&score_of(left))
                .then_with(|| left.image_id.cmp(&right.image_id))
        }),
        Prefer::Earliest => candidates.sort_by(|left, right| {
            left.timeline_ts
                .cmp(&right.timeline_ts)
                .then_with(|| left.image_id.cmp(&right.image_id))
        }),
    }
}

/// The sentence a forced frame contributes to its rule's warning.
fn describe_weak(candidate: &Candidate, score: f32, floor: f32) -> String {
    match candidate.veto {
        Some(code) => format!("one frame where {}", code.user_text()),
        None => format!(
            "one frame scoring {score:.2}, below the {floor:.2} this kind of photograph \
             usually needs"
        ),
    }
}

/// Record one warning in both places it has to appear.
fn note(outcome: &mut GuardOutcome, rule: MustHave, text: String) {
    outcome
        .rule_warnings
        .entry(rule)
        .and_modify(|existing| {
            existing.push(' ');
            existing.push_str(&text);
        })
        .or_insert_with(|| text.clone());
    outcome.warnings.push(text);
}

/// Where a rule sits in the frozen order.
fn position_of(rule: MustHave) -> usize {
    MustHave::ALL
        .iter()
        .position(|candidate| *candidate == rule)
        .unwrap_or(MustHave::COUNT)
}
