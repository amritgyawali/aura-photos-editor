//! The task registry, and `SegmentNaming` - the reference task that proves the
//! whole contract end to end.
//!
//! ## `Scored`, and invariant 2
//!
//! Every task's `Output` must implement [`Scored`]. That is where invariant 2 -
//! "every AI decision carries `confidence` (0-1) and `reasons[]`; a decision
//! without an explanation is a bug" - stops being a rule people remember and
//! becomes one the compiler enforces. A task whose answer cannot explain itself
//! does not build.
//!
//! ## Why `SegmentNaming` and not something easier
//!
//! It is the hardest shape the product has: multimodal input, a controlled
//! vocabulary, a cultural judgement that must not become a guess about
//! individuals, a real local fallback, and a cost ceiling tight enough that the
//! payload design matters. A reference task that proved the contract on a text
//! classification would prove very little.
//!
//! ## Determinism, and why the scores are integers
//!
//! `CloudTask::Input` is bound by `Hash`, which `f32` does not implement, and the
//! reason that bound is in the frozen contract is in
//! `docs/adr/ADR-0009-cloud-ai-policy.md`: a local classifier returning
//! `0.640_000_1` on one run and `0.639_999_9` on the next would miss the cache and
//! bill the user twice for the same question. Scores are therefore per-mille
//! `u16` - 640 is 0.64 - quantised by the caller before they reach here.

use std::collections::BTreeSet;

use aura_core::AuraError;
use serde::{Deserialize, Serialize};

use crate::contract::cloud::{CloudTask, ImagePart, PromptSpec, Tier, Validate};
use crate::redact::ExifSummary;

/// A decision that can explain itself. Required of every task's `Output`.
pub trait Scored {
    /// How sure the answer is, 0.0 to 1.0.
    fn confidence(&self) -> f32;

    /// Why. At least one entry for any decision above low confidence.
    fn reasons(&self) -> &[String];
}

/// Below this confidence a decision may be reported without reasons.
///
/// Above it, an unexplained decision is rejected by [`Validate`]. A model that is
/// genuinely unsure and says so is useful; a model that is confident and silent
/// is a decision nobody can defend to a client.
pub const REASONS_REQUIRED_ABOVE: f32 = 0.40;

/// The controlled vocabulary of wedding scenes.
///
/// A closed list, sent in the prompt and checked on the way back. An open-ended
/// scene name is unusable downstream: phase 07 conditions its thresholds on the
/// scene, and a threshold table cannot be written against free text.
pub const ALLOWED_SCENES: &[&str] = &[
    "getting_ready",
    "first_look",
    "ceremony",
    "ritual",
    "portraits",
    "family_formals",
    "group_photo",
    "reception_entrance",
    "speeches",
    "first_dance",
    "dancing",
    "cake_cutting",
    "dinner",
    "details",
    "venue",
    "departure",
    "candid",
    "other",
];

/// The controlled vocabulary of rituals, across the traditions this product is
/// built for.
///
/// Named rituals rather than a generic "religious ceremony", because a Nepali
/// wedding's `sindoor` moment and a Christian wedding's ring exchange are both
/// the frame the couple will enlarge, and a culling engine that cannot tell them
/// apart will throw one of them away.
pub const ALLOWED_RITUALS: &[&str] = &[
    "haldi",
    "mehndi",
    "baraat",
    "jaimala_varmala",
    "kanyadaan",
    "saptapadi_pheras",
    "sindoor_mangalsutra",
    "vidaai",
    "nikah",
    "walima",
    "ring_exchange",
    "vows",
    "unity_candle",
    "communion",
    "tea_ceremony",
    "handfasting",
    "none",
    "other",
];

/// The traditions the vocabulary covers.
pub const ALLOWED_TRADITIONS: &[&str] = &[
    "hindu",
    "nepali",
    "muslim",
    "christian",
    "sikh",
    "civil",
    "mixed",
    "unclear",
];

/// One of the local classifier's guesses about a segment.
///
/// `score_milli` is per-mille: 640 means 0.64. See the module note.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SceneGuess {
    /// A member of [`ALLOWED_SCENES`].
    pub scene: String,
    /// Confidence in per-mille, 0 to 1000.
    pub score_milli: u16,
}

