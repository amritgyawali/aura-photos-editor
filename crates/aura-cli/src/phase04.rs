//! The phase 04 gate.
//!
//! `aura-cli verify --phase 04` proves, on this machine, in one run, everything
//! section 13 of the phase document asks for that can be proved without a human:
//! the migration lands, a key round-trips through a store without ever entering
//! `argv`, a payload carries no original pixels, a wedding's worth of segments
//! completes with the network unplugged, a cap stops spending without stopping
//! the gallery, a re-run is answered from the cache with identical decisions, a
//! malformed answer is repaired once and then abandoned, and nothing anywhere in
//! the trail is key-shaped.
//!
//! It is deliberately a separate binary path from the tests, for the same reason
//! the phase 03 gate is: the tests prove the pieces, the gate proves the
//! assembly, with the same code CI runs and a human reads the output of.
//!
//! It never opens a socket. The transport is a cassette or the offline refusal,
//! which is acceptance criterion 7.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::consent::{AlwaysConsent, CatalogConsent};
use aura_catalog::Catalog;
use aura_cloud::anthropic::AnthropicProvider;
use aura_cloud::audit::{AuditSink, CatalogAudit};
use aura_cloud::budget::{BudgetStore, CatalogBudget, CostGovernor};
use aura_cloud::cache::{CatalogCache, ResponseCache};
use aura_cloud::cassette::CassetteTransport;
use aura_cloud::contract::cloud::{ImagePart, Source};
use aura_cloud::keys::{
    KeyStore, MemoryKeyStore, OsKeyStore, Platform, RecordingRunner, SecretKey,
};
use aura_cloud::payload::{self, PayloadPolicy, SourceImage};
use aura_cloud::provider::{
    OfflineTransport, ProviderClient, RecordingSleeper, RetryPolicy, Transport,
};
use aura_cloud::redact::{self, ExifSummary};
use aura_cloud::tasks::{SceneGuess, Scored, SegmentInput, SegmentNaming};
use aura_cloud::{CallContext, CloudAiGateway, CloudPolicy};
use aura_core::clock::{Clock, SystemClock};
use aura_core::progress::CancelToken;
use aura_core::ProjectId;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};

/// The key the gate stores. Fake, and shaped like a real one on purpose: the
/// redaction check has to have something to find.
const TEST_KEY: &str = "sk-ant-api03-PHASE04GATE0000000000000000000000000000AA";

/// Section 11: gateway overhead excluding provider latency, per call.
///
/// Measured on the development machine, and **multiplied by `aura_perf::host_scale`** for the
/// reason phase 14's proxy guardrail is: a timing budget is a statement about hardware as well
/// as about code, and a guardrail that is red on every run of every slower host is a guardrail
/// nobody reads. See `crates/aura-perf/src/lib.rs`.
///
/// This row did not honour the scale until the post-review pass, which is why the phases 01 to
/// 30 review recorded phase 14 as the only host-sensitive gate: the container it ran on happened
/// to clear 15 ms on this path and the reference Windows machine measures 66.7 ms. The gap is
/// the SQLite round trip the cache does, and it is disk speed rather than arithmetic. Sizes,
/// counts and costs are never scaled - a byte is a byte and a cent is a cent on any host.
const OVERHEAD_BUDGET_MS: f64 = 15.0;

/// The overhead budget this host is held to, in milliseconds.
fn overhead_budget_ms() -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let scale = aura_perf::host_scale() as f64;
    OVERHEAD_BUDGET_MS * scale
}

/// Section 11: cache hit rate on a re-run.
const CACHE_HIT_BUDGET: f64 = 0.70;

/// Section 6.1: at most one call per this many images.
const IMAGES_PER_CALL: u32 = 40;

