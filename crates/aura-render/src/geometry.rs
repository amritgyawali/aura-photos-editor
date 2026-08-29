//! The processor reference for PHASE-23's geometry stages, and the lens table underneath them.
//!
//! `aura-geometry` decides *what* a photograph's optics and framing should be;
//! `aura_core::contract::geometry` carries the decision; this module is what turns the decision
//! into pixels. `geometry.wgsl` is the same arithmetic for a device, and
//! `crates/aura-render/tests/shader_parity.rs` holds the two to the same constants.
//!
//! ## Why the lens table lives here rather than in the deciding crate
//!
//! Every other table in this product lives with the phase that decides from it - phase 09's
//! calibration, phase 12's weights, phase 22's noise models. This one cannot, and the reason is
//! phase 14's frozen recipe: `Lens` carries `profile: Option<String>` and nothing else, so at
//! render time the renderer holds **a name and no numbers**. Either the renderer resolves the
//! name, or a second channel is invented to carry coefficients past a frozen contract.
//!
//! So [`database`] is here, `aura_geometry::profiles` reads it through this module for its
//! matching policy, and there is exactly one parser and one table. Two parsers of one file is two
//! sets of coefficients that agree until somebody edits the file in a way only one of them
//! tolerates - and the symptom would be a decision panel that promises a correction the render
//! does not make.
//!
//! ## The three operators, and what each one may not do
//!
//! **Distortion** ([`correct_distortion`]) is a radial resample. It never invents a pixel and
//! never leaves one undefined: for a barrel model it reads inside the frame by construction, and
//! for a pincushion model [`fill_scale`] shrinks the field just far enough that the corner is
//! still inside the source. A correction that left black wedges would hand phase 24's job to a
//! lens profile.
//!
//! **Lateral chromatic aberration** ([`correct_ca`]) is a per-channel radial scale with green
//! fixed. Green fixed rather than all three moved, because green is where the luminance is: a
//! model that moved all three would resample the sharpest channel in the frame to correct a
//! defect in the other two.
//!
//! **Perspective** ([`perspective`]) is a projective warp with the magnification that hides the
//! corners it opens folded into the same resample. One resample rather than a warp followed by a
//! zoom - phase 14's rule about `crop_rotate`, applied a second time.
//!
//! ## Everything here is linear
//!
//! Invariant 8. There is no encoding anywhere in this module; the three operators are
//! coordinate maps and a bilinear read, and a coordinate map has no opinion about tone.
//!
//! ## What is deliberately absent
//!
//! There is no upscale. [`perspective`] and [`crate::spatial::crop_rotate`] both return a buffer
//! no larger than the region they read, [`fill_scale`] is bounded at or below one, and nothing
//! here takes an output resolution. PHASE-23 section 2.2 puts fill in phase 24 and the contract
//! has no field for a scale; this module has no argument for one.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use rayon::prelude::*;

/// The bundled lens profile table, compiled in.
///
/// `assets/lens_profiles/` rather than `crates/aura-render/config/`, because PHASE-23 section 4
/// puts it there and because it is read by two crates. `ATTRIBUTION.md` beside it is the part
/// that matters: **no row in it was measured**.
const TABLE: &str = include_str!("../../../assets/lens_profiles/profiles.toml");

/// The largest projective coefficient a full-deflection keystone slider asks for.
///
/// A fifth. The slider runs `-100..100` and this is what `100` means before the frame's own
/// aspect ratio scales it, so a full-slider vertical keystone on a 3:2 frame asks for a warp
/// whose near end is magnified by `1 / (1 - 0.2/1.5)` and whose far end is reduced to match.
///
/// It is well past [`aura_core::contract::geometry::MAX_STRETCH`] **on purpose**. The cap is what
/// decides how far a correction may go; this constant decides what the slider's own range means,
/// and a slider whose full deflection was exactly the cap would be a slider with no travel left
/// for a photographer who disagrees with the cap on one frame.
pub const KEYSTONE_MAX_P: f32 = 0.20;

