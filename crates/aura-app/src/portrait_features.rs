//! Measured, landmark-guided portrait finishing: blemishes, eyes, under-eyes, teeth and shine.
//!
//! Every decision here is a *measurement against the same face*, never a comparison with an
//! ideal appearance. A spot is a candidate only when it is redder than the skin immediately
//! around it; a tooth is whitened only when it is yellower than the frame's own neutral; an
//! under-eye is lifted only when it is darker than the same person's cheek. Dark spots that
//! are not redder than their surroundings (moles, freckles, beauty marks) are always kept and
//! counted in the default pass. The separate opt-in deep cleanup can include dark marks.
//!
//! Each finding becomes an ordinary, editable native retouch operation with a stable ID, so
//! the photographer can inspect, weaken, disable or remove any single one of them.
// Every index below is produced from bounds-checked window coordinates; out-of-range reads
// fall back to `get`. Pixel geometry is intentionally computed in f32 and truncated.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::too_many_lines
)]

use crate::retouch_settings::{gain, threshold, Settings};
use aura_recipe::retouch_tools::{BrushMask, BrushStroke, Edit, LuminanceRange, Selection, Tool};
use aura_vision::portrait::PortraitFace;
use serde::{Deserialize, Serialize};

/// The planner version recorded in the report; bump on any behavioural change.
pub const VERSION: &str = "measured-features-v3";
pub(crate) mod deep_blemish;
/// At most this many healed spots per face. A face with more is left for a person to judge.
pub const MAX_SPOTS: usize = 12;
/// More compact red marks than this on one face is a pattern (freckles), not blemishes.
pub const FRECKLE_FIELD: usize = 15;

/// Packed 8-bit sRGB pixels, validated on construction.
#[derive(Debug, Clone, Copy)]
pub struct Pixels<'a> {
    data: &'a [u8],
    pub width: usize,
    pub height: usize,
}

impl<'a> Pixels<'a> {
    /// `None` when the buffer does not match the dimensions.
    #[must_use]
    pub fn new(bytes: &'a [u8], width: u32, height: u32) -> Option<Self> {
        let (w, h) = (width as usize, height as usize);
        (w > 2 && h > 2 && w.checked_mul(h)?.checked_mul(3)? == bytes.len()).then_some(Self {
            data: bytes,
            width: w,
            height: h,
        })
    }

    /// Linear-light RGB at an integer position; black outside the frame.
    #[must_use]
    pub fn linear(&self, x: usize, y: usize) -> [f32; 3] {
        if x >= self.width || y >= self.height {
            return [0.0; 3];
        }
        let i = (y * self.width + x) * 3;
        self.data
            .get(i..i + 3)
            .map_or([0.0; 3], |p| [decode(p[0]), decode(p[1]), decode(p[2])])
    }

    /// Display-encoded RGB in `0..1`.
    #[must_use]
    pub fn encoded(&self, x: usize, y: usize) -> [f32; 3] {
        if x >= self.width || y >= self.height {
            return [0.0; 3];
        }
        let i = (y * self.width + x) * 3;
        self.data.get(i..i + 3).map_or([0.0; 3], |p| {
            [
                f32::from(p[0]) / 255.0,
                f32::from(p[1]) / 255.0,
                f32::from(p[2]) / 255.0,
            ]
        })
    }
}

fn decode(v: u8) -> f32 {
    aura_raw::colour::curve::srgb_decode(f32::from(v) / 255.0)
}

/// Rec.709 luminance of linear RGB.
#[must_use]
pub fn luma(p: [f32; 3]) -> f32 {
    p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722
}

fn chroma(p: [f32; 3]) -> [f32; 3] {
    let total = (p[0] + p[1] + p[2]).max(1e-5);
    p.map(|v| v / total)
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn add(a: [f32; 2], b: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] + b[0] * k, a[1] + b[1] * k]
}

/// Distance from `p` to the segment `a..b`.
fn to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    let t = if len2 > 1e-6 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    distance(p, [a[0] + d[0] * t, a[1] + d[1] * t])
}

/// A capsule (a thick line segment) in pixel space.
#[derive(Debug, Clone, Copy)]
struct Capsule {
    a: [f32; 2],
    b: [f32; 2],
    r: f32,
}

impl Capsule {
    fn disk(c: [f32; 2], r: f32) -> Self {
        Self { a: c, b: c, r }
    }
    fn contains(self, p: [f32; 2]) -> bool {
        to_segment(p, self.a, self.b) <= self.r
    }
    /// Visit every pixel centre inside the capsule.
    fn each(self, px: &Pixels<'_>, mut f: impl FnMut(usize, usize)) {
        let x0 = (self.a[0].min(self.b[0]) - self.r).floor().max(0.0) as usize;
        let y0 = (self.a[1].min(self.b[1]) - self.r).floor().max(0.0) as usize;
        let x1 = ((self.a[0].max(self.b[0]) + self.r).ceil().max(0.0) as usize).min(px.width);
        let y1 = ((self.a[1].max(self.b[1]) + self.r).ceil().max(0.0) as usize).min(px.height);
        for y in y0..y1 {
            for x in x0..x1 {
                if self.contains([x as f32 + 0.5, y as f32 + 0.5]) {
                    f(x, y);
                }
            }
        }
    }
    /// The capsule as a normalized, editable brush stroke.
    fn stroke(self, px: &Pixels<'_>) -> BrushStroke {
        let (w, h) = (px.width as f32, px.height as f32);
        let short = w.min(h);
        let point = |[x, y]: [f32; 2]| [(x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0), 1.0];
        let mut points = vec![point(self.a)];
        if distance(self.a, self.b) > 0.5 {
            points.push(point(self.b));
        }
        BrushStroke {
            erase: false,
            radius: (self.r / short).clamp(0.0005, 0.25),
            opacity: 1.0,
            points,
        }
    }
}

/// The face in pixel space, with an eye axis `u` and a downward axis `v`.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    eyes: [[f32; 2]; 2],
    nose: [f32; 2],
    mouth: [[f32; 2]; 2],
    mid: [f32; 2],
    mouth_centre: [f32; 2],
    u: [f32; 2],
    v: [f32; 2],
    /// Distance between the eye landmarks, in pixels.
    d: f32,
    /// Distance between the mouth corners, in pixels.
    mouth_width: f32,
    bounds: [f32; 4],
}

impl Geometry {
    fn new(face: &PortraitFace, px: &Pixels<'_>) -> Option<Self> {
        let (w, h) = (px.width as f32, px.height as f32);
        let [eye_a, eye_b, nose, mouth_a, mouth_b] = face.landmarks.map(|[x, y]| [x * w, y * h]);
        let d = distance(eye_a, eye_b);
        if d < 1.0 {
            return None;
        }
        let u = [(eye_b[0] - eye_a[0]) / d, (eye_b[1] - eye_a[1]) / d];
        let mut v = [-u[1], u[0]];
        let mid = [(eye_a[0] + eye_b[0]) * 0.5, (eye_a[1] + eye_b[1]) * 0.5];
        let mouth_centre = [
            (mouth_a[0] + mouth_b[0]) * 0.5,
            (mouth_a[1] + mouth_b[1]) * 0.5,
        ];
        if (mouth_centre[0] - mid[0]) * v[0] + (mouth_centre[1] - mid[1]) * v[1] < 0.0 {
            v = [-v[0], -v[1]];
        }
        let [l, t, r, b] = face.bounds;
        Some(Self {
            eyes: [eye_a, eye_b],
            nose,
            mouth: [mouth_a, mouth_b],
            mid,
            mouth_centre,
            u,
            v,
            d,
            mouth_width: distance(mouth_a, mouth_b),
            bounds: [l * w, t * h, r * w, b * h],
        })
    }

    fn skin_areas(&self) -> [Capsule; 4] {
        let d = self.d;
        [
            Capsule::disk(add(self.eyes[0], self.v, 0.55 * d), 0.28 * d),
            Capsule::disk(add(self.eyes[1], self.v, 0.55 * d), 0.28 * d),
            Capsule::disk(add(self.mid, self.v, -0.5 * d), 0.27 * d),
            Capsule::disk(add(self.mouth_centre, self.v, 0.32 * d), 0.17 * d),
        ]
    }

    /// Eyes, brows, nostrils and lips: never searched for blemishes.
    fn exclusions(&self) -> Vec<Capsule> {
        let d = self.d;
        let mut out = Vec::with_capacity(10);
        // Order matters: the first four (eyes and brows) are also shine exclusions.
        for eye in self.eyes {
            out.push(Capsule::disk(eye, 0.27 * d));
            out.push(Capsule::disk(add(eye, self.v, -0.33 * d), 0.24 * d));
        }
        out.push(Capsule::disk(add(self.mid, self.v, -0.2 * d), 0.2 * d));
        out.push(Capsule::disk(add(self.nose, self.v, 0.06 * d), 0.22 * d));
        out.push(Capsule {
            a: self.mouth[0],
            b: self.mouth[1],
            r: 0.18 * d,
        });
        // Smile lines, from the nose wing to each mouth corner: creases, not blemishes.
        for (corner, side) in [(self.mouth[0], -1.0), (self.mouth[1], 1.0)] {
            out.push(Capsule {
                a: add(self.nose, self.u, side * 0.28 * d),
                b: add(corner, self.u, side * 0.08 * d),
                r: 0.05 * d,
            });
        }
        out
    }
}

