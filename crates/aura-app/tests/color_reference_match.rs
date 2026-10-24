//! A different reference frame, native renderer, saved strengths and stale-edit protection.
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::float_cmp)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::{color_match, AppState};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use std::sync::Arc;

fn setup(path: &std::path::Path, directory: &std::path::Path) -> (AppState, ProjectId, PhotoId) {
    setup_with_baseline(path, directory, true)
}

fn setup_with_baseline(
    path: &std::path::Path,
    directory: &std::path::Path,
    save_baseline: bool,
) -> (AppState, ProjectId, PhotoId) {
    let state = AppState::open(&directory.join("catalog.aura"))
        .expect("isolated catalogue")
        .with_cache_root(&directory.join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Reference colour verification".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let project_id = ProjectId::from_db(&project.id).expect("project id");
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id,
            roots: vec![path.to_path_buf()],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import target");
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("list target");
    let photo = PhotoId::from_db(&photos.first().expect("target").id).expect("photo id");
    if save_baseline {
        let baseline = aura_app::develop_commands::neutral_recipe(&state, photo);
        state
            .recipe_store()
            .save(&project_id, &photo, &baseline, &[], "Camera baseline")
            .expect("initial edit");
    }
    (state, project_id, photo)
}
fn job() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[test]
fn palette_matching_checks_the_native_endpoint_without_writing_until_apply() {
    let directory = tempfile::tempdir().expect("native palette catalogue");
    let target_path = directory.path().join("target.png");
    let reference_path = directory.path().join("different-reference.png");
    image::RgbImage::from_fn(144, 96, |x, y| {
        let value = (35 + x * 150 / 144 + y * 35 / 96) as u8;
        image::Rgb([value, value.saturating_add(9), value.saturating_add(15)])
    })
    .save(&target_path)
    .expect("target");
    image::RgbImage::from_fn(96, 144, |x, y| {
        let value = (45 + y * 150 / 144 + x * 25 / 96) as u8;
        image::Rgb([value.saturating_add(15), value.saturating_add(5), value])
    })
    .save(&reference_path)
    .expect("reference");
    let source_bytes = std::fs::read(&target_path).expect("source bytes");
    let (state, _, photo) = setup_with_baseline(&target_path, directory.path(), false);
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("reference import");
    let analysis = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            photo_id: photo.to_db(),
            reference_id: reference.id,
            cancel_id: job(),
            quality: color_match::MatchQuality::Quick,
            intent: color_match::MatchIntent::Palette,
        },
    )
    .expect("native palette analysis");
    let fitting = analysis.fitting.as_ref().expect("diagnostics");
    assert_eq!(fitting.intent, color_match::MatchIntent::Palette);
    assert!(fitting.native_candidate_checked);
    assert!(fitting.native_backtrack_factor > 0.0 && fitting.native_backtrack_factor <= 1.0);
    assert!(!analysis.full_resolution_checked);
    assert!(state
        .recipe_store()
        .load(&photo)
        .expect("no edit")
        .is_none());
    let apply = |controls| {
        color_match::apply(
            &state,
            &color_match::ApplyInput {
                photo_id: photo.to_db(),
                session_id: analysis.session_id.clone(),
                controls,
                cancel_id: job(),
            },
        )
    };
    let result = apply(color_match::Controls::default()).expect("full-strength apply");
    assert!(
        result
            .fitting
            .as_ref()
            .expect("native proof")
            .cached_proposals
            > 0,
        "identical full-resolution pixels should reuse the in-process native proof"
    );
    assert!(result.current && result.full_resolution_checked);
    assert!(result.after_distance <= result.before_distance + 0.0001);
    let matched = state.recipe_store().load(&photo).expect("saved");
    assert_eq!(
        apply(color_match::Controls::default())
            .expect("repeat")
            .changed,
        0
    );
    apply(color_match::Controls {
        amount: 0.0,
        ..Default::default()
    })
    .expect("zero");
    apply(color_match::Controls::default()).expect("restore fitted endpoint");
    assert_eq!(
        state.recipe_store().load(&photo).expect("restored"),
        matched
    );
    assert_eq!(
        std::fs::read(&target_path).expect("source unchanged"),
        source_bytes
    );
    let mono_path = directory.path().join("different-monochrome.png");
    image::RgbImage::from_fn(96, 144, |_, y| image::Rgb([(115 + y * 120 / 144) as u8; 3]))
        .save(&mono_path)
        .expect("monochrome reference");
    let mono = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: mono_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("monochrome import");
    let analysis = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            photo_id: photo.to_db(),
            reference_id: mono.id,
            cancel_id: job(),
            quality: color_match::MatchQuality::Quick,
            intent: color_match::MatchIntent::Palette,
        },
    )
    .expect("native monochrome palette fit");
    let monochrome = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: analysis.session_id,
            controls: Default::default(),
            cancel_id: job(),
        },
    )
    .expect("native monochrome apply");
    assert!(monochrome.full_resolution_checked);
    let saved = state
        .recipe_store()
        .load(&photo)
        .expect("saved monochrome")
        .expect("recipe");
    let model = aura_recipe::reference_grade::read(&saved)
        .expect("grade")
        .expect("monochrome model");
    assert!(model.chroma.iter().all(|v| v.abs() < 1e-6));
    assert!(
        model.skin_fidelity.abs() < 1e-6,
        "skin must desaturate with a monochrome reference"
    );
}

