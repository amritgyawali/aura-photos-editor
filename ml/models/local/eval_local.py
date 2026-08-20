#!/usr/bin/env python3
"""Evaluate Phase 19's halo, subtlety, texture and fairness gates.

This is the Python side of ``tests/eval/local_eval.rs``. It deliberately uses only the
standard library so a dataset curator can run it before installing a training stack. Real
evaluation input is one JSON object with ``frames``, ``groups`` and ``bands`` arrays; run
``--schema`` for the exact shape.

``--self-test`` proves every metric rejects a degenerate implementation. It is not a quality
claim about the placeholder targets shipped by this repository.

Two metrics are worth reading carefully before using this script.

**The halo metric is not an edge-gradient ratio.** Section 10.1 asks for "an automated
edge-gradient test" and the obvious reading of that is wrong: *every* local brightening
increases the step at its own boundary, because that is what "local" means, so a before/after
gradient ratio scores the edit's size and calls it an artefact. Two refinements of it are also
wrong and ``HALO_NOTES`` below records why. What a halo actually is, is an edit that is
*stronger further from the subject than nearer to it*, so what is measured is the edit
profile: it must be monotonic in the matte and must never exceed its value at full coverage.

**The subtlety metric is not a quality score.** It reports how much of the per-image
perceptual allowance an edit spent, which is a measurement of *how much was changed* rather
than of whether the change was right. Section 10.1's own subtlety gate is a human study over
four hundred frames; this cannot stand in for it and does not try to.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections.abc import Iterable, Sequence
from pathlib import Path
from typing import Any

# Section 10.1, and every one of these is also a constant in
# `aura_core::contract::local`. They are restated rather than imported because this script has
# to run with nothing installed; `tests/eval/local_eval.rs` asserts the pair agree.
PERCEPTUAL_BUDGET = 0.045
MAX_INTER_FACE_SPREAD = 0.08
MAX_MEAN_LUMA_DRIFT = 0.03
MID_BAND_TOLERANCE = 0.05
HALO_CLEAN_FRACTION = 0.99
SUBTLETY_GATE = 4.2

HALO_NOTES = """\
Three readings of section 10.1's edge-gradient test were implemented in the Rust harness and
discarded before the one this script uses:

  1. before/after gradient ratio - measures the edit's size, because every local brightening
     increases the step at its own boundary;
  2. peak-over-mean gradient - a hard edge puts its whole transition into one sample, so its
     peak and its mean are the same number and it scores perfectly;
  3. transition width of the difference image - breaks when the matte's edge coincides with a
     content edge, which is what a good subject matte does.

What is measured instead: the edit profile across the boundary must be monotonic in the matte
and must never exceed its value at full coverage. An edit that is stronger further out is a
rim, and a rim is what a halo is."""


def _mean(values: Sequence[float]) -> float:
    return sum(values) / len(values) if values else 0.0


# ---------------------------------------------------------------------------
# The halo metric
# ---------------------------------------------------------------------------


def halo_score(profile: Sequence[float]) -> dict[str, Any]:
    """Judge one boundary's edit profile.

    ``profile`` is the luminance change the edit made, sampled from the centre of the mask
    outward across its falloff. A clean edit starts at its maximum and decreases to zero.
    """
    if len(profile) < 2:
        return {"clean": True, "monotonic": True, "overshoot": 0.0}
    full = abs(profile[0])
    monotonic = all(
        abs(profile[i]) <= abs(profile[i - 1]) + 1e-6 for i in range(1, len(profile))
    )
    overshoot = max((abs(v) - full for v in profile), default=0.0)
    return {
        "clean": monotonic and overshoot <= 1e-6,
        "monotonic": monotonic,
        "overshoot": max(overshoot, 0.0),
    }


def halo_report(frames: Iterable[dict[str, Any]]) -> dict[str, Any]:
    results = [halo_score(frame.get("profile", [])) for frame in frames]
    if not results:
        return {"frames": 0, "clean_fraction": 1.0, "pass": True}
    clean = sum(1 for r in results if r["clean"])
    fraction = clean / len(results)
    return {
        "frames": len(results),
        "clean_fraction": fraction,
        "worst_overshoot": max((r["overshoot"] for r in results), default=0.0),
        "pass": fraction >= HALO_CLEAN_FRACTION,
    }


# ---------------------------------------------------------------------------
# The pairing metric
# ---------------------------------------------------------------------------


def pairing_report(frames: Iterable[dict[str, Any]]) -> dict[str, Any]:
    """Section 10.1: the paired operations keep the frame's mean luminance within 3 %."""
    drifts = [
        abs(frame.get("mean_after", 0.0) - frame.get("mean_before", 0.0))
        for frame in frames
        if frame.get("paired", False)
    ]
    if not drifts:
        return {"frames": 0, "worst_drift": 0.0, "pass": True}
    worst = max(drifts)
    return {
        "frames": len(drifts),
        "mean_drift": _mean(drifts),
        "worst_drift": worst,
        "pass": worst <= MAX_MEAN_LUMA_DRIFT + 1e-6,
    }


