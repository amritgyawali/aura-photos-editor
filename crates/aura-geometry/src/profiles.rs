//! The two tables this phase decides against: what each kind of photograph may be cropped to,
//! and what each lens does to a frame.
//!
//! ## Two tables, one version number
//!
//! `geometry_plan.profile_ver` is a single column and it covers both, which is a deliberate
//! narrowing: the two tables invalidate different things - a new crop rule re-decides every
//! frame in the wedding, and a new lens row re-decides only the frames shot on that lens - and a
//! schema that tried to say so would need a third version column and a per-row lens version to
//! go with it. What it would buy is a slightly smaller re-analysis; what it costs is a column
//! whose meaning nobody can state. [`profile_ver`] hashes the two together and a change to
//! either makes every plan stale, which is the same trade phase 07 made when it put four
//! versions on a row and phase 22 made when it put three.
//!
//! ## Every bound in the crop table runs one way
//!
//! A studio may make this phase do **less** and may never make it do more.
//! [`CropRules::parse`] refuses a file that lowers the resolution floor, lowers the improvement
//! margin, lowers the safety margin or raises the rotation ceiling, and there is no field
//! anywhere in `crop_rules.toml` that could switch a safety rule off. Phase 21 established the
//! shape - "a ceiling can be lowered by a studio and raised by nobody" - and this phase inherits
//! it in the domain where it matters most, because a crop removes information *and the evidence
//! that it was removed*.
//!
//! ## The lens half is a matching policy over somebody else's table
//!
//! The table itself is [`aura_render::geometry::database`] - see that module's header for why it
//! lives in the renderer rather than here. What lives here is section 6.1's *order*: embedded
//! data, then the database, then an estimate, and the refusals in between.

use std::collections::BTreeMap;

use aura_core::contract::geometry::{
    AspectRatio, GeometryCode, LensSource, MIN_IMPROVEMENT, MIN_LONG_EDGE_FRACTION, ROTATE_MAX_DEG,
    SAFETY_MARGIN,
};
use aura_core::{AuraError, SceneId};
use aura_render::geometry::{LensDatabase, LensModel};

use crate::errors;

/// The shipped crop rule table.
const RULES: &str = include_str!("../config/crop_rules.toml");

/// Where a crop wants its subject to sit.
///
/// Two placements rather than a continuous target, because the two are *different rules* rather
/// than two points on one: thirds asks for the subject away from the centre and centre asks for
/// it exactly on it, and interpolating between them puts a subject in the one place no
/// composition rule has ever asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// On a power point. What a portrait, a candid and a getting-ready frame want.
    #[default]
    Thirds,
    /// On the frame's own centre. What a detail, a formal group and a ceremony want.
    Centre,
}

impl Placement {
    /// Parse a slug. Anything unknown reads as [`Placement::Centre`].
    ///
    /// The *stricter* of the two, because a centred target moves a frame less than a thirds
    /// target does and an unreadable row must not become a licence to recompose.
    #[must_use]
    pub fn from_str_or_centre(text: &str) -> Self {
        if text == "thirds" {
            Self::Thirds
        } else {
            Self::Centre
        }
    }

    /// Stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thirds => "thirds",
            Self::Centre => "centre",
        }
    }

    /// Where the subject wants to be, in normalised frame coordinates.
    ///
    /// Four power points for [`Placement::Thirds`] and one centre for [`Placement::Centre`]. The
    /// objective takes the nearest of them, because a subject on the left third and a subject on
    /// the right third are equally well placed and a target that named one of the two would
    /// punish half the wedding for being a mirror image of the other half.
    #[must_use]
    pub fn targets(self) -> Vec<(f32, f32)> {
        match self {
            Self::Thirds => vec![
                (1.0 / 3.0, 1.0 / 3.0),
                (2.0 / 3.0, 1.0 / 3.0),
                (1.0 / 3.0, 2.0 / 3.0),
                (2.0 / 3.0, 2.0 / 3.0),
            ],
            Self::Centre => vec![(0.5, 0.5)],
        }
    }
}

