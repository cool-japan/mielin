//! Model Serialization Module
//!
//! Provides functionality to save and load neural network models,
//! tensors, and training state. Designed for no_std environments.
//!
//! # Features
//! - Binary serialization format for efficiency
//! - Save/load individual tensors
//! - Save/load model parameters (weights and biases)
//! - Checkpoint training state (optimizer state, epoch, loss)
//! - Version compatibility checking
//!
//! # Format
//! The serialization format is a simple binary format:
//! - Magic bytes: "MIEL" (4 bytes)
//! - Version: u32 (4 bytes)
//! - Data type tag: u8 (1 byte)
//! - Tensor metadata (shape, dtype)
//! - Tensor data (raw bytes)

#![allow(dead_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;

/// Magic bytes for file format identification
const MAGIC: &[u8; 4] = b"MIEL";

/// Current serialization format version
const VERSION: u32 = 1;

/// Data type tags for serialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataType {
    /// 32-bit floating point
    F32 = 0,
    /// 32-bit signed integer
    I32 = 1,
    /// 8-bit unsigned integer
    U8 = 2,
}

/// Serialization format for model metadata
#[derive(Debug, Clone)]
pub struct ModelMetadata {
    /// Model name or identifier
    pub name: String,
    /// Model version
    pub version: u32,
    /// Number of parameters
    pub num_params: usize,
    /// Training epoch (if applicable)
    pub epoch: Option<u32>,
    /// Training loss (if applicable)
    pub loss: Option<f32>,
}

impl ModelMetadata {
    /// Create new model metadata
    pub fn new(name: String) -> Self {
        Self {
            name,
            version: VERSION,
            num_params: 0,
            epoch: None,
            loss: None,
        }
    }

    /// Set training state
    pub fn with_training_state(mut self, epoch: u32, loss: f32) -> Self {
        self.epoch = Some(epoch);
        self.loss = Some(loss);
        self
    }

    /// Set number of parameters
    pub fn with_num_params(mut self, num_params: usize) -> Self {
        self.num_params = num_params;
        self
    }
}

/// Serializer for tensors and models
pub struct Serializer {
    buffer: Vec<u8>,
}

impl Serializer {
    /// Create a new serializer
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Create a serializer with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    /// Get the serialized bytes
    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }

    /// Get a reference to the serialized bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Write magic bytes and version header
    fn write_header(&mut self) {
        self.buffer.extend_from_slice(MAGIC);
        self.buffer.extend_from_slice(&VERSION.to_le_bytes());
    }

    /// Write a u32 value
    fn write_u32(&mut self, value: u32) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a usize value (as u64 for portability)
    fn write_usize(&mut self, value: usize) {
        self.buffer.extend_from_slice(&(value as u64).to_le_bytes());
    }

    /// Write a f32 value
    fn write_f32(&mut self, value: f32) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a byte
    fn write_u8(&mut self, value: u8) {
        self.buffer.push(value);
    }

    /// Write a string (length-prefixed)
    fn write_string(&mut self, s: &str) {
        self.write_usize(s.len());
        self.buffer.extend_from_slice(s.as_bytes());
    }

    /// Serialize a single f32 tensor
    pub fn serialize_tensor(&mut self, tensor: &Tensor<f32>) -> TensorResult<()> {
        // Write header
        self.write_header();

        // Write data type
        self.write_u8(DataType::F32 as u8);

        // Write shape
        let shape = tensor.shape();
        self.write_usize(shape.len());
        for &dim in shape {
            self.write_usize(dim);
        }

        // Write data
        self.write_usize(tensor.data().len());
        for &val in tensor.data() {
            self.write_f32(val);
        }

        Ok(())
    }

    /// Serialize model metadata
    pub fn serialize_metadata(&mut self, metadata: &ModelMetadata) -> TensorResult<()> {
        self.write_string(&metadata.name);
        self.write_u32(metadata.version);
        self.write_usize(metadata.num_params);

        // Write optional training state
        match metadata.epoch {
            Some(epoch) => {
                self.write_u8(1); // present flag
                self.write_u32(epoch);
            }
            None => self.write_u8(0), // not present
        }

        match metadata.loss {
            Some(loss) => {
                self.write_u8(1); // present flag
                self.write_f32(loss);
            }
            None => self.write_u8(0), // not present
        }

        Ok(())
    }

    /// Serialize multiple tensors (model parameters)
    pub fn serialize_model(
        &mut self,
        metadata: &ModelMetadata,
        tensors: &[Tensor<f32>],
    ) -> TensorResult<()> {
        // Write header
        self.write_header();

        // Write metadata
        self.serialize_metadata(metadata)?;

        // Write number of tensors
        self.write_usize(tensors.len());

        // Write each tensor
        for tensor in tensors {
            self.write_u8(DataType::F32 as u8);

            let shape = tensor.shape();
            self.write_usize(shape.len());
            for &dim in shape {
                self.write_usize(dim);
            }

            self.write_usize(tensor.data().len());
            for &val in tensor.data() {
                self.write_f32(val);
            }
        }

        Ok(())
    }
}

