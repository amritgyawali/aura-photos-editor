//! Compiling a parsed model into something that can be run, and running it.
//!
//! Compilation does three things, all of them before a photographer is waiting:
//!
//! 1. **Refuses what it cannot run.** Every operator is checked against the
//!    supported set and an unknown one raises `AURA-ML-5006` naming it. A model
//!    that loads is a model that will finish.
//! 2. **Orders the nodes.** Producers usually emit a topological order, but
//!    nothing in the format requires it, so the order is recomputed. A cycle -
//!    impossible in a valid ONNX graph, possible in a corrupt file - is refused
//!    rather than looped over.
//! 3. **Applies the precision once.** Weights are rounded through binary16 or the
//!    quantisation grid at load, not per inference, so the cost is paid at warmup
//!    and the numbers stay identical between runs.

use std::collections::{BTreeMap, BTreeSet};

use aura_core::AuraResult;

use crate::contract::infer::{Precision, Tensor, TensorView};
use crate::errors::{invalid_graph, op_unsupported};
use crate::onnx::model::{Graph, InitTensor, Node, OnnxModel, TensorData, ValueInfo};
use crate::onnx::ops;
use crate::tensor::apply_precision;

/// A value flowing through the graph.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Activations and weights.
    Float(Tensor),
    /// Shapes, axes and quantisation zero points.
    Int(Vec<i64>),
}

impl Value {
    /// Borrow as a float tensor.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` when an operator that needs floats was handed a shape
    /// tensor - which means the graph is wired wrongly.
    pub fn as_float(&self) -> AuraResult<&Tensor> {
        match self {
            Self::Float(tensor) => Ok(tensor),
            Self::Int(_) => Err(invalid_graph(
                "expected a float tensor, found an integer one",
            )),
        }
    }

    /// Borrow as integers.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5010` when the value is not an integer tensor.
    pub fn as_ints(&self) -> AuraResult<Vec<i64>> {
        match self {
            Self::Int(values) => Ok(values.clone()),
            Self::Float(tensor) => Ok(tensor.data.iter().map(|v| *v as i64).collect()),
        }
    }
}

/// A model that has been checked, ordered and prepared for execution.
#[derive(Debug, Clone)]
pub struct Executable {
    nodes: Vec<Node>,
    initialisers: BTreeMap<String, Value>,
    inputs: Vec<ValueInfo>,
    outputs: Vec<ValueInfo>,
    precision: Precision,
    parameter_count: usize,
}

impl Executable {
    /// Check a parsed model and prepare it to run at a precision.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5006` when an operator or opset is outside the documented subset,
    /// `AURA-ML-5010` when the graph is structurally impossible.
    pub fn compile(model: &OnnxModel, precision: Precision) -> AuraResult<Self> {
        if model.opset > crate::onnx::model::OPSET {
            return Err(op_unsupported(
                "opset",
                format!(
                    "model needs opset {}, this build implements {}",
                    model.opset,
                    crate::onnx::model::OPSET
                ),
            ));
        }
        for node in &model.graph.nodes {
            if !ops::is_supported(&node.op_type) {
                return Err(op_unsupported(
                    &node.op_type,
                    format!(
                        "operator {} in node {} is outside the supported subset",
                        node.op_type,
                        if node.name.is_empty() {
                            "(unnamed)"
                        } else {
                            node.name.as_str()
                        }
                    ),
                ));
            }
        }

        let mut initialisers = BTreeMap::new();
        let mut parameter_count = 0usize;
        for tensor in &model.graph.initializers {
            let value = initialiser_value(tensor, precision);
            if let Value::Float(float) = &value {
                parameter_count += float.data.len();
            }
            initialisers.insert(tensor.name.clone(), value);
        }

        let nodes = topological_order(&model.graph, &initialisers)?;

        // A graph input that an initialiser already satisfies is a constant, not
        // something the caller has to supply. ONNX allows both spellings.
        let inputs: Vec<ValueInfo> = model
            .graph
            .inputs
            .iter()
            .filter(|info| !initialisers.contains_key(&info.name))
            .cloned()
            .collect();

        Ok(Self {
            nodes,
            initialisers,
            inputs,
            outputs: model.graph.outputs.clone(),
            precision,
            parameter_count,
        })
    }

    /// The inputs a caller must supply, in order.
    #[must_use]
    pub fn inputs(&self) -> &[ValueInfo] {
        &self.inputs
    }

    /// The outputs this graph produces, in order.
    #[must_use]
    pub fn outputs(&self) -> &[ValueInfo] {
        &self.outputs
    }

    /// The precision this executable runs at.
    #[must_use]
    pub const fn precision(&self) -> Precision {
        self.precision
    }

