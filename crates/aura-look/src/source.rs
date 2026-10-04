//! Where reference photographs come from, and the one source this build cannot use.
//!
//! ## The Instagram question, answered once
//!
//! The feature a photographer asks for is "paste the link and go". This build accepts the link
//! and does not go, and the reason is two separate facts that happen to point the same way.
//!
//! The first is a property of this repository. `scripts/check-banned.sh` fails the build on an
//! outbound socket anywhere outside `aura-cloud`, and `aura-cloud`'s transport is a hand-written
//! HTTP/1.1 client with no TLS - ADR-0009 waived it - so there is no route from this process to
//! an `https://` host at all, for any purpose. That is not a gap this phase may quietly fill:
//! phase 04's rule is that the gateway is the only crate that opens a socket, and a
//! client-gallery fetcher is not a model provider.
//!
//! The second is about the platform rather than about us. Reading a page's media in bulk is
//! something Instagram grants through its own API to the account that owns the page, and the
//! unofficial routes - scraping the web view, replaying a private endpoint - are against its
//! terms, break without notice, and would put a photographer's own account at risk to save them
//! a folder drag.
//!
//! So [`MediaSource::PublicUrl`] refuses, [`aura_core::contract::look::refuse_fetch`] is the one
//! place that decides what the refusal says, and the two paths that *do* work are the two a
//! photographer can actually use today: a folder of photographs, and Instagram's own "Download
//! your information" export.
//!
//! **The link is not wasted.** [`ReferenceOrigin::parse`] validates it, and the handle is stored
//! beside the profile and rendered in every report - so "matched to @somebody" is a fact the
//! product knows, even though the bytes arrived by another route.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use aura_core::contract::error::AuraResult;
use aura_core::contract::look::{
    LookCode, LookReason, MediaSource, ReferenceOrigin, MAX_REFERENCES, MIN_REFERENCES,
};
use aura_core::errors::ml::look_reference_refused;
use aura_raw::codec::{decode_jpeg, Rgb8};
use aura_raw::timeout::DecodeLimits;

/// The extensions this build reads a reference photograph from.
///
/// JPEG and PNG only, and deliberately not RAW. A reference is somebody's *finished* work: a RAW
/// in a reference folder is an original nobody has graded, and measuring a look from one would
/// measure the camera. Phase 02's rule - pixels carry their provenance - in the phase where
/// mixing the two would be silent.
pub const READABLE: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

/// The sub-paths an Instagram data export keeps its posted media under.
///
/// Checked in order, and the walk falls back on the whole tree when none of them is there. The
/// layout has changed twice that this repository knows of, which is why it is a list rather than
/// a path and why missing it is a fallback rather than a refusal.
pub const EXPORT_MEDIA_PATHS: [&str; 3] = [
    "media/posts",
    "your_instagram_activity/media/posts",
    "content/posts",
];

/// One file that might be a reference photograph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFile {
    /// Where it is.
    pub path: PathBuf,
    /// Its content hash, which is also the reading's key.
    ///
    /// Content rather than path, so a photographer who has the same photograph in two folders
    /// gets one vote rather than two - and so that re-running a measurement over a folder whose
    /// files were renamed is the same measurement.
    pub key: String,
}

/// A reference, resolved: which page, how the files arrived, and which files they are.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    /// Which page or body of work.
    pub origin: ReferenceOrigin,
    /// How the files arrived.
    pub source: MediaSource,
    /// Where the walk started.
    pub root: PathBuf,
    /// The files, in a deterministic order.
    pub files: Vec<ReferenceFile>,
    /// What the walk found that it could not use, and what it noticed on the way.
    pub reasons: Vec<LookReason>,
}

impl Reference {
    /// How many usable photographs this reference holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True when the walk found nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Read one reference file into pixels.
///
/// # Errors
///
/// Whatever `aura-raw` raised. The caller records it as
/// [`LookCode::ReferenceUnreadable`] and carries on with the rest of the folder, because one
/// unreadable file in a folder of four hundred is not a reason to refuse a look.
pub fn decode(file: &ReferenceFile) -> AuraResult<Rgb8> {
    let bytes = std::fs::read(&file.path)
        .map_err(|error| look_reference_refused(format!("{}: {error}", file.path.display())))?;
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        aura_raw::codec::decode_png(&bytes, DecodeLimits::tier1())
    } else {
        decode_jpeg(&bytes, DecodeLimits::tier1())
    }
}

/// True when this path has an extension this build reads.
fn is_readable(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| READABLE.iter().any(|known| *known == extension))
}

/// Where inside an export directory the posted media is, when this looks like one.
#[must_use]
pub fn export_media_root(root: &Path) -> Option<PathBuf> {
    EXPORT_MEDIA_PATHS
        .iter()
        .map(|relative| root.join(relative))
        .find(|candidate| candidate.is_dir())
}

