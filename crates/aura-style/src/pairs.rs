//! Matching a photographer's originals to the finals they delivered.
//!
//! PHASE-17 section 2.1 names four strategies and section 8 step 1 says to implement all of
//! them "including perceptual matching for renamed exports". They are tried strongest first
//! and the method that succeeded is recorded on the pair, because it is the single most useful
//! number in a support conversation about a messy archive: an archive matched entirely by
//! content hash and an archive matched entirely perceptually are two very different levels of
//! confidence in everything downstream.
//!
//! ## Why the order is the policy
//!
//! 1. **Content hash.** Some exporters write the source file's digest into the final's
//!    metadata. When it is there it is exact and free, and nothing else can beat it.
//! 2. **Filename stem.** `IMG_4471.CR3` and `IMG_4471.jpg`. Nearly always right, and
//!    occasionally catastrophic - two cameras that both started at `IMG_0001` in the same
//!    folder. The ambiguity check below refuses a stem that matches more than one original
//!    rather than picking the first, because picking the first is how a profile silently
//!    learns from the wrong photograph.
//! 3. **Capture time.** Exact to the second, and a wedding at 10 fps has fourteen frames in
//!    that second. So a time match is only accepted when it is *unique* - the same rule as the
//!    stem, for the same reason, and phase 08 learned it the hard way when whole-second EXIF
//!    could not separate a burst.
//! 4. **Perceptual.** A 64-bit difference hash, Hamming distance below
//!    [`PERCEPTUAL_MAX_HAMMING`]. This is the one that handles a renamed, re-timed export, and
//!    it is last because it is the only one that can match two photographs that are merely
//!    *similar* - the second and third frame of a burst are four bits apart.
//!
//! Every strategy after the first is confirmed by the fit: a pair that was matched wrongly
//! produces a residual no parameter setting can reduce, and `fit::REJECT_DE00` throws it out.
//! That is the real defence, and it is why an occasionally wrong matcher is acceptable and a
//! silently wrong one is not.
//!
//! ## What this module never does
//!
//! It never opens a photograph. Everything here is names, digests, timestamps and 64-bit
//! hashes, supplied by whoever walked the folder. That keeps the matching testable without
//! camera files - which is the difference between this phase having measured gates and having
//! none.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::style::MatchMethod;
use aura_core::AuraResult;

/// The largest Hamming distance between two difference hashes that still counts as a match.
///
/// Eight bits of sixty-four. Phase 05's near-duplicate label sits at ten and is deliberately
/// looser, because it is answering "are these the same shot"; this is answering "is this final
/// a render of this original", where the two images are the same photograph and differ only by
/// a grade. Eight leaves room for a heavy grade and stops two frames of one burst matching.
pub const PERCEPTUAL_MAX_HAMMING: u32 = 8;

/// How far apart two capture times may be and still be the same photograph, in milliseconds.
///
/// Two seconds. EXIF's `DateTimeOriginal` has whole-second resolution and an exporter may
/// round rather than truncate, so a one-second window would drop a matched pair on a boundary.
pub const CAPTURE_WINDOW_MS: i64 = 2_000;

/// The file extensions treated as originals.
///
/// The formats `aura_raw::format::sniff` recognises, plus DNG. A folder of JPEGs with no RAWs
/// is not an archive this phase can learn from - the whole method is *the difference between
/// what the camera recorded and what the photographer delivered* - and saying so at the scan
/// is much clearer than discovering it after two thousand failed fits.
pub const ORIGINAL_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "nef", "nrw", "arw", "orf", "raf", "rw2", "dng", "pef", "srw",
];

/// The file extensions treated as delivered finals.
pub const FINAL_EXTENSIONS: &[&str] = &["jpg", "jpeg", "tif", "tiff", "png"];

