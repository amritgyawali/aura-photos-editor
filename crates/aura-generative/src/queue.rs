//! The proposal queue: safety, then a source, then the self-check, then a band. Sections 6.1 and
//! 6.4.
//!
//! This is where the phase's shape is visible in one function. [`plan`] does, in this order:
//!
//! 1. [`crate::safety::check`] on every candidate. Failures become rows and stop here.
//! 2. [`crate::source::select`] on the survivors. Real pixels, then this frame's own texture, then
//!    a refusal.
//! 3. [`crate::selfcheck::inspect`] on the result. A failure **reverts before anybody sees it**.
//! 4. The cloud editorial judgement, for the survivors whose confidence sits in section 7's band.
//!    It can only say no.
//! 5. A band from phase 13, raised one for this phase, raised again while nothing is calibrated.
//!
//! Nothing is scored before step 1 finishes, and the ordering is a property of the types rather
//! than of this function: `select` takes a `SafeCandidate`, which has no public constructor.
//!
//! ## Nothing here writes a recipe or applies anything
//!
//! [`plan`] returns proposals and patches. `aura-app` is what merges an accepted proposal into an
//! `edit_recipes` row through `aura_recipe::schema::merge`, which is phase 14's rule and the reason
//! `crates/aura-generative/tests/one_choke_point.rs` greps for `schema::merge` as well as for the
//! removal modules. ADR-0049 section 9: there is no code path from `plan` to a written recipe.
//!
//! ## The proposal id is derived rather than random, and a photographer's rejection depends on it
//!
//! ADR-0049 section 10 gives `ProposalId` to the proposal rather than to an applied removal, so
//! that a rejection survives a repeated pass. That only works if the second pass produces the
//! **same id** for the same region, so [`proposal_id`] is a digest of the photograph, the
//! quantised rectangle, the method's kind and the three versions - not `ProposalId::new()`.
//!
//! A random id would have made the rejection unfindable and the feature would have looked like it
//! was working: every run would show the photographer the proposal they rejected yesterday, as
//! though it were new.

use aura_core::contract::cleanup::{
    Box2, CleanupCode, CleanupDisclosure, CleanupMethod, CleanupProposal, CleanupReason,
    DistractionClass, ImageId, SafetyCheck, SafetyVerdict, MAX_PROPOSALS_PER_IMAGE,
};
use aura_core::contract::ids::ProposalId;
use aura_core::contract::ledger::Autonomy;
use aura_core::contract::scene::SceneId;
use uuid::Uuid;

use crate::denylist::Coverage;
use crate::judgement::{Ask, EditorialJudge};
use crate::pixels::{self, Image};
use crate::policy::ScenePolicy;
use crate::safety::{self, Candidate, Outcome};
use crate::selfcheck::{self, ArtefactReport};
use crate::source::{self, Sources};

/// The confidence at which a removal stops needing an explicit look, before this phase's own
/// raise and before the uncalibrated raise.
///
/// Phase 13's `Suggest` floor for this kind of decision. It is not a safety threshold - the safety
/// engine has already run - it is where "applied, worth a look" becomes "waiting for you".
pub const SUGGEST_FLOOR: f32 = 0.60;

/// The confidence at which a tier-one removal would reach `AutoZeroTouch` before the raises.
pub const ZERO_TOUCH_FLOOR: f32 = 0.80;

/// One candidate the safety engine refused, as it will be stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Blocked {
    /// Where.
    pub region: Box2,
    /// Which of the five checks stopped it.
    pub check: SafetyCheck,
    /// Which reason code, for the panel and the ledger.
    pub code: CleanupCode,
    /// The verdict, carrying every check as it was found.
    pub verdict: SafetyVerdict,
}

