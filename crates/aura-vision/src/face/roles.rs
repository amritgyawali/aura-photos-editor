//! Working out who the wedding is about, from evidence about photographs.
//!
//! This is the module most likely to be got wrong by guessing, so it is worth being
//! precise about what it does and does not do.
//!
//! **It never infers anything about a person.** Not gender, not ethnicity, not
//! religion, not relationship beyond the three categories a photographic record can
//! actually support: the pair this event is about, the people photographed *with*
//! that pair, and everybody else. Section 6.3 and the cloud prompt in section 7 both
//! say this, and it is enforced here by the shape of the input: the only things this
//! module can see are counts, areas, centralities and timestamps.
//!
//! **`bride` and `groom` are never assigned automatically.** The evidence identifies
//! a *pair*; which of two people is the bride is not a photographic fact, the couple
//! may be same-sex, and there is no version of guessing that is acceptable. So
//! [`infer_roles`] assigns [`Role::Couple`] to both, [`RoleOutcome::couple`] tells
//! the People panel which two to offer a one-click confirmation for, and
//! [`Role::Bride`] only ever arrives through `PeopleService::set_role`.
//!
//! **A locked identity is not an input to be weighed, it is an answer.** When a
//! photographer has said who somebody is, [`infer_roles`] copies that decision
//! through untouched at confidence 1.0 and excludes it from every contest. Section
//! 12's first failure mode - "wrong couple identification poisons every later
//! decision" - is mitigated by making the human's answer structurally unbeatable
//! rather than merely higher-weighted.
//!
//! ## Scenes are missing, and that is reported rather than hidden
//!
//! Section 6.3 wants the couple chosen from co-occurrence in `couple_portrait`,
//! centrality during `ceremony`, and frequency across `getting_ready`. Phase 07
//! assigns scenes; it has not run. So every scene count that arrives here is zero,
//! the scene terms contribute nothing, and the decision falls back to co-occurrence
//! and frequency.
//!
//! That fallback is *not* silently equivalent. It is weaker, and a couple chosen
//! without scene evidence is more likely to be a bride and her mother than a bride
//! and a groom - because mothers appear in getting-ready frames with the bride
//! constantly. So [`SCENELESS_CONFIDENCE_CEILING`] caps the confidence, the reason
//! string says why, and `SubjectHierarchy::couple_unconfirmed` stays true. When
//! phase 07 lands, the same code path gets real scene counts and the ceiling stops
//! applying. This is condition C4 in `docs/progress/PHASE-06-EXIT.md`.

use std::collections::BTreeMap;

use aura_core::contract::people::Role;

/// The role-inference generation, recorded with every verdict.
pub const ROLE_VER: u16 = 1;

/// Scene labels this module reads. They are the phase 04 controlled vocabulary, so
/// phase 07 will produce exactly these strings and nothing here has to change.
pub const SCENE_GETTING_READY: &str = "getting_ready";
/// The ceremony itself.
pub const SCENE_CEREMONY: &str = "ceremony";
/// Portraits of the couple alone.
pub const SCENE_COUPLE_PORTRAIT: &str = "portraits";
/// Posed family groups.
pub const SCENE_FAMILY: &str = "family_formals";

/// How close the second-best couple pair may be before the answer is ambiguous.
///
/// Section 7: the cloud hint fires "only when top-2 couple candidates are within
/// 0.05 score". The same number decides whether the People panel asks the
/// photographer, because the two questions are the same question.
pub const COUPLE_AMBIGUOUS_MARGIN: f32 = 0.05;

/// The most confidence a couple decision may carry with no scene labels.
///
/// See this module's header. Deliberately below the 0.90 line that phase 04's rule
/// Deliberately below the 0.90 line that phase 04's rule draws - "a cloud answer may not
/// overrule a local decision at confidence 0.90 or above" - so that the optional couple
/// hint in section 7 is *allowed* to help while scenes are missing, and is locked out once
/// they exist and the local confidence rises above it.
pub const SCENELESS_CONFIDENCE_CEILING: f32 = 0.62;

