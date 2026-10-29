//! The hundred measured checks every photograph is scored on when a collection is culled.
//! ADR-0185.
//!
//! [`crate::measured_cull`] decides which frames are *unusable* - black, blown, out of focus,
//! smeared - with four calibrated rules. Those rules say nothing about which of two usable
//! frames is the better photograph, and a wedding is mostly usable frames. This module is that
//! second question: one hundred named measurements in ten groups of ten, each turned into a
//! score between 0 and 1, so the best frame of a moment can be chosen on evidence a person can
//! read rather than on edge strength alone.
//!
//! Every check is a measurement of the photograph's own pixels, its camera settings or its
//! neighbours in the collection. None is a learned judgement of taste: a check called
//! `faces_smile` measures how wide a mouth is against the eyes above it, and says nothing about
//! whether anybody was happy. The names say what is measured and the documentation for people
//! (`docs/how-aura-culls.md`) says so in the product's voice.
//!
//! Three rules shape the scoring:
//!
//! 1. **A check that cannot be measured abstains.** A landscape has no eyes; a black-and-white
//!    frame has no colour cast. `None` is not a low score, and a group with nothing to say is
//!    left out of the total rather than counted as a failure.
//! 2. **Groups combine as a weighted geometric mean.** A frame whose focus failed is not rescued
//!    by its colour, which is phase 12's fusion rule applied to a hundred signals instead of
//!    four.
//! 3. **Some checks are ranks within the collection.** "Best in the bulk" is relative, and an
//!    absolute threshold for texture or fine detail would be a statement about one camera. A
//!    rank never scores below [`RANK_FLOOR`], so being the softest of a sharp wedding is a
//!    small penalty and never a failure.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    // Analysis coordinates never exceed the analysis edge, so a signed cast cannot wrap.
    clippy::cast_possible_wrap,
    clippy::similar_names,
    // r, g, b, w and h are what pixel arithmetic is written in.
    clippy::many_single_char_names,
    clippy::too_many_lines
)]

use aura_vision::portrait::PortraitFace;
use serde::Serialize;

/// How many checks there are.
pub const COUNT: usize = 100;
/// The first collection check: every check before it is measured from one photograph alone,
/// every check from it onward needs the frames around it.
pub const CONTEXT: usize = 90;
/// Long edge the checks are measured at. Smaller than the hard rules' 1024: these are a
/// hundred passes rather than four, and they are relative to the frame anyway.
const EDGE: usize = 768;
/// The lowest score a collection rank can give.
pub const RANK_FLOOR: f32 = 0.4;
/// Below this mean saturation a frame is monochrome, and its colour checks abstain.
const MONOCHROME: f32 = 0.04;
/// Eyes closer together than this, in analysis pixels, are too small to measure.
const MIN_EYE_DISTANCE: f32 = 12.0;
/// The groups no other group can make up for: a frame that is badly exposed, soft, smeared
/// or blinking is not a good photograph however it is composed.
const CORE: [Category; 4] = [
    Category::Exposure,
    Category::Focus,
    Category::Motion,
    Category::Eyes,
];
/// A core group below this score pulls the total down in proportion.
const CORE_FLOOR: f32 = 0.6;

/// The ten groups a check belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Is the frame exposed so that it can be used?
    Exposure,
    /// Is something in it sharp, and is it the subject?
    Focus,
    /// Did the camera or the subject move?
    Motion,
    /// Noise and the quality of the file.
    Noise,
    /// Colour cast, saturation and skin.
    Colour,
    /// Level horizon, placement, edges and clutter.
    Composition,
    /// The people in the frame.
    Faces,
    /// Their eyes.
    Eyes,
    /// The quality of the light on them.
    Light,
    /// The photograph among its neighbours.
    Context,
}

impl Category {
    /// Every group, in order.
    pub const ALL: [Self; 10] = [
        Self::Exposure,
        Self::Focus,
        Self::Motion,
        Self::Noise,
        Self::Colour,
        Self::Composition,
        Self::Faces,
        Self::Eyes,
        Self::Light,
        Self::Context,
    ];

    /// The group's name, for people.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Exposure => "Exposure",
            Self::Focus => "Focus",
            Self::Motion => "Motion and shake",
            Self::Noise => "Noise and quality",
            Self::Colour => "Colour",
            Self::Composition => "Composition",
            Self::Faces => "Faces",
            Self::Eyes => "Eyes",
            Self::Light => "Light",
            Self::Context => "Among similar photos",
        }
    }

    /// How much the group counts in the total. Focus first, because a soft frame is the one
    /// mistake no edit repairs; noise and light last, because an edit repairs most of both.
    #[must_use]
    pub fn weight(self) -> f32 {
        match self {
            Self::Focus => 2.0,
            Self::Exposure | Self::Motion | Self::Eyes => 1.2,
            Self::Context => 1.1,
            Self::Faces => 1.0,
            Self::Composition => 0.7,
            Self::Colour => 0.6,
            Self::Noise | Self::Light => 0.5,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }
}

/// How a measured value becomes a score between 0 and 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scale {
    /// Higher is better: the first value scores 0, the second 1, linear between.
    Rise(f32, f32),
    /// Lower is better: the first value scores 1, the second 0.
    Fall(f32, f32),
    /// Best anywhere inside `low..=high`, falling to 0 a further `width` outside it.
    Band(f32, f32, f32),
    /// Rank among the collection's own values, higher better, never below [`RANK_FLOOR`].
    Rank,
    /// The same, lower better.
    RankLow,
    /// The value is already a score between 0 and 1.
    Direct,
}

impl Scale {
    /// Score one absolute value. Ranks need the collection and are scored in [`score`].
    #[must_use]
    pub fn absolute(self, value: f32) -> Option<f32> {
        if !value.is_finite() {
            return None;
        }
        Some(match self {
            Self::Rise(bad, good) => ramp(value, bad, good),
            Self::Fall(good, bad) => 1.0 - ramp(value, good, bad),
            Self::Band(low, high, width) => {
                let outside = if value < low {
                    low - value
                } else if value > high {
                    value - high
                } else {
                    0.0
                };
                1.0 - ramp(outside, 0.0, width.max(f32::EPSILON))
            }
            Self::Direct => value.clamp(0.0, 1.0),
            Self::Rank | Self::RankLow => return None,
        })
    }
}

/// `0` at `from`, `1` at `to`, linear between; works in either direction.
fn ramp(value: f32, from: f32, to: f32) -> f32 {
    if (to - from).abs() < f32::EPSILON {
        return if value >= to { 1.0 } else { 0.0 };
    }
    ((value - from) / (to - from)).clamp(0.0, 1.0)
}

/// One check: what it is called, what it measures and how much it counts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Check {
    /// Stable slug; never renamed once shipped.
    pub id: &'static str,
    /// Its group.
    pub category: Category,
    /// What it measures, for people.
    pub name: &'static str,
    /// Its weight inside its group.
    pub weight: f32,
    /// How its value is scored.
    pub scale: Scale,
}

const fn check(
    id: &'static str,
    category: Category,
    name: &'static str,
    weight: f32,
    scale: Scale,
) -> Check {
    Check {
        id,
        category,
        name,
        weight,
        scale,
    }
}

use Category::{Colour, Composition, Context, Exposure, Eyes, Faces, Focus, Light, Motion, Noise};
use Scale::{Band, Direct, Fall, Rank, RankLow, Rise};

