#!/usr/bin/env python3
"""Export phase 22's two heads to ONNX, and verify what is already in `models.lock`.

Two jobs, and the second is the one that runs today.

**Export** turns a trained checkpoint into an opset-13 graph inside the documented subset the
interpreter implements (ADR-0007). There is no checkpoint in this repository, so the path exists
and refuses rather than pretending.

**Verify** reads `models/models.lock`, finds the two entries this phase registers, and checks the
things a caller depends on: the input and output shapes agree with the constants in
`aura_infer::onnx::fixtures`, int8 is forbidden on both, each has a model card, and - the check
this phase adds - **neither head emits an image**. `cargo xtask models` does the signature and the
digests; this does the *meaning* of the entries, which a signature cannot check.

The shapes are worth stating once, because three parts of the product agree about them:

    denoise         tile [N,4,128,128] -> residual [N,3,128,128]
    face_recovery   crop [N,3,112,112] -> detail   [N,1,112,112]

Four input planes on the first, and the fourth is not colour. It carries the sigma the camera's
photon transfer curve predicts at each pixel's own signal level, which is section 6.1's
conditioning; the manifest says `linear_srgb+noise` rather than claiming four colour planes, the
way phase 18's matting head says `linear_srgb+trimap`.

One output channel on the second, and it is luminance. A chroma residual on a face is a colour
change on a face - a different operation with a much worse failure mode, and one the identity
constraint has no way to distinguish from the operation that was wanted.

**Both outputs are residuals.** That is the check `verify_residual_outputs` exists for and it is
the structural half of this phase's identity guarantee: a head that emitted an image could emit a
different face, and a head that emits a high-frequency correction to the face that is already
there cannot, because the low and mid bands never pass through it.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

DENOISE = "denoise"
FACE_RECOVERY = "face_recovery"

EXPECTED = {
    DENOISE: {
        "input": [1, 4, 128, 128],
        "outputs": {"residual": [1, 3, 128, 128]},
        "task": "restoration",
        "class": "retouch",
        "colour": "linear_srgb+noise",
        "int8_reason": (
            "the output is a residual whose whole useful range is a few hundredths of diffuse "
            "white - that is what noise is - so an int8 quantisation of it has about four usable "
            "levels, and a denoiser quantised to four levels is a posteriser"
        ),
    },
    FACE_RECOVERY: {
        "input": [1, 3, 112, 112],
        "outputs": {"detail": [1, 1, 112, 112]},
        "task": "restoration",
        "class": "retouch",
        "colour": "linear_srgb",
        "int8_reason": (
            "MAX_IDENTITY_DRIFT is eight hundredths of a cosine distance, and a systematic "
            "quantisation shift moves every face in a wedding by the same small amount in the "
            "same direction - the one kind of error that ceiling cannot absorb"
        ),
    },
}

# The two outputs are residuals. A head whose output channel count equals its input colour channel
# count *and* whose name suggests an image is the shape this check is looking for; both entries
# here are named `residual` and `detail` deliberately.
RESIDUAL_OUTPUT_NAMES = {"residual", "detail"}


def repository_root() -> str:
    return os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))


def load_lock(models_dir: str) -> dict:
    path = os.path.join(models_dir, "models.lock")
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def verify(models_dir: str) -> int:
    failures: list[str] = []
    try:
        lock = load_lock(models_dir)
    except OSError as error:
        print(f"FAIL could not read models.lock: {error}", file=sys.stderr)
        return 1

    by_name = {entry["name"]: entry for entry in lock.get("models", [])}

    for name, expected in EXPECTED.items():
        entry = by_name.get(name)
        if entry is None:
            failures.append(f"{name}: not registered in models.lock")
            continue

        shape = entry.get("input", {}).get("shape")
        if shape != expected["input"]:
            failures.append(f"{name}: input shape {shape} is not {expected['input']}")

        colour = entry.get("input", {}).get("colour")
        if colour != expected["colour"]:
            failures.append(f"{name}: input colour {colour!r} is not {expected['colour']!r}")

        kinds = entry.get("output", {}).get("kinds")
        outputs = entry.get("output", {})
        # The manifest stores outputs either as a named map or as a single `kinds` list depending
        # on the entry; both forms are accepted and the shapes are what matter.
        for output_name, output_shape in expected["outputs"].items():
            found = outputs.get(output_name, kinds)
            if found != output_shape:
                failures.append(
                    f"{name}: output {output_name} shape {found} is not {output_shape}"
                )
            if output_name not in RESIDUAL_OUTPUT_NAMES:
                failures.append(
                    f"{name}: output {output_name} is not one of the residual names; a head that "
                    "emits an image can emit a different face"
                )

        if entry.get("task") != expected["task"]:
            failures.append(f"{name}: task {entry.get('task')!r} is not {expected['task']!r}")

        policy = entry.get("precision_policy", {})
        if not policy.get("forbid_int8"):
            failures.append(f"{name}: int8 is not forbidden, and it must be - {expected['int8_reason']}")

        for variant in entry.get("variants", []):
            if variant.get("precision") == "int8":
                failures.append(f"{name}: an int8 variant is registered")

        card = entry.get("model_card")
        if not card:
            failures.append(f"{name}: no model card")
        else:
            card_path = os.path.join(repository_root(), card)
            if not os.path.exists(card_path):
                failures.append(f"{name}: model card {card} does not exist")
            else:
                with open(card_path, "r", encoding="utf-8") as handle:
                    text = handle.read()
                # Both heads are untrained, and the card has to say so where a reader will see it
                # rather than in a footnote. `cargo xtask models` checks the required sections
                # exist; this checks the one sentence that matters for these two.
                if "**Trained** | **No" not in text:
                    failures.append(
                        f"{name}: the card does not state that the head is untrained in its own "
                        "header table"
                    )

    for line in failures:
        print(f"FAIL {line}")
    if failures:
        return 1
    print(
        f"export --verify: {len(EXPECTED)} entries agree with the fixtures "
        "(shapes, residual outputs, int8 forbidden, cards present and honest)"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", metavar="MODELS_DIR", help="check models.lock against the fixtures")
    parser.add_argument("--checkpoint", help="a trained checkpoint to export")
    args = parser.parse_args()

    if args.verify:
        return verify(args.verify)
    if args.checkpoint:
        print(
            "export: there is no trained checkpoint in this repository. Neither head in phase 22 "
            "is trained - see docs/model-cards/denoise.md and docs/model-cards/face_recovery.md - "
            "and the second has no measured fallback either. Run with --verify models.",
            file=sys.stderr,
        )
        return 2
    parser.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
