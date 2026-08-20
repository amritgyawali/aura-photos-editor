//! The phase 08 evaluation harness: section 10.1's gates, and the proof that the
//! harness itself would catch a bad grouper.
//!
//! Named by section 4 of the phase document as `ml/eval/burst_eval.py`'s Rust
//! counterpart, and wired in as a test target of `aura-brain-wedding`, so
//! `cargo test --workspace` runs it and CI cannot forget it.
//!
//! ## What can and cannot be gated today
//!
//! Section 10.1 sets six gates. Every one of them is a statement about an *algorithm*
//! rather than about a trained model - grouping has no model of its own; it reads phase
//! 05's embedding - and that is a materially better position than phases 06 and 07 were
//! in, where three of six gates had to be deferred.
//!
//! One dependency is still open and it is the important one. **The shipped embedding is
//! a placeholder** (phase 05 condition C10): it carries no wedding semantics, so
//! `cosine_distance` between two real photographs describes a random projection. The
//! `0.55 x (1 - embed_dist)` term of section 6.2's score is therefore untested against
//! photographs, and no number here is a claim about how this phase behaves on a real
//! wedding's pixels.
//!
//! So this file takes the same shape phases 05, 06 and 07 took:
//!
//! 1. **It measures every metric** with the arithmetic `ml/eval/burst_eval.py` uses, so
//!    the day the embedding is trained the two numbers can be compared rather than
//!    argued about.
//! 2. **It gates them on ground truth whose answer is known by construction** - the
//!    five patterns in `aura_brain_wedding::fixtures::bursts`, whose embeddings are
//!    authored angles rather than model output - which proves the harness would fail a
//!    grouper that had learned nothing.
//! 3. **It gates, for real and today, everything that does not depend on the
//!    embedding's semantics**: the cadence estimator, the hard-gap rule, the drive-mode
//!    evidence, the union-find, the size cap, the scene conditioning, the cross-camera
//!    rule, the duplicate conjunction, the split and merge validation, and determinism.
//!
//! Every deferred gate is printed with the reason it is deferred, so a reader of the CI
//! log sees the same list the exit report carries.

use std::collections::{BTreeMap, BTreeSet};

use aura_brain_wedding::fixtures::bursts::{self, BurstTruth};
use aura_brain_wedding::moments::cadence::{Cadence, CameraTable};
use aura_brain_wedding::moments::duplicate::{self, FaceOverlap};
use aura_brain_wedding::moments::graph::{self, Frame, MomentProfileRegistry};
use aura_brain_wedding::moments::merge::{self, MomentSpan};
use aura_brain_wedding::moments::moment::{self, Assembly};
use aura_core::contract::moment::DuplicateKind;
use aura_core::SceneId;

// -- section 10.1's thresholds, in one place ---------------------------------

/// Burst grouping ARI against human grouping. Section 0's headline KPI.
const ARI_GATE: f64 = 0.90;
/// Near-identical detection recall.
const DUPLICATE_RECALL_GATE: f64 = 0.98;
/// Near-identical detection precision.
const DUPLICATE_PRECISION_GATE: f64 = 0.95;

/// The scene the fixtures are grouped as unless a test says otherwise.
///
/// `Unknown` - the defaults - because a wedding can be grouped before it is classified
/// and that is the path most likely to run in the product. A gate measured only on the
/// tuned scenes would not notice a broken default.
const BASELINE_SCENE: SceneId = SceneId::Unknown;

// -- the pipeline under test -------------------------------------------------

/// Group one pattern the way the pass does, and return the partition.
fn group_pattern(truth: &BurstTruth, scene: SceneId, classified: bool) -> Vec<Vec<usize>> {
    let frames = bursts::to_frames(truth, scene, classified);
    let profiles = MomentProfileRegistry::embedded().expect("the shipped thresholds load");
    let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
    let edges = graph::candidates(&frames, &cadence, &profiles);
    let grouped = graph::group(&frames, &edges, &profiles);

    // The cross-camera pass, to a fixed point, exactly as `Moments::group` runs it.
    let mut groups = grouped;
    for _ in 0..4 {
        let spans: Vec<MomentSpan> = groups
            .iter()
            .map(|members| MomentSpan::of(&frames, members))
            .collect();
        let merges = merge::cross_camera(&frames, &spans);
        if merges.is_empty() {
            break;
        }
        groups = merge::apply(&groups, &merges, &frames);
    }
    groups
}

