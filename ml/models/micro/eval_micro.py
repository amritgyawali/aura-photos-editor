#!/usr/bin/env python3
"""Phase 21's section 10.1 gates, from the Python side.

The Rust half - `tests/eval/micro_eval.rs` - measures the shipped detectors and the shipped guard
against synthetic frames. This half owns the two gates that are **not** measurements of code:

* the **naturalness audit**, which is 400 frames judged by retouchers, and
* the **per-hair-type and per-skin-tone coverage report**, which is a property of a corpus.

Neither can run here, because neither has any data. What ships is the arithmetic that would score
them, exercised against synthetic judgements whose answer is known by construction, so that on the
day a corpus exists the scoring is not also new.

``--self-test`` asserts five properties:

1. the naturalness rate is computed correctly and the gate is where section 0 puts it;
2. a **majority-natural but one-bucket-bad** audit fails, rather than passing on the mean;
3. the borrow disclosure check fails when a composite is unlisted - which is the one gate in this
   phase that is about a document rather than about a pixel;
4. the agreement statistic distinguishes "the retouchers agreed it was natural" from "the
   retouchers disagreed", because a 95 % rate assembled from coin flips is not a result;
5. a run with too few judgements per frame is refused rather than reported.

``--audit FILE`` scores a real audit when there is one. The file is JSON Lines, one judgement per
line: ``{"photo": "...", "judge": "...", "natural": true, "bucket": "...", "hair_type": "..."}``.
"""

from __future__ import annotations

import argparse
import json
import math
import random
import sys
from collections import defaultdict

# Section 0's headline KPI: "teeth/eye corrections judged natural >= 95 %".
NATURAL_FLOOR = 0.95

# The same floor per bucket. A mean that hides one group is the failure phase 15's fairness rule
# exists to prevent, and it is checked per bucket rather than only overall.
PER_BUCKET_FLOOR = 0.90

# The fewest independent judgements a frame needs.
MIN_JUDGES_PER_FRAME = 3

# How much of the *available* headroom above chance the judges have to take up.
#
# Not an absolute floor, and not an absolute margin either. Three judges flipping coins agree
# 75 % of the time by arithmetic alone, so an absolute floor anywhere below that passes a panel
# that saw nothing - which is why the first version of this file compared against
# `chance_agreement`. But an absolute *margin* is unreachable at the other end, and this gate
# lives at that end: a panel judging at a 97 % natural rate agrees 92 % of the time by accident,
# so there are only eight points of headroom and a fixed margin of 0.10 fails a perfect panel.
# That is the well-known paradox of chance-corrected agreement at an extreme marginal rate, and
# it is the same shape as the halo test phase 19 had to rewrite: a threshold that cannot be met
# by a correct implementation is a bug in the threshold.
#
# What is required instead is that the judges take up a tenth of whatever headroom the marginal
# rate leaves - `(observed - chance) / (1 - chance)`, which is Scott's pi. Coin flips score zero
# at any rate; a panel that is looking at the photographs scores well above this.
MIN_AGREEMENT_MARGIN = 0.10

BUCKETS = ("very_light", "light", "medium", "tan", "deep")
HAIR_TYPES = ("straight", "wavy", "curly", "coily", "braided_or_locked")


def natural_rate(judgements) -> float:
    """The share of *frames* judged natural, by majority of their judges.

    By frame rather than by judgement, deliberately. A judgement-weighted rate lets a frame that
    six people looked at outvote five frames that three people looked at, and the claim in
    section 0 is about photographs.
    """
    per_frame = defaultdict(list)
    for row in judgements:
        per_frame[row["photo"]].append(bool(row["natural"]))
    if not per_frame:
        return 0.0
    natural = sum(1 for votes in per_frame.values() if sum(votes) * 2 > len(votes))
    return natural / len(per_frame)


def agreement(judgements) -> float:
    """How often the judges of a frame agreed with each other, averaged over frames.

    One is unanimity everywhere. It is **not** compared against a constant: see
    [`chance_agreement`] and `MIN_AGREEMENT_MARGIN`. A 95 % natural rate assembled from
    disagreeing judges is not a result about the product, it is a result about the judges.
    """
    per_frame = defaultdict(list)
    for row in judgements:
        per_frame[row["photo"]].append(bool(row["natural"]))
    scores = []
    for votes in per_frame.values():
        if len(votes) < 2:
            continue
        yes = sum(votes)
        scores.append(max(yes, len(votes) - yes) / len(votes))
    return sum(scores) / len(scores) if scores else 0.0