impl SceneGuess {
    /// A guess from a name and a per-mille score.
    #[must_use]
    pub fn new(scene: &str, score_milli: u16) -> Self {
        Self {
            scene: scene.to_string(),
            score_milli: score_milli.min(1000),
        }
    }
}

/// What the caller knows about one timeline segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SegmentInput {
    /// Stable identifier of the segment, used as the decision reference.
    pub segment_id: String,
    /// Seconds from the start of the wedding day to the first frame.
    pub start_offset_s: u32,
    /// Seconds from the start of the wedding day to the last frame.
    pub end_offset_s: u32,
    /// Frames in the segment, which may be more than the tiles on the sheet.
    pub frame_count: u32,
    /// The local classifier's top guesses, best first.
    pub local_guesses: Vec<SceneGuess>,
    /// One summary per tile on the contact sheet, in tile order.
    pub exif: Vec<ExifSummary>,
    /// blake3 of each tile's bytes, in tile order.
    ///
    /// Carried in the input as well as in the prompt's images so that the audit
    /// row can prove which frames a decision was made from, even after the
    /// derivative itself has been discarded.
    pub tile_hashes: Vec<String>,
}

impl SegmentInput {
    /// The local classifier's best guess, or `other`.
    #[must_use]
    pub fn best_guess(&self) -> &str {
        self.local_guesses
            .first()
            .map_or("other", |guess| guess.scene.as_str())
    }

    /// The best guess's confidence, 0.0 to 1.0.
    #[must_use]
    pub fn best_score(&self) -> f32 {
        f32::from(
            self.local_guesses
                .first()
                .map_or(0u16, |guess| guess.score_milli),
        ) / 1000.0
    }

    /// How long the segment lasted, in whole minutes.
    #[must_use]
    pub const fn minutes(&self) -> u32 {
        self.end_offset_s.saturating_sub(self.start_offset_s) / 60
    }

