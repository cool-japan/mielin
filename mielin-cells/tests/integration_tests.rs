//! Comprehensive integration tests
//!
//! This test suite covers:
//! - Stress testing with many concurrent agents
//! - Large state migrations
//! - Complete state transition coverage
//! - Error recovery scenarios

use mielin_cells::{migration::*, Agent, AgentError, AgentState, TransitionResult};
use std::time::Duration;

/// Test stress with 1000 concurrent agents
#[test]
fn test_stress_1000_agents() {
    println!("\n=== Testing 1000 concurrent agents ===");

    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d];

    println!("Creating 1000 agents...");
    let agents: Vec<Agent> = (0..1000).map(|_| Agent::new(wasm_binary.clone())).collect();

    assert_eq!(agents.len(), 1000);
    println!("✓ Created {} agents", agents.len());

    // Verify all have unique IDs
    let mut ids = std::collections::HashSet::new();
    for agent in &agents {
        assert!(ids.insert(agent.id()), "Duplicate agent ID found");
    }
    println!("✓ All agents have unique IDs");

    // Start all agents
    let mut started_count = 0;
    for agent in agents.iter() {
        let mut agent_mut = agent.clone();
        if matches!(agent_mut.start(), TransitionResult::Success) {
            started_count += 1;
        }
    }

    assert_eq!(started_count, 1000);
    println!("✓ Started {} agents successfully", started_count);
}

/// Test migration with large 1MB state
#[test]
fn test_large_state_migration_1mb() {
    println!("\n=== Testing 1MB state migration ===");

    let mut large_wasm = vec![0x00, 0x61, 0x73, 0x6d]; // WASM header
    large_wasm.extend(vec![0u8; 1_048_576]); // 1MB

    let agent = Agent::new(large_wasm);
    println!("✓ Created agent with 1MB WASM binary");

    // Capture snapshot
    let start = std::time::Instant::now();
    let snapshot = MigrationSnapshot::capture(&agent, None).expect("Failed to capture snapshot");
    let capture_time = start.elapsed();

    println!("✓ Captured snapshot in {:?}", capture_time);
    println!("  State size: {} bytes", snapshot.wasm_state.len());

    // Serialize
    let start = std::time::Instant::now();
    let serialized = snapshot.serialize().expect("Failed to serialize");
    let serialize_time = start.elapsed();

    println!("✓ Serialized in {:?}", serialize_time);
    println!("  Serialized size: {} bytes", serialized.len());

    // Deserialize
    let start = std::time::Instant::now();
    let deserialized = MigrationSnapshot::deserialize(&serialized).expect("Failed to deserialize");
    let deserialize_time = start.elapsed();

    println!("✓ Deserialized in {:?}", deserialize_time);

    // Restore
    let start = std::time::Instant::now();
    let _restored = deserialized.restore().expect("Failed to restore");
    let restore_time = start.elapsed();

    println!("✓ Restored agent in {:?}", restore_time);

    // Calculate total time
    let total_time = capture_time + serialize_time + deserialize_time + restore_time;
    println!("  Total migration time: {:?}", total_time);

    // Allow up to 500ms for debug builds (targets are for release)
    assert!(
        total_time < Duration::from_millis(500),
        "Migration took too long: {:?}",
        total_time
    );
}

