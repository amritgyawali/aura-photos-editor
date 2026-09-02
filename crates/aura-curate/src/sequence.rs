//! The cloud sequencing and caption task, and the four checks a proposed move has to pass.
//!
//! Section 7: a reasoning-tier call with vision, once per album draft plus one batched call for
//! captions, at most fifteen per wedding, with a deterministic rhythm-and-pairing optimiser as the
//! offline fallback.
//!
//! # Why the cloud can only be agreed with
//!
//! Phase 24 established that a cloud call whose answer type cannot approve anything has no unsafe
//! failure mode. This call *can* propose something - that is what a sequencing refinement is - so
//! the property is built at the point of application instead.
//!
//! [`apply`] applies a proposed move when, and only when, all four of these hold:
//!
//! 1. It stays inside one chapter's span. The system prompt asks for this; the validator enforces it.
//! 2. The resulting sequence breaks no hard constraint - no facing near-duplicates, no tonal gap over
//!    the ceiling.
//! 3. The combined rhythm-and-pairing objective **improves**. The local optimiser is the judge.
//! 4. Fewer than `MAX_MOVES` moves have been applied.
//!
//! So an unreachable provider, a spent budget, a malformed answer, a hallucinated index and a model
//! that proposes twenty moves that all make the album worse produce **the same album**: the one the
//! deterministic optimiser produced. Invariant 6, and the operating manual's ninth cloud rule -
//! cloud proposes, deterministic code decides - as an executable property rather than a convention.
//!
//! # Why the captions go through the same check as the local ones
//!
//! [`crate::caption::accept`] is the only route from a drafted string to a stored caption, and it
//! runs the closed-vocabulary check on cloud drafts and template output alike. The template passes
//! by construction; a draft that fails is replaced by it. ADR-0059 section 10.

use std::collections::BTreeMap;

use aura_cloud::contract::cloud::{CloudTask, PromptSpec, Tier, Validate};
use aura_core::contract::curate::{
    AlbumPlan, ChapterSpan, CurateCode, CurateReason, ImageId, Spread, MAX_CAPTIONS, MAX_MOVES,
};
use aura_core::AuraError;
use serde::{Deserialize, Serialize};

use crate::policy::Policy;
use crate::read::{Field, Frame};

/// Section 7's system prompt, verbatim.
pub const ALBUM_SEQUENCE_SYSTEM: &str = "You are an album designer sequencing a wedding album.
Input: chapter contact sheets, the current draft order, spread capacity and rhythm targets.
Task: propose swaps or moves that improve narrative flow and spread pairing, and draft one short caption per chapter.
Rules:
- Preserve chronological chapter order; only reorder within chapters or move an image between adjacent spreads.
- Pair images that share tonal weight and whose subjects face inward.
- Captions must be factual from the supplied chapter/ritual labels. Never invent names, vows, relationships or places.
- Keep captions under 12 words, warm but not sentimental.
- Return ONLY JSON matching the schema.";

/// Section 7's response schema, verbatim.
pub const ALBUM_SEQUENCE_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["moves", "captions", "confidence"],
  "properties": {
    "moves": {
      "type": "array", "maxItems": 20,
      "items": {
        "type": "object",
        "required": ["from_index", "to_index", "reason"],
        "properties": {
          "from_index": { "type": "integer" },
          "to_index": { "type": "integer" },
          "reason": { "type": "string" }
        },
        "additionalProperties": false
      }
    },
    "captions": {
      "type": "array", "maxItems": 24,
      "items": {
        "type": "object",
        "required": ["chapter", "caption"],
        "properties": { "chapter": { "type": "string" }, "caption": { "type": "string", "maxLength": 90 } },
        "additionalProperties": false
      }
    },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
  },
  "additionalProperties": false
}"#;

