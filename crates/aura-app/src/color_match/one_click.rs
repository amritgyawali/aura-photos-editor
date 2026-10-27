//! The one-click colour match (ADR-0170): choose a reference and the photograph is matched,
//! a preference level that backs off when a check refuses it, a sharp preview of the
//! reference for side-by-side viewing, references ranked for the open photograph, and
//! saved presets.
//!
//! Nothing here fits colour. Every match goes through [`super::prepare`] and
//! [`super::apply`], so the existing full-resolution highlight, skin and distance checks
//! decide every saved edit exactly as they do for the detailed panel.
use aura_core::progress::CancelToken;
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_raw::codec::Rgb8;
use image::ImageEncoder;
use serde::{Deserialize, Serialize};

use super::{
    appearance, apply, asset_key, check_cancel, content, fitting_frame, invalid, job, matcher,
    prepare_mode, read, rendered, session_key, status, store, ApplyInput, Controls, MatchReport,
    PrepareInput, Reference, Session, ENGINE,
};
use crate::{commands::IpcResult, AppState};

const PREVIEW: &str = "colour_reference_preview_v1:";
const PRESETS: &str = "colour_match_presets_v1";
/// Long edge of the stored reference preview. Sharp on a laptop canvas, about 200 KB.
const PREVIEW_EDGE: u32 = 1280;
const MAX_PRESETS: usize = 64;
const MAX_RECOMMENDATIONS: usize = 12;
/// Each refused attempt keeps this share of the previous amount.
const STEP: f32 = 0.8;
const MAX_ATTEMPTS: u32 = 6;
/// Below this an automatic back-off has nothing worth saving.
const MIN_AMOUNT: f32 = 0.15;
/// The refusals a smaller amount can cure. Both matching engines word their checks with
/// these openings; `the_refusals_we_step_down_from_are_the_ones_the_engines_raise` keeps the
/// two lists honest.
const RECOVERABLE: [&str; 2] = [
    "This strength failed the full-resolution highlight or skin-colour check",
    "These strengths moved the native-resolution colour farther from the reference",
];
/// The small preview finds the level before one full-resolution check confirms it. Its steps
/// are finer because each costs a fraction of a second, and its clipping allowance is tighter
/// because a 512 px picture hides some of the clipped pixels a full render shows.
const PREDICT_STEP: f32 = 0.9;
const PREDICT_CLIP_MARGIN: f32 = 0.01;
/// Structural similarity at or above which a reference is the photograph being matched (or a
/// copy, an edit or a crop-free burst twin of it), not a different photo. Matches the threshold
/// the matcher already treats as the same frame.
pub(super) const DUPLICATE_SIMILARITY: f32 = 0.95;
/// The refusal for a duplicate. The panel and Sync read its opening to report a skip.
pub(super) const DUPLICATE: &str = "This reference is the same photo you are matching";
/// The opening of the refusal when no amount of a look passes the checks.
pub(super) const NOT_SUITED: &str = "This reference's look does not suit this photo:";
const LEVELS: [&str; 6] = [
    "light", "natural", "standard", "strong", "extreme", "custom",
];

const fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuickMatchInput {
    pub photo_id: String,
    pub reference_id: String,
    pub controls: Controls,
    pub cancel_id: String,
    /// When a check refuses the requested amount, try smaller ones before giving up.
    #[serde(default = "yes")]
    pub step_down: bool,
    /// Measure the photograph again even when a current analysis exists.
    #[serde(default)]
    pub reanalyse: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickMatchReport {
    #[serde(flatten)]
    pub report: MatchReport,
    /// The amount that was asked for; `report.controls.amount` is the one that was saved.
    pub requested_amount: f32,
    /// True when a check refused the requested amount and a smaller one was saved.
    pub limited: bool,
    /// True when the photo had no saved edit and its unedited state was saved as the start.
    pub created_baseline: bool,
    /// The check that held the match below the request, when one did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limited_by: Option<Limiter>,
    /// True when the look fit could not bring this photo closer to the reference and the
    /// cautious estimate was used instead: a smaller move that is still toward the reference.
    #[serde(default)]
    pub cautious: bool,
    pub attempts: u32,
}

/// Keeps one cancel id answering for the whole sequence of analyse and apply calls, each of
/// which registers and forgets the same id for its own duration.
struct Registration<'a> {
    state: &'a AppState,
    id: &'a str,
    token: CancelToken,
}
impl<'a> Registration<'a> {
    fn new(state: &'a AppState, id: &'a str) -> Self {
        let token = CancelToken::new();
        state.register_job(id, token.clone());
        Self { state, id, token }
    }
    fn resume(&self) -> AuraResult<()> {
        self.state.register_job(self.id, self.token.clone());
        check_cancel(&self.token)
    }
}
impl Drop for Registration<'_> {
    fn drop(&mut self) {
        self.state.finish_job(self.id);
    }
}

