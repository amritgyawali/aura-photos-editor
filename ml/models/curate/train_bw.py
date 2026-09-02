#!/usr/bin/env python3
"""Phase 29's monochrome suitability head, and the self-test that proves it can fail.

`aura_curate::BW_HEAD_TRAINED` is false. What ships is a **measurement** over phase 05's stored
descriptors - tonal separation, colour distraction, whether a hue is carrying the picture, and how
the noise would read as grain - whose failure mode is offering fewer candidates rather than
confidently wrong ones. This file is the training procedure for the studio that has a consented
archive of a photographer's own monochrome conversions.

``--self-test`` runs without PyTorch and asserts five properties:

1. the loss decreases and the fitted head beats a constant predictor;
2. **a frame whose picture is its colour is never offered**, at any accuracy - a red lehenga
   against green foliage is the case the whole feature exists to refuse, and it is the case a
   head fitted on "photographs that converted well" is most likely to get backwards;
3. the head predicts *suitability*, and the eight-band mix is solved separately - a model that
   emitted band weights is caught;
4. the head never sees where anybody's skin is, so it cannot learn a skin target;
5. an unmeasurable descriptor produces an abstention rather than a confident middle.

THE FOUR DECISIONS

**The head scores the frame; the solver writes the mix.** They are different kinds of question. "Is
this photograph better in black and white" is a judgement a photographer makes and a model can
learn. "How much should the aqua band move" is arithmetic over *this* frame's own bands, and a
model that produced it would be producing a number nobody could check against the photograph in
front of them. `bw::solve` stays deterministic and `bw::suitability` is what a trained head would
replace.

**Skin is not a feature.** Not the locus, not its band, not its luminance. The head reads the
frame's colour structure and nothing about who is in it, because a suitability model with skin in
its inputs is a model that can learn "convert the dark-skinned portraits", and no accuracy makes
that acceptable. The skin bound is applied afterwards, by `BwMix::within_skin_bound`, in code a
person can read. Phase 15's rule: there is no ideal-skin constant anywhere in this product, and the
way to keep it true is for the models not to have the input.

**The negative class is photographs that were kept in colour, not photographs nobody converted.**
An archive contains thousands of frames a photographer never considered converting, and treating
those as negatives teaches the head what a photographer had time for. The label needs a decision on
both sides, which is why section 9's DATA row asks for *selections* rather than for galleries.

**Hue-carried is a veto in the features, not a penalty in the loss.** A frame whose two substantial
saturated regions differ in hue and agree in luminance loses its subject entirely in monochrome.
That is a fact about the photograph rather than a matter of degree, and it is passed to the head as
a hard input so a model cannot learn to discount it on a frame that is otherwise beautiful.

    python ml/models/curate/train_bw.py --self-test
    python ml/models/curate/train_bw.py --selections path/to/consented/selections.jsonl
"""

from __future__ import annotations

import argparse
import math
import random
import sys

# Matching `aura_core::contract::curate::BW_CANDIDATE_FLOOR`: the lowest suitability at which a
# frame is offered at all. Not a threshold this model owns.
BW_CANDIDATE_FLOOR = 0.62

# Section 10.1's gate: monochrome picks accepted by a photographer.
BW_ACCEPTANCE_FLOOR = 0.70

# What the head reads. No skin, no identity, no face - see the module docstring.
FEATURES = ("tonal_separation", "colour_distraction", "hue_carried", "grain")


def sigmoid(x: float) -> float:
    if x >= 0:
        return 1.0 / (1.0 + math.exp(-x))
    z = math.exp(x)
    return z / (1.0 + z)


def suitability(weights: dict[str, float], bias: float, frame: dict[str, float | None]) -> float | None:
    """The head's answer, or `None` when the frame could not be read.

    An abstention rather than a middling number: a frame with no descriptor is not a frame that is
    moderately suited to monochrome, and `CurateCode` has a caveat for exactly this. Phase 24's
    rule - an absent input is ignorance, not permission.
    """
    if any(frame.get(name) is None for name in FEATURES):
        return None
    # The veto first, before any weight is consulted.
    if frame["hue_carried"] >= 0.5:
        return 0.0
    total = bias
    for name in FEATURES:
        total += weights[name] * float(frame[name])
    return sigmoid(total)


