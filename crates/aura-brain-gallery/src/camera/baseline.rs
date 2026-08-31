//! `assets/camera_baselines/<brand>.toml`: what this build knows about a brand when the wedding
//! itself says nothing.
//!
//! Section 8 step 1 gives COL "measure bundled brand baselines in controlled conditions for the top
//! brands and profiles", and section 12's first failure mode - "not enough matched pairs" - is
//! what they exist for. A second shooter who arrived after the ceremony and left before the
//! speeches may never have photographed the same conditions as the lead, and the choice is between
//! doing nothing and applying what is known about the two brands in general.
//!
//! ## A baseline is stated against a neutral reference, never against another brand
//!
//! The obvious shape - one file per ordered pair of brands - is quadratic in the number of brands
//! and would need thirty files for seven manufacturers, of which twenty-eight would be somebody's
//! arithmetic rather than somebody's measurement. So each file states **one brand's departure from
//! a neutral reference rendering**, and [`between`] composes two of them: the transform from a
//! Canon to a Sony is Sony's departure minus Canon's.
//!
//! That has a property worth naming, because it is what makes the fallback trustworthy at all:
//! **the composition of a brand with itself is exactly the identity**, so two bodies of the same
//! make are never corrected toward each other by a baseline. A per-pair table could not guarantee
//! that without seven more rows nobody would check.
//!
//! ## What this build's baselines actually are
//!
//! Fabricated. There is no photographed ColorChecker in this repository, no lab, and no body -
//! phase 02's condition C2 and phase 14's condition C2 are both still open. The numbers in
//! `assets/camera_baselines/` were **chosen to be plausible and are not measurements**, every file
//! says so in its own header, and `Baseline::measured` is false on every one of them. That is
//! condition C2 of `docs/progress/PHASE-26-EXIT.md`, it is a Sev 2 trigger, and **the first
//! measured baseline reopens this phase's criteria whatever phase is in flight** - exactly as the
//! first real camera file reopens phase 02's.
//!
//! The consequence is deliberately conservative rather than hidden: a body matched from a
//! fabricated baseline carries [`CameraCode::BaselineOnly`][b] in its reason set, the per-camera
//! report leads with it, and the neutral baseline - which is the identity - is what an unknown
//! brand gets, because guessing that an unrecognised body behaves like a Canon is how a product
//! ships a correction it cannot defend.
//!
//! [b]: aura_core::contract::camera::CameraCode::BaselineOnly

use std::collections::BTreeMap;
use std::path::Path;

use aura_core::contract::camera::{
    Brand, CameraTransform, FlashState, TransformBound, MAX_CHANNEL_GAIN, MAX_CONTRAST_SHAPE,
    MAX_T_CCT_K, MAX_T_EXPOSURE_EV, MAX_T_SATURATION, MAX_T_TINT, SKIN_LUMA_CAP, SKIN_UV_CAP,
};
use aura_core::AuraError;
use serde::Deserialize;

use super::errors;

/// The seven files as they ship, compiled in.
///
/// Compiled rather than read from disk for the reason phase 23's lens profiles are: a bundled
/// asset that can go missing is a bundled asset that produces a different answer on a machine
/// somebody has tidied. A studio may still override one with [`Library::load_dir`].
const BUNDLED: [(Brand, &str); 8] = [
    (
        Brand::Sony,
        include_str!("../../../../assets/camera_baselines/sony.toml"),
    ),
    (
        Brand::Canon,
        include_str!("../../../../assets/camera_baselines/canon.toml"),
    ),
    (
        Brand::Nikon,
        include_str!("../../../../assets/camera_baselines/nikon.toml"),
    ),
    (
        Brand::Fujifilm,
        include_str!("../../../../assets/camera_baselines/fujifilm.toml"),
    ),
    (
        Brand::Panasonic,
        include_str!("../../../../assets/camera_baselines/panasonic.toml"),
    ),
    (
        Brand::Olympus,
        include_str!("../../../../assets/camera_baselines/olympus.toml"),
    ),
    (
        Brand::Leica,
        include_str!("../../../../assets/camera_baselines/leica.toml"),
    ),
    (
        Brand::Other,
        include_str!("../../../../assets/camera_baselines/neutral.toml"),
    ),
];

