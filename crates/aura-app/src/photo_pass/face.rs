//! Batch 2 of the photo pass: faces and skin, checked on the edited photograph (steps 26-50).
//! ADR-0186.
//!
//! Auto enhance already retouches every face it finds - smoothing, tone, blemishes, lines,
//! under-eyes, teeth, shine, body skin matched to the face. These steps do not retouch again.
//! They render the edit twice - with its retouch operations and without them - and read each
//! person in the renderer's own parse of the faces, so every judgement compares the photograph
//! with itself: the skin after the grade against the same skin before it, the retouched
//! under-eye against the same person's cheek, never a face against an ideal complexion.
//!
//! What they change goes through Auto enhance's own strength factors (ADR-0130) when Auto enhance
//! went too far, or is one operation limited to that person's region by a matte. What cannot be
//! repaired automatically - a soft eye, a ceremonial mark a retouch touched - is reported with
//! where it is.
// Planes and buffers are indexed by positions derived from their own sizes; statistics convert
// freely between pixel counts and f32.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::collections::BTreeMap;

use super::scene::{hue, saturation, Scene};
use super::{Change, Finding, StepDef, STEPS};
use crate::advanced_retouch::{base_edit, measure};
use aura_core::{AuraResult, PhotoId};
use aura_portrait::{FaceGeometry, PortraitMap, Region};
use aura_recipe::retouch_intensity::{self as intensity, Category as Kind};
use aura_recipe::retouch_tools::{self, Edit, Matte, Tool};
use aura_recipe::Recipe;

/// The edit rendered twice at the same size, with and without its retouch operations.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// Every global setting, no retouch operation.
    pub bare: Vec<u8>,
    /// The whole edit.
    pub full: Vec<u8>,
    pub w: usize,
    pub h: usize,
}

impl Rendered {
    /// Render `recipe` with and without its retouch stack.
    ///
    /// # Errors
    /// Rendering failures, or two renders that do not line up.
    pub fn of(state: &crate::AppState, photo: PhotoId, recipe: &Recipe) -> AuraResult<Self> {
        let (full, w, h) = crate::advanced_retouch::qc_render(state, photo, recipe)?;
        let mut bare = recipe.clone();
        retouch_tools::write_with_mattes(&mut bare, &[], &BTreeMap::new())?;
        let (bare, bw, bh) = crate::advanced_retouch::qc_render(state, photo, &bare)?;
        if (bw, bh) != (w, h) || bare.len() != full.len() {
            return Err(aura_core::errors::render::recipe_invalid(
                "photo pass",
                "the two renders of the edit do not line up",
            ));
        }
        Ok(Self { bare, full, w, h })
    }

    fn at(buf: &[u8], i: usize) -> [f32; 3] {
        [
            f32::from(buf[i * 3]) / 255.0,
            f32::from(buf[i * 3 + 1]) / 255.0,
            f32::from(buf[i * 3 + 2]) / 255.0,
        ]
    }
}

/// Skin on the face: the parse's skin, inside its face. On real photographs its skin region alone
/// reached a red bodice and a white shirt, which are skin-coloured to a measurement.
const FACE_SKIN: &[Region] = &[Region::Skin, Region::Face];

/// The neck where the parse also sees skin: below a chin the parse's neck region can hold a
/// baby's face, a hand or a bouquet.
const NECK_SKIN: &[Region] = &[Region::Neck, Region::Skin];

/// Faces measured at fewer pixels than this between the eyes are too small to judge.
const SMALLEST_IOD: f32 = 18.0;
/// The most people a step places operations for, largest first: a matte each, and the recipe
/// holds a bounded number.
const MOST_PEOPLE: usize = 6;

fn luma(p: [f32; 3]) -> f32 {
    p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722
}

fn decode(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn median(v: &mut [f32]) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    let mid = v.len() / 2;
    v.select_nth_unstable_by(mid, f32::total_cmp);
    Some(v[mid])
}

fn mean3(v: &[[f32; 3]]) -> Option<[f32; 3]> {
    (!v.is_empty()).then(|| {
        let mut s = [0.0_f32; 3];
        for p in v {
            for c in 0..3 {
                s[c] += p[c];
            }
        }
        s.map(|x| x / v.len() as f32)
    })
}

/// Each channel over the mean of the three, in logs: colour without brightness.
fn log_chroma(p: [f32; 3]) -> [f32; 3] {
    let m = (p[0] + p[1] + p[2]).max(1e-4) / 3.0;
    p.map(|v| (v.max(1e-4) / m).ln())
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// One person of the parse, placed in the render.
#[derive(Debug, Clone, Copy)]
struct Person {
    /// Position among the parse's faces, largest first; reports count from 1.
    n: usize,
    geo: FaceGeometry,
    /// Pixels between the eyes in the render.
    iod: f32,
}

/// A person's own render pixels: index and canvas position.
type Own = std::rc::Rc<Vec<(usize, f32, f32)>>;

/// The parse placed over the two renders.
struct Faces<'a> {
    map: &'a PortraitMap,
    people: Vec<Person>,
    r: &'a Rendered,
    s: &'a Scene<'a>,
    /// Each person's own pixels (render index and canvas position), found once.
    owned: std::cell::RefCell<BTreeMap<usize, Own>>,
    /// Each person's pixels of each area, found once: every step reads the same few.
    areas: std::cell::RefCell<BTreeMap<(usize, String), std::rc::Rc<Vec<usize>>>>,
}

impl<'a> Faces<'a> {
    fn new(s: &'a Scene<'a>, r: &'a Rendered) -> Option<Self> {
        let map = s.map.as_deref()?;
        let scale = r.w as f32 / map.width.max(1) as f32;
        // A face both detectors found: the parse alone took the rope pattern on a drum for one,
        // and Auto enhance's detector is the one its retouch trusts. Each of its faces vouches
        // for one parse face at most.
        let (cw, ch) = (map.width.max(1) as f32, map.height.max(1) as f32);
        let mut vouched = vec![false; s.faces.len()];
        let people = s
            .map_faces()
            .into_iter()
            .filter(|geo| {
                let (u, v) = (geo.center[0] / cw, geo.center[1] / ch);
                let found = s.faces.iter().enumerate().find(|(k, f)| {
                    let [l, t, r, b] = f.bounds;
                    !vouched[*k] && u >= l && u <= r && v >= t && v <= b
                });
                if let Some((k, _)) = found {
                    vouched[k] = true;
                }
                found.is_some()
            })
            .enumerate()
            .map(|(n, geo)| {
                let iod = (geo.right_eye[0] - geo.left_eye[0])
                    .hypot(geo.right_eye[1] - geo.left_eye[1])
                    * scale;
                Person { n, geo, iod }
            })
            .collect();
        Some(Self {
            map,
            people,
            r,
            s,
            owned: std::cell::RefCell::new(BTreeMap::new()),
            areas: std::cell::RefCell::new(BTreeMap::new()),
        })
    }

    /// The people large enough to judge, largest first.
    fn judged(&self) -> impl Iterator<Item = &Person> {
        self.people
            .iter()
            .filter(|p| p.iod >= SMALLEST_IOD)
            .take(MOST_PEOPLE)
    }

    /// Canvas position of a render pixel.
    fn canvas(&self, x: usize, y: usize) -> (f32, f32) {
        (
            (x as f32 + 0.5) * self.map.width as f32 / self.r.w as f32,
            (y as f32 + 0.5) * self.map.height as f32 / self.r.h as f32,
        )
    }

