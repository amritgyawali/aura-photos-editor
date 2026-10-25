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
        None,
    )?;
    let endpoint = review.fit(state, cancel, input.ungraded, input.grade)?
        .ok_or_else(|| super::invalid("No colour match kept this photo's smooth gradients, skin tones and highlights while bringing it closer to the reference. Try a different reference. No edit was saved."))?;
    *input.grade = endpoint.grade;
    Ok((
        review.before,
        endpoint.after,
        endpoint.factor,
        endpoint.renders,
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
        let checked = self.learned.is_some() || native_checks::non_regressing(self.before, after);
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