/// One brand's departure from the neutral reference, in one flash state.
///
/// The same nine axes a [`CameraTransform`] carries, and deliberately so: a baseline is a transform
/// that happens to have come from a laboratory rather than from this wedding, and giving it its own
/// shape would mean two things to compose instead of one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Departure {
    /// Kelvin away from neutral. Positive is warmer.
    pub d_cct: f32,
    /// Tint away from neutral. Positive is magenta.
    pub d_tint: f32,
    /// Stops away from neutral. Positive is brighter.
    pub d_exposure: f32,
    /// Per-channel linear gain around one.
    pub channel_gain: [f32; 3],
    /// Saturation away from neutral, in the recipe's units.
    pub d_saturation: f32,
    /// Shadow, mid and highlight multipliers around one.
    pub contrast_shape: [f32; 3],
    /// Skin chromaticity away from neutral, in CIE 1976 `u'v'`.
    pub skin_uv: [f32; 2],
    /// Skin luminance away from neutral, `0..1`.
    pub skin_luma: f32,
}

impl Departure {
    /// The departure that says "this brand renders exactly like the reference".
    pub const NEUTRAL: Self = Self {
        d_cct: 0.0,
        d_tint: 0.0,
        d_exposure: 0.0,
        channel_gain: [1.0; 3],
        d_saturation: 0.0,
        contrast_shape: [1.0; 3],
        skin_uv: [0.0; 2],
        skin_luma: 0.0,
    };

    /// The correction that turns a body departing by `self` into one departing by `target`.
    ///
    /// A subtraction on the additive axes and a ratio on the multiplicative ones, which is the one
    /// place the distinction between the two kinds of axis is made. Getting it wrong would be
    /// invisible: subtracting two gains near one produces a number near zero, which reads as "no
    /// correction" while meaning "multiply everything by nothing".
    #[must_use]
    pub fn to(self, target: Self) -> Self {
        let ratio = |a: f32, b: f32| if a.abs() < 1e-6 { 1.0 } else { b / a };
        Self {
            d_cct: target.d_cct - self.d_cct,
            d_tint: target.d_tint - self.d_tint,
            d_exposure: target.d_exposure - self.d_exposure,
            channel_gain: [
                ratio(self.channel_gain[0], target.channel_gain[0]),
                ratio(self.channel_gain[1], target.channel_gain[1]),
                ratio(self.channel_gain[2], target.channel_gain[2]),
            ],
            d_saturation: target.d_saturation - self.d_saturation,
            contrast_shape: [
                ratio(self.contrast_shape[0], target.contrast_shape[0]),
                ratio(self.contrast_shape[1], target.contrast_shape[1]),
                ratio(self.contrast_shape[2], target.contrast_shape[2]),
            ],
            skin_uv: [
                target.skin_uv[0] - self.skin_uv[0],
                target.skin_uv[1] - self.skin_uv[1],
            ],
            skin_luma: target.skin_luma - self.skin_luma,
        }
    }

