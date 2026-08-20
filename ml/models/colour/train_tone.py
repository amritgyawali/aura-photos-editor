#!/usr/bin/env python3
"""Train Phase 16's tone parameter model from expert edits.

The input is JSON Lines. Each row carries ``wedding_id``, ``photographer_id``, ``scene``,
``split`` (train/validation/test), eleven normalised ``measurements`` in
``colour::analyse::tone_features`` order, and the expert's own five ``targets`` -
``contrast``, ``highlights``, ``shadows``, ``whites`` and ``blacks`` - in the recipe's units.

Three rules in this file are load-bearing.

**The targets are per photographer as well as per scene.** Section 12's fourth failure mode
is "HSL fights the photographer's taste", and the tone half of that is that two photographers
grade the same ceremony differently and both are right. So the dataset must carry at least
``MIN_PHOTOGRAPHERS`` of them and the split must be by *wedding*, never by frame: a random
frame split lets the model see the same wedding on both sides and reports a validation score
that is really a memorisation score. Phase 17 is where a single photographer's taste is
learned; this head learns the consensus, and a dataset from one photographer would teach it
that the consensus is that photographer.

**A subtlety penalty is part of the loss, not a post-processing step.** Section 12's first
failure mode is over-graded results. A model trained on mean squared error alone will happily
predict a large value when it is unsure, because a large error on a rare heavy edit costs
more than a small error on a common light one. The penalty term makes the opposite trade,
which is the trade a photographer would make.

**The head never predicts a curve.** Five numbers, and the curve is solved from the scene's
contrast intent under three constraints - ADR-0033 decision 2. A head that emitted control
points would be a head that could emit non-monotone ones, and monotonicity is the one
property of this phase that is structural rather than checked.

There is no labelled corpus in this repository. ``--plan`` is therefore the runnable default,
and the shipped ``tone_model`` weights stay an explicitly unavailable placeholder -
``colour::tone::TONE_HEAD_TRAINED`` is ``false`` and the deterministic solver decides every
frame - until a training invocation passes every check here.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any

SEED = 16_2026
MIN_FRAMES = 12_000
MIN_WEDDINGS = 50
MIN_PHOTOGRAPHERS = 5

# `colour::analyse::tone_features` order. This tuple IS the input contract: renumbering it
# relabels a trained head, exactly as phase 15's `MEASUREMENTS` does.
MEASUREMENTS = (
    "p01",
    "p05",
    "p50",
    "p95",
    "p99",
    "range",
    "subject_spread",
    "subject_luma",
    "clipped_fraction",
    "crushed_fraction",
    "shadow_headroom",
)

# The five outputs, in the order `colour::analyse::Head::predict` decodes them.
TARGETS = ("contrast", "highlights", "shadows", "whites", "blacks")

# The bound each output is decoded onto, from `colour::tone`. A sigmoid maps `0..1` onto
# plus or minus these, so a training target outside them is one the head can never reach and
# is a dataset error rather than a hard example.
BOUNDS = {
    "contrast": 50.0,
    "highlights": 60.0,
    "shadows": 60.0,
    "whites": 40.0,
    "blacks": 40.0,
}

SCENES = (
    "getting_ready_bride", "getting_ready_groom", "details", "first_look",
    "ceremony_entrance", "ceremony", "ritual", "vows", "rings", "kiss",
    "family_portrait", "group_portrait", "couple_portrait", "golden_hour",
    "reception_entrance", "speeches", "cake", "first_dance", "dance_floor",
    "candid", "venue", "exit", "unknown",
)

# The weight of the subtlety penalty in the loss. See the module docstring: it is what makes
# the model prefer a small wrong answer to a large one, which is the trade a photographer
# makes and the opposite of what mean squared error alone rewards.
SUBTLETY_WEIGHT = 0.25

# The ceiling the penalty is measured against, matching `SUBTLETY_CEILING` in
# `aura_core::contract::colour`.
SUBTLETY_CEILING = 0.45


def load(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open(encoding="utf-8") as handle:
        for number, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as error:
                raise SystemExit(f"{path}:{number}: {error}") from error
    return rows


def check(rows: list[dict[str, Any]]) -> list[str]:
    """Every reason this dataset would train a head nobody should ship."""
    problems: list[str] = []
    if len(rows) < MIN_FRAMES:
        problems.append(f"{len(rows)} frames; at least {MIN_FRAMES} are needed")

    weddings = {row.get("wedding_id") for row in rows}
    photographers = {row.get("photographer_id") for row in rows}
    if len(weddings) < MIN_WEDDINGS:
        problems.append(f"{len(weddings)} weddings; at least {MIN_WEDDINGS} are needed")
    if len(photographers) < MIN_PHOTOGRAPHERS:
        problems.append(
            f"{len(photographers)} photographers; at least {MIN_PHOTOGRAPHERS} are needed, "
            "because a head trained on one person learns that person's taste and calls it "
            "the consensus"
        )

    # The split must be by wedding. A wedding on both sides of it turns the validation score
    # into a memorisation score, and every number downstream of that is decoration.
    by_split: dict[str, set[Any]] = {}
    for row in rows:
        by_split.setdefault(str(row.get("split", "")), set()).add(row.get("wedding_id"))
    for left in ("train", "validation", "test"):
        for right in ("train", "validation", "test"):
            if left >= right:
                continue
            shared = by_split.get(left, set()) & by_split.get(right, set())
            if shared:
                problems.append(
                    f"{len(shared)} weddings appear in both the {left} and {right} splits; "
                    "the split must be by wedding"
                )

    for index, row in enumerate(rows):
        measurements = row.get("measurements", [])
        if len(measurements) != len(MEASUREMENTS):
            problems.append(
                f"row {index}: {len(measurements)} measurements, expected {len(MEASUREMENTS)}"
            )
            break
        targets = row.get("targets", {})
        for name in TARGETS:
            if name not in targets:
                problems.append(f"row {index}: no `{name}` target")
                break
            value = float(targets[name])
            if abs(value) > BOUNDS[name]:
                problems.append(
                    f"row {index}: `{name}` is {value}, outside the {BOUNDS[name]} the head "
                    "can decode; that is a dataset error rather than a hard example"
                )
                break
        if row.get("scene") not in SCENES:
            problems.append(f"row {index}: `{row.get('scene')}` is not in the taxonomy")
            break

    # Scene coverage. A head conditioned on twenty-three scenes and trained on four of them
    # is a head that has learned four scenes and a prior.
    counts = Counter(str(row.get("scene", "")) for row in rows)
    thin = [scene for scene in SCENES if scene != "unknown" and counts[scene] < 100]
    if thin:
        problems.append(
            f"{len(thin)} scenes have fewer than 100 frames: {', '.join(thin[:6])}"
            + ("..." if len(thin) > 6 else "")
        )

    return problems


def plan() -> dict[str, Any]:
    """What a real training run would do, printed rather than done."""
    return {
        "seed": SEED,
        "architecture": {
            "input": len(MEASUREMENTS) + len(SCENES),
            "hidden": [64, 32],
            "output": len(TARGETS),
            "activation": "relu",
            "head": "sigmoid, decoded onto the per-parameter bounds",
        },
        "loss": {
            "primary": "mean squared error over the five decoded parameters",
            "penalty": f"{SUBTLETY_WEIGHT} * relu(subtlety - {SUBTLETY_CEILING})",
            "why": (
                "mean squared error alone rewards a large prediction when the model is "
                "unsure, because a large error on a rare heavy edit costs more than a small "
                "error on a common light one. The penalty makes the opposite trade."
            ),
        },
        "split": "by wedding, never by frame",
        "requirements": {
            "frames": MIN_FRAMES,
            "weddings": MIN_WEDDINGS,
            "photographers": MIN_PHOTOGRAPHERS,
            "frames_per_scene": 100,
        },
        "after_training": [
            "ml/models/colour/export.py --check",
            "cargo test -p aura-brain-photo --test colour_eval",
            "ml/models/colour/eval_colour.py --input <held-out>.json",
            "flip colour::tone::TONE_HEAD_TRAINED once the gates pass",
            "close condition C1 in docs/progress/PHASE-16-EXIT.md",
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, help="training rows, as JSON Lines")
    parser.add_argument("--plan", action="store_true", help="print the training plan")
    parser.add_argument("--check", action="store_true", help="validate a dataset and stop")
    args = parser.parse_args(argv)

    if args.plan or not args.input:
        print(json.dumps(plan(), indent=2))
        if not args.input:
            print(
                "\nno dataset supplied; nothing was trained. "
                "`tone_model` remains an untrained placeholder and "
                "`TONE_HEAD_TRAINED` stays false.",
                file=sys.stderr,
            )
        return 0

    rows = load(args.input)
    problems = check(rows)
    for problem in problems:
        print(f"FAIL: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"dataset accepted: {len(rows)} frames")
    if args.check:
        return 0

    print(
        "training needs PyTorch, which this repository does not vendor. "
        "Run `ml/models/colour/export.py --check` after training to verify the graph.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
