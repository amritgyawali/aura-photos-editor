//! Which frame represents a chapter in the strip.
//!
//! Section 6.2: "key-frame per segment = medoid embedding among frames with high
//! `subject_focus_score` and at least one primary identity."
//!
//! Three conditions, and they are applied as a **filter then a medoid** rather than as
//! a weighted score. The reason is what a key frame is for: it is the one image a
//! photographer sees for a chapter, and a weighted score would happily return the
//! sharpest frame of an empty room because it beat a slightly soft frame of the couple
//! on two terms out of three.
//!
//! So: eligibility is a gate, and among the eligible frames the medoid wins.
//!
//! ## Why the medoid and not the best
//!
//! The medoid is the frame closest to every other frame in the chapter - the most
//! *typical* one. The sharpest frame of a dance floor is a flash-lit portrait of one
//! person; the medoid is a dance floor. A chapter strip built from superlatives
//! misrepresents what is in the chapter, which is the one job it has.
//!
//! Phase 05's `aura_index` already computes medoids for its clusters, and this is the
//! same idea applied to a different grouping. It is not shared code because the inputs
//! differ - that one works over an index's entries, this one over a run of a timeline
//! with an eligibility filter - and a shared function with two modes would be worse
//! than twenty lines twice.
//!
//! ## Falling back, in order
//!
//! A chapter of detail shots has no primary identity and no subject focus at all, and
//! it still needs a key frame. So the filter relaxes in three documented steps and the
//! last one always succeeds:
//!
//! 1. primary identity present **and** subject focus above the floor;
//! 2. subject focus above the floor;
//! 3. every frame in the chapter.
//!
//! [`KeyFrame::relaxed_to`] records which step produced the answer, because "the
//! Details chapter's cover is arbitrary" is a true and useful thing for the panel to
//! know and a false thing to hide.

use aura_core::PhotoId;

/// Least subject focus a frame needs to be a preferred key frame.
///
/// 0.35 on phase 06's `subject_focus_score`, which is the prominence-weighted sharpness
/// of the faces rather than of the frame. Below it the people in the shot are soft,
/// and a chapter cover with a soft bride is the single most visible quality failure
/// this product could ship.
pub const MIN_SUBJECT_FOCUS: f32 = 0.35;

/// One frame, as the key-frame chooser sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The photograph.
    pub image_id: PhotoId,
    /// Its phase 05 embedding. Empty when it has none.
    pub embedding: Vec<f32>,
    /// Phase 06's prominence-weighted face sharpness, `0..1`.
    pub subject_focus: f32,
    /// True when at least one of the couple is in the frame.
    ///
    /// From `PeopleService`. Phase 06's rule: no phase keeps its own idea of who the
    /// couple are.
    pub has_primary: bool,
}

/// Which step of the filter produced the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relaxation {
    /// A primary identity, in focus. The answer a wedding chapter should give.
    Preferred,
    /// In focus, nobody in particular. Portraits of guests, a speech.
    FocusOnly,
    /// Anything at all. Details, venue, an unscanned chapter.
    Any,
}

impl Relaxation {
    /// Stable text for the IPC surface and the reason string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preferred => "preferred",
            Self::FocusOnly => "focus_only",
            Self::Any => "any",
        }
    }
}

/// The chosen frame and how it was chosen.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyFrame {
    /// The photograph.
    pub image_id: PhotoId,
    /// Which step of the filter found it.
    pub relaxed_to: Relaxation,
    /// Mean cosine distance from this frame to the rest of the eligible set, `0..1`.
    ///
    /// How typical it is. A high value means the chapter has no centre - which is
    /// usually a chapter that should have been two - and is worth surfacing rather than
    /// hiding behind a confident-looking thumbnail.
    pub spread: f32,
    /// How many frames were eligible at the step that succeeded.
    pub eligible: usize,
}

/// Choose the frame that represents a chapter.
///
/// Returns `None` only for an empty chapter, which the segmenter never produces.
#[must_use]
pub fn choose(candidates: &[Candidate]) -> Option<KeyFrame> {
    if candidates.is_empty() {
        return None;
    }

    for (relaxation, eligible) in [
        (
            Relaxation::Preferred,
            filter(candidates, |frame| {
                frame.has_primary && frame.subject_focus >= MIN_SUBJECT_FOCUS
            }),
        ),
        (
            Relaxation::FocusOnly,
            filter(candidates, |frame| frame.subject_focus >= MIN_SUBJECT_FOCUS),
        ),
        (Relaxation::Any, candidates.iter().collect()),
    ] {
        if eligible.is_empty() {
            continue;
        }
        let (image_id, spread) = medoid(&eligible);
        return Some(KeyFrame {
            image_id,
            relaxed_to: relaxation,
            spread,
            eligible: eligible.len(),
        });
    }
    None
}

/// Every candidate satisfying a predicate.
fn filter(candidates: &[Candidate], keep: impl Fn(&Candidate) -> bool) -> Vec<&Candidate> {
    candidates.iter().filter(|frame| keep(frame)).collect()
}

/// The most typical frame of a set, and how spread out the set is.
///
/// Exact, over the whole set. A chapter is at most a few hundred frames, so the
/// quadratic pass is tens of thousands of dot products - microseconds - and an
/// approximation would buy nothing and be one more thing that could differ between two
/// machines.
///
/// Frames with no embedding are still *eligible* - they are photographs in the
/// chapter - but they cannot be compared, so they land at the end of the ordering and
/// win only when nothing in the chapter has an embedding at all. That is the correct
/// behaviour for a project mid-way through its phase 05 pass.
fn medoid(candidates: &[&Candidate]) -> (PhotoId, f32) {
    let fallback = candidates
        .first()
        .map_or_else(PhotoId::new, |frame| frame.image_id);

    let embedded: Vec<&&Candidate> = candidates
        .iter()
        .filter(|frame| !frame.embedding.is_empty())
        .collect();
    if embedded.is_empty() {
        return (fallback, 0.0);
    }
    if embedded.len() == 1 {
        return (
            embedded.first().map_or(fallback, |frame| frame.image_id),
            0.0,
        );
    }

    let mut best = fallback;
    let mut best_mean = f32::INFINITY;
    for frame in &embedded {
        let mut total = 0.0f32;
        for other in &embedded {
            if other.image_id == frame.image_id {
                continue;
            }
            total += crate::story::changepoint::cosine_distance(&frame.embedding, &other.embedding);
        }
        let mean = total / (embedded.len().saturating_sub(1)) as f32;
        // Strictly less, so a tie keeps the earlier frame and two machines agree.
        if mean < best_mean {
            best_mean = mean;
            best = frame.image_id;
        }
    }
    (best, best_mean.clamp(0.0, 1.0))
}
