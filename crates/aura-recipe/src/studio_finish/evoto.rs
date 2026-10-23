//! The Studio's measured beauty, attire, background and subject adjustments. ADR-0150.
//!
//! Evoto's sliders work on what they find - every face, the hair, the clothes, the background -
//! rather than on an area somebody drew. These six groups are AURA's own version of that idea:
//! each value is anchored on the faces and regions `aura_portrait` measures from the pixels, and
//! each is measured against the photograph it is applied to (a person's own skin, the frame's
//! own gold, the group's own median face) rather than against a constant.
//!
//! * [`SkinFinish`] - aegyo-sal, lip lines, glow, stubble shadow, rosy complexion, teeth
//!   evening, feature sharpening, beard definition, reflected colour on skin, pores.
//! * [`MakeupFinish`] - eyeliner and its wing, upper and lower lashes, lip liner, ombre lips,
//!   highlighter, bronzer, enhancing makeup already worn, and filling sparse brows.
//! * [`HairBodyFinish`] - hair gloss, a filled parting, hair colour depth, a calmer hair edge,
//!   and body definition.
//! * [`AttireFinish`] - gold jewellery, clipped red fabric, a see-through veil, embroidery, a
//!   garment's silhouette, mehndi, white and dark garments, and outfit colour.
//! * [`SceneFinish`] - backdrop glow, background saturation, brightness and warmth, depth blur,
//!   background noise and a light wrap.
//! * [`SubjectFinish`] - the people's exposure, warmth and clarity, a group's faces and skin
//!   balanced against each other, relighting, harsh shadows, a backlit subject, a vignette that
//!   avoids faces, and face brightness.
//!
//! Every value is zero when untouched, and an untouched group is not stored, so a recipe made
//! before ADR-0150 serializes - and hashes - exactly as it did. Amounts are `0..=100`; values
//! that can go either way are `-100..=100`.
use serde::{Deserialize, Serialize};

/// Skin finishing measured on each face. Every value `0..=100`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct SkinFinish {
    /// A soft highlight along the lower lid with a faint shadow under it: the "eye smile".
    pub aegyo_sal: f32,
    /// Fine vertical lines across the lips, smoothed along the lip and kept across it.
    pub lip_lines: f32,
    /// A dewy glow on the face's high points - forehead, nose bridge, cheekbones, chin.
    pub glow: f32,
    /// The blue-grey shadow of shaved stubble, evened toward the cheeks' own colour.
    pub stubble: f32,
    /// A healthier, rosier complexion: sallow yellow-green turned a little toward red.
    pub rosy: f32,
    /// A darker tooth brought toward the others; never whiter than the brightest.
    pub teeth_evening: f32,
    /// Sharpens the eyes, lashes, brows and lips, never the skin around them.
    pub feature_sharpen: f32,
    /// Deepens and defines a beard or moustache.
    pub beard_define: f32,
    /// Colour cast onto skin by bright clothing nearby - a red sari, a green wall - taken out.
    pub colour_spill: f32,
    /// Pore-sized texture on the nose and inner cheeks reduced, finer grain kept.
    pub pores: f32,
}

/// Makeup placed from each face's landmarks. Every value `0..=100`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct MakeupFinish {
    /// A dark line along the upper lash line.
    pub eyeliner: f32,
    /// How far the eyeliner flicks up and out past the outer corner.
    pub eyeliner_wing: f32,
    /// Fuller, darker upper lashes.
    pub lashes: f32,
    /// Defined lower lashes.
    pub lower_lashes: f32,
    /// The border of the lips defined in a deeper shade of their own colour.
    pub lip_liner: f32,
    /// Gradient lips: deeper colour at the centre fading toward the edge.
    pub lip_ombre: f32,
    /// A soft sheen on the cheekbones, the bridge of the nose and the cupid's bow.
    pub highlighter: f32,
    /// A warm sun-kissed tone around the face's edge and under the cheekbones.
    pub bronzer: f32,
    /// Makeup already worn - lipstick, eyeshadow, blush - made richer, measured against the skin.
    pub enhance: f32,
    /// Sparse eyebrows filled with the colour of their own darkest hairs.
    pub brow_fill: f32,
}

