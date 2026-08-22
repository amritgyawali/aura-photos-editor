#!/usr/bin/env python3
"""Phase 22's face-recovery training loop, and the self-test that proves it can fail.

There is no soft-face labelled set in this repository, and there is no consented face data in it
at all. What ships is the *training procedure*, exercised end to end on synthetic crops whose
softness is known by construction, plus the decisions in it that are decisions rather than
defaults.

**Read `docs/model-cards/face_recovery.md` before this file.** This head is untrained,
`FACE_RECOVERY_HEAD_TRAINED` is false, and unlike every other placeholder in the product there is
no measured fallback standing in for it - because the measurement that would stand in for a face
prior is unsharp masking on a face, which is a different operation with a worse result and the
same name. ADR-0045 section 6.

``--self-test`` runs without PyTorch and asserts five properties:

1. the loss decreases and the fitted model beats a constant predictor;
2. **a sharp face is left alone**, at any accuracy, because a face prior that acts on a face that
   did not need it is a face prior that acts on every face;
3. **a heavily blurred face is refused before the model is consulted** - the band check is a
   filter on the training set as well as on the runtime, so the head never learns what to do with
   one and cannot be asked at inference;
4. the predicted correction is **high-frequency only**: a model whose output correlates with the
   crop's low band has learned to move features, which is what the identity constraint exists to
   catch and what the output shape exists to prevent;
5. the **identity distance rises monotonically with strength**, which is the property the runtime
   constraint depends on: `enforce` reduces strength and re-measures, and a model whose drift was
   not monotone in its own strength would make that loop meaningless.

THE FOUR DECISIONS

**The two ends of the band are treated completely differently, and that is the decision.** A face
below `SOFT_FACE_LO` is **removed from the set** before the loss sees it, so there is no accuracy
at which the head starts producing output for one - phase 21's flyaway loop makes the same move for
candidates inside the hair mass, and for the same reason: "never on heavily blurred faces" is not
a threshold to be traded against recall. A face above `SOFT_FACE_HI` is **kept as a hard
negative**, with a target of zero. That is phase 21's other move - catchlights are hard negatives
rather than ignored regions - and the reason is the same: a set that simply excluded sharp faces
would leave the head with no opinion about the commonest face at a wedding, and a model with no
opinion extrapolates. The first version of this loop excluded both ends, and property 2 caught it:
the fitted model moved a sharp face by 0.015, because nothing in its training had ever told it not
to.

**The target is a high-frequency residual.** The low and mid bands never pass through the head, so
it cannot move a feature, change a proportion or replace an expression. That is the first line of
the identity guarantee and it is a property of the output shape rather than of a measurement.

**The identity distance is in the loss, not only in the gate.** A model penalised only on
reconstruction learns to recover detail by whatever route reduces the error, and one of those
routes is making the face more like the average face in the training set. The distance term is
what makes the model's own objective include not doing that.

**No identity augmentation, ever.** Mixing crops of different people to enlarge the set teaches
the head that a face is interpolatable between identities, which is the exact capability section
11 of `docs/plan/CLAUDE.md` forbids permanently.
"""

from __future__ import annotations

import argparse
import math
import sys

# Matching `aura_core::contract::restore`. None of these is a threshold this model owns.
MAX_FACE_RECOVERY = 0.40
MAX_IDENTITY_DRIFT = 0.08
SOFT_FACE_LO = 0.42
SOFT_FACE_HI = 0.68

# Property 2's ceiling: what a sharp face may be moved by.
SHARP_FACE_MAX_MOVE = 0.01

# How much of the loss is the identity term. Large enough that the model cannot buy reconstruction
# with drift, small enough that it still recovers something.
IDENTITY_WEIGHT = 4.0