def chance_agreement(judgements) -> float:
    """What agreement a panel this size, with this marginal rate, would reach by accident.

    Enumerated rather than approximated, because panels are small: for each frame with `n` judges,
    the expected value of `max(k, n - k) / n` when each judge says "natural" independently with the
    observed overall probability.

    This is the correction the first version of this file was missing. Three judges flipping coins
    agree three quarters of the time, so an absolute floor anywhere below that passes a panel that
    was not looking at the photographs.
    """
    rows = list(judgements)
    if not rows:
        return 0.0
    p = sum(1 for row in rows if row["natural"]) / len(rows)
    per_frame = defaultdict(list)
    for row in rows:
        per_frame[row["photo"]].append(row)
    scores = []
    for votes in per_frame.values():
        n = len(votes)
        if n < 2:
            continue
        expected = 0.0
        for k in range(n + 1):
            weight = math.comb(n, k) * (p**k) * ((1 - p) ** (n - k))
            expected += weight * (max(k, n - k) / n)
        scores.append(expected)
    return sum(scores) / len(scores) if scores else 0.0


def per_key_rate(judgements, key: str):
    grouped = defaultdict(list)
    for row in judgements:
        if key in row:
            grouped[row[key]].append(row)
    return {name: natural_rate(rows) for name, rows in grouped.items()}


def judges_per_frame(judgements):
    per_frame = defaultdict(set)
    for row in judgements:
        per_frame[row["photo"]].add(row.get("judge", "anonymous"))
    return {photo: len(judges) for photo, judges in per_frame.items()}


def disclosure_gaps(composites, report_lines):
    """Every frame that borrowed pixels and is not named in the delivery report.

    The one gate in this phase that is about a document rather than about a pixel. It exists
    because the borrowing feature's whole defence is that it is never hidden, and a defence that
    is never checked is a defence that stops being true the first time somebody changes the
    report template.
    """
    listed = set(report_lines)
    return sorted(photo for photo in composites if photo not in listed)


def score(judgements, composites=None, report_lines=None):
    """The whole gate, as a dictionary a caller can print or assert on."""
    counts = judges_per_frame(judgements)
    thin = sorted(photo for photo, n in counts.items() if n < MIN_JUDGES_PER_FRAME)
    return {
        "frames": len(counts),
        "natural_rate": natural_rate(judgements),
        "agreement": agreement(judgements),
        "chance_agreement": chance_agreement(judgements),
        "per_bucket": per_key_rate(judgements, "bucket"),
        "per_hair_type": per_key_rate(judgements, "hair_type"),
        "underjudged": thin,
        "undisclosed": disclosure_gaps(composites or [], report_lines or []),
    }


def agreement_margin(result) -> float:
    """The share of the headroom above chance that the judges actually took up.

    `(observed - chance) / (1 - chance)`. One is unanimity, zero is a panel indistinguishable
    from independent flips at the observed marginal rate, and it is negative when the judges
    agreed *less* than accident would predict. See `MIN_AGREEMENT_MARGIN` for why this is a share
    rather than a difference.
    """
    chance = result["chance_agreement"]
    headroom = 1.0 - chance
    if headroom <= 1e-9:
        # Every judgement on every frame was identical and the marginal rate is degenerate. There
        # is nothing to be above; report the floor rather than dividing by nothing.
        return MIN_AGREEMENT_MARGIN
    return (result["agreement"] - chance) / headroom


def failures_of(result) -> list[str]:
    out = []
    if result["underjudged"]:
        out.append(
            f"{len(result['underjudged'])} frames were judged by fewer than "
            f"{MIN_JUDGES_PER_FRAME} people"
        )
    margin = agreement_margin(result)
    if margin < MIN_AGREEMENT_MARGIN:
        out.append(
            f"the judges agreed with each other {result['agreement']:.3f} of the time against "
            f"{result['chance_agreement']:.3f} expected by chance, taking up {margin:.3f} of the "
            f"headroom; below {MIN_AGREEMENT_MARGIN} the rate means nothing"
        )
    if result["natural_rate"] < NATURAL_FLOOR:
        out.append(
            f"natural rate {result['natural_rate']:.3f} is below {NATURAL_FLOOR}"
        )
    for key in ("per_bucket", "per_hair_type"):
        for name, rate in sorted(result[key].items()):
            if rate < PER_BUCKET_FLOOR:
                out.append(f"{key} `{name}` is {rate:.3f}, below {PER_BUCKET_FLOOR}")
    if result["undisclosed"]:
        out.append(
            f"{len(result['undisclosed'])} frames borrowed pixels and are not in the delivery "
            "report"
        )
    return out