/// The same person's skin, measured from their own cheeks and forehead.
#[derive(Debug, Clone, Copy)]
struct SkinReference {
    luma: f32,
    chroma: [f32; 3],
}

impl SkinReference {
    fn measure(g: &Geometry, px: &Pixels<'_>) -> Option<Self> {
        let mut means = Vec::new();
        for area in g.skin_areas().into_iter().take(3) {
            let mut sum = [0.0; 3];
            let mut n = 0.0;
            Capsule::disk(area.a, 0.1 * g.d).each(px, |x, y| {
                let p = px.linear(x, y);
                if p.iter().all(|v| *v < 0.93) && luma(p) > 0.004 {
                    for c in 0..3 {
                        sum[c] += p[c];
                    }
                    n += 1.0;
                }
            });
            if n >= 4.0 {
                means.push(sum.map(|v| v / n));
            }
        }
        if means.is_empty() {
            return None;
        }
        means.sort_by(|a, b| luma(*a).total_cmp(&luma(*b)));
        let rgb = *means.get(means.len() / 2)?;
        Some(Self {
            luma: luma(rgb),
            chroma: chroma(rgb),
        })
    }

    fn affine(&self, p: [f32; 3]) -> bool {
        let l = luma(p);
        let c = chroma(p);
        let spread = (0..3)
            .map(|i| (c[i] - self.chroma[i]).powi(2))
            .sum::<f32>()
            .sqrt();
        spread < 0.075 && l > self.luma * 0.35 && l < self.luma * 2.4
    }

    /// Stops relative to 18 % grey, the unit the renderer's luminance selections use.
    fn stops(&self, exposure: f32) -> f32 {
        ((self.luma.max(1e-4) / 0.18).log2() + exposure).clamp(-15.0, 15.0)
    }
}

/// One measured finding, as reported to the photographer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureReport {
    /// Temporary-looking spots healed.
    pub spots_healed: usize,
    /// Dark marks deliberately left alone because they may be permanent.
    pub marks_kept: usize,
    /// Human-readable findings, one per decision, in the order they were made.
    pub findings: Vec<String>,
}

/// Edits grouped by the history step that saves them.
#[derive(Debug, Clone, Default)]
pub struct FeatureEdits {
    pub blemishes: Vec<Edit>,
    pub refine: Vec<Edit>,
    pub eyes: Vec<Edit>,
    pub finishing: Vec<Edit>,
    pub report: FeatureReport,
}

fn base_edit(id: String, tool: Tool, amount: f32, px: &Pixels<'_>, region_px: [f32; 4]) -> Edit {
    let (w, h) = (px.width as f32, px.height as f32);
    let [cx, cy, rx, ry] = region_px;
    Edit {
        id,
        tool,
        enabled: true,
        region: [
            (cx / w).clamp(0.0, 1.0),
            (cy / h).clamp(0.0, 1.0),
            (rx / w).clamp(0.001, 1.0),
            (ry / h).clamp(0.001, 1.0),
        ],
        source: None,
        amount: amount.clamp(0.0, 1.0),
        feather: 0.7,
        radius: 0.002,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture: 1.0,
        tone: 0.5,
        warmth: 0.0,
        tint: 0.0,
        mask: None,
        skin: None,
        selection: None,
        matte: None,
    }
}

fn masked(mut edit: Edit, px: &Pixels<'_>, capsules: &[Capsule]) -> Edit {
    edit.mask = Some(BrushMask {
        strokes: capsules.iter().map(|c| c.stroke(px)).collect(),
    });
    edit
}

fn brighter_than(stops: f32, softness: f32) -> Selection {
    Selection {
        inverted: false,
        gradient: None,
        luminance: Some(LuminanceRange {
            low: stops.clamp(-16.0, 16.0),
            high: 16.0,
            softness,
        }),
    }
}

/// Measure one face and propose its finishing operations. `exposure` is the global change
/// (in stops) the same automatic pass applies first, so luminance selections stay aligned.
#[must_use]
pub fn plan(
    face: &PortraitFace,
    index: usize,
    px: &Pixels<'_>,
    exposure: f32,
    prefix: &str,
    options: &Options,
    face_matte: Option<&str>,
) -> FeatureEdits {
    let options = options.sanitised();
    let settings = &options.settings;
    let mut out = FeatureEdits::default();
    let Some(g) = Geometry::new(face, px) else {
        return out;
    };
    if g.d < 28.0 {
        out.report.findings.push(format!(
            "The face is {:.0} px between the eyes at analysis resolution; blemish, eye and teeth finishing need at least 28 px and were skipped.",
            g.d
        ));
        return out;
    }
    let Some(skin) = SkinReference::measure(&g, px) else {
        out.report
            .findings
            .push("No usable skin reference was found; finishing was skipped.".into());
        return out;
    };
    if options.blemishes {
        blemishes(&g, &skin, px, index, prefix, settings, &mut out);
    }
    if options.refine {
        refine(&g, &skin, px, index, prefix, settings, &mut out);
    }
    if options.eyes {
        eyes(&g, &skin, px, index, prefix, exposure, settings, &mut out);
    }
    if options.teeth {
        mouth(&g, &skin, px, index, prefix, exposure, settings, &mut out);
    }
    if settings.shine > 0.0 {
        shine(&g, &skin, px, index, prefix, settings, &mut out);
    }
    sculpt(&g, px, index, prefix, settings, face_matte, &mut out);
    // A heal either repairs a spot or it does not; every other strength follows the chosen
    // intensity, within each tool's own bounds.
    for edit in out
        .refine
        .iter_mut()
        .chain(&mut out.eyes)
        .chain(&mut out.finishing)
    {
        if edit.tool != Tool::RedEye {
            edit.amount = (edit.amount * options.intensity).clamp(0.05, 0.9);
        }
    }
    out
}

/// Which automatic finishing runs, and how strongly. Chosen by the photographer in Retouch and
/// remembered in the recipe's report, so a later Auto enhance repeats the same choice.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Options {
    /// Multiplies every measured strength. `0.5` is subtle, `1.0` natural, `1.5` polished.
    pub intensity: f32,
    pub blemishes: bool,
    pub eyes: bool,
    pub teeth: bool,
    /// Fine lines, smile-line softening and local redness evening.
    pub refine: bool,
    /// Which skin the automatic retouch works on.
    pub scope: Scope,
    /// The fine controls. ADR-0082.
    pub settings: crate::retouch_settings::Settings,
}

/// The area the automatic retouch is allowed to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The face only: skin, blemishes, lines, eyes and teeth.
    #[default]
    Face,
    /// Visible body skin (neck, shoulders, arms) only; the face is left as it is.
    Body,
    /// Both.
    FaceAndBody,
}

impl Scope {
    #[must_use]
    pub const fn face(self) -> bool {
        !matches!(self, Self::Body)
    }
    #[must_use]
    pub const fn body(self) -> bool {
        !matches!(self, Self::Face)
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            blemishes: true,
            eyes: true,
            teeth: true,
            refine: true,
            scope: Scope::Face,
            settings: crate::retouch_settings::Settings::default(),
        }
    }
}

impl Options {
    /// Clamp values a caller might send out of range.
    #[must_use]
    pub fn sanitised(self) -> Self {
        Self {
            intensity: if self.intensity.is_finite() {
                self.intensity.clamp(0.25, 1.5)
            } else {
                1.0
            },
            settings: self.settings.sanitised(),
            ..self
        }
    }
}

/// Mean absolute departure of luminance from its local mean at `k` pixels: texture and lines at
/// that scale. Skin-coloured pixels only, so hair, brows and shadows do not count.
fn line_energy(zone: Capsule, g: &Geometry, skin: &SkinReference, px: &Pixels<'_>) -> Option<f32> {
    let k = ((0.03 * g.d).round() as usize).max(2);
    let mut sum = 0.0_f32;
    let mut n = 0.0_f32;
    zone.each(px, |x, y| {
        let p = px.linear(x, y);
        if !skin.affine(p) {
            return;
        }
        let (mut local, mut m) = (0.0_f32, 0.0_f32);
        let step = (k / 3).max(1);
        for yy in (y.saturating_sub(k)..=(y + k).min(px.height - 1)).step_by(step) {
            for xx in (x.saturating_sub(k)..=(x + k).min(px.width - 1)).step_by(step) {
                local += luma(px.linear(xx, yy));
                m += 1.0;
            }
        }
        let mean = local / m.max(1.0);
        sum += (luma(p) - mean).abs() / mean.max(1e-4);
        n += 1.0;
    });
    (n >= 20.0).then(|| sum / n)
}

fn mean_encoded_redness(zone: Capsule, skin: &SkinReference, px: &Pixels<'_>) -> Option<f32> {
    let mut sum = 0.0_f32;
    let mut n = 0.0_f32;
    zone.each(px, |x, y| {
        let p = px.linear(x, y);
        let l = luma(p);
        // Nostril shadows and highlights are not skin tone.
        if l < skin.luma * 0.5 || l > skin.luma * 1.8 {
            return;
        }
        let q = px.encoded(x, y);
        sum += (q[0] - (q[1] + q[2]) * 0.5) / q[0].max(q[1]).max(q[2]).max(1e-4);
        n += 1.0;
    });
    (n >= 12.0).then(|| sum / n)
}

