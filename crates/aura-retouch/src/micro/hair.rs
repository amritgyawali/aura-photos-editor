//! Which thin structures beside the hair are strays over a quiet background.
//!
//! PHASE-21 section 6.1, and it is the module in this phase with the largest gap between what
//! sounds easy and what is safe:
//!
//! > Detect flyaways as thin high-contrast structures outside the hair alpha but connected to
//! > it; require a clean, low-detail background, otherwise skip.
//!
//! ## Three conditions, all of which must hold
//!
//! **Outside the hair mass.** A strand inside the hair is hair. The detector reads the hair
//! region's alpha and looks only where it is low - between [`OUTSIDE_MIN`] and [`OUTSIDE_MAX`],
//! which is the halo where a matte has already decided the pixel is mostly background.
//!
//! **Connected to the hair.** An isolated dark thread on a wall is not a flyaway, and treating
//! it as one is phase 24's job rather than this one's. Connection is measured as distance from
//! the nearest sample where the hair alpha is above [`INSIDE_MIN`], in units of the frame's
//! shorter side.
//!
//! **Over a quiet background.** This is the condition that does the real work.
//! [`background_detail`] measures the local structure of what is *behind* the candidate,
//! excluding the candidate itself, and a value above
//! [`aura_core::contract::micro::MAX_FLYAWAY_BACKGROUND_DETAIL`] refuses the candidate outright.
//! A flyaway against foliage, a chandelier or a crowd is indistinguishable from the foliage, the
//! chandelier and the crowd, and an attenuation there is an edit to the photograph rather than to
//! the hair.
//!
//! ## Reduce, never remove
//!
//! The operation this module produces attenuates the candidate's contrast against its own
//! surroundings by at most [`aura_core::contract::micro::MAX_FLYAWAY_STRENGTH`]. There is no
//! value of the strength at which a strand disappears, and there is no inpainting path here at
//! all - the renderer's flyaway operator is a contrast pull toward the local background and
//! cannot synthesise a pixel. That is what makes "no bald patches" a property of the operator
//! rather than of a threshold.
//!
//! ## Everything here is linear
//!
//! Invariant 8. Luminance is a weighted sum of scene-referred values and there is no transfer
//! function in this module.

use aura_core::contract::composition::Box2;
use aura_core::contract::micro::{MAX_FLYAWAY_AREA, MAX_FLYAWAY_BACKGROUND_DETAIL};

use crate::texture_guard::Frame;

/// Hair alpha at or above this is the hair mass, which is never edited.
///
/// Four fifths. A matte between this and [`OUTSIDE_MAX`] is the soft boundary phase 18's
/// trimap solved, and a strand living there is exactly the thing this module is looking for.
pub const INSIDE_MIN: f32 = 0.80;

/// Hair alpha below this is background with no hair in it at all.
pub const OUTSIDE_MIN: f32 = 0.02;

/// Hair alpha above this is too much hair to be a stray.
pub const OUTSIDE_MAX: f32 = 0.45;

/// How far from the hair mass a candidate may sit, as a fraction of the frame's shorter side.
///
/// Two per cent. A strand further out than that is either a hair somebody would notice was
/// missing or something that is not hair.
pub const MAX_DISTANCE: f32 = 0.02;

/// The smallest contrast against the local background that counts as a flyaway.
///
/// Below this it is not visible, and attenuating what nobody can see spends allowance on
/// nothing.
pub const MIN_CONTRAST: f32 = 0.035;

/// How wide the neighbourhood a candidate is measured against is, in pixels at proxy scale.
pub const NEIGHBOURHOOD: usize = 7;

/// The largest a single candidate's box may be, as a fraction of the frame.
///
/// A tenth of the whole-frame cap. One flyaway is a strand; a box a tenth of the frame across
/// containing "flyaways" is a hairstyle.
pub const MAX_SINGLE_AREA: f32 = MAX_FLYAWAY_AREA / 10.0;

