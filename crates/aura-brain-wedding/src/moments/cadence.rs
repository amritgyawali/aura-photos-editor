//! How fast the photographer was shooting, minute by minute, per camera.
//!
//! PHASE-08 section 6.1. This is the first thing the grouping pass does and the thing
//! that makes the rest of it work on both a 10 fps bouquet toss and a ceremony shot in
//! ones and twos.
//!
//! ## Why the window is adaptive at all
//!
//! A fixed burst window cannot be right. Choose two seconds and a 10 fps burst of forty
//! frames spanning four seconds becomes two moments. Choose eight seconds and a
//! ceremony shot at one frame every five seconds becomes one moment per minute of the
//! service - which is worse, because section 12's first failure mode is that a merge
//! hides a keeper and a photographer never sees the frame that was absorbed.
//!
//! So the window is derived from what the photographer was actually doing at that
//! moment of the day, on that body:
//!
//! ```text
//! w(t) = clamp(2.5 x median_interval(t), 0.7 s, 8 s)
//! ```
//!
//! where `median_interval(t)` is the median gap between consecutive frames inside a
//! 60-second neighbourhood of `t`. During a burst the median collapses to about 0.1 s
//! and the window goes to its 0.7 s floor - which is still seven frames of a 10 fps
//! burst, and the graph's similarity term joins them anyway. During a slow ceremony the
//! median is seconds and the window stretches, which is what stops two consecutive
//! ceremony frames from being separate moments merely because eleven seconds passed.
//!
//! **The multiplier is 2.5 and not 2.** At exactly 2 x median, half the intervals in a
//! Poisson-ish shooting pattern fall outside the window by construction, so every
//! second frame starts a new moment. The extra half is what makes the window a
//! description of the cadence rather than a coin flip about it.
//!
//! ## Per camera, always
//!
//! Two photographers interleaved on one timeline have a combined median interval of
//! roughly half of either's, which would halve the window for both. So cadence is
//! estimated per body and the merge of the two happens later, in
//! [`super::merge`], where it can be justified by overlap and appearance rather than
//! by an arithmetic accident. This is the same two-shooter failure phase 07's
//! `two_shooters_do_not_fragment_the_timeline` guards, one layer down.
//!
//! ## What is measured rather than inferred
//!
//! Section 6.1: "sub-second EXIF and drive-mode tags, where present, mark true camera
//! bursts explicitly - use them as strong evidence rather than inferring". [`Drive`]
//! carries that evidence, and [`Cadence::drive_pair`] reports it. A pair of frames the
//! camera itself says were a burst does not need the estimator's opinion.
//!
//! Nothing here decides anything. A window is evidence about how fast the shutter was
//! moving; whether two frames are one moment is [`super::graph`]'s question and the
//! threshold is the scene's.

use std::collections::BTreeMap;

use aura_core::contract::moment::{CameraId, ImageId, Timestamp};

/// The narrowest the burst window may be, in milliseconds.
///
/// Section 6.1's 0.7 s. Seven frames of a 10 fps burst, which is longer than almost
/// every real burst's *internal* gap and shorter than the gap between two deliberate
/// presses of the shutter.
pub const MIN_WINDOW_MS: i64 = 700;

/// The widest the burst window may be, in milliseconds.
///
/// Section 6.1's 8 s clamp. Beyond this the window stops describing a burst and starts
/// describing a chapter, which is phase 07's job.
pub const MAX_WINDOW_MS: i64 = 8_000;

/// How many median intervals wide the window is. Section 6.1's 2.5.
pub const CADENCE_MULTIPLIER: f32 = 2.5;

/// The neighbourhood the rolling median is taken over, in milliseconds.
///
/// Section 6.1's 60 s. Long enough to contain several presses of the shutter at any
/// realistic cadence, short enough that the ceremony's pace does not leak into the
/// confetti line thirty seconds later.
pub const NEIGHBOURHOOD_MS: i64 = 60_000;

