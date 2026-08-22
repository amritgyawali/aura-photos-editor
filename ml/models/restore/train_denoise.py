#!/usr/bin/env python3
"""Phase 22's denoiser training loop, and the self-test that proves it can fail.

There is no paired noisy/clean capture in this repository. What ships is the *training
procedure*, exercised end to end on synthetic tiles whose noise is known by construction, plus
the decisions in it that are decisions rather than defaults.

``--self-test`` runs without PyTorch and asserts five properties:

1. the loss decreases and the fitted model beats a constant predictor;
2. **a clean tile is left alone** - the residual the model predicts for a tile with no noise in it
   is near zero, at any accuracy, because a denoiser that removes something from a clean frame is
   a denoiser that removes texture from every frame;
3. the model **reads the noise plane** - two tiles with identical pixels and different predicted
   sigmas get different residuals, which is the whole of section 6.1's conditioning and is the one
   property a network can quietly stop having while its loss keeps falling;
4. **chroma is reduced at least as hard as luminance**, because chroma noise carries no detail
   anybody wants while luminance noise sits next to detail that somebody does - and because
   `DenoiseSpec::problem` refuses a plan where it is not, so a head that learned the other way
   round would produce plans the store rejects on every frame;
5. **structure survives** - a step of `EDGE_SIGMAS` or more is not noise the sensor could have
   produced, and a model that removes it has learned to remove edges.

THE FIVE DECISIONS

**The target is a residual, not a clean image.** A network trained to output the photograph has to
reproduce the photograph. Starting from the identity means the model's errors are errors in the
correction, and the failure mode of an under-trained residual model is leaving noise behind -
which a photographer can see and fix. The failure mode of an under-trained image model is invented
texture, which they cannot.

**The noise plane is an input, not a conditioning vector.** Shot noise grows with the square root
of the signal, so a shadow and a highlight in the same tile have different sigmas. A scalar would
force the network to learn that relationship from the pixels it is trying to denoise; a plane
hands it the sensor's own answer. Property 3 is what checks it is being used.

**Luminance and chroma are two heads sharing a trunk, not one head with three channels.** They are
learning different functions: the chroma target is the *whole* of the chroma noise, because a
red-to-green step at constant luminance carries nothing a photographer wants; the luminance target
exempts anything that could not have been noise. Property 4 is the consequence, and it falls out
of the targets rather than being imposed by a penalty.

**Loss is measured in units of the predicted sigma.** A plain L2 on the residual weights a bright
region's error more heavily than a shadow's, because there is more noise there to get wrong - so
the model learns to denoise highlights and ignore shadows, which is the opposite of what a
photographer needs. Dividing the error by the local sigma makes the loss about *fractions of the
noise* rather than about absolute levels.

**No exposure jitter, and no gamma jitter.** Both are standard augmentations and both destroy the
thing this model is for. The relationship between signal level and noise level is a property of
the sensor, and an augmentation that breaks it teaches the head to ignore the plane it was given.
"""

from __future__ import annotations

import argparse
import math
import sys

# Matching `aura_render::restore::EDGE_SIGMAS`. Not a threshold this model owns - the reference
# filter uses it and the trained model has to agree with it about what an edge is, or a frame
# denoised by one looks different from the same frame denoised by the other.
EDGE_SIGMAS = 3.0

# Matching `aura_restore::denoise::CHROMA_OVER_LUMA`. The amount asymmetry, not the radius one.
CHROMA_OVER_LUMA = 1.35

# Section 10.1's gate: the denoiser beats the bilinear baseline decisively.
PSNR_MARGIN_DB = 3.0

# Property 2's ceiling: what a clean tile may be moved by.
CLEAN_TILE_MAX_MOVE = 0.005

# Property 5's ceiling: what share of a real edge may be removed.
STRUCTURE_MAX_LOSS = 0.15

# The luminance head reads the local step, the sigma the sensor predicts there, and the level it
# sits at. The chroma head reads the chroma step and the same two.
FEATURES = ("step", "sigma", "local_mean")


def deterministic(index: int, salt: int) -> float:
    """A hash rather than a generator.

    Invariant 4 requires the same input to produce the same output. A training self-test seeded
    from the clock is a test whose thresholds are a coin toss, and one that fails on a Tuesday is
    a test nobody trusts by Wednesday.
    """
    h = (index * 0x9E3779B97F4A7C15 + salt * 0x123456789ABCDEF1) & 0xFFFFFFFFFFFFFFFF
    h ^= h >> 33
    h = (h * 0xFF51AFD7ED558CCD) & 0xFFFFFFFFFFFFFFFF
    h ^= h >> 33
    return ((h >> 40) / 8388608.0) - 1.0


