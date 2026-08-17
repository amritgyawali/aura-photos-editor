//! The explainability command surface.
//!
//! Eight commands. Six read, one records the stored gallery's decisions into the ledger, and
//! one exports a support bundle.
//!
//! ## Nothing here decides anything
//!
//! There is no command in this file that changes a decision, and that is the boundary worth
//! defending. A photographer who disagrees with the panel changes the *decision*, through the
//! culling surface, and the ledger then records a new decision that supersedes the old one.
//! A surface that could edit an explanation from inside the explanation would be a surface
//! where the explanation and the decision could disagree, and section 12's first failure mode
//! is exactly that.
//!
//! ## Why the panel is assembled here rather than in the interface
//!
//! Because four of its six tabs come from four different frozen services, and the fifth and
//! sixth come from phases that do not exist. Assembling that in TypeScript would mean the web
//! view knowing which phases exist, which codes are caveats and which tabs are legitimately
//! empty - and a tab that renders blank because phase 14 has not been written looks exactly
//! like a tab that renders blank because a photograph is perfect.
//!
//! So the backend says: available, or unavailable *with a reason*.

use std::time::Instant;

use aura_core::contract::composition::CompositionService;
use aura_core::contract::cull::{CullService, Decision as CullDecision};
use aura_core::contract::emotion::EmotionService;
use aura_core::contract::integrity::IntegrityService;
use aura_core::contract::ledger::{
    Autonomy, DecisionKind, DecisionSource, DecisionSubject, Evidence, ExplainService,
    LedgerDecision, LedgerReason,
};
use aura_core::{PhotoId, ProjectId};
use aura_explain::adapt::{self, CullContext};
use aura_explain::policy::Risk;
use aura_explain::reason::Catalog as ReasonCatalog;
use aura_explain::summary::Summariser;
use aura_explain::{Bundle, BundleOptions};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AlternativeDto, EvidenceCropDto, ExplainPanelDto, ExplainTabDto, ExportBundleInput, IpcError,
    LedgerDecisionDto, LedgerReasonDto, LedgerStatusDto, ParamDeltaDto, RecordDecisionsDto,
    RecordDecisionsInput, ReviewQueueInput, SupportBundleDto,
};
use crate::state::AppState;

/// How many decisions a review queue returns when the caller does not say.
const DEFAULT_QUEUE: u32 = 200;

