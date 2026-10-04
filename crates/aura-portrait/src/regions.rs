//! The vocabulary of a parse: nineteen regions, each a soft plane.
//!
//! Deliberately not `aura_vision::MaskKind` and not `aura_recipe::MaskKind`. Phase 18's
//! twenty classes are a frozen contract with a store behind them, and the recipe's eight are
//! what a mask may be *drawn from*. This list is what a portrait parse can *measure*, and a
//! recipe mask names one of these in its `target` - so the recipe stays frozen and the parse
//! can grow a region without an ADR.

use serde::{Deserialize, Serialize};

/// One region of a portrait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// Every visible patch of skin on a person AURA found: face, neck, arms, hands.
    Skin,
    /// The face: the oval from hairline to chin, features included, hair excluded.
    Face,
    /// The eye openings: sclera, iris and lashes between the lids.
    Eyes,
    /// The coloured ring of the eye, pupil included.
    Iris,
    /// The whites of the eyes.
    Sclera,
    /// The eyebrows.
    Eyebrows,
    /// The crescent of skin under each eye.
    UnderEyes,
    /// The nose.
    Nose,
    /// The lips, teeth excluded.
    Lips,
    /// The visible teeth.
    Teeth,
    /// The whole mouth: lips, teeth and the dark between them.
    Mouth,
    /// A beard or moustache.
    FacialHair,
    /// The neck.
    Neck,
    /// The hair on the head.
    Hair,
    /// Skin below the neck: shoulders, arms, hands.
    BodySkin,
    /// What a person is wearing.
    Clothing,
    /// The whole person: head, hair, body and clothing.
    Body,
    /// Everything that is not a person.
    Background,
    /// Open sky reaching the top of the frame.
    Sky,
}

/// Every region, in the order a parse stores and reports them.
pub const ALL_REGIONS: [Region; 19] = [
    Region::Skin,
    Region::Face,
    Region::Eyes,
    Region::Iris,
    Region::Sclera,
    Region::Eyebrows,
    Region::UnderEyes,
    Region::Nose,
    Region::Lips,
    Region::Teeth,
    Region::Mouth,
    Region::FacialHair,
    Region::Neck,
    Region::Hair,
    Region::BodySkin,
    Region::Clothing,
    Region::Body,
    Region::Background,
    Region::Sky,
];

impl Region {
    /// Stable text for the wire and for a recipe mask's `target`. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skin => "skin",
            Self::Face => "face",
            Self::Eyes => "eyes",
            Self::Iris => "iris",
            Self::Sclera => "sclera",
            Self::Eyebrows => "eyebrows",
            Self::UnderEyes => "under_eyes",
            Self::Nose => "nose",
            Self::Lips => "lips",
            Self::Teeth => "teeth",
            Self::Mouth => "mouth",
            Self::FacialHair => "facial_hair",
            Self::Neck => "neck",
            Self::Hair => "hair",
            Self::BodySkin => "body_skin",
            Self::Clothing => "clothing",
            Self::Body => "body",
            Self::Background => "background",
            Self::Sky => "sky",
        }
    }

    /// The region named by a stable slug.
    #[must_use]
    pub fn parse(slug: &str) -> Option<Self> {
        ALL_REGIONS.iter().copied().find(|r| r.as_str() == slug)
    }

    /// The region's name in the product's own words.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Skin => "Skin",
            Self::Face => "Face",
            Self::Eyes => "Eyes",
            Self::Iris => "Iris",
            Self::Sclera => "Whites of the eyes",
            Self::Eyebrows => "Eyebrows",
            Self::UnderEyes => "Under the eyes",
            Self::Nose => "Nose",
            Self::Lips => "Lips",
            Self::Teeth => "Teeth",
            Self::Mouth => "Mouth",
            Self::FacialHair => "Beard and moustache",
            Self::Neck => "Neck",
            Self::Hair => "Hair",
            Self::BodySkin => "Arms and shoulders",
            Self::Clothing => "Clothing",
            Self::Body => "Person",
            Self::Background => "Background",
            Self::Sky => "Sky",
        }
    }

    /// True for a region that only exists because a face was found.
    #[must_use]
    pub const fn needs_a_face(self) -> bool {
        !matches!(self, Self::Sky | Self::Background)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slug_round_trips_and_is_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for region in ALL_REGIONS {
            assert_eq!(Region::parse(region.as_str()), Some(region));
            assert!(seen.insert(region.as_str()));
            assert!(!region.label().is_empty());
        }
        assert!(Region::parse("wings").is_none());
    }
}