/// A partition as a per-frame label vector, which is what an ARI is computed from.
fn labels_of(groups: &[Vec<usize>], frames: usize) -> Vec<usize> {
    let mut out = vec![usize::MAX; frames];
    for (label, group) in groups.iter().enumerate() {
        for member in group {
            if let Some(slot) = out.get_mut(*member) {
                *slot = label;
            }
        }
    }
    out
}

// -- the metrics -------------------------------------------------------------

/// Adjusted Rand Index between two labellings.
///
/// The metric section 0 names and `ml/eval/burst_eval.py` computes, in the standard
/// contingency-table form:
///
/// ```text
/// ARI = (sum_ij C(n_ij,2) - E) / (0.5 x (sum_i C(a_i,2) + sum_j C(b_j,2)) - E)
/// E   = sum_i C(a_i,2) x sum_j C(b_j,2) / C(n,2)
/// ```
///
/// Adjusted rather than raw, and that is the whole reason it is the gate: a raw Rand
/// index on a wedding where most frames are singletons is above 0.95 for *any*
/// grouping, including one that puts every frame on its own. The adjustment subtracts
/// exactly that expected agreement, which is why `an_everything_is_one_moment_grouper_fails_the_gate`
/// can exist at all.
fn adjusted_rand_index(truth: &[usize], found: &[usize]) -> f64 {
    let n = truth.len().min(found.len());
    if n < 2 {
        return 1.0;
    }
    let choose2 = |count: usize| -> f64 {
        let value = count as f64;
        value * (value - 1.0) / 2.0
    };

    let mut table: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut rows: BTreeMap<usize, usize> = BTreeMap::new();
    let mut cols: BTreeMap<usize, usize> = BTreeMap::new();
    for index in 0..n {
        let (Some(a), Some(b)) = (truth.get(index), found.get(index)) else {
            continue;
        };
        *table.entry((*a, *b)).or_insert(0) += 1;
        *rows.entry(*a).or_insert(0) += 1;
        *cols.entry(*b).or_insert(0) += 1;
    }

    let sum_cells: f64 = table.values().copied().map(choose2).sum();
    let sum_rows: f64 = rows.values().copied().map(choose2).sum();
    let sum_cols: f64 = cols.values().copied().map(choose2).sum();
    let total = choose2(n);
    if total <= 0.0 {
        return 1.0;
    }
    let expected = sum_rows * sum_cols / total;
    let maximum = 0.5 * (sum_rows + sum_cols);
    if (maximum - expected).abs() < f64::EPSILON {
        // Both labellings are trivial in the same way - every frame in one group, or
        // every frame in its own. Perfect agreement, and the formula is 0/0 there.
        return 1.0;
    }
    (sum_cells - expected) / (maximum - expected)
}

/// Recall and precision of near-identical detection against a labelled pair set.
fn duplicate_scores(
    found: &BTreeSet<(usize, usize)>,
    expected: &BTreeSet<(usize, usize)>,
) -> (f64, f64) {
    if expected.is_empty() {
        return (1.0, if found.is_empty() { 1.0 } else { 0.0 });
    }
    let hits = found.intersection(expected).count() as f64;
    let recall = hits / expected.len() as f64;
    let precision = if found.is_empty() {
        1.0
    } else {
        hits / found.len() as f64
    };
    (recall, precision)
}

