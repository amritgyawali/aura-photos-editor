"""Check that a model's precision variants agree with its full-precision file.

Section 6.4 of the phase document asks for a parity check across every available
execution provider, with a maximum absolute difference of 1e-3 for half precision
and 1e-2 for int8. That check needs a runtime per provider, so it exists in two
halves and this script is honest about which half it is running:

* **Weight-space parity** always runs. It reads the initialisers out of both
  files and compares them tensor by tensor. It catches the failure that actually
  happens in practice - a quantisation step that used the wrong scale, or an
  exporter that rounded a tensor it should have left alone - and it needs nothing
  installed.
* **Output-space parity** runs when ``onnxruntime`` is importable, feeding both
  files a fixed sample set and comparing the outputs.

The output-space check across the *runtime's own* precisions - which is what
ships - lives in Rust, in ``crates/aura-infer/tests/parity.rs``, and runs on
every commit. This script does not duplicate it; it checks the artefacts.

Usage::

    python ml/export_onnx/verify_parity.py models/x.fp32.onnx models/x.int8.onnx
    python ml/export_onnx/verify_parity.py --all models
"""

from __future__ import annotations

import argparse
import pathlib
import sys

import numpy as np

from onnx_min import read_initialisers, read_operators, read_opset, OPSET

TOLERANCE = {"fp16": 1e-3, "int8": 1e-2, "fp32": 0.0}

SAMPLES = 8
INPUT_SIDE = 32


def precision_of(path: pathlib.Path) -> str:
    """The precision a file name declares."""
    for precision in TOLERANCE:
        if f".{precision}." in path.name:
            return precision
    return "fp32"


def sample_batch() -> np.ndarray:
    """The same deterministic input the Rust fixtures use."""
    batch = np.zeros((SAMPLES, 3, INPUT_SIDE, INPUT_SIDE), dtype=np.float32)
    for image in range(SAMPLES):
        for channel in range(3):
            for y in range(INPUT_SIDE):
                for x in range(INPUT_SIDE):
                    across = x / INPUT_SIDE
                    down = y / INPUT_SIDE
                    value = (across + down) * 0.5 + channel * 0.13 + image * 0.07
                    batch[image, channel, y, x] = value % 1.0
    return batch


def compare_weights(reference: pathlib.Path, variant: pathlib.Path) -> int:
    """Compare initialisers, returning the number of problems."""
    left = read_initialisers(reference.read_bytes())
    right = read_initialisers(variant.read_bytes())
    tolerance = TOLERANCE[precision_of(variant)]

    problems = 0
    if set(left) != set(right):
        print(f"{variant.name}: tensor names differ from {reference.name}")
        return 1

    worst_overall = 0.0
    for name, values in left.items():
        a = np.array(values, dtype=np.float32)
        b = np.array(right[name], dtype=np.float32)
        if a.shape != b.shape:
            print(f"{variant.name}: {name} has {b.size} weights against {a.size}")
            problems += 1
            continue
        worst = float(np.max(np.abs(a - b))) if a.size else 0.0
        worst_overall = max(worst_overall, worst)
        if worst > tolerance:
            print(f"{variant.name}: {name} drifts by {worst:.3e}, tolerance {tolerance:.0e}")
            problems += 1

    print(
        f"{variant.name}: {len(left)} tensors, worst weight drift {worst_overall:.3e}, "
        f"tolerance {tolerance:.0e}"
    )
    return problems


def compare_outputs(reference: pathlib.Path, variant: pathlib.Path) -> int:
    """Compare outputs when onnxruntime is available."""
    try:
        import onnxruntime  # noqa: PLC0415  (optional dependency, imported on demand)
    except ImportError:
        print(
            "onnxruntime is not installed, so output-space parity was not run here. "
            "The shipping runtime's own parity suite is `cargo test -p aura-infer --test parity`."
        )
        return 0

    batch = sample_batch()
    sessions = [onnxruntime.InferenceSession(str(path)) for path in (reference, variant)]
    outputs = [
        session.run(None, {session.get_inputs()[0].name: batch})[0] for session in sessions
    ]
    worst = float(np.max(np.abs(outputs[0] - outputs[1])))
    tolerance = TOLERANCE[precision_of(variant)]
    print(f"{variant.name}: worst output drift {worst:.3e}, tolerance {tolerance:.0e}")
    return 0 if worst <= tolerance else 1


