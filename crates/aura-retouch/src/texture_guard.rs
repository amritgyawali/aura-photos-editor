//! The guarantee, measured rather than asserted.
//!
//! PHASE-20 section 6.3, and the reason this phase can make a defensible quality claim at all:
//!
//! > Decompose skin regions into frequency bands; measure high-band energy before and after
//! > retouching. Hard floor: post-retouch high-band energy >= 0.90 of the original. If violated,
//! > re-solve with lower strength and log it. This turns "we don't produce plastic skin" from a
//! > claim into a tested invariant with a number in CI.
//!
//! ## It measures the renderer, not a model of it
//!
//! [`enforce`] applies the plan through `aura_render::retouch` - the same code the delivered
//! JPEG goes through - and measures the result. That is phase 16 rule inherited: the skin guard
//! there grades this frame own skin through the real renderer, because a guarantee about a
//! pixel that is enforced on a parameter is not a guarantee. Between a parameter and a pixel
//! here sit a patch synthesis whose frequency content depends on where the donor came from, a
//! blend that is only as good as its alignment, and an under-eye correction that is not linear
//! in its own cap.
//!
//! ## It measures over skin, not over a rectangle
//!
//! A box around a face contains hair, and hair carries more high-band energy than anything else
//! in a photograph. A ratio measured over a box would be dominated by the samples the retouch
//! never touched and would pass every time - which is the failure mode of a guard that looks
//! rigorous and tests nothing.
//!
//! ## What happens when it fails
//!
//! The strength is reduced by [`aura_core::contract::retouch::TEXTURE_RESOLVE_STEP`] and the
//! whole thing is measured again, up to [`aura_core::contract::retouch::TEXTURE_MAX_RESOLVES`]
//! times. If it still cannot reach the floor the retouch is **withdrawn entirely** and the frame
//! ships unretouched. A frame nobody could retouch safely is a much smaller failure than a frame
//! that ships plastic, and a floor that can be exceeded once is not a floor.

use aura_core::contract::retouch::{
    RetouchOp, TextureReport, TEXTURE_MAX_RESOLVES, TEXTURE_RESOLVE_STEP,
};
use aura_render::bands;
use aura_render::retouch::{self, RetouchContext};

/// One frame, as the guard needs it.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Interleaved linear RGB, `width * height * 3`.
    pub rgb: Vec<f32>,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
}

/// What the guard decided, and the pixels it decided it on.
#[derive(Debug, Clone)]
pub struct Guarded {
    /// The operations that survived, at the strengths they survived at.
    ///
    /// Empty when the retouch was withdrawn, which is the state
    /// [`TextureReport::withdrawn`] describes and which
    /// [`aura_core::contract::retouch::RetouchPlan::broken_guarantee`] insists on.
    pub ops: Vec<RetouchOp>,
    /// The measurement.
    pub report: TextureReport,
    /// The retouched pixels, for a caller that wants to show or export them.
    pub rendered: Vec<f32>,
}

/// Measure a plan, re-solve it if it costs too much texture, and withdraw it if it still does.
///
/// The returned `ops` are the ones to store: at most the ones passed in, at strengths at most
/// the ones passed in, and possibly none at all.
#[must_use]
pub fn enforce(frame: &Frame, ops: &[RetouchOp], context: &RetouchContext, floor: f32) -> Guarded {
    let before_bands = bands::separate(
        &retouch::luma_plane(&frame.rgb, frame.width, frame.height),
        frame.width,
        frame.height,
    );
    let (before, counted) = before_bands.high_energy_masked(&context.skin);

    if ops.is_empty() {
        return Guarded {
            ops: Vec::new(),
            report: TextureReport {
                measured_on: counted,
                ..TextureReport::UNTOUCHED
            },
            rendered: frame.rgb.clone(),
        };
    }

    let mut attempt = ops.to_vec();
    let mut resolves = 0u8;

    loop {
        let mut pixels = frame.rgb.clone();
        retouch::apply(&mut pixels, frame.width, frame.height, &attempt, context);

        let after_bands = bands::separate(
            &retouch::luma_plane(&pixels, frame.width, frame.height),
            frame.width,
            frame.height,
        );
        let (after, _) = after_bands.high_energy_masked(&context.skin);

        // Zero energy before means skin with no measurable texture at all - a face at eleven
        // pixels, or a frame so smooth there is nothing to lose. The ratio is reported as one
        // and `measured_on` is what tells a reader not to believe it.
        let ratio = if before <= 1e-6 { 1.0 } else { after / before };

        if ratio + 1e-4 >= floor || resolves >= TEXTURE_MAX_RESOLVES {
            let passed = ratio + 1e-4 >= floor;
            if passed {
                return Guarded {
                    ops: attempt,
                    report: TextureReport {
                        band_ratio: ratio,
                        floor,
                        passed: true,
                        measured_on: counted,
                        resolves,
                        withdrawn: false,
                    },
                    rendered: pixels,
                };
            }
            // Out of re-solves. Withdraw everything: a partly applied plan that failed its own
            // floor is exactly the photograph this phase exists to not ship.
            return Guarded {
                ops: Vec::new(),
                report: TextureReport {
                    band_ratio: ratio,
                    floor,
                    passed: false,
                    measured_on: counted,
                    resolves,
                    withdrawn: true,
                },
                rendered: frame.rgb.clone(),
            };
        }

        resolves += 1;
        attempt = attempt
            .iter()
            .map(|op| weaken(op, TEXTURE_RESOLVE_STEP))
            .collect();
    }
}

