//! Paired reference check through the native commands, run by hand.
//!
//! `AURA_PAIR_DIR` names a folder of `NAME-before.png` photographs (or `.jpg`, `.tif`, or
//! `.dng`, a camera file, which is how a real exposure mistake is tested). Every one is imported into
//! an isolated catalogue and edited exactly as the studio's buttons would edit it, once per
//! workflow in `AURA_PAIR_MODES` (default `auto`):
//!
//! - `neutral` is no edit at all, which is how a render is compared with its own source.
//! - `auto` is Auto enhance: measured light and colour plus the remembered finishing.
//! - `light` is the light and colour half of Auto enhance alone, through Auto edit with no
//!   provider configured: quick enough to run over a matrix of conditions.
//! - `retouch` is Auto retouch with the options in `AURA_PAIR_OPTIONS` (a JSON file in the
//!   wire's own shape, as the Retouch panel sends it) and no global change.
//! - `both` is the same options with light and colour measured as well.
//! - `match` is Auto retouch followed by Choose finished reference against `NAME-after.png`.
//!   With `AURA_PAIR_KEEP=NAME` the look is kept as a personal look of that name and applied
//!   again as any other look would be, rendered as `kept`.
//! - `look_ID` is the edit profile `ID` (hyphens written as underscores) at full strength.
//! - `edits` keeps the selections Auto retouch made and replaces its operations with the
//!   stack in `AURA_PAIR_EDITS` (JSON), where `"matte": "auto"` names the first selection.
//!
//! `AURA_PAIR_REVERT` names `;`-separated sets of `+`-separated global controls (`whites`,
//! `temperature`, `sharpen.amount`): after the edit, each set is put back to neutral in turn
//! and the photograph rendered again as `MODEnoINDEX`, which is how the part each control
//! plays in a result is measured.
//!
//! The full-resolution render is written beside the input as `NAME.MODE_WxH.rgb` (packed sRGB)
//! with `NAME.MODE.recipe.json` and the saved coverage as `NAME.MODE.selection_WxH.raw`, for `scripts/compare-reference-pairs.py` to score against
//! `NAME-after.png` and for a person to look at. The assertions are that the source file is
//! unchanged and that the render keeps the photograph's dimensions; nothing here is a claim
//! about how close the result is to anybody's finished version.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::too_many_lines
)]
use aura_app::contract::ipc::{CreateProjectInput, DevelopImageInput, ListImagesInput};
use aura_app::edit_profiles::{self, ApplyProfileInput};
use aura_app::native_retouch::{self, CoverageInput, RetouchInput};
use aura_app::paired_reference::{self, PairedReferenceInput};
use aura_app::portrait_features::Options;
use aura_app::smart_edit::{self, AutoRetouchInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_recipe::retouch_tools::Edit;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for v in text.bytes().filter_map(value) {
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// Render the saved recipe at full resolution and write it, its coverage and the recipe.
fn write_render(
    state: &AppState,
    project: &str,
    photo: &str,
    folder: &Path,
    name: &str,
    mode: &str,
) {
    let render = native_retouch::preview(state, project, photo, false).expect("render");
    let before = native_retouch::preview(state, project, photo, true).expect("original");
    assert_eq!((render.width, render.height), (before.width, before.height));
    let rgb = base64_decode(&render.rgb_base64);
    assert_eq!(
        rgb.len(),
        render.width as usize * render.height as usize * 3
    );
    std::fs::write(
        folder.join(format!(
            "{name}.{mode}_{}x{}.rgb",
            render.width, render.height
        )),
        &rgb,
    )
    .unwrap();
    // Everything the saved operations select, for a person to check against the photograph.
    if let Ok(coverage) = native_retouch::saved_selection(
        state,
        &CoverageInput {
            project_id: project.to_owned(),
            photo_id: photo.to_owned(),
            operation_id: None,
        },
    ) {
        std::fs::write(
            folder.join(format!(
                "{name}.{mode}.selection_{}x{}.raw",
                coverage.width, coverage.height
            )),
            base64_decode(&coverage.rgb_base64),
        )
        .unwrap();
    }
    let develop = DevelopImageInput {
        photo_id: photo.to_owned(),
    };
    let recipe = aura_app::image_recipe(state, &develop).expect("saved recipe");
    std::fs::write(
        folder.join(format!("{name}.{mode}.recipe.json")),
        &recipe.body,
    )
    .unwrap();
}

fn edit_one(path: &Path, mode: &str, options: Option<Options>) {
    let original = std::fs::read(path).expect("read source photograph");
    let dir = tempfile::tempdir().expect("isolated catalog directory");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("open isolated catalog")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Paired reference check".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("create project");
    let imported = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("project id"),
            roots: vec![path.to_path_buf()],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import photograph");
    assert_eq!(imported.files_imported, 1);
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("list imported photograph");
    let photo = photos.first().expect("one photo").id.clone();
    let started = std::time::Instant::now();
    let develop = DevelopImageInput {
        photo_id: photo.clone(),
    };
    // The personal look a `match` kept, when `AURA_PAIR_KEEP` names one.
    let mut kept: Option<String> = None;
    match (mode, options) {
        // No edit at all: the render has to be the file.
        ("neutral", _) => {}
        // Auto retouch, then the studio's own Choose finished reference against the
        // finished half of the pair: the nine global controls fitted to it, nothing copied.
        ("match", Some(options)) => {
            smart_edit::auto_retouch(
                &state,
                &AutoRetouchInput {
                    project_id: project.id.clone(),
                    photo_id: photo.clone(),
                    global: false,
                    options,
                },
            )
            .expect("auto retouch");
            let name = path.file_name().unwrap().to_string_lossy();
            let reference = path.with_file_name(name.replace("-before.png", "-after.png"));
            let report = paired_reference::match_paired_reference(
                &state,
                &PairedReferenceInput {
                    photo_id: photo.clone(),
                    reference_path: reference.to_string_lossy().into_owned(),
                    cancel_id: "00000000-0000-4000-8000-000000000001".into(),
                    keep_as: std::env::var("AURA_PAIR_KEEP").ok(),
                },
            )
            .expect("match the finished reference");
            println!(
                "  reference match: {} controls changed after {} probes, error {:.3} -> {:.3}",
                report.changed, report.probes, report.before_error, report.after_error
            );
            kept = report.kept_look;
        }
        ("auto", _) => {
            smart_edit::run(&state, &develop, true).expect("auto enhance");
        }
        // The configured AI providers (`AURA_AI_CONFIG_FILE`) grade the photograph; a call
        // that fell back while a provider rests is tried again after the rest, three times.
        ("ai", _) => {
            aura_app::internal_ai::initialize(&state, false).expect("AI providers");
            for attempt in 0..4 {
                let edited = aura_app::photo_auto_edit(
                    &state,
                    &aura_app::contract::ipc::PhotoAutoEditInput {
                        project_id: project.id.clone(),
                        photo_id: photo.clone(),
                        job_id: format!("pair-ai-{attempt}"),
                    },
                )
                .expect("AI grade");
                if edited.source != "local_fallback" || attempt == 3 {
                    println!("  ai: source {} model {}", edited.source, edited.model);
                    for reason in &edited.reasons {
                        println!("  ai: {reason}");
                    }
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(20));
            }
        }
        ("light", _) => {
            let edited = aura_app::photo_auto_edit(
                &state,
                &aura_app::contract::ipc::PhotoAutoEditInput {
                    project_id: project.id.clone(),
                    photo_id: photo.clone(),
                    job_id: "pair-light".into(),
                },
            )
            .expect("measured light and colour");
            for reason in edited.reasons.iter().skip(1) {
                println!("  light: {reason}");
            }
        }
        // One of the start screen's looks in one click, with no finished version to fit to.
        (look, _) if look.starts_with("look_") => {
            let report = edit_profiles::apply_edit_profile(
                &state,
                &ApplyProfileInput {
                    photo_id: photo.clone(),
                    profile_id: look.trim_start_matches("look_").replace('_', "-"),
                    strength: 1.0,
                },
            )
            .expect("apply the look");
            for note in &report.adaptations {
                println!("  {look}: {note}");
            }
        }
        ("retouch" | "both", Some(options)) => {
            smart_edit::auto_retouch(
                &state,
                &AutoRetouchInput {
                    project_id: project.id.clone(),
                    photo_id: photo.clone(),
                    global: mode == "both",
                    options,
                },
            )
            .expect("auto retouch");
        }
        // The automatic selections with a hand-written operation stack in place of the
        // planned one: how an operation is tried on a real photograph before it is planned.
        ("edits", Some(options)) => {
            smart_edit::auto_retouch(
                &state,
                &AutoRetouchInput {
                    project_id: project.id.clone(),
                    photo_id: photo.clone(),
                    global: false,
                    options,
                },
            )
            .expect("auto retouch");
            let file = std::env::var_os("AURA_PAIR_EDITS").expect("AURA_PAIR_EDITS");
            let text = std::fs::read_to_string(file).expect("read AURA_PAIR_EDITS");
            let mut input = RetouchInput {
                project_id: project.id.clone(),
                photo_id: photo.clone(),
                action: "list".into(),
                edits: Vec::new(),
                id: None,
            };
            let planned = native_retouch::edit(&state, &input).expect("planned stack");
            let matte = planned.iter().find_map(|e| e.matte.clone());
            input.action = "append".into();
            input.edits = serde_json::from_str::<Vec<Edit>>(&text).expect("edits on the wire");
            for edit in &mut input.edits {
                // `"matte": "auto"` stands for the first selection the planner made.
                if edit.matte.as_deref() == Some("auto") {
                    edit.matte.clone_from(&matte);
                }
            }
            native_retouch::edit(&state, &input).expect("append the stack under test");
            input.action = "remove".into();
            input.edits.clear();
            for edit in planned {
                input.id = Some(edit.id);
                native_retouch::edit(&state, &input).expect("remove a planned operation");
            }
        }
        _ => panic!("Mode {mode} needs AURA_PAIR_OPTIONS, or is not one of the workflows"),
    }
    let planned = started.elapsed();
    let stem = path.file_stem().unwrap().to_string_lossy();
    let name = stem.strip_suffix("-before").unwrap_or(&stem);
    let folder = path.parent().unwrap();
    write_render(&state, &project.id, &photo, folder, name, mode);
    println!(
        "{name} {mode}: planned in {:.1}s, rendered in {:.1}s",
        planned.as_secs_f32(),
        (started.elapsed() - planned).as_secs_f32()
    );
    // A kept look is applied the way it will be to the next photograph: the match's own
    // settings are replaced by this photograph's measured correction plus the look.
    if let Some(look) = kept {
        let report = edit_profiles::apply_edit_profile(
            &state,
            &ApplyProfileInput {
                photo_id: photo.clone(),
                profile_id: look.clone(),
                strength: 1.0,
            },
        )
        .expect("apply the kept look");
        println!(
            "  kept as {look}; applied again with {} changes",
            report.changed
        );
        for note in &report.adaptations {
            println!("  {look}: {note}");
        }
        write_render(&state, &project.id, &photo, folder, name, "kept");
    }
    // The same edit with one group of global controls back at neutral.
    if let Ok(sets) = std::env::var("AURA_PAIR_REVERT") {
        let saved = aura_app::image_recipe(&state, &develop).expect("saved recipe");
        let body: serde_json::Value = serde_json::from_str(&saved.body).expect("recipe body");
        let at = |root: &serde_json::Value, control: &str| {
            control
                .split('.')
                .fold(root.clone(), |value, key| value[key].clone())
        };
        for (index, set) in sets.split(';').enumerate() {
            for restore in [false, true] {
                for control in set.split('+') {
                    let value = if restore {
                        at(&body["global"], control)
                    } else if control == "temperature" {
                        // 5500 K is where this renderer adds no correction of its own.
                        serde_json::json!(5500)
                    } else {
                        serde_json::json!(0)
                    };
                    aura_app::set_param(
                        &state,
                        &aura_app::contract::ipc::SetParamInput {
                            project_id: project.id.clone(),
                            photo_id: photo.clone(),
                            path: format!("global.{control}"),
                            value,
                            label: None,
                        },
                    )
                    .expect("set a global control");
                }
                if !restore {
                    let mode = format!("{mode}no{index}");
                    write_render(&state, &project.id, &photo, folder, name, &mode);
                }
            }
        }
    }
    // Ablations: the same plan rendered again without the operations whose id contains one
    // of the `|`-separated fragments in each `;`-separated set of `AURA_PAIR_DROP`.
    if let Ok(sets) = std::env::var("AURA_PAIR_DROP") {
        let mut input = RetouchInput {
            project_id: project.id.clone(),
            photo_id: photo.clone(),
            action: "list".into(),
            edits: Vec::new(),
            id: None,
        };
        for (index, set) in sets.split(';').enumerate() {
            // Switched off and back on in place, so the order of the stack never changes.
            let mut switched = 0;
            for enabled in [false, true] {
                input.action = "list".into();
                input.edits.clear();
                let stack = native_retouch::edit(&state, &input).expect("saved stack");
                input.action = "update".into();
                for mut edit in stack {
                    let named = set.split('|').any(|fragment| edit.id.contains(fragment));
                    if named && edit.enabled != enabled {
                        edit.enabled = enabled;
                        input.edits = vec![edit];
                        native_retouch::edit(&state, &input).expect("switch an operation");
                        switched += usize::from(!enabled);
                    }
                }
                if !enabled {
                    println!("  without {set}: {switched} operations switched off");
                    let mode = format!("{mode}drop{index}");
                    write_render(&state, &project.id, &photo, folder, name, &mode);
                }
            }
        }
    }
    assert_eq!(std::fs::read(path).expect("source afterwards"), original);
}

#[test]
#[ignore = "needs AURA_PAIR_DIR; writes renders for a person to compare"]
fn edits_reference_pairs() {
    let dir = PathBuf::from(std::env::var_os("AURA_PAIR_DIR").expect("AURA_PAIR_DIR"));
    let modes = std::env::var("AURA_PAIR_MODES").unwrap_or_else(|_| "auto".into());
    let only = std::env::var("AURA_PAIR_ONLY").ok();
    let options = std::env::var_os("AURA_PAIR_OPTIONS").map(|file| {
        let text = std::fs::read_to_string(file).expect("read AURA_PAIR_OPTIONS");
        serde_json::from_str::<Options>(&text).expect("options in the wire's shape")
    });
    let mut inputs: Vec<_> = std::fs::read_dir(&dir)
        .expect("read AURA_PAIR_DIR")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                ["png", "jpg", "jpeg", "tif", "tiff", "dng"]
                    .iter()
                    .any(|ext| n.ends_with(&format!("-before.{ext}")))
            })
        })
        .filter(|p| {
            only.as_ref().is_none_or(|names| {
                names
                    .split(',')
                    .any(|n| p.file_name().unwrap().to_string_lossy().starts_with(n))
            })
        })
        .collect();
    assert!(!inputs.is_empty(), "No NAME-before.png in AURA_PAIR_DIR");
    inputs.sort();
    // One photograph that cannot be opened is a result about that photograph, not a reason to
    // leave the rest of the folder unmeasured: each is run on its own, and the run fails at the
    // end, naming every one that did.
    let mut failed = Vec::new();
    for path in &inputs {
        for mode in modes.split(',') {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                edit_one(path, mode, options);
            }));
            if outcome.is_err() {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                println!("FAILED {name} {mode}");
                failed.push(format!("{name} ({mode})"));
            }
        }
    }
    assert!(failed.is_empty(), "could not edit: {}", failed.join(", "));
}
