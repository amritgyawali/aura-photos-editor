"""Section 10.1's burst-grouping gates, computed the way the Rust harness computes them.

The Python counterpart of `tests/eval/burst_eval.rs`, named by PHASE-08 section 4. The
two implement the same arithmetic deliberately: when a trained embedding arrives (phase
05 condition C10), the number this script produces on a labelled corpus and the number
CI produces on the synthetic patterns have to be comparable rather than argued about.

    python ml/eval/burst_eval.py --self-test
    python ml/eval/burst_eval.py --labels tests/fixtures/labels --predictions groups.json
    python ml/eval/burst_eval.py --labels tests/fixtures/labels --predictions groups.json --ablate

## The gates

| Gate | Threshold | Section |
|---|---|---|
| Burst grouping ARI vs human grouping | >= 0.90 | 0, 10.1 |
| No group mixes two labelled moments | exact | 10.1 |
| Near-identical recall | >= 0.98 | 10.1 |
| Near-identical precision | >= 0.95 | 10.1 |
| 10 fps burst stays one moment | exact | 10.1 |
| A long dance sequence becomes several moments | exact | 10.1 |
| Two cameras on one kiss make one moment with two sub-groups | exact | 10.1 |

## Three measurement choices that are easy to get wrong

**The index is adjusted, and that is the whole point.** A raw Rand index on a wedding
where most frames are singletons is above 0.95 for any grouping at all, including one
that puts every frame on its own. The adjustment subtracts exactly that expected
agreement. `--self-test` asserts that both degenerate groupers - everything in one
moment, and every frame alone - fail the gate.

**Precision is measured against the negatives, not only the positives.** Recall alone
is maximised by a detector that calls every pair a duplicate. The pair set this script
scores against is every pair *within a labelled moment*: the positives are the pairs a
labeller marked as the same photograph, and every other in-moment pair is a negative.
Pairs from different moments are excluded, because a detector is never asked about them.

**NMI is reported and never gated.** It is the metric the clustering literature reaches
for and it is systematically kinder than ARI to a grouper that over-splits, which is the
failure mode a photographer notices least and this product cares about least. It is here
so a reader can see both numbers; the gate is the harsher one.

## What this cannot measure yet

The embedding underneath is a placeholder (phase 05 condition C10) and the face models
are placeholders (phase 06 condition C1), so the `0.55 x (1 - embed_dist)` and
`0.15 x identity_overlap` terms of section 6.2's score are untested against photographs.
Everything this script reports on the shipped label files is a measurement of the
*grouping algorithm* against ground truth whose answer is known by construction. That is
condition **C2** in `docs/progress/PHASE-08-EXIT.md`.
"""

from __future__ import annotations

import argparse
import itertools
import json
import math
import pathlib
import sys
from collections import Counter, defaultdict

# -- section 10.1's thresholds, in one place ---------------------------------

ARI_GATE = 0.90
DUPLICATE_RECALL_GATE = 0.98
DUPLICATE_PRECISION_GATE = 0.95


# -- the metrics -------------------------------------------------------------


def _choose2(count: int) -> float:
    return count * (count - 1) / 2.0


def adjusted_rand_index(truth: list[int], found: list[int]) -> float:
    """ARI in the standard contingency-table form.

    Identical to `adjusted_rand_index` in `tests/eval/burst_eval.rs`, including the
    two degenerate cases: fewer than two frames is 1.0, and a maximum equal to the
    expectation - both labellings trivial in the same way - is 1.0 rather than a
    division by zero.
    """
    n = min(len(truth), len(found))
    if n < 2:
        return 1.0

    table: Counter[tuple[int, int]] = Counter()
    rows: Counter[int] = Counter()
    cols: Counter[int] = Counter()
    for a, b in zip(truth[:n], found[:n]):
        table[(a, b)] += 1
        rows[a] += 1
        cols[b] += 1

    sum_cells = sum(_choose2(v) for v in table.values())
    sum_rows = sum(_choose2(v) for v in rows.values())
    sum_cols = sum(_choose2(v) for v in cols.values())
    total = _choose2(n)
    if total <= 0:
        return 1.0
    expected = sum_rows * sum_cols / total
    maximum = 0.5 * (sum_rows + sum_cols)
    if abs(maximum - expected) < 1e-12:
        return 1.0
    return (sum_cells - expected) / (maximum - expected)


