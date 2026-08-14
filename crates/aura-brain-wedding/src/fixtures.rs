//! Synthetic weddings whose answer is known by construction.
//!
//! ## Why this module exists, stated plainly
//!
//! **The two shipped models are placeholders.** `scene_classifier` and
//! `ritual_classifier` 1.0.0 have the architecture of a multi-head adapter and none of
//! its training; run on a real photograph they produce a posterior that describes a
//! random projection. This is condition C1 of the phase 07 exit report and it is a
//! Sev 2 trigger.
//!
//! So section 10.1's gates - 0.92 top-1 accuracy, 45-second boundary error, 0.85 ritual
//! F1 per tradition - cannot be measured against the weights. They can be measured
//! against the *algorithms*, which is a different and still worthwhile claim: the
//! abstention rules, the emission mapping, the Viterbi path, the penalty search, the
//! merge pass, the key-frame filter and the confidence arithmetic are all exactly what
//! will run when a trained model lands, and every one of them can be wrong.
//!
//! This module builds timelines with a known answer, at a controllable noise level. A
//! gate measured here proves the harness would fail a model that had learned nothing -
//! see `the_gate_rejects_a_useless_classifier` in `tests/eval/scene_eval.rs`, which is
//! the same guard phase 06 wrote for its clustering metric and exists for the same
//! reason.
//!
//! ## What is synthetic and what is not
//!
//! The **posteriors** are synthetic. Everything else is real: the timestamps have real
//! gaps, the two-shooter fixture really interleaves two cameras' frames, the chapter
//! order is a real wedding's, and the durations come from a photographer's shot list
//! rather than from round numbers.
//!
//! Three weddings, matching section 14's three reference weddings:
//!
//! * [`hindu_night`] - an indoor Hindu night ceremony, the hardest light in the product;
//! * [`christian_day`] - an outdoor daylight Christian wedding, the easy case;
//! * [`nepali_mixed`] - a mixed-light Nepali reception, shot by two photographers.

use aura_core::contract::scene::{ChapterId, SceneId, SceneScore};

use crate::story::hmm::Observation;

/// One frame of a synthetic wedding, with the answer attached.
#[derive(Debug, Clone, PartialEq)]
pub struct TruthFrame {
    /// Milliseconds since the Unix epoch.
    pub ts: i64,
    /// What this frame really is.
    pub scene: SceneId,
    /// Which chapter it really belongs to.
    pub chapter: ChapterId,
    /// The rite in it, when there is one. A slug from a shipped taxonomy file.
    pub ritual: Option<&'static str>,
    /// Which camera body took it. Two shooters interleave.
    pub camera: u8,
}

/// A synthetic wedding.
#[derive(Debug, Clone, PartialEq)]
pub struct Truth {
    /// A name for the failure message.
    pub name: &'static str,
    /// The tradition, for the ritual gate's per-tradition breakdown.
    pub tradition: &'static str,
    /// Every frame, in timeline order.
    pub frames: Vec<TruthFrame>,
    /// The real chapter boundaries, as timestamps of the first frame of each chapter.
    pub boundaries: Vec<i64>,
}

impl Truth {
    /// How many chapters this wedding really has.
    #[must_use]
    pub fn chapters(&self) -> usize {
        self.boundaries.len()
    }

