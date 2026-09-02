//! The delivery manifest: the document that says what was delivered.
//!
//! Two copies, because they answer different questions. The catalog's copy is what the panel reads
//! and what a re-run compares against; `aura-delivery-manifest.json` beside the files is what
//! survives the catalog being lost - which is exactly the situation in which somebody needs to know
//! what was delivered. Phase 14 made the same call about edit recipes and the sidecars beside the
//! RAWs.
//!
//! ## What is in it that a file listing does not have
//!
//! Four things, and all four are about a delivery rather than about files.
//!
//! `qc_report_path` points at phase 27's archived report, so a photographer handing a gallery to a
//! second shooter hands over what was checked. `cleanup_disclosures` carries phase 24's removals,
//! because a removal that is not disclosed in the thing handed to the client is a removal nobody
//! can audit. `engine_versions` is what makes the whole gallery reproducible from four values a
//! year later. And `verify` records whether the read-back ran, so a delivery that skipped it can
//! never be mistaken for one that did not.
//!
//! ## The document is canonical, and its own digest is stored
//!
//! Keys in a fixed order, no trailing whitespace, one file per line. Two identical deliveries
//! produce two identical manifests, which is what makes the stored `manifest_hash` a check on the
//! document rather than a timestamp of it - and a manifest somebody edited detectable.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use aura_core::contract::delivery::{DeliveryManifest, ExportedFile, ImageId, MANIFEST_NAME};
use aura_core::{AuraResult, ProjectId};

use crate::verify::write_and_verify;

/// Assemble a manifest from files that were **read back**.
///
/// Takes `ExportedFile`s rather than paths, which is what makes "a manifest is a record of files
/// somebody read back" a property of the type rather than a convention: there is no constructor
/// here that takes a directory listing.
#[must_use]
pub fn assemble(
    project: ProjectId,
    created_at: i64,
    files: &[ExportedFile],
    sets: &[(String, u32)],
    qc_report_path: Option<PathBuf>,
    engine_versions: Vec<(String, String)>,
) -> DeliveryManifest {
    let mut disclosures: Vec<(ImageId, String)> = Vec::new();
    for f in files {
        for r in &f.reasons {
            if r.code == aura_core::contract::delivery::DeliveryCode::CleanupDisclosed {
                if let Some(detail) = &r.detail {
                    disclosures.push((f.image, detail.clone()));
                }
            }
        }
    }
    disclosures.sort_by(|a, b| a.0.to_db().cmp(&b.0.to_db()).then(a.1.cmp(&b.1)));
    disclosures.dedup();

    DeliveryManifest {
        project,
        created_at,
        files: files
            .iter()
            .map(|f| (f.path.clone(), f.bytes, f.hash.clone()))
            .collect(),
        sets: sets.to_vec(),
        qc_report_path,
        cleanup_disclosures: disclosures,
        engine_versions,
    }
}

