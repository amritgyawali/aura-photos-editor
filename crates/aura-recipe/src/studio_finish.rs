//! The Studio's finishing tools, carried by the recipe extension map. ADR-0108.
//!
//! Four groups a portrait editor such as Evoto offers beside skin retouching, each a
//! photographer's explicit choice and none of them run by an automatic pass:
//!
//! * **Face and body shape** - sliders anchored on the faces and the person the renderer
//!   measures from the pixels (`aura_portrait`), applied as one smooth displacement field.
//! * **Liquify** - push, bloat and pinch strokes painted by hand, and a restore brush.
//! * **Background** - replace everything behind the people with a colour, a gradient or a
//!   blur, or replace only the sky with a gradient sky.
//! * **Feature colour and makeup** - hair, eye, lip, blush and eyeshadow colour.
//!
//! Every slider is `-100..=100` (or `0..=100` for an amount) and zero means untouched, so an
//! absent block and a block of zeroes render the same photograph. Colours are stored as sRGB
//! in `0..=1`, the way a colour picker shows them, and the renderer converts them into the
//! working space so a chosen white exports as that white.
//!
//! Carried by the extension map rather than a frozen field for ADR-0102's reason: the recipe's
//! frozen shape has nowhere to put a displacement or a stroke, and widening it needs its own ADR.
use crate::{errors::recipe_invalid, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};

/// The extension key.
pub const KEY: &str = "studio_finish_v1";
/// At most this many liquify strokes on one photograph.
pub const MAX_STROKES: usize = 256;
/// At most this many points across every stroke.
pub const MAX_POINTS: usize = 16_384;

/// Face shape, applied to every detected face. Positive values enlarge, lengthen or widen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct FaceShape {
    /// Positive slims the cheeks and the jaw toward the centre of the face.
    pub slim: f32,
    /// Positive narrows the jawline.
    pub jaw: f32,
    /// Positive lengthens the chin, negative shortens it.
    pub chin: f32,
    /// Positive raises the hairline, negative lowers it.
    pub forehead: f32,
    /// Positive narrows the cheekbones.
    pub cheekbones: f32,
    /// Positive enlarges the eyes.
    pub eye_size: f32,
    /// Positive moves the eyes apart.
    pub eye_distance: f32,
    /// Positive narrows the nose.
    pub nose_width: f32,
    /// Positive lengthens the nose, negative shortens it.
    pub nose_length: f32,
    /// Positive widens the mouth.
    pub mouth_width: f32,
    /// Positive makes the lips fuller.
    pub lips: f32,
    /// Positive lifts the corners of the mouth.
    pub smile: f32,
    /// Positive enlarges the whole head, negative makes it smaller.
    pub head_size: f32,
}

/// Body shape, applied to the largest person. Positive values slim or lengthen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct BodyShape {
    /// Positive slims the whole figure horizontally.
    pub slim: f32,
    /// Positive narrows the waist.
    pub waist: f32,
    /// Positive narrows the hips.
    pub hips: f32,
    /// Positive slims the upper arms.
    pub arms: f32,
    /// Positive widens the shoulders, negative narrows them.
    pub shoulders: f32,
    /// Positive lengthens the legs.
    pub legs: f32,
    /// Positive lengthens the neck.
    pub neck: f32,
}

/// What a liquify stroke does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiquifyMode {
    /// Moves the pixels under the brush along the stroke.
    Push,
    /// Enlarges what is under the brush.
    Bloat,
    /// Shrinks what is under the brush.
    Pinch,
    /// Takes the liquify back toward the photograph as taken.
    Restore,
}

/// One liquify stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LiquifyStroke {
    pub mode: LiquifyMode,
    /// Radius relative to the shorter image edge.
    pub radius: f32,
    /// `0..=1`.
    pub strength: f32,
    /// Normalised frame coordinates, in the order they were painted.
    pub points: Vec<[f32; 2]>,
}

/// What replaces the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMode {
    /// A flat colour - a studio backdrop or a pure white for a product or ID photograph.
    Colour,
    /// A vertical gradient from `colour` at the top to `colour2` at the bottom.
    Gradient,
    /// The photograph's own background, blurred.
    Blur,
    /// Only the sky, replaced by a gradient from `colour` at the top to `colour2` at the horizon.
    Sky,
}