/// All one hundred, in the order their values are stored. One line each, so the table reads as one.
#[rustfmt::skip]
pub const CHECKS: [Check; COUNT] = [
    // Exposure: luminance is sRGB-encoded, 0 black to 1 white.
    check("exposure_black_frame", Exposure, "Brightest 1% of the frame is not black", 2.0, Rise(0.05, 0.30)),
    check("exposure_blown_area", Exposure, "Little of the frame is pure white", 2.0, Fall(0.01, 0.20)),
    check("exposure_mid_level", Exposure, "Middle brightness sits in a usable range", 1.0, Band(0.22, 0.65, 0.22)),
    check("exposure_crushed_shadows", Exposure, "Little of the frame is pure black", 1.0, Fall(0.03, 0.30)),
    check("exposure_red_clipping", Exposure, "Red channel is not clipped", 0.5, Fall(0.02, 0.18)),
    check("exposure_green_clipping", Exposure, "Green channel is not clipped", 0.5, Fall(0.01, 0.12)),
    check("exposure_blue_clipping", Exposure, "Blue channel is not clipped", 0.5, Fall(0.01, 0.12)),
    check("exposure_dynamic_range", Exposure, "Uses the range from dark to bright", 0.8, Rise(0.25, 0.70)),
    check("exposure_tonal_spread", Exposure, "Tones are spread rather than bunched", 0.6, Rise(0.50, 0.82)),
    check("exposure_subject_level", Exposure, "The subject is neither too dark nor too bright", 1.5, Band(0.28, 0.75, 0.25)),
    // Focus: edge strength over the frame's own contrast, at the analysis edge.
    check("focus_strongest_edges", Focus, "The strongest edges are crisp", 2.0, Rise(0.07, 0.20)),
    check("focus_peak_edges", Focus, "Something in the frame is very sharp", 1.5, Rise(0.10, 0.30)),
    check("focus_fine_detail", Focus, "Fine detail survives (Laplacian energy)", 1.0, Rank),
    check("focus_centre", Focus, "The centre of the frame is sharp", 0.8, Rise(0.05, 0.16)),
    check("focus_best_region", Focus, "At least one region is sharp", 1.5, Rise(0.07, 0.22)),
    check("focus_sharp_area", Focus, "Share of the frame that is sharp", 0.6, Rise(0.04, 0.30)),
    check("focus_subject_separation", Focus, "The subject is sharper than the background", 0.4, Rise(0.9, 1.4)),
    check("focus_thirds_points", Focus, "The thirds intersections are sharp", 0.5, Rise(0.04, 0.15)),
    check("focus_face", Focus, "The main face is sharp", 2.5, Rise(0.05, 0.16)),
    check("focus_collection_rank", Focus, "Sharpness among this collection's photos", 1.0, Rank),
    // Motion: four edge directions, and the camera's own settings.
    check("motion_weakest_direction", Motion, "No edge direction is smeared away", 2.0, Rise(0.04, 0.12)),
    check("motion_direction_ratio", Motion, "Edges are as strong one way as another", 1.5, Rise(0.40, 0.75)),
    check("motion_horizontal_vertical", Motion, "Horizontal and vertical edges agree", 0.6, Rise(0.30, 0.70)),
    check("motion_diagonals", Motion, "The two diagonals agree", 0.6, Rise(0.40, 0.80)),
    check("motion_handheld_rule", Motion, "Shutter is fast enough for the focal length", 1.0, Fall(1.0, 4.0)),
    check("motion_shutter_speed", Motion, "Shutter is fast enough to freeze people", 0.6, Fall(0.006_25, 0.066_7)),
    check("motion_double_edges", Motion, "No doubled edges from shake", 0.5, Fall(0.25, 0.60)),
    check("motion_face_smear", Motion, "The main face is not smeared", 1.5, Rise(0.35, 0.70)),
    check("motion_fine_detail", Motion, "Fine detail against edge strength", 0.5, Rank),
    check("motion_collection_rank", Motion, "Steadiness among this collection's photos", 0.8, Rank),
    // Noise and file quality.
    check("noise_luma", Noise, "Little brightness noise in smooth areas", 1.5, Fall(0.004, 0.020)),
    check("noise_shadows", Noise, "Little noise in the shadows", 1.0, Fall(0.005, 0.030)),
    check("noise_chroma", Noise, "Little colour noise", 1.0, Fall(0.003, 0.015)),
    check("noise_iso", Noise, "Camera sensitivity (ISO) is moderate", 0.6, Fall(800.0, 6400.0)),
    check("noise_signal_to_noise", Noise, "Signal well above the noise", 1.0, Rise(15.0, 60.0)),
    check("quality_resolution", Noise, "Enough pixels to print", 0.5, Rise(1.0, 6.0)),
    check("quality_posterisation", Noise, "No gaps in the tonal steps (banding)", 0.6, Fall(0.05, 0.35)),
    check("quality_texture", Noise, "Texture richness among this collection's photos", 0.5, Rank),
    check("quality_haze", Noise, "No veil of haze over the frame", 0.6, Fall(0.20, 0.55)),
    check("quality_contrast", Noise, "Overall contrast", 0.8, Rise(0.08, 0.20)),
    // Colour: abstains on a monochrome frame.
    check("colour_cast", Colour, "Neutral mid-tones are not tinted", 1.0, Fall(0.06, 0.25)),
    check("colour_white_cast", Colour, "Bright neutrals are neutral", 1.0, Fall(0.08, 0.30)),
    check("colour_saturation", Colour, "Saturation is natural", 1.0, Band(0.10, 0.55, 0.15)),
    check("colour_oversaturation", Colour, "No colours pushed past saturation", 0.8, Fall(0.01, 0.10)),
    check("colour_colourfulness", Colour, "Colourfulness (Hasler and Suesstrunk)", 0.5, Rise(8.0, 35.0)),
    check("colour_skin_tone", Colour, "Skin sits inside the range skin occupies", 1.5, Direct),
    check("colour_skin_saturation", Colour, "Skin is neither grey nor orange", 1.0, Band(0.12, 0.65, 0.15)),
    check("colour_palette_focus", Colour, "A few colour families rather than many", 0.4, Rise(0.40, 0.85)),
    check("colour_tint", Colour, "No green or magenta tint on neutrals", 0.8, Band(-0.03, 0.03, 0.06)),
    check("colour_shadow_cast", Colour, "Neutral shadows are not tinted", 0.5, Fall(0.12, 0.40)),
    // Composition.
    check("composition_level", Composition, "Strong lines are level or plumb", 0.6, Fall(1.5, 6.0)),
    check("composition_placement", Composition, "Subject on a third or in the centre", 0.8, Fall(0.05, 0.25)),
    check("composition_headroom", Composition, "Room above the main face", 0.8, Band(0.04, 0.40, 0.08)),
    check("composition_faces_cut", Composition, "No face cut by the frame edge", 1.5, Direct),
    check("composition_bright_edges", Composition, "Edges no brighter than the centre", 0.6, Fall(1.15, 1.80)),
    check("composition_balance", Composition, "Detail balanced left and right", 0.3, Rise(0.30, 0.80)),
    check("composition_subject_size", Composition, "The main face is a readable size", 0.5, Band(0.004, 0.30, 0.004)),
    check("composition_clutter", Composition, "Uncluttered edges among this collection's photos", 0.5, RankLow),
    check("composition_dark_corners", Composition, "Corners are not lost to vignetting", 0.5, Rise(0.30, 0.70)),
    check("composition_breathing_space", Composition, "Some quiet space around the subject", 0.3, Band(0.10, 0.70, 0.20)),
    // Faces: only frames with somebody in them.
    check("faces_confidence", Faces, "Faces are clearly visible", 0.8, Rise(0.50, 0.95)),
    check("faces_size", Faces, "The main face is large enough to read", 1.0, Rise(0.001, 0.01)),
    check("faces_all_sharp", Faces, "Every face is as sharp as the sharpest", 1.5, Rise(0.30, 0.70)),
    check("faces_exposure", Faces, "The main face is well exposed", 1.5, Band(0.30, 0.75, 0.20)),
    check("faces_even_light", Faces, "Both sides of the face are lit", 0.4, Rise(0.40, 0.85)),
    check("faces_frontal", Faces, "Faces turned towards the camera", 0.6, Fall(0.25, 0.70)),
    check("faces_smile", Faces, "Mouth width against the eyes (a smile measure)", 0.5, Rise(0.68, 0.88)),
    check("faces_teeth", Faces, "Teeth visible (a smile measure)", 0.2, Rise(0.0, 0.10)),
    check("faces_inside_frame", Faces, "Faces fully inside the frame", 1.0, Rise(0.50, 1.0)),
    check("faces_head_level", Faces, "Heads not strongly tilted", 0.3, Fall(10.0, 35.0)),
    // Eyes.
    check("eyes_open", Eyes, "Eyes are open (contrast against the skin)", 2.5, Rise(0.20, 0.50)),
    check("eyes_found", Eyes, "Both eyes found inside each face", 0.6, Direct),
    check("eyes_sharp", Eyes, "The eyes are sharp", 2.0, Rise(0.03, 0.10)),
    check("eyes_catchlight", Eyes, "Light reflected in the eyes", 0.6, Rise(0.05, 0.20)),
    check("eyes_red_eye", Eyes, "No red-eye from the flash", 1.0, Fall(0.15, 0.35)),
    check("eyes_match", Eyes, "Both eyes equally open (no wink)", 1.2, Rise(0.25, 0.60)),
    check("eyes_whites", Eyes, "The whites of the eyes show", 0.3, Rise(0.0, 0.01)),
    check("eyes_not_shadowed", Eyes, "Eyes are not lost in shadow", 0.8, Rise(0.35, 0.65)),
    check("eyes_gaze", Eyes, "Looking towards the camera", 0.2, Fall(0.50, 0.90)),
    check("eyes_spacing", Eyes, "Eye positions are reliable", 0.4, Band(0.28, 0.55, 0.10)),
    // Light.
    check("light_face_modelling", Light, "Shape on the face from light and shade", 0.6, Band(1.15, 3.0, 1.5)),
    check("light_hard_shadows", Light, "No harsh shadow edges", 0.6, Fall(0.005, 0.04)),
    check("light_flash_falloff", Light, "Flash did not leave the background black", 0.6, Fall(1.5, 4.0)),
    check("light_mixed_colour", Light, "One colour of light rather than several", 0.3, Fall(0.15, 0.45)),
    check("light_backlit_subject", Light, "The subject is not dark against the background", 1.2, Rise(0.45, 0.80)),
    check("light_face_shine", Light, "No shine burnt out on the face", 0.8, Fall(0.005, 0.05)),
    check("light_highlight_rolloff", Light, "Highlights roll off rather than clip", 0.6, Rise(0.20, 0.60)),
    check("light_face_contrast", Light, "Face contrast against the scene", 0.4, Band(0.30, 1.20, 0.30)),
    check("light_ambient_level", Light, "Enough light in the scene", 0.6, Band(0.18, 0.70, 0.18)),
    check("light_separation", Light, "Subject stands out from the background", 0.6, Rise(0.01, 0.08)),
    // Among similar photos: filled by `measured_cull::decide`, which sees the neighbours.
    check("context_burst_rank", Context, "Place in its burst", 1.5, Direct),
    check("context_moment_rank", Context, "Place among similar shots of the moment", 1.5, Direct),
    check("context_eyes_vs_similar", Context, "Eyes as open as the best similar shot", 1.5, Rise(0.60, 0.95)),
    check("context_sharpest_of_similar", Context, "As sharp as the sharpest similar shot", 1.5, Rise(0.60, 0.95)),
    check("context_uniqueness", Context, "Different from the photos around it", 1.0, Rise(4.0, 16.0)),
    check("context_new_scene", Context, "Opens a new scene", 0.5, Direct),
    check("context_exposure_fit", Context, "Exposure matches its neighbours", 0.6, Fall(0.05, 0.25)),
    check("context_colour_fit", Context, "Colour matches its neighbours", 0.6, Fall(0.04, 0.15)),
    check("context_deliberate", Context, "A deliberate shot rather than a spray", 0.4, Rise(0.5, 5.0)),
    check("context_collection_rank", Context, "Overall quality among this collection's photos", 1.0, Rank),
];

/// The camera settings a check can read, from the catalog.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Settings {
    /// Sensitivity.
    pub iso: Option<f32>,
    /// Exposure time in seconds.
    pub shutter_s: Option<f32>,
    /// Focal length in millimetres, as recorded (not 35 mm equivalent).
    pub focal_mm: Option<f32>,
    /// Whether the flash fired.
    pub flash: Option<bool>,
    /// The original's size in megapixels.
    pub megapixels: Option<f32>,
}