def normalised_mutual_information(truth: list[int], found: list[int]) -> float:
    """NMI with arithmetic-mean normalisation. Reported, never gated - see the header."""
    n = min(len(truth), len(found))
    if n == 0:
        return 1.0
    joint: Counter[tuple[int, int]] = Counter()
    rows: Counter[int] = Counter()
    cols: Counter[int] = Counter()
    for a, b in zip(truth[:n], found[:n]):
        joint[(a, b)] += 1
        rows[a] += 1
        cols[b] += 1

    def entropy(counts: Counter[int]) -> float:
        return -sum((c / n) * math.log(c / n) for c in counts.values() if c > 0)

    mutual = 0.0
    for (a, b), count in joint.items():
        p_ab = count / n
        p_a = rows[a] / n
        p_b = cols[b] / n
        if p_ab > 0:
            mutual += p_ab * math.log(p_ab / (p_a * p_b))

    denominator = 0.5 * (entropy(rows) + entropy(cols))
    if denominator <= 0:
        return 1.0
    return mutual / denominator


def group_purity(truth: list[int], found: list[int]) -> tuple[int, int]:
    """Groups that mix two labelled moments, and groups in total.

    Section 10.1's "no group mixes two labelled moments", which is a stricter and more
    readable statement than the ARI it sits beside: a merge costs a photographer a frame
    they never see, and one such group is a failure however good the index is.
    """
    members: defaultdict[int, set[int]] = defaultdict(set)
    for label, group in zip(truth, found):
        members[group].add(label)
    mixed = sum(1 for labels in members.values() if len(labels) > 1)
    return mixed, len(members)


def duplicate_scores(
    found_pairs: set[tuple[int, int]],
    positive_pairs: set[tuple[int, int]],
    candidate_pairs: set[tuple[int, int]],
) -> tuple[float, float]:
    """Recall and precision of near-identical detection.

    `candidate_pairs` is every pair a detector was actually asked about - every pair
    inside a labelled moment. Pairs outside it are excluded from both numbers, because
    a detector never sees them and counting them would inflate precision for free.
    """
    found = found_pairs & candidate_pairs
    if not positive_pairs:
        return 1.0, 1.0 if not found else 0.0
    hits = len(found & positive_pairs)
    recall = hits / len(positive_pairs)
    precision = 1.0 if not found else hits / len(found)
    return recall, precision


# -- input -------------------------------------------------------------------


def load_labels(directory: pathlib.Path) -> dict[str, dict]:
    """Read every `bursts_*.json` in a directory.

    The same files `tests/eval/burst_eval.rs::the_label_files_and_the_rust_fixtures_agree`
    checks against the Rust fixtures, so a pattern edited in one place and not the other
    fails a test rather than silently splitting the two gates.
    """
    out: dict[str, dict] = {}
    for path in sorted(directory.glob("bursts_*.json")):
        with path.open(encoding="utf-8") as handle:
            payload = json.load(handle)
        if payload.get("schema") != "aura.burst-labels.v1":
            raise SystemExit(f"{path}: unexpected schema {payload.get('schema')!r}")
        name = payload.get("name") or path.stem.removeprefix("bursts_")
        out[name] = payload
    if not out:
        raise SystemExit(f"no bursts_*.json under {directory}")
    return out


def truth_labels(payload: dict) -> list[int]:
    return [int(frame["moment"]) for frame in payload["frames"]]


