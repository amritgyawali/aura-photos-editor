//! The four aspects an album and a feed want, and the one that is delivered.
//!
//! Section 2.1: "Multi-aspect delivery: generate additional crop variants for social/album use
//! without duplicating files (crop variants live in the recipe)." Section 6.3: "Aspect variants
//! for social and album use are generated as additional entries so the delivered gallery keeps
//! native framing while Phase 29 can use the variants."
//!
//! ## The first entry is always the original, and the delivered one is an index
//!
//! [`assemble`] returns a list whose first element is the frame as it was shot, at its own
//! aspect, always safe and always present. Everything else is an alternative, and
//! `GeometryPlan::primary_crop` points at whichever one is delivered. That is what makes "a plan
//! that decided nothing" and "a plan nobody ran" different rows rather than the same absence, and
//! it is why reverting is one field rather than a reconstruction.
//!
//! ## An unsafe variant is stored rather than dropped
//!
//! "Why is there no square crop of this photograph" is a question the panel has to be able to
//! answer, and it cannot answer it from an absence. So a variant that could not be generated
//! safely is written with `safe = false` and the code that refused it - and what an unsafe
//! variant may never be is the delivered one. The contract says so, a database CHECK says so
//! again, and [`assemble`] cannot construct one because it takes the primary separately.

use aura_core::contract::geometry::{
    AspectRatio, CropPurpose, CropVariant, GeometryCode, MAX_VARIANTS,
};

use crate::crop::{self, Search};

/// One aspect that was asked for and what came back.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    /// The aspect.
    pub aspect: AspectRatio,
    /// The variant, safe or not.
    pub variant: CropVariant,
    /// Why it is unsafe, when it is.
    pub refusal: Option<GeometryCode>,
}

/// Generate every aspect variant a scene asks for.
///
/// The variants are generated for **every frame that can carry them**, including the frames whose
/// scene row switches automatic cropping off. That is deliberate and it is the difference between
/// the two decisions this phase makes: `crop = false` says AURA may not *replace* what the
/// photographer framed, and it does not say phase 29 may not lay the photograph out square. One
/// is a decision about the delivery and the other is an option on it.
#[must_use]
pub fn generate(search: &Search<'_>, aspects: &[AspectRatio]) -> Vec<Attempt> {
    let mut out = Vec::with_capacity(aspects.len());
    for aspect in aspects {
        if *aspect == AspectRatio::Original {
            continue;
        }
        let (best, refusals) = crop::search(search, *aspect);
        match best {
            Some(candidate) => out.push(Attempt {
                aspect: *aspect,
                variant: crop::into_variant(&candidate, *aspect, true),
                refusal: None,
            }),
            None => {
                // Nothing at this aspect was safe. The rectangle stored is the aspect fitted to
                // the bounds and centred - not a proposal, but a place for the panel to point at
                // when it says why there is no square crop of this photograph.
                let rect = aura_core::contract::geometry::fit_aspect(
                    search.bounds,
                    search.limits.frame_aspect,
                    aspect.ratio().unwrap_or(search.limits.frame_aspect),
                    (
                        search.bounds.x + search.bounds.w / 2.0,
                        search.bounds.y + search.bounds.h / 2.0,
                    ),
                );
                out.push(Attempt {
                    aspect: *aspect,
                    variant: CropVariant {
                        aspect: *aspect,
                        rect,
                        purpose: aspect.purpose(),
                        score: 0.0,
                        safe: false,
                    },
                    // The most specific refusal available, and `VariantUnsafe` when the search
                    // produced none - which happens when the bounds themselves were degenerate.
                    refusal: Some(
                        refusals
                            .into_iter()
                            .find(|code| code.is_safety_refusal())
                            .unwrap_or(GeometryCode::VariantUnsafe),
                    ),
                });
            }
        }
    }
    out
}

