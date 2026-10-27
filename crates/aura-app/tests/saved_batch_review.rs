//! Opt-in full-resolution review of already saved recipes using AURA's native renderer.
//! Originals and the desktop catalog are never modified. The input catalog is copied.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout
)]
use aura_app::AppState;
use aura_core::PhotoId;
use aura_recipe::Recipe;
use aura_render::{
    OutputColour, OutputSpec, RenderLevel, RenderPurpose, RenderRequest, RenderService,
    RenderedData,
};
use std::{io::Write, path::PathBuf, sync::Arc};

#[test]
#[ignore = "requires AURA_SAVED_REVIEW_ROOT and recorded native recipes"]
fn render_saved_recipes_for_review() {
    let root = PathBuf::from(std::env::var_os("AURA_SAVED_REVIEW_ROOT").expect("review root"));
    let worker = std::env::var("AURA_REVIEW_WORKER")
        .unwrap_or_else(|_| "0".into())
        .parse::<usize>()
        .unwrap();
    let workers = std::env::var("AURA_REVIEW_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse::<usize>()
        .unwrap();
    assert!(workers > 0 && worker < workers);
    let temp = tempfile::tempdir().unwrap();
    let catalog = temp.path().join("review.sqlite");
    std::fs::copy(root.join("workflow/catalog-before-final.sqlite"), &catalog).unwrap();
    let state = AppState::open(&catalog)
        .unwrap()
        .with_cache_root(&temp.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let disable = std::env::var("AURA_REVIEW_DISABLE").unwrap_or_default();
    let out = if disable.is_empty() {
        root.join("workflow/native-review-renders")
    } else {
        root.join("workflow/native-review-variants")
            .join(disable.replace(';', "_"))
    };
    std::fs::create_dir_all(&out).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("workflow/baseline-selection.json")).unwrap(),
    )
    .unwrap();
    let names: Vec<_> = baseline["photos"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let only = std::env::var("AURA_REVIEW_NAMES").unwrap_or_default();
    for (index, name) in names
        .iter()
        .enumerate()
        .filter(|(i, _)| i % workers == worker)
    {
        if !only.is_empty() && !only.split(';').any(|n| n == name) {
            continue;
        }
        let audit: serde_json::Value =
            std::fs::read_to_string(root.join("workflow/retouch_audit.json"))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
        // Re-render the first frame as an independent pixel-parity probe against desktop evidence.
        let started = std::time::Instant::now();
        let row = loop {
            let saved: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(root.join("workflow/revision_state.json")).unwrap(),
            )
            .unwrap();
            if !saved["photos"][name].is_null() {
                break saved["photos"][name].clone();
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        };
        if disable.is_empty()
            && index != 0
            && audit[name]["status"] == "full_resolution_verified"
            && audit[name]["recipeHash"] == row["recipeHash"]
        {
            continue;
        }
        let photo = PhotoId::from_db(row["photoId"].as_str().unwrap()).unwrap();
        let original: Recipe = serde_json::from_str(
            &std::fs::read_to_string(root.join(format!("Recipes/{name}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            aura_recipe::recipe_hash(&original).unwrap(),
            row["recipeHash"].as_str().unwrap()
        );
        let marker = out.join(format!("{name}-native.json"));
        if marker.exists() {
            let old: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&marker).unwrap()).unwrap();
            if old["recipeHash"] == row["recipeHash"] {
                continue;
            }
        }
        let mut renders = serde_json::Map::new();
        for label in ["before", "after"] {
            if !disable.is_empty() && label == "before" {
                continue;
            }
            println!("NATIVE REVIEW W{worker} {name} {label}");
            let mut recipe = original.clone();
            // Exactly the same review framing as native_retouch::render_preview.
            recipe.geometry = aura_recipe::Geometry::default();
            recipe.global.effects = aura_recipe::Effects::default();
            if label == "before" {
                recipe.extra.remove(aura_recipe::retouch_tools::KEY);
            }
            if !disable.is_empty() {
                let mut edits = aura_recipe::retouch_tools::read(&recipe).unwrap();
                for edit in &mut edits {
                    let kind = serde_json::to_value(edit.tool).unwrap();
                    if disable
                        .split(';')
                        .any(|tool| Some(tool) == kind.as_str() || tool == edit.id)
                    {
                        edit.enabled = false;
                    }
                }
                let mattes = aura_recipe::retouch_tools::read_mattes(&recipe).unwrap();
                aura_recipe::retouch_tools::write_with_mattes(&mut recipe, &edits, &mattes)
                    .unwrap();
            }
            if label == "before" {
                if let Some(base) = std::env::var_os("AURA_REVIEW_BASELINE_ROOT")
                    .map(PathBuf::from)
                    .filter(|base| {
                        base.join(format!("workflow/native-review-renders/{name}-native.json"))
                            .exists()
                            && base
                                .join(format!("workflow/native-review-renders/{name}-before.ppm"))
                                .exists()
                    })
                {
                    let mut previous: Recipe = serde_json::from_str(
                        &std::fs::read_to_string(base.join(format!("Recipes/{name}.json")))
                            .unwrap(),
                    )
                    .unwrap();
                    previous.geometry = aura_recipe::Geometry::default();
                    previous.global.effects = aura_recipe::Effects::default();
                    let old = serde_json::to_value(&previous).unwrap();
                    let new = serde_json::to_value(&recipe).unwrap();
                    // Only ignored studio metadata/provenance may differ in a
                    // no-retouch render. A changed global, RAW transform or typed
                    // local adjustment requires a fresh baseline instead.
                    for key in [
                        "bw",
                        "engine",
                        "geometry",
                        "global",
                        "image",
                        "lens",
                        "masks",
                        "restoration",
                        "retouch",
                        "schema",
                    ] {
                        assert_eq!(
                            old[key], new[key],
                            "{name}: baseline render field {key} changed"
                        );
                    }
                    let metadata: serde_json::Value = serde_json::from_str(
                        &std::fs::read_to_string(
                            base.join(format!("workflow/native-review-renders/{name}-native.json")),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    std::fs::copy(
                        base.join(format!("workflow/native-review-renders/{name}-before.ppm")),
                        out.join(format!("{name}-before.ppm")),
                    )
                    .unwrap();
                    let mut evidence = metadata["renders"]["before"].clone();
                    evidence["reusedBaselineRoot"] = serde_json::json!(base.to_string_lossy());
                    evidence["renderFieldsVerifiedEqual"] = serde_json::json!(true);
                    renders.insert(label.into(), evidence);
                    continue;
                }
            }
            let image = state
                .render()
                .unwrap()
                .render(RenderRequest {
                    image_id: photo,
                    recipe,
                    level: RenderLevel::Full,
                    purpose: RenderPurpose::Export,
                    output: OutputSpec {
                        colour_space: OutputColour::Srgb,
                        bit_depth: 8,
                        icc: None,
                    },
                })
                .unwrap();
            let RenderedData::Eight(pixels) = image.data else {
                panic!("expected 8-bit review RGB")
            };
            assert_eq!(
                pixels.len(),
                image.width as usize * image.height as usize * 3
            );
            let path = out.join(format!("{name}-{label}.ppm"));
            let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
            write!(file, "P6\n{} {}\n255\n", image.width, image.height).unwrap();
            file.write_all(&pixels).unwrap();
            file.flush().unwrap();
            renders.insert(label.into(),serde_json::json!({"width":image.width,"height":image.height,
                "renderHash":image.render_hash,"rgbBlake3":blake3::hash(&pixels).to_hex().to_string()}));
        }
        let result = serde_json::json!({"photo":name,"recipeHash":row["recipeHash"],"native":true,
            "diagnosticDisabledTools":disable,
            "framing":"native_retouch full review: geometry/effects neutral, globals retained", "renders":renders,
            "seconds":started.elapsed().as_secs_f32()});
        let partial = marker.with_extension("tmp");
        std::fs::write(&partial, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        std::fs::rename(partial, marker).unwrap();
        println!(
            "NATIVE REVIEW READY W{worker} {name}: {:.1}s",
            started.elapsed().as_secs_f32()
        );
    }
}
