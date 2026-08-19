//! Synthetic archives whose style is known by construction.
//!
//! There are no photographers' archives in this repository, no camera files and no delivered
//! galleries. Section 9's DATA task - "collect consented archives from 5 photographers across
//! traditions" - has not happened and cannot happen here. So every gate in this phase is
//! measured against archives built the only honest way available: **a style is chosen, applied
//! to authored pixels through the real renderer, and then recovered.**
//!
//! That proves the arithmetic - the matcher, the fitter, the regression, the shrinkage, the
//! archive cap, the bundle - and it says nothing about a photograph. It is condition C1 in
//! `docs/progress/PHASE-17-EXIT.md` and it is printed at the end of every gate run rather than
//! left in a header nobody reads.
//!
//! ## What "through the real renderer" buys
//!
//! [`AuthoredArchive`] renders each final by asking `aura_render::CpuEngine` to develop the
//! original with the chosen parameters. So the target the fitter is chasing is a genuine output
//! of the pipeline the fitter is optimising, which means a recovery failure is a **real** defect
//! in the optimiser rather than a mismatch between two implementations of the same maths.
//!
//! It also means the fixtures exercise the parts of the pipeline that are hardest to fit: the
//! highlight-recovery knee, the curve's Fritsch-Carlson interpolation and the output transform's
//! encoding all sit between a parameter and the pixel it is recovered from.
//!
//! ## The three archives, and why they are different from each other
//!
//! * **`airy`** - a light, low-contrast, slightly cool photographer. Every parameter small, which
//!   is the hard case for a fitter: a small true value and a coarse first step.
//! * **`moody`** - dark, contrasty, warm. Large parameters, where the risk is the optimiser
//!   stopping at a local minimum on the wrong side of the curve's knee.
//! * **`erratic`** - the outlier wedding. Two stops up on everything, four hundred frames, one
//!   archive label. It exists for section 10.1's robustness gate and for nothing else.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::style::{LightingBucket, StyleQuery};
use aura_core::{AuraResult, SceneId};
use aura_recipe::Recipe;
use aura_render::contract::render::{OutputSpec, RenderLevel, RenderPurpose};
use aura_render::cpu::Frame;
use aura_render::RenderedData;

use crate::bucket::PairSource;
use crate::extract::TheirParams;
use crate::fit::FIT_LONG_EDGE;
use crate::pairs::{Archive, ArchiveFile};

/// The width every fixture frame is rendered at.
///
/// 96 by 64. Small enough that a hundred fits run inside a unit test and large enough that the
/// dE00 lattice takes several hundred samples - which is what stops a loss from being dominated
/// by one pixel.
pub const WIDTH: u32 = 96;

/// The height every fixture frame is rendered at.
pub const HEIGHT: u32 = 64;

/// The camera every fixture claims to be from.
///
/// A name that resolves to the neutral reference profile in `aura_render::profiles`, so a
/// recovered parameter is not partly a camera matrix. Phase 14's condition C2 - no measured
/// camera profiles in this repository - means every profile here would be the reference one
/// anyway; naming it says so rather than relying on it.
pub const CAMERA: &str = "AURA Reference";

/// One synthetic photographer.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// What to call the profile.
    pub name: &'static str,
    /// The parameters every frame in this archive was edited with.
    pub params: TheirParams,
}

impl Look {
    /// The light, low-contrast, slightly cool photographer.
    ///
    /// Every value small. This is the hard case: the fitter's first step for contrast is eight
    /// points and the true value is six, so a naive sweep overshoots on its first move and has
    /// to halve twice to find it.
    #[must_use]
    pub fn airy() -> Self {
        Self {
            name: "airy",
            params: TheirParams {
                exposure: 0.30,
                temperature_k: 5900.0,
                tint: -3.0,
                contrast: -6.0,
                highlights: -14.0,
                shadows: 18.0,
                whites: 5.0,
                blacks: 6.0,
                vibrance: -4.0,
                saturation: -6.0,
                ..TheirParams::default()
            },
        }
    }