/// Everything one photograph's checks are measured from.
#[derive(Debug, Clone, Copy)]
pub struct Sample<'a> {
    /// 8-bit sRGB, row-major.
    pub rgb: &'a [u8],
    /// Pixel width of `rgb`.
    pub width: usize,
    /// Pixel height of `rgb`.
    pub height: usize,
    /// Faces, normalised to the frame.
    pub faces: &'a [PortraitFace],
    /// Camera settings.
    pub settings: Settings,
}

/// What a frame's checks scored.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scored {
    /// One score per check, `0..=100`; `None` when the check abstained.
    pub checks: Vec<Option<u8>>,
    /// One score per group, `0..=100`; `None` when every check in it abstained.
    pub categories: Vec<Option<u8>>,
    /// The total, `0..=100`.
    pub overall: f32,
}

// ----------------------------------------------------------------------------------------
// Pixels.
// ----------------------------------------------------------------------------------------

fn px(plane: &[f32], index: usize) -> f32 {
    plane.get(index).copied().unwrap_or_default()
}

/// A pixel rectangle at the analysis scale, right and bottom exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

impl Rect {
    fn usable(self) -> bool {
        self.right > self.left + 2 && self.bottom > self.top + 2
    }
}

/// The frame at the analysis scale, with the planes every check shares.
struct Planes {
    width: usize,
    height: usize,
    luma: Vec<f32>,
    red: Vec<f32>,
    green: Vec<f32>,
    blue: Vec<f32>,
    /// HSV saturation.
    saturation: Vec<f32>,
    /// Luminance after a 3x3 binomial smooth, so sensor noise is not read as detail.
    smooth: Vec<f32>,
    /// Gradients of `smooth`: horizontal, vertical, two diagonals and magnitude.
    gx: Vec<f32>,
    gy: Vec<f32>,
    d1: Vec<f32>,
    d2: Vec<f32>,
    magnitude: Vec<f32>,
    /// The frame's own contrast, p99 minus p1, floored.
    contrast: f32,
}

impl Planes {
    fn build(rgb: &[u8], width: usize, height: usize) -> Option<Self> {
        if width == 0 || height == 0 || rgb.len() < width * height * 3 {
            return None;
        }
        let factor = width.max(height).div_ceil(EDGE).max(1);
        let (w, h) = (width / factor, height / factor);
        if w < 16 || h < 16 {
            return None;
        }
        let (mut red, mut green, mut blue) =
            (vec![0.0_f32; w * h], vec![0.0; w * h], vec![0.0; w * h]);
        for (row_index, row) in rgb.chunks_exact(width * 3).take(height).enumerate() {
            let y = row_index / factor;
            if y >= h {
                break;
            }
            for (column, pixel) in row.chunks_exact(3).enumerate() {
                let x = column / factor;
                if x >= w {
                    break;
                }
                let index = y * w + x;
                if let (Some(r), Some(g), Some(b), [pr, pg, pb]) = (
                    red.get_mut(index),
                    green.get_mut(index),
                    blue.get_mut(index),
                    pixel,
                ) {
                    *r += f32::from(*pr);
                    *g += f32::from(*pg);
                    *b += f32::from(*pb);
                }
            }
        }
        let scale = 1.0 / (255.0 * (factor * factor) as f32);
        for plane in [&mut red, &mut green, &mut blue] {
            for value in plane.iter_mut() {
                *value *= scale;
            }
        }
        let mut luma = Vec::with_capacity(w * h);
        let mut saturation = Vec::with_capacity(w * h);
        for ((r, g), b) in red.iter().zip(&green).zip(&blue) {
            luma.push(0.2126 * r + 0.7152 * g + 0.0722 * b);
            let (high, low) = (r.max(*g).max(*b), r.min(*g).min(*b));
            saturation.push(if high > 1e-4 {
                (high - low) / high
            } else {
                0.0
            });
        }
        let mut planes = Self {
            width: w,
            height: h,
            smooth: Vec::new(),
            gx: Vec::new(),
            gy: Vec::new(),
            d1: Vec::new(),
            d2: Vec::new(),
            magnitude: Vec::new(),
            contrast: 0.05,
            luma,
            red,
            green,
            blue,
            saturation,
        };
        planes.smooth = planes.smoothed(&planes.luma);
        planes.gradients();
        let mut sample = planes.luma.clone();
        planes.contrast = (quantile(&mut sample, 0.99) - quantile(&mut sample, 0.01)).max(0.05);
        Some(planes)
    }

    /// Clamped read: the border repeats.
    fn at(&self, plane: &[f32], x: isize, y: isize) -> f32 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        px(plane, y * self.width + x)
    }

    fn smoothed(&self, plane: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(plane.len());
        for y in 0..self.height as isize {
            for x in 0..self.width as isize {
                let mut sum = 0.0;
                for (dy, row_weight) in [(-1, 1.0), (0, 2.0), (1, 1.0)] {
                    for (dx, weight) in [(-1, 1.0), (0, 2.0), (1, 1.0)] {
                        sum += self.at(plane, x + dx, y + dy) * row_weight * weight;
                    }
                }
                out.push(sum / 16.0);
            }
        }
        out
    }

    fn gradients(&mut self) {
        let n = self.width * self.height;
        let diagonal = 2.0 * std::f32::consts::SQRT_2;
        let (mut gx, mut gy, mut d1, mut d2, mut magnitude) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        let smooth = &self.smooth;
        for y in 0..self.height as isize {
            for x in 0..self.width as isize {
                let h = (self.at(smooth, x + 1, y) - self.at(smooth, x - 1, y)) / 2.0;
                let v = (self.at(smooth, x, y + 1) - self.at(smooth, x, y - 1)) / 2.0;
                let a = (self.at(smooth, x + 1, y + 1) - self.at(smooth, x - 1, y - 1)) / diagonal;
                let b = (self.at(smooth, x - 1, y + 1) - self.at(smooth, x + 1, y - 1)) / diagonal;
                gx.push(h);
                gy.push(v);
                d1.push(a);
                d2.push(b);
                magnitude.push(h.hypot(v));
            }
        }
        (self.gx, self.gy, self.d1, self.d2, self.magnitude) = (gx, gy, d1, d2, magnitude);
    }

    /// A rectangle from fractions of the frame.
    fn rect(&self, left: f32, top: f32, right: f32, bottom: f32) -> Rect {
        let (w, h) = (self.width as f32, self.height as f32);
        Rect {
            left: (left.clamp(0.0, 1.0) * w) as usize,
            top: (top.clamp(0.0, 1.0) * h) as usize,
            right: ((right.clamp(0.0, 1.0) * w) as usize).min(self.width),
            bottom: ((bottom.clamp(0.0, 1.0) * h) as usize).min(self.height),
        }
    }

    /// A rectangle centred on a point in analysis pixels.
    fn around(&self, x: f32, y: f32, half_width: f32, half_height: f32) -> Rect {
        Rect {
            left: (x - half_width).max(0.0) as usize,
            top: (y - half_height).max(0.0) as usize,
            right: ((x + half_width).max(0.0) as usize).min(self.width),
            bottom: ((y + half_height).max(0.0) as usize).min(self.height),
        }
    }

    fn values(&self, plane: &[f32], rect: Rect) -> Vec<f32> {
        let mut out = Vec::new();
        for y in rect.top..rect.bottom.min(self.height) {
            let start = y * self.width;
            if let Some(row) = plane.get(start + rect.left..start + rect.right.min(self.width)) {
                out.extend_from_slice(row);
            }
        }
        out
    }

    fn mean(&self, plane: &[f32], rect: Rect) -> Option<f32> {
        let values = self.values(plane, rect);
        mean(&values)
    }

    fn spread(&self, plane: &[f32], rect: Rect) -> Option<f32> {
        spread(&self.values(plane, rect))
    }

    /// An edge-strength percentile inside a rectangle, over the frame's contrast.
    fn edges(&self, rect: Rect, fraction: f32) -> Option<f32> {
        if !rect.usable() {
            return None;
        }
        let mut values = self.values(&self.magnitude, rect);
        (!values.is_empty()).then(|| quantile(&mut values, fraction) / self.contrast)
    }

    /// The frame cut into `n` by `n` cells.
    fn grid(&self, n: usize) -> Vec<Rect> {
        let mut cells = Vec::with_capacity(n * n);
        for row in 0..n {
            for column in 0..n {
                cells.push(Rect {
                    left: column * self.width / n,
                    top: row * self.height / n,
                    right: (column + 1) * self.width / n,
                    bottom: (row + 1) * self.height / n,
                });
            }
        }
        cells
    }
}

fn mean(values: &[f32]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}

fn spread(values: &[f32]) -> Option<f32> {
    let m = mean(values)?;
    let variance = values.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / values.len() as f32;
    Some(variance.max(0.0).sqrt())
}

/// A quantile, by partial selection; reorders `values`.
fn quantile(values: &mut [f32], fraction: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() - 1) as f32 * fraction.clamp(0.0, 1.0)).round() as usize;
    let (_, value, _) = values.select_nth_unstable_by(index, f32::total_cmp);
    *value
}

fn fraction(values: &[f32], test: impl Fn(f32) -> bool) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().filter(|v| test(**v)).count() as f32 / values.len() as f32
}

/// A face at the analysis scale.
#[derive(Debug, Clone, Copy)]
struct Face {
    rect: Rect,
    /// Normalised area of the face box.
    area: f32,
    eyes: [(f32, f32); 2],
    nose: (f32, f32),
    mouth: [(f32, f32); 2],
    eye_distance: f32,
    confidence: f32,
    /// The box touches the frame edge.
    cut: bool,
}

