//! The trial, the licence key and the one thing they gate: exporting. ADR-0105.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

use std::sync::Arc;

use aura_app::licensing::{activate_licence, deactivate_licence, licence_status, require_export};
use aura_app::AppState;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, FixedClock};
use time::macros::datetime;

/// Issued by `tools/licence-issue` with the vendor key, valid through 2020 only: it opens this
/// test's 2020 clock and nothing in the real world.
const TEST_KEY: &str = "AURA1.eyJpZCI6InRlc3QtMDAwMSIsIm5hbWUiOiJBVVJBIHRlc3Qgc3VpdGUiLCJlbWFpbCI6InRlc3RAZXhhbXBsZS5pbnZhbGlkIiwiZWRpdGlvbiI6InBybyIsImlzc3VlZCI6IjIwMjAtMDEtMDEiLCJleHBpcmVzIjoiMjAyMC0xMi0zMSJ9.kdBUV9d6Yn-X0ya7a63AshG_V4HmhmCYOR2JVqCsxBQW5rqhWTEofGEt70uOCdf3Dpy1rsu7JEME9TezEnasCQ";

fn state(dir: &std::path::Path, clock: &Arc<FixedClock>) -> AppState {
    let clock: Arc<dyn Clock> = clock.clone();
    let catalog = Catalog::open(&dir.join("catalog.aura"), Arc::clone(&clock), "test").unwrap();
    AppState::with_catalog(Arc::new(catalog), clock)
}

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

#[test]
fn a_new_installation_gets_fourteen_days_then_only_export_needs_a_key() {
    let dir = tempfile::tempdir().unwrap();
    let clock = FixedClock::at(datetime!(2020-06-01 10:00 UTC));
    let state = state(dir.path(), &clock);

    let first = licence_status(&state).unwrap();
    assert_eq!(first.state, "trial");
    assert_eq!(first.days_left, Some(14));
    assert!(require_export(&state).is_ok());

    clock.advance_ms(13 * DAY_MS);
    assert_eq!(licence_status(&state).unwrap().days_left, Some(1));
    clock.advance_ms(DAY_MS);
    let ended = licence_status(&state).unwrap();
    assert_eq!(ended.state, "trial_ended");
    assert!(!ended.may_export);
    let refused = require_export(&state).unwrap_err();
    assert_eq!(refused.code.0, "AURA-REL-12005");
    assert!(refused.user_message.contains("Editing still works"));

    // Deleting the licence file does not restart the trial: the catalog remembers it too.
    std::fs::remove_dir_all(state.licence_dir()).unwrap();
    assert_eq!(licence_status(&state).unwrap().state, "trial_ended");
}

#[test]
fn a_valid_key_unlocks_export_and_a_bad_one_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let clock = FixedClock::at(datetime!(2020-06-01 10:00 UTC));
    let state = state(dir.path(), &clock);
    // The trial starts the first time AURA looks.
    assert_eq!(licence_status(&state).unwrap().state, "trial");
    clock.advance_ms(30 * DAY_MS);
    assert!(require_export(&state).is_err());

    let bad = activate_licence(&state, "AURA1.not-a-key.at-all").unwrap_err();
    assert_eq!(bad.code, "AURA-REL-12006");
    assert!(require_export(&state).is_err());

    // Pasted with the line breaks an email adds.
    let wrapped = format!("{}\n  {}", &TEST_KEY[..50], &TEST_KEY[50..]);
    let licensed = activate_licence(&state, &wrapped).unwrap();
    assert_eq!(licensed.state, "licensed");
    assert_eq!(licensed.name.as_deref(), Some("AURA test suite"));
    assert!(require_export(&state).is_ok());

    // A term licence ends after its last day; editing would carry on, export stops.
    clock.set_wall_clock(datetime!(2021-01-01 09:00 UTC));
    assert_eq!(licence_status(&state).unwrap().state, "expired");
    assert!(require_export(&state).is_err());
    // And an ended key is refused when pasted.
    assert!(activate_licence(&state, TEST_KEY).is_err());

    clock.set_wall_clock(datetime!(2020-07-15 09:00 UTC));
    assert!(require_export(&state).is_ok());
    let removed = deactivate_licence(&state).unwrap();
    assert_eq!(removed.state, "trial_ended");
}
