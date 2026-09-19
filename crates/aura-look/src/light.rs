//! Which of ten lights a reference photograph was made in, from its pixels alone.
//!
//! ## Why this is a weaker measurement than phase 15's, and says so
//!
//! Phase 15 asks what colour the light in the room was. It has the RAW, it has the wedding's
//! own known neutrals accumulated across hundreds of frames, it has every person's own skin
//! locus, and it generates four competing hypotheses and scores them against each other.
//!
//! This has one finished JPEG somebody else made, and the only thing it can read off it is what
//! the photograph *renders* at - which is the light and the edit together, permanently. A
//! photographer who warms every frame by 400 K has shifted every bucket boundary in this file by
//! 400 K, and nothing here can tell that from a venue with warmer bulbs.
//!
//! That is survivable because of what the bucket is *for*. It is not a correction and nothing is
//! white-balanced against it. It is an axis to group reference photographs along, so that "how
//! this page treats candlelight" and "how this page treats open shade" are two answers rather
//! than one average - and for that purpose, sorting by rendered appearance is arguably the more
//! correct thing to do anyway, because the photographer's treatment is part of what is being
//! grouped.
//!
//! **[`LightingBucket::Flash`] is never returned.** Flash is an EXIF fact rather than a visible
//! one, a reference JPEG's EXIF is whatever the platform left on it, and a frame lit by a
//! bare strobe and a frame lit by an overcast sky render within a few hundred kelvin of each
//! other. Guessing would put reference photographs in a bucket the photographer's own frames -
//! which *do* have EXIF, through phase 15 - would rarely land in, and the two sides of the
//! comparison would stop being about the same thing.

use aura_core::contract::look::ReferenceReading;
use aura_core::contract::style::LightingBucket;

use crate::measure::NO_EVIDENCE_CCT_K;

/// Below this rendered colour temperature a frame is candlelight.
pub const CANDLE_BELOW_K: f32 = 2_400.0;

/// Below this, and above [`CANDLE_BELOW_K`], a frame is tungsten.
pub const TUNGSTEN_BELOW_K: f32 = 3_600.0;

/// Below this, and above [`TUNGSTEN_BELOW_K`], a frame is golden hour.
///
/// [`LightingBucket::GOLDEN_BELOW_K`], phase 17's own boundary, used here rather than a second
/// one. Phase 17 draws it at 4,500 K as a *style* boundary - where a photographer stops calling
/// a frame "outdoors" and starts calling it "golden" - and a second number here would mean a
/// reference frame and one of the photographer's own frames at 4,400 K could land in different
/// buckets, which is the one thing this axis must never do.
pub const GOLDEN_BELOW_K: f32 = LightingBucket::GOLDEN_BELOW_K;

/// Below this, and above [`GOLDEN_BELOW_K`], a frame is daylight.
pub const DAYLIGHT_BELOW_K: f32 = 6_200.0;

/// Below this, and above [`DAYLIGHT_BELOW_K`], a frame is overcast. Above it, open shade.
pub const OVERCAST_BELOW_K: f32 = 7_600.0;

/// Above this 90th-percentile chroma a frame is a candidate for stage light.
///
/// Stage light is the one bucket that is not a temperature, because a dance-floor wash is not
/// *on* the Planckian locus at all - it is a saturated magenta or blue that a correlated colour
/// temperature describes about as well as it describes a traffic light. What identifies it is
/// that the frame is extremely saturated and that the saturation is concentrated in one or two
/// bands rather than spread across the eight.
pub const STAGE_CHROMA: f32 = 0.34;

/// Above this share in the three cool bands a saturated frame is stage light rather than a
/// saturated daylight frame.
///
/// Half. A garden in summer is saturated across green, yellow and aqua; an uplit dance floor is
/// two thirds one colour. The three bands are blue, purple and magenta because every stage wash
/// a wedding meets is one of them - a saturated *warm* frame at this chroma is a sunset, which
/// is golden hour and is decided by its temperature.
pub const STAGE_CONCENTRATION: f32 = 0.5;

/// Above this a frame's green cast is a gas-discharge tube or a cheap LED rather than daylight.
///
/// Negative `a*` is green. Three units, measured on the midtones, where a fluorescent tube's
/// spike shows and where a tungsten-to-daylight mixture does not.
pub const ARTIFICIAL_GREEN: f32 = -3.0;

/// Which light, and how sure.
///
/// The confidence is the *distance from the nearest boundary*, normalised, multiplied by whether
/// there was neutral content to measure a temperature from at all. It exists so that
/// `aggregate::fold` can weight a frame that sits solidly inside a bucket above one that sits a
/// hundred kelvin from its edge, rather than treating a hard assignment as a fact. Invariant 2.
#[must_use]
pub fn bucket(reading: &ReferenceReading) -> (LightingBucket, f32) {
    // No neutral content, no temperature, no claim. A frame of nothing but a red sari renders
    // at whatever McCamy makes of a red sari, and the honest answer is that this build does not
    // know what light it was made in.
    if (reading.rendered_cct_k - NO_EVIDENCE_CCT_K).abs() < 0.5 {
        return (LightingBucket::Unknown, 0.0);
    }

    // Stage first, because it is the one bucket that a temperature would answer wrongly rather
    // than imprecisely.
    if reading.chroma_p90 >= STAGE_CHROMA {
        let cool: f32 = reading
            .bands
            .iter()
            .skip(5)
            .map(|band| band.share)
            .sum::<f32>();
        if cool >= STAGE_CONCENTRATION {
            let over = ((reading.chroma_p90 - STAGE_CHROMA) / STAGE_CHROMA).clamp(0.0, 1.0);
            return (LightingBucket::Stage, (0.55 + 0.45 * over).clamp(0.0, 1.0));
        }
    }

    let cct = reading.rendered_cct_k;
    let (bucket, lower, upper) = if cct < CANDLE_BELOW_K {
        (LightingBucket::Candle, 1_500.0, CANDLE_BELOW_K)
    } else if cct < TUNGSTEN_BELOW_K {
        (LightingBucket::Tungsten, CANDLE_BELOW_K, TUNGSTEN_BELOW_K)
    } else if cct < GOLDEN_BELOW_K {
        (LightingBucket::GoldenHour, TUNGSTEN_BELOW_K, GOLDEN_BELOW_K)
    } else if cct < DAYLIGHT_BELOW_K {
        (LightingBucket::Daylight, GOLDEN_BELOW_K, DAYLIGHT_BELOW_K)
    } else if cct < OVERCAST_BELOW_K {
        (LightingBucket::Overcast, DAYLIGHT_BELOW_K, OVERCAST_BELOW_K)
    } else {
        (LightingBucket::Shade, OVERCAST_BELOW_K, 20_000.0)
    };

    // A green midtone cast inside the two indoor-ish bands is a tube rather than a room.
    // Checked after the temperature rather than before it, because a green cast on a frame at
    // 2,600 K is a tungsten room with one fluorescent in the corner, and the room is what the
    // photographer graded for.
    let bucket = if reading.mid.a <= ARTIFICIAL_GREEN
        && matches!(bucket, LightingBucket::GoldenHour | LightingBucket::Daylight)
    {
        LightingBucket::Artificial
    } else {
        bucket
    };

    let span = (upper - lower).max(1.0);
    let from_edge = (cct - lower).min(upper - cct).max(0.0) / (span * 0.5);
    let confidence = (0.45 + 0.55 * from_edge.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    (bucket, confidence)
}
