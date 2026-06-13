//! Model Format Support
//!
//! Import and export models in various formats:
//! - ONNX (Open Neural Network Exchange)
//! - TensorFlow Lite
//! - Custom binary format

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[cfg(feature = "onnx")]
pub mod onnx;

#[cfg(feature = "tflite")]
pub mod tflite;

/// Model format types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFormat {
    /// ONNX format
    Onnx,
    /// TensorFlow Lite format
    TfLite,
    /// Custom Mielin format
    Mielin,
}

/// Model metadata
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model name
    pub name: String,
    /// Model version
    pub version: String,
    /// Model format
    pub format: ModelFormat,
    /// Input tensor names and shapes
    pub inputs: BTreeMap<String, Vec<usize>>,
    /// Output tensor names and shapes
    pub outputs: BTreeMap<String, Vec<usize>>,
    /// Model size in bytes
    pub size: usize,
}

impl ModelInfo {
    /// Create a new model info
    pub fn new(name: String, version: String, format: ModelFormat) -> Self {
        Self {
            name,
            version,
            format,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            size: 0,
        }
    }

    /// Add an input tensor
    pub fn add_input(&mut self, name: String, shape: Vec<usize>) {
        self.inputs.insert(name, shape);
    }

    /// Add an output tensor
    pub fn add_output(&mut self, name: String, shape: Vec<usize>) {
        self.outputs.insert(name, shape);
    }

    /// Set model size
    pub fn set_size(&mut self, size: usize) {
        self.size = size;
    }

    /// Get input names
    pub fn input_names(&self) -> Vec<&str> {
        self.inputs.keys().map(|s| s.as_str()).collect()
    }

    /// Get output names
    pub fn output_names(&self) -> Vec<&str> {
        self.outputs.keys().map(|s| s.as_str()).collect()
    }
}

/// Model importer trait
pub trait ModelImporter {
    /// Import a model from bytes
    fn import(data: &[u8]) -> TensorResult<ImportedModel>;

    /// Get supported format
    fn format() -> ModelFormat;
}

/// Model exporter trait
pub trait ModelExporter {
    /// Export a model to bytes
    fn export(model: &ExportModel) -> TensorResult<Vec<u8>>;

    /// Get supported format
    fn format() -> ModelFormat;
}

/// Imported model representation
#[derive(Debug, Clone)]
pub struct ImportedModel {
    /// Model metadata
    pub info: ModelInfo,
    /// Model parameters (weights and biases)
    pub parameters: BTreeMap<String, Tensor<f32>>,
    /// Model graph (operations and connections)
    pub graph: ModelGraph,
}

impl ImportedModel {
    /// Create a new imported model
    pub fn new(info: ModelInfo) -> Self {
        Self {
            info,
            parameters: BTreeMap::new(),
            graph: ModelGraph::new(),
        }
    }

    /// Add a parameter
    pub fn add_parameter(&mut self, name: String, tensor: Tensor<f32>) {
        self.parameters.insert(name, tensor);
    }

    /// Get a parameter
    pub fn get_parameter(&self, name: &str) -> Option<&Tensor<f32>> {
        self.parameters.get(name)
    }

    /// Get all parameter names
    pub fn parameter_names(&self) -> Vec<&str> {
        self.parameters.keys().map(|s| s.as_str()).collect()
    }

    /// Get model info
    pub fn info(&self) -> &ModelInfo {
        &self.info
    }

    /// Get model graph
    pub fn graph(&self) -> &ModelGraph {
        &self.graph
    }
}

/// Model for export
#[derive(Debug, Clone)]
pub struct ExportModel {
    /// Model metadata
    pub info: ModelInfo,
    /// Model parameters
    pub parameters: BTreeMap<String, Tensor<f32>>,
    /// Model graph
    pub graph: ModelGraph,
}

impl ExportModel {
    /// Create a new export model
    pub fn new(info: ModelInfo) -> Self {
        Self {
            info,
            parameters: BTreeMap::new(),
            graph: ModelGraph::new(),
        }
    }