/// Render a manifest as its canonical document.
///
/// Hand-written rather than `serde_json`, for the reason phase 29's exporter is: the document is a
/// published format that another studio's software will parse, and a derived serialisation changes
/// shape whenever a field is added to the struct. Every key here is written on purpose.
#[must_use]
pub fn to_document(manifest: &DeliveryManifest, verify: bool) -> String {
    let esc = |s: &str| {
        let mut out = String::with_capacity(s.len() + 2);
        for ch in s.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    let _ = write!(out, "\\u{:04x}", c as u32);
                }
                c => out.push(c),
            }
        }
        out
    };
    // Paths are written with forward slashes whatever produced them, because this document is read
    // on a different machine from the one that wrote it as often as not.
    let slash = |p: &Path| esc(&p.to_string_lossy().replace('\\', "/"));

    let mut out = String::with_capacity(manifest.files.len() * 120 + 512);
    out.push_str("{\n");
    let _ = writeln!(out, "  \"schema\": \"aura.delivery-manifest/1\",");
    let _ = writeln!(out, "  \"project\": \"{}\",", manifest.project.to_db());
    let _ = writeln!(out, "  \"created_at\": {},", manifest.created_at);
    let _ = writeln!(out, "  \"verified\": {verify},");
    let _ = writeln!(out, "  \"file_count\": {},", manifest.files.len());
    let _ = writeln!(out, "  \"total_bytes\": {},", manifest.total_bytes());

    out.push_str("  \"sets\": [\n");
    for (i, (name, count)) in manifest.sets.iter().enumerate() {
        let comma = if i + 1 == manifest.sets.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(
            out,
            "    {{ \"name\": \"{}\", \"files\": {count} }}{comma}",
            esc(name)
        );
    }
    out.push_str("  ],\n");

    out.push_str("  \"files\": [\n");
    for (i, (path, bytes, hash)) in manifest.files.iter().enumerate() {
        let comma = if i + 1 == manifest.files.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(
            out,
            "    {{ \"path\": \"{}\", \"bytes\": {bytes}, \"blake3\": \"{}\" }}{comma}",
            slash(path),
            esc(hash)
        );
    }
    out.push_str("  ],\n");

    match &manifest.qc_report_path {
        Some(p) => {
            let _ = writeln!(out, "  \"qc_report\": \"{}\",", slash(p));
        }
        None => out.push_str("  \"qc_report\": null,\n"),
    }

    out.push_str("  \"cleanup_disclosures\": [\n");
    for (i, (image, what)) in manifest.cleanup_disclosures.iter().enumerate() {
        let comma = if i + 1 == manifest.cleanup_disclosures.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(
            out,
            "    {{ \"image\": \"{}\", \"removed\": \"{}\" }}{comma}",
            image.to_db(),
            esc(what)
        );
    }
    out.push_str("  ],\n");

    out.push_str("  \"engine_versions\": {\n");
    for (i, (k, v)) in manifest.engine_versions.iter().enumerate() {
        let comma = if i + 1 == manifest.engine_versions.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(out, "    \"{}\": \"{}\"{comma}", esc(k), esc(v));
    }
    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

