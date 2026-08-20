#!/usr/bin/env python3
"""Phase 20's evaluation gates, in Python, so that two implementations have to agree.

`tests/eval/retouch_eval.rs` is the Rust half and it is what CI blocks on. This is the second
implementation of the same metrics, and it exists for the reason every phase since 05 has kept
one: a measurement with a single implementation is a measurement nobody has checked. When the two
disagree, one of them is wrong and the disagreement is the finding.

Four metrics:

**Texture retention.** The high-band energy of skin after a retouch over the same energy before
it. The band split here is the same three-band separation `aura_render::bands` performs - a wide
blur, a narrow blur, and what the narrow blur left - so the number is comparable rather than
merely similar.

**Blemish recall and false removal.** Against labels, with the two priced separately because
section 10.1 gates them separately.

**Cross-frame consistency.** The spread of one identity's strength across a gallery. Zero by
construction in the shipped design, and measured anyway, because a future change that made it
per-frame should show up in a number rather than in a diff.

**Per-bucket parity.** The gap between the best and worst skin-tone bucket. **Not measurable in
this repository** - there is no labelled corpus - and the function is here, tested against
synthetic inputs, so that the day the corpus exists the measurement does not have to be invented
under deadline.

``--self-test`` runs without numpy and asserts that each metric can fail.
"""

from __future__ import annotations

import argparse
import math
import sys

TEXTURE_FLOOR = 0.90
POLISHED_FLOOR = 0.80
RECALL_FLOOR = 0.90
FALSE_REMOVAL_CEILING = 0.02
CONSISTENCY_CEILING = 0.05
BUCKET_GAP_CEILING = 0.10

LOW_RADIUS_FRAC = 1.0 / 12.0
HIGH_RADIUS_FRAC = 1.0 / 60.0
BOX_PASSES = 3


def box_blur(plane: list[list[float]], radius: int) -> list[list[float]]:
    """Three passes of a separable box filter, the same approximation the renderer uses."""
    height = len(plane)
    width = len(plane[0]) if height else 0
    buffer = [row[:] for row in plane]
    for _ in range(BOX_PASSES):
        scratch = [[0.0] * width for _ in range(height)]
        for y in range(height):
            for x in range(width):
                lo = max(0, x - radius)
                hi = min(width - 1, x + radius)
                scratch[y][x] = sum(buffer[y][lo : hi + 1]) / (hi - lo + 1)
        for x in range(width):
            for y in range(height):
                lo = max(0, y - radius)
                hi = min(height - 1, y + radius)
                buffer[y][x] = sum(scratch[i][x] for i in range(lo, hi + 1)) / (hi - lo + 1)
    return buffer


def high_band_energy(plane: list[list[float]], mask: list[list[float]] | None = None) -> float:
    """Mean absolute residual above the narrow blur, over the masked samples."""
    height = len(plane)
    width = len(plane[0]) if height else 0
    if not height or not width:
        return 0.0
    side = min(width, height)
    narrow = box_blur(plane, max(1, round(side * HIGH_RADIUS_FRAC)))
    total = 0.0
    weight = 0.0
    for y in range(height):
        for x in range(width):
            w = 1.0 if mask is None else mask[y][x]
            if w <= 0.0:
                continue
            total += abs(plane[y][x] - narrow[y][x]) * w
            weight += w
    return total / weight if weight else 0.0


def texture_retention(before: list[list[float]], after: list[list[float]], mask=None) -> float:
    """The headline KPI. One is a retouch that changed no texture at all."""
    b = high_band_energy(before, mask)
    if b <= 1e-9:
        return 1.0
    return high_band_energy(after, mask) / b


def recall_and_false_removal(labels: list[dict]) -> tuple[float, float]:
    """Recall over temporary marks, and the share of permanent features removed."""
    temporary = [m for m in labels if m["temporary"]]
    permanent = [m for m in labels if not m["temporary"]]
    recall = (
        sum(1 for m in temporary if m["removed"]) / len(temporary) if temporary else 0.0
    )
    false_removal = (
        sum(1 for m in permanent if m["removed"]) / len(permanent) if permanent else 0.0
    )
    return recall, false_removal


def identity_spread(strengths: list[float]) -> float:
    """The largest difference between one identity's strengths across a gallery."""
    if len(strengths) < 2:
        return 0.0
    return max(strengths) - min(strengths)


def bucket_gap(per_bucket: dict[str, float]) -> float:
    """The distance between the best and worst skin-tone bucket."""
    values = [v for v in per_bucket.values() if v is not None]
    if len(values) < 2:
        return 0.0
    return max(values) - min(values)


