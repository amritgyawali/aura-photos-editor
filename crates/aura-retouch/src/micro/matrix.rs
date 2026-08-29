//! Which operations this studio permits, on this kind of photograph, and how far.
//!
//! PHASE-21 section 4 asks for `config/micro_retouch.toml` and section 9 gives PM two tasks: own
//! the ethics policy and the default opt-in matrix, and approve the ceilings. This module loads
//! that file, refuses it when it is wrong, and is the only place either question is answered.
//!
//! ## The refusal is whole-file
//!
//! As phases 15 to 20 all do. Half a matrix would clean the ceremony against measured ceilings
//! and the reception against nothing, and that inconsistency is invisible in a delivered gallery.
//! `AURA-ML-5105` is run-blocking.
//!
//! ## A studio may lower a ceiling and may never raise one
//!
//! [`MicroTable::parse`] compares every ceiling against
//! [`aura_core::contract::micro::NaturalnessGuard::problem`], which bounds each against the
//! constant the contract owns. `docs/retouch-ethics.md` section 4 makes that promise to a
//! photographer and this is the half that keeps it: a claim a text file can retract is not a
//! claim.
//!
//! ## The two opt-in operations cannot be switched on here at all
//!
//! [`aura_core::contract::micro::ClothingIssue::is_opt_in_only`] is true for a bra strap and a
//! crease, and a file that sets `default_on = true` for either is **refused** rather than obeyed.
//! A studio switches them on per project through `MicroService::set_matrix`, which is a person
//! making a decision about their own clients rather than a default somebody inherited.

use std::collections::BTreeMap;

use aura_core::contract::error::AuraError;
use aura_core::contract::micro::{ClothingIssue, ColourLocus, MicroOp, NaturalnessGuard};
use aura_core::SceneId;
use serde::Deserialize;

use crate::errors;

/// The file this table is loaded from, for the error messages.
pub const FILE: &str = "crates/aura-retouch/config/micro_retouch.toml";

/// The table compiled into the binary.
const EMBEDDED: &str = include_str!("../../config/micro_retouch.toml");

/// Which of the five operations are permitted, in [`MicroOp::NAMES`] order.
pub type OpSwitches = [bool; 5];

/// Which of the five clothing issues are permitted, in [`ClothingIssue::ALL`] order.
pub type ClothingSwitches = [bool; ClothingIssue::COUNT];

/// One scene: which operations may run on this kind of photograph, and how far.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRow {
    /// The ceiling on what any operation may do here, `0..1`. Zero means nothing runs.
    pub limit: f32,
    /// Which operations run, in [`MicroOp::NAMES`] order.
    pub ops: OpSwitches,
    /// Why. Never empty; the loader refuses a row without one.
    pub reason: String,
}

impl SceneRow {
    /// True when this scene permits one operation, by index.
    #[must_use]
    pub fn allows(&self, index: usize) -> bool {
        self.limit > 0.0 && self.ops.get(index).copied().unwrap_or(false)
    }
}

/// The whole table.
#[derive(Debug, Clone, PartialEq)]
pub struct MicroTable {
    version: u16,
    guard: NaturalnessGuard,
    sclera_locus: ColourLocus,
    defaults: OpSwitches,
    clothing: ClothingSwitches,
    borrowing: bool,
    scenes: BTreeMap<String, SceneRow>,
    neutral: SceneRow,
    unlisted: Vec<String>,
}

impl MicroTable {
    /// The table compiled into this build.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` when the embedded table will not load, which is a build fault rather than
    /// an installation one and is therefore never expected in the field.
    pub fn embedded() -> Result<Self, AuraError> {
        Self::parse(EMBEDDED, FILE)
    }

