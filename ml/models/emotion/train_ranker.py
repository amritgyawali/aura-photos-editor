"""Fit the Bradley-Terry ranker from pairwise photographer comparisons.

PHASE-10 section 6.4 and section 8 step 8. Unlike the two head trainers beside it, **this
one runs** - it needs no torch, no GPU and no pixels, because the ranker is a linear
utility over nine features that are already numbers.

    python ml/models/emotion/train_ranker.py --fixtures
    python ml/models/emotion/train_ranker.py --data comparisons.json

## What it produced for this build, and what that is worth

`--fixtures` fits on the authored comparison set in `tests/eval/emotion_eval.rs`, which
is eight preferences a working photographer would recognise - laughter over nothing, tears
over a held smile, composure in a rite over a broad smile in the same rite. The
coefficients in `crates/aura-brain-wedding/config/emotion_weights.toml` are that fit,
rounded to two decimals by hand so a person can read them.

**That is condition C2 of `docs/progress/PHASE-10-EXIT.md`.** Section 9 gives `DATA`
"10k pairwise photographer comparisons across traditions" and that work has not been done.
The distinction matters for how a wrong coefficient behaves: a number argued from
photographic practice and written down is wrong in a way somebody can find and fix, and a
number fitted to the wrong dataset is wrong in a way that looks like evidence.

## Why a linear utility, and why it is fitted rather than chosen

`P(a beats b) = sigmoid(u(a) - u(b))` with `u(x) = w . f(x)` is Bradley-Terry with a
linear utility. Three properties come out of that shape and all three are load-bearing:

* the coefficients are a list a product manager can argue with;
* every reason the product writes can name one feature and carry its contribution, so
  "why is this frame above that one" has an arithmetic answer;
* phase 30 can move nine numbers towards one photographer's comparisons, which is a
  well-posed problem in a way that moving a network is not.

## The thing this file must never do

**Refit at run time.** `emotion_preferences` collects a photographer's comparisons and
nothing in the product reads them: a ranker that refitted itself while somebody was
culling would reorder the grid under their hands, and invariant 4 - same inputs, same
versions, same output - would stop being true the moment it did. Phase 30's learning loop
is where those rows start moving these numbers, deliberately and with a version bump.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from eval_emotion import (  # noqa: E402  - the sibling module, imported after the path fix
    AGREEMENT_GATE,
    FEATURES,
    fit_ranker,
    pairwise_agreement,
)

# The authored comparison set, in the same order and with the same intent as
# `pairwise_agreement_on_the_authored_comparison_set` in `tests/eval/emotion_eval.rs`.
#
# Each row is (name, winner features, loser features) over the nine features in
# `FEATURES` order:
#   subject_expression, genuineness, interaction, peak, reaction, mutual_gaze,
#   composure, tears, group
FIXTURE_COMPARISONS = [
    (
        "laughter over nothing",
        [0.62, 0.70, 0.05, 0.50, 0.0, 0.0, 0.03, 0.0, 0.25],
        [0.08, 0.01, 0.05, 0.50, 0.0, 0.0, 0.11, 0.0, 0.00],
    ),
    (
        "tears over a held smile",
        [0.55, 0.14, 0.05, 0.50, 0.0, 0.0, 0.20, 0.94, 0.25],
        [0.30, 0.10, 0.05, 0.50, 0.0, 0.0, 0.47, 0.00, 0.25],
    ),
    (
        "composure in a rite",
        [0.48, 0.03, 0.05, 0.50, 0.0, 0.0, 0.92, 0.0, 0.25],
        [0.44, 0.36, 0.05, 0.50, 0.0, 0.0, 0.06, 0.0, 0.25],
    ),
    (
        "a smile over awkwardness",
        [0.58, 0.10, 0.04, 0.50, 0.0, 0.0, 0.11, 0.0, 0.25],
        [0.12, 0.01, 0.04, 0.50, 0.0, 0.0, 0.21, 0.0, 0.00],
    ),
    (
        "tenderness at a first look",
        [0.60, 0.16, 0.06, 0.50, 0.0, 0.0, 0.21, 0.94, 0.25],
        [0.31, 0.10, 0.06, 0.50, 0.0, 0.0, 0.47, 0.00, 0.25],
    ),
    (
        "laughter on a dance floor",
        [0.64, 0.71, 0.04, 0.40, 0.0, 0.0, 0.02, 0.0, 0.25],
        [0.30, 0.04, 0.04, 0.40, 0.0, 0.0, 0.32, 0.0, 0.25],
    ),
    (
        "a portrait smile",
        [0.52, 0.02, 0.04, 0.50, 0.0, 0.0, 0.37, 0.0, 0.25],
        [0.09, 0.00, 0.04, 0.50, 0.0, 0.0, 0.21, 0.0, 0.00],
    ),
    (
        "awkward under quiet",
        [0.08, 0.01, 0.05, 0.50, 0.0, 0.0, 0.11, 0.0, 0.00],
        [0.02, 0.00, 0.05, 0.50, 0.0, 0.0, 0.11, 0.0, 0.00],
    ),
]


def render(weights: list[float], bias: float, agreement: float, count: int) -> str:
    lines = [
        "# paste into crates/aura-brain-wedding/config/emotion_weights.toml",
        "",
        "[ranker]",
        'rationale = """',
        f"Fitted by ml/models/emotion/train_ranker.py on {count} comparisons, then \\",
        "rounded to two decimals by hand so that a person can read them. The ordering of \\",
        "the coefficients is the finding, not their exact values.",
        '"""',
        f"bias = {bias:.2f}",
    ]
    lines.extend(f"{name} = {value:.2f}" for name, value in zip(FEATURES, weights))
    lines.append("")
    lines.append(f"# in-sample agreement: {agreement:.3f} (gate {AGREEMENT_GATE})")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fixtures",
        action="store_true",
        help="fit on the authored comparison set; this is what the shipped table is",
    )
    parser.add_argument(
        "--data",
        metavar="FILE",
        help='a JSON list of {"winner": [9 features], "loser": [9 features]}',
    )
    parser.add_argument("--epochs", type=int, default=400)
    args = parser.parse_args()

    if args.data:
        with open(args.data, encoding="utf-8") as handle:
            rows = json.load(handle)
        comparisons = [(row["winner"], row["loser"]) for row in rows]
        source = f"{args.data} ({len(comparisons)} comparisons)"
    elif args.fixtures:
        comparisons = [(winner, loser) for _, winner, loser in FIXTURE_COMPARISONS]
        source = f"the authored fixture set ({len(comparisons)} comparisons)"
    else:
        parser.print_help()
        return 2

    weights, bias = fit_ranker(comparisons, epochs=args.epochs)
    agreement = pairwise_agreement(comparisons, weights, bias)

    print(f"# fitted on {source}", file=sys.stderr)
    if args.fixtures:
        print(
            "# NOTE: authored comparisons, not photographers'. Condition C2 of the phase 10\n"
            "# exit report. What this measures is the ALGORITHM, not taste.",
            file=sys.stderr,
        )
        # Which comparisons the fit reproduces, named. A fit that gets seven of eight is
        # more useful reported that way than as a single number: the one it misses is the
        # argument somebody should have.
        for name, winner, loser in FIXTURE_COMPARISONS:
            left = sum(w * f for w, f in zip(weights, winner)) + bias
            right = sum(w * f for w, f in zip(weights, loser)) + bias
            mark = "ok " if left > right else "MISS"
            print(f"#   {mark} {name}: {left:.3f} against {right:.3f}", file=sys.stderr)
        print(file=sys.stderr)

    print(render(weights, bias, agreement, len(comparisons)))
    return 0 if agreement >= AGREEMENT_GATE else 1


if __name__ == "__main__":
    sys.exit(main())