/// Every pair a grouping calls near identical, as sorted index pairs.
fn near_identical_pairs(frames: &[Frame], groups: &[Vec<usize>]) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for members in groups {
        let bursts_in = aura_brain_wedding::moments::burst::partition(
            frames,
            members,
            &Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>()),
        );
        let sets = duplicate::sets_of_moment(
            frames,
            members,
            &bursts_in,
            &|_, _| FaceOverlap::default(),
            &|_, _| false,
        );
        for set in sets
            .iter()
            .filter(|set| set.kind == DuplicateKind::NearIdentical)
        {
            let positions: Vec<usize> = set
                .image_ids
                .iter()
                .filter_map(|image| frames.iter().position(|frame| frame.image == *image))
                .collect();
            for (offset, left) in positions.iter().enumerate() {
                for right in positions.iter().skip(offset + 1) {
                    out.insert((*left.min(right), *left.max(right)));
                }
            }
        }
    }
    out
}

// -- section 10.1, gate by gate ----------------------------------------------

#[test]
fn burst_grouping_matches_human_grouping_on_every_fixture() {
    let mut worst = 1.0f64;
    for truth in bursts::all() {
        let groups = group_pattern(&truth, BASELINE_SCENE, false);
        let found = labels_of(&groups, truth.frames.len());
        let ari = adjusted_rand_index(&truth.labels(), &found);
        println!(
            "  {:<18} ARI {ari:.3}  ({} moments found, {} expected)",
            truth.name,
            groups.len(),
            truth.moments()
        );
        assert!(
            ari >= ARI_GATE,
            "{} grouped at ARI {ari:.3}, below the {ARI_GATE} gate: {} moments where a \
             photographer said {}",
            truth.name,
            groups.len(),
            truth.moments()
        );
        worst = worst.min(ari);
    }
    println!("burst grouping: worst ARI {worst:.3} across five patterns, gate {ARI_GATE}");
}

#[test]
fn no_group_mixes_two_labelled_moments() {
    // Section 10.1's second half of the ARI gate, and the one QAIQ audits by eye:
    // "no group mixes two labelled moments". Stated separately because a high ARI can
    // still hide one bad merge in a large fixture, and a merge is the failure that
    // costs a photographer a keeper they never see.
    for truth in bursts::all() {
        let groups = group_pattern(&truth, BASELINE_SCENE, false);
        for group in &groups {
            let distinct: BTreeSet<usize> = group
                .iter()
                .filter_map(|index| truth.frames.get(*index).map(|frame| frame.moment))
                .collect();
            assert_eq!(
                distinct.len(),
                1,
                "{}: one group spans {} labelled moments",
                truth.name,
                distinct.len()
            );
        }
    }
}

#[test]
fn a_ten_frame_per_second_burst_stays_one_moment() {
    let truth = bursts::bouquet_toss();
    let groups = group_pattern(&truth, BASELINE_SCENE, false);
    assert_eq!(groups.len(), 1, "the bouquet toss came apart");
    assert_eq!(groups.first().map(Vec::len), Some(truth.frames.len()));
}

#[test]
fn a_long_dance_sequence_becomes_multiple_moments() {
    let truth = bursts::dance_floor();
    let groups = group_pattern(&truth, SceneId::DanceFloor, true);
    assert!(
        groups.len() >= 2,
        "the dance floor collapsed into {} moment(s)",
        groups.len()
    );
    let found = labels_of(&groups, truth.frames.len());
    let ari = adjusted_rand_index(&truth.labels(), &found);
    assert!(ari >= ARI_GATE, "dance floor ARI {ari:.3}");
}

#[test]
fn two_cameras_on_one_kiss_produce_one_moment_with_two_sub_groups() {
    let truth = bursts::two_shooters();
    let frames = bursts::to_frames(&truth, SceneId::Kiss, true);
    let groups = group_pattern(&truth, SceneId::Kiss, true);
    assert_eq!(groups.len(), 1, "the two shooters were not merged");

    let members = groups.first().cloned().unwrap_or_default();
    let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
    let sub_groups = aura_brain_wedding::moments::burst::partition(&frames, &members, &cadence);
    assert_eq!(sub_groups.len(), 2, "the per-camera sub-groups were lost");

    // And each sub-group is one body's, which is what makes a bad merge recoverable.
    for group in &sub_groups {
        let bodies: BTreeSet<u32> = group
            .iter()
            .filter_map(|index| frames.get(*index).map(|frame| frame.camera))
            .collect();
        assert_eq!(bodies.len(), 1);
    }
}

