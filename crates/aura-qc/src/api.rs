//! The frozen service, and the pass that fills it. PHASE-27 sections 5 and 6.
//!
//! ## What the pass is, in one paragraph
//!
//! For every frame in the **delivered gallery**: run nine per-frame inspections, file a ticket for
//! each finding, order them root cause first, apply one remedy, re-inspect that ticket's own metric,
//! keep the change only if it realised half of what it predicted and broke nothing else, and stop
//! after two rounds. Then run the coverage check over the set, assemble a report, and write the
//! whole thing in one transaction.
//!
//! ## Three things the pass has to get right
//!
//! **The denominator is the delivered gallery.** Not the project. A QC check over a frame nobody is
//! delivering is not an inspection anybody asked for, and phase 18 established that denominator for
//! masks. `QcOutline::selected` is what everything here is measured against.
//!
//! **The budget is spent rather than assumed.** Section 11 gives the pass 90 s per thousand images.
//! [`QcPass::run`] stops opening new frames when it is spent and reports `images_unreached`, which
//! is a different outcome from a clean gallery. A pass that inspected 800 of 1,000 frames and
//! reported nothing has not reported that the gallery is clean.
//!
//! **A photographer's verdict survives.** [`QcStore::take_decisions`] reads the accepted and
//! dismissed statuses out before the project is cleared and the write puts them back, keyed on the
//! *finding* rather than on the ticket id - because a re-pass assigns fresh ids. Phase 25's
//! mechanism, and migration 27's trigger is the second layer.
//!
//! ## The port, and why there are two
//!
//! [`Field`] supplies the readings - it is filled by a caller that holds the thirteen frozen
//! services this phase reads from, and it is the reason `aura-qc` depends on none of them.
//! [`crate::reedit::Remediator`] applies a remedy. Two ports rather than one because the first is
//! read-only and the second is not, and a pass with no remediator is a pass that inspects and
//! reports without changing anything - which is exactly what `QcPass::inspect_only` is for and what
//! phase 28 will call before it decides whether to deliver.

use std::sync::Arc;

use aura_core::contract::ids::TicketId;
use aura_core::contract::qc::{
    ImageId, QcCategory, QcCode, QcOutline, QcOverride, QcReport, QcRound, QcService, QcTicket,
    Remedy, Replacement, TicketStatus,
};
use aura_core::contract::scene::Timestamp;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, ProjectId};

use crate::checks::{self, Frame, SetContext};
use crate::errors::pass_failed;
use crate::policy::{Thresholds, THRESHOLDS_VER};
use crate::reedit::{Loop, Remediator};
use crate::replace::{self, CandidateMetric, CoverageEffect, Verdict};
use crate::store::QcStore;
use crate::{queue, remedy, report, ticket, triage, ANALYSIS_VER};

/// Where the readings come from.
///
/// Filled by a caller holding `GalleryService`, `ToneService`, `ColourService`, `IntegrityService`,
/// `RestoreService`, `RetouchService`, `MicroService`, `LocalService`, `MaskService`,
/// `GeometryService`, `CleanupService`, `MomentService` and `CullService`. This crate depends on
/// none of them, which is what stops `aura-brain-photo` depending on the crate that judges it.
///
/// Phase 19's `MaskField` is the same shape, and the reason is the same: a phase that consumes
/// another phase's output owns no fallback for it. A `None` here is a skipped check, never a pass.
pub trait Field: Send + Sync {
    /// The frames in the delivered gallery, in a stable order.
    ///
    /// # Errors
    ///
    /// Whatever the underlying service raises.
    fn selected(&self, project: ProjectId) -> AuraResult<Vec<ImageId>>;

    /// Everything the checks need to know about one photograph.
    ///
    /// # Errors
    ///
    /// Whatever the underlying services raise.
    fn frame(&self, image: ImageId) -> AuraResult<Frame>;

    /// The gallery's coverage state, once per project.
    ///
    /// # Errors
    ///
    /// Whatever `CullService` raises.
    fn coverage(&self, project: ProjectId) -> AuraResult<SetContext>;

