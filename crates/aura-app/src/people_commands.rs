//! The people half of the command surface.
//!
//! Eleven commands, and the same three rules the preview, hardware, cloud and
//! similarity commands follow, plus one this half adds.
//!
//! **Nothing here blocks for long, except the two commands that say they do.**
//! `people_status`, `list_identities`, `image_subjects`, `identity_timelines`,
//! `identity_cover` and the five edit commands read the catalog or one sealed blob.
//! `scan_faces` is a batch pass and `group_people` is a clustering pass; the UI calls
//! both from a job rather than from a click handler, and both are resumable so a cancel
//! costs nothing.
//!
//! **The panel tells the truth about what is missing.** An empty People panel on an
//! unscanned project is a support ticket, so `PeopleStatusDto` carries coverage, the
//! stale version pairs, and whether the project's biometric data has been erased -
//! rather than making the UI infer any of it from silence.
//!
//! **No template crosses the boundary.** There is no command that returns a
//! recognition template. Every comparison happens in Rust.
//!
//! **A destructive command needs the photographer to type the thing it destroys.**
//! `erase_biometrics` refuses unless `confirm` equals `project_id`. It is the one
//! operation in this product with no undo, and a single argument that is also the
//! target is one mis-click from a promise nobody can take back.

use aura_core::contract::people::{PeopleService, Role};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{FaceId, IdentityId, PhotoId, Priority};
use aura_people::store::FaceRow;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    CompanionDto, CoverageGapDto, EraseBiometricsDto, EraseBiometricsInput, FaceBoxDto,
    FaceCropDto, GroupPeopleDto, GroupPeopleInput, IdentityCardDto, IdentityHandleDto,
    IdentityTimelineDto, ImageSubjectsDto, IpcError, MergeIdentitiesInput, PeopleStatusDto,
    ProminenceEntryDto, RenameIdentityInput, ScanFacesDto, ScanFacesInput,
    SetIdentityImportanceInput, SetIdentityRoleInput, SplitIdentityInput,
};
use crate::state::AppState;

/// What the People panel opens with.
///
/// # Errors
///
/// `AURA-DB-3006` when the coverage view or the identities cannot be read, and
/// `AURA-SEC-9001` when the project's biometric key is unavailable - which is the
/// honest answer for a catalog that was copied without its keychain entry.
pub fn people_status(state: &AppState, project_id: &str) -> IpcResult<PeopleStatusDto> {
    let project = parse_project(project_id)?;
    let store = state.people_store();
    let (photos, scanned, faces, voting) = store.coverage(&project)?;
    let identities = store.load_identities(&project)?;
    let versions = store.face_versions(&project)?;
    let current = (
        aura_vision::face::detect::MODEL_VER,
        aura_vision::face::embed::EMBED_VER,
    );

    Ok(PeopleStatusDto {
        photos: u32::try_from(photos).unwrap_or(u32::MAX),
        scanned: u32::try_from(scanned).unwrap_or(u32::MAX),
        // Per-mille integer arithmetic, then one lossless widening - the same shape the
        // index status uses, for the same reason: a direct cast from `i64` to a float is
        // a precision loss the compiler is right to object to even when the numbers are
        // small, and the next reader should not have to check that they are.
        coverage: coverage_fraction(scanned, photos),
        faces: u32::try_from(faces).unwrap_or(u32::MAX),
        voting_faces: u32::try_from(voting).unwrap_or(u32::MAX),
        identities: u32::try_from(identities.len()).unwrap_or(u32::MAX),
        tiled_frames: u32::try_from(state.people_tiled_frames(&project)?).unwrap_or(u32::MAX),
        couple_unconfirmed: !identities
            .iter()
            .any(|identity| identity.user_locked && identity.role.is_couple()),
        stale_versions: versions
            .iter()
            .filter(|(detector, recogniser, _)| (*detector, *recogniser) != current)
            .map(|(detector, recogniser, _)| u32::from(*detector) * 1_000 + u32::from(*recogniser))
            .collect(),
        erased: state.people_erased(&project)?,
        key_store: state.key_store_name().to_string(),
        weights_ver: u32::from(state.people().weights().version),
    })
}