/// Appearances below which an identity is not given a role at all.
///
/// Two. One appearance is a stranger who walked through the frame.
pub const MIN_FRAMES_FOR_ROLE: u32 = 2;

/// Fraction of the frame's half-diagonal within which a face counts as central.
const EDGE_CENTRALITY: f32 = 0.35;

/// What is known about one identity, in terms a role decision can use.
///
/// Everything here is a count, a mean or a timestamp. There is deliberately no
/// pixel, no template and no colour: see this module's header.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityEvidence {
    /// Dense handle. Usually the identity's position in a sorted list, which is
    /// what makes the outcome reproducible.
    pub index: usize,
    /// Photographs this identity appears in.
    pub frames: u32,
    /// Frames per scene label. Empty until phase 07 runs.
    pub scene_frames: BTreeMap<String, u32>,
    /// Mean fraction of the frame this identity's face covers.
    pub mean_area: f32,
    /// Mean centrality of this identity's faces, `0..1`.
    pub mean_centrality: f32,
    /// Fraction of appearances where the face sat near the frame edge, `0..1`.
    pub edge_fraction: f32,
    /// Evidence that this identity is a child, `0..1`.
    ///
    /// Supplied by the caller from face-to-body proportion, which is a geometric
    /// measurement and not a judgement about a person: a child's head is a larger
    /// fraction of their standing height than an adult's. It is used only to decide
    /// whether coverage rules and retouch policy treat this identity as a child, and
    /// it is overridden the moment a photographer says otherwise.
    pub child_score: f32,
    /// Milliseconds since the epoch of the first appearance.
    pub first_seen_ms: Option<i64>,
    /// Milliseconds since the epoch of the last appearance.
    pub last_seen_ms: Option<i64>,
    /// True when a photographer has set this identity's role.
    pub user_locked: bool,
    /// The role a photographer set, when they set one.
    pub locked_role: Role,
}

impl IdentityEvidence {
    /// Frames in the three scenes that identify a couple.
    #[must_use]
    pub fn key_scene_frames(&self) -> u32 {
        let count = |label: &str| self.scene_frames.get(label).copied().unwrap_or(0);
        count(SCENE_GETTING_READY)
            .saturating_add(count(SCENE_CEREMONY))
            .saturating_add(count(SCENE_COUPLE_PORTRAIT))
    }

    /// True when any scene label at all is known for this identity.
    #[must_use]
    pub fn has_scenes(&self) -> bool {
        self.scene_frames.values().any(|count| *count > 0)
    }
}

/// What is known about one pair of identities.
#[derive(Debug, Clone, PartialEq)]
pub struct PairEvidence {
    /// One identity's handle.
    pub a: usize,
    /// The other's.
    pub b: usize,
    /// Photographs both appear in.
    pub frames: u32,
    /// Shared frames per scene label. Empty until phase 07 runs.
    pub scene_frames: BTreeMap<String, u32>,
}

impl PairEvidence {
    /// Shared frames in couple portraits.
    #[must_use]
    pub fn portrait_frames(&self) -> u32 {
        self.scene_frames
            .get(SCENE_COUPLE_PORTRAIT)
            .copied()
            .unwrap_or(0)
    }

    /// Shared frames in posed family groups.
    #[must_use]
    pub fn family_frames(&self) -> u32 {
        self.scene_frames.get(SCENE_FAMILY).copied().unwrap_or(0)
    }

    /// Canonical ordering, so one pair is one entry.
    #[must_use]
    pub fn key(&self) -> (usize, usize) {
        if self.a <= self.b {
            (self.a, self.b)
        } else {
            (self.b, self.a)
        }
    }
}

/// One identity's role, with the evidence for it.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleVerdict {
    /// Which identity.
    pub index: usize,
    /// What it is.
    pub role: Role,
    /// How sure, `0..1`.
    pub confidence: f32,
    /// Why. Invariant 2: never empty above zero confidence.
    pub reasons: Vec<String>,
    /// True when this verdict is a photographer's decision copied through.
    pub locked: bool,
}