    /// The dark, contrasty, warm photographer.
    #[must_use]
    pub fn moody() -> Self {
        Self {
            name: "moody",
            params: TheirParams {
                exposure: -0.35,
                temperature_k: 4600.0,
                tint: 4.0,
                contrast: 28.0,
                highlights: -30.0,
                shadows: -12.0,
                whites: -8.0,
                blacks: -22.0,
                vibrance: 10.0,
                saturation: -4.0,
                ..TheirParams::default()
            },
        }
    }

    /// The outlier wedding, for section 10.1's robustness gate.
    #[must_use]
    pub fn erratic() -> Self {
        Self {
            name: "erratic",
            params: TheirParams {
                exposure: 2.0,
                temperature_k: 9000.0,
                contrast: 80.0,
                saturation: 60.0,
                ..TheirParams::default()
            },
        }
    }
}

/// An archive whose finals were rendered from its originals by the real engine.
#[derive(Debug)]
pub struct AuthoredArchive {
    originals: BTreeMap<String, (Vec<f32>, u32, u32)>,
    finals: BTreeMap<String, (Vec<u8>, u32, u32)>,
    packets: BTreeMap<String, String>,
    /// The archive, as the matcher sees it.
    pub archive: Archive,
    /// The queries each key implies, so the caller's `context_of` can be a lookup.
    pub queries: BTreeMap<String, StyleQuery>,
}

impl AuthoredArchive {
    /// Build one archive of `frames` photographs, all edited with `look`.
    ///
    /// `with_xmp` writes a sidecar for every frame, which sends the whole archive down the exact
    /// path; without it every pair is fitted, which is the expensive path and the one section
    /// 10.1's first gate is about.
    ///
    /// # Errors
    ///
    /// Whatever the renderer raised while producing a final.
    pub fn build(look: &Look, frames: usize, with_xmp: bool) -> AuraResult<Self> {
        let clock: Arc<dyn Clock> = FixedClock::at(time::OffsetDateTime::UNIX_EPOCH);
        let engine = crate::fit::optimise::engine(clock);
        let output = OutputSpec::default();

        let mut out = Self {
            originals: BTreeMap::new(),
            finals: BTreeMap::new(),
            packets: BTreeMap::new(),
            archive: Archive {
                label: look.name.to_string(),
                ..Archive::default()
            },
            queries: BTreeMap::new(),
        };

        for index in 0..frames {
            let key = format!("{}-{index:04}", look.name);
            let rgb = plate(index);
            let base = aura_recipe::fixtures::neutral(&key, CAMERA);
            let recipe = look.params.into_recipe(&base);
            let frame = Frame::working(rgb.clone(), WIDTH, HEIGHT, CAMERA);
            let rendered = engine.render_frame(
                &frame,
                &recipe,
                RenderLevel::Screen(WIDTH, HEIGHT),
                RenderPurpose::Analysis,
                &output,
            )?;
            let RenderedData::Eight(bytes) = rendered.data else {
                return Err(aura_core::errors::ml::shape_mismatch(
                    "8-bit sRGB",
                    "16-bit output",
                ));
            };

            out.originals.insert(key.clone(), (rgb, WIDTH, HEIGHT));
            out.finals
                .insert(key.clone(), (bytes, rendered.width, rendered.height));
            if with_xmp {
                out.packets
                    .insert(key.clone(), aura_recipe::xmp::write(&recipe));
            }
            out.queries.insert(key.clone(), query_for(index));
            // The matcher keys on the content hash, and the fixtures hand it the key itself -
            // which is what makes `PairSource::original(key)` and the matcher agree without a
            // second lookup table.
            out.archive
                .originals
                .push(ArchiveFile::named(format!("{key}.cr3")).with_hash(key.clone()));
            out.archive
                .finals
                .push(ArchiveFile::named(format!("{key}.jpg")).with_source(key.clone()));
        }
        Ok(out)
    }

    /// The archive behind an `Arc`, which is what [`crate::api::TrainPass`] takes.
    #[must_use]
    pub fn shared(self) -> Arc<dyn PairSource> {
        Arc::new(self)
    }

    /// The query one key implies, for the caller's `context_of`.
    #[must_use]
    pub fn query(&self, key: &str) -> StyleQuery {
        self.queries.get(key).copied().unwrap_or_default()
    }