#[test]
fn near_identical_detection_meets_the_recall_and_precision_gates() {
    // The labelled pair set: within `bracketed_detail` every pair is the same
    // photograph, and across every other fixture no pair is. That second half is what
    // makes the precision number mean anything - a detector that called everything a
    // duplicate would score recall 1.0 and precision near zero.
    let mut recalls = Vec::new();
    let mut precisions = Vec::new();

    for truth in bursts::all() {
        let frames = bursts::to_frames(&truth, BASELINE_SCENE, false);
        let groups = group_pattern(&truth, BASELINE_SCENE, false);
        let found = near_identical_pairs(&frames, &groups);

        let expected: BTreeSet<(usize, usize)> = if truth.name == "bracketed_detail" {
            let mut pairs = BTreeSet::new();
            for left in 0..truth.frames.len() {
                for right in (left + 1)..truth.frames.len() {
                    pairs.insert((left, right));
                }
            }
            pairs
        } else {
            BTreeSet::new()
        };

        let (recall, precision) = duplicate_scores(&found, &expected);
        println!(
            "  {:<18} duplicate recall {recall:.3} precision {precision:.3} ({} pairs found)",
            truth.name,
            found.len()
        );
        recalls.push(recall);
        precisions.push(precision);
    }

    let worst_recall = recalls.iter().copied().fold(1.0f64, f64::min);
    let worst_precision = precisions.iter().copied().fold(1.0f64, f64::min);
    assert!(
        worst_recall >= DUPLICATE_RECALL_GATE,
        "duplicate recall {worst_recall:.3} below {DUPLICATE_RECALL_GATE}"
    );
    assert!(
        worst_precision >= DUPLICATE_PRECISION_GATE,
        "duplicate precision {worst_precision:.3} below {DUPLICATE_PRECISION_GATE}"
    );
}

#[test]
fn one_stop_apart_exposures_are_caught() {
    // Section 10.1 names this case explicitly: "including one-stop-apart exposures".
    let truth = bursts::bracketed_detail();
    let frames = bursts::to_frames(&truth, SceneId::Details, true);
    let groups = group_pattern(&truth, SceneId::Details, true);
    let pairs = near_identical_pairs(&frames, &groups);
    assert_eq!(pairs.len(), 3, "the three-frame bracket produced {pairs:?}");
}

#[test]
fn the_gate_rejects_a_grouper_that_puts_everything_in_one_moment() {
    // The guard phase 06 and phase 07 both wrote, a third time. A gate that cannot
    // fail is not a gate, and the failure mode a grouping metric is most likely to be
    // blind to is a grouper that merges everything - which scores a raw Rand index
    // above 0.9 on a wedding of mostly-singletons.
    let truth = bursts::slow_ceremony();
    let everything_one = vec![0usize; truth.frames.len()];
    let ari = adjusted_rand_index(&truth.labels(), &everything_one);
    assert!(
        ari < ARI_GATE,
        "a grouper that merged the whole ceremony scored ARI {ari:.3}, which the gate accepted"
    );
}

#[test]
fn the_gate_rejects_a_grouper_that_puts_every_frame_on_its_own() {
    // The opposite failure, and the one a strict threshold produces. It costs no
    // photographs and it makes the product useless, so it has to fail too.
    let truth = bursts::bouquet_toss();
    let all_singletons: Vec<usize> = (0..truth.frames.len()).collect();
    let ari = adjusted_rand_index(&truth.labels(), &all_singletons);
    assert!(
        ari < ARI_GATE,
        "a grouper that split a 10 fps burst into fourteen moments scored ARI {ari:.3}"
    );
}

// -- everything that does not depend on the embedding ------------------------

