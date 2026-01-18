//! Simple Compression Example
//!
//! Demonstrates message compression with LZ4 and Zstd.
//!
//! Run with:
//! ```bash
//! cargo run --example simple_compression
//! ```

use mielin_mesh_wire::{compress, decompress, CompressionAlgorithm, Compressor};
use std::error::Error;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting simple compression example");

    // Create sample data
    let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    info!("Original data size: {} bytes", data.len());

    // Test LZ4 compression
    info!("\n--- LZ4 Compression ---");
    let compressor = Compressor::with_algorithm(CompressionAlgorithm::Lz4);
    let compressed_lz4 = compressor.compress(&data)?;
    info!("Compressed size: {} bytes", compressed_lz4.data.len());
    info!(
        "Compression ratio: {:.2}%",
        compressed_lz4.compression_ratio() * 100.0
    );

    // Decompress and verify
    let decompressed_lz4 = compressor.decompress(&compressed_lz4)?;
    assert_eq!(decompressed_lz4, data);
    info!("✓ Decompression successful");

    // Test Zstd compression
    info!("\n--- Zstd Compression ---");
    let compressor = Compressor::with_algorithm(CompressionAlgorithm::Zstd);
    let compressed_zstd = compressor.compress(&data)?;
    info!("Compressed size: {} bytes", compressed_zstd.data.len());
    info!(
        "Compression ratio: {:.2}%",
        compressed_zstd.compression_ratio() * 100.0
    );

    // Decompress and verify
    let decompressed_zstd = compressor.decompress(&compressed_zstd)?;
    assert_eq!(decompressed_zstd, data);
    info!("✓ Decompression successful");

    // Using standalone functions (auto-select algorithm)
    info!("\n--- Using standalone compress/decompress functions ---");
    let compressed = compress(&data)?;
    let decompressed = decompress(&compressed)?;
    assert_eq!(decompressed, data);
    info!("✓ Standalone functions work correctly");

    info!("\nExample completed");
    Ok(())
}