/// One file the scan found, and everything about it that matching needs.
///
/// **No pixels and no absolute path.** The name is the file name without its directory, which
/// is enough for a report and a support conversation and not enough to reconstruct where
/// anybody's wedding lives. Migration 17 stores exactly these two strings for the same reason.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArchiveFile {
    /// The file name, without its directory.
    pub name: String,
    /// The name with its extension and any export suffix removed, lower-cased.
    pub stem: String,
    /// BLAKE3 of the bytes, when it was computed.
    pub hash: Option<String>,
    /// What the final's metadata says its source was, when it says anything.
    pub source_hash: Option<String>,
    /// Capture time in milliseconds since the Unix epoch, when EXIF had one.
    pub captured_ms: Option<i64>,
    /// A 64-bit difference hash, when one was computed.
    pub dhash: Option<u64>,
}

impl ArchiveFile {
    /// A file named by its path, with the stem derived.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        let name = name.into();
        let stem = stem_of(&name);
        Self {
            name,
            stem,
            ..Self::default()
        }
    }

    /// The same file with a content hash.
    #[must_use]
    pub fn with_hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = Some(hash.into());
        self
    }

    /// The same file with a capture time.
    #[must_use]
    pub const fn with_capture(mut self, ms: i64) -> Self {
        self.captured_ms = Some(ms);
        self
    }

    /// The same file with a difference hash.
    #[must_use]
    pub const fn with_dhash(mut self, dhash: u64) -> Self {
        self.dhash = Some(dhash);
        self
    }

    /// The same file with the source digest its metadata declared.
    #[must_use]
    pub fn with_source(mut self, hash: impl Into<String>) -> Self {
        self.source_hash = Some(hash.into());
        self
    }
}

/// One folder of a photographer's own work.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Archive {
    /// What to call this archive in a report, and the key section 10.1's robustness gate groups
    /// by. One wedding, one label.
    pub label: String,
    /// The camera originals.
    pub originals: Vec<ArchiveFile>,
    /// The delivered finals.
    pub finals: Vec<ArchiveFile>,
}

/// One original matched to one final.
#[derive(Debug, Clone, PartialEq)]
pub struct Matched {
    /// The original.
    pub original: ArchiveFile,
    /// The final.
    pub finished: ArchiveFile,
    /// How the two were found.
    pub method: MatchMethod,
    /// Which archive they came from.
    pub archive: String,
}

/// What one archive's matching produced.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MatchReport {
    /// The pairs, in the order the originals were listed.
    pub pairs: Vec<Matched>,
    /// Originals with no final. Usually a frame that was culled, which is normal and is not an
    /// error - a photographer delivers a fraction of what they shoot.
    pub unmatched_originals: Vec<String>,
    /// Finals with no original. **This one is worth reporting**: it means the RAWs are missing,
    /// on another disk, or in a format this build does not decode, and it is the commonest
    /// cause of a training run that finds nothing.
    pub unmatched_finals: Vec<String>,
    /// How many pairs each method found, for the report and for
    /// [`MatchReport::weakest_method`].
    pub by_method: BTreeMap<String, u32>,
}

impl MatchReport {
    /// The weakest method any pair in this archive needed.
    ///
    /// `max` over [`MatchMethod`]'s ordering, which is strongest-first, so this is the one
    /// number that answers "how much should I trust this archive's pairing".
    #[must_use]
    pub fn weakest_method(&self) -> MatchMethod {
        self.pairs
            .iter()
            .map(|pair| pair.method)
            .max()
            .unwrap_or(MatchMethod::Unmatched)
    }

    /// The fraction of finals that found an original, `0..1`.
    #[must_use]
    pub fn coverage(&self) -> f32 {
        let total = self.pairs.len() + self.unmatched_finals.len();
        if total == 0 {
            return 0.0;
        }
        self.pairs.len() as f32 / total as f32
    }
}

