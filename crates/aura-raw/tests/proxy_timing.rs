//! Where building one 2048-pixel proxy spends its time, run by hand on a real file:
//!
//! ```text
//! AURA_PROBE_FILE=<photo.jpg> cargo test --release -p aura-raw --test proxy_timing -- --ignored --nocapture
//! ```
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout
)]

use std::time::Instant;

use aura_core::clock::SystemClock;
use aura_raw::{codec, demosaic, meta, proxy, DecodeLimits};

#[test]
#[ignore = "needs AURA_PROBE_FILE"]
fn proxy_timing() {
    let path =
        std::path::PathBuf::from(std::env::var_os("AURA_PROBE_FILE").expect("AURA_PROBE_FILE"));
    let bytes = std::fs::read(&path).expect("read the file");
    let t = Instant::now();
    let parsed = meta::read(&bytes, &path).expect("metadata");
    println!("metadata      {:?}", t.elapsed());
    let t = Instant::now();
    let decoded = codec::decode_jpeg(&bytes, DecodeLimits::tier2()).expect("decode");
    println!(
        "jpeg decode   {:?} ({}x{})",
        t.elapsed(),
        decoded.width,
        decoded.height
    );
    let t = Instant::now();
    let linear = demosaic::from_srgb8(&decoded.data, decoded.width, decoded.height);
    println!("to linear     {:?}", t.elapsed());
    let (w, h) = demosaic::fit_long_edge(decoded.width, decoded.height, 2048);
    let t = Instant::now();
    let scaled = demosaic::resize(&linear, w, h);
    println!("resize        {:?}", t.elapsed());
    let t = Instant::now();
    let encoded = demosaic::to_srgb8(&scaled);
    println!("to srgb8      {:?}", t.elapsed());
    let t = Instant::now();
    let jpeg = codec::encode_jpeg(
        &codec::Rgb8 {
            width: w,
            height: h,
            data: encoded,
        },
        90,
    )
    .expect("encode");
    println!("jpeg encode   {:?} ({} bytes)", t.elapsed(), jpeg.len());
    let t = Instant::now();
    let _ = codec::decode_jpeg(&jpeg, DecodeLimits::tier2()).expect("re-decode");
    println!("re-decode     {:?}", t.elapsed());
    let clock = SystemClock::default();
    let t = Instant::now();
    let _ = proxy::tier2(&bytes, &parsed, DecodeLimits::tier2(), &clock, None, &path).unwrap();
    println!("tier2 in all  {:?}", t.elapsed());
}
