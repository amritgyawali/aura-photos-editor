//! From faces to regions: every plane measured relative to the person it belongs to.
//!
//! # One unit for every face
//!
//! Every facial region is sized in **interocular distances** and placed in the face's own
//! frame - across the eye line and down from it - so a guest twenty pixels tall and a bride
//! filling the frame, upright or tilted, are parsed by the same numbers. The anthropometry is
//! textbook: a face is about two interocular distances wide at the cheekbones, the mouth sits
//! about one below the eyes, the brows a third above them, the iris is a tenth of one across.
//!
//! # A region is geometry *times* evidence
//!
//! Each facial region starts as a soft shape where that feature must be, and the pixels then
//! decide how much of the shape is the feature: a lip is redder or darker than *this person's*
//! skin, a tooth is brighter and less saturated than *this person's* lips, a brow is darker than
//! their forehead. Geometry alone puts a whitening operator on a closed mouth; evidence alone
//! finds a red scarf. The product of the two does neither.
//!
//! # What nobody found stays empty
//!
//! A frame with no face has no skin, no eyes and no teeth - not a guess at them. The retouch
//! operators gate on that, and the panel offers to let a person draw the face instead.

use crate::canvas::{Canvas, Primaries};
use crate::face::{self, FaceGeometry, FaceHint};
use crate::plane::{Ellipse, Plane};
use crate::regions::{Region, ALL_REGIONS};
use crate::skin::{self, ramp, SkinModel};

/// The long edge regions are measured on.
///
/// 1024: a face filling a third of a portrait is then 340 px across and a front tooth is a dozen
/// pixels wide - enough to tell from the lip - while a 24 MP original costs one box-filtered
/// pass to get here.
pub const ANALYSIS_EDGE: u32 = 1024;

/// The arithmetic these regions were produced under. Bumping it re-parses every frame.
pub const PARSE_VER: u16 = 1;

/// How sure the parse is of one region, and how much of the frame it covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionStat {
    /// The region.
    pub region: Region,
    /// Share of the frame, `0..=1`.
    pub coverage: f32,
    /// `0..=1`.
    pub confidence: f32,
}

/// Everything one photograph's parse produced, on the analysis grid.
#[derive(Debug, Clone, PartialEq)]
pub struct PortraitMap {
    /// Analysis grid width.
    pub width: u32,
    /// Analysis grid height.
    pub height: u32,
    /// The faces, largest first.
    pub faces: Vec<FaceGeometry>,
    /// One plane per region, in [`ALL_REGIONS`] order.
    pub planes: Vec<(Region, Plane)>,
    /// One statistic per region, in [`ALL_REGIONS`] order.
    pub stats: Vec<RegionStat>,
    /// True when the frame's colour was informative.
    pub colourful: bool,
}

impl PortraitMap {
    /// The plane of one region on the analysis grid.
    #[must_use]
    pub fn plane(&self, region: Region) -> Option<&Plane> {
        self.planes
            .iter()
            .find(|(r, _)| *r == region)
            .map(|(_, p)| p)
    }

    /// The statistic for one region.
    #[must_use]
    pub fn stat(&self, region: Region) -> Option<RegionStat> {
        self.stats.iter().find(|s| s.region == region).copied()
    }

    /// One region resolved onto another grid, bilinearly.
    #[must_use]
    pub fn resolve(&self, region: Region, width: u32, height: u32) -> Plane {
        self.plane(region).map_or_else(
            || Plane::zeros(width, height),
            |plane| plane.resize(width, height),
        )
    }

    /// The parse of a frame nobody could read.
    #[must_use]
    pub fn empty(width: u32, height: u32) -> Self {
        let planes = ALL_REGIONS
            .iter()
            .map(|r| {
                let fill = if *r == Region::Background { 1.0 } else { 0.0 };
                (*r, Plane::filled(width, height, fill))
            })
            .collect();
        let stats = ALL_REGIONS
            .iter()
            .map(|r| RegionStat {
                region: *r,
                coverage: if *r == Region::Background { 1.0 } else { 0.0 },
                confidence: 0.0,
            })
            .collect();
        Self {
            width,
            height,
            faces: Vec::new(),
            planes,
            stats,
            colourful: false,
        }
    }
}

/// Parse a linear frame, downsampling it onto the analysis grid first.
#[must_use]
pub fn analyse_linear(
    rgb: &[f32],
    width: u32,
    height: u32,
    primaries: Primaries,
    hints: &[FaceHint],
) -> Option<PortraitMap> {
    let canvas = Canvas::from_linear(rgb, width, height, primaries, ANALYSIS_EDGE)?;
    Some(analyse(&canvas, hints))
}

/// The face's own frame: an origin at the eye midpoint, an across axis along the eye line, a
/// down axis perpendicular to it, and one unit of interocular distance.
#[derive(Debug, Clone, Copy)]
struct FaceFrame {
    origin: [f32; 2],
    across: [f32; 2],
    down: [f32; 2],
    unit: f32,
    /// How far the mouth sits below the eye line, in units.
    mouth_down: f32,
}

impl FaceFrame {
    fn of(face: &FaceGeometry) -> Self {
        let (s, c) = face.roll.sin_cos();
        let origin = face.eye_mid();
        let unit = face.interocular().max(2.0);
        let down = [-s, c];
        let to_mouth = [face.mouth[0] - origin[0], face.mouth[1] - origin[1]];
        let mouth_down = ((to_mouth[0] * down[0] + to_mouth[1] * down[1]) / unit).clamp(0.7, 1.5);
        Self {
            origin,
            across: [c, s],
            down,
            unit,
            mouth_down,
        }
    }

    /// Canvas pixel to face coordinates.
    fn local(&self, x: f32, y: f32) -> (f32, f32) {
        let dx = x - self.origin[0];
        let dy = y - self.origin[1];
        (
            (dx * self.across[0] + dy * self.across[1]) / self.unit,
            (dx * self.down[0] + dy * self.down[1]) / self.unit,
        )
    }

    /// Face coordinates to a canvas pixel.
    fn point(&self, a: f32, b: f32) -> [f32; 2] {
        [
            self.origin[0] + (self.across[0] * a + self.down[0] * b) * self.unit,
            self.origin[1] + (self.across[1] * a + self.down[1] * b) * self.unit,
        ]
    }

    /// An ellipse in face coordinates as a canvas ellipse.
    fn ellipse(&self, a: f32, b: f32, ra: f32, rb: f32) -> Ellipse {
        let c = self.point(a, b);
        Ellipse {
            cx: c[0],
            cy: c[1],
            rx: ra * self.unit,
            ry: rb * self.unit,
            angle: self.across[1].atan2(self.across[0]),
        }
    }

