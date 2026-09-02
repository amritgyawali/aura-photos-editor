//! FROZEN CONTRACT. Export, backup and client-gallery delivery. PHASE-30 section 5.
//!
//! Twenty-nine phases decided things. This one **writes files**, and that is the whole of what
//! makes it different from every contract before it.
//!
//! Everything from phase 06 to phase 29 produces a row: a judgement, a grouping, a proposal, a
//! plan. A row that is wrong is a row somebody re-runs. A file that is wrong is a gallery a
//! photographer has already sent to a couple, and there is no re-run for that. So the shapes in
//! this module are built around one asymmetry: **it is always better to fail an export than to
//! deliver a bad one.**
//!
//! ## The seven properties this contract exists to make structural
//!
//! **A hash that was not read back is not a hash.** [`ExportedFile::hash`] is defined as the digest
//! of the bytes *re-read from the destination*, never of the buffer that was written. That is not a
//! pedantic distinction: a short write, a full disk, a NAS that acknowledges and drops, and a
//! failing SD card all produce a correct in-memory buffer and a wrong file. Section 6.1's first
//! sentence is "photographers have lost galleries to silent write failures", and this is the shape
//! that makes the claim checkable rather than asserted. [`DeliveryManifest`] can only be assembled
//! out of [`ExportedFile`]s, so a manifest is a record of files that were read back.
//!
//! **`verify` is on the job because section 5 put it there, and turning it off changes what the job
//! *is*.** [`ExportJob::verify`] defaults to `true` and [`ExportOutline::unverified`] counts what
//! ran without it, on the wire, in the panel and in the manifest's own header. A product that let a
//! photographer quietly switch off the only check that catches a silent corruption would be a
//! product whose guarantee is a default.
//!
//! **A name is a template, and a collision is resolved rather than overwritten.** [`NamingTemplate`]
//! is parsed and validated up front - it cannot contain a path separator, cannot be empty after
//! substitution, and cannot name a token this build does not have - and the writer appends a
//! collision suffix rather than clobbering. Two cameras on one wedding produce `DSC_0431.NEF`
//! twice; a naming scheme that silently kept one of them delivers 3,998 files out of 4,000 and
//! reports success.
//!
//! **Stripping is a policy and keeping is the exception.** [`MetadataPolicy::strip_gps`] defaults
//! to `true`. A wedding gallery carries the coordinates of somebody's house in every frame of the
//! getting-ready chapter, and the photographer who has to remember to switch that on is the
//! photographer who forgets once.
//!
//! **A destination is a place, never a protocol.** [`Destination`] names where the files go;
//! nothing in this module says how they get there. That is what makes a new gallery provider a new
//! implementation of one trait rather than an edit to the export engine - section 6.2's "adding a
//! provider must not touch core code".
//!
//! **An upload is resumable because its unit is a file with a hash.** [`UploadState`] is per file
//! and carries the bytes already accepted, so a resumed job re-sends the tail of one file rather
//! than the head of a wedding. Invariant 5, in the phase where the network makes it unavoidable.
//!
//! **Nothing here re-derives a pixel.** An export renders through phase 14's `RenderService`, which
//! is the only way in this product to turn a recipe into pixels, and records the render hash beside
//! every file. Phase 14's rule, and the first phase that had a reason to want its own copy: an
//! exporter with its own resampler is a delivered JPEG that does not match the proof the couple
//! approved.
//!
//! ## The one thing a later phase can get wrong
//!
//! **The denominator here is the *set*, not the gallery and not the project.** A photographer who
//! exports the album exports 80 frames out of a gallery of 700 out of a project of 4,000, and all
//! three numbers are on [`ExportOutline`] for that reason. A panel that reported "80 of 4,000
//! exported" would read as a failure on a job that did exactly what it was asked to.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult, ErrorCode, Recovery, Severity};
use crate::contract::ids::ProjectId;
pub use crate::contract::scene::{ImageId, Timestamp};

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// How many sets one job may carry.
///
/// Five are named in section 2.1 - gallery, album, social, teaser and black-and-white - and eight
/// leaves room for a studio's own without letting a job become a batch queue. A job is one
/// transaction over one destination; a photographer with twenty sets has twenty jobs, and the
/// difference matters when one of them fails.
pub const MAX_SETS: usize = 8;

/// How many reasons any one decision in this phase may carry. Phase 13's bound, unchanged.
pub const MAX_REASONS: usize = 6;

/// The lowest JPEG quality this product will write.
///
/// Sixty rather than zero. Below about 60 the artefacts are visible on skin at 100 %, and a slider
/// that reaches a setting no photographer should deliver is a slider somebody will deliver from.
pub const MIN_JPEG_QUALITY: u8 = 60;

/// The highest JPEG quality this product will write.
pub const MAX_JPEG_QUALITY: u8 = 100;

/// The smallest long edge a resize may ask for.
pub const MIN_LONG_EDGE: u32 = 240;

/// The largest long edge a resize may ask for.
///
/// Twenty thousand is above the long edge of every camera that exists and below the point at which
/// a single frame stops fitting in a sensible amount of memory. It exists to catch a typo in a
/// template, not to express a policy.
pub const MAX_LONG_EDGE: u32 = 20_000;

/// The longest a naming template may be, in characters.
pub const MAX_TEMPLATE_LEN: usize = 200;

/// The longest a set's name may be, in characters.
pub const MAX_SET_NAME: usize = 64;

/// How many keywords a metadata policy may carry.
pub const MAX_KEYWORDS: usize = 32;

/// The file name of the delivery manifest inside a destination.
pub const MANIFEST_NAME: &str = "aura-delivery-manifest.json";

/// The extension of an XMP sidecar.
pub const SIDECAR_EXT: &str = "xmp";

/// The suffix appended to a colliding name, before the numeral.
pub const COLLISION_SEPARATOR: &str = "_";

// ---------------------------------------------------------------------------
// Formats
// ---------------------------------------------------------------------------

/// What kind of file is written.
///
/// Three, and deliberately no RAW and no PSD. A RAW output would be a mutated original in every
/// sense that matters to invariant 1, and a PSD is a Photoshop document rather than a delivery -
/// section 2.1 sends layers to Photoshop through the plugin, which is a hand-off and not an export.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    /// Eight-bit JPEG. What a client gallery and every social platform take.
    #[default]
    Jpeg,
    /// Eight- or sixteen-bit TIFF. What a print lab and a retoucher take.
    Tiff,
    /// Eight- or sixteen-bit PNG. Lossless, and what a graphic designer asks for.
    Png,
}

impl FileFormat {
    /// Every format.
    pub const ALL: [Self; 3] = [Self::Jpeg, Self::Tiff, Self::Png];

    /// How many there are.
    pub const COUNT: usize = 3;

    /// The stored text and the wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Tiff => "tiff",
            Self::Png => "png",
        }
    }

    /// The file extension, without the dot, lower case.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Tiff => "tif",
            Self::Png => "png",
        }
    }

    /// Whether the format discards information.
    ///
    /// Read by [`ExportSet::quality`]'s validation: a quality setting on a lossless format is a
    /// setting that does nothing, and accepting one silently is how a photographer comes to believe
    /// their TIFFs are compressed at 92.
    #[must_use]
    pub const fn is_lossy(self) -> bool {
        matches!(self, Self::Jpeg)
    }

    /// Whether the format can carry sixteen bits per sample.
    #[must_use]
    pub const fn supports_sixteen_bit(self) -> bool {
        matches!(self, Self::Tiff | Self::Png)
    }

    /// Parse the wire value.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the text names no format this build has.
    pub fn parse(text: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|f| f.as_str() == text)
            .ok_or_else(|| {
                bad_job(
                    format!("unknown export format `{text}`"),
                    "AURA does not write that file format. Choose JPEG, TIFF or PNG.",
                )
            })
    }
}