/// Run the phase 04 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase04-verify".into()),
    );
    let cassettes = PathBuf::from(
        crate::flag(args, "--cassettes").unwrap_or_else(|| "tests/cloud/cassettes".into()),
    );

    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. The catalog reaches schema 4 and has the three cloud tables.
    let catalog_path = work.join("phase04.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(catalog) => Arc::new(catalog),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    // At LEAST 4, not exactly 4. A later phase's migration adds tables and removes
    // none, so a catalog at schema 7 still satisfies everything phase 04 asserts - and
    // an equality check here would fail this gate on every future phase, which is a
    // false alarm nobody would act on twice.
    match catalog.schema_version() {
        Ok(version) if version >= 4 => println!("migration: catalog at schema {version}"),
        Ok(other) => {
            eprintln!("migration: catalog is at schema {other}, expected at least 4");
            failures += 1;
        }
        Err(err) => {
            eprintln!("migration: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for table in ["cloud_calls", "cloud_cache", "cloud_budget"] {
        if let Err(err) = catalog.count(table) {
            eprintln!(
                "migration: {table} is missing - [{}] {}",
                err.code, err.detail
            );
            failures += 1;
        }
    }
    if !migration_documents_its_rollback() {
        eprintln!("migration: the rollback statements are not recorded in the migration file");
        failures += 1;
    }

    let project = match crate::ensure_project(&catalog, clock.as_ref(), "phase-04") {
        Ok(project) => project,
        Err(message) => {
            eprintln!("catalog: {message}");
            return ExitCode::FAILURE;
        }
    };

    // 2. A key round-trips, and never appears in argv on any platform.
    match key_safety() {
        Ok(line) => println!("{line}"),
        Err(message) => {
            eprintln!("{message}");
            failures += 1;
        }
    }

    // 3. The payload carries no original pixels.
    let sheet = match build_sheet(12) {
        Ok(sheet) => sheet,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "payload: 12 tiles, {}x{}, {} KB, sha {}",
        sheet.width,
        sheet.height,
        sheet.bytes.len() / 1024,
        sheet.content_hash.get(..12).unwrap_or_default()
    );
    if sheet.width.max(sheet.height) > payload::MAX_SHEET_LONG_EDGE {
        eprintln!("payload: the sheet is over the long-edge ceiling");
        failures += 1;
    }
    if let Some(found) = raw_magic_in(&sheet.bytes) {
        eprintln!("payload: the sheet carries {found} container magic");
        failures += 1;
    }

    // 4. A whole wedding's segments, answered from cassettes.
    let transport = match CassetteTransport::from_dir(&cassettes) {
        Ok(loaded) => Arc::new(loaded),
        Err(err) => {
            eprintln!("cassettes: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!("cassettes: {} recordings loaded", transport.len());

    let keys: Arc<dyn KeyStore> = Arc::new(MemoryKeyStore::with("api-key:anthropic", TEST_KEY));
    let gateway = build_gateway(
        &catalog,
        &clock,
        Arc::clone(&transport) as Arc<dyn Transport>,
        Arc::clone(&keys),
        CloudPolicy::enabled(),
        true,
    );
    let task = SegmentNaming::for_sheet(sheet.clone());
    let cancel = CancelToken::new();

    let started = clock.monotonic_us();
    let first = gateway.run(
        &task,
        &segment("seg-mehndi", "candid", 410, 12),
        &CallContext {
            project: &project,
            decision_ref: Some("seg-mehndi"),
            cancel: &cancel,
        },
    );
    let first_us = clock.monotonic_us().saturating_sub(started);
    match &first {
        Ok(result) if result.source == Source::Cloud => println!(
            "cloud: {} / {} at {:.2}, {} tokens in, {} out, ${:.4}",
            result.value.scene,
            result.value.ritual.clone().unwrap_or_else(|| "-".into()),
            result.value.confidence(),
            result.tokens_in,
            result.tokens_out,
            result.cost_usd
        ),
        Ok(result) => {
            eprintln!("cloud: expected a provider answer, got {}", result.source);
            failures += 1;
        }
        Err(err) => {
            eprintln!("cloud: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. The rest of the first pass: a segment whose first answer is malformed
    //    and is repaired in one round trip.
    match gateway.run(
        &task,
        &segment("seg-ring-exchange", "ceremony", 600, 8),
        &CallContext {
            project: &project,
            decision_ref: Some("seg-ring"),
            cancel: &cancel,
        },
    ) {
        Ok(result) if result.source == Source::Cloud => {
            println!("repair: one repair round trip recovered a schema-valid answer");
        }
        Ok(result) => {
            eprintln!(
                "repair: expected a repaired cloud answer, got {}",
                result.source
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("repair: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 6. Re-running the wedding. Section 11's ">= 70 % cache hit rate on a
    //    re-run" is a property of the *second* run, not of the two runs
    //    together, so the second pass gets its own gateway over the same
    //    catalog - which is exactly what re-opening the project does.
    let rerun = build_gateway(
        &catalog,
        &clock,
        Arc::clone(&transport) as Arc<dyn Transport>,
        Arc::clone(&keys),
        CloudPolicy::enabled(),
        true,
    );
    let cache_started = clock.monotonic_us();
    let second = rerun.run(
        &task,
        &segment("seg-mehndi", "candid", 410, 12),
        &CallContext {
            project: &project,
            decision_ref: Some("seg-mehndi"),
            cancel: &cancel,
        },
    );
    let cache_us = clock.monotonic_us().saturating_sub(cache_started);
    match (&first, &second) {
        (Ok(first), Ok(second)) if second.source == Source::Cache => {
            if first.value == second.value {
                println!(
                    "cache: the re-run produced an identical decision from the cache in {:.1} ms",
                    cache_us as f64 / 1000.0
                );
            } else {
                eprintln!("cache: the cached decision differs from the one that was cached");
                failures += 1;
            }
        }
        (_, Ok(second)) => {
            eprintln!("cache: expected a cache hit, got {}", second.source);
            failures += 1;
        }
        (_, Err(err)) => {
            eprintln!("cache: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    // The rest of the re-run: every segment the first pass answered, asked again.
    drop(rerun.run(
        &task,
        &segment("seg-ring-exchange", "ceremony", 600, 8),
        &CallContext {
            project: &project,
            decision_ref: None,
            cancel: &cancel,
        },
    ));
    let hit_rate = rerun.ledger().cache_hit_rate();
    println!(
        "cache: re-run hit rate {:.0}% against a {:.0}% budget",
        hit_rate * 100.0,
        CACHE_HIT_BUDGET * 100.0
    );
    if hit_rate < CACHE_HIT_BUDGET {
        eprintln!("cache: under the budget");
        failures += 1;
    }

    // The cache path is the gateway's own cost with no provider in it, which is
    // what section 11's 15 ms budget measures.
    let overhead_ms = cache_us as f64 / 1000.0;
    let overhead_budget = overhead_budget_ms();
    println!(
        "overhead: {overhead_ms:.2} ms per call against a {overhead_budget:.0} ms budget \
         (first call including the payload build: {:.1} ms)",
        first_us as f64 / 1000.0
    );
    if overhead_ms > overhead_budget {
        eprintln!("overhead: over the budget");
        failures += 1;
    }

    // 7. A second failure is abandoned rather than repaired again.
    match gateway.run(
        &task,
        &segment("seg-unrepairable", "ceremony", 550, 8),
        &CallContext {
            project: &project,
            decision_ref: Some("seg-unrepairable"),
            cancel: &cancel,
        },
    ) {
        Ok(result) if result.source == Source::LocalFallback => {
            println!("repair: a second failure fell back locally rather than retrying again");
        }
        Ok(result) => {
            eprintln!("repair: expected a local fallback, got {}", result.source);
            failures += 1;
        }
        Err(err) => {
            eprintln!("repair: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 7. The network unplugged: a whole wedding, every decision local.
    let offline_project = match crate::ensure_project(&catalog, clock.as_ref(), "phase-04-offline")
    {
        Ok(project) => project,
        Err(message) => {
            eprintln!("offline: {message}");
            return ExitCode::FAILURE;
        }
    };
    let offline = build_gateway(
        &catalog,
        &clock,
        Arc::new(OfflineTransport),
        Arc::clone(&keys),
        CloudPolicy::enabled(),
        true,
    );
    let segments = 3_000 / IMAGES_PER_CALL;
    let mut offline_failures = 0usize;
    for index in 0..segments {
        let result = offline.run(
            &task,
            &segment(&format!("seg-offline-{index}"), "dancing", 480, 12),
            &CallContext {
                project: &offline_project,
                decision_ref: None,
                cancel: &cancel,
            },
        );
        match result {
            Ok(answer) if answer.source == Source::LocalFallback => {
                if answer.value.reasons().is_empty() {
                    offline_failures += 1;
                }
            }
            _ => offline_failures += 1,
        }
    }
    if offline_failures == 0 {
        println!(
            "offline: {segments} segments - a 3,000 image wedding at one call per \
             {IMAGES_PER_CALL} - completed locally, every decision with reasons"
        );
    } else {
        eprintln!("offline: {offline_failures} of {segments} segments did not complete");
        failures += 1;
    }

    // 8. A cap stops the spending without stopping the gallery.
    let capped_project = match crate::ensure_project(&catalog, clock.as_ref(), "phase-04-capped") {
        Ok(project) => project,
        Err(message) => {
            eprintln!("budget: {message}");
            return ExitCode::FAILURE;
        }
    };
    let budget_store = Arc::new(CatalogBudget::new(Arc::clone(&catalog), Arc::clone(&clock)));
    if let Err(err) = budget_store.set_cap(&capped_project.to_string(), 0.0005, true) {
        eprintln!("budget: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    let capped = build_gateway(
        &catalog,
        &clock,
        Arc::clone(&transport) as Arc<dyn Transport>,
        Arc::clone(&keys),
        CloudPolicy::enabled(),
        true,
    );
    let mut capped_complete = 0usize;
    for index in 0..10 {
        if let Ok(result) = capped.run(
            &task,
            &segment(&format!("seg-capped-{index}"), "dinner", 520, 12),
            &CallContext {
                project: &capped_project,
                decision_ref: None,
                cancel: &cancel,
            },
        ) {
            if result.source == Source::LocalFallback && result.cost_usd == 0.0 {
                capped_complete += 1;
            }
        }
    }
    if capped_complete == 10 {
        println!("budget: a job at its cap produced 10 of 10 decisions locally, billing nothing");
    } else {
        eprintln!("budget: only {capped_complete} of 10 decisions completed under the cap");
        failures += 1;
    }

    // 9. Consent withheld means nothing is sent.
    let strict = build_gateway(
        &catalog,
        &clock,
        Arc::clone(&transport) as Arc<dyn Transport>,
        Arc::clone(&keys),
        CloudPolicy::enabled(),
        false,
    );
    match strict.run(
        &task,
        &segment("seg-mehndi", "candid", 410, 12),
        &CallContext {
            project: &project,
            decision_ref: None,
            cancel: &cancel,
        },
    ) {
        Ok(result) if result.source == Source::LocalFallback => {
            println!("consent: a project without consent produced a local decision, sent nothing");
        }
        Ok(result) => {
            eprintln!("consent: expected a refusal, got {}", result.source);
            failures += 1;
        }
        Err(err) => {
            eprintln!("consent: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 10. Nothing anywhere in the trail is key-shaped.
    match key_leak_scan(&catalog, &project) {
        Ok(line) => println!("{line}"),
        Err(message) => {
            eprintln!("{message}");
            failures += 1;
        }
    }

    // 11. The audit trail can answer for every decision.
    let audit: Arc<dyn AuditSink> =
        Arc::new(CatalogAudit::new(Arc::clone(&catalog), Arc::clone(&clock)));
    match audit.recent(&project.to_string(), 100) {
        Ok(rows) => {
            let traced = rows.iter().filter(|row| !row.task.is_empty()).count();
            let spent = audit.spent_usd(&project.to_string()).unwrap_or(0.0);
            println!(
                "audit: {traced} rows for this job, ${spent:.4} billed, \
                 {} answered locally",
                rows.iter()
                    .filter(|row| row.source == Source::LocalFallback)
                    .count()
            );
            if rows.is_empty() {
                eprintln!("audit: no rows were written");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("audit: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    if failures == 0 {
        println!("phase-04 verify: all checks clean");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase-04 verify: {failures} failures");
        ExitCode::FAILURE
    }
}

/// Build a gateway over one catalog, with the transport and policy given.
fn build_gateway(
    catalog: &Arc<Catalog>,
    clock: &Arc<dyn Clock>,
    transport: Arc<dyn Transport>,
    keys: Arc<dyn KeyStore>,
    policy: CloudPolicy,
    consent: bool,
) -> CloudAiGateway {
    let client = Arc::new(
        ProviderClient::new(
            Arc::new(AnthropicProvider::default()),
            transport,
            Arc::new(RecordingSleeper::default()),
        )
        .with_policy(RetryPolicy {
            attempts: 2,
            base_ms: 1,
            max_backoff_ms: 2,
            timeout: std::time::Duration::from_millis(200),
        }),
    );

    let consent_gate: Arc<dyn aura_core::contract::consent::ConsentGate> = if consent {
        Arc::new(AlwaysConsent)
    } else {
        Arc::new(CatalogConsent::new(Arc::clone(catalog), Arc::clone(clock)))
    };

    CloudAiGateway::new(
        client,
        keys,
        Arc::new(CatalogCache::new(Arc::clone(catalog), Arc::clone(clock)))
            as Arc<dyn ResponseCache>,
        Arc::new(CatalogAudit::new(Arc::clone(catalog), Arc::clone(clock))) as Arc<dyn AuditSink>,
        Arc::new(CostGovernor::new(Arc::new(CatalogBudget::new(
            Arc::clone(catalog),
            Arc::clone(clock),
        )) as Arc<dyn BudgetStore>)),
        consent_gate,
        Arc::clone(clock),
        policy,
    )
}

/// A contact sheet from synthetic frames.
fn build_sheet(tiles: usize) -> Result<ImagePart, String> {
    let frames: Vec<PixelBuffer> = (0..tiles)
        .map(|index| synthetic_frame(640, 480, u8::try_from(index).unwrap_or(0)))
        .collect();
    let sources: Vec<SourceImage<'_>> = frames.iter().map(SourceImage::new).collect();
    payload::contact_sheet(&sources, PayloadPolicy::default())
        .map_err(|err| format!("payload: [{}] {}", err.code, err.detail))
}

/// A deterministic sRGB frame.
fn synthetic_frame(width: u32, height: u32, seed: u8) -> PixelBuffer {
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.push(((x * 3 + u32::from(seed)) % 256) as u8);
            pixels.push(((y * 5 + u32::from(seed) * 7) % 256) as u8);
            pixels.push((((x + y) * 2 + u32::from(seed) * 11) % 256) as u8);
        }
    }
    PixelBuffer {
        width,
        height,
        data: PixelData::Srgb8(pixels),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 1,
    }
}

/// One segment's worth of input.
fn segment(id: &str, best: &str, score_milli: u16, tiles: usize) -> SegmentInput {
    SegmentInput {
        segment_id: id.to_string(),
        start_offset_s: 7_200,
        end_offset_s: 9_000,
        frame_count: 148,
        local_guesses: vec![
            SceneGuess::new(best, score_milli),
            SceneGuess::new("candid", 210),
        ],
        exif: (0..tiles)
            .map(|index| ExifSummary {
                camera: Some("Canon EOS R5".to_string()),
                lens: Some("RF 35mm F1.8".to_string()),
                focal_mm: Some(35),
                aperture_x100: Some(180),
                shutter_denominator: Some(250),
                iso: Some(3200),
                flash: Some(false),
                offset_seconds: Some(u32::try_from(index).unwrap_or(0) * 30),
                orientation: Some(1),
                shutter_seconds: None,
            })
            .collect(),
        tile_hashes: (0..tiles).map(|index| format!("tile-{index}")).collect(),
    }
}

/// RAW container magic that must never appear in an upload.
fn raw_magic_in(bytes: &[u8]) -> Option<&'static str> {
    const MAGIC: &[(&str, &[u8])] = &[
        ("TIFF little-endian", b"II*\0"),
        ("TIFF big-endian", b"MM\0*"),
        ("Fujifilm RAF", b"FUJIFILMCCD-RAW"),
        ("Canon CR3", b"ftypcrx "),
    ];
    MAGIC
        .iter()
        .find(|(_, needle)| {
            bytes.len() >= needle.len() && bytes.windows(needle.len()).any(|w| w == *needle)
        })
        .map(|(name, _)| *name)
}

/// A key round-trips, and no platform's command shape puts it in `argv`.
fn key_safety() -> Result<String, String> {
    let secret = SecretKey::new(TEST_KEY);
    let store = MemoryKeyStore::new();
    store
        .store("api-key:anthropic", &secret)
        .map_err(|err| format!("keys: [{}] {}", err.code, err.detail))?;
    let read = store
        .load("api-key:anthropic")
        .map_err(|err| format!("keys: [{}] {}", err.code, err.detail))?
        .ok_or_else(|| "keys: the stored key could not be read back".to_string())?;
    if read.expose() != TEST_KEY {
        return Err("keys: the key that came back is not the key that went in".to_string());
    }

    let mut checked = 0usize;
    for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
        let os = OsKeyStore::with_runner(
            platform,
            Arc::new(RecordingRunner::new()),
            Path::new("target/phase04-verify/credentials"),
        );
        for command in [
            os.store_command("api-key:anthropic", &secret),
            os.load_command("api-key:anthropic"),
            os.delete_command("api-key:anthropic"),
        ] {
            if command.leaks_secret(TEST_KEY) {
                return Err(format!(
                    "keys: the {} command puts the secret in argv",
                    platform.as_str()
                ));
            }
            checked += 1;
        }
    }

    Ok(format!(
        "keys: round trip clean, {checked} command shapes across three platforms keep the \
         secret out of argv, fingerprint {}",
        read.fingerprint()
    ))
}

/// Nothing in the audit trail or the cache is key-shaped.
fn key_leak_scan(catalog: &Arc<Catalog>, project: &ProjectId) -> Result<String, String> {
    let audit = CatalogAudit::new(Arc::clone(catalog), Arc::new(SystemClock::default()));
    let rows = audit
        .recent(&project.to_string(), 500)
        .map_err(|err| format!("privacy: [{}] {}", err.code, err.detail))?;

    let mut scanned = 0usize;
    for row in &rows {
        let rendered = format!("{row:?}");
        if redact::looks_like_a_key(&rendered) {
            return Err(format!("privacy: audit row {} is key-shaped", row.id));
        }
        scanned += 1;
    }

    let cached: Vec<String> = catalog
        .read(|conn| {
            let mut statement = conn
                .prepare("SELECT response_json FROM cloud_cache")
                .map_err(|err| aura_core::errors::db::statement_failed("read cache", &err))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|err| aura_core::errors::db::statement_failed("read cache", &err))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(|err| {
                    aura_core::errors::db::statement_failed("read cache row", &err)
                })?);
            }
            Ok(out)
        })
        .map_err(|err| format!("privacy: [{}] {}", err.code, err.detail))?;

    for entry in &cached {
        if redact::looks_like_a_key(entry) {
            return Err("privacy: a cached response is key-shaped".to_string());
        }
        scanned += 1;
    }

    Ok(format!(
        "privacy: {scanned} stored rows scanned, nothing key-shaped"
    ))
}

/// The migration file records how to undo itself.
fn migration_documents_its_rollback() -> bool {
    const MIGRATION: &str = include_str!("../../aura-catalog/migrations/0004_cloud_audit.sql");
    ["cloud_calls", "cloud_cache", "cloud_budget"]
        .iter()
        .all(|table| MIGRATION.contains(&format!("DROP TABLE IF EXISTS {table}")))
        && MIGRATION.contains("DELETE FROM schema_version WHERE version = 4")
}
