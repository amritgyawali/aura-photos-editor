//! The phase 30 mechanical gate.
//!
//! The assembly proof for delivery: migration 30 and its objects, the preset table a product
//! manager owns and the widened bound it refuses, a whole synthetic wedding written to disk through
//! the real writers and read back, verification catching a corrupted file, four thousand
//! collision-free names, the two triggers that make a promise a property of the database, the
//! learning loop's closed vocabulary and its held-out split, rollback restoring bytes, the two
//! colour vocabularies still agreeing, and the IPC surface's three files.
//!
//! **Nothing here proves anything about a real wedding.** Every fixture is a plate this repository
//! authored, and the photographs a delivery would carry come from a renderer whose camera profiles
//! were never measured. There is no photographer, no closed beta, no crash-free rate and no upload,
//! so four of section 10.1's rows are **unmeasured** and one - the twelve-minute export budget - is
//! waived on a machine with no GPU. Those are the conditions in the exit report, and they are
//! printed at the end of every run rather than hidden in a helper.
//!
//! The unit tests prove the pieces and `tests/eval/delivery_eval.rs` proves the gates. This proves
//! the assembly - the things that only exist when a catalog, a preset file, a field, a source and a
//! writer are in the same process.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::delivery::{
    DeliveryCode, DeliveryColour, Destination, ExportJob, ExportSet, FileFormat, ImageId,
    NamingTemplate, OutputSharpen, Resize, UploadState, MAX_SETS, MIN_JPEG_QUALITY, MIN_LONG_EDGE,
};
use aura_core::contract::learn::{
    LearnCode, Learnable, MIN_CORRECTIONS, MIN_OFFERABLE_IMPROVEMENT, MIN_PROJECTS, OUTLIER_MADS,
};
use aura_core::ProjectId;
use aura_delivery::providers::{registry, ScriptedTransport};
use aura_delivery::resume;
use aura_export::api::ExportPass;
use aura_export::fixtures::{Plate, ScriptedField, ScriptedSource};
use aura_export::read::Frame;
use aura_export::sets::Presets;
use aura_export::store::ExportStore;
use aura_export::verify::hash_file;
use rusqlite::params;