fn recoverable(error: &crate::contract::ipc::IpcError) -> bool {
    RECOVERABLE
        .iter()
        .any(|prefix| error.message.starts_with(prefix))
}

/// Which check held a match below the amount asked for. Named in the report so the panel can say
/// why, and so a photographer knows whether a different reference would help.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Limiter {
    /// More of the look would newly clip highlights.
    Highlights,
    /// More of the look would change somebody's skin beyond the look guard.
    Skin,
    /// More of the look would move the photo farther from the reference, not closer.
    Distance,
}

/// The largest amount, from the request down in steps of a tenth, that passes the same three
/// checks on the 512 px fitting picture, and the check that refused the request. `None` when
/// there is nothing to predict with: an older slider-fit session, or measurements this build
/// cannot use.
fn predicted(
    state: &AppState,
    photo_id: &str,
    photo: PhotoId,
    controls: &Controls,
    cancel: &CancelToken,
) -> AuraResult<Option<(f32, Option<Limiter>)>> {
    let Some(session) = read::<Session>(state, &session_key(photo_id)?)? else {
        return Ok(None);
    };
    let Some(transfer) = session.transfer.clone() else {
        return Ok(None);
    };
    if !transfer.reference.usable() || controls.amount <= 0.0 {
        return Ok(None);
    }
    let _work = state.image_work_gate().acquire();
    let frame = fitting_frame(state, &photo)?;
    let base = rendered(state, &frame, &matcher::guard_baseline(&session))?;
    let before_regions = appearance::readings(&base, &session.target_regions);
    let before_clip = appearance::clipped(&base);
    let before = matcher::measured(&transfer.reference, &base).total;
    let mut amount = controls.amount;
    let mut smallest = amount;
    let mut limiter = None;
    while amount >= MIN_AMOUNT {
        check_cancel(cancel)?;
        let candidate = matcher::candidate(
            &session,
            &transfer,
            &Controls {
                amount,
                ..*controls
            },
        )?;
        let image = rendered(state, &frame, &candidate)?;
        let failed = if appearance::clipped(&image) > before_clip + PREDICT_CLIP_MARGIN {
            Some(Limiter::Highlights)
        } else if !matcher::skin_ok(
            &session,
            &before_regions,
            &appearance::readings(&image, &session.target_regions),
        ) {
            Some(Limiter::Skin)
        } else if matcher::measured(&transfer.reference, &image).total > before + 0.0001 {
            Some(Limiter::Distance)
        } else {
            None
        };
        let Some(reason) = failed else {
            return Ok(Some((amount, limiter)));
        };
        limiter = Some(reason);
        smallest = amount;
        amount *= PREDICT_STEP;
    }
    Ok(Some((smallest, limiter)))
}

/// The amounts tried, largest first: the request, then smaller ones while each is refused.
fn amounts(requested: f32, step_down: bool) -> Vec<f32> {
    let mut out = vec![requested];
    if !step_down || requested <= 0.0 {
        return out;
    }
    let mut next = requested * STEP;
    while out.len() < MAX_ATTEMPTS as usize && next >= MIN_AMOUNT {
        out.push(next);
        next *= STEP;
    }
    out
}

fn project_of(state: &AppState, photo: &str) -> AuraResult<ProjectId> {
    let photo = photo.to_owned();
    let project: String = state.catalog().read(move |conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [&photo],
            |r| r.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("colour match photo", &e))
    })?;
    ProjectId::from_db(&project).map_err(|_| invalid("Invalid collection."))
}

/// A photograph nobody has edited is matched from how it looks now: save that as its first
/// edit, so Undo returns to it and the matcher has a baseline to measure from.
fn ensure_baseline(state: &AppState, photo_id: &str, photo: PhotoId) -> AuraResult<bool> {
    if state.recipe_store().load(&photo)?.is_some() {
        return Ok(false);
    }
    let project = project_of(state, photo_id)?;
    let start = crate::develop_commands::neutral_recipe(state, photo);
    match state.recipe_store().save_first(
        &project,
        &photo,
        &start,
        &["baseline".into()],
        "Starting point for reference colour",
    ) {
        Ok(()) => Ok(true),
        // Another edit arrived first; match from that one.
        Err(_) if state.recipe_store().load(&photo)?.is_some() => Ok(false),
        Err(error) => Err(error),
    }
}

