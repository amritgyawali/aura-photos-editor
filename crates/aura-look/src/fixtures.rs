//! Synthetic reference galleries with a known answer.
//!
//! Every gate in this phase is measured against photographs whose look was **chosen, applied to
//! an authored plate by an analytic transform, and read back through the real measurer**. That
//! proves the measurer, the lighting sort, the fold, the solver and the store. It is not
//! evidence that a photographer would recognise a page they admire in the result, and the exit
//! report says so.
//!
//! ## What the plate has to contain, and why each thing is there
//!
//! A plate that is one flat colour measures nothing. Phase 29 shipped three fixture defects of
//! exactly that shape and its lesson was that a fixture has to be something the product would
//! actually see. So every plate here carries:
//!
//! - **A full tonal range**, because five of the seven tone landmarks are quantiles and a plate
//!   spanning half the range would put `p01` and `p99` on the same pixel.
//! - **Near-neutral content**, because [`crate::measure::white_point_cct`]'s whole input is
//!   pixels that are plausibly meant to be grey, and a plate without any reports
//!   [`crate::measure::NO_EVIDENCE_CCT_K`] - which sorts every frame into
//!   `LightingBucket::Unknown` and empties every bucket in the phase.
//! - **Several hue bands with a real share each**, because a band below
//!   [`crate::solve::BAND_MIN_SHARE`] is not acted on, and a plate with one hue would leave
//!   seven of the eight band terms untested.
//! - **Texture**, because a plate of flat patches has a midtone spread that is a property of the
//!   patch values rather than of the photograph, and contrast is solved from that spread.

use aura_raw::codec::Rgb8;

/// A look, as the analytic transform a fixture applies.
///
/// Deliberately **not** a [`aura_core::contract::style::StyleDelta`]. A fixture that applied the
/// product's own delta through the product's own renderer could not distinguish a correct
/// solver from a solver and a renderer that are wrong in opposite directions. This is an
/// independent transform, written in this file, and what the gates assert is that measuring its
/// result recovers its direction and its rough size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyntheticLook {
    /// Multiplicative gain on linear light. Above one is brighter.
    pub gain: f32,
    /// Contrast about mid grey, as an exponent. Above one is more contrast.
    pub contrast: f32,
    /// Warmth: red gain up and blue gain down by this fraction.
    pub warmth: f32,
    /// Green-magenta: green gain by this fraction. Positive is greener.
    pub green: f32,
    /// Saturation multiplier about each pixel's own luminance.
    pub saturation: f32,
    /// A constant lift added after everything else, in 0-1 units. What "lifted blacks" is.
    pub lift: f32,
}

impl Default for SyntheticLook {
    fn default() -> Self {
        Self::neutral()
    }
}

impl SyntheticLook {
    /// The look that changes nothing.
    #[must_use]
    pub const fn neutral() -> Self {
        Self {
            gain: 1.0,
            contrast: 1.0,
            warmth: 0.0,
            green: 0.0,
            saturation: 1.0,
            lift: 0.0,
        }
    }

    /// The look a photographer would call "light and airy": brighter, lifted, gently warm, a
    /// little less contrast.
    #[must_use]
    pub const fn light_and_airy() -> Self {
        Self {
            gain: 1.25,
            contrast: 0.88,
            warmth: 0.04,
            green: 0.0,
            saturation: 0.92,
            lift: 0.045,
        }
    }

    /// The look a photographer would call "dark and moody": darker, more contrast, cooler,
    /// richer.
    #[must_use]
    pub const fn dark_and_moody() -> Self {
        Self {
            gain: 0.78,
            contrast: 1.18,
            warmth: -0.05,
            green: 0.0,
            saturation: 1.12,
            lift: 0.0,
        }
    }

    /// A warm film look: much warmer, slightly lifted, slightly desaturated.
    #[must_use]
    pub const fn warm_film() -> Self {
        Self {
            gain: 1.04,
            contrast: 0.95,
            warmth: 0.10,
            green: 0.015,
            saturation: 0.88,
            lift: 0.03,
        }
    }
}

/// A deterministic value in `0..1` from three integers.
///
/// A hash rather than a generator with state, so that a plate's pixel depends on its coordinates
/// and the seed and on nothing else - which is what makes generating the same plate twice, in
/// either order, produce the same bytes. `rand` is banned by `scripts/check-banned.sh` and this
/// is the reason the ban is right rather than merely a rule.
fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(y.wrapping_mul(0x85EB_CA6B))
        .wrapping_add(seed.wrapping_mul(0xC2B2_AE35));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    (h % 10_000) as f32 / 10_000.0
}

