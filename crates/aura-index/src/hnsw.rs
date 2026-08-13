//! The graph. `M = 32`, `ef_construction = 200`, `ef_search = 64`, cosine
//! distance on L2-normalised half-precision vectors, exactly as section 6.3 of
//! the phase document specifies.
//!
//! Two things here are not textbook HNSW, and both exist to serve invariant 4.
//!
//! **Levels come from the id, not from a generator.** A textbook implementation
//! draws each node's level from an exponential distribution using a random number
//! generator, which means two runs of the same import build two different graphs
//! and every clustering decision downstream becomes irreproducible.
//! [`level_for`] takes the first eight bytes of `blake3(image_id)` instead: the
//! same distribution, and the same answer on every machine forever.
//!
//! **The parallel build is batched, not concurrent.** Section 6.3 asks for a
//! parallel build, and a lock-per-node concurrent insert would make the result
//! depend on thread scheduling. Instead the nodes are inserted in geometrically
//! growing chunks: every member of a chunk searches the same frozen graph, in
//! parallel, and then the chunk's links are committed in order. Two machines with
//! different core counts produce byte-identical graphs, and the searching - which
//! is all of the cost - still uses every core.

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use aura_core::clock::Clock;
use parking_lot::RwLock;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use crate::contract::index::{
    cosine_distance, CameraId, ImageEmbedding, ImageId, IndexFilter, IndexStats, SceneId,
    SimilarityIndex,
};

/// How many vectors this crate is documented to hold in memory.
///
/// Section 12 of the phase document asks for a documented ceiling rather than a
/// silent slide into swap. At 512 halves plus a 32-neighbour list per vector,
/// 20,000 images is about 25 MB of vectors and 10 MB of graph - comfortable. Past
/// it, [`crate::errors::index_too_large`] is raised and the caller falls back to
/// the flat scan in [`crate::metrics`], which is slower per query and holds no
/// graph at all.
pub const IN_MEMORY_CEILING: usize = 20_000;

/// Largest chunk the batched parallel build commits at once.
///
/// Small enough that a node almost always sees the neighbours committed just
/// before it, large enough that the parallel search phase has real work in it.
const MAX_BUILD_CHUNK: usize = 128;

/// Where a query starts when a filter has restricted the candidate set to
/// something small. Below this, an exact scan of the subset is both faster than
/// a graph descent and exactly correct.
const EXACT_SCAN_MAX: usize = 4_096;

/// The graph's shape. Frozen by section 6.3, not by an ADR: these are tuning
/// numbers, and a different set changes recall, not a caller's code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HnswParams {
    /// Neighbours per node above layer zero.
    pub m: usize,
    /// Neighbours per node at layer zero, where connectivity matters most.
    pub m0: usize,
    /// Candidate list size while building.
    pub ef_construction: usize,
    /// Candidate list size while querying.
    pub ef_search: usize,
}

impl Default for HnswParams {
    /// Section 6.3: `M = 32`, `ef_construction = 200`, `ef_search = 64`.
    fn default() -> Self {
        Self {
            m: 32,
            m0: 64,
            ef_construction: 200,
            ef_search: 64,
        }
    }
}

impl HnswParams {
    /// Neighbour ceiling at one layer.
    #[must_use]
    pub const fn max_neighbours(&self, level: usize) -> usize {
        if level == 0 {
            self.m0
        } else {
            self.m
        }
    }
}

/// What a filtered query needs to know about a frame, beyond its vector.
///
/// `timeline_ms` is the clock-aligned capture time phase 01 computes, in
/// milliseconds since the Unix epoch. `None` means the frame has no usable
/// capture time; such a frame is returned by unfiltered queries and never by a
/// time-windowed one, because "within 5 seconds of an unknown time" has no
/// truthful answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntryMeta {
    /// Clock-aligned capture time in milliseconds since the epoch.
    pub timeline_ms: Option<i64>,
    /// Which body shot it.
    pub camera: Option<CameraId>,
    /// Which scene it belongs to. Assigned by phase 07.
    pub scene: Option<SceneId>,
}

