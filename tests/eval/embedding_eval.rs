//! The evaluation harness: section 6.4's gates, and the proof that the harness
//! itself would catch a bad model.
//!
//! Named by section 4 of the phase document and wired in as a test target of
//! `aura-vision`, so `cargo test --workspace` runs it and CI cannot forget it.
//!
//! ## What can and cannot be gated today
//!
//! Section 6.4 sets four gates. Three of them - cluster purity, normalised mutual
//! information and retrieval mAP against the raw backbone - are statements about
//! *wedding semantics*, and measuring them needs human scene labels on real
//! weddings. There are none in this repository, and the shipped
//! `wedding_embedding` 1.0.0 is a placeholder backbone
//! (`docs/adr/ADR-0011-embeddings-and-similarity-index.md` section 3). Computing
//! them against generated fixtures would produce three numbers that describe the
//! fixture generator, and a gate that passes for the wrong reason is worse than a
//! gate that is honestly deferred. They are condition C10 in
//! `docs/progress/PHASE-05-EXIT.md`.
//!
//! What this file does instead is the useful half:
//!
//! 1. **It measures all four metrics** with the same arithmetic
//!    `ml/models/embed/eval_retrieval.py` uses, so the day a head is trained the
//!    two numbers can be compared rather than argued about.
//! 2. **It gates them on a corpus whose structure is known by construction.** A
//!    labelled synthetic corpus has real clusters, so purity, NMI and mAP have
//!    correct answers - and asserting the section 6.4 thresholds against it proves
//!    the harness would fail a head that had learned nothing. A broken metric
//!    passes silently forever otherwise.
//! 3. **It gates, for real and today, the two things that do not depend on the
//!    model**: duplicate detection, which the difference hash answers, and
//!    determinism, which is invariant 4.

use std::collections::BTreeMap;

use aura_core::clock::{Clock, SystemClock};
use aura_core::{PhotoId, Priority};
use aura_index::contract::index::{
    cosine_distance, ImageEmbedding, ImageId, IndexFilter, SimilarityIndex, EMBED_DIM, F16,
};
use aura_index::hnsw::{EntryMeta, HnswIndex, HnswParams, IndexEntry};
use aura_index::metrics::{
    cluster_purity, duplicate_pr_curve, exact_knn, mean_average_precision,
    normalised_mutual_information, recall_at_k, recall_at_precision,
};
use aura_index::query::normalise;
use aura_index::CameraId;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelLevel, PixelSource};
use aura_vision::embed::descriptors;
use aura_vision::embed::hash::{dhash, hamming};
use aura_vision::embed::model::EmbeddingModel;

// -- section 6.4's thresholds, in one place ----------------------------------

/// Cluster purity against scene labels.
const PURITY_GATE: f64 = 0.85;
/// Normalised mutual information against scene labels.
const NMI_GATE: f64 = 0.80;
/// Duplicate recall, at or above the precision floor below.
const DUPLICATE_RECALL_GATE: f64 = 0.98;
/// The precision the duplicate recall is measured at.
const DUPLICATE_PRECISION_FLOOR: f64 = 0.95;
/// Retrieval mAP@10. Section 6.4 gates the *improvement over the backbone* at
/// eight points; this is the absolute floor the harness itself must clear on a
/// corpus with known structure.
const MAP_HARNESS_FLOOR: f64 = 0.80;

// -- fixtures ----------------------------------------------------------------

/// A deterministic generator. Not `rand`: every number this file prints ends up in
/// an exit report, and a figure that changes when a dependency updates is not a
/// measurement.
struct Rng(u64);

