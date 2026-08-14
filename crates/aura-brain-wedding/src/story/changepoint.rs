//! PELT change-point detection over a fused signal, with the penalty searched.
//!
//! Section 6.2, implemented literally, and the one decision the phase document leaves
//! open is settled in ADR-0015 section 6: **the penalty is searched, not fixed.**
//!
//! ## Why the penalty cannot be a constant
//!
//! A PELT penalty trades segments against fit. Tuned on a ten-hour Hindu wedding it
//! produces two chapters for a ninety-minute registry office and forty for a three-day
//! Nepali wedding. Section 10.1 requires *every* wedding to land between 6 and 20
//! chapters, so the penalty is a free parameter the segmenter solves for: bisect over
//! it until the chapter count is inside [`CHAPTER_BAND`].
//!
//! That is not a hack around a mistuned constant. Chapter count is monotone
//! non-increasing in the penalty - a higher penalty never adds a boundary - so the
//! bisection is well-posed and terminates. Nine to twelve iterations bound it.
//!
//! ## The signal
//!
//! Three terms, and each one covers a case the others miss:
//!
//! 1. **Embedding distance** between consecutive frames, normalised by the time gap.
//!    Section 6.2's primary signal. Catches the room changing.
//! 2. **Chapter disagreement** from the smoothed posteriors. Catches the ceremony
//!    ending in the same room with the same light - which the embedding barely sees and
//!    which is the boundary a photographer most cares about.
//! 3. **Time gap**, log-scaled. Catches travel.
//!
//! Fused, not chosen between, because the three fail on different weddings.
//!
//! ## Three hard rules that are not tuning
//!
//! * A gap above [`HARD_GAP_MS`] is a boundary whatever the signal says. Photographers
//!   travel between locations, and twenty minutes with no frame is not a lull.
//! * A segment shorter than [`MIN_SEGMENT_MS`] **or** thinner than
//!   [`MIN_SEGMENT_FRAMES`] merges into its nearest neighbour - unless its dominant
//!   chapter differs, which is the `kiss` case: eight frames in ninety seconds that
//!   nobody wants merged into the ceremony around them.
//! * The band is a band. A wedding that cannot be made to land inside it gets
//!   `AURA-ML-5026` and gap-only segmentation, which is honest and still usable.

use aura_core::contract::scene::ChapterId;

use crate::story::hmm::Smoothed;

/// A time gap above which a boundary is forced, in milliseconds. Twenty minutes.
pub const HARD_GAP_MS: i64 = 20 * 60 * 1000;

/// The shortest a segment may be before the merge pass absorbs it. Ninety seconds.
pub const MIN_SEGMENT_MS: i64 = 90 * 1000;

/// The fewest frames a segment may hold before the merge pass absorbs it.
pub const MIN_SEGMENT_FRAMES: usize = 8;

/// How different a short segment's chapter must be to survive the merge pass.
///
/// Support is the share of a frame's posterior mass on its chapter, so this is a
/// statement about the *classifier's* conviction rather than about duration: a
/// forty-second run of frames the head is sure are `kiss` is a chapter, and a
/// forty-second run it is ambivalent about is noise.
pub const DISTINCT_POSTERIOR_MARGIN: f32 = 0.25;

/// How many chapters a wedding must end up with. Section 10.1.
pub const CHAPTER_BAND: (usize, usize) = (6, 20);

/// The penalty range the search covers.
///
/// The lower bound produces far too many boundaries and the upper bound produces one
/// segment on any real wedding; the answer is always between them, and stating the
/// bounds is what makes `AURA-ML-5026`'s message useful - it reports what each end
/// produced.
///
/// Five decades wide, and it has to be. The PELT cost is the within-run variance of a
/// signal that is near zero almost everywhere and spikes at a chapter change, so the
/// absolute cost of a boundary depends on how many chapter changes a wedding has: a
/// ten-chapter Hindu wedding settles near 0.07 and a wedding with several consecutive
/// same-chapter blocks needs an order of magnitude less. A range that started at 0.05
/// would simply fail on the second kind, which is exactly what
/// `crates/aura-brain-wedding/tests/story.rs` caught.
pub const PENALTY_RANGE: (f32, f32) = (0.000_5, 40.0);