/// One candidate stray.
#[derive(Debug, Clone, PartialEq)]
pub struct Flyaway {
    /// Where it is, normalised to the frame.
    pub region: Box2,
    /// How strongly it stands out against its own background, `0..1`.
    pub contrast: f32,
    /// How structured the background behind it is, `0..1`.
    ///
    /// The number [`MAX_FLYAWAY_BACKGROUND_DETAIL`] refuses on, kept on the candidate so that a
    /// refusal can be explained with the measurement rather than with the word "busy".
    pub background_detail: f32,
    /// How far from the hair mass it sits, as a fraction of the frame's shorter side.
    pub distance: f32,
    /// True when the background behind it is too structured to work against.
    pub background_busy: bool,
}

impl Flyaway {
    /// True when this candidate may be acted on at all.
    #[must_use]
    pub fn is_actionable(&self) -> bool {
        !self.background_busy
            && self.contrast >= MIN_CONTRAST
            && self.distance <= MAX_DISTANCE
            && self.region.w * self.region.h <= MAX_SINGLE_AREA + 1e-9
    }
}

/// Find the stray strands around one hair region.
///
/// `hair` is the per-pixel hair coverage from phase 18, `frame.width * frame.height` long. An
/// empty or absent region returns no candidates - there is no geometric fallback here, for the
/// reason phase 19 gives and this phase inherits twice.
///
/// The result is ordered by contrast, strongest first, then by position, so that a caller
/// applying an area cap takes the strands a photographer would notice first and the ordering is
/// the same on every machine. Invariant 4.
#[must_use]
pub fn detect(frame: &Frame, hair: &[f32]) -> Vec<Flyaway> {
    let (width, height) = (frame.width, frame.height);
    if width == 0 || height == 0 || hair.len() < width * height {
        return Vec::new();
    }

    let luminance = luma_plane(frame);
    // The local background estimate is computed from the background, with the hair mass taken
    // out of it first. Blurring across the edge of the hair drags the estimate down for every
    // sample in the halo, and the halo is precisely where this detector looks - so the boundary
    // of every well-matted head reads as a column of flyaways, at whatever contrast the hair
    // happens to have against the room. See [`background_estimate`].
    let local = background_estimate(&luminance, hair, width, height);
    let short = width.min(height) as f32;
    let reach = ((MAX_DISTANCE * short).ceil() as usize).max(1);

    // The halo: samples the matte has already called mostly-background but which sit within
    // reach of the hair mass. Everything else is either hair or nowhere near it.
    let mut halo = vec![false; width * height];
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let alpha = hair.get(index).copied().unwrap_or(0.0);
            if !(OUTSIDE_MIN..=OUTSIDE_MAX).contains(&alpha) {
                continue;
            }
            if let Some(slot) = halo.get_mut(index) {
                *slot = true;
            }
        }
    }

    let mut seen = vec![false; width * height];
    let mut out: Vec<Flyaway> = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if !halo.get(index).copied().unwrap_or(false)
                || seen.get(index).copied().unwrap_or(false)
            {
                continue;
            }
            let value = luminance.get(index).copied().unwrap_or(0.0);
            let base = local.get(index).copied().unwrap_or(value);
            if (value - base).abs() < MIN_CONTRAST {
                if let Some(slot) = seen.get_mut(index) {
                    *slot = true;
                }
                continue;
            }
            // A strand is a *signed* structure: it is either darker than the background or
            // brighter than it, and a component that mixes the two is a texture rather than a
            // hair. The sign is fixed at the seed and grown with.
            let sign = if value > base { 1.0f32 } else { -1.0f32 };
            let component = grow(
                index, sign, &luminance, &local, &halo, &mut seen, width, height,
            );
            if component.count == 0 {
                continue;
            }

            let region = Box2 {
                x: component.x0 as f32 / width as f32,
                y: component.y0 as f32 / height as f32,
                w: (component.x1 - component.x0 + 1) as f32 / width as f32,
                h: (component.y1 - component.y0 + 1) as f32 / height as f32,
            };
            let distance = nearest_hair(&component, hair, width, height, reach) / short;
            let detail = background_detail(
                &luminance,
                &local,
                hair,
                &component,
                width,
                height,
                reach.max(NEIGHBOURHOOD),
            );

            out.push(Flyaway {
                region,
                contrast: component.contrast.clamp(0.0, 1.0),
                background_detail: detail,
                distance,
                background_busy: detail > MAX_FLYAWAY_BACKGROUND_DETAIL,
            });
        }
    }

    // Strongest first, then by position. `total_cmp` rather than `partial_cmp` because a NaN
    // contrast would otherwise make the ordering machine-dependent, and the order decides which
    // strands survive the area cap.
    out.sort_by(|a, b| {
        b.contrast
            .total_cmp(&a.contrast)
            .then(a.region.y.total_cmp(&b.region.y))
            .then(a.region.x.total_cmp(&b.region.x))
    });
    out
}