/// sRGB's encoding curve.
fn encode(linear: f32) -> u8 {
    let value = linear.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// sRGB's decoding curve.
fn decode(encoded: u8) -> f32 {
    let value = f32::from(encoded) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// One authored photograph: a tonal ramp, six colour patches, neutral ground and texture.
///
/// The seed moves the texture and shifts which patch sits where, so a gallery of these is a set
/// of different photographs with one look rather than one photograph repeated - which matters,
/// because a median over identical frames has a spread of zero and would make every bucket
/// coherent by construction.
#[must_use]
pub fn plate(width: u32, height: u32, seed: u32) -> Rgb8 {
    let mut samples = Vec::with_capacity((width * height * 3) as usize);

    for y in 0..height {
        for x in 0..width {
            let across = x as f32 / width.max(1) as f32;
            let down = y as f32 / height.max(1) as f32;

            // A vertical tonal ramp over the full range, so the quantiles have somewhere to sit.
            let base = 0.02 + 0.92 * down;
            // Texture, so the midtone spread is a property of the photograph.
            let grain = (noise(x, y, seed) - 0.5) * 0.06;
            let luma = (base + grain).clamp(0.0, 1.0);

            // Six colour patches across the middle third, and neutral everywhere else. The
            // neutral majority is what gives the white-point estimate something to read, and
            // the patches are what give the eight bands a share each.
            let patch = if down > 0.33 && down < 0.67 {
                Some(((across * 6.0) as u32 + seed) % 6)
            } else {
                None
            };

            let (r, g, b) = match patch {
                // Deliberately moderate rather than saturated: a fixture full of primaries
                // measures a gamut rather than a wedding, and every band reading would sit at
                // the top of its range where a difference cannot be seen.
                Some(0) => (luma * 1.35, luma * 0.80, luma * 0.72), // red
                Some(1) => (luma * 1.28, luma * 1.02, luma * 0.66), // orange / yellow
                Some(2) => (luma * 0.74, luma * 1.22, luma * 0.80), // green
                Some(3) => (luma * 0.70, luma * 1.10, luma * 1.24), // aqua
                Some(4) => (luma * 0.72, luma * 0.84, luma * 1.34), // blue
                Some(5) => (luma * 1.20, luma * 0.76, luma * 1.18), // magenta
                _ => (luma, luma, luma),
            };

            samples.push(encode(r));
            samples.push(encode(g));
            samples.push(encode(b));
        }
    }

    Rgb8 {
        width,
        height,
        data: samples,
    }
}

/// One plate with a synthetic look applied.
#[must_use]
pub fn apply(image: &Rgb8, look: SyntheticLook) -> Rgb8 {
    let mut samples = Vec::with_capacity(image.data.len());

    for triple in image.data.chunks_exact(3) {
        let [r, g, b] = match triple {
            [r, g, b] => [decode(*r), decode(*g), decode(*b)],
            _ => continue,
        };

        // Exposure, then contrast about mid grey in linear light.
        let shape = |value: f32| {
            let lit = (value * look.gain).max(0.0);
            let contrasted = if look.contrast > 0.0 {
                0.18 * (lit / 0.18).max(0.0).powf(look.contrast)
            } else {
                lit
            };
            contrasted + look.lift
        };
        let (mut r, mut g, mut b) = (shape(r), shape(g), shape(b));

        // Warmth and green, as channel gains.
        r *= 1.0 + look.warmth;
        b *= 1.0 - look.warmth;
        g *= 1.0 + look.green;

        // Saturation about the pixel's own luminance, so it changes colour and not brightness.
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        r = y + (r - y) * look.saturation;
        g = y + (g - y) * look.saturation;
        b = y + (b - y) * look.saturation;

        samples.push(encode(r));
        samples.push(encode(g));
        samples.push(encode(b));
    }

    Rgb8 {
        width: image.width,
        height: image.height,
        data: samples,
    }
}

/// A gallery: `count` different plates, all carrying one look.
#[must_use]
pub fn gallery(count: u32, look: SyntheticLook) -> Vec<Rgb8> {
    (0..count)
        .map(|seed| apply(&plate(192, 128, seed), look))
        .collect()
}

/// A gallery, already measured.
#[must_use]
pub fn readings(
    count: u32,
    look: SyntheticLook,
) -> Vec<aura_core::contract::look::ReferenceReading> {
    gallery(count, look)
        .iter()
        .enumerate()
        .map(|(index, image)| crate::measure::read(format!("fixture-{index:04}"), image))
        .collect()
}

/// The aggregate of a gallery.
#[must_use]
pub fn aggregate(count: u32, look: SyntheticLook) -> aura_core::contract::look::LookAggregate {
    let readings = readings(count, look);
    crate::aggregate::fold(&crate::aggregate::all(&readings))
}

/// Write a gallery to a folder, so the walk and the store can be exercised on real files.
///
/// PNG is not written - the encoder this crate reaches is `aura-export`'s and it is not a
/// dependency here - so the files are written as JPEG through the same writer the product
/// delivers with. That is the right choice anyway: a reference photograph from a page is a
/// JPEG, and a fixture that used a lossless format would skip the one lossy step every real
/// reference has been through.
///
/// # Errors
///
/// Whatever the filesystem or the encoder raised.
pub fn write_gallery(
    folder: &std::path::Path,
    count: u32,
    look: SyntheticLook,
    encode_jpeg: impl Fn(&Rgb8) -> aura_core::contract::error::AuraResult<Vec<u8>>,
) -> aura_core::contract::error::AuraResult<Vec<std::path::PathBuf>> {
    std::fs::create_dir_all(folder)
        .map_err(|error| aura_core::errors::io::from_io(&error, folder))?;
    let mut written = Vec::new();
    for (index, image) in gallery(count, look).iter().enumerate() {
        let path = folder.join(format!("reference-{index:04}.jpg"));
        let bytes = encode_jpeg(image)?;
        std::fs::write(&path, bytes)
            .map_err(|error| aura_core::errors::io::from_io(&error, &path))?;
        written.push(path);
    }
    Ok(written)
}
