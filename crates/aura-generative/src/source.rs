//! The choke point. Real pixels first, then this photograph's own texture, then nothing.
//!
//! ADR-0049 section 4 freezes the order and puts it in the type: [`CleanupMethod::preference`] is
//! borrow 0, fill 1, inpaint 2, and [`select`] tries them in exactly that sequence with no
//! configuration that reorders it. A studio that could reorder them would eventually do it,
//! because diffusion is faster than a homography search across a moment - which is the reason that
//! makes the reordering worst.
//!
//! ## This is the only function in the crate that may reach a removal
//!
//! [`crate::borrow`], [`crate::fill`] and [`crate::inpaint`] are called from here and from nowhere
//! else. `crates/aura-generative/tests/one_choke_point.rs` is a grep as a test that fails the
//! build otherwise - the fifth in the repository after `colour_discipline.rs`,
//! `no_recipe_writes.rs`, `no_template_writes.rs` and `no_render_calls.rs`.
//!
//! Section 12's third failure mode is "safety bypass through a new code path", and its mitigation
//! is "single choke-point API ... and a lint forbidding direct calls to fill/inpaint". The lint is
//! that test. The choke point is this function, and it is enforced twice over: [`select`] takes a
//! [`SafeCandidate`], which has no public constructor and can only be obtained from
//! [`crate::safety::check`] returning `Allowed`. A caller who wanted to fill an unchecked region
//! could not construct the argument.
//!
//! ## Every method that was tried is recorded, including the ones that failed
//!
//! A selection that fell through to a fill carries [`CleanupCode::NoAlignedSibling`] beside its
//! [`CleanupCode::TextureUniform`], so the stored reasons say *the better method was tried and
//! why it did not work*. Without that, a delivery report can say a region was filled and cannot
//! say whether a borrow was ever possible - and "were there real pixels available for this?" is
//! the question a photographer asks when they are unhappy with a patch.
//!
//! [`CleanupMethod::preference`]: aura_core::contract::cleanup::CleanupMethod::preference

use aura_core::contract::cleanup::{
    CleanupCode, CleanupMethod, CleanupReason, DistractionClass, ImageId,
};

use crate::borrow::{self, Borrowed};
use crate::fill::{self, Filled};
use crate::inpaint;
use crate::pixels::Image;
use crate::safety::SafeCandidate;

/// How much weight the source method carries in a proposal's confidence.
///
/// A borrow at a perfect alignment is worth more than a fill of perfect texture, because one is a
/// record of the room and the other is a rearrangement of it. The numbers are the *ceiling* each
/// method's own quality is scaled into, not a bonus added to it.
pub const BORROW_CEILING: f32 = 0.95;

/// The ceiling for a classical fill. See [`BORROW_CEILING`].
///
/// Below [`ZERO_TOUCH_CONFIDENCE`], deliberately and permanently: a filled region is never
/// certain enough to apply without somebody looking, whatever the texture measured. Section 6.4
/// permits tier-1 auto-apply at 0.97 and this ceiling means the tier-1 method that can reach it is
/// the borrow. Removing this line would be the single smallest edit in the crate that changes what
/// the product does unattended.
///
/// [`ZERO_TOUCH_CONFIDENCE`]: aura_core::contract::cleanup::ZERO_TOUCH_CONFIDENCE
pub const FILL_CEILING: f32 = 0.90;

/// One frame of the same moment, and its pixels.
#[derive(Debug, Clone, Copy)]
pub struct Sibling<'a> {
    /// Which photograph.
    pub id: ImageId,
    /// Its pixels, linear, at the same level as the target.
    pub image: &'a Image,
}

/// Everything [`select`] may draw on.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    /// The photograph being cleaned.
    pub target: &'a Image,
    /// Frames of the same moment, in the order a caller wants them tried.
    ///
    /// Phase 08's `MomentService` is the only thing that decides what "the same moment" means; a
    /// caller assembling this list from timestamps of its own would be the second answer to "what
    /// was shot once" that phase 08's rule forbids.
    pub siblings: &'a [Sibling<'a>],
    /// Whether the studio has opted the diffusion tier in. Off at installation.
    pub studio_opted_in: bool,
}

/// A chosen source, with the pixels it produces.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    /// How the pixels were replaced, for the disclosure.
    pub method: CleanupMethod,
    /// The frame with the region replaced.
    pub result: Image,
    /// How sure this removal is, `0..1`, before phase 13's calibration.
    pub confidence: f32,
    /// Why, worst first, including the methods that were tried and failed.
    pub reasons: Vec<CleanupReason>,
}