impl Rng {
    const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_bits(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn unit(&mut self) -> f32 {
        let bits = (self.next_bits() >> 40) as f32 / 16_777_216.0;
        bits * 2.0 - 1.0
    }
}

fn id_for(n: u64) -> ImageId {
    let text = format!("pht_05550000-0000-0001-0000-{n:012x}");
    match PhotoId::from_db(&text) {
        Ok(id) => id,
        Err(err) => panic!("{text} is not a photo id: {err}"),
    }
}

fn embedding_from(id: ImageId, mut vector: Vec<f32>, dhash_bits: u64) -> ImageEmbedding {
    vector.resize(EMBED_DIM, 0.0);
    normalise(&mut vector);
    let mut out = ImageEmbedding::zeroed(id, 100);
    for (slot, value) in out.vec.iter_mut().zip(&vector) {
        *slot = F16::from_f32(*value);
    }
    out.dhash = dhash_bits;
    out
}

/// A labelled corpus with real scene structure: `scenes` scenes, `per_scene`
/// frames each, drawn around a scene centre.
///
/// `spread` is per component, and the scale that matters is a component of a unit
/// 512-d vector - about 0.044. A spread of 0.02 therefore perturbs a frame by
/// roughly a quarter of its scene centre, which is tight structure; a spread of 0.3
/// would drown the centre entirely and produce a corpus with no clusters in it at
/// all, which is a mistake worth naming because every metric still returns a
/// plausible number on it.
///
/// This stands in for the labelled fixture weddings section 6.4 asks for. What it
/// can prove is that the metrics are implemented correctly; what it cannot prove is
/// anything about wedding semantics, and the module header says so.
fn labelled_corpus(
    scenes: usize,
    per_scene: usize,
    spread: f32,
) -> (Vec<IndexEntry>, BTreeMap<ImageId, u32>) {
    let mut rng = Rng::new(0x0505_0604_0302_0100);
    let mut centres = Vec::with_capacity(scenes);
    for _ in 0..scenes {
        let mut centre: Vec<f32> = (0..EMBED_DIM).map(|_| rng.unit()).collect();
        normalise(&mut centre);
        centres.push(centre);
    }

    let mut entries = Vec::with_capacity(scenes * per_scene);
    let mut truth = BTreeMap::new();
    let mut serial = 0u64;
    for (scene, centre) in centres.iter().enumerate() {
        for member in 0..per_scene {
            serial += 1;
            let id = id_for(serial);
            let vector: Vec<f32> = centre
                .iter()
                .map(|value| value + rng.unit() * spread)
                .collect();
            entries.push(IndexEntry {
                embedding: embedding_from(id, vector, rng.next_bits()),
                meta: EntryMeta {
                    timeline_ms: Some(
                        1_700_000_000_000i64 + scene as i64 * 1_800_000 + member as i64 * 2_000,
                    ),
                    camera: Some(CameraId((member % 2) as u32)),
                    scene: None,
                },
            });
            truth.insert(id, scene as u32);
        }
    }
    (entries, truth)
}

/// One synthetic photograph. Same generator as the crate's own tests, repeated
/// here so the harness is readable on its own.
fn frame(width: u32, height: u32, hue_shift: f32, brightness: f32, stripes: u32) -> PixelBuffer {
    let mut bytes = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let across = x as f32 / width.max(1) as f32;
            let down = y as f32 / height.max(1) as f32;
            let stripe = if stripes > 0 && (x / stripes.max(1)).is_multiple_of(2) {
                1.0
            } else {
                0.55
            };
            let base = (across * 0.6 + down * 0.4) * stripe * brightness;
            bytes.push(((base + hue_shift).clamp(0.0, 1.0) * 255.0) as u8);
            bytes.push(((base * 0.8 + hue_shift * 0.5).clamp(0.0, 1.0) * 255.0) as u8);
            bytes.push(((base * 0.6).clamp(0.0, 1.0) * 255.0) as u8);
        }
    }
    PixelBuffer {
        width,
        height,
        data: PixelData::Srgb8(bytes),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Embedded,
        decode_ms: 1,
    }
}

