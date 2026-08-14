//! `People`: the one implementation of the frozen `PeopleService`.
//!
//! Everything a later phase can do with identities goes through this type, which is
//! what makes the biometric store's three guarantees enforceable in one place instead
//! of trusted in nine: the templates are sealed, the queries are project-scoped, and
//! the photographer's decisions are unbeatable.
//!
//! ## The grouping pass, and the order of its steps
//!
//! [`People::regroup`] runs seven steps and the order is the design, not a
//! convenience:
//!
//! 1. **Cluster** the voting faces into identities.
//! 2. **Write** those identities, replacing whatever was there.
//! 3. **Replay** the photographer's decisions from `identity_links`.
//! 4. **Build** the co-occurrence graph over the *replayed* assignment.
//! 5. **Infer** roles from that graph.
//! 6. **Propose** importances from the roles and the timelines.
//! 7. **Report**, with coverage attached.
//!
//! Step 3 before step 4 is the part that is easy to get wrong. A photographer who
//! merged two identities has told the product that those faces are one person, and the
//! co-occurrence graph built before that merge would count that person as appearing
//! with themselves. Roles inferred from that graph would then place them in the couple.
//! Replaying first means every downstream conclusion is drawn from what the
//! photographer actually said.
//!
//! ## Why a re-analysis does not lose a decision
//!
//! Clustering after a model change produces new identity ids - it *is* a different
//! grouping, and pretending the old ids survived would attach a name to a person who
//! is now three people. So decisions are replayed by **face set** rather than by id:
//! each journal entry carries the faces the identity held at the time, and
//! [`identity_holding`] finds whichever new identity holds most of them. A decision
//! whose faces have scattered across four identities is applied to none of them and
//! logged, which is the honest outcome: the product does not know which of the four the
//! photographer meant.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::people::{FaceRef, ImageSubjects, PeopleService, Role, SubjectHierarchy};
use aura_core::{AuraError, AuraResult, FaceId, IdentityId, PhotoId, ProjectId};
use aura_vision::face::cluster::{self, ClusterConfig, ClusterFace};
use aura_vision::face::prominence::{self, ProminenceInput, ProminenceWeights, SubjectFace};
use aura_vision::face::roles::{self, RoleOutcome};

use crate::errors;
use crate::graph::CooccurrenceGraph;
use crate::importance;
use crate::store::{EraseReport, FaceRow, IdentityLink, IdentityRow, PeopleStore};
use crate::timeline::{self, IdentityTimeline};

/// The fewest voting faces a project needs before grouping is meaningful.
///
/// Four. Below that, "clustering" is a description of two or three photographs and the
/// honest answer is `AURA-ML-5019` plus an empty hierarchy rather than a confident
/// grouping of a wedding nobody has analysed yet.
pub const MIN_VOTING_FACES: usize = 4;

/// What one grouping pass did.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupReport {
    /// Identities created.
    pub identities: usize,
    /// Faces assigned to one.
    pub assigned: usize,
    /// Faces deliberately left unassigned.
    pub unassigned: usize,
    /// The clustering threshold that was used.
    pub threshold: f32,
    /// Merges the threshold allowed and cohesion verification refused.
    pub refused_merges: usize,
    /// Identities that grew sub-centroids for a change of look.
    pub sub_clustered: usize,
    /// The couple, when the evidence or the photographer identified one.
    pub couple: Option<(IdentityId, IdentityId)>,
    /// The couple decision's confidence.
    pub couple_confidence: f32,
    /// True when the top two candidate pairs were within the ambiguity margin.
    pub couple_ambiguous: bool,
    /// True when no scene labels were available, so the couple was chosen from
    /// co-occurrence and frequency alone.
    pub scene_starved: bool,
    /// Journal entries that were replayed onto the new grouping.
    pub decisions_replayed: usize,
    /// Journal entries whose faces had scattered too far to replay.
    pub decisions_orphaned: usize,
    /// Fraction of the project scanned for faces, `0..1`.
    pub coverage: f32,
    /// Wall clock.
    pub elapsed_ms: u64,
}

/// The subject hierarchy and identities service.
#[derive(Debug)]
pub struct People {
    store: Arc<PeopleStore>,
    clock: Arc<dyn Clock>,
    weights: ProminenceWeights,
    cluster_config: ClusterConfig,
}

