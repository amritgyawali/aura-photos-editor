//! What is live, what is pending, and what rolled back.
//!
//! The state file is derivable from what is on disk, so every failure here is
//! answered by starting again rather than by refusing to start.

use std::fs;

use aura_infer::contract::infer::Version;
use aura_models::rollback::{state_path, InstallState};
use tempfile::tempdir;

const V1: Version = Version::new(1, 0, 0);
const V2: Version = Version::new(1, 1, 0);

#[test]
fn a_staged_version_is_the_candidate_before_it_is_confirmed() {
    let mut state = InstallState::new();
    state.confirm("scene", V1);
    state.stage("scene", V2);
    assert_eq!(state.state_of("scene").candidate(), Some(V2));
}

#[test]
fn rejecting_a_version_puts_the_previous_one_back() {
    let mut state = InstallState::new();
    state.confirm("scene", V1);
    state.stage("scene", V2);

    assert_eq!(state.reject("scene", V2), Some(V1));
    assert_eq!(state.state_of("scene").candidate(), Some(V1));
    assert!(state.state_of("scene").was_rejected(V2));
}

#[test]
fn a_rejected_version_is_not_retried_on_the_next_launch() {
    let dir = tempdir().expect("tempdir");
    let mut state = InstallState::new();
    state.confirm("scene", V1);
    state.stage("scene", V2);
    state.reject("scene", V2);
    state.save(dir.path()).expect("save");

    let reloaded = InstallState::load(dir.path());
    assert!(reloaded.state_of("scene").was_rejected(V2));
    assert_eq!(reloaded.state_of("scene").candidate(), Some(V1));
}

#[test]
fn re_installing_a_rejected_version_gives_it_another_chance() {
    // An operator who re-installs after a fix expects the new file to be tried,
    // not silently refused because a previous build of it failed.
    let mut state = InstallState::new();
    state.stage("scene", V2);
    state.reject("scene", V2);
    state.stage("scene", V2);
    assert!(!state.state_of("scene").was_rejected(V2));
    assert_eq!(state.state_of("scene").candidate(), Some(V2));
}

#[test]
fn a_corrupt_state_file_starts_over_rather_than_refusing_to_start() {
    let dir = tempdir().expect("tempdir");
    fs::write(state_path(dir.path()), b"{not json").expect("write");
    assert_eq!(InstallState::load(dir.path()).models.len(), 0);
}