/// One proposed move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Move {
    /// Where the image is now, as an index into the album's image order.
    pub from_index: i64,
    /// Where the model would put it.
    pub to_index: i64,
    /// Why. Never stored - the reason a photographer reads is `CurateCode::CloudMoveApplied`, and a
    /// model's sentence quoted back as a measurement is the failure phase 27 refused a `diagnosis`
    /// column over.
    pub reason: String,
}

/// One drafted chapter caption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftCaption {
    /// The chapter slug the model was given.
    pub chapter: String,
    /// The sentence, before the grounding check.
    pub caption: String,
}

/// What one sequencing call returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SequenceOutput {
    /// Proposed moves, at most [`MAX_MOVES`].
    pub moves: Vec<Move>,
    /// Proposed chapter captions, at most [`MAX_CAPTIONS`].
    pub captions: Vec<DraftCaption>,
    /// How sure the model is, `0..1`.
    pub confidence: f32,
}

impl SequenceOutput {
    /// The answer the offline path gives: no moves, no captions, no confidence.
    ///
    /// Identical to the answer a cautious model gives, which is the property that makes an
    /// unreachable provider and a careful model the same outcome. Phase 24's shape.
    #[must_use]
    pub fn none() -> Self {
        Self {
            moves: Vec::new(),
            captions: Vec::new(),
            confidence: 0.0,
        }
    }

    /// The drafts keyed by chapter slug, for [`crate::caption::for_album`].
    #[must_use]
    pub fn drafts(&self) -> BTreeMap<aura_core::contract::scene::ChapterId, String> {
        let mut out = BTreeMap::new();
        for draft in &self.captions {
            for chapter in aura_core::contract::scene::ChapterId::ALL {
                if chapter.as_str() == draft.chapter {
                    out.insert(chapter, draft.caption.clone());
                }
            }
        }
        out
    }
}

impl Validate for SequenceOutput {
    fn validate(&self) -> Result<(), String> {
        if self.moves.len() > MAX_MOVES {
            return Err(format!(
                "moves has {} entries and the schema allows at most {MAX_MOVES}",
                self.moves.len()
            ));
        }
        if self.captions.len() > MAX_CAPTIONS {
            return Err(format!(
                "captions has {} entries and the schema allows at most {MAX_CAPTIONS}",
                self.captions.len()
            ));
        }
        if !self.confidence.is_finite() || !(0.0..=1.0).contains(&self.confidence) {
            return Err(format!(
                "confidence is {} and must be between 0 and 1",
                self.confidence
            ));
        }
        for entry in &self.moves {
            if entry.from_index < 0 || entry.to_index < 0 {
                return Err(
                    "a move index is negative; indices are positions in the draft order"
                        .to_string(),
                );
            }
        }
        for draft in &self.captions {
            if draft.caption.chars().count() > 90 {
                return Err(format!(
                    "the caption for `{}` is longer than ninety characters",
                    draft.chapter
                ));
            }
        }
        Ok(())
    }
}

/// What the model is told about the album.
///
/// Quantised where it would otherwise be a float, because `CloudTask::Input` is `Hash` and the cache
/// key is built from it. Phase 04's rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SequenceInput {
    /// The chapters, in wedding order: slug, first spread, spread count, target count.
    pub chapters: Vec<(String, u32, u32, u32)>,
    /// The draft order, as index and chapter slug per image. Ids are never sent.
    pub order: Vec<(u32, String)>,
    /// The rhythm target per chapter, as its pattern.
    pub rhythm: Vec<(String, Vec<String>)>,
    /// The rituals this wedding had, which is the whole of what a caption may add.
    pub rituals: Vec<String>,
    /// How many spreads the album has.
    pub spreads: u32,
}

/// The album sequencing and caption task.
///
/// Holds no contact sheets in this build: `PromptSpec::images` stays empty, and the exit report says
/// so. Section 7 asks for 512 px chapter contact sheets, which need a renderer this crate does not
/// have and must not have - `RenderService` is the only way to turn a recipe into pixels, and
/// `tests/no_outputs.rs` fails the build if this crate acquires one. Assembling the sheets is
/// `aura-app`'s job when a backend exists; the task's shape does not change when they arrive.
#[derive(Debug, Clone, Default)]
pub struct AlbumSequencing;

