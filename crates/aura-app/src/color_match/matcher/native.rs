//! Reuse one native frame and baseline when checking a bounded sequence of palette fits.
use aura_core::{progress::CancelToken, AuraResult, PhotoId};
use aura_raw::codec::Rgb8;
use aura_recipe::reference_grade::{self as grade_model, ReferenceGrade};
use aura_recipe::Recipe;
use aura_render::{Frame, FrameSource, RenderLevel};
use std::sync::Arc;

use super::super::gallery_fit::Allowance;
use super::super::native_checks::{self, Digest, Guard, Key};
use super::super::{appearance, check_cancel, rendered, skin_guard, transfer, AppState};

pub(super) struct Review {
    frame: Frame,
    pub(super) before: transfer::Distance,
    guard: Arc<Guard>,
    points: Vec<appearance::Region>,
    monochrome: bool,
    reference: transfer::GradeStats,
    before_digest: Digest,
    context: Digest,
    /// Present when the look was learned from the photographer's own edit of a photograph in the
    /// collection (ADR-0161): skin and clipping may move as far as that edit moved them, and the
    /// result need not move closer to the reference's colour distribution - a correct edit of a
    /// different scene does not have to.
    learned: Option<Allowance>,
    /// The original's colours at a fixed set of pixels, so the result can be compared with what
    /// the photograph looked like before - a "match" nobody can see is reported as one.
    original: Vec<(usize, [f32; 3])>,
    /// Whether a result must also come closer to the reference's colour distribution. A learned
    /// look need not: a correct edit of a different scene does not have to.
    regression: bool,
    /// The reference's own regions, which skin may move toward under an allowance that asks for
    /// the reference's skin (ADR-0183).
    skin_target: Vec<appearance::Region>,
}

/// Up to this many pixels compare a result with the original.
const VISIBLE_SAMPLES: usize = 4096;

fn oklab_at(image: &Rgb8, index: usize) -> Option<[f32; 3]> {
    let bytes = image.data.get(index * 3..index * 3 + 3)?;
    let linear: [f32; 3] = std::array::from_fn(|c| {
        aura_raw::colour::curve::srgb_byte_to_linear(bytes.get(c).copied().unwrap_or(0))
    });
    Some(aura_render::reference_grade::oklab_from_linear_srgb(linear).map(|v| v * 100.0))
}

fn sampled(image: &Rgb8) -> Vec<(usize, [f32; 3])> {
    let count = image.data.len() / 3;
    let stride = (count / VISIBLE_SAMPLES).max(1);
    (0..count)
        .step_by(stride)
        .filter_map(|i| oklab_at(image, i).map(|lab| (i, lab)))
        .collect()
}

/// A learned look's endpoint: the recipe the native checks accepted, and at what share of the
/// requested strength.
pub(super) struct LearnedEndpoint {
    pub(super) recipe: Recipe,
    pub(super) after: transfer::Distance,
    pub(super) factor: f32,
    pub(super) renders: usize,
}

pub(super) struct Checked {
    pub after: transfer::Distance,
    pub reused: bool,
    /// Mean `OKLab` difference times a hundred between the original and this result: below
    /// about 0.45 (about one CIE76 unit), the result looks the same as the photograph did.
    pub visible: f32,
}

pub(super) struct Endpoint {
    pub(super) grade: ReferenceGrade,
    pub(super) after: transfer::Distance,
    pub(super) factor: f32,
    pub(super) renders: usize,
    /// The share of the photograph the look newly clipped before its highlights were held back,
    /// when they were.
    pub(super) highlights_limited_from: Option<f32>,
}

pub(super) struct FitInput<'a> {
    pub(super) photo: PhotoId,
    pub(super) baseline: &'a Recipe,
    pub(super) ungraded: &'a Recipe,
    pub(super) regions: &'a [appearance::Region],
    pub(super) reference: &'a transfer::GradeStats,
    pub(super) monochrome: bool,
    pub(super) grade: &'a mut ReferenceGrade,
    /// Larger skin and clipping limits, for the reference's exact colours (ADR-0182).
    pub(super) allowance: Option<Allowance>,
    /// The reference's regions: the skin the exact colours move skin toward (ADR-0183).
    pub(super) skin_target: &'a [appearance::Region],
}

