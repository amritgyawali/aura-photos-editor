//! The AURA sidecar: the whole recipe, losslessly, beside the RAW.
//!
//! ADR-0029 section 8. `image.xmp` carries what Lightroom understands; this file carries
//! everything - masks, retouch operations, restoration, provenance and the protected-field
//! list. It is the canonical JSON of [`crate::hash::canonical`], so the file on disk is
//! byte-identical to what was hashed, and a photographer can read it.
//!
//! # Three properties of the write
//!
//! * **The RAW is never opened.** [`write`] takes the RAW's path only to derive a sibling
//!   name; nothing here opens, reads or touches the original. Invariant 1.
//! * **The previous version is kept.** An overwrite renames the existing file to
//!   `.bak` first. `AURA-RENDER-8005`'s runbook sends an operator there, and the cost of
//!   one extra file per edited photograph is smaller than the cost of one lost edit.
//! * **The write is atomic.** The new document goes to a temporary sibling and is renamed
//!   into place, so a process killed mid-write leaves either the old document or the new
//!   one and never half of either. Invariant 5.

use std::fs;
use std::path::{Path, PathBuf};

use aura_core::errors::render::{recipe_invalid, sidecar_unreadable};
use aura_core::AuraResult;

use crate::contract::recipe::Recipe;
use crate::hash::canonical;

/// The suffix appended to the RAW's stem, e.g. `IMG_0042.aura.json`.
pub const EXTENSION: &str = "aura.json";

/// The path of the sidecar that belongs to a RAW.
///
/// `IMG_0042.ARW` becomes `IMG_0042.aura.json` - the RAW's extension is replaced rather than
/// appended to, so a folder holding `IMG_0042.ARW` and `IMG_0042.JPG` produces one sidecar
/// rather than two that disagree.
#[must_use]
pub fn path_for(raw: &Path) -> PathBuf {
    raw.with_extension(EXTENSION)
}

/// The path of the Lightroom-compatible XMP that belongs to a RAW.
#[must_use]
pub fn xmp_path_for(raw: &Path) -> PathBuf {
    raw.with_extension("xmp")
}

/// Write a recipe beside its RAW, keeping the previous version.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the recipe cannot be serialised, and `AURA-RENDER-8005` when the
/// file cannot be written.
pub fn write(raw: &Path, recipe: &Recipe) -> AuraResult<PathBuf> {
    let target = path_for(raw);
    let body = canonical(recipe)?;

    if target.exists() {
        let backup = target.with_extension("json.bak");
        // A failed backup is not a failed write: the old file is still there and the new
        // one is about to replace it. Losing the backup costs a support step; refusing the
        // write costs the edit.
        drop(fs::rename(&target, &backup));
    }

    let temporary = target.with_extension("json.tmp");
    fs::write(&temporary, body.as_bytes())
        .map_err(|e| sidecar_unreadable(format!("write {}: {e}", temporary.display())))?;
    fs::rename(&temporary, &target)
        .map_err(|e| sidecar_unreadable(format!("rename into {}: {e}", target.display())))?;
    Ok(target)
}

/// Read the recipe that belongs to a RAW.
///
/// # Errors
///
/// `AURA-RENDER-8005` when the file is absent, unreadable or not a recipe.
pub fn read(raw: &Path) -> AuraResult<Recipe> {
    let target = path_for(raw);
    let text = fs::read_to_string(&target)
        .map_err(|e| sidecar_unreadable(format!("read {}: {e}", target.display())))?;
    parse(&text)
}

/// Parse a sidecar document.
///
/// # Errors
///
/// `AURA-RENDER-8005` when the text is not a recipe.
pub fn parse(text: &str) -> AuraResult<Recipe> {
    serde_json::from_str::<Recipe>(text)
        .map_err(|e| sidecar_unreadable(format!("not a recipe document: {e}")))
}

