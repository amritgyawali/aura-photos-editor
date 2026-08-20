#!/usr/bin/env python3
"""Phase 18's evaluation metrics, and the proof that every one of them can fail.

Section 10.1 asks for four things this file computes:

* per-class mIoU, against the gates in ``aura_vision::contract::mask``;
* the same figures on a **dark-skin subset** and an **ethnic-attire subset**;
* a matting quality measure that catches a halo rather than averaging it away;
* a storage figure per image, against the 180 KB budget in section 11.

``--self-test`` runs without any data and asserts that each metric **rejects** a predictor that
is wrong in the way the metric exists to catch. A metric that can only pass is a metric that
proves nothing, and every phase since 09 has shipped this check for the same reason.

WHAT THIS FILE CANNOT DO HERE

There are no labelled wedding frames in this repository, so the subset reports have no data to
run on. They are implemented and self-tested; they are not *measured*. That is condition C1 of
``docs/progress/PHASE-18-EXIT.md`` and it is a Sev 2 trigger, and the two empty rows in
``docs/model-cards/semantic_segment.md`` are the same fact stated where a reader will meet it.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

# The gates, mirroring `aura_vision::contract::mask`. Duplicated rather than parsed out of the
# Rust, and the phase gate checks the two agree - a parser over a source file is a second thing
# to keep working.
FACE_SKIN_MIOU = 0.92
HAIR_MIOU = 0.88
SUBJECT_MIOU = 0.90
PAYLOAD_BUDGET_BYTES = 180 * 1024

# How far a subset may fall below the overall figure before it is a finding.
#
# Three points. Section 10.1 does not name a number, so this is the product's: below three
# points of mIoU a boundary difference is not visible at 100 % zoom, and above it the mask on
# one kind of wedding is measurably worse than on another - which is the disparity the subset
# rows exist to find.
SUBSET_TOLERANCE = 0.03

# The band width, in pixels, the halo check measures inside.
HALO_BAND = 6


def iou(truth: list[float], predicted: list[float]) -> float:
    """Soft intersection over union.

    Soft rather than thresholded, because this is the *matting* metric's companion and a matte
    that fades out over a boundary is scored as the partial coverage it is. The class-assignment
    gates in ``tests/eval/mask_eval.rs`` threshold both sides first, which is what mIoU means
    everywhere it is reported; both are correct for what they measure.
    """
    inter = sum(min(t, p) for t, p in zip(truth, predicted))
    union = sum(max(t, p) for t, p in zip(truth, predicted))
    if union <= 1e-9:
        # Two empty regions agree completely. Returning zero would make "there is no sky in this
        # photograph and the mask says so" score as a total failure, which is how a mIoU gate
        # ends up measuring how many skies the fixtures happen to contain.
        return 1.0
    return inter / union


def halo_score(truth: list[float], predicted: list[float], band: int = HALO_BAND) -> float:
    """How much alpha the prediction puts outside the truth, near the boundary.

    A halo is not a low mIoU - it is a *small* amount of alpha in a *specific* place, and an
    overall IoU averages it into invisibility. This measures the excess inside a band around the
    true boundary and normalises by the boundary length, so a one-pixel rim around a large
    subject and around a small one score the same.

    Lower is better. Zero is no excess anywhere near the edge.
    """
    n = len(truth)
    if n == 0:
        return 0.0
    boundary = [
        i
        for i in range(n - 1)
        if (truth[i] > 0.5) != (truth[i + 1] > 0.5)
    ]
    if not boundary:
        return 0.0
    excess = 0.0
    for edge in boundary:
        for offset in range(1, band + 1):
            for index in (edge - offset, edge + 1 + offset):
                if 0 <= index < n:
                    excess += max(0.0, predicted[index] - truth[index])
    return excess / (len(boundary) * band * 2)


def subset_report(
    overall: dict[str, float],
    subsets: dict[str, dict[str, float]],
    tolerance: float = SUBSET_TOLERANCE,
) -> list[str]:
    """Every class where a subset falls more than ``tolerance`` below the overall figure.

    The report is a list of findings rather than a pass or a fail, because "the hair mask is four
    points worse on the dark-skin subset" is an instruction to go and label more of something,
    and a boolean is not.
    """
    findings: list[str] = []
    for subset, per_class in sorted(subsets.items()):
        for name, value in sorted(per_class.items()):
            reference = overall.get(name)
            if reference is None:
                continue
            if reference - value > tolerance:
                findings.append(
                    f"{subset}: {name} {value:.3f} against {reference:.3f} overall "
                    f"({reference - value:.3f} below)"
                )
    return findings


def gates(per_class: dict[str, float]) -> list[str]:
    """Section 10.1's three headline gates, as failures."""
    problems: list[str] = []
    for name in ("skin", "face"):
        value = per_class.get(name)
        if value is not None and value < FACE_SKIN_MIOU:
            problems.append(f"{name} mIoU {value:.3f} below {FACE_SKIN_MIOU}")
    hair = per_class.get("hair")
    if hair is not None and hair < HAIR_MIOU:
        problems.append(f"hair mIoU {hair:.3f} below {HAIR_MIOU}")
    subject = per_class.get("subject")
    if subject is not None and subject < SUBJECT_MIOU:
        problems.append(f"subject mIoU {subject:.3f} below {SUBJECT_MIOU}")
    return problems


