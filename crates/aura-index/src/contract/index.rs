//! FROZEN CONTRACT. The vector substrate every later phase asks questions of.
//!
//! Phase 05 section 5 freezes these shapes before any index exists, and the
//! ordering is the point: scene clustering (07), burst grouping and duplicate
//! detection (08), coverage (12), gallery intelligence (25), camera matching (26)
//! and curation (29) are all written against `SimilarityIndex`, so none of them
//! can grow a dependency on *how* the neighbours were found. A caller hands over
//! a query vector, a `k` and a filter, and gets back ids and distances.
//!
//! What is deliberately **not** frozen is the graph underneath. `HnswIndex` is
//! what ships; a disk-backed index for very large projects, or a different graph
//! entirely, can replace it without an ADR and without touching a caller.
//!
//! Three spellings differ from the phase document, all recorded in
//! `docs/adr/ADR-0011-embeddings-and-similarity-index.md`:
//!
//! * `[f16; 512]` is `[F16; 512]`, because `f16` is a nightly primitive on the
//!   pinned toolchain. Same 1,024 bytes, same rounding, no dependency.
//! * `exclude: Option<&'static [ImageId]>` is `Option<&'a [ImageId]>`, because a
//!   `&'static` slice can only be built at runtime by leaking, and every caller
//!   builds its exclusion set from a query it just ran.
//! * `insert` is unchanged and joined by `insert_entry` on the concrete index,
//!   because `insert` cannot carry the timeline stamp and camera that the
//!   filters need.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The identity of a photograph in a similarity query.
///
/// Phase 05 names this `ImageId`, phase 02 aliased the same thing, and the
/// catalog calls it a `PhotoId`. One type, aliased rather than duplicated, so no
/// conversion can ever disagree.
pub type ImageId = aura_core::PhotoId;

/// Length of every embedding this contract carries.
pub const EMBED_DIM: usize = 512;

/// Bins in the 8x8x8 HSV histogram.
pub const HSV_BINS: usize = 512;

/// Half-precision float, stored as its IEEE 754 binary16 bits.
///
/// The phase document spells the field `[f16; 512]`. Rust's `f16` is a nightly
/// primitive on the toolchain this workspace pins, and pulling in a crate for a
/// forty-line conversion would put a dependency in the middle of a frozen
/// contract. The bits are identical, so a stored vector is byte-compatible with
/// anything that later reads it as `f16`.
///
/// Rounding is round-to-nearest, ties-to-even, the same rule
/// `aura_infer::tensor::round_to_f16` uses, so a vector that has been through the
/// runtime and a vector that has been through here agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct F16(u16);

impl F16 {
    /// Positive zero.
    pub const ZERO: Self = Self(0);

    /// Wrap raw binary16 bits.
    #[must_use]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// The raw binary16 bits, as stored in the catalog blob.
    #[must_use]
    pub const fn to_bits(self) -> u16 {
        self.0
    }

    /// Round a single-precision value to half precision.
    ///
    /// Infinities and NaN are preserved; anything above the half-precision range
    /// saturates to infinity, and anything below the subnormal range flushes to
    /// a signed zero. An embedding component is in `-1..1` after normalisation,
    /// so none of those paths is reachable in practice - they exist so that a
    /// malformed model output cannot produce a silently wrong bit pattern.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn from_f32(value: f32) -> Self {
        let bits = value.to_bits();
        let sign = ((bits >> 16) & 0x8000) as u16;
        let exponent = ((bits >> 23) & 0xff).cast_signed();
        let mantissa = bits & 0x007f_ffff;

        // NaN and infinity keep their class.
        if exponent == 0xff {
            let payload = if mantissa == 0 { 0 } else { 0x0200 };
            return Self(sign | 0x7c00 | payload);
        }

        // Unbias from f32 (127) and rebias to f16 (15).
        let unbiased = exponent - 127 + 15;
        if unbiased >= 0x1f {
            return Self(sign | 0x7c00);
        }
        if unbiased <= 0 {
            // Subnormal, or too small to represent at all.
            if unbiased < -10 {
                return Self(sign);
            }
            let with_implicit = mantissa | 0x0080_0000;
            let shift = (14 - unbiased) as u32;
            let value = with_implicit >> shift;
            let round = round_bit(with_implicit, shift);
            return Self(sign | ((value + round) as u16));
        }

