//! The grouping engine's unit tests, module by module.
//!
//! An integration test rather than six `#[cfg(test)]` modules inside the source, which
//! is this workspace's convention and is enforced rather than merely preferred:
//! `scripts/check-banned.sh` exempts `*/tests/*` from the `unwrap`/`expect`/`panic!`
//! ban and exempts nothing inside `src/`. A test that cannot say `expect("profiles")`
//! is a test that has to thread a `Result` through its own arrangement, which is noise
//! in the one place a reader is trying to see the claim.
//!
//! Everything here reaches the modules through their public API, which is a second
//! benefit: a test that needs a private item is usually a test of an implementation
//! detail, and the six modules were designed so that none of these does.

/// The types every module below needs, in one place.
///
/// A prelude rather than six copies of the same six `use` lines: the modules test one
/// engine, and a reader comparing `merge`'s arrangement with `burst`'s should not have to
/// check whether they imported the same things.
mod prelude {
    pub use aura_core::contract::moment::{DuplicateKind, DuplicateSet, ImageId, Moment};
    pub use aura_core::{PhotoId, SceneId};
    pub use std::collections::BTreeSet;
}

// ---------------------------------------------------------------------------
// cadence
// ---------------------------------------------------------------------------

mod cadence_tests {
    use aura_brain_wedding::moments::cadence::*;
    use aura_core::PhotoId;

    fn shot(ts: i64, camera: u32) -> Shot {
        Shot {
            image: PhotoId::new(),
            ts,
            camera,
            drive: Drive::NONE,
        }
    }

    #[test]
    fn a_ten_frame_per_second_burst_collapses_the_window_to_its_floor() {
        let shots: Vec<Shot> = (0..40).map(|i| shot(1_000 + i * 100, 1)).collect();
        let cadence = Cadence::estimate(&shots);
        // 2.5 x 100 ms is 250 ms, below the floor.
        assert_eq!(cadence.window_ms(1, 1_000), MIN_WINDOW_MS);
        assert_eq!(cadence.median_ms(), 100);
    }

    #[test]
    fn a_slow_ceremony_stretches_the_window_towards_its_ceiling() {
        // One frame every five seconds for two minutes.
        let shots: Vec<Shot> = (0..24).map(|i| shot(i * 5_000, 1)).collect();
        let cadence = Cadence::estimate(&shots);
        // 2.5 x 5 s is 12.5 s, above the ceiling.
        assert_eq!(cadence.window_ms(1, 30_000), MAX_WINDOW_MS);
    }

    #[test]
    fn a_moderate_cadence_lands_between_the_clamps() {
        // One frame every second: 2.5 s, inside 0.7..8.
        let shots: Vec<Shot> = (0..90).map(|i| shot(i * 1_000, 1)).collect();
        let cadence = Cadence::estimate(&shots);
        assert_eq!(cadence.window_ms(1, 45_000), 2_500);
    }

    #[test]
    fn two_shooters_do_not_halve_each_others_windows() {
        // Two bodies, each at one frame per second, interleaved 500 ms apart. The
        // combined median interval is 500 ms and each body's is 1,000.
        let mut shots = Vec::new();
        for i in 0..60 {
            shots.push(shot(i * 1_000, 1));
            shots.push(shot(i * 1_000 + 500, 2));
        }
        let cadence = Cadence::estimate(&shots);
        assert_eq!(cadence.window_ms(1, 30_000), 2_500);
        assert_eq!(cadence.window_ms(2, 30_000), 2_500);
        // And the reported median is each body's own, not the interleaved 500 ms -
        // which is the number a photographer means by "how fast were we shooting".
        assert_eq!(cadence.median_ms(), 1_000);
        assert_eq!(cadence.cameras(), 2);
    }

    #[test]
    fn the_window_follows_the_day_rather_than_averaging_it() {
        // Two minutes of slow shooting, then a burst, then slow again.
        let mut shots: Vec<Shot> = (0..24).map(|i| shot(i * 5_000, 1)).collect();
        let burst_start = 120_000;
        shots.extend((0..40).map(|i| shot(burst_start + i * 100, 1)));
        shots.extend((0..24).map(|i| shot(200_000 + i * 5_000, 1)));

        let cadence = Cadence::estimate(&shots);
        assert_eq!(cadence.window_ms(1, 30_000), MAX_WINDOW_MS);
        assert_eq!(cadence.window_ms(1, burst_start + 2_000), MIN_WINDOW_MS);
        assert_eq!(cadence.window_ms(1, 260_000), MAX_WINDOW_MS);
    }

    #[test]
    fn continuous_drive_beats_the_estimate_and_a_second_camera_beats_both() {
        let mut left = shot(0, 1);
        let mut right = shot(3_000, 1);
        assert!(!Cadence::drive_pair(&left, &right));

        left.drive.continuous = true;
        right.drive.continuous = true;
        assert!(Cadence::drive_pair(&left, &right));

        right.camera = 2;
        assert!(!Cadence::drive_pair(&left, &right));
    }

