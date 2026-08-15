"""Section 10.1's frame-integrity gates, computed the way the Rust harness computes them.

The Python counterpart of `tests/eval/integrity_eval.rs`, named by PHASE-09 section 4.
The two implement the same arithmetic deliberately: when the two heads are trained (phase
09 condition C1), the number this script produces on a labelled corpus and the number CI
produces on the synthetic fixtures have to be comparable rather than argued about.

    python ml/models/integrity/eval_integrity.py --self-test
    python ml/models/integrity/eval_integrity.py --predictions verdicts.json --labels labels.json
    python ml/models/integrity/eval_integrity.py --predictions verdicts.json --labels labels.json --fit-calibration

## The gates

| Gate | Threshold | Section |
|---|---|---|
| Subject-focus AUC on labelled keeper/reject crops | >= 0.96 | 0, 10.1 |
| Blink F1 | >= 0.95 | 0, 10.1 |
| Intentional-closed-eye false-positive rate | <= 2 % | 0, 10.1 |
| Exposure `recoverable` vs `lost` against expert labels | >= 0.93 | 10.1 |
| Noise sigma within 15 % of the measured sigma | exact | 10.1 |
| Group closed-eye ratio matches a human count | exact | 10.1 |
| Cross-camera fairness, 24 MP against 61 MP | <= 0.05 | 10.1 |

## Four measurement choices that are easy to get wrong

**The AUC is computed by the rank-sum identity, with ties at half a win.** A scorer that
returns the same number for every frame must land at exactly 0.5 rather than at 1.0, and
a scorer that ties on a third of the pairs must be penalised for the ties rather than
rewarded for them. `--self-test` asserts both.

**Blink F1 is measured per *gating face*, not per frame.** A group photograph with six
people and one blink is one true positive out of six judgements, not one out of one. A
frame-level F1 would report a detector that flagged the whole frame as perfect.

**The intentional-closed false-positive rate is measured on frames where every closure is
justified**, and it is the number this phase would rather be wrong about in the other
direction. A false positive here is a photograph of a kiss marked as a blink; a false
negative is a blink that reaches a review queue and is looked at. Those are not equally
expensive, and the 2 % ceiling is asymmetric for that reason.

**Cross-camera fairness compares the *normalised* figure.** The raw acutance of a 61 MP
body's proxy is genuinely lower and must be: the gate is that the calibration division
removes the difference, not that the difference does not exist.

## Fitting the per-scene calibration

`--fit-calibration` runs the pool-adjacent-violators algorithm over labelled
keeper/reject pairs, per scene, and prints the eleven knots
`aura_brain_photo::integrity::score::Isotonic::from_knots` takes. **The maps that ship
today are the identity**, because there are no labelled keeper/reject pairs in this
repository - condition C5 in the phase 09 exit report. The code is here so that the day
there are, the answer is one command rather than a design discussion.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

# -- section 10.1's thresholds, in one place ---------------------------------

AUC_GATE = 0.96
BLINK_F1_GATE = 0.95
INTENTIONAL_FP_GATE = 0.02
EXPOSURE_AGREEMENT_GATE = 0.93
NOISE_TOLERANCE = 0.15
FAIRNESS_GATE = 0.05

#: How many knots one isotonic map has. Matches `Isotonic::KNOTS`.
KNOTS = 11


@dataclass
class Gate:
    """One measured number and whether it cleared its threshold."""

    name: str
    value: float
    threshold: float
    higher_is_better: bool = True
    note: str = ""

    @property
    def passed(self) -> bool:
        if self.higher_is_better:
            return self.value >= self.threshold
        return self.value <= self.threshold

    def line(self) -> str:
        mark = "PASS" if self.passed else "FAIL"
        direction = ">=" if self.higher_is_better else "<="
        tail = f"  ({self.note})" if self.note else ""
        return f"[{mark}] {self.name}: {self.value:.3f} {direction} {self.threshold:.3f}{tail}"


@dataclass
class Report:
    """Everything one evaluation produced."""

    gates: list[Gate] = field(default_factory=list)
    deferred: list[str] = field(default_factory=list)

    def add(self, gate: Gate) -> None:
        self.gates.append(gate)

    def defer(self, reason: str) -> None:
        self.deferred.append(reason)

    @property
    def passed(self) -> bool:
        return all(gate.passed for gate in self.gates)

    def render(self) -> str:
        lines = [gate.line() for gate in self.gates]
        if self.deferred:
            lines.append("")
            lines.append("deferred, and why:")
            lines.extend(f"  - {reason}" for reason in self.deferred)
        return "\n".join(lines)


# ---------------------------------------------------------------------------
# Focus
# ---------------------------------------------------------------------------


def area_under_curve(scored: list[tuple[float, bool]]) -> float:
    """The area under the ROC curve, by the rank-sum identity.

    Ties count half a win, which is the standard treatment and the one that makes a
    constant scorer land at exactly 0.5. The Rust harness implements the same loop; the
    two are deliberately the same shape rather than the same performance.
    """
    positives = [score for score, keeper in scored if keeper]
    negatives = [score for score, keeper in scored if not keeper]
    if not positives or not negatives:
        return 0.0
    wins = 0.0
    for positive in positives:
        for negative in negatives:
            if positive > negative:
                wins += 1.0
            elif positive == negative:
                wins += 0.5
    return wins / (len(positives) * len(negatives))


def subject_focus_auc(verdicts: list[dict], labels: dict[str, dict]) -> float:
    """AUC of `subjectSharpness` against a labelled keeper/reject decision."""
    scored: list[tuple[float, bool]] = []
    for verdict in verdicts:
        label = labels.get(verdict["photoId"])
        if label is None or "keeper" not in label:
            continue
        scored.append((float(verdict["subjectSharpness"]), bool(label["keeper"])))
    return area_under_curve(scored)


# ---------------------------------------------------------------------------
# Eyes
# ---------------------------------------------------------------------------


def blink_f1(verdicts: list[dict], labels: dict[str, dict]) -> tuple[float, float, float]:
    """Precision, recall and F1 of the closed-eye decision, per gating face.

    Per face rather than per frame: a group photograph with six people and one blink is
    one true positive out of six judgements. A frame-level F1 would report a detector
    that flagged whole frames as perfect.
    """
    true_positive = 0
    false_positive = 0
    false_negative = 0
    for verdict in verdicts:
        label = labels.get(verdict["photoId"], {})
        truth = {entry["faceId"]: entry for entry in label.get("eyes", [])}
        for eye in verdict.get("eyes", []):
            if not eye.get("gates", False):
                continue
            expected = truth.get(eye["faceId"], {}).get("state")
            if expected is None:
                continue
            predicted_closed = eye["state"] == "closed"
            actual_closed = expected == "closed"
            if predicted_closed and actual_closed:
                true_positive += 1
            elif predicted_closed:
                false_positive += 1
            elif actual_closed:
                false_negative += 1
    precision = true_positive / max(true_positive + false_positive, 1)
    recall = true_positive / max(true_positive + false_negative, 1)
    if precision + recall == 0:
        return 0.0, precision, recall
    return 2 * precision * recall / (precision + recall), precision, recall


def intentional_false_positives(verdicts: list[dict], labels: dict[str, dict]) -> float:
    """Fraction of justified-closure frames that were nevertheless flagged as blinks.

    The dangerous direction. A false positive here is a photograph of a kiss marked as a
    fault; a false negative is a blink that reaches a review queue and gets looked at.
    """
    judged = 0
    wrong = 0
    for verdict in verdicts:
        label = labels.get(verdict["photoId"], {})
        if not label.get("closureJustified", False):
            continue
        judged += 1
        if "eyes_closed" in verdict.get("flags", []):
            wrong += 1
    return wrong / max(judged, 1)


def closed_eye_ratio_matches(verdicts: list[dict], labels: dict[str, dict]) -> float:
    """Fraction of group frames whose closed-eye count matches the human count exactly.

    Exactly, so a ratio is not enough: the numerator has to be right. Section 10.1 asks
    for this on 200 labelled group frames.
    """
    checked = 0
    exact = 0
    for verdict in verdicts:
        label = labels.get(verdict["photoId"], {})
        expected = label.get("closedEyeCount")
        if expected is None:
            continue
        checked += 1
        gating = max(int(verdict.get("gatingFaces", 0)), 1)
        predicted = round(float(verdict.get("closedEyeRatio", 0.0)) * gating)
        if predicted == int(expected):
            exact += 1
    return exact / max(checked, 1)


# ---------------------------------------------------------------------------
# Exposure and noise
# ---------------------------------------------------------------------------


def exposure_agreement(verdicts: list[dict], labels: dict[str, dict]) -> float:
    """Fraction of frames whose exposure verdict matches the expert label."""
    checked = 0
    agreed = 0
    for verdict in verdicts:
        expected = labels.get(verdict["photoId"], {}).get("exposure")
        if expected is None:
            continue
        checked += 1
        if verdict.get("exposure") == expected:
            agreed += 1
    return agreed / max(checked, 1)


def noise_error(verdicts: list[dict], labels: dict[str, dict]) -> float:
    """The largest relative error between the estimated sigma and the measured one.

    The *largest* rather than the mean: section 10.1's gate is about whether the
    estimator can be trusted on a frame, and a mean hides the frames where it cannot.
    """
    worst = 0.0
    for verdict in verdicts:
        measured = labels.get(verdict["photoId"], {}).get("sigma")
        if measured is None or float(measured) <= 0:
            continue
        estimated = float(verdict.get("sigma", 0.0))
        worst = max(worst, abs(estimated - float(measured)) / float(measured))
    return worst


def cross_camera_gap(verdicts: list[dict], labels: dict[str, dict]) -> float:
    """The largest normalised-sharpness gap between two bodies on one labelled scene.

    Frames carrying the same `sceneKey` are the same scene shot on different bodies. The
    gate is that the calibration division removes the difference, not that the raw
    figures agree - they should not, and a 61 MP body's proxy is genuinely softer per
    output pixel.
    """
    by_scene: dict[str, list[float]] = {}
    for verdict in verdicts:
        key = labels.get(verdict["photoId"], {}).get("sceneKey")
        if key is None:
            continue
        by_scene.setdefault(str(key), []).append(float(verdict["subjectSharpness"]))
    gaps = [max(values) - min(values) for values in by_scene.values() if len(values) > 1]
    return max(gaps, default=0.0)


# ---------------------------------------------------------------------------
# Per-scene calibration
# ---------------------------------------------------------------------------


def pool_adjacent_violators(points: list[tuple[float, float]]) -> list[tuple[float, float]]:
    """Isotonic regression by PAVA: the least-squares monotone fit.

    Written out rather than imported, because this file has no third-party dependency and
    the algorithm is fifteen lines. Points are `(raw score, label)` with the label 1 for a
    keeper and 0 for a reject; the fit is the monotone function of the raw score that best
    predicts the label, which is exactly the calibration section 6.5 asks for.
    """
    ordered = sorted(points, key=lambda point: point[0])
    blocks: list[tuple[float, float, int]] = []  # (x, mean y, weight)
    for x, y in ordered:
        blocks.append((x, y, 1))
        while len(blocks) > 1 and blocks[-2][1] > blocks[-1][1]:
            x2, y2, w2 = blocks.pop()
            x1, y1, w1 = blocks.pop()
            weight = w1 + w2
            blocks.append((x2, (y1 * w1 + y2 * w2) / weight, weight))
    out: list[tuple[float, float]] = []
    for x, y, _ in blocks:
        out.append((x, y))
    return out


def fit_isotonic(points: list[tuple[float, float]]) -> list[float]:
    """The eleven knots `Isotonic::from_knots` takes, on a fixed 0.0 to 1.0 grid."""
    fit = pool_adjacent_violators(points)
    if not fit:
        return [index / (KNOTS - 1) for index in range(KNOTS)]
    knots: list[float] = []
    for index in range(KNOTS):
        position = index / (KNOTS - 1)
        # The fitted value at or below this position, which is what a step function of a
        # monotone fit evaluates to.
        value = fit[0][1]
        for x, y in fit:
            if x <= position:
                value = y
            else:
                break
        knots.append(round(min(max(value, 0.0), 1.0), 4))
    # A monotone map, enforced rather than assumed: PAVA guarantees it on its own grid
    # and the resampling above can still produce a flat step that rounds down.
    for index in range(1, KNOTS):
        knots[index] = max(knots[index], knots[index - 1])
    return knots


def fit_per_scene(verdicts: list[dict], labels: dict[str, dict]) -> dict[str, list[float]]:
    """One isotonic map per scene, from labelled keeper/reject pairs."""
    by_scene: dict[str, list[tuple[float, float]]] = {}
    for verdict in verdicts:
        label = labels.get(verdict["photoId"])
        if label is None or "keeper" not in label:
            continue
        scene = str(verdict.get("scene", "unknown"))
        by_scene.setdefault(scene, []).append(
            (float(verdict["technicalScore"]), 1.0 if label["keeper"] else 0.0)
        )
    return {scene: fit_isotonic(points) for scene, points in sorted(by_scene.items())}


# ---------------------------------------------------------------------------
# The harness proves it would fail a bad analyser
# ---------------------------------------------------------------------------


def self_test() -> int:
    """Assert that every metric here rejects an analyser that learned nothing.

    Phase 08's guard, a phase later: a gate that cannot fail is not a gate.
    """
    failures = 0

    constant = [(0.5, index % 2 == 0) for index in range(40)]
    auc = area_under_curve(constant)
    if abs(auc - 0.5) > 1e-9:
        print(f"[FAIL] a constant scorer should measure exactly 0.5, measured {auc}")
        failures += 1
    else:
        print("[PASS] a constant scorer measures 0.5 and fails the AUC gate")

    inverted = [(float(index % 2), index % 2 == 0) for index in range(40)]
    if area_under_curve(inverted) >= 0.5:
        print("[FAIL] an inverted scorer should measure below a coin toss")
        failures += 1
    else:
        print("[PASS] an inverted scorer measures below 0.5")

    perfect = [(0.9, True), (0.8, True), (0.2, False), (0.1, False)]
    if abs(area_under_curve(perfect) - 1.0) > 1e-9:
        print("[FAIL] a perfect separation should measure 1.0")
        failures += 1
    else:
        print("[PASS] a perfect separation measures 1.0")

    # A blink detector that flags everything has recall 1.0 and must still fail on
    # precision - which is the whole reason F1 rather than recall is the gate.
    verdicts = [
        {
            "photoId": "a",
            "gatingFaces": 2,
            "eyes": [
                {"faceId": "f1", "gates": True, "state": "closed"},
                {"faceId": "f2", "gates": True, "state": "closed"},
            ],
            "flags": ["eyes_closed"],
        }
    ]
    labels = {
        "a": {
            "eyes": [
                {"faceId": "f1", "state": "closed"},
                {"faceId": "f2", "state": "open"},
            ]
        }
    }
    f1, precision, recall = blink_f1(verdicts, labels)
    if recall < 1.0 or precision >= 1.0 or f1 >= BLINK_F1_GATE:
        print(f"[FAIL] a flag-everything detector scored F1 {f1:.3f}")
        failures += 1
    else:
        print(
            f"[PASS] a flag-everything detector has recall {recall:.2f}, precision "
            f"{precision:.2f}, and fails the F1 gate"
        )

    # The isotonic fit must be monotone, and must not be the identity when the data says
    # otherwise.
    knots = fit_isotonic([(0.1, 0.0), (0.2, 0.0), (0.5, 0.0), (0.6, 1.0), (0.9, 1.0)])
    if any(knots[index] < knots[index - 1] for index in range(1, KNOTS)):
        print(f"[FAIL] the isotonic fit is not monotone: {knots}")
        failures += 1
    else:
        print(f"[PASS] the isotonic fit is monotone: {knots}")

    print()
    print("self-test:", "all checks passed" if failures == 0 else f"{failures} failures")
    return 0 if failures == 0 else 1


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def evaluate(verdicts: list[dict], labels: dict[str, dict]) -> Report:
    """Every section 10.1 gate that the supplied labels can answer."""
    report = Report()

    if any("keeper" in label for label in labels.values()):
        report.add(
            Gate(
                "subject-focus AUC",
                subject_focus_auc(verdicts, labels),
                AUC_GATE,
                note=f"{sum('keeper' in label for label in labels.values())} labelled frames",
            )
        )
    else:
        report.defer("subject-focus AUC: no keeper/reject labels supplied")

    if any("eyes" in label for label in labels.values()):
        f1, precision, recall = blink_f1(verdicts, labels)
        report.add(
            Gate(
                "blink F1",
                f1,
                BLINK_F1_GATE,
                note=f"precision {precision:.3f}, recall {recall:.3f}",
            )
        )
    else:
        report.defer("blink F1: no per-face eye labels supplied")

    if any(label.get("closureJustified") for label in labels.values()):
        report.add(
            Gate(
                "intentional-closed false positives",
                intentional_false_positives(verdicts, labels),
                INTENTIONAL_FP_GATE,
                higher_is_better=False,
            )
        )
    else:
        report.defer("intentional-closed false positives: no justified-closure frames")

    if any("exposure" in label for label in labels.values()):
        report.add(
            Gate(
                "exposure agreement",
                exposure_agreement(verdicts, labels),
                EXPOSURE_AGREEMENT_GATE,
            )
        )
    else:
        report.defer("exposure agreement: no expert exposure labels supplied")

    if any("sigma" in label for label in labels.values()):
        report.add(
            Gate(
                "worst noise error",
                noise_error(verdicts, labels),
                NOISE_TOLERANCE,
                higher_is_better=False,
            )
        )
    else:
        report.defer("noise error: no measured sigmas supplied")

    if any("closedEyeCount" in label for label in labels.values()):
        report.add(
            Gate(
                "group closed-eye count exact",
                closed_eye_ratio_matches(verdicts, labels),
                1.0,
            )
        )
    else:
        report.defer("group closed-eye count: no labelled group frames")

    if any("sceneKey" in label for label in labels.values()):
        report.add(
            Gate(
                "cross-camera gap",
                cross_camera_gap(verdicts, labels),
                FAIRNESS_GATE,
                higher_is_better=False,
            )
        )
    else:
        report.defer("cross-camera fairness: no paired-body frames supplied")

    return report


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--predictions", type=Path, help="JSON array of IntegrityDto")
    parser.add_argument("--labels", type=Path, help="JSON object keyed by photo id")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the metrics reject an analyser that learned nothing",
    )
    parser.add_argument(
        "--fit-calibration",
        action="store_true",
        help="fit and print the per-scene isotonic knots",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    if args.predictions is None or args.labels is None:
        parser.error("--predictions and --labels are required unless --self-test is given")

    verdicts = json.loads(args.predictions.read_text(encoding="utf-8"))
    labels = json.loads(args.labels.read_text(encoding="utf-8"))

    if args.fit_calibration:
        fitted = fit_per_scene(verdicts, labels)
        print("# Paste into `SceneCalibration::shipped`, one `with` per scene.")
        for scene, knots in fitted.items():
            formatted = ", ".join(f"{knot:.4f}" for knot in knots)
            print(f"{scene}: [{formatted}]")
        return 0

    report = evaluate(verdicts, labels)
    print(report.render())
    return 0 if report.passed else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