/// Everything the Explain panel draws for one photograph.
///
/// # Errors
///
/// `AURA-DB-3006` when any of the four services cannot be read.
pub fn explain_image(state: &AppState, photo_id: &str) -> IpcResult<ExplainPanelDto> {
    let image = parse_photo(photo_id)?;
    let registry = ReasonCatalog::shipped();

    let cull_decision = state.cull()?.of_image(image)?;
    let ledger_decision = state
        .explain()?
        .latest(DecisionSubject::Image(image), DecisionKind::Cull)?;

    let mut tabs = Vec::with_capacity(6);
    tabs.push(selection_tab(cull_decision.as_ref(), &registry));

    match state.integrity().of_image(image)? {
        Some(result) => tabs.push(ExplainTabDto {
            id: "technical".to_string(),
            title: "Technical".to_string(),
            available: true,
            unavailable_reason: None,
            reasons: adapt::technical_reasons(&result)
                .iter()
                .map(|reason| reason_dto(reason, &registry))
                .collect(),
            score: Some(result.technical_score),
            confidence: Some(result.confidence),
        }),
        None => tabs.push(unavailable(
            "technical",
            "Technical",
            "AURA has not checked this photograph yet, so there is nothing to show here. This \
             is not a judgement about it.",
        )),
    }

    match state.emotion()?.of_image(image)? {
        Some(reading) => tabs.push(ExplainTabDto {
            id: "emotion".to_string(),
            title: "Emotion".to_string(),
            available: true,
            unavailable_reason: None,
            reasons: adapt::emotion_reasons(&reading)
                .iter()
                .map(|reason| reason_dto(reason, &registry))
                .collect(),
            score: Some(reading.emotion_score),
            confidence: Some(reading.confidence),
        }),
        None => tabs.push(unavailable(
            "emotion",
            "Emotion",
            "AURA has not read this photograph for expression or interaction yet.",
        )),
    }

    match state.composition().of_image(image)? {
        Some(result) => tabs.push(ExplainTabDto {
            id: "composition".to_string(),
            title: "Composition".to_string(),
            available: true,
            unavailable_reason: None,
            reasons: adapt::composition_reasons(&result)
                .iter()
                .map(|reason| reason_dto(reason, &registry))
                .collect(),
            score: Some(result.composition_score),
            confidence: Some(result.confidence),
        }),
        None => tabs.push(unavailable(
            "composition",
            "Composition",
            "AURA has not judged how this photograph is framed yet.",
        )),
    }

    // The two tabs whose phases do not exist. They are present and say why, rather than
    // absent: a missing tab reads as "there is nothing to say about the edit", and what is
    // true is that nothing has edited anything yet.
    tabs.push(unavailable(
        "edit",
        "Edit",
        "AURA has not developed this photograph. The develop engine arrives in a later \
         release, and this tab will then list the exact parameters and masks it applied.",
    ));
    tabs.push(unavailable(
        "qc",
        "Quality check",
        "The quality-check agent arrives in a later release. When it does, anything it \
         found here and what it did about it will be listed on this tab.",
    ));

    let (summary, headline, from_cloud) = match ledger_decision.as_ref() {
        Some(decision) => {
            let assembled = Summariser::template(decision);
            (assembled.summary, assembled.headline, assembled.from_cloud)
        }
        None => (
            "AURA has not recorded a decision about this photograph yet.".to_string(),
            None,
            false,
        ),
    };

    Ok(ExplainPanelDto {
        photo_id: image.to_db(),
        tabs,
        decision: ledger_decision.as_ref().map(decision_dto),
        headline,
        summary,
        summary_from_cloud: from_cloud,
        alternatives: alternatives(state, image, cull_decision.as_ref())?,
    })
}

/// Every decision ever recorded about one photograph, newest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the ledger cannot be read.
pub fn decision_history(state: &AppState, photo_id: &str) -> IpcResult<Vec<LedgerDecisionDto>> {
    let image = parse_photo(photo_id)?;
    Ok(state
        .explain()?
        .history(DecisionSubject::Image(image))?
        .iter()
        .map(decision_dto)
        .collect())
}

/// What one wedding's ledger holds.
///
/// # Errors
///
/// `AURA-DB-3006` when the ledger cannot be read.
pub fn ledger_status(state: &AppState, project_id: &str) -> IpcResult<LedgerStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.explain()?.outline(project)?;
    Ok(LedgerStatusDto {
        decisions: outline.decisions,
        explained: outline.explained,
        explanation_coverage: outline.explanation_coverage(),
        current: outline.current,
        superseded: outline.superseded,
        by_kind: outline.by_kind.to_vec(),
        kind_names: DecisionKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        by_autonomy: outline.by_autonomy.to_vec(),
        autonomy_names: Autonomy::ALL
            .iter()
            .map(|band| band.as_str().to_string())
            .collect(),
        by_source: outline.by_source.to_vec(),
        source_names: DecisionSource::ALL
            .iter()
            .map(|source| source.as_str().to_string())
            .collect(),
        calibrated: outline.calibrated,
        calibration_ver: outline.calibration_ver,
        evidenced: outline.evidenced,
        reasons: outline.reasons,
        bytes: outline.bytes,
    })
}

/// The decisions waiting for a person.
///
/// # Errors
///
/// `AURA-DB-3006` when the ledger cannot be read.
pub fn review_queue(
    state: &AppState,
    input: &ReviewQueueInput,
) -> IpcResult<Vec<LedgerDecisionDto>> {
    let project = parse_project(&input.project_id)?;
    let band = input.band.as_deref().map(Autonomy::from_str_or_review);
    let limit = input.limit.unwrap_or(DEFAULT_QUEUE);
    Ok(state
        .explain()?
        .ledger()
        .review_queue(project, band, limit)?
        .iter()
        .map(decision_dto)
        .collect())
}