    /// The canvas pixels covering a face-coordinate box, clamped into the canvas.
    fn bounds(&self, a0: f32, a1: f32, b0: f32, b1: f32, w: u32, h: u32) -> [u32; 4] {
        let corners = [
            self.point(a0, b0),
            self.point(a1, b0),
            self.point(a0, b1),
            self.point(a1, b1),
        ];
        let x0 = corners.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
        let x1 = corners
            .iter()
            .map(|p| p[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let y0 = corners.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let y1 = corners
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let clamp_x = |v: f32| v.clamp(0.0, w.saturating_sub(1) as f32) as u32;
        let clamp_y = |v: f32| v.clamp(0.0, h.saturating_sub(1) as f32) as u32;
        [
            clamp_x(x0.floor()),
            clamp_x(x1.ceil()),
            clamp_y(y0.floor()),
            clamp_y(y1.ceil()),
        ]
    }
}

/// One person's measurements, shared by every region built for them.
#[derive(Debug, Clone)]
struct Person {
    face: FaceGeometry,
    frame: FaceFrame,
    model: Option<SkinModel>,
}

impl Person {
    /// How much a pixel looks like this person's skin.
    fn skin_like(&self, lab: [f32; 3], colourful: bool) -> f32 {
        match (&self.model, colourful) {
            (Some(model), true) => model.likelihood(lab),
            (Some(model), false) => {
                // No colour: lightness is all there is, and it is weak evidence.
                let lo = model.lightness.0 - 14.0;
                let hi = model.lightness.1 + 10.0;
                0.7 * ramp(lab[0], lo - 8.0, lo) * (1.0 - ramp(lab[0], hi, hi + 8.0))
            }
            (None, _) => 0.0,
        }
    }

    fn skin_l(&self) -> f32 {
        self.model.map_or(55.0, |m| m.mean[0])
    }

    fn skin_l_sd(&self) -> f32 {
        self.model.map_or(8.0, |m| m.lightness_sd.max(3.0))
    }

    fn skin_a(&self) -> f32 {
        self.model.map_or(12.0, |m| m.mean[1])
    }
}

/// Parse a canvas.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn analyse(canvas: &Canvas, hints: &[FaceHint]) -> PortraitMap {
    let (w, h) = (canvas.width, canvas.height);
    if canvas.is_empty() {
        return PortraitMap::empty(w.max(1), h.max(1));
    }
    let colourful = canvas.is_colourful();
    let faces = face::detect(canvas, hints);
    let people: Vec<Person> = faces
        .iter()
        .map(|f| {
            let frame = FaceFrame::of(f);
            let model = fit_skin(canvas, &frame, colourful);
            Person {
                face: *f,
                frame,
                model,
            }
        })
        .collect();

    let mut map = PortraitMap::empty(w, h);
    map.faces = faces;
    map.colourful = colourful;
    let sky = sky(canvas);
    if people.is_empty() {
        set(&mut map, Region::Sky, sky.clone());
        set(&mut map, Region::Background, Plane::filled(w, h, 1.0));
        finish_stats(&mut map, &[]);
        return map;
    }

    let mut skin_plane = Plane::zeros(w, h);
    let mut face_plane = Plane::zeros(w, h);
    let mut core_plane = Plane::zeros(w, h);
    let mut eyes = Plane::zeros(w, h);
    let mut iris = Plane::zeros(w, h);
    let mut sclera = Plane::zeros(w, h);
    let mut brows = Plane::zeros(w, h);
    let mut under = Plane::zeros(w, h);
    let mut nose = Plane::zeros(w, h);
    let mut lips = Plane::zeros(w, h);
    let mut teeth = Plane::zeros(w, h);
    let mut mouth = Plane::zeros(w, h);
    let mut beard = Plane::zeros(w, h);
    let mut neck = Plane::zeros(w, h);
    let mut hair = Plane::zeros(w, h);
    let mut body = Plane::zeros(w, h);
    let mut head = Plane::zeros(w, h);
    let mut torso_skin_zone = Plane::zeros(w, h);

    for person in &people {
        skin_for(
            canvas,
            person,
            colourful,
            &mut skin_plane,
            &mut torso_skin_zone,
        );
        oval_for(canvas, person, colourful, &mut face_plane, &mut core_plane);
        for eye in [person.face.left_eye, person.face.right_eye] {
            eye_for(
                canvas,
                person,
                eye,
                colourful,
                &mut eyes,
                &mut iris,
                &mut sclera,
            );
            brow_for(canvas, person, eye, colourful, &mut brows);
            under_for(canvas, person, eye, colourful, &mut under);
        }
        nose_for(person, &mut nose);
        mouth_for(canvas, person, colourful, &mut lips, &mut teeth, &mut mouth);
        beard_for(canvas, person, colourful, &mut beard);
        neck_for(canvas, person, colourful, &mut neck);
        hair_for(canvas, person, colourful, &mut hair);
        body_for(canvas, person, &people, &mut body);
        // A conservative head silhouette: ears, temples and the hair nearest the scalp belong
        // to the person even where no colour test claimed them, and a background operation
        // that softened an ear would be the most visible mistake this parse could make.
        head.paint_ellipse(&person.frame.ellipse(0.0, 0.05, 1.2, 1.75), 0.15);
    }

    // Features are not skin and are not hair; hair is not face.
    let features = eyes.union(&brows).union(&lips).union(&teeth).union(&mouth);
    hair = hair.subtract(&core_plane);
    let face_plane = face_plane
        .subtract(&hair)
        .union(&core_plane.subtract(&hair));
    beard = beard.subtract(&mouth);
    let skin_clean = skin_plane
        .subtract(&features)
        .subtract(&hair)
        .subtract(&beard)
        .open(1)
        .blur(1);
    let under = under.mul(&skin_clean.map(|v| ramp(v, 0.15, 0.5)));
    let nose = nose.mul(&face_plane);
    let neck = neck.subtract(&face_plane.map(|v| ramp(v, 0.5, 0.9)));
    let body_skin = skin_clean
        .subtract(&face_plane)
        .subtract(&neck)
        .mul(&torso_skin_zone);
    let person_plane = body
        .union(&face_plane)
        .union(&hair)
        .union(&neck)
        .union(&skin_clean)
        .union(&head)
        .close(2)
        .blur(1);
    let clothing = body
        .subtract(&skin_clean)
        .subtract(&hair)
        .subtract(&face_plane)
        .open(1);
    let background = person_plane.invert();
    let sky = sky.mul(&background);

    set(&mut map, Region::Skin, skin_clean);
    set(&mut map, Region::Face, face_plane);
    set(&mut map, Region::Eyes, eyes);
    set(&mut map, Region::Iris, iris);
    set(&mut map, Region::Sclera, sclera);
    set(&mut map, Region::Eyebrows, brows);
    set(&mut map, Region::UnderEyes, under);
    set(&mut map, Region::Nose, nose);
    set(&mut map, Region::Lips, lips);
    set(&mut map, Region::Teeth, teeth);
    set(&mut map, Region::Mouth, mouth);
    set(&mut map, Region::FacialHair, beard);
    set(&mut map, Region::Neck, neck);
    set(&mut map, Region::Hair, hair);
    set(&mut map, Region::BodySkin, body_skin);
    set(&mut map, Region::Clothing, clothing);
    set(&mut map, Region::Body, person_plane);
    set(&mut map, Region::Background, background);
    set(&mut map, Region::Sky, sky);
    finish_stats(&mut map, &people);
    map
}

fn set(map: &mut PortraitMap, region: Region, mut plane: Plane) {
    plane.clamp_unit();
    if let Some(slot) = map.planes.iter_mut().find(|(r, _)| *r == region) {
        slot.1 = plane;
    }
}

/// A region's confidence: how sure the faces it came from were, how much of each face's
/// measurement was evidence rather than prior, and whether the frame had colour to read.
fn finish_stats(map: &mut PortraitMap, people: &[Person]) {
    let face_conf = if people.is_empty() {
        0.0
    } else {
        people.iter().map(|p| p.face.confidence).sum::<f32>() / people.len() as f32
    };
    let eye_share = if people.is_empty() {
        0.0
    } else {
        people
            .iter()
            .map(|p| f32::from(p.face.eyes_measured) / 2.0)
            .sum::<f32>()
            / people.len() as f32
    };
    let mouth_share = if people.is_empty() {
        0.0
    } else {
        people
            .iter()
            .map(|p| f32::from(u8::from(p.face.mouth_measured)))
            .sum::<f32>()
            / people.len() as f32
    };
    let modelled = if people.is_empty() {
        0.0
    } else {
        people.iter().filter(|p| p.model.is_some()).count() as f32 / people.len() as f32
    };
    let colour = if map.colourful { 1.0 } else { 0.6 };
    let stats: Vec<RegionStat> = map
        .planes
        .iter()
        .map(|(region, plane)| {
            let coverage = plane.coverage();
            let confidence = match region {
                Region::Sky | Region::Background => {
                    if coverage > 0.0 {
                        0.6 + 0.3 * face_conf
                    } else {
                        0.0
                    }
                }
                _ if coverage <= 1e-5 => 0.0,
                Region::Skin | Region::BodySkin | Region::Neck => face_conf * modelled * colour,
                Region::Face | Region::Nose | Region::UnderEyes => {
                    face_conf * (0.6 + 0.4 * modelled)
                }
                Region::Eyes | Region::Iris | Region::Sclera | Region::Eyebrows => {
                    face_conf * (0.5 + 0.5 * eye_share)
                }
                Region::Lips | Region::Teeth | Region::Mouth | Region::FacialHair => {
                    face_conf * (0.45 + 0.55 * mouth_share) * colour
                }
                Region::Hair | Region::Clothing | Region::Body => face_conf * 0.8,
            };
            RegionStat {
                region: *region,
                coverage,
                confidence: confidence.clamp(0.0, 1.0),
            }
        })
        .collect();
    map.stats = stats;
}

/// Sample this person's cheeks, forehead, nose bridge and chin and fit their skin.
fn fit_skin(canvas: &Canvas, frame: &FaceFrame, colourful: bool) -> Option<SkinModel> {
    let md = frame.mouth_down;
    let patches = [
        (-0.75, -0.25, 0.35, 0.8),
        (0.25, 0.75, 0.35, 0.8),
        (-0.4, 0.4, -0.85, -0.5),
        (-0.1, 0.1, 0.05, 0.45),
        (-0.25, 0.25, md + 0.35, md + 0.6),
    ];
    let step = (1.0 / frame.unit).max(1.0 / 24.0);
    let mut samples = Vec::new();
    for (a0, a1, b0, b1) in patches {
        let mut b = b0;
        while b <= b1 {
            let mut a = a0;
            while a <= a1 {
                let p = frame.point(a, b);
                let lab = canvas.lab_at(p[0] as i64, p[1] as i64);
                if (3.0..99.0).contains(&lab[0]) {
                    let weight = if colourful { skin::prior(lab) } else { 1.0 };
                    samples.push((lab, weight));
                }
                a += step;
            }
            b += step;
        }
    }
    SkinModel::fit(&samples).or_else(|| {
        let flat: Vec<([f32; 3], f32)> = samples.iter().map(|(lab, _)| (*lab, 1.0)).collect();
        SkinModel::fit(&flat)
    })
}

/// Iterate the canvas pixels inside a face-coordinate box.
fn for_box(
    canvas: &Canvas,
    frame: &FaceFrame,
    bounds: (f32, f32, f32, f32),
    mut f: impl FnMut(i64, i64, f32, f32, [f32; 3]),
) {
    let [x0, x1, y0, y1] = frame.bounds(
        bounds.0,
        bounds.1,
        bounds.2,
        bounds.3,
        canvas.width,
        canvas.height,
    );
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (a, b) = frame.local(x as f32 + 0.5, y as f32 + 0.5);
            if a < bounds.0 || a > bounds.1 || b < bounds.2 || b > bounds.3 {
                continue;
            }
            let lab = canvas.lab_at(i64::from(x), i64::from(y));
            f(i64::from(x), i64::from(y), a, b, lab);
        }
    }
}