# ---------------------------------------------------------------------------
# The group fairness metric
# ---------------------------------------------------------------------------


def group_report(groups: Iterable[dict[str, Any]]) -> dict[str, Any]:
    """Section 10.1: inter-face luminance spread after lighting.

    Reported as **two** numbers, for the reason the Rust contract's ``group_is_fair`` gives:
    read as an absolute, this is a promise no arithmetic can keep on a frame where one person
    is two stops down under a doorway, and the two ways of keeping it anyway - refuse to plan
    the frame, or darken everybody else - are both worse than the problem.

    * ``inside`` - the fraction of groups that ended within the threshold;
    * ``improved`` - the fraction that ended no wider than they started.

    The gate is on the second. The first is reported because it is what a photographer sees.
    """
    rows = list(groups)
    if not rows:
        return {"groups": 0, "inside": 1.0, "improved": 1.0, "pass": True}
    inside = 0
    improved = 0
    for group in rows:
        after = group.get("spread_after", 0.0)
        before = group.get("spread_before", after)
        if after <= MAX_INTER_FACE_SPREAD + 1e-6:
            inside += 1
        if after <= before + 1e-6:
            improved += 1
    return {
        "groups": len(rows),
        "inside": inside / len(rows),
        "improved": improved / len(rows),
        "pass": improved == len(rows),
    }


# ---------------------------------------------------------------------------
# The texture metric
# ---------------------------------------------------------------------------


def texture_report(bands: Iterable[dict[str, Any]]) -> dict[str, Any]:
    """Section 10.1: dodge and burn preserves mid-frequency texture."""
    drifts = []
    for band in bands:
        before = band.get("before", 0.0)
        after = band.get("after", before)
        if before <= 1e-9:
            continue
        drifts.append(abs(after - before) / before)
    if not drifts:
        return {"faces": 0, "worst_drift": 0.0, "pass": True}
    worst = max(drifts)
    return {
        "faces": len(drifts),
        "mean_drift": _mean(drifts),
        "worst_drift": worst,
        "pass": worst <= MID_BAND_TOLERANCE + 1e-6,
    }


# ---------------------------------------------------------------------------
# The subtlety measurement, which is not a gate
# ---------------------------------------------------------------------------


def subtlety_report(frames: Iterable[dict[str, Any]]) -> dict[str, Any]:
    """How much of the allowance the edits spent.

    **Not section 10.1's subtlety gate.** That gate is an expert rating of four hundred frames
    and no arithmetic substitutes for it. What this reports is how much was changed, which is
    the thing a rating would be *of* - so a build whose mean spend has doubled is a build worth
    re-rating, and that is the whole of what this number is for.
    """
    spends = [frame.get("budget_used", 0.0) for frame in frames]
    ratings = [frame.get("expert_rating") for frame in frames]
    rated = [r for r in ratings if isinstance(r, (int, float))]
    report: dict[str, Any] = {
        "frames": len(spends),
        "mean_budget_used": _mean(spends),
        "max_budget_used": max(spends, default=0.0),
        "perceptual_budget": PERCEPTUAL_BUDGET,
        "expert_rated": len(rated),
    }
    if rated:
        report["mean_rating"] = _mean(rated)
        report["pass"] = _mean(rated) >= SUBTLETY_GATE
    else:
        report["pass"] = None
        report["note"] = (
            "no expert ratings in the input; section 10.1's subtlety gate cannot be "
            "evaluated and this is a measurement of how much changed, not of whether it "
            "was right"
        )
    return report


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------