#[test]
fn identical_source_and_renamed_references_are_refused_without_editing_the_target() {
    let directory = tempfile::tempdir().expect("distinct reference catalogue");
    let target_path = directory.path().join("target.png");
    let copy_path = directory.path().join("renamed-copy.png");
    let different_path = directory.path().join("different-photo.png");
    image::RgbImage::from_pixel(80, 64, image::Rgb([80u8, 90, 100]))
        .save(&target_path)
        .expect("target");
    std::fs::copy(&target_path, &copy_path).expect("renamed copy");
    image::RgbImage::from_pixel(80, 64, image::Rgb([100u8, 90, 80]))
        .save(&different_path)
        .expect("different photo");
    let original = std::fs::read(&target_path).expect("source bytes");
    let (state, _, photo) = setup_with_baseline(&target_path, directory.path(), false);
    for path in [&target_path, &copy_path] {
        let reference = color_match::import_reference(
            &state,
            &color_match::ImportInput {
                path: path.to_string_lossy().into_owned(),
                cancel_id: job(),
            },
        )
        .expect("import duplicate reference");
        let error = color_match::prepare(
            &state,
            &color_match::PrepareInput {
                intent: Default::default(),
                photo_id: photo.to_db(),
                reference_id: reference.id,
                cancel_id: job(),
                quality: Default::default(),
            },
        )
        .expect_err("same source must be refused");
        assert!(error.message.contains("same source photo"));
        assert!(state
            .recipe_store()
            .load(&photo)
            .expect("no edit")
            .is_none());
        let writes: i64 = state.catalog().read(|conn| {
            conn.query_row("SELECT (SELECT COUNT(*) FROM edit_history)+(SELECT COUNT(*) FROM setting WHERE key LIKE 'colour_match_v1:%')", [], |row| row.get(0))
                .map_err(|e| aura_core::errors::db::statement_failed("duplicate refusal writes", &e))
        }).expect("no matching writes");
        assert_eq!(
            writes, 0,
            "duplicate refusal creates no recipe, history or session"
        );
    }
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: different_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("different reference");
    let accepted = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            intent: Default::default(),
            photo_id: photo.to_db(),
            reference_id: reference.id,
            cancel_id: job(),
            quality: Default::default(),
        },
    )
    .expect("different photo must remain usable");
    assert!(accepted.current);
    assert!(state
        .recipe_store()
        .load(&photo)
        .expect("analysis remains read-only")
        .is_none());
    assert_eq!(
        std::fs::read(&target_path).expect("source unchanged"),
        original
    );
}

#[test]
fn fractional_reanalysis_and_quality_changes_start_from_the_original_match_baseline() {
    for legacy in [false, true] {
        let directory = tempfile::tempdir().expect("fractional reference matching");
        let target_path = directory.path().join("original.png");
        let reference_path = directory.path().join("reference.png");
        image::RgbImage::from_fn(96, 80, |x, y| {
            let l = (45 + x * 100 / 96 + y * 35 / 80) as u8;
            image::Rgb([l, l.saturating_add(6), l.saturating_add(12)])
        })
        .save(&target_path)
        .expect("target");
        image::RgbImage::from_fn(80, 96, |x, y| {
            let l = (55 + x * 100 / 80 + y * 35 / 96) as u8;
            image::Rgb([l.saturating_add(7), l.saturating_add(4), l])
        })
        .save(&reference_path)
        .expect("reference");
        let actual_directory = directory.path().join("actual");
        let fresh_directory = directory.path().join("fresh");
        std::fs::create_dir_all(&actual_directory).expect("actual directory");
        std::fs::create_dir_all(&fresh_directory).expect("fresh directory");
        let (actual, actual_project, actual_photo) = setup(&target_path, &actual_directory);
        let (fresh, fresh_project, fresh_photo) = setup(&target_path, &fresh_directory);
        let import_reference = |state: &AppState| {
            let reference = color_match::import_reference(
                state,
                &color_match::ImportInput {
                    path: reference_path.to_string_lossy().into_owned(),
                    cancel_id: job(),
                },
            )
            .expect("reference");
            if legacy {
                let key = format!("colour_reference_grade_v1:{}", reference.id);
                state
                    .catalog()
                    .writer()
                    .with(move |conn| {
                        conn.execute("DELETE FROM setting WHERE key=?1", [key])
                            .map_err(|e| {
                                aura_core::errors::db::statement_failed(
                                    "legacy fractional reference",
                                    &e,
                                )
                            })?;
                        Ok(())
                    })
                    .expect("legacy descriptors");
            }
            reference.id
        };
        let actual_reference = import_reference(&actual);
        let fresh_reference = import_reference(&fresh);
        let apply_fraction = |state: &AppState,
                              project: ProjectId,
                              photo: PhotoId,
                              reference: &str,
                              quality| {
            let analysis = color_match::prepare(
                state,
                &color_match::PrepareInput {
                    intent: Default::default(),
                    photo_id: photo.to_db(),
                    reference_id: reference.into(),
                    cancel_id: job(),
                    quality,
                },
            )
            .expect("analyse fractional look");
            let applied = color_match::apply(
                state,
                &color_match::ApplyInput {
                    photo_id: photo.to_db(),
                    session_id: analysis.session_id,
                    cancel_id: job(),
                    controls: color_match::Controls {
                        amount: 0.75,
                        ..Default::default()
                    },
                },
            )
            .expect("apply fractional look");
            assert!(applied.full_resolution_checked && applied.current);
            let recipe = state
                .recipe_store()
                .load(&photo)
                .expect("fractional recipe")
                .expect("saved");
            let pixels =
                aura_app::native_retouch::preview(state, &project.to_db(), &photo.to_db(), false)
                    .expect("native fractional pixels")
                    .rgb_base64;
            (recipe, pixels)
        };
        let precise = apply_fraction(
            &actual,
            actual_project,
            actual_photo,
            &actual_reference,
            color_match::MatchQuality::Precise,
        );
        let repeated = apply_fraction(
            &actual,
            actual_project,
            actual_photo,
            &actual_reference,
            color_match::MatchQuality::Precise,
        );
        assert!(
            repeated.0 == precise.0,
            "reanalysis must not compound 75% into 93.75% (legacy={legacy})"
        );
        assert!(
            repeated.1 == precise.1,
            "reanalysis must preserve native pixels (legacy={legacy})"
        );
        let quick = apply_fraction(
            &actual,
            actual_project,
            actual_photo,
            &actual_reference,
            color_match::MatchQuality::Quick,
        );
        let fresh_quick = apply_fraction(
            &fresh,
            fresh_project,
            fresh_photo,
            &fresh_reference,
            color_match::MatchQuality::Quick,
        );
        assert!(
            quick.0 == fresh_quick.0,
            "Quick after matching must equal a fresh Quick recipe (legacy={legacy})"
        );
        assert!(
            quick.1 == fresh_quick.1,
            "Quick after matching must equal fresh Quick native pixels (legacy={legacy})"
        );
        let restored = apply_fraction(
            &actual,
            actual_project,
            actual_photo,
            &actual_reference,
            color_match::MatchQuality::Precise,
        );
        assert!(
            restored.0 == precise.0,
            "switching back to Precise must retain absolute strength (legacy={legacy})"
        );
        assert!(
            restored.1 == precise.1,
            "Precise mode must return to identical native pixels (legacy={legacy})"
        );
    }
}

