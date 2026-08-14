//! Context features in, attribute bits out.
//!
//! Two halves of the same idea, in one module because they share the frame's
//! measurements.
//!
//! **Going in** ([`ContextFeatures`]): section 6.1 asks for "hour-of-day offset from
//! first frame, flash flag, ISO bucket, face count, dominant-identity presence,
//! indoor/outdoor prior from luma and colour temperature" to be concatenated to the
//! embedding. Sixteen numbers, and every one of them is something the catalog already
//! knows - no pixel is opened here.
//!
//! **Coming out** ([`decode_attributes`]): the fourteen sigmoid outputs become
//! [`AttrFlags`], and four of them are then **corrected from measurements the model
//! cannot see**. That correction is the part worth reading.
//!
//! ## Why four attributes are decided rather than predicted
//!
//! `flash`, `night`, `tungsten` and `indoor` are all recorded, exactly, in EXIF or in
//! the phase 05 luminance statistics. A model asked to predict the flash flag from a
//! 512-d perceptual vector will be right most of the time; the camera is right every
//! time. So the head's opinion on those four is used only where the measurement is
//! absent, and where it is present the measurement wins and
//! [`AttributeReport::corrected`] counts it.
//!
//! This is not a workaround for a weak model. It is the same rule phase 06 applies to
//! face pose - "a geometric estimate from points the detector already produced is free,
//! deterministic, and does not pretend to a precision a placeholder model could not
//! deliver" - and it stays correct when the model is trained. A trained head will still
//! be wrong about flash on a frame lit by a window at 1/200th, and the EXIF will not.
//!
//! The other ten - `mixed_light`, `backlit`, `crowd`, `stage`, `vehicle`, `food`,
//! `decor`, `kids`, `pets`, `rain` - are genuinely visual and come from the head alone.

use aura_core::contract::scene::AttrFlags;
use aura_index::contract::index::LumaStats;
use aura_infer::onnx::fixtures::SCENE_CONTEXT_FEATURES;

/// Above this sigmoid output an attribute bit is set.
///
/// 0.5 for the ten visual attributes. Not tuned, and deliberately: a threshold tuned
/// against a placeholder head would be a number about a random projection, and a
/// trained head is calibrated by its own loss rather than by a constant here. When the
/// first trained model lands this becomes a per-attribute vector, which is what
/// `ml/models/scene/eval_scene.py --calibrate` produces.
pub const ATTRIBUTE_THRESHOLD: f32 = 0.5;

/// Below this median luminance a frame is treated as shot after dark.
///
/// The phase 05 luminance statistics are scene-referred and normalised to `0..1`, so
/// this is a statement about the rendered frame rather than about the scene: a night
/// reception exposed correctly is not dark, and this is not trying to detect one. It is
/// the *unlit* case - the outdoor exit, the dance floor between flashes - where p50 is
/// genuinely low and every downstream noise tolerance should widen.
pub const NIGHT_MEDIAN_BELOW: f32 = 0.18;

/// Above this ISO a frame was almost certainly shot indoors or after dark.
///
/// A prior, not a decision. It moves the `indoor` bit only when the head is
/// undecided - between [`INDOOR_UNSURE`] and its complement - because a photographer
/// shooting a backlit outdoor portrait at ISO 3200 exists and should not be relabelled.
pub const HIGH_ISO: u32 = 3200;

/// How far from 0.5 the head's `indoor` output must be to resist the ISO prior.
pub const INDOOR_UNSURE: f32 = 0.15;

/// Colour temperature below which the light is tungsten-dominant, in kelvin.
pub const TUNGSTEN_BELOW_K: u32 = 3400;

/// Everything the catalog already knows about one frame.
///
/// Every field is optional because every one of them can genuinely be missing: a card
/// with no EXIF, a frame that has not been scanned for faces, a project with no clock
/// alignment. A missing field substitutes a documented neutral rather than a zero -
/// see [`ContextFeatures::to_vector`] - because zero is a *measurement* for several of
/// these and "unknown" is not.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ContextFeatures {
    /// Milliseconds from the wedding's first frame to this one.
    pub offset_ms: Option<i64>,
    /// True when the flash fired, from EXIF.
    pub flash: Option<bool>,
    /// ISO, from EXIF.
    pub iso: Option<u32>,
    /// Colour temperature in kelvin, from EXIF when the camera recorded one.
    pub kelvin: Option<u32>,
    /// Faces the phase 06 pass found in this frame.
    pub faces: Option<u32>,
    /// True when at least one of the couple is in the frame.
    ///
    /// From `PeopleService`, never recomputed here. Phase 06's rule, inherited: no
    /// phase keeps its own idea of who the couple are.
    pub has_primary: Option<bool>,
    /// The phase 05 luminance statistics for this frame.
    pub luma: Option<LumaStats>,
}