    /// The person nearest a canvas position, within reach of their face.
    fn owner(&self, cx: f32, cy: f32) -> Option<usize> {
        self.people
            .iter()
            .map(|p| {
                let d = (cx - p.geo.center[0]).hypot(cy - p.geo.center[1]) / p.geo.size.max(1.0);
                (p.n, d)
            })
            .filter(|(_, d)| *d < 2.6)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(n, _)| n)
    }

    /// The render-pixel box round a person, from above the hair to below the neck.
    fn bbox(&self, p: &Person) -> [usize; 4] {
        let k = self.r.w as f32 / self.map.width.max(1) as f32;
        let (cx, cy, s) = (p.geo.center[0] * k, p.geo.center[1] * k, p.geo.size * k);
        [
            (cx - 1.6 * s).max(0.0) as usize,
            (cy - 1.3 * s).max(0.0) as usize,
            ((cx + 1.6 * s) as usize).min(self.r.w),
            ((cy + 2.4 * s) as usize).min(self.r.h),
        ]
    }

    /// How much a canvas position belongs to an area: the product of its regions' planes.
    fn weight(&self, area: &[Region], cx: f32, cy: f32) -> f32 {
        area.iter()
            .map(|r| {
                self.map
                    .plane(*r)
                    .map_or(0.0, |pl| pl.sample(cx, cy).clamp(0.0, 1.0))
            })
            .product()
    }

    /// Render pixels of `area` that belong to `p`, at a stride that keeps the count bounded.
    fn pixels(&self, p: &Person, area: &[Region]) -> Vec<usize> {
        if area.iter().any(|r| self.map.plane(*r).is_none()) {
            return Vec::new();
        }
        let key = (p.n, format!("{area:?}"));
        if let Some(found) = self.areas.borrow().get(&key) {
            return found.as_ref().clone();
        }
        let out: Vec<usize> = self
            .own(p)
            .iter()
            .filter(|(_, cx, cy)| self.weight(area, *cx, *cy) > 0.5)
            .map(|(i, _, _)| *i)
            .collect();
        self.areas
            .borrow_mut()
            .insert(key, std::rc::Rc::new(out.clone()));
        out
    }

    /// The render pixels round `p` that are theirs, at a stride that keeps the count bounded.
    fn own(&self, p: &Person) -> Own {
        if let Some(found) = self.owned.borrow().get(&p.n) {
            return found.clone();
        }
        let [x0, y0, x1, y1] = self.bbox(p);
        let step = ((p.iod / 60.0) as usize).max(1);
        let mut out = Vec::new();
        for y in (y0..y1).step_by(step) {
            for x in (x0..x1).step_by(step) {
                let (cx, cy) = self.canvas(x, y);
                if self.owner(cx, cy) == Some(p.n) {
                    out.push((y * self.r.w + x, cx, cy));
                }
            }
        }
        let out = std::rc::Rc::new(out);
        self.owned.borrow_mut().insert(p.n, out.clone());
        out
    }

    /// The same pixels of the original, from the preview the pass measured.
    fn original(&self, i: usize) -> [f32; 3] {
        let (x, y) = (i % self.r.w, i / self.r.w);
        let px = (x * self.s.w / self.r.w).min(self.s.w - 1);
        let py = (y * self.s.h / self.r.h).min(self.s.h - 1);
        self.s.px.encoded(px, py)
    }

    /// A selection of `region` for one person, on a grid over their box, for an operation.
    fn matte(&self, p: &Person, area: &[Region], refine: bool) -> Option<(Matte, [f32; 4])> {
        if area.iter().any(|r| self.map.plane(*r).is_none()) {
            return None;
        }
        let [x0, y0, x1, y1] = self.bbox(p);
        if x1 <= x0 + 4 || y1 <= y0 + 4 {
            return None;
        }
        let gw = 96_usize;
        let gh = ((y1 - y0) * gw / (x1 - x0)).clamp(8, 192);
        let mut alpha = Vec::with_capacity(gw * gh);
        let mut any = false;
        for gy in 0..gh {
            for gx in 0..gw {
                let x = x0 + (gx * (x1 - x0)) / gw;
                let y = y0 + (gy * (y1 - y0)) / gh;
                let (cx, cy) = self.canvas(x, y);
                let v = if self.owner(cx, cy) == Some(p.n) {
                    self.weight(area, cx, cy)
                } else {
                    0.0
                };
                any |= v > 0.5;
                alpha.push((v * 255.0).round() as u8);
            }
        }
        if !any {
            return None;
        }
        let (w, h) = (self.r.w as f32, self.r.h as f32);
        let bounds = [x0 as f32 / w, y0 as f32 / h, x1 as f32 / w, y1 as f32 / h];
        let mut m = Matte::encode(bounds, gw as u32, gh as u32, &alpha);
        m.refine_edges = refine;
        let region = [
            f32::midpoint(bounds[0], bounds[2]),
            f32::midpoint(bounds[1], bounds[3]),
            (bounds[2] - bounds[0]) / 2.0 * std::f32::consts::SQRT_2,
            (bounds[3] - bounds[1]) / 2.0 * std::f32::consts::SQRT_2,
        ];
        Some((m, region))
    }

    /// An operation on one person's region, limited to it by a matte.
    fn op(
        &self,
        def: &StepDef,
        p: &Person,
        area: &[Region],
        tool: Tool,
        amount: f32,
    ) -> Option<(Edit, String, Matte)> {
        let (matte, ellipse) = self.matte(p, area, true)?;
        let id = format!("{}-face{}", super::op_id(def, p.n), p.n);
        let mut e = base_edit(id.clone(), tool, ellipse, amount);
        e.feather = 0.0;
        e.matte = Some(id.clone());
        Some((e, id, matte))
    }

    /// One plane on the render grid, for the measures that read whole planes.
    fn plane_on_render(&self, region: Region) -> Vec<f32> {
        let Some(plane) = self.map.plane(region) else {
            return vec![0.0; self.r.w * self.r.h];
        };
        let mut out = vec![0.0; self.r.w * self.r.h];
        for y in 0..self.r.h {
            for x in 0..self.r.w {
                let (cx, cy) = self.canvas(x, y);
                out[y * self.r.w + x] = plane.sample(cx, cy).clamp(0.0, 1.0);
            }
        }
        out
    }
}

/// Collects a step's operations and mattes, one per person.
#[derive(Default)]
struct Ops {
    edits: Vec<Edit>,
    mattes: BTreeMap<String, Matte>,
}

impl Ops {
    fn push(&mut self, made: Option<(Edit, String, Matte)>) -> Option<&mut Edit> {
        let (e, id, m) = made?;
        self.mattes.insert(id, m);
        self.edits.push(e);
        self.edits.last_mut()
    }

    fn finding(self, what: String, change: String) -> Finding {
        let mut f = Finding::applied(what, change, Vec::new(), self.edits);
        f.mattes = self.mattes;
        f
    }
}

/// Plan every step of batch 2, in catalogue order.
#[must_use]
pub fn plan(s: &Scene<'_>, rendered: Option<&Rendered>, current: &Recipe) -> Vec<Finding> {
    let count = STEPS.len() - 25;
    let Some(r) = rendered else {
        return (0..count)
            .map(|_| Finding::not_applicable("Nobody is in the frame."))
            .collect();
    };
    let Some(f) = Faces::new(s, r) else {
        return (0..count)
            .map(|_| Finding::not_applicable("The faces could not be parsed."))
            .collect();
    };
    if f.judged().next().is_none() {
        let small = small_faces(&f);
        return (0..count)
            .map(|k| {
                if k == 23 {
                    small.clone()
                } else {
                    Finding::not_applicable("Every face is too small to judge.")
                }
            })
            .collect();
    }
    // Strength factors as the steps leave them, so two steps that both touch smoothing agree.
    let mut factors = intensity::read(current).unwrap_or_default();
    let stack = retouch_tools::read(current).unwrap_or_default();
    let skin = f.plane_on_render(Region::Skin);
    let def = |n: usize| &STEPS[n - 1];
    vec![
        face_exposure(&f),
        skin_clipping(&f, def(27)),
        skin_hue(&f, def(28)),
        skin_saturation(&f, def(29)),
        lip_colour(&f, def(30)),
        pores_kept(&f, &skin, &stack, &mut factors),
        skin_tone_kept(&f, &skin, &stack, &mut factors),
        texture_kept(&f, Region::FacialHair, "beard", true, &stack, &mut factors),
        texture_kept(
            &f,
            Region::Eyebrows,
            "eyebrows",
            false,
            &stack,
            &mut factors,
        ),
        texture_kept(&f, Region::Hair, "hairline", false, &stack, &mut factors),
        under_eye_kept(&f, &stack, &mut factors),
        teeth_natural(&f, &stack, &mut factors),
        shine_left(&f, &stack, &mut factors),
        ceremonial_kept(&f),
        moles_kept(&f),
        retouch_balance(&f),
        retouch_leak(&f),
        skin_noise(&f, def(43)),
        eye_focus(&f),
        side_light(&f, def(45)),
        face_neck_colour(&f, &stack, def(46)),
        mehndi_kept(&f),
        makeup_kept(&f),
        small_faces(&f),
        neck_light(&f, def(50)),
    ]
}

fn auto_ops(stack: &[Edit], kind: Kind) -> usize {
    stack
        .iter()
        .filter(|e| e.enabled && intensity::category_of(e) == Some(kind))
        .count()
}

fn factor(factors: &BTreeMap<Kind, f32>, kind: Kind) -> f32 {
    factors.get(&kind).copied().unwrap_or(1.0)
}

// ---- 26. Face against its background -----------------------------------------------------------

