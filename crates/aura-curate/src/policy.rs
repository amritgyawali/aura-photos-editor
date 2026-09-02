//! The product manager's table, and everything it may not say.
//!
//! `config/curation.toml` decides album sizes, chapter importance, rhythm patterns, the five hero
//! weights, the five monochrome weights, the legibility weights and the two set sizes. It may
//! **tighten** a bound the contract owns and may never widen one, which is phase 21's rule in its
//! sixth application, and the pass halts rather than falling back on defaults - `AURA-ML-5145`.
//!
//! # The scan
//!
//! [`Policy::load_str`] refuses any key whose name suggests a skin target, as phases 15, 25 and 27
//! scan their own schemas for one. The band a monochrome mix protects is looked up per identity
//! from phase 15's measured loci; a configurable one would be the constant `docs/skin-fairness.md`
//! says this product does not have, and it would be the single easiest thing in this file to add.

use std::collections::BTreeMap;

use aura_core::contract::curate::{
    ShotScale, ALBUM_MAX, ALBUM_MIN, BW_CANDIDATE_FLOOR, HERO_TECHNICAL_FLOOR,
    MAX_HEROES_PER_CHAPTER, MAX_PAIR_SIMILARITY, MAX_PAIR_TONAL_GAP, TEASER_MAX, TEASER_MIN,
};
use aura_core::contract::scene::ChapterId;
use aura_core::AuraResult;
use serde::Deserialize;

use crate::errors::policy_refused;

/// The shipped table, compiled in so a build can never disagree with its own defaults.
pub const DEFAULT_TOML: &str = include_str!("../config/curation.toml");

/// Words that must not appear in a key. A key naming a skin target is what this exists to stop.
const FORBIDDEN_KEY_FRAGMENTS: [&str; 6] = [
    "skin_target",
    "skin_luma",
    "ideal_skin",
    "skin_reference",
    "target_skin",
    "skin_band",
];

/// The five hero weights.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct HeroWeights {
    /// Phase 09's technical score. Light, because it is already a veto.
    pub technical: f32,
    /// Phase 10's emotion score. The heaviest: a portfolio is how a photographer is hired.
    pub emotion: f32,
    /// Phase 11's composition score.
    pub composition: f32,
    /// Distance from the heroes already chosen, in the phase 05 index.
    pub uniqueness: f32,
    /// How much the moment matters to the story.
    pub story: f32,
}

impl HeroWeights {
    /// The five, in the order the panel lists them.
    #[must_use]
    pub fn all(&self) -> [(&'static str, f32); 5] {
        [
            ("technical", self.technical),
            ("emotion", self.emotion),
            ("composition", self.composition),
            ("uniqueness", self.uniqueness),
            ("story", self.story),
        ]
    }
}

/// The five monochrome weights.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct BwWeights {
    /// How far apart the tones stay once the colour is gone. The heaviest: it decides whether the
    /// conversion works at all.
    pub tonal_separation: f32,
    /// How much saturated colour away from the subject is pulling the eye.
    pub colour_distraction: f32,
    /// How strongly the frame is carried by what people are doing.
    pub gesture: f32,
    /// Phase 10's emotion score.
    pub emotion: f32,
    /// How well the frame's noise would read as grain.
    pub grain: f32,
}

impl BwWeights {
    /// The five, in the order the panel lists them.
    #[must_use]
    pub fn all(&self) -> [(&'static str, f32); 5] {
        [
            ("tonal_separation", self.tonal_separation),
            ("colour_distraction", self.colour_distraction),
            ("gesture", self.gesture),
            ("emotion", self.emotion),
            ("grain", self.grain),
        ]
    }
}

/// The four terms of a spread's pairing score.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct PairingWeights {
    /// Difference in tonal weight. The heaviest, because it is what a photographer notices first.
    pub tonal: f32,
    /// Difference in colour temperature after phase 25's normalisation.
    pub warmth: f32,
    /// Whether the subjects face inward.
    pub direction: f32,
    /// Whether the two frames are interestingly different.
    pub variety: f32,
}

/// The four terms of thumbnail legibility.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct LegibilityWeights {
    /// How much of the frame the subject occupies. The only thing that survives at 150 px.
    pub subject_size: f32,
    /// How sharp the subject is.
    pub subject_sharp: f32,
    /// How little clutter there is. Context at full size, noise at thumbnail size.
    pub uncluttered: f32,
    /// How much of the frame is empty.
    pub negative_space: f32,
}