#[test]
fn scene_thresholds_change_the_grouping() {
    // Invariant 7, measured rather than asserted: the same frames, grouped under two
    // scenes, come out differently. This is section 6.2's "`dance_floor` needs looser
    // thresholds than `family_portrait`" as a behaviour rather than as a config file.
    let truth = bursts::dance_floor();
    let loose = group_pattern(&truth, SceneId::DanceFloor, true).len();
    let strict = group_pattern(&truth, SceneId::FamilyPortrait, true).len();
    println!("  dance_floor grouped as dance_floor: {loose} moments; as family_portrait: {strict}");
    assert!(
        strict >= loose,
        "the strict scene produced fewer moments ({strict}) than the loose one ({loose})"
    );
}

#[test]
fn a_hard_gap_always_breaks_a_group() {
    // Section 6.1's unconditional rule, tested against the strongest possible
    // counter-evidence: two byte-identical frames either side of a 21-second gap.
    let profiles = MomentProfileRegistry::embedded().expect("profiles");
    let truth = bursts::bouquet_toss();
    let mut frames = bursts::to_frames(&truth, BASELINE_SCENE, false);
    frames.truncate(2);
    if let Some(second) = frames.get_mut(1) {
        second.ts += aura_brain_wedding::moments::cadence::HARD_GAP_MS + 1;
    }
    let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
    let edges = graph::candidates(&frames, &cadence, &profiles);
    assert!(edges.is_empty(), "an edge crossed a hard gap");
}

#[test]
fn the_size_cap_holds_on_every_scene() {
    // Section 6.2's over-large split, checked against the worst input: two hundred
    // identical frames at 5 fps, which no re-cluster can separate. The cut-on-gap
    // fallback is what makes the cap a guarantee rather than an aspiration.
    let profiles = MomentProfileRegistry::embedded().expect("profiles");
    let truth = bursts::bouquet_toss();
    let mut frames = Vec::new();
    for i in 0..200i64 {
        let mut copy = bursts::to_frames(&truth, BASELINE_SCENE, false)
            .first()
            .cloned()
            .expect("a frame");
        copy.image = aura_core::PhotoId::new();
        copy.ts = bursts::BURST_DAY_MS + i * 200;
        frames.push(copy);
    }
    for scene in [
        SceneId::DanceFloor,
        SceneId::FamilyPortrait,
        SceneId::Unknown,
    ] {
        for frame in &mut frames {
            frame.scene = scene;
            frame.classified = true;
        }
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        let edges = graph::candidates(&frames, &cadence, &profiles);
        let groups = graph::group(&frames, &edges, &profiles);
        let cap = profiles.get(scene).max_group;
        for group in &groups {
            assert!(
                group.len() <= cap,
                "{scene}: a group of {} survived a cap of {cap}",
                group.len()
            );
        }
        assert_eq!(
            groups.iter().map(Vec::len).sum::<usize>(),
            frames.len(),
            "{scene}: the split lost frames"
        );
    }
}

#[test]
fn grouping_is_deterministic() {
    // Invariant 4. Two runs over the same frames produce the same partition, the same
    // bursts, the same duplicate sets and the same reasons.
    for truth in bursts::all() {
        let first = group_pattern(&truth, BASELINE_SCENE, false);
        let second = group_pattern(&truth, BASELINE_SCENE, false);
        assert_eq!(first, second, "{} grouped differently twice", truth.name);
    }
}

#[test]
fn every_moment_explains_itself() {
    // Invariant 2, across every fixture: a decision without an explanation is a bug.
    let profiles = MomentProfileRegistry::embedded().expect("profiles");
    let cameras = CameraTable::new(vec!["cam_a".into(), "cam_b".into()]);
    for truth in bursts::all() {
        let frames = bursts::to_frames(&truth, BASELINE_SCENE, false);
        let cadence = Cadence::estimate(&frames.iter().map(Frame::shot).collect::<Vec<_>>());
        for members in group_pattern(&truth, BASELINE_SCENE, false) {
            let row = moment::assemble(
                &frames,
                &members,
                &cadence,
                &cameras,
                Assembly {
                    profiles: &profiles,
                    segment: None,
                    embed_ver: 100,
                    weakest_edge: Some(0.80),
                    all_drive: false,
                    reclustered: false,
                    cross_camera: None,
                },
                &|_, _| FaceOverlap::default(),
                &|_, _| false,
            );
            assert!(
                !row.moment.reasons.is_empty(),
                "{}: a moment with no reasons",
                truth.name
            );
            assert!(row.moment.confidence > 0.0);
            assert!((0.0..=1.0).contains(&row.moment.diversity));
            let flattened: usize = row.moment.bursts.iter().map(Vec::len).sum();
            assert_eq!(
                flattened,
                row.moment.image_ids.len(),
                "{}: the bursts are not a partition",
                truth.name
            );
        }
    }
}