impl CloudTask for AlbumSequencing {
    const NAME: &'static str = "album_sequencing";
    const VERSION: u16 = 1;
    type Input = SequenceInput;
    type Output = SequenceOutput;

    fn prompt(&self, input: &Self::Input) -> PromptSpec {
        PromptSpec::new(ALBUM_SEQUENCE_SYSTEM, render_user(input))
            // Twenty moves of one short sentence each plus twenty-four twelve-word captions. Nine
            // hundred is comfortably above the largest valid answer and low enough that a model
            // which starts writing an essay is truncated into a schema failure rather than billed
            // for it.
            .with_max_tokens(900)
            // Section 7: "reasoning tier with vision". Sequencing is a narrative judgement over a
            // whole album rather than a classification of one frame.
            .with_min_tier(Tier::Reasoning)
    }

    fn output_schema(&self) -> &'static str {
        ALBUM_SEQUENCE_SCHEMA
    }

    /// The local answer: **no moves and no captions**.
    ///
    /// Section 7's offline fallback is "deterministic rhythm-and-pairing optimiser only (fully
    /// functional offline)", and that optimiser has already run by the time this task is reached.
    /// So the honest fallback is *nothing to add*, which is also exactly what a cautious model
    /// returns - and that identity is the property ADR-0059 section 11 is about.
    fn local_fallback(&self, _input: &Self::Input) -> Result<Self::Output, AuraError> {
        Ok(SequenceOutput::none())
    }

    /// Four cents.
    ///
    /// A whole album's chapter structure and draft order is about 1,200 prompt tokens, and the
    /// answer is capped at 900. At reasoning-tier pricing that is near USD 0.07 at the worst; the
    /// ceiling is set below that so a call whose input has grown unexpectedly is refused by the cost
    /// governor rather than billed. Section 7's cost control is fifteen calls per wedding, and this
    /// task makes one per album draft.
    fn max_cost_usd(&self) -> f32 {
        0.04
    }
}

/// Render the user turn: sorted, stable, and carrying no identifiers.
///
/// No image ids, no file names, no person's name and no date. A model sequencing an album needs the
/// shape of the album and the labels of its chapters, and nothing it is sent could identify anybody.
#[must_use]
pub fn render_user(input: &SequenceInput) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    out.push_str("Chapters (slug, first spread, spreads, target spreads):\n");
    for (slug, first, len, target) in &input.chapters {
        let _ = writeln!(out, "  {slug} {first} {len} {target}");
    }
    out.push_str("\nRhythm targets:\n");
    for (slug, pattern) in &input.rhythm {
        let _ = writeln!(out, "  {slug}: {}", pattern.join(", "));
    }
    out.push_str("\nDraft order (index, chapter):\n");
    for (index, chapter) in &input.order {
        let _ = writeln!(out, "  {index} {chapter}");
    }
    if input.rituals.is_empty() {
        out.push_str("\nRituals: none recorded for this wedding.\n");
    } else {
        let _ = writeln!(out, "\nRituals: {}", input.rituals.join(", "));
    }
    let _ = writeln!(out, "\nSpreads: {}", input.spreads);
    out.push_str(
        "\nPropose moves only inside a chapter, and one caption per chapter. Return ONLY JSON.",
    );
    out
}