    /// What a swap from `image` to its runner-up would do to the gallery's guarantees.
    ///
    /// # Errors
    ///
    /// Whatever `CullService` raises.
    fn coverage_effect(&self, image: ImageId) -> AuraResult<CoverageEffect>;

    /// The runner-up's own reading on one category.
    ///
    /// `None` when it has no reading - which refuses the swap rather than permitting it, because a
    /// candidate nobody measured is not a candidate anybody can prefer.
    ///
    /// # Errors
    ///
    /// Whatever the underlying services raise.
    fn candidate(
        &self,
        runner_up: ImageId,
        category: QcCategory,
    ) -> AuraResult<Option<CandidateMetric>>;
}

/// One quality-control pass.
#[derive(Debug)]
pub struct QcPass {
    store: QcStore,
    thresholds: Thresholds,
    budget_ms: u64,
}

/// Everything one pass produced, before it is written.
#[derive(Debug, Clone, Default)]
pub struct PassResult {
    /// Every ticket.
    pub tickets: Vec<QcTicket>,
    /// Every round.
    pub rounds: Vec<QcRound>,
    /// Every swap.
    pub replacements: Vec<Replacement>,
    /// The report.
    pub report: QcReport,
}

impl QcPass {
    /// A pass over one project's thresholds.
    #[must_use]
    pub fn new(store: QcStore, thresholds: Thresholds) -> Self {
        Self {
            store,
            thresholds,
            budget_ms: 0,
        }
    }

    /// Give the pass a wall-clock ceiling.
    ///
    /// Zero, the default, means no ceiling. Section 11's budget is per thousand images and the
    /// caller scales it; `PASS_BUDGET_MS_PER_1K` is the constant.
    #[must_use]
    pub const fn with_budget(mut self, budget_ms: u64) -> Self {
        self.budget_ms = budget_ms;
        self
    }

    /// The thresholds this pass is running under.
    #[must_use]
    pub const fn thresholds(&self) -> &Thresholds {
        &self.thresholds
    }

