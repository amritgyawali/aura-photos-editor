//! The style command surface. PHASE-17.
//!
//! Ten commands: four read - the project's status, the profile list, one profile's honest
//! report and the pairs behind it - two act on an archive, two on a profile's lifecycle, and
//! two move a profile between machines.
//!
//! # The three things this module cannot do, and could not be made to
//!
//! **It cannot return imagery.** Every shape on this surface carries names, numbers and
//! verdicts. `StylePairDto` names two files and says what happened to them; there is no field
//! anywhere that could hold a pixel. Section 13's "never uploads imagery" and section 9's SEC
//! task are therefore properties of fourteen struct definitions rather than promises about the
//! code that fills them - and `aura-style` depends on no cloud crate, which is the other half.
//!
//! **It cannot adopt on training.** `train_profile` produces a `candidate`; `adopt_profile` is
//! a separate command a photographer invokes after looking at `compare_profiles`. Section
//! 6.3's "adoption is an explicit action", and the alternative would change how every future
//! photograph looks as a side effect of a progress bar finishing.
//!
//! **It cannot write a recipe.** A style reaches the pixels through `ColourPass` and
//! `TonePass`, which reach them through `aura_recipe::schema::merge`. Phase 14's rule, for the
//! fourth phase running, and the reason there is no `apply_style` command here.
//!
//! # What `unchangedSinceSigning` means, once, in the module that produces it
//!
//! It means the document has not changed since it was signed by whoever holds the key inside
//! the bundle. It does **not** mean the bundle came from anybody in particular: there is no key
//! distribution in this product and nothing to check a key against. ADR-0035 decision 8. The
//! field is deliberately not called `verified`, and `ProfileReport.test.tsx` asserts the panel
//! never renders that word.

use std::path::Path;
use std::sync::Arc;

use aura_core::contract::scene::ChapterId;
use aura_core::contract::style::{SceneGroup, StyleBucket, StyleProfile, StyleQuery, StyleService};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ProfileId, ProjectId, SceneId};
use aura_style::api::{TrainPass, ANALYSIS_VER, MIN_PAIRS};
use aura_style::pairs::{self, Archive};
use aura_style::{diagnostics, infer, profile as bundle};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    AdoptProfileInput, CompareProfilesInput, ExportProfileDto, ExportProfileInput,
    ImportProfileDto, ImportProfileInput, IpcError, ProfileReportDto, ScanArchiveDto,
    ScanArchiveInput, SetProjectProfileInput, StyleBucketDto, StyleComparisonDto, StylePairDto,
    StyleProfileDto, StyleReasonDto, StyleStatusDto, TrainProfileDto, TrainProfileInput,
};
use crate::state::AppState;

/// How many leaves the comparison returns by default.
///
/// Twelve, which is the same number `StyleProfile::strength` measures coverage against and is
/// about what a photographer will actually look through before deciding. A comparison of eighty
/// leaves is a comparison nobody reads.
const COMPARE_DEFAULT: u32 = 12;

/// What the style panel's project header shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the profiles cannot be read.
pub fn style_status(state: &AppState, project_id: &str) -> IpcResult<StyleStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.style().outline(project)?;
    Ok(StyleStatusDto {
        profiles: outline.profiles,
        active: outline.active.map(|id| id.to_db()),
        active_name: outline.active_name.clone(),
        active_version: u32::from(outline.active_version),
        trained_pairs: outline.trained_pairs,
        strength: outline.strength,
        overall_de00: outline.overall_de00,
        chapter_overrides: outline.chapter_overrides.clone(),
        bucket_ratio: outline.bucket_ratio(),
        level_counts: outline
            .level_counts
            .iter()
            .map(|(level, count)| (level.clone(), *count))
            .collect(),
        analysis_ver: u32::from(outline.analysis_ver),
    })
}

/// Every profile, newest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the profiles cannot be read.
pub fn list_profiles(state: &AppState) -> IpcResult<Vec<StyleProfileDto>> {
    Ok(state.style().profiles()?.iter().map(profile_dto).collect())
}

/// One profile's honest report.
///
/// `None` means nobody has trained it. That is not an empty report and the panel renders it
/// separately: a photographer who has trained nothing sees the wizard, not a profile that
/// scored zero.
///
/// # Errors
///
/// `AURA-DB-3006` when the profile cannot be read.
pub fn profile_report(state: &AppState, profile_id: &str) -> IpcResult<Option<ProfileReportDto>> {
    let id = parse_profile(profile_id)?;
    let Some(profile) = state.style().profile(id)? else {
        return Ok(None);
    };
    Ok(Some(report_dto(&profile)))
}

