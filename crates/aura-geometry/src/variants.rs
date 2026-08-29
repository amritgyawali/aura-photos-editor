//! Aspect variants. PHASE-23 section 6.3's last bullet.
//!
//! "Aspect variants for social and album use are generated as additional entries so the
//! delivered gallery keeps native framing while Phase 29 can use the variants." Two properties
//! of that sentence are load-bearing and easy to lose:
//!
//! **They are entries, not files.** Section 2.1: "generate additional crop variants for
//! social/album use without duplicating files (crop variants live in the recipe)." A wedding
//! with four variants per frame stored as files is five copies of a wedding. Here it is four
//! rectangles - sixty-four bytes.
//!
//! **They do not have to be improvements.** [`CropPurpose::must_improve`] is false for every
//! one of them. A 1:1 crop of a wide reception frame will essentially never score better than
//! the frame it came out of, because it has thrown away the context that made the composition
//! work; requiring an improvement from the variants ships a product with no square variants in
//! it at all. They *do* have to pass every safety rule, which is why a dance floor scene
//! generates none.
//!
//! ## Which aspect serves which purpose
//!
//! Fixed by orientation rather than by preference, and the mapping is the one thing here a
//! photographer would notice being wrong: an album page is portrait, a social post is square
//! or vertical, a header is wide. A landscape frame's album variant is 5:4 and a portrait
//! frame's is 4:5 - the same purpose, the shape that fits.

use aura_core::contract::geometry::{
    Aspect, CropPurpose, CropVariant, GeometryCode, GeometryReason, MAX_VARIANTS,
};

use crate::crop::{self, Objective};
use crate::safety::SafetyInput;

/// The aspect one purpose is delivered in, for a frame of this shape.
#[must_use]
pub const fn aspect_for(purpose: CropPurpose, frame_aspect: f32) -> Aspect {
    let landscape = frame_aspect >= 1.0;
    match purpose {
        CropPurpose::Original | CropPurpose::Primary => Aspect::Original,
        // An album page is portrait; on a landscape frame the nearest thing that still fits is
        // 5:4, and forcing 4:5 out of a 3:2 landscape throws away half the width.
        CropPurpose::Album => {
            if landscape {
                Aspect::FiveFour
            } else {
                Aspect::FourFive
            }
        }
        // Square reads correctly in every feed and crops from either orientation.
        CropPurpose::Social => Aspect::Square,
        CropPurpose::Wide => Aspect::SixteenNine,
    }
}

