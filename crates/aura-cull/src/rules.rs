//! What the gallery must contain, whatever the scores say.
//!
//! PHASE-12 section 8's first step names two files. [`weights`](crate::weights) loads the
//! one that decides what a photograph is *worth*; this loads the one that decides what the
//! gallery must *look like*. The split is not arbitrary and it is the reason the modes
//! cannot weaken a guarantee: [`crate::modes`] reads the first file and has no way to
//! reach this one.
//!
//! Five things live here.
//!
//! * **The twelve must-have rules.** Section 6.3's declarative form, exactly:
//!   `{ id: 'kiss', match: scene in [kiss] or interaction=kiss, min: 2, prefer: peak }`.
//! * **The identity minimums.** Section 6.3's "every close-family identity gets >= 3
//!   frames, every recurring guest >= 1" - the rule that exists so nobody's aunt is
//!   missing from the gallery.
//! * **The chapter bands.** Section 6.2's "clamped by min/max bands from
//!   `coverage_rules.toml`", plus the importance term in the quota formula.
//! * **The diversity caps.** Section 2.1's "avoid 40 near-identical dance frames".
//! * **The veto policy.** Section 6.1's three hard vetoes and, more importantly, the list
//!   of scenes in which the closed-eye veto may fire at all.
//!
//! ## The rule that surprised everybody who reviewed it
//!
//! **A veto excludes a frame from candidacy. It does not exclude it from a guarantee.**
//!
//! If the only photograph of the ring exchange is out of focus, the guard force-adds it,
//! marks the rule [`Coverage::CoveredWeak`](aura_core::Coverage::CoveredWeak) and writes a
//! warning that names the veto. A blurred photograph of the rings beats no photograph of
//! the rings, and a product that dropped it silently because a threshold said so would be
//! the exact failure section 12 opens with.
//!
//! The one veto that must never fire in the wrong place is the closed-eye one, which is
//! why [`VetoPolicy::posed_scenes`] is a list in this file rather than a `matches!` in the
//! code. A kiss, a prayer and a moment of tears are all photographs of closed eyes that
//! somebody meant, and phase 09 already exonerates most of them through phase 10's tears
//! path. This list is the second line of that defence.
//!
//! ## Refusal is whole-file, and stricter than anywhere else
//!
//! `AURA-ML-5052`, and an **unknown must-have slug is a refusal rather than a default**.
//! `MustHave` is the one vocabulary in this workspace with no `from_str_or_*`, because a
//! typo that silently disabled the vows guarantee would be invisible until a customer
//! found it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use aura_core::contract::emotion::Interaction;
use aura_core::contract::error::AuraError;
use aura_core::{ChapterId, MustHave, SceneId};
use serde::Deserialize;

use crate::errors;

/// The shipped table, embedded so a binary can never disagree with its own guarantees.
const EMBEDDED: &str = include_str!("../config/coverage_rules.toml");

/// The file name an installation override must have.
pub const OVERRIDE_FILE: &str = "coverage_rules.toml";

/// The fewest characters a rationale may have. Nine, as everywhere else.
pub const MIN_RATIONALE: usize = 9;

/// Which frame a rule prefers when it has to choose one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prefer {
    /// The moment's peak frame. Section 6.3's `prefer: peak`.
    Peak,
    /// The highest fused score. The default.
    #[default]
    Score,
    /// The earliest frame, for a rule about an arrival.
    Earliest,
}

impl Prefer {
    /// Parse the stored slug.
    fn parse(text: &str) -> Option<Self> {
        match text {
            "peak" => Some(Self::Peak),
            "score" => Some(Self::Score),
            "earliest" => Some(Self::Earliest),
            _ => None,
        }
    }
}

/// One declarative must-have rule.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageRule {
    /// Which guarantee this is.
    pub id: MustHave,
    /// Scenes that satisfy it.
    pub scenes: BTreeSet<SceneId>,
    /// Interactions that satisfy it, whatever the scene says.
    ///
    /// The second half of section 6.3's `match` clause and the half that does the work. A
    /// ring exchange inside a forty-minute ceremony segment is labelled `ceremony` at most
    /// weddings, so a rule that matched only the scene would report the rings missing
    /// almost everywhere.
    pub interactions: BTreeSet<Interaction>,
    /// The fewest frames the gallery may contain.
    pub min: u32,
    /// Which frame to reach for first when force-adding.
    pub prefer: Prefer,
}