/// Every role in a project, plus the couple contest's own result.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleOutcome {
    /// One verdict per identity, in input order.
    pub verdicts: Vec<RoleVerdict>,
    /// The pair the evidence chose, when it chose one.
    pub couple: Option<(usize, usize)>,
    /// The winning pair's score.
    pub couple_score: f32,
    /// The runner-up's score. The gap is what decides ambiguity.
    pub runner_up_score: f32,
    /// True when the top two pairs are within [`COUPLE_AMBIGUOUS_MARGIN`].
    ///
    /// The trigger for the section 7 cloud hint *and* for the People panel's
    /// "which of these two pairs is the couple" prompt. Both, because the cloud is
    /// an accelerator and the photographer is the authority.
    pub ambiguous: bool,
    /// True when no scene labels were available and the decision was made without
    /// them.
    pub scene_starved: bool,
    /// Which inference generation produced this.
    pub role_ver: u16,
}

/// The weights of the couple contest, summing to one.
///
/// Section 6.3 names three terms - co-occurrence in couple portraits, ceremony
/// centrality, total frequency - and the bride/groom bullet adds mean face area and
/// mutual co-occurrence. All five are here. The split puts most of the weight on
/// portrait co-occurrence because it is the single most discriminating signal at a
/// wedding: the couple are the only two people who are repeatedly photographed
/// alone together.
const COUPLE_PORTRAIT_WEIGHT: f32 = 0.34;
/// Weight of centrality during the ceremony.
const COUPLE_CEREMONY_WEIGHT: f32 = 0.22;
/// Weight of raw mutual co-occurrence, which is what carries the decision when no
/// scenes are known.
const COUPLE_COOCCURRENCE_WEIGHT: f32 = 0.22;
/// Weight of how much of a subject each of the two is, on average.
const COUPLE_SUBJECT_WEIGHT: f32 = 0.22;

