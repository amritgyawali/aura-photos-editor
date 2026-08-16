//! Compact JSON and owned SQL-row representations for the composition store.

use aura_core::contract::composition::{
    Box2, CompositionCode, CompositionFlags, CompositionReason, CompositionResult, CropHint,
    FrameEdge, Joint, JointCut,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraError, AuraResult};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::Provenance;

/// A reason as stored.
///
/// `text` is `None` whenever the sentence is exactly `CompositionCode::user_text`, which
/// is the case for twenty-three of the twenty-six codes. That is where most of the 800-byte
/// budget's headroom comes from.
#[derive(Debug, Serialize, Deserialize)]
struct StoredReason {
    #[serde(rename = "c")]
    code: String,
    #[serde(rename = "w")]
    weight: f32,
    #[serde(rename = "e", skip_serializing_if = "Option::is_none")]
    evidence: Option<StoredBox>,
    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

/// A rectangle as stored. Four floats with one-letter keys, because this shape appears up
/// to thirteen times in one row.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct StoredBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl StoredBox {
    const fn of(area: Box2) -> Self {
        Self {
            x: area.x,
            y: area.y,
            w: area.w,
            h: area.h,
        }
    }

    const fn to_box(self) -> Box2 {
        Box2 {
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
        }
    }
}

/// One cut as stored.
#[derive(Debug, Serialize, Deserialize)]
struct StoredCut {
    #[serde(rename = "j")]
    joint: String,
    #[serde(rename = "d")]
    edge: String,
    #[serde(rename = "a")]
    at_joint: bool,
    #[serde(rename = "s")]
    severity: f32,
    #[serde(rename = "b")]
    area: StoredBox,
}

impl StoredCut {
    fn of(cut: &JointCut) -> Self {
        Self {
            joint: cut.joint.as_str().to_string(),
            edge: cut.edge.as_str().to_string(),
            at_joint: cut.at_joint,
            severity: cut.severity,
            area: StoredBox::of(cut.area),
        }
    }

    fn to_cut(&self) -> JointCut {
        JointCut {
            joint: Joint::from_str_or_neck(&self.joint),
            edge: FrameEdge::from_str_or_bottom(&self.edge),
            at_joint: self.at_joint,
            severity: self.severity,
            area: self.area.to_box(),
        }
    }
}

/// The three evidence arrays, in one object, each omitted when empty.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct StoredViolations {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    cuts: Vec<StoredCut>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    intrusions: Vec<StoredBox>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blobs: Vec<StoredBox>,
}

impl StoredViolations {
    pub(super) fn cuts(&self) -> Vec<JointCut> {
        self.cuts.iter().map(StoredCut::to_cut).collect()
    }

    pub(super) fn intrusions(&self) -> Vec<Box2> {
        self.intrusions
            .iter()
            .copied()
            .map(StoredBox::to_box)
            .collect()
    }

    pub(super) fn blobs(&self) -> Vec<Box2> {
        self.blobs.iter().copied().map(StoredBox::to_box).collect()
    }
}

/// A crop hint as stored.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StoredHint {
    #[serde(rename = "r")]
    region: StoredBox,
    #[serde(rename = "m")]
    safe_margin: f32,
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    straighten_deg: Option<f32>,
    #[serde(rename = "c")]
    confidence: f32,
}

impl StoredHint {
    pub(super) fn into_hint(self) -> CropHint {
        CropHint {
            region: self.region.to_box(),
            safe_margin: self.safe_margin,
            straighten_deg: self.straighten_deg,
            confidence: self.confidence,
        }
    }
}

/// Render the reasons.
pub(super) fn encode_reasons(reasons: &[CompositionReason]) -> AuraResult<String> {
    let rows: Vec<StoredReason> = reasons
        .iter()
        .take(CompositionResult::MAX_REASONS)
        .map(|reason| StoredReason {
            code: reason.code.as_str().to_string(),
            weight: reason.weight,
            evidence: reason.evidence.map(StoredBox::of),
            text: if reason.text == reason.code.user_text() {
                None
            } else {
                Some(reason.text.clone())
            },
        })
        .collect();
    encode_json("composition reasons", &rows)
}

/// Read the reasons back, substituting the code's own sentence where none was stored.
pub(super) fn decode_reasons(text: &str) -> AuraResult<Vec<CompositionReason>> {
    let rows: Vec<StoredReason> = decode_json("composition reasons", text)?;
    rows.into_iter()
        .map(|row| {
            let code = CompositionCode::ALL
                .into_iter()
                .find(|code| code.as_str() == row.code)
                .ok_or_else(|| {
                    invalid_stored_value(
                        "composition reasons",
                        format!("unknown composition reason code {:?}", row.code),
                    )
                })?;
            Ok(CompositionReason {
                code,
                text: row.text.unwrap_or_else(|| code.user_text().to_string()),
                weight: row.weight,
                evidence: row.evidence.map(StoredBox::to_box),
            })
        })
        .collect()
}

/// Render the three evidence arrays.
pub(super) fn encode_violations(result: &CompositionResult) -> AuraResult<String> {
    let stored = StoredViolations {
        cuts: result.joint_cuts.iter().map(StoredCut::of).collect(),
        intrusions: result
            .edge_intrusions
            .iter()
            .copied()
            .map(StoredBox::of)
            .collect(),
        blobs: result
            .bright_blobs
            .iter()
            .copied()
            .map(StoredBox::of)
            .collect(),
    };
    encode_json("composition violations", &stored)
}

