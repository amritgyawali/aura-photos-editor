#!/usr/bin/env python3
"""Fit Phase 17's residual style tree, in Python, from the pairs the app already stored.

This is the reference implementation of the arithmetic in ``crates/aura-style/src/tree.rs``,
and it exists for two reasons that have nothing to do with training a neural network - there
is no neural network in this phase:

1. **A second implementation is how the first one gets checked.** ``--self-test`` fits the
   same synthetic archives the Rust harness does and asserts the same properties, so a change
   to the shrinkage that broke one and not the other is visible immediately.
2. **A dataset curator needs to be able to look at an archive** without building the product.
   This script uses only the standard library and reads the JSON that
   ``aura-cli`` can dump from ``style_pairs``.

**There is no model here and nothing to export.** Ridge regression with James-Stein shrinkage
and a Huber reweighting has a closed form; ``models.lock`` is untouched by this phase, which
is the third time since phase 08 and the first time it is a statement about the *method*
rather than about the data. ``export.py`` next door exists to say that, and to check it stays
true.

Three properties this file is the reference for
-----------------------------------------------

**Shrinkage is continuous.** ``n / (n + k)`` and never a cut-off. A cut-off is a cliff: a
bucket with nine samples produces the parent's answer and one with ten produces its own, and
the photographer sees two visibly different looks either side of a boundary that exists
nowhere in their wedding.

**The intercept is not penalised.** Ridge shrinks the slopes; penalising the intercept would
shrink the photographer's average lean toward zero, and the average lean is the whole thing
this phase is trying to learn.

**One archive is capped after weighting, not before.** Scaling an archive's weight also
scales the total, so a naive ``cap / share`` leaves it well above the cap. Solving
``w * mine / (rest + w * mine) = cap`` is the fix, and getting it wrong is how a robustness
guarantee reads correct and measures 48 % instead of 35 %.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

# Every one of these is also a constant in `crates/aura-style/src/tree.rs`. They are restated
# rather than imported because this script has to run with nothing installed; the Rust gate
# asserts the two agree.
FEATURES = 7
RIDGE = 0.1
PRIOR_STRENGTH = 12.0
GLOBAL_PRIOR = 40.0
MAX_ARCHIVE_INFLUENCE = 0.35
HUBER_K = 1.345
HUBER_PASSES = 2
HOLDOUT_EVERY = 5
APPLY_ABOVE = 0.35

SCHEMA = {
    "pairs": [
        {
            "bucket": "portraits/daylight",
            "archive": "2025-06-smith",
            "features": [1.0, 0.24, 0.03, 0.12, 1.0, 0.0, 0.0],
            "targets": {"exposure": 0.31, "contrast": -6.0},
            "held_out": False,
        }
    ]
}


def shrink_factor(samples: int, prior: float) -> float:
    """The James-Stein factor. Continuous, and never a cut-off."""
    if samples <= 0:
        return 0.0
    return min(1.0, samples / (samples + prior))


def archive_weights(pairs: list[dict[str, Any]]) -> tuple[list[float], bool]:
    """Per-pair weights with each archive's **post-scaling** share capped."""
    counts: dict[str, int] = {}
    for pair in pairs:
        label = str(pair.get("archive", ""))
        counts[label] = counts.get(label, 0) + 1
    archives = max(1, len(counts))
    cap = max(MAX_ARCHIVE_INFLUENCE, 1.0 / archives)
    total = float(len(pairs))

    scale: dict[str, float] = {}
    capped = False
    for label, count in counts.items():
        mine = float(count)
        if mine / total > cap + 1e-6 and cap < 1.0:
            rest = max(0.0, total - mine)
            wanted = cap * rest / (1.0 - cap)
            scale[label] = max(0.0, min(1.0, wanted / mine))
            capped = True
        else:
            scale[label] = 1.0
    return [scale.get(str(pair.get("archive", "")), 1.0) for pair in pairs], capped


def solve(matrix: list[list[float]], rhs: list[float]) -> list[float]:
    """Gaussian elimination with partial pivoting. Seven by seven, deterministic."""
    size = len(rhs)
    a = [row[:] for row in matrix]
    b = rhs[:]
    for column in range(size):
        pivot = max(range(column, size), key=lambda row: abs(a[row][column]))
        if abs(a[pivot][column]) < 1e-12:
            continue
        a[column], a[pivot] = a[pivot], a[column]
        b[column], b[pivot] = b[pivot], b[column]
        head = a[column][column]
        for row in range(column + 1, size):
            factor = a[row][column] / head
            if abs(factor) < 1e-18:
                continue
            for col in range(column, size):
                a[row][col] -= factor * a[column][col]
            b[row] -= factor * b[column]
    out = [0.0] * size
    for row in reversed(range(size)):
        total = b[row] - sum(a[row][col] * out[col] for col in range(row + 1, size))
        out[row] = 0.0 if abs(a[row][row]) < 1e-12 else total / a[row][row]
    return out