    /// Every axis clamped to its contract ceiling.
    ///
    /// A composition of two in-bounds departures can leave the bounds - two brands each 500 K from
    /// neutral in opposite directions are 1,000 K apart - so the clamp is here and not only in the
    /// solver. Which axis bit is returned beside it, for the reason set.
    #[must_use]
    pub fn clamped(self) -> (Self, Option<TransformBound>) {
        let mut hit: Option<TransformBound> = None;
        let mut note = |bound: TransformBound, over: bool| {
            if over && hit.is_none() {
                hit = Some(bound);
            }
        };
        let clamp = |value: f32, ceiling: f32| value.clamp(-ceiling, ceiling);
        let clamp_around_one =
            |value: f32, ceiling: f32| (value - 1.0).clamp(-ceiling, ceiling) + 1.0;

        note(TransformBound::Cct, self.d_cct.abs() > MAX_T_CCT_K);
        note(TransformBound::Tint, self.d_tint.abs() > MAX_T_TINT);
        note(
            TransformBound::Exposure,
            self.d_exposure.abs() > MAX_T_EXPOSURE_EV,
        );
        note(
            TransformBound::ChannelGain,
            self.channel_gain
                .iter()
                .any(|g| (g - 1.0).abs() > MAX_CHANNEL_GAIN),
        );
        note(
            TransformBound::Saturation,
            self.d_saturation.abs() > MAX_T_SATURATION,
        );
        note(
            TransformBound::ContrastShape,
            self.contrast_shape
                .iter()
                .any(|c| (c - 1.0).abs() > MAX_CONTRAST_SHAPE),
        );
        let uv_len = (self.skin_uv[0] * self.skin_uv[0] + self.skin_uv[1] * self.skin_uv[1]).sqrt();
        note(
            TransformBound::Skin,
            uv_len > SKIN_UV_CAP || self.skin_luma.abs() > SKIN_LUMA_CAP,
        );

        // No epsilon guard beside this: `SKIN_UV_CAP` is a positive constant, so the test
        // already excludes a zero divisor. The second condition read as a guard and was not one.
        let skin_scale = if uv_len > SKIN_UV_CAP {
            SKIN_UV_CAP / uv_len
        } else {
            1.0
        };
        (
            Self {
                d_cct: clamp(self.d_cct, MAX_T_CCT_K),
                d_tint: clamp(self.d_tint, MAX_T_TINT),
                d_exposure: clamp(self.d_exposure, MAX_T_EXPOSURE_EV),
                channel_gain: [
                    clamp_around_one(self.channel_gain[0], MAX_CHANNEL_GAIN),
                    clamp_around_one(self.channel_gain[1], MAX_CHANNEL_GAIN),
                    clamp_around_one(self.channel_gain[2], MAX_CHANNEL_GAIN),
                ],
                d_saturation: clamp(self.d_saturation, MAX_T_SATURATION),
                contrast_shape: [
                    clamp_around_one(self.contrast_shape[0], MAX_CONTRAST_SHAPE),
                    clamp_around_one(self.contrast_shape[1], MAX_CONTRAST_SHAPE),
                    clamp_around_one(self.contrast_shape[2], MAX_CONTRAST_SHAPE),
                ],
                skin_uv: [self.skin_uv[0] * skin_scale, self.skin_uv[1] * skin_scale],
                skin_luma: clamp(self.skin_luma, SKIN_LUMA_CAP),
            },
            hit,
        )
    }

    /// True when this departure changes nothing.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        const EPS: f32 = 1e-6;
        self.d_cct.abs() < EPS
            && self.d_tint.abs() < EPS
            && self.d_exposure.abs() < EPS
            && self.d_saturation.abs() < EPS
            && self.skin_luma.abs() < EPS
            && self.skin_uv.iter().all(|v| v.abs() < EPS)
            && self.channel_gain.iter().all(|g| (g - 1.0).abs() < EPS)
            && self.contrast_shape.iter().all(|c| (c - 1.0).abs() < EPS)
    }

    /// Write this departure onto a transform's own axes.
    ///
    /// Used by the solver when a body falls back on a baseline, and by the blender at weight zero.
    pub fn write_into(self, transform: &mut CameraTransform) {
        transform.d_cct = self.d_cct;
        transform.d_tint = self.d_tint;
        transform.d_exposure = self.d_exposure;
        transform.channel_gain = self.channel_gain;
        transform.d_saturation = self.d_saturation;
        transform.contrast_shape = self.contrast_shape;
        transform.skin_correction.d_uv = self.skin_uv;
        transform.skin_correction.d_luma = self.skin_luma;
    }
}