/// Infer every identity's role from evidence.
///
/// Deterministic: every contest is decided by a score with an index tie-break, and
/// every normalisation is against a maximum taken over the same slice in the same
/// order.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn infer_roles(identities: &[IdentityEvidence], pairs: &[PairEvidence]) -> RoleOutcome {
    let mut outcome = RoleOutcome {
        verdicts: Vec::with_capacity(identities.len()),
        couple: None,
        couple_score: 0.0,
        runner_up_score: 0.0,
        ambiguous: false,
        scene_starved: !identities.iter().any(IdentityEvidence::has_scenes),
        role_ver: ROLE_VER,
    };
    if identities.is_empty() {
        return outcome;
    }

    let max_frames = identities
        .iter()
        .map(|identity| identity.frames)
        .max()
        .unwrap_or(1)
        .max(1);
    let max_area = identities
        .iter()
        .map(|identity| identity.mean_area)
        .fold(0.0f32, f32::max)
        .max(f32::EPSILON);
    let max_key_scene = identities
        .iter()
        .map(IdentityEvidence::key_scene_frames)
        .max()
        .unwrap_or(0)
        .max(1);

    // How much of a photographic subject each identity is, on its own.
    let subject_scores: Vec<f32> = identities
        .iter()
        .map(|identity| {
            let frequency = f64::from(identity.frames) as f32 / f64::from(max_frames) as f32;
            let area = identity.mean_area / max_area;
            let scenes =
                f64::from(identity.key_scene_frames()) as f32 / f64::from(max_key_scene) as f32;
            (0.35 * frequency + 0.30 * area + 0.35 * scenes).clamp(0.0, 1.0)
        })
        .collect();

    // --- The couple contest --------------------------------------------------
    let max_pair_frames = pairs
        .iter()
        .map(|pair| pair.frames)
        .max()
        .unwrap_or(0)
        .max(1);
    let max_portrait = pairs
        .iter()
        .map(PairEvidence::portrait_frames)
        .max()
        .unwrap_or(0)
        .max(1);

    let locked_couple: Vec<usize> = identities
        .iter()
        .filter(|identity| identity.user_locked && identity.locked_role.is_couple())
        .map(|identity| identity.index)
        .collect();

    let mut scored: Vec<((usize, usize), f32)> = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let (a, b) = pair.key();
        let (Some(left), Some(right)) = (find(identities, a), find(identities, b)) else {
            continue;
        };
        // A photographer's decision is not a candidate in a contest it has already
        // won or lost. An identity locked to something that is not the couple is
        // excluded outright.
        if (left.user_locked && !left.locked_role.is_couple())
            || (right.user_locked && !right.locked_role.is_couple())
        {
            continue;
        }

        let portrait = f64::from(pair.portrait_frames()) as f32 / f64::from(max_portrait) as f32;
        let ceremony = ceremony_centrality(left, right);
        let cooccurrence = f64::from(pair.frames) as f32 / f64::from(max_pair_frames) as f32;
        let subject = (subject_at(&subject_scores, identities, a)
            + subject_at(&subject_scores, identities, b))
            * 0.5;

        let score = COUPLE_PORTRAIT_WEIGHT * portrait
            + COUPLE_CEREMONY_WEIGHT * ceremony
            + COUPLE_COOCCURRENCE_WEIGHT * cooccurrence
            + COUPLE_SUBJECT_WEIGHT * subject;
        scored.push(((a, b), score.clamp(0.0, 1.0)));
    }

    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });

    if let Some((pair, score)) = scored.first().copied() {
        outcome.couple = Some(pair);
        outcome.couple_score = score;
        outcome.runner_up_score = scored.get(1).map_or(0.0, |entry| entry.1);
        outcome.ambiguous = scored.len() > 1
            && (outcome.couple_score - outcome.runner_up_score) < COUPLE_AMBIGUOUS_MARGIN;
    }

    // A locked couple overrides the contest entirely.
    if locked_couple.len() == 2 {
        if let (Some(first), Some(second)) = (locked_couple.first(), locked_couple.last()) {
            outcome.couple = Some((*first.min(second), *first.max(second)));
            outcome.couple_score = 1.0;
            outcome.runner_up_score = 0.0;
            outcome.ambiguous = false;
        }
    }

    // --- Per-identity verdicts ----------------------------------------------
    let couple_members: Vec<usize> = outcome.couple.map(|(a, b)| vec![a, b]).unwrap_or_default();

    let couple_confidence = if outcome.scene_starved {
        outcome.couple_score.min(SCENELESS_CONFIDENCE_CEILING)
    } else {
        outcome.couple_score
    };

    let vendor_frame_floor = percentile_frames(identities, 0.75);

    for (position, identity) in identities.iter().enumerate() {
        if identity.user_locked {
            outcome.verdicts.push(RoleVerdict {
                index: identity.index,
                role: identity.locked_role,
                confidence: 1.0,
                reasons: vec![format!(
                    "set to {} by the photographer; automation may not change it",
                    identity.locked_role
                )],
                locked: true,
            });
            continue;
        }

        if identity.frames < MIN_FRAMES_FOR_ROLE {
            outcome.verdicts.push(RoleVerdict {
                index: identity.index,
                role: Role::Unknown,
                confidence: 0.0,
                reasons: vec![format!(
                    "appears in {} photograph(s), below the {MIN_FRAMES_FOR_ROLE} needed to say \
                     anything",
                    identity.frames
                )],
                locked: false,
            });
            continue;
        }

        if couple_members.contains(&identity.index) {
            let mut reasons = vec![format!(
                "one of the two identities most photographed together at this wedding (pair score \
                 {:.2}, runner-up {:.2})",
                outcome.couple_score, outcome.runner_up_score
            )];
            if outcome.scene_starved {
                reasons.push(
                    "no scene labels are available yet, so this was decided from co-occurrence and \
                     frequency alone and needs confirming in the People panel"
                        .to_string(),
                );
            }
            if outcome.ambiguous {
                reasons.push(format!(
                    "another pair scored within {COUPLE_AMBIGUOUS_MARGIN:.2}; the photographer \
                     should confirm"
                ));
            }
            outcome.verdicts.push(RoleVerdict {
                index: identity.index,
                role: Role::Couple,
                confidence: couple_confidence,
                reasons,
                locked: false,
            });
            continue;
        }

        // Vendor: present all day, almost never the subject, usually at the edge.
        let subject = subject_scores.get(position).copied().unwrap_or(0.0);
        if identity.frames >= vendor_frame_floor
            && identity.mean_area < max_area * 0.35
            && identity.mean_centrality < EDGE_CENTRALITY
        {
            outcome.verdicts.push(RoleVerdict {
                index: identity.index,
                role: Role::Vendor,
                confidence: (0.45 + 0.35 * (1.0 - identity.mean_centrality)).clamp(0.0, 0.85),
                reasons: vec![
                    format!(
                        "appears in {} photographs but averages {:.1}% of the frame and sits away \
                         from the centre",
                        identity.frames,
                        identity.mean_area * 100.0
                    ),
                    format!(
                        "{:.0}% of appearances are near the frame edge",
                        identity.edge_fraction * 100.0
                    ),
                ],
                locked: false,
            });
            continue;
        }

        // Family, by co-occurrence with the couple.
        let with_couple = couple_cooccurrence(pairs, &couple_members, identity.index);
        let couple_frames = couple_members
            .iter()
            .filter_map(|member| find(identities, *member))
            .map(|member| member.frames)
            .max()
            .unwrap_or(1)
            .max(1);
        let closeness = f64::from(with_couple) as f32 / f64::from(couple_frames) as f32;
        let family_scene_frames: u32 = pairs
            .iter()
            .filter(|pair| {
                let (a, b) = pair.key();
                (a == identity.index && couple_members.contains(&b))
                    || (b == identity.index && couple_members.contains(&a))
            })
            .map(PairEvidence::family_frames)
            .sum();
        let getting_ready = identity
            .scene_frames
            .get(SCENE_GETTING_READY)
            .copied()
            .unwrap_or(0);

        if identity.child_score >= 0.6 {
            outcome.verdicts.push(RoleVerdict {
                index: identity.index,
                role: Role::Child,
                confidence: (identity.child_score * 0.8).clamp(0.0, 0.8),
                reasons: vec![format!(
                    "face-to-body proportion is consistent with a child ({:.2}); coverage and \
                     retouch rules treat this identity accordingly",
                    identity.child_score
                )],
                locked: false,
            });
            continue;
        }

        if closeness >= 0.35 || family_scene_frames >= 3 || getting_ready >= 3 {
            let mut reasons = vec![format!(
                "appears with the couple in {with_couple} photographs, {:.0}% of their coverage",
                closeness * 100.0
            )];
            if family_scene_frames > 0 {
                reasons.push(format!(
                    "{family_scene_frames} of those are posed family groups"
                ));
            }
            if getting_ready > 0 {
                reasons.push(format!("present in {getting_ready} getting-ready frames"));
            }
            if outcome.scene_starved {
                reasons.push(
                    "with no scene labels, close and extended family are separated by \
                     co-occurrence alone"
                        .to_string(),
                );
            }
            let confidence = (0.35 + 0.4 * closeness).clamp(0.0, 0.8);
            outcome.verdicts.push(RoleVerdict {
                index: identity.index,
                role: Role::FamilyClose,
                confidence: if outcome.scene_starved {
                    confidence.min(SCENELESS_CONFIDENCE_CEILING)
                } else {
                    confidence
                },
                reasons,
                locked: false,
            });
            continue;
        }

        if closeness >= 0.12 {
            outcome.verdicts.push(RoleVerdict {
                index: identity.index,
                role: Role::FamilyExtended,
                confidence: (0.25 + 0.3 * closeness).clamp(0.0, 0.6),
                reasons: vec![format!(
                    "appears with the couple in {with_couple} photographs, {:.0}% of their \
                     coverage - enough for extended family, not enough for close",
                    closeness * 100.0
                )],
                locked: false,
            });
            continue;
        }

        // A guest who is repeatedly photographed as a subject is a guest of honour.
        if subject >= 0.45 {
            outcome.verdicts.push(RoleVerdict {
                index: identity.index,
                role: Role::Vip,
                confidence: (0.30 + 0.35 * subject).clamp(0.0, 0.7),
                reasons: vec![format!(
                    "photographed as a subject unusually often for somebody with little couple \
                     co-occurrence (subject score {subject:.2})"
                )],
                locked: false,
            });
            continue;
        }

        outcome.verdicts.push(RoleVerdict {
            index: identity.index,
            role: Role::Guest,
            confidence: 0.45,
            reasons: vec![format!(
                "appears in {} photographs with little couple co-occurrence and no vendor pattern",
                identity.frames
            )],
            locked: false,
        });
    }

    outcome
}