/// Render the crop hint.
pub(super) fn encode_hint(hint: CropHint) -> AuraResult<String> {
    encode_json(
        "composition crop hint",
        &StoredHint {
            region: StoredBox::of(hint.region),
            safe_margin: hint.safe_margin,
            straighten_deg: hint.straighten_deg,
            confidence: hint.confidence,
        },
    )
}

fn encode_json<T: Serialize>(field: &str, value: &T) -> AuraResult<String> {
    serde_json::to_string(value)
        .map_err(|error| statement_failed(format!("could not encode {field}"), &error))
}

pub(super) fn decode_json<T: DeserializeOwned>(field: &str, text: &str) -> AuraResult<T> {
    serde_json::from_str(text)
        .map_err(|error| statement_failed(format!("stored {field} is corrupt"), &error))
}

fn invalid_stored_value(field: &str, detail: String) -> AuraError {
    let error = std::io::Error::new(std::io::ErrorKind::InvalidData, detail);
    statement_failed(format!("stored {field} is corrupt"), &error)
}

fn reason_is_dismissed(reason: &CompositionReason, dismissed: CompositionFlags) -> bool {
    crate::composition::flags::flag_for_code(reason.code)
        .is_some_and(|flag| dismissed.contains(flag))
}

pub(super) fn visible_reasons(
    reasons: Vec<CompositionReason>,
    dismissed: CompositionFlags,
) -> Vec<CompositionReason> {
    let mut visible: Vec<CompositionReason> = reasons
        .into_iter()
        .filter(|reason| !reason_is_dismissed(reason, dismissed))
        .collect();
    if visible.is_empty() {
        visible.push(CompositionReason::frame(
            CompositionCode::Clean,
            "the photographer dismissed the remaining composition notes",
            0.0,
        ));
    }
    visible
}

/// Every scalar column, owned, so the write closure captures no borrow.
#[derive(Debug, Clone)]
pub(super) struct OwnedRow {
    pub(super) tilt_deg: f64,
    pub(super) tilt_intentional: i64,
    pub(super) horizon_conf: f64,
    pub(super) horizon_source: String,
    pub(super) headroom: f64,
    pub(super) thirds_offset: f64,
    pub(super) balance: f64,
    pub(super) negative_space: f64,
    pub(super) head_crop: i64,
    pub(super) clutter: f64,
    pub(super) head_merge: i64,
    pub(super) colour_competition: f64,
    pub(super) aesthetic: f64,
    pub(super) composition_score: f64,
    pub(super) relative_composition: f64,
    pub(super) keypoint_subjects: i64,
    pub(super) flags: i64,
    pub(super) confidence: f64,
    pub(super) scene: String,
    pub(super) unruled: i64,
    pub(super) model_ver: i64,
    pub(super) analysis_ver: i64,
    pub(super) rules_ver: i64,
}

impl From<(&CompositionResult, &Provenance)> for OwnedRow {
    fn from((result, provenance): (&CompositionResult, &Provenance)) -> Self {
        let headroom = if result.head_crop {
            f64::from(result.headroom).min(0.0)
        } else {
            f64::from(result.headroom)
        };
        Self {
            tilt_deg: f64::from(result.tilt_deg.clamp(-45.0, 45.0)),
            tilt_intentional: i64::from(result.tilt_intentional),
            horizon_conf: f64::from(result.horizon_conf.clamp(0.0, 1.0)),
            horizon_source: result.horizon_source.as_str().to_string(),
            headroom: headroom.clamp(-1.0, 1.0),
            thirds_offset: f64::from(result.thirds_offset.clamp(0.0, 1.0)),
            balance: f64::from(result.balance.clamp(0.0, 1.0)),
            negative_space: f64::from(result.negative_space.clamp(0.0, 1.0)),
            head_crop: i64::from(result.head_crop),
            clutter: f64::from(result.clutter.max(0.0)),
            head_merge: i64::from(result.head_merge),
            colour_competition: f64::from(result.colour_competition.clamp(0.0, 1.0)),
            aesthetic: f64::from(result.aesthetic.clamp(0.0, 1.0)),
            composition_score: f64::from(result.composition_score.clamp(0.0, 1.0)),
            relative_composition: f64::from(result.relative_composition.clamp(0.0, 1.0)),
            keypoint_subjects: i64::from(result.keypoint_subjects),
            flags: i64::from(result.flags.bits()),
            confidence: f64::from(result.confidence.clamp(0.0, 1.0)),
            scene: result.scene.as_str().to_string(),
            unruled: i64::from(provenance.unruled),
            model_ver: i64::from(result.model_ver),
            analysis_ver: i64::from(result.analysis_ver),
            rules_ver: i64::from(result.rules_ver),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_reason_json_is_an_error() {
        assert!(decode_reasons("not-json").is_err());
    }

    #[test]
    fn review_projection_removes_the_flag_reason_but_keeps_an_explanation() {
        let reasons = vec![CompositionReason::frame(
            CompositionCode::HeadCropped,
            CompositionCode::HeadCropped.user_text(),
            -0.2,
        )];
        let visible = visible_reasons(reasons, CompositionFlags::HEAD_CROPPED);
        assert!(visible
            .iter()
            .all(|reason| reason.code != CompositionCode::HeadCropped));
        assert!(visible
            .iter()
            .any(|reason| reason.code == CompositionCode::Clean));
    }
}