/// Every identity in a project, largest first.
///
/// # Errors
///
/// `AURA-DB-3006` or `AURA-SEC-9001`.
pub fn list_identities(state: &AppState, project_id: &str) -> IpcResult<Vec<IdentityCardDto>> {
    let project = parse_project(project_id)?;
    let store = state.people_store();
    let identities = store.load_identities(&project)?;
    let faces = store.load_faces(&project)?;
    let edges = store.load_cooccurrence(&project)?;
    let graph = aura_people::CooccurrenceGraph::from_rows(&edges);

    let labels: std::collections::BTreeMap<IdentityId, Option<String>> = identities
        .iter()
        .map(|identity| (identity.identity_id, identity.label.clone()))
        .collect();

    let mut out = Vec::with_capacity(identities.len());
    for identity in &identities {
        let members: Vec<&FaceRow> = faces
            .iter()
            .filter(|face| face.identity_id == Some(identity.identity_id))
            .collect();
        // A divisor that is never zero, widened through `u16` so no cast can lose a
        // bit: a wedding does not have sixty-five thousand faces of one person, and a
        // count that did would be a bug worth clamping rather than averaging.
        let count = f32::from(u16::try_from(members.len().max(1)).unwrap_or(u16::MAX));

        // The cover face is the best one, by quality then by area. A large soft face
        // makes a worse card than a smaller sharp one.
        let cover = members
            .iter()
            .max_by(|left, right| {
                left.quality
                    .partial_cmp(&right.quality)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left.area_frac
                            .partial_cmp(&right.area_frac)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| right.face_id.cmp(&left.face_id))
            })
            .map(|face| face.face_id.to_db());

        out.push(IdentityCardDto {
            id: identity.identity_id.to_db(),
            label: identity.label.clone(),
            role: identity.role.as_str().to_string(),
            role_confidence: identity.role_confidence,
            role_reasons: identity.role_reasons.clone(),
            user_locked: identity.user_locked,
            importance: identity.importance,
            faces: u32::try_from(members.len()).unwrap_or(u32::MAX),
            voting_faces: u32::try_from(members.iter().filter(|face| face.votes).count())
                .unwrap_or(u32::MAX),
            frames: u32::try_from(
                members
                    .iter()
                    .map(|face| face.image_id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
            )
            .unwrap_or(u32::MAX),
            first_seen: identity.first_seen.clone(),
            last_seen: identity.last_seen.clone(),
            mean_quality: members.iter().map(|face| face.quality).sum::<f32>() / count,
            cover_face_id: cover,
            variance: identity.variance,
            sub_count: u32::try_from(identity.sub_centroids.len()).unwrap_or(u32::MAX),
            companions: graph
                .companions(identity.identity_id)
                .into_iter()
                .take(8)
                .map(|(other, frames)| CompanionDto {
                    id: other.to_db(),
                    label: labels.get(&other).cloned().flatten(),
                    frames,
                })
                .collect(),
        });
    }
    Ok(out)
}

/// Who is in one photograph, and how much of it is about them.
///
/// # Errors
///
/// `AURA-DB-3006` or `AURA-SEC-9001`.
pub fn image_subjects(state: &AppState, photo_id: &str) -> IpcResult<ImageSubjectsDto> {
    let photo = parse_photo(photo_id)?;
    let people = state.people();
    let subjects = people.subjects(photo)?;

    // The face boxes come from the store rather than from `subjects`, because
    // `ImageSubjects::faces` is deliberately a subject *reference* - it carries no pose,
    // no reasons and no provenance, so that a phase which ranks subjects cannot hold
    // biometric detail it has no use for. The panel needs the detail, so it asks.
    let store = state.people_store();
    let boxes = match store.project_of_photo(photo)? {
        Some(project) => store
            .faces_of_image(&project, photo)?
            .iter()
            .map(face_box)
            .collect(),
        None => Vec::new(),
    };

    Ok(ImageSubjectsDto {
        photo_id: photo_id.to_string(),
        faces: boxes,
        dominant: subjects.dominant.map(|id| id.to_db()),
        prominence: subjects
            .prominence
            .iter()
            .map(|(identity, value)| ProminenceEntryDto {
                identity_id: identity.to_db(),
                prominence: *value,
            })
            .collect(),
        subject_focus_score: subjects.subject_focus_score,
        people_count: subjects.people_count,
        weights_ver: u32::from(subjects.weights_ver),
    })
}