impl People {
    /// Assemble the service over a store.
    #[must_use]
    pub fn new(store: Arc<PeopleStore>, clock: Arc<dyn Clock>) -> Self {
        Self {
            store,
            clock,
            weights: ProminenceWeights::embedded(),
            cluster_config: ClusterConfig::default(),
        }
    }

    /// Use a different prominence weight table.
    ///
    /// The installation override path, and how QA tunes section 6.4 without a rebuild.
    #[must_use]
    pub fn with_weights(mut self, weights: ProminenceWeights) -> Self {
        self.weights = weights;
        self
    }

    /// Use a different clustering policy. QA's threshold sweeps use this.
    #[must_use]
    pub const fn with_cluster_config(mut self, config: ClusterConfig) -> Self {
        self.cluster_config = config;
        self
    }

    /// The weight table in force.
    #[must_use]
    pub const fn weights(&self) -> &ProminenceWeights {
        &self.weights
    }

    /// The clustering policy in force.
    #[must_use]
    pub const fn cluster_config(&self) -> &ClusterConfig {
        &self.cluster_config
    }

    /// The store underneath.
    #[must_use]
    pub fn store(&self) -> &Arc<PeopleStore> {
        &self.store
    }

    /// Group a project's faces into identities, infer roles, and report.
    ///
    /// See this module's header for why the steps are in this order.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the biometric key is unavailable, `AURA-DB-3006` on a
    /// catalog failure, `AURA-ML-5019` when there are too few voting faces to group
    /// anybody.
    #[allow(clippy::too_many_lines)]
    pub fn regroup(&self, project: &ProjectId) -> AuraResult<GroupReport> {
        let started = self.clock.monotonic_ms();
        let faces = self.store.load_faces(project)?;
        let (photos, scanned, _, _) = self.store.coverage(project)?;
        let coverage = if photos <= 0 {
            0.0
        } else {
            let per_mille = (scanned.saturating_mul(1_000) / photos).clamp(0, 1_000);
            f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
        };

        // Section 12: a version mismatch is reported, never silently compared.
        let versions = self.store.face_versions(project)?;
        let current = (
            aura_vision::face::detect::MODEL_VER,
            aura_vision::face::embed::EMBED_VER,
        );
        for (detector, recogniser, rows) in versions
            .iter()
            .filter(|(detector, recogniser, _)| (*detector, *recogniser) != current)
        {
            let mismatch =
                errors::face_version_mismatch(*detector, *recogniser, current.0, current.1, *rows);
            tracing::warn!(target: "people.group", code = %mismatch.code, "{}", mismatch.detail);
        }

        if faces.len() > FACE_CEILING {
            let over = errors::face_ceiling(faces.len(), FACE_CEILING);
            tracing::warn!(target: "people.group", code = %over.code, "{}", over.detail);
        }

        // 1. Cluster. Only faces with a template can be clustered at all; a face whose
        //    envelope would not open is a face with no opinion about identity.
        let inputs: Vec<ClusterFace> = faces
            .iter()
            .enumerate()
            .filter_map(|(index, face)| {
                face.template.as_ref().map(|template| ClusterFace {
                    key: index as u64,
                    template: template.clone(),
                    quality: face.quality,
                    votes: face.votes,
                })
            })
            .collect();
        // The mapping back, because `inputs` is a filtered subset and its indices are
        // not the face slice's.
        let back: Vec<usize> = faces
            .iter()
            .enumerate()
            .filter(|(_, face)| face.template.is_some())
            .map(|(index, _)| index)
            .collect();

        let voting = inputs.iter().filter(|face| face.votes).count();
        if voting < MIN_VOTING_FACES {
            let under =
                errors::identity_underdetermined(&project.to_db(), voting, MIN_VOTING_FACES);
            tracing::warn!(target: "people.group", code = %under.code, "{}", under.detail);
            return Ok(GroupReport {
                identities: 0,
                assigned: 0,
                unassigned: faces.len(),
                threshold: self.cluster_config.threshold,
                refused_merges: 0,
                sub_clustered: 0,
                couple: None,
                couple_confidence: 0.0,
                couple_ambiguous: false,
                scene_starved: true,
                decisions_replayed: 0,
                decisions_orphaned: 0,
                coverage,
                elapsed_ms: self.clock.monotonic_ms().saturating_sub(started),
            });
        }

        let outcome = cluster::cluster(&inputs, &self.cluster_config);

        // Translate cluster membership from `inputs` indices back to `faces` indices.
        let translated: Vec<cluster::Cluster> = outcome
            .clusters
            .iter()
            .map(|found| cluster::Cluster {
                members: found
                    .members
                    .iter()
                    .filter_map(|index| back.get(*index).copied())
                    .collect(),
                centroid: found.centroid.clone(),
                sub_centroids: found.sub_centroids.clone(),
                variance: found.variance,
            })
            .collect();

        // 2. Write.
        self.store
            .put_identities(project, &faces, &translated, cluster::CLUSTER_VER)?;

        // 3. Replay.
        let refreshed = self.store.load_faces(project)?;
        let (replayed, orphaned) = self.replay_decisions(project, &refreshed)?;

        // 4. Build the graph over what the photographer actually said.
        let assigned_faces = self.store.load_faces(project)?;
        let timestamps = self.store.photo_timestamps(project)?;
        // PHASE-07. The scene labels are real now. An empty map is still a legitimate
        // answer - a project whose scene pass has not run - and `RoleOutcome::
        // scene_starved` reports which of the two happened, so a couple decision made
        // without scenes is still capped at `SCENELESS_CONFIDENCE_CEILING`.
        let scenes = self.store.scene_labels(project)?;
        let graph = CooccurrenceGraph::build(&assigned_faces, &scenes);
        self.store.put_cooccurrence(project, &graph.to_rows())?;

        // 5. Infer roles.
        let identities = self.store.load_identities(project)?;
        let (evidence, handles) =
            crate::graph::identity_evidence(&identities, &assigned_faces, &graph, &timestamps);
        let pairs = graph.pair_evidence(&handles);
        let role_outcome = roles::infer_roles(&evidence, &pairs);
        let by_handle: BTreeMap<usize, IdentityId> =
            handles.iter().map(|(id, index)| (*index, *id)).collect();

        for verdict in &role_outcome.verdicts {
            if verdict.locked {
                // A photographer's decision was copied through. Writing it back would be
                // a no-op at best and would bump `updated_at` at worst.
                continue;
            }
            let Some(identity) = by_handle.get(&verdict.index) else {
                continue;
            };
            self.store.set_role(
                project,
                *identity,
                verdict.role,
                verdict.confidence,
                &verdict.reasons,
                false,
            )?;
        }

        if role_outcome.ambiguous {
            let ambiguous = errors::couple_ambiguous(
                &project.to_db(),
                role_outcome.couple_score,
                role_outcome.runner_up_score,
            );
            tracing::warn!(target: "people.group", code = %ambiguous.code, "{}", ambiguous.detail);
        }

        // Section 11: `identity.cluster` and `identity.role_inferred`.
        tracing::info!(
            target: "identity.cluster",
            faces_used = outcome.skeleton_faces,
            identities = translated.len(),
            threshold = outcome.threshold,
            refused_merges = outcome.refused_merges,
            ms = self.clock.monotonic_ms().saturating_sub(started),
            "identities clustered"
        );
        for verdict in &role_outcome.verdicts {
            tracing::info!(
                target: "identity.role_inferred",
                role = verdict.role.as_str(),
                confidence = verdict.confidence,
                evidence_kinds = verdict.reasons.len(),
                "role inferred"
            );
        }

        // 6. Importance.
        let timelines = timeline::build_timelines(&assigned_faces, &timestamps, &scenes, &graph);
        let dominant = self.dominant_counts(&assigned_faces, &identities);
        let identities = self.store.load_identities(project)?;
        let proposals = importance::propose(
            &importance::inputs_from(&identities, &timelines, &dominant),
            &self.weights,
        );
        for proposal in proposals.iter().filter(|p| p.is_change()) {
            self.store
                .set_importance(project, proposal.identity, proposal.importance)?;
        }

        let couple =
            role_outcome
                .couple
                .and_then(|(a, b)| match (by_handle.get(&a), by_handle.get(&b)) {
                    (Some(left), Some(right)) => Some((*left, *right)),
                    _ => None,
                });

        Ok(GroupReport {
            identities: translated.len(),
            assigned: assigned_faces
                .iter()
                .filter(|face| face.identity_id.is_some())
                .count(),
            unassigned: assigned_faces
                .iter()
                .filter(|face| face.identity_id.is_none())
                .count(),
            threshold: outcome.threshold,
            refused_merges: outcome.refused_merges,
            sub_clustered: outcome.sub_clustered,
            couple,
            couple_confidence: couple_confidence(&role_outcome),
            couple_ambiguous: role_outcome.ambiguous,
            scene_starved: role_outcome.scene_starved,
            decisions_replayed: replayed,
            decisions_orphaned: orphaned,
            coverage,
            elapsed_ms: self.clock.monotonic_ms().saturating_sub(started),
        })
    }

