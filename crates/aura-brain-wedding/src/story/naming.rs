//! The cloud path: `SegmentNaming` for the chapters the local model is unsure about.
//!
//! Section 6.4 and section 7. Phase 04 already built the task, the gateway, the cost
//! governor, the schema validator, the repair retry, the cache and the audit trail;
//! **this module is the caller, and a caller is where a cloud budget is actually
//! spent.** So what lives here is policy, not plumbing:
//!
//! * which chapters are worth asking about ([`worth_asking`]);
//! * how many calls a wedding may make ([`MAX_CALLS_PER_WEDDING`]);
//! * what a cloud answer is allowed to change ([`merge`]).
//!
//! ## Two spellings, one task
//!
//! Section 7 of the phase document specifies a schema whose keys are `chapter` and
//! `split_at_index`. Phase 04 shipped `SegmentNaming` with `scene` and `boundary_hint`
//! and froze it at `VERSION = 1`. ADR-0015 section 5 settles it: they are the same two
//! fields, renaming them would bump the task version and invalidate three recorded
//! cassettes plus every cached answer in every installed build, and the gain would be
//! that two documents use the same noun. So the task is unchanged and the translation
//! is here.
//!
//! ## What a cloud answer may and may not do
//!
//! Phase 04's rule, quoted: *"a cloud answer may not overrule a local decision at
//! confidence 0.90 or above unless it cites contradicting visual evidence, and the
//! conflict is logged."* [`merge`] implements exactly that, plus two rules of its own:
//!
//! * **it may never overrule a photographer.** `user_locked` chapters are filtered out
//!   before the call is even priced, so a locked chapter costs nothing as well as
//!   changing nothing;
//! * **it may not invent a chapter.** The answer is mapped back through
//!   `SceneId::from_cloud_label` and then `SceneId::chapter`, both total functions over
//!   closed vocabularies, so an unrecognised label becomes `ChapterId::Other` with the
//!   model's own note attached rather than a new kind of chapter.
//!
//! The split hint is deliberately **advisory**. `SegmentNamingOutput::boundary_hint`
//! becomes a reason string and a flag on [`NamingOutcome`]; it does not split anything.
//! A model looking at twelve thumbnails has less information about where a chapter
//! divides than a change-point detector looking at four thousand embeddings and their
//! timestamps, and letting it move a boundary would trade the better signal for the
//! more confident one.

use aura_cloud::contract::cloud::CloudTask;
use aura_cloud::gateway::{CallContext, CloudAiGateway};
use aura_cloud::tasks::{SceneGuess, SegmentInput, SegmentNaming, SegmentNamingOutput};
use aura_cloud::ImagePart;
use aura_core::contract::scene::{ChapterId, SceneId, Segment, NEEDS_REVIEW_BELOW};
use aura_core::{AuraResult, SegmentId};

/// The most cloud calls one wedding may make. Section 7's cost control.
///
/// Sixteen, against phase 04's `IMAGES_PER_CALL = 40` which would allow seventy-five on
/// a 3,000-image wedding. The two are not in conflict: phase 04's constant bounds calls
/// *per image* across every task in the product, and this one bounds calls *per wedding*
/// for this task. The tighter of the two wins, which is this one, because a chapter is
/// not an image and a wedding has between six and twenty of them.
pub const MAX_CALLS_PER_WEDDING: u32 = 16;

/// Tiles on the contact sheet for one chapter. Section 6.4.
pub const SHEET_TILES: usize = 12;

/// Tile side in pixels. Section 7: "up to 12 key thumbnails (768 px)".
pub const TILE_PX: u32 = 768;

/// Above this local confidence a cloud answer may not change the chapter.
///
/// Phase 04's rule, as a constant so it is checked rather than remembered.
pub const LOCAL_AUTHORITY: f32 = 0.90;

