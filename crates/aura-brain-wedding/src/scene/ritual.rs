//! The ritual head, and the abstention that is the point of it.
//!
//! Section 6.1: "a tradition-conditioned ritual head that is **only evaluated when the
//! segment context suggests a ceremony**", with "rare rituals get oversampling plus a
//! 'don't guess' abstain threshold".
//!
//! Both halves of that sentence are enforced here rather than assumed.
//!
//! ## Why it is gated on context
//!
//! Running a 160-way rite classifier over a detail shot of the rings produces an
//! answer. It is not a *useful* answer - the head has never seen a frame that is not a
//! ceremony frame, so its posterior over a table of flowers is whatever the nearest
//! training example was - and once written to `image_scenes.ritual` it is
//! indistinguishable from a real one. [`should_evaluate`] is the gate, and it costs
//! nothing: skipping a frame is also the cheapest possible inference.
//!
//! ## Why abstention is two things
//!
//! Slot 0 is `RitualId::NONE`, so a head that believes there is no rite says so
//! *through the softmax*, competing with every rite on equal terms. That is the
//! well-posed half.
//!
//! The other half is [`ABSTAIN_MARGIN`]. A head split 0.31 / 0.29 between
//! `saptapadi_pheras` and `saat_phera` has not failed - it has correctly identified a
//! fire circumambulation and cannot tell which tradition's name to use, which is a
//! *different* answer from "no rite". Recording either name at 0.31 would put a
//! Nepali wedding's rites under Hindu names in a client-facing timeline. Below the
//! margin the answer is `None` and the segment carries the reason.
//!
//! ## Tradition conditioning at decode time
//!
//! `Taxonomy::mask_for` zeroes the slots of rites the evidence has ruled out. That is
//! stronger than hoping the head learned the same thing, and it is what makes a
//! `communion` logit on a wedding that has been Hindu since the haldi impossible rather
//! than merely unlikely. The tradition itself is inferred from the rites already
//! accepted in the project - see [`TraditionVote`] - and never from a person.
//!
//! **Nothing in this module reads a face, a name, a skin tone or a garment.** A
//! tradition is inferred from *rites*, and a rite is inferred from objects and staging.
//! That is the same boundary `aura_cloud::tasks::SEGMENT_NAMING_SYSTEM` draws in its
//! third rule, and it is drawn twice on purpose.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::contract::priority::Priority;
use aura_core::contract::scene::{AttrFlags, RitualId, SceneId};
use aura_core::AuraResult;
use aura_infer::contract::infer::{InferRequest, InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{
    RITUAL_CLASSIFIER_MODEL, RITUAL_INPUT_DIM, RITUAL_SLOTS, RITUAL_TRADITIONS,
    SCENE_CONTEXT_FEATURES,
};

use crate::scene::taxonomy::Taxonomy;

/// Registry name of the ritual head.
pub const RITUAL_MODEL: &str = RITUAL_CLASSIFIER_MODEL;

/// Pinned version of that model.
pub const RITUAL_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The version stored beside a ritual slug.
pub const RITUAL_MODEL_VER: u16 =
    RITUAL_MODEL_VERSION.major * 100 + RITUAL_MODEL_VERSION.minor * 10 + RITUAL_MODEL_VERSION.patch;

/// The eight traditions the head is conditioned on.
///
/// The same eight as `aura_cloud::tasks::ALLOWED_TRADITIONS`, in the same order, and a
/// test asserts it. One list in the product: a cloud answer naming `sikh` and a local
/// prior with no slot for it would silently disagree.
pub const TRADITIONS: [&str; RITUAL_TRADITIONS] = [
    "hindu",
    "nepali",
    "muslim",
    "christian",
    "sikh",
    "civil",
    "mixed",
    "unclear",
];

/// Least posterior a rite may be named at.
pub const MIN_CONFIDENCE: f32 = 0.35;

/// Least gap between the top two rites. See the module header.
pub const ABSTAIN_MARGIN: f32 = 0.10;

/// The pinned model, as a value a caller can pass around.
#[must_use]
pub fn model_ref() -> ModelRef {
    ModelRef::new(RITUAL_MODEL, RITUAL_MODEL_VERSION)
}

/// Does this frame's context suggest a ceremony.
///
/// Section 6.1's gate. Three ways in, and the third is the one that matters for the
/// traditions this product is built for:
///
/// 1. the scene head chose a ceremony-family class;
/// 2. the scene head abstained but its top-1 was a ceremony-family class - an
///    abstention is not evidence that this is *not* a ceremony;
/// 3. the frame is on a stage, at night, with a crowd - which describes a mandap, a
///    swayambar and a nikah, and describes almost nothing else at a wedding.
#[must_use]
pub fn should_evaluate(top1: SceneId, attributes: AttrFlags) -> bool {
    let ceremonial = matches!(
        top1,
        SceneId::Ceremony
            | SceneId::CeremonyEntrance
            | SceneId::Ritual
            | SceneId::Vows
            | SceneId::Rings
            | SceneId::Kiss
    );
    let staged = attributes.contains(AttrFlags::STAGE)
        && (attributes.contains(AttrFlags::CROWD) || attributes.contains(AttrFlags::NIGHT));
    ceremonial || staged
}

/// One frame's inputs to the ritual head.
#[derive(Debug, Clone)]
pub struct RitualInput {
    /// The phase 05 embedding.
    pub embedding: Vec<f32>,
    /// The sixteen context numbers, already assembled by `scene::attributes`.
    pub context: [f32; SCENE_CONTEXT_FEATURES],
    /// The tradition the project's evidence supports so far, if any.
    pub tradition: Option<String>,
}

impl RitualInput {
    /// The 536-wide feature vector this model takes.
    #[must_use]
    pub fn to_features(&self) -> Vec<f32> {
        let width = RITUAL_INPUT_DIM - SCENE_CONTEXT_FEATURES - RITUAL_TRADITIONS;
        let mut out = Vec::with_capacity(RITUAL_INPUT_DIM);
        out.extend(self.embedding.iter().take(width).copied());
        out.resize(width, 0.0);
        out.extend_from_slice(&self.context);

        // The tradition one-hot. All zero when nothing is known yet, which is the
        // honest encoding of "the wedding has not told us": `unclear` is a *conclusion*
        // the evidence can reach and is not the same as having no evidence.
        let mut prior = [0.0f32; RITUAL_TRADITIONS];
        if let Some(name) = &self.tradition {
            if let Some(index) = TRADITIONS.iter().position(|known| known == name) {
                if let Some(slot) = prior.get_mut(index) {
                    *slot = 1.0;
                }
            }
        }
        out.extend_from_slice(&prior);
        out
    }
}

/// One frame's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct RitualVerdict {
    /// The rite, when one was named above the floor and the margin.
    pub ritual: Option<RitualId>,
    /// Its slug, for the catalog.
    pub slug: Option<String>,
    /// Which tradition declared it.
    pub tradition: Option<String>,
    /// The top-1 posterior, whether or not it was accepted.
    pub confidence: f32,
    /// Why. Empty when the head was not evaluated at all.
    pub reasons: Vec<String>,
}

