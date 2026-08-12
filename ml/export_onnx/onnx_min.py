"""The ONNX subset, written and read in pure Python.

This is the mirror image of ``crates/aura-infer/src/onnx/{wire,model}.rs``. It
exists so the export pipeline does not need PyTorch, ONNX or protobuf installed
to produce a file the application can load - and, more usefully, so that two
independent implementations of the same format can be compared against each
other. ``export.py --verify`` does exactly that: it rebuilds a model here and
checks the bytes against the file the Rust generator wrote.

Field numbers are from ``onnx.proto3`` and are written next to the fields they
belong to, the same way the Rust side does it.

No third-party imports. numpy is used only where the caller already has arrays.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field
from typing import Iterable

FLOAT = 1
INT64 = 7

IR_VERSION = 8
OPSET = 13

# AttributeProto.AttributeType
ATTR_FLOAT = 1
ATTR_INT = 2
ATTR_STRING = 3
ATTR_TENSOR = 4
ATTR_FLOATS = 6
ATTR_INTS = 7


def varint(value: int) -> bytes:
    """Encode one protobuf varint.

    Negative values are encoded as 64-bit two's complement, which is what
    protobuf does for a signed ``int64`` field and what the Rust writer produces.
    Without the mask a negative attribute - ``Softmax``'s ``axis = -1`` is the one
    that appears in practice - shifts towards -1 forever.
    """
    value &= (1 << 64) - 1
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def tag(field_number: int, wire_type: int) -> bytes:
    """Encode a field tag."""
    return varint((field_number << 3) | wire_type)


def length_delimited(field_number: int, payload: bytes) -> bytes:
    """Encode a length-prefixed field."""
    return tag(field_number, 2) + varint(len(payload)) + payload


def varint_field(field_number: int, value: int) -> bytes:
    """Encode a varint field."""
    return tag(field_number, 0) + varint(value)


def string_field(field_number: int, value: str) -> bytes:
    """Encode a UTF-8 string field."""
    return length_delimited(field_number, value.encode("utf-8"))


@dataclass
class Tensor:
    """A named constant: weights, biases, reshape targets."""

    name: str
    dims: list[int]
    data: list[float]
    data_type: int = FLOAT

    def encode(self) -> bytes:
        """TensorProto: dims = 1, data_type = 2, name = 8, raw_data = 9."""
        out = b"".join(varint_field(1, dim) for dim in self.dims)
        out += varint_field(2, self.data_type)
        out += string_field(8, self.name)
        if self.data_type == FLOAT:
            raw = b"".join(struct.pack("<f", value) for value in self.data)
        else:
            raw = b"".join(struct.pack("<q", int(value)) for value in self.data)
        out += length_delimited(9, raw)
        return out


@dataclass
class Attribute:
    """One node attribute."""

    name: str
    kind: int
    value: object

    def encode(self) -> bytes:
        """AttributeProto: name = 1, f = 2, i = 3, s = 4, ints = 8, type = 20."""
        out = string_field(1, self.name)
        if self.kind == ATTR_INT:
            out += varint_field(3, int(self.value))  # type: ignore[arg-type]
        elif self.kind == ATTR_FLOAT:
            out += tag(2, 5) + struct.pack("<f", float(self.value))  # type: ignore[arg-type]
        elif self.kind == ATTR_STRING:
            out += string_field(4, str(self.value))
        elif self.kind == ATTR_INTS:
            packed = b"".join(varint(int(item)) for item in self.value)  # type: ignore[union-attr]
            out += length_delimited(8, packed)
        else:
            raise ValueError(f"attribute kind {self.kind} is not in the subset")
        out += varint_field(20, self.kind)
        return out


@dataclass
class Node:
    """One operator invocation."""

    op_type: str
    name: str
    inputs: list[str]
    outputs: list[str]
    attributes: list[Attribute] = field(default_factory=list)

    def encode(self) -> bytes:
        """NodeProto: input = 1, output = 2, name = 3, op_type = 4, attribute = 5."""
        out = b"".join(string_field(1, name) for name in self.inputs)
        out += b"".join(string_field(2, name) for name in self.outputs)
        out += string_field(3, self.name)
        out += string_field(4, self.op_type)
        out += b"".join(length_delimited(5, attr.encode()) for attr in self.attributes)
        return out


@dataclass
class ValueInfo:
    """A declared graph input or output. ``None`` is a symbolic dimension."""

    name: str
    shape: list[int | None]

    def encode(self) -> bytes:
        """ValueInfoProto: name = 1, type = 2."""
        dims = b""
        for dim in self.shape:
            if dim is None:
                dims += length_delimited(1, string_field(2, "batch"))
            else:
                dims += length_delimited(1, varint_field(1, dim))
        tensor_type = varint_field(1, FLOAT) + length_delimited(2, dims)
        type_proto = length_delimited(1, tensor_type)
        return string_field(1, self.name) + length_delimited(2, type_proto)


@dataclass
class Graph:
    """A computation graph."""

    name: str
    nodes: list[Node]
    initializers: list[Tensor]
    inputs: list[ValueInfo]
    outputs: list[ValueInfo]

    def encode(self) -> bytes:
        """GraphProto: node = 1, name = 2, initializer = 5, input = 11, output = 12."""
        out = b"".join(length_delimited(1, node.encode()) for node in self.nodes)
        out += string_field(2, self.name)
        out += b"".join(length_delimited(5, tensor.encode()) for tensor in self.initializers)
        out += b"".join(length_delimited(11, info.encode()) for info in self.inputs)
        out += b"".join(length_delimited(12, info.encode()) for info in self.outputs)
        return out


@dataclass
class Model:
    """A whole model file."""

    graph: Graph
    producer_name: str = "aura-infer fixtures"
    ir_version: int = IR_VERSION
    opset: int = OPSET

    def encode(self) -> bytes:
        """ModelProto: ir_version = 1, producer_name = 2, graph = 7, opset_import = 8."""
        out = varint_field(1, self.ir_version)
        out += string_field(2, self.producer_name)
        out += length_delimited(7, self.graph.encode())
        opset = string_field(1, "") + varint_field(2, self.opset)
        out += length_delimited(8, opset)
        return out


class Reader:
    """Just enough of the reader to inspect a file's operators and weights."""

    def __init__(self, payload: bytes) -> None:
        self.payload = payload
        self.at = 0

    def done(self) -> bool:
        """True when every byte has been consumed."""
        return self.at >= len(self.payload)

    def varint(self) -> int:
        """Read one varint."""
        value = 0
        shift = 0
        while True:
            if self.at >= len(self.payload):
                raise ValueError("varint truncated")
            byte = self.payload[self.at]
            self.at += 1
            value |= (byte & 0x7F) << shift
            if not byte & 0x80:
                return value
            shift += 7

    def field(self) -> tuple[int, int]:
        """Read a field number and wire type."""
        key = self.varint()
        return key >> 3, key & 0x7

    def blob(self) -> bytes:
        """Read a length-delimited field."""
        length = self.varint()
        end = self.at + length
        if end > len(self.payload):
            raise ValueError("field overruns the message")
        out = self.payload[self.at : end]
        self.at = end
        return out

    def skip(self, wire_type: int) -> None:
        """Skip a field we do not read."""
        if wire_type == 0:
            self.varint()
        elif wire_type == 1:
            self.at += 8
        elif wire_type == 2:
            self.blob()
        elif wire_type == 5:
            self.at += 4
        else:
            raise ValueError(f"unsupported wire type {wire_type}")


