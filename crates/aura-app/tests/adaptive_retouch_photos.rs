//! Real-photograph check of the adaptive retouch, run by hand. ADR-0086.
//!
//! `AURA_ADAPTIVE_PHOTOS` names a folder with `proxy/NAME_WxH.rgb` (up to 2048 px) and
//! `thumb/NAME_WxH.rgb` (up to 512 px) packed sRGB files of the same photographs, which is the
//! pair of renditions the desktop app plans from. Every photograph is planned twice from the
//! same chosen settings - once exactly as set, once adapted - and the readings, the notes and
//! the changed controls are printed and written to `adaptive-report.json` for a person to read.
//!
//! The assertions are about the mechanism, not about taste: every adapted plan validates, a
//! photograph without the option is planned exactly as before, and a batch of different
//! photographs does not come out with one shared set of settings.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use aura_app::{portrait_auto, portrait_features};
use aura_recipe::retouch_tools;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn inputs(dir: &Path) -> BTreeMap<String, (PathBuf, u32, u32)> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rgb"))
        .map(|p| {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            let (name, dims) = stem.rsplit_once('_').unwrap();
            let (w, h) = dims.split_once('x').unwrap();
            (name.to_owned(), (p, w.parse().unwrap(), h.parse().unwrap()))
        })
        .collect()
}

#[test]
#[ignore = "needs AURA_ADAPTIVE_PHOTOS; prints what each photograph was given"]
fn every_photograph_gets_its_own_settings() {
    let dir = PathBuf::from(std::env::var("AURA_ADAPTIVE_PHOTOS").expect("AURA_ADAPTIVE_PHOTOS"));
    let proxies = inputs(&dir.join("proxy"));
    let thumbs = inputs(&dir.join("thumb"));
    assert!(!proxies.is_empty(), "no proxy/NAME_WxH.rgb inputs");
    let exact = portrait_features::Options {
        adaptive: false,
        ..portrait_features::Options::default()
    };
    let adaptive = portrait_features::Options::default();
    assert!(adaptive.adaptive);
    let mut signatures = BTreeSet::new();
    let mut with_faces = 0_usize;
    let mut report = Vec::new();
    for (name, (proxy_path, pw, ph)) in &proxies {
        let (thumb_path, tw, th) = thumbs.get(name).expect("a thumb for every proxy");
        let proxy = std::fs::read(proxy_path).unwrap();
        let thumb = std::fs::read(thumb_path).unwrap();
        let found = aura_vision::portrait::detect(&thumb, *tw, *th).unwrap();
        let faces =
            aura_vision::portrait::detect_small_faces(&proxy, *pw, *ph, &found).unwrap_or(found);
        let recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t");
        let plan_with = |options: &portrait_features::Options| {
            portrait_auto::plan_with_faces(
                &recipe,
                &thumb,
                *tw,
                *th,
                Some((&proxy, *pw, *ph)),
                0.0,
                Some(faces.clone()),
                options,
                true,
            )
            .unwrap()
        };
        let started = std::time::Instant::now();
        let adapted = plan_with(&adaptive);
        let took = started.elapsed();
        let plain = plan_with(&exact);
        assert!(
            plain.report.assessments.iter().all(|a| a.expert.is_none()),
            "{name}: a pass without the option must not adapt"
        );
        let stack = portrait_auto::staged(&[], &adapted.groups, portrait_auto::Group::Finishing);
        retouch_tools::validate(&stack).unwrap();
        let mut written = recipe.clone();
        retouch_tools::write_with_mattes(&mut written, &stack, &adapted.mattes).unwrap();
        aura_recipe::schema::Validation::check(&written).unwrap();
        // The same photograph planned again must be given the same settings.
        let again = plan_with(&adaptive);
        assert_eq!(
            serde_json::to_string(&again.report.assessments).unwrap(),
            serde_json::to_string(&adapted.report.assessments).unwrap(),
            "{name}: deterministic"
        );
        println!(
            "\n== {name} ({pw}x{ph}) faces {} · operations adapted {} / exact {} · planned in {took:.1?}",
            faces.len(),
            adapted.report.operations,
            plain.report.operations
        );
        let mut rows = Vec::new();
        for a in &adapted.report.assessments {
            let Some(e) = &a.expert else {
                println!(
                    "  face {}: {} (not measured) {}",
                    a.face, a.status, a.reason
                );
                continue;
            };
            let c = e.condition;
            println!(
                "  face {} {}: eye {:.0}px rough {:.3} blotch {:.4} light {:.2} shine {:.3} marks {:.0} lines {:.2} luma {:.3} · strength {:.2}",
                a.face, a.status, c.eye_px, c.roughness, c.blotch, c.light_stops, c.shine, c.marks, c.lines, c.skin_luma, e.intensity
            );
            for (key, [was, now]) in &e.adjusted {
                println!("      {key}: {was:.2} -> {now:.2}");
            }
            for note in &e.notes {
                println!("      - {note}");
            }
            println!(
                "      spots healed {} · marks kept {}",
                a.spots_healed, a.marks_kept
            );
            for value in e.adjusted.values().flatten() {
                assert!(value.is_finite());
            }
            rows.push(
                serde_json::json!({ "face": a.face, "status": a.status, "expert": e,
                "spotsHealed": a.spots_healed, "marksKept": a.marks_kept }),
            );
        }
        if let Some(first) = adapted
            .report
            .assessments
            .iter()
            .find_map(|a| a.expert.as_ref())
        {
            with_faces += 1;
            signatures.insert(serde_json::to_string(&first.adjusted).unwrap());
        }
        report.push(serde_json::json!({ "photo": name, "width": pw, "height": ph, "faces": rows,
            "operationsAdapted": adapted.report.operations, "operationsExact": plain.report.operations }));
    }
    println!(
        "\n{with_faces} photographs with a measured face; {} distinct sets of settings",
        signatures.len()
    );
    std::fs::write(
        dir.join("adaptive-report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(with_faces >= 2, "the folder needs at least two portraits");
    assert!(
        signatures.len() * 10 >= with_faces * 7,
        "different photographs must not share one set of settings: {} of {with_faces}",
        signatures.len()
    );
}
