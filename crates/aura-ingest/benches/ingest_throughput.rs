//! Budget benchmarks for the ingest stages. A regression here fails CI.

use std::path::{Path, PathBuf};

use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_ingest::{hash, pair, scan};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

/// Write `count` files of `size_bytes` into a fresh temporary tree.
fn corpus(count: usize, size_bytes: usize) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("cards");
    std::fs::create_dir_all(&root).expect("create root");
    let payload = vec![0x5Au8; size_bytes];
    for index in 0..count {
        let path = root.join(format!("IMG_{index:05}.JPG"));
        std::fs::write(&path, &payload).expect("write fixture");
    }
    (dir, root)
}

fn plan_for(root: &Path) -> ImportPlan {
    ImportPlan {
        import_id: ImportId::new(),
        project_id: ProjectId::new(),
        roots: vec![root.to_path_buf()],
        mode: ImportMode::Reference,
        extensions: Vec::new(),
        extract_embedded_previews: false,
        settle_window_ms: 0,
    }
}

fn bench_scan(c: &mut Criterion) {
    let (_dir, root) = corpus(2_000, 1_024);
    let plan = plan_for(&root);
    c.bench_function("scan_2000_files", |b| {
        b.iter(|| scan::scan_root(&root, &plan, i64::MAX).expect("scan"));
    });
}

fn bench_hash(c: &mut Criterion) {
    let (_dir, root) = corpus(200, 256 * 1024);
    let plan = plan_for(&root);
    let scanned = scan::scan_root(&root, &plan, i64::MAX).expect("scan");
    c.bench_function("hash_200_files_256kib", |b| {
        b.iter_batched(
            || scanned.files.clone(),
            |files| hash::hash_batch(&files, 4),
            BatchSize::SmallInput,
        );
    });
}

fn bench_pair(c: &mut Criterion) {
    let (_dir, root) = corpus(2_000, 512);
    let plan = plan_for(&root);
    let scanned = scan::scan_root(&root, &plan, i64::MAX).expect("scan");
    c.bench_function("pair_2000_files", |b| {
        b.iter(|| pair::group(&scanned.files));
    });
}

criterion_group!(benches, bench_scan, bench_hash, bench_pair);
criterion_main!(benches);
