//! Automatic portrait planning produces ordinary, reversible retouch operations.
// Pixel indices are computed from bounds-checked planning coordinates; pixel geometry uses
// the conventional single-letter names (x, y, w, h, l, t, r, b).
#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use aura_core::AuraResult;
use aura_recipe::{
    retouch_tools::{
        self, BrushMask, BrushStroke, Edit, LuminanceRange, Matte, Selection, SkinSettings, Tool,
    },
    Recipe,
};
use aura_vision::portrait::{self, PortraitFace};
use aura_vision::skin;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::portrait_features;
use crate::retouch_settings::{gain, Settings};

pub const KEY: &str = "studio_portrait_auto_v1";
/// Stable ID prefix of every automatic portrait operation.
pub const PREFIX: &str = "auto-portrait-v1-";
/// Stable ID prefix of every automatic scene operation (for example, sky balance).
pub const SCENE_PREFIX: &str = "auto-scene-v1-";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceAssessment {
    pub face: usize,
    pub status: String,
    pub confidence: f32,
    pub reason: String,
    pub strengths: [f32; 3],
    /// Measured finishing decisions for this face: spots, eyes, teeth and shine. ADR-0076.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
    #[serde(default)]
    pub spots_healed: usize,
    #[serde(default)]
    pub marks_kept: usize,
}

/// One saved history step of an automatic pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepSummary {
    pub step: usize,
    pub title: String,
    pub detail: String,
    pub operations: usize,
}

/// What the scene analysis measured and decided.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSummary {
    pub kind: String,
    pub decisions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub model: String,
    pub model_hash: String,
    pub status: String,
    pub detected_faces: usize,
    pub retouched_faces: usize,
    pub operations: usize,
    pub message: String,
    pub faces: Vec<PortraitFace>,
    #[serde(default)]
    pub assessments: Vec<FaceAssessment>,
    #[serde(default)]
    pub planner_version: String,
    /// The history steps the automatic pass saved, in order. Undo walks back through them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<StepSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<SceneSummary>,
    /// The finishing choices this pass used; a later pass repeats them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<portrait_features::Options>,
    /// How skin was found: by the bundled segmenter or by landmark geometry. ADR-0077.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segmentation: Option<SegmentationSummary>,
}

/// What the skin segmenter found, per detected face.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentationSummary {
    pub model: String,
    pub model_hash: String,
    /// Network passes: the whole frame plus a crop per small person.
    pub passes: usize,
    /// Per face, the fraction of the frame selected as that person's face skin, body skin,
    /// hair and clothes.
    pub people: Vec<[f32; 4]>,
    /// Why segmentation was not used, when it was not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

/// The history step an automatic retouch operation belongs to. Order is save order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Scene,
    Skin,
    Blemishes,
    Refine,
    Eyes,
    Finishing,
}

impl Group {
    pub const ALL: [Self; 6] = [
        Self::Scene,
        Self::Skin,
        Self::Blemishes,
        Self::Refine,
        Self::Eyes,
        Self::Finishing,
    ];
}

/// Operations written by automation carry a stable prefix; everything else is manual.
#[must_use]
pub fn group_of(id: &str) -> Option<Group> {
    if id.starts_with(SCENE_PREFIX) {
        return Some(Group::Scene);
    }
    let rest = id.strip_prefix(PREFIX)?;
    Some(
        if rest.contains("-spot-") || rest.ends_with("-body-spots") {
            Group::Blemishes
        } else if rest.contains("-lines-") || rest.contains("-fold-") || rest.contains("-redness-")
        {
            Group::Refine
        } else if rest.contains("-eye-") || rest.contains("-undereye-") {
            Group::Eyes
        } else if rest.ends_with("-teeth")
            || rest.ends_with("-shine")
            || rest.contains("-lips-")
            || rest.contains("-sculpt-")
            || rest.contains("-makeup-")
            || rest.contains("-hair-")
            || rest.ends_with("-fabric")
            || rest == "backdrop"
        {
            Group::Finishing
        } else {
            Group::Skin
        },
    )
}

/// The retouch stack after saving every step up to and including `upto`.
///
/// Manual operations keep their order and come first. Each automatic group is either the
/// newly planned one (when it has been reached and was planned) or whatever the stack held,
/// so re-running an unchanged plan produces an identical stack at every step - which is what
/// keeps a repeat pass from adding history entries.
#[must_use]
pub fn staged(current: &[Edit], planned: &BTreeMap<Group, Vec<Edit>>, upto: Group) -> Vec<Edit> {
    let mut out: Vec<Edit> = current
        .iter()
        .filter(|e| group_of(&e.id).is_none())
        .cloned()
        .collect();
    for group in Group::ALL {
        match planned.get(&group) {
            Some(edits) if group <= upto => out.extend(edits.iter().cloned()),
            _ => out.extend(
                current
                    .iter()
                    .filter(|e| group_of(&e.id) == Some(group))
                    .cloned(),
            ),
        }
    }
    out
}

/// A full portrait plan, not yet written into a recipe.
#[derive(Debug, Clone)]
pub struct Plan {
    pub report: Report,
    /// Planned operations per group. A present-but-empty group removes older automatic work.
    pub groups: BTreeMap<Group, Vec<Edit>>,
    /// Segmentation mattes the planned operations refer to, by id. ADR-0077.
    pub mattes: BTreeMap<String, Matte>,
}

/// The mattes a stack written from `plan` needs: the recipe's own (for operations a person
/// added that refer to them) and the plan's, the plan's winning on a shared id.
/// # Errors
/// The recipe's stored mattes are malformed.
pub fn mattes_for(recipe: &Recipe, plan: &Plan) -> AuraResult<BTreeMap<String, Matte>> {
    let mut all = retouch_tools::read_mattes(recipe)?;
    all.extend(plan.mattes.iter().map(|(k, v)| (k.clone(), v.clone())));
    Ok(all)
}

/// Add a bounded portrait plan to an AI proposal; manually authored stacks stay intact.
/// # Errors
/// Pixel analysis, recipe validation, or report serialization failed.
pub fn apply(proposal: &mut Recipe, rgb: &[u8], width: u32, height: u32) -> AuraResult<Report> {
    let plan = plan(proposal, rgb, width, height, None, 0.0)?;
    let current = retouch_tools::read(proposal)?;
    if !plan.groups.is_empty()
        && (plan.report.operations > 0 || proposal.extra.contains_key(retouch_tools::KEY))
    {
        let mattes = mattes_for(proposal, &plan)?;
        retouch_tools::write_with_mattes(
            proposal,
            &staged(&current, &plan.groups, Group::Finishing),
            &mattes,
        )?;
    }
    write_report(proposal, &plan.report)?;
    Ok(plan.report)
}

/// Store the report in the recipe's extension map.
/// # Errors
/// Serialization failed.
pub fn write_report(proposal: &mut Recipe, report: &Report) -> AuraResult<()> {
    proposal.extra.insert(
        KEY.into(),
        serde_json::to_value(report)
            .map_err(|e| aura_core::errors::render::recipe_invalid(KEY, &e.to_string()))?,
    );
    Ok(())
}

/// Detect faces and plan skin, blemish, eye and finishing operations.
///
/// `rgb` is the small analysis thumbnail the skin planner was tuned on; `detail` is an
/// optional larger rendition of the same photograph for fine features. `exposure` is the
/// global change the same pass applies, in stops.
/// # Errors
/// Invalid pixels or a failed model run.
pub fn plan(
    proposal: &Recipe,
    rgb: &[u8],
    width: u32,
    height: u32,
    detail: Option<(&[u8], u32, u32)>,
    exposure: f32,
) -> AuraResult<Plan> {
    plan_with_faces(
        proposal,
        rgb,
        width,
        height,
        detail,
        exposure,
        None,
        &portrait_features::Options::default(),
        false,
    )
}

