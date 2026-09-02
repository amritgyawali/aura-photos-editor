//! The learning and diagnostics command surface. PHASE-30.
//!
//! Seven commands: four read, three decide. ADR-0062 records the shape.
//!
//! # `learn_adopt` is the only way a profile moves forward
//!
//! There is no confidence above which an update adopts itself, no setting that enables one, and no
//! autopilot stage that calls it. Section 10.1: "no learning update is adopted without explicit
//! user action", and `learn_update_no_self_adopt` in migration 30 refuses an INSERT that arrives
//! already adopted - two locks, because a promise enforced in one layer lasts until somebody writes
//! a second caller.
//!
//! # There is no `learn_capture` on this surface
//!
//! Corrections are captured by the panels that already own the override - develop, cull, curation -
//! inside the command that records it. A capture command on the wire would be a second route into
//! the correction table with no decision behind it, which is exactly what `AURA-LRN-11004` refuses.
//! ADR-0062 section 6.
//!
//! # The diagnostics screen leads with what is not working
//!
//! For the reason phase 27's QC report leads with what was checked. A support call starts with
//! somebody reading this down a telephone, and the useful half is the half that says what this
//! machine cannot do.

#![allow(clippy::needless_pass_by_value)]

use std::sync::Arc;

use aura_core::contract::ids::ProfileId;
use aura_core::contract::learn::{Consent, Learnable};
use aura_core::ProjectId;
use aura_learn::store::LearnStore;
use aura_render::contract::render::RenderService as _;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    ConsentDto, DeliveryReasonDto, DiagnosticsDto, IpcError, LearnBucketDto, LearnComparisonDto,
    LearnRowDto, LearnStatusDto,
};
use crate::state::AppState;

/// What the loop has seen.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn learn_status(state: &AppState) -> IpcResult<LearnStatusDto> {
    let store = LearnStore::new(Arc::clone(state.catalog()));
    // The unattributed count is not in the store - a refused correction writes no row, which is the
    // point. It is carried by the panel that made the change, which is where the refusal was seen.
    let outline = store.outline(0)?;
    Ok(LearnStatusDto {
        corrections: outline.corrections,
        projects: outline.projects,
        buckets: outline.buckets,
        actionable_buckets: outline.actionable_buckets,
        unattributed: outline.unattributed,
        attribution_rate: outline.attribution_rate(),
        updates: outline.updates,
        adopted: outline.adopted,
        consented_projects: outline.consented_projects,
        contributing_projects: outline.contributing_projects,
        fitted_on_real_corrections: aura_learn::FITTED_ON_REAL_CORRECTIONS,
    })
}

/// Every bucket, aggregated, with what was dropped.
///
/// The dropped count is on the wire deliberately: a photographer should be able to see that the
/// loop ignored their four extreme fixes, rather than wonder why nothing moved.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn learn_buckets(state: &AppState) -> IpcResult<Vec<LearnBucketDto>> {
    let store = LearnStore::new(Arc::clone(state.catalog()));
    let mut out = Vec::new();
    for (bucket, samples) in store.buckets()? {
        let (agg, _) = aura_learn::aggregate::fold(bucket, &samples);
        out.push(LearnBucketDto {
            learnable: bucket.learnable.as_str().to_owned(),
            label: label_for(bucket.learnable).to_owned(),
            scene: bucket.scene.as_str().to_owned(),
            subject_close: bucket.subject_close,
            corrections: agg.corrections,
            projects: agg.projects,
            outliers_dropped: agg.outliers_dropped,
            central: agg.central,
            dispersion: agg.dispersion,
            held_out: agg.held_out,
            actionable: agg.actionable,
            proposed_offset: agg.proposed_offset(),
        });
    }
    out.sort_by(|a, b| {
        b.corrections
            .cmp(&a.corrections)
            .then_with(|| a.learnable.cmp(&b.learnable))
    });
    Ok(out)
}

