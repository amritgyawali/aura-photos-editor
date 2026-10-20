//! Opt-in diagnosis of a saved portrait. All renders use a disposable catalog.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout
)]
use aura_core::PhotoId;
use aura_recipe::retouch_tools::{self, Tool};
use aura_render::{
    OutputColour, OutputSpec, RenderLevel, RenderPurpose, RenderRequest, RenderService,
    RenderedData,
};
use std::{io::Write, path::PathBuf, sync::Arc};

#[test]
#[ignore = "requires local chin/neck review inputs"]
fn isolate_chin_neck_repairs() {
    let out = PathBuf::from(std::env::var_os("AURA_CHIN_OUT").unwrap());
    std::fs::create_dir_all(&out).unwrap();
    let recipe_path = PathBuf::from(std::env::var_os("AURA_CHIN_RECIPE").unwrap());
    let original: aura_recipe::Recipe =
        serde_json::from_slice(&std::fs::read(recipe_path).unwrap()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let catalog = temp.path().join("isolated.sqlite");
    std::fs::copy(std::env::var_os("AURA_CHIN_CATALOG").unwrap(), &catalog).unwrap();
    let state = aura_app::AppState::open(&catalog)
        .unwrap()
        .with_cache_root(&temp.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let photo = PhotoId::from_db(&std::env::var("AURA_CHIN_PHOTO_ID").unwrap()).unwrap();
    let full = std::env::var("AURA_CHIN_FULL").as_deref() == Ok("1");
    let variants=std::env::var("AURA_CHIN_VARIANTS").unwrap_or_else(|_|"all;without_jaw_repairs;without_all_patch;without_frequency_heal;without_finish;without_body;before".into());
    let ops = retouch_tools::read(&original).unwrap();
    let mattes = retouch_tools::read_mattes(&original).unwrap();
    let mut results = Vec::new();
    for label in variants.split(';') {
        let mut recipe = original.clone();
        recipe.geometry = aura_recipe::Geometry::default();
        recipe.global.effects = aura_recipe::Effects::default();
        let mut edits = ops.clone();
        for e in &mut edits {
            let disable = match label {
                "all" => false,
                "without_jaw_repairs" => {
                    matches!(e.tool, Tool::PatchHeal | Tool::Heal) && e.region[1] > 0.45
                }
                "without_all_patch" => matches!(e.tool, Tool::PatchHeal | Tool::Heal),
                "without_frequency_heal" => e.tool == Tool::FrequencyHeal,
                "without_undereye" => e.tool == Tool::UnderEye,
                "only_undereye" => e.tool != Tool::UnderEye,
                "without_skin_uniformity" => {
                    matches!(e.tool, Tool::SkinUniformity | Tool::SkinColor)
                }
                "without_finish" => matches!(
                    e.tool,
                    Tool::SkinSmooth
                        | Tool::SkinUniformity
                        | Tool::PortraitDodgeBurn
                        | Tool::Frequency
                        | Tool::MicroDodgeBurn
                        | Tool::Dodge
                ),
                "without_body" => e.id.contains("-body-") || e.id.contains("-lines-neck"),
                "before" => true,
                id => e.id == id,
            };
            if disable {
                e.enabled = false;
            }
        }
        retouch_tools::write_with_mattes(&mut recipe, &edits, &mattes).unwrap();
        println!("CHIN RENDER {label}");
        let frame = state
            .render()
            .unwrap()
            .render(RenderRequest {
                image_id: photo,
                recipe,
                level: if full {
                    RenderLevel::Full
                } else {
                    RenderLevel::Proxy2048
                },
                purpose: RenderPurpose::Export,
                output: OutputSpec {
                    colour_space: OutputColour::Srgb,
                    bit_depth: 8,
                    icc: None,
                },
            })
            .unwrap();
        let RenderedData::Eight(rgb) = frame.data else {
            panic!("expected RGB8")
        };
        let mut file = std::io::BufWriter::new(
            std::fs::File::create(out.join(format!("{label}.ppm"))).unwrap(),
        );
        write!(file, "P6\n{} {}\n255\n", frame.width, frame.height).unwrap();
        file.write_all(&rgb).unwrap();
        file.flush().unwrap();
        results.push(serde_json::json!({"label":label,"dimensions":[frame.width,frame.height],"rgbBlake3":blake3::hash(&rgb).to_hex().to_string()}));
        std::fs::write(
            out.join("renders.json"),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
    }
}