/// The most a distortion correction may shrink the field to keep its corners defined, `0..1`.
///
/// Four fifths. A pincushion model strong enough to need more than this is a model that is
/// wrong: at 0.80 the frame has lost a fifth of its long edge to a lens correction, which is
/// more than [`aura_core::contract::geometry::MIN_LONG_EDGE_FRACTION`] allows a *deliberate crop*
/// to take. [`resolve`] refuses a row that would breach it, so the refusal happens once when the
/// table is loaded rather than silently on one frame in a wedding.
pub const MIN_FILL_SCALE: f32 = 0.80;

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// What one lens does to a photograph, as coefficients.
///
/// Deliberately a plain value with no name in it. The name is the table's key, and a model that
/// carried its own id would make it possible to hold one lens's coefficients under another
/// lens's name and never find out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensModel {
    /// Brown-Conrady `r^2` coefficient. Negative corrects barrel, positive corrects pincushion.
    pub k1: f32,
    /// Brown-Conrady `r^4` coefficient.
    pub k2: f32,
    /// Brown-Conrady `r^6` coefficient.
    pub k3: f32,
    /// Lateral chromatic aberration on red, as a fractional radial scale.
    pub ca_red: f32,
    /// Lateral chromatic aberration on blue, as a fractional radial scale.
    pub ca_blue: f32,
    /// How much of the recipe's `0..=100` vignette correction this lens wants wide open.
    pub vignette: u8,
    /// The focal lengths this row covers, in millimetres, inclusive.
    pub focal_mm: (f32, f32),
    /// True when somebody photographed a target to produce these numbers.
    ///
    /// **False on every row this build ships.** See `assets/lens_profiles/ATTRIBUTION.md`, and
    /// [`LensDatabase::parse`] refuses a row that claims otherwise without naming who measured
    /// it and under what licence.
    pub measured: bool,
}

impl LensModel {
    /// A model that moves no pixel.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            k1: 0.0,
            k2: 0.0,
            k3: 0.0,
            ca_red: 0.0,
            ca_blue: 0.0,
            vignette: 0,
            focal_mm: (0.0, f32::MAX),
            measured: false,
        }
    }

    /// True when neither the distortion nor the chromatic aberration terms move anything.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.k1.abs() < 1e-9
            && self.k2.abs() < 1e-9
            && self.k3.abs() < 1e-9
            && self.ca_red.abs() < 1e-9
            && self.ca_blue.abs() < 1e-9
    }

    /// The distorted radius a corrected radius reads from, both normalised to the corner.
    ///
    /// The whole distortion model, in one line, shared by the estimator in `aura-geometry`, the
    /// operator below and the shader. Monotone in `r` for every row the table accepts, which is
    /// what makes [`fill_scale`]'s bisection valid.
    #[must_use]
    pub fn source_radius(&self, r: f32) -> f32 {
        let r2 = r * r;
        r * (1.0 + self.k1 * r2 + self.k2 * r2 * r2 + self.k3 * r2 * r2 * r2)
    }

    /// True when this row covers a focal length.
    ///
    /// Inclusive at both ends, and a row with a degenerate range covers exactly its own prime.
    #[must_use]
    pub fn covers(&self, focal_mm: f32) -> bool {
        focal_mm >= self.focal_mm.0 - 1e-3 && focal_mm <= self.focal_mm.1 + 1e-3
    }
}

/// The bundled table, parsed.
#[derive(Debug, Clone, Default)]
pub struct LensDatabase {
    /// The table's own version, written into `geometry_plan.profile_ver`.
    pub version: u16,
    /// Every row, by id.
    pub rows: BTreeMap<String, LensModel>,
    /// The EXIF substrings each family row answers to, longest first.
    ///
    /// Longest first so that `24-70mm` is tried before `24-70`: a shorter substring that is a
    /// prefix of a longer one would otherwise win by being earlier in the map.
    pub matches: Vec<(String, String)>,
    /// The class rows, in ascending focal order, for the fallback ladder.
    pub classes: Vec<String>,
}

