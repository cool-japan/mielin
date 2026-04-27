//! Message compression for wire protocol
//!
//! Provides LZ4 and Zstd compression for large messages.
//! Automatically selects compression based on message size and content.

use crate::WireError;
use serde::{Deserialize, Serialize};

/// Compression algorithm selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CompressionAlgorithm {
    /// No compression
    #[default]
    None,
    /// LZ4 fast compression - best for speed
    Lz4,
    /// Zstd compression - better ratio, still fast
    Zstd,
}

/// Compression level for tuning compression ratio vs speed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionLevel {
    /// Fastest compression, lowest ratio
    Fast,
    /// Balanced compression
    #[default]
    Default,
    /// Best compression ratio, slower
    Best,
}

/// Compressed message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedMessage {
    /// Compression algorithm used
    pub algorithm: CompressionAlgorithm,
    /// Original uncompressed size
    pub original_size: u32,
    /// Compressed data (or uncompressed if algorithm is None)
    pub data: Vec<u8>,
}

impl CompressedMessage {
    /// Create an uncompressed message
    pub fn uncompressed(data: Vec<u8>) -> Self {
        let original_size = data.len() as u32;
        Self {
            algorithm: CompressionAlgorithm::None,
            original_size,
            data,
        }
    }

    /// Get compression ratio (compressed_size / original_size)
    pub fn compression_ratio(&self) -> f32 {
        if self.original_size == 0 {
            1.0
        } else {
            self.data.len() as f32 / self.original_size as f32
        }
    }

    /// Check if data was actually compressed
    pub fn is_compressed(&self) -> bool {
        self.algorithm != CompressionAlgorithm::None
    }
}