/// Reported, never applied. From these measurements a face in shade and a darker complexion
/// in good light read the same: any rule that lifted "dark" faces would lighten people for their
/// skin. Back light is told by the background instead, and Portrait light, which models the
/// light on the face, is where to fill it.
fn face_exposure(f: &Faces<'_>) -> Finding {
    let mut lines = Vec::new();
    for p in f.judged() {
        let skin = f.pixels(p, FACE_SKIN);
        let back = f.pixels(p, &[Region::Background]);
        if skin.len() < 40 || back.len() < 40 || veiled(f, &skin) {
            continue;
        }
        let (Some(face), Some(around)) = (med_luma(&f.r.full, &skin), med_luma(&f.r.full, &back))
        else {
            continue;
        };
        if around > 0.8 && around - face > 0.3 {
            lines.push(format!(
                "face {} (background at {around:.2}, face at {face:.2})",
                p.n + 1
            ));
        }
    }
    if lines.is_empty() {
        return Finding::checked("No face is against a background near white.");
    }
    Finding::checked(format!(
        "Back light behind {}: a window or the sky. Not changed automatically - a face in shade and a darker complexion measure alike. If a face looks under-lit, Portrait light > Auto light fills it from the light that is there.",
        lines.join("; ")
    ))
}

// ---- 27. Skin clipped by the edit --------------------------------------------------------------

fn clipped(c: [f32; 3]) -> bool {
    c[0].max(c[1]).max(c[2]) >= 0.995
}

fn skin_clipping(f: &Faces<'_>, def: &StepDef) -> Finding {
    let mut ops = Ops::default();
    let mut lines = Vec::new();
    for p in f.judged() {
        let skin = f.pixels(p, FACE_SKIN);
        if skin.len() < 60 {
            continue;
        }
        let after = skin
            .iter()
            .filter(|i| clipped(Rendered::at(&f.r.full, **i)))
            .count();
        let before = skin.iter().filter(|i| clipped(f.original(**i))).count();
        let (a, b) = (
            after as f32 / skin.len() as f32,
            before as f32 / skin.len() as f32,
        );
        if a - b >= 0.02 {
            // The roll-off starts a little below the clipped skin's own brightness, linear: a
            // burnt red channel on sunlit skin is far below white in luminance.
            let mut lit: Vec<f32> = skin
                .iter()
                .map(|i| Rendered::at(&f.r.full, *i))
                .filter(|c| clipped(*c))
                .map(|c| decode(luma(c)))
                .collect();
            lit.sort_by(f32::total_cmp);
            let Some(low) = lit.get(lit.len() / 5).copied() else {
                continue;
            };
            let threshold = (low * 0.8).max(0.05);
            if let Some(e) = ops.push(f.op(def, p, FACE_SKIN, Tool::HighlightCompress, 0.8)) {
                // The renderer compresses above 0.18 x 2^(4 tone), linear.
                e.tone = ((threshold / 0.18).log2() / 4.0).clamp(-1.0, 1.0);
                lines.push(format!(
                    "face {}: {:.1}% of the skin burnt out after the edit, {:.1}% before",
                    p.n + 1,
                    a * 100.0,
                    b * 100.0
                ));
            }
        }
    }
    if ops.edits.is_empty() {
        return Finding::checked("The edit burnt out no skin that the original had detail in.");
    }
    ops.finding(
        format!("The edit clipped skin: {}.", lines.join("; ")),
        "The brightest skin rolled off below white, so it keeps its detail.".into(),
    )
}

// ---- 28-29. Skin colour against the grade ------------------------------------------------------

/// How the grade moved neutral colours: the mean log-chroma shift of near-neutral pixels away
/// from people, the original against the bare render.
fn neutral_shift(f: &Faces<'_>) -> Option<[f32; 3]> {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    let step = 3;
    for y in (0..f.r.h).step_by(step) {
        for x in (0..f.r.w).step_by(step) {
            let i = y * f.r.w + x;
            let (cx, cy) = f.canvas(x, y);
            if f.owner(cx, cy).is_some() {
                continue;
            }
            let o = f.original(i);
            let l = luma(o);
            if (0.25..0.85).contains(&l) && saturation(o) < 0.12 {
                before.push(log_chroma(o));
                after.push(log_chroma(Rendered::at(&f.r.bare, i)));
            }
        }
    }
    let total = (f.r.w / step) * (f.r.h / step);
    if (before.len() as f32) < total as f32 * 0.01 {
        return None;
    }
    let (b, a) = (mean3(&before)?, mean3(&after)?);
    Some(std::array::from_fn(|c| a[c] - b[c]))
}

/// Skin seen through a coloured veil or under a strong coloured light: far more saturated than
/// skin is. Its colour is the veil's or the light's, and correcting it would fight them.
fn veiled(f: &Faces<'_>, skin: &[usize]) -> bool {
    let mut s: Vec<f32> = skin.iter().map(|i| saturation(f.original(*i))).collect();
    median(&mut s).is_some_and(|m| m > 0.55)
}