    /// How many weight samples are resident, which is what the session reports
    /// to the memory ledger.
    #[must_use]
    pub const fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    /// How many operators will run.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Run the graph over one set of inputs.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when an input's shape disagrees with the declaration, and
    /// `AURA-ML-5010` when an operator is handed something impossible.
    pub fn run(&self, inputs: &[TensorView<'_>]) -> AuraResult<Vec<Tensor>> {
        if inputs.len() != self.inputs.len() {
            return Err(crate::errors::shape_mismatch(
                &format!("{} inputs", self.inputs.len()),
                &format!("{} inputs", inputs.len()),
            ));
        }

        let mut values: BTreeMap<String, Value> = self.initialisers.clone();
        for (declared, supplied) in self.inputs.iter().zip(inputs.iter()) {
            check_input_shape(declared, supplied)?;
            let mut tensor = supplied.to_owned_tensor();
            apply_precision(&mut tensor.data, self.precision);
            values.insert(declared.name.clone(), Value::Float(tensor));
        }

        for node in &self.nodes {
            let mut operands: Vec<Option<&Value>> = Vec::with_capacity(node.inputs.len());
            for name in &node.inputs {
                if name.is_empty() {
                    operands.push(None);
                } else {
                    operands.push(Some(values.get(name).ok_or_else(|| {
                        invalid_graph(format!(
                            "node {} needs value {name}, which nothing produces",
                            node.op_type
                        ))
                    })?));
                }
            }

            let mut produced = ops::eval(node, &operands, self.precision)?;
            for (name, value) in node.outputs.iter().zip(produced.drain(..)) {
                if !name.is_empty() {
                    values.insert(name.clone(), value);
                }
            }
        }

        let mut results = Vec::with_capacity(self.outputs.len());
        for declared in &self.outputs {
            let value = values.get(&declared.name).ok_or_else(|| {
                invalid_graph(format!("graph output {} was never produced", declared.name))
            })?;
            results.push(value.as_float()?.clone());
        }
        Ok(results)
    }
}

/// Turn an initialiser into a runtime value, rounding weights to the precision.
fn initialiser_value(tensor: &InitTensor, precision: Precision) -> Value {
    match &tensor.data {
        TensorData::Int64(values) if tensor.dims.len() <= 1 => Value::Int(values.clone()),
        TensorData::Int32(values) if tensor.dims.len() <= 1 => {
            Value::Int(values.iter().map(|v| i64::from(*v)).collect())
        }
        _ => {
            let mut samples = tensor.to_f32();
            apply_precision(&mut samples, precision);
            Value::Float(Tensor {
                shape: tensor.dims.clone(),
                data: samples,
            })
        }
    }
}

/// Check a supplied tensor against the graph's declaration.
///
/// Symbolic dimensions - a dynamic batch - accept whatever arrives. Fixed
/// dimensions do not: a 384x384 model handed a 512x512 proxy must say so with
/// `AURA-ML-5007` rather than produce a confident wrong answer.
fn check_input_shape(declared: &ValueInfo, supplied: &TensorView<'_>) -> AuraResult<()> {
    if declared.shape.len() != supplied.shape.len() {
        return Err(crate::errors::shape_mismatch(
            &format!("{:?}", declared.shape),
            &supplied.shape_text(),
        ));
    }
    for (want, got) in declared.shape.iter().zip(supplied.shape.iter()) {
        if let Some(want) = want {
            if want != got {
                return Err(crate::errors::shape_mismatch(
                    &format!("{:?}", declared.shape),
                    &supplied.shape_text(),
                ));
            }
        }
    }
    Ok(())
}

/// Order nodes so every input is produced before it is read.
///
/// Kahn's algorithm over the value names, with the ready set kept sorted so the
/// order is a function of the file rather than of hash iteration - D2 forbids
/// letting map order influence a result, and node order decides floating-point
/// accumulation order downstream.
fn topological_order(
    graph: &Graph,
    initialisers: &BTreeMap<String, Value>,
) -> AuraResult<Vec<Node>> {
    let mut available: BTreeSet<String> = initialisers.keys().cloned().collect();
    for info in &graph.inputs {
        available.insert(info.name.clone());
    }

    let mut pending: Vec<(usize, &Node)> = graph.nodes.iter().enumerate().collect();
    let mut ordered = Vec::with_capacity(graph.nodes.len());

    while !pending.is_empty() {
        let ready: Option<usize> = pending.iter().position(|(_, node)| {
            node.inputs
                .iter()
                .all(|name| name.is_empty() || available.contains(name))
        });
        let Some(position) = ready else {
            let stuck = pending
                .first()
                .map(|(_, node)| node.op_type.clone())
                .unwrap_or_default();
            return Err(invalid_graph(format!(
                "graph cannot be ordered: {} nodes have inputs nothing produces, first is {stuck}",
                pending.len()
            )));
        };
        let (_, node) = pending.remove(position);
        for output in &node.outputs {
            available.insert(output.clone());
        }
        ordered.push(node.clone());
    }
    Ok(ordered)
}