/// Fine lines around the eyes and on the forehead, smile-line depth and local redness around
/// the nose: each softened only when it measures stronger than the same person's cheek.
fn refine(
    g: &Geometry,
    skin: &SkinReference,
    px: &Pixels<'_>,
    face: usize,
    prefix: &str,
    settings: &Settings,
    out: &mut FeatureEdits,
) {
    if g.d < 50.0 {
        return;
    }
    let d = g.d;
    let short = px.width.min(px.height) as f32;
    let cheek_zone = Capsule::disk(add(g.eyes[0], g.v, 0.6 * d), 0.14 * d);
    let cheek_zone_b = Capsule::disk(add(g.eyes[1], g.v, 0.6 * d), 0.14 * d);
    let cheek = match (
        line_energy(cheek_zone, g, skin, px),
        line_energy(cheek_zone_b, g, skin, px),
    ) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => return,
    };
    let mut parts = Vec::new();
    // 1. Fine lines: crow's feet beside each eye and lines across the forehead.
    let zones = [
        (
            "lines-a",
            Capsule::disk(add(add(g.eyes[0], g.u, -0.4 * d), g.v, 0.04 * d), 0.1 * d),
        ),
        (
            "lines-b",
            Capsule::disk(add(add(g.eyes[1], g.u, 0.4 * d), g.v, 0.04 * d), 0.1 * d),
        ),
        (
            "lines-forehead",
            Capsule {
                a: add(add(g.mid, g.v, -0.72 * d), g.u, -0.3 * d),
                b: add(add(g.mid, g.v, -0.72 * d), g.u, 0.3 * d),
                r: 0.09 * d,
            },
        ),
    ];
    let mut softened = 0;
    for (name, zone) in zones {
        let strength = gain(if name == "lines-forehead" {
            settings.forehead_lines
        } else {
            settings.crows_feet
        });
        if strength <= 0.0 {
            continue;
        }
        let Some(energy) = line_energy(zone, g, skin, px) else {
            continue;
        };
        let ratio = energy / cheek.max(1e-4);
        // A stronger setting also softens slightly fainter lines.
        if ratio < 1.35 - 0.15 * (strength - 1.0) {
            continue;
        }
        let [cx, cy] = zone.a;
        let mut edit = masked(
            base_edit(
                format!("{prefix}{face}-{name}"),
                Tool::Wrinkle,
                ((ratio - 1.2) * 0.5).clamp(0.15, 0.45) * strength,
                px,
                [
                    cx,
                    cy,
                    zone.r * 1.5 + distance(zone.a, zone.b),
                    zone.r * 1.5,
                ],
            ),
            px,
            &[zone],
        );
        edit.feather = 0.85;
        edit.tone = 0.8;
        edit.texture = 1.0;
        edit.radius = (0.012 * d / short).clamp(0.0005, 0.05);
        out.refine.push(edit);
        softened += 1;
    }
    // 1b. Fine lines under each eye, measured the same way.
    let under_strength = gain(settings.under_eye_lines);
    if under_strength > 0.0 {
        for (eye, name) in g
            .eyes
            .into_iter()
            .zip(["lines-undereye-a", "lines-undereye-b"])
        {
            let zone = Capsule {
                a: add(add(eye, g.v, 0.2 * d), g.u, -0.13 * d),
                b: add(add(eye, g.v, 0.2 * d), g.u, 0.13 * d),
                r: 0.055 * d,
            };
            let Some(energy) = line_energy(zone, g, skin, px) else {
                continue;
            };
            let ratio = energy / cheek.max(1e-4);
            if ratio < 1.3 - 0.15 * (under_strength - 1.0) {
                continue;
            }
            let [cx, cy] = [(zone.a[0] + zone.b[0]) * 0.5, (zone.a[1] + zone.b[1]) * 0.5];
            let mut edit = masked(
                base_edit(
                    format!("{prefix}{face}-{name}"),
                    Tool::Wrinkle,
                    ((ratio - 1.1) * 0.5).clamp(0.12, 0.4) * under_strength,
                    px,
                    [cx, cy, 0.25 * d, 0.15 * d],
                ),
                px,
                &[zone],
            );
            edit.feather = 0.9;
            edit.tone = 0.7;
            edit.radius = (0.01 * d / short).clamp(0.0005, 0.05);
            out.refine.push(edit);
            softened += 1;
        }
    }
    if softened > 0 {
        parts.push(format!(
            "softened fine lines in {softened} area{} where line texture measured stronger than the cheek (fine skin texture kept)",
            plural(softened)
        ));
    }
    // 2. Smile lines: lift the fold only where it is darker than the cheek beside it.
    let fold_strength = gain(settings.smile_lines);
    let mut folds = 0;
    for (corner, side, name) in [(g.mouth[0], -1.0, "fold-a"), (g.mouth[1], 1.0, "fold-b")] {
        if fold_strength <= 0.0 {
            break;
        }
        let fold = Capsule {
            a: add(g.nose, g.u, side * 0.28 * d),
            b: add(corner, g.u, side * 0.08 * d),
            r: 0.045 * d,
        };
        let beside = Capsule {
            a: add(fold.a, g.u, side * 0.12 * d),
            b: add(fold.b, g.u, side * 0.12 * d),
            r: 0.045 * d,
        };
        let mean = |c: Capsule| {
            let (mut sum, mut n) = (0.0_f32, 0.0_f32);
            c.each(px, |x, y| {
                let p = px.linear(x, y);
                if skin.affine(p) {
                    sum += luma(p);
                    n += 1.0;
                }
            });
            (n >= 12.0).then(|| sum / n)
        };
        if let (Some(f), Some(b)) = (mean(fold), mean(beside)) {
            let drop = 1.0 - f / b.max(1e-4);
            if drop > 0.08 - 0.03 * (fold_strength - 1.0) {
                let [cx, cy] = [(fold.a[0] + fold.b[0]) * 0.5, (fold.a[1] + fold.b[1]) * 0.5];
                let mut edit = masked(
                    base_edit(
                        format!("{prefix}{face}-{name}"),
                        Tool::MicroDodgeBurn,
                        ((drop - 0.05) * 2.5).clamp(0.15, 0.4) * fold_strength,
                        px,
                        [cx, cy, 0.3 * d, 0.3 * d],
                    ),
                    px,
                    &[fold],
                );
                edit.feather = 0.9;
                edit.radius = (0.03 * d / short).clamp(0.0005, 0.05);
                out.refine.push(edit);
                folds += 1;
            }
        }
    }
    if folds > 0 {
        parts.push(format!(
            "softened {folds} smile line{} that measured darker than the cheek beside {} (expression kept)",
            plural(folds),
            if folds == 1 { "it" } else { "them" }
        ));
    }
    // 3. Local redness around the nose wings, evened toward this person's own cheek colour.
    let reference = add(g.eyes[0], g.v, 0.6 * d);
    let cheek_red = mean_encoded_redness(Capsule::disk(reference, 0.1 * d), skin, px);
    let mut evened = 0;
    let red_strength = gain(settings.redness);
    for (side, name) in [(-1.0, "redness-a"), (1.0, "redness-b")] {
        if red_strength <= 0.0 {
            break;
        }
        let zone = Capsule::disk(add(add(g.nose, g.u, side * 0.2 * d), g.v, 0.0), 0.08 * d);
        if let (Some(zone_red), Some(base)) = (mean_encoded_redness(zone, skin, px), cheek_red) {
            let excess = zone_red - base;
            if excess > 0.05 - 0.02 * (red_strength - 1.0) {
                let (w, h) = (px.width as f32, px.height as f32);
                let mut edit = masked(
                    base_edit(
                        format!("{prefix}{face}-{name}"),
                        Tool::ColorMatch,
                        (excess * 5.0).clamp(0.2, 0.6) * red_strength,
                        px,
                        [zone.a[0], zone.a[1], 0.12 * d, 0.12 * d],
                    ),
                    px,
                    &[zone],
                );
                edit.source = Some([
                    (reference[0] / w).clamp(0.0, 1.0),
                    (reference[1] / h).clamp(0.0, 1.0),
                ]);
                edit.feather = 0.9;
                out.refine.push(edit);
                evened += 1;
            }
        }
    }
    if evened > 0 {
        parts.push(format!(
            "evened redness beside the nose on {evened} side{} toward the same person's cheek colour",
            plural(evened)
        ));
    }
    if !parts.is_empty() {
        out.report
            .findings
            .push(format!("Refine: {}.", parts.join("; ")));
    }
}

/// Box mean of `value` over masked pixels, using integral images.
struct MaskedMean {
    w: usize,
    sum: Vec<f64>,
    count: Vec<u32>,
}

impl MaskedMean {
    fn new(values: &[f32], mask: &[bool], w: usize, h: usize) -> Self {
        let stride = w + 1;
        let mut sum = vec![0.0_f64; stride * (h + 1)];
        let mut count = vec![0_u32; stride * (h + 1)];
        for y in 0..h {
            let mut row_sum = 0.0_f64;
            let mut row_count = 0_u32;
            for x in 0..w {
                let i = y * w + x;
                if mask.get(i).copied().unwrap_or(false) {
                    row_sum += f64::from(values.get(i).copied().unwrap_or(0.0));
                    row_count += 1;
                }
                let above = y * stride + x + 1;
                let here = (y + 1) * stride + x + 1;
                let s = sum.get(above).copied().unwrap_or(0.0) + row_sum;
                let c = count.get(above).copied().unwrap_or(0) + row_count;
                if let Some(slot) = sum.get_mut(here) {
                    *slot = s;
                }
                if let Some(slot) = count.get_mut(here) {
                    *slot = c;
                }
            }
        }
        Self { w, sum, count }
    }