impl ContextFeatures {
    /// The sixteen numbers that ride alongside the embedding.
    ///
    /// The order is a contract with the trained head, exactly as `SceneId::ALL`'s order
    /// is a contract with its output. Reordering these silently retrains nothing and
    /// breaks everything, so the slots are numbered in the comments and the numbering
    /// is repeated in `ml/models/scene/dataset.py`.
    ///
    /// Every value is in `0..1` except the two cyclic ones, which are in `-1..1`. That
    /// is not for the model's benefit - a linear layer does not care - but so that a
    /// human reading a dumped feature row can tell at a glance which slot is wrong.
    #[must_use]
    pub fn to_vector(&self) -> [f32; SCENE_CONTEXT_FEATURES] {
        let mut out = [0.0f32; SCENE_CONTEXT_FEATURES];

        // 0, 1: the hour of the wedding, as a sine and a cosine over 24 hours.
        //
        // Two slots and not one, so that a frame at 23:50 and a frame at 00:10 are
        // twenty minutes apart rather than a day. A wedding that runs past midnight is
        // the common case for three of the five traditions in the taxonomy, and a
        // linear feature would put the vidaai at the opposite end of the day from the
        // saptapadi that preceded it by half an hour.
        //
        // Missing time is (0, 0) - the centre of the circle - which is the one point
        // that is not any hour. A neutral that cannot be confused with a measurement.
        if let Some(offset) = self.offset_ms {
            let hours = offset as f64 / 3_600_000.0;
            let angle = hours * std::f64::consts::TAU / 24.0;
            out[0] = angle.sin() as f32;
            out[1] = angle.cos() as f32;
        }

        // 2: how far into the day, saturating at 24 hours. A three-day Nepali wedding
        // saturates, which is correct: "very late in the day" is the signal, and the
        // difference between hour 26 and hour 60 is a different day rather than a
        // later one.
        if let Some(offset) = self.offset_ms {
            out[2] = ((offset as f64 / 86_400_000.0).clamp(0.0, 1.0)) as f32;
        }

        // 3: flash. 0.5 when unknown, which is the honest midpoint between the two
        // states rather than "no flash".
        out[3] = self
            .flash
            .map_or(0.5, |fired| if fired { 1.0 } else { 0.0 });

        // 4..8: the ISO bucket, one-hot over base / 800 / 3200 / above.
        //
        // Buckets and not a logarithm, because the boundaries are where photographic
        // behaviour changes rather than where the arithmetic is smooth: below 800 is
        // daylight, 800 to 3200 is an indoor ceremony, above 3200 is a dark reception.
        // All four zero when the ISO is unknown.
        if let Some(iso) = self.iso {
            let bucket = match iso {
                0..=799 => 4,
                800..=3199 => 5,
                3200..=12799 => 6,
                _ => 7,
            };
            if let Some(slot) = out.get_mut(bucket) {
                *slot = 1.0;
            }
        }

        // 8: face count, saturating at twelve. Twelve rather than sixty: the
        // information is "nobody / one / a couple / a group / a crowd", and every
        // wedding has frames with forty faces in which the fortieth adds nothing.
        if let Some(faces) = self.faces {
            out[8] = (f32::from(u16::try_from(faces).unwrap_or(u16::MAX)) / 12.0).clamp(0.0, 1.0);
        }

        // 9: is one of the couple here. 0.5 when the face pass has not run - which is
        // the ordinary case in a project that has embeddings and no faces yet, and is
        // why this is not simply 0.
        out[9] = self
            .has_primary
            .map_or(0.5, |present| if present { 1.0 } else { 0.0 });

        // 10..14: the luminance shape. Median, first and ninety-ninth percentile, and
        // the two clipping fractions. This is the indoor/outdoor prior section 6.1
        // asks for, and it is four numbers rather than one because the useful signal
        // is the *shape*: an overcast outdoor frame and a bright indoor one have
        // similar medians and completely different p1-to-p99 spreads.
        if let Some(luma) = self.luma {
            out[10] = luma.p50.clamp(0.0, 1.0);
            out[11] = luma.p1.clamp(0.0, 1.0);
            out[12] = luma.p99.clamp(0.0, 1.0);
            out[13] = (luma.clip_lo + luma.clip_hi).clamp(0.0, 1.0);
        }

        // 14, 15: colour temperature, as a normalised value and a present flag.
        //
        // The flag is separate because 0.0 kelvin and "the camera did not record one"
        // are different, and a single slot would make an unrecorded temperature look
        // like the most tungsten frame in the wedding.
        if let Some(kelvin) = self.kelvin {
            out[14] = ((f64::from(kelvin) - 2000.0) / 8000.0).clamp(0.0, 1.0) as f32;
            out[15] = 1.0;
        }

        out
    }

