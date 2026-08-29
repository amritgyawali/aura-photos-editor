#!/usr/bin/env python3
"""Train the wedding-distraction detector. PHASE-24 section 6.1.

THIS SCRIPT HAS NEVER BEEN RUN, AND THE REASON IS NOT A TODO
============================================================

Section 9's DATA row asks for a labelled wedding-distraction vocabulary on ten thousand frames.
There are no wedding photographs in this repository, so there are no labels, so there is no
detector to train. `aura_generative::DISTRACTION_HEAD_TRAINED` is `false` and nothing in the
shipped build consults a model.

What ships instead is `aura_generative::detect::candidates`, which measures *unexplained
salience* - the other half of section 6.1's pair, and the half that can be built from measurement
rather than from labels. It names nothing: every candidate it produces is
`DistractionClass::Unclassified`, which cannot be shown to be story-irrelevant, so the safety
engine blocks all of them at the confidence check. **This build therefore proposes no removals on
a real photograph**, which is the correct behaviour for a build that cannot tell a bin from a
gift.

This file exists so that the day labels arrive, the training run is a command rather than a
design exercise, and so that the *shape* of the dataset the phase needs is written down while the
argument for it is fresh. ADR-0049 section 6.

WHAT THE LABELS HAVE TO CONTAIN, AND THE TWO THAT ARE EASY TO GET WRONG
======================================================================

The vocabulary is `aura_core::contract::cleanup::DistractionClass` and it is **closed**: eight
nameable classes, plus `background_person` and `unclassified`. A detector that emitted a class
outside it would be emitting a word three phases have already stored strings from.

1. **Negative examples must include the same objects when they are the subject.** A bin in the
   corner of a portrait is clutter; a bin in a photograph of the caterers packing up is the
   photograph. A dataset of bins-labelled-bin teaches a detector that a bin is always removable,
   which is exactly the failure the cloud editorial judgement exists to catch and should not have
   to.

2. **`background_person` is labelled and never trained toward removal.** It is in the vocabulary
   because the *detector* has to be able to say "that is a person" so the safety engine can refuse
   it. A dataset that omitted the class would leave a stray guest as `unclassified`, which is also
   refused - but for the weaker reason that nothing could name it, rather than the strong reason
   that it is somebody.

The intended architecture is a small anchor-free detector on the frozen phase 05 trunk, in the
shape phase 07's scene head takes: an adapter, not a second backbone. It opens no pixels of its
own beyond the 2048 px proxy every phase since 06 reads.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

# The closed vocabulary, mirroring `DistractionClass::ALL`. Copied rather than generated: a list
# that regenerated itself from the Rust enum would silently follow a rename that three phases had
# already stored strings from.
CLASSES: list[str] = [
    "exit_sign",
    "bin",
    "cable",
    "gaffer_tape",
    "bottle",
    "chair",
    "phone_screen",
    "stray_hand",
    "background_person",
    "unclassified",
]

# The classes that may ever be proposed for automated removal. Mirrors
# `DistractionClass::story_safe`. `background_person` and `unclassified` are absent and there is no
# training flag that could add them.
REMOVABLE: set[str] = {
    "exit_sign",
    "bin",
    "cable",
    "gaffer_tape",
    "bottle",
    "chair",
    "phone_screen",
    "stray_hand",
}

# Section 9's DATA row.
FRAMES_REQUIRED = 10_000

# What "enough negatives" means, and it is the number this file exists to argue for. A class whose
# examples are all distractions teaches a detector that the object is always a distraction.
MIN_NEGATIVE_SHARE = 0.30


@dataclass
class Split:
    """One train/validate/test partition, counted by class."""

    name: str
    frames: int = 0
    boxes: dict[str, int] = field(default_factory=dict)
    negatives: dict[str, int] = field(default_factory=dict)

    def negative_share(self, klass: str) -> float:
        """How many of this class's examples are of it *not* being a distraction."""
        positive = self.boxes.get(klass, 0)
        negative = self.negatives.get(klass, 0)
        total = positive + negative
        return 0.0 if total == 0 else negative / total


def load_labels(path: Path) -> list[dict]:
    """Read a JSONL label file, one frame per line."""
    rows: list[dict] = []
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def audit(rows: list[dict]) -> tuple[Split, list[str]]:
    """Count the dataset and return every reason it is not ready to train on.

    The audit runs before anything is trained, and it *refuses* rather than warns. A detector
    trained on a dataset with no negatives is a detector that will confidently propose removing
    the cake, and the cost of finding that out from a photographer is far higher than the cost of
    a failed command.
    """
    split = Split(name="all")
    problems: list[str] = []

    for row in rows:
        split.frames += 1
        for box in row.get("boxes", []):
            klass = box.get("class", "")
            if klass not in CLASSES:
                problems.append(f"unknown class {klass!r}; the vocabulary is closed")
                continue
            bucket = split.negatives if box.get("is_subject", False) else split.boxes
            bucket[klass] = bucket.get(klass, 0) + 1

    if split.frames < FRAMES_REQUIRED:
        problems.append(
            f"{split.frames} frames labelled; section 9 asks for {FRAMES_REQUIRED}"
        )

    for klass in sorted(REMOVABLE):
        if split.boxes.get(klass, 0) == 0:
            problems.append(f"no examples of {klass}")
            continue
        share = split.negative_share(klass)
        if share < MIN_NEGATIVE_SHARE:
            problems.append(
                f"{klass}: only {share:.0%} of examples are of it being the subject; "
                f"at least {MIN_NEGATIVE_SHARE:.0%} is needed or the detector learns that "
                f"a {klass} is always clutter"
            )

    if split.boxes.get("background_person", 0) == 0:
        problems.append(
            "no background_person examples. The class is labelled so the safety engine can "
            "refuse a person by name rather than by not recognising them."
        )

    return split, problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--labels", type=Path, help="JSONL, one frame per line")
    parser.add_argument("--out", type=Path, help="where the ONNX artefact would be written")
    parser.add_argument(
        "--audit-only",
        action="store_true",
        help="count the dataset and report what is missing, without training",
    )
    args = parser.parse_args()

    if args.labels is None or not args.labels.exists():
        print(
            "No labelled wedding-distraction data is present in this repository, which is\n"
            "condition C2 of the phase 24 exit report rather than a missing file.\n"
            "\n"
            "What ships instead is `aura_generative::detect::candidates`, which measures\n"
            "unexplained salience and names nothing. Every candidate it produces is\n"
            "`unclassified`, the safety engine refuses all of them at the confidence check,\n"
            "and this build therefore proposes no removals on a real photograph.\n"
            "\n"
            "See ADR-0049 section 6 and `docs/generative-policy.md`.",
            file=sys.stderr,
        )
        return 2

    rows = load_labels(args.labels)
    split, problems = audit(rows)
    print(f"{split.frames} frames, {sum(split.boxes.values())} distraction boxes")
    for klass in sorted(REMOVABLE):
        print(
            f"  {klass:<18} {split.boxes.get(klass, 0):>6} distraction "
            f"{split.negatives.get(klass, 0):>6} subject "
            f"({split.negative_share(klass):.0%} negative)"
        )

    if problems:
        print("\nthis dataset is not ready to train on:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    if args.audit_only:
        return 0

    print(
        "\nThe dataset audit passes. Training is not implemented: there has never been a\n"
        "dataset to write it against, and a training loop written blind is a training loop\n"
        "whose bugs are discovered on the first real run. Implement it here.",
        file=sys.stderr,
    )
    return 3


if __name__ == "__main__":
    raise SystemExit(main())