impl fmt::Display for FileFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The output colour space a set is written in.
///
/// **Mirrored from `aura_render::contract::render::OutputColour` rather than imported**, because
/// `aura-core` depends on no workspace crate and a test asserts it. The mirror is deliberate and it
/// is one-directional: `aura-export` maps this onto the render contract's enum in one function, and
/// the phase gate checks that the two vocabularies still have the same members. A third spelling
/// would be a delivered file whose ICC profile disagrees with the pixels inside it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryColour {
    /// sRGB primaries, sRGB transfer. The web, a client gallery, and the default.
    #[default]
    Srgb,
    /// Adobe RGB (1998) primaries, gamma 2.2. Print.
    AdobeRgb,
    /// Display P3 primaries, sRGB transfer. Modern displays.
    DisplayP3,
}

impl DeliveryColour {
    /// Every space.
    pub const ALL: [Self; 3] = [Self::Srgb, Self::AdobeRgb, Self::DisplayP3];

    /// The stored text, the wire value, and the name of the ICC profile embedded beside it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::AdobeRgb => "adobe_rgb",
            Self::DisplayP3 => "display_p3",
        }
    }

    /// Parse the wire value.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the text names no space this build has.
    pub fn parse(text: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == text)
            .ok_or_else(|| {
                bad_job(
                    format!("unknown output colour space `{text}`"),
                    "AURA does not write that colour space. Choose sRGB, Adobe RGB or Display P3.",
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Resize and output sharpening
// ---------------------------------------------------------------------------

/// How large the written file is.
///
/// **Never upscales.** [`Resize::target`] returns the source size when the request is larger than
/// the frame, and [`DeliveryCode::ResizeIgnoredUpscale`] says so. A gallery whose 24 MP frames were
/// stretched to 45 MP because one set asked for a long edge is a gallery that is softer than the
/// originals and larger than them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Resize {
    /// The frame at its rendered size.
    #[default]
    Full,
    /// Scaled so the longer side is this many pixels.
    LongEdge {
        /// Pixels on the long edge.
        pixels: u32,
    },
    /// Scaled to fit inside a box, preserving aspect.
    Fit {
        /// Maximum width.
        width: u32,
        /// Maximum height.
        height: u32,
    },
}

impl Resize {
    /// The size this resize produces for a source frame, preserving aspect and never upscaling.
    ///
    /// Returns `(0, 0)` for a degenerate source rather than dividing by zero; the writer refuses
    /// such a frame with [`DeliveryCode::RenderUnavailable`].
    #[must_use]
    #[allow(
        clippy::cast_lossless,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn target(self, src_w: u32, src_h: u32) -> (u32, u32) {
        if src_w == 0 || src_h == 0 {
            return (0, 0);
        }
        let scale = match self {
            Self::Full => 1.0_f64,
            Self::LongEdge { pixels } => {
                let long = src_w.max(src_h) as f64;
                f64::from(pixels) / long
            }
            Self::Fit { width, height } => {
                let sw = f64::from(width) / f64::from(src_w);
                let sh = f64::from(height) / f64::from(src_h);
                sw.min(sh)
            }
        };
        // Never upscale. Section 6.1 sizes a set down for a purpose; sizing one up invents detail.
        let scale = scale.min(1.0);
        let w = ((f64::from(src_w) * scale).round() as u32).max(1);
        let h = ((f64::from(src_h) * scale).round() as u32).max(1);
        (w, h)
    }

    /// Whether this resize would have enlarged the frame, which is the case it refuses.
    #[must_use]
    pub fn would_upscale(self, src_w: u32, src_h: u32) -> bool {
        match self {
            Self::Full => false,
            Self::LongEdge { pixels } => u64::from(pixels) > u64::from(src_w.max(src_h)),
            Self::Fit { width, height } => {
                u64::from(width) > u64::from(src_w) && u64::from(height) > u64::from(src_h)
            }
        }
    }

    /// Check the request's own bounds.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when a dimension is outside [`MIN_LONG_EDGE`]..=[`MAX_LONG_EDGE`].
    pub fn validate(self) -> AuraResult<()> {
        let check = |v: u32, what: &str| -> AuraResult<()> {
            if !(MIN_LONG_EDGE..=MAX_LONG_EDGE).contains(&v) {
                return Err(bad_job(
                    format!("{what} {v} outside {MIN_LONG_EDGE}..={MAX_LONG_EDGE}"),
                    "That output size is outside the range AURA writes. Choose a size between 240 \
                     and 20,000 pixels.",
                ));
            }
            Ok(())
        };
        match self {
            Self::Full => Ok(()),
            Self::LongEdge { pixels } => check(pixels, "long edge"),
            Self::Fit { width, height } => {
                check(width, "width")?;
                check(height, "height")
            }
        }
    }
}

/// Output sharpening, applied **after** resize.
///
/// Section 6.1: "resolution-aware and applied after resize". The order is the whole of it - a frame
/// sharpened at 45 MP and then scaled to 2,048 px has had its sharpening scaled away, and one
/// sharpened for print and delivered to a phone screen has halos on it.
///
/// The amount is not a slider. A photographer choosing "screen" or "print" is choosing what the
/// file is *for*, and [`OutputSharpen::amount`] turns that into a number that depends on how far
/// the frame was scaled. A number a person set would be a number that is right at one output size.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum OutputSharpen {
    /// Nothing. What a hand-off to another editor wants.
    #[default]
    None,
    /// For a screen: a client gallery, a social post, a proof.
    Screen,
    /// For paper: an album, a print lab.
    Print,
}

impl OutputSharpen {
    /// Every setting.
    pub const ALL: [Self; 3] = [Self::None, Self::Screen, Self::Print];

    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Screen => "screen",
            Self::Print => "print",
        }
    }

    /// Parse the wire value.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the text names no setting this build has.
    pub fn parse(text: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|s| s.as_str() == text)
            .ok_or_else(|| {
                bad_job(
                    format!("unknown output sharpening `{text}`"),
                    "AURA does not have that output sharpening setting. Choose none, screen or \
                     print.",
                )
            })
    }

    /// How much unsharp mask to apply, given how far the frame was scaled.
    ///
    /// `scale` is the output long edge over the source long edge, so a full-size export is 1.0 and
    /// a 2,048 px web file off a 45 MP frame is about 0.24.
    ///
    /// The shape: a frame that was scaled down hard has lost its acutance to the resampler and
    /// needs the most; a frame at full size needs the least, because everything phase 22 did to it
    /// is still there. The curve is `base * (1 + k * (1 - scale))`, which is monotone in how far the
    /// frame was scaled and returns `base` at full size.
    #[must_use]
    pub fn amount(self, scale: f32) -> f32 {
        let base = match self {
            Self::None => return 0.0,
            Self::Screen => 0.22_f32,
            Self::Print => 0.34_f32,
        };
        let scale = scale.clamp(0.05, 1.0);
        let lift = 1.0 + 1.6 * (1.0 - scale);
        base * lift
    }
}

// ---------------------------------------------------------------------------
// Naming
// ---------------------------------------------------------------------------

