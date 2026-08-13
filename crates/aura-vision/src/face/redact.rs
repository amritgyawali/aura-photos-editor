//! Blurring a thumbnail before it is allowed to leave the machine.
//!
//! Section 7 of the phase document permits exactly one cloud call in phase 06 - an
//! optional couple hint when the local evidence is ambiguous - and it constrains what
//! may be sent: "up to 8 blurred-background thumbnails (faces visible only if the user
//! allows)".
//!
//! That sentence describes two different redactions, and this module implements both so
//! that the claim is enforced by code rather than by a prompt:
//!
//! * [`blur_background`] blurs everything **except** the face boxes. The venue, the
//!   other guests, the signage, the table plan with names on it - all of it goes. What
//!   survives is the faces, which is what the question "which two of these people are
//!   the couple" actually needs.
//! * [`blur_all`] blurs the faces too, for a photographer who has not granted
//!   face-visible cloud reasoning. The result still carries composition, framing and
//!   who-is-next-to-whom, which is weak evidence and honest about being weak.
//!
//! ## Why a box blur, and why this radius
//!
//! A box blur, applied twice, separably. Two passes of a box blur approximate a
//! Gaussian well enough that no structure survives, and separable box blur is `O(n)` in
//! the radius rather than `O(r^2)` - which matters because this runs on eight
//! thumbnails on the interactive path while a photographer waits.
//!
//! The radius is a **fraction of the image**, not a pixel count. A 12-pixel blur
//! destroys a 384 px thumbnail and does nothing to a 2048 px one, so a fixed radius
//! would silently weaken as thumbnails grew. [`BLUR_FRACTION`] is one twenty-fifth of
//! the long edge, which is far past the point where a face or a sign is legible.
//!
//! ## What this module is not
//!
//! It is not anonymisation. A blurred photograph of a wedding still shows a wedding,
//! and a blurred face at 40 px is not a face anybody could recognise but it is also not
//! nothing. The real privacy control is the consent gate in `aura-cloud` and the default,
//! which is that no image leaves the machine at all. This module is what makes the
//! *permitted* case narrow, not what makes it safe.

use crate::face::detect::NormBox;

/// Blur radius as a fraction of the image's long edge.
///
/// One twenty-fifth. On a 384 px thumbnail that is a 15 px radius applied twice, which
/// leaves large shapes and colour and destroys text and facial structure.
pub const BLUR_FRACTION: f32 = 0.04;

/// How far outside a face box the sharp region extends, as a fraction of the box.
///
/// A tenth. A face box is tight to the face, and a crop that ends exactly at the jawline
/// looks like a cut-out pasted onto a blur - which is both ugly and, more importantly,
/// draws the eye to precisely the region a reviewer wants to check was legitimate. The
/// margin makes the transition read as a photograph.
pub const KEEP_MARGIN: f32 = 0.10;

/// Blur everything except the given boxes.
///
/// The boxes are grown by [`KEEP_MARGIN`] and clamped to the frame. Everything outside
/// every box is replaced by the blurred image; inside, the original pixels survive.
#[must_use]
pub fn blur_background(rgb: &[u8], width: usize, height: usize, keep: &[NormBox]) -> Vec<u8> {
    let blurred = blur_all(rgb, width, height);
    if keep.is_empty() {
        return blurred;
    }

    let mut out = blurred;
    for region in keep {
        let grown = grow(region, KEEP_MARGIN);
        let left = (grown.x * width as f32).floor().max(0.0) as usize;
        let top = (grown.y * height as f32).floor().max(0.0) as usize;
        let right = ((grown.right() * width as f32).ceil() as usize).min(width);
        let bottom = ((grown.bottom() * height as f32).ceil() as usize).min(height);

        for row in top..bottom {
            for column in left..right {
                for channel in 0..3 {
                    let index = (row * width + column) * 3 + channel;
                    if let (Some(slot), Some(original)) =
                        (out.get_mut(index), rgb.get(index).copied())
                    {
                        *slot = original;
                    }
                }
            }
        }
    }
    out
}

