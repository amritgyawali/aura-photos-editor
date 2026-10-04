//! The robust middle of many readings, and how much they disagreed.
//!
//! Medians and median absolute deviations throughout. Phase 25's `stats.rs` wrote the argument
//! and this phase needs it more than that one did: a page has a black-and-white frame on it, a
//! flat-lay of invitations on a white desk, and one photograph of a sunset that is orange from
//! corner to corner. Each of those moves a mean by a visible amount and a median by nothing.
//!
//! ## What decides which bucket a reading contributes to
//!
//! Every reading contributes to the **global** aggregate, unconditionally. A reading contributes
//! to its own **lighting bucket** only when [`ReferenceReading::lighting_confidence`] is at or
//! above [`APPLY_ABOVE`] - the same threshold, and the same argument, as the one that decides
//! whether a bucket's delta is applied at all.
//!
//! The asymmetry is deliberate. A frame nobody can place still says something true about how
//! this page looks overall, and dropping it would throw away evidence for the one aggregate that
//! is always used. Putting it in a bucket it is not confidently in is different: it would make
//! that bucket's answer partly about a frame from somewhere else, and the whole reason to have
//! buckets is that a page treats candlelight differently from open shade.

use std::collections::BTreeMap;

use aura_core::contract::colour::HslBand;
use aura_core::contract::look::{
    BandReading, LookAggregate, ReferenceReading, ToneLandmarks, ZoneTint, APPLY_ABOVE,
};
use aura_core::contract::style::LightingBucket;

/// Above this median absolute deviation in the median tone, a bucket's photographs are not one
/// look.
///
/// Twelve points of L\*, as a fraction: 0.12. A page whose frames' medians scatter by more than
/// that in one kind of light is a page with two looks in it, or a bucket that has collected
/// frames from three different lights because the temperature estimate could not separate them.
/// Either way the median of it is an average of things that should not be averaged, and
/// [`aura_core::contract::look::LookCode::BucketIncoherent`] is what says so.
pub const INCOHERENT_SPREAD: f32 = 0.12;

/// The median of a slice. Returns zero when it is empty.
///
/// The lower of the two middle values on an even count rather than their mean, because every
/// caller here is taking a median of a *measurement* and the mean of two measurements is a value
/// no photograph had. It also makes the fold exactly reproducible, which is what
/// `the_same_reference_produces_the_same_look` asserts.
#[must_use]
pub fn median(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted.get((sorted.len() - 1) / 2).copied().unwrap_or(0.0)
}

/// The median absolute deviation from the median: this set's spread, robustly.
#[must_use]
pub fn mad(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let centre = median(values);
    let deviations: Vec<f32> = values.iter().map(|value| (value - centre).abs()).collect();
    median(&deviations)
}

/// Pull one field out of every reading.
fn column<F>(readings: &[&ReferenceReading], pick: F) -> Vec<f32>
where
    F: Fn(&ReferenceReading) -> f32,
{
    readings.iter().map(|reading| pick(reading)).collect()
}

/// The robust middle of many readings.
#[must_use]
pub fn fold(readings: &[&ReferenceReading]) -> LookAggregate {
    if readings.is_empty() {
        return LookAggregate::default();
    }

    let tone = ToneLandmarks::from_array([
        median(&column(readings, |r| r.tone.p01)),
        median(&column(readings, |r| r.tone.p05)),
        median(&column(readings, |r| r.tone.p25)),
        median(&column(readings, |r| r.tone.p50)),
        median(&column(readings, |r| r.tone.p75)),
        median(&column(readings, |r| r.tone.p95)),
        median(&column(readings, |r| r.tone.p99)),
    ]);

    let mut bands = [BandReading::default(); HslBand::COUNT];
    for (index, slot) in bands.iter_mut().enumerate() {
        *slot = BandReading {
            share: median(&column(readings, |r| {
                r.bands.get(index).map_or(0.0, |band| band.share)
            })),
            chroma: median(&column(readings, |r| {
                r.bands.get(index).map_or(0.0, |band| band.chroma)
            })),
            luma: median(&column(readings, |r| {
                r.bands.get(index).map_or(0.0, |band| band.luma)
            })),
        };
    }

    LookAggregate {
        samples: u32::try_from(readings.len()).unwrap_or(u32::MAX),
        tone,
        // The spread of the *median* landmark, which is the one number that says whether these
        // photographs are one look. Spread in `p01` is mostly how dark the darkest corner
        // happened to be; spread in `p50` is whether the page is consistently exposed.
        tone_spread: mad(&column(readings, |r| r.tone.p50)),
        shadow: ZoneTint {
            a: median(&column(readings, |r| r.shadow.a)),
            b: median(&column(readings, |r| r.shadow.b)),
        },
        mid: ZoneTint {
            a: median(&column(readings, |r| r.mid.a)),
            b: median(&column(readings, |r| r.mid.b)),
        },
        high: ZoneTint {
            a: median(&column(readings, |r| r.high.a)),
            b: median(&column(readings, |r| r.high.b)),
        },
        chroma_p50: median(&column(readings, |r| r.chroma_p50)),
        chroma_p90: median(&column(readings, |r| r.chroma_p90)),
        bands,
        rendered_cct_k: median(&column(readings, |r| r.rendered_cct_k)),
    }
}

/// Every reading, sorted into the lighting bucket it is confidently in.
///
/// A reading below [`APPLY_ABOVE`] appears in no bucket. It is **not** dropped - see this
/// module's header - it simply does not get a vote on how this page treats one kind of light.
#[must_use]
pub fn by_lighting<'a>(
    readings: &'a [ReferenceReading],
) -> BTreeMap<LightingBucket, Vec<&'a ReferenceReading>> {
    let mut out: BTreeMap<LightingBucket, Vec<&'a ReferenceReading>> = BTreeMap::new();
    for reading in readings {
        if reading.lighting_confidence < APPLY_ABOVE {
            continue;
        }
        out.entry(reading.lighting).or_default().push(reading);
    }
    out
}

/// Every reading, as borrowed references, for the global fold.
#[must_use]
pub fn all(readings: &[ReferenceReading]) -> Vec<&ReferenceReading> {
    readings.iter().collect()
}

/// True when the photographs behind an aggregate scatter too much to call them one look.
#[must_use]
pub fn is_incoherent(aggregate: &LookAggregate) -> bool {
    aggregate.samples > 1 && aggregate.tone_spread > INCOHERENT_SPREAD
}