/// Hair and body skin. Every value `0..=100`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct HairBodyFinish {
    /// Shine along the highlights hair already has.
    pub gloss: f32,
    /// A bright parting line in the hair filled with the hair's own colour.
    pub parting: f32,
    /// Deeper, richer hair colour without changing its hue.
    pub richness: f32,
    /// A frizzy, haloed hair edge calmed against the background.
    pub edge: f32,
    /// Muscle, collarbone and arm definition: the light and shade the body already has.
    pub definition: f32,
}

/// What people are wearing. Every value `0..=100`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct AttireFinish {
    /// Richer, brighter gold jewellery, found by its colour and its glints.
    pub gold: f32,
    /// Detail back in red fabric whose colour clipped - saris, veils, lehengas.
    pub red_recovery: f32,
    /// A face seen through a net veil, cleared of the veil's haze and colour.
    pub veil_clarity: f32,
    /// Embroidery, beadwork and lace brought forward.
    pub embroidery: f32,
    /// A garment's ragged or fuzzy silhouette smoothed along its edge.
    pub garment_edge: f32,
    /// Henna on hands and arms deepened.
    pub mehndi: f32,
    /// White garments made clean white: their colour cast removed, their folds kept.
    pub white_garment: f32,
    /// Detail lifted out of crushed black suits and dark fabric, still black.
    pub dark_garment: f32,
    /// More colour in the clothing, never in the skin.
    pub outfit_colour: f32,
}

/// The background behind the people.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct SceneFinish {
    /// `0..=100`: a soft pool of light on the backdrop behind each head, as a background light.
    pub backdrop_glow: f32,
    /// `-100..=100`: background saturation; negative toward grey.
    pub saturation: f32,
    /// `-100..=100`: background brightness, in up to a stop either way.
    pub brightness: f32,
    /// `-100..=100`: background warmth; negative is cooler.
    pub warmth: f32,
    /// `0..=100`: a blur that grows with distance from the people.
    pub depth_blur: f32,
    /// `0..=100`: noise reduction on the background only.
    pub denoise: f32,
    /// `0..=100`: the background's light wrapped onto the people's edges, so they sit in it.
    pub light_wrap: f32,
}

/// The people as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct SubjectFinish {
    /// `-100..=100`: the people's exposure, up to a stop either way, background unchanged.
    pub exposure: f32,
    /// `-100..=100`: the people's warmth.
    pub warmth: f32,
    /// `-100..=100`: local contrast on the people; skin takes a quarter of it.
    pub clarity: f32,
    /// `0..=100`: darker faces in a group lifted toward the group's median face.
    pub group_balance: f32,
    /// `0..=100`: each face's skin colour pulled toward the group's median skin colour.
    pub group_skin: f32,
    /// `0..=100`: a soft directional light across the whole person.
    pub relight: f32,
    /// `-180..=180` degrees: where that light comes from, seen from the camera; 0 is the front.
    pub relight_direction: f32,
    /// `0..=100`: hard shadows across faces softened toward the face's own median.
    pub harsh_shadows: f32,
    /// `0..=100`: a backlit, dark subject lifted while the bright background stays.
    pub backlight: f32,
    /// `0..=100`: darker corners that never reach a face.
    pub face_vignette: f32,
    /// `-100..=100`: the faces' brightness alone.
    pub face_brightness: f32,
}

impl Default for SubjectFinish {
    /// Untouched; a relight, when there is one, starts from the camera's left.
    fn default() -> Self {
        Self {
            exposure: 0.0,
            warmth: 0.0,
            clarity: 0.0,
            group_balance: 0.0,
            group_skin: 0.0,
            relight: 0.0,
            relight_direction: -45.0,
            harsh_shadows: 0.0,
            backlight: 0.0,
            face_vignette: 0.0,
            face_brightness: 0.0,
        }
    }
}

