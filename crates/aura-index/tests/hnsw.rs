//! The graph: recall against brute force, determinism, filters, medoids.
//!
//! An approximate index is worth nothing without a measurement of how
//! approximate it is, so the first test here compares the graph against the flat
//! scan it approximates. The rest are the properties later phases will rely on
//! without asking: that two runs agree, that a time window means a time window,
//! and that an exclusion set is honoured.

mod support;

use std::collections::BTreeSet;

use aura_core::clock::SystemClock;
use aura_index::contract::index::{cosine_distance, IndexFilter, SimilarityIndex, EMBED_DIM, F16};
use aura_index::hnsw::{level_for, EntryMeta, HnswIndex, HnswParams, IndexEntry};
use aura_index::metrics::{exact_knn, recall_at_k};
use aura_index::{query, CameraId};

use support::{clustered, embedding_from, embeddings_of, id_for};

fn clock() -> SystemClock {
    SystemClock::default()
}

fn built(clusters: usize, per_cluster: usize, spread: f32) -> (HnswIndex, Vec<IndexEntry>) {
    let (entries, _) = clustered(clusters, per_cluster, spread);
    let index = HnswIndex::build(entries.clone(), HnswParams::default(), &clock());
    (index, entries)
}

#[test]
fn the_graph_finds_what_the_flat_scan_finds() {
    let (index, entries) = built(40, 25, 0.02);
    let corpus = embeddings_of(&entries);
    assert_eq!(index.len(), 1_000);

    let mut total = 0.0f64;
    let mut queries = 0usize;
    for entry in entries.iter().step_by(37) {
        let approximate = index.knn(&entry.embedding, 10, IndexFilter::none());
        let exact = exact_knn(&entry.embedding, &corpus, 10);
        total += recall_at_k(&approximate, &exact);
        queries += 1;
    }
    let recall = total / queries as f64;
    assert!(
        recall >= 0.95,
        "recall@10 was {recall:.4}, the graph is not worth its memory below 0.95"
    );
}

#[test]
fn two_builds_of_the_same_corpus_answer_identically() {
    let (entries, _) = clustered(20, 15, 0.04);
    let first = HnswIndex::build(entries.clone(), HnswParams::default(), &clock());
    // Reversed insertion order: `extend` sorts by (timeline, id) before inserting,
    // so the graph must not depend on the order the rows arrived.
    let mut shuffled = entries.clone();
    shuffled.reverse();
    let second = HnswIndex::build(shuffled, HnswParams::default(), &clock());

    for entry in entries.iter().step_by(11) {
        let left = first.knn(&entry.embedding, 12, IndexFilter::none());
        let right = second.knn(&entry.embedding, 12, IndexFilter::none());
        assert_eq!(
            left, right,
            "two builds disagreed about the neighbours of {}",
            entry.embedding.image_id
        );
    }
}

#[test]
fn a_query_never_returns_itself() {
    let (index, entries) = built(10, 10, 0.03);
    for entry in &entries {
        let found = index.knn(&entry.embedding, 100, IndexFilter::none());
        assert!(
            !found.iter().any(|(id, _)| *id == entry.embedding.image_id),
            "the index returned the query frame as its own neighbour"
        );
    }
}

#[test]
fn neighbours_are_ordered_by_distance_then_by_data_in_the_row() {
    let (index, entries) = built(12, 12, 0.03);
    let Some(query) = entries.first() else {
        panic!("no entries");
    };
    let found = index.knn(&query.embedding, 30, IndexFilter::none());
    let mut previous = -1.0f32;
    for (_, distance) in &found {
        assert!(
            *distance >= previous - f32::EPSILON,
            "distances are not ascending: {distance} after {previous}"
        );
        previous = *distance;
    }
}

#[test]
fn a_time_window_returns_only_in_window_frames() {
    // Clusters are an hour apart, members a second apart, so a five-second window
    // can only contain members of the same cluster.
    let (index, entries) = built(8, 20, 0.03);
    let Some(query) = entries.get(40) else {
        panic!("no entry at 40");
    };
    let Some(at) = query.meta.timeline_ms else {
        panic!("the fixture has no timeline");
    };

    let found = index.knn(&query.embedding, 50, IndexFilter::within_seconds(5));
    assert!(!found.is_empty(), "a five-second window found nothing");
    for (id, _) in &found {
        let Some(meta) = index.meta_of(id) else {
            panic!("a returned frame has no metadata");
        };
        let Some(stamp) = meta.timeline_ms else {
            panic!("a returned frame has no timeline stamp");
        };
        assert!(
            (stamp - at).abs() <= 5_000,
            "{id} is {} ms away and the window was 5,000",
            (stamp - at).abs()
        );
    }
}

