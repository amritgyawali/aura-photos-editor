//! Headless driver for AURA. Everything the UI can do, the CLI can do, which is
//! what makes CI able to prove the phase gate without a screen.
//!
//! Panics are permitted in this file only: it is a binary entry point, and a bad
//! command line should end the process loudly.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_ingest::fixtures;

mod phase03;
mod phase04;
mod phase05;
mod phase06;
mod phase07;
mod phase08;
mod phase09;
mod phase10;

pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fixtures") => cmd_fixtures(&args),
        Some("raw-fixtures") => cmd_raw_fixtures(&args),
        Some("import") => cmd_import(&args),
        Some("previews") => cmd_previews(&args),
        Some("verify") => match flag(&args, "--phase").as_deref() {
            Some("02") => cmd_verify_previews(&args),
            Some("03") => phase03::verify(&args),
            Some("04") => phase04::verify(&args),
            Some("05") => phase05::verify(&args),
            Some("06") => phase06::verify(&args),
            Some("07") => phase07::verify(&args),
            Some("08") => phase08::verify(&args),
            Some("09") => phase09::verify(&args),
            Some("10") => phase10::verify(&args),
            _ => cmd_verify(&args),
        },
        Some("infer") => phase03::infer(&args),
        Some("info") => cmd_info(&args),
        _ => {
            eprintln!(
                "usage:\n  \
                 aura-cli fixtures --out DIR\n  \
                 aura-cli raw-fixtures --out DIR\n  \
                 aura-cli import --catalog FILE --project NAME --root DIR [--root DIR]\n  \
                 aura-cli previews --catalog FILE --project NAME [--level thumb|proxy]\n  \
                 aura-cli verify [--phase 01|02|03|04] --work DIR\n  \
                 aura-cli infer --model FILE [--precision fp32|fp16|int8] [--batch N]\n  \
                 aura-cli info --catalog FILE"
            );
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn flag(args: &[String], name: &str) -> Option<String> {
    let index = args.iter().position(|a| a == name)?;
    args.get(index + 1).cloned()
}

fn flags(args: &[String], name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        if arg == name {
            if let Some(value) = args.get(index + 1) {
                out.push(value.clone());
            }
        }
    }
    out
}