impl CoverageRule {
    /// True when one frame's scene or interactions satisfy this rule.
    #[must_use]
    pub fn matches(&self, scene: SceneId, interactions: &[Interaction]) -> bool {
        self.scenes.contains(&scene)
            || interactions
                .iter()
                .any(|interaction| self.interactions.contains(interaction))
    }
}

/// How many frames each kind of person must appear in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IdentityRule {
    /// Frames every close-family identity must appear in. Section 6.3's three.
    pub close_family_min: u32,
    /// Frames every *recurring* guest must appear in. Section 6.3's one.
    pub guest_min: u32,
    /// How many photographs a guest must appear in before they count as recurring.
    ///
    /// The `M` of section 2.1's "every `guest` identity appearing more than M times". A
    /// guest in four frames was at the wedding; a guest in one frame walked past the
    /// camera, and guaranteeing them a place would fill the gallery with strangers.
    pub guest_recurrence: u32,
}

/// One chapter's share of the gallery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChapterBand {
    /// How much of the gallery this chapter deserves per frame shot, `0..2`.
    ///
    /// The `importance_c` of section 6.2's quota formula. Above one means the chapter
    /// earns more than its share of the shoot volume, below one means less.
    pub importance: f32,
    /// The fewest frames it may be allocated, when it has that many candidates.
    ///
    /// Section 6.2's "details/venue chapters get small fixed allocations", generalised: a
    /// minimum is what stops a chapter that was shot lightly from vanishing under a
    /// square-root volume term.
    pub min: u32,
    /// The most it may be allocated.
    pub max: u32,
}

/// How much repetition the gallery tolerates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiversityPolicy {
    /// The width of the sliding window the per-window cap applies over, in milliseconds.
    pub window_ms: i64,
    /// The most frames one chapter may deliver from one window.
    ///
    /// Section 2.1's "avoid 40 near-identical dance frames", as a number. It is a *time*
    /// cap rather than a similarity cap because forty frames of one song are forty frames
    /// of one song whether or not the embedding says they are similar - and the
    /// similarity cap already exists in phase 08's duplicate sets.
    pub per_window: u32,
    /// The most frames in one window that may share the same framing bucket.
    ///
    /// Section 2.1's "spread across framing (wide/medium/tight)".
    pub per_window_framing: u32,
    /// The most frames in one window that may be dominated by the same identity.
    ///
    /// Section 2.1's "identity mix". Deliberately generous: the couple are supposed to be
    /// in most of the photographs, and this exists to stop a *guest* being in forty of
    /// them, not to ration the bride.
    pub per_window_identity: u32,
}

/// When a frame never reaches the arithmetic.
#[derive(Debug, Clone, PartialEq)]
pub struct VetoPolicy {
    /// Below this subject sharpness the frame is vetoed as out of focus.
    ///
    /// Deliberately low. Section 6.1's veto is "subject **completely** out of focus", not
    /// "softer than average"; softness is what the score is for. A veto that fired at 0.35
    /// would be a threshold pretending to be a fact.
    pub focus_floor: f32,
    /// Scenes in which a closed-eye veto may fire at all.
    ///
    /// Section 6.1: "primary identity's eyes closed without intent **in a posed scene**".
    /// The list is here rather than in code because getting it wrong means vetoing the
    /// kiss, and a list in a file with a rationale beside it is a list somebody reviews.
    pub posed_scenes: BTreeSet<SceneId>,
}

/// Everything the gallery must contain, and the digest of the file it came from.
#[derive(Debug, Clone)]
pub struct RuleTable {
    version: u16,
    rules: Vec<CoverageRule>,
    identity: IdentityRule,
    chapters: BTreeMap<ChapterId, ChapterBand>,
    diversity: DiversityPolicy,
    veto: VetoPolicy,
    digest: String,
}

