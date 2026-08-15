//! Reaction linking: the mother's tears while the couple kisses.
//!
//! Section 6.3 calls this "a feature no competitor ships", and the reason it is worth
//! shipping is not the link itself but what two later phases do with it. Phase 12 can
//! guarantee that a kiss keeper is delivered *with* the reaction to it rather than
//! separately; phase 29 can build a cause-and-effect album spread that a human editor
//! would have built by remembering.
//!
//! ## The three conditions, and why each one is needed
//!
//! For a candidate frame R to be a reaction to an action frame A:
//!
//! 1. **Time.** `|t(R) - t(A)| <= 4 s`, section 6.3's window, as a hard bound rather
//!    than a scored term. Signed, because a reaction *before* its action is real: a
//!    second shooter's clock is not the first shooter's, and phase 01's timeline
//!    alignment is good to about a second.
//! 2. **Not the same photograph of the same thing.** R must be from a different moment,
//!    or from a different camera. Without this, every frame of a burst is a reaction to
//!    every other frame of the same burst, which is 91 links for a fourteen-frame toss
//!    and none of them means anything.
//! 3. **Attention and expression.** Somebody in R is looking into the room rather than
//!    at the lens - `GazeTarget::is_engaged` - *and* is expressing something. A face
//!    turned away with a flat expression is somebody checking the time.
//!
//! A fourth condition is deliberately **absent**: the reactor is not required to be
//! looking geometrically towards where the action happened. Two cameras have two frames
//! and no shared coordinate system, so a claim about the direction between them would be
//! invented. Section 6.3 asks for "frames whose subjects gaze toward the action" and
//! this is as close as two independent 2D frames honestly get - which is condition C3 of
//! the phase 10 exit report, shared with the gaze module.
//!
//! ## Why the caps exist
//!
//! Section 10.1 wants under 10 % spurious links. Two bounds do most of that work:
//! [`MAX_PER_ACTION`] stops a busy reception turning one kiss into forty links, and the
//! one-action-per-reaction rule in the schema means a frame that could react to three
//! things reacts to the strongest.

use std::collections::BTreeMap;

use aura_core::contract::emotion::{
    EmotionCode, EmotionReason, FaceExpression, ImageId, Interaction, ReactionLink, Timestamp,
};
use aura_core::{IdentityId, MomentId, Role};

/// The strength an action frame must reach before reactions are looked for.
///
/// A frame with nothing happening in it has nothing to react to, and searching for
/// reactions to all four thousand frames of a wedding is four thousand window scans for
/// the handful that matter. 0.55 on the action's own interaction-or-expression strength.
pub const ACTION_FLOOR: f32 = 0.55;

/// The expression intensity a reactor must reach.
///
/// Lower than [`ACTION_FLOOR`] on purpose: a reaction is usually a smaller thing than
/// the action - a hand to the mouth, a wet eye - and requiring the same strength would
/// find only the reactions that are themselves photographs.
pub const REACTOR_FLOOR: f32 = 0.40;

/// The most reactions one action frame may have.
///
/// Four. A milestone in a full room genuinely has several - both mothers, a sibling,
/// the officiant - and past four the marginal link is somebody who happened to be
/// laughing at something else.
pub const MAX_PER_ACTION: usize = 4;

/// What one candidate frame offers.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The photograph.
    pub image: ImageId,
    /// Its timeline time.
    pub ts: Timestamp,
    /// The moment it belongs to, when it has one.
    pub moment: Option<MomentId>,
    /// Which body took it.
    pub camera: String,
    /// Every face's reading.
    pub faces: Vec<FaceExpression>,
    /// The interactions detected, strongest first.
    pub interactions: Vec<(Interaction, f32)>,
    /// The strongest thing about this frame, `0..1`. The action test's input.
    pub strength: f32,
}

impl Candidate {
    /// The strongest expression on a face that is looking into the room.
    ///
    /// Zero when nobody is engaged, which is what makes condition 3 a single number.
    #[must_use]
    pub fn engaged_intensity(&self) -> f32 {
        self.faces
            .iter()
            .filter(|face| face.gaze.is_engaged())
            .map(FaceExpression::intensity)
            .fold(0.0f32, f32::max)
    }

    /// The identities in this frame.
    #[must_use]
    pub fn identities(&self) -> Vec<IdentityId> {
        self.faces.iter().filter_map(|face| face.identity).collect()
    }
}

/// How much a reaction is worth, by who is reacting.
///
/// Section 6.3: "reactions are scored with a bonus proportional to the action's
/// significance and the reactor's role weight." The role weights are phase 06's ranking
/// order collapsed to four bands rather than ten numbers, because the difference between
/// an aunt and a cousin does not change whether a photograph of them crying is worth
/// keeping.
///
/// An unassigned face gets the guest weight rather than zero. A wedding whose face pass
/// has run but whose clustering has not is a wedding where every reactor is unassigned,
/// and zeroing them would silently switch this feature off.
#[must_use]
pub fn role_weight(role: Role) -> f32 {
    if role.is_couple() {
        return 1.00;
    }
    match role {
        Role::FamilyClose => 0.90,
        Role::FamilyExtended | Role::Vip | Role::Child => 0.70,
        Role::Vendor => 0.25,
        _ => 0.50,
    }
}