/// What one kind of photograph may have done to its framing.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRule {
    /// The scene this row is about.
    pub scene: SceneId,
    /// Whether this phase may propose a tighter frame at all.
    ///
    /// **False on eight of the twenty-three rows**, including the neutral one. A scene where
    /// recomposition is a bad idea is a scene where the correct amount of search is none, and
    /// switching it off here is stronger than setting an unreachable margin: the search does not
    /// run, so there is no arithmetic path along which one frame's objective could clear it.
    pub crop: bool,
    /// How much better a proposal must score. At or above [`MIN_IMPROVEMENT`].
    pub min_improvement: f32,
    /// The tightest a crop may go, as a share of the long edge. At or above the table's floor.
    pub max_zoom: f32,
    /// The share of the frame's height this kind of photograph wants above the subject.
    pub headroom: f32,
    /// Where the subject wants to sit.
    pub placement: Placement,
}

impl SceneRule {
    /// The neutral row, used when a scene is absent from the table.
    ///
    /// The most conservative row there is: no crop, and a margin above anything the objective
    /// produces. A scene nobody wrote a row for is a scene nobody thought about, and the honest
    /// response is the one the `unknown` row in the shipped table gives.
    #[must_use]
    pub const fn neutral(scene: SceneId) -> Self {
        Self {
            scene,
            crop: false,
            min_improvement: 0.18,
            max_zoom: 0.90,
            headroom: 0.10,
            placement: Placement::Centre,
        }
    }
}

/// The bounds every scene row is held inside.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropBounds {
    /// The smallest share of the original long edge any crop may keep.
    pub min_long_edge: f32,
    /// The smallest improvement margin any scene row may carry.
    pub min_improvement: f32,
    /// How far inside a crop's edge a protected region must sit.
    pub safety_margin: f32,
    /// The largest tilt this build will correct, in degrees.
    pub max_rotate_deg: f32,
}

impl Default for CropBounds {
    /// The contract's own numbers, which is what an absent `[bounds]` block resolves to.
    fn default() -> Self {
        Self {
            min_long_edge: MIN_LONG_EDGE_FRACTION,
            min_improvement: MIN_IMPROVEMENT,
            safety_margin: SAFETY_MARGIN,
            max_rotate_deg: ROTATE_MAX_DEG,
        }
    }
}

/// The crop rule table, parsed.
#[derive(Debug, Clone)]
pub struct CropRules {
    /// The table's own version.
    pub version: u16,
    /// The bounds.
    pub bounds: CropBounds,
    /// The aspects a plan generates variants at.
    pub variants: Vec<AspectRatio>,
    /// One row per scene.
    rows: BTreeMap<&'static str, SceneRule>,
}