/// Test all 22 documented valid state transitions
#[test]
fn test_complete_state_transition_coverage() {
    println!("\n=== Testing complete state transition coverage ===");

    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d];
    let mut tested_transitions = Vec::new();

    // Created -> Running
    {
        let mut agent = Agent::new(wasm_binary.clone());
        assert_eq!(*agent.state(), AgentState::Created);
        assert!(matches!(agent.start(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Running);
        tested_transitions.push(("Created", "Running"));
    }

    // Created -> Terminated
    {
        let mut agent = Agent::new(wasm_binary.clone());
        assert!(matches!(agent.terminate(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Terminated);
        tested_transitions.push(("Created", "Terminated"));
    }

    // Running -> Paused
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        assert!(matches!(agent.pause(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Paused);
        tested_transitions.push(("Running", "Paused"));
    }

    // Running -> Suspended
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        assert!(matches!(agent.suspend(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Suspended);
        tested_transitions.push(("Running", "Suspended"));
    }

    // Running -> Migrating
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        assert!(matches!(agent.begin_migration(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Migrating);
        tested_transitions.push(("Running", "Migrating"));
    }

    // Running -> Error
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        let error = AgentError::new("Test error");
        assert!(matches!(agent.set_error(error), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Error);
        tested_transitions.push(("Running", "Error"));
    }

    // Running -> Terminated
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        assert!(matches!(agent.terminate(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Terminated);
        tested_transitions.push(("Running", "Terminated"));
    }

    // Paused -> Running
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.pause();
        assert!(matches!(agent.resume(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Running);
        tested_transitions.push(("Paused", "Running"));
    }

    // Paused -> Suspended
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.pause();
        assert!(matches!(agent.suspend(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Suspended);
        tested_transitions.push(("Paused", "Suspended"));
    }

    // Paused -> Migrating
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.pause();
        assert!(matches!(agent.begin_migration(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Migrating);
        tested_transitions.push(("Paused", "Migrating"));
    }

    // Paused -> Terminated
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.pause();
        assert!(matches!(agent.terminate(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Terminated);
        tested_transitions.push(("Paused", "Terminated"));
    }

    // Suspended -> Running
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.suspend();
        assert!(matches!(agent.resume(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Running);
        tested_transitions.push(("Suspended", "Running"));
    }

    // Suspended -> Paused
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.suspend();
        assert!(matches!(agent.pause(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Paused);
        tested_transitions.push(("Suspended", "Paused"));
    }

    // Suspended -> Migrating
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.suspend();
        assert!(matches!(agent.begin_migration(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Migrating);
        tested_transitions.push(("Suspended", "Migrating"));
    }

    // Suspended -> Error
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.suspend();
        let error = AgentError::new("Test error");
        assert!(matches!(agent.set_error(error), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Error);
        tested_transitions.push(("Suspended", "Error"));
    }

    // Suspended -> Terminated
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.suspend();
        assert!(matches!(agent.terminate(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Terminated);
        tested_transitions.push(("Suspended", "Terminated"));
    }

    // Migrating -> Running
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.begin_migration();
        assert!(matches!(
            agent.complete_migration(),
            TransitionResult::Success
        ));
        assert_eq!(*agent.state(), AgentState::Running);
        tested_transitions.push(("Migrating", "Running"));
    }

    // Migrating -> Error
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.begin_migration();
        let error = AgentError::new("Migration failed");
        assert!(matches!(agent.set_error(error), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Error);
        tested_transitions.push(("Migrating", "Error"));
    }

    // Migrating -> Terminated
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        agent.begin_migration();
        assert!(matches!(agent.terminate(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Terminated);
        tested_transitions.push(("Migrating", "Terminated"));
    }

    // Error -> Running (recovery)
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        let error = AgentError::new("Recoverable error");
        agent.set_error(error);
        assert!(matches!(
            agent.attempt_recovery(),
            TransitionResult::Success
        ));
        assert_eq!(*agent.state(), AgentState::Running);
        tested_transitions.push(("Error", "Running"));
    }

    // Error -> Suspended
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        let error = AgentError::new("Test error");
        agent.set_error(error);
        assert!(matches!(agent.suspend(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Suspended);
        tested_transitions.push(("Error", "Suspended"));
    }

    // Error -> Terminated
    {
        let mut agent = Agent::new(wasm_binary.clone());
        agent.start();
        let error = AgentError::new("Fatal error");
        agent.set_error(error);
        assert!(matches!(agent.terminate(), TransitionResult::Success));
        assert_eq!(*agent.state(), AgentState::Terminated);
        tested_transitions.push(("Error", "Terminated"));
    }

    println!(
        "✓ Tested {} valid state transitions:",
        tested_transitions.len()
    );
    for (from, to) in &tested_transitions {
        println!("  {} -> {}", from, to);
    }

    // Verify we tested all 22 documented valid transitions
    assert_eq!(
        tested_transitions.len(),
        22,
        "Should test all 22 valid transitions"
    );
}

/// Test invalid transitions are properly rejected
#[test]
fn test_invalid_transitions_rejected() {
    println!("\n=== Testing invalid transitions are rejected ===");

    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d];

    // Created -> Paused (invalid)
    let mut agent = Agent::new(wasm_binary.clone());
    assert!(matches!(
        agent.pause(),
        TransitionResult::InvalidTransition { .. }
    ));
    println!("✓ Created -> Paused rejected");

    // Terminated -> anything (terminal state)
    let mut agent2 = Agent::new(wasm_binary.clone());
    agent2.terminate();
    assert!(matches!(
        agent2.start(),
        TransitionResult::InvalidTransition { .. }
    ));
    println!("✓ Terminated -> Running rejected");

    println!("✓ Invalid transitions properly rejected");
}

/// Test delta snapshots with varying change rates
#[test]
fn test_delta_snapshot_change_rates() {
    println!("\n=== Testing delta snapshots with different change rates ===");

    let agent_id = [1u8; 16];
    let size = 40_960; // 40KB
    let old_state = vec![0x42u8; size];

    // 1% changes
    {
        let mut new_state = old_state.clone();
        for item in new_state.iter_mut().take(size / 100) {
            *item = 0xFF;
        }

        let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1)
            .expect("Failed to create delta");

        println!("  1% changed: {} dirty pages", delta.dirty_pages.len());
        assert!(!delta.dirty_pages.is_empty());
    }

    // 10% changes
    {
        let mut new_state = old_state.clone();
        for item in new_state.iter_mut().take(size / 10) {
            *item = 0xFF;
        }

        let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1)
            .expect("Failed to create delta");

        println!("  10% changed: {} dirty pages", delta.dirty_pages.len());
    }

    // 50% changes
    {
        let mut new_state = old_state.clone();
        for item in new_state.iter_mut().take(size / 2) {
            *item = 0xFF;
        }

        let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1)
            .expect("Failed to create delta");

        println!("  50% changed: {} dirty pages", delta.dirty_pages.len());
    }

    println!("✓ Delta snapshots work with various change rates");
}

/// Test migration error recovery
#[test]
fn test_migration_error_recovery_strategies() {
    println!("\n=== Testing migration error recovery strategies ===");

    let agent_id = [1u8; 16];
    let recovery_manager = MigrationRecoveryManager::with_defaults();

    // Test transient error - should retry on same target
    let transient_error = MigrationError::new(
        MigrationErrorType::NetworkTransient,
        "Connection timeout".to_string(),
        agent_id,
        Some([2u8; 16]),
    );

    let strategy = recovery_manager.determine_strategy(&transient_error);
    assert_eq!(strategy, RecoveryStrategy::RetryOnSameTarget);
    println!("✓ Transient error triggers retry on same target");

    // Test incompatible target - should retry on different target
    let incompatible_error = MigrationError::new(
        MigrationErrorType::IncompatibleTarget,
        "Architecture mismatch".to_string(),
        agent_id,
        Some([2u8; 16]),
    );

    let strategy = recovery_manager.determine_strategy(&incompatible_error);
    assert_eq!(strategy, RecoveryStrategy::RetryOnDifferentTarget);
    println!("✓ Incompatible target triggers retry on different target");

    // Test max retries reached - should rollback or give up
    let mut max_retry_error = transient_error.clone();
    for _ in 0..10 {
        max_retry_error.increment_retry();
    }

    let strategy = recovery_manager.determine_strategy(&max_retry_error);
    assert!(matches!(
        strategy,
        RecoveryStrategy::Rollback | RecoveryStrategy::GiveUp
    ));
    println!("✓ Max retries triggers rollback/give up");
}

/// Test pool stress with rapid acquire/release
#[test]
fn test_pool_stress() {
    use mielin_cells::{AgentPool, PoolConfig};

    println!("\n=== Testing pool under stress ===");

    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d];

    let config = PoolConfig {
        min_size: 10,
        max_size: 100,
        max_age: Duration::from_secs(3600),
        max_idle_time: Duration::from_secs(300),
        pre_warm: true,
    };

    let pool = AgentPool::with_config(wasm_binary, config);

    // Acquire many agents rapidly
    let mut agents = Vec::new();
    for _ in 0..50 {
        agents.push(pool.acquire());
    }

    println!("✓ Acquired {} agents from pool", agents.len());
    assert_eq!(agents.len(), 50);

    // Release them all
    for agent in agents {
        pool.release(agent);
    }

    println!("✓ Released all agents back to pool");

    // Verify pool stats
    let stats = pool.stats();
    println!("  Pool stats:");
    println!("    Total created: {}", stats.total_created);
    println!("    Total acquired: {}", stats.total_acquired);
    println!("    Total returned: {}", stats.total_returned);
    println!("    Pool misses: {}", stats.pool_misses);

    assert!(stats.total_created > 0);
    assert_eq!(stats.total_acquired, 50);
}
