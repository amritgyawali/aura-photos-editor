//! How differently a second photographer exposes, and how much of it survives.
//!
//! Section 8 step 6 and section 6.3. This is the half of the phase that is not about cameras at
//! all, and it is here rather than in a phase of its own because **gear and habit are entangled in
//! practice**: the second shooter carries the second body, so the difference between the two sets
//! of files is one number with two causes, and separating them would need the same person to shoot
//! both bodies at the same wedding.
//!
//! What is separable, and what this module keeps apart, is the *measurement* from the *correction*.
//! [`ShooterBias::measured_ev`] is the whole systematic offset and [`ShooterBias::applied_ev`] is
//! the part of it that is corrected, and the two are separate fields because a report that only
//! stored the second could not tell a photographer that their second shooter works two thirds of a
//! stop darker and has been moved by a third of one.
//!
//! ## A median, per scene class, over subject luminance
//!
//! Three choices, and each of them fails differently if made the other way.
//!
//! **A median rather than a mean**, because a wedding contains a handful of deliberately dark
//! frames from everybody - the silhouette at the exit, the candle-lit vow - and a mean over them
//! measures how many of those somebody shot rather than how they expose.
//!
//! **Per scene class**, because a second shooter who works darker during a ceremony may not during
//! a reception, and one number for both is a number that is wrong twice. Invariant 7.
//!
//! **Over subject luminance rather than over exposure compensation**, because what a photographer
//! sets on a dial is a response to their metering and what lands on somebody's face is the habit.
//! Two people can shoot at the same compensation and produce faces a third of a stop apart.
//!
//! ## The cap is the product decision, and it is bounded twice
//!
//! Section 6.3 asks for the correction to be capped "so a deliberately moodier second shooter is
//! harmonised, not erased". [`Matching::shooter_correction`][s] applies sixty per cent of the
//! measured habit and then clamps it at a third of a stop; the two caps are not redundant, because
//! a share bounds the correction relative to the habit and the clamp bounds it outright. A second
//! shooter who works a stop and a half darker is moved by a third of a stop rather than by nine
//! tenths.
//!
//! [s]: super::policy::Matching::shooter_correction

use std::collections::BTreeMap;

use aura_core::contract::camera::{
    CameraCode, CameraReason, ShooterBias, MIN_SHOOTER_FRAMES, SHOOTER_DEADBAND_EV,
};
use aura_core::contract::moment::CameraId;
use aura_core::SceneId;

use crate::stats;

use super::fingerprint::CameraFrame;
use super::policy::Matching;
use super::ANALYSIS_VER;

/// The subject luminance below which a frame is not evidence about a habit.
///
/// A frame whose subject sits at two per cent of the range is a silhouette, and the ratio of two
/// numbers that small is dominated by whichever of them is closer to zero. Excluded rather than
/// clamped, because a clamped silhouette would report as an ordinary frame exposed at the floor.
pub const MIN_USABLE_LUMA: f32 = 0.03;

