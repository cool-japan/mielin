//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::*;

/// Page size for dirty tracking (4KB)
pub const PAGE_SIZE: usize = 4096;

/// Default zstd compression level (balanced between speed and ratio)
pub const ZSTD_DEFAULT_LEVEL: i32 = 3;
/// High compression level for maximum compression ratio
pub const ZSTD_HIGH_LEVEL: i32 = 10;
/// Trait for migration progress callbacks
pub trait MigrationProgressCallback: Send + Sync {
    /// Called when a progress event occurs
    fn on_progress(&mut self, event: MigrationProgressEvent);
}
/// Simple RLE-like compression for memory pages
pub fn simple_compress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        let mut count = 1usize;
        while i + count < data.len() && data[i + count] == byte && count < 127 {
            count += 1;
        }
        if count >= 4 || byte == 0xFF {
            result.push(0xFF);
            result.push(count as u8);
            result.push(byte);
        } else {
            for _ in 0..count {
                if byte == 0xFF {
                    result.push(0xFF);
                    result.push(1);
                    result.push(0xFF);
                } else {
                    result.push(byte);
                }
            }
        }
        i += count;
    }
    result
}
/// Decompress RLE-encoded data
pub fn simple_decompress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0xFF && i + 2 < data.len() {
            let count = data[i + 1] as usize;
            let byte = data[i + 2];
            for _ in 0..count {
                result.push(byte);
            }
            i += 3;
        } else {
            result.push(data[i]);
            i += 1;
        }
    }
    result
}
/// Simple checksum for validation
pub fn simple_checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for (i, &byte) in data.iter().enumerate() {
        sum = sum.wrapping_add((byte as u32).wrapping_mul((i as u32).wrapping_add(1)));
    }
    sum
}

/// LZ4 compression for memory pages (improved compression ratio).
///
/// Uses the self-describing LZ4 frame format (content size embedded in the
/// frame header), so [`lz4_decompress`] does not need the original length.
pub fn lz4_compress(data: &[u8]) -> Vec<u8> {
    // The LZ4 frame format embeds the content size; compression of in-memory
    // pages never fails, so fall back to the raw bytes on the unreachable error
    // path rather than panicking.
    oxiarc_lz4::compress(data).unwrap_or_else(|_| data.to_vec())
}

/// Decompress LZ4-encoded data
pub fn lz4_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    // `usize::MAX` imposes no artificial output cap; the frame's embedded
    // content size drives the actual allocation, matching the self-describing
    // semantics of the previous `decompress_size_prepended` call.
    oxiarc_lz4::decompress(data, usize::MAX)
        .map_err(|e| format!("LZ4 decompression failed: {:?}", e))
}

/// Zstd compression with configurable level
pub fn zstd_compress(data: &[u8], level: i32) -> Result<Vec<u8>, String> {
    oxiarc_zstd::encode_all(data, level).map_err(|e| format!("Zstd compression failed: {}", e))
}

/// Decompress Zstd-encoded data
pub fn zstd_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    oxiarc_zstd::decode_all(data).map_err(|e| format!("Zstd decompression failed: {}", e))
}

/// Compression method identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionMethod {
    /// Simple RLE compression (best for sparse data)
    Rle = 0,
    /// LZ4 compression (fast, decent ratio)
    Lz4 = 1,
    /// Zstd compression with default level (balanced)
    ZstdDefault = 2,
    /// Zstd compression with high level (best ratio, slower)
    ZstdHigh = 3,
}

impl CompressionMethod {
    /// Get compression method from u8 identifier
    pub fn from_u8(value: u8) -> Result<Self, String> {
        match value {
            0 => Ok(Self::Rle),
            1 => Ok(Self::Lz4),
            2 => Ok(Self::ZstdDefault),
            3 => Ok(Self::ZstdHigh),
            _ => Err(format!("Unknown compression method: {}", value)),
        }
    }

    /// Get the name of this compression method
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rle => "RLE",
            Self::Lz4 => "LZ4",
            Self::ZstdDefault => "Zstd (default)",
            Self::ZstdHigh => "Zstd (high)",
        }
    }
}

/// Adaptive compression that chooses the best method
/// Returns (compressed_data, method) where method indicates the compression used
///
/// This function tries multiple compression methods and selects the one with
/// the best compression ratio. For small data (<4KB), it only tries RLE and LZ4
/// for speed. For larger data, it also tries Zstd.
pub fn adaptive_compress(data: &[u8]) -> (Vec<u8>, u8) {
    // For very small data, just use RLE
    if data.len() < 256 {
        return (simple_compress(data), CompressionMethod::Rle as u8);
    }

    let rle_compressed = simple_compress(data);
    let lz4_compressed = lz4_compress(data);

    // For medium data, choose between RLE and LZ4
    if data.len() < 4096 {
        return if lz4_compressed.len() < rle_compressed.len() {
            (lz4_compressed, CompressionMethod::Lz4 as u8)
        } else {
            (rle_compressed, CompressionMethod::Rle as u8)
        };
    }

    // For large data, also try Zstd
    let zstd_default = zstd_compress(data, ZSTD_DEFAULT_LEVEL).unwrap_or_else(|_| data.to_vec());

    // Find the best compression
    let mut best_size = rle_compressed.len();
    let mut best_data = rle_compressed;
    let mut best_method = CompressionMethod::Rle as u8;

    if lz4_compressed.len() < best_size {
        best_size = lz4_compressed.len();
        best_data = lz4_compressed;
        best_method = CompressionMethod::Lz4 as u8;
    }

    if zstd_default.len() < best_size {
        best_data = zstd_default;
        best_method = CompressionMethod::ZstdDefault as u8;
    }

    (best_data, best_method)
}

/// Adaptive compression with aggressive settings (tries all methods including high compression)
/// Use this when compression ratio is more important than speed
pub fn adaptive_compress_aggressive(data: &[u8]) -> (Vec<u8>, u8) {
    let rle_compressed = simple_compress(data);
    let lz4_compressed = lz4_compress(data);
    let zstd_default = zstd_compress(data, ZSTD_DEFAULT_LEVEL).unwrap_or_else(|_| data.to_vec());
    let zstd_high = zstd_compress(data, ZSTD_HIGH_LEVEL).unwrap_or_else(|_| data.to_vec());

    // Find the best compression
    let mut best_size = rle_compressed.len();
    let mut best_data = rle_compressed;
    let mut best_method = CompressionMethod::Rle as u8;

    if lz4_compressed.len() < best_size {
        best_size = lz4_compressed.len();
        best_data = lz4_compressed;
        best_method = CompressionMethod::Lz4 as u8;
    }

    if zstd_default.len() < best_size {
        best_size = zstd_default.len();
        best_data = zstd_default;
        best_method = CompressionMethod::ZstdDefault as u8;
    }

    if zstd_high.len() < best_size {
        best_data = zstd_high;
        best_method = CompressionMethod::ZstdHigh as u8;
    }

    (best_data, best_method)
}