def in_moment_pairs(labels: list[int]) -> set[tuple[int, int]]:
    """Every pair of frames a labeller put in the same moment."""
    pairs: set[tuple[int, int]] = set()
    by_moment: defaultdict[int, list[int]] = defaultdict(list)
    for index, label in enumerate(labels):
        by_moment[label].append(index)
    for members in by_moment.values():
        pairs.update(itertools.combinations(sorted(members), 2))
    return pairs


# -- reporting ---------------------------------------------------------------


def evaluate(labels: dict[str, dict], predictions: dict[str, dict]) -> int:
    """Score every pattern and return the number of failures."""
    failures = 0
    worst_ari = 1.0
    worst_recall = 1.0
    worst_precision = 1.0

    print(f"{'pattern':<20} {'ARI':>7} {'NMI':>7} {'mixed':>6} {'recall':>7} {'prec':>7}")
    print("-" * 60)

    for name, payload in labels.items():
        truth = truth_labels(payload)
        predicted = predictions.get(name)
        if predicted is None:
            print(f"{name:<20} {'-':>7} {'-':>7} {'-':>6} {'-':>7} {'-':>7}  no prediction")
            failures += 1
            continue

        found = [int(value) for value in predicted.get("moments", [])]
        if len(found) != len(truth):
            print(f"{name:<20} predicted {len(found)} frames, labelled {len(truth)}")
            failures += 1
            continue

        ari = adjusted_rand_index(truth, found)
        nmi = normalised_mutual_information(truth, found)
        mixed, _ = group_purity(truth, found)

        candidates = in_moment_pairs(truth)
        positives = {
            (min(a, b), max(a, b))
            for a, b in (tuple(pair) for pair in payload.get("near_identical_pairs", []))
        }
        # A pattern whose label file declares a near-identical set count but no explicit
        # pair list means "every in-moment pair", which is what a bracket is.
        if not positives and payload.get("expected_near_identical_sets"):
            positives = set(candidates)
        detected = {
            (min(a, b), max(a, b))
            for a, b in (tuple(pair) for pair in predicted.get("near_identical_pairs", []))
        }
        recall, precision = duplicate_scores(detected, positives, candidates)

        print(
            f"{name:<20} {ari:>7.3f} {nmi:>7.3f} {mixed:>6} {recall:>7.3f} {precision:>7.3f}"
        )

        if ari < ARI_GATE:
            print(f"  FAIL {name}: ARI {ari:.3f} below {ARI_GATE}")
            failures += 1
        if mixed:
            print(f"  FAIL {name}: {mixed} group(s) mix two labelled moments")
            failures += 1
        if recall < DUPLICATE_RECALL_GATE:
            print(f"  FAIL {name}: duplicate recall {recall:.3f} below {DUPLICATE_RECALL_GATE}")
            failures += 1
        if precision < DUPLICATE_PRECISION_GATE:
            print(
                f"  FAIL {name}: duplicate precision {precision:.3f} "
                f"below {DUPLICATE_PRECISION_GATE}"
            )
            failures += 1

        worst_ari = min(worst_ari, ari)
        worst_recall = min(worst_recall, recall)
        worst_precision = min(worst_precision, precision)

    print("-" * 60)
    print(
        f"worst: ARI {worst_ari:.3f} (gate {ARI_GATE}), "
        f"recall {worst_recall:.3f} (gate {DUPLICATE_RECALL_GATE}), "
        f"precision {worst_precision:.3f} (gate {DUPLICATE_PRECISION_GATE})"
    )
    print()
    print("deferred, and why:")
    print(
        "  the 0.55 embedding term is untested against photographs - the shipped "
        "embedding is a placeholder (phase 05 condition C10)."
    )
    print(
        "  the 0.15 identity term is untested against photographs - the shipped face "
        "models are placeholders (phase 06 condition C1)."
    )
    print(
        "  the QAIQ audit of 500 groups (section 9) needs real weddings and a human."
    )
    return failures