/// [`plan`], reusing faces the caller already detected on the same `rgb` thumbnail.
/// # Errors
/// Invalid pixels or a failed model run.
pub fn plan_with_faces(
    proposal: &Recipe,
    rgb: &[u8],
    width: u32,
    height: u32,
    detail: Option<(&[u8], u32, u32)>,
    exposure: f32,
    faces: Option<Vec<PortraitFace>>,
    options: &portrait_features::Options,
    explicit: bool,
) -> AuraResult<Plan> {
    let options = options.sanitised();
    // An explicit request from the photographer replaces automatic operations even in a stack
    // they have edited; their own operations are always kept.
    let protected = !explicit
        && proposal.provenance.user_edited_fields.iter().any(|path| {
            path == retouch_tools::KEY || path.starts_with(&format!("{}.", retouch_tools::KEY))
        });
    let disabled = std::env::var_os("AURA_DISABLE_AUTO_PORTRAIT").is_some_and(|v| v == "1");
    let mut report = Report {
        model: portrait::VERSION.into(),
        model_hash: portrait::MODEL_HASH.into(),
        status: "complete".into(),
        detected_faces: 0,
        retouched_faces: 0,
        operations: 0,
        message: String::new(),
        faces: Vec::new(),
        assessments: Vec::new(),
        planner_version: format!("sample-consensus-v2+{}", portrait_features::VERSION),
        steps: Vec::new(),
        scene: None,
        options: Some(options),
        segmentation: None,
    };
    let mut groups = BTreeMap::new();
    let mut mattes = BTreeMap::new();
    if disabled || protected {
        report.status = if disabled { "disabled" } else { "protected" }.into();
        report.message = if disabled { "Automatic portrait retouch is disabled on this device." } else { "Your manual retouch steps are protected. Undo those steps to return to the automatic version." }.into();
        return Ok(Plan {
            report,
            groups,
            mattes,
        });
    }
    let settings = options.settings;
    report.faces = match faces {
        Some(faces) => faces,
        None => portrait::detect(rgb, width, height)?,
    };
    report.detected_faces = report.faces.len();
    let manual = retouch_tools::read(proposal)?
        .into_iter()
        .filter(|e| group_of(&e.id).is_none())
        .count();
    let scene_ops = retouch_tools::read(proposal)?
        .into_iter()
        .filter(|e| group_of(&e.id) == Some(Group::Scene))
        .count();
    let detail_pixels = detail
        .and_then(|(data, w, h)| portrait_features::Pixels::new(data, w, h))
        .or_else(|| portrait_features::Pixels::new(rgb, width, height));
    // Face and body skin from the bundled person segmenter, measured once for every face.
    let segmentation = segment(rgb, width, height, detail, &report.faces, &settings);
    report.segmentation = Some(segmentation.summary.clone());
    let main = settings
        .main_subject_only
        .then(|| {
            report
                .faces
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| {
                    let area = |f: &PortraitFace| {
                        (f.bounds[2] - f.bounds[0]) * (f.bounds[3] - f.bounds[1])
                    };
                    area(a).total_cmp(&area(b))
                })
                .map(|(i, _)| i)
        })
        .flatten();
    let mut planned: [Vec<Edit>; 5] = Default::default();
    let mut bodies = 0_usize;
    for (index, face) in report.faces.iter().enumerate() {
        if main.is_some_and(|m| m != index) {
            report.assessments.push(FaceAssessment {
                face: index + 1,
                status: "skipped".into(),
                confidence: face.confidence,
                reason: "Only the main subject is retouched in this pass.".into(),
                strengths: [0.0; 3],
                findings: Vec::new(),
                spots_healed: 0,
                marks_kept: 0,
            });
            continue;
        }
        let person = segmentation.people.get(index);
        // A face found only on the larger rendition is too small in the thumbnail to sample;
        // plan it on the larger pixels instead.
        let eye_px = {
            let [[ax, ay], [bx, by], ..] = face.landmarks;
            ((bx - ax) * width as f32).hypot((by - ay) * height as f32)
        };
        let (prgb, pw, ph) = match detail {
            Some(d) if eye_px < 24.0 => d,
            _ => (rgb, width, height),
        };
        let mut face_plan = plan_face(face, index, prgb, pw, ph);
        let face_matte = person.and_then(|p| store(&mut mattes, p.face.as_ref(), index, "face"));
        // Landmark-measured features (spots, eyes, teeth, lines) need a frontal face whose
        // landmarks were trusted; segmented skin does not.
        let landmarks_trusted = !face_plan.edits.is_empty();
        if let Some(matte) = &face_matte {
            if face_plan.edits.is_empty() {
                // Oblique, profile, small or textured faces: the landmark sampler refused, but
                // the segmenter knows where the skin is, so measure from the skin itself.
                let skipped = face_plan.reason.clone();
                face_plan = plan_face_from_matte(face, index, prgb, pw, ph, matte, &skipped);
            }
            for edit in &mut face_plan.edits {
                use_matte(edit, matte, [pw as f32, ph as f32]);
            }
        }
        face_plan.edits = tune_face(face_plan.edits, &settings);
        if let (Some(sample), Some(template)) = (face_plan.sample, face_plan.edits.first().cloned())
        {
            let extra = face_extras(&template, index, &sample, exposure, &settings);
            face_plan.edits.extend(extra);
        }
        let mut body = if options.scope.body() {
            match (&face_plan.sample, person) {
                (Some(sample), Some(p)) if p.body.is_some() => {
                    let matte = store(&mut mattes, p.body.as_ref(), index, "body");
                    matte.map_or_else(
                        || FacePlan::skip(""),
                        |m| plan_body_matte(face, index, prgb, pw, ph, sample, &m, &settings, exposure),
                    )
                }
                (Some(_), Some(p)) if segmentation.summary.unavailable.is_none() && p.face.is_some() => {
                    FacePlan::skip(
                        "Body skin: the segmenter found no visible body skin for this person (covered by clothing, hair or out of frame).",
                    )
                }
                (Some(sample), _) => tune_body(plan_body(face, index, prgb, pw, ph, sample), &settings),
                (None, _) => FacePlan::skip(
                    "Body skin was not retouched: no reliable face skin sample to compare it with.",
                ),
            }
        } else {
            FacePlan::skip("")
        };
        // Hair and clothes are finished whatever skin was chosen, when asked for.
        let mut garments = Vec::new();
        if let Some(p) = person {
            if settings.hair_detail > 0.0 || settings.hair_shine > 0.0 {
                if let Some(m) = store(&mut mattes, p.hair.as_ref(), index, "hair") {
                    garments.extend(hair_ops(index, &m, prgb, pw, ph, exposure, &settings));
                }
            }
            if settings.fabric > 0.0 {
                let inside = p
                    .clothes
                    .as_ref()
                    .map(|m| erode(m, (m.width.max(m.height) / 60).max(1)));
                if let Some(m) = store(&mut mattes, inside.as_ref(), index, "clothes") {
                    garments.push(fabric_op(index, &m, face, &settings));
                }
            }
        }
        let mut plan = if options.scope.face() {
            face_plan
        } else {
            FacePlan {
                edits: Vec::new(),
                reason:
                    "Face retouch is switched off for this pass; only body skin was considered."
                        .into(),
                sample: face_plan.sample,
            }
        };
        for edit in plan.edits.iter_mut().chain(&mut body.edits) {
            edit.amount = (edit.amount * options.intensity).clamp(0.05, 0.95);
        }
        let mut features = portrait_features::FeatureEdits::default();
        if !plan.edits.is_empty() && landmarks_trusted {
            if let Some(px) = &detail_pixels {
                features = portrait_features::plan(
                    face,
                    index,
                    px,
                    exposure,
                    PREFIX,
                    &options,
                    face_matte.as_ref().map(|m| m.id.as_str()),
                );
            }
        }
        features.finishing.extend(garments);
        let used: usize = planned.iter().map(Vec::len).sum();
        let wanted = plan.edits.len()
            + body.edits.len()
            + features.blemishes.len()
            + features.refine.len()
            + features.eyes.len()
            + features.finishing.len();
        if manual + scene_ops + used + wanted > retouch_tools::MAX_EDITS {
            plan = FacePlan::skip("The saved retouch stack has reached its operation limit.");
            body = FacePlan::skip("");
            features = portrait_features::FeatureEdits::default();
        }
        let mut findings = features.report.findings;
        if options.scope.body() && !body.reason.is_empty() {
            findings.insert(0, body.reason.clone());
        }
        findings.insert(0, segmentation.finding(index));
        let mut strengths = [0.0; 3];
        for (strength, edit) in strengths.iter_mut().zip(&plan.edits) {
            *strength = edit.amount;
        }
        report.assessments.push(FaceAssessment {
            face: index + 1,
            status: if plan.edits.is_empty() && body.edits.is_empty() {
                "skipped"
            } else {
                "retouched"
            }
            .into(),
            confidence: face.confidence,
            reason: plan.reason,
            strengths,
            findings,
            spots_healed: features.report.spots_healed,
            marks_kept: features.report.marks_kept,
        });
        if plan.edits.is_empty() && body.edits.is_empty() {
            continue;
        }
        report.retouched_faces += 1;
        report.operations += wanted;
        bodies += usize::from(!body.edits.is_empty());
        let [skin, spots, refine, eyes, finishing] = &mut planned;
        skin.extend(plan.edits);
        for edit in body.edits {
            if group_of(&edit.id) == Some(Group::Blemishes) {
                spots.push(edit);
            } else if group_of(&edit.id) == Some(Group::Refine) {
                refine.push(edit);
            } else {
                skin.push(edit);
            }
        }
        spots.extend(features.blemishes);
        refine.extend(features.refine);
        eyes.extend(features.eyes);
        finishing.extend(features.finishing);
    }
    let [skin, spots, refine, eyes, mut finishing] = planned;
    // A plain backdrop, smoothed for the whole frame when asked for.
    if settings.backdrop > 0.0 && !report.faces.is_empty() {
        // Kept clear of the subject by the blur's own reach, so no colour bleeds across.
        let away = segmentation
            .background
            .as_ref()
            .map(|m| erode(m, (m.width.max(m.height) / 40).max(2)));
        let texture = away
            .as_ref()
            .map_or(f32::INFINITY, |m| matte_texture(m, rgb, width, height));
        if texture > PLAIN_BACKDROP {
            report.message += " Backdrop smoothing was skipped: the background is textured, not a plain backdrop.";
        } else if let Some(m) = store(&mut mattes, away.as_ref(), 0, "background") {
            let used: usize = [&skin, &spots, &refine, &eyes, &finishing]
                .iter()
                .map(|v| v.len())
                .sum();
            if manual + scene_ops + used < retouch_tools::MAX_EDITS {
                finishing.push(backdrop_op(&m, &report.faces, &settings));
                report.operations += 1;
            }
        }
    }
    groups.insert(Group::Skin, skin);
    groups.insert(Group::Blemishes, spots);
    groups.insert(Group::Refine, refine);
    groups.insert(Group::Eyes, eyes);
    groups.insert(Group::Finishing, finishing);
    let spots: usize = report.assessments.iter().map(|a| a.spots_healed).sum();
    let kept: usize = report.assessments.iter().map(|a| a.marks_kept).sum();
    report.message = if report.detected_faces == 0 {
        "No confident, sufficiently large face found. Portrait retouch was skipped.".into()
    } else if report.retouched_faces == 0 {
        "Faces detected, but no suitable skin sample was found or the operation limit was reached. Portrait retouch was skipped.".into()
    } else if !options.scope.face() {
        format!(
            "Retouched body skin for {bodies} of {} people with {} editable steps; faces were left as they are. Undo walks back one automatic step at a time.",
            report.detected_faces, report.operations,
        )
    } else {
        format!(
            "Retouched {} of {} detected faces with {} editable steps: skin texture, tone and light{}, eye and teeth finishing where measured.{}{} Undo walks back one automatic step at a time.",
            report.retouched_faces,
            report.detected_faces,
            report.operations,
            if spots > 0 { format!(", {spots} healed spot{}", if spots == 1 { "" } else { "s" }) } else { String::new() },
            if kept > 0 { format!(" {kept} possible permanent mark{} kept.", if kept == 1 { "" } else { "s" }) } else { String::new() },
            if options.scope.body() { format!(" Body skin retouched for {bodies} {}.", if bodies == 1 { "person" } else { "people" }) } else { String::new() },
        )
    };
    // Only mattes an operation refers to are kept.
    let used: std::collections::BTreeSet<&str> = groups
        .values()
        .flatten()
        .filter_map(|e| e.matte.as_deref())
        .collect();
    mattes.retain(|id, _| used.contains(id.as_str()));
    Ok(Plan {
        report,
        groups,
        mattes,
    })
}

struct FacePlan {
    edits: Vec<Edit>,
    reason: String,
    /// The representative skin sample, when one was found.
    sample: Option<Sample>,
}

