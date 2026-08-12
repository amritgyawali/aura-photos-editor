//! The ONNX messages this runtime understands, and nothing else.
//!
//! Nine messages out of the schema's several dozen: `ModelProto`,
//! `OperatorSetIdProto`, `GraphProto`, `NodeProto`, `AttributeProto`,
//! `TensorProto`, `ValueInfoProto`, `TypeProto` and `TensorShapeProto`. Field
//! numbers are from `onnx.proto3` and are the load-bearing constants in this
//! file, so each one is written next to the field it belongs to.
//!
//! Unknown fields are skipped, unknown *operators* are refused. That asymmetry is
//! deliberate: a `doc_string` we ignore changes no number, and an operator we
//! ignore changes every number downstream of it.

use aura_core::AuraResult;

use crate::errors::malformed;
use crate::onnx::wire::{packed_floats, packed_varints, Reader, WireType, Writer};

/// ONNX tensor element types, from `TensorProto.DataType`.
mod data_type {
    pub(super) const FLOAT: i64 = 1;
    pub(super) const UINT8: i64 = 2;
    pub(super) const INT8: i64 = 3;
    pub(super) const INT32: i64 = 6;
    pub(super) const INT64: i64 = 7;
}

/// The bytes every ONNX file this build writes starts as: IR version 8, which is
/// the version paired with opset 13 in the ONNX release matrix.
pub const IR_VERSION: i64 = 8;

/// The opset this runtime implements. See `docs/adr/ADR-0007-inference-runtime.md`.
pub const OPSET: i64 = 13;

/// A whole model file.
#[derive(Debug, Clone, PartialEq)]
pub struct OnnxModel {
    /// ONNX IR version.
    pub ir_version: i64,
    /// Who wrote the file.
    pub producer_name: String,
    /// Default-domain opset version.
    pub opset: i64,
    /// The single graph. Subgraphs (`If`, `Loop`) are not supported.
    pub graph: Graph,
}

/// A computation graph.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Graph {
    /// Graph name, for diagnostics.
    pub name: String,
    /// Nodes in the order the producer wrote them.
    pub nodes: Vec<Node>,
    /// Constant tensors: weights, biases, reshape targets.
    pub initializers: Vec<InitTensor>,
    /// Declared inputs, including those satisfied by initialisers.
    pub inputs: Vec<ValueInfo>,
    /// Declared outputs.
    pub outputs: Vec<ValueInfo>,
}

/// One operator invocation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    /// Node name, for diagnostics. May be empty.
    pub name: String,
    /// Operator, e.g. `Conv`.
    pub op_type: String,
    /// Input value names, in the operator's declared order.
    pub inputs: Vec<String>,
    /// Output value names.
    pub outputs: Vec<String>,
    /// Attributes, in file order.
    pub attributes: Vec<Attribute>,
}

impl Node {
    /// Look up an attribute by name.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&AttrValue> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }

    /// An integer attribute with a default.
    #[must_use]
    pub fn attr_int(&self, name: &str, default: i64) -> i64 {
        match self.attr(name) {
            Some(AttrValue::Int(value)) => *value,
            _ => default,
        }
    }

    /// A float attribute with a default.
    #[must_use]
    pub fn attr_float(&self, name: &str, default: f32) -> f32 {
        match self.attr(name) {
            Some(AttrValue::Float(value)) => *value,
            _ => default,
        }
    }

    /// An integer-list attribute, empty when absent.
    #[must_use]
    pub fn attr_ints(&self, name: &str) -> Vec<i64> {
        match self.attr(name) {
            Some(AttrValue::Ints(values)) => values.clone(),
            Some(AttrValue::Int(value)) => vec![*value],
            _ => Vec::new(),
        }
    }

    /// The input name at a position, when the model provided one.
    ///
    /// ONNX marks a skipped optional input with an empty name, so an empty string
    /// is the same as absent and both answer `None`.
    #[must_use]
    pub fn input_at(&self, index: usize) -> Option<&str> {
        match self.inputs.get(index) {
            Some(name) if !name.is_empty() => Some(name.as_str()),
            _ => None,
        }
    }
}

/// A named attribute value.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// Attribute name.
    pub name: String,
    /// Attribute value.
    pub value: AttrValue,
}

/// The attribute kinds this runtime reads.
#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    /// Single float, e.g. `epsilon`.
    Float(f32),
    /// Single integer, e.g. `axis`.
    Int(i64),
    /// Integer list, e.g. `kernel_shape`.
    Ints(Vec<i64>),
    /// Float list.
    Floats(Vec<f32>),
    /// String, e.g. `auto_pad`.
    Str(String),
    /// Constant tensor, used by `Constant`.
    Tensor(InitTensor),
}