#[test]
fn pristine_analysis_never_overwrites_a_later_first_manual_edit() {
    let directory = tempfile::tempdir().expect("pristine manual race");
    let target_path = directory.path().join("original.png");
    let reference_path = directory.path().join("reference.png");
    image::RgbImage::from_pixel(80, 64, image::Rgb([80u8, 90, 100]))
        .save(&target_path)
        .expect("target");
    image::RgbImage::from_pixel(80, 64, image::Rgb([100u8, 90, 80]))
        .save(&reference_path)
        .expect("reference");
    let (state, project, photo) = setup_with_baseline(&target_path, directory.path(), false);
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("reference import");
    let analysis = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            intent: Default::default(),
            photo_id: photo.to_db(),
            reference_id: reference.id,
            cancel_id: job(),
            quality: Default::default(),
        },
    )
    .expect("analyse before the first edit");
    assert!(state
        .recipe_store()
        .load(&photo)
        .expect("pristine")
        .is_none());
    let mut manual = aura_app::develop_commands::neutral_recipe(&state, photo);
    manual.global.exposure = 0.25;
    state
        .recipe_store()
        .save_first(
            &project,
            &photo,
            &manual,
            &["global.exposure".into()],
            "First manual correction",
        )
        .expect("manual edit wins");
    assert!(color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: analysis.session_id,
            cancel_id: job(),
            controls: Default::default(),
        }
    )
    .is_err());
    assert_eq!(
        state
            .recipe_store()
            .load(&photo)
            .expect("manual recipe")
            .expect("saved"),
        manual
    );
    let history: i64 = state
        .catalog()
        .read(|conn| {
            conn.query_row("SELECT COUNT(*) FROM edit_history", [], |row| row.get(0))
                .map_err(|e| aura_core::errors::db::statement_failed("manual first history", &e))
        })
        .expect("history");
    assert_eq!(history, 1, "refusal must not add a matching history entry");
}

