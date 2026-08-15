//! What "sharp" means on this body, and what "noisy" costs on it.
//!
//! PHASE-09 section 6.1 requires sharpness to be "normalised by camera calibration:
//! expected MTF50 for the body/lens/aperture combination, so 'sharp' means sharp *for
//! this gear*", and section 12's third failure mode is "camera bias favours
//! high-resolution bodies". Section 10.1's last gate makes it measurable: identical
//! scenes shot on a 24 MP and a 61 MP body must score within 0.05.
//!
//! This module is the denominator of every number in the phase.
//!
//! ## The counter-intuitive part, stated once
//!
//! [`Calibration::expected_mtf50`] **falls** as sensor resolution rises. That is not a
//! ranking of cameras and it is the single easiest thing here to read backwards.
//!
//! Every analyser in this crate works on the 2048 px proxy, because invariant 3 puts
//! medium analysis there and phase 02 already built and cached it. A 61 MP frame
//! downsampled to 2048 px has been reduced by a factor of nine; a 24 MP frame by a
//! factor of six. The bigger reduction averages more sensor pixels into each output
//! pixel, so a well-focused frame from the larger sensor measures a *lower* edge
//! response per output pixel - and dividing by that expectation is exactly what stops
//! the product preferring whoever bought the more expensive camera.
//!
//! The `GFX100S` row is the extreme case and is kept in the shipped table partly as a
//! reminder: 102 MP, the lowest `expected_mtf50` in the file, and by a distance the
//! sharpest camera in it.
//!
//! ## What is honest about the shipped numbers, and what is not
//!
//! The mechanism is real: the loader, the matching, the fallback scaling, the
//! confidence penalty and the fairness arithmetic all do what they say. The **twenty
//! rows are derived from published sensor specifications rather than measured from
//! bodies**, because there are no camera files in this repository - phase 02's first
//! exit condition, still open. That is condition C2 in the phase 09 exit report and the
//! shipped file says so at the top.

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::error::AuraError;
use serde::Deserialize;

use crate::errors;

/// The shipped table, embedded so a binary can never disagree with its own defaults.
const EMBEDDED: &str = include_str!("../../config/camera_calibration.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "camera_calibration.toml";

/// The fewest characters a rationale may have.
///
/// Nine, the same as `moment_profiles`' and `scene_profiles`'. Three files, one rule,
/// and a test asserts the constants agree - a measurement nobody can explain is a magic
/// number even when it came off a chart.
pub const MIN_RATIONALE: usize = 9;

/// How sharply the fallback's expected MTF50 tracks sensor resolution.
///
/// The fourth root of the megapixel ratio, not the square root. At a fixed output
/// width, per-output-pixel edge response grows sub-linearly with sensor resolution
/// because past roughly 30 MP the lens rather than the sensor is the limit. The
/// exponent is checked against the shipped rows by
/// `fallback_scaling_agrees_with_the_measured_rows`: the a7 III to a7R IV pair implies
/// 0.79 and the fourth-root rule predicts 0.795.
const MP_EXPONENT: f32 = 0.25;

/// One body's measurements.
///
/// Every field is documented in `config/camera_calibration.toml`, which is where a
/// person adding a row reads about them. The doc comments here say what the *code* does
/// with each number, which is the other half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    /// Sensor resolution, for the fallback's scaling and the fairness test.
    pub megapixels: f32,
    /// Edge response a well-focused frame reaches on the proxy. The denominator of
    /// every sharpness number this crate produces.
    pub expected_mtf50: f32,
    /// Read noise in electrons at base ISO. The floor a shadow lift is measured
    /// against.
    pub read_noise_e: f32,
    /// How fast noise grows with ISO relative to ideal shot noise. 1.0 is textbook.
    pub noise_slope: f32,
    /// Stops of clipped highlight this body's RAW can return.
    pub highlight_headroom_ev: f32,
    /// Stops a shadow can be lifted at base ISO before the noise exceeds tolerance.
    pub shadow_headroom_ev: f32,
    /// Where the two headroom figures were measured.
    pub base_iso: u32,
    /// Subtracted from every verdict this row produces.
    ///
    /// Zero for a measured row. Non-zero only for the fallback, which is what makes
    /// "we have not measured your camera" visible in the panel rather than only in a
    /// log.
    pub confidence_penalty: f32,
    /// True when this came from a named row rather than from the fallback.
    pub measured: bool,
}