impl EntryMeta {
    /// True when a filtered query can see a frame with this metadata.
    #[must_use]
    pub const fn is_filterable(&self) -> bool {
        self.timeline_ms.is_some() || self.camera.is_some()
    }
}

/// One thing to put in the index: a vector and the metadata filters need.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexEntry {
    /// The vector and its descriptors.
    pub embedding: ImageEmbedding,
    /// Timeline stamp, camera and scene.
    pub meta: EntryMeta,
}

impl IndexEntry {
    /// An entry with no metadata: visible to unfiltered queries only.
    #[must_use]
    pub fn bare(embedding: ImageEmbedding) -> Self {
        Self {
            embedding,
            meta: EntryMeta::default(),
        }
    }
}

/// One node in the graph.
#[derive(Debug, Clone)]
struct Node {
    id: ImageId,
    /// Widened once at insert. Every distance in the crate reads this, so paying
    /// 2 KB per image to avoid 512 conversions per comparison is the right trade.
    vector: Vec<f32>,
    dhash: u64,
    model_ver: u16,
    meta: EntryMeta,
    /// Neighbours per level, level zero first. Each list is sorted by node index
    /// so that a snapshot's bytes do not depend on the order the builder visited.
    neighbours: Vec<Vec<u32>>,
}

/// A candidate in a search, ordered by distance and then by data already in the
/// row, so that no two candidates are ever equal and heap order is total.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Ranked {
    dist: f32,
    /// `timeline_ms` with `i64::MAX` for "unknown", then the id as a number.
    tie: (i64, u128),
    node: u32,
}

impl Eq for Ranked {}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> Ordering {
        self.dist
            .total_cmp(&other.dist)
            .then(self.tie.cmp(&other.tie))
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The neighbour lists a new node wants, per level, before they are committed.
#[derive(Debug, Clone, Default)]
struct InsertPlan {
    /// Level zero first.
    per_level: Vec<Vec<u32>>,
}

/// The graph and everything derived from it.
#[derive(Debug, Default)]
struct Inner {
    nodes: Vec<Node>,
    by_id: BTreeMap<ImageId, u32>,
    entry: Option<u32>,
    top_level: usize,
    /// `(timeline_ms, node)` sorted ascending. The pre-filter section 6.3 asks
    /// for: a time-windowed query binary-searches this rather than filtering
    /// results afterwards.
    timeline: Vec<(i64, u32)>,
    /// Catalog camera ids, indexed by the dense handle. Sorted, so the handle a
    /// body gets does not depend on the order the rows came back.
    camera_labels: Vec<String>,
    build_ms: u32,
    from_snapshot: bool,
}

/// The similarity index.
///
/// Cheap to clone by reference, safe to share: every query takes a read lock and
/// every insert takes a write lock, so a 4,000-image background embed can insert
/// while the grid asks what looks like the frame under the cursor.
#[derive(Debug)]
pub struct HnswIndex {
    params: HnswParams,
    inner: RwLock<Inner>,
}

impl Default for HnswIndex {
    fn default() -> Self {
        Self::new(HnswParams::default())
    }
}

impl HnswIndex {
    /// An empty index with the given shape.
    #[must_use]
    pub fn new(params: HnswParams) -> Self {
        Self {
            params,
            inner: RwLock::new(Inner::default()),
        }
    }

    /// The shape this index was built with.
    #[must_use]
    pub const fn params(&self) -> HnswParams {
        self.params
    }

    /// Build an index from every entry in a project.
    ///
    /// Entries are sorted by `(timeline_ms, image_id)` before insertion, so the
    /// graph does not depend on the order the catalog happened to return rows.
    /// The build itself is the batched parallel scheme described in this module's
    /// header.
    #[must_use]
    pub fn build(entries: Vec<IndexEntry>, params: HnswParams, clock: &dyn Clock) -> Self {
        let started = clock.monotonic_ms();
        let index = Self::new(params);
        index.extend(entries);
        let mut inner = index.inner.write();
        inner.build_ms = clock.monotonic_ms().saturating_sub(started) as u32;
        inner.from_snapshot = false;
        drop(inner);
        index
    }