#[test]
fn pristine_imports_match_without_analysis_edits_and_first_apply_supports_undo() {
    for legacy in [false, true] {
        let directory = tempfile::tempdir().expect("pristine catalogue");
        let target_path = directory.path().join("original.png");
        let reference_path = directory.path().join("reference.png");
        image::RgbImage::from_fn(160, 120, |x, y| {
            let l = (45 + x * 100 / 160 + y * 35 / 120) as u8;
            image::Rgb([l, l.saturating_add(6), l.saturating_add(12)])
        })
        .save(&target_path)
        .expect("target");
        image::RgbImage::from_fn(120, 160, |x, y| {
            let l = (55 + x * 100 / 120 + y * 35 / 160) as u8;
            image::Rgb([l.saturating_add(7), l.saturating_add(4), l])
        })
        .save(&reference_path)
        .expect("reference");
        let original_bytes = std::fs::read(&target_path).expect("source bytes");
        let (state, project, photo) = setup_with_baseline(&target_path, directory.path(), false);
        let baseline = aura_app::develop_commands::neutral_recipe(&state, photo);
        let reference = color_match::import_reference(
            &state,
            &color_match::ImportInput {
                path: reference_path.to_string_lossy().into_owned(),
                cancel_id: job(),
            },
        )
        .expect("import reference");
        if legacy {
            // Descriptor-only libraries and references predating distribution transfer
            // retain the legacy fitting path without a separately stored grade measurement.
            let measurement_key = format!("colour_reference_grade_v1:{}", reference.id);
            state
                .catalog()
                .writer()
                .with(move |conn| {
                    conn.execute("DELETE FROM setting WHERE key=?1", [measurement_key])
                        .map_err(|e| {
                            aura_core::errors::db::statement_failed("simulate legacy reference", &e)
                        })?;
                    Ok(())
                })
                .expect("legacy reference");
        }
        let history_count = || -> i64 {
            state
                .catalog()
                .read(|conn| {
                    conn.query_row("SELECT COUNT(*) FROM edit_history", [], |row| row.get(0))
                        .map_err(|e| {
                            aura_core::errors::db::statement_failed("pristine history", &e)
                        })
                })
                .expect("history count")
        };
        assert!(state
            .recipe_store()
            .load(&photo)
            .expect("unmodified import")
            .is_none());
        let analysis = color_match::prepare(
            &state,
            &color_match::PrepareInput {
                intent: Default::default(),
                photo_id: photo.to_db(),
                reference_id: reference.id,
                cancel_id: job(),
                quality: Default::default(),
            },
        )
        .expect("analyse pristine import");
        assert!(state
            .recipe_store()
            .load(&photo)
            .expect("analysis does not initialize edits")
            .is_none());
        assert_eq!(history_count(), 0, "analysis must not create an Undo step");
        assert_eq!(
            analysis.fitting.as_ref().expect("fitting").engine,
            if legacy {
                "bounded-pattern-search-v1"
            } else {
                "reference-grade-transfer-v3"
            }
        );
        let input = |amount| color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: analysis.session_id.clone(),
            cancel_id: job(),
            controls: color_match::Controls {
                amount,
                ..Default::default()
            },
        };
        let zero = color_match::apply(&state, &input(0.0)).expect("zero before first edit");
        assert_eq!(zero.changed, 0);
        assert!(state
            .recipe_store()
            .load(&photo)
            .expect("zero keeps pristine import")
            .is_none());
        assert_eq!(history_count(), 0);
        let matched = color_match::apply(&state, &input(1.0)).expect("first colour edit");
        assert!(matched.changed > 0 && matched.full_resolution_checked && matched.current);
        assert!(matched.after_distance < matched.before_distance);
        assert_eq!(
            history_count(),
            1,
            "first actual edit creates exactly one Undo step"
        );
        let first = state
            .recipe_store()
            .load(&photo)
            .expect("first edit")
            .expect("saved");
        aura_app::history_step(
            &state,
            &aura_app::contract::ipc::HistoryStepInput {
                project_id: project.to_db(),
                photo_id: photo.to_db(),
                action: "undo".into(),
            },
        )
        .expect("undo first edit");
        assert_eq!(
            state
                .recipe_store()
                .load(&photo)
                .expect("undone")
                .expect("neutral"),
            baseline
        );
        aura_app::history_step(
            &state,
            &aura_app::contract::ipc::HistoryStepInput {
                project_id: project.to_db(),
                photo_id: photo.to_db(),
                action: "redo".into(),
            },
        )
        .expect("redo first edit");
        assert_eq!(
            state
                .recipe_store()
                .load(&photo)
                .expect("redone")
                .expect("saved"),
            first
        );
        let mut manual = first;
        manual.global.exposure += 0.2;
        state
            .recipe_store()
            .save(
                &project,
                &photo,
                &manual,
                &["global.exposure".into()],
                "Later manual edit",
            )
            .expect("manual correction");
        assert!(color_match::apply(&state, &input(0.5)).is_err());
        assert_eq!(
            state
                .recipe_store()
                .load(&photo)
                .expect("manual preserved")
                .expect("saved"),
            manual
        );
        assert_eq!(
            std::fs::read(&target_path).expect("source preserved"),
            original_bytes
        );
    }
}

#[test]
fn framing_context_and_color_only_preview_preserve_saved_recipe_history_and_originals() {
    let directory = tempfile::tempdir().expect("isolated framing");
    let path = directory.path().join("framing-source.png");
    image::RgbImage::from_fn(128, 96, |x, y| {
        let l = (40 + x / 3 + y / 4) as u8;
        image::Rgb([l, l.saturating_add(12), l.saturating_add(22)])
    })
    .save(&path)
    .expect("fixture");
    let original = std::fs::read(&path).expect("original");
    let (state, project, photo) = setup(&path, directory.path());
    let mut recipe = state
        .recipe_store()
        .load(&photo)
        .expect("load")
        .expect("initial");
    recipe.global.exposure = 0.25;
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &recipe,
            &["global.exposure".into()],
            "Colour baseline",
        )
        .expect("colour");
    let baseline =
        aura_app::native_retouch::preview(&state, &project.to_db(), &photo.to_db(), false)
            .expect("preview")
            .rgb_base64;
    let edit = serde_json::from_str(
        r#"{"id":"manual-light","tool":"dodge","enabled":true,
        "region":[0.5,0.5,0.5,0.5],"source":null,"amount":0.65,"feather":0.1,"radius":0.01,
        "texture":1.0,"tone":0.5,"warmth":0.0,"tint":0.0}"#,
    )
    .expect("operation");
    aura_recipe::retouch_tools::write(&mut recipe, &[edit]).expect("write retouch");
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &recipe,
            &[aura_recipe::retouch_tools::KEY.into()],
            "Manual light",
        )
        .expect("manual");
    let input = aura_app::framing_workflows::Input {
        project_id: project.to_db(),
        photo_id: photo.to_db(),
        recipe_hash: aura_recipe::hash::recipe_hash(&recipe).expect("hash"),
    };
    let edited = aura_app::native_retouch::preview(&state, &project.to_db(), &photo.to_db(), false)
        .expect("edited")
        .rgb_base64;
    assert_ne!(edited, baseline);
    let before_history = aura_app::image_history(
        &state,
        &aura_app::contract::ipc::DevelopImageInput {
            photo_id: photo.to_db(),
        },
    )
    .expect("history")
    .entries
    .len();
    let stored_before = state
        .recipe_store()
        .load(&photo)
        .expect("read before")
        .expect("saved before");
    let output = aura_app::framing_workflows::color_only(&state, &input).expect("colour only");
    assert_eq!(
        output.rgb_base64, baseline,
        "temporary view omits the retouch but keeps colour exposure"
    );
    let measured = aura_app::framing_workflows::context(&state, &input).expect("framing evidence");
    assert_eq!((measured.width, measured.height), (128, 96));
    assert!(!measured.reasons.is_empty());
    assert_eq!(
        state
            .recipe_store()
            .load(&photo)
            .expect("read")
            .expect("saved"),
        stored_before
    );
    assert_eq!(
        aura_app::image_history(
            &state,
            &aura_app::contract::ipc::DevelopImageInput {
                photo_id: photo.to_db()
            }
        )
        .expect("history")
        .entries
        .len(),
        before_history
    );
    let stale = aura_app::framing_workflows::Input {
        recipe_hash: "stale".into(),
        ..input
    };
    assert!(aura_app::framing_workflows::context(&state, &stale).is_err());
    assert!(aura_app::framing_workflows::color_only(&state, &stale).is_err());
    assert_eq!(std::fs::read(&path).expect("read original"), original);
}

