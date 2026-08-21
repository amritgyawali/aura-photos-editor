#!/usr/bin/env python3
"""Export phase 21's three heads to ONNX, and verify what is already in `models.lock`.

Two jobs, and the second is the one that runs today.

**Export** turns a trained checkpoint into an opset-13 graph inside the documented subset the
interpreter implements (ADR-0007). There is no checkpoint in this repository, so the path exists
and refuses rather than pretending.

**Verify** reads `models/models.lock`, finds the three entries this phase registers, and checks
the things a caller depends on: the input and output shapes agree with the constants in
`aura_infer::onnx::fixtures`, every head reads linear light, int8 is forbidden on all three, and
each has a model card. `cargo xtask models` does the signature and the digests; this does the
*meaning* of the entries, which a signature cannot check.

The output shapes are worth stating once, because three parts of the product agree about them:

    flyaway_detector    tile   [N,3,128,128] -> strands [N,1,16,16]
    glare_detector      region [N,3,64,64]   -> glare   [N,2]
    lint_detector       patch  [N,3,64,64]   -> kinds   [N,4]

One channel on the first, where phase 20's blemish detector has two: a mark on skin can be
temporary or permanent and the product treats those differently, and a strand of hair is never
temporary. Two outputs on the second, and they are different kinds of number - a judgement, then
a *measured share* of the region that carries no information, which is what
`MIN_SPECULAR_FRACTION` turns into permission to borrow. Four classes on the third, not six: a
visible strap and a crease are opt-in only, and they are absent from the head rather than
suppressed downstream, so there is no accuracy at which they start being found.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

FLYAWAY = "flyaway_detector"
GLARE = "glare_detector"
LINT = "lint_detector"

EXPECTED = {
    FLYAWAY: {
        "input": [1, 3, 128, 128],
        "outputs": {"strands": [1, 1, 16, 16]},
        "task": "detection",
        "class": "retouch",
        "int8_reason": (
            "a strand is only a strand where the background behind it is quiet, and both halves "
            "of that comparison sit a few hundredths apart on a busy background"
        ),
    },
    GLARE: {
        "input": [1, 3, 64, 64],
        "outputs": {"glare": [1, 2]},
        "task": "detection",
        "class": "retouch",
        "int8_reason": (
            "the second output is a share rather than a score, and MIN_SPECULAR_FRACTION turns "
            "it into permission to composite two photographs"
        ),
    },
    LINT: {
        "input": [1, 3, 64, 64],
        "outputs": {"kinds": [1, 4]},
        "task": "classification",
        "class": "retouch",
        "int8_reason": (
            "the classes differ by how far a small mark departs from the fabric around it, which "
            "is the smallest quantity anything in this phase measures"
        ),
    },
}


def verify(models_dir: str) -> int:
    lock_path = os.path.join(models_dir, "models.lock")
    if not os.path.exists(lock_path):
        print(f"no manifest at {lock_path}", file=sys.stderr)
        return 1
    with open(lock_path, encoding="utf-8") as handle:
        lock = json.load(handle)

    entries = {m["name"]: m for m in lock.get("models", [])}
    failures = []

    for name, expected in EXPECTED.items():
        model = entries.get(name)
        if model is None:
            failures.append(f"{name} is not in models.lock")
            continue

        if model.get("task") != expected["task"]:
            failures.append(f"{name} task is {model.get('task')}, expected {expected['task']}")
        if model.get("class") != expected["class"]:
            failures.append(f"{name} class is {model.get('class')}, expected {expected['class']}")

        shape = model.get("input", {}).get("shape")
        if shape != expected["input"]:
            failures.append(f"{name} input is {shape}, expected {expected['input']}")

        colour = model.get("input", {}).get("colour")
        if colour != "linear_srgb":
            failures.append(
                f"{name} reads {colour}; all three heads read linear light, because every "
                "operator in this phase works in it and a head trained on encoded pixels would "
                "disagree with the measurement underneath it"
            )

        for output, expected_shape in expected["outputs"].items():
            actual = model.get("output", {}).get(output)
            if actual != expected_shape:
                failures.append(
                    f"{name} output {output} is {actual}, expected {expected_shape}"
                )

        precisions = {v.get("precision") for v in model.get("variants", [])}
        if "int8" in precisions:
            failures.append(
                f"{name} ships an int8 variant, and it forbids one: {expected['int8_reason']}"
            )
        if not precisions:
            failures.append(f"{name} has no variants")

        card = os.path.join("docs", "model-cards", f"{name}.md")
        if not os.path.exists(card):
            failures.append(f"{name} has no model card at {card}")

    # The head that cannot name an opt-in kind. Four classes rather than six is a product
    # decision rather than a size, and a manifest that widened it would be the first sign that
    # somebody had made crease removal a detection problem.
    lint = entries.get(LINT)
    if lint is not None:
        kinds = lint.get("output", {}).get("kinds")
        if kinds is not None and len(kinds) == 2 and kinds[1] != 4:
            failures.append(
                f"{LINT} predicts {kinds[1]} classes. Four is the whole set it may name: none, "
                "lint, thread and stain. A strap and a crease are opt-in, and a head that can "
                "name them is a head that can find them in a studio that never asked"
            )

    if failures:
        for failure in failures:
            print(f"FAIL {failure}", file=sys.stderr)
        return 1

    print(f"export.py verify: {len(EXPECTED)} phase 21 models, shapes and cards agree")
    for name, expected in EXPECTED.items():
        outputs = ", ".join(f"{k} {v}" for k, v in expected["outputs"].items())
        print(f"  {name}: {expected['input']} -> {outputs}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", metavar="MODELS_DIR", help="check models.lock and the cards")
    parser.add_argument("--checkpoint", help="a trained checkpoint, which does not exist here")
    parser.add_argument("--out", help="where to write the ONNX graph")
    args = parser.parse_args()

    if args.verify:
        return verify(args.verify)

    if args.checkpoint:
        print(
            "no trained checkpoint exists in this repository; the placeholder graphs are built "
            "by `cargo xtask models --generate` from aura_infer::onnx::fixtures, signed, and "
            "carded. See docs/model-cards/flyaway_detector.md",
            file=sys.stderr,
        )
        return 1

    parser.print_help()
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