    #[test]
    fn two_frames_in_one_second_with_different_sub_seconds_are_a_burst() {
        let mut left = shot(1_000, 1);
        let mut right = shot(1_800, 1);
        left.drive.sub_sec = Some(10);
        right.drive.sub_sec = Some(80);
        assert!(Cadence::drive_pair(&left, &right));

        // The same sub-second field twice is one frame counted twice, not two frames.
        right.drive.sub_sec = Some(10);
        right.ts = 1_900;
        assert!(!Cadence::drive_pair(&left, &right));
    }

    #[test]
    fn a_hard_gap_is_twenty_seconds_and_not_a_scaled_number() {
        assert!(!Cadence::is_hard_gap(HARD_GAP_MS));
        assert!(Cadence::is_hard_gap(HARD_GAP_MS + 1));
    }

    #[test]
    fn an_unknown_camera_gets_the_widest_window_rather_than_the_narrowest() {
        let cadence = Cadence::estimate(&[shot(0, 1)]);
        assert_eq!(cadence.window_ms(99, 0), MAX_WINDOW_MS);
    }

    #[test]
    fn camera_handles_are_stable_across_machines() {
        let mut table = CameraTable::new(vec!["cam_b".into(), "cam_a".into()]);
        assert_eq!(table.handle("cam_a"), 0);
        assert_eq!(table.handle("cam_b"), 1);
        // A body seen for the first time is added rather than merged into another.
        assert_eq!(table.handle("cam_c"), 2);
        assert_eq!(table.label(1).as_str(), "cam_b");
        assert!(table.label(9).is_unknown());
        assert_eq!(table.len(), 3);
    }
}

// ---------------------------------------------------------------------------
// graph
// ---------------------------------------------------------------------------

mod graph_tests {
    use crate::prelude::*;
    use aura_brain_wedding::moments::cadence::{Cadence, Drive};
    use aura_brain_wedding::moments::graph::*;
    use aura_core::PhotoId;
    use std::collections::BTreeSet;

    fn frame(ts: i64, camera: u32, embedding: Vec<f32>, dhash: u64) -> Frame {
        Frame {
            image: PhotoId::new(),
            ts,
            camera,
            drive: Drive::NONE,
            embedding,
            dhash,
            identities: BTreeSet::new(),
            scene: SceneId::Unknown,
            classified: false,
            edge_energy: 0.5,
            subject_focus: 0.0,
        }
    }

    /// A unit vector at an angle, so a test can set a cosine distance by choosing one.
    fn unit(direction: f32) -> Vec<f32> {
        let mut out = vec![0.0f32; 8];
        if let Some(x) = out.first_mut() {
            *x = direction.cos();
        }
        if let Some(y) = out.get_mut(1) {
            *y = direction.sin();
        }
        out
    }

    #[test]
    fn the_shipped_thresholds_load_and_name_the_scenes_they_argue_about() {
        let registry = MomentProfileRegistry::embedded().expect("embedded profiles load");
        assert_eq!(registry.version(), 1);
        assert!(registry.overrides() >= 10);
        // The two section 6.2 names explicitly, in the documented direction.
        assert!(
            registry.get(SceneId::DanceFloor).edge_threshold
                < registry.get(SceneId::FamilyPortrait).edge_threshold
        );
        // A scene with no row takes the defaults rather than a refusal.
        assert_eq!(registry.get(SceneId::Candid), registry.get(SceneId::Vows));
    }

    #[test]
    fn every_shipped_threshold_is_reachable_without_faces() {
        let registry = MomentProfileRegistry::embedded().expect("embedded profiles load");
        let ceiling = W_EMBED + W_DHASH + W_CAMERA;
        for scene in SceneId::ALL {
            assert!(
                registry.get(scene).edge_threshold <= ceiling,
                "{scene} is unreachable for a frame with no faces"
            );
        }
    }

    #[test]
    fn a_profile_without_a_rationale_is_refused() {
        let text = r#"
version = 1
[defaults]
edge_threshold = 0.7
window_scale = 1.0
max_group = 60
rationale = "because"
"#;
        let refused = MomentProfileRegistry::parse("test", text).expect_err("refused");
        assert_eq!(refused.code.0, "AURA-ML-5031");
    }

    #[test]
    fn a_threshold_no_faceless_frame_could_reach_is_refused() {
        let text = r#"
version = 1
[defaults]
edge_threshold = 0.9
window_scale = 1.0
max_group = 60
rationale = "a long enough sentence to pass the rationale rule"
"#;
        let refused = MomentProfileRegistry::parse("test", text).expect_err("refused");
        assert_eq!(refused.code.0, "AURA-ML-5031");
        assert!(refused.detail.contains("would ever group"));
    }