/// A photograph made of coarse random blocks, at a given exposure.
///
/// The duplicate fixture needs two things the gradient generator above cannot give
/// it: 60 photographs that are genuinely *different from each other* after the hash
/// reduces them to nine columns, and an exposure change that is a clean scale rather
/// than a clip. Blocks of seed-derived luminance give the first; keeping every value
/// under `0.5 / brightness` gives the second, so no channel saturates and the
/// ordering the hash reads is exactly preserved.
///
/// That is the claim being gated: a difference hash is invariant to exposure, so a
/// frame and the same frame one stop brighter are the same photograph to it.
fn blocks(seed: u64, brightness: f32) -> PixelBuffer {
    const WIDTH: u32 = 288;
    const HEIGHT: u32 = 192;
    const BLOCK: u32 = 24;

    let mut rng = Rng::new(seed);
    let across = (WIDTH / BLOCK) as usize;
    let down = (HEIGHT / BLOCK) as usize;
    let levels: Vec<f32> = (0..across * down)
        .map(|_| (rng.unit().abs() * 0.45 + 0.02).min(0.5))
        .collect();

    let mut bytes = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let cell = (y / BLOCK) as usize * across + (x / BLOCK) as usize;
            let level = levels.get(cell).copied().unwrap_or(0.25) * brightness;
            let value = (level.clamp(0.0, 1.0) * 255.0) as u8;
            bytes.push(value);
            bytes.push(value);
            bytes.push(value);
        }
    }
    PixelBuffer {
        width: WIDTH,
        height: HEIGHT,
        data: PixelData::Srgb8(bytes),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Embedded,
        decode_ms: 1,
    }
}

/// The difference hash of a synthetic frame.
fn hash_of(buffer: &PixelBuffer) -> u64 {
    let Some(rgb) = buffer.as_srgb8() else {
        panic!("the fixture is not 8-bit sRGB");
    };
    let width = buffer.width as usize;
    let height = buffer.height as usize;
    let plane = descriptors::luma_plane(rgb, width, height);
    dhash(&plane.values, width, height)
}

/// A deterministic clustering, for the purity and NMI measurements only.
///
/// Single link over the k-nearest-neighbour graph: every edge shorter than `radius`
/// merges its two ends, and the clusters are the connected components. Walking
/// frames in id order makes it deterministic; union by smaller root makes the
/// component labels independent of the order edges were seen.
///
/// It is not the clustering the product will ship - scene clustering is phase 07 and
/// owns its own algorithm - and it is deliberately the simplest thing that turns
/// distances into labels, so that a purity figure is a statement about the
/// *embedding* rather than about a clever clustering.
///
/// A greedy seed-and-claim pass was tried first and rejected: it scores purity 1.0
/// while splitting each scene into several clusters, which is exactly the failure
/// mode purity cannot see and NMI can. That is why section 6.4 gates both.
fn single_link_clusters(
    index: &HnswIndex,
    corpus: &[ImageEmbedding],
    radius: f32,
    neighbours: usize,
) -> Vec<(ImageId, u32)> {
    let mut position: BTreeMap<ImageId, usize> = BTreeMap::new();
    for (slot, embedding) in corpus.iter().enumerate() {
        position.insert(embedding.image_id, slot);
    }
    let mut parent: Vec<usize> = (0..corpus.len()).collect();

    fn root(parent: &mut [usize], mut node: usize) -> usize {
        while parent.get(node).copied().unwrap_or(node) != node {
            let next = parent.get(node).copied().unwrap_or(node);
            // Path halving, so a long chain does not cost quadratic time.
            let grandparent = parent.get(next).copied().unwrap_or(next);
            if let Some(slot) = parent.get_mut(node) {
                *slot = grandparent;
            }
            node = grandparent;
        }
        node
    }

    for embedding in corpus {
        let Some(left) = position.get(&embedding.image_id).copied() else {
            continue;
        };
        for (id, distance) in index.knn(embedding, neighbours, IndexFilter::none()) {
            if distance > radius {
                continue;
            }
            let Some(right) = position.get(&id).copied() else {
                continue;
            };
            let (a, b) = (root(&mut parent, left), root(&mut parent, right));
            if a != b {
                let (low, high) = if a < b { (a, b) } else { (b, a) };
                if let Some(slot) = parent.get_mut(high) {
                    *slot = low;
                }
            }
        }
    }

    corpus
        .iter()
        .enumerate()
        .map(|(slot, embedding)| {
            let component = root(&mut parent, slot);
            (embedding.image_id, component as u32)
        })
        .collect()
}