fn cmd_fixtures(args: &[String]) -> ExitCode {
    let out =
        PathBuf::from(flag(args, "--out").unwrap_or_else(|| "tests/fixtures/generated".into()));
    match fixtures::generate_all(&out) {
        Ok(weddings) => {
            for wedding in weddings {
                println!(
                    "{} -> {} files, {} photos",
                    wedding.root.display(),
                    wedding.files,
                    wedding.photos
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fixtures failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn open_catalog(path: &Path) -> Result<(Catalog, Arc<dyn Clock>), String> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Catalog::open(path, Arc::clone(&clock), APP_VERSION)
        .map_err(|e| format!("[{}] {}", e.code, e.user_message))?;
    Ok((catalog, clock))
}

fn cmd_import(args: &[String]) -> ExitCode {
    let Some(catalog_path) = flag(args, "--catalog").map(PathBuf::from) else {
        eprintln!("--catalog is required");
        return ExitCode::FAILURE;
    };
    let roots: Vec<PathBuf> = flags(args, "--root")
        .into_iter()
        .map(PathBuf::from)
        .collect();
    if roots.is_empty() {
        eprintln!("at least one --root is required");
        return ExitCode::FAILURE;
    }
    let name = flag(args, "--project").unwrap_or_else(|| "Untitled wedding".to_string());

    let (catalog, clock) = match open_catalog(&catalog_path) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let project_id = match ensure_project(&catalog, clock.as_ref(), &name) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let plan = ImportPlan {
        import_id: ImportId::new(),
        project_id,
        roots,
        mode: ImportMode::Reference,
        extensions: Vec::new(),
        extract_embedded_previews: false,
        settle_window_ms: 0,
    };

    match aura_ingest::run(&catalog, &plan, &CancelToken::new(), &NullProgress) {
        Ok(report) => {
            println!("discovered      {}", report.files_discovered);
            println!("imported        {}", report.files_imported);
            println!("already present {}", report.files_already_present);
            println!("photos created  {}", report.photos_created);
            println!("quarantined     {}", report.files_quarantined);
            println!("bytes hashed    {}", report.bytes_hashed);
            println!("duration ms     {}", report.duration_ms);
            for (code, count) in &report.quarantine_by_code {
                println!("  {code}: {count}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[{}] {}", e.code, e.user_message);
            eprintln!("detail: {}", e.detail);
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn ensure_project(
    catalog: &Catalog,
    clock: &dyn Clock,
    name: &str,
) -> Result<ProjectId, String> {
    let existing = catalog
        .read(repo::list_projects)
        .map_err(|e| format!("[{}] {}", e.code, e.user_message))?;

    if let Some(found) = existing.iter().find(|p| p.name == name) {
        return ProjectId::from_db(&found.project_id).map_err(|e| e.to_string());
    }

    let project_id = ProjectId::new();
    let now = rfc3339(clock.now_utc());
    let row = ProjectRow {
        project_id: project_id.to_db(),
        name: name.to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    catalog
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))
        .map_err(|e| format!("[{}] {}", e.code, e.user_message))?;
    Ok(project_id)
}

/// Generate fixtures, import them twice, and prove the second import is a no-op.
fn cmd_verify(args: &[String]) -> ExitCode {
    let work =
        PathBuf::from(flag(args, "--work").unwrap_or_else(|| "target/phase01-verify".into()));
    if let Err(e) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {e}", work.display());
        return ExitCode::FAILURE;
    }

    let fixture_dir = work.join("fixtures");
    let weddings = match fixtures::generate_all(&fixture_dir) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("fixtures failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut failures = 0;
    for wedding in weddings {
        let slug = wedding
            .root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "wedding".to_string());
        let catalog_path = work.join(format!("{slug}.sqlite"));
        let _ = std::fs::remove_file(&catalog_path);

        let (catalog, clock) = match open_catalog(&catalog_path) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("{slug}: {e}");
                failures += 1;
                continue;
            }
        };

        let project_id = match ensure_project(&catalog, clock.as_ref(), &slug) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("{slug}: {e}");
                failures += 1;
                continue;
            }
        };

        let plan = |import_id: ImportId| ImportPlan {
            import_id,
            project_id,
            roots: vec![wedding.root.clone()],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        };

        let first = match aura_ingest::run(
            &catalog,
            &plan(ImportId::new()),
            &CancelToken::new(),
            &NullProgress,
        ) {
            Ok(report) => report,
            Err(e) => {
                eprintln!("{slug}: first import failed [{}] {}", e.code, e.detail);
                failures += 1;
                continue;
            }
        };

        let digest_a = catalog
            .read(|conn| repo::catalog_digest(conn, &project_id.to_db()))
            .unwrap_or_default();

        let second = match aura_ingest::run(
            &catalog,
            &plan(ImportId::new()),
            &CancelToken::new(),
            &NullProgress,
        ) {
            Ok(report) => report,
            Err(e) => {
                eprintln!("{slug}: second import failed [{}] {}", e.code, e.detail);
                failures += 1;
                continue;
            }
        };

        let digest_b = catalog
            .read(|conn| repo::catalog_digest(conn, &project_id.to_db()))
            .unwrap_or_default();

        let photos = catalog.count("photo").unwrap_or(-1);
        let files = catalog.count("photo_file").unwrap_or(-1);
        let cameras = catalog.count("camera").unwrap_or(-1);

        println!("{slug}: photos={photos} files={files} cameras={cameras}");
        println!(
            "  first : imported={} photos={}",
            first.files_imported, first.photos_created
        );
        println!(
            "  second: imported={} hashed_bytes={}",
            second.files_imported, second.bytes_hashed
        );
        println!("  digest: {digest_a}");

        if second.files_imported != 0 || second.photos_created != 0 || second.bytes_hashed != 0 {
            eprintln!("{slug}: re-import was not a no-op");
            failures += 1;
        }
        if digest_a != digest_b {
            eprintln!("{slug}: catalog digest changed across a re-import");
            failures += 1;
        }
        if let Err(e) = catalog.integrity_check() {
            eprintln!("{slug}: integrity check failed [{}] {}", e.code, e.detail);
            failures += 1;
        }
    }

    if failures == 0 {
        println!("phase-01 verify: all fixtures clean");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase-01 verify: {failures} failures");
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------------- PHASE 02

/// Write the synthetic RAW bench set: eight bodies, three mosaic encodings, one
/// deliberately corrupt file so the quarantine path is exercised too.
fn cmd_raw_fixtures(args: &[String]) -> ExitCode {
    let out = PathBuf::from(flag(args, "--out").unwrap_or_else(|| "tests/fixtures/raw".into()));
    match write_raw_fixtures(&out) {
        Ok(count) => {
            println!("{count} synthetic RAW files written to {}", out.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("raw fixtures failed: {e}");
            ExitCode::FAILURE
        }
    }
}

/// One file per bench body, cycling the mosaic encodings, plus a poison file.
fn write_raw_fixtures(out: &Path) -> Result<usize, String> {
    use aura_raw::fixtures::{write_bench_raw, FixtureOptions, MosaicEncoding};

    std::fs::create_dir_all(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    // Every encoding the decoder claims to read, cycled across the bodies so a
    // single gate run exercises all of them.
    let encodings = [
        MosaicEncoding::Unpacked16,
        MosaicEncoding::Packed14,
        MosaicEncoding::LosslessJpeg,
        MosaicEncoding::NikonCompressed,
        MosaicEncoding::SonyArw2,
        MosaicEncoding::OlympusCompressed,
        MosaicEncoding::XTrans16,
    ];

    let mut written = 0usize;
    for body in 1..=8usize {
        let encoding = encodings[(body - 1) % encodings.len()];
        let options = FixtureOptions {
            body,
            encoding,
            with_preview: true,
            // A quarter of the set is rotated, so orientation is exercised by
            // the gate rather than only by the unit tests.
            orientation: if body % 4 == 0 { 6 } else { 1 },
            with_colour_matrix: body % 3 != 0,
            ..FixtureOptions::default()
        };
        let path = out.join(format!("bench-{body:02}-{}.dng", encoding.as_str()));
        write_bench_raw(&path, options)?;
        written += 1;
    }

    // A file with a RAW extension that is not a RAW at all. Every phase-02 run
    // must set it aside and finish the rest.
    let poison = out.join("poison.dng");
    std::fs::write(&poison, b"AURA phase-02 poison fixture: not a photograph")
        .map_err(|e| format!("write {}: {e}", poison.display()))?;
    written += 1;

    Ok(written)
}

/// Build previews for every photograph in a project.
fn cmd_previews(args: &[String]) -> ExitCode {
    let Some(catalog_path) = flag(args, "--catalog").map(PathBuf::from) else {
        eprintln!("--catalog is required");
        return ExitCode::FAILURE;
    };
    let name = flag(args, "--project").unwrap_or_else(|| "Untitled wedding".to_string());
    let level = match flag(args, "--level").as_deref() {
        Some("proxy") => aura_raw::PixelLevel::Proxy2048,
        _ => aura_raw::PixelLevel::Thumb(512),
    };

    let (catalog, clock) = match open_catalog(&catalog_path) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let project_id = match ensure_project(&catalog, clock.as_ref(), &name) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let cache_root = catalog_path
        .parent()
        .map_or_else(|| PathBuf::from("cache"), |parent| parent.join("cache"));
    match build_previews(
        std::sync::Arc::new(catalog),
        &project_id.to_db(),
        &cache_root,
        level,
    ) {
        Ok(report) => {
            println!("built    {}", report.built);
            println!("failed   {}", report.failed);
            println!(
                "cache    {} bytes, {} entries",
                report.bytes, report.entries
            );
            println!("hit rate {:.1}%", report.hit_rate * 100.0);
            println!("elapsed  {} ms", report.elapsed_ms);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

struct PreviewReport {
    built: usize,
    failed: usize,
    bytes: u64,
    entries: u64,
    hit_rate: f64,
    elapsed_ms: u64,
    misses: u64,
    problems: Vec<(String, String)>,
}

fn build_previews(
    catalog: std::sync::Arc<Catalog>,
    project_id: &str,
    cache_root: &Path,
    level: aura_raw::PixelLevel,
) -> Result<PreviewReport, String> {
    use aura_preview::contract::service::PreviewService;
    use aura_preview::{CatalogSource, PreviewConfig, PreviewSource, Previews, Priority};

    let started = catalog.clock().monotonic_ms();
    let source: std::sync::Arc<dyn PreviewSource> = std::sync::Arc::new(CatalogSource::new(
        std::sync::Arc::clone(&catalog),
        project_id.to_string(),
    ));
    let service = Previews::open(
        cache_root,
        aura_cache::CacheBudget::default(),
        source,
        PreviewConfig::default(),
    )
    .map_err(|e| format!("[{}] {}", e.code, e.detail))?;

    // Hit and miss counters are lifetime figures that survive in the on-disk
    // index, so a pass is measured as a delta rather than as an absolute.
    let before = service.cache_stats();

    let photos = catalog
        .read(|conn| repo::list_photos(conn, project_id, 0, 100_000, repo::PhotoOrder::Timeline))
        .map_err(|e| format!("[{}] {}", e.code, e.detail))?;

    let mut built = 0usize;
    let mut failed = 0usize;
    for row in photos {
        let Ok(id) = aura_core::PhotoId::from_db(&row.id) else {
            continue;
        };
        match service.get(id, level, Priority::AiBatch) {
            Ok(_) => built += 1,
            Err(error) => {
                failed += 1;
                tracing::warn!(target: "previews", code = error.code.0, detail = error.detail, "decode failed");
            }
        }
    }

    let stats = service.cache_stats();
    let hits = stats.hits.saturating_sub(before.hits);
    let misses = stats.misses.saturating_sub(before.misses);
    #[allow(clippy::cast_precision_loss)]
    let hit_rate = if hits + misses == 0 {
        0.0
    } else {
        hits as f64 / (hits + misses) as f64
    };
    Ok(PreviewReport {
        built,
        failed,
        bytes: stats.bytes_used,
        entries: stats.entries,
        hit_rate,
        elapsed_ms: catalog.clock().monotonic_ms().saturating_sub(started),
        misses,
        problems: service
            .quarantine()
            .into_iter()
            .map(|(id, reason)| (id.to_db(), reason))
            .collect(),
    })
}

/// The phase 02 gate: generate RAW fixtures, import them, build both cached
/// tiers, prove the second pass costs nothing, and measure the colour chart.
#[allow(clippy::too_many_lines)]
fn cmd_verify_previews(args: &[String]) -> ExitCode {
    let work =
        PathBuf::from(flag(args, "--work").unwrap_or_else(|| "target/phase02-verify".into()));
    if let Err(e) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {e}", work.display());
        return ExitCode::FAILURE;
    }

    let fixture_dir = work.join("raw");
    let _ = std::fs::remove_dir_all(&fixture_dir);
    let fixture_count = match write_raw_fixtures(&fixture_dir) {
        Ok(count) => count,
        Err(e) => {
            eprintln!("raw fixtures failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let catalog_path = work.join("phase02.sqlite");
    let _ = std::fs::remove_file(&catalog_path);
    let cache_root = work.join("cache");
    let _ = std::fs::remove_dir_all(&cache_root);

    let (catalog, clock) = match open_catalog(&catalog_path) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let project_id = match ensure_project(&catalog, clock.as_ref(), "phase-02") {
        Ok(id) => id,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let plan = ImportPlan {
        import_id: ImportId::new(),
        project_id,
        roots: vec![fixture_dir.clone()],
        mode: ImportMode::Reference,
        extensions: Vec::new(),
        extract_embedded_previews: true,
        settle_window_ms: 0,
    };
    let import = match aura_ingest::run(&catalog, &plan, &CancelToken::new(), &NullProgress) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("import failed: [{}] {}", e.code, e.detail);
            return ExitCode::FAILURE;
        }
    };

    let catalog = std::sync::Arc::new(catalog);
    let project = project_id.to_db();
    let mut failures = 0usize;

    println!(
        "fixtures: {fixture_count} files, imported {}",
        import.files_imported
    );

    // Pass one: build both tiers.
    let mut first_pass_ms = 0u64;
    for level in [
        aura_raw::PixelLevel::Thumb(512),
        aura_raw::PixelLevel::Proxy2048,
    ] {
        match build_previews(
            std::sync::Arc::clone(&catalog),
            &project,
            &cache_root,
            level,
        ) {
            Ok(report) => {
                first_pass_ms += report.elapsed_ms;
                println!(
                    "tier {}: built={} failed={} cache={} bytes elapsed={} ms",
                    level.tier(),
                    report.built,
                    report.failed,
                    report.bytes,
                    report.elapsed_ms
                );
                if report.failed != 1 {
                    eprintln!(
                        "expected exactly one undecodable fixture, saw {}",
                        report.failed
                    );
                    failures += 1;
                }
                for (photo, reason) in &report.problems {
                    println!("  quarantined {photo}: {reason}");
                }
            }
            Err(e) => {
                eprintln!("{e}");
                failures += 1;
            }
        }
    }

    // Pass two: everything must come from the cache.
    match build_previews(
        std::sync::Arc::clone(&catalog),
        &project,
        &cache_root,
        aura_raw::PixelLevel::Thumb(512),
    ) {
        Ok(report) => {
            println!(
                "second pass: built={} hit_rate={:.1}% elapsed={} ms",
                report.built,
                report.hit_rate * 100.0,
                report.elapsed_ms
            );
            // One miss is expected: the poison file has no cache entry to hit.
            if report.misses > 1 {
                eprintln!("second pass missed the cache {} times", report.misses);
                failures += 1;
            }
        }
        Err(e) => {
            eprintln!("{e}");
            failures += 1;
        }
    }

    // Every photograph that decoded must now have both preview rows.
    for tier in [1i64, 2] {
        let missing = catalog
            .read(|conn| repo::photos_without_preview(conn, &project, tier, 1_000))
            .unwrap_or_default();
        // The poison file still becomes a photo row, so exactly one is expected.
        if missing.len() > 1 {
            eprintln!("{} photographs have no tier {tier} preview", missing.len());
            failures += 1;
        }
        let built = catalog
            .read(|conn| repo::count_previews(conn, &project, tier))
            .unwrap_or(-1);
        println!("tier {tier} preview rows: {built}");
    }

    // Colour: the chart must survive the pipeline on every bench body.
    match verify_colour(&fixture_dir) {
        Ok(worst) => println!("colour: worst mean dE2000 {worst:.3} across 8 bodies"),
        Err(e) => {
            eprintln!("colour check failed: {e}");
            failures += 1;
        }
    }

    println!("first pass total: {first_pass_ms} ms");

    if failures == 0 {
        println!("phase-02 verify: all fixtures clean");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase-02 verify: {failures} failures");
        ExitCode::FAILURE
    }
}

/// Render each bench body's chart and compare it with what was written.
fn verify_colour(fixture_dir: &Path) -> Result<f64, String> {
    use aura_raw::colour::de2000::{summarise, xyz_d65_to_lab};
    use aura_raw::colour::matrix::{apply, SRGB_TO_XYZ_D65};
    use aura_raw::colour::working_space::working_to_xyz;
    use aura_raw::fixtures::{write_bench_raw, FixtureOptions, MosaicEncoding};

    let clock = aura_core::clock::SystemClock::default();
    // Every encoding the decoder claims to read, cycled across the bodies so a
    // single gate run exercises all of them.
    let encodings = [
        MosaicEncoding::Unpacked16,
        MosaicEncoding::Packed14,
        MosaicEncoding::LosslessJpeg,
        MosaicEncoding::NikonCompressed,
        MosaicEncoding::SonyArw2,
        MosaicEncoding::OlympusCompressed,
        MosaicEncoding::XTrans16,
    ];
    let mut worst = 0.0f64;

    for body in 1..=8usize {
        let encoding = encodings[(body - 1) % encodings.len()];
        let path = fixture_dir.join(format!("colour-{body:02}.dng"));
        let fixture = write_bench_raw(
            &path,
            FixtureOptions {
                body,
                encoding,
                ..FixtureOptions::default()
            },
        )?;
        let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let meta = aura_raw::read_meta(&bytes, &path).map_err(|e| e.detail.clone())?;
        let rendered = aura_raw::proxy::tier2(
            &bytes,
            &meta,
            aura_raw::DecodeLimits::tier2(),
            &clock,
            None,
            &path,
        )
        .map_err(|e| e.detail.clone())?;

        let samples = rendered
            .pair
            .linear
            .as_linear16()
            .ok_or_else(|| "the linear buffer is missing".to_string())?;
        let width = rendered.pair.linear.width as usize;

        // A Bayer proxy is half the sensor; an X-Trans proxy is a third.
        let divisor = encoding.cfa().block();
        let measured: Vec<_> = (0..fixture.patch_count())
            .map(|index| {
                let (x, y) = fixture.patch_centre(index, divisor);
                let at = (y as usize * width + x as usize) * 3;
                let channel = |offset: usize| {
                    f64::from(aura_raw::colour::curve::linear_u16_to_scene(
                        samples.get(at + offset).copied().unwrap_or(0),
                    ))
                };
                xyz_d65_to_lab(working_to_xyz([channel(0), channel(1), channel(2)]))
            })
            .collect();
        let expected: Vec<_> = fixture
            .expected_linear_srgb
            .iter()
            .map(|patch| xyz_d65_to_lab(apply(SRGB_TO_XYZ_D65, *patch)))
            .collect();

        let delta = summarise(&expected, &measured);
        worst = worst.max(delta.mean);
        if delta.mean > 2.0 {
            return Err(format!(
                "{}: mean dE2000 {:.3} exceeds the 2.0 tolerance",
                fixture.model, delta.mean
            ));
        }
        // The colour fixtures are working files, not part of the imported set.
        let _ = std::fs::remove_file(&path);
    }
    Ok(worst)
}

fn cmd_info(args: &[String]) -> ExitCode {
    let Some(path) = flag(args, "--catalog").map(PathBuf::from) else {
        eprintln!("--catalog is required");
        return ExitCode::FAILURE;
    };
    let (catalog, _clock) = match open_catalog(&path) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    println!("schema version {}", catalog.schema_version().unwrap_or(-1));
    for table in [
        "project",
        "camera",
        "photo",
        "photo_file",
        "quarantine",
        "task",
    ] {
        println!("{table:<12} {}", catalog.count(table).unwrap_or(-1));
    }
    ExitCode::SUCCESS
}