        let value = ((unbiased as u32) << 10) | (mantissa >> 13);
        let round = round_bit(mantissa, 13);
        Self(sign | ((value + round) as u16))
    }

    /// Widen back to single precision. Exact: every binary16 value is a binary32
    /// value.
    #[must_use]
    pub fn to_f32(self) -> f32 {
        let sign = u32::from(self.0 & 0x8000) << 16;
        let exponent = u32::from((self.0 >> 10) & 0x1f);
        let mantissa = u32::from(self.0 & 0x03ff);

        if exponent == 0 {
            if mantissa == 0 {
                return f32::from_bits(sign);
            }
            // Subnormal: the value is `mantissa * 2^-24`. Shift the leading one
            // into the implicit position and count how far it moved.
            //
            // The exponent starts at zero, not at minus one. A half-precision
            // subnormal component is around 1e-5, which is exactly the magnitude a
            // normalised 512-d embedding produces in its quietest dimensions, and an
            // off-by-one factor of two there is invisible in every test except the
            // one that compares a reloaded snapshot's distances with the original's.
            let mut exponent = 0i32;
            let mut mantissa = mantissa;
            while mantissa & 0x0400 == 0 {
                mantissa <<= 1;
                exponent -= 1;
            }
            let mantissa = mantissa & 0x03ff;
            let biased = (exponent + 127 - 14) as u32;
            return f32::from_bits(sign | (biased << 23) | (mantissa << 13));
        }
        if exponent == 0x1f {
            let payload = if mantissa == 0 { 0 } else { mantissa << 13 };
            return f32::from_bits(sign | 0x7f80_0000 | payload);
        }
        let biased = exponent + 127 - 15;
        f32::from_bits(sign | (biased << 23) | (mantissa << 13))
    }
}

/// Round-to-nearest, ties-to-even, on the bits about to be discarded.
fn round_bit(mantissa: u32, shift: u32) -> u32 {
    let Some(half) = 1u32.checked_shl(shift.saturating_sub(1)) else {
        return 0;
    };
    let dropped = mantissa & ((1u32 << shift) - 1);
    let keeps_odd = (mantissa >> shift) & 1 == 1;
    u32::from(dropped > half || (dropped == half && keeps_odd))
}

impl fmt::Display for F16 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_f32())
    }
}

/// Luminance statistics, computed on the same pass as the embedding.
///
/// Phases 25 and 26 read these for gallery colour consistency and camera
/// matching, phase 09 uses the clipping fractions as a free exposure prior, and
/// phase 15 uses the percentiles as the starting point for exposure. Computing
/// them here means the pixels are decoded once.
///
/// All six are in `0..1` against the 8-bit sRGB proxy the embedding runs on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LumaStats {
    /// Arithmetic mean luminance.
    pub mean: f32,
    /// First percentile: where the shadows actually start.
    pub p1: f32,
    /// Median.
    pub p50: f32,
    /// Ninety-ninth percentile: where the highlights actually end.
    pub p99: f32,
    /// Fraction of pixels at or below the black point.
    pub clip_lo: f32,
    /// Fraction of pixels at or above the white point.
    pub clip_hi: f32,
}

impl LumaStats {
    /// All zeroes. The value a frame with no pixels would report, and never a
    /// value a real frame produces: `p99` of a real frame is above zero.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            mean: 0.0,
            p1: 0.0,
            p50: 0.0,
            p99: 0.0,
            clip_lo: 0.0,
            clip_hi: 0.0,
        }
    }

    /// Dynamic range actually used, p1 to p99.
    #[must_use]
    pub fn spread(&self) -> f32 {
        (self.p99 - self.p1).max(0.0)
    }
}

/// One image's perceptual signature: the vector plus the cheap descriptors that
/// were computed in the same pass.
///
/// This is the phase document's `ImageEmbedding`, with `[f16; 512]` spelled
/// `[F16; 512]` for the reason in this module's header.
///
/// Deliberately **not** `Serialize`: serde implements its array traits only up to
/// 32 elements, and the two places this type is written down - the catalog blob in
/// [`crate::store`] and the graph snapshot in [`crate::snapshot`] - both have a
/// fixed byte layout of their own that a derived encoding would not match. A
/// derive here would be a third, incompatible format for the same 1,024 bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageEmbedding {
    /// The photograph this describes.
    pub image_id: ImageId,
    /// L2-normalised 512-d embedding, half precision.
    pub vec: [F16; EMBED_DIM],
    /// 64-bit difference hash. Hamming distance answers "is this the same frame"
    /// without touching the index.
    pub dhash: u64,
    /// 8x8x8 HSV histogram, each bin scaled to `0..255`.
    pub hsv_hist: [u8; HSV_BINS],
    /// Luminance statistics for this frame.
    pub luma: LumaStats,
    /// Which model version produced `vec`. A row whose version is not the current
    /// one is re-embedded in the background, never silently compared.
    pub model_ver: u16,
}

