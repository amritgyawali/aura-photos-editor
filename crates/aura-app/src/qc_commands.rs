//! The quality-control command surface. PHASE-27.
//!
//! Nine commands. Five read - the project header, the report, the report as text, the escalation
//! queue and one photograph's findings - one runs the pass, and three record what a photographer
//! decided. ADR-0056 records the shape and what is deliberately absent from it.
//!
//! # What this surface does that no earlier command surface does
//!
//! **Its primary object is a problem.** Every earlier panel answers "what did AURA decide about this
//! photograph". This one answers "what does AURA think is wrong with what it decided", so the reader
//! arrives sceptical and every number that would let them check a finding travels beside the
//! sentence: `deviation`, `threshold`, `unit` and `severity`, never the sentence alone.
//!
//! **The user's answer is a judgement rather than a value.** `accepted` and `dismissed` are the
//! whole of what a photographer writes here. `fixed`, `reverted`, `escalated` and `open` are
//! automation's, because they are a record of what happened rather than an opinion about it - and a
//! surface that let a person set `fixed` would let somebody record a measurement they had not made.
//!
//! # The field, and why it lives here
//!
//! `aura-qc` depends on none of the thirteen deciding crates: it takes its readings through
//! [`AppField`], which is this module's implementation of `aura_qc::api::Field`. That indirection is
//! what stops `aura-brain-photo` depending on the crate that judges it, and it is where every
//! `None` in a `Frame` comes from - a service with no row for a photograph produces an absent
//! reading, and an absent reading is a **skipped check** rather than a passed one.
//!
//! In this build most of them are absent. Phase 06's detector finds no faces, phase 18's segmenter
//! is untrained, and phase 22's face recovery never runs - so the skin, mask and sharpness checks
//! skip on most frames, `QcStatusDto.completeness` is well below one, and that is the honest answer
//! rather than a defect.
//!
//! # What is not here
//!
//! No `qc_apply`, no threshold read or write, no way to build a remedy, and no bulk action that
//! authorises one. Section 8 of ADR-0056.

use std::sync::Arc;

use aura_core::contract::cull::CullService as _;
use aura_core::contract::ids::TicketId;
use aura_core::contract::qc::{
    Evidence, ImageId, QcCategory, QcOverride, QcReport, QcService, QcTicket, Replacement,
    TicketStatus,
};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{AuraResult, ProjectId};
use aura_qc::api::{Field, Qc, QcPass, SelectedCount};
use aura_qc::checks::{Frame, SetContext};
use aura_qc::policy::Thresholds;
use aura_qc::replace::{CandidateMetric, CoverageEffect};
use aura_qc::store::QcStore;
use aura_qc::{queue, report};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    IpcError, QcDecideBulkInput, QcDecideInput, QcGroupDto, QcPassInput, QcReplacementDto,
    QcReportDto, QcRoundDto, QcStatusDto, QcTallyDto, QcTicketDto,
};
use crate::state::{AppState, QcSetReadings};

/// What the QC panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn qc_status(state: &AppState, project_id: &str) -> IpcResult<QcStatusDto> {
    let project = parse_project(project_id)?;
    let service = service(state, project)?;
    let outline = service.outline(project)?;
    Ok(QcStatusDto {
        selected: outline.selected,
        checked: outline.checked,
        coverage: outline.coverage(),
        inspections: outline.inspections,
        inspections_skipped: outline.inspections_skipped,
        completeness: outline.inspection_completeness(),
        open: outline.open,
        accepted: outline.accepted,
        dismissed: outline.dismissed,
        false_ticket_rate: outline.false_ticket_rate(),
        replaced: outline.replaced,
        rounds: outline.rounds,
        planner_calls: outline.planner_calls,
        by_category: outline.by_category.to_vec(),
        by_status: outline.by_status.to_vec(),
        bytes: outline.bytes,
        thresholds_ver: outline.thresholds_ver,
        analysis_ver: outline.analysis_ver,
        detector_trained: outline.detector_trained,
    })
}

/// What the most recent pass did.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn qc_report(state: &AppState, project_id: &str) -> IpcResult<Option<QcReportDto>> {
    let project = parse_project(project_id)?;
    let service = service(state, project)?;
    Ok(service.report(project)?.map(|report| report_dto(&report)))
}

