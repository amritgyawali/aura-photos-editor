"""Tune the identity clustering threshold on one project's labelled faces.

Section 6.2 of PHASE-06 asks for the clustering threshold to be "tuned per project by a
small validation sweep using high-quality faces only". This is that sweep, and it is the
Python half of `aura_vision::face::cluster::tune_threshold` - the two implement the same
arithmetic on purpose, so a number produced here and a number produced in the product can
be compared rather than argued about.

Run it against exported templates:

    python ml/models/face/tune_cluster.py --templates templates.npz --report sweep.json

The export format is deliberately plain: an ``.npz`` with ``templates`` (N x 512 float32,
L2 normalised), ``labels`` (N int32, the ground-truth identity, -1 for unlabelled) and
``quality`` (N float32). Producing it needs consented, labelled faces, which do not exist
in this repository - so this script is specified and tested against synthetic data rather
than run against a wedding. That is condition C1 in docs/progress/PHASE-06-EXIT.md.

Three things here are not obvious and are the reason this file is longer than a loop:

1. **Purity vetoes F1.** A threshold that scores marginally better on F1 while producing
   an impure cluster loses. An impure cluster is the failure a photographer notices; a
   fraction of a point of F1 is not.
2. **Ties break towards the lower threshold.** A lower threshold splits rather than
   merges, and a split identity is one merge dialog away from correct while a merged one
   has to be found first.
3. **The sweep is reported, not just its argmax.** A flat curve and a sharp peak call for
   different amounts of trust in the chosen number, and only the curve shows which one
   this project has.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path

# The shipped default, mirroring `ClusterConfig::default().threshold`. The ArcFace-family
# verification operating point in cosine distance.
DEFAULT_THRESHOLD = 0.55

# The within-cluster spread a pair of singletons is credited with, mirroring
# `aura_vision::face::cluster::COHESION_FLOOR`.
COHESION_FLOOR = 0.12

# The gap between two clusters may be at most this multiple of their own spread, mirroring
# `ClusterConfig::default().max_cohesion_ratio`.
MAX_COHESION_RATIO = 2.2

# Faces below this fused quality do not vote, mirroring `quality::MIN_USABLE`.
MIN_USABLE = 0.40


@dataclass
class Metrics:
    """Pairwise precision, recall and F1, plus impurity."""

    true_positive: int = 0
    false_positive: int = 0
    false_negative: int = 0
    impure_clusters: int = 0

    @property
    def precision(self) -> float:
        joined = self.true_positive + self.false_positive
        return 1.0 if joined == 0 else self.true_positive / joined

    @property
    def recall(self) -> float:
        should = self.true_positive + self.false_negative
        return 1.0 if should == 0 else self.true_positive / should

    @property
    def f1(self) -> float:
        total = self.precision + self.recall
        return 0.0 if total <= 0.0 else 2.0 * self.precision * self.recall / total


@dataclass
class SweepPoint:
    threshold: float
    clusters: int
    unassigned: int
    metrics: Metrics = field(default_factory=Metrics)


def cosine_distance(a, b) -> float:
    """1 - dot, for L2-normalised vectors. Clamped, as the Rust side clamps."""
    dot = sum(x * y for x, y in zip(a, b))
    return max(0.0, min(2.0, 1.0 - dot))


def mean_vector(vectors):
    if not vectors:
        return []
    width = len(vectors[0])
    total = [0.0] * width
    for vector in vectors:
        for index, value in enumerate(vector):
            total[index] += value
    scale = 1.0 / len(vectors)
    mean = [value * scale for value in total]
    norm = math.sqrt(sum(value * value for value in mean))
    if norm <= 1e-12:
        return mean
    return [value / norm for value in mean]


def cluster(templates, quality, threshold: float):
    """Average-linkage agglomerative clustering with cohesion verification.

    The same algorithm as `aura_vision::face::cluster::cluster`, written plainly rather
    than for speed: this runs on a few thousand faces in a tuning loop, not on a wedding
    in a pipeline. The two must agree on *answers*, not on implementation.

    Average linkage is computed from running sums, using the identity that makes it cheap
    in Rust: for unit vectors, the mean pairwise cosine distance between two clusters is
    one minus the dot product of their unnormalised means, divided by the two sizes.
    """
    voting = [index for index, score in enumerate(quality) if score >= MIN_USABLE]
    members = {index: [index] for index in voting}
    sums = {index: list(templates[index]) for index in voting}
    refused: set[tuple[int, int, int, int]] = set()

    def linkage(left: int, right: int) -> float:
        scale = 1.0 / (len(members[left]) * len(members[right]))
        dot = sum(x * y for x, y in zip(sums[left], sums[right]))
        return max(0.0, min(2.0, 1.0 - dot * scale))

    def spread(index: int) -> float:
        size = len(members[index])
        if size < 2:
            return COHESION_FLOOR
        energy = sum(value * value for value in sums[index])
        pairs = size * (size - 1)
        return max(0.0, min(2.0, 1.0 - (energy - size) / pairs))

    while True:
        best = None
        keys = sorted(members)
        for position, left in enumerate(keys):
            for right in keys[position + 1 :]:
                key = (left, right, len(members[left]), len(members[right]))
                if key in refused:
                    continue
                distance = linkage(left, right)
                if distance > threshold:
                    continue
                if best is None or distance < best[2]:
                    best = (left, right, distance)
        if best is None:
            break

        left, right, _ = best
        # Cohesion verification: the gap must be small relative to how spread out the two
        # clusters already are. This is the chain-merge guard; see the module header of
        # `aura_vision::face::cluster`.
        allowed = max(spread(left), spread(right), COHESION_FLOOR) * MAX_COHESION_RATIO
        if linkage(left, right) > allowed:
            refused.add((left, right, len(members[left]), len(members[right])))
            continue

        members[left].extend(members[right])
        members[left].sort()
        sums[left] = [a + b for a, b in zip(sums[left], sums[right])]
        del members[right]
        del sums[right]

    # Clusters of one are not identities.
    assignments = {}
    clusters = []
    for label, (_, group) in enumerate(sorted(members.items())):
        if len(group) < 2:
            continue
        clusters.append(group)
        for index in group:
            assignments[index] = label
    return clusters, assignments


def pairwise_metrics(truth, assignments, count: int) -> Metrics:
    """The metric section 10.1 states the gate in.

    Pairwise rather than cluster accuracy, because pairwise punishes the failure that
    matters: one chain merge joining two thirty-face identities creates 900 false-positive
    pairs and collapses precision, where a cluster-counting metric would look almost right.
    """
    metrics = Metrics()
    for left in range(count):
        for right in range(left + 1, count):
            if truth[left] < 0 or truth[right] < 0:
                continue
            same_truth = truth[left] == truth[right]
            a = assignments.get(left)
            b = assignments.get(right)
            same_predicted = a is not None and a == b
            if same_truth and same_predicted:
                metrics.true_positive += 1
            elif same_predicted:
                metrics.false_positive += 1
            elif same_truth:
                metrics.false_negative += 1

    per_cluster: dict[int, set[int]] = {}
    for index, label in assignments.items():
        if truth[index] < 0:
            continue
        per_cluster.setdefault(label, set()).add(truth[index])
    metrics.impure_clusters = sum(1 for labels in per_cluster.values() if len(labels) > 1)
    return metrics


def sweep(templates, labels, quality, candidates):
    points = []
    for threshold in candidates:
        clusters, assignments = cluster(templates, quality, threshold)
        metrics = pairwise_metrics(labels, assignments, len(templates))
        points.append(
            SweepPoint(
                threshold=threshold,
                clusters=len(clusters),
                unassigned=len(templates) - len(assignments),
                metrics=metrics,
            )
        )
    return points


def choose(points):
    """Impurity first, then F1 descending, then the lower threshold.

    The ordering is the whole opinion of this script; see the module header.
    """
    return min(
        points,
        key=lambda point: (
            point.metrics.impure_clusters,
            -point.metrics.f1,
            point.threshold,
        ),
    )


def load(path: Path):
    try:
        import numpy  # noqa: PLC0415 - optional, and only needed to read a real export
    except ImportError:  # pragma: no cover - the message is the point
        raise SystemExit(
            "numpy is required to read a .npz export. Install it, or pass --self-test to "
            "run the sweep against generated data."
        ) from None
    data = numpy.load(path)
    return (
        data["templates"].astype("float32").tolist(),
        data["labels"].astype("int32").tolist(),
        data["quality"].astype("float32").tolist(),
    )


def generated(identities: int = 8, per_identity: int = 6, spread: float = 0.4):
    """A labelled corpus with known structure, for the self-test.

    Mirrors `aura_vision::face::fixtures::synthetic_template`: a deterministic direction
    per identity, perturbed per face. Two independent directions in 512 dimensions are
    very nearly orthogonal, which is what makes this usable as ground truth.
    """
    dimension = 512

    def direction(seed: int):
        state = seed | 1
        out = []
        for _ in range(dimension):
            state ^= (state << 13) & 0xFFFFFFFFFFFFFFFF
            state ^= state >> 7
            state ^= (state << 17) & 0xFFFFFFFFFFFFFFFF
            bits = (state * 0x2545F4914F6CDD1D) & 0xFFFFFFFFFFFFFFFF
            out.append(((bits >> 40) / 16777216.0) * 2.0 - 1.0)
        norm = math.sqrt(sum(value * value for value in out))
        return [value / norm for value in out]

    templates, labels, quality = [], [], []
    for identity in range(identities):
        base = direction((identity * 0x9E3779B97F4A7C15 + 1) & 0xFFFFFFFFFFFFFFFF)
        for variation in range(per_identity):
            noise = direction(
                (((identity << 32) | variation) * 0xD6E8FEB86659FD93 + 7) & 0xFFFFFFFFFFFFFFFF
            )
            vector = [b + n * spread for b, n in zip(base, noise)]
            norm = math.sqrt(sum(value * value for value in vector))
            templates.append([value / norm for value in vector])
            labels.append(identity)
            quality.append(0.8)
    return templates, labels, quality


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--templates", type=Path, help="an .npz export of one project")
    parser.add_argument("--report", type=Path, help="where to write the sweep as JSON")
    parser.add_argument(
        "--candidates",
        default="0.35,0.40,0.45,0.50,0.55,0.60,0.65,0.70",
        help="comma-separated thresholds to try",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run against generated data with known structure",
    )
    args = parser.parse_args()

    if args.self_test:
        templates, labels, quality = generated()
    elif args.templates is not None:
        templates, labels, quality = load(args.templates)
    else:
        parser.error("pass --templates or --self-test")
        return 2

    candidates = [float(value) for value in args.candidates.split(",")]
    points = sweep(templates, labels, quality, candidates)
    chosen = choose(points)

    print(f"{len(templates)} faces, {len(set(labels)) - (1 if -1 in labels else 0)} labelled people")
    print(f"{'threshold':>10} {'clusters':>9} {'unassigned':>11} {'F1':>7} {'impure':>7}")
    for point in points:
        marker = " <-" if point is chosen else ""
        print(
            f"{point.threshold:>10.3f} {point.clusters:>9} {point.unassigned:>11} "
            f"{point.metrics.f1:>7.4f} {point.metrics.impure_clusters:>7}{marker}"
        )
    print(f"chosen: {chosen.threshold:.3f} (default {DEFAULT_THRESHOLD})")

    if args.report is not None:
        args.report.write_text(
            json.dumps(
                {
                    "chosen": chosen.threshold,
                    "default": DEFAULT_THRESHOLD,
                    "points": [
                        {
                            "threshold": point.threshold,
                            "clusters": point.clusters,
                            "unassigned": point.unassigned,
                            "precision": point.metrics.precision,
                            "recall": point.metrics.recall,
                            "f1": point.metrics.f1,
                            "impure_clusters": point.metrics.impure_clusters,
                        }
                        for point in points
                    ],
                },
                indent=2,
            ),
            encoding="utf-8",
        )
        print(f"report: {args.report}")

    if args.self_test:
        # The self-test is a gate on the *script*, not on a model: generated identities
        # are separable by construction, so a sweep that cannot find a clean threshold
        # means this file is broken.
        if chosen.metrics.impure_clusters != 0 or chosen.metrics.f1 < 0.93:
            print(
                f"self-test failed: F1 {chosen.metrics.f1:.4f}, "
                f"{chosen.metrics.impure_clusters} impure clusters",
                file=sys.stderr,
            )
            return 1
        print("self-test: the sweep separates generated identities cleanly")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
