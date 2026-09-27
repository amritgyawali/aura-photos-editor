//! An actual JPEG import, local correction and render round trip.
use aura_app::contract::ipc::*;
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::codec::{decode_jpeg, encode_jpeg, Rgb8};
use std::sync::Arc;

#[test]
fn imported_photograph_renders_real_pixels_and_keeps_original() {
    photo_roundtrip(false);
}

#[test]
fn imported_png_renders_and_exports_without_changing_the_original() {
    photo_roundtrip(true);
}

fn photo_roundtrip(is_png: bool) {
    let dir = tempfile::tempdir().expect("temp");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Real photo workflow".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let path = dir.path().join(if is_png {
        "original.png"
    } else {
        "original.jpg"
    });
    let rgb = Rgb8 {
        width: 96,
        height: 64,
        data: (0..96 * 64)
            .flat_map(|i| {
                [
                    35 + (i % 55) as u8,
                    40 + (i % 60) as u8,
                    45 + (i % 50) as u8,
                ]
            })
            .collect(),
    };
    let original = if is_png {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, rgb.width, rgb.height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .expect("valid photo fixture and successful operation")
                .write_image_data(&rgb.data)
                .expect("valid photo fixture and successful operation");
        }
        bytes
    } else {
        encode_jpeg(&rgb, 95).expect("JPEG")
    };
    std::fs::write(&path, &original).expect("write fixture");
    let report = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![path.clone()],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    assert_eq!(report.files_imported, 1);
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 100,
            order_by: None,
        },
    )
    .expect("photos");
    let photo = &photos[0].id;
    let request = RenderImageInput {
        photo_id: photo.clone(),
        level: Some("full".into()),
        screen: None,
        colour_space: None,
        purpose: Some("export".into()),
    };
    let before = aura_app::render_image(&state, &request).expect("original render");
    assert_eq!((before.width, before.height), (96, 64));
    assert!(!before.rgb_base64.is_empty());
    let enhanced = aura_app::photo_enhance::enhance_photo(
        &state,
        &DevelopImageInput {
            photo_id: photo.clone(),
        },
    )
    .expect("valid photo fixture and successful operation");
    let repeated = aura_app::photo_enhance::enhance_photo(
        &state,
        &DevelopImageInput {
            photo_id: photo.clone(),
        },
    )
    .expect("valid photo fixture and successful operation");
    assert_eq!(enhanced.recipe_hash, repeated.recipe_hash);
    aura_app::set_param(
        &state,
        &SetParamInput {
            project_id: project.id.clone(),
            photo_id: photo.clone(),
            path: "global.exposure".into(),
            value: serde_json::Value::from(1.0),
            label: Some("Brighter".into()),
        },
    )
    .expect("valid photo fixture and successful operation");
    let protected = aura_app::photo_enhance::enhance_photo(
        &state,
        &DevelopImageInput {
            photo_id: photo.clone(),
        },
    )
    .expect("valid photo fixture and successful operation");
    let exposure = protected
        .params
        .iter()
        .find(|p| p.path == "global.exposure")
        .expect("valid photo fixture and successful operation");
    assert_eq!(exposure.value, serde_json::Value::from(1.0));
    assert!(exposure.protected);
    let reference_folder = dir.path().join("reference-photos");
    std::fs::create_dir(&reference_folder).expect("valid photo fixture and successful operation");
    for number in 0..8_u8 {
        let reference_rgb = Rgb8 {
            width: 96,
            height: 64,
            data: (0..96 * 64)
                .flat_map(|i| {
                    [
                        110 + number * 3 + (i % 55) as u8,
                        85 + number + (i % 60) as u8,
                        55 + number + (i % 50) as u8,
                    ]
                })
                .collect(),
        };
        std::fs::write(
            reference_folder.join(format!("reference-{number}.jpg")),
            encode_jpeg(&reference_rgb, 95).expect("valid photo fixture and successful operation"),
        )
        .expect("valid photo fixture and successful operation");
    }
    let style = aura_app::reference_style::analyse_reference(
        &state,
        &aura_app::reference_style::AnalyseReferenceInput {
            address: "https://www.instagram.com/example_photographer/".into(),
            folder: reference_folder.to_string_lossy().into(),
            cancel_id: "analyse-test".into(),
        },
    )
    .expect("valid photo fixture and successful operation");
    assert_eq!(style.measured, 8);
    assert!(!style.colors.is_empty());
    let apply = aura_app::reference_style::ApplyReferenceInput {
        photo_id: photo.clone(),
        reference_id: style.id.clone(),
        strength: 0.8,
        profile_id: None,
        profile_strength: None,
    };
    let report = aura_app::reference_style::apply_reference(&state, &apply)
        .expect("valid photo fixture and successful operation");
    assert!(report.after_distance <= report.before_distance);
    assert!(
        report.changed > 0,
        "a visibly different reference must produce an edit"
    );
    assert!(report.protected_fields > 0);
    let recipe_input = DevelopImageInput {
        photo_id: photo.clone(),
    };
    let first = aura_app::image_recipe(&state, &recipe_input)
        .expect("valid photo fixture and successful operation");
    aura_app::reference_style::apply_reference(&state, &apply)
        .expect("valid photo fixture and successful operation");
    let second = aura_app::image_recipe(&state, &recipe_input)
        .expect("valid photo fixture and successful operation");
    assert_eq!(
        first.recipe_hash, second.recipe_hash,
        "reference application must not compound"
    );
    assert_eq!(
        second
            .params
            .iter()
            .find(|p| p.path == "global.exposure")
            .expect("valid photo fixture and successful operation")
            .value,
        serde_json::Value::from(1.0)
    );
    // An edit profile on the same imported photo: saved, never compounding, the manual exposure
    // untouched, and gentler because a JPEG or PNG is already developed.
    let profile = aura_app::edit_profiles::ApplyProfileInput {
        photo_id: photo.clone(),
        profile_id: "fivek-expert-c".into(),
        strength: 1.0,
    };
    let applied = aura_app::edit_profiles::apply_edit_profile(&state, &profile)
        .expect("profile applies to an imported photo");
    assert!(
        applied.changed > 0,
        "a learned profile must change the edit"
    );
    assert!(applied
        .protected_fields
        .iter()
        .any(|f| f == "global.exposure"));
    assert!(
        applied
            .adaptations
            .iter()
            .any(|n| n.contains("learned from RAW")),
        "a developed photo gets the RAW-learned look at reduced strength: {:?}",
        applied.adaptations
    );
    let once = aura_app::image_recipe(&state, &recipe_input).expect("recipe");
    aura_app::edit_profiles::apply_edit_profile(&state, &profile).expect("profile again");
    let twice = aura_app::image_recipe(&state, &recipe_input).expect("recipe");
    assert_eq!(
        once.recipe_hash, twice.recipe_hash,
        "a profile must not compound"
    );
    let preview = aura_app::edit_profiles::preview_edit_profile(
        &state,
        &aura_app::edit_profiles::PreviewProfileInput {
            profile_id: "film-portra".into(),
            photo_id: Some(photo.clone()),
            strength: 1.0,
            size: Some(128),
        },
    )
    .expect("preview on the imported photo");
    assert!(preview.before.starts_with("data:image/jpeg;base64,"));
    assert_ne!(preview.before, preview.after);
    let sample = aura_app::edit_profiles::preview_edit_profile(
        &state,
        &aura_app::edit_profiles::PreviewProfileInput {
            profile_id: "bw-noir".into(),
            photo_id: None,
            strength: 1.0,
            size: Some(96),
        },
    )
    .expect("preview on the sample scene");
    assert_ne!(sample.before, sample.after);
    // The reference can be fitted on top of a profile, and still never compounds.
    let layered = aura_app::reference_style::ApplyReferenceInput {
        profile_id: Some("film-portra".into()),
        profile_strength: Some(0.8),
        ..apply
    };
    aura_app::reference_style::apply_reference(&state, &layered).expect("reference over profile");
    let layered_once = aura_app::image_recipe(&state, &recipe_input).expect("recipe");
    aura_app::reference_style::apply_reference(&state, &layered).expect("again");
    let layered_twice = aura_app::image_recipe(&state, &recipe_input).expect("recipe");
    assert_eq!(layered_once.recipe_hash, layered_twice.recipe_hash);

    let after = aura_app::render_image(&state, &request)
        .expect("valid photo fixture and successful operation");
    assert_ne!(before.rgb_base64, after.rgb_base64);
    let presets = aura_app::export_presets().expect("presets");
    let preset = presets
        .iter()
        .find(|p| p.format == "jpeg")
        .expect("JPEG preset");
    let mut set = serde_json::to_value(preset).expect("preset json");
    set["imageIds"] = serde_json::to_value([photo]).expect("photo identifiers");
    let destination = dir.path().join("export");
    let job = ExportJobInput {
        project_id: project.id,
        sets: vec![serde_json::from_value(set).expect("export set")],
        destination: destination.to_string_lossy().into_owned(),
        destination_kind: "folder".into(),
        copyright: None,
        contact: None,
        creator: None,
        keywords: vec![],
        strip_gps: true,
        strip_camera_serial: true,
        verify: true,
    };
    aura_app::export_run(&state, job).expect("first export");
    let exported = walk_jpegs(&destination);
    assert!(!exported.is_empty(), "first export must create a JPEG");
    let decoded = decode_jpeg(
        &std::fs::read(&exported[0]).expect("export bytes"),
        aura_raw::DecodeLimits::tier3(),
    )
    .expect("valid exported JPEG");
    assert!(
        decoded.data.windows(3).any(|w| w != [118, 118, 118]),
        "not grey placeholder"
    );
    assert_eq!(std::fs::read(&path).expect("original remains"), original);
}

fn walk_jpegs(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for entry in std::fs::read_dir(root).expect("export folder") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            out.extend(walk_jpegs(&path));
        } else if path.extension().is_some_and(|e| e == "jpg" || e == "jpeg") {
            out.push(path);
        }
    }
    out
}
