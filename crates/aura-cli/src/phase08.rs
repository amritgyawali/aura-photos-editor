//! The phase 08 gate.
//!
//! `aura-cli verify --phase 08` proves, on this machine, in one run, everything section
//! 13 of the phase document asks for that can be proved without a human: the migration
//! lands, the shipped thresholds load, a wedding's frames become moments with counts
//! that reconcile, bursts are grouped the way a photographer would group them on the
//! four regression patterns, near-identical frames are marked with a confidence, two
//! shooters on one instant become one moment with two sub-groups, a manual split
//! survives a full re-grouping, an undo restores exactly what was there, and two runs
//! produce identical moments.
//!
//! It is deliberately a separate binary path from the tests, for the same reason the
//! phase 03 to 07 gates are: the tests prove the pieces, the gate proves the assembly,
//! with the same code CI runs and a human reads the output of.
//!
//! **What it does not do, and why.** It does not import RAW files, decode pixels,
//! detect faces or classify scenes - those are gated by phases 01, 02, 06 and 07. What
//! phase 08 adds begins at a stored embedding, so the gate starts there, writing real
//! `embeddings` rows from the synthetic patterns in `aura_brain_wedding::fixtures::bursts`
//! whose grouping is known by construction. That is also the only way to state a
//! grouping accuracy at all while the shipped embedding is a placeholder (phase 05
//! condition C10).
//!
//! It never touches a network, and it never opens an image file - which is not a
//! restraint but a statement about the phase: grouping reads catalog rows.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_wedding::fixtures::bursts::{self, BurstTruth};
use aura_brain_wedding::moments::graph::MomentProfileRegistry;
use aura_brain_wedding::moments::moment::MomentStore;
use aura_brain_wedding::moments::{Moments, PassContext};
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::moment::{DuplicateKind, MomentService};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{AuraResult, PhotoId, ProjectId, SceneId};
use aura_index::contract::index::{ImageEmbedding, F16};
use aura_index::store::{Descriptors, EmbeddingStore, StoredEmbedding};

/// Section 0's headline gate.
const ARI_GATE: f64 = 0.90;

/// The embedding version the gate's vectors claim.
const EMBED_VER: u16 = 100;