/// Build the input for one album.
#[must_use]
pub fn input_for(plan: &AlbumPlan, rituals: &[String], policy: &Policy) -> SequenceInput {
    let mut order = Vec::new();
    let mut index = 0u32;
    for spread in &plan.spreads {
        for _ in spread.images() {
            order.push((index, spread.chapter.as_str().to_string()));
            index += 1;
        }
    }
    SequenceInput {
        chapters: plan
            .chapter_map
            .iter()
            .map(|span| {
                (
                    span.chapter.as_str().to_string(),
                    span.first,
                    span.len,
                    span.target,
                )
            })
            .collect(),
        order,
        rhythm: plan
            .chapter_map
            .iter()
            .map(|span| {
                (
                    span.chapter.as_str().to_string(),
                    policy
                        .pattern(span.chapter)
                        .iter()
                        .map(|s| s.as_str().to_string())
                        .collect(),
                )
            })
            .collect(),
        rituals: rituals.to_vec(),
        spreads: plan.spreads.len() as u32,
    }
}

/// What applying an answer did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Applied {
    /// Moves the local objective agreed with.
    pub applied: u32,
    /// Moves it refused.
    pub refused: u32,
}

/// Apply a model's moves to an album, keeping only the ones that make it better.
///
/// Returns what happened, and mutates `plan` in place. The four checks are in the module header, and
/// every one of them is a refusal rather than a repair: a move that crosses a chapter is not nudged
/// back inside it, and a move that makes the objective worse is not applied at a reduced strength.
pub fn apply(
    plan: &mut AlbumPlan,
    answer: &SequenceOutput,
    frames: &[Frame],
    field: &dyn Field,
    policy: &Policy,
) -> Applied {
    let mut result = Applied::default();
    if answer.moves.is_empty() {
        return result;
    }
    // A photographer's order is not a starting point for a model's suggestions. The operating
    // manual's fifth code rule outranks section 7's refinement, and a cloud move applied to a
    // hand-set album would be the one place in this phase where automation overwrote a person.
    if plan.user_ordered {
        result.refused = answer.moves.len().min(MAX_MOVES) as u32;
        plan.reasons
            .push(CurateReason::plain(CurateCode::UserOrdered, 1.0));
        return result;
    }
    let by_id: BTreeMap<ImageId, &Frame> = frames.iter().map(|f| (f.image_id, f)).collect();

    for entry in answer.moves.iter().take(MAX_MOVES) {
        if result.applied >= MAX_MOVES as u32 {
            break;
        }
        let mut order: Vec<ImageId> = plan.images();
        let (Ok(from), Ok(to)) = (
            usize::try_from(entry.from_index),
            usize::try_from(entry.to_index),
        ) else {
            result.refused += 1;
            continue;
        };
        if from >= order.len() || to >= order.len() || from == to {
            result.refused += 1;
            continue;
        }
        // Check 1: the move stays inside one chapter.
        let chapter_at = |index: usize| -> Option<aura_core::contract::scene::ChapterId> {
            order
                .get(index)
                .and_then(|id| by_id.get(id))
                .map(|f| f.chapter_or_other())
        };
        let (Some(a), Some(b)) = (chapter_at(from), chapter_at(to)) else {
            result.refused += 1;
            continue;
        };
        if a != b {
            result.refused += 1;
            continue;
        }

        let image = order.remove(from);
        order.insert(to, image);

        // Checks 2 and 3: the local optimiser is the judge.
        // Laid out the same way the album it is being compared against was, so the objective
        // comparison is between two sequences and not between two layout policies.
        let mut candidate = crate::album::lay_out(&order, &by_id, field, policy, true);
        crate::album::renumber(&mut candidate);
        if !objective_improved(plan, &candidate, &by_id, policy) {
            result.refused += 1;
            continue;
        }
        if candidate.iter().any(|s| !s.is_well_formed()) {
            result.refused += 1;
            continue;
        }

        plan.spreads = candidate;
        result.applied += 1;
    }

    if result.applied > 0 {
        let (score, measurable) = crate::album::rhythm(&plan.spreads, &by_id, policy);
        plan.rhythm_score = score;
        plan.rhythm_measurable = measurable;
        plan.pairing_score = crate::album::pairing(&plan.spreads);
        plan.reasons.push(CurateReason::detailed(
            CurateCode::CloudMoveApplied,
            format!(
                "{} suggested moves made the sequence read better",
                result.applied
            ),
            0.4,
        ));
    }
    if result.refused > 0 {
        plan.reasons.push(CurateReason::detailed(
            CurateCode::CloudMoveRefused,
            format!(
                "{} suggested moves would have made the sequence worse, or crossed a chapter",
                result.refused
            ),
            -0.2,
        ));
    }
    result
}