/// How many bisection steps to take.
///
/// **The search is bisected in log space**, and that is not a micro-optimisation. A
/// penalty is a scale parameter: linear bisection of `0.0005..40` spends its first ten
/// steps between 40 and 0.04 and never reaches the bottom two decades at all, so a
/// wedding whose answer is 0.002 falls back for no reason other than arithmetic.
/// Eighteen halvings of a five-decade log range resolve the penalty to about 4 %, which
/// is far finer than the chapter count's sensitivity to it, and the loop exits early
/// when the count lands in the band - so the typical wedding costs four or five.
pub const SEARCH_STEPS: usize = 18;

/// Weight of the embedding term in the fused signal.
pub const W_EMBEDDING: f32 = 0.50;
/// Weight of the chapter-disagreement term.
pub const W_CHAPTER: f32 = 0.35;
/// Weight of the time-gap term.
pub const W_GAP: f32 = 0.15;

/// One frame, as the segmenter sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Timeline time, milliseconds since the Unix epoch.
    pub ts: i64,
    /// The phase 05 embedding. Empty when the frame has none, which contributes
    /// nothing to term 1 rather than contributing a spurious distance of 1.0.
    pub embedding: Vec<f32>,
    /// What the smoother decided.
    pub smoothed: Smoothed,
}

/// A run of consecutive frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// First frame index, inclusive.
    pub start: usize,
    /// Last frame index, inclusive.
    pub end: usize,
}

impl Run {
    /// How many frames.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start) + 1
    }

    /// Never, for a run built by this module; present because clippy asks.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

/// What one segmentation pass did.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SegmentReport {
    /// The penalty the search settled on.
    pub penalty: f32,
    /// Boundaries forced by a time gap.
    pub hard_boundaries: usize,
    /// Boundaries the detector proposed.
    pub detected: usize,
    /// Short or thin runs the merge pass absorbed.
    pub merged: usize,
    /// True when the search never landed inside the band and gap-only was used.
    pub fell_back: bool,
    /// What the lowest penalty produced, for `AURA-ML-5026`.
    pub at_low: (f32, usize),
    /// What the highest penalty produced.
    pub at_high: (f32, usize),
}

/// Split a timeline into chapters.
///
/// Returns the runs in order and what the pass did. Never returns an empty vector for a
/// non-empty input: a wedding with one frame is one chapter.
#[must_use]
pub fn segment(frames: &[Frame]) -> (Vec<Run>, SegmentReport) {
    if frames.is_empty() {
        return (Vec::new(), SegmentReport::default());
    }
    if frames.len() == 1 {
        return (vec![Run { start: 0, end: 0 }], SegmentReport::default());
    }

    let signal = fuse(frames);
    let hard = hard_boundaries(frames);

    let mut report = SegmentReport {
        hard_boundaries: hard.iter().filter(|forced| **forced).count(),
        ..SegmentReport::default()
    };

    // Bisect on the penalty, in log space. Chapter count is monotone non-increasing in
    // it, so a low penalty is the many-chapters end and a high one is the few-chapters
    // end. See `SEARCH_STEPS` for why the bisection is logarithmic.
    let mut low = PENALTY_RANGE.0.ln();
    let mut high = PENALTY_RANGE.1.ln();
    report.at_low = (PENALTY_RANGE.0, run_count(&signal, &hard, PENALTY_RANGE.0));
    report.at_high = (PENALTY_RANGE.1, run_count(&signal, &hard, PENALTY_RANGE.1));

    let mut chosen: Option<(f32, Vec<Run>)> = None;
    for _ in 0..SEARCH_STEPS {
        let middle = f32::midpoint(low, high).exp();
        let runs = merge_short(frames, &pelt(&signal, &hard, middle), &mut 0);
        let count = runs.len();
        if count > CHAPTER_BAND.1 {
            // Too many chapters: raise the penalty.
            low = middle.ln();
        } else if count < CHAPTER_BAND.0 {
            high = middle.ln();
        } else {
            chosen = Some((middle, runs));
            break;
        }
    }

    let Some((penalty, _runs)) = chosen else {
        // The band was never reached. Section 6.2's rules still apply - the hard gaps
        // are real boundaries whatever the signal did - so fall back to them alone,
        // which is deterministic, never over-segments, and is the segmentation a
        // photographer already understands.
        let runs = gaps_only(frames, &hard);
        report.fell_back = true;
        report.detected = runs.len().saturating_sub(1);
        return (runs, report);
    };

    let mut merged = 0usize;
    let runs = merge_short(frames, &pelt(&signal, &hard, penalty), &mut merged);
    report.penalty = penalty;
    report.merged = merged;
    report.detected = runs.len().saturating_sub(1);
    (runs, report)
}

