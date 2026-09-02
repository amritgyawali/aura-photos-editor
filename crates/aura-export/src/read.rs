//! The port through which this crate learns anything about a photograph.
//!
//! `aura-export` depends on none of the deciding crates. What to export arrives as an
//! [`aura_core::ExportJob`] - a list somebody chose - and the facts a naming template needs arrive
//! through [`Field`], which `aura-app` implements out of the frozen services.
//!
//! That indirection is what stops `aura-cull` from being visible to the crate that writes the
//! gallery it chose. An exporter that could ask the cull engine what is delivered is an exporter
//! with an opinion about what is delivered, and section 2.1 gives it none.
//!
//! ## Every reading is an `Option`, and a missing one is dropped rather than defaulted
//!
//! Phase 29 established this and it matters more here, because the readings become **file names**.
//! A frame whose chapter is unknown must produce `2026-05-16_alex-and-sam_0031.jpg` and not
//! `2026-05-16_alex-and-sam_unknown_0031.jpg`, because the second is a lie that four thousand files
//! carry. [`aura_core::DeliveryCode::NameTokenUnavailable`] is the note that says a token was
//! dropped.

use std::path::PathBuf;

use aura_core::contract::delivery::{DeliveryColour, ImageId};
use aura_core::{AuraResult, ProjectId};

/// One photograph, as an export needs to see it.
///
/// Everything except `image` is optional, and the optionality is load-bearing: these values become
/// file names, and a substituted default becomes four thousand files claiming a fact nobody
/// measured.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    /// The photograph.
    pub image: Option<ImageId>,
    /// The original file's stem, without its extension. `{original}`.
    pub original_stem: Option<String>,
    /// The capture date as `YYYY-MM-DD`. `{date}`.
    pub date: Option<String>,
    /// The phase 07 chapter, slugified. `{chapter}`.
    pub chapter: Option<String>,
    /// The camera body from EXIF, slugified. `{camera}`.
    pub camera: Option<String>,
    /// Phase 24's disclosures for this frame, if any. They go in the manifest.
    pub cleanup_disclosures: Vec<String>,
    /// The frame's recipe, canonically, for the XMP sidecar. `None` means no sidecar is written.
    pub recipe_json: Option<String>,
    /// The original file, for a sidecar that has to name what it describes.
    pub original_path: Option<PathBuf>,
}

impl Frame {
    /// A frame that knows only which photograph it is.
    #[must_use]
    pub fn bare(image: ImageId) -> Self {
        Self {
            image: Some(image),
            ..Self::default()
        }
    }
}

/// One project's facts, gathered once.
///
/// Gathered rather than fetched per call, for the reason phase 27's and phase 29's fields are: a
/// naming plan asks every frame in a 4,000-image job for six facts, and a service round trip per
/// question is 24,000 catalog reads inside a twelve-minute budget that is mostly pixels.
pub trait Field: Send + Sync + std::fmt::Debug {
    /// The couple, slugified. `{couple}`. `None` when the project has no couple recorded.
    fn couple(&self) -> Option<String>;

    /// How many photographs the project holds. The outline's widest denominator.
    fn photos(&self) -> u32;

    /// How many photographs phase 12 selected. The outline's middle denominator.
    fn selected(&self) -> u32;

    /// One frame's facts.
    fn frame(&self, image: ImageId) -> Frame;

    /// Phase 27's archived report, when there is one to put beside the delivery.
    fn qc_report_path(&self) -> Option<PathBuf>;

    /// The versions this delivery was made by: app, render engine, recipe schema, model set.
    fn engine_versions(&self) -> Vec<(String, String)>;
}

/// The one way this crate obtains pixels.
///
/// A port rather than a direct `RenderService` call, for two reasons. It keeps the export loop
/// testable without a renderer - the eval harness drives it with authored plates - and it is where
/// `aura-app` decides which `RenderLevel` a set's resize implies, which is a policy question rather
/// than a writer's.
pub trait Source: Send + Sync + std::fmt::Debug {
    /// Render one photograph for export.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8024` when the frame cannot be rendered, plus whatever the renderer reports.
    fn render(
        &self,
        project: ProjectId,
        image: ImageId,
        colour: DeliveryColour,
        bit_depth: u8,
    ) -> AuraResult<Rendered>;
}

/// Pixels, ready to encode.
///
/// Mirrors `aura_render::RenderedImage` down to the four fields a writer needs. Not the render
/// contract's own type, because this port is what makes the export loop testable without a
/// renderer, and a test that has to construct a `RenderedImage` has to construct its notes, its
/// stage list and its timing as well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Interleaved RGB.
    pub data: Samples,
    /// The space the samples are in.
    pub colour: DeliveryColour,
    /// Phase 14's four-input hash. Recorded beside every written file.
    pub render_hash: String,
}

/// Interleaved RGB samples at one of the two depths a delivery uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Samples {
    /// Three `u8` per pixel.
    Eight(Vec<u8>),
    /// Three `u16` per pixel.
    Sixteen(Vec<u16>),
}

impl Samples {
    /// How many samples there are, which is three per pixel.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Eight(v) => v.len(),
            Self::Sixteen(v) => v.len(),
        }
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bits per sample: 8 or 16.
    #[must_use]
    pub const fn bit_depth(&self) -> u8 {
        match self {
            Self::Eight(_) => 8,
            Self::Sixteen(_) => 16,
        }
    }

    /// One sample as a `0..=1` float, or `None` when the index is outside the buffer.
    #[must_use]
    pub fn unit(&self, i: usize) -> Option<f32> {
        match self {
            Self::Eight(v) => v.get(i).map(|&s| f32::from(s) / 255.0),
            Self::Sixteen(v) => v.get(i).map(|&s| f32::from(s) / 65535.0),
        }
    }
}

impl Rendered {
    /// Whether the buffer's length agrees with its dimensions.
    ///
    /// Checked at the edge rather than trusted, because every writer below indexes on this
    /// arithmetic and a disagreement is the one way a writer can produce a file that decodes to
    /// somebody else's photograph.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        let expect = self.width as usize * self.height as usize * 3;
        self.width > 0 && self.height > 0 && self.data.len() == expect
    }
}
