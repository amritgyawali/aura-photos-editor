//! Which matrix turns *this camera's* numbers into colour.
//!
//! There are exactly three sources, tried in this order, and the one that was
//! used is always recorded in the sidecar so a photographer, a support engineer
//! or a re-training run can tell them apart:
//!
//! 1. **The file.** DNG files carry `ColorMatrix1`/`ColorMatrix2`, measured by
//!    whoever wrote the file. Nothing we bundle can beat that, so it wins.
//! 2. **The bundled table.** Profiles AURA has measured from a ColorChecker
//!    frame shot on that body, each one signed off by the Colour Scientist role.
//! 3. **The generic matrix.** A documented sRGB-primaries assumption. It is
//!    honest, deterministic and wrong by a knowable amount, and it raises
//!    `AURA-RAW-2006` plus a `profile=generic` badge in the UI so nobody mistakes
//!    it for a measurement.
//!
//! What is deliberately *not* here: invented matrices for real cameras. An
//! unsigned profile cannot ship (phase 02 section 6.5), so the bundled table
//! contains only bodies with evidence behind them. Today that is the eight
//! synthetic bench bodies the golden suite renders, whose matrices are exact by
//! construction; every real body a photographer owns therefore takes path 1 when
//! it shoots DNG and path 3 otherwise, until a ColorChecker frame arrives.

use crate::colour::matrix::{Mat3, WHITE_D50};

/// Where a profile came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileSource {
    /// Read from the file's own colour matrix tags.
    EmbeddedDng,
    /// Looked up in the signed table bundled with this build.
    Bundled,
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