/// Everything a product manager decides about curation.
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    /// Which revision of the table this is. Stored on the run.
    pub policy_ver: u16,
    /// The default album size a photographer gets without asking.
    pub album_default: u32,
    /// The smallest album this studio composes. Never below [`ALBUM_MIN`].
    pub album_min: u32,
    /// The largest. Never above [`ALBUM_MAX`].
    pub album_max: u32,
    /// How many passes the optimiser makes.
    pub swap_passes: u32,
    /// The pairing objective's four terms.
    pub pairing: PairingWeights,
    /// How much a minute of each chapter is worth, before duration is taken into account.
    pub chapter_importance: BTreeMap<ChapterId, f32>,
    /// The target shot-scale pattern per chapter, repeated cyclically.
    pub rhythm: BTreeMap<ChapterId, Vec<ShotScale>>,
    /// How many heroes.
    pub hero_target: u32,
    /// The most one chapter may contribute. Never above [`MAX_HEROES_PER_CHAPTER`].
    pub heroes_per_chapter: u32,
    /// The most heroes that may share one measured shot scale, as a share.
    pub hero_scale_share: f32,
    /// The technical veto. Never below [`HERO_TECHNICAL_FLOOR`].
    pub hero_technical_floor: f32,
    /// The five hero weights.
    pub hero_weights: HeroWeights,
    /// The lowest suitability at which a monochrome candidate is offered. Never below
    /// [`BW_CANDIDATE_FLOOR`].
    pub bw_candidate_floor: f32,
    /// Below this tonal separation a frame is never offered as monochrome.
    pub bw_flat_floor: f32,
    /// The five monochrome weights.
    pub bw_weights: BwWeights,
    /// How many frames the story set holds.
    pub story_size: u32,
    /// The four legibility terms.
    pub legibility: LegibilityWeights,
    /// How many frames the teaser holds. Inside [`TEASER_MIN`] and [`TEASER_MAX`].
    pub teaser_size: u32,
}

impl Default for Policy {
    /// The shipped table.
    ///
    /// Parsing the compiled-in file rather than repeating its numbers in Rust, because two copies of
    /// a product decision is two product decisions by the third release. A file this build ships
    /// that this build cannot parse is a build failure, so the fallback here is unreachable in a
    /// shipped binary and the test below asserts it.
    fn default() -> Self {
        Self::load_str(DEFAULT_TOML).unwrap_or_else(|_| Self::minimal())
    }
}

