//! Learn a personal style from a real Lightroom catalogue, as the Personal Style screen does,
//! into a throwaway AURA catalogue, and print what was learned. ADR-0104.
//!
//! Ignored. `AURA_LRCAT` names the `.lrcat` (it is copied and read, never written).
//!
//! ```text
//! AURA_LRCAT="C:/Users/me/Pictures/Lightroom/Lightroom Catalog.lrcat" \
//!   cargo test -p aura-app --release --test lightroom_style -- --ignored --nocapture
//! ```
#![allow(clippy::expect_used, clippy::print_stderr, clippy::disallowed_methods)]
use std::sync::Arc;

use aura_app::personal_style::{self, LearnInput};
use aura_app::AppState;

#[test]
#[ignore = "needs AURA_LRCAT, a real Lightroom catalogue"]
fn learn_a_personal_style_from_a_real_catalogue() {
    let Some(catalog) = std::env::var_os("AURA_LRCAT") else {
        eprintln!("AURA_LRCAT is not set");
        return;
    };
    let dir = tempfile::tempdir().expect("isolated catalog directory");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("open isolated catalog")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let started = std::time::Instant::now();
    let learned = personal_style::learn_from_lightroom(
        &state,
        &LearnInput {
            catalog: catalog.to_string_lossy().into_owned(),
            name: "My Lightroom style".into(),
        },
    )
    .expect("learn");
    eprintln!(
        "learned from {} of {} edited photographs in {:.0} s",
        learned.photos,
        learned.edited,
        started.elapsed().as_secs_f32()
    );
    for finding in &learned.findings {
        eprintln!("  - {finding}");
    }
    eprintln!(
        "profile: {}",
        serde_json::to_string_pretty(&learned.profile).unwrap_or_default()
    );
}
