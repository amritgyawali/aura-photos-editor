"""Quantise a model's weights, the same way the runtime does.

Per-tensor affine int8: one scale and one zero point per tensor, chosen from the
tensor's own range, with zero always exactly representable. That last property is
not cosmetic - padded convolutions insert real zeros, and a quantisation that
renders zero as -0.004 puts a grey halo around every padded edge.

The arithmetic here is the mirror of ``QuantParams`` in
``crates/aura-infer/src/tensor.rs``. It has to be: a file quantised here is
loaded there, and a difference between the two would show up as a model that
passes its parity check at export and fails it on a customer's machine.

Half precision is also offered, because the fp16 variants in ``models.lock`` are
produced the same way - by rounding every weight through binary16 at export
rather than at load.

Usage::

    python ml/export_onnx/quantise.py models/scene_1.0.0.fp32.onnx --int8
    python ml/export_onnx/quantise.py models/scene_1.0.0.fp32.onnx --fp16 --out /tmp/x.onnx
"""

from __future__ import annotations

import argparse
import pathlib
import sys

import numpy as np

from onnx_min import rewrite_initialisers


class QuantParams:
    """A per-tensor affine quantisation."""

    def __init__(self, scale: float, zero_point: int) -> None:
        self.scale = scale
        self.zero_point = zero_point

    @classmethod
    def observe(cls, values: list[float]) -> "QuantParams":
        """Choose parameters covering the values present in a tensor."""
        low = min(0.0, min(values, default=0.0))
        high = max(0.0, max(values, default=0.0))
        span = high - low
        if span <= float(np.finfo(np.float32).eps):
            return cls(1.0, 0)
        scale = span / 255.0
        zero_point = int(np.clip(round(-128.0 - low / scale), -128, 127))
        return cls(scale, zero_point)

    def round_trip(self, value: float) -> float:
        """Round one value through the quantised grid and back."""
        quantised = int(np.clip(round(value / self.scale) + self.zero_point, -128, 127))
        return float(np.float32((quantised - self.zero_point) * self.scale))


def quantise_int8(values: list[float]) -> tuple[list[float], float]:
    """Quantise a tensor, returning the values and the worst error."""
    params = QuantParams.observe(values)
    out = [params.round_trip(value) for value in values]
    worst = max((abs(a - b) for a, b in zip(values, out)), default=0.0)
    return out, worst


def round_fp16(values: list[float]) -> tuple[list[float], float]:
    """Round a tensor through binary16, returning the values and the worst error."""
    rounded = np.array(values, dtype=np.float32).astype(np.float16).astype(np.float32)
    out = [float(value) for value in rounded]
    worst = max((abs(a - b) for a, b in zip(values, out)), default=0.0)
    return out, worst


def convert(path: pathlib.Path, out: pathlib.Path, precision: str) -> int:
    """Write a converted copy of a model and report the worst weight error."""
    payload = path.read_bytes()
    worst_overall = 0.0
    tensors = 0

    def rewrite(name: str, values: list[float]) -> list[float]:
        nonlocal worst_overall, tensors
        tensors += 1
        converted, worst = quantise_int8(values) if precision == "int8" else round_fp16(values)
        if worst > worst_overall:
            worst_overall = worst
        print(f"  {name}: {len(values)} weights, worst error {worst:.3e}")
        return converted

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(rewrite_initialisers(payload, rewrite))
    print(
        f"{out.name}: {tensors} tensors at {precision}, worst weight error {worst_overall:.3e}"
    )

    # The tolerances are the ones the runtime asserts: section 6.4 of the phase
    # document, and `Precision::parity_tolerance` in the Rust contract.
    budget = 1e-2 if precision == "int8" else 1e-3
    if worst_overall > budget:
        print(f"{out.name}: worst weight error {worst_overall:.3e} is above the {budget:.0e} budget")
        return 1
    return 0


def main() -> int:
    """Command line entry point."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", type=pathlib.Path)
    parser.add_argument("--int8", action="store_true")
    parser.add_argument("--fp16", action="store_true")
    parser.add_argument("--out", type=pathlib.Path)
    args = parser.parse_args()

    if args.int8 == args.fp16:
        parser.error("choose exactly one of --int8 and --fp16")

    precision = "int8" if args.int8 else "fp16"
    out = args.out or args.model.with_name(
        args.model.name.replace(".fp32.onnx", f".{precision}.onnx")
    )
    return convert(args.model, out, precision)


if __name__ == "__main__":
    sys.exit(main())
