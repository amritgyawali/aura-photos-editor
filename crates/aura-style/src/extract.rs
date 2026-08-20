//! Getting the photographer's own numbers out of a wedding they already delivered.
//!
//! PHASE-17 section 6.1 gives two paths and is emphatic about which one is preferred:
//!
//! > Preferred path: read XMP/Lightroom catalogue parameters directly - exact and free.
//! > Fallback path: fit the Phase 14 recipe to reproduce the final JPEG.
//!
//! Both end here, as an [`Extracted`]: a recipe in AURA's own schema, the source it came from,
//! and - for the fitted path - how much of the final the fit could not explain.
//!
//! ## The XMP path is exact and it is also partial
//!
//! `aura_recipe::xmp::read` understands the twenty-two `crs:` parameters Adobe defines with the
//! meaning we mean, which is every scalar this phase learns from **and** the curve **and** the
//! eight HSL bands. What it does not carry is masks, retouch operations, restoration and
//! provenance - and none of those are things a style profile learns, so nothing is lost here
//! that this phase wanted.
//!
//! What *is* lost is that an XMP describes the settings and not the result. A photographer who
//! finished a frame in Photoshop after Lightroom has an XMP that is honestly incomplete, and
//! the residual check in [`crate::fit`] is what catches it - because the fitted path is run
//! **as a confirmation** on a sample of the XMP pairs rather than skipped entirely. See
//! [`CONFIRM_EVERY`].
//!
//! ## The fitted path recovers twelve parameters and deliberately not thirty-six
//!
//! [`crate::fit::PARAMS`] is twelve: exposure, temperature, tint, the five tone parameters,
//! vibrance, saturation and two curve anchors. The eight HSL bands are **not** fitted, and that
//! is a decision rather than an omission.
//!
//! Twenty-four extra parameters recovered from one photograph is a system with far more
//! freedom than the data constrains: a frame with no green in it says nothing about what the
//! photographer does to green, and a fitter given the freedom to move it will move it to
//! whatever reduces the residual by a thousandth. That is not a style, it is noise wearing a
//! band name, and it would then be averaged into a bucket delta and applied to every frame that
//! *does* have green in it.
//!
//! So HSL evidence comes from XMP pairs only, and [`Extracted::hsl_known`] is what says whether
//! a pair has any. `crate::tree` weights the HSL columns by that flag, so a photographer who
//! kept no catalogue gets a profile with an empty HSL block and a report that says so - rather
//! than a confident one that was invented.

use aura_core::contract::colour::{HslAdjustments, HslBand, HslShift, ToneCurve};
use aura_core::contract::style::{ExtractSource, StyleCode};
use aura_core::AuraResult;
use aura_recipe::{Recipe, HSL_BANDS};

/// One pair in every this many XMP pairs is also fitted, as a check on the XMP.
///
/// Twenty. An XMP describes settings and a final describes a result, and the two diverge when a
/// photograph left the catalogue and came back - a Photoshop round trip, a plug-in, a printed
/// LUT. Fitting every XMP pair would throw away the whole speed advantage of the exact path;
/// fitting none of them would mean the product never finds out that an archive's sidecars do
/// not describe its finals.
///
/// A confirmation whose residual is above [`crate::fit::REJECT_DE00`] does not reject that one
/// pair - it is evidence about the *archive*, and [`XmpAudit`] is what carries it.
pub const CONFIRM_EVERY: usize = 20;

/// Above this fraction of failed confirmations, an archive's XMP is not describing its finals.
///
/// A quarter. Below it, a few frames went through Photoshop and the rest are fine. Above it,
/// something systematic is wrong - the sidecars belong to a different export - and the honest
/// response is to fit the whole archive rather than trust it.
pub const XMP_DISTRUST_ABOVE: f32 = 0.25;

/// The photographer's own numbers, in the recipe's own units.
///
/// The twelve scalars migration 17 stores on a pair, plus the curve and the eight bands. They
/// are **absolute** - what the photographer set - and the delta that reaches a profile is the
/// difference between these and what phases 15 and 16 would have said about the same frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TheirParams {
    /// Exposure in stops.
    pub exposure: f32,
    /// Colour temperature in kelvin.
    pub temperature_k: f32,
    /// Green-magenta tint.
    pub tint: f32,
    /// Contrast, `-100..100`.
    pub contrast: f32,
    /// Highlight recovery.
    pub highlights: f32,
    /// Shadow lift.
    pub shadows: f32,
    /// White point.
    pub whites: f32,
    /// Black point.
    pub blacks: f32,
    /// Vibrance.
    pub vibrance: f32,
    /// Flat saturation.
    pub saturation: f32,
    /// The point curve they used.
    pub curve: ToneCurve,
    /// The eight bands they used.
    pub hsl: HslAdjustments,
}