fn raise(plane: &mut Plane, x: i64, y: i64, value: f32) {
    if value > plane.at(x, y) {
        plane.set(x, y, value);
    }
}

/// A working plane covering one person's bounding box rather than the whole frame.
///
/// Morphology and blurs on a whole-frame plane once per person made a group photograph cost
/// faces times frame; on the person's own box it costs the person.
#[derive(Debug, Clone)]
struct Patch {
    x0: i64,
    y0: i64,
    plane: Plane,
}

impl Patch {
    fn over(frame: &FaceFrame, bounds: (f32, f32, f32, f32), canvas: &Canvas) -> Self {
        let [x0, x1, y0, y1] = frame.bounds(
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            canvas.width,
            canvas.height,
        );
        Self {
            x0: i64::from(x0),
            y0: i64::from(y0),
            plane: Plane::zeros(x1 - x0 + 1, y1 - y0 + 1),
        }
    }

    fn set(&mut self, x: i64, y: i64, value: f32) {
        self.plane.set(x - self.x0, y - self.y0, value);
    }

    fn with(&self, plane: Plane) -> Self {
        Self {
            x0: self.x0,
            y0: self.y0,
            plane,
        }
    }

    /// Raise the target to this patch wherever the patch is higher.
    fn paste_max(&self, target: &mut Plane) {
        let w = i64::from(self.plane.width);
        for (i, value) in self.plane.values.iter().enumerate() {
            if *value <= 0.0 {
                continue;
            }
            let x = self.x0 + i as i64 % w.max(1);
            let y = self.y0 + i as i64 / w.max(1);
            raise(target, x, y, *value);
        }
    }
}

