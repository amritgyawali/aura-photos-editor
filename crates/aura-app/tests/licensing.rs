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

/// Subscription keys, valid only in 2020: the first period, the renewed one, and a key for the same
/// subscription id in somebody else's name.
const SUB_JULY: &str = "AURA1.eyJpZCI6InN1Yl8wMXRlc3QiLCJuYW1lIjoiQVVSQSB0ZXN0IHN1YnNjcmliZXIiLCJlbWFpbCI6InN1YkBleGFtcGxlLmludmFsaWQiLCJlZGl0aW9uIjoicHJvIiwiaXNzdWVkIjoiMjAyMC0wNi0wMSIsImV4cGlyZXMiOiIyMDIwLTA3LTA1In0.nC5N5IDCd8F3zRVxHKlIderNmfFb4dy8FLMoATDXMNiYSzkYmxVCvtDDr0ykkv-6gz3vyXwQ9YnNFKBytj_GBg";
const SUB_AUGUST: &str = "AURA1.eyJpZCI6InN1Yl8wMXRlc3QiLCJuYW1lIjoiQVVSQSB0ZXN0IHN1YnNjcmliZXIiLCJlbWFpbCI6InN1YkBleGFtcGxlLmludmFsaWQiLCJlZGl0aW9uIjoicHJvIiwiaXNzdWVkIjoiMjAyMC0wNi0wMSIsImV4cGlyZXMiOiIyMDIwLTA4LTA1In0.kKTSTpCWpkBuxAYu_bqfZhwBFm9pNh8HJ_RKV_LHGcUbW_69-5OREzl3beAGbCrKpzeYIVtKAVfY83u4m_fzDg";
const SUB_STRANGER: &str = "AURA1.eyJpZCI6InN1Yl8wMXRlc3QiLCJuYW1lIjoiU29tZWJvZHkgZWxzZSIsImVtYWlsIjoib3RoZXJAZXhhbXBsZS5pbnZhbGlkIiwiZWRpdGlvbiI6InBybyIsImlzc3VlZCI6IjIwMjAtMDYtMDEiLCJleHBpcmVzIjoiMjAyMC0wOS0wNSJ9.tt85Q-vfxKdrZ2P64UomtLZApzoUXjQ92OapF9wizUBcyMey_MO_uJJnBPf7jcvMspsxObcxrSdhszoRJAwUDg";

/// A licence server that answers with a fixed status and body, and remembers what it was asked.
#[derive(Debug)]
struct Server {
    status: u16,
    body: String,
    asked: std::sync::Mutex<Vec<String>>,
}

impl aura_cloud::provider::Transport for Server {
    fn send(
        &self,
        request: &aura_cloud::provider::HttpRequest,
        _timeout: std::time::Duration,
    ) -> aura_core::AuraResult<aura_cloud::provider::HttpResponse> {
        self.asked
            .lock()
            .unwrap()
            .push(format!("{} {}", request.method, request.url));
        Ok(aura_cloud::provider::HttpResponse {
            status: self.status,
            headers: Vec::new(),
            body: self.body.clone().into_bytes(),
        })
    }

    fn name(&self) -> &'static str {
        "test"
    }
}

fn server(status: u16, body: String) -> Server {
    Server {
        status,
        body,
        asked: std::sync::Mutex::new(Vec::new()),
    }
}

#[test]
fn a_subscription_renews_from_the_licence_server_and_only_to_a_genuine_later_key() {
    use aura_app::licensing::refresh_with;
    let dir = tempfile::tempdir().unwrap();
    let clock = FixedClock::at(datetime!(2020-06-20 10:00 UTC));
    let state = state(dir.path(), &clock);
    let first = activate_licence(&state, SUB_JULY).unwrap();
    assert!(first.renews);
    assert!(!first.renewal_due, "fifteen days left is not yet due");
    assert!(first.message.contains("renews automatically"));

    clock.set_wall_clock(datetime!(2020-06-28 10:00 UTC));
    assert!(licence_status(&state).unwrap().renewal_due);

    // A server that is down, refuses, or answers with something that is not a better key for
    // this subscription and this person changes nothing.
    let down = server(502, "<html>bad gateway</html>".into());
    assert_eq!(
        refresh_with(&state, &down, "https://shop.test")
            .unwrap_err()
            .code,
        "AURA-REL-12007"
    );
    let cancelled = server(
        402,
        r#"{"error":"This subscription was cancelled."}"#.into(),
    );
    let refused = refresh_with(&state, &cancelled, "https://shop.test").unwrap_err();
    assert!(refused.message.contains("cancelled"));
    let stranger = server(200, format!(r#"{{"key":"{SUB_STRANGER}"}}"#));
    assert!(refresh_with(&state, &stranger, "https://shop.test").is_err());
    let same = server(200, format!(r#"{{"key":"{SUB_JULY}"}}"#));
    assert!(refresh_with(&state, &same, "https://shop.test").is_err());
    let forged = server(200, r#"{"key":"AURA1.e30.AAAA"}"#.into());
    assert!(refresh_with(&state, &forged, "https://shop.test").is_err());
    assert_eq!(
        licence_status(&state).unwrap().expires.as_deref(),
        Some("2020-07-05")
    );

    // The renewed key is kept; the request carried only the subscription and the email.
    let renewed = server(200, format!(r#"{{"key":"{SUB_AUGUST}"}}"#));
    let status = refresh_with(&state, &renewed, "https://shop.test").unwrap();
    assert_eq!(status.expires.as_deref(), Some("2020-08-05"));
    assert!(!status.renewal_due);
    assert_eq!(
        renewed.asked.lock().unwrap().as_slice(),
        ["GET https://shop.test/api/licence?subscription=sub_01test&email=sub%40example.invalid"]
    );

    // Lapsed and not renewed: export stops, editing does not depend on it.
    clock.set_wall_clock(datetime!(2020-08-06 10:00 UTC));
    assert_eq!(licence_status(&state).unwrap().state, "expired");
    assert!(require_export(&state).is_err());
}