fn faces_at(planes: &Planes, faces: &[PortraitFace]) -> Vec<Face> {
    let (w, h) = (planes.width as f32, planes.height as f32);
    let mut out: Vec<Face> = faces
        .iter()
        .filter_map(|face| {
            let [left, top, right, bottom] = face.bounds;
            let rect = planes.rect(left, top, right, bottom);
            if !rect.usable() || rect.right < rect.left + 6 || rect.bottom < rect.top + 6 {
                return None;
            }
            let point = |index: usize| {
                face.landmarks
                    .get(index)
                    .map_or((0.0, 0.0), |p| (p[0] * w, p[1] * h))
            };
            let eyes = [point(0), point(1)];
            let margin = 0.01;
            Some(Face {
                rect,
                area: (right - left).max(0.0) * (bottom - top).max(0.0),
                eye_distance: (eyes[0].0 - eyes[1].0).hypot(eyes[0].1 - eyes[1].1),
                eyes,
                nose: point(2),
                mouth: [point(3), point(4)],
                confidence: face.confidence,
                cut: left < margin || top < margin || right > 1.0 - margin || bottom > 1.0 - margin,
            })
        })
        .collect();
    out.sort_by(|a, b| b.area.total_cmp(&a.area));
    out.truncate(4);
    out
}

// ----------------------------------------------------------------------------------------
// The ninety per-photograph checks.
// ----------------------------------------------------------------------------------------

/// Measure the ninety checks one photograph can answer alone. The ten collection checks are
/// left `None` for [`crate::measured_cull::decide`] to fill. `None` for a frame too small to
/// judge.
#[must_use]
pub fn measure(sample: &Sample<'_>) -> Option<Vec<Option<f32>>> {
    let planes = Planes::build(sample.rgb, sample.width, sample.height)?;
    let faces = faces_at(&planes, sample.faces);
    let mut values = Vec::with_capacity(COUNT);
    values.extend(exposure(&planes, &faces));
    values.extend(focus(&planes, &faces));
    values.extend(motion(&planes, &faces, &sample.settings));
    values.extend(noise(&planes, sample, &sample.settings));
    values.extend(colour(&planes, &faces));
    values.extend(composition(&planes, &faces));
    values.extend(people(&planes, &faces));
    values.extend(eyes(&planes, &faces, &sample.settings));
    values.extend(light(&planes, &faces, &sample.settings));
    values.resize(COUNT, None);
    Some(values)
}

/// The subject: the main face when there is one, otherwise the centre third.
fn subject(planes: &Planes, faces: &[Face]) -> Rect {
    faces.first().map_or_else(
        || planes.rect(1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0),
        |f| f.rect,
    )
}

fn exposure(planes: &Planes, faces: &[Face]) -> [Option<f32>; 10] {
    let mut luma = planes.luma.clone();
    let p99 = quantile(&mut luma, 0.99);
    let p1 = quantile(&mut luma, 0.01);
    let median = quantile(&mut luma, 0.5);
    let clipped = |plane: &[f32]| fraction(plane, |v| v > 0.99);
    let mut bins = [0_u32; 64];
    for value in &planes.luma {
        if let Some(bin) = bins.get_mut(((value.clamp(0.0, 1.0)) * 63.0) as usize) {
            *bin += 1;
        }
    }
    let total = planes.luma.len().max(1) as f32;
    let entropy: f32 = bins
        .iter()
        .filter(|count| **count > 0)
        .map(|count| {
            let p = *count as f32 / total;
            -p * p.log2()
        })
        .sum::<f32>()
        / 6.0;
    [
        Some(p99),
        Some(fraction(&planes.luma, |v| v > 0.98)),
        Some(median),
        Some(fraction(&planes.luma, |v| v < 0.02)),
        Some(clipped(&planes.red)),
        Some(clipped(&planes.green)),
        Some(clipped(&planes.blue)),
        Some(p99 - p1),
        Some(entropy),
        planes.mean(&planes.luma, subject(planes, faces)),
    ]
}

fn focus(planes: &Planes, faces: &[Face]) -> [Option<f32>; 10] {
    let all = planes.rect(0.0, 0.0, 1.0, 1.0);
    let cells = planes.grid(4);
    let per_cell: Vec<f32> = cells
        .iter()
        .filter_map(|c| planes.edges(*c, 0.99))
        .collect();
    let best = per_cell.iter().copied().fold(0.0_f32, f32::max);
    let mut sorted = per_cell.clone();
    let median_cell = quantile(&mut sorted, 0.5);
    let fine = planes.grid(8);
    let sharp_cells = fine
        .iter()
        .filter(|c| planes.edges(**c, 0.95).is_some_and(|v| v > 0.08))
        .count() as f32
        / fine.len().max(1) as f32;
    let mut laplacian = Vec::with_capacity(planes.luma.len() / 4);
    for y in (1..planes.height.saturating_sub(1)).step_by(2) {
        for x in (1..planes.width.saturating_sub(1)).step_by(2) {
            let (x, y) = (x as isize, y as isize);
            let l = &planes.luma;
            let value = planes.at(l, x + 1, y)
                + planes.at(l, x - 1, y)
                + planes.at(l, x, y + 1)
                + planes.at(l, x, y - 1)
                - 4.0 * planes.at(l, x, y);
            laplacian.push(value / planes.contrast);
        }
    }
    let thirds: Vec<f32> = [(1.0, 1.0), (2.0, 1.0), (1.0, 2.0), (2.0, 2.0)]
        .iter()
        .filter_map(|(cx, cy)| {
            let (x, y) = (cx / 3.0, cy / 3.0);
            planes.edges(planes.rect(x - 0.08, y - 0.08, x + 0.08, y + 0.08), 0.99)
        })
        .collect();
    let strongest = planes.edges(all, 0.999);
    [
        strongest,
        planes.edges(all, 0.9999),
        spread(&laplacian),
        planes.edges(
            planes.rect(1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0),
            0.99,
        ),
        (best > 0.0).then_some(best),
        Some(sharp_cells),
        (median_cell > 1e-4).then(|| best / median_cell),
        mean(&thirds),
        faces.first().and_then(|f| planes.edges(f.rect, 0.99)),
        strongest,
    ]
}

fn direction_strengths(planes: &Planes, rect: Rect) -> Option<[f32; 4]> {
    if !rect.usable() {
        return None;
    }
    let mut out = [0.0_f32; 4];
    for (slot, plane) in out
        .iter_mut()
        .zip([&planes.gx, &planes.gy, &planes.d1, &planes.d2])
    {
        let mut values: Vec<f32> = planes.values(plane, rect).iter().map(|v| v.abs()).collect();
        *slot = quantile(&mut values, 0.999) / planes.contrast;
    }
    Some(out)
}

fn ratio(a: f32, b: f32) -> Option<f32> {
    let high = a.max(b);
    (high > 1e-5).then(|| a.min(b) / high)
}

/// The strongest secondary echo of a gradient along rows and columns: a frame shaken during
/// the exposure carries every edge twice, a few pixels apart.
fn double_edges(planes: &Planes) -> f32 {
    let lags = 4..=14_usize;
    let mut best = 0.0_f32;
    for (plane, along_rows) in [(&planes.gx, true), (&planes.gy, false)] {
        let (lines, length) = if along_rows {
            (planes.height, planes.width)
        } else {
            (planes.width, planes.height)
        };
        let read = |line: usize, i: usize| {
            if along_rows {
                px(plane, line * planes.width + i)
            } else {
                px(plane, i * planes.width + line)
            }
        };
        let mut sums = vec![0.0_f64; lags.clone().count()];
        let mut energy = 0.0_f64;
        for line in (0..lines).step_by(4) {
            for i in 0..length {
                let a = f64::from(read(line, i));
                energy += a * a;
                for (slot, lag) in sums.iter_mut().zip(lags.clone()) {
                    if i + lag < length {
                        *slot += a * f64::from(read(line, i + lag));
                    }
                }
            }
        }
        if energy > 1e-9 {
            for sum in sums {
                best = best.max((sum / energy) as f32);
            }
        }
    }
    best
}

fn motion(planes: &Planes, faces: &[Face], settings: &Settings) -> [Option<f32>; 10] {
    let all = planes.rect(0.0, 0.0, 1.0, 1.0);
    let directions = direction_strengths(planes, all);
    let weakest = directions.map(|d| d.iter().copied().fold(f32::INFINITY, f32::min));
    let strongest = directions.map(|d| d.iter().copied().fold(0.0_f32, f32::max));
    let flash = settings.flash == Some(true);
    let face_ratio = faces.first().and_then(|f| {
        let d = direction_strengths(planes, f.rect)?;
        let low = d.iter().copied().fold(f32::INFINITY, f32::min);
        let high = d.iter().copied().fold(0.0_f32, f32::max);
        (high > 1e-5).then(|| low / high)
    });
    let laplacian_mean = mean(
        &planes
            .smooth
            .iter()
            .enumerate()
            .step_by(3)
            .map(|(i, _)| {
                let (x, y) = ((i % planes.width) as isize, (i / planes.width) as isize);
                let s = &planes.smooth;
                (planes.at(s, x + 1, y)
                    + planes.at(s, x - 1, y)
                    + planes.at(s, x, y + 1)
                    + planes.at(s, x, y - 1)
                    - 4.0 * planes.at(s, x, y))
                .abs()
            })
            .collect::<Vec<_>>(),
    );
    let gradient_mean = mean(&planes.magnitude);
    [
        weakest,
        weakest
            .zip(strongest)
            .and_then(|(low, high)| ratio(low, high)),
        directions.and_then(|d| ratio(d[0], d[1])),
        directions.and_then(|d| ratio(d[2], d[3])),
        if flash {
            Some(0.5)
        } else {
            settings
                .shutter_s
                .zip(settings.focal_mm)
                .map(|(s, f)| s * f)
        },
        if flash { None } else { settings.shutter_s },
        Some(double_edges(planes)),
        face_ratio,
        laplacian_mean
            .zip(gradient_mean)
            .and_then(|(l, g)| (g > 1e-6).then(|| l / g)),
        weakest,
    ]
}