    #[test]
    fn visual_similarity_alone_cannot_group_two_frames() {
        // Identical embeddings, identical hashes, different bodies, no shared faces.
        let left = frame(0, 1, unit(0.0), 0);
        let right = frame(100, 2, unit(0.0), 0);
        let score = score_pair(&left, &right).total();
        // 0.55 + 0.20 = 0.75, which clears the 0.68 default but not by the embedding
        // alone: dropping the hash term takes it to 0.55, below every threshold.
        let embed_only = score_pair(&left, &right).embed;
        let registry = MomentProfileRegistry::embedded().expect("profiles");
        assert!(embed_only < registry.get(SceneId::Unknown).edge_threshold);
        assert!(score > embed_only);
    }

    #[test]
    fn a_scene_mismatch_costs_a_pair_the_documented_penalty() {
        let mut left = frame(0, 1, unit(0.0), 0);
        let mut right = frame(100, 1, unit(0.0), 0);
        left.classified = true;
        right.classified = true;
        left.scene = SceneId::Ceremony;
        right.scene = SceneId::Ceremony;
        let same = score_pair(&left, &right).total();

        right.scene = SceneId::DanceFloor;
        let different = score_pair(&left, &right).total();
        assert!((same - different - SCENE_MISMATCH_PENALTY).abs() < 1e-5);

        // An unclassified wedding pays nothing.
        left.classified = false;
        right.classified = false;
        assert!((score_pair(&left, &right).total() - same).abs() < 1e-5);
    }

    #[test]
    fn a_hard_gap_stops_the_sweep_whatever_the_frames_look_like() {
        let cadence = Cadence::estimate(&[]);
        let profiles = MomentProfileRegistry::embedded().expect("profiles");
        let frames = vec![
            frame(0, 1, unit(0.0), 0),
            frame(
                aura_brain_wedding::moments::cadence::HARD_GAP_MS + 1,
                1,
                unit(0.0),
                0,
            ),
        ];
        assert!(candidates(&frames, &cadence, &profiles).is_empty());
    }

    #[test]
    fn union_find_is_deterministic_and_orders_groups_by_first_member() {
        let mut sets = UnionFind::new(6);
        assert!(sets.union(4, 5));
        assert!(sets.union(0, 1));
        assert!(!sets.union(1, 0));
        let groups = sets.groups();
        assert_eq!(groups, vec![vec![0, 1], vec![2], vec![3], vec![4, 5]]);
    }

    #[test]
    fn an_oversized_group_is_split_rather_than_accepted() {
        let profiles = MomentProfileRegistry::embedded().expect("profiles");
        // Ninety frames that all look identical: no re-cluster can separate them, so
        // the cut-on-gap fallback has to hold the cap.
        let frames: Vec<Frame> = (0..90).map(|i| frame(i * 100, 1, unit(0.0), 0)).collect();
        let edges: Vec<Edge> = (0..89)
            .map(|i| Edge {
                left: i,
                right: i + 1,
                score: 0.9,
                embed_dist: 0.0,
                dhash_dist: 0,
                drive_evidence: false,
                cross_camera: false,
            })
            .collect();
        let groups = group(&frames, &edges, &profiles);
        assert!(groups.len() > 1);
        let cap = profiles.get(SceneId::Unknown).max_group;
        for found in &groups {
            assert!(found.len() <= cap, "a group of {} survived", found.len());
        }
        // Nothing was lost.
        assert_eq!(groups.iter().map(Vec::len).sum::<usize>(), 90);
    }
}

// ---------------------------------------------------------------------------
// burst
// ---------------------------------------------------------------------------

mod burst_tests {
    use aura_brain_wedding::moments::burst::*;
    use aura_brain_wedding::moments::cadence::Cadence;
    use aura_brain_wedding::moments::cadence::Drive;
    use aura_brain_wedding::moments::graph::Frame;
    use aura_core::{PhotoId, SceneId};
    use std::collections::BTreeSet;

    fn frame(ts: i64, camera: u32, drift: f32) -> Frame {
        // A vector that walks slowly away from (1,0) as `drift` grows, so a test can
        // set a cosine distance directly.
        let mut embedding = vec![0.0f32; 8];
        if let Some(x) = embedding.first_mut() {
            *x = (1.0 - drift * drift).max(0.0).sqrt();
        }
        if let Some(y) = embedding.get_mut(1) {
            *y = drift;
        }
        Frame {
            image: PhotoId::new(),
            ts,
            camera,
            drive: Drive::NONE,
            embedding,
            dhash: 0,
            identities: BTreeSet::new(),
            scene: SceneId::Unknown,
            classified: false,
            edge_energy: 0.5,
            subject_focus: 0.0,
        }
    }

    #[test]
    fn a_ten_frame_per_second_run_is_one_burst() {
        let frames: Vec<Frame> = (0..12).map(|i| frame(i * 100, 1, 0.0)).collect();
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let members: Vec<usize> = (0..12).collect();
        let bursts = partition(&frames, &members, &cadence);
        assert_eq!(bursts.len(), 1);
        assert_eq!(bursts.first().map(Vec::len), Some(12));
    }

