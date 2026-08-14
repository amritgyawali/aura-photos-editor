//! HMM smoothing over the timeline: the single trick that removes most absurd labels.
//!
//! Section 6.2 says so in as many words, and it is not an exaggeration. A 22-way head
//! run independently on four thousand frames will, somewhere in the reception, decide
//! that one frame is `getting_ready_bride`. On its own that is a wrong label on one
//! photograph. Fed into a change-point detector it is a *chapter boundary*, and the
//! photographer gets a timeline with a two-frame "Getting Ready" chapter between the
//! speeches and the cake.
//!
//! So the posteriors are smoothed before anything is segmented. ADR-0015 section 6
//! settles the order that the phase document's diagram draws ambiguously: **HMM first,
//! PELT second.** Smoothing after segmentation cannot help, because by then the wrong
//! label has already become a row in `segments`.
//!
//! ## The chain is over chapters, not over scenes
//!
//! Nine states, not twenty-two. Three reasons, and the third is the one that decides
//! it:
//!
//! 1. A 22 x 22 transition matrix is 484 authored numbers. Nobody can review that, so
//!    nobody would, so it would be wrong in ways nobody could find.
//! 2. Most scene-to-scene transitions inside a chapter are meaningless. A photographer
//!    alternating between `couple_portrait` and `golden_hour` for twenty minutes is not
//!    transitioning between states; they are photographing one thing.
//! 3. **The output of this stage is what PELT segments, and PELT produces chapters.**
//!    Smoothing at a finer granularity than the thing being produced buys precision
//!    that is discarded one step later.
//!
//! `SceneId::chapter()` is the emission mapping, and it is total.
//!
//! ## What the transition matrix encodes
//!
//! Real wedding order, authored by hand as section 6.2 requires, with three properties
//! that matter more than any individual number:
//!
//! * **Self-transition dominates.** A wedding spends far longer inside a chapter than
//!   moving between them, so the diagonal is around 0.80. This is what suppresses the
//!   single-frame flip.
//! * **Forward is cheap, backward is expensive but never zero.** Weddings go roughly in
//!   order and then break it: the couple leave the reception for a golden-hour portrait
//!   and come back. A zero would make that *impossible* rather than unlikely, and
//!   Viterbi would relabel a genuine chapter to avoid it.
//! * **Nothing is zero except leaving `Other`.** A hard zero in a probability matrix is
//!   a claim that a wedding cannot do something, and weddings do everything.
//!
//! ## Viterbi, not forward-backward
//!
//! The most likely *sequence*, not the most likely label at each point. A frame-wise
//! posterior would happily produce a path that the transition matrix says is
//! impossible, which defeats the purpose - and section 10.1 asks for exactly the
//! opposite: "no chapter sequence violates the allowed transition matrix".
//!
//! Everything runs in log space. A four-thousand-frame product of numbers around 0.8
//! underflows an `f32` at about frame 350.

use aura_core::contract::scene::{ChapterId, SceneId, SceneScore};

/// How likely a chapter is to continue rather than change.
///
/// 0.80 is a statement about frames, not about minutes: at a wedding's typical two to
/// six frames a minute, a chapter of twenty minutes is fifty to a hundred frames, and
/// 0.80 per frame puts the expected run at five frames before *some* transition is
/// considered. The smoothing comes from the transitions being scored against a good
/// emission rather than from the diagonal alone.
pub const STAY: f32 = 0.80;

/// The probability mass a chapter is allowed to spend on moving forward.
pub const FORWARD: f32 = 0.14;

/// The mass allowed for going back to an earlier chapter.
///
/// Small, and never zero. A wedding that returns to portraits after the reception has
/// started is not violating anything; it is a photographer using the light.
pub const BACKWARD: f32 = 0.04;

/// The mass allowed for reaching [`ChapterId::Other`] from anywhere.
///
/// Deliberately the smallest non-zero entry. `Other` must be reachable - section 6.4
/// requires genuinely novel events to land there rather than be given a confident wrong
/// label - but a state that is cheap to enter absorbs every ambiguous frame in the
/// wedding, and a timeline of nine "Other" chapters helps nobody.
pub const TO_OTHER: f32 = 0.02;