/// A gap above which no two frames are ever the same moment, in milliseconds.
///
/// Section 6.1: "long gaps (> 20 s) break groups regardless of similarity, because the
/// same pose 30 seconds later is usually a new attempt at a moment, not the same
/// moment."
///
/// It is a hard rule rather than a scaled one, and it is checked before any similarity
/// is computed. A high-similarity edge across a 25-second gap is exactly the edge that
/// merges two family-portrait groups into one, and no threshold that also passes real
/// bursts can reject it on appearance.
pub const HARD_GAP_MS: i64 = 20_000;

/// Below this interval, consecutive frames are a camera burst whatever else is true.
///
/// Two frames 250 ms apart cannot be two deliberate compositions; no photographer
/// re-frames in a quarter of a second. This is the inferred half of section 6.1's
/// drive-mode evidence, used when the EXIF half is absent - which it is for most
/// cameras, because sub-second and drive-mode tags are patchily written.
pub const DRIVE_INTERVAL_MS: i64 = 250;

/// What the camera itself said about how a frame was taken.
///
/// Section 6.1's "strong evidence rather than inferring". Two frames whose EXIF says
/// they were one continuous release are one burst, and no estimate is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Drive {
    /// The EXIF sub-second field, when the camera wrote one.
    ///
    /// Present on most bodies since about 2010 and absent on scanned film, phone
    /// screenshots and anything that has been through a lossy metadata pipeline.
    pub sub_sec: Option<u16>,
    /// True when the body recorded a continuous-drive exposure.
    ///
    /// Read from the maker-note drive-mode field where phase 01's EXIF pass could
    /// resolve it. `false` means "not recorded", never "single shot" - which is why
    /// this is only ever used as positive evidence.
    pub continuous: bool,
}

impl Drive {
    /// Nothing was recorded. The value for a frame whose EXIF said neither thing.
    pub const NONE: Self = Self {
        sub_sec: None,
        continuous: false,
    };

    /// True when the camera said anything at all about the drive.
    #[must_use]
    pub const fn is_recorded(&self) -> bool {
        self.sub_sec.is_some() || self.continuous
    }
}

/// One frame, as the cadence estimator needs to see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    /// The photograph.
    pub image: ImageId,
    /// Timeline time, milliseconds since the Unix epoch. Clock-aligned by phase 01.
    pub ts: Timestamp,
    /// The body it came off.
    pub camera: u32,
    /// What the camera said about the drive.
    pub drive: Drive,
}

/// The shooting cadence of one wedding, per body and per moment of the day.
///
/// Built once per grouping pass and read O(1) per candidate pair, which is what keeps
/// section 11's six-second budget for four thousand images reachable: the estimate is
/// a binary search over a small per-camera array rather than a scan.
#[derive(Debug, Clone, Default)]
pub struct Cadence {
    /// Per camera: the frame times, ascending, and the window in force at each.
    ///
    /// Parallel arrays rather than a `Vec<(i64, i64)>` because the binary search only
    /// ever touches the first, and a struct of two arrays keeps that search inside one
    /// cache line's worth of stride.
    per_camera: BTreeMap<u32, (Vec<Timestamp>, Vec<i64>)>,
    /// The median interval across the whole wedding, for the report.
    median_ms: i64,
}

impl Cadence {
    /// Estimate the cadence of a wedding.
    ///
    /// `shots` need not be sorted; they are grouped by camera and sorted inside. The
    /// cost is one sort per body plus one linear pass, both of which are inside the
    /// budget by an order of magnitude on a 12,000-frame project.
    #[must_use]
    pub fn estimate(shots: &[Shot]) -> Self {
        let mut by_camera: BTreeMap<u32, Vec<Timestamp>> = BTreeMap::new();
        for shot in shots {
            by_camera.entry(shot.camera).or_default().push(shot.ts);
        }

        let mut per_camera = BTreeMap::new();
        let mut every_interval: Vec<i64> = Vec::new();

        for (camera, mut times) in by_camera {
            times.sort_unstable();
            let intervals = intervals_of(&times);
            every_interval.extend(intervals.iter().copied());
            let windows = rolling_windows(&times, &intervals);
            per_camera.insert(camera, (times, windows));
        }

        Self {
            per_camera,
            median_ms: median(&mut every_interval),
        }
    }