impl FacePlan {
    fn skip(reason: &str) -> Self {
        Self {
            edits: Vec::new(),
            reason: reason.into(),
            sample: None,
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    point: [f32; 2],
    mean: f32,
    variation: f32,
    chroma: [f32; 3],
}

fn median(values: impl Iterator<Item = f32>) -> f32 {
    let mut values: Vec<_> = values.collect();
    values.sort_by(f32::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

fn color_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

fn representative_sample(samples: &[Sample]) -> Option<(Sample, [f32; 3])> {
    if samples.is_empty() {
        return None;
    }
    let chroma = std::array::from_fn(|c| {
        median(
            samples
                .iter()
                .map(|s| s.chroma.get(c).copied().unwrap_or(0.0)),
        )
    });
    let luminance = median(samples.iter().map(|s| s.mean));
    let candidates: Vec<_> = samples
        .iter()
        .filter(|s| color_distance(s.chroma, chroma) <= 0.12)
        .collect();
    let score = |s: &&Sample| {
        s.variation + color_distance(s.chroma, chroma) * 0.4 + (s.mean - luminance).abs() * 0.04
    };
    let sample = **candidates
        .iter()
        .min_by(|a, b| score(a).total_cmp(&score(b)))?;
    let texture = median(candidates.iter().map(|s| s.variation));
    let color_spread = median(candidates.iter().map(|s| color_distance(s.chroma, chroma)));
    let light_spread = median(candidates.iter().map(|s| (s.mean - luminance).abs()));
    // Low signal gets a gentler correction regardless of complexion. Variations
    // are relative to each face's own signal rather than to a desired skin tone.
    let signal = if luminance < 0.15 { 0.65 } else { 1.0 };
    Some((
        sample,
        [
            (0.5 + texture / luminance.max(0.08) * 0.4).clamp(0.5, 0.8) * signal,
            (0.25 + color_spread * 1.5).clamp(0.25, 0.5) * signal,
            (0.2 + light_spread / luminance.max(0.08) * 0.5).clamp(0.2, 0.4) * signal,
        ],
    ))
}

// Pixel-space geometry avoids elongated masks on portrait/landscape images.
// Every indexed coordinate below is a statically sized two-element array.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing
)]
fn plan_face(face: &PortraitFace, index: usize, rgb: &[u8], width: u32, height: u32) -> FacePlan {
    let w = width as f32;
    let h = height as f32;
    let short = w.min(h);
    let [eye_a, eye_b, nose, mouth_a, mouth_b] = face.landmarks.map(|[x, y]| [x * w, y * h]);
    let dx = eye_b[0] - eye_a[0];
    let dy = eye_b[1] - eye_a[1];
    let distance = dx.hypot(dy);
    if distance < 12.0 {
        return FacePlan::skip("The face is too small for reliable skin sampling.");
    }
    let u = [dx / distance, dy / distance];
    let mut v = [-u[1], u[0]];
    let mid = [(eye_a[0] + eye_b[0]) * 0.5, (eye_a[1] + eye_b[1]) * 0.5];
    let mouth = [
        (mouth_a[0] + mouth_b[0]) * 0.5,
        (mouth_a[1] + mouth_b[1]) * 0.5,
    ];
    if (mouth[0] - mid[0]) * v[0] + (mouth[1] - mid[1]) * v[1] < 0.0 {
        v = [-v[0], -v[1]];
    }
    // Refuse highly oblique/occluded configurations instead of guessing skin.
    let mouth_down = (mouth[0] - mid[0]) * v[0] + (mouth[1] - mid[1]) * v[1];
    let nose_side = ((nose[0] - mid[0]) * u[0] + (nose[1] - mid[1]) * u[1]).abs();
    if !(0.4 * distance..1.5 * distance).contains(&mouth_down) || nose_side > 0.45 * distance {
        return FacePlan::skip("The facial landmarks suggest an oblique or occluded face; automatic skin retouch was skipped.");
    }
    let [left, top, right, bottom] = face.bounds;
    let inside = |[x, y]: [f32; 2], r: f32| {
        x - r >= left * w && x + r <= right * w && y - r >= top * h && y + r <= bottom * h
    };
    let centers = [
        [
            eye_a[0] + v[0] * distance * 0.5,
            eye_a[1] + v[1] * distance * 0.5,
        ],
        [
            eye_b[0] + v[0] * distance * 0.5,
            eye_b[1] + v[1] * distance * 0.5,
        ],
        [
            mid[0] - v[0] * distance * 0.48,
            mid[1] - v[1] * distance * 0.48,
        ],
    ];
    let radius = distance * 0.29;
    let centers: Vec<_> = centers
        .into_iter()
        .filter(|point| inside(*point, radius))
        .collect();
    let mut candidates = Vec::new();
    for center in &centers {
        for oy in [-0.08, 0.0, 0.08] {
            for ox in [-0.08, 0.0, 0.08] {
                let point = [center[0] + ox * distance, center[1] + oy * distance];
                if let Some(sample) = sample_quality(rgb, width, height, point) {
                    candidates.push(sample);
                }
            }
        }
    }
    let Some((sample, [texture, tone, light])) = representative_sample(&candidates) else {
        return FacePlan::skip("No representative skin patch was available inside the face; patches may be clipped, too dark or highly textured.");
    };
    let _ = centers;
    let mask = face_skin_mask(
        FaceFrame {
            eyes: [eye_a, eye_b],
            nose,
            mouth: [mouth_a, mouth_b],
            mid,
            mouth_centre: mouth,
            u,
            v,
            d: distance,
        },
        [w, h],
    );
    let region = [
        (left + right) * 0.5,
        (top + bottom) * 0.5,
        (right - left) * 0.5,
        (bottom - top) * 0.5,
    ];
    let edits = [
        (Tool::SkinSmooth, texture, "texture"),
        (Tool::SkinUniformity, tone, "tone"),
        (Tool::PortraitDodgeBurn, light, "light"),
    ]
    .into_iter()
    .map(|(tool, amount, name)| {
        // Smoothing works on the band between pores and facial form: a small radius keeps
        // the form, 0.9 replaces most uneven mid-scale texture, and 70 % of the finest
        // detail (pores) is kept so skin never turns plastic. Colour evening and local light
        // look at broader blotches, so they keep the wider radius.
        let smooth = tool == Tool::SkinSmooth;
        Edit {
            id: format!("{PREFIX}{index}-{name}"),
            tool,
            enabled: true,
            region,
            source: Some([sample.point[0] / w, sample.point[1] / h]),
            amount,
            feather: 0.6,
            radius: if smooth {
                (distance * 0.015 / short).clamp(0.0005, 0.012)
            } else {
                (distance * 0.045 / short).clamp(0.001, 0.012)
            },
            texture: if smooth { 0.7 } else { 1.0 },
            tone: if smooth { 0.9 } else { 0.5 },
            warmth: 0.0,
            tint: 0.0,
            mask: Some(mask.clone()),
            // JPEG chroma varies across one cheek by more than 0.07; 0.13 still rejects lips,
            // brows, hair and background, and the mask already erases eyes and mouth.
            skin: Some(SkinSettings {
                tolerance: 0.13,
                edge_protection: 0.9,
                connected: false,
            }),
            selection: None,
            matte: None,
        }
    })
    .collect();
    FacePlan {
        edits,
        sample: Some(sample),
        reason: format!("Compared {} cheek/forehead patches. Selected a representative low-variation sample; strengths follow this face's texture, color variation and lighting. Eyes and mouth remain excluded.{}", candidates.len(), if sample.mean < 0.15 { " Low skin signal reduces correction strength." } else { "" }),
    }
}

/// Visible body skin below and beside a face: neck, shoulders, chest and arms.
///
/// There is no body segmentation model. The search area is drawn from the face's own size
/// and position, and inside it only pixels close to a sample of *this person's* body skin
/// are changed: the sample is taken below the face and must match the face's own skin
/// colour, so it is never compared with an ideal tone. The face itself is erased from the
/// mask so face and body are never smoothed twice.
// Pixel-space geometry; every coordinate is clamped to the image before use.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn plan_body(
    face: &PortraitFace,
    index: usize,
    rgb: &[u8],
    width: u32,
    height: u32,
    face_sample: &Sample,
) -> FacePlan {
    let (w, h) = (width as f32, height as f32);
    let short = w.min(h);
    let [l, t, r, b] = face.bounds;
    let (fw, fh) = ((r - l) * w, (b - t) * h);
    let cx = (l + r) * 0.5 * w;
    let cy = (t + b) * 0.5 * h;
    let x0 = (cx - fw * 2.6).max(0.0);
    let x1 = (cx + fw * 2.6).min(w);
    let y0 = (b * h - fh * 0.1).max(0.0);
    let y1 = (b * h + fh * 6.0).min(h);
    if y1 - y0 < fh * 0.5 || x1 - x0 < fw {
        return FacePlan::skip(
            "Body skin: the face reaches the bottom of the frame, so no body skin is visible.",
        );
    }
    let Some(region) = body_skin_region(face, rgb, width, height, face_sample, [x0, y0, x1, y1])
    else {
        return FacePlan::skip(
            "Body skin: no visible skin connected to the neck matches this person's face (covered by clothing, hair or out of frame).",
        );
    };
    // Measure the region's own texture and colour spread for the strengths.
    let [gw, _] = region.grid;
    let candidates: Vec<Sample> = region
        .cells
        .iter()
        .enumerate()
        .filter(|(_, selected)| **selected)
        .step_by(3)
        .filter_map(|(k, _)| {
            let point = [
                (region.origin[0] + (k % gw) * region.cell + region.cell / 2) as f32,
                (region.origin[1] + (k / gw) * region.cell + region.cell / 2) as f32,
            ];
            sample_quality(rgb, width, height, point)
        })
        .take(400)
        .collect();
    let Some((sample, [texture, tone, _])) = representative_sample(&candidates) else {
        return FacePlan::skip("Body skin: no representative skin patch was found.");
    };
    let selected = region.cells.iter().filter(|c| **c).count();
    let mut strokes = region.strokes(w, h);
    // The face is retouched by its own operations; never smooth it twice.
    strokes.push(BrushStroke {
        erase: true,
        radius: ((fw.max(fh) * 0.55) / short).clamp(0.0005, 0.25),
        opacity: 1.0,
        points: vec![[(cx / w).clamp(0.0, 1.0), (cy / h).clamp(0.0, 1.0), 1.0]],
    });
    let mask = BrushMask { strokes };
    let region = [
        ((x0 + x1) * 0.5 / w).clamp(0.0, 1.0),
        ((y0 + y1) * 0.5 / h).clamp(0.0, 1.0),
        ((x1 - x0) * 0.5 / w).clamp(0.001, 1.0),
        ((y1 - y0) * 0.5 / h).clamp(0.001, 1.0),
    ];
    // Body skin carries less retouch than a face: no make-up, and smoothing reads sooner.
    let edits = [
        (Tool::SkinSmooth, texture * 0.8, "body-texture"),
        (Tool::SkinUniformity, tone * 0.9, "body-tone"),
    ]
    .into_iter()
    .map(|(tool, amount, name)| Edit {
        id: format!("{PREFIX}{index}-{name}"),
        tool,
        enabled: true,
        region,
        source: Some([sample.point[0] / w, sample.point[1] / h]),
        amount,
        feather: 0.8,
        radius: if tool == Tool::SkinSmooth {
            (fw * 0.012 / short).clamp(0.0005, 0.012)
        } else {
            (fw * 0.03 / short).clamp(0.001, 0.012)
        },
        texture: if tool == Tool::SkinSmooth { 0.75 } else { 1.0 },
        tone: if tool == Tool::SkinSmooth { 0.9 } else { 0.5 },
        warmth: 0.0,
        tint: 0.0,
        mask: Some(mask.clone()),
        skin: Some(SkinSettings {
            tolerance: 0.12,
            edge_protection: 0.9,
            connected: false,
        }),
        selection: None,
        matte: None,
    })
    .collect();
    FacePlan {
        edits,
        sample: Some(sample),
        reason: format!(
            "Body skin: selected {selected} skin cells connected to the neck and matching this person's own skin; clothing, hair and background are not part of the mask."
        ),
    }
}

/// This person's visible body skin as a set of grid cells, found by growing from the neck.
///
/// Colour alone cannot separate skin from a pink wall, a beige dress or a wooden table, so
/// a cell joins only when (1) its colour, brightness and saturation are close to the
/// person's own neck skin (itself checked against their face), and (2) it is connected to
/// the neck through neighbours without crossing an edge in brightness or colour. Holes inside
/// the region (a necklace, a shadow) are filled; nothing outside it is ever selected.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]
fn body_skin_region(
    face: &PortraitFace,
    rgb: &[u8],
    width: u32,
    height: u32,
    face_sample: &Sample,
    area: [f32; 4],
) -> Option<BodyRegion> {
    const TOLERANCE: f32 = 0.055;
    let (w, h) = (width as usize, height as usize);
    let [x0, y0, x1, y1] = area.map(|v| v.max(0.0) as usize);
    let (x1, y1) = (x1.min(w), y1.min(h));
    if x1 <= x0 + 4 || y1 <= y0 + 4 {
        return None;
    }
    let cell = ((x1 - x0).max(y1 - y0) / 160).max(2);
    let (gw, gh) = ((x1 - x0) / cell, (y1 - y0) / cell);
    if gw < 3 || gh < 3 {
        return None;
    }
    let decode = |v: u8| aura_raw::colour::curve::srgb_decode(f32::from(v) / 255.0);
    let luma = |p: [f32; 3]| p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593;
    let chroma = |p: [f32; 3]| {
        let s = (p[0] + p[1] + p[2]).max(1e-6);
        [p[0] / s, p[1] / s, p[2] / s]
    };
    let sat = |p: [f32; 3]| {
        let max = p[0].max(p[1]).max(p[2]).max(1e-6);
        (max - p[0].min(p[1]).min(p[2])) / max
    };
    let dist = |a: [f32; 3], b: [f32; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let mut means = vec![[0.0_f32; 3]; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let mut sum = [0.0_f32; 3];
            for y in y0 + gy * cell..y0 + (gy + 1) * cell {
                for x in x0 + gx * cell..x0 + (gx + 1) * cell {
                    let i = (y * w + x) * 3;
                    if let Some(p) = rgb.get(i..i + 3) {
                        for c in 0..3 {
                            sum[c] += decode(p[c]);
                        }
                    }
                }
            }
            if let Some(slot) = means.get_mut(gy * gw + gx) {
                *slot = sum.map(|v| v / (cell * cell) as f32);
            }
        }
    }
    // The face's own skin, in linear light, around the representative cheek sample.
    let face_rgb = {
        let (fx, fy) = (face_sample.point[0] as usize, face_sample.point[1] as usize);
        let mut sum = [0.0_f32; 3];
        let mut n = 0.0;
        for y in fy.saturating_sub(2)..(fy + 3).min(h) {
            for x in fx.saturating_sub(2)..(fx + 3).min(w) {
                if let Some(p) = rgb.get((y * w + x) * 3..(y * w + x) * 3 + 3) {
                    for c in 0..3 {
                        sum[c] += decode(p[c]);
                    }
                    n += 1.0;
                }
            }
        }
        sum.map(|v| v / f32::max(n, 1.0))
    };
    let (face_c, face_l, face_s) = (chroma(face_rgb), luma(face_rgb), sat(face_rgb));
    let at = |k: usize| means.get(k).copied().unwrap_or([0.0; 3]);
    let like = |p: [f32; 3], c: [f32; 3], l: f32, s: f32, tol: f32| {
        dist(chroma(p), c) < tol
            && luma(p) > l * 0.4
            && luma(p) < l * 2.0
            && (sat(p) - s).abs() < 0.2
    };
    // Seeds: skin-like cells in the neck zone, from the chin down one face height.
    let [l, _, r, b] = face.bounds;
    let fw = (r - l) * width as f32;
    let fh = (face.bounds[3] - face.bounds[1]) * height as f32;
    let cx = (l + r) * 0.5 * width as f32;
    let to_gx = |x: f32| ((x - x0 as f32) / cell as f32).clamp(0.0, (gw - 1) as f32) as usize;
    let to_gy = |y: f32| ((y - y0 as f32) / cell as f32).clamp(0.0, (gh - 1) as f32) as usize;
    let (sy0, sy1) = (to_gy(b * height as f32), to_gy(b * height as f32 + fh));
    let (sx0, sx1) = (to_gx(cx - fw * 0.6), to_gx(cx + fw * 0.6));
    let mut seeds: Vec<usize> = (sy0..=sy1)
        .flat_map(|gy| (sx0..=sx1).map(move |gx| gy * gw + gx))
        .filter(|&k| like(at(k), face_c, face_l, face_s, TOLERANCE))
        .collect();
    if seeds.is_empty() {
        return None;
    }
    // Body skin is judged against the neck's own colour (no make-up, different light).
    let neck = {
        let n = seeds.len() as f32;
        let sum = seeds.iter().fold([0.0_f32; 3], |acc, &k| {
            let p = at(k);
            [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
        });
        sum.map(|v| v / n)
    };
    let (neck_c, neck_l, neck_s) = (chroma(neck), luma(neck), sat(neck));
    let usable: Vec<bool> = (0..gw * gh)
        .map(|k| {
            let p = at(k);
            like(p, neck_c, neck_l, neck_s, TOLERANCE) && dist(chroma(p), face_c) < TOLERANCE * 1.6
        })
        .collect();
    seeds.retain(|&k| usable.get(k).copied().unwrap_or(false));
    let mut reached = vec![false; gw * gh];
    let mut stack = seeds.clone();
    for &k in &seeds {
        if let Some(slot) = reached.get_mut(k) {
            *slot = true;
        }
    }
    while let Some(k) = stack.pop() {
        let (gx, gy) = (k % gw, k / gw);
        let here = at(k);
        let mut next = Vec::with_capacity(4);
        if gx > 0 {
            next.push(k - 1);
        }
        if gx + 1 < gw {
            next.push(k + 1);
        }
        if gy > 0 {
            next.push(k - gw);
        }
        if gy + 1 < gh {
            next.push(k + gw);
        }
        for n in next {
            if reached.get(n).copied().unwrap_or(true) || !usable.get(n).copied().unwrap_or(false) {
                continue;
            }
            let there = at(n);
            let step = (luma(here) - luma(there)).abs() / luma(here).max(luma(there)).max(1e-6);
            if step < 0.15 && dist(chroma(here), chroma(there)) < 0.035 {
                if let Some(slot) = reached.get_mut(n) {
                    *slot = true;
                }
                stack.push(n);
            }
        }
    }
    // Fill holes: anything not reachable from the border through unselected cells.
    let mut outside = vec![false; gw * gh];
    let mut stack: Vec<usize> = (0..gw * gh)
        .filter(|&k| {
            let (gx, gy) = (k % gw, k / gw);
            (gx == 0 || gy == 0 || gx + 1 == gw || gy + 1 == gh)
                && !reached.get(k).copied().unwrap_or(false)
        })
        .collect();
    for &k in &stack {
        if let Some(slot) = outside.get_mut(k) {
            *slot = true;
        }
    }
    while let Some(k) = stack.pop() {
        let (gx, gy) = (k % gw, k / gw);
        let mut next = Vec::with_capacity(4);
        if gx > 0 {
            next.push(k - 1);
        }
        if gx + 1 < gw {
            next.push(k + 1);
        }
        if gy > 0 {
            next.push(k - gw);
        }
        if gy + 1 < gh {
            next.push(k + gw);
        }
        for n in next {
            if !outside.get(n).copied().unwrap_or(true) && !reached.get(n).copied().unwrap_or(false)
            {
                if let Some(slot) = outside.get_mut(n) {
                    *slot = true;
                }
                stack.push(n);
            }
        }
    }
    let cells: Vec<bool> = outside.iter().map(|o| !o).collect();
    let count = cells.iter().filter(|c| **c).count();
    if count < 6 {
        return None;
    }
    Some(BodyRegion {
        cells,
        grid: [gw, gh],
        origin: [x0, y0],
        cell,
        seed: seeds.first().map(|&k| {
            [
                (x0 + (k % gw) * cell + cell / 2) as f32,
                (y0 + (k / gw) * cell + cell / 2) as f32,
            ]
        }),
    })
}

/// A body-skin region on a grid of square cells in planning pixels.
struct BodyRegion {
    cells: Vec<bool>,
    grid: [usize; 2],
    origin: [usize; 2],
    cell: usize,
    seed: Option<[f32; 2]>,
}

impl BodyRegion {
    /// The region as editable horizontal brush strokes, one per run of selected cells. Rows
    /// are merged in pairs, threes and so on until the run count fits the stroke limit.
    #[allow(clippy::cast_precision_loss)]
    fn strokes(&self, width: f32, height: f32) -> Vec<BrushStroke> {
        let short = width.min(height);
        let [gw, gh] = self.grid;
        let runs_for = |group: usize| {
            let mut runs = Vec::new();
            for top in (0..gh).step_by(group) {
                let filled = |gx: usize| {
                    (top..(top + group).min(gh))
                        .any(|gy| self.cells.get(gy * gw + gx).copied().unwrap_or(false))
                };
                let mut gx = 0;
                while gx < gw {
                    if filled(gx) {
                        let start = gx;
                        while gx < gw && filled(gx) {
                            gx += 1;
                        }
                        runs.push((top, start, gx));
                    } else {
                        gx += 1;
                    }
                }
            }
            runs
        };
        let mut group = 1;
        let mut runs = runs_for(group);
        while runs.len() > retouch_tools::MAX_STROKES - 2 && group < gh {
            group += 1;
            runs = runs_for(group);
        }
        let cell = self.cell as f32;
        let radius = cell * group as f32 * 0.5 + cell * 0.25;
        runs.into_iter()
            .map(|(top, a, b)| {
                let y = self.origin[1] as f32 + top as f32 * cell + cell * group as f32 * 0.5;
                let xa = self.origin[0] as f32 + a as f32 * cell + radius;
                let xb = (self.origin[0] as f32 + b as f32 * cell - radius).max(xa);
                BrushStroke {
                    erase: false,
                    radius: (radius / short).clamp(0.0005, 0.25),
                    opacity: 1.0,
                    points: vec![
                        [
                            (xa / width).clamp(0.0, 1.0),
                            (y / height).clamp(0.0, 1.0),
                            1.0,
                        ],
                        [
                            (xb / width).clamp(0.0, 1.0),
                            (y / height).clamp(0.0, 1.0),
                            1.0,
                        ],
                    ],
                }
            })
            .collect()
    }
}

/// The face in planning-pixel space: eye axis `u`, downward axis `v`, eye distance `d`.
#[derive(Clone, Copy)]
struct FaceFrame {
    eyes: [[f32; 2]; 2],
    nose: [f32; 2],
    mouth: [[f32; 2]; 2],
    mid: [f32; 2],
    mouth_centre: [f32; 2],
    u: [f32; 2],
    v: [f32; 2],
    d: f32,
}

/// Skin of the whole face - forehead, cheeks, nose, jaw and chin - as editable brush strokes,
/// with eyes, brows, lips and nostrils erased. Hair, beard and background are left to the
/// sampled-skin selection, which only changes pixels close to this person's own skin.
#[allow(clippy::many_single_char_names)]
fn face_skin_mask(f: FaceFrame, [width, height]: [f32; 2]) -> BrushMask {
    let short = width.min(height);
    let at = |p: [f32; 2], du: f32, dv: f32| {
        [
            p[0] + f.u[0] * du * f.d + f.v[0] * dv * f.d,
            p[1] + f.u[1] * du * f.d + f.v[1] * dv * f.d,
        ]
    };
    let stroke = |a: [f32; 2], b: [f32; 2], r: f32, erase: bool| {
        let point = |[x, y]: [f32; 2]| {
            [
                (x / width).clamp(0.0, 1.0),
                (y / height).clamp(0.0, 1.0),
                1.0,
            ]
        };
        let mut points = vec![point(a)];
        if (a[0] - b[0]).hypot(a[1] - b[1]) > 0.5 {
            points.push(point(b));
        }
        BrushStroke {
            erase,
            radius: (r * f.d / short).clamp(0.0005, 0.25),
            opacity: 1.0,
            points,
        }
    };
    let [eye_a, eye_b] = f.eyes;
    // The face oval, filled with overlapping horizontal strokes from the upper forehead to the
    // chin. Rows are 0.1 eye-distances apart with a 0.14 radius, so neighbouring rows overlap
    // past their feathering and the coverage is even - no bands of stronger and weaker retouch.
    let centre = at(f.mid, 0.0, 0.3);
    let (half_width, half_height) = (0.82, 1.05);
    let mut strokes: Vec<BrushStroke> = (0..=20)
        .filter_map(|row| {
            let dy = -half_height + row as f32 * 0.1;
            let fraction = 1.0 - (dy / half_height).powi(2);
            (fraction > 0.05).then(|| {
                let span = (half_width * fraction.sqrt() - 0.14).max(0.0);
                stroke(at(centre, -span, dy), at(centre, span, dy), 0.14, false)
            })
        })
        .collect();
    strokes.extend([
        // Eyes with lashes, brows, lips and nostrils stay exactly as they are.
        stroke(at(eye_a, -0.15, 0.0), at(eye_a, 0.15, 0.0), 0.11, true),
        stroke(at(eye_b, -0.15, 0.0), at(eye_b, 0.15, 0.0), 0.11, true),
        stroke(at(eye_a, -0.18, -0.3), at(eye_a, 0.18, -0.3), 0.08, true),
        stroke(at(eye_b, -0.18, -0.3), at(eye_b, 0.18, -0.3), 0.08, true),
        stroke(f.mouth[0], f.mouth[1], 0.14, true),
        stroke(at(f.nose, -0.09, 0.04), at(f.nose, -0.09, 0.04), 0.05, true),
        stroke(at(f.nose, 0.09, 0.04), at(f.nose, 0.09, 0.04), 0.05, true),
    ]);
    BrushMask { strokes }
}

// Bounds checked before integer conversion; sampled positions are within the image.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn sample_quality(rgb: &[u8], width: u32, height: u32, [x, y]: [f32; 2]) -> Option<Sample> {
    if x < 2.0 || y < 2.0 || x >= width as f32 - 2.0 || y >= height as f32 - 2.0 {
        return None;
    }
    let mut low = 1.0_f32;
    let mut high = 0.0_f32;
    let mut sum = 0.0;
    let mut color = [0.0; 3];
    for yy in y as usize - 2..=y as usize + 2 {
        for xx in x as usize - 2..=x as usize + 2 {
            let start = (yy * width as usize + xx) * 3;
            let pixel = rgb.get(start..start + 3)?;
            if pixel.iter().any(|v| *v > 246) {
                return None;
            }
            for (sum, channel) in color.iter_mut().zip(pixel) {
                *sum += f32::from(*channel);
            }
            let l = pixel
                .iter()
                .zip([0.2126, 0.7152, 0.0722])
                .map(|(v, k)| f32::from(*v) * k / 255.0)
                .sum::<f32>();
            low = low.min(l);
            high = high.max(l);
            sum += l;
        }
    }
    let mean = sum / 25.0;
    let variation = high - low;
    if mean < 0.06 || variation > 0.3 {
        return None;
    }
    let total = color.iter().sum::<f32>().max(1.0);
    Some(Sample {
        point: [x, y],
        mean,
        variation,
        chroma: color.map(|v| v / total),
    })
}

// ---- Segmented skin (ADR-0077) -----------------------------------------------------------

/// The segmenter's answer for this pass, or why there is none.
struct Segmentation {
    people: Vec<skin::Person>,
    background: Option<skin::Matte>,
    summary: SegmentationSummary,
}

impl Segmentation {
    fn finding(&self, index: usize) -> String {
        if let Some(reason) = &self.summary.unavailable {
            return format!(
                "Skin detection: {reason}; face skin follows the landmarks and this person's sampled skin colour instead."
            );
        }
        match self.summary.people.get(index) {
            Some([face, body, ..]) if *face > 0.0 => format!(
                "Skin detection: AI segmentation selected this person's face skin ({:.1}% of the frame){}; edges follow the photograph, and beard, brows, eyes and lips are left out.",
                face * 100.0,
                if *body > 0.0 {
                    format!(" and body skin ({:.1}%)", body * 100.0)
                } else {
                    String::new()
                }
            ),
            _ => "Skin detection: the segmenter found no face skin for this face; face skin follows the landmarks and sampled skin colour instead.".into(),
        }
    }
}

/// Run the bundled person segmenter on the larger rendition when there is one.
fn segment(
    rgb: &[u8],
    width: u32,
    height: u32,
    detail: Option<(&[u8], u32, u32)>,
    faces: &[PortraitFace],
    settings: &Settings,
) -> Segmentation {
    let mut summary = SegmentationSummary {
        model: skin::VERSION.into(),
        model_hash: skin::MODEL_HASH.into(),
        ..SegmentationSummary::default()
    };
    let unavailable = |summary: SegmentationSummary, reason: &str| Segmentation {
        people: Vec::new(),
        background: None,
        summary: SegmentationSummary {
            unavailable: Some(reason.into()),
            ..summary
        },
    };
    if !settings.ai_skin_detection {
        return unavailable(summary, "AI skin detection is switched off in the settings");
    }
    if std::env::var_os("AURA_DISABLE_SKIN_SEGMENTATION").is_some_and(|v| v == "1") {
        return unavailable(summary, "AI skin detection is disabled on this device");
    }
    if faces.is_empty() {
        return Segmentation {
            people: Vec::new(),
            background: None,
            summary,
        };
    }
    let (pixels, w, h) = detail.unwrap_or((rgb, width, height));
    let options = skin::Options {
        precision: settings.mask_precision,
        softness: settings.edge_softness,
        max_crops: 6,
        protect_dark_hair: settings.protect_facial_hair,
    };
    match skin::analyse(pixels, w, h, faces, options) {
        Ok(analysis) => {
            summary.passes = analysis.passes;
            summary.people = analysis
                .people
                .iter()
                .map(|p| {
                    [&p.face, &p.body, &p.hair, &p.clothes]
                        .map(|m| m.as_ref().map_or(0.0, skin::Matte::area))
                })
                .collect();
            Segmentation {
                people: analysis.people,
                background: analysis.background,
                summary,
            }
        }
        Err(error) => unavailable(
            summary,
            &format!("AI skin detection failed ({})", error.code),
        ),
    }
}

/// Mean local detail of a matte's fully covered area, in encoded luminance: about 0.01 on
/// studio paper, several times that on brick, foliage or a room.
const PLAIN_BACKDROP: f32 = 0.025;

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn matte_texture(m: &skin::Matte, rgb: &[u8], width: u32, height: u32) -> f32 {
    let (w, h) = (width as usize, height as usize);
    let luma = |x: usize, y: usize| -> f32 {
        let i = (y.min(h - 1) * w + x.min(w - 1)) * 3;
        rgb.get(i..i + 3).map_or(0.0, |p| {
            (f32::from(p[0]) * 0.2126 + f32::from(p[1]) * 0.7152 + f32::from(p[2]) * 0.0722) / 255.0
        })
    };
    let [l, t, r, b] = m.bounds;
    let (mut sum, mut n) = (0.0_f32, 0.0_f32);
    for k in (0..m.alpha.len()).step_by(3) {
        if m.alpha.get(k).is_none_or(|a| *a < 250) {
            continue;
        }
        let x = ((l + (r - l) * ((k % m.width) as f32 + 0.5) / m.width as f32) * w as f32) as usize;
        let y =
            ((t + (b - t) * ((k / m.width) as f32 + 0.5) / m.height as f32) * h as f32) as usize;
        if x < 1 || y < 1 || x + 1 >= w || y + 1 >= h {
            continue;
        }
        let around = (luma(x - 1, y) + luma(x + 1, y) + luma(x, y - 1) + luma(x, y + 1)) * 0.25;
        sum += (luma(x, y) - around).abs();
        n += 1.0;
    }
    if n < 50.0 {
        f32::INFINITY
    } else {
        sum / n
    }
}

/// The matte shrunk by `cells` on every side (a minimum filter), so an operation that reads
/// pixels around itself never reaches what lies outside the matte.
fn erode(m: &skin::Matte, cells: usize) -> skin::Matte {
    let (w, h) = (m.width, m.height);
    let mut rows = vec![0_u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let lo = x.saturating_sub(cells);
            let hi = (x + cells + 1).min(w);
            rows[y * w + x] = (lo..hi)
                .filter_map(|xx| m.alpha.get(y * w + xx).copied())
                .min()
                .unwrap_or(0);
        }
    }
    let mut alpha = vec![0_u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let lo = y.saturating_sub(cells);
            let hi = (y + cells + 1).min(h);
            alpha[y * w + x] = (lo..hi)
                .filter_map(|yy| rows.get(yy * w + x).copied())
                .min()
                .unwrap_or(0);
        }
    }
    skin::Matte { alpha, ..m.clone() }
}