    /// Add a parameter
    pub fn add_parameter(&mut self, name: String, tensor: Tensor<f32>) {
        self.parameters.insert(name, tensor);
    }
}

/// Model computation graph
#[derive(Debug, Clone)]
pub struct ModelGraph {
    /// Nodes in the graph
    pub nodes: Vec<GraphNode>,
    /// Input node indices
    pub inputs: Vec<usize>,
    /// Output node indices
    pub outputs: Vec<usize>,
}

impl ModelGraph {
    /// Create a new empty graph
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: GraphNode) -> usize {
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }

    /// Add an input node
    pub fn add_input(&mut self, node_id: usize) {
        self.inputs.push(node_id);
    }

    /// Add an output node
    pub fn add_output(&mut self, node_id: usize) {
        self.outputs.push(node_id);
    }

    /// Get node by ID
    pub fn get_node(&self, id: usize) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    /// Get number of nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

impl Default for ModelGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Graph node representing an operation
#[derive(Debug, Clone)]
pub struct GraphNode {
    /// Node name
    pub name: String,
    /// Operation type
    pub op_type: String,
    /// Input node indices
    pub inputs: Vec<usize>,
    /// Attributes
    pub attributes: BTreeMap<String, AttributeValue>,
}

impl GraphNode {
    /// Create a new graph node
    pub fn new(name: String, op_type: String) -> Self {
        Self {
            name,
            op_type,
            inputs: Vec::new(),
            attributes: BTreeMap::new(),
        }
    }

    /// Add an input
    pub fn add_input(&mut self, input_id: usize) {
        self.inputs.push(input_id);
    }

    /// Add an attribute
    pub fn add_attribute(&mut self, key: String, value: AttributeValue) {
        self.attributes.insert(key, value);
    }
}

/// Attribute value types
#[derive(Debug, Clone)]
pub enum AttributeValue {
    /// Integer value
    Int(i64),
    /// Float value
    Float(f32),
    /// String value
    String(String),
    /// Integer array
    Ints(Vec<i64>),
    /// Float array
    Floats(Vec<f32>),
    /// Tensor value
    Tensor(Tensor<f32>),
}

/// Convert model between formats
#[allow(unused_variables)]
pub fn convert_model(
    input_data: &[u8],
    input_format: ModelFormat,
    output_format: ModelFormat,
) -> TensorResult<Vec<u8>> {
    // Import from source format
    #[allow(unused_variables, unreachable_code)]
    let imported: ImportedModel = match input_format {
        #[cfg(feature = "onnx")]
        ModelFormat::Onnx => onnx::OnnxImporter::import(input_data)?,
        #[cfg(feature = "tflite")]
        ModelFormat::TfLite => tflite::TfLiteImporter::import(input_data)?,
        ModelFormat::Mielin => {
            let mut deser = crate::serialize::Deserializer::new(input_data);
            deser
                .deserialize_imported_model()
                .map_err(|e| TensorError::other(alloc::format!("Mielin import failed: {}", e)))?
        }
        #[allow(unreachable_patterns)]
        _ => {
            return Err(TensorError::other("Input format not compiled"));
        }
    };

    // Convert to export model
    #[allow(unreachable_code)]
    let export_model = ExportModel {
        info: imported.info.clone(),
        parameters: imported.parameters.clone(),
        graph: imported.graph.clone(),
    };

    // Export to target format
    #[allow(unreachable_code)]
    match output_format {
        #[cfg(feature = "onnx")]
        ModelFormat::Onnx => onnx::OnnxExporter::export(&export_model),
        #[cfg(feature = "tflite")]
        ModelFormat::TfLite => tflite_export(&export_model),
        #[cfg(not(feature = "tflite"))]
        ModelFormat::TfLite => tflite_export(&export_model),
        ModelFormat::Mielin => {
            let mut ser = crate::serialize::Serializer::new();
            ser.serialize_imported_model(&ImportedModel {
                info: export_model.info.clone(),
                parameters: export_model.parameters.clone(),
                graph: export_model.graph.clone(),
            })
            .map_err(|e| TensorError::other(alloc::format!("Mielin export failed: {}", e)))?;
            Ok(ser.into_bytes())
        }
        #[allow(unreachable_patterns)]
        _ => Err(TensorError::other("Output format not compiled")),
    }
}