/// Build the stored list, with the original first and the delivered one indexed.
///
/// `primary` is the rectangle that is delivered *at the frame's own aspect*, which is the whole
/// frame on the seventy per cent of a wedding this phase leaves alone. Returning the index rather
/// than a copy is what makes exactly one place in the product hold a delivered crop.
#[must_use]
pub fn assemble(
    original_score: f32,
    primary: Option<CropVariant>,
    attempts: &[Attempt],
) -> (Vec<CropVariant>, usize) {
    let mut crops = Vec::with_capacity(MAX_VARIANTS);
    let mut primary_index = 0;

    match primary {
        Some(variant) if !variant.is_full_frame() && variant.safe => {
            // A proposal replaced the original framing. It goes first and *is* the original
            // entry: the list is one row per aspect, and two rows at the frame's own aspect
            // would make `alternates` return the thing that was delivered.
            crops.push(CropVariant {
                aspect: AspectRatio::Original,
                purpose: CropPurpose::Primary,
                ..variant
            });
        }
        _ => crops.push(CropVariant::original(original_score)),
    }

    for attempt in attempts {
        if crops.len() >= MAX_VARIANTS {
            break;
        }
        crops.push(attempt.variant);
    }

    // Belt and braces. `primary_index` is zero by construction above, and the contract, the
    // schema and the panel all assume the delivered variant is safe - so the one place that
    // could break the assumption checks it rather than trusting the construction.
    if crops.get(primary_index).is_some_and(|v| !v.safe) {
        primary_index = crops
            .iter()
            .position(|variant| variant.safe)
            .unwrap_or(primary_index);
    }
    (crops, primary_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::composition::Box2;
    use aura_core::contract::geometry::{ProtectedContent, ProtectedRegion};
    use aura_core::SceneId;

    use crate::crop::{Measured, Objective};
    use crate::profiles::{Placement, SceneRule};
    use crate::safety::Limits;

    fn measured() -> Measured {
        let (w, h) = (192usize, 128usize);
        let mut rgb = vec![0.15f32; w * h * 3];
        for y in 40..90 {
            for x in 60..130 {
                let value = if (x / 2 + y / 2) % 2 == 0 { 0.85 } else { 0.2 };
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut((y * w + x) * 3 + channel) {
                        *slot = value;
                    }
                }
            }
        }
        Measured::of_proxy(&rgb, w, h)
    }

    fn rule() -> SceneRule {
        SceneRule {
            scene: SceneId::CouplePortrait,
            crop: true,
            min_improvement: 0.06,
            max_zoom: 0.75,
            headroom: 0.12,
            placement: Placement::Thirds,
        }
    }

    #[test]
    fn every_requested_aspect_comes_back_exactly_once() {
        let frame = measured();
        let search = Search {
            objective: Objective {
                frame: &frame,
                subject: Box2 {
                    x: 0.31,
                    y: 0.31,
                    w: 0.36,
                    h: 0.39,
                },
                placement: Placement::Thirds,
                headroom_target: 0.12,
            },
            protected: &[],
            limits: Limits {
                frame_aspect: frame.frame_aspect,
                ..Limits::default()
            },
            rule: rule(),
            bounds: Box2::FULL,
        };
        let attempts = generate(&search, &AspectRatio::VARIANTS);
        assert_eq!(attempts.len(), 4);
        for aspect in AspectRatio::VARIANTS {
            assert_eq!(
                attempts.iter().filter(|a| a.aspect == aspect).count(),
                1,
                "{aspect}"
            );
        }
    }

    #[test]
    fn a_variant_that_cannot_be_generated_safely_is_still_stored_with_its_reason() {
        // Faces at the extreme left and right of a wide frame: no square crop can contain both.
        let frame = measured();
        let protected: Vec<ProtectedRegion> = [0.02f32, 0.92]
            .into_iter()
            .map(|x| {
                ProtectedRegion::anonymous(
                    ProtectedContent::PrimaryFace,
                    Box2 {
                        x,
                        y: 0.45,
                        w: 0.06,
                        h: 0.10,
                    },
                )
            })
            .collect();
        let search = Search {
            objective: Objective {
                frame: &frame,
                subject: Box2 {
                    x: 0.02,
                    y: 0.45,
                    w: 0.96,
                    h: 0.10,
                },
                placement: Placement::Centre,
                headroom_target: 0.12,
            },
            protected: &protected,
            limits: Limits {
                frame_aspect: frame.frame_aspect,
                ..Limits::default()
            },
            rule: rule(),
            bounds: Box2::FULL,
        };
        let attempts = generate(&search, &[AspectRatio::Square]);
        let square = attempts.first().expect("one attempt");
        assert!(!square.variant.safe);
        assert!(square.refusal.is_some());
        assert!(square
            .refusal
            .is_some_and(GeometryCode::is_safety_refusal));
    }

    #[test]
    fn the_first_entry_is_the_original_when_nothing_replaced_it() {
        let (crops, primary) = assemble(0.5, None, &[]);
        assert_eq!(primary, 0);
        assert_eq!(crops.len(), 1);
        assert_eq!(crops[0].aspect, AspectRatio::Original);
        assert!(crops[0].is_full_frame());
        assert!(crops[0].safe);
    }

    #[test]
    fn a_proposal_replaces_the_original_entry_rather_than_joining_it() {
        let proposal = CropVariant {
            aspect: AspectRatio::Original,
            rect: Box2 {
                x: 0.1,
                y: 0.1,
                w: 0.8,
                h: 0.8,
            },
            purpose: CropPurpose::Primary,
            score: 0.7,
            safe: true,
        };
        let (crops, primary) = assemble(0.5, Some(proposal), &[]);
        assert_eq!(primary, 0);
        assert_eq!(
            crops
                .iter()
                .filter(|v| v.aspect == AspectRatio::Original)
                .count(),
            1
        );
        assert!(!crops[0].is_full_frame());
    }

    #[test]
    fn the_delivered_variant_is_never_an_unsafe_one() {
        let unsafe_primary = CropVariant {
            aspect: AspectRatio::Original,
            rect: Box2 {
                x: 0.2,
                y: 0.2,
                w: 0.5,
                h: 0.5,
            },
            purpose: CropPurpose::Primary,
            score: 0.9,
            safe: false,
        };
        let (crops, primary) = assemble(0.4, Some(unsafe_primary), &[]);
        assert!(crops[primary].safe, "an unsafe variant was delivered");
        assert!(crops[primary].is_full_frame());
    }

    #[test]
    fn the_list_never_exceeds_the_contracts_bound() {
        let attempt = |aspect: AspectRatio| Attempt {
            aspect,
            variant: CropVariant {
                aspect,
                rect: Box2::FULL,
                purpose: aspect.purpose(),
                score: 0.5,
                safe: true,
            },
            refusal: None,
        };
        let many: Vec<Attempt> = AspectRatio::VARIANTS
            .into_iter()
            .chain(AspectRatio::VARIANTS)
            .map(attempt)
            .collect();
        let (crops, _) = assemble(0.5, None, &many);
        assert!(crops.len() <= MAX_VARIANTS, "{}", crops.len());
    }
}