/// Generate the variants one scene asks for.
///
/// Returns the variants that survived the safety filter and the refusals they cost, summed
/// into the same histogram the primary search feeds.
#[must_use]
pub fn generate(
    wanted: &[CropPurpose],
    objective: &Objective<'_>,
    input: &SafetyInput<'_>,
) -> (Vec<CropVariant>, [u32; 4], Vec<GeometryReason>) {
    let mut out = Vec::new();
    let mut refused = [0u32; 4];
    let mut reasons = Vec::new();
    for purpose in wanted {
        if matches!(purpose, CropPurpose::Original | CropPurpose::Primary) {
            continue; // Both are on every plan by construction; the loader refuses them too.
        }
        if out.len() + 2 >= MAX_VARIANTS {
            break; // The original and the primary hold the first two slots.
        }
        let aspect = aspect_for(*purpose, objective.aspect);
        let (best, cost) = crop::best(aspect, *purpose, objective, input);
        for (slot, add) in refused.iter_mut().zip(cost.iter()) {
            *slot += add;
        }
        if let Some(variant) = best {
            reasons.push(GeometryReason::frame(
                GeometryCode::VariantAdded,
                format!(
                    "A {} crop was prepared for {} delivery.",
                    aspect,
                    purpose.title().to_lowercase()
                ),
                0.01,
            ));
            out.push(variant);
        }
    }
    (out, refused, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::geometry::{ProtectedKind, ProtectedRegion};
    use aura_core::contract::integrity::CropRect;

    const LANDSCAPE: f32 = 1.5;
    const PORTRAIT: f32 = 0.6667;

    fn face(x: f32, y: f32) -> ProtectedRegion {
        ProtectedRegion {
            kind: ProtectedKind::Face,
            identity: None,
            rect: CropRect {
                x,
                y,
                w: 0.09,
                h: 0.13,
            },
            primary: true,
        }
    }

    #[test]
    fn an_album_variant_follows_the_frames_orientation() {
        assert_eq!(aspect_for(CropPurpose::Album, LANDSCAPE), Aspect::FiveFour);
        assert_eq!(aspect_for(CropPurpose::Album, PORTRAIT), Aspect::FourFive);
        assert_eq!(aspect_for(CropPurpose::Social, LANDSCAPE), Aspect::Square);
        assert_eq!(aspect_for(CropPurpose::Wide, PORTRAIT), Aspect::SixteenNine);
    }

    #[test]
    fn a_variant_does_not_have_to_beat_the_original() {
        // One centred face, nothing else. Every variant scores worse than the full frame
        // because it removes context - and every one of them is still produced.
        let regions = [face(0.45, 0.40)];
        let none: [CropRect; 0] = [];
        let objective = Objective {
            regions: &regions,
            distractions: &none,
            subject: None,
            headroom: (0.05, 0.20),
            aspect: LANDSCAPE,
        };
        let input = SafetyInput {
            regions: &regions,
            aspect: LANDSCAPE,
            resolution_floor: 0.60,
        };
        let (variants, _, reasons) = generate(
            &[CropPurpose::Album, CropPurpose::Social, CropPurpose::Wide],
            &objective,
            &input,
        );
        // Three asked for, three produced, and not one of them was compared against the frame
        // as shot on the way. `generate` has no access to the scene's improvement margin and
        // nowhere to put one - which is the property, expressed the only way that cannot rot.
        assert_eq!(variants.len(), 3, "{variants:?}");
        assert_eq!(reasons.len(), 3);
        assert!(variants.iter().all(|v| v.safe));
        for purpose in [CropPurpose::Album, CropPurpose::Social, CropPurpose::Wide] {
            assert!(
                !purpose.must_improve(),
                "{purpose} was made to earn its place"
            );
            assert!(variants.iter().any(|v| v.purpose == purpose));
        }
    }

    #[test]
    fn a_frame_the_filter_refuses_produces_no_variant_rather_than_an_unsafe_one() {
        let regions = [face(0.01, 0.40), face(0.92, 0.40)];
        let none: [CropRect; 0] = [];
        let objective = Objective {
            regions: &regions,
            distractions: &none,
            subject: None,
            headroom: (0.05, 0.20),
            aspect: LANDSCAPE,
        };
        let input = SafetyInput {
            regions: &regions,
            aspect: LANDSCAPE,
            resolution_floor: 0.60,
        };
        let (variants, refused, _) = generate(&[CropPurpose::Social], &objective, &input);
        assert!(variants.is_empty());
        assert!(refused.iter().sum::<u32>() > 0);
    }

    #[test]
    fn the_original_and_the_primary_are_never_generated_as_variants() {
        let regions = [face(0.45, 0.40)];
        let none: [CropRect; 0] = [];
        let objective = Objective {
            regions: &regions,
            distractions: &none,
            subject: None,
            headroom: (0.05, 0.20),
            aspect: LANDSCAPE,
        };
        let input = SafetyInput {
            regions: &regions,
            aspect: LANDSCAPE,
            resolution_floor: 0.60,
        };
        let (variants, _, _) = generate(
            &[CropPurpose::Original, CropPurpose::Primary],
            &objective,
            &input,
        );
        assert!(variants.is_empty());
    }

    #[test]
    fn the_variant_list_cannot_overflow_the_contracts_cap() {
        let regions = [face(0.45, 0.40)];
        let none: [CropRect; 0] = [];
        let objective = Objective {
            regions: &regions,
            distractions: &none,
            subject: None,
            headroom: (0.05, 0.20),
            aspect: LANDSCAPE,
        };
        let input = SafetyInput {
            regions: &regions,
            aspect: LANDSCAPE,
            resolution_floor: 0.60,
        };
        let wanted = [
            CropPurpose::Album,
            CropPurpose::Social,
            CropPurpose::Wide,
            CropPurpose::Album,
            CropPurpose::Social,
        ];
        let (variants, _, _) = generate(&wanted, &objective, &input);
        assert!(variants.len() + 2 <= MAX_VARIANTS, "{}", variants.len());
    }
}