/// Choose a source for one safe candidate and produce the pixels.
///
/// # Errors
///
/// The reasons every method failed, in the order they were tried. An empty vector is impossible:
/// three methods are always attempted and each one either succeeds or names a code.
pub fn select(
    sources: &Sources<'_>,
    safe: &SafeCandidate,
) -> Result<Selection, Vec<CleanupReason>> {
    let candidate = safe.candidate();
    let region = candidate.region;
    let mut tried: Vec<CleanupReason> = Vec::new();

    // 1. Real pixels, from a sibling frame of the same moment.
    let mut borrow_failure: Option<CleanupCode> = None;
    for sibling in sources.siblings {
        match borrow::borrow(sources.target, sibling.image, sibling.id, &region) {
            Ok(found) => {
                return Ok(from_borrow(&found, candidate.removability, tried, region));
            }
            Err(code) => borrow_failure = Some(code),
        }
    }
    let borrow_code = if sources.siblings.is_empty() {
        CleanupCode::NoAlignedSibling
    } else {
        borrow_failure.unwrap_or(CleanupCode::NoAlignedSibling)
    };
    tried.push(CleanupReason::at(borrow_code, 0.30, region));

    // 2. This photograph's own texture.
    match fill::fill(sources.target, &region) {
        Ok(filled) => return Ok(from_fill(&filled, candidate.removability, tried, region)),
        Err(code) => tried.push(CleanupReason::at(code, 0.45, region)),
    }

    // 3. A model that would have made the pixels up. Always refuses in this build; see
    //    `crate::inpaint`.
    let request = inpaint::Request {
        image: sources.target,
        region: &region,
        studio_opted_in: sources.studio_opted_in,
    };
    match inpaint::solve(&request) {
        Ok(_) => {
            // Unreachable in this build and deliberately written out rather than left as an
            // `unreachable!`: the day a pack ships, this arm is the one that has to be filled in,
            // and a panic macro here would be a panic in a background pass.
            tried.push(CleanupReason::at(
                CleanupCode::InpaintUnavailable,
                0.60,
                region,
            ));
        }
        Err(code) => tried.push(CleanupReason::at(code, 0.60, region)),
    }

    Err(tried)
}

/// Build the selection a successful borrow produces.
fn from_borrow(
    found: &Borrowed,
    removability: f32,
    mut reasons: Vec<CleanupReason>,
    region: aura_core::contract::cleanup::Box2,
) -> Selection {
    let alignment = found.alignment.confidence();
    // Geometric, so a confident detector cannot rescue a sloppy alignment and a perfect alignment
    // cannot rescue a candidate nobody was sure about. Phases 09, 11, 12 and 18, and the same
    // reason every time: no signal may stand in for another.
    let confidence = (alignment * removability.clamp(0.0, 1.0)).sqrt() * BORROW_CEILING;
    reasons.insert(
        0,
        CleanupReason::at(CleanupCode::SiblingAvailable, 1.0, region),
    );
    Selection {
        method: CleanupMethod::BorrowFrom(found.source),
        result: found.result.clone(),
        confidence: confidence.clamp(0.0, 1.0),
        reasons,
    }
}

/// Build the selection a successful fill produces.
fn from_fill(
    filled: &Filled,
    removability: f32,
    mut reasons: Vec<CleanupReason>,
    region: aura_core::contract::cleanup::Box2,
) -> Selection {
    // How safe the texture was to copy from, expressed the way the fill measured it: a low
    // coherence is good, and a uniform ring is better still.
    let texture = ((1.0 - filled.texture.coherence).clamp(0.0, 1.0) * 0.7
        + filled.texture.uniformity.clamp(0.0, 1.0) * 0.3)
        .clamp(0.0, 1.0);
    let confidence = (texture * removability.clamp(0.0, 1.0)).sqrt() * FILL_CEILING;
    reasons.insert(
        0,
        CleanupReason::at(CleanupCode::TextureUniform, 1.0, region),
    );
    Selection {
        method: CleanupMethod::ClassicalFill,
        result: filled.result.clone(),
        confidence: confidence.clamp(0.0, 1.0),
        reasons,
    }
}