    fn mean(&self, x: usize, y: usize, r: usize, h: usize) -> Option<(f32, u32)> {
        let stride = self.w + 1;
        let x0 = x.saturating_sub(r);
        let y0 = y.saturating_sub(r);
        let x1 = (x + r + 1).min(self.w);
        let y1 = (y + r + 1).min(h);
        let at = |xx: usize, yy: usize| {
            (
                self.sum.get(yy * stride + xx).copied().unwrap_or(0.0),
                self.count.get(yy * stride + xx).copied().unwrap_or(0),
            )
        };
        let (a, ca) = at(x1, y1);
        let (b, cb) = at(x0, y1);
        let (c, cc) = at(x1, y0);
        let (d, cd) = at(x0, y0);
        let n = (ca + cd).saturating_sub(cb + cc);
        (n > 0).then(|| (((a - b - c + d) / f64::from(n)) as f32, n))
    }
}

/// Median absolute deviation, scaled to a standard deviation.
fn robust_spread(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mid = values.len() / 2;
    values.select_nth_unstable_by(mid, f32::total_cmp);
    let median = values.get(mid).copied().unwrap_or(0.0);
    let mut deviations: Vec<f32> = values.iter().map(|v| (v - median).abs()).collect();
    deviations.select_nth_unstable_by(mid, f32::total_cmp);
    deviations.get(mid).copied().unwrap_or(0.0) * 1.4826
}

fn blemishes(
    g: &Geometry,
    skin: &SkinReference,
    px: &Pixels<'_>,
    face: usize,
    prefix: &str,
    settings: &Settings,
    out: &mut FeatureEdits,
) {
    let [bl, bt, br, bb] = g.bounds;
    let x0 = bl.floor().max(0.0) as usize;
    let y0 = bt.floor().max(0.0) as usize;
    let x1 = (br.ceil().max(0.0) as usize).min(px.width);
    let y1 = (bb.ceil().max(0.0) as usize).min(px.height);
    if x1 <= x0 + 8 || y1 <= y0 + 8 {
        return;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let areas = g.skin_areas();
    let exclusions = g.exclusions();
    let mut lum = vec![0.0_f32; w * h];
    let mut red = vec![0.0_f32; w * h];
    // `region` is where a mark may be; `clean` is the skin its background is measured from.
    let mut region = vec![false; w * h];
    let mut clean = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let p = px.linear(x0 + x, y0 + y);
            let point = [(x0 + x) as f32 + 0.5, (y0 + y) as f32 + 0.5];
            let i = y * w + x;
            let inside = areas.iter().any(|a| a.contains(point))
                && !exclusions.iter().any(|e| e.contains(point));
            if let (Some(l), Some(r), Some(m), Some(c)) = (
                lum.get_mut(i),
                red.get_mut(i),
                region.get_mut(i),
                clean.get_mut(i),
            ) {
                *l = luma(p);
                *r = (p[0] - (p[1] + p[2]) * 0.5) / (p[0] + p[1] + p[2]).max(1e-4);
                *m = inside;
                *c = inside && skin.affine(p);
            }
        }
    }
    let k = ((0.07 * g.d).round() as usize).max(3);
    let lum_bg = MaskedMean::new(&lum, &clean, w, h);
    let red_bg = MaskedMean::new(&red, &clean, w, h);
    let at = |v: &[f32], x: usize, y: usize| v.get(y * w + x).copied().unwrap_or(0.0);
    let flag = |v: &[bool], x: usize, y: usize| v.get(y * w + x).copied().unwrap_or(false);
    // Redness above and darkness below the surrounding skin. The background excludes
    // non-skin pixels, so a nearby brow or strand of hair cannot fake a spot beside it.
    let mut excess = vec![0.0_f32; w * h];
    let mut dark = vec![0.0_f32; w * h];
    let mut valid = vec![false; w * h];
    let mut skin_excess = Vec::new();
    let mut skin_dark = Vec::new();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            if !flag(&region, x, y) {
                continue;
            }
            let (Some((lb, n)), Some((rb, _))) = (lum_bg.mean(x, y, k, h), red_bg.mean(x, y, k, h))
            else {
                continue;
            };
            if n < (k * k) as u32 {
                continue;
            }
            let i = y * w + x;
            let e = at(&red, x, y) - rb;
            let dk = (lb - at(&lum, x, y)) / lb.max(1e-4);
            if let (Some(es), Some(ds), Some(vs)) =
                (excess.get_mut(i), dark.get_mut(i), valid.get_mut(i))
            {
                *es = e;
                *ds = dk;
                *vs = true;
            }
            if flag(&clean, x, y) {
                skin_excess.push(e);
                skin_dark.push(dk);
            }
        }
    }
    // Thresholds follow this face's own texture: a grainy or strongly lit face needs a
    // larger departure before a pixel counts as a mark. Bounded both ways.
    // The photographer's sensitivity scales all three thresholds together.
    let sensitivity = settings.blemish_sensitivity;
    let te = threshold(
        (robust_spread(skin_excess) * 4.0).clamp(0.035, 0.1),
        sensitivity,
    );
    let spread_dark = robust_spread(skin_dark);
    let td = threshold((spread_dark * 2.5).clamp(0.04, 0.2), sensitivity);
    let tm = threshold((spread_dark * 3.5).max(0.3), sensitivity);
    let mut score = vec![0.0_f32; w * h];
    for (i, slot) in score.iter_mut().enumerate() {
        let (e, dk) = (
            excess.get(i).copied().unwrap_or(0.0),
            dark.get(i).copied().unwrap_or(0.0),
        );
        if valid.get(i).copied().unwrap_or(false) && ((e > te && dk > td) || dk > tm) {
            *slot = e.max(0.0) + dk * 0.5;
        }
    }
    let local_max = |x: usize, y: usize| {
        let c = at(&score, x, y);
        c > 0.0
            && (0..9_usize).all(|n| {
                let (dx, dy) = (n % 3, n / 3);
                let other = at(&score, x + dx - 1, y + dy - 1);
                (dx == 1 && dy == 1) || other < c || (other <= c && (dy, dx) > (1, 1))
            })
    };
    let r_max = (0.045 * g.d).max(3.0);
    let mut peaks = Vec::new();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            if local_max(x, y) {
                peaks.push((at(&score, x, y), x, y));
            }
        }
    }
    peaks.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));
    let mut chosen: Vec<([f32; 2], f32)> = Vec::new();
    let mut kept_marks: Vec<[f32; 2]> = Vec::new();
    for (_, x, y) in peaks.into_iter().take(4000) {
        let e_peak = at(&excess, x, y);
        let d_peak = at(&dark, x, y);
        // Much darker than the skin around it, or darker without being redder: more like a
        // mole, freckle or beauty mark than a blemish. Measured, recorded, never removed.
        let mole = d_peak > 0.5 || e_peak <= te;
        let grows = |nx: usize, ny: usize| {
            if mole {
                at(&dark, nx, ny) > d_peak * 0.5
            } else {
                at(&excess, nx, ny) > e_peak * 0.5
            }
        };
        let mut stack = vec![(x, y)];
        let mut seen = std::collections::BTreeSet::new();
        seen.insert((x, y));
        let mut members = Vec::new();
        let mut too_large = false;
        let area_cap = (std::f32::consts::PI * r_max * r_max * 1.2) as usize;
        while let Some((cx, cy)) = stack.pop() {
            members.push((cx as f32, cy as f32));
            if members.len() > area_cap {
                too_large = true;
                break;
            }
            for (nx, ny) in [
                (cx.wrapping_sub(1), cy),
                (cx + 1, cy),
                (cx, cy.wrapping_sub(1)),
                (cx, cy + 1),
            ] {
                if nx < w
                    && ny < h
                    && flag(&valid, nx, ny)
                    && grows(nx, ny)
                    && seen.insert((nx, ny))
                {
                    stack.push((nx, ny));
                }
            }
        }
        if too_large {
            continue;
        }
        let area = members.len();
        if area < 3 {
            continue;
        }
        // Compact, not a crease or a line: the spread along the long axis may be at most
        // about twice that along the short one.
        let n = area as f32;
        let (mx, my) = (
            members.iter().map(|m| m.0).sum::<f32>() / n,
            members.iter().map(|m| m.1).sum::<f32>() / n,
        );
        let (mut sxx, mut syy, mut sxy) = (0.0_f32, 0.0_f32, 0.0_f32);
        for (mx_, my_) in &members {
            sxx += (mx_ - mx).powi(2);
            syy += (my_ - my).powi(2);
            sxy += (mx_ - mx) * (my_ - my);
        }
        let (sxx, syy, sxy) = (sxx / n, syy / n, sxy / n);
        // Measured from the spot's own centre, not from its brightest pixel, so a flat-topped
        // mark is judged by its real extent.
        if members
            .iter()
            .any(|(px_, py_)| distance([*px_, *py_], [mx, my]) > r_max)
        {
            continue;
        }
        let half = (sxx + syy) * 0.5;
        let root = (((sxx - syy) * 0.5).powi(2) + sxy * sxy).sqrt();
        let elongation = ((half + root).max(1e-6) / (half - root).max(1e-6)).sqrt();
        if elongation > 2.2 {
            continue;
        }
        let r = (n / std::f32::consts::PI).sqrt().max(1.0);
        let centre = [x0 as f32 + mx + 0.5, y0 as f32 + my + 0.5];
        // The ring around a real spot is skin; around an edge or a strand it is not.
        let (mut ring, mut ring_clean) = (0_usize, 0_usize);
        let local = [mx + 0.5, my + 0.5];
        let outer = 2.4 * r + 2.0;
        let inner = 1.4 * r + 1.0;
        let reach = outer.ceil() as usize;
        let (ux, uy) = (mx.round().max(0.0) as usize, my.round().max(0.0) as usize);
        for yy in uy.saturating_sub(reach + 1)..(uy + reach + 2).min(h) {
            for xx in ux.saturating_sub(reach + 1)..(ux + reach + 2).min(w) {
                let dist = distance([xx as f32 + 0.5, yy as f32 + 0.5], local);
                if dist <= outer && dist > inner {
                    ring += 1;
                    ring_clean += usize::from(flag(&clean, xx, yy));
                }
            }
        }
        if ring == 0 || (ring_clean as f32) < ring as f32 * 0.8 {
            continue;
        }
        if mole {
            if kept_marks
                .iter()
                .all(|m| distance(*m, centre) > r_max * 2.0)
            {
                kept_marks.push(centre);
            }
            continue;
        }
        if chosen
            .iter()
            .any(|(c, cr)| distance(*c, centre) < (r + cr) * 3.0)
        {
            continue;
        }
        chosen.push((centre, r));
    }
    // Many small red marks together read as freckles or a skin condition, not blemishes:
    // that is somebody's face, so none of them is removed automatically.
    let field = if settings.keep_freckles {
        FRECKLE_FIELD
    } else {
        FRECKLE_FIELD * 3
    };
    let freckles = chosen.len() > field;
    if freckles {
        chosen.clear();
    }
    chosen.truncate(usize::from(settings.max_spots).min(MAX_SPOTS * 2));
    for (n, (centre, r)) in chosen.iter().enumerate() {
        let repair = (r * 1.8).max(2.5);
        let mut edit = base_edit(
            format!("{prefix}{face}-spot-{n}"),
            Tool::PatchHeal,
            0.9,
            px,
            [centre[0], centre[1], repair, repair],
        );
        edit.feather = 0.6;
        edit.source = donor(*centre, repair, g, skin, px);
        out.blemishes.push(edit);
    }
    out.report.spots_healed = out.blemishes.len();
    out.report.marks_kept = kept_marks.len();
    if freckles {
        out.report.findings.push(format!(
            "Blemishes: found more than {field} small marks, which reads as freckles or a skin pattern; none were removed. Heal individual spots by hand if you want to."
        ));
    } else if out.blemishes.is_empty() {
        out.report.findings.push(
            "Blemishes: no temporary-looking spots found (a spot must be small, round and redder than the skin around it)."
                .into(),
        );
    } else {
        out.report.findings.push(format!(
            "Blemishes: healed {} small spot{} that {} redder than the surrounding skin.",
            out.blemishes.len(),
            plural(out.blemishes.len()),
            if out.blemishes.len() == 1 {
                "was"
            } else {
                "were"
            }
        ));
    }
    if !kept_marks.is_empty() {
        out.report.findings.push(format!(
            "Kept {} darker mark{} that may be permanent (mole, freckle or beauty mark).",
            kept_marks.len(),
            plural(kept_marks.len())
        ));
    }
}