/// The fused change signal between frame `i` and frame `i + 1`.
///
/// One entry shorter than `frames`. Every term is in `0..1`, so the fused value is too,
/// which is what lets one penalty scale mean the same thing on every wedding.
#[must_use]
pub fn fuse(frames: &[Frame]) -> Vec<f32> {
    frames
        .windows(2)
        .map(|pair| {
            let (Some(left), Some(right)) = (pair.first(), pair.get(1)) else {
                return 0.0;
            };
            let gap_ms = right.ts.saturating_sub(left.ts).max(0);

            // 1. Embedding distance, normalised by time. Two frames a second apart that
            //    look different are a change; two frames five minutes apart that look
            //    different are a photographer who moved, which is much weaker evidence
            //    of a *chapter* boundary and is already covered by term 3.
            let raw = cosine_distance(&left.embedding, &right.embedding);
            let seconds = (gap_ms as f32 / 1000.0).max(1.0);
            let embedding = (raw / seconds.sqrt()).clamp(0.0, 1.0);

            // 2. Chapter disagreement. Binary, weighted by how sure both sides are: two
            //    confident frames in different chapters is the strongest boundary
            //    evidence in the product, and two unsure ones is nothing.
            let chapter = if left.smoothed.chapter == right.smoothed.chapter {
                0.0
            } else {
                (left.smoothed.support * right.smoothed.support)
                    .sqrt()
                    .clamp(0.0, 1.0)
            };

            // 3. Time gap, log-scaled against the hard threshold. Log, because the step
            //    from one minute to two matters and the step from eighteen to nineteen
            //    does not.
            let gap = if gap_ms <= 0 {
                0.0
            } else {
                ((gap_ms as f32).ln() / (HARD_GAP_MS as f32).ln()).clamp(0.0, 1.0)
            };

            (W_EMBEDDING * embedding + W_CHAPTER * chapter + W_GAP * gap).clamp(0.0, 1.0)
        })
        .collect()
}

/// Which inter-frame positions are forced boundaries.
#[must_use]
fn hard_boundaries(frames: &[Frame]) -> Vec<bool> {
    frames
        .windows(2)
        .map(|pair| match (pair.first(), pair.get(1)) {
            (Some(left), Some(right)) => right.ts.saturating_sub(left.ts) >= HARD_GAP_MS,
            _ => false,
        })
        .collect()
}

