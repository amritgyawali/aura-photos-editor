//! What is removed before anything leaves the machine, and before anything
//! reaches a log.
//!
//! The module has two halves that share one rule: **redaction happens at the
//! boundary, not at the source**. Callers hand the gateway whatever they have;
//! this module decides what survives the trip.
//!
//! The first half is [`scrub`], which removes key-shaped strings from any text
//! about to be traced, stored in an audit row, or put in an error's `detail`.
//! Provider error bodies routinely echo the key back - "invalid api key
//! sk-ant-…" is a real response shape - so a gateway that logged raw provider
//! errors would publish the user's key on the first typo.
//!
//! The second half is [`ExifSummary`], the only description of a photograph that
//! may be sent, and [`blur_regions`], the optional pre-upload face blur. Both are
//! allow-lists rather than deny-lists. A deny-list of EXIF tags is a promise to
//! keep up with every camera maker's private tags forever, and that promise gets
//! broken by the first firmware update.

use serde::{Deserialize, Serialize};

/// Text substituted for anything that looks like a credential.
pub const REDACTED: &str = "<redacted key>";

/// Prefixes that are always a credential, whatever else the token looks like.
/// `Bearer` is not here: it is a word followed by a space, and the token scanner
/// below never sees a space. An `Authorization` header echoed back by a provider
/// is caught by the token after it, which is the credential itself.
const KEY_PREFIXES: &[&str] = &[
    "sk-",  // OpenAI, Anthropic
    "sk_",  // several compatible gateways
    "AIza", // Google
    "gsk_", // Groq
    "xai-", // xAI
    "ghp_",
    "github_pat_", // not ours, but never worth logging
];

/// Remove key-shaped strings from text that is about to be logged or stored.
///
/// Two rules, and the second exists because the first can never be complete:
///
/// 1. A token starting with a known credential prefix is replaced whole.
/// 2. A token of 32 characters or more drawn from the credential alphabet, that
///    contains both an upper-case letter and a digit, is replaced.
///
/// Rule 2 deliberately does **not** fire on all-lower-case hexadecimal, because
/// every content hash, prompt hash and cache key in the product is exactly that,
/// and those are the identifiers a support engineer needs in order to find the
/// call being complained about.
#[must_use]
pub fn scrub(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();

    // A token ends at anything that cannot appear inside a credential. Quotes and
    // commas are separators, so a key inside a JSON body is still found.
    let is_token_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';

    for ch in text.chars() {
        if is_token_char(ch) {
            token.push(ch);
            continue;
        }
        out.push_str(&scrub_token(&token));
        token.clear();
        out.push(ch);
    }
    out.push_str(&scrub_token(&token));
    out
}

fn scrub_token(token: &str) -> String {
    if token.is_empty() {
        return String::new();
    }
    if KEY_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() > prefix.len() + 4)
    {
        return REDACTED.to_string();
    }
    let long_enough = token.len() >= 32;
    let has_upper = token.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = token.chars().any(|c| c.is_ascii_digit());
    if long_enough && has_upper && has_digit {
        return REDACTED.to_string();
    }
    token.to_string()
}

/// True when the text still contains something key-shaped.
///
/// The key-safety test in `tests/privacy.rs` runs this over every audit row,
/// every cache row and the whole tracing output of a gate run.
#[must_use]
pub fn looks_like_a_key(text: &str) -> bool {
    scrub(text) != text
}

/// Everything about a photograph that may be described to a cloud model.
///
/// An allow-list, and a short one. What is **not** here is the point: no GPS, no
/// filename, no camera serial number, no owner or artist tag, no absolute
/// timestamp, no couple's name, no folder path. A model naming a wedding ritual
/// needs to know it was a 35 mm frame at f/1.8, ISO 6400, with the flash off. It
/// does not need to know where the church is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExifSummary {
    /// Camera model as the maker writes it, e.g. `Canon EOS R5`.
    pub camera: Option<String>,
    /// Lens model.
    pub lens: Option<String>,
    /// Focal length in whole millimetres.
    pub focal_mm: Option<u16>,
    /// Aperture in hundredths of an f-stop: 180 is f/1.8.
    pub aperture_x100: Option<u16>,
    /// Shutter speed as a denominator: 250 is 1/250 s. Zero for one second or
    /// longer, with `shutter_seconds` carrying the whole number instead.
    pub shutter_denominator: Option<u32>,
    /// Whole seconds, for long exposures.
    pub shutter_seconds: Option<u16>,
    /// Sensitivity.
    pub iso: Option<u32>,
    /// Whether the flash fired.
    pub flash: Option<bool>,
    /// Seconds from the start of the segment. **Never an absolute time**: an
    /// absolute timestamp plus a guessed venue is an identification.
    pub offset_seconds: Option<u32>,
    /// Orientation as the 1-8 EXIF value, so the model knows which way is up.
    pub orientation: Option<u8>,
}