/// Measure every body's exposure habit against the reference body's, scene class by scene class.
///
/// One row per `(camera, scene)` with at least [`MIN_SHOOTER_FRAMES`] usable frames on **both**
/// sides - the reference needs the samples as much as the body does, because the measurement is a
/// difference and a difference with one weak side is a weak difference.
///
/// Rows are returned in `(camera, scene)` order, which is what the store writes and what the report
/// renders. Invariant 4.
#[must_use]
pub fn measure(
    frames: &[CameraFrame],
    reference: &CameraId,
    policy: &Matching,
) -> Vec<ShooterBias> {
    // Group subject luminances by body and scene, keeping only the frames that carry one and are
    // bright enough to be a ratio.
    let mut by_key: BTreeMap<(String, SceneId), Vec<f32>> = BTreeMap::new();
    let mut labels: BTreeMap<String, (CameraId, String)> = BTreeMap::new();
    for frame in frames {
        let Some(luma) = frame.subject_luma else {
            continue;
        };
        if !luma.is_finite() || luma < MIN_USABLE_LUMA {
            continue;
        }
        by_key
            .entry((frame.camera.as_str().to_string(), frame.scene))
            .or_default()
            .push(luma);
        labels
            .entry(frame.camera.as_str().to_string())
            .or_insert_with(|| (frame.camera.clone(), frame.shooter.clone()));
    }

    let reference_key = reference.as_str().to_string();
    let mut out = Vec::new();

    for ((camera_key, scene), values) in &by_key {
        if camera_key == &reference_key {
            continue;
        }
        let scene_policy = policy.scene(*scene);
        let Some((camera, shooter)) = labels.get(camera_key).cloned() else {
            continue;
        };
        let frames_seen = u32::try_from(values.len()).unwrap_or(u32::MAX);
        let reference_values = by_key.get(&(reference_key.clone(), *scene));

        // Four ways to end up with no measurement, and they are three different rows plus one
        // that is not written at all. The scene being one where exposure is the photograph is the
        // one that is not written: nothing was measured, so there is nothing to report about.
        if !scene_policy.correct_shooter {
            continue;
        }

        let measured_enough = frames_seen >= MIN_SHOOTER_FRAMES;
        let reference_enough = reference_values.is_some_and(|values| {
            u32::try_from(values.len()).unwrap_or(u32::MAX) >= MIN_SHOOTER_FRAMES
        });
        if !measured_enough || !reference_enough {
            out.push(ShooterBias {
                shooter,
                camera_id: camera,
                scene: *scene,
                measured_ev: 0.0,
                applied_ev: 0.0,
                frames: frames_seen,
                capped: false,
                reasons: vec![CameraReason::of(CameraCode::ShooterBiasAbsent)],
                analysis_ver: ANALYSIS_VER,
            });
            continue;
        }

        let Some(reference_values) = reference_values else {
            continue;
        };
        let Some(mine) = stats::median(values) else {
            continue;
        };
        let Some(theirs) = stats::median(reference_values) else {
            continue;
        };
        let measured_ev = offset_ev(mine, theirs);
        let applied_ev = policy.shooter_correction(measured_ev);

        // A correction that came out larger than the habit is a sign error rather than a cap that
        // failed, and migration 26 has the same rule as a CHECK. Belt and braces, because the
        // failure is invisible: a gallery in which every second-shooter frame is corrected past the
        // lead's exposure looks like a gallery that was matched.
        let applied_ev = if applied_ev.abs() > measured_ev.abs() {
            measured_ev
        } else {
            applied_ev
        };

        let capped = applied_ev.abs() + f32::EPSILON < measured_ev.abs();
        let mut reasons = Vec::new();
        if measured_ev.abs() < SHOOTER_DEADBAND_EV {
            reasons.push(CameraReason::of(CameraCode::ShooterStylePreserved));
        } else {
            reasons.push(CameraReason::of(CameraCode::ShooterBiasCorrected));
            if capped {
                reasons.push(CameraReason::of(CameraCode::ShooterBiasCapped));
            }
        }
        reasons.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.code.cmp(&b.code))
        });

        out.push(ShooterBias {
            shooter,
            camera_id: camera,
            scene: *scene,
            measured_ev,
            applied_ev,
            frames: frames_seen,
            capped,
            reasons,
            analysis_ver: ANALYSIS_VER,
        });
    }

    out
}

/// The difference between two subject luminances, in stops.
///
/// A base-two logarithm of the ratio, which is what a stop *is*, and not a subtraction: a
/// difference of 0.1 between two faces at 0.15 and 0.25 is nearly a stop, and the same difference
/// between 0.60 and 0.70 is a fifth of one. A subtraction would report a second shooter as having
/// a huge habit in dark scenes and none in bright ones, which is a measurement of the scenes.
#[must_use]
pub fn offset_ev(mine: f32, theirs: f32) -> f32 {
    if !mine.is_finite() || !theirs.is_finite() || mine <= 0.0 || theirs <= 0.0 {
        return 0.0;
    }
    (mine / theirs).log2()
}

