//! The distraction-cleanup command surface. PHASE-24.
//!
//! Eight commands. Four read - the project coverage, one photograph's proposals, one photograph's
//! refusals and the delivery report - one runs the resumable pass, and three record what the
//! photographer decided. ADR-0050 records the shape and what is deliberately absent from it.
//!
//! # What this module does that no earlier command surface does
//!
//! **It publishes the refusals.** `cleanup_blocked` exists on the frozen service and on this
//! surface because more than half of `CleanupCode` is refusals, section 10.1's adversarial audit is
//! scored from them, and teaching a photographer what AURA will never do is most of the trust this
//! feature needs. Every earlier phase published what it *did*; this one has to publish what it
//! declined.
//!
//! **Accepting is not applying, and applying is not on this surface at all.** `decide_cleanup`
//! marks a proposal accepted. What turns an accepted proposal into replaced pixels is
//! `CleanupStore::apply`, which writes the disclosure and the applied flag in one transaction with
//! a trigger that aborts the second half if the first is missing - and **no command here calls
//! it**, because nothing in this build produces a proposal to apply. See conditions C1 and C2 of
//! the exit report.
//!
//! The separation is deliberate rather than incidental. A single command that accepted and applied
//! together invites one specific failure: the panel marks the proposal accepted, the render fails,
//! and the catalog now carries a disclosure saying a removal happened to a photograph that still
//! has the bin in it. A disclosure that is not true is worse than no feature.
//!
//! **The manual tool runs the whole safety engine.** `manual_remove` takes a rectangle a person
//! drew and puts it through all five checks in order. A person choosing a region is a reason to
//! skip the *detector*, not a reason to skip the filter - and the one thing it can never do,
//! whatever anybody confirms, is remove a person.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! No strength, no size, no prompt, no pixels, and no way to raise a cap. See the header of the
//! DTO block in `contract::ipc`.

use aura_core::contract::cleanup::{
    CleanupCode, CleanupMethod, CleanupOverride, CleanupProposal, CleanupService, DistractionClass,
    SafetyCheck,
};
use aura_core::contract::ids::ProposalId;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{PhotoId, Priority, ProjectId};
use aura_generative::denylist::Coverage;
use aura_generative::queue::{self, Context};
use aura_generative::safety::Candidate;
use aura_generative::source::Sources;
use aura_generative::{api, Policy};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    CleanupBlockedDto, CleanupDisclosureDto, CleanupPassDto, CleanupPassInput, CleanupProposalDto,
    CleanupReasonDto, CleanupStatusDto, CropRectDto, DecideCleanupInput, DisableCleanupInput,
    IpcError, ManualRemoveDto, ManualRemoveInput,
};
use crate::state::AppState;

/// What the Cleanup panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored rows cannot be read, and `AURA-ML-5119` when the policy table
/// will not load.
pub fn cleanup_status(state: &AppState, project_id: &str) -> IpcResult<CleanupStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.cleanup()?.outline(project)?;
    Ok(CleanupStatusDto {
        photos: outline.photos,
        examined: outline.examined,
        coverage: outline.coverage,
        with_proposals: outline.with_proposals,
        applied: outline.applied,
        blocked: outline.blocked.to_vec(),
        check_names: SafetyCheck::ALL
            .iter()
            .map(|check| check.as_str().to_string())
            .collect(),
        borrowed: outline.borrowed,
        filled: outline.filled,
        inpainted: outline.inpainted,
        reverted: outline.reverted,
        mask_covered: outline.mask_covered,
        // Both false in this build, and on the wire rather than inferred: a panel that had to
        // guess would eventually render "no distractions found" for a build that cannot look.
        detector_trained: aura_generative::DISTRACTION_HEAD_TRAINED,
        inpaint_available: aura_generative::INPAINT_PACK_INSTALLED,
    })
}

/// Every proposal on one photograph, strongest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn image_cleanup(state: &AppState, photo_id: &str) -> IpcResult<Vec<CleanupProposalDto>> {
    let photo = parse_photo(photo_id)?;
    let proposals = state.cleanup()?.proposals(photo)?;
    Ok(proposals.iter().map(to_dto).collect())
}