    /// Replay the photographer's decisions onto a fresh grouping.
    ///
    /// Returns how many were applied and how many were orphaned. See this module's
    /// header for why this is done by face set rather than by identity id.
    fn replay_decisions(
        &self,
        project: &ProjectId,
        faces: &[FaceRow],
    ) -> AuraResult<(usize, usize)> {
        let links = self.store.replayable_links(project)?;
        let mut applied = 0usize;
        let mut orphaned = 0usize;

        for link in &links {
            let Some(target) = identity_holding(faces, &link.face_ids) else {
                orphaned += 1;
                tracing::warn!(
                    target: "people.replay",
                    kind = %link.kind,
                    faces = link.face_ids.len(),
                    "a photographer's decision could not be replayed: its faces are now spread \
                     across several identities or none"
                );
                continue;
            };

            let outcome = match link.kind.as_str() {
                "rename" => self
                    .store
                    .set_label(project, target, link.new_value.as_deref())
                    .map(|_| ()),
                "role" => {
                    let role = link
                        .new_value
                        .as_deref()
                        .map_or(Role::Unknown, Role::from_str_or_unknown);
                    self.store
                        .set_role(
                            project,
                            target,
                            role,
                            1.0,
                            &[format!(
                                "set to {role} by the photographer; replayed after a re-analysis"
                            )],
                            true,
                        )
                        .map(|_| ())
                }
                "importance" => {
                    let value = link
                        .new_value
                        .as_deref()
                        .and_then(|text| text.parse::<f32>().ok())
                        .unwrap_or(importance::NEUTRAL);
                    self.store
                        .set_importance(project, target, value)
                        .map(|_| ())
                }
                "merge" => {
                    // The absorbed identity's faces are in `face_ids`, so a replay is
                    // simply "put them back on the survivor" - which is what
                    // `identity_holding` just found, unless the clustering has already
                    // done it, in which case this is a no-op.
                    self.store
                        .assign_faces(project, Some(target), &link.face_ids)
                        .map(|_| ())
                }
                "split" => {
                    let Some(anchor) = link.face_ids.iter().min().copied() else {
                        orphaned += 1;
                        continue;
                    };
                    let created =
                        self.store
                            .create_identity(project, anchor, cluster::CLUSTER_VER)?;
                    self.store
                        .assign_faces(project, Some(created), &link.face_ids)
                        .map(|_| ())
                }
                other => {
                    tracing::warn!(
                        target: "people.replay",
                        kind = other,
                        "unknown journal entry kind; skipped"
                    );
                    orphaned += 1;
                    continue;
                }
            };

            match outcome {
                Ok(()) => applied += 1,
                Err(err) => {
                    tracing::warn!(
                        target: "people.replay",
                        kind = %link.kind,
                        code = %err.code,
                        "a photographer's decision could not be replayed"
                    );
                    orphaned += 1;
                }
            }
        }
        Ok((applied, orphaned))
    }

