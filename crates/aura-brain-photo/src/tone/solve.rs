//! The constrained solve: choose a light, then decide how much of it to remove.
//!
//! PHASE-15 section 8's step 5, and section 3's data flow in one function:
//!
//! ```text
//! constrained solve:  minimise (skin chroma error + neutral error)
//!                     subject to (no new clipping, scene intent, plausible skin locus)
//! ```
//!
//! ## Choosing a light and correcting for it are two decisions
//!
//! This is the module's central idea and it is what makes section 2.1's coloured-light
//! requirement expressible at all. Estimating that a dance floor is lit by a purple wash is a
//! *measurement*; deciding how much of the purple to remove is a *product decision*. A solver
//! that fused them would have only one answer available - full neutralisation - and section
//! 12's second failure mode is what that ships.
//!
//! So [`choose`] picks the light, and [`correct`] decides where between "leave it alone" and
//! "neutralise it completely" the recipe should sit. The scene's own row supplies the policy
//! for the second, and the skin locus supplies the floor: however much mood a scene wants to
//! keep, it may not keep so much that a person's skin stops being plausible.
//!
//! ## What a recipe temperature actually means
//!
//! Worth stating once, because getting it backwards is easy and the failure is invisible in
//! a unit test that only checks a number moved. Setting the recipe's temperature **equal to
//! the light's own** renders the frame neutral. Setting it **higher** leaves warmth behind.
//! Setting it **lower** over-corrects into blue.
//!
//! So `SceneTarget::tungsten_floor_k` is a floor on the value written, and a floor is an
//! *under*-correction: when the light is warmer than the floor, the correction stops at the
//! floor and the remaining warmth is kept. `aura_render::colour::white_balance` is the other
//! half of this sentence and the two are asserted against each other in
//! `tests/eval/tone_eval.rs`.

use aura_core::contract::tone::{
    HypothesisSource, Illuminant, ToneAlternative, ToneCode, KELVIN_RANGE, TINT_RANGE,
};
use aura_core::AttrFlags;
use aura_raw::colour::illuminant::{cct_from_uv, uv_distance};

use crate::tone::illuminant::{self, AsShot, Hypothesis, MixedLight};
use crate::tone::neutrals::NeutralPatch;
use crate::tone::store::codec::{self, Explained};
use crate::tone::targets::{SceneTarget, UNTARGETED_CONFIDENCE_PENALTY};
use crate::tone::wb::{self, Constraints, Score};

/// How many interpolation steps the partial correction searches.
///
/// Twenty, over the segment from "leave the light alone" to "remove it completely". A
/// twentieth of that segment is well below what anybody can see, and a bisection would be
/// wrong here: the constraint is not monotonic along the segment when two people with
/// different loci are in frame, and a linear scan finds the first satisfying point where a
/// bisection would find an arbitrary one.
pub const CORRECTION_STEPS: usize = 20;

/// How far apart in `u'v'` two independent generators may land and still count as agreeing.
///
/// **The evidence behind the white-balance confidence is agreement between the top two
/// answers, not the gap between their costs.** That distinction is the whole of this
/// constant and it was got wrong once, so it is written down: a cost gap says the winner
/// explained the evidence better than the runner-up, and it collapses to nothing in exactly
/// the case that should be the *most* confident - two independent estimators landing on the
/// same chromaticity. Reading a collapsed gap as doubt made every frame in a wedding score
/// below `skin_locus::MIN_CONTRIBUTING_CONF`, which meant no frame ever contributed a skin
/// sample, which meant no locus was ever built, which meant section 6.3's hard constraint
/// never bound on anything. A confidence model that cannot bootstrap the mechanism it
/// gates is worse than none, because it fails silently and looks careful.
///
/// 0.004 in `u'v'` is section 10.1's own 200 K tolerance near daylight. Two generators
/// inside it have given the same answer to the precision the phase is measured at;
/// agreement falls linearly to nothing at [`AGREE_UV_NONE`], which is far enough apart that
/// a photographer would call the frame ambiguous too. ADR-0031 section 5 has the derivation.
pub const AGREE_UV: f32 = 0.004;

/// The `u'v'` separation at which two answers carry no agreement at all.
///
/// Three times [`AGREE_UV`], which near daylight is about 600 K - a difference nobody would
/// describe as the same white balance.
pub const AGREE_UV_NONE: f32 = 0.012;

