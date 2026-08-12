//! FROZEN CONTRACT. The per-image metadata written beside every cached preview.
//!
//! Phase 02 section 5 fixes this JSON. Phases 15 and 16 read `as_shot_neutral`,
//! the black and white levels and the clipping statistics to make exposure and
//! white-balance decisions; phase 26 reads `camera_profile` to match bodies.
//! Adding a field is additive and safe; changing or removing one needs an ADR.
//!
//! `pipeline_ver` is the load-bearing number. It is part of every cache key and
//! part of every dataset key, so bumping it invalidates cached proxies cleanly
//! and forces a model re-validation run instead of silently changing what the
//! models were trained on.

use serde::{Deserialize, Serialize};

/// The rendering-pipeline version. **Bumping this requires ML-lead sign-off**,
/// because every trained model is keyed to the pixels this version produces.
///
/// * 1 - phase 02 initial: half-size demosaic, camera matrix to linear Rec.2020,
///   neutral filmic-lite curve, sRGB 8-bit plus linear 16-bit.
pub const PIPELINE_VER: u32 = 1;

/// What one tier produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierMeta {
    /// Width in pixels.
    pub w: u32,
    /// Height in pixels.
    pub h: u32,
    /// `embedded` or `decoded`, matching `PixelSource::as_str`.
    pub source: String,
    /// Bytes stored in the cache for this tier.
    pub bytes: u64,
    /// Wall-clock milliseconds this tier cost to produce.
    pub decode_ms: u32,
    /// True when a 16-bit linear companion was written next to this tier.
    #[serde(default)]
    pub linear16: bool,
    /// How the pixels were rendered, finer than `source`: `embedded_jpeg`,
    /// `demosaic_half`, `demosaic_full` or `embedded_upscaled`.
    pub render_path: String,
}

/// The colour profile actually used, named honestly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraProfileMeta {
    /// Manufacturer as EXIF spells it.
    pub make: String,
    /// Model as EXIF spells it.
    pub model: String,
    /// `embedded_dng`, `bundled`, or `generic` when nothing matched.
    pub matrix: String,
    /// Illuminant the matrix is defined for, for example `D65`.
    pub illuminant: String,
}

/// How much of the frame hit the rails. Phase 15 needs this to decide whether an
/// exposure correction is even possible.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClippingStats {
    /// Fraction of pixels at or above the white level, 0.0 to 1.0.
    pub highlight: f32,
    /// Fraction of pixels at or below the black level, 0.0 to 1.0.
    pub shadow: f32,
}

/// Which decoder produced these pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineMeta {
    /// LibRaw version, when a build links it. This build does not: see
    /// `docs/adr/ADR-0004-raw-decode-backend.md`. The key is kept so a future
    /// LibRaw-linked build writes into the same shape.
    pub libraw: Option<String>,
    /// The in-tree decoder and its version, for example `aura-raw 0.1.0`.
    pub decoder: String,
    /// Rendering pipeline version; see [`PIPELINE_VER`].
    pub pipeline_ver: u32,
}

/// The sidecar itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewSidecar {
    /// Catalog id of the photograph, prefixed form.
    pub image_id: String,
    /// BLAKE3 of the source file, hex. Half of the cache key.
    pub content_hash: String,
    /// Tier 1, when it has been built.
    pub tier1: Option<TierMeta>,
    /// Tier 2, when it has been built.
    pub tier2: Option<TierMeta>,
    /// The profile used for tier 2.
    pub camera_profile: CameraProfileMeta,
    /// As-shot neutral multipliers, R/G/B, normalised so green is 1.0.
    pub as_shot_neutral: [f32; 3],
    /// Sensor black level in raw code values.
    pub black_level: u32,
    /// Sensor white level in raw code values.
    pub white_level: u32,
    /// Clipping measured on the tier 2 render.
    pub clipping: ClippingStats,
    /// Decoder provenance.
    pub engine: EngineMeta,
}

impl PreviewSidecar {
    /// Serialise deterministically: `serde_json` writes struct fields in
    /// declaration order, so the same inputs always produce the same bytes.
    ///
    /// # Errors
    ///
    /// Returns the serialisation failure text; callers map it to a coded error.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Parse a sidecar written by [`PreviewSidecar::to_json`].
    ///
    /// # Errors
    ///
    /// Returns the parse failure text.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
}
