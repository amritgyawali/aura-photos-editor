//! The pipeline, in the order section 3 draws it.
//!
//! ```text
//! fusion  ->  moment  ->  chapter  ->  COVERAGE  ->  diversity  ->  sizing  ->  COVERAGE
//! ```
//!
//! Seven steps, of which two are the same step. Section 6.4 is why: "the coverage guard
//! always runs last so shrinking never breaks must-haves." Running it once after the
//! chapter pass stops a quota starving a guarantee; running it again after sizing stops the
//! slider doing it. The guard only ever *adds*, so the second run over a gallery the first
//! run fixed does nothing, and idempotence is what makes running it twice affordable.
//!
//! ## This function is pure
//!
//! [`Engine::select`] takes a [`CullInput`] and returns a [`SelectionResult`]. There is no
//! database, no clock, no network and no filesystem inside it, every map it iterates is a
//! `BTreeMap`, and every ordering comparison ends in a tie-break on the photo id.
//!
//! That is the whole of section 10.1's "two runs on two machines with the same inputs
//! produce byte-identical selections". It is a unit test rather than a field report because
//! there is nothing else for two machines to differ about.
//!
//! ## Where the overrides go in, and why it is not at the start
//!
//! A photographer's keep or reject is applied **after** the chapter pass and **before** the
//! coverage guard. Not at the start, because a forced keep should count toward its
//! chapter's quota rather than sitting outside the accounting; and not at the end, because a
//! forced *reject* can leave a guarantee short and the guard has to be given the chance to
//! find another frame - or, if there is none, to report the rule as missing with a warning
//! that names the override.
//!
//! That last case is the one worth stating plainly: **a forced reject can degrade a
//! must-have to `Missing`.** The product does not silently overrule the photographer and it
//! does not silently lose the vows; it does what they asked and says what it cost.
//!
//! ## Every frame is explained, including the ones nobody looked at
//!
//! A frame with no phase 09 verdict is not rejected. It is `CullCode::NotAnalysed`, it
//! carries a reason saying nobody has checked it, and it is counted outside
//! `CullOutline::eligible`. "We have not checked this yet" and "this is not good enough"
//! must never look the same in a gallery, and this is the only place that distinction can
//! be made.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::contract::cull::ImageId;
use aura_core::{
    ChapterId, Coverage, CullCode, CullMode, CullReason, KeepScore, MustHave, Rejected, SceneId,
    Selected, SelectionResult,
};

use crate::chapter_pass::{self, ChapterPlan};
use crate::coverage::Guard;
use crate::diversity;
use crate::explain;
use crate::fusion::{self, Calibration};
use crate::input::{Candidate, CullInput};
use crate::modes;
use crate::moment_pass::{self, MomentOutcome};
use crate::rules::RuleTable;
use crate::sizing::{self, SizeFeatures};
use crate::weights::WeightTable;

/// What one run was asked for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Request {
    /// Which autonomy mode.
    pub mode: CullMode,
    /// How many frames to aim for. `None` asks the size model to predict one.
    ///
    /// The slider passes `Some`; the first cull of a project passes `None`. Both go through
    /// the same passes, which is what makes section 11's two-second slider budget the same
    /// code path as the first run rather than a second implementation of it.
    pub target: Option<u32>,
    /// The photographer's own decisions. `true` keeps, `false` rejects.
    pub overrides: BTreeMap<ImageId, bool>,
}