/// How much of the frame a list of candidates covers.
#[must_use]
pub fn total_area(candidates: &[Flyaway]) -> f32 {
    candidates
        .iter()
        .map(|c| c.region.w * c.region.h)
        .sum::<f32>()
}

/// One connected run of samples all departing from the local background in the same direction.
#[derive(Debug, Clone, Copy)]
struct Component {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    count: usize,
    contrast: f32,
}

/// Grow a component from one seed, eight-connected, staying inside the halo and the sign.
///
/// Iterative rather than recursive: a strand across a 2048 px proxy can be two thousand samples
/// long, and a recursive flood fill on one is a stack overflow in the middle of a wedding.
#[allow(clippy::too_many_arguments)]
fn grow(
    seed: usize,
    sign: f32,
    luminance: &[f32],
    local: &[f32],
    halo: &[bool],
    seen: &mut [bool],
    width: usize,
    height: usize,
) -> Component {
    let mut stack = vec![seed];
    if let Some(slot) = seen.get_mut(seed) {
        *slot = true;
    }
    let (mut x0, mut y0) = (seed % width, seed / width);
    let (mut x1, mut y1) = (x0, y0);
    let mut count = 0usize;
    let mut peak = 0.0f32;

    // A safety bound on the work rather than the guarantee: a pathological matte must not turn
    // one seed into a whole-frame fill. The *guarantee* is the bounding-box area checked in
    // `Flyaway::is_actionable`, and the two are deliberately different quantities - a strand is
    // thin and long, so its box is far larger than the number of samples in it, and capping the
    // sample count by an area fraction would truncate every real strand halfway down.
    let limit = ((width * height) as f32 * MAX_SINGLE_AREA * 8.0).ceil() as usize + 256;

    while let Some(index) = stack.pop() {
        let value = luminance.get(index).copied().unwrap_or(0.0);
        let base = local.get(index).copied().unwrap_or(value);
        let departure = (value - base) * sign;
        if departure < MIN_CONTRAST {
            continue;
        }
        count += 1;
        peak = peak.max(departure);
        let (x, y) = (index % width, index / width);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
        if count >= limit {
            break;
        }

        // The eight neighbours, in unsigned coordinates. The window is clamped to the frame
        // rather than tested for negatives after a signed cast: a strand against the top edge is
        // an ordinary strand.
        let left = x.saturating_sub(1);
        let right = (x + 1).min(width.saturating_sub(1));
        let top = y.saturating_sub(1);
        let bottom = (y + 1).min(height.saturating_sub(1));
        for ny in top..=bottom {
            for nx in left..=right {
                if nx == x && ny == y {
                    continue;
                }
                let neighbour = ny * width + nx;
                if !halo.get(neighbour).copied().unwrap_or(false) {
                    continue;
                }
                if seen.get(neighbour).copied().unwrap_or(true) {
                    continue;
                }
                if let Some(slot) = seen.get_mut(neighbour) {
                    *slot = true;
                }
                stack.push(neighbour);
            }
        }
    }

    Component {
        x0,
        y0,
        x1,
        y1,
        count,
        contrast: peak,
    }
}