    /// The burst window in force on `camera` at `when`, in milliseconds.
    ///
    /// A camera the estimator never saw gets the widest sensible answer rather than the
    /// narrowest: an unknown cadence should not fragment a wedding, and the similarity
    /// threshold is still in front of every edge.
    #[must_use]
    pub fn window_ms(&self, camera: u32, when: Timestamp) -> i64 {
        let Some((times, windows)) = self.per_camera.get(&camera) else {
            return MAX_WINDOW_MS;
        };
        let slot = match times.binary_search(&when) {
            Ok(exact) => exact,
            // `partition_point` semantics: the insertion point is the first frame after
            // `when`, so the window that applies is the one before it. Saturating rather
            // than checked, because index zero is the correct answer for a time before
            // the first frame.
            Err(after) => after.saturating_sub(1),
        };
        windows.get(slot).copied().unwrap_or(MAX_WINDOW_MS)
    }

    /// The median inter-frame interval across the whole wedding, in milliseconds.
    ///
    /// Pooled from every body's *own* intervals, never from the interleaved timeline.
    /// Two photographers each shooting once a second produce a merged timeline with a
    /// 500 ms median and this reports 1,000 - which is what a photographer means by
    /// "how fast were we shooting", and is the number the windows were actually
    /// derived from.
    ///
    /// Reported rather than used: it is the number a support engineer reads first when
    /// a wedding groups strangely, because it says at a glance whether this was a
    /// sports-style shooter or a one-frame-per-pose one.
    #[must_use]
    pub const fn median_ms(&self) -> i64 {
        self.median_ms
    }

    /// How many bodies contributed.
    #[must_use]
    pub fn cameras(&self) -> usize {
        self.per_camera.len()
    }

    /// True when two frames of the same body are certainly one camera burst.
    ///
    /// Section 6.1's strong evidence, in the order the evidence is trustworthy:
    ///
    /// 1. **Both frames say continuous drive** - the body itself recorded that the
    ///    shutter was held down. Nothing beats this and nothing needs to.
    /// 2. **Same whole second, different sub-second** - two frames inside one second is
    ///    a burst by definition, and the sub-second fields prove they are two frames
    ///    rather than one frame counted twice.
    /// 3. **Under [`DRIVE_INTERVAL_MS`] apart** - the inferred fallback, for the
    ///    majority of bodies that write neither tag.
    ///
    /// Returns `false` across cameras: two bodies firing 100 ms apart is two
    /// photographers, and calling that a burst would produce a "burst" no camera took.
    #[must_use]
    pub fn drive_pair(left: &Shot, right: &Shot) -> bool {
        if left.camera != right.camera {
            return false;
        }
        let gap = (left.ts - right.ts).abs();
        if left.drive.continuous && right.drive.continuous && gap <= MAX_WINDOW_MS {
            return true;
        }
        if let (Some(a), Some(b)) = (left.drive.sub_sec, right.drive.sub_sec) {
            if left.ts / 1_000 == right.ts / 1_000 && a != b {
                return true;
            }
        }
        gap <= DRIVE_INTERVAL_MS
    }

    /// True when a gap is long enough to break a group whatever the frames look like.
    #[must_use]
    pub const fn is_hard_gap(gap_ms: i64) -> bool {
        gap_ms > HARD_GAP_MS
    }
}

/// Gaps between consecutive frames, in milliseconds. One shorter than `times`.
fn intervals_of(times: &[Timestamp]) -> Vec<i64> {
    times
        .windows(2)
        .map(|pair| match pair {
            [first, second] => (second - first).max(0),
            _ => 0,
        })
        .collect()
}