    /// Insert many entries, in geometrically growing parallel chunks.
    ///
    /// Used by [`HnswIndex::build`] and by the incremental path when a second
    /// card is imported: inserting 300 new frames into a 4,000-frame index takes
    /// the same code, and acceptance criterion 5 - "importing another card embeds
    /// only the new files" - is the store's job, not this one's.
    pub fn extend(&self, mut entries: Vec<IndexEntry>) {
        entries.sort_by(|left, right| {
            order_key(&left.meta, &left.embedding.image_id)
                .cmp(&order_key(&right.meta, &right.embedding.image_id))
        });
        entries.retain(|entry| !self.contains(&entry.embedding.image_id));

        let mut done = 0usize;
        let mut chunk = 1usize;
        while done < entries.len() {
            let take = chunk.min(entries.len() - done);
            let Some(slice) = entries.get(done..done + take) else {
                break;
            };
            let plans: Vec<InsertPlan> = {
                let inner = self.inner.read();
                if inner.nodes.is_empty() {
                    vec![InsertPlan::default(); slice.len()]
                } else {
                    slice
                        .par_iter()
                        .map(|entry| self.plan_for(&inner, entry))
                        .collect()
                }
            };
            {
                let mut inner = self.inner.write();
                for (entry, plan) in slice.iter().zip(plans) {
                    self.commit(&mut inner, entry, plan);
                }
            }
            done += take;
            chunk = (chunk * 2).min(MAX_BUILD_CHUNK);
        }
    }

    /// Insert one entry with the metadata a filtered query needs.
    ///
    /// The method the ingest path uses. [`SimilarityIndex::insert`] is the frozen
    /// signature and cannot carry metadata; see ADR-0011 section 2.
    pub fn insert_entry(&self, entry: IndexEntry) {
        self.extend(vec![entry]);
    }

    /// True when this id already has a vector in the graph.
    #[must_use]
    pub fn contains(&self, id: &ImageId) -> bool {
        self.inner.read().by_id.contains_key(id)
    }

    /// Vectors in the graph.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.read().nodes.len()
    }

    /// True when nothing has been indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The dense handle for a catalog camera id, if this index has seen it.
    #[must_use]
    pub fn camera_handle(&self, catalog_camera_id: &str) -> Option<CameraId> {
        let inner = self.inner.read();
        inner
            .camera_labels
            .iter()
            .position(|label| label == catalog_camera_id)
            .map(|position| CameraId(position as u32))
    }

    /// The catalog camera id behind a dense handle.
    #[must_use]
    pub fn camera_label(&self, camera: CameraId) -> Option<String> {
        let inner = self.inner.read();
        inner.camera_labels.get(camera.0 as usize).cloned()
    }

    /// Teach the index the catalog's camera ids, so handles can be resolved by
    /// name. Sorted and de-duplicated, so a handle is stable across runs.
    pub fn set_camera_labels(&self, labels: &[String]) {
        let mut sorted: Vec<String> = labels.to_vec();
        sorted.sort();
        sorted.dedup();
        self.inner.write().camera_labels = sorted;
    }

    /// The difference hash stored for an id.
    #[must_use]
    pub fn dhash_of(&self, id: &ImageId) -> Option<u64> {
        let inner = self.inner.read();
        let index = inner.by_id.get(id)?;
        inner.nodes.get(*index as usize).map(|node| node.dhash)
    }

    /// The metadata stored for an id.
    #[must_use]
    pub fn meta_of(&self, id: &ImageId) -> Option<EntryMeta> {
        let inner = self.inner.read();
        let index = inner.by_id.get(id)?;
        inner.nodes.get(*index as usize).map(|node| node.meta)
    }

