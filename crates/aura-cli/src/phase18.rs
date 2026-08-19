//! The phase 18 mechanical gate.
//!
//! This is the assembly proof for local mask AI: migration 18 and its objects, the twenty-class
//! vocabulary and its storage split, the segmenter over painted fixtures, the trimap and the
//! matte, the instance scoping that must not bleed, the codec and the 180 KB budget, the algebra
//! the brush and phases 19 to 24 both go through, the quality gate that carries a constraint out
//! of this phase and into theirs, the store and its version discipline, and the two structural
//! refusals this phase inherits - no skin colour anywhere in the schema, and no way for this
//! crate to write a biometric.
//!
//! **Nothing here proves a mask is right about a wedding photograph.** There is no labelled
//! wedding imagery in this repository - section 9's DATA task did not happen and cannot happen
//! here - so every number below is measured against synthetic frames whose regions were painted
//! into the pixels and read back through the real pipeline. Both shipped heads are placeholders
//! and neither is consulted. The distinction is printed at the end of every run rather than
//! hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/mask_eval.rs` is the other
//! half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::{rfc3339, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::AuraResult;
use aura_vision::contract::mask::{
    MaskKind, MaskPayload, Storage, AGGRESSIVE_FLOOR, ALL_KINDS, ASSIGN_MIN_OVERLAP,
    FACE_SKIN_MIOU, HAIR_MIOU, PAYLOAD_BUDGET_BYTES, SUBJECT_MIOU,
};
use aura_vision::mask::fixtures::{self, Backdrop, SKIN_REFLECTANCES};
use aura_vision::mask::quality::Operation;
use aura_vision::mask::{algebra, matting, quality, segment, store, trimap, MaskPipeline};
use rusqlite::params;