/// A token a naming template may contain.
///
/// The six of section 6.1 plus the set's own name. Every one of them is a fact this product already
/// knows about the photograph; there is no free-text token and no counter a person maintains,
/// because a template that can express "whatever I typed last time" is a template that produces two
/// weddings' files in one folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameToken {
    /// The wedding's date, `YYYY-MM-DD`, from the project.
    Date,
    /// The couple's names as the project records them, slugified.
    Couple,
    /// The phase 07 chapter this frame belongs to.
    Chapter,
    /// The frame's position within its set, zero-padded to four digits.
    Sequence,
    /// The camera body, from EXIF.
    Camera,
    /// The original file's stem, without its extension.
    Original,
    /// The set's own name.
    Set,
}

impl NameToken {
    /// Every token.
    pub const ALL: [Self; 7] = [
        Self::Date,
        Self::Couple,
        Self::Chapter,
        Self::Sequence,
        Self::Camera,
        Self::Original,
        Self::Set,
    ];

    /// How many there are.
    pub const COUNT: usize = 7;

    /// The text inside the braces.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Couple => "couple",
            Self::Chapter => "chapter",
            Self::Sequence => "seq",
            Self::Camera => "camera",
            Self::Original => "original",
            Self::Set => "set",
        }
    }

    /// Parse the text inside the braces.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == text)
    }
}

/// A validated file-naming template.
///
/// The raw form is a string with `{token}` placeholders: `"{date}_{couple}_{seq}"`. It is parsed
/// once, at the edge, and what travels afterwards is a value that is known to name only tokens this
/// build has and to contain no path separator.
///
/// **A template can never produce a directory.** `/`, `\`, `:` and the parent-directory sequence are
/// refused at parse time. A naming template that could contain a separator is a template that could
/// write outside the destination a photographer chose, which is the same class of bug as a path
/// traversal in an archive extractor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NamingTemplate(String);

impl NamingTemplate {
    /// What a gallery set is named when nobody chose.
    pub const GALLERY_DEFAULT: &'static str = "{date}_{couple}_{seq}";

    /// What an album set is named when nobody chose.
    pub const ALBUM_DEFAULT: &'static str = "{date}_{couple}_album_{seq}";

    /// What a hand-off to another editor is named when nobody chose: the original's own stem, so a
    /// photographer can match a delivered file to the frame it came from without a lookup.
    pub const HANDOFF_DEFAULT: &'static str = "{original}";

    /// Parse and validate a template.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the template is empty, is longer than [`MAX_TEMPLATE_LEN`], contains a
    /// path separator, names a token this build does not have, or has an unclosed brace.
    pub fn parse(raw: &str) -> AuraResult<Self> {
        if raw.is_empty() {
            return Err(bad_job(
                "empty naming template".to_owned(),
                "A file-naming template cannot be empty.",
            ));
        }
        if raw.chars().count() > MAX_TEMPLATE_LEN {
            return Err(bad_job(
                format!("naming template longer than {MAX_TEMPLATE_LEN} characters"),
                "That file-naming template is too long.",
            ));
        }
        if raw.contains('/') || raw.contains('\\') || raw.contains(':') || raw.contains("..") {
            return Err(bad_job(
                "naming template contains a path separator".to_owned(),
                "A file-naming template names a file, not a folder. Remove the slashes and colons.",
            ));
        }
        // Walk it once. Every `{` must close, and every token between braces must be known.
        let mut rest = raw;
        let mut saw_token = false;
        while let Some(open) = rest.find('{') {
            let after = rest.get(open + 1..).unwrap_or_default();
            let Some(close) = after.find('}') else {
                return Err(bad_job(
                    "naming template has an unclosed `{`".to_owned(),
                    "That file-naming template has a `{` with no matching `}`.",
                ));
            };
            let name = after.get(..close).unwrap_or_default();
            if NameToken::parse(name).is_none() {
                return Err(bad_job(
                    format!("naming template names unknown token `{name}`"),
                    "That file-naming template uses a placeholder AURA does not have.",
                ));
            }
            saw_token = true;
            rest = after.get(close + 1..).unwrap_or_default();
        }
        if rest.contains('}') || (!saw_token && raw.contains('}')) {
            return Err(bad_job(
                "naming template has a `}` with no `{`".to_owned(),
                "That file-naming template has a `}` with no matching `{`.",
            ));
        }
        Ok(Self(raw.to_owned()))
    }

    /// The raw template text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Which tokens this template uses, in the order they appear.
    #[must_use]
    pub fn tokens(&self) -> Vec<NameToken> {
        let mut out = Vec::new();
        let mut rest = self.0.as_str();
        while let Some(open) = rest.find('{') {
            let after = rest.get(open + 1..).unwrap_or_default();
            let Some(close) = after.find('}') else { break };
            if let Some(tok) = NameToken::parse(after.get(..close).unwrap_or_default()) {
                out.push(tok);
            }
            rest = after.get(close + 1..).unwrap_or_default();
        }
        out
    }

    /// Whether this template can distinguish two frames of one set on its own.
    ///
    /// A template with neither `{seq}` nor `{original}` in it names every frame of a set the same
    /// thing, so every file after the first is a collision. That is legal - the writer will suffix
    /// them - and it is worth saying out loud, which is what
    /// [`DeliveryCode::NamingTemplateNotUnique`] is for.
    #[must_use]
    pub fn is_distinguishing(&self) -> bool {
        self.tokens()
            .iter()
            .any(|t| matches!(t, NameToken::Sequence | NameToken::Original))
    }
}

impl Default for NamingTemplate {
    fn default() -> Self {
        Self(Self::GALLERY_DEFAULT.to_owned())
    }
}

impl fmt::Display for NamingTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

/// What travels with a delivered file, and what does not.
///
/// **`strip_gps` defaults to `true`**, which is the only default in this struct that is a safety
/// decision rather than a convenience. The getting-ready chapter of a wedding is shot at somebody's
/// house, and the coordinates of that house are in every frame of it. A photographer who has to
/// remember to switch stripping on is a photographer who forgets once, and the forgetting is
/// invisible until it is not.
///
/// **There is no `strip_all` and no `keep_all`.** Copyright and creator are the photographer's
/// claim on their own work and go out on every file; the camera's serial number is an identifier
/// for the photographer rather than for the couple, and [`MetadataPolicy::strip_camera_serial`]
/// exists because a second shooter's body should not be traceable through a gallery they were hired
/// into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataPolicy {
    /// The copyright line, e.g. `© 2026 Studio Name`.
    #[serde(default)]
    pub copyright: Option<String>,
    /// How to reach the photographer: a URL or an email, as the studio prefers.
    #[serde(default)]
    pub contact: Option<String>,
    /// The creator's name, IPTC `by-line`.
    #[serde(default)]
    pub creator: Option<String>,
    /// Keywords written into every file of the job.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Remove every location tag. **Defaults to true.**
    pub strip_gps: bool,
    /// Remove the camera body's serial number. Defaults to true.
    pub strip_camera_serial: bool,
}

impl Default for MetadataPolicy {
    fn default() -> Self {
        Self {
            copyright: None,
            contact: None,
            creator: None,
            keywords: Vec::new(),
            strip_gps: true,
            strip_camera_serial: true,
        }
    }
}

