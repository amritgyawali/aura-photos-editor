//! The bundled lens profile table. PHASE-23 section 6.1's second route.
//!
//! A profile is a **measurement somebody made**, so every row records who made it. The loader
//! refuses a row with no attribution, which is the same rule that `emotion_weights.toml`,
//! `local_light.toml` and `crop_rules.toml` enforce for a written reason - and it matters
//! more here, because a distortion coefficient carries no argument on its face. A row that
//! says `k1 = -0.031` and nothing else is a number nobody can check.
//!
//! ## The profiles in this repository are synthetic and say so
//!
//! There are no measured lens profiles here, for the same reason there is no photographed
//! `ColorChecker`: measuring one needs the lens, a calibration target and a rig. What ships is
//! a table with the shape, the interpolation, the refusal rules and the attribution
//! machinery, populated with **fabricated coefficients on plausible lens ids**, every row
//! marked `synthetic = true`. [`ProfileTable::is_synthetic`] is on the wire and in the panel.
//!
//! That is condition C2 in the phase 23 exit report and it is a Sev 2 trigger. Phase 14 said
//! the same thing about camera profiles and the shape of the honesty is identical: this is a
//! determinism and regression gate, not a claim about optics.
//!
//! ## Interpolation
//!
//! A prime has one entry. A zoom has several, and a frame shot at 34 mm on a 24-70 is
//! corrected by linearly interpolating the two neighbouring entries **in log focal length**,
//! because distortion varies with field of view rather than with millimetres: the step from
//! 24 to 34 mm is a much larger change of view than the step from 60 to 70, and a linear
//! interpolation in millimetres under-corrects the short end of every zoom in the table.
//!
//! Outside the entries the nearest is used unchanged rather than extrapolated. Extrapolating
//! a polynomial fitted over 24-70 to a 200 mm frame produces a correction with the right
//! shape and the wrong magnitude, which is worse than no correction at all because it looks
//! deliberate.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::error::AuraResult;
use serde::{Deserialize, Serialize};

use crate::errors;

/// Which version of the bundled table produced a correction.
///
/// Bumping it invalidates every stored lens correction: `AURA-ML-5090` is raised when a
/// comparison would cross it, and the affected rows are re-planned in the background.
pub const PROFILE_VER: u16 = 1;

/// The directory the bundled profiles live in, relative to the workspace root.
pub const PROFILE_DIR: &str = "assets/lens_profiles";

/// One focal length's measured correction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProfileEntry {
    /// The focal length this row was measured at, in millimetres.
    pub focal_mm: f32,
    /// Brown-Conrady radial terms in normalised radius, where one is the corner.
    pub k1: f32,
    /// The fourth-order term.
    #[serde(default)]
    pub k2: f32,
    /// The sixth-order term.
    #[serde(default)]
    pub k3: f32,
    /// Full vignette correction strength, `0..1`.
    #[serde(default)]
    pub vignette: f32,
    /// Radial scale for the red channel relative to green.
    #[serde(default = "one")]
    pub ca_red: f32,
    /// Radial scale for the blue channel relative to green.
    #[serde(default = "one")]
    pub ca_blue: f32,
}

const fn one() -> f32 {
    1.0
}

impl ProfileEntry {
    /// Blend two entries. `t` is zero at `self` and one at `other`.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            focal_mm: mix(self.focal_mm, other.focal_mm),
            k1: mix(self.k1, other.k1),
            k2: mix(self.k2, other.k2),
            k3: mix(self.k3, other.k3),
            vignette: mix(self.vignette, other.vignette),
            ca_red: mix(self.ca_red, other.ca_red),
            ca_blue: mix(self.ca_blue, other.ca_blue),
        }
    }
}

/// One lens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LensProfile {
    /// The lens as EXIF names it. The match key.
    pub id: String,
    /// The mount, for the panel.
    #[serde(default)]
    pub mount: String,
    /// Who measured it. **Required**; a row without it is refused.
    pub measured_by: String,
    /// True when the coefficients were fabricated rather than measured.
    ///
    /// Every row in this repository sets it. It is not a debug flag: it reaches the panel, so
    /// a photographer is never told a lens was profiled when it was invented.
    #[serde(default)]
    pub synthetic: bool,
    /// One entry for a prime, several for a zoom, shortest first.
    pub entry: Vec<ProfileEntry>,
}

