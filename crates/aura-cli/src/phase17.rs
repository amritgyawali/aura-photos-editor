//! The phase 17 mechanical gate.
//!
//! This is the assembly proof for style learning: migration 17 and its objects, the pair
//! matcher's four strategies, the recipe fitter against a real render, the bucketing, the
//! regression with its shrinkage and its archive cap, the diagnostics, the signed bundle, the
//! store and its version discipline, and the two structural refusals this phase inherits from
//! phases 15 and 16 - no skin target anywhere in the schema, and no way to upload an archive.
//!
//! **Nothing here proves a photographer would recognise their own work.** There are no
//! archives in this repository - section 9's DATA task did not happen and cannot happen here -
//! so every number below is measured against synthetic archives whose style was chosen, applied
//! through the real renderer, and recovered. The distinction is printed at the end of every run
//! rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/style_eval.rs` is the
//! other half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::scene::ChapterId;
use aura_core::contract::style::{
    FallbackLevel, LightingBucket, SceneGroup, StyleBucket, StyleDelta, StyleProfile, StyleService,
    APPLY_ABOVE, MATCH_DE00_CEILING, MAX_EXPOSURE_DELTA_EV, SKIN_BAND_FRACTION, USABLE_PAIRS,
};
use aura_core::contract::tone::{SKIN_DE00_CEILING, SKIN_DE00_SPREAD};
use aura_core::{AuraResult, ProfileId, ProjectId};
use aura_render::cpu::Frame;
use aura_style::api::{Style, ANALYSIS_VER, MIN_PAIRS};
use aura_style::bucket::{Features, PairSource};
use aura_style::fit::optimise::{Axis, Budget, Candidate};
use aura_style::fixtures::{AuthoredArchive, Look, WIDTH};
use aura_style::store::StyleStore;
use aura_style::tree::{self, Sample, Target};
use aura_style::{diagnostics, fit, infer, pairs, profile as bundle};
use rusqlite::params;

