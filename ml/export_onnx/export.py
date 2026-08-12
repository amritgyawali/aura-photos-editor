"""Export a model to the ONNX subset AURA can run, and prove it is that subset.

Two modes, because this phase has no trained weights yet and the phases that do
will not have them for months:

* ``--placeholder`` rebuilds the two placeholder models, weight for weight, from
  the same seeded generator the Rust fixtures use. With ``--verify`` it compares
  its output against the file already in ``models/`` and fails on any difference.
  Two independent implementations of one file format agreeing byte for byte is
  the strongest check available here, and it is the same argument phase 02 made
  for shipping an encoder beside every decoder.
* ``--torch <module>:<factory>`` exports a real PyTorch model when PyTorch is
  installed. That path is not exercised in this repository yet - nothing here
  trains anything - so it stays deliberately small and it says so when it runs.

Every export ends with the subset check: the operator list is read back out of
the written file and compared against the operators the runtime implements,
which are parsed from the Rust source so the two lists cannot drift.

Usage::

    python ml/export_onnx/export.py --placeholder --out models
    python ml/export_onnx/export.py --placeholder --verify models
    python ml/export_onnx/export.py --check models/aura_tiny_scene_1.0.0.fp32.onnx
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

import numpy as np

from onnx_min import (
    ATTR_INT,
    ATTR_INTS,
    Attribute,
    Graph,
    Model,
    Node,
    Tensor,
    ValueInfo,
    read_operators,
    read_opset,
    OPSET,
)

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
OPS_SOURCE = REPO_ROOT / "crates/aura-infer/src/onnx/ops/mod.rs"

INPUT_SIDE = 32
EMBEDDING_DIM = 32
SCENE_CLASSES = 6

EMBEDDING_SEED = 0x00A0_1234_5678_9ABC
SCENE_SEED = 0x00B0_9876_5432_1FED

U64 = (1 << 64) - 1


class Weights:
    """xorshift64*, the same generator as ``onnx::fixtures`` in Rust.

    The arithmetic is done in 64-bit integers and the scaling in float32, in the
    same order as the Rust code, so both produce identical bytes. A generator
    from a library would not: they change between releases, and a model file
    whose bytes move is a `models.lock` that stops verifying everywhere at once.
    """

    def __init__(self, seed: int) -> None:
        self.state = seed | 1

    def next_bits(self) -> int:
        """One 64-bit sample."""
        state = self.state
        state ^= (state << 13) & U64
        state ^= state >> 7
        state ^= (state << 17) & U64
        self.state = state
        return (state * 0x2545_F491_4F6C_DD1D) & U64

    def next(self, scale: float) -> float:
        """One weight in ``-scale..scale``, rounded exactly as float32."""
        unit = np.float32(self.next_bits() >> 40) / np.float32(16_777_216.0)
        return float((unit * np.float32(2.0) - np.float32(1.0)) * np.float32(scale))

    def tensor(self, name: str, dims: list[int], scale: float) -> Tensor:
        """A weight tensor of the given shape."""
        count = int(np.prod(dims))
        return Tensor(name=name, dims=dims, data=[self.next(scale) for _ in range(count)])


def ints(name: str, values: list[int]) -> Attribute:
    """An integer-list attribute."""
    return Attribute(name=name, kind=ATTR_INTS, value=values)


def integer(name: str, value: int) -> Attribute:
    """An integer attribute."""
    return Attribute(name=name, kind=ATTR_INT, value=value)


def dynamic_batch(name: str, rest: list[int]) -> ValueInfo:
    """A value whose leading dimension is the batch."""
    return ValueInfo(name=name, shape=[None, *rest])


def tiny_embedding() -> Model:
    """The embedding placeholder: two convolutions, a pool and a linear head."""
    weights = Weights(EMBEDDING_SEED)
    graph = Graph(
        name="aura_tiny_embedding",
        nodes=[
            Node("Conv", "conv1", ["pixels", "conv1_w", "conv1_b"], ["conv1_out"],
                 [ints("kernel_shape", [3, 3]), ints("pads", [1, 1, 1, 1]), ints("strides", [1, 1])]),
            Node("Relu", "relu1", ["conv1_out"], ["relu1_out"]),
            Node("MaxPool", "pool1", ["relu1_out"], ["pool1_out"],
                 [ints("kernel_shape", [2, 2]), ints("strides", [2, 2])]),
            Node("Conv", "conv2", ["pool1_out", "conv2_w", "conv2_b"], ["conv2_out"],
                 [ints("kernel_shape", [3, 3]), ints("pads", [1, 1, 1, 1]), ints("strides", [1, 1])]),
            Node("Relu", "relu2", ["conv2_out"], ["relu2_out"]),
            Node("GlobalAveragePool", "gap", ["relu2_out"], ["gap_out"]),
            Node("Flatten", "flatten", ["gap_out"], ["flat_out"], [integer("axis", 1)]),
            Node("Gemm", "head", ["flat_out", "head_w", "head_b"], ["embedding"],
                 [integer("transB", 1)]),
        ],
        initializers=[
            weights.tensor("conv1_w", [8, 3, 3, 3], 0.30),
            weights.tensor("conv1_b", [8], 0.05),
            weights.tensor("conv2_w", [16, 8, 3, 3], 0.20),
            weights.tensor("conv2_b", [16], 0.05),
            weights.tensor("head_w", [EMBEDDING_DIM, 16], 0.35),
            weights.tensor("head_b", [EMBEDDING_DIM], 0.05),
        ],
        inputs=[dynamic_batch("pixels", [3, INPUT_SIDE, INPUT_SIDE])],
        outputs=[dynamic_batch("embedding", [EMBEDDING_DIM])],
    )
    return Model(graph=graph)


def tiny_scene() -> Model:
    """The classifier placeholder, ending in a softmax so it returns confidences."""
    weights = Weights(SCENE_SEED)
    graph = Graph(
        name="aura_tiny_scene",
        nodes=[
            Node("Conv", "stem", ["pixels", "stem_w", "stem_b"], ["stem_out"],
                 [ints("kernel_shape", [3, 3]), ints("pads", [1, 1, 1, 1]), ints("strides", [2, 2])]),
            Node("Relu", "act", ["stem_out"], ["act_out"]),
            Node("GlobalAveragePool", "gap", ["act_out"], ["gap_out"]),
            Node("Flatten", "flatten", ["gap_out"], ["flat_out"], [integer("axis", 1)]),
            Node("Gemm", "head", ["flat_out", "head_w", "head_b"], ["logits"],
                 [integer("transB", 1)]),
            Node("Softmax", "softmax", ["logits"], ["scene_probs"], [integer("axis", -1)]),
        ],
        initializers=[
            weights.tensor("stem_w", [8, 3, 3, 3], 0.30),
            weights.tensor("stem_b", [8], 0.05),
            weights.tensor("head_w", [SCENE_CLASSES, 8], 0.40),
            weights.tensor("head_b", [SCENE_CLASSES], 0.05),
        ],
        inputs=[dynamic_batch("pixels", [3, INPUT_SIDE, INPUT_SIDE])],
        outputs=[dynamic_batch("scene_probs", [SCENE_CLASSES])],
    )
    return Model(graph=graph)


PLACEHOLDERS = {
    "aura_tiny_embedding_1.0.0.fp32.onnx": tiny_embedding,
    "aura_tiny_scene_1.0.0.fp32.onnx": tiny_scene,
}


def supported_operators() -> list[str]:
    """The operator subset, read from the Rust source that implements it."""
    source = OPS_SOURCE.read_text(encoding="utf-8")
    match = re.search(r"pub const SUPPORTED: &\[&str\] = &\[(.*?)\];", source, re.S)
    if not match:
        raise SystemExit(f"could not find the operator list in {OPS_SOURCE}")
    return re.findall(r'"([A-Za-z]+)"', match.group(1))


def check_subset(path: pathlib.Path) -> int:
    """Fail unless every operator in a file is one the runtime implements."""
    payload = path.read_bytes()
    allowed = set(supported_operators())
    used = read_operators(payload)
    opset = read_opset(payload)

    problems = 0
    for operator in sorted(set(used)):
        if operator not in allowed:
            print(f"{path.name}: operator {operator} is outside the runtime subset")
            problems += 1
    if opset > OPSET:
        print(f"{path.name}: opset {opset} is above the runtime's {OPSET}")
        problems += 1
    if problems == 0:
        print(f"{path.name}: {len(used)} nodes, opset {opset}, all inside the subset")
    return problems


def write_placeholders(out: pathlib.Path) -> int:
    """Write both placeholder models."""
    out.mkdir(parents=True, exist_ok=True)
    problems = 0
    for name, factory in PLACEHOLDERS.items():
        path = out / name
        path.write_bytes(factory().encode())
        problems += check_subset(path)
    return problems


def verify_placeholders(directory: pathlib.Path) -> int:
    """Compare this exporter's bytes against the files already on disk."""
    problems = 0
    for name, factory in PLACEHOLDERS.items():
        path = directory / name
        if not path.exists():
            print(f"{name}: missing from {directory}")
            problems += 1
            continue
        ours = factory().encode()
        theirs = path.read_bytes()
        if ours == theirs:
            print(f"{name}: {len(ours)} bytes, identical to the Rust generator")
        else:
            print(
                f"{name}: {len(ours)} bytes here against {len(theirs)} on disk - "
                "the two implementations of the format disagree"
            )
            problems += 1
    return problems


