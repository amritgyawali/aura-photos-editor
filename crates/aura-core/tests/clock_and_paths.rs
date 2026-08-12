use std::sync::Arc;

use aura_core::clock::{Clock, FixedClock, SystemClock};
use aura_core::paths::{assert_not_cloud_synced, platform_safe};
use time::{Duration, OffsetDateTime};

#[test]
fn wall_clock_may_go_backwards_while_monotonic_does_not() {
    let clock: Arc<FixedClock> = FixedClock::at(OffsetDateTime::UNIX_EPOCH + Duration::days(365));
    let before_mono = clock.monotonic_ms();
    let before_wall = clock.now_utc();

    clock.advance_ms(5_000);
    clock.set_wall_clock(before_wall - Duration::hours(3)); // an NTP correction after a flight

    assert!(
        clock.now_utc() < before_wall,
        "wall clock moved backwards, as intended"
    );
    assert!(
        clock.monotonic_ms() > before_mono,
        "monotonic clock must never go backwards"
    );
}

#[test]
fn system_clock_monotonic_is_non_decreasing() {
    let clock = SystemClock::default();
    let first = clock.monotonic_ms();
    let second = clock.monotonic_ms();
    assert!(second >= first);
}

#[test]
fn cloud_markers_are_refused() {
    let markers = [
        "Dropbox",
        "OneDrive",
        "Google Drive",
        "GoogleDrive",
        "iCloud",
        "com~apple~CloudDocs",
        "pCloud",
        "Box Sync",
        "MEGA",
        "YandexDisk",
        "Nextcloud",
        "ownCloud",
        "Sync.com",
        "IDrive",
        "Tresorit",
    ];
    for marker in markers {
        let path = std::path::PathBuf::from(format!("C:/Users/nina/{marker}/Weddings/catalog"));
        let refused = assert_not_cloud_synced(&path);
        assert!(refused.is_err(), "{marker} must be refused");
        let err = refused.expect_err("error");
        assert_eq!(err.code.0, "AURA-IO-1008");
    }
}

#[test]
fn a_renamed_folder_with_a_sync_marker_file_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join(".dropbox"), b"").expect("write marker");
    let nested = dir.path().join("Weddings/sarah-and-tom");
    std::fs::create_dir_all(&nested).expect("create nested");

    let refused = assert_not_cloud_synced(&nested);
    assert!(
        refused.is_err(),
        "a marker file in an ancestor must be refused"
    );
}

#[test]
fn a_plain_local_path_is_accepted() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(assert_not_cloud_synced(dir.path()).is_ok());
}

#[test]
fn long_paths_get_the_extended_prefix_on_windows() {
    let long = std::path::PathBuf::from(format!("C:/{}/catalog.sqlite", "a".repeat(260)));
    let safe = platform_safe(&long);
    if cfg!(windows) {
        assert!(safe.to_string_lossy().starts_with("\\\\?\\"));
    } else {
        assert_eq!(safe, long);
    }
}

#[test]
fn short_paths_are_untouched() {
    let short = std::path::PathBuf::from("C:/weddings/catalog.sqlite");
    assert_eq!(platform_safe(&short), short);
}