/// What one run did, for telemetry and the phase gate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PassReport {
    /// Photographs in the project.
    pub photos: usize,
    /// Photographs with a phase 09 verdict.
    pub eligible: usize,
    /// Photographs in the gallery.
    pub selected: usize,
    /// Eligible photographs that also had a phase 10 reading.
    pub emotion_aware: usize,
    /// ...a phase 11 judgement.
    pub composition_aware: usize,
    /// ...a moment.
    pub grouped: usize,
    /// How many frames each veto rejected, in `CullCode::VETOES` order.
    ///
    /// Section 11's `cull.veto {reason_code, count}` event.
    pub vetoes: [u32; 3],
    /// Improving swaps the chapter local search made.
    pub swaps: usize,
    /// Frames the coverage guard forced in.
    pub coverage_added: usize,
    /// Frames the diversity pass removed.
    pub diversity_dropped: usize,
    /// Frames sizing added, and removed.
    pub size_added: usize,
    /// ...and removed.
    pub size_trimmed: usize,
    /// Moment peaks that were rejected. Section 6.2 requires each to say so out loud.
    pub peaks_rejected: usize,
    /// Rules that ended `CoveredWeak`.
    pub coverage_weak: usize,
    /// Rules that ended `Missing`.
    pub coverage_missing: usize,
    /// Scenes with no weight row, by slug.
    pub unweighted_scenes: Vec<String>,
    /// How many candidates each guarantee had at all.
    ///
    /// The number behind `Missing`, and the number the coverage panel needs to tell "nobody
    /// shot it" from "the engine chose not to". Carried out of the guard rather than
    /// recomputed by the caller, because recomputing it would mean a second implementation
    /// of the match clause.
    pub rule_candidates: BTreeMap<MustHave, u32>,
    /// How many gallery frames each guarantee ended with.
    pub rule_found: BTreeMap<MustHave, u32>,
    /// The warning each guarantee carries, when it carries one.
    pub rule_warnings: BTreeMap<MustHave, String>,
    /// Milliseconds. Filled by the caller, because the engine has no clock.
    pub elapsed_ms: u64,
}

/// The culling engine.
///
/// Borrows both configuration tables rather than owning them: they are loaded once per
/// process and the engine is constructed per run.
#[derive(Debug)]
pub struct Engine<'a> {
    weights: &'a WeightTable,
    rules: &'a RuleTable,
    calibration: Calibration,
}

impl<'a> Engine<'a> {
    /// An engine over one pair of tables, with the shipped identity calibration.
    #[must_use]
    pub fn new(weights: &'a WeightTable, rules: &'a RuleTable) -> Self {
        Self {
            weights,
            rules,
            calibration: Calibration::identity(),
        }
    }

    /// The same engine with a fitted calibration.
    ///
    /// Nothing in this repository calls it yet, and it exists rather than being added later
    /// because the version it carries is already a stored column and already the subject of
    /// `AURA-ML-5048`. ADR-0025 section 3.
    #[must_use]
    pub fn with_calibration(mut self, calibration: Calibration) -> Self {
        self.calibration = calibration;
        self
    }

    /// The digest of the two tables this engine reads.
    #[must_use]
    pub fn config_digest(&self) -> String {
        explain::config_digest(self.weights, self.rules)
    }

    /// Choose a gallery.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn select(&self, mut input: CullInput, request: &Request) -> (SelectionResult, PassReport) {
        input.sort();
        let mut report = PassReport {
            photos: input.candidates.len(),
            unweighted_scenes: self.weights.unweighted(),
            ..PassReport::default()
        };

        // ---- 1. Fusion ---------------------------------------------------
        let mut scores: BTreeMap<ImageId, KeepScore> = BTreeMap::new();
        let mut floors: BTreeMap<ImageId, f32> = BTreeMap::new();
        let mut confidences: BTreeMap<ImageId, f32> = BTreeMap::new();
        let mut chapters: BTreeMap<ImageId, ChapterId> = BTreeMap::new();
        let mut volumes: BTreeMap<ChapterId, u32> = BTreeMap::new();
        for candidate in &input.candidates {
            let (weights, unweighted) = self.weights.for_scene(candidate.scene);
            let tuning = modes::tune_row(self.weights, request.mode, weights, unweighted);
            floors.insert(candidate.image_id, tuning.floor);
            confidences.insert(
                candidate.image_id,
                fusion::confidence(candidate, unweighted),
            );
            chapters.insert(candidate.image_id, candidate.chapter);
            scores.insert(
                candidate.image_id,
                fusion::fuse(candidate, weights, &self.calibration),
            );
            if candidate.analysed {
                report.eligible += 1;
                *volumes.entry(candidate.chapter).or_insert(0) += 1;
                if candidate.has_emotion {
                    report.emotion_aware += 1;
                }
                if candidate.has_composition {
                    report.composition_aware += 1;
                }
                if candidate.moment_id.is_some() {
                    report.grouped += 1;
                }
            }
            if let Some(code) = candidate.veto {
                if let Some(slot) = CullCode::VETOES
                    .iter()
                    .position(|veto| *veto == code)
                    .and_then(|index| report.vetoes.get_mut(index))
                {
                    *slot = slot.saturating_add(1);
                }
            }
        }