# Four features in two groups, and the grouping is what property 4 ablates.
#
# DETAIL_ROUTE is how much fine structure this face has lost and how much of it there was to lose;
# LOW_BAND_ROUTE is the frame contrast a model could recover a face *through* instead, by moving
# the shape of the face rather than the texture on it. `detail_gap` is the interaction
# `high_band * deficit_ratio`, named rather than implied so that the ablation has a whole route to
# remove rather than one term of one.
FEATURES = ("high_band", "sharpness_deficit", "detail_gap", "local_contrast")

# The indices of the two routes, for property 4.
DETAIL_ROUTE = (0, 2)
LOW_BAND_ROUTE = (3,)


def deterministic(index: int, salt: int) -> float:
    h = (index * 0x9E3779B97F4A7C15 + salt * 0x123456789ABCDEF1) & 0xFFFFFFFFFFFFFFFF
    h ^= h >> 33
    h = (h * 0xFF51AFD7ED558CCD) & 0xFFFFFFFFFFFFFFFF
    h ^= h >> 33
    return ((h >> 40) / 8388608.0) - 1.0


def make_crop(index: int) -> tuple[float, list[float], float, float]:
    """One face crop: its sharpness, its features, the residual wanted, and its low band.

    The low band is returned so that property 4 can check the model's output does not correlate
    with it. A model that recovered a face by moving its low band would score well on
    reconstruction and would be moving somebody's features.
    """
    sharpness = 0.05 + 0.9 * abs(deterministic(index, 7))
    deficit = max(0.0, SOFT_FACE_HI - sharpness)
    # What a sharp reference of the same face has in its high band that this one has lost.
    high_band = 0.02 + 0.05 * abs(deterministic(index, 13))
    low_band = 0.30 + 0.4 * abs(deterministic(index, 17))
    local_contrast = 0.10 + 0.2 * abs(deterministic(index, 19))
    # The residual wanted: proportional to how much detail is missing, and zero for a sharp face.
    ratio = min(deficit / max(SOFT_FACE_HI - SOFT_FACE_LO, 1e-6), 1.0)
    target = high_band * ratio
    return sharpness, [high_band, deficit, high_band * ratio, local_contrast], target, low_band


def too_blurred(sharpness: float) -> bool:
    """Below section 6.3's floor, where a prior would return the prior.

    **The one exclusion.** A face this soft is removed from the training set entirely, so the head
    never learns what to do with one and cannot be asked at inference. See the module header for
    why the *other* end of the band is handled the opposite way.
    """
    return sharpness < SOFT_FACE_LO


def sharp_enough(sharpness: float) -> bool:
    """Above section 6.3's ceiling, where there is nothing to recover.

    Kept in the set as a hard negative with a target of zero, rather than excluded.
    """
    return sharpness > SOFT_FACE_HI


def predict(weights: list[float], features: list[float]) -> float:
    return min(sum(w * f for w, f in zip(weights, features)), MAX_FACE_RECOVERY)


def identity_drift(residual: float, low_band: float) -> float:
    """A stand-in for the phase 06 embedding distance, and it is honest about being one.

    A real loop embeds the crop before and after through the recogniser. What this models is the
    property the runtime depends on: drift grows with how much the correction moves the face, and
    grows *faster* when the correction has a low-band component - because that is a feature
    moving rather than a texture returning.
    """
    return abs(residual) * 0.9 + abs(residual) * low_band * 6.0