/// Scan every photograph in a project that has not been looked at.
///
/// Resumable by construction: what is left to do is a query, so a cancelled pass and a
/// killed process leave the same state behind and the next call continues.
///
/// # Errors
///
/// `AURA-DB-3006` when the pending set cannot be read, `AURA-SEC-9001` when the key is
/// unavailable, and whatever the model loader raised on first use. Per-photograph
/// failures are counted in the result rather than returned.
pub fn scan_faces(state: &AppState, input: &ScanFacesInput) -> IpcResult<ScanFacesDto> {
    let project = parse_project(&input.project_id)?;
    let scanner = state.face_scanner()?;
    let report = scanner.scan_project(
        &project,
        Priority::AiBatch,
        &CancelToken::new(),
        &NullProgress,
    )?;
    let remaining = state
        .people_store()
        .pending(
            &project,
            aura_vision::face::detect::MODEL_VER,
            aura_vision::face::detect::PREPROCESS_VER,
        )?
        .len();

    Ok(ScanFacesDto {
        scanned: u32::try_from(report.scanned).unwrap_or(u32::MAX),
        faces: u32::try_from(report.faces).unwrap_or(u32::MAX),
        voting: u32::try_from(report.voting).unwrap_or(u32::MAX),
        gated: u32::try_from(report.gated).unwrap_or(u32::MAX),
        person_boxes: u32::try_from(report.person_boxes).unwrap_or(u32::MAX),
        headless: u32::try_from(report.headless).unwrap_or(u32::MAX),
        failed: u32::try_from(report.failed).unwrap_or(u32::MAX),
        remaining: u32::try_from(remaining).unwrap_or(u32::MAX),
        tiled_frames: u32::try_from(report.tiled_frames).unwrap_or(u32::MAX),
        // One lossless narrowing of a value that is a ratio in 0..1 by construction.
        #[allow(clippy::cast_possible_truncation)]
        tile_ratio: report.tile_ratio() as f32,
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

/// Group a project's faces into identities, infer roles, and report.
///
/// # Errors
///
/// `AURA-DB-3006`, `AURA-SEC-9001`, or `AURA-ML-5019` when there are too few voting
/// faces to group anybody - which is reported in the result's `identities: 0` and its
/// coverage rather than as a failure.
pub fn group_people(state: &AppState, input: &GroupPeopleInput) -> IpcResult<GroupPeopleDto> {
    let project = parse_project(&input.project_id)?;
    let report = state.people().regroup(&project)?;

    Ok(GroupPeopleDto {
        identities: u32::try_from(report.identities).unwrap_or(u32::MAX),
        assigned: u32::try_from(report.assigned).unwrap_or(u32::MAX),
        unassigned: u32::try_from(report.unassigned).unwrap_or(u32::MAX),
        threshold: report.threshold,
        refused_merges: u32::try_from(report.refused_merges).unwrap_or(u32::MAX),
        sub_clustered: u32::try_from(report.sub_clustered).unwrap_or(u32::MAX),
        couple: report
            .couple
            .map(|(a, b)| vec![a.to_db(), b.to_db()])
            .unwrap_or_default(),
        couple_confidence: report.couple_confidence,
        couple_ambiguous: report.couple_ambiguous,
        scene_starved: report.scene_starved,
        decisions_replayed: u32::try_from(report.decisions_replayed).unwrap_or(u32::MAX),
        decisions_orphaned: u32::try_from(report.decisions_orphaned).unwrap_or(u32::MAX),
        coverage: report.coverage,
        elapsed_ms: report.elapsed_ms,
    })
}

/// Fold one identity into another.
///
/// # Errors
///
/// `AURA-SEC-9005` when either id is unknown or the merge is refused, and
/// `AURA-SEC-9004` when the two belong to different projects.
pub fn merge_identities(
    state: &AppState,
    input: &MergeIdentitiesInput,
) -> IpcResult<IdentityHandleDto> {
    let a = parse_identity(&input.a)?;
    let b = parse_identity(&input.b)?;
    let survivor = state.people().merge(a, b)?;
    Ok(IdentityHandleDto {
        id: survivor.to_db(),
    })
}

/// Move faces out of an identity into a new one.
///
/// # Errors
///
/// `AURA-SEC-9005` when the identity is unknown, when a face does not belong to it, or
/// when the split would leave nothing behind.
pub fn split_identity(
    state: &AppState,
    input: &SplitIdentityInput,
) -> IpcResult<IdentityHandleDto> {
    let identity = parse_identity(&input.identity_id)?;
    let faces: Vec<FaceId> = input
        .face_ids
        .iter()
        .filter_map(|text| FaceId::from_db(text).ok())
        .collect();
    if faces.len() != input.face_ids.len() {
        return Err(IpcError::from(aura_people::errors::edit_refused(
            "split",
            "one of the face ids was not a valid AURA id",
        )));
    }
    let created = state.people().split(identity, &faces)?;
    Ok(IdentityHandleDto {
        id: created.to_db(),
    })
}

/// Set a role, and lock it against automation.
///
/// This is the "this is the bride" button. It sets `user_locked`, so no re-analysis,
/// cloud hint or model update may overwrite it.
///
/// # Errors
///
/// `AURA-SEC-9005` when the identity is unknown or the role is not one this build knows.
pub fn set_identity_role(state: &AppState, input: &SetIdentityRoleInput) -> IpcResult<()> {
    let identity = parse_identity(&input.identity_id)?;
    let role = Role::from_str_or_unknown(&input.role);
    // An unrecognised role string would silently become `unknown`, which for a *locked*
    // decision is worse than a refusal: the photographer would have told the product
    // something and the product would have recorded the opposite.
    if role == Role::Unknown && input.role != Role::Unknown.as_str() {
        return Err(IpcError::from(aura_people::errors::edit_refused(
            "role",
            &format!("\"{}\" is not a role this build knows", input.role),
        )));
    }
    state.people().set_role(identity, role)?;
    Ok(())
}

/// Rename an identity.
///
/// # Errors
///
/// `AURA-SEC-9005` when the identity is unknown.
pub fn rename_identity(state: &AppState, input: &RenameIdentityInput) -> IpcResult<()> {
    let identity = parse_identity(&input.identity_id)?;
    state.people().rename(identity, input.label.as_deref())?;
    Ok(())
}

/// Move the importance slider.
///
/// # Errors
///
/// `AURA-SEC-9005` when the identity is unknown.
pub fn set_identity_importance(
    state: &AppState,
    input: &SetIdentityImportanceInput,
) -> IpcResult<()> {
    let identity = parse_identity(&input.identity_id)?;
    state.people().set_importance(identity, input.importance)?;
    Ok(())
}

/// Every identity's timeline, for the person timeline view.
///
/// # Errors
///
/// `AURA-DB-3006` or `AURA-SEC-9001`.
pub fn identity_timelines(
    state: &AppState,
    project_id: &str,
) -> IpcResult<Vec<IdentityTimelineDto>> {
    let project = parse_project(project_id)?;
    Ok(state
        .people()
        .timelines(&project)?
        .into_iter()
        .map(|timeline| IdentityTimelineDto {
            identity_id: timeline.identity.to_db(),
            first_ms: timeline.first_ms,
            last_ms: timeline.last_ms,
            span_minutes: timeline.span_minutes(),
            frames: timeline.frames,
            gaps: timeline
                .gaps
                .iter()
                .map(|gap| CoverageGapDto {
                    from_ms: gap.from_ms,
                    to_ms: gap.to_ms,
                    minutes: gap.minutes(),
                })
                .collect(),
        })
        .collect())
}

/// One sealed face crop, unsealed for display.
///
/// The only route from the sealed store to a screen. It unseals exactly one crop and
/// hands the JPEG straight through: the crop was encoded before it was sealed, so there
/// is nothing to decode and re-encode here.
///
/// # Errors
///
/// `AURA-SEC-9001` when the key is unavailable and `AURA-SEC-9002` when the crop file is
/// missing or does not authenticate.
pub fn identity_cover(state: &AppState, project_id: &str, face_id: &str) -> IpcResult<FaceCropDto> {
    let project = parse_project(project_id)?;
    let face = FaceId::from_db(face_id).map_err(|_| {
        IpcError::from(aura_people::errors::edit_refused(
            "face lookup",
            "face id was not a valid AURA id",
        ))
    })?;
    let jpeg = state.people_store().open_crop(&project, face)?;

    Ok(FaceCropDto {
        face_id: face_id.to_string(),
        data_url: format!("data:image/jpeg;base64,{}", crate::base64(&jpeg)),
    })
}

/// Erase a project's biometric data.
///
/// Deletes the credential-store entry first - so a crash mid-erasure leaves unreadable
/// data rather than readable data - then the sealed crops, then the rows, then verifies
/// that nothing survived. **Culling decisions, edits and exports are untouched**, which
/// is section 6.5's requirement.
///
/// # Errors
///
/// `AURA-SEC-9005` when `confirm` does not match, and `AURA-SEC-9003` when anything was
/// left behind.
pub fn erase_biometrics(
    state: &AppState,
    input: &EraseBiometricsInput,
) -> IpcResult<EraseBiometricsDto> {
    if input.confirm != input.project_id {
        return Err(IpcError::from(aura_people::errors::edit_refused(
            "erase biometric data",
            "the confirmation did not match the project id; nothing was deleted",
        )));
    }
    let project = parse_project(&input.project_id)?;
    let report = state.people().erase(&project)?;

    Ok(EraseBiometricsDto {
        faces: u32::try_from(report.faces).unwrap_or(u32::MAX),
        identities: u32::try_from(report.identities).unwrap_or(u32::MAX),
        crops: u32::try_from(report.crops).unwrap_or(u32::MAX),
        key_removed: report.key_removed,
    })
}

/// One face row as the wire shape.
fn face_box(face: &FaceRow) -> FaceBoxDto {
    FaceBoxDto {
        face_id: face.face_id.to_db(),
        identity_id: face.identity_id.map(|id| id.to_db()),
        x: face.bbox.x,
        y: face.bbox.y,
        w: face.bbox.w,
        h: face.bbox.h,
        det_score: face.det_score,
        quality: face.quality,
        blur: face.blur,
        occlusion: face.occlusion,
        yaw: face.pose.yaw,
        pitch: face.pose.pitch,
        roll: face.pose.roll,
        px_height: face.px_height,
        votes: face.votes,
        found_by: face.found_by.as_str().to_string(),
        // Re-derived rather than stored. The quality verdict's reasons are a function of
        // the stored numbers and the gate version, so keeping a second copy in a text
        // column would be a cache that could disagree with the numbers beside it.
        reasons: quality_reasons(face),
    }
}

/// The reasons a stored face does or does not vote, from its stored numbers.
fn quality_reasons(face: &FaceRow) -> Vec<String> {
    let mut reasons = Vec::new();
    if face.votes {
        reasons.push(format!(
            "{} px tall and {:.2} usable; votes on identity",
            face.px_height, face.quality
        ));
    } else {
        if face.px_height < aura_vision::face::quality::MIN_PX_HEIGHT {
            reasons.push(format!(
                "{} px tall, below the {} px identity gate",
                face.px_height,
                aura_vision::face::quality::MIN_PX_HEIGHT
            ));
        }
        if face.quality < aura_vision::face::quality::MIN_USABLE {
            reasons.push(format!(
                "usability {:.2} is below the {:.2} identity gate",
                face.quality,
                aura_vision::face::quality::MIN_USABLE
            ));
        }
        if reasons.is_empty() {
            reasons.push(
                "excluded from identity voting; the recogniser returned no usable template"
                    .to_string(),
            );
        }
    }
    if face.blur > 0.65 {
        reasons.push(format!("soft: blur {:.2}", face.blur));
    }
    if face.occlusion > 0.25 {
        reasons.push(format!(
            "{:.0}% of the crop has no facial structure in it",
            face.occlusion * 100.0
        ));
    }
    if face.pose.yaw.abs() > 40.0 {
        reasons.push(format!("turned away: yaw {:.0} degrees", face.pose.yaw));
    }
    reasons
}

/// Scanned over total, as a fraction, without a lossy cast.
fn coverage_fraction(scanned: i64, photos: i64) -> f32 {
    if photos <= 0 {
        return 0.0;
    }
    let per_mille = (scanned.saturating_mul(1_000) / photos).clamp(0, 1_000);
    f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
}

/// Parse a project id, or refuse with a coded error rather than a panic.
#[allow(clippy::doc_markdown)]
fn parse_project(text: &str) -> IpcResult<aura_core::ProjectId> {
    aura_core::ProjectId::from_db(text).map_err(|_| {
        IpcError::from(aura_core::errors::db::statement_failed(
            "project id was not a valid AURA id",
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })
}

/// Parse a photograph id.
fn parse_photo(text: &str) -> IpcResult<PhotoId> {
    PhotoId::from_db(text).map_err(|_| {
        IpcError::from(aura_core::errors::db::statement_failed(
            "photo id was not a valid AURA id",
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })
}

/// Parse an identity id.
fn parse_identity(text: &str) -> IpcResult<IdentityId> {
    IdentityId::from_db(text).map_err(|_| {
        IpcError::from(aura_people::errors::edit_refused(
            "identity lookup",
            "identity id was not a valid AURA id",
        ))
    })
}
