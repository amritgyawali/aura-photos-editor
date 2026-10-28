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
//! * **Portrait light** - a key light shaped by each head, a fill on the shadow side,
//!   catchlights in eyes that have none, and a soft glow. ADR-0127.
//! * **Measured beauty, attire, background and subject** - six groups of sliders that work on
//!   what the parse finds, twenty more face sliders, ten more body sliders, two more
//!   backgrounds, and a choice of which faces a finish applies to. ADR-0150, in [`evoto`].
//!
//! Every slider is `-100..=100` (or `0..=100` for an amount) and zero means untouched, so an
//! absent block and a block of zeroes render the same photograph. Colours are stored as sRGB
//! in `0..=1`, the way a colour picker shows them, and the renderer converts them into the
//! working space so a chosen white exports as that white.
//!
//! Carried by the extension map rather than a frozen field for ADR-0102's reason: the recipe's
//! frozen shape has nowhere to put a displacement or a stroke, and widening it needs its own ADR.
use crate::{errors::recipe_invalid, retouch_tools::Matte, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};

pub mod evoto;
pub use evoto::{
    AttireFinish, HairBodyFinish, MakeupFinish, Range, SceneFinish, SkinFinish, SubjectFinish,
    MAX_CHOSEN_FACES,
};

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
    // ---- ADR-0150 ----
    /// Positive raises the eyes, negative lowers them.
    pub eye_height: f32,
    /// Positive lifts the outer corners of the eyes.
    pub eye_tilt: f32,
    /// Positive opens the inner corners of the eyes toward the nose.
    pub inner_corner: f32,
    /// Positive lengthens the outer corners of the eyes.
    pub outer_corner: f32,
    /// `0..=100`: the smaller eye opened toward the larger, measured on each face.
    pub eye_symmetry: f32,
    /// Positive narrows the bridge of the nose.
    pub nose_bridge: f32,
    /// Positive makes the tip of the nose smaller.
    pub nose_tip: f32,
    /// Positive lifts the tip of the nose.
    pub nose_lift: f32,
    /// Positive narrows the nostrils.
    pub nostrils: f32,
    /// Positive makes the apples of the cheeks fuller.
    pub cheek_apples: f32,
    /// Positive makes the upper lip fuller.
    pub upper_lip: f32,
    /// Positive makes the lower lip fuller.
    pub lower_lip: f32,
    /// Positive raises the mouth, shortening the space under the nose.
    pub mouth_height: f32,
    /// Positive raises the eyebrows.
    pub brow_height: f32,
    /// Positive lifts the tails of the eyebrows.
    pub brow_tilt: f32,
    /// Positive makes the eyebrows thicker.
    pub brow_thickness: f32,
    /// Positive fills out the temples.
    pub temples: f32,
    /// Positive lengthens the lower face.
    pub face_length: f32,
    /// Positive narrows the chin.
    pub chin_width: f32,
    /// Positive adds volume at the crown.
    pub hair_volume: f32,
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
    // ---- ADR-0150 ----
    /// Positive slims the thighs.
    pub thighs: f32,
    /// Positive slims the calves.
    pub calves: f32,
    /// Positive flattens the stomach on the side the silhouette shows it.
    pub belly: f32,
    /// Positive lifts the hips.
    pub hip_lift: f32,
    /// Positive raises and squares the shoulders, negative lets them slope.
    pub shoulder_slope: f32,
    /// Positive lengthens the torso between the shoulders and the waist.
    pub torso: f32,
    /// Positive makes the whole figure below the shoulders taller.
    pub height: f32,
    /// Positive slims the neck.
    pub neck_width: f32,
    /// `0..=100`: a double chin lifted and its fold evened.
    pub double_chin: f32,
    /// `0..=100`: a tilted head turned toward level, measured from the eye line.
    pub head_level: f32,
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
    /// A flat colour that keeps the shadows the people cast on the backdrop. ADR-0150.
    KeepShadows,
    /// Only the sky, replaced by a gradient sky with soft clouds. ADR-0150.
    CloudSky,
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