/// The pairs behind one profile, accepted and rejected.
///
/// **Both**, and the rejected ones are the point. Section 6.1: "report how many pairs were used
/// versus rejected - honesty here prevents 'why doesn't it look like me' support tickets."
///
/// # Errors
///
/// `AURA-DB-3006` when the pairs cannot be read.
pub fn profile_pairs(
    state: &AppState,
    name: &str,
    limit: Option<u32>,
) -> IpcResult<Vec<StylePairDto>> {
    let limit = limit.unwrap_or(500).clamp(1, 20_000) as usize;
    Ok(state
        .style_store()
        .pairs(name, false)?
        .into_iter()
        .take(limit)
        .map(|row| StylePairDto {
            original: row.original_name,
            final_image: row.final_name,
            matched_by: row.matched_by.as_str().to_string(),
            extracted_from: row.extracted_from.as_str().to_string(),
            bucket: row.bucket.as_key(),
            residual_de00: row.residual_de00,
            accepted: row.accepted,
            rejection: row.rejection.map(|code| code.as_str().to_string()),
        })
        .collect())
}

/// Look at what is in a folder of the photographer's own work, before anything is fitted.
///
/// It opens nothing: a directory walk, an extension census and the matcher. A forty-thousand
/// file archive costs a walk rather than forty thousand decodes, and the expensive work happens
/// once per pair that actually matched.
///
/// # Errors
///
/// `AURA-IO-1001` when a folder cannot be read.
pub fn scan_archive(state: &AppState, input: &ScanArchiveInput) -> IpcResult<ScanArchiveDto> {
    let _ = state;
    let mut out = ScanArchiveDto {
        originals: 0,
        finals: 0,
        matched: 0,
        unmatched_originals: 0,
        unmatched_finals: 0,
        by_method: Vec::new(),
        weakest_method: aura_core::contract::style::MatchMethod::Unmatched
            .as_str()
            .to_string(),
        enough: false,
    };
    let mut weakest = aura_core::contract::style::MatchMethod::ContentHash;
    let mut totals: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();

    for root in &input.roots {
        let label = Path::new(root)
            .file_name()
            .map_or_else(|| root.clone(), |name| name.to_string_lossy().to_string());
        let archive: Archive = pairs::scan(Path::new(root), &label)?;
        out.originals += u32::try_from(archive.originals.len()).unwrap_or(u32::MAX);
        out.finals += u32::try_from(archive.finals.len()).unwrap_or(u32::MAX);

        let report = pairs::match_all(&archive);
        out.matched += u32::try_from(report.pairs.len()).unwrap_or(u32::MAX);
        out.unmatched_originals +=
            u32::try_from(report.unmatched_originals.len()).unwrap_or(u32::MAX);
        out.unmatched_finals += u32::try_from(report.unmatched_finals.len()).unwrap_or(u32::MAX);
        weakest = weakest.max(report.weakest_method());
        for (method, count) in report.by_method {
            *totals.entry(method).or_insert(0) += count;
        }
    }
    out.by_method = totals.into_iter().collect();
    out.weakest_method = weakest.as_str().to_string();
    out.enough = out.matched as usize >= MIN_PAIRS;
    Ok(out)
}

