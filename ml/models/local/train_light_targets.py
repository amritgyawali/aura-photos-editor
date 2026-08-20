#!/usr/bin/env python3
"""Learn Phase 19's local light targets from expert difference maps.

Section 8's first step: "extract local-adjustment behaviour from expert edits (difference maps
between baseline-graded and final images) to learn realistic targets."

**Nothing in this repository can run this on real data.** There is no corpus of RAW files
paired with expert edits here, which is condition C2 of ``docs/progress/PHASE-19-EXIT.md``.
What ships is the extraction and the fit, written so that the day a corpus exists the answer is
a run rather than a project - the same shape ``ml/models/tone/`` and ``ml/models/scene/`` take.

Run ``--self-test`` to check the arithmetic against a synthetic corpus whose answer is known by
construction. It is not a quality claim about anything.

WHAT IS BEING LEARNED, AND WHAT IS NOT
--------------------------------------

Learned: the per-scene *targets* - how much a photographer of this kind of photograph actually
lifts a face, how much separation they give a subject, how far they calm a background.

Not learned, and deliberately:

* **the caps.** ``MAX_FACE_LIFT_EV``, ``MAX_INTER_FACE_SPREAD``, ``MID_BAND_TOLERANCE`` and the
  per-image perceptual budget are product decisions with arguments attached, and a model fitted
  on a corpus of heavy-handed edits would raise every one of them. A fit cannot move a cap.
* **the priority order.** Section 6.4 gives face lighting the first claim on the budget and
  dodge and burn the last; that is an argument about what a photographer would miss, not a
  statistic.
* **anything about a person.** The extraction reads difference maps and region statistics. It
  never reads an identity, and there is nowhere in its output to put one.

THE EXTRACTION
--------------

A difference map is ``final - baseline`` in a perceptual space, restricted to a region. What is
extracted per region is the *mean* change and the *shape* of it against the region's own
tonality - because a flat lift and a shadow-weighted lift have the same mean and are different
edits, and the whole of section 6.1 is about the second one.

So each sample carries two numbers per region: ``mean_ev`` and ``shadow_bias``, where the bias
is the correlation between the change and the darkness of what it was applied to. A bias near
one is a retoucher lifting shadows; a bias near zero is somebody moving a slider.
"""

from __future__ import annotations

import argparse
import json
import math
import statistics
import sys
from collections.abc import Sequence
from pathlib import Path
from typing import Any

# Bounds the fit may not leave. Every one of them is a constant in
# `aura_core::contract::local` or a column check in migration 16, restated here because this
# script has to run with nothing installed.
MAX_FACE_LIFT_EV = 1.20
MAX_BACKGROUND_EV = 0.67
MAX_CLARITY = 22.0
MIN_SAMPLES_PER_SCENE = 25

# The scenes phase 07 names. A fit for a scene not in this list is refused rather than written
# under a name nothing will look up.
SCENES = [
    "getting_ready_bride",
    "getting_ready_groom",
    "details",
    "first_look",
    "ceremony_entrance",
    "ceremony",
    "ritual",
    "vows",
    "rings",
    "kiss",
    "family_portrait",
    "group_portrait",
    "couple_portrait",
    "golden_hour",
    "reception_entrance",
    "speeches",
    "cake",
    "first_dance",
    "dance_floor",
    "candid",
    "venue",
    "exit",
]


def extract(sample: dict[str, Any]) -> dict[str, float]:
    """Turn one expert edit into the two numbers per region the fit reads.

    ``sample`` carries, per region, the paired ``(luma_before, luma_after)`` of a set of
    points. The mean change is the obvious half; the shadow bias is the half that matters.
    """
    out: dict[str, float] = {}
    for region in ("face", "subject", "background"):
        points = sample.get(region, [])
        if len(points) < 2:
            continue
        befores = [p[0] for p in points]
        deltas = [_ev_between(p[0], p[1]) for p in points]
        out[f"{region}_mean_ev"] = statistics.fmean(deltas)
        out[f"{region}_shadow_bias"] = _bias(befores, deltas)
    return out


def _ev_between(before: float, after: float) -> float:
    """Stops between two perceptual luminances, with the encoding written down."""
    before = max(before, 1e-4)
    after = max(after, 1e-4)
    return 2.2 * math.log2(after / before)


def _bias(befores: Sequence[float], deltas: Sequence[float]) -> float:
    """How much of the change went where it was darkest, -1..1.

    Pearson correlation between darkness and change. One is a retoucher lifting shadows and
    leaving the highlights; zero is a flat exposure slider; below zero is somebody lifting the
    bright side, which is the move that makes a face glow.
    """
    darkness = [1.0 - b for b in befores]
    if len(darkness) < 2:
        return 0.0
    try:
        return max(-1.0, min(1.0, statistics.correlation(darkness, deltas)))
    except statistics.StatisticsError:
        # Constant input on either side: no correlation is defined, and reporting zero is the
        # honest answer rather than an exception a caller will catch and ignore.
        return 0.0


