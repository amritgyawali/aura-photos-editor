//! Which frames inside a moment are the same photograph.
//!
//! PHASE-08 section 6.3, and the single most consequential claim this phase makes.
//! Calling two frames `NearIdentical` means at most one of them may ever reach a
//! gallery; calling them `Variant` means phase 12 gets to choose between them. Getting
//! the first wrong loses a photograph nobody knew existed, and getting the second wrong
//! sends a client two copies of one picture.
//!
//! ## The three classes, and the exact tests
//!
//! ```text
//! identical       same content hash                        (phase 01, at ingest)
//! near_identical  dhash <= 4  AND  embed <= 0.03  AND  face IoU >= 0.9
//! variant         in the same moment, and not the above
//! ```
//!
//! Section 6.3's thresholds, unchanged. **All three conditions, not any of them.** The
//! conjunction is the whole design: a difference hash is blind to a blink, an embedding
//! is blind to a one-stop exposure change, and the face-box overlap is blind to
//! everything except where people are standing. Three blind tests that must all agree
//! is a far stronger claim than one confident one, and section 10.1 asks for recall
//! 0.98 at precision 0.95 - which is a demand for a test that is almost never wrong in
//! the direction that suppresses a frame.
//!
//! ## The one-stop-apart case, which section 10.1 names
//!
//! Two frames of the same instant, one stop apart, are the same photograph: a
//! photographer bracketed, and only one of the two is delivered. A difference hash is
//! largely exposure-invariant by construction - it compares neighbouring pixels rather
//! than absolute levels - so it says "the same frame" and the embedding, computed on
//! normalised pixels, agrees. This case is a regression fixture rather than a special
//! case in the code, and `bracketed_exposures_are_near_identical` is where it lives.
//!
//! ## `keep_hint` is not a decision
//!
//! Section 6.3: "provisional: the technically strongest frame by edge energy and
//! subject focus, replaced by the real decision in Phase 12". Nothing in this crate
//! reads it back, and section 2.2 puts every question about a photograph's fate in
//! phase 12. It exists so the review panel has something to put on the left.

use aura_core::contract::moment::{DuplicateKind, DuplicateSet, ImageId};

use crate::moments::graph::Frame;

/// Section 6.3's difference-hash bound for `near_identical`, in bits.
pub const NEAR_DHASH_MAX: u32 = 4;

/// Section 6.3's embedding bound for `near_identical`, as a cosine distance.
pub const NEAR_EMBED_MAX: f32 = 0.03;

/// Section 6.3's face-box overlap bound for `near_identical`.
///
/// Applied as "the people are in the same places": the mean intersection-over-union of
/// the matched face boxes. When neither frame has a face box - a detail shot, a venue,
/// or any wedding where phase 06 has not run - the test is **skipped rather than
/// failed**, and the set records that it was skipped in its reasons.
///
/// Skipping is the right direction. Requiring a face overlap on a photograph of a ring
/// would mean no two frames of a ring were ever near identical, which would send every
/// bracketed detail shot to a client.
pub const NEAR_FACE_IOU_MIN: f32 = 0.9;

/// How many frames a set may hold before its reasons stop naming frames.
///
/// A set of forty near-identical frames is real - a photographer held the button - and
/// listing forty ids in a reason string produces a sentence nobody reads and a row that
/// blows the storage budget.
pub const REASON_FRAME_CAP: usize = 3;

/// What the caller knows about where the faces are, when it knows anything.
///
/// Deliberately not a face box, a template or a crop: this is the mean overlap phase 06
/// already computed, passed in as a number. A phase that groups photographs has no
/// business holding biometric data, and this signature is what keeps that structural
/// rather than advisory - the same rule `FaceRef` states for phase 06's own callers.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FaceOverlap {
    /// Mean intersection-over-union of the matched boxes, `0..1`.
    pub iou: f32,
    /// True when both frames had at least one face box to compare.
    ///
    /// When false the `iou` is meaningless and the test is skipped; see
    /// [`NEAR_FACE_IOU_MIN`].
    pub measured: bool,
}