/// Immerkaer's fast noise estimate over the given pixels: the image convolved with a mask
/// that cancels every smooth surface, so what is left is noise.
fn noise_sigma(planes: &Planes, plane: &[f32], keep: &dyn Fn(usize) -> bool) -> Option<f32> {
    let mut sum = 0.0_f64;
    let mut count = 0_u64;
    for y in 1..planes.height.saturating_sub(1) {
        for x in 1..planes.width.saturating_sub(1) {
            let index = y * planes.width + x;
            if !keep(index) {
                continue;
            }
            let (xi, yi) = (x as isize, y as isize);
            let at = |dx: isize, dy: isize| f64::from(planes.at(plane, xi + dx, yi + dy));
            let value = at(-1, -1) - 2.0 * at(0, -1) + at(1, -1) - 2.0 * at(-1, 0) + 4.0 * at(0, 0)
                - 2.0 * at(1, 0)
                + at(-1, 1)
                - 2.0 * at(0, 1)
                + at(1, 1);
            sum += value.abs();
            count += 1;
        }
    }
    (count > 200).then(|| ((sum / count as f64) * (std::f64::consts::PI / 2.0).sqrt() / 6.0) as f32)
}

fn noise(planes: &Planes, sample: &Sample<'_>, settings: &Settings) -> [Option<f32>; 10] {
    let mut magnitudes = planes.magnitude.clone();
    let flat_limit = quantile(&mut magnitudes, 0.5);
    let flat = |i: usize| px(&planes.magnitude, i) <= flat_limit;
    let luma_noise = noise_sigma(planes, &planes.luma, &flat);
    let shadow_noise = noise_sigma(planes, &planes.luma, &|i| {
        flat(i) && px(&planes.luma, i) < 0.3
    });
    let rg: Vec<f32> = planes
        .red
        .iter()
        .zip(&planes.green)
        .map(|(r, g)| r - g)
        .collect();
    let bg: Vec<f32> = planes
        .blue
        .iter()
        .zip(&planes.green)
        .map(|(b, g)| b - g)
        .collect();
    let chroma = noise_sigma(planes, &rg, &flat)
        .zip(noise_sigma(planes, &bg, &flat))
        .map(|(a, b)| f32::midpoint(a, b));
    let mut luma = planes.luma.clone();
    let median = quantile(&mut luma, 0.5);
    // Banding: gaps between the 8-bit steps the camera's own file uses, read before any
    // averaging, which would fill them in.
    let mut bins = [0_u32; 256];
    let step = (sample.rgb.len() / 3 / 200_000).max(1);
    for pixel in sample.rgb.chunks_exact(3).step_by(step) {
        if let Some(slot) = pixel.get(1).and_then(|g| bins.get_mut(usize::from(*g))) {
            *slot += 1;
        }
    }
    let occupied: Vec<usize> = bins
        .iter()
        .enumerate()
        .filter(|(_, c)| **c > 0)
        .map(|(i, _)| i)
        .collect();
    let posterisation = match (occupied.first(), occupied.last()) {
        (Some(low), Some(high)) if high > low => {
            let span = high - low + 1;
            Some((span - occupied.len()) as f32 / span as f32)
        }
        _ => None,
    };
    let texture: Vec<f32> = planes
        .grid(16)
        .iter()
        .filter_map(|c| planes.spread(&planes.luma, *c))
        .collect();
    let haze: Vec<f32> = planes
        .grid(24)
        .iter()
        .map(|cell| {
            let reds = planes.values(&planes.red, *cell);
            let greens = planes.values(&planes.green, *cell);
            let blues = planes.values(&planes.blue, *cell);
            reds.iter()
                .zip(&greens)
                .zip(&blues)
                .map(|((r, g), b)| r.min(*g).min(*b))
                .fold(1.0_f32, f32::min)
        })
        .collect();
    [
        luma_noise,
        shadow_noise,
        chroma,
        settings.iso,
        luma_noise.and_then(|n| (n > 1e-5).then(|| median / n)),
        settings.megapixels,
        posterisation,
        mean(&texture).map(|t| t / planes.contrast),
        mean(&haze),
        spread(&planes.luma),
    ]
}

/// How far an average colour is from neutral, as the largest channel difference over the
/// mean.
fn cast(r: f32, g: f32, b: f32) -> Option<f32> {
    let m = (r + g + b) / 3.0;
    (m > 1e-3).then(|| (r - g).abs().max((b - g).abs()).max((r - b).abs()) / m)
}

fn mean_rgb(planes: &Planes, keep: impl Fn(usize) -> bool) -> Option<(f32, f32, f32)> {
    let (mut r, mut g, mut b, mut n) = (0.0_f64, 0.0_f64, 0.0_f64, 0_u64);
    for i in 0..planes.luma.len() {
        if keep(i) {
            r += f64::from(px(&planes.red, i));
            g += f64::from(px(&planes.green, i));
            b += f64::from(px(&planes.blue, i));
            n += 1;
        }
    }
    (n > 50).then(|| {
        let n = n as f64;
        ((r / n) as f32, (g / n) as f32, (b / n) as f32)
    })
}

