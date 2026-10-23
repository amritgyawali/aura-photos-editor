//! Revision-bound screen checks before automatic portrait delivery.
//! This is a boundary/clipping gate, not native-resolution or Evoto-parity certification.
use crate::{
    advanced_retouch::{measure, quality},
    portrait_auto, AppState,
};
use aura_core::{AuraResult, PhotoId};
use aura_recipe::{retouch_tools, Recipe};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    pub version: String,
    pub recipe_hash: String,
    pub photo_id: String,
    pub source_hash_at_import: Option<String>,
    pub renderer: String,
    pub analysis_width: usize,
    pub analysis_height: usize,
    pub status: String,
    pub contours: Vec<quality::ContourReading>,
    pub clipped_before: Option<f32>,
    pub clipped_after: Option<f32>,
    pub reasons: Vec<String>,
    pub scope: String,
}

impl Report {
    pub(crate) fn eligible(&self) -> bool {
        matches!(self.status.as_str(), "measured" | "not_applicable")
    }
}

fn invalid(message: &str) -> aura_core::AuraError {
    aura_core::errors::render::recipe_invalid("portrait preflight", message)
}

/// Baseline keeps global colour, manual edits and automatic scene corrections identical.
/// Only automatic portrait operations are disabled. It cannot save or mutate a recipe.
fn baseline(recipe: &Recipe) -> AuraResult<(Recipe, usize)> {
    let mut before = recipe.clone();
    let mut edits = retouch_tools::read(recipe)?;
    let mut count = 0;
    for edit in &mut edits {
        if edit.id.starts_with(portrait_auto::PREFIX) && edit.enabled && edit.amount > 0.0 {
            edit.enabled = false;
            count += 1;
        }
    }
    retouch_tools::write(&mut before, &edits)?;
    Ok((before, count))
}