/// Mean centrality of two identities during the ceremony.
///
/// Falls back to overall mean centrality when no ceremony scene is known, which is
/// the case until phase 07 runs. The fallback is reported by
/// [`RoleOutcome::scene_starved`] rather than hidden, and it is a weaker signal:
/// overall centrality includes portraits, where everybody is central.
fn ceremony_centrality(left: &IdentityEvidence, right: &IdentityEvidence) -> f32 {
    let ceremony_frames = |identity: &IdentityEvidence| {
        identity
            .scene_frames
            .get(SCENE_CEREMONY)
            .copied()
            .unwrap_or(0)
    };
    if ceremony_frames(left) == 0 && ceremony_frames(right) == 0 {
        return (left.mean_centrality + right.mean_centrality) * 0.5;
    }
    let weight = |identity: &IdentityEvidence| {
        let frames = f64::from(ceremony_frames(identity)) as f32;
        if frames <= 0.0 {
            0.0
        } else {
            identity.mean_centrality
        }
    };
    (weight(left) + weight(right)) * 0.5
}

/// Look up an identity by handle.
fn find(identities: &[IdentityEvidence], index: usize) -> Option<&IdentityEvidence> {
    identities.iter().find(|identity| identity.index == index)
}

/// A subject score by identity handle.
fn subject_at(scores: &[f32], identities: &[IdentityEvidence], index: usize) -> f32 {
    identities
        .iter()
        .position(|identity| identity.index == index)
        .and_then(|position| scores.get(position).copied())
        .unwrap_or(0.0)
}