/// One operation, giving up a share of itself.
///
/// Every magnitude scales by the same factor, including the two on an under-eye correction -
/// which is what keeps a re-solve from changing the *character* of the retouch as well as its
/// size. A re-solve that reduced the luminance lift and left the chroma move would produce a
/// gentler retouch that looked different rather than smaller.
#[must_use]
pub fn weaken(op: &RetouchOp, step: f32) -> RetouchOp {
    let keep = (1.0 - step).clamp(0.0, 1.0);
    match op {
        RetouchOp::Blemish {
            area,
            method,
            strength,
        } => RetouchOp::Blemish {
            area: *area,
            method: *method,
            strength: strength * keep,
        },
        RetouchOp::UnderEye {
            identity,
            luma,
            chroma,
        } => RetouchOp::UnderEye {
            identity: *identity,
            luma: luma * keep,
            chroma: chroma * keep,
        },
        RetouchOp::ToneEvening {
            mask,
            strength,
            band,
        } => RetouchOp::ToneEvening {
            mask: *mask,
            strength: strength * keep,
            band: *band,
        },
        RetouchOp::ShineReduce { area, strength } => RetouchOp::ShineReduce {
            area: *area,
            strength: strength * keep,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::retouch::{FreqBand, InpaintMethod, TEXTURE_FLOOR};
    use aura_core::MaskId;

    use crate::fixtures;

    fn mask() -> MaskId {
        MaskId::from_db("msk_00000000-0000-4000-8000-000000000020").expect("a mask id")
    }

    #[test]
    fn an_ordinary_heal_passes_the_floor_and_says_what_it_measured() {
        let (frame, context, area) = fixtures::frame_with_blemish();
        let ops = vec![RetouchOp::Blemish {
            area,
            method: InpaintMethod::Patch,
            strength: 1.0,
        }];
        let guarded = enforce(&frame, &ops, &context, TEXTURE_FLOOR);
        assert!(guarded.report.passed, "{:?}", guarded.report);
        assert!(!guarded.report.withdrawn);
        assert_eq!(guarded.ops.len(), 1);
        assert!(guarded.report.is_well_measured());
        assert_eq!(guarded.report.resolves, 0);
    }

    #[test]
    fn an_evening_only_plan_costs_no_texture_at_all() {
        // The strongest claim in the phase: `low + mid + high` reconstructs exactly, so scaling
        // the mid band cannot touch a pore however hard it runs.
        let (frame, context, _) = fixtures::frame_with_blemish();
        let ops = vec![RetouchOp::ToneEvening {
            mask: mask(),
            strength: 1.0,
            band: FreqBand::Mid,
        }];
        let guarded = enforce(&frame, &ops, &context, TEXTURE_FLOOR);
        assert!(guarded.report.passed);
        assert!(
            guarded.report.band_ratio > 0.99,
            "evening cost texture: {:.4}",
            guarded.report.band_ratio
        );
    }

    #[test]
    fn a_plan_that_cannot_meet_an_impossible_floor_is_withdrawn_entirely() {
        // A floor of 1.0 cannot be met by any heal, because a heal replaces texture with other
        // texture and the ratio lands near but not exactly at one. The guard must give up
        // rather than ship something.
        let (frame, context, area) = fixtures::frame_with_blemish();
        let ops = vec![RetouchOp::Blemish {
            area,
            method: InpaintMethod::Patch,
            strength: 1.0,
        }];
        let guarded = enforce(&frame, &ops, &context, 1.5);
        assert!(guarded.report.withdrawn);
        assert!(guarded.ops.is_empty());
        assert_eq!(guarded.report.resolves, TEXTURE_MAX_RESOLVES);
        assert_eq!(guarded.rendered, frame.rgb);
    }

    #[test]
    fn a_frame_with_no_operations_reports_an_untouched_ratio() {
        let (frame, context, _) = fixtures::frame_with_blemish();
        let guarded = enforce(&frame, &[], &context, TEXTURE_FLOOR);
        assert!(guarded.ops.is_empty());
        assert!(guarded.report.passed);
        assert!((guarded.report.band_ratio - 1.0).abs() < 1e-6);
    }

    #[test]
    fn weakening_scales_both_halves_of_an_under_eye_correction() {
        let identity = aura_core::IdentityId::from_db("idt_00000000-0000-4000-8000-000000000020")
            .expect("an identity");
        let op = RetouchOp::UnderEye {
            identity,
            luma: 0.20,
            chroma: 0.10,
        };
        let weaker = weaken(&op, 0.25);
        match weaker {
            RetouchOp::UnderEye { luma, chroma, .. } => {
                assert!((luma - 0.15).abs() < 1e-6);
                assert!((chroma - 0.075).abs() < 1e-6);
            }
            _ => panic!("the kind changed"),
        }
    }
}