/// Choose a reference and match: analyse when needed, apply the requested strengths and,
/// when a full-resolution check refuses them, save the largest smaller amount that passes.
/// # Errors
/// Invalid input, cancellation, a refusal at every amount tried, or any analyse/apply error.
pub fn quick_match(state: &AppState, input: &QuickMatchInput) -> IpcResult<QuickMatchReport> {
    input.controls.check()?;
    asset_key(&input.reference_id)?;
    uuid::Uuid::parse_str(&input.cancel_id).map_err(|_| invalid("Invalid colour matching job."))?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
    let registration = Registration::new(state, &input.cancel_id);
    let created_baseline = ensure_baseline(state, &input.photo_id, photo)?;
    registration.resume()?;
    match attempt(state, input, photo, &registration, created_baseline, true) {
        // The look fit moved this photo away from the reference at every amount. A smaller
        // move toward it is better than none: try the cautious estimate before refusing.
        Err(error) if input.step_down && error.message.starts_with(NOT_SUITED) => {
            registration.resume()?;
            attempt(state, input, photo, &registration, created_baseline, false)
                .map(|report| QuickMatchReport {
                    cautious: true,
                    ..report
                })
                .map_err(|_| error)
        }
        other => other,
    }
}

/// One analyse-and-apply with the look fit (`look`) or the cautious estimate.
fn attempt(
    state: &AppState,
    input: &QuickMatchInput,
    photo: PhotoId,
    registration: &Registration<'_>,
    created_baseline: bool,
    look: bool,
) -> IpcResult<QuickMatchReport> {
    let saved = status(state, &input.photo_id)?;
    let same_fit = read::<Session>(state, &session_key(&input.photo_id)?)?
        .is_some_and(|s| s.transfer.as_ref().is_some_and(|t| t.look) == look);
    let session_id = match saved {
        Some(report)
            if !input.reanalyse
                && same_fit
                && report.current
                && report.reference.id == input.reference_id =>
        {
            report.session_id
        }
        _ => {
            let prepared = prepare_mode(
                state,
                &PrepareInput {
                    photo_id: input.photo_id.clone(),
                    reference_id: input.reference_id.clone(),
                    cancel_id: input.cancel_id.clone(),
                },
                look,
            )?;
            registration.resume()?;
            prepared.session_id
        }
    };
    let requested = input.controls.amount;
    // Start the full-resolution checks where the small preview says they will pass, so a
    // refused Extreme costs one or two full renders rather than six.
    let (start, mut limiter) = if input.step_down {
        registration.resume()?;
        let (amount, limiter) = predicted(
            state,
            &input.photo_id,
            photo,
            &input.controls,
            &registration.token,
        )?
        .unwrap_or((requested, None));
        (amount.min(requested), limiter)
    } else {
        (requested, None)
    };
    let mut refused = None;
    for (tried, amount) in amounts(start, input.step_down).into_iter().enumerate() {
        registration.resume()?;
        let attempts = u32::try_from(tried + 1).unwrap_or(u32::MAX);
        let applied = apply(
            state,
            &ApplyInput {
                photo_id: input.photo_id.clone(),
                session_id: session_id.clone(),
                controls: Controls {
                    amount,
                    ..input.controls
                },
                cancel_id: input.cancel_id.clone(),
            },
        );
        match applied {
            Ok(report) => {
                let reduced = amount < requested;
                return Ok(QuickMatchReport {
                    report,
                    requested_amount: requested,
                    limited: reduced,
                    created_baseline,
                    attempts,
                    limited_by: if reduced { limiter } else { None },
                    cautious: false,
                });
            }
            Err(error) if recoverable(&error) => {
                if limiter.is_none() {
                    limiter = Some(if error.message.starts_with(RECOVERABLE[1]) {
                        Limiter::Distance
                    } else {
                        Limiter::Highlights
                    });
                }
                refused = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    let mut error = refused.unwrap_or_else(|| invalid("No colour match was saved.").into());
    if input.step_down {
        let why = match limiter {
            Some(Limiter::Skin) => "would change how skin looks",
            Some(Limiter::Distance) => {
                "moved its colours away from the reference instead of toward it"
            }
            Some(Limiter::Highlights) | None => "would blow out highlights",
        };
        error.message = format!(
            "{NOT_SUITED} even a light match {why}. Nothing was changed. Try a reference shot in light closer to this photo's."
        );
    }
    Err(error)
}

// ---------------------------------------------------------------- reference preview

/// Keep a sharp copy of the reference for the side-by-side view. Never used for fitting.
pub(super) fn remember_preview(state: &AppState, id: &str, image: &Rgb8) -> AuraResult<()> {
    let key = format!("{PREVIEW}{id}");
    if read::<String>(state, &key)?.is_some() {
        return Ok(());
    }
    let rgb = image::RgbImage::from_raw(image.width, image.height, image.data.clone())
        .ok_or_else(|| invalid("Reference preview dimensions are invalid."))?;
    let small = if image.width.max(image.height) > PREVIEW_EDGE {
        image::imageops::thumbnail(&rgb, PREVIEW_EDGE, PREVIEW_EDGE)
    } else {
        rgb
    };
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 86)
        .write_image(
            small.as_raw(),
            small.width(),
            small.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|_| invalid("Cannot save the reference preview."))?;
    store(
        state,
        &key,
        &format!(
            "data:image/jpeg;base64,{}",
            crate::develop_commands::base64(&bytes)
        ),
    )
}

/// Remove a reference's preview in the same transaction that removes the reference.
pub(super) fn forget_preview(conn: &rusqlite::Connection, id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM setting WHERE key=?1",
        [format!("{PREVIEW}{id}")],
    )
}

/// The reference as a picture, for viewing beside the photograph. `None` for a reference
/// saved before previews were kept; uploading it again adds one.
/// # Errors
/// Invalid id or unreadable catalogue state.
pub fn reference_preview(state: &AppState, reference_id: &str) -> IpcResult<Option<String>> {
    let asset = asset_key(reference_id)?;
    if read::<serde_json::Value>(state, &asset)?.is_none() {
        return Ok(None);
    }
    Ok(read::<String>(state, &format!("{PREVIEW}{reference_id}"))?)
}

// ---------------------------------------------------------------- recommendations

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecommendInput {
    pub photo_id: String,
    pub cancel_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub reference_id: String,
    /// 0 to 1. How alike the two photographs are in subject, brightness, colourfulness and
    /// shape - not a prediction of how good the match will look.
    pub score: f32,
    pub reasons: Vec<String>,
}

/// What a photograph is of and how it is lit, in the terms recommendations compare.
#[derive(Debug, Clone, PartialEq)]
struct Scene {
    subjects: Vec<String>,
    brightness: f32,
    colourfulness: f32,
    orientation: i8,
}

fn orientation(width: u32, height: u32) -> i8 {
    let ratio = width as f32 / height.max(1) as f32;
    if ratio > 1.1 {
        1
    } else if ratio < 0.9 {
        -1
    } else {
        0
    }
}

/// Mean luminance and mean channel spread of a small sRGB picture, both 0 to 1.
fn key_of(rgb: &[u8]) -> (f32, f32) {
    let mut luma = 0.0_f64;
    let mut spread = 0.0_f64;
    let mut n = 0.0_f64;
    for p in rgb.chunks_exact(3) {
        let &[r, g, b] = p else { continue };
        let (r, g, b) = (f64::from(r), f64::from(g), f64::from(b));
        luma += 0.2126 * r + 0.7152 * g + 0.0722 * b;
        spread += r.max(g).max(b) - r.min(g).min(b);
        n += 1.0;
    }
    if n == 0.0 {
        return (0.5, 0.0);
    }
    ((luma / n / 255.0) as f32, (spread / n / 255.0) as f32)
}

fn subjects(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = names
        .into_iter()
        .filter(|n| n != "whole image" && !n.starts_with("guard:"))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn has_people(subjects: &[String]) -> bool {
    subjects.iter().any(|s| s.contains("skin"))
}

/// Score one reference against the photograph. Weights: subjects 0.5, brightness 0.25,
/// colourfulness 0.15, shape 0.1, and a small lift for a favourite.
fn score(target: &Scene, reference: &Scene, favorite: bool) -> (f32, Vec<String>) {
    let mut reasons = Vec::new();
    let shared: Vec<&String> = target
        .subjects
        .iter()
        .filter(|s| reference.subjects.contains(s))
        .collect();
    let union = target
        .subjects
        .iter()
        .chain(
            reference
                .subjects
                .iter()
                .filter(|s| !target.subjects.contains(s)),
        )
        .count();
    let mut subject = if union == 0 {
        1.0
    } else {
        shared.len() as f32 / union as f32
    };
    if has_people(&target.subjects) != has_people(&reference.subjects) {
        subject *= 0.5;
    } else if has_people(&target.subjects) {
        reasons.push("People in both".to_owned());
    }
    if !shared.is_empty() && shared.len() == union {
        reasons.push("Same subjects".to_owned());
    }
    let brightness = 1.0 - ((target.brightness - reference.brightness).abs() / 0.4).min(1.0);
    if brightness >= 0.75 {
        reasons.push("Similar brightness".to_owned());
    }
    let colour = 1.0 - ((target.colourfulness - reference.colourfulness).abs() / 0.25).min(1.0);
    if colour >= 0.75 {
        reasons.push("Similar colourfulness".to_owned());
    }
    let shape = if target.orientation == reference.orientation {
        1.0
    } else {
        0.0
    };
    let mut total = 0.5 * subject + 0.25 * brightness + 0.15 * colour + 0.1 * shape;
    if favorite {
        total += 0.05;
        reasons.push("Favourite".to_owned());
    }
    (total.clamp(0.0, 1.0), reasons)
}

fn rank(target: &Scene, references: &[(String, Scene, bool)]) -> Vec<Recommendation> {
    let mut out: Vec<Recommendation> = references
        .iter()
        .map(|(id, scene, favorite)| {
            let (score, reasons) = score(target, scene, *favorite);
            Recommendation {
                reference_id: id.clone(),
                score,
                reasons,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.reference_id.cmp(&b.reference_id))
    });
    out.truncate(MAX_RECOMMENDATIONS);
    out
}

/// Rank the library for the open photograph: references of similar subjects, brightness
/// and colourfulness first. Reads the photograph once at fitting size; changes nothing.
/// # Errors
/// Invalid photo, cancellation, decode/render or catalogue failure.
pub fn recommend(state: &AppState, input: &RecommendInput) -> IpcResult<Vec<Recommendation>> {
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
    let summaries = super::list_references(state)?;
    if summaries.is_empty() {
        return Ok(Vec::new());
    }
    job(state, &input.cancel_id, |cancel| {
        let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
        let frame = fitting_frame(state, &photo)?;
        check_cancel(cancel)?;
        let image = rendered(state, &frame, &recipe)?;
        let regions = appearance::analyse(&image)?;
        check_cancel(cancel)?;
        let (brightness, colourfulness) = key_of(&content::thumbnail(&image));
        let target = Scene {
            subjects: subjects(regions.into_iter().map(|r| r.name)),
            brightness,
            colourfulness,
            orientation: orientation(image.width, image.height),
        };
        let mut references = Vec::with_capacity(summaries.len());
        for summary in &summaries {
            let Some(reference) = read::<Reference>(state, &asset_key(&summary.id)?)? else {
                continue;
            };
            if reference.engine != ENGINE {
                continue;
            }
            let (brightness, colourfulness) = key_of(&reference.small_rgb);
            references.push((
                summary.id.clone(),
                Scene {
                    subjects: subjects(summary.regions.iter().cloned()),
                    brightness,
                    colourfulness,
                    orientation: orientation(summary.width, summary.height),
                },
                summary.favorite,
            ));
        }
        Ok(rank(&target, &references))
    })
}

// ---------------------------------------------------------------- presets

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorMatchPreset {
    pub id: String,
    pub name: String,
    pub reference_id: String,
    pub reference_name: String,
    pub level: String,
    pub controls: Controls,
    pub created_at: String,
    /// Whether the reference is still in the library. Recomputed every time presets are listed.
    #[serde(default)]
    pub available: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetInput {
    /// `list`, `save`, `rename` or `remove`.
    pub action: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub reference_id: Option<String>,
    pub level: Option<String>,
    pub controls: Option<Controls>,
}

fn preset_name(value: Option<&str>) -> AuraResult<String> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() || value.chars().count() > 60 || value.chars().any(char::is_control) {
        return Err(invalid("Give the preset a name of up to 60 characters."));
    }
    Ok(value.to_owned())
}

/// Save, rename, remove or list colour match presets: a reference plus the strengths to
/// match it with. Applying one re-measures each photograph; nothing is copied between them.
/// # Errors
/// Invalid input, a missing reference or preset, a full list, or a catalogue failure.
#[allow(clippy::too_many_lines)] // Read, change and write the list in one writer transaction.
pub fn presets(state: &AppState, input: &PresetInput) -> IpcResult<Vec<ColorMatchPreset>> {
    let action = input.action.as_str();
    if !matches!(action, "list" | "save" | "rename" | "remove") {
        return Err(invalid("Unknown colour match preset action.").into());
    }
    let mut new_preset = None;
    if action == "save" {
        let name = preset_name(input.name.as_deref())?;
        let reference_id = input.reference_id.clone().unwrap_or_default();
        let reference: Reference = read(state, &asset_key(&reference_id)?)?
            .ok_or_else(|| invalid("Choose a reference in the library before saving a preset."))?;
        let level = input.level.clone().unwrap_or_else(|| "custom".into());
        if !LEVELS.contains(&level.as_str()) {
            return Err(invalid("Unknown colour match preference.").into());
        }
        let controls = input
            .controls
            .ok_or_else(|| invalid("A preset needs its strengths."))?;
        controls.check()?;
        new_preset = Some(ColorMatchPreset {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            reference_id,
            reference_name: reference.summary.name,
            level,
            controls,
            created_at: aura_catalog::rfc3339(state.clock().now_utc()),
            available: true,
        });
    }
    let rename = if action == "rename" {
        Some(preset_name(input.name.as_deref())?)
    } else {
        None
    };
    let id = input.id.clone();
    if matches!(action, "rename" | "remove") && id.is_none() {
        return Err(invalid("Choose a preset first.").into());
    }
    let at = aura_catalog::rfc3339(state.clock().now_utc());
    let action = input.action.clone();
    let mut list: Vec<ColorMatchPreset> = state.catalog().writer().with(move |conn| {
        let db = |e: &rusqlite::Error| {
            aura_core::errors::db::statement_failed("colour match presets", e)
        };
        let text: Option<String> = rusqlite::OptionalExtension::optional(conn.query_row(
            "SELECT value_json FROM setting WHERE key=?1",
            [PRESETS],
            |row| row.get(0),
        ))
        .map_err(|e| db(&e))?;
        let mut list: Vec<ColorMatchPreset> = match text {
            Some(text) if text.len() <= 1024 * 1024 => serde_json::from_str(&text)
                .map_err(|_| invalid("The saved colour match presets are unreadable."))?,
            Some(_) => return Err(invalid("The saved colour match presets are too large.")),
            None => Vec::new(),
        };
        match action.as_str() {
            "save" => {
                if let Some(preset) = new_preset {
                    let same = |p: &ColorMatchPreset| {
                        p.name.to_lowercase() == preset.name.to_lowercase()
                    };
                    // Saving under an existing name replaces that preset, keeping its place.
                    if let Some(slot) = list.iter_mut().find(|p| same(p)) {
                        *slot = preset;
                    } else if list.len() >= MAX_PRESETS {
                        return Err(invalid(
                            "You can keep up to 64 colour match presets. Remove one first.",
                        ));
                    } else {
                        list.insert(0, preset);
                    }
                }
            }
            "rename" | "remove" => {
                let id = id.unwrap_or_default();
                let index = list
                    .iter()
                    .position(|p| p.id == id)
                    .ok_or_else(|| invalid("This preset no longer exists."))?;
                if let Some(name) = rename {
                    if list
                        .iter()
                        .any(|p| p.id != id && p.name.to_lowercase() == name.to_lowercase())
                    {
                        return Err(invalid("Another preset already has this name."));
                    }
                    if let Some(preset) = list.get_mut(index) {
                        preset.name = name;
                    }
                } else {
                    list.remove(index);
                }
            }
            _ => return Ok(list),
        }
        let text = serde_json::to_string(&list)
            .map_err(|_| invalid("Could not save colour match presets."))?;
        conn.execute(
            "INSERT INTO setting(key,value_json,updated_at) VALUES(?1,?2,?3)
             ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
            rusqlite::params![PRESETS, text, at],
        )
        .map_err(|e| db(&e))?;
        Ok(list)
    })?;
    let library: Vec<String> = super::list_references(state)?
        .into_iter()
        .map(|r| r.id)
        .collect();
    for preset in &mut list {
        preset.available = library.contains(&preset.reference_id);
    }
    Ok(list)
}

#[cfg(test)]
mod tests;
