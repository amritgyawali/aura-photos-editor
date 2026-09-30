//! Automatic portrait planning produces ordinary, reversible retouch operations.
use aura_core::AuraResult;
use aura_recipe::{
    retouch_tools::{self, BrushMask, BrushStroke, Edit, SkinSettings, Tool},
    Recipe,
};
use aura_vision::portrait::{self, PortraitFace};
use serde::{Deserialize, Serialize};

pub const KEY: &str = "studio_portrait_auto_v1";
const PREFIX: &str = "auto-portrait-v1-";

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
}

/// Add a bounded portrait plan to an AI proposal; manually authored stacks stay intact.
/// # Errors
/// Pixel analysis, recipe validation, or report serialization failed.
pub fn apply(proposal: &mut Recipe, rgb: &[u8], width: u32, height: u32) -> AuraResult<Report> {
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
    };
    if disabled || protected {
        report.status = if disabled { "disabled" } else { "protected" }.into();
        report.message = if disabled { "Automatic portrait retouch is disabled on this device." } else { "Your manual retouch steps are protected. Undo those steps to return to the automatic version." }.into();
    } else {
        report.faces = portrait::detect(rgb, width, height)?;
        report.detected_faces = report.faces.len();
        let mut edits = retouch_tools::read(proposal)?;
        edits.retain(|e| !e.id.starts_with(PREFIX));
        for (index, face) in report.faces.iter().enumerate() {
            let planned = plan_face(face, index, rgb, width, height);
            if planned.is_empty() || edits.len() + planned.len() > retouch_tools::MAX_EDITS {
                continue;
            }
            report.retouched_faces += 1;
            report.operations += planned.len();
            edits.extend(planned);
        }
        if report.operations > 0 || proposal.extra.contains_key(retouch_tools::KEY) {
            retouch_tools::write(proposal, &edits)?;
        }
        report.message = if report.detected_faces == 0 {
            "No confident, sufficiently large face found. Portrait retouch was skipped.".into()
        } else if report.retouched_faces == 0 {
            "Faces detected, but no suitable skin sample was found or the operation limit was reached. Portrait retouch was skipped.".into()
        } else {
            format!("Retouched {} of {} detected faces with {} editable steps: skin texture, tone uniformity and local light balance. Eye and mouth areas are excluded. Undo restores the previous version.", report.retouched_faces, report.detected_faces, report.operations)
        };
    }
    proposal.extra.insert(
        KEY.into(),
        serde_json::to_value(&report)
            .map_err(|e| aura_core::errors::render::recipe_invalid(KEY, &e.to_string()))?,
    );
    Ok(report)
}

// Pixel-space geometry avoids elongated masks on portrait/landscape images.
// Every indexed coordinate below is a statically sized two-element array.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing
)]
fn plan_face(face: &PortraitFace, index: usize, rgb: &[u8], width: u32, height: u32) -> Vec<Edit> {
    let w = width as f32;
    let h = height as f32;
    let short = w.min(h);
    let [eye_a, eye_b, nose, mouth_a, mouth_b] = face.landmarks.map(|[x, y]| [x * w, y * h]);
    let dx = eye_b[0] - eye_a[0];
    let dy = eye_b[1] - eye_a[1];
    let distance = dx.hypot(dy);
    if distance < 12.0 {
        return Vec::new();
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
        return Vec::new();
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
                if let Some((score, variation)) = sample_quality(rgb, width, height, point) {
                    candidates.push((score, variation, point));
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some((_, variation, sample)) = candidates.first() else {
        return Vec::new();
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
    let amount = (0.18 + variation * 0.8).clamp(0.18, 0.32);
    [
        (Tool::SkinSmooth, amount, "texture"),
        (Tool::SkinUniformity, 0.16, "tone"),
        (Tool::PortraitDodgeBurn, 0.20, "light"),
    ]
    .into_iter()
    .map(|(tool, amount, name)| Edit {
        id: format!("{PREFIX}{index}-{name}"),
        tool,
        enabled: true,
        region,
        source: Some([sample[0] / w, sample[1] / h]),
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
    .collect()
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
fn sample_quality(rgb: &[u8], width: u32, height: u32, [x, y]: [f32; 2]) -> Option<(f32, f32)> {
    if x < 2.0 || y < 2.0 || x >= width as f32 - 2.0 || y >= height as f32 - 2.0 {
        return None;
    }
    let mut low = 1.0_f32;
    let mut high = 0.0_f32;
    let mut sum = 0.0;
    for yy in y as usize - 2..=y as usize + 2 {
        for xx in x as usize - 2..=x as usize + 2 {
            let start = (yy * width as usize + xx) * 3;
            let pixel = rgb.get(start..start + 3)?;
            if pixel.iter().any(|v| *v > 246) {
                return None;
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
    Some((variation + (mean - 0.5).abs() * 0.025, variation))
}

#[cfg(test)]
mod tests {
    use super::*;
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
            let edits = plan_face(&face, 0, &rgb, 100, 100);
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
            assert_eq!(edits, plan_face(&face, 0, &rgb, 100, 100));
        }
        assert!(plan_face(&face, 0, &vec![255; 100 * 100 * 3], 100, 100).is_empty());
    }
}