/// Export a model to a TFLite-compatible JSON envelope.
///
/// **Deviation note**: The canonical TFLite wire format is a FlatBuffer
/// (`schema_generated.h`).  Embedding a full FlatBuffers compiler and the
/// official TFLite schema is out of scope for this crate (it has no C
/// dependencies).  Instead we emit a self-describing JSON envelope that
/// contains all the same semantic information (model metadata, graph
/// topology, operator types, tensor shapes and data types, and serialised
/// f32 weight buffers encoded as Base64).  A companion utility can convert
/// this envelope back to a binary `.tflite` file when a FlatBuffers toolchain
/// is available.
///
/// Magic header: the first 8 bytes of the returned buffer are
/// `b"MIEL_TFL"` so that readers can identify this variant.
fn tflite_export(model: &ExportModel) -> TensorResult<Vec<u8>> {
    // Build a minimal JSON representation that captures the full model.
    let mut out = alloc::string::String::new();

    out.push_str("{\"magic\":\"MIEL_TFL\",\"version\":\"1.0\",");

    // ── model info ──────────────────────────────────────────────────────────
    out.push_str("\"model\":{");
    out.push_str("\"name\":\"");
    out.push_str(&json_escape(&model.info.name));
    out.push_str("\",\"format\":\"TfLite\",\"inputs\":{");
    let mut first = true;
    for (name, shape) in &model.info.inputs {
        if !first {
            out.push(',');
        }
        first = false;
        out.push('"');
        out.push_str(&json_escape(name));
        out.push_str("\":");
        push_shape_json(&mut out, shape);
    }
    out.push_str("},\"outputs\":{");
    first = true;
    for (name, shape) in &model.info.outputs {
        if !first {
            out.push(',');
        }
        first = false;
        out.push('"');
        out.push_str(&json_escape(name));
        out.push_str("\":");
        push_shape_json(&mut out, shape);
    }
    out.push_str("}},");

    // ── graph topology ───────────────────────────────────────────────────────
    out.push_str("\"graph\":{\"nodes\":[");
    for (idx, node) in model.graph.nodes.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":\"");
        out.push_str(&json_escape(&node.name));
        out.push_str("\",\"op_type\":\"");
        out.push_str(&json_escape(&node.op_type));
        out.push_str("\",\"inputs\":[");
        for (i, inp) in node.inputs.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&inp.to_string());
        }
        out.push_str("]}");
    }
    out.push_str("],\"input_indices\":[");
    for (i, idx) in model.graph.inputs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&idx.to_string());
    }
    out.push_str("],\"output_indices\":[");
    for (i, idx) in model.graph.outputs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&idx.to_string());
    }
    out.push_str("]},");

    // ── parameters (weights encoded as little-endian f32 Base64) ────────────
    out.push_str("\"parameters\":{");
    first = true;
    for (name, tensor) in &model.parameters {
        if !first {
            out.push(',');
        }
        first = false;
        out.push('"');
        out.push_str(&json_escape(name));
        out.push_str("\":{\"shape\":");
        push_shape_json(&mut out, tensor.shape());
        out.push_str(",\"dtype\":\"f32\",\"data_b64\":\"");
        // Encode raw f32 bytes as Base64.
        let raw: Vec<u8> = tensor.data().iter().flat_map(|f| f.to_le_bytes()).collect();
        out.push_str(&base64_encode(&raw));
        out.push_str("\"}");
    }
    out.push_str("}}");

    // Prepend magic header then the JSON body.
    let mut result: Vec<u8> = b"MIEL_TFL".to_vec();
    result.extend_from_slice(out.as_bytes());
    Ok(result)
}