/// Every candidate the safety engine refused on one photograph.
///
/// **A separate call rather than a field.** See the module header.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn cleanup_blocked(state: &AppState, photo_id: &str) -> IpcResult<Vec<CleanupBlockedDto>> {
    let photo = parse_photo(photo_id)?;
    let blocked = state.cleanup()?.blocked(photo)?;
    Ok(blocked
        .into_iter()
        .map(|(region, check, code)| CleanupBlockedDto {
            region: to_rect(&region),
            check: check.as_str().to_string(),
            code: code.as_str().to_string(),
            text: code.user_text().to_string(),
        })
        .collect())
}

/// Everything removed from one project, for the delivery report.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn cleanup_disclosures(
    state: &AppState,
    project_id: &str,
) -> IpcResult<Vec<CleanupDisclosureDto>> {
    let project = parse_project(project_id)?;
    let rows = state.cleanup()?.disclosures(project)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let (method, borrowed_from, model) = method_parts(&row.method);
            CleanupDisclosureDto {
                proposal_id: row.proposal_id.to_db(),
                photo_id: row.image_id.to_db(),
                method,
                borrowed_from,
                model,
                region: to_rect(&row.region),
                accepted_by_user: row.accepted_by_user,
                artefact_score: row.artefact_score,
            }
        })
        .collect())
}

/// Run the resumable pass.
///
/// # Errors
///
/// `AURA-ML-5119` when the policy table will not load, `AURA-DB-3006` when the pending set cannot
/// be read. Per-photograph failures are counted in the report rather than returned.
pub fn cleanup_pass(state: &AppState, input: CleanupPassInput) -> IpcResult<CleanupPassDto> {
    let project = parse_project(&input.project_id)?;
    let pass = state.cleanup_pass(&input.project_id)?;
    let cancel = CancelToken::new();
    let progress = NullProgress;

    let report = if input.photo_ids.is_empty() {
        pass.run(&project, Priority::Background, &cancel, &progress)?
    } else {
        let ids = input
            .photo_ids
            .iter()
            .map(|id| parse_photo(id))
            .collect::<Result<Vec<_>, _>>()?;
        pass.run_ids(&project, &ids, Priority::Background, &cancel, &progress)?
    };

    Ok(CleanupPassDto {
        examined: report.examined,
        with_proposals: report.with_proposals,
        proposals: report.proposals,
        blocked: report.blocked.to_vec(),
        reverted: report.reverted,
        judged: report.judged,
        declined: report.declined,
        failed: report.failed,
        cancelled: report.cancelled,
        elapsed_ms: report.elapsed_ms,
    })
}

/// Accept or reject one proposal.
///
/// **Accepting does not apply it.** See the module header.
///
/// # Errors
///
/// `AURA-ML-5117` when the proposal is not on this photograph.
pub fn decide_cleanup(state: &AppState, input: DecideCleanupInput) -> IpcResult<()> {
    let photo = parse_photo(&input.photo_id)?;
    let proposal = ProposalId::from_db(&input.proposal_id)
        .map_err(|_| refused(format!("`{}` is not a proposal id", input.proposal_id)))?;
    state.cleanup()?.decide(
        photo,
        &CleanupOverride {
            proposal_id: Some(proposal),
            accept: Some(input.accept),
            disable_for_image: None,
        },
    )?;
    Ok(())
}

/// Switch cleanup off, or back on, for one photograph.
///
/// A photograph switched off is excluded from the pending set, so a re-analysis does not spend
/// time re-proposing removals somebody has already said they do not want.
///
/// # Errors
///
/// `AURA-DB-3006` when the write fails.
pub fn disable_cleanup(state: &AppState, input: DisableCleanupInput) -> IpcResult<()> {
    let photo = parse_photo(&input.photo_id)?;
    state.cleanup()?.decide(
        photo,
        &CleanupOverride {
            proposal_id: None,
            accept: None,
            disable_for_image: Some(input.disabled),
        },
    )?;
    Ok(())
}