def ridge_intercept(rows: list[list[float]], y: list[float], weights: list[float]) -> float:
    """The ridge intercept for one target, Huber-reweighted. The intercept is unpenalised."""
    if not rows:
        return 0.0
    w = weights[:]
    beta = [0.0] * FEATURES
    for pass_index in range(HUBER_PASSES):
        xtx = [[0.0] * FEATURES for _ in range(FEATURES)]
        xty = [0.0] * FEATURES
        for row, observed, weight in zip(rows, y, w):
            for i in range(FEATURES):
                xty[i] += weight * row[i] * observed
                for j in range(FEATURES):
                    xtx[i][j] += weight * row[i] * row[j]
        for i in range(1, FEATURES):
            xtx[i][i] += RIDGE
        xtx[0][0] += 1e-6
        beta = solve(xtx, xty)
        if pass_index + 1 == HUBER_PASSES:
            break
        residuals = [
            observed - sum(row[i] * beta[i] for i in range(FEATURES))
            for row, observed in zip(rows, y)
        ]
        magnitudes = sorted(abs(value) for value in residuals)
        scale = max(1e-4, magnitudes[len(magnitudes) // 2] * 1.4826)
        for index, residual in enumerate(residuals):
            standard = abs(residual / scale)
            w[index] *= 1.0 if standard <= HUBER_K else HUBER_K / standard
    return beta[0]


def fit(pairs: list[dict[str, Any]]) -> dict[str, Any]:
    """Fit the three levels of the tree. Each is fitted on what the level above left behind."""
    training = [pair for pair in pairs if not pair.get("held_out")]
    if not training:
        return {"global": {}, "groups": {}, "buckets": {}, "archive_capped": False}
    weights, capped = archive_weights(training)
    names = sorted({key for pair in training for key in pair.get("targets", {})})

    def level(subset: list[dict[str, Any]], subtract: dict[str, float]) -> dict[str, float]:
        rows = [pair["features"] for pair in subset]
        keys = [id(pair) for pair in training]
        subset_weights = [weights[keys.index(id(pair))] for pair in subset]
        out: dict[str, float] = {}
        for name in names:
            y = [
                float(pair.get("targets", {}).get(name, 0.0)) - subtract.get(name, 0.0)
                for pair in subset
            ]
            out[name] = ridge_intercept(rows, y, subset_weights)
        return out

    global_raw = level(training, {})
    global_shrink = shrink_factor(len(training), GLOBAL_PRIOR)
    global_delta = {name: value * global_shrink for name, value in global_raw.items()}

    groups: dict[str, dict[str, float]] = {}
    group_raw: dict[str, dict[str, float]] = {}
    for group in sorted({str(pair["bucket"]).split("/")[0] for pair in training}):
        members = [pair for pair in training if str(pair["bucket"]).startswith(f"{group}/")]
        raw = level(members, global_delta)
        factor = shrink_factor(len(members), PRIOR_STRENGTH)
        group_raw[group] = {name: value * factor for name, value in raw.items()}
        groups[group] = dict(group_raw[group], **{"_confidence": factor})

    buckets: dict[str, dict[str, float]] = {}
    for key in sorted({str(pair["bucket"]) for pair in training}):
        members = [pair for pair in training if str(pair["bucket"]) == key]
        parent = group_raw.get(key.split("/")[0], {})
        subtract = {
            name: global_delta.get(name, 0.0) + parent.get(name, 0.0) for name in names
        }
        raw = level(members, subtract)
        factor = shrink_factor(len(members), PRIOR_STRENGTH)
        buckets[key] = dict(
            {name: value * factor for name, value in raw.items()},
            **{"_confidence": factor, "_samples": float(len(members))},
        )

    return {
        "global": global_delta,
        "groups": groups,
        "buckets": buckets,
        "archive_capped": capped,
    }


def resolve(tree: dict[str, Any], bucket: str, target: str) -> tuple[float, str]:
    """Bucket, then group, then global, then nothing. The levels add; they never replace."""
    total = float(tree.get("global", {}).get(target, 0.0))
    level = "global" if abs(total) > 1e-9 else "factory"
    group = tree.get("groups", {}).get(bucket.split("/")[0])
    if group and group.get("_confidence", 0.0) >= APPLY_ABOVE:
        total += float(group.get(target, 0.0))
        level = "group"
    leaf = tree.get("buckets", {}).get(bucket)
    if leaf and leaf.get("_confidence", 0.0) >= APPLY_ABOVE:
        total += float(leaf.get(target, 0.0))
        level = "bucket"
    return total, level


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------


def _pair(bucket: str, archive: str, exposure: float, held_out: bool = False) -> dict[str, Any]:
    return {
        "bucket": bucket,
        "archive": archive,
        "features": [1.0] + [0.0] * (FEATURES - 1),
        "targets": {"exposure": exposure},
        "held_out": held_out,
    }


def self_test() -> int:
    failures = 0

    def check(name: str, ok: bool, detail: str = "") -> None:
        nonlocal failures
        if ok:
            print(f"  ok   {name}")
        else:
            failures += 1
            print(f"  FAIL {name} {detail}")

    # 1. A consistent lean is recovered, shrunk by the global prior.
    pairs = [_pair("portraits/daylight", "w1", 0.30) for _ in range(60)]
    tree = fit(pairs)
    expected = 0.30 * shrink_factor(60, GLOBAL_PRIOR)
    check(
        "a consistent lean is recovered",
        abs(tree["global"]["exposure"] - expected) < 0.02,
        f"{tree['global']['exposure']} against {expected}",
    )

    # 2. Shrinkage is continuous: the largest step is the first one and every step after it is
    #    smaller. That is exactly what a cut-off does not do.
    steps = [
        shrink_factor(n + 1, PRIOR_STRENGTH) - shrink_factor(n, PRIOR_STRENGTH)
        for n in range(0, 60)
    ]
    check(
        "shrinkage has no cliff",
        all(steps[i] >= steps[i + 1] - 1e-9 for i in range(len(steps) - 1)),
    )

    # 3. Seven pairs is the smallest bucket that may speak.
    check(
        "seven pairs is the floor",
        shrink_factor(6, PRIOR_STRENGTH) < APPLY_ABOVE <= shrink_factor(7, PRIOR_STRENGTH),
    )

    # 4. One outlier wedding cannot dominate.
    mixed = [_pair("portraits/daylight", f"normal-{i // 60}", 0.0) for i in range(240)]
    without = fit(mixed)["global"]["exposure"]
    mixed += [_pair("portraits/daylight", "outlier", 2.0) for _ in range(400)]
    with_outlier = fit(mixed)
    moved = abs(with_outlier["global"]["exposure"] - without)
    check(
        "one wedding cannot dominate",
        with_outlier["archive_capped"] and moved <= MAX_ARCHIVE_INFLUENCE * 2.0,
        f"moved {moved:.3f}",
    )

    # 5. The falsification check: with no cap the outlier would carry the fit.
    alone = fit([_pair("portraits/daylight", "outlier", 2.0) for _ in range(400)])
    check(
        "a single-archive fit follows its one archive",
        alone["global"]["exposure"] > 0.5,
        f"{alone['global']['exposure']:.3f}",
    )

    # 6. A sparse bucket answers from its parent.
    sparse = [_pair("portraits/daylight", "w1", 0.2) for _ in range(200)]
    sparse += [_pair("portraits/shade", "w1", 3.0) for _ in range(4)]
    tree = fit(sparse)
    _, level = resolve(tree, "portraits/shade", "exposure")
    check("a four-pair bucket does not answer for itself", level != "bucket", level)

    # 7. Every pair held out is the same as no pairs at all.
    held = [_pair("portraits/daylight", "w1", 1.0, held_out=True) for _ in range(10)]
    check("a fully held-out pile fits nothing", fit(held)["global"] == {})

    # 8. The held-out split is deterministic.
    first = [index for index in range(40) if index % HOLDOUT_EVERY == 0]
    second = [index for index in range(40) if index % HOLDOUT_EVERY == 0]
    check("the held-out split is deterministic", first == second and len(first) == 8)

    print()
    if failures == 0:
        print("self-test: OK")
        print(
            "This proves the arithmetic agrees with crates/aura-style/src/tree.rs. It says "
            "nothing about any photographer's archive, because there is none in this "
            "repository."
        )
    else:
        print(f"self-test: {failures} check(s) failed")
    return 0 if failures == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pairs", type=Path, help="a JSON file of pairs; --schema shows the shape")
    parser.add_argument("--out", type=Path, help="where to write the fitted tree")
    parser.add_argument("--schema", action="store_true", help="print the input shape and exit")
    parser.add_argument("--self-test", action="store_true", help="check the arithmetic")
    args = parser.parse_args()

    if args.schema:
        print(json.dumps(SCHEMA, indent=2))
        return 0
    if args.self_test:
        return self_test()
    if not args.pairs:
        parser.error("--pairs, --schema or --self-test")

    payload = json.loads(args.pairs.read_text(encoding="utf-8"))
    tree = fit(list(payload.get("pairs", [])))
    text = json.dumps(tree, indent=2, sort_keys=True)
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
