//! Real-photograph check of the scene decisions, run by hand. ADR-0086.
//!
//! `AURA_SCENE_PHOTOS` names a folder of `NAME_WxH.rgb` thumbnails (packed sRGB, up to 512 px).
//! For each one this prints what the automatic edit decides before any face is considered: the
//! histogram's tone correction, what the scene's intent changed about it, and every decision
//! with its reason. The assertions are bounds, not taste.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use aura_app::{photo_enhance, portrait_features::Pixels, smart_edit};

#[test]
#[ignore = "needs AURA_SCENE_PHOTOS; prints what each photograph was given"]
fn scenes_are_read_before_they_are_corrected() {
    let dir = std::env::var("AURA_SCENE_PHOTOS").expect("AURA_SCENE_PHOTOS");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rgb"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no NAME_WxH.rgb inputs");
    let mut kinds = std::collections::BTreeSet::new();
    for path in entries {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let (name, dims) = stem.rsplit_once('_').unwrap();
        let (w, h) = dims.split_once('x').unwrap();
        let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
        let rgb = std::fs::read(&path).unwrap();
        let px = Pixels::new(&rgb, w, h).unwrap();
        let histogram = photo_enhance::correction(&rgb).unwrap();
        let mut tone = histogram;
        let note = smart_edit::respect_intent(&mut tone, &px, &[]);
        let plan = smart_edit::analyse(&px, None, &[], tone.0);
        println!(
            "\n== {name}: {} · exposure {:+.2} EV (histogram {:+.2}) highlights {} shadows {} contrast {}",
            plan.kind.label(), tone.0, histogram.0, tone.1, tone.2, tone.3
        );
        println!("   ({})", smart_edit::readings(&px, &[]));
        if let Some(note) = note {
            println!("   * {note}");
        }
        for decision in &plan.decisions {
            println!("   - {decision}");
        }
        assert!(tone.0.abs() <= 1.5 && plan.vibrance <= 20 && plan.blacks <= 0 && plan.whites >= 0);
        kinds.insert(plan.kind.label());
    }
    println!("\nkinds seen: {kinds:?}");
}