impl RuleTable {
    /// The table this build ships.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5052` when the embedded table is malformed, which is a build failure.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse(EMBEDDED, "embedded coverage_rules.toml")
    }

    /// An installation override.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5052` when the file cannot be read or is malformed. **Nothing is loaded.**
    pub fn load(path: &Path) -> Result<Self, AuraError> {
        let text = std::fs::read_to_string(path).map_err(|error| {
            errors::coverage_rules_refused(
                &path.display().to_string(),
                "file",
                &format!("could not be read: {error}"),
            )
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    /// Parse and validate a table. Every rule is checked here and nowhere else.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5052`, naming the file, the key and the rule.
    #[allow(clippy::too_many_lines)]
    pub fn parse(text: &str, origin: &str) -> Result<Self, AuraError> {
        let file: FileShape = toml::from_str(text)
            .map_err(|error| errors::coverage_rules_refused(origin, "file", &format!("{error}")))?;
        if file.version == 0 {
            return Err(errors::coverage_rules_refused(
                origin,
                "version",
                "must be at least 1",
            ));
        }

        let mut seen = BTreeSet::new();
        let mut rules = Vec::new();
        for row in &file.rule {
            // Refused rather than defaulted. See this module's header: a typo that
            // silently disabled the vows guarantee is invisible until a customer finds it.
            let Some(id) = MustHave::parse(&row.id) else {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "is not one of the twelve frozen must-have rules",
                ));
            };
            if !seen.insert(id) {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "appears twice",
                ));
            }
            if row.rationale.trim().len() < MIN_RATIONALE {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "needs a rationale of at least nine characters",
                ));
            }
            if row.min == 0 {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "min must be at least 1; a guarantee of zero frames is not a guarantee",
                ));
            }
            let Some(prefer) = Prefer::parse(&row.prefer) else {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "prefer must be peak, score or earliest",
                ));
            };
            let mut scenes = BTreeSet::new();
            for slug in &row.scenes {
                let scene = SceneId::from_str_or_unknown(slug);
                if scene == SceneId::Unknown {
                    return Err(errors::coverage_rules_refused(
                        origin,
                        &row.id,
                        &format!("names `{slug}`, which is not a scene in the frozen taxonomy"),
                    ));
                }
                scenes.insert(scene);
            }
            let mut interactions = BTreeSet::new();
            for slug in &row.interactions {
                let Some(interaction) = Interaction::ALL
                    .into_iter()
                    .find(|candidate| candidate.as_str() == slug)
                else {
                    return Err(errors::coverage_rules_refused(
                        origin,
                        &row.id,
                        &format!("names `{slug}`, which is not a frozen interaction"),
                    ));
                };
                interactions.insert(interaction);
            }
            if scenes.is_empty() && interactions.is_empty() && !id.is_identity_rule() {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "matches nothing; a rule with no scenes and no interactions can never be \
                     satisfied",
                ));
            }
            rules.push(CoverageRule {
                id,
                scenes,
                interactions,
                min: row.min,
                prefer,
            });
        }
        for rule in MustHave::ALL {
            if !seen.contains(&rule) {
                return Err(errors::coverage_rules_refused(
                    origin,
                    rule.as_str(),
                    "has no row, and all twelve guarantees must be defined",
                ));
            }
        }
        rules.sort_by_key(|rule| {
            MustHave::ALL
                .iter()
                .position(|candidate| *candidate == rule.id)
                .unwrap_or(MustHave::COUNT)
        });

        if file.identity.rationale.trim().len() < MIN_RATIONALE {
            return Err(errors::coverage_rules_refused(
                origin,
                "identity",
                "needs a rationale of at least nine characters",
            ));
        }
        if file.identity.close_family_min == 0 || file.identity.guest_min == 0 {
            return Err(errors::coverage_rules_refused(
                origin,
                "identity",
                "minimums must be at least 1",
            ));
        }

        let mut chapters = BTreeMap::new();
        for row in &file.chapter {
            let Some(chapter) = ChapterId::ALL
                .into_iter()
                .find(|candidate| candidate.as_str() == row.id)
            else {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "is not a chapter in the frozen taxonomy",
                ));
            };
            if row.rationale.trim().len() < MIN_RATIONALE {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "needs a rationale of at least nine characters",
                ));
            }
            if !(0.0..=2.0).contains(&row.importance) {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "importance must be within 0.0..2.0",
                ));
            }
            if row.min > row.max {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "min is above max",
                ));
            }
            if chapters
                .insert(
                    chapter,
                    ChapterBand {
                        importance: row.importance,
                        min: row.min,
                        max: row.max,
                    },
                )
                .is_some()
            {
                return Err(errors::coverage_rules_refused(
                    origin,
                    &row.id,
                    "appears twice",
                ));
            }
        }
        for chapter in ChapterId::ALL {
            if !chapters.contains_key(&chapter) {
                return Err(errors::coverage_rules_refused(
                    origin,
                    chapter.as_str(),
                    "has no band, and all nine chapters must be defined",
                ));
            }
        }

        if file.diversity.window_ms <= 0 {
            return Err(errors::coverage_rules_refused(
                origin,
                "diversity",
                "window_ms must be positive",
            ));
        }
        for (name, value) in [
            ("per_window", file.diversity.per_window),
            ("per_window_framing", file.diversity.per_window_framing),
            ("per_window_identity", file.diversity.per_window_identity),
        ] {
            if value == 0 {
                return Err(errors::coverage_rules_refused(
                    origin,
                    "diversity",
                    &format!("{name} must be at least 1"),
                ));
            }
        }

        if !(0.0..=0.5).contains(&file.veto.focus_floor) {
            return Err(errors::coverage_rules_refused(
                origin,
                "veto",
                "focus_floor must be within 0.0..0.5; a veto above that is a threshold \
                 pretending to be a fact",
            ));
        }
        let mut posed = BTreeSet::new();
        for slug in &file.veto.posed_scenes {
            let scene = SceneId::from_str_or_unknown(slug);
            if scene == SceneId::Unknown {
                return Err(errors::coverage_rules_refused(
                    origin,
                    "veto",
                    &format!("names `{slug}`, which is not a scene in the frozen taxonomy"),
                ));
            }
            posed.insert(scene);
        }
        if posed.contains(&SceneId::Kiss) {
            return Err(errors::coverage_rules_refused(
                origin,
                "veto",
                "lists `kiss` as a posed scene, which would let AURA veto the kiss for closed \
                 eyes",
            ));
        }

        Ok(Self {
            version: file.version,
            rules,
            identity: IdentityRule {
                close_family_min: file.identity.close_family_min,
                guest_min: file.identity.guest_min,
                guest_recurrence: file.identity.guest_recurrence,
            },
            chapters,
            diversity: DiversityPolicy {
                window_ms: file.diversity.window_ms,
                per_window: file.diversity.per_window,
                per_window_framing: file.diversity.per_window_framing,
                per_window_identity: file.diversity.per_window_identity,
            },
            veto: VetoPolicy {
                focus_floor: file.veto.focus_floor,
                posed_scenes: posed,
            },
            digest: blake3::hash(text.as_bytes()).to_hex().to_string(),
        })
    }

    /// Which table this is.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// The digest of the exact text this table was parsed from.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The twelve rules, in [`MustHave::ALL`] order.
    #[must_use]
    pub fn rules(&self) -> &[CoverageRule] {
        &self.rules
    }

    /// One rule, by id.
    #[must_use]
    pub fn rule(&self, id: MustHave) -> Option<&CoverageRule> {
        self.rules.iter().find(|rule| rule.id == id)
    }

    /// How many frames each kind of person must appear in.
    #[must_use]
    pub const fn identity(&self) -> IdentityRule {
        self.identity
    }

    /// One chapter's band. Infallible: the loader refuses a table missing any of the nine.
    #[must_use]
    pub fn chapter(&self, chapter: ChapterId) -> ChapterBand {
        self.chapters.get(&chapter).copied().unwrap_or(ChapterBand {
            importance: 1.0,
            min: 0,
            max: u32::MAX,
        })
    }

    /// How much repetition the gallery tolerates.
    #[must_use]
    pub const fn diversity(&self) -> DiversityPolicy {
        self.diversity
    }

    /// When a frame never reaches the arithmetic.
    #[must_use]
    pub const fn veto(&self) -> &VetoPolicy {
        &self.veto
    }
}