/// How a value may range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    /// `0..=100`.
    Amount,
    /// `-100..=100`.
    Signed,
    /// `-180..=180` degrees.
    Degrees,
}

impl Range {
    /// True when `v` is inside this range.
    #[must_use]
    pub fn holds(self, v: f32) -> bool {
        v.is_finite()
            && match self {
                Self::Amount => (0.0..=100.0).contains(&v),
                Self::Signed => (-100.0..=100.0).contains(&v),
                Self::Degrees => (-180.0..=180.0).contains(&v),
            }
    }
}

impl SkinFinish {
    /// Every value with its wire name.
    #[must_use]
    pub fn values(&self) -> [(&'static str, f32, Range); 10] {
        [
            ("aegyoSal", self.aegyo_sal, Range::Amount),
            ("lipLines", self.lip_lines, Range::Amount),
            ("glow", self.glow, Range::Amount),
            ("stubble", self.stubble, Range::Amount),
            ("rosy", self.rosy, Range::Amount),
            ("teethEvening", self.teeth_evening, Range::Amount),
            ("featureSharpen", self.feature_sharpen, Range::Amount),
            ("beardDefine", self.beard_define, Range::Amount),
            ("colourSpill", self.colour_spill, Range::Amount),
            ("pores", self.pores, Range::Amount),
        ]
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.values().iter().all(|(_, v, _)| *v == 0.0)
    }
}

impl MakeupFinish {
    /// Every value with its wire name.
    #[must_use]
    pub fn values(&self) -> [(&'static str, f32, Range); 10] {
        [
            ("eyeliner", self.eyeliner, Range::Amount),
            ("eyelinerWing", self.eyeliner_wing, Range::Amount),
            ("lashes", self.lashes, Range::Amount),
            ("lowerLashes", self.lower_lashes, Range::Amount),
            ("lipLiner", self.lip_liner, Range::Amount),
            ("lipOmbre", self.lip_ombre, Range::Amount),
            ("highlighter", self.highlighter, Range::Amount),
            ("bronzer", self.bronzer, Range::Amount),
            ("enhance", self.enhance, Range::Amount),
            ("browFill", self.brow_fill, Range::Amount),
        ]
    }

    /// True when nothing is set. A wing without a liner draws nothing.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.values()
            .iter()
            .filter(|(name, _, _)| *name != "eyelinerWing")
            .all(|(_, v, _)| *v == 0.0)
    }