/// Write the travelling copy beside the delivery, and hash it.
///
/// The manifest is verified like every other file this phase writes, because a manifest that did
/// not land is a delivery with no record of itself.
///
/// # Errors
///
/// `AURA-RENDER-8023` when it cannot be written, `AURA-RENDER-8022` when it does not read back.
pub fn seal(
    root: &Path,
    manifest: &DeliveryManifest,
    verify: bool,
) -> AuraResult<(PathBuf, String)> {
    let doc = to_document(manifest, verify);
    let path = root.join(MANIFEST_NAME);
    let written = write_and_verify(&path, doc.as_bytes(), true)?;
    Ok((path, written.hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::delivery::{DeliveryCode, DeliveryReason};

    fn file(name: &str, hash: &str) -> ExportedFile {
        ExportedFile {
            image: ImageId::new(),
            set: "gallery".to_owned(),
            path: PathBuf::from("gallery").join(name),
            bytes: 1234,
            hash: hash.to_owned(),
            width: 4000,
            height: 3000,
            render_hash: "a".repeat(64),
            verified: true,
            renamed: false,
            reasons: Vec::new(),
        }
    }

    #[test]
    fn a_manifest_is_valid_json_that_another_tool_can_parse() {
        // Section 13's "delivery manifest" is a document somebody else's software reads. A format
        // nobody parsed is a format nobody has checked.
        let files = vec![
            file("a.jpg", &"1".repeat(64)),
            file("b.jpg", &"2".repeat(64)),
        ];
        let m = assemble(
            ProjectId::new(),
            1_760_000_000_000,
            &files,
            &[("gallery".to_owned(), 2)],
            Some(PathBuf::from("qc/report.json")),
            vec![("app".to_owned(), "0.1.0".to_owned())],
        );
        let doc = to_document(&m, true);
        let parsed: serde_json::Value = serde_json::from_str(&doc).expect("valid json");
        assert_eq!(parsed["schema"], "aura.delivery-manifest/1");
        assert_eq!(parsed["file_count"], 2);
        assert_eq!(parsed["total_bytes"], 2468);
        assert_eq!(parsed["verified"], true);
        assert_eq!(parsed["files"][0]["blake3"], "1".repeat(64));
        assert_eq!(parsed["engine_versions"]["app"], "0.1.0");
        assert!(m.fully_hashed());
    }

    #[test]
    fn a_manifest_records_whether_the_read_back_ran() {
        let m = assemble(
            ProjectId::new(),
            0,
            &[file("a.jpg", &"1".repeat(64))],
            &[("gallery".to_owned(), 1)],
            None,
            Vec::new(),
        );
        let doc = to_document(&m, false);
        let parsed: serde_json::Value = serde_json::from_str(&doc).unwrap();
        assert_eq!(parsed["verified"], false);
    }

    #[test]
    fn disclosures_are_lifted_out_of_the_files_that_carry_them() {
        // A removal that is not disclosed in the thing handed to the client is a removal nobody
        // can audit.
        let mut f = file("a.jpg", &"1".repeat(64));
        f.reasons.push(DeliveryReason::with(
            DeliveryCode::CleanupDisclosed,
            "an exit sign was removed from the background",
        ));
        let m = assemble(
            ProjectId::new(),
            0,
            &[f],
            &[("gallery".to_owned(), 1)],
            None,
            Vec::new(),
        );
        assert_eq!(m.cleanup_disclosures.len(), 1);
        let doc = to_document(&m, true);
        assert!(doc.contains("an exit sign was removed"));
    }

    #[test]
    fn two_identical_deliveries_produce_identical_documents() {
        // What makes the stored manifest_hash a check on the document rather than a timestamp of
        // it, and a manifest somebody edited detectable.
        let files = vec![file("a.jpg", &"1".repeat(64))];
        let project = ProjectId::new();
        let a = to_document(
            &assemble(
                project,
                42,
                &files,
                &[("g".to_owned(), 1)],
                None,
                Vec::new(),
            ),
            true,
        );
        let b = to_document(
            &assemble(
                project,
                42,
                &files,
                &[("g".to_owned(), 1)],
                None,
                Vec::new(),
            ),
            true,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn a_windows_path_travels_with_forward_slashes() {
        // This document is read on a different machine from the one that wrote it as often as not.
        let mut f = file("a.jpg", &"1".repeat(64));
        f.path = PathBuf::from("gallery\\sub\\a.jpg");
        let m = assemble(
            ProjectId::new(),
            0,
            &[f],
            &[("g".to_owned(), 1)],
            None,
            Vec::new(),
        );
        let doc = to_document(&m, true);
        let parsed: serde_json::Value = serde_json::from_str(&doc).unwrap();
        assert_eq!(parsed["files"][0]["path"], "gallery/sub/a.jpg");
    }

    #[test]
    fn a_studio_name_with_a_quote_in_it_does_not_break_the_document() {
        let m = assemble(
            ProjectId::new(),
            0,
            &[file("a.jpg", &"1".repeat(64))],
            &[("the \"gallery\"\n".to_owned(), 1)],
            None,
            Vec::new(),
        );
        let doc = to_document(&m, true);
        let parsed: serde_json::Value = serde_json::from_str(&doc).expect("escaped");
        assert_eq!(parsed["sets"][0]["name"], "the \"gallery\"\n");
    }

    #[test]
    fn the_sealed_manifest_is_itself_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let m = assemble(
            ProjectId::new(),
            0,
            &[file("a.jpg", &"1".repeat(64))],
            &[("g".to_owned(), 1)],
            None,
            Vec::new(),
        );
        let (path, hash) = seal(dir.path(), &m, true).unwrap();
        assert!(path.exists());
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, crate::verify::hash_file(&path).unwrap());
    }
}