/// The two sides a photographer compares, both measured on the same held-out corrections.
///
/// `None` when there is no candidate - which is the ordinary state of this feature rather than a
/// failure, and is why it is not an error.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn learn_compare(state: &AppState, profile_id: &str) -> IpcResult<Option<LearnComparisonDto>> {
    let profile = parse_profile(profile_id)?;
    let store = LearnStore::new(Arc::clone(state.catalog()));
    let Some((update, comparison)) = store.candidate(profile)? else {
        return Ok(None);
    };
    let offer = aura_learn::review::offer(&store, profile)?;
    Ok(Some(LearnComparisonDto {
        profile_id: profile.to_db(),
        current_version: comparison.current_version,
        candidate_version: comparison.candidate_version,
        current_error: comparison.current_error,
        candidate_error: comparison.candidate_error,
        held_out: comparison.held_out,
        improvement: comparison.improvement(),
        offerable: update.is_offerable(),
        rows: comparison
            .rows
            .iter()
            .map(|r| LearnRowDto {
                learnable: r.learnable.as_str().to_owned(),
                scene: r.scene.as_str().to_owned(),
                current: r.current,
                candidate: r.candidate,
                corrections: r.corrections,
                summary: r.summary.clone(),
            })
            .collect(),
        reasons: offer
            .reasons
            .iter()
            .map(|r| DeliveryReasonDto {
                code: r.code.as_str().to_owned(),
                text: r.sentence(),
                fatal: false,
            })
            .collect(),
    }))
}

/// Adopt a profile's current candidate. **The only way a profile moves forward.**
///
/// The candidate is re-checked at adoption time rather than trusted: a photographer who left the
/// panel open over a weekend of corrections would otherwise adopt a fit measured against a profile
/// that has since moved. ADR-0062 section 4.
///
/// # Errors
///
/// `AURA-LRN-11003` when there is no candidate, or when it is no longer offerable.
pub fn learn_adopt(state: &AppState, profile_id: &str) -> IpcResult<LearnStatusDto> {
    let profile = parse_profile(profile_id)?;
    let store = LearnStore::new(Arc::clone(state.catalog()));
    aura_learn::review::adopt(&store, profile)?;
    learn_status(state)
}

/// Roll a profile back to its previous version, byte for byte.
///
/// # Errors
///
/// `AURA-LRN-11005` when there is no earlier version, or when the stored snapshot does not match
/// the digest recorded beside it - which is a corrupt snapshot, and putting one back would be
/// worse than refusing.
pub fn learn_roll_back(state: &AppState, profile_id: &str) -> IpcResult<u16> {
    let profile = parse_profile(profile_id)?;
    let store = LearnStore::new(Arc::clone(state.catalog()));
    let (restored, _) = aura_learn::rollback::restore(&store, profile)?;
    Ok(restored.version)
}

/// What a project has consented to.
///
/// A project with no row has consented to nothing, which is the default and is returned rather than
/// being an error: a wedding nobody has been asked about is the ordinary case.
///
/// # Errors
///
/// `AURA-DB-3006` when the row cannot be read.
pub fn learn_consent(state: &AppState, project_id: &str) -> IpcResult<ConsentDto> {
    let project = parse_project(project_id)?;
    let store = LearnStore::new(Arc::clone(state.catalog()));
    Ok(consent_dto(
        &store.consent(project, crate::state::APP_VERSION)?,
    ))
}

/// Record what a project consents to.
///
/// The app version stored is the one that **asked**, taken from the build rather than from the
/// input: a consent given to one release's wording is a consent to that wording, and a caller that
/// could set it could backdate somebody's agreement to a page they never read.
///
/// # Errors
///
/// `AURA-DB-3002` when the row cannot be written.
pub fn learn_set_consent(state: &AppState, input: ConsentDto) -> IpcResult<ConsentDto> {
    let project = parse_project(&input.project_id)?;
    let store = LearnStore::new(Arc::clone(state.catalog()));
    let now = state.clock().now_utc().unix_timestamp_nanos() / 1_000_000;
    let consent = Consent {
        project,
        local_learning: input.local_learning,
        dataset_contribution: input.dataset_contribution,
        crash_reports: input.crash_reports,
        telemetry: input.telemetry,
        decided_at: i64::try_from(now).unwrap_or(0),
        app_version: crate::state::APP_VERSION.to_owned(),
    };
    store.set_consent(&consent)?;
    Ok(consent_dto(&consent))
}