impl MetadataPolicy {
    /// Check the policy's own bounds.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when there are more than [`MAX_KEYWORDS`] keywords or one of them is empty.
    pub fn validate(&self) -> AuraResult<()> {
        if self.keywords.len() > MAX_KEYWORDS {
            return Err(bad_job(
                format!("{} keywords, more than {MAX_KEYWORDS}", self.keywords.len()),
                "That is more keywords than AURA writes into a file.",
            ));
        }
        if self.keywords.iter().any(|k| k.trim().is_empty()) {
            return Err(bad_job(
                "empty keyword".to_owned(),
                "One of the keywords is blank. Remove it or give it a word.",
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Destinations and providers
// ---------------------------------------------------------------------------

/// A client-gallery or object-storage provider, by name.
///
/// A newtype rather than an enum, and that is the point of section 6.2: adding Pic-Time must not be
/// an edit to this file. What a provider *is* lives in `aura-delivery`'s registry; what travels
/// here is the name a photographer configured.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    /// Wrap a provider name.
    ///
    /// # Errors
    ///
    /// `AURA-DLV-10001` when the name is empty, longer than 64 characters, or contains anything but
    /// lower-case letters, digits, `-` and `_`. A provider name reaches a file path and a catalog
    /// key, and the two have different ideas about what is legal in one.
    pub fn parse(name: &str) -> AuraResult<Self> {
        let ok = !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !ok {
            return Err(AuraError::new(
                ErrorCode("AURA-DLV-10001"),
                Severity::ItemFailed,
                Recovery::AskUser,
                format!("invalid provider name `{name}`"),
                "That gallery provider name is not one AURA recognises.",
            ));
        }
        Ok(Self(name.to_owned()))
    }

    /// The provider's name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a job's files go.
///
/// A *place*, never a protocol. Folder and NAS are the same mechanism with different failure modes -
/// a NAS disappears mid-job and a folder does not - and they are separate variants because the
/// panel says different things about them and the resume logic treats them differently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Destination {
    /// A directory on this machine.
    Folder {
        /// Where.
        path: PathBuf,
    },
    /// A directory on a network share.
    Nas {
        /// Where, as this machine sees it.
        path: PathBuf,
    },
    /// An object-storage bucket.
    CloudBucket {
        /// The bucket.
        bucket: String,
        /// The key prefix inside it.
        prefix: String,
    },
    /// A client-gallery provider.
    Provider(ProviderId),
}

impl Destination {
    /// The stable word for the kind, for the catalog, the manifest and telemetry's
    /// `destination_kind`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Folder { .. } => "folder",
            Self::Nas { .. } => "nas",
            Self::CloudBucket { .. } => "cloud_bucket",
            Self::Provider(_) => "provider",
        }
    }

    /// The local directory this destination writes into, when it has one.
    ///
    /// `None` for a bucket and a provider: those receive files that were written somewhere else
    /// first, which is what makes an upload resumable and what makes a manifest possible for them.
    #[must_use]
    pub fn local_root(&self) -> Option<&Path> {
        match self {
            Self::Folder { path } | Self::Nas { path } => Some(path.as_path()),
            Self::CloudBucket { .. } | Self::Provider(_) => None,
        }
    }

    /// Whether reaching this destination needs a network.
    ///
    /// Read by the pre-flight check, because "the phase must work with the network cable unplugged"
    /// is section 7 and a job that cannot start should say so before it renders 700 frames.
    #[must_use]
    pub const fn needs_network(&self) -> bool {
        matches!(self, Self::CloudBucket { .. } | Self::Provider(_))
    }
}

// ---------------------------------------------------------------------------
// The job - section 5, verbatim
// ---------------------------------------------------------------------------

/// One set of photographs, written one way.
///
/// Section 5's `{ name, images, format, quality, resize, sharpen, naming }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSet {
    /// What this set is called: `gallery`, `album`, `social`, `teaser`, `bw`, or a studio's own.
    pub name: String,
    /// The photographs, in the order they are written. **The sequence token counts this order.**
    pub images: Vec<ImageId>,
    /// What kind of file.
    pub format: FileFormat,
    /// JPEG quality, `60..=100`. Ignored - and refused - on a lossless format.
    pub quality: u8,
    /// How large.
    pub resize: Resize,
    /// Output sharpening, applied after the resize.
    pub sharpen: OutputSharpen,
    /// How the files are named.
    pub naming: NamingTemplate,
    /// The output colour space.
    #[serde(default)]
    pub colour: DeliveryColour,
    /// Bits per sample: 8, or 16 on a format that supports it.
    #[serde(default = "eight")]
    pub bit_depth: u8,
    /// Write an XMP sidecar beside each file. Section 6.2's universal hand-off path.
    #[serde(default)]
    pub sidecar: bool,
}

const fn eight() -> u8 {
    8
}

impl ExportSet {
    /// Check the set's own bounds.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the name is empty or too long, the set has no images, the quality is
    /// outside [`MIN_JPEG_QUALITY`]..=[`MAX_JPEG_QUALITY`] on a lossy format, a bit depth is asked
    /// for that the format cannot carry, or the resize is out of range.
    pub fn validate(&self) -> AuraResult<()> {
        if self.name.trim().is_empty() || self.name.chars().count() > MAX_SET_NAME {
            return Err(bad_job(
                format!("set name `{}` is empty or too long", self.name),
                "Give the set a name of up to 64 characters.",
            ));
        }
        if self.images.is_empty() {
            return Err(bad_job(
                format!("set `{}` has no images", self.name),
                "That set has no photographs in it.",
            ));
        }
        if self.format.is_lossy() && !(MIN_JPEG_QUALITY..=MAX_JPEG_QUALITY).contains(&self.quality)
        {
            return Err(bad_job(
                format!(
                    "quality {} outside {MIN_JPEG_QUALITY}..={MAX_JPEG_QUALITY}",
                    self.quality
                ),
                "JPEG quality has to be between 60 and 100.",
            ));
        }
        if self.bit_depth != 8 && !(self.bit_depth == 16 && self.format.supports_sixteen_bit()) {
            return Err(bad_job(
                format!(
                    "{} bits per sample is not available for {}",
                    self.bit_depth, self.format
                ),
                "That file format cannot carry that many bits per sample.",
            ));
        }
        self.resize.validate()
    }
}

/// One export, asked for. Section 5's frozen shape.
///
/// **`verify` is a field and not a constant, because section 5 froze it as one.** It defaults to
/// `true` in [`ExportJob::new`], the outline counts what ran without it, and the manifest header
/// records it - so a job that skipped verification is visible in three places rather than being a
/// property nobody can find afterwards. ADR-0061 section 3 has the argument for why it was not
/// removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportJob {
    /// The sets. At most [`MAX_SETS`], and at least one.
    pub sets: Vec<ExportSet>,
    /// Where the files go.
    pub destination: Destination,
    /// What metadata travels with them.
    pub metadata: MetadataPolicy,
    /// Re-read and hash every written file.
    pub verify: bool,
}

impl ExportJob {
    /// A job over one destination, verified.
    #[must_use]
    pub fn new(sets: Vec<ExportSet>, destination: Destination) -> Self {
        Self {
            sets,
            destination,
            metadata: MetadataPolicy::default(),
            // Section 6.1: "Verification is mandatory by default".
            verify: true,
        }
    }

    /// How many files this job writes, sidecars excluded.
    #[must_use]
    pub fn file_count(&self) -> u32 {
        self.sets
            .iter()
            .map(|s| u32::try_from(s.images.len()).unwrap_or(u32::MAX))
            .fold(0_u32, u32::saturating_add)
    }