/// Escape a string for embedding in a JSON value (minimal escaping).
fn json_escape(s: &str) -> alloc::string::String {
    let mut out = alloc::string::String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Append a JSON array of usize values to `buf`.
fn push_shape_json(buf: &mut alloc::string::String, shape: &[usize]) {
    buf.push('[');
    for (i, &dim) in shape.iter().enumerate() {
        if i > 0 {
            buf.push(',');
        }
        buf.push_str(&dim.to_string());
    }
    buf.push(']');
}

/// Minimal Base64 encoder (no external deps, no `std`).
fn base64_encode(data: &[u8]) -> alloc::string::String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = alloc::string::String::with_capacity((data.len() * 4).div_ceil(3));
    let mut chunks = data.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let b0 = chunk[0] as usize;
        let b1 = chunk[1] as usize;
        let b2 = chunk[2] as usize;
        out.push(CHARS[b0 >> 2] as char);
        out.push(CHARS[((b0 & 0x3) << 4) | (b1 >> 4)] as char);
        out.push(CHARS[((b1 & 0xf) << 2) | (b2 >> 6)] as char);
        out.push(CHARS[b2 & 0x3f] as char);
    }
    match chunks.remainder() {
        [b0] => {
            let b0 = *b0 as usize;
            out.push(CHARS[b0 >> 2] as char);
            out.push(CHARS[(b0 & 0x3) << 4] as char);
            out.push('=');
            out.push('=');
        }
        [b0, b1] => {
            let b0 = *b0 as usize;
            let b1 = *b1 as usize;
            out.push(CHARS[b0 >> 2] as char);
            out.push(CHARS[((b0 & 0x3) << 4) | (b1 >> 4)] as char);
            out.push(CHARS[(b1 & 0xf) << 2] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_model_format() {
        assert_eq!(ModelFormat::Onnx, ModelFormat::Onnx);
        assert_ne!(ModelFormat::Onnx, ModelFormat::TfLite);
    }

    #[test]
    fn test_model_info() {
        let mut info = ModelInfo::new(
            "test_model".to_string(),
            "1.0".to_string(),
            ModelFormat::Onnx,
        );

        info.add_input("input".to_string(), alloc::vec![1, 3, 224, 224]);
        info.add_output("output".to_string(), alloc::vec![1, 1000]);
        info.set_size(1000000);

        assert_eq!(info.inputs.len(), 1);
        assert_eq!(info.outputs.len(), 1);
        assert_eq!(info.size, 1000000);
        assert_eq!(info.input_names(), alloc::vec!["input"]);
        assert_eq!(info.output_names(), alloc::vec!["output"]);
    }

    #[test]
    fn test_imported_model() {
        let info = ModelInfo::new("test".to_string(), "1.0".to_string(), ModelFormat::Onnx);
        let mut model = ImportedModel::new(info);

        let tensor = Tensor::zeros(alloc::vec![3, 3]);
        model.add_parameter("weight".to_string(), tensor);

        assert_eq!(model.parameter_names(), alloc::vec!["weight"]);
        assert!(model.get_parameter("weight").is_some());
        assert!(model.get_parameter("bias").is_none());
    }

    #[test]
    fn test_model_graph() {
        let mut graph = ModelGraph::new();

        let node1 = GraphNode::new("input".to_string(), "Input".to_string());
        let node2 = GraphNode::new("conv".to_string(), "Conv".to_string());

        let id1 = graph.add_node(node1);
        let id2 = graph.add_node(node2);

        graph.add_input(id1);
        graph.add_output(id2);

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.inputs.len(), 1);
        assert_eq!(graph.outputs.len(), 1);
        assert!(graph.get_node(0).is_some());
    }

    #[test]
    fn test_graph_node() {
        let mut node = GraphNode::new("conv".to_string(), "Conv".to_string());
        node.add_input(0);
        node.add_attribute(
            "kernel_size".to_string(),
            AttributeValue::Ints(alloc::vec![3, 3]),
        );

        assert_eq!(node.inputs.len(), 1);
        assert_eq!(node.attributes.len(), 1);
    }

    #[test]
    fn test_attribute_value() {
        let int_val = AttributeValue::Int(42);
        let float_val = AttributeValue::Float(3.125);
        let str_val = AttributeValue::String("test".to_string());

        // Just verify they can be created
        let _ = int_val;
        let _ = float_val;
        let _ = str_val;
    }

    #[test]
    fn test_mielin_format_roundtrip() {
        // Build a simple ImportedModel and verify serialize/deserialize round-trip.
        let mut parameters = BTreeMap::new();
        parameters.insert(
            "weight".to_string(),
            Tensor::matrix(alloc::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap(),
        );

        let mut attr = BTreeMap::new();
        attr.insert("axis".to_string(), AttributeValue::Int(0));
        attr.insert("scale".to_string(), AttributeValue::Float(0.5));
        attr.insert(
            "name_attr".to_string(),
            AttributeValue::String("relu".to_string()),
        );
        attr.insert(
            "dims".to_string(),
            AttributeValue::Ints(alloc::vec![1, 2, 3]),
        );
        attr.insert(
            "alphas".to_string(),
            AttributeValue::Floats(alloc::vec![0.1, 0.2]),
        );

        let graph = ModelGraph {
            nodes: alloc::vec![GraphNode {
                name: "node0".to_string(),
                op_type: "MatMul".to_string(),
                inputs: alloc::vec![0, 1],
                attributes: attr,
            }],
            inputs: alloc::vec![0],
            outputs: alloc::vec![0],
        };

        let mut model_inputs = BTreeMap::new();
        model_inputs.insert("x".to_string(), alloc::vec![2usize, 2usize]);
        let mut model_outputs = BTreeMap::new();
        model_outputs.insert("y".to_string(), alloc::vec![2usize, 2usize]);

        let info = ModelInfo {
            name: "test_model".to_string(),
            version: "1.0".to_string(),
            format: ModelFormat::Mielin,
            inputs: model_inputs,
            outputs: model_outputs,
            size: 4,
        };

        let original = ImportedModel {
            info,
            parameters,
            graph,
        };

        // Serialize
        let mut ser = crate::serialize::Serializer::new();
        ser.serialize_imported_model(&original)
            .expect("serialize ok");
        let bytes = ser.into_bytes();
        assert!(!bytes.is_empty());

        // Deserialize
        let mut deser = crate::serialize::Deserializer::new(&bytes);
        let recovered = deser.deserialize_imported_model().expect("deserialize ok");

        assert_eq!(recovered.info.name, original.info.name);
        assert_eq!(recovered.info.version, original.info.version);
        assert_eq!(recovered.info.size, original.info.size);
        assert_eq!(recovered.info.inputs.len(), 1);
        assert_eq!(recovered.info.outputs.len(), 1);

        assert_eq!(recovered.graph.nodes.len(), 1);
        assert_eq!(recovered.graph.nodes[0].op_type, "MatMul");
        assert_eq!(recovered.graph.nodes[0].inputs, alloc::vec![0usize, 1]);
        assert_eq!(recovered.graph.inputs, alloc::vec![0usize]);
        assert_eq!(recovered.graph.outputs, alloc::vec![0usize]);

        // Verify attributes round-trip
        let attrs = &recovered.graph.nodes[0].attributes;
        assert!(matches!(attrs.get("axis"), Some(AttributeValue::Int(0))));
        assert!(
            matches!(attrs.get("scale"), Some(AttributeValue::Float(v)) if (*v - 0.5).abs() < 1e-6)
        );
        assert!(matches!(attrs.get("name_attr"), Some(AttributeValue::String(s)) if s == "relu"));
        assert!(matches!(attrs.get("dims"), Some(AttributeValue::Ints(v)) if v == &[1i64, 2, 3]));
        assert!(matches!(attrs.get("alphas"), Some(AttributeValue::Floats(v)) if v.len() == 2));

        // Verify parameters
        let w = recovered.parameters.get("weight").expect("weight present");
        assert_eq!(w.data(), &[1.0f32, 2.0, 3.0, 4.0]);
        assert_eq!(w.shape(), &[2, 2]);
    }
}