/// A background replacement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Background {
    pub mode: BackgroundMode,
    /// sRGB, `0..=1`.
    #[serde(default = "white")]
    pub colour: [f32; 3],
    /// sRGB, `0..=1`. The gradient's second colour.
    #[serde(default = "white")]
    pub colour2: [f32; 3],
    /// `0..=100`: how much of the replacement applies; for `blur`, how strong the blur is.
    #[serde(default = "full")]
    pub amount: f32,
    /// `0..=100`: how soft the edge between the person and the replacement is.
    #[serde(default = "default_feather")]
    pub feather: f32,
}

fn white() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn full() -> f32 {
    100.0
}

fn default_feather() -> f32 {
    30.0
}

/// A colour laid over one feature, keeping its light and shade.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Tint {
    /// sRGB, `0..=1`.
    pub colour: [f32; 3],
    /// `0..=100`.
    pub amount: f32,
}

/// Feature colours and makeup. `None` leaves a feature as photographed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Colours {
    pub hair: Option<Tint>,
    pub eyes: Option<Tint>,
    pub lips: Option<Tint>,
    pub blush: Option<Tint>,
    pub eyeshadow: Option<Tint>,
    pub eyebrows: Option<Tint>,
}

/// Everything the finishing tools can do to one photograph.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct StudioFinish {
    pub face: FaceShape,
    pub body: BodyShape,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub liquify: Vec<LiquifyStroke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    pub colours: Colours,
}

impl FaceShape {
    /// Every slider with its name, for validation and for the panel.
    #[must_use]
    pub fn sliders(&self) -> [(&'static str, f32); 13] {
        [
            ("slim", self.slim),
            ("jaw", self.jaw),
            ("chin", self.chin),
            ("forehead", self.forehead),
            ("cheekbones", self.cheekbones),
            ("eyeSize", self.eye_size),
            ("eyeDistance", self.eye_distance),
            ("noseWidth", self.nose_width),
            ("noseLength", self.nose_length),
            ("mouthWidth", self.mouth_width),
            ("lips", self.lips),
            ("smile", self.smile),
            ("headSize", self.head_size),
        ]
    }

    /// True when every slider is at zero.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.sliders().iter().all(|(_, v)| *v == 0.0)
    }
}

impl BodyShape {
    /// Every slider with its name.
    #[must_use]
    pub fn sliders(&self) -> [(&'static str, f32); 7] {
        [
            ("slim", self.slim),
            ("waist", self.waist),
            ("hips", self.hips),
            ("arms", self.arms),
            ("shoulders", self.shoulders),
            ("legs", self.legs),
            ("neck", self.neck),
        ]
    }

    /// True when every slider is at zero.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.sliders().iter().all(|(_, v)| *v == 0.0)
    }
}

impl Colours {
    /// Every feature with its name.
    #[must_use]
    pub fn all(&self) -> [(&'static str, Option<Tint>); 6] {
        [
            ("hair", self.hair),
            ("eyes", self.eyes),
            ("lips", self.lips),
            ("blush", self.blush),
            ("eyeshadow", self.eyeshadow),
            ("eyebrows", self.eyebrows),
        ]
    }

    /// True when no feature is tinted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.all()
            .iter()
            .all(|(_, t)| t.is_none_or(|t| t.amount <= 0.0))
    }
}

impl StudioFinish {
    /// True when rendering this changes no pixel.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.face.is_identity()
            && self.body.is_identity()
            && self.liquify.iter().all(|s| s.strength <= 0.0)
            && self.background.is_none_or(|b| b.amount <= 0.0)
            && self.colours.is_empty()
    }

    /// True when the renderer needs the portrait parse for this.
    #[must_use]
    pub fn needs_parse(&self) -> bool {
        !self.face.is_identity()
            || !self.body.is_identity()
            || self.background.is_some_and(|b| b.amount > 0.0)
            || !self.colours.is_empty()
    }
}

fn slider(v: f32) -> bool {
    v.is_finite() && (-100.0..=100.0).contains(&v)
}

fn amount(v: f32) -> bool {
    v.is_finite() && (0.0..=100.0).contains(&v)
}

