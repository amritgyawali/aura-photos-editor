use super::*;

#[test]
fn a_refused_amount_steps_down_by_a_fifth_and_never_below_the_floor() {
    assert_eq!(amounts(1.0, false), vec![1.0]);
    assert_eq!(
        amounts(0.0, true),
        vec![0.0],
        "removing a match is never stepped"
    );
    let tried = amounts(1.6, true);
    assert_eq!(tried.first().copied(), Some(1.6));
    assert!(tried.len() <= MAX_ATTEMPTS as usize);
    for pair in tried.windows(2) {
        assert!((pair[1] - pair[0] * STEP).abs() < 1e-6);
    }
    assert!(tried.iter().skip(1).all(|a| *a >= MIN_AMOUNT));
    assert_eq!(amounts(0.16, true), vec![0.16], "0.128 is below the floor");
}

#[test]
fn the_refusals_we_step_down_from_are_the_ones_the_engines_raise() {
    // A reworded check would silently turn every Extreme into an error; this catches it.
    let sources = [
        include_str!("../../color_match.rs"),
        include_str!("../matcher.rs"),
    ];
    for prefix in RECOVERABLE {
        for source in sources {
            assert!(source.contains(prefix), "{prefix:?} missing from an engine");
        }
    }
    let refusal = crate::contract::ipc::IpcError::from(invalid(format!(
        "{}. Reduce the strengths and apply again.",
        RECOVERABLE[0]
    )));
    assert!(recoverable(&refusal));
    let stopped = crate::contract::ipc::IpcError::from(invalid(
        "Colour matching stopped. No edit was saved.",
    ));
    assert!(!recoverable(&stopped), "a cancellation is never retried");
}

fn scene(subjects: &[&str], brightness: f32, colourfulness: f32, orientation: i8) -> Scene {
    Scene {
        subjects: subjects.iter().map(|s| (*s).to_owned()).collect(),
        brightness,
        colourfulness,
        orientation,
    }
}

#[test]
fn references_of_the_same_kind_of_photograph_rank_first() {
    let target = scene(
        &["background", "clothes", "face skin", "hair"],
        0.55,
        0.2,
        -1,
    );
    let references = vec![
        (
            "b-landscape-dark".to_owned(),
            scene(&["background"], 0.15, 0.05, 1),
            false,
        ),
        (
            "a-portrait-alike".to_owned(),
            scene(
                &["background", "clothes", "face skin", "hair"],
                0.6,
                0.22,
                -1,
            ),
            false,
        ),
        (
            "c-portrait-dark".to_owned(),
            scene(&["background", "face skin", "hair"], 0.2, 0.2, -1),
            false,
        ),
    ];
    let ranked = rank(&target, &references);
    let order: Vec<&str> = ranked.iter().map(|r| r.reference_id.as_str()).collect();
    assert_eq!(
        order,
        ["a-portrait-alike", "c-portrait-dark", "b-landscape-dark"]
    );
    assert!(ranked[0].reasons.contains(&"Same subjects".to_owned()));
    assert!(ranked[0].reasons.contains(&"Similar brightness".to_owned()));
    assert!(ranked.iter().all(|r| (0.0..=1.0).contains(&r.score)));
}

#[test]
fn ranking_is_deterministic_and_bounded() {
    let target = scene(&[], 0.5, 0.1, 0);
    let references: Vec<_> = (0..30)
        .map(|i| (format!("{i:02}"), scene(&[], 0.5, 0.1, 0), false))
        .collect();
    let ranked = rank(&target, &references);
    assert_eq!(ranked.len(), MAX_RECOMMENDATIONS);
    assert_eq!(
        ranked[0].reference_id, "00",
        "equal scores fall back to the id"
    );
    assert_eq!(rank(&target, &references), ranked);
}

#[test]
fn a_favourite_breaks_a_near_tie_but_not_a_mismatch() {
    let target = scene(&["face skin"], 0.5, 0.2, -1);
    let alike = scene(&["face skin"], 0.5, 0.2, -1);
    let unlike = scene(&["background"], 0.1, 0.6, 1);
    let ranked = rank(
        &target,
        &[
            ("plain".to_owned(), alike.clone(), false),
            ("loved".to_owned(), alike, true),
            ("loved-but-wrong".to_owned(), unlike, true),
        ],
    );
    assert_eq!(ranked[0].reference_id, "loved");
    assert_eq!(ranked[2].reference_id, "loved-but-wrong");
}

#[test]
fn photos_without_people_are_not_matched_to_portraits_first() {
    let target = scene(&["background"], 0.5, 0.2, 1);
    let ranked = rank(
        &target,
        &[
            (
                "portrait".to_owned(),
                scene(&["background", "face skin"], 0.5, 0.2, 1),
                false,
            ),
            (
                "scene".to_owned(),
                scene(&["background"], 0.5, 0.2, 1),
                false,
            ),
        ],
    );
    assert_eq!(ranked[0].reference_id, "scene");
}