/// Match one archive's originals to its finals.
///
/// Deterministic: the originals are visited in the order they were listed and each strategy is
/// tried in the documented order. Invariant 4 reaches here because a profile fitted from a
/// different pairing is a different profile.
#[must_use]
pub fn match_all(archive: &Archive) -> MatchReport {
    let mut report = MatchReport::default();
    let mut taken: Vec<bool> = vec![false; archive.finals.len()];

    // Index the finals once per strategy rather than scanning per original. A four-thousand
    // frame wedding against a nine-hundred frame gallery is 3.6 million comparisons on the
    // naive path and it is the only part of the scan that is not dominated by I/O.
    let mut by_source: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut by_stem: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (position, file) in archive.finals.iter().enumerate() {
        if let Some(source) = file.source_hash.as_deref() {
            by_source.entry(source).or_default().push(position);
        }
        if !file.stem.is_empty() {
            by_stem
                .entry(file.stem.as_str())
                .or_default()
                .push(position);
        }
    }

    for original in &archive.originals {
        let found = find_by_source(original, &by_source, &archive.finals, &taken)
            .map(|index| (index, MatchMethod::ContentHash))
            .or_else(|| {
                find_by_stem(original, &by_stem, &taken).map(|i| (i, MatchMethod::FilenameStem))
            })
            .or_else(|| {
                find_by_capture(original, &archive.finals, &taken)
                    .map(|i| (i, MatchMethod::CaptureTime))
            })
            .or_else(|| {
                find_by_dhash(original, &archive.finals, &taken)
                    .map(|i| (i, MatchMethod::Perceptual))
            });

        match found {
            Some((index, method)) => {
                let Some(finished) = archive.finals.get(index) else {
                    report.unmatched_originals.push(original.name.clone());
                    continue;
                };
                if let Some(slot) = taken.get_mut(index) {
                    *slot = true;
                }
                *report
                    .by_method
                    .entry(method.as_str().to_string())
                    .or_insert(0) += 1;
                report.pairs.push(Matched {
                    original: original.clone(),
                    finished: finished.clone(),
                    method,
                    archive: archive.label.clone(),
                });
            }
            None => report.unmatched_originals.push(original.name.clone()),
        }
    }

    for (index, finished) in archive.finals.iter().enumerate() {
        if !taken.get(index).copied().unwrap_or(false) {
            report.unmatched_finals.push(finished.name.clone());
        }
    }
    report
}

/// Walk a directory and classify what is in it.
///
/// Names, stems and nothing else: hashes, capture times and difference hashes are filled in by
/// whoever can afford to open the files, which is the caller. **This function opens nothing**,
/// so a scan of a forty-thousand-file archive costs a directory walk rather than forty thousand
/// decodes, and the expensive work happens once per pair that actually matched.
///
/// # Errors
///
/// `AURA-IO-1001` when the directory cannot be read.
pub fn scan(root: &Path, label: &str) -> AuraResult<Archive> {
    if !root.is_dir() {
        return Err(aura_core::errors::io::not_found(root));
    }
    let mut archive = Archive {
        label: label.to_string(),
        ..Archive::default()
    };
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str() else {
            continue;
        };
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        let file = ArchiveFile::named(name);
        if ORIGINAL_EXTENSIONS.contains(&extension.as_str()) {
            archive.originals.push(file);
        } else if FINAL_EXTENSIONS.contains(&extension.as_str()) {
            archive.finals.push(file);
        }
    }
    Ok(archive)
}

/// The comparable stem of a file name.
///
/// The extension goes, then one trailing export suffix, then the case. Lightroom writes
/// `IMG_4471-Edit.jpg` and `IMG_4471-2.jpg`; Capture One writes `IMG_4471_1.jpg`. Stripping
/// exactly one suffix rather than every trailing token is deliberate: a photographer whose
/// files really are called `Smith-Wedding-2` would otherwise have every frame collapse onto one
/// stem.
#[must_use]
pub fn stem_of(name: &str) -> String {
    let base = name.rsplit_once('.').map_or(name, |(head, _)| head);
    let lower = base.to_ascii_lowercase();
    for suffix in ["-edit", "_edit", "-final", "_final", "-export", "_export"] {
        if let Some(head) = lower.strip_suffix(suffix) {
            return head.to_string();
        }
    }
    // A trailing `-2` or `_1` from a duplicate export. Only when what precedes it is not empty,
    // so a file genuinely called `2.jpg` keeps its name.
    if let Some((head, tail)) = lower.rsplit_once(['-', '_']) {
        if !head.is_empty() && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            // Only strip when the head still ends in something non-numeric. `IMG_4471` must
            // keep its number; `IMG_4471-2` must lose its suffix. The difference is whether
            // the head itself already ends in a digit run that looks like a frame number.
            if head.chars().filter(char::is_ascii_digit).count() >= 3 {
                return head.to_string();
            }
        }
    }
    lower
}