/// Train a profile from the pairs a scan already stored.
///
/// The result is a **candidate**. Adoption is `adopt_profile`.
///
/// # Errors
///
/// `AURA-ML-5073` when fewer than [`MIN_PAIRS`] pairs survive.
pub fn train_profile(
    state: &AppState,
    input: &TrainProfileInput,
    source: Arc<dyn aura_style::bucket::PairSource>,
    baseline: Arc<dyn aura_style::api::Baseline>,
) -> IpcResult<TrainProfileDto> {
    let cancel = CancelToken::new();
    if let Some(id) = input.cancel_id.as_deref() {
        state.register_job(id, cancel.clone());
    }
    let pass = TrainPass::new(
        state.style_store(),
        Arc::clone(state.clock()),
        source,
        baseline,
        input.name.clone(),
    );
    let report = pass.run(&[], |_| StyleQuery::default(), &NullProgress, &cancel);
    if let Some(id) = input.cancel_id.as_deref() {
        state.finish_job(id);
    }
    let report = report?;
    let trained = state
        .style()
        .profiles()?
        .into_iter()
        .filter(|profile| profile.name == input.name)
        .max_by_key(|profile| profile.version);

    Ok(TrainProfileDto {
        profile: trained.as_ref().map(profile_dto),
        matched: u32::try_from(report.matched).unwrap_or(u32::MAX),
        accepted: u32::try_from(report.accepted).unwrap_or(u32::MAX),
        rejected: u32::try_from(report.rejected).unwrap_or(u32::MAX),
        reused: u32::try_from(report.reused).unwrap_or(u32::MAX),
        from_xmp: u32::try_from(report.from_xmp).unwrap_or(u32::MAX),
        buckets: u32::try_from(report.buckets).unwrap_or(u32::MAX),
        overall_de00: report.report.diagnostics.overall_de00,
        baseline_de00: report.report.baseline_de00,
        elapsed_ms: report.elapsed_ms,
        cancelled: report.cancelled,
    })
}

/// Adopt one profile.
///
/// # Errors
///
/// `AURA-ML-5075` when the profile does not exist or was fitted against a different engine.
pub fn adopt_profile(state: &AppState, input: &AdoptProfileInput) -> IpcResult<StyleProfileDto> {
    let id = parse_profile(&input.profile_id)?;
    let style = state.style();
    style.adopt(id)?;
    let Some(profile) = style.profile(id)? else {
        return Err(IpcError::from(aura_style::errors::profile_refused(
            &input.profile_id,
            "the profile disappeared between adoption and reading it back",
        )));
    };
    Ok(profile_dto(&profile))
}

/// The side-by-side of the baseline, the adopted profile and a candidate.
///
/// One row per leaf rather than per photograph, and it returns **no pixels**: the numbers are
/// here and the previews come from the develop surface, which already owns turning a recipe
/// into an image. ADR-0036 decision 3.
///
/// # Errors
///
/// `AURA-ML-5075` when the candidate does not exist.
pub fn compare_profiles(
    state: &AppState,
    input: &CompareProfilesInput,
) -> IpcResult<Vec<StyleComparisonDto>> {
    let project = parse_project(&input.project_id)?;
    let candidate_id = parse_profile(&input.candidate_id)?;
    let style = state.style();
    let Some(candidate) = style.profile(candidate_id)? else {
        return Err(IpcError::from(aura_style::errors::profile_refused(
            &input.candidate_id,
            "no profile with that id exists",
        )));
    };
    let current = match style.outline(project)?.active {
        Some(active) => style.profile(active)?,
        None => None,
    };
    let context = infer::Context {
        engine: style.engine(),
        analysis_ver: ANALYSIS_VER,
    };
    let limit = input.limit.unwrap_or(COMPARE_DEFAULT).clamp(1, 80) as usize;

    let mut out = Vec::new();
    for bucket in candidate
        .buckets
        .values()
        .filter(|model| model.samples > 0)
        .take(limit)
    {
        let query = query_for(project, bucket.bucket);
        let advice = infer::advise(Some(&candidate), &query, &context);
        let theirs = current
            .as_ref()
            .map(|profile| infer::advise(Some(profile), &query, &context));
        out.push(StyleComparisonDto {
            bucket: bucket.bucket.as_key(),
            title: bucket.bucket.title(),
            // The baseline is four zeros, because a style is a residual: what phases 15 and 16
            // decided *is* the origin this comparison is drawn from.
            baseline: vec![0.0, 0.0, 0.0, 0.0],
            current: theirs.as_ref().map(summarise).unwrap_or_default(),
            candidate: summarise(&advice),
            level: advice.level.as_str().to_string(),
            confidence: advice.confidence,
            reasons: advice
                .reasons
                .iter()
                .map(|reason| StyleReasonDto {
                    code: reason.code.as_str().to_string(),
                    text: reason.text.clone(),
                    weight: reason.weight,
                })
                .collect(),
        });
    }
    Ok(out)
}