/// Portrait light: a virtual light on every detected face, and the finishing touches that go
/// with light. ADR-0127.
///
/// Every amount is `0..=100` and zero means untouched; the direction and height only say where
/// the key light is, so they change nothing while `key` and `catchlights` are zero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Light {
    /// `0..=100`: how strong the key light added to each face is.
    pub key: f32,
    /// `-180..=180` degrees: where the key light comes from, seen from the camera. `0` is from
    /// the camera, `-90` from the photograph's left, `90` from its right.
    pub direction: f32,
    /// `-30..=80` degrees: how far above the eyes the key light is.
    pub height: f32,
    /// `0..=100`: how far the shadow side of each face is opened.
    pub fill: f32,
    /// `0..=100`: catchlights added to open eyes that have none, placed from the key light.
    pub catchlights: f32,
    /// `0..=100`: a soft glow from the brightest parts of the photograph.
    pub glow: f32,
}

impl Default for Light {
    /// No light added; a key light, when one is, starts where a portrait light usually is: to
    /// the camera's left and a little above the eyes.
    fn default() -> Self {
        Self {
            key: 0.0,
            direction: -45.0,
            height: 35.0,
            fill: 0.0,
            catchlights: 0.0,
            glow: 0.0,
        }
    }
}

impl Light {
    /// True when rendering this changes no pixel.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.key <= 0.0 && self.fill <= 0.0 && self.catchlights <= 0.0 && self.glow <= 0.0
    }

    /// True when this is exactly the default, so it need not be stored.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// True when the renderer needs the faces for this.
    #[must_use]
    pub fn needs_faces(&self) -> bool {
        self.key > 0.0 || self.fill > 0.0 || self.catchlights > 0.0
    }
}

/// Everything the finishing tools can do to one photograph.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct StudioFinish {
    pub face: FaceShape,
    pub body: BodyShape,
    pub liquify: Vec<LiquifyStroke>,
    /// Restore displacement on confidently segmented background, 0..=100.
    /// Omitted at zero to preserve existing recipe hashes.
    #[serde(skip_serializing_if = "is_zero")]
    pub background_repair: f32,
    pub background: Option<Background>,
    pub colours: Colours,
    pub light: Light,
    /// ADR-0150: measured skin finishing on each face.
    pub skin: SkinFinish,
    /// ADR-0150: makeup placed from each face's landmarks.
    pub makeup: MakeupFinish,
    /// ADR-0150: hair and body skin.
    pub hair: HairBodyFinish,
    /// ADR-0150: what people are wearing.
    pub attire: AttireFinish,
    /// ADR-0150: the background behind the people.
    pub scene: SceneFinish,
    /// ADR-0150: the people as a whole.
    pub subject: SubjectFinish,
    /// ADR-0150: the faces the face, skin, makeup and per-face light adjustments apply to, as
    /// points in normalised frame coordinates; empty is every face.
    pub faces: Vec<[f32; 2]>,
    /// ADR-0150: the people, as the learned subject selection found them when a tool first
    /// needed to tell them from their background, frozen here so the photograph renders the same
    /// on every machine. Absent where the model is not installed; the parse is used then.
    #[serde(default)]
    pub subject_matte: Option<Matte>,
    /// ADR-0185: the faces above were agreed between the portrait parse and the learned face
    /// detector the first time a tool needed the parse here. Once set it stays set, so a
    /// photographer who then chooses every face again keeps that choice.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub faces_reconciled: bool,
}

