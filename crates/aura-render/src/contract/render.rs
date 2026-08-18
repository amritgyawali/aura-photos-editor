//! FROZEN CONTRACT. The render API.
//!
//! PHASE-14 section 5 prints `RenderRequest`, `RenderLevel`, `OutputSpec`, `RenderPurpose`
//! and `RenderService` and freezes them before any pixel moves. Everything below either
//! appears there or is named in `docs/adr/ADR-0029-render-pipeline.md`. Changing anything
//! here requires an ADR and a re-lock of `contracts.lock`.
//!
//! # The three rules encoded in the types
//!
//! * **A request cannot name a destination.** There is no path, no file handle and no
//!   `write` anywhere in this module. Invariant 1: the RAW is opened read-only and the
//!   renderer has nowhere to put an answer except back to its caller.
//! * **A render says what it skipped.** [`RenderedImage::notes`] is not decoration. A stage
//!   that could not run - an unavailable mask generator, a lens profile that does not
//!   exist, a retouch operator phase 20 has not written yet - is reported rather than
//!   silently omitted. Invariant 9, and the reason `AURA-RENDER-8001` exists.
//! * **A hash is over inputs, never over output pixels.** [`RenderService::render_hash`]
//!   takes a request and no image, so it can be computed before the render and compared
//!   after it. ADR-0029 section 5.

use serde::{Deserialize, Serialize};

use aura_core::{AuraError, AuraResult, PhotoId};
use aura_recipe::Recipe;

/// Which rung of the render pyramid a caller wants.
///
/// Mirrors `aura_raw::contract::pixels::PixelLevel` deliberately rather than reusing it: a
/// render level names an **output** size and a decode level names an **input** tier, and the
/// two stopped being the same thing the moment `Screen` appeared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderLevel {
    /// The interactive proxy: 2048 px on the long edge.
    Proxy2048,
    /// Fit inside these dimensions, preserving aspect. Review at screen resolution.
    Screen(u32, u32),
    /// Full sensor resolution, tiled and 16-bit.
    Full,
}

impl RenderLevel {
    /// Stable text for cache paths, the catalog and telemetry. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proxy2048 => "proxy2048",
            Self::Screen(_, _) => "screen",
            Self::Full => "full",
        }
    }

    /// Longest output edge, or `None` when the sensor decides.
    #[must_use]
    pub const fn long_edge(self) -> Option<u32> {
        match self {
            Self::Proxy2048 => Some(2048),
            Self::Screen(w, h) => Some(if w > h { w } else { h }),
            Self::Full => None,
        }
    }
}

/// What the render is for. Chooses which stages the graph may skip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPurpose {
    /// A slider moved. Section 6.3: restoration and heavy retouch wait until zoom.
    Interactive,
    /// A model is about to read these pixels. Nothing is skipped.
    Analysis,
    /// A file is about to be written. Nothing is skipped, at full precision.
    Export,
}

impl RenderPurpose {
    /// Stable text for telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Analysis => "analysis",
            Self::Export => "export",
        }
    }

    /// True when the graph may skip restoration and heavy retouch.
    ///
    /// Only the interactive path may skip, and it may never skip for an export - which is
    /// what stops a photographer seeing one thing and delivering another.
    #[must_use]
    pub const fn may_skip_heavy_stages(self) -> bool {
        matches!(self, Self::Interactive)
    }
}

/// The colour space a rendered file is written in.
///
/// Three, and no "unknown". A buffer whose space we cannot name is a buffer we must not
/// deliver, which is phase 02's rule about `ColourSpace` applied at the other end of the
/// pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputColour {
    /// sRGB primaries, sRGB transfer. The web, and the default.
    Srgb,
    /// Adobe RGB (1998) primaries, gamma 2.2. Print.
    AdobeRgb,
    /// Display P3 primaries, sRGB transfer. Modern displays.
    DisplayP3,
}

impl OutputColour {
    /// Stable text for the catalog and the ICC name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::AdobeRgb => "adobe_rgb",
            Self::DisplayP3 => "display_p3",
        }
    }
}

/// What the caller wants written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputSpec {
    /// The output colour space.
    pub colour_space: OutputColour,
    /// 8 or 16. Anything else is refused by the engine rather than rounded.
    pub bit_depth: u8,
    /// The ICC profile to embed, by name. `None` means the space's own standard profile.
    #[serde(default)]
    pub icc: Option<String>,
}

impl Default for OutputSpec {
    fn default() -> Self {
        Self {
            colour_space: OutputColour::Srgb,
            bit_depth: 8,
            icc: None,
        }
    }
}

impl OutputSpec {
    /// The canonical text this spec contributes to a render hash.
    ///
    /// One string rather than three fields, because the hash is over bytes and a
    /// field-by-field hash would have to fix an order anyway. ADR-0029 section 5.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}/{}/{}",
            self.colour_space.as_str(),
            self.bit_depth,
            self.icc.as_deref().unwrap_or("standard")
        )
    }
}

/// One render, asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderRequest {
    /// The photograph.
    pub image_id: PhotoId,
    /// The edit to execute.
    pub recipe: Recipe,
    /// Which rung.
    pub level: RenderLevel,
    /// What to write.
    pub output: OutputSpec,
    /// What it is for.
    pub purpose: RenderPurpose,
}