/// What the white-balance solve decided.
#[derive(Debug, Clone, PartialEq)]
pub struct WbSolution {
    /// The temperature to write into the recipe, in kelvin.
    pub temperature_k: f32,
    /// The tint to write into the recipe.
    pub tint: f32,
    /// How sure it is, `0..1`.
    pub confidence: f32,
    /// Every light found, the chosen one first.
    pub illuminants: Vec<Illuminant>,
    /// True when a saturated light was preserved rather than neutralised.
    pub coloured_light: bool,
    /// The estimated skin error this answer leaves, in dE00.
    pub skin_de00: f32,
    /// Runner-up answers, best first.
    pub alternatives: Vec<ToneAlternative>,
    /// Why, in the order the module decided them, each with the numbers its sentence
    /// names.
    pub reasons: Vec<Explained>,
}

/// What [`choose`] settled on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chosen {
    /// The winning hypothesis.
    pub hypothesis: Hypothesis,
    /// Its score.
    pub score: Score,
    /// How much better it was than the runner-up, in cost. Zero when there was no runner-up.
    ///
    /// Reported in the alternatives and used for nothing else. See [`AGREE_UV`] for why the
    /// confidence does not read it.
    pub margin: f32,
    /// How closely the top two answers agreed, `0..1`. One when there was no runner-up.
    ///
    /// One rather than zero for a single hypothesis, because "nothing disagreed" is the
    /// honest reading - and the `evidence` term in the confidence is what accounts for a
    /// frame that had only the camera's own guess to go on.
    pub agreement: f32,
    /// True when every hypothesis violated a skin locus and the least-bad one was taken.
    pub all_vetoed: bool,
}

/// Pick the light.
///
/// Returns `None` only when there were no hypotheses at all, which cannot happen through
/// [`illuminant::generate`] - it always produces the camera's own as-shot value - and is
/// handled anyway because a function that can return nothing should say so rather than invent
/// a default that a caller then treats as a measurement.
#[must_use]
pub fn choose(
    hypotheses: &[Hypothesis],
    constraints: &Constraints,
    patches: &[NeutralPatch],
) -> Option<Chosen> {
    if hypotheses.is_empty() {
        return None;
    }
    let mut scored: Vec<(Hypothesis, Score)> = hypotheses
        .iter()
        .map(|hypothesis| (*hypothesis, wb::score(hypothesis, constraints, patches)))
        .collect();

    // Ties break by the generator's position in `HypothesisSource::ALL`, which is fixed.
    // Without that, two hypotheses with identical costs would be ordered by whatever
    // `sort_by` happened to do with equal keys, and a white balance that depends on a sort's
    // stability is a white balance that changes when the standard library does.
    scored.sort_by(|a, b| {
        a.1.cost
            .total_cmp(&b.1.cost)
            .then_with(|| source_rank(a.0.source).cmp(&source_rank(b.0.source)))
    });

    let all_vetoed = scored.iter().all(|(_, score)| score.vetoed);
    if all_vetoed {
        // Every answer would make somebody's skin implausible, which means one of the two
        // measurements is wrong - the locus or the patch. Take the least-bad rather than
        // refusing: a frame with a slightly wrong white balance is recoverable and a frame
        // with none is a hole in the gallery. The confidence and the reason both say so.
        let mut relaxed = scored.clone();
        relaxed.sort_by(|a, b| {
            a.1.skin
                .total_cmp(&b.1.skin)
                .then_with(|| source_rank(a.0.source).cmp(&source_rank(b.0.source)))
        });
        let (least_bad, score) = relaxed.first().copied()?;
        return Some(Chosen {
            hypothesis: least_bad,
            score,
            margin: 0.0,
            agreement: 0.0,
            all_vetoed: true,
        });
    }

    let (winner, score) = scored.first().copied()?;
    let margin = scored
        .get(1)
        .map_or(0.0, |(_, runner_up)| runner_up.cost - score.cost);
    let agreement = scored.get(1).map_or(1.0, |(runner_up, _)| {
        let distance = uv_distance(winner.uv, runner_up.uv);
        (1.0 - (distance - AGREE_UV) / (AGREE_UV_NONE - AGREE_UV)).clamp(0.0, 1.0)
    });
    Some(Chosen {
        hypothesis: winner,
        score,
        margin,
        agreement,
        all_vetoed: false,
    })
}

/// Position of a generator in the fixed order. The tie-break.
fn source_rank(source: HypothesisSource) -> usize {
    HypothesisSource::ALL
        .iter()
        .position(|candidate| *candidate == source)
        .unwrap_or(usize::MAX)
}