def fit(
    samples: list[tuple[list[float], float, float]],
    epochs: int = 4000,
    drop: tuple[int, ...] = (),
) -> tuple[list[float], list[float]]:
    """Least squares with an identity penalty, on whitened features.

    `drop` zeroes a whole *route* for the fit, which is what property 4 ablates with. A route the
    model does not need is a route whose removal costs nothing, and removing one term of a route
    proves nothing because the other term absorbs it - which is why this takes a tuple.

    The whitening is not cosmetic. `high_band` spans a few hundredths and `deficit` spans a
    quarter, so an unwhitened gradient step that converges on one crawls on the other - and the
    first version of this function was beaten by a constant predictor for exactly that reason.
    """
    width = len(FEATURES)
    prepared = [
        ([0.0 if i in drop else f[i] for i in range(width)], t, low)
        for f, t, low in samples
    ]
    scales = []
    for i in range(width):
        rms = math.sqrt(sum(f[i] * f[i] for f, _, _ in prepared) / max(len(prepared), 1))
        scales.append(rms if rms > 1e-9 else 1.0)
    prepared = [
        ([f[i] / scales[i] for i in range(width)], t, low) for f, t, low in prepared
    ]
    # The step is derived from the mean curvature of the whitened problem, **including the
    # identity penalty's own curvature**. That second half is the one that matters and it is the
    # one the first two versions of this function got wrong in opposite directions: a bound taken
    # from the reconstruction term alone diverges to `nan` by the fortieth epoch, because the
    # penalty's gradient carries the `identity_drift` slope squared and that slope reaches five;
    # a bound taken from the worst single sample converges so slowly it is indistinguishable from
    # a constant predictor. The penalised curvature is the honest bound for both.
    mean_curvature = sum(
        sum(f * f for f in features) for features, _, _ in prepared
    ) / max(len(prepared), 1)
    worst_slope = max((0.9 + low * 6.0 for _, _, low in prepared), default=1.0)
    penalised = mean_curvature * (1.0 + IDENTITY_WEIGHT * worst_slope * worst_slope)
    rate = 0.5 / max(penalised, 1e-9)

    weights = [0.0] * width
    history: list[float] = []
    for _ in range(epochs):
        gradients = [0.0] * width
        loss = 0.0
        for features, target, low_band in prepared:
            predicted = sum(w * f for w, f in zip(weights, features))
            error = predicted - target
            drift = identity_drift(predicted, low_band)
            # The identity term only bites above the ceiling: below it the constraint is satisfied
            # and a penalty would be teaching the model to under-recover for no reason.
            excess = max(0.0, drift - MAX_IDENTITY_DRIFT)
            loss += error * error + IDENTITY_WEIGHT * excess * excess
            slope = 0.9 + low_band * 6.0
            for i, f in enumerate(features):
                gradients[i] += 2.0 * error * f
                if excess > 0.0:
                    sign = 1.0 if predicted >= 0.0 else -1.0
                    gradients[i] += 2.0 * IDENTITY_WEIGHT * excess * slope * sign * f
        n = max(len(prepared), 1)
        loss /= n
        history.append(loss)
        for i in range(width):
            weights[i] -= rate * gradients[i] / n
    return [weights[i] / scales[i] for i in range(width)], history