#[test]
fn regional_reference_masks_restore_history_reopen_and_preserve_manual_masks_and_originals() {
    use aura_recipe::local_masks::{self, Component, LocalMask, Mode, Source};
    use std::collections::BTreeMap;
    let directory = tempfile::tempdir().expect("isolated regional workflow");
    let path = directory.path().join("source.png");
    let reference_path = directory.path().join("reference.png");
    image::RgbImage::from_fn(160, 120, |x, y| {
        let l = (45 + x * 100 / 160 + y * 35 / 120) as u8;
        image::Rgb([l, l.saturating_add(6), l.saturating_add(12)])
    })
    .save(&path)
    .expect("source");
    image::RgbImage::from_fn(120, 160, |x, y| {
        let l = (55 + x * 100 / 120 + y * 35 / 160) as u8;
        image::Rgb([l.saturating_add(7), l.saturating_add(4), l])
    })
    .save(&reference_path)
    .expect("reference");
    let original = std::fs::read(&path).expect("original bytes");
    let (state, project, photo) = setup(&path, directory.path());
    let mut baseline = state
        .recipe_store()
        .load(&photo)
        .expect("read")
        .expect("baseline");
    baseline.geometry.crop = [0.1, 0.05, 0.95, 0.9];
    let manual = LocalMask {
        id: "manual-light".into(),
        name: "Existing local correction".into(),
        enabled: true,
        amount: 0.5,
        components: vec![Component {
            mode: Mode::Add,
            invert: false,
            source: Source::Linear {
                start: [0.0, 0.0],
                end: [0.0, 1.0],
            },
        }],
        params: aura_recipe::MaskParams {
            exposure: Some(0.03),
            ..aura_recipe::MaskParams::default()
        },
    };
    local_masks::write(
        &mut baseline,
        std::slice::from_ref(&manual),
        &BTreeMap::new(),
    )
    .expect("manual mask");
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &baseline,
            &[
                format!("extra.{}", local_masks::KEY),
                "geometry.crop".into(),
            ],
            "Manual crop and local light",
        )
        .expect("save manual work");
    let render = |state: &AppState| {
        aura_app::native_retouch::preview(state, &project.to_db(), &photo.to_db(), false)
            .expect("native preview")
            .rgb_base64
    };
    let baseline_pixels = render(&state);
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("reference");
    let prepared = color_match::prepare_regional(
        &state,
        &color_match::RegionalPrepareInput {
            photo_id: photo.to_db(),
            reference_id: reference.id.clone(),
            cancel_id: job(),
        },
    )
    .expect("native semantic fit");
    assert!(prepared
        .regions
        .iter()
        .any(|r| r.name == color_match::RegionalKind::Background));
    assert!(
        prepared
            .regions
            .iter()
            .any(|r| r.after_distance + 0.01 < r.before_distance),
        "exercise actual regional fitting"
    );
    let full_strengths: BTreeMap<_, _> = prepared
        .regions
        .iter()
        .map(|r| (r.name, color_match::RegionalControls::default()))
        .collect();
    let apply = |state: &AppState, hash: String, strengths| {
        color_match::apply_regional(
            state,
            &color_match::RegionalApplyInput {
                photo_id: photo.to_db(),
                session_id: prepared.session_id.clone(),
                recipe_hash: hash,
                strengths,
                cancel_id: job(),
            },
        )
    };
    let applied = apply(&state, prepared.recipe_hash.clone(), full_strengths.clone())
        .expect("full-resolution guarded apply");
    assert!(applied.full_resolution_checked && applied.changed > 0);
    let full_pixels = render(&state);
    assert_ne!(full_pixels, baseline_pixels, "real pixels must change");
    let full = state
        .recipe_store()
        .load(&photo)
        .expect("read")
        .expect("full");
    assert_eq!(
        local_masks::read(&full).expect("masks").first(),
        Some(&manual)
    );
    assert_eq!(full.geometry, baseline.geometry);
    assert_eq!(full.global, baseline.global);
    let zero_strengths: BTreeMap<_, _> = full_strengths
        .iter()
        .map(|(kind, controls)| {
            (
                *kind,
                color_match::RegionalControls {
                    amount: 0.0,
                    ..*controls
                },
            )
        })
        .collect();
    let zero = apply(&state, applied.recipe_hash.clone(), zero_strengths).expect("zero regions");
    assert_eq!(
        render(&state),
        baseline_pixels,
        "zero restores actual cropped original edit"
    );
    let mut step = aura_app::contract::ipc::HistoryStepInput {
        project_id: project.to_db(),
        photo_id: photo.to_db(),
        action: "undo".into(),
    };
    aura_app::history_step(&state, &step).expect("undo zero");
    assert_eq!(render(&state), full_pixels);
    step.action = "redo".into();
    aura_app::history_step(&state, &step).expect("redo zero");
    assert_eq!(render(&state), baseline_pixels);
    assert!(
        apply(&state, applied.recipe_hash, full_strengths.clone()).is_err(),
        "stale expected hash refused"
    );
    let restored = apply(&state, zero.recipe_hash, full_strengths.clone())
        .expect("restore exact fitted masks");
    assert_eq!(render(&state), full_pixels);
    color_match::manage_reference(
        &state,
        &color_match::ManageInput {
            reference_id: reference.id,
            action: "remove".into(),
            name: None,
            group: None,
            favorite: None,
            default_controls: None,
        },
    )
    .expect("remove library entry independently");
    drop(state);
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("reopen")
        .with_cache_root(&directory.path().join("cache"));
    let saved = color_match::regional_status(&state, &photo.to_db())
        .expect("status")
        .expect("session");
    assert!(saved.current && saved.full_resolution_checked);
    assert_eq!(saved.recipe_hash, restored.recipe_hash);
    let half: BTreeMap<_, _> = full_strengths
        .iter()
        .map(|(kind, c)| (*kind, color_match::RegionalControls { amount: 0.5, ..*c }))
        .collect();
    let half =
        apply(&state, saved.recipe_hash, half).expect("saved session works without library entry");
    let mut newer = state
        .recipe_store()
        .load(&photo)
        .expect("read")
        .expect("saved");
    newer.global.exposure += 0.05;
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &newer,
            &["global.exposure".into()],
            "Later manual edit",
        )
        .expect("manual edit");
    assert!(
        !color_match::regional_status(&state, &photo.to_db())
            .expect("status")
            .expect("session")
            .current
    );
    assert!(apply(&state, half.recipe_hash, full_strengths).is_err());
    assert_eq!(std::fs::read(&path).expect("read-only original"), original);
}