/// Decide how much of the chosen light to remove, and write the answer.
///
/// The second of the two decisions this module separates. Everything here is policy: the
/// scene's mood, the correction bound, the tungsten floor and the skin floor.
#[must_use]
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn correct(
    chosen: Chosen,
    constraints: &Constraints,
    target: SceneTarget,
    as_shot: AsShot,
    mixed: &MixedLight,
    attrs: AttrFlags,
    hypotheses: &[Hypothesis],
    patches: &[NeutralPatch],
) -> WbSolution {
    let mut reasons: Vec<Explained> = Vec::new();
    let light_uv = chosen.hypothesis.uv;
    let mut coloured_light = false;

    // The chosen light, described. Everything below adjusts how much of it is removed; the
    // recorded illuminant is the measurement and does not move.
    let primary = illuminant::describe(
        light_uv,
        chosen.hypothesis.source,
        1.0,
        chosen.hypothesis.region,
        attrs,
    );

    // Where a full correction would land: the recipe temperature equal to the light's own.
    let full_k = primary.cct_k;
    let full_tint = primary.tint;

    // The starting point for a partial correction: the camera's own value, which is what
    // "leave the light alone" means in the recipe's units.
    let rest_k = if as_shot.is_known() {
        as_shot.temperature_k
    } else {
        full_k
    };

    let mut chosen_k = full_k;
    let mut chosen_tint = full_tint;

    // --- the preserve-mood policy -------------------------------------------------------
    //
    // The witness is the room rather than the winner. `primary` describes whichever
    // hypothesis best explained the *subject*, and on a coloured dance floor that is
    // white-patch before the wedding has any skin loci and the skin-anchored answer after -
    // two readings of one wash, on opposite sides of `SATURATED_ABOVE`, which made the same
    // room classify as `Stage` early in a project and `Led` later. `illuminant::ambient` asks
    // the two generators that measure the light falling on the frame, so the answer stops
    // depending on how much of the wedding has been analysed. Either witness is enough:
    // dropping the `primary` test would lose the frames where the coloured light *is* the
    // chosen one, which is every faceless frame on the same dance floor.
    //
    // The ambient witness is held to two tests the chosen one is not, and both are about the
    // same hazard: the two generators `ambient` reads are exactly the two this module's table
    // lists as confounded by a strongly coloured *surface*, so on a red mandap or a green
    // marquee they report a saturated chromaticity for a room that is lit neutrally.
    //
    // First, the scene must be one phase 07 marked `STAGE`. `classify` will otherwise still
    // reach `Stage` on its own evidence, at twice the saturation threshold - which is a sound
    // fallback for a chromaticity somebody chose as the answer and an unsound one for a
    // reading taken off a wall. The mandap fixture clears twice the threshold, so this is not
    // a hypothetical: without the attribute test it preserved a red cast on all five ritual
    // frames and cost four of them the white-balance gate.
    //
    // Second, the light's *kind* must be intentional, where the chosen hypothesis is held to
    // `should_preserve`. That one also accepts raw saturation, which is the same leak by a
    // shorter route.
    //
    // What is left is a witness that can only say "the room phase 07 called a stage is lit by
    // something a long way off the locus", which is the claim actually being made.
    let ambient = if attrs.contains(AttrFlags::STAGE) {
        illuminant::ambient(hypotheses, attrs)
    } else {
        None
    };
    let light_is_a_choice =
        primary.should_preserve() || ambient.is_some_and(|light| light.kind.is_intentional());
    if light_is_a_choice && target.preserve_coloured_light {
        coloured_light = true;
        if constraints.is_empty() {
            // Nothing to protect, so nothing to compromise. The light stays as it was and
            // the panel says the product chose to leave it.
            chosen_k = rest_k;
            chosen_tint = if as_shot.is_known() {
                as_shot.tint
            } else {
                0.0
            };
            reasons.push(codec::plain(ToneCode::ColouredLightPreserved, 0.0));
        } else {
            // Section 6.2's compromise: "correct only enough to keep skin within its
            // plausible locus". Walk from leaving it alone toward removing it entirely and
            // stop at the first point where every prominent person is plausible again.
            //
            // **The walk is in `u'v'`, not in kelvin.** Invariant 8, and here it is load-
            // bearing rather than tidy: a coloured light is by definition off the Planckian
            // locus, and interpolating its temperature walks *along* the locus and throws
            // the tint away at every step. On the purple dance floor that put every one of
            // the twenty candidates back on the locus, so none of them ever satisfied a skin
            // constraint the wash had pushed off it, so the scan always fell through to the
            // full correction below. The endpoints are chromaticities and the line between
            // them is a straight one in the plane the constraint is written in.
            let rest_uv = if as_shot.is_known() {
                illuminant::uv_of(as_shot.temperature_k, as_shot.tint)
            } else {
                light_uv
            };
            let mut landed = None;
            for step in 0..=CORRECTION_STEPS {
                let t = step as f32 / CORRECTION_STEPS as f32;
                let uv = [
                    rest_uv[0] + (light_uv[0] - rest_uv[0]) * t,
                    rest_uv[1] + (light_uv[1] - rest_uv[1]) * t,
                ];
                if !constraints.rejects(uv) {
                    landed = Some((t, uv));
                    break;
                }
            }
            match landed {
                Some((t, _)) if t <= f32::EPSILON => {
                    chosen_k = rest_k;
                    chosen_tint = if as_shot.is_known() {
                        as_shot.tint
                    } else {
                        0.0
                    };
                    reasons.push(codec::plain(ToneCode::ColouredLightPreserved, 0.0));
                }
                Some((_, uv)) => {
                    // Read the landing point back through the one conversion, so the pair
                    // written into the recipe is the pair `describe` would report for the
                    // chromaticity that actually satisfied everybody.
                    let landing = illuminant::describe(
                        uv,
                        chosen.hypothesis.source,
                        1.0,
                        chosen.hypothesis.region,
                        attrs,
                    );
                    chosen_k = landing.cct_k;
                    chosen_tint = landing.tint;
                    reasons.push(codec::plain(ToneCode::ColouredLightPartial, -0.03));
                }
                None => {
                    // No amount of correction along this path satisfies everybody. Correct
                    // fully: the mood is lost and the people are right, which is the trade
                    // section 6.3 makes when it calls the skin locus a hard constraint.
                    reasons.push(codec::plain(ToneCode::SkinLocusConstrained, -0.08));
                }
            }
        }
    }

    // --- the tungsten floor -------------------------------------------------------------
    // A floor on the value written, which is an under-correction. See the module header.
    if chosen_k < target.tungsten_floor_k {
        chosen_k = target.tungsten_floor_k;
        reasons.push(codec::plain(ToneCode::TungstenReception, 0.0));
    }

    // --- the correction bound -----------------------------------------------------------
    if as_shot.is_known() {
        let low = as_shot.temperature_k - target.max_correction_k;
        let high = as_shot.temperature_k + target.max_correction_k;
        chosen_k = chosen_k.clamp(low, high);
    }
    let temperature_k = chosen_k.clamp(KELVIN_RANGE.0, KELVIN_RANGE.1);
    let tint = chosen_tint.clamp(TINT_RANGE.0, TINT_RANGE.1);

    // --- the evidence reason ------------------------------------------------------------
    match chosen.hypothesis.source {
        HypothesisSource::KnownNeutral => reasons.push(codec::plain_at(
            ToneCode::NeutralFound,
            0.0,
            chosen
                .hypothesis
                .region
                .unwrap_or(aura_core::CropRect::FULL),
        )),
        HypothesisSource::Skin => reasons.push(codec::plain(ToneCode::SkinAnchored, 0.0)),
        HypothesisSource::Flash => reasons.push(codec::plain(ToneCode::FlashDetected, 0.0)),
        HypothesisSource::GreyWorld => {
            reasons.push(codec::plain(ToneCode::GreyWorldFallback, -0.10));
        }
        HypothesisSource::CameraAsShot => {
            reasons.push(codec::plain(ToneCode::CameraAsShotUsed, -0.05));
        }
        HypothesisSource::WhitePatch | HypothesisSource::Learned => {}
    }
    if constraints.is_empty() {
        reasons.push(codec::plain(ToneCode::SkinLocusUnavailable, -0.10));
    } else if chosen.all_vetoed {
        reasons.push(codec::plain(ToneCode::SkinLocusConstrained, -0.15));
    }

    // --- the illuminant list ------------------------------------------------------------
    let mut illuminants = vec![primary];
    if mixed.mixed {
        for (share, uv, region) in &mixed.groups {
            // Skip the group the chosen light already describes: recording the same light
            // twice would make `dominant_on_subject` ambiguous and the panel repeat itself.
            if uv_distance(*uv, light_uv) < illuminant::MIXED_SEPARATION / 2.0 {
                continue;
            }
            illuminants.push(illuminant::describe(
                *uv,
                HypothesisSource::GreyWorld,
                *share,
                Some(*region),
                attrs,
            ));
        }
        let separation_k = (mixed
            .groups
            .last()
            .map_or(0.0, |(_, uv, _)| cct_from_uv(*uv))
            - mixed
                .groups
                .first()
                .map_or(0.0, |(_, uv, _)| cct_from_uv(*uv)))
        .abs();
        reasons.push(codec::reason_with(
            ToneCode::MixedLight,
            -0.12,
            &[separation_k],
            Some(
                mixed
                    .groups
                    .first()
                    .map_or(aura_core::CropRect::FULL, |(_, _, region)| *region),
            ),
        ));
    }

    // --- the alternatives ---------------------------------------------------------------
    let mut alternatives: Vec<ToneAlternative> = Vec::new();
    let mut ranked: Vec<(Hypothesis, Score)> = hypotheses
        .iter()
        .filter(|candidate| uv_distance(candidate.uv, light_uv) > 1e-6)
        .map(|candidate| (*candidate, wb::score(candidate, constraints, patches)))
        .collect();
    ranked.sort_by(|a, b| {
        a.1.cost
            .total_cmp(&b.1.cost)
            .then_with(|| source_rank(a.0.source).cmp(&source_rank(b.0.source)))
    });
    for (candidate, score) in ranked.into_iter().take(ToneAlternative::MAX) {
        let described = illuminant::describe(candidate.uv, candidate.source, 0.0, None, attrs);
        alternatives.push(ToneAlternative {
            // The exposure is not re-solved per alternative: section 5's runner-ups are
            // colour answers, and the exposure the caller already solved is what each of them
            // would be applied on top of. The caller fills this in.
            exposure_ev: 0.0,
            temperature_k: described.cct_k.clamp(KELVIN_RANGE.0, KELVIN_RANGE.1),
            tint: described.tint.clamp(TINT_RANGE.0, TINT_RANGE.1),
            cost: score.cost,
            why: if score.vetoed {
                ToneCode::SkinLocusConstrained
            } else {
                match candidate.source {
                    HypothesisSource::KnownNeutral => ToneCode::NeutralFound,
                    HypothesisSource::Skin => ToneCode::SkinAnchored,
                    HypothesisSource::Flash => ToneCode::FlashDetected,
                    HypothesisSource::CameraAsShot => ToneCode::CameraAsShotUsed,
                    HypothesisSource::GreyWorld
                    | HypothesisSource::WhitePatch
                    | HypothesisSource::Learned => ToneCode::GreyWorldFallback,
                }
            },
        });
    }

    // --- the confidence -----------------------------------------------------------------
    //
    // Four terms that add up to 1.0 at their best, then penalties. Additive rather than the
    // geometric fusion phases 09 and 12 use, and the difference is deliberate: those combine
    // things that all have to be true, and these are four independent pieces of *evidence*
    // where having three is genuinely better than having two.
    let separation = chosen.agreement.clamp(0.0, 1.0);
    let evidence = chosen.hypothesis.prior.clamp(0.0, 1.0);
    let skin_ok = 1.0 - chosen.score.skin.clamp(0.0, 1.0);
    let neutral_ok = 1.0 - chosen.score.neutral.clamp(0.0, 1.0);
    let mut confidence = 0.30 * separation + 0.25 * evidence + 0.30 * skin_ok + 0.15 * neutral_ok;
    if mixed.mixed {
        confidence -= 0.12;
    }
    if constraints.is_empty() {
        confidence -= 0.12;
    }
    if chosen.all_vetoed {
        confidence -= 0.25;
    }
    if !target.measured {
        confidence -= UNTARGETED_CONFIDENCE_PENALTY;
    }
    let confidence = confidence.clamp(0.0, 1.0);
    if confidence < aura_core::contract::tone::REVIEW_WB_BELOW {
        reasons.push(codec::plain(ToneCode::WbLowConfidence, -0.05));
    }

    WbSolution {
        temperature_k,
        tint,
        confidence,
        illuminants,
        coloured_light,
        skin_de00: wb::skin_de00_estimate(constraints, light_uv),
        alternatives,
        reasons,
    }
}
