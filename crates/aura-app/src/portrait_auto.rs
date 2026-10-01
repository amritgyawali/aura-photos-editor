//! Automatic portrait planning produces ordinary, reversible retouch operations.
use aura_core::AuraResult;
use aura_recipe::{
    retouch_tools::{self, BrushMask, BrushStroke, Edit, SkinSettings, Tool},
    Recipe,
};
use aura_vision::portrait::{self, PortraitFace};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::portrait_features;

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
    Some(if rest.contains("-spot-") {
        Group::Blemishes
    } else if rest.contains("-lines-") || rest.contains("-fold-") || rest.contains("-redness-") {
        Group::Refine
    } else if rest.contains("-eye-") || rest.contains("-undereye-") {
        Group::Eyes
    } else if rest.ends_with("-teeth") || rest.ends_with("-shine") {
        Group::Finishing
    } else {
        Group::Skin
    })
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
        retouch_tools::write(proposal, &staged(&current, &plan.groups, Group::Finishing))?;
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
    };
    let mut groups = BTreeMap::new();
    if disabled || protected {
        report.status = if disabled { "disabled" } else { "protected" }.into();
        report.message = if disabled { "Automatic portrait retouch is disabled on this device." } else { "Your manual retouch steps are protected. Undo those steps to return to the automatic version." }.into();
        return Ok(Plan { report, groups });
    }
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
    let mut planned: [Vec<Edit>; 5] = Default::default();
    let mut bodies = 0_usize;
    for (index, face) in report.faces.iter().enumerate() {
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
        let face_plan = plan_face(face, index, prgb, pw, ph);
        let mut body = if options.scope.body() {
            match &face_plan.sample {
                Some(sample) => plan_body(face, index, prgb, pw, ph, sample),
                None => FacePlan::skip(
                    "Body skin was not retouched: no reliable face skin sample to compare it with.",
                ),
            }
        } else {
            FacePlan::skip("")
        };
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
        if !plan.edits.is_empty() {
            if let Some(px) = &detail_pixels {
                features = portrait_features::plan(face, index, px, exposure, PREFIX, &options);
            }
        }
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
        skin.extend(body.edits);
        spots.extend(features.blemishes);
        refine.extend(features.refine);
        eyes.extend(features.eyes);
        finishing.extend(features.finishing);
    }
    let [skin, spots, refine, eyes, finishing] = planned;
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
    Ok(Plan { report, groups })
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
            }),
            selection: None,
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
    // Sample a grid over the area and keep patches that look like this person's skin.
    let mut candidates = Vec::new();
    let mut probes = 0_usize;
    for gy in 0..14 {
        for gx in 0..14 {
            let point = [
                x0 + (x1 - x0) * (gx as f32 + 0.5) / 14.0,
                y0 + (y1 - y0) * (gy as f32 + 0.5) / 14.0,
            ];
            probes += 1;
            if let Some(sample) = sample_quality(rgb, width, height, point) {
                let near = color_distance(sample.chroma, face_sample.chroma) <= 0.06;
                let lit =
                    sample.mean > face_sample.mean * 0.4 && sample.mean < face_sample.mean * 2.0;
                if near && lit {
                    candidates.push(sample);
                }
            }
        }
    }
    if candidates.len() < 4 {
        return FacePlan::skip(
            "Body skin: too little visible skin matching this person's face was found below it (covered by clothing or out of frame).",
        );
    }
    let Some((sample, [texture, tone, _])) = representative_sample(&candidates) else {
        return FacePlan::skip("Body skin: no representative skin patch was found.");
    };
    let coverage = candidates.len() as f32 / probes as f32;
    // Cover the search area with feathered horizontal strokes, then erase the face.
    let radius = ((y1 - y0).min(x1 - x0) / 4.0)
        .min(short * 0.24)
        .max(short * 0.02);
    let mut strokes = Vec::new();
    let mut y = y0 + radius * 0.8;
    while y < y1 + radius * 0.2 && strokes.len() < 24 {
        strokes.push(BrushStroke {
            erase: false,
            radius: (radius / short).clamp(0.0005, 0.25),
            opacity: 1.0,
            points: vec![
                [
                    ((x0 + radius * 0.8) / w).clamp(0.0, 1.0),
                    (y / h).clamp(0.0, 1.0),
                    1.0,
                ],
                [
                    ((x1 - radius * 0.8) / w).clamp(0.0, 1.0),
                    (y / h).clamp(0.0, 1.0),
                    1.0,
                ],
            ],
        });
        y += radius * 1.4;
    }
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
        }),
        selection: None,
    })
    .collect();
    FacePlan {
        edits,
        sample: Some(sample),
        reason: format!(
            "Body skin: found {} skin patches below the face that match this person's own face colour; smoothed and evened them only.{}",
            candidates.len(),
            if coverage > 0.8 {
                " Much of the area matched skin colour, so a similar-coloured background may be softened slightly; check the result."
            } else {
                ""
            }
        ),
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
    let strokes = vec![
        // Forehead, cheeks at eye height, lower cheeks and jaw, chin, nose bridge.
        stroke(at(f.mid, -0.5, -0.55), at(f.mid, 0.5, -0.55), 0.22, false),
        stroke(at(eye_a, -0.2, 0.45), at(eye_b, 0.2, 0.45), 0.26, false),
        stroke(at(f.mid, -0.5, 0.95), at(f.mid, 0.5, 0.95), 0.22, false),
        stroke(
            at(f.mouth_centre, 0.0, 0.35),
            at(f.mouth_centre, 0.0, 0.35),
            0.2,
            false,
        ),
        stroke(at(f.mid, 0.0, 0.05), at(f.nose, 0.0, -0.05), 0.1, false),
        // Eyes with lashes, brows, lips and nostrils stay exactly as they are.
        stroke(at(eye_a, -0.15, 0.0), at(eye_a, 0.15, 0.0), 0.11, true),
        stroke(at(eye_b, -0.15, 0.0), at(eye_b, 0.15, 0.0), 0.11, true),
        stroke(at(eye_a, -0.18, -0.3), at(eye_a, 0.18, -0.3), 0.08, true),
        stroke(at(eye_b, -0.18, -0.3), at(eye_b, 0.18, -0.3), 0.08, true),
        stroke(f.mouth[0], f.mouth[1], 0.14, true),
        stroke(at(f.nose, -0.09, 0.04), at(f.nose, -0.09, 0.04), 0.05, true),
        stroke(at(f.nose, 0.09, 0.04), at(f.nose, 0.09, 0.04), 0.05, true),
    ];
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
        let options = portrait_features::Options {
            scope,
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
}