    /// Check the whole job before a single frame is rendered.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the job has no sets, more than [`MAX_SETS`], two sets with the same
    /// name, or a set that fails its own validation. Two sets with one name write into one another;
    /// the collision suffix would hide it and the manifest would report both.
    pub fn validate(&self) -> AuraResult<()> {
        if self.sets.is_empty() {
            return Err(bad_job(
                "job has no sets".to_owned(),
                "There is nothing in this export. Add at least one set.",
            ));
        }
        if self.sets.len() > MAX_SETS {
            return Err(bad_job(
                format!("{} sets, more than {MAX_SETS}", self.sets.len()),
                "That is more sets than one export can carry. Split it into two exports.",
            ));
        }
        let mut names: Vec<&str> = self.sets.iter().map(|s| s.name.as_str()).collect();
        names.sort_unstable();
        if names.windows(2).any(|w| w.first() == w.get(1)) {
            return Err(bad_job(
                "two sets share a name".to_owned(),
                "Two of these sets have the same name, so their files would land on top of each \
                 other. Rename one.",
            ));
        }
        for set in &self.sets {
            set.validate()?;
        }
        self.metadata.validate()
    }
}

// ---------------------------------------------------------------------------
// What came out
// ---------------------------------------------------------------------------

/// One file, written and read back.
///
/// **`hash` is the digest of the bytes re-read from the destination**, never of the buffer that was
/// written. The distinction is the whole of section 6.1's first bullet: a short write, a full disk,
/// a NAS that acknowledges and drops and a failing card all produce a correct buffer and a wrong
/// file, and only a read-back notices.
///
/// `render_hash` is phase 14's four-input hash, so a delivered file can be re-created from the RAW's
/// content hash, the canonical recipe, the engine string and the output spec - and
/// `AURA-RENDER-8007` can say which of the four moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedFile {
    /// The photograph.
    pub image: ImageId,
    /// Which set wrote it.
    pub set: String,
    /// Where it landed, relative to the destination root.
    pub path: PathBuf,
    /// Its size on disk.
    pub bytes: u64,
    /// BLAKE3 of the bytes **read back** from the destination, lower-case hex.
    pub hash: String,
    /// Written width.
    pub width: u32,
    /// Written height.
    pub height: u32,
    /// Phase 14's render hash for the pixels inside it.
    pub render_hash: String,
    /// Whether the bytes were re-read. False only on a job with `verify = false`.
    pub verified: bool,
    /// Whether a name collision was resolved by suffixing this file.
    pub renamed: bool,
    /// What the writer wants said about this file.
    pub reasons: Vec<DeliveryReason>,
}

/// What was delivered, and what it was made of. Section 5's frozen shape.
///
/// The last four fields are the ones that make this a *delivery* record rather than a file listing.
/// `qc_report_path` points at phase 27's archived report; `cleanup_disclosures` carries phase 24's
/// removals, because a removal that is not disclosed in the thing handed to the client is a removal
/// nobody can audit; and `engine_versions` is what makes the whole gallery reproducible from four
/// values a year later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryManifest {
    /// The wedding.
    pub project: ProjectId,
    /// When the manifest was sealed, epoch milliseconds.
    pub created_at: Timestamp,
    /// Every file: path, bytes, hash.
    pub files: Vec<(PathBuf, u64, String)>,
    /// Every set and how many files it produced.
    pub sets: Vec<(String, u32)>,
    /// Phase 27's report, if one was archived beside the delivery.
    pub qc_report_path: Option<PathBuf>,
    /// Phase 24's disclosures: which photograph, and what was removed from it.
    pub cleanup_disclosures: Vec<(ImageId, String)>,
    /// The versions that produced this: app, render engine, recipe schema, model set, profile.
    pub engine_versions: Vec<(String, String)>,
}

impl DeliveryManifest {
    /// Total bytes across every file.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.files
            .iter()
            .map(|(_, bytes, _)| *bytes)
            .fold(0_u64, u64::saturating_add)
    }

    /// Whether every file in the manifest carries a 64-character digest.
    ///
    /// A manifest is only worth having if this is true, and it is checked rather than assumed
    /// because a manifest with one blank hash in it looks exactly like a manifest.
    #[must_use]
    pub fn fully_hashed(&self) -> bool {
        !self.files.is_empty()
            && self
                .files
                .iter()
                .all(|(_, _, hash)| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
    }
}

// ---------------------------------------------------------------------------
// Upload
// ---------------------------------------------------------------------------

/// Where one file has got to on its way to a provider.
///
/// **The unit is a file**, which is what makes section 10.1's "provider uploads resume correctly
/// after a network drop" achievable: a resumed job re-sends the tail of one file, not the head of a
/// wedding. A state machine whose unit was the job would have exactly two states and would have to
/// start again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum UploadState {
    /// Not started.
    Pending,
    /// Partially sent. `sent` bytes have been accepted by the far end.
    InProgress {
        /// Bytes the far end has acknowledged.
        sent: u64,
        /// How many times this file has been resumed.
        resumes: u32,
    },
    /// Sent, and the far end's digest matched.
    Verified,
    /// Sent, and the far end's digest did **not** match. Distinct from a failure to send.
    Corrupt,
    /// Not sent, and this is why.
    Failed {
        /// The error code the transport reported.
        code: String,
    },
}

impl UploadState {
    /// The stored word.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress { .. } => "in_progress",
            Self::Verified => "verified",
            Self::Corrupt => "corrupt",
            Self::Failed { .. } => "failed",
        }
    }

    /// Whether this file still needs work.
    #[must_use]
    pub const fn is_outstanding(&self) -> bool {
        matches!(
            self,
            Self::Pending | Self::InProgress { .. } | Self::Corrupt | Self::Failed { .. }
        )
    }

    /// Bytes already accepted by the far end.
    #[must_use]
    pub const fn sent(&self) -> u64 {
        match self {
            Self::InProgress { sent, .. } => *sent,
            Self::Verified => u64::MAX,
            _ => 0,
        }
    }
}

/// Which set goes where inside a provider.
///
/// Section 6.2's "per-set mapping". A gallery goes to the client's main collection, a teaser to a
/// preview collection, the album to a private one - and getting that wrong publishes the whole
/// wedding on the night of the wedding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMapping {
    /// The set's name, as the job spells it.
    pub set: String,
    /// The provider-side collection or folder it lands in.
    pub remote: String,
    /// Whether the provider should publish it immediately.
    ///
    /// Defaults to false, and that default is the point: publishing is a thing a photographer does,
    /// not a thing an upload does.
    #[serde(default)]
    pub publish: bool,
}

/// One file's place in an upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadItem {
    /// The photograph.
    pub image: ImageId,
    /// Which set it belongs to.
    pub set: String,
    /// The local file, relative to the export root.
    pub path: PathBuf,
    /// Its size.
    pub bytes: u64,
    /// The digest the local file was verified with.
    pub hash: String,
    /// Where it has got to.
    pub state: UploadState,
}

