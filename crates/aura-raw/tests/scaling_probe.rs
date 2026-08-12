//! How decode cost scales with sensor size.
//!
//! Ignored by default: it generates and decodes frames up to 25 MP, which is
//! minutes of work and megabytes of temporary files - not something CI should
//! pay for on every push. It exists because the phase 02 budget table is stated
//! per image on a 45 MP camera file, and a benchmark that only ever measures a
//! 0.1 MP fixture cannot honestly speak to that number.
//!
//! Run it when the decode path changes, and paste the table into the exit
//! report or the performance ticket:
//!
//! ```text
//! cargo test --release -p aura-raw --test scaling_probe -- --ignored --nocapture
//! ```

use std::sync::Arc;

use aura_core::clock::{Clock, SystemClock};
use aura_raw::fixtures::{write_bench_raw, FixtureOptions, MosaicEncoding};
use aura_raw::timeout::DecodeLimits;
use aura_raw::{full, meta, proxy, thumb};

#[test]
#[ignore = "generates frames up to 25 MP; run it deliberately, not in CI"]
fn decode_cost_per_megapixel() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());

    println!("\n  patch  sensor        MP   tier1 ms  tier2 ms  tier3 ms  tier2 ms/MP");
    for patch_edge in [64u32, 256, 512, 1024] {
        let path = directory.path().join(format!("scale-{patch_edge}.dng"));
        let fixture = write_bench_raw(
            &path,
            FixtureOptions {
                patch_edge,
                encoding: MosaicEncoding::Unpacked16,
                ..FixtureOptions::default()
            },
        )
        .expect("write the fixture");
        let bytes = std::fs::read(&path).expect("read the fixture");
        let parsed = meta::read(&bytes, &path).expect("parse the fixture");

        let started = clock.monotonic_ms();
        thumb::tier1(
            &bytes,
            &parsed,
            512,
            DecodeLimits::tier1(),
            clock.as_ref(),
            &path,
        )
        .expect("tier 1");
        let tier1 = clock.monotonic_ms() - started;

        let started = clock.monotonic_ms();
        proxy::tier2(
            &bytes,
            &parsed,
            DecodeLimits::tier2(),
            clock.as_ref(),
            None,
            &path,
        )
        .expect("tier 2");
        let tier2 = clock.monotonic_ms() - started;

        let started = clock.monotonic_ms();
        full::tier3(&bytes, &parsed, DecodeLimits::tier3(), clock.as_ref()).expect("tier 3");
        let tier3 = clock.monotonic_ms() - started;

        let megapixels = f64::from(fixture.width) * f64::from(fixture.height) / 1_000_000.0;
        println!(
            "  {patch_edge:>5}  {:>5}x{:<5} {megapixels:>6.2}  {tier1:>8}  {tier2:>8}  {tier3:>8}  {:>11.1}",
            fixture.width,
            fixture.height,
            tier2 as f64 / megapixels.max(0.001)
        );

        let _ = std::fs::remove_file(&path);
    }
    println!();
}