/// One brand's two departures, plus the provenance of the file they came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Baseline {
    /// The brand.
    pub brand: Brand,
    /// Which revision of the file this is.
    pub version: u16,
    /// **True only when the numbers came from a photographed target.**
    ///
    /// False on every file in this build. It is a field rather than a comment because the reason
    /// set, the report and the phase gate all have to be able to say so, and a claim that lives in
    /// a header is a claim nothing can check.
    pub measured: bool,
    /// The ambient departure.
    pub ambient: Departure,
    /// The flash departure.
    ///
    /// A separate measurement rather than a scaled copy, because section 6.1's whole argument for
    /// separating the populations is that the difference between brands is not the same under a
    /// strobe as it is under a room.
    pub flash: Departure,
}

impl Baseline {
    /// The baseline that changes nothing, for an unknown manufacturer.
    #[must_use]
    pub const fn neutral(brand: Brand) -> Self {
        Self {
            brand,
            version: 0,
            measured: false,
            ambient: Departure::NEUTRAL,
            flash: Departure::NEUTRAL,
        }
    }

    /// The departure for one flash state.
    #[must_use]
    pub const fn departure(&self, flash: FlashState) -> Departure {
        match flash {
            FlashState::Ambient => self.ambient,
            FlashState::Flash => self.flash,
        }
    }

    /// Parse and validate one file.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5134` when the file will not parse, when its brand slug is unknown, or when a
    /// departure is outside a contract ceiling. The last of those is a *file* being refused rather
    /// than clamped, for the reason the policy loader refuses a widened bound: a baseline that
    /// declares a 2,000 K departure is not a baseline with a typo in it, it is a baseline that
    /// would move every frame from that brand further than the product promises.
    pub fn load(text: &str) -> Result<Self, AuraError> {
        let raw: RawBaseline =
            toml::from_str(text).map_err(|err| errors::baseline_refused(err.to_string()))?;
        let brand = Brand::from_str_or_other(&raw.brand);
        if brand == Brand::Other && raw.brand != Brand::Other.as_str() {
            return Err(errors::baseline_refused(format!(
                "unknown brand slug '{}' in a camera baseline",
                raw.brand
            )));
        }
        let ambient = departure_from(&raw.ambient, &raw.brand, "ambient")?;
        let flash = departure_from(&raw.flash, &raw.brand, "flash")?;
        Ok(Self {
            brand,
            version: raw.version,
            measured: raw.measured.unwrap_or(false),
            ambient,
            flash,
        })
    }
}

/// Every brand this build knows about.
#[derive(Debug, Clone, PartialEq)]
pub struct Library {
    baselines: BTreeMap<Brand, Baseline>,
}

impl Default for Library {
    fn default() -> Self {
        Self::bundled()
    }
}

impl Library {
    /// The eight compiled-in files.
    ///
    /// A file that will not parse is **left out** rather than failing the build's startup: a bad
    /// Panasonic baseline should degrade Panasonic bodies to the neutral transform and must not
    /// stop a wedding from being matched. That is the difference between `AURA-ML-5134` and
    /// `AURA-ML-5133`, and it is why one is a warning and the other halts.
    #[must_use]
    pub fn bundled() -> Self {
        let mut baselines = BTreeMap::new();
        for (brand, text) in BUNDLED {
            match Baseline::load(text) {
                Ok(baseline) => {
                    baselines.insert(brand, baseline);
                }
                Err(err) => {
                    tracing::warn!(
                        brand = brand.as_str(),
                        error = %err.code,
                        "a bundled camera baseline would not load; that brand falls back on neutral"
                    );
                }
            }
        }
        Self { baselines }
    }