#[test]
fn the_label_files_and_the_rust_fixtures_agree() {
    // Two copies of the ground truth exist - `fixtures::bursts` for the Rust gates and
    // `tests/fixtures/labels/bursts_*.json` for `ml/eval/burst_eval.py` - and this is
    // what stops them drifting. A fixture edited in one place and not the other fails
    // here rather than silently splitting the two gates.
    for truth in bursts::all() {
        // Up two from the crate root rather than a string edit on the manifest path: this
        // file is compiled from `crates/aura-brain-wedding`, and a `replace` of the
        // Windows spelling of that suffix silently does nothing on a Linux or macOS
        // runner - which is the three-OS matrix condition finding a bug the moment it is
        // honoured. Every other fixture path in the workspace already joins its way up.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/labels")
            .join(format!("bursts_{}.json", truth.name));
        let path = path.display();
        let text = std::fs::read_to_string(path.to_string())
            .unwrap_or_else(|err| panic!("cannot read {path}: {err}"));
        let parsed: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|err| panic!("{path} is not JSON: {err}"));

        let expected = parsed
            .get("expected_moments")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        assert_eq!(
            expected,
            truth.moments(),
            "{}: the label file says {expected} moments, the fixture says {}",
            truth.name,
            truth.moments()
        );

        let frames = parsed
            .get("frames")
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("{path} has no frames"));
        assert_eq!(
            frames.len(),
            truth.frames.len(),
            "{}: frame count",
            truth.name
        );

        let base = truth.frames.first().map_or(0, |frame| frame.ts);
        for (index, entry) in frames.iter().enumerate() {
            let Some(authored) = truth.frames.get(index) else {
                continue;
            };
            let offset = entry
                .get("offset_ms")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(-1);
            assert_eq!(
                offset,
                authored.ts - base,
                "{} frame {index}: offset",
                truth.name
            );
            assert_eq!(
                entry.get("camera").and_then(serde_json::Value::as_u64),
                Some(u64::from(authored.camera)),
                "{} frame {index}: camera",
                truth.name
            );
            assert_eq!(
                entry.get("moment").and_then(serde_json::Value::as_u64),
                Some(authored.moment as u64),
                "{} frame {index}: moment",
                truth.name
            );
        }
        // Every label file says why a photographer would group it that way. A ground
        // truth nobody can justify is a ground truth nobody should be graded against.
        let why = parsed
            .get("why")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        assert!(
            why.len() > 40,
            "{}: the label file has no rationale",
            truth.name
        );
    }
}

#[test]
fn deferred_gates_are_printed_rather_than_quietly_passed() {
    // The list the exit report carries, printed into the CI log so the two cannot
    // drift apart unnoticed.
    println!("deferred, and why:");
    println!(
        "  the 0.55 embedding term is untested against photographs - the shipped embedding is a \
         placeholder (phase 05 condition C10). Every distance here comes from an authored angle."
    );
    println!(
        "  the 0.15 identity term is untested against photographs - the shipped face models are \
         placeholders (phase 06 condition C1). The fixtures carry no identities."
    );
    println!(
        "  the QAIQ audit of 500 groups (section 9) needs real weddings and a human. Nobody has \
         looked at a moment stack for a wedding they attended."
    );
    println!(
        "  the perceptual A/B against Aftershoot and Narrative Select (section 10.2) needs a panel \
         and two installed competitors."
    );
}