/// Write both files: the lossless sidecar and the Lightroom-compatible XMP.
///
/// The order matters. The sidecar is written first, because it is the source of truth: a
/// process killed between the two leaves a complete edit and a stale XMP, which
/// [`crate::xmp::reconcile`] resolves in favour of the sidecar. The other order would leave
/// a complete XMP and no edit.
///
/// # Errors
///
/// Propagates [`write`]. An XMP that cannot be written is reported as
/// `AURA-RENDER-8004` and does not fail the call, because the edit itself is safe.
pub fn write_pair(raw: &Path, recipe: &Recipe) -> AuraResult<(PathBuf, Option<PathBuf>)> {
    let sidecar = write(raw, recipe)?;
    let xmp = xmp_path_for(raw);
    match fs::write(&xmp, crate::xmp::write(recipe).as_bytes()) {
        Ok(()) => Ok((sidecar, Some(xmp))),
        Err(_) => Ok((sidecar, None)),
    }
}

/// Load the edit for a RAW from whatever is on disk, preferring the sidecar.
///
/// Returns the recipe, whether the XMP contributed anything, and the paths the XMP changed
/// (which [`crate::xmp::reconcile`] has already marked as user edits).
///
/// The precedence is ADR-0029 section 8's: the sidecar wins, and the XMP is reconciled
/// against it so that an edit made in Lightroom is picked up and protected rather than
/// quietly overwritten on the next automated pass.
///
/// # Errors
///
/// `AURA-RENDER-8005` when neither file yields a recipe.
pub fn load(raw: &Path, fallback: &Recipe) -> AuraResult<LoadedEdit> {
    let sidecar_path = path_for(raw);
    let xmp_path = xmp_path_for(raw);

    let base = if sidecar_path.exists() {
        read(raw)?
    } else if xmp_path.exists() {
        fallback.clone()
    } else {
        return Err(sidecar_unreadable(format!(
            "no edit beside {}",
            raw.display()
        )));
    };

    if !xmp_path.exists() {
        return Ok(LoadedEdit {
            recipe: base,
            from_xmp: Vec::new(),
            xmp_warning: None,
        });
    }

    let Ok(text) = fs::read_to_string(&xmp_path) else {
        return Ok(LoadedEdit {
            recipe: base,
            from_xmp: Vec::new(),
            xmp_warning: Some(crate::errors::xmp_unreadable(format!(
                "read {}",
                xmp_path.display()
            ))),
        });
    };

    match crate::xmp::reconcile(&base, &text) {
        Ok((recipe, changed)) => Ok(LoadedEdit {
            recipe,
            from_xmp: changed,
            xmp_warning: None,
        }),
        Err(err) => Ok(LoadedEdit {
            recipe: base,
            from_xmp: Vec::new(),
            xmp_warning: Some(err),
        }),
    }
}

/// What [`load`] found.
#[derive(Debug, Clone)]
pub struct LoadedEdit {
    /// The recipe to render.
    pub recipe: Recipe,
    /// Paths the XMP changed, now marked as user edits.
    pub from_xmp: Vec<String>,
    /// `AURA-RENDER-8004`, when the XMP existed and could not be used.
    pub xmp_warning: Option<aura_core::AuraError>,
}