/// Distance in pixels from a component's own box to the nearest sample that is really hair.
///
/// Searched outward from the component's own box rather than over the whole frame: a full
/// distance transform is the general answer and this only ever needs to know whether the hair is
/// within `reach`.
fn nearest_hair(
    component: &Component,
    hair: &[f32],
    width: usize,
    height: usize,
    reach: usize,
) -> f32 {
    let mut best = f32::INFINITY;

    let x0 = component.x0.saturating_sub(reach);
    let y0 = component.y0.saturating_sub(reach);
    let x1 = (component.x1 + reach).min(width.saturating_sub(1));
    let y1 = (component.y1 + reach).min(height.saturating_sub(1));

    for y in y0..=y1 {
        for x in x0..=x1 {
            if hair.get(y * width + x).copied().unwrap_or(0.0) < INSIDE_MIN {
                continue;
            }
            // To the nearest point of the component's own box, not to its centre. A strand is
            // long and thin: measuring from the middle of one reports it as detached from hair
            // its top is touching.
            let dx = (x as f32 - (x as f32).clamp(component.x0 as f32, component.x1 as f32)).abs();
            let dy = (y as f32 - (y as f32).clamp(component.y0 as f32, component.y1 as f32)).abs();
            best = best.min((dx * dx + dy * dy).sqrt());
        }
    }
    if best.is_finite() {
        best
    } else {
        // No hair mass anywhere near it. Reported as just past the cap rather than as infinity,
        // so the number stays printable in a reason.
        (reach + 1) as f32
    }
}

/// How structured the background around a candidate is, `0..1`.
///
/// The mean absolute local residual over the ring around the component, **excluding the component
/// itself and excluding the hair**. Both exclusions matter and both were found by a test.
///
/// Measuring inside the component would score the strand, and a strand is high-frequency by
/// definition, so every candidate would look like it sat on a busy background and nothing would
/// ever be calmed.
///
/// Measuring over the hair would score *the edge of the hair mass*, which is the largest step
/// anywhere near a flyaway - the whole reason the strand is visible. A ring that includes it
/// reports a busy background on every well-mattted head in the wedding, which is a refusal that
/// looks like caution and is actually a bug.
fn background_detail(
    luminance: &[f32],
    local: &[f32],
    hair: &[f32],
    component: &Component,
    width: usize,
    height: usize,
    reach: usize,
) -> f32 {
    let x0 = component.x0.saturating_sub(reach);
    let y0 = component.y0.saturating_sub(reach);
    let x1 = (component.x1 + reach).min(width.saturating_sub(1));
    let y1 = (component.y1 + reach).min(height.saturating_sub(1));

    let mut total = 0.0f64;
    let mut count = 0u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let inside =
                x >= component.x0 && x <= component.x1 && y >= component.y0 && y <= component.y1;
            if inside {
                continue;
            }
            let index = y * width + x;
            if hair.get(index).copied().unwrap_or(0.0) > OUTSIDE_MIN {
                continue;
            }
            let value = luminance.get(index).copied().unwrap_or(0.0);
            let base = local.get(index).copied().unwrap_or(value);
            total += f64::from((value - base).abs());
            count += 1;
        }
    }
    if count == 0 {
        // Nothing measurable around it. Refused rather than permitted: an unmeasured background
        // is not a quiet one.
        return 1.0;
    }
    ((total / f64::from(count)) as f32 * 8.0).clamp(0.0, 1.0)
}

