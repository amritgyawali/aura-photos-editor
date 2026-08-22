#!/usr/bin/env python3
"""Phase 22's evaluation gates, from the Python side, and the self-test that proves they can fail.

Section 10.1 asks for seven things. Four of them are measured in Rust, against synthetic frames
read back through the real operators and the real renderer - `tests/eval/restore_eval.rs`. Three
are *study* protocols rather than arithmetic, and this file is where those live:

* the **expert preference study** at ISO 3200/6400/12800/25600, against no-denoise;
* the **competitive study** against DxO DeepPRIME, Topaz Photo AI and Lightroom AI Denoise;
* the **identity-preservation gate**, which is 100 % rather than a margin.

``--self-test`` runs without a panel and asserts four properties of the *estimators*:

1. the preference estimator catches a build that is worse than no denoising;
2. the preference estimator does not pass a build on a sample too small to distinguish it from
   chance - a 4-of-5 preference is not an 80 % preference;
3. the inter-rater agreement is **chance-corrected**, and a panel at an extreme marginal rate can
   still reach the margin;
4. the identity gate is a **maximum, not a mean** - one face over the ceiling fails a wedding, and
   an estimator that averaged would pass a build that had changed one person's face completely.

WHY PROPERTY 3 IS HERE AT ALL

Phase 21 shipped an agreement gate that a perfect panel could not pass: it required an absolute
0.10 above chance, and at a 97 % marginal rate chance agreement is already 0.92, leaving eight
points of headroom for a ten-point margin. The fix was to make the margin a share of the available
headroom, which is Scott's pi. Phase 22 has the same shape of measurement and inherits the fix
rather than the defect. ADR-0043 section 11 and phase 21's exit report record it.

WHAT THIS FILE CANNOT DO

There is no panel, no reference wedding and no competitor output in this repository, so every
study below runs on authored responses whose answer is known by construction. That proves the
estimators; it says nothing about a photograph. Conditions C1 and C4 of
`docs/progress/PHASE-22-EXIT.md`.
"""

from __future__ import annotations

import argparse
import math
import sys

# Section 0's headline KPI: expert preference at or above 80 % versus no denoise at ISO >= 6400.
PREFERENCE_FLOOR = 0.80

# The ISO steps section 8 step 10 names.
ISO_STEPS = (3200, 6400, 12800, 25600)

# Section 10.1: identity distance below the threshold on 100 % of fixtures.
IDENTITY_CEILING = 0.08
IDENTITY_PASS_RATE = 1.0

# Scott's pi floor. A share of the headroom above chance rather than an absolute margin - see the
# module header for why phase 21's absolute version could not be met.
AGREEMENT_FLOOR = 0.55

# The smallest panel that can distinguish 80 % from chance at p < 0.05. Below this the study
# reports "inconclusive" rather than a number, because a 4-of-5 preference is not an 80 %
# preference.
MIN_PANEL = 20


def wilson_lower_bound(successes: int, trials: int, z: float = 1.96) -> float:
    """The lower end of a Wilson interval on a proportion.

    The gate is read against this rather than against the point estimate, which is what makes a
    small panel fail rather than pass loudly. A point estimate of 0.80 on five judgements has a
    lower bound near 0.38.
    """
    if trials <= 0:
        return 0.0
    p = successes / trials
    denominator = 1.0 + z * z / trials
    centre = p + z * z / (2.0 * trials)
    spread = z * math.sqrt(p * (1.0 - p) / trials + z * z / (4.0 * trials * trials))
    return max(0.0, (centre - spread) / denominator)


def scotts_pi(a: list[bool], b: list[bool]) -> float:
    """Chance-corrected agreement between two raters on a binary question.

    Scott's pi rather than a raw agreement rate, and rather than Cohen's kappa: the two raters are
    interchangeable experts answering the same question, so the chance term is built from the
    pooled marginal rather than from each rater's own.
    """
    if not a or len(a) != len(b):
        return 0.0
    n = len(a)
    observed = sum(1 for x, y in zip(a, b) if x == y) / n
    positives = (sum(a) + sum(b)) / (2.0 * n)
    expected = positives * positives + (1.0 - positives) * (1.0 - positives)
    if expected >= 1.0 - 1e-9:
        # A panel that answered one way every time. Undefined rather than perfect, and reported
        # as zero so it cannot pass a gate by accident.
        return 0.0
    return (observed - expected) / (1.0 - expected)


def preference_gate(preferred: int, trials: int) -> tuple[bool, str]:
    """Whether a build passes the preference gate at one ISO step."""
    if trials < MIN_PANEL:
        return False, f"inconclusive: {trials} judgements is below {MIN_PANEL}"
    bound = wilson_lower_bound(preferred, trials)
    if bound < PREFERENCE_FLOOR:
        return False, f"{preferred}/{trials}, lower bound {bound:.3f} below {PREFERENCE_FLOOR}"
    return True, f"{preferred}/{trials}, lower bound {bound:.3f}"