impl Calibration {
    /// Noise sigma expected at one ISO, in electrons.
    ///
    /// Shot noise is the square root of the signal, so doubling ISO multiplies it by √2
    /// in the ideal case; `noise_slope` scales the exponent to fit a body whose
    /// on-sensor processing beats or misses that. Read noise adds in quadrature because
    /// the two sources are independent, which is the one place in this crate where the
    /// textbook form is used unaltered.
    ///
    /// The shot term is expressed in units of one electron at base ISO, so the returned
    /// figure is comparable between bodies and is not an absolute electron count. What
    /// consumes it - [`crate::integrity::noise`] - only ever takes ratios of it.
    #[must_use]
    pub fn expected_noise_e(&self, iso: u32) -> f32 {
        let base = self.base_iso.max(1);
        let ratio = (f64::from(iso.max(base)) / f64::from(base)) as f32;
        let shot = ratio.powf(0.5 * self.noise_slope);
        (self.read_noise_e.powi(2) + shot.powi(2)).sqrt()
    }

    /// How many stops of shadow this body can give back at one ISO.
    ///
    /// The base-ISO headroom, reduced as noise rises. A body at eight times base ISO
    /// has lost about a stand and a half of what it could lift, and a body with a flat
    /// noise slope loses less - which is the dual-gain behaviour the `DC-S5` row's
    /// rationale complains it cannot express properly with one slope figure.
    #[must_use]
    pub fn shadow_headroom_at(&self, iso: u32) -> f32 {
        let base = self.base_iso.max(1);
        let stops_up = (f64::from(iso.max(base)) / f64::from(base)) as f32;
        let cost = stops_up.log2() * 0.5 * self.noise_slope;
        (self.shadow_headroom_ev - cost).max(0.0)
    }

    /// The fallback, scaled to a frame's actual sensor resolution.
    ///
    /// Called when no row matched. `megapixels` comes from the frame's own pixel count,
    /// so a body nobody has measured is still judged against something that knows how
    /// big its sensor is.
    #[must_use]
    pub fn scaled_to(&self, megapixels: f32) -> Self {
        if megapixels <= 0.0 || self.megapixels <= 0.0 {
            return *self;
        }
        let ratio = self.megapixels / megapixels;
        Self {
            megapixels,
            expected_mtf50: (self.expected_mtf50 * ratio.powf(MP_EXPONENT)).clamp(0.05, 0.95),
            ..*self
        }
    }
}

/// Every body this build knows, and the row for the ones it does not.
#[derive(Debug, Clone)]
pub struct CalibrationTable {
    version: u16,
    fallback: Calibration,
    rows: BTreeMap<(String, String), Calibration>,
}

impl CalibrationTable {
    /// Load the shipped table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5036` when the embedded file is malformed, which would be a build bug
    /// rather than a deployment one - and is why `tests/calibration.rs` loads it too.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse("camera_calibration.toml (embedded)", EMBEDDED)
    }

    /// Load an installation override, or fall back to the shipped table.
    ///
    /// A malformed override is not fatal; a malformed baseline is. The same asymmetry
    /// the scene and moment profile registries have, for the same reason: the baseline
    /// is known good and falling back to it keeps the product open.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5036` only when the embedded baseline itself will not load.
    pub fn load_or_embedded(directory: &Path) -> Result<(Self, Option<AuraError>), AuraError> {
        let path = directory.join(OVERRIDE_FILE);
        if !path.exists() {
            return Ok((Self::embedded()?, None));
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            let refusal = errors::calibration_refused(
                &path.display().to_string(),
                "file",
                "could not be read; the shipped measurements are in use",
            );
            return Ok((Self::embedded()?, Some(refusal)));
        };
        match Self::parse(&path.display().to_string(), &text) {
            Ok(loaded) => Ok((loaded, None)),
            Err(refusal) => Ok((Self::embedded()?, Some(refusal))),
        }
    }

    /// Parse and validate one document.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5036`, naming the file, the key and the rule.
    pub fn parse(name: &str, text: &str) -> Result<Self, AuraError> {
        let parsed: TableFile = toml::from_str(text).map_err(|err| {
            errors::calibration_refused(name, "file", &format!("is not valid TOML: {err}"))
        })?;

        if parsed.version == 0 {
            return Err(errors::calibration_refused(
                name,
                "version",
                "must be at least 1; it is written into every verdict and 0 means unversioned",
            ));
        }

        let fallback = validate(name, "fallback", &parsed.fallback)?;
        if fallback.confidence_penalty <= 0.0 {
            return Err(errors::calibration_refused(
                name,
                "fallback.confidence_penalty",
                "must be above zero; a guessed calibration that does not lower the confidence is \
                 indistinguishable from a measured one",
            ));
        }

        let mut rows = BTreeMap::new();
        for entry in &parsed.camera {
            let key = (normalise(&entry.make), normalise(&entry.model));
            if key.0.is_empty() || key.1.is_empty() {
                return Err(errors::calibration_refused(
                    name,
                    "camera",
                    "has an entry with an empty make or model",
                ));
            }
            let label = format!("camera.{} {}", entry.make, entry.model);
            let mut row = validate(name, &label, &entry.body)?;
            row.confidence_penalty = 0.0;
            row.measured = true;
            if rows.insert(key, row).is_some() {
                return Err(errors::calibration_refused(
                    name,
                    &label,
                    "appears twice; two rows for one body would make the answer depend on file \
                     order",
                ));
            }
        }

        Ok(Self {
            version: parsed.version,
            fallback,
            rows,
        })
    }

    /// Which version of the table this is. Written into every verdict.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// How many bodies are named.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    /// The row for one body, or the fallback scaled to the frame's resolution.
    ///
    /// `megapixels` is the frame's own pixel count from EXIF and is only consulted when
    /// no row matched. A body with a row is judged by its measurements whatever a
    /// particular file's dimensions say - a cropped frame from an a7R IV is still an
    /// a7R IV.
    #[must_use]
    pub fn get(&self, make: &str, model: &str, megapixels: f32) -> Calibration {
        self.rows
            .get(&(normalise(make), normalise(model)))
            .copied()
            .unwrap_or_else(|| self.fallback.scaled_to(megapixels))
    }

    /// The fallback row as written, unscaled.
    #[must_use]
    pub const fn fallback(&self) -> Calibration {
        self.fallback
    }

    /// Every named body, as normalised `(make, model)` pairs.
    ///
    /// A pair rather than a joined string, because several manufacturers write a make
    /// with a space in it - `NIKON CORPORATION`, `Leica Camera AG` - and a caller that
    /// had to split the join back apart would get those wrong.
    #[must_use]
    pub fn bodies(&self) -> Vec<(String, String)> {
        self.rows.keys().cloned().collect()
    }
}