/// The same report as Markdown, for a studio's records.
///
/// Section 2.1 asks for "PDF/Markdown". Markdown ships; ADR-0055 section 10 records why. It is
/// rendered in Rust rather than in the panel so the archived report and the queue a photographer
/// reads say the same thing.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read.
pub fn qc_report_markdown(state: &AppState, project_id: &str) -> IpcResult<Option<String>> {
    let project = parse_project(project_id)?;
    let service = service(state, project)?;
    let Some(stored) = service.report(project)? else {
        return Ok(None);
    };
    let replacements = service.replacements(project)?;
    Ok(Some(report::to_markdown(&stored, &replacements)))
}

/// The escalation queue, worst first.
///
/// `category` narrows it to one inspection, which is section 2.1's "grouped by category so a
/// photographer can clear 40 tickets in minutes".
///
/// # Errors
///
/// `AURA-ML-5137` when the category is not one of the ten. `AURA-DB-3006` when the read fails.
pub fn qc_queue(
    state: &AppState,
    project_id: &str,
    category: Option<String>,
    limit: usize,
) -> IpcResult<Vec<QcTicketDto>> {
    let project = parse_project(project_id)?;
    let filter = parse_category(category.as_deref())?;
    let service = service(state, project)?;
    let tickets = service.queue(project, filter, limit.min(500))?;
    Ok(tickets.iter().map(ticket_dto).collect())
}

/// The escalation queue, grouped by category, worst group first.
///
/// Groups are ordered by their **worst member** rather than by their size: one unsafe crop outranks
/// forty marginal colour drifts, because a photographer with twenty minutes needs the first thing
/// they open to be the worst thing there is.
///
/// # Errors
///
/// `AURA-DB-3006` when the read fails.
pub fn qc_queue_grouped(
    state: &AppState,
    project_id: &str,
    limit: usize,
) -> IpcResult<Vec<QcGroupDto>> {
    let project = parse_project(project_id)?;
    let service = service(state, project)?;
    let tickets = service.queue(project, None, limit.min(1_000))?;
    Ok(queue::group(&tickets)
        .into_iter()
        .map(|group| QcGroupDto {
            category: group.category.as_str().to_string(),
            worst: group.worst,
            tickets: group.tickets.iter().map(ticket_dto).collect(),
        })
        .collect())
}

/// Every finding on one photograph, worst first.
///
/// # Errors
///
/// `AURA-DB-3006` when the read fails.
pub fn qc_tickets(state: &AppState, image_id: &str) -> IpcResult<Vec<QcTicketDto>> {
    let image = parse_image(image_id)?;
    let project = state.project_of(aura_core::PhotoId::from_uuid(*image.as_uuid()))?;
    let service = service(state, project)?;
    Ok(service.tickets(image)?.iter().map(ticket_dto).collect())
}

/// What was tried on one finding, and what happened.
///
/// # Errors
///
/// `AURA-DB-3006` when the read fails.
pub fn qc_rounds(
    state: &AppState,
    project_id: &str,
    ticket_id: &str,
) -> IpcResult<Vec<QcRoundDto>> {
    let project = parse_project(project_id)?;
    let ticket = TicketId::from_db(ticket_id).map_err(|_| {
        IpcError::from(aura_core::errors::ml::qc_decision_refused(
            "that finding id is not one this catalog uses",
        ))
    })?;
    let service = service(state, project)?;
    Ok(service
        .rounds(ticket)?
        .into_iter()
        .map(|round| QcRoundDto {
            round: round.round,
            remedy_kind: round.remedy.kind_str().to_string(),
            remedy_target: remedy_target(&round.remedy),
            deviation_before: round.deviation_before,
            deviation_after: round.deviation_after,
            expected_gain: round.expected_gain,
            realised_share: round.realised_share(),
            collateral: round.collateral,
            collateral_category: round
                .collateral_category
                .map(|kind| kind.as_str().to_string()),
            kept: round.kept,
            outcome: round.outcome.as_str().to_string(),
            ms: round.ms,
        })
        .collect())
}

