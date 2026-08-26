"""Section 10.1's expert-crop gate, computed the way the Rust harness computes it.

The Python counterpart of `tests/eval/geometry_eval.rs`, named by PHASE-23 section 9's MLL
task: "define the crop objective and improvement margin; evaluate against expert crops". The
two implement the same arithmetic deliberately: when section 9's DATA task produces expert
crop labels on two thousand frames, the number this script computes on those labels and the
number CI computes on the synthetic ones have to be comparable rather than argued about.

    python ml/eval/crop_agreement.py --self-test
    python ml/eval/crop_agreement.py --labels tests/fixtures/labels/crops.json --plans plans.json

## The gates

| Gate | Threshold | Section |
|---|---|---|
| Straightening within 0.3 deg of expert | >= 90 % of labelled frames | 0, 10.1 |
| Auto-crops that cut a detected face or primary hands | 0 | 10.1 |
| Frames keeping their original framing | >= 70 % | 10.1 |
| Crops below the resolution floor | 0 | 10.1 |

## Three measurement choices that are easy to get wrong

**Crop agreement is intersection-over-union against the expert's rectangle, and it is
reported rather than gated.** There is no threshold in section 10.1 for how closely an
automatic crop must match an expert's, and there should not be: two editors given the same
frame produce rectangles that overlap at about 0.8, so a gate at any value above that would
be measuring which editor labelled the set. What *is* gated is the safety - zero cut faces -
and the conservatism - most frames untouched. Those are the two claims the product actually
makes.

**A frame the expert did not crop counts, and counts as agreement when AURA did not crop it
either.** Excluding them would measure the crops AURA chose to make against the crops an
expert chose to make on a different subset, which is not a comparison. It would also make the
seventy-per-cent conservatism target unmeasurable, since the frames that satisfy it are
exactly the ones being excluded.

**The straightening gate is measured on frames the expert levelled OR AURA levelled**, not on
the intersection. A frame AURA turned by two degrees and the expert left alone is a two-degree
error, and scoring it as "not labelled" would hide the failure this gate exists to catch.

## What this script cannot do here

There are no expert crop labels in this repository. `--self-test` runs the whole computation
against an authored answer, which proves the arithmetic and says nothing about a photographer.
That is condition C1 in `docs/progress/PHASE-23-EXIT.md`.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

STRAIGHTEN_TOLERANCE_DEG = 0.3
STRAIGHTEN_RATE = 0.90
KEPT_ORIGINAL_RATE = 0.70
RESOLUTION_FLOOR = 0.60


@dataclass
class Frame:
    """One photograph: what the expert did, what AURA did, and what must stay in frame."""

    photo_id: str
    expert_rotate_deg: float | None = None
    aura_rotate_deg: float = 0.0
    # [x, y, w, h] normalised, or None when the frame was delivered as shot.
    expert_crop: list[float] | None = None
    aura_crop: list[float] | None = None
    # Every region that must survive, as [x, y, w, h].
    faces: list[list[float]] = field(default_factory=list)
    hands: list[list[float]] = field(default_factory=list)
    frame_aspect: float = 1.5


@dataclass
class Report:
    frames: int = 0
    straighten_labelled: int = 0
    straighten_within: int = 0
    kept_original: int = 0
    cut_faces: int = 0
    cut_hands: int = 0
    below_floor: int = 0
    iou_sum: float = 0.0
    iou_count: int = 0

    @property
    def straighten_rate(self) -> float:
        if self.straighten_labelled == 0:
            return 1.0
        return self.straighten_within / self.straighten_labelled

    @property
    def kept_rate(self) -> float:
        if self.frames == 0:
            return 1.0
        return self.kept_original / self.frames

    @property
    def mean_iou(self) -> float:
        if self.iou_count == 0:
            return 1.0
        return self.iou_sum / self.iou_count

    def passes(self) -> bool:
        return (
            self.straighten_rate >= STRAIGHTEN_RATE
            and self.cut_faces == 0
            and self.cut_hands == 0
            and self.below_floor == 0
            and self.kept_rate >= KEPT_ORIGINAL_RATE
        )


def _iou(a: list[float], b: list[float]) -> float:
    """Intersection over union of two [x, y, w, h] rectangles."""
    ax0, ay0, aw, ah = a
    bx0, by0, bw, bh = b
    ax1, ay1 = ax0 + aw, ay0 + ah
    bx1, by1 = bx0 + bw, by0 + bh
    ix = min(ax1, bx1) - max(ax0, bx0)
    iy = min(ay1, by1) - max(ay0, by0)
    if ix <= 0 or iy <= 0:
        return 0.0
    inter = ix * iy
    union = aw * ah + bw * bh - inter
    return inter / union if union > 0 else 0.0


def _inside(region: list[float], crop: list[float]) -> bool:
    """True when `region` is entirely within `crop`. No margin: this is the audit, not the filter."""
    rx0, ry0, rw, rh = region
    cx0, cy0, cw, ch = crop
    return (
        rx0 >= cx0 - 1e-6
        and ry0 >= cy0 - 1e-6
        and rx0 + rw <= cx0 + cw + 1e-6
        and ry0 + rh <= cy0 + ch + 1e-6
    )


def _long_edge_fraction(crop: list[float], frame_aspect: float) -> float:
    """What fraction of the *original* long edge this crop keeps.

    Against the frame as shot rather than against the corrected frame, which is the same
    choice `CropVariant::long_edge_fraction` makes and for the same reason: measuring against
    the corrected frame makes the floor depend on which lens was in somebody's hand.
    """
    _, _, w, h = crop
    long_edge = max(w * frame_aspect, h)
    frame_long = max(frame_aspect, 1.0)
    return long_edge / frame_long if frame_long > 0 else 0.0


def evaluate(frames: list[Frame]) -> Report:
    report = Report(frames=len(frames))
    full = [0.0, 0.0, 1.0, 1.0]
    for frame in frames:
        delivered = frame.aura_crop or full

        # Straightening. Labelled when either side turned the frame - see the module header.
        expert_angle = frame.expert_rotate_deg
        if expert_angle is not None or abs(frame.aura_rotate_deg) > 1e-6:
            report.straighten_labelled += 1
            wanted = expert_angle if expert_angle is not None else 0.0
            if abs(frame.aura_rotate_deg - wanted) <= STRAIGHTEN_TOLERANCE_DEG:
                report.straighten_within += 1

        # Conservatism.
        if frame.aura_crop is None or _iou(delivered, full) > 0.999:
            report.kept_original += 1

        # Safety. Zero tolerance, and hands are counted separately because on this build
        # there are none and a combined number would read as a passed check.
        for face in frame.faces:
            if not _inside(face, delivered):
                report.cut_faces += 1
        for pair in frame.hands:
            if not _inside(pair, delivered):
                report.cut_hands += 1

        # Resolution.
        if _long_edge_fraction(delivered, frame.frame_aspect) < RESOLUTION_FLOOR - 1e-3:
            report.below_floor += 1

        # Agreement, reported and never gated.
        if frame.expert_crop is not None:
            report.iou_sum += _iou(delivered, frame.expert_crop)
            report.iou_count += 1
    return report


def _print(report: Report) -> None:
    print(f"frames                       {report.frames}")
    print(
        f"straightening within {STRAIGHTEN_TOLERANCE_DEG} deg  "
        f"{report.straighten_within}/{report.straighten_labelled} "
        f"({report.straighten_rate:.2%}, gate {STRAIGHTEN_RATE:.0%})"
    )
    print(
        f"kept original framing        {report.kept_original}/{report.frames} "
        f"({report.kept_rate:.2%}, gate {KEPT_ORIGINAL_RATE:.0%})"
    )
    print(f"crops that cut a face        {report.cut_faces} (gate 0)")
    print(f"crops that cut primary hands {report.cut_hands} (gate 0)")
    print(f"crops below the floor        {report.below_floor} (gate 0)")
    print(
        f"mean IoU against the expert  {report.mean_iou:.3f} over {report.iou_count} "
        "cropped frames - REPORTED, NEVER GATED (see the module header)"
    )


def _load(path: Path) -> list[Frame]:
    raw = json.loads(path.read_text())
    return [Frame(**item) for item in raw]


def _self_test() -> int:
    """Run the whole computation against an authored answer.

    Six frames, and every gate is exercised in both directions. Nothing here is a photograph.
    """
    frames = [
        # Levelled correctly, delivered as shot.
        Frame("a", expert_rotate_deg=-2.6, aura_rotate_deg=-2.5, faces=[[0.4, 0.3, 0.1, 0.14]]),
        # Left alone by both.
        Frame("b", faces=[[0.4, 0.3, 0.1, 0.14]]),
        # Cropped, agreeing closely with the expert, keeping the face.
        Frame(
            "c",
            expert_crop=[0.05, 0.04, 0.85, 0.86],
            aura_crop=[0.06, 0.05, 0.84, 0.85],
            faces=[[0.4, 0.3, 0.1, 0.14]],
        ),
        # Left alone by both, with hands present.
        Frame("d", hands=[[0.45, 0.6, 0.09, 0.07]]),
        Frame("e", faces=[[0.2, 0.2, 0.1, 0.14]]),
        Frame("f", expert_rotate_deg=0.0, aura_rotate_deg=0.0),
    ]
    report = evaluate(frames)
    _print(report)
    ok = True

    if not report.passes():
        print("FAIL: a clean set did not pass", file=sys.stderr)
        ok = False
    if report.kept_original != 5:
        print(f"FAIL: expected 5 untouched frames, got {report.kept_original}", file=sys.stderr)
        ok = False

    # A cut face must be caught, and it must fail the gate on its own.
    print()
    print("-- a crop that cuts a face --")
    cut = list(frames)
    cut[2] = Frame(
        "c",
        expert_crop=[0.05, 0.04, 0.85, 0.86],
        aura_crop=[0.5, 0.05, 0.45, 0.85],
        faces=[[0.4, 0.3, 0.1, 0.14]],
    )
    bad = evaluate(cut)
    _print(bad)
    if bad.cut_faces != 1 or bad.passes():
        print("FAIL: a cut face was not caught", file=sys.stderr)
        ok = False

    # A frame turned two degrees the expert left alone must fail the straightening gate,
    # rather than being excluded as unlabelled.
    print()
    print("-- an unwanted rotation --")
    turned = [Frame(f"t{i}", aura_rotate_deg=2.0) for i in range(10)]
    over = evaluate(turned)
    _print(over)
    if over.straighten_labelled != 10 or over.straighten_within != 0:
        print("FAIL: an unwanted rotation was excluded rather than scored", file=sys.stderr)
        ok = False

    # A crop below the floor must be caught.
    print()
    print("-- a crop below the resolution floor --")
    small = evaluate([Frame("s", aura_crop=[0.25, 0.25, 0.5, 0.5])])
    if small.below_floor != 1 or small.passes():
        print("FAIL: a crop below the floor was not caught", file=sys.stderr)
        ok = False

    print()
    if ok:
        print("self-test: OK")
        print()
        print(
            "There are no expert crop labels in this repository. This proves the arithmetic\n"
            "and says nothing about whether a photographer would agree with a crop - which is\n"
            "condition C1 in docs/progress/PHASE-23-EXIT.md."
        )
        return 0
    print("self-test: FAILED", file=sys.stderr)
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run against an authored answer")
    parser.add_argument("--labels", type=Path, help="expert crop labels, as JSON")
    parser.add_argument("--plans", type=Path, help="AURA's plans, as JSON")
    args = parser.parse_args()

    if args.self_test:
        return _self_test()
    if not args.labels:
        parser.error("--labels is required without --self-test")

    frames = _load(args.labels)
    if args.plans:
        plans = {item["photo_id"]: item for item in json.loads(args.plans.read_text())}
        for frame in frames:
            plan = plans.get(frame.photo_id)
            if plan is None:
                continue
            frame.aura_rotate_deg = plan.get("rotate_deg", 0.0)
            frame.aura_crop = plan.get("crop")
    report = evaluate(frames)
    _print(report)
    return 0 if report.passes() else 1


if __name__ == "__main__":
    raise SystemExit(main())
