//! Backup destinations: copy a sealed delivery somewhere else, and check what arrived.
//!
//! ## The read-back is `aura_export::verify`'s, not a second copy of it
//!
//! "Did the bytes survive" is one question with one answer. A second implementation of it in the
//! crate that copies files is a second answer that can disagree with the first about the same file,
//! and the two would be found to disagree exactly once - on the wedding where it mattered.
//!
//! ## Three outcomes and not two
//!
//! **Verified**: copied, read back, digest matched.
//! **Already present**: the destination holds a file with this name and *the same digest*, so
//! nothing was copied. The ordinary case on a re-run, and it is cheap - a hash rather than a copy.
//! **Diverged**: the destination holds a file with this name and a *different* digest. This
//! **halts**, and nothing is overwritten.
//!
//! Divergence halting is the one decision in this module worth arguing about, and the argument is
//! the same as `AURA-RENDER-8022`'s with one addition: a backup that silently contains a different
//! file from the original is worse than no backup, because somebody will restore from it. The
//! photographer needs to find out which file, and why, before another 700 are copied over a drive
//! that is doing this.

use std::fs;
use std::path::Path;

use aura_core::contract::delivery::{DeliveryCode, DeliveryOutline, DeliveryReason, ExportedFile};
use aura_core::AuraResult;
use aura_export::verify::{hash_file, write_and_verify};

use crate::errors::{backup_diverged, unreachable};

/// What one file's backup produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    /// Where it landed, relative to the backup root.
    pub rel_path: std::path::PathBuf,
    /// Its size.
    pub bytes: u64,
    /// The digest of the copy, read back from the destination.
    pub hash: String,
    /// Whether the destination already held it.
    pub already_present: bool,
    /// What the panel says.
    pub reasons: Vec<DeliveryReason>,
}

/// Copy one file to a backup destination, verifying what arrives.
///
/// # Errors
///
/// `AURA-DLV-10003` when the destination holds a different file under this name, or when the copy
/// does not read back the same. `AURA-DLV-10002` when the destination cannot be reached.
pub fn copy_one(source_root: &Path, backup_root: &Path, file: &ExportedFile) -> AuraResult<Copied> {
    let src = source_root.join(&file.path);
    let dst = backup_root.join(&file.path);

    // Already there? Compare digests rather than copying. A hash is a read; a copy is a read and a
    // write, and on a 700-frame wedding re-run the difference is minutes.
    if dst.exists() {
        let existing = hash_file(&dst)?;
        // The source's own digest, when the export verified it. An unverified export has none, so
        // the source is hashed here - which is the only place in this crate that hashes a file the
        // export did not.
        let expected = if file.hash.len() == 64 {
            file.hash.clone()
        } else {
            hash_file(&src)?
        };
        if existing == expected {
            return Ok(Copied {
                rel_path: file.path.clone(),
                bytes: fs::metadata(&dst).map_or(file.bytes, |m| m.len()),
                hash: existing,
                already_present: true,
                reasons: vec![DeliveryReason::plain(DeliveryCode::BackupAlreadyPresent)],
            });
        }
        // A different file under this name. Nothing is overwritten and the backup stops.
        return Err(backup_diverged(&file.path.to_string_lossy()));
    }

    let bytes =
        fs::read(&src).map_err(|e| unreachable(format!("cannot read `{}`: {e}", src.display())))?;
    // The read-back is the export's, deliberately. See the note at the top of this module.
    let written = write_and_verify(&dst, &bytes, true)?;

    Ok(Copied {
        rel_path: file.path.clone(),
        bytes: written.bytes,
        hash: written.hash,
        already_present: false,
        reasons: vec![DeliveryReason::plain(DeliveryCode::BackupVerified)],
    })
}