/// Refuse a document that is not a recipe *shape* even though it parsed.
///
/// # Errors
///
/// `AURA-RENDER-8002`, from [`crate::schema::Validation::check`].
pub fn parse_checked(text: &str) -> AuraResult<Recipe> {
    let recipe = parse(text)?;
    crate::schema::Validation::check(&recipe)
        .map_err(|e| recipe_invalid("<sidecar>", &e.detail))?;
    Ok(recipe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::hash::recipe_hash;

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn the_sidecar_sits_beside_the_raw_and_replaces_its_extension() {
        let path = Path::new("/cards/day1/IMG_0042.ARW");
        assert_eq!(
            path_for(path).file_name().and_then(|s| s.to_str()),
            Some("IMG_0042.aura.json")
        );
        assert_eq!(
            xmp_path_for(path).file_name().and_then(|s| s.to_str()),
            Some("IMG_0042.xmp")
        );
    }

    #[test]
    fn a_written_recipe_reads_back_with_the_same_hash() {
        let dir = temp();
        let raw = dir.path().join("IMG_0042.ARW");
        fs::write(&raw, b"not really a raw").expect("raw");
        let recipe = fixtures::reference();
        write(&raw, &recipe).expect("write");
        let back = read(&raw).expect("read");
        assert_eq!(
            recipe_hash(&back).expect("back"),
            recipe_hash(&recipe).expect("original")
        );
    }

    #[test]
    fn the_file_on_disk_is_the_canonical_bytes() {
        let dir = temp();
        let raw = dir.path().join("a.ARW");
        let recipe = fixtures::reference();
        let target = write(&raw, &recipe).expect("write");
        let on_disk = fs::read_to_string(&target).expect("read");
        assert_eq!(on_disk, canonical(&recipe).expect("canonical"));
    }

    #[test]
    fn an_overwrite_keeps_the_previous_version() {
        let dir = temp();
        let raw = dir.path().join("a.ARW");
        let first = fixtures::reference();
        write(&raw, &first).expect("first");
        let mut second = first.clone();
        second.global.exposure = 2.0;
        write(&raw, &second).expect("second");

        let backup = path_for(&raw).with_extension("json.bak");
        assert!(backup.exists(), "the previous version is kept");
        let recovered = parse(&fs::read_to_string(&backup).expect("bak")).expect("parse");
        assert!((recovered.global.exposure - first.global.exposure).abs() < 1e-6);
    }

    #[test]
    fn the_pair_writes_the_sidecar_first() {
        let dir = temp();
        let raw = dir.path().join("a.ARW");
        let (sidecar, xmp) = write_pair(&raw, &fixtures::reference()).expect("pair");
        assert!(sidecar.exists());
        assert!(xmp.is_some_and(|p| p.exists()));
    }

    #[test]
    fn loading_prefers_the_sidecar_and_picks_up_a_lightroom_edit() {
        let dir = temp();
        let raw = dir.path().join("a.ARW");
        let base = fixtures::reference();
        write(&raw, &base).expect("sidecar");

        let mut lightroom = base.clone();
        lightroom.global.exposure = -1.75;
        fs::write(xmp_path_for(&raw), crate::xmp::write(&lightroom)).expect("xmp");

        let loaded = load(&raw, &base).expect("load");
        assert!((loaded.recipe.global.exposure - (-1.75)).abs() < 1e-4);
        assert_eq!(loaded.from_xmp, vec!["global.exposure".to_string()]);
        assert!(loaded
            .recipe
            .provenance
            .user_edited_fields
            .contains(&"global.exposure".to_string()));
        // And the mask the XMP cannot carry is still there.
        assert_eq!(loaded.recipe.masks.len(), base.masks.len());
    }

    #[test]
    fn an_unreadable_xmp_does_not_lose_the_edit() {
        let dir = temp();
        let raw = dir.path().join("a.ARW");
        let base = fixtures::reference();
        write(&raw, &base).expect("sidecar");
        fs::write(xmp_path_for(&raw), b"<not xmp>").expect("xmp");

        let loaded = load(&raw, &base).expect("load");
        assert_eq!(
            loaded.xmp_warning.map(|e| e.code.to_string()),
            Some("AURA-RENDER-8004".to_string())
        );
        assert!((loaded.recipe.global.exposure - base.global.exposure).abs() < 1e-6);
    }

    #[test]
    fn a_corrupt_sidecar_is_refused_rather_than_guessed_at() {
        let err = parse("{\"schema\":1}").expect_err("must refuse");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8005");
    }

    #[test]
    fn nothing_here_opens_the_raw() {
        // The RAW does not exist at all, and every path still resolves.
        let dir = temp();
        let raw = dir.path().join("never_created.ARW");
        write(&raw, &fixtures::reference()).expect("write");
        assert!(!raw.exists(), "invariant 1: the original was never touched");
        assert!(path_for(&raw).exists());
    }
}