// -- the gates that hold today ------------------------------------------------

#[test]
fn duplicate_detection_meets_its_gate() {
    // Section 6.4: "recall >= 0.98 at precision >= 0.95 using the labelled
    // duplicate sets", and section 10.1: "including near-duplicates one stop
    // apart".
    //
    // This gate is met today and is not deferred, because it does not depend on the
    // embedding: near-duplicate detection is the difference hash, which is a
    // deterministic function of the pixels with no learned component in it.
    //
    // The labelled set: 60 photographs, each of which also appears one stop
    // brighter. Every same-content pair is a true duplicate; every other pair is
    // not.
    let mut originals = Vec::new();
    let mut brighter = Vec::new();
    for n in 0..60u64 {
        // `wrapping_mul`, because the dev profile turns on overflow checks and a
        // 64-bit golden-ratio stride overflows on the second iteration. Wrapping is
        // what a mixing step wants anyway; the release build was doing it silently.
        let seed = 0x0505_D0D0_0000_0000 ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        originals.push(hash_of(&blocks(seed, 1.0)));
        brighter.push(hash_of(&blocks(seed, 1.8)));
    }

    let mut scored: Vec<(bool, f64)> = Vec::new();
    for (left_index, left) in originals.iter().enumerate() {
        for (right_index, right) in brighter.iter().enumerate() {
            let truly_the_same = left_index == right_index;
            scored.push((truly_the_same, f64::from(hamming(*left, *right))));
        }
    }

    let curve = duplicate_pr_curve(&scored);
    let recall = recall_at_precision(&curve, DUPLICATE_PRECISION_FLOOR);
    println!("duplicate recall at precision {DUPLICATE_PRECISION_FLOOR}: {recall:.4}");
    assert!(
        recall >= DUPLICATE_RECALL_GATE,
        "duplicate recall was {recall:.4} at precision {DUPLICATE_PRECISION_FLOOR}, gate is \
         {DUPLICATE_RECALL_GATE}"
    );
}

#[test]
fn the_same_input_produces_the_same_vector_and_the_same_neighbours() {
    // Invariant 4, gated as an equality rather than a tolerance. This is the gate
    // that makes every other number in the exit report reproducible.
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("wedding_embedding.onnx");
    std::fs::write(
        &path,
        aura_infer::onnx::serialise(&aura_infer::onnx::fixtures::wedding_embedding()),
    )
    .expect("write the model");

    let source = std::sync::Arc::new(aura_infer::source::StaticModelSource::new().with(
        aura_vision::embed::model::model_ref(),
        &path,
        aura_infer::contract::infer::Precision::Fp32,
        aura_infer::contract::infer::ModelClass::Embedding,
        16,
    ));
    let clock: std::sync::Arc<dyn Clock> = std::sync::Arc::new(SystemClock::default());
    let engine = std::sync::Arc::new(aura_infer::service::InferEngine::new(
        aura_infer::ep::BackendRegistry::with_reference(),
        source,
        aura_infer::contract::infer::HardwarePlan::conservative(
            4,
            "2026-08-13T00:00:00Z".to_string(),
        ),
        std::sync::Arc::clone(&clock),
    ));
    let model = EmbeddingModel::new(engine);

    let mut first_pass = Vec::new();
    let mut second_pass = Vec::new();
    for n in 0..12u64 {
        let buffer = frame(600, 400, n as f32 * 0.05, 0.7, 30 + (n as u32) * 5);
        for sink in [&mut first_pass, &mut second_pass] {
            let analysed = aura_vision::embed::run_one(
                &model,
                id_for(n + 1),
                &buffer,
                PixelLevel::Thumb(384),
                Priority::AiBatch,
            )
            .expect("embed");
            sink.push(analysed.stored.embedding);
        }
    }

    for (left, right) in first_pass.iter().zip(&second_pass) {
        assert_eq!(
            left.vec, right.vec,
            "{} drifted between runs",
            left.image_id
        );
        assert_eq!(left.dhash, right.dhash);
    }

    let entries: Vec<IndexEntry> = first_pass.iter().cloned().map(IndexEntry::bare).collect();
    let one = HnswIndex::build(entries.clone(), HnswParams::default(), clock.as_ref());
    let two = HnswIndex::build(entries, HnswParams::default(), clock.as_ref());
    for embedding in &first_pass {
        assert_eq!(
            one.knn(embedding, 5, IndexFilter::none()),
            two.knn(embedding, 5, IndexFilter::none()),
            "two indexes disagreed about {}",
            embedding.image_id
        );
    }
}