/// How an upload is going.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UploadProgress {
    /// Files in the upload.
    pub files: u32,
    /// Files the far end has accepted and whose digest matched.
    pub verified: u32,
    /// Files still outstanding.
    pub outstanding: u32,
    /// Files that failed and will not be retried without a person.
    pub failed: u32,
    /// Bytes accepted.
    pub bytes_sent: u64,
    /// Bytes in total.
    pub bytes_total: u64,
    /// How many times any file was resumed. Telemetry's `resumes`.
    pub resumes: u32,
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why the delivery surface did what it did.
///
/// Invariant 2 in the phase that writes files: a delivered gallery whose files differ from what a
/// photographer expected has to be able to say why, per file, in the manifest and in the panel.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryCode {
    // -- export -------------------------------------------------------------------------
    /// The file was written and its bytes read back and hashed. The ordinary case.
    #[default]
    WrittenAndVerified,
    /// The file was written and **not** read back, because the job asked for no verification.
    WrittenUnverified,
    /// The name this template produced was already taken, so a numeral was appended.
    NameCollisionResolved,
    /// This template names every frame of the set the same thing.
    NamingTemplateNotUnique,
    /// A token had nothing to substitute - no chapter, no camera, no couple - so it was dropped.
    NameTokenUnavailable,
    /// The requested size was larger than the frame, so the frame was written at its own size.
    ResizeIgnoredUpscale,
    /// Output sharpening was applied for this output size.
    SharpenedForOutput,
    /// The location tags were removed.
    GpsStripped,
    /// The camera body's serial number was removed.
    SerialStripped,
    /// The colour space's ICC profile was embedded.
    IccEmbedded,
    /// The ICC profile for this space is not in the build, so the file carries the space's name
    /// only. A caveat, never silent.
    IccUnavailable,
    /// An XMP sidecar was written beside the file.
    SidecarWritten,
    /// This frame carries phase 24 disclosures, which are in the manifest.
    CleanupDisclosed,
    /// The frame could not be rendered, so nothing was written for it.
    RenderUnavailable,
    /// The bytes read back did not match the bytes written. **The job fails.**
    VerificationFailed,
    /// The destination did not have room for the job.
    DestinationFull,
    /// The destination could not be written to at all.
    DestinationUnwritable,
    /// The job was cancelled; what had been written is listed and the manifest is not sealed.
    Cancelled,

    // -- backup -------------------------------------------------------------------------
    /// The file was copied to the backup destination and its digest matched the source.
    BackupVerified,
    /// The backup destination already held a file with this digest, so nothing was copied.
    BackupAlreadyPresent,
    /// The backup destination held a file with this name and a **different** digest.
    BackupDiverged,
    /// The backup destination went away mid-job.
    BackupUnreachable,

    // -- upload -------------------------------------------------------------------------
    /// The provider accepted the file and its digest matched.
    UploadVerified,
    /// The upload was resumed from a byte offset the provider reported.
    UploadResumed,
    /// The provider accepted the file and reported a **different** digest.
    UploadCorrupt,
    /// The provider refused the file.
    UploadRefused,
    /// The provider could not be reached. Not a corruption; a different runbook.
    ProviderUnreachable,
    /// No credential is configured for this provider.
    ProviderNotConfigured,
    /// This set has no mapping, so it was not uploaded.
    SetUnmapped,
    /// The set was uploaded and left unpublished, which is the default.
    LeftUnpublished,
}

impl DeliveryCode {
    /// Every code, in the order this module declares them.
    pub const ALL: [Self; 30] = [
        Self::WrittenAndVerified,
        Self::WrittenUnverified,
        Self::NameCollisionResolved,
        Self::NamingTemplateNotUnique,
        Self::NameTokenUnavailable,
        Self::ResizeIgnoredUpscale,
        Self::SharpenedForOutput,
        Self::GpsStripped,
        Self::SerialStripped,
        Self::IccEmbedded,
        Self::IccUnavailable,
        Self::SidecarWritten,
        Self::CleanupDisclosed,
        Self::RenderUnavailable,
        Self::VerificationFailed,
        Self::DestinationFull,
        Self::DestinationUnwritable,
        Self::Cancelled,
        Self::BackupVerified,
        Self::BackupAlreadyPresent,
        Self::BackupDiverged,
        Self::BackupUnreachable,
        Self::UploadVerified,
        Self::UploadResumed,
        Self::UploadCorrupt,
        Self::UploadRefused,
        Self::ProviderUnreachable,
        Self::ProviderNotConfigured,
        Self::SetUnmapped,
        Self::LeftUnpublished,
    ];

    /// How many codes there are.
    pub const COUNT: usize = 30;

    /// The stable slug, in the catalog, on the wire and in `docs/reason-codes.md`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WrittenAndVerified => "written_and_verified",
            Self::WrittenUnverified => "written_unverified",
            Self::NameCollisionResolved => "name_collision_resolved",
            Self::NamingTemplateNotUnique => "naming_template_not_unique",
            Self::NameTokenUnavailable => "name_token_unavailable",
            Self::ResizeIgnoredUpscale => "resize_ignored_upscale",
            Self::SharpenedForOutput => "sharpened_for_output",
            Self::GpsStripped => "gps_stripped",
            Self::SerialStripped => "serial_stripped",
            Self::IccEmbedded => "icc_embedded",
            Self::IccUnavailable => "icc_unavailable",
            Self::SidecarWritten => "sidecar_written",
            Self::CleanupDisclosed => "cleanup_disclosed",
            Self::RenderUnavailable => "render_unavailable",
            Self::VerificationFailed => "verification_failed",
            Self::DestinationFull => "destination_full",
            Self::DestinationUnwritable => "destination_unwritable",
            Self::Cancelled => "cancelled",
            Self::BackupVerified => "backup_verified",
            Self::BackupAlreadyPresent => "backup_already_present",
            Self::BackupDiverged => "backup_diverged",
            Self::BackupUnreachable => "backup_unreachable",
            Self::UploadVerified => "upload_verified",
            Self::UploadResumed => "upload_resumed",
            Self::UploadCorrupt => "upload_corrupt",
            Self::UploadRefused => "upload_refused",
            Self::ProviderUnreachable => "provider_unreachable",
            Self::ProviderNotConfigured => "provider_not_configured",
            Self::SetUnmapped => "set_unmapped",
            Self::LeftUnpublished => "left_unpublished",
        }
    }

    /// The sentence a photographer reads.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::WrittenAndVerified => "Written, then read back and checked.",
            Self::WrittenUnverified => {
                "Written without the read-back check, because this job asked for no verification."
            }
            Self::NameCollisionResolved => {
                "Another photograph had already taken this name, so a number was added."
            }
            Self::NamingTemplateNotUnique => {
                "This naming template gives every photograph the same name, so numbers were added \
                 to tell them apart."
            }
            Self::NameTokenUnavailable => {
                "Part of the name had nothing to fill it in with, so it was left out."
            }
            Self::ResizeIgnoredUpscale => {
                "The size asked for was bigger than the photograph, so it was written at its own \
                 size rather than enlarged."
            }
            Self::SharpenedForOutput => "Sharpened for the size it was written at.",
            Self::GpsStripped => "The location was removed.",
            Self::SerialStripped => "The camera's serial number was removed.",
            Self::IccEmbedded => "The colour profile was embedded.",
            Self::IccUnavailable => {
                "AURA does not have the colour profile for this space, so the file names the space \
                 without embedding it."
            }
            Self::CleanupDisclosed => {
                "Something was removed from this photograph, and the delivery manifest says what."
            }
            Self::SidecarWritten => "An XMP sidecar was written beside it for Lightroom.",
            Self::RenderUnavailable => {
                "This photograph could not be rendered, so nothing was written."
            }
            Self::VerificationFailed => {
                "What was read back did not match what was written, so this delivery was stopped."
            }
            Self::DestinationFull => "There is not enough room where this was going.",
            Self::DestinationUnwritable => "AURA could not write to that place at all.",
            Self::Cancelled => "You stopped this export.",
            Self::BackupVerified => "Copied to the backup and checked against the original.",
            Self::BackupAlreadyPresent => "The backup already had this exact file.",
            Self::BackupDiverged => {
                "The backup has a different file under this name. Nothing was overwritten."
            }
            Self::BackupUnreachable => "The backup went away part-way through.",
            Self::UploadVerified => "Uploaded, and the gallery's checksum matched.",
            Self::UploadResumed => "Uploading continued from where it stopped.",
            Self::UploadCorrupt => {
                "The gallery received something different from what was sent, so it will be sent \
                 again."
            }
            Self::UploadRefused => "The gallery refused this file.",
            Self::ProviderUnreachable => "AURA could not reach that gallery.",
            Self::ProviderNotConfigured => "There is no sign-in saved for that gallery.",
            Self::SetUnmapped => "This set has nowhere to go in that gallery, so it was not sent.",
            Self::LeftUnpublished => "Uploaded but not published, which is what AURA always does.",
        }
    }

    /// Whether this code stops the job it appears on.
    ///
    /// Three do, and they are the three where continuing would deliver something wrong: a failed
    /// verification, a full destination and an unwritable one. Everything else is per-file.
    #[must_use]
    pub const fn is_fatal(self) -> bool {
        matches!(
            self,
            Self::VerificationFailed | Self::DestinationFull | Self::DestinationUnwritable
        )
    }

    /// Parse the stored slug.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8020` when the slug is not one this build knows, which is what a catalog written
    /// by a newer release looks like.
    pub fn parse(slug: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == slug)
            .ok_or_else(|| {
                AuraError::new(
                    ErrorCode("AURA-RENDER-8020"),
                    Severity::Degraded,
                    Recovery::Fallback,
                    format!("unknown delivery reason code `{slug}`"),
                    "AURA found a delivery note it does not recognise, which usually means this \
                     wedding was delivered by a newer version.",
                )
            })
    }
}

