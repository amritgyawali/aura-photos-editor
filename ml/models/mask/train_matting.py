#!/usr/bin/env python3
"""Phase 18's matting training loop, and the self-test that proves its losses can fail.

There is no alpha ground truth in this repository. What ships is the *procedure* and the three
decisions inside it, exercised on synthetic bands whose alpha is known by construction.

``--self-test`` runs without PyTorch and asserts three properties:

1. the composite loss decreases and the fit beats predicting a constant half;
2. **the gradient term is doing something** - a run with the composite loss alone and a run with
   the composite plus the gradient term disagree, and the second matches the truth's own
   transition more closely, which is the whole reason the term is there;
3. **the loss can fail**: a matte that ignores the image and returns the trimap's own coarse
   answer is caught rather than scoring well, because that is the exact degenerate solution a
   matting network falls into.

Property 3 is the important one and it is the same trap the Rust side guards with
``matting::VARIANCE_FLOOR``. A network that learns "return the dilated coarse mask" scores
respectably on alpha error and produces a ten-pixel halo around every subject.

THE THREE DECISIONS

**The loss is on the composite, not only on alpha.** ``|a*F + (1-a)*B - I|`` in linear light,
alongside ``|a - a_true|``. Alpha error alone treats a wrong alpha over a boundary where the
foreground and the background are the same colour as seriously as one where they differ by two
stops - and only the second is visible. The composite term is what makes the loss agree with
what a photographer sees at 100 % zoom.

**A gradient term on alpha.** ``|grad a - grad a_true|`` holds the transition to the width the
photograph actually has. Without it the minimiser is free to trade a wrong transition width for
a slightly better mean, because an L1 alpha loss is nearly indifferent between a ramp that is
too soft and one that is too hard as long as it crosses in the right place - and a transition of
the wrong width is precisely what a halo is. The self-test asserts the term reduces the gradient
error rather than that it sharpens: on a genuinely soft edge, matching the truth means being
*softer*, and a term that only ever sharpened would be a term that puts a hard edge on a veil.

**Only the band contributes.** Pixels the trimap calls foreground or background are excluded
from the loss entirely rather than supervised toward one and zero. Supervising them teaches the
network to reproduce the trimap, which is the degenerate solution property 3 tests for.
"""

from __future__ import annotations

import argparse
import json
import math
import random
import sys
from typing import Any

# How many synthetic band samples the self-test fits on.
SELF_TEST_SAMPLES = 400

# Weights on the three loss terms. Composite dominates; see the module docstring.
W_ALPHA = 1.0
W_COMPOSITE = 2.0
W_GRADIENT = 0.5

SEED = 18

# Below this separation between foreground and background the guide carries no information and
# the model keeps the coarse answer. Mirrors `matting::VARIANCE_FLOOR` on the Rust side, in the
# units this file works in - a luminance difference rather than a variance.
FLAT_BOUNDARY = 0.05


def synthetic_band(rng: random.Random, width: int = 24) -> dict[str, Any]:
    """One row across a boundary: a true alpha ramp, and the pixels it produces.

    The ramp is placed at a random offset with a random width, so a model that learns "the edge
    is always in the middle" cannot pass. Foreground and background luminances are drawn
    independently, so some samples have a strong boundary and some have almost none - which is
    the case the composite term treats differently from the alpha term, and the reason both are
    in the loss.
    """
    edge = rng.uniform(width * 0.3, width * 0.7)
    softness = rng.uniform(0.8, 4.0)
    fg = rng.uniform(0.05, 0.9)
    bg = rng.uniform(0.05, 0.9)

    alpha = []
    image = []
    for x in range(width):
        a = 1.0 / (1.0 + math.exp((x - edge) / softness))
        alpha.append(a)
        image.append(a * fg + (1.0 - a) * bg)
    return {
        "alpha": alpha,
        "image": image,
        "fg": fg,
        "bg": bg,
        "edge": edge,
        "softness": softness,
    }


def predict(sample: dict[str, Any], params: list[float]) -> list[float]:
    """A three-parameter stand-in for the network: a logistic over the normalised guide.

    The guide is ``(I - bg) / (fg - bg)``, which is what a matting network effectively recovers
    locally: inside a band, the pixel is a mixture of two colours and its position between them
    *is* the alpha. The logistic over it is the network's freedom to be sharper or softer than
    the evidence, and `scale` is the parameter the gradient term pushes on.

    `params` are (scale, bias, blend). `blend` mixes toward the coarse step, and it is here on
    purpose: at ``blend = 1`` the model is "return the dilated coarse mask", which is exactly the
    degenerate solution a matting network collapses into when the certain regions are supervised.
    Property 3 of the self-test is that the loss can tell that solution apart from an honest one.

    When the foreground and the background are within ``FLAT_BOUNDARY`` of each other the guide
    carries no information and the model falls back to the coarse answer - the same guard
    ``matting::VARIANCE_FLOOR`` implements on the Rust side, and for the same reason.
    """
    scale, bias, blend = params
    fg = sample["fg"]
    bg = sample["bg"]
    span = fg - bg
    width = len(sample["image"])
    out = []
    for x, value in enumerate(sample["image"]):
        coarse = 1.0 if x < width / 2 else 0.0
        if abs(span) < FLAT_BOUNDARY:
            out.append(coarse)
            continue
        guide = (value - bg) / span
        learned = 1.0 / (1.0 + math.exp(-(scale * (guide - bias))))
        out.append(max(0.0, min(1.0, (1.0 - blend) * learned + blend * coarse)))
    return out