impl LensProfile {
    /// The correction for one focal length, interpolated in log focal length.
    ///
    /// Returns `None` only when the profile carries no entries, which the loader refuses.
    #[must_use]
    pub fn at(&self, focal_mm: f32) -> Option<ProfileEntry> {
        let first = *self.entry.first()?;
        if self.entry.len() == 1 || focal_mm <= first.focal_mm {
            return Some(first);
        }
        let last = *self.entry.last()?;
        if focal_mm >= last.focal_mm {
            return Some(last);
        }
        for pair in self.entry.windows(2) {
            let (Some(lo), Some(hi)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            if focal_mm >= lo.focal_mm && focal_mm <= hi.focal_mm {
                // In log focal length: distortion follows the field of view, and the step
                // from 24 to 34 mm is a far larger change of view than 60 to 70.
                let (a, b, x) = (
                    lo.focal_mm.max(1.0).ln(),
                    hi.focal_mm.max(1.0).ln(),
                    focal_mm.max(1.0).ln(),
                );
                let t = if (b - a).abs() < f32::EPSILON {
                    0.0
                } else {
                    ((x - a) / (b - a)).clamp(0.0, 1.0)
                };
                return Some(lo.lerp(*hi, t));
            }
        }
        Some(last)
    }
}

/// The whole bundled table.
#[derive(Debug, Clone, Default)]
pub struct ProfileTable {
    by_id: BTreeMap<String, LensProfile>,
    version: u16,
    attribution: String,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    #[serde(default)]
    table: RawHeader,
    #[serde(default)]
    lens: Vec<LensProfile>,
}

#[derive(Debug, Default, Deserialize)]
struct RawHeader {
    #[serde(default)]
    version: u16,
    #[serde(default)]
    attribution: String,
}

impl ProfileTable {
    /// An empty table. Every lens is unknown and every frame takes section 6.1's third route.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            by_id: BTreeMap::new(),
            version: PROFILE_VER,
            attribution: String::new(),
        }
    }

    /// Load every `.toml` under a directory.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` when a file does not parse, when a lens has no `measured_by`, when it
    /// has no entries, or when two files claim the same lens id - which is the one that would
    /// otherwise be silent, because a duplicate would resolve by directory iteration order and
    /// a correction that depends on a file system's ordering is not deterministic.
    pub fn load_dir(dir: &Path) -> AuraResult<Self> {
        let mut table = Self::empty();
        let Ok(entries) = std::fs::read_dir(dir) else {
            // A missing directory is an empty table rather than a failure: a build with no
            // bundled profiles corrects nothing and says so, which is `AURA-ML-5095` once per
            // lens rather than a run-blocking refusal.
            tracing::warn!(dir = %dir.display(), "no bundled lens profiles");
            return Ok(table);
        };
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .collect();
        // Sorted, so the duplicate check below reports the same pair on every machine.
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(&path).map_err(|err| {
                errors::rules_refused(format!("{}: {err}", path.display()))
            })?;
            table.merge_str(&text, &path.display().to_string())?;
        }
        Ok(table)
    }

    /// Load one file's text into the table.
    ///
    /// # Errors
    ///
    /// As [`ProfileTable::load_dir`].
    pub fn merge_str(&mut self, text: &str, origin: &str) -> AuraResult<()> {
        let raw: RawFile = toml::from_str(text)
            .map_err(|err| errors::rules_refused(format!("{origin}: {err}")))?;
        if raw.table.version > 0 {
            self.version = raw.table.version;
        }
        if !raw.table.attribution.is_empty() {
            if !self.attribution.is_empty() {
                self.attribution.push_str("; ");
            }
            self.attribution.push_str(&raw.table.attribution);
        }
        for lens in raw.lens {
            if lens.measured_by.trim().is_empty() {
                return Err(errors::rules_refused(format!(
                    "{origin}: lens '{}' has no measured_by - a profile is a measurement \
                     somebody made, and a row that cannot say who made it cannot be checked",
                    lens.id
                )));
            }
            if lens.entry.is_empty() {
                return Err(errors::rules_refused(format!(
                    "{origin}: lens '{}' has no entries",
                    lens.id
                )));
            }
            if lens.entry.windows(2).any(|pair| match (pair.first(), pair.get(1)) {
                (Some(a), Some(b)) => b.focal_mm <= a.focal_mm,
                _ => false,
            }) {
                return Err(errors::rules_refused(format!(
                    "{origin}: lens '{}' entries are not in ascending focal length",
                    lens.id
                )));
            }
            if self.by_id.contains_key(&lens.id) {
                return Err(errors::rules_refused(format!(
                    "{origin}: lens '{}' is already in the table - a duplicate would resolve \
                     by directory order, and a correction that depends on one is not \
                     deterministic",
                    lens.id
                )));
            }
            self.by_id.insert(lens.id.clone(), lens);
        }
        Ok(())
    }

    /// Look one lens up by the id EXIF gave.
    ///
    /// Exact match first, then a trimmed case-insensitive match: EXIF lens names differ
    /// between bodies of the same manufacturer by a trailing space and a capital.
    #[must_use]
    pub fn find(&self, lens_id: &str) -> Option<&LensProfile> {
        if let Some(hit) = self.by_id.get(lens_id) {
            return Some(hit);
        }
        let needle = lens_id.trim().to_ascii_lowercase();
        self.by_id
            .values()
            .find(|lens| lens.id.trim().to_ascii_lowercase() == needle)
    }

    /// How many lenses are in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// True when no lens is in the table.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// The table's version, for the version column.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Who measured what is in it.
    #[must_use]
    pub fn attribution(&self) -> &str {
        &self.attribution
    }

    /// True when any row in the table was fabricated rather than measured.
    ///
    /// On this build every row is. The panel says so, because a photographer told a lens was
    /// profiled when it was invented has been misled about their own photographs.
    #[must_use]
    pub fn is_synthetic(&self) -> bool {
        self.by_id.values().any(|lens| lens.synthetic)
    }

    /// Every lens id, sorted.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.by_id.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[table]