impl ImageEmbedding {
    /// An all-zero embedding for `id`. Used by tests and by the refusal path,
    /// never stored: `IndexStats::degenerate` counts any that reach the index.
    #[must_use]
    pub fn zeroed(image_id: ImageId, model_ver: u16) -> Self {
        Self {
            image_id,
            vec: [F16::ZERO; EMBED_DIM],
            dhash: 0,
            hsv_hist: [0; HSV_BINS],
            luma: LumaStats::zero(),
            model_ver,
        }
    }

    /// Widen the stored vector to single precision for arithmetic.
    #[must_use]
    pub fn to_f32(&self) -> Vec<f32> {
        self.vec.iter().map(|half| half.to_f32()).collect()
    }

    /// True when every component is zero. A model that failed and returned zeros
    /// would otherwise sit at cosine distance 1.0 from everything and look like a
    /// legitimately unusual photograph.
    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        self.vec
            .iter()
            .all(|half| half.to_bits().trailing_zeros() >= 15)
    }

    /// Hamming distance between two difference hashes, `0..=64`.
    #[must_use]
    pub fn dhash_distance(&self, other: &Self) -> u32 {
        (self.dhash ^ other.dhash).count_ones()
    }
}

/// A dense handle for a camera body inside one index.
///
/// Not the catalog's `camera_id`, which is `cam_<64 hex characters>`. A filter
/// that compared those per candidate would cost more than the distance
/// computation it filters. The index owns the mapping; see ADR-0011 section 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CameraId(pub u32);

/// A dense handle for a scene, assigned by phase 07.
///
/// Present in the contract now because the filter is frozen now. Until phase 07
/// runs, no entry carries one and `IndexFilter::scene` matches nothing - which is
/// the honest answer, not an empty one dressed up as "no filter".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SceneId(pub u32);

/// Which frames a query is allowed to return.
///
/// Every field is `None` by default, and `IndexFilter::none()` is the unfiltered
/// query. Filters compose: a time window *and* a camera means both.
///
/// Time windows are applied as a pre-filter over a sorted timeline array rather
/// than by discarding results afterwards (section 6.3), which is what keeps a
/// burst query under a millisecond on a 4,000-image project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexFilter<'a> {
    /// Only frames captured within plus or minus this many seconds of the query.
    pub time_window_s: Option<u32>,
    /// Only frames from this camera body.
    pub camera_id: Option<CameraId>,
    /// Only frames in this scene. No effect until phase 07 assigns scenes.
    pub scene: Option<SceneId>,
    /// Never return these. Usually the results of the query just before.
    pub exclude: Option<&'a [ImageId]>,
}

impl<'a> IndexFilter<'a> {
    /// No restriction at all.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            time_window_s: None,
            camera_id: None,
            scene: None,
            exclude: None,
        }
    }

    /// Only frames within plus or minus `seconds` of the query frame.
    #[must_use]
    pub const fn within_seconds(seconds: u32) -> Self {
        Self {
            time_window_s: Some(seconds),
            camera_id: None,
            scene: None,
            exclude: None,
        }
    }

    /// Restrict to one camera body.
    #[must_use]
    pub const fn with_camera(mut self, camera: CameraId) -> Self {
        self.camera_id = Some(camera);
        self
    }

    /// Restrict to one scene.
    #[must_use]
    pub const fn with_scene(mut self, scene: SceneId) -> Self {
        self.scene = Some(scene);
        self
    }

    /// Exclude a set of frames that a previous query already claimed.
    #[must_use]
    pub const fn excluding(mut self, ids: &'a [ImageId]) -> Self {
        self.exclude = Some(ids);
        self
    }

    /// True when nothing is restricted.
    #[must_use]
    pub const fn is_unfiltered(&self) -> bool {
        self.time_window_s.is_none()
            && self.camera_id.is_none()
            && self.scene.is_none()
            && self.exclude.is_none()
    }

    /// True when this filter needs metadata a bare `insert` cannot supply.
    #[must_use]
    pub const fn needs_metadata(&self) -> bool {
        self.time_window_s.is_some() || self.camera_id.is_some() || self.scene.is_some()
    }

    /// Stable text for the `index.query` telemetry event. Never the filter's
    /// contents: a time window plus a camera identifies a shoot.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match (self.time_window_s, self.camera_id, self.scene) {
            (None, None, None) => "none",
            (Some(_), None, None) => "time",
            (None, Some(_), None) => "camera",
            (None, None, Some(_)) => "scene",
            _ => "composite",
        }
    }
}