/// Run a quality-control pass.
///
/// `remediate = false` inspects and reports without changing anything, which is what a photographer
/// gets by default and what phase 28 runs before it decides whether to deliver.
///
/// **`remediate = true` is accepted and does nothing in this build.** There is no `Remediator`
/// wired: applying a remedy means re-running phase 15's or phase 16's solver for one frame, and the
/// pass that does it is phase 28's orchestration rather than this phase's. The report says so
/// through `escalated` rather than silently reporting fixes that did not happen - which is the
/// distinction ADR-0055 section 8 exists to keep.
///
/// # Errors
///
/// `AURA-ML-5136` when the pass cannot run. `AURA-ML-5140` when the thresholds table is refused.
pub fn qc_run(state: &AppState, input: QcPassInput) -> IpcResult<QcReportDto> {
    let project = parse_project(&input.project_id)?;
    let thresholds = Thresholds::shipped()?;
    let store = QcStore::new(Arc::clone(state.catalog()), Arc::clone(state.clock()));
    let pass = QcPass::new(store, thresholds);
    let field = AppField::new(state, project)?;
    let now = state.clock().now_utc().unix_timestamp() * 1_000;
    let cancel = CancelToken::new();
    // Always the inspecting form. See this function's own note: there is no remediator in this
    // build, and a `remediate = true` that quietly did nothing would be worse than one that is
    // documented as doing nothing.
    let result = pass.inspect_only(project, &field, now, &cancel, &NullProgress)?;
    Ok(report_dto(&result.report))
}

/// Record what a photographer decided about one finding.
///
/// # Errors
///
/// `AURA-ML-5137` when the status is one automation owns, when the note is too long, or when the
/// finding is not in the catalog.
pub fn qc_decide(state: &AppState, project_id: &str, input: QcDecideInput) -> IpcResult<()> {
    let project = parse_project(project_id)?;
    let service = service(state, project)?;
    let over = decide_override(&input)?;
    service.decide(&over)?;
    Ok(())
}

/// Record what a photographer decided about many findings at once.
///
/// **No remedy is ever applied here**, whatever the caller asks for: `QcDecideBulkInput` has no
/// field for it and `queue::bulk` writes `false` on every override. Agreeing that forty findings are
/// real is a statement about the findings; instructing AURA to act on forty frames unattended is a
/// statement about the remedies, and the two are different judgements made with different amounts of
/// attention. ADR-0056 section 5.
///
/// # Errors
///
/// `AURA-ML-5137` when the status is one automation owns, or when a finding is not in the catalog.
pub fn qc_decide_bulk(state: &AppState, input: QcDecideBulkInput) -> IpcResult<u32> {
    let project = parse_project(&input.project_id)?;
    let status = parse_status(&input.status)?;
    let service = service(state, project)?;
    let mut written = 0u32;
    for id in &input.ticket_ids {
        let ticket = TicketId::from_db(id).map_err(|_| {
            IpcError::from(aura_core::errors::ml::qc_decision_refused(
                "one of those finding ids is not one this catalog uses",
            ))
        })?;
        service.decide(&QcOverride {
            ticket,
            status,
            // Never true. See this function's own note.
            apply_remedy: false,
            note: input.note.clone(),
        })?;
        written = written.saturating_add(1);
    }
    Ok(written)
}

// ---------------------------------------------------------------------------
// The field
// ---------------------------------------------------------------------------

/// The readings, assembled from the thirteen frozen services.
///
/// Every `None` here becomes a skipped check rather than a passed one. See this module's header for
/// why most of them are `None` in this build.
#[derive(Debug)]
pub struct AppField<'a> {
    state: &'a AppState,
    project: ProjectId,
    selected: Vec<ImageId>,
    set: QcSetReadings,
}

impl<'a> AppField<'a> {
    /// Assemble one project's field.
    ///
    /// # Errors
    ///
    /// Whatever `CullService` raises.
    pub fn new(state: &'a AppState, project: ProjectId) -> AuraResult<Self> {
        let selected = state
            .cull()?
            .selection(project)?
            .map(|result| {
                result
                    .selected
                    .into_iter()
                    .map(|keeper| keeper.image_id)
                    .collect()
            })
            .unwrap_or_default();
        // The gallery-scoped readings, gathered once. A nearest-neighbour search run per frame
        // would be a thousand scans of one table inside a ninety-second budget.
        let set = state.qc_set_readings(project)?;
        Ok(Self {
            state,
            project,
            selected,
            set,
        })
    }
}