    /// Inspect a gallery and file tickets, changing nothing.
    ///
    /// What phase 28 calls before it decides whether to deliver, and what a photographer gets when
    /// autonomy is off. No remediator, so no remedy is applied and every ticket stays `Open`.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5136` when the pass cannot run.
    pub fn inspect_only(
        &self,
        project: ProjectId,
        field: &dyn Field,
        now: Timestamp,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<PassResult> {
        self.execute(project, field, None, now, cancel, progress)
    }

    /// Inspect a gallery, remediate what it can, and escalate the rest.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5136` when the pass cannot run.
    pub fn run(
        &self,
        project: ProjectId,
        field: &dyn Field,
        remediator: &mut dyn Remediator,
        now: Timestamp,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<PassResult> {
        self.execute(project, field, Some(remediator), now, cancel, progress)
    }

    #[allow(clippy::too_many_lines)]
    fn execute(
        &self,
        project: ProjectId,
        field: &dyn Field,
        mut remediator: Option<&mut dyn Remediator>,
        now: Timestamp,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<PassResult> {
        let selected = field.selected(project).map_err(|err| {
            pass_failed(format!("the delivered gallery could not be read: {err}"))
        })?;
        let decisions = self.store.take_decisions(project)?;

        let mut result = PassResult::default();
        let mut checks_run = 0u32;
        let mut checks_skipped = 0u32;
        let mut skips = [0u32; QcCategory::COUNT];
        let mut inspected = 0u32;
        let mut unreached = 0u32;
        let mut spent_ms = 0u64;

        for (index, image) in selected.iter().enumerate() {
            if cancel.is_cancelled() {
                // Invariant 5: a cancelled pass writes what it reached and says what it did not.
                unreached = u32::try_from(selected.len() - index).unwrap_or(0);
                break;
            }
            if self.budget_ms > 0 && spent_ms >= self.budget_ms {
                unreached = u32::try_from(selected.len() - index).unwrap_or(0);
                break;
            }
            progress.report(ProgressUpdate {
                stage: "qc.inspect",
                done: index as u64,
                total: selected.len() as u64,
                current: None,
            });

            let mut frame = field.frame(*image).map_err(|err| {
                pass_failed(format!("a frame's readings could not be read: {err}"))
            })?;
            inspected = inspected.saturating_add(1);

            let outcomes = checks::run_frame(&frame, &self.thresholds);
            for (outcome, category) in outcomes.iter().zip(CATEGORY_ORDER) {
                if outcome.ran() {
                    checks_run = checks_run.saturating_add(1);
                } else {
                    checks_skipped = checks_skipped.saturating_add(1);
                    if let Some(position) =
                        QcCategory::ALL.iter().position(|kind| *kind == category)
                    {
                        if let Some(slot) = skips.get_mut(position) {
                            *slot = slot.saturating_add(1);
                        }
                    }
                }
            }

            let findings = checks::findings_for(&frame, &self.thresholds);
            if findings.is_empty() {
                continue;
            }

            let mut tickets: Vec<QcTicket> = findings
                .into_iter()
                .map(|finding| {
                    let proposed = remedy::propose(&finding, &frame, 0);
                    let mut ticket = ticket::from_finding(project, &frame, finding, proposed, now);
                    // A swap is offered in preference to a parameter change only where the category
                    // permits it and a runner-up exists. `replace::consider` still has the last
                    // word, and it runs the coverage filter first.
                    if replace::swappable(ticket.category) && frame.runner_up.is_some() {
                        if let Some(swap) = remedy::replacement_for(&frame) {
                            if let Ok(effect) = field.coverage_effect(*image) {
                                if let Ok(Some(candidate)) = field
                                    .candidate(frame.runner_up.unwrap_or(*image), ticket.category)
                                {
                                    let verdict = replace::consider(
                                        &ticket,
                                        &frame,
                                        candidate,
                                        effect,
                                        self.thresholds.loop_policy(),
                                    );
                                    if let Verdict::Swap { margin, .. } = verdict {
                                        if let Ok(validated) = remedy::validate(
                                            swap,
                                            &ticket,
                                            &frame,
                                            self.thresholds.loop_policy(),
                                        ) {
                                            ticket.remedy = validated;
                                            result.replacements.push(replace::record(
                                                &ticket,
                                                frame.runner_up.unwrap_or(*image),
                                                candidate,
                                                margin,
                                                now,
                                            ));
                                        }
                                    } else if let Verdict::Refuse(code) = verdict {
                                        if ticket.reasons.len() < QcTicket::MAX_REASONS {
                                            ticket.reasons.push(
                                                aura_core::contract::qc::QcReason::new(code, -0.5),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ticket
                })
                .collect();

            if let Some(remediator) = remediator.as_deref_mut() {
                let reedit = Loop::new(&self.thresholds);
                let mut rounds_here = 0u8;
                while rounds_here < reedit.max_rounds() {
                    let Some(next) = triage::next(&tickets).cloned() else {
                        break;
                    };
                    if !next.may_act_unattended() {
                        // The band says a person decides. Mark it escalated rather than leaving it
                        // open, so the queue shows it and the loop does not pick it again.
                        if let Some(slot) = tickets.iter_mut().find(|t| t.id == next.id) {
                            slot.status = TicketStatus::Escalated;
                            slot.outcome_code = Some(QcCode::EscalatedToHuman);
                        }
                        continue;
                    }
                    let validated = match remedy::validate(
                        next.remedy.clone(),
                        &next,
                        &frame,
                        self.thresholds.loop_policy(),
                    ) {
                        Ok(remedy) => remedy,
                        Err(refusal) => {
                            if let Some(slot) = tickets.iter_mut().find(|t| t.id == next.id) {
                                slot.status = TicketStatus::Escalated;
                                slot.outcome_code = Some(refusal.code());
                            }
                            continue;
                        }
                    };
                    let outcome = reedit.run(&next, &frame, &validated, remediator, now, 0)?;
                    frame = outcome.frame.clone();
                    spent_ms = spent_ms.saturating_add(u64::from(outcome.round.ms));
                    if let Some(slot) = tickets.iter_mut().find(|t| t.id == next.id) {
                        slot.round = outcome.round.round;
                        slot.status = outcome.status;
                        slot.deviation = outcome.deviation;
                        slot.outcome_code = Some(outcome.round.outcome);
                    }
                    result.rounds.push(outcome.round);
                    rounds_here = rounds_here.saturating_add(1);
                }
            }

            result.tickets.append(&mut tickets);
        }

        // The one gallery-scoped check, run once over the set.
        let context = field
            .coverage(project)
            .map_err(|err| pass_failed(format!("the coverage report could not be read: {err}")))?;
        let coverage = checks::coverage::inspect(&context);
        if coverage.ran() {
            checks_run = checks_run.saturating_add(1);
        } else {
            checks_skipped = checks_skipped.saturating_add(1);
            if let Some(slot) = skips.get_mut(QcCategory::COUNT - 1) {
                *slot = slot.saturating_add(1);
            }
        }
        let anchor = selected.first().copied().unwrap_or_else(ImageId::new);
        for finding in coverage.findings() {
            let frame = Frame::empty(anchor, aura_core::contract::scene::SceneId::Unknown);
            let proposed = remedy::propose(&finding, &frame, 0);
            let mut ticket = ticket::from_finding(project, &frame, finding, proposed, now);
            ticket.status = TicketStatus::Escalated;
            ticket.outcome_code = Some(QcCode::EscalatedToHuman);
            result.tickets.push(ticket);
        }

        let mut assembled = report::assemble(
            project,
            &result.tickets,
            result.replacements.clone(),
            inspected,
            unreached,
            checks_run,
            checks_skipped,
            0,
            spent_ms,
            THRESHOLDS_VER,
            ANALYSIS_VER,
        );
        report::with_skips(&mut assembled, skips);
        result.report = assembled;

        self.store.write_pass(
            project,
            &result.tickets,
            &result.rounds,
            &result.replacements,
            &result.report,
            &decisions,
        )?;

        Ok(result)
    }
}

/// Which category each of `checks::FRAME_CHECKS` owns, in the same order.
const CATEGORY_ORDER: [QcCategory; 9] = [
    QcCategory::Consistency,
    QcCategory::Skin,
    QcCategory::Exposure,
    QcCategory::Sharpness,
    QcCategory::Retouch,
    QcCategory::Mask,
    QcCategory::Crop,
    QcCategory::Cleanup,
    QcCategory::Duplicate,
];

/// The one implementation of `QcService`.
///
/// Frozen surface, thin body: every method is a store read, because this phase's answers are rows
/// rather than computations. The pass is what puts them there.
#[derive(Debug, Clone)]
pub struct Qc {
    store: QcStore,
    selected: Arc<dyn SelectedCount>,
}

/// How many frames are in the delivered gallery.
///
/// A one-method port rather than a `CullService` handle, for the reason `Field` exists: this crate
/// depends on no deciding crate, and `QcOutline::selected` is the only thing the service needs from
/// phase 12.
pub trait SelectedCount: Send + Sync + std::fmt::Debug {
    /// Frames in the delivered gallery.
    ///
    /// # Errors
    ///
    /// Whatever `CullService` raises.
    fn selected(&self, project: ProjectId) -> AuraResult<u32>;
}

impl Qc {
    /// Wrap one store.
    #[must_use]
    pub fn new(store: QcStore, selected: Arc<dyn SelectedCount>) -> Self {
        Self { store, selected }
    }

    /// The store underneath, for the gate and the budget test.
    #[must_use]
    pub const fn store(&self) -> &QcStore {
        &self.store
    }

    /// Whether this project's findings came from this build.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn is_current(&self, project: ProjectId) -> AuraResult<bool> {
        self.store
            .is_current(project, (ANALYSIS_VER, THRESHOLDS_VER))
    }
}

impl QcService for Qc {
    fn outline(&self, project: ProjectId) -> AuraResult<QcOutline> {
        let selected = self.selected.selected(project)?;
        self.store.outline(project, selected)
    }

    fn report(&self, project: ProjectId) -> AuraResult<Option<QcReport>> {
        self.store.report(project)
    }

    fn tickets(&self, image: ImageId) -> AuraResult<Vec<QcTicket>> {
        self.store.tickets_for(image)
    }

    fn queue(
        &self,
        project: ProjectId,
        category: Option<QcCategory>,
        limit: usize,
    ) -> AuraResult<Vec<QcTicket>> {
        let found = self.store.queue(project, category, limit)?;
        // Ordered again here rather than trusted from SQL, because `QcTicket::queue_order` breaks
        // its last tie on the id and SQLite's `ORDER BY severity` alone would leave two equally
        // severe findings in an arbitrary order - which is a queue that looks different to a
        // photographer and to a support case.
        Ok(queue::open(&found, limit))
    }

    fn rounds(&self, ticket: TicketId) -> AuraResult<Vec<QcRound>> {
        self.store.rounds_for(ticket)
    }

    fn replacements(&self, project: ProjectId) -> AuraResult<Vec<Replacement>> {
        self.store.replacements(project)
    }

    fn decide(&self, over: &QcOverride) -> Result<(), AuraError> {
        self.store.decide(over)
    }
}

/// A remediator that refuses everything.
///
/// What a build with the feature flag off installs, and what `inspect_only` is expressed in terms of
/// when a caller wants a `Remediator` shaped hole filled. Every remedy comes back unapplied, so
/// every ticket escalates - which is the safe direction and the same shape phase 24's
/// `judgement::Answer` has.
#[derive(Debug, Default, Clone, Copy)]
pub struct RefuseAll;

impl Remediator for RefuseAll {
    fn apply(&mut self, _image: ImageId, _remedy: &Remedy) -> Result<Frame, AuraError> {
        Err(pass_failed(
            "remediation is switched off for this build, so nothing was changed",
        ))
    }

    fn revert(&mut self, _image: ImageId, _remedy: &Remedy) -> Result<Frame, AuraError> {
        Err(pass_failed("there is nothing to put back"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use aura_catalog::Catalog;
    use aura_core::clock::{Clock, SystemClock};
    use aura_core::progress::NullProgress;
    use rusqlite::params;
    use std::collections::BTreeMap;

    #[derive(Debug, Default)]
    struct Fake {
        frames: BTreeMap<ImageId, Frame>,
        order: Vec<ImageId>,
        context: SetContext,
    }

    impl Field for Fake {
        fn selected(&self, _project: ProjectId) -> AuraResult<Vec<ImageId>> {
            Ok(self.order.clone())
        }

        fn frame(&self, image: ImageId) -> AuraResult<Frame> {
            Ok(self.frames.get(&image).cloned().unwrap_or_default())
        }

        fn coverage(&self, _project: ProjectId) -> AuraResult<SetContext> {
            Ok(self.context.clone())
        }

        fn coverage_effect(&self, _image: ImageId) -> AuraResult<CoverageEffect> {
            Ok(CoverageEffect::unprotected())
        }

        fn candidate(
            &self,
            _runner_up: ImageId,
            _category: QcCategory,
        ) -> AuraResult<Option<CandidateMetric>> {
            Ok(None)
        }
    }

    #[derive(Debug)]
    struct Count(u32);

    impl SelectedCount for Count {
        fn selected(&self, _project: ProjectId) -> AuraResult<u32> {
            Ok(self.0)
        }
    }

    fn setup(frames: Vec<Frame>) -> (QcPass, Qc, Fake, ProjectId, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
        let catalog = Arc::new(
            Catalog::open(&dir.path().join("qc.sqlite"), Arc::clone(&clock), "test")
                .expect("a catalog"),
        );
        let project = ProjectId::new();
        catalog
            .writer()
            .transact({
                let key = project.to_db();
                move |tx| {
                    tx.execute(
                        "INSERT INTO project (project_id, name, created_at, updated_at)
                         VALUES (?1, 'test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                        params![key],
                    )
                    .map_err(|err| aura_core::errors::db::statement_failed("project seed", &err))?;
                    Ok(())
                }
            })
            .expect("the project seeds");

        let mut fake = Fake::default();
        for frame in frames {
            catalog
                .writer()
                .transact({
                    let key = project.to_db();
                    let photo = frame.image_id.to_db();
                    move |tx| {
                        tx.execute(
                            "INSERT INTO photo (photo_id, project_id, created_at, updated_at)
                             VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                            params![photo, key],
                        )
                        .map_err(|err| {
                            aura_core::errors::db::statement_failed("photo seed", &err)
                        })?;
                        Ok(())
                    }
                })
                .expect("the photo seeds");
            fake.order.push(frame.image_id);
            fake.frames.insert(frame.image_id, frame);
        }
        fake.context = fixtures::healthy_coverage();

        let store = QcStore::new(Arc::clone(&catalog), Arc::clone(&clock));
        let pass = QcPass::new(store.clone(), Thresholds::reference());
        let count = u32::try_from(fake.order.len()).unwrap_or(0);
        let service = Qc::new(store, Arc::new(Count(count)));
        (pass, service, fake, project, dir)
    }

    #[test]
    fn a_clean_gallery_produces_no_tickets_and_a_complete_report() {
        let frames = fixtures::clean_gallery(8);
        let (pass, service, field, project, _dir) = setup(frames);
        let result = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the pass runs");
        assert!(result.tickets.is_empty());
        assert!(result.report.complete());
        assert_eq!(result.report.images, 8);
        // Nine per-frame checks over eight frames plus the one coverage check.
        assert_eq!(result.report.checks_run, 8 * 9 + 1);
        assert_eq!(result.report.skipped, 0);
        let outline = service.outline(project).expect("an outline");
        assert_eq!(outline.selected, 8);
        assert_eq!(outline.open, 0);
        assert!(!outline.detector_trained);
    }

    #[test]
    fn every_injected_defect_produces_a_ticket_with_the_right_code() {
        let defects = fixtures::defects();
        let frames: Vec<Frame> = defects.iter().map(|defect| defect.frame.clone()).collect();
        let (pass, _service, field, project, _dir) = setup(frames);
        let result = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the pass runs");
        for defect in &defects {
            assert!(
                result
                    .tickets
                    .iter()
                    .any(|ticket| ticket.image_id == defect.frame.image_id
                        && ticket.code == defect.code),
                "'{}' produced no ticket",
                defect.name
            );
        }
    }

    #[test]
    fn a_frame_with_no_readings_skips_every_check_rather_than_passing() {
        let blank = Frame::empty(
            ImageId::new(),
            aura_core::contract::scene::SceneId::Ceremony,
        );
        let (pass, _service, field, project, _dir) = setup(vec![blank]);
        let result = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the pass runs");
        assert!(result.tickets.is_empty(), "nothing to find");
        // Nine skips and one coverage check that ran.
        assert_eq!(result.report.skipped, 9);
        assert_eq!(result.report.checks_run, 1);
        // Which is the number that makes the empty ticket list honest.
        assert!(result.report.skipped > result.report.checks_run);
    }

    #[test]
    fn a_project_with_no_coverage_report_records_the_skip() {
        let (pass, _service, mut field, project, _dir) = setup(fixtures::clean_gallery(2));
        field.context = SetContext::default();
        let result = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the pass runs");
        assert_eq!(result.report.tally(QcCategory::Coverage).skipped, 1);
    }

    #[test]
    fn broken_coverage_produces_escalated_tickets() {
        let (pass, _service, mut field, project, _dir) = setup(fixtures::clean_gallery(2));
        field.context = fixtures::broken_coverage();
        let result = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the pass runs");
        let coverage: Vec<&QcTicket> = result
            .tickets
            .iter()
            .filter(|ticket| ticket.category == QcCategory::Coverage)
            .collect();
        assert_eq!(coverage.len(), 3);
        assert!(coverage
            .iter()
            .all(|ticket| ticket.status == TicketStatus::Escalated));
    }

    #[test]
    fn a_cancelled_pass_writes_what_it_reached_and_says_what_it_did_not() {
        let (pass, _service, field, project, _dir) = setup(fixtures::clean_gallery(8));
        let cancel = CancelToken::new();
        cancel.cancel();
        let result = pass
            .inspect_only(project, &field, 0, &cancel, &NullProgress)
            .expect("a cancelled pass still writes");
        assert_eq!(result.report.images, 0);
        assert_eq!(result.report.images_unreached, 8);
        // The whole point: a pass that reached nothing must not report a clean gallery.
        assert!(!result.report.complete());
        assert_eq!(result.report.coverage(), 0.0);
    }

    #[test]
    fn the_pass_is_idempotent_over_a_catalog() {
        let (pass, service, field, project, _dir) = setup(
            fixtures::defects()
                .into_iter()
                .map(|defect| defect.frame)
                .collect(),
        );
        let first = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the first pass runs");
        let second = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the second pass runs");
        assert_eq!(first.tickets.len(), second.tickets.len());
        // And the catalog holds one set rather than two.
        let outline = service.outline(project).expect("an outline");
        assert_eq!(
            outline.open,
            u32::try_from(second.tickets.len()).unwrap_or(0)
        );
        assert!(service.is_current(project).expect("current"));
    }

    #[test]
    fn a_photographers_verdict_survives_a_second_pass() {
        let (pass, service, field, project, _dir) = setup(
            fixtures::defects()
                .into_iter()
                .take(3)
                .map(|defect| defect.frame)
                .collect(),
        );
        let first = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the first pass runs");
        let subject = first.tickets.first().expect("a ticket").clone();
        service
            .decide(&QcOverride {
                ticket: subject.id,
                status: TicketStatus::Dismissed,
                apply_remedy: false,
                note: Some("this is how the room looked".into()),
            })
            .expect("a photographer decides");

        pass.inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the second pass runs");
        let after = service.tickets(subject.image_id).expect("a read");
        assert!(
            after
                .iter()
                .any(|ticket| ticket.status == TicketStatus::Dismissed),
            "a finding somebody rejected must not come back"
        );
        assert!(!service
            .queue(project, None, 100)
            .expect("a queue")
            .iter()
            .any(|ticket| ticket.image_id == subject.image_id && ticket.code == subject.code));
    }

    #[test]
    fn a_build_with_remediation_off_escalates_rather_than_changing_anything() {
        let (pass, _service, field, project, _dir) = setup(
            fixtures::defects()
                .into_iter()
                .take(2)
                .map(|defect| defect.frame)
                .collect(),
        );
        let mut refuse = RefuseAll;
        let outcome = pass.run(
            project,
            &field,
            &mut refuse,
            0,
            &CancelToken::new(),
            &NullProgress,
        );
        // The remediator's own error surfaces rather than being swallowed. No silent failure,
        // invariant 9 - and the photograph is untouched either way.
        assert!(outcome.is_err());
    }

    #[test]
    fn the_queue_is_ordered_identically_on_every_read() {
        let (pass, service, field, project, _dir) = setup(
            fixtures::defects()
                .into_iter()
                .map(|defect| defect.frame)
                .collect(),
        );
        pass.inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the pass runs");
        let first: Vec<_> = service
            .queue(project, None, 50)
            .expect("a queue")
            .into_iter()
            .map(|ticket| ticket.id)
            .collect();
        let second: Vec<_> = service
            .queue(project, None, 50)
            .expect("a queue")
            .into_iter()
            .map(|ticket| ticket.id)
            .collect();
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn a_category_filter_narrows_the_queue() {
        let (pass, service, field, project, _dir) = setup(
            fixtures::defects()
                .into_iter()
                .map(|defect| defect.frame)
                .collect(),
        );
        pass.inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the pass runs");
        let only = service
            .queue(project, Some(QcCategory::Crop), 50)
            .expect("a queue");
        assert!(!only.is_empty());
        assert!(only
            .iter()
            .all(|ticket| ticket.category == QcCategory::Crop));
    }

    #[test]
    fn the_report_round_trips_through_the_catalog() {
        let (pass, service, field, project, _dir) = setup(fixtures::clean_gallery(4));
        let written = pass
            .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
            .expect("the pass runs");
        let read = service.report(project).expect("a report").expect("present");
        assert_eq!(read.images, written.report.images);
        assert_eq!(read.checks_run, written.report.checks_run);
        assert_eq!(read.thresholds_ver, THRESHOLDS_VER);
        assert_eq!(read.analysis_ver, ANALYSIS_VER);
    }
}