/// A matte stored in the plan, as the planner refers to it.
#[derive(Debug, Clone)]
struct MatteUse {
    id: String,
    matte: skin::Matte,
}

/// Store a segmenter matte under a stable id, unless it selects almost nothing.
fn store(
    mattes: &mut BTreeMap<String, Matte>,
    matte: Option<&skin::Matte>,
    index: usize,
    kind: &str,
) -> Option<MatteUse> {
    let matte = matte?;
    if matte.area() < 0.0004 || matte.width == 0 || matte.height == 0 {
        return None;
    }
    let id = if kind == "background" {
        format!("{PREFIX}background")
    } else {
        format!("{PREFIX}{index}-{kind}")
    };
    mattes.insert(
        id.clone(),
        Matte::encode(
            matte.bounds,
            u32::try_from(matte.width).ok()?,
            u32::try_from(matte.height).ok()?,
            &matte.alpha,
        ),
    );
    Some(MatteUse {
        id,
        matte: matte.clone(),
    })
}

/// Brush strokes whose fully covered core spans `bounds`, for an edit with `feather`. The
/// matte provides the soft edge, so the brush only has to reach everywhere it might be.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn cover(bounds: [f32; 4], [w, h]: [f32; 2], feather: f32) -> Vec<BrushStroke> {
    let short = w.min(h);
    let [l, t, r, b] = bounds;
    let (left, top, right, bottom) = (l * w, t * h, r * w, b * h);
    let core = (1.0 - feather).clamp(0.2, 1.0);
    let radius = (0.25 * short).min(((bottom - top).max(right - left) * 0.5 / core).max(2.0));
    let band = 2.0 * radius * core * 0.9;
    let rows = ((bottom - top) / band).ceil().max(0.0) as usize + 1;
    (0..rows)
        .map(|k| {
            let y = (top + k as f32 * band).min(bottom);
            BrushStroke {
                erase: false,
                radius: (radius / short).clamp(0.0005, 0.25),
                opacity: 1.0,
                points: vec![
                    [(left / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0), 1.0],
                    [(right / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0), 1.0],
                ],
            }
        })
        .collect()
}

