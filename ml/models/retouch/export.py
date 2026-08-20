#!/usr/bin/env python3
"""Export phase 20's two heads to ONNX, and verify what is already in `models.lock`.

Two jobs, and the second is the one that runs today.

**Export** turns a trained checkpoint into an opset-13 graph inside the documented subset the
interpreter implements (ADR-0007). There is no checkpoint in this repository, so the path exists
and refuses rather than pretending.

**Verify** reads `models/models.lock`, finds the two entries this phase registers, and checks the
things a caller depends on: the input and output shapes agree with the constants in
`aura_infer::onnx::fixtures` and `aura_core::contract::retouch`, int8 is forbidden on both, and
each has a model card. `cargo xtask models` does the signature and the digests; this does the
*meaning* of the entries, which a signature cannot check.

The output shapes are worth stating once, because three parts of the product agree about them:

    blemish_detector    crop  [N,3,256,256] -> anomalies [N,2,32,32]
    permanent_features  patch [N,3,64,64]   -> kinds     [N,6]

Two channels, not one: objectness and *temporary*. Six classes, not five: the sixth is `tattoo`
and it is the one class whose protection can never be cleared, which is why the head predicts it
rather than leaving it to be inferred.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

BLEMISH = "blemish_detector"
PERMANENT = "permanent_features"

EXPECTED = {
    BLEMISH: {
        "input": [1, 3, 256, 256],
        "outputs": {"anomalies": [1, 2, 32, 32]},
        "task": "detection",
        "class": "retouch",
    },
    PERMANENT: {
        "input": [1, 3, 64, 64],
        "outputs": {"kinds": [1, 6]},
        "task": "classification",
        "class": "retouch",
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
                f"{name} reads {colour}; both heads read linear light, because every operator "
                "in this phase works in it and a head trained on encoded pixels would disagree "
                "with the detector underneath it"
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
                f"{name} ships an int8 variant. Both heads forbid it: for the detector a "
                "quantised temporary channel moves marks across TEMPORARY_FLOOR, and for the "
                "classifier it moves the boundary between tattoo and birthmark - which is a "
                "promise this product cannot keep at eight bits"
            )
        if not precisions:
            failures.append(f"{name} has no variants")

        card = os.path.join("docs", "model-cards", f"{name}.md")
        if not os.path.exists(card):
            failures.append(f"{name} has no model card at {card}")

    if failures:
        for failure in failures:
            print(f"FAIL {failure}", file=sys.stderr)
        return 1

    print(f"export.py verify: {len(EXPECTED)} phase 20 models, shapes and cards agree")
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
            "carded. See docs/model-cards/blemish_detector.md",
            file=sys.stderr,
        )
        return 1

    parser.print_help()
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
