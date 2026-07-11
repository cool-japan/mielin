//! Delta migration and incremental snapshot types

use crate::migration::functions::{simple_checksum, simple_compress, simple_decompress};
use crate::migration::PAGE_SIZE;
use crate::CellError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single dirty page with its data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirtyPage {
    /// Page index (offset / PAGE_SIZE)
    pub index: u32,
    /// Page data (may be compressed)
    pub data: Vec<u8>,
    /// Whether the data is compressed
    pub compressed: bool,
}

impl DirtyPage {
    /// Create a new dirty page
    pub fn new(index: u32, data: Vec<u8>) -> Self {
        Self {
            index,
            data,
            compressed: false,
        }
    }

    /// Compress the page data using simple RLE-like compression
    pub fn compress(&mut self) {
        if self.compressed || self.data.is_empty() {
            return;
        }
        let compressed = simple_compress(&self.data);
        if compressed.len() < self.data.len() {
            self.data = compressed;
            self.compressed = true;
        }
    }

    /// Decompress the page data
    pub fn decompress(&mut self) {
        if !self.compressed || self.data.is_empty() {
            return;
        }
        self.data = simple_decompress(&self.data);
        self.compressed = false;
    }

    /// Get the actual data (decompressed if needed)
    pub fn get_data(&self) -> Vec<u8> {
        if self.compressed {
            simple_decompress(&self.data)
        } else {
            self.data.clone()
        }
    }
}

/// Delta snapshot containing only changed pages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaSnapshot {
    /// Agent ID this delta belongs to
    pub agent_id: [u8; 16],
    /// Sequence number for ordering deltas
    pub sequence: u64,
    /// Base snapshot sequence this delta applies to
    pub base_sequence: u64,
    /// Dirty pages with their data
    pub dirty_pages: Vec<DirtyPage>,
    /// Total memory size (for validation)
    pub total_size: usize,
    /// Timestamp of capture
    pub timestamp: u64,
    /// Checksum for validation
    pub checksum: u32,
}

impl DeltaSnapshot {
    /// Create a delta from two memory states
    pub fn create(
        agent_id: [u8; 16],
        old_state: &[u8],
        new_state: &[u8],
        base_sequence: u64,
        sequence: u64,
    ) -> Result<Self, CellError> {
        if old_state.len() != new_state.len() {
            return Err(CellError::InvalidState(
                "State sizes must match for delta".to_string(),
            ));
        }
        let mut dirty_pages = Vec::new();
        let num_pages = new_state.len().div_ceil(PAGE_SIZE);
        for i in 0..num_pages {
            let start = i * PAGE_SIZE;
            let end = std::cmp::min(start + PAGE_SIZE, new_state.len());
            let old_page = &old_state[start..end];
            let new_page = &new_state[start..end];
            if old_page != new_page {
                let mut page = DirtyPage::new(i as u32, new_page.to_vec());
                page.compress();
                dirty_pages.push(page);
            }
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("Time error".to_string()))?
            .as_secs();
        let checksum = simple_checksum(new_state);
        Ok(Self {
            agent_id,
            sequence,
            base_sequence,
            dirty_pages,
            total_size: new_state.len(),
            timestamp,
            checksum,
        })
    }

    /// Apply this delta to a base state
    pub fn apply(&self, base_state: &mut Vec<u8>) -> Result<(), CellError> {
        if base_state.len() < self.total_size {
            base_state.resize(self.total_size, 0);
        }
        for page in &self.dirty_pages {
            let start = page.index as usize * PAGE_SIZE;
            let data = page.get_data();
            let end = std::cmp::min(start + data.len(), base_state.len());
            if end > base_state.len() {
                return Err(CellError::InvalidState(
                    "Delta page exceeds state size".to_string(),
                ));
            }
            base_state[start..end].copy_from_slice(&data[..end - start]);
        }
        let actual_checksum = simple_checksum(base_state);
        if actual_checksum != self.checksum {
            return Err(CellError::InvalidState(
                "Delta checksum mismatch".to_string(),
            ));
        }
        Ok(())
    }

    /// Get the number of dirty pages
    pub fn dirty_page_count(&self) -> usize {
        self.dirty_pages.len()
    }

    /// Get the compressed size of all dirty pages
    pub fn compressed_size(&self) -> usize {
        self.dirty_pages.iter().map(|p| p.data.len()).sum()
    }

