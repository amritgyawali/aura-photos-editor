#!/usr/bin/env python3
"""Export and statically check Phase 15's two ONNX graphs at opset 13.

``--check`` runs without PyTorch and verifies the train/serve contract for both heads. A
real export requires the checkpoints produced by ``train_wb.py`` and ``train_exposure.py``
and writes fp32 and fp16 graphs; model signing and parity remain the repository-wide
``cargo xtask models`` and ``ml/export_onnx/verify_parity.py`` steps.

Two contract checks here have no counterpart in earlier phases and both are colour rather
than shape:

* the white-balance input is declared **linear**, so an exporter that normalised an sRGB
  thumbnail is a contract violation rather than a preprocessing choice;
* **no int8 variant is written for either head.** ``xtask`` sets ``forbid_int8`` on both
  manifests, and the reasons are on the model cards - a 200 K tolerance is three
  quantisation steps wide for the white-balance head, and a systematic bias per chapter is
  what int8 costs the exposure head.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

OPSET = 13

WB_SIDE = 64
WB_CHANNELS = 3
WB_OUTPUTS = 2
WB_INPUT = ("thumb", (None, WB_CHANNELS, WB_SIDE, WB_SIDE))
WB_OUTPUT = ("illuminant_uv", (None, WB_OUTPUTS))
WB_COLOUR = "linear_srgb"
WB_ALLOWED_OPS = (
    "Conv",
    "Relu",
    "MaxPool",
    "GlobalAveragePool",
    "Flatten",
    "Gemm",
    "Sigmoid",
)

EXPOSURE_FEATURES = 35
EXPOSURE_INPUT = ("features", (None, EXPOSURE_FEATURES))
EXPOSURE_OUTPUT = ("exposure", (None, 1))
EXPOSURE_ALLOWED_OPS = ("Gemm", "Relu", "Sigmoid")

# Written variants. int8 is absent on purpose; see the module docstring.
PRECISIONS = ("fp32", "fp16")


def check_contract() -> list[str]:
    problems: list[str] = []
    if OPSET != 13:
        problems.append("aura-infer accepts opset 13 for these graphs")
    if WB_INPUT[1] != (None, 3, 64, 64):
        problems.append("white_balance input must be dynamic [N,3,64,64]")
    if WB_OUTPUT[1] != (None, 2):
        problems.append("white_balance output must be dynamic [N,2]")
    if WB_COLOUR != "linear_srgb":
        problems.append(
            "white_balance input must be linear; a transfer curve before the first "
            "convolution makes a channel ratio depend on brightness"
        )
    if EXPOSURE_INPUT[1] != (None, 35):
        problems.append("exposure_scene input must be dynamic [N,35]")
    if EXPOSURE_OUTPUT[1] != (None, 1):
        problems.append("exposure_scene output must be dynamic [N,1]")
    if "int8" in PRECISIONS:
        problems.append("neither Phase 15 head may ship an int8 variant")
    return problems


def export_white_balance(checkpoint_path: Path, out: Path) -> int:
    try:
        import torch
        from torch import nn
    except ImportError:
        print("export requires PyTorch; no ONNX file was written", file=sys.stderr)
        return 1

    class Illuminant(nn.Module):
        def __init__(self) -> None:
            super().__init__()
            self.trunk = nn.Sequential(
                nn.Conv2d(WB_CHANNELS, 16, 3, stride=2, padding=1),
                nn.ReLU(),
                nn.Conv2d(16, 32, 3, stride=2, padding=1),
                nn.ReLU(),
                nn.MaxPool2d(2),
                nn.Conv2d(32, 48, 3, stride=2, padding=1),
                nn.ReLU(),
                nn.AdaptiveAvgPool2d(1),
                nn.Flatten(),
            )
            self.head = nn.Sequential(nn.Linear(48, 64), nn.ReLU(), nn.Linear(64, WB_OUTPUTS))

        def forward(self, values: Any) -> Any:
            # Sigmoid is added at export, not at training. The decoder applies the fixed
            # rescale onto the u'v' box, so the graph's output stays 0..1 and the one place
            # that knows the box is `tone::analyse`.
            return torch.sigmoid(self.head(self.trunk(values)))

    checkpoint = torch.load(checkpoint_path, map_location="cpu", weights_only=True)
    model = Illuminant()
    model.load_state_dict(checkpoint["state_dict"])
    model.eval()
    out.mkdir(parents=True, exist_ok=True)

    for precision, dtype in (("fp32", torch.float32), ("fp16", torch.float16)):
        candidate = model.to(dtype=dtype)
        target = out / f"white_balance_1.0.0.{precision}.onnx"
        sample = torch.zeros((2, WB_CHANNELS, WB_SIDE, WB_SIDE), dtype=dtype)
        torch.onnx.export(
            candidate,
            sample,
            target,
            input_names=[WB_INPUT[0]],
            output_names=[WB_OUTPUT[0]],
            dynamic_axes={WB_INPUT[0]: {0: "batch"}, WB_OUTPUT[0]: {0: "batch"}},
            opset_version=OPSET,
            do_constant_folding=True,
        )
        print(target)
    return 0


def export_exposure(checkpoint_path: Path, out: Path) -> int:
    try:
        import torch
        from torch import nn
    except ImportError:
        print("export requires PyTorch; no ONNX file was written", file=sys.stderr)
        return 1

    class Exposure(nn.Module):
        def __init__(self) -> None:
            super().__init__()
            self.layers = nn.Sequential(
                nn.Linear(EXPOSURE_FEATURES, 48),
                nn.ReLU(),
                nn.Linear(48, 24),
                nn.ReLU(),
                nn.Linear(24, 1),
            )

        def forward(self, values: Any) -> Any:
            return torch.sigmoid(self.layers(values))

    checkpoint = torch.load(checkpoint_path, map_location="cpu", weights_only=True)
    model = Exposure()
    model.load_state_dict(checkpoint["state_dict"])
    model.eval()
    out.mkdir(parents=True, exist_ok=True)

    for precision, dtype in (("fp32", torch.float32), ("fp16", torch.float16)):
        candidate = model.to(dtype=dtype)
        target = out / f"exposure_scene_1.0.0.{precision}.onnx"
        sample = torch.zeros((2, EXPOSURE_FEATURES), dtype=dtype)
        torch.onnx.export(
            candidate,
            sample,
            target,
            input_names=[EXPOSURE_INPUT[0]],
            output_names=[EXPOSURE_OUTPUT[0]],
            dynamic_axes={EXPOSURE_INPUT[0]: {0: "batch"}, EXPOSURE_OUTPUT[0]: {0: "batch"}},
            opset_version=OPSET,
            do_constant_folding=True,
        )
        print(target)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--wb-checkpoint")
    parser.add_argument("--exposure-checkpoint")
    parser.add_argument("--out", default="models")
    args = parser.parse_args()

    problems = check_contract()
    print(f"opset {OPSET}")
    print(f"  {WB_INPUT[0]} {WB_INPUT[1]} ({WB_COLOUR}) -> {WB_OUTPUT[0]} {WB_OUTPUT[1]}")
    print(f"  allowed operators: {', '.join(WB_ALLOWED_OPS)}")
    print(f"  {EXPOSURE_INPUT[0]} {EXPOSURE_INPUT[1]} -> {EXPOSURE_OUTPUT[0]} {EXPOSURE_OUTPUT[1]}")
    print(f"  allowed operators: {', '.join(EXPOSURE_ALLOWED_OPS)}")
    print(f"  variants written: {', '.join(PRECISIONS)} (int8 forbidden on both heads)")
    for problem in problems:
        print(f"[FAIL] {problem}")
    if problems:
        return 1
    print("[PASS] declared graphs match the Phase 15 train/serve contract")
    if args.check:
        return 0
    if not args.wb_checkpoint and not args.exposure_checkpoint:
        print("no checkpoint supplied; nothing was exported (condition C1)")
        return 1

    out = Path(args.out)
    status = 0
    if args.wb_checkpoint:
        status |= export_white_balance(Path(args.wb_checkpoint), out)
    if args.exposure_checkpoint:
        status |= export_exposure(Path(args.exposure_checkpoint), out)
    print("run: cargo xtask models --generate && cargo xtask models")
    print("run: python ml/export_onnx/verify_parity.py --all models")
    return status


if __name__ == "__main__":
    sys.exit(main())