#[test]
fn a_time_window_on_a_frame_with_no_capture_time_returns_nothing() {
    // "Within five seconds of an unknown time" has no truthful answer, and an
    // empty result is the honest one. Returning everything would be worse: a burst
    // grouping stage would silently treat an undated frame as adjacent to all
    // 4,000 others.
    let (index, _) = built(6, 8, 0.03);
    let orphan = embedding_from(id_for(99_999), vec![0.2; EMBED_DIM], 7);
    index.insert_entry(IndexEntry {
        embedding: orphan.clone(),
        meta: EntryMeta::default(),
    });

    let found = index.knn(&orphan, 10, IndexFilter::within_seconds(5));
    assert!(found.is_empty(), "an undated frame matched a time window");

    // Unfiltered, it is a perfectly ordinary member of the index.
    let unfiltered = index.knn(&orphan, 10, IndexFilter::none());
    assert!(!unfiltered.is_empty());
}

#[test]
fn a_camera_filter_returns_only_that_body() {
    let (index, entries) = built(10, 16, 0.02);
    let Some(query) = entries.first() else {
        panic!("no entries");
    };
    let found = index.knn(
        &query.embedding,
        40,
        IndexFilter::none().with_camera(CameraId(1)),
    );
    assert!(!found.is_empty(), "a camera filter found nothing");
    for (id, _) in &found {
        assert_eq!(
            index.meta_of(id).and_then(|meta| meta.camera),
            Some(CameraId(1)),
            "{id} is not from camera 1"
        );
    }
}

#[test]
fn a_scene_filter_matches_nothing_until_phase_07_assigns_scenes() {
    // Deliberate: an unassigned scene must not silently degrade to "no filter".
    let (index, entries) = built(6, 8, 0.03);
    let Some(query) = entries.first() else {
        panic!("no entries");
    };
    let found = index.knn(
        &query.embedding,
        20,
        IndexFilter::none().with_scene(aura_index::SceneId(0)),
    );
    assert!(
        found.is_empty(),
        "a scene filter matched an unassigned frame"
    );
}

#[test]
fn an_exclusion_set_is_honoured() {
    let (index, entries) = built(10, 12, 0.03);
    let Some(query) = entries.first() else {
        panic!("no entries");
    };
    let first_pass = index.knn(&query.embedding, 5, IndexFilter::none());
    let excluded: Vec<_> = first_pass.iter().map(|(id, _)| *id).collect();

    let second_pass = index.knn(
        &query.embedding,
        5,
        IndexFilter::none().excluding(&excluded),
    );
    let seen: BTreeSet<_> = second_pass.iter().map(|(id, _)| *id).collect();
    for id in &excluded {
        assert!(!seen.contains(id), "{id} was excluded and came back anyway");
    }
    assert_eq!(second_pass.len(), 5, "the second pass should still fill k");
}

#[test]
fn radius_returns_everything_inside_it_and_nothing_outside() {
    let (index, entries) = built(20, 20, 0.03);
    let corpus = embeddings_of(&entries);
    let Some(query) = entries.get(7) else {
        panic!("no entry at 7");
    };
    let ceiling = 0.25f32;

    let found = index.radius(&query.embedding, ceiling, IndexFilter::none());
    for (_, distance) in &found {
        assert!(
            *distance <= ceiling,
            "radius returned {distance} > {ceiling}"
        );
    }

    // Everything the flat scan says is inside the radius should be found. The
    // graph is approximate, so this is a recall assertion rather than equality.
    let truth: Vec<_> = exact_knn(&query.embedding, &corpus, corpus.len())
        .into_iter()
        .filter(|(_, distance)| *distance <= ceiling)
        .collect();
    if !truth.is_empty() {
        let recall = recall_at_k(&found, &truth);
        assert!(recall >= 0.90, "radius recall was {recall:.3}");
    }
}