    /// Parse and validate a table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` naming the key and the rule it broke.
    #[allow(clippy::too_many_lines)]
    pub fn parse(text: &str, file: &str) -> Result<Self, AuraError> {
        let raw: RawTable = toml::from_str(text)
            .map_err(|error| errors::micro_matrix_refused(file, "table", &error.to_string()))?;

        if raw.version == 0 {
            return Err(errors::micro_matrix_refused(
                file,
                "version",
                "must be at least 1: it is written into every stored plan",
            ));
        }

        // --- the ceilings ------------------------------------------------------------------
        let teeth_locus = locus(file, "locus.teeth", &raw.locus.teeth)?;
        let sclera_locus = locus(file, "locus.sclera", &raw.locus.sclera)?;
        let guard = NaturalnessGuard {
            teeth_max_luma: raw.guard.teeth_max_luma,
            teeth_locus,
            sclera_max: raw.guard.sclera_max,
            iris_max: raw.guard.iris_max,
            flyaway_max_area_frac: raw.guard.flyaway_max_area_frac,
            require_confidence: raw.guard.require_confidence,
        };
        // The bound the code owns. A file that tries to widen one is refused whole - see the
        // module header, and `docs/retouch-ethics.md` section 4.
        if let Some(problem) = guard.problem() {
            return Err(errors::micro_matrix_refused(file, "guard", &problem));
        }
        if raw.guard.reason.trim().is_empty() {
            return Err(errors::micro_matrix_refused(
                file,
                "guard",
                "has no written reason, and every ceiling here is a product decision",
            ));
        }

        // --- the opt-in matrix -------------------------------------------------------------
        let mut defaults = [false; 5];
        for (index, name) in MicroOp::NAMES.iter().enumerate() {
            let row = raw.op.get(*name).ok_or_else(|| {
                errors::micro_matrix_refused(
                    file,
                    name,
                    "is missing, and every operation must have a row",
                )
            })?;
            if row.reason.trim().is_empty() {
                return Err(errors::micro_matrix_refused(
                    file,
                    name,
                    "has no written reason, and every default here is a product decision",
                ));
            }
            if let Some(slot) = defaults.get_mut(index) {
                *slot = row.default_on;
            }
        }

        let mut clothing = [false; ClothingIssue::COUNT];
        for (index, issue) in ClothingIssue::ALL.iter().enumerate() {
            let key = issue.as_str();
            let row = raw.clothing.get(key).ok_or_else(|| {
                errors::micro_matrix_refused(
                    file,
                    key,
                    "is missing, and every clothing issue must have a row",
                )
            })?;
            if row.reason.trim().is_empty() {
                return Err(errors::micro_matrix_refused(
                    file,
                    key,
                    "has no written reason",
                ));
            }
            // Refused rather than obeyed. See the module header: a bra strap is a garment
            // somebody chose to wear and a crease is what fabric does, and neither may arrive
            // switched on from a file a studio never read.
            if issue.is_opt_in_only() && row.default_on {
                return Err(errors::micro_matrix_refused(
                    file,
                    key,
                    "may not default to on: it is opt-in only, and a studio switches it on per \
                     project rather than inheriting it",
                ));
            }
            if let Some(slot) = clothing.get_mut(index) {
                *slot = row.default_on;
            }
        }

        if raw.borrow.reason.trim().is_empty() {
            return Err(errors::micro_matrix_refused(
                file,
                "borrow",
                "has no written reason, and this is the row that decides whether this product \
                 delivers composited pixels",
            ));
        }

        // --- the scenes ---------------------------------------------------------------------
        let neutral = scene_row(file, "neutral", &raw.neutral)?;
        let mut scenes = BTreeMap::new();
        let mut unlisted = Vec::new();
        for scene in SceneId::ALL {
            let key = scene.as_str();
            match raw.scene.get(key) {
                Some(row) => {
                    scenes.insert(key.to_string(), scene_row(file, key, row)?);
                }
                None => unlisted.push(key.to_string()),
            }
        }

        Ok(Self {
            version: raw.version,
            guard,
            sclera_locus,
            defaults,
            clothing,
            borrowing: raw.borrow.default_on,
            scenes,
            neutral,
            unlisted,
        })
    }

    /// The version stamped into every plan.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// The ceilings.
    #[must_use]
    pub const fn guard(&self) -> NaturalnessGuard {
        self.guard
    }

    /// The region plausible sclera chromaticities sit in, relative to the frame's neutral.
    #[must_use]
    pub const fn sclera_locus(&self) -> ColourLocus {
        self.sclera_locus
    }

    /// The studio defaults for the five operations.
    #[must_use]
    pub const fn defaults(&self) -> OpSwitches {
        self.defaults
    }

