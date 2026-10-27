//! Which matrix turns *this camera's* numbers into colour.
//!
//! There are exactly four sources, tried in this order, and the one that was
//! used is always recorded in the sidecar so a photographer, a support engineer
//! or a re-training run can tell them apart:
//!
//! 1. **The file.** DNG files carry `ColorMatrix1`/`ColorMatrix2`, measured by
//!    whoever wrote the file. Nothing we bundle can beat that, so it wins.
//! 2. **The bundled table.** Profiles AURA has measured from a ColorChecker
//!    frame shot on that body, each one signed off by the Colour Scientist role.
//!    Today that is the eight synthetic bench bodies the golden suite renders.
//! 3. **The published table.** Adobe DNG Converter's own `ColorMatrix2` for about
//!    seven hundred real bodies - the matrix every converted DNG of that body
//!    carries - as compiled by dcraw and LibRaw (`data/camera_matrices.tsv`,
//!    distributed under CDDL-1.0, ADR-0124). It is a measurement, made by Adobe,
//!    of the same body; it is what path 1 would have read had the photographer
//!    converted the file to DNG first.
//! 4. **The generic matrix.** A documented sRGB-primaries assumption. It is
//!    honest, deterministic and wrong by a knowable amount, and it raises
//!    `AURA-RAW-2006` plus a `profile=generic` badge in the UI so nobody mistakes
//!    it for a measurement.
//!
//! What is deliberately *not* here: invented matrices for real cameras. Every
//! matrix this module returns was measured by somebody, and the sidecar says who.

use crate::colour::matrix::{Mat3, WHITE_D50};

/// Where a profile came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileSource {
    /// Read from the file's own colour matrix tags.
    EmbeddedDng,
    /// Looked up in the signed table bundled with this build.
    Bundled,
    /// Adobe's published matrix for this body, from the table in `data/`.
    Published,
    /// Nothing matched; the documented generic matrix was used.
    Generic,
}

impl ProfileSource {
    /// The text written into the sidecar's `camera_profile.matrix` field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmbeddedDng => "embedded_dng",
            Self::Bundled => "bundled",
            Self::Published => "published",
            Self::Generic => "generic",
        }
    }
}

/// One camera colour profile, in DNG's convention: the matrix maps CIE XYZ under
/// the stated illuminant **into** camera native RGB. The pipeline inverts it.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraProfile {
    /// Manufacturer exactly as EXIF spells it.
    pub make: String,
    /// Model exactly as EXIF spells it.
    pub model: String,
    /// `XYZ(illuminant) -> camera native RGB`.
    pub xyz_to_camera: Mat3,
    /// The illuminant the matrix is defined for, for example `D50`.
    pub illuminant: &'static str,
    /// Which of the three sources produced it.
    pub source: ProfileSource,
    /// Who signed the measurement off. Empty for the generic fallback.
    pub signed_by: &'static str,
}

/// CIE XYZ (D50) to linear sRGB, Bradford adapted. The base of the generic
/// profile and of every bench body.
const XYZ_D50_TO_SRGB: Mat3 = [
    [3.133_856_1, -1.616_866_7, -0.490_614_6],
    [-0.978_768_4, 1.916_141_5, 0.033_454_0],
    [0.071_945_3, -0.228_991_4, 1.405_242_7],
];

/// The direction each bench body is pushed away from the base matrix. Adding
/// `k * CROSS_TALK` to the base leaves the matrix invertible and neutral-safe
/// while giving each body a genuinely different colour response, which is what
/// makes a cross-body ColorChecker test mean something.
const CROSS_TALK: Mat3 = [
    [0.0, 0.120, -0.120],
    [-0.100, 0.0, 0.100],
    [0.080, -0.080, 0.0],
];

/// The eight synthetic bench bodies the golden and ColorChecker suites use.
/// `k` is the cross-talk coefficient; the sign alternates so half the bodies
/// are warm-biased and half cool-biased.
const BENCH_BODIES: &[(&str, f64)] = &[
    ("Bench-01", 0.00),
    ("Bench-02", 0.05),
    ("Bench-03", -0.05),
    ("Bench-04", 0.10),
    ("Bench-05", -0.10),
    ("Bench-06", 0.15),
    ("Bench-07", -0.15),
    ("Bench-08", 0.20),
];

/// The make every bench body reports. Never a real manufacturer, so a bundled
/// bench profile can never be mistaken for a measured camera profile.
pub const BENCH_MAKE: &str = "AURA";