def fit(samples: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """One target row per scene, with the count behind it.

    A scene with fewer than ``MIN_SAMPLES_PER_SCENE`` edits behind it is **not written**. A
    target fitted on nine photographs is a target that looks like evidence, which is worse than
    no target at all - the same argument ``SkinLocus`` makes about a weak locus in phase 15.
    """
    by_scene: dict[str, list[dict[str, float]]] = {}
    refused: list[str] = []
    for sample in samples:
        scene = sample.get("scene", "unknown")
        if scene not in SCENES:
            if scene not in refused:
                refused.append(scene)
            continue
        by_scene.setdefault(scene, []).append(extract(sample))

    rows: dict[str, Any] = {}
    thin: list[str] = []
    for scene, extracted in sorted(by_scene.items()):
        if len(extracted) < MIN_SAMPLES_PER_SCENE:
            thin.append(scene)
            continue
        rows[scene] = {
            "face_lift_ev": _bounded(
                _median(extracted, "face_mean_ev"), 0.0, MAX_FACE_LIFT_EV
            ),
            "face_shadow_bias": _bounded(
                _median(extracted, "face_shadow_bias"), -1.0, 1.0
            ),
            "background_ev": _bounded(
                _median(extracted, "background_mean_ev"), -MAX_BACKGROUND_EV, 0.0
            ),
            "subject_clarity": _bounded(
                _median(extracted, "subject_mean_ev") * 40.0, 0.0, MAX_CLARITY
            ),
            "samples": len(extracted),
        }
    return {
        "scenes": rows,
        "refused_scenes": refused,
        "thin_scenes": thin,
        "note": (
            "targets only. The caps, the per-image budget and the priority order are product "
            "decisions and no fit may move them."
        ),
    }


def _median(rows: Sequence[dict[str, float]], key: str) -> float:
    values = [row[key] for row in rows if key in row]
    return statistics.median(values) if values else 0.0


def _bounded(value: float, low: float, high: float) -> float:
    return round(max(low, min(high, value)), 4)


def self_test() -> int:
    """Check the arithmetic against a corpus whose answer is known by construction."""
    failures = 0

    # A pure shadow lift: the darkest points move most.
    shadow_lift = [(0.10, 0.20), (0.30, 0.40), (0.50, 0.55), (0.70, 0.71)]
    bias = _bias([p[0] for p in shadow_lift], [_ev_between(*p) for p in shadow_lift])
    if bias < 0.8:
        print(f"FAIL: a shadow lift read as bias {bias:.2f}")
        failures += 1

    # A flat exposure move: every point moves by the same number of stops.
    flat = [(b, b * 1.15) for b in (0.10, 0.30, 0.50, 0.70)]
    bias = _bias([p[0] for p in flat], [_ev_between(*p) for p in flat])
    if abs(bias) > 0.2:
        print(f"FAIL: a flat lift read as bias {bias:.2f}")
        failures += 1

    # A thin scene is refused rather than written.
    thin = fit([{"scene": "ceremony", "face": [(0.2, 0.3), (0.4, 0.45)]}] * 5)
    if thin["scenes"]:
        print("FAIL: a scene with five samples was given a target")
        failures += 1
    if "ceremony" not in thin["thin_scenes"]:
        print("FAIL: a thin scene was not reported")
        failures += 1

    # A scene phase 07 does not name is refused.
    unknown = fit([{"scene": "confetti", "face": [(0.2, 0.3), (0.4, 0.45)]}] * 40)
    if "confetti" not in unknown["refused_scenes"]:
        print("FAIL: a scene outside the taxonomy was accepted")
        failures += 1

    # A corpus of heavy-handed edits cannot raise the cap.
    heavy = fit(
        [{"scene": "ceremony", "face": [(0.05, 0.90), (0.10, 0.95)]}] * 40
    )
    lift = heavy["scenes"].get("ceremony", {}).get("face_lift_ev", 0.0)
    if lift > MAX_FACE_LIFT_EV:
        print(f"FAIL: a heavy corpus fitted a {lift:.2f} EV lift, above the cap")
        failures += 1

    if failures == 0:
        print("self-test: the extraction and the bounds behave")
    return failures


def main(argv: Sequence[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, help="expert edit pairs, as JSON lines")
    parser.add_argument("--out", type=Path, help="where to write the fitted targets")
    parser.add_argument("--self-test", action="store_true", help="check the arithmetic")
    args = parser.parse_args(argv)

    if args.self_test:
        return 1 if self_test() else 0
    if not args.corpus:
        parser.error("one of --corpus or --self-test is required")

    samples = [
        json.loads(line)
        for line in args.corpus.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    targets = fit(samples)
    rendered = json.dumps(targets, indent=2)
    if args.out:
        args.out.write_text(rendered + "\n", encoding="utf-8")
    else:
        print(rendered)
    if targets["thin_scenes"]:
        print(
            f"scenes with too few edits to fit: {', '.join(targets['thin_scenes'])}",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