    /// The studio defaults for the five clothing issues.
    #[must_use]
    pub const fn clothing_defaults(&self) -> ClothingSwitches {
        self.clothing
    }

    /// Whether cross-frame borrowing is on by default.
    #[must_use]
    pub const fn borrowing_default(&self) -> bool {
        self.borrowing
    }

    /// The three studio defaults together, in the shape the store and the pass both want.
    ///
    /// One call rather than three, because the three are always read together and a caller that
    /// took two of them from this table and one from somewhere else would produce a project whose
    /// matrix half came from the file and half from a default.
    #[must_use]
    pub const fn defaults_triple(&self) -> (OpSwitches, ClothingSwitches, bool) {
        (self.defaults, self.clothing, self.borrowing)
    }

    /// The row for a scene, and whether it was found.
    ///
    /// A scene with no row gets the neutral row and the caller records
    /// `MicroCode::SceneLimited` - the shape phases 15 to 20 all use, because a threshold that
    /// silently defaults is a threshold nobody notices is missing.
    #[must_use]
    pub fn scene(&self, scene: SceneId) -> (&SceneRow, bool) {
        match self.scenes.get(scene.as_str()) {
            Some(row) => (row, true),
            None => (&self.neutral, false),
        }
    }

    /// The scenes this table has no row for.
    #[must_use]
    pub fn unlisted(&self) -> Vec<String> {
        self.unlisted.clone()
    }
}

/// Turn a raw scene table into a checked row.
fn scene_row(file: &str, key: &str, raw: &RawScene) -> Result<SceneRow, AuraError> {
    if !(0.0..=1.0).contains(&raw.limit) {
        return Err(errors::micro_matrix_refused(
            file,
            key,
            "sets a limit outside 0..1",
        ));
    }
    if raw.reason.trim().is_empty() {
        return Err(errors::micro_matrix_refused(
            file,
            key,
            "has no written reason, and every scene threshold here is a product decision",
        ));
    }
    Ok(SceneRow {
        limit: raw.limit,
        ops: [raw.flyaway, raw.teeth, raw.eyes, raw.clothing, raw.glare],
        reason: raw.reason.clone(),
    })
}