    /// Posteriors for every frame at a given accuracy, deterministically.
    ///
    /// `accuracy` is the share of frames whose true class gets the dominant mass; the
    /// rest get their mass on a **plausible confusion** rather than on a random class.
    /// That distinction is the whole value of the fixture: a classifier's errors are
    /// not uniform, and a smoother tuned against uniform noise would look far better
    /// here than it will on a wedding. So `ceremony` is confused with `vows`,
    /// `dance_floor` with `first_dance`, `details` with `venue` - neighbours in the
    /// same or an adjacent chapter, which is what a real head does and what actually
    /// tests the transition matrix.
    #[must_use]
    pub fn observations(&self, accuracy: f32, seed: u64) -> Vec<Observation> {
        let mut state = seed | 1;
        let day_start = self.frames.first().map_or(0, |frame| frame.ts);

        self.frames
            .iter()
            .map(|frame| {
                let roll = next_unit(&mut state);
                let correct = roll < accuracy;
                let (top, second) = if correct {
                    (frame.scene, confuse(frame.scene))
                } else {
                    (confuse(frame.scene), frame.scene)
                };
                // 0.55 / 0.28 / 0.10 is a realistic well-calibrated 22-way head: above
                // `classifier::MIN_CONFIDENCE`, and with a 0.27 margin that clears
                // `MIN_MARGIN` comfortably. The confused case inverts the top two, so
                // it clears the margin too - which is the point. A fixture whose errors
                // were all low-confidence would be tested by the abstention rule rather
                // than by the smoother.
                Observation {
                    top3: [
                        SceneScore {
                            scene: top,
                            score: 0.55,
                        },
                        SceneScore {
                            scene: second,
                            score: 0.28,
                        },
                        SceneScore {
                            scene: SceneId::Candid,
                            score: 0.10,
                        },
                    ],
                    offset_ms: frame.ts.saturating_sub(day_start),
                }
            })
            .collect()
    }

    /// Posteriors from a classifier that has learned nothing.
    ///
    /// Every frame gets the same distribution. A gate that passes on this is a gate
    /// that is not measuring anything, and `tests/eval/scene_eval.rs` asserts that
    /// every one of section 10.1's thresholds fails here.
    #[must_use]
    pub fn useless_observations(&self) -> Vec<Observation> {
        let day_start = self.frames.first().map_or(0, |frame| frame.ts);
        self.frames
            .iter()
            .map(|frame| Observation {
                top3: [
                    SceneScore {
                        scene: SceneId::Candid,
                        score: 0.05,
                    },
                    SceneScore {
                        scene: SceneId::Details,
                        score: 0.05,
                    },
                    SceneScore {
                        scene: SceneId::Venue,
                        score: 0.05,
                    },
                ],
                offset_ms: frame.ts.saturating_sub(day_start),
            })
            .collect()
    }
}

/// The plausible confusion for a scene.
///
/// Hand-authored, and every pair is one a photographer would also hesitate over. Not a
/// random neighbour: the whole point is that a real head's errors cluster inside a
/// chapter, which is precisely what the HMM cannot fix and what the accuracy gate must
/// therefore actually measure.
#[must_use]
// One arm per scene, even where two scenes are confused with the same third. This is a
// lookup table a photographer is expected to read and argue with, and merging the
// patterns would hide which scenes are covered.
#[allow(clippy::match_same_arms)]
pub const fn confuse(scene: SceneId) -> SceneId {
    match scene {
        SceneId::GettingReadyBride => SceneId::GettingReadyGroom,
        SceneId::GettingReadyGroom => SceneId::GettingReadyBride,
        SceneId::Details => SceneId::Venue,
        SceneId::Venue => SceneId::Details,
        SceneId::FirstLook => SceneId::CouplePortrait,
        SceneId::CeremonyEntrance => SceneId::Ceremony,
        SceneId::Ceremony => SceneId::Vows,
        SceneId::Vows => SceneId::Ceremony,
        SceneId::Ritual => SceneId::Ceremony,
        SceneId::Rings => SceneId::Vows,
        SceneId::Kiss => SceneId::Ceremony,
        SceneId::FamilyPortrait => SceneId::GroupPortrait,
        SceneId::GroupPortrait => SceneId::FamilyPortrait,
        SceneId::CouplePortrait => SceneId::GoldenHour,
        SceneId::GoldenHour => SceneId::CouplePortrait,
        SceneId::ReceptionEntrance => SceneId::DanceFloor,
        SceneId::Speeches => SceneId::Candid,
        SceneId::Cake => SceneId::Speeches,
        SceneId::FirstDance => SceneId::DanceFloor,
        SceneId::DanceFloor => SceneId::FirstDance,
        SceneId::Candid => SceneId::Speeches,
        SceneId::Exit => SceneId::DanceFloor,
        SceneId::Unknown => SceneId::Candid,
    }
}