/// Blur the whole image.
#[must_use]
pub fn blur_all(rgb: &[u8], width: usize, height: usize) -> Vec<u8> {
    if width == 0 || height == 0 || rgb.len() < width * height * 3 {
        return rgb.to_vec();
    }
    let radius = ((width.max(height) as f32 * BLUR_FRACTION).round() as usize).max(1);

    // Two passes. One box blur leaves visible banding at this radius; two do not, and
    // two is where the Gaussian approximation becomes good enough to stop.
    let once = box_blur(rgb, width, height, radius);
    box_blur(&once, width, height, radius)
}

/// Grow a box by a fraction of its own size, clamped to the frame.
fn grow(region: &NormBox, fraction: f32) -> NormBox {
    let dx = region.w * fraction;
    let dy = region.h * fraction;
    NormBox::from_corners(
        region.x - dx,
        region.y - dy,
        region.right() + dx,
        region.bottom() + dy,
    )
}

/// One separable box blur, horizontal then vertical.
///
/// Edges are handled by clamping the sample position rather than by shrinking the
/// window, so the border does not darken - a darkened border on a blurred thumbnail
/// looks like a vignette and would be mistaken for a photographic choice.
///
/// All index arithmetic is unsigned. A signed offset would read more naturally and
/// would also be a cast from `usize` on every sample; `saturating_sub` expresses the
/// same clamp-at-zero the sampler wants, with no cast and no chance of a wrap.
fn box_blur(rgb: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    let mut horizontal = vec![0u8; width * height * 3];
    let window = (radius * 2 + 1) as u32;

    for row in 0..height {
        for channel in 0..3 {
            // A running sum: each step adds one sample and drops one, so the cost is
            // independent of the radius.
            let sample = |column: usize| -> u32 {
                let clamped = column.min(width.saturating_sub(1));
                u32::from(
                    rgb.get((row * width + clamped) * 3 + channel)
                        .copied()
                        .unwrap_or(0),
                )
            };
            let mut sum: u32 = 0;
            for offset in 0..=radius * 2 {
                sum += sample(offset.saturating_sub(radius));
            }
            for column in 0..width {
                if let Some(slot) = horizontal.get_mut((row * width + column) * 3 + channel) {
                    *slot = (sum / window) as u8;
                }
                sum = sum + sample(column + radius + 1) - sample(column.saturating_sub(radius));
            }
        }
    }

    let mut out = vec![0u8; width * height * 3];
    for column in 0..width {
        for channel in 0..3 {
            let sample = |row: usize| -> u32 {
                let clamped = row.min(height.saturating_sub(1));
                u32::from(
                    horizontal
                        .get((clamped * width + column) * 3 + channel)
                        .copied()
                        .unwrap_or(0),
                )
            };
            let mut sum: u32 = 0;
            for offset in 0..=radius * 2 {
                sum += sample(offset.saturating_sub(radius));
            }
            for row in 0..height {
                if let Some(slot) = out.get_mut((row * width + column) * 3 + channel) {
                    *slot = (sum / window) as u8;
                }
                sum = sum + sample(row + radius + 1) - sample(row.saturating_sub(radius));
            }
        }
    }
    out
}

/// How much detail survived a blur, as a fraction of the original's edge energy.
///
/// The test that makes the privacy claim measurable rather than asserted. A blur that
/// left 40 % of the edge energy would still show a face; this returns the number so a
/// test can refuse it.
#[must_use]
pub fn residual_detail(original: &[u8], blurred: &[u8], width: usize, height: usize) -> f32 {
    let energy = |buffer: &[u8]| -> f64 {
        let mut total = 0.0f64;
        for row in 1..height.saturating_sub(1) {
            for column in 1..width.saturating_sub(1) {
                let at = |r: usize, c: usize| -> f64 {
                    let index = (r * width + c) * 3;
                    let read =
                        |offset: usize| f64::from(buffer.get(index + offset).copied().unwrap_or(0));
                    (read(0) + read(1) + read(2)) / 3.0
                };
                let dx = at(row, column + 1) - at(row, column - 1);
                let dy = at(row + 1, column) - at(row - 1, column);
                total += (dx * dx + dy * dy).sqrt();
            }
        }
        total
    };
    let before = energy(original);
    if before <= f64::EPSILON {
        return 0.0;
    }
    ((energy(blurred) / before) as f32).clamp(0.0, 1.0)
}