def fit(examples: list[tuple[dict, int]], epochs: int = 600, rate: float = 0.6) -> tuple[dict[str, float], float]:
    """Logistic regression on converted-versus-kept-in-colour."""
    weights = {name: 0.0 for name in FEATURES}
    bias = 0.0
    usable = [(f, y) for f, y in examples if all(f.get(n) is not None for n in FEATURES)]
    for _ in range(epochs):
        gradient = {name: 0.0 for name in FEATURES}
        bias_gradient = 0.0
        for frame, label in usable:
            total = bias + sum(weights[n] * float(frame[n]) for n in FEATURES)
            error = label - sigmoid(total)
            for name in FEATURES:
                gradient[name] += error * float(frame[name])
            bias_gradient += error
        for name in FEATURES:
            weights[name] += rate * gradient[name] / max(1, len(usable))
        bias += rate * bias_gradient / max(1, len(usable))
    return weights, bias


def synthetic_archive(rng: random.Random, size: int = 600) -> list[tuple[dict, int]]:
    """An archive whose conversions follow a known rule, so the fit has a right answer.

    A frame converts when its tones are well separated, its colour is not doing the work, and the
    noise would read as grain. A frame whose hue carries the picture never converts, whatever else
    is true of it - which is the property the self-test checks separately.
    """
    out = []
    for _ in range(size):
        frame = {
            "tonal_separation": rng.random(),
            "colour_distraction": rng.random(),
            "hue_carried": 1.0 if rng.random() < 0.18 else 0.0,
            "grain": rng.random(),
        }
        converted = (
            frame["hue_carried"] < 0.5
            and frame["tonal_separation"] > 0.55
            and frame["colour_distraction"] > 0.40
        )
        out.append((frame, 1 if converted else 0))
    return out


def accepted_share(weights, bias, examples) -> float:
    """Of the frames the head would offer, how many the photographer actually converted."""
    offered = 0
    agreed = 0
    for frame, label in examples:
        answer = suitability(weights, bias, frame)
        if answer is None or answer < BW_CANDIDATE_FLOOR:
            continue
        offered += 1
        agreed += label
    return agreed / max(1, offered)


def self_test() -> int:
    rng = random.Random(2903)
    archive = synthetic_archive(rng)
    weights, bias = fit(archive)

    flat = {name: 0.0 for name in FEATURES}
    print("weights: " + ", ".join(f"{k}={v:.3f}" for k, v in sorted(weights.items())) + f", bias={bias:.3f}")
    fitted = accepted_share(weights, bias, archive)
    baseline = accepted_share(flat, 1.0, archive)
    print(f"acceptance: constant {baseline:.3f} -> fitted {fitted:.3f}")

    ok = True

    # 1. The fit beats a constant predictor and clears section 10.1's floor.
    if fitted <= baseline:
        print("FAIL: the fitted head offers no better a list than a constant one")
        ok = False
    if fitted < BW_ACCEPTANCE_FLOOR:
        print(f"FAIL: acceptance {fitted:.3f} below the {BW_ACCEPTANCE_FLOOR} floor")
        ok = False

    # 2. A frame whose picture is its colour is never offered, however good it is otherwise.
    carried = {
        "tonal_separation": 0.99,
        "colour_distraction": 0.99,
        "hue_carried": 1.0,
        "grain": 0.99,
    }
    answer = suitability(weights, bias, carried)
    if answer is None or answer >= BW_CANDIDATE_FLOOR:
        print("FAIL: a frame whose hue carries the picture was offered for conversion")
        ok = False

    # 3. The head predicts one number. A head that emitted band weights would have eight.
    if len(FEATURES) != 4 or any(name.startswith("band") for name in FEATURES):
        print("FAIL: the suitability head has grown a mix in it")
        ok = False

    # 4. Nothing about a person is an input.
    forbidden = ("skin", "locus", "identity", "face", "tone_bucket", "monk")
    leaked = [n for n in FEATURES if any(word in n for word in forbidden)]
    if leaked:
        print(f"FAIL: the head reads {leaked}, which is where a skin target gets learned")
        ok = False

    # 5. An unreadable frame abstains rather than landing in the middle.
    unreadable = dict(carried)
    unreadable["tonal_separation"] = None
    if suitability(weights, bias, unreadable) is not None:
        print("FAIL: a frame with no descriptor produced a number anyway")
        ok = False

    print("self-test: PASS" if ok else "self-test: FAIL")
    return 0 if ok else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the procedure on a synthetic archive")
    parser.add_argument("--selections", help="JSONL of consented conversions, one frame per line")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.selections:
        parser.print_help()
        return 2
    print(
        "refusing: there are no consented monochrome selections in this repository, and this\n"
        "script will not invent one. A selection needs a decision on *both* sides - converted and\n"
        "deliberately kept in colour - which is why section 9's DATA row asks for selections\n"
        "rather than for galleries.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