def identity_gate(distances: list[float]) -> tuple[bool, str]:
    """Whether every delivered face stayed inside the ceiling.

    A **maximum**, not a mean. One face over the ceiling fails a wedding, and the whole point of
    storing the distance on every `restore_face` row is that this is a query rather than a
    sample.
    """
    if not distances:
        return True, "no faces were recovered, so none moved"
    worst = max(distances)
    inside = sum(1 for d in distances if d <= IDENTITY_CEILING)
    rate = inside / len(distances)
    if rate < IDENTITY_PASS_RATE:
        return False, f"{len(distances) - inside} of {len(distances)} faces over {IDENTITY_CEILING}, worst {worst:.4f}"
    return True, f"{len(distances)} faces, worst {worst:.4f}"


def self_test() -> int:
    failures: list[str] = []

    # 1. A build worse than no denoising fails.
    ok, detail = preference_gate(preferred=9, trials=30)
    if ok:
        failures.append(f"1: a 30 % preference passed the gate: {detail}")

    # And a genuinely good build passes - on a panel large enough to say so. A 93 % rate on
    # thirty judgements has a lower bound of 0.787, which is *below* the floor: the gate is read
    # against the interval rather than the point estimate, so a good build measured on a small
    # panel fails too. That is the correct behaviour and it is why section 9 gives QAIQ five days
    # rather than an afternoon.
    ok, detail = preference_gate(preferred=92, trials=100)
    if not ok:
        failures.append(f"1: a 92 % preference on a hundred judgements failed the gate: {detail}")

    # The same rate on a thirty-judgement panel does not pass, and that is deliberate.
    ok, _ = preference_gate(preferred=28, trials=30)
    if ok:
        failures.append("1: a 93 % rate on thirty judgements passed; the interval is not being read")

    # 2. A small panel is inconclusive rather than a pass. Four of five is 80 % and is not an
    #    80 % preference.
    ok, detail = preference_gate(preferred=4, trials=5)
    if ok:
        failures.append(f"2: a five-judgement panel passed: {detail}")
    if "inconclusive" not in detail:
        failures.append(f"2: a five-judgement panel was not reported as inconclusive: {detail}")

    # 3. Agreement is chance-corrected, and a panel at an extreme marginal rate can still reach
    #    the margin. Phase 21's defect, inherited as a test rather than as a bug.
    n = 200
    # 97 % of the judgements are positive - the extreme rate that broke phase 21's absolute
    # margin - and the two raters disagree on four of them.
    rater_a = [True] * n
    rater_b = [True] * n
    for index in range(6):
        rater_a[index] = False
        rater_b[index] = False
    for index in range(6, 10):
        rater_a[index] = False
    pi = scotts_pi(rater_a, rater_b)
    if pi < AGREEMENT_FLOOR:
        failures.append(
            f"3: a panel disagreeing on 4 of 200 at a 97 % rate scored pi {pi:.3f}, below "
            f"{AGREEMENT_FLOOR}; this is phase 21's defect"
        )
    # And a panel that agrees only by chance does not pass.
    coin_a = [index % 2 == 0 for index in range(n)]
    coin_b = [index % 3 == 0 for index in range(n)]
    if scotts_pi(coin_a, coin_b) >= AGREEMENT_FLOOR:
        failures.append("3: two unrelated raters passed the agreement floor")

    # 4. The identity gate is a maximum. An estimator that averaged would pass a build that had
    #    changed one person's face completely.
    ok, detail = identity_gate([0.01] * 99 + [0.9])
    if ok:
        failures.append(f"4: one face at 0.9 passed a gate of {IDENTITY_CEILING}: {detail}")
    ok, detail = identity_gate([0.01, 0.03, 0.079])
    if not ok:
        failures.append(f"4: three faces inside the ceiling failed: {detail}")

    for line in failures:
        print(f"FAIL {line}")
    if failures:
        return 1
    print(
        "eval_restore self-test: 4 estimator properties hold "
        f"(pi {pi:.3f} at a 97 % marginal rate, panel floor {MIN_PANEL}, identity ceiling "
        f"{IDENTITY_CEILING} as a maximum)"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the estimators on authored data")
    parser.add_argument("--panel", help="a CSV of expert judgements")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.panel:
        print(
            "eval_restore: there is no expert panel and no reference wedding in this repository. "
            "PHASE-22 section 9 gives QAIQ five days for a competitive study at four ISO steps "
            f"({', '.join(str(s) for s in ISO_STEPS)}); none has been run. Run with --self-test.",
            file=sys.stderr,
        )
        return 2
    parser.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