#[test]
fn edited_preview_references_keep_library_metadata_and_saved_strengths_after_reopen() {
    let directory = tempfile::tempdir().expect("isolated library");
    let path = directory.path().join("source.png");
    image::RgbImage::from_pixel(80, 64, image::Rgb([120u8, 95, 75]))
        .save(&path)
        .expect("source");
    let original_bytes = std::fs::read(&path).expect("original bytes");
    let (state, project, photo) = setup(&path, directory.path());
    let before = aura_app::image_recipe(
        &state,
        &aura_app::contract::ipc::DevelopImageInput {
            photo_id: photo.to_db(),
        },
    )
    .expect("saved edit");
    let reference = color_match::capture_reference(
        &state,
        &color_match::CaptureInput {
            project_id: project.to_db(),
            photo_id: photo.to_db(),
            name: "My edited portrait".into(),
            cancel_id: job(),
        },
    )
    .expect("create from actual edited preview");
    assert!(reference
        .preview
        .as_ref()
        .expect("comparison preview")
        .starts_with("data:image/jpeg;base64,"));
    assert!(reference
        .thumbnail
        .as_ref()
        .expect("preview")
        .starts_with("data:image/png;base64,"));
    let request = color_match::ManageInput {
        reference_id: reference.id.clone(),
        action: "update".into(),
        name: Some("Warm reception".into()),
        group: Some("Weddings".into()),
        favorite: Some(true),
        default_controls: Some(color_match::Controls {
            amount: 0.7,
            detail: color_match::FineControls {
                temperature: 0.5,
                ..color_match::FineControls::default()
            },
            ..color_match::Controls::default()
        }),
    };
    color_match::manage_reference(&state, &request)
        .expect("metadata update")
        .expect("reference");
    let after = aura_app::image_recipe(
        &state,
        &aura_app::contract::ipc::DevelopImageInput {
            photo_id: photo.to_db(),
        },
    )
    .expect("unchanged photo");
    assert_eq!(
        before.recipe_hash, after.recipe_hash,
        "library operations do not edit the photo"
    );
    drop(state);
    let reopened = AppState::open(&directory.path().join("catalog.aura"))
        .expect("reopen catalogue")
        .with_cache_root(&directory.path().join("cache"));
    let references = color_match::list_references(&reopened).expect("durable library");
    let saved = references.first().expect("saved reference");
    assert_eq!(
        saved.preview, reference.preview,
        "comparison pixels survive reopening"
    );
    assert_eq!(saved.name, "Warm reception");
    assert_eq!(saved.group, "Weddings");
    assert!(saved.favorite);
    assert_eq!(saved.default_controls.expect("saved strengths").amount, 0.7);
    assert_eq!(
        saved
            .default_controls
            .expect("fine strengths")
            .detail
            .temperature,
        0.5
    );
    assert!(color_match::manage_reference(
        &reopened,
        &color_match::ManageInput {
            name: Some("\n".into()),
            ..request.clone()
        }
    )
    .is_err());
    color_match::manage_reference(
        &reopened,
        &color_match::ManageInput {
            action: "remove".into(),
            name: None,
            group: None,
            favorite: None,
            default_controls: None,
            ..request
        },
    )
    .expect("remove descriptor only");
    assert!(color_match::list_references(&reopened)
        .expect("library")
        .is_empty());
    assert_eq!(
        std::fs::read(&path).expect("unchanged original"),
        original_bytes
    );
}