/// Frames one identity shares with either member of the couple.
///
/// The maximum rather than the sum, because a frame containing the identity and both
/// members of the couple would otherwise be counted twice, and the frame containing
/// all three is the family portrait - the single most informative frame there is.
fn couple_cooccurrence(pairs: &[PairEvidence], couple: &[usize], identity: usize) -> u32 {
    pairs
        .iter()
        .filter_map(|pair| {
            let (a, b) = pair.key();
            let touches_couple =
                (a == identity && couple.contains(&b)) || (b == identity && couple.contains(&a));
            touches_couple.then_some(pair.frames)
        })
        .max()
        .unwrap_or(0)
}

/// The frame count at a given percentile of the project's identities.
///
/// Used for the vendor test, which is about being *unusually* present rather than
/// present a fixed number of times: a two-hour registry wedding and a three-day
/// Nepali one have different absolute numbers and the same shape.
fn percentile_frames(identities: &[IdentityEvidence], percentile: f32) -> u32 {
    if identities.is_empty() {
        return u32::MAX;
    }
    let mut counts: Vec<u32> = identities.iter().map(|identity| identity.frames).collect();
    counts.sort_unstable();
    let position = ((counts.len() as f32 - 1.0) * percentile.clamp(0.0, 1.0)).round() as usize;
    counts.get(position).copied().unwrap_or(u32::MAX)
}