/// Write a signed, portable profile.
///
/// # Errors
///
/// `AURA-ML-5075` when the profile does not exist, `AURA-ML-5076` when it will not serialise,
/// `AURA-IO-1002` when the file cannot be written.
pub fn export_profile(state: &AppState, input: &ExportProfileInput) -> IpcResult<ExportProfileDto> {
    let id = parse_profile(&input.profile_id)?;
    let Some(profile) = state.style().profile(id)? else {
        return Err(IpcError::from(aura_style::errors::profile_refused(
            &input.profile_id,
            "no profile with that id exists",
        )));
    };
    let identity = state.style_identity();
    let signed = bundle::export(&profile, &identity)?;
    let text = signed.to_file();
    std::fs::write(&input.path, text.as_bytes())
        .map_err(|err| aura_core::errors::io::from_io(&err, Path::new(&input.path)))?;
    Ok(ExportProfileDto {
        path: input.path.clone(),
        bytes: text.len() as u64,
        fingerprint: signed.fingerprint(),
    })
}

/// Read a signed profile bundle and store it as a candidate.
///
/// # Errors
///
/// `AURA-ML-5076` when the bundle is too large, is not a bundle, is from a future schema, does
/// not match its digest or does not verify. `AURA-IO-1001` when the file cannot be read.
pub fn import_profile(state: &AppState, input: &ImportProfileInput) -> IpcResult<ImportProfileDto> {
    let text = std::fs::read_to_string(&input.path)
        .map_err(|_| aura_core::errors::io::not_found(Path::new(&input.path)))?;
    let read = bundle::import(&text)?;
    // Stored with an empty tree beside it: a bundle carries its own buckets and the store
    // writes them from the profile, so the tree argument is only the group rows and those
    // travel inside `StyleProfile::groups`.
    let tree = aura_style::tree::Tree {
        global: read.profile.global.clone(),
        groups: read.profile.groups.clone(),
        buckets: read.profile.buckets.clone(),
        archives: 0,
        archive_capped: false,
    };
    state.style_store().put_profile(&read.profile, &tree)?;
    Ok(ImportProfileDto {
        profile: profile_dto(&read.profile),
        fingerprint: read.fingerprint,
        unchanged_since_signing: read.unchanged_since_signing,
    })
}

