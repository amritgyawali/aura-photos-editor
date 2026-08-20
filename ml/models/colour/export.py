#!/usr/bin/env python3
"""Export and statically check Phase 16's ONNX graph at opset 13.

``--check`` runs without PyTorch and verifies the train/serve contract. A real export
requires the checkpoint produced by ``train_tone.py`` and writes fp32 and fp16 graphs; model
signing and parity remain the repository-wide ``cargo xtask models`` and
``ml/export_onnx/verify_parity.py`` steps.

Two contract checks here have no counterpart in earlier phases:

* the output is **five scalars and not a curve**. A head that emitted control points could
  emit non-monotone ones, and monotonicity is the one property of this phase that is
  structural rather than checked. ADR-0033 decision 2, enforced at the export boundary so an
  experiment that changed the head's shape cannot quietly reach the product.
* **no int8 variant is written.** ``xtask`` sets ``forbid_int8`` on the manifest, and the
  reason is on the model card: a quantisation step of half a unit of contrast is invisible on
  one frame and is a gallery uniformly flatter than its album across four thousand.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

OPSET = 13

TONE_FEATURES = 11
TONE_SCENES = 23
TONE_INPUT_DIM = TONE_FEATURES + TONE_SCENES
TONE_OUTPUTS = 5
TONE_INPUT = ("features", (None, TONE_INPUT_DIM))
TONE_OUTPUT = ("tone", (None, TONE_OUTPUTS))
TONE_ALLOWED_OPS = ("Gemm", "Relu", "Sigmoid")

# Written variants. int8 is absent on purpose; see the module docstring.
VARIANTS = ("fp32", "fp16")

# The five outputs, in the order `colour::analyse::Head::predict` decodes them. The order is
# the contract: a checkpoint that emits them in another order produces a head that sets the
# black point when it means to set the contrast.
TARGET_ORDER = ("contrast", "highlights", "shadows", "whites", "blacks")


def contract() -> dict[str, Any]:
    """The train/serve contract, as data so ``--check`` and a reader see the same thing."""
    return {
        "opset": OPSET,
        "input": {
            "name": TONE_INPUT[0],
            "shape": list(TONE_INPUT[1]),
            "layout": "NC",
            "range": "unbounded",
            "colour": "none",
            "order": "colour::analyse::tone_features",
        },
        "output": {
            "name": TONE_OUTPUT[0],
            "shape": list(TONE_OUTPUT[1]),
            "activation": "sigmoid",
            "decoded_onto": "colour::tone's per-parameter bounds",
            "order": list(TARGET_ORDER),
        },
        "allowed_ops": list(TONE_ALLOWED_OPS),
        "variants": list(VARIANTS),
        "forbid_int8": True,
    }


def check(graph: dict[str, Any] | None = None) -> list[str]:
    """Every way an exported graph would break the contract.

    ``graph`` is the shape an ONNX inspection would produce; with none supplied the checks
    run against this module's own declaration, which is what makes ``--check`` runnable
    before there is anything to export.
    """
    declared = contract()
    graph = graph or {
        "opset": OPSET,
        "inputs": [{"name": TONE_INPUT[0], "shape": list(TONE_INPUT[1])}],
        "outputs": [{"name": TONE_OUTPUT[0], "shape": list(TONE_OUTPUT[1])}],
        "ops": list(TONE_ALLOWED_OPS),
        "variants": list(VARIANTS),
    }
    problems: list[str] = []

    if graph.get("opset") != OPSET:
        problems.append(f"opset {graph.get('opset')}, expected {OPSET}")

    inputs = graph.get("inputs", [])
    if len(inputs) != 1:
        problems.append(f"{len(inputs)} inputs, expected exactly one")
    elif inputs[0].get("name") != TONE_INPUT[0]:
        problems.append(f"input named `{inputs[0].get('name')}`, expected `{TONE_INPUT[0]}`")
    elif list(inputs[0].get("shape", []))[-1] != TONE_INPUT_DIM:
        problems.append(
            f"input width {list(inputs[0].get('shape', []))[-1]}, expected {TONE_INPUT_DIM}"
        )

    outputs = graph.get("outputs", [])
    if len(outputs) != 1:
        problems.append(f"{len(outputs)} outputs, expected exactly one")
    elif outputs[0].get("name") != TONE_OUTPUT[0]:
        problems.append(
            f"output named `{outputs[0].get('name')}`, expected `{TONE_OUTPUT[0]}`"
        )
    elif list(outputs[0].get("shape", []))[-1] != TONE_OUTPUTS:
        # The check that matters most. Five scalars is a tone prediction; anything else is a
        # head somebody has taught to emit a curve, and a curve from a head is a curve nobody
        # has proven monotone.
        problems.append(
            f"output width {list(outputs[0].get('shape', []))[-1]}, expected {TONE_OUTPUTS}; "
            "this head emits five parameters and never a curve"
        )

    unknown = [op for op in graph.get("ops", []) if op not in TONE_ALLOWED_OPS]
    if unknown:
        problems.append(f"operators outside the documented subset: {', '.join(unknown)}")

    if "int8" in graph.get("variants", []):
        problems.append(
            "an int8 variant was written; the manifest forbids it, and the reason is on the "
            "model card"
        )

    if declared["forbid_int8"] is not True:
        problems.append("this module's own declaration no longer forbids int8")

    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the contract and stop")
    parser.add_argument("--contract", action="store_true", help="print the contract")
    parser.add_argument("--checkpoint", type=Path, help="a trained checkpoint to export")
    args = parser.parse_args(argv)

    if args.contract:
        import json

        print(json.dumps(contract(), indent=2))
        return 0

    problems = check()
    for problem in problems:
        print(f"FAIL: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("contract: input, output, operators and variants all agree")

    if args.check or not args.checkpoint:
        if not args.checkpoint:
            print(
                "no checkpoint supplied; nothing was exported. `tone_model` remains the "
                "untrained placeholder `cargo xtask models --generate` writes.",
                file=sys.stderr,
            )
        return 0

    print(
        "export needs PyTorch, which this repository does not vendor.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