def make_sample(index: int, sigma: float, clean: bool = False):
    """One training sample: the two feature vectors, and the two residuals wanted.

    The truth is constructed rather than labelled, which is what makes this a test of the
    *procedure*. A real corpus replaces `make_sample` and nothing else.
    """
    level = 0.15 + 0.6 * abs(deterministic(index, 11))
    # The structure this sample carries under its noise. A real edge, one time in five.
    structure = 0.12 if index % 5 == 0 else 0.0
    noise = 0.0 if clean else deterministic(index, 23) * sigma * math.sqrt(3.0)
    chroma_noise = 0.0 if clean else deterministic(index, 29) * sigma * math.sqrt(3.0)

    step = structure + noise
    # What should come off the luminance: the noise, and none of the structure. A step of
    # EDGE_SIGMAS or more is a step the sensor could not have produced by noise, so it is
    # structure by definition and must survive whole.
    luma_target = 0.0 if abs(step) >= sigma * EDGE_SIGMAS else noise

    # What should come off the chroma: all of it. A red-to-green step at constant luminance is
    # a sensor artefact far more often than it is a photograph, and the cases where it is not -
    # a coloured thread on a lapel - are what the *radius* asymmetry protects rather than the
    # amount one. See `aura_render::restore::CHROMA_RADIUS_RATIO`.
    chroma_target = chroma_noise

    return (
        [step, sigma, level],
        luma_target,
        [chroma_noise, sigma, level],
        chroma_target,
    )


def predict(weights: list[float], features: list[float]) -> float:
    return sum(w * f for w, f in zip(weights, features))


def fit(samples: list[tuple[list[float], float]], epochs: int = 600) -> tuple[list[float], list[float]]:
    """Least squares by gradient descent, in units of the local sigma.

    See the module header for why the loss is divided by sigma.

    **The features are whitened and the step size is derived**, and neither is a detail. Dividing
    the error by sigma is what makes the loss about fractions of the noise, and it also multiplies
    the curvature by `1 / sigma^2` - which across the four ISO steps in the set is a spread of
    nearly four orders of magnitude. A fixed rate that converges on a dance floor diverges on a
    portrait: the first version of this function reached `nan` by the fortieth epoch. Whitening
    puts every feature on the same scale, and the rate is then the reciprocal of the largest
    curvature that remains, which is the standard bound for a quadratic.

    `history` is returned so the self-test can assert the loss actually fell rather than assuming
    it.
    """
    width = len(FEATURES)
    scales = []
    for i in range(width):
        rms = math.sqrt(sum(f[i] * f[i] for f, _ in samples) / max(len(samples), 1))
        scales.append(rms if rms > 1e-9 else 1.0)

    whitened = [([f[i] / scales[i] for i in range(width)], t) for f, t in samples]
    curvature = 0.0
    for features, _ in whitened:
        sigma = max(features[1] * scales[1], 1e-6)
        curvature = max(curvature, sum(f * f for f in features) / (sigma * sigma))
    rate = 0.5 / max(curvature, 1e-9)

    weights = [0.0] * width
    history: list[float] = []
    for _ in range(epochs):
        gradients = [0.0] * width
        loss = 0.0
        for features, target in whitened:
            sigma = max(features[1] * scales[1], 1e-6)
            error = (predict(weights, features) - target) / sigma
            loss += error * error
            for i, f in enumerate(features):
                gradients[i] += 2.0 * error * f / sigma
        n = max(len(whitened), 1)
        loss /= n
        history.append(loss)
        for i in range(width):
            weights[i] -= rate * gradients[i] / n

    # Back into the units the caller measures in, so `predict` takes real features.
    return [weights[i] / scales[i] for i in range(width)], history


def psnr(reference: list[float], candidate: list[float]) -> float:
    if not reference or len(reference) != len(candidate):
        return 0.0
    mse = sum((a - b) ** 2 for a, b in zip(reference, candidate)) / len(reference)
    if mse <= 1e-12:
        return 99.0
    return 10.0 * math.log10(1.0 / mse)