    /// The base recipe for one key.
    #[must_use]
    pub fn base(key: &str) -> Recipe {
        aura_recipe::fixtures::neutral(key, CAMERA)
    }
}

impl PairSource for AuthoredArchive {
    fn original(&self, key: &str, _long_edge: u32) -> AuraResult<(Vec<f32>, u32, u32)> {
        self.originals.get(key).cloned().ok_or_else(|| {
            crate::errors::pair_failed(key, "the fixture archive has no original with that key")
        })
    }

    fn finished(&self, key: &str, _long_edge: u32) -> AuraResult<(Vec<u8>, u32, u32)> {
        self.finals.get(key).cloned().ok_or_else(|| {
            crate::errors::pair_failed(key, "the fixture archive has no final with that key")
        })
    }

    fn camera(&self, _key: &str) -> String {
        CAMERA.to_string()
    }

    fn xmp(&self, key: &str) -> Option<String> {
        self.packets.get(key).cloned()
    }
}

/// A pair source with nothing in it.
///
/// The desktop shell holds one of these until the archive-import flow exists: `train_profile`
/// is a registered command with a frozen shape, and with no source to read it produces
/// `AURA-ML-5073` - "not enough usable pairs" - which is the honest answer rather than a silent
/// success. It lives in `fixtures` rather than in `api` because it is scaffolding and saying so
/// in the module name is cheaper than saying so in a comment somebody has to find.
///
/// It is not a null object that pretends to work. Every method returns
/// `AURA-ML-5072`, so a caller that reaches one finds out.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptySource;

impl PairSource for EmptySource {
    fn original(&self, key: &str, _long_edge: u32) -> AuraResult<(Vec<f32>, u32, u32)> {
        Err(crate::errors::pair_failed(
            key,
            "no archive is wired to this build's training command yet",
        ))
    }

    fn finished(&self, key: &str, _long_edge: u32) -> AuraResult<(Vec<u8>, u32, u32)> {
        Err(crate::errors::pair_failed(
            key,
            "no archive is wired to this build's training command yet",
        ))
    }

    fn camera(&self, _key: &str) -> String {
        CAMERA.to_string()
    }

    fn xmp(&self, _key: &str) -> Option<String> {
        None
    }
}

/// One authored plate, in linear working space.
///
/// A horizontal luminance ramp crossed with three vertical colour bands, plus a mid-grey patch
/// in the middle. The shape is chosen so that **every parameter the fitter recovers has
/// something to act on**: the ramp gives the tone parameters and the curve a gradient, the
/// colour bands give white balance and saturation one, and the grey patch is what makes a tint
/// error visible at all.
///
/// A flat plate would be reproducible by an infinity of parameter settings, and the fit would
/// converge to whichever one it happened to walk into - which looks like an optimiser bug and is
/// a fixture bug.
#[must_use]
pub fn plate(seed: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
    // Deterministic per frame, and it varies the plate a little so that a hundred frames of one
    // archive are not one frame measured a hundred times - a fit that only works on one plate is
    // a fit nobody has tested.
    let jitter = ((seed % 7) as f32) * 0.01;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let across = x as f32 / (WIDTH - 1).max(1) as f32;
            let down = y as f32 / (HEIGHT - 1).max(1) as f32;
            // The ramp, in linear light, from near black to just under clipping. Not to 1.0:
            // a plate that clips has no headroom for a positive exposure to be recovered from.
            let luma = (0.02 + across * 0.80).clamp(0.0, 0.92);
            let band = (down * 3.0) as u32;
            let (r, g, b) = match band {
                0 => (luma * (1.05 + jitter), luma, luma * 0.92),
                1 => (luma * 0.90, luma * (1.02 + jitter), luma * 0.88),
                _ => (luma * 0.88, luma * 0.95, luma * (1.08 + jitter)),
            };
            let centre = (x as f32 - WIDTH as f32 * 0.5).abs() < 6.0
                && (y as f32 - HEIGHT as f32 * 0.5).abs() < 6.0;
            if centre {
                out.extend_from_slice(&[0.18, 0.18, 0.18]);
            } else {
                out.extend_from_slice(&[r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]);
            }
        }
    }
    out
}