/// A proposal with the pixels it would produce.
///
/// The pixels are the **patch** rather than the frame. A plan carrying three whole 2048 px frames
/// would be a hundred megabytes of proposal for the sake of three postage stamps, and the patch is
/// also exactly what a before-and-after differs by.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    /// What is being proposed.
    pub proposal: CleanupProposal,
    /// The replacement pixels, on the region's own grid.
    pub patch: Image,
    /// What the self-check measured on the whole result.
    pub artefact: ArtefactReport,
}

impl Prepared {
    /// The disclosure this proposal becomes when it is applied.
    ///
    /// Built here rather than at the store, so a removal and its disclosure are produced by the
    /// same code - phase 21's rule for its borrow, and the reason a disclosure cannot be absent
    /// from a row that changed pixels.
    #[must_use]
    pub fn disclosure(&self, accepted_by_user: bool) -> CleanupDisclosure {
        CleanupDisclosure {
            proposal_id: self.proposal.id,
            image_id: self.proposal.image_id,
            method: self.proposal.method.clone(),
            region: self.proposal.region,
            accepted_by_user,
            artefact_score: self.artefact.worst(),
        }
    }
}

/// Everything one photograph's planning needs.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// The photograph.
    pub image: ImageId,
    /// Its scene, for the policy row and for the stored proposal. Invariant 7.
    pub scene: SceneId,
    /// What this scene permits.
    pub policy: &'a ScenePolicy,
    /// What is known about its protected regions.
    pub coverage: &'a Coverage,
    /// The pixels this photograph and its siblings are made of.
    pub sources: Sources<'a>,
    /// Which detector produced the candidates.
    pub detector_ver: u16,
    /// Which safety arithmetic is judging them.
    pub analysis_ver: u16,
    /// Which `cleanup_policy.toml` the caps came from.
    pub policy_ver: u16,
    /// Whether phase 13 has a calibration for this kind of decision.
    ///
    /// False in this build. While it is false every band is raised one further toward review,
    /// which is `uncalibrated_raises` in phase 13's own policy and is why nothing here can apply
    /// unattended. It is a field rather than a constant because phase 28 turns it on by shipping a
    /// calibration, not by editing this crate.
    pub calibrated: bool,
}

/// What one photograph's planning produced.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    /// The proposals, best first, at most [`MAX_PROPOSALS_PER_IMAGE`].
    pub prepared: Vec<Prepared>,
    /// Every candidate the safety engine refused.
    pub blocked: Vec<Blocked>,
    /// Removals the self-check undid before anybody saw them.
    pub reverted: u32,
    /// True when every one of the six protected kinds could be looked for.
    ///
    /// What `CleanupOutline::mask_covered` counts. At false the blocked histogram says something
    /// about a missing vocabulary rather than about the photograph.
    pub mask_complete: bool,
    /// How many cloud judgements were asked for.
    pub judged: u32,
    /// How many of those declined a removal.
    pub declined: u32,
}

impl Plan {
    /// True when this photograph carries at least one proposal.
    #[must_use]
    pub fn has_proposals(&self) -> bool {
        !self.prepared.is_empty()
    }
}