    /// A studio's own directory of `<brand>.toml` files, falling back on the bundled set per brand.
    ///
    /// Per brand rather than wholesale, and that is the opposite of the policy table's rule: a
    /// policy file is a coherent set of decisions and a half-replaced one is nobody's, while a
    /// baseline directory is eight independent measurements and a studio that has measured their
    /// own two bodies should not have to fabricate six more.
    ///
    /// # Errors
    ///
    /// Never. A file that will not load leaves the bundled one in place and logs.
    #[must_use]
    pub fn load_dir(dir: &Path) -> Self {
        let mut library = Self::bundled();
        for brand in Brand::ALL {
            let path = dir.join(format!("{}.toml", brand.as_str()));
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            match Baseline::load(&text) {
                Ok(baseline) if baseline.brand == brand => {
                    library.baselines.insert(brand, baseline);
                }
                Ok(baseline) => {
                    tracing::warn!(
                        file = brand.as_str(),
                        declared = baseline.brand.as_str(),
                        "a camera baseline file names a different brand than its own filename; ignored"
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        brand = brand.as_str(),
                        error = %err.code,
                        "a studio camera baseline would not load; the bundled one is used"
                    );
                }
            }
        }
        library
    }

    /// One brand's baseline, or the neutral one.
    ///
    /// Infallible. An unknown manufacturer gets the identity and
    /// [`CameraCode::BaselineUnknownBrand`][u] beside it, rather than the nearest brand's numbers -
    /// guessing that an unrecognised body behaves like a Canon is how a product ships a correction
    /// it cannot defend.
    ///
    /// [u]: aura_core::contract::camera::CameraCode::BaselineUnknownBrand
    #[must_use]
    pub fn get(&self, brand: Brand) -> Baseline {
        self.baselines
            .get(&brand)
            .copied()
            .unwrap_or(Baseline::neutral(brand))
    }

    /// True when this build has a baseline for a brand at all.
    #[must_use]
    pub fn knows(&self, brand: Brand) -> bool {
        brand != Brand::Other && self.baselines.contains_key(&brand)
    }

    /// True when **any** baseline in this library came from a photographed target.
    ///
    /// False in this build, and the phase gate prints it on every run. See the module header.
    #[must_use]
    pub fn any_measured(&self) -> bool {
        self.baselines.values().any(|baseline| baseline.measured)
    }

    /// How many brands are loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.baselines.len()
    }

    /// True when nothing loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.baselines.is_empty()
    }
}

/// The bounded correction that turns one brand into another, in one flash state.
///
/// The whole fallback path of section 6.1 in one function. Composing through the neutral reference
/// rather than through a per-pair table is what makes `between(library, brand, brand, flash)`
/// **exactly** the identity for every brand, which is the property the phase gate asserts: two
/// bodies of one make are never corrected toward each other by a baseline.
#[must_use]
pub fn between(
    library: &Library,
    from: Brand,
    to: Brand,
    flash: FlashState,
) -> (Departure, Option<TransformBound>) {
    let source = library.get(from).departure(flash);
    let target = library.get(to).departure(flash);
    source.to(target).clamped()
}

// ---------------------------------------------------------------------------
// The file's own shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawBaseline {
    brand: String,
    version: u16,
    measured: Option<bool>,
    #[serde(default)]
    ambient: RawDeparture,
    #[serde(default)]
    flash: RawDeparture,
}

#[derive(Debug, Default, Deserialize)]
struct RawDeparture {
    d_cct: Option<f32>,
    d_tint: Option<f32>,
    d_exposure: Option<f32>,
    channel_gain: Option<[f32; 3]>,
    d_saturation: Option<f32>,
    contrast_shape: Option<[f32; 3]>,
    skin_uv: Option<[f32; 2]>,
    skin_luma: Option<f32>,
}