impl LensDatabase {
    /// Parse a table.
    ///
    /// **Whole-file refusal**, the shape phases 15 to 22 all use: a returned `Err` names the
    /// first row that broke a rule and nothing is loaded. Half a lens table is worse than none,
    /// because a lens whose distortion row parsed and whose chromatic aberration row did not
    /// would be corrected in one respect and not the other, which looks exactly like a lens that
    /// behaves that way.
    ///
    /// # Errors
    ///
    /// A sentence naming the offending row and the rule it broke. The caller wraps it in
    /// `AURA-ML-5112`; this module has no error registry of its own, because a parser that
    /// constructed product errors would make `aura-render` the owner of a code phase 23 owns.
    pub fn parse(text: &str) -> Result<Self, String> {
        let parsed: toml::Value =
            toml::from_str(text).map_err(|e| format!("the lens table is not valid TOML: {e}"))?;
        let version = parsed
            .get("profiles_ver")
            .and_then(toml::Value::as_integer)
            .ok_or_else(|| "the lens table has no profiles_ver".to_string())?;
        let version = u16::try_from(version).map_err(|_| {
            format!("profiles_ver {version} does not fit the column it is written into")
        })?;

        let rows = parsed
            .get("lens")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| "the lens table has no [[lens]] rows".to_string())?;

        let mut out = Self {
            version,
            ..Self::default()
        };
        let mut classes: Vec<(f32, String)> = Vec::new();

        for row in rows {
            let id = row
                .get("id")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| "a [[lens]] row has no id".to_string())?
                .to_string();
            let number = |key: &str| -> f32 {
                row.get(key)
                    .and_then(toml::Value::as_float)
                    .map(|v| v as f32)
                    .or_else(|| {
                        row.get(key)
                            .and_then(toml::Value::as_integer)
                            .map(|v| v as f32)
                    })
                    .unwrap_or(0.0)
            };
            let focal_min = number("focal_min_mm");
            let focal_max = number("focal_max_mm");
            if focal_min <= 0.0 || focal_max < focal_min {
                return Err(format!("{id}: focal_min_mm and focal_max_mm are not a range"));
            }
            let vignette = row
                .get("vignette")
                .and_then(toml::Value::as_integer)
                .unwrap_or(0);
            if !(0..=100).contains(&vignette) {
                return Err(format!("{id}: vignette {vignette} is outside 0..=100"));
            }