impl Policy {
    /// Load and check a table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5145` when the file cannot be parsed, when it widens a bound the contract owns, when
    /// a weight set is degenerate, when a rhythm pattern names something that is not a shot scale,
    /// or when any key names a skin target.
    pub fn load_str(text: &str) -> AuraResult<Self> {
        for fragment in FORBIDDEN_KEY_FRAGMENTS {
            if strip_comments(text).contains(fragment) {
                return Err(policy_refused(
                    fragment,
                    "the band a monochrome mix protects is measured per person from phase 15's \
                     skin loci; this file may not name one",
                ));
            }
        }

        let raw: RawPolicy = toml::from_str(text)
            .map_err(|err| policy_refused("<file>", format!("could not be parsed: {err}")))?;

        let album_min = raw.album.min_size;
        let album_max = raw.album.max_size;
        if album_min < ALBUM_MIN {
            return Err(policy_refused(
                "album.min_size",
                format!("{album_min} is below the contract's {ALBUM_MIN}"),
            ));
        }
        if album_max > ALBUM_MAX {
            return Err(policy_refused(
                "album.max_size",
                format!("{album_max} is above the contract's {ALBUM_MAX}"),
            ));
        }
        if album_min > album_max {
            return Err(policy_refused(
                "album.min_size",
                format!("{album_min} is above album.max_size {album_max}"),
            ));
        }
        if raw.album.default_size < album_min || raw.album.default_size > album_max {
            return Err(policy_refused(
                "album.default_size",
                format!(
                    "{} is outside {album_min}..={album_max}",
                    raw.album.default_size
                ),
            ));
        }
        if raw.album.swap_passes == 0 {
            return Err(policy_refused(
                "album.swap_passes",
                "zero passes would leave the sequence unoptimised while reporting a rhythm score",
            ));
        }

        if raw.hero.per_chapter > MAX_HEROES_PER_CHAPTER {
            return Err(policy_refused(
                "hero.per_chapter",
                format!(
                    "{} is above the contract's {MAX_HEROES_PER_CHAPTER}",
                    raw.hero.per_chapter
                ),
            ));
        }
        if raw.hero.technical_floor < HERO_TECHNICAL_FLOOR {
            return Err(policy_refused(
                "hero.technical_floor",
                format!(
                    "{} is below the contract's {HERO_TECHNICAL_FLOOR}; a studio may demand sharper \
                     portfolio work and never softer",
                    raw.hero.technical_floor
                ),
            ));
        }
        if raw.hero.target == 0 {
            return Err(policy_refused("hero.target", "a portfolio of zero"));
        }
        if !(0.1..=1.0).contains(&raw.hero.scale_share) {
            return Err(policy_refused(
                "hero.scale_share",
                format!("{} is outside 0.1..=1.0", raw.hero.scale_share),
            ));
        }
        check_weights("hero.weights", &raw.hero.weights.all())?;

        if raw.bw.candidate_floor < BW_CANDIDATE_FLOOR {
            return Err(policy_refused(
                "bw.candidate_floor",
                format!(
                    "{} is below the contract's {BW_CANDIDATE_FLOOR}",
                    raw.bw.candidate_floor
                ),
            ));
        }
        if !(0.0..=1.0).contains(&raw.bw.flat_floor) {
            return Err(policy_refused(
                "bw.flat_floor",
                format!("{} is outside 0..=1", raw.bw.flat_floor),
            ));
        }
        check_weights("bw.weights", &raw.bw.weights.all())?;

        check_weights(
            "album.pairing",
            &[
                ("tonal", raw.album.pairing.tonal),
                ("warmth", raw.album.pairing.warmth),
                ("direction", raw.album.pairing.direction),
                ("variety", raw.album.pairing.variety),
            ],
        )?;
        check_weights(
            "social.legibility",
            &[
                ("subject_size", raw.social.legibility.subject_size),
                ("subject_sharp", raw.social.legibility.subject_sharp),
                ("uncluttered", raw.social.legibility.uncluttered),
                ("negative_space", raw.social.legibility.negative_space),
            ],
        )?;

        if raw.teaser.size < TEASER_MIN || raw.teaser.size > TEASER_MAX {
            return Err(policy_refused(
                "teaser.size",
                format!(
                    "{} is outside the contract's {TEASER_MIN}..={TEASER_MAX}",
                    raw.teaser.size
                ),
            ));
        }
        if raw.social.story_size == 0 {
            return Err(policy_refused("social.story_size", "a story set of zero"));
        }

        let mut chapter_importance = BTreeMap::new();
        let mut rhythm = BTreeMap::new();
        for chapter in ChapterId::ALL {
            let key = chapter.as_str();
            let Some(weight) = raw.chapters.get(key).copied() else {
                return Err(policy_refused(
                    "chapters",
                    format!("no importance for `{key}`; every chapter needs one"),
                ));
            };
            if !weight.is_finite() || weight <= 0.0 {
                return Err(policy_refused(
                    "chapters",
                    format!("`{key}` is {weight}, which would allocate it no pages at all"),
                ));
            }
            chapter_importance.insert(chapter, weight);

            let Some(pattern) = raw.rhythm.get(key) else {
                return Err(policy_refused(
                    "rhythm",
                    format!("no pattern for `{key}`; every chapter needs one"),
                ));
            };
            if pattern.is_empty() {
                return Err(policy_refused(
                    "rhythm",
                    format!("`{key}` has an empty pattern, so nothing could ever match it"),
                ));
            }
            let mut scales = Vec::with_capacity(pattern.len());
            for token in pattern {
                let scale = ShotScale::parse(token);
                if !scale.is_known() {
                    return Err(policy_refused(
                        "rhythm",
                        format!(
                            "`{key}` names `{token}`, which is not one of wide, medium or tight"
                        ),
                    ));
                }
                scales.push(scale);
            }
            rhythm.insert(chapter, scales);
        }

        Ok(Self {
            policy_ver: raw.policy_ver,
            album_default: raw.album.default_size,
            album_min,
            album_max,
            swap_passes: raw.album.swap_passes,
            pairing: raw.album.pairing,
            chapter_importance,
            rhythm,
            hero_target: raw.hero.target,
            heroes_per_chapter: raw.hero.per_chapter,
            hero_scale_share: raw.hero.scale_share,
            hero_technical_floor: raw.hero.technical_floor,
            hero_weights: raw.hero.weights,
            bw_candidate_floor: raw.bw.candidate_floor,
            bw_flat_floor: raw.bw.flat_floor,
            bw_weights: raw.bw.weights,
            story_size: raw.social.story_size,
            legibility: raw.social.legibility,
            teaser_size: raw.teaser.size,
        })
    }

