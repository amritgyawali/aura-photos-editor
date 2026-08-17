"""Section 6.1's calibration report: ECE, Brier and a reliability diagram.

The Python counterpart of `aura_explain::calibration`, named by PHASE-13 section 4. The two
implement the same arithmetic deliberately: when a photographer's own overrides eventually
produce labelled outcomes from real weddings, the number this script computes on those
outcomes and the number CI computes on the synthetic ones have to be comparable rather than
argued about.

    python ml/eval/calibration_report.py --self-test
    python ml/eval/calibration_report.py --outcomes outcomes.json
    python ml/eval/calibration_report.py --outcomes outcomes.json --diagram reliability.svg

## The gate

| Gate | Threshold | Section |
|---|---|---|
| Expected calibration error, per decision type with >= 500 samples | <= 0.05 | 0, 6.1, 10.1 |

## Three measurement choices that are easy to get wrong

**Ten bins, always.** An ECE computed over twenty bins is a different number wearing the
same name, and this one is a CI gate. The bin count is not a flag.

**The gate does not apply below 500 samples.** Section 6.1 says so, and the reason is
statistical rather than lenient: an ECE over forty outcomes has a confidence interval wider
than the threshold, so gating on it would fail builds at random and teach everybody to
ignore the gate.

**Brier is reported beside ECE and never instead of it.** They fail differently. A model
that always answers 0.5 has an excellent ECE and a poor Brier score, and a product that
gated only on ECE would ship it.

## What this build can measure, and what it cannot

Everything below runs on outcomes. AURA has no fitted calibration and no labelled outcomes
from real weddings, so the only outcomes this script has ever been run on are the synthetic
predictors in `aura_explain::fixtures` - whose error is authored, which is exactly what makes
`--self-test` able to assert that the estimator is right.

That is condition C2 of the phase 13 exit report. This script is the thing that closes it,
and it cannot close itself.

## The shape of an outcomes file

    {
      "kind": "cull",
      "source": "local",
      "outcomes": [{"raw": 0.91, "correct": true}, ...]
    }

`raw` is what the deciding code believed. `correct` is whether the decision turned out to be
right - a photographer who did not override it, or a labelled fixture with a known answer.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

# Section 6.1.
ECE_BINS = 10
ECE_GATE = 0.05
ECE_GATE_SAMPLES = 500


def bin_of(confidence: float) -> int:
    """Which of the ten bins a confidence falls in."""
    scaled = int(max(0.0, min(1.0, confidence)) * ECE_BINS)
    return min(scaled, ECE_BINS - 1)


def reliability(outcomes: list[dict]) -> dict:
    """ECE, Brier and the per-bin table, computed as `Reliability::measure` computes them."""
    if not outcomes:
        return {"ece": 0.0, "brier": 0.0, "samples": 0, "bins": []}

    sums = [[0.0, 0, 0] for _ in range(ECE_BINS)]
    brier = 0.0
    for outcome in outcomes:
        raw = max(0.0, min(1.0, float(outcome["raw"])))
        actual = 1.0 if outcome["correct"] else 0.0
        brier += (raw - actual) ** 2
        slot = sums[bin_of(raw)]
        slot[0] += raw
        slot[1] += 1 if outcome["correct"] else 0
        slot[2] += 1

    total = len(outcomes)
    ece = 0.0
    bins = []
    for sum_conf, hits, count in sums:
        if count == 0:
            continue
        mean_conf = sum_conf / count
        accuracy = hits / count
        ece += (count / total) * abs(accuracy - mean_conf)
        bins.append({"confidence": mean_conf, "accuracy": accuracy, "count": count})

    return {"ece": ece, "brier": brier / total, "samples": total, "bins": bins}


def fit_isotonic(outcomes: list[dict]) -> list[tuple[float, float]]:
    """Pool-adjacent-violators, as `Calibrator::fit_isotonic` runs it.

    Returns the knots as `(raw, calibrated)` pairs, ascending in `raw`. Monotone by
    construction, which matters more than the squared error does: a calibration that could
    map 0.9 below 0.8 would reorder the review queue.
    """
    if len(outcomes) < 2:
        return []
    ordered = sorted(outcomes, key=lambda entry: float(entry["raw"]))
    blocks: list[list[float]] = []
    for entry in ordered:
        blocks.append([1.0 if entry["correct"] else 0.0, 1.0, float(entry["raw"])])
        while len(blocks) >= 2:
            right = blocks[-1]
            left = blocks[-2]
            if left[0] / left[1] <= right[0] / right[1]:
                break
            merged = [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
            blocks.pop()
            blocks.pop()
            blocks.append(merged)
    return [(block[2] / block[1], block[0] / block[1]) for block in blocks]


def apply_isotonic(knots: list[tuple[float, float]], raw: float) -> float:
    """Linear interpolation between knots, as the Rust side applies them."""
    if not knots:
        return raw
    if raw <= knots[0][0]:
        return knots[0][1]
    if raw >= knots[-1][0]:
        return knots[-1][1]
    for left, right in zip(knots, knots[1:]):
        if left[0] <= raw <= right[0]:
            span = right[0] - left[0]
            if span <= 0:
                return right[1]
            t = (raw - left[0]) / span
            return left[1] + t * (right[1] - left[1])
    return raw


def diagram(report: dict) -> str:
    """The reliability diagram, as a standalone SVG.

    Hand-written rather than matplotlib: this script has to run in CI on three operating
    systems with nothing installed, and a plotting dependency for eleven rectangles and a
    diagonal would be the heaviest thing in `ml/`.
    """
    width, height, pad = 420, 420, 40
    plot = width - pad * 2
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}">',
        '<rect width="100%" height="100%" fill="white"/>',
        f'<line x1="{pad}" y1="{height - pad}" x2="{width - pad}" y2="{pad}" '
        'stroke="#888" stroke-dasharray="4 4"/>',
        f'<rect x="{pad}" y="{pad}" width="{plot}" height="{plot}" fill="none" stroke="#333"/>',
    ]
    bar = plot / ECE_BINS
    for entry in report["bins"]:
        index = bin_of(entry["confidence"])
        x = pad + index * bar
        top = pad + plot * (1.0 - entry["accuracy"])
        parts.append(
            f'<rect x="{x:.1f}" y="{top:.1f}" width="{bar - 2:.1f}" '
            f'height="{(height - pad) - top:.1f}" fill="#4a7dbb" opacity="0.75"/>'
        )
    parts.append(
        f'<text x="{pad}" y="{pad - 12}" font-family="sans-serif" font-size="13">'
        f'ECE {report["ece"]:.4f} · Brier {report["brier"]:.4f} · '
        f'{report["samples"]} outcomes</text>'
    )
    parts.append(
        f'<text x="{pad}" y="{height - 10}" font-family="sans-serif" font-size="11">'
        'predicted confidence, left to right; observed accuracy, bottom to top</text>'
    )
    parts.append("</svg>")
    return "\n".join(parts)


def report_lines(name: str, report: dict) -> list[str]:
    """The report a person reads, and the one CI prints."""
    lines = [
        f"{name}: {report['samples']} outcomes",
        f"  ECE   {report['ece']:.4f}  (gate {ECE_GATE} at >= {ECE_GATE_SAMPLES} samples)",
        f"  Brier {report['brier']:.4f}",
    ]
    for entry in report["bins"]:
        gap = entry["accuracy"] - entry["confidence"]
        direction = "overconfident" if gap < 0 else "underconfident"
        lines.append(
            f"    said {entry['confidence']:.2f}, was right {entry['accuracy']:.2f} "
            f"over {entry['count']:>5} - {direction if abs(gap) > 0.02 else 'calibrated'}"
        )
    return lines


def gate(report: dict) -> bool:
    """Section 6.1's gate. Thin data passes; see this module's header."""
    return report["samples"] < ECE_GATE_SAMPLES or report["ece"] <= ECE_GATE


class Lcg:
    """The generator `aura_explain::fixtures` uses, so both sides draw the same outcomes."""

    def __init__(self, seed: int) -> None:
        self.state = seed & 0xFFFFFFFFFFFFFFFF

    def next_unit(self) -> float:
        self.state = (self.state * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        bits = (self.state >> 40) & 0x007FFFFF
        packed = 0x3F800000 | bits
        # Reassemble the float exactly as `f32::from_bits` does: sign 0, exponent 127, the
        # 23 mantissa bits, then subtract one to land in 0..1.
        mantissa = packed & 0x007FFFFF
        return (1.0 + mantissa / float(1 << 23)) - 1.0


def synthetic(shape: str, count: int, seed: int) -> list[dict]:
    """The three synthetic predictors, matching `OutcomeShape`."""
    rng = Lcg(seed)
    out = []
    for _ in range(count):
        confidence = 0.5 + rng.next_unit() * 0.5
        if shape == "calibrated":
            truth = confidence
        elif shape == "overconfident":
            truth = confidence - 0.15 * confidence * (1.0 - confidence) * 4.0
        else:
            truth = confidence + 0.20 * confidence * (1.0 - confidence) * 4.0
        truth = max(0.0, min(1.0, truth))
        out.append({"raw": confidence, "correct": rng.next_unit() < truth})
    return out


def self_test() -> int:
    """Check the harness itself, on predictors whose error is known by construction."""
    failures = 0

    calibrated = reliability(synthetic("calibrated", 4000, 7))
    if calibrated["ece"] > ECE_GATE:
        print(f"  a calibrated predictor measured ECE {calibrated['ece']:.4f}, above the gate")
        failures += 1

    overconfident_raw = synthetic("overconfident", 4000, 11)
    overconfident = reliability(overconfident_raw)
    if gate(overconfident):
        print(f"  an overconfident predictor passed the gate at ECE {overconfident['ece']:.4f}")
        failures += 1

    knots = fit_isotonic(synthetic("overconfident", 4000, 3))
    if len(knots) < 2:
        print("  the isotonic fit produced no usable knots")
        failures += 1
    else:
        for left, right in zip(knots, knots[1:]):
            if right[1] < left[1] - 1e-6:
                print("  the isotonic fit is not monotone")
                failures += 1
                break

    held_out = synthetic("overconfident", 4000, 99)
    mapped = [
        {"raw": apply_isotonic(knots, entry["raw"]), "correct": entry["correct"]}
        for entry in held_out
    ]
    after = reliability(mapped)
    before = reliability(held_out)
    if after["ece"] >= before["ece"]:
        print(f"  calibration did not improve held-out ECE: {before['ece']:.4f} -> {after['ece']:.4f}")
        failures += 1
    else:
        print(f"  held-out ECE {before['ece']:.4f} -> {after['ece']:.4f} after fitting")

    if math.isnan(calibrated["brier"]) or calibrated["brier"] <= 0.0:
        print("  the Brier score is not being computed")
        failures += 1

    if not gate({"ece": 0.9, "samples": 40}):
        print("  the gate fired on forty samples, which section 6.1 exempts")
        failures += 1

    print("self-test:", "ok" if failures == 0 else f"{failures} failure(s)")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--outcomes", type=Path, help="a JSON file of labelled outcomes")
    parser.add_argument("--diagram", type=Path, help="write the reliability diagram here")
    parser.add_argument("--fit", action="store_true", help="also fit and report a calibration")
    parser.add_argument("--self-test", action="store_true", help="check the harness itself")
    args = parser.parse_args()

    if args.self_test:
        return 1 if self_test() else 0

    if not args.outcomes:
        parser.print_help()
        return 2

    payload = json.loads(args.outcomes.read_text(encoding="utf-8"))
    outcomes = payload["outcomes"]
    name = f"{payload.get('kind', 'unknown')}/{payload.get('source', 'unknown')}"
    report = reliability(outcomes)
    for line in report_lines(name, report):
        print(line)

    if args.fit:
        knots = fit_isotonic(outcomes)
        mapped = [
            {"raw": apply_isotonic(knots, entry["raw"]), "correct": entry["correct"]}
            for entry in outcomes
        ]
        after = reliability(mapped)
        print(f"  fitted {len(knots)} knots; in-sample ECE {report['ece']:.4f} -> {after['ece']:.4f}")
        print("  in-sample only: a fit measured on its own training data flatters itself.")

    if args.diagram:
        args.diagram.write_text(diagram(report), encoding="utf-8")
        print(f"  diagram: {args.diagram}")

    if not gate(report):
        print(f"FAILED: ECE {report['ece']:.4f} exceeds {ECE_GATE} at {report['samples']} samples")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