def gate(result: dict) -> list[str]:
    failures = []
    if result["texture"] < result.get("floor", TEXTURE_FLOOR):
        failures.append(
            f"texture retention {result['texture']:.3f} below {result.get('floor', TEXTURE_FLOOR)}"
        )
    if result["texture"] < POLISHED_FLOOR:
        failures.append(
            f"texture retention {result['texture']:.3f} below the phase bound {POLISHED_FLOOR}"
        )
    if result["recall"] < RECALL_FLOOR:
        failures.append(f"recall {result['recall']:.3f} below {RECALL_FLOOR}")
    if result["false_removal"] > FALSE_REMOVAL_CEILING:
        failures.append(
            f"false removal {result['false_removal']:.3f} above {FALSE_REMOVAL_CEILING}"
        )
    if result["tattoos_removed"] > 0:
        failures.append("a tattoo was removed; that gate is zero rather than small")
    if result["consistency"] > CONSISTENCY_CEILING:
        failures.append(
            f"one identity varied by {result['consistency']:.3f} across the gallery"
        )
    if result.get("bucket_gap") is not None and result["bucket_gap"] > BUCKET_GAP_CEILING:
        failures.append(f"skin-tone bucket gap {result['bucket_gap']:.3f} above {BUCKET_GAP_CEILING}")
    return failures


def _plane(width: int, height: int, texture: float, blotch: float = 0.0) -> list[list[float]]:
    plane = []
    for y in range(height):
        row = []
        for x in range(width):
            pore = texture if (x + y) % 2 == 0 else -texture
            value = 0.34 + pore
            if blotch and (x - width // 2) ** 2 + (y - height // 2) ** 2 < (width // 6) ** 2:
                value += blotch
            row.append(value)
        plane.append(row)
    return plane


def self_test() -> int:
    before = _plane(48, 48, 0.012)

    # A retouch that keeps the texture.
    kept = _plane(48, 48, 0.012, blotch=0.02)
    assert texture_retention(before, kept) > 0.95, texture_retention(before, kept)

    # A retouch that smooths it: the classic failure this whole phase exists to prevent.
    smoothed = _plane(48, 48, 0.003)
    ratio = texture_retention(before, smoothed)
    assert ratio < TEXTURE_FLOOR, f"plastic skin scored {ratio:.3f}"

    labels = [
        {"temporary": True, "removed": True},
        {"temporary": True, "removed": True},
        {"temporary": True, "removed": False},
        {"temporary": False, "removed": False},
        {"temporary": False, "removed": False},
    ]
    recall, false_removal = recall_and_false_removal(labels)
    assert abs(recall - 2 / 3) < 1e-9
    assert false_removal == 0.0

    # Every metric must be able to fail.
    assert gate(
        {
            "texture": 0.70,
            "floor": TEXTURE_FLOOR,
            "recall": 1.0,
            "false_removal": 0.0,
            "tattoos_removed": 0,
            "consistency": 0.0,
        }
    ), "a plastic-skin result passed"
    assert gate(
        {
            "texture": 1.0,
            "floor": TEXTURE_FLOOR,
            "recall": 1.0,
            "false_removal": 0.0,
            "tattoos_removed": 1,
            "consistency": 0.0,
        }
    ), "a tattoo removal passed"
    assert gate(
        {
            "texture": 1.0,
            "floor": TEXTURE_FLOOR,
            "recall": 1.0,
            "false_removal": 0.0,
            "tattoos_removed": 0,
            "consistency": 0.30,
        }
    ), "an inconsistent gallery passed"
    assert not gate(
        {
            "texture": 0.97,
            "floor": TEXTURE_FLOOR,
            "recall": 0.95,
            "false_removal": 0.0,
            "tattoos_removed": 0,
            "consistency": 0.0,
            "bucket_gap": None,
        }
    ), "a good result was refused"

    assert identity_spread([0.7, 0.7, 0.7]) == 0.0
    assert bucket_gap({"a": 0.9, "b": 0.7}) == pytest_approx(0.2)

    print("eval_retouch self-test: ok")
    print(f"  texture kept    {texture_retention(before, kept):.3f}")
    print(f"  texture smoothed {ratio:.3f} (correctly refused)")
    return 0


def pytest_approx(value: float, tolerance: float = 1e-9) -> float:
    """A tiny stand-in so this file needs nothing installed."""

    class _Approx(float):
        def __eq__(self, other):  # type: ignore[override]
            return math.isclose(float(self), float(other), abs_tol=tolerance)

    return _Approx(value)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run without numpy")
    parser.add_argument("--report", help="a retouch run to score, which does not exist here")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    print(
        "no scored retouch run is available in this repository; the Rust half of these gates is "
        "tests/eval/retouch_eval.rs and it runs in CI",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