        // ---- 2. Moment pass ----------------------------------------------
        let outcomes = moment_pass::run(&input, &scores, self.weights, request.mode);

        // ---- 3. Chapter quotas -------------------------------------------
        let target = request.target.unwrap_or_else(|| {
            sizing::predict(
                self.features(&input, &scores, &volumes),
                modes::target_scale(self.weights, request.mode),
            )
        });
        let plan = chapter_pass::plan(target, &volumes, self.rules);
        let allocation = chapter_pass::allocate(&outcomes, &chapters, &scores, &floors, &plan);
        report.swaps = allocation.swaps;
        let mut kept: BTreeSet<ImageId> = allocation.kept.clone();

        // ---- 4. The photographer's own decisions -------------------------
        //
        // After the quota so a forced keep is counted, before the guard so a forced reject
        // can be repaired - or honestly reported as a gap. See this module's header.
        for (image, keep) in &request.overrides {
            if *keep {
                if scores.contains_key(image) {
                    kept.insert(*image);
                }
            } else {
                kept.remove(image);
            }
        }

        // ---- 5. Coverage, the first time ---------------------------------
        let guard = Guard::new(self.rules);
        let first = guard.enforce(&input, &scores, &floors, &request.overrides, &mut kept);
        report.coverage_added = first.added.len();

        // ---- 6. Diversity ------------------------------------------------
        let mut protected: BTreeSet<ImageId> = first.added.keys().copied().collect();
        for (image, keep) in &request.overrides {
            if *keep {
                protected.insert(*image);
            }
        }
        let spread = diversity::prune(
            &input,
            &scores,
            &protected,
            self.rules.diversity(),
            &mut kept,
        );
        report.diversity_dropped = spread.dropped.len();

        // ---- 7. Sizing ---------------------------------------------------
        // The pool excludes what the photographer removed by hand. Without that exclusion
        // the size reconciliation would quietly put a rejected frame back to hit a number,
        // which is the one thing in this phase that must never happen: an override is
        // unbeatable, and a gallery that re-added one would be lying about it in the panel.
        let pool: Vec<ImageId> = input
            .candidates
            .iter()
            .filter(|candidate| candidate.is_eligible())
            .filter(|candidate| request.overrides.get(&candidate.image_id) != Some(&false))
            .map(|candidate| candidate.image_id)
            .collect();
        let sized = sizing::reconcile(target, &pool, &scores, &floors, &protected, &mut kept);
        report.size_added = sized.added.len();
        report.size_trimmed = sized.trimmed.len();

        // ---- 8. Coverage, last ------------------------------------------
        let final_guard = guard.enforce(&input, &scores, &floors, &request.overrides, &mut kept);
        report.coverage_added += final_guard
            .added
            .len()
            .saturating_sub(first.added.len().min(final_guard.added.len()));

        // ---- 9. Explain --------------------------------------------------
        let mut coverage = Guard::report(&final_guard);
        coverage.chapter_counts = chapter_counts(&input, &kept, &plan);
        report.coverage_weak = final_guard
            .states
            .iter()
            .filter(|(_, state)| *state == Coverage::CoveredWeak)
            .count();
        report.coverage_missing = final_guard
            .states
            .iter()
            .filter(|(_, state)| *state == Coverage::Missing)
            .count();
        report.rule_candidates = final_guard.candidates.clone();
        report.rule_found = final_guard.found.clone();
        report.rule_warnings = final_guard.rule_warnings.clone();