/// The scene and light one fixture frame claims.
///
/// Cycled deterministically over four buckets so an archive of a hundred frames populates four
/// leaves with twenty-five each - which is above `PRIOR_STRENGTH` and therefore enough for each
/// bucket to speak, and is what makes the per-bucket gates measurable at all.
#[must_use]
pub fn query_for(index: usize) -> StyleQuery {
    let (scene, lighting, luma, cct, iso, flash) = match index % 4 {
        0 => (
            SceneId::CouplePortrait,
            LightingBucket::Daylight,
            0.42,
            5600.0,
            200,
            false,
        ),
        1 => (
            SceneId::Ceremony,
            LightingBucket::Tungsten,
            0.24,
            3100.0,
            2000,
            false,
        ),
        2 => (
            SceneId::DanceFloor,
            LightingBucket::Flash,
            0.31,
            5200.0,
            1600,
            true,
        ),
        _ => (
            SceneId::GoldenHour,
            LightingBucket::GoldenHour,
            0.38,
            4100.0,
            400,
            false,
        ),
    };
    StyleQuery {
        project: None,
        scene,
        chapter: aura_core::contract::style::SceneGroup::of_scene(scene).usual_chapter(),
        lighting,
        subject_luma: luma,
        cct_k: cct,
        dynamic_range_ev: 7.5,
        iso,
        mixed_light: false,
        flash,
    }
}

/// The fitting long edge the fixtures use.
///
/// The plates are already smaller than [`FIT_LONG_EDGE`], so nothing is resampled and the fit
/// measures the pipeline rather than the resampler. Named here so a test that changes `WIDTH`
/// finds out.
#[must_use]
pub const fn fit_long_edge() -> u32 {
    FIT_LONG_EDGE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archive_pairs_every_original_with_its_own_final() {
        let Ok(built) = AuthoredArchive::build(&Look::airy(), 8, false) else {
            unreachable!("the fixture renderer must work")
        };
        let report = crate::pairs::match_all(&built.archive);
        assert_eq!(report.pairs.len(), 8);
        assert_eq!(
            report.weakest_method(),
            aura_core::contract::style::MatchMethod::ContentHash
        );
    }

    #[test]
    fn the_finals_are_actually_different_from_the_originals() {
        let Ok(built) = AuthoredArchive::build(&Look::moody(), 2, false) else {
            unreachable!()
        };
        let Ok(neutral) = AuthoredArchive::build(
            &Look {
                name: "neutral",
                params: TheirParams::default(),
            },
            2,
            false,
        ) else {
            unreachable!()
        };
        let Ok((moody, _, _)) = built.finished("moody-0000", WIDTH) else {
            unreachable!()
        };
        let Ok((plain, _, _)) = neutral.finished("neutral-0000", WIDTH) else {
            unreachable!()
        };
        assert_ne!(moody, plain, "a look that changes nothing is not a look");
    }

    #[test]
    fn the_plate_has_a_gradient_for_every_parameter_to_act_on() {
        let pixels = plate(0);
        let first = pixels.first().copied().unwrap_or(0.0);
        let last = pixels
            .get(pixels.len().saturating_sub(3))
            .copied()
            .unwrap_or(0.0);
        assert!(last > first + 0.5, "the plate has no luminance ramp");
        assert!(
            pixels.iter().all(|value| *value <= 1.0),
            "a plate that clips has no headroom to recover an exposure from"
        );
    }

    #[test]
    fn four_buckets_are_populated_evenly() {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for index in 0..100 {
            let query = query_for(index);
            *counts
                .entry(crate::infer::bucket_of(&query).as_key())
                .or_insert(0) += 1;
        }
        assert_eq!(counts.len(), 4);
        for count in counts.values() {
            assert_eq!(*count, 25);
        }
    }

    #[test]
    fn an_xmp_archive_hands_back_a_packet_for_every_frame() {
        let Ok(built) = AuthoredArchive::build(&Look::airy(), 3, true) else {
            unreachable!()
        };
        for index in 0..3 {
            assert!(built.xmp(&format!("airy-{index:04}")).is_some());
        }
    }
}