/// A nearby, clean, skin-coloured donor for a spot, or `None` to let the renderer search.
fn donor(
    centre: [f32; 2],
    radius: f32,
    g: &Geometry,
    skin: &SkinReference,
    px: &Pixels<'_>,
) -> Option<[f32; 2]> {
    let exclusions = g.exclusions();
    let areas = g.skin_areas();
    let reach = (radius * 2.4).max(0.03 * g.d);
    let mut best: Option<(f32, [f32; 2])> = None;
    for step in 0..8 {
        let angle = std::f32::consts::TAU * step as f32 / 8.0;
        let c = [
            centre[0] + angle.cos() * reach,
            centre[1] + angle.sin() * reach,
        ];
        let mut values = Vec::new();
        let mut ok = true;
        Capsule::disk(c, radius * 1.3).each(px, |x, y| {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let p = px.linear(x, y);
            if !skin.affine(p)
                || exclusions.iter().any(|e| e.contains(point))
                || !areas.iter().any(|a| a.contains(point))
            {
                ok = false;
            }
            values.push(luma(p));
        });
        if !ok || values.len() < 4 {
            continue;
        }
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32;
        let cost =
            var.sqrt() / mean.max(1e-4) + ((mean - skin.luma) / skin.luma.max(1e-4)).abs() * 0.3;
        if best.is_none_or(|(b, _)| cost < b) {
            best = Some((cost, c));
        }
    }
    let (w, h) = (px.width as f32, px.height as f32);
    best.map(|(_, [x, y])| [(x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0)])
}