#[test]
fn the_medoid_is_the_frame_closest_to_all_the_others() {
    let (index, entries) = built(4, 12, 0.03);
    let members: Vec<_> = entries
        .iter()
        .take(12)
        .map(|entry| entry.embedding.image_id)
        .collect();
    let Some(medoid) = index.medoid(&members) else {
        panic!("no medoid for a non-empty set");
    };

    // Recompute the answer the slow, obvious way.
    let vectors: Vec<_> = entries
        .iter()
        .take(12)
        .map(|entry| (entry.embedding.image_id, entry.embedding.to_f32()))
        .collect();
    let mut best = (f32::MAX, None);
    for (id, vector) in &vectors {
        let total: f32 = vectors
            .iter()
            .map(|(_, other)| cosine_distance(vector, other))
            .sum();
        if total < best.0 {
            best = (total, Some(*id));
        }
    }
    assert_eq!(Some(medoid), best.1);
}

#[test]
fn the_medoid_of_an_empty_or_unknown_set_is_none() {
    let (index, _) = built(3, 4, 0.03);
    assert_eq!(index.medoid(&[]), None);
    assert_eq!(index.medoid(&[id_for(1_234_567)]), None);
}

#[test]
fn an_empty_index_answers_every_query_with_nothing() {
    let index = HnswIndex::new(HnswParams::default());
    let query = embedding_from(id_for(1), vec![0.5; EMBED_DIM], 0);
    assert!(index.is_empty());
    assert!(index.knn(&query, 10, IndexFilter::none()).is_empty());
    assert!(index.radius(&query, 1.0, IndexFilter::none()).is_empty());
    assert_eq!(index.stats().vectors, 0);
}

#[test]
fn incremental_insert_keeps_recall_within_one_percent_of_a_full_rebuild() {
    // Acceptance criterion 5, and section 10.1's "incremental insert after a second
    // card import keeps recall within 1 % of a full rebuild".
    let (entries, _) = clustered(30, 20, 0.02);
    let split = 450;
    let Some(first_card) = entries.get(..split) else {
        panic!("short corpus");
    };
    let Some(second_card) = entries.get(split..) else {
        panic!("short corpus");
    };

    let incremental = HnswIndex::build(first_card.to_vec(), HnswParams::default(), &clock());
    incremental.extend(second_card.to_vec());
    let rebuilt = HnswIndex::build(entries.clone(), HnswParams::default(), &clock());
    assert_eq!(incremental.len(), rebuilt.len());

    let corpus = embeddings_of(&entries);
    let mut incremental_recall = 0.0f64;
    let mut rebuilt_recall = 0.0f64;
    let mut queries = 0usize;
    for entry in entries.iter().step_by(23) {
        let exact = exact_knn(&entry.embedding, &corpus, 10);
        incremental_recall += recall_at_k(
            &incremental.knn(&entry.embedding, 10, IndexFilter::none()),
            &exact,
        );
        rebuilt_recall += recall_at_k(
            &rebuilt.knn(&entry.embedding, 10, IndexFilter::none()),
            &exact,
        );
        queries += 1;
    }
    incremental_recall /= queries as f64;
    rebuilt_recall /= queries as f64;

    assert!(
        incremental_recall >= rebuilt_recall - 0.01,
        "incremental recall {incremental_recall:.4} is more than one point below the \
         rebuild's {rebuilt_recall:.4}"
    );
}

#[test]
fn inserting_a_frame_twice_does_not_duplicate_it() {
    let (index, entries) = built(3, 5, 0.03);
    let before = index.len();
    let Some(entry) = entries.first() else {
        panic!("no entries");
    };
    index.insert_entry(entry.clone());
    index.insert(&entry.embedding);
    assert_eq!(index.len(), before, "a re-insert grew the graph");
}

#[test]
fn the_frozen_insert_keeps_whatever_metadata_the_index_already_knew() {
    // `SimilarityIndex::insert` cannot carry metadata; ADR-0011 section 2 says it
    // reuses what the index already has for that id rather than erasing it.
    let (index, entries) = built(4, 6, 0.03);
    let Some(entry) = entries.first() else {
        panic!("no entries");
    };
    let before = index.meta_of(&entry.embedding.image_id);
    index.insert(&entry.embedding);
    assert_eq!(index.meta_of(&entry.embedding.image_id), before);
    assert!(before.is_some_and(|meta: EntryMeta| meta.is_filterable()));
}

#[test]
fn a_vector_inserted_without_metadata_is_counted_and_never_silently_filtered_in() {
    let index = HnswIndex::new(HnswParams::default());
    let (entries, _) = clustered(2, 4, 0.03);
    index.extend(entries);
    let bare = embedding_from(id_for(555_555), vec![0.1, 0.9, 0.3], 3);
    index.insert_entry(IndexEntry::bare(bare.clone()));

    let stats = index.stats();
    assert_eq!(
        stats.unfiltered, 1,
        "the metadata-free vector was not counted"
    );
    assert_eq!(stats.filterable, stats.vectors - 1);
}

