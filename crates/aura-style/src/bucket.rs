//! Which leaf of the style tree a photograph belongs in, and the features that condition the
//! regression inside it.
//!
//! PHASE-17 section 2.1: "every pair is assigned a scene + lighting bucket from Phases 07/15,
//! producing a style tree rather than a single model". This module is that assignment, and it
//! is deliberately the whole of it: a second place that decided what "golden hour" means would
//! be a second style tree.
//!
//! ## Why the lighting axis is not phase 15's illuminant vocabulary verbatim
//!
//! Two changes, both of them arguments about how photographers work rather than about physics.
//!
//! **Fluorescent and LED are one bucket.** Phase 15 separates them because their *spectra*
//! differ and skin reads differently under each - which matters for white balance and matters
//! not at all for taste. Nobody grades a fluorescent hallway differently from an LED one. Two
//! buckets here would halve the sample count of every modern indoor venue for no style anybody
//! could name.
//!
//! **Golden hour is a bucket and is not an illuminant.** Phase 15 calls low sun "daylight",
//! correctly: it is a black body at a lower temperature. But every wedding photographer alive
//! edits golden hour as a separate look, and a tree that put a 3,800 K backlit portrait in the
//! same leaf as a noon group shot would average the two into something neither.
//! [`LightingBucket::GOLDEN_BELOW_K`] is where the split happens and it is a style boundary
//! rather than a physical one.
//!
//! ## The feature section 6.2 names and this module deliberately does not compute
//!
//! Section 6.2 lists "subject luma, CCT, dynamic range, ISO, flash, scene, **skin group**".
//!
//! There is no skin group here and there will not be one. Phase 06's rule - never infer
//! anything about a person - is not weakened for a feature vector, and the Monk-scale buckets
//! phases 15 and 16's fairness harnesses use live in `tests/eval` and never reach the catalog
//! or a decision. A style profile conditioned on a guess about somebody's skin is a product
//! that edits two people differently because of what it decided they were, which is the exact
//! failure the last three phases have been structured to make impossible.
//!
//! What is lost is small and the trade is not close: skin tone correlates with the subject
//! luminance the vector already carries, and the *guarantee* about skin is phase 16's
//! post-condition, which is measured rather than predicted.
//!
//! Scene is not a feature either, and for a different reason: the bucket **is** the scene
//! conditioning. A feature that repeats the leaf's own identity inside the leaf is a column of
//! constants, and ridge regression on a column of constants is an intercept that already
//! exists.

use std::fmt;

use aura_core::contract::style::{LightingBucket, SceneGroup, StyleBucket, StyleQuery};
use aura_core::contract::tone::IlluminantKind;
use aura_core::{AuraResult, SceneId};

/// How many columns the design vector has, including the intercept.
///
/// Seven: an intercept, the subject's own luminance, the colour temperature, the dynamic
/// range, the log ISO, whether the flash fired and whether the light was mixed. Small on
/// purpose - a bucket with eleven samples and a twenty-column design matrix is a fit that
/// interpolates its own noise, and ridge would be holding the whole thing up.
pub const FEATURES: usize = 7;

/// The colour temperature the CCT feature is centred on, in kelvin.
///
/// 5,500 K. Daylight, and therefore the value at which the feature is zero, which puts the
/// intercept where "an ordinary outdoor frame" is rather than at absolute zero kelvin.
pub const CCT_CENTRE_K: f32 = 5500.0;

/// The colour temperature span the CCT feature is scaled by, in kelvin.
///
/// 3,000 K, so candlelight is about -1.1 and open shade about +0.8. Scaling matters more than
/// it looks: ridge penalises every coefficient equally, so a column that ranges over thousands
/// and a column that ranges over one are not penalised equally unless both are normalised.
pub const CCT_SPAN_K: f32 = 3000.0;

/// The dynamic range the range feature is centred on, in stops.
pub const RANGE_CENTRE_EV: f32 = 7.0;

/// The dynamic range span the range feature is scaled by, in stops.
pub const RANGE_SPAN_EV: f32 = 4.0;

/// The ISO the log-ISO feature is centred on.
///
/// Eight hundred. Base ISO would put every daylight frame at the same extreme of the column
/// and every reception frame in a long tail; this is roughly where a wedding's ISO distribution
/// actually sits.
pub const ISO_CENTRE: f32 = 800.0;

/// One frame's design vector, and the bucket it belongs to.
///
/// Both together, because the regression is fitted *inside* a bucket and a vector without its
/// bucket is a row nothing can be grouped by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Features {
    /// Which leaf.
    pub bucket: StyleBucket,
    /// The design vector, [`FEATURES`] long, intercept first.
    pub row: [f32; FEATURES],
}