/// The window in force at each frame, from the rolling median of its neighbourhood.
///
/// A two-pointer sweep rather than a median over a re-sorted slice per frame: the
/// neighbourhood moves monotonically, so the same window is entered and left once each.
/// The median inside it is taken by copying the slice - which is the honest cost, and
/// is bounded by how many frames fit in sixty seconds, not by the project size.
///
/// The distinction matters at 12,000 frames: a per-frame sort over the whole project
/// would be the one part of this phase that is not linear-ish, which section 9's PERF
/// task exists to prevent.
fn rolling_windows(times: &[Timestamp], intervals: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(times.len());
    let half = NEIGHBOURHOOD_MS / 2;
    let mut low = 0usize;
    let mut high = 0usize;

    for (index, at) in times.iter().enumerate() {
        // Intervals are indexed by their left-hand frame, so the neighbourhood of
        // interval `i` is the neighbourhood of `times[i]`.
        while low < intervals.len() && times.get(low).is_some_and(|t| *t < at - half) {
            low += 1;
        }
        while high < intervals.len() && times.get(high).is_some_and(|t| *t <= at + half) {
            high += 1;
        }
        let mut local: Vec<i64> = intervals.get(low..high).unwrap_or_default().to_vec();
        let median_ms = if local.is_empty() {
            // A frame with no neighbour inside a minute. The widest window, for the
            // reason `window_ms` gives for an unknown camera: an isolated frame should
            // not be prevented from joining something, only from joining the wrong
            // thing, and the similarity threshold does the second part.
            MAX_WINDOW_MS
        } else {
            median(&mut local)
        };
        let scaled = (median_ms as f32 * CADENCE_MULTIPLIER) as i64;
        out.push(scaled.clamp(MIN_WINDOW_MS, MAX_WINDOW_MS));
        debug_assert_eq!(out.len(), index + 1);
    }
    out
}

/// The median of a slice, sorting it in place. Zero for an empty slice.
///
/// The lower median for an even count, deliberately: a burst's interval distribution is
/// heavily right-skewed - many 100 ms gaps and a few multi-second ones - and averaging
/// the two central values lets a single long gap widen the window for the whole burst.
fn median(values: &mut [i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0)
}

/// Which camera handle a body's catalog id maps to inside one grouping pass.
///
/// Dense and per pass, exactly as `aura_index`'s is and for the same reason: comparing
/// 68-character text ids per candidate pair costs more than the distance computation it
/// guards. The catalog ids are kept alongside so a stored [`CameraId`] can be written
/// back out; see [`CameraTable::label`].
#[derive(Debug, Clone, Default)]
pub struct CameraTable {
    labels: Vec<String>,
}

impl CameraTable {
    /// Build the table from every body id seen in a project, sorted as text.
    ///
    /// Sorted, so the same wedding produces the same handles on every machine.
    /// Invariant 4.
    #[must_use]
    pub fn new(mut ids: Vec<String>) -> Self {
        ids.sort();
        ids.dedup();
        Self { labels: ids }
    }

    /// The dense handle for a catalog camera id, adding it if it is new.
    ///
    /// Adding rather than refusing, because a body that appears only in one frame is
    /// still a body and refusing would silently merge it with another.
    pub fn handle(&mut self, id: &str) -> u32 {
        if let Some(found) = self.labels.iter().position(|label| label == id) {
            return found as u32;
        }
        self.labels.push(id.to_string());
        (self.labels.len() - 1) as u32
    }

    /// The catalog id behind a handle.
    #[must_use]
    pub fn label(&self, handle: u32) -> CameraId {
        self.labels
            .get(handle as usize)
            .map_or_else(|| CameraId::new(CameraId::UNKNOWN), CameraId::new)
    }

    /// How many bodies the table knows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    /// True when no body has been seen.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}