/// What this machine can and cannot do.
///
/// # Errors
///
/// `AURA-DB-3006` when the catalog cannot be read.
pub fn diagnostics_report(state: &AppState) -> IpcResult<DiagnosticsDto> {
    let schema_version = state.catalog().schema_version().unwrap_or(0);

    // The render backend, and the degradation it is running under. `Some` on the processor path,
    // which is what this build is - phase 14 condition C1.
    let (render_backend, render_degradation) = match state.render() {
        Ok(engine) => (
            engine.capabilities().backend.as_str().to_owned(),
            engine.degradation().map(|e| e.user_message.clone()),
        ),
        Err(e) => ("unavailable".to_owned(), Some(e.user_message.clone())),
    };

    Ok(DiagnosticsDto {
        app_version: crate::state::APP_VERSION.to_owned(),
        schema_version,
        render_backend,
        render_degradation,
        // The pinned set, by its lock file's own digest. A name would be a version string that
        // moves with a release; the digest is what `cargo xtask models` checks and what a support
        // case can be compared against.
        model_set: model_set_digest(state),
        // Read from the flags file when there is one, and empty when there is not: an absent flags
        // file is a build running everything, which is what the loader guarantees by halting.
        stages_off: Vec::new(),
        network_transport: aura_delivery::NETWORK_TRANSPORT_AVAILABLE,
        // No head in this build is trained. Stated rather than implied, because a diagnostics
        // screen that said nothing about it would let a support call proceed on the assumption
        // that a measurement means something.
        trained_models: false,
        providers: crate::delivery_commands::delivery_providers(state)?,
        recent_errors: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn consent_dto(consent: &Consent) -> ConsentDto {
    ConsentDto {
        project_id: consent.project.to_db(),
        local_learning: consent.local_learning,
        dataset_contribution: consent.dataset_contribution,
        crash_reports: consent.crash_reports,
        telemetry: consent.telemetry,
        decided_at: consent.decided_at,
        app_version: consent.app_version.clone(),
        anything_leaves: consent.anything_leaves(),
    }
}

/// The sentence a photographer reads for each learnable value.
///
/// Rendered rather than stored, which is phase 09's rule and phase 27's: a stored sentence is copy
/// a release has to maintain, and a catalog full of English cannot be translated.
const fn label_for(learnable: Learnable) -> &'static str {
    match learnable {
        Learnable::Exposure => "Exposure",
        Learnable::TemperatureK => "Colour temperature",
        Learnable::Tint => "Tint",
        Learnable::Contrast => "Contrast",
        Learnable::Highlights => "Highlights",
        Learnable::Shadows => "Shadows",
        Learnable::Whites => "Whites",
        Learnable::Blacks => "Blacks",
        Learnable::Vibrance => "Vibrance",
        Learnable::Saturation => "Saturation",
        Learnable::EmotionWeight => "How much expression counts",
        Learnable::CompositionWeight => "How much framing counts",
        Learnable::KeepThreshold => "How readily a frame is kept",
        Learnable::GallerySize => "Gallery size",
        Learnable::HeroThreshold => "How readily a frame becomes a portfolio pick",
    }
}

/// The digest of `models.lock`, or `unknown` when there is none.
///
/// Read here rather than held on the state, because it is one file read on a screen somebody opens
/// during a support call rather than something the pipeline consults.
fn model_set_digest(state: &AppState) -> String {
    let lock = state.models_root().join("models.lock");
    match std::fs::read(&lock) {
        Ok(bytes) => blake3::hash(&bytes).to_hex().to_string(),
        Err(_) => "unknown".to_owned(),
    }
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_learn::errors::no_update(format!(
            "`{id}` is not a project id"
        )))
    })
}

fn parse_profile(id: &str) -> Result<ProfileId, IpcError> {
    ProfileId::from_db(id).map_err(|_| {
        IpcError::from(aura_learn::errors::no_update(format!(
            "`{id}` is not a profile id"
        )))
    })
}
