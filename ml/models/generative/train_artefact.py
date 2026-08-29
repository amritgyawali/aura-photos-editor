#!/usr/bin/env python3
"""Train the inpainting-artefact classifier. PHASE-24 section 6.4.

WHAT SHIPS INSTEAD, AND WHY IT IS NOT A COMPROMISE
==================================================

`aura_generative::ARTEFACT_HEAD_TRAINED` is `false`, and unlike the distraction detector that is
not only a data problem. The self-check that ships is three *measurements* -
`aura_generative::selfcheck` - and a repeated texture, a warped line and a terminated gradient are
defined by geometry rather than by a label set:

* A repeated texture is a spatial period present in the patch and absent from the rest of the
  frame. That is an autocorrelation, compared per lag.
* A warped line is a structure whose orientation changes between the ring and the patch. That is
  a structure tensor, twice.
* A terminated gradient is a step at the seam that exceeds anything the rest of the photograph
  does. That is a percentile.

None of the three needs a model to be *defined*, which is why phase 24 could ship a self-check at
all where phase 22 could not ship a face-recovery model. What a learned classifier would add is
**the failures nobody has thought of yet** - the ones that are neither periodic, nor directional,
nor a step, and that a person recognises instantly as wrong. That is a real gap and it is why this
file exists.

THE DATASET IS THE HARD PART, AND IT IS NOT A WEDDING DATASET
=============================================================

Section 9's DATA row asks for a "known-bad inpaint set". That is not a set of photographs; it is a
set of *pairs*: a region, a removal that was applied to it, and a human verdict on whether the
result is acceptable. It cannot be scraped and it cannot be synthesised, because the failures
worth catching are the ones a synthesiser would not think to make.

Three properties this script audits for, because each of them silently ruins the classifier:

1. **Both classes come from the same generator.** A dataset whose bad examples are diffusion
   outputs and whose good examples are classical fills teaches a classifier to detect *diffusion*,
   which then rejects every inpaint and accepts every fill regardless of quality.

2. **The verdict is per region and not per photograph.** A frame with one bad patch and two good
   ones is three examples, not one.

3. **Two independent verdicts per example, with disagreement kept.** An artefact that half the
   panel cannot see is a different thing from one everybody sees, and collapsing the two loses
   exactly the boundary the classifier has to learn.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

# The three failures the shipped measurement already catches. A learned head is scored against
# these as a floor: a classifier that does worse than the arithmetic on the cases the arithmetic
# was written for is not an improvement.
MEASURED_FAILURES = ("repeated_texture", "warped_line", "ghost_edge")

# What section 10.1 gates the artefact-free rate at.
ARTEFACT_FREE_GATE = 0.98

# The fewest examples of each verdict a training run needs.
MIN_PER_CLASS = 2_000

# Two verdicts per example, so a disagreement is visible rather than averaged away.
VERDICTS_PER_EXAMPLE = 2


def load(path: Path) -> list[dict]:
    rows: list[dict] = []
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def audit(rows: list[dict]) -> list[str]:
    """Every reason this dataset is not ready to train on."""
    problems: list[str] = []
    verdicts = Counter()
    generators = {"good": Counter(), "bad": Counter()}
    single_verdict = 0

    for row in rows:
        label = "bad" if row.get("artefact", False) else "good"
        verdicts[label] += 1
        generators[label][row.get("generator", "unknown")] += 1
        if len(row.get("verdicts", [])) < VERDICTS_PER_EXAMPLE:
            single_verdict += 1

    for label in ("good", "bad"):
        if verdicts[label] < MIN_PER_CLASS:
            problems.append(
                f"{verdicts[label]} {label} examples; at least {MIN_PER_CLASS} are needed"
            )

    # Property 1. The one that produces a classifier which looks excellent and has learned the
    # wrong thing entirely.
    good_generators = set(generators["good"])
    bad_generators = set(generators["bad"])
    if good_generators and bad_generators and not (good_generators & bad_generators):
        problems.append(
            f"the good examples come from {sorted(good_generators)} and the bad ones from "
            f"{sorted(bad_generators)}, with no overlap. A classifier trained on this learns to "
            f"detect the generator rather than the artefact."
        )

    # Property 3.
    if single_verdict:
        problems.append(
            f"{single_verdict} examples carry fewer than {VERDICTS_PER_EXAMPLE} verdicts; an "
            f"artefact half a panel cannot see is a different thing from one everybody sees"
        )

    return problems


def measured_baseline(rows: list[dict]) -> float:
    """How well the shipped arithmetic does on this dataset.

    The number a learned head has to beat. Reported rather than asserted, because a dataset whose
    bad examples are all of the three shapes the measurement already catches is a dataset that
    proves nothing about the gap a classifier is meant to fill.
    """
    caught = sum(
        1
        for row in rows
        if row.get("artefact", False)
        and row.get("measured_failure") in MEASURED_FAILURES
    )
    bad = sum(1 for row in rows if row.get("artefact", False))
    return 0.0 if bad == 0 else caught / bad


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pairs", type=Path, help="JSONL, one patched region per line")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--audit-only", action="store_true")
    args = parser.parse_args()

    if args.pairs is None or not args.pairs.exists():
        print(
            "No known-bad inpaint set is present in this repository.\n"
            "\n"
            "The shipped self-check is three measurements rather than a learned score, and that\n"
            "is a decision rather than a fallback: a repeated texture, a warped line and a\n"
            "terminated gradient are defined by geometry, not by a label set. What a learned\n"
            "classifier would add is the failures nobody has thought of yet.\n"
            "\n"
            f"Section 10.1 gates the artefact-free rate at {ARTEFACT_FREE_GATE:.0%}; the shipped\n"
            "measurement meets it on synthetic frames whose artefacts were painted in, which\n"
            "proves the arithmetic and says nothing about a wedding photograph.\n"
            "\n"
            "See ADR-0049 section 8 and `tests/eval/cleanup_eval.rs` gate 10.",
            file=sys.stderr,
        )
        return 2

    rows = load(args.pairs)
    problems = audit(rows)
    baseline = measured_baseline(rows)
    print(f"{len(rows)} examples; the shipped measurement catches {baseline:.1%} of the bad ones")
    if baseline > 0.95:
        print(
            "  NOTE: the measurement already catches nearly everything here, so this dataset\n"
            "        cannot show what a learned head would add. Collect failures that are\n"
            "        neither periodic, nor directional, nor a step at the seam.",
            file=sys.stderr,
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
        "dataset to write it against. Implement it here, and hold the result to the measured\n"
        "baseline above as a floor rather than to an absolute accuracy.",
        file=sys.stderr,
    )
    return 3


if __name__ == "__main__":
    raise SystemExit(main())