fn hue(r: f32, g: f32, b: f32) -> f32 {
    let (high, low) = (r.max(g).max(b), r.min(g).min(b));
    let chroma = high - low;
    if chroma < 1e-5 {
        return 0.0;
    }
    let h = if (high - r).abs() < f32::EPSILON {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if (high - g).abs() < f32::EPSILON {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    };
    h * 60.0
}

/// The face's centre half, where skin is and hair and background are not.
fn skin_rect(face: &Face) -> Rect {
    let (w, h) = (
        face.rect.right.saturating_sub(face.rect.left),
        face.rect.bottom.saturating_sub(face.rect.top),
    );
    Rect {
        left: face.rect.left + w / 4,
        top: face.rect.top + h / 4,
        right: face.rect.right.saturating_sub(w / 4),
        bottom: face.rect.bottom.saturating_sub(h / 4),
    }
}

fn colour(planes: &Planes, faces: &[Face]) -> [Option<f32>; 10] {
    let saturation = mean(&planes.saturation).unwrap_or_default();
    if saturation < MONOCHROME {
        return [None; 10];
    }
    let luma = &planes.luma;
    // A cast is read on what should be neutral. Over every pixel it reads the scene: a red
    // and gold mandap measured as a 40% cast on the first calibration run.
    let neutral = |i: usize, limit: f32| px(&planes.saturation, i) < limit;
    let mid = mean_rgb(planes, |i| {
        (0.2..=0.8).contains(&px(luma, i)) && neutral(i, 0.25)
    });
    let mut sorted = luma.clone();
    let p98 = quantile(&mut sorted, 0.98);
    let whites = mean_rgb(planes, |i| {
        let v = px(luma, i);
        v >= p98 && v < 0.99
    });
    let shadows = mean_rgb(planes, |i| {
        (0.05..=0.25).contains(&px(luma, i)) && neutral(i, 0.35)
    });
    let over = fraction(
        &planes
            .saturation
            .iter()
            .zip(&planes.red)
            .zip(&planes.green)
            .zip(&planes.blue)
            .map(|(((s, r), g), b)| {
                if *s > 0.92 && r.max(*g).max(*b) > 0.6 {
                    1.0
                } else {
                    0.0
                }
            })
            .collect::<Vec<_>>(),
        |v| v > 0.5,
    );
    let (mut rg, mut yb) = (
        Vec::with_capacity(luma.len()),
        Vec::with_capacity(luma.len()),
    );
    for ((r, g), b) in planes.red.iter().zip(&planes.green).zip(&planes.blue) {
        rg.push((r - g) * 255.0);
        yb.push((0.5 * (r + g) - b) * 255.0);
    }
    let colourfulness = match (spread(&rg), spread(&yb), mean(&rg), mean(&yb)) {
        (Some(s1), Some(s2), Some(m1), Some(m2)) => Some(s1.hypot(s2) + 0.3 * m1.hypot(m2)),
        _ => None,
    };
    let skin = faces.first().map(skin_rect).filter(|r| r.usable());
    let skin_rgb = skin.and_then(|rect| {
        Some((
            planes.mean(&planes.red, rect)?,
            planes.mean(&planes.green, rect)?,
            planes.mean(&planes.blue, rect)?,
        ))
    });
    // Chai and Ngan's skin box in YCbCr, which holds every human skin tone: a distance outside
    // it, not a distance from any one tone.
    let skin_tone = skin_rgb.map(|(r, g, b)| {
        let (r, g, b) = (r * 255.0, g * 255.0, b * 255.0);
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let cb = 128.0 + (b - y) * 0.564;
        let cr = 128.0 + (r - y) * 0.713;
        let outside_cr = (133.0 - cr).max(cr - 173.0).max(0.0);
        let outside_cb = (77.0 - cb).max(cb - 127.0).max(0.0);
        1.0 - ramp(outside_cr.hypot(outside_cb), 0.0, 20.0)
    });
    let mut families = [0_u32; 12];
    for (i, s) in planes.saturation.iter().enumerate().step_by(2) {
        if *s > 0.2 {
            let h = hue(
                px(&planes.red, i),
                px(&planes.green, i),
                px(&planes.blue, i),
            );
            if let Some(slot) = families.get_mut(((h / 30.0) as usize).min(11)) {
                *slot += 1;
            }
        }
    }
    let coloured: u32 = families.iter().sum();
    let mut ranked = families;
    ranked.sort_unstable_by(|a, b| b.cmp(a));
    let palette =
        (coloured > 100).then(|| ranked.iter().take(3).sum::<u32>() as f32 / coloured as f32);
    [
        mid.and_then(|(r, g, b)| cast(r, g, b)),
        whites.and_then(|(r, g, b)| cast(r, g, b)),
        Some(saturation),
        Some(over),
        colourfulness,
        skin_tone,
        skin.and_then(|rect| planes.mean(&planes.saturation, rect)),
        palette,
        mid.map(|(r, g, b)| g - f32::midpoint(r, b)),
        shadows.and_then(|(r, g, b)| cast(r, g, b)),
    ]
}

/// The tilt of the frame's strong straight lines from level or plumb, in degrees, when the
/// frame has enough of them to say.
fn tilt(planes: &Planes) -> Option<f32> {
    let mut magnitudes = planes.magnitude.clone();
    let floor = quantile(&mut magnitudes, 0.9).max(0.02);
    let mut bins = [0.0_f32; 180];
    let mut total = 0.0_f32;
    for i in 0..planes.magnitude.len() {
        let m = px(&planes.magnitude, i);
        if m < floor {
            continue;
        }
        // The line runs perpendicular to its gradient; fold to 0..180 degrees.
        let angle =
            (px(&planes.gy, i).atan2(px(&planes.gx, i)).to_degrees() + 90.0).rem_euclid(180.0);
        if let Some(bin) = bins.get_mut((angle as usize).min(179)) {
            *bin += m;
        }
        total += m;
    }
    if total <= 0.0 {
        return None;
    }
    let near = |centre: i32| -> (f32, i32) {
        let mut best = (0.0, 0);
        let mut mass = 0.0;
        for offset in -8..=8 {
            let bin = (centre + offset).rem_euclid(180) as usize;
            let value = bins.get(bin).copied().unwrap_or_default();
            mass += value;
            if value > best.0 {
                best = (value, offset);
            }
        }
        (mass, best.1)
    };
    let (level_mass, level_offset) = near(0);
    let (plumb_mass, plumb_offset) = near(90);
    if (level_mass + plumb_mass) / total < 0.25 {
        return None;
    }
    let offset = if level_mass >= plumb_mass {
        level_offset
    } else {
        plumb_offset
    };
    Some(offset.unsigned_abs() as f32)
}

fn composition(planes: &Planes, faces: &[Face]) -> [Option<f32>; 10] {
    let (w, h) = (planes.width as f32, planes.height as f32);
    let point = faces.first().map_or_else(
        || {
            // The centroid of the sharpest cells stands in for a subject nobody detected.
            let cells = planes.grid(8);
            let mut scored: Vec<(f32, Rect)> = cells
                .iter()
                .filter_map(|c| Some((planes.edges(*c, 0.95)?, *c)))
                .collect();
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            let top: Vec<Rect> = scored.iter().take(6).map(|(_, c)| *c).collect();
            let n = top.len().max(1) as f32;
            (
                top.iter()
                    .map(|c| (c.left + c.right) as f32 / 2.0)
                    .sum::<f32>()
                    / n
                    / w,
                top.iter()
                    .map(|c| (c.top + c.bottom) as f32 / 2.0)
                    .sum::<f32>()
                    / n
                    / h,
            )
        },
        |f| {
            (
                (f.rect.left + f.rect.right) as f32 / 2.0 / w,
                (f.rect.top + f.rect.bottom) as f32 / 2.0 / h,
            )
        },
    );
    let anchors = [
        (1.0 / 3.0, 1.0 / 3.0),
        (2.0 / 3.0, 1.0 / 3.0),
        (1.0 / 3.0, 2.0 / 3.0),
        (2.0 / 3.0, 2.0 / 3.0),
        (0.5, 0.5),
    ];
    let placement = anchors
        .iter()
        .map(|(x, y)| (point.0 - x).hypot(point.1 - y))
        .fold(f32::INFINITY, f32::min)
        / std::f32::consts::SQRT_2;
    let centre = planes.mean(&planes.luma, planes.rect(0.25, 0.25, 0.75, 0.75));
    let border: Vec<f32> = [
        planes.rect(0.0, 0.0, 1.0, 0.06),
        planes.rect(0.0, 0.94, 1.0, 1.0),
        planes.rect(0.0, 0.0, 0.06, 1.0),
        planes.rect(0.94, 0.0, 1.0, 1.0),
    ]
    .iter()
    .filter_map(|r| planes.mean(&planes.luma, *r))
    .collect();
    let corners: Vec<f32> = [
        planes.rect(0.0, 0.0, 0.1, 0.1),
        planes.rect(0.9, 0.0, 1.0, 0.1),
        planes.rect(0.0, 0.9, 0.1, 1.0),
        planes.rect(0.9, 0.9, 1.0, 1.0),
    ]
    .iter()
    .filter_map(|r| planes.mean(&planes.luma, *r))
    .collect();
    let left: f32 = planes
        .values(&planes.magnitude, planes.rect(0.0, 0.0, 0.5, 1.0))
        .iter()
        .sum();
    let right: f32 = planes
        .values(&planes.magnitude, planes.rect(0.5, 0.0, 1.0, 1.0))
        .iter()
        .sum();
    let ring: Vec<f32> = planes
        .grid(8)
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let (row, column) = (i / 8, i % 8);
            row == 0 || row == 7 || column == 0 || column == 7
        })
        .flat_map(|(_, c)| planes.values(&planes.magnitude, *c))
        .collect();
    let quiet = planes
        .grid(8)
        .iter()
        .filter(|c| planes.edges(**c, 0.9).is_some_and(|v| v < 0.04))
        .count() as f32
        / 64.0;
    let faces_cut = (!faces.is_empty()).then(|| {
        if faces.iter().any(|f| f.cut) {
            0.0
        } else {
            1.0
        }
    });
    [
        tilt(planes),
        Some(placement),
        faces.first().map(|f| f.rect.top as f32 / h),
        faces_cut,
        centre
            .zip(mean(&border))
            .and_then(|(c, b)| (c > 0.02).then(|| b / c)),
        (left + right > 0.0).then(|| 1.0 - (left - right).abs() / (left + right)),
        faces.first().map(|f| f.area),
        Some(fraction(&ring, |v| v / planes.contrast > 0.1)),
        centre
            .zip(mean(&corners))
            .and_then(|(c, k)| (c > 0.02).then(|| k / c)),
        Some(quiet),
    ]
}

fn mouth_rect(planes: &Planes, face: &Face) -> Rect {
    let [a, b] = face.mouth;
    let half_width = ((a.0 - b.0).abs() / 2.0).max(2.0);
    planes.around(
        f32::midpoint(a.0, b.0),
        f32::midpoint(a.1, b.1),
        half_width,
        face.eye_distance * 0.15,
    )
}

fn people(planes: &Planes, faces: &[Face]) -> [Option<f32>; 10] {
    let Some(main) = faces.first() else {
        return [None; 10];
    };
    let sharpness: Vec<f32> = faces
        .iter()
        .filter_map(|f| planes.edges(f.rect, 0.99))
        .collect();
    let all_sharp = (sharpness.len() >= 2).then(|| {
        let high = sharpness.iter().copied().fold(0.0_f32, f32::max);
        let low = sharpness.iter().copied().fold(f32::INFINITY, f32::min);
        if high > 1e-5 {
            low / high
        } else {
            1.0
        }
    });
    let mid_x = usize::midpoint(main.rect.left, main.rect.right);
    let halves = planes
        .mean(
            &planes.luma,
            Rect {
                right: mid_x,
                ..main.rect
            },
        )
        .zip(planes.mean(
            &planes.luma,
            Rect {
                left: mid_x,
                ..main.rect
            },
        ));
    let reliable = main.eye_distance > 3.0;
    let eye_mid = (
        f32::midpoint(main.eyes[0].0, main.eyes[1].0),
        f32::midpoint(main.eyes[0].1, main.eyes[1].1),
    );
    let mouth_width = (main.mouth[0].0 - main.mouth[1].0).hypot(main.mouth[0].1 - main.mouth[1].1);
    let face_luma = planes.mean(&planes.luma, skin_rect(main));
    let mouth = mouth_rect(planes, main);
    let teeth = face_luma.and_then(|skin| {
        let lumas = planes.values(&planes.luma, mouth);
        let sats = planes.values(&planes.saturation, mouth);
        (lumas.len() > 6).then(|| {
            lumas
                .iter()
                .zip(&sats)
                .filter(|(l, s)| **l > skin * 1.1 && **s < 0.3)
                .count() as f32
                / lumas.len() as f32
        })
    });
    let tilt = (main.eyes[1].1 - main.eyes[0].1)
        .atan2(main.eyes[1].0 - main.eyes[0].0)
        .to_degrees()
        .abs();
    let tilt = if tilt > 90.0 { 180.0 - tilt } else { tilt };
    [
        Some(faces.iter().map(|f| f.confidence).sum::<f32>() / faces.len() as f32),
        Some(main.area),
        all_sharp,
        face_luma,
        halves.and_then(|(l, r)| ratio(l, r)),
        reliable.then(|| (main.nose.0 - eye_mid.0).abs() / main.eye_distance),
        reliable.then(|| mouth_width / main.eye_distance),
        teeth,
        Some(faces.iter().filter(|f| !f.cut).count() as f32 / faces.len() as f32),
        reliable.then_some(tilt),
    ]
}

struct EyeReading {
    contrast: f32,
    sharp: Option<f32>,
    catchlight: f32,
    red: f32,
    whites: f32,
    level: f32,
    gaze: f32,
}

fn eye_box(planes: &Planes, face: &Face, eye: (f32, f32)) -> Rect {
    planes.around(
        eye.0,
        eye.1,
        face.eye_distance * 0.22,
        face.eye_distance * 0.12,
    )
}

fn read_eye(
    planes: &Planes,
    face: &Face,
    eye: (f32, f32),
    skin: f32,
    skin_spread: f32,
) -> Option<EyeReading> {
    let rect = eye_box(planes, face, eye);
    if !rect.usable() {
        return None;
    }
    let lumas = planes.values(&planes.luma, rect);
    let sats = planes.values(&planes.saturation, rect);
    let own = spread(&lumas)?;
    let average = mean(&lumas)?;
    let brightest = lumas.iter().copied().fold(0.0_f32, f32::max);
    // The pupil: the darkest pixel, and how far it sits from the middle of the eye.
    let (darkest, _) = lumas
        .iter()
        .enumerate()
        .fold((0_usize, f32::INFINITY), |best, (i, v)| {
            if *v < best.1 {
                (i, *v)
            } else {
                best
            }
        });
    let box_width = rect.right - rect.left;
    let pupil_x = (darkest % box_width.max(1)) as f32 / box_width.max(1) as f32;
    let centre = (eye.0 as isize, eye.1 as isize);
    let red = {
        let mut values = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let r = planes.at(&planes.red, centre.0 + dx, centre.1 + dy);
                let g = planes.at(&planes.green, centre.0 + dx, centre.1 + dy);
                let b = planes.at(&planes.blue, centre.0 + dx, centre.1 + dy);
                values.push(r - f32::midpoint(g, b));
            }
        }
        mean(&values).unwrap_or_default()
    };
    Some(EyeReading {
        contrast: own / (skin_spread + 0.01),
        sharp: planes.edges(rect, 0.95),
        catchlight: brightest - average,
        red,
        whites: lumas
            .iter()
            .zip(&sats)
            .filter(|(l, s)| **l > skin * 1.15 && **s < 0.25)
            .count() as f32
            / lumas.len() as f32,
        level: average / skin.max(0.02),
        gaze: (pupil_x - 0.5).abs() * 2.0,
    })
}