fn colour(c: [f32; 3]) -> bool {
    c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

/// Refuse a finish whose shape has no correct interpretation.
///
/// # Errors
/// A recipe error naming the first slider, colour or stroke out of range.
pub fn validate(finish: &StudioFinish) -> AuraResult<()> {
    for (name, value) in finish.face.sliders() {
        if !slider(value) {
            return Err(recipe_invalid(
                &format!("{KEY}.face.{name}"),
                "must be -100..=100",
            ));
        }
    }
    for (name, value) in finish.body.sliders() {
        if !slider(value) {
            return Err(recipe_invalid(
                &format!("{KEY}.body.{name}"),
                "must be -100..=100",
            ));
        }
    }
    if finish.liquify.len() > MAX_STROKES {
        return Err(recipe_invalid(
            &format!("{KEY}.liquify"),
            "too many strokes",
        ));
    }
    let mut points = 0;
    for stroke in &finish.liquify {
        points += stroke.points.len();
        let ok = stroke.radius.is_finite()
            && (0.002..=0.5).contains(&stroke.radius)
            && stroke.strength.is_finite()
            && (0.0..=1.0).contains(&stroke.strength)
            && !stroke.points.is_empty()
            && stroke
                .points
                .iter()
                .all(|p| p.iter().all(|v| v.is_finite() && (-0.5..=1.5).contains(v)));
        if !ok {
            return Err(recipe_invalid(&format!("{KEY}.liquify"), "invalid stroke"));
        }
    }
    if points > MAX_POINTS {
        return Err(recipe_invalid(&format!("{KEY}.liquify"), "too many points"));
    }
    if let Some(b) = finish.background {
        if !colour(b.colour) || !colour(b.colour2) || !amount(b.amount) || !amount(b.feather) {
            return Err(recipe_invalid(
                &format!("{KEY}.background"),
                "colours must be 0..=1 and amounts 0..=100",
            ));
        }
    }
    for (name, tint) in finish.colours.all() {
        if let Some(t) = tint {
            if !colour(t.colour) || !amount(t.amount) {
                return Err(recipe_invalid(
                    &format!("{KEY}.colours.{name}"),
                    "colour must be 0..=1 and amount 0..=100",
                ));
            }
        }
    }
    Ok(())
}

/// Read and validate the finish. An absent key is the identity.
///
/// # Errors
/// A recipe error when it is malformed or fails [`validate`].
pub fn read(recipe: &Recipe) -> AuraResult<StudioFinish> {
    let Some(value) = recipe.extra.get(KEY) else {
        return Ok(StudioFinish::default());
    };
    let finish: StudioFinish = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(KEY, "invalid finishing format"))?;
    validate(&finish)?;
    Ok(finish)
}

/// Store the finish; the identity removes the key, so an untouched recipe stays byte-identical.
///
/// # Errors
/// A recipe error when validation or serialization fails.
pub fn write(recipe: &mut Recipe, finish: &StudioFinish) -> AuraResult<()> {
    validate(finish)?;
    if *finish == StudioFinish::default() {
        recipe.extra.remove(KEY);
        return Ok(());
    }
    recipe.extra.insert(
        KEY.into(),
        serde_json::to_value(finish).map_err(|_| recipe_invalid(KEY, "cannot serialize"))?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe() -> Recipe {
        crate::fixtures::reference()
    }

    #[test]
    fn an_absent_key_reads_as_the_identity() {
        let finish = read(&recipe()).unwrap();
        assert!(finish.is_identity());
        assert!(!finish.needs_parse());
    }

    #[test]
    fn a_finish_round_trips_and_the_identity_removes_the_key() {
        let mut r = recipe();
        let mut finish = StudioFinish::default();
        finish.face.eye_size = 30.0;
        finish.background = Some(Background {
            mode: BackgroundMode::Colour,
            colour: [1.0, 1.0, 1.0],
            colour2: [0.5, 0.5, 0.5],
            amount: 100.0,
            feather: 20.0,
        });
        finish.liquify.push(LiquifyStroke {
            mode: LiquifyMode::Push,
            radius: 0.05,
            strength: 0.5,
            points: vec![[0.4, 0.4], [0.45, 0.4]],
        });
        write(&mut r, &finish).unwrap();
        assert_eq!(read(&r).unwrap(), finish);
        assert!(read(&r).unwrap().needs_parse());
        write(&mut r, &StudioFinish::default()).unwrap();
        assert!(!r.extra.contains_key(KEY));
    }

    #[test]
    fn out_of_range_values_are_refused() {
        let mut finish = StudioFinish::default();
        finish.body.legs = 150.0;
        assert!(validate(&finish).is_err());
        let mut finish = StudioFinish::default();
        finish.colours.hair = Some(Tint {
            colour: [1.2, 0.0, 0.0],
            amount: 50.0,
        });
        assert!(validate(&finish).is_err());
        let mut finish = StudioFinish::default();
        finish.liquify.push(LiquifyStroke {
            mode: LiquifyMode::Bloat,
            radius: 0.05,
            strength: 0.5,
            points: vec![],
        });
        assert!(validate(&finish).is_err());
        let mut r = recipe();
        r.extra
            .insert(KEY.into(), serde_json::json!({ "face": { "nonsense": 1 } }));
        assert!(read(&r).is_err());
    }
}