/// Copy a whole delivery, stopping on the first divergence.
///
/// # Errors
///
/// `AURA-DLV-10003` on a divergence or a failed read-back, `AURA-DLV-10002` when the destination
/// goes away mid-job.
pub fn copy_all(
    source_root: &Path,
    backup_root: &Path,
    files: &[ExportedFile],
) -> AuraResult<(Vec<Copied>, DeliveryOutline)> {
    aura_export::verify::check_destination(backup_root)
        .map_err(|e| unreachable(format!("backup destination refused: {}", e.detail)))?;

    let mut copied = Vec::with_capacity(files.len());
    let mut outline = DeliveryOutline {
        files: u32::try_from(files.len()).unwrap_or(u32::MAX),
        backups: 1,
        ..DeliveryOutline::default()
    };

    for file in files {
        let one = copy_one(source_root, backup_root, file)?;
        outline.backed_up = outline.backed_up.saturating_add(1);
        outline.bytes_sent =
            outline
                .bytes_sent
                .saturating_add(if one.already_present { 0 } else { one.bytes });
        copied.push(one);
    }

    Ok((copied, outline))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::delivery::ImageId;
    use std::path::PathBuf;

    fn file(name: &str, bytes: &[u8]) -> ExportedFile {
        ExportedFile {
            image: ImageId::new(),
            set: "gallery".to_owned(),
            path: PathBuf::from("gallery").join(name),
            bytes: bytes.len() as u64,
            hash: blake3::hash(bytes).to_hex().to_string(),
            width: 100,
            height: 100,
            render_hash: "a".repeat(64),
            verified: true,
            renamed: false,
            reasons: Vec::new(),
        }
    }

    fn place(root: &Path, file: &ExportedFile, bytes: &[u8]) {
        let p = root.join(&file.path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    #[test]
    fn a_delivery_is_copied_and_every_copy_is_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("delivery");
        let dst = dir.path().join("backup");
        let files: Vec<ExportedFile> = (0..3)
            .map(|i| {
                let bytes = vec![i as u8; 100 + i];
                let f = file(&format!("a{i}.jpg"), &bytes);
                place(&src, &f, &bytes);
                f
            })
            .collect();

        let (copied, outline) = copy_all(&src, &dst, &files).unwrap();
        assert_eq!(copied.len(), 3);
        assert_eq!(outline.backed_up, 3);
        assert_eq!(outline.diverged, 0);
        for (c, f) in copied.iter().zip(files.iter()) {
            assert!(!c.already_present);
            assert_eq!(c.hash, f.hash, "the copy hashes to what the source did");
            assert!(dst.join(&f.path).exists());
        }
    }

    #[test]
    fn a_file_the_backup_already_holds_identically_is_not_copied_again() {
        // The ordinary case on a re-run, and it is a hash rather than a copy - which on a
        // 700-frame wedding is minutes.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("delivery");
        let dst = dir.path().join("backup");
        let bytes = vec![4_u8; 200];
        let f = file("a.jpg", &bytes);
        place(&src, &f, &bytes);
        place(&dst, &f, &bytes);

        let one = copy_one(&src, &dst, &f).unwrap();
        assert!(one.already_present);
        assert!(one
            .reasons
            .iter()
            .any(|r| r.code == DeliveryCode::BackupAlreadyPresent));
    }

    #[test]
    fn a_backup_holding_a_different_file_under_the_same_name_halts_and_overwrites_nothing() {
        // The decision worth arguing about. A backup that silently contains a different file from
        // the original is worse than no backup, because somebody will restore from it.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("delivery");
        let dst = dir.path().join("backup");
        let bytes = vec![4_u8; 200];
        let f = file("a.jpg", &bytes);
        place(&src, &f, &bytes);
        place(&dst, &f, b"a completely different photograph");

        let err = copy_one(&src, &dst, &f).expect_err("diverged");
        assert_eq!(err.code.0, "AURA-DLV-10003");
        assert_eq!(
            fs::read(dst.join(&f.path)).unwrap(),
            b"a completely different photograph",
            "nothing was overwritten"
        );
    }

    #[test]
    fn a_divergence_stops_the_whole_backup_rather_than_the_one_file() {
        // Because the fault is the drive's rather than the file's, which means the next three
        // hundred files are at the same risk.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("delivery");
        let dst = dir.path().join("backup");
        let mut files = Vec::new();
        for i in 0..4 {
            let bytes = vec![i as u8; 50];
            let f = file(&format!("a{i}.jpg"), &bytes);
            place(&src, &f, &bytes);
            files.push(f);
        }
        place(&dst, &files[2], b"wrong");

        assert!(copy_all(&src, &dst, &files).is_err());
        assert!(!dst.join(&files[3].path).exists(), "it stopped");
    }

    #[test]
    fn an_unreachable_backup_destination_is_refused_before_anything_is_copied() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("delivery");
        let blocked = dir.path().join("blocked");
        fs::write(&blocked, b"not a directory").unwrap();
        let bytes = vec![1_u8; 10];
        let f = file("a.jpg", &bytes);
        place(&src, &f, &bytes);
        let err = copy_all(&src, &blocked, &[f]).expect_err("refused");
        assert_eq!(err.code.0, "AURA-DLV-10002");
    }

    #[test]
    fn an_unverified_export_still_backs_up_by_hashing_its_source() {
        // A job that ran without the read-back has no stored digest. The backup still checks what
        // arrived; it just has to compute the source's digest itself, which is the one place in
        // this crate that hashes a file the export did not.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("delivery");
        let dst = dir.path().join("backup");
        let bytes = vec![6_u8; 80];
        let mut f = file("a.jpg", &bytes);
        f.hash = String::new();
        f.verified = false;
        place(&src, &f, &bytes);
        place(&dst, &f, &bytes);

        let one = copy_one(&src, &dst, &f).unwrap();
        assert!(one.already_present);
    }
}