    /// Milliseconds the last full build took.
    #[must_use]
    pub fn build_ms(&self) -> u32 {
        self.inner.read().build_ms
    }

    /// Every id whose difference hash is within `max_hamming` of `dhash`.
    ///
    /// The cheap question, answered without touching the graph: 4,000 exclusive
    /// ors and pop counts is microseconds, and it is what keeps the expensive
    /// index from being asked whether two identical frames are similar.
    #[must_use]
    pub fn dhash_within(&self, dhash: u64, max_hamming: u32) -> Vec<(ImageId, u32)> {
        let inner = self.inner.read();
        let mut out: Vec<(ImageId, u32)> = inner
            .nodes
            .iter()
            .map(|node| (node.id, (node.dhash ^ dhash).count_ones()))
            .filter(|(_, distance)| *distance <= max_hamming)
            .collect();
        out.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
        out
    }

    // -- construction ------------------------------------------------------

    /// Search the frozen graph for one new node's neighbour lists.
    fn plan_for(&self, inner: &Inner, entry: &IndexEntry) -> InsertPlan {
        let vector = entry.embedding.to_f32();
        let level = level_for(&entry.embedding.image_id, self.params.m);

        let Some(graph_entry) = inner.entry else {
            return InsertPlan::default();
        };

        let mut current = vec![graph_entry];
        let mut descend = inner.top_level;
        while descend > level {
            current = inner.greedy_step(&vector, &current, descend);
            descend -= 1;
        }

        let mut per_level = vec![Vec::new(); level + 1];
        let ceiling = level.min(inner.top_level);
        for layer in (0..=ceiling).rev() {
            let found = inner.search_layer(
                &vector,
                &current,
                self.params.ef_construction,
                layer,
                |_| true,
            );
            current = found.iter().map(|ranked| ranked.node).collect();
            let keep = self.params.max_neighbours(layer);
            if let Some(slot) = per_level.get_mut(layer) {
                *slot = found
                    .iter()
                    .take(keep)
                    .map(|ranked| ranked.node)
                    .collect::<Vec<u32>>();
                slot.sort_unstable();
            }
        }
        InsertPlan { per_level }
    }

    /// Add the node and make every link bidirectional, shrinking any neighbour
    /// list that has outgrown its level's ceiling.
    fn commit(&self, inner: &mut Inner, entry: &IndexEntry, plan: InsertPlan) {
        let id = entry.embedding.image_id;
        let level = level_for(&id, self.params.m);
        let index = inner.nodes.len() as u32;

        let mut neighbours = plan.per_level;
        neighbours.resize(level + 1, Vec::new());

        inner.nodes.push(Node {
            id,
            vector: entry.embedding.to_f32(),
            dhash: entry.embedding.dhash,
            model_ver: entry.embedding.model_ver,
            meta: entry.meta,
            neighbours,
        });
        inner.by_id.insert(id, index);

        if let Some(stamp) = entry.meta.timeline_ms {
            let position = inner
                .timeline
                .partition_point(|(at, node)| (*at, *node) < (stamp, index));
            inner.timeline.insert(position, (stamp, index));
        }

        // Back-links. Collected first so the borrow of `nodes` ends before the
        // shrink pass, which needs to read other nodes' vectors.
        let back: Vec<(u32, usize)> = inner
            .nodes
            .get(index as usize)
            .map(|node| {
                node.neighbours
                    .iter()
                    .enumerate()
                    .flat_map(|(layer, list)| list.iter().map(move |other| (*other, layer)))
                    .collect()
            })
            .unwrap_or_default();

        for (other, layer) in back {
            let ceiling = self.params.max_neighbours(layer);
            let overflowed = {
                let Some(node) = inner.nodes.get_mut(other as usize) else {
                    continue;
                };
                let Some(list) = node.neighbours.get_mut(layer) else {
                    continue;
                };
                if list.contains(&index) {
                    continue;
                }
                let position = list.partition_point(|existing| *existing < index);
                list.insert(position, index);
                list.len() > ceiling
            };
            if overflowed {
                inner.shrink(other, layer, ceiling);
            }
        }

        if level > inner.top_level || inner.entry.is_none() {
            inner.top_level = level;
            inner.entry = Some(index);
        }
    }
}

