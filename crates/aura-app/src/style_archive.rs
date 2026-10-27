//! Local archive evidence for the desktop training path. Source files are read only.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_core::{AuraError, AuraResult};
use aura_style::bucket::PairSource;
use aura_style::pairs::{self, Archive};
use rusqlite::OptionalExtension;

use crate::AppState;

fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("style archive", message);
    message.clone_into(&mut error.user_message);
    error
}

fn key(name: &str) -> String {
    format!(
        "style_archive_v1:{}",
        blake3::hash(name.as_bytes()).to_hex()
    )
}

pub(super) fn save_scan(state: &AppState, name: &str, roots: &[String]) -> AuraResult<()> {
    if name.trim().is_empty() || name.len() > 128 || roots.is_empty() || roots.len() > 64 {
        return Err(invalid(
            "Provide a look name and between one and 64 archive folders.",
        ));
    }
    let mut unique = BTreeSet::new();
    for root in roots {
        let canonical = std::fs::canonicalize(root)
            .map_err(|_| invalid("An archive folder is unavailable."))?;
        if !unique.insert(canonical) {
            return Err(invalid(
                "The same archive folder was selected more than once.",
            ));
        }
    }
    let value =
        serde_json::to_string(roots).map_err(|_| invalid("Cannot save archive folders."))?;
    let key = key(name);
    let name = name.to_owned();
    let at = aura_catalog::rfc3339(state.clock().now_utc());
    state.catalog().writer().transact(move |conn| {
        conn.execute("INSERT INTO setting(key,value_json,updated_at) VALUES(?1,?2,?3)
            ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
            rusqlite::params![key,value,at])
            .map_err(|e| aura_core::errors::db::statement_failed("save style archive", &e))?;
        // A deliberate rescan invalidates fits of earlier finals. Cancellation without
        // rescanning still resumes the completed pairs from this exact scan.
        conn.execute("DELETE FROM style_pairs WHERE profile_name=?1", [name])
            .map_err(|e| aura_core::errors::db::statement_failed("refresh style evidence", &e))?;
        Ok(())
    })
}

pub(super) struct Files {
    pairs: BTreeMap<String, (PathBuf, PathBuf)>,
    clock: Arc<dyn aura_core::clock::Clock>,
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalStyleArchive")
            .field("pairs", &self.pairs.len())
            .finish_non_exhaustive()
    }
}

fn paths(root: &Path, found: &mut BTreeMap<String, PathBuf>) -> AuraResult<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut visited = 0_usize;
    while let Some(folder) = pending.pop() {
        let entries =
            std::fs::read_dir(&folder).map_err(|_| invalid("Cannot read an archive folder."))?;
        for entry in entries {
            visited += 1;
            if visited > 200_000 {
                return Err(invalid(
                    "Split this archive into folders of at most 200,000 entries.",
                ));
            }
            let entry = entry.map_err(|_| invalid("Cannot read an archive entry."))?;
            let kind = entry
                .file_type()
                .map_err(|_| invalid("Cannot inspect an archive entry."))?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                let path = entry.path();
                let extension = path
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !pairs::ORIGINAL_EXTENSIONS.contains(&extension.as_str())
                    && !pairs::FINAL_EXTENSIONS.contains(&extension.as_str())
                {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if found.insert(name, path).is_some() {
                    return Err(invalid("This archive contains repeated file names. Select separate shoots with unique camera/export names before training."));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn load(state: &AppState, name: &str) -> AuraResult<(Files, Vec<Archive>)> {
    let saved: Option<String> = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT value_json FROM setting WHERE key=?1",
            [key(name)],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| aura_core::errors::db::statement_failed("read style archive", &e))
    })?;
    let saved = saved.ok_or_else(|| {
        invalid("Scan the original and edited folders before training this look.")
    })?;
    let roots: Vec<String> =
        serde_json::from_str(&saved).map_err(|_| invalid("Scan these archive folders again."))?;
    if roots.is_empty() || roots.len() > 64 {
        return Err(invalid("Scan these archive folders again."));
    }
    let mut archives = Vec::new();
    let mut found = BTreeMap::new();
    for (index, root) in roots.iter().enumerate() {
        let path = Path::new(root);
        paths(path, &mut found)?;
        archives.push(pairs::scan(path, &format!("archive-{}", index + 1))?);
    }
    let mut selected = BTreeMap::new();
    for pair in archives
        .iter()
        .flat_map(|archive| pairs::match_all(archive).pairs)
    {
        let original = found
            .get(&pair.original.name)
            .ok_or_else(|| invalid("An original disappeared. Scan the archive again."))?;
        let finished = found
            .get(&pair.finished.name)
            .ok_or_else(|| invalid("An edited photo disappeared. Scan the archive again."))?;
        selected.insert(pair.original.name, (original.clone(), finished.clone()));
    }
    if selected.len() < aura_style::api::MIN_PAIRS {
        return Err(invalid("The current archive has fewer than 120 unique original/edited pairs. Scan a larger archive."));
    }
    Ok((
        Files {
            pairs: selected,
            clock: Arc::clone(state.clock()),
        },
        archives,
    ))
}

fn original_bytes(path: &Path) -> AuraResult<Vec<u8>> {
    use std::io::Read;
    let file =
        std::fs::File::open(path).map_err(|_| invalid("The training original is unavailable."))?;
    let mut bytes = Vec::new();
    file.take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("Cannot read the training original."))?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(invalid("The training original exceeds the 256 MB limit."));
    }
    Ok(bytes)
}