            let measured = row
                .get("measured")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false);
            if measured {
                // The one rule in this parser that is about honesty rather than about arithmetic.
                // A row may only call itself measured when it says who measured it and under what
                // licence, so promoting a reference model takes two more fields rather than one
                // edit. `assets/lens_profiles/ATTRIBUTION.md` is the argument.
                let named = |key: &str| {
                    row.get(key)
                        .and_then(toml::Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty())
                };
                if !named("source") || !named("licence") {
                    return Err(format!(
                        "{id}: measured = true needs a source and a licence, because a reference \
                         model that calls itself a measurement is the one thing this table may \
                         not contain"
                    ));
                }
            }

            let model = LensModel {
                k1: number("k1"),
                k2: number("k2"),
                k3: number("k3"),
                ca_red: number("ca_red"),
                ca_blue: number("ca_blue"),
                vignette: u8::try_from(vignette).unwrap_or(0),
                focal_mm: (focal_min, focal_max),
                measured,
            };
            if fill_scale(&model) < MIN_FILL_SCALE {
                return Err(format!(
                    "{id}: correcting this row would cost more than {:.0} % of the frame",
                    (1.0 - MIN_FILL_SCALE) * 100.0
                ));
            }
            if model.ca_red.abs() > 0.01 || model.ca_blue.abs() > 0.01 {
                return Err(format!(
                    "{id}: a lateral chromatic aberration above one per cent of the half-diagonal \
                     is a decentred lens rather than a profile"
                ));
            }

            match row.get("kind").and_then(toml::Value::as_str) {
                Some("class") => classes.push((focal_min, id.clone())),
                Some("family") => {
                    let names = row
                        .get("match")
                        .and_then(toml::Value::as_array)
                        .ok_or_else(|| format!("{id}: a family row has no match list"))?;
                    for name in names {
                        if let Some(text) = name.as_str() {
                            out.matches.push((text.to_lowercase(), id.clone()));
                        }
                    }
                }
                other => {
                    return Err(format!(
                        "{id}: kind is {other:?} rather than class or family"
                    ))
                }
            }
            out.rows.insert(id, model);
        }

        if classes.is_empty() {
            return Err("the lens table has no class rows to fall back on".to_string());
        }
        classes.sort_by(|left, right| left.0.total_cmp(&right.0));
        out.classes = classes.into_iter().map(|(_, id)| id).collect();
        // Longest first. See the field's own note.
        out.matches
            .sort_by(|left, right| right.0.len().cmp(&left.0.len()).then(left.0.cmp(&right.0)));
        Ok(out)
    }

    /// The row a lens name and a focal length resolve to, with the id it was found under.
    ///
    /// Section 6.1's order minus its first preference: embedded data is read from the file by
    /// `aura-geometry` and never reaches this table. What is left is *family, then class*, and
    /// the family is tried first because a row written for a lens beats a row written for
    /// everything of its length.
    ///
    /// `None` when the focal length is outside every row, which is
    /// `GeometryCode::LensFocalOutOfRange` rather than a silent fallback to the nearest class.
    #[must_use]
    pub fn resolve(&self, lens_name: &str, focal_mm: f32) -> Option<(&str, &LensModel)> {
        if !focal_mm.is_finite() || focal_mm <= 0.0 {
            return None;
        }
        let needle = lens_name.to_lowercase();
        if !needle.trim().is_empty() {
            for (pattern, id) in &self.matches {
                if needle.contains(pattern.as_str()) {
                    if let Some(model) = self.rows.get(id) {
                        // A family row that does not cover this focal length is not a licence to
                        // fall back on the class ladder: the lens has been identified and the
                        // table has nothing for it here. Extrapolating a named lens is exactly
                        // the guess-dressed-as-a-measurement the code exists to refuse.
                        return model.covers(focal_mm).then_some((id.as_str(), model));
                    }
                }
            }
        }
        for id in &self.classes {
            if let Some(model) = self.rows.get(id) {
                if model.covers(focal_mm) {
                    return Some((id.as_str(), model));
                }
            }
        }
        None
    }

    /// One row by its id, which is what a stored `profile_id` is.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&LensModel> {
        self.rows.get(id)
    }

    /// True when nothing in this table was measured.
    ///
    /// What the panel's caveat is rendered from, and what the phase gate asserts on this build.
    #[must_use]
    pub fn is_all_reference(&self) -> bool {
        self.rows.values().all(|model| !model.measured)
    }
}

/// The bundled table, parsed once.
///
/// Empty rather than absent when the compiled-in file will not parse, and the caller finds out
/// through `rows.is_empty()`. A panic here would take out every render in the product because
/// somebody put a comma in a TOML file.
#[must_use]
pub fn database() -> &'static LensDatabase {
    static DB: OnceLock<LensDatabase> = OnceLock::new();
    DB.get_or_init(|| LensDatabase::parse(TABLE).unwrap_or_default())
}

/// The raw table text, so a caller can report or re-parse what this build shipped.
#[must_use]
pub const fn table_source() -> &'static str {
    TABLE
}

// ---------------------------------------------------------------------------
// Distortion
// ---------------------------------------------------------------------------