/// One chapter of a synthetic day.
struct Block {
    scene: SceneId,
    chapter: ChapterId,
    ritual: Option<&'static str>,
    /// How long the chapter lasts, in minutes.
    minutes: i64,
    /// How many frames the photographer took in it.
    frames: usize,
    /// Minutes of travel or downtime before this chapter starts.
    gap_before: i64,
}

/// Turn a list of blocks into a timeline.
fn build(name: &'static str, tradition: &'static str, start_ms: i64, blocks: &[Block]) -> Truth {
    let mut frames = Vec::new();
    let mut boundaries = Vec::new();
    let mut at = start_ms;

    for block in blocks {
        at += block.gap_before * 60_000;
        boundaries.push(at);
        let span = (block.minutes * 60_000).max(1);
        let step = span / i64::try_from(block.frames.max(1)).unwrap_or(1);
        for index in 0..block.frames {
            frames.push(TruthFrame {
                ts: at + step * i64::try_from(index).unwrap_or(0),
                scene: block.scene,
                chapter: block.chapter,
                ritual: block.ritual,
                camera: 1,
            });
        }
        at += span;
    }

    Truth {
        name,
        tradition,
        frames,
        boundaries,
    }
}

/// The base timestamp every fixture starts from.
///
/// 2026-03-14T08:00:00Z as epoch milliseconds. A fixed date rather than "now", because
/// invariant 4 requires two machines to agree and the wall clock is banned here.
// DETERMINISM: the sentence above names the banned call in prose to explain why the
// constant exists; nothing in this module reads a clock.
pub const DAY_START_MS: i64 = 1_773_475_200_000;

/// An indoor Hindu night ceremony. The hardest light in the product.
///
/// Section 14's first reference wedding. Ten chapters, a long mandap ceremony after
/// dark, and a 35-minute travel gap between the getting-ready venue and the mandap -
/// which is the hard boundary `changepoint::HARD_GAP_MS` exists for, appearing here in
/// its natural habitat rather than as a contrived test case.
#[must_use]
pub fn hindu_night() -> Truth {
    build(
        "hindu night ceremony",
        "hindu",
        DAY_START_MS,
        &[
            Block {
                scene: SceneId::Details,
                chapter: ChapterId::Details,
                ritual: None,
                minutes: 40,
                frames: 46,
                gap_before: 0,
            },
            Block {
                scene: SceneId::GettingReadyBride,
                chapter: ChapterId::GettingReady,
                ritual: None,
                minutes: 95,
                frames: 210,
                gap_before: 5,
            },
            Block {
                scene: SceneId::Ritual,
                chapter: ChapterId::Rituals,
                ritual: Some("haldi"),
                minutes: 55,
                frames: 130,
                gap_before: 10,
            },
            // The travel gap. Thirty-five minutes with no frame at all.
            Block {
                scene: SceneId::CeremonyEntrance,
                chapter: ChapterId::Ceremony,
                ritual: Some("baraat"),
                minutes: 45,
                frames: 190,
                gap_before: 35,
            },
            Block {
                scene: SceneId::Ritual,
                chapter: ChapterId::Rituals,
                ritual: Some("jaimala_varmala"),
                minutes: 25,
                frames: 120,
                gap_before: 0,
            },
            Block {
                scene: SceneId::Ceremony,
                chapter: ChapterId::Ceremony,
                ritual: None,
                minutes: 70,
                frames: 240,
                gap_before: 0,
            },
            Block {
                scene: SceneId::Ritual,
                chapter: ChapterId::Rituals,
                ritual: Some("saptapadi_pheras"),
                minutes: 30,
                frames: 145,
                gap_before: 0,
            },
            Block {
                scene: SceneId::Ritual,
                chapter: ChapterId::Rituals,
                ritual: Some("sindoor_mangalsutra"),
                minutes: 12,
                frames: 64,
                gap_before: 0,
            },
            Block {
                scene: SceneId::FamilyPortrait,
                chapter: ChapterId::Portraits,
                ritual: None,
                minutes: 40,
                frames: 155,
                gap_before: 8,
            },
            Block {
                scene: SceneId::DanceFloor,
                chapter: ChapterId::Dance,
                ritual: None,
                minutes: 110,
                frames: 320,
                gap_before: 22,
            },
            Block {
                scene: SceneId::Exit,
                chapter: ChapterId::Exit,
                ritual: Some("vidaai"),
                minutes: 18,
                frames: 72,
                gap_before: 0,
            },
        ],
    )
}