/// Which chapters are worth asking about, best candidates first.
///
/// Three filters, and the order they are applied in is the cost model:
///
/// 1. **locked chapters are dropped**, because the answer cannot be used;
/// 2. **confident chapters are dropped** at [`NEEDS_REVIEW_BELOW`], which is the same
///    0.75 the timeline flags for review and the same 0.75
///    `aura_cloud::tasks::ASK_BELOW_CONFIDENCE` uses - one number, because "flag it for
///    the photographer" and "ask a model" are the same question;
/// 3. the rest are sorted **least confident first** and truncated to
///    [`MAX_CALLS_PER_WEDDING`], so a wedding with twenty uncertain chapters spends its
///    budget on the twenty least certain rather than on the first sixteen in time
///    order.
#[must_use]
pub fn worth_asking(segments: &[Segment]) -> Vec<SegmentId> {
    let mut candidates: Vec<(&Segment, f32)> = segments
        .iter()
        .filter(|segment| !segment.user_locked && segment.confidence < NEEDS_REVIEW_BELOW)
        .map(|segment| (segment, segment.confidence))
        .collect();
    candidates.sort_by(|left, right| {
        left.1
            .total_cmp(&right.1)
            // Ties broken by start time so two machines spend the budget identically.
            .then_with(|| left.0.start_ts.cmp(&right.0.start_ts))
    });
    candidates
        .into_iter()
        .take(MAX_CALLS_PER_WEDDING as usize)
        .map(|(segment, _)| segment.id)
        .collect()
}

/// What one cloud call concluded.
#[derive(Debug, Clone, PartialEq)]
pub struct NamingOutcome {
    /// The chapter this is about.
    pub segment: SegmentId,
    /// The chapter label after merging. Unchanged when the answer was refused.
    pub chapter: ChapterId,
    /// The merged confidence.
    pub confidence: f32,
    /// Why, including the reason a cloud answer was refused when it was.
    pub reasons: Vec<String>,
    /// The tradition the visual evidence supported, when the model named one.
    pub tradition: Option<String>,
    /// True when the model suggested this chapter should be split.
    ///
    /// Advisory. See the module header for why it does not split anything.
    pub suggests_split: bool,
    /// True when the answer actually changed the label.
    pub changed: bool,
    /// True when the answer came from a model rather than from the local fallback.
    pub from_cloud: bool,
}

/// Assemble the input for one chapter.
///
/// `local_guesses` is quantised to per-mille before it reaches here, because
/// `CloudTask::Input` is bound by `Hash` and a classifier returning `0.640_000_1` on one
/// run and `0.639_999_9` on the next would miss the cache and bill the photographer
/// twice for the same question. Phase 04's rule, and the arithmetic is done here rather
/// than trusted to the caller.
#[must_use]
pub fn build_input(
    segment: &Segment,
    day_start_ms: i64,
    top3: &[(SceneId, f32)],
    tile_hashes: Vec<String>,
) -> SegmentInput {
    let offset = |ts: i64| -> u32 {
        u32::try_from((ts.saturating_sub(day_start_ms) / 1000).max(0)).unwrap_or(u32::MAX)
    };
    SegmentInput {
        segment_id: segment.id.to_db(),
        start_offset_s: offset(segment.start_ts),
        end_offset_s: offset(segment.end_ts),
        frame_count: segment.image_count,
        local_guesses: top3
            .iter()
            .take(3)
            .map(|(scene, score)| {
                let milli = (score.clamp(0.0, 1.0) * 1000.0).round();
                SceneGuess::new(scene.cloud_label(), milli as u16)
            })
            .collect(),
        // No EXIF. The tiles are key frames from across a chapter rather than a
        // consecutive burst, so a per-tile aperture tells the model nothing it cannot
        // see, and every field sent is a field that has left the machine.
        exif: Vec::new(),
        tile_hashes,
    }
}

/// Ask about one chapter and merge the answer.
///
/// Never fails for an ordinary cloud reason: the gateway turns every refusal, outage,
/// budget stop and malformed answer into the task's local fallback. It returns an error
/// only when the photographer cancelled.
///
/// # Errors
///
/// `AURA-CLOUD-6013` on cancellation.
pub fn name_one(
    gateway: &CloudAiGateway,
    sheet: ImagePart,
    segment: &Segment,
    input: &SegmentInput,
    context: &CallContext<'_>,
) -> AuraResult<NamingOutcome> {
    let task = SegmentNaming::for_sheet(sheet);
    let result = gateway.run(&task, input, context)?;
    let from_cloud = !matches!(result.source, aura_cloud::Source::LocalFallback);
    Ok(merge(segment, &result.value, from_cloud))
}