/// How far the field must be shrunk so a corrected corner still reads inside the source, `0..1`.
///
/// One for every barrel model, because a barrel correction reads inward by construction. Below
/// one for a pincushion model, and the value is the `s` that solves `source_radius(s) = 1` - the
/// output corner mapping exactly onto the source corner.
///
/// Bisection rather than a closed form: the polynomial is cubic in `r^2` and its analytic
/// inversion has three branches, only one of which is the physical one. Forty steps of bisection
/// on a monotone function is exact to a float and has one branch.
///
/// # Panics
///
/// Never.
#[must_use]
pub fn fill_scale(model: &LensModel) -> f32 {
    if model.source_radius(1.0) <= 1.0 {
        return 1.0;
    }
    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = f32::midpoint(low, high);
        if model.source_radius(mid) > 1.0 {
            high = mid;
        } else {
            low = mid;
        }
    }
    f32::midpoint(low, high).clamp(0.0, 1.0)
}

/// Correct geometric distortion in place, over an interleaved linear RGB buffer.
///
/// **Whole-frame only.** The radius this model is written against is the *frame's*, and the
/// displacement at a corner is a percent or two of the half-diagonal - fifty pixels on a 45 MP
/// frame - so no fixed tile halo can be right. `crate::tiles::render_streamed` renders whole when
/// either lens resample is scheduled, for the same reason it already does for a rotation.
///
/// The buffer keeps its size: this is a resample, not a crop. What the correction costs in field
/// of view is [`fill_scale`], folded in here, and what a *deliberate* crop costs is phase 23's
/// decision and lives somewhere else entirely.
pub fn correct_distortion(rgb: &mut [f32], width: usize, height: usize, model: &LensModel) {
    if width == 0 || height == 0 || model.is_identity() {
        return;
    }
    let source = rgb.to_vec();
    let scale = fill_scale(model);
    let full_w = width as f32;
    let full_h = height as f32;
    let cx = (full_w - 1.0) / 2.0;
    let cy = (full_h - 1.0) / 2.0;
    let max_r = cx.hypot(cy).max(1e-6);

    rgb.par_chunks_mut(width * 3)
        .enumerate()
        .take(height)
        .for_each(|(y, row)| {
            let dy = y as f32 - cy;
            for x in 0..width {
                let dx = x as f32 - cx;
                let r = (dx * dx + dy * dy).sqrt() / max_r;
                let gain = if r < 1e-6 {
                    scale
                } else {
                    scale * model.source_radius(r * scale) / (r * scale)
                };
                let sx = cx + dx * gain;
                let sy = cy + dy * gain;
                for channel in 0..3 {
                    if let Some(slot) = row.get_mut(x * 3 + channel) {
                        *slot = sample(&source, width, height, sx, sy, channel);
                    }
                }
            }
        });
}

/// Correct lateral chromatic aberration in place.
///
/// Red and blue are scaled radially about the frame's centre and green is left alone. The
/// scales are tiny - a fringe is a pixel or two even when it is obvious - so the sub-pixel read
/// is the whole operation: a nearest-neighbour version of this function would be the identity on
/// every frame it was given, while appearing to run.
pub fn correct_ca(rgb: &mut [f32], width: usize, height: usize, model: &LensModel) {
    if width == 0 || height == 0 || (model.ca_red.abs() < 1e-9 && model.ca_blue.abs() < 1e-9) {
        return;
    }
    let source = rgb.to_vec();
    let cx = (width as f32 - 1.0) / 2.0;
    let cy = (height as f32 - 1.0) / 2.0;
    let gains = [1.0 + model.ca_red, 1.0, 1.0 + model.ca_blue];

    rgb.par_chunks_mut(width * 3)
        .enumerate()
        .take(height)
        .for_each(|(y, row)| {
            let dy = y as f32 - cy;
            for x in 0..width {
                let dx = x as f32 - cx;
                for (channel, gain) in gains.iter().enumerate() {
                    if (gain - 1.0).abs() < 1e-9 {
                        continue;
                    }
                    let sx = cx + dx * gain;
                    let sy = cy + dy * gain;
                    if let Some(slot) = row.get_mut(x * 3 + channel) {
                        *slot = sample(&source, width, height, sx, sy, channel);
                    }
                }
            }
        });
}