def compare_against_runtime(model: pathlib.Path, precision: str) -> int:
    """Compare AURA's own interpreter against ONNX Runtime on the same file.

    This is the only independent check of the interpreter that exists anywhere in
    the project: everything else compares our code against our own encoder. ONNX
    Runtime is not linked into the application - see ADR-0007 for why - but when
    it happens to be installed for Python, it is an oracle worth using.

    Absent onnxruntime the check is skipped and says so, because a check that
    silently does nothing is worse than no check.
    """
    try:
        import onnxruntime  # noqa: PLC0415  (optional dependency, imported on demand)
    except ImportError:
        print("onnxruntime is not installed; the cross-runtime check was skipped")
        return 0

    import json
    import subprocess

    batch = sample_batch()
    session = onnxruntime.InferenceSession(str(model))
    theirs = session.run(None, {session.get_inputs()[0].name: batch})[0]

    completed = subprocess.run(
        [
            "cargo",
            "run",
            "--release",
            "--quiet",
            "-p",
            "aura-cli",
            "--",
            "infer",
            "--model",
            str(model),
            "--precision",
            precision,
            "--batch",
            str(SAMPLES),
        ],
        capture_output=True,
        text=True,
        check=False,
        cwd=str(pathlib.Path(__file__).resolve().parents[2]),
    )
    if completed.returncode != 0:
        print(f"aura-cli infer failed: {completed.stderr.strip().splitlines()[-1:] }")
        return 1

    payload = json.loads(completed.stdout.strip().splitlines()[-1])
    ours = np.array(payload["outputs"][0]["data"], dtype=np.float32).reshape(
        payload["outputs"][0]["shape"]
    )

    if ours.shape != theirs.shape:
        print(f"{model.name}: shapes differ, {ours.shape} against {theirs.shape}")
        return 1
    worst = float(np.max(np.abs(ours - theirs)))
    tolerance = TOLERANCE["fp16"] if precision == "fp32" else TOLERANCE[precision]
    print(
        f"{model.name}: AURA against onnxruntime at {precision}, worst {worst:.3e}, "
        f"tolerance {tolerance:.0e}"
    )
    return 0 if worst <= tolerance else 1


def check_subset(path: pathlib.Path) -> int:
    """Refuse a file that uses something the runtime cannot execute."""
    payload = path.read_bytes()
    opset = read_opset(payload)
    if opset > OPSET:
        print(f"{path.name}: opset {opset} is above the runtime's {OPSET}")
        return 1
    print(f"{path.name}: {len(read_operators(payload))} nodes, opset {opset}")
    return 0


def verify_pair(reference: pathlib.Path, variant: pathlib.Path) -> int:
    """Run every check on one reference and one variant."""
    problems = check_subset(reference) + check_subset(variant)
    problems += compare_weights(reference, variant)
    problems += compare_outputs(reference, variant)
    return problems


def verify_directory(directory: pathlib.Path) -> int:
    """Check every variant in a models directory against its fp32 file."""
    problems = 0
    pairs = 0
    for reference in sorted(directory.glob("*.fp32.onnx")):
        for precision in ("fp16", "int8"):
            variant = reference.with_name(reference.name.replace(".fp32.", f".{precision}."))
            if variant.exists():
                pairs += 1
                problems += verify_pair(reference, variant)
    if pairs == 0:
        print(f"no fp32/variant pairs found in {directory}")
        return 1
    print(f"parity: {pairs} pairs checked, {problems} problems")
    return problems


def main() -> int:
    """Command line entry point."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=pathlib.Path, nargs="?")
    parser.add_argument("variant", type=pathlib.Path, nargs="?")
    parser.add_argument("--all", type=pathlib.Path, help="check every pair in a directory")
    parser.add_argument(
        "--against-runtime",
        type=pathlib.Path,
        help="compare AURA's interpreter against onnxruntime on one file",
    )
    parser.add_argument("--precision", default="fp32", choices=sorted(TOLERANCE))
    args = parser.parse_args()

    if args.against_runtime:
        return 1 if compare_against_runtime(args.against_runtime, args.precision) else 0
    if args.all:
        return 1 if verify_directory(args.all) else 0
    if args.reference and args.variant:
        return 1 if verify_pair(args.reference, args.variant) else 0

    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