/// How two frames of one moment relate, and why.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    /// The class.
    pub kind: DuplicateKind,
    /// How sure, `0..1`.
    pub confidence: f32,
    /// Hamming distance between the two hashes.
    pub dhash_dist: u32,
    /// Cosine distance between the two embeddings.
    pub embed_dist: f32,
    /// True when the face-overlap test was skipped for want of a face.
    pub face_skipped: bool,
}

/// Classify one pair.
///
/// `same_file` is phase 01's answer, passed in rather than recomputed: a content hash
/// is a property of bytes on disk and this crate never opens a file.
#[must_use]
pub fn classify(left: &Frame, right: &Frame, faces: FaceOverlap, same_file: bool) -> Verdict {
    let dhash_dist = (left.dhash ^ right.dhash).count_ones();
    let embed_dist =
        aura_index::contract::index::cosine_distance(&left.embedding, &right.embedding);

    if same_file {
        return Verdict {
            kind: DuplicateKind::Identical,
            // Certain, and it is the only certainty in this module: two files with the
            // same BLAKE3 digest are the same bytes.
            confidence: 1.0,
            dhash_dist,
            embed_dist,
            face_skipped: !faces.measured,
        };
    }

    let hash_agrees = dhash_dist <= NEAR_DHASH_MAX;
    let embed_agrees = embed_dist <= NEAR_EMBED_MAX;
    let faces_agree = !faces.measured || faces.iou >= NEAR_FACE_IOU_MIN;

    if hash_agrees && embed_agrees && faces_agree {
        return Verdict {
            kind: DuplicateKind::NearIdentical,
            confidence: near_confidence(dhash_dist, embed_dist, faces),
            dhash_dist,
            embed_dist,
            face_skipped: !faces.measured,
        };
    }

    Verdict {
        kind: DuplicateKind::Variant,
        // How sure we are that these are *not* the same photograph: the further past
        // any one bound the pair is, the more certain the variant call. Reported the
        // same way round as every other confidence in the product - high means "sure
        // of the label I gave", not "sure they are duplicates".
        confidence: variant_confidence(dhash_dist, embed_dist, faces),
        dhash_dist,
        embed_dist,
        face_skipped: !faces.measured,
    }
}

/// How sure a `near_identical` call is.
///
/// The mean of three margins, each expressed as "how far inside its bound this pair
/// is". A pair at exactly the bounds scores 0.5 rather than 0.0, because a pair that
/// passed all three tests is more likely than not to be what the tests say it is - and
/// a confidence of zero on a positive classification would read as an abstention.
///
/// A skipped face test contributes the neutral 0.5 rather than a free 1.0. Section
/// 10.1's precision gate is about not suppressing frames wrongly, and a set that was
/// never able to check where the people were should say so in its number as well as in
/// its reasons.
fn near_confidence(dhash_dist: u32, embed_dist: f32, faces: FaceOverlap) -> f32 {
    let hash_margin = 1.0 - (dhash_dist as f32 / NEAR_DHASH_MAX.max(1) as f32);
    let embed_margin = 1.0 - (embed_dist / NEAR_EMBED_MAX).clamp(0.0, 1.0);
    let face_margin = if faces.measured {
        ((faces.iou - NEAR_FACE_IOU_MIN) / (1.0 - NEAR_FACE_IOU_MIN)).clamp(0.0, 1.0)
    } else {
        0.5
    };
    (0.5 + 0.5 * (hash_margin + embed_margin + face_margin) / 3.0).clamp(0.0, 1.0)
}