/// Remove one region a photographer drew, by hand.
///
/// Section 2.2's manual tool. It runs the whole safety engine on the region, and the one thing it
/// can never do - whatever anybody confirms - is remove a person: the class is `Unclassified`,
/// which `story_safe` refuses, so a manual removal that would take out a guest comes back as a
/// refusal naming the check.
///
/// # Errors
///
/// `AURA-ML-5118` when the photograph's proxy cannot be read, and a bad-request error when
/// `confirmed` is false. The refusal cases are **not** errors: they come back in
/// [`ManualRemoveDto::blocked`], because a refusal is a result rather than a failure.
pub fn manual_remove(state: &AppState, input: ManualRemoveInput) -> IpcResult<ManualRemoveDto> {
    if !input.confirmed {
        return Err(refused(
            "a manual removal needs explicit confirmation before AURA will touch a photograph",
        ));
    }
    let photo = parse_photo(&input.photo_id)?;
    let project = project_of(state, photo)?;
    let region = from_rect(&input.region);

    let policy = Policy::shipped()?;
    let scene = state
        .story()
        .ok()
        .and_then(|story| {
            use aura_core::contract::scene::StoryService;
            story.scene(photo).ok().flatten()
        })
        .map_or(aura_core::SceneId::Unknown, |result| result.scene);
    let Some(row) = policy.scene(scene) else {
        return Ok(ManualRemoveDto {
            proposal: None,
            blocked: Some(CleanupBlockedDto {
                region: input.region,
                check: SafetyCheck::SizeCap.as_str().to_string(),
                code: CleanupCode::ProposalCapReached.as_str().to_string(),
                text: "AURA has no tidying guidance recorded for this kind of photograph yet"
                    .to_string(),
            }),
        });
    };

    // The pixels. A manual removal is the one path that renders on request rather than in a
    // background pass, so it reads the same proxy the pass does - invariant 3, and the same level
    // means the same rectangle resolves to the same samples.
    // Through the frozen trait rather than through the concrete service, which is what makes the
    // preview cache the only route to pixels here as it is everywhere else.
    let previews: std::sync::Arc<dyn aura_preview::contract::service::PreviewService> =
        state.previews(&project.to_db())?;
    let buffer = previews
        .get(
            photo,
            api::level(),
            aura_preview::contract::service::Priority::Interactive,
        )
        .map_err(|err| IpcError::from(aura_generative::errors::item_failed(err.detail)))?;
    let Some(target) = api::to_image(&buffer) else {
        return Err(IpcError::from(aura_generative::errors::item_failed(
            "the proxy carries tiles, which this path never asks for",
        )));
    };

    // The candidate, built here rather than taken from `aura_generative::fixtures`: that module is
    // test support, and a change to a fixture must never change what the manual tool does.
    //
    // The class is `Unclassified`, because a person drawing a rectangle has not told AURA what is
    // inside it - and `Unclassified` is refused at the confidence check, which is why a manual
    // removal in this build comes back as a refusal that names the missing detector rather than as
    // a removal AURA could not justify.
    //
    // `salience` and `removability` are both one. A person asking for this region *is* the
    // evidence; there is no detector reading to soften. Everything that can still stop it - the
    // size cap, the denylist, the identity check and the structure check - is a property of the
    // photograph rather than of how sure anybody is.
    let candidate = Candidate {
        region,
        class: DistractionClass::Unclassified,
        salience: 1.0,
        removability: 1.0,
        // Not measured for a hand-drawn region: the structure check reads the frame's own
        // gradients and this path has not run the detector that fills the flag. False is the
        // permissive value, which is safe here only because every other check still runs - and it
        // is the one place on this surface where a person's choice does relax something.
        crosses_structure: false,
        touches_identity: false,
    };
    let coverage = Coverage::Absent;
    let context = Context {
        image: photo,
        scene,
        policy: row,
        coverage: &coverage,
        sources: Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        },
        detector_ver: aura_generative::DETECTOR_VER,
        analysis_ver: aura_generative::ANALYSIS_VER,
        policy_ver: policy.version,
        calibrated: false,
    };

    let plan = queue::plan(&context, &[candidate], None);
    if let Some(prepared) = plan.prepared.first() {
        state.cleanup_store().put(
            &project,
            photo,
            scene,
            &plan,
            (
                aura_generative::DETECTOR_VER,
                aura_generative::ANALYSIS_VER,
                policy.version,
            ),
        )?;
        return Ok(ManualRemoveDto {
            proposal: Some(to_dto(&prepared.proposal)),
            blocked: None,
        });
    }

    Ok(ManualRemoveDto {
        proposal: None,
        blocked: plan.blocked.first().map(|block| CleanupBlockedDto {
            region: to_rect(&block.region),
            check: block.check.as_str().to_string(),
            code: block.code.as_str().to_string(),
            text: block.code.user_text().to_string(),
        }),
    })
}