fn eyes(planes: &Planes, faces: &[Face], settings: &Settings) -> [Option<f32>; 10] {
    let mut readings: Vec<(EyeReading, EyeReading)> = Vec::new();
    let mut found = Vec::new();
    let mut spacing = Vec::new();
    for face in faces {
        let width = (face.rect.right - face.rect.left) as f32;
        let inside = face.eyes.iter().all(|(x, y)| {
            *x >= face.rect.left as f32
                && *x <= face.rect.right as f32
                && *y >= face.rect.top as f32
                && *y <= (face.rect.top + face.rect.bottom) as f32 / 2.0 + 2.0
        });
        found.push(if inside { 1.0 } else { 0.0 });
        if width > 0.0 {
            spacing.push(face.eye_distance / width);
        }
        // Below this the eye box is a few pixels across and reads the analysis grid, not an
        // eyelid. The comparison with similar shots still catches a blink on a smaller face.
        if !inside || face.eye_distance < MIN_EYE_DISTANCE {
            continue;
        }
        let skin_rect = skin_rect(face);
        let (Some(skin), Some(skin_spread)) = (
            planes.mean(&planes.luma, skin_rect),
            planes.spread(&planes.luma, skin_rect),
        ) else {
            continue;
        };
        if let (Some(first), Some(second)) = (
            read_eye(planes, face, face.eyes[0], skin, skin_spread),
            read_eye(planes, face, face.eyes[1], skin, skin_spread),
        ) {
            readings.push((first, second));
        }
    }
    if faces.is_empty() {
        return [None; 10];
    }
    if readings.is_empty() {
        return [
            None,
            mean(&found),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            mean(&spacing),
        ];
    }
    let over = |f: &dyn Fn(&EyeReading) -> f32| -> Option<f32> {
        let values: Vec<f32> = readings.iter().flat_map(|(a, b)| [f(a), f(b)]).collect();
        mean(&values)
    };
    // The least open eye in the frame decides: one blink spoils a group.
    let open = readings
        .iter()
        .map(|(a, b)| f32::midpoint(a.contrast, b.contrast))
        .fold(None, |low: Option<f32>, v| {
            Some(low.map_or(v, |l| l.min(v)))
        });
    let sharp: Vec<f32> = readings
        .iter()
        .flat_map(|(a, b)| [a.sharp, b.sharp])
        .flatten()
        .collect();
    let matched = readings
        .iter()
        .filter_map(|(a, b)| ratio(a.contrast, b.contrast))
        .fold(None, |low: Option<f32>, v| {
            Some(low.map_or(v, |l| l.min(v)))
        });
    [
        open,
        mean(&found),
        mean(&sharp),
        over(&|e| e.catchlight),
        // Without a flash the centre of an eye reads iris and skin, not a red pupil.
        over(&|e| e.red).filter(|_| settings.flash == Some(true)),
        matched,
        over(&|e| e.whites),
        over(&|e| e.level),
        over(&|e| e.gaze),
        mean(&spacing),
    ]
}

fn light(planes: &Planes, faces: &[Face], settings: &Settings) -> [Option<f32>; 10] {
    let face = faces.first();
    let modelling = face.and_then(|f| {
        let mut lumas = planes.values(&planes.luma, skin_rect(f));
        if lumas.len() < 16 {
            return None;
        }
        let dark = quantile(&mut lumas, 0.25);
        let bright = quantile(&mut lumas, 0.75);
        (dark > 0.01).then(|| bright / dark)
    });
    let hard = fraction(&planes.magnitude, |m| m > 0.25);
    let centre = planes.mean(&planes.luma, planes.rect(0.3, 0.3, 0.7, 0.7));
    let border = planes.mean(&planes.luma, planes.rect(0.0, 0.0, 1.0, 0.1));
    let falloff = (settings.flash == Some(true))
        .then(|| {
            centre
                .zip(border)
                .and_then(|(c, b)| (b > 0.005).then(|| c / b))
        })
        .flatten();
    let casts: Vec<(f32, f32)> = planes
        .grid(4)
        .iter()
        .filter_map(|cell| {
            let (r, g, b) = (
                planes.mean(&planes.red, *cell)?,
                planes.mean(&planes.green, *cell)?,
                planes.mean(&planes.blue, *cell)?,
            );
            let sum = r + g + b;
            (sum > 0.15).then(|| (r / sum, b / sum))
        })
        .collect();
    let mixed = (casts.len() >= 4).then(|| {
        let reds: Vec<f32> = casts.iter().map(|c| c.0).collect();
        let blues: Vec<f32> = casts.iter().map(|c| c.1).collect();
        spread(&reds)
            .unwrap_or_default()
            .hypot(spread(&blues).unwrap_or_default())
            * 3.0
    });
    let mut luma = planes.luma.clone();
    let median = quantile(&mut luma, 0.5);
    let face_luma = face.and_then(|f| planes.mean(&planes.luma, skin_rect(f)));
    let near = fraction(&planes.luma, |v| (0.92..0.98).contains(&v));
    let clipped = fraction(&planes.luma, |v| v >= 0.98);
    let frame_spread = spread(&planes.luma);
    let subject_luma = planes.mean(&planes.luma, subject(planes, faces));
    let background = mean(
        &[
            planes.rect(0.0, 0.0, 1.0, 0.12),
            planes.rect(0.0, 0.0, 0.12, 1.0),
            planes.rect(0.88, 0.0, 1.0, 1.0),
        ]
        .iter()
        .filter_map(|r| planes.mean(&planes.luma, *r))
        .collect::<Vec<_>>(),
    );
    [
        modelling,
        Some(hard),
        falloff,
        mixed,
        face_luma.and_then(|f| (median > 0.02).then(|| f / median)),
        face.map(|f| fraction(&planes.values(&planes.luma, skin_rect(f)), |v| v > 0.95)),
        (near + clipped > 0.005).then(|| near / (near + clipped)),
        face.and_then(|f| planes.spread(&planes.luma, f.rect))
            .zip(frame_spread)
            .and_then(|(f, s)| (s > 1e-3).then(|| f / s)),
        mean(&planes.luma),
        subject_luma.zip(background).map(|(s, b)| (s - b).abs()),
    ]
}

// ----------------------------------------------------------------------------------------
// Scoring a collection.
// ----------------------------------------------------------------------------------------

/// The rank of `value` among `others`, as a fraction: 0 lowest, 1 highest, ties shared.
fn rank_of(value: f32, others: &[f32]) -> f32 {
    if others.len() < 2 {
        return 1.0;
    }
    let below = others.iter().filter(|v| **v < value).count() as f32;
    let equal = others
        .iter()
        .filter(|v| (**v - value).abs() <= f32::EPSILON)
        .count() as f32;
    ((below + (equal - 1.0).max(0.0) / 2.0) / (others.len() - 1) as f32).clamp(0.0, 1.0)
}

/// Score every frame's checks against the whole collection. `values` has one entry per frame,
/// each `COUNT` long (shorter is padded with abstentions).
#[must_use]
pub fn score(values: &[Vec<Option<f32>>]) -> Vec<Scored> {
    let columns: Vec<Vec<f32>> = (0..COUNT)
        .map(|check| {
            values
                .iter()
                .filter_map(|v| v.get(check).copied().flatten())
                .filter(|v| v.is_finite())
                .collect()
        })
        .collect();
    values
        .iter()
        .map(|frame| {
            let checks: Vec<Option<u8>> = CHECKS
                .iter()
                .enumerate()
                .map(|(index, spec)| {
                    let value = frame
                        .get(index)
                        .copied()
                        .flatten()
                        .filter(|v| v.is_finite())?;
                    let column = columns.get(index).map_or(&[][..], Vec::as_slice);
                    let score = match spec.scale {
                        Scale::Rank => RANK_FLOOR + (1.0 - RANK_FLOOR) * rank_of(value, column),
                        Scale::RankLow => {
                            RANK_FLOOR + (1.0 - RANK_FLOOR) * (1.0 - rank_of(value, column))
                        }
                        absolute => absolute.absolute(value)?,
                    };
                    Some((score * 100.0).round().clamp(0.0, 100.0) as u8)
                })
                .collect();
            combine(checks)
        })
        .collect()
}