impl Features {
    /// The design vector for one query.
    ///
    /// Every column is centred and scaled so ridge's single penalty means the same thing for
    /// all of them. The intercept is **not** penalised by [`crate::tree`], which is the usual
    /// convention and the right one here: penalising it would shrink the photographer's
    /// average lean toward zero, and the average lean is precisely what the phase is trying to
    /// learn.
    #[must_use]
    pub fn of(query: &StyleQuery) -> Self {
        let bucket = StyleBucket::new(SceneGroup::of_scene(query.scene), query.lighting);
        Self {
            bucket,
            row: [
                1.0,
                // Centred on mid grey rather than on zero: a frame whose subject sits at 0.18
                // is the ordinary case and should be where the intercept applies.
                query.subject_luma.clamp(0.0, 1.0) - 0.18,
                (query.cct_k - CCT_CENTRE_K) / CCT_SPAN_K,
                (query.dynamic_range_ev - RANGE_CENTRE_EV) / RANGE_SPAN_EV,
                log_iso(query.iso),
                if query.flash { 1.0 } else { 0.0 },
                if query.mixed_light { 1.0 } else { 0.0 },
            ],
        }
    }

    /// The vector for a frame nothing is known about: the intercept alone.
    ///
    /// Every other column is zero, so the prediction is exactly the bucket's intercept - which
    /// is the photographer's average lean for that kind of frame, and the honest answer when
    /// there is nothing to condition on.
    #[must_use]
    pub fn intercept_only(bucket: StyleBucket) -> Self {
        let mut row = [0.0_f32; FEATURES];
        if let Some(slot) = row.first_mut() {
            *slot = 1.0;
        }
        Self { bucket, row }
    }
}

impl fmt::Display for Features {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.bucket)
    }
}

/// The log-ISO column: `log2(iso / ISO_CENTRE)`, with zero ISO treated as base.
///
/// Logarithmic because a photographer's response to noise is logarithmic: the difference
/// between 100 and 400 is one decision and the difference between 6,400 and 6,700 is none.
#[must_use]
pub fn log_iso(iso: u32) -> f32 {
    let iso = if iso == 0 { 100.0 } else { iso as f32 };
    (iso / ISO_CENTRE).max(1e-3).log2()
}

/// The lighting bucket one frame's dominant light falls in.
///
/// `cct_k` is what phase 15 settled on, `kind` is the illuminant it named, and `flash` is what
/// EXIF said. The three disagree often enough that the order of the checks is the policy:
///
/// 1. **A saturated or intentional light wins.** Stage and candle are looks a photographer
///    keeps, and a stage wash that happens to sit at 3,000 K is not tungsten.
/// 2. **Flash wins next**, because a flash is local to the subject and governs how the frame
///    was graded even when the ambient is something else. Phase 15 makes the same call about
///    which illuminant governs.
/// 3. **Everything else is read off the kind**, with daylight split at
///    [`LightingBucket::GOLDEN_BELOW_K`].
#[must_use]
pub fn lighting_of(kind: IlluminantKind, cct_k: f32, flash: bool) -> LightingBucket {
    match kind {
        IlluminantKind::Stage => return LightingBucket::Stage,
        IlluminantKind::Candle => return LightingBucket::Candle,
        _ => {}
    }
    if flash || kind == IlluminantKind::Flash {
        return LightingBucket::Flash;
    }
    match kind {
        IlluminantKind::Daylight => {
            if cct_k > 0.0 && cct_k < LightingBucket::GOLDEN_BELOW_K {
                LightingBucket::GoldenHour
            } else {
                LightingBucket::Daylight
            }
        }
        IlluminantKind::Shade => LightingBucket::Shade,
        IlluminantKind::Cloudy => LightingBucket::Overcast,
        IlluminantKind::Tungsten => LightingBucket::Tungsten,
        IlluminantKind::Fluorescent | IlluminantKind::Led => LightingBucket::Artificial,
        IlluminantKind::Flash => LightingBucket::Flash,
        IlluminantKind::Candle => LightingBucket::Candle,
        IlluminantKind::Stage => LightingBucket::Stage,
        IlluminantKind::Unknown => {
            // No illuminant was named, but a temperature usually still was. Reading the bucket
            // off the temperature alone is weaker evidence than reading it off a named kind,
            // and it is much better than dropping every unclassified frame into one leaf: a
            // wedding whose illuminant head never fired would otherwise train exactly one
            // bucket, and the whole scene conditioning would quietly do nothing.
            if cct_k <= 0.0 {
                LightingBucket::Unknown
            } else if cct_k < 2600.0 {
                LightingBucket::Candle
            } else if cct_k < 3600.0 {
                LightingBucket::Tungsten
            } else if cct_k < LightingBucket::GOLDEN_BELOW_K {
                LightingBucket::GoldenHour
            } else if cct_k < 6200.0 {
                LightingBucket::Daylight
            } else if cct_k < 7400.0 {
                LightingBucket::Overcast
            } else {
                LightingBucket::Shade
            }
        }
    }
}

