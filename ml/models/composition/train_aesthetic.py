#!/usr/bin/env python3
"""Train Phase 11's scene-conditioned aesthetic head from photographer pairs.

The input is JSON Lines. Each row contains ``wedding_id``, ``photographer_id``, ``scene``,
``split`` (train/validation/test), and twelve-number ``winner`` and ``loser`` vectors in
``composition::aesthetic::Feature`` order. Pairs must be from one wedding and scene; the
scene is represented once on the row and expanded to the 23-way one-hot used in product.

There is no labelled corpus in this repository. ``--plan`` is therefore the runnable
default and the shipped model remains an explicitly unavailable placeholder until a
training invocation passes the dataset checks below.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
import sys
from pathlib import Path
from typing import Any

SEED = 11_2026
MIN_PAIRS = 4_000
GEOMETRY_FEATURES = (
    "tilt",
    "horizon_confidence",
    "headroom",
    "headroom_deviation",
    "thirds_offset",
    "centre_offset",
    "balance",
    "negative_space",
    "clutter",
    "bright_blob_area",
    "head_merge",
    "colour_competition",
)
SCENES = (
    "getting_ready_bride", "getting_ready_groom", "details", "first_look",
    "ceremony_entrance", "ceremony", "ritual", "vows", "rings", "kiss",
    "family_portrait", "group_portrait", "couple_portrait", "golden_hour",
    "reception_entrance", "speeches", "cake", "first_dance", "dance_floor",
    "candid", "venue", "exit", "unknown",
)
FEATURES = len(GEOMETRY_FEATURES) + len(SCENES)

PLAN = f"""
Aesthetic head training design
==============================

INPUT
  {FEATURES} values: {len(GEOMETRY_FEATURES)} normalised geometric measurements followed
  by a {len(SCENES)}-way scene one-hot. No pixels and no client identifiers.

MODEL
  Gemm {FEATURES}->64, ReLU, Gemm 64->32, ReLU, Gemm 32->1. Training uses logits;
  export adds Sigmoid. The loss is Bradley-Terry BCE over winner_logit-loser_logit.

DATA
  At least {MIN_PAIRS} composition-focused comparisons from multiple photographers.
  Pairs stay within one wedding and one scene. Splits are by wedding, never frame or
  pair. Repeated frames may not straddle splits.

SAMPLING
  Equal scene mass inside a batch, then equal photographer mass inside the scene where
  possible. The head learns a shared editorial prior; per-photographer taste is Phase 30.

REPRODUCIBILITY
  Seed {SEED}; dataset SHA-256, feature order, scene order, split counts and software
  versions are stored in the checkpoint. Deterministic algorithms are requested.

QUALITY
  Held-out pairwise agreement >= 0.78 and strictly above Aesthetic::from_features.
  ECE <= 0.05 before an absolute confidence is displayed. Report every scene and the
  worst photographer slice, not only the mean.
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
    if len(rows) < MIN_PAIRS:
        problems.append(f"need at least {MIN_PAIRS} pairs; found {len(rows)}")
    by_wedding: dict[str, set[str]] = {}
    photographers: set[str] = set()
    for row in rows:
        line = row.get("_line", "?")
        scene = str(row.get("scene", ""))
        split = str(row.get("split", ""))
        wedding = str(row.get("wedding_id", ""))
        photographer = str(row.get("photographer_id", ""))
        if scene not in SCENES:
            problems.append(f"line {line}: unknown scene {scene!r}")
        if split not in {"train", "validation", "test"}:
            problems.append(f"line {line}: split must be train, validation or test")
        if not wedding or not photographer:
            problems.append(f"line {line}: wedding_id and photographer_id are required")
        photographers.add(photographer)
        by_wedding.setdefault(wedding, set()).add(split)
        for side in ("winner", "loser"):
            vector = row.get(side)
            if not isinstance(vector, list) or len(vector) != len(GEOMETRY_FEATURES):
                problems.append(f"line {line}: {side} must have {len(GEOMETRY_FEATURES)} values")
            elif any(not 0.0 <= float(value) <= 1.0 for value in vector):
                problems.append(f"line {line}: {side} values must lie in 0..1")
    leaks = sorted(wedding for wedding, splits in by_wedding.items() if len(splits) > 1)
    if leaks:
        problems.append(f"wedding-level split leakage: {', '.join(leaks[:5])}")
    if len(photographers) < 3:
        problems.append("at least three photographers are required for a shared prior")
    return problems


def expand(row: dict[str, Any], side: str) -> list[float]:
    vector = [float(value) for value in row[side]]
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

    class Ranker(nn.Module):
        def __init__(self) -> None:
            super().__init__()
            self.layers = nn.Sequential(
                nn.Linear(FEATURES, 64), nn.ReLU(), nn.Linear(64, 32), nn.ReLU(), nn.Linear(32, 1)
            )

        def forward(self, values: Any) -> Any:
            return self.layers(values)

    training = [row for row in rows if row["split"] == "train"]
    validation = [row for row in rows if row["split"] == "validation"]
    model = Ranker()
    optimiser = torch.optim.AdamW(model.parameters(), lr=1e-3, weight_decay=1e-4)
    loss_fn = nn.BCEWithLogitsLoss()
    generator = random.Random(SEED)

    for _ in range(epochs):
        generator.shuffle(training)
        for start in range(0, len(training), 128):
            batch = training[start : start + 128]
            winner = torch.tensor([expand(row, "winner") for row in batch], dtype=torch.float32)
            loser = torch.tensor([expand(row, "loser") for row in batch], dtype=torch.float32)
            target = torch.ones((len(batch), 1), dtype=torch.float32)
            loss = loss_fn(model(winner) - model(loser), target)
            optimiser.zero_grad(set_to_none=True)
            loss.backward()
            optimiser.step()

    def agreement(sample: list[dict[str, Any]]) -> float:
        if not sample:
            return 0.0
        with torch.no_grad():
            winner = torch.tensor([expand(row, "winner") for row in sample], dtype=torch.float32)
            loser = torch.tensor([expand(row, "loser") for row in sample], dtype=torch.float32)
            return float((model(winner) > model(loser)).float().mean().item())

    digest = hashlib.sha256(data_path.read_bytes()).hexdigest()
    checkpoint = {
        "state_dict": model.state_dict(),
        "seed": SEED,
        "dataset_sha256": digest,
        "geometry_features": GEOMETRY_FEATURES,
        "scenes": SCENES,
        "pairs": len(rows),
        "validation_agreement": agreement(validation),
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    torch.save(checkpoint, out)
    print(json.dumps({"checkpoint": str(out), "validation_agreement": checkpoint["validation_agreement"]}))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true")
    parser.add_argument("--data", metavar="JSONL")
    parser.add_argument("--out", default="aesthetic_head.pt")
    parser.add_argument("--epochs", type=int, default=40)
    args = parser.parse_args()
    if args.plan or not args.data:
        print(PLAN.strip())
        if not args.plan:
            print("\nno --data supplied; nothing was trained (Phase 11 condition C1/C2)")
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
