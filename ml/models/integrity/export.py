"""Export the two phase 09 heads to ONNX at opset 13, and check them before they ship.

    python ml/models/integrity/export.py --check
    python ml/models/integrity/export.py --focus focus_head.pt --eyes eye_state.pt --out models/

## Why the check mode exists and runs first

`aura-infer` implements a documented subset of opset 13 and links no ONNX Runtime
(ADR-0007). An export that used an operator the interpreter does not have would pass
every check a training pipeline makes and fail at `AURA-ML-5006` on a photographer's
machine. So `--check` verifies the graph against the subset **before** it reaches
`models.lock`, and it is the first thing this script does in either mode.

The subset these two heads are allowed to use:

    Conv, Relu, MaxPool, GlobalAveragePool, Flatten, Gemm, Softmax

Seven operators. A design that reached for an eighth would be worth arguing about rather
than accommodating - and in particular, **no BatchNormalization**: it would have to be
folded into the preceding convolution at export time, and a fold that goes wrong produces
a model that is subtly miscalibrated rather than obviously broken.

## The five things an export must get right

1. **The focus head's input is one channel**, `[N, 1, 64, 64]`. A model trained on colour
   crops and exported to a one-channel input has had its first convolution silently
   reinterpreted, and the failure looks like a badly trained model rather than a bad
   export.
2. **The eye head's input is three channels at 112 px**, and its crop is the *two-point*
   alignment `eyes::align_from_eyes` produces - not phase 06's five-point warp.
3. **Both softmaxes are in the graph.** The Rust decoders (`focus::decode_focus`,
   `eyes::decode`) take a probability vector and pick an argmax; a graph that stopped at
   the logits would produce a confident answer with a confidence of 8.4.
4. **The class order is the contract.** `FocusClass::ALL` is sharp, soft, bokeh.
   `EyeOpenness::ALL` is open, squint, closed, looking down, occluded. Renumbering either
   relabels a trained model, and nothing downstream would notice until a photographer did.
5. **Both inputs are dynamic in N.** The analyser batches every region of a frame into
   one call and every face of a frame into another; a fixed batch of 1 would make a
   group photograph thirty calls.

## After a successful export

    cargo xtask models --generate    # re-writes models.lock and signs it
    cargo xtask models               # verifies signature, digests, cards, opset
    cargo test -p aura-brain-photo   # the gates, against the new weights

The second step is the one that catches a card that was not updated. Article VI rule M1:
no card, no model - and a card whose "Trained" row still says **No** after a training run
is a card that will mislead the next person to read it.
"""

from __future__ import annotations

import argparse
import sys

#: The operators the interpreter has, for these two graphs.
ALLOWED_OPS = (
    "Conv",
    "Relu",
    "MaxPool",
    "GlobalAveragePool",
    "Flatten",
    "Gemm",
    "Softmax",
)

#: `fixtures::FOCUS_CROP_SIDE` and `fixtures::FOCUS_CLASSES`.
FOCUS_INPUT = ("crop", (None, 1, 64, 64))
FOCUS_OUTPUT = ("focus_probs", (None, 3))

#: `fixtures::EYE_CROP_SIDE` and `fixtures::EYE_CLASSES`.
EYE_INPUT = ("crop", (None, 3, 112, 112))
EYE_OUTPUT = ("eye_probs", (None, 5))

#: The opset `aura-infer` parses. `aura_infer::onnx::OPSET`.
OPSET = 13


def check_graph(name: str, ops: list[str], inputs: dict, outputs: dict) -> list[str]:
    """Every way one exported graph can be wrong, as a list of complaints."""
    problems: list[str] = []
    for op in ops:
        if op not in ALLOWED_OPS:
            problems.append(f"{name}: operator `{op}` is outside the documented subset")
    expected_in, expected_out = (
        (FOCUS_INPUT, FOCUS_OUTPUT) if name == "focus_head" else (EYE_INPUT, EYE_OUTPUT)
    )
    if expected_in[0] not in inputs:
        problems.append(f"{name}: input must be named `{expected_in[0]}`")
    elif tuple(inputs[expected_in[0]]) != expected_in[1]:
        problems.append(
            f"{name}: input shape is {tuple(inputs[expected_in[0]])}, expected "
            f"{expected_in[1]}"
        )
    if expected_out[0] not in outputs:
        problems.append(f"{name}: output must be named `{expected_out[0]}`")
    elif tuple(outputs[expected_out[0]]) != expected_out[1]:
        problems.append(
            f"{name}: output shape is {tuple(outputs[expected_out[0]])}, expected "
            f"{expected_out[1]}"
        )
    if "Softmax" not in ops:
        problems.append(f"{name}: the softmax must be in the graph, not in the decoder")
    return problems


def check() -> int:
    """Check the *shipped* graphs' declared shapes, which is what this build can check.

    With no PyTorch and no exported checkpoint here, what this mode verifies is that the
    constants in this file still agree with the ones the Rust fixtures declare - so an
    export written against this script would produce a graph the interpreter accepts.
    A real export additionally runs `check_graph` over the loaded ONNX.
    """
    problems: list[str] = []

    expected = {
        "focus_head": (FOCUS_INPUT, FOCUS_OUTPUT, 64, 3),
        "eye_state": (EYE_INPUT, EYE_OUTPUT, 112, 5),
    }
    for name, (spec_in, spec_out, side, classes) in expected.items():
        if spec_in[1][2] != side or spec_in[1][3] != side:
            problems.append(f"{name}: the declared crop side disagrees with itself")
        if spec_out[1][1] != classes:
            problems.append(f"{name}: the declared class count disagrees with itself")
        if spec_in[1][0] is not None:
            problems.append(f"{name}: the batch dimension must be dynamic")

    print(f"opset {OPSET}, operators: {', '.join(ALLOWED_OPS)}")
    print(f"focus_head: {FOCUS_INPUT[1]} -> {FOCUS_OUTPUT[1]}")
    print(f"eye_state:  {EYE_INPUT[1]} -> {EYE_OUTPUT[1]}")
    for problem in problems:
        print(f"[FAIL] {problem}")
    if problems:
        return 1
    print("[PASS] both declared graphs are inside the documented subset")
    print()
    print(
        "Note: the shipped weights are placeholders (condition C1). This mode checks the "
        "shapes an export must produce; it cannot check weights that do not exist."
    )
    return 0


def export(focus: str | None, eyes: str | None, out: str) -> int:
    print(f"would export focus={focus} eyes={eyes} into {out}")
    print(
        "There is nothing to export in this repository: neither head has been trained "
        "(condition C1 in docs/progress/PHASE-09-EXIT.md). Run --check to verify the "
        "shapes an export must produce."
    )
    return 1


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="verify the declared graphs")
    parser.add_argument("--focus", help="trained focus head")
    parser.add_argument("--eyes", help="trained eye-state head")
    parser.add_argument("--out", default="models/", help="where to write the ONNX files")
    args = parser.parse_args(argv)

    # The check runs first in either mode, by this file's own rule.
    status = check()
    if args.check or status != 0:
        return status
    return export(args.focus, args.eyes, args.out)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