impl ExifSummary {
    /// A one-line description for the prompt, in a fixed field order.
    ///
    /// Fixed order matters: the text goes into the prompt hash, and a summary
    /// rendered from a map would reorder between runs and miss the cache.
    #[must_use]
    pub fn render(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(camera) = &self.camera {
            parts.push(camera.clone());
        }
        if let Some(lens) = &self.lens {
            parts.push(lens.clone());
        }
        if let Some(mm) = self.focal_mm {
            parts.push(format!("{mm}mm"));
        }
        if let Some(hundredths) = self.aperture_x100 {
            parts.push(format!("f/{:.1}", f64::from(hundredths) / 100.0));
        }
        if let Some(denominator) = self.shutter_denominator {
            parts.push(format!("1/{denominator}s"));
        } else if let Some(seconds) = self.shutter_seconds {
            parts.push(format!("{seconds}s"));
        }
        if let Some(iso) = self.iso {
            parts.push(format!("ISO {iso}"));
        }
        if let Some(flash) = self.flash {
            parts.push(if flash {
                "flash".into()
            } else {
                "no flash".into()
            });
        }
        if let Some(offset) = self.offset_seconds {
            parts.push(format!("+{offset}s"));
        }
        parts.join(", ")
    }

    /// True when this summary carries nothing at all, so the prompt can omit it
    /// rather than sending an empty line.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.render().is_empty()
    }
}

/// A rectangle of pixels to obscure before upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// Left edge, pixels from the left.
    pub x: u32,
    /// Top edge, pixels from the top.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Region {
    /// A rectangle grown by `pad` pixels on every side, clamped to the image.
    ///
    /// Face detectors return a tight box around the features. A tight box blurs
    /// the eyes and nose and leaves the hairline, jaw and ears - which is enough
    /// to recognise someone. Ten per cent of the larger edge is added before
    /// blurring for that reason.
    #[must_use]
    pub fn padded(self, pad: u32, width: u32, height: u32) -> Self {
        let x = self.x.saturating_sub(pad);
        let y = self.y.saturating_sub(pad);
        let right = (self.x + self.width + pad).min(width);
        let bottom = (self.y + self.height + pad).min(height);
        Self {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        }
    }
}

/// How many box-blur passes approximate a Gaussian well enough to be
/// irreversible at these radii. Three is the standard answer.
const BLUR_PASSES: usize = 3;

/// Blur every region in place, in an interleaved 8-bit RGB buffer.
///
/// The blur reads only pixels inside the region, so no detail leaks out of the
/// rectangle and, more importantly, no detail leaks *in*: a blur that averaged
/// with the surrounding pixels would leave the face's own energy recoverable from
/// the difference between the blurred patch and its neighbourhood.
///
/// Returns how many regions were actually blurred. A region entirely outside the
/// image is skipped rather than being an error - a detector that reports a face
/// half off the edge is normal.
pub fn blur_regions(rgb: &mut [u8], width: u32, height: u32, regions: &[Region]) -> usize {
    let mut blurred = 0usize;
    for region in regions {
        let pad = region.width.max(region.height) / 10;
        let region = region.padded(pad, width, height);
        if region.width < 2 || region.height < 2 {
            continue;
        }
        let radius = (region.width.min(region.height) / 6).max(2);
        for _ in 0..BLUR_PASSES {
            box_blur_region(rgb, width, height, region, radius);
        }
        blurred += 1;
    }
    blurred
}

/// One separable box-blur pass over one rectangle.
fn box_blur_region(rgb: &mut [u8], width: u32, height: u32, region: Region, radius: u32) {
    let (rw, rh) = (region.width as usize, region.height as usize);
    let radius = radius as usize;
    let stride = width as usize * 3;
    if region.x + region.width > width || region.y + region.height > height {
        return;
    }

    // Horizontal pass into a scratch copy of the rectangle, then vertical back.
    let mut patch = vec![0u8; rw * rh * 3];
    for row in 0..rh {
        let base = (region.y as usize + row) * stride + region.x as usize * 3;
        for column in 0..rw {
            for channel in 0..3 {
                let low = column.saturating_sub(radius);
                let high = (column + radius).min(rw - 1);
                let mut total = 0u32;
                let mut count = 0u32;
                for sample in low..=high {
                    if let Some(value) = rgb.get(base + sample * 3 + channel) {
                        total += u32::from(*value);
                        count += 1;
                    }
                }
                if let Some(slot) = patch.get_mut((row * rw + column) * 3 + channel) {
                    *slot = u8::try_from(total / count.max(1)).unwrap_or(u8::MAX);
                }
            }
        }
    }

    for column in 0..rw {
        for row in 0..rh {
            for channel in 0..3 {
                let low = row.saturating_sub(radius);
                let high = (row + radius).min(rh - 1);
                let mut total = 0u32;
                let mut count = 0u32;
                for sample in low..=high {
                    if let Some(value) = patch.get((sample * rw + column) * 3 + channel) {
                        total += u32::from(*value);
                        count += 1;
                    }
                }
                let target = (region.y as usize + row) * stride + (region.x as usize + column) * 3;
                if let Some(slot) = rgb.get_mut(target + channel) {
                    *slot = u8::try_from(total / count.max(1)).unwrap_or(u8::MAX);
                }
            }
        }
    }
}