/// The single exposure correction one body carries, folded across its scenes.
///
/// Section 6.3 corrects the habit "as part of the camera transform", and a transform has one
/// exposure axis - so the per-scene rows are a *measurement* and this is what reaches a photograph.
/// It is a frame-weighted mean of the applied corrections, so a scene the body barely shot cannot
/// move the number, and it is zero when nothing was measured.
///
/// **The per-scene detail is not lost**: it is stored, and the report renders it. What is folded
/// here is only what the transform can express, and that limitation is a property of the recipe
/// rather than of this phase - phase 25 is what applies a per-scene residual on top, node by node.
#[must_use]
pub fn folded_ev(rows: &[ShooterBias], camera: &CameraId) -> f32 {
    let mut weighted = 0.0_f64;
    let mut total = 0.0_f64;
    for row in rows.iter().filter(|row| &row.camera_id == camera) {
        if row.frames == 0 {
            continue;
        }
        let weight = f64::from(row.frames);
        weighted += f64::from(row.applied_ev) * weight;
        total += weight;
    }
    if total <= 0.0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    {
        (weighted / total) as f32
    }
}

/// How many rows carry a real measurement, and how many of those a cap reduced.
#[must_use]
pub fn counts(rows: &[ShooterBias]) -> (u32, u32) {
    let measured = rows.iter().filter(|row| row.is_usable()).count();
    let capped = rows.iter().filter(|row| row.capped).count();
    (
        u32::try_from(measured).unwrap_or(u32::MAX),
        u32::try_from(capped).unwrap_or(u32::MAX),
    )
}

#[cfg(test)]
mod tests {
    use aura_core::contract::camera::{Brand, FlashState, MAX_SHOOTER_EV};
    use aura_core::contract::gallery::ImageId;
    use aura_core::contract::ids::NodeId;

    use super::*;

    fn frame(camera: &str, shooter: &str, scene: SceneId, luma: f32) -> CameraFrame {
        CameraFrame {
            image: ImageId::new(),
            camera: CameraId::new(camera),
            brand: Brand::Canon,
            shooter: shooter.to_string(),
            flash: FlashState::Ambient,
            node: Some(NodeId::new()),
            scene,
            timeline_ms: 0,
            cct_k: Some(5200.0),
            tint: Some(0.0),
            exposure_ev: Some(0.0),
            subject_luma: Some(luma),
            wb_conf: 0.8,
            white_uv: Some([0.20, 0.47]),
            skin_uv: None,
            skin_luma: None,
            contrast: Some(8.0),
            saturation: Some(4.0),
            signature: Some([0.1; 8]),
            embedding: Some(vec![1.0, 0.0]),
            background: None,
        }
    }

    fn wedding(lead_luma: f32, second_luma: f32, scene: SceneId) -> Vec<CameraFrame> {
        let mut frames = Vec::new();
        for i in 0..40 {
            #[allow(clippy::cast_precision_loss)]
            let jitter = ((i % 7) as f32 - 3.0) * 0.004;
            frames.push(frame("cam_a", "primary", scene, lead_luma + jitter));
            frames.push(frame("cam_b", "second", scene, second_luma + jitter));
        }
        frames
    }

    #[test]
    fn a_darker_second_shooter_is_brought_partly_toward_the_lead() {
        // Half a stop darker: 0.30 against 0.42.
        let frames = wedding(0.42, 0.30, SceneId::Ceremony);
        let rows = measure(&frames, &CameraId::new("cam_a"), &Matching::default());
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(row.measured_ev < -0.4, "measured {}", row.measured_ev);
        assert!(row.applied_ev > 0.0, "the correction brightens them");
        assert!(
            row.applied_ev.abs() < row.measured_ev.abs(),
            "harmonised, not erased"
        );
        assert!(row.capped);
        assert!(row
            .reasons
            .iter()
            .any(|r| r.code == CameraCode::ShooterBiasCorrected));
    }

    #[test]
    fn a_habit_is_never_erased_however_large_it_is() {
        // A stop and a half darker. Section 6.3's cap is the whole point of the row.
        let frames = wedding(0.60, 0.21, SceneId::Ceremony);
        let rows = measure(&frames, &CameraId::new("cam_a"), &Matching::default());
        let row = &rows[0];
        assert!(row.measured_ev < -1.4, "measured {}", row.measured_ev);
        assert!(row.applied_ev.abs() <= MAX_SHOOTER_EV + f32::EPSILON);
        assert!(row.capped);
    }