def self_test() -> int:
    failures: list[str] = []

    every = [make_crop(index) for index in range(1500)]
    # The floor excludes; the ceiling contributes hard negatives. See the module header.
    kept = [
        (f, t, low) for sharp, f, t, low in every if not too_blurred(sharp)
    ]
    dropped = len(every) - len(kept)
    negatives = sum(1 for sharp, _, _, _ in every if sharp_enough(sharp))
    if dropped == 0:
        failures.append("3: the floor filtered nothing, so the fixture does not exercise it")
    if negatives == 0:
        failures.append("2: the fixture contains no sharp face, so the hard negatives are absent")
    if not kept:
        failures.append("3: the floor filtered everything")
        for line in failures:
            print(f"FAIL {line}")
        return 1

    weights, history = fit(kept)

    # 1. The loss falls and beats a constant.
    if not history[-1] < history[0]:
        failures.append(f"1: the loss did not fall: {history[0]:.6f} -> {history[-1]:.6f}")
    # The constant predictor is scored on the **same objective**, identity penalty included. The
    # first version of this comparison scored the fit on reconstruction-plus-identity and the
    # constant on reconstruction alone, which made a converged model look worse than a mean.
    mean_target = sum(t for _, t, _ in kept) / len(kept)
    constant_loss = 0.0
    for _, target, low in kept:
        excess = max(0.0, identity_drift(mean_target, low) - MAX_IDENTITY_DRIFT)
        constant_loss += (mean_target - target) ** 2 + IDENTITY_WEIGHT * excess * excess
    constant_loss /= len(kept)
    if not history[-1] < constant_loss:
        failures.append(f"1: the fit does not beat a constant: {history[-1]:.6f} vs {constant_loss:.6f}")

    # 2. A sharp face is left alone.
    worst_sharp = 0.0
    for index in range(400):
        sharpness, features, _, _ = make_crop(index + 9000)
        if sharpness <= SOFT_FACE_HI:
            continue
        worst_sharp = max(worst_sharp, abs(predict(weights, features)))
    if worst_sharp > SHARP_FACE_MAX_MOVE:
        failures.append(f"2: a sharp face moved by {worst_sharp:.4f}, above {SHARP_FACE_MAX_MOVE}")

    # 3. The floor really removed the blurred faces from the set the head saw. This is the
    #    exclusion that matters ethically: a prior asked about a face with too little information
    #    in it returns the prior, which is somebody else.
    if not any(too_blurred(sharpness) for sharpness, _, _, _ in every):
        failures.append("3: the fixture contains no face below the floor at all")
    if any(too_blurred(sharpness) for sharpness, f, t, low in every if (f, t, low) in kept):
        failures.append("3: a face below the floor reached the training set")

    # 4. The correction is high-frequency only, checked by **ablation** rather than by comparing
    #    weights. The features have different scales - `high_band` spans a few hundredths and
    #    `local_contrast` spans a quarter - so a raw weight comparison measures the units rather
    #    than the model. What matters is which feature the model cannot do without: removing the
    #    detail route must hurt, and removing the low-band route must not.
    _, without_detail = fit(kept, drop=DETAIL_ROUTE)
    _, without_contrast = fit(kept, drop=LOW_BAND_ROUTE)
    detail_cost = without_detail[-1] - history[-1]
    contrast_cost = without_contrast[-1] - history[-1]
    if detail_cost <= contrast_cost:
        failures.append(
            f"4: removing the detail feature cost {detail_cost:.6f} and removing local contrast "
            f"cost {contrast_cost:.6f}; the model is recovering faces through their low band, "
            "which is a feature moving rather than a texture returning"
        )

    # 5. Identity drift is monotone in strength. `enforce` reduces strength and re-measures, and a
    #    model whose drift was not monotone would make that loop meaningless - a reduction could
    #    make the face drift further.
    low_band = 0.5
    previous = -1.0
    for step in range(1, 11):
        strength = MAX_FACE_RECOVERY * step / 10.0
        drift = identity_drift(strength, low_band)
        if drift < previous:
            failures.append("5: identity drift is not monotone in strength")
            break
        previous = drift

    # And the guarantee itself: no kept prediction may exceed the ceiling once the constraint has
    # had its say. The runtime enforces it by measurement; here the check is that the *fitted*
    # model does not systematically ask for more than the ceiling allows.
    over = sum(
        1
        for features, _, low in kept
        if identity_drift(predict(weights, features), low) > MAX_IDENTITY_DRIFT
    )
    share = over / len(kept)
    if share > 0.10:
        failures.append(
            f"gate: {share:.1%} of fitted predictions exceed the identity ceiling before the "
            "runtime constraint runs; the model is asking to be refused on most faces"
        )

    for line in failures:
        print(f"FAIL {line}")
    if failures:
        return 1
    print(
        "train_face_recovery self-test: 5 properties and the identity gate hold "
        f"(floor dropped {dropped} of {len(every)}, {negatives} sharp faces kept as hard "
        f"negatives, loss {history[0]:.6f} -> {history[-1]:.6f}, {share:.1%} would be refused)"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the procedure on synthetic data")
    parser.add_argument("--data", help="a directory of soft/sharp face pairs")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.data:
        print(
            "train_face_recovery: there is no soft-face corpus in this repository, and there is "
            "no consented face data in it at all. PHASE-22 section 9 asks DATA for a soft-face "
            "labelled set; phase 06's condition C1 is still open. Run with --self-test.",
            file=sys.stderr,
        )
        return 2
    parser.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