    #[test]
    fn three_takes_of_one_thing_are_three_bursts_in_one_moment() {
        // Three runs of four frames at 10 fps, five seconds apart. One moment by the
        // graph's reckoning; three presses of the shutter by the camera's.
        let mut frames = Vec::new();
        for take in 0..3i64 {
            for shot in 0..4i64 {
                frames.push(frame(take * 5_000 + shot * 100, 1, 0.0));
            }
        }
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let members: Vec<usize> = (0..frames.len()).collect();
        let bursts = partition(&frames, &members, &cadence);
        assert_eq!(bursts.len(), 3);
        for burst in &bursts {
            assert_eq!(burst.len(), 4);
        }
    }

    #[test]
    fn two_shooters_do_not_share_a_burst() {
        // Interleaved: even indices camera 1, odd camera 2, 100 ms apart.
        let frames: Vec<Frame> = (0..12)
            .map(|i| frame(i * 100, if i % 2 == 0 { 1 } else { 2 }, 0.0))
            .collect();
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let members: Vec<usize> = (0..12).collect();
        let bursts = partition(&frames, &members, &cadence);
        assert_eq!(bursts.len(), 2);
        for burst in &bursts {
            let cameras: BTreeSet<u32> = burst
                .iter()
                .filter_map(|index| frames.get(*index).map(|frame| frame.camera))
                .collect();
            assert_eq!(cameras.len(), 1, "a burst spanned two bodies");
        }
    }

    #[test]
    fn every_frame_lands_in_exactly_one_burst() {
        let frames: Vec<Frame> = (0..30)
            .map(|i| frame(i * 900, if i % 3 == 0 { 2 } else { 1 }, 0.02 * i as f32))
            .collect();
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let members: Vec<usize> = (0..30).collect();
        let bursts = partition(&frames, &members, &cadence);

        let mut seen: Vec<usize> = bursts.iter().flatten().copied().collect();
        seen.sort_unstable();
        assert_eq!(
            seen, members,
            "the bursts are not a partition of the moment"
        );
    }

    #[test]
    fn a_frame_that_looks_different_breaks_the_burst_even_inside_the_window() {
        // 400 ms apart: inside the window the cadence estimator derives from a 400 ms
        // median, and above `DRIVE_INTERVAL_MS`, so the camera-arithmetic paths do not
        // fire and path 4 has to decide. It needs BOTH time and appearance.
        let frames = vec![frame(0, 1, 0.0), frame(400, 1, 0.0), frame(800, 1, 0.6)];
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        assert!(
            cadence.window_ms(1, 400) >= 800,
            "the third frame is inside the window"
        );
        let bursts = partition(&frames, &[0, 1, 2], &cadence);
        // The first two are one burst; the third stands alone.
        assert_eq!(bursts.len(), 2);
    }

    #[test]
    fn continuous_drive_holds_a_burst_together_through_a_visual_change() {
        let mut frames = vec![frame(0, 1, 0.0), frame(300, 1, 0.6)];
        for entry in &mut frames {
            entry.drive.continuous = true;
        }
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let bursts = partition(&frames, &[0, 1], &cadence);
        assert_eq!(bursts.len(), 1, "the camera said the shutter was held down");
    }

    #[test]
    fn an_empty_moment_has_no_bursts() {
        let cadence = Cadence::estimate(&[]);
        assert!(partition(&[], &[], &cadence).is_empty());
    }
}

// ---------------------------------------------------------------------------
// duplicate
// ---------------------------------------------------------------------------

mod duplicate_tests {
    use crate::prelude::*;
    use aura_brain_wedding::moments::cadence::Drive;
    use aura_brain_wedding::moments::duplicate::*;
    use aura_brain_wedding::moments::graph::Frame;
    use aura_core::{PhotoId, SceneId};
    use std::collections::BTreeSet;

    fn frame(dhash: u64, drift: f32, focus: f32) -> Frame {
        let mut embedding = vec![0.0f32; 8];
        if let Some(x) = embedding.first_mut() {
            *x = (1.0 - drift * drift).max(0.0).sqrt();
        }
        if let Some(y) = embedding.get_mut(1) {
            *y = drift;
        }
        Frame {
            image: PhotoId::new(),
            ts: 0,
            camera: 1,
            drive: Drive::NONE,
            embedding,
            dhash,
            identities: BTreeSet::new(),
            scene: SceneId::Unknown,
            classified: false,
            edge_energy: 0.5,
            subject_focus: focus,
        }
    }

    fn measured(iou: f32) -> FaceOverlap {
        FaceOverlap {
            iou,
            measured: true,
        }
    }

    #[test]
    fn all_three_tests_must_agree_for_near_identical() {
        let left = frame(0, 0.0, 0.5);
        let right = frame(0b11, 0.0, 0.5);

        // Hash and appearance agree, faces agree: near identical.
        assert_eq!(
            classify(&left, &right, measured(0.95), false).kind,
            DuplicateKind::NearIdentical
        );
        // The faces have moved. One failing test is enough.
        assert_eq!(
            classify(&left, &right, measured(0.4), false).kind,
            DuplicateKind::Variant
        );
        // The hash has moved past the bound.
        let far = frame(0b1_1111_1111, 0.0, 0.5);
        assert_eq!(
            classify(&left, &far, measured(0.95), false).kind,
            DuplicateKind::Variant
        );
        // The appearance has moved past the bound.
        let different = frame(0, 0.4, 0.5);
        assert_eq!(
            classify(&left, &different, measured(0.95), false).kind,
            DuplicateKind::Variant
        );
    }

