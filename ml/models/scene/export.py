"""Export the two scene heads to ONNX at opset 13, and check them before they ship.

    python ml/models/scene/export.py --check
    python ml/models/scene/export.py --scene scene_head.pt --out models/

## Why the check mode exists and runs first

`aura-infer` implements a documented subset of opset 13 and links no ONNX Runtime
(ADR-0007). An export that used an operator the interpreter does not have would pass
every check a training pipeline makes and fail at `AURA-ML-5006` on a photographer's
machine. So `--check` verifies the graph against the subset **before** it reaches
`models.lock`, and it is the first thing this script does in either mode.

The subset the two heads are allowed to use is deliberately tiny:

    Gemm, Relu, Softmax, Sigmoid

That is the whole list. An adapter and two linear heads need nothing else, and a design
that reached for a fifth operator would be worth arguing about rather than accommodating.

## The four things an export must get right

1. **The scene head has two outputs, in this order**: `scene_probs [N,22]` then
   `attr_probs [N,14]`. `SceneClassifier::classify` reads `outputs[0]` and `outputs[1]`
   positionally, because the interpreter's output map is ordered by the graph.
2. **Both activations are in the graph.** Softmax over the scenes, sigmoid over the
   attributes. The Rust decoder does not apply either, and a graph that stopped at the
   logits would produce a top-1 of 40 with a confidence of 40.
3. **The input is `[N, 528]`, dynamic in N.** Not `[1, 528]`: the pass batches 256.
4. **The abstention is not a slot.** 22 scene outputs, not 23. `SceneId::Unknown` is a
   decoder decision from the top-1 margin; adding a slot for it would make the model
   compete "I am not sure" against the classes it is unsure between.

## After a successful export

    cargo xtask models --generate    # re-writes models.lock and signs it
    cargo xtask models               # verifies signature, digests, cards, opset
    cargo test -p aura-brain-wedding # the gates, against the new weights

The second step is the one that catches a card that was not updated. Article VI rule M1:
no card, no model.
"""

from __future__ import annotations

import argparse
import sys
from typing import Sequence

OPSET = 13
ALLOWED_OPS = ("Gemm", "Relu", "Softmax", "Sigmoid")

SCENE_INPUT_DIM = 528
SCENE_CLASSES = 22
SCENE_ATTRIBUTES = 14
RITUAL_INPUT_DIM = 536
RITUAL_SLOTS = 160


def check_graph(path: str) -> int:
    """Verify one exported graph against the interpreter's subset."""
    try:
        import onnx
    except ImportError:
        print(
            "onnx is not installed, so the graph cannot be inspected here. "
            "`cargo xtask models` performs the same opset check in CI.",
            file=sys.stderr,
        )
        return 2

    model = onnx.load(path)
    failures = 0

    versions = {imported.domain: imported.version for imported in model.opset_import}
    if versions.get("", 0) != OPSET:
        print(f"FAIL: opset is {versions.get('', 0)}, expected {OPSET}")
        failures += 1

    for node in model.graph.node:
        if node.op_type not in ALLOWED_OPS:
            print(
                f"FAIL: `{node.op_type}` is not in the interpreter's subset "
                f"({', '.join(ALLOWED_OPS)}); this would be AURA-ML-5006 on a "
                f"photographer's machine"
            )
            failures += 1

    outputs = [output.name for output in model.graph.output]
    if outputs == ["scene_probs", "attr_probs"]:
        _check_shape(model, "scene_probs", SCENE_CLASSES, failures)
        _check_shape(model, "attr_probs", SCENE_ATTRIBUTES, failures)
    elif outputs == ["ritual_probs"]:
        _check_shape(model, "ritual_probs", RITUAL_SLOTS, failures)
    else:
        print(f"FAIL: outputs are {outputs}, expected the scene pair or the ritual head")
        failures += 1

    inputs = model.graph.input
    if len(inputs) != 1:
        print(f"FAIL: {len(inputs)} inputs, expected one feature vector")
        failures += 1
    else:
        dims = inputs[0].type.tensor_type.shape.dim
        if len(dims) != 2:
            print(f"FAIL: the input has rank {len(dims)}, expected [N, D]")
            failures += 1
        elif dims[0].dim_value != 0 and not dims[0].dim_param:
            print("FAIL: the batch dimension is fixed; the pass batches 256")
            failures += 1

    print(f"{path}:", "ok" if failures == 0 else f"{failures} problems")
    return 1 if failures else 0


def _check_shape(model, name: str, width: int, failures: int) -> int:
    for output in model.graph.output:
        if output.name != name:
            continue
        dims = output.type.tensor_type.shape.dim
        if len(dims) != 2 or dims[1].dim_value != width:
            print(f"FAIL: `{name}` is not [N, {width}]")
            return failures + 1
    return failures


def _describe() -> None:
    """What an export must produce, for a reader with no corpus and no torch."""
    print(
        f"""
scene_classifier
  input   features    [N, {SCENE_INPUT_DIM}]  dynamic N
  outputs scene_probs [N, {SCENE_CLASSES}]   Softmax IN the graph, axis -1
          attr_probs  [N, {SCENE_ATTRIBUTES}]   Sigmoid IN the graph
  ops     {', '.join(ALLOWED_OPS)}
  opset   {OPSET}

ritual_classifier
  input   features     [N, {RITUAL_INPUT_DIM}]  dynamic N
  outputs ritual_probs [N, {RITUAL_SLOTS}]  Softmax IN the graph, axis -1
  ops     {', '.join(ALLOWED_OPS)}
  opset   {OPSET}

  Slot 0 is RitualId::NONE. The slot a rite occupies is the `id` authored in its
  taxonomy file, so a renumbering relabels a trained output - which is why
  ADR-0015 forbids deriving ids from file order.

There is no trained head in this repository (condition C1), so there is nothing to
export today. `aura_infer::onnx::fixtures::scene_classifier` and `::ritual_classifier`
build the placeholder graphs with exactly these shapes, and `cargo xtask models
--generate` writes and signs them.
"""
    )


def main(argv: Sequence[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", nargs="*", help="verify exported .onnx files")
    parser.add_argument("--scene", help="a trained scene head")
    parser.add_argument("--ritual", help="a trained ritual head")
    parser.add_argument("--out", default="models/")
    args = parser.parse_args(argv)

    if args.check is not None:
        if not args.check:
            _describe()
            return 0
        return max(check_graph(path) for path in args.check)

    if not args.scene and not args.ritual:
        _describe()
        return 0

    print(
        "There is no trained head in this repository - condition C1 in "
        "docs/progress/PHASE-07-EXIT.md. Run --check for the required shapes.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
