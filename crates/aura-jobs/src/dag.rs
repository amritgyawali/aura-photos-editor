//! The stage graph: what may run, in what order, and what a disabled stage does to the rest.
//!
//! Twenty-five declarations and one topological order. The whole of this module is arithmetic over
//! [`crate::stages::ALL`], and it decides nothing about a photograph - it decides what the runner
//! is allowed to start next.
//!
//! ## Why the order is deterministic rather than merely valid
//!
//! Invariant 4. Two runs of the same wedding on the same build must do the same things in the same
//! order, because a stage order that varied would make a resumed run a different run - and the
//! checkpoint that resumed it would be keyed to a plan that no longer exists. Kahn's algorithm
//! with the ready set drained in [`StageId::ALL`] order gives one answer, always, and a test
//! asserts it is the same answer every time.
//!
//! ## What a disabled stage does
//!
//! Nothing to the graph. A stage the photographer switched off is still visited, still recorded
//! and still unblocks its dependents - it is [`StageOutcome::Skipped`] with
//! [`SkipCause::TurnedOff`]. Removing it from the graph instead would make its dependents look
//! like stages with fewer dependencies than they have, which is how a build ends up grading before
//! it culls because somebody unticked a box.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::autopilot::{StageDecl, StageId};
use crate::stages;

/// A validated stage graph.
///
/// Construction is the validation: [`Dag::build`] refuses a cycle and refuses an edge naming a
/// stage that is not declared, so a `Dag` that exists is a graph the runner can walk. There is no
/// mutating method - the graph is the product's shape rather than a run's state.
#[derive(Debug, Clone)]
pub struct Dag {
    order: Vec<StageId>,
    dependents: BTreeMap<StageId, Vec<StageId>>,
    dependencies: BTreeMap<StageId, Vec<StageId>>,
}

/// Why a graph was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagError {
    /// Two or more stages depend on each other, directly or through a chain.
    Cycle {
        /// The stages that could never become ready, in declaration order.
        stuck: Vec<StageId>,
    },
    /// A declaration names a dependency with no declaration of its own.
    UnknownDependency {
        /// The stage whose list is wrong.
        stage: StageId,
        /// What it named.
        missing: StageId,
    },
}

impl std::fmt::Display for DagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cycle { stuck } => {
                write!(f, "the stage graph has a cycle involving")?;
                for stage in stuck {
                    write!(f, " {stage}")?;
                }
                Ok(())
            }
            Self::UnknownDependency { stage, missing } => {
                write!(f, "{stage} depends on {missing}, which is not declared")
            }
        }
    }
}

impl std::error::Error for DagError {}

impl Dag {
    /// Build the product's graph from [`crate::stages::ALL`].
    ///
    /// # Errors
    ///
    /// [`DagError::Cycle`] when the declarations cannot be ordered, and
    /// [`DagError::UnknownDependency`] when one of them names a stage that does not exist. Neither
    /// is reachable from the shipped table - a test proves it on every build - and both are here
    /// because the alternative is a scheduler that spins forever on a table somebody edited.
    pub fn build() -> Result<Self, DagError> {
        Self::from_declarations(&stages::ALL)
    }

    /// Build a graph from an arbitrary table, for the tests that prove the refusals.
    ///
    /// # Errors
    ///
    /// As [`Dag::build`].
    pub fn from_declarations(declarations: &[StageDecl]) -> Result<Self, DagError> {
        let declared: BTreeSet<StageId> = declarations.iter().map(|decl| decl.id).collect();
        let mut remaining: BTreeMap<StageId, BTreeSet<StageId>> = BTreeMap::new();
        let mut dependents: BTreeMap<StageId, Vec<StageId>> = BTreeMap::new();
        let mut dependencies: BTreeMap<StageId, Vec<StageId>> = BTreeMap::new();

        for decl in declarations {
            let mut needs = BTreeSet::new();
            for dependency in decl.depends_on {
                if !declared.contains(dependency) {
                    return Err(DagError::UnknownDependency {
                        stage: decl.id,
                        missing: *dependency,
                    });
                }
                needs.insert(*dependency);
                dependents.entry(*dependency).or_default().push(decl.id);
            }
            dependencies.insert(decl.id, decl.depends_on.to_vec());
            remaining.insert(decl.id, needs);
        }

        // Kahn, with the ready set drained in declaration order rather than in whatever order a
        // hash map hands back. That is the difference between a valid order and *the* order, and
        // invariant 4 needs the second.
        let declaration_order: Vec<StageId> = declarations.iter().map(|decl| decl.id).collect();
        let mut order = Vec::with_capacity(declarations.len());
        let mut done: BTreeSet<StageId> = BTreeSet::new();

        while order.len() < declarations.len() {
            let next = declaration_order.iter().copied().find(|stage| {
                !done.contains(stage)
                    && remaining
                        .get(stage)
                        .is_some_and(|needs| needs.iter().all(|need| done.contains(need)))
            });
            match next {
                Some(stage) => {
                    done.insert(stage);
                    order.push(stage);
                }
                None => {
                    let stuck = declaration_order
                        .iter()
                        .copied()
                        .filter(|stage| !done.contains(stage))
                        .collect();
                    return Err(DagError::Cycle { stuck });
                }
            }
        }

        Ok(Self {
            order,
            dependents,
            dependencies,
        })
    }

    /// Every stage, in the order the runner visits them.
    #[must_use]
    pub fn order(&self) -> &[StageId] {
        &self.order
    }

    /// The stages that depend on this one, directly.
    #[must_use]
    pub fn dependents_of(&self, stage: StageId) -> &[StageId] {
        self.dependents
            .get(&stage)
            .map_or(&[] as &[StageId], Vec::as_slice)
    }