/// Limit a face edit to the segmented face skin: the brush covers the matte's grid, the
/// landmark exclusions (eyes, brows, lips, nostrils) are kept, and colour sampling is no
/// longer needed to find the skin.
fn use_matte(edit: &mut Edit, matte: &MatteUse, size: [f32; 2]) {
    let erase: Vec<BrushStroke> = edit
        .mask
        .as_ref()
        .map(|m| m.strokes.iter().filter(|s| s.erase).cloned().collect())
        .unwrap_or_default();
    let mut strokes = cover(matte.matte.bounds, size, edit.feather);
    strokes.extend(erase);
    edit.mask = Some(BrushMask { strokes });
    edit.matte = Some(matte.id.clone());
    edit.skin = None;
}

/// Scale the measured face-skin strengths by the photographer's settings.
fn tune_face(edits: Vec<Edit>, settings: &Settings) -> Vec<Edit> {
    edits
        .into_iter()
        .filter_map(|mut edit| {
            match edit.tool {
                Tool::SkinSmooth => {
                    edit.amount *= gain(settings.smoothing);
                    edit.texture = 0.4 + 0.6 * settings.texture;
                    edit.radius =
                        (edit.radius * (0.6 + 0.8 * settings.smoothing_size)).clamp(0.0005, 0.012);
                }
                Tool::SkinUniformity => edit.amount *= gain(settings.tone_evenness),
                Tool::PortraitDodgeBurn => edit.amount *= gain(settings.light_evenness),
                _ => {}
            }
            edit.amount = edit.amount.min(0.95);
            (edit.amount >= 0.02).then_some(edit)
        })
        .collect()
}

