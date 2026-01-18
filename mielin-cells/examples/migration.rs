//! Agent migration example
//!
//! This example demonstrates:
//! - Capturing agent snapshots
//! - Migrating agents between nodes
//! - Using delta snapshots
//! - Compression strategies
//! - Migration progress tracking

use mielin_cells::{migration::*, Agent};

fn main() {
    println!("=== Agent Migration Example ===\n");

    // Create a test agent
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d];
    let mut agent = Agent::new(wasm_binary);
    agent.start();

    println!("1. Agent created with ID: {}", agent.id());

    // Example 1: Basic snapshot capture and restore
    println!("\n2. Basic Snapshot Capture:");
    let snapshot = MigrationSnapshot::capture(&agent, None).expect("Failed to capture snapshot");

    println!("   ✓ Snapshot captured");
    println!("   Agent ID: {:?}", snapshot.agent_id);
    println!("   WASM binary size: {} bytes", snapshot.wasm_binary.len());
    println!("   WASM state size: {} bytes", snapshot.wasm_state.len());

    // Restore the snapshot
    let restored_agent = snapshot.restore().expect("Failed to restore snapshot");

    println!("\n3. Snapshot Restored:");
    println!("   Restored agent ID: {}", restored_agent.id());
    println!("   State: {:?}", restored_agent.state());

    // Example 2: Snapshot serialization
    println!("\n4. Snapshot Serialization:");
    let serialized = snapshot.serialize().expect("Failed to serialize");

    println!("   ✓ Serialized snapshot");
    println!("   Serialized size: {} bytes", serialized.len());

    let deserialized = MigrationSnapshot::deserialize(&serialized).expect("Failed to deserialize");

    println!("   ✓ Deserialized snapshot");
    println!(
        "   Agent ID matches: {}",
        deserialized.agent_id == snapshot.agent_id
    );

    // Example 3: Compression strategies
    println!("\n5. Compression Strategies:");

    // Test data
    let test_data = vec![0u8; 4096]; // Sparse data

    // RLE compression
    let rle_compressed = simple_compress(&test_data);
    println!("   RLE compression:");
    println!("   - Original: {} bytes", test_data.len());
    println!("   - Compressed: {} bytes", rle_compressed.len());
    println!(
        "   - Ratio: {:.2}%",
        (rle_compressed.len() as f64 / test_data.len() as f64) * 100.0
    );

    // LZ4 compression
    let lz4_compressed = lz4_compress(&test_data);
    println!("   LZ4 compression:");
    println!("   - Compressed: {} bytes", lz4_compressed.len());
    println!(
        "   - Ratio: {:.2}%",
        (lz4_compressed.len() as f64 / test_data.len() as f64) * 100.0
    );

    // Adaptive compression
    let (adaptive_compressed, method) = adaptive_compress(&test_data);
    let method_name = if method == 0 { "RLE" } else { "LZ4" };
    println!("   Adaptive compression:");
    println!("   - Method chosen: {}", method_name);
    println!("   - Compressed: {} bytes", adaptive_compressed.len());
    println!(
        "   - Ratio: {:.2}%",
        (adaptive_compressed.len() as f64 / test_data.len() as f64) * 100.0
    );

    // Example 4: Delta snapshots
    println!("\n6. Delta Snapshots:");

    let agent_id = [1u8; 16];
    let old_state = vec![0u8; 8192];
    let mut new_state = old_state.clone();

    // Modify some data
    new_state[100] = 0xFF;
    new_state[200] = 0xFF;
    new_state[4096] = 0xFF;

    let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1)
        .expect("Failed to create delta");

    println!("   ✓ Delta snapshot created");
    println!("   Base sequence: {}", delta.base_sequence);
    println!("   Current sequence: {}", delta.sequence);
    println!("   Dirty pages: {}", delta.dirty_pages.len());

    // Apply delta
    let mut restored_state = old_state.clone();
    delta
        .apply(&mut restored_state)
        .expect("Failed to apply delta");

    println!("   ✓ Delta applied");
    println!("   State matches: {}", restored_state == new_state);

    // Example 5: Dirty page tracking
    println!("\n7. Dirty Page Tracking:");

    let tracker_id = [2u8; 16];
    let state_size = 8192;
    let mut tracker = DirtyPageTracker::new(tracker_id, state_size);

    let initial_state = vec![0u8; state_size];
    tracker.initialize(&initial_state);

    println!("   ✓ Tracker initialized");
    println!("   State size: {} bytes", state_size);

    // Mark some pages dirty
    tracker.mark_dirty(0);
    tracker.mark_dirty(1);

    println!("   Marked 2 pages dirty");
    println!("   Dirty pages count: {}", tracker.dirty_count());

    // Example 6: Migration manager
    println!("\n8. Migration Manager:");

    let mut manager = MigrationManager::new();

    // Initiate migration
    let migration_agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let migration_snapshot = manager
        .initiate_migration(&migration_agent, None)
        .expect("Failed to initiate migration");

    println!("   ✓ Migration initiated");
    println!("   Agent ID: {:?}", migration_snapshot.agent_id);

    // Complete migration
    manager.complete_migration(&migration_snapshot.agent_id);
    println!("   ✓ Migration completed");

    // Example 7: Progress tracking
    println!("\n9. Progress Tracking:");

    let progress_id = [3u8; 16];
    let total_bytes = 1_000_000;
    let mut progress = MigrationProgressTracker::new(progress_id, total_bytes);

    println!("   Total bytes: {}", total_bytes);
    println!("   Initial progress: {}%", progress.progress_percent());

    // Simulate transfer progress
    progress.update_transfer_progress(250_000);
    println!("   After 250KB: {}%", progress.progress_percent());

    progress.update_transfer_progress(500_000);
    println!("   After 500KB: {}%", progress.progress_percent());

    progress.update_transfer_progress(1_000_000);
    println!("   After 1MB: {}%", progress.progress_percent());

    println!("   Summary:");
    println!("   - Final progress: {}%", progress.progress_percent());
    println!("   - Elapsed time: {} ms", progress.elapsed_ms());
    println!("   - Phase: {:?}", progress.phase());

    println!("\n=== Example Complete ===");
}