/// Normalise a make or model for matching.
///
/// Lower-cased, with runs of whitespace, hyphens and underscores collapsed to a single
/// space and the result trimmed. Manufacturers are inconsistent in EXIF - `NIKON
/// CORPORATION` writes `NIKON Z 6_2`, Canon writes `Canon EOS R6m2`, and several bodies
/// pad the model to a fixed width with trailing spaces - and matching on the raw string
/// would send half the table to the fallback.
///
/// The one place this happens, so that the table and the lookup can never disagree.
#[must_use]
pub fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() || ch == '_' || ch == '-' {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
    out
}

/// Check one row's numbers and turn it into a [`Calibration`].
fn validate(file: &str, key: &str, body: &Body) -> Result<Calibration, AuraError> {
    let refuse = |field: &str, rule: &str| {
        errors::calibration_refused(file, &format!("{key}.{field}"), rule)
    };

    if body.rationale.trim().len() < MIN_RATIONALE {
        return Err(refuse(
            "rationale",
            "is missing or too short; a measurement nobody can explain is a magic number",
        ));
    }
    if !(0.05..=0.95).contains(&body.expected_mtf50) {
        return Err(refuse(
            "expected_mtf50",
            "must be between 0.05 and 0.95; it is a per-output-pixel edge response, not a rating",
        ));
    }
    if body.read_noise_e <= 0.0 {
        return Err(refuse("read_noise_e", "must be above zero"));
    }
    if !(0.0..=2.0).contains(&body.noise_slope) {
        return Err(refuse("noise_slope", "must be between 0.0 and 2.0"));
    }
    if !(0.0..=2.5).contains(&body.highlight_headroom_ev) {
        return Err(refuse(
            "highlight_headroom_ev",
            "must be between 0.0 and 2.5 stops; a body claiming three stops of recoverable \
             highlight is a typo, and believing it would call a blown frame recoverable",
        ));
    }
    if !(0.0..=6.0).contains(&body.shadow_headroom_ev) {
        return Err(refuse(
            "shadow_headroom_ev",
            "must be between 0.0 and 6.0 stops",
        ));
    }
    if body.base_iso == 0 {
        return Err(refuse("base_iso", "must be above zero"));
    }
    if body.megapixels <= 0.0 {
        return Err(refuse("megapixels", "must be above zero"));
    }

    Ok(Calibration {
        megapixels: body.megapixels,
        expected_mtf50: body.expected_mtf50,
        read_noise_e: body.read_noise_e,
        noise_slope: body.noise_slope,
        highlight_headroom_ev: body.highlight_headroom_ev,
        shadow_headroom_ev: body.shadow_headroom_ev,
        base_iso: body.base_iso,
        confidence_penalty: body.confidence_penalty,
        measured: false,
    })
}

// ---------------------------------------------------------------------------
// The file's shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TableFile {
    version: u16,
    fallback: Body,
    #[serde(default)]
    camera: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    make: String,
    model: String,
    #[serde(flatten)]
    body: Body,
}

#[derive(Debug, Deserialize)]
struct Body {
    megapixels: f32,
    expected_mtf50: f32,
    read_noise_e: f32,
    noise_slope: f32,
    highlight_headroom_ev: f32,
    shadow_headroom_ev: f32,
    base_iso: u32,
    #[serde(default)]
    confidence_penalty: f32,
    #[serde(default)]
    rationale: String,
}