impl Default for Serializer {
    fn default() -> Self {
        Self::new()
    }
}

/// Deserializer for tensors and models
pub struct Deserializer<'a> {
    buffer: &'a [u8],
    pos: usize,
}

impl<'a> Deserializer<'a> {
    /// Create a new deserializer from bytes
    pub fn new(buffer: &'a [u8]) -> Self {
        Self { buffer, pos: 0 }
    }

    /// Check if there are more bytes to read
    pub fn has_more(&self) -> bool {
        self.pos < self.buffer.len()
    }

    /// Get current position
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Read exact number of bytes
    fn read_bytes(&mut self, count: usize) -> TensorResult<&'a [u8]> {
        if self.pos + count > self.buffer.len() {
            return Err(TensorError::Other {
                message: "Unexpected end of buffer".into(),
            });
        }
        let slice = &self.buffer[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    /// Read and verify magic bytes and version
    fn read_header(&mut self) -> TensorResult<u32> {
        let magic = self.read_bytes(4)?;
        if magic != MAGIC {
            return Err(TensorError::Other {
                message: "Invalid magic bytes".into(),
            });
        }

        let version_bytes = self.read_bytes(4)?;
        let version =
            u32::from_le_bytes(version_bytes.try_into().map_err(|_| TensorError::Other {
                message: "Failed to parse version".into(),
            })?);

        if version != VERSION {
            return Err(TensorError::Other {
                message: "Unsupported format version".into(),
            });
        }

        Ok(version)
    }

    /// Read a u32 value
    fn read_u32(&mut self) -> TensorResult<u32> {
        let bytes = self.read_bytes(size_of::<u32>())?;
        Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
            TensorError::Other {
                message: "Failed to parse u32".into(),
            }
        })?))
    }

    /// Read a usize value (stored as u64)
    fn read_usize(&mut self) -> TensorResult<usize> {
        let bytes = self.read_bytes(size_of::<u64>())?;
        let val = u64::from_le_bytes(bytes.try_into().map_err(|_| TensorError::Other {
            message: "Failed to parse usize".into(),
        })?);
        Ok(val as usize)
    }

    /// Read a f32 value
    fn read_f32(&mut self) -> TensorResult<f32> {
        let bytes = self.read_bytes(size_of::<f32>())?;
        Ok(f32::from_le_bytes(bytes.try_into().map_err(|_| {
            TensorError::Other {
                message: "Failed to parse f32".into(),
            }
        })?))
    }

    /// Read a u8 value
    fn read_u8(&mut self) -> TensorResult<u8> {
        let bytes = self.read_bytes(1)?;
        Ok(bytes[0])
    }

    /// Read a string (length-prefixed)
    fn read_string(&mut self) -> TensorResult<String> {
        let len = self.read_usize()?;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| TensorError::Other {
            message: "Invalid UTF-8 string".into(),
        })
    }

    /// Deserialize a single f32 tensor
    pub fn deserialize_tensor(&mut self) -> TensorResult<Tensor<f32>> {
        // Read and verify header
        self.read_header()?;

        // Read data type
        let dtype = self.read_u8()?;
        if dtype != DataType::F32 as u8 {
            return Err(TensorError::Other {
                message: "Unsupported data type".into(),
            });
        }

        // Read shape
        let ndim = self.read_usize()?;
        let mut shape = Vec::with_capacity(ndim);
        for _ in 0..ndim {
            shape.push(self.read_usize()?);
        }

        // Read data
        let data_len = self.read_usize()?;
        let mut data = Vec::with_capacity(data_len);
        for _ in 0..data_len {
            data.push(self.read_f32()?);
        }

        Tensor::from_vec(data, shape).ok_or_else(|| TensorError::Other {
            message: "Failed to create tensor from deserialized data".into(),
        })
    }

    /// Deserialize model metadata
    pub fn deserialize_metadata(&mut self) -> TensorResult<ModelMetadata> {
        let name = self.read_string()?;
        let version = self.read_u32()?;
        let num_params = self.read_usize()?;

        let mut metadata = ModelMetadata::new(name);
        metadata.version = version;
        metadata.num_params = num_params;

        // Read optional epoch
        let epoch_present = self.read_u8()?;
        if epoch_present == 1 {
            metadata.epoch = Some(self.read_u32()?);
        }

        // Read optional loss
        let loss_present = self.read_u8()?;
        if loss_present == 1 {
            metadata.loss = Some(self.read_f32()?);
        }

        Ok(metadata)
    }

    /// Deserialize multiple tensors (model parameters)
    pub fn deserialize_model(&mut self) -> TensorResult<(ModelMetadata, Vec<Tensor<f32>>)> {
        // Read and verify header
        self.read_header()?;

        // Read metadata
        let metadata = self.deserialize_metadata()?;

        // Read number of tensors
        let num_tensors = self.read_usize()?;
        let mut tensors = Vec::with_capacity(num_tensors);

        // Read each tensor
        for _ in 0..num_tensors {
            let dtype = self.read_u8()?;
            if dtype != DataType::F32 as u8 {
                return Err(TensorError::Other {
                    message: "Unsupported data type".into(),
                });
            }

            let ndim = self.read_usize()?;
            let mut shape = Vec::with_capacity(ndim);
            for _ in 0..ndim {
                shape.push(self.read_usize()?);
            }

            let data_len = self.read_usize()?;
            let mut data = Vec::with_capacity(data_len);
            for _ in 0..data_len {
                data.push(self.read_f32()?);
            }

            let tensor = Tensor::from_vec(data, shape).ok_or_else(|| TensorError::Other {
                message: "Failed to create tensor from deserialized data".into(),
            })?;
            tensors.push(tensor);
        }

        Ok((metadata, tensors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_serialize_deserialize_tensor() {
        let tensor = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();

        // Serialize
        let mut serializer = Serializer::new();
        serializer.serialize_tensor(&tensor).unwrap();
        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = Deserializer::new(&bytes);
        let recovered = deserializer.deserialize_tensor().unwrap();

        assert_eq!(tensor.shape(), recovered.shape());
        assert_eq!(tensor.data(), recovered.data());
    }

    #[test]
    fn test_serialize_deserialize_metadata() {
        let metadata = ModelMetadata::new("test_model".into())
            .with_num_params(1000)
            .with_training_state(10, 0.5);

        let mut serializer = Serializer::new();
        serializer.serialize_metadata(&metadata).unwrap();
        let bytes = serializer.into_bytes();

        let mut deserializer = Deserializer::new(&bytes);
        let recovered = deserializer.deserialize_metadata().unwrap();

        assert_eq!(metadata.name, recovered.name);
        assert_eq!(metadata.num_params, recovered.num_params);
        assert_eq!(metadata.epoch, recovered.epoch);
        assert_eq!(metadata.loss, recovered.loss);
    }

    #[test]
    fn test_serialize_deserialize_model() {
        let tensors = vec![
            Tensor::from_vec(vec![1.0, 2.0, 3.0], vec![3]).unwrap(),
            Tensor::from_vec(vec![4.0, 5.0, 6.0, 7.0], vec![2, 2]).unwrap(),
        ];

        let metadata = ModelMetadata::new("my_model".into())
            .with_num_params(7)
            .with_training_state(5, 1.2);

        // Serialize
        let mut serializer = Serializer::new();
        serializer.serialize_model(&metadata, &tensors).unwrap();
        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = Deserializer::new(&bytes);
        let (recovered_meta, recovered_tensors) = deserializer.deserialize_model().unwrap();

        assert_eq!(metadata.name, recovered_meta.name);
        assert_eq!(metadata.num_params, recovered_meta.num_params);
        assert_eq!(tensors.len(), recovered_tensors.len());

        for (orig, recov) in tensors.iter().zip(recovered_tensors.iter()) {
            assert_eq!(orig.shape(), recov.shape());
            assert_eq!(orig.data(), recov.data());
        }
    }

    #[test]
    fn test_invalid_magic_bytes() {
        let bad_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF, 1, 0, 0, 0];
        let mut deserializer = Deserializer::new(&bad_bytes);
        assert!(deserializer.read_header().is_err());
    }

    #[test]
    fn test_empty_buffer() {
        let empty_bytes: Vec<u8> = Vec::new();
        let mut deserializer = Deserializer::new(&empty_bytes);
        assert!(deserializer.deserialize_tensor().is_err());
    }

    #[test]
    fn test_serializer_with_capacity() {
        let serializer = Serializer::with_capacity(1024);
        assert!(serializer.buffer.capacity() >= 1024);
    }

    #[test]
    fn test_deserializer_position() {
        let tensor = Tensor::scalar(42.0);
        let mut serializer = Serializer::new();
        serializer.serialize_tensor(&tensor).unwrap();
        let bytes = serializer.into_bytes();

        let mut deserializer = Deserializer::new(&bytes);
        assert_eq!(deserializer.position(), 0);
        assert!(deserializer.has_more());

        deserializer.deserialize_tensor().unwrap();
        assert!(!deserializer.has_more());
    }
}