/// The leaf one frame belongs in.
#[must_use]
pub fn of(scene: SceneId, lighting: LightingBucket) -> StyleBucket {
    StyleBucket::new(SceneGroup::of_scene(scene), lighting)
}

/// Where the fitter gets pixels for one pair.
///
/// A port, deliberately **not** frozen, exactly as `aura_render::FrameSource` and
/// `aura_infer::Backend` are. The production implementation reads a RAW with `aura-raw` and a
/// final with `aura_raw::codec::decode_jpeg`; the fixtures hand back authored pixels whose
/// style is known by construction; neither is a contract change.
///
/// It exists because **the alternative is a crate that cannot be tested in this repository.**
/// There are no photographers' archives here and no camera files, so a fitter that could only
/// read files would have exactly zero measured gates. With the port, every gate in
/// `tests/eval/style_eval.rs` runs a real fit through the real renderer against a real target.
pub trait PairSource: Send + Sync + fmt::Debug {
    /// The original, as an interleaved linear RGB working-space buffer at `long_edge`.
    ///
    /// # Errors
    ///
    /// Whatever the decode raised. `aura-style` adds nothing to it.
    fn original(&self, key: &str, long_edge: u32) -> AuraResult<(Vec<f32>, u32, u32)>;

    /// The delivered final, as interleaved 8-bit sRGB at the same size.
    ///
    /// # Errors
    ///
    /// Whatever the decode raised.
    fn finished(&self, key: &str, long_edge: u32) -> AuraResult<(Vec<u8>, u32, u32)>;

    /// The camera model EXIF recorded, for the profile lookup inside the renderer.
    fn camera(&self, key: &str) -> String;

    /// An XMP packet for this pair, when the archive had one.
    ///
    /// `None` sends the pair down the fitting path, which is section 6.1's fallback and the
    /// expensive one. A photographer who kept their catalogue gets the exact path for free.
    fn xmp(&self, key: &str) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_hour_is_daylight_below_the_boundary_and_daylight_above_it() {
        assert_eq!(
            lighting_of(IlluminantKind::Daylight, 3900.0, false),
            LightingBucket::GoldenHour
        );
        assert_eq!(
            lighting_of(IlluminantKind::Daylight, 5600.0, false),
            LightingBucket::Daylight
        );
    }

    #[test]
    fn a_stage_wash_at_tungsten_temperature_is_still_stage() {
        assert_eq!(
            lighting_of(IlluminantKind::Stage, 3100.0, false),
            LightingBucket::Stage
        );
    }

    #[test]
    fn flash_beats_the_ambient_it_was_fired_into() {
        assert_eq!(
            lighting_of(IlluminantKind::Tungsten, 2900.0, true),
            LightingBucket::Flash
        );
    }

    #[test]
    fn a_saturated_light_beats_flash_because_it_is_a_look_rather_than_a_correction() {
        assert_eq!(
            lighting_of(IlluminantKind::Stage, 3100.0, true),
            LightingBucket::Stage
        );
    }

    #[test]
    fn fluorescent_and_led_land_in_one_bucket() {
        assert_eq!(
            lighting_of(IlluminantKind::Fluorescent, 4100.0, false),
            lighting_of(IlluminantKind::Led, 4100.0, false)
        );
    }

    #[test]
    fn an_unknown_kind_still_buckets_off_its_temperature() {
        assert_eq!(
            lighting_of(IlluminantKind::Unknown, 2400.0, false),
            LightingBucket::Candle
        );
        assert_eq!(
            lighting_of(IlluminantKind::Unknown, 8000.0, false),
            LightingBucket::Shade
        );
        assert_eq!(
            lighting_of(IlluminantKind::Unknown, 0.0, false),
            LightingBucket::Unknown
        );
    }

    #[test]
    fn the_design_vector_is_centred_where_an_ordinary_frame_is() {
        let query = StyleQuery {
            subject_luma: 0.18,
            cct_k: CCT_CENTRE_K,
            dynamic_range_ev: RANGE_CENTRE_EV,
            iso: ISO_CENTRE as u32,
            ..StyleQuery::default()
        };
        let features = Features::of(&query);
        assert!((features.row[0] - 1.0).abs() < 1e-6);
        for column in features.row.iter().skip(1) {
            assert!(column.abs() < 1e-3, "column {column} is not centred");
        }
    }

    #[test]
    fn log_iso_is_zero_at_the_centre_and_one_stop_per_doubling() {
        assert!(log_iso(800).abs() < 1e-6);
        assert!((log_iso(1600) - 1.0).abs() < 1e-6);
        assert!((log_iso(400) + 1.0).abs() < 1e-6);
    }
}