        let context = Context {
            outcomes: &outcomes,
            added: &final_guard.added,
            spread: &spread.dropped,
            below_floor: &allocation.below_floor,
            chapter_full: &allocation.chapter_full,
            trimmed: &sized.trimmed,
            grown: &sized.added,
            overrides: &request.overrides,
            confidences: &confidences,
            found: &final_guard.found,
        };
        let (selected, rejected, peaks_rejected) =
            self.explain_all(&input, &scores, &kept, &context);
        report.peaks_rejected = peaks_rejected;
        report.selected = selected.len();

        let hash = explain::deterministic_hash(
            &input,
            &scores,
            &self.config_digest(),
            request.mode,
            target,
            crate::ANALYSIS_VER,
        );

        let result = SelectionResult {
            actual_count: u32::try_from(selected.len()).unwrap_or(u32::MAX),
            selected,
            rejected,
            coverage,
            target_count: target,
            mode: request.mode,
            deterministic_hash: hash,
            model_ver: input.model_ver,
            analysis_ver: crate::ANALYSIS_VER,
            calibration_ver: self.calibration.version(),
        };
        (result, report)
    }

    /// The size model's features for this wedding.
    #[allow(clippy::unused_self)]
    fn features(
        &self,
        input: &CullInput,
        scores: &BTreeMap<ImageId, KeepScore>,
        volumes: &BTreeMap<ChapterId, u32>,
    ) -> SizeFeatures {
        let values: Vec<f32> = input
            .candidates
            .iter()
            .filter(|candidate| candidate.is_eligible())
            .filter_map(|candidate| scores.get(&candidate.image_id))
            .map(|score| score.scene_weighted)
            .collect();
        let mean = if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f32>() / values.len() as f32
        };
        SizeFeatures {
            eligible: u32::try_from(input.eligible()).unwrap_or(u32::MAX),
            moments: u32::try_from(input.moments.len()).unwrap_or(u32::MAX),
            chapters: u32::try_from(volumes.len()).unwrap_or(u32::MAX),
            hours: input.hours,
            mean_score: mean,
            p75_score: sizing::upper_quartile(values),
        }
    }

    /// Turn the passes' bookkeeping into a reason for every photograph.
    #[allow(clippy::too_many_lines)]
    fn explain_all(
        &self,
        input: &CullInput,
        scores: &BTreeMap<ImageId, KeepScore>,
        kept: &BTreeSet<ImageId>,
        context: &Context<'_>,
    ) -> (Vec<Selected>, Vec<Rejected>, usize) {
        let mut selected = Vec::new();
        let mut rejected = Vec::new();
        let mut peaks_rejected = 0usize;

        // Which moment outcome each frame belongs to, so the reasons can name its rank and
        // its runner-up without a linear search per photograph.
        let mut membership: BTreeMap<ImageId, usize> = BTreeMap::new();
        for (index, outcome) in context.outcomes.iter().enumerate() {
            for (image, _) in &outcome.ranked {
                membership.insert(*image, index);
            }
        }

        for candidate in &input.candidates {
            let image = candidate.image_id;
            let score = scores.get(&image).map_or(0.0, |keep| keep.scene_weighted);
            let outcome = membership
                .get(&image)
                .and_then(|index| context.outcomes.get(*index));

            if kept.contains(&image) {
                let (reasons, role) = self.keep_reasons(candidate, outcome, context);
                selected.push(Selected {
                    image_id: image,
                    moment_id: candidate.moment_id,
                    keep_score: score,
                    confidence: context.confidences.get(&image).copied().unwrap_or(0.0),
                    reasons: explain::rank(reasons, Selected::MAX_REASONS),
                    // The best frame of the same moment that is *not* in the finished
                    // gallery. Computed here rather than taken from the moment pass,
                    // because four passes have run since then and the frame that pass
                    // called the runner-up may itself have been delivered - in which case
                    // there is no alternative to offer and the honest answer is `None`.
                    runner_up: outcome.and_then(|outcome| {
                        outcome
                            .ranked
                            .iter()
                            .map(|(candidate, _)| *candidate)
                            .find(|candidate| *candidate != image && !kept.contains(candidate))
                    }),
                    coverage_role: role,
                });
                continue;
            }

            let (reasons, kept_instead, was_peak) =
                self.reject_reasons(candidate, outcome, kept, context);
            if was_peak {
                peaks_rejected += 1;
            }
            rejected.push(Rejected {
                image_id: image,
                moment_id: candidate.moment_id,
                keep_score: if candidate.veto.is_some() { 0.0 } else { score },
                reasons: explain::rank(reasons, Rejected::MAX_REASONS),
                kept_instead,
            });
        }

        (selected, rejected, peaks_rejected)
    }

    fn keep_reasons(
        &self,
        candidate: &Candidate,
        outcome: Option<&MomentOutcome>,
        context: &Context<'_>,
    ) -> (Vec<CullReason>, Option<MustHave>) {
        let image = candidate.image_id;
        let mut reasons = Vec::new();
        let mut role = None;

        if context.overrides.get(&image) == Some(&true) {
            reasons.push(CullReason::plain(CullCode::UserKept, 1.0));
        }
        if let Some(rule) = context.added.get(&image) {
            role = Some(*rule);
            let code = if rule.is_identity_rule() {
                CullCode::IdentityCoverage
            } else {
                CullCode::CoverageProtected
            };
            let text = if rule.is_identity_rule() {
                explain::identity_text(
                    context.found.get(rule).copied().unwrap_or(0),
                    self.rules.identity().close_family_min,
                )
            } else {
                explain::coverage_text(*rule, context.found.get(rule).copied().unwrap_or(0))
            };
            reasons.push(CullReason::spoken(code, text, 0.95));
        }
        if let Some(outcome) = outcome {
            let rank = outcome.winners.iter().position(|winner| *winner == image);
            match rank {
                Some(0) if outcome.is_sole() => {
                    reasons.push(CullReason::plain(CullCode::OnlyCandidate, 0.60));
                }
                Some(0) => reasons.push(CullReason::plain(CullCode::MomentWinner, 0.80)),
                Some(_) => reasons.push(CullReason::plain(CullCode::DiversitySpread, 0.55)),
                None => {}
            }
            if outcome.peak == Some(image) {
                reasons.push(CullReason::plain(CullCode::PeakFrame, 0.70));
            }
        }
        if context.grown.contains(&image) {
            reasons.push(CullReason::plain(CullCode::SizeTarget, 0.35));
        }
        if reasons.is_empty() {
            // Reached only by a frame the chapter pass admitted that is neither a first
            // keeper nor a promotion - a second keeper whose moment outcome was lost. It is
            // still in the gallery because it filled a chapter's target, and saying so is
            // better than saying nothing. Invariant 2 has no exception for the awkward case.
            reasons.push(CullReason::plain(CullCode::ChapterQuota, 0.30));
        }
        if candidate.scene == SceneId::Unknown {
            reasons.push(CullReason::plain(CullCode::NoScene, 0.05));
        }
        if candidate.moment_id.is_none() {
            reasons.push(CullReason::plain(CullCode::NoMoment, 0.05));
        }
        (reasons, role)
    }

    /// Why a photograph is not in the gallery.
    ///
    /// Takes `&self` and does not read it, unlike its sibling: the keep path consults the
    /// rule table for an identity minimum and this one does not. Kept as a method anyway so
    /// the two read as a pair at the call site.
    #[allow(clippy::unused_self)]
    fn reject_reasons(
        &self,
        candidate: &Candidate,
        outcome: Option<&MomentOutcome>,
        kept: &BTreeSet<ImageId>,
        context: &Context<'_>,
    ) -> (Vec<CullReason>, Option<ImageId>, bool) {
        let image = candidate.image_id;
        let mut reasons = Vec::new();
        let mut kept_instead = None;

        if !candidate.analysed {
            // Never a rejection. See this module's header.
            reasons.push(CullReason::plain(CullCode::NotAnalysed, -0.10));
            return (reasons, None, false);
        }
        if context.overrides.get(&image) == Some(&false) {
            reasons.push(CullReason::plain(CullCode::UserRejected, -1.0));
        }
        if let Some(code) = candidate.veto {
            reasons.push(CullReason::plain(code, -1.0));
        }
        if let Some(winner) = outcome.and_then(|outcome| outcome.suppressed.get(&image)) {
            kept_instead = Some(*winner);
            reasons.push(CullReason::spoken(
                CullCode::NearDuplicate,
                explain::duplicate_text(),
                -0.75,
            ));
        }
        if let Some(winner) = context.spread.get(&image) {
            kept_instead = kept_instead.or(Some(*winner));
            reasons.push(CullReason::plain(CullCode::DiversityCap, -0.55));
        }
        if context.below_floor.contains(&image) {
            reasons.push(CullReason::plain(CullCode::BelowFloor, -0.60));
        }
        if context.chapter_full.contains(&image) {
            reasons.push(CullReason::plain(CullCode::ChapterFull, -0.50));
        }
        if context.trimmed.contains(&image) {
            reasons.push(CullReason::plain(CullCode::SizeTrim, -0.40));
        }

        let mut was_peak = false;
        if let Some(outcome) = outcome {
            if outcome.peak == Some(image) {
                was_peak = true;
                reasons.push(CullReason::plain(CullCode::PeakRejected, -0.30));
            }
            if outcome.runner_up == Some(image) {
                reasons.push(CullReason::plain(CullCode::RunnerUp, -0.05));
            }
            if kept_instead.is_none() {
                kept_instead = outcome
                    .winners
                    .iter()
                    .copied()
                    .find(|winner| kept.contains(winner));
            }
            if reasons.is_empty() {
                let rank = outcome
                    .ranked
                    .iter()
                    .position(|(candidate, _)| *candidate == image)
                    .unwrap_or(0);
                reasons.push(CullReason::spoken(
                    CullCode::LostMomentRank,
                    explain::lost_text(rank, outcome.ranked.len()),
                    -0.45,
                ));
            }
        }
        if reasons.is_empty() {
            // A frame with no moment that nothing else explained. It lost on score alone,
            // which is what `BelowFloor` says, and saying it is better than an empty
            // reason list - which invariant 2 and the database's own CHECK both forbid.
            reasons.push(CullReason::plain(CullCode::BelowFloor, -0.45));
        }
        (reasons, kept_instead, was_peak)
    }
}