/// The Hamming distance between two difference hashes.
#[must_use]
pub const fn hamming(left: u64, right: u64) -> u32 {
    (left ^ right).count_ones()
}

fn find_by_source(
    original: &ArchiveFile,
    by_source: &BTreeMap<&str, Vec<usize>>,
    finals: &[ArchiveFile],
    taken: &[bool],
) -> Option<usize> {
    let hash = original.hash.as_deref()?;
    let candidates = by_source.get(hash)?;
    candidates.iter().copied().find(|position| {
        !taken.get(*position).copied().unwrap_or(false)
            && finals
                .get(*position)
                .and_then(|file| file.source_hash.as_deref())
                == Some(hash)
    })
}

fn find_by_stem(
    original: &ArchiveFile,
    by_stem: &BTreeMap<&str, Vec<usize>>,
    taken: &[bool],
) -> Option<usize> {
    if original.stem.is_empty() {
        return None;
    }
    let candidates = by_stem.get(original.stem.as_str())?;
    let free: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|position| !taken.get(*position).copied().unwrap_or(false))
        .collect();
    // Ambiguity is a refusal, not a coin toss. Two originals sharing a stem is two cameras
    // that both started at IMG_0001, and picking the first is how a profile learns from the
    // wrong photograph without anybody finding out.
    if free.len() == 1 {
        free.first().copied()
    } else {
        None
    }
}

fn find_by_capture(
    original: &ArchiveFile,
    finals: &[ArchiveFile],
    taken: &[bool],
) -> Option<usize> {
    let when = original.captured_ms?;
    let mut hit = None;
    for (position, file) in finals.iter().enumerate() {
        if taken.get(position).copied().unwrap_or(false) {
            continue;
        }
        let Some(theirs) = file.captured_ms else {
            continue;
        };
        if (theirs - when).abs() <= CAPTURE_WINDOW_MS {
            if hit.is_some() {
                // Not unique. A wedding at 10 fps has fourteen frames inside EXIF's own
                // one-second resolution, which is the defect phase 08 found and fixed.
                return None;
            }
            hit = Some(position);
        }
    }
    hit
}