fn veil_note(faces: &[usize]) -> String {
    if faces.is_empty() {
        String::new()
    } else {
        format!(
            " {} seen through a veil or a coloured light, so left as it is.",
            faces
                .iter()
                .map(|n| format!("Face {}", n + 1))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn skin_hue(f: &Faces<'_>, def: &StepDef) -> Finding {
    let Some(neutral) = neutral_shift(f) else {
        return Finding::not_applicable(
            "Too few neutral surfaces to tell the white balance from the grade.",
        );
    };
    let mut ops = Ops::default();
    let mut lines = Vec::new();
    let mut veils = Vec::new();
    for p in f.judged() {
        let skin = f.pixels(p, FACE_SKIN);
        if skin.len() < 60 {
            continue;
        }
        if veiled(f, &skin) {
            veils.push(p.n);
            continue;
        }
        let before = mean3(
            &skin
                .iter()
                .map(|i| log_chroma(f.original(*i)))
                .collect::<Vec<_>>(),
        );
        let after = mean3(
            &skin
                .iter()
                .map(|i| log_chroma(Rendered::at(&f.r.bare, *i)))
                .collect::<Vec<_>>(),
        );
        let (Some(b), Some(a)) = (before, after) else {
            continue;
        };
        // What the grade did to this skin beyond what it did to white and grey.
        let extra: [f32; 3] = std::array::from_fn(|c| (a[c] - b[c]) - neutral[c]);
        let size = extra.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        // No grade moves skin a third further than it moves grey: such a reading is the two
        // pipelines disagreeing on a saturated colour, not the edit.
        if !(0.06..0.35).contains(&size) {
            continue;
        }
        // SkinColor: warmth moves red against blue by 0.5 w, tint green by -0.24 t (logs).
        let warmth = (-(extra[0] - extra[2]) * 0.7 / 0.5).clamp(-1.0, 1.0);
        let tint = ((extra[1] - 0.5 * (extra[0] + extra[2])) * 0.7 / 0.24).clamp(-1.0, 1.0);
        if let Some(e) = ops.push(f.op(def, p, FACE_SKIN, Tool::SkinColor, 0.9)) {
            e.warmth = warmth;
            e.tint = tint;
            lines.push(format!(
                "face {}: red {:+.0}%, green {:+.0}%, blue {:+.0}% beyond the neutrals",
                p.n + 1,
                extra[0] * 100.0,
                extra[1] * 100.0,
                extra[2] * 100.0
            ));
        }
    }
    if ops.edits.is_empty() {
        return Finding::checked(format!(
            "The grade moved skin as it moved white and grey: the skin's own colour is kept.{}",
            veil_note(&veils)
        ));
    }
    ops.finding(
        format!("The grade moved skin more than neutrals: {}.", lines.join("; ")),
        "Most of that extra colour taken back out of the skin only; the grade stays on everything else.".into(),
    )
}

fn skin_saturation(f: &Faces<'_>, def: &StepDef) -> Finding {
    let mut ops = Ops::default();
    let mut lines = Vec::new();
    let mut veils = Vec::new();
    for p in f.judged() {
        let skin = f.pixels(p, FACE_SKIN);
        if skin.len() < 60 {
            continue;
        }
        if veiled(f, &skin) {
            veils.push(p.n);
            continue;
        }
        let s = |c: [f32; 3]| saturation(c);
        let before = skin.iter().map(|i| s(f.original(*i))).sum::<f32>() / skin.len() as f32;
        let after = skin
            .iter()
            .map(|i| s(Rendered::at(&f.r.bare, *i)))
            .sum::<f32>()
            / skin.len() as f32;
        let ratio = after / before.max(1e-3);
        if ratio > 1.25 && after > 0.3 {
            if let Some(e) = ops.push(f.op(def, p, FACE_SKIN, Tool::ColorSaturation, 0.9)) {
                e.warmth = (1.1 / ratio - 1.0).clamp(-0.4, 0.0);
                lines.push(format!(
                    "face {}: saturation {before:.2} -> {after:.2} ({:+.0}%)",
                    p.n + 1,
                    (ratio - 1.0) * 100.0
                ));
            }
        }
    }
    if ops.edits.is_empty() {
        return Finding::checked(format!(
            "Skin saturation is within a quarter of the original's.{}",
            veil_note(&veils)
        ));
    }
    ops.finding(
        format!("The grade over-saturated skin: {}.", lines.join("; ")),
        "Skin saturation brought back to within a tenth of the original's.".into(),
    )
}

// ---- 30. Lips ----------------------------------------------------------------------------------

fn lip_colour(f: &Faces<'_>, def: &StepDef) -> Finding {
    let mut ops = Ops::default();
    let mut lines = Vec::new();
    let mut seen = 0;
    for p in f.judged() {
        let lips = f.pixels(p, &[Region::Lips]);
        if lips.len() < 20 {
            continue;
        }
        seen += 1;
        let before =
            lips.iter().map(|i| saturation(f.original(*i))).sum::<f32>() / lips.len() as f32;
        let after = lips
            .iter()
            .map(|i| saturation(Rendered::at(&f.r.full, *i)))
            .sum::<f32>()
            / lips.len() as f32;
        let ratio = after / before.max(1e-3);
        if ratio < 0.8 && before > 0.15 {
            if let Some(e) = ops.push(f.op(def, p, &[Region::Lips], Tool::ColorSaturation, 0.9)) {
                e.warmth = (0.95 / ratio - 1.0).clamp(0.0, 0.4);
                lines.push(format!(
                    "face {}: lip saturation {before:.2} -> {after:.2}",
                    p.n + 1
                ));
            }
        }
    }
    if seen == 0 {
        return Finding::not_applicable("No lips were measured clearly enough.");
    }
    if ops.edits.is_empty() {
        return Finding::checked("Lips keep their colour after the edit.");
    }
    ops.finding(
        format!(
            "The edit drained colour from the lips: {}.",
            lines.join("; ")
        ),
        "Lip colour brought back to within a twentieth of the original's.".into(),
    )
}

// ---- 31-35. What the retouch must keep ---------------------------------------------------------

/// Fine detail kept on `idx` from the bare render to the full one: the mean absolute 3 x 3
/// high-pass, after over before.
fn detail_kept(f: &Faces<'_>, idx: &[usize]) -> Option<f32> {
    let w = f.r.w;
    let hp = |buf: &[u8], i: usize| {
        let (x, y) = (i % w, i / w);
        if x == 0 || y == 0 || x + 1 >= w || y + 1 >= f.r.h {
            return None;
        }
        let l = |j: usize| luma(Rendered::at(buf, j));
        let around = (l(i - 1) + l(i + 1) + l(i - w) + l(i + w)) / 4.0;
        Some((l(i) - around).abs())
    };
    let (mut b, mut a, mut n) = (0.0_f32, 0.0_f32, 0);
    for &i in idx {
        if let (Some(x), Some(y)) = (hp(&f.r.bare, i), hp(&f.r.full, i)) {
            b += x;
            a += y;
            n += 1;
        }
    }
    (n >= 40 && b > 1e-4 * n as f32).then(|| a / b)
}

fn pores_kept(
    f: &Faces<'_>,
    skin: &[f32],
    stack: &[Edit],
    factors: &mut BTreeMap<Kind, f32>,
) -> Finding {
    if auto_ops(stack, Kind::Smoothing) == 0 {
        return Finding::not_applicable("Auto enhance placed no skin smoothing to check.");
    }
    let kept = measure::texture_retention(&f.r.bare, &f.r.full, f.r.w, f.r.h, skin);
    let Some(kept) = kept.filter(|v| v.is_finite()) else {
        return Finding::not_applicable("Too little skin to measure its texture.");
    };
    let reading = format!(
        "The retouch kept {:.0}% of the skin's fine texture.",
        kept * 100.0
    );
    if kept >= crate::advanced_retouch::TEXTURE_FLOOR {
        return Finding::checked(format!("{reading} Pores are kept."));
    }
    // Texture lost scales with the smoothing's strength; aim for 70% kept.
    let now = factor(factors, Kind::Smoothing);
    let next = (now * (0.3 / (1.0 - kept).max(0.05))).clamp(0.3, now);
    factors.insert(Kind::Smoothing, next);
    Finding::applied(
        reading,
        format!(
            "Skin smoothing {:.0}% -> {:.0}%, so the pores come back.",
            now * 100.0,
            next * 100.0
        ),
        vec![Change::Intensity(Kind::Smoothing, next)],
        Vec::new(),
    )
}

fn skin_tone_kept(
    f: &Faces<'_>,
    skin: &[f32],
    stack: &[Edit],
    factors: &mut BTreeMap<Kind, f32>,
) -> Finding {
    if auto_ops(stack, Kind::ToneEvenness) + auto_ops(stack, Kind::Redness) == 0 {
        return Finding::not_applicable("Auto enhance placed no tone or redness evening to check.");
    }
    let Some(shift) = measure::skin_shift(&f.r.bare, &f.r.full, f.r.w, f.r.h, skin)
        .map(|(shift, _)| shift)
        .filter(|v| v.is_finite())
    else {
        return Finding::not_applicable("Too little skin to measure its colour.");
    };
    let reading = format!("The retouch moved the skin's average colour by {shift:.4}.");
    if shift <= crate::advanced_retouch::SKIN_SHIFT_CEILING {
        return Finding::checked(format!("{reading} The person's own colour is kept."));
    }
    let tone = factor(factors, Kind::ToneEvenness) * 0.5;
    let red = factor(factors, Kind::Redness) * 0.5;
    factors.insert(Kind::ToneEvenness, tone);
    factors.insert(Kind::Redness, red);
    Finding::applied(
        reading,
        "Tone and redness evening halved, as Auto advanced retouch's own quality control does.",
        vec![
            Change::Intensity(Kind::ToneEvenness, tone),
            Change::Intensity(Kind::Redness, red),
        ],
        Vec::new(),
    )
}

fn texture_kept(
    f: &Faces<'_>,
    region: Region,
    name: &str,
    act: bool,
    stack: &[Edit],
    factors: &mut BTreeMap<Kind, f32>,
) -> Finding {
    let mut worst: Option<(usize, f32)> = None;
    let mut seen = 0;
    for p in f.judged() {
        let idx = f.pixels(p, &[region]);
        if let Some(kept) = detail_kept(f, &idx) {
            seen += 1;
            if worst.is_none_or(|(_, w)| kept < w) {
                worst = Some((p.n, kept));
            }
        }
    }
    let Some((n, kept)) = worst else {
        return Finding::not_applicable(format!("No {name} measured clearly enough."));
    };
    let reading = format!(
        "The retouch kept {:.0}% of the {name}'s detail where it lost most (face {}; {} measured).",
        kept * 100.0,
        n + 1,
        plural(seen, "face", "faces")
    );
    if kept >= 0.6 {
        return Finding::checked(format!("{reading} Kept."));
    }
    if !act || auto_ops(stack, Kind::Smoothing) == 0 {
        return Finding::checked(format!(
            "{reading} Flagged for review: open the photograph's retouch and lower the operation that covers it."
        ));
    }
    let now = factor(factors, Kind::Smoothing);
    let next = (now * 0.75).clamp(0.3, now);
    factors.insert(Kind::Smoothing, next);
    Finding::applied(
        reading,
        format!(
            "Skin smoothing {:.0}% -> {:.0}% so the {name} keeps its hairs.",
            now * 100.0,
            next * 100.0
        ),
        vec![Change::Intensity(Kind::Smoothing, next)],
        Vec::new(),
    )
}

// ---- 36-38. Features the retouch must not overdo ------------------------------------------------

/// Median display luminance of `idx` in a buffer.
fn med_luma(buf: &[u8], idx: &[usize]) -> Option<f32> {
    let mut v: Vec<f32> = idx.iter().map(|i| luma(Rendered::at(buf, *i))).collect();
    median(&mut v)
}

/// The cheek below one person's eyes: skin between the eye line and the mouth, not under-eye.
fn cheek(f: &Faces<'_>, p: &Person) -> Vec<usize> {
    let k = f.r.w as f32 / f.map.width.max(1) as f32;
    let (eye_y, mouth_y) = (
        f32::midpoint(p.geo.left_eye[1], p.geo.right_eye[1]) * k,
        p.geo.mouth[1] * k,
    );
    let under: std::collections::BTreeSet<usize> =
        f.pixels(p, &[Region::UnderEyes]).into_iter().collect();
    f.pixels(p, FACE_SKIN)
        .into_iter()
        .filter(|i| {
            let y = (i / f.r.w) as f32;
            y > eye_y + 0.35 * p.iod && y < mouth_y && !under.contains(i)
        })
        .collect()
}

fn under_eye_kept(f: &Faces<'_>, stack: &[Edit], factors: &mut BTreeMap<Kind, f32>) -> Finding {
    if auto_ops(stack, Kind::UnderEye) == 0 {
        return Finding::not_applicable("Auto enhance placed no under-eye lift to check.");
    }
    let mut over = Vec::new();
    for p in f.judged() {
        let ue = f.pixels(p, &[Region::UnderEyes]);
        let ch = cheek(f, p);
        let (Some(before), Some(after), Some(c)) = (
            med_luma(&f.r.bare, &ue),
            med_luma(&f.r.full, &ue),
            med_luma(&f.r.full, &ch),
        ) else {
            continue;
        };
        if after > c + 0.03 && after > before + 0.01 {
            over.push((p.n, before, after, c));
        }
    }
    if over.is_empty() {
        return Finding::checked("No under-eye was lifted past the person's own cheek.");
    }
    // The lift that reaches the cheek and no further, on the face that went furthest.
    let scale = over
        .iter()
        .map(|(_, b, a, c)| ((c - b) / (a - b).max(1e-3)).clamp(0.3, 1.0))
        .fold(1.0_f32, f32::min);
    let now = factor(factors, Kind::UnderEye);
    let next = (now * scale).clamp(0.3, now);
    factors.insert(Kind::UnderEye, next);
    Finding::applied(
        format!(
            "Under-eyes lifted lighter than the cheek: {}.",
            over.iter()
                .map(|(n, _, a, c)| format!("face {} at {a:.2} against a cheek at {c:.2}", n + 1))
                .collect::<Vec<_>>()
                .join("; ")
        ),
        format!(
            "Under-eye lift {:.0}% -> {:.0}%: lifted to the cheek, not past it.",
            now * 100.0,
            next * 100.0
        ),
        vec![Change::Intensity(Kind::UnderEye, next)],
        Vec::new(),
    )
}

fn teeth_natural(f: &Faces<'_>, stack: &[Edit], factors: &mut BTreeMap<Kind, f32>) -> Finding {
    if auto_ops(stack, Kind::Teeth) == 0 {
        return Finding::not_applicable("Auto enhance placed no teeth whitening to check.");
    }
    let mut over = Vec::new();
    for p in f.judged() {
        let teeth = f.pixels(p, &[Region::Teeth]);
        let eyes = f.pixels(p, &[Region::Sclera]);
        if teeth.len() < 12 || eyes.len() < 8 {
            continue;
        }
        let (Some(before), Some(after), Some(white)) = (
            med_luma(&f.r.bare, &teeth),
            med_luma(&f.r.full, &teeth),
            med_luma(&f.r.full, &eyes),
        ) else {
            continue;
        };
        if after > white + 0.03 && after > before + 0.01 {
            over.push((p.n, before, after, white));
        }
    }
    if over.is_empty() {
        return Finding::checked(
            "No teeth were whitened past the whites of the same person's eyes.",
        );
    }
    let scale = over
        .iter()
        .map(|(_, b, a, w)| ((w - b) / (a - b).max(1e-3)).clamp(0.3, 1.0))
        .fold(1.0_f32, f32::min);
    let now = factor(factors, Kind::Teeth);
    let next = (now * scale).clamp(0.3, now);
    factors.insert(Kind::Teeth, next);
    Finding::applied(
        format!(
            "Teeth whitened past the eyes: {}.",
            over.iter()
                .map(|(n, _, a, w)| format!("face {}: teeth {a:.2}, eyes {w:.2}", n + 1))
                .collect::<Vec<_>>()
                .join("; ")
        ),
        format!(
            "Teeth whitening {:.0}% -> {:.0}%: no whiter than the eyes.",
            now * 100.0,
            next * 100.0
        ),
        vec![Change::Intensity(Kind::Teeth, next)],
        Vec::new(),
    )
}

fn shine_left(f: &Faces<'_>, stack: &[Edit], factors: &mut BTreeMap<Kind, f32>) -> Finding {
    let hot = |c: [f32; 3]| luma(c) > 0.9 && saturation(c) < 0.15;
    let mut worst: Option<(usize, f32, f32)> = None;
    for p in f.judged() {
        let skin = f.pixels(p, FACE_SKIN);
        if skin.len() < 80 {
            continue;
        }
        let share = |buf: &[u8]| {
            skin.iter().filter(|i| hot(Rendered::at(buf, **i))).count() as f32 / skin.len() as f32
        };
        let (b, a) = (share(&f.r.bare), share(&f.r.full));
        if worst.is_none_or(|(_, _, w)| a > w) {
            worst = Some((p.n, b, a));
        }
    }
    let Some((n, before, after)) = worst else {
        return Finding::not_applicable("Too little skin to look for shine on.");
    };
    let reading = format!(
        "Shiny skin on face {}: {:.1}% before the retouch, {:.1}% after.",
        n + 1,
        before * 100.0,
        after * 100.0
    );
    if after < 0.03 {
        return Finding::checked(format!("{reading} No shine left to speak of."));
    }
    if auto_ops(stack, Kind::Shine) == 0 {
        return Finding::checked(format!(
            "{reading} Auto enhance placed no shine control; flagged for review."
        ));
    }
    if after < before * 0.7 {
        return Finding::checked(format!("{reading} Shine control already took most of it."));
    }
    let now = factor(factors, Kind::Shine);
    let next = (now * 1.5).min(intensity::MAX_FACTOR);
    factors.insert(Kind::Shine, next);
    Finding::applied(
        reading,
        format!("Shine control {:.0}% -> {:.0}%.", now * 100.0, next * 100.0),
        vec![Change::Intensity(Kind::Shine, next)],
        Vec::new(),
    )
}

// ---- 39-42. What the retouch must leave alone (reported) ----------------------------------------

fn ceremonial_kept(f: &Faces<'_>) -> Finding {
    // Strong red or magenta on the forehead and in the parting: bindi, tika, sindoor.
    let mut marks = 0;
    let mut lost = Vec::new();
    let k = f.r.w as f32 / f.map.width.max(1) as f32;
    for p in f.judged() {
        let eye_y = f32::midpoint(p.geo.left_eye[1], p.geo.right_eye[1]) * k;
        let cx = p.geo.center[0] * k;
        let mut idx = Vec::new();
        let [x0, y0, x1, y1] = f.bbox(p);
        for y in y0..y1.min(eye_y as usize) {
            for x in x0..x1 {
                if (x as f32 - cx).abs() > 0.6 * p.iod || (eye_y - y as f32) > 2.4 * p.iod {
                    continue;
                }
                let i = y * f.r.w + x;
                let c = Rendered::at(&f.r.bare, i);
                if saturation(c) > 0.55
                    && hue(c).is_some_and(|h| !(0.08..0.8).contains(&h))
                    && c[0] > 0.3
                {
                    idx.push(i);
                }
            }
        }
        if idx.len() < 6 {
            continue;
        }
        marks += 1;
        let sat = |buf: &[u8]| {
            idx.iter()
                .map(|i| saturation(Rendered::at(buf, *i)))
                .sum::<f32>()
                / idx.len() as f32
        };
        let (b, a) = (sat(&f.r.bare), sat(&f.r.full));
        if a < b * 0.75 {
            lost.push(format!("face {}: saturation {b:.2} -> {a:.2}", p.n + 1));
        }
    }
    if marks == 0 {
        return Finding::not_applicable("No bindi, tika or sindoor on the faces measured.");
    }
    if lost.is_empty() {
        return Finding::checked(format!(
            "Strong red above the eyes of {} (a bindi, tika, sindoor or red veil); the retouch kept its colour on every one.",
            plural(marks, "face", "faces")
        ));
    }
    Finding::checked(format!(
        "A ceremonial mark lost colour in the retouch ({}). Flagged for review: open Retouch and remove the operation over it.",
        lost.join("; ")
    ))
}

fn moles_kept(f: &Faces<'_>) -> Finding {
    // A mole: a compact spot clearly darker than the skin round it, not red.
    let w = f.r.w;
    let mut found = 0;
    let mut gone = 0;
    for p in f.judged() {
        let r = (p.iod * 0.08).round().max(2.0) as usize;
        let skin = f.pixels(p, FACE_SKIN);
        for &i in skin.iter().step_by(3) {
            let (x, y) = (i % w, i / w);
            if x < 2 * r || y < 2 * r || x + 2 * r >= w || y + 2 * r >= f.r.h {
                continue;
            }
            let at = |buf: &[u8], j: usize| luma(Rendered::at(buf, j));
            let ring = |buf: &[u8]| {
                [(2 * r, 0), (0, 2 * r)]
                    .iter()
                    .flat_map(|(dx, dy)| [i + dx + dy * w, i - dx - dy * w])
                    .map(|j| at(buf, j))
                    .sum::<f32>()
                    / 4.0
            };
            let c = Rendered::at(&f.r.bare, i);
            let depth = ring(&f.r.bare) - luma(c);
            let redder = c[0] > c[1] * 1.35;
            // Local minimum against its immediate neighbours, so one spot counts once.
            let local_min = [1_usize, w].iter().all(|d| {
                at(&f.r.bare, i) <= at(&f.r.bare, i + d) && at(&f.r.bare, i) <= at(&f.r.bare, i - d)
            });
            if depth > 0.1 && !redder && local_min {
                found += 1;
                let after = ring(&f.r.full) - at(&f.r.full, i);
                if after < depth * 0.4 {
                    gone += 1;
                }
            }
        }
    }
    if found == 0 {
        return Finding::not_applicable("No mole or beauty mark measured on the faces.");
    }
    if gone == 0 {
        return Finding::checked(format!(
            "{} measured; the retouch kept every one.",
            plural(found, "dark mark", "dark marks")
        ));
    }
    Finding::checked(format!(
        "{gone} of {found} dark marks were mostly removed by the retouch. Moles and beauty marks are a person's own: flagged for review."
    ))
}

fn retouch_balance(f: &Faces<'_>) -> Finding {
    let mut lines = Vec::new();
    let mut seen = 0;
    for face in f.s.faces.iter().take(MOST_PEOPLE) {
        // Only a face the retouch changed measurably, as Auto advanced retouch counts it.
        let Some((b, _)) = measure::mirror_balance(&f.r.bare, &f.r.full, f.r.w, f.r.h, face)
            .filter(|(_, change)| *change > 0.004)
        else {
            continue;
        };
        seen += 1;
        if b > 2.5 {
            lines.push(format!(
                "one half changed {b:.1} times as much as the other"
            ));
        }
    }
    if seen == 0 {
        return Finding::not_applicable("No face large enough to compare its halves.");
    }
    if lines.is_empty() {
        return Finding::checked(format!(
            "Both halves of {} were retouched alike.",
            plural(seen, "face", "faces")
        ));
    }
    Finding::checked(format!(
        "{}: flagged for review - one side of a face may look retouched and the other not.",
        lines.join("; ")
    ))
}

fn retouch_leak(f: &Faces<'_>) -> Finding {
    let mut lines = Vec::new();
    for p in f.judged() {
        let off: Vec<usize> = [Region::Background, Region::Clothing]
            .into_iter()
            .flat_map(|r| f.pixels(p, &[r]))
            .collect();
        if off.len() < 100 {
            continue;
        }
        let moved = off
            .iter()
            .filter(|i| {
                let (a, b) = (Rendered::at(&f.r.full, **i), Rendered::at(&f.r.bare, **i));
                (0..3).any(|c| (a[c] - b[c]).abs() > 6.0 / 255.0)
            })
            .count() as f32
            / off.len() as f32;
        if moved > 0.04 {
            lines.push(format!(
                "face {}: {:.0}% of the clothes and background beside it",
                p.n + 1,
                moved * 100.0
            ));
        }
    }
    if lines.is_empty() {
        return Finding::checked("The face retouch stayed on skin and features.");
    }
    Finding::checked(format!(
        "The retouch also changed {}. Flagged for review.",
        lines.join("; ")
    ))
}

// ---- 43-44. Noise and focus --------------------------------------------------------------------

fn skin_noise(f: &Faces<'_>, def: &StepDef) -> Finding {
    let mut ops = Ops::default();
    let mut lines = Vec::new();
    let w = f.r.w;
    for p in f.judged() {
        let skin = f.pixels(p, FACE_SKIN);
        let mut d: Vec<f32> = skin
            .iter()
            .filter(|i| {
                let (x, y) = (**i % w, **i / w);
                x > 0 && y > 0 && x + 1 < w && y + 1 < f.r.h
            })
            .map(|&i| {
                let l = |j: usize| luma(Rendered::at(&f.r.full, j));
                (l(i) - (l(i - 1) + l(i + 1) + l(i - w) + l(i + w)) / 4.0).abs()
            })
            .collect();
        if d.len() < 80 {
            continue;
        }
        // A robust spread of the residual: grain, not pores, at this size.
        let sigma = median(&mut d).unwrap_or(0.0) / 0.6745 * 0.8;
        if sigma > 0.014 {
            if let Some(e) = ops.push(f.op(def, p, FACE_SKIN, Tool::LocalDenoise, 0.6)) {
                e.tone = ((sigma - 0.01) / 0.03).clamp(0.1, 0.5);
                e.radius = 0.002;
                lines.push(format!(
                    "face {}: grain {sigma:.3} on the skin after the edit",
                    p.n + 1
                ));
            }
        }
    }
    if ops.edits.is_empty() {
        return Finding::checked("Skin is clean after the edit.");
    }
    ops.finding(
        format!("The edit lifted noise onto skin: {}.", lines.join("; ")),
        "Grain on that skin softened, its pores kept.".into(),
    )
}

fn eye_focus(f: &Faces<'_>) -> Finding {
    let w = f.r.w;
    let (mut soft, mut hidden) = (Vec::new(), Vec::new());
    let mut seen = 0;
    for p in f.judged().filter(|p| p.iod >= 40.0) {
        let eyes = f.pixels(p, &[Region::Eyes]);
        let skin = f.pixels(p, FACE_SKIN);
        let grad = |idx: &[usize]| {
            let mut g: Vec<f32> = idx
                .iter()
                .filter(|i| **i % w > 0 && **i % w + 1 < w && **i / w > 0 && **i / w + 1 < f.r.h)
                .map(|&i| {
                    let l = |j: usize| luma(Rendered::at(&f.r.bare, j));
                    (l(i + 1) - l(i - 1)).hypot(l(i + w) - l(i - w)) / 2.0
                })
                .collect();
            let n = g.len();
            if n < 30 {
                return None;
            }
            g.select_nth_unstable_by(n * 9 / 10, f32::total_cmp);
            Some(g[n * 9 / 10])
        };
        let around: Vec<usize> = [Region::Lips, Region::Nose, Region::Eyebrows, Region::Hair]
            .into_iter()
            .flat_map(|r| f.pixels(p, &[r]))
            .collect();
        let (Some(e), Some(_), Some(rest)) = (grad(&eyes), grad(&skin), grad(&around)) else {
            continue;
        };
        seen += 1;
        // Lashes and the iris's rim are the sharpest things on a face in focus.
        if e >= 0.035 {
            continue;
        }
        if rest < 0.035 {
            soft.push(format!(
                "face {} (eyes {e:.3}, the rest of the face {rest:.3} per pixel)",
                p.n + 1
            ));
        } else {
            hidden.push(format!(
                "face {} (eyes {e:.3}, the rest of the face {rest:.3})",
                p.n + 1
            ));
        }
    }
    if seen == 0 {
        return Finding::not_applicable("No face large enough to judge the focus on its eyes.");
    }
    if soft.is_empty() && hidden.is_empty() {
        return Finding::checked(format!(
            "The eyes are sharp on {}.",
            plural(seen, "face", "faces")
        ));
    }
    let mut parts = Vec::new();
    if !soft.is_empty() {
        parts.push(format!(
            "Soft focus on {}: the whole face is soft, and sharpening cannot put focus back.",
            soft.join("; ")
        ));
    }
    if !hidden.is_empty() {
        parts.push(format!(
            "Little detail in the eyes of {}, while the rest of the face is sharp: closed or looking down, behind a veil or sunglasses.",
            hidden.join("; ")
        ));
    }
    Finding::checked(format!("{} Flagged for review.", parts.join(" ")))
}

// ---- 45-46. Colour across one person -----------------------------------------------------------

fn side_light(f: &Faces<'_>, def: &StepDef) -> Finding {
    let mut ops = Ops::default();
    let mut lines = Vec::new();
    let k = f.r.w as f32 / f.map.width.max(1) as f32;
    for p in f.judged().filter(|p| p.iod >= 30.0) {
        let nose_x = p.geo.nose[0] * k;
        let ch = cheek(f, p);
        if veiled(f, &ch) {
            continue;
        }
        let (left, right): (Vec<usize>, Vec<usize>) =
            ch.iter().partition(|i| ((**i % f.r.w) as f32) < nose_x);
        if left.len() < 30 || right.len() < 30 {
            continue;
        }
        let colour = |idx: &[usize]| {
            mean3(
                &idx.iter()
                    .map(|i| log_chroma(Rendered::at(&f.r.full, *i)))
                    .collect::<Vec<_>>(),
            )
        };
        let (Some(l), Some(r)) = (colour(&left), colour(&right)) else {
            continue;
        };
        let d: [f32; 3] = std::array::from_fn(|c| l[c] - r[c]);
        let size = d.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        // A coloured light on one side: a strong hue difference between two cheeks that are lit
        // about as brightly. A lit cheek and a shaded one differ in colour by nature (sun and
        // skylight) - that is the light's shape, and it is kept.
        let (Some(bl), Some(br)) = (med_luma(&f.r.full, &left), med_luma(&f.r.full, &right)) else {
            continue;
        };
        if size < 0.1 || (bl - br).abs() > 0.06 {
            continue;
        }
        // The side further from the forehead's colour is the one the light coloured.
        let forehead: Vec<usize> = f
            .pixels(p, FACE_SKIN)
            .into_iter()
            .filter(|i| {
                ((*i / f.r.w) as f32)
                    < f32::midpoint(p.geo.left_eye[1], p.geo.right_eye[1]) * k - 0.4 * p.iod
            })
            .collect();
        let Some(top) = colour(&forehead) else {
            continue;
        };
        let dist = |c: [f32; 3]| {
            (0..3)
                .map(|j| (c[j] - top[j]).abs())
                .fold(0.0_f32, f32::max)
        };
        let (tinted_left, cast) = if dist(l) > dist(r) {
            (true, std::array::from_fn::<f32, 3, _>(|c| l[c] - top[c]))
        } else {
            (false, std::array::from_fn::<f32, 3, _>(|c| r[c] - top[c]))
        };
        if let Some(e) = ops.push(f.op(def, p, FACE_SKIN, Tool::SkinColor, 0.8)) {
            // Only that half of the face: the ellipse moves onto it.
            let half = e.region[2] / 2.0;
            e.region[0] += if tinted_left { -half } else { half };
            e.region[2] = half * 1.2;
            e.feather = 0.6;
            e.warmth = (-(cast[0] - cast[2]) * 0.6 / 0.5).clamp(-1.0, 1.0);
            e.tint = ((cast[1] - 0.5 * (cast[0] + cast[2])) * 0.6 / 0.24).clamp(-1.0, 1.0);
            lines.push(format!(
                "face {}: the {} cheek differs from the other by {:.0}% in colour",
                p.n + 1,
                if tinted_left {
                    "image-left"
                } else {
                    "image-right"
                },
                size * 100.0
            ));
        }
    }
    if ops.edits.is_empty() {
        return Finding::checked("Both cheeks of every face are the same colour.");
    }
    ops.finding(
        format!("A coloured light on one side: {}.", lines.join("; ")),
        "That side taken most of the way to the colour of the same person's forehead.".into(),
    )
}

fn face_neck_colour(f: &Faces<'_>, stack: &[Edit], def: &StepDef) -> Finding {
    let mut ops = Ops::default();
    let mut lines = Vec::new();
    // Auto enhance numbers faces its own way; any body match it placed covers the neck.
    if stack
        .iter()
        .any(|e| e.enabled && e.id.ends_with("-body-match"))
    {
        return Finding::checked(
            "Auto enhance already matched body skin, the neck with it, to each face.",
        );
    }
    let matched = 0;
    for p in f.judged() {
        let neck = f.pixels(p, NECK_SKIN);
        let face = cheek(f, p);
        if neck.len() < 40 || face.len() < 40 || veiled(f, &face) {
            continue;
        }
        let colour = |idx: &[usize]| {
            mean3(
                &idx.iter()
                    .map(|i| log_chroma(Rendered::at(&f.r.full, *i)))
                    .collect::<Vec<_>>(),
            )
        };
        let (Some(n), Some(c)) = (colour(&neck), colour(&face)) else {
            continue;
        };
        let d: [f32; 3] = std::array::from_fn(|k| n[k] - c[k]);
        let size = d.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        if size < 0.07 {
            continue;
        }
        if let Some(e) = ops.push(f.op(def, p, NECK_SKIN, Tool::SkinColor, 0.8)) {
            e.warmth = (-(d[0] - d[2]) * 0.6 / 0.5).clamp(-1.0, 1.0);
            e.tint = ((d[1] - 0.5 * (d[0] + d[2])) * 0.6 / 0.24).clamp(-1.0, 1.0);
            lines.push(format!(
                "face {}: neck differs from the cheek by {:.0}% in colour",
                p.n + 1,
                size * 100.0
            ));
        }
    }
    if ops.edits.is_empty() {
        return Finding::checked(if matched > 0 {
            format!(
                "Auto enhance already matched body skin to the face for {}; other necks match.",
                plural(matched, "person", "people")
            )
        } else {
            "Necks match their faces.".to_owned()
        });
    }
    ops.finding(
        format!("Neck and face colour differ: {}.", lines.join("; ")),
        "The neck taken most of the way to the same person's cheek colour, brightness kept.".into(),
    )
}

// ---- 47-49. Reported -----------------------------------------------------------------------------

fn mehndi_kept(f: &Faces<'_>) -> Finding {
    let w = f.r.w;
    let mut idx = Vec::new();
    for p in f.judged() {
        for i in f.pixels(p, &[Region::BodySkin]) {
            let c = Rendered::at(&f.r.bare, i);
            // Henna stain: a dark orange-brown, well below the skin's brightness.
            if hue(c).is_some_and(|h| (0.0..0.09).contains(&h))
                && saturation(c) > 0.45
                && luma(c) < 0.35
            {
                idx.push(i);
            }
        }
    }
    if idx.len() < 60 {
        return Finding::not_applicable("No mehndi pattern on the skin measured.");
    }
    let detail = |buf: &[u8]| {
        idx.iter()
            .filter(|i| **i % w > 0 && **i % w + 1 < w)
            .map(|&i| (luma(Rendered::at(buf, i + 1)) - luma(Rendered::at(buf, i - 1))).abs())
            .sum::<f32>()
    };
    let kept = detail(&f.r.full) / detail(&f.r.bare).max(1e-4);
    if kept >= 0.75 {
        return Finding::checked(format!(
            "Mehndi pattern measured; the retouch kept {:.0}% of its detail.",
            kept * 100.0
        ));
    }
    Finding::checked(format!(
        "The retouch softened the mehndi pattern to {:.0}% of its detail. Flagged for review: lower body skin smoothing or remove the operation over the hands.",
        kept * 100.0
    ))
}

fn makeup_kept(f: &Faces<'_>) -> Finding {
    let k = f.r.w as f32 / f.map.width.max(1) as f32;
    let mut lines = Vec::new();
    let mut seen = 0;
    for p in f.judged().filter(|p| p.iod >= 30.0) {
        // Between the eye and the brow: where eyeshadow sits.
        let eye_y = f32::midpoint(p.geo.left_eye[1], p.geo.right_eye[1]) * k;
        let lid: Vec<usize> = f
            .pixels(p, FACE_SKIN)
            .into_iter()
            .filter(|i| {
                let y = (*i / f.r.w) as f32;
                y < eye_y - 0.12 * p.iod && y > eye_y - 0.45 * p.iod
            })
            .collect();
        if lid.len() < 20 {
            continue;
        }
        seen += 1;
        let sat = |buf: &[u8]| {
            lid.iter()
                .map(|i| saturation(Rendered::at(buf, *i)))
                .sum::<f32>()
                / lid.len() as f32
        };
        let (b, a) = (sat(&f.r.bare), sat(&f.r.full));
        if b > 0.2 && a < b * 0.8 {
            lines.push(format!("face {}: eyelid colour {b:.2} -> {a:.2}", p.n + 1));
        }
    }
    if seen == 0 {
        return Finding::not_applicable("No eyelids measured clearly enough.");
    }
    if lines.is_empty() {
        return Finding::checked("Eye make-up keeps its colour after the retouch.");
    }
    Finding::checked(format!(
        "The retouch evened away eye make-up colour ({}). Flagged for review: lower tone evening.",
        lines.join("; ")
    ))
}

fn small_faces(f: &Faces<'_>) -> Finding {
    let small = f.people.iter().filter(|p| p.iod < SMALLEST_IOD).count();
    let beyond = f
        .people
        .iter()
        .filter(|p| p.iod >= SMALLEST_IOD)
        .count()
        .saturating_sub(MOST_PEOPLE);
    if small == 0 && beyond == 0 {
        return Finding::checked(format!(
            "All {} judged.",
            plural(f.people.len(), "face was", "faces were")
        ));
    }
    let mut parts = Vec::new();
    if small > 0 {
        parts.push(format!(
            "{} too small to judge (under {SMALLEST_IOD:.0} px between the eyes at 1600 px)",
            plural(small, "face is", "faces are")
        ));
    }
    if beyond > 0 {
        parts.push(format!("{beyond} more beyond the {MOST_PEOPLE} largest"));
    }
    Finding::checked(format!(
        "{}; Auto enhance's own retouch still covers them.",
        parts.join("; ")
    ))
}

// ---- 50. Neck in shadow ------------------------------------------------------------------------

fn neck_light(f: &Faces<'_>, def: &StepDef) -> Finding {
    let mut ops = Ops::default();
    let mut lines = Vec::new();
    for p in f.judged() {
        let neck = f.pixels(p, NECK_SKIN);
        let face = cheek(f, p);
        let (Some(n), Some(c)) = (med_luma(&f.r.full, &neck), med_luma(&f.r.full, &face)) else {
            continue;
        };
        if neck.len() < 40 || c - n < 0.18 || n < 0.12 || veiled(f, &face) {
            continue;
        }
        // Most of it must be the colour of this person's cheek: a neck, not a collar, a hand or
        // somebody else held below the chin.
        let Some(skin) = mean3(
            &face
                .iter()
                .map(|i| log_chroma(Rendered::at(&f.r.full, *i)))
                .collect::<Vec<_>>(),
        ) else {
            continue;
        };
        let alike = neck
            .iter()
            .filter(|i| {
                let c = log_chroma(Rendered::at(&f.r.full, **i));
                (0..3).all(|k| (c[k] - skin[k]).abs() < 0.12)
            })
            .count();
        if (alike as f32) < neck.len() as f32 * 0.7 {
            continue;
        }
        // Lift half the gap: a neck below a chin is in some shadow by nature.
        let stops = (decode(n + (c - n) * 0.5) / decode(n).max(1e-4))
            .log2()
            .clamp(0.15, 0.7);
        let amount = ((stops.exp2() - 1.0) / (0.75_f32.exp2() - 1.0)).clamp(0.1, 0.9);
        if ops
            .push(f.op(def, p, NECK_SKIN, Tool::Dodge, amount))
            .is_some()
        {
            lines.push(format!(
                "face {}: neck {n:.2} under a cheek of {c:.2}",
                p.n + 1
            ));
        }
    }
    if ops.edits.is_empty() {
        return Finding::checked("No neck sits in deep shadow under its face.");
    }
    ops.finding(
        format!("A neck in deep shadow: {}.", lines.join("; ")),
        "The neck lifted half way toward the face, its shadow kept.".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photo_pass::scene::Capture;
    use crate::portrait_features::Pixels;

    #[test]
    fn nobody_in_the_frame_makes_every_face_step_not_applicable() {
        let grey = vec![120_u8; 64 * 64 * 3];
        let s = Scene::new(
            Pixels::new(&grey, 64, 64).unwrap(),
            Vec::new(),
            Capture::default(),
            None,
        );
        let base = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t");
        let found = plan(&s, None, &base);
        assert_eq!(found.len(), 25);
        assert!(found
            .iter()
            .all(|f| f.outcome == crate::photo_pass::Outcome::NotApplicable));
    }

    use aura_portrait::fixtures::{portrait, PortraitSpec, MONK};
    use aura_vision::portrait::PortraitFace;

    /// A painted portrait, parsed by the parse the renderer uses, as the pass would see it with
    /// the edit changing nothing: the original, the bare render and the full render the same.
    fn painted(
        skin: [u8; 3],
        ground: [u8; 3],
        paint: impl Fn(&mut Vec<u8>, &aura_portrait::fixtures::Portrait),
    ) -> Option<(Vec<u8>, PortraitFace, std::sync::Arc<PortraitMap>)> {
        let spec = PortraitSpec {
            width: 384,
            height: 384,
            skin,
            ground,
            ..PortraitSpec::default()
        };
        let p = portrait(&spec);
        let mut rgb = p.rgb.clone();
        paint(&mut rgb, &p);
        let canvas =
            aura_portrait::Canvas::from_srgb8(&rgb, 384, 384, aura_portrait::ANALYSIS_EDGE)?;
        let map = aura_portrait::analyse(&canvas, &[]);
        if map.faces.is_empty() {
            return None;
        }
        let (cx, cy, r) = (spec.centre[0], spec.centre[1], spec.radius);
        let face = PortraitFace {
            bounds: [cx - r, cy - r * 4.0 / 3.0, cx + r, cy + r * 4.0 / 3.0],
            landmarks: [[0.0; 2]; 5],
            confidence: 0.99,
        };
        Some((rgb, face, std::sync::Arc::new(map)))
    }

    fn outcomes(
        rgb: &[u8],
        face: PortraitFace,
        map: std::sync::Arc<PortraitMap>,
    ) -> Vec<(u8, crate::photo_pass::Outcome, String)> {
        let s = Scene::new(
            Pixels::new(rgb, 384, 384).unwrap(),
            vec![face],
            Capture::default(),
            None,
        )
        .with_map(Some(map));
        let r = Rendered {
            bare: rgb.to_vec(),
            full: rgb.to_vec(),
            w: 384,
            h: 384,
        };
        let base = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t");
        plan(&s, Some(&r), &base)
            .into_iter()
            .zip(STEPS.iter().skip(25))
            .map(|(f, d)| (d.number, f.outcome, f.checks.join(" ")))
            .collect()
    }

    #[test]
    fn no_face_is_brightened_for_its_skin_tone_in_front_of_any_background() {
        let mut parsed = 0;
        for tone in MONK {
            for ground in [[120, 120, 120], [240, 240, 240], [30, 30, 30]] {
                let Some((rgb, face, map)) = painted(tone, ground, |_, _| {}) else {
                    continue;
                };
                parsed += 1;
                let found = outcomes(&rgb, face, map);
                assert_ne!(
                    found[0].1,
                    crate::photo_pass::Outcome::Applied,
                    "{tone:?} on {ground:?}: {}",
                    found[0].2
                );
            }
        }
        assert!(
            parsed >= 20,
            "the parse found too few painted faces to test ({parsed})"
        );
    }

    #[test]
    fn a_face_the_edit_left_alone_needs_nothing_at_any_skin_tone() {
        let mut parsed = 0;
        for tone in MONK {
            let Some((rgb, face, map)) = painted(tone, [120, 120, 120], |_, _| {}) else {
                continue;
            };
            parsed += 1;
            for (n, outcome, why) in outcomes(&rgb, face, map) {
                assert_ne!(
                    outcome,
                    crate::photo_pass::Outcome::Applied,
                    "step {n} at {tone:?}: {why}"
                );
            }
        }
        assert!(
            parsed >= 7,
            "the parse found too few painted faces to test ({parsed})"
        );
    }

    #[test]
    fn a_face_behind_a_red_veil_keeps_the_veils_colour() {
        // A translucent red net over the face: each pixel blended toward red. The strongest blend
        // the parse still finds a face through is the one tested.
        let veil = |k: f32| {
            move |rgb: &mut Vec<u8>, p: &aura_portrait::fixtures::Portrait| {
                for y in 0..384_u32 {
                    for x in 0..384_u32 {
                        if p.face.at(i64::from(x), i64::from(y)) > 0.5 {
                            let i = ((y * 384 + x) * 3) as usize;
                            for (c, red) in [235.0_f32, 25.0, 45.0].into_iter().enumerate() {
                                rgb[i + c] = (f32::from(rgb[i + c]) * (1.0 - k) + red * k) as u8;
                            }
                        }
                    }
                }
            }
        };
        let (k, (rgb, face, map)) = [0.6_f32, 0.55, 0.5, 0.45, 0.4]
            .into_iter()
            .find_map(|k| painted(MONK[3], [120, 120, 120], veil(k)).map(|v| (k, v)))
            .expect("the parse finds the face through some veil");
        let found = outcomes(&rgb, face, map);
        for (n, outcome, why) in &found {
            if [28, 29, 45, 46, 50].contains(n) {
                assert_ne!(
                    *outcome,
                    crate::photo_pass::Outcome::Applied,
                    "veil {k}, step {n}: {why}"
                );
            }
        }
        // The guard is what held them back, not a face the steps never looked at.
        assert!(
            found
                .iter()
                .any(|(n, _, why)| *n == 29 && why.contains("veil")),
            "veil {k}: {found:?}"
        );
    }

    #[test]
    fn log_chroma_ignores_brightness() {
        let a = log_chroma([0.4, 0.3, 0.2]);
        let b = log_chroma([0.8, 0.6, 0.4]);
        assert!((0..3).all(|c| (a[c] - b[c]).abs() < 1e-5));
    }
}