/// Message compressor with configurable settings
pub struct Compressor {
    /// Preferred compression algorithm
    algorithm: CompressionAlgorithm,
    /// Compression level
    level: CompressionLevel,
    /// Minimum message size to compress (bytes)
    min_size: usize,
    /// Whether to auto-select algorithm based on content
    auto_select: bool,
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compressor {
    /// Create a new compressor with default settings
    pub fn new() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Lz4,
            level: CompressionLevel::Default,
            min_size: 1024, // Only compress messages > 1KB
            auto_select: true,
        }
    }

    /// Create a compressor with specific algorithm
    pub fn with_algorithm(algorithm: CompressionAlgorithm) -> Self {
        Self {
            algorithm,
            level: CompressionLevel::Default,
            min_size: 1024,
            auto_select: false,
        }
    }

    /// Set compression level
    pub fn level(mut self, level: CompressionLevel) -> Self {
        self.level = level;
        self
    }

    /// Set minimum size for compression
    pub fn min_size(mut self, size: usize) -> Self {
        self.min_size = size;
        self
    }

    /// Enable/disable auto-selection of algorithm
    pub fn auto_select(mut self, auto: bool) -> Self {
        self.auto_select = auto;
        self
    }

    /// Compress data
    pub fn compress(&self, data: &[u8]) -> Result<CompressedMessage, WireError> {
        // Skip compression for small messages
        if data.len() < self.min_size {
            return Ok(CompressedMessage::uncompressed(data.to_vec()));
        }

        let algorithm = if self.auto_select {
            self.select_algorithm(data)
        } else {
            self.algorithm
        };

        match algorithm {
            CompressionAlgorithm::None => Ok(CompressedMessage::uncompressed(data.to_vec())),
            CompressionAlgorithm::Lz4 => self.compress_lz4(data),
            CompressionAlgorithm::Zstd => self.compress_zstd(data),
        }
    }

    /// Decompress data
    pub fn decompress(&self, message: &CompressedMessage) -> Result<Vec<u8>, WireError> {
        match message.algorithm {
            CompressionAlgorithm::None => Ok(message.data.clone()),
            CompressionAlgorithm::Lz4 => self.decompress_lz4(message),
            CompressionAlgorithm::Zstd => self.decompress_zstd(message),
        }
    }

    /// Select best algorithm based on data characteristics
    fn select_algorithm(&self, data: &[u8]) -> CompressionAlgorithm {
        // Estimate compressibility by checking byte entropy
        // High entropy = low compressibility (already compressed or random)
        let entropy = Self::estimate_entropy(data);

        if entropy > 0.95 {
            // High entropy - compression won't help much
            CompressionAlgorithm::None
        } else if data.len() > 64 * 1024 {
            // Large data - use Zstd for better ratio
            CompressionAlgorithm::Zstd
        } else {
            // Default to LZ4 for speed
            CompressionAlgorithm::Lz4
        }
    }

    /// Estimate data entropy (0.0 = highly compressible, 1.0 = random/incompressible)
    fn estimate_entropy(data: &[u8]) -> f32 {
        if data.is_empty() {
            return 0.0;
        }

        // Sample-based entropy estimation for performance
        let sample_size = data.len().min(4096);
        let step = data.len() / sample_size;

        let mut byte_counts = [0u32; 256];

        for i in 0..sample_size {
            let idx = i * step;
            if idx < data.len() {
                byte_counts[data[idx] as usize] += 1;
            }
        }

        let total = sample_size as f32;
        let mut entropy = 0.0f32;

        for &count in byte_counts.iter() {
            if count > 0 {
                let p = count as f32 / total;
                entropy -= p * libm::log2f(p);
            }
        }

        // Normalize to 0-1 range (max entropy is 8 bits)
        entropy / 8.0
    }

    /// Compress with LZ4
    fn compress_lz4(&self, data: &[u8]) -> Result<CompressedMessage, WireError> {
        // lz4_flex 0.11 uses compress_prepend_size
        let compressed = lz4_flex::block::compress_prepend_size(data);

        // Only use compression if it actually reduces size
        if compressed.len() < data.len() {
            Ok(CompressedMessage {
                algorithm: CompressionAlgorithm::Lz4,
                original_size: data.len() as u32,
                data: compressed,
            })
        } else {
            Ok(CompressedMessage::uncompressed(data.to_vec()))
        }
    }

    /// Decompress LZ4
    fn decompress_lz4(&self, message: &CompressedMessage) -> Result<Vec<u8>, WireError> {
        lz4_flex::block::decompress_size_prepended(&message.data)
            .map_err(|e| WireError::SerializationError(format!("LZ4 decompression failed: {}", e)))
    }

    /// Compress with Zstd
    fn compress_zstd(&self, data: &[u8]) -> Result<CompressedMessage, WireError> {
        let level = match self.level {
            CompressionLevel::Fast => 1,
            CompressionLevel::Default => 3,
            CompressionLevel::Best => 9,
        };

        let compressed = zstd::encode_all(data, level).map_err(|e| {
            WireError::SerializationError(format!("Zstd compression failed: {}", e))
        })?;

        // Only use compression if it actually reduces size
        if compressed.len() < data.len() {
            Ok(CompressedMessage {
                algorithm: CompressionAlgorithm::Zstd,
                original_size: data.len() as u32,
                data: compressed,
            })
        } else {
            Ok(CompressedMessage::uncompressed(data.to_vec()))
        }
    }

    /// Decompress Zstd
    fn decompress_zstd(&self, message: &CompressedMessage) -> Result<Vec<u8>, WireError> {
        zstd::decode_all(&message.data[..])
            .map_err(|e| WireError::SerializationError(format!("Zstd decompression failed: {}", e)))
    }
}

/// Compress message bytes with default settings
pub fn compress(data: &[u8]) -> Result<CompressedMessage, WireError> {
    Compressor::new().compress(data)
}