    /// The hard-coded skeleton, reached only if the compiled-in file fails to parse.
    ///
    /// Unreachable in a shipped binary - `the_shipped_table_parses` fails the build otherwise - and
    /// present because `Default` cannot return an error and a `panic!` in a library is banned.
    fn minimal() -> Self {
        let mut chapter_importance = BTreeMap::new();
        let mut rhythm = BTreeMap::new();
        for chapter in ChapterId::ALL {
            chapter_importance.insert(chapter, 1.0);
            rhythm.insert(
                chapter,
                vec![ShotScale::Wide, ShotScale::Medium, ShotScale::Tight],
            );
        }
        Self {
            policy_ver: 0,
            album_default: aura_core::contract::curate::ALBUM_DEFAULT,
            album_min: ALBUM_MIN,
            album_max: ALBUM_MAX,
            swap_passes: 1,
            pairing: PairingWeights {
                tonal: 0.4,
                warmth: 0.25,
                direction: 0.2,
                variety: 0.15,
            },
            chapter_importance,
            rhythm,
            hero_target: aura_core::contract::curate::HERO_TARGET,
            heroes_per_chapter: MAX_HEROES_PER_CHAPTER,
            hero_scale_share: 0.6,
            hero_technical_floor: HERO_TECHNICAL_FLOOR,
            hero_weights: HeroWeights {
                technical: 0.12,
                emotion: 0.34,
                composition: 0.24,
                uniqueness: 0.18,
                story: 0.12,
            },
            bw_candidate_floor: BW_CANDIDATE_FLOOR,
            bw_flat_floor: 0.30,
            bw_weights: BwWeights {
                tonal_separation: 0.38,
                colour_distraction: 0.24,
                gesture: 0.16,
                emotion: 0.14,
                grain: 0.08,
            },
            story_size: 8,
            legibility: LegibilityWeights {
                subject_size: 0.4,
                subject_sharp: 0.25,
                uncluttered: 0.2,
                negative_space: 0.15,
            },
            teaser_size: 18,
        }
    }

    /// How much a minute of this chapter is worth. Unknown chapters read as
    /// [`ChapterId::Other`]'s weight, which is the least this table can say about a chapter.
    #[must_use]
    pub fn importance(&self, chapter: ChapterId) -> f32 {
        self.chapter_importance
            .get(&chapter)
            .copied()
            .unwrap_or(0.5)
    }

    /// The target pattern for this chapter.
    #[must_use]
    pub fn pattern(&self, chapter: ChapterId) -> &[ShotScale] {
        self.rhythm.get(&chapter).map_or(&[], Vec::as_slice)
    }

    /// The largest tonal gap a pair may have. The contract's, and a studio may only tighten it.
    #[must_use]
    pub const fn max_tonal_gap(&self) -> f32 {
        MAX_PAIR_TONAL_GAP
    }

    /// The largest similarity a facing pair may have. The contract's, and there is deliberately no
    /// configuration that widens it: two versions of the same photograph never face each other.
    #[must_use]
    pub const fn max_similarity(&self) -> f32 {
        MAX_PAIR_SIMILARITY
    }
}

/// A weight set has to be finite, non-negative and sum to something usable.
///
/// Not "sum to exactly 1.0": a product manager editing five numbers by hand will produce 0.99, and
/// refusing that would make the file harder to own than it needs to be. The weights are renormalised
/// when they are used, so what actually matters is that they are not degenerate.
fn check_weights(key: &str, weights: &[(&str, f32)]) -> AuraResult<()> {
    let mut total = 0.0f32;
    for (name, value) in weights {
        if !value.is_finite() || *value < 0.0 {
            return Err(policy_refused(
                key,
                format!("`{name}` is {value}; a weight is finite and not negative"),
            ));
        }
        total += *value;
    }
    if total <= f32::EPSILON {
        return Err(policy_refused(
            key,
            "every weight is zero, so the score would be a constant",
        ));
    }
    Ok(())
}

/// Strip `#` comments before scanning for a forbidden key.
///
/// Phase 27 found this twice in one phase: a grep that reads documentation as if it were code fails
/// hardest on the codebases that document themselves best, and this file's whole purpose is to
/// explain in prose why there is no skin target in it. Without this, the sentence saying so would
/// be the thing that failed the check.
fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// The wire shape of the file
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawPolicy {
    policy_ver: u16,
    album: RawAlbum,
    chapters: BTreeMap<String, f32>,
    rhythm: BTreeMap<String, Vec<String>>,
    hero: RawHero,
    bw: RawBw,
    social: RawSocial,
    teaser: RawTeaser,
}

#[derive(Debug, Deserialize)]
struct RawAlbum {
    default_size: u32,
    min_size: u32,
    max_size: u32,
    swap_passes: u32,
    pairing: PairingWeights,
}

#[derive(Debug, Deserialize)]
struct RawHero {
    target: u32,
    per_chapter: u32,
    scale_share: f32,
    technical_floor: f32,
    weights: HeroWeights,
}