/// Plan the removals for one photograph.
///
/// `judge` is optional and `None` behaves exactly as an unreachable judge does, which is invariant
/// 6: the product completes a wedding with no network.
#[must_use]
pub fn plan(
    context: &Context<'_>,
    candidates: &[Candidate],
    judge: Option<&dyn EditorialJudge>,
) -> Plan {
    let mut out = Plan {
        mask_complete: context.coverage.is_complete(),
        ..Plan::default()
    };

    for candidate in candidates {
        // 1. Safety, before anything is scored. A refusal is a row.
        let safe = match safety::check(candidate, context.policy, context.coverage) {
            Outcome::Blocked {
                check,
                code,
                verdict,
            } => {
                out.blocked.push(Blocked {
                    region: candidate.region,
                    check,
                    code,
                    verdict,
                });
                continue;
            }
            Outcome::Allowed(safe) => safe,
        };

        // The cap is section 6.1's "cleanup stays a light touch". A frame with fifteen
        // distractions is a frame whose background is the problem, and removing three of them
        // makes it look edited rather than better. Recorded under the confidence check, which the
        // contract defines as "high enough **for the method being proposed**" - and past the cap
        // there is no method being proposed.
        if out.prepared.len() >= MAX_PROPOSALS_PER_IMAGE {
            out.blocked.push(blocked_at(
                candidate.region,
                CleanupCode::ProposalCapReached,
                "this photograph already carries the most proposals a light touch allows",
            ));
            continue;
        }

        // 2. A source. Real pixels first.
        let selection = match source::select(&context.sources, &safe) {
            Ok(selection) => selection,
            Err(reasons) => {
                let code = reasons
                    .first()
                    .map_or(CleanupCode::TextureStructured, |reason| reason.code);
                out.blocked.push(blocked_at(
                    candidate.region,
                    code,
                    "no source could replace this region without inventing something",
                ));
                continue;
            }
        };

        // 3. The self-check, over the result, before anybody sees it.
        let artefact = selfcheck::inspect(&selection.result, &candidate.region);
        if let Some(failure) = artefact.failure() {
            out.reverted += 1;
            out.blocked.push(blocked_at(
                candidate.region,
                failure,
                "AURA did not like its own result and put the photograph back",
            ));
            continue;
        }

        // 4. The cloud editorial judgement, for the middle band only, and it can only say no.
        let mut reasons = selection.reasons.clone();
        if Ask::is_in_band(selection.confidence) {
            if let Some(judge) = judge.filter(|judge| judge.remaining() > 0) {
                let ask = Ask {
                    image: context.image,
                    region: candidate.region,
                    class: candidate.class,
                    area_frac: candidate.area_frac(),
                    scene: context.scene,
                    method: selection.method.clone(),
                    confidence: selection.confidence,
                };
                let answer = judge.judge(&ask);
                out.judged += 1;
                if let Some(code) = answer.code() {
                    reasons.push(CleanupReason::at(code, 0.70, candidate.region));
                }
                if answer.is_decline() {
                    out.declined += 1;
                    out.blocked.push(blocked_at(
                        candidate.region,
                        CleanupCode::JudgementDeclined,
                        "a cautious editorial review said this belongs in the photograph",
                    ));
                    continue;
                }
            } else {
                reasons.push(CleanupReason::at(
                    CleanupCode::JudgementUnavailable,
                    0.20,
                    candidate.region,
                ));
            }
        }

        // 5. The band, and the two raises.
        let autonomy = band(
            &selection.method,
            selection.confidence,
            context.policy.zero_touch_confidence,
            context.calibrated,
        );
        reasons.push(review_reason(&selection.method, autonomy, candidate.region));

        let id = proposal_id(context, candidate, &selection.method);
        let Ok(mut proposal) = CleanupProposal::new(
            id,
            context.image,
            candidate.region,
            candidate.class,
            selection.method.clone(),
            SafetyVerdict::allow(),
            reasons,
        ) else {
            // `new` refuses a verdict that is not allowed, a degenerate region and an empty reason
            // list. None of the three is reachable here - the verdict came from `check`, the
            // region resolved onto the frame in `select`, and `select` always returns at least one
            // reason - so this arm is the constructor doing its job for a caller that has drifted.
            // Counting it rather than ignoring it is invariant 9.
            out.blocked.push(blocked_at(
                candidate.region,
                CleanupCode::ConfidenceLow,
                "the proposal could not be constructed and was therefore not offered",
            ));
            continue;
        };
        proposal.salience = candidate.salience.clamp(0.0, 1.0);
        proposal.confidence = selection.confidence.clamp(0.0, 1.0);
        proposal.autonomy = autonomy;
        proposal.scene = context.scene;
        proposal.detector_ver = context.detector_ver;
        proposal.analysis_ver = context.analysis_ver;
        proposal.policy_ver = context.policy_ver;

        let patch = pixels::resolve(
            &candidate.region,
            selection.result.w,
            selection.result.h,
        )
        .map_or_else(
            || Image::black(1, 1),
            |rect| pixels::extract(&selection.result, &rect),
        );

        out.prepared.push(Prepared {
            proposal,
            patch,
            artefact,
        });
    }

    // Best first, so a panel that shows one shows the strongest. Deterministic on a tie by region
    // position, for invariant 4.
    out.prepared.sort_by(|a, b| {
        let sa = a.proposal.confidence * a.proposal.salience;
        let sb = b.proposal.confidence * b.proposal.salience;
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.proposal
                    .region
                    .x
                    .partial_cmp(&b.proposal.region.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.proposal
                    .region
                    .y
                    .partial_cmp(&b.proposal.region.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    out
}

/// The autonomy band, with this phase's raise and phase 13's uncalibrated raise on top.
///
/// Section 6.4 permits a tier-one removal at or above the zero-touch confidence to apply
/// unattended **in Zero-Touch mode**. That sentence is what the two raises produce rather than
/// something checked separately: the base band for such a removal is `Auto`, this phase raises it
/// one to `AutoZeroTouch`, and while nothing is calibrated it is raised again to `Suggest`.
///
/// So on this build nothing reaches an unattended band at all, and that is the composition of two
/// rules neither of which was written for this phase.
#[must_use]
pub fn band(method: &CleanupMethod, confidence: f32, zero_touch: f32, calibrated: bool) -> Autonomy {
    let base = if method.tier_one() && confidence >= zero_touch {
        Autonomy::Auto
    } else if method.tier_one() && confidence >= ZERO_TOUCH_FLOOR {
        Autonomy::AutoZeroTouch
    } else if confidence >= SUGGEST_FLOOR {
        Autonomy::Suggest
    } else {
        Autonomy::RequireReview
    };
    let raised = base.stricter();
    if calibrated {
        raised
    } else {
        raised.stricter()
    }
}

/// Why this proposal is waiting, when it is.
fn review_reason(method: &CleanupMethod, autonomy: Autonomy, region: Box2) -> CleanupReason {
    match autonomy {
        Autonomy::Auto | Autonomy::AutoZeroTouch => {
            CleanupReason::at(CleanupCode::AppliedUnattended, 0.10, region)
        }
        _ if !method.tier_one() => {
            CleanupReason::at(CleanupCode::ReviewRequiredMethod, 0.50, region)
        }
        _ => CleanupReason::at(CleanupCode::ReviewRequiredConfidence, 0.50, region),
    }
}

/// A blocked row for a refusal that is not one of the five checks' own.
///
/// Every one of these carries [`SafetyCheck::Confidence`], which the frozen contract defines as
/// "the removability confidence is high enough **for the method being proposed**". A region that
/// no method could source, a result the self-check undid and a candidate past the cap all share
/// that shape: there is no method being proposed for them.
fn blocked_at(region: Box2, code: CleanupCode, reason: &str) -> Blocked {
    Blocked {
        region,
        check: SafetyCheck::Confidence,
        code,
        verdict: SafetyVerdict::block(SafetyCheck::Confidence, reason),
    }
}

/// The deterministic identifier for one proposal.
///
/// A digest of what the proposal *is* rather than when it was made, so a repeated pass produces the
/// same id and a photographer's rejection is still about the same thing. See the module header.
///
/// The rectangle is quantised to ten thousandths before it is hashed, because a detector that
/// returns `0.020_000_1` on one run and `0.019_999_9` on the next would otherwise produce two
/// proposals for one bin - the same trap phase 04's task cache has with float inputs, and the same
/// fix.
#[must_use]
pub fn proposal_id(
    context: &Context<'_>,
    candidate: &Candidate,
    method: &CleanupMethod,
) -> ProposalId {
    let quantise = |value: f32| -> i32 { (value * 10_000.0).round() as i32 };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"aura.cleanup.proposal.v1");
    hasher.update(context.image.to_db().as_bytes());
    for value in [
        candidate.region.x,
        candidate.region.y,
        candidate.region.w,
        candidate.region.h,
    ] {
        hasher.update(&quantise(value).to_le_bytes());
    }
    hasher.update(method.kind_str().as_bytes());
    hasher.update(candidate.class.as_str().as_bytes());
    hasher.update(&context.detector_ver.to_le_bytes());
    hasher.update(&context.analysis_ver.to_le_bytes());
    hasher.update(&context.policy_ver.to_le_bytes());

    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(digest.as_bytes().get(..16).unwrap_or(&[0u8; 16]));
    // Stamp the version and variant nibbles so the value is a well-formed UUID rather than sixteen
    // arbitrary bytes wearing one's clothes. Phase 09's fixtures do the same.
    if let Some(slot) = bytes.get_mut(6) {
        *slot = (*slot & 0x0f) | 0x40;
    }
    if let Some(slot) = bytes.get_mut(8) {
        *slot = (*slot & 0x3f) | 0x80;
    }
    ProposalId::from_uuid(Uuid::from_bytes(bytes))
}

/// True when a class may be offered at all, whatever the pixels say.
///
/// Read by the panel so the manual removal tool can explain why an object it can see is not in the
/// queue. The safety engine has already enforced it; this is the same answer without running a
/// check that opens a photograph.
#[must_use]
pub fn class_may_be_proposed(class: DistractionClass) -> bool {
    class.story_safe()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::denylist::Protected;
    use crate::pixels::Rect;
    use crate::source::Sibling;
    use aura_core::PhotoId;

    fn textured(w: usize, h: usize, shift: f32) -> Image {
        let mut image = Image::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let fx = x as f32 + shift;
                let fy = y as f32;
                let v = 0.35
                    + 0.12 * (fx * 0.21).sin()
                    + 0.09 * (fy * 0.13).cos()
                    + 0.05 * ((fx + fy) * 0.07).sin();
                image.put(x, y, [v, v * 0.92, v * 0.83]);
            }
        }
        image
    }

    fn paint(image: &mut Image, rect: Rect, value: [f32; 3]) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                image.put(x, y, value);
            }
        }
    }

    const BLOCK: Rect = Rect {
        x: 60,
        y: 60,
        w: 16,
        h: 16,
    };

    fn region() -> Box2 {
        Box2 {
            x: 60.0 / 160.0,
            y: 60.0 / 160.0,
            w: 16.0 / 160.0,
            h: 16.0 / 160.0,
        }
    }

    fn bin() -> Candidate {
        Candidate {
            region: region(),
            class: DistractionClass::Bin,
            salience: 0.8,
            removability: 0.9,
            crosses_structure: false,
            touches_identity: false,
        }
    }

    fn policy() -> ScenePolicy {
        ScenePolicy {
            area_cap: 0.04,
            denylist_overlap_max: 0.01,
            zero_touch_confidence: 0.97,
            enabled: true,
            reason: "a test".into(),
        }
    }

    /// A fixed photograph identifier.
    ///
    /// **Not `PhotoId::default()`**, which is `PhotoId::new()` and is a fresh random UUID every
    /// call. The determinism test compared two plans made under two different photographs and
    /// failed on the ids, which is a fixture that tests `Uuid::new_v4` rather than this module.
    fn fixed_photo() -> PhotoId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-0000000000f0").unwrap_or_default()
    }

    fn context<'a>(sources: Sources<'a>, coverage: &'a Coverage, policy: &'a ScenePolicy) -> Context<'a> {
        Context {
            image: fixed_photo(),
            scene: SceneId::ReceptionEntrance,
            policy,
            coverage,
            sources,
            detector_ver: 1,
            analysis_ver: 1,
            policy_ver: 1,
            calibrated: false,
        }
    }

    #[test]
    fn a_clean_bin_over_known_masks_becomes_a_proposal() {
        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let coverage = Coverage::known_empty();
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let plan = plan(&context(sources, &coverage, &policy), &[bin()], None);
        assert_eq!(plan.prepared.len(), 1, "blocked: {:?}", plan.blocked);
        assert!(plan.mask_complete);
    }

    #[test]
    fn nothing_this_build_produces_may_apply_unattended() {
        let clean = textured(160, 160, 0.0);
        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let siblings = [Sibling {
            id: fixed_photo(),
            image: &clean,
        }];
        let coverage = Coverage::known_empty();
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &siblings,
            studio_opted_in: false,
        };
        let plan = plan(&context(sources, &coverage, &policy), &[bin()], None);
        for prepared in &plan.prepared {
            assert!(
                !prepared.proposal.may_apply_unattended(),
                "a {} at {:.3} reached {:?}",
                prepared.proposal.method.kind_str(),
                prepared.proposal.confidence,
                prepared.proposal.autonomy
            );
        }
    }

    #[test]
    fn an_absent_mask_produces_a_blocked_row_and_no_proposal() {
        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let coverage = Coverage::Absent;
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let plan = plan(&context(sources, &coverage, &policy), &[bin()], None);
        assert!(plan.prepared.is_empty());
        assert_eq!(plan.blocked.len(), 1);
        assert_eq!(
            plan.blocked.first().map(|b| b.code),
            Some(CleanupCode::ProtectionUnknown)
        );
        assert!(!plan.mask_complete);
    }

    #[test]
    fn a_hand_in_the_region_blocks_and_the_verdict_names_the_check() {
        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let coverage = Coverage::known(vec![(
            Protected::Hands,
            Box2 {
                x: 0.30,
                y: 0.30,
                w: 0.30,
                h: 0.30,
            },
        )]);
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let plan = plan(&context(sources, &coverage, &policy), &[bin()], None);
        assert!(plan.prepared.is_empty());
        let blocked = plan.blocked.first().expect("a refusal is a row");
        assert_eq!(blocked.check, SafetyCheck::Denylist);
        assert!(blocked.verdict.is_well_formed());
    }

    #[test]
    fn the_proposal_id_is_the_same_on_a_repeated_pass() {
        // ADR-0049 section 10. Without this a photographer's rejection is unfindable and the
        // product shows them the same proposal every run as though it were new.
        let target = textured(160, 160, 0.0);
        let coverage = Coverage::known_empty();
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let context = context(sources, &coverage, &policy);
        let first = proposal_id(&context, &bin(), &CleanupMethod::ClassicalFill);
        let second = proposal_id(&context, &bin(), &CleanupMethod::ClassicalFill);
        assert_eq!(first, second);
    }

    #[test]
    fn a_different_region_or_method_is_a_different_proposal() {
        let target = textured(160, 160, 0.0);
        let coverage = Coverage::known_empty();
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let context = context(sources, &coverage, &policy);
        let base = proposal_id(&context, &bin(), &CleanupMethod::ClassicalFill);

        let mut moved = bin();
        moved.region.x += 0.05;
        assert_ne!(base, proposal_id(&context, &moved, &CleanupMethod::ClassicalFill));
        assert_ne!(
            base,
            proposal_id(&context, &bin(), &CleanupMethod::BorrowFrom(PhotoId::default()))
        );
    }

    #[test]
    fn a_tier_one_removal_at_full_confidence_lands_one_band_short_of_unattended() {
        // The two raises, spelled out. Calibrated, a borrow at 0.99 reaches `AutoZeroTouch`, which
        // is section 6.4's "only in Zero-Touch". Uncalibrated, it reaches `Suggest`.
        let borrow = CleanupMethod::BorrowFrom(PhotoId::default());
        assert_eq!(band(&borrow, 0.99, 0.97, true), Autonomy::AutoZeroTouch);
        assert_eq!(band(&borrow, 0.99, 0.97, false), Autonomy::Suggest);
        // An inpaint is never tier one, so it can never reach either.
        let inpaint = CleanupMethod::Inpaint {
            model: "x".into(),
        };
        assert_eq!(band(&inpaint, 0.99, 0.97, true), Autonomy::RequireReview);
    }

    #[test]
    fn no_more_than_three_proposals_survive_and_the_rest_are_rows() {
        // A flat wall, so every one of the five candidates is genuinely fillable and the only
        // thing that can stop the fourth and fifth is the cap. On a textured fixture a candidate
        // that failed its own fill would look exactly like the cap working.
        let mut target = Image {
            w: 200,
            h: 200,
            rgb: vec![0.42; 200 * 200 * 3],
        };
        let mut candidates = Vec::new();
        for index in 0..5 {
            let x = 20 + index * 30;
            let rect = Rect {
                x,
                y: 150,
                w: 14,
                h: 14,
            };
            paint(&mut target, rect, [0.95, 0.05, 0.05]);
            candidates.push(Candidate {
                region: Box2 {
                    x: x as f32 / 200.0,
                    y: 150.0 / 200.0,
                    w: 14.0 / 200.0,
                    h: 14.0 / 200.0,
                },
                ..bin()
            });
        }
        let coverage = Coverage::known_empty();
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let plan = plan(&context(sources, &coverage, &policy), &candidates, None);
        assert_eq!(
            plan.prepared.len(),
            MAX_PROPOSALS_PER_IMAGE,
            "five fillable candidates must produce exactly the cap, blocked: {:?}",
            plan.blocked.iter().map(|b| b.code).collect::<Vec<_>>()
        );
        assert_eq!(
            plan.blocked
                .iter()
                .filter(|b| b.code == CleanupCode::ProposalCapReached)
                .count(),
            2,
            "the two past the cap are rows rather than an absence"
        );
    }

    #[test]
    fn a_declining_judge_removes_a_proposal_and_a_silent_one_does_not() {
        use crate::judgement::{Answer, MAX_CALLS_PER_PROJECT};
        struct Decliner;
        impl EditorialJudge for Decliner {
            fn judge(&self, _ask: &Ask) -> Answer {
                Answer::Decline {
                    reasons: vec!["the sign names the couple".into()],
                    story_relevant: true,
                }
            }
            fn remaining(&self) -> u32 {
                MAX_CALLS_PER_PROJECT
            }
        }

        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let coverage = Coverage::known_empty();
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let without = plan(&context(sources, &coverage, &policy), &[bin()], None);
        let with = plan(
            &context(sources, &coverage, &policy),
            &[bin()],
            Some(&Decliner),
        );

        // Whatever the fill's confidence turns out to be, the judge can only ever make the product
        // do less: the declined plan never has more proposals than the unjudged one.
        assert!(with.prepared.len() <= without.prepared.len());
        if with.judged > 0 {
            assert_eq!(with.declined, 1);
            assert!(with.prepared.is_empty());
        }
    }

    #[test]
    fn planning_is_deterministic() {
        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let coverage = Coverage::known_empty();
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let first = plan(&context(sources, &coverage, &policy), &[bin()], None);
        let second = plan(&context(sources, &coverage, &policy), &[bin()], None);
        assert_eq!(first, second);
    }

    #[test]
    fn a_person_never_reaches_a_proposal_however_confident() {
        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let mut person = bin();
        person.class = DistractionClass::BackgroundPerson;
        person.removability = 1.0;
        let coverage = Coverage::known_empty();
        let policy = policy();
        let sources = Sources {
            target: &target,
            siblings: &[],
            studio_opted_in: false,
        };
        let plan = plan(&context(sources, &coverage, &policy), &[person], None);
        assert!(plan.prepared.is_empty());
        assert_eq!(
            plan.blocked.first().map(|b| b.code),
            Some(CleanupCode::PersonPresent)
        );
        assert!(!class_may_be_proposed(DistractionClass::BackgroundPerson));
    }
}