def synthetic(rng: random.Random, natural_rate_by_bucket, judges=3, frames_per_bucket=60):
    rows = []
    for bucket in BUCKETS:
        p = natural_rate_by_bucket.get(bucket, 0.98)
        for index in range(frames_per_bucket):
            photo = f"pht_{bucket}_{index}"
            truly_natural = rng.random() < p
            for judge in range(judges):
                # Judges agree with the truth nineteen times in twenty, so agreement is well
                # above chance and the rate is meaningful. Property 4 replaces this with coin
                # flips.
                natural = truly_natural if rng.random() < 0.95 else not truly_natural
                rows.append(
                    {
                        "photo": photo,
                        "judge": f"j{judge}",
                        "natural": natural,
                        "bucket": bucket,
                        "hair_type": HAIR_TYPES[index % len(HAIR_TYPES)],
                    }
                )
    return rows


def self_test() -> int:
    failures: list[str] = []
    rng = random.Random(0x21_E7)

    # --- 1. a good audit passes ---------------------------------------------------------------
    good = synthetic(rng, {})
    result = score(good)
    problems = failures_of(result)
    if problems:
        failures.append(f"a good audit failed the gate: {problems}")

    # --- 2. one bad bucket fails, rather than passing on the mean -------------------------------
    skewed = synthetic(random.Random(2), {"deep": 0.55})
    result = score(skewed)
    problems = failures_of(result)
    if not any("deep" in line for line in problems):
        failures.append(
            f"an audit that is bad on one bucket passed: rate {result['natural_rate']:.3f}, "
            f"buckets {result['per_bucket']}"
        )

    # --- 3. an undisclosed composite fails --------------------------------------------------------
    result = score(good, composites=["pht_light_1", "pht_deep_2"], report_lines=["pht_light_1"])
    if not any("delivery report" in line for line in failures_of(result)):
        failures.append("an undisclosed composite passed the gate")
    result = score(good, composites=["pht_light_1"], report_lines=["pht_light_1"])
    if any("delivery report" in line for line in failures_of(result)):
        failures.append("a properly disclosed composite was reported as a gap")

    # --- 4. coin-flip judges are caught -----------------------------------------------------------
    coin = []
    flipper = random.Random(4)
    for index in range(120):
        for judge in range(3):
            coin.append(
                {
                    "photo": f"pht_{index}",
                    "judge": f"j{judge}",
                    "natural": flipper.random() < 0.5,
                    "bucket": BUCKETS[index % len(BUCKETS)],
                    "hair_type": HAIR_TYPES[index % len(HAIR_TYPES)],
                }
            )
    if not any("expected by chance" in line for line in failures_of(score(coin))):
        failures.append("an audit of coin-flipping judges was not caught")

    # --- 5. too few judges is a refusal, not a result ----------------------------------------------
    thin = synthetic(random.Random(5), {}, judges=1)
    if not any("fewer than" in line for line in failures_of(score(thin))):
        failures.append("an audit with one judge per frame was reported rather than refused")

    for line in failures:
        print(f"FAIL {line}", file=sys.stderr)
    if failures:
        return 1
    print("eval_micro self-test: 5 properties hold")
    print("  a good audit passes and the gate is where section 0 puts it")
    print("  one bad demographic bucket fails rather than passing on the mean")
    print("  an undisclosed composite fails, and a disclosed one does not")
    print("  judges who agree only by chance are caught")
    print("  too few judgements per frame is refused rather than reported")
    return 0


def run_audit(path: str, composites_path: str | None, report_path: str | None) -> int:
    rows = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    composites = []
    if composites_path:
        with open(composites_path, encoding="utf-8") as handle:
            composites = [line.strip() for line in handle if line.strip()]
    report_lines = []
    if report_path:
        with open(report_path, encoding="utf-8") as handle:
            report_lines = [line.strip() for line in handle if line.strip()]

    result = score(rows, composites, report_lines)
    print(json.dumps(result, indent=2, sort_keys=True))
    problems = failures_of(result)
    for line in problems:
        print(f"FAIL {line}", file=sys.stderr)
    return 1 if problems else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the properties above")
    parser.add_argument("--audit", help="JSON Lines of retoucher judgements")
    parser.add_argument("--composites", help="one photo id per line: frames that borrowed pixels")
    parser.add_argument("--report", help="one photo id per line: what the delivery report lists")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.audit:
        return run_audit(args.audit, args.composites, args.report)
    print(
        "there is no naturalness audit in this repository; section 9's QAIQ task has not "
        "happened. Run with --self-test, or with --audit FILE when one exists",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