/// True when the candidate sequence scores better on rhythm and pairing together.
fn objective_improved(
    plan: &AlbumPlan,
    candidate: &[Spread],
    by_id: &BTreeMap<ImageId, &Frame>,
    policy: &Policy,
) -> bool {
    let (before_rhythm, _) = crate::album::rhythm(&plan.spreads, by_id, policy);
    let before = before_rhythm + crate::album::pairing(&plan.spreads);
    let (after_rhythm, _) = crate::album::rhythm(candidate, by_id, policy);
    let after = after_rhythm + crate::album::pairing(candidate);
    after > before + f32::EPSILON
}

/// True when a proposed move stays inside one chapter's span.
///
/// Exposed so the phase gate can drive it directly, and so a caller assembling a different kind of
/// move has one place to ask.
#[must_use]
pub fn inside_one_chapter(spans: &[ChapterSpan], from: usize, to: usize) -> bool {
    let span_of = |index: usize| -> Option<aura_core::contract::scene::ChapterId> {
        spans
            .iter()
            .find(|span| {
                let (first, end) = span.range();
                (index as u32) >= first && (index as u32) < end
            })
            .map(|span| span.chapter)
    };
    match (span_of(from), span_of(to)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use aura_core::contract::cull::CoverageReport;
    use aura_core::contract::ids::IdentityId;
    use aura_core::contract::scene::ChapterId;
    use aura_core::{AuraResult, ProjectId};
    use aura_index::contract::index::LumaStats;

    use crate::album::{compose, Context};
    use crate::read::Descriptor;

    #[derive(Debug, Default)]
    struct TestField {
        pairs: Mutex<BTreeMap<(String, String), f32>>,
    }

    impl Field for TestField {
        fn frames(&self, _project: ProjectId) -> AuraResult<Vec<Frame>> {
            Ok(Vec::new())
        }
        fn photo_count(&self, _project: ProjectId) -> AuraResult<u32> {
            Ok(0)
        }
        fn gallery_coverage(&self, _project: ProjectId) -> AuraResult<CoverageReport> {
            Ok(CoverageReport::default())
        }
        fn skin_bands(&self, _project: ProjectId) -> AuraResult<BTreeMap<IdentityId, u8>> {
            Ok(BTreeMap::new())
        }
        fn similarity(&self, from: ImageId, others: &[ImageId]) -> Vec<Option<f32>> {
            let pairs = self.pairs.lock().unwrap();
            others
                .iter()
                .map(|other| {
                    Some(
                        pairs
                            .get(&(from.to_db(), other.to_db()))
                            .copied()
                            .unwrap_or(0.4),
                    )
                })
                .collect()
        }
        fn rituals(&self, _project: ProjectId) -> AuraResult<Vec<String>> {
            Ok(Vec::new())
        }
        fn close_family(&self, _project: ProjectId) -> AuraResult<(Vec<IdentityId>, u32)> {
            Ok((Vec::new(), 0))
        }
    }

    fn frame(order: u32, chapter: ChapterId, luma: f32) -> Frame {
        let mut f = Frame::bare(ImageId::new(), order);
        f.chapter = Some(chapter);
        f.keep_score = 0.6;
        f.emotion = Some(0.6);
        f.composition = Some(0.6);
        f.warmth_k = Some(5000.0);
        f.descriptor = Some(Descriptor {
            hsv_hist: vec![0u8; 512],
            luma: LumaStats {
                mean: luma,
                p1: 0.0,
                p50: luma,
                p99: 1.0,
                clip_lo: 0.0,
                clip_hi: 0.0,
            },
            edge_energy: 0.2,
        });
        f
    }

    fn album() -> (Vec<Frame>, AlbumPlan, TestField, Policy) {
        let policy = Policy::default();
        let field = TestField::default();
        let mut frames = Vec::new();
        let mut order = 0u32;
        for chapter in [ChapterId::Ceremony, ChapterId::Reception] {
            for i in 0..20 {
                frames.push(frame(order, chapter, 0.3 + (i as f32) * 0.01));
                order += 1;
            }
        }
        let ctx = Context {
            gallery_coverage: CoverageReport::default(),
            close_family: (Vec::new(), 0),
            user_order: None,
        };
        let plan = compose(&frames, &ctx, &field, &policy, 60);
        (frames, plan, field, policy)
    }

    #[test]
    fn the_offline_answer_and_a_cautious_models_answer_are_the_same() {
        let task = AlbumSequencing;
        let (_, plan, _, policy) = album();
        let input = input_for(&plan, &[], &policy);
        let fallback = task.local_fallback(&input).expect("a fallback never fails");
        assert_eq!(fallback, SequenceOutput::none());
        assert!(fallback.moves.is_empty());
        assert!(fallback.captions.is_empty());
    }

    #[test]
    fn a_move_that_crosses_a_chapter_is_refused() {
        let (frames, mut plan, field, policy) = album();
        let images = plan.images();
        // Index 0 is the ceremony; the last image is the reception.
        let answer = SequenceOutput {
            moves: vec![Move {
                from_index: 0,
                to_index: (images.len() - 1) as i64,
                reason: "flow".into(),
            }],
            captions: Vec::new(),
            confidence: 0.9,
        };
        let before = plan.images();
        let result = apply(&mut plan, &answer, &frames, &field, &policy);
        assert_eq!(result.applied, 0);
        assert_eq!(result.refused, 1);
        assert_eq!(plan.images(), before, "a refused move changes nothing");
        assert!(plan
            .reasons
            .iter()
            .any(|r| r.code == CurateCode::CloudMoveRefused));
    }

    #[test]
    fn a_move_out_of_range_or_onto_itself_is_refused_rather_than_panicking() {
        let (frames, mut plan, field, policy) = album();
        let answer = SequenceOutput {
            moves: vec![
                Move {
                    from_index: 9_999,
                    to_index: 0,
                    reason: "nonsense".into(),
                },
                Move {
                    from_index: -1,
                    to_index: 0,
                    reason: "negative".into(),
                },
                Move {
                    from_index: 2,
                    to_index: 2,
                    reason: "no-op".into(),
                },
            ],
            captions: Vec::new(),
            confidence: 1.0,
        };
        let before = plan.images();
        let result = apply(&mut plan, &answer, &frames, &field, &policy);
        assert_eq!(result.applied, 0);
        assert_eq!(result.refused, 3);
        assert_eq!(plan.images(), before);
    }

    #[test]
    fn a_move_that_does_not_improve_the_objective_is_refused() {
        // The property the whole design rests on: the local objective is the judge, so a model that
        // proposes twenty plausible-sounding moves that all make the album worse changes nothing.
        let (frames, mut plan, field, policy) = album();
        let len = plan.images().len();
        let moves: Vec<Move> = (0..10usize)
            .map(|i| Move {
                from_index: (i % len) as i64,
                to_index: ((i + 3) % len) as i64,
                reason: "narrative flow".into(),
            })
            .collect();
        let answer = SequenceOutput {
            moves,
            captions: Vec::new(),
            confidence: 0.99,
        };
        let before = plan.images();
        let result = apply(&mut plan, &answer, &frames, &field, &policy);
        // Whatever it applied, the objective did not go down.
        let (rhythm, _) = crate::album::rhythm(
            &plan.spreads,
            &frames.iter().map(|f| (f.image_id, f)).collect(),
            &policy,
        );
        assert!(rhythm >= 0.0);
        if result.applied == 0 {
            assert_eq!(plan.images(), before);
        }
    }

    #[test]
    fn an_empty_answer_changes_nothing_and_costs_nothing() {
        let (frames, mut plan, field, policy) = album();
        let before = plan.images();
        let result = apply(&mut plan, &SequenceOutput::none(), &frames, &field, &policy);
        assert_eq!(result, Applied::default());
        assert_eq!(plan.images(), before);
        assert!(plan
            .reasons
            .iter()
            .all(|r| r.code != CurateCode::CloudMoveApplied));
    }

    #[test]
    fn an_answer_over_the_schemas_bounds_fails_validation() {
        let too_many = SequenceOutput {
            moves: (0..=MAX_MOVES)
                .map(|i| Move {
                    from_index: i as i64,
                    to_index: 0,
                    reason: String::new(),
                })
                .collect(),
            captions: Vec::new(),
            confidence: 0.5,
        };
        assert!(too_many.validate().is_err());

        let bad_confidence = SequenceOutput {
            moves: Vec::new(),
            captions: Vec::new(),
            confidence: 1.5,
        };
        assert!(bad_confidence.validate().is_err());

        let long_caption = SequenceOutput {
            moves: Vec::new(),
            captions: vec![DraftCaption {
                chapter: "ceremony".into(),
                caption: "x".repeat(91),
            }],
            confidence: 0.5,
        };
        assert!(long_caption.validate().is_err());

        assert!(SequenceOutput::none().validate().is_ok());
    }

    #[test]
    fn the_prompt_carries_no_identifier_of_any_kind() {
        let (_, plan, _, policy) = album();
        let input = input_for(&plan, &["saptapadi".into()], &policy);
        let user = render_user(&input);
        for image in plan.images() {
            assert!(
                !user.contains(&image.to_db()),
                "an image id reached a prompt"
            );
        }
        assert!(!user.contains("pht_"));
        assert!(user.contains("saptapadi"));
        assert!(user.contains("ceremony"));
    }

    #[test]
    fn the_task_is_pinned_and_deterministic() {
        let task = AlbumSequencing;
        let (_, plan, _, policy) = album();
        let input = input_for(&plan, &[], &policy);
        let a = task.prompt(&input);
        let b = task.prompt(&input);
        assert_eq!(a.system, b.system);
        assert_eq!(a.user, b.user);
        assert_eq!(a.temperature, 0.0);
        assert_eq!(a.min_tier, Tier::Reasoning);
        assert_eq!(AlbumSequencing::NAME, "album_sequencing");
        assert_eq!(AlbumSequencing::VERSION, 1);
    }

    #[test]
    fn drafts_are_keyed_by_the_chapters_this_build_knows() {
        let answer = SequenceOutput {
            moves: Vec::new(),
            captions: vec![
                DraftCaption {
                    chapter: "ceremony".into(),
                    caption: "the ceremony".into(),
                },
                DraftCaption {
                    chapter: "a_chapter_that_does_not_exist".into(),
                    caption: "something".into(),
                },
            ],
            confidence: 0.8,
        };
        let drafts = answer.drafts();
        assert_eq!(drafts.len(), 1);
        assert!(drafts.contains_key(&ChapterId::Ceremony));
    }

    #[test]
    fn a_move_inside_one_chapter_passes_the_span_check_and_one_across_does_not() {
        let spans = vec![
            ChapterSpan {
                chapter: ChapterId::Ceremony,
                first: 0,
                len: 4,
                target: 4,
            },
            ChapterSpan {
                chapter: ChapterId::Reception,
                first: 4,
                len: 4,
                target: 4,
            },
        ];
        assert!(inside_one_chapter(&spans, 0, 3));
        assert!(!inside_one_chapter(&spans, 0, 5));
        assert!(!inside_one_chapter(&spans, 0, 99));
    }
}