/// PELT over the fused signal at one penalty.
///
/// The cost of a run is its within-run variance about its own mean, which is the
/// standard least-squares cost PELT is derived for; the objective is that cost summed
/// over runs plus `penalty` per boundary.
///
/// This is exact dynamic programming with PELT's pruning rule - a candidate whose best
/// cost plus the penalty already exceeds the current optimum can never become optimal
/// later - which is what makes it linear in practice rather than quadratic. Four
/// thousand frames is under a millisecond, which is how section 11's 2-second
/// segmentation budget is met with room for twelve penalty iterations inside it.
#[must_use]
fn pelt(signal: &[f32], hard: &[bool], penalty: f32) -> Vec<Run> {
    let n = signal.len() + 1;
    if n <= 1 {
        return vec![Run { start: 0, end: 0 }];
    }

    // Prefix sums, so a run's variance is O(1).
    let mut sum = vec![0.0f64; signal.len() + 1];
    let mut sum_sq = vec![0.0f64; signal.len() + 1];
    for (index, value) in signal.iter().enumerate() {
        let previous = sum.get(index).copied().unwrap_or(0.0);
        let previous_sq = sum_sq.get(index).copied().unwrap_or(0.0);
        if let Some(slot) = sum.get_mut(index + 1) {
            *slot = previous + f64::from(*value);
        }
        if let Some(slot) = sum_sq.get_mut(index + 1) {
            *slot = previous_sq + f64::from(*value) * f64::from(*value);
        }
    }
    let cost = |from: usize, to: usize| -> f64 {
        // Cost of signal[from..to].
        if to <= from {
            return 0.0;
        }
        let count = (to - from) as f64;
        let total = sum.get(to).copied().unwrap_or(0.0) - sum.get(from).copied().unwrap_or(0.0);
        let total_sq =
            sum_sq.get(to).copied().unwrap_or(0.0) - sum_sq.get(from).copied().unwrap_or(0.0);
        (total_sq - total * total / count).max(0.0)
    };

    let mut best = vec![f64::INFINITY; n];
    let mut previous = vec![0usize; n];
    if let Some(slot) = best.first_mut() {
        *slot = -f64::from(penalty);
    }
    let mut candidates: Vec<usize> = vec![0];

    for end in 1..n {
        let mut best_cost = f64::INFINITY;
        let mut best_start = 0usize;
        for start in &candidates {
            let value = best.get(*start).copied().unwrap_or(f64::INFINITY)
                + cost(*start, end - 1)
                + f64::from(penalty);
            if value < best_cost {
                best_cost = value;
                best_start = *start;
            }
        }
        if let Some(slot) = best.get_mut(end) {
            *slot = best_cost;
        }
        if let Some(slot) = previous.get_mut(end) {
            *slot = best_start;
        }

        // PELT's pruning rule.
        candidates.retain(|start| {
            best.get(*start).copied().unwrap_or(f64::INFINITY) + cost(*start, end - 1) <= best_cost
        });
        candidates.push(end);
    }

    // Backtrace into runs, then force every hard boundary. Forcing after rather than
    // during is deliberate: PELT's optimum is over the signal, and a travel gap is a
    // fact about the day that no cost function gets a vote on.
    let mut cuts = Vec::new();
    let mut at = n - 1;
    while at > 0 {
        cuts.push(at);
        at = previous.get(at).copied().unwrap_or(0);
    }
    cuts.push(0);
    cuts.reverse();

    let mut boundaries: Vec<usize> = cuts.clone();
    for (index, forced) in hard.iter().enumerate() {
        if *forced {
            boundaries.push(index + 1);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut runs = Vec::new();
    for pair in boundaries.windows(2) {
        let (Some(start), Some(end)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        if end > start {
            runs.push(Run {
                start: *start,
                end: end.saturating_sub(1),
            });
        }
    }
    if runs.is_empty() {
        runs.push(Run {
            start: 0,
            end: n - 1,
        });
    }
    runs
}

/// One run per stretch between hard time gaps, and nothing else.
///
/// The `AURA-ML-5026` fallback. Deterministic, never over-segments, and describes the
/// segmentation photographers already have in their heads: "these are the times I moved
/// location".
#[must_use]
fn gaps_only(frames: &[Frame], hard: &[bool]) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut start = 0usize;
    for (index, forced) in hard.iter().enumerate() {
        if *forced {
            runs.push(Run { start, end: index });
            start = index + 1;
        }
    }
    runs.push(Run {
        start,
        end: frames.len().saturating_sub(1),
    });
    runs
}

/// Absorb runs that are too short or too thin into their nearest neighbour.
///
/// Section 6.2's merge pass. A run survives when it is long enough, thick enough, *or*
/// its chapter is distinct enough - see [`DISTINCT_POSTERIOR_MARGIN`] for why the third
/// exception exists and what it protects.
#[must_use]
fn merge_short(frames: &[Frame], runs: &[Run], merged: &mut usize) -> Vec<Run> {
    if runs.len() <= 1 {
        return runs.to_vec();
    }
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());

    for run in runs {
        let duration = duration_of(frames, *run);
        let long_enough = duration >= MIN_SEGMENT_MS && run.len() >= MIN_SEGMENT_FRAMES;
        let distinct = is_distinct(frames, *run, out.last().copied());

        if long_enough || distinct || out.is_empty() {
            out.push(*run);
            continue;
        }
        // Absorb into the run before it, which is always the nearest neighbour in a
        // forward walk: a run after this one has not been examined and may itself be
        // absorbed, and merging into something that then disappears would lose frames.
        if let Some(previous) = out.last_mut() {
            previous.end = run.end;
            *merged += 1;
        }
    }

    out
}

/// How long a run lasted.
fn duration_of(frames: &[Frame], run: Run) -> i64 {
    let start = frames.get(run.start).map_or(0, |frame| frame.ts);
    let end = frames.get(run.end).map_or(start, |frame| frame.ts);
    end.saturating_sub(start)
}

/// True when a short run's chapter is different enough from its neighbour to keep.
fn is_distinct(frames: &[Frame], run: Run, previous: Option<Run>) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let mine = dominant(frames, run);
    let theirs = dominant(frames, previous);
    if mine.0 != theirs.0 {
        return mine.1 >= DISTINCT_POSTERIOR_MARGIN;
    }
    false
}