/// Adaptive decompression that uses the correct method
pub fn adaptive_decompress(data: &[u8], method: u8) -> Result<Vec<u8>, String> {
    let compression_method = CompressionMethod::from_u8(method)?;
    match compression_method {
        CompressionMethod::Rle => Ok(simple_decompress(data)),
        CompressionMethod::Lz4 => lz4_decompress(data),
        CompressionMethod::ZstdDefault | CompressionMethod::ZstdHigh => zstd_decompress(data),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Agent;
    #[test]
    fn test_snapshot_creation() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();
        assert_eq!(snapshot.agent_id, *agent.id().as_bytes());
        assert_eq!(snapshot.wasm_binary.len(), 4);
    }
    #[test]
    fn test_snapshot_serialization() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();
        let serialized = snapshot.serialize().unwrap();
        let deserialized = MigrationSnapshot::deserialize(&serialized).unwrap();
        assert_eq!(snapshot.agent_id, deserialized.agent_id);
        assert_eq!(snapshot.wasm_binary, deserialized.wasm_binary);
    }
    #[test]
    fn test_snapshot_restore() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();
        let restored = snapshot.restore().unwrap();
        assert_eq!(restored.dna().binary(), agent.dna().binary());
    }
    #[test]
    fn test_migration_manager() {
        let mut manager = MigrationManager::new();
        let agent = Agent::new(vec![]);
        let snapshot = manager.initiate_migration(&agent, None).unwrap();
        assert_eq!(manager.pending_count(), 1);
        manager.complete_migration(&snapshot.agent_id);
        assert_eq!(manager.pending_count(), 0);
    }
    #[test]
    fn test_dirty_page_compression() {
        let data = vec![0u8; PAGE_SIZE];
        let mut page = DirtyPage::new(0, data.clone());
        page.compress();
        assert!(page.compressed);
        assert!(page.data.len() < data.len());
        assert_eq!(page.get_data(), data);
    }
    #[test]
    fn test_dirty_page_no_compression() {
        let data: Vec<u8> = (0..100).map(|i| i as u8).collect();
        let mut page = DirtyPage::new(0, data.clone());
        page.compress();
        assert_eq!(page.get_data(), data);
    }
    #[test]
    fn test_delta_snapshot_identical() {
        let agent_id = [1u8; 16];
        let state = vec![42u8; PAGE_SIZE * 4];
        let delta = DeltaSnapshot::create(agent_id, &state, &state, 0, 1).unwrap();
        assert_eq!(delta.dirty_page_count(), 0);
        assert_eq!(delta.compressed_size(), 0);
    }
    #[test]
    fn test_delta_snapshot_single_change() {
        let agent_id = [1u8; 16];
        let old_state = vec![0u8; PAGE_SIZE * 4];
        let mut new_state = old_state.clone();
        new_state[100] = 42;
        let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1).unwrap();
        assert_eq!(delta.dirty_page_count(), 1);
        assert_eq!(delta.dirty_pages[0].index, 0);
    }
    #[test]
    fn test_delta_snapshot_apply() {
        let agent_id = [1u8; 16];
        let old_state = vec![0u8; PAGE_SIZE * 4];
        let mut new_state = old_state.clone();
        new_state[100] = 42;
        new_state[PAGE_SIZE + 50] = 99;
        let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1).unwrap();
        let mut restored = old_state.clone();
        delta.apply(&mut restored).unwrap();
        assert_eq!(restored, new_state);
    }
    #[test]
    fn test_delta_snapshot_serialization() {
        let agent_id = [1u8; 16];
        let old_state = vec![0u8; PAGE_SIZE];
        let mut new_state = old_state.clone();
        new_state[50] = 100;
        let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1).unwrap();
        let serialized = delta.serialize().unwrap();
        let deserialized = DeltaSnapshot::deserialize(&serialized).unwrap();
        assert_eq!(delta.agent_id, deserialized.agent_id);
        assert_eq!(delta.sequence, deserialized.sequence);
        assert_eq!(delta.dirty_page_count(), deserialized.dirty_page_count());
    }
    #[test]
    fn test_dirty_page_tracker() {
        let agent_id = [1u8; 16];
        let mut tracker = DirtyPageTracker::new(agent_id, PAGE_SIZE * 4);
        tracker.mark_dirty(0);
        tracker.mark_dirty(2);
        assert!(tracker.is_dirty(0));
        assert!(!tracker.is_dirty(1));
        assert!(tracker.is_dirty(2));
        assert_eq!(tracker.dirty_count(), 2);
        tracker.clear_all();
        assert_eq!(tracker.dirty_count(), 0);
    }
    #[test]
    fn test_dirty_page_tracker_range() {
        let agent_id = [1u8; 16];
        let mut tracker = DirtyPageTracker::new(agent_id, PAGE_SIZE * 10);
        tracker.mark_range_dirty(PAGE_SIZE + 100, PAGE_SIZE * 2);
        assert!(!tracker.is_dirty(0));
        assert!(tracker.is_dirty(1));
        assert!(tracker.is_dirty(2));
        assert!(tracker.is_dirty(3));
        assert!(!tracker.is_dirty(4));
    }
    #[test]
    fn test_dirty_page_tracker_create_delta() {
        let agent_id = [1u8; 16];
        let initial_state = vec![0u8; PAGE_SIZE * 2];
        let mut tracker = DirtyPageTracker::new(agent_id, initial_state.len());
        tracker.initialize(&initial_state);
        let mut new_state = initial_state.clone();
        new_state[50] = 42;
        let delta = tracker.create_delta(&new_state).unwrap();
        assert_eq!(delta.dirty_page_count(), 1);
        assert_eq!(delta.sequence, 2);
        assert_eq!(tracker.sequence(), 2);
    }
    #[test]
    fn test_incremental_migrator() {
        let mut migrator = IncrementalMigrator::new();
        let agent_id = [1u8; 16];
        let initial_state = vec![0u8; PAGE_SIZE * 4];
        migrator.start_tracking(agent_id, &initial_state);
        assert!(migrator.is_tracking(&agent_id));
        assert_eq!(migrator.get_sequence(&agent_id), Some(1));
        let mut new_state = initial_state.clone();
        new_state[100] = 42;
        let delta = migrator.create_incremental(&agent_id, &new_state).unwrap();
        assert_eq!(delta.dirty_page_count(), 1);
        migrator.stop_tracking(&agent_id);
        assert!(!migrator.is_tracking(&agent_id));
    }
    #[test]
    fn test_incremental_migrator_stats() {
        let mut migrator = IncrementalMigrator::new();
        let agent_id = [1u8; 16];
        let initial_state = vec![0u8; PAGE_SIZE * 4];
        migrator.start_tracking(agent_id, &initial_state);
        let mut new_state = initial_state.clone();
        new_state[100] = 42;
        migrator.create_incremental(&agent_id, &new_state).unwrap();
        let stats = migrator.stats();
        assert_eq!(stats.incremental_migrations, 1);
        assert!(stats.bytes_transferred > 0);
    }
    #[test]
    fn test_migration_stats() {
        let mut stats = MigrationStats::default();
        stats.record_full(1000, 100);
        assert_eq!(stats.full_migrations, 1);
        assert_eq!(stats.bytes_transferred, 1000);
        stats.record_incremental(200, 1000, 50);
        assert_eq!(stats.incremental_migrations, 1);
        assert_eq!(stats.bytes_saved_delta, 800);
    }
    #[test]
    fn test_compression_roundtrip() {
        let original = vec![0u8; 1000];
        let compressed = simple_compress(&original);
        let decompressed = simple_decompress(&compressed);
        assert_eq!(original, decompressed);
        assert!(compressed.len() < original.len());
    }
    #[test]
    fn test_compression_mixed_data() {
        let mut original = vec![0u8; 500];
        original.extend((0..200).map(|i| i as u8));
        original.extend(vec![255u8; 300]);
        let compressed = simple_compress(&original);
        let decompressed = simple_decompress(&compressed);
        assert_eq!(original, decompressed);
    }
    #[test]
    fn test_checksum() {
        let data1 = vec![1, 2, 3, 4, 5];
        let data2 = vec![1, 2, 3, 4, 6];
        let sum1 = simple_checksum(&data1);
        let sum2 = simple_checksum(&data2);
        assert_ne!(sum1, sum2);
    }
    #[test]
    fn test_lz4_compression_roundtrip() {
        let original = vec![0u8; 1000];
        let compressed = lz4_compress(&original);
        let decompressed = lz4_decompress(&compressed).unwrap();
        assert_eq!(original, decompressed);
        assert!(compressed.len() < original.len());
    }
    #[test]
    fn test_lz4_compression_mixed_data() {
        let mut original = vec![0u8; 500];
        original.extend((0..200).map(|i| i as u8));
        original.extend(vec![255u8; 300]);
        let compressed = lz4_compress(&original);
        let decompressed = lz4_decompress(&compressed).unwrap();
        assert_eq!(original, decompressed);
    }
    #[test]
    fn test_lz4_compression_complex_data() {
        // Create data with patterns
        let mut original = Vec::new();
        for i in 0..100 {
            original.extend(&[i as u8, (i + 1) as u8, i as u8, (i + 1) as u8]);
        }
        let rle_compressed = simple_compress(&original);
        let lz4_compressed = lz4_compress(&original);

        // Verify both methods decompress correctly
        assert_eq!(original, simple_decompress(&rle_compressed));
        assert_eq!(original, lz4_decompress(&lz4_compressed).unwrap());
    }
    #[test]
    fn test_adaptive_compression_chooses_best() {
        // Test with data that compresses well with RLE (lots of zeros)
        let sparse_data = vec![0u8; 1000];
        let (compressed, method) = adaptive_compress(&sparse_data);
        let decompressed = adaptive_decompress(&compressed, method).unwrap();
        assert_eq!(sparse_data, decompressed);

        // Test with mixed data
        let mut mixed_data = Vec::new();
        for i in 0..100 {
            mixed_data.extend(&[i as u8, (i + 1) as u8, i as u8, (i + 1) as u8]);
        }
        let (compressed2, method2) = adaptive_compress(&mixed_data);
        let decompressed2 = adaptive_decompress(&compressed2, method2).unwrap();
        assert_eq!(mixed_data, decompressed2);
    }
    #[test]
    fn test_adaptive_decompress_invalid_method() {
        let data = vec![1, 2, 3];
        let result = adaptive_decompress(&data, 99);
        assert!(result.is_err());
    }

    // ========================================================================
    // Zstd compression tests
    // ========================================================================

    #[test]
    fn test_zstd_compression_roundtrip_default() {
        let original = vec![0u8; 1000];
        let compressed = zstd_compress(&original, ZSTD_DEFAULT_LEVEL).unwrap();
        let decompressed = zstd_decompress(&compressed).unwrap();
        assert_eq!(original, decompressed);
        assert!(compressed.len() < original.len());
    }

    #[test]
    fn test_zstd_compression_roundtrip_high() {
        let original = vec![0u8; 1000];
        let compressed = zstd_compress(&original, ZSTD_HIGH_LEVEL).unwrap();
        let decompressed = zstd_decompress(&compressed).unwrap();
        assert_eq!(original, decompressed);
        assert!(compressed.len() < original.len());
    }

    #[test]
    fn test_zstd_compression_mixed_data() {
        let mut original = vec![0u8; 500];
        original.extend((0..200).map(|i| i as u8));
        original.extend(vec![255u8; 300]);
        let compressed = zstd_compress(&original, ZSTD_DEFAULT_LEVEL).unwrap();
        let decompressed = zstd_decompress(&compressed).unwrap();
        assert_eq!(original, decompressed);
    }

    #[test]
    fn test_zstd_compression_complex_data() {
        // Create data with patterns
        let mut original = Vec::new();
        for i in 0..100 {
            original.extend(&[i as u8, (i + 1) as u8, i as u8, (i + 1) as u8]);
        }
        let compressed = zstd_compress(&original, ZSTD_DEFAULT_LEVEL).unwrap();
        let decompressed = zstd_decompress(&compressed).unwrap();
        assert_eq!(original, decompressed);
    }

    #[test]
    fn test_zstd_compression_levels() {
        let original = vec![0u8; 5000];
        let compressed_default = zstd_compress(&original, ZSTD_DEFAULT_LEVEL).unwrap();
        let compressed_high = zstd_compress(&original, ZSTD_HIGH_LEVEL).unwrap();

        // High compression should be better or equal
        assert!(compressed_high.len() <= compressed_default.len());

        // Both should decompress correctly
        assert_eq!(original, zstd_decompress(&compressed_default).unwrap());
        assert_eq!(original, zstd_decompress(&compressed_high).unwrap());
    }

    // ========================================================================
    // Compression method tests
    // ========================================================================

    #[test]
    fn test_compression_method_from_u8() {
        assert_eq!(
            CompressionMethod::from_u8(0).unwrap(),
            CompressionMethod::Rle
        );
        assert_eq!(
            CompressionMethod::from_u8(1).unwrap(),
            CompressionMethod::Lz4
        );
        assert_eq!(
            CompressionMethod::from_u8(2).unwrap(),
            CompressionMethod::ZstdDefault
        );
        assert_eq!(
            CompressionMethod::from_u8(3).unwrap(),
            CompressionMethod::ZstdHigh
        );
        assert!(CompressionMethod::from_u8(4).is_err());
    }

    #[test]
    fn test_compression_method_names() {
        assert_eq!(CompressionMethod::Rle.name(), "RLE");
        assert_eq!(CompressionMethod::Lz4.name(), "LZ4");
        assert_eq!(CompressionMethod::ZstdDefault.name(), "Zstd (default)");
        assert_eq!(CompressionMethod::ZstdHigh.name(), "Zstd (high)");
    }

    // ========================================================================
    // Improved adaptive compression tests
    // ========================================================================

    #[test]
    fn test_adaptive_compression_small_data() {
        // Very small data should use RLE
        let small_data = vec![0u8; 100];
        let (compressed, method) = adaptive_compress(&small_data);
        assert_eq!(method, CompressionMethod::Rle as u8);
        let decompressed = adaptive_decompress(&compressed, method).unwrap();
        assert_eq!(small_data, decompressed);
    }

    #[test]
    fn test_adaptive_compression_medium_data() {
        // Medium data should choose between RLE and LZ4
        let medium_data = vec![0u8; 2000];
        let (compressed, method) = adaptive_compress(&medium_data);
        assert!(method == CompressionMethod::Rle as u8 || method == CompressionMethod::Lz4 as u8);
        let decompressed = adaptive_decompress(&compressed, method).unwrap();
        assert_eq!(medium_data, decompressed);
    }

    #[test]
    fn test_adaptive_compression_large_data() {
        // Large data should also try Zstd
        let large_data = vec![0u8; 10000];
        let (compressed, method) = adaptive_compress(&large_data);
        let decompressed = adaptive_decompress(&compressed, method).unwrap();
        assert_eq!(large_data, decompressed);
        // For sparse data, should achieve good compression
        assert!(compressed.len() < large_data.len() / 10);
    }

    #[test]
    fn test_adaptive_compression_random_like_data() {
        // Random-like data (less compressible)
        let random_data: Vec<u8> = (0..5000).map(|i| ((i * 13 + 7) % 256) as u8).collect();
        let (compressed, method) = adaptive_compress(&random_data);
        let decompressed = adaptive_decompress(&compressed, method).unwrap();
        assert_eq!(random_data, decompressed);
    }

    #[test]
    fn test_adaptive_compression_aggressive() {
        let large_data = vec![0u8; 10000];
        let (compressed_normal, method_normal) = adaptive_compress(&large_data);
        let (compressed_aggressive, method_aggressive) = adaptive_compress_aggressive(&large_data);

        // Both should decompress correctly
        assert_eq!(
            large_data,
            adaptive_decompress(&compressed_normal, method_normal).unwrap()
        );
        assert_eq!(
            large_data,
            adaptive_decompress(&compressed_aggressive, method_aggressive).unwrap()
        );

        // Aggressive should be equal or better
        assert!(compressed_aggressive.len() <= compressed_normal.len());
    }

    #[test]
    fn test_compression_ratio_improvements() {
        // Test that zstd provides better compression for large sparse data
        let sparse_data = vec![0u8; 50000];

        let rle_compressed = simple_compress(&sparse_data);
        let lz4_compressed = lz4_compress(&sparse_data);
        let zstd_compressed = zstd_compress(&sparse_data, ZSTD_DEFAULT_LEVEL).unwrap();

        // All should decompress correctly
        assert_eq!(sparse_data, simple_decompress(&rle_compressed));
        assert_eq!(sparse_data, lz4_decompress(&lz4_compressed).unwrap());
        assert_eq!(sparse_data, zstd_decompress(&zstd_compressed).unwrap());

        // For sparse data, all should compress well (at least 90% reduction)
        assert!(
            rle_compressed.len() < sparse_data.len() / 10,
            "RLE compression ratio: {:.2}%",
            (rle_compressed.len() as f64 / sparse_data.len() as f64) * 100.0
        );
        assert!(
            lz4_compressed.len() < sparse_data.len() / 10,
            "LZ4 compression ratio: {:.2}%",
            (lz4_compressed.len() as f64 / sparse_data.len() as f64) * 100.0
        );
        assert!(
            zstd_compressed.len() < sparse_data.len() / 10,
            "Zstd compression ratio: {:.2}%",
            (zstd_compressed.len() as f64 / sparse_data.len() as f64) * 100.0
        );
    }

    #[test]
    fn test_compression_with_patterns() {
        // Create data with repeating patterns (common in state snapshots)
        let mut pattern_data = Vec::new();
        for _ in 0..100 {
            pattern_data.extend(&[0u8, 0u8, 0u8, 0u8, 1u8, 2u8, 3u8, 4u8]);
        }

        let (compressed, method) = adaptive_compress_aggressive(&pattern_data);
        let decompressed = adaptive_decompress(&compressed, method).unwrap();
        assert_eq!(pattern_data, decompressed);

        // Pattern data should compress well
        let ratio = compressed.len() as f64 / pattern_data.len() as f64;
        assert!(ratio < 0.5, "Compression ratio should be < 50%");
    }

    #[test]
    fn test_all_compression_methods_roundtrip() {
        let test_data = vec![42u8; 5000];

        // Test all methods via adaptive_decompress
        for method in 0..4 {
            let compressed = match CompressionMethod::from_u8(method).unwrap() {
                CompressionMethod::Rle => simple_compress(&test_data),
                CompressionMethod::Lz4 => lz4_compress(&test_data),
                CompressionMethod::ZstdDefault => {
                    zstd_compress(&test_data, ZSTD_DEFAULT_LEVEL).unwrap()
                }
                CompressionMethod::ZstdHigh => zstd_compress(&test_data, ZSTD_HIGH_LEVEL).unwrap(),
            };

            let decompressed = adaptive_decompress(&compressed, method).unwrap();
            assert_eq!(
                test_data, decompressed,
                "Method {} failed roundtrip",
                method
            );
        }
    }
    #[test]
    fn test_delta_large_state() {
        let agent_id = [1u8; 16];
        let size = PAGE_SIZE * 100;
        let old_state = vec![0u8; size];
        let mut new_state = old_state.clone();
        for i in 0..5 {
            let offset = i * PAGE_SIZE * 10;
            new_state[offset..offset + PAGE_SIZE].fill(i as u8 + 1);
        }
        let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1).unwrap();
        assert_eq!(delta.dirty_page_count(), 5);
        let mut restored = old_state.clone();
        delta.apply(&mut restored).unwrap();
        assert_eq!(restored, new_state);
    }
    #[test]
    fn test_compatibility_status_can_proceed() {
        assert!(CompatibilityStatus::Compatible.can_proceed());
        assert!(
            CompatibilityStatus::CompatibleWithWarnings(vec!["warning".to_string()]).can_proceed()
        );
        assert!(!CompatibilityStatus::Incompatible("reason".to_string()).can_proceed());
    }
    #[test]
    fn test_compatibility_status_warnings() {
        let warnings = vec!["warning1".to_string(), "warning2".to_string()];
        let status = CompatibilityStatus::CompatibleWithWarnings(warnings.clone());
        assert_eq!(status.warnings(), warnings);
        assert!(CompatibilityStatus::Compatible.warnings().is_empty());
        assert!(CompatibilityStatus::Incompatible("reason".to_string())
            .warnings()
            .is_empty());
    }
    #[test]
    fn test_node_capabilities_new() {
        let caps = NodeCapabilities::new();
        assert!(!caps.architectures.is_empty());
        assert!(caps.available_memory > 0);
        assert!(!caps.wasm_features.is_empty());
    }
    #[test]
    fn test_compatibility_check_compatible() {
        let requirements = AgentRequirements {
            architecture: Some("x86_64".to_string()),
            required_memory: 1024 * 1024,
            required_storage: 1024 * 1024,
            required_wasm_features: vec!["bulk-memory".to_string()],
            state_size: 1024,
            version: "0.1.0".to_string(),
        };
        let capabilities = NodeCapabilities::new();
        let status = CompatibilityValidator::check_compatibility(&requirements, &capabilities);
        assert!(status.can_proceed());
    }
    #[test]
    fn test_compatibility_check_architecture_mismatch() {
        let requirements = AgentRequirements {
            architecture: Some("riscv64".to_string()),
            ..Default::default()
        };
        let capabilities = NodeCapabilities::new();
        let status = CompatibilityValidator::check_compatibility(&requirements, &capabilities);
        assert!(!status.can_proceed());
        assert!(matches!(status, CompatibilityStatus::Incompatible(_)));
    }
    #[test]
    fn test_compatibility_check_insufficient_memory() {
        let requirements = AgentRequirements {
            required_memory: u64::MAX,
            ..Default::default()
        };
        let capabilities = NodeCapabilities::new();
        let status = CompatibilityValidator::check_compatibility(&requirements, &capabilities);
        assert!(!status.can_proceed());
    }
    #[test]
    fn test_compatibility_check_low_memory_warning() {
        let requirements = AgentRequirements {
            required_memory: 7 * 1024 * 1024 * 1024,
            version: "0.1.0".to_string(),
            ..Default::default()
        };
        let capabilities = NodeCapabilities::new();
        let status = CompatibilityValidator::check_compatibility(&requirements, &capabilities);
        assert!(status.can_proceed());
        assert!(matches!(
            status,
            CompatibilityStatus::CompatibleWithWarnings(_)
        ));
    }
    #[test]
    fn test_compatibility_check_wasm_feature_missing() {
        let requirements = AgentRequirements {
            required_wasm_features: vec!["threads".to_string()],
            version: "0.1.0".to_string(),
            ..Default::default()
        };
        let capabilities = NodeCapabilities::new();
        let status = CompatibilityValidator::check_compatibility(&requirements, &capabilities);
        assert!(!status.can_proceed());
    }
    #[test]
    fn test_compatibility_check_version_incompatible() {
        let requirements = AgentRequirements {
            version: "0.0.1".to_string(),
            ..Default::default()
        };
        let mut capabilities = NodeCapabilities::new();
        capabilities.min_compatible_version = "0.1.0".to_string();
        let status = CompatibilityValidator::check_compatibility(&requirements, &capabilities);
        assert!(!status.can_proceed());
    }
    #[test]
    fn test_verification_result_success() {
        let result = VerificationResult::success();
        assert!(result.passed);
        assert!(result.checksum_valid);
        assert!(result.size_valid);
    }
    #[test]
    fn test_verification_result_failure() {
        let result = VerificationResult::failure("Test failure");
        assert!(!result.passed);
        assert!(!result.checksum_valid);
        assert!(result.details.contains(&"Test failure".to_string()));
    }
    #[test]
    fn test_state_verifier_verify_state_success() {
        let state = vec![1u8, 2, 3, 4, 5];
        let checksum = simple_checksum(&state);
        let result = StateVerifier::verify_state(&state, &state, checksum);
        assert!(result.passed);
        assert!(result.checksum_valid);
        assert!(result.size_valid);
    }
    #[test]
    fn test_state_verifier_verify_state_size_mismatch() {
        let original = vec![1u8, 2, 3, 4, 5];
        let migrated = vec![1u8, 2, 3];
        let checksum = simple_checksum(&original);
        let result = StateVerifier::verify_state(&original, &migrated, checksum);
        assert!(!result.passed);
        assert!(!result.size_valid);
    }
    #[test]
    fn test_state_verifier_verify_state_checksum_mismatch() {
        let original = vec![1u8, 2, 3, 4, 5];
        let migrated = vec![1u8, 2, 3, 4, 6];
        let checksum = simple_checksum(&original);
        let result = StateVerifier::verify_state(&original, &migrated, checksum);
        assert!(!result.passed);
        assert!(!result.checksum_valid);
    }
    #[test]
    fn test_state_verifier_verify_delta() {
        let agent_id = [1u8; 16];
        let base_state = vec![0u8; PAGE_SIZE];
        let mut new_state = base_state.clone();
        new_state[100] = 42;
        let delta = DeltaSnapshot::create(agent_id, &base_state, &new_state, 0, 1).unwrap();
        let result = StateVerifier::verify_delta(&base_state, &delta, delta.checksum);
        assert!(result.passed);
        assert!(result.checksum_valid);
        assert!(result.size_valid);
    }
    #[test]
    fn test_rollback_info_capture() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let state = vec![1u8, 2, 3, 4, 5];
        let rollback = RollbackInfo::capture(&agent, &state).unwrap();
        assert_eq!(rollback.agent_id, *agent.id().as_bytes());
        assert_eq!(rollback.original_state, state);
        assert_eq!(rollback.original_checksum, simple_checksum(&state));
    }
    #[test]
    fn test_rollback_info_is_valid() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let state = vec![1u8, 2, 3, 4, 5];
        let rollback = RollbackInfo::capture(&agent, &state).unwrap();
        assert!(rollback.is_valid(3600));
        assert!(!rollback.is_valid(0));
    }
    #[test]
    fn test_audit_entry_creation() {
        let migration_id = [1u8; 16];
        let agent_id = [2u8; 16];
        let entry = AuditEntry::new(
            migration_id,
            agent_id,
            AuditEventType::MigrationStarted,
            "Test".to_string(),
            true,
        );
        assert_eq!(entry.migration_id, migration_id);
        assert_eq!(entry.agent_id, agent_id);
        assert_eq!(entry.event_type, AuditEventType::MigrationStarted);
        assert!(entry.success);
    }
    #[test]
    fn test_audit_entry_with_nodes() {
        let migration_id = [1u8; 16];
        let agent_id = [2u8; 16];
        let source = [3u8; 16];
        let target = [4u8; 16];
        let entry = AuditEntry::new(
            migration_id,
            agent_id,
            AuditEventType::MigrationStarted,
            "Test".to_string(),
            true,
        )
        .with_nodes(Some(source), Some(target));
        assert_eq!(entry.source_node, Some(source));
        assert_eq!(entry.target_node, Some(target));
    }
    #[test]
    fn test_audit_log_basic() {
        let mut log = MigrationAuditLog::new();
        assert!(log.is_empty());
        let migration_id = [1u8; 16];
        let agent_id = [2u8; 16];
        log.log_start(migration_id, agent_id, None, None);
        assert_eq!(log.len(), 1);
        assert!(!log.is_empty());
    }
    #[test]
    fn test_audit_log_get_entries() {
        let mut log = MigrationAuditLog::new();
        let migration_id1 = [1u8; 16];
        let migration_id2 = [2u8; 16];
        let agent_id = [3u8; 16];
        log.log_start(migration_id1, agent_id, None, None);
        log.log_start(migration_id2, agent_id, None, None);
        log.log_completion(migration_id1, agent_id, true, "Success".to_string());
        assert_eq!(log.get_migration_entries(&migration_id1).len(), 2);
        assert_eq!(log.get_migration_entries(&migration_id2).len(), 1);
        assert_eq!(log.get_agent_entries(&agent_id).len(), 3);
    }
    #[test]
    fn test_audit_log_get_failures() {
        let mut log = MigrationAuditLog::new();
        let migration_id = [1u8; 16];
        let agent_id = [2u8; 16];
        log.log_completion(migration_id, agent_id, true, "Success".to_string());
        log.log_completion(migration_id, agent_id, false, "Failed".to_string());
        let failures = log.get_failures();
        assert_eq!(failures.len(), 1);
    }
    #[test]
    fn test_audit_log_capacity_limit() {
        let mut log = MigrationAuditLog::with_capacity(5);
        let migration_id = [1u8; 16];
        let agent_id = [2u8; 16];
        for _ in 0..10 {
            log.log_start(migration_id, agent_id, None, None);
        }
        assert_eq!(log.len(), 5);
    }
    #[test]
    fn test_audit_log_clear() {
        let mut log = MigrationAuditLog::new();
        let migration_id = [1u8; 16];
        let agent_id = [2u8; 16];
        log.log_start(migration_id, agent_id, None, None);
        assert!(!log.is_empty());
        log.clear();
        assert!(log.is_empty());
    }
    #[test]
    fn test_migration_validator_creation() {
        let validator = MigrationValidator::new();
        assert!(validator.audit_log().is_empty());
    }
    #[test]
    fn test_migration_validator_pre_migration() {
        let mut validator = MigrationValidator::new();
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let migration_id = [1u8; 16];
        let requirements = AgentRequirements {
            version: "0.1.0".to_string(),
            ..Default::default()
        };
        let capabilities = NodeCapabilities::new();
        let status = validator
            .validate_pre_migration(migration_id, &agent, &requirements, &capabilities)
            .unwrap();
        assert!(status.can_proceed());
        assert_eq!(validator.audit_log().len(), 1);
    }
    #[test]
    fn test_migration_validator_capture_rollback() {
        let mut validator = MigrationValidator::new();
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let migration_id = [1u8; 16];
        let state = vec![1u8, 2, 3, 4, 5];
        validator
            .capture_rollback_info(migration_id, &agent, &state)
            .unwrap();
        assert_eq!(validator.audit_log().len(), 1);
    }
    #[test]
    fn test_migration_validator_verify_post_migration() {
        let mut validator = MigrationValidator::new();
        let migration_id = [1u8; 16];
        let agent_id = [2u8; 16];
        let state = vec![1u8, 2, 3, 4, 5];
        let result = validator.verify_post_migration(migration_id, agent_id, &state, &state);
        assert!(result.passed);
        assert_eq!(validator.audit_log().len(), 1);
    }
    #[test]
    fn test_migration_validator_execute_rollback() {
        let mut validator = MigrationValidator::new();
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let migration_id = [1u8; 16];
        let state = vec![1u8, 2, 3, 4, 5];
        validator
            .capture_rollback_info(migration_id, &agent, &state)
            .unwrap();
        let rollback = validator.execute_rollback(migration_id).unwrap();
        assert_eq!(rollback.original_state, state);
    }
    #[test]
    fn test_migration_validator_rollback_not_found() {
        let mut validator = MigrationValidator::new();
        let migration_id = [1u8; 16];
        let result = validator.execute_rollback(migration_id);
        assert!(result.is_err());
    }
    #[test]
    fn test_migration_validator_complete_migration() {
        let mut validator = MigrationValidator::new();
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let migration_id = [1u8; 16];
        let state = vec![1u8, 2, 3, 4, 5];
        validator
            .capture_rollback_info(migration_id, &agent, &state)
            .unwrap();
        validator.complete_migration(migration_id, true, "Success".to_string());
        let result = validator.execute_rollback(migration_id);
        assert!(result.is_err());
    }
    #[test]
    fn test_migration_error_type_is_retryable() {
        assert!(MigrationErrorType::NetworkTransient.is_retryable());
        assert!(MigrationErrorType::InsufficientResources.is_retryable());
        assert!(MigrationErrorType::Timeout.is_retryable());
        assert!(MigrationErrorType::TargetUnreachable.is_retryable());
        assert!(!MigrationErrorType::NetworkPermanent.is_retryable());
        assert!(!MigrationErrorType::StateCorruption.is_retryable());
        assert!(!MigrationErrorType::IncompatibleTarget.is_retryable());
    }
    #[test]
    fn test_migration_error_type_should_change_target() {
        assert!(MigrationErrorType::InsufficientResources.should_change_target());
        assert!(MigrationErrorType::IncompatibleTarget.should_change_target());
        assert!(MigrationErrorType::TargetUnreachable.should_change_target());
        assert!(!MigrationErrorType::NetworkTransient.should_change_target());
        assert!(!MigrationErrorType::Timeout.should_change_target());
    }
    #[test]
    fn test_migration_error_creation() {
        let agent_id = [1u8; 16];
        let target = [2u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::NetworkTransient,
            "Connection timeout".to_string(),
            agent_id,
            Some(target),
        );
        assert_eq!(error.error_type, MigrationErrorType::NetworkTransient);
        assert_eq!(error.agent_id, agent_id);
        assert_eq!(error.target_node, Some(target));
        assert_eq!(error.retry_count, 0);
    }
    #[test]
    fn test_migration_error_increment_retry() {
        let agent_id = [1u8; 16];
        let mut error = MigrationError::new(
            MigrationErrorType::Timeout,
            "Timed out".to_string(),
            agent_id,
            None,
        );
        assert_eq!(error.retry_count, 0);
        error.increment_retry();
        assert_eq!(error.retry_count, 1);
        error.increment_retry();
        assert_eq!(error.retry_count, 2);
    }
    #[test]
    fn test_recovery_config_default() {
        let config = RecoveryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_retry_delay_secs, 10);
        assert_eq!(config.max_retry_delay_secs, 300);
        assert_eq!(config.recovery_timeout_secs, 600);
        assert!(config.use_exponential_backoff);
        assert!(config.auto_rollback);
    }
    #[test]
    fn test_recovery_config_calculate_retry_delay() {
        let config = RecoveryConfig::default();
        // attempt 0: 10 * 2^0 = 10
        assert_eq!(config.calculate_retry_delay(0), 10);
        // attempt 1: 10 * 2^0 = 10
        assert_eq!(config.calculate_retry_delay(1), 10);
        // attempt 2: 10 * 2^1 = 20
        assert_eq!(config.calculate_retry_delay(2), 20);
        // attempt 3: 10 * 2^2 = 40
        assert_eq!(config.calculate_retry_delay(3), 40);
        // attempt 4: 10 * 2^3 = 80
        assert_eq!(config.calculate_retry_delay(4), 80);
        // attempt 5: 10 * 2^4 = 160
        assert_eq!(config.calculate_retry_delay(5), 160);
        // attempt 10: 10 * 2^9 = 5120, but capped at max_retry_delay_secs (300)
        assert_eq!(config.calculate_retry_delay(10), 300);
    }
    #[test]
    fn test_recovery_config_no_exponential_backoff() {
        let config = RecoveryConfig {
            use_exponential_backoff: false,
            initial_retry_delay_secs: 10,
            ..Default::default()
        };
        assert_eq!(config.calculate_retry_delay(0), 10);
        assert_eq!(config.calculate_retry_delay(1), 10);
        assert_eq!(config.calculate_retry_delay(5), 10);
    }
    #[test]
    fn test_recovery_config_validate() {
        let config = RecoveryConfig::default();
        assert!(config.validate().is_ok());
        let invalid = RecoveryConfig {
            initial_retry_delay_secs: 0,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
        let invalid2 = RecoveryConfig {
            max_retry_delay_secs: 2,
            initial_retry_delay_secs: 5,
            ..Default::default()
        };
        assert!(invalid2.validate().is_err());
    }
    #[test]
    fn test_recovery_manager_creation() {
        let manager = MigrationRecoveryManager::with_defaults();
        assert_eq!(manager.pending_count(), 0);
    }
    #[test]
    fn test_recovery_manager_record_failure() {
        let mut manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::NetworkTransient,
            "Failed".to_string(),
            agent_id,
            None,
        );
        manager.record_failure(error);
        assert_eq!(manager.pending_count(), 1);
    }
    #[test]
    fn test_recovery_manager_determine_strategy_transient() {
        let manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::NetworkTransient,
            "Transient error".to_string(),
            agent_id,
            None,
        );
        let strategy = manager.determine_strategy(&error);
        assert_eq!(strategy, RecoveryStrategy::RetryOnSameTarget);
    }
    #[test]
    fn test_recovery_manager_determine_strategy_insufficient_resources() {
        let manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::InsufficientResources,
            "Out of memory".to_string(),
            agent_id,
            None,
        );
        let strategy = manager.determine_strategy(&error);
        assert_eq!(strategy, RecoveryStrategy::RetryOnDifferentTarget);
    }
    #[test]
    fn test_recovery_manager_determine_strategy_permanent_error() {
        let manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::StateCorruption,
            "State corrupted".to_string(),
            agent_id,
            None,
        );
        let strategy = manager.determine_strategy(&error);
        assert_eq!(strategy, RecoveryStrategy::Rollback);
    }
    #[test]
    fn test_recovery_manager_determine_strategy_max_retries() {
        let manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let mut error = MigrationError::new(
            MigrationErrorType::NetworkTransient,
            "Transient error".to_string(),
            agent_id,
            None,
        );
        error.retry_count = 5;
        let strategy = manager.determine_strategy(&error);
        assert_eq!(strategy, RecoveryStrategy::Rollback);
    }
    #[test]
    fn test_recovery_manager_execute_recovery() {
        let mut manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::Timeout,
            "Timeout".to_string(),
            agent_id,
            None,
        );
        manager.record_failure(error);
        let strategy = manager.execute_recovery(&agent_id).unwrap();
        assert_eq!(strategy, RecoveryStrategy::RetryOnSameTarget);
        let history = manager.get_history(&agent_id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].attempt_number, 1);
    }
    #[test]
    fn test_recovery_manager_mark_success() {
        let mut manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::Timeout,
            "Timeout".to_string(),
            agent_id,
            None,
        );
        manager.record_failure(error);
        manager.execute_recovery(&agent_id).unwrap();
        manager.mark_recovery_success(&agent_id);
        assert_eq!(manager.pending_count(), 0);
        let history = manager.get_history(&agent_id).unwrap();
        assert!(history[0].success);
    }
    #[test]
    fn test_recovery_manager_mark_failure() {
        let mut manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::Timeout,
            "Timeout".to_string(),
            agent_id,
            None,
        );
        manager.record_failure(error);
        manager.execute_recovery(&agent_id).unwrap();
        manager.mark_recovery_failure(&agent_id, "Failed again".to_string());
        let history = manager.get_history(&agent_id).unwrap();
        assert!(!history[0].success);
        assert_eq!(history[0].error, Some("Failed again".to_string()));
    }
    #[test]
    fn test_recovery_manager_multiple_retries() {
        let mut manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        let error = MigrationError::new(
            MigrationErrorType::NetworkTransient,
            "Network error".to_string(),
            agent_id,
            None,
        );
        manager.record_failure(error);
        manager.execute_recovery(&agent_id).unwrap();
        manager.mark_recovery_failure(&agent_id, "Try 1 failed".to_string());
        manager.execute_recovery(&agent_id).unwrap();
        manager.mark_recovery_failure(&agent_id, "Try 2 failed".to_string());
        manager.execute_recovery(&agent_id).unwrap();
        let history = manager.get_history(&agent_id).unwrap();
        assert_eq!(history.len(), 3);
    }
    #[test]
    fn test_recovery_manager_get_pending() {
        let mut manager = MigrationRecoveryManager::with_defaults();
        let agent1 = [1u8; 16];
        let agent2 = [2u8; 16];
        manager.record_failure(MigrationError::new(
            MigrationErrorType::Timeout,
            "Error 1".to_string(),
            agent1,
            None,
        ));
        manager.record_failure(MigrationError::new(
            MigrationErrorType::NetworkTransient,
            "Error 2".to_string(),
            agent2,
            None,
        ));
        let pending = manager.get_pending();
        assert_eq!(pending.len(), 2);
    }
    #[test]
    fn test_recovery_manager_clear_history() {
        let mut manager = MigrationRecoveryManager::with_defaults();
        let agent_id = [1u8; 16];
        manager.record_failure(MigrationError::new(
            MigrationErrorType::Timeout,
            "Error".to_string(),
            agent_id,
            None,
        ));
        manager.execute_recovery(&agent_id).unwrap();
        assert!(manager.get_history(&agent_id).is_some());
        manager.clear_history(&agent_id);
        assert!(manager.get_history(&agent_id).is_none());
        assert_eq!(manager.pending_count(), 0);
    }
    #[test]
    fn test_recovery_manager_set_config() {
        let mut manager = MigrationRecoveryManager::with_defaults();
        let new_config = RecoveryConfig {
            max_retries: 5,
            ..Default::default()
        };
        assert!(manager.set_config(new_config).is_ok());
        assert_eq!(manager.config().max_retries, 5);
    }
    #[test]
    fn test_recovery_manager_no_auto_rollback() {
        let config = RecoveryConfig {
            auto_rollback: false,
            ..Default::default()
        };
        let manager = MigrationRecoveryManager::new(config).unwrap();
        let agent_id = [1u8; 16];
        let mut error = MigrationError::new(
            MigrationErrorType::StateCorruption,
            "Corrupted".to_string(),
            agent_id,
            None,
        );
        error.retry_count = 5;
        let strategy = manager.determine_strategy(&error);
        assert_eq!(strategy, RecoveryStrategy::GiveUp);
    }
    #[test]
    fn test_progress_event_agent_id() {
        let agent_id = [1u8; 16];
        let event = MigrationProgressEvent::Started {
            agent_id,
            total_bytes: 1000,
        };
        assert_eq!(event.agent_id(), agent_id);
    }
    #[test]
    fn test_progress_event_progress_percent() {
        let agent_id = [1u8; 16];
        let event = MigrationProgressEvent::TransferProgress {
            agent_id,
            transferred_bytes: 500,
            total_bytes: 1000,
            percent: 50,
        };
        assert_eq!(event.progress_percent(), Some(50));
        let event = MigrationProgressEvent::Started {
            agent_id,
            total_bytes: 1000,
        };
        assert_eq!(event.progress_percent(), None);
    }
    #[test]
    fn test_progress_event_is_terminal() {
        let agent_id = [1u8; 16];
        let event = MigrationProgressEvent::Completed {
            agent_id,
            duration_ms: 1000,
            bytes_transferred: 1000,
        };
        assert!(event.is_terminal());
        let event = MigrationProgressEvent::Failed {
            agent_id,
            error: "Error".to_string(),
        };
        assert!(event.is_terminal());
        let event = MigrationProgressEvent::Started {
            agent_id,
            total_bytes: 1000,
        };
        assert!(!event.is_terminal());
    }
    #[test]
    fn test_collecting_callback() {
        let mut callback = CollectingCallback::new();
        assert_eq!(callback.event_count(), 0);
        let agent_id = [1u8; 16];
        callback.on_progress(MigrationProgressEvent::Started {
            agent_id,
            total_bytes: 1000,
        });
        assert_eq!(callback.event_count(), 1);
        assert!(callback.latest_event().is_some());
        callback.clear();
        assert_eq!(callback.event_count(), 0);
    }
    #[test]
    fn test_multi_callback() {
        let mut multi = MultiCallback::new();
        multi.add(CollectingCallback::new());
        multi.add(CollectingCallback::new());
        assert_eq!(multi.callback_count(), 2);
        let agent_id = [1u8; 16];
        multi.on_progress(MigrationProgressEvent::Started {
            agent_id,
            total_bytes: 1000,
        });
    }
    #[test]
    fn test_progress_tracker_creation() {
        let agent_id = [1u8; 16];
        let tracker = MigrationProgressTracker::new(agent_id, 1000);
        assert_eq!(tracker.phase(), MigrationPhase::Capturing);
        assert_eq!(tracker.progress_percent(), 0);
    }
    #[test]
    fn test_progress_tracker_started() {
        let agent_id = [1u8; 16];
        let callback = CollectingCallback::new();
        let mut tracker = MigrationProgressTracker::new(agent_id, 1000).with_callback(callback);
        tracker.report_started();
    }
    #[test]
    fn test_progress_tracker_transfer_progress() {
        let agent_id = [1u8; 16];
        let mut tracker = MigrationProgressTracker::new(agent_id, 1000);
        tracker.update_transfer_progress(500);
        assert_eq!(tracker.phase(), MigrationPhase::Transferring);
        assert_eq!(tracker.progress_percent(), 50);
        tracker.update_transfer_progress(1000);
        assert_eq!(tracker.progress_percent(), 100);
    }
    #[test]
    fn test_progress_tracker_validating() {
        let agent_id = [1u8; 16];
        let mut tracker = MigrationProgressTracker::new(agent_id, 1000);
        tracker.report_validating("Checking checksums".to_string());
        assert_eq!(tracker.phase(), MigrationPhase::Validating);
    }
    #[test]
    fn test_progress_tracker_restoring() {
        let agent_id = [1u8; 16];
        let mut tracker = MigrationProgressTracker::new(agent_id, 1000);
        tracker.update_restoring_progress(75);
        assert_eq!(tracker.phase(), MigrationPhase::Restoring);
    }
    #[test]
    fn test_progress_tracker_completed() {
        let agent_id = [1u8; 16];
        let mut tracker = MigrationProgressTracker::new(agent_id, 1000);
        tracker.update_transfer_progress(1000);
        tracker.report_completed();
        assert_eq!(tracker.phase(), MigrationPhase::Completed);
        let _elapsed = tracker.elapsed_ms();
    }
    #[test]
    fn test_progress_tracker_failed() {
        let agent_id = [1u8; 16];
        let mut tracker = MigrationProgressTracker::new(agent_id, 1000);
        tracker.report_failed("Test error".to_string());
        assert_eq!(tracker.phase(), MigrationPhase::Failed);
    }
    #[test]
    fn test_progress_tracker_with_collecting_callback() {
        let agent_id = [1u8; 16];
        let callback = CollectingCallback::new();
        let mut tracker = MigrationProgressTracker::new(agent_id, 1000).with_callback(callback);
        tracker.report_started();
        tracker.update_capturing_progress(50);
        tracker.update_transfer_progress(500);
        tracker.report_validating("Checksums".to_string());
        tracker.update_restoring_progress(75);
        tracker.report_completed();
    }
    #[test]
    fn test_progress_tracker_zero_bytes() {
        let agent_id = [1u8; 16];
        let mut tracker = MigrationProgressTracker::new(agent_id, 0);
        assert_eq!(tracker.progress_percent(), 100);
        tracker.update_transfer_progress(0);
        assert_eq!(tracker.progress_percent(), 100);
    }
}
