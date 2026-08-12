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

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

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
        Some("import") => cmd_import(&args),
        Some("verify") => cmd_verify(&args),
        Some("info") => cmd_info(&args),
        _ => {
            eprintln!(
                "usage:\n  \
                 aura-cli fixtures --out DIR\n  \
                 aura-cli import --catalog FILE --project NAME --root DIR [--root DIR]\n  \
                 aura-cli verify --work DIR\n  \
                 aura-cli info --catalog FILE"
            );
            ExitCode::FAILURE
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
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

fn ensure_project(catalog: &Catalog, clock: &dyn Clock, name: &str) -> Result<ProjectId, String> {
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