impl RitualVerdict {
    /// The verdict for a frame the head was never run on.
    #[must_use]
    pub fn not_evaluated() -> Self {
        Self {
            ritual: None,
            slug: None,
            tradition: None,
            confidence: 0.0,
            reasons: Vec::new(),
        }
    }
}

/// The ritual head, with its taxonomy attached.
#[derive(Debug, Clone)]
pub struct RitualClassifier {
    infer: Arc<dyn InferService>,
    model: ModelRef,
    taxonomy: Arc<Taxonomy>,
}

impl RitualClassifier {
    /// Wrap the runtime and a loaded taxonomy.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>, taxonomy: Arc<Taxonomy>) -> Self {
        Self {
            infer,
            model: model_ref(),
            taxonomy,
        }
    }

    /// The taxonomy this head names rites from.
    #[must_use]
    pub fn taxonomy(&self) -> &Arc<Taxonomy> {
        &self.taxonomy
    }

    /// The pinned model.
    #[must_use]
    pub const fn model(&self) -> &ModelRef {
        &self.model
    }

    /// Name the rite in each frame, or abstain.
    ///
    /// # Errors
    ///
    /// Whatever the runtime raised.
    pub fn classify(
        &self,
        batch: &[RitualInput],
        prio: Priority,
    ) -> AuraResult<Vec<RitualVerdict>> {
        if batch.is_empty() {
            return Ok(Vec::new());
        }
        let features: Vec<f32> = batch.iter().flat_map(RitualInput::to_features).collect();
        let view = TensorView::new(vec![batch.len(), RITUAL_INPUT_DIM], &features)?;
        let request = InferRequest::new(self.model, vec![view], prio);
        let result = self.infer.run(request)?;
        let probs = result.outputs.first();

        Ok(batch
            .iter()
            .enumerate()
            .map(|(index, input)| {
                let row = probs
                    .and_then(|tensor| {
                        tensor
                            .data
                            .get(index * RITUAL_SLOTS..index * RITUAL_SLOTS + RITUAL_SLOTS)
                    })
                    .unwrap_or(&[]);
                self.decode(row, input.tradition.as_deref())
            })
            .collect())
    }

    /// Turn 160 posteriors into a rite or an abstention.
    ///
    /// Public so the eval harness can drive it with synthetic distributions - which is
    /// how section 10.1's "abstention rather than wrong labels on unseen rituals" is
    /// measured against a placeholder model. See condition C1 of the exit report.
    #[must_use]
    pub fn decode(&self, probs: &[f32], tradition: Option<&str>) -> RitualVerdict {
        if probs.is_empty() {
            return RitualVerdict::not_evaluated();
        }
        let mask = self.taxonomy.mask_for(tradition);

        // Every allowed slot above 0, best first, ties broken by slot so two machines
        // agree. Slot 0 - the abstention - is ranked with the rest rather than checked
        // separately: it wins or loses on the same terms as every rite.
        let mut ranked: Vec<(usize, f32)> = probs
            .iter()
            .enumerate()
            .filter(|(slot, _)| mask.get(*slot).copied().unwrap_or(false))
            .map(|(slot, score)| (slot, *score))
            .collect();

        // **Renormalise after masking.** The mask is a hard prior of zero on rites the
        // evidence has ruled out, and a posterior conditioned on a prior has to be
        // renormalised or the floor below is being applied to a distribution that no
        // longer sums to one. Without this, establishing the tradition would make the
        // head *less* likely to name a rite - which is precisely backwards, and is what
        // `ritual_abstention_beats_a_wrong_tradition` caught.
        let surviving: f32 = ranked.iter().map(|(_, score)| *score).sum();
        if surviving > 0.0 {
            for entry in &mut ranked {
                entry.1 /= surviving;
            }
        }

        ranked.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });

        let (best_slot, best_score) = ranked.first().copied().unwrap_or((0, 0.0));
        let second_score = ranked.get(1).map_or(0.0, |entry| entry.1);

        if best_slot == 0 {
            return RitualVerdict {
                ritual: None,
                slug: None,
                tradition: tradition.map(ToString::to_string),
                confidence: best_score,
                reasons: vec![format!(
                    "the ritual head chose 'no rite' at {best_score:.2}; this frame is inside a \
                     ceremony but shows no named rite"
                )],
            };
        }
        if best_score < MIN_CONFIDENCE {
            return RitualVerdict {
                ritual: None,
                slug: None,
                tradition: tradition.map(ToString::to_string),
                confidence: best_score,
                reasons: vec![format!(
                    "no rite reached the {MIN_CONFIDENCE:.2} floor; the strongest was \
                     {best_score:.2}, which is a guess rather than an identification"
                )],
            };
        }
        if best_score - second_score < ABSTAIN_MARGIN {
            let runner_up = ranked
                .get(1)
                .and_then(|(slot, _)| self.taxonomy.get(RitualId::new(*slot as u16)))
                .map_or_else(|| "another rite".to_string(), |rite| rite.title.clone());
            let best = self
                .taxonomy
                .get(RitualId::new(best_slot as u16))
                .map_or_else(|| "a rite".to_string(), |rite| rite.title.clone());
            return RitualVerdict {
                ritual: None,
                slug: None,
                tradition: tradition.map(ToString::to_string),
                confidence: best_score,
                reasons: vec![format!(
                    "{best} at {best_score:.2} and {runner_up} at {second_score:.2} are within \
                     {ABSTAIN_MARGIN:.2}; naming either would be a guess about which tradition's \
                     name to use"
                )],
            };
        }

        let id = RitualId::new(best_slot as u16);
        let Some(rite) = self.taxonomy.get(id) else {
            // A slot with no rite behind it means the head and the taxonomy disagree
            // about the world - a model trained against a taxonomy this build does not
            // have. Abstain and say so; do not invent a name for slot 91.
            return RitualVerdict {
                ritual: None,
                slug: None,
                tradition: tradition.map(ToString::to_string),
                confidence: best_score,
                reasons: vec![format!(
                    "the ritual head named slot {best_slot}, which no loaded taxonomy declares; \
                     the head and the taxonomy files are from different versions"
                )],
            };
        };

        RitualVerdict {
            ritual: Some(id),
            slug: Some(rite.slug.clone()),
            tradition: Some(rite.tradition.clone()),
            confidence: best_score,
            reasons: vec![
                format!(
                    "{} at {best_score:.2}, {:.2} clear of the runner-up",
                    rite.title,
                    best_score - second_score
                ),
                format!("visible evidence for this rite: {}", rite.evidence),
            ],
        }
    }
}