    #[test]
    fn a_small_difference_is_left_entirely_alone_and_says_so() {
        let frames = wedding(0.420, 0.410, SceneId::Ceremony);
        let rows = measure(&frames, &CameraId::new("cam_a"), &Matching::default());
        let row = &rows[0];
        assert_eq!(row.applied_ev, 0.0);
        assert!(row
            .reasons
            .iter()
            .any(|r| r.code == CameraCode::ShooterStylePreserved));
    }

    #[test]
    fn a_scene_where_the_exposure_is_the_photograph_is_not_measured_at_all() {
        // The exit: sparklers, a hand-held flame, and an exposure both photographers chose.
        let frames = wedding(0.40, 0.20, SceneId::Exit);
        let rows = measure(&frames, &CameraId::new("cam_a"), &Matching::default());
        assert!(
            rows.is_empty(),
            "nothing was measured, so there is nothing to report"
        );
    }

    #[test]
    fn too_few_frames_is_a_stated_absence_rather_than_a_zero() {
        let mut frames = Vec::new();
        for _ in 0..(MIN_SHOOTER_FRAMES as usize - 1) {
            frames.push(frame("cam_a", "primary", SceneId::Ceremony, 0.42));
            frames.push(frame("cam_b", "second", SceneId::Ceremony, 0.30));
        }
        let rows = measure(&frames, &CameraId::new("cam_a"), &Matching::default());
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].is_usable());
        assert_eq!(rows[0].measured_ev, 0.0);
        assert!(rows[0]
            .reasons
            .iter()
            .any(|r| r.code == CameraCode::ShooterBiasAbsent));
    }

    #[test]
    fn a_habit_is_measured_per_scene_and_not_across_the_wedding() {
        let mut frames = wedding(0.42, 0.30, SceneId::Ceremony);
        frames.extend(wedding(0.50, 0.50, SceneId::Speeches));
        let rows = measure(&frames, &CameraId::new("cam_a"), &Matching::default());
        assert_eq!(rows.len(), 2);
        let ceremony = rows
            .iter()
            .find(|r| r.scene == SceneId::Ceremony)
            .expect("ceremony row");
        let speeches = rows
            .iter()
            .find(|r| r.scene == SceneId::Speeches)
            .expect("speeches row");
        assert!(ceremony.applied_ev.abs() > 0.05);
        assert_eq!(speeches.applied_ev, 0.0, "they agree during the speeches");
    }

    #[test]
    fn the_offset_is_a_ratio_in_stops_and_not_a_subtraction() {
        // The same 0.10 difference, dark and bright. A subtraction would call these equal.
        let dark = offset_ev(0.15, 0.25);
        let bright = offset_ev(0.60, 0.70);
        assert!(
            dark.abs() > bright.abs() * 3.0,
            "dark {dark} bright {bright}"
        );
        assert_eq!(offset_ev(0.4, 0.4), 0.0);
        assert_eq!(offset_ev(0.0, 0.4), 0.0, "a black frame is not a habit");
    }

    #[test]
    fn the_folded_correction_is_frame_weighted_so_a_thin_scene_cannot_move_it() {
        let camera = CameraId::new("cam_b");
        let row = |scene, applied: f32, frames: u32| ShooterBias {
            shooter: "second".to_string(),
            camera_id: camera.clone(),
            scene,
            measured_ev: applied * 2.0,
            applied_ev: applied,
            frames,
            capped: true,
            reasons: Vec::new(),
            analysis_ver: 1,
        };
        let rows = vec![
            row(SceneId::Ceremony, 0.20, 400),
            row(SceneId::Cake, -0.30, 20),
        ];
        let folded = folded_ev(&rows, &camera);
        assert!(folded > 0.16 && folded < 0.20, "{folded}");
        assert_eq!(folded_ev(&rows, &CameraId::new("cam_c")), 0.0);
    }

    #[test]
    fn a_silhouette_is_excluded_rather_than_clamped() {
        let mut frames = wedding(0.42, 0.40, SceneId::Ceremony);
        for _ in 0..20 {
            frames.push(frame("cam_b", "second", SceneId::Ceremony, 0.005));
        }
        let rows = measure(&frames, &CameraId::new("cam_a"), &Matching::default());
        let row = &rows[0];
        assert_eq!(
            row.frames, 40,
            "the silhouettes are not evidence about a habit"
        );
    }
}