/// Choose which profile a project, or one chapter of it, uses.
///
/// # Errors
///
/// `AURA-ML-5075` when the profile does not exist or has not been adopted.
pub fn set_project_profile(
    state: &AppState,
    input: &SetProjectProfileInput,
) -> IpcResult<StyleStatusDto> {
    let project = parse_project(&input.project_id)?;
    let chapter = match input.chapter.as_deref() {
        None => None,
        Some(slug) => {
            let chapter = ChapterId::from_str_or_other(slug);
            if chapter == ChapterId::Other && slug != ChapterId::Other.as_str() {
                return Err(IpcError::from(aura_style::errors::profile_refused(
                    slug,
                    "that is not one of the nine chapters, so an override for it could never be \
                     read",
                )));
            }
            Some(chapter)
        }
    };
    let profile = match input.profile_id.as_deref() {
        None => None,
        Some(id) => Some(parse_profile(id)?),
    };
    state.style().select(project, chapter, profile)?;
    style_status(state, &input.project_id)
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

fn profile_dto(profile: &StyleProfile) -> StyleProfileDto {
    StyleProfileDto {
        profile_id: profile.id.to_db(),
        name: profile.name.clone(),
        version: u32::from(profile.version),
        status: profile.status.as_str().to_string(),
        trained_pairs: profile.trained_pairs,
        strength: profile.strength(),
        overall_de00: profile.diagnostics.overall_de00,
        taught_buckets: u32::try_from(profile.taught_buckets()).unwrap_or(u32::MAX),
        usable: profile.is_usable(),
        engine_ver: profile.engine_ver.clone(),
        trained_at: profile.trained_at,
    }
}

fn report_dto(profile: &StyleProfile) -> ProfileReportDto {
    let per_bucket = profile
        .diagnostics
        .per_bucket
        .iter()
        .map(|row| {
            let model = profile.buckets.get(&row.bucket);
            StyleBucketDto {
                key: row.bucket.as_key(),
                group: row.bucket.group.as_str().to_string(),
                lighting: row.bucket.lighting.as_str().to_string(),
                title: row.bucket.title(),
                samples: row.samples,
                held_out: model.map_or(0, |model| model.held_out),
                match_de00: row.match_de00,
                level: row.level.as_str().to_string(),
                weak: model.is_some_and(aura_core::contract::style::BucketModel::is_weak),
            }
        })
        .collect();
    ProfileReportDto {
        profile: profile_dto(profile),
        per_bucket,
        weak_buckets: diagnostics::weak_buckets(profile)
            .into_iter()
            .map(|bucket| bucket.as_key())
            .collect(),
        recommendation: profile.diagnostics.recommendation.clone(),
        accepted_pairs: profile.diagnostics.accepted_pairs,
        rejected_pairs: profile.diagnostics.rejected_pairs,
        acceptance: profile.diagnostics.acceptance(),
        met_ceiling: profile.diagnostics.met_ceiling(),
    }
}

/// The four numbers the comparison summarises a delta with.
fn summarise(advice: &aura_core::contract::style::StyleAdvice) -> Vec<f32> {
    vec![
        advice.delta.exposure,
        advice.delta.temperature_k,
        advice.delta.contrast,
        advice.delta.vibrance,
    ]
}

/// A query that lands in one particular leaf.
///
/// The comparison is about *buckets* rather than about photographs, so the frame features are
/// left at their defaults: what is being compared is the lean a leaf carries, and conditioning
/// it on one arbitrary frame's ISO would make the panel's numbers depend on which frame the
/// list happened to start at.
fn query_for(project: ProjectId, bucket: StyleBucket) -> StyleQuery {
    StyleQuery {
        project: Some(project),
        scene: scene_for(bucket.group),
        chapter: bucket.group.usual_chapter(),
        lighting: bucket.lighting,
        ..StyleQuery::default()
    }
}

/// A representative scene for one group, so `SceneGroup::of_scene` round-trips.
const fn scene_for(group: SceneGroup) -> SceneId {
    match group {
        SceneGroup::Preparation => SceneId::GettingReadyBride,
        SceneGroup::Details => SceneId::Details,
        SceneGroup::Ceremony => SceneId::Ceremony,
        SceneGroup::Portraits => SceneId::CouplePortrait,
        SceneGroup::Reception => SceneId::Speeches,
        SceneGroup::Dance => SceneId::DanceFloor,
        SceneGroup::Candid => SceneId::Candid,
        SceneGroup::Other => SceneId::Unknown,
    }
}

fn parse_project(text: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(text).map_err(|_| bad_id("project", text))
}

fn parse_profile(text: &str) -> IpcResult<ProfileId> {
    ProfileId::from_db(text).map_err(|_| bad_id("profile", text))
}

fn bad_id(kind: &str, id: &str) -> IpcError {
    IpcError::from(aura_core::errors::db::statement_failed(
        format!("not a {kind} id: {id}"),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_group_round_trips_through_its_representative_scene() {
        for group in SceneGroup::ALL {
            assert_eq!(SceneGroup::of_scene(scene_for(group)), group);
        }
    }

    #[test]
    fn this_surface_cannot_return_a_pixel() {
        // A grep as a test, in the module that would have to grow the field. Section 13's
        // "never uploads imagery" and section 9's SEC task are properties of the shapes; this
        // is what fails the build if a shape changes.
        let source = include_str!("contract/ipc.rs");
        let start = source
            .find("// PHASE-17. Style learning")
            .unwrap_or(source.len());
        // Bounded at the next phase's block. PHASE-18 added a mask overlay that carries a
        // base64 alpha plane - derived geometry about a region rather than a photograph - and an
        // unbounded scan would fail on it while claiming to be about style profiles. The check
        // is about *this* block, and it has to say which block it is about.
        let end = source
            .get(start..)
            .and_then(|rest| rest.find("// PHASE-18. Local mask AI"))
            .map_or(source.len(), |at| start + at);
        let source = source.get(..end).unwrap_or(source);
        // Comments are stripped first: the block *says* it carries no pixels, and a scan that
        // read its own explanation would fail on the sentence that describes the guarantee.
        let block: String = source
            .get(start..)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<&str>>()
            .join(
                "
",
            );
        for forbidden in [
            "Vec<u8>",
            "base64",
            "thumbnail",
            "PixelBuffer",
            "RenderedData",
        ] {
            assert!(
                !block.contains(forbidden),
                "the style IPC surface grew a {forbidden} field"
            );
        }
    }

    #[test]
    fn a_representative_query_lands_in_the_bucket_it_was_built_for() {
        use aura_core::contract::style::LightingBucket;

        let bucket = StyleBucket::new(SceneGroup::Dance, LightingBucket::Stage);
        let query = query_for(ProjectId::new(), bucket);
        assert_eq!(infer::bucket_of(&query), bucket);
    }
}