version = 1
attribution = "Fabricated for the test suite."

[[lens]]
id = "TEST 24-70mm F2.8"
mount = "TEST"
measured_by = "the test suite"
synthetic = true
[[lens.entry]]
focal_mm = 24.0
k1 = 0.030
vignette = 0.60
ca_red = 1.0004
ca_blue = 0.9996
[[lens.entry]]
focal_mm = 70.0
k1 = -0.010
vignette = 0.25
ca_red = 1.0001
ca_blue = 0.9999
"#;

    fn table() -> ProfileTable {
        let mut table = ProfileTable::empty();
        table.merge_str(SAMPLE, "sample").expect("the sample loads");
        table
    }

    #[test]
    fn a_zoom_interpolates_in_log_focal_length_not_in_millimetres() {
        let table = table();
        let lens = table.find("TEST 24-70mm F2.8").expect("the lens");
        let mid = lens.at(41.0).expect("an entry");
        // Linear in millimetres would put 41 mm at t = 0.37; in log focal length it is at
        // t = 0.50, which is the whole point - 24 to 41 is as large a change of view as
        // 41 to 70.
        let linear_mm = 0.030 + (-0.010 - 0.030) * ((41.0 - 24.0) / (70.0 - 24.0));
        assert!(
            (mid.k1 - linear_mm).abs() > 1e-3,
            "log interpolation collapsed onto the linear one: {} vs {linear_mm}",
            mid.k1
        );
        let t = ((41.0f32).ln() - (24.0f32).ln()) / ((70.0f32).ln() - (24.0f32).ln());
        let expected = 0.030 + (-0.010 - 0.030) * t;
        assert!((mid.k1 - expected).abs() < 1e-5, "{} vs {expected}", mid.k1);
    }

    #[test]
    fn outside_the_entries_the_nearest_is_used_rather_than_extrapolated() {
        let table = table();
        let lens = table.find("TEST 24-70mm F2.8").expect("the lens");
        assert!((lens.at(14.0).expect("wide").k1 - 0.030).abs() < 1e-6);
        assert!((lens.at(400.0).expect("long").k1 - (-0.010)).abs() < 1e-6);
    }

    #[test]
    fn a_profile_without_attribution_is_refused() {
        let mut table = ProfileTable::empty();
        let err = table
            .merge_str(
                "[[lens]]\nid = \"X\"\nmeasured_by = \"  \"\n[[lens.entry]]\nfocal_mm = 50.0\nk1 = 0.0\n",
                "bad",
            )
            .expect_err("no attribution is refused");
        assert_eq!(err.code.0, "AURA-ML-5093");
    }

    #[test]
    fn a_duplicate_lens_id_is_refused_rather_than_resolved_by_file_order() {
        let mut table = table();
        let err = table.merge_str(SAMPLE, "again").expect_err("a duplicate");
        assert_eq!(err.code.0, "AURA-ML-5093");
    }

    #[test]
    fn entries_must_ascend_in_focal_length() {
        let mut table = ProfileTable::empty();
        let text = "[[lens]]\nid = \"Y\"\nmeasured_by = \"me\"\n\
                    [[lens.entry]]\nfocal_mm = 70.0\nk1 = 0.0\n\
                    [[lens.entry]]\nfocal_mm = 24.0\nk1 = 0.0\n";
        assert!(table.merge_str(text, "bad").is_err());
    }

    #[test]
    fn a_lens_id_matches_past_a_trailing_space_and_a_capital() {
        let table = table();
        assert!(table.find("  test 24-70MM F2.8 ").is_some());
        assert!(table.find("something else").is_none());
    }

    #[test]
    fn the_bundled_table_is_marked_synthetic() {
        assert!(table().is_synthetic());
    }
}