/// Merge one cloud answer into a local chapter.
///
/// Split out from [`name_one`] so it is testable without a gateway, a cassette or a
/// transport - which is how `crates/aura-brain-wedding/tests/naming.rs` asserts phase
/// 04's authority rule directly rather than through three layers of fixture.
#[must_use]
pub fn merge(segment: &Segment, answer: &SegmentNamingOutput, from_cloud: bool) -> NamingOutcome {
    let proposed = SceneId::from_cloud_label(&answer.scene).chapter();
    let mut reasons: Vec<String> = answer.reasons.clone();
    reasons.truncate(Segment::MAX_REASONS.saturating_sub(1));

    // Rule 1: a photographer is unbeatable. This should never fire - `worth_asking`
    // filters locked chapters out before anything is priced - and it is checked anyway,
    // because "the filter upstream handles it" is how a locked chapter eventually gets
    // overwritten.
    if segment.user_locked {
        return NamingOutcome {
            segment: segment.id,
            chapter: segment.chapter,
            confidence: segment.confidence,
            reasons: vec![
                "this chapter was set by the photographer, so the cloud answer was not applied"
                    .to_string(),
            ],
            tradition: None,
            suggests_split: false,
            changed: false,
            from_cloud,
        };
    }

    // Rule 2: phase 04's authority line. A local decision at 0.90 or above stands
    // unless the model cites contradicting visual evidence - and "cites" is checked as
    // "gave any reason at all", because a model that changes a confident local answer
    // without saying why is exactly the case the rule was written for.
    let contradicts = proposed != segment.chapter;
    if contradicts && segment.confidence >= LOCAL_AUTHORITY && answer.reasons.is_empty() {
        tracing::warn!(
            target: "scene.cloud_conflict",
            segment = %segment.id,
            local = segment.chapter.as_str(),
            proposed = proposed.as_str(),
            local_confidence = segment.confidence,
            "a cloud answer disagreed with a confident local chapter and cited no evidence; \
             the local answer stands"
        );
        return NamingOutcome {
            segment: segment.id,
            chapter: segment.chapter,
            confidence: segment.confidence,
            reasons: vec![format!(
                "cloud reasoning proposed {} but gave no visual evidence, and the local \
                 classifier is at {:.2}; the local answer stands",
                proposed.title(),
                segment.confidence
            )],
            tradition: answer.tradition.clone(),
            suggests_split: answer.boundary_hint.is_some(),
            changed: false,
            from_cloud,
        };
    }
    if contradicts && segment.confidence >= LOCAL_AUTHORITY {
        tracing::info!(
            target: "scene.cloud_conflict",
            segment = %segment.id,
            local = segment.chapter.as_str(),
            proposed = proposed.as_str(),
            local_confidence = segment.confidence,
            "a cloud answer overruled a confident local chapter, citing visual evidence"
        );
    }

    // The merged confidence. The maximum of the two rather than the mean, because the
    // call was made precisely *because* the local answer was weak: averaging a 0.40
    // local with a 0.85 cloud answer produces 0.62, which is still below the review
    // threshold and would leave a chapter flagged that a model has just identified.
    //
    // When the two agree the confidence is raised further, capped below 1.0 - which is
    // reserved for a photographer. Two sources agreeing is strong evidence and is not
    // certainty.
    let agreed = proposed == segment.chapter;
    let merged = if agreed {
        (segment.confidence.max(answer.confidence) + 0.10).min(0.98)
    } else {
        answer.confidence
    };

    if agreed {
        reasons.insert(
            0,
            format!(
                "cloud reasoning agreed with the local classifier that this is {}",
                proposed.title()
            ),
        );
    } else {
        reasons.insert(
            0,
            format!(
                "cloud reasoning named this {} where the local classifier had {} at {:.2}",
                proposed.title(),
                segment.chapter.title(),
                segment.confidence
            ),
        );
    }
    if let Some(note) = &answer.notes {
        if !note.trim().is_empty() {
            reasons.push(format!("note: {}", note.trim()));
        }
    }
    reasons.truncate(Segment::MAX_REASONS);

    NamingOutcome {
        segment: segment.id,
        chapter: proposed,
        confidence: merged.clamp(0.0, 0.98),
        reasons,
        tradition: answer.tradition.clone(),
        suggests_split: answer.boundary_hint.is_some(),
        changed: !agreed,
        from_cloud,
    }
}

/// What the task costs at its ceiling, for the budget check before any call is made.
///
/// `SegmentNaming::max_cost_usd` is two cents, so sixteen calls is USD 0.32 for a whole
/// wedding. The governor enforces the real budget; this is what the UI shows before the
/// photographer presses the button, and a number shown before a decision has to be an
/// upper bound rather than an estimate.
#[must_use]
pub fn worst_case_usd(calls: usize) -> f32 {
    let task = SegmentNaming::for_sheet(ImagePart {
        media_type: "image/jpeg".to_string(),
        bytes: Vec::new(),
        content_hash: String::new(),
        width: 0,
        height: 0,
    });
    task.max_cost_usd() * calls.min(MAX_CALLS_PER_WEDDING as usize) as f32
}
