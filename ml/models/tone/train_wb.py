#!/usr/bin/env python3
"""Train Phase 15's learned illuminant predictor from expert white-balance edits.

The input is JSON Lines. Each row carries ``wedding_id``, ``photographer_id``,
``lighting_class``, ``split`` (train/validation/test), a ``thumb`` of 3x64x64 **linear**
values in ``0..1``, and the expert's own answer as ``illuminant_uv`` - a chromaticity in
CIE 1976 ``u'v'``.

Two things about this file are deliberate and are the reason it exists as code rather than
as a paragraph in a model card.

**It refuses to build a kelvin target.** ``--data`` rows carrying ``temperature_k`` instead
of ``illuminant_uv`` are rejected rather than converted. A correlated colour temperature is
the projection of a chromaticity onto the Planckian locus, and most reception lighting is
nowhere near that locus; training on kelvin trains the head to discard the very error it
exists to measure. The conversion belongs in the dataset builder, once, using the same
function the product uses (``aura_raw::colour::illuminant``), and it belongs in the
direction chromaticity-to-kelvin only.

**It refuses a dataset that is mostly daylight.** Section 9 names eight lighting classes.
A corpus that is 70 percent outdoor daylight produces a head that has learned "5,500 K" and
a held-out score that says it is excellent, because the held-out set is also 70 percent
outdoor daylight. The stratification check below is the only thing standing between that
and a shipped model.

There is no labelled corpus in this repository. ``--plan`` is therefore the runnable
default, and the shipped ``white_balance`` weights stay an explicitly unavailable
placeholder - ``analyse::WB_HEAD_TRAINED`` is ``false`` and the learned hypothesis is never
generated - until a training invocation passes every check here.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import sys
from pathlib import Path
from typing import Any

SEED = 15_2026
SIDE = 64
CHANNELS = 3
OUTPUTS = 2
MIN_FRAMES = 20_000
MIN_WEDDINGS = 40
MIN_PHOTOGRAPHERS = 3

# Section 9's eight lighting classes. The names are the dataset's contract; the product
# has its own `IlluminantKind` vocabulary and the two are deliberately not the same list -
# `mixed` is a property of a frame rather than of a light.
LIGHTING_CLASSES = (
    "outdoor_daylight",
    "open_shade",
    "overcast",
    "tungsten",
    "fluorescent",
    "led_panel",
    "candle",
    "mixed",
)

# The smallest share of the corpus any one class may have, and the largest. Eight classes
# means a uniform corpus is 12.5 percent each; the band is wide enough to be collectable
# and narrow enough that no class can dominate the loss.
MIN_CLASS_SHARE = 0.05
MAX_CLASS_SHARE = 0.30

# The gate, from section 10.1, restated in the space the head actually predicts. 200 K near
# daylight is about 0.004 in u'v'; the figure is checked against the product's own
# tolerance by `eval_tone.py` rather than approximated twice.
UV_TOLERANCE = 0.004

PLAN = f"""
Learned illuminant predictor: training design
=============================================

INPUT
  thumb [3, {SIDE}, {SIDE}], LINEAR sRGB in 0..1. Not gamma-encoded, and this is not a
  preference: an illuminant estimate is a statement about a ratio of channel energies, and
  a transfer curve applied before the first convolution makes that ratio depend on
  brightness. Build it with the same downsample the product uses
  (`tone::stats::FrameStats::thumbnail`), never with a library resize on an sRGB JPEG.

TARGET
  illuminant_uv [2], CIE 1976 u'v'. NOT temperature and tint. Rows carrying
  `temperature_k` are refused; convert in the dataset builder, in that direction only.

MODEL
  Conv 16 s2, ReLU, Conv 32 s2, ReLU, MaxPool 2, Conv 48 s2, ReLU, GlobalAveragePool,
  Gemm 48->64, ReLU, Gemm 64->2. Training uses logits; export adds Sigmoid and the fixed
  rescale onto the u'v' box, so the head cannot propose a chromaticity off the diagram.

LOSS
  Angular error in u'v', not squared error in kelvin. A 200 K error at 2,800 K is a visible
  shift and a 200 K error at 9,000 K is nothing, so a kelvin loss weights the easy end of
  the range roughly four times too heavily.

DATA
  At least {MIN_FRAMES:,} frames from at least {MIN_WEDDINGS} weddings and
  {MIN_PHOTOGRAPHERS} photographers. Splits are BY WEDDING, never by frame: two frames from
  one reception share an illuminant, and a frame-level split makes the held-out error a
  measurement of memorisation.

STRATIFICATION
  Every one of the {len(LIGHTING_CLASSES)} lighting classes between {MIN_CLASS_SHARE:.0%}
  and {MAX_CLASS_SHARE:.0%} of the corpus. This is the check that a real dataset will fail
  first and it is the one worth fixing rather than relaxing.

REPRODUCIBILITY
  Seed {SEED}; dataset SHA-256, class shares, split counts and software versions are stored
  in the checkpoint. Deterministic algorithms are requested.

QUALITY
  Held-out agreement within {UV_TOLERANCE} u'v' on at least 85 percent of frames, AND
  strictly better than the four arithmetic hypotheses on the `mixed` and `led_panel`
  subsets. A learned head that cannot beat grey-world plus white-patch plus neutrals on the
  frames those fail is a head that has learned to agree with them, which is worth nothing.
  Report every class and the worst wedding slice, not only the mean.