/// Record the stored gallery's decisions into the ledger.
///
/// The one command here that writes. It reads the selection through `CullService` - which is
/// still the only way to ask what is being delivered - and records one decision per
/// photograph on both sides of the line.
///
/// **Re-running it is safe and is not idempotent.** Each run appends a new decision per
/// frame, because the ledger is append-only and a second cull genuinely is a second decision.
/// Compaction is what keeps that bounded.
///
/// # Errors
///
/// `AURA-DB-3006` when the selection or the ledger cannot be read, and `AURA-ML-5055` when
/// the band table will not load. A decision that cannot explain itself is counted in
/// `refused` rather than failing the pass.
pub fn record_decisions(
    state: &AppState,
    input: &RecordDecisionsInput,
) -> IpcResult<RecordDecisionsDto> {
    let project = parse_project(&input.project_id)?;
    let cull = state.cull()?;
    let explain = state.explain()?;

    let Some(selection) = cull.selection(project)? else {
        return Err(refused(
            "this wedding has not been culled yet, so nothing has been decided",
        ));
    };

    // Telemetry only, never an input to a decision: a ledger row that differed because one
    // machine was slower would break the replay.
    let started = Instant::now(); // DETERMINISM: telemetry only, never an input to a decision
    let context = CullContext {
        model_ver: selection.model_ver,
        analysis_ver: selection.analysis_ver,
        score_calibration_ver: selection.calibration_ver,
        run_hash: selection.deterministic_hash,
        ..CullContext::default()
    };

    let mut recorded = 0u32;
    let mut refused_count = 0u32;
    let mut bands = [0u32; Autonomy::COUNT];

    for keep in &selection.selected {
        let decision = CullDecision::Keep(Box::new(keep.clone()));
        let risk = if keep.is_protected() {
            Risk::must_have()
        } else {
            Risk::NONE
        };
        let builder = adapt::cull_decision(project, &decision, &context);
        match explain.record_built(builder, risk) {
            Ok(stored) => {
                recorded = recorded.saturating_add(1);
                bump(&mut bands, stored.autonomy);
            }
            Err(_) => refused_count = refused_count.saturating_add(1),
        }
    }
    for drop in &selection.rejected {
        let decision = CullDecision::Drop(Box::new(drop.clone()));
        let builder = adapt::cull_decision(project, &decision, &context);
        match explain.record_built(builder, Risk::NONE) {
            Ok(stored) => {
                recorded = recorded.saturating_add(1);
                bump(&mut bands, stored.autonomy);
            }
            Err(_) => refused_count = refused_count.saturating_add(1),
        }
    }

    Ok(RecordDecisionsDto {
        recorded,
        refused: refused_count,
        by_autonomy: bands.to_vec(),
        autonomy_names: Autonomy::ALL
            .iter()
            .map(|band| band.as_str().to_string())
            .collect(),
        uncalibrated: !explain.calibration().is_fitted(),
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

/// Export an anonymised support bundle.
///
/// # Errors
///
/// `AURA-ML-5059` when there is nothing to export, and `AURA-DB-3006` when the ledger cannot
/// be read.
pub fn export_support_bundle(
    state: &AppState,
    input: &ExportBundleInput,
) -> IpcResult<SupportBundleDto> {
    let project = parse_project(&input.project_id)?;
    let explain = state.explain()?;
    let decisions = explain.ledger().project(project)?;
    let outline = explain.outline(project)?;
    let options = BundleOptions {
        include_text: true,
        limit: input
            .limit
            .and_then(|limit| usize::try_from(limit).ok())
            .unwrap_or(BundleOptions::default().limit),
    };
    let bundle = Bundle::build(&decisions, &outline, &options)?;
    Ok(SupportBundleDto {
        decisions: u32::try_from(bundle.decisions).unwrap_or(u32::MAX),
        anonymised: u32::try_from(bundle.anonymised).unwrap_or(u32::MAX),
        safe: bundle.is_safe(),
        json: bundle.json,
    })
}

/// Compact one project's ledger, returning how many rows went.
///
/// # Errors
///
/// `AURA-DB-3006` when the compaction cannot run.
pub fn compact_ledger(state: &AppState, project_id: &str) -> IpcResult<u32> {
    let project = parse_project(project_id)?;
    Ok(state.explain()?.compact(project)?)
}

/// One decision, by its id, for the support engineer and for `aura-cli replay`.
///
/// # Errors
///
/// `AURA-DB-3006` when the ledger cannot be read.
pub fn decision_by_id(state: &AppState, decision_id: &str) -> IpcResult<Option<LedgerDecisionDto>> {
    let id = aura_core::DecisionId::from_db(decision_id).map_err(|_| bad_id("decision"))?;
    Ok(state.explain()?.decision(id)?.as_ref().map(decision_dto))
}

// -- wire shapes ------------------------------------------------------------

fn selection_tab(decision: Option<&CullDecision>, registry: &ReasonCatalog) -> ExplainTabDto {
    let Some(decision) = decision else {
        return unavailable(
            "selection",
            "Selection",
            "This wedding has not been culled yet, so nothing has been decided about this \
             photograph. That is not the same as it having been left out.",
        );
    };
    let (score, confidence) = match decision {
        CullDecision::Keep(keep) => (Some(keep.keep_score), Some(keep.confidence)),
        CullDecision::Drop(drop) => (Some(drop.keep_score), None),
    };
    ExplainTabDto {
        id: "selection".to_string(),
        title: "Selection".to_string(),
        available: true,
        unavailable_reason: None,
        reasons: decision
            .reasons()
            .iter()
            .map(|reason| {
                reason_dto(
                    &LedgerReason {
                        code: reason.code.as_str().to_string(),
                        text: reason.text.clone(),
                        weight: reason.weight,
                        evidence: Evidence::None,
                    },
                    registry,
                )
            })
            .collect(),
        score,
        confidence,
    }
}

/// A tab that has nothing to show, and says why.
fn unavailable(id: &str, title: &str, why: &str) -> ExplainTabDto {
    ExplainTabDto {
        id: id.to_string(),
        title: title.to_string(),
        available: false,
        unavailable_reason: Some(why.to_string()),
        reasons: Vec::new(),
        score: None,
        confidence: None,
    }
}

/// The delivered frame and the one that nearly won, with both breakdowns.
///
/// Section 6.2 calls this "the single most trust-building screen in the product", and the
/// reason is that it is the only screen where a photographer can check the decision rather
/// than read about it. Empty when the moment held one frame - which is honest, and is not the
/// same as "there was nothing as good".
fn alternatives(
    state: &AppState,
    image: PhotoId,
    decision: Option<&CullDecision>,
) -> IpcResult<Vec<AlternativeDto>> {
    let Some(decision) = decision else {
        return Ok(Vec::new());
    };
    let other = match decision {
        CullDecision::Keep(keep) => keep.runner_up,
        CullDecision::Drop(drop) => drop.kept_instead,
    };
    let Some(other) = other else {
        return Ok(Vec::new());
    };

    let mut out = Vec::with_capacity(2);
    for (id, delivered) in [(image, decision.is_keep()), (other, !decision.is_keep())] {
        out.push(breakdown(state, id, delivered)?);
    }
    Ok(out)
}

/// One frame's score breakdown, read back through the three frozen services.
///
/// The sub-scores are read from the analysis rather than from the selection row, and the
/// difference matters: a rejected frame's row carries its fused score and this carries the
/// four numbers underneath it, which is what a photographer is actually asking about when
/// they open a comparison.
fn breakdown(state: &AppState, image: PhotoId, delivered: bool) -> IpcResult<AlternativeDto> {
    let technical = state
        .integrity()
        .of_image(image)?
        .map_or(0.0, |result| result.technical_score);
    let emotion = state
        .emotion()?
        .of_image(image)?
        .map_or(0.0, |reading| reading.emotion_score);
    let composition = state
        .composition()
        .of_image(image)?
        .map_or(0.0, |result| result.composition_score);
    let keep_score = match state.cull()?.of_image(image)? {
        Some(CullDecision::Keep(keep)) => keep.keep_score,
        Some(CullDecision::Drop(drop)) => drop.keep_score,
        None => 0.0,
    };
    Ok(AlternativeDto {
        photo_id: image.to_db(),
        keep_score,
        technical,
        emotion,
        composition,
        // Neutral rather than zero when no face pass has run, which is the value
        // `KeepScore::NEUTRAL` documents: a frame nobody looked for faces in must not be
        // rendered as a frame with nobody in it.
        prominence: aura_core::KeepScore::NEUTRAL,
        delivered,
    })
}

fn decision_dto(decision: &LedgerDecision) -> LedgerDecisionDto {
    let registry = ReasonCatalog::shipped();
    LedgerDecisionDto {
        decision_id: decision.id.to_db(),
        kind: decision.kind.as_str().to_string(),
        kind_title: decision.kind.title().to_string(),
        subject_kind: decision.subject.kind_str().to_string(),
        subject_id: decision.subject.id_str(),
        raw_confidence: decision.raw_confidence,
        calibrated_confidence: decision.calibrated_confidence,
        calibration_ver: decision.calibration_ver,
        calibrated: decision.is_calibrated(),
        autonomy: decision.autonomy.as_str().to_string(),
        autonomy_title: decision.autonomy.title().to_string(),
        autonomy_text: decision.autonomy.user_text().to_string(),
        needs_review: decision.needs_review(),
        source: decision.source.as_str().to_string(),
        reasons: decision
            .reasons
            .iter()
            .map(|reason| reason_dto(reason, &registry))
            .collect(),
        outputs_json: decision.outputs_json.clone(),
        inputs_hash: format!("{:016x}", decision.inputs_hash),
        model_versions: decision.model_versions.clone(),
        config_versions: decision.config_versions.clone(),
        ms: decision.ms,
        created_at: decision.created_at,
        supersedes: decision.supersedes.map(|id| id.to_db()),
    }
}

fn reason_dto(reason: &LedgerReason, registry: &ReasonCatalog) -> LedgerReasonDto {
    let entry = registry.get(&reason.code);
    LedgerReasonDto {
        code: reason.code.clone(),
        text: reason.text.clone(),
        weight: reason.weight,
        severity: registry.severity(&reason.code).as_str().to_string(),
        domain: entry.map_or("ledger", |found| found.domain).to_string(),
        evidence_kind: reason.evidence.kind_str().to_string(),
        crop: reason.evidence.crop().map(|area| EvidenceCropDto {
            x: area.x,
            y: area.y,
            w: area.w,
            h: area.h,
        }),
        frames: reason
            .evidence
            .frames()
            .iter()
            .map(aura_core::PhotoId::to_db)
            .collect(),
        params: reason
            .evidence
            .params()
            .iter()
            .map(|(name, value)| ParamDeltaDto {
                name: name.clone(),
                value: *value,
            })
            .collect(),
    }
}

fn bump(bands: &mut [u32; Autonomy::COUNT], band: Autonomy) {
    if let Some(slot) = Autonomy::ALL
        .iter()
        .position(|candidate| *candidate == band)
        .and_then(|index| bands.get_mut(index))
    {
        *slot = slot.saturating_add(1);
    }
}

fn parse_project(text: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(text).map_err(|_| bad_id("project"))
}

fn parse_photo(text: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(text).map_err(|_| bad_id("photo"))
}

fn bad_id(kind: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        format!("{kind} id was not a valid AURA id"),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}

fn refused(detail: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        detail,
        &std::io::Error::from(std::io::ErrorKind::NotFound),
    ))
}
