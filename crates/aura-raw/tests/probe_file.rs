//! Decode real files at every tier and print where each one fails, run by hand.
//!
//! `AURA_PROBE_FILE` names one file; `AURA_PROBE_LIST` names a text file with one path per
//! line. `AURA_PROBE_FULL=1` also runs the full-size decode. The error's own detail is
//! printed, which the photographer-facing message ("this photo file is damaged")
//! deliberately leaves out.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

use aura_core::clock::SystemClock;
use aura_raw::{full, meta, proxy, DecodeLimits};

fn probe(path: &std::path::Path, run_full: bool) {
    println!("== {}", path.display());
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("   read: {err}");
            return;
        }
    };
    let parsed = match meta::read(&bytes, path) {
        Ok(parsed) => parsed,
        Err(err) => {
            println!("   metadata: {err:?}");
            return;
        }
    };
    println!(
        "   format {:?} make {:?} model {:?} cfa {} black {} white {} wb {:?} matrix {}",
        parsed.format,
        parsed.make,
        parsed.model,
        parsed.cfa.as_str(),
        parsed.black_level,
        parsed.white_level,
        parsed.as_shot_neutral,
        parsed.colour_matrix.is_some()
    );
    if let Some(mosaic) = &parsed.mosaic {
        println!(
            "   mosaic {}x{} bits {} compression {} scheme {} segments {} support {:?}",
            mosaic.width,
            mosaic.height,
            mosaic.bits_per_sample,
            mosaic.compression,
            mosaic.scheme.as_str(),
            mosaic.segments.len(),
            parsed.mosaic_support()
        );
    } else {
        println!("   no mosaic");
    }
    let clock = SystemClock::default();
    let started = std::time::Instant::now();
    match proxy::tier2(&bytes, &parsed, DecodeLimits::tier2(), &clock, None, path) {
        Ok(out) => {
            println!(
                "   tier 2: ok {}x{} {:?} profile {} in {:?}",
                out.pair.srgb.width,
                out.pair.srgb.height,
                out.pair.srgb.source,
                out.profile.source.as_str(),
                started.elapsed()
            );
            if let Some(dir) = std::env::var_os("AURA_PROBE_OUT") {
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                let dir = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(format!("{stem}-aura.jpg")), &out.jpeg).unwrap();
                if let Some(preview) = parsed.best_preview() {
                    std::fs::write(
                        dir.join(format!("{stem}-camera.jpg")),
                        &bytes[preview.offset..preview.offset + preview.len],
                    )
                    .unwrap();
                }
            }
        }
        Err(err) => println!("   tier 2: {err:?}"),
    }
    if run_full {
        let started = std::time::Instant::now();
        match full::tier3(&bytes, &parsed, DecodeLimits::tier3(), &clock) {
            Ok(out) => println!(
                "   tier 3: ok {}x{} {:?} in {:?}",
                out.width,
                out.height,
                out.source,
                started.elapsed()
            ),
            Err(err) => println!("   tier 3: {err:?}"),
        }
    }
}

#[test]
#[ignore = "needs AURA_PROBE_FILE or AURA_PROBE_LIST"]
fn probe_files() {
    let run_full = std::env::var_os("AURA_PROBE_FULL").is_some();
    let mut paths = Vec::new();
    if let Some(one) = std::env::var_os("AURA_PROBE_FILE") {
        paths.push(std::path::PathBuf::from(one));
    }
    if let Some(list) = std::env::var_os("AURA_PROBE_LIST") {
        let text = std::fs::read_to_string(list).expect("read the list");
        paths.extend(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(std::path::PathBuf::from),
        );
    }
    assert!(!paths.is_empty(), "set AURA_PROBE_FILE or AURA_PROBE_LIST");
    for path in &paths {
        probe(path, run_full);
    }
}