    /// The stages this one depends on, directly.
    ///
    /// Read out of the graph rather than out of [`crate::stages::decl`], so a `Dag` built from a
    /// test table answers about that table. A method that always read the shipped declarations
    /// would make every refusal test a test of the shipped graph instead.
    #[must_use]
    pub fn dependencies_of(&self, stage: StageId) -> &[StageId] {
        self.dependencies
            .get(&stage)
            .map_or(&[] as &[StageId], Vec::as_slice)
    }

    /// Every stage reachable downstream of this one, including it.
    ///
    /// What an invalidated checkpoint costs. Section 6.1 says an upstream change forces "a clean
    /// re-run of the affected stage only", and that is true of the *stage whose inputs moved* -
    /// but the stages below it read what it wrote, so their own `inputs_hash` moves too and they
    /// re-run on their own account. This function is how the panel says so before it happens
    /// rather than after.
    #[must_use]
    pub fn downstream_of(&self, stage: StageId) -> Vec<StageId> {
        let mut reached: BTreeSet<StageId> = BTreeSet::new();
        let mut frontier = vec![stage];
        while let Some(current) = frontier.pop() {
            if !reached.insert(current) {
                continue;
            }
            for dependent in self.dependents_of(current) {
                frontier.push(*dependent);
            }
        }
        self.order
            .iter()
            .copied()
            .filter(|candidate| reached.contains(candidate))
            .collect()
    }

    /// Whether every dependency of `stage` is in `finished`.
    #[must_use]
    pub fn is_ready(&self, stage: StageId, finished: &BTreeSet<StageId>) -> bool {
        self.dependencies_of(stage)
            .iter()
            .all(|need| finished.contains(need))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::autopilot::{CheckpointKind, ResourceNeeds, StageScope};

    fn decl(id: StageId, depends_on: &'static [StageId]) -> StageDecl {
        StageDecl {
            id,
            name: id.as_str(),
            depends_on,
            scope: StageScope::Gallery,
            checkpoint: CheckpointKind::PerStage,
            optional: true,
            est_ms_per_item: 1,
            resources: ResourceNeeds::cpu(1, 1),
        }
    }

    #[test]
    fn the_shipped_graph_builds() {
        let dag = Dag::build().expect("the shipped stage table must be a valid graph");
        assert_eq!(dag.order().len(), StageId::COUNT);
    }

    #[test]
    fn the_order_is_the_same_on_every_build() {
        let first = Dag::build().expect("graph").order().to_vec();
        let second = Dag::build().expect("graph").order().to_vec();
        assert_eq!(first, second);
    }

    #[test]
    fn every_dependency_comes_before_its_dependent() {
        let dag = Dag::build().expect("graph");
        let order = dag.order();
        for stage in StageId::ALL {
            let position = order
                .iter()
                .position(|candidate| *candidate == stage)
                .expect("every stage is in the order");
            for dependency in dag.dependencies_of(stage) {
                let dependency_position = order
                    .iter()
                    .position(|candidate| candidate == dependency)
                    .expect("every dependency is in the order");
                assert!(
                    dependency_position < position,
                    "{stage} runs before its dependency {dependency}"
                );
            }
        }
    }

    #[test]
    fn the_cull_separates_the_two_scopes() {
        // Invariant 3 as a property of the graph rather than a convention: everything before the
        // cull works on every photograph and everything after it works on survivors.
        let dag = Dag::build().expect("graph");
        let cull = dag
            .order()
            .iter()
            .position(|stage| *stage == StageId::Cull)
            .expect("the cull is in the order");
        for (index, stage) in dag.order().iter().enumerate() {
            let scope = crate::stages::decl(*stage).scope;
            if index < cull {
                assert_ne!(
                    scope,
                    StageScope::SelectedImages,
                    "{stage} runs before the cull and works on selected frames"
                );
            }
        }
    }

    #[test]
    fn a_cycle_is_refused_rather_than_looped_on() {
        let table = [
            decl(StageId::Tone, &[StageId::Colour]),
            decl(StageId::Colour, &[StageId::Tone]),
        ];
        let error = Dag::from_declarations(&table).expect_err("a cycle must be refused");
        assert_eq!(
            error,
            DagError::Cycle {
                stuck: vec![StageId::Tone, StageId::Colour]
            }
        );
    }

    #[test]
    fn an_edge_to_an_undeclared_stage_is_refused() {
        let table = [decl(StageId::Colour, &[StageId::Tone])];
        let error = Dag::from_declarations(&table).expect_err("a dangling edge must be refused");
        assert_eq!(
            error,
            DagError::UnknownDependency {
                stage: StageId::Colour,
                missing: StageId::Tone
            }
        );
    }

    #[test]
    fn downstream_of_the_cull_is_most_of_the_wedding() {
        let dag = Dag::build().expect("graph");
        let downstream = dag.downstream_of(StageId::Cull);
        assert!(downstream.contains(&StageId::Cull));
        assert!(downstream.contains(&StageId::Colour));
        assert!(downstream.contains(&StageId::Export));
        assert!(!downstream.contains(&StageId::Ingest));
    }

    #[test]
    fn downstream_of_a_leaf_is_only_itself() {
        let dag = Dag::build().expect("graph");
        assert_eq!(dag.downstream_of(StageId::Export), vec![StageId::Export]);
    }

    #[test]
    fn readiness_reads_the_declared_dependencies() {
        let dag = Dag::build().expect("graph");
        let mut finished = BTreeSet::new();
        assert!(dag.is_ready(StageId::Ingest, &finished));
        assert!(!dag.is_ready(StageId::Previews, &finished));
        finished.insert(StageId::Ingest);
        assert!(dag.is_ready(StageId::Previews, &finished));
    }
}