#[test]
fn the_graph_agrees_with_the_flat_scan_it_approximates() {
    // Not one of section 6.4's four, and the one that makes the other three
    // meaningful: a purity figure measured through a graph that loses a third of its
    // neighbours is a figure about the graph.
    let (entries, _) = labelled_corpus(40, 25, 0.02);
    let corpus: Vec<ImageEmbedding> = entries
        .iter()
        .map(|entry| entry.embedding.clone())
        .collect();
    let clock = SystemClock::default();
    let index = HnswIndex::build(entries, HnswParams::default(), &clock);

    let mut total = 0.0f64;
    let mut queries = 0usize;
    for embedding in corpus.iter().step_by(17) {
        let exact = exact_knn(embedding, &corpus, 10);
        total += recall_at_k(&index.knn(embedding, 10, IndexFilter::none()), &exact);
        queries += 1;
    }
    let recall = total / queries as f64;
    println!("graph recall@10 against the flat scan: {recall:.4}");
    assert!(recall >= 0.95, "recall@10 was {recall:.4}");
}

// -- the harness proves itself against known structure ------------------------

#[test]
fn the_purity_and_nmi_gates_pass_on_a_corpus_with_known_structure() {
    // What this proves: the metrics are implemented correctly and the harness would
    // fail a head that had learned nothing. What it does not prove: anything about
    // weddings. The same gates against human scene labels are C10.
    let (entries, truth) = labelled_corpus(30, 20, 0.02);
    let corpus: Vec<ImageEmbedding> = entries
        .iter()
        .map(|entry| entry.embedding.clone())
        .collect();
    let clock = SystemClock::default();
    let index = HnswIndex::build(entries, HnswParams::default(), &clock);

    let assignments = single_link_clusters(&index, &corpus, 0.35, 25);
    let purity = cluster_purity(&assignments, &truth);
    let nmi = normalised_mutual_information(&assignments, &truth);
    println!("purity {purity:.4}, NMI {nmi:.4} on the synthetic labelled corpus");

    assert!(
        purity >= PURITY_GATE,
        "purity was {purity:.4} on a corpus with real clusters, gate is {PURITY_GATE}; the \
         metric or the clustering is broken"
    );
    assert!(
        nmi >= NMI_GATE,
        "NMI was {nmi:.4} on a corpus with real clusters, gate is {NMI_GATE}"
    );
}

#[test]
fn the_purity_and_nmi_metrics_reject_a_useless_embedding() {
    // The other half of proving a gate: it must fail when it should. A corpus of
    // pure noise has no cluster structure, so a metric that still reported 0.9
    // purity would be measuring the clustering rather than the embedding.
    let mut rng = Rng::new(0xDEAD_0505_BEEF_0001);
    let mut entries = Vec::new();
    let mut truth = BTreeMap::new();
    for n in 0..600u64 {
        let id = id_for(n + 1);
        let vector: Vec<f32> = (0..EMBED_DIM).map(|_| rng.unit()).collect();
        entries.push(IndexEntry::bare(embedding_from(
            id,
            vector,
            rng.next_bits(),
        )));
        // Labels that the vectors know nothing about.
        truth.insert(id, (n % 30) as u32);
    }
    let corpus: Vec<ImageEmbedding> = entries
        .iter()
        .map(|entry| entry.embedding.clone())
        .collect();
    let clock = SystemClock::default();
    let index = HnswIndex::build(entries, HnswParams::default(), &clock);

    let assignments = single_link_clusters(&index, &corpus, 0.35, 25);
    let nmi = normalised_mutual_information(&assignments, &truth);
    println!("NMI on an embedding that knows nothing: {nmi:.4}");
    assert!(
        nmi < NMI_GATE,
        "a random embedding scored {nmi:.4} NMI, which means the gate would pass anything"
    );
}