FAIRNESS
  `eval_tone.py` measures skin dE00 per Monk-scale bucket. It is a gate on the SOLVE rather
  than on this head, because the skin constraint is what bounds a wrong hypothesis - but a
  head whose errors correlate with skin tone will show up there, and that is the number to
  look at before shipping.
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
    class_counts: dict[str, int] = {name: 0 for name in LIGHTING_CLASSES}
    expected = CHANNELS * SIDE * SIDE

    for row in rows:
        line = row.get("_line", "?")
        wedding = str(row.get("wedding_id", ""))
        photographer = str(row.get("photographer_id", ""))
        split = str(row.get("split", ""))
        lighting = str(row.get("lighting_class", ""))

        if "temperature_k" in row and "illuminant_uv" not in row:
            problems.append(
                f"line {line}: rows carry illuminant_uv, not temperature_k; "
                "convert in the dataset builder"
            )
        if not wedding or not photographer:
            problems.append(f"line {line}: wedding_id and photographer_id are required")
        if split not in {"train", "validation", "test"}:
            problems.append(f"line {line}: split must be train, validation or test")
        if lighting not in LIGHTING_CLASSES:
            problems.append(f"line {line}: unknown lighting_class {lighting!r}")
        else:
            class_counts[lighting] += 1

        thumb = row.get("thumb")
        if not isinstance(thumb, list) or len(thumb) != expected:
            problems.append(f"line {line}: thumb must have {expected} values")
        elif any(not 0.0 <= float(value) <= 1.0 for value in thumb):
            problems.append(f"line {line}: thumb values must lie in 0..1")

        target = row.get("illuminant_uv")
        if not isinstance(target, list) or len(target) != OUTPUTS:
            problems.append(f"line {line}: illuminant_uv must have {OUTPUTS} values")

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
    for name, count in class_counts.items():
        share = count / total
        if share < MIN_CLASS_SHARE:
            problems.append(
                f"lighting class {name} is {share:.1%} of the corpus, below {MIN_CLASS_SHARE:.0%}"
            )
        elif share > MAX_CLASS_SHARE:
            problems.append(
                f"lighting class {name} is {share:.1%} of the corpus, above {MAX_CLASS_SHARE:.0%}"
            )
    return problems


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

    class Illuminant(nn.Module):
        def __init__(self) -> None:
            super().__init__()
            self.trunk = nn.Sequential(
                nn.Conv2d(CHANNELS, 16, 3, stride=2, padding=1),
                nn.ReLU(),
                nn.Conv2d(16, 32, 3, stride=2, padding=1),
                nn.ReLU(),
                nn.MaxPool2d(2),
                nn.Conv2d(32, 48, 3, stride=2, padding=1),
                nn.ReLU(),
                nn.AdaptiveAvgPool2d(1),
                nn.Flatten(),
            )
            self.head = nn.Sequential(nn.Linear(48, 64), nn.ReLU(), nn.Linear(64, OUTPUTS))

        def forward(self, values: Any) -> Any:
            return self.head(self.trunk(values))

    def batch_of(sample: list[dict[str, Any]]) -> tuple[Any, Any]:
        thumbs = torch.tensor(
            [row["thumb"] for row in sample], dtype=torch.float32
        ).reshape(len(sample), CHANNELS, SIDE, SIDE)
        targets = torch.tensor(
            [row["illuminant_uv"] for row in sample], dtype=torch.float32
        )
        return thumbs, targets

    training = [row for row in rows if row["split"] == "train"]
    validation = [row for row in rows if row["split"] == "validation"]
    model = Illuminant()
    optimiser = torch.optim.AdamW(model.parameters(), lr=1e-3, weight_decay=1e-4)
    generator = random.Random(SEED)

    for _ in range(epochs):
        generator.shuffle(training)
        for start in range(0, len(training), 64):
            chunk = training[start : start + 64]
            if not chunk:
                continue
            thumbs, targets = batch_of(chunk)
            # Euclidean distance in u'v', which IS the perceptual distance this space was
            # built to make uniform. Summed as a norm rather than a squared error so one
            # badly wrong frame does not dominate a batch of nearly-right ones.
            loss = torch.linalg.vector_norm(model(thumbs) - targets, dim=1).mean()
            optimiser.zero_grad(set_to_none=True)
            loss.backward()
            optimiser.step()

    def within_tolerance(sample: list[dict[str, Any]]) -> float:
        if not sample:
            return 0.0
        with torch.no_grad():
            thumbs, targets = batch_of(sample)
            error = torch.linalg.vector_norm(model(thumbs) - targets, dim=1)
            return float((error <= UV_TOLERANCE).float().mean().item())

    def by_class(sample: list[dict[str, Any]]) -> dict[str, float]:
        out: dict[str, float] = {}
        for name in LIGHTING_CLASSES:
            subset = [row for row in sample if row["lighting_class"] == name]
            out[name] = within_tolerance(subset)
        return out

    digest = hashlib.sha256(data_path.read_bytes()).hexdigest()
    total = max(len(rows), 1)
    checkpoint = {
        "state_dict": model.state_dict(),
        "seed": SEED,
        "dataset_sha256": digest,
        "frames": len(rows),
        "weddings": len({row["wedding_id"] for row in rows}),
        "class_shares": {
            name: sum(1 for row in rows if row["lighting_class"] == name) / total
            for name in LIGHTING_CLASSES
        },
        "validation_within_tolerance": within_tolerance(validation),
        "validation_by_class": by_class(validation),
        "uv_tolerance": UV_TOLERANCE,
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    torch.save(checkpoint, out)
    print(
        json.dumps(
            {
                "checkpoint": str(out),
                "validation_within_tolerance": checkpoint["validation_within_tolerance"],
                "worst_class": min(
                    checkpoint["validation_by_class"].items(), key=lambda pair: pair[1]
                ),
            }
        )
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true")
    parser.add_argument("--data", metavar="JSONL")
    parser.add_argument("--out", default="white_balance.pt")
    parser.add_argument("--epochs", type=int, default=30)
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