    /// The local guesses rendered for the prompt, in a fixed order.
    #[must_use]
    pub fn render_guesses(&self) -> String {
        self.local_guesses
            .iter()
            .take(3)
            .map(|guess| {
                format!(
                    "{} {:.2}",
                    guess.scene,
                    f32::from(guess.score_milli) / 1000.0
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// What the model must return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SegmentNamingOutput {
    /// A member of [`ALLOWED_SCENES`].
    pub scene: String,
    /// A member of [`ALLOWED_RITUALS`], or absent.
    #[serde(default)]
    pub ritual: Option<String>,
    /// A member of [`ALLOWED_TRADITIONS`], or absent.
    #[serde(default)]
    pub tradition: Option<String>,
    /// 0.0 to 1.0.
    pub confidence: f32,
    /// Up to five short reasons, each citing visible evidence.
    pub reasons: Vec<String>,
    /// Tile index where the scene appears to change.
    #[serde(default)]
    pub boundary_hint: Option<String>,
    /// Anything the vocabulary could not express.
    #[serde(default)]
    pub notes: Option<String>,
}

impl Scored for SegmentNamingOutput {
    fn confidence(&self) -> f32 {
        self.confidence.clamp(0.0, 1.0)
    }

    fn reasons(&self) -> &[String] {
        &self.reasons
    }
}

impl Validate for SegmentNamingOutput {
    /// The rules the JSON Schema cannot express.
    ///
    /// The schema proves `scene` is a string; only this can prove it is a string
    /// from the vocabulary phase 07 will condition its thresholds on. And only
    /// this can enforce invariant 2's "a decision without an explanation is a
    /// bug", because "confident answers must give reasons" is a relationship
    /// between two fields.
    fn validate(&self) -> Result<(), String> {
        let scenes: BTreeSet<&str> = ALLOWED_SCENES.iter().copied().collect();
        if !scenes.contains(self.scene.as_str()) {
            return Err(format!(
                "/scene: \"{}\" is not in allowed_scenes; use \"other\" and describe it in notes",
                self.scene
            ));
        }
        if let Some(ritual) = &self.ritual {
            let rituals: BTreeSet<&str> = ALLOWED_RITUALS.iter().copied().collect();
            if !rituals.contains(ritual.as_str()) {
                return Err(format!(
                    "/ritual: \"{ritual}\" is not in allowed_rituals; use \"other\" or null"
                ));
            }
        }
        if let Some(tradition) = &self.tradition {
            let traditions: BTreeSet<&str> = ALLOWED_TRADITIONS.iter().copied().collect();
            if !traditions.contains(tradition.as_str()) {
                return Err(format!(
                    "/tradition: \"{tradition}\" is not in allowed_traditions; use \"unclear\" or null"
                ));
            }
        }
        if self.confidence > REASONS_REQUIRED_ABOVE && self.reasons.is_empty() {
            return Err(format!(
                "/reasons: a decision at confidence {:.2} must give at least one reason",
                self.confidence
            ));
        }
        if self.reasons.iter().any(|reason| reason.trim().is_empty()) {
            return Err("/reasons: an empty reason is not a reason".to_string());
        }
        Ok(())
    }
}

/// The response schema, exactly as section 7 of the phase document specifies it.
///
/// Copied rather than generated. It is a contract with a model, the wording of
/// the constraints is part of what the model is told, and a generator that
/// reordered the keys would change the prompt hash and invalidate every cached
/// answer in the product.
pub const SEGMENT_NAMING_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["scene", "confidence", "reasons"],
  "properties": {
    "scene": { "type": "string" },
    "ritual": { "type": ["string", "null"] },
    "tradition": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 5 },
    "boundary_hint": { "type": ["string", "null"] },
    "notes": { "type": ["string", "null"] }
  },
  "additionalProperties": false
}"#;

/// The system prompt, exactly as section 7 of the phase document specifies it.
///
/// The third rule is the one that matters most and the one a rewrite would lose:
/// **never infer names, ethnicity or religion of individuals**. A model asked to
/// identify a ritual will, unprompted, volunteer what it thinks the people in the
/// frame are. That is not a capability this product wants at any confidence.
pub const SEGMENT_NAMING_SYSTEM: &str = "You are a wedding post-production analyst. You will \
receive a contact sheet of consecutive photographs from one wedding, a time range, and a local \
classifier's top guesses.\n\
Task: name the wedding scene, name the specific ritual or activity if present, and state which \
cultural tradition the visual evidence supports.\n\
Rules:\n\
- Judge only from visible evidence. If evidence is weak, say so with low confidence rather than \
guessing.\n\
- Use the controlled vocabulary supplied in `allowed_scenes` and `allowed_rituals`. If nothing \
fits, return \"other\" and describe it in `notes`.\n\
- Never infer names, ethnicity or religion of individuals; describe the ceremony type only.\n\
- Return ONLY JSON matching the provided schema. No prose, no markdown.";

/// Below this local confidence the segment is worth asking about.
///
/// Section 7: "one call per timeline segment whose local scene confidence < 0.75".
pub const ASK_BELOW_CONFIDENCE: f32 = 0.75;

/// At most one cloud call per this many images, across a whole wedding.
///
/// Section 6.1's rule, as a constant the caller can assert against: 3,000 images
/// divided by 40 is 75 calls, which is exactly section 11's budget.
pub const IMAGES_PER_CALL: u32 = 40;

/// Name the scene, ritual and tradition of one timeline segment.
///
/// Holds the built contact sheet rather than taking it in the input, because the
/// input is bound by `Hash` and a megabyte of JPEG in a hash key would be paid
/// for on every cache lookup. The sheet's content hash is in
/// [`SegmentInput::tile_hashes`] and in the prompt hash, so the cache key still
/// covers the pixels.
#[derive(Debug, Clone)]
pub struct SegmentNaming {
    sheet: ImagePart,
}

impl SegmentNaming {
    /// A task instance for one already-built contact sheet.
    #[must_use]
    pub const fn for_sheet(sheet: ImagePart) -> Self {
        Self { sheet }
    }

    /// The sheet this instance will send.
    #[must_use]
    pub const fn sheet(&self) -> &ImagePart {
        &self.sheet
    }

    /// True when a segment is uncertain enough to be worth the money.
    #[must_use]
    pub fn is_worth_asking(input: &SegmentInput) -> bool {
        input.best_score() < ASK_BELOW_CONFIDENCE
    }

    /// The user turn. Deterministic, with the fields in a fixed order.
    fn render_user(input: &SegmentInput) -> String {
        use std::fmt::Write as _;

        // `write!` into a `String` is infallible; the result is discarded rather
        // than unwrapped, which R1 bans even where it cannot fire.
        let mut out = String::with_capacity(1024);
        let _ = writeln!(
            out,
            "segment: {}\ntime range: {} s to {} s ({} minutes)\nframes in segment: {}",
            input.segment_id,
            input.start_offset_s,
            input.end_offset_s,
            input.minutes(),
            input.frame_count
        );
        let _ = writeln!(
            out,
            "local classifier top guesses: {}",
            input.render_guesses()
        );
        let _ = writeln!(out, "allowed_scenes: {}", ALLOWED_SCENES.join(", "));
        let _ = writeln!(out, "allowed_rituals: {}", ALLOWED_RITUALS.join(", "));
        let _ = writeln!(out, "allowed_traditions: {}", ALLOWED_TRADITIONS.join(", "));

        if input.exif.iter().any(|summary| !summary.is_empty()) {
            out.push_str("tiles, left to right and top to bottom:\n");
            for (index, summary) in input.exif.iter().enumerate() {
                let _ = writeln!(out, "  {index}: {}", summary.render());
            }
        }
        out.push_str(
            "Return only the JSON object. `boundary_hint` is the tile index where the scene \
             appears to change, or null.",
        );
        out
    }
}

impl CloudTask for SegmentNaming {
    const NAME: &'static str = "segment_naming";
    const VERSION: u16 = 1;
    type Input = SegmentInput;
    type Output = SegmentNamingOutput;

    fn prompt(&self, input: &Self::Input) -> PromptSpec {
        PromptSpec::new(SEGMENT_NAMING_SYSTEM, Self::render_user(input))
            .with_images(vec![self.sheet.clone()])
            // 512 is comfortably above the largest valid answer - a scene, a
            // ritual, a tradition, five short reasons and a note - and low enough
            // that a model which starts writing an essay is truncated into a
            // schema failure rather than billed for it.
            .with_max_tokens(512)
            .with_min_tier(Tier::Balanced)
    }

    fn output_schema(&self) -> &'static str {
        SEGMENT_NAMING_SCHEMA
    }

    /// The local answer: the classifier's own argmax, with its reasoning stated.
    ///
    /// It cannot fail. Phase 07's scene classifier is not built yet, so the
    /// guesses arrive in the input; when a segment has none at all the answer is
    /// `other` at confidence zero, which is honest and still lets the pipeline
    /// finish. That is what invariant 6 asks of a fallback - not that it be good,
    /// but that it always exist.
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError> {
        let scene = input.best_guess().to_string();
        let confidence = input.best_score();
        let reasons = if input.local_guesses.is_empty() {
            vec!["no local scene guess was available for this segment".to_string()]
        } else {
            vec![format!(
                "local scene classifier chose {scene} at {confidence:.2}; cloud reasoning was not \
                 available for this segment"
            )]
        };
        Ok(SegmentNamingOutput {
            scene,
            ritual: None,
            tradition: None,
            confidence,
            reasons,
            boundary_hint: None,
            notes: None,
        })
    }

    /// Two cents.
    ///
    /// The arithmetic, because it constrains the payload design and is easy to
    /// get wrong: a 4 x 3 sheet of 384 px tiles is 1536 x 1152, about 1.8
    /// megapixels, near 2,500 image tokens. With roughly 400 tokens of text and a
    /// 512 token ceiling on the answer, a balanced-tier call is about
    /// 2,900 x $3/M + 512 x $15/M, which is close to USD 0.016. Seventy-five of
    /// those is USD 1.20, inside section 11's USD 1.50 budget with room for the
    /// occasional repair.
    fn max_cost_usd(&self) -> f32 {
        0.02
    }
}