fn departure_from(raw: &RawDeparture, brand: &str, state: &str) -> Result<Departure, AuraError> {
    let departure = Departure {
        d_cct: raw.d_cct.unwrap_or(0.0),
        d_tint: raw.d_tint.unwrap_or(0.0),
        d_exposure: raw.d_exposure.unwrap_or(0.0),
        channel_gain: raw.channel_gain.unwrap_or([1.0; 3]),
        d_saturation: raw.d_saturation.unwrap_or(0.0),
        contrast_shape: raw.contrast_shape.unwrap_or([1.0; 3]),
        skin_uv: raw.skin_uv.unwrap_or([0.0; 2]),
        skin_luma: raw.skin_luma.unwrap_or(0.0),
    };
    // A *declared* departure has to be inside half of each ceiling, because the composition of two
    // of them is what a body actually receives: two brands each at the full ceiling in opposite
    // directions would compose to twice it and be clamped, which would silently turn a measurement
    // into a bound. Half is the largest declaration that can never do that.
    let half = |ceiling: f32| ceiling / 2.0;
    let checks: [(&str, f32, f32); 5] = [
        ("d_cct", departure.d_cct.abs(), half(MAX_T_CCT_K)),
        ("d_tint", departure.d_tint.abs(), half(MAX_T_TINT)),
        (
            "d_exposure",
            departure.d_exposure.abs(),
            half(MAX_T_EXPOSURE_EV),
        ),
        (
            "d_saturation",
            departure.d_saturation.abs(),
            half(MAX_T_SATURATION),
        ),
        ("skin_luma", departure.skin_luma.abs(), half(SKIN_LUMA_CAP)),
    ];
    for (name, value, ceiling) in checks {
        if !value.is_finite() || value > ceiling {
            return Err(errors::baseline_refused(format!(
                "{brand}.{state}.{name} is {value}, outside the half-ceiling {ceiling} a declared \
                 departure may take"
            )));
        }
    }
    for gain in departure.channel_gain {
        if !gain.is_finite() || (gain - 1.0).abs() > half(MAX_CHANNEL_GAIN) {
            return Err(errors::baseline_refused(format!(
                "{brand}.{state}.channel_gain has {gain}, outside 1 +/- {}",
                half(MAX_CHANNEL_GAIN)
            )));
        }
    }
    for shape in departure.contrast_shape {
        if !shape.is_finite() || (shape - 1.0).abs() > half(MAX_CONTRAST_SHAPE) {
            return Err(errors::baseline_refused(format!(
                "{brand}.{state}.contrast_shape has {shape}, outside 1 +/- {}",
                half(MAX_CONTRAST_SHAPE)
            )));
        }
    }
    let uv_len = (departure.skin_uv[0] * departure.skin_uv[0]
        + departure.skin_uv[1] * departure.skin_uv[1])
        .sqrt();
    if !uv_len.is_finite() || uv_len > half(SKIN_UV_CAP) {
        return Err(errors::baseline_refused(format!(
            "{brand}.{state}.skin_uv is {uv_len} from neutral, outside the half-ceiling {}",
            half(SKIN_UV_CAP)
        )));
    }
    Ok(departure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_eight_bundled_baselines_load() {
        let library = Library::bundled();
        assert_eq!(
            library.len(),
            Brand::COUNT,
            "a bundled baseline failed to load"
        );
        for brand in Brand::ALL {
            let baseline = library.get(brand);
            assert_eq!(baseline.brand, brand);
        }
    }

    #[test]
    fn nothing_in_this_build_was_measured() {
        // Condition C2 of the phase 26 exit report, as an assertion rather than a paragraph. When
        // the first real baseline is measured this test is the thing that fails, which is exactly
        // when the exit report needs revisiting.
        let library = Library::bundled();
        assert!(
            !library.any_measured(),
            "a baseline now claims to be measured; reopen PHASE-26-EXIT.md condition C2"
        );
    }

    #[test]
    fn the_neutral_baseline_changes_nothing() {
        let library = Library::bundled();
        for flash in FlashState::ALL {
            assert!(library.get(Brand::Other).departure(flash).is_neutral());
        }
        assert!(!library.knows(Brand::Other));
        assert!(library.knows(Brand::Canon));
    }

    #[test]
    fn a_brand_composed_with_itself_is_exactly_the_identity() {
        // The property that makes composition through a neutral reference the right shape: two
        // bodies of one make are never corrected toward each other by a baseline. A per-pair table
        // could not guarantee this without seven more rows nobody would check.
        let library = Library::bundled();
        for brand in Brand::ALL {
            for flash in FlashState::ALL {
                let (departure, bound) = between(&library, brand, brand, flash);
                assert!(
                    departure.is_neutral(),
                    "{brand} composed with itself moved something under {flash}"
                );
                assert_eq!(bound, None);
            }
        }
    }

    #[test]
    fn composition_is_antisymmetric_on_the_additive_axes() {
        let library = Library::bundled();
        let (there, _) = between(&library, Brand::Canon, Brand::Sony, FlashState::Ambient);
        let (back, _) = between(&library, Brand::Sony, Brand::Canon, FlashState::Ambient);
        assert!((there.d_cct + back.d_cct).abs() < 1e-3);
        assert!((there.d_tint + back.d_tint).abs() < 1e-3);
        // And reciprocal on the multiplicative ones, which is the distinction `Departure::to`
        // exists to keep straight.
        assert!((there.channel_gain[0] * back.channel_gain[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn the_flash_departure_is_its_own_measurement() {
        // Section 6.1's argument for separating the populations, as an assertion: if every brand's
        // flash departure were a copy of its ambient one, keying on the flash state would be
        // ceremony rather than mechanism.
        let library = Library::bundled();
        let differs = Brand::ALL
            .into_iter()
            .filter(|brand| *brand != Brand::Other)
            .any(|brand| {
                let baseline = library.get(brand);
                (baseline.ambient.d_cct - baseline.flash.d_cct).abs() > 1.0
            });
        assert!(
            differs,
            "no brand's flash departure differs from its ambient one"
        );
    }

    #[test]
    fn a_baseline_that_declares_too_much_is_refused_rather_than_clamped() {
        let text = format!(
            "brand = \"canon\"\nversion = 1\n[ambient]\nd_cct = {}\n",
            MAX_T_CCT_K
        );
        let err = Baseline::load(&text).expect_err("a full-ceiling declaration must be refused");
        assert_eq!(err.code, errors::CAMERA_BASELINE_REFUSED);
    }

    #[test]
    fn an_unknown_brand_slug_is_refused() {
        let text = "brand = \"hasselblad\"\nversion = 1\n";
        let err = Baseline::load(text).expect_err("an unknown brand must be refused");
        assert_eq!(err.code, errors::CAMERA_BASELINE_REFUSED);
    }

    #[test]
    fn a_composed_departure_is_always_inside_the_contract_bounds() {
        let library = Library::bundled();
        for from in Brand::ALL {
            for to in Brand::ALL {
                for flash in FlashState::ALL {
                    let (d, _) = between(&library, from, to, flash);
                    assert!(d.d_cct.abs() <= MAX_T_CCT_K + 1e-3);
                    assert!(d.d_tint.abs() <= MAX_T_TINT + 1e-3);
                    assert!(d.d_exposure.abs() <= MAX_T_EXPOSURE_EV + 1e-3);
                    assert!(d.d_saturation.abs() <= MAX_T_SATURATION + 1e-3);
                    assert!(d
                        .channel_gain
                        .iter()
                        .all(|g| (g - 1.0).abs() <= MAX_CHANNEL_GAIN + 1e-4));
                    assert!(d
                        .contrast_shape
                        .iter()
                        .all(|c| (c - 1.0).abs() <= MAX_CONTRAST_SHAPE + 1e-4));
                }
            }
        }
    }
}