#[test]
fn levels_come_from_the_id_and_not_from_a_generator() {
    let id = id_for(42);
    let first = level_for(&id, 32);
    for _ in 0..50 {
        assert_eq!(level_for(&id, 32), first, "the level moved between calls");
    }
    // And the distribution is not degenerate: over a thousand ids, some are above
    // layer zero. A generator that always returned zero would build a flat graph
    // that still passes every recall test, slowly.
    let above = (0..1_000)
        .filter(|n| level_for(&id_for(*n), 32) > 0)
        .count();
    assert!(
        (10..400).contains(&above),
        "{above} of 1,000 ids landed above layer zero, which is not an exponential \
         distribution with mL = 1/ln(32)"
    );
}

#[test]
fn stats_report_the_model_version_and_the_stragglers() {
    let (entries, _) = clustered(3, 5, 0.03);
    let index = HnswIndex::build(entries, HnswParams::default(), &clock());
    let mut newer = embedding_from(id_for(777), vec![0.4; EMBED_DIM], 11);
    newer.model_ver = 110;
    index.insert_entry(IndexEntry::bare(newer));

    let stats = index.stats();
    assert_eq!(
        stats.model_ver, 100,
        "the lowest version is the reported one"
    );
    assert_eq!(stats.stale, 1);
}

#[test]
fn a_degenerate_vector_that_reaches_the_index_is_counted() {
    // The runner refuses these (AURA-ML-5013), so this asserts the second line of
    // defence: if one ever arrives, it is visible rather than looking like an
    // unusually distinctive photograph.
    let index = HnswIndex::new(HnswParams::default());
    let mut zeroed = aura_index::contract::index::ImageEmbedding::zeroed(id_for(1), 100);
    zeroed.vec = [F16::ZERO; EMBED_DIM];
    index.insert_entry(IndexEntry::bare(zeroed));
    assert_eq!(index.stats().degenerate, 1);
}

#[test]
fn find_similar_reports_the_hash_distance_and_the_human_number() {
    let (index, entries) = built(10, 10, 0.03);
    let Some(query) = entries.first() else {
        panic!("no entries");
    };
    let answer = query::find_similar(&index, &query.embedding, 8, IndexFilter::none(), &clock());

    assert_eq!(answer.query, query.embedding.image_id);
    assert_eq!(answer.filter_kind, "none");
    assert_eq!(answer.neighbours.len(), 8);
    for neighbour in &answer.neighbours {
        assert!((0.0..=1.0).contains(&neighbour.similarity));
        assert!(neighbour.dhash_distance <= 64);
        assert_eq!(
            neighbour.near_duplicate,
            neighbour.dhash_distance <= query::NEAR_DUPLICATE_HAMMING
        );
    }

    // The first two frames of each cluster share a hash by construction, so the
    // near-duplicate list is not empty and is not everything.
    let duplicates = answer.near_duplicates();
    assert!(
        !duplicates.is_empty(),
        "the fixture's shared hashes were not found"
    );
    assert!(duplicates.len() < answer.neighbours.len());
}

#[test]
fn the_difference_hash_answers_without_touching_the_graph() {
    let (index, entries) = built(12, 10, 0.03);
    let Some(query) = entries.first() else {
        panic!("no entries");
    };
    let found = index.dhash_within(query.embedding.dhash, 0);
    assert!(
        found.iter().any(|(id, _)| *id == query.embedding.image_id),
        "an exact hash match did not include the frame itself"
    );
    for (_, distance) in &found {
        assert_eq!(*distance, 0);
    }
}

#[test]
fn the_filter_kind_is_the_telemetry_vocabulary_and_not_the_filter_contents() {
    assert_eq!(IndexFilter::none().kind(), "none");
    assert_eq!(IndexFilter::within_seconds(5).kind(), "time");
    assert_eq!(
        IndexFilter::none().with_camera(CameraId(0)).kind(),
        "camera"
    );
    assert_eq!(
        IndexFilter::none()
            .with_scene(aura_index::SceneId(0))
            .kind(),
        "scene"
    );
    assert_eq!(
        IndexFilter::within_seconds(5)
            .with_camera(CameraId(0))
            .kind(),
        "composite"
    );
}