def self_test() -> int:
    failures: list[str] = []

    sigmas = [0.004, 0.010, 0.020, 0.035]
    luma_set: list[tuple[list[float], float]] = []
    chroma_set: list[tuple[list[float], float]] = []
    for index in range(1200):
        sigma = sigmas[index % len(sigmas)]
        luma_features, luma_target, chroma_features, chroma_target = make_sample(index, sigma)
        luma_set.append((luma_features, luma_target))
        chroma_set.append((chroma_features, chroma_target))

    luma_weights, history = fit(luma_set)
    chroma_weights, chroma_history = fit(chroma_set)

    # 1. The loss falls, and each head beats a constant predictor.
    if not history[-1] < history[0]:
        failures.append(f"1: the luminance loss did not fall: {history[0]:.5f} -> {history[-1]:.5f}")
    if not chroma_history[-1] < chroma_history[0]:
        failures.append("1: the chroma loss did not fall")
    for name, weights, samples, hist in (
        ("luminance", luma_weights, luma_set, history),
        ("chroma", chroma_weights, chroma_set, chroma_history),
    ):
        mean_target = sum(t for _, t in samples) / len(samples)
        constant = sum(((mean_target - t) / max(f[1], 1e-6)) ** 2 for f, t in samples) / len(samples)
        if not hist[-1] < constant:
            failures.append(f"1: the {name} fit does not beat a constant: {hist[-1]:.5f} vs {constant:.5f}")

    # 2. A clean tile is left alone. The single most important property here: a denoiser that
    #    removes something from a frame with no noise in it removes texture from every frame.
    worst_clean = 0.0
    for index in range(200):
        luma_features, _, chroma_features, _ = make_sample(index, sigma=0.0005, clean=True)
        worst_clean = max(worst_clean, abs(predict(luma_weights, luma_features)))
        worst_clean = max(worst_clean, abs(predict(chroma_weights, chroma_features)))
    if worst_clean > CLEAN_TILE_MAX_MOVE:
        failures.append(f"2: a clean tile moved by {worst_clean:.5f}, above {CLEAN_TILE_MAX_MOVE}")

    # 3. The model reads the noise plane. Two samples with the same pixels and different sigmas
    #    must get different residuals - this is section 6.1's conditioning, and it is the property
    #    a network can quietly stop having while its loss keeps falling.
    quiet = [0.010, 0.004, 0.40]
    loud = [0.010, 0.030, 0.40]
    separation = abs(predict(luma_weights, quiet) - predict(luma_weights, loud))
    if separation < 1e-4:
        failures.append(
            f"3: the model separates two sigmas by {separation:.6f}; it is not conditioned on "
            "the noise plane at all"
        )

    # 4. Chroma is reduced at least as hard as luminance. Measured as the *response to a unit
    #    step* rather than as a raw weight comparison, because the two heads read different
    #    features and a weight is not comparable across them.
    unit = 0.01
    luma_response = abs(predict(luma_weights, [unit, 0.010, 0.40]))
    chroma_response = abs(predict(chroma_weights, [unit, 0.010, 0.40]))
    if chroma_response < luma_response:
        failures.append(
            f"4: chroma removes {chroma_response:.5f} of a unit step and luminance removes "
            f"{luma_response:.5f}; the two are the wrong way round and every plan would be refused"
        )

    # 5. Structure survives. A step of EDGE_SIGMAS or more could not have been noise.
    sigma = 0.010
    edge = sigma * EDGE_SIGMAS * 1.5
    removed = abs(predict(luma_weights, [edge, sigma, 0.40]))
    if removed > edge * STRUCTURE_MAX_LOSS:
        failures.append(
            f"5: a {edge:.4f} edge lost {removed:.4f}, more than {STRUCTURE_MAX_LOSS:.0%}; the "
            "model has learned to remove edges"
        )

    # And the gate itself: the fitted residual model beats a bilinear baseline decisively. The
    # baseline is the mean of the neighbourhood, which as a residual is the whole step.
    clean_signal = []
    denoised = []
    baseline = []
    for index in range(400):
        sigma = sigmas[index % len(sigmas)]
        luma_features, luma_target, _, _ = make_sample(index, sigma)
        truth = luma_features[0] - luma_target
        clean_signal.append(truth)
        denoised.append(luma_features[0] - predict(luma_weights, luma_features))
        baseline.append(0.0)
    margin = psnr(clean_signal, denoised) - psnr(clean_signal, baseline)
    if margin < PSNR_MARGIN_DB:
        failures.append(f"gate: PSNR margin {margin:.2f} dB is below {PSNR_MARGIN_DB} dB")

    for line in failures:
        print(f"FAIL {line}")
    if failures:
        return 1
    print(
        "train_denoise self-test: 5 properties and the PSNR gate hold "
        f"(loss {history[0]:.5f} -> {history[-1]:.5f}, chroma/luma response "
        f"{chroma_response / max(luma_response, 1e-9):.2f}, margin {margin:.2f} dB)"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the procedure on synthetic data")
    parser.add_argument("--data", help="a directory of paired noisy/clean captures")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.data:
        print(
            "train_denoise: there is no paired capture corpus in this repository. "
            "PHASE-22 section 8 step 2 asks for bracketed low-ISO references against high-ISO "
            "captures of the same scene across twenty bodies and six ISO steps; phase 02's first "
            "exit condition is still open. Run with --self-test.",
            file=sys.stderr,
        )
        return 2
    parser.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