    /// How many frames each identity is the dominant subject of.
    ///
    /// One pass over the faces grouped by photograph, scoring each frame once. The
    /// alternative - calling `subjects` per frame - would reload the identity table four
    /// thousand times.
    fn dominant_counts(
        &self,
        faces: &[FaceRow],
        identities: &[IdentityRow],
    ) -> BTreeMap<IdentityId, u32> {
        let hierarchy = self.hierarchy_from(identities, 1.0, false);
        let mut per_frame: BTreeMap<PhotoId, Vec<&FaceRow>> = BTreeMap::new();
        for face in faces {
            per_frame.entry(face.image_id).or_default().push(face);
        }

        let mut counts: BTreeMap<IdentityId, u32> = BTreeMap::new();
        for (_, frame_faces) in per_frame {
            let subjects = self.subjects_from(&frame_faces, identities, &hierarchy, None, 0);
            if let Some(dominant) = subjects.dominant {
                *counts.entry(dominant).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Build a hierarchy from already-loaded identities.
    fn hierarchy_from(
        &self,
        identities: &[IdentityRow],
        coverage: f32,
        unconfirmed: bool,
    ) -> SubjectHierarchy {
        let rows: Vec<(IdentityId, Role, f32)> = identities
            .iter()
            .map(|identity| (identity.identity_id, identity.role, identity.importance))
            .collect();
        prominence::subject_hierarchy(&rows, coverage, unconfirmed, &self.weights)
    }

    /// Score one frame from already-loaded rows.
    fn subjects_from(
        &self,
        faces: &[&FaceRow],
        identities: &[IdentityRow],
        hierarchy: &SubjectHierarchy,
        scene: Option<&str>,
        people_count: u32,
    ) -> ImageSubjects {
        let by_id: BTreeMap<IdentityId, &IdentityRow> = identities
            .iter()
            .map(|identity| (identity.identity_id, identity))
            .collect();

        let subject_faces: Vec<SubjectFace> = faces
            .iter()
            .map(|face| {
                let identity = face.identity_id.and_then(|id| by_id.get(&id));
                SubjectFace {
                    reference: FaceRef {
                        face_id: face.face_id,
                        identity_id: face.identity_id,
                        area_frac: face.area_frac,
                        centrality: face.centrality,
                        sharpness: face.sharpness,
                        quality: face.quality,
                        votes: face.votes,
                    },
                    input: ProminenceInput {
                        area_frac: face.area_frac,
                        centrality: face.centrality,
                        sharpness: face.sharpness,
                        gaze: prominence::gaze_from_pose(face.pose),
                        role: identity.map_or(Role::Unknown, |row| row.role),
                        importance: identity.map_or(importance::NEUTRAL, |row| row.importance),
                    },
                }
            })
            .collect();

        prominence::image_subjects(
            &subject_faces,
            hierarchy,
            scene,
            people_count,
            &self.weights,
        )
    }

    /// Every identity's timeline, for the People panel.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` or `AURA-DB-3006`.
    pub fn timelines(&self, project: &ProjectId) -> AuraResult<Vec<IdentityTimeline>> {
        let faces = self.store.load_faces(project)?;
        let timestamps = self.store.photo_timestamps(project)?;
        let scenes = self.store.scene_labels(project)?;
        let rows = self.store.load_cooccurrence(project)?;
        let graph = CooccurrenceGraph::from_rows(&rows);
        Ok(timeline::build_timelines(
            &faces,
            &timestamps,
            &scenes,
            &graph,
        ))
    }

    /// Erase every trace of biometric data for one project.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9003` when anything is left behind.
    pub fn erase(&self, project: &ProjectId) -> AuraResult<EraseReport> {
        self.store.erase(project)
    }

    /// Rename an identity, and record it so the name survives a re-analysis.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9005` when the identity is unknown.
    pub fn rename(&self, identity: IdentityId, label: Option<&str>) -> Result<(), AuraError> {
        let project = self.owner_of(identity)?;
        let faces = self.store.faces_of_identity(identity)?;
        let changed = self.store.set_label(&project, identity, label)?;
        if changed == 0 {
            return Err(errors::edit_refused(
                "rename",
                "that identity does not exist in this project",
            ));
        }
        self.store.record_link(
            &project,
            &IdentityLink {
                kind: "rename".to_string(),
                identity_id: identity,
                other_id: None,
                face_ids: faces,
                old_value: None,
                new_value: label.map(ToString::to_string),
                actor: "user".to_string(),
            },
        )?;
        Ok(())
    }

    /// Move the importance slider, and record it.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9005` when the identity is unknown.
    pub fn set_importance(&self, identity: IdentityId, importance: f32) -> Result<(), AuraError> {
        let project = self.owner_of(identity)?;
        let faces = self.store.faces_of_identity(identity)?;
        let changed = self.store.set_importance(&project, identity, importance)?;
        if changed == 0 {
            return Err(errors::edit_refused(
                "importance",
                "that identity does not exist in this project",
            ));
        }
        self.store.record_link(
            &project,
            &IdentityLink {
                kind: "importance".to_string(),
                identity_id: identity,
                other_id: None,
                face_ids: faces,
                old_value: None,
                new_value: Some(format!("{importance:.3}")),
                actor: "user".to_string(),
            },
        )?;
        Ok(())
    }

    /// Undo the most recent decision on one identity.
    ///
    /// Section 10.1: "merge/split/rename are undoable". The journal entry is marked
    /// undone rather than deleted, so the audit trail of what was decided and then
    /// changed survives - which is the difference between an undo and a rewrite of
    /// history.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a catalog failure.
    pub fn undo(&self, identity: IdentityId) -> AuraResult<Option<IdentityLink>> {
        let project = self.owner_of(identity)?;
        self.store.undo_last_link(&project, identity)
    }

    /// Which project owns an identity, or a refusal.
    fn owner_of(&self, identity: IdentityId) -> Result<ProjectId, AuraError> {
        self.store
            .project_of_identity(identity)?
            .ok_or_else(|| errors::edit_refused("identity lookup", "that identity does not exist"))
    }
}

/// The documented ceiling on faces held in memory during one grouping pass.
///
/// Section 11 budgets clustering for 25,000 faces. Past that, `AURA-ML-5021` is raised
/// and the pass still completes - the skeleton cap in
/// `aura_vision::face::cluster::MAX_SKELETON` bounds the quadratic work regardless -
/// but the photographer is told why it is slow.
pub const FACE_CEILING: usize = 25_000;

/// The identity that holds most of a set of faces.
///
/// A strict majority, not a plurality. A decision whose faces are split evenly between
/// two identities is a decision the product cannot honour without guessing which one
/// the photographer meant, and guessing on a rename is how somebody's grandmother ends
/// up labelled with a stranger's name.
#[must_use]
pub fn identity_holding(faces: &[FaceRow], face_ids: &[FaceId]) -> Option<IdentityId> {
    if face_ids.is_empty() {
        return None;
    }
    let mut counts: BTreeMap<IdentityId, usize> = BTreeMap::new();
    let mut found = 0usize;
    for face in faces.iter().filter(|face| face_ids.contains(&face.face_id)) {
        found += 1;
        if let Some(identity) = face.identity_id {
            *counts.entry(identity).or_insert(0) += 1;
        }
    }
    if found == 0 {
        return None;
    }
    // Highest count, ties broken by id so two machines agree.
    let (identity, count) = counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))?;
    if count * 2 > found {
        Some(identity)
    } else {
        None
    }
}

/// The couple decision's confidence, from the verdicts rather than the raw score.
fn couple_confidence(outcome: &RoleOutcome) -> f32 {
    outcome
        .verdicts
        .iter()
        .filter(|verdict| verdict.role.is_couple())
        .map(|verdict| verdict.confidence)
        .fold(0.0f32, f32::max)
}

impl PeopleService for People {
    fn hierarchy(&self, project: ProjectId) -> AuraResult<SubjectHierarchy> {
        let identities = self.store.load_identities(&project)?;
        let (photos, scanned, _, _) = self.store.coverage(&project)?;
        let coverage = if photos <= 0 {
            0.0
        } else {
            let per_mille = (scanned.saturating_mul(1_000) / photos).clamp(0, 1_000);
            f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
        };
        // Unconfirmed until a photographer has locked at least one half of the couple.
        // An inferred `Role::Couple` is a proposal; `Role::Bride` or `Role::Groom` with
        // `user_locked` is an answer.
        let confirmed = identities
            .iter()
            .any(|identity| identity.user_locked && identity.role.is_couple());
        Ok(self.hierarchy_from(&identities, coverage, !confirmed))
    }