/// Everything the reason builders need, gathered once.
///
/// A struct rather than eleven arguments, because `clippy::too_many_arguments` is right
/// about eleven arguments and because the alternative - rebuilding these maps per
/// photograph - is four thousand allocations on the hot path.
struct Context<'a> {
    outcomes: &'a [MomentOutcome],
    added: &'a BTreeMap<ImageId, MustHave>,
    spread: &'a BTreeMap<ImageId, ImageId>,
    below_floor: &'a BTreeSet<ImageId>,
    chapter_full: &'a BTreeSet<ImageId>,
    trimmed: &'a [ImageId],
    grown: &'a [ImageId],
    overrides: &'a BTreeMap<ImageId, bool>,
    confidences: &'a BTreeMap<ImageId, f32>,
    found: &'a BTreeMap<MustHave, u32>,
}

/// Delivered and targeted counts per chapter, in `ChapterId::ALL` order.
fn chapter_counts(
    input: &CullInput,
    kept: &BTreeSet<ImageId>,
    plan: &ChapterPlan,
) -> Vec<(ChapterId, u32, u32)> {
    let mut delivered: BTreeMap<ChapterId, u32> = BTreeMap::new();
    for candidate in &input.candidates {
        if kept.contains(&candidate.image_id) {
            *delivered.entry(candidate.chapter).or_insert(0) += 1;
        }
    }
    ChapterId::ALL
        .into_iter()
        .map(|chapter| {
            (
                chapter,
                delivered.get(&chapter).copied().unwrap_or(0),
                plan.target(chapter),
            )
        })
        .collect()
}