def losses(sample: dict[str, Any], alpha: list[float]) -> dict[str, float]:
    """The three terms, separately, so the self-test can see which one is doing the work."""
    truth = sample["alpha"]
    fg = sample["fg"]
    bg = sample["bg"]
    image = sample["image"]
    n = max(len(truth), 1)

    alpha_error = sum(abs(a - t) for a, t in zip(alpha, truth)) / n
    composite = (
        sum(abs(a * fg + (1.0 - a) * bg - i) for a, i in zip(alpha, image)) / n
    )
    grad_pred = [alpha[i + 1] - alpha[i] for i in range(len(alpha) - 1)]
    grad_true = [truth[i + 1] - truth[i] for i in range(len(truth) - 1)]
    gradient = sum(abs(p - t) for p, t in zip(grad_pred, grad_true)) / max(len(grad_pred), 1)

    return {"alpha": alpha_error, "composite": composite, "gradient": gradient}


def total(parts: dict[str, float], with_gradient: bool = True) -> float:
    return (
        W_ALPHA * parts["alpha"]
        + W_COMPOSITE * parts["composite"]
        + (W_GRADIENT * parts["gradient"] if with_gradient else 0.0)
    )


def fit(
    samples: list[dict[str, Any]],
    with_gradient: bool,
    steps: int = 400,
) -> tuple[list[float], float, float]:
    """Coordinate descent over the three parameters.

    Coordinate descent rather than gradient descent because the loss has an absolute value in
    every term and a subgradient implementation would be more code than the property needs. It
    is deterministic, which is the requirement that matters.
    """
    params = [4.0, 0.5, 0.5]
    first = mean_loss(samples, params, with_gradient)
    step = [2.0, 0.2, 0.25]
    for _ in range(steps):
        for index in range(3):
            best = mean_loss(samples, params, with_gradient)
            for delta in (step[index], -step[index]):
                trial = params[:]
                trial[index] = trial[index] + delta
                if index == 2:
                    trial[index] = max(0.0, min(1.0, trial[index]))
                score = mean_loss(samples, trial, with_gradient)
                if score < best:
                    best = score
                    params = trial
        step = [s * 0.97 for s in step]
    return params, first, mean_loss(samples, params, with_gradient)


def mean_loss(samples: list[dict[str, Any]], params: list[float], with_gradient: bool) -> float:
    return sum(
        total(losses(s, predict(s, params)), with_gradient) for s in samples
    ) / max(len(samples), 1)


def self_test() -> int:
    rng = random.Random(SEED)
    samples = [synthetic_band(rng) for _ in range(SELF_TEST_SAMPLES)]
    problems: list[str] = []

    # 1. The loss decreases and beats a constant half.
    params, first, last = fit(samples, with_gradient=True)
    if last >= first:
        problems.append(f"the loss did not decrease: {first:.4f} -> {last:.4f}")
    constant = sum(
        total(losses(s, [0.5] * len(s["alpha"])), True) for s in samples
    ) / len(samples)
    if last >= constant:
        problems.append(
            f"the fit {last:.4f} is no better than predicting a constant half {constant:.4f}"
        )

    # 2. The gradient term changes the answer, and moves it toward the truth's own transition.
    plain, _, _ = fit(samples, with_gradient=False)
    if abs(plain[0] - params[0]) < 1e-6:
        problems.append("the gradient term changed nothing about the fitted model")
    grad_with = sum(losses(s, predict(s, params))["gradient"] for s in samples) / len(samples)
    grad_without = sum(losses(s, predict(s, plain))["gradient"] for s in samples) / len(samples)
    if grad_with >= grad_without:
        problems.append(
            f"the gradient term did not improve the transition: "
            f"{grad_without:.4f} without, {grad_with:.4f} with"
        )

    # 3. The degenerate solution is caught.
    #    `blend = 1` is "return the coarse mask", which is what a matting network collapses to
    #    when the band is supervised toward the trimap. It must score worse than the honest fit.
    degenerate = [params[0], params[1], 1.0]
    degenerate_loss = mean_loss(samples, degenerate, True)
    if degenerate_loss <= last:
        problems.append(
            f"returning the coarse mask scored {degenerate_loss:.4f} against an honest fit of "
            f"{last:.4f}; the loss cannot tell them apart"
        )

    for problem in problems:
        print(f"train_matting: {problem}", file=sys.stderr)
    if problems:
        return 1

    print(f"train_matting: loss {first:.4f} -> {last:.4f} over {len(samples)} bands")
    print(
        f"train_matting: gradient error {grad_without:.4f} without the term, "
        f"{grad_with:.4f} with it (scale {plain[0]:.2f} -> {params[0]:.2f})"
    )
    print(
        f"train_matting: returning the coarse mask scores {degenerate_loss:.4f}, worse than the "
        f"honest fit at {last:.4f}"
    )
    print(
        "train_matting: NO ALPHA GROUND TRUTH IN THIS REPOSITORY. This exercises the procedure "
        "on synthetic bands and says nothing about a veil."
    )
    return 0


def describe() -> dict[str, Any]:
    return {
        "loss": {
            "alpha_l1": W_ALPHA,
            "composite_l1_linear_light": W_COMPOSITE,
            "alpha_gradient_l1": W_GRADIENT,
        },
        "supervised_region": "the trimap band only; foreground and background are excluded",
        "why": "supervising the certain regions teaches the network to reproduce the trimap, "
        "which is a ten-pixel halo around every subject",
        "seed": SEED,
        "trained": False,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the procedure checks")
    parser.add_argument("--describe", action="store_true", help="print the training recipe")
    args = parser.parse_args(argv)

    if args.describe:
        print(json.dumps(describe(), indent=2))
        return 0
    if args.self_test:
        return self_test()

    print(
        "train_matting: no alpha ground truth is available in this repository, so there is "
        "nothing to train. Run with --self-test to exercise the procedure.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