fn bench_matrix(k: f64) -> Mat3 {
    let mut out = XYZ_D50_TO_SRGB;
    for row in 0..3 {
        for column in 0..3 {
            let delta = CROSS_TALK
                .get(row)
                .and_then(|r| r.get(column))
                .copied()
                .unwrap_or(0.0);
            if let Some(slot) = out.get_mut(row).and_then(|r| r.get_mut(column)) {
                *slot += k * delta;
            }
        }
    }
    out
}

/// Every profile this build ships, in a stable order.
#[must_use]
pub fn bundled() -> Vec<CameraProfile> {
    BENCH_BODIES
        .iter()
        .map(|(model, k)| CameraProfile {
            make: BENCH_MAKE.to_string(),
            model: (*model).to_string(),
            xyz_to_camera: bench_matrix(*k),
            illuminant: "D50",
            source: ProfileSource::Bundled,
            signed_by: "COL (synthetic bench body, exact by construction)",
        })
        .collect()
}

/// Look a body up in the bundled table. Matching is case-insensitive and
/// trims surrounding whitespace, because EXIF strings are padded inconsistently
/// across brands; it is never fuzzy, because the wrong profile is worse than no
/// profile.
#[must_use]
pub fn lookup(make: &str, model: &str) -> Option<CameraProfile> {
    let make_key = make.trim().to_ascii_lowercase();
    let model_key = model.trim().to_ascii_lowercase();
    bundled().into_iter().find(|profile| {
        profile.make.trim().to_ascii_lowercase() == make_key
            && profile.model.trim().to_ascii_lowercase() == model_key
    })
}

/// The published table, parsed once.
fn published_table() -> &'static [PublishedRow] {
    static TABLE: std::sync::OnceLock<Vec<PublishedRow>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| parse_published(include_str!("../../data/camera_matrices.tsv")))
}

/// One row of `data/camera_matrices.tsv`.
#[derive(Debug, Clone, PartialEq)]
struct PublishedRow {
    maker: String,
    prefix: String,
    /// `None` when the row exists only to stop a shorter prefix matching.
    matrix: Option<Mat3>,
}

fn parse_published(text: &str) -> Vec<PublishedRow> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let maker = fields.next()?.trim().to_string();
            let prefix = fields.next()?.trim().to_string();
            let _black = fields.next()?;
            let _maximum = fields.next()?;
            let values: Vec<f64> = fields
                .next()?
                .split_whitespace()
                .filter_map(|value| value.parse::<f64>().ok())
                .map(|value| value / 10_000.0)
                .collect();
            let at = |index: usize| values.get(index).copied().unwrap_or(0.0);
            let matrix = (values.len() == 9 && values.iter().any(|v| v.abs() > 0.0)).then(|| {
                [
                    [at(0), at(1), at(2)],
                    [at(3), at(4), at(5)],
                    [at(6), at(7), at(8)],
                ]
            });
            Some(PublishedRow {
                maker,
                prefix,
                matrix,
            })
        })
        .collect()
}

/// The table's name for whoever made a body, from EXIF's `Make`.
fn published_maker(make: &str, model: &str) -> Option<&'static str> {
    const MAKERS: [(&str, &str); 22] = [
        ("CANON", "Canon"),
        ("NIKON", "Nikon"),
        ("SONY", "Sony"),
        ("FUJI", "Fujifilm"),
        ("PANASONIC", "Panasonic"),
        ("OLYMPUS", "Olympus"),
        ("PENTAX", "Pentax"),
        ("RICOH", "Ricoh"),
        ("SAMSUNG", "Samsung"),
        ("LEICA", "Leica"),
        ("HASSELBLAD", "Hasselblad"),
        ("PHASE ONE", "PhaseOne"),
        ("SIGMA", "Sigma"),
        ("KODAK", "Kodak"),
        ("LEAF", "Leaf"),
        ("MINOLTA", "Minolta"),
        ("KONICA", "Minolta"),
        ("MAMIYA", "Mamiya"),
        ("CASIO", "Casio"),
        ("GOPRO", "GoPro"),
        ("APPLE", "Apple"),
        ("SINAR", "Sinar"),
    ];
    let make = make.trim().to_ascii_uppercase();
    let model = model.trim().to_ascii_uppercase();
    // Ricoh sells Pentax bodies under its own name; OM System sells Olympus ones.
    if model.starts_with("PENTAX") {
        return Some("Pentax");
    }
    if make.starts_with("OM DIGITAL") || make.starts_with("OM SYSTEM") {
        return Some("Olympus");
    }

    MAKERS
        .iter()
        .find(|(key, _)| make.contains(key))
        .map(|(_, name)| *name)
}