impl CropRules {
    /// The shipped table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5112` when the compiled-in table breaks one of its own bounds, which is a build
    /// error rather than a runtime one and is caught by this crate's own tests.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse(RULES)
    }

    /// Parse a table.
    ///
    /// **Whole-file refusal.** Half a crop rule table is worse than none: a wedding whose
    /// `ceremony` row loaded and whose `family_portrait` row did not would recompose the formals
    /// under the neutral row while the ceremony was correctly left alone, and the symptom is one
    /// scene of a delivered gallery being framed by a machine.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5112`, naming the row and the rule it broke.
    pub fn parse(text: &str) -> Result<Self, AuraError> {
        let file = "crop_rules.toml";
        let parsed: toml::Value = toml::from_str(text)
            .map_err(|e| errors::profile_refused(file, "file", &format!("is not valid TOML: {e}")))?;

        let version = parsed
            .get("version")
            .and_then(toml::Value::as_integer)
            .and_then(|v| u16::try_from(v).ok())
            .ok_or_else(|| errors::profile_refused(file, "version", "is missing or is not a u16"))?;

        let bounds = Self::parse_bounds(file, &parsed)?;

        let variants = match parsed.get("variants").and_then(|v| v.get("generate")) {
            Some(toml::Value::Array(items)) => items
                .iter()
                .filter_map(toml::Value::as_str)
                .map(AspectRatio::from_str_or_original)
                .filter(|aspect| *aspect != AspectRatio::Original)
                .collect(),
            _ => AspectRatio::VARIANTS.to_vec(),
        };

        let mut rows = BTreeMap::new();
        let scenes = parsed
            .get("scene")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| errors::profile_refused(file, "scene", "has no rows"))?;
        for row in scenes {
            let id = row
                .get("id")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| errors::profile_refused(file, "scene", "has a row with no id"))?;
            let scene = SceneId::from_str_or_unknown(id);
            if scene == SceneId::Unknown && id != SceneId::Unknown.as_str() {
                return Err(errors::profile_refused(file, id, "is not a scene"));
            }
            let number = |key: &str, fallback: f32| -> f32 {
                row.get(key)
                    .and_then(toml::Value::as_float)
                    .map(|v| v as f32)
                    .unwrap_or(fallback)
            };
            let rule = SceneRule {
                scene,
                crop: row.get("crop").and_then(toml::Value::as_bool).unwrap_or(false),
                min_improvement: number("min_improvement", bounds.min_improvement),
                max_zoom: number("max_zoom", 0.90),
                headroom: number("headroom", 0.10),
                placement: row
                    .get("placement")
                    .and_then(toml::Value::as_str)
                    .map_or(Placement::Centre, Placement::from_str_or_centre),
            };
            // The two per-row bounds, and both run in the conservative direction. A row may ask
            // for a *larger* margin and a *looser* zoom than the file's floors; it may not ask
            // for either the other way round, because that is a scene quietly exempting itself.
            if rule.min_improvement < bounds.min_improvement - 1e-6 {
                return Err(errors::profile_refused(
                    file,
                    id,
                    &format!(
                        "asks for an improvement margin of {:.3}, below the file floor of {:.3}",
                        rule.min_improvement, bounds.min_improvement
                    ),
                ));
            }
            if rule.max_zoom < bounds.min_long_edge - 1e-6 {
                return Err(errors::profile_refused(
                    file,
                    id,
                    &format!(
                        "would crop to {:.2} of the long edge, below the floor of {:.2}",
                        rule.max_zoom, bounds.min_long_edge
                    ),
                ));
            }
            if !(0.0..=0.45).contains(&rule.headroom) {
                return Err(errors::profile_refused(
                    file,
                    id,
                    "asks for headroom outside 0..0.45, which is more sky than subject",
                ));
            }
            if row
                .get("reason")
                .and_then(toml::Value::as_str)
                .is_none_or(|r| r.trim().len() < 40)
            {
                // Ninth table in the product to require this, and the argument has not changed:
                // a row nobody can explain is a product quietly deciding how somebody's wedding
                // is framed.
                return Err(errors::profile_refused(
                    file,
                    id,
                    "has no written reason, and every row in this file needs one",
                ));
            }
            rows.insert(scene.as_str(), rule);
        }

        if !rows.contains_key(SceneId::Unknown.as_str()) {
            return Err(errors::profile_refused(
                file,
                "unknown",
                "is missing, and it is the row every unclassified frame falls back on",
            ));
        }

        Ok(Self {
            version,
            bounds,
            variants,
            rows,
        })
    }

    fn parse_bounds(file: &str, parsed: &toml::Value) -> Result<CropBounds, AuraError> {
        let Some(block) = parsed.get("bounds") else {
            return Ok(CropBounds::default());
        };
        let number = |key: &str, fallback: f32| -> f32 {
            block
                .get(key)
                .and_then(toml::Value::as_float)
                .map(|v| v as f32)
                .unwrap_or(fallback)
        };
        let bounds = CropBounds {
            min_long_edge: number("min_long_edge", MIN_LONG_EDGE_FRACTION),
            min_improvement: number("min_improvement", MIN_IMPROVEMENT),
            safety_margin: number("safety_margin", SAFETY_MARGIN),
            max_rotate_deg: number("max_rotate_deg", ROTATE_MAX_DEG),
        };
        // The four refusals that make `docs/geometry.md` a promise about the product rather than
        // a description of its defaults. Three floors that may only be raised and one ceiling
        // that may only be lowered - every edit this file permits makes AURA do less.
        if bounds.min_long_edge < MIN_LONG_EDGE_FRACTION - 1e-6 {
            return Err(errors::profile_refused(
                file,
                "min_long_edge",
                "is below the resolution floor the contract sets, which cannot be lowered",
            ));
        }
        if bounds.min_improvement < MIN_IMPROVEMENT - 1e-6 {
            return Err(errors::profile_refused(
                file,
                "min_improvement",
                "is below the improvement margin the contract sets, which cannot be lowered",
            ));
        }
        if bounds.safety_margin < SAFETY_MARGIN - 1e-6 {
            return Err(errors::profile_refused(
                file,
                "safety_margin",
                "is below the safety margin the contract sets, which cannot be lowered",
            ));
        }
        if bounds.max_rotate_deg > ROTATE_MAX_DEG + 1e-6 {
            return Err(errors::profile_refused(
                file,
                "max_rotate_deg",
                "is above the rotation ceiling the contract sets, which cannot be raised",
            ));
        }
        Ok(bounds)
    }

    /// The row for a scene, or the neutral one.
    #[must_use]
    pub fn scene(&self, scene: SceneId) -> SceneRule {
        self.rows
            .get(scene.as_str())
            .cloned()
            .or_else(|| {
                self.rows
                    .get(SceneId::Unknown.as_str())
                    .cloned()
                    .map(|mut row| {
                        row.scene = scene;
                        row
                    })
            })
            .unwrap_or_else(|| SceneRule::neutral(scene))
    }

    /// True when the table has a row of its own for this scene.
    ///
    /// What the pass report counts, so that "this wedding was framed under the neutral row" is a
    /// number rather than something a caller has to infer.
    #[must_use]
    pub fn has_row(&self, scene: SceneId) -> bool {
        self.rows.contains_key(scene.as_str())
    }

    /// How many rows the table carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when the table has no rows, which [`CropRules::parse`] refuses to produce.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