/// Sort key that makes insertion order independent of the catalog's row order.
fn order_key(meta: &EntryMeta, id: &ImageId) -> (i64, u128) {
    (meta.timeline_ms.unwrap_or(i64::MAX), id.as_uuid().as_u128())
}

/// The level a node lives at, derived from its id.
///
/// The textbook formula is `floor(-ln(u) * mL)` with `mL = 1 / ln(M)` and `u`
/// uniform in `(0, 1]`. `u` here comes from the first eight bytes of
/// `blake3(image_id)`, which is uniform enough for a level assignment and,
/// unlike a generator, is the same on every machine and in any insertion order.
#[must_use]
pub fn level_for(id: &ImageId, m: usize) -> usize {
    let digest = blake3::hash(id.to_db().as_bytes());
    let bytes = digest.as_bytes();
    let mut head = [0u8; 8];
    head.copy_from_slice(bytes.get(..8).unwrap_or(&[0; 8]));
    let raw = u64::from_le_bytes(head);
    // (0, 1]: never zero, so the logarithm is always defined.
    let unit = (f64::from(u32::try_from(raw >> 32).unwrap_or(0)) + 1.0) / f64::from(u32::MAX);
    let scale = 1.0 / (m.max(2) as f64).ln();
    let level = (-unit.ln() * scale).floor();
    if level.is_finite() && level > 0.0 {
        (level as usize).min(16)
    } else {
        0
    }
}

impl Inner {
    /// One greedy hop at an upper layer: move to the best neighbour until none is
    /// better. Used while descending to the insertion level.
    fn greedy_step(&self, query: &[f32], from: &[u32], layer: usize) -> Vec<u32> {
        let Some(mut best) = from.first().and_then(|node| self.ranked(query, *node)) else {
            return from.to_vec();
        };
        for candidate in from.iter().skip(1) {
            if let Some(ranked) = self.ranked(query, *candidate) {
                if ranked < best {
                    best = ranked;
                }
            }
        }
        let mut improved = true;
        while improved {
            improved = false;
            for candidate in self.neighbours_of(best.node, layer) {
                if let Some(ranked) = self.ranked(query, *candidate) {
                    if ranked < best {
                        best = ranked;
                        improved = true;
                    }
                }
            }
        }
        vec![best.node]
    }

    /// The classic layer search: expand the nearest unvisited candidate until no
    /// candidate can improve on the worst of the `ef` results.
    fn search_layer(
        &self,
        query: &[f32],
        entry_points: &[u32],
        ef: usize,
        layer: usize,
        allow: impl Fn(u32) -> bool,
    ) -> Vec<Ranked> {
        let mut visited: BTreeSet<u32> = BTreeSet::new();
        let mut candidates: BinaryHeap<Reverse<Ranked>> = BinaryHeap::new();
        let mut results: BinaryHeap<Ranked> = BinaryHeap::new();

        for start in entry_points {
            if !visited.insert(*start) {
                continue;
            }
            let Some(ranked) = self.ranked(query, *start) else {
                continue;
            };
            candidates.push(Reverse(ranked));
            if allow(*start) {
                results.push(ranked);
            }
        }
        while results.len() > ef {
            results.pop();
        }

        while let Some(Reverse(current)) = candidates.pop() {
            if results.len() >= ef {
                if let Some(worst) = results.peek() {
                    if current.dist > worst.dist {
                        break;
                    }
                }
            }
            for candidate in self.neighbours_of(current.node, layer) {
                let candidate = *candidate;
                if !visited.insert(candidate) {
                    continue;
                }
                let Some(ranked) = self.ranked(query, candidate) else {
                    continue;
                };
                let room = results.len() < ef;
                let better = results.peek().is_some_and(|worst| ranked.dist < worst.dist);
                if room || better {
                    candidates.push(Reverse(ranked));
                    if allow(candidate) {
                        results.push(ranked);
                        while results.len() > ef {
                            results.pop();
                        }
                    }
                }
            }
        }

        let mut out = results.into_sorted_vec();
        out.dedup_by_key(|ranked| ranked.node);
        out
    }