#[derive(Debug, Deserialize)]
struct RawBw {
    candidate_floor: f32,
    flat_floor: f32,
    weights: BwWeights,
}

#[derive(Debug, Deserialize)]
struct RawSocial {
    story_size: u32,
    legibility: LegibilityWeights,
}

#[derive(Debug, Deserialize)]
struct RawTeaser {
    size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_parses_and_covers_every_chapter() {
        let policy = Policy::load_str(DEFAULT_TOML).expect("the shipped table must parse");
        assert_eq!(policy.policy_ver, 1);
        for chapter in ChapterId::ALL {
            assert!(policy.importance(chapter) > 0.0, "{chapter:?}");
            assert!(!policy.pattern(chapter).is_empty(), "{chapter:?}");
        }
        // `Default` must be the shipped table rather than the skeleton.
        assert_eq!(Policy::default(), policy);
    }

    #[test]
    fn a_widened_bound_is_refused_and_a_tightened_one_is_not() {
        let widened = DEFAULT_TOML.replace("max_size = 120", "max_size = 200");
        assert!(Policy::load_str(&widened).is_err());

        let tightened = DEFAULT_TOML.replace("max_size = 120", "max_size = 100");
        assert!(Policy::load_str(&tightened).is_ok());

        let softer = DEFAULT_TOML.replace("technical_floor = 0.55", "technical_floor = 0.20");
        assert!(
            Policy::load_str(&softer).is_err(),
            "a studio may demand sharper portfolio work and never softer"
        );
        let sharper = DEFAULT_TOML.replace("technical_floor = 0.55", "technical_floor = 0.75");
        assert!(Policy::load_str(&sharper).is_ok());
    }

    #[test]
    fn a_key_naming_a_skin_target_is_refused() {
        let with_target = format!("{DEFAULT_TOML}\n[skin]\nskin_target = 0.62\n");
        let err = Policy::load_str(&with_target).unwrap_err();
        assert_eq!(err.code.0, "AURA-ML-5145");
    }

    #[test]
    fn the_scan_reads_code_rather_than_the_prose_explaining_it() {
        // Phase 27's lesson, twice over: a check that reads documentation as if it were code fails
        // hardest on the codebases that document themselves best - and this file's header is four
        // paragraphs about why there is no skin target in it.
        assert!(
            DEFAULT_TOML.contains("skin loci"),
            "the header should explain the rule"
        );
        assert!(Policy::load_str(DEFAULT_TOML).is_ok());
    }

    #[test]
    fn a_degenerate_weight_set_is_refused() {
        let zeroed = DEFAULT_TOML
            .replace("technical   = 0.12", "technical   = 0.0")
            .replace("emotion     = 0.34", "emotion     = 0.0")
            .replace("composition = 0.24", "composition = 0.0")
            .replace("uniqueness  = 0.18", "uniqueness  = 0.0")
            .replace("story       = 0.12", "story       = 0.0");
        assert!(Policy::load_str(&zeroed).is_err());

        let negative = DEFAULT_TOML.replace("emotion     = 0.34", "emotion     = -0.10");
        assert!(Policy::load_str(&negative).is_err());
    }

    #[test]
    fn a_rhythm_pattern_naming_something_that_is_not_a_scale_is_refused() {
        let bad = DEFAULT_TOML.replace(
            r#"ceremony      = ["wide", "medium", "tight", "medium"]"#,
            r#"ceremony      = ["wide", "cinematic", "tight"]"#,
        );
        let err = Policy::load_str(&bad).unwrap_err();
        assert!(err.detail.contains("cinematic"), "{}", err.detail);
    }

    #[test]
    fn an_unknown_rhythm_token_is_not_silently_read_as_unmeasurable() {
        // `ShotScale::parse` reads anything unknown as `Unknown`, which is the right default for a
        // stored value and the wrong one for a configured pattern: a chapter whose whole pattern
        // read as `Unknown` would score a rhythm of zero on every album, for ever, silently.
        assert_eq!(ShotScale::parse("cinematic"), ShotScale::Unknown);
        let bad = DEFAULT_TOML.replace(r#"exit          = ["wide", "medium"]"#, r#"exit = [""]"#);
        assert!(Policy::load_str(&bad).is_err());
    }

    #[test]
    fn there_is_no_strength_anywhere_in_the_shipped_table() {
        // Section 6.1: a tailored mix per frame rather than a preset, and a preset scaled by a
        // number is a preset.
        let code = strip_comments(DEFAULT_TOML);
        assert!(!code.contains("strength"), "{code}");
    }
}
