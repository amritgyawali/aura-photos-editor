//! Decode one real file at every tier and print where it fails, run by hand.
//!
//! `AURA_PROBE_FILE` names the file. The error's own detail is printed, which the
//! photographer-facing message ("this photo file is damaged") deliberately leaves out.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

use aura_core::clock::SystemClock;
use aura_raw::{full, meta, proxy, DecodeLimits};

#[test]
#[ignore = "needs AURA_PROBE_FILE"]
fn probe_one_file() {
    let path =
        std::path::PathBuf::from(std::env::var_os("AURA_PROBE_FILE").expect("AURA_PROBE_FILE"));
    let bytes = std::fs::read(&path).expect("read the file");
    let parsed = match meta::read(&bytes, &path) {
        Ok(parsed) => parsed,
        Err(err) => panic!("metadata: {err:?}"),
    };
    println!("format {:?}", parsed.format);
    if let Some(mosaic) = &parsed.mosaic {
        println!(
            "mosaic {}x{} bits {} compression {} scheme {:?} segments {}",
            mosaic.width,
            mosaic.height,
            mosaic.bits_per_sample,
            mosaic.compression,
            mosaic.scheme,
            mosaic.segments.len()
        );
    }
    let clock = SystemClock::default();
    match proxy::tier2(&bytes, &parsed, DecodeLimits::tier2(), &clock, None, &path) {
        Ok(_) => println!("tier 2: ok"),
        Err(err) => println!("tier 2: {err:?}"),
    }
    match full::tier3(&bytes, &parsed, DecodeLimits::tier3(), &clock) {
        Ok(_) => println!("tier 3: ok"),
        Err(err) => println!("tier 3: {err:?}"),
    }
}