pub(super) fn fit(
    state: &AppState,
    cancel: &CancelToken,
    input: FitInput<'_>,
) -> AuraResult<(
    transfer::Distance,
    transfer::Distance,
    f32,
    usize,
    Option<f32>,
)> {
    let review = Review::new(
        state,
        cancel,
        input.photo,
        input.baseline,
        input.regions,
        input.reference,
        input.monochrome,
        input.allowance,
    )?
    .with_regression_check(true)
    .with_skin_target(input.skin_target);
    let endpoint = review.fit(state, cancel, input.ungraded, input.grade)?
        .ok_or_else(|| super::invalid("No colour match kept this photo's smooth gradients, skin tones and highlights while bringing it closer to the reference. Try a different reference. No edit was saved."))?;
    *input.grade = endpoint.grade;
    Ok((
        review.before,
        endpoint.after,
        endpoint.factor,
        endpoint.renders,
        endpoint.highlights_limited_from,
    ))
}

impl Review {
    /// How different a rendered result looks from the original, as mean `OKLab` times a hundred.
    pub(super) fn visible_change(&self, image: &Rgb8) -> f32 {
        let (mut total, mut count) = (0.0_f32, 0.0_f32);
        for (index, before) in &self.original {
            if let Some(after) = oklab_at(image, *index) {
                total += before
                    .iter()
                    .zip(after)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f32>()
                    .sqrt();
                count += 1.0;
            }
        }
        if count > 0.0 {
            total / count
        } else {
            0.0
        }
    }

    /// The strongest share of `strength` at which a learned look passes every native check: the
    /// photographer's edit at the strength asked for, or as near it as the photograph allows.
    pub(super) fn fit_learned(
        &self,
        state: &AppState,
        cancel: &CancelToken,
        strength: f32,
        recipe_at: impl Fn(f32) -> AuraResult<Recipe>,
    ) -> AuraResult<Option<LearnedEndpoint>> {
        for (attempt, factor) in [1.0, 0.75, 0.5, 0.25].into_iter().enumerate() {
            check_cancel(cancel)?;
            let recipe = recipe_at(strength * factor)?;
            // The baseline render plus every attempt so far.
            let renders = attempt + 2;
            if let Some(checked) = self.check(state, cancel, &recipe)? {
                return Ok(Some(LearnedEndpoint {
                    recipe,
                    after: checked.after,
                    factor,
                    renders,
                }));
            }
        }
        Ok(None)
    }

    pub(super) fn fit(
        &self,
        state: &AppState,
        cancel: &CancelToken,
        ungraded: &Recipe,
        grade: &ReferenceGrade,
    ) -> AuraResult<Option<Endpoint>> {
        let mut best = self.fit_candidate(state, cancel, ungraded, grade)?;
        // A look shortened for the native checks may have been refused only for clipping its
        // highlights. Hold those back instead and keep whichever endpoint comes closer to the
        // reference; only this case pays for the extra renders. Not for a palette transport:
        // with its highlights held, the whole transport passes at full strength and overshoots
        // the look (desktop run, ADR-0181).
        if !self.monochrome
            && grade.palette.is_none()
            && best.as_ref().is_none_or(|fit| fit.factor < 1.0)
        {
            let (held, from, spent) = self.hold_highlights(state, cancel, ungraded, grade)?;
            if from.is_some() {
                let alternative = self.fit_candidate(state, cancel, ungraded, &held)?;
                let renders = best.as_ref().map_or(7, |v| v.renders)
                    + spent
                    + alternative.as_ref().map_or(7, |v| v.renders)
                    - 1;
                if alternative
                    .as_ref()
                    .is_some_and(|a| best.as_ref().is_none_or(|b| a.after.total < b.after.total))
                {
                    best = alternative.map(|mut endpoint| {
                        endpoint.highlights_limited_from = from;
                        endpoint
                    });
                }
                if let Some(best) = best.as_mut() {
                    best.renders = renders;
                }
            }
        }
        // A strong chromatic transport can force a global backtrack to preserve a face.
        // Also check the underlying tone/cast fit, using the same native frame and guards.
        // Keep whichever actually passes with the smaller full-palette distance.
        if grade.palette.is_some() && best.as_ref().is_none_or(|fit| fit.factor < 0.5) {
            let mut underlying = grade.clone();
            underlying.palette = None;
            let alternative = self.fit_candidate(state, cancel, ungraded, &underlying)?;
            let renders = best.as_ref().map_or(7, |v| v.renders)
                + alternative.as_ref().map_or(7, |v| v.renders)
                - 1;
            if alternative
                .as_ref()
                .is_some_and(|a| best.as_ref().is_none_or(|b| a.after.total < b.after.total))
            {
                best = alternative;
            }
            if let Some(best) = best.as_mut() {
                best.renders = renders;
            }
        }
        Ok(best)
    }

