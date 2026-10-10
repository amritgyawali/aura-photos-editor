//! Optional native spot-planner inspection on local real photographs.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]
use super::*;

#[cfg(test)]
#[test]
#[ignore = "needs AURA_SPOT_INPUT JSON and matching .rgb; prints native proposals for review"]
fn inspect_real_photo_spot_proposals() {
    let path = std::path::PathBuf::from(std::env::var("AURA_SPOT_INPUT").unwrap());
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let face: PortraitFace = serde_json::from_value(data["face"].clone()).unwrap();
    let mut settings: Settings = serde_json::from_value(data["settings"].clone()).unwrap();
    settings.max_spots = 900;
    let (w, h) = (
        data["width"].as_u64().unwrap() as u32,
        data["height"].as_u64().unwrap() as u32,
    );
    let rgb = std::fs::read(path.with_extension("rgb")).unwrap();
    let px = Pixels::new(&rgb, w, h).unwrap();
    let residual = std::env::var_os("AURA_SPOT_RECIPE").map(|recipe_path| {
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(recipe_path).unwrap()).unwrap();
        let body: serde_json::Value =
            serde_json::from_str(saved["body"].as_str().unwrap()).unwrap();
        let edits: Vec<aura_recipe::retouch_tools::Edit> =
            serde_json::from_value(body["studio_retouch_v1"].clone()).unwrap();
        let mattes: std::collections::BTreeMap<String, aura_recipe::retouch_tools::Matte> =
            serde_json::from_value(body["studio_retouch_mattes_v1"].clone()).unwrap();
        let edit = edits.iter().find(|e| e.id.ends_with("-clear")).unwrap();
        after_frequency_heal(&px, edit, &mattes[edit.matte.as_ref().unwrap()]).unwrap()
    });
    let px = residual
        .as_deref()
        .map_or(px, |bytes| Pixels::new(bytes, w, h).unwrap());
    println!(
        "Measured opening pair: {:?}",
        Geometry::new(&face, &px).unwrap().nostrils
    );
    let captured: Option<Matte> = data
        .get("matte")
        .map(|m| serde_json::from_value(m.clone()).unwrap());
    let analysis = captured.is_none().then(|| {
        aura_vision::skin::analyse(
            &rgb,
            w,
            h,
            std::slice::from_ref(&face),
            aura_vision::skin::Options::default(),
        )
        .unwrap()
    });
    let matte = captured
        .as_ref()
        .unwrap_or_else(|| analysis.as_ref().unwrap().people[0].face.as_ref().unwrap());
    let mut out = plan(&face, 0, &px, "diagnostic-", &settings, Some(matte));
    if std::env::var_os("AURA_SPOT_REFINE").is_some() {
        refine(
            &face,
            0,
            &px,
            &px,
            "diagnostic-",
            &settings,
            Some(matte),
            &std::collections::BTreeMap::new(),
            &mut out,
        )
        .unwrap();
    }
    println!("{:?}", out.report.findings);
    let edits = out.blemishes;
    if let Ok(target) = std::env::var("AURA_SPOT_TARGET") {
        let point: Vec<f32> = target.split(',').map(|v| v.parse().unwrap()).collect();
        assert_eq!(point.len(), 2);
        assert!(
            edits.iter().any(|edit| {
                let [x, y, rx, ry] = edit.region;
                !edit.heal_samples.is_empty()
                    && ((point[0] - x) / rx).hypot((point[1] - y) / ry) < 0.75
            }),
            "The requested residual lesion lacks a saved healthy-context repair"
        );
    }
    let json = serde_json::to_vec_pretty(&edits).unwrap();
    if let Ok(target) = std::env::var("AURA_SPOT_TEXTURE_TARGET") {
        let point: Vec<f32> = target.split(',').map(|v| v.parse().unwrap()).collect();
        assert_eq!(point.len(), 2);
        assert!(
            edits.iter().any(|e| e.texture_sources.len() >= 3
                && ((point[0] - e.region[0]) / e.region[2])
                    .hypot((point[1] - e.region[1]) / e.region[3])
                    < 0.75),
            "The requested large repair still uses a mirrored single donor"
        );
    }
    std::fs::write(path.with_extension("proposals.json"), json).unwrap();
    assert!(!edits.is_empty());
}