/// A run's most common chapter and the mean support for it.
#[must_use]
pub fn dominant(frames: &[Frame], run: Run) -> (ChapterId, f32) {
    let mut counts = [0u32; ChapterId::COUNT];
    let mut support = [0.0f32; ChapterId::COUNT];
    let mut total = 0u32;

    for frame in frames.iter().skip(run.start).take(run.len()) {
        let index = frame.smoothed.chapter.as_index();
        if let Some(slot) = counts.get_mut(index) {
            *slot += 1;
        }
        if let Some(slot) = support.get_mut(index) {
            *slot += frame.smoothed.support;
        }
        total += 1;
    }
    if total == 0 {
        return (ChapterId::Other, 0.0);
    }

    let mut best = 0usize;
    let mut best_count = 0u32;
    for (index, count) in counts.iter().enumerate() {
        // Strictly greater, so a tie keeps the earlier chapter in wedding order and two
        // machines agree. Invariant 4.
        if *count > best_count {
            best_count = *count;
            best = index;
        }
    }
    let mean = if best_count == 0 {
        0.0
    } else {
        support.get(best).copied().unwrap_or(0.0) / best_count as f32
    };
    (ChapterId::from_index(best), mean.clamp(0.0, 1.0))
}

/// How many runs one penalty produces. Used only by the search.
fn run_count(signal: &[f32], hard: &[bool], penalty: f32) -> usize {
    pelt(signal, hard, penalty).len()
}

/// Cosine distance between two vectors, in `0..1`.
///
/// Zero when either is empty, which is the "this frame has no embedding" case: it must
/// contribute *nothing* to the change signal rather than a spurious maximum, or a
/// project that is 60 % embedded would show a boundary at every unembedded frame.
#[must_use]
pub fn cosine_distance(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut left_sq = 0.0f32;
    let mut right_sq = 0.0f32;
    for (a, b) in left.iter().zip(right.iter()) {
        dot += a * b;
        left_sq += a * a;
        right_sq += b * b;
    }
    if left_sq <= 0.0 || right_sq <= 0.0 {
        return 0.0;
    }
    let similarity = dot / (left_sq.sqrt() * right_sq.sqrt());
    ((1.0 - similarity) / 2.0).clamp(0.0, 1.0)
}