#[test]
fn finishing_strengths_restore_pixels_preserve_manual_work_and_support_history() {
    use aura_app::automatic_strength::{self, ApplyInput, Input, Strengths};
    use aura_recipe::retouch_tools;
    let directory = tempfile::tempdir().expect("isolated finishing");
    let path = directory.path().join("portrait-chart.png");
    image::RgbImage::from_fn(96, 80, |x, y| {
        let detail = ((x / 2 + y / 2) % 2) as u8;
        image::Rgb([150 + 40 * detail, 100 + 27 * detail, 75 + 20 * detail])
    })
    .save(&path)
    .expect("texture chart");
    let original_bytes = std::fs::read(&path).expect("original bytes");
    let (state, project, photo) = setup(&path, directory.path());
    let mut base = state
        .recipe_store()
        .load(&photo)
        .expect("recipe")
        .expect("saved");
    let edits: Vec<retouch_tools::Edit> = serde_json::from_str(r#"[
      {"id":"auto-portrait-v1-0-texture","tool":"frequency","enabled":true,"region":[0.5,0.5,1,1],"source":null,"amount":0.6,"feather":0,"radius":0.04,"texture":0,"tone":0.5,"warmth":0,"tint":0},
      {"id":"photographer-dodge","tool":"dodge","enabled":true,"region":[0.8,0.8,0.2,0.2],"source":null,"amount":0.2,"feather":0.1,"radius":0.03,"texture":1,"tone":0.5,"warmth":0,"tint":0}
    ]"#).expect("automatic and manual steps");
    retouch_tools::write(&mut base, &edits).expect("saved finishing");
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &base,
            &aura_recipe::history::changed_paths(
                &aura_app::develop_commands::neutral_recipe(&state, photo),
                &base,
            ),
            "Automatic finishing fixture",
        )
        .expect("baseline");
    let query = Input {
        project_id: project.to_db(),
        photo_id: photo.to_db(),
    };
    let render = || {
        aura_app::native_retouch::preview(&state, &project.to_db(), &photo.to_db(), false)
            .expect("native pixels")
            .rgb_base64
    };
    let full_pixels = render();
    let report = automatic_strength::read(&state, &query).expect("controls");
    let zero = automatic_strength::apply(
        &state,
        &ApplyInput {
            project_id: project.to_db(),
            photo_id: photo.to_db(),
            recipe_hash: report.recipe_hash.clone(),
            strengths: Strengths {
                overall: 0.0,
                ..Strengths::default()
            },
        },
    )
    .expect("zero automatic strength");
    let zero_pixels = render();
    assert!(
        zero_pixels != full_pixels,
        "the smoothing must actually affect pixels"
    );
    let saved = state
        .recipe_store()
        .load(&photo)
        .expect("read")
        .expect("saved");
    assert_eq!(
        retouch_tools::read(&saved).expect("steps").get(1),
        edits.get(1),
        "manual dodge is unchanged"
    );
    let mut history = aura_app::contract::ipc::HistoryStepInput {
        project_id: project.to_db(),
        photo_id: photo.to_db(),
        action: "undo".into(),
    };
    aura_app::history_step(&state, &history).expect("undo strength");
    assert!(
        render() == full_pixels,
        "Undo restores original finishing pixels"
    );
    history.action = "redo".into();
    aura_app::history_step(&state, &history).expect("redo strength");
    assert!(
        render() == zero_pixels,
        "Redo restores zero-strength pixels"
    );
    let restored = automatic_strength::apply(
        &state,
        &ApplyInput {
            project_id: project.to_db(),
            photo_id: photo.to_db(),
            recipe_hash: zero.recipe_hash,
            strengths: Strengths::default(),
        },
    )
    .expect("restore from zero");
    assert!(
        render() == full_pixels,
        "restored full strength is pixel-exact"
    );
    assert!(
        automatic_strength::apply(
            &state,
            &ApplyInput {
                project_id: project.to_db(),
                photo_id: photo.to_db(),
                recipe_hash: report.recipe_hash,
                strengths: Strengths::default(),
            }
        )
        .is_err(),
        "stale controls must fail"
    );
    assert_eq!(
        automatic_strength::read(&state, &query)
            .expect("unchanged after refusal")
            .recipe_hash,
        restored.recipe_hash
    );
    assert_eq!(
        std::fs::read(path).expect("read-only original"),
        original_bytes
    );
}