    /// Hold back the highlights of a look the full render shows clipping, before any of the rest
    /// of the look is given up for them (ADR-0181). The fit estimates clipping by evaluating the
    /// look on a reduced frame's colours, but the look is one stage of the render and the stages
    /// after it decide the brightest values, so the estimate is calibrated against the render: a
    /// look that newly clips three times what the estimate says is limited to a third of the
    /// target. Returns the held look, the new clipping it began with when it had to be held, and
    /// the renders spent (at most four).
    fn hold_highlights(
        &self,
        state: &AppState,
        cancel: &CancelToken,
        ungraded: &Recipe,
        grade: &ReferenceGrade,
    ) -> AuraResult<(ReferenceGrade, Option<f32>, usize)> {
        check_cancel(cancel)?;
        let full = transfer::samples(
            &rendered(state, &self.frame, ungraded)?,
            transfer::TARGET_SAMPLES,
        );
        let (mut limited, mut from, mut renders) = (grade.clone(), None, 1);
        for _ in 0..3 {
            check_cancel(cancel)?;
            let mut recipe = ungraded.clone();
            grade_model::write(&mut recipe, &limited)?;
            let image = rendered(state, &self.frame, &super::canonical(&recipe)?)?;
            renders += 1;
            let measured = (appearance::clipped(&image) - self.guard.clipped).max(0.0);
            if measured <= transfer::MAX_NEW_CLIPPING {
                break;
            }
            from.get_or_insert(measured);
            let estimated = transfer::new_clipping(&limited, &full);
            let target = transfer::MAX_NEW_CLIPPING * (estimated / measured).clamp(0.0, 1.0);
            let (next, changed) = transfer::limit_highlights_to(limited.clone(), &full, target);
            if changed.is_none() || next == limited {
                break;
            }
            limited = next;
        }
        Ok((limited, from, renders))
    }

    fn fit_candidate(
        &self,
        state: &AppState,
        cancel: &CancelToken,
        ungraded: &Recipe,
        grade: &ReferenceGrade,
    ) -> AuraResult<Option<Endpoint>> {
        let identity = ReferenceGrade::identity();
        let (mut lower, mut upper, mut factor) = (0.0, 1.0, 1.0);
        let mut accepted: Option<Endpoint> = None;
        let mut attempts = 0;
        // Bracket a safe fit, then refine the bracket within a fixed six-render budget.
        // Every incumbent is actually checked; non-monotone safety/colour behaviour cannot
        // make an unchecked interpolated endpoint reach the saved recipe.
        for _ in 0..6 {
            check_cancel(cancel)?;
            attempts += 1;
            let mut candidate_grade = transfer::toward(&identity, grade, factor);
            // A monochrome reference requests removal of colour. Refine tone/cast safety
            // without bringing colour back into its fitted 100% endpoint.
            if self.monochrome {
                candidate_grade.chroma.clone_from(&grade.chroma);
                candidate_grade.cast_out.clone_from(&grade.cast_out);
            }
            let mut recipe = ungraded.clone();
            grade_model::write(&mut recipe, &candidate_grade)?;
            if let Some(checked) = self.check(state, cancel, &super::canonical(&recipe)?)? {
                let after = checked.after;
                if accepted
                    .as_ref()
                    .is_none_or(|best| after.total <= best.after.total)
                {
                    accepted = Some(Endpoint {
                        grade: candidate_grade,
                        after,
                        factor,
                        renders: 0,
                        highlights_limited_from: None,
                    });
                }
                lower = factor;
                if factor >= 1.0 {
                    break;
                }
            } else {
                upper = factor;
            }
            factor = if accepted.is_some() {
                0.5 * (lower + upper)
            } else {
                0.5 * factor
            };
        }
        if let Some(endpoint) = accepted.as_mut() {
            endpoint.renders = attempts + 1;
        }
        Ok(accepted)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        state: &AppState,
        cancel: &CancelToken,
        photo: PhotoId,
        baseline: &Recipe,
        regions: &[appearance::Region],
        reference: &transfer::GradeStats,
        monochrome: bool,
        learned: Option<Allowance>,
    ) -> AuraResult<Self> {
        check_cancel(cancel)?;
        let frame = crate::photo_frames::CatalogFrames::new(state.clone())
            .frame(&photo, RenderLevel::Full)?;
        let image = rendered(state, &frame, baseline)?;
        check_cancel(cancel)?;
        let before_digest = native_checks::image_digest(&image);
        let mut context = native_checks::context_digest(reference, regions, monochrome)?;
        if let Some(allowance) = learned {
            // A proof made under one set of limits must never certify another.
            let bytes = serde_json::to_vec(&allowance).map_err(|_| {
                super::invalid("The native colour-check context could not be encoded.")
            })?;
            let mut hasher = blake3::Hasher::new();
            hasher.update(&context);
            hasher.update(b"learned-look-allowance");
            hasher.update(&bytes);
            context = *hasher.finalize().as_bytes();
        }
        let (before, guard) = state
            .color_match_checks()
            .baseline(before_digest, context)
            .unwrap_or_else(|| {
                let mut readings = appearance::readings(&image, regions);
                // Only names and measured values are used by the guard. Keep sample coordinates
                // once on the review, rather than duplicating them in bounded cached proofs.
                for region in &mut readings {
                    region.points = Vec::new();
                }
                (
                    super::measured(reference, &image),
                    Arc::new(Guard {
                        regions: readings,
                        clipped: appearance::clipped(&image),
                        contours: super::super::contours::Contours::new(&image),
                    }),
                )
            });
        Ok(Self {
            before,
            guard,
            points: regions.to_vec(),
            frame,
            monochrome,
            reference: reference.clone(),
            before_digest,
            context,
            learned,
            original: sampled(&image),
            regression: learned.is_none(),
            skin_target: Vec::new(),
        })
    }