def export_torch(spec: str, out: pathlib.Path) -> int:
    """Export a PyTorch model, when PyTorch is installed."""
    try:
        import torch  # noqa: PLC0415  (optional dependency, imported on demand)
    except ImportError:
        print(
            "PyTorch is not installed. Nothing in this repository trains a model yet; "
            "this path becomes live in phase 05."
        )
        return 1

    module_name, _, factory_name = spec.partition(":")
    module = __import__(module_name, fromlist=[factory_name])
    model = getattr(module, factory_name)()
    model.eval()

    out.parent.mkdir(parents=True, exist_ok=True)
    example = torch.zeros(1, 3, INPUT_SIDE, INPUT_SIDE)
    torch.onnx.export(
        model,
        example,
        str(out),
        opset_version=OPSET,
        input_names=["pixels"],
        output_names=["output"],
        dynamic_axes={"pixels": {0: "batch"}, "output": {0: "batch"}},
    )
    return check_subset(out)


def main() -> int:
    """Command line entry point."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--placeholder", action="store_true", help="build the placeholder models")
    parser.add_argument("--out", type=pathlib.Path, default=REPO_ROOT / "models")
    parser.add_argument("--verify", type=pathlib.Path, help="compare against files in this directory")
    parser.add_argument("--check", type=pathlib.Path, help="check one file against the subset")
    parser.add_argument("--torch", help="module:factory of a PyTorch model to export")
    args = parser.parse_args()

    if args.check:
        return 1 if check_subset(args.check) else 0
    if args.verify:
        return 1 if verify_placeholders(args.verify) else 0
    if args.torch:
        return 1 if export_torch(args.torch, args.out) else 0
    if args.placeholder:
        return 1 if write_placeholders(args.out) else 0

    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