def read_operators(payload: bytes) -> list[str]:
    """Every operator used by a model file, in graph order."""
    operators: list[str] = []
    for node in _graph_nodes(payload):
        reader = Reader(node)
        while not reader.done():
            number, wire = reader.field()
            if (number, wire) == (4, 2):
                operators.append(reader.blob().decode("utf-8"))
            else:
                reader.skip(wire)
    return operators


def read_opset(payload: bytes) -> int:
    """The default-domain opset a file declares."""
    reader = Reader(payload)
    opset = 0
    while not reader.done():
        number, wire = reader.field()
        if (number, wire) == (8, 2):
            inner = Reader(reader.blob())
            domain = ""
            version = 0
            while not inner.done():
                inner_number, inner_wire = inner.field()
                if (inner_number, inner_wire) == (1, 2):
                    domain = inner.blob().decode("utf-8")
                elif (inner_number, inner_wire) == (2, 0):
                    version = inner.varint()
                else:
                    inner.skip(inner_wire)
            if domain in ("", "ai.onnx"):
                opset = max(opset, version)
        else:
            reader.skip(wire)
    return opset


def read_initialisers(payload: bytes) -> dict[str, list[float]]:
    """Every float initialiser in a model file, by name."""
    weights: dict[str, list[float]] = {}
    for tensor in _graph_field(payload, 5):
        reader = Reader(tensor)
        name = ""
        raw = b""
        data_type = FLOAT
        while not reader.done():
            number, wire = reader.field()
            if (number, wire) == (2, 0):
                data_type = reader.varint()
            elif (number, wire) == (8, 2):
                name = reader.blob().decode("utf-8")
            elif (number, wire) == (9, 2):
                raw = reader.blob()
            else:
                reader.skip(wire)
        if data_type == FLOAT and raw:
            count = len(raw) // 4
            weights[name] = list(struct.unpack(f"<{count}f", raw))
    return weights