    /// True when EXIF says the flash fired.
    #[must_use]
    pub fn flash_fired(&self) -> Option<bool> {
        self.flash
    }

    /// True when the luminance statistics say the frame is unlit.
    #[must_use]
    pub fn is_dark(&self) -> Option<bool> {
        self.luma.map(|luma| luma.p50 < NIGHT_MEDIAN_BELOW)
    }

    /// True when the recorded colour temperature is tungsten-dominant.
    #[must_use]
    pub fn is_tungsten(&self) -> Option<bool> {
        self.kelvin.map(|kelvin| kelvin < TUNGSTEN_BELOW_K)
    }
}

/// What the attribute decode did, for the telemetry line and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AttributeReport {
    /// The bits.
    pub flags: AttrFlags,
    /// How many bits the head set on its own.
    pub predicted: u8,
    /// How many bits a measurement overrode. See the module header.
    pub corrected: u8,
}

/// Turn fourteen sigmoid outputs plus what the catalog knows into fourteen bits.
///
/// Returns [`AttrFlags::EMPTY`] with `predicted = 0` when `probs` is not fourteen long,
/// which is the shape-mismatch case; the caller has already been told by the runtime
/// and this must not be the second place it fails.
#[must_use]
pub fn decode_attributes(probs: &[f32], context: &ContextFeatures) -> AttributeReport {
    if probs.len() != AttrFlags::COUNT {
        return AttributeReport::default();
    }

    let mut flags = AttrFlags::EMPTY;
    let mut predicted = 0u8;
    for (index, flag) in AttrFlags::ALL.into_iter().enumerate() {
        let above = probs.get(index).is_some_and(|p| *p >= ATTRIBUTE_THRESHOLD);
        if above {
            flags = flags.union(flag);
            predicted = predicted.saturating_add(1);
        }
    }

    // The four measured attributes. Each one overrides only when the measurement
    // exists, and each one is counted so `AttributeReport::corrected` can be asserted
    // in a test rather than trusted.
    let mut corrected = 0u8;

    if let Some(fired) = context.flash_fired() {
        if flags.contains(AttrFlags::FLASH) != fired {
            corrected = corrected.saturating_add(1);
        }
        flags = flags.with(AttrFlags::FLASH, fired);
    }

    if let Some(dark) = context.is_dark() {
        if flags.contains(AttrFlags::NIGHT) != dark {
            corrected = corrected.saturating_add(1);
        }
        flags = flags.with(AttrFlags::NIGHT, dark);
    }

    if let Some(tungsten) = context.is_tungsten() {
        if flags.contains(AttrFlags::TUNGSTEN) != tungsten {
            corrected = corrected.saturating_add(1);
        }
        flags = flags.with(AttrFlags::TUNGSTEN, tungsten);
    }

    // `indoor` is the one of the four that is a *prior* rather than a measurement, so
    // it only moves the bit when the head is genuinely undecided. A confident head is
    // left alone: an outdoor backlit portrait at ISO 3200 is a real photograph and
    // relabelling it indoors would widen exactly the wrong tolerances.
    if let Some(iso) = context.iso {
        let raw = probs.first().copied().unwrap_or(0.5);
        let unsure = (raw - 0.5).abs() < INDOOR_UNSURE;
        if unsure && iso >= HIGH_ISO && !flags.contains(AttrFlags::INDOOR) {
            flags = flags.union(AttrFlags::INDOOR);
            corrected = corrected.saturating_add(1);
        }
    }

    AttributeReport {
        flags,
        predicted,
        corrected,
    }
}