#[test]
fn retrieval_map_is_computed_the_way_the_python_harness_computes_it() {
    let (entries, truth) = labelled_corpus(25, 20, 0.02);
    let corpus: Vec<ImageEmbedding> = entries
        .iter()
        .map(|entry| entry.embedding.clone())
        .collect();
    let clock = SystemClock::default();
    let index = HnswIndex::build(entries, HnswParams::default(), &clock);

    let results: Vec<(ImageId, Vec<ImageId>)> = corpus
        .iter()
        .map(|embedding| {
            (
                embedding.image_id,
                index
                    .knn(embedding, 10, IndexFilter::none())
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect(),
            )
        })
        .collect();

    let same_scene = |query: ImageId, neighbour: ImageId| -> bool {
        match (truth.get(&query), truth.get(&neighbour)) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        }
    };
    let score = mean_average_precision(&results, &same_scene);
    println!("mAP@10 on the synthetic labelled corpus: {score:.4}");
    assert!(
        score >= MAP_HARNESS_FLOOR,
        "mAP@10 was {score:.4} on a corpus with real clusters, floor is {MAP_HARNESS_FLOOR}"
    );

    // And it must be near zero when the labels are unrelated to the vectors.
    let unrelated = |query: ImageId, neighbour: ImageId| -> bool {
        let _ = (query, neighbour);
        false
    };
    assert_eq!(mean_average_precision(&results, &unrelated), 0.0);
}

// -- the deferred gates, computed and reported rather than asserted -----------

#[test]
fn the_deferred_gates_are_reported_with_the_reason_they_are_deferred() {
    // This test always passes, and that is deliberate. Its job is to put the three
    // deferred numbers and the reason for the deferral into the test output, so that
    // "we forgot" and "we recorded it" are distinguishable in a CI log.
    //
    // When labelled wedding data and a trained head exist (C10), this becomes an
    // assertion and the printed figures become the baseline it is compared against.
    println!("---- section 6.4 gates ----");
    println!("duplicate recall >= {DUPLICATE_RECALL_GATE} at precision >= {DUPLICATE_PRECISION_FLOOR}: ASSERTED (difference hash, no learned component)");
    println!("cluster purity >= {PURITY_GATE} against human scene labels: DEFERRED (C10, no labelled wedding data)");
    println!(
        "NMI >= {NMI_GATE} against human scene labels: DEFERRED (C10, no labelled wedding data)"
    );
    println!("mAP@10 >= backbone + 8 points: DEFERRED (C10, the shipped model is a placeholder backbone with no head)");
    println!(
        "see docs/progress/PHASE-05-EXIT.md section 8 and docs/model-cards/wedding_embedding.md"
    );

    // What is asserted here is that the deferral is recorded where a reader will
    // find it, rather than only in this printout.
    let card = include_str!("../../docs/model-cards/wedding_embedding.md");
    assert!(
        card.contains("deferred (C10)"),
        "the model card does not record the deferred gates"
    );
    let adr = include_str!("../../docs/adr/ADR-0011-embeddings-and-similarity-index.md");
    assert!(
        adr.contains("Sev 2 carried condition"),
        "ADR-0011 does not record the placeholder model as a carried condition"
    );
}

// -- the dark-scene regression fixture from section 12 ------------------------