/// A constant tensor from the graph's initialiser list.
#[derive(Debug, Clone, PartialEq)]
pub struct InitTensor {
    /// Value name this tensor satisfies.
    pub name: String,
    /// Dimensions, outermost first.
    pub dims: Vec<usize>,
    /// The samples, in whatever type the producer wrote.
    pub data: TensorData,
}

impl InitTensor {
    /// Samples as `f32`, converting integer types.
    #[must_use]
    pub fn to_f32(&self) -> Vec<f32> {
        match &self.data {
            TensorData::Float(values) => values.clone(),
            TensorData::Int64(values) => values.iter().map(|v| *v as f32).collect(),
            TensorData::Int8(values) => values.iter().map(|v| f32::from(*v)).collect(),
            TensorData::Uint8(values) => values.iter().map(|v| f32::from(*v)).collect(),
            TensorData::Int32(values) => values.iter().map(|v| *v as f32).collect(),
        }
    }

    /// Samples as `i64`, for shape and axis tensors.
    #[must_use]
    pub fn to_i64(&self) -> Vec<i64> {
        match &self.data {
            TensorData::Float(values) => values.iter().map(|v| *v as i64).collect(),
            TensorData::Int64(values) => values.clone(),
            TensorData::Int8(values) => values.iter().map(|v| i64::from(*v)).collect(),
            TensorData::Uint8(values) => values.iter().map(|v| i64::from(*v)).collect(),
            TensorData::Int32(values) => values.iter().map(|v| i64::from(*v)).collect(),
        }
    }

    /// Total sample count implied by the dimensions.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.dims.iter().product()
    }
}

/// Initialiser payloads, in the producer's type.
#[derive(Debug, Clone, PartialEq)]
pub enum TensorData {
    /// 32-bit floats.
    Float(Vec<f32>),
    /// 64-bit integers: shapes, axes.
    Int64(Vec<i64>),
    /// 32-bit integers.
    Int32(Vec<i32>),
    /// Signed 8-bit: quantised weights.
    Int8(Vec<i8>),
    /// Unsigned 8-bit: quantised activations and zero points.
    Uint8(Vec<u8>),
}

/// A declared graph input or output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ValueInfo {
    /// Value name.
    pub name: String,
    /// Shape, with `None` for a symbolic dimension such as a dynamic batch.
    pub shape: Vec<Option<usize>>,
}

impl ValueInfo {
    /// The shape with symbolic dimensions resolved to a batch size.
    #[must_use]
    pub fn concrete_shape(&self, batch: usize) -> Vec<usize> {
        self.shape.iter().map(|dim| dim.unwrap_or(batch)).collect()
    }
}

/// Parse a whole model file.
///
/// # Errors
///
/// `AURA-ML-5010` on any structural problem: a truncated field, a length that
/// overruns, a non-UTF-8 name, or a tensor whose payload disagrees with its
/// declared dimensions.
pub fn parse(bytes: &[u8]) -> AuraResult<OnnxModel> {
    let mut model = OnnxModel {
        ir_version: 0,
        producer_name: String::new(),
        opset: 0,
        graph: Graph::default(),
    };
    let mut reader = Reader::new(bytes);
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            // ModelProto.ir_version = 1
            (1, WireType::Varint) => model.ir_version = reader.varint()?.cast_signed(),
            // ModelProto.producer_name = 2
            (2, WireType::Bytes) => model.producer_name = reader.string()?,
            // ModelProto.graph = 7
            (7, WireType::Bytes) => model.graph = parse_graph(reader.bytes()?)?,
            // ModelProto.opset_import = 8
            (8, WireType::Bytes) => {
                let version = parse_opset(reader.bytes()?)?;
                model.opset = model.opset.max(version);
            }
            _ => reader.skip(wire)?,
        }
    }
    Ok(model)
}

/// `OperatorSetIdProto`: domain = 1, version = 2. Only the default domain counts.
fn parse_opset(bytes: &[u8]) -> AuraResult<i64> {
    let mut reader = Reader::new(bytes);
    let mut domain = String::new();
    let mut version = 0i64;
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            (1, WireType::Bytes) => domain = reader.string()?,
            (2, WireType::Varint) => version = reader.varint()?.cast_signed(),
            _ => reader.skip(wire)?,
        }
    }
    if domain.is_empty() || domain == "ai.onnx" {
        Ok(version)
    } else {
        Ok(0)
    }
}