def rewrite_initialisers(payload: bytes, rewrite) -> bytes:
    """Return a copy of a model with every float initialiser passed through
    ``rewrite``.

    Sub-messages that are not touched are copied verbatim, so this preserves
    fields this reader does not understand. That matters: quantisation must not
    quietly discard something a future exporter put in the file.
    """
    reader = Reader(payload)
    out = bytearray()
    while not reader.done():
        number, wire = reader.field()
        if (number, wire) == (7, 2):
            out += length_delimited(7, _rewrite_graph(reader.blob(), rewrite))
        else:
            out += _copy_field(reader, number, wire)
    return bytes(out)


def _rewrite_graph(graph: bytes, rewrite) -> bytes:
    reader = Reader(graph)
    out = bytearray()
    while not reader.done():
        number, wire = reader.field()
        if (number, wire) == (5, 2):
            out += length_delimited(5, _rewrite_tensor(reader.blob(), rewrite))
        else:
            out += _copy_field(reader, number, wire)
    return bytes(out)


def _rewrite_tensor(tensor: bytes, rewrite) -> bytes:
    reader = Reader(tensor)
    fields: list[tuple[int, int, bytes]] = []
    data_type = FLOAT
    name = ""
    raw = b""
    while not reader.done():
        number, wire = reader.field()
        if (number, wire) == (2, 0):
            data_type = reader.varint()
            fields.append((2, 0, varint(data_type)))
        elif (number, wire) == (8, 2):
            name = reader.blob().decode("utf-8")
            fields.append((8, 2, name.encode("utf-8")))
        elif (number, wire) == (9, 2):
            raw = reader.blob()
            fields.append((9, 2, b""))
        else:
            fields.append((number, wire, _payload_of(reader, wire)))

    if data_type == FLOAT and raw:
        count = len(raw) // 4
        values = list(struct.unpack(f"<{count}f", raw))
        raw = b"".join(struct.pack("<f", value) for value in rewrite(name, values))

    out = bytearray()
    for number, wire, payload in fields:
        if (number, wire) == (9, 2):
            out += length_delimited(9, raw)
        elif wire == 2:
            out += length_delimited(number, payload)
        elif wire == 0:
            out += tag(number, 0) + payload
        else:
            out += tag(number, wire) + payload
    return bytes(out)


def _copy_field(reader: "Reader", number: int, wire: int) -> bytes:
    payload = _payload_of(reader, wire)
    if wire == 2:
        return length_delimited(number, payload)
    return tag(number, wire) + payload


def _payload_of(reader: "Reader", wire: int) -> bytes:
    if wire == 0:
        return varint(reader.varint())
    if wire == 2:
        return reader.blob()
    width = 8 if wire == 1 else 4
    out = reader.payload[reader.at : reader.at + width]
    reader.at += width
    return out


def _graph_nodes(payload: bytes) -> Iterable[bytes]:
    return _graph_field(payload, 1)


def _graph_field(payload: bytes, field_number: int) -> list[bytes]:
    reader = Reader(payload)
    graph = b""
    while not reader.done():
        number, wire = reader.field()
        if (number, wire) == (7, 2):
            graph = reader.blob()
        else:
            reader.skip(wire)

    found: list[bytes] = []
    inner = Reader(graph)
    while not inner.done():
        number, wire = inner.field()
        if number == field_number and wire == 2:
            found.append(inner.blob())
        else:
            inner.skip(wire)
    return found
