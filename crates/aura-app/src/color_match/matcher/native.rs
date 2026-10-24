//! Reuse one native frame and baseline when checking a bounded sequence of palette fits.
use aura_core::{progress::CancelToken, AuraResult, PhotoId};
use aura_recipe::reference_grade::{self as grade_model, ReferenceGrade};
use aura_recipe::Recipe;
use aura_render::{Frame, FrameSource, RenderLevel};
use std::sync::Arc;

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
}

pub(super) struct Checked {
    pub after: transfer::Distance,
    pub reused: bool,
}

pub(super) struct Endpoint {
    pub(super) grade: ReferenceGrade,
    pub(super) after: transfer::Distance,
    pub(super) factor: f32,
    pub(super) renders: usize,
}

pub(super) struct FitInput<'a> {
    pub(super) photo: PhotoId,
    pub(super) baseline: &'a Recipe,
    pub(super) ungraded: &'a Recipe,
    pub(super) regions: &'a [appearance::Region],
    pub(super) reference: &'a transfer::GradeStats,
    pub(super) monochrome: bool,
    pub(super) grade: &'a mut ReferenceGrade,
}

pub(super) fn fit(
    state: &AppState,
    cancel: &CancelToken,
    input: FitInput<'_>,
) -> AuraResult<(transfer::Distance, transfer::Distance, f32, usize)> {
    let review = Review::new(
        state,
        cancel,
        input.photo,
        input.baseline,
        input.regions,
        input.reference,
        input.monochrome,
    )?;
    let endpoint = review.fit(state, cancel, input.ungraded, input.grade)?
        .ok_or_else(|| super::invalid("No reference palette fit preserved this photo's native skin tones, highlights and measured colour distance. Try Natural look or a different reference. No edit was saved."))?;
    *input.grade = endpoint.grade;
    Ok((
        review.before,
        endpoint.after,
        endpoint.factor,
        endpoint.renders,
    ))
}

impl Review {
    pub(super) fn fit(
        &self,
        state: &AppState,
        cancel: &CancelToken,
        ungraded: &Recipe,
        grade: &ReferenceGrade,
    ) -> AuraResult<Option<Endpoint>> {
        let mut best = self.fit_candidate(state, cancel, ungraded, grade)?;
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

    pub(super) fn new(
        state: &AppState,
        cancel: &CancelToken,
        photo: PhotoId,
        baseline: &Recipe,
        regions: &[appearance::Region],
        reference: &transfer::GradeStats,
        monochrome: bool,
    ) -> AuraResult<Self> {
        check_cancel(cancel)?;
        let frame = crate::photo_frames::CatalogFrames::new(state.clone())
            .frame(&photo, RenderLevel::Full)?;
        let image = rendered(state, &frame, baseline)?;
        check_cancel(cancel)?;
        let before_digest = native_checks::image_digest(&image);
        let context = native_checks::context_digest(reference, regions, monochrome)?;
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
        })
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
        if let Some(after) = state.color_match_checks().checked(key) {
            return Ok(Some(Checked {
                after,
                reused: true,
            }));
        }
        // Cheap rejection first: do not sort colour distributions for a failed safety check.
        if appearance::clipped(&image) > self.guard.clipped + 0.01
            || !skin_guard(
                &self.guard.regions,
                &appearance::readings(&image, &self.points),
                self.monochrome,
            )
        {
            return Ok(None);
        }
        let after = super::measured(&self.reference, &image);
        check_cancel(cancel)?;
        let checked = native_checks::non_regressing(self.before, after);
        if checked {
            state
                .color_match_checks()
                .remember(key, self.before, after, Arc::clone(&self.guard));
        }
        Ok(checked.then_some(Checked {
            after,
            reused: false,
        }))
    }
}