/// What the index is holding, for the settings panel and the phase gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IndexStats {
    /// Vectors in the graph.
    pub vectors: usize,
    /// Vectors that carry timeline and camera metadata, and can therefore be
    /// reached by a filtered query.
    pub filterable: usize,
    /// Vectors inserted without metadata, invisible to a filtered query. Never
    /// zero silently: this is the number that explains a short result list.
    pub unfiltered: usize,
    /// All-zero vectors that reached the index. Always a bug upstream.
    pub degenerate: usize,
    /// Model version every vector in the graph was produced by. A mixed index
    /// reports the lowest, and `stale` says how many disagree.
    pub model_ver: u16,
    /// Vectors produced by a version other than `model_ver`.
    pub stale: usize,
    /// Wall-clock milliseconds the current graph took to build.
    pub build_ms: u32,
    /// True when the graph was loaded from a snapshot rather than built.
    pub from_snapshot: bool,
}

/// The one way to ask what looks like this.
pub trait SimilarityIndex: Send + Sync + fmt::Debug {
    /// The `k` nearest frames to `q`, nearest first.
    ///
    /// Never returns `q` itself. Ties are broken by `timeline_ts` then by
    /// `image_id`, so two machines return the same list in the same order.
    fn knn(&self, q: &ImageEmbedding, k: usize, filter: IndexFilter<'_>) -> Vec<(ImageId, f32)>;

    /// Every frame within `max_dist` of `q`, nearest first.
    fn radius(
        &self,
        q: &ImageEmbedding,
        max_dist: f32,
        filter: IndexFilter<'_>,
    ) -> Vec<(ImageId, f32)>;

    /// The member of `ids` with the smallest summed distance to the others - the
    /// most representative frame of a set, and the one phases 25 and 26 use as a
    /// reference frame.
    ///
    /// `None` when the set is empty or none of its members are indexed.
    fn medoid(&self, ids: &[ImageId]) -> Option<ImageId>;

    /// Add one vector, with whatever metadata the index already knows for its id.
    ///
    /// Frozen as the phase document spells it. The ingest path uses
    /// `HnswIndex::insert_entry`, which carries the metadata a filtered query
    /// needs; see ADR-0011 section 2.
    fn insert(&self, e: &ImageEmbedding);

    /// What the index is holding.
    fn stats(&self) -> IndexStats;
}

/// How many partial sums the dot product accumulates into.
///
/// Eight, and the number is load-bearing twice over.
///
/// It is a **performance** decision because floating-point addition is not
/// associative, so a single-accumulator reduction cannot be vectorised by any
/// compiler - each addition depends on the one before it. Eight independent
/// accumulators over a 512-wide vector give the optimiser eight parallel chains to
/// fill a register with, and building a 4,000-vector graph is roughly 30 GFLOP of
/// exactly this loop.
///
/// It is a **determinism** decision because the number is fixed. Eight partial sums
/// combined in a fixed order is as reproducible as one sum: what would break
/// invariant 4 is an accumulation order that varied with the machine, the thread
/// count or the compiler's mood - not one that is written down here.
const DOT_LANES: usize = 8;

/// Cosine distance between two L2-normalised vectors, in `0..2`.
///
/// Accumulation order is fixed - eight partial sums over `DOT_LANES`-wide chunks,
/// combined left to right, then the tail - so this returns the same bits on every
/// machine, which is what lets the eval harness assert exact equality rather than a
/// tolerance.
///
/// The vectors are normalised at the source, so this is `1 - dot`. When they are
/// not - a degenerate all-zero vector is the only way that happens - the result is
/// 1.0, the "knows nothing" distance, rather than a division by zero.
#[must_use]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let width = a.len().min(b.len());
    let chunks = width / DOT_LANES;
    let mut lanes = [0.0f32; DOT_LANES];

    for chunk in 0..chunks {
        let base = chunk * DOT_LANES;
        // The two slices are the same fixed width, so both `get` calls succeed for
        // every chunk. Written with `get` rather than an index because
        // `clippy::indexing_slicing` is denied crate-wide, and the compiler hoists
        // the check out of the loop when the bound is a local like this.
        let (Some(left), Some(right)) =
            (a.get(base..base + DOT_LANES), b.get(base..base + DOT_LANES))
        else {
            break;
        };
        for (lane, (x, y)) in lanes.iter_mut().zip(left.iter().zip(right.iter())) {
            *lane += x * y;
        }
    }

    let mut dot = 0.0f32;
    for lane in lanes {
        dot += lane;
    }
    // The tail, for a width that is not a multiple of the lane count. `EMBED_DIM`
    // is 512, so this loop does not run in the product - it runs for the shorter
    // vectors the tests use.
    for (x, y) in a
        .iter()
        .zip(b.iter())
        .skip(chunks * DOT_LANES)
        .take(width - chunks * DOT_LANES)
    {
        dot += x * y;
    }

    (1.0 - dot).clamp(0.0, 2.0)
}