// -- the file's shape -------------------------------------------------------

#[derive(Debug, Deserialize)]
struct FileShape {
    version: u16,
    identity: IdentityShape,
    diversity: DiversityShape,
    veto: VetoShape,
    #[serde(default)]
    rule: Vec<RuleShape>,
    #[serde(default)]
    chapter: Vec<ChapterShape>,
}

#[derive(Debug, Deserialize)]
struct RuleShape {
    id: String,
    #[serde(default)]
    scenes: Vec<String>,
    #[serde(default)]
    interactions: Vec<String>,
    min: u32,
    prefer: String,
    rationale: String,
}

#[derive(Debug, Deserialize)]
struct IdentityShape {
    close_family_min: u32,
    guest_min: u32,
    guest_recurrence: u32,
    rationale: String,
}

#[derive(Debug, Deserialize)]
struct ChapterShape {
    id: String,
    importance: f32,
    min: u32,
    max: u32,
    rationale: String,
}

#[derive(Debug, Deserialize)]
struct DiversityShape {
    window_ms: i64,
    per_window: u32,
    per_window_framing: u32,
    per_window_identity: u32,
    #[allow(dead_code)]
    rationale: String,
}

#[derive(Debug, Deserialize)]
struct VetoShape {
    focus_floor: f32,
    posed_scenes: Vec<String>,
    #[allow(dead_code)]
    rationale: String,
}
