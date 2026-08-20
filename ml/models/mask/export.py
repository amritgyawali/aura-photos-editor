#!/usr/bin/env python3
"""Export and statically check Phase 18's two ONNX graphs at opset 13.

``--check`` runs without PyTorch and verifies the train/serve contract for both heads. A real
export requires the checkpoints produced by ``train_seg.py`` and ``train_matting.py`` and writes
fp32 and fp16 graphs; model signing and parity remain the repository-wide
``cargo xtask models`` and ``ml/export_onnx/verify_parity.py`` steps.

Four contract checks here have no counterpart in earlier phases:

* **the segmentation output is a coarse grid, not a full-resolution mask.** A checkpoint whose
  decoder took the logits back to 768 px would not run in this build at all - the interpreter
  implements no ``Resize`` and no ``ConvTranspose`` (ADR-0007) - and it would also be the wrong
  shape: the upsample is a guided filter that can see the photograph's own edges, which a
  decoder cannot.
* **there is no softmax in the segmentation graph.** The interpreter normalises the *last* axis
  and the class axis is the second, so a softmax in the graph normalises across columns of the
  logit grid and produces a plausible tensor that means nothing.
* **there is a sigmoid in the matting graph.** A logistic is per element and cannot be applied
  to the wrong dimension, and an unbounded alpha is a clamp every caller has to remember.
* **no int8 variant is written** for either head. ``xtask`` sets ``forbid_int8`` on both
  manifests and the model cards carry the reasons: for the segmenter it is the margin between
  the top two logits, which is what the mask confidence is built from; for the matting head it
  is banding along a soft edge, which is the artefact section 10.1 audits for at 100 % zoom.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

OPSET = 13

# ---------------------------------------------------------------------------
# semantic_segment
# ---------------------------------------------------------------------------

SEG_INPUT_SIDE = 768
SEG_STRIDE = 16
SEG_HEAD_SIDE = SEG_INPUT_SIDE // SEG_STRIDE
SEG_CLASSES = 20
SEG_INPUT = ("pixels", (None, 3, SEG_INPUT_SIDE, SEG_INPUT_SIDE))
SEG_OUTPUT = ("logits", (None, SEG_CLASSES, SEG_HEAD_SIDE, SEG_HEAD_SIDE))
SEG_ALLOWED_OPS = ("Conv", "Relu", "MaxPool")

# The twenty classes, in the frozen iteration order of
# `aura_vision::contract::mask::ALL_KINDS`. The order is the contract: a checkpoint that emits
# them in another order produces a segmenter that calls somebody's hair their clothing.
SEG_CLASS_ORDER = (
    "skin",
    "face",
    "eyes",
    "sclera",
    "iris",
    "teeth",
    "lips",
    "eyebrows",
    "hair",
    "facial_hair",
    "clothing",
    "dress",
    "background",
    "sky",
    "subject",
    "greenery",
    "water",
    "floor",
    "window",
    "skin_safe",
)

# ---------------------------------------------------------------------------
# alpha_matting
# ---------------------------------------------------------------------------

MAT_PATCH_SIDE = 128
MAT_CHANNELS = 4
MAT_OUTPUT_SIDE = MAT_PATCH_SIDE // 4
MAT_INPUT = ("patch", (None, MAT_CHANNELS, MAT_PATCH_SIDE, MAT_PATCH_SIDE))
MAT_OUTPUT = ("alpha", (None, 1, MAT_OUTPUT_SIDE, MAT_OUTPUT_SIDE))
MAT_ALLOWED_OPS = ("Conv", "Relu", "MaxPool", "Sigmoid")

VARIANTS = ("fp32", "fp16")


def contract() -> dict[str, Any]:
    """The train/serve contract for both heads, as data so ``--check`` and a reader agree."""
    return {
        "opset": OPSET,
        "semantic_segment": {
            "input": {
                "name": SEG_INPUT[0],
                "shape": list(SEG_INPUT[1]),
                "layout": "NCHW",
                "range": "0..1",
                "colour": "linear_srgb",
            },
            "output": {
                "name": SEG_OUTPUT[0],
                "shape": list(SEG_OUTPUT[1]),
                "activation": "none",
                "normalised_by": "aura_vision::mask::segment, per pixel over the class axis",
                "classes": list(SEG_CLASS_ORDER),
            },
            "allowed_ops": list(SEG_ALLOWED_OPS),
            "stride": SEG_STRIDE,
        },
        "alpha_matting": {
            "input": {
                "name": MAT_INPUT[0],
                "shape": list(MAT_INPUT[1]),
                "layout": "NCHW",
                "range": "0..1",
                "colour": "linear_srgb+trimap",
            },
            "output": {
                "name": MAT_OUTPUT[0],
                "shape": list(MAT_OUTPUT[1]),
                "activation": "sigmoid",
                "upsampled_by": "the guided filter, with the photograph as the guide",
            },
            "allowed_ops": list(MAT_ALLOWED_OPS),
        },
        "variants": list(VARIANTS),
        "forbid_int8": True,
        "trained": False,
    }


def check_segmentation(graph: dict[str, Any]) -> list[str]:
    """Every way an exported segmentation graph would break the contract."""
    problems: list[str] = []

    if graph.get("opset") != OPSET:
        problems.append(f"segment: opset {graph.get('opset')}, expected {OPSET}")

    inputs = graph.get("inputs", [])
    if len(inputs) != 1:
        problems.append(f"segment: {len(inputs)} inputs, expected exactly one")
    elif inputs[0].get("name") != SEG_INPUT[0]:
        problems.append(
            f"segment: input named `{inputs[0].get('name')}`, expected `{SEG_INPUT[0]}`"
        )
    elif list(inputs[0].get("shape", []))[-1] != SEG_INPUT_SIDE:
        problems.append(
            f"segment: input side {list(inputs[0].get('shape', []))[-1]}, "
            f"expected {SEG_INPUT_SIDE}"
        )

    outputs = graph.get("outputs", [])
    if len(outputs) != 1:
        problems.append(f"segment: {len(outputs)} outputs, expected exactly one")
    else:
        shape = list(outputs[0].get("shape", []))
        if outputs[0].get("name") != SEG_OUTPUT[0]:
            problems.append(
                f"segment: output named `{outputs[0].get('name')}`, expected `{SEG_OUTPUT[0]}`"
            )
        elif len(shape) != 4:
            problems.append(f"segment: output rank {len(shape)}, expected 4")
        elif shape[1] != SEG_CLASSES:
            problems.append(
                f"segment: {shape[1]} classes, expected {SEG_CLASSES}; the head predicts every "
                "kind in ALL_KINDS and a mismatch is a silent reinterpretation of one class as "
                "another"
            )
        elif shape[-1] != SEG_HEAD_SIDE:
            # The check that matters most. A full-resolution output is a decoder this build
            # cannot execute and a design the guided filter does better.
            problems.append(
                f"segment: output side {shape[-1]}, expected {SEG_HEAD_SIDE}; this head emits a "
                f"stride-{SEG_STRIDE} grid and the upsample is a guided filter at render time"
            )

    unknown = [op for op in graph.get("ops", []) if op not in SEG_ALLOWED_OPS]
    if unknown:
        problems.append(
            f"segment: operators outside the documented subset: {', '.join(unknown)}"
        )
    if "Softmax" in graph.get("ops", []):
        problems.append(
            "segment: a Softmax is in the graph; the interpreter normalises the last axis and "
            "the class axis is the second, so it would normalise across columns of the grid"
        )

    classes = list(graph.get("classes", SEG_CLASS_ORDER))
    if classes != list(SEG_CLASS_ORDER):
        problems.append("segment: the class order does not match ALL_KINDS")

    return problems


def check_matting(graph: dict[str, Any]) -> list[str]:
    """Every way an exported matting graph would break the contract."""
    problems: list[str] = []

    if graph.get("opset") != OPSET:
        problems.append(f"matting: opset {graph.get('opset')}, expected {OPSET}")

    inputs = graph.get("inputs", [])
    if len(inputs) != 1:
        problems.append(f"matting: {len(inputs)} inputs, expected exactly one")
    else:
        shape = list(inputs[0].get("shape", []))
        if inputs[0].get("name") != MAT_INPUT[0]:
            problems.append(
                f"matting: input named `{inputs[0].get('name')}`, expected `{MAT_INPUT[0]}`"
            )
        elif len(shape) != 4 or shape[1] != MAT_CHANNELS:
            problems.append(
                f"matting: {shape[1] if len(shape) > 1 else '?'} input channels, expected "
                f"{MAT_CHANNELS}; three colour and one trimap, and without the trimap the "
                "network cannot know which side of the band is foreground"
            )

    outputs = graph.get("outputs", [])
    if len(outputs) != 1:
        problems.append(f"matting: {len(outputs)} outputs, expected exactly one")
    elif outputs[0].get("name") != MAT_OUTPUT[0]:
        problems.append(
            f"matting: output named `{outputs[0].get('name')}`, expected `{MAT_OUTPUT[0]}`"
        )

    if "Sigmoid" not in graph.get("ops", []):
        problems.append(
            "matting: no Sigmoid in the graph; an unbounded alpha is a clamp every caller has "
            "to remember, and a clamp outside the model is a contract nothing checks"
        )

    unknown = [op for op in graph.get("ops", []) if op not in MAT_ALLOWED_OPS]
    if unknown:
        problems.append(
            f"matting: operators outside the documented subset: {', '.join(unknown)}"
        )

    return problems


def check(graphs: dict[str, Any] | None = None) -> list[str]:
    """Both heads, plus the variant policy that applies to both."""
    declared = contract()
    graphs = graphs or {
        "semantic_segment": {
            "opset": OPSET,
            "inputs": [{"name": SEG_INPUT[0], "shape": list(SEG_INPUT[1])}],
            "outputs": [{"name": SEG_OUTPUT[0], "shape": list(SEG_OUTPUT[1])}],
            "ops": list(SEG_ALLOWED_OPS),
            "classes": list(SEG_CLASS_ORDER),
        },
        "alpha_matting": {
            "opset": OPSET,
            "inputs": [{"name": MAT_INPUT[0], "shape": list(MAT_INPUT[1])}],
            "outputs": [{"name": MAT_OUTPUT[0], "shape": list(MAT_OUTPUT[1])}],
            "ops": list(MAT_ALLOWED_OPS),
        },
        "variants": list(VARIANTS),
    }

    problems = check_segmentation(graphs.get("semantic_segment", {}))
    problems += check_matting(graphs.get("alpha_matting", {}))

    if "int8" in graphs.get("variants", []):
        problems.append(
            "an int8 variant was written; both manifests forbid it, and the reasons are on the "
            "model cards"
        )
    if declared["forbid_int8"] is not True:
        problems.append("this module's own declaration no longer forbids int8")
    if declared["trained"] is not False:
        problems.append(
            "this module claims a trained head; neither is trained, and SEG_HEAD_TRAINED and "
            "MATTING_HEAD_TRAINED are both false in the Rust that would consult them"
        )

    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the contract and stop")
    parser.add_argument("--contract", action="store_true", help="print the contract")
    args = parser.parse_args(argv)

    if args.contract:
        print(json.dumps(contract(), indent=2))
        return 0

    if args.check:
        problems = check()
        if problems:
            for problem in problems:
                print(f"export: {problem}", file=sys.stderr)
            return 1
        print(
            f"export: both graphs satisfy the opset {OPSET} contract; "
            f"{SEG_CLASSES} classes at stride {SEG_STRIDE}, "
            f"{MAT_CHANNELS}-channel matting patches, no int8 variant"
        )
        print(
            "export: neither head is trained and neither is consulted; "
            "see docs/model-cards/semantic_segment.md"
        )
        return 0

    print(
        "export: nothing to export. This phase ships architecture fixtures and the weights are "
        "produced by `cargo xtask models --generate`; run with --check to verify the contract.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