impl fmt::Display for DeliveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason, with the specific half of its sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryReason {
    /// Which code.
    pub code: DeliveryCode,
    /// The measured half, when there is one: the numeral appended, the size written, the byte
    /// offset resumed from.
    #[serde(default)]
    pub detail: Option<String>,
}

impl DeliveryReason {
    /// A reason with no detail.
    #[must_use]
    pub const fn plain(code: DeliveryCode) -> Self {
        Self { code, detail: None }
    }

    /// A reason with its measured half.
    #[must_use]
    pub fn with(code: DeliveryCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
        }
    }

    /// The whole sentence.
    #[must_use]
    pub fn sentence(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{} ({detail})", self.code.user_text()),
            None => self.code.user_text().to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Outlines
// ---------------------------------------------------------------------------

/// What a project's exports have covered and found.
///
/// **Three denominators, on purpose.** `photos` is the project, `selected` is phase 12's gallery and
/// `requested` is what this job was asked for. A panel that measured an album export against the
/// project would report an 80-frame album as having missed 98 % of a wedding it was never asked
/// about.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExportOutline {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs phase 12 selected.
    pub selected: u32,
    /// Photographs the last job asked for, across every set.
    pub requested: u32,
    /// Files written.
    pub written: u32,
    /// Files read back and hashed.
    pub verified: u32,
    /// Files written without the read-back check.
    pub unverified: u32,
    /// Files whose read-back did not match.
    pub corrupt: u32,
    /// Photographs that could not be rendered.
    pub render_failed: u32,
    /// Names that collided and were suffixed.
    pub renamed: u32,
    /// Sidecars written.
    pub sidecars: u32,
    /// Bytes written.
    pub bytes: u64,
    /// Whether the last job sealed a manifest.
    pub manifest_sealed: bool,
    /// Wall-clock milliseconds of the last job.
    pub ms: u64,
}

impl ExportOutline {
    /// The share of requested photographs that were written, `0..1`.
    #[must_use]
    pub fn completion(&self) -> f32 {
        if self.requested == 0 {
            return 0.0;
        }
        f64_ratio(u64::from(self.written), u64::from(self.requested))
    }

    /// The share of written files that were read back, `0..1`.
    ///
    /// The number section 13's first acceptance criterion is about. Anything below 1.0 on a job
    /// that asked for verification is a defect; anything below 1.0 on a job that did not is a
    /// choice somebody made, and the outline cannot tell them apart on its own - which is why
    /// `unverified` is a separate count rather than a subtraction.
    #[must_use]
    pub fn verified_share(&self) -> f32 {
        if self.written == 0 {
            return 0.0;
        }
        f64_ratio(u64::from(self.verified), u64::from(self.written))
    }
}

/// What a project's backups and uploads have covered.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeliveryOutline {
    /// Files in the last export.
    pub files: u32,
    /// Backup destinations configured.
    pub backups: u32,
    /// Files present and matching at every backup destination.
    pub backed_up: u32,
    /// Files whose backup copy has a different digest.
    pub diverged: u32,
    /// Providers configured.
    pub providers: u32,
    /// Files a provider has accepted and verified.
    pub uploaded: u32,
    /// Files still outstanding at a provider.
    pub outstanding: u32,
    /// Files a provider refused.
    pub refused: u32,
    /// How many times a file was resumed.
    pub resumes: u32,
    /// Sets with no mapping.
    pub unmapped_sets: u32,
    /// Bytes sent.
    pub bytes_sent: u64,
}

// ---------------------------------------------------------------------------
// The services
// ---------------------------------------------------------------------------

/// The one way to write a delivered file.
///
/// **Twenty-sixth service of its kind and the first that produces something outside the catalog.**
/// Phase 14's `RenderService` produces pixels in memory; this produces a file a photographer sends
/// to a couple. No later phase may keep its own exporter, its own naming scheme or its own idea of
/// what a verified write is - two answers to "what did we deliver" is a manifest that does not
/// match the gallery.
pub trait ExportService: Send + Sync + fmt::Debug {
    /// What a project's exports covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<ExportOutline>;

    /// Run a job, writing every file and reading each one back.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the job does not validate, `AURA-RENDER-8022` when a written file did not
    /// read back the same, `AURA-RENDER-8023` when the destination is full or unwritable, and
    /// `AURA-RENDER-*` when a frame could not be rendered.
    fn run(&self, project: ProjectId, job: &ExportJob) -> AuraResult<DeliveryManifest>;

    /// Every file the last job wrote.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn files(&self, project: ProjectId) -> AuraResult<Vec<ExportedFile>>;

    /// The last sealed manifest, or `None` when this project has not been delivered.
    ///
    /// `None` is not an empty manifest. A wedding nobody has exported and a wedding whose export
    /// wrote nothing are different answers.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn manifest(&self, project: ProjectId) -> AuraResult<Option<DeliveryManifest>>;

    /// The names a job would produce, without writing anything.
    ///
    /// A dry run, because section 10.1 asks for collision-free names across 4,000 files and a
    /// photographer should be able to see the answer before the wedding is written.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the job does not validate.
    fn preview_names(
        &self,
        project: ProjectId,
        job: &ExportJob,
    ) -> AuraResult<Vec<(ImageId, PathBuf)>>;
}

/// The one way to get a delivered file somewhere else.
///
/// **Twenty-seventh service of its kind.** Backups and client galleries are the same shape - take a
/// verified local file, put it somewhere, check what arrived - and they are one service because the
/// thing that must not be duplicated is the *verification*, not the transport.
pub trait DeliveryService: Send + Sync + fmt::Debug {
    /// What a project's backups and uploads covered.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<DeliveryOutline>;