/// Turn a raw locus into a checked one.
fn locus(file: &str, key: &str, raw: &RawLocus) -> Result<ColourLocus, AuraError> {
    let value = ColourLocus {
        du: raw.du,
        dv: raw.dv,
        radius: raw.radius,
    };
    if let Some(problem) = value.problem() {
        return Err(errors::micro_matrix_refused(file, key, &problem));
    }
    if raw.reason.trim().is_empty() {
        return Err(errors::micro_matrix_refused(
            file,
            key,
            "has no written reason, and a colour locus nobody can explain is a product deciding \
             what somebody should look like",
        ));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// The file's own shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawTable {
    version: u16,
    guard: RawGuard,
    locus: RawLoci,
    op: BTreeMap<String, RawSwitch>,
    clothing: BTreeMap<String, RawSwitch>,
    borrow: RawSwitch,
    neutral: RawScene,
    scene: BTreeMap<String, RawScene>,
}

#[derive(Debug, Deserialize)]
struct RawGuard {
    teeth_max_luma: f32,
    sclera_max: f32,
    iris_max: f32,
    flyaway_max_area_frac: f32,
    require_confidence: f32,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawLoci {
    teeth: RawLocus,
    sclera: RawLocus,
}

#[derive(Debug, Deserialize)]
struct RawLocus {
    du: f32,
    dv: f32,
    radius: f32,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawSwitch {
    default_on: bool,
    reason: String,
}

/// Five switches and a limit, as the file spells one scene row.
///
/// `struct_excessive_bools` is allowed here rather than worked around: this *is* a switch table,
/// the five names are the five operators, and packing them into a bitfield or a sub-struct would
/// make the TOML harder to read for a product manager, which is the only audience the file has.
#[derive(Debug, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
struct RawScene {
    limit: f32,
    flyaway: bool,
    teeth: bool,
    eyes: bool,
    clothing: bool,
    glare: bool,
    reason: String,
}

#[cfg(test)]
impl MicroTable {
    /// The embedded text, for the tests that mutate one key and re-parse.
    fn embedded_text() -> String {
        EMBEDDED.to_string()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use aura_core::contract::micro::{MAX_SCLERA, MAX_TEETH_LUMA_EV};

    #[test]
    fn the_embedded_table_loads_and_every_scene_has_a_row() {
        let table = MicroTable::embedded().expect("the embedded table");
        assert!(table.version() >= 1);
        assert!(
            table.unlisted().is_empty(),
            "scenes with no row: {:?}",
            table.unlisted()
        );
        for scene in SceneId::ALL {
            let (row, found) = table.scene(scene);
            assert!(found, "{scene} has no row");
            assert!(!row.reason.trim().is_empty());
        }
    }

    #[test]
    fn the_two_scenes_with_none_of_these_regions_in_them_get_nothing() {
        let table = MicroTable::embedded().expect("the embedded table");
        for scene in [SceneId::Details, SceneId::Venue] {
            let (row, _) = table.scene(scene);
            assert_eq!(
                row.limit, 0.0,
                "{scene} would run an operation on a photograph with no person in it"
            );
        }
    }

    #[test]
    fn a_ritual_never_cleans_clothing() {
        // The row that would cause a cultural failure if it were set by convenience. A lint
        // detector cannot tell haldi, sindoor or a ceremonial mark from a stain.
        let table = MicroTable::embedded().expect("the embedded table");
        let (row, found) = table.scene(SceneId::Ritual);
        assert!(found);
        assert!(
            !row.allows(3),
            "the ritual row cleans clothing, and a ceremonial mark is not a stain"
        );
    }

    #[test]
    fn a_table_that_raises_a_ceiling_is_refused_whole() {
        let text = MicroTable::embedded_text().replace(
            "teeth_max_luma        = 0.20",
            "teeth_max_luma        = 0.40",
        );
        let error = MicroTable::parse(&text, "test").expect_err("refused");
        assert_eq!(error.code.0, "AURA-ML-5105");
        assert!(error.detail.contains("teeth_max_luma"), "{}", error.detail);
        // And the ceiling it was compared against is the contract's, not the file's. Read
        // through a binding so the compiler compares a value rather than folding the assertion
        // away entirely.
        let contract_ceiling = MAX_TEETH_LUMA_EV;
        assert!(contract_ceiling < 0.40);
    }

    #[test]
    fn a_table_that_switches_on_an_opt_in_operation_is_refused() {
        let text = MicroTable::embedded_text().replace(
            "[clothing.strap]\ndefault_on = false",
            "[clothing.strap]\ndefault_on = true",
        );
        let error = MicroTable::parse(&text, "test").expect_err("refused");
        assert_eq!(error.code.0, "AURA-ML-5105");
        assert!(error.detail.contains("strap"), "{}", error.detail);
    }

    #[test]
    fn a_row_with_no_written_reason_is_refused() {
        let text = blank_first_reason_after(&MicroTable::embedded_text(), "[op.teeth]");
        let error = MicroTable::parse(&text, "test").expect_err("an empty reason is refused");
        assert_eq!(error.code.0, "AURA-ML-5105");
        assert!(error.detail.contains("teeth"), "{}", error.detail);
    }

    /// Empty the body of the first `reason` block after a marker.
    ///
    /// Written against the shape of the file rather than against one sentence in it, so that
    /// re-wording a rationale does not silently turn this test into a no-op - which is what the
    /// previous version of it did.
    fn blank_first_reason_after(text: &str, marker: &str) -> String {
        const FENCE: &str = "reason = \"\"\"";
        let start = text.find(marker).expect("the marker is in the file");
        let open = text[start..].find(FENCE).expect("a reason block") + start;
        let body = open + FENCE.len();
        let close = text[body..].find("\"\"\"").expect("a closing fence") + body;
        format!(
            "{}
{}",
            &text[..body],
            &text[close..]
        )
    }

    #[test]
    fn a_studio_may_always_be_more_conservative() {
        let text = MicroTable::embedded_text().replace(
            "sclera_max            = 0.30",
            "sclera_max            = 0.05",
        );
        let table = MicroTable::parse(&text, "test").expect("a cautious table loads");
        assert!(table.guard().sclera_max < MAX_SCLERA);
        assert!(table.guard().is_sound());
    }
}