    /// Get the uncompressed size (if all pages were full)
    pub fn uncompressed_size(&self) -> usize {
        self.dirty_pages.len() * PAGE_SIZE
    }

    /// Calculate compression ratio
    pub fn compression_ratio(&self) -> f64 {
        let uncompressed = self.uncompressed_size();
        if uncompressed == 0 {
            return 1.0;
        }
        self.compressed_size() as f64 / uncompressed as f64
    }

    /// Serialize the delta snapshot
    pub fn serialize(&self) -> Result<Vec<u8>, CellError> {
        oxicode::encode_to_vec(&oxicode::serde::Compat(self))
            .map_err(|e| CellError::InvalidState(format!("Serialization failed: {}", e)))
    }

    /// Deserialize a delta snapshot
    pub fn deserialize(data: &[u8]) -> Result<Self, CellError> {
        let (compat, _): (oxicode::serde::Compat<Self>, _) = oxicode::decode_from_slice(data)
            .map_err(|e| CellError::InvalidState(format!("Deserialization failed: {}", e)))?;
        Ok(compat.0)
    }
}

/// Dirty page tracker for an agent
#[derive(Debug)]
pub struct DirtyPageTracker {
    /// Agent ID
    agent_id: [u8; 16],
    /// Bitmap of dirty pages (1 bit per page)
    dirty_bitmap: Vec<u64>,
    /// Number of pages being tracked
    num_pages: usize,
    /// Last known state for delta computation
    last_state: Vec<u8>,
    /// Current sequence number
    sequence: u64,
}

impl DirtyPageTracker {
    /// Create a new tracker for the given state size
    pub fn new(agent_id: [u8; 16], state_size: usize) -> Self {
        let num_pages = state_size.div_ceil(PAGE_SIZE);
        let bitmap_size = num_pages.div_ceil(64);
        Self {
            agent_id,
            dirty_bitmap: vec![0; bitmap_size],
            num_pages,
            last_state: Vec::new(),
            sequence: 0,
        }
    }

    /// Initialize with a base state
    pub fn initialize(&mut self, state: &[u8]) {
        self.last_state = state.to_vec();
        self.clear_all();
        self.sequence = 1;
    }

    /// Mark a page as dirty
    pub fn mark_dirty(&mut self, page_index: usize) {
        if page_index < self.num_pages {
            let word = page_index / 64;
            let bit = page_index % 64;
            self.dirty_bitmap[word] |= 1 << bit;
        }
    }

    /// Mark a range of bytes as dirty
    pub fn mark_range_dirty(&mut self, offset: usize, length: usize) {
        let start_page = offset / PAGE_SIZE;
        let end_page = (offset + length).div_ceil(PAGE_SIZE);
        for page in start_page..end_page.min(self.num_pages) {
            self.mark_dirty(page);
        }
    }

    /// Check if a page is dirty
    pub fn is_dirty(&self, page_index: usize) -> bool {
        if page_index >= self.num_pages {
            return false;
        }
        let word = page_index / 64;
        let bit = page_index % 64;
        (self.dirty_bitmap[word] & (1 << bit)) != 0
    }

    /// Get the count of dirty pages
    pub fn dirty_count(&self) -> usize {
        self.dirty_bitmap
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum()
    }

    /// Clear all dirty bits
    pub fn clear_all(&mut self) {
        self.dirty_bitmap.fill(0);
    }

    /// Create a delta snapshot from the current state
    pub fn create_delta(&mut self, current_state: &[u8]) -> Result<DeltaSnapshot, CellError> {
        let base_sequence = self.sequence;
        self.sequence += 1;
        if self.last_state.is_empty() {
            self.last_state = current_state.to_vec();
            return DeltaSnapshot::create(
                self.agent_id,
                &vec![0; current_state.len()],
                current_state,
                0,
                self.sequence,
            );
        }
        let delta = DeltaSnapshot::create(
            self.agent_id,
            &self.last_state,
            current_state,
            base_sequence,
            self.sequence,
        )?;
        self.last_state = current_state.to_vec();
        self.clear_all();
        Ok(delta)
    }

