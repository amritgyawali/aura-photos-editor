#!/usr/bin/env python3
"""Export and statically check Phase 11's aesthetic ONNX graph at opset 13.

``--check`` runs without PyTorch and verifies the train/serve contract. A real export
requires the checkpoint produced by ``train_aesthetic.py`` and writes fp32 and fp16
graphs; model signing and parity remain the repository-wide ``cargo xtask models`` and
``ml/export_onnx/verify_parity.py`` steps.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

OPSET = 13
FEATURES = 35
ALLOWED_OPS = ("Gemm", "Relu", "Sigmoid")
INPUT = ("features", (None, FEATURES))
OUTPUT = ("aesthetic", (None, 1))


def check_contract() -> list[str]:
    problems: list[str] = []
    if INPUT[1] != (None, 35):
        problems.append("input must be dynamic [N,35]")
    if OUTPUT[1] != (None, 1):
        problems.append("output must be dynamic [N,1]")
    if OPSET != 13:
        problems.append("aura-infer accepts opset 13 for this graph")
    return problems


def export(checkpoint_path: Path, out: Path) -> int:
    try:
        import torch
        from torch import nn
    except ImportError:
        print("export requires PyTorch; no ONNX file was written", file=sys.stderr)
        return 1

    class Ranker(nn.Module):
        def __init__(self) -> None:
            super().__init__()
            self.layers = nn.Sequential(
                nn.Linear(FEATURES, 64), nn.ReLU(), nn.Linear(64, 32), nn.ReLU(), nn.Linear(32, 1)
            )

        def forward(self, values: Any) -> Any:
            return torch.sigmoid(self.layers(values))

    checkpoint = torch.load(checkpoint_path, map_location="cpu", weights_only=True)
    model = Ranker()
    model.load_state_dict(checkpoint["state_dict"])
    model.eval()
    out.mkdir(parents=True, exist_ok=True)

    for precision, dtype in (("fp32", torch.float32), ("fp16", torch.float16)):
        candidate = model.to(dtype=dtype)
        target = out / f"aesthetic_head_1.0.0.{precision}.onnx"
        sample = torch.zeros((2, FEATURES), dtype=dtype)
        torch.onnx.export(
            candidate,
            sample,
            target,
            input_names=[INPUT[0]],
            output_names=[OUTPUT[0]],
            dynamic_axes={INPUT[0]: {0: "batch"}, OUTPUT[0]: {0: "batch"}},
            opset_version=OPSET,
            do_constant_folding=True,
        )
        print(target)
    print("run: cargo xtask models --generate && cargo xtask models --check")
    print("run: python ml/export_onnx/verify_parity.py --all models")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--checkpoint")
    parser.add_argument("--out", default="models")
    args = parser.parse_args()

    problems = check_contract()
    print(f"opset {OPSET}; {INPUT[0]} {INPUT[1]} -> {OUTPUT[0]} {OUTPUT[1]}")
    print(f"allowed operators: {', '.join(ALLOWED_OPS)}")
    for problem in problems:
        print(f"[FAIL] {problem}")
    if problems:
        return 1
    print("[PASS] declared graph matches the Phase 11 train/serve contract")
    if args.check:
        return 0
    if not args.checkpoint:
        print("no --checkpoint supplied; nothing was exported (condition C1)")
        return 1
    return export(Path(args.checkpoint), Path(args.out))


if __name__ == "__main__":
    sys.exit(main())