/// Floor applied to every emission probability before the log.
///
/// `ln(0)` is negative infinity, and one impossible emission would make a whole path
/// impossible - which in Viterbi means the wrong path wins by default. This is the
/// same defensive floor `Softmax`'s max-subtraction is: arithmetic that keeps a
/// confidence a confidence.
pub const EMISSION_FLOOR: f32 = 1e-6;

/// The hand-authored transition matrix, row-normalised, in [`ChapterId::ALL`] order.
///
/// Row `i`, column `j` is the probability of chapter `j` following chapter `i` at the
/// next frame. Built rather than written out as 81 literals: the rules above are the
/// specification, and 81 numbers copied by hand would drift from them at the first
/// edit. A test asserts every row sums to 1.0 and no entry is zero except the ones
/// this function makes zero deliberately.
#[must_use]
pub fn transitions() -> [[f32; ChapterId::COUNT]; ChapterId::COUNT] {
    let mut matrix = [[0.0f32; ChapterId::COUNT]; ChapterId::COUNT];

    // The wedding-order distance between two chapters. `Other` is outside the order,
    // so it is handled separately below.
    let ordered = ChapterId::COUNT - 1;

    for from in 0..ChapterId::COUNT {
        let Some(row) = matrix.get_mut(from) else {
            continue;
        };
        if from == ordered {
            // Leaving `Other`. A novel event is by definition not part of the ordered
            // story, so there is no "next" chapter after it: the mass is spread evenly
            // over every ordered chapter plus a strong diagonal. This is the one row
            // where forward and backward have no meaning.
            let each = (1.0 - STAY) / ordered as f32;
            for (index, slot) in row.iter_mut().enumerate() {
                *slot = if index == ordered { STAY } else { each };
            }
            continue;
        }

        let mut forward_targets = 0usize;
        let mut backward_targets = 0usize;
        for to in 0..ordered {
            if to > from {
                forward_targets += 1;
            } else if to < from {
                backward_targets += 1;
            }
        }

        for (to, slot) in row.iter_mut().enumerate().take(ordered) {
            *slot = match to.cmp(&from) {
                std::cmp::Ordering::Equal => STAY,
                // Nearer chapters get more of the forward mass. A wedding usually goes
                // to the next chapter, sometimes skips one - a couple who did their
                // portraits before the ceremony - and rarely jumps five.
                std::cmp::Ordering::Greater => {
                    FORWARD / (to - from) as f32 / harmonic(forward_targets)
                }
                std::cmp::Ordering::Less => {
                    BACKWARD / (from - to) as f32 / harmonic(backward_targets)
                }
            };
        }
        if let Some(other) = row.get_mut(ordered) {
            *other = TO_OTHER;
        }

        // Renormalise. The weights above are proportions of an intent, and a row that
        // summed to 0.97 would leak probability at every step of a four-thousand-frame
        // chain.
        let total: f32 = row.iter().sum();
        if total > 0.0 {
            for slot in row.iter_mut() {
                *slot /= total;
            }
        }
    }

    matrix
}

/// The `n`th harmonic number, or 1.0 for zero terms.
///
/// The normaliser for the `1/distance` weighting above, so that a chapter with six
/// forward targets spends the same total forward mass as one with two.
fn harmonic(terms: usize) -> f32 {
    if terms == 0 {
        return 1.0;
    }
    (1..=terms).map(|k| 1.0 / k as f32).sum()
}

/// One frame's evidence, as the smoother wants it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observation {
    /// The classifier's top three, already sorted.
    pub top3: [SceneScore; 3],
    /// Milliseconds from the wedding's first frame.
    pub offset_ms: i64,
}