fn find_by_dhash(original: &ArchiveFile, finals: &[ArchiveFile], taken: &[bool]) -> Option<usize> {
    let ours = original.dhash?;
    let mut best: Option<(u32, usize)> = None;
    for (position, file) in finals.iter().enumerate() {
        if taken.get(position).copied().unwrap_or(false) {
            continue;
        }
        let Some(theirs) = file.dhash else {
            continue;
        };
        let distance = hamming(ours, theirs);
        if distance > PERCEPTUAL_MAX_HAMMING {
            continue;
        }
        match best {
            Some((current, _)) if distance >= current => {}
            _ => best = Some((distance, position)),
        }
    }
    best.map(|(_, position)| position)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(originals: Vec<ArchiveFile>, finals: Vec<ArchiveFile>) -> Archive {
        Archive {
            label: "wedding".to_string(),
            originals,
            finals,
        }
    }

    #[test]
    fn a_declared_source_digest_beats_every_other_strategy() {
        let report = match_all(&archive(
            vec![ArchiveFile::named("A.cr3").with_hash("abc")],
            vec![
                ArchiveFile::named("A.jpg"),
                ArchiveFile::named("Z.jpg").with_source("abc"),
            ],
        ));
        assert_eq!(report.pairs.len(), 1);
        let Some(pair) = report.pairs.first() else {
            unreachable!("one pair was asserted")
        };
        assert_eq!(pair.method, MatchMethod::ContentHash);
        assert_eq!(pair.finished.name, "Z.jpg");
    }

    #[test]
    fn an_ambiguous_stem_is_refused_rather_than_guessed() {
        let report = match_all(&archive(
            vec![
                ArchiveFile::named("IMG_0001.cr3"),
                ArchiveFile::named("IMG_0001.arw"),
            ],
            vec![
                ArchiveFile::named("IMG_0001.jpg"),
                ArchiveFile::named("IMG_0001-2.jpg"),
            ],
        ));
        // Both finals share the stem, so neither original may claim one.
        assert!(report.pairs.is_empty(), "{:?}", report.pairs);
        assert_eq!(report.unmatched_originals.len(), 2);
    }

    #[test]
    fn an_export_suffix_does_not_break_the_stem_match() {
        let report = match_all(&archive(
            vec![ArchiveFile::named("IMG_4471.CR3")],
            vec![ArchiveFile::named("IMG_4471-Edit.jpg")],
        ));
        assert_eq!(report.pairs.len(), 1);
        let Some(pair) = report.pairs.first() else {
            unreachable!("one pair was asserted")
        };
        assert_eq!(pair.method, MatchMethod::FilenameStem);
    }

    #[test]
    fn a_capture_time_shared_by_two_finals_is_not_a_match() {
        let report = match_all(&archive(
            vec![ArchiveFile::named("a.nef").with_capture(1_000)],
            vec![
                ArchiveFile::named("x.jpg").with_capture(1_100),
                ArchiveFile::named("y.jpg").with_capture(1_200),
            ],
        ));
        assert!(report.pairs.is_empty());
    }

    #[test]
    fn a_renamed_export_is_found_perceptually_and_a_neighbouring_burst_frame_is_not() {
        let report = match_all(&archive(
            vec![ArchiveFile::named("a.nef").with_dhash(0xFFFF_0000_FFFF_0000)],
            vec![
                // Three bits different: the same photograph, graded.
                ArchiveFile::named("renamed.jpg").with_dhash(0xFFFF_0000_FFFF_0007),
                // Far away: a different frame.
                ArchiveFile::named("other.jpg").with_dhash(0x0000_FFFF_0000_FFFF),
            ],
        ));
        assert_eq!(report.pairs.len(), 1);
        let Some(pair) = report.pairs.first() else {
            unreachable!("one pair was asserted")
        };
        assert_eq!(pair.method, MatchMethod::Perceptual);
        assert_eq!(pair.finished.name, "renamed.jpg");
    }

    #[test]
    fn finals_with_no_original_are_reported_because_that_is_the_common_failure() {
        let report = match_all(&archive(
            vec![],
            vec![ArchiveFile::named("a.jpg"), ArchiveFile::named("b.jpg")],
        ));
        assert_eq!(report.unmatched_finals.len(), 2);
        assert!((report.coverage() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_weakest_method_is_what_the_report_leads_with() {
        let report = match_all(&archive(
            vec![
                ArchiveFile::named("a.cr3").with_hash("h1"),
                ArchiveFile::named("b.cr3").with_dhash(1),
            ],
            vec![
                ArchiveFile::named("a.jpg").with_source("h1"),
                ArchiveFile::named("zz.jpg").with_dhash(1),
            ],
        ));
        assert_eq!(report.pairs.len(), 2);
        assert_eq!(report.weakest_method(), MatchMethod::Perceptual);
    }

    #[test]
    fn a_final_is_claimed_once() {
        let report = match_all(&archive(
            vec![
                ArchiveFile::named("one.cr3").with_dhash(0),
                ArchiveFile::named("two.cr3").with_dhash(0),
            ],
            vec![ArchiveFile::named("only.jpg").with_dhash(0)],
        ));
        assert_eq!(report.pairs.len(), 1);
        assert_eq!(report.unmatched_originals.len(), 1);
    }

    #[test]
    fn stems_keep_a_frame_number_and_lose_a_duplicate_suffix() {
        assert_eq!(stem_of("IMG_4471.CR3"), "img_4471");
        assert_eq!(stem_of("IMG_4471-2.jpg"), "img_4471");
        assert_eq!(stem_of("2.jpg"), "2");
    }
}
