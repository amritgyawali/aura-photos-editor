//! `Stage::Cleanup`: putting a stored patch back where an object was. PHASE-24.
//!
//! The shortest operator in the pipeline, and the reason it is short is worth stating before the
//! code: **every decision this stage embodies was made somewhere else.**
//!
//! `aura-generative` decided the rectangle, proved it safe against phase 18's regions, chose
//! between a sibling borrow and a classical fill, produced the pixels and ran the artefact
//! self-check over the result. A photographer then accepted it, and migration 24 wrote a
//! disclosure. What is left for a renderer to do is copy the approved samples into the frame.
//!
//! ## The three things this stage must not do
//!
//! **It must not re-derive the patch.** A `cleanup[]` operation in a recipe is a *disclosure* that
//! pixels were replaced; the pixels themselves are stored beside the proposal. Running the fill
//! again at render time would put different samples into the delivered file from the ones the
//! self-check passed, and the disclosure would then describe a removal that never happened. A
//! render that cannot find the patch leaves the object in the photograph and reports
//! [`SkipReason::CleanupPatchAbsent`].
//!
//! **It must not feather.** The patch already covers the whole object at full weight, and where a
//! borrow needed a transition it baked one into the band of real background *outside* the object.
//! Feathering here would blend the outermost samples of the replacement back toward the thing
//! being removed - a rim of the exit sign, left by the code that exists to hide the seam. Both of
//! phase 24's removal modules shipped that defect once and
//! `aura_generative::pixels::feather_out` is where it is written down.
//!
//! **It must not resample.** A patch whose dimensions do not match its rectangle at this render
//! level is refused rather than scaled, because a resampled patch is a different set of samples
//! from the one the self-check saw and the one the photographer approved.
//!
//! ## Why the patch is handed in rather than read
//!
//! `aura-render` depends on no catalog and reaches no store. [`Patch`] is filled by `aura-app`
//! from `cleanup_proposal`, which is the same shape phase 18's resolved planes and phase 19's
//! `MaskField` take, and for the same reason: a renderer that could read a database is a renderer
//! whose output depends on something that is not one of phase 14's four values.
//!
//! [`SkipReason::CleanupPatchAbsent`]: crate::contract::render::SkipReason::CleanupPatchAbsent

use aura_recipe::CleanupOp;

/// The replacement pixels for one removal, at one render level.
///
/// Linear, interleaved, `w * h * 3` samples - the same layout the working buffer carries between
/// `Stage::CameraMatrix` and `Stage::OutputTransform`.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// Which proposal these pixels belong to. Matched against `CleanupOp::proposal`.
    pub proposal: String,
    /// Width in pixels at this render level.
    pub w: usize,
    /// Height in pixels at this render level.
    pub h: usize,
    /// `w * h * 3` linear samples.
    pub rgb: Vec<f32>,
}

impl Patch {
    /// True when the buffer is the length the dimensions claim.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.w > 0 && self.h > 0 && self.rgb.len() == self.w * self.h * 3
    }
}

/// What one call to [`apply`] did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Applied {
    /// How many operations were pasted.
    pub pasted: usize,
    /// The proposals whose patch was missing, degenerate or the wrong size, in recipe order.
    ///
    /// **Named rather than counted**, because the panel and the delivery report both have to be
    /// able to say *which* removal did not reach the file. A count would tell a photographer that
    /// something in this photograph is not what they approved without telling them what.
    pub skipped: Vec<String>,
}

impl Applied {
    /// True when every operation in the recipe reached the frame.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.skipped.is_empty()
    }
}

/// The pixel rectangle one normalised region resolves to at a given size.
///
/// Rounds outward, exactly as `aura_generative::pixels::resolve` does, so the two agree about
/// which samples a removal covers. Two answers to that question is a one-pixel rim of the object
/// left behind at one render level and not at another, which is the worst possible version of this
/// bug because it only appears in the export.
#[must_use]
pub fn resolve(region: &[f32; 4], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let (rx, ry, rw, rh) = (
        region.first().copied().unwrap_or(0.0),
        region.get(1).copied().unwrap_or(0.0),
        region.get(2).copied().unwrap_or(0.0),
        region.get(3).copied().unwrap_or(0.0),
    );
    if w == 0 || h == 0 || rw <= 0.0 || rh <= 0.0 {
        return None;
    }
    let left = (rx * w as f32).floor().max(0.0) as usize;
    let top = (ry * h as f32).floor().max(0.0) as usize;
    let right = (((rx + rw) * w as f32).ceil() as usize).min(w);
    let bottom = (((ry + rh) * h as f32).ceil() as usize).min(h);
    if right <= left || bottom <= top {
        return None;
    }
    Some((left, top, right - left, bottom - top))
}