/// An outdoor daylight Christian wedding. The easy case, and it must not be assumed.
///
/// Section 14's second reference wedding. Nine chapters, good light throughout, and a
/// short ceremony - which is the *other* failure this fixture guards: a penalty tuned
/// on the Hindu wedding above would give this one three chapters, and section 10.1
/// forbids fewer than six.
// A shot list, not code. Thirteen chapters is thirteen lines of `Block` and the
// alternative - splitting the list across two functions - would make it harder to read
// as the one thing it is: a photographer's day.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn christian_day() -> Truth {
    build(
        "christian daylight wedding",
        "christian",
        DAY_START_MS + 3_600_000,
        &[
            Block {
                scene: SceneId::Details,
                chapter: ChapterId::Details,
                ritual: None,
                minutes: 35,
                frames: 40,
                gap_before: 0,
            },
            Block {
                scene: SceneId::GettingReadyBride,
                chapter: ChapterId::GettingReady,
                ritual: None,
                minutes: 80,
                frames: 165,
                gap_before: 5,
            },
            Block {
                scene: SceneId::FirstLook,
                chapter: ChapterId::Ceremony,
                ritual: None,
                minutes: 18,
                frames: 85,
                gap_before: 25,
            },
            Block {
                scene: SceneId::CeremonyEntrance,
                chapter: ChapterId::Ceremony,
                ritual: Some("processional"),
                minutes: 12,
                frames: 70,
                gap_before: 20,
            },
            Block {
                scene: SceneId::Vows,
                chapter: ChapterId::Ceremony,
                ritual: Some("vows"),
                minutes: 14,
                frames: 68,
                gap_before: 0,
            },
            Block {
                scene: SceneId::Rings,
                chapter: ChapterId::Rituals,
                ritual: Some("ring_exchange"),
                minutes: 6,
                frames: 38,
                gap_before: 0,
            },
            Block {
                scene: SceneId::FamilyPortrait,
                chapter: ChapterId::Portraits,
                ritual: None,
                minutes: 45,
                frames: 175,
                gap_before: 12,
            },
            Block {
                scene: SceneId::CouplePortrait,
                chapter: ChapterId::Portraits,
                ritual: None,
                minutes: 30,
                frames: 140,
                gap_before: 0,
            },
            Block {
                scene: SceneId::Speeches,
                chapter: ChapterId::Reception,
                ritual: None,
                minutes: 55,
                frames: 130,
                gap_before: 30,
            },
            Block {
                scene: SceneId::Cake,
                chapter: ChapterId::Reception,
                ritual: None,
                minutes: 10,
                frames: 42,
                gap_before: 0,
            },
            Block {
                scene: SceneId::FirstDance,
                chapter: ChapterId::Dance,
                ritual: None,
                minutes: 8,
                frames: 55,
                gap_before: 0,
            },
            Block {
                scene: SceneId::DanceFloor,
                chapter: ChapterId::Dance,
                ritual: None,
                minutes: 95,
                frames: 260,
                gap_before: 0,
            },
            Block {
                scene: SceneId::Exit,
                chapter: ChapterId::Exit,
                ritual: None,
                minutes: 6,
                frames: 44,
                gap_before: 0,
            },
        ],
    )
}

