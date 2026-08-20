#!/usr/bin/env python3
"""Evaluate Phase 17's style-match, coverage and robustness gates.

The Python side of ``tests/eval/style_eval.rs``, standard library only, so a dataset curator
can measure an archive before installing anything. Real input is one JSON object with
``held_out``, ``buckets`` and ``archives`` arrays; run ``--schema`` for the shape.

``--self-test`` proves every metric rejects a degenerate implementation. It is not a quality
claim about anything: there are no photographers' archives in this repository.

Three of the metrics here are worth reading carefully
-----------------------------------------------------

**The match metric is measured against two ceilings, not one.** Section 10.1 sets 2.5 dE00 for
a populated bucket and 3.5 for a weak one, and the difference is deliberate: a bucket
answering out of its parent group is *allowed* to be worse and is not allowed to be arbitrary.
A single ceiling would either fail every sparse archive or pass every bad one.

**A bucket with no held-out pairs is unmeasured, and unmeasured is not zero.** It is reported
as ``null`` and excluded from the mean. A metric that scored it as a perfect match would look
best exactly where it knows least, which is the one thing a report about accuracy must never
do.

**Improvement is measured against the baseline, not against zero.** Section 13's criterion is
that edits made with a profile are *measurably closer* to the photographer's own than the
factory baseline. A style that scores 2.4 dE00 where doing nothing scores 2.2 has made things
worse, and a metric that only reported the first number would call it a pass.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

# Section 10.1, and each of these is also a constant in `aura_core::contract::style`.
MATCH_DE00_CEILING = 2.5
WEAK_MATCH_DE00_CEILING = 3.5
WEAK_BUCKET_SAMPLES = 10
USABLE_PAIRS = 300
MAX_ARCHIVE_INFLUENCE = 0.35

SCHEMA = {
    "held_out": [
        {"bucket": "portraits/daylight", "de00": 1.4, "baseline_de00": 3.8}
    ],
    "buckets": [{"bucket": "portraits/daylight", "samples": 44, "level": "bucket"}],
    "archives": [{"label": "2025-06-smith", "pairs": 380, "global_lean": 0.31}],
    "accepted_pairs": 812,
    "rejected_pairs": 96,
}


def ceiling_for(samples: int) -> float:
    """Which ceiling one bucket is held to."""
    return WEAK_MATCH_DE00_CEILING if samples <= WEAK_BUCKET_SAMPLES else MATCH_DE00_CEILING


def match_report(payload: dict[str, Any]) -> dict[str, Any]:
    """Per-bucket and overall style match, with the unmeasured buckets left unmeasured."""
    held = list(payload.get("held_out", []))
    samples = {
        str(row.get("bucket")): int(row.get("samples", 0))
        for row in payload.get("buckets", [])
    }

    by_bucket: dict[str, list[dict[str, float]]] = {}
    for row in held:
        by_bucket.setdefault(str(row.get("bucket")), []).append(row)

    rows = []
    regressed = []
    for bucket in sorted(set(samples) | set(by_bucket)):
        mine = by_bucket.get(bucket, [])
        if not mine:
            rows.append(
                {"bucket": bucket, "samples": samples.get(bucket, 0), "match_de00": None}
            )
            continue
        measured = sum(float(row.get("de00", 0.0)) for row in mine) / len(mine)
        baseline = sum(float(row.get("baseline_de00", 0.0)) for row in mine) / len(mine)
        if measured >= baseline:
            regressed.append(bucket)
        rows.append(
            {
                "bucket": bucket,
                "samples": samples.get(bucket, 0),
                "match_de00": measured,
                "baseline_de00": baseline,
                "ceiling": ceiling_for(samples.get(bucket, 0)),
                "met": measured <= ceiling_for(samples.get(bucket, 0)),
            }
        )

    overall = (
        sum(float(row.get("de00", 0.0)) for row in held) / len(held) if held else None
    )
    baseline = (
        sum(float(row.get("baseline_de00", 0.0)) for row in held) / len(held)
        if held
        else None
    )
    return {
        "per_bucket": rows,
        "overall_de00": overall,
        "baseline_de00": baseline,
        "measured_buckets": sum(1 for row in rows if row.get("match_de00") is not None),
        "regressed_buckets": regressed,
        "met_ceiling": overall is not None and overall <= MATCH_DE00_CEILING,
        "improved": overall is not None and baseline is not None and overall < baseline,
    }


def acceptance(payload: dict[str, Any]) -> dict[str, Any]:
    """How honest the match figure is: what fraction of the archive it was measured from."""
    accepted = int(payload.get("accepted_pairs", 0))
    rejected = int(payload.get("rejected_pairs", 0))
    total = accepted + rejected
    return {
        "accepted": accepted,
        "rejected": rejected,
        "acceptance": (accepted / total) if total else 0.0,
        "usable": accepted >= USABLE_PAIRS,
    }


def influence(payload: dict[str, Any]) -> dict[str, Any]:
    """The largest share any one wedding holds, against the cap."""
    archives = list(payload.get("archives", []))
    total = sum(int(row.get("pairs", 0)) for row in archives)
    if total == 0:
        return {"largest_share": 0.0, "within_cap": True, "archives": 0}
    cap = max(MAX_ARCHIVE_INFLUENCE, 1.0 / max(1, len(archives)))
    largest = max(int(row.get("pairs", 0)) for row in archives) / total
    return {
        "largest_share": largest,
        "cap": cap,
        # The share *before* weighting may exceed the cap; what the fit enforces is the share
        # after it. This reports the raw figure so a curator can see an archive that will be
        # capped, which is information rather than a failure.
        "will_be_capped": largest > cap + 1e-6,
        "archives": len(archives),
    }


def report(payload: dict[str, Any]) -> dict[str, Any]:
    return {
        "match": match_report(payload),
        "pairs": acceptance(payload),
        "influence": influence(payload),
    }


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------


def self_test() -> int:
    failures = 0

    def check(name: str, ok: bool, detail: str = "") -> None:
        nonlocal failures
        if ok:
            print(f"  ok   {name}")
        else:
            failures += 1
            print(f"  FAIL {name} {detail}")

    good = {
        "held_out": [
            {"bucket": "portraits/daylight", "de00": 1.2, "baseline_de00": 3.9},
            {"bucket": "portraits/daylight", "de00": 1.6, "baseline_de00": 3.5},
        ],
        "buckets": [
            {"bucket": "portraits/daylight", "samples": 44},
            {"bucket": "dance/flash", "samples": 6},
        ],
        "archives": [{"label": "a", "pairs": 400}, {"label": "b", "pairs": 412}],
        "accepted_pairs": 812,
        "rejected_pairs": 96,
    }
    out = report(good)
    check("a good profile meets the ceiling", out["match"]["met_ceiling"])
    check("and beats the baseline", out["match"]["improved"])
    check(
        "a bucket with no held-out pairs is null, not zero",
        any(
            row["bucket"] == "dance/flash" and row["match_de00"] is None
            for row in out["match"]["per_bucket"]
        ),
    )
    check("only measured buckets are counted", out["match"]["measured_buckets"] == 1)
    check("the acceptance is reported", abs(out["pairs"]["acceptance"] - 0.894) < 0.01)

    # The falsification checks: each metric must be able to say no.
    bad = dict(good, held_out=[{"bucket": "portraits/daylight", "de00": 4.0, "baseline_de00": 3.9}])
    out = report(bad)
    check("a bad profile fails the ceiling", not out["match"]["met_ceiling"])
    check(
        "and is named as a regression rather than merely a miss",
        out["match"]["regressed_buckets"] == ["portraits/daylight"],
    )

    weak = {
        "held_out": [{"bucket": "dance/flash", "de00": 3.2, "baseline_de00": 5.0}],
        "buckets": [{"bucket": "dance/flash", "samples": 6}],
        "accepted_pairs": 90,
        "rejected_pairs": 5,
    }
    out = report(weak)
    row = out["match"]["per_bucket"][0]
    check("a weak bucket is held to the weaker ceiling", row["met"] and row["ceiling"] == 3.5)
    check("an under-trained profile is not called usable", not out["pairs"]["usable"])

    dominated = {
        "archives": [{"label": "a", "pairs": 40}, {"label": "outlier", "pairs": 900}],
    }
    check("a dominating archive is flagged", influence(dominated)["will_be_capped"])
    balanced = {"archives": [{"label": "a", "pairs": 400}, {"label": "b", "pairs": 400}]}
    check("a balanced pair of archives is not", not influence(balanced)["will_be_capped"])

    print()
    if failures == 0:
        print("self-test: OK")
        print(
            "Every metric above rejects a degenerate answer. None of it is a claim about a "
            "photograph: there are no archives in this repository."
        )
    else:
        print(f"self-test: {failures} check(s) failed")
    return 0 if failures == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, help="a JSON evaluation file")
    parser.add_argument("--schema", action="store_true", help="print the input shape and exit")
    parser.add_argument("--self-test", action="store_true", help="check every metric can fail")
    args = parser.parse_args()

    if args.schema:
        print(json.dumps(SCHEMA, indent=2))
        return 0
    if args.self_test:
        return self_test()
    if not args.input:
        parser.error("--input, --schema or --self-test")

    payload = json.loads(args.input.read_text(encoding="utf-8"))
    print(json.dumps(report(payload), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