/// The model as the table spells it: EXIF's string without the maker's name
/// in front ("NIKON D780" is "D780", "Canon EOS R" is "EOS R").
fn published_model(make: &str, model: &str, maker: &str) -> String {
    let mut model = model.trim().to_string();
    let first_word = make
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    for prefix in [first_word.as_str(), &maker.to_ascii_uppercase(), "PENTAX"] {
        if prefix.is_empty() {
            continue;
        }
        if model.to_ascii_uppercase().starts_with(prefix) {
            model = model.get(prefix.len()..).unwrap_or("").trim().to_string();
            break;
        }
    }
    model
}

/// Adobe's published matrix for a body, when the table has one.
///
/// The table is matched the way its compilers match it: the first row whose
/// model prefix starts the model wins, so "EOS R5" is found before "EOS R".
#[must_use]
pub fn published(make: &str, model: &str) -> Option<CameraProfile> {
    let maker = published_maker(make, model)?;
    let wanted = published_model(make, model, maker).to_ascii_lowercase();
    if wanted.is_empty() {
        return None;
    }
    let row = published_table()
        .iter()
        .filter(|row| row.maker == maker)
        .find(|row| wanted.starts_with(&row.prefix.to_ascii_lowercase()))?;
    let matrix = row.matrix?;
    crate::colour::matrix::invert(matrix)?;
    Some(CameraProfile {
        make: make.trim().to_string(),
        model: model.trim().to_string(),
        xyz_to_camera: matrix,
        illuminant: "D65",
        source: ProfileSource::Published,
        signed_by: "Adobe DNG Converter (ColorMatrix2), via LibRaw's table",
    })
}

/// The documented fallback: sRGB primaries under D50.
#[must_use]
pub fn generic() -> CameraProfile {
    CameraProfile {
        make: String::new(),
        model: String::new(),
        xyz_to_camera: XYZ_D50_TO_SRGB,
        illuminant: "D50",
        source: ProfileSource::Generic,
        signed_by: "",
    }
}

/// Resolve a profile for one file.
///
/// `embedded` is whatever the container yielded, already in DNG convention.
/// The return value is never absent: the pipeline must always be able to render,
/// and the caller inspects `source` to decide whether to raise `AURA-RAW-2006`.
#[must_use]
pub fn resolve(make: &str, model: &str, embedded: Option<Mat3>) -> CameraProfile {
    if let Some(matrix) = embedded {
        // A matrix that cannot be inverted is not a profile, it is noise; fall
        // through to the table rather than rendering through a singular matrix.
        if crate::colour::matrix::invert(matrix).is_some() {
            return CameraProfile {
                make: make.trim().to_string(),
                model: model.trim().to_string(),
                xyz_to_camera: matrix,
                illuminant: "D50",
                source: ProfileSource::EmbeddedDng,
                signed_by: "file",
            };
        }
    }
    if let Some(found) = lookup(make, model) {
        return found;
    }
    if let Some(found) = published(make, model) {
        return found;
    }
    let mut fallback = generic();
    fallback.make = make.trim().to_string();
    fallback.model = model.trim().to_string();
    fallback
}

/// The white point a profile's matrix is defined against.
#[must_use]
pub fn illuminant_white(profile: &CameraProfile) -> crate::colour::matrix::Vec3 {
    match profile.illuminant {
        "D65" => crate::colour::matrix::WHITE_D65,
        _ => WHITE_D50,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod published_tests {
    use super::*;

    #[test]
    fn the_published_table_finds_real_bodies_by_their_exif_names() {
        for (make, model) in [
            ("Canon", "Canon EOS 5D Mark IV"),
            ("Canon", "Canon EOS R"),
            ("SONY", "ILCE-7M3"),
            ("SONY", "ILCE-7M4"),
            ("NIKON CORPORATION", "NIKON D780"),
            ("NIKON CORPORATION", "NIKON Z 50"),
            ("FUJIFILM", "X-T4"),
            ("Panasonic", "DC-S5"),
        ] {
            let found = resolve(make, model, None);
            assert_eq!(found.source, ProfileSource::Published, "{make} {model}");
            assert_eq!(found.illuminant, "D65");
        }
        assert_eq!(
            resolve("Acme", "Wedding-o-tron", None).source,
            ProfileSource::Generic
        );
    }

    #[test]
    fn a_longer_model_is_matched_before_its_prefix() {
        let r5 = published("Canon", "Canon EOS R5").unwrap();
        let r = published("Canon", "Canon EOS R").unwrap();
        assert_ne!(r5.xyz_to_camera, r.xyz_to_camera);
        // The EOS R's matrix as Adobe publishes it.
        assert!((r.xyz_to_camera[0][0] - 0.8293).abs() < 1e-9);
    }
}