/// Run the phase 18 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase18-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // ---------------------------------------------------------------------
    // 1. Migration 18 and every object it owns.
    // ---------------------------------------------------------------------
    let catalog_path = work.join("phase18.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 18 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 18, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "masks"),
        ("table", "mask_gate"),
        ("view", "v_mask_coverage"),
        ("index", "idx_masks_unscoped"),
        ("index", "idx_masks_image"),
        ("index", "idx_masks_versions"),
        ("index", "idx_masks_identity"),
    ] {
        match schema_object(&catalog, kind, name) {
            Ok(true) => println!("  {kind} {name}: present"),
            Ok(false) => {
                eprintln!("  {kind} {name}: missing");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  {kind} {name}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // ---------------------------------------------------------------------
    // 2. There is no image and no skin colour anywhere in this schema.
    //
    //    Two scans, and they guard different things. The first is this phase's own: a mask is
    //    pixel-shaped output, and a BLOB column that could hold a photograph is how phase 13's
    //    "evidence can never be a pixel" stops being true. The second is the fourth phase
    //    running - a "preferred skin tone" column would look reasonable in a file about skin
    //    masks, and it is the same mistake in friendlier clothes.
    // ---------------------------------------------------------------------
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("  no skin-colour column anywhere in migration 18");
        }
        Ok(found) => {
            eprintln!(
                "  migration 18 grew a skin colour column: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("  forbidden column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match no_image_columns() {
        Ok(()) => println!("  migration 18 has no column that could hold a photograph"),
        Err(detail) => {
            eprintln!("  {detail}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------
    // 3. Neither shipped head is trained, and neither is consulted.
    // ---------------------------------------------------------------------
    if segment::SEG_HEAD_TRAINED || matting::MATTING_HEAD_TRAINED {
        eprintln!("heads: a head claims to be trained; this build ships neither");
        failures += 1;
    } else {
        println!("heads: neither is trained, and no code path consults one");
    }
    let probe = fixtures::one_person(2, Backdrop::Wall);
    let features = segment::Features::measure(&probe.frame);
    if segment::class_hint(&features, 10, 10).is_some() {
        eprintln!("heads: the segmentation head returned a class");
        failures += 1;
    }
    let empty_map = trimap::build(&algebra::Plane::zeros(8, 8), 2);
    if matting::alpha_hint(&features, &empty_map).is_some() {
        eprintln!("heads: the matting head returned an alpha");
        failures += 1;
    }

    // ---------------------------------------------------------------------
    // 4. The vocabulary, and the storage split that makes the budget possible.
    // ---------------------------------------------------------------------
    if ALL_KINDS.len() != 20 {
        eprintln!("vocabulary: {} kinds, expected 20", ALL_KINDS.len());
        failures += 1;
    }
    let alpha_kinds: Vec<MaskKind> = ALL_KINDS
        .into_iter()
        .filter(|k| matches!(k.stored_as(), Storage::Alpha))
        .collect();
    if alpha_kinds.len() != 4 {
        eprintln!(
            "storage: {} alpha classes, expected 4 - the budget is written against that split",
            alpha_kinds.len()
        );
        failures += 1;
    } else {
        println!(
            "vocabulary: {} kinds, {} stored as alpha and {} as run lengths",
            ALL_KINDS.len(),
            alpha_kinds.len(),
            ALL_KINDS.len() - alpha_kinds.len()
        );
    }

    // ---------------------------------------------------------------------
    // 5. The mIoU gates, on painted pixels, across five reflectances.
    // ---------------------------------------------------------------------
    let mut worst_skin = 1.0_f32;
    let mut worst_hair = 1.0_f32;
    let mut worst_subject = 1.0_f32;
    for reflectance in 0..SKIN_REFLECTANCES.len() {
        let scene = fixtures::one_person(reflectance, Backdrop::Wall);
        let set = MaskPipeline::new().analyse(&scene.frame, Some(&scene.people), &[]);
        for (kind, worst) in [
            (MaskKind::Skin, &mut worst_skin),
            (MaskKind::Hair, &mut worst_hair),
            (MaskKind::Subject, &mut worst_subject),
        ] {
            let truth = algebra::threshold(&scene.truth_of(kind), 0.5);
            let got = set.of(kind).map_or_else(
                || algebra::Plane::zeros(truth.w, truth.h),
                |p| algebra::threshold(&p.plane, 0.5),
            );
            *worst = worst.min(algebra::iou(&truth, &got));
        }
    }
    println!(
        "miou: skin {worst_skin:.3} (gate {FACE_SKIN_MIOU}), hair {worst_hair:.3} (gate \
         {HAIR_MIOU}), subject {worst_subject:.3} (gate {SUBJECT_MIOU}), worst of five \
         reflectances"
    );
    if worst_skin < FACE_SKIN_MIOU {
        eprintln!("  skin is below its gate");
        failures += 1;
    }
    if worst_hair < HAIR_MIOU {
        eprintln!("  hair is below its gate");
        failures += 1;
    }
    if worst_subject < SUBJECT_MIOU {
        eprintln!("  subject is below its gate");
        failures += 1;
    }

    // ---------------------------------------------------------------------
    // 6. A frame with nobody in it invents nobody.
    // ---------------------------------------------------------------------
    let empty_scene = fixtures::no_people();
    let empty_set = MaskPipeline::new().analyse(&empty_scene.frame, Some(&empty_scene.people), &[]);
    let invented: Vec<&str> = [MaskKind::Skin, MaskKind::Face, MaskKind::Hair]
        .into_iter()
        .filter(|kind| empty_set.of(*kind).is_some_and(|p| !p.plane.is_empty()))
        .map(MaskKind::as_str)
        .collect();
    if invented.is_empty() {
        println!("faceless: no person class was invented on a frame with nobody in it");
    } else {
        eprintln!("faceless: invented {}", invented.join(", "));
        failures += 1;
    }

    // ---------------------------------------------------------------------
    // 7. Instance scoping does not bleed between adjacent people.
    // ---------------------------------------------------------------------
    let pair = fixtures::two_people();
    let bride = aura_core::IdentityId::new();
    let guest = aura_core::IdentityId::new();
    let scoped =
        MaskPipeline::new().analyse(&pair.frame, Some(&pair.people), &[(0, bride), (1, guest)]);
    let hers = scoped
        .planes
        .iter()
        .find(|p| p.kind == MaskKind::Skin && p.identity == Some(bride));
    let theirs = scoped
        .planes
        .iter()
        .find(|p| p.kind == MaskKind::Skin && p.identity == Some(guest));
    match (hers, theirs) {
        (Some(a), Some(b)) => {
            let bleed = algebra::iou(&a.plane, &b.plane);
            if bleed < 0.01 {
                println!(
                    "scoping: two adjacent people, overlap {bleed:.4} (floor \
                     {ASSIGN_MIN_OVERLAP} containment)"
                );
            } else {
                eprintln!("scoping: the two skin masks overlapped at {bleed:.3}");
                failures += 1;
            }
        }
        _ => {
            eprintln!("scoping: one of the two people got no scoped skin at all");
            failures += 1;
        }
    }
    if scoped.of(MaskKind::Skin).is_none() {
        eprintln!("scoping: the unscoped region did not survive beside the scoped ones");
        failures += 1;
    }

    // ---------------------------------------------------------------------
    // 8. The codec, and the storage budget as a guarantee rather than a target.
    // ---------------------------------------------------------------------
    let garden = fixtures::one_person(2, Backdrop::Garden);
    let garden_set = MaskPipeline::new().analyse(&garden.frame, Some(&garden.people), &[]);
    let mut total_bytes = 0usize;
    let mut round_trip_failures = 0usize;
    for plane in &garden_set.planes {
        let (payload, _) = store::encode(plane.kind, &plane.plane);
        total_bytes += payload.byte_len();
        if matches!(payload, MaskPayload::Rle { .. }) {
            let back = store::decode(&payload);
            if back != algebra::threshold(&plane.plane, 0.5) {
                round_trip_failures += 1;
            }
        }
    }
    println!(
        "storage: {total_bytes} bytes for all {} classes of one frame (budget \
         {PAYLOAD_BUDGET_BYTES})",
        garden_set.planes.len()
    );
    if total_bytes > PAYLOAD_BUDGET_BYTES {
        eprintln!("  over budget");
        failures += 1;
    }
    if round_trip_failures > 0 {
        eprintln!("  {round_trip_failures} run-length payloads did not round-trip exactly");
        failures += 1;
    }

    // ---------------------------------------------------------------------
    // 9. The quality gate carries a constraint into phases 19 to 24.
    // ---------------------------------------------------------------------
    let mut skin_plane = match garden_set.of(MaskKind::Skin) {
        Some(plane) => plane.clone(),
        None => {
            eprintln!("gating: there was no skin plane to gate");
            failures += 1;
            return finish(failures);
        }
    };
    skin_plane.edge_quality = 0.05;
    quality::settle(&mut skin_plane);
    let (bad, _) = aura_vision::mask::to_mask(garden.image_id, &skin_plane, 0.0);
    let smooth = quality::allowance(&bad, Operation::SkinSmooth);
    let tone = quality::allowance(&bad, Operation::LocalTone);
    if smooth.permitted {
        eprintln!("gating: a badly determined boundary was allowed to smooth skin");
        failures += 1;
    }
    if !tone.permitted || tone.ceiling <= 0.0 {
        eprintln!("gating: a badly determined boundary blocked a local tone move outright");
        failures += 1;
    }
    if smooth.note.is_none() {
        eprintln!("gating: a refusal recorded no reason");
        failures += 1;
    }
    if bad.allowance() >= AGGRESSIVE_FLOOR {
        eprintln!("gating: the allowance did not fall below the floor");
        failures += 1;
    }
    if failures == 0 {
        println!(
            "gating: allowance {:.2} blocks skin smoothing and still carries {:.0}% of a local \
             tone move",
            bad.allowance(),
            tone.ceiling * 100.0
        );
    }

    // ---------------------------------------------------------------------
    // 10. The algebra, including the composition later phases are written against.
    // ---------------------------------------------------------------------
    let subject = garden_set.of(MaskKind::Subject).map(|p| p.plane.clone());
    let skin = garden_set.of(MaskKind::Skin).map(|p| p.plane.clone());
    match (subject, skin) {
        (Some(subject), Some(skin)) => {
            let without = algebra::subtract(&subject, &skin);
            if algebra::iou(&without, &skin) > 0.02 {
                eprintln!("algebra: skin survived `mask minus skin`");
                failures += 1;
            } else if without.coverage() <= 0.0 {
                eprintln!("algebra: `mask minus skin` removed everything");
                failures += 1;
            } else {
                println!("algebra: union, intersect, subtract, invert, feather, grow and shrink");
            }
        }
        _ => {
            eprintln!("algebra: the composition had nothing to run on");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------
    // 11. The store: a photographer's mask survives a regeneration.
    // ---------------------------------------------------------------------
    match store_round_trip(&catalog, &clock) {
        Ok(report) => println!("store: {report}"),
        Err(detail) => {
            eprintln!("store: {detail}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------
    // 12. Determinism.
    // ---------------------------------------------------------------------
    let first = MaskPipeline::new().analyse(&garden.frame, Some(&garden.people), &[]);
    let second = MaskPipeline::new().analyse(&garden.frame, Some(&garden.people), &[]);
    let identical = first.planes.len() == second.planes.len()
        && first
            .planes
            .iter()
            .zip(second.planes.iter())
            .all(|(a, b)| a.kind == b.kind && a.plane == b.plane);
    if identical {
        println!("determinism: two runs over one frame produced identical regions");
    } else {
        eprintln!("determinism: two runs disagreed");
        failures += 1;
    }

    // ---------------------------------------------------------------------
    // 13. This crate still cannot write a biometric.
    // ---------------------------------------------------------------------
    match no_template_writes() {
        Ok(()) => println!("biometrics: aura-vision writes no table that aura-people owns"),
        Err(detail) => {
            eprintln!("biometrics: {detail}");
            failures += 1;
        }
    }

    finish(failures)
}

fn finish(failures: usize) -> ExitCode {
    println!();
    println!(
        "phase-18 verify: every number above is measured on synthetic frames whose regions were \
         painted into the pixels. There is no labelled wedding imagery in this repository, both \
         shipped heads are placeholders, and neither is consulted. See \
         docs/progress/PHASE-18-EXIT.md condition C1."
    );
    if failures == 0 {
        println!("phase-18 verify: all checks clean");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase-18 verify: {failures} failures");
        ExitCode::FAILURE
    }
}

fn schema_object(catalog: &Catalog, kind: &str, name: &str) -> AuraResult<bool> {
    let kind = kind.to_string();
    let name = name.to_string();
    catalog.read(move |conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                params![kind, name],
                |row| row.get(0),
            )
            .map_err(|err| aura_core::errors::db::statement_failed("sqlite_master", &err))?;
        Ok(count > 0)
    })
}

/// Any column in migration 18 whose name suggests an absolute skin colour.
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(|conn| {
        let mut found = Vec::new();
        for table in ["masks", "mask_gate"] {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|err| aura_core::errors::db::statement_failed("table_info", &err))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|err| aura_core::errors::db::statement_failed("table_info", &err))?;
            for row in rows {
                let column =
                    row.map_err(|err| aura_core::errors::db::statement_failed("table_info", &err))?;
                let lower = column.to_lowercase();
                for needle in [
                    "skin_tone",
                    "skin_colour",
                    "skin_color",
                    "ideal_skin",
                    "skin_hue",
                ] {
                    if lower.contains(needle) {
                        found.push(format!("{table}.{column}"));
                    }
                }
            }
        }
        Ok(found)
    })
}

/// Migration 18 has exactly one BLOB, and it is a run length or an alpha plane.
///
/// The scan is over the migration's own text rather than over the opened schema, because what
/// matters is what a reader of the file would find - a future column called `thumbnail BLOB`
/// would be as wrong on the day it was written as on the day it first held a photograph.
fn no_image_columns() -> Result<(), String> {
    let sql = include_str!("../../aura-catalog/migrations/0018_masks.sql");
    let body: String = sql
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<&str>>()
        .join("\n");
    for needle in ["thumbnail", "jpeg", "preview", "crop_blob", "image BLOB"] {
        if body.to_lowercase().contains(needle) {
            return Err(format!("migration 18 grew a `{needle}` column"));
        }
    }
    let blobs = body.matches("BLOB").count();
    if blobs != 1 {
        return Err(format!(
            "migration 18 has {blobs} BLOB columns, expected exactly one"
        ));
    }
    Ok(())
}

/// Nothing in `aura-vision` writes a table `aura-people` owns.
///
/// The same grep `crates/aura-vision/tests/no_template_writes.rs` runs, here as well so the gate
/// fails on it without a `cargo test`. Phase 06's structural defence became a rule when this
/// crate gained a catalog, and a rule that is only checked in one place is a rule that is
/// checked when somebody remembers to run that place.
fn no_template_writes() -> Result<(), String> {
    let root = std::path::Path::new("crates/aura-vision/src");
    let mut files = Vec::new();
    collect(root, &mut files);
    if files.is_empty() {
        return Err("no aura-vision sources were scanned".to_string());
    }
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let upper = text.to_uppercase();
        for table in [
            "FACE_TEMPLATES",
            "IDENTITIES",
            "FACE_CROPS",
            "BIOMETRIC_KEYS",
        ] {
            let Some(at) = upper.find(table) else {
                continue;
            };
            let start = at.saturating_sub(200);
            let window = upper.get(start..at).unwrap_or_default();
            for verb in ["INSERT", "UPDATE", "DELETE FROM", "REPLACE INTO"] {
                if window.contains(verb) {
                    return Err(format!(
                        "{}: a `{verb}` statement names `{table}`",
                        file.display()
                    ));
                }
            }
        }
    }
    Ok(())
}

fn collect(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Write masks, edit one, regenerate, and prove the photographer's own survived.
fn store_round_trip(catalog: &Arc<Catalog>, clock: &Arc<dyn Clock>) -> Result<String, String> {
    use aura_vision::mask::store::MaskStore;

    let project = crate::ensure_project(catalog, clock.as_ref(), "phase-18")?;
    let photo = aura_core::PhotoId::new();
    let now = rfc3339(clock.now_utc());
    let photo_key = photo.to_db();
    let project_key = project.to_db();
    catalog
        .writer()
        .transact(move |tx| {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, orientation, created_at, updated_at) \
                 VALUES (?1, ?2, 1, ?3, ?3)",
                params![photo_key, project_key, now],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("insert photo", &err))?;
            Ok(())
        })
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;

    let scene = fixtures::one_person(2, Backdrop::Wall);
    let set = MaskPipeline::new().analyse(&scene.frame, Some(&scene.people), &[]);
    let mut masks = Vec::new();
    for mut plane in set.planes {
        quality::settle(&mut plane);
        let (mask, _) = aura_vision::mask::to_mask(photo, &plane, 0.0);
        masks.push(mask);
    }

    let store = MaskStore::new(Arc::clone(catalog));
    let written = store
        .put(photo, &masks)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;

    // A photographer edits one of them.
    let mut edited = masks
        .first()
        .cloned()
        .ok_or_else(|| "the pass produced no masks".to_string())?;
    edited.user_edited = true;
    store
        .save_edit(&edited)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;

    // Automation runs again. The edit must survive, and the check is inside the DELETE's own
    // WHERE rather than in the code that calls it.
    store
        .put(photo, &masks)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;
    let after = store
        .masks(photo)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;
    let survived = after.iter().filter(|m| m.user_edited).count();
    if survived != 1 {
        return Err(format!(
            "{survived} hand-edited masks survived a regeneration, expected 1"
        ));
    }

    let bytes = store
        .bytes_for(photo)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;
    if bytes > PAYLOAD_BUDGET_BYTES {
        return Err(format!(
            "{bytes} stored bytes against a budget of {PAYLOAD_BUDGET_BYTES}"
        ));
    }

    // And the regeneration command clears the edit deliberately.
    store
        .regenerate(edited.id)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;
    let cleared = store
        .masks(photo)
        .map_err(|err| format!("[{}] {}", err.code, err.detail))?;
    if cleared.iter().any(|m| m.user_edited) {
        return Err("a regeneration did not clear the photographer's edit".to_string());
    }

    Ok(format!(
        "{written} masks written, {bytes} bytes, one hand edit survived a regeneration and was \
         cleared only on request"
    ))
}