// ---------------------------------------------------------------------------
// Perspective
// ---------------------------------------------------------------------------

/// The projective coefficients a keystone pair asks for on a frame of this aspect.
///
/// `(p, q)`, where the source of an output pixel at normalised `(u, v)` in `-1..1` is
/// `(u, v) / (1 + p*v + q*u)`.
///
/// **The frame's own aspect is in here**, which is why the same slider value on a 16:9 frame and
/// a 4:5 frame stretches by different amounts - `aura_core::contract::geometry::MAX_STRETCH`
/// says so in the contract and this is where it becomes true. The vertical coefficient is
/// divided by the aspect and the horizontal multiplied by it, because `p` acts along the axis
/// whose half-extent is being measured in units of the other one.
#[must_use]
pub fn keystone_coefficients(vertical: f32, horizontal: f32, frame_aspect: f32) -> (f32, f32) {
    if !frame_aspect.is_finite() || frame_aspect <= 0.0 {
        return (0.0, 0.0);
    }
    let aspect = frame_aspect.clamp(0.1, 10.0);
    let p = -(vertical.clamp(-100.0, 100.0) / 100.0) * KEYSTONE_MAX_P / aspect;
    let q = -(horizontal.clamp(-100.0, 100.0) / 100.0) * KEYSTONE_MAX_P * aspect;
    (p, q)
}

/// The largest ratio between the two axis scales a keystone pair introduces.
///
/// **One implementation, two crates.** `aura_geometry::keystone` compares this against
/// `aura_core::contract::geometry::MAX_STRETCH` before it agrees to a correction, and
/// [`perspective`] uses the same number as the magnification that hides the corners the warp
/// opens. Those two being the same number is not a coincidence: the minimum zoom that keeps the
/// frame filled is exactly `1 / (1 - |p| - |q|)`, which is also the anisotropy at the point where
/// the warp is strongest.
///
/// `f32::INFINITY` for a degenerate pair, which fails every cap it is compared against.
#[must_use]
pub fn stretch_of(vertical: f32, horizontal: f32, frame_aspect: f32) -> f32 {
    let (p, q) = keystone_coefficients(vertical, horizontal, frame_aspect);
    let extreme = p.abs() + q.abs();
    if extreme >= 1.0 {
        return f32::INFINITY;
    }
    1.0 / (1.0 - extreme)
}

/// Apply a perspective correction, returning a new buffer of the same size.
///
/// The magnification that hides the opened corners is [`stretch_of`] and it is folded into this
/// one resample. A caller that warped and then zoomed would resample twice and lose the detail
/// the second time for nothing, which is the argument `crate::spatial::crop_rotate` makes for
/// doing a rotation and a crop together.
///
/// Returns the buffer unchanged when the pair is degenerate, because a warp nobody can invert is
/// a frame nobody can get back.
#[must_use]
pub fn perspective(
    rgb: &[f32],
    width: usize,
    height: usize,
    vertical: f32,
    horizontal: f32,
) -> Vec<f32> {
    if width == 0 || height == 0 {
        return rgb.to_vec();
    }
    let frame_aspect = width as f32 / height as f32;
    let (p, q) = keystone_coefficients(vertical, horizontal, frame_aspect);
    if p.abs() < 1e-9 && q.abs() < 1e-9 {
        return rgb.to_vec();
    }
    let magnify = stretch_of(vertical, horizontal, frame_aspect);
    if !magnify.is_finite() {
        return rgb.to_vec();
    }

    let cx = (width as f32 - 1.0) / 2.0;
    let cy = (height as f32 - 1.0) / 2.0;
    let mut out = vec![0.0f32; width * height * 3];
    out.par_chunks_mut(width * 3)
        .enumerate()
        .take(height)
        .for_each(|(y, row)| {
            let v = (y as f32 - cy) / cy.max(1e-6);
            for x in 0..width {
                let u = (x as f32 - cx) / cx.max(1e-6);
                let denom = 1.0 + p * v + q * u;
                if denom.abs() < 1e-6 {
                    continue;
                }
                let su = u / (denom * magnify);
                let sv = v / (denom * magnify);
                let sx = cx + su * cx;
                let sy = cy + sv * cy;
                for channel in 0..3 {
                    if let Some(slot) = row.get_mut(x * 3 + channel) {
                        *slot = sample(rgb, width, height, sx, sy, channel);
                    }
                }
            }
        });
    out
}