    /// One node's neighbours at one layer, borrowed rather than copied.
    ///
    /// A layer-zero expansion reads 64 neighbours, and a 4,000-vector build performs
    /// something like a million expansions. Cloning each list into a fresh `Vec` was
    /// a million allocations, which is measurable and entirely avoidable: the search
    /// state is all local, so nothing conflicts with an immutable borrow of the
    /// graph.
    fn neighbours_of(&self, node: u32, layer: usize) -> &[u32] {
        self.nodes
            .get(node as usize)
            .and_then(|entry| entry.neighbours.get(layer))
            .map_or(&[][..], Vec::as_slice)
    }

    /// Distance from a query to one node, with its tie-break key attached.
    fn ranked(&self, query: &[f32], node: u32) -> Option<Ranked> {
        let entry = self.nodes.get(node as usize)?;
        Some(Ranked {
            dist: cosine_distance(query, &entry.vector),
            tie: order_key(&entry.meta, &entry.id),
            node,
        })
    }

    /// Re-select the `ceiling` nearest neighbours of a node at one layer.
    fn shrink(&mut self, node: u32, layer: usize, ceiling: usize) {
        let Some(vector) = self.nodes.get(node as usize).map(|n| n.vector.clone()) else {
            return;
        };
        let list = self
            .nodes
            .get(node as usize)
            .and_then(|n| n.neighbours.get(layer))
            .cloned()
            .unwrap_or_default();
        let mut ranked: Vec<Ranked> = list
            .iter()
            .filter_map(|other| self.ranked(&vector, *other))
            .collect();
        ranked.sort();
        let mut kept: Vec<u32> = ranked
            .iter()
            .take(ceiling)
            .map(|entry| entry.node)
            .collect();
        kept.sort_unstable();
        if let Some(slot) = self
            .nodes
            .get_mut(node as usize)
            .and_then(|n| n.neighbours.get_mut(layer))
        {
            *slot = kept;
        }
    }

    /// Node indices whose timeline stamp is within `window_s` seconds of `at`.
    ///
    /// The pre-filter from section 6.3: two binary searches over a sorted array,
    /// not a scan and not a post-filter.
    fn timeline_window(&self, at: i64, window_s: u32) -> Vec<u32> {
        let span = i64::from(window_s).saturating_mul(1_000);
        let low = at.saturating_sub(span);
        let high = at.saturating_add(span);
        let start = self.timeline.partition_point(|(stamp, _)| *stamp < low);
        let end = self.timeline.partition_point(|(stamp, _)| *stamp <= high);
        self.timeline
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .map(|(_, node)| *node)
            .collect()
    }

    /// True when a node passes everything the filter asks of it.
    fn passes(&self, node: u32, filter: &IndexFilter<'_>, exclude: &BTreeSet<ImageId>) -> bool {
        let Some(entry) = self.nodes.get(node as usize) else {
            return false;
        };
        if exclude.contains(&entry.id) {
            return false;
        }
        if filter.needs_metadata() && !entry.meta.is_filterable() {
            return false;
        }
        if let Some(camera) = filter.camera_id {
            if entry.meta.camera != Some(camera) {
                return false;
            }
        }
        if let Some(scene) = filter.scene {
            if entry.meta.scene != Some(scene) {
                return false;
            }
        }
        true
    }
}