def self_test() -> int:
    """Prove the metrics would catch a bad grouper.

    The same three assertions `tests/eval/burst_eval.rs` makes, so that a change to
    either implementation that broke the agreement shows up on both sides.
    """
    truth = [0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 2]

    perfect = adjusted_rand_index(truth, truth)
    assert abs(perfect - 1.0) < 1e-9, f"a perfect grouping scored {perfect}"

    everything_one = adjusted_rand_index(truth, [0] * len(truth))
    assert everything_one < ARI_GATE, (
        f"a grouper that merged everything scored {everything_one:.3f}, "
        "which the gate accepted"
    )

    all_singletons = adjusted_rand_index(truth, list(range(len(truth))))
    assert all_singletons < ARI_GATE, (
        f"a grouper that split everything scored {all_singletons:.3f}, "
        "which the gate accepted"
    )

    one_bad_merge = [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1]
    mixed, groups = group_purity(truth, one_bad_merge)
    assert mixed == 1, f"a merged pair of moments reported {mixed} mixed groups"
    assert groups == 2

    # Precision has to be able to fail: a detector that calls every in-moment pair a
    # duplicate on a pattern with no duplicates scores zero.
    candidates = in_moment_pairs(truth)
    recall, precision = duplicate_scores(set(candidates), set(), candidates)
    assert precision < DUPLICATE_PRECISION_GATE, (
        f"a detector that flagged everything scored precision {precision:.3f}"
    )

    print("self-test: the metrics agree with the Rust harness and reject both")
    print("           degenerate groupers, one bad merge, and an over-eager duplicate")
    print("           detector.")
    print(f"           perfect {perfect:.3f} | all-one {everything_one:.3f} | "
          f"all-singletons {all_singletons:.3f}")
    return 0


def ablate(labels: dict[str, dict]) -> int:
    """Print what each label file claims, for the MLR tuning report in section 9.

    Not an ablation of the model - there is no model - but of the *ground truth*: which
    patterns exercise which signal, so a reviewer tuning section 6.2's weights knows
    what would move if they changed one.
    """
    print(f"{'pattern':<20} {'frames':>7} {'moments':>8}  signal under test")
    print("-" * 78)
    signals = {
        "bouquet_toss": "drive-mode evidence and the burst window floor",
        "slow_ceremony": "the cadence window ceiling and the proximity multiplier",
        "dance_floor": "the scene threshold and the size cap",
        "two_shooters": "cross-camera overlap, medoid proximity, per-camera bursts",
        "bracketed_detail": "the three-way near-identical conjunction",
    }
    for name, payload in labels.items():
        frames = len(payload["frames"])
        moments = payload.get("expected_moments", 0)
        print(f"{name:<20} {frames:>7} {moments:>8}  {signals.get(name, 'unclassified')}")
    print()
    print("Every pattern's `why` field states what a photographer would say and why.")
    print("A ground truth nobody can justify is a ground truth nobody should be graded")
    print("against - which is the same rule `scene_profiles.toml` applies to thresholds.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--labels",
        type=pathlib.Path,
        default=pathlib.Path("tests/fixtures/labels"),
        help="directory of bursts_*.json human grouping labels",
    )
    parser.add_argument(
        "--predictions",
        type=pathlib.Path,
        help='JSON: {"<pattern>": {"moments": [...], "near_identical_pairs": [[i,j],...]}}',
    )
    parser.add_argument("--self-test", action="store_true", help="prove the metrics can fail")
    parser.add_argument("--ablate", action="store_true", help="print what each pattern tests")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    labels = load_labels(args.labels)
    if args.ablate:
        return ablate(labels)

    if not args.predictions:
        parser.error("--predictions is required unless --self-test or --ablate is given")

    with args.predictions.open(encoding="utf-8") as handle:
        predictions = json.load(handle)

    failures = evaluate(labels, predictions)
    if failures:
        print(f"\n{failures} gate(s) failed")
        return 1
    print("\nevery gate passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
