//! Who appears with whom.
//!
//! The co-occurrence graph is the evidence role inference actually runs on, and it is
//! the reason this product can name a couple without guessing anything about the
//! people in the frame. Two identities photographed together forty times are related;
//! two photographed together once were in the same room. That is a photographic fact
//! and it is the only kind of fact section 6.3 permits.
//!
//! Three properties, each of which matters more than it looks.
//!
//! **The pair is canonical.** An edge is stored once, with the lexicographically lower
//! identity first, so no arithmetic can double-count it. The migration enforces the
//! same ordering with a `CHECK`, so a hand-written statement cannot break it either.
//!
//! **An edge counts frames, not faces.** A frame that caught the bride twice - once
//! directly and once in a mirror - is one photograph of the bride, and counting faces
//! would make a mirror in the room look like a relationship.
//!
//! **A `BTreeMap`, never a hash map.** Every consumer of this graph iterates it, and
//! invariant 4 requires that two machines produce the same answer. `check-banned.sh`
//! refuses a hash map's constructor in this workspace for exactly this reason.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::{IdentityId, PhotoId};
use aura_vision::face::roles::{IdentityEvidence, PairEvidence};

use crate::store::FaceRow;

/// One edge: how often two identities were photographed together.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Edge {
    /// Photographs both appear in.
    pub frames: u32,
    /// Shared frames per scene label. Empty until phase 07 assigns scenes.
    pub scenes: BTreeMap<String, u32>,
}

impl Edge {
    /// The `cooccurrence.scenes` JSON payload.
    ///
    /// Hand-rolled rather than through `serde_json`, so the key order is the map's -
    /// sorted - and two runs produce byte-identical column values. A derived encoding
    /// over a hash map would not.
    #[must_use]
    pub fn scenes_json(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::from("{");
        for (index, (scene, count)) in self.scenes.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            // Writing into a String cannot fail; the result is discarded deliberately.
            let _ = write!(out, "\"{}\":{count}", scene.replace('"', ""));
        }
        out.push('}');
        out
    }
}

/// The graph for one project.
#[derive(Debug, Clone, Default)]
pub struct CooccurrenceGraph {
    edges: BTreeMap<(IdentityId, IdentityId), Edge>,
    frames: BTreeMap<IdentityId, u32>,
    scenes: BTreeMap<IdentityId, BTreeMap<String, u32>>,
}