    /// The reference's regions, for an allowance that lets skin move toward the reference's skin.
    pub(super) fn with_skin_target(mut self, regions: &[appearance::Region]) -> Self {
        self.skin_target = regions
            .iter()
            .filter(|region| region.name.ends_with("skin"))
            .map(|region| appearance::Region {
                name: region.name.clone(),
                points: Vec::new(),
                appearance: region.appearance.clone(),
            })
            .collect();
        self
    }

    /// Whether `readings` put every person's skin closer to the reference's own skin than the
    /// photograph had it, in every measured respect: then a large move is the reference's skin, not
    /// damage. A reference without measured skin gives nothing to move toward.
    fn skin_toward_reference(&self, readings: &[appearance::Region]) -> bool {
        appearance::skin_toward(&self.skin_target, &self.guard.regions, readings)
    }

    /// Require, or not, that a result comes closer to the reference's colour distribution.
    pub(super) fn with_regression_check(mut self, required: bool) -> Self {
        self.regression = required;
        self
    }

    pub(super) fn check(
        &self,
        state: &AppState,
        cancel: &CancelToken,
        recipe: &Recipe,
    ) -> AuraResult<Option<Checked>> {
        check_cancel(cancel)?;
        let image = rendered(state, &self.frame, recipe)?;
        check_cancel(cancel)?;
        let key = Key {
            before: self.before_digest,
            after: native_checks::image_digest(&image),
            context: self.context,
        };
        let visible = self.visible_change(&image);
        if let Some(after) = state.color_match_checks().checked(key) {
            return Ok(Some(Checked {
                after,
                reused: true,
                visible,
            }));
        }
        // Cheap rejection first: do not sort colour distributions for a failed safety check.
        let clipping = self.learned.map_or(0.01, |allowance| allowance.clipping);
        let readings = appearance::readings(&image, &self.points);
        let skin_ok = match self.learned {
            Some(allowance) if !self.monochrome => {
                appearance::skin_change(&self.guard.regions, &readings) <= allowance.skin
                    || (allowance.toward_reference && self.skin_toward_reference(&readings))
            }
            _ => skin_guard(&self.guard.regions, &readings, self.monochrome),
        };
        if !self.guard.contours.accepts(&image)
            || appearance::clipped(&image) > self.guard.clipped + clipping
            || !skin_ok
        {
            return Ok(None);
        }
        let after = super::measured(&self.reference, &image);
        check_cancel(cancel)?;
        let checked = !self.regression || native_checks::non_regressing(self.before, after);
        if checked {
            state
                .color_match_checks()
                .remember(key, self.before, after, Arc::clone(&self.guard));
        }
        Ok(checked.then_some(Checked {
            after,
            reused: false,
            visible,
        }))
    }
}