/// Groups from checks, and the total from groups.
#[must_use]
pub fn combine(checks: Vec<Option<u8>>) -> Scored {
    let mut sums = [(0.0_f32, 0.0_f32); 10];
    for (spec, score) in CHECKS.iter().zip(&checks) {
        if let (Some(score), Some(slot)) = (score, sums.get_mut(spec.category.index())) {
            slot.0 += f32::from(*score) / 100.0 * spec.weight;
            slot.1 += spec.weight;
        }
    }
    let categories: Vec<Option<f32>> = sums
        .iter()
        .map(|(sum, weight)| (*weight > 0.0).then(|| sum / weight))
        .collect();
    // A weighted geometric mean: no group rescues another.
    let (mut log_sum, mut weight_sum) = (0.0_f32, 0.0_f32);
    for (category, score) in Category::ALL.iter().zip(&categories) {
        if let Some(score) = score {
            log_sum += category.weight() * score.max(0.02).ln();
            weight_sum += category.weight();
        }
    }
    let mean = if weight_sum > 0.0 {
        (log_sum / weight_sum).exp() * 100.0
    } else {
        50.0
    };
    // A mean over nine groups dilutes one severe failure: a frame three stops under scored 78
    // against 92 for the same frame exposed, on real photographs. A core group that fails
    // takes the total down with it.
    let worst = CORE
        .iter()
        .filter_map(|c| categories.get(c.index()).copied().flatten())
        .fold(1.0_f32, f32::min);
    let overall = mean * (worst / CORE_FLOOR).min(1.0);
    Scored {
        checks,
        categories: categories
            .iter()
            .map(|c| c.map(|v| (v * 100.0).round().clamp(0.0, 100.0) as u8))
            .collect(),
        overall,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Large random blocks, optionally box-blurred along x and y and tinted.
    fn scene(
        seed: u32,
        blur_x: usize,
        blur_y: usize,
        tint: [f32; 3],
        level: f32,
    ) -> (Vec<u8>, usize, usize) {
        let (width, height) = (640_usize, 480_usize);
        let mut luma = vec![0.0_f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let cell = (x / 48) as u32 * 31 + (y / 48) as u32 * 17 + seed * 7919;
                let hashed = cell.wrapping_mul(2_654_435_761).rotate_left(13) % 180;
                luma[y * width + x] = (40.0 + hashed as f32) * level;
            }
        }
        let pass = |plane: &[f32], radius: usize, horizontal: bool| -> Vec<f32> {
            if radius == 0 {
                return plane.to_vec();
            }
            let mut out = vec![0.0; plane.len()];
            for y in 0..height {
                for x in 0..width {
                    let mut sum = 0.0;
                    let mut n = 0.0;
                    for k in 0..=2 * radius {
                        let (sx, sy) = if horizontal {
                            ((x + k).saturating_sub(radius).min(width - 1), y)
                        } else {
                            (x, (y + k).saturating_sub(radius).min(height - 1))
                        };
                        sum += plane[sy * width + sx];
                        n += 1.0;
                    }
                    out[y * width + x] = sum / n;
                }
            }
            out
        };
        let blurred = pass(&pass(&luma, blur_x, true), blur_y, false);
        let mut rgb = Vec::with_capacity(width * height * 3);
        for value in blurred {
            for channel in tint {
                rgb.push((value * channel).clamp(0.0, 255.0) as u8);
            }
        }
        (rgb, width, height)
    }

    fn checks_of(rgb: &[u8], width: usize, height: usize) -> Vec<Option<f32>> {
        measure(&Sample {
            rgb,
            width,
            height,
            faces: &[],
            settings: Settings::default(),
        })
        .expect("measured")
    }

    fn index(id: &str) -> usize {
        CHECKS.iter().position(|c| c.id == id).expect("known check")
    }

    #[test]
    fn there_are_one_hundred_checks_ten_in_each_group_with_unique_ids() {
        assert_eq!(CHECKS.len(), 100);
        for category in Category::ALL {
            assert_eq!(
                CHECKS.iter().filter(|c| c.category == category).count(),
                10,
                "{category:?}"
            );
        }
        let mut ids: Vec<&str> = CHECKS.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 100);
        assert!(CHECKS[..CONTEXT]
            .iter()
            .all(|c| c.category != Category::Context));
        assert!(CHECKS[CONTEXT..]
            .iter()
            .all(|c| c.category == Category::Context));
        assert!(CHECKS.iter().all(|c| c.weight > 0.0));
    }

    #[test]
    fn a_frame_measures_ninety_checks_and_leaves_the_collection_ten_open() {
        let (rgb, w, h) = scene(1, 0, 0, [1.0, 1.0, 1.0], 1.0);
        let values = checks_of(&rgb, w, h);
        assert_eq!(values.len(), COUNT);
        assert!(values[CONTEXT..].iter().all(Option::is_none));
        // No faces: every face and eye check abstains rather than failing.
        for (spec, value) in CHECKS.iter().zip(&values) {
            if matches!(spec.category, Category::Faces | Category::Eyes) {
                assert!(value.is_none(), "{} should abstain without a face", spec.id);
            }
        }
    }

    #[test]
    fn blur_lowers_every_focus_check() {
        let (sharp, w, h) = scene(2, 0, 0, [1.0, 1.0, 1.0], 1.0);
        let (soft, _, _) = scene(2, 6, 6, [1.0, 1.0, 1.0], 1.0);
        let (a, b) = (checks_of(&sharp, w, h), checks_of(&soft, w, h));
        for id in [
            "focus_strongest_edges",
            "focus_peak_edges",
            "focus_best_region",
            "focus_centre",
        ] {
            let i = index(id);
            assert!(
                a[i].unwrap() > b[i].unwrap() * 1.5,
                "{id}: {:?} vs {:?}",
                a[i],
                b[i]
            );
        }
        let scored = score(&[a, b]);
        assert!(
            scored[0].overall > scored[1].overall + 10.0,
            "{} vs {}",
            scored[0].overall,
            scored[1].overall
        );
        assert!(scored[1].categories[1].unwrap() < 50);
    }

    #[test]
    fn smearing_one_way_is_read_as_motion() {
        let (sharp, w, h) = scene(3, 0, 0, [1.0, 1.0, 1.0], 1.0);
        let (smeared, _, _) = scene(3, 9, 0, [1.0, 1.0, 1.0], 1.0);
        let (a, b) = (checks_of(&sharp, w, h), checks_of(&smeared, w, h));
        let ratio = index("motion_direction_ratio");
        // Square blocks: a diagonal reads an axis-aligned edge at about 1/sqrt(2) of its
        // strength, so 0.7 is what a perfectly sharp frame of them measures.
        assert!(a[ratio].unwrap() > 0.6, "{:?}", a[ratio]);
        assert!(
            b[ratio].unwrap() < a[ratio].unwrap() * 0.6,
            "{:?} vs {:?}",
            b[ratio],
            a[ratio]
        );
    }

    #[test]
    fn a_dark_frame_fails_exposure_and_a_tint_is_a_cast() {
        let (normal, w, h) = scene(4, 0, 0, [1.0, 1.0, 1.0], 1.0);
        let (dark, _, _) = scene(4, 0, 0, [1.0, 1.0, 1.0], 0.08);
        // A cast is read on what should be grey: mild enough that the greys stay near-neutral.
        let (warm, _, _) = scene(4, 0, 0, [1.12, 1.0, 0.9], 1.0);
        let values = [
            checks_of(&normal, w, h),
            checks_of(&dark, w, h),
            checks_of(&warm, w, h),
        ];
        let scored = score(&values);
        let black = index("exposure_black_frame");
        assert!(scored[0].checks[black].unwrap() > 80);
        assert!(scored[1].checks[black].unwrap() < 30);
        // Monochrome: the neutral frames' colour checks abstain; the tinted one is a cast.
        let cast = index("colour_cast");
        assert!(values[0][cast].is_none());
        assert!(
            scored[2].checks[cast].unwrap()
                < scored[2].checks[index("exposure_blown_area")].unwrap()
        );
        assert!(values[2][cast].unwrap() > 0.15, "{:?}", values[2][cast]);
    }

    #[test]
    fn scales_score_in_the_direction_they_say() {
        assert_eq!(Scale::Rise(0.0, 1.0).absolute(0.5), Some(0.5));
        assert_eq!(Scale::Fall(0.0, 1.0).absolute(0.25), Some(0.75));
        assert_eq!(Scale::Band(0.2, 0.4, 0.1).absolute(0.3), Some(1.0));
        assert!(Scale::Band(0.2, 0.4, 0.1).absolute(0.55).unwrap() < 0.01);
        assert_eq!(Scale::Rank.absolute(0.3), None);
        assert_eq!(Scale::Rise(0.0, 1.0).absolute(f32::NAN), None);
    }

    #[test]
    fn a_rank_never_fails_a_frame_and_a_lone_frame_tops_it() {
        let mut values = vec![vec![None; COUNT]; 3];
        let i = index("focus_fine_detail");
        for (frame, value) in values.iter_mut().zip([0.1, 0.2, 0.3]) {
            frame[i] = Some(value);
        }
        let scored = score(&values);
        assert_eq!(scored[0].checks[i], Some((RANK_FLOOR * 100.0) as u8));
        assert_eq!(scored[2].checks[i], Some(100));
        let lone = score(&values[..1]);
        assert_eq!(lone[0].checks[i], Some(100));
    }

    #[test]
    fn one_failed_group_is_not_rescued_by_the_others() {
        let mut checks = vec![Some(100_u8); COUNT];
        let good = combine(checks.clone()).overall;
        for (spec, slot) in CHECKS.iter().zip(checks.iter_mut()) {
            if spec.category == Category::Focus {
                *slot = Some(0);
            }
        }
        let soft = combine(checks).overall;
        assert!(good > 99.0);
        assert!(soft < 50.0, "{soft}");
    }

    #[test]
    fn a_failed_core_group_takes_the_total_down_with_it() {
        let mut checks = vec![Some(95_u8); COUNT];
        for (spec, slot) in CHECKS.iter().zip(checks.iter_mut()) {
            if spec.category == Category::Exposure {
                *slot = Some(20);
            }
        }
        let dark = combine(checks.clone()).overall;
        assert!(dark < 30.0, "{dark}");
        // The same failure in a group that is not core costs only its share.
        for (spec, slot) in CHECKS.iter().zip(checks.iter_mut()) {
            *slot = Some(if spec.category == Category::Light {
                20
            } else {
                95
            });
        }
        let flat = combine(checks).overall;
        assert!(flat > 80.0, "{flat}");
    }

    #[test]
    fn abstaining_groups_leave_the_total_alone() {
        let mut checks = vec![None; COUNT];
        for (spec, slot) in CHECKS.iter().zip(checks.iter_mut()) {
            if spec.category == Category::Focus {
                *slot = Some(80);
            }
        }
        let scored = combine(checks);
        assert!((scored.overall - 80.0).abs() < 0.5, "{}", scored.overall);
        assert_eq!(scored.categories.iter().flatten().count(), 1);
    }
}