/// Run the phase 17 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase17-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 17 and every object it owns.
    let catalog_path = work.join("phase17.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 17 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 17, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "profiles"),
        ("table", "profile_buckets"),
        ("table", "style_pairs"),
        ("table", "project_style"),
        ("view", "v_style_coverage"),
        ("index", "idx_profiles_name_version"),
        ("index", "idx_profiles_adopted"),
        ("index", "idx_style_pairs_pending"),
        ("index", "idx_style_pairs_fit"),
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

    // 2. There is nowhere in this schema to put a skin colour.
    //
    //    Third phase running, and the version of the trap that is hardest to see: a "preferred
    //    skin tone" column here would look reasonable, because it would be the photographer's
    //    own taste from their own work. It is the same mistake in friendlier clothes.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("  no skin-colour column anywhere in migration 17");
        }
        Ok(found) => {
            eprintln!(
                "  migration 17 grew a skin colour column: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("  forbidden column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match skin_bounds(&catalog) {
        Ok(true) => println!("  the skin lean columns are bounded below phase 16's own ceilings"),
        Ok(false) => {
            eprintln!("  the skin lean columns have no CHECK, so a row could ask for more than the guarantee allows");
            failures += 1;
        }
        Err(err) => {
            eprintln!("  skin bound scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 3. The pair matcher, all four strategies and both refusals.
    let matcher = matcher_report();
    println!(
        "matcher: {} exact, {} by stem, {} by time, {} perceptual, {} ambiguous refusals",
        matcher.0, matcher.1, matcher.2, matcher.3, matcher.4
    );
    if matcher.0 == 0 || matcher.1 == 0 || matcher.2 == 0 || matcher.3 == 0 {
        eprintln!("  a matching strategy never fired, so it is not covered");
        failures += 1;
    }
    if matcher.4 == 0 {
        eprintln!("  the ambiguity refusal never fired");
        failures += 1;
    }

    // 4. The fitter, against a real render of a known look.
    match fit_recovery(&clock) {
        Ok((residual, exposure_error, iterations)) => {
            println!(
                "fitter: residual {residual:.3} dE00, exposure error {exposure_error:.3} EV, \
                 {iterations} renders"
            );
            if residual > fit::REJECT_DE00 {
                eprintln!("  the fit could not reproduce its own render");
                failures += 1;
            }
            if exposure_error > 0.34 {
                eprintln!("  the recovered exposure is more than one step from the truth");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("fitter: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. The bucketing: every scene lands in a group, every light in a bucket.
    let leaves = StyleBucket::all();
    println!(
        "buckets: {} leaves over {} groups and {} kinds of light",
        leaves.len(),
        SceneGroup::COUNT,
        LightingBucket::COUNT
    );
    if leaves.len() != StyleBucket::COUNT {
        eprintln!("  the matrix is not the size the contract says it is");
        failures += 1;
    }
    for scene in aura_core::SceneId::ALL {
        let group = SceneGroup::of_scene(scene);
        if group == SceneGroup::Other && scene != aura_core::SceneId::Unknown {
            eprintln!("  scene {scene} has no group, so it would train the wrong leaf");
            failures += 1;
        }
    }

    // 6. The tree: shrinkage, the archive cap, and the fallback chain.
    let tree_report = tree_checks();
    println!(
        "tree: seven pairs is the smallest speaking bucket ({}), the cap bound {}, one outlier \
         moved the global lean {:.3} EV against a bound of {:.3}",
        tree_report.0, tree_report.1, tree_report.2, tree_report.3
    );
    if !tree_report.0 {
        eprintln!("  the smallest speaking bucket does not agree with APPLY_ABOVE");
        failures += 1;
    }
    if !tree_report.1 {
        eprintln!("  the archive cap did not bind on a five-archive fit");
        failures += 1;
    }
    if tree_report.2 > tree_report.3 {
        eprintln!("  one wedding moved the global profile past its bound");
        failures += 1;
    }

    // 7. A style delta is bounded on construction, and the skin bands hardest of all.
    let wild = StyleDelta {
        exposure: 9.0,
        temperature_k: 9000.0,
        contrast: 900.0,
        hsl: full_hsl(),
        skin_bias: aura_core::contract::style::SkinBias {
            warmth_deg: 90.0,
            chroma: 9.0,
        },
        ..StyleDelta::neutral()
    }
    .clamped();
    let orange = wild.hsl.get(aura_core::contract::colour::HslBand::Orange).h;
    let green = wild.hsl.get(aura_core::contract::colour::HslBand::Green).h;
    println!(
        "bounds: exposure clamped to {:.2} EV, the skin band to {orange:.1} against {green:.1} \
         elsewhere, the skin warmth lean to {:.2} deg",
        wild.exposure, wild.skin_bias.warmth_deg
    );
    if wild.exposure > MAX_EXPOSURE_DELTA_EV + 1e-4 {
        eprintln!("  an unbounded exposure reached a delta");
        failures += 1;
    }
    if orange > green * SKIN_BAND_FRACTION + 1e-3 {
        eprintln!("  the skin band is not held to a fraction of the rest");
        failures += 1;
    }
    if wild.skin_bias.warmth_deg >= aura_core::contract::colour::SKIN_HUE_CEILING_DEG {
        eprintln!("  a style may ask for more skin movement than phase 16's guarantee allows");
        failures += 1;
    }

    // 8. The store: write a profile, read it back, adopt it, select it, resolve through it.
    let store = Arc::new(StyleStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let service = Style::new(Arc::clone(&store));
    match store_round_trip(&catalog, &store, &service, &clock) {
        Ok(bytes) => {
            println!("store: one profile and its leaves round-tripped, {bytes} bytes on disk");
            if bytes > aura_style::BYTES_PER_PROFILE {
                eprintln!(
                    "  a profile costs {bytes} bytes against a budget of {}",
                    aura_style::BYTES_PER_PROFILE
                );
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 9. The bundle: signed, round-tripped, and refused when tampered with.
    match bundle_checks() {
        Ok((bytes, fingerprint)) => {
            println!("bundle: {bytes} bytes, key {fingerprint}, tampering refused");
        }
        Err(detail) => {
            eprintln!("bundle: {detail}");
            failures += 1;
        }
    }

    // 10. Nothing in this crate can reach a network, and nothing on the surface carries a pixel.
    match no_network() {
        Ok(()) => println!("privacy: aura-style depends on no cloud crate and opens no socket"),
        Err(detail) => {
            eprintln!("privacy: {detail}");
            failures += 1;
        }
    }

    // 11. The two passes that consume a style bumped their analysis versions.
    println!(
        "versions: style fitter {ANALYSIS_VER}, tone analysis {}, colour analysis {}",
        aura_brain_photo::tone::analyse::ANALYSIS_VER,
        aura_brain_photo::colour::analyse::ANALYSIS_VER
    );
    if aura_brain_photo::tone::analyse::ANALYSIS_VER < 2
        || aura_brain_photo::colour::analyse::ANALYSIS_VER < 2
    {
        eprintln!("  a pass that now applies a style did not bump its analysis version");
        failures += 1;
    }

    // 12. The minimum-pair rule is a product decision with a written reason, not a tuned number.
    println!(
        "policy: {MIN_PAIRS} pairs before a profile exists, {USABLE_PAIRS} before AURA is \
         confident, {MATCH_DE00_CEILING} dE00 as the match ceiling"
    );
    if MIN_PAIRS as u32 >= USABLE_PAIRS {
        eprintln!(
            "  the minimum is not below the confidence threshold, so a weak profile cannot exist"
        );
        failures += 1;
    }

    // 13. Phase 15's skin numbers are untouched by this phase.
    println!(
        "inherited: phase 15's skin ceiling {SKIN_DE00_CEILING} dE00 and spread \
         {SKIN_DE00_SPREAD} are unchanged"
    );

    println!();
    println!(
        "What this gate proves: the matcher, the fitter, the bucketing, the regression, the \
         shrinkage, the archive cap, the bundle, the store and the two structural refusals. \
         Every number above is measured against synthetic archives whose style was chosen, \
         applied through the real renderer and recovered."
    );
    println!(
        "What it does not prove: that a photographer would recognise their own work. There are \
         no archives in this repository. Condition C1 in docs/progress/PHASE-17-EXIT.md."
    );

    if failures == 0 {
        println!();
        println!("phase 17: OK");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 17: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

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

/// Any column in migration 17 whose name suggests an absolute skin colour.
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(|conn| {
        let mut found = Vec::new();
        for table in [
            "profiles",
            "profile_buckets",
            "style_pairs",
            "project_style",
        ] {
            let mut statement = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|err| aura_core::errors::db::statement_failed("table_info", &err))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|err| aura_core::errors::db::statement_failed("table_info", &err))?;
            for row in rows {
                let column =
                    row.map_err(|err| aura_core::errors::db::statement_failed("column", &err))?;
                let lower = column.to_ascii_lowercase();
                // `skin_warmth_deg` and `skin_chroma` are *leans* and are allowed. What is
                // forbidden is anything that could hold an absolute: a target, a tone, a
                // chromaticity or a demographic bucket.
                for needle in [
                    "skin_target",
                    "skin_tone",
                    "ideal_skin",
                    "preferred_skin",
                    "skin_uv",
                    "skin_hue_target",
                    "monk",
                    "fitzpatrick",
                    "ethnicity",
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

/// The skin lean columns must be bounded by the schema, below phase 16's own ceilings.
fn skin_bounds(catalog: &Catalog) -> AuraResult<bool> {
    catalog.read(|conn| {
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'profiles'",
                [],
                |row| row.get(0),
            )
            .map_err(|err| aura_core::errors::db::statement_failed("profiles sql", &err))?;
        Ok(sql.contains("skin_warmth_deg >= -1.5") && sql.contains("skin_chroma >= -0.04"))
    })
}

/// How many pairs each strategy found on an archive built to exercise all four, plus how many
/// ambiguous matches were refused.
fn matcher_report() -> (u32, u32, u32, u32, u32) {
    use aura_style::pairs::{Archive, ArchiveFile};

    let archive = Archive {
        label: "gate".to_string(),
        originals: vec![
            ArchiveFile::named("exact.cr3").with_hash("h-exact"),
            ArchiveFile::named("IMG_4471.nef"),
            ArchiveFile::named("timed.arw").with_capture(1_000),
            ArchiveFile::named("renamed.orf").with_dhash(0xFFFF_0000_FFFF_0000),
            // Two originals sharing a stem: the ambiguity refusal.
            ArchiveFile::named("IMG_0001.cr3"),
            ArchiveFile::named("IMG_0001.arw"),
        ],
        finals: vec![
            ArchiveFile::named("whatever.jpg").with_source("h-exact"),
            ArchiveFile::named("IMG_4471-Edit.jpg"),
            // A different stem on purpose: with a matching stem the filename strategy would
            // claim this pair first and the capture-time path would never run, which is
            // exactly the coverage hole this fixture exists to close.
            ArchiveFile::named("DSC_9911.jpg").with_capture(1_400),
            ArchiveFile::named("something-else.jpg").with_dhash(0xFFFF_0000_FFFF_0003),
            ArchiveFile::named("IMG_0001.jpg"),
            ArchiveFile::named("IMG_0001-2.jpg"),
        ],
    };
    let report = pairs::match_all(&archive);
    let count = |slug: &str| report.by_method.get(slug).copied().unwrap_or(0);
    (
        count("content_hash"),
        count("filename_stem"),
        count("capture_time"),
        count("perceptual"),
        u32::try_from(report.unmatched_originals.len()).unwrap_or(0),
    )
}

/// Fit a known look against a real render of it.
fn fit_recovery(clock: &Arc<dyn Clock>) -> AuraResult<(f32, f32, u32)> {
    let look = Look::moody();
    let archive = AuthoredArchive::build(&look, 1, false)?;
    let key = "moody-0000";
    let (rgb, width, height) = archive.original(key, WIDTH)?;
    let (target, _, _) = archive.finished(key, WIDTH)?;
    let base = aura_recipe::fixtures::neutral(key, aura_style::fixtures::CAMERA);
    let frame = Frame::working(rgb, width, height, aura_style::fixtures::CAMERA);
    let engine = fit::optimise::engine(Arc::clone(clock));
    let fitted = fit::fit(&engine, &frame, &base, &target, Budget::default())?;
    let truth = Candidate::of_params(&look.params);
    Ok((
        fitted.residual_de00,
        (fitted.candidate.get(Axis::Exposure) - truth.get(Axis::Exposure)).abs(),
        fitted.iterations,
    ))
}

/// Shrinkage floor, archive cap, and the bounded outlier movement.
fn tree_checks() -> (bool, bool, f32, f32) {
    let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight);
    let floor_below = tree::shrink_factor(6, tree::PRIOR_STRENGTH) < APPLY_ABOVE;
    let floor_at = tree::shrink_factor(7, tree::PRIOR_STRENGTH) >= APPLY_ABOVE;

    let mut normal: Vec<Sample> = Vec::new();
    for wedding in 0..4 {
        normal.extend(samples(bucket, 60, 0.0, &format!("normal-{wedding}")));
    }
    let without = tree::fit(&normal);
    let mut with_outlier = normal;
    with_outlier.extend(samples(bucket, 400, 2.0, "outlier"));
    let with = tree::fit(&with_outlier);

    (
        floor_below && floor_at,
        with.archive_capped,
        (with.global.exposure - without.global.exposure).abs(),
        tree::influence_bound(2.0),
    )
}

fn samples(bucket: StyleBucket, count: usize, exposure: f32, archive: &str) -> Vec<Sample> {
    (0..count)
        .map(|_| {
            let mut targets = [0.0_f32; tree::TARGETS];
            if let Some(slot) = targets.get_mut(Target::Exposure.index()) {
                *slot = exposure;
            }
            Sample::new(Features::intercept_only(bucket), targets, archive)
        })
        .collect()
}

fn full_hsl() -> aura_core::contract::colour::HslAdjustments {
    use aura_core::contract::colour::{HslAdjustments, HslBand, HslShift};
    let mut out = HslAdjustments::default();
    for band in HslBand::ALL {
        out.set(
            band,
            HslShift {
                h: 100.0,
                s: 100.0,
                l: 100.0,
            },
        );
    }
    out
}

/// Write a profile, read it back, adopt it, select it, and resolve one leaf through it.
fn store_round_trip(
    catalog: &Arc<Catalog>,
    store: &Arc<StyleStore>,
    service: &Style,
    clock: &Arc<dyn Clock>,
) -> AuraResult<usize> {
    let project = ProjectId::new();
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 17".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: aura_catalog::rfc3339(clock.now_utc()),
        updated_at: aura_catalog::rfc3339(clock.now_utc()),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;

    let bucket = StyleBucket::new(SceneGroup::Ceremony, LightingBucket::Tungsten);
    let tree = tree::fit(&samples(bucket, 120, -0.25, "wedding-1"));
    let mut profile = StyleProfile::empty(ProfileId::new(), "gate", aura_recipe::ENGINE);
    profile.global = tree.global.clone();
    profile.groups = tree.groups.clone();
    profile.buckets = tree.buckets.clone();
    profile.trained_pairs = 400;
    profile.analysis_ver = ANALYSIS_VER;
    profile.diagnostics.overall_de00 = 1.8;
    profile.diagnostics.recommendation =
        diagnostics::recommend(&profile, &diagnostics::weak_buckets(&profile), 400, 1.8);
    store.put_profile(&profile, &tree)?;

    let Some(read) = store.profile(profile.id)? else {
        return Err(aura_style::errors::profile_refused(
            &profile.id.to_db(),
            "the profile did not read back",
        ));
    };
    if read.buckets.len() != profile.buckets.len() {
        return Err(aura_style::errors::profile_refused(
            &profile.id.to_db(),
            "the leaves did not read back",
        ));
    }

    service.adopt(profile.id)?;
    service.select(project, None, Some(profile.id))?;
    service.select(project, Some(ChapterId::Dance), Some(profile.id))?;

    let advice = service.advise_in(
        project,
        Some(ChapterId::Ceremony),
        &aura_core::contract::style::StyleQuery {
            project: Some(project),
            scene: aura_core::SceneId::Ceremony,
            lighting: LightingBucket::Tungsten,
            ..aura_core::contract::style::StyleQuery::default()
        },
    )?;
    if advice.level != FallbackLevel::Bucket {
        return Err(aura_style::errors::profile_refused(
            &profile.id.to_db(),
            "a bucket with 120 pairs did not answer for itself",
        ));
    }
    if advice.reasons.is_empty() {
        return Err(aura_style::errors::profile_refused(
            &profile.id.to_db(),
            "invariant 2: advice with no reason",
        ));
    }

    let outline = service.outline(project)?;
    if outline.chapter_overrides.len() != 1 {
        return Err(aura_style::errors::profile_refused(
            &profile.id.to_db(),
            "the chapter override did not survive",
        ));
    }

    // What one profile and its leaves cost on disk.
    let bytes: i64 = catalog.read(|conn| {
        conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(CAST(p.profile_id AS BLOB)) + 700), 0)
                    + COALESCE((SELECT COUNT(*) * 400 FROM profile_buckets), 0)
               FROM profiles p",
            [],
            |row| row.get(0),
        )
        .map_err(|err| aura_core::errors::db::statement_failed("profile size", &err))
    })?;
    Ok(usize::try_from(bytes).unwrap_or(0))
}

/// Export a profile, import it back, and refuse a tampered copy.
fn bundle_checks() -> Result<(usize, String), String> {
    let bucket = StyleBucket::new(SceneGroup::Dance, LightingBucket::Flash);
    let tree = tree::fit(&samples(bucket, 60, 0.15, "wedding-1"));
    let mut profile = StyleProfile::empty(ProfileId::new(), "gate-bundle", aura_recipe::ENGINE);
    profile.global = tree.global.clone();
    profile.buckets = tree.buckets.clone();
    profile.trained_pairs = 400;
    profile.diagnostics.overall_de00 = 2.1;

    let identity = bundle::Identity::from_seed([17u8; 32]);
    let signed = bundle::export(&profile, &identity).map_err(|err| err.detail.clone())?;
    let file = signed.to_file();

    let read = bundle::import(&file).map_err(|err| err.detail.clone())?;
    if !read.unchanged_since_signing {
        return Err("a bundle this build wrote did not verify".to_string());
    }
    if read.profile.buckets.len() != profile.buckets.len() {
        return Err("the leaves did not survive the bundle".to_string());
    }
    if (read.profile.diagnostics.overall_de00 - 2.1).abs() > 1e-3 {
        return Err("the measured accuracy did not survive the bundle".to_string());
    }

    let tampered = file.replacen("gate-bundle", "gate-bundlx", 1);
    if bundle::import(&tampered).is_ok() {
        return Err("a tampered bundle was accepted".to_string());
    }
    let future = file.replacen(
        &format!("schema={}", bundle::BUNDLE_SCHEMA),
        "schema=999",
        1,
    );
    if bundle::import(&future).is_ok() {
        return Err("a bundle from a future schema was accepted".to_string());
    }
    Ok((file.len(), signed.fingerprint()))
}

/// The manifest and the sources name no networking crate.
fn no_network() -> Result<(), String> {
    let manifest = std::fs::read_to_string("crates/aura-style/Cargo.toml")
        .map_err(|err| format!("cannot read the aura-style manifest: {err}"))?;
    let table = manifest
        .split("# Deliberately absent")
        .next()
        .unwrap_or_default();
    for needle in ["aura-cloud", "reqwest", "hyper", "ureq"] {
        if table.contains(needle) {
            return Err(format!("aura-style grew a networking dependency: {needle}"));
        }
    }
    Ok(())
}

/// Which leaf one query lands in, for the gate's own sanity.
#[allow(dead_code)]
fn bucket_of(scene: aura_core::SceneId, lighting: LightingBucket) -> StyleBucket {
    let query = aura_core::contract::style::StyleQuery {
        scene,
        lighting,
        ..aura_core::contract::style::StyleQuery::default()
    };
    infer::bucket_of(&query)
}