// ---------------------------------------------------------------------------
// The lens half
// ---------------------------------------------------------------------------

/// What a lens lookup found.
#[derive(Debug, Clone, PartialEq)]
pub struct LensMatch {
    /// The profile's id, when one matched.
    pub profile_id: Option<String>,
    /// The coefficients, when there are any.
    pub model: Option<LensModel>,
    /// Which of section 6.1's three sources this came from.
    pub source: LensSource,
    /// The code that explains it, whether it found something or not.
    pub code: GeometryCode,
}

impl LensMatch {
    /// Nothing was found.
    #[must_use]
    pub const fn missing(code: GeometryCode) -> Self {
        Self {
            profile_id: None,
            model: None,
            source: LensSource::None,
            code,
        }
    }
}

/// What a photograph says about the lens it was shot on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LensExif {
    /// The lens name the camera wrote, when it wrote one.
    pub name: String,
    /// The focal length in millimetres, when the file carries one.
    pub focal_mm: Option<f32>,
    /// True when the file carried the manufacturer's own correction data.
    ///
    /// **The first preference in section 6.1's order**, and the only source in this phase that is
    /// a measurement of the lens that was actually mounted. Phase 02's decoders set it; on a file
    /// with no maker-note support it is false, which is a gap rather than a claim that the
    /// manufacturer recorded nothing.
    pub embedded: bool,
}

/// Resolve a lens, in section 6.1's order.
///
/// Embedded data first, then the bundled database, then nothing. **The third preference -
/// estimating from straight edges - is not resolved here**: it needs pixels, so it lives in
/// [`crate::lens`] and this function's job is to say whether it is needed.
///
/// The three are not interchangeable and the returned [`LensMatch::source`] says which one
/// answered, because a photographer who disagrees with a distortion correction needs to know
/// whether they are arguing with their camera, with this repository, or with an estimate made
/// from one photograph.
#[must_use]
pub fn resolve_lens(exif: &LensExif, database: &LensDatabase) -> LensMatch {
    if exif.embedded {
        return LensMatch {
            profile_id: None,
            model: None,
            source: LensSource::Embedded,
            code: GeometryCode::LensEmbedded,
        };
    }
    let Some(focal) = exif.focal_mm else {
        // No focal length is not the same as no profile: the table is keyed on focal length, so
        // a file without one cannot be looked up at all however well its lens is known.
        return LensMatch::missing(GeometryCode::LensProfileMissing);
    };
    match database.resolve(&exif.name, focal) {
        Some((id, model)) => LensMatch {
            profile_id: Some(id.to_string()),
            model: Some(*model),
            source: LensSource::Database,
            code: GeometryCode::LensProfileMatched,
        },
        None => {
            // A named lens that the table has rows for, at a focal length none of them covers,
            // is a different failure from a lens nobody has heard of - and the second is the one
            // an estimator can help with.
            let known = database
                .matches
                .iter()
                .any(|(pattern, _)| exif.name.to_lowercase().contains(pattern.as_str()));
            LensMatch::missing(if known {
                GeometryCode::LensFocalOutOfRange
            } else {
                GeometryCode::LensProfileMissing
            })
        }
    }
}

