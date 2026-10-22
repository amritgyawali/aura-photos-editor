//! Versioned local retouch authoring, carried by the recipe extension map. ADR-0073.
use crate::{errors::recipe_invalid, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY: &str = "studio_retouch_v1";
/// Segmentation mattes that retouch operations refer to by id. ADR-0082.
pub const MATTE_KEY: &str = "studio_retouch_mattes_v1";
pub const MAX_EDITS: usize = 1024;
pub const MAX_MATTES: usize = 64;
/// At most this many cells per matte; the renderer refines its edges at full resolution.
/// 512 a side, for the learned masking selections (ADR-0103); the segmenter's own mattes stay
/// at 256.
pub const MAX_MATTE_CELLS: usize = 512 * 512;
pub const MAX_STROKES: usize = 128;
pub const MAX_POINTS: usize = 8192;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrushMask {
    pub strokes: Vec<BrushStroke>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrushStroke {
    pub erase: bool,
    /// Radius relative to the shorter image edge, before pressure.
    pub radius: f32,
    pub opacity: f32,
    /// Normalized x, y, pressure. Pressure changes radius, not repeated opacity.
    pub points: Vec<[f32; 3]>,
}

/// Sample-relative selection; it does not classify people or infer a skin tone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SkinSettings {
    pub tolerance: f32,
    pub edge_protection: f32,
    /// Only change skin connected to the sample without crossing a strong edge, so a
    /// skin-coloured wall, table or backdrop that is not touching the person is never
    /// selected. Absent (false) in recipes written before it existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Gradient {
    pub start: [f32; 2],
    pub end: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LuminanceRange {
    /// Stops relative to 18% linear working luminance, before this operation.
    pub low: f32,
    pub high: f32,
    /// Smooth falloff outside each end of the selected interval, in stops.
    pub softness: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Selection {
    #[serde(default)]
    pub inverted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub luminance: Option<LuminanceRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorRange>,
}

/// Colour measured at this coordinate immediately before the operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ColorRange {
    pub sample: [f32; 2],
    /// Euclidean D65 Lab distance, used only as a selection metric.
    pub tolerance: f32,
    pub softness: f32,
    pub lightness: f32,
}

impl Default for SkinSettings {
    fn default() -> Self {
        Self {
            tolerance: 0.08,
            edge_protection: 0.8,
            connected: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Heal,
    Clone,
    AutoBlemish,
    Frequency,
    MicroDodgeBurn,
    Dodge,
    Burn,
    SkinColor,
    ColorMatch,
    Mattify,
    UnderEye,
    Wrinkle,
    Teeth,
    EyeClean,
    EyeDetail,
    RedEye,
    Fabric,
    /// Selection-limited chroma filtering; original luminance detail is preserved.
    FabricMoire,
    /// Low-frequency fabric correction toward a required clean source sample.
    FabricStain,
    /// Lift a smooth shadow field toward selected upper midtones, bounded by `tone * 2` stops.
    ShadowBalance,
    /// Blur selected background pixels without mixing excluded subjects into the kernel.
    BackgroundBlur,
    /// Spread highlight energy above `0.18 * 2^(tone * 4)` in linear light.
    HighlightGlow,
    /// Selected line integration; `warmth * 180` is direction in degrees.
    MotionBlur,
    /// Radial integration around `source` or the image centre; travel is `radius * 10`.
    ZoomBlur,
    /// Suppress chroma aligned to `target_color`, preserving working luminance.
    ColorDespill,
    /// Continuous luminance shoulder above `0.18 * 2^(tone * 4)`.
    HighlightCompress,
    /// Local chroma gain: `warmth` is saturation, `tint` is adaptive vibrance.
    ColorSaturation,
    Backdrop,
    Glare,
    Makeup,
    /// Set chroma from a chosen sRGB color, preserving the photographed luminance.
    Colorize,
    /// Composite a solid chosen sRGB color inside the saved selection.
    BackgroundColor,
    /// Explicit local width/height change. Warmth/tint are signed horizontal/vertical amounts.
    Reshape,
    SkinSmooth,
    SkinUniformity,
    PortraitDodgeBurn,
    PatchHeal,
    /// Finds compact marks inside the selection and rebuilds the tone under each one from
    /// the clean skin around it, leaving pore-scale detail where it is. ADR-0090.
    FrequencyHeal,
    /// Brings fine texture back up to the selection's own typical level by borrowing real
    /// pore detail from clean skin in the same selection. ADR-0090.
    TextureGraft,
    /// Finds every blemish against a robust estimate of the clean skin around it - marks in a
    /// dense cluster included - and rebuilds the tone and colour under each one, keeping the
    /// pores. With `preserve_microtexture` it also evens flat redness. Without a matte it is
    /// a brush: paint over what is left. ADR-0092.
    AcneClear,
    /// Evens long, faint marks out of an expanse of skin with a filter that flows across
    /// them and stops at structure: a hand resting on an abdomen and a navel are left as
    /// they are. `radius` is the reach of the evening and `texture` how much of the clean
    /// skin's own grain is put back. ADR-0118.
    StretchMarks,
    // Selected effects (ADR-0131). Each reads only the fields its comment names; the warps
    // move pixels, and `amount` is their strength rather than an opacity.
    /// Push the selection's content by (`warmth`, `tint`) region radii, times `amount`.
    LiquifyPush,
    /// Enlarge the content around the region centre (eyes, hair volume).
    LiquifyBloat,
    /// Shrink the content toward the region centre.
    LiquifyPucker,
    /// Rotate the content around the region centre by up to `warmth * 180` degrees.
    LiquifyTwirl,
    /// Rotational blur around `source` or the region centre; `tone` is the sweep (0..30 degrees).
    SpinBlur,
    /// Square blocks `radius` of the short edge across; for hiding a face, plate or screen.
    Pixelate,
    /// A strong even blur of radius `radius`; for hiding detail or softening a background.
    PrivacyBlur,
    /// Diffusion filter: highlights bloom into their surroundings; `tone` is the overall softness.
    SoftFocus,
    /// Orton glow: a brightened, blurred copy screened over the photograph; `tone` brightens it.
    OrtonGlow,
    /// Unsharp mask on luminance: `radius` is the detail size, `texture` the gain.
    LocalSharpen,
    /// Edge-preserving noise reduction: all colour noise, and luminance noise up to `tone`.
    LocalDenoise,
    /// Rotates hue by `warmth * 180` degrees, keeping each pixel's luminance.
    HueShift,
    /// Inverts colours in the display encoding.
    InvertColors,
    /// Replaces the colour sampled at `source` (a green screen) with `target_color`, or removes
    /// its spill when no colour is given; `tone` is the tolerance.
    ChromaKey,
    /// Evens bluish, darker lines (veins) toward the skin around them; `tone` evens the darkness.
    VeinReduce,
    /// Pulls every colour in the selection toward the selection's own average (hair, clothing).
    ColorUnify,
    /// A coloured leak of light from `source` or the region centre, in `target_color` or warm.
    LightLeak,
    /// Channel offsets of `radius` and, with `tone`, horizontal slice displacement.
    Glitch,
    /// Atmospheric haze toward `target_color` or a light grey.
    Haze,
    /// Rays of light from `source` or the region centre; `tone` sets how many.
    Starburst,
    /// White balance for the selection only, from a neutral sampled at `source`.
    LocalWhiteBalance,
    /// A soft light in the eye, the shape of the selection, in `target_color` or white.
    Catchlight,
    /// Darkens glowing pupils of any colour (flash in animal eyes); `tone` is how dark.
    EyeGlowFix,
    /// Lifts the highlights already on the lips; `radius` is the highlight size.
    LipGloss,
    /// Adds deterministic freckles: `tone` is the density, `radius` the size.
    Freckles,
    /// Mirrors what is above the selection's top edge into it, fading over `tone` of its height.
    FloorReflection,
    /// A shadow of the selection offset by (`warmth`, `tint`) region radii, blurred by `radius`.
    DropShadow,
    /// Film halation: a red glow around highlights brighter than `tone` sets.
    Halation,
    // Practical repairs and creative looks (ADR-0132), read the same fields as above.
    /// Replaces specks that differ from the median of `radius` around them by more than `tone`.
    DustScratches,
    /// Smooths small steps at the 8 x 8 block edges of a compressed JPEG; `tone` is the step
    /// size still counted as an artefact, so real edges stay.
    JpegCleanup,
    /// Removes banding from smooth gradients such as skies, with a fine fixed dither.
    Deband,
    /// Stretches the selection's own darkest and brightest tones to a full range.
    AutoLevels,
    /// Removes the selection's own average colour cast (grey world).
    AutoNeutral,
    /// Evens out a gradient of light across the selection, keeping its detail.
    FlattenLight,
    /// Large-scale local contrast in log luminance: `radius` is the structure size,
    /// `texture` the gain, `tone` lifts shadows.
    HdrDetail,
    /// Turns highlights brighter than `tone` sets into round out-of-focus discs of `radius`.
    BokehHighlights,
    /// Miniature look: blur that grows away from a sharp band through the region centre.
    TiltShift,
    /// Maps luminance between a shadow colour and `target_color`; `warmth` turns the shadows.
    Duotone,
    /// Reduces each channel to `2 + tone * 14` levels.
    Posterize,
    /// A rotated print screen of dots `radius` across, in `target_color` or black.
    Halftone,
    /// A pencil drawing made from the photograph; `tone` keeps some of its colour.
    PencilSketch,
    /// Kuwahara oil-paint filter of `radius`.
    OilPaint,
    /// Cross-processed film colour.
    CrossProcess,
    /// Infrared: foliage goes white; `tone` from monochrome to false colour.
    Infrared,
    /// Mirrors one half of the region onto the other: `warmth` chooses the side,
    /// a positive `tint` mirrors top and bottom instead.
    MirrorSymmetry,
    /// Lens flare ghosts on the line from `source` through the frame centre.
    LensFlare,
    /// Rays of light streaming from `source` through the highlights between.
    GodRays,
    /// Falling snow, or rain streaks when `tint` is positive; `tone` is the density.
    Weather,
    // Twenty more selected effects (ADR-0134): colour looks, light and lens, artistic, repair.
    /// Shadows toward a colour turned from `target_color` by `warmth`, highlights toward `target_color`; `tone` is the balance.
    SplitTone,
    /// Faded film: blacks lifted by `tone`, colour muted, a cast of `warmth`.
    FilmFade,
    /// Silver retained: desaturated, with contrast raised by `tone`.
    BleachBypass,
    /// Shadows toward teal and warm tones toward orange by `tone`, skin hues held.
    TealOrange,
    /// Keeps the colour sampled at `source` within `tone` and turns the rest of the selection grey.
    ColourSplash,
    /// Restores blue to a washed-out sky: bright, low-colour or bluish pixels move toward `target_color` or sky blue.
    SkyBlue,
    /// Light falling on `source` or the region centre: lighter inside `radius`, darker away by `tone`.
    Spotlight,
    /// A glow along the selection's own edge on the side facing (`warmth`, `tint`), in `target_color` or warm white.
    RimLight,
    /// Creative lateral colour: red and blue scaled apart around the region centre by `radius`.
    ChromaticAberration,
    /// Maps the ellipse onto a sphere, bulging its centre; `amount` is the curvature.
    Fisheye,
    /// Long horizontal streaks from highlights brighter than `tone` sets, in `target_color` or blue.
    AnamorphicStreak,
    /// Mirrors the region into `4 + tone * 8` segments around its centre.
    Kaleidoscope,
    /// Watercolour: flattened washes, pigment pooled at edges, paper grain.
    Watercolor,
    /// Voronoi cells `radius` across, each one colour, with dark leading between them.
    StainedGlass,
    /// Flat smoothed colours in `2 + tone * 8` levels with ink lines at edges.
    Cartoon,
    /// Edges glow in their own colour over a background darkened by `tone`.
    NeonEdges,
    /// A thermal camera palette mapped from luminance.
    Thermal,
    /// Rebuilds the colour of channels clipped in highlights from the unclipped pixels around them.
    ClippedHighlightRepair,
    /// Dark-channel haze removal measured on the selection; `tone` is how much haze is removed.
    LocalDehaze,
    /// Darkens the iris's outer ring, the band just inside the ellipse; `tone` is its width.
    LimbalRing,
    // Twenty more selected effects (ADR-0135): grooming, colour, distortion, art and light.
    /// Darkens thin dark strokes (lashes, liner) against their surroundings by `amount`.
    LashDarken,
    /// Fills the light gaps between hairs of a brow or beard with the hair's own darker colour; `tone` is how full.
    HairFill,
    /// Adds grain matching the photograph's own measured noise, for a smoothed area that looks too clean; `tone` scales it.
    MatchGrain,
    /// Swaps colour channels; `warmth` chooses which.
    ChannelSwap,
    /// A warm glow falling from the top of the region, in `target_color` or sunset orange.
    SunsetGlow,
    /// Night: darker by `tone`, blue, with the brightest lights kept warm.
    DayToNight,
    /// Lomography: saturated, contrasty, shifted colour and a dark ring at the region edge.
    Lomo,
    /// Instant film: cyan shadows, creamy highlights, soft contrast.
    InstantFilm,
    /// Four bold flat colours by brightness, the first `target_color`.
    PopArt,
    /// Sharp at the region centre, blurring toward its edge; `tone` is the sharp area's size.
    IrisBlur,
    /// A faint copy offset by (`warmth`, `tint`) region radii, screened over the photograph.
    EchoDouble,
    /// Water ripples of wavelength `radius`, fading at the ellipse edge.
    Ripple,
    /// Each pixel scattered by up to `radius`, as through frosted glass.
    FrostedGlass,
    /// Sorts runs of pixels in each row by brightness, between thresholds `tone` sets.
    PixelSort,
    /// VHS: scanlines, colour bleeding sideways, soft and a little noisy.
    VhsRetro,
    /// A relief lit from the top left; `tone` keeps the colour.
    Emboss,
    /// Pen hatching that thickens in the darker tones.
    Crosshatch,
    /// Four-point stars on the brightest points - jewellery, sequins, water.
    Sparkle,
    /// Soft warm orbs of light scattered over the selection; `tone` is the density.
    FairyLights,
    /// A prism rainbow band through the region centre at `warmth` times 180 degrees.
    RainbowPrism,
    /// Removes the purple (`warmth`) and green (`tint`) rims a lens leaves on the dark side of a
    /// bright edge, `radius` from it; each rim pixel takes the colour of its calm surroundings
    /// at its own luminance. A colour its calm surroundings share, or the glow of a coloured
    /// light, is kept. ADR-0125.
    Defringe,
}

impl Tool {
    /// The tools that read a point on the photograph and cannot run without one.
    #[must_use]
    pub fn needs_source(self) -> bool {
        matches!(
            self,
            Self::Clone
                | Self::ColorMatch
                | Self::FabricStain
                | Self::SkinSmooth
                | Self::SkinUniformity
                | Self::PortraitDodgeBurn
                | Self::ChromaKey
                | Self::LocalWhiteBalance
                | Self::ColourSplash
        )
    }

    /// The warps: they move pixels, so an inverted selection would warp the whole frame.
    #[must_use]
    pub fn is_warp(self) -> bool {
        matches!(
            self,
            Self::Reshape
                | Self::LiquifyPush
                | Self::LiquifyBloat
                | Self::LiquifyPucker
                | Self::LiquifyTwirl
                | Self::Fisheye
                | Self::Ripple
                | Self::FrostedGlass
        )
    }
}

// Each flag is an independent opt-in that old recipes read as off; none of them is a state
// of the others.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Edit {
    pub id: String,
    pub tool: Tool,
    pub enabled: bool,
    /// Bounded repeated residual healing. Absent keeps the original two-pass rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_passes: Option<u8>,
    /// Center x/y and ellipse radii x/y in normalized full-frame coordinates.
    pub region: [f32; 4],
    /// Normalized donor/reference point; mandatory for clone and color matching.
    pub source: Option<[f32; 2]>,
    /// Patch-heal donor footprint relative to the target (1 keeps the original scale).
    /// Smaller clean donors can cover dense blemishes; only used with an explicit source.
    #[serde(default = "default_source_scale")]
    pub source_scale: f32,
    pub amount: f32,
    /// Feather fraction: 0 is a hard edge, 1 is fully feathered.
    pub feather: f32,
    /// Frequency radius as a fraction of the image's shorter dimension.
    pub radius: f32,
    /// High-band gain. 1 preserves the original high band.
    pub texture: f32,
    /// Separate fine pores from larger uneven texture during frequency separation.
    /// Off for old recipes; on for the automatic deep skin finish. On [`Tool::AcneClear`] it
    /// also evens flat redness; on [`Tool::TextureGraft`] it borrows no donor texture and leaves
    /// healed marks as they were healed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preserve_microtexture: bool,
    /// Transfer real donor texture over a robust local lighting fit. Old heals stay unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub texture_heal: bool,
    /// Trim neighboring blemishes from donor-heal tone fitting. Old recipes
    /// retain their original fit; this explicit flag versions the new behavior.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clean_ring_fit: bool,
    /// Reconstruct smooth curved illumination under a repair. Default-off keeps
    /// existing flat-lighting heals unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub curved_heal: bool,
    /// Healthy selected-skin lighting samples in target-ellipse coordinates.
    /// Empty preserves the original circular-ring fit of existing recipes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heal_samples: Vec<[f32; 2]>,
    /// Additional measured clean donors for blending fine texture without a
    /// mirrored pore grid. Empty preserves the original single-donor rendering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texture_sources: Vec<[f32; 2]>,
    /// How readily frequency healing calls a compact deviation a mark, 0..=1. Absent means
    /// 0.5. Only read by [`Tool::FrequencyHeal`] and [`Tool::AcneClear`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<f32>,
    /// Frequency healing leaves marks that are darker but not redder than the skin around
    /// them, so a mole or a freckle is never removed by default-off automation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_dark_marks: bool,
    /// Healing leaves pigment: a connected patch whose red channel is under half that of the
    /// skin around it. The value is the step its edge must have, as a share of the skin's red
    /// across one frequency radius, to count. `1` keeps what has a crisp edge - a mole, a
    /// beauty mark - and still heals the diffuse trace of an old spot, which is what a face
    /// wants. `0` keeps every such patch, which is what a body wants: it has no landmarks,
    /// and a navel is pigment-dark with no edge at all. Absent heals pigment like any other
    /// mark, as recipes written before it existed do. Only read by [`Tool::FrequencyHeal`].
    /// ADR-0118.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mole_edge: Option<f32>,
    pub tone: f32,
    pub warmth: f32,
    pub tint: f32,
    /// Display-referred sRGB in 0..=1, only for explicit color tools. Absent on old edits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_color: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<BrushMask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<SkinSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,
    /// A segmentation matte from [`MATTE_KEY`] that limits the operation to, for example,
    /// one person's face skin. Multiplies the region, brush and selection coverage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matte: Option<String>,
}

/// A soft selection measured by the skin segmenter, stored compactly. ADR-0082.
///
/// `data` is base64 of a run-length code: each byte is `(level << 4) | (run - 1)`, where
/// `level` is coverage in fifteenths (0..15, decoded as `level * 17`) and `run` is 1..16 cells
/// in row-major order. Sixteen levels are enough because the renderer re-derives the soft
/// edge from the photograph itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Matte {
    /// Normalized left, top, right, bottom of the grid.
    pub bounds: [f32; 4],
    pub width: u32,
    pub height: u32,
    pub data: String,
    /// Re-detect fine image edges when upsampling. Disable for an already protected
    /// soft surface selection, so blemishes cannot cut holes in their own correction.
    #[serde(default = "default_refine_edges")]
    pub refine_edges: bool,
    /// Contract only the external contour against image edges. Protected holes and
    /// skin texture retain their authored coverage; this mode never adds selection.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refine_contour: bool,
}

fn default_refine_edges() -> bool {
    true
}

fn default_source_scale() -> f32 {
    1.0
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (k, shift) in [18_u32, 12, 6, 0].into_iter().enumerate() {
            if k <= chunk.len() {
                out.push(char::from(
                    BASE64
                        .get(((n >> shift) & 63) as usize)
                        .copied()
                        .unwrap_or(b'A'),
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| -> Option<u32> {
        BASE64
            .iter()
            .position(|v| *v == c)
            .and_then(|p| u32::try_from(p).ok())
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|c| **c == b'=').count();
        if pad > 2 {
            return None;
        }
        let mut n = 0_u32;
        for (k, c) in chunk.iter().enumerate() {
            let v = if k >= 4 - pad { 0 } else { value(*c)? };
            n = (n << 6) | v;
        }
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(decoded.get(..3 - pad)?);
    }
    Some(out)
}

impl Matte {
    /// Encode coverage values (0..=255, row-major, `width * height` of them).
    #[must_use]
    pub fn encode(bounds: [f32; 4], width: u32, height: u32, alpha: &[u8]) -> Self {
        let mut runs: Vec<u8> = Vec::new();
        let mut current: Option<(u8, u8)> = None;
        for value in alpha {
            let level = ((u16::from(*value) * 15 + 127) / 255) as u8;
            current = match current {
                Some((l, run)) if l == level && run < 16 => Some((l, run + 1)),
                Some((l, run)) => {
                    runs.push((l << 4) | (run - 1));
                    Some((level, 1))
                }
                None => Some((level, 1)),
            };
        }
        if let Some((l, run)) = current {
            runs.push((l << 4) | (run - 1));
        }
        Self {
            bounds,
            width,
            height,
            data: base64_encode(&runs),
            refine_edges: true,
            refine_contour: false,
        }
    }

    /// Coverage values 0..=255, row-major, or `None` when the data does not decode to exactly
    /// `width * height` cells.
    #[must_use]
    pub fn decode(&self) -> Option<Vec<u8>> {
        let cells = (self.width as usize).checked_mul(self.height as usize)?;
        if cells == 0 || cells > MAX_MATTE_CELLS {
            return None;
        }
        let mut out = Vec::with_capacity(cells);
        for byte in base64_decode(&self.data)? {
            let level = (byte >> 4) * 17;
            for _ in 0..=(byte & 15) {
                out.push(level);
            }
            if out.len() > cells {
                return None;
            }
        }
        (out.len() == cells).then_some(out)
    }

    fn valid(&self) -> bool {
        let [l, t, r, b] = self.bounds;
        self.bounds
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && r > l
            && b > t
            && self.decode().is_some()
    }
}

fn valid_heal_support(edit: &Edit) -> bool {
    let samples = edit.heal_samples.is_empty()
        || ((58..=224).contains(&edit.heal_samples.len())
            && edit.heal_samples.iter().all(|p| {
                p.iter().all(|v| v.is_finite()) && (1.0..=3.1).contains(&p[0].hypot(p[1]))
            })
            && edit.tool == Tool::PatchHeal
            && edit.texture_heal
            && edit.curved_heal
            && edit.source.is_some());
    let donors = edit.texture_sources.is_empty()
        || ((3..=8).contains(&edit.texture_sources.len())
            && edit
                .texture_sources
                .iter()
                .all(|p| p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
            && edit.tool == Tool::PatchHeal
            && edit.texture_heal
            && edit.source.is_some()
            && edit.source_scale < 1.0);
    samples && donors
}

fn valid_target_color(edit: &Edit) -> bool {
    let Some(color) = edit.target_color else {
        return !matches!(
            edit.tool,
            Tool::Colorize | Tool::BackgroundColor | Tool::ColorDespill
        );
    };
    if !color
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    {
        return false;
    }
    let lo = color.into_iter().fold(f32::INFINITY, f32::min);
    let hi = color.into_iter().fold(f32::NEG_INFINITY, f32::max);
    edit.tool != Tool::ColorDespill || hi - lo >= 0.02
}

/// Validate retouch parameters and aggregate authoring limits.
///
/// # Errors
/// Returns a recipe error for invalid parameters, missing required sources,
/// duplicate identifiers or excessive operations, strokes or points.
pub fn validate(edits: &[Edit]) -> AuraResult<()> {
    if edits.len() > MAX_EDITS {
        return Err(recipe_invalid(KEY, "too many retouch operations"));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut total_points = 0;
    for edit in edits {
        let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        if edit.id.is_empty()
            || edit.id.len() > 100
            || !ids.insert(&edit.id)
            || !edit.region.iter().all(|v| unit(*v))
            || edit.region[2] < 0.001
            || edit.region[3] < 0.001
            || !unit(edit.amount)
            || !unit(edit.feather)
            || !unit(edit.tone)
            || !edit.radius.is_finite()
            || !(0.0005..=0.05).contains(&edit.radius)
            || !edit.texture.is_finite()
            || !(0.0..=2.0).contains(&edit.texture)
            || !edit.warmth.is_finite()
            || !(-1.0..=1.0).contains(&edit.warmth)
            || !edit.tint.is_finite()
            || !(-1.0..=1.0).contains(&edit.tint)
            || edit.source.is_some_and(|p| !p.iter().all(|v| unit(*v)))
            || !valid_heal_support(edit)
            || edit.repair_passes.is_some_and(|n| {
                !(1..=4).contains(&n) || !matches!(edit.tool, Tool::AcneClear | Tool::FrequencyHeal)
            })
            || !valid_target_color(edit)
            || (edit.tool.is_warp() && edit.selection.as_ref().is_some_and(|s| s.inverted))
            || !edit.source_scale.is_finite()
            || !(0.2..=1.0).contains(&edit.source_scale)
            || edit.sensitivity.is_some_and(|v| !unit(v))
            || edit
                .mole_edge
                .is_some_and(|v| !v.is_finite() || !(0.0..=4.0).contains(&v))
            || (edit.source_scale < 1.0 && edit.tool == Tool::PatchHeal && edit.source.is_none())
            || (edit.tool.needs_source() && edit.source.is_none())
        {
            return Err(recipe_invalid(
                KEY,
                "invalid retouch parameters, duplicate ID, or missing source",
            ));
        }
        if edit.skin.is_some_and(|s| {
            !s.tolerance.is_finite()
                || !(0.015..=0.3).contains(&s.tolerance)
                || !unit(s.edge_protection)
        }) {
            return Err(recipe_invalid(KEY, "invalid skin range or edge protection"));
        }
        if edit.tool == Tool::PatchHeal
            && edit.source.is_none()
            && (edit.mask.is_some()
                || edit.region[2] > 0.1
                || edit.region[3] > 0.1
                || edit
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.inverted || s.gradient.is_some()))
        {
            return Err(recipe_invalid(
                KEY,
                "choose a source for painted or large patch repairs",
            ));
        }
        validate_selection(edit)?;
        if let Some(mask) = &edit.mask {
            if mask.strokes.len() > MAX_STROKES {
                return Err(recipe_invalid(KEY, "too many mask strokes"));
            }
            for stroke in &mask.strokes {
                total_points += stroke.points.len();
                if stroke.points.is_empty()
                    || total_points > MAX_POINTS
                    || !stroke.radius.is_finite()
                    || !(0.0005..=0.25).contains(&stroke.radius)
                    || !unit(stroke.opacity)
                    || !stroke.points.iter().flatten().all(|v| unit(*v))
                {
                    return Err(recipe_invalid(KEY, "invalid or oversized brush mask"));
                }
            }
        }
    }
    Ok(())
}

/// Read and validate the optional native retouch extension.
///
/// # Errors
/// Returns a recipe error if the stored extension is malformed or fails validation.
pub fn read(recipe: &Recipe) -> AuraResult<Vec<Edit>> {
    read_mattes(recipe)?;
    let Some(value) = recipe.extra.get(KEY) else {
        return Ok(Vec::new());
    };
    let edits: Vec<Edit> = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(KEY, "invalid retouch operation format"))?;
    validate(&edits)?;
    Ok(edits)
}

/// Read and validate the segmentation mattes retouch operations refer to.
///
/// # Errors
/// Returns a recipe error if the stored mattes are malformed or too many.
pub fn read_mattes(recipe: &Recipe) -> AuraResult<BTreeMap<String, Matte>> {
    let Some(value) = recipe.extra.get(MATTE_KEY) else {
        return Ok(BTreeMap::new());
    };
    let mattes: BTreeMap<String, Matte> = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(MATTE_KEY, "invalid matte format"))?;
    if mattes.len() > MAX_MATTES
        || mattes
            .iter()
            .any(|(id, m)| id.is_empty() || id.len() > 100 || !m.valid())
    {
        return Err(recipe_invalid(MATTE_KEY, "invalid or too many mattes"));
    }
    Ok(mattes)
}

/// Store operations together with the mattes they use. Mattes no operation refers to are
/// dropped, so removing an automatic operation never leaves its selection behind.
///
/// # Errors
/// Returns a recipe error if validation or serialization fails, or an operation refers to
/// a matte that is not given.
pub fn write_with_mattes(
    recipe: &mut Recipe,
    edits: &[Edit],
    mattes: &BTreeMap<String, Matte>,
) -> AuraResult<()> {
    validate(edits)?;
    let used: BTreeMap<String, Matte> = mattes
        .iter()
        .filter(|(id, _)| {
            edits
                .iter()
                .any(|e| e.matte.as_deref() == Some(id.as_str()))
        })
        .map(|(id, m)| (id.clone(), m.clone()))
        .collect();
    if edits
        .iter()
        .filter_map(|e| e.matte.as_deref())
        .any(|id| !used.contains_key(id))
    {
        return Err(recipe_invalid(
            MATTE_KEY,
            "an operation refers to a missing matte",
        ));
    }
    if used.len() > MAX_MATTES || used.values().any(|m| !m.valid()) {
        return Err(recipe_invalid(MATTE_KEY, "invalid or too many mattes"));
    }
    if used.is_empty() {
        recipe.extra.remove(MATTE_KEY);
    } else {
        recipe.extra.insert(
            MATTE_KEY.into(),
            serde_json::to_value(&used)
                .map_err(|_| recipe_invalid(MATTE_KEY, "cannot serialize mattes"))?,
        );
    }
    // Keep an empty array after clearing so merge records the deletion as a user edit.
    recipe.extra.insert(
        KEY.into(),
        serde_json::to_value(edits)
            .map_err(|_| recipe_invalid(KEY, "cannot serialize retouch operations"))?,
    );
    Ok(())
}

/// Store validated native operations without modifying other recipe fields.
///
/// # Errors
/// Returns a recipe error if validation or serialization fails.
pub fn write(recipe: &mut Recipe, edits: &[Edit]) -> AuraResult<()> {
    validate(edits)?;
    // Losing a skin matte must never turn a skin-only operation into a broad brush edit.
    let mattes = read_mattes(recipe)?;
    write_with_mattes(recipe, edits, &mattes)
}

fn validate_selection(edit: &Edit) -> AuraResult<()> {
    let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
    if let Some(selection) = &edit.selection {
        if let Some(g) = &selection.gradient {
            if edit.mask.is_some()
                || !g.start.iter().chain(&g.end).all(|v| unit(*v))
                || (g.start[0] - g.end[0]).hypot(g.start[1] - g.end[1]) < 0.001
            {
                return Err(recipe_invalid(
                    KEY,
                    "invalid gradient or conflicting painted mask",
                ));
            }
        }
        if selection.luminance.as_ref().is_some_and(|r| {
            !r.low.is_finite()
                || !r.high.is_finite()
                || !r.softness.is_finite()
                || !(-16.0..=16.0).contains(&r.low)
                || !(-16.0..=16.0).contains(&r.high)
                || r.low > r.high
                || !(0.0..=4.0).contains(&r.softness)
        }) {
            return Err(recipe_invalid(KEY, "invalid luminance range"));
        }
        if selection.color.as_ref().is_some_and(|r| {
            !r.sample.iter().all(|v| unit(*v))
                || !r.tolerance.is_finite()
                || !(0.0..=100.0).contains(&r.tolerance)
                || !r.softness.is_finite()
                || !(0.0..=100.0).contains(&r.softness)
                || !unit(r.lightness)
        }) {
            return Err(recipe_invalid(KEY, "invalid color range"));
        }
        if edit.tool == Tool::Heal
            && edit.source.is_none()
            && (selection.inverted || selection.gradient.is_some())
        {
            return Err(recipe_invalid(
                KEY,
                "choose a source for gradient or inverted healing",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod matte_tests {
    use super::*;

    #[test]
    fn healthy_context_is_opt_in_bounded_and_invalid_updates_preserve_the_recipe() {
        let mut edit: Edit = serde_json::from_value(serde_json::json!({
            "id":"context", "tool":"patch_heal", "enabled":true,
            "region":[0.5,0.5,0.01,0.01], "source":[0.3,0.3],
            "amount":1.0,"feather":0.25,"radius":0.01,
            "texture":1.0,"tone":0.5,"warmth":0.0,"tint":0.0
        }))
        .unwrap();
        assert!(edit.heal_samples.is_empty());
        assert!(!serde_json::to_string(&edit)
            .unwrap()
            .contains("healSamples"));
        edit.texture_heal = true;
        edit.curved_heal = true;
        edit.heal_samples = (0..96)
            .map(|n| {
                let a = std::f32::consts::TAU * n as f32 / 96.0;
                [2.0 * a.cos(), 2.0 * a.sin()]
            })
            .collect();
        let mut recipe = crate::fixtures::neutral(crate::fixtures::FIXTURE_HASH, "test");
        write(&mut recipe, std::slice::from_ref(&edit)).unwrap();
        assert_eq!(read(&recipe).unwrap(), vec![edit.clone()]);
        let saved = recipe.clone();
        let mut invalid = edit.clone();
        invalid.heal_samples[0] = [f32::NAN, 0.0];
        assert!(write(&mut recipe, &[invalid]).is_err());
        let mut invalid = edit.clone();
        invalid.heal_samples[0] = [0.5, 0.0];
        assert!(write(&mut recipe, &[invalid]).is_err());
        let mut invalid = edit.clone();
        invalid.heal_samples.resize(225, [2.0, 0.0]);
        assert!(write(&mut recipe, &[invalid]).is_err());
        let mut invalid = edit.clone();
        invalid.heal_samples.truncate(57);
        assert!(write(&mut recipe, &[invalid]).is_err());
        let mut invalid = edit.clone();
        invalid.tool = Tool::Dodge;
        assert!(write(&mut recipe, &[invalid]).is_err());
        let mut invalid = edit;
        invalid.source = None;
        assert!(write(&mut recipe, &[invalid]).is_err());
        assert_eq!(recipe.extra, saved.extra);
    }

    #[test]
    fn dense_repair_recipes_round_trip_and_over_budget_writes_preserve_the_recipe() {
        let template: Edit = serde_json::from_value(serde_json::json!({
            "id": "repair", "tool": "dodge", "enabled": true,
            "region": [0.5, 0.5, 0.01, 0.01], "source": null,
            "amount": 0.5, "feather": 0.25, "radius": 0.01,
            "texture": 1.0, "tone": 0.5, "warmth": 0.0, "tint": 0.0
        }))
        .unwrap();
        let mut recipe = crate::fixtures::neutral(crate::fixtures::FIXTURE_HASH, "test");
        let mut edits: Vec<_> = (0..MAX_EDITS)
            .map(|i| Edit {
                id: format!("repair-{i}"),
                ..template.clone()
            })
            .collect();
        write(&mut recipe, &edits).unwrap();
        assert_eq!(read(&recipe).unwrap(), edits);
        let saved = recipe.clone();
        edits.push(Edit {
            id: "one-too-many".into(),
            ..template
        });
        assert!(write(&mut recipe, &edits).is_err());
        assert_eq!(recipe.extra, saved.extra);
    }

    #[test]
    fn writing_a_missing_or_corrupt_matte_never_expands_the_selection() {
        let mut recipe = crate::fixtures::neutral(crate::fixtures::FIXTURE_HASH, "test");
        let edit: Edit = serde_json::from_value(serde_json::json!({
            "id": "skin", "tool": "dodge", "enabled": true,
            "region": [0.5, 0.5, 1.0, 1.0], "source": null,
            "amount": 0.5, "feather": 0.0, "radius": 0.01,
            "texture": 1.0, "tone": 0.5, "warmth": 0.0, "tint": 0.0,
            "matte": "missing-skin"
        }))
        .unwrap();
        let before = recipe.clone();
        assert!(write(&mut recipe, &[edit]).is_err());
        assert_eq!(recipe.extra, before.extra);
        recipe
            .extra
            .insert(MATTE_KEY.into(), serde_json::json!({"broken": 5}));
        assert!(write(&mut recipe, &[]).is_err());
    }

    #[test]
    fn mattes_round_trip_through_sixteen_levels_and_refuse_bad_data() {
        let alpha: Vec<u8> = (0..37 * 23)
            .map(|i| {
                if i % 37 < 10 {
                    0
                } else if i % 37 < 30 {
                    255
                } else {
                    (i * 7 % 256) as u8
                }
            })
            .collect();
        let matte = Matte::encode([0.1, 0.2, 0.6, 0.9], 37, 23, &alpha);
        let decoded = matte.decode().unwrap();
        assert_eq!(decoded.len(), alpha.len());
        for (a, b) in alpha.iter().zip(&decoded) {
            assert!(a.abs_diff(*b) <= 9, "{a} {b}");
        }
        assert!(matte.valid());
        // Long uniform runs compress to a byte per sixteen cells.
        let flat = Matte::encode([0.0, 0.0, 1.0, 1.0], 256, 256, &vec![255; 256 * 256]);
        assert!(flat.data.len() < 6000, "{}", flat.data.len());
        assert_eq!(flat.decode().unwrap().len(), 256 * 256);
        let mut short = matte.clone();
        short.width += 1;
        assert!(short.decode().is_none());
        let mut garbage = matte.clone();
        garbage.data = "!!!!".into();
        assert!(garbage.decode().is_none());
        let mut inverted = matte;
        inverted.bounds = [0.6, 0.2, 0.1, 0.9];
        assert!(!inverted.valid());
    }
}