/// A soft ellipse in face units: one inside, falling to zero across `feather` of the radius.
/// No trigonometry - the face frame already turned the pixel into the face's own axes - which
/// is what keeps a twenty-four-face group photograph from costing a second.
fn ell(a: f32, b: f32, centre: [f32; 2], ra: f32, rb: f32, feather: f32) -> f32 {
    let d = (((a - centre[0]) / ra).powi(2) + ((b - centre[1]) / rb).powi(2)).sqrt();
    let t = ((d - (1.0 - feather * 0.5)) / feather.max(1e-4)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

fn soft_box(a: f32, b: f32, a_half: f32, b0: f32, b1: f32, edge: f32) -> f32 {
    let ax = 1.0 - ramp(a.abs(), a_half - edge, a_half + edge);
    let top = ramp(b, b0 - edge, b0 + edge);
    let bottom = 1.0 - ramp(b, b1 - edge, b1 + edge);
    ax * top * bottom
}

/// Skin, everywhere this person could have skin: a head zone and a widening torso zone.
fn skin_for(
    canvas: &Canvas,
    person: &Person,
    colourful: bool,
    skin_plane: &mut Plane,
    torso_zone: &mut Plane,
) {
    if person.model.is_none() {
        return;
    }
    let md = person.frame.mouth_down;
    for_box(
        canvas,
        &person.frame,
        (-4.6, 4.6, -2.2, 12.0),
        |x, y, a, b, lab| {
            let head_w = ell(a, b, [0.0, 0.25], 1.35, 1.85, 0.25);
            let neck_w = soft_box(a, b, 0.9, md + 0.4, md + 2.0, 0.25);
            let torso_w = if colourful {
                let half = 3.4 + 0.25 * (b - md - 1.8).max(0.0);
                soft_box(a, b, half, md + 1.6, 16.0, 0.5)
            } else {
                0.0
            };
            let zone = head_w.max(neck_w).max(torso_w);
            if zone <= 0.0 {
                return;
            }
            if torso_w > 0.0 {
                raise(torso_zone, x, y, torso_w);
            }
            let like = person.skin_like(lab, colourful);
            raise(skin_plane, x, y, like * zone);
        },
    );
}

/// The face oval and its feature core.
fn oval_for(
    canvas: &Canvas,
    person: &Person,
    colourful: bool,
    face_plane: &mut Plane,
    core_plane: &mut Plane,
) {
    let md = person.frame.mouth_down;
    for_box(
        canvas,
        &person.frame,
        (-1.4, 1.4, -1.6, md + 1.2),
        |x, y, a, b, lab| {
            let o = ell(a, b, [0.0, 0.32], 1.05, 1.62, 0.18);
            if o <= 0.0 {
                return;
            }
            let c = ell(a, b, [0.0, md * 0.42], 0.86, md * 0.55 + 0.62, 0.2);
            let like = if person.model.is_some() {
                ramp(person.skin_like(lab, colourful), 0.1, 0.45)
            } else {
                0.6
            };
            raise(face_plane, x, y, o * like.max(c));
            raise(core_plane, x, y, c);
        },
    );
}

/// One eye: the opening, the iris and the sclera.
#[allow(clippy::too_many_arguments)]
fn eye_for(
    canvas: &Canvas,
    person: &Person,
    eye: [f32; 2],
    colourful: bool,
    eyes: &mut Plane,
    iris: &mut Plane,
    sclera: &mut Plane,
) {
    let frame = &person.frame;
    let (ea, eb) = frame.local(eye[0], eye[1]);
    let opening = frame.ellipse(ea, eb, 0.29, 0.125);
    let skin_l = person.skin_l();
    let skin_a = person.skin_a();
    let bounds = (ea - 0.4, ea + 0.4, eb - 0.22, eb + 0.22);

    // The iris: the darkest compact blob in the opening.
    let mut dark: Vec<(f32, f32, f32)> = Vec::new();
    for_box(canvas, frame, bounds, |x, y, _, _, lab| {
        let w = opening.weight(x as f32 + 0.5, y as f32 + 0.5, 0.0);
        if w > 0.0 {
            dark.push((lab[0], x as f32 + 0.5, y as f32 + 0.5));
        }
    });
    let mut lights: Vec<f32> = dark.iter().map(|d| d.0).collect();
    lights.sort_by(f32::total_cmp);
    let darkest = lights.first().copied().unwrap_or(0.0);
    let median = lights.get(lights.len() / 2).copied().unwrap_or(50.0);
    let cut = darkest + (median - darkest) * 0.35;
    let (mut sx, mut sy, mut total) = (0.0, 0.0, 0.0);
    for (l, x, y) in &dark {
        if *l <= cut {
            let weight = cut - l + 0.5;
            sx += weight * x;
            sy += weight * y;
            total += weight;
        }
    }
    let iris_centre = if total > 0.0 && median - darkest > 8.0 {
        [sx / total, sy / total]
    } else {
        eye
    };
    let iris_disc = Ellipse {
        cx: iris_centre[0],
        cy: iris_centre[1],
        rx: frame.unit * 0.1,
        ry: frame.unit * 0.1,
        angle: 0.0,
    };
    let iris_l = darkest + (median - darkest) * 0.25;

    for_box(canvas, frame, bounds, |x, y, _, _, lab| {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let shape = opening.weight(px, py, 0.35);
        if shape <= 0.0 {
            return;
        }
        let darker = ramp(skin_l - lab[0], 15.0, 30.0);
        let whiter = if colourful {
            ramp(skin_a - lab[1], 2.0, 7.0) * ramp(lab[0], iris_l + 15.0, iris_l + 30.0)
        } else {
            0.0
        };
        let not_skin = if colourful {
            (1.0 - person.skin_like(lab, colourful)) * 0.7
        } else {
            0.0
        };
        let evidence = darker.max(whiter).max(not_skin);
        let value = (shape * (0.5 + 0.5 * evidence) / 0.75).min(1.0);
        raise(eyes, x, y, value);
        let disc = iris_disc.weight(px, py, 0.3);
        raise(iris, x, y, (disc * value).min(1.0));
        let bright = ramp(lab[0], iris_l + 10.0, iris_l + 25.0);
        raise(sclera, x, y, (value * (1.0 - disc) * bright).min(1.0));
    });
}

/// One eyebrow: a band above the eye, darker than the forehead.
fn brow_for(canvas: &Canvas, person: &Person, eye: [f32; 2], colourful: bool, brows: &mut Plane) {
    let frame = &person.frame;
    let (ea, eb) = frame.local(eye[0], eye[1]);
    let outward = if ea < 0.0 { -0.03 } else { 0.03 };
    let band = frame.ellipse(ea + outward, eb - 0.34, 0.36, 0.12);
    let skin_l = person.skin_l();
    let sd = person.skin_l_sd();
    for_box(
        canvas,
        frame,
        (ea - 0.5, ea + 0.5, eb - 0.55, eb - 0.12),
        |x, y, _, _, lab| {
            let shape = band.weight(x as f32 + 0.5, y as f32 + 0.5, 0.4);
            if shape <= 0.0 {
                return;
            }
            let darker = ramp(skin_l - lab[0], 4.0 + 0.5 * sd, 12.0 + sd);
            let not_skin = if colourful {
                (1.0 - person.skin_like(lab, colourful)) * 0.6
            } else {
                0.0
            };
            raise(brows, x, y, shape * ramp(darker.max(not_skin), 0.3, 0.6));
        },
    );
}

/// The crescent under one eye.
fn under_for(canvas: &Canvas, person: &Person, eye: [f32; 2], colourful: bool, under: &mut Plane) {
    let frame = &person.frame;
    let (ea, eb) = frame.local(eye[0], eye[1]);
    let crescent = frame.ellipse(ea, eb + 0.25, 0.3, 0.12);
    let opening = frame.ellipse(ea, eb, 0.33, 0.16);
    for_box(
        canvas,
        frame,
        (ea - 0.45, ea + 0.45, eb + 0.05, eb + 0.45),
        |x, y, _, _, lab| {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let shape = crescent.weight(px, py, 0.5) * (1.0 - opening.weight(px, py, 0.2));
            if shape <= 0.0 {
                return;
            }
            let like = if person.model.is_some() {
                ramp(person.skin_like(lab, colourful), 0.05, 0.35)
            } else {
                0.0
            };
            raise(under, x, y, shape * like);
        },
    );
}

fn nose_for(person: &Person, nose: &mut Plane) {
    let frame = &person.frame;
    let md = frame.mouth_down;
    let shape = frame.ellipse(0.0, md * 0.55, 0.22, md * 0.42);
    nose.paint_ellipse(&shape, 0.4);
}

/// Lips, teeth and the mouth around them.
fn mouth_for(
    canvas: &Canvas,
    person: &Person,
    colourful: bool,
    lips: &mut Plane,
    teeth: &mut Plane,
    mouth: &mut Plane,
) {
    let frame = &person.frame;
    let centre = person.face.mouth;
    let (ma, mb) = frame.local(centre[0], centre[1]);
    let width = (person.face.mouth_width / frame.unit).clamp(0.55, 1.3);
    let outer = frame.ellipse(ma, mb, width * 0.6, width * 0.34);
    let inner = frame.ellipse(ma, mb, width * 0.46, width * 0.2);
    let skin_l = person.skin_l();
    let skin_a = person.skin_a();
    let bounds = (
        ma - width * 0.75,
        ma + width * 0.75,
        mb - width * 0.45,
        mb + width * 0.45,
    );

    // First pass: lip evidence, and the lips' own colour for the teeth test.
    let mut lip_samples: Vec<([f32; 3], f32)> = Vec::new();
    let lip_score = |lab: [f32; 3]| -> f32 {
        let darker = (skin_l - lab[0]).max(0.0);
        if colourful {
            let redder = lab[1] - skin_a;
            ramp(redder + 0.25 * darker, 2.5, 8.0) * (1.0 - 0.7 * person.skin_like(lab, colourful))
        } else {
            0.7 * ramp(darker, 6.0, 18.0)
        }
    };
    for_box(canvas, frame, bounds, |x, y, _, _, lab| {
        let shape = outer.weight(x as f32 + 0.5, y as f32 + 0.5, 0.3);
        let score = lip_score(lab) * shape;
        if score > 0.5 {
            lip_samples.push((lab, score));
        }
    });
    let total: f32 = lip_samples.iter().map(|s| s.1).sum();
    let (lip_l, lip_c) = if total > 0.0 {
        let l = lip_samples.iter().map(|(lab, w)| lab[0] * w).sum::<f32>() / total;
        let c = lip_samples
            .iter()
            .map(|(lab, w)| lab[1].hypot(lab[2]) * w)
            .sum::<f32>()
            / total;
        (l, c)
    } else {
        (skin_l - 8.0, 20.0)
    };

    for_box(canvas, frame, bounds, |x, y, _, _, lab| {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let shape = outer.weight(px, py, 0.3);
        if shape <= 0.0 {
            return;
        }
        let inside = inner.weight(px, py, 0.35);
        let chroma = lab[1].hypot(lab[2]);
        let tooth = if colourful {
            ramp(lab[0] - lip_l, 6.0, 16.0) * ramp(lip_c - chroma, 2.0, 9.0)
        } else {
            ramp(lab[0] - lip_l, 15.0, 30.0)
        } * inside;
        let gap = ramp(lip_l - lab[0], 12.0, 25.0) * inside;
        let lip = lip_score(lab) * shape * (1.0 - tooth) * (1.0 - gap);
        raise(lips, x, y, ramp(lip, 0.25, 0.55));
        raise(teeth, x, y, ramp(tooth, 0.3, 0.6));
        raise(mouth, x, y, ramp(lip.max(tooth).max(gap * 0.9), 0.25, 0.55));
    });
}

/// A beard or a moustache: hair-dark texture in the lower face that is not lip.
///
/// Two things are darker than a cheek in the lower face and only one of them is hair. A
/// shadow under the jaw keeps the skin's own *chroma* and is smooth; a beard changes the chroma
/// and is textured. So the evidence is darkness times either of those - and the zone stops at
/// the chin, because below it the darkest thing is nearly always the shadow.
fn beard_for(canvas: &Canvas, person: &Person, colourful: bool, beard: &mut Plane) {
    let frame = &person.frame;
    let md = frame.mouth_down;
    let skin_l = person.skin_l();
    let sd = person.skin_l_sd();
    let mut sum = 0.0;
    let mut count = 0.0;
    let mut cells: Vec<(i64, i64, f32)> = Vec::new();
    for_box(
        canvas,
        frame,
        (-1.0, 1.0, md * 0.55, md + 0.8),
        |x, y, a, b, lab| {
            let shape = ell(a, b, [0.0, md * 0.95], 0.95, md * 0.5 + 0.55, 0.3)
                * (1.0 - ramp(b, md + 0.6, md + 0.8));
            if shape <= 0.0 {
                return;
            }
            let darker = ramp(skin_l - lab[0], 8.0 + sd, 18.0 + sd);
            let off_chroma = match (&person.model, colourful) {
                (Some(model), true) => 1.0 - (-0.5 * model.chroma_distance2(lab) / 2.2).exp(),
                _ => 0.5,
            };
            let g = (canvas.lab_at(x + 1, y)[0] - canvas.lab_at(x - 1, y)[0])
                .abs()
                .max((canvas.lab_at(x, y + 1)[0] - canvas.lab_at(x, y - 1)[0]).abs());
            let texture = ramp(g, 3.0, 9.0);
            let value = shape * darker * off_chroma.max(texture);
            sum += value;
            count += shape;
            cells.push((x, y, value));
        },
    );
    // Stubble and shadow are not a beard: below an eighth of the zone, nothing is reported.
    if count <= 0.0 || sum / count < 0.12 {
        return;
    }
    let mut local = Patch::over(frame, (-1.0, 1.0, md * 0.55, md + 0.8), canvas);
    for (x, y, value) in cells {
        local.set(x, y, ramp(value, 0.2, 0.5));
    }
    local.with(local.plane.close(1).blur(1)).paste_max(beard);
}

fn neck_for(canvas: &Canvas, person: &Person, colourful: bool, neck: &mut Plane) {
    let frame = &person.frame;
    let md = frame.mouth_down;
    let shape = frame.ellipse(0.0, md + 1.3, 0.75, 0.85);
    for_box(
        canvas,
        frame,
        (-1.0, 1.0, md + 0.3, md + 2.3),
        |x, y, _, _, lab| {
            let s = shape.weight(x as f32 + 0.5, y as f32 + 0.5, 0.3);
            if s <= 0.0 {
                return;
            }
            let like = ramp(person.skin_like(lab, colourful), 0.1, 0.4);
            raise(neck, x, y, s * like);
        },
    );
}

/// A diagonal Gaussian in Lab, with floors so a flat sample is not a single colour.
#[derive(Debug, Clone, Copy)]
struct Blob {
    mean: [f32; 3],
    sd: [f32; 3],
}

impl Blob {
    fn fit(samples: &[[f32; 3]], floor: [f32; 3]) -> Option<Self> {
        if samples.len() < 8 {
            return None;
        }
        let n = samples.len() as f32;
        let mut mean = [0.0_f32; 3];
        for s in samples {
            for (m, v) in mean.iter_mut().zip(s) {
                *m += v / n;
            }
        }
        let mut var = [0.0_f32; 3];
        for s in samples {
            for ((v, x), m) in var.iter_mut().zip(s).zip(mean.iter()) {
                *v += (x - m) * (x - m) / n;
            }
        }
        Some(Self {
            mean,
            sd: [
                var[0].sqrt().max(floor[0]),
                var[1].sqrt().max(floor[1]),
                var[2].sqrt().max(floor[2]),
            ],
        })
    }

    fn distance2(&self, lab: [f32; 3]) -> f32 {
        lab.iter()
            .zip(self.mean.iter().zip(self.sd.iter()))
            .map(|(x, (m, s))| ((x - m) / s).powi(2))
            .sum()
    }
}

/// A handful of colour clusters, for a ground that is not one colour.
fn clusters(samples: &[[f32; 3]], k: usize, floor: [f32; 3]) -> Vec<Blob> {
    if samples.len() < 8 {
        return Vec::new();
    }
    // Deterministic seeds: spread through the samples sorted by lightness.
    let mut sorted: Vec<[f32; 3]> = samples.to_vec();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut centres: Vec<[f32; 3]> = (0..k)
        .filter_map(|i| {
            sorted
                .get((sorted.len() - 1) * (2 * i + 1) / (2 * k))
                .copied()
        })
        .collect();
    for _ in 0..6 {
        let mut sums = vec![[0.0_f32; 3]; centres.len()];
        let mut counts = vec![0.0_f32; centres.len()];
        for s in samples {
            let nearest = centres
                .iter()
                .enumerate()
                .min_by(|a, b| dist2(a.1, s).total_cmp(&dist2(b.1, s)))
                .map_or(0, |(i, _)| i);
            if let (Some(sum), Some(count)) = (sums.get_mut(nearest), counts.get_mut(nearest)) {
                for (t, v) in sum.iter_mut().zip(s) {
                    *t += v;
                }
                *count += 1.0;
            }
        }
        for ((c, sum), count) in centres.iter_mut().zip(sums.iter()).zip(counts.iter()) {
            if *count > 0.0 {
                *c = [sum[0] / count, sum[1] / count, sum[2] / count];
            }
        }
    }
    centres
        .iter()
        .filter_map(|c| {
            let members: Vec<[f32; 3]> = samples
                .iter()
                .copied()
                .filter(|s| {
                    centres
                        .iter()
                        .min_by(|a, b| dist2(a, s).total_cmp(&dist2(b, s)))
                        .is_some_and(|best| dist2(best, c) < 1e-6)
                })
                .collect();
            Blob::fit(&members, floor)
        })
        .collect()
}

fn dist2(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn nearest(blobs: &[Blob], lab: [f32; 3]) -> f32 {
    blobs
        .iter()
        .map(|b| b.distance2(lab))
        .fold(f32::INFINITY, f32::min)
}

/// Hair: a colour model seeded above the forehead and beside the temples, held against a
/// model of the ground beside the head, and kept only where it is connected to the seeds.
#[allow(clippy::too_many_lines)]
fn hair_for(canvas: &Canvas, person: &Person, colourful: bool, hair: &mut Plane) {
    let frame = &person.frame;
    let md = frame.mouth_down;
    let mut seeds = Vec::new();
    let mut seed_points = Vec::new();
    let mut ground = Vec::new();
    let step = (1.0 / frame.unit).max(1.0 / 20.0);
    let sample = |a0: f32,
                  a1: f32,
                  b0: f32,
                  b1: f32,
                  out: &mut Vec<[f32; 3]>,
                  pts: Option<&mut Vec<[f32; 2]>>| {
        let mut points = Vec::new();
        let mut b = b0;
        while b <= b1 {
            let mut a = a0;
            while a <= a1 {
                let p = frame.point(a, b);
                if p[0] >= 0.0
                    && p[1] >= 0.0
                    && p[0] < canvas.width as f32
                    && p[1] < canvas.height as f32
                {
                    let lab = canvas.lab_at(p[0] as i64, p[1] as i64);
                    if person.skin_like(lab, colourful) < 0.25 {
                        out.push(lab);
                        points.push(p);
                    }
                }
                a += step;
            }
            b += step;
        }
        if let Some(pts) = pts {
            pts.extend(points);
        }
    };
    sample(-0.8, 0.8, -1.75, -1.3, &mut seeds, Some(&mut seed_points));
    sample(-1.3, -1.05, -1.0, -0.2, &mut seeds, Some(&mut seed_points));
    sample(1.05, 1.3, -1.0, -0.2, &mut seeds, Some(&mut seed_points));
    sample(-2.9, -2.3, -1.9, 0.2, &mut ground, None);
    sample(2.3, 2.9, -1.9, 0.2, &mut ground, None);
    sample(-1.2, 1.2, -2.8, -2.4, &mut ground, None);

    let Some(model) = Blob::fit(&seeds, [7.0, 3.5, 3.5]) else {
        return;
    };
    let ground_blobs = clusters(&ground, 3, [6.0, 3.0, 3.0]);
    // Hair indistinguishable from the ground beside it is reported at half strength rather
    // than guessed at full.
    let separable = ground_blobs.is_empty() || nearest(&ground_blobs, model.mean) > 2.5;
    let strength = if separable { 1.0 } else { 0.5 };

    let hair_bounds = (-2.3, 2.3, -2.8, md + 3.0);
    let mut local = Patch::over(frame, hair_bounds, canvas);
    for_box(
        canvas,
        frame,
        (-2.1, 2.1, -2.7, md + 3.0),
        |x, y, a, b, lab| {
            let zone = ell(a, b, [0.0, -0.35], 1.8, 1.95, 0.2)
                .max(0.85 * soft_box(a, b, 1.9, -0.2, md + 2.4, 0.3));
            if zone <= 0.0 {
                return;
            }
            let lh = (-0.5 * model.distance2(lab) / 1.5).exp();
            let lg = if ground_blobs.is_empty() {
                0.05
            } else {
                (-0.5 * nearest(&ground_blobs, lab) / 1.5).exp()
            };
            let posterior = lh / (lh + lg + 0.03);
            let not_skin = 1.0 - person.skin_like(lab, colourful);
            local.set(x, y, zone * posterior * not_skin * strength);
        },
    );
    let mut seed_plane = Patch::over(frame, hair_bounds, canvas);
    for p in &seed_points {
        seed_plane.set(p[0] as i64, p[1] as i64, 1.0);
    }
    let kept = local
        .plane
        .map(|v| ramp(v, 0.25, 0.55))
        .keep_seeded(&seed_plane.plane, 0.3)
        .close(2)
        .blur(1);
    local.with(kept).paste_max(hair);
}

/// The body below the head: a clothing model from the chest against a model of the ground
/// beside it, inside a zone that widens from the neck to the shoulders.
fn body_for(canvas: &Canvas, person: &Person, people: &[Person], body: &mut Plane) {
    let frame = &person.frame;
    let md = frame.mouth_down;
    let mut chest = Vec::new();
    let mut chest_points = Vec::new();
    let mut ground = Vec::new();
    let step = (1.0 / frame.unit).max(1.0 / 12.0);
    let inside = |p: [f32; 2]| {
        p[0] >= 0.0 && p[1] >= 0.0 && p[0] < canvas.width as f32 && p[1] < canvas.height as f32
    };
    let mut b = md + 2.2;
    while b <= md + 3.6 {
        let mut a = -1.3;
        while a <= 1.3 {
            let p = frame.point(a, b);
            if inside(p) {
                let lab = canvas.lab_at(p[0] as i64, p[1] as i64);
                if person.skin_like(lab, true) < 0.3 {
                    chest.push(lab);
                    chest_points.push(p);
                }
            }
            a += step;
        }
        b += step;
    }
    // The ground: beside the shoulders, outside every person's head.
    let mut b = md + 0.5;
    while b <= md + 6.0 {
        for a in [-5.2_f32, -4.6, 4.6, 5.2] {
            let p = frame.point(a, b);
            let near_someone = people.iter().any(|other| {
                let (oa, ob) = other.frame.local(p[0], p[1]);
                oa.abs() < 2.4 && ob > -2.0
            });
            if inside(p) && !near_someone {
                ground.push(canvas.lab_at(p[0] as i64, p[1] as i64));
            }
        }
        b += step * 2.0;
    }
    if chest.len() < 12 {
        return;
    }
    // Fabric folds and turns away from the light, so its clusters are wider than a wall's.
    let fg = clusters(&chest, 3, [8.0, 4.0, 4.0]);
    let bg = clusters(&ground, 3, [6.0, 3.0, 3.0]);
    let body_bounds = (-6.0, 6.0, md + 0.6, 20.0);
    let mut local = Patch::over(frame, body_bounds, canvas);
    for_box(
        canvas,
        frame,
        (-6.0, 6.0, md + 0.6, 20.0),
        |x, y, a, b, lab| {
            // Shoulders slope down and outward from the base of the neck, so the top of the torso
            // is a line rather than a level: a collar beside the neck is the person, the wall at
            // the same height a metre further out is not.
            let half = 3.3 + 0.2 * (b - md - 1.8).max(0.0);
            let shoulder = md + 0.85 + 0.22 * a.abs();
            let zone = (ramp(b, shoulder - 0.3, shoulder + 0.3)
                * (1.0 - ramp(a.abs(), half - 0.6, half + 0.6)))
            .max(soft_box(a, b, 1.0, md + 0.6, md + 2.0, 0.3));
            if zone <= 0.0 {
                return;
            }
            let lf = (-0.5 * nearest(&fg, lab) / 2.0).exp();
            let lb = if bg.is_empty() {
                0.05
            } else {
                (-0.5 * nearest(&bg, lab) / 2.0).exp()
            };
            let skin = person.skin_like(lab, true);
            // A pixel neither model explains - a sleeve in shadow, a fold - leans toward the person
            // near the body's axis and toward the ground at the zone's edge, rather than being
            // handed to whichever model was least unlikely.
            let axis = 0.7 * (1.0 - ramp(a.abs(), 1.0, half));
            let posterior = (lf + skin + 0.05 * axis) / (lf + skin + lb + 0.05);
            local.set(x, y, zone * posterior);
        },
    );
    let mut seed = Patch::over(frame, body_bounds, canvas);
    for p in &chest_points {
        seed.set(p[0] as i64, p[1] as i64, 1.0);
    }
    let radius = (frame.unit * 0.08).clamp(1.0, 6.0) as u32;
    let kept = local
        .plane
        .blur(radius)
        .map(|v| ramp(v, 0.35, 0.6))
        .keep_seeded(&seed.plane, 0.3)
        .close(radius + 1);
    local.with(kept).paste_max(body);
}

/// Open sky: bright, smooth, blue or overcast, and connected to the top of the frame.
fn sky(canvas: &Canvas) -> Plane {
    let (w, h) = (canvas.width, canvas.height);
    let mut texture = Plane::zeros(w, h);
    for y in 0..i64::from(h) {
        for x in 0..i64::from(w) {
            let gx = canvas.lab_at(x + 1, y)[0] - canvas.lab_at(x - 1, y)[0];
            let gy = canvas.lab_at(x, y + 1)[0] - canvas.lab_at(x, y - 1)[0];
            texture.set(x, y, gx.hypot(gy));
        }
    }
    let texture = texture.blur(2);
    // An overcast sky is the brightest thing in its frame. A studio backdrop is bright, smooth
    // and grey as well, and the difference is that it is not brighter than nearly everything
    // else in the photograph.
    let mut lights: Vec<f32> = canvas.lab.iter().map(|lab| lab[0]).collect();
    lights.sort_by(f32::total_cmp);
    let p70 = lights.get(lights.len() * 7 / 10).copied().unwrap_or(50.0);
    let overcast_floor = (p70 + 5.0).max(80.0);
    let mut candidate = Plane::zeros(w, h);
    for y in 0..i64::from(h) {
        // Sky is above the subject: the lower part of a frame is never searched.
        let height_w = 1.0 - ramp(y as f32 / h.max(1) as f32, 0.55, 0.8);
        for x in 0..i64::from(w) {
            let lab = canvas.lab_at(x, y);
            let chroma = lab[1].hypot(lab[2]);
            let blue = ramp(-lab[2], 6.0, 14.0) * (1.0 - ramp(lab[1].abs(), 12.0, 20.0));
            let overcast = ramp(lab[0], overcast_floor, overcast_floor + 6.0)
                * (1.0 - ramp(chroma, 6.0, 12.0));
            let smooth = 1.0 - ramp(texture.at(x, y), 2.0, 5.0);
            let bright = ramp(lab[0], 45.0, 60.0);
            candidate.set(x, y, height_w * bright * smooth * blue.max(overcast));
        }
    }
    let mut seed = Plane::zeros(w, h);
    let rows = (h / 50).max(1);
    for y in 0..i64::from(rows) {
        for x in 0..i64::from(w) {
            seed.set(x, y, 1.0);
        }
    }
    let kept = candidate.keep_seeded(&seed, 0.4).close(1);
    if kept.coverage() < 0.005 {
        Plane::zeros(w, h)
    } else {
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{iou, portrait, PortraitSpec, MONK};

    fn parse(spec: &PortraitSpec) -> (crate::fixtures::Portrait, PortraitMap) {
        let p = portrait(spec);
        let canvas = Canvas::from_srgb8(&p.rgb, p.width, p.height, ANALYSIS_EDGE).unwrap();
        let map = analyse(&canvas, &[]);
        (p, map)
    }

    #[test]
    fn a_plain_frame_has_no_person_and_is_all_background() {
        let canvas = Canvas::from_srgb8(&vec![120; 3 * 96 * 64], 96, 64, ANALYSIS_EDGE).unwrap();
        let map = analyse(&canvas, &[]);
        assert!(map.faces.is_empty());
        for region in ALL_REGIONS {
            let coverage = map.plane(region).unwrap().coverage();
            if region == Region::Background {
                assert!((coverage - 1.0).abs() < 1e-6);
            } else {
                assert!(coverage < 1e-6, "{region:?} covers {coverage}");
            }
        }
    }

    #[test]
    fn a_painted_portrait_parses_into_its_features() {
        let (p, map) = parse(&PortraitSpec::default());
        assert_eq!(map.faces.len(), 1, "{:?}", map.faces);
        let face = map.faces[0];
        let iod = (p.eyes[1][0] - p.eyes[0][0]).abs();
        for (found, truth) in [face.left_eye, face.right_eye].iter().zip(p.eyes.iter()) {
            let error = (found[0] - truth[0]).hypot(found[1] - truth[1]);
            assert!(error < 0.15 * iod, "eye off by {error} px of {iod}");
        }
        let mouth_error = (face.mouth[0] - p.mouth[0]).hypot(face.mouth[1] - p.mouth[1]);
        assert!(mouth_error < 0.2 * iod, "mouth off by {mouth_error}");
        let face_iou = iou(map.plane(Region::Face).unwrap(), &p.face);
        assert!(face_iou > 0.7, "face IoU {face_iou}");
        let teeth_iou = iou(map.plane(Region::Teeth).unwrap(), &p.teeth);
        assert!(teeth_iou > 0.45, "teeth IoU {teeth_iou}");
        let lips_iou = iou(map.plane(Region::Lips).unwrap(), &p.lips);
        assert!(lips_iou > 0.35, "lips IoU {lips_iou}");
        let eye_iou = iou(map.plane(Region::Eyes).unwrap(), &p.eye_openings);
        assert!(eye_iou > 0.3, "eye IoU {eye_iou}");
        let hair_iou = iou(map.plane(Region::Hair).unwrap(), &p.hair);
        assert!(hair_iou > 0.4, "hair IoU {hair_iou}");
        // Teeth are never skin, and the background is never the face.
        let skin = map.plane(Region::Skin).unwrap();
        assert!(skin.at(p.mouth[0] as i64, p.mouth[1] as i64) < 0.3);
        let background = map.plane(Region::Background).unwrap();
        assert!(background.at(p.centre[0] as i64, p.centre[1] as i64) < 0.2);
        assert!(background.at(4, 250) > 0.8);
    }

    #[test]
    fn every_monk_tone_is_found_and_parsed_alike() {
        // The fairness regression: the same painted face at all ten published Monk swatches
        // must be found, and its teeth and face regions must agree with the truth about as well
        // at one end of the scale as at the other.
        let mut face_ious = Vec::new();
        for (i, tone) in MONK.iter().enumerate() {
            let (p, map) = parse(&PortraitSpec {
                skin: *tone,
                ..PortraitSpec::default()
            });
            assert_eq!(
                map.faces.len(),
                1,
                "MST {} found {} faces",
                i + 1,
                map.faces.len()
            );
            let f = iou(map.plane(Region::Face).unwrap(), &p.face);
            assert!(f > 0.6, "MST {} face IoU {f}", i + 1);
            face_ious.push(f);
        }
        let best = face_ious.iter().copied().fold(0.0_f32, f32::max);
        let worst = face_ious.iter().copied().fold(1.0_f32, f32::min);
        assert!(best - worst < 0.25, "face IoU spread {worst}..{best}");
    }

    #[test]
    fn a_tilted_head_is_found_with_its_tilt() {
        let (_, map) = parse(&PortraitSpec {
            roll: 0.3,
            ..PortraitSpec::default()
        });
        assert_eq!(map.faces.len(), 1);
        assert!(
            (map.faces[0].roll - 0.3).abs() < 0.12,
            "roll {}",
            map.faces[0].roll
        );
    }

    #[test]
    fn a_drawn_box_is_parsed_like_a_found_face() {
        let p = portrait(&PortraitSpec::default());
        let canvas = Canvas::from_srgb8(&p.rgb, p.width, p.height, ANALYSIS_EDGE).unwrap();
        let hint = FaceHint {
            x: 0.2,
            y: 0.2,
            w: 0.6,
            h: 0.68,
        };
        let map = analyse(&canvas, &[hint]);
        assert_eq!(map.faces.len(), 1);
        assert_eq!(map.faces[0].source, crate::face::FaceSource::Hint);
        assert!(map.plane(Region::Teeth).unwrap().coverage() > 0.001);
    }

    #[test]
    fn the_parse_is_deterministic() {
        let p = portrait(&PortraitSpec::default());
        let canvas = Canvas::from_srgb8(&p.rgb, p.width, p.height, ANALYSIS_EDGE).unwrap();
        assert_eq!(analyse(&canvas, &[]), analyse(&canvas, &[]));
    }
}