/// Resolve what a photographer pointed at into a set of files to measure.
///
/// The three arguments are the three separate facts: what they typed into the box, which route
/// they chose, and which folder they pointed at. `folder` is `None` exactly when the route is
/// [`MediaSource::PublicUrl`], which is the route that refuses.
///
/// # Errors
///
/// [`aura_core::errors::ml::ML_LOOK_REFERENCE_REFUSED`] when the address will not parse, when the
/// route is one this build cannot fetch through, when the folder is not a folder, or when it
/// holds fewer than [`MIN_REFERENCES`] readable photographs.
pub fn resolve(address: &str, source: MediaSource, folder: Option<&Path>) -> AuraResult<Reference> {
    let origin = if address.trim().is_empty() {
        ReferenceOrigin::Local {
            label: folder
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("A folder of photographs")
                .to_string(),
        }
    } else {
        ReferenceOrigin::parse(address)?
    };

    // The refusal, before anything else touches a disk. A source that cannot supply bytes is
    // not a walk that returns nothing - those are different facts and phase 24's rule is that
    // they must not render the same.
    if !source.can_fetch() {
        return Err(aura_core::contract::look::refuse_fetch(source));
    }

    let Some(root) = folder else {
        return Err(look_reference_refused(
            "no folder of reference photographs was given",
        ));
    };
    if !root.is_dir() {
        return Err(look_reference_refused(format!(
            "{} is not a folder",
            root.display()
        )));
    }

    let mut reasons = Vec::new();
    if matches!(
        origin,
        ReferenceOrigin::Instagram { .. } | ReferenceOrigin::Web { .. }
    ) {
        reasons.push(LookReason::bare(LookCode::OriginRecordedNotFetched));
    }

    // An export's own layout, when it has one. Falling back on the whole tree matters: an export
    // whose layout has moved still holds the photographs, three directories deeper.
    let walk_root = if matches!(source, MediaSource::InstagramExport) {
        match export_media_root(root) {
            Some(found) => {
                reasons.push(LookReason::bare(LookCode::InstagramExportLayout));
                found
            }
            None => root.to_path_buf(),
        }
    } else {
        root.to_path_buf()
    };

    let (files, walk_reasons) = walk(&walk_root);
    reasons.extend(walk_reasons);

    if u32::try_from(files.len()).unwrap_or(u32::MAX) < MIN_REFERENCES {
        return Err(look_reference_refused(format!(
            "{} holds {} photographs AURA can read, against a minimum of {MIN_REFERENCES}",
            walk_root.display(),
            files.len()
        )));
    }

    Ok(Reference {
        origin,
        source,
        root: walk_root,
        files,
        reasons,
    })
}

/// Every readable, distinct photograph under a root, in a deterministic order.
///
/// **Sorted by path before hashing**, so the order the filesystem happens to return entries in
/// cannot change which of two identical photographs is kept, which is what makes a second run
/// over one folder produce the same look. Phase 29 shipped a determinism defect of exactly this
/// shape and its lesson was that a determinism test has to compare the identifiers.
#[must_use]
pub fn walk(root: &Path) -> (Vec<ReferenceFile>, Vec<LookReason>) {
    let mut reasons = Vec::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut skipped_non_image = 0_u32;

    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.into_path();
        if is_readable(&path) {
            candidates.push(path);
        } else {
            skipped_non_image = skipped_non_image.saturating_add(1);
        }
    }
    candidates.sort();

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut files: Vec<ReferenceFile> = Vec::new();
    let mut duplicates = 0_u32;
    let mut unreadable = 0_u32;

    for path in candidates {
        if u32::try_from(files.len()).unwrap_or(u32::MAX) >= MAX_REFERENCES {
            reasons.push(LookReason::counted(
                LookCode::ReferenceLimitReached,
                MAX_REFERENCES as f32,
            ));
            break;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            unreadable = unreadable.saturating_add(1);
            continue;
        };
        let key = blake3::hash(&bytes).to_hex().to_string();
        if !seen.insert(key.clone()) {
            duplicates = duplicates.saturating_add(1);
            continue;
        }
        files.push(ReferenceFile { path, key });
    }

    if skipped_non_image > 0 {
        reasons.push(LookReason::counted(
            LookCode::ReferenceNotAnImage,
            f32::from(u16::try_from(skipped_non_image).unwrap_or(u16::MAX)),
        ));
    }
    if duplicates > 0 {
        reasons.push(LookReason::counted(
            LookCode::ReferenceDuplicate,
            f32::from(u16::try_from(duplicates).unwrap_or(u16::MAX)),
        ));
    }
    if unreadable > 0 {
        reasons.push(LookReason::counted(
            LookCode::ReferenceUnreadable,
            f32::from(u16::try_from(unreadable).unwrap_or(u16::MAX)),
        ));
    }

    (files, reasons)
}
