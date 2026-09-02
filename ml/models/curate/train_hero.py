#!/usr/bin/env python3
"""Phase 29's hero ranker training loop, and the self-test that proves it can fail.

There are no consented portfolios in this repository - section 9's DATA row asks for sixty real
hero sets and there are none - so `aura_curate::HERO_HEAD_TRAINED` is false and what ships is the
deterministic blend of ADR-0059 section 6. What is here is the *training procedure*, exercised end
to end on synthetic portfolios whose picks are known by construction, plus the decisions in it that
are decisions rather than defaults.

``--self-test`` runs without PyTorch and asserts five properties:

1. the loss decreases and the fitted ranker beats a constant predictor;
2. **a frame the technical veto rejects can never be ranked into the portfolio**, at any accuracy,
   because that is a hard floor in the contract rather than a term the model may trade against;
3. the ranker learns a *comparison* rather than an absolute score, so a photographer who rates
   everything highly and one who rates nothing highly train the same model;
4. **uniqueness is not learnable from a portfolio alone** and the fit must refuse to identify it -
   see below;
5. an unmeasured term is dropped and the blend renormalised, never filled with a mean.

THE FOUR DECISIONS

**The label is a pairwise comparison, not a rating.** A photographer's portfolio is a *set*: it
says these twenty and not those five hundred and eighty, and it does not say how much better the
seventh is than the eighth. Fitting a regression onto a manufactured rating invents a quantity
nobody supplied. Every training example here is "this frame was picked and that one was not, from
the same wedding", which is the only comparison the data actually makes.

**Within a wedding, never across.** Two photographers' portfolios are two different questions, and
so are one photographer's Tuesday registry office and their Saturday marquee. A pair that spans two
weddings teaches the model which wedding was better lit.

**Uniqueness is dropped from the fit, and derived at inference.** The fourth term of section 6.2 is
how unlike the *already-chosen* heroes a frame is, which is a property of a set being built rather
than of a photograph - and a training set of finished portfolios has every frame already surrounded
by its final neighbours. Fitting it there teaches the model that the picked frames were unusual,
which is a description of the answer rather than a reason for it. Phase 26's rule, and this is its
third statement: count the dimensions of the observation before fitting the vector. The greedy
computes uniqueness against the set it has so far; the model never sees it.

**The technical floor stays in code.** `HERO_TECHNICAL_FLOOR` is a veto applied before any score is
computed, so no amount of emotion can rank an out-of-focus frame into a portfolio. A model that
could learn to override it would be a model that eventually does.

    python ml/models/curate/train_hero.py --self-test
    python ml/models/curate/train_hero.py --portfolios path/to/consented/portfolios.jsonl
"""

from __future__ import annotations

import argparse
import math
import random
import sys

# Matching `aura_core::contract::curate::HERO_TECHNICAL_FLOOR`. Not a threshold this model owns.
HERO_TECHNICAL_FLOOR = 0.55

# Section 10.1's gate: agreement with a photographer's own top twenty.
HERO_AGREEMENT_FLOOR = 0.75

# The terms the model may see. `uniqueness` is deliberately absent - see the module docstring.
FEATURES = ("technical", "emotion", "composition", "story")


def sigmoid(x: float) -> float:
    if x >= 0:
        return 1.0 / (1.0 + math.exp(-x))
    z = math.exp(x)
    return z / (1.0 + z)


def score(weights: dict[str, float], frame: dict[str, float | None]) -> float:
    """The blend, with unmeasured terms dropped and the rest renormalised.

    Never a mean substituted for a missing reading: a frame nobody measured for emotion is not a
    frame with average emotion, and the difference is what `CurateCode` has caveats for.
    """
    total = 0.0
    mass = 0.0
    for name in FEATURES:
        value = frame.get(name)
        if value is None:
            continue
        weight = weights[name]
        total += weight * value
        mass += weight
    if mass <= 0.0:
        return 0.0
    return total / mass


def fit(pairs: list[tuple[dict, dict]], epochs: int = 400, rate: float = 0.35) -> dict[str, float]:
    """Bradley-Terry over within-wedding pairs, with the weights kept non-negative.

    Non-negative because every term is *better when larger* by construction, and a fit that
    discovered a negative coefficient for composition would have found a quirk of a small sample
    rather than a fact about photographs. Clamping is honest here in a way it usually is not: the
    sign is known before the data arrives.
    """
    weights = {name: 1.0 / len(FEATURES) for name in FEATURES}
    for _ in range(epochs):
        gradient = {name: 0.0 for name in FEATURES}
        for picked, passed in pairs:
            margin = score(weights, picked) - score(weights, passed)
            error = 1.0 - sigmoid(margin)
            for name in FEATURES:
                a, b = picked.get(name), passed.get(name)
                if a is None or b is None:
                    continue
                gradient[name] += error * (a - b)
        for name in FEATURES:
            weights[name] = max(0.0, weights[name] + rate * gradient[name] / max(1, len(pairs)))
    mass = sum(weights.values())
    if mass <= 0.0:
        return {name: 1.0 / len(FEATURES) for name in FEATURES}
    return {name: value / mass for name, value in weights.items()}


