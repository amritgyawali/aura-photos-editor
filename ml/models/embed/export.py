"""Export the wedding embedding model, and refuse to export one AURA cannot run.

Section 6.1 asks for a *fused* export - resize, normalise, backbone, head in one
graph - "so preprocessing can never drift between training and inference". That is
the right requirement and it cannot be met literally today: the runtime is a
deterministic pure-Rust interpreter over a documented subset of ONNX opset 13
(``docs/adr/ADR-0007-inference-runtime.md``) and the subset has no ``Resize``, no
``LayerNormalization`` and no attention primitives.

``docs/adr/ADR-0011-embeddings-and-similarity-index.md`` section 4 records what
replaces it: the preprocessing steps are constants of
``crates/aura-vision/src/embed/model.rs``, and every stored row carries
``PREPROCESS_VER``. This script's job is to make that arrangement enforceable
rather than aspirational, so it does three things:

* ``--placeholder`` rebuilds ``wedding_embedding`` 1.0.0 from the same seeded
  generator the Rust fixture uses, and with ``--verify`` compares the bytes against
  the file in ``models/``. Two independent implementations of one file format
  agreeing byte for byte is the strongest check available here, and it is the same
  argument phase 02 made for shipping an encoder beside every decoder.
* ``--check`` reads any ONNX file's operator list back out and fails on any
  operator the runtime does not implement - with the runtime's list parsed from the
  Rust source, so the two cannot drift.
* ``--manifest-entry`` prints the ``models.lock`` entry a real export would need,
  including the input and output shapes the Rust loader validates against, so that
  the day a head is trained the manifest is not hand-written.

Usage::

    python ml/models/embed/export.py --placeholder --out models
    python ml/models/embed/export.py --placeholder --verify models
    python ml/models/embed/export.py --check models/wedding_embedding_1.0.0.fp32.onnx
    python ml/models/embed/export.py --manifest-entry
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

import numpy as np

_HERE = pathlib.Path(__file__).resolve()
_REPO = _HERE.parents[3]
sys.path.insert(0, str(_REPO / "ml" / "export_onnx"))

from onnx_min import (  # noqa: E402  - path shim above
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
)

# --------------------------------------------------------------------------
# The shape, copied from the phase document and from the Rust fixture
# --------------------------------------------------------------------------

MODEL_NAME = "wedding_embedding"
MODEL_VERSION = "1.0.0"
INPUT_SIDE = 384
EMBED_DIM = 512
TRUNK_DIM = 256
OPSET = 13
IR_VERSION = 8
#: The seed the Rust fixture uses. Two implementations, one constant, byte-identical
#: files - which is the only reason this script can verify anything.
SEED = 0x00C0_0505_1234_5678


class Weights:
    """xorshift64*, identical to ``crates/aura-infer/src/onnx/fixtures.rs``.

    Not ``numpy.random``: a model file whose bytes changed when a dependency updated
    its generator would break ``models.lock`` on every machine at once.
    """

    def __init__(self, seed: int) -> None:
        self.state = (seed | 1) & 0xFFFFFFFFFFFFFFFF

    def next_bits(self) -> int:
        state = self.state
        state ^= (state << 13) & 0xFFFFFFFFFFFFFFFF
        state ^= state >> 7
        state ^= (state << 17) & 0xFFFFFFFFFFFFFFFF
        self.state = state
        return (state * 0x2545F4914F6CDD1D) & 0xFFFFFFFFFFFFFFFF

    def next(self, scale: float) -> float:
        """One weight in ``-scale..scale``, rounded exactly as float32.

        Every step is float32 and in the same order as the Rust code. Doing the
        arithmetic in double precision and rounding once at the end gives a
        different last bit on about one weight in a thousand, which is enough to
        move the file's digest and therefore enough to make this whole check useless.
        """
        unit = np.float32(self.next_bits() >> 40) / np.float32(16_777_216.0)
        return float((unit * np.float32(2.0) - np.float32(1.0)) * np.float32(scale))

    def tensor(self, name: str, dims: list[int], scale: float) -> Tensor:
        count = 1
        for dim in dims:
            count *= dim
        return Tensor(name=name, dims=dims, data=[self.next(scale) for _ in range(count)])


def _node(op_type: str, name: str, inputs: list[str], outputs: list[str], attributes: list[Attribute] | None = None) -> Node:
    return Node(op_type=op_type, name=name, inputs=inputs, outputs=outputs, attributes=attributes or [])


def _conv(name: str, inputs: list[str], outputs: list[str], stride: int) -> Node:
    return _node(
        "Conv",
        name,
        inputs,
        outputs,
        [
            Attribute("kernel_shape", ATTR_INTS, [3, 3]),
            Attribute("pads", ATTR_INTS, [1, 1, 1, 1]),
            Attribute("strides", ATTR_INTS, [stride, stride]),
        ],
    )


def build_placeholder() -> Model:
    """The graph the Rust fixture builds, node for node and weight for weight."""
    weights = Weights(SEED)

    nodes = [
        _conv("stem", ["pixels", "stem_w", "stem_b"], ["stem_out"], 4),
        _node("Relu", "stem_act", ["stem_out"], ["stem_relu"]),
        _node(
            "MaxPool",
            "pool",
            ["stem_relu"],
            ["pool_out"],
            [
                Attribute("kernel_shape", ATTR_INTS, [2, 2]),
                Attribute("strides", ATTR_INTS, [2, 2]),
            ],
        ),
        _conv("block1", ["pool_out", "block1_w", "block1_b"], ["block1_out"], 2),
        _node("Relu", "block1_act", ["block1_out"], ["block1_relu"]),
        _conv("block2", ["block1_relu", "block2_w", "block2_b"], ["block2_out"], 2),
        _node("Relu", "block2_act", ["block2_out"], ["block2_relu"]),
        _node("GlobalAveragePool", "gap", ["block2_relu"], ["gap_out"]),
        _node("Flatten", "flatten", ["gap_out"], ["features"], [Attribute("axis", ATTR_INT, 1)]),
        _node(
            "Gemm",
            "trunk",
            ["features", "trunk_w", "trunk_b"],
            ["trunk_out"],
            [Attribute("transB", ATTR_INT, 1)],
        ),
        _node("Relu", "trunk_act", ["trunk_out"], ["trunk_relu"]),
        _node(
            "Gemm",
            "project",
            ["trunk_relu", "project_w", "project_b"],
            ["embedding"],
            [Attribute("transB", ATTR_INT, 1)],
        ),
    ]

    initializers = [
        weights.tensor("stem_w", [32, 3, 3, 3], 0.25),
        weights.tensor("stem_b", [32], 0.05),
        weights.tensor("block1_w", [64, 32, 3, 3], 0.12),
        weights.tensor("block1_b", [64], 0.05),
        weights.tensor("block2_w", [96, 64, 3, 3], 0.10),
        weights.tensor("block2_b", [96], 0.05),
        weights.tensor("trunk_w", [TRUNK_DIM, 96], 0.18),
        weights.tensor("trunk_b", [TRUNK_DIM], 0.05),
        weights.tensor("project_w", [EMBED_DIM, TRUNK_DIM], 0.14),
        weights.tensor("project_b", [EMBED_DIM], 0.05),
    ]

    graph = Graph(
        name=MODEL_NAME,
        nodes=nodes,
        initializers=initializers,
        inputs=[ValueInfo("pixels", [None, 3, INPUT_SIDE, INPUT_SIDE])],
        outputs=[ValueInfo("embedding", [None, EMBED_DIM])],
    )
    return Model(graph=graph, producer_name="aura-infer fixtures", ir_version=IR_VERSION, opset=OPSET)


# --------------------------------------------------------------------------
# The subset check
# --------------------------------------------------------------------------


def runtime_operators() -> set[str]:
    """The operators the Rust runtime implements, parsed from its source.

    Reading them out of the source rather than restating them here is the whole
    point: a list in two places is a list that will disagree.
    """
    source = (_REPO / "crates" / "aura-infer" / "src" / "onnx" / "ops" / "mod.rs").read_text(encoding="utf-8")
    match = re.search(r"pub const SUPPORTED: &\[&str\] = &\[(.*?)\];", source, re.S)
    if match is None:
        raise SystemExit("could not find SUPPORTED in the runtime source; the parser needs updating")
    return set(re.findall(r'"([A-Za-z0-9]+)"', match.group(1)))


def check_subset(path: pathlib.Path) -> int:
    """Fail on any operator the runtime cannot execute, or an opset it cannot read."""
    payload = path.read_bytes()
    supported = runtime_operators()
    used = set(read_operators(payload))
    unsupported = sorted(used - supported)
    declared_opset = read_opset(payload)

    problems = 0
    if declared_opset > OPSET:
        print(f"{path.name} needs opset {declared_opset}; the runtime implements {OPSET}", file=sys.stderr)
        problems += 1
    for operator in unsupported:
        print(f"{path.name} uses {operator}, which the runtime does not implement", file=sys.stderr)
        problems += 1

    if problems:
        return 1
    print(f"{path.name}: {len(used)} operator kinds, all inside the runtime subset")
    return 0


# --------------------------------------------------------------------------
# The manifest entry
# --------------------------------------------------------------------------


def manifest_entry() -> dict[str, object]:
    """The ``models.lock`` entry shape, so nobody hand-writes one.

    ``variants`` is left empty: the digests belong to whichever files an export
    actually produced, and inventing them here would be inventing the one field the
    loader trusts.
    """
    return {
        "name": MODEL_NAME,
        "version": MODEL_VERSION,
        "task": "embedding",
        "class": "embedding",
        "input": {
            "shape": [1, 3, INPUT_SIDE, INPUT_SIDE],
            "layout": "NCHW",
            "range": "0..1",
            "colour": "srgb",
        },
        "output": {"embedding": [1, EMBED_DIM]},
        "variants": [],
        "licence": "proprietary",
        "model_card": f"docs/model-cards/{MODEL_NAME}.md",
        "min_app_version": "0.1.0",
        "working_set_mb": 16,
        "precision_policy": {"forbid_int8": False, "require_fp32": False},
        "opset": OPSET,
        "_notes": [
            "L2 normalisation is applied outside the graph, in",
            "crates/aura-vision/src/embed/model.rs, because the runtime subset has no",
            "reduction operator. It is covered by PREPROCESS_VER; see ADR-0011 section 4.",
            "Preprocessing - centre crop, box resize to 384, NCHW, divide by 255 - is",
            "likewise outside the graph and likewise versioned.",
        ],
    }


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--placeholder", action="store_true", help="rebuild the placeholder graph")
    parser.add_argument("--out", type=pathlib.Path, help="write the placeholder into this directory")
    parser.add_argument("--verify", type=pathlib.Path, help="compare the placeholder against the file in this directory")
    parser.add_argument("--check", type=pathlib.Path, help="check one ONNX file against the runtime subset")
    parser.add_argument("--manifest-entry", action="store_true", help="print the models.lock entry shape")
    args = parser.parse_args(argv)

    if args.manifest_entry:
        print(json.dumps(manifest_entry(), indent=2, sort_keys=False))
        return 0

    if args.check is not None:
        return check_subset(args.check)

    if not args.placeholder:
        parser.error("give --placeholder, --check or --manifest-entry")

    model = build_placeholder()
    encoded = model.encode()

    if args.verify is not None:
        target = args.verify / f"{MODEL_NAME}_{MODEL_VERSION}.fp32.onnx"
        if not target.exists():
            print(f"{target} does not exist; run `cargo xtask models --generate` first", file=sys.stderr)
            return 1
        existing = target.read_bytes()
        if existing != encoded:
            print(
                f"MISMATCH: this exporter produced {len(encoded)} bytes and {target.name} is "
                f"{len(existing)}.\nTwo implementations of one file format have drifted; the "
                "graph, the seed or the writer differs.",
                file=sys.stderr,
            )
            return 1
        print(f"{target.name}: {len(encoded)} bytes, byte-identical to the Rust fixture")
        return 0

    if args.out is None:
        parser.error("--placeholder needs --out or --verify")
    args.out.mkdir(parents=True, exist_ok=True)
    written = args.out / f"{MODEL_NAME}_{MODEL_VERSION}.fp32.onnx"
    written.write_bytes(encoded)
    print(f"wrote {written} ({len(encoded)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
