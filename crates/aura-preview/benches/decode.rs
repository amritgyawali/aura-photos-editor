//! Criterion benchmarks, one per tier and per mosaic encoding.
//!
//! These are the numbers the phase 02 budget table is filled in from. They run
//! against generated bench fixtures rather than camera files, so the figures are
//! comparable across machines and across time; the absolute throughput on real
//! 45 MP RAWs is recorded separately in the exit report.
//!
//! Panics are permitted here: a benchmark is developer tooling, and a fixture
//! that will not generate should stop the run loudly.

use std::path::Path;

use aura_core::clock::SystemClock;
use aura_raw::contract::pixels::PixelLevel;
use aura_raw::fixtures::{FixtureOptions, MosaicEncoding};
use aura_raw::timeout::DecodeLimits;
use aura_raw::{full, meta, proxy, thumb};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn fixture(directory: &Path, encoding: MosaicEncoding) -> (Vec<u8>, aura_raw::RawMeta) {
    let path = directory.join(format!("bench-{}.dng", encoding.as_str()));
    aura_raw::fixtures::write_bench_raw(
        &path,
        FixtureOptions {
            body: 1,
            encoding,
            with_preview: true,
            orientation: 1,
        },
    )
    .expect("write the bench fixture");
    let bytes = std::fs::read(&path).expect("read the bench fixture");
    let parsed = meta::read(&bytes, &path).expect("parse the bench fixture");
    (bytes, parsed)
}

fn tiers(criterion: &mut Criterion) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let clock = SystemClock::default();

    let mut group = criterion.benchmark_group("preview");
    group.throughput(Throughput::Elements(1));

    for encoding in [
        MosaicEncoding::Unpacked16,
        MosaicEncoding::Packed14,
        MosaicEncoding::LosslessJpeg,
    ] {
        let (bytes, parsed) = fixture(directory.path(), encoding);
        let path = directory.path().join("bench.dng");

        group.bench_function(format!("tier1/{}", encoding.as_str()), |bencher| {
            bencher.iter(|| {
                thumb::tier1(
                    &bytes,
                    &parsed,
                    PixelLevel::Thumb(512).long_edge(),
                    DecodeLimits::tier1(),
                    &clock,
                    &path,
                )
                .expect("tier 1")
            });
        });

        group.bench_function(format!("tier2/{}", encoding.as_str()), |bencher| {
            bencher.iter(|| {
                proxy::tier2(
                    &bytes,
                    &parsed,
                    DecodeLimits::tier2(),
                    &clock,
                    None,
                    &path,
                )
                .expect("tier 2")
            });
        });

        group.bench_function(format!("tier3/{}", encoding.as_str()), |bencher| {
            bencher.iter(|| {
                full::tier3(&bytes, &parsed, DecodeLimits::tier3(), &clock).expect("tier 3")
            });
        });
    }

    group.finish();
}

criterion_group!(benches, tiers);
criterion_main!(benches);
