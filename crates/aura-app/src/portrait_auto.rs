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
}

/// The history step an automatic retouch operation belongs to. Order is save order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Scene,
    Skin,
    Blemishes,
    Eyes,
    Finishing,
}

impl Group {
    pub const ALL: [Self; 5] = [
        Self::Scene,
        Self::Skin,
        Self::Blemishes,
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
    plan_with_faces(proposal, rgb, width, height, detail, exposure, None)
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
) -> AuraResult<Plan> {
    let protected = proposal.provenance.user_edited_fields.iter().any(|path| {
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
    let mut planned: [Vec<Edit>; 4] = Default::default();
    for (index, face) in report.faces.iter().enumerate() {
        let mut plan = plan_face(face, index, rgb, width, height);
        let mut features = portrait_features::FeatureEdits::default();
        if !plan.edits.is_empty() {
            if let Some(px) = &detail_pixels {
                features = portrait_features::plan(face, index, px, exposure, PREFIX);
            }
        }
        let used: usize = planned.iter().map(Vec::len).sum();
        let wanted = plan.edits.len()
            + features.blemishes.len()
            + features.eyes.len()
            + features.finishing.len();
        if manual + scene_ops + used + wanted > retouch_tools::MAX_EDITS {
            plan = FacePlan::skip("The saved retouch stack has reached its operation limit.");
            features = portrait_features::FeatureEdits::default();
        }
        let mut strengths = [0.0; 3];
        for (strength, edit) in strengths.iter_mut().zip(&plan.edits) {
            *strength = edit.amount;
        }
        report.assessments.push(FaceAssessment {
            face: index + 1,
            status: if plan.edits.is_empty() {
                "skipped"
            } else {
                "retouched"
            }
            .into(),
            confidence: face.confidence,
            reason: plan.reason,
            strengths,
            findings: features.report.findings,
            spots_healed: features.report.spots_healed,
            marks_kept: features.report.marks_kept,
        });
        if plan.edits.is_empty() {
            continue;
        }
        report.retouched_faces += 1;
        report.operations += wanted;
        let [skin, spots, eyes, finishing] = &mut planned;
        skin.extend(plan.edits);
        spots.extend(features.blemishes);
        eyes.extend(features.eyes);
        finishing.extend(features.finishing);
    }
    let [skin, spots, eyes, finishing] = planned;
    groups.insert(Group::Skin, skin);
    groups.insert(Group::Blemishes, spots);
    groups.insert(Group::Eyes, eyes);
    groups.insert(Group::Finishing, finishing);
    let spots: usize = report.assessments.iter().map(|a| a.spots_healed).sum();
    let kept: usize = report.assessments.iter().map(|a| a.marks_kept).sum();
    report.message = if report.detected_faces == 0 {
        "No confident, sufficiently large face found. Portrait retouch was skipped.".into()
    } else if report.retouched_faces == 0 {
        "Faces detected, but no suitable skin sample was found or the operation limit was reached. Portrait retouch was skipped.".into()
    } else {
        format!(
            "Retouched {} of {} detected faces with {} editable steps: skin texture, tone and light{}, eye and teeth finishing where measured.{} Undo walks back one automatic step at a time.",
            report.retouched_faces,
            report.detected_faces,
            report.operations,
            if spots > 0 { format!(", {spots} healed spot{}", if spots == 1 { "" } else { "s" }) } else { String::new() },
            if kept > 0 { format!(" {kept} possible permanent mark{} kept.", if kept == 1 { "" } else { "s" }) } else { String::new() },
        )
    };
    Ok(Plan { report, groups })
}

struct FacePlan {
    edits: Vec<Edit>,
    reason: String,
}

impl FacePlan {
    fn skip(reason: &str) -> Self {
        Self {
            edits: Vec::new(),
            reason: reason.into(),
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
            (0.10 + texture / luminance.max(0.08) * 0.18).clamp(0.10, 0.28) * signal,
            (0.08 + color_spread * 1.2).clamp(0.08, 0.20) * signal,
            (0.08 + light_spread / luminance.max(0.08) * 0.4).clamp(0.08, 0.22) * signal,
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
    let mask = skin_mask(
        centers,
        [eye_a, eye_b, nose, mouth_a, mouth_b],
        distance,
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
    .map(|(tool, amount, name)| Edit {
        id: format!("{PREFIX}{index}-{name}"),
        tool,
        enabled: true,
        region,
        source: Some([sample.point[0] / w, sample.point[1] / h]),
        amount,
        feather: 0.7,
        radius: (distance * 0.045 / short).clamp(0.001, 0.012),
        texture: 1.0,
        tone: 0.5,
        warmth: 0.0,
        tint: 0.0,
        mask: Some(mask.clone()),
        skin: Some(SkinSettings {
            tolerance: 0.07,
            edge_protection: 0.9,
        }),
        selection: None,
    })
    .collect();
    FacePlan {
        edits,
        reason: format!("Compared {} cheek/forehead patches. Selected a representative low-variation sample; strengths follow this face's texture, color variation and lighting. Eyes and mouth remain excluded.{}", candidates.len(), if sample.mean < 0.15 { " Low skin signal reduces correction strength." } else { "" }),
    }
}

fn skin_mask(
    centers: Vec<[f32; 2]>,
    landmarks: [[f32; 2]; 5],
    distance: f32,
    [width, height]: [f32; 2],
) -> BrushMask {
    let short = width.min(height);
    let mut strokes: Vec<_> = centers
        .into_iter()
        .map(|[x, y]| BrushStroke {
            erase: false,
            radius: (distance * 0.29 / short).clamp(0.0005, 0.25),
            opacity: 1.0,
            points: vec![[x / width, y / height, 1.0]],
        })
        .collect();
    // Explicit exclusions remain editable and protect landmarks if masks overlap.
    for [x, y] in landmarks {
        strokes.push(BrushStroke {
            erase: true,
            radius: (distance * 0.20 / short).clamp(0.0005, 0.25),
            opacity: 1.0,
            points: vec![[x / width, y / height, 1.0]],
        });
    }
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
                5
            );
            assert_eq!(edits, plan_face(&face, 0, &rgb, 100, 100).edits);
        }
        assert!(plan_face(&face, 0, &vec![255; 100 * 100 * 3], 100, 100)
            .edits
            .is_empty());
    }
}