/// Scale the measured body-skin strengths of the colour-sampling fallback.
fn tune_body(mut plan: FacePlan, settings: &Settings) -> FacePlan {
    plan.edits.retain_mut(|edit| {
        edit.amount *= match edit.tool {
            Tool::SkinSmooth => gain(settings.body_smoothing),
            Tool::SkinUniformity => gain(settings.body_tone),
            _ => 1.0,
        };
        edit.amount = edit.amount.min(0.95);
        edit.amount >= 0.02
    });
    plan
}

fn stops_of(encoded_luma: f32, exposure: f32) -> f32 {
    let linear = aura_raw::colour::curve::srgb_decode(encoded_luma.clamp(0.0, 1.0)).max(1e-4);
    ((linear / 0.18).log2() + exposure).clamp(-15.0, 15.0)
}

fn brighter_than(stops: f32, softness: f32) -> Selection {
    Selection {
        inverted: false,
        gradient: None,
        luminance: Some(LuminanceRange {
            low: stops.clamp(-16.0, 16.0),
            high: 16.0,
            softness,
        }),
    }
}

/// Further face-skin operations from the settings, all limited to the segmented face skin.
/// The colour-sampling fallback gets none of them: without a matte they would reach hair
/// and background inside the brush.
fn face_extras(
    template: &Edit,
    index: usize,
    sample: &Sample,
    exposure: f32,
    settings: &Settings,
) -> Vec<Edit> {
    if template.matte.is_none() {
        return Vec::new();
    }
    let make = |name: &str, tool: Tool, amount: f32| -> Option<Edit> {
        (amount >= 0.01).then(|| Edit {
            id: format!("{PREFIX}{index}-{name}"),
            tool,
            amount: amount.min(0.95),
            texture: 1.0,
            tone: 0.5,
            warmth: 0.0,
            tint: 0.0,
            selection: None,
            source: if tool == Tool::Dodge || tool == Tool::Burn {
                None
            } else {
                template.source
            },
            ..template.clone()
        })
    };
    let mut out = Vec::new();
    if let Some(mut e) = make(
        "microdb",
        Tool::MicroDodgeBurn,
        settings.micro_dodge_burn * 0.6,
    ) {
        e.radius = (template.radius * 1.5).clamp(0.0005, 0.05);
        out.push(e);
    }
    if let Some(mut e) = make("pores", Tool::Frequency, settings.pore_refine * 0.8) {
        e.radius = (template.radius * 0.4).clamp(0.0005, 0.05);
        e.tone = 0.5;
        e.texture = 0.9;
        out.push(e);
    }
    if let Some(mut e) = make("glow", Tool::Dodge, settings.glow * 0.25) {
        e.selection = Some(brighter_than(stops_of(sample.mean, exposure), 0.8));
        out.push(e);
    }
    let brightness = settings.skin_brightness;
    if brightness.abs() >= 0.01 {
        let tool = if brightness > 0.0 {
            Tool::Dodge
        } else {
            Tool::Burn
        };
        out.extend(make("brightness", tool, brightness.abs() * 0.35));
    }
    if settings.skin_warmth.abs() >= 0.01 || settings.skin_tint.abs() >= 0.01 {
        if let Some(mut e) = make("colour", Tool::SkinColor, 0.8) {
            e.warmth = settings.skin_warmth * 0.6;
            e.tint = settings.skin_tint * 0.6;
            out.push(e);
        }
    }
    out.extend(make("facelight", Tool::Dodge, settings.face_light * 0.3));
    out
}

/// Face skin planned from the segmenter's matte alone, for faces the landmark sampler refused
/// (turned, tilted, partly covered or small): strengths are measured from the segmented skin,
/// and the eyes and mouth are still kept out by their landmarks.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn plan_face_from_matte(
    face: &PortraitFace,
    index: usize,
    rgb: &[u8],
    width: u32,
    height: u32,
    matte: &MatteUse,
    skipped: &str,
) -> FacePlan {
    let (w, h) = (width as f32, height as f32);
    let short = w.min(h);
    let m = &matte.matte;
    let [l, t, r, b] = m.bounds;
    let confident: Vec<usize> = (0..m.alpha.len())
        .filter(|&k| m.alpha.get(k).is_some_and(|a| *a >= 230))
        .collect();
    let stride = (confident.len() / 300).max(1);
    let candidates: Vec<Sample> = confident
        .iter()
        .step_by(stride)
        .filter_map(|&k| {
            let x = (l + (r - l) * ((k % m.width) as f32 + 0.5) / m.width as f32) * w;
            let y = (t + (b - t) * ((k / m.width) as f32 + 0.5) / m.height as f32) * h;
            sample_quality(rgb, width, height, [x, y])
        })
        .collect();
    let Some((sample, [texture, tone, light])) = representative_sample(&candidates) else {
        return FacePlan::skip(skipped);
    };
    let fw = (face.bounds[2] - face.bounds[0]) * w;
    let [eye_a, eye_b, nose, mouth_a, mouth_b] = face.landmarks.map(|[x, y]| [x * w, y * h]);
    // In a turned face the eye distance shrinks; the face width does not.
    let d = (eye_a[0] - eye_b[0])
        .hypot(eye_a[1] - eye_b[1])
        .max(fw * 0.4);
    let disk = |c: [f32; 2], radius: f32| BrushStroke {
        erase: true,
        radius: (radius / short).clamp(0.0005, 0.25),
        opacity: 1.0,
        points: vec![[(c[0] / w).clamp(0.0, 1.0), (c[1] / h).clamp(0.0, 1.0), 1.0]],
    };
    let mouth = BrushStroke {
        erase: true,
        radius: (0.14 * d / short).clamp(0.0005, 0.25),
        opacity: 1.0,
        points: vec![
            [
                (mouth_a[0] / w).clamp(0.0, 1.0),
                (mouth_a[1] / h).clamp(0.0, 1.0),
                1.0,
            ],
            [
                (mouth_b[0] / w).clamp(0.0, 1.0),
                (mouth_b[1] / h).clamp(0.0, 1.0),
                1.0,
            ],
        ],
    };
    let mask = BrushMask {
        strokes: vec![
            disk(eye_a, 0.13 * d),
            disk(eye_b, 0.13 * d),
            disk(nose, 0.05 * d),
            mouth,
        ],
    };
    let point = [
        (sample.point[0] / w).clamp(0.0, 1.0),
        (sample.point[1] / h).clamp(0.0, 1.0),
    ];
    let region = [
        point[0],
        point[1],
        ((r - l) * 0.5).clamp(0.001, 1.0),
        ((b - t) * 0.5).clamp(0.001, 1.0),
    ];
    let edits = [
        (Tool::SkinSmooth, texture, "texture"),
        (Tool::SkinUniformity, tone, "tone"),
        (Tool::PortraitDodgeBurn, light, "light"),
    ]
    .into_iter()
    .map(|(tool, amount, name)| {
        let smooth = tool == Tool::SkinSmooth;
        Edit {
            id: format!("{PREFIX}{index}-{name}"),
            tool,
            enabled: true,
            region,
            source: Some(point),
            amount,
            feather: 0.6,
            radius: if smooth {
                (d * 0.015 / short).clamp(0.0005, 0.012)
            } else {
                (d * 0.045 / short).clamp(0.001, 0.012)
            },
            texture: if smooth { 0.7 } else { 1.0 },
            tone: if smooth { 0.9 } else { 0.5 },
            warmth: 0.0,
            tint: 0.0,
            mask: Some(mask.clone()),
            skin: None,
            selection: None,
            matte: Some(matte.id.clone()),
        }
    })
    .collect();
    FacePlan {
        edits,
        sample: Some(sample),
        reason: format!(
            "Landmark sampling was not usable ({}), so skin texture, tone and light were measured from {} patches of the segmented face skin instead. Spots, eyes and teeth need a frontal face and were left alone.",
            skipped.trim_end_matches('.'),
            candidates.len()
        ),
    }
}