def storage_report(per_image_bytes: list[int]) -> dict[str, Any]:
    """The budget figure, and what it is made of."""
    if not per_image_bytes:
        return {"images": 0, "mean": 0, "worst": 0, "over_budget": 0, "within": True}
    over = [b for b in per_image_bytes if b > PAYLOAD_BUDGET_BYTES]
    return {
        "images": len(per_image_bytes),
        "mean": sum(per_image_bytes) // len(per_image_bytes),
        "worst": max(per_image_bytes),
        "over_budget": len(over),
        "budget": PAYLOAD_BUDGET_BYTES,
        # A budget that is met on average and blown on one frame in fifty is a budget that is
        # not met. The worst frame is what decides.
        "within": max(per_image_bytes) <= PAYLOAD_BUDGET_BYTES,
    }


def self_test() -> int:
    problems: list[str] = []

    # A step region: sixteen on, sixteen off.
    truth = [1.0] * 16 + [0.0] * 16

    # 1. mIoU rejects a prediction that is offset from the truth.
    perfect = iou(truth, truth)
    offset = iou(truth, [0.0] * 4 + [1.0] * 16 + [0.0] * 12)
    if perfect < 0.999:
        problems.append("mIoU did not score an exact match as one")
    if offset >= 0.8:
        problems.append(f"mIoU scored a four-pixel offset at {offset:.3f}; it is too forgiving")

    # 2. mIoU scores two empty regions as agreement.
    if iou([0.0] * 32, [0.0] * 32) < 0.999:
        problems.append("two empty regions did not agree")

    # 3. The halo measure catches what mIoU averages away.
    #    A one-pixel rim of 0.3 alpha around the boundary is invisible to IoU and is the
    #    artefact section 10.1 audits for at 100 % zoom.
    haloed = truth[:]
    for index in range(16, 22):
        haloed[index] = 0.3
    halo_iou = iou(truth, haloed)
    if halo_iou < 0.85:
        problems.append(
            f"the haloed prediction scored {halo_iou:.3f} on mIoU; the point of the halo "
            "measure is that mIoU does *not* catch it, so this test proves nothing"
        )
    if halo_score(truth, haloed) <= 0.0:
        problems.append("the halo measure scored a visible halo as zero")
    if halo_score(truth, truth) != 0.0:
        problems.append("the halo measure scored an exact match as non-zero")

    # 4. The gates reject a model below them.
    if not gates({"skin": 0.80, "face": 0.95, "hair": 0.95, "subject": 0.95}):
        problems.append("the gates passed a skin mIoU of 0.80")
    if gates({"skin": 0.95, "face": 0.95, "hair": 0.90, "subject": 0.92}):
        problems.append("the gates failed a model that clears all three")

    # 5. The subset report finds a disparity and is quiet when there is none.
    overall = {"skin": 0.94, "hair": 0.90}
    disparity = subset_report(overall, {"dark_skin": {"skin": 0.88, "hair": 0.89}})
    if len(disparity) != 1 or "skin" not in disparity[0]:
        problems.append(f"the subset report did not find the disparity: {disparity}")
    quiet = subset_report(overall, {"ethnic_attire": {"skin": 0.93, "hair": 0.89}})
    if quiet:
        problems.append(f"the subset report invented a disparity: {quiet}")

    # 6. The storage report fails on the worst frame rather than on the mean.
    mostly_fine = [1000] * 49 + [PAYLOAD_BUDGET_BYTES * 2]
    if storage_report(mostly_fine)["within"]:
        problems.append("the storage report passed a gallery with one frame over budget")
    if not storage_report([PAYLOAD_BUDGET_BYTES] * 10)["within"]:
        problems.append("the storage report failed a gallery exactly at budget")

    for problem in problems:
        print(f"eval_mask: {problem}", file=sys.stderr)
    if problems:
        return 1

    print("eval_mask: mIoU rejects an offset region and accepts two empty ones")
    print(
        f"eval_mask: a halo mIoU misses at {halo_iou:.3f} is caught at "
        f"{halo_score(truth, haloed):.3f}"
    )
    print("eval_mask: the three headline gates and the subset report both reject and both pass")
    print("eval_mask: the storage report is decided by the worst frame, not the mean")
    print(
        "eval_mask: NO LABELLED WEDDING FRAMES IN THIS REPOSITORY. The subset reports are "
        "implemented and self-tested; they are not measured."
    )
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="prove every metric can fail")
    parser.add_argument("--gates", action="store_true", help="print the gate thresholds")
    args = parser.parse_args(argv)

    if args.gates:
        print(
            json.dumps(
                {
                    "face_skin_miou": FACE_SKIN_MIOU,
                    "hair_miou": HAIR_MIOU,
                    "subject_miou": SUBJECT_MIOU,
                    "payload_budget_bytes": PAYLOAD_BUDGET_BYTES,
                    "subset_tolerance": SUBSET_TOLERANCE,
                },
                indent=2,
            )
        )
        return 0
    if args.self_test:
        return self_test()

    print(
        "eval_mask: no labelled frames are available in this repository. Run with --self-test "
        "to prove the metrics can fail, or --gates to print the thresholds.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