    fn subjects(&self, image: aura_core::contract::people::ImageId) -> AuraResult<ImageSubjects> {
        let Some(project) = self.store.project_of_photo(image)? else {
            // A photograph that is not in the catalog has no subjects. An empty answer
            // rather than an error: a panel asking about a frame that was just removed
            // should not get a failure.
            return Ok(ImageSubjects {
                weights_ver: self.weights.version,
                ..ImageSubjects::default()
            });
        };
        let faces = self.store.faces_of_image(&project, image)?;
        let identities = self.store.load_identities(&project)?;
        let hierarchy = self.hierarchy(project)?;
        let borrowed: Vec<&FaceRow> = faces.iter().collect();

        // People count from the stored bodies rather than recomputed: the association
        // was made when the pixels were in memory and cannot be improved on now.
        let people_count = u32::try_from(faces.len()).unwrap_or(0);
        Ok(self.subjects_from(&borrowed, &identities, &hierarchy, None, people_count))
    }

    fn merge(&self, a: IdentityId, b: IdentityId) -> Result<IdentityId, AuraError> {
        if a == b {
            return Err(errors::edit_refused(
                "merge",
                "an identity cannot be merged with itself",
            ));
        }
        let project = self.owner_of(a)?;
        self.store.assert_same_project(&project, &[a, b])?;

        let moved = self.store.faces_of_identity(b)?;
        if moved.is_empty() {
            return Err(errors::edit_refused(
                "merge",
                "the identity being merged has no faces",
            ));
        }
        self.store.assign_faces(&project, Some(a), &moved)?;
        self.store.delete_identity(&project, b)?;
        self.store.record_link(
            &project,
            &IdentityLink {
                kind: "merge".to_string(),
                identity_id: a,
                other_id: Some(b),
                face_ids: moved,
                old_value: Some(b.to_db()),
                new_value: Some(a.to_db()),
                actor: "user".to_string(),
            },
        )?;

        tracing::info!(
            target: "people.user_edit",
            action = "merge",
            identities = 2,
            "identities merged"
        );
        Ok(a)
    }