/// Which tradition a project's accepted rites point at.
///
/// A vote rather than an argmax over one frame, because a wedding is one tradition and
/// a single frame is weak evidence for it. `mixed` is a real answer - an interfaith
/// wedding with a nikah and a signing is not a classifier failure - and `unclear` is
/// the answer when nothing has been named at all.
#[derive(Debug, Clone, Default)]
pub struct TraditionVote {
    counts: BTreeMap<String, u32>,
}

impl TraditionVote {
    /// Start with nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one accepted rite.
    pub fn add(&mut self, tradition: &str) {
        *self.counts.entry(tradition.to_string()).or_insert(0) += 1;
    }

    /// How many rites have voted.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.counts.values().sum()
    }

    /// The tradition, or `None` when nothing has been named.
    ///
    /// Returns `"mixed"` when the runner-up holds at least [`MIXED_SHARE`] of the
    /// votes: two traditions each with a third of the rites is an interfaith wedding,
    /// and calling it by the larger one's name would mislabel half of it.
    #[must_use]
    pub fn verdict(&self) -> Option<String> {
        if self.counts.is_empty() {
            return None;
        }
        let mut ranked: Vec<(&String, &u32)> = self.counts.iter().collect();
        ranked.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));

        let total = f64::from(self.total());
        let best = ranked.first()?;
        let second = ranked.get(1).map_or(0u32, |entry| *entry.1);
        if total > 0.0 && f64::from(second) / total >= f64::from(MIXED_SHARE) {
            return Some("mixed".to_string());
        }
        Some(best.0.clone())
    }
}

/// Share of the rite votes the runner-up needs before a wedding is called `mixed`.
pub const MIXED_SHARE: f32 = 0.30;