impl HnswIndex {
    /// The shared implementation behind `knn` and `radius`.
    ///
    /// Three strategies, chosen by what the filter restricts to:
    ///
    /// 1. A time window pre-filters to a candidate list, which is scanned
    ///    exactly. A five-second window on a 4,000-image wedding is a few dozen
    ///    frames, so this is both faster than a graph descent and exact - which
    ///    is what section 10.1 means by "returns only in-window frames".
    /// 2. Any other filter that leaves few enough candidates is scanned exactly.
    /// 3. Otherwise the graph is searched with the filter applied during the
    ///    search rather than after it, so `k` results means `k` results.
    fn search(
        &self,
        query: &ImageEmbedding,
        want: usize,
        max_dist: Option<f32>,
        filter: &IndexFilter<'_>,
    ) -> Vec<(ImageId, f32)> {
        let vector = query.to_f32();
        let exclude: BTreeSet<ImageId> = filter
            .exclude
            .unwrap_or_default()
            .iter()
            .copied()
            .chain(std::iter::once(query.image_id))
            .collect();
        let inner = self.inner.read();
        if inner.nodes.is_empty() || want == 0 {
            return Vec::new();
        }

        let windowed = filter.time_window_s.and_then(|window| {
            let at = inner
                .by_id
                .get(&query.image_id)
                .and_then(|node| inner.nodes.get(*node as usize))
                .and_then(|node| node.meta.timeline_ms)?;
            Some(inner.timeline_window(at, window))
        });

        let candidates: Option<Vec<u32>> = match (filter.time_window_s, windowed) {
            // A window was asked for and the query frame has no time: no frame is
            // truthfully within it.
            (Some(_), None) => return Vec::new(),
            (Some(_), Some(list)) => Some(list),
            (None, _) => {
                if filter.needs_metadata() && inner.nodes.len() <= EXACT_SCAN_MAX {
                    Some((0..inner.nodes.len() as u32).collect())
                } else {
                    None
                }
            }
        };

        let mut ranked: Vec<Ranked> = if let Some(list) = candidates {
            list.iter()
                .filter(|node| inner.passes(**node, filter, &exclude))
                .filter_map(|node| inner.ranked(&vector, *node))
                .collect()
        } else {
            let Some(entry) = inner.entry else {
                return Vec::new();
            };
            let mut current = vec![entry];
            let mut layer = inner.top_level;
            while layer > 0 {
                current = inner.greedy_step(&vector, &current, layer);
                layer -= 1;
            }
            let ef = self.params.ef_search.max(want);
            inner.search_layer(&vector, &current, ef, 0, |node| {
                inner.passes(node, filter, &exclude)
            })
        };

        ranked.sort();
        ranked
            .into_iter()
            .filter(|entry| max_dist.is_none_or(|ceiling| entry.dist <= ceiling))
            .take(want)
            .filter_map(|entry| {
                inner
                    .nodes
                    .get(entry.node as usize)
                    .map(|node| (node.id, entry.dist))
            })
            .collect()
    }
}

impl SimilarityIndex for HnswIndex {
    fn knn(&self, q: &ImageEmbedding, k: usize, filter: IndexFilter<'_>) -> Vec<(ImageId, f32)> {
        self.search(q, k, None, &filter)
    }

    fn radius(
        &self,
        q: &ImageEmbedding,
        max_dist: f32,
        filter: IndexFilter<'_>,
    ) -> Vec<(ImageId, f32)> {
        self.search(q, usize::MAX, Some(max_dist), &filter)
    }

    fn medoid(&self, ids: &[ImageId]) -> Option<ImageId> {
        let inner = self.inner.read();
        let members: Vec<&Node> = ids
            .iter()
            .filter_map(|id| inner.by_id.get(id))
            .filter_map(|node| inner.nodes.get(*node as usize))
            .collect();
        if members.is_empty() {
            return None;
        }
        let mut best: Option<(f32, (i64, u128), ImageId)> = None;
        for candidate in &members {
            let mut total = 0.0f32;
            for other in &members {
                total += cosine_distance(&candidate.vector, &other.vector);
            }
            let key = (
                total,
                order_key(&candidate.meta, &candidate.id),
                candidate.id,
            );
            let replace = match &best {
                None => true,
                Some(current) => {
                    key.0.total_cmp(&current.0).then(key.1.cmp(&current.1)) == Ordering::Less
                }
            };
            if replace {
                best = Some(key);
            }
        }
        best.map(|(_, _, id)| id)
    }