/// The reason codes this build can emit, for the panel's legend and the Explain surface.
///
/// A generated list rather than a typed one, so a code added to the contract appears here without
/// anybody editing a second table. Phase 13's registry rule, applied to one phase's vocabulary.
///
/// # Errors
///
/// Never; the signature matches the rest of the surface.
pub fn cleanup_reason_codes(_state: &AppState) -> IpcResult<Vec<CleanupReasonDto>> {
    Ok(CleanupCode::ALL
        .into_iter()
        .map(|code| CleanupReasonDto {
            code: code.as_str().to_string(),
            text: code.user_text().to_string(),
            weight: 0.0,
            is_refusal: code.is_refusal(),
            evidence: None,
        })
        .collect())
}

// -------------------------------------------------------------------------------------------
// Conversions.
// -------------------------------------------------------------------------------------------

fn to_dto(proposal: &CleanupProposal) -> CleanupProposalDto {
    let (method, borrowed_from, model) = method_parts(&proposal.method);
    CleanupProposalDto {
        proposal_id: proposal.id.to_db(),
        photo_id: proposal.image_id.to_db(),
        region: to_rect(&proposal.region),
        class: proposal.class.as_str().to_string(),
        class_text: proposal.class.user_text().to_string(),
        area_frac: proposal.area_frac,
        salience: proposal.salience,
        method,
        borrowed_from,
        model,
        confidence: proposal.confidence,
        artefact_score: 0.0,
        autonomy: proposal.autonomy.as_str().to_string(),
        scene: proposal.scene.as_str().to_string(),
        reasons: proposal
            .reasons
            .iter()
            .map(|reason| CleanupReasonDto {
                code: reason.code.as_str().to_string(),
                text: reason.code.user_text().to_string(),
                weight: reason.weight,
                is_refusal: reason.code.is_refusal(),
                evidence: reason.evidence.as_ref().map(to_rect),
            })
            .collect(),
        accepted: None,
        applied: false,
        may_apply_unattended: proposal.may_apply_unattended(),
        versions: vec![
            proposal.detector_ver,
            proposal.analysis_ver,
            proposal.policy_ver,
        ],
    }
}

fn method_parts(method: &CleanupMethod) -> (String, Option<String>, Option<String>) {
    match method {
        CleanupMethod::BorrowFrom(source) => ("borrow".to_string(), Some(source.to_db()), None),
        CleanupMethod::ClassicalFill => ("fill".to_string(), None, None),
        CleanupMethod::Inpaint { model } => ("inpaint".to_string(), None, Some(model.clone())),
    }
}

fn to_rect(region: &aura_core::contract::cleanup::Box2) -> CropRectDto {
    CropRectDto {
        x: region.x,
        y: region.y,
        w: region.w,
        h: region.h,
    }
}

fn from_rect(rect: &CropRectDto) -> aura_core::contract::cleanup::Box2 {
    aura_core::contract::cleanup::Box2 {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

fn refused(detail: impl Into<String>) -> IpcError {
    IpcError::from(aura_core::errors::ml::cleanup_override_refused(detail))
}

fn parse_project(text: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(text).map_err(|_| refused(format!("`{text}` is not a project id")))
}

fn parse_photo(text: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(text).map_err(|_| refused(format!("`{text}` is not a photograph id")))
}

fn project_of(state: &AppState, photo: PhotoId) -> IpcResult<ProjectId> {
    state.project_of(photo).map_err(IpcError::from)
}