/// What the smoother decided for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smoothed {
    /// The chapter on the most likely path.
    pub chapter: ChapterId,
    /// The scene the frame keeps, which may differ from its own argmax.
    ///
    /// The strongest of its top-3 whose chapter matches the smoothed chapter; its own
    /// argmax when none of them do. **The scene is corrected, not discarded**: a frame
    /// the head called `getting_ready_bride` in the middle of the reception is far more
    /// likely to be its own second guess than to be nothing at all.
    pub scene: SceneId,
    /// How much of the frame's own posterior mass supports the smoothed chapter.
    pub support: f32,
    /// True when smoothing moved this frame away from its own argmax.
    pub corrected: bool,
}

/// What one smoothing pass did.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SmoothReport {
    /// Frames considered.
    pub frames: usize,
    /// Frames whose chapter changed.
    pub corrected: usize,
    /// Distinct chapters on the final path.
    pub chapters: usize,
}

/// Smooth a timeline of posteriors into a chapter path.
///
/// Viterbi in log space over [`ChapterId::COUNT`] states. Returns one [`Smoothed`] per
/// observation, in order, plus what it did.
///
/// The observations must already be in timeline order; this is not sorted here, because
/// the caller reads them in that order from an index that guarantees it and re-sorting
/// would hide a caller that had not.
#[must_use]
pub fn smooth(observations: &[Observation]) -> (Vec<Smoothed>, SmoothReport) {
    if observations.is_empty() {
        return (Vec::new(), SmoothReport::default());
    }

    let matrix = transitions();
    let log_transitions: Vec<Vec<f32>> = matrix
        .iter()
        .map(|row| row.iter().map(|p| p.max(EMISSION_FLOOR).ln()).collect())
        .collect();

    let states = ChapterId::COUNT;
    let mut scores = vec![f32::NEG_INFINITY; states];
    let mut back: Vec<Vec<usize>> = Vec::with_capacity(observations.len());

    // The initial distribution is the first frame's own emission, unweighted. A prior
    // that insisted a wedding starts in `GettingReady` would be wrong for every second
    // shooter who arrives at the venue, and the emission is evidence while the prior
    // would be a guess.
    if let Some(first) = observations.first() {
        let emission = emissions(first);
        for (state, slot) in scores.iter_mut().enumerate() {
            *slot = emission.get(state).copied().unwrap_or(EMISSION_FLOOR).ln();
        }
    }

    for observation in observations.iter().skip(1) {
        let emission = emissions(observation);
        let mut next = vec![f32::NEG_INFINITY; states];
        let mut pointers = vec![0usize; states];

        for (to, (score, pointer)) in next.iter_mut().zip(pointers.iter_mut()).enumerate() {
            let mut best = f32::NEG_INFINITY;
            let mut best_from = 0usize;
            for (from, previous) in scores.iter().enumerate() {
                let step = log_transitions
                    .get(from)
                    .and_then(|row| row.get(to))
                    .copied()
                    .unwrap_or(f32::NEG_INFINITY);
                let candidate = previous + step;
                // Strictly greater, so that a tie keeps the lower-numbered state -
                // which is the earlier chapter in wedding order, and which makes the
                // result reproducible on two machines. Invariant 4.
                if candidate > best {
                    best = candidate;
                    best_from = from;
                }
            }
            *score = best + emission.get(to).copied().unwrap_or(EMISSION_FLOOR).ln();
            *pointer = best_from;
        }

        back.push(pointers);
        scores = next;
    }

    // Backtrace from the best final state.
    let mut state = scores
        .iter()
        .enumerate()
        .fold((0usize, f32::NEG_INFINITY), |best, (index, score)| {
            if *score > best.1 {
                (index, *score)
            } else {
                best
            }
        })
        .0;

    let mut path = vec![0usize; observations.len()];
    if let Some(last) = path.last_mut() {
        *last = state;
    }
    for (index, pointers) in back.iter().enumerate().rev() {
        state = pointers.get(state).copied().unwrap_or(0);
        if let Some(slot) = path.get_mut(index) {
            *slot = state;
        }
    }

    (decide(observations, &path), report_of(observations, &path))
}