/// Why a stage did not run.
///
/// A closed set, because "something was skipped" without a reason is exactly the silence
/// invariant 9 forbids. Each variant names the phase that fills the gap, so a note in the
/// UI can say *when* rather than only *not yet*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// The stage is not enabled by this recipe. The ordinary case, and not a caveat.
    NotRequested,
    /// The interactive path skipped it deliberately; a zoom or an export runs it.
    InteractivePath,
    /// A mask generator this build does not have. Phase 18.
    MaskGeneratorAbsent,
    /// A retouch operator this build does not have. Phases 20 and 21.
    OperatorAbsent,
    /// A restoration model this build does not have. Phase 22.
    RestorationAbsent,
    /// A geometry correction this build does not have. Phase 23.
    GeometryAbsent,
    /// The lens profile named by the recipe is not in the table.
    LensProfileAbsent,
    /// The camera profile is not in the table; the reference profile rendered it.
    CameraProfileAbsent,
    /// The pixels arrived already in the working space, so this stage had nothing to do.
    UpstreamAlready,
}

impl SkipReason {
    /// Stable text for the wire and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::InteractivePath => "interactive_path",
            Self::MaskGeneratorAbsent => "mask_generator_absent",
            Self::OperatorAbsent => "operator_absent",
            Self::RestorationAbsent => "restoration_absent",
            Self::GeometryAbsent => "geometry_absent",
            Self::LensProfileAbsent => "lens_profile_absent",
            Self::CameraProfileAbsent => "camera_profile_absent",
            Self::UpstreamAlready => "upstream_already",
        }
    }

    /// True when this is worth telling a photographer about.
    ///
    /// `NotRequested` and `UpstreamAlready` are not: a recipe that asks for no dehaze has
    /// not had its dehaze withheld, and reporting either would bury the six that matter.
    #[must_use]
    pub const fn is_a_caveat(self) -> bool {
        !matches!(self, Self::NotRequested | Self::UpstreamAlready)
    }
}

/// One stage that did not run, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderNote {
    /// The stage's stable slug, from `graph::Stage::as_str`.
    pub stage: String,
    /// Why it did not run.
    pub reason: SkipReason,
    /// What was being asked for, when naming it helps - a mask id, a lens profile.
    #[serde(default)]
    pub detail: Option<String>,
}

/// A rendered image.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedImage {
    /// Output width.
    pub width: u32,
    /// Output height.
    pub height: u32,
    /// Interleaved RGB. Three `u8` per pixel at bit depth 8, three `u16` at 16.
    pub data: RenderedData,
    /// The space the samples are in.
    pub colour_space: OutputColour,
    /// The hash of the four inputs that produced this. ADR-0029 section 5.
    pub render_hash: String,
    /// Which backend produced it: `cpu`, or the accelerator's name.
    pub backend: String,
    /// Stages that did not run, and why.
    pub notes: Vec<RenderNote>,
    /// Stages that did run, in order. Telemetry's `stages_run`.
    pub stages_run: Vec<String>,
    /// Wall-clock milliseconds.
    pub ms: u32,
    /// True when this came from the render cache rather than from the pipeline.
    pub cache_hit: bool,
}

/// The pixels of a rendered image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderedData {
    /// Three bytes per pixel.
    Eight(Vec<u8>),
    /// Three `u16` per pixel.
    Sixteen(Vec<u16>),
}

impl RenderedData {
    /// Bytes the payload occupies.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Eight(bytes) => bytes.len(),
            Self::Sixteen(words) => words.len() * 2,
        }
    }

    /// 8 or 16.
    #[must_use]
    pub const fn bit_depth(&self) -> u8 {
        match self {
            Self::Eight(_) => 8,
            Self::Sixteen(_) => 16,
        }
    }
}

/// Which engine is behind `RenderService`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// The processor reference path. The ground truth every accelerator is measured against.
    Cpu,
    /// A wgpu backend. Not in this build; see ADR-0029 section 4.
    Wgpu,
}

impl Backend {
    /// Stable text for the catalog and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Wgpu => "wgpu",
        }
    }
}

/// What this machine's renderer can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderCaps {
    /// Which engine.
    pub backend: Backend,
    /// Longest edge the backend will render in one pass before tiling.
    pub max_texture: u32,
    /// Bits of mantissa the colour maths carries. 24 on the reference path.
    pub precision_bits: u8,
    /// The working buffer ceiling, in bytes, past which a render is streamed in tiles.
    pub max_working_bytes: u64,
    /// The engine string that goes into every render hash.
    pub engine: String,
}

/// The one way to turn a recipe into pixels.
///
/// Sixth service of its kind and the first that produces an *output* rather than a
/// judgement. The rule the five before it established applies unchanged: no phase may keep
/// its own renderer, because two answers to "what does this photograph look like" is a
/// gallery that does not match the album that does not match the proof.
pub trait RenderService: Send + Sync {
    /// Execute a recipe.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002` when the recipe's shape is invalid, `AURA-RENDER-8009` when the
    /// frame had to be streamed and streaming was refused, and `AURA-RAW-*` when the pixels
    /// could not be obtained.
    fn render(&self, req: RenderRequest) -> AuraResult<RenderedImage>;

    /// The hash of a request's four inputs, without rendering it.
    ///
    /// Deliberately infallible in spirit and fallible in signature: a recipe that cannot be
    /// canonicalised has no hash, and returning a plausible one would make the determinism
    /// check pass on a document nothing can render.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002` when the recipe cannot be canonicalised.
    fn render_hash(&self, req: &RenderRequest) -> AuraResult<String>;

    /// What this machine can do.
    fn capabilities(&self) -> RenderCaps;

    /// The degradation this backend is running under, if any.
    ///
    /// `Some(AURA-RENDER-8001)` on the processor path. Returned rather than logged so the
    /// Hardware panel can render it once per session instead of the log carrying it once per
    /// photograph.
    fn degradation(&self) -> Option<AuraError> {
        None
    }
}