/// Paste every cleanup operation's stored patch into the working buffer.
///
/// `rgb` is the frame, linear and interleaved. `ops` is the recipe's `cleanup[]` in order.
/// `patches` is what the caller resolved for this render level, in any order.
///
/// Operations are applied in recipe order, which is the order they were accepted in. Two removals
/// cannot overlap - the safety engine caps a region at 4 % of the frame and the proposal cap is
/// three - but the order is fixed anyway, because a rendered file that depended on a map's
/// iteration order would not be byte-identical across two runs. Invariant 4.
pub fn apply(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    ops: &[CleanupOp],
    patches: &[Patch],
) -> Applied {
    let mut out = Applied::default();
    if rgb.len() != w * h * 3 {
        // A frame that is not the size it says it is. Every operation is skipped and named, rather
        // than a partial paste that would leave the photograph in a state nobody can describe.
        out.skipped = ops.iter().map(|op| op.proposal.clone()).collect();
        return out;
    }

    for op in ops {
        // A malformed operation is a disclosure that contradicts itself - a borrow with no source,
        // a region off the frame. It is skipped and named rather than best-guessed, because the
        // one thing worse than not applying a removal is applying one that cannot be accounted for.
        if !op.is_well_formed() {
            out.skipped.push(op.proposal.clone());
            continue;
        }
        let Some((x, y, pw, ph)) = resolve(&op.region, w, h) else {
            out.skipped.push(op.proposal.clone());
            continue;
        };
        let Some(patch) = patches
            .iter()
            .find(|patch| patch.proposal == op.proposal)
            .filter(|patch| patch.is_well_formed() && patch.w == pw && patch.h == ph)
        else {
            out.skipped.push(op.proposal.clone());
            continue;
        };

        for row in 0..ph {
            for column in 0..pw {
                let target = ((y + row) * w + (x + column)) * 3;
                let source = (row * pw + column) * 3;
                for channel in 0..3 {
                    let value = patch.rgb.get(source + channel).copied().unwrap_or(0.0);
                    if let Some(slot) = rgb.get_mut(target + channel) {
                        *slot = value;
                    }
                }
            }
        }
        out.pasted += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize, value: f32) -> Vec<f32> {
        vec![value; w * h * 3]
    }

    fn op(proposal: &str, region: [f32; 4], method: &str) -> CleanupOp {
        CleanupOp {
            proposal: proposal.to_string(),
            method: method.to_string(),
            borrowed_from: if method == "borrow" {
                Some("pht_00000000-0000-4000-8000-000000000001".to_string())
            } else {
                None
            },
            model: None,
            region,
            class: "bin".to_string(),
            artefact_score: 0.02,
            accepted_by_user: true,
        }
    }

    fn patch(proposal: &str, w: usize, h: usize, value: f32) -> Patch {
        Patch {
            proposal: proposal.to_string(),
            w,
            h,
            rgb: vec![value; w * h * 3],
        }
    }

    #[test]
    fn a_patch_lands_exactly_on_its_region_and_nowhere_else() {
        let (w, h) = (40, 40);
        let mut rgb = frame(w, h, 0.10);
        let region = [0.25, 0.25, 0.25, 0.25];
        let (x, y, pw, ph) = resolve(&region, w, h).expect("resolves");
        let done = apply(
            &mut rgb,
            w,
            h,
            &[op("prp_a", region, "fill")],
            &[patch("prp_a", pw, ph, 0.90)],
        );
        assert_eq!(done.pasted, 1);
        assert!(done.is_complete());

        for row in 0..h {
            for column in 0..w {
                let inside = column >= x && column < x + pw && row >= y && row < y + ph;
                let sample = rgb.get((row * w + column) * 3).copied().unwrap_or(0.0);
                let wanted = if inside { 0.90 } else { 0.10 };
                assert!(
                    (sample - wanted).abs() < 1e-6,
                    "at {column},{row}: {sample} wanted {wanted}"
                );
            }
        }
    }

    #[test]
    fn a_missing_patch_leaves_the_object_and_names_the_proposal() {
        // The rule the whole stage rests on: a render that cannot find the approved pixels leaves
        // the photograph alone rather than re-deriving them.
        let (w, h) = (40, 40);
        let mut rgb = frame(w, h, 0.10);
        let before = rgb.clone();
        let done = apply(&mut rgb, w, h, &[op("prp_a", [0.25, 0.25, 0.25, 0.25], "fill")], &[]);
        assert_eq!(done.pasted, 0);
        assert_eq!(done.skipped, vec!["prp_a".to_string()]);
        assert_eq!(rgb, before, "the frame must be untouched");
    }

    #[test]
    fn a_patch_of_the_wrong_size_is_refused_rather_than_scaled() {
        let (w, h) = (40, 40);
        let mut rgb = frame(w, h, 0.10);
        let before = rgb.clone();
        let done = apply(
            &mut rgb,
            w,
            h,
            &[op("prp_a", [0.25, 0.25, 0.25, 0.25], "fill")],
            &[patch("prp_a", 3, 3, 0.90)],
        );
        assert_eq!(done.skipped, vec!["prp_a".to_string()]);
        assert_eq!(rgb, before);
    }

    #[test]
    fn a_borrow_with_no_source_is_a_contradiction_and_is_skipped() {
        let (w, h) = (40, 40);
        let mut rgb = frame(w, h, 0.10);
        let mut broken = op("prp_a", [0.25, 0.25, 0.25, 0.25], "borrow");
        broken.borrowed_from = None;
        let (_, _, pw, ph) = resolve(&broken.region, w, h).expect("resolves");
        let done = apply(&mut rgb, w, h, &[broken], &[patch("prp_a", pw, ph, 0.9)]);
        assert_eq!(done.pasted, 0);
        assert_eq!(done.skipped, vec!["prp_a".to_string()]);
    }

    #[test]
    fn two_removals_are_applied_in_recipe_order() {
        // Invariant 4: two runs of the same recipe produce the same pixels, so the order cannot
        // come from a map.
        let (w, h) = (60, 60);
        let mut rgb = frame(w, h, 0.10);
        let first = [0.10, 0.10, 0.20, 0.20];
        let second = [0.60, 0.60, 0.20, 0.20];
        let (_, _, w1, h1) = resolve(&first, w, h).expect("resolves");
        let (_, _, w2, h2) = resolve(&second, w, h).expect("resolves");
        let done = apply(
            &mut rgb,
            w,
            h,
            &[op("prp_a", first, "fill"), op("prp_b", second, "fill")],
            &[patch("prp_b", w2, h2, 0.7), patch("prp_a", w1, h1, 0.4)],
        );
        assert_eq!(done.pasted, 2);
        assert!((rgb.get(((6 * w) + 6) * 3).copied().unwrap_or(0.0) - 0.4).abs() < 1e-6);
        assert!((rgb.get(((40 * w) + 40) * 3).copied().unwrap_or(0.0) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn a_region_resolves_outward_so_no_rim_of_the_object_is_left() {
        // The same rounding `aura_generative::pixels::resolve` uses. Two answers to which samples
        // a removal covers is a one-pixel rim that only appears at one render level.
        let resolved = resolve(&[0.101, 0.101, 0.101, 0.101], 100, 100).expect("resolves");
        assert_eq!(resolved.0, 10);
        assert_eq!(resolved.1, 10);
        assert!(resolved.0 + resolved.2 >= 21);
    }

    #[test]
    fn a_frame_that_is_not_the_size_it_claims_skips_everything() {
        let mut rgb = vec![0.1f32; 10];
        let done = apply(
            &mut rgb,
            40,
            40,
            &[op("prp_a", [0.25, 0.25, 0.25, 0.25], "fill")],
            &[],
        );
        assert_eq!(done.pasted, 0);
        assert_eq!(done.skipped.len(), 1);
    }
}
