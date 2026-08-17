//! Camera profiles: which matrix renders which body, and what happens when none does.
//!
//! The table is `config/camera_profiles.toml`, a versioned product-owned file in the same
//! shape as phase 09's calibration and phase 12's weights. The matrices themselves come from
//! `aura_raw::colour::profile`, which already holds the bundled bench bodies and the
//! documented fallback - two copies of a colour matrix is two renderers that drift apart
//! while looking identical.
//!
//! # An unknown body renders, and says so
//!
//! `resolve` never returns nothing. A body that is not in the table renders through the
//! reference profile and the caller receives `AURA-RENDER-8008` at `Degraded`. That is the
//! correct failure mode: a photograph that looks slightly flat and is labelled uncalibrated
//! beats a photograph rendered confidently through a matrix somebody guessed.
//!
//! # Changing a matrix invalidates renders
//!
//! [`PROFILES_VER`] is in every render hash, so a table change makes every stored hash
//! stale rather than making a stale hash silently wrong. That is the same argument phase 09
//! made for `calib_ver` and phase 12 for the config digest, one layer down.

use std::collections::BTreeMap;

use aura_core::errors::render::profile_unknown;
use aura_core::AuraError;
use aura_raw::colour::matrix::Mat3;
use aura_raw::colour::profile::{self, CameraProfile, ProfileSource};
use aura_raw::colour::working_space::camera_to_working;

/// The version of the shipped table. Bump on any matrix change.
pub const PROFILES_VER: u16 = 1;

/// The table as shipped, so a caller can report what this build knows about.
const TABLE: &str = include_str!("../config/camera_profiles.toml");

/// A camera profile resolved for one photograph.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The profile itself, in DNG convention.
    pub profile: CameraProfile,
    /// `camera native RGB -> linear Rec.2020 at D65`, ready for the pixel loop.
    pub camera_to_working: [[f32; 3]; 3],
    /// The warning to surface when the body was not in the table.
    pub warning: Option<AuraError>,
}

impl Resolved {
    /// True when this rendered through the documented fallback rather than a measurement.
    #[must_use]
    pub fn is_reference(&self) -> bool {
        self.profile.source == ProfileSource::Generic
    }
}

/// Resolve the profile for a body, never failing.
///
/// `camera` is EXIF's model string. The make is not separated out here because
/// `Recipe::image.camera` carries one string; `aura_raw::colour::profile::lookup` is
/// case-insensitive and trims, and a model string is unique enough across the bodies this
/// product will ever ship a matrix for.
#[must_use]
pub fn resolve(camera: &str) -> Resolved {
    let looked_up =
        profile::lookup(profile::BENCH_MAKE, camera).or_else(|| profile::lookup("", camera));

    let (profile, warning) = if let Some(found) = looked_up {
        (found, None)
    } else {
        let mut fallback = profile::generic();
        fallback.model = camera.trim().to_string();
        let warning = if camera.trim().is_empty() {
            None
        } else {
            Some(profile_unknown(camera))
        };
        (fallback, warning)
    };

    let white = profile::illuminant_white(&profile);
    let matrix =
        camera_to_working(profile.xyz_to_camera, white).map_or(identity(), crate::colour::narrow);

    Resolved {
        profile,
        camera_to_working: matrix,
        warning,
    }
}

fn identity() -> [[f32; 3]; 3] {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

/// Every body this build carries a matrix for, model to note.
///
/// Read from the shipped table rather than from the code, so that the file and the binary
/// cannot disagree about what is calibrated.
#[must_use]
pub fn known_bodies() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(parsed) = toml::from_str::<toml::Value>(TABLE) else {
        return out;
    };
    let Some(rows) = parsed.get("bench").and_then(toml::Value::as_array) else {
        return out;
    };
    for row in rows {
        let model = row.get("model").and_then(toml::Value::as_str).unwrap_or("");
        let note = row.get("note").and_then(toml::Value::as_str).unwrap_or("");
        if !model.is_empty() {
            out.insert(model.to_string(), note.to_string());
        }
    }
    out
}

/// The table's declared version, read from the file.
///
/// Compared against [`PROFILES_VER`] by `the_shipped_table_matches_the_constant`, so a
/// matrix change that forgets the bump fails the build rather than silently reusing stale
/// render hashes.
#[must_use]
pub fn declared_version() -> u16 {
    toml::from_str::<toml::Value>(TABLE)
        .ok()
        .and_then(|v| v.get("profiles_ver").and_then(toml::Value::as_integer))
        .and_then(|v| u16::try_from(v).ok())
        .unwrap_or(0)
}

/// The reference matrix, for the parity harness and the golden suite.
#[must_use]
pub fn reference_matrix() -> Mat3 {
    profile::generic().xyz_to_camera
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_matches_the_constant() {
        assert_eq!(
            declared_version(),
            PROFILES_VER,
            "changing a matrix without bumping profiles_ver would reuse stale render hashes"
        );
    }

    #[test]
    fn every_bench_body_resolves_without_a_warning() {
        let bodies = known_bodies();
        assert_eq!(bodies.len(), 8, "eight synthetic bench bodies");
        for model in bodies.keys() {
            let resolved = resolve(model);
            assert!(resolved.warning.is_none(), "{model} should be known");
            assert!(!resolved.is_reference(), "{model} should have a matrix");
        }
    }

    #[test]
    fn an_unknown_body_renders_through_the_reference_and_warns() {
        let resolved = resolve("ILCE-7M4");
        assert!(resolved.is_reference());
        assert_eq!(
            resolved.warning.map(|e| e.code.to_string()),
            Some("AURA-RENDER-8008".to_string())
        );
    }

    #[test]
    fn an_empty_camera_string_does_not_warn() {
        // A photograph with no EXIF model is a phase 01 gap, not a colour gap, and raising
        // a colour code for it would send somebody looking in the wrong place.
        assert!(resolve("").warning.is_none());
    }

    #[test]
    fn two_bench_bodies_produce_genuinely_different_matrices() {
        let a = resolve("Bench-02").camera_to_working;
        let b = resolve("Bench-05").camera_to_working;
        let mut differs = false;
        for (row_a, row_b) in a.iter().zip(b.iter()) {
            for (x, y) in row_a.iter().zip(row_b.iter()) {
                if (x - y).abs() > 1e-4 {
                    differs = true;
                }
            }
        }
        assert!(differs, "the cross-body golden suite would prove nothing");
    }

    #[test]
    fn a_resolved_matrix_takes_a_neutral_camera_signal_to_a_neutral_working_pixel() {
        for model in ["Bench-01", "Bench-04", "Bench-08"] {
            let resolved = resolve(model);
            let out = crate::colour::apply_f32(resolved.camera_to_working, [0.5; 3]);
            assert!(
                (out[0] - out[1]).abs() < 5e-3 && (out[1] - out[2]).abs() < 5e-3,
                "{model} tinted a neutral: {out:?}"
            );
        }
    }
}