/// Rec.709 luminance of every pixel, linear.
fn luma_plane(frame: &Frame) -> Vec<f32> {
    let mut out = Vec::with_capacity(frame.width * frame.height);
    for index in 0..frame.width * frame.height {
        let slot = index * 3;
        let value = frame.rgb.get(slot..slot + 3).map_or(0.0, |rgb| {
            0.2126 * rgb.first().copied().unwrap_or(0.0)
                + 0.7152 * rgb.get(1).copied().unwrap_or(0.0)
                + 0.0722 * rgb.get(2).copied().unwrap_or(0.0)
        });
        out.push(value);
    }
    out
}

/// The local background, estimated without letting the hair itself into it.
///
/// Samples inside the hair mass are replaced by the median of the true background before the
/// blur, so the estimate describes *what is behind the head* everywhere - including in the halo,
/// where half of a blur window would otherwise be hair.
///
/// This is the same correction three modules in this phase needed and it is worth naming once:
/// **a local estimate must be computed from the region it describes.** [`super::clothing`] needs
/// it because a lint drags its own neighbourhood, and [`super::eyes`] needs it because a sclera
/// drags an iris. Each was found by a test that expected an obvious detection and got a refusal.
fn background_estimate(luminance: &[f32], hair: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut background: Vec<f32> = Vec::new();
    for index in 0..width * height {
        if hair.get(index).copied().unwrap_or(0.0) >= OUTSIDE_MIN {
            continue;
        }
        background.push(luminance.get(index).copied().unwrap_or(0.0));
    }
    if background.is_empty() {
        return box_blur(luminance, width, height, NEIGHBOURHOOD);
    }
    background.sort_by(f32::total_cmp);
    let median = background
        .get(background.len() / 2)
        .copied()
        .unwrap_or_default();

    let mut masked = luminance.to_vec();
    for index in 0..width * height {
        if hair.get(index).copied().unwrap_or(0.0) <= OUTSIDE_MAX {
            continue;
        }
        if let Some(slot) = masked.get_mut(index) {
            *slot = median;
        }
    }
    box_blur(&masked, width, height, NEIGHBOURHOOD)
}