impl Field for AppField<'_> {
    fn selected(&self, _project: ProjectId) -> AuraResult<Vec<ImageId>> {
        Ok(self.selected.clone())
    }

    fn frame(&self, image: ImageId) -> AuraResult<Frame> {
        Ok(self.state.qc_frame(self.project, image, &self.set))
    }

    fn coverage(&self, project: ProjectId) -> AuraResult<SetContext> {
        let Some(selection) = self.state.cull()?.selection(project)? else {
            // A project phase 12 never selected has no coverage to be missing, which is a skip
            // rather than a clean bill of health.
            return Ok(SetContext::default());
        };
        let mut context = SetContext {
            coverage_available: true,
            ..SetContext::default()
        };
        for (rule, state) in &selection.coverage.must_haves {
            // `Missing` because nobody shot it is not a QC finding: there is nothing to fix and
            // nothing to escalate, and a ticket would send a photographer looking for a photograph
            // that does not exist. Phase 12's own rule, consumed here.
            // Three arms rather than two, and `Missing` and `Covered` are deliberately separate:
            // one says nobody shot it and the other says it is covered, and collapsing them would
            // leave the next reader of this function unable to tell which case was considered.
            #[allow(clippy::match_same_arms)]
            match state {
                aura_core::contract::cull::Coverage::Missing => {}
                aura_core::contract::cull::Coverage::CoveredWeak => {
                    context.weak_rules.push(format!("{rule:?}"));
                }
                aura_core::contract::cull::Coverage::Covered => {}
            }
        }
        Ok(context)
    }

    fn coverage_effect(&self, _image: ImageId) -> AuraResult<CoverageEffect> {
        // Conservative in the direction that refuses swaps. Establishing that a runner-up carries
        // the same guarantee needs phase 12's coverage guard re-run over a hypothetical selection,
        // which is phase 28's orchestration; until then a protected frame is never swapped and the
        // ticket carries `ReplacementBreaksCoverage` saying so.
        Ok(CoverageEffect::unprotected())
    }

    fn candidate(
        &self,
        _runner_up: ImageId,
        _category: QcCategory,
    ) -> AuraResult<Option<CandidateMetric>> {
        // `None` refuses the swap rather than permitting it: a candidate nobody measured is not a
        // candidate anybody can prefer. Measuring a runner-up means running the deciding phases over
        // a frame that was never selected, which is work this build does not do.
        Ok(None)
    }
}

/// How many frames are in the delivered gallery.
#[derive(Debug)]
pub struct AppSelected(pub u32);

impl SelectedCount for AppSelected {
    fn selected(&self, _project: ProjectId) -> AuraResult<u32> {
        Ok(self.0)
    }
}

// ---------------------------------------------------------------------------
// DTO helpers
// ---------------------------------------------------------------------------

fn ticket_dto(ticket: &QcTicket) -> QcTicketDto {
    let (frames, crop) = match &ticket.evidence {
        Evidence::Frames(list) | Evidence::Anchors(list) => {
            (list.iter().map(ImageId::to_db).collect(), None)
        }
        Evidence::Crop(rect) => (Vec::new(), Some(vec![rect.x, rect.y, rect.w, rect.h])),
        Evidence::Params(_) | Evidence::None => (Vec::new(), None),
    };
    QcTicketDto {
        ticket_id: ticket.id.to_db(),
        image_id: ticket.image_id.to_db(),
        category: ticket.category.as_str().to_string(),
        code: ticket.code.as_str().to_string(),
        // Rendered rather than stored - phase 09's rule and ADR-0055 section 3 - and rendered here
        // rather than in the panel so the archived report and the queue say the same thing.
        diagnosis: ticket.render_diagnosis(),
        deviation: ticket.deviation,
        threshold: ticket.threshold,
        unit: ticket.category.unit().to_string(),
        severity: ticket.severity(),
        remedy_kind: ticket.remedy.kind_str().to_string(),
        remedy_target: remedy_target(&ticket.remedy),
        expected_gain: ticket.expected_gain,
        confidence: ticket.confidence,
        autonomy: ticket.autonomy.as_str().to_string(),
        may_act_unattended: ticket.may_act_unattended(),
        round: ticket.round,
        status: ticket.status.as_str().to_string(),
        outcome_code: ticket.outcome_code.map(|code| code.as_str().to_string()),
        scene: ticket.scene.as_str().to_string(),
        evidence_kind: ticket.evidence.kind_str().to_string(),
        evidence_frames: frames,
        evidence_crop: crop,
        reasons: ticket
            .top_reasons(QcTicket::MAX_REASONS)
            .into_iter()
            .map(|reason| reason.text().to_string())
            .collect(),
    }
}