/// Body skin from the segmenter's matte: neck, shoulders, arms and hands of this person.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn plan_body_matte(
    face: &PortraitFace,
    index: usize,
    rgb: &[u8],
    width: u32,
    height: u32,
    face_sample: &Sample,
    matte: &MatteUse,
    settings: &Settings,
    exposure: f32,
) -> FacePlan {
    let (w, h) = (width as f32, height as f32);
    let short = w.min(h);
    let m = &matte.matte;
    let [l, t, r, b] = m.bounds;
    // Confident body-skin cells, measured on the planning pixels.
    let confident: Vec<usize> = (0..m.alpha.len())
        .filter(|&k| m.alpha.get(k).is_some_and(|a| *a >= 230))
        .collect();
    let stride = (confident.len() / 400).max(1);
    let candidates: Vec<Sample> = confident
        .iter()
        .step_by(stride)
        .filter_map(|&k| {
            let gx = (k % m.width) as f32 + 0.5;
            let gy = (k / m.width) as f32 + 0.5;
            let x = (l + (r - l) * gx / m.width as f32) * w;
            let y = (t + (b - t) * gy / m.height as f32) * h;
            sample_quality(rgb, width, height, [x, y])
        })
        .collect();
    let Some((sample, [texture, tone, _])) = representative_sample(&candidates) else {
        return FacePlan::skip("Body skin: the segmented body skin had no clean, evenly lit patch to measure strengths from.");
    };
    let fw = (face.bounds[2] - face.bounds[0]) * w;
    let fh = (face.bounds[3] - face.bounds[1]) * h;
    let feather = 0.6;
    let strokes = cover(m.bounds, [w, h], feather);
    let point = [
        (sample.point[0] / w).clamp(0.0, 1.0),
        (sample.point[1] / h).clamp(0.0, 1.0),
    ];
    // The region's centre is the measured skin sample: tools that compare a pixel with the
    // region centre (shine, colour) compare it with this person's own skin.
    let region = [
        point[0],
        point[1],
        ((r - l) * 0.5).clamp(0.001, 1.0),
        ((b - t) * 0.5).clamp(0.001, 1.0),
    ];
    let base = |name: &str, tool: Tool, amount: f32| -> Option<Edit> {
        (amount >= 0.02).then(|| Edit {
            id: format!("{PREFIX}{index}-{name}"),
            tool,
            enabled: true,
            region,
            source: Some(point),
            amount: amount.min(0.95),
            feather,
            radius: (fw * 0.03 / short).clamp(0.001, 0.012),
            texture: 1.0,
            tone: 0.5,
            warmth: 0.0,
            tint: 0.0,
            mask: Some(BrushMask {
                strokes: strokes.clone(),
            }),
            skin: None,
            selection: None,
            matte: Some(matte.id.clone()),
        })
    };
    let mut edits = Vec::new();
    // Body skin carries less retouch than a face: no make-up, and smoothing reads sooner.
    if let Some(mut e) = base(
        "body-texture",
        Tool::SkinSmooth,
        texture * 0.8 * gain(settings.body_smoothing),
    ) {
        e.radius = (fw * 0.012 / short).clamp(0.0005, 0.012);
        e.texture = 0.45 + 0.6 * settings.texture.min(0.9);
        e.tone = 0.9;
        edits.push(e);
    }
    edits.extend(base(
        "body-tone",
        Tool::SkinUniformity,
        tone * 0.9 * gain(settings.body_tone),
    ));
    if let Some(mut e) = base(
        "body-match",
        Tool::SkinUniformity,
        settings.match_body_to_face * 0.6,
    ) {
        // Toward the same person's face, never toward a reference complexion.
        e.source = Some([
            (face_sample.point[0] / w).clamp(0.0, 1.0),
            (face_sample.point[1] / h).clamp(0.0, 1.0),
        ]);
        edits.push(e);
    }
    if let Some(mut e) = base("body-shine", Tool::Mattify, settings.body_shine * 0.4) {
        e.source = None;
        e.selection = Some(brighter_than(stops_of(sample.mean, exposure) + 0.4, 0.6));
        edits.push(e);
    }
    if let Some(mut e) = base("body-redness", Tool::SkinColor, settings.body_redness * 0.7) {
        e.source = None;
        e.tint = -0.8;
        edits.push(e);
    }
    if let Some(mut e) = base("body-spots", Tool::AutoBlemish, settings.body_blemishes) {
        e.source = None;
        edits.push(e);
    }
    if settings.neck_lines > 0.0 {
        let cx = (face.bounds[0] + face.bounds[2]) * 0.5 * w;
        let chin = face.bounds[3] * h;
        let neck = BrushStroke {
            erase: false,
            radius: ((fw * 0.45) / short).clamp(0.0005, 0.25),
            opacity: 1.0,
            points: vec![
                [
                    (cx / w).clamp(0.0, 1.0),
                    ((chin + fh * 0.1) / h).clamp(0.0, 1.0),
                    1.0,
                ],
                [
                    (cx / w).clamp(0.0, 1.0),
                    ((chin + fh * 0.8) / h).clamp(0.0, 1.0),
                    1.0,
                ],
            ],
        };
        if let Some(mut e) = base("lines-neck", Tool::Wrinkle, settings.neck_lines * 0.6) {
            e.source = None;
            e.mask = Some(BrushMask {
                strokes: vec![neck],
            });
            e.feather = 0.8;
            e.tone = 0.8;
            e.radius = (fw * 0.015 / short).clamp(0.0005, 0.05);
            edits.push(e);
        }
    }
    FacePlan {
        edits,
        sample: Some(sample),
        reason: format!(
            "Body skin: AI segmentation selected this person's visible body skin ({:.1}% of the frame); strengths measured from {} of its patches. Clothing, hair and background are not part of it.",
            m.area() * 100.0,
            candidates.len()
        ),
    }
}

/// Hair detail and shine, limited to this person's segmented hair.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn hair_ops(
    index: usize,
    matte: &MatteUse,
    rgb: &[u8],
    width: u32,
    height: u32,
    exposure: f32,
    settings: &Settings,
) -> Vec<Edit> {
    let (w, h) = (width as f32, height as f32);
    let m = &matte.matte;
    let [l, t, r, b] = m.bounds;
    // Median brightness of the hair, so shine lifts only the hair's own highlights.
    let mut lumas: Vec<f32> = (0..m.alpha.len())
        .filter(|&k| m.alpha.get(k).is_some_and(|a| *a >= 200))
        .filter_map(|k| {
            let x = ((l + (r - l) * ((k % m.width) as f32 + 0.5) / m.width as f32) * w) as usize;
            let y = ((t + (b - t) * ((k / m.width) as f32 + 0.5) / m.height as f32) * h) as usize;
            let i = (y.min(height as usize - 1) * width as usize + x.min(width as usize - 1)) * 3;
            rgb.get(i..i + 3).map(|p| {
                (f32::from(p[0]) * 0.2126 + f32::from(p[1]) * 0.7152 + f32::from(p[2]) * 0.0722)
                    / 255.0
            })
        })
        .collect();
    if lumas.is_empty() {
        return Vec::new();
    }
    lumas.sort_by(f32::total_cmp);
    let median = lumas.get(lumas.len() / 2).copied().unwrap_or(0.2);
    let strokes = cover(m.bounds, [w, h], 0.6);
    let edit = |name: &str, tool: Tool, amount: f32| Edit {
        id: format!("{PREFIX}{index}-hair-{name}"),
        tool,
        enabled: true,
        region: [
            ((l + r) * 0.5).clamp(0.0, 1.0),
            ((t + b) * 0.5).clamp(0.0, 1.0),
            ((r - l) * 0.5).clamp(0.001, 1.0),
            ((b - t) * 0.5).clamp(0.001, 1.0),
        ],
        source: None,
        amount: amount.min(0.95),
        feather: 0.6,
        radius: 0.0015,
        texture: 1.0,
        tone: 0.0,
        warmth: 0.0,
        tint: 0.0,
        mask: Some(BrushMask {
            strokes: strokes.clone(),
        }),
        skin: None,
        selection: None,
        matte: Some(matte.id.clone()),
    };
    let mut out = Vec::new();
    if settings.hair_detail > 0.0 {
        let mut e = edit("detail", Tool::Frequency, 0.85);
        e.texture = 1.0 + settings.hair_detail * 0.9;
        out.push(e);
    }
    if settings.hair_shine > 0.0 {
        let mut e = edit("shine-lift", Tool::Dodge, settings.hair_shine * 0.3);
        e.selection = Some(brighter_than(stops_of(median, exposure) + 0.4, 0.6));
        out.push(e);
    }
    out
}

/// Clothing crease softening on this person's segmented clothes.
fn fabric_op(index: usize, matte: &MatteUse, face: &PortraitFace, settings: &Settings) -> Edit {
    let [l, t, r, b] = matte.matte.bounds;
    let face_height = face.bounds[3] - face.bounds[1];
    Edit {
        id: format!("{PREFIX}{index}-fabric"),
        tool: Tool::Fabric,
        enabled: true,
        region: [
            ((l + r) * 0.5).clamp(0.0, 1.0),
            ((t + b) * 0.5).clamp(0.0, 1.0),
            ((r - l) * 0.71).clamp(0.001, 1.0),
            ((b - t) * 0.71).clamp(0.001, 1.0),
        ],
        source: None,
        amount: (settings.fabric * 0.9).min(0.95),
        feather: 0.0,
        radius: (face_height * 0.04).clamp(0.002, 0.02),
        texture: 1.0,
        tone: 0.85,
        warmth: 0.0,
        tint: 0.0,
        mask: None,
        skin: None,
        selection: None,
        matte: Some(matte.id.clone()),
    }
}