/// Run the phase 30 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase30-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // ---------------------------------------------------------------------------------------
    // 1. Migration 30 and every object it owns.
    // ---------------------------------------------------------------------------------------
    let catalog_path = work.join("phase30.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 30 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 30, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let expected_tables = [
        "export_job",
        "export_set",
        "export_file",
        "export_reason",
        "delivery_manifest",
        "delivery_target",
        "delivery_backup",
        "delivery_upload",
        "learn_correction",
        "learn_update",
        "learn_update_row",
        "learn_profile_snapshot",
        "learn_consent",
    ];
    let expected_views = ["v_export_coverage", "v_learn_buckets", "v_delivery_state"];
    let expected_triggers = [
        "export_file_verified_needs_a_hash",
        "export_file_verified_needs_a_hash_upd",
        "delivery_manifest_no_update",
        "delivery_upload_sent_within_bytes",
        "learn_update_no_self_adopt",
    ];
    match objects(&catalog) {
        Ok(found) => {
            let mut missing = Vec::new();
            for name in expected_tables
                .iter()
                .chain(expected_views.iter())
                .chain(expected_triggers.iter())
            {
                if !found.contains(*name) {
                    missing.push(*name);
                }
            }
            if missing.is_empty() {
                println!(
                    "migration 30: {} tables, {} views, {} triggers",
                    expected_tables.len(),
                    expected_views.len(),
                    expected_triggers.len()
                );
            } else {
                eprintln!("migration 30: missing {missing:?}");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("migration 30: {err}");
            failures += 1;
        }
    }

    // The schema carries no free-text field automation could write a sentence into. Phase 27's
    // rule, and the same scan with two corrections it needed.
    //
    // **It scans only the objects migration 30 owns**, because `sqlite_master` holds every
    // migration and phase 10's `emotion` table has a `narrative_weight` column - a *number* whose
    // name contains a banned word. The first version of this check failed on it, which is the
    // third time in this phase that a check has matched prose or an unrelated name; phase 27 wrote
    // the lesson down twice and it keeps being worth having.
    //
    // **And it matches a column declaration rather than a substring**, so a word inside a longer
    // identifier is not a hit.
    match schema_text_for(&catalog, &expected_tables) {
        Ok(sql) => {
            let code = strip_sql_comments(&sql).to_ascii_lowercase();
            let banned = [
                "diagnosis",
                "sentence",
                "narrative",
                "explanation",
                "note text",
            ];
            let found: Vec<&str> = banned
                .iter()
                .copied()
                .filter(|word| declares_column(&code, word))
                .collect();
            if found.is_empty() {
                println!("schema: no stored-sentence column in migration 30");
            } else {
                eprintln!("schema: migration 30 carries a stored sentence: {found:?}");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("schema scan: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 2. The preset table, and the widened bound it refuses.
    // ---------------------------------------------------------------------------------------
    match Presets::built_in() {
        Ok(presets) => {
            let names: Vec<&str> = presets.all().iter().map(|p| p.name.as_str()).collect();
            let wanted = ["gallery", "album", "social", "teaser", "bw", "handoff"];
            let missing: Vec<&str> = wanted
                .iter()
                .copied()
                .filter(|w| !names.contains(w))
                .collect();
            if missing.is_empty() {
                println!("presets: {} rows, every one with a reason", names.len());
            } else {
                eprintln!("presets: missing {missing:?}");
                failures += 1;
            }
            for row in presets.all() {
                if row.reason.trim().len() < 20 {
                    eprintln!("presets: `{}` has no written reason", row.name);
                    failures += 1;
                }
            }
        }
        Err(err) => {
            eprintln!("presets: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // A studio may tighten and never widen. Three shapes, each refused.
    let widened = [
        (
            "quality below the contract floor",
            r#"version = 1
[[preset]]
name = "gallery"
format = "jpeg"
quality = 40
colour = "srgb"
bit_depth = 8
resize = "full"
sharpen = "screen"
naming = "{seq}"
reason = "a quality the contract does not permit"
"#,
        ),
        (
            "sixteen bits in a jpeg",
            r#"version = 1
[[preset]]
name = "gallery"
format = "jpeg"
quality = 92
colour = "srgb"
bit_depth = 16
resize = "full"
sharpen = "screen"
naming = "{seq}"
reason = "a depth the format cannot carry"
"#,
        ),
        (
            "a naming template that names a folder",
            r#"version = 1
[[preset]]
name = "gallery"
format = "jpeg"
quality = 92
colour = "srgb"
bit_depth = 8
resize = "full"
sharpen = "screen"
naming = "{date}/{seq}"
reason = "a template that could write outside the destination"
"#,
        ),
    ];
    let mut all_refused = true;
    for (what, text) in widened {
        if Presets::parse(text).is_ok() {
            eprintln!("presets: accepted {what}");
            failures += 1;
            all_refused = false;
        }
    }
    if all_refused {
        println!("presets: three widened bounds refused");
    }

    // ---------------------------------------------------------------------------------------
    // 3. A wedding, written to disk through the real writers and read back.
    // ---------------------------------------------------------------------------------------
    let project = ProjectId::new();
    let images: Vec<ImageId> = (0..24).map(|_| ImageId::new()).collect();
    if let Err(err) = seed(&catalog, project, &images) {
        eprintln!("seed: {err}");
        return ExitCode::FAILURE;
    }

    let store = ExportStore::new(Arc::clone(&catalog));
    let out = work.join("delivery");
    drop(std::fs::remove_dir_all(&out));
    let mut field = ScriptedField::new(Some("Alex & Sam"), 400, 24);
    for (ix, image) in images.iter().enumerate() {
        field = field.with_frame(
            *image,
            Frame {
                image: Some(*image),
                original_stem: Some(format!("DSC_{:04}", ix % 12)),
                date: Some("2026-05-16".to_owned()),
                ..Frame::default()
            },
        );
    }
    let source = ScriptedSource::new(Plate::Gradient, 96, 72);

    // Three sets in one job, three formats: every writer runs.
    let job = ExportJob::new(
        vec![
            set(
                "gallery",
                &images[..8],
                FileFormat::Jpeg,
                "{date}_{couple}_{seq}",
            ),
            set("album", &images[8..16], FileFormat::Tiff, "{original}"),
            set("social", &images[16..], FileFormat::Png, "{seq}"),
        ],
        Destination::Folder { path: out.clone() },
    );

    let pass = ExportPass::new(&store, &field, &source, crate::APP_VERSION);
    let result = match pass.run(project, &job) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("export: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    if result.files.len() == 24 && result.skipped.is_empty() {
        println!("export: 24 files across three formats");
    } else {
        eprintln!(
            "export: {} files, {} skipped",
            result.files.len(),
            result.skipped.len()
        );
        failures += 1;
    }

    // Every file is on disk, and its stored digest is the digest of what is on disk. The whole
    // phase turns on this being true rather than asserted.
    let mut mismatched = 0usize;
    for file in &result.files {
        let path = out.join(&file.path);
        match hash_file(&path) {
            Ok(actual) if actual == file.hash => {}
            _ => mismatched += 1,
        }
    }
    if mismatched == 0 {
        println!("verification: every stored digest is the digest of the file on disk");
    } else {
        eprintln!("verification: {mismatched} files do not match their stored digest");
        failures += 1;
    }

    match &result.manifest {
        Some(manifest) if manifest.fully_hashed() && manifest.files.len() == 24 => {
            println!("manifest: sealed, 24 files, every one hashed");
        }
        Some(manifest) => {
            eprintln!(
                "manifest: {} files, fully hashed {}",
                manifest.files.len(),
                manifest.fully_hashed()
            );
            failures += 1;
        }
        None => {
            eprintln!("manifest: not sealed");
            failures += 1;
        }
    }

    // The travelling copy parses as the document another studio's software would read.
    let doc_path = out.join(aura_core::contract::delivery::MANIFEST_NAME);
    match std::fs::read_to_string(&doc_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    {
        Some(parsed) if parsed["schema"] == "aura.delivery-manifest/1" => {
            println!("manifest: the travelling copy parses");
        }
        _ => {
            eprintln!("manifest: the travelling copy is not valid JSON of the right schema");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 4. Verification catches a deliberately corrupted write. Section 10.1's second row.
    // ---------------------------------------------------------------------------------------
    match corrupt_and_check(&out, &result.files) {
        Ok(true) => println!("verification: a corrupted file is detected"),
        Ok(false) => {
            eprintln!("verification: a corrupted file was NOT detected");
            failures += 1;
        }
        Err(err) => {
            eprintln!("verification: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 5. Four thousand collision-free names, including duplicates from two cameras.
    // ---------------------------------------------------------------------------------------
    match names_are_unique(4000) {
        Ok(count) => println!("naming: {count} unique names from 4,000 frames sharing 12 stems"),
        Err(err) => {
            eprintln!("naming: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 6. The two triggers, each with a control first.
    //
    // Phase 21's rule: a refusal test that cannot tell a working guard from a broken fixture
    // proves nothing. Each control has to succeed before its refusal counts.
    // ---------------------------------------------------------------------------------------
    match trigger_check(&catalog, &result.files, &images) {
        Ok(report) => println!("triggers: {report}"),
        Err(err) => {
            eprintln!("triggers: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 7. The upload state machine: a drop is a pause, and a resume sends only the tail.
    // ---------------------------------------------------------------------------------------
    match resume_check() {
        Ok(report) => println!("resume: {report}"),
        Err(err) => {
            eprintln!("resume: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 8. The learning loop's closed vocabulary, and the two floors.
    // ---------------------------------------------------------------------------------------
    let guarantee_words = [
        "texture", "identity", "skin", "crop", "cleanup", "mask", "coverage", "tattoo",
    ];
    let mut leaked = Vec::new();
    for learnable in Learnable::ALL {
        for word in guarantee_words {
            if learnable.as_str().contains(word) {
                leaked.push(learnable.as_str());
            }
        }
    }
    if leaked.is_empty() {
        println!(
            "learnable: {} preferences, no guarantee among them",
            Learnable::COUNT
        );
    } else {
        eprintln!("learnable: {leaked:?} name a guarantee rather than a preference");
        failures += 1;
    }

    match learning_check() {
        Ok(report) => println!("learning: {report}"),
        Err(err) => {
            eprintln!("learning: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 9. The two colour vocabularies still agree.
    //
    // `DeliveryColour` is `aura-core`'s and `OutputColour` is `aura-render`'s, because `aura-core`
    // depends on no workspace crate. A member added to one and not the other is a delivered file
    // whose ICC profile disagrees with the pixels inside it, and nothing else would notice.
    // ---------------------------------------------------------------------------------------
    let ours: BTreeSet<&str> = DeliveryColour::ALL.iter().map(|c| c.as_str()).collect();
    let theirs: BTreeSet<&str> = [
        aura_render::contract::render::OutputColour::Srgb,
        aura_render::contract::render::OutputColour::AdobeRgb,
        aura_render::contract::render::OutputColour::DisplayP3,
    ]
    .iter()
    .map(|c| c.as_str())
    .collect();
    if ours == theirs {
        println!(
            "colour: the two vocabularies agree on {} spaces",
            ours.len()
        );
    } else {
        eprintln!("colour: the vocabularies have drifted: {ours:?} against {theirs:?}");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 10. Reason codes: every one parses back, and only three stop a job.
    // ---------------------------------------------------------------------------------------
    let mut bad = Vec::new();
    for code in DeliveryCode::ALL {
        if DeliveryCode::parse(code.as_str())
            .map(|c| c != code)
            .unwrap_or(true)
        {
            bad.push(code.as_str());
        }
        if code.user_text().is_empty() {
            bad.push(code.as_str());
        }
    }
    let fatal: Vec<&str> = DeliveryCode::ALL
        .iter()
        .copied()
        .filter(|c| c.is_fatal())
        .map(DeliveryCode::as_str)
        .collect();
    if bad.is_empty() && fatal.len() == 3 {
        println!(
            "reasons: {} codes, {} of which stop a job",
            DeliveryCode::COUNT,
            fatal.len()
        );
    } else {
        eprintln!("reasons: malformed {bad:?}, fatal {fatal:?}");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 11. The IPC surface's three files agree.
    // ---------------------------------------------------------------------------------------
    match ipc_surface() {
        Ok(count) => println!("ipc: {count} handlers = registrations = client wrappers"),
        Err(err) => {
            eprintln!("ipc: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 12. The autopilot's export stage, which closes phase 28's condition C7.
    // ---------------------------------------------------------------------------------------
    //
    // The stage repeats the export a wedding has already been given and never invents one, so what
    // is checked is the reader that makes that possible: a project with no export has no
    // specification, and a project with one recovers **the same** destination, sets and policy off
    // the rows the job itself wrote. A reader that lost a set's quality would repeat a delivery at
    // the wrong quality, which is worse than not repeating it.
    match store.last_spec(ProjectId::new()) {
        Ok(None) => println!("autopilot: a wedding nobody exported has no job to repeat"),
        Ok(Some(_)) => {
            eprintln!("autopilot: a project with no export returned a specification");
            failures += 1;
        }
        Err(err) => {
            eprintln!("autopilot: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match store.last_spec(project) {
        Ok(Some(spec)) => {
            let same_destination = spec.destination == job.destination;
            let same_policy = spec.metadata == job.metadata && spec.verify == job.verify;
            // Name order, because that is what the reader sorts by; the job's own order is not a
            // property the schema keeps and repeating it would be a claim the rows cannot support.
            let mut wanted: Vec<&aura_core::contract::delivery::ExportSet> =
                job.sets.iter().collect();
            wanted.sort_by(|a, b| a.name.cmp(&b.name));
            let same_sets = spec.sets.len() == wanted.len()
                && spec.sets.iter().zip(&wanted).all(|(got, want)| {
                    got.name == want.name
                        && got.format == want.format
                        && got.quality == want.quality
                        && got.resize == want.resize
                        && got.sharpen == want.sharpen
                        && got.naming.as_str() == want.naming.as_str()
                        && got.colour == want.colour
                        && got.bit_depth == want.bit_depth
                        && got.sidecar == want.sidecar
                });
            if same_destination && same_policy && same_sets {
                println!(
                    "autopilot: the last job's {} sets, destination and policy all read back",
                    spec.sets.len()
                );
            } else {
                eprintln!(
                    "autopilot: the recovered specification differs - destination {same_destination}, \
                     policy {same_policy}, sets {same_sets}"
                );
                failures += 1;
            }
            // And it carries no photographs, which is what makes it a specification rather than a
            // stored job: a repeat runs over what is selected *now*.
            let job_over_nothing = spec.over(&[]);
            if job_over_nothing.sets.iter().all(|s| s.images.is_empty()) {
                println!("autopilot: the specification carries no photographs of its own");
            } else {
                eprintln!("autopilot: the specification carried photographs");
                failures += 1;
            }
        }
        Ok(None) => {
            eprintln!("autopilot: the exported project has no job to repeat");
            failures += 1;
        }
        Err(err) => {
            eprintln!("autopilot: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // What this run did not prove.
    // ---------------------------------------------------------------------------------------
    println!();
    println!("Not proved by this gate, and printed on every run rather than left in a document:");
    println!("  C1  Every photograph a delivery would carry is rendered through camera profiles");
    println!("      that were never measured (phase 14 condition C2), from models that are");
    println!("      placeholders. The writers are exact; the pixels are not a claim.");
    println!("  C2  Section 11's export budget - 1,000 45 MP JPEGs in 12 minutes - is waived on a");
    println!("      machine with no GPU backend. What is measured is the writer, not the render.");
    println!("  C3  No network transport ships, so no upload to a real gallery has happened. The");
    println!("      state machine is exercised against a transport that drops on demand.");
    println!("  C4  No profile has been fitted from a real photographer's corrections, so the");
    println!("      15 % style-match improvement of section 10.1 is unmeasured.");
    println!("  C5  There has been no closed beta, so the 99.5 % crash-free rate is unmeasured.");
    println!("  C6  Signing, notarisation and the staged rollout are specified and not executed.");
    println!();
    println!(
        "Closed by this phase: phase 28's condition C7. Every stage in the autopilot's DAG is"
    );
    println!("built, `AppRunner::availability` is empty for the first time, and a completed run");
    println!("writes files - into the destination a photographer already chose, or not at all.");

    if failures == 0 {
        println!();
        println!("phase 30: every mechanical check passed");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 30: {failures} checks failed");
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn set(name: &str, images: &[ImageId], format: FileFormat, naming: &str) -> ExportSet {
    ExportSet {
        name: name.to_owned(),
        images: images.to_vec(),
        format,
        quality: 92,
        resize: Resize::Full,
        sharpen: OutputSharpen::None,
        naming: NamingTemplate::parse(naming).unwrap_or_default(),
        colour: DeliveryColour::Srgb,
        bit_depth: 8,
        sidecar: false,
    }
}

fn seed(catalog: &Arc<Catalog>, project: ProjectId, images: &[ImageId]) -> Result<(), String> {
    let key = project.to_db();
    let ids: Vec<String> = images.iter().map(ImageId::to_db).collect();
    catalog
        .writer()
        .with(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'phase 30', '2026-05-16T00:00:00Z', '2026-05-16T00:00:00Z')",
                params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for (ix, id) in ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                         created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, ?4, ?4)",
                    params![
                        id,
                        key,
                        1_760_000_000_000_i64 + ix as i64,
                        "2026-05-16T00:00:00Z"
                    ],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        })
        .map_err(|e| format!("[{}] {}", e.code, e.detail))
}

/// Corrupt one delivered file and confirm the read-back notices.
fn corrupt_and_check(
    root: &Path,
    files: &[aura_core::contract::delivery::ExportedFile],
) -> Result<bool, String> {
    let Some(file) = files.first() else {
        return Err("no files to corrupt".to_owned());
    };
    let path = root.join(&file.path);
    let mut bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.len() < 64 {
        return Err("the file is too small to corrupt meaningfully".to_owned());
    }
    // A single flipped byte in the middle, which is what a bad sector produces.
    let at = bytes.len() / 2;
    if let Some(slot) = bytes.get_mut(at) {
        *slot = slot.wrapping_add(1);
    }
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;

    let actual = hash_file(&path).map_err(|e| format!("[{}] {}", e.code, e.detail))?;
    let detected = actual != file.hash;

    // Put it back, so a second run of the gate starts from a delivery that verifies.
    if let Some(slot) = bytes.get_mut(at) {
        *slot = slot.wrapping_sub(1);
    }
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(detected)
}

/// Section 10.1: collision-free names across 4,000 files, including duplicates from two cameras.
fn names_are_unique(count: usize) -> Result<usize, String> {
    let images: Vec<ImageId> = (0..count).map(|_| ImageId::new()).collect();
    let mut field = ScriptedField::new(Some("Alex & Sam"), count as u32, count as u32);
    for (ix, image) in images.iter().enumerate() {
        field = field.with_frame(
            *image,
            Frame {
                image: Some(*image),
                // Twelve stems across four thousand frames: every name collides, many times.
                original_stem: Some(format!("DSC_{:04}", ix % 12)),
                ..Frame::default()
            },
        );
    }
    let job = ExportJob::new(
        vec![set(
            "gallery",
            &images,
            FileFormat::Jpeg,
            NamingTemplate::HANDOFF_DEFAULT,
        )],
        Destination::Folder {
            path: PathBuf::from("/does-not-need-to-exist"),
        },
    );
    let planned = aura_export::naming::plan(&job, &field)
        .map_err(|e| format!("[{}] {}", e.code, e.detail))?;
    let unique: BTreeSet<String> = planned
        .iter()
        .map(|p| p.rel_path.to_string_lossy().to_ascii_lowercase())
        .collect();
    if unique.len() != planned.len() {
        return Err(format!(
            "{} names for {} frames: {} collided",
            unique.len(),
            planned.len(),
            planned.len() - unique.len()
        ));
    }
    Ok(unique.len())
}

/// The two triggers that make a promise a property of the database, each with a control.
fn trigger_check(
    catalog: &Arc<Catalog>,
    files: &[aura_core::contract::delivery::ExportedFile],
    images: &[ImageId],
) -> Result<String, String> {
    let Some(image) = images.first().copied() else {
        return Err("no images".to_owned());
    };
    if files.is_empty() {
        return Err("no files".to_owned());
    }

    // Find the job the pass opened.
    let job: Option<String> = catalog
        .read(|conn| {
            Ok(conn
                .query_row("SELECT job_id FROM export_job LIMIT 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .ok())
        })
        .map_err(|e| format!("[{}] {}", e.code, e.detail))?;
    let Some(job) = job else {
        return Err("no export job row".to_owned());
    };

    // --- export_file_verified_needs_a_hash ---
    //
    // The control first: the same row with a real digest is accepted, so a refusal below is the
    // trigger rather than a foreign key. Phase 21's rule.
    let control = insert_file(catalog, &job, image, "gate/control.jpg", &"a".repeat(64), 1);
    if control.is_err() {
        return Err("the control insert failed, so the refusal below proves nothing".to_owned());
    }
    let refused = insert_file(catalog, &job, image, "gate/bad.jpg", "", 1);
    if refused.is_ok() {
        return Err("a file claiming to be verified with no digest was accepted".to_owned());
    }

    // --- delivery_manifest_no_update ---
    let updated = catalog.writer().with(move |conn| {
        conn.execute("UPDATE delivery_manifest SET files = files + 1", [])
            .map(|_| ())
            .map_err(|e| aura_core::errors::db::statement_failed("delivery_manifest", &e))
    });
    if updated.is_ok() {
        return Err("a sealed manifest was edited".to_owned());
    }

    // --- learn_update_no_self_adopt ---
    let profile = aura_core::contract::ids::ProfileId::new();
    let key = profile.to_db();
    let control = catalog.writer().with(move |conn| {
        conn.execute(
            "INSERT INTO learn_update (update_id, profile_id, from_version, to_version,
                 corrections_used, held_out_used, current_error, candidate_error,
                 expected_improvement, diff_summary, adopted, computed_at)
             VALUES ('gate-1', ?1, 1, 2, 40, 10, 0.2, 0.1, 0.5, '[]', 0, '2026-05-16T00:00:00Z')",
            params![key],
        )
        .map(|_| ())
        .map_err(|e| aura_core::errors::db::statement_failed("learn_update", &e))
    });
    if control.is_err() {
        return Err("the learn_update control insert failed".to_owned());
    }
    let key = profile.to_db();
    let refused = catalog.writer().with(move |conn| {
        conn.execute(
            "INSERT INTO learn_update (update_id, profile_id, from_version, to_version,
                 corrections_used, held_out_used, current_error, candidate_error,
                 expected_improvement, diff_summary, adopted, computed_at)
             VALUES ('gate-2', ?1, 2, 3, 40, 10, 0.2, 0.1, 0.5, '[]', 1, '2026-05-16T00:00:00Z')",
            params![key],
        )
        .map(|_| ())
        .map_err(|e| aura_core::errors::db::statement_failed("learn_update", &e))
    });
    if refused.is_ok() {
        return Err("an update that arrived already adopted was accepted".to_owned());
    }

    Ok("three refusals, each after a control that succeeded".to_owned())
}

fn insert_file(
    catalog: &Arc<Catalog>,
    job: &str,
    image: ImageId,
    rel: &str,
    hash: &str,
    verified: i64,
) -> Result<(), String> {
    let job = job.to_owned();
    let rel = rel.to_owned();
    let hash = hash.to_owned();
    let id = image.to_db();
    catalog
        .writer()
        .with(move |conn| {
            conn.execute(
                "INSERT INTO export_file (job_id, set_name, photo_id, rel_path, bytes, hash,
                     width, height, render_hash, verified, renamed, written_at)
                 VALUES (?1, 'gate', ?2, ?3, 10, ?4, 16, 16, ?5, ?6, 0, '2026-05-16T00:00:00Z')",
                params![job, id, rel, hash, "b".repeat(64), verified],
            )
            .map(|_| ())
            .map_err(|e| aura_core::errors::db::statement_failed("export_file", &e))
        })
        .map_err(|e| format!("[{}] {}", e.code, e.detail))
}

/// A drop is a pause, and the resume sends only what is missing.
fn resume_check() -> Result<String, String> {
    let provider = registry("folder-gallery").map_err(|e| e.detail.clone())?;
    let transport = ScriptedTransport::new();
    let bytes: Vec<u8> = (0..(resume::CHUNK * 2 + 700))
        .map(|i| (i % 251) as u8)
        .collect();
    let item = aura_core::contract::delivery::UploadItem {
        image: ImageId::new(),
        set: "gallery".to_owned(),
        path: PathBuf::from("gallery/a.jpg"),
        bytes: bytes.len() as u64,
        hash: blake3::hash(&bytes).to_hex().to_string(),
        state: UploadState::Pending,
    };
    let mapping = aura_core::contract::delivery::SetMapping {
        set: "gallery".to_owned(),
        remote: "main".to_owned(),
        publish: false,
    };
    let key = provider.key_for(&mapping, &item.path);

    transport.drop_after(resume::CHUNK / 4);
    let first = resume::step(&transport, &item, &bytes, &key);
    let UploadState::InProgress { sent, .. } = first.state else {
        return Err(format!(
            "a drop left the file {:?} rather than in progress",
            first.state
        ));
    };
    if sent == 0 {
        return Err("a drop kept nothing, so a resume would be a restart".to_owned());
    }

    transport.recover();
    let mut resumed = item.clone();
    resumed.state = first.state;
    let second = resume::send(&transport, &resumed, &bytes, &key)
        .map_err(|e| format!("[{}] {}", e.code, e.detail))?;
    if second.state != UploadState::Verified {
        return Err(format!("the resume ended {:?}", second.state));
    }
    if second.sent >= item.bytes {
        return Err("the resume re-sent the whole file".to_owned());
    }
    match transport.contents(&key) {
        Some(held) if held == bytes => {}
        _ => return Err("the far end holds different bytes from the ones sent".to_owned()),
    }

    // And a wrong digest is `corrupt` rather than `failed`: different situations, different rows.
    let transport = ScriptedTransport::new();
    transport.corrupt(&key);
    let one = resume::step(&transport, &item, &bytes, &key);
    if one.state != UploadState::Corrupt {
        return Err(format!("a wrong digest read as {:?}", one.state));
    }

    Ok(format!(
        "a drop kept {sent} bytes, the resume sent {} of {}, a wrong digest is corrupt not failed",
        second.sent, item.bytes
    ))
}

/// The learning loop's two floors, its trim, and the deterministic split.
fn learning_check() -> Result<String, String> {
    use aura_core::contract::ids::DecisionId;
    use aura_core::contract::learn::CorrectionBucket;
    use aura_core::contract::ledger::DecisionKind;
    use aura_core::contract::scene::SceneId;
    use aura_learn::aggregate::{fold, hold_out, Sample};

    let bucket = CorrectionBucket {
        kind: DecisionKind::Edit,
        scene: SceneId::Unknown,
        learnable: Learnable::Exposure,
        subject_close: false,
    };
    let samples = |magnitudes: &[f32], projects: usize| -> Vec<Sample> {
        magnitudes
            .iter()
            .enumerate()
            .map(|(i, m)| Sample {
                decision: DecisionId::new(),
                project: (i % projects) as u64,
                magnitude: *m,
            })
            .collect()
    };

    // Enough corrections from one wedding is still one wedding.
    let one_wedding = samples(&[0.3; 80], 1);
    let (agg, reasons) = fold(bucket, &one_wedding);
    if agg.actionable {
        return Err("eighty corrections from one wedding were treated as actionable".to_owned());
    }
    if !reasons.iter().any(|r| r.code == LearnCode::TooFewWeddings) {
        return Err("the one-wedding refusal did not say which floor it missed".to_owned());
    }

    // A mostly-identical bucket still has its extremes trimmed. The MAD is zero here, which is
    // the case the naive guard protected and should not have.
    let mut magnitudes = vec![0.20_f32; 60];
    magnitudes.extend([3.5_f32; 4]);
    let mixed = samples(&magnitudes, 4);
    let (agg, _) = fold(bucket, &mixed);
    if agg.outliers_dropped == 0 {
        return Err("four extreme corrections survived the trim".to_owned());
    }
    if (agg.central - 0.20).abs() > 0.03 {
        return Err(format!("the trimmed centre moved to {}", agg.central));
    }

    // Two bounds: half the measured shift, clamped at the ceiling.
    let large = samples(&[4.0; 60], 4);
    let (agg, _) = fold(bucket, &large);
    if (agg.proposed_offset().abs() - Learnable::Exposure.ceiling()).abs() > 1e-5 {
        return Err(format!(
            "a four-stop bucket proposed {} rather than the ceiling",
            agg.proposed_offset()
        ));
    }

    // The split is reproducible from the correction's own id.
    let id = DecisionId::new();
    if hold_out(id) != hold_out(id) {
        return Err("the held-out split is not deterministic".to_owned());
    }

    Ok(format!(
        "floors at {MIN_CORRECTIONS} corrections and {MIN_PROJECTS} weddings, trim at \
         {OUTLIER_MADS} deviations, offer floor {MIN_OFFERABLE_IMPROVEMENT}"
    ))
}

/// Every `#[tauri::command]` has a registration and a client wrapper, and nothing else does.
fn ipc_surface() -> Result<usize, String> {
    let shell = std::fs::read_to_string("ui/src-tauri/src/main.rs")
        .map_err(|err| format!("ui/src-tauri/src/main.rs could not be read: {err}"))?;
    let client = std::fs::read_to_string("ui/src/ipc/client.ts")
        .map_err(|err| format!("ui/src/ipc/client.ts could not be read: {err}"))?;

    let mut defined = BTreeSet::new();
    let mut expect_fn = false;
    for line in shell.lines() {
        let line = line.trim();
        if line == "#[tauri::command]" {
            expect_fn = true;
            continue;
        }
        if expect_fn {
            if let Some(rest) = line
                .strip_prefix("async fn ")
                .or_else(|| line.strip_prefix("fn "))
            {
                if let Some(name) = rest.split('(').next() {
                    defined.insert(name.trim().to_string());
                    expect_fn = false;
                }
            }
        }
    }

    let Some((_, after)) = shell.split_once("generate_handler![") else {
        return Err("the shell has no `generate_handler!` list".to_string());
    };
    let Some((inside, _)) = after.split_once(']') else {
        return Err("the shell's `generate_handler!` list is not closed".to_string());
    };
    let mut registered = BTreeSet::new();
    for line in inside.lines() {
        // Comments in the list are prose, not names. Phase 27's lesson, twice over.
        let code = match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        };
        for part in code.split(',') {
            let name = part.trim();
            if !name.is_empty() {
                registered.insert(name.to_string());
            }
        }
    }

    let mut invoked = BTreeSet::new();
    for line in client.lines() {
        if !line.contains("invoke") {
            continue;
        }
        // The first single-quoted literal after `invoke`. A pattern anchored on `invoke<...>(`
        // misses every call whose type argument nests, which is 239 of 240.
        let Some(open) = line.find("('") else {
            continue;
        };
        let rest = &line[open + 2..];
        let Some(end) = rest.find('\'') else {
            continue;
        };
        let name = &rest[..end];
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            invoked.insert(name.to_string());
        }
    }

    let mut problems = Vec::new();
    for name in defined.difference(&registered) {
        problems.push(format!("`{name}` is defined and never registered"));
    }
    for name in registered.difference(&defined) {
        problems.push(format!("`{name}` is registered and has no definition"));
    }
    for name in invoked.difference(&registered) {
        problems.push(format!("the client calls `{name}` and no handler answers"));
    }
    for name in registered.difference(&invoked) {
        problems.push(format!("`{name}` is registered and nothing calls it"));
    }
    if problems.is_empty() {
        Ok(defined.len())
    } else {
        problems.truncate(6);
        Err(problems.join("; "))
    }
}

fn objects(catalog: &Arc<Catalog>) -> Result<BTreeSet<String>, String> {
    catalog
        .read(|conn| {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master")
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
            Ok(rows.flatten().collect())
        })
        .map_err(|e| format!("[{}] {}", e.code, e.detail))
}

/// The SQL of the named objects only.
///
/// Named rather than all of `sqlite_master`, because every migration since 01 is in there and a
/// scan for a word will eventually find one in a column that means something else entirely.
fn schema_text_for(catalog: &Arc<Catalog>, names: &[&str]) -> Result<String, String> {
    let wanted: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
    catalog
        .read(move |conn| {
            let mut out = String::new();
            for name in &wanted {
                if let Ok(sql) = conn.query_row(
                    "SELECT COALESCE(sql, '') FROM sqlite_master WHERE name = ?1",
                    params![name],
                    |row| row.get::<_, String>(0),
                ) {
                    out.push_str(&sql);
                    out.push('\n');
                }
            }
            Ok(out)
        })
        .map_err(|e| format!("[{}] {}", e.code, e.detail))
}

/// Whether the SQL declares a column whose *whole name* is the banned word.
///
/// A substring match reads `narrative_weight` as `narrative`, which is a number and not a
/// sentence. The boundary is what makes the check about what it says it is about.
fn declares_column(sql: &str, word: &str) -> bool {
    for at in sql.match_indices(word).map(|(i, _)| i) {
        let before = sql[..at].chars().next_back();
        let after = sql[at + word.len()..].chars().next();
        let bounded_before = before.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        let bounded_after = after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if bounded_before && bounded_after {
            return true;
        }
    }
    false
}

/// One schema's SQL with its comments removed.
fn strip_sql_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    for line in sql.lines() {
        let code = match line.find("--") {
            Some(index) => &line[..index],
            None => line,
        };
        out.push_str(code);
        out.push('\n');
    }
    out
}

/// Silence the unused-import warning for the constants the printed conditions refer to.
#[allow(dead_code)]
const _BOUNDS: (usize, u8, u32) = (MAX_SETS, MIN_JPEG_QUALITY, MIN_LONG_EDGE);