impl Default for TheirParams {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            temperature_k: 5500.0,
            tint: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            curve: ToneCurve::identity(),
            hsl: HslAdjustments::default(),
        }
    }
}

impl TheirParams {
    /// Read the twelve numbers out of a recipe.
    ///
    /// The recipe's integers become floats here and never come back: a delta is a mean over
    /// hundreds of pairs and rounding it to the recipe's own resolution at every step would
    /// quantise a lean of 0.4 points to zero.
    #[must_use]
    pub fn of_recipe(recipe: &Recipe) -> Self {
        let g = &recipe.global;
        let curve = ToneCurve::new(
            g.curve
                .points
                .iter()
                .map(|point| {
                    aura_core::contract::colour::CurvePoint::new(
                        point.first().copied().unwrap_or(0),
                        point.get(1).copied().unwrap_or(0),
                    )
                })
                .collect(),
        )
        .unwrap_or_else(|_| ToneCurve::identity());

        let mut hsl = HslAdjustments::default();
        for (index, name) in HSL_BANDS.iter().enumerate() {
            let Some(band) = HslBand::ALL.get(index).copied() else {
                continue;
            };
            if let Some(shift) = g.hsl.get(*name) {
                hsl.set(
                    band,
                    HslShift {
                        h: f32::from(shift.h),
                        s: f32::from(shift.s),
                        l: f32::from(shift.l),
                    },
                );
            }
        }

        Self {
            exposure: g.exposure,
            temperature_k: g.temperature as f32,
            tint: f32::from(g.tint),
            contrast: f32::from(g.contrast),
            highlights: f32::from(g.highlights),
            shadows: f32::from(g.shadows),
            whites: f32::from(g.whites),
            blacks: f32::from(g.blacks),
            vibrance: f32::from(g.vibrance),
            saturation: f32::from(g.saturation),
            curve,
            hsl,
        }
    }

    /// Write the twelve numbers into a recipe, leaving everything else alone.
    ///
    /// Used by the fitter to build a candidate and by the A/B comparison to show what the
    /// photographer actually did. It writes **only** the fields this phase learns from, so a
    /// recipe carrying a crop or a mask keeps them.
    #[must_use]
    pub fn into_recipe(&self, base: &Recipe) -> Recipe {
        let mut out = base.clone();
        let g = &mut out.global;
        g.exposure = self.exposure;
        g.temperature = self.temperature_k.clamp(2000.0, 50_000.0) as u32;
        g.tint = clamp_i16(self.tint);
        g.contrast = clamp_i16(self.contrast);
        g.highlights = clamp_i16(self.highlights);
        g.shadows = clamp_i16(self.shadows);
        g.whites = clamp_i16(self.whites);
        g.blacks = clamp_i16(self.blacks);
        g.vibrance = clamp_i16(self.vibrance);
        g.saturation = clamp_i16(self.saturation);
        g.curve = aura_recipe::Curve {
            points: self.curve.recipe_points(),
        };
        for (index, name) in HSL_BANDS.iter().enumerate() {
            let Some(band) = HslBand::ALL.get(index).copied() else {
                continue;
            };
            let shift = self.hsl.get(band);
            g.hsl.insert(
                (*name).to_string(),
                aura_recipe::HslShift {
                    h: clamp_i16(shift.h),
                    s: clamp_i16(shift.s),
                    l: clamp_i16(shift.l),
                },
            );
        }
        out
    }
}

/// What one pair's extraction produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Extracted {
    /// The photographer's own numbers.
    pub params: TheirParams,
    /// Where they came from.
    pub source: ExtractSource,
    /// What the fit could not explain, in dE00. Zero for a pure XMP read.
    pub residual_de00: f32,
    /// True when this pair carries real evidence about the eight HSL bands.
    ///
    /// Only an XMP pair does. See this module's header: fitting twenty-four band parameters
    /// from one photograph recovers noise, and averaging noise into a bucket delta applies it
    /// to every frame that *does* have that colour in it.
    pub hsl_known: bool,
    /// Why this pair cannot be used, when it cannot.
    pub rejection: Option<StyleCode>,
}

impl Extracted {
    /// A pair that could not be used.
    #[must_use]
    pub fn rejected(code: StyleCode, residual: f32) -> Self {
        Self {
            params: TheirParams::default(),
            source: ExtractSource::None,
            residual_de00: residual,
            hsl_known: false,
            rejection: Some(code),
        }
    }

    /// True when this pair may be fitted from.
    #[must_use]
    pub const fn accepted(&self) -> bool {
        self.rejection.is_none()
    }
}

