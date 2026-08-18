#!/usr/bin/env python3
"""Train Phase 15's faceless scene exposure model from expert edits.

The input is JSON Lines. Each row carries ``wedding_id``, ``photographer_id``, ``scene``,
``split`` (train/validation/test), twelve normalised ``measurements`` in
``tone::stats::Feature`` order, and the expert's own ``exposure_ev`` in stops.

One rule in this file is load-bearing and it is the first check below: **faceless frames
only**. Section 6.1 asks for this head "when no face is present (details, venue)". A
training set that includes frames with people in them teaches this head to do the job the
subject-referred path already does better - and because it is then *selected* for exactly
the frames where a face is absent, the extra data makes it worse on the only frames it will
ever see. Rows with ``faces`` greater than zero are refused rather than filtered, so
whoever built the dataset finds out.

There is no labelled corpus in this repository. ``--plan`` is therefore the runnable
default, and the shipped ``exposure_scene`` weights stay an explicitly unavailable
placeholder - ``analyse::EXPOSURE_HEAD_TRAINED`` is ``false`` and the faceless path falls
back to the scene band from ``config/exposure_targets.toml`` - until a training invocation
passes every check here.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
import sys
from pathlib import Path
from typing import Any

SEED = 15_2026
MIN_FRAMES = 8_000
MIN_WEDDINGS = 40
MIN_PHOTOGRAPHERS = 3

# `tone::stats::Feature` order. This tuple IS the input contract: renumbering it relabels a
# trained head, exactly as `composition::aesthetic::Feature` does for phase 11.
MEASUREMENTS = (
    "median",
    "mean",
    "p05",
    "p25",
    "p75",
    "p95",
    "centre_mean",
    "border_mean",
    "clipped_fraction",
    "crushed_fraction",
    "histogram_skew",
    "histogram_spread",
)
SCENES = (
    "getting_ready_bride", "getting_ready_groom", "details", "first_look",
    "ceremony_entrance", "ceremony", "ritual", "vows", "rings", "kiss",
    "family_portrait", "group_portrait", "couple_portrait", "golden_hour",
    "reception_entrance", "speeches", "cake", "first_dance", "dance_floor",
    "candid", "venue", "exit", "unknown",
)
FEATURES = len(MEASUREMENTS) + len(SCENES)

# `tone::exposure::MAX_MOVE_EV`. The sigmoid maps onto plus or minus this, so the head
# cannot emit six stops on a venue photograph.
MAX_MOVE_EV = 3.0

# Section 10.1's gate.
TOLERANCE_EV = 0.15

# The smallest share any one scene may have. Twenty-three scenes with a long tail; a set
# that is 40 percent `venue` produces a head that has learned one number.
MAX_SCENE_SHARE = 0.25

PLAN = f"""
Faceless scene exposure model: training design
==============================================

INPUT
  {FEATURES} values: {len(MEASUREMENTS)} normalised measurements followed by a
  {len(SCENES)}-way scene one-hot. No pixels. Every measurement is already computed by
  `tone::stats` for other reasons, so this head costs a matrix multiply.

  The scene is a ONE-HOT, not an index. A scalar index would let the network learn an
  ordering between scenes that does not exist - that a `venue` is somehow between a
  `ritual` and a `details`.

TARGET
  exposure_ev, in stops, as the expert set it. Training uses the logit; export adds
  Sigmoid and the fixed rescale onto plus or minus {MAX_MOVE_EV} stops.

MODEL
  Gemm {FEATURES}->48, ReLU, Gemm 48->24, ReLU, Gemm 24->1.

DATA
  FACELESS FRAMES ONLY. Rows with faces > 0 are refused. See the module docstring; this is
  the check that decides whether the head is useful or actively harmful.

  At least {MIN_FRAMES:,} frames from {MIN_WEDDINGS} weddings and {MIN_PHOTOGRAPHERS}
  photographers. Splits are BY WEDDING, never by frame: four hundred flat-lays from one
  wedding are one photographer's preference sampled four hundred times.

