//! Real camera files decoded by aura-raw and by LibRaw, compared sample for sample.
//!
//! `scripts/raw-reference-dump.py <dir> <files...>` writes LibRaw's reading of each file;
//! `AURA_REF_DIR=<dir>` points this test at it. For every file the whole sensor - masked
//! border included - must match exactly, and the black level, white balance, crop and CFA
//! phase are compared with what LibRaw read from the same file. Run by hand; the camera
//! files are photographers' work and never enter the repository.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use aura_raw::{cfa, meta, DecodeLimits};

fn number(value: &serde_json::Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

#[test]
#[ignore = "needs AURA_REF_DIR from scripts/raw-reference-dump.py"]
fn every_file_decodes_exactly_like_the_reference() {
    let dir = std::path::PathBuf::from(std::env::var_os("AURA_REF_DIR").expect("AURA_REF_DIR"));
    let mut records: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .expect("read the dump directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    records.sort_by_key(|path| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<u64>().ok())
            .unwrap_or(u64::MAX)
    });

    let mut failures = Vec::new();
    for record_path in &records {
        let record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(record_path).unwrap()).unwrap();
        let path = std::path::PathBuf::from(record["path"].as_str().unwrap());
        let name = path.display().to_string();
        if record.get("error").is_some() {
            println!(
                "-- {name}: reference could not read it ({})",
                record["error"]
            );
            continue;
        }
        let bytes = std::fs::read(&path).expect("read the camera file");
        let started = std::time::Instant::now();
        let parsed = match meta::read(&bytes, &path) {
            Ok(parsed) => parsed,
            Err(err) => {
                failures.push(format!("{name}: metadata {err:?}"));
                continue;
            }
        };
        let Some(mosaic) = parsed.mosaic.as_ref() else {
            failures.push(format!("{name}: no mosaic located"));
            continue;
        };
        let sensor = match cfa::decode_sensor(
            &bytes,
            mosaic,
            parsed.little_endian,
            DecodeLimits::tier3().max_pixels,
        ) {
            Ok(sensor) => sensor,
            Err(err) => {
                failures.push(format!(
                    "{name}: decode ({}) {err:?}",
                    mosaic.scheme.as_str()
                ));
                continue;
            }
        };
        let decode_time = started.elapsed();

        let (rw, rh) = (
            record["raw_width"].as_u64().unwrap() as u32,
            record["raw_height"].as_u64().unwrap() as u32,
        );
        let mut problems = Vec::new();
        if (sensor.width, sensor.height) == (rw, rh) {
            let reference = std::fs::read(dir.join(record["samples"].as_str().unwrap())).unwrap();
            let mut mismatched = 0usize;
            let mut first = None;
            for (index, (ours, pair)) in sensor
                .data
                .iter()
                .zip(reference.chunks_exact(2))
                .enumerate()
            {
                let theirs = u16::from_le_bytes([pair[0], pair[1]]);
                if *ours != theirs {
                    mismatched += 1;
                    if first.is_none() {
                        first = Some((index % rw as usize, index / rw as usize, *ours, theirs));
                    }
                }
            }
            if mismatched > 0 {
                problems.push(format!(
                    "{mismatched} samples differ, first at {first:?} (x, y, ours, reference)"
                ));
            }
        } else {
            problems.push(format!(
                "sensor is {}x{}, reference {rw}x{rh}",
                sensor.width, sensor.height
            ));
        }

        // The picture inside the sensor.
        let (left, top) = (
            record["left_margin"].as_u64().unwrap() as u32,
            record["top_margin"].as_u64().unwrap() as u32,
        );
        let (width, height) = (
            record["width"].as_u64().unwrap() as u32,
            record["height"].as_u64().unwrap() as u32,
        );
        // The camera's own crop is often tighter than the reference's table, and
        // that is the one AURA keeps; it must lie inside the sensor's live area.
        let ours_area = mosaic
            .active_area
            .unwrap_or([0, 0, sensor.height, sensor.width]);
        let theirs_area = [top, left, top + height, left + width];
        // One photosite of slack on the far edges: Canon writes those edges
        // inclusively and the reference reads them exclusively in crop mode.
        let inside = ours_area[0] >= theirs_area[0]
            && ours_area[1] >= theirs_area[1]
            && ours_area[2] <= theirs_area[2] + 1
            && ours_area[3] <= theirs_area[3] + 1;
        if !inside {
            problems.push(format!(
                "picture area {ours_area:?} is not inside the reference's {theirs_area:?} (top, left, bottom, right)"
            ));
        } else if ours_area != theirs_area {
            println!(
                "   {name}: picture {}x{} at ({}, {}), reference {width}x{height} at ({left}, {top})",
                ours_area[3] - ours_area[1],
                ours_area[2] - ours_area[0],
                ours_area[1],
                ours_area[0]
            );
        }

        // Levels and white balance.
        let blacks: Vec<f64> = record["black"]
            .as_array()
            .unwrap()
            .iter()
            .map(number)
            .collect();
        let black = blacks.iter().sum::<f64>() / blacks.len().max(1) as f64;
        if (f64::from(parsed.black_level) - black).abs() > 1.5 {
            problems.push(format!("black {} reference {black}", parsed.black_level));
        }
        // Canon and Sony state where each ISO actually clips; that can sit well
        // below the reference's container maximum and is the better value to clip
        // at, so a lower white is reported rather than failed.
        let white = number(&record["white"]);
        if f64::from(parsed.white_level) > white * 1.06
            || f64::from(parsed.white_level) < white * 0.5
        {
            problems.push(format!("white {} reference {white}", parsed.white_level));
        } else if (f64::from(parsed.white_level) - white).abs() > 1.0 {
            println!(
                "   {name}: white {} (reference {white})",
                parsed.white_level
            );
        }
        let wb: Vec<f64> = record["camera_wb"]
            .as_array()
            .unwrap()
            .iter()
            .map(number)
            .collect();
        // A reference that read no white balance reports exactly 1:1:1; there is
        // nothing to compare then.
        let reference_has_wb = !(wb.len() >= 3 && wb[0] == wb[1] && wb[2] == wb[1]);
        if wb.len() >= 3 && wb[1] > 0.0 && reference_has_wb {
            let expected = [wb[0] / wb[1], wb[2] / wb[1]];
            let got = [
                f64::from(parsed.as_shot_neutral[0]),
                f64::from(parsed.as_shot_neutral[2]),
            ];
            if (expected[0] - got[0]).abs() > 0.01 || (expected[1] - got[1]).abs() > 0.01 {
                problems.push(format!(
                    "white balance r/g b/g {got:?}, reference {expected:?}"
                ));
            }
        }

        // The colour of the picture's top-left photosite: LibRaw's pattern is for the
        // visible area, ours is for the sensor's origin.
        if let Some(pattern) = record["pattern"].as_array() {
            let desc: Vec<char> = record["colour_desc"]
                .as_str()
                .unwrap_or("RGBG")
                .chars()
                .collect();
            let colour_of = |index: u64| match desc.get(index as usize) {
                Some('R') => 0u8,
                Some('B') => 2,
                _ => 1,
            };
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let (px, py) = (ours_area[1] - left + dx, ours_area[0] - top + dy);
                    // Two rows and columns for Bayer, six for X-Trans.
                    let size = pattern.len() as u32;
                    let theirs = colour_of(
                        pattern[(py % size) as usize].as_array().unwrap()[(px % size) as usize]
                            .as_u64()
                            .unwrap(),
                    );
                    let ours = parsed.cfa.colour_at(dx, dy);
                    if ours != theirs {
                        problems.push(format!(
                            "cfa colour at picture ({dx},{dy}) is {ours}, reference {theirs}"
                        ));
                    }
                }
            }
        }

        if problems.is_empty() {
            println!(
                "ok {name}: {:?} {} {}x{} in {decode_time:?}",
                parsed.format,
                mosaic.scheme.as_str(),
                sensor.width,
                sensor.height
            );
        } else {
            for problem in &problems {
                println!("!! {name}: {problem}");
            }
            failures.push(format!("{name}: {}", problems.join("; ")));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} files differ from the reference:\n{}",
        failures.len(),
        records.len(),
        failures.join("\n")
    );
}
