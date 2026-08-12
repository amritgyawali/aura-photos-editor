//! The refusal chain, the schema and the single-writer rule.

use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, FixedClock};
use time::OffsetDateTime;

fn test_clock() -> Arc<dyn Clock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000))
}

fn fresh() -> (tempfile::TempDir, Catalog) {
    let dir = tempfile::tempdir().expect("tmp");
    let catalog = Catalog::open(&dir.path().join("c.sqlite"), test_clock(), "0.1.0-test")
        .expect("open catalog");
    (dir, catalog)
}

fn seed_project(catalog: &Catalog) -> String {
    let now = rfc3339(catalog.clock().now_utc());
    let row = ProjectRow {
        project_id: "prj_test".to_string(),
        name: "Sarah and Tom".to_string(),
        couple_label: None,
        event_date: Some("2026-05-02".to_string()),
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    let id = row.project_id.clone();
    catalog
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))
        .expect("create project");
    id
}

#[test]
fn fresh_catalog_reaches_current_version_and_is_clean() {
    let (_dir, catalog) = fresh();
    // Not a literal: the version moves with the migration list, and a test that
    // hard-coded it would have to be edited by the same person who forgot to add
    // the migration.
    assert_eq!(
        catalog.schema_version().expect("version"),
        aura_catalog::APP_SCHEMA_VERSION
    );
    catalog.integrity_check().expect("integrity");
}

#[test]
fn a_fresh_catalog_has_the_cloud_tables_migration_4_adds() {
    let (_dir, catalog) = fresh();
    for table in ["cloud_calls", "cloud_cache", "cloud_budget"] {
        assert_eq!(
            catalog.count(table).unwrap_or(-1),
            0,
            "{table} is missing from a fresh catalog"
        );
    }
}

#[test]
fn creating_a_project_creates_a_denied_consent_row() {
    let (_dir, catalog) = fresh();
    let project_id = seed_project(&catalog);

    let granted: i64 = catalog
        .read(|conn| {
            conn.query_row(
                "SELECT cloud_metadata + cloud_derived_imagery + cloud_full_imagery + telemetry
                        + improve_models
                   FROM project_consent WHERE project_id = ?1",
                [&project_id],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("read consent", &e))
        })
        .expect("read consent");
    assert_eq!(granted, 0, "a new project must grant nothing");
}

#[test]
fn strict_tables_reject_wrong_types() {
    let (_dir, catalog) = fresh();
    seed_project(&catalog);

    let result = catalog.writer().with(|conn| {
        Ok(conn.execute(
            "INSERT INTO photo_file (file_id, photo_id, project_id, root_id, rel_path, file_name,
             ext, kind, role, size_bytes, content_hash, mtime, fast_key, first_seen_at, last_seen_at)
             VALUES ('f','p','prj_test','r','a','a','cr2','raw','primary','not-a-number','h','t','k','t','t')",
            [],
        ))
    });

    let inner = result.expect("call reached the writer");
    assert!(inner.is_err(), "STRICT must reject a text size_bytes");
}

#[test]
fn second_instance_is_refused_with_3005() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("c.sqlite");
    let _first = Catalog::open(&path, test_clock(), "0.1.0").expect("first open");

    let second = Catalog::open(&path, test_clock(), "0.1.0");
    let err = second.expect_err("second open must fail");
    assert_eq!(err.code.0, "AURA-DB-3005");
    assert!(err.user_message.contains("another AURA window"));
}

#[test]
fn lock_is_released_when_the_first_catalog_is_dropped() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("c.sqlite");
    {
        let _first = Catalog::open(&path, test_clock(), "0.1.0").expect("first open");
    }
    let second = Catalog::open(&path, test_clock(), "0.1.0");
    assert!(
        second.is_ok(),
        "the lock must be free once the owner is gone"
    );
}

#[test]
fn cloud_synced_path_is_refused_with_1008() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("Dropbox").join("c.sqlite");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");

    let err = Catalog::open(&path, test_clock(), "0.1.0").expect_err("must fail");
    assert_eq!(err.code.0, "AURA-IO-1008");
}

#[test]
fn newer_schema_is_refused_not_downgraded() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("c.sqlite");
    {
        let catalog = Catalog::open(&path, test_clock(), "0.1.0").expect("open");
        catalog
            .writer()
            .with(|conn| {
                Ok(conn.execute(
                    "INSERT INTO schema_version VALUES (99, '2026-01-01T00:00:00Z', '9.9.9', 'x')",
                    [],
                ))
            })
            .expect("call")
            .expect("insert");
    }

    let err = Catalog::open(&path, test_clock(), "0.1.0").expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-DB-3004");
}

#[test]
fn corrupt_catalog_is_detected_before_any_read() {
    use std::io::{Seek, SeekFrom, Write};

    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("c.sqlite");
    {
        let catalog = Catalog::open(&path, test_clock(), "0.1.0").expect("open");
        seed_project(&catalog);
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open raw");
    file.seek(SeekFrom::Start(8192)).expect("seek");
    file.write_all(&[0xAB; 4096]).expect("scribble");
    file.sync_all().expect("sync");
    drop(file);

    let err = Catalog::open(&path, test_clock(), "0.1.0").expect_err("must fail");
    assert!(
        matches!(err.code.0, "AURA-DB-3003" | "AURA-DB-3001"),
        "unexpected code {}",
        err.code.0
    );
}

#[test]
fn writer_is_single_threaded_under_contention() {
    let (_dir, catalog) = fresh();
    let writer = catalog.writer().clone();

    let handles: Vec<_> = (0..16)
        .map(|i| {
            let writer = writer.clone();
            std::thread::spawn(move || {
                writer.transact(move |tx| {
                    tx.execute(
                        "INSERT INTO setting VALUES (?1, '1', '2026-01-01T00:00:00Z')",
                        [format!("k{i}")],
                    )
                    .map_err(|e| aura_core::errors::db::statement_failed("insert setting", &e))?;
                    Ok(())
                })
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("join").expect("txn");
    }
    assert_eq!(catalog.count("setting").expect("count"), 16);
}

#[test]
fn hot_queries_use_indexes_not_table_scans() {
    let (_dir, catalog) = fresh();
    seed_project(&catalog);

    let plans = [
        "SELECT task_id FROM task WHERE project_id = 'prj_test' AND state = 'ready' \
         ORDER BY priority, created_at, task_id LIMIT 1",
        "SELECT photo_id FROM photo_file WHERE project_id = 'prj_test' AND content_hash = 'x'",
        "SELECT photo_id FROM photo WHERE project_id = 'prj_test' ORDER BY timeline_time",
        "SELECT abs_path FROM quarantine WHERE project_id = 'prj_test' AND resolved_at IS NULL",
    ];

    for sql in plans {
        let plan: String = catalog
            .read(|conn| {
                conn.query_row(&format!("EXPLAIN QUERY PLAN {sql}"), [], |row| {
                    row.get::<_, String>(3)
                })
                .map_err(|e| aura_core::errors::db::statement_failed("explain", &e))
            })
            .expect("explain");
        assert!(
            plan.contains("USING INDEX") || plan.contains("USING COVERING INDEX"),
            "query planned as a scan: {sql} -> {plan}"
        );
    }
}

#[test]
fn count_refuses_an_unknown_table_name() {
    let (_dir, catalog) = fresh();
    let err = catalog
        .count("sqlite_master; DROP TABLE photo")
        .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-DB-3006");
}