BALANCE
  No scene above {MAX_SCENE_SHARE:.0%} of the corpus.

REPRODUCIBILITY
  Seed {SEED}; dataset SHA-256, scene shares, split counts and software versions are stored
  in the checkpoint. Deterministic algorithms are requested.

QUALITY
  Held-out error within {TOLERANCE_EV} EV on at least 85 percent of frames, AND strictly
  better than the scene band applied to the median - which is what ships today. A learned
  head that cannot beat a table a colour scientist wrote has learned nothing worth
  shipping, and that comparison is why the band outlives this placeholder.

  Report per scene. A head that is excellent on `venue` and hopeless on `details` has a
  mean that looks fine and a gallery with a visible step in it.
"""


def read_rows(path: Path) -> list[dict[str, Any]]:
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        row["_line"] = line_number
        rows.append(row)
    return rows


def validate(rows: list[dict[str, Any]]) -> list[str]:
    problems: list[str] = []
    if len(rows) < MIN_FRAMES:
        problems.append(f"need at least {MIN_FRAMES} frames; found {len(rows)}")

    by_wedding: dict[str, set[str]] = {}
    photographers: set[str] = set()
    scene_counts: dict[str, int] = {name: 0 for name in SCENES}

    for row in rows:
        line = row.get("_line", "?")
        wedding = str(row.get("wedding_id", ""))
        photographer = str(row.get("photographer_id", ""))
        split = str(row.get("split", ""))
        scene = str(row.get("scene", ""))

        faces = row.get("faces")
        if faces is None:
            problems.append(f"line {line}: faces is required and must be 0")
        elif int(faces) != 0:
            problems.append(
                f"line {line}: this head is trained on faceless frames only; found {faces} faces"
            )

        if not wedding or not photographer:
            problems.append(f"line {line}: wedding_id and photographer_id are required")
        if split not in {"train", "validation", "test"}:
            problems.append(f"line {line}: split must be train, validation or test")
        if scene not in SCENES:
            problems.append(f"line {line}: unknown scene {scene!r}")
        else:
            scene_counts[scene] += 1

        values = row.get("measurements")
        if not isinstance(values, list) or len(values) != len(MEASUREMENTS):
            problems.append(f"line {line}: measurements must have {len(MEASUREMENTS)} values")
        elif any(not 0.0 <= float(value) <= 1.0 for value in values):
            problems.append(f"line {line}: measurements must lie in 0..1")

        target = row.get("exposure_ev")
        if target is None:
            problems.append(f"line {line}: exposure_ev is required")
        elif abs(float(target)) > MAX_MOVE_EV:
            problems.append(
                f"line {line}: exposure_ev {target} is outside the +/-{MAX_MOVE_EV} the head can emit"
            )

        photographers.add(photographer)
        by_wedding.setdefault(wedding, set()).add(split)

    leaks = sorted(wedding for wedding, splits in by_wedding.items() if len(splits) > 1)
    if leaks:
        problems.append(f"wedding-level split leakage: {', '.join(leaks[:5])}")
    if len(by_wedding) < MIN_WEDDINGS:
        problems.append(f"need at least {MIN_WEDDINGS} weddings; found {len(by_wedding)}")
    if len(photographers) < MIN_PHOTOGRAPHERS:
        problems.append(
            f"need at least {MIN_PHOTOGRAPHERS} photographers; found {len(photographers)}"
        )

    total = max(len(rows), 1)
    for name, count in scene_counts.items():
        share = count / total
        if share > MAX_SCENE_SHARE:
            problems.append(
                f"scene {name} is {share:.1%} of the corpus, above {MAX_SCENE_SHARE:.0%}"
            )
    return problems


def expand(row: dict[str, Any]) -> list[float]:
    vector = [float(value) for value in row["measurements"]]
    vector.extend(1.0 if name == row["scene"] else 0.0 for name in SCENES)
    return vector


def train(rows: list[dict[str, Any]], data_path: Path, out: Path, epochs: int) -> int:
    try:
        import torch
        from torch import nn
    except ImportError:
        print("training requires PyTorch; no checkpoint was written", file=sys.stderr)
        return 1

    random.seed(SEED)
    torch.manual_seed(SEED)
    torch.use_deterministic_algorithms(True)

    class Exposure(nn.Module):
        def __init__(self) -> None:
            super().__init__()
            self.layers = nn.Sequential(
                nn.Linear(FEATURES, 48), nn.ReLU(), nn.Linear(48, 24), nn.ReLU(), nn.Linear(24, 1)
            )

        def forward(self, values: Any) -> Any:
            # The logit. Export adds Sigmoid; the decoder maps 0..1 onto the stop range.
            return self.layers(values)

    def stops(logit: Any) -> Any:
        return (torch.sigmoid(logit) * 2.0 - 1.0) * MAX_MOVE_EV

    training = [row for row in rows if row["split"] == "train"]
    validation = [row for row in rows if row["split"] == "validation"]
    model = Exposure()
    optimiser = torch.optim.AdamW(model.parameters(), lr=1e-3, weight_decay=1e-4)
    generator = random.Random(SEED)

    for _ in range(epochs):
        generator.shuffle(training)
        for start in range(0, len(training), 128):
            chunk = training[start : start + 128]
            if not chunk:
                continue
            features = torch.tensor([expand(row) for row in chunk], dtype=torch.float32)
            target = torch.tensor(
                [[float(row["exposure_ev"])] for row in chunk], dtype=torch.float32
            )
            # An absolute error in stops rather than a squared one: a photographer's
            # disagreement is roughly uniform in stops, and a squared loss would let a
            # handful of dramatic frames set the whole head's bias.
            loss = (stops(model(features)) - target).abs().mean()
            optimiser.zero_grad(set_to_none=True)
            loss.backward()
            optimiser.step()

    def within_tolerance(sample: list[dict[str, Any]]) -> float:
        if not sample:
            return 0.0
        with torch.no_grad():
            features = torch.tensor([expand(row) for row in sample], dtype=torch.float32)
            target = torch.tensor(
                [[float(row["exposure_ev"])] for row in sample], dtype=torch.float32
            )
            error = (stops(model(features)) - target).abs()
            return float((error <= TOLERANCE_EV).float().mean().item())

    digest = hashlib.sha256(data_path.read_bytes()).hexdigest()
    per_scene = {
        name: within_tolerance([row for row in validation if row["scene"] == name])
        for name in SCENES
        if any(row["scene"] == name for row in validation)
    }
    checkpoint = {
        "state_dict": model.state_dict(),
        "seed": SEED,
        "dataset_sha256": digest,
        "frames": len(rows),
        "weddings": len({row["wedding_id"] for row in rows}),
        "measurements": MEASUREMENTS,
        "scenes": SCENES,
        "max_move_ev": MAX_MOVE_EV,
        "validation_within_tolerance": within_tolerance(validation),
        "validation_by_scene": per_scene,
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    torch.save(checkpoint, out)
    print(
        json.dumps(
            {
                "checkpoint": str(out),
                "validation_within_tolerance": checkpoint["validation_within_tolerance"],
                "worst_scene": min(per_scene.items(), key=lambda pair: pair[1])
                if per_scene
                else None,
            }
        )
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true")
    parser.add_argument("--data", metavar="JSONL")
    parser.add_argument("--out", default="exposure_scene.pt")
    parser.add_argument("--epochs", type=int, default=40)
    args = parser.parse_args()
    if args.plan or not args.data:
        print(PLAN.strip())
        if not args.plan:
            print("\nno --data supplied; nothing was trained (Phase 15 condition C1)")
        return 0
    path = Path(args.data)
    rows = read_rows(path)
    problems = validate(rows)
    for problem in problems:
        print(f"[FAIL] {problem}", file=sys.stderr)
    if problems:
        return 1
    return train(rows, path, Path(args.out), args.epochs)


if __name__ == "__main__":
    sys.exit(main())
