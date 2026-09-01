//! The twenty-five stage declarations, and nothing else.
//!
//! Section 4 of the phase document asks for "one thin adapter per pipeline stage". This is the
//! thin half: what a stage *is* - its dependencies, its unit, its checkpoint granularity, whether
//! the wedding survives without it, what it costs and what it wants from the machine. The other
//! half - how a stage is actually executed - is `StageRunner`, implemented in `aura-app`, because
//! this crate depends on none of the twenty-two deciding crates and must not.
//!
//! Grouped into five files by lane rather than one file per stage: `analysis` for the eight
//! measuring stages, `selection` for the cull, `edit` for the nine per-frame editing stages,
//! `gallery` for the three set-level stages, and `deliver` for QC, curation and export. Twenty-five
//! files of eight lines each would be twenty-five places to look for one table.
//!
//! ## Why the table is `const`
//!
//! Because the DAG's correctness is the one thing in this phase nobody may edit. `autopilot.toml`
//! decides which stages a photographer wants *run*; it cannot decide what depends on what, and a
//! stage list assembled at run time would be a stage list a studio could reorder into a wedding
//! that graded before it culled. [`crate::policy`] is the file, this is the graph, and the split
//! is deliberate.
//!
//! ## Why the estimates are what they are
//!
//! `est_ms_per_item` is the *declared* estimate that gets a run through its first minute before
//! this machine has measured anything. Every one of them was taken from the phase's own section 11
//! budget divided by its unit count, on the reference laptop, and none of them was measured on the
//! machine this repository is built on. That is condition C4 of the exit report. They are wrong in
//! a direction nobody can predict and they stop mattering ten per cent into a run, when
//! `Eta::measured` goes true.

pub mod analysis;
pub mod deliver;
pub mod edit;
pub mod gallery;
pub mod selection;

use crate::contract::autopilot::{StageDecl, StageId};

/// Every stage's declaration, in [`StageId::ALL`] order.
///
/// The order of this array is the order of `StageId::ALL`, and a test asserts it. Nothing about
/// execution depends on that - the scheduler works from `depends_on` - but a lookup that assumed
/// it and was wrong would silently return a different stage's resource needs.
pub const ALL: [StageDecl; StageId::COUNT] = [
    analysis::INGEST,
    analysis::PREVIEWS,
    analysis::EMBED,
    analysis::FACES,
    analysis::STORY,
    analysis::MOMENTS,
    analysis::INTEGRITY,
    analysis::EMOTION,
    analysis::COMPOSITION,
    selection::CULL,
    edit::MASKS,
    edit::TONE,
    edit::COLOUR,
    edit::STYLE,
    edit::LOCAL_LIGHT,
    edit::RETOUCH,
    edit::MICRO,
    edit::RESTORATION,
    edit::GEOMETRY,
    edit::CLEANUP,
    gallery::CAMERA_MATCH,
    gallery::CONSISTENCY,
    deliver::QC,
    deliver::CURATION,
    deliver::EXPORT,
];

/// One stage's declaration.
///
/// Total rather than fallible: [`StageId`] is a closed enum and [`ALL`] has a row for every
/// variant, which a test asserts. A `get` that could return `None` here would be a lookup every
/// caller had to handle and none of them could.
#[must_use]
pub fn decl(stage: StageId) -> &'static StageDecl {
    let index = StageId::ALL
        .iter()
        .position(|candidate| *candidate == stage)
        .unwrap_or(0);
    #[allow(clippy::indexing_slicing)]
    {
        &ALL[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::autopilot::{CheckpointKind, StageScope};

    #[test]
    fn the_table_is_in_stage_id_order() {
        for (index, stage) in StageId::ALL.into_iter().enumerate() {
            #[allow(clippy::indexing_slicing)]
            let decl = &ALL[index];
            assert_eq!(decl.id, stage, "row {index} is not {stage}");
            assert_eq!(decl.name, stage.as_str(), "row {index} has the wrong name");
        }
    }

    #[test]
    fn every_stage_has_a_declaration() {
        for stage in StageId::ALL {
            assert_eq!(decl(stage).id, stage);
        }
    }

    #[test]
    fn no_stage_depends_on_itself() {
        for row in ALL {
            assert!(
                !row.depends_on.contains(&row.id),
                "{} depends on itself",
                row.id
            );
        }
    }

    #[test]
    fn a_gallery_stage_checkpoints_per_stage() {
        // A set-level solver has no half-finished state a resume could continue from. Declaring
        // one `PerImage` would produce a checkpoint that claimed 400 of 1 units done.
        for row in ALL {
            if row.scope == StageScope::Gallery {
                assert_eq!(
                    row.checkpoint,
                    CheckpointKind::PerStage,
                    "{} is a gallery stage and does not checkpoint per stage",
                    row.id
                );
            }
        }
    }

    #[test]
    fn every_estimate_is_positive() {
        // A zero estimate is an ETA of zero for every stage that has not started, which is a run
        // that promises to finish immediately and then does not.
        for row in ALL {
            assert!(row.est_ms_per_item > 0, "{} estimates nothing", row.id);
        }
    }

    #[test]
    fn a_stage_that_wants_no_gpu_asks_for_no_vram() {
        for row in ALL {
            if !row.resources.gpu {
                assert_eq!(
                    row.resources.vram_mb, 0,
                    "{} is CPU-only and asks for video memory",
                    row.id
                );
            }
        }
    }

    #[test]
    fn the_stages_a_wedding_cannot_do_without_are_the_ones_that_are_not_optional() {
        // Ingest, previews, embed and cull. Everything else can be skipped and still leave a
        // photographer with a wedding: a gallery with no retouching is a gallery, and a gallery
        // with no selection is four thousand unsorted files.
        let mandatory: Vec<StageId> = ALL
            .iter()
            .filter(|row| !row.optional)
            .map(|row| row.id)
            .collect();
        assert_eq!(
            mandatory,
            vec![
                StageId::Ingest,
                StageId::Previews,
                StageId::Embed,
                StageId::Cull
            ]
        );
    }
}