/// Link one action frame to its reactions.
///
/// `candidates` is every frame inside the action's time window, from any camera.
/// `roles` maps identities to their phase 06 role; a missing entry is `Role::Unknown`.
///
/// Deterministic: candidates are considered in `(gap, image id)` order and the result is
/// sorted by bonus then by image id, so two runs over the same wedding produce the same
/// links in the same order. Invariant 4.
#[must_use]
pub fn link(
    action: &Candidate,
    candidates: &[Candidate],
    roles: &BTreeMap<IdentityId, Role>,
) -> Vec<ReactionLink> {
    if action.strength < ACTION_FLOOR {
        return Vec::new();
    }
    let subjects = action.identities();

    let mut found: Vec<ReactionLink> = Vec::new();
    for candidate in candidates {
        if candidate.image == action.image {
            continue;
        }
        let gap = candidate.ts.saturating_sub(action.ts);
        if gap.abs() > ReactionLink::WINDOW_MS {
            continue;
        }
        // Condition 2. Same moment and same camera is the burst case: fourteen frames of
        // one toss are not fourteen reactions to each other.
        let same_moment = match (action.moment, candidate.moment) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        };
        if same_moment && candidate.camera == action.camera {
            continue;
        }

        let intensity = candidate.engaged_intensity();
        if intensity < REACTOR_FLOOR {
            continue;
        }

        // The reactor must not be one of the people the action is about. Somebody
        // reacting to themselves is the same photograph twice.
        let reactor: Vec<IdentityId> = candidate
            .identities()
            .into_iter()
            .filter(|identity| !subjects.contains(identity))
            .collect();
        if !subjects.is_empty() && reactor.is_empty() && !candidate.identities().is_empty() {
            continue;
        }

        let weight = reactor
            .iter()
            .map(|identity| role_weight(roles.get(identity).copied().unwrap_or(Role::Unknown)))
            .fold(0.0f32, f32::max)
            .max(if reactor.is_empty() {
                // No identified reactor: an unclustered face, which is real evidence at
                // the guest weight. See `role_weight`.
                role_weight(Role::Guest)
            } else {
                0.0
            });

        let bonus = (action.strength * intensity * weight).clamp(0.0, 1.0);
        let confidence = (intensity * weight).clamp(0.0, 1.0);

        let mut reasons = Vec::with_capacity(ReactionLink::MAX_REASONS);
        reasons.push(EmotionReason::frame(
            EmotionCode::ReactionTo,
            format!(
                "somebody here is turned into the room and reacting, {:.1}s {} the moment itself",
                (gap.abs() as f32) / 1000.0,
                if gap >= 0 { "after" } else { "before" }
            ),
            bonus,
        ));
        if let Some(face) = candidate
            .faces
            .iter()
            .filter(|face| face.gaze.is_engaged())
            .max_by(|left, right| left.intensity().total_cmp(&right.intensity()))
        {
            let code = crate::emotion::peak::lead_code(face);
            reasons.push(EmotionReason::at(
                code,
                code.user_text(),
                face.intensity(),
                face.crop,
            ));
        }
        if let Some((interaction, strength)) = action.interactions.first() {
            reasons.push(EmotionReason::frame(
                EmotionCode::ActionOf,
                format!("the moment being reacted to is {}", interaction.title()),
                *strength,
            ));
        }
        reasons.truncate(ReactionLink::MAX_REASONS);

        found.push(ReactionLink {
            action: action.image,
            reaction: candidate.image,
            gap_ms: gap.clamp(-ReactionLink::WINDOW_MS, ReactionLink::WINDOW_MS),
            bonus,
            confidence,
            reasons,
        });
    }

    found.sort_by(|left, right| {
        right
            .bonus
            .total_cmp(&left.bonus)
            .then_with(|| left.reaction.to_db().cmp(&right.reaction.to_db()))
    });
    found.truncate(MAX_PER_ACTION);
    found
}

/// Resolve a whole project's links so that each reaction points at one action.
///
/// The schema allows many reactions per action and exactly one action per reaction, and
/// this is where that is enforced rather than discovered at INSERT time. A frame that
/// could react to three things reacts to the one with the strongest bonus; ties go to the
/// smaller image id, which is arbitrary and deterministic - the two properties a tie-break
/// needs.
#[must_use]
pub fn resolve(mut links: Vec<ReactionLink>) -> Vec<ReactionLink> {
    links.sort_by(|left, right| {
        right
            .bonus
            .total_cmp(&left.bonus)
            .then_with(|| left.reaction.to_db().cmp(&right.reaction.to_db()))
            .then_with(|| left.action.to_db().cmp(&right.action.to_db()))
    });
    let mut claimed: BTreeMap<ImageId, ImageId> = BTreeMap::new();
    let mut per_action: BTreeMap<ImageId, usize> = BTreeMap::new();
    let mut out = Vec::with_capacity(links.len());
    for link in links {
        if claimed.contains_key(&link.reaction) {
            continue;
        }
        let count = per_action.entry(link.action).or_insert(0);
        if *count >= MAX_PER_ACTION {
            continue;
        }
        *count += 1;
        claimed.insert(link.reaction, link.action);
        out.push(link);
    }
    // Back into a stable order for storage and for the eval harness's comparisons.
    out.sort_by(|left, right| {
        left.action
            .to_db()
            .cmp(&right.action.to_db())
            .then_with(|| left.reaction.to_db().cmp(&right.reaction.to_db()))
    });
    out
}