// ---------------------------------------------------------------------------
// Vignette, conditioned on a profile
// ---------------------------------------------------------------------------

/// The vignette correction a profile asks for, as the recipe's `0..=100`.
///
/// A pass-through with a clamp, and it exists so that the number's origin is a function call
/// rather than a field read: `aura-geometry` decides how much of a profile's correction to
/// apply, and a caller that read `model.vignette` directly would be reading a recommendation as
/// though it were a decision.
#[must_use]
pub fn vignette_amount(model: &LensModel, share: f32) -> u8 {
    let share = share.clamp(0.0, 1.0);
    let amount = f32::from(model.vignette) * share;
    amount.round().clamp(0.0, 100.0) as u8
}

// ---------------------------------------------------------------------------
// The one read
// ---------------------------------------------------------------------------

/// Bilinear read with edge clamping, of one channel.
///
/// Clamped rather than zeroed, and this is the opposite of what `crate::spatial::crop_rotate`
/// does half a pixel outside its own source - deliberately. There, an out-of-range read is a
/// corner the rotation opened and the crop is about to remove, so smearing the edge pixel into
/// it would be a lie about pixels that are supposed to disappear. Here, an out-of-range read is
/// half a pixel of rounding at the frame edge on an operator that keeps the frame's size, and
/// zeroing it is the one-pixel dark rim phase 18 found in `Plane::resize_bilinear`.
fn sample(rgb: &[f32], width: usize, height: usize, x: f32, y: f32, channel: usize) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let max_x = width as f32 - 1.0;
    let max_y = height as f32 - 1.0;
    let cx = x.clamp(0.0, max_x);
    let cy = y.clamp(0.0, max_y);
    let x0 = cx.floor() as usize;
    let y0 = cy.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = cx - x0 as f32;
    let fy = cy - y0 as f32;
    let at = |px: usize, py: usize| -> f32 {
        rgb.get((py * width + px) * 3 + channel)
            .copied()
            .unwrap_or(0.0)
    };
    let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
    let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
    top * (1.0 - fy) + bottom * fy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(width: usize, height: usize, value: f32) -> Vec<f32> {
        vec![value; width * height * 3]
    }

    #[test]
    fn the_bundled_table_parses_and_nothing_in_it_was_measured() {
        let db = database();
        assert!(!db.rows.is_empty(), "the bundled lens table did not parse");
        assert_eq!(db.version, 1);
        assert!(
            db.is_all_reference(),
            "a row in the bundled table calls itself measured"
        );
    }

    #[test]
    fn a_measured_row_without_a_source_is_refused() {
        let text = "profiles_ver = 1\n\
                    [[lens]]\n\
                    id = \"class:x\"\nkind = \"class\"\nmeasured = true\n\
                    focal_min_mm = 10.0\nfocal_max_mm = 20.0\nk1 = 0.0\nvignette = 10\n";
        let err = LensDatabase::parse(text).unwrap_err();
        assert!(err.contains("source and a licence"), "{err}");
    }

    #[test]
    fn a_family_row_beats_the_class_ladder_and_a_focal_outside_it_matches_nothing() {
        let db = database();
        let (id, _) = db.resolve("EF24-70mm f/2.8L II USM", 35.0).unwrap();
        assert_eq!(id, "family:24-70/2.8");
        // The same lens name at a focal length its family row does not cover resolves to
        // nothing rather than sliding onto a class row.
        assert!(db.resolve("EF24-70mm f/2.8L II USM", 120.0).is_none());
        // An unknown lens falls onto the ladder.
        let (id, _) = db.resolve("some unknown lens", 24.0).unwrap();
        assert_eq!(id, "class:wide");
        assert!(db.resolve("some unknown lens", 4000.0).is_none());
    }

    #[test]
    fn a_barrel_model_never_reads_outside_the_frame_and_a_pincushion_one_shrinks_to_fit() {
        let barrel = LensModel {
            k1: -0.04,
            ..LensModel::identity()
        };
        assert!((fill_scale(&barrel) - 1.0).abs() < 1e-6);

        let pincushion = LensModel {
            k1: 0.05,
            ..LensModel::identity()
        };
        let scale = fill_scale(&pincushion);
        assert!(scale < 1.0 && scale > MIN_FILL_SCALE, "{scale}");
        // At the solved scale the output corner reads exactly the source corner.
        assert!((pincushion.source_radius(scale) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn correcting_a_flat_frame_leaves_it_flat() {
        // The regression that catches an out-of-range read being zeroed: every operator here
        // keeps the frame's size, so a constant frame must come back constant to the edge.
        let model = LensModel {
            k1: -0.03,
            k2: 0.01,
            ca_red: 0.0004,
            ca_blue: -0.0005,
            ..LensModel::identity()
        };
        let mut rgb = flat(64, 48, 0.42);
        correct_distortion(&mut rgb, 64, 48, &model);
        correct_ca(&mut rgb, 64, 48, &model);
        for value in &rgb {
            assert!((value - 0.42).abs() < 1e-4, "{value}");
        }
        let warped = perspective(&rgb, 64, 48, 40.0, 0.0);
        for value in &warped {
            assert!((value - 0.42).abs() < 1e-4, "{value}");
        }
    }

    #[test]
    fn an_identity_model_moves_nothing() {
        let mut rgb: Vec<f32> = (0..64 * 48 * 3).map(|i| (i % 97) as f32 / 97.0).collect();
        let before = rgb.clone();
        correct_distortion(&mut rgb, 64, 48, &LensModel::identity());
        correct_ca(&mut rgb, 64, 48, &LensModel::identity());
        assert_eq!(rgb, before);
        assert_eq!(perspective(&rgb, 64, 48, 0.0, 0.0), before);
    }

    #[test]
    fn the_stretch_depends_on_the_frames_own_aspect() {
        // The claim `MAX_STRETCH` makes in the contract, as a test: the same slider on two
        // frame shapes is two different warps.
        let landscape = stretch_of(40.0, 0.0, 16.0 / 9.0);
        let portrait = stretch_of(40.0, 0.0, 0.8);
        assert!(portrait > landscape, "{portrait} !> {landscape}");
        assert!((stretch_of(0.0, 0.0, 1.5) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_magnification_is_exactly_what_keeps_the_frame_filled() {
        // A gradient rather than a flat field: a magnification that was too small would leave
        // the clamped edge repeated in a corner, and a flat field cannot show that.
        let (w, h) = (48usize, 32usize);
        let mut rgb = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let value = (x + y) as f32 / (w + h) as f32;
                for channel in 0..3 {
                    rgb[(y * w + x) * 3 + channel] = value;
                }
            }
        }
        let warped = perspective(&rgb, w, h, 60.0, 0.0);
        // No output pixel is black, which is what an unfilled corner would be.
        for (index, value) in warped.iter().enumerate() {
            assert!(*value > 0.0 || index / 3 % w == 0, "black at {index}");
        }
    }

    #[test]
    fn a_vignette_share_scales_the_profiles_own_recommendation() {
        let model = LensModel {
            vignette: 80,
            ..LensModel::identity()
        };
        assert_eq!(vignette_amount(&model, 1.0), 80);
        assert_eq!(vignette_amount(&model, 0.5), 40);
        assert_eq!(vignette_amount(&model, 0.0), 0);
        assert_eq!(vignette_amount(&model, 9.0), 80);
    }
}