fn parse_graph(bytes: &[u8]) -> AuraResult<Graph> {
    let mut graph = Graph::default();
    let mut reader = Reader::new(bytes);
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            // GraphProto.node = 1
            (1, WireType::Bytes) => graph.nodes.push(parse_node(reader.bytes()?)?),
            // GraphProto.name = 2
            (2, WireType::Bytes) => graph.name = reader.string()?,
            // GraphProto.initializer = 5
            (5, WireType::Bytes) => graph.initializers.push(parse_tensor(reader.bytes()?)?),
            // GraphProto.input = 11
            (11, WireType::Bytes) => graph.inputs.push(parse_value_info(reader.bytes()?)?),
            // GraphProto.output = 12
            (12, WireType::Bytes) => graph.outputs.push(parse_value_info(reader.bytes()?)?),
            _ => reader.skip(wire)?,
        }
    }
    Ok(graph)
}

fn parse_node(bytes: &[u8]) -> AuraResult<Node> {
    let mut node = Node::default();
    let mut reader = Reader::new(bytes);
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            // NodeProto.input = 1, output = 2, name = 3, op_type = 4, attribute = 5
            (1, WireType::Bytes) => node.inputs.push(reader.string()?),
            (2, WireType::Bytes) => node.outputs.push(reader.string()?),
            (3, WireType::Bytes) => node.name = reader.string()?,
            (4, WireType::Bytes) => node.op_type = reader.string()?,
            (5, WireType::Bytes) => node.attributes.push(parse_attribute(reader.bytes()?)?),
            _ => reader.skip(wire)?,
        }
    }
    Ok(node)
}

fn parse_attribute(bytes: &[u8]) -> AuraResult<Attribute> {
    let mut name = String::new();
    let mut float = None;
    let mut int = None;
    let mut string = None;
    let mut tensor = None;
    let mut floats = Vec::new();
    let mut ints = Vec::new();
    let mut declared_type = 0i64;

    let mut reader = Reader::new(bytes);
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            // AttributeProto: name = 1, f = 2, i = 3, s = 4, t = 5,
            // floats = 7, ints = 8, type = 20
            (1, WireType::Bytes) => name = reader.string()?,
            (2, WireType::Fixed32) => float = Some(f32::from_bits(reader.fixed32()?)),
            (3, WireType::Varint) => int = Some(reader.varint()?.cast_signed()),
            (4, WireType::Bytes) => {
                string = Some(String::from_utf8_lossy(reader.bytes()?).into_owned());
            }
            (5, WireType::Bytes) => tensor = Some(parse_tensor(reader.bytes()?)?),
            (7, WireType::Bytes) => floats.extend(packed_floats(reader.bytes()?)?),
            (7, WireType::Fixed32) => floats.push(f32::from_bits(reader.fixed32()?)),
            (8, WireType::Bytes) => ints.extend(packed_varints(reader.bytes()?)?),
            (8, WireType::Varint) => ints.push(reader.varint()?.cast_signed()),
            (20, WireType::Varint) => declared_type = reader.varint()?.cast_signed(),
            _ => reader.skip(wire)?,
        }
    }

    // AttributeType: FLOAT 1, INT 2, STRING 3, TENSOR 4, FLOATS 6, INTS 7.
    let value = match declared_type {
        1 => AttrValue::Float(float.unwrap_or_default()),
        2 => AttrValue::Int(int.unwrap_or_default()),
        3 => AttrValue::Str(string.unwrap_or_default()),
        4 => tensor.map(AttrValue::Tensor).ok_or_else(|| {
            malformed(format!(
                "attribute {name} declares a tensor but carries none"
            ))
        })?,
        6 => AttrValue::Floats(floats),
        7 => AttrValue::Ints(ints),
        // A producer that omitted `type` - legal in older files - is read from
        // whichever payload actually arrived.
        _ => {
            if let Some(value) = int {
                AttrValue::Int(value)
            } else if let Some(value) = float {
                AttrValue::Float(value)
            } else if !ints.is_empty() {
                AttrValue::Ints(ints)
            } else if !floats.is_empty() {
                AttrValue::Floats(floats)
            } else if let Some(value) = tensor {
                AttrValue::Tensor(value)
            } else {
                AttrValue::Str(string.unwrap_or_default())
            }
        }
    };
    Ok(Attribute { name, value })
}