SCHEMA = {
    "frames": [
        {
            "id": "pht_...",
            "profile": [0.08, 0.06, 0.03, 0.0],
            "paired": True,
            "mean_before": 0.50,
            "mean_after": 0.49,
            "budget_used": 0.42,
            "expert_rating": 4.5,
        }
    ],
    "groups": [{"id": "pht_...", "spread_before": 0.34, "spread_after": 0.21}],
    "bands": [{"id": "pht_...", "before": 0.0120, "after": 0.0117}],
}


def evaluate(document: dict[str, Any]) -> dict[str, Any]:
    frames = document.get("frames", [])
    return {
        "halo": halo_report(frames),
        "pairing": pairing_report(frames),
        "group": group_report(document.get("groups", [])),
        "texture": texture_report(document.get("bands", [])),
        "subtlety": subtlety_report(frames),
    }


def self_test() -> int:
    """Prove every metric rejects a degenerate implementation."""
    failures = 0

    # A clean falloff passes; one that is stronger further out does not.
    if not halo_score([0.08, 0.06, 0.03, 0.0])["clean"]:
        print("FAIL: a monotonic falloff was called a halo")
        failures += 1
    if halo_score([0.05, 0.09, 0.03, 0.0])["clean"]:
        print("FAIL: an edit stronger at the boundary than inside was called clean")
        failures += 1

    # The pairing gate bites.
    over = pairing_report([{"paired": True, "mean_before": 0.50, "mean_after": 0.44}])
    if over["pass"]:
        print("FAIL: a six-per-cent luminance drift passed the pairing gate")
        failures += 1

    # The group gate is about the edit, not about the frame.
    widened = group_report([{"spread_before": 0.10, "spread_after": 0.20}])
    if widened["pass"]:
        print("FAIL: a group the pass made less even passed")
        failures += 1
    narrowed = group_report([{"spread_before": 0.34, "spread_after": 0.21}])
    if not narrowed["pass"]:
        print("FAIL: a group that was improved but not to the threshold failed")
        failures += 1
    if narrowed["inside"] != 0.0:
        print("FAIL: a 0.21 spread was reported as inside the 0.08 threshold")
        failures += 1

    # The texture gate bites.
    smoothed = texture_report([{"before": 0.012, "after": 0.006}])
    if smoothed["pass"]:
        print("FAIL: halving the mid-band energy passed the texture gate")
        failures += 1

    # The subtlety measurement refuses to claim a rating it does not have.
    unrated = subtlety_report([{"budget_used": 0.4}])
    if unrated["pass"] is not None:
        print("FAIL: subtlety claimed a verdict with no expert ratings")
        failures += 1

    if failures == 0:
        print("self-test: every metric rejects a degenerate implementation")
    return failures


def main(argv: Sequence[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, help="evaluation document")
    parser.add_argument("--schema", action="store_true", help="print the input shape")
    parser.add_argument("--self-test", action="store_true", help="check the metrics")
    parser.add_argument("--halo-notes", action="store_true", help="why not a gradient ratio")
    args = parser.parse_args(argv)

    if args.schema:
        print(json.dumps(SCHEMA, indent=2))
        return 0
    if args.halo_notes:
        print(HALO_NOTES)
        return 0
    if args.self_test:
        return 1 if self_test() else 0
    if not args.input:
        parser.error("one of --input, --schema, --halo-notes or --self-test is required")

    document = json.loads(args.input.read_text(encoding="utf-8"))
    report = evaluate(document)
    print(json.dumps(report, indent=2))
    failed = [name for name, section in report.items() if section.get("pass") is False]
    if failed:
        print(f"gates failed: {', '.join(failed)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