pub(crate) fn check(state: &AppState, photo: PhotoId, recipe: &Recipe) -> AuraResult<Report> {
    let mut report = Report {
        version: "portrait-boundary-preflight-v2-regions".into(),
        recipe_hash: aura_recipe::recipe_hash(recipe)?,
        photo_id: photo.to_db(),
        source_hash_at_import: state.catalog().read(|conn| {
            Ok(aura_catalog::repo::primary_file_for_photo(conn, &photo.to_db())?.map(|(_, hash)| hash))
        })?,
        renderer: aura_render::graph::engine_string(),
        analysis_width: 0, analysis_height: 0, status: "incomplete".into(),
        contours: Vec::new(), clipped_before: None, clipped_after: None, reasons: Vec::new(),
        scope: "Screen-resolution face/adjacent-neck and landmark-estimated eye, nose, mouth and chin regions: seams and clipping only. Native-resolution appearance, skin semantics, body boundaries and Evoto equivalence are not measured.".into(),
    };
    let (before, operations) = baseline(recipe)?;
    if operations == 0 {
        report.status = "not_applicable".into();
        report
            .reasons
            .push("No enabled automatic portrait operations in this recipe.".into());
        return Ok(report);
    }
    let faces: Vec<aura_vision::portrait::PortraitFace> = recipe
        .extra
        .get(portrait_auto::KEY)
        .and_then(|value| value.get("faces"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    if faces.is_empty() {
        report.reasons.push(
            "Automatic portrait edits have no valid saved face geometry; review before delivery."
                .into(),
        );
        return Ok(report);
    }
    let (before, w, h) = crate::advanced_retouch::qc_render(state, photo, &before)?;
    let (after, aw, ah) = crate::advanced_retouch::qc_render(state, photo, recipe)?;
    report.analysis_width = w;
    report.analysis_height = h;
    if (w, h) != (aw, ah) || w == 0 || h == 0 || before.len() != after.len() {
        return Err(invalid("Before and after renders do not align."));
    }
    report.clipped_before = Some(measure::clipped(&before));
    report.clipped_after = Some(measure::clipped(&after));
    for (index, face) in faces.iter().enumerate() {
        if let Some(reading) = quality::contour(&before, &after, w, h, face, index) {
            report.contours.push(reading);
        } else {
            report.reasons.push(format!(
                "Face {} has insufficient geometry or resolution for the contour check.",
                index + 1
            ));
        }
    }
    finish(&mut report, faces.len());
    Ok(report)
}

fn finish(report: &mut Report, expected: usize) {
    if expected == 0
        || report.contours.len() != expected
        || !report
            .contours
            .iter()
            .enumerate()
            .all(|(i, c)| c.face == i && c.complete())
        || !report.clipped_before.is_some_and(f32::is_finite)
        || !report.clipped_after.is_some_and(f32::is_finite)
        || report.analysis_width == 0
        || report.analysis_height == 0
    {
        report.status = "incomplete".into();
        report.reasons.push("Complete face, local-region and clipping measurements are required before automatic delivery.".into());
        return;
    }
    for reading in &report.contours {
        for region in reading.regions.iter().filter(|r| r.review) {
            report.reasons.push(format!("Face {}, {}: {} new seam pairs and {} weakened edges out of {} source edges need review.",
                reading.face + 1, region.region.label(), region.new_seam_pairs,
                region.weakened_edge_pairs, region.source_edge_pairs));
        }
        if reading.needs_review() {
            report.reasons.push(format!(
                "Face {}: a new seam or weakened source edge needs review.",
                reading.face + 1
            ));
        }
    }
    if report
        .clipped_after
        .zip(report.clipped_before)
        .is_some_and(|(a, b)| a > b + 0.005)
    {
        report.reasons.push(
            "Automatic retouch increased clipped pixels by more than 0.5 percentage points.".into(),
        );
    }
    report.status = if report.reasons.is_empty() {
        "measured"
    } else {
        "review"
    }
    .into();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_measurements_and_new_seams_cannot_enter_automatic_delivery() {
        let mut report = Report {
            version: "test".into(),
            recipe_hash: "revision".into(),
            photo_id: "test".into(),
            source_hash_at_import: None,
            renderer: "test".into(),
            analysis_width: 1600,
            analysis_height: 1200,
            status: "measured".into(),
            contours: Vec::new(),
            clipped_before: Some(0.0),
            clipped_after: Some(0.0),
            reasons: Vec::new(),
            scope: "test".into(),
        };
        finish(&mut report, 1);
        assert!(!report.eligible());
        report.contours.push(quality::ContourReading {
            face: 0,
            sampled_pairs: 1000,
            new_seam_pairs: 0,
            source_edge_pairs: 100,
            weakened_edge_pairs: 0,
            review: false,
            regions: quality::REGIONS
                .map(|region| quality::RegionReading {
                    region,
                    sampled_pairs: 100,
                    new_seam_pairs: 0,
                    source_edge_pairs: 20,
                    weakened_edge_pairs: 0,
                    measured: true,
                    review: false,
                })
                .into(),
        });
        report.reasons.clear();
        finish(&mut report, 1);
        assert!(report.eligible());
        report.contours[0].review = true;
        finish(&mut report, 1);
        assert!(!report.eligible());
        report.contours[0].review = false;
        report.clipped_after = Some(f32::NAN);
        finish(&mut report, 1);
        assert_eq!(report.status, "incomplete");
    }

    #[test]
    fn comparison_keeps_global_colour_manual_work_and_scene_corrections() {
        let mut recipe = aura_recipe::fixtures::neutral("test", "test");
        recipe.global.exposure = 0.7;
        let manual = crate::advanced_retouch::base_edit(
            "manual".into(),
            retouch_tools::Tool::Dodge,
            [0.5, 0.5, 0.2, 0.2],
            0.3,
        );
        let mut portrait = manual.clone();
        portrait.id = format!("{}0-test", portrait_auto::PREFIX);
        let mut scene = manual.clone();
        scene.id = format!("{}sky", portrait_auto::SCENE_PREFIX);
        retouch_tools::write(
            &mut recipe,
            &[manual.clone(), portrait.clone(), scene.clone()],
        )
        .unwrap();
        let original_hash = aura_recipe::recipe_hash(&recipe).unwrap();
        let (before, count) = baseline(&recipe).unwrap();
        assert_eq!(count, 1);
        assert_eq!(before.global, recipe.global);
        assert_eq!(before.geometry, recipe.geometry);
        let edits = retouch_tools::read(&before).unwrap();
        assert_eq!(edits[0], manual);
        assert!(!edits[1].enabled);
        assert_eq!(edits[2], scene);
        assert_eq!(original_hash, aura_recipe::recipe_hash(&recipe).unwrap());
    }
}