/// True when a class may ever reach [`select`] at all.
///
/// A second reading of `DistractionClass::story_safe`, at the point pixels would move, rather than
/// a re-derivation of it. [`crate::safety::check`] has already refused everything this refuses; the
/// value is that a future caller who reached this module by another route still cannot remove a
/// person, and the assertion is one line rather than a comment asking them not to.
#[must_use]
pub fn class_may_be_sourced(class: DistractionClass) -> bool {
    class.story_safe()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::denylist::Coverage;
    use crate::pixels::Rect;
    use crate::policy::ScenePolicy;
    use crate::safety::{self, Candidate, Outcome};
    use aura_core::contract::cleanup::Box2;
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

    fn bars(w: usize, h: usize) -> Image {
        let mut image = Image::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = if (y / 6) % 2 == 0 { 0.65 } else { 0.18 };
                image.put(x, y, [v, v, v]);
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

    fn block_region() -> Box2 {
        Box2 {
            x: 60.0 / 160.0,
            y: 60.0 / 160.0,
            w: 16.0 / 160.0,
            h: 16.0 / 160.0,
        }
    }

    fn safe_bin() -> SafeCandidate {
        let candidate = Candidate {
            region: block_region(),
            class: DistractionClass::Bin,
            salience: 0.8,
            removability: 0.9,
            crosses_structure: false,
            touches_identity: false,
        };
        let policy = ScenePolicy {
            area_cap: 0.04,
            denylist_overlap_max: 0.01,
            zero_touch_confidence: 0.97,
            enabled: true,
            reason: "a test".into(),
        };
        match safety::check(&candidate, &policy, &Coverage::known_empty()) {
            Outcome::Allowed(safe) => *safe,
            Outcome::Blocked { check, .. } => {
                panic!("the fixture must be safe, blocked by {check:?}")
            }
        }
    }

    fn some_id() -> ImageId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000024").unwrap_or_default()
    }

    #[test]
    fn a_sibling_is_preferred_over_a_fill_whenever_one_is_available() {
        // Section 10.1's "sibling borrowing is preferred whenever available", measured.
        let clean = textured(160, 160, 0.0);
        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);

        let siblings = [Sibling {
            id: some_id(),
            image: &clean,
        }];
        let selection = select(
            &Sources {
                target: &target,
                siblings: &siblings,
                studio_opted_in: false,
            },
            &safe_bin(),
        )
        .expect("a clean sibling is available");

        assert_eq!(selection.method.preference(), 0);
        assert!(selection.method.is_real_pixels());
        assert!(selection
            .reasons
            .iter()
            .any(|r| r.code == CleanupCode::SiblingAvailable));
    }

    #[test]
    fn with_no_sibling_the_fill_runs_and_the_borrow_refusal_is_recorded() {
        let mut target = textured(160, 160, 0.0);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let selection = select(
            &Sources {
                target: &target,
                siblings: &[],
                studio_opted_in: false,
            },
            &safe_bin(),
        )
        .expect("texture is fillable");

        assert_eq!(selection.method, CleanupMethod::ClassicalFill);
        // The better method was tried, and the row says why it did not work.
        assert!(selection
            .reasons
            .iter()
            .any(|r| r.code == CleanupCode::NoAlignedSibling));
    }

    #[test]
    fn with_no_sibling_and_structured_texture_everything_refuses_and_names_all_three() {
        let mut target = bars(160, 160);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let failure = select(
            &Sources {
                target: &target,
                siblings: &[],
                studio_opted_in: true,
            },
            &safe_bin(),
        )
        .expect_err("nothing can source this");

        let codes: Vec<CleanupCode> = failure.iter().map(|r| r.code).collect();
        assert!(codes.contains(&CleanupCode::NoAlignedSibling));
        assert!(codes.contains(&CleanupCode::TextureStructured));
        assert!(codes.contains(&CleanupCode::InpaintUnavailable));
    }

    #[test]
    fn a_studio_opt_in_does_not_produce_an_inpaint() {
        // The one switch a studio can set must not be able to conjure a model pack, and the
        // failure must be named rather than silently falling back on the fill it already tried.
        let mut target = bars(160, 160);
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let failure = select(
            &Sources {
                target: &target,
                siblings: &[],
                studio_opted_in: true,
            },
            &safe_bin(),
        )
        .expect_err("nothing can source this");
        assert!(failure
            .iter()
            .any(|r| r.code == CleanupCode::InpaintUnavailable));
    }

    #[test]
    fn a_fill_can_never_reach_the_unattended_threshold() {
        // `FILL_CEILING` is below `ZERO_TOUCH_CONFIDENCE`, so a perfect fill of perfect texture on
        // a perfectly confident candidate still requires somebody to look.
        let mut target = Image {
            w: 160,
            h: 160,
            rgb: vec![0.42; 160 * 160 * 3],
        };
        paint(&mut target, BLOCK, [0.95, 0.05, 0.05]);
        let selection = select(
            &Sources {
                target: &target,
                siblings: &[],
                studio_opted_in: false,
            },
            &safe_bin(),
        )
        .expect("a flat wall is fillable");
        assert!(
            selection.confidence < aura_core::contract::cleanup::ZERO_TOUCH_CONFIDENCE,
            "a fill reached {}",
            selection.confidence
        );
    }

    #[test]
    fn a_person_can_never_be_sourced() {
        assert!(!class_may_be_sourced(DistractionClass::BackgroundPerson));
        assert!(!class_may_be_sourced(DistractionClass::Unclassified));
        assert!(class_may_be_sourced(DistractionClass::Bin));
    }
}