    #[test]
    fn bracketed_exposures_are_near_identical() {
        // Section 10.1 names this case: "including one-stop-apart exposures". A
        // difference hash compares neighbouring pixels rather than absolute levels, so
        // a stop of exposure moves it by a bit or two and not more, and the embedding
        // runs on normalised pixels.
        let base = frame(0x0F0F_0F0F_0F0F_0F0F, 0.0, 0.6);
        let one_stop_up = frame(0x0F0F_0F0F_0F0F_0F0D, 0.01, 0.6);
        let verdict = classify(&base, &one_stop_up, measured(0.98), false);
        assert_eq!(verdict.kind, DuplicateKind::NearIdentical);
        assert!(verdict.confidence > 0.5);
    }

    #[test]
    fn a_detail_shot_with_no_faces_skips_the_face_test_rather_than_failing_it() {
        let left = frame(0, 0.0, 0.0);
        let right = frame(0b1, 0.0, 0.0);
        let verdict = classify(&left, &right, FaceOverlap::default(), false);
        assert_eq!(verdict.kind, DuplicateKind::NearIdentical);
        assert!(verdict.face_skipped);
        // And it pays for the skip in its confidence rather than getting it free.
        let with_faces = classify(&left, &right, measured(1.0), false);
        assert!(with_faces.confidence > verdict.confidence);
    }