#[test]
fn image_key_reads_brightness_and_colourfulness() {
    let grey = [128_u8; 32 * 32 * 3];
    let (l, c) = key_of(&grey);
    assert!((l - 128.0 / 255.0).abs() < 0.01);
    assert!(c.abs() < 1e-6);
    let red: Vec<u8> = (0..32 * 32).flat_map(|_| [255_u8, 0, 0]).collect();
    let (_, c) = key_of(&red);
    assert!((c - 1.0).abs() < 1e-6);
    assert_eq!(key_of(&[]), (0.5, 0.0));
}

#[test]
fn preset_names_are_short_and_printable() {
    assert!(preset_name(Some("  Warm church  ")).is_ok_and(|n| n == "Warm church"));
    assert!(preset_name(Some("")).is_err());
    assert!(preset_name(None).is_err());
    assert!(preset_name(Some("tab\there")).is_err());
    assert!(preset_name(Some(&"x".repeat(61))).is_err());
}

/// Which check refuses which amount on a real photograph, and how long each step takes.
/// `AURA_ONE_CLICK_TARGET=<raw or jpeg> AURA_ONE_CLICK_REFERENCE=<edited jpeg>`, release build.
#[test]
#[ignore = "needs a real photograph and reference"]
#[allow(clippy::too_many_lines, clippy::print_stderr, clippy::expect_used)]
fn real_photo_diagnosis() {
    use crate::contract::ipc::{CreateProjectInput, ListImagesInput};
    use aura_core::progress::NullProgress;
    use aura_render::FrameSource;
    let (Ok(target), Ok(reference)) = (
        std::env::var("AURA_ONE_CLICK_TARGET"),
        std::env::var("AURA_ONE_CLICK_REFERENCE"),
    ) else {
        return;
    };
    let directory = tempfile::tempdir().expect("isolated");
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("catalogue")
        .with_cache_root(&directory.path().join("cache"))
        .with_key_store(std::sync::Arc::new(
            aura_cloud::keys::MemoryKeyStore::default(),
        ));
    let project = crate::create_project(
        &state,
        CreateProjectInput {
            name: "diagnosis".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let project_id = ProjectId::from_db(&project.id).expect("project id");
    aura_ingest::run(
        state.catalog(),
        &aura_ingest::contract::ingest::ImportPlan {
            import_id: aura_core::ImportId::new(),
            project_id,
            roots: vec![std::path::PathBuf::from(&target)],
            mode: aura_ingest::contract::ingest::ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let photo_row = crate::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 1,
            order_by: None,
        },
    )
    .expect("list");
    let photo_id = photo_row.first().expect("photo").id.clone();
    let photo = PhotoId::from_db(&photo_id).expect("photo id");
    let clock = std::time::Instant::now(); // ALLOW-BANNED: test timing
    let summary = super::super::import_reference(
        &state,
        &super::super::ImportInput {
            path: reference,
            cancel_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .expect("reference");
    eprintln!("import reference: {:.1}s", clock.elapsed().as_secs_f32());
    ensure_baseline(&state, &photo_id, photo).expect("baseline");
    let clock = std::time::Instant::now(); // ALLOW-BANNED: test timing
    let prepared = super::super::prepare(
        &state,
        &PrepareInput {
            photo_id: photo_id.clone(),
            reference_id: summary.id.clone(),
            cancel_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .expect("prepare");
    eprintln!(
        "prepare: {:.1}s, engine {:?}, predicted {:.2} -> {:.2}",
        clock.elapsed().as_secs_f32(),
        prepared.fitting.as_ref().map(|f| f.engine.clone()),
        prepared.before_distance,
        prepared.after_distance
    );
    let session: super::super::Session =
        read(&state, &super::super::session_key(&photo_id).expect("key"))
            .expect("read")
            .expect("session");
    let frame = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, aura_render::RenderLevel::Full)
        .expect("frame");
    let clock = std::time::Instant::now(); // ALLOW-BANNED: test timing
    let before = rendered(&state, &frame, &session.baseline).expect("baseline render");
    eprintln!(
        "full render {}x{}: {:.1}s",
        before.width,
        before.height,
        clock.elapsed().as_secs_f32()
    );
    let before_regions = appearance::readings(&before, &session.target_regions);
    let before_clip = appearance::clipped(&before);
    let transfer = session.transfer.clone().expect("transfer session");
    let before_distance = super::super::matcher::measured(&transfer.reference, &before);
    for amount in [1.0_f32, 0.64, 0.41] {
        let controls = Controls {
            amount,
            ..Controls::default()
        };
        let candidate =
            super::super::matcher::candidate(&session, &transfer, &controls).expect("candidate");
        let clock = std::time::Instant::now(); // ALLOW-BANNED: test timing
        let image = rendered(&state, &frame, &candidate).expect("render");
        let render_s = clock.elapsed().as_secs_f32();
        let after = appearance::readings(&image, &session.target_regions);
        let clip = appearance::clipped(&image);
        let readings_s = clock.elapsed().as_secs_f32() - render_s;
        let distance = super::super::matcher::measured(&transfer.reference, &image);
        eprintln!(
            "  timing: render {render_s:.1}s, readings+clip {readings_s:.1}s, distance {:.1}s",
            clock.elapsed().as_secs_f32() - render_s - readings_s
        );
        let small_frame = fitting_frame(&state, &photo).expect("small frame");
        let small_base = rendered(&state, &small_frame, &session.baseline).expect("small base");
        let small = rendered(&state, &small_frame, &candidate).expect("small render");
        let small_before = appearance::readings(&small_base, &session.target_regions);
        let small_after = appearance::readings(&small, &session.target_regions);
        eprintln!(
            "  512px: clip {:.4} -> {:.4} skin ok {} distance {:.2} -> {:.2}",
            appearance::clipped(&small_base),
            appearance::clipped(&small),
            super::super::skin_guard(&small_before, &small_after, session.reference_monochrome),
            super::super::matcher::measured(&transfer.reference, &small_base).total,
            super::super::matcher::measured(&transfer.reference, &small).total
        );
        let mut skin = Vec::new();
        for a in before_regions.iter().filter(|r| r.name.ends_with("skin")) {
            if let Some(b) = after.iter().find(|b| b.name == a.name) {
                skin.push(format!(
                    "{}: sh {:.1} mid {:.1} hi {:.1} chroma {:.1} tone {:.3}",
                    a.name,
                    a.appearance.shadow.distance(b.appearance.shadow),
                    a.appearance.mid.distance(b.appearance.mid),
                    a.appearance.high.distance(b.appearance.high),
                    (a.appearance.chroma_p50 - b.appearance.chroma_p50).abs() * 128.0,
                    (a.appearance.tone.p50 - b.appearance.tone.p50).abs()
                ));
            }
        }
        eprintln!(
            "amount {amount:.2}: clip {:.4} -> {:.4} (limit +0.01) skin ok {} distance {:.2} -> {:.2}\n    {}",
            before_clip,
            clip,
            super::super::skin_guard(&before_regions, &after, session.reference_monochrome),
            before_distance.total,
            distance.total,
            skin.join("\n    ")
        );
    }
    // How much closer a fit gets when it treats the reference as a look to follow.
    let fitting = fitting_frame(&state, &photo).expect("fitting frame");
    let plain = rendered(&state, &fitting, &session.baseline).expect("plain");
    let target_samples =
        super::super::transfer::samples(&plain, super::super::transfer::TARGET_SAMPLES);
    for similarity in [None, Some(0.66_f32), Some(0.78), Some(0.9)] {
        let mut fit = super::super::transfer::fit_with_checkpoint(
            &transfer.reference,
            &target_samples,
            similarity,
            || Ok(()),
        )
        .expect("fit");
        let skin_review = super::super::transfer::review_skin(
            &mut fit.grade,
            &plain,
            &session.target_regions,
            false,
        );
        let mut candidate = session.baseline.clone();
        aura_recipe::reference_grade::write(&mut candidate, &fit.grade).expect("write");
        let image = rendered(&state, &frame, &candidate).expect("render");
        let after = appearance::readings(&image, &session.target_regions);
        let skin: Vec<String> = before_regions
            .iter()
            .filter(|r| r.name == "face skin")
            .filter_map(|a| {
                after.iter().find(|b| b.name == a.name).map(|b| {
                    format!(
                        "mid {:.1} hi {:.1} chroma {:.1} tone {:.3}",
                        a.appearance.mid.distance(b.appearance.mid),
                        a.appearance.high.distance(b.appearance.high),
                        (a.appearance.chroma_p50 - b.appearance.chroma_p50).abs() * 128.0,
                        (a.appearance.tone.p50 - b.appearance.tone.p50).abs()
                    )
                })
            })
            .collect();
        eprintln!(
            "faithful {similarity:?}: predicted {:.2} -> {:.2}, rendered {:.2}, clip {:.4} -> {:.4}, skin review {:?}, face {}",
            fit.before.total,
            fit.after.total,
            super::super::matcher::measured(&transfer.reference, &image).total,
            before_clip,
            appearance::clipped(&image),
            skin_review,
            skin.join(" ")
        );
        image::RgbImage::from_raw(image.width, image.height, image.data.clone())
            .map(|i| image::imageops::thumbnail(&i, 900, 900))
            .map(|i| {
                i.save(format!(
                    "F:/aura-evoto-colormatch/faithful-{}.jpg",
                    similarity.map_or(0, |s| (s * 100.0) as i32)
                ))
            });
    }
    for (label, amount) in [("Standard", 1.0_f32), ("Extreme", 1.6)] {
        let clock = std::time::Instant::now(); // ALLOW-BANNED: test timing
        let result = quick_match(
            &state,
            &QuickMatchInput {
                photo_id: photo_id.clone(),
                reference_id: summary.id.clone(),
                controls: Controls {
                    amount,
                    ..Controls::default()
                },
                cancel_id: uuid::Uuid::new_v4().to_string(),
                step_down: true,
                reanalyse: false,
            },
        )
        .expect("quick match");
        eprintln!(
            "one click {label}: {:.1}s, saved {:.0}% after {} full checks, distance {:.2} -> {:.2}",
            clock.elapsed().as_secs_f32(),
            result.report.controls.amount * 100.0,
            result.attempts,
            result.report.before_distance,
            result.report.after_distance
        );
    }
}

mod sweep;