/// Backdrop smoothing over the frame's segmented background.
fn backdrop_op(matte: &MatteUse, faces: &[PortraitFace], settings: &Settings) -> Edit {
    let [l, t, r, b] = matte.matte.bounds;
    let face_height = faces
        .iter()
        .map(|f| f.bounds[3] - f.bounds[1])
        .fold(0.05_f32, f32::max);
    Edit {
        id: format!("{PREFIX}backdrop"),
        tool: Tool::Backdrop,
        enabled: true,
        region: [
            ((l + r) * 0.5).clamp(0.0, 1.0),
            ((t + b) * 0.5).clamp(0.0, 1.0),
            ((r - l) * 0.71).clamp(0.001, 1.0),
            ((b - t) * 0.71).clamp(0.001, 1.0),
        ],
        source: None,
        amount: (settings.backdrop * 0.7).min(0.95),
        feather: 0.0,
        radius: (face_height * 0.03).clamp(0.002, 0.015),
        texture: 1.0,
        tone: 0.5,
        warmth: 0.0,
        tint: 0.0,
        mask: None,
        skin: None,
        selection: None,
        matte: Some(matte.id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consensus_rejects_color_outliers_and_adapts_to_each_faces_signal() {
        let calm = Sample {
            point: [20.0, 30.0],
            mean: 0.5,
            variation: 0.005,
            chroma: [0.45, 0.32, 0.23],
        };
        let outlier = Sample {
            point: [80.0, 30.0],
            mean: 0.5,
            variation: 0.0,
            chroma: [0.05, 0.05, 0.9],
        };
        let (sample, gentle) = representative_sample(&[calm, calm, calm, outlier]).unwrap();
        assert_eq!(sample.point, calm.point);
        let (_, textured) = representative_sample(
            &[Sample {
                variation: 0.15,
                ..calm
            }; 3],
        )
        .unwrap();
        assert!(textured[0] > gentle[0]);
        let (_, low_signal) = representative_sample(
            &[Sample {
                mean: 0.1,
                variation: 0.001,
                ..calm
            }; 3],
        )
        .unwrap();
        assert!(low_signal[0] < gentle[0]);
        let (_, uneven) = representative_sample(&[
            Sample { mean: 0.3, ..calm },
            calm,
            Sample { mean: 0.7, ..calm },
        ])
        .unwrap();
        assert!(uneven[2] > gentle[2]);
        assert!(representative_sample(&[]).is_none());
    }
    #[test]
    fn targeted_plan_is_valid_and_has_landmark_exclusions_for_different_complexions() {
        let face = PortraitFace {
            bounds: [0.1, 0.05, 0.9, 0.95],
            landmarks: [
                [0.3, 0.4],
                [0.7, 0.4],
                [0.5, 0.57],
                [0.36, 0.7],
                [0.64, 0.7],
            ],
            confidence: 0.95,
        };
        for color in [[72, 48, 38], [145, 101, 74], [218, 178, 154]] {
            let rgb = color.repeat(100 * 100);
            let edits = plan_face(&face, 0, &rgb, 100, 100).edits;
            assert_eq!(edits.len(), 3);
            retouch_tools::validate(&edits).unwrap();
            assert_eq!(
                edits[0]
                    .mask
                    .as_ref()
                    .unwrap()
                    .strokes
                    .iter()
                    .filter(|s| s.erase)
                    .count(),
                7
            );
            assert_eq!(edits, plan_face(&face, 0, &rgb, 100, 100).edits);
        }
        assert!(plan_face(&face, 0, &vec![255; 100 * 100 * 3], 100, 100)
            .edits
            .is_empty());
    }

    fn person(clothing: Option<[u8; 3]>) -> (Vec<u8>, PortraitFace) {
        let (w, h) = (200_usize, 300_usize);
        let skin = [145_u8, 101, 74];
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for _ in 0..w {
                rgb.extend(if y > 100 {
                    clothing.unwrap_or(skin)
                } else {
                    skin
                });
            }
        }
        let face = PortraitFace {
            bounds: [0.3, 0.05, 0.7, 0.35],
            landmarks: [
                [0.4, 0.15],
                [0.6, 0.15],
                [0.5, 0.21],
                [0.43, 0.27],
                [0.57, 0.27],
            ],
            confidence: 0.95,
        };
        (rgb, face)
    }

    fn scoped(rgb: &[u8], face: &PortraitFace, scope: portrait_features::Scope) -> Plan {
        let recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t");
        // These cases exercise the landmark and colour-sampling fallback on synthetic pixels,
        // which the person segmenter rightly does not recognise as people.
        let options = portrait_features::Options {
            scope,
            settings: Settings {
                ai_skin_detection: false,
                ..Settings::default()
            },
            ..portrait_features::Options::default()
        };
        plan_with_faces(
            &recipe,
            rgb,
            200,
            300,
            None,
            0.0,
            Some(vec![face.clone()]),
            &options,
            false,
        )
        .unwrap()
    }

    #[test]
    fn face_body_and_both_scopes_change_only_the_chosen_skin() {
        let (rgb, face) = person(None);
        let ids = |plan: &Plan| -> Vec<String> {
            plan.groups
                .values()
                .flatten()
                .map(|e| e.id.clone())
                .collect()
        };
        let face_only = ids(&scoped(&rgb, &face, portrait_features::Scope::Face));
        assert!(face_only
            .iter()
            .any(|id| id.ends_with("-texture") && !id.contains("body")));
        assert!(
            !face_only.iter().any(|id| id.contains("-body-")),
            "{face_only:?}"
        );
        let body_only = scoped(&rgb, &face, portrait_features::Scope::Body);
        let body_ids = ids(&body_only);
        assert_eq!(body_ids.len(), 2, "{body_ids:?}");
        assert!(body_ids.iter().all(|id| id.contains("-body-")));
        assert!(body_only
            .report
            .message
            .contains("faces were left as they are"));
        let both = ids(&scoped(&rgb, &face, portrait_features::Scope::FaceAndBody));
        assert!(
            both.iter().any(|id| id.contains("-body-"))
                && both.iter().any(|id| id.ends_with("0-texture"))
        );
        let all: Vec<Edit> = scoped(&rgb, &face, portrait_features::Scope::FaceAndBody)
            .groups
            .into_values()
            .flatten()
            .collect();
        retouch_tools::validate(&all).unwrap();
        // The face is erased from the body mask so it is never smoothed twice.
        let body = all
            .iter()
            .find(|e| e.id.ends_with("-body-texture"))
            .unwrap();
        assert!(body.mask.as_ref().unwrap().strokes.iter().any(|s| s.erase));
        assert_eq!(group_of(&body.id), Some(Group::Skin));
    }

    #[test]
    fn covered_body_is_skipped_with_a_reason() {
        let (rgb, face) = person(Some([30, 60, 140]));
        let plan = scoped(&rgb, &face, portrait_features::Scope::Body);
        assert!(plan.groups.values().all(Vec::is_empty));
        let findings = &plan.report.assessments[0].findings;
        assert!(
            findings.iter().any(|f| f.starts_with("Body skin:")),
            "{findings:?}"
        );
    }

    #[test]
    fn body_mask_never_reaches_a_skin_coloured_backdrop() {
        // Skin-coloured backdrop on both sides, separated from the person by dark outlines.
        let (w, h) = (200_usize, 300_usize);
        let skin = [150_u8, 105, 80];
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let outline = (44..52).contains(&x) || (148..156).contains(&x);
                rgb.extend(if outline { [20, 20, 25] } else { skin });
            }
        }
        let face = PortraitFace {
            bounds: [0.3, 0.05, 0.7, 0.35],
            landmarks: [
                [0.4, 0.15],
                [0.6, 0.15],
                [0.5, 0.21],
                [0.43, 0.27],
                [0.57, 0.27],
            ],
            confidence: 0.95,
        };
        let plan = scoped(&rgb, &face, portrait_features::Scope::Body);
        let body: Vec<&Edit> = plan
            .groups
            .values()
            .flatten()
            .filter(|e| e.id.contains("-body-"))
            .collect();
        assert!(!body.is_empty(), "{:?}", plan.report.assessments);
        for stroke in body[0]
            .mask
            .as_ref()
            .unwrap()
            .strokes
            .iter()
            .filter(|s| !s.erase)
        {
            for p in &stroke.points {
                let reach = stroke.radius * 200.0 / 200.0;
                assert!(
                    p[0] - reach > 0.2 && p[0] + reach < 0.8,
                    "stroke reaches the backdrop: {p:?} r={}",
                    stroke.radius
                );
            }
        }
    }

    fn square_matte(bounds: [f32; 4], cells: usize) -> skin::Matte {
        skin::Matte {
            bounds,
            width: cells,
            height: cells,
            alpha: vec![255; cells * cells],
        }
    }

    #[test]
    fn matte_operations_validate_store_and_change_only_the_selected_skin() {
        let (rgb, face) = person(None);
        let mut mattes = BTreeMap::new();
        let body = square_matte([0.2, 0.45, 0.8, 0.95], 40);
        let used = store(&mut mattes, Some(&body), 0, "body").unwrap();
        let sample = sample_quality(&rgb, 200, 300, [100.0, 60.0]).unwrap();
        let settings = Settings {
            body_redness: 0.5,
            body_blemishes: 0.5,
            neck_lines: 0.5,
            ..Settings::default()
        };
        let plan = plan_body_matte(&face, 0, &rgb, 200, 300, &sample, &used, &settings, 0.0);
        let ids: Vec<&str> = plan.edits.iter().map(|e| e.id.as_str()).collect();
        for want in [
            "0-body-texture",
            "0-body-match",
            "0-body-redness",
            "0-body-spots",
            "0-lines-neck",
        ] {
            assert!(
                ids.iter().any(|id| id.ends_with(want)),
                "{want} missing from {ids:?}"
            );
        }
        assert!(plan
            .edits
            .iter()
            .all(|e| e.matte.as_deref() == Some(used.id.as_str())));
        assert_eq!(
            group_of(&format!("{PREFIX}0-body-spots")),
            Some(Group::Blemishes)
        );
        assert_eq!(
            group_of(&format!("{PREFIX}0-lines-neck")),
            Some(Group::Refine)
        );
        let mut recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t");
        retouch_tools::write_with_mattes(&mut recipe, &plan.edits, &mattes).unwrap();
        aura_recipe::schema::Validation::check(&recipe).unwrap();
        assert_eq!(retouch_tools::read_mattes(&recipe).unwrap().len(), 1);
        // Removing every operation removes the matte with them.
        retouch_tools::write(&mut recipe, &[]).unwrap();
        assert!(retouch_tools::read_mattes(&recipe).unwrap().is_empty());
        // Rendering touches nothing outside the matte's grid.
        let mut linear: Vec<f32> = rgb.iter().map(|v| f32::from(*v) / 255.0).collect();
        let before = linear.clone();
        let stored: BTreeMap<String, Matte> = mattes;
        aura_render::retouch_tools::apply_with_mattes(&mut linear, 200, 300, &plan.edits, &stored);
        for y in 0..300 {
            for x in 0..200 {
                let i = (y * 200 + x) * 3;
                let inside = (40..160).contains(&x) && (135..285).contains(&y);
                if !inside {
                    assert_eq!(&linear[i..i + 3], &before[i..i + 3], "changed at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn settings_scale_measured_face_strengths_and_off_removes_them() {
        let face = PortraitFace {
            bounds: [0.1, 0.05, 0.9, 0.95],
            landmarks: [
                [0.3, 0.4],
                [0.7, 0.4],
                [0.5, 0.57],
                [0.36, 0.7],
                [0.64, 0.7],
            ],
            confidence: 0.95,
        };
        let rgb = [145_u8, 101, 74].repeat(100 * 100);
        let edits = plan_face(&face, 0, &rgb, 100, 100).edits;
        let natural = tune_face(edits.clone(), &Settings::default());
        let strong = tune_face(
            edits.clone(),
            &Settings {
                smoothing: 1.0,
                texture: 1.0,
                ..Settings::default()
            },
        );
        let smooth = |v: &[Edit]| v.iter().find(|e| e.tool == Tool::SkinSmooth).cloned();
        let (n, s) = (smooth(&natural).unwrap(), smooth(&strong).unwrap());
        assert!(s.amount > n.amount && s.texture > n.texture);
        let off = tune_face(
            edits,
            &Settings {
                smoothing: 0.0,
                tone_evenness: 0.0,
                light_evenness: 0.0,
                ..Settings::default()
            },
        );
        assert!(off.is_empty());
        // Extra face operations exist only with a matte: without one they would reach hair
        // and background inside the brush.
        let sample = sample_quality(&rgb, 100, 100, [50.0, 50.0]).unwrap();
        let glow = Settings {
            glow: 0.5,
            skin_warmth: 0.3,
            micro_dodge_burn: 0.5,
            ..Settings::default()
        };
        assert!(face_extras(&n, 0, &sample, 0.0, &glow).is_empty());
        let mut with = n.clone();
        with.matte = Some("m".into());
        let extra = face_extras(&with, 0, &sample, 0.0, &glow);
        assert_eq!(
            extra.len(),
            3,
            "{:?}",
            extra.iter().map(|e| &e.id).collect::<Vec<_>>()
        );
        retouch_tools::validate(&extra).unwrap();
    }

    #[test]
    fn cover_strokes_reach_every_corner_of_the_matte_and_erosion_shrinks_it() {
        let bounds = [0.1, 0.2, 0.7, 0.9];
        let strokes = cover(bounds, [400.0, 300.0], 0.6);
        let edit = Edit {
            id: "x".into(),
            tool: Tool::Dodge,
            enabled: true,
            region: [0.4, 0.55, 0.3, 0.35],
            source: None,
            amount: 1.0,
            feather: 0.6,
            radius: 0.002,
            texture: 1.0,
            tone: 0.5,
            warmth: 0.0,
            tint: 0.0,
            mask: Some(BrushMask { strokes }),
            skin: None,
            selection: None,
            matte: None,
        };
        let rgb = vec![0.2_f32; 400 * 300 * 3];
        let mask = aura_render::retouch_tools::selection_mask(&rgb, 400, 300, &edit);
        for [x, y] in [[0.1, 0.2], [0.7, 0.2], [0.1, 0.9], [0.7, 0.9], [0.4, 0.55]] {
            let (px, py) = (
                ((x * 400.0) as usize).min(399),
                ((y * 300.0) as usize).min(299),
            );
            assert!(
                mask[py * 400 + px] > 0.99,
                "{x},{y}: {}",
                mask[py * 400 + px]
            );
        }
        let mut m = square_matte([0.0, 0.0, 1.0, 1.0], 20);
        m.alpha[0] = 0;
        let eroded = erode(&m, 2);
        assert_eq!(eroded.alpha[2 * 20 + 2], 0);
        assert_eq!(eroded.alpha[3 * 20 + 3], 255);
    }
}