/// How sure a `variant` call is.
///
/// Driven by the single most-exceeded bound rather than by an average, because one
/// decisive difference is enough: two frames whose hashes agree perfectly but whose
/// subjects have moved are variants, and averaging would drown that out.
fn variant_confidence(dhash_dist: u32, embed_dist: f32, faces: FaceOverlap) -> f32 {
    let hash_excess = (dhash_dist as f32 - NEAR_DHASH_MAX as f32).max(0.0) / 16.0;
    let embed_excess = (embed_dist - NEAR_EMBED_MAX).max(0.0) / 0.2;
    let face_excess = if faces.measured {
        (NEAR_FACE_IOU_MIN - faces.iou).max(0.0) / NEAR_FACE_IOU_MIN
    } else {
        0.0
    };
    let worst = hash_excess.max(embed_excess).max(face_excess);
    (0.55 + 0.45 * worst.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

/// The technically strongest frame of a set. Section 6.3's `keep_hint`.
///
/// Subject focus first, edge energy as the tie-break, and the order matters: a frame
/// that is razor sharp on the cake and soft on the bride has high edge energy and low
/// subject focus, and only the second describes whether the photograph is usable. That
/// is phase 06's `subject_focus_score` doing the work it exists for.
///
/// When no face pass has run every frame's subject focus is zero, so the choice falls
/// through to edge energy alone - and the caller records that in a reason, because a
/// hint chosen on global sharpness is a weaker hint and the panel should say so.
///
/// Ties break on the earliest frame, so two machines choose the same one. Invariant 4.
#[must_use]
pub fn keep_hint(frames: &[Frame], members: &[usize]) -> Option<usize> {
    members
        .iter()
        .copied()
        .filter(|index| frames.get(*index).is_some())
        .max_by(|a, b| {
            let (Some(left), Some(right)) = (frames.get(*a), frames.get(*b)) else {
                return std::cmp::Ordering::Equal;
            };
            left.subject_focus
                .total_cmp(&right.subject_focus)
                .then(left.edge_energy.total_cmp(&right.edge_energy))
                // Reversed, so that `max_by` prefers the earlier frame on a full tie.
                .then(b.cmp(a))
        })
}

/// Build the duplicate sets of one moment.
///
/// Sets are transitive closures over the pairs that cap the gallery rather than
/// pairwise rows: if A and B are the same photograph and B and C are, then all three
/// are one set, and a photographer reviewing them should see one panel rather than two.
///
/// ## Only sets that constrain delivery are returned
///
/// A `Variant` pair is "the same moment, meaningfully different", and section 6.3 says
/// of it: "all frames stay eligible and Phase 12 chooses". That is a statement that
/// **nothing is constrained**, and a set expressing it would be a row saying so about
/// data that already says it: the frames of one burst are exactly the alternatives
/// phase 12 chooses between, and `Moment::bursts` is that list.
///
/// So a variant is a *classification outcome* - [`classify`] returns it, the review
/// panel shows it, the telemetry counts it - and not a stored set. Storing them cost
/// 380 bytes per photograph against section 11's 200-byte budget, all of it restating
/// `moment_images.burst_ix`. See ADR-0017 section 7.
///
/// `same_file` is asked per pair rather than precomputed, because a moment holds a
/// handful of frames and the ingest answer is a lookup.
#[must_use]
pub fn sets_of_moment(
    frames: &[Frame],
    members: &[usize],
    bursts: &[Vec<usize>],
    faces: &dyn Fn(usize, usize) -> FaceOverlap,
    same_file: &dyn Fn(usize, usize) -> bool,
) -> Vec<DuplicateSet> {
    if members.len() < 2 {
        return Vec::new();
    }

    let mut union = crate::moments::graph::UnionFind::new(members.len());
    let position: std::collections::BTreeMap<usize, usize> = members
        .iter()
        .enumerate()
        .map(|(local, global)| (*global, local))
        .collect();
    let mut verdicts: std::collections::BTreeMap<(usize, usize), Verdict> =
        std::collections::BTreeMap::new();

    for (offset, left) in members.iter().enumerate() {
        for right in members.iter().skip(offset + 1) {
            let (Some(a), Some(b)) = (frames.get(*left), frames.get(*right)) else {
                continue;
            };
            let verdict = classify(a, b, faces(*left, *right), same_file(*left, *right));
            verdicts.insert((*left, *right), verdict);
            if verdict.kind.caps_gallery_to_one() {
                if let (Some(x), Some(y)) = (position.get(left), position.get(right)) {
                    union.union(*x, *y);
                }
            }
        }
    }

    // `bursts` is not read here, and the signature keeps it because the caller has it
    // and because an earlier draft built variant sets per burst. See this function's
    // header for why that draft is gone.
    let _ = bursts;

    let mut out = Vec::new();
    for local_group in union.groups() {
        if local_group.len() < 2 {
            continue;
        }
        let global: Vec<usize> = local_group
            .iter()
            .filter_map(|local| members.get(*local).copied())
            .collect();
        if let Some(set) = assemble(frames, &global, &verdicts) {
            out.push(set);
        }
    }

    out
}

/// Turn a set of frame indices into a `DuplicateSet` with its reasons.
fn assemble(
    frames: &[Frame],
    members: &[usize],
    verdicts: &std::collections::BTreeMap<(usize, usize), Verdict>,
) -> Option<DuplicateSet> {
    let hint_index = keep_hint(frames, members)?;
    let hint = frames.get(hint_index)?;

    // The set's class and confidence are those of its weakest pair: a set is only as
    // strong a claim as the least certain link that holds it together.
    let mut kind = DuplicateKind::Identical;
    let mut confidence = 1.0f32;
    let mut worst_dhash = 0u32;
    let mut worst_embed = 0.0f32;
    let mut any_face_skipped = false;

    for (offset, left) in members.iter().enumerate() {
        for right in members.iter().skip(offset + 1) {
            let key = if left <= right {
                (*left, *right)
            } else {
                (*right, *left)
            };
            let Some(verdict) = verdicts.get(&key) else {
                continue;
            };
            if verdict.kind > kind {
                kind = verdict.kind;
            }
            confidence = confidence.min(verdict.confidence);
            worst_dhash = worst_dhash.max(verdict.dhash_dist);
            worst_embed = worst_embed.max(verdict.embed_dist);
            any_face_skipped |= verdict.face_skipped;
        }
    }

    let mut reasons = Vec::new();
    match kind {
        DuplicateKind::Identical => {
            reasons.push("the same file was imported more than once".to_string());
        }
        DuplicateKind::NearIdentical => {
            reasons.push(format!(
                "difference hash within {worst_dhash} of {NEAR_DHASH_MAX} bits and appearance \
                 within {worst_embed:.3}"
            ));
            if any_face_skipped {
                reasons
                    .push("no faces to compare, so the check was on appearance alone".to_string());
            } else {
                reasons.push("the people are in the same places".to_string());
            }
        }
        DuplicateKind::Variant => {
            reasons.push(format!(
                "the same moment, but they differ: {worst_dhash} bits of hash and {worst_embed:.3} of appearance"
            ));
        }
    }
    if hint.subject_focus > 0.0 {
        reasons.push("kept frame chosen on face sharpness".to_string());
    } else {
        reasons.push("kept frame chosen on overall sharpness; no face pass has run".to_string());
    }
    if members.len() > REASON_FRAME_CAP {
        reasons.push(format!("{} frames in this set", members.len()));
    }
    reasons.truncate(DuplicateSet::MAX_REASONS);

    let mut image_ids: Vec<ImageId> = members
        .iter()
        .filter_map(|index| frames.get(*index).map(|frame| frame.image))
        .collect();
    image_ids.dedup();

    Some(DuplicateSet {
        kind,
        image_ids,
        keep_hint: hint.image,
        confidence,
        reasons,
        user_chosen: false,
    })
}