    fn split(&self, id: IdentityId, faces: &[FaceId]) -> Result<IdentityId, AuraError> {
        if faces.is_empty() {
            return Err(errors::edit_refused(
                "split",
                "a split needs at least one face to move",
            ));
        }
        let project = self.owner_of(id)?;
        let members = self.store.faces_of_identity(id)?;

        // Every face must belong to this identity, and something must be left behind.
        // The second check is what stops a "split" that is really a rename with extra
        // steps, and would leave an empty identity behind for the panel to show.
        if let Some(stray) = faces.iter().find(|face| !members.contains(face)) {
            return Err(errors::edit_refused(
                "split",
                &format!("face {stray} does not belong to that identity"),
            ));
        }
        if faces.len() >= members.len() {
            return Err(errors::edit_refused(
                "split",
                "a split has to leave at least one face behind; rename the identity instead",
            ));
        }

        let Some(anchor) = faces.iter().min().copied() else {
            return Err(errors::edit_refused("split", "no faces to move"));
        };
        let created = self
            .store
            .create_identity(&project, anchor, cluster::CLUSTER_VER)?;
        self.store.assign_faces(&project, Some(created), faces)?;
        self.store.record_link(
            &project,
            &IdentityLink {
                kind: "split".to_string(),
                identity_id: created,
                other_id: Some(id),
                face_ids: faces.to_vec(),
                old_value: Some(id.to_db()),
                new_value: Some(created.to_db()),
                actor: "user".to_string(),
            },
        )?;

        tracing::info!(
            target: "people.user_edit",
            action = "split",
            identities = 2,
            "identity split"
        );
        Ok(created)
    }

    fn set_role(&self, id: IdentityId, role: Role) -> Result<(), AuraError> {
        let project = self.owner_of(id)?;
        let faces = self.store.faces_of_identity(id)?;
        let changed = self.store.set_role(
            &project,
            id,
            role,
            1.0,
            &[format!("set to {role} by the photographer")],
            true,
        )?;
        if changed == 0 {
            return Err(errors::edit_refused(
                "role",
                "that identity does not exist in this project",
            ));
        }
        self.store.record_link(
            &project,
            &IdentityLink {
                kind: "role".to_string(),
                identity_id: id,
                other_id: None,
                face_ids: faces,
                old_value: None,
                new_value: Some(role.as_str().to_string()),
                actor: "user".to_string(),
            },
        )?;

        tracing::info!(
            target: "people.user_edit",
            action = "role",
            identities = 1,
            role = role.as_str(),
            "role set by the photographer"
        );
        Ok(())
    }
}