    #[test]
    fn the_same_file_twice_is_identical_and_certain() {
        let left = frame(0, 0.0, 0.5);
        let right = frame(0xFFFF_FFFF_FFFF_FFFF, 0.9, 0.5);
        // Even with everything else disagreeing: the bytes are the bytes.
        let verdict = classify(&left, &right, measured(0.0), true);
        assert_eq!(verdict.kind, DuplicateKind::Identical);
        assert!((verdict.confidence - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_keep_hint_prefers_face_sharpness_over_frame_sharpness() {
        let mut sharp_frame_soft_face = frame(0, 0.0, 0.1);
        sharp_frame_soft_face.edge_energy = 0.95;
        let mut soft_frame_sharp_face = frame(0, 0.0, 0.9);
        soft_frame_sharp_face.edge_energy = 0.30;

        let frames = vec![sharp_frame_soft_face, soft_frame_sharp_face];
        assert_eq!(keep_hint(&frames, &[0, 1]), Some(1));
    }

    #[test]
    fn with_no_face_pass_the_hint_falls_through_to_edge_energy() {
        let mut soft = frame(0, 0.0, 0.0);
        soft.edge_energy = 0.2;
        let mut sharp = frame(0, 0.0, 0.0);
        sharp.edge_energy = 0.8;
        let frames = vec![soft, sharp];
        assert_eq!(keep_hint(&frames, &[0, 1]), Some(1));
    }

    #[test]
    fn near_identical_sets_are_transitive() {
        // A and B agree, B and C agree, A and C are two bits apart - all one set.
        let frames = vec![
            frame(0b0000, 0.0, 0.5),
            frame(0b0001, 0.0, 0.5),
            frame(0b0011, 0.0, 0.5),
        ];
        let sets = sets_of_moment(
            &frames,
            &[0, 1, 2],
            &[vec![0, 1, 2]],
            &|_, _| measured(0.99),
            &|_, _| false,
        );
        assert_eq!(sets.len(), 1);
        assert_eq!(sets.first().map(DuplicateSet::len), Some(3));
        assert_eq!(
            sets.first().map(|set| set.kind),
            Some(DuplicateKind::NearIdentical)
        );
    }

    #[test]
    fn frames_of_one_burst_that_differ_are_variants_and_are_not_stored_as_a_set() {
        let left = frame(0, 0.0, 0.5);
        let right = frame(0xFFFF, 0.5, 0.5);
        // The pair is classified, and the classification is `Variant`.
        assert_eq!(
            classify(&left, &right, measured(0.99), false).kind,
            DuplicateKind::Variant
        );
        // Every frame stays eligible, so nothing constrains delivery and no set is
        // written. The two frames are already the alternatives, in `Moment::bursts`.
        let frames = vec![left, right];
        let sets = sets_of_moment(
            &frames,
            &[0, 1],
            &[vec![0, 1]],
            &|_, _| measured(0.99),
            &|_, _| false,
        );
        assert!(sets.is_empty());
        assert!(!DuplicateKind::Variant.caps_gallery_to_one());
    }

    #[test]
    fn a_single_frame_moment_has_no_sets() {
        let frames = vec![frame(0, 0.0, 0.5)];
        assert!(
            sets_of_moment(&frames, &[0], &[vec![0]], &|_, _| measured(1.0), &|_, _| {
                false
            })
            .is_empty()
        );
    }

    #[test]
    fn every_stored_set_carries_reasons_within_the_cap() {
        let frames = vec![
            frame(0b0000, 0.0, 0.5),
            frame(0b0001, 0.0, 0.5),
            frame(0b0010, 0.0, 0.5),
            frame(0b0011, 0.0, 0.5),
        ];
        let sets = sets_of_moment(
            &frames,
            &[0, 1, 2, 3],
            &[vec![0, 1, 2, 3]],
            &|_, _| measured(0.99),
            &|_, _| false,
        );
        assert!(!sets.is_empty(), "four near-identical frames stored no set");
        for set in &sets {
            assert!(!set.reasons.is_empty(), "invariant 2");
            assert!(set.reasons.len() <= DuplicateSet::MAX_REASONS);
            assert!(set.confidence > 0.0);
            assert!(
                set.kind.caps_gallery_to_one(),
                "a set that constrains nothing was stored"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// merge
// ---------------------------------------------------------------------------

mod merge_tests {
    use crate::prelude::*;
    use aura_brain_wedding::moments::cadence::Drive;
    use aura_brain_wedding::moments::graph::Frame;
    use aura_brain_wedding::moments::merge::*;
    use aura_core::{PhotoId, SceneId};
    use std::collections::BTreeSet;

    fn frame(ts: i64, camera: u32, drift: f32) -> Frame {
        let mut embedding = vec![0.0f32; 8];
        if let Some(x) = embedding.first_mut() {
            *x = (1.0 - drift * drift).max(0.0).sqrt();
        }
        if let Some(y) = embedding.get_mut(1) {
            *y = drift;
        }
        Frame {
            image: PhotoId::new(),
            ts,
            camera,
            drive: Drive::NONE,
            embedding,
            dhash: 0,
            identities: BTreeSet::new(),
            scene: SceneId::Unknown,
            classified: false,
            edge_energy: 0.5,
            subject_focus: 0.0,
        }
    }

    #[test]
    fn two_shooters_on_one_kiss_become_one_moment() {
        // Camera 1 shoots six frames over three seconds; camera 2 shoots five over the
        // same three seconds from a slightly different angle.
        let mut frames = Vec::new();
        for i in 0..6 {
            frames.push(frame(i * 500, 1, 0.02));
        }
        for i in 0..5 {
            frames.push(frame(i * 600 + 100, 2, 0.05));
        }
        let left = MomentSpan::of(&frames, &[0, 1, 2, 3, 4, 5]);
        let right = MomentSpan::of(&frames, &[6, 7, 8, 9, 10]);

        let merges = cross_camera(&frames, &[left.clone(), right.clone()]);
        assert_eq!(merges.len(), 1);
        let merge = merges.first().copied().expect("one merge");
        assert!(merge.overlap >= CROSS_CAMERA_OVERLAP);
        assert!(merge.medoid_distance <= CROSS_CAMERA_MEDOID);

        let groups = apply(
            &[left.members.clone(), right.members.clone()],
            &merges,
            &frames,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups.first().map(Vec::len), Some(11));
        // And the frames came out in timeline order, which is what makes the bursts
        // recomputed over them come out per camera.
        let times: Vec<i64> = groups
            .first()
            .into_iter()
            .flatten()
            .filter_map(|index| frames.get(*index).map(|frame| frame.ts))
            .collect();
        assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn two_family_groups_shot_simultaneously_from_two_sides_do_not_merge() {
        // Perfect temporal overlap, genuinely different photographs.
        let mut frames = Vec::new();
        for i in 0..5 {
            frames.push(frame(i * 400, 1, 0.0));
        }
        for i in 0..5 {
            frames.push(frame(i * 400 + 50, 2, 0.7));
        }
        let spans = vec![
            MomentSpan::of(&frames, &[0, 1, 2, 3, 4]),
            MomentSpan::of(&frames, &[5, 6, 7, 8, 9]),
        ];
        assert!(
            cross_camera(&frames, &spans).is_empty(),
            "the medoid test is what stops this"
        );
    }

    #[test]
    fn moments_that_barely_overlap_in_time_do_not_merge() {
        let mut frames = Vec::new();
        for i in 0..5 {
            frames.push(frame(i * 1_000, 1, 0.0));
        }
        // Starts as the first ends: identical appearance, 0 % overlap.
        for i in 0..5 {
            frames.push(frame(4_000 + i * 1_000, 2, 0.0));
        }
        let spans = vec![
            MomentSpan::of(&frames, &[0, 1, 2, 3, 4]),
            MomentSpan::of(&frames, &[5, 6, 7, 8, 9]),
        ];
        assert!(cross_camera(&frames, &spans).is_empty());
    }

    #[test]
    fn one_camera_never_merges_with_itself() {
        let frames: Vec<Frame> = (0..10).map(|i| frame(i * 200, 1, 0.0)).collect();
        let spans = vec![
            MomentSpan::of(&frames, &[0, 1, 2, 3, 4]),
            MomentSpan::of(&frames, &[5, 6, 7, 8, 9]),
        ];
        assert!(cross_camera(&frames, &spans).is_empty());
    }

    #[test]
    fn the_medoid_is_a_frame_and_ties_go_to_the_earliest() {
        let frames: Vec<Frame> = (0..4).map(|i| frame(i * 100, 1, 0.0)).collect();
        assert_eq!(medoid(&frames, &[0, 1, 2, 3]), Some(0));
        assert_eq!(medoid(&frames, &[2]), Some(2));
        assert_eq!(medoid(&frames, &[]), None);
    }

    #[test]
    fn a_split_puts_the_named_frame_at_the_head_of_the_new_moment() {
        let ids: Vec<ImageId> = (0..5).map(|_| PhotoId::new()).collect();
        let at = ids.get(2).copied().expect("five ids");
        let plan = plan_split(&ids, at).expect("a valid split");
        assert_eq!(plan.keep.len(), 2);
        assert_eq!(plan.moved.len(), 3);
        assert_eq!(plan.moved.first().copied(), Some(at));
    }

    #[test]
    fn a_split_that_would_empty_a_side_is_refused() {
        let ids: Vec<ImageId> = (0..3).map(|_| PhotoId::new()).collect();
        let first = ids.first().copied().expect("three ids");
        let refused = plan_split(&ids, first).expect_err("refused");
        assert_eq!(refused.code.0, "AURA-ML-5029");

        let stranger = PhotoId::new();
        assert_eq!(
            plan_split(&ids, stranger).expect_err("refused").code.0,
            "AURA-ML-5029"
        );
        let single = vec![first];
        assert_eq!(
            plan_split(&single, first).expect_err("refused").code.0,
            "AURA-ML-5029"
        );
    }

    #[test]
    fn a_merge_orders_by_time_and_refuses_a_shared_frame() {
        let a = PhotoId::new();
        let b = PhotoId::new();
        let c = PhotoId::new();
        let plan = plan_merge(&[(a, 100), (c, 300)], &[(b, 200)]).expect("a valid merge");
        assert_eq!(plan.members, vec![a, b, c]);

        let refused = plan_merge(&[(a, 100)], &[(a, 200)]).expect_err("refused");
        assert_eq!(refused.code.0, "AURA-ML-5029");
        assert_eq!(
            plan_merge(&[], &[(a, 100)]).expect_err("refused").code.0,
            "AURA-ML-5029"
        );
    }
}

// ---------------------------------------------------------------------------
// moment
// ---------------------------------------------------------------------------

mod moment_tests {
    use crate::prelude::*;
    use aura_brain_wedding::moments::cadence::Cadence;
    use aura_brain_wedding::moments::cadence::CameraTable;
    use aura_brain_wedding::moments::cadence::Drive;
    use aura_brain_wedding::moments::graph::{Frame, MomentProfileRegistry};
    use aura_brain_wedding::moments::moment::*;

    fn frame(ts: i64, camera: u32, drift: f32) -> Frame {
        let mut embedding = vec![0.0f32; 8];
        if let Some(x) = embedding.first_mut() {
            *x = (1.0 - drift * drift).max(0.0).sqrt();
        }
        if let Some(y) = embedding.get_mut(1) {
            *y = drift;
        }
        Frame {
            image: PhotoId::new(),
            ts,
            camera,
            drive: Drive::NONE,
            embedding,
            dhash: 0,
            identities: BTreeSet::new(),
            scene: SceneId::Unknown,
            classified: false,
            edge_energy: 0.5,
            subject_focus: 0.0,
        }
    }

    fn context(profiles: &MomentProfileRegistry) -> Assembly<'_> {
        Assembly {
            profiles,
            segment: None,
            embed_ver: 100,
            weakest_edge: Some(0.85),
            all_drive: false,
            reclustered: false,
            cross_camera: None,
        }
    }

    #[test]
    fn a_bracketed_static_subject_scores_zero_diversity() {
        let frames: Vec<Frame> = (0..5).map(|i| frame(i * 200, 1, 0.0)).collect();
        assert!(diversity_of(&frames, &[0, 1, 2, 3, 4]) < 0.01);
    }

    #[test]
    fn an_evolving_action_scores_high_diversity_and_earns_more_keepers() {
        let frames: Vec<Frame> = (0..5).map(|i| frame(i * 200, 1, 0.20 * i as f32)).collect();
        let diversity = diversity_of(&frames, &[0, 1, 2, 3, 4]);
        assert!(diversity > 0.2, "diversity was {diversity}");

        let profiles = MomentProfileRegistry::embedded().expect("profiles");
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let cameras = CameraTable::new(vec!["cam_a".into()]);
        let row = assemble(
            &frames,
            &[0, 1, 2, 3, 4],
            &cadence,
            &cameras,
            context(&profiles),
            &|_, _| aura_brain_wedding::moments::duplicate::FaceOverlap::default(),
            &|_, _| false,
        );
        assert!(row.moment.suggested_keepers() >= 1);
        assert_eq!(row.moment.len(), 5);
    }

    #[test]
    fn every_moment_carries_reasons_and_a_confidence() {
        let profiles = MomentProfileRegistry::embedded().expect("profiles");
        let frames: Vec<Frame> = (0..4).map(|i| frame(i * 200, 1, 0.0)).collect();
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let cameras = CameraTable::new(vec!["cam_a".into()]);
        let row = assemble(
            &frames,
            &[0, 1, 2, 3],
            &cadence,
            &cameras,
            context(&profiles),
            &|_, _| aura_brain_wedding::moments::duplicate::FaceOverlap::default(),
            &|_, _| false,
        );
        assert!(!row.moment.reasons.is_empty(), "invariant 2");
        assert!(row.moment.reasons.len() <= Moment::MAX_REASONS);
        assert!(row.moment.confidence > 0.5);
        // The bursts are a partition of the frames.
        let flattened: usize = row.moment.bursts.iter().map(Vec::len).sum();
        assert_eq!(flattened, row.moment.image_ids.len());
        assert_eq!(row.burst_of.len(), row.moment.image_ids.len());
    }

    #[test]
    fn a_singleton_says_why_it_is_alone() {
        let profiles = MomentProfileRegistry::embedded().expect("profiles");
        let frames = vec![frame(0, 1, 0.0)];
        let cadence = Cadence::estimate(&[frames[0].shot()]);
        let cameras = CameraTable::new(vec!["cam_a".into()]);
        let mut assembly = context(&profiles);
        assembly.weakest_edge = None;
        let row = assemble(
            &frames,
            &[0],
            &cadence,
            &cameras,
            assembly,
            &|_, _| aura_brain_wedding::moments::duplicate::FaceOverlap::default(),
            &|_, _| false,
        );
        assert!((row.moment.confidence - SINGLETON_CONFIDENCE).abs() < f32::EPSILON);
        assert!(row
            .moment
            .reasons
            .first()
            .is_some_and(|why| why.contains("close enough")));
    }

    #[test]
    fn a_moment_the_camera_vouched_for_is_near_certain() {
        let profiles = MomentProfileRegistry::embedded().expect("profiles");
        let frames: Vec<Frame> = (0..6).map(|i| frame(i * 100, 1, 0.0)).collect();
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let cameras = CameraTable::new(vec!["cam_a".into()]);
        let mut assembly = context(&profiles);
        assembly.all_drive = true;
        assembly.weakest_edge = Some(0.69);
        let row = assemble(
            &frames,
            &[0, 1, 2, 3, 4, 5],
            &cadence,
            &cameras,
            assembly,
            &|_, _| aura_brain_wedding::moments::duplicate::FaceOverlap::default(),
            &|_, _| false,
        );
        assert!(row.moment.confidence >= 0.95);
    }

    #[test]
    fn a_reclustered_moment_inherits_the_doubt() {
        let profiles = MomentProfileRegistry::embedded().expect("profiles");
        let frames: Vec<Frame> = (0..4).map(|i| frame(i * 100, 1, 0.0)).collect();
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let cameras = CameraTable::new(vec!["cam_a".into()]);
        let mut assembly = context(&profiles);
        assembly.reclustered = true;
        let row = assemble(
            &frames,
            &[0, 1, 2, 3],
            &cadence,
            &cameras,
            assembly,
            &|_, _| aura_brain_wedding::moments::duplicate::FaceOverlap::default(),
            &|_, _| false,
        );
        let plain = assemble(
            &frames,
            &[0, 1, 2, 3],
            &cadence,
            &cameras,
            context(&profiles),
            &|_, _| aura_brain_wedding::moments::duplicate::FaceOverlap::default(),
            &|_, _| false,
        );
        assert!(row.moment.confidence < plain.moment.confidence);
        assert!(row
            .moment
            .reasons
            .iter()
            .any(|why| why.contains("size cap")));
    }

    #[test]
    fn the_storage_estimate_matches_the_waived_budget() {
        // Section 11 asks for 200 and this phase measured 319; the waiver in ADR-0017
        // section 8 is at 340. This asserts the constant agrees with the waiver rather
        // than with the original budget, so that a schema change which pushed the cost
        // past the waiver would fail here as well as in `moment_budgets.rs`.
        //
        // Read through a binding rather than compared as a literal, because both
        // comparisons are constant at compile time and clippy is right that a constant
        // assertion proves nothing about a *run*. What it does prove is that somebody
        // editing the constant has to come here and re-read the waiver, which is the
        // whole point of the test.
        let claimed = BYTES_PER_IMAGE;
        assert!(
            claimed <= 340,
            "{claimed} bytes per image is past the recorded waiver"
        );
        assert!(
            claimed > 200,
            "the constant claims the unwaived budget is met; re-measure before changing it"
        );
    }
}