impl FaceShape {
    /// Every slider with its name, for validation and for the panel.
    #[must_use]
    pub fn sliders(&self) -> [(&'static str, f32); 33] {
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
            ("eyeHeight", self.eye_height),
            ("eyeTilt", self.eye_tilt),
            ("innerCorner", self.inner_corner),
            ("outerCorner", self.outer_corner),
            ("eyeSymmetry", self.eye_symmetry),
            ("noseBridge", self.nose_bridge),
            ("noseTip", self.nose_tip),
            ("noseLift", self.nose_lift),
            ("nostrils", self.nostrils),
            ("cheekApples", self.cheek_apples),
            ("upperLip", self.upper_lip),
            ("lowerLip", self.lower_lip),
            ("mouthHeight", self.mouth_height),
            ("browHeight", self.brow_height),
            ("browTilt", self.brow_tilt),
            ("browThickness", self.brow_thickness),
            ("temples", self.temples),
            ("faceLength", self.face_length),
            ("chinWidth", self.chin_width),
            ("hairVolume", self.hair_volume),
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
    pub fn sliders(&self) -> [(&'static str, f32); 17] {
        [
            ("slim", self.slim),
            ("waist", self.waist),
            ("hips", self.hips),
            ("arms", self.arms),
            ("shoulders", self.shoulders),
            ("legs", self.legs),
            ("neck", self.neck),
            ("thighs", self.thighs),
            ("calves", self.calves),
            ("belly", self.belly),
            ("hipLift", self.hip_lift),
            ("shoulderSlope", self.shoulder_slope),
            ("torso", self.torso),
            ("height", self.height),
            ("neckWidth", self.neck_width),
            ("doubleChin", self.double_chin),
            ("headLevel", self.head_level),
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
            && self.light.is_identity()
            && !self.has_measured()
    }

    /// True when something here tells the people from their background, and so is better for a
    /// learned subject selection than for the parse's colour models. ADR-0150.
    #[must_use]
    pub fn needs_subject(&self) -> bool {
        let s = &self.subject;
        self.background.is_some_and(|b| {
            b.amount > 0.0 && !matches!(b.mode, BackgroundMode::Sky | BackgroundMode::CloudSky)
        }) || !self.scene.is_identity()
            || !self.body.is_identity()
            || self.hair.edge > 0.0
            || s.exposure != 0.0
            || s.warmth != 0.0
            || s.clarity != 0.0
            || s.backlight > 0.0
            || s.relight > 0.0
            || s.face_vignette > 0.0
    }

    /// True when any ADR-0150 group changes pixels.
    #[must_use]
    pub fn has_measured(&self) -> bool {
        !(self.skin.is_identity()
            && self.makeup.is_identity()
            && self.hair.is_identity()
            && self.attire.is_identity()
            && self.scene.is_identity()
            && self.subject.is_identity())
    }

    /// True when the renderer needs the portrait parse for this.
    #[must_use]
    pub fn needs_parse(&self) -> bool {
        !self.face.is_identity()
            || !self.body.is_identity()
            || self.background_repair > 0.0
            || self.background.is_some_and(|b| b.amount > 0.0)
            || !self.colours.is_empty()
            || self.light.needs_faces()
            || self.has_measured()
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip predicate receives a reference.
fn is_zero(value: &f32) -> bool {
    *value == 0.0
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
    if !amount(finish.background_repair) {
        return Err(recipe_invalid(
            &format!("{KEY}.backgroundRepair"),
            "must be 0..=100",
        ));
    }
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
    let l = finish.light;
    let light_ok = amount(l.key)
        && amount(l.fill)
        && amount(l.catchlights)
        && amount(l.glow)
        && l.direction.is_finite()
        && (-180.0..=180.0).contains(&l.direction)
        && l.height.is_finite()
        && (-30.0..=80.0).contains(&l.height);
    if !light_ok {
        return Err(recipe_invalid(
            &format!("{KEY}.light"),
            "amounts must be 0..=100, direction -180..=180 and height -30..=80",
        ));
    }
    validate_measured(finish)?;
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

/// The ADR-0150 values: the three one-sided shape sliders, the six groups and the chosen faces.
fn validate_measured(finish: &StudioFinish) -> AuraResult<()> {
    for (name, value) in [
        ("face.eyeSymmetry", finish.face.eye_symmetry),
        ("body.doubleChin", finish.body.double_chin),
        ("body.headLevel", finish.body.head_level),
    ] {
        if !amount(value) {
            return Err(recipe_invalid(&format!("{KEY}.{name}"), "must be 0..=100"));
        }
    }
    for (group, name, value, range) in evoto::all_values(
        &finish.skin,
        &finish.makeup,
        &finish.hair,
        &finish.attire,
        &finish.scene,
        &finish.subject,
    ) {
        if !range.holds(value) {
            return Err(recipe_invalid(
                &format!("{KEY}.{group}.{name}"),
                "out of range",
            ));
        }
    }
    if finish
        .subject_matte
        .as_ref()
        .is_some_and(|m| m.decode().is_none() || m.width == 0 || m.height == 0)
    {
        return Err(recipe_invalid(
            &format!("{KEY}.subjectMatte"),
            "an unreadable selection",
        ));
    }
    if finish.faces.len() > MAX_CHOSEN_FACES
        || !finish
            .faces
            .iter()
            .all(|p| p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
    {
        return Err(recipe_invalid(
            &format!("{KEY}.faces"),
            "at most 32 points inside the frame",
        ));
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

/// Store the finish, every field written out - zeroes, an absent background as `null`, no
/// strokes as `[]`, the identity included.
///
/// `schema::merge` writes the leaves a proposal carries and keeps every other leaf of the base,
/// so a field left out of the stored finish is a field that can never go back: a reset, "Keep
/// original background", "Clear liquify" and a slider returned to zero were all kept from the
/// previous step while this omitted them. ADR-0150 section 5.
///
/// # Errors
/// A recipe error when validation or serialization fails.
pub fn write(recipe: &mut Recipe, finish: &StudioFinish) -> AuraResult<()> {
    validate(finish)?;
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
    fn background_repair_round_trips_and_old_recipes_omit_the_new_field() {
        let mut finish = StudioFinish::default();
        assert!(!serde_json::to_string(&finish)
            .unwrap()
            .contains("backgroundRepair"));
        finish.face.slim = 20.0;
        finish.background_repair = 75.0;
        let mut r = recipe();
        write(&mut r, &finish).unwrap();
        assert_eq!(read(&r).unwrap(), finish);
        assert!(finish.needs_parse());
        for invalid in [-1.0, 101.0, f32::NAN, f32::INFINITY] {
            finish.background_repair = invalid;
            assert!(validate(&finish).is_err());
        }
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
        assert_eq!(read(&r).unwrap(), StudioFinish::default());
    }

    #[test]
    fn a_light_round_trips_and_a_default_light_is_not_stored() {
        let mut r = recipe();
        let mut finish = StudioFinish::default();
        finish.light.key = 40.0;
        finish.light.direction = 30.0;
        finish.light.catchlights = 50.0;
        write(&mut r, &finish).unwrap();
        let back = read(&r).unwrap();
        assert_eq!(back, finish);
        assert!(back.needs_parse());
        // A glow alone needs no faces.
        let mut glow = StudioFinish::default();
        glow.light.glow = 30.0;
        assert!(!glow.is_identity());
        assert!(!glow.needs_parse());
        // Every field is written out, so the merge can always take it back to neutral.
        let mut shape = StudioFinish::default();
        shape.face.slim = 10.0;
        let json = serde_json::to_value(&shape).unwrap();
        assert!(json.get("light").is_some(), "{json}");
        // The direction alone changes nothing.
        let mut aimed = StudioFinish::default();
        aimed.light.direction = 90.0;
        assert!(aimed.is_identity());
    }

    #[test]
    fn the_hundred_additions_round_trip_and_are_not_stored_untouched() {
        let mut r = recipe();
        let mut finish = StudioFinish::default();
        finish.face.eye_tilt = 20.0;
        finish.body.head_level = 60.0;
        finish.skin.aegyo_sal = 40.0;
        finish.makeup.eyeliner = 50.0;
        finish.hair.gloss = 30.0;
        finish.attire.gold = 70.0;
        finish.scene.saturation = -40.0;
        finish.subject.relight = 30.0;
        finish.faces.push([0.4, 0.3]);
        finish.background = Some(Background {
            mode: BackgroundMode::CloudSky,
            colour: [0.3, 0.5, 0.9],
            colour2: [0.8, 0.9, 1.0],
            amount: 100.0,
            feather: 30.0,
        });
        write(&mut r, &finish).unwrap();
        let back = read(&r).unwrap();
        assert_eq!(back, finish);
        assert!(back.needs_parse() && back.has_measured());
        // A finish stored before ADR-0150 still reads, its new groups untouched.
        let mut r2 = recipe();
        r2.extra.insert(
            KEY.into(),
            serde_json::json!({ "face": { "slim": 10.0 }, "body": { "legs": 5.0 } }),
        );
        let old = read(&r2).unwrap();
        assert!(
            (old.face.slim - 10.0).abs() < 1e-6 && old.skin.is_identity() && old.faces.is_empty()
        );
        // An eyeliner wing alone, or a relight direction alone, changes nothing.
        let mut nothing = StudioFinish::default();
        nothing.makeup.eyeliner_wing = 80.0;
        nothing.subject.relight_direction = 90.0;
        assert!(nothing.is_identity());
    }

    #[test]
    fn the_hundred_additions_refuse_values_out_of_range() {
        let mut finish = StudioFinish::default();
        finish.face.eye_symmetry = -10.0;
        assert!(validate(&finish).is_err());
        let mut finish = StudioFinish::default();
        finish.scene.saturation = -120.0;
        assert!(validate(&finish).is_err());
        let mut finish = StudioFinish::default();
        finish.subject.relight_direction = 200.0;
        assert!(validate(&finish).is_err());
        let finish = StudioFinish {
            faces: vec![[0.5, 1.5]],
            ..StudioFinish::default()
        };
        assert!(validate(&finish).is_err());
        let finish = StudioFinish {
            faces: vec![[0.5, 0.5]; MAX_CHOSEN_FACES + 1],
            ..StudioFinish::default()
        };
        assert!(validate(&finish).is_err());
    }

    #[test]
    fn out_of_range_light_is_refused() {
        let mut finish = StudioFinish::default();
        finish.light.key = 120.0;
        assert!(validate(&finish).is_err());
        let mut finish = StudioFinish::default();
        finish.light.height = 90.0;
        assert!(validate(&finish).is_err());
        let mut finish = StudioFinish::default();
        finish.light.direction = f32::NAN;
        assert!(validate(&finish).is_err());
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

#[cfg(test)]
mod through_merge {
    use super::*;

    fn merged(base: &Recipe, finish: &StudioFinish) -> StudioFinish {
        let mut proposal = base.clone();
        write(&mut proposal, finish).unwrap();
        let (merged, _) = crate::schema::merge(base, &proposal, crate::EditSource::User).unwrap();
        read(&merged).unwrap()
    }

    /// `schema::merge` keeps every leaf a proposal leaves out. Every way back to neutral has to
    /// survive it: a reset, a background removed, strokes cleared, light and new sliders zeroed.
    #[test]
    fn every_reset_survives_the_merge() {
        let mut finish = StudioFinish::default();
        finish.face.slim = 20.0;
        finish.face.eye_tilt = 30.0;
        finish.skin.glow = 40.0;
        finish.light.key = 50.0;
        finish.faces = vec![[0.4, 0.4]];
        finish.liquify.push(LiquifyStroke {
            mode: LiquifyMode::Push,
            radius: 0.05,
            strength: 0.5,
            points: vec![[0.4, 0.4], [0.45, 0.4]],
        });
        finish.background = Some(Background {
            mode: BackgroundMode::Colour,
            colour: [1.0; 3],
            colour2: [1.0; 3],
            amount: 100.0,
            feather: 20.0,
        });
        let base = {
            let r0 = crate::fixtures::reference();
            let mut r = r0.clone();
            write(&mut r, &finish).unwrap();
            crate::schema::merge(&r0, &r, crate::EditSource::User)
                .unwrap()
                .0
        };
        assert_eq!(read(&base).unwrap(), finish);
        assert_eq!(
            merged(&base, &StudioFinish::default()),
            StudioFinish::default()
        );
        let mut partly = finish.clone();
        partly.face.eye_tilt = 0.0;
        partly.skin.glow = 0.0;
        partly.light.key = 0.0;
        partly.background = None;
        partly.liquify.clear();
        partly.faces.clear();
        assert_eq!(merged(&base, &partly), partly);
    }
}