impl CooccurrenceGraph {
    /// Build the graph from a project's assigned faces.
    ///
    /// `scenes` maps a photograph to its scene label; pass an empty map until phase 07
    /// runs, which is what happens today and is reported rather than hidden by
    /// `aura_vision::face::roles::RoleOutcome::scene_starved`.
    #[must_use]
    pub fn build(faces: &[FaceRow], scenes: &BTreeMap<PhotoId, String>) -> Self {
        // One pass to group identities by photograph, so the pair loop below is over
        // the identities in one frame rather than over every pair in the wedding.
        let mut per_frame: BTreeMap<PhotoId, BTreeSet<IdentityId>> = BTreeMap::new();
        for face in faces {
            let Some(identity) = face.identity_id else {
                continue;
            };
            per_frame.entry(face.image_id).or_default().insert(identity);
        }

        let mut graph = Self::default();
        for (photo, identities) in &per_frame {
            let scene = scenes.get(photo);
            for identity in identities {
                *graph.frames.entry(*identity).or_insert(0) += 1;
                if let Some(label) = scene {
                    *graph
                        .scenes
                        .entry(*identity)
                        .or_default()
                        .entry(label.clone())
                        .or_insert(0) += 1;
                }
            }
            // Every unordered pair in this frame. `BTreeSet` iteration is ascending, so
            // `left < right` holds and the pair is canonical without a comparison.
            let ordered: Vec<IdentityId> = identities.iter().copied().collect();
            for (position, left) in ordered.iter().enumerate() {
                for right in ordered.iter().skip(position + 1) {
                    let edge = graph.edges.entry((*left, *right)).or_default();
                    edge.frames += 1;
                    if let Some(label) = scene {
                        *edge.scenes.entry(label.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
        graph
    }

    /// Load a graph that was already computed.
    #[must_use]
    pub fn from_rows(rows: &[(IdentityId, IdentityId, u32, String)]) -> Self {
        let mut graph = Self::default();
        for (left, right, frames, scenes) in rows {
            let (a, b) = if left <= right {
                (*left, *right)
            } else {
                (*right, *left)
            };
            let parsed: BTreeMap<String, u32> = serde_json::from_str(scenes).unwrap_or_default();
            graph.edges.insert(
                (a, b),
                Edge {
                    frames: *frames,
                    scenes: parsed,
                },
            );
        }
        graph
    }

    /// Photographs one identity appears in.
    #[must_use]
    pub fn frames_of(&self, identity: IdentityId) -> u32 {
        self.frames.get(&identity).copied().unwrap_or(0)
    }

    /// The edge between two identities, if they ever shared a frame.
    #[must_use]
    pub fn edge(&self, a: IdentityId, b: IdentityId) -> Option<&Edge> {
        let key = if a <= b { (a, b) } else { (b, a) };
        self.edges.get(&key)
    }

    /// Everybody one identity was photographed with, most often first.
    #[must_use]
    pub fn companions(&self, identity: IdentityId) -> Vec<(IdentityId, u32)> {
        let mut out: Vec<(IdentityId, u32)> = self
            .edges
            .iter()
            .filter_map(|((left, right), edge)| {
                if *left == identity {
                    Some((*right, edge.frames))
                } else if *right == identity {
                    Some((*left, edge.frames))
                } else {
                    None
                }
            })
            .collect();
        // Frames descending, then id, so the People panel's "seen with" list is stable.
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// Scene counts for one identity.
    #[must_use]
    pub fn scenes_of(&self, identity: IdentityId) -> BTreeMap<String, u32> {
        self.scenes.get(&identity).cloned().unwrap_or_default()
    }

    /// Edges, for the `cooccurrence` table.
    #[must_use]
    pub fn to_rows(&self) -> Vec<(IdentityId, IdentityId, u32, String)> {
        self.edges
            .iter()
            .map(|((left, right), edge)| (*left, *right, edge.frames, edge.scenes_json()))
            .collect()
    }

    /// Edges, in the shape role inference wants.
    ///
    /// `handles` maps an identity to the dense index the role module uses. An identity
    /// with no handle is skipped rather than given one, because a handle invented here
    /// would not match the one in the identity evidence and the pair would refer to
    /// somebody else.
    #[must_use]
    pub fn pair_evidence(&self, handles: &BTreeMap<IdentityId, usize>) -> Vec<PairEvidence> {
        let mut out = Vec::with_capacity(self.edges.len());
        for ((left, right), edge) in &self.edges {
            let (Some(a), Some(b)) = (handles.get(left), handles.get(right)) else {
                continue;
            };
            out.push(PairEvidence {
                a: *a,
                b: *b,
                frames: edge.frames,
                scene_frames: edge.scenes.clone(),
            });
        }
        out
    }

    /// How many edges the graph holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// True when nobody was ever photographed with anybody.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }
}

/// Per-identity evidence for role inference, from the faces and the graph.
///
/// The dense handles are the identities' positions in the returned slice, which is
/// ordered by identity id - so the handles are reproducible and
/// [`CooccurrenceGraph::pair_evidence`] can be built against them.
#[must_use]
pub fn identity_evidence(
    identities: &[crate::store::IdentityRow],
    faces: &[FaceRow],
    graph: &CooccurrenceGraph,
    timestamps: &BTreeMap<PhotoId, i64>,
) -> (Vec<IdentityEvidence>, BTreeMap<IdentityId, usize>) {
    let mut ordered: Vec<&crate::store::IdentityRow> = identities.iter().collect();
    ordered.sort_by_key(|identity| identity.identity_id);

    let mut handles = BTreeMap::new();
    let mut out = Vec::with_capacity(ordered.len());

    for (index, identity) in ordered.iter().enumerate() {
        handles.insert(identity.identity_id, index);

        let members: Vec<&FaceRow> = faces
            .iter()
            .filter(|face| face.identity_id == Some(identity.identity_id))
            .collect();
        let count = members.len().max(1) as f32;

        let mean_area = members.iter().map(|face| face.area_frac).sum::<f32>() / count;
        let mean_centrality = members.iter().map(|face| face.centrality).sum::<f32>() / count;
        // "Near the frame edge" is a centrality below a third, which is roughly the
        // outer quarter of the frame - where a vendor stands and a subject does not.
        let edge_fraction =
            members.iter().filter(|face| face.centrality < 0.35).count() as f32 / count;
        let child_score = members.iter().map(|face| face.child_score).sum::<f32>() / count;

        let mut first_seen_ms = None;
        let mut last_seen_ms = None;
        for face in &members {
            if let Some(at) = timestamps.get(&face.image_id) {
                first_seen_ms = Some(first_seen_ms.map_or(*at, |current: i64| current.min(*at)));
                last_seen_ms = Some(last_seen_ms.map_or(*at, |current: i64| current.max(*at)));
            }
        }

        out.push(IdentityEvidence {
            index,
            frames: graph.frames_of(identity.identity_id).max(
                u32::try_from(
                    members
                        .iter()
                        .map(|face| face.image_id)
                        .collect::<BTreeSet<_>>()
                        .len(),
                )
                .unwrap_or(0),
            ),
            scene_frames: graph.scenes_of(identity.identity_id),
            mean_area,
            mean_centrality,
            edge_fraction,
            child_score,
            first_seen_ms,
            last_seen_ms,
            user_locked: identity.user_locked,
            locked_role: identity.role,
        });
    }

    (out, handles)
}