/// A separable box blur, used as the local background estimate.
///
/// Shared with [`super::clothing`], which needs exactly the same local estimate over a garment.
/// One implementation rather than two, for the reason phase 10 moved the face warp and phase 16
/// moved the curve interpolation: two local backgrounds are two answers to "what is this pixel
/// sitting on".
pub(super) fn box_blur(values: &[f32], width: usize, height: usize, side: usize) -> Vec<f32> {
    let radius = side / 2;
    if radius == 0 || width == 0 || height == 0 {
        return values.to_vec();
    }
    let mut horizontal = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut total = 0.0f32;
            let mut count = 0u32;
            for dx in 0..=radius * 2 {
                // `x + dx - radius`, kept unsigned. A sample off either edge is skipped rather
                // than clamped, which is what makes `count` the number of real samples and the
                // mean an average of the frame rather than of the frame plus its own border.
                if x + dx < radius {
                    continue;
                }
                let sx = x + dx - radius;
                if sx >= width {
                    continue;
                }
                total += values.get(y * width + sx).copied().unwrap_or(0.0);
                count += 1;
            }
            if let Some(slot) = horizontal.get_mut(y * width + x) {
                *slot = if count == 0 {
                    0.0
                } else {
                    total / count as f32
                };
            }
        }
    }
    let mut out = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut total = 0.0f32;
            let mut count = 0u32;
            for dy in 0..=radius * 2 {
                // `y + dy - radius`, kept unsigned, as the horizontal half is.
                if y + dy < radius {
                    continue;
                }
                let sy = y + dy - radius;
                if sy >= height {
                    continue;
                }
                total += horizontal.get(sy * width + x).copied().unwrap_or(0.0);
                count += 1;
            }
            if let Some(slot) = out.get_mut(y * width + x) {
                *slot = if count == 0 {
                    0.0
                } else {
                    total / count as f32
                };
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame with a hair mass on the left, a quiet background, and one bright strand in the
    /// halo beside it.
    ///
    /// 256 px a side rather than 64, because `MAX_SINGLE_AREA` is a fraction of the frame and a
    /// strand long enough to be a strand is a large fraction of a small frame. At proxy scale a
    /// flyaway is a few pixels wide and a couple of hundred long; a 64 px fixture cannot express
    /// that ratio and would be testing the fixture rather than the detector.
    fn strand_over_quiet_background() -> (Frame, Vec<f32>) {
        let (width, height) = (256usize, 256usize);
        let mut rgb = vec![0.0f32; width * height * 3];
        let mut hair = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let index = y * width + x;
                let value = if x < 80 { 0.05 } else { 0.42 };
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut(index * 3 + channel) {
                        *slot = value;
                    }
                }
                if let Some(slot) = hair.get_mut(index) {
                    *slot = if x < 80 {
                        1.0
                    } else if x < 87 {
                        0.30
                    } else {
                        0.0
                    };
                }
            }
        }
        // One bright strand across the halo: one pixel wide, twenty tall. Its bounding box is
        // 20/65536 of the frame, comfortably inside `MAX_SINGLE_AREA`.
        for y in 100..120 {
            let index = y * width + 83;
            for channel in 0..3 {
                if let Some(slot) = rgb.get_mut(index * 3 + channel) {
                    *slot = 0.80;
                }
            }
        }
        (Frame { rgb, width, height }, hair)
    }

    #[test]
    fn a_strand_in_the_halo_over_a_quiet_background_is_found_and_is_actionable() {
        let (frame, hair) = strand_over_quiet_background();
        let found = detect(&frame, &hair);
        assert!(!found.is_empty(), "no candidate was found at all");
        let strongest = found.first().expect("a candidate");
        assert!(
            strongest.is_actionable(),
            "the strand was refused: {strongest:?}"
        );
        assert!(strongest.contrast >= MIN_CONTRAST);
        assert!(strongest.distance <= MAX_DISTANCE);
    }

    #[test]
    fn the_same_strand_over_a_busy_background_is_refused() {
        let (mut frame, hair) = strand_over_quiet_background();
        // Speckle the background. Deterministic rather than random - invariant 4 - and strong
        // enough that the local residual is unambiguous.
        for y in 0..frame.height {
            for x in 87..frame.width {
                if (x + y) % 3 != 0 {
                    continue;
                }
                let index = y * frame.width + x;
                for channel in 0..3 {
                    if let Some(slot) = frame.rgb.get_mut(index * 3 + channel) {
                        *slot = 0.90;
                    }
                }
            }
        }
        let found = detect(&frame, &hair);
        assert!(
            found.iter().all(|c| !c.is_actionable()),
            "a candidate survived a busy background: {found:?}"
        );
        assert!(
            found.iter().any(|c| c.background_busy),
            "no candidate recorded the background as busy"
        );
    }

    #[test]
    fn nothing_inside_the_hair_mass_is_ever_a_candidate() {
        let (mut frame, hair) = strand_over_quiet_background();
        // A bright strand at x = 40, which is deep inside the hair mass.
        for y in 100..120 {
            let index = y * frame.width + 40;
            for channel in 0..3 {
                if let Some(slot) = frame.rgb.get_mut(index * 3 + channel) {
                    *slot = 0.80;
                }
            }
        }
        for candidate in detect(&frame, &hair) {
            assert!(
                candidate.region.x * frame.width as f32 >= 80.0,
                "a candidate was found inside the hair mass: {candidate:?}"
            );
        }
    }

    #[test]
    fn an_absent_region_produces_no_candidates() {
        let (frame, _) = strand_over_quiet_background();
        assert!(detect(&frame, &[]).is_empty());
    }

    #[test]
    fn detection_is_deterministic() {
        let (frame, hair) = strand_over_quiet_background();
        assert_eq!(detect(&frame, &hair), detect(&frame, &hair));
    }
}