impl PairSource for Files {
    fn original(&self, key: &str, long_edge: u32) -> AuraResult<(Vec<f32>, u32, u32)> {
        let path = &self
            .pairs
            .get(key)
            .ok_or_else(|| invalid("The training original is unavailable."))?
            .0;
        let bytes = original_bytes(path)?;
        let meta = aura_raw::read_meta(&bytes, path)?;
        let proxy = aura_raw::proxy::tier2(
            &bytes,
            &meta,
            aura_raw::DecodeLimits::default(),
            self.clock.as_ref(),
            None,
            path,
        )?;
        let image = proxy.pair.linear;
        let aura_raw::PixelData::Linear16(codes) = image.data else {
            return Err(invalid(
                "The original did not decode into linear camera pixels.",
            ));
        };
        let working = aura_raw::demosaic::RgbF32 {
            width: image.width,
            height: image.height,
            data: codes
                .into_iter()
                .map(aura_raw::colour::curve::linear_u16_to_scene)
                .collect(),
        };
        let (width, height) =
            aura_raw::demosaic::fit_long_edge(working.width, working.height, long_edge.max(1));
        let resized = aura_raw::demosaic::resize(&working, width, height);
        Ok((resized.data, width, height))
    }

    fn finished(&self, key: &str, long_edge: u32) -> AuraResult<(Vec<u8>, u32, u32)> {
        let path = &self
            .pairs
            .get(key)
            .ok_or_else(|| invalid("The edited training photo is unavailable."))?
            .1;
        let decoded = crate::color_match::gallery_fit::decode(path)?;
        let (width, height) =
            aura_raw::demosaic::fit_long_edge(decoded.width, decoded.height, long_edge.max(1));
        let image = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.data)
            .ok_or_else(|| invalid("Cannot read the edited training pixels."))?;
        Ok((
            image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle)
                .into_raw(),
            width,
            height,
        ))
    }

    fn camera(&self, key: &str) -> String {
        self.pairs
            .get(key)
            .and_then(|(path, _)| {
                original_bytes(path)
                    .ok()
                    .and_then(|bytes| aura_raw::read_meta(&bytes, path).ok())
                    .map(|meta| meta.model)
            })
            .unwrap_or_default()
    }
    // Pixel fits provide the evidence; unconfirmed sidecar settings are not accepted as a fit.
    fn xmp(&self, _key: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanned_local_camera_pairs_reach_the_real_training_pass() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("pairs");
        std::fs::create_dir_all(&folder).unwrap();
        let raw = folder.join("capture0000.dng");
        aura_raw::fixtures::write_bench_raw(
            &raw,
            aura_raw::fixtures::FixtureOptions {
                patch_edge: 8,
                ..aura_raw::fixtures::FixtureOptions::default()
            },
        )
        .unwrap();
        let state = AppState::open(&dir.path().join("catalog.sqlite")).unwrap();
        let probe = Files {
            pairs: BTreeMap::from([("first".into(), (raw.clone(), folder.join("unused.png")))]),
            clock: Arc::clone(state.clock()),
        };
        let (pixels, width, height) = probe.original("first", 512).unwrap();
        let camera = probe.camera("first");
        let frame = aura_render::Frame::working(pixels, width, height, &camera);
        let engine = aura_style::fit::optimise::engine(Arc::clone(state.clock()));
        let recipe = aura_style::extract::TheirParams {
            exposure: 0.2,
            contrast: 6.0,
            ..aura_style::extract::TheirParams::default()
        }
        .into_recipe(&aura_recipe::fixtures::neutral("first", &camera));
        let rendered = engine
            .render_frame(
                &frame,
                &recipe,
                aura_render::RenderLevel::Full,
                aura_render::RenderPurpose::Analysis,
                &aura_render::OutputSpec::default(),
            )
            .unwrap();
        let aura_render::RenderedData::Eight(bytes) = rendered.data else {
            panic!("RGB8 output");
        };
        image::RgbImage::from_raw(width, height, bytes)
            .unwrap()
            .save(folder.join("capture0000.png"))
            .unwrap();
        for index in 1..120 {
            std::fs::copy(&raw, folder.join(format!("capture{index:04}.dng"))).unwrap();
            std::fs::copy(
                folder.join("capture0000.png"),
                folder.join(format!("capture{index:04}.png")),
            )
            .unwrap();
        }
        let roots = vec![folder.to_string_lossy().into_owned()];
        let input = crate::contract::ipc::ScanArchiveInput {
            name: "local-adapter".into(),
            roots,
        };
        let scan = crate::style_commands::scan_archive(&state, &input).unwrap();
        assert!(scan.enough);
        let (_, archives) = load(&state, &input.name).unwrap();
        assert_eq!(
            archives
                .iter()
                .map(|a| pairs::match_all(a).pairs.len())
                .sum::<usize>(),
            120
        );
        let trained = crate::style_commands::train_scanned_profile(
            &state,
            &crate::contract::ipc::TrainProfileInput {
                name: input.name.clone(),
                cancel_id: Some("local-archive-test".into()),
            },
        )
        .unwrap();
        assert_eq!(trained.matched, 120);
        assert_eq!(trained.accepted, 120);
        assert!(!trained.cancelled);
        assert!(trained.profile.is_some());
        // This fixture proves the production file adapter and command wiring,
        // not generalisation: all 120 files contain the same authored chart.
        if let Ok(destination) = std::env::var("AURA_HOT_TRAIN_DATASET") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for entry in std::fs::read_dir(&folder).unwrap() {
                let entry = entry.unwrap();
                std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
            }
        }
        crate::style_commands::scan_archive(&state, &input).unwrap();
        assert!(
            state
                .style_store()
                .pairs(&input.name, false)
                .unwrap()
                .is_empty(),
            "rescanning must invalidate fits of earlier exports"
        );
        assert!(
            !state.style_store().profiles().unwrap().is_empty(),
            "rescan preserves trained profiles"
        );
    }

    #[test]
    fn missing_scan_and_ambiguous_names_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(&dir.path().join("catalog.sqlite")).unwrap();
        assert!(load(&state, "missing").is_err());
        let first = dir.path().join("a");
        let second = dir.path().join("b");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("same.dng"), b"test").unwrap();
        std::fs::write(second.join("same.dng"), b"test").unwrap();
        let mut found = BTreeMap::new();
        paths(&first, &mut found).unwrap();
        assert!(paths(&second, &mut found).is_err());
    }
}