    /// Get the current sequence number
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

/// Statistics for migration operations
#[derive(Debug, Clone, Default)]
pub struct MigrationStats {
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Bytes saved by compression
    pub bytes_saved_compression: u64,
    /// Bytes saved by delta encoding
    pub bytes_saved_delta: u64,
    /// Number of full migrations
    pub full_migrations: u64,
    /// Number of incremental migrations
    pub incremental_migrations: u64,
    /// Average delta size ratio (delta size / full size)
    pub avg_delta_ratio: f64,
    /// Total migration time in microseconds
    pub total_time_us: u64,
}

impl MigrationStats {
    /// Record a full migration
    pub fn record_full(&mut self, size: usize, time_us: u64) {
        self.bytes_transferred += size as u64;
        self.full_migrations += 1;
        self.total_time_us += time_us;
    }

    /// Record an incremental migration
    pub fn record_incremental(&mut self, delta_size: usize, full_size: usize, time_us: u64) {
        self.bytes_transferred += delta_size as u64;
        self.bytes_saved_delta += (full_size - delta_size) as u64;
        self.incremental_migrations += 1;
        self.total_time_us += time_us;
        let ratio = delta_size as f64 / full_size.max(1) as f64;
        let total = self.full_migrations + self.incremental_migrations;
        self.avg_delta_ratio = (self.avg_delta_ratio * (total - 1) as f64 + ratio) / total as f64;
    }

    /// Record compression savings
    pub fn record_compression(&mut self, uncompressed: usize, compressed: usize) {
        self.bytes_saved_compression += (uncompressed - compressed) as u64;
    }

    /// Get overall compression ratio
    pub fn overall_compression_ratio(&self) -> f64 {
        let total = self.bytes_transferred + self.bytes_saved_compression;
        if total == 0 {
            return 1.0;
        }
        self.bytes_transferred as f64 / total as f64
    }

    /// Get throughput in MB/s
    pub fn throughput_mbps(&self) -> f64 {
        if self.total_time_us == 0 {
            return 0.0;
        }
        (self.bytes_transferred as f64 / 1_000_000.0) / (self.total_time_us as f64 / 1_000_000.0)
    }
}

/// Incremental migration coordinator
pub struct IncrementalMigrator {
    /// Dirty page trackers per agent
    trackers: HashMap<[u8; 16], DirtyPageTracker>,
    /// Migration statistics
    stats: MigrationStats,
}

impl IncrementalMigrator {
    /// Create a new incremental migrator
    pub fn new() -> Self {
        Self {
            trackers: HashMap::new(),
            stats: MigrationStats::default(),
        }
    }

    /// Start tracking an agent for incremental migration
    pub fn start_tracking(&mut self, agent_id: [u8; 16], initial_state: &[u8]) {
        let mut tracker = DirtyPageTracker::new(agent_id, initial_state.len());
        tracker.initialize(initial_state);
        self.trackers.insert(agent_id, tracker);
    }

    /// Stop tracking an agent
    pub fn stop_tracking(&mut self, agent_id: &[u8; 16]) {
        self.trackers.remove(agent_id);
    }

    /// Mark a memory range as dirty for an agent
    pub fn mark_dirty(&mut self, agent_id: &[u8; 16], offset: usize, length: usize) {
        if let Some(tracker) = self.trackers.get_mut(agent_id) {
            tracker.mark_range_dirty(offset, length);
        }
    }

    /// Create an incremental snapshot
    pub fn create_incremental(
        &mut self,
        agent_id: &[u8; 16],
        current_state: &[u8],
    ) -> Result<DeltaSnapshot, CellError> {
        let start_time = std::time::Instant::now();
        let tracker = self.trackers.get_mut(agent_id).ok_or_else(|| {
            CellError::InvalidState("Agent not being tracked for incremental migration".to_string())
        })?;
        let delta = tracker.create_delta(current_state)?;
        let elapsed = start_time.elapsed().as_micros() as u64;
        self.stats
            .record_incremental(delta.compressed_size(), current_state.len(), elapsed);
        self.stats
            .record_compression(delta.uncompressed_size(), delta.compressed_size());
        Ok(delta)
    }

    /// Check if an agent is being tracked
    pub fn is_tracking(&self, agent_id: &[u8; 16]) -> bool {
        self.trackers.contains_key(agent_id)
    }

    /// Get the current sequence for an agent
    pub fn get_sequence(&self, agent_id: &[u8; 16]) -> Option<u64> {
        self.trackers.get(agent_id).map(|t| t.sequence())
    }

    /// Get migration statistics
    pub fn stats(&self) -> &MigrationStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = MigrationStats::default();
    }
}