fn parse_tensor(bytes: &[u8]) -> AuraResult<InitTensor> {
    let mut dims: Vec<usize> = Vec::new();
    let mut elem_type = data_type::FLOAT;
    let mut name = String::new();
    let mut float_data = Vec::new();
    let mut int64_data = Vec::new();
    let mut int32_data = Vec::new();
    let mut raw: Option<Vec<u8>> = None;

    let mut reader = Reader::new(bytes);
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            // TensorProto: dims = 1, data_type = 2, float_data = 4,
            // int32_data = 5, int64_data = 7, name = 8, raw_data = 9
            (1, WireType::Varint) => dims.push(reader.varint()? as usize),
            (1, WireType::Bytes) => {
                for value in packed_varints(reader.bytes()?)? {
                    dims.push(value.max(0) as usize);
                }
            }
            (2, WireType::Varint) => elem_type = reader.varint()?.cast_signed(),
            (4, WireType::Bytes) => float_data.extend(packed_floats(reader.bytes()?)?),
            (4, WireType::Fixed32) => float_data.push(f32::from_bits(reader.fixed32()?)),
            (5, WireType::Bytes) => {
                for value in packed_varints(reader.bytes()?)? {
                    int32_data.push(value as i32);
                }
            }
            (7, WireType::Bytes) => int64_data.extend(packed_varints(reader.bytes()?)?),
            (8, WireType::Bytes) => name = reader.string()?,
            (9, WireType::Bytes) => raw = Some(reader.bytes()?.to_vec()),
            _ => reader.skip(wire)?,
        }
    }

    let payload = match (elem_type, raw) {
        (data_type::FLOAT, Some(raw)) => TensorData::Float(packed_floats(&raw)?),
        (data_type::FLOAT, None) => TensorData::Float(float_data),
        (data_type::INT64, Some(raw)) => TensorData::Int64(
            raw.chunks_exact(8)
                .map(|chunk| {
                    let mut buf = [0u8; 8];
                    buf.copy_from_slice(chunk);
                    i64::from_le_bytes(buf)
                })
                .collect(),
        ),
        (data_type::INT64, None) => TensorData::Int64(int64_data),
        (data_type::INT32, Some(raw)) => TensorData::Int32(
            raw.chunks_exact(4)
                .map(|chunk| {
                    let mut buf = [0u8; 4];
                    buf.copy_from_slice(chunk);
                    i32::from_le_bytes(buf)
                })
                .collect(),
        ),
        (data_type::INT32, None) => TensorData::Int32(int32_data),
        (data_type::INT8, Some(raw)) => {
            TensorData::Int8(raw.iter().map(|byte| byte.cast_signed()).collect())
        }
        (data_type::UINT8, Some(raw)) => TensorData::Uint8(raw),
        (other, _) => {
            return Err(malformed(format!(
                "tensor {name} uses element type {other}, which this build does not read"
            )))
        }
    };

    let tensor = InitTensor {
        name,
        dims,
        data: payload,
    };
    let declared = tensor.element_count();
    let actual = match &tensor.data {
        TensorData::Float(values) => values.len(),
        TensorData::Int64(values) => values.len(),
        TensorData::Int32(values) => values.len(),
        TensorData::Int8(values) => values.len(),
        TensorData::Uint8(values) => values.len(),
    };
    if declared != actual {
        return Err(malformed(format!(
            "tensor {} declares {declared} samples and carries {actual}",
            tensor.name
        )));
    }
    Ok(tensor)
}

fn parse_value_info(bytes: &[u8]) -> AuraResult<ValueInfo> {
    let mut info = ValueInfo::default();
    let mut reader = Reader::new(bytes);
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            // ValueInfoProto: name = 1, type = 2
            (1, WireType::Bytes) => info.name = reader.string()?,
            (2, WireType::Bytes) => info.shape = parse_type(reader.bytes()?)?,
            _ => reader.skip(wire)?,
        }
    }
    Ok(info)
}

/// `TypeProto.tensor_type = 1` -> `TensorTypeProto.shape = 2` -> `dim = 1`.
fn parse_type(bytes: &[u8]) -> AuraResult<Vec<Option<usize>>> {
    let mut reader = Reader::new(bytes);
    let mut shape = Vec::new();
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            (1, WireType::Bytes) => {
                let tensor_type = reader.bytes()?;
                let mut inner = Reader::new(tensor_type);
                while !inner.is_done() {
                    let (inner_field, inner_wire) = inner.field()?;
                    match (inner_field, inner_wire) {
                        (2, WireType::Bytes) => shape = parse_shape(inner.bytes()?)?,
                        _ => inner.skip(inner_wire)?,
                    }
                }
            }
            _ => reader.skip(wire)?,
        }
    }
    Ok(shape)
}