    fn insert(&self, e: &ImageEmbedding) {
        let meta = self.meta_of(&e.image_id).unwrap_or_default();
        self.insert_entry(IndexEntry {
            embedding: e.clone(),
            meta,
        });
    }

    fn stats(&self) -> IndexStats {
        let inner = self.inner.read();
        let filterable = inner
            .nodes
            .iter()
            .filter(|node| node.meta.is_filterable())
            .count();
        let degenerate = inner
            .nodes
            .iter()
            .filter(|node| node.vector.iter().all(|value| value.abs() < f32::EPSILON))
            .count();
        let model_ver = inner
            .nodes
            .iter()
            .map(|node| node.model_ver)
            .min()
            .unwrap_or(0);
        let stale = inner
            .nodes
            .iter()
            .filter(|node| node.model_ver != model_ver)
            .count();
        IndexStats {
            vectors: inner.nodes.len(),
            filterable,
            unfiltered: inner.nodes.len() - filterable,
            degenerate,
            model_ver,
            stale,
            build_ms: inner.build_ms,
            from_snapshot: inner.from_snapshot,
        }
    }
}

/// Everything the snapshot writer needs, and nothing it does not.
pub(crate) mod raw {
    use super::{EntryMeta, HnswIndex, ImageId, Node};
    use crate::contract::index::EMBED_DIM;

    /// One node, flattened for serialisation.
    #[derive(Debug, Clone)]
    pub(crate) struct RawNode {
        pub(crate) id: ImageId,
        pub(crate) vector: Vec<f32>,
        pub(crate) dhash: u64,
        pub(crate) model_ver: u16,
        pub(crate) meta: EntryMeta,
        pub(crate) neighbours: Vec<Vec<u32>>,
    }

    impl HnswIndex {
        /// Copy the graph out for the snapshot writer.
        pub(crate) fn to_raw(&self) -> (Vec<RawNode>, Vec<String>, Option<u32>, usize) {
            let inner = self.inner.read();
            let nodes = inner
                .nodes
                .iter()
                .map(|node| RawNode {
                    id: node.id,
                    vector: node.vector.clone(),
                    dhash: node.dhash,
                    model_ver: node.model_ver,
                    meta: node.meta,
                    neighbours: node.neighbours.clone(),
                })
                .collect();
            (
                nodes,
                inner.camera_labels.clone(),
                inner.entry,
                inner.top_level,
            )
        }

        /// Rebuild the graph from a snapshot, without searching anything.
        pub(crate) fn from_raw(
            params: super::HnswParams,
            nodes: Vec<RawNode>,
            camera_labels: Vec<String>,
            entry: Option<u32>,
            top_level: usize,
        ) -> Self {
            let index = Self::new(params);
            {
                let mut inner = index.inner.write();
                for (position, node) in nodes.into_iter().enumerate() {
                    if node.vector.len() != EMBED_DIM {
                        continue;
                    }
                    let slot = position as u32;
                    inner.by_id.insert(node.id, slot);
                    if let Some(stamp) = node.meta.timeline_ms {
                        inner.timeline.push((stamp, slot));
                    }
                    inner.nodes.push(Node {
                        id: node.id,
                        vector: node.vector,
                        dhash: node.dhash,
                        model_ver: node.model_ver,
                        meta: node.meta,
                        neighbours: node.neighbours,
                    });
                }
                inner.timeline.sort_unstable();
                inner.camera_labels = camera_labels;
                inner.entry = entry.filter(|slot| (*slot as usize) < inner.nodes.len());
                inner.top_level = top_level;
                inner.from_snapshot = true;
            }
            index
        }
    }
}