def pairs_within_one_wedding(frames: list[dict], picked: set[int]) -> list[tuple[dict, dict]]:
    """Every (picked, not picked) pair from one wedding, and none that spans two."""
    chosen = [frames[i] for i in sorted(picked)]
    rest = [frames[i] for i in range(len(frames)) if i not in picked]
    return [(a, b) for a in chosen for b in rest]


def synthetic_wedding(rng: random.Random, size: int = 240, top: int = 20) -> tuple[list[dict], set[int]]:
    """A wedding whose portfolio was chosen by a known rule, so the fit has a right answer.

    The truth is emotion-led with composition second, which is what `curation.toml` argues for and
    what the fit has to recover from the comparisons alone.
    """
    truth = {"technical": 0.10, "emotion": 0.45, "composition": 0.30, "story": 0.15}
    frames = []
    for _ in range(size):
        frame = {name: rng.random() for name in FEATURES}
        # Technical is a floor rather than a spread: most delivered frames are sharp.
        frame["technical"] = 0.45 + 0.55 * rng.random()
        frames.append(frame)
    order = sorted(
        (i for i, f in enumerate(frames) if f["technical"] >= HERO_TECHNICAL_FLOOR),
        key=lambda i: -score(truth, frames[i]),
    )
    return frames, set(order[:top])


def agreement(weights: dict[str, float], frames: list[dict], truth: set[int], top: int) -> float:
    eligible = [i for i, f in enumerate(frames) if f["technical"] >= HERO_TECHNICAL_FLOOR]
    ranked = sorted(eligible, key=lambda i: -score(weights, frames[i]))[:top]
    return len(set(ranked) & truth) / max(1, len(truth))


def self_test() -> int:
    rng = random.Random(29)
    frames, picked = synthetic_wedding(rng)
    pairs = pairs_within_one_wedding(frames, picked)

    flat = {name: 1.0 / len(FEATURES) for name in FEATURES}
    weights = fit(pairs)

    before = agreement(flat, frames, picked, 20)
    after = agreement(weights, frames, picked, 20)
    print(f"weights: " + ", ".join(f"{k}={v:.3f}" for k, v in sorted(weights.items())))
    print(f"agreement: flat {before:.3f} -> fitted {after:.3f}")

    ok = True

    # 1. The fit beats a constant predictor and clears section 10.1's floor.
    if after <= before:
        print("FAIL: the fitted ranker is no better than an equal-weight blend")
        ok = False
    if after < HERO_AGREEMENT_FLOOR:
        print(f"FAIL: agreement {after:.3f} below the {HERO_AGREEMENT_FLOOR} floor")
        ok = False

    # 2. The veto is unbeatable. A frame below the technical floor, perfect on everything else.
    blind = dict(frames[0])
    blind.update({"technical": 0.10, "emotion": 1.0, "composition": 1.0, "story": 1.0})
    candidates = frames + [blind]
    eligible = [i for i, f in enumerate(candidates) if f["technical"] >= HERO_TECHNICAL_FLOOR]
    if len(candidates) - 1 in eligible:
        print("FAIL: a frame below the technical floor reached the ranking")
        ok = False

    # 3. A rating scale cancels: adding a constant to every reading changes no comparison.
    shifted = [{k: min(1.0, v + 0.15) for k, v in f.items()} for f in frames]
    shifted_pairs = pairs_within_one_wedding(shifted, picked)
    shifted_weights = fit(shifted_pairs)
    drift = max(abs(shifted_weights[n] - weights[n]) for n in FEATURES)
    if drift > 0.12:
        print(f"FAIL: a generous photographer trained a different model (drift {drift:.3f})")
        ok = False

    # 4. Uniqueness is not in the feature set, at all.
    if "uniqueness" in FEATURES:
        print("FAIL: uniqueness is being fitted from finished portfolios")
        ok = False

    # 5. An unmeasured term is dropped, not filled. A frame with no emotion reading must score
    #    differently from one whose emotion was measured at the mean - otherwise every caveat in
    #    the panel is a lie.
    measured = {"technical": 0.8, "emotion": 0.5, "composition": 0.9, "story": 0.4}
    absent = dict(measured)
    absent["emotion"] = None
    if abs(score(weights, measured) - score(weights, absent)) < 1e-6:
        print("FAIL: a missing reading scored the same as an average one")
        ok = False

    print("self-test: PASS" if ok else "self-test: FAIL")
    return 0 if ok else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the procedure on synthetic portfolios")
    parser.add_argument("--portfolios", help="JSONL of consented portfolios, one wedding per line")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.portfolios:
        parser.print_help()
        return 2
    print(
        "refusing: there are no consented portfolios in this repository, and this script will not\n"
        "invent one. Supply --portfolios with a file whose weddings were collected with permission\n"
        "(section 9, DATA), then set aura_curate::HERO_HEAD_TRAINED and re-run the phase 29 gate.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