fn report_dto(report: &QcReport) -> QcReportDto {
    QcReportDto {
        images: report.images,
        images_unreached: report.images_unreached,
        complete: report.complete(),
        checks_run: report.checks_run,
        skipped: report.skipped,
        by_category: QcCategory::ALL
            .iter()
            .zip(report.by_category.iter())
            .map(|(category, tally)| QcTallyDto {
                category: category.as_str().to_string(),
                found: tally.found,
                fixed: tally.fixed,
                escalated: tally.escalated,
                skipped: tally.skipped,
            })
            .collect(),
        found: report.found(),
        fixed: report.fixed,
        reverted: report.reverted,
        escalated: report.escalated,
        replacements: report.replacements.iter().map(replacement_dto).collect(),
        planner_calls: report.planner_calls,
        cloud_used: report.cloud_used,
        duration_ms: report.duration_ms,
        thresholds_ver: report.thresholds_ver,
        analysis_ver: report.analysis_ver,
    }
}

fn replacement_dto(swap: &Replacement) -> QcReplacementDto {
    QcReplacementDto {
        ticket_id: swap.ticket.to_db(),
        replaced: swap.replaced.to_db(),
        replacement: swap.replacement.to_db(),
        category: swap.category.as_str().to_string(),
        metric_before: swap.metric_before,
        metric_after: swap.metric_after,
        confidence: swap.confidence,
        coverage_held: swap.coverage_held,
        note: swap.note.clone(),
    }
}

/// What a remedy acts on, as one string.
fn remedy_target(remedy: &aura_core::contract::qc::Remedy) -> String {
    use aura_core::contract::qc::Remedy;
    match remedy {
        Remedy::ResolveParam { target, constraint } => format!("{target}: {constraint}"),
        Remedy::ReduceStrength { op, factor } => format!("{op} to {factor:.2}"),
        Remedy::RevertOp { op } => op.clone(),
        Remedy::ReplaceFrame { with } => with.to_db(),
        Remedy::Escalate { note } => note.clone(),
    }
}

fn decide_override(input: &QcDecideInput) -> IpcResult<QcOverride> {
    Ok(QcOverride {
        ticket: TicketId::from_db(&input.ticket_id).map_err(|_| {
            IpcError::from(aura_core::errors::ml::qc_decision_refused(
                "that finding id is not one this catalog uses",
            ))
        })?,
        status: parse_status(&input.status)?,
        apply_remedy: input.apply_remedy,
        note: input.note.clone(),
    })
}

fn parse_status(text: &str) -> IpcResult<TicketStatus> {
    match TicketStatus::parse(text) {
        Some(status) if status.is_user_set() => Ok(status),
        _ => Err(IpcError::from(aura_core::errors::ml::qc_decision_refused(
            format!(
                "'{text}' is not a verdict a person may record. Only 'accepted' and 'dismissed' \
                 are; the others are a record of what AURA did rather than an opinion about it"
            ),
        ))),
    }
}

fn parse_category(text: Option<&str>) -> IpcResult<Option<QcCategory>> {
    match text {
        None => Ok(None),
        Some(name) => QcCategory::parse(name).map(Some).ok_or_else(|| {
            IpcError::from(aura_core::errors::ml::qc_decision_refused(format!(
                "'{name}' is not one of the ten inspections"
            )))
        }),
    }
}

fn parse_project(text: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(text).map_err(|_| {
        IpcError::from(aura_core::errors::ml::qc_decision_refused(
            "that project id is not one this catalog uses",
        ))
    })
}

fn parse_image(text: &str) -> IpcResult<ImageId> {
    ImageId::from_db(text).map_err(|_| {
        IpcError::from(aura_core::errors::ml::qc_decision_refused(
            "that photograph id is not one this catalog uses",
        ))
    })
}

/// The one implementation of `QcService`, wrapped for this surface.
pub(crate) fn service(state: &AppState, project: ProjectId) -> AuraResult<Qc> {
    let selected = state.cull()?.selection(project)?.map_or(0, |result| {
        u32::try_from(result.selected.len()).unwrap_or(0)
    });
    Ok(Qc::new(
        QcStore::new(Arc::clone(state.catalog()), Arc::clone(state.clock())),
        Arc::new(AppSelected(selected)),
    ))
}