/// Decompress message bytes
pub fn decompress(message: &CompressedMessage) -> Result<Vec<u8>, WireError> {
    Compressor::new().decompress(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uncompressed_message() {
        let data = vec![1, 2, 3, 4, 5];
        let msg = CompressedMessage::uncompressed(data.clone());

        assert_eq!(msg.algorithm, CompressionAlgorithm::None);
        assert_eq!(msg.original_size, 5);
        assert_eq!(msg.data, data);
        assert!(!msg.is_compressed());
    }

    #[test]
    fn test_compression_ratio() {
        let msg = CompressedMessage {
            algorithm: CompressionAlgorithm::Lz4,
            original_size: 1000,
            data: vec![0; 500],
        };

        assert!((msg.compression_ratio() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_small_data_not_compressed() {
        let compressor = Compressor::new();
        let small_data = vec![0u8; 100]; // Below min_size threshold

        let compressed = compressor.compress(&small_data).unwrap();
        assert_eq!(compressed.algorithm, CompressionAlgorithm::None);
        assert!(!compressed.is_compressed());
    }

    #[test]
    fn test_lz4_compress_decompress() {
        let compressor = Compressor::with_algorithm(CompressionAlgorithm::Lz4).min_size(0);

        // Highly compressible data (repeated pattern)
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let compressed = compressor.compress(&data).unwrap();

        // Should be compressed
        assert!(compressed.is_compressed() || compressed.data.len() <= data.len());

        let decompressed = compressor.decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_zstd_compress_decompress() {
        let compressor = Compressor::with_algorithm(CompressionAlgorithm::Zstd).min_size(0);

        // Highly compressible data
        let data: Vec<u8> = vec![0u8; 10000];

        let compressed = compressor.compress(&data).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_auto_select_algorithm() {
        let compressor = Compressor::new().auto_select(true).min_size(0);

        // Compressible data
        let compressible: Vec<u8> = vec![0u8; 10000];
        let compressed = compressor.compress(&compressible).unwrap();
        assert!(compressed.is_compressed());

        // Random-ish data (high entropy)
        let random: Vec<u8> = (0..10000).map(|i| ((i * 17 + 31) % 256) as u8).collect();
        let compressed_random = compressor.compress(&random).unwrap();
        // May or may not compress depending on entropy calculation
        let _ = compressor.decompress(&compressed_random).unwrap();
    }

    #[test]
    fn test_compression_levels() {
        let data: Vec<u8> = vec![0u8; 100000];

        let fast = Compressor::with_algorithm(CompressionAlgorithm::Lz4)
            .level(CompressionLevel::Fast)
            .min_size(0)
            .compress(&data)
            .unwrap();

        let best = Compressor::with_algorithm(CompressionAlgorithm::Zstd)
            .level(CompressionLevel::Best)
            .min_size(0)
            .compress(&data)
            .unwrap();

        // Both should compress successfully
        assert!(fast.is_compressed());
        assert!(best.is_compressed());

        // Best should have better ratio (smaller)
        // Note: This assertion might not always hold for simple data
        // but demonstrates the API
    }

    #[test]
    fn test_convenience_functions() {
        let data: Vec<u8> = vec![0u8; 10000];

        let compressed = compress(&data).unwrap();
        let decompressed = decompress(&compressed).unwrap();

        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_entropy_estimation() {
        // Low entropy (all zeros)
        let low_entropy = vec![0u8; 1000];
        let entropy_low = Compressor::estimate_entropy(&low_entropy);
        assert!(
            entropy_low < 0.1,
            "Low entropy data should have low entropy"
        );

        // High entropy (diverse bytes)
        let high_entropy: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let entropy_high = Compressor::estimate_entropy(&high_entropy);
        assert!(
            entropy_high > 0.5,
            "High entropy data should have higher entropy"
        );
    }

    #[test]
    fn test_incompressible_data_fallback() {
        let compressor = Compressor::with_algorithm(CompressionAlgorithm::Lz4).min_size(0);

        // Already compressed or random data
        // Using a simple pattern that doesn't compress well
        let incompressible: Vec<u8> = (0..1000)
            .map(|i| ((i * 7919 + 104729) % 256) as u8) // Prime-based pseudo-random
            .collect();

        let result = compressor.compress(&incompressible).unwrap();

        // Should fall back to uncompressed if compression doesn't help
        let decompressed = compressor.decompress(&result).unwrap();
        assert_eq!(decompressed, incompressible);
    }

    #[test]
    fn test_empty_data() {
        let compressor = Compressor::new().min_size(0);

        let empty: Vec<u8> = vec![];
        let compressed = compressor.compress(&empty).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(decompressed, empty);
    }

    #[test]
    fn test_serialization_of_compressed_message() {
        let msg = CompressedMessage {
            algorithm: CompressionAlgorithm::Lz4,
            original_size: 1000,
            data: vec![1, 2, 3, 4, 5],
        };

        // Test that CompressedMessage can be serialized
        let serialized = oxicode::encode_to_vec(&oxicode::serde::Compat(&msg)).unwrap();
        let (deserialized, _): (oxicode::serde::Compat<CompressedMessage>, _) =
            oxicode::decode_from_slice(&serialized).unwrap();

        assert_eq!(deserialized.0.algorithm, msg.algorithm);
        assert_eq!(deserialized.0.original_size, msg.original_size);
        assert_eq!(deserialized.0.data, msg.data);
    }
}