    /// Copy a sealed delivery to a backup destination, verifying every file.
    ///
    /// # Errors
    ///
    /// `AURA-DLV-10002` when the destination cannot be reached, `AURA-DLV-10003` when a copy did not
    /// verify.
    fn backup(&self, project: ProjectId, to: &Destination) -> AuraResult<DeliveryOutline>;

    /// Start or resume an upload to a provider.
    ///
    /// Resuming is not a separate call: an upload that has already sent half a wedding picks up
    /// from its stored per-file state, which is what makes a network drop a pause rather than a
    /// restart.
    ///
    /// # Errors
    ///
    /// `AURA-DLV-10001` when the provider is unknown, `AURA-DLV-10004` when no credential is
    /// configured, `AURA-DLV-10002` when it cannot be reached.
    fn upload(
        &self,
        project: ProjectId,
        provider: &ProviderId,
        mapping: &[SetMapping],
    ) -> AuraResult<UploadProgress>;

    /// How an upload is going.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn progress(&self, project: ProjectId, provider: &ProviderId) -> AuraResult<UploadProgress>;

    /// Every file's state at a provider.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn items(&self, project: ProjectId, provider: &ProviderId) -> AuraResult<Vec<UploadItem>>;

    /// The providers this machine has configured, and whether each has a credential.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn providers(&self) -> AuraResult<Vec<(ProviderId, bool)>>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The error a job that does not validate produces.
fn bad_job(detail: String, user: &str) -> AuraError {
    AuraError::new(
        ErrorCode("AURA-RENDER-8021"),
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        user,
    )
}

/// A ratio that is exact in `f64` for every count this product can produce.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn f64_ratio(num: u64, den: u64) -> f32 {
    if den == 0 {
        return 0.0;
    }
    (num as f64 / den as f64) as f32
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::assertions_on_constants,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::disallowed_methods,
    clippy::panic
)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_a_distinct_slug_and_a_sentence() {
        let mut slugs: Vec<&str> = DeliveryCode::ALL.iter().map(|c| c.as_str()).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), DeliveryCode::COUNT);
        assert_eq!(DeliveryCode::ALL.len(), DeliveryCode::COUNT);
        for code in DeliveryCode::ALL {
            assert!(!code.user_text().is_empty());
            assert_eq!(DeliveryCode::parse(code.as_str()).unwrap(), code);
        }
    }

    #[test]
    fn a_resize_never_upscales() {
        let r = Resize::LongEdge { pixels: 8000 };
        assert_eq!(r.target(4000, 3000), (4000, 3000));
        assert!(r.would_upscale(4000, 3000));

        let r = Resize::LongEdge { pixels: 2048 };
        assert_eq!(r.target(4000, 3000), (2048, 1536));
        assert!(!r.would_upscale(4000, 3000));

        let r = Resize::Fit {
            width: 1000,
            height: 1000,
        };
        assert_eq!(r.target(4000, 3000), (1000, 750));
    }

    #[test]
    fn a_template_cannot_name_a_folder_or_an_unknown_token() {
        assert!(NamingTemplate::parse("{date}/{seq}").is_err());
        assert!(NamingTemplate::parse("../{seq}").is_err());
        assert!(NamingTemplate::parse("{venue}_{seq}").is_err());
        assert!(NamingTemplate::parse("{date}_{seq").is_err());
        assert!(NamingTemplate::parse("").is_err());

        let t = NamingTemplate::parse("{date}_{couple}_{seq}").unwrap();
        assert_eq!(
            t.tokens(),
            vec![NameToken::Date, NameToken::Couple, NameToken::Sequence]
        );
        assert!(t.is_distinguishing());

        // A template with neither {seq} nor {original} names every frame the same thing. Legal,
        // and worth saying out loud.
        let flat = NamingTemplate::parse("{date}_{couple}").unwrap();
        assert!(!flat.is_distinguishing());
    }

    #[test]
    fn stripping_the_location_is_the_default() {
        let p = MetadataPolicy::default();
        assert!(p.strip_gps);
        assert!(p.strip_camera_serial);
    }

    #[test]
    fn a_job_is_verified_unless_somebody_says_otherwise() {
        let job = ExportJob::new(
            vec![],
            Destination::Folder {
                path: PathBuf::from("."),
            },
        );
        assert!(job.verify);
    }

    #[test]
    fn two_sets_with_one_name_are_refused() {
        let set = |name: &str| ExportSet {
            name: name.to_owned(),
            images: vec![ImageId::new()],
            format: FileFormat::Jpeg,
            quality: 92,
            resize: Resize::Full,
            sharpen: OutputSharpen::Screen,
            naming: NamingTemplate::default(),
            colour: DeliveryColour::Srgb,
            bit_depth: 8,
            sidecar: false,
        };
        let job = ExportJob::new(
            vec![set("gallery"), set("gallery")],
            Destination::Folder {
                path: PathBuf::from("."),
            },
        );
        assert!(job.validate().is_err());

        let job = ExportJob::new(
            vec![set("gallery"), set("album")],
            Destination::Folder {
                path: PathBuf::from("."),
            },
        );
        assert!(job.validate().is_ok());
    }

    #[test]
    fn sixteen_bits_is_refused_on_a_jpeg() {
        let set = ExportSet {
            name: "gallery".to_owned(),
            images: vec![ImageId::new()],
            format: FileFormat::Jpeg,
            quality: 92,
            resize: Resize::Full,
            sharpen: OutputSharpen::None,
            naming: NamingTemplate::default(),
            colour: DeliveryColour::Srgb,
            bit_depth: 16,
            sidecar: false,
        };
        assert!(set.validate().is_err());
    }

    #[test]
    fn output_sharpening_grows_as_a_frame_is_scaled_down() {
        let full = OutputSharpen::Screen.amount(1.0);
        let web = OutputSharpen::Screen.amount(0.24);
        assert!(web > full, "{web} should exceed {full}");
        assert_eq!(OutputSharpen::None.amount(0.2), 0.0);
        assert!(OutputSharpen::Print.amount(1.0) > OutputSharpen::Screen.amount(1.0));
    }

    #[test]
    fn a_manifest_with_a_blank_hash_is_not_fully_hashed() {
        let mut m = DeliveryManifest {
            project: ProjectId::new(),
            created_at: 0,
            files: vec![(PathBuf::from("a.jpg"), 10, "a".repeat(64))],
            sets: vec![("gallery".to_owned(), 1)],
            qc_report_path: None,
            cleanup_disclosures: Vec::new(),
            engine_versions: Vec::new(),
        };
        assert!(m.fully_hashed());
        m.files.push((PathBuf::from("b.jpg"), 10, String::new()));
        assert!(!m.fully_hashed());
    }

    #[test]
    fn a_destination_knows_whether_it_needs_a_network() {
        assert!(!Destination::Folder {
            path: PathBuf::from("/x")
        }
        .needs_network());
        assert!(!Destination::Nas {
            path: PathBuf::from("/x")
        }
        .needs_network());
        assert!(Destination::Provider(ProviderId::parse("pic-time").unwrap()).needs_network());
        assert!(ProviderId::parse("Pic Time").is_err());
    }

    #[test]
    fn only_three_codes_stop_a_job() {
        let fatal: Vec<_> = DeliveryCode::ALL
            .iter()
            .copied()
            .filter(|c| c.is_fatal())
            .collect();
        assert_eq!(
            fatal,
            vec![
                DeliveryCode::VerificationFailed,
                DeliveryCode::DestinationFull,
                DeliveryCode::DestinationUnwritable
            ]
        );
    }
}