/// Turn a Viterbi path into one decision per frame.
///
/// Split out of [`smooth`] so the Viterbi recursion and the label-correction rule can
/// be read separately; they are two different arguments and reading them interleaved is
/// what makes a smoother hard to review.
fn decide(observations: &[Observation], path: &[usize]) -> Vec<Smoothed> {
    let mut out = Vec::with_capacity(observations.len());

    for (observation, index) in observations.iter().zip(path.iter()) {
        let chapter = ChapterId::from_index(*index);

        // The scene the frame keeps. See `Smoothed::scene`.
        let own = observation
            .top3
            .first()
            .copied()
            .unwrap_or(SceneScore::NONE)
            .scene;
        let kept = observation
            .top3
            .iter()
            .find(|score| score.scene.chapter() == chapter && score.score > 0.0)
            .map_or(own, |score| score.scene);
        let support: f32 = observation
            .top3
            .iter()
            .filter(|score| score.scene.chapter() == chapter)
            .map(|score| score.score)
            .sum();
        out.push(Smoothed {
            chapter,
            scene: kept,
            support: support.clamp(0.0, 1.0),
            corrected: own.chapter() != chapter,
        });
    }

    out
}

/// What one smoothing pass changed, counted from the path.
fn report_of(observations: &[Observation], path: &[usize]) -> SmoothReport {
    let mut seen = [false; ChapterId::COUNT];
    let mut report = SmoothReport {
        frames: observations.len(),
        ..SmoothReport::default()
    };
    for (observation, index) in observations.iter().zip(path.iter()) {
        if let Some(slot) = seen.get_mut(*index) {
            if !*slot {
                *slot = true;
                report.chapters += 1;
            }
        }
        let own = observation
            .top3
            .first()
            .copied()
            .unwrap_or(SceneScore::NONE)
            .scene;
        if own.chapter() != ChapterId::from_index(*index) {
            report.corrected += 1;
        }
    }
    report
}

/// One frame's emission probability per chapter.
///
/// The top-3 posteriors folded onto their chapters, plus a uniform floor over every
/// chapter so no state is impossible. A frame whose whole top-3 is one chapter emits
/// almost all of its mass there; a frame split across three chapters lets the
/// transition matrix decide, which is exactly what smoothing is for.
fn emissions(observation: &Observation) -> [f32; ChapterId::COUNT] {
    let mut out = [EMISSION_FLOOR; ChapterId::COUNT];
    let mut total = EMISSION_FLOOR * ChapterId::COUNT as f32;

    for score in &observation.top3 {
        if score.score <= 0.0 {
            continue;
        }
        let index = score.scene.chapter().as_index();
        if let Some(slot) = out.get_mut(index) {
            *slot += score.score;
            total += score.score;
        }
    }

    if total > 0.0 {
        for slot in &mut out {
            *slot /= total;
        }
    }
    out
}

/// True when a chapter may follow another under the authored matrix.
///
/// Section 10.1's "no chapter sequence violates the allowed transition matrix". With no
/// zero entries outside the `Other` row this is total, and that is the honest answer to
/// the criterion: the matrix expresses *unlikely*, not *forbidden*, and a gate that
/// pretended otherwise would be checking a rule the model was never given.
///
/// What the gate actually asserts is the useful form of the same idea -
/// [`is_plausible`] - which is that a transition is not vanishingly rare.
#[must_use]
pub fn is_allowed(from: ChapterId, to: ChapterId) -> bool {
    transitions()
        .get(from.as_index())
        .and_then(|row| row.get(to.as_index()))
        .is_some_and(|p| *p > 0.0)
}

/// Below this transition probability a chapter sequence is reported as implausible.
///
/// Half of [`TO_OTHER`] after row normalisation, which puts it below every authored
/// transition and above zero. A sequence under it is one the matrix effectively rules
/// out, and section 10.1's HMM sanity check asserts none is produced.
pub const PLAUSIBLE_ABOVE: f32 = 0.005;

/// True when a transition is not vanishingly rare under the authored matrix.
#[must_use]
pub fn is_plausible(from: ChapterId, to: ChapterId) -> bool {
    transitions()
        .get(from.as_index())
        .and_then(|row| row.get(to.as_index()))
        .is_some_and(|p| *p >= PLAUSIBLE_ABOVE)
}