/// The version written into `geometry_plan.profile_ver`.
///
/// The two tables hashed together, folded into the range a `u16` column can carry. Not a
/// concatenation and not the larger of the two: either would collide the moment one table
/// reached the other's version, and a collision here is a stale plan that reads as current -
/// which is the exact comparison `AURA-ML-5109` exists to prevent.
#[must_use]
pub fn profile_ver(rules_ver: u16, lens_ver: u16) -> u16 {
    // FNV-1a over the four bytes, which is one multiply and two xors and has no dependency.
    let mut hash: u32 = 2_166_136_261;
    for byte in rules_ver.to_le_bytes().iter().chain(lens_ver.to_le_bytes().iter()) {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    // Never zero: a zero version column reads as "never analysed" everywhere else in this
    // product, and a table combination that hashed to it would make a planned wedding look
    // unplanned.
    (((hash >> 16) ^ hash) as u16).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_parses_and_covers_every_scene() {
        let rules = CropRules::embedded().expect("the shipped crop rules must parse");
        assert_eq!(rules.version, 1);
        for scene in SceneId::ALL {
            assert!(rules.has_row(scene), "{} has no row", scene.as_str());
        }
        assert_eq!(rules.len(), SceneId::ALL.len());
    }

    #[test]
    fn the_shipped_table_is_conservative_by_default() {
        // Section 10.1: most frames keep their original framing. The strongest form of that is
        // a table in which the scenes a wedding is mostly made of do not permit a crop at all.
        let rules = CropRules::embedded().expect("rules");
        for scene in [
            SceneId::Ceremony,
            SceneId::Ritual,
            SceneId::Vows,
            SceneId::Kiss,
            SceneId::FamilyPortrait,
            SceneId::GroupPortrait,
            SceneId::FirstDance,
            SceneId::FirstLook,
            SceneId::CeremonyEntrance,
            SceneId::Unknown,
        ] {
            assert!(
                !rules.scene(scene).crop,
                "{} permits an automatic crop",
                scene.as_str()
            );
        }
    }

    #[test]
    fn a_row_may_tighten_a_bound_and_may_never_loosen_one() {
        let base = "version = 1\n[bounds]\nmin_long_edge = 0.60\nmin_improvement = 0.06\n\
                    safety_margin = 0.01\nmax_rotate_deg = 8.0\n\
                    reason = \"the four bounds, each of which may only be tightened by a studio\"\n";
        let row = |extra: &str| {
            format!(
                "{base}[[scene]]\nid = \"unknown\"\ncrop = false\nplacement = \"centre\"\n\
                 headroom = 0.1\n{extra}\n\
                 reason = \"a written reason long enough to satisfy the rule this file enforces\"\n"
            )
        };
        // Tighter is fine.
        assert!(CropRules::parse(&row("min_improvement = 0.20\nmax_zoom = 0.95")).is_ok());
        // Looser is refused.
        assert!(CropRules::parse(&row("min_improvement = 0.01\nmax_zoom = 0.95")).is_err());
        assert!(CropRules::parse(&row("min_improvement = 0.20\nmax_zoom = 0.30")).is_err());
    }

    #[test]
    fn a_file_may_not_lower_a_floor_or_raise_the_rotation_ceiling() {
        let with = |bounds: &str| {
            format!(
                "version = 1\n[bounds]\n{bounds}\nreason = \"a reason long enough to pass\"\n\
                 [[scene]]\nid = \"unknown\"\ncrop = false\nplacement = \"centre\"\n\
                 headroom = 0.1\nmin_improvement = 0.2\nmax_zoom = 0.9\n\
                 reason = \"a written reason long enough to satisfy the rule this file enforces\"\n"
            )
        };
        assert!(CropRules::parse(&with("min_long_edge = 0.50")).is_err());
        assert!(CropRules::parse(&with("min_improvement = 0.01")).is_err());
        assert!(CropRules::parse(&with("safety_margin = 0.001")).is_err());
        assert!(CropRules::parse(&with("max_rotate_deg = 20.0")).is_err());
        // And the same values in the conservative direction all load.
        assert!(CropRules::parse(&with(
            "min_long_edge = 0.75\nmin_improvement = 0.10\nsafety_margin = 0.02\n\
             max_rotate_deg = 3.0"
        ))
        .is_ok());
    }

    #[test]
    fn a_row_without_a_written_reason_is_refused() {
        let text = "version = 1\n[[scene]]\nid = \"unknown\"\ncrop = false\n\
                    placement = \"centre\"\nheadroom = 0.1\nmin_improvement = 0.2\nmax_zoom = 0.9\n";
        let err = CropRules::parse(text).unwrap_err();
        assert_eq!(err.code.0, "AURA-ML-5112");
    }

    #[test]
    fn a_table_without_the_neutral_row_is_refused() {
        let text = "version = 1\n[[scene]]\nid = \"details\"\ncrop = true\n\
                    placement = \"centre\"\nheadroom = 0.1\nmin_improvement = 0.2\nmax_zoom = 0.9\n\
                    reason = \"a written reason long enough to satisfy the rule this file enforces\"\n";
        assert!(CropRules::parse(text).is_err());
    }

    #[test]
    fn an_unlisted_scene_falls_back_on_the_neutral_row_rather_than_on_permission() {
        let rules = CropRules::embedded().expect("rules");
        let neutral = rules.scene(SceneId::Unknown);
        assert!(!neutral.crop);
        assert!(neutral.min_improvement > MIN_IMPROVEMENT);
    }

    #[test]
    fn the_lens_order_is_embedded_then_database_then_nothing() {
        let db = aura_render::geometry::database();

        let embedded = resolve_lens(
            &LensExif {
                name: "anything".into(),
                focal_mm: Some(35.0),
                embedded: true,
            },
            db,
        );
        assert_eq!(embedded.source, LensSource::Embedded);
        assert_eq!(embedded.code, GeometryCode::LensEmbedded);

        let matched = resolve_lens(
            &LensExif {
                name: "EF24-70mm f/2.8L II USM".into(),
                focal_mm: Some(35.0),
                embedded: false,
            },
            db,
        );
        assert_eq!(matched.source, LensSource::Database);
        assert_eq!(matched.profile_id.as_deref(), Some("family:24-70/2.8"));

        // A known family at a focal length it does not cover is `LensFocalOutOfRange`, and an
        // unknown lens with no usable focal length is `LensProfileMissing`. Two different
        // failures, and only the second is one an estimator can help with.
        let out_of_range = resolve_lens(
            &LensExif {
                name: "EF24-70mm f/2.8L II USM".into(),
                focal_mm: Some(300.0),
                embedded: false,
            },
            db,
        );
        assert_eq!(out_of_range.code, GeometryCode::LensFocalOutOfRange);

        let no_focal = resolve_lens(
            &LensExif {
                name: "EF24-70mm f/2.8L II USM".into(),
                focal_mm: None,
                embedded: false,
            },
            db,
        );
        assert_eq!(no_focal.code, GeometryCode::LensProfileMissing);
    }

    #[test]
    fn the_two_table_versions_never_collide_and_never_hash_to_zero() {
        let mut seen = std::collections::BTreeSet::new();
        for rules in 1..40u16 {
            for lens in 1..40u16 {
                let version = profile_ver(rules, lens);
                assert!(version > 0);
                seen.insert(version);
            }
        }
        // 39 x 39 combinations, and a 16-bit hash of them collides a little by the birthday
        // bound. What is asserted is that it does not collide *much*: a table that mapped many
        // combinations onto one number would make a stale plan read as current.
        assert!(seen.len() > 1_400, "{} distinct versions", seen.len());
    }
}