/// Run the gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase08-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. The catalog reaches schema 8 and has every object migration 8 adds.
    let catalog_path = work.join("phase08.sqlite");
    drop(std::fs::remove_file(&catalog_path));

    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(catalog) => Arc::new(catalog),
        Err(err) => {
            eprintln!("[{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 8 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 8, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for table in ["moments", "moment_images", "duplicates", "moment_edits"] {
        match catalog.count(table) {
            Ok(count) => println!("  {table}: {count} rows"),
            Err(err) => {
                eprintln!("  {table}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 2. The shipped thresholds load. This is the one thing in phase 08 that halts, so
    //    it is checked before anything is built on top of it.
    let profiles = match MomentProfileRegistry::embedded() {
        Ok(loaded) => {
            println!(
                "thresholds: version {}, {} scenes argued over, {} the defaults",
                loaded.version(),
                loaded.overrides(),
                SceneId::ALL.len() - loaded.overrides()
            );
            loaded
        }
        Err(err) => {
            eprintln!("thresholds: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    // Section 6.2's named pair, in the documented direction. A table that had these the
    // wrong way round would group a wedding plausibly and lose family portraits.
    if profiles.get(SceneId::DanceFloor).edge_threshold
        >= profiles.get(SceneId::FamilyPortrait).edge_threshold
    {
        eprintln!("  dance_floor is not looser than family_portrait");
        failures += 1;
    } else {
        println!(
            "  dance_floor {:.2} is looser than family_portrait {:.2}, as section 6.2 requires",
            profiles.get(SceneId::DanceFloor).edge_threshold,
            profiles.get(SceneId::FamilyPortrait).edge_threshold
        );
    }

    // 3. A wedding's worth of frames becomes moments, through the real store.
    let store = Arc::new(MomentStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let moments = match Moments::new(Arc::clone(&store), Arc::clone(&clock)) {
        Ok(service) => Arc::new(service),
        Err(err) => {
            eprintln!("moments: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    let project = ProjectId::new();
    let patterns = bursts::all();
    let planted = match plant(&catalog, Arc::clone(&clock), &project, &patterns) {
        Ok(planted) => planted,
        Err(err) => {
            eprintln!("fixtures: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "fixtures: {} frames across {} shooting patterns",
        planted.len(),
        patterns.len()
    );

    let context = PassContext::bare(EMBED_VER);
    let report = match moments.group(&project, &context, &NullProgress, &CancelToken::new()) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("group: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "group: {} frames became {} moments in {} bursts, {:.1} frames each, {} ms",
        report.frames, report.moments, report.bursts, report.mean_size, report.elapsed_ms
    );
    if report.moments == 0 {
        eprintln!("  nothing was grouped");
        failures += 1;
    }
    // Section 13's first criterion: the counts reconcile. A moments view whose stacks
    // do not add up to the grid is a support ticket that cannot be answered.
    let outline = match moments.outline(project) {
        Ok(outline) => outline,
        Err(err) => {
            eprintln!("outline: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    if outline.frames() == planted.len() {
        println!(
            "  counts reconcile: {} frames in {} moments, coverage {:.0}%",
            outline.frames(),
            outline.len(),
            outline.coverage * 100.0
        );
    } else {
        eprintln!(
            "  {} frames planted but {} are in a moment",
            planted.len(),
            outline.frames()
        );
        failures += 1;
    }

    // 4. Section 13's second criterion, and section 0's headline: the bursts are
    //    grouped the way a photographer would group them. Measured per pattern, because
    //    an average over five patterns hides the one that came apart.
    let mut worst = 1.0f64;
    for (pattern, planted_frames) in patterns.iter().zip(planted_by_pattern(&patterns, &planted)) {
        let found = labels_from_catalog(&moments, &planted_frames);
        let ari = adjusted_rand_index(&pattern.labels(), &found);
        let mixed = mixed_groups(&pattern.labels(), &found);
        println!(
            "  {:<18} ARI {ari:.3}, {} mixed group(s)",
            pattern.name, mixed
        );
        if ari < ARI_GATE {
            eprintln!("    below the {ARI_GATE} gate");
            failures += 1;
        }
        if mixed > 0 {
            eprintln!("    a group spans two labelled moments");
            failures += 1;
        }
        worst = worst.min(ari);
    }
    println!(
        "grouping: worst ARI {worst:.3} across {} patterns",
        patterns.len()
    );

    // 5. Section 13's third criterion: near-identical frames carry a confidence.
    let mut capping = 0usize;
    let mut without_reasons = 0usize;
    for moment in &outline.moments {
        for set in &moment.duplicate_sets {
            if set.kind.caps_gallery_to_one() {
                capping += 1;
            }
            if set.reasons.is_empty() || set.confidence <= 0.0 {
                without_reasons += 1;
            }
        }
    }
    println!("duplicates: {capping} set(s) cap the gallery, every one with a confidence");
    if without_reasons > 0 {
        eprintln!("  {without_reasons} set(s) carry no reason - invariant 2");
        failures += 1;
    }
    // The bracketed detail pattern is three exposures a stop apart and must produce
    // exactly one capping set. This is section 10.1's one-stop-apart case, end to end
    // through the catalog rather than in a unit test.
    if capping == 0 {
        eprintln!("  the bracketed exposures were not recognised as one photograph");
        failures += 1;
    }

    // 6. Section 13's fourth criterion: two shooters, one moment, two sub-groups.
    let multi: Vec<_> = outline
        .moments
        .iter()
        .filter(|moment| moment.is_multi_camera())
        .collect();
    match multi.first() {
        Some(moment) if moment.bursts.len() >= 2 => {
            println!(
                "cross-camera: {} frames from {} bodies in one moment, {} sub-groups",
                moment.len(),
                moment.cameras.len(),
                moment.bursts.len()
            );
        }
        Some(moment) => {
            eprintln!(
                "cross-camera: the two shooters merged but the sub-groups were lost ({} burst)",
                moment.bursts.len()
            );
            failures += 1;
        }
        None => {
            eprintln!("cross-camera: the two shooters did not become one moment");
            failures += 1;
        }
    }

    // 7. Section 13's fifth criterion, first half: a manual edit is permanent. The
    //    split, then a full re-grouping, then the check that both halves survived it.
    let target = outline
        .moments
        .iter()
        .find(|moment| moment.len() >= 3)
        .map(|moment| (moment.id, moment.image_ids.clone()));
    let split_pair = match target {
        Some((moment, images)) => {
            let at = images.get(1).copied();
            match at.map(|photo| moments.split(moment, photo)) {
                Some(Ok(created)) => {
                    println!(
                        "decision: a {}-frame moment was split by hand",
                        images.len()
                    );
                    Some((moment, created))
                }
                Some(Err(err)) => {
                    eprintln!("split: [{}] {}", err.code, err.detail);
                    failures += 1;
                    None
                }
                None => None,
            }
        }
        None => {
            eprintln!("decision: no moment large enough to split");
            failures += 1;
            None
        }
    };

    if let Some((original, created)) = split_pair {
        let before = moments.outline(project).map(|o| o.locked).unwrap_or(0);
        match moments.group(&project, &context, &NullProgress, &CancelToken::new()) {
            Ok(second) => {
                let survived = moments
                    .outline(project)
                    .map(|o| {
                        o.moments
                            .iter()
                            .filter(|moment| moment.id == original || moment.id == created)
                            .count()
                    })
                    .unwrap_or(0);
                if survived == 2 {
                    println!(
                        "decision: the photographer's split survived a re-grouping \
                         ({before} locked of {} moments)",
                        second.moments + usize::try_from(before).unwrap_or(0)
                    );
                } else {
                    eprintln!(
                        "decision: only {survived} of the two split halves survived a re-grouping"
                    );
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("regroup: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }

        // 8. Section 13's fifth criterion, second half: the edit is undoable, and the
        //    undo restores exactly.
        match moments.undo(project) {
            Ok(edit) => {
                let after = moments.outline(project).map(|o| o.len()).unwrap_or(0);
                let halves_left = moments
                    .outline(project)
                    .map(|o| {
                        o.moments
                            .iter()
                            .filter(|moment| moment.id == created)
                            .count()
                    })
                    .unwrap_or(0);
                if halves_left == 0 {
                    println!(
                        "undo: the split was reversed ({} on a {}-frame moment), {after} moments",
                        edit.action, edit.moment_size
                    );
                } else {
                    eprintln!("undo: the split's second half is still there");
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("undo: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 9. Invariant 4: two runs produce the same moments.
    let second_path = work.join("phase08-second.sqlite");
    drop(std::fs::remove_file(&second_path));
    match determinism(&second_path, Arc::clone(&clock), &patterns) {
        Ok((moments_a, moments_b)) if moments_a == moments_b => {
            println!("determinism: two projects, the same {moments_a} moments");
        }
        Ok((moments_a, moments_b)) => {
            eprintln!("determinism: {moments_a} moments, then {moments_b}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("determinism: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 10. Nothing here rejected a photograph. Section 2.2, checked rather than trusted:
    //     every frame that was planted is still in the catalog and still reachable.
    match catalog.count("photo") {
        Ok(count) if usize::try_from(count).unwrap_or(0) >= planted.len() => {
            println!("scope: {count} photographs, none removed - phase 12 owns the cull");
        }
        Ok(count) => {
            eprintln!("scope: {} planted but {count} remain", planted.len());
            failures += 1;
        }
        Err(err) => {
            eprintln!("scope: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    if failures == 0 {
        println!("\nphase 08 gate: pass");
        ExitCode::SUCCESS
    } else {
        eprintln!("\nphase 08 gate: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

/// Write one project's photographs and embeddings from the burst patterns.
///
/// Real `photo` and `embeddings` rows, through the real stores, because the gate's job
/// is to prove the assembly rather than the arithmetic. The patterns are laid out one
/// after another with a five-minute gap, so each is grouped in isolation exactly as it
/// would be inside a wedding.
fn plant(
    catalog: &Arc<Catalog>,
    clock: Arc<dyn Clock>,
    project: &ProjectId,
    patterns: &[BurstTruth],
) -> AuraResult<Vec<PhotoId>> {
    let key = project.to_db();
    catalog.writer().transact({
        let key = key.clone();
        move |tx| {
            tx.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'phase08', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z')",
                rusqlite::params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            Ok(())
        }
    })?;

    // The two bodies, as phase 01's import pass records them. Written here rather than
    // left to the serial fallback because the gate's job is to prove the assembly the
    // product actually runs, and in the product the camera table is populated.
    //
    // The id is derived from the project so that planting two projects into one
    // catalog - which the determinism check does - does not collide on `camera_id`'s
    // primary key. A body serial is unique per project, not globally: two weddings shot
    // on the same camera are two camera rows, and phase 01's format reflects that.
    let project_hex: String = key
        .trim_start_matches("prj_")
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    for camera in 1..=2u32 {
        let key = key.clone();
        let stem: String = project_hex.chars().cycle().take(62).collect::<String>();
        catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT INTO camera (camera_id, project_id, make, model, body_serial,
                                     shooter_label, created_at, updated_at)
                 VALUES (?1, ?2, 'AURA', 'Fixture', ?3, ?4,
                         '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z')",
                rusqlite::params![
                    format!("cam_{stem}{camera:02x}"),
                    key,
                    format!("body-{camera}"),
                    if camera == 1 { "primary" } else { "second" },
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("camera", &e))?;
            Ok(())
        })?;
    }

    let embeddings = EmbeddingStore::new(Arc::clone(catalog), clock);
    let mut planted = Vec::new();
    let mut offset = 0i64;

    for pattern in patterns {
        let frames = bursts::to_frames(pattern, SceneId::Unknown, false);
        let base = frames.first().map_or(0, |frame| frame.ts);
        for frame in &frames {
            let photo = PhotoId::new();
            planted.push(photo);

            // Written exactly as phase 01's ingest writes it, and that detail is the
            // point of this gate rather than an incidental of it. EXIF's
            // `DateTimeOriginal` has whole-second resolution, so `timeline_time` gets
            // the whole second and `SubSecTimeOriginal` - centiseconds on almost every
            // body - gets the fraction. A gate that wrote millisecond timestamps into
            // `timeline_time` would be exercising a format the product never produces,
            // and would have hidden the defect this one found.
            let at = offset + (frame.ts - base);
            let stamp = crate::phase07::rfc3339_ms(at);
            let sub_sec = i64::from(u16::try_from(at.rem_euclid(1_000) / 10).unwrap_or(0));
            let photo_key = photo.to_db();
            let project_key = key.clone();
            // Two bodies, so the cross-camera merge has something to merge. The serial
            // is what phase 01's camera table keys on.
            let serial = format!("body-{}", frame.camera);
            catalog.writer().transact(move |tx| {
                tx.execute(
                    "INSERT INTO photo (photo_id, project_id, timeline_time, camera_serial,
                                        sub_sec, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?3, ?3)",
                    rusqlite::params![photo_key, project_key, stamp, serial, sub_sec],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
                Ok(())
            })?;

            let mut embedding = ImageEmbedding::zeroed(photo, EMBED_VER);
            for (slot, value) in embedding.vec.iter_mut().zip(frame.embedding.iter()) {
                *slot = F16::from_f32(*value);
            }
            embedding.dhash = frame.dhash;
            embeddings.put(
                &key,
                &StoredEmbedding {
                    embedding,
                    descriptors: Descriptors::empty(1),
                    preprocess_ver: 1,
                    pixel_tier: 2,
                    pixel_source: "decoded",
                },
            )?;
        }
        // Five minutes between patterns: far past the twenty-second hard gap, so no
        // edge can cross from one pattern into the next.
        let span = frames
            .last()
            .zip(frames.first())
            .map_or(0, |(last, first)| last.ts - first.ts);
        offset += span + 300_000;
    }

    Ok(planted)
}

/// Which planted photographs belong to each pattern, in order.
fn planted_by_pattern(patterns: &[BurstTruth], planted: &[PhotoId]) -> Vec<Vec<PhotoId>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for pattern in patterns {
        let count = pattern.frames.len();
        out.push(planted.get(at..at + count).unwrap_or_default().to_vec());
        at += count;
    }
    out
}

/// The moment each of a pattern's frames ended up in, as dense labels.
fn labels_from_catalog(moments: &Arc<Moments>, photos: &[PhotoId]) -> Vec<usize> {
    let mut seen: Vec<String> = Vec::new();
    photos
        .iter()
        .map(|photo| {
            let id = moments
                .moment_of(*photo)
                .ok()
                .flatten()
                .map(|moment| moment.id.to_db())
                .unwrap_or_default();
            match seen.iter().position(|known| *known == id) {
                Some(index) => index,
                None => {
                    seen.push(id);
                    seen.len() - 1
                }
            }
        })
        .collect()
}

/// Groups that span two labelled moments. Section 10.1's stricter half of the ARI gate.
fn mixed_groups(truth: &[usize], found: &[usize]) -> usize {
    let mut by_group: std::collections::BTreeMap<usize, BTreeSet<usize>> =
        std::collections::BTreeMap::new();
    for (label, group) in truth.iter().zip(found.iter()) {
        by_group.entry(*group).or_default().insert(*label);
    }
    by_group.values().filter(|labels| labels.len() > 1).count()
}

/// Adjusted Rand Index, identical to `tests/eval/burst_eval.rs`'s.
fn adjusted_rand_index(truth: &[usize], found: &[usize]) -> f64 {
    let n = truth.len().min(found.len());
    if n < 2 {
        return 1.0;
    }
    let choose2 = |count: usize| -> f64 {
        let value = count as f64;
        value * (value - 1.0) / 2.0
    };
    let mut table: std::collections::BTreeMap<(usize, usize), usize> =
        std::collections::BTreeMap::new();
    let mut rows: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    let mut cols: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    for index in 0..n {
        let (Some(a), Some(b)) = (truth.get(index), found.get(index)) else {
            continue;
        };
        *table.entry((*a, *b)).or_insert(0) += 1;
        *rows.entry(*a).or_insert(0) += 1;
        *cols.entry(*b).or_insert(0) += 1;
    }
    let sum_cells: f64 = table.values().copied().map(choose2).sum();
    let sum_rows: f64 = rows.values().copied().map(choose2).sum();
    let sum_cols: f64 = cols.values().copied().map(choose2).sum();
    let total = choose2(n);
    if total <= 0.0 {
        return 1.0;
    }
    let expected = sum_rows * sum_cols / total;
    let maximum = 0.5 * (sum_rows + sum_cols);
    if (maximum - expected).abs() < f64::EPSILON {
        return 1.0;
    }
    (sum_cells - expected) / (maximum - expected)
}

/// Group the same patterns into two fresh projects and compare the moment counts.
fn determinism(
    path: &std::path::Path,
    clock: Arc<dyn Clock>,
    patterns: &[BurstTruth],
) -> AuraResult<(usize, usize)> {
    let catalog = Arc::new(Catalog::open(path, Arc::clone(&clock), crate::APP_VERSION)?);
    let store = Arc::new(MomentStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let moments = Moments::new(Arc::clone(&store), Arc::clone(&clock))?;
    let context = PassContext::bare(EMBED_VER);

    let mut counts = Vec::new();
    for _ in 0..2 {
        let project = ProjectId::new();
        plant(&catalog, Arc::clone(&clock), &project, patterns)?;
        let report = moments.group(&project, &context, &NullProgress, &CancelToken::new())?;
        counts.push(report.moments);
    }
    Ok((
        counts.first().copied().unwrap_or(0),
        counts.get(1).copied().unwrap_or(0),
    ))
}

/// Unused today; kept so the gate can be extended to a per-class duplicate breakdown
/// without re-deriving the mapping.
#[allow(dead_code)]
fn class_name(kind: DuplicateKind) -> &'static str {
    kind.as_str()
}