    /// True when every value is zero, so it need not be stored. A wing set without a liner is
    /// kept, so the panel shows it again.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl HairBodyFinish {
    /// Every value with its wire name.
    #[must_use]
    pub fn values(&self) -> [(&'static str, f32, Range); 5] {
        [
            ("gloss", self.gloss, Range::Amount),
            ("parting", self.parting, Range::Amount),
            ("richness", self.richness, Range::Amount),
            ("edge", self.edge, Range::Amount),
            ("definition", self.definition, Range::Amount),
        ]
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.values().iter().all(|(_, v, _)| *v == 0.0)
    }
}

impl AttireFinish {
    /// Every value with its wire name.
    #[must_use]
    pub fn values(&self) -> [(&'static str, f32, Range); 9] {
        [
            ("gold", self.gold, Range::Amount),
            ("redRecovery", self.red_recovery, Range::Amount),
            ("veilClarity", self.veil_clarity, Range::Amount),
            ("embroidery", self.embroidery, Range::Amount),
            ("garmentEdge", self.garment_edge, Range::Amount),
            ("mehndi", self.mehndi, Range::Amount),
            ("whiteGarment", self.white_garment, Range::Amount),
            ("darkGarment", self.dark_garment, Range::Amount),
            ("outfitColour", self.outfit_colour, Range::Amount),
        ]
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.values().iter().all(|(_, v, _)| *v == 0.0)
    }
}

impl SceneFinish {
    /// Every value with its wire name.
    #[must_use]
    pub fn values(&self) -> [(&'static str, f32, Range); 7] {
        [
            ("backdropGlow", self.backdrop_glow, Range::Amount),
            ("saturation", self.saturation, Range::Signed),
            ("brightness", self.brightness, Range::Signed),
            ("warmth", self.warmth, Range::Signed),
            ("depthBlur", self.depth_blur, Range::Amount),
            ("denoise", self.denoise, Range::Amount),
            ("lightWrap", self.light_wrap, Range::Amount),
        ]
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.values().iter().all(|(_, v, _)| *v == 0.0)
    }
}

impl SubjectFinish {
    /// Every value with its wire name.
    #[must_use]
    pub fn values(&self) -> [(&'static str, f32, Range); 11] {
        [
            ("exposure", self.exposure, Range::Signed),
            ("warmth", self.warmth, Range::Signed),
            ("clarity", self.clarity, Range::Signed),
            ("groupBalance", self.group_balance, Range::Amount),
            ("groupSkin", self.group_skin, Range::Amount),
            ("relight", self.relight, Range::Amount),
            ("relightDirection", self.relight_direction, Range::Degrees),
            ("harshShadows", self.harsh_shadows, Range::Amount),
            ("backlight", self.backlight, Range::Amount),
            ("faceVignette", self.face_vignette, Range::Amount),
            ("faceBrightness", self.face_brightness, Range::Signed),
        ]
    }

    /// True when rendering this changes no pixel. The light's direction alone changes nothing.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.values()
            .iter()
            .filter(|(name, _, _)| *name != "relightDirection")
            .all(|(_, v, _)| *v == 0.0)
    }

    /// True when this is exactly the default, so it need not be stored.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Every group's values, for validation: `(group, name, value, range)`.
pub(crate) fn all_values(
    skin: &SkinFinish,
    makeup: &MakeupFinish,
    hair: &HairBodyFinish,
    attire: &AttireFinish,
    scene: &SceneFinish,
    subject: &SubjectFinish,
) -> Vec<(&'static str, &'static str, f32, Range)> {
    let mut out = Vec::with_capacity(52);
    out.extend(skin.values().map(|(n, v, r)| ("skin", n, v, r)));
    out.extend(makeup.values().map(|(n, v, r)| ("makeup", n, v, r)));
    out.extend(hair.values().map(|(n, v, r)| ("hair", n, v, r)));
    out.extend(attire.values().map(|(n, v, r)| ("attire", n, v, r)));
    out.extend(scene.values().map(|(n, v, r)| ("scene", n, v, r)));
    out.extend(subject.values().map(|(n, v, r)| ("subject", n, v, r)));
    out
}

/// At most this many chosen faces.
pub const MAX_CHOSEN_FACES: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_group_starts_untouched_and_a_wing_alone_draws_nothing() {
        assert!(SkinFinish::default().is_identity());
        assert!(HairBodyFinish::default().is_identity());
        assert!(AttireFinish::default().is_identity());
        assert!(SceneFinish::default().is_identity());
        assert!(SubjectFinish::default().is_identity());
        let wing_only = MakeupFinish {
            eyeliner_wing: 80.0,
            ..MakeupFinish::default()
        };
        assert!(wing_only.is_identity());
        let turned = SubjectFinish {
            relight_direction: 90.0,
            ..SubjectFinish::default()
        };
        assert!(turned.is_identity());
        assert!(!turned.is_default());
    }

    #[test]
    fn ranges_hold_what_they_say() {
        assert!(Range::Amount.holds(0.0) && Range::Amount.holds(100.0));
        assert!(!Range::Amount.holds(-1.0) && !Range::Amount.holds(f32::NAN));
        assert!(Range::Signed.holds(-100.0) && !Range::Signed.holds(100.5));
        assert!(Range::Degrees.holds(-180.0) && !Range::Degrees.holds(181.0));
    }
}
