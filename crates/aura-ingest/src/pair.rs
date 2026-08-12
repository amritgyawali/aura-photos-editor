//! Grouping files into photographs. A RAW, its camera JPEG and its sidecar are
//! one photograph, not three grid cells.

use std::collections::BTreeMap;

use crate::contract::ingest::{FileKind, FileRole};
use crate::scan::ScannedFile;

/// A photograph and every file that represents it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoGroup {
    /// Index into the scanned slice of the file the pipeline will read.
    pub primary: usize,
    /// Every other file, with the role it plays.
    pub companions: Vec<(usize, FileRole)>,
}

/// Group by directory plus stem.
///
/// `IMG_0421.CR2`, `IMG_0421.JPG` and `IMG_0421.xmp` in one folder are one
/// photograph. The same stem in a different folder is a different photograph,
/// because photographers use folders to mean something.
///
/// RAW wins as primary. If there is no RAW, the largest image wins. A sidecar is
/// never primary. Ties are broken by the sorted scan order, never by filesystem
/// order.
#[must_use]
pub fn group(files: &[ScannedFile]) -> Vec<PhotoGroup> {
    let mut buckets: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();

    for (idx, f) in files.iter().enumerate() {
        let dir = f
            .rel_path
            .rsplit_once('/')
            .map_or(String::new(), |(dir, _)| dir.to_string());
        let stem = crate::util::grouping_stem(&f.file_name);
        buckets
            .entry((dir, crate::util::case_fold_key(&stem)))
            .or_default()
            .push(idx);
    }

    let mut groups = Vec::with_capacity(buckets.len());
    for members in buckets.into_values() {
        let primary = members
            .iter()
            .copied()
            .filter(|i| {
                files
                    .get(*i)
                    .is_some_and(|f| f.kind != FileKind::SidecarXmp)
            })
            .max_by_key(|i| {
                let f = files.get(*i);
                // A RAW must beat any plausible file size, so a 60 MB RAW wins
                // over a 90 MB TIFF derivative without a branchy comparator.
                let raw_bonus = u64::from(f.is_some_and(|f| f.kind == FileKind::Raw)) << 40;
                raw_bonus + f.map_or(0, |f| f.size_bytes)
            });

        let Some(primary) = primary else {
            // Sidecar with no image: keep it, attached to nothing, so a later
            // import of the RAW can adopt it. Never discard photographer data.
            continue;
        };

        let companions = members
            .iter()
            .copied()
            .filter(|i| *i != primary)
            .filter_map(|i| {
                files.get(i).map(|f| {
                    let role = match f.kind {
                        FileKind::SidecarXmp => FileRole::Sidecar,
                        FileKind::Jpeg | FileKind::Heif => FileRole::CompanionJpeg,
                        _ => FileRole::Derivative,
                    };
                    (i, role)
                })
            })
            .collect();

        groups.push(PhotoGroup {
            primary,
            companions,
        });
    }

    groups
}