fn parse_shape(bytes: &[u8]) -> AuraResult<Vec<Option<usize>>> {
    let mut reader = Reader::new(bytes);
    let mut dims = Vec::new();
    while !reader.is_done() {
        let (field, wire) = reader.field()?;
        match (field, wire) {
            (1, WireType::Bytes) => {
                let dim = reader.bytes()?;
                let mut inner = Reader::new(dim);
                let mut value = None;
                while !inner.is_done() {
                    let (inner_field, inner_wire) = inner.field()?;
                    match (inner_field, inner_wire) {
                        // Dimension.dim_value = 1, dim_param = 2 (symbolic)
                        (1, WireType::Varint) => value = Some(inner.varint()? as usize),
                        _ => inner.skip(inner_wire)?,
                    }
                }
                dims.push(value);
            }
            _ => reader.skip(wire)?,
        }
    }
    Ok(dims)
}

/// Serialise a model back to ONNX bytes.
///
/// The writer exists to prove the reader. It emits the same subset it reads, so
/// `parse(serialise(model)) == model` for every model this runtime can execute.
#[must_use]
pub fn serialise(model: &OnnxModel) -> Vec<u8> {
    let mut out = Writer::new();
    out.varint(1, model.ir_version);
    out.string(2, &model.producer_name);
    out.message(7, write_graph(&model.graph));
    let mut opset = Writer::new();
    opset.string(1, "");
    opset.varint(2, model.opset);
    out.message(8, opset);
    out.finish()
}

fn write_graph(graph: &Graph) -> Writer {
    let mut out = Writer::new();
    for node in &graph.nodes {
        out.message(1, write_node(node));
    }
    out.string(2, &graph.name);
    for tensor in &graph.initializers {
        out.message(5, write_tensor(tensor));
    }
    for info in &graph.inputs {
        out.message(11, write_value_info(info));
    }
    for info in &graph.outputs {
        out.message(12, write_value_info(info));
    }
    out
}

fn write_node(node: &Node) -> Writer {
    let mut out = Writer::new();
    for name in &node.inputs {
        out.string(1, name);
    }
    for name in &node.outputs {
        out.string(2, name);
    }
    out.string(3, &node.name);
    out.string(4, &node.op_type);
    for attribute in &node.attributes {
        out.message(5, write_attribute(attribute));
    }
    out
}

fn write_attribute(attribute: &Attribute) -> Writer {
    let mut out = Writer::new();
    out.string(1, &attribute.name);
    match &attribute.value {
        AttrValue::Float(value) => {
            out.float(2, *value);
            out.varint(20, 1);
        }
        AttrValue::Int(value) => {
            out.varint(3, *value);
            out.varint(20, 2);
        }
        AttrValue::Str(value) => {
            out.string(4, value);
            out.varint(20, 3);
        }
        AttrValue::Tensor(value) => {
            out.message(5, write_tensor(value));
            out.varint(20, 4);
        }
        AttrValue::Floats(values) => {
            out.packed_floats(7, values);
            out.varint(20, 6);
        }
        AttrValue::Ints(values) => {
            out.packed_varints(8, values);
            out.varint(20, 7);
        }
    }
    out
}

fn write_tensor(tensor: &InitTensor) -> Writer {
    let mut out = Writer::new();
    for dim in &tensor.dims {
        out.varint(1, i64::try_from(*dim).unwrap_or(i64::MAX));
    }
    let (elem_type, raw) = match &tensor.data {
        TensorData::Float(values) => (data_type::FLOAT, {
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes
        }),
        TensorData::Int64(values) => (data_type::INT64, {
            let mut bytes = Vec::with_capacity(values.len() * 8);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes
        }),
        TensorData::Int32(values) => (data_type::INT32, {
            let mut bytes = Vec::with_capacity(values.len() * 4);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes
        }),
        TensorData::Int8(values) => (
            data_type::INT8,
            values.iter().map(|value| *value as u8).collect(),
        ),
        TensorData::Uint8(values) => (data_type::UINT8, values.clone()),
    };
    out.varint(2, elem_type);
    out.string(8, &tensor.name);
    out.bytes(9, &raw);
    out
}

fn write_value_info(info: &ValueInfo) -> Writer {
    let mut out = Writer::new();
    out.string(1, &info.name);

    let mut shape = Writer::new();
    for dim in &info.shape {
        let mut dimension = Writer::new();
        match dim {
            Some(value) => dimension.varint(1, i64::try_from(*value).unwrap_or(i64::MAX)),
            None => dimension.string(2, "batch"),
        }
        shape.message(1, dimension);
    }
    let mut tensor_type = Writer::new();
    tensor_type.varint(1, data_type::FLOAT);
    tensor_type.message(2, shape);
    let mut type_proto = Writer::new();
    type_proto.message(1, tensor_type);
    out.message(2, type_proto);
    out
}