/// Read a photographer's parameters out of an XMP packet.
///
/// `base` is the neutral recipe for this photograph - phase 14's, with the right content hash
/// and camera - because `aura_recipe::xmp::read` returns a *whole* recipe and the packet only
/// carries a subset of it.
///
/// # Errors
///
/// `AURA-RENDER-8005` when the packet is not readable XMP.
pub fn from_xmp(packet: &str, base: &Recipe) -> AuraResult<Extracted> {
    let recipe = aura_recipe::xmp::read(packet, base)?;
    Ok(Extracted {
        params: TheirParams::of_recipe(&recipe),
        source: ExtractSource::Xmp,
        residual_de00: 0.0,
        hsl_known: true,
        rejection: None,
    })
}

/// What the periodic confirmation of an archive's XMP found.
///
/// Kept per archive rather than per pair, because a sidecar that does not describe its final is
/// a property of how that wedding was finished and not of that frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct XmpAudit {
    /// How many XMP pairs were confirmed by a fit.
    pub confirmed: u32,
    /// How many of those had a residual above the rejection threshold.
    pub failed: u32,
    /// The worst residual seen, in dE00.
    pub worst_de00: f32,
}

impl XmpAudit {
    /// Record one confirmation.
    pub fn record(&mut self, residual: f32, threshold: f32) {
        self.confirmed = self.confirmed.saturating_add(1);
        if residual > threshold {
            self.failed = self.failed.saturating_add(1);
        }
        self.worst_de00 = self.worst_de00.max(residual);
    }

    /// True when this archive's sidecars should not be trusted and everything should be fitted.
    #[must_use]
    pub fn distrusted(&self) -> bool {
        if self.confirmed == 0 {
            return false;
        }
        (self.failed as f32 / self.confirmed as f32) > XMP_DISTRUST_ABOVE
    }
}

fn clamp_i16(value: f32) -> i16 {
    value.clamp(-100.0, 100.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::fixtures;

    fn base() -> Recipe {
        fixtures::neutral("abc", "Test Camera")
    }

    #[test]
    fn the_twelve_numbers_round_trip_through_a_recipe() {
        let mut params = TheirParams {
            exposure: 0.35,
            temperature_k: 4200.0,
            tint: -6.0,
            contrast: 18.0,
            highlights: -24.0,
            shadows: 31.0,
            whites: 7.0,
            blacks: -12.0,
            vibrance: 14.0,
            saturation: -3.0,
            ..TheirParams::default()
        };
        params.hsl.set(
            HslBand::Green,
            HslShift {
                h: 9.0,
                s: -11.0,
                l: 2.0,
            },
        );
        let recipe = params.into_recipe(&base());
        let read = TheirParams::of_recipe(&recipe);
        assert!((read.exposure - params.exposure).abs() < 1e-3);
        assert!((read.temperature_k - params.temperature_k).abs() < 1.0);
        assert!((read.contrast - params.contrast).abs() < 1.0);
        assert!((read.hsl.get(HslBand::Green).s - -11.0).abs() < 1.0);
    }

    #[test]
    fn writing_the_style_fields_leaves_the_rest_of_the_recipe_alone() {
        let mut original = base();
        original.geometry.rotate = 1.5;
        original.provenance.style_profile = Some("someone".to_string());
        let written = TheirParams::default().into_recipe(&original);
        assert!((written.geometry.rotate - 1.5).abs() < f32::EPSILON);
        assert_eq!(written.provenance.style_profile.as_deref(), Some("someone"));
    }

    #[test]
    fn a_non_monotone_stored_curve_reads_as_the_identity_rather_than_as_a_refusal() {
        let mut recipe = base();
        recipe.global.curve = aura_recipe::Curve {
            points: vec![[0, 0], [128, 200], [64, 30], [255, 255]],
        };
        let read = TheirParams::of_recipe(&recipe);
        assert!(read.curve.is_identity());
    }

    #[test]
    fn an_xmp_read_carries_hsl_evidence_and_a_zero_residual() {
        let mut params = TheirParams::default();
        params.hsl.set(
            HslBand::Orange,
            HslShift {
                h: 3.0,
                s: 0.0,
                l: 0.0,
            },
        );
        let packet = aura_recipe::xmp::write(&params.into_recipe(&base()));
        let Ok(extracted) = from_xmp(&packet, &base()) else {
            unreachable!("the packet was written by the same module")
        };
        assert_eq!(extracted.source, ExtractSource::Xmp);
        assert!(extracted.hsl_known);
        assert!(extracted.residual_de00.abs() < f32::EPSILON);
    }

    #[test]
    fn an_archive_whose_sidecars_do_not_describe_its_finals_is_distrusted() {
        let mut audit = XmpAudit::default();
        for _ in 0..10 {
            audit.record(6.0, 3.0);
        }
        assert!(audit.distrusted());

        let mut healthy = XmpAudit::default();
        for _ in 0..10 {
            healthy.record(0.4, 3.0);
        }
        assert!(!healthy.distrusted());
    }

    #[test]
    fn an_archive_nobody_confirmed_is_not_distrusted() {
        assert!(!XmpAudit::default().distrusted());
    }
}
