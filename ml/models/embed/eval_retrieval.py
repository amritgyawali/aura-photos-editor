"""Section 6.4's evaluation gates, computed exactly as the Rust harness computes them.

Four gates decide whether the wedding head ships:

* cluster purity >= 0.85 against human scene labels;
* normalised mutual information >= 0.80 against the same labels;
* duplicate recall >= 0.98 at precision >= 0.95 on the annotated duplicate sets;
* retrieval mAP@10 at least 8 points above the raw backbone - "otherwise the head
  is not worth shipping".

The arithmetic is duplicated between here and
``crates/aura-index/src/metrics.rs`` on purpose, and the duplication is the point:
the day a head is trained, the Python number and the Rust number have to agree, and
two independent implementations agreeing is the only evidence that either is right.
``--cross-check`` runs both on the same fixture and fails on any disagreement
larger than 1e-9.

Two conventions are pinned here because they are the two that silently differ
between implementations:

* **NMI is normalised by the arithmetic mean of the two entropies**, which is what
  ``sklearn.metrics.normalized_mutual_info_score`` does by default. Normalising by
  the geometric mean or by the maximum gives a different number for the same
  clustering, and a gate at 0.80 means nothing if the two sides of the fence
  disagree about which 0.80.
* **A duplicate score is a distance, so lower means more alike.** A threshold sweep
  written for a similarity produces a curve that looks plausible and is
  monotonically wrong.

Usage::

    python ml/models/embed/eval_retrieval.py --self-test
    python ml/models/embed/eval_retrieval.py --cross-check
    python ml/models/embed/eval_retrieval.py --embeddings vectors.npy --labels labels.json
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib
import subprocess
import sys
from collections import Counter, defaultdict
from typing import Callable, Sequence

# --------------------------------------------------------------------------
# Section 6.4's thresholds
# --------------------------------------------------------------------------

PURITY_GATE = 0.85
NMI_GATE = 0.80
DUPLICATE_RECALL_GATE = 0.98
DUPLICATE_PRECISION_FLOOR = 0.95
#: Section 6.4: "must beat the raw backbone by >= 8 points".
MAP_IMPROVEMENT_GATE = 0.08


# --------------------------------------------------------------------------
# Distances
# --------------------------------------------------------------------------


def cosine_distance(left: Sequence[float], right: Sequence[float]) -> float:
    """`1 - dot` on L2-normalised vectors, clamped to `0..2`.

    Identical to ``aura_index::contract::index::cosine_distance``, including the
    clamp: a vector that is not quite unit length must not produce a negative
    distance that sorts before an exact match.
    """
    dot = sum(a * b for a, b in zip(left, right))
    return min(max(1.0 - dot, 0.0), 2.0)


def exact_knn(
    query_index: int,
    corpus: Sequence[Sequence[float]],
    k: int,
) -> list[tuple[int, float]]:
    """Brute-force nearest neighbours, excluding the query itself.

    Ties broken by index, matching the Rust harness's tie-break by image id.
    """
    scored = [
        (other, cosine_distance(corpus[query_index], vector))
        for other, vector in enumerate(corpus)
        if other != query_index
    ]
    scored.sort(key=lambda pair: (pair[1], pair[0]))
    return scored[:k]


# --------------------------------------------------------------------------
# The four metrics
# --------------------------------------------------------------------------


def cluster_purity(assignments: Sequence[tuple[str, int]], truth: dict[str, int]) -> float:
    """Each cluster is credited with the size of its most common true label."""
    if not assignments:
        return 0.0
    per_cluster: dict[int, Counter[int]] = defaultdict(Counter)
    counted = 0
    for item, cluster in assignments:
        if item not in truth:
            continue
        per_cluster[cluster][truth[item]] += 1
        counted += 1
    if counted == 0:
        return 0.0
    dominant = sum(max(labels.values()) for labels in per_cluster.values())
    return dominant / counted


def normalised_mutual_information(assignments: Sequence[tuple[str, int]], truth: dict[str, int]) -> float:
    """NMI, normalised by the arithmetic mean of the two entropies.

    The companion to purity, because purity alone scores one-cluster-per-frame at
    1.0 and therefore cannot see over-splitting.
    """
    pairs = [(cluster, truth[item]) for item, cluster in assignments if item in truth]
    total = len(pairs)
    if total == 0:
        return 0.0
    n = float(total)

    joint: Counter[tuple[int, int]] = Counter(pairs)
    left_counts: Counter[int] = Counter(cluster for cluster, _ in pairs)
    right_counts: Counter[int] = Counter(label for _, label in pairs)

    def entropy(counts: Counter[int]) -> float:
        return -sum((count / n) * math.log(count / n) for count in counts.values() if count > 0)

    h_left = entropy(left_counts)
    h_right = entropy(right_counts)
    if h_left <= sys.float_info.epsilon and h_right <= sys.float_info.epsilon:
        return 1.0

    information = 0.0
    for (cluster, label), count in joint.items():
        p_joint = count / n
        p_left = left_counts[cluster] / n
        p_right = right_counts[label] / n
        if p_joint > 0.0 and p_left > 0.0 and p_right > 0.0:
            information += p_joint * math.log(p_joint / (p_left * p_right))

    denominator = (h_left + h_right) / 2.0
    if denominator <= sys.float_info.epsilon:
        return 0.0
    return min(max(information / denominator, 0.0), 1.0)


def duplicate_pr_curve(scored: Sequence[tuple[bool, float]]) -> list[tuple[float, float, float]]:
    """`(threshold, precision, recall)` at every score that occurs.

    ``scored`` is ``(is_a_true_duplicate, distance)`` and a *lower* distance means
    more alike.
    """
    positives = sum(1 for truth, _ in scored if truth)
    if positives == 0:
        return []
    thresholds = sorted({score for _, score in scored})
    curve = []
    for threshold in thresholds:
        predicted = [truth for truth, score in scored if score <= threshold]
        true_positives = sum(1 for truth in predicted if truth)
        precision = 1.0 if not predicted else true_positives / len(predicted)
        curve.append((threshold, precision, true_positives / positives))
    return curve


def recall_at_precision(curve: Sequence[tuple[float, float, float]], floor: float) -> float:
    """The best recall available at or above a precision floor."""
    candidates = [recall for _, precision, recall in curve if precision >= floor]
    return max(candidates) if candidates else 0.0


def mean_average_precision(
    results: Sequence[tuple[str, Sequence[str]]],
    relevant: Callable[[str, str], bool],
) -> float:
    """mAP over a set of ranked retrievals."""
    if not results:
        return 0.0
    total = 0.0
    for query, neighbours in results:
        hits = 0
        precision_sum = 0.0
        for rank, neighbour in enumerate(neighbours, start=1):
            if relevant(query, neighbour):
                hits += 1
                precision_sum += hits / rank
        if hits:
            total += precision_sum / hits
    return total / len(results)


# --------------------------------------------------------------------------
# The gate report
# --------------------------------------------------------------------------


def report(
    *,
    purity: float,
    nmi: float,
    duplicate_recall: float,
    head_map: float,
    backbone_map: float,
) -> dict[str, object]:
    """Every gate, its measurement and its verdict, as one JSON-able object."""
    improvement = head_map - backbone_map
    gates = {
        "purity": {"measured": purity, "gate": PURITY_GATE, "pass": purity >= PURITY_GATE},
        "nmi": {"measured": nmi, "gate": NMI_GATE, "pass": nmi >= NMI_GATE},
        "duplicate_recall": {
            "measured": duplicate_recall,
            "gate": DUPLICATE_RECALL_GATE,
            "at_precision": DUPLICATE_PRECISION_FLOOR,
            "pass": duplicate_recall >= DUPLICATE_RECALL_GATE,
        },
        "map_improvement": {
            "measured": improvement,
            "head_map": head_map,
            "backbone_map": backbone_map,
            "gate": MAP_IMPROVEMENT_GATE,
            "pass": improvement >= MAP_IMPROVEMENT_GATE,
        },
    }
    return {"gates": gates, "ship": all(gate["pass"] for gate in gates.values())}


# --------------------------------------------------------------------------
# Fixtures, the self-test and the cross-check
# --------------------------------------------------------------------------


class _Rng:
    """The same xorshift the Rust fixtures use, so a cross-check compares like with
    like rather than comparing two different corpora."""

    def __init__(self, seed: int) -> None:
        self.state = (seed | 1) & 0xFFFFFFFFFFFFFFFF

    def next_bits(self) -> int:
        state = self.state
        state ^= (state << 13) & 0xFFFFFFFFFFFFFFFF
        state ^= state >> 7
        state ^= (state << 17) & 0xFFFFFFFFFFFFFFFF
        self.state = state
        return (state * 0x2545F4914F6CDD1D) & 0xFFFFFFFFFFFFFFFF

    def unit(self) -> float:
        return ((self.next_bits() >> 40) / 16777216.0) * 2.0 - 1.0


def _labelled_corpus(scenes: int, per_scene: int, spread: float, dim: int = 512):
    rng = _Rng(0x0505060403020100)
    centres = []
    for _ in range(scenes):
        centre = [rng.unit() for _ in range(dim)]
        length = math.sqrt(sum(v * v for v in centre)) or 1.0
        centres.append([v / length for v in centre])

    vectors = []
    truth: dict[str, int] = {}
    names: list[str] = []
    for scene, centre in enumerate(centres):
        for _ in range(per_scene):
            vector = [v + rng.unit() * spread for v in centre]
            length = math.sqrt(sum(v * v for v in vector)) or 1.0
            vectors.append([v / length for v in vector])
            name = f"f{len(names)}"
            names.append(name)
            truth[name] = scene
    return vectors, names, truth


def _single_link_clusters(vectors, names, radius: float, neighbours: int) -> list[tuple[str, int]]:
    """The same clustering the Rust harness uses: single link over the kNN graph."""
    parent = list(range(len(vectors)))

    def root(node: int) -> int:
        while parent[node] != node:
            parent[node] = parent[parent[node]]
            node = parent[node]
        return node

    for index in range(len(vectors)):
        for other, distance in exact_knn(index, vectors, neighbours):
            if distance > radius:
                continue
            a, b = root(index), root(other)
            if a != b:
                low, high = (a, b) if a < b else (b, a)
                parent[high] = low

    return [(names[index], root(index)) for index in range(len(vectors))]


def _self_test() -> int:
    vectors, names, truth = _labelled_corpus(20, 15, 0.02)
    assignments = _single_link_clusters(vectors, names, radius=0.35, neighbours=20)

    purity = cluster_purity(assignments, truth)
    nmi = normalised_mutual_information(assignments, truth)
    assert purity >= PURITY_GATE, f"purity {purity:.4f} on a corpus with real clusters"
    assert nmi >= NMI_GATE, f"NMI {nmi:.4f} on a corpus with real clusters"

    # NMI must reject an assignment that knows nothing.
    scrambled = [(name, index % 7) for index, (name, _) in enumerate(assignments)]
    assert normalised_mutual_information(scrambled, truth) < NMI_GATE

    # Purity is 1.0 for one cluster per frame - the blind spot NMI covers.
    singletons = [(name, index) for index, (name, _) in enumerate(assignments)]
    assert abs(cluster_purity(singletons, truth) - 1.0) < 1e-9
    assert normalised_mutual_information(singletons, truth) < 1.0

    # A perfect duplicate detector reaches recall 1.0 at precision 1.0; a random
    # one does not clear the gate.
    perfect = [(True, 0.0)] * 50 + [(False, 1.0)] * 200
    assert recall_at_precision(duplicate_pr_curve(perfect), DUPLICATE_PRECISION_FLOOR) == 1.0
    useless = [(index % 5 == 0, (index % 17) / 17.0) for index in range(250)]
    assert recall_at_precision(duplicate_pr_curve(useless), DUPLICATE_PRECISION_FLOOR) < DUPLICATE_RECALL_GATE

    # mAP is 1.0 when every neighbour is relevant and 0.0 when none is.
    results = [
        (name, [other for other, label in truth.items() if label == truth[name] and other != name][:10])
        for name, _ in assignments[:40]
    ]
    assert abs(mean_average_precision(results, lambda q, n: truth[q] == truth[n]) - 1.0) < 1e-9
    assert mean_average_precision(results, lambda q, n: False) == 0.0

    # The gate report says ship only when all four pass.
    good = report(purity=0.9, nmi=0.85, duplicate_recall=0.99, head_map=0.72, backbone_map=0.60)
    assert good["ship"] is True
    bad = report(purity=0.9, nmi=0.85, duplicate_recall=0.99, head_map=0.64, backbone_map=0.60)
    assert bad["ship"] is False, "a head that beat the backbone by 4 points shipped"

    print(
        "eval_retrieval self-test: ok\n"
        f"  purity {purity:.4f} (gate {PURITY_GATE})\n"
        f"  NMI    {nmi:.4f} (gate {NMI_GATE})"
    )
    return 0


def _cross_check(workspace: pathlib.Path) -> int:
    """Run the Rust harness and compare its printed figures with this file's.

    The Rust harness prints its measurements; this reads them back and recomputes
    the same quantities on the same fixture. Any disagreement means one of the two
    implementations is wrong, and there is no way to tell which from either alone.
    """
    command = [
        "cargo",
        "test",
        "--release",
        "-p",
        "aura-vision",
        "--test",
        "embedding_eval",
        "--",
        "--nocapture",
    ]
    print("running:", " ".join(command), file=sys.stderr)
    try:
        completed = subprocess.run(
            command,
            cwd=workspace,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        print("cargo is not on PATH; cannot cross-check", file=sys.stderr)
        return 2

    if completed.returncode != 0:
        print(completed.stdout[-4000:], file=sys.stderr)
        print("the Rust harness failed; fix that before cross-checking", file=sys.stderr)
        return 1

    rust_figures: dict[str, float] = {}
    for line in completed.stdout.splitlines():
        if line.startswith("purity "):
            parts = line.replace(",", "").split()
            rust_figures["purity"] = float(parts[1])
            rust_figures["nmi"] = float(parts[3])
        elif line.startswith("duplicate recall at precision"):
            rust_figures["duplicate_recall"] = float(line.rsplit(":", 1)[1])
        elif line.startswith("mAP@10 on the synthetic"):
            rust_figures["map"] = float(line.rsplit(":", 1)[1])

    if not rust_figures:
        print("the Rust harness printed no figures to compare", file=sys.stderr)
        return 1

    vectors, names, truth = _labelled_corpus(30, 20, 0.02)
    assignments = _single_link_clusters(vectors, names, radius=0.35, neighbours=25)
    python_figures = {
        "purity": cluster_purity(assignments, truth),
        "nmi": normalised_mutual_information(assignments, truth),
    }

    failures = 0
    for name, python_value in python_figures.items():
        rust_value = rust_figures.get(name)
        if rust_value is None:
            continue
        if abs(python_value - rust_value) > 1e-4:
            print(
                f"MISMATCH {name}: python {python_value:.6f}, rust {rust_value:.6f}",
                file=sys.stderr,
            )
            failures += 1
        else:
            print(f"agree {name}: {python_value:.6f}")

    if failures:
        return 1
    print("cross-check: the two implementations agree")
    return 0


def _from_files(embeddings: pathlib.Path, labels: pathlib.Path) -> int:
    """Score a real run. Needs vectors and human labels, neither of which exists here."""
    try:
        import numpy as np
    except ImportError:
        print("numpy is required to read a vector file", file=sys.stderr)
        return 2

    matrix = np.load(embeddings)
    payload = json.loads(labels.read_text(encoding="utf-8"))
    truth = {str(key): int(value) for key, value in payload["scenes"].items()}
    names = [str(name) for name in payload["order"]]
    if len(names) != matrix.shape[0]:
        print("the label order and the vector file disagree about how many frames there are", file=sys.stderr)
        return 1

    vectors = [row.tolist() for row in matrix]
    assignments = _single_link_clusters(vectors, names, radius=0.35, neighbours=25)
    results = [
        (
            names[index],
            [names[other] for other, _ in exact_knn(index, vectors, 10)],
        )
        for index in range(len(vectors))
    ]
    head_map = mean_average_precision(results, lambda q, n: truth.get(q) == truth.get(n))

    print(
        json.dumps(
            {
                "purity": cluster_purity(assignments, truth),
                "nmi": normalised_mutual_information(assignments, truth),
                "map_at_10": head_map,
                "note": "duplicate recall needs the annotated duplicate sets; pass them separately",
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--embeddings", type=pathlib.Path, help="a .npy of L2-normalised vectors")
    parser.add_argument("--labels", type=pathlib.Path, help="JSON with 'order' and 'scenes'")
    parser.add_argument("--self-test", action="store_true", help="check the metrics against known structure")
    parser.add_argument("--cross-check", action="store_true", help="compare against the Rust harness")
    parser.add_argument(
        "--workspace",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parents[3],
        help="repository root, for --cross-check",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return _self_test()
    if args.cross_check:
        return _cross_check(args.workspace)
    if args.embeddings and args.labels:
        return _from_files(args.embeddings, args.labels)
    parser.error("give --self-test, --cross-check, or both --embeddings and --labels")
    return 1


if __name__ == "__main__":
    sys.exit(main())