#[test]
fn dark_and_bright_subsets_do_not_collapse_into_one_cluster() {
    // Section 10.1: "dark dance-floor and backlit-ceremony subsets are not collapsed
    // into one cluster (specific regression fixtures)".
    //
    // With a placeholder backbone this is a test of the descriptors rather than of
    // the embedding, and that is worth having on its own: the luminance statistics
    // and the histogram are what phase 25 uses, and they must separate a dance floor
    // from a backlit ceremony even when the vector cannot.
    let dark: Vec<_> = (0..20u32)
        .map(|n| frame(600, 400, 0.02, 0.12, 40 + n))
        .collect();
    let bright: Vec<_> = (0..20u32)
        .map(|n| frame(600, 400, 0.02, 0.95, 40 + n))
        .collect();

    let stats_of = |buffer: &PixelBuffer| {
        let Some(rgb) = buffer.as_srgb8() else {
            panic!("not sRGB");
        };
        let plane = descriptors::luma_plane(rgb, buffer.width as usize, buffer.height as usize);
        descriptors::luma_stats(&plane)
    };

    let dark_means: Vec<f32> = dark.iter().map(|f| stats_of(f).mean).collect();
    let bright_means: Vec<f32> = bright.iter().map(|f| stats_of(f).mean).collect();

    let worst_dark = dark_means.iter().copied().fold(0.0f32, f32::max);
    let best_bright = bright_means.iter().copied().fold(1.0f32, f32::min);
    println!("dark subset up to {worst_dark:.4}, bright subset from {best_bright:.4}");
    assert!(
        worst_dark < best_bright,
        "the two subsets overlap in luminance, so nothing downstream can separate them"
    );

    // And the histograms are not the same shape either.
    let histogram_of = |buffer: &PixelBuffer| {
        let Some(rgb) = buffer.as_srgb8() else {
            panic!("not sRGB");
        };
        descriptors::hsv_histogram(rgb, buffer.width as usize, buffer.height as usize)
    };
    let Some(one_dark) = dark.first().map(histogram_of) else {
        panic!("no dark frames");
    };
    let Some(one_bright) = bright.first().map(histogram_of) else {
        panic!("no bright frames");
    };
    assert_ne!(one_dark, one_bright);
}

// -- the property every consumer will assume ----------------------------------

#[test]
fn a_near_duplicate_pair_is_closer_than_an_unrelated_pair_in_both_measures() {
    // The one semantic property a placeholder backbone does hold, because it follows
    // from continuity rather than from training: two frames that differ by an
    // exposure change are closer in vector space than two frames of different
    // content. Phase 08 needs exactly this.
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("wedding_embedding.onnx");
    std::fs::write(
        &path,
        aura_infer::onnx::serialise(&aura_infer::onnx::fixtures::wedding_embedding()),
    )
    .expect("write the model");
    let source = std::sync::Arc::new(aura_infer::source::StaticModelSource::new().with(
        aura_vision::embed::model::model_ref(),
        &path,
        aura_infer::contract::infer::Precision::Fp32,
        aura_infer::contract::infer::ModelClass::Embedding,
        16,
    ));
    let clock: std::sync::Arc<dyn Clock> = std::sync::Arc::new(SystemClock::default());
    let model = EmbeddingModel::new(std::sync::Arc::new(aura_infer::service::InferEngine::new(
        aura_infer::ep::BackendRegistry::with_reference(),
        source,
        aura_infer::contract::infer::HardwarePlan::conservative(
            4,
            "2026-08-13T00:00:00Z".to_string(),
        ),
        clock,
    )));

    let embed = |n: u64, buffer: &PixelBuffer| {
        aura_vision::embed::run_one(
            &model,
            id_for(n),
            buffer,
            PixelLevel::Thumb(384),
            Priority::AiBatch,
        )
        .expect("embed")
        .stored
        .embedding
    };

    let base = frame(600, 400, 0.0, 0.60, 50);
    let one_stop = frame(600, 400, 0.0, 0.85, 50);
    let different = frame(600, 400, 0.45, 0.60, 137);

    let left = embed(1, &base);
    let sibling = embed(2, &one_stop);
    let stranger = embed(3, &different);

    let near = cosine_distance(&left.to_f32(), &sibling.to_f32());
    let far = cosine_distance(&left.to_f32(), &stranger.to_f32());
    println!("one stop apart: {near:.5}; different content: {far:.5}");
    assert!(
        near < far,
        "an exposure change ({near:.5}) moved the vector further than a content change ({far:.5})"
    );

    // And the hash agrees, which is the cheap half of the same statement.
    assert!(hamming(left.dhash, sibling.dhash) < hamming(left.dhash, stranger.dhash));
}