#[test]
fn different_dimensions_match_strengths_restore_and_new_manual_edits_are_kept() {
    let directory = tempfile::tempdir().expect("isolated directory");
    let target_path = directory.path().join("original.jpg");
    let reference_path = directory.path().join("edited-reference.tiff");
    image::RgbImage::from_fn(160, 120, |x, y| {
        let l = (45 + x * 100 / 160 + y * 35 / 120) as u8;
        image::Rgb([l, l.saturating_add(6), l.saturating_add(12)])
    })
    .save(&target_path)
    .expect("target fixture");
    image::RgbImage::from_fn(120, 160, |x, y| {
        let l = (55 + x * 100 / 120 + y * 35 / 160) as u8;
        image::Rgb([l.saturating_add(7), l.saturating_add(4), l])
    })
    .save(&reference_path)
    .expect("reference fixture");
    let (state, project, photo) = setup(&target_path, directory.path());
    let original = state
        .recipe_store()
        .load(&photo)
        .expect("baseline")
        .expect("saved baseline");
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("import TIFF reference");
    assert_eq!((reference.width, reference.height), (120, 160));
    assert_eq!(
        color_match::list_references(&state)
            .expect("saved references")
            .len(),
        1
    );
    let prepared = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            intent: Default::default(),
            quality: Default::default(),
            photo_id: photo.to_db(),
            reference_id: reference.id.clone(),
            cancel_id: job(),
        },
    )
    .expect("fit a different image");
    assert!(!prepared.full_resolution_checked);
    let fitting = prepared
        .fitting
        .as_ref()
        .expect("measured fitting diagnostics");
    assert_eq!(fitting.engine, "reference-grade-transfer-v3");
    assert_eq!(fitting.quality, color_match::MatchQuality::Precise);
    assert_eq!(fitting.max_iterations, 4);
    assert!((1..=fitting.max_iterations).contains(&fitting.refinement_candidates));
    assert!((1..=40_000).contains(&fitting.target_samples));
    assert_eq!(fitting.unique_renders, 2);
    assert_eq!(fitting.cached_proposals, 0);
    assert_eq!(fitting.grade_candidates, 0);
    assert!(!fitting.native_candidate_checked);
    assert!((0.0..=1.0).contains(&fitting.native_backtrack_factor));
    let transfer = fitting
        .transfer
        .as_ref()
        .expect("rendered transfer diagnostics");
    assert_eq!(transfer.rendered_before.total, prepared.before_distance);
    assert_eq!(transfer.rendered_after.total, prepared.after_distance);
    assert!(prepared.after_distance <= prepared.before_distance);
    assert!(
        prepared.after_distance < prepared.before_distance - 0.01,
        "fixture must exercise an actual colour fit"
    );
    let input = |controls| color_match::ApplyInput {
        photo_id: photo.to_db(),
        session_id: prepared.session_id.clone(),
        controls,
        cancel_id: job(),
    };
    let applied =
        color_match::apply(&state, &input(color_match::Controls::default())).expect("native apply");
    assert!(applied.full_resolution_checked && applied.changed > 0);
    assert!(
        applied
            .fitting
            .as_ref()
            .expect("verified fitting diagnostics")
            .native_candidate_checked
    );
    assert!(applied.after_distance <= applied.before_distance + 0.0001);
    let full = state
        .recipe_store()
        .load(&photo)
        .expect("read full strength")
        .expect("recipe");
    let history_count = || -> i64 {
        state
            .catalog()
            .read(|conn| {
                conn.query_row("SELECT COUNT(*) FROM edit_history", [], |row| row.get(0))
                    .map_err(|e| {
                        aura_core::errors::db::statement_failed("count repeated colour history", &e)
                    })
            })
            .expect("history count")
    };
    let full_history = history_count();
    let repeated = color_match::apply(&state, &input(color_match::Controls::default()))
        .expect("repeat identical strengths through native renderer");
    assert_eq!(
        repeated.changed, 0,
        "same strengths must not create a duplicate edit"
    );
    assert_eq!(
        history_count(),
        full_history,
        "identical native controls add no Undo step"
    );
    assert_eq!(
        state
            .recipe_store()
            .load(&photo)
            .expect("same recipe")
            .expect("saved"),
        full
    );
    let half_controls = color_match::Controls {
        amount: 0.5,
        ..color_match::Controls::default()
    };
    color_match::apply(&state, &input(half_controls)).expect("reduce strength");
    color_match::apply(&state, &input(color_match::Controls::default())).expect("restore strength");
    assert_eq!(
        state
            .recipe_store()
            .load(&photo)
            .expect("recipe")
            .expect("saved")
            .global,
        full.global
    );
    let zero = color_match::Controls {
        amount: 0.0,
        ..color_match::Controls::default()
    };
    color_match::apply(&state, &input(zero)).expect("remove treatment");
    let restored = state
        .recipe_store()
        .load(&photo)
        .expect("restored recipe")
        .expect("saved");
    assert_eq!(restored.global, original.global);
    assert_eq!(restored.retouch, original.retouch);
    assert_eq!(
        restored.extra, original.extra,
        "zero also removes the matched colour extension exactly"
    );
    let status = color_match::status(&state, &photo.to_db())
        .expect("status")
        .expect("saved strengths");
    assert_eq!(status.controls.amount, 0.0);
    assert!(status.full_resolution_checked);
    let mut manual = restored;
    manual.global.exposure = 0.15;
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &manual,
            &["global.exposure".into()],
            "Later manual edit",
        )
        .expect("manual change");
    assert!(
        !color_match::status(&state, &photo.to_db())
            .expect("stale status")
            .expect("session")
            .current
    );
    assert!(color_match::apply(&state, &input(color_match::Controls::default())).is_err());
    assert_eq!(
        state
            .recipe_store()
            .load(&photo)
            .expect("manual recipe")
            .expect("saved")
            .global
            .exposure,
        0.15
    );
}

#[test]
#[ignore = "requires AURA_COLOR_MATCH_RAW and AURA_COLOR_MATCH_REFERENCE; isolated native RAW verification"]
fn native_raw_reference_match_does_not_touch_originals() {
    let path =
        std::path::PathBuf::from(std::env::var_os("AURA_COLOR_MATCH_RAW").expect("RAW path"));
    let reference_path = std::path::PathBuf::from(
        std::env::var_os("AURA_COLOR_MATCH_REFERENCE").expect("reference path"),
    );
    let before = blake3::hash(&std::fs::read(&path).expect("source before"));
    let directory = tempfile::tempdir().expect("isolated test");
    let (state, _, photo) = setup(&path, directory.path());
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("edited reference import");
    let prepared = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            intent: Default::default(),
            quality: Default::default(),
            photo_id: photo.to_db(),
            reference_id: reference.id,
            cancel_id: job(),
        },
    )
    .expect("native RAW analysis");
    let report = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: prepared.session_id,
            controls: color_match::Controls {
                amount: 0.5,
                ..color_match::Controls::default()
            },
            cancel_id: job(),
        },
    )
    .expect("native RAW apply");
    assert!(report.full_resolution_checked);
    assert_eq!(
        before,
        blake3::hash(&std::fs::read(&path).expect("source after"))
    );
    println!(
        "{}",
        serde_json::to_string(&report).expect("verification report")
    );
}