#[allow(clippy::too_many_arguments)]
fn eyes(
    g: &Geometry,
    skin: &SkinReference,
    px: &Pixels<'_>,
    face: usize,
    prefix: &str,
    exposure: f32,
    settings: &Settings,
    out: &mut FeatureEdits,
) {
    if g.d < 40.0 {
        out.report
            .findings
            .push("Eyes are too small at analysis resolution for automatic eye finishing.".into());
        return;
    }
    let short = px.width.min(px.height) as f32;
    let d = g.d;
    let names = ["a", "b"];
    let mut cleaned = 0;
    let mut detailed = 0;
    let mut red_eye = 0;
    let mut lifted = 0;
    let mut closed = 0;
    for (eye, name) in g.eyes.into_iter().zip(names) {
        // The landmark sits near the lower lid; the opening is centred slightly above it.
        let centre = add(eye, g.v, -0.03 * d);
        let opening = Capsule {
            a: add(centre, g.u, -0.13 * d),
            b: add(centre, g.u, 0.13 * d),
            r: 0.08 * d,
        };
        // Sclera: clearly less saturated than this person's own cheek and not much darker,
        // measured in display values (linear light exaggerates the chroma of a white).
        // Measured per side of the iris, so a correction can be placed on the white only.
        let (cheek_luma, cheek_sat) = {
            let (mut l, mut sat, mut n) = (0.0_f32, Vec::new(), 0.0_f32);
            Capsule::disk(add(eye, g.v, 0.55 * d), 0.08 * d).each(px, |x, y| {
                let q = px.encoded(x, y);
                let max = q[0].max(q[1]).max(q[2]).max(1e-4);
                l += q[0] * 0.2126 + q[1] * 0.7152 + q[2] * 0.0722;
                sat.push((max - q[0].min(q[1]).min(q[2])) / max);
                n += 1.0;
            });
            sat.sort_by(f32::total_cmp);
            (
                l / n.max(1.0),
                sat.get(sat.len() / 2).copied().unwrap_or(0.3),
            )
        };
        let sat_limit = (cheek_sat * 0.85).min(0.4);
        let mut sides: [Vec<([f32; 2], [f32; 3])>; 2] = [Vec::new(), Vec::new()];
        let mut total = 0_usize;
        opening.each(px, |x, y| {
            total += 1;
            let q = px.encoded(x, y);
            let max = q[0].max(q[1]).max(q[2]).max(1e-4);
            let min = q[0].min(q[1]).min(q[2]);
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let along = (point[0] - centre[0]) * g.u[0] + (point[1] - centre[1]) * g.u[1];
            let l = q[0] * 0.2126 + q[1] * 0.7152 + q[2] * 0.0722;
            if l > cheek_luma * 0.6
                && (max - min) / max < sat_limit
                && max < 0.99
                && along.abs() > 0.06 * d
            {
                if let Some(side) = sides.get_mut(usize::from(along > 0.0)) {
                    side.push((point, q));
                }
            }
        });
        let found: usize = sides.iter().map(Vec::len).sum();
        let open = total > 0 && found as f32 / total as f32 > 0.02 && found >= 6;
        if !open {
            closed += 1;
            continue;
        }
        let redness = sides
            .iter()
            .flatten()
            .map(|(_, p)| (p[0] - (p[1] + p[2]) * 0.5) / p[0].max(p[1]).max(p[2]).max(1e-4))
            .sum::<f32>()
            / found as f32;
        let patches: Vec<Capsule> = sides
            .iter()
            .filter(|side| side.len() >= 3)
            .map(|side| {
                let n = side.len() as f32;
                let c = [
                    side.iter().map(|(q, _)| q[0]).sum::<f32>() / n,
                    side.iter().map(|(q, _)| q[1]).sum::<f32>() / n,
                ];
                Capsule::disk(c, 0.05 * d)
            })
            .collect();
        // Whites of the eyes: a small lift of the sclera only, bounded so they never go paper
        // white. Measured patches beside the iris, never the iris or the lids.
        if settings.eye_whitening > 0.0 && !patches.is_empty() {
            let mut edit = masked(
                base_edit(
                    format!("{prefix}{face}-eye-{name}-white"),
                    Tool::Dodge,
                    settings.eye_whitening * 0.3,
                    px,
                    [centre[0], centre[1], 0.22 * d, 0.22 * d],
                ),
                px,
                &patches,
            );
            edit.feather = 0.6;
            edit.selection = Some(brighter_than(skin.stops(exposure) - 0.9, 0.5));
            out.eyes.push(edit);
        }
        // A healthy white measures about 0.1-0.25 here; only a clearly red one is cleaned.
        let vessels = gain(settings.eye_vessels);
        if vessels > 0.0 && redness > 0.3 - 0.06 * (vessels - 1.0) {
            let amount = ((redness - 0.25) * 2.0).clamp(0.15, 0.6) * vessels;
            let patches: Vec<Capsule> = sides
                .iter()
                .filter(|side| side.len() >= 3)
                .map(|side| {
                    let n = side.len() as f32;
                    let c = [
                        side.iter().map(|(q, _)| q[0]).sum::<f32>() / n,
                        side.iter().map(|(q, _)| q[1]).sum::<f32>() / n,
                    ];
                    Capsule::disk(c, 0.05 * d)
                })
                .collect();
            if !patches.is_empty() {
                let mut edit = masked(
                    base_edit(
                        format!("{prefix}{face}-eye-{name}-clean"),
                        Tool::EyeClean,
                        amount,
                        px,
                        [centre[0], centre[1], 0.22 * d, 0.22 * d],
                    ),
                    px,
                    &patches,
                );
                edit.feather = 0.5;
                edit.selection = Some(brighter_than(skin.stops(exposure) - 0.9, 0.5));
                out.eyes.push(edit);
                cleaned += 1;
            }
        }
        // Iris and lash detail: a small fine-band contrast lift inside the opening only.
        let iris = gain(settings.iris_detail);
        if iris > 0.0 {
            let mut detail = masked(
                base_edit(
                    format!("{prefix}{face}-eye-{name}-detail"),
                    Tool::EyeDetail,
                    if d >= 70.0 { 0.35 } else { 0.25 } * iris,
                    px,
                    [eye[0], eye[1], 0.2 * d, 0.2 * d],
                ),
                px,
                &[Capsule::disk(centre, 0.11 * d)],
            );
            detail.feather = 0.6;
            detail.radius = (0.012 * d / short).clamp(0.0005, 0.05);
            out.eyes.push(detail);
            detailed += 1;
        }
        // Iris brilliance: a soft lift of the iris itself.
        if settings.iris_brightness > 0.0 {
            let mut edit = masked(
                base_edit(
                    format!("{prefix}{face}-eye-{name}-iris"),
                    Tool::Dodge,
                    settings.iris_brightness * 0.35,
                    px,
                    [centre[0], centre[1], 0.12 * d, 0.12 * d],
                ),
                px,
                &[Capsule::disk(centre, 0.065 * d)],
            );
            edit.feather = 0.8;
            out.eyes.push(edit);
        }
        // Lash line: fine detail and a whisper of depth along the upper lid.
        if settings.lash_definition > 0.0 {
            let lid = Capsule {
                a: add(add(centre, g.u, -0.15 * d), g.v, -0.06 * d),
                b: add(add(centre, g.u, 0.15 * d), g.v, -0.06 * d),
                r: 0.035 * d,
            };
            let mut detail = masked(
                base_edit(
                    format!("{prefix}{face}-eye-{name}-lash"),
                    Tool::Frequency,
                    0.85,
                    px,
                    [centre[0], centre[1], 0.22 * d, 0.12 * d],
                ),
                px,
                &[lid],
            );
            detail.tone = 0.0;
            detail.texture = 1.0 + settings.lash_definition * 0.8;
            detail.feather = 0.7;
            detail.radius = (0.01 * d / short).clamp(0.0005, 0.05);
            out.eyes.push(detail);
            let mut depth = masked(
                base_edit(
                    format!("{prefix}{face}-eye-{name}-lash-depth"),
                    Tool::Burn,
                    settings.lash_definition * 0.12,
                    px,
                    [centre[0], centre[1], 0.22 * d, 0.12 * d],
                ),
                px,
                &[lid],
            );
            depth.feather = 0.8;
            out.eyes.push(depth);
        }
        // Brow definition: fine detail in the brow, never a change of its shape.
        if settings.brow_definition > 0.0 {
            let brow = Capsule {
                a: add(add(eye, g.v, -0.33 * d), g.u, -0.2 * d),
                b: add(add(eye, g.v, -0.36 * d), g.u, 0.2 * d),
                r: 0.07 * d,
            };
            let mut edit = masked(
                base_edit(
                    format!("{prefix}{face}-eye-brow-{name}"),
                    Tool::Frequency,
                    0.85,
                    px,
                    [eye[0], eye[1] - 0.35 * d, 0.3 * d, 0.15 * d],
                ),
                px,
                &[brow],
            );
            edit.tone = 0.0;
            edit.texture = 1.0 + settings.brow_definition * 0.7;
            edit.feather = 0.8;
            edit.radius = (0.012 * d / short).clamp(0.0005, 0.05);
            out.eyes.push(edit);
        }
        // Flash red-eye: the pupil itself is strongly and dominantly red.
        let mut red_px = 0_usize;
        let mut pupil = 0_usize;
        Capsule::disk(centre, 0.06 * d).each(px, |x, y| {
            pupil += 1;
            let p = px.encoded(x, y);
            // Flash red-eye is a red with green and blue about equal; a brown iris has far
            // less blue than green and must never be read as red-eye.
            if p[0] > 0.3 && p[0] > p[1].max(p[2]) * 1.8 && p[2] >= p[1] * 0.6 {
                red_px += 1;
            }
        });
        if settings.red_eye && pupil > 0 && red_px as f32 / pupil as f32 > 0.3 {
            out.eyes.push(masked(
                base_edit(
                    format!("{prefix}{face}-eye-{name}-redeye"),
                    Tool::RedEye,
                    0.9,
                    px,
                    [eye[0], eye[1], 0.1 * d, 0.1 * d],
                ),
                px,
                &[Capsule::disk(centre, 0.085 * d)],
            ));
            red_eye += 1;
        }
        // Under-eye: lift only when this area is darker than the same person's cheek.
        let under = Capsule {
            a: add(add(eye, g.v, 0.27 * d), g.u, -0.12 * d),
            b: add(add(eye, g.v, 0.27 * d), g.u, 0.12 * d),
            r: 0.07 * d,
        };
        let cheek = Capsule::disk(add(eye, g.v, 0.6 * d), 0.1 * d);
        let mean_luma = |c: Capsule| {
            let mut sum = 0.0;
            let mut n = 0.0;
            c.each(px, |x, y| {
                let p = px.linear(x, y);
                if skin.affine(p) {
                    sum += luma(p);
                    n += 1.0;
                }
            });
            (n >= 6.0).then(|| sum / n)
        };
        let circles = gain(settings.dark_circles);
        if let (Some(u_l), Some(c_l), true) = (mean_luma(under), mean_luma(cheek), circles > 0.0) {
            let drop = 1.0 - u_l / c_l.max(1e-4);
            if drop > 0.08 - 0.03 * (circles - 1.0) {
                let mut edit = masked(
                    base_edit(
                        format!("{prefix}{face}-undereye-{name}"),
                        Tool::UnderEye,
                        ((drop - 0.05) * 2.5).clamp(0.15, 0.55) * circles,
                        px,
                        [eye[0], eye[1], 0.3 * d, 0.3 * d],
                    ),
                    px,
                    &[under],
                );
                edit.feather = 0.85;
                edit.radius = (0.1 * d / short).clamp(0.0005, 0.05);
                out.eyes.push(edit);
                lifted += 1;
            }
        }
        // Eye bags: the puffy band below the shadow, evened rather than removed.
        if settings.eye_bags > 0.0 {
            let bag = Capsule {
                a: add(add(eye, g.v, 0.36 * d), g.u, -0.12 * d),
                b: add(add(eye, g.v, 0.36 * d), g.u, 0.12 * d),
                r: 0.07 * d,
            };
            let mut edit = masked(
                base_edit(
                    format!("{prefix}{face}-undereye-bag-{name}"),
                    Tool::MicroDodgeBurn,
                    settings.eye_bags * 0.5,
                    px,
                    [eye[0], eye[1] + 0.36 * d, 0.25 * d, 0.15 * d],
                ),
                px,
                &[bag],
            );
            edit.feather = 0.9;
            edit.radius = (0.035 * d / short).clamp(0.0005, 0.05);
            out.eyes.push(edit);
        }
    }
    let mut parts = Vec::new();
    if detailed > 0 {
        parts.push(format!(
            "added subtle iris detail to {detailed} eye{}",
            plural(detailed)
        ));
    }
    if cleaned > 0 {
        parts.push(format!(
            "reduced redness in {cleaned} eye{}",
            plural(cleaned)
        ));
    }
    if red_eye > 0 {
        parts.push(format!(
            "corrected flash red-eye in {red_eye} pupil{}",
            plural(red_eye)
        ));
    }
    if lifted > 0 {
        parts.push(format!(
            "lifted {lifted} under-eye shadow{} that measured darker than the cheek",
            plural(lifted)
        ));
    }
    if closed > 0 {
        parts.push(format!(
            "left {closed} eye{} alone because no open eye was visible",
            plural(closed)
        ));
    }
    if !parts.is_empty() {
        out.report
            .findings
            .push(format!("Eyes: {}.", parts.join("; ")));
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[allow(clippy::too_many_arguments)]
fn mouth(
    g: &Geometry,
    skin: &SkinReference,
    px: &Pixels<'_>,
    face: usize,
    prefix: &str,
    exposure: f32,
    settings: &Settings,
    out: &mut FeatureEdits,
) {
    if g.mouth_width < 20.0 {
        return;
    }
    let mw = g.mouth_width;
    // Lips: colour and definition, never on the teeth (which are brighter than the skin).
    let lips = Capsule {
        a: g.mouth[0],
        b: g.mouth[1],
        r: 0.2 * mw,
    };
    let not_teeth = Selection {
        inverted: false,
        gradient: None,
        luminance: Some(LuminanceRange {
            low: -16.0,
            high: (skin.stops(exposure) + 0.15).clamp(-16.0, 16.0),
            softness: 0.5,
        }),
    };
    if settings.lip_colour > 0.0 {
        let mut edit = masked(
            base_edit(
                format!("{prefix}{face}-lips-colour"),
                Tool::Makeup,
                settings.lip_colour * 0.5,
                px,
                [g.mouth_centre[0], g.mouth_centre[1], 0.6 * mw, 0.4 * mw],
            ),
            px,
            &[lips],
        );
        edit.warmth = 0.25;
        edit.tint = 0.45;
        edit.feather = 0.8;
        edit.selection = Some(not_teeth.clone());
        out.finishing.push(edit);
    }
    if settings.lip_definition > 0.0 {
        let mut edit = masked(
            base_edit(
                format!("{prefix}{face}-lips-detail"),
                Tool::Frequency,
                0.8,
                px,
                [g.mouth_centre[0], g.mouth_centre[1], 0.6 * mw, 0.4 * mw],
            ),
            px,
            &[lips],
        );
        edit.tone = 0.0;
        edit.texture = 1.0 + settings.lip_definition * 0.6;
        edit.feather = 0.8;
        edit.radius = (0.02 * mw / px.width.min(px.height) as f32).clamp(0.0005, 0.05);
        edit.selection = Some(not_teeth);
        out.finishing.push(edit);
    }
    let whitening = gain(settings.teeth_whitening);
    if whitening <= 0.0 {
        return;
    }
    let teeth_area = Capsule {
        a: add(g.mouth_centre, g.u, -0.28 * mw),
        b: add(g.mouth_centre, g.u, 0.28 * mw),
        r: 0.12 * mw,
    };
    let mut teeth = Vec::new();
    let mut total = 0_usize;
    teeth_area.each(px, |x, y| {
        total += 1;
        let p = px.linear(x, y);
        let max = p[0].max(p[1]).max(p[2]).max(1e-4);
        let min = p[0].min(p[1]).min(p[2]);
        // Brighter than the skin, low in chroma and not lip-red.
        if luma(p) > skin.luma * 1.1 && (max - min) / max < 0.75 && p[0] < p[1] * 1.6 {
            teeth.push(p);
        }
    });
    if total == 0 || (teeth.len() as f32) / (total as f32) < 0.12 || teeth.len() < 20 {
        out.report
            .findings
            .push("Teeth: none clearly visible, so no whitening was applied.".into());
        return;
    }
    let yellow = teeth
        .iter()
        .map(|p| ((p[0] + p[1]) * 0.5 - p[2]) / p[0].max(p[1]).max(p[2]).max(1e-4))
        .sum::<f32>()
        / teeth.len() as f32;
    if yellow <= 0.18 - 0.05 * (whitening - 1.0) {
        out.report
            .findings
            .push("Teeth: visible and already neutral, so they were left alone.".into());
        return;
    }
    let mut edit = masked(
        base_edit(
            format!("{prefix}{face}-teeth"),
            Tool::Teeth,
            ((yellow - 0.08) * 2.0).clamp(0.15, 0.45) * whitening,
            px,
            [g.mouth_centre[0], g.mouth_centre[1], 0.5 * mw, 0.5 * mw],
        ),
        px,
        &[teeth_area],
    );
    edit.feather = 0.6;
    edit.selection = Some(brighter_than(skin.stops(exposure) + 0.1, 0.6));
    out.finishing.push(edit);
    out.report.findings.push(format!(
        "Teeth: reduced a measured yellow cast ({:.0}% of the visible tooth area).",
        (teeth.len() as f32) / (total as f32) * 100.0
    ));
}

fn shine(
    g: &Geometry,
    skin: &SkinReference,
    px: &Pixels<'_>,
    face: usize,
    prefix: &str,
    settings: &Settings,
    out: &mut FeatureEdits,
) {
    let d = g.d;
    let mut zones: Vec<Capsule> = g.skin_areas().into_iter().take(3).collect();
    zones.push(Capsule::disk(add(g.nose, g.v, -0.15 * d), 0.16 * d));
    let exclusions = g.exclusions();
    let mut total = 0_usize;
    let mut specular = 0_usize;
    for zone in &zones {
        zone.each(px, |x, y| {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            if exclusions.iter().take(4).any(|e| e.contains(point)) {
                return;
            }
            total += 1;
            let p = px.linear(x, y);
            let max = p[0].max(p[1]).max(p[2]).max(1e-4);
            let min = p[0].min(p[1]).min(p[2]);
            if luma(p) > (skin.luma * 1.8).max(0.5) && (max - min) / max < 0.25 {
                specular += 1;
            }
        });
    }
    if total == 0 {
        return;
    }
    let fraction = specular as f32 / total as f32;
    // Under 1.5 % is normal skin sheen; over 15 % is the light itself, not oily shine.
    if !(0.015..=0.15).contains(&fraction) {
        return;
    }
    let [l, t, r, b] = g.bounds;
    let mut edit = masked(
        base_edit(
            format!("{prefix}{face}-shine"),
            Tool::Mattify,
            (fraction * 5.0).clamp(0.15, 0.4) * gain(settings.shine),
            px,
            [g.nose[0], g.nose[1], (r - l) * 0.5, (b - t) * 0.5],
        ),
        px,
        &zones,
    );
    edit.feather = 0.8;
    out.finishing.push(edit);
    out.report.findings.push(format!(
        "Shine: softened specular highlights covering {:.1}% of the skin.",
        fraction * 100.0
    ));
}

/// Portrait volumes and make-up: soft contour, highlight and blush on landmark zones, limited
/// to the person's segmented face skin when a matte is available. Each is off unless chosen.
fn sculpt(
    g: &Geometry,
    px: &Pixels<'_>,
    face: usize,
    prefix: &str,
    settings: &Settings,
    face_matte: Option<&str>,
    out: &mut FeatureEdits,
) {
    if g.d < 40.0
        || [settings.contour, settings.highlight, settings.blush]
            .iter()
            .all(|v| *v <= 0.0)
    {
        return;
    }
    // Without a segmented face, a soft shadow along the jaw would also darken the background.
    if face_matte.is_none() {
        out.report.findings.push(
            "Portrait volumes need AI skin detection, which was not available for this face; contour, highlight and blush were skipped."
                .into(),
        );
        return;
    }
    let d = g.d;
    let [l, t, r, b] = g.bounds;
    let region = [(l + r) * 0.5, (t + b) * 0.5, (r - l) * 0.6, (b - t) * 0.6];
    let mut push =
        |name: &str, tool: Tool, amount: f32, zones: Vec<Capsule>, warmth: f32, tint: f32| {
            if amount <= 0.0 {
                return;
            }
            let mut edit = masked(
                base_edit(format!("{prefix}{face}-{name}"), tool, amount, px, region),
                px,
                &zones,
            );
            edit.feather = 1.0;
            edit.warmth = warmth;
            edit.tint = tint;
            edit.matte = face_matte.map(str::to_owned);
            out.finishing.push(edit);
        };
    let sides = [(-1.0_f32, 0_usize), (1.0, 1)];
    let mut contour = Vec::new();
    let mut highlight = vec![
        Capsule {
            a: add(g.mid, g.v, 0.12 * d),
            b: add(g.nose, g.v, -0.12 * d),
            r: 0.06 * d,
        },
        Capsule::disk(add(g.mid, g.v, -0.62 * d), 0.17 * d),
        Capsule::disk(add(g.mouth_centre, g.v, 0.45 * d), 0.1 * d),
    ];
    let mut blush = Vec::new();
    for (side, k) in sides {
        let eye = g.eyes[k];
        let corner = g.mouth[k];
        contour.push(Capsule {
            a: add(add(g.mid, g.u, side * 0.78 * d), g.v, 0.72 * d),
            b: add(add(corner, g.u, side * 0.28 * d), g.v, -0.05 * d),
            r: 0.11 * d,
        });
        contour.push(Capsule {
            a: add(add(corner, g.u, side * 0.55 * d), g.v, 0.25 * d),
            b: add(add(g.mouth_centre, g.u, side * 0.35 * d), g.v, 0.72 * d),
            r: 0.09 * d,
        });
        highlight.push(Capsule::disk(
            add(add(eye, g.v, 0.42 * d), g.u, side * 0.16 * d),
            0.12 * d,
        ));
        blush.push(Capsule::disk(
            add(add(eye, g.v, 0.72 * d), g.u, side * 0.1 * d),
            0.19 * d,
        ));
    }
    push(
        "sculpt-contour",
        Tool::Burn,
        settings.contour * 0.3,
        contour,
        0.0,
        0.0,
    );
    push(
        "sculpt-highlight",
        Tool::Dodge,
        settings.highlight * 0.25,
        highlight,
        0.0,
        0.0,
    );
    push(
        "makeup-blush",
        Tool::Makeup,
        settings.blush * 0.4,
        blush,
        0.5,
        0.55,
    );
    let made = [settings.contour, settings.highlight, settings.blush]
        .iter()
        .filter(|v| **v > 0.0)
        .count();
    if made > 0 {
        out.report.findings.push(format!(
            "Portrait volumes: added {made} soft sculpting or blush layer{} you asked for, inside the face skin only.",
            plural(made)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::retouch_tools;

    fn face() -> PortraitFace {
        PortraitFace {
            bounds: [0.2, 0.1, 0.8, 0.95],
            landmarks: [
                [0.38, 0.4],
                [0.62, 0.4],
                [0.5, 0.55],
                [0.41, 0.7],
                [0.59, 0.7],
            ],
            confidence: 0.95,
        }
    }

    fn canvas(size: usize, skin: [u8; 3]) -> Vec<u8> {
        skin.repeat(size * size)
    }

    fn paint(rgb: &mut [u8], size: usize, centre: [f32; 2], r: f32, colour: [u8; 3]) {
        for y in 0..size {
            for x in 0..size {
                if distance([x as f32 + 0.5, y as f32 + 0.5], centre) <= r {
                    let i = (y * size + x) * 3;
                    rgb[i..i + 3].copy_from_slice(&colour);
                }
            }
        }
    }

    #[test]
    fn a_red_spot_is_healed_and_a_dark_mole_is_kept_on_every_complexion() {
        let size = 400;
        for (skin, spot, mole) in [
            ([232, 190, 170], [220, 150, 140], [95, 70, 60]),
            ([160, 110, 85], [158, 88, 72], [55, 38, 30]),
            ([92, 60, 45], [92, 40, 32], [30, 20, 16]),
        ] {
            let mut rgb = canvas(size, skin);
            // Left cheek: a redder spot. Right cheek: a much darker, not-redder mark.
            paint(
                &mut rgb,
                size,
                [0.38 * 400.0, 0.4 * 400.0 + 0.55 * 96.0],
                3.5,
                spot,
            );
            paint(
                &mut rgb,
                size,
                [0.62 * 400.0, 0.4 * 400.0 + 0.55 * 96.0],
                3.5,
                mole,
            );
            let px = Pixels::new(&rgb, size as u32, size as u32).unwrap();
            let plan = plan(&face(), 0, &px, 0.0, "p-", &Options::default(), None);
            assert_eq!(plan.report.spots_healed, 1, "{skin:?}: {:?}", plan.report);
            assert!(plan.report.marks_kept >= 1, "{skin:?}: {:?}", plan.report);
            let spot_edit = &plan.blemishes[0];
            assert!(
                spot_edit.region[0] < 0.5,
                "the red spot is on the left cheek"
            );
            let mut all = plan.blemishes.clone();
            all.extend(plan.eyes.clone());
            all.extend(plan.finishing.clone());
            retouch_tools::validate(&all).unwrap();
        }
    }

    #[test]
    fn a_clean_face_gets_no_blemish_or_teeth_edits_and_is_deterministic() {
        let rgb = canvas(300, [180, 130, 105]);
        let px = Pixels::new(&rgb, 300, 300).unwrap();
        let a = plan(&face(), 0, &px, 0.0, "p-", &Options::default(), None);
        assert!(a.blemishes.is_empty());
        assert!(a.finishing.is_empty());
        // A uniform canvas has no open eye (no sclera brighter than skin).
        assert!(a.eyes.is_empty(), "{:?}", a.report);
        let b = plan(&face(), 0, &px, 0.0, "p-", &Options::default(), None);
        assert_eq!(a.report, b.report);
    }

    #[test]
    fn open_eyes_get_detail_and_bloodshot_eyes_get_cleaned() {
        let size = 400;
        let mut rgb = canvas(size, [170, 120, 95]);
        let d = 96.0;
        for (x, sclera) in [
            (0.38 * 400.0, [235, 232, 228]),
            (0.62 * 400.0, [236, 165, 160]),
        ] {
            let c = [x, 160.0];
            for dx in [-0.1, -0.05, 0.05, 0.1] {
                paint(&mut rgb, size, [c[0] + dx * d, c[1]], 0.05 * d, sclera);
            }
            paint(&mut rgb, size, c, 0.035 * d, [40, 30, 25]);
        }
        let px = Pixels::new(&rgb, size as u32, size as u32).unwrap();
        let plan = plan(&face(), 0, &px, 0.0, "p-", &Options::default(), None);
        let tools: Vec<_> = plan.eyes.iter().map(|e| (e.id.clone(), e.tool)).collect();
        assert_eq!(
            tools.iter().filter(|(_, t)| *t == Tool::EyeDetail).count(),
            2,
            "{tools:?}"
        );
        let cleaned: Vec<_> = tools.iter().filter(|(_, t)| *t == Tool::EyeClean).collect();
        assert_eq!(cleaned.len(), 1, "{tools:?}");
        assert!(cleaned[0].0.ends_with("-b-clean"));
        retouch_tools::validate(&plan.eyes).unwrap();
    }

    #[test]
    fn yellow_teeth_are_whitened_and_neutral_teeth_are_left_alone() {
        let size = 400;
        for (teeth, expect) in [([230, 205, 140], true), ([238, 236, 232], false)] {
            let mut rgb = canvas(size, [160, 112, 90]);
            for dx in -6..=6 {
                paint(&mut rgb, size, [200.0 + dx as f32 * 3.0, 280.0], 4.5, teeth);
            }
            let px = Pixels::new(&rgb, size as u32, size as u32).unwrap();
            let plan = plan(&face(), 0, &px, 0.0, "p-", &Options::default(), None);
            let whitened = plan.finishing.iter().any(|e| e.tool == Tool::Teeth);
            assert_eq!(whitened, expect, "{:?}", plan.report);
        }
    }

    #[test]
    fn measured_lines_folds_and_nose_redness_are_softened_and_options_are_respected() {
        let size = 400;
        let skin = [170, 125, 100];
        let mut rgb = canvas(size, skin);
        let d = 96.0;
        let (eye_a, nose, mouth_a) = ([0.38 * 400.0, 160.0], [200.0, 220.0], [164.0, 280.0]);
        // Crow's feet beside eye a: thin darker lines at the line scale.
        let c = [eye_a[0] - 0.4 * d, eye_a[1] + 0.04 * d];
        for k in -3..=3 {
            let y = c[1] + k as f32 * 3.0;
            for x in (c[0] - 8.0) as usize..(c[0] + 8.0) as usize {
                let i = (y as usize * size + x) * 3;
                rgb[i..i + 3].copy_from_slice(&[140, 100, 80]);
            }
        }
        // A smile line from the nose wing to the mouth corner, darker than the cheek beside it.
        for t in 0..60 {
            let f = t as f32 / 59.0;
            let a = [nose[0] - 0.28 * d, nose[1]];
            let b = [mouth_a[0] - 0.08 * d, mouth_a[1]];
            paint(
                &mut rgb,
                size,
                [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f],
                3.0,
                [135, 98, 80],
            );
        }
        // Redness beside the other nose wing.
        paint(
            &mut rgb,
            size,
            [nose[0] + 0.2 * d, nose[1]],
            0.07 * d,
            [185, 110, 95],
        );
        let px = Pixels::new(&rgb, size as u32, size as u32).unwrap();
        let all = plan(&face(), 0, &px, 0.0, "p-", &Options::default(), None);
        let ids: Vec<_> = all.refine.iter().map(|e| (e.id.clone(), e.tool)).collect();
        assert!(
            ids.iter()
                .any(|(id, t)| id.ends_with("-lines-a") && *t == Tool::Wrinkle),
            "{ids:?} {:?}",
            all.report
        );
        assert!(
            ids.iter()
                .any(|(id, t)| id.ends_with("-fold-a") && *t == Tool::MicroDodgeBurn),
            "{ids:?}"
        );
        assert!(
            ids.iter()
                .any(|(id, t)| id.ends_with("-redness-b") && *t == Tool::ColorMatch),
            "{ids:?}"
        );
        retouch_tools::validate(&all.refine).unwrap();
        let off = plan(
            &face(),
            0,
            &px,
            0.0,
            "p-",
            &Options {
                refine: false,
                ..Options::default()
            },
            None,
        );
        assert!(off.refine.is_empty());
        let gentle = plan(
            &face(),
            0,
            &px,
            0.0,
            "p-",
            &Options {
                intensity: 0.5,
                ..Options::default()
            },
            None,
        );
        for (a, b) in all.refine.iter().zip(&gentle.refine) {
            assert!(b.amount < a.amount, "{} {} {}", a.id, a.amount, b.amount);
        }
    }

    #[test]
    fn tiny_faces_are_skipped_with_a_reason() {
        let rgb = canvas(60, [180, 130, 105]);
        let px = Pixels::new(&rgb, 60, 60).unwrap();
        let plan = plan(&face(), 0, &px, 0.0, "p-", &Options::default(), None);
        assert!(plan.blemishes.is_empty() && plan.eyes.is_empty() && plan.finishing.is_empty());
        assert!(!plan.report.findings.is_empty());
    }
}