/// A mixed-light Nepali reception, shot by two photographers.
///
/// Section 14's third reference wedding, and it carries section 10.1's two-shooter
/// regression: [`interleave`] merges a second camera's frames into the same timeline,
/// so consecutive frames alternate between two bodies with different exposure and
/// slightly different clocks. A segmenter that keyed on camera identity or on
/// frame-to-frame appearance alone fragments this wedding into thirty chapters.
#[must_use]
pub fn nepali_mixed() -> Truth {
    let primary = build(
        "nepali mixed-light reception",
        "nepali",
        DAY_START_MS + 7_200_000,
        &[
            Block {
                scene: SceneId::Details,
                chapter: ChapterId::Details,
                ritual: None,
                minutes: 30,
                frames: 36,
                gap_before: 0,
            },
            Block {
                scene: SceneId::GettingReadyBride,
                chapter: ChapterId::GettingReady,
                ritual: Some("mehendi_nepali"),
                minutes: 70,
                frames: 150,
                gap_before: 5,
            },
            Block {
                scene: SceneId::CeremonyEntrance,
                chapter: ChapterId::Ceremony,
                ritual: Some("jantee"),
                minutes: 40,
                frames: 165,
                gap_before: 30,
            },
            Block {
                scene: SceneId::Ritual,
                chapter: ChapterId::Rituals,
                ritual: Some("swayambar"),
                minutes: 22,
                frames: 110,
                gap_before: 0,
            },
            Block {
                scene: SceneId::Ritual,
                chapter: ChapterId::Rituals,
                ritual: Some("saat_phera"),
                minutes: 35,
                frames: 150,
                gap_before: 0,
            },
            Block {
                scene: SceneId::GroupPortrait,
                chapter: ChapterId::Portraits,
                ritual: None,
                minutes: 35,
                frames: 130,
                gap_before: 10,
            },
            Block {
                scene: SceneId::ReceptionEntrance,
                chapter: ChapterId::Reception,
                ritual: None,
                minutes: 15,
                frames: 70,
                gap_before: 25,
            },
            Block {
                scene: SceneId::Speeches,
                chapter: ChapterId::Reception,
                ritual: None,
                minutes: 50,
                frames: 120,
                gap_before: 0,
            },
            Block {
                scene: SceneId::DanceFloor,
                chapter: ChapterId::Dance,
                ritual: None,
                minutes: 105,
                frames: 290,
                gap_before: 0,
            },
            Block {
                scene: SceneId::Exit,
                chapter: ChapterId::Exit,
                ritual: None,
                minutes: 12,
                frames: 50,
                gap_before: 0,
            },
        ],
    );
    interleave(primary)
}

/// Add a second photographer to a wedding.
///
/// Every second frame becomes camera 2, offset by a few seconds - which is what two
/// bodies aligned by phase 01's clock offset actually look like. The chapter answer is
/// unchanged: two photographers at one wedding are at the same wedding, and a story
/// that says otherwise is the bug this fixture exists to catch.
#[must_use]
pub fn interleave(mut truth: Truth) -> Truth {
    for (index, frame) in truth.frames.iter_mut().enumerate() {
        if index % 2 == 1 {
            frame.camera = 2;
            // Three seconds of residual clock skew after alignment. Small enough that
            // the order is preserved, large enough that a segmenter comparing
            // consecutive timestamps sees a jitter rather than a clean sequence.
            frame.ts += 3_000;
        }
    }
    truth.frames.sort_by_key(|frame| frame.ts);
    truth
}

/// The three reference weddings, in the order section 14 lists them.
#[must_use]
pub fn all() -> Vec<Truth> {
    vec![hindu_night(), christian_day(), nepali_mixed()]
}

/// A 64-bit xorshift, so two machines produce the same fixture.
///
/// The same generator `aura_infer::onnx::fixtures` uses, and for the same reason: a
/// fixture whose bytes changed when a dependency updated its random-number crate would
/// break a gate on every machine at once.
fn next_unit(state: &mut u64) -> f32 {
    let mut value = *state;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *state = value;
    let bits = value.wrapping_mul(0x2545_f491_4f6c_dd1d);
    (bits >> 40) as f32 / 16_777_216.0
}
