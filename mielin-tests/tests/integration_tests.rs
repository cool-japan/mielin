//! MielinOS End-to-End Integration Tests
//!
//! Comprehensive cross-crate integration tests covering:
//! - Mesh + Cells Integration
//! - Tensor + HAL Integration
//! - Kernel + Cells Integration
//! - Migration Flow
//! - Full Pipeline

use std::net::SocketAddr;
use std::sync::Arc;

// Import crates under test
use mielin_cells::{Agent, AgentError, AgentState, TransitionResult};
use mielin_hal::{
    capabilities::{HardwareCapabilities, HardwareProfile},
    detect_architecture,
};
use mielin_kernel::{
    memory::{AllocationStrategy, MemoryManager, MAX_PAGES, PAGE_SIZE},
    scheduler::{Scheduler, TaskState},
    KernelError,
};
use mielin_mesh_core::{
    node::{Node, NodeRole},
    service::{MeshConfig, MeshService},
};
use mielin_tensor::{ops::TensorOps, tensor::Tensor, TensorRuntime};

// =============================================================================
// Test Utilities and Setup
// =============================================================================

/// Test context for integration tests providing common setup
#[allow(dead_code)]
struct TestContext {
    node: Arc<Node>,
    scheduler: Scheduler,
    memory: MemoryManager,
}

#[allow(dead_code)]
impl TestContext {
    fn new() -> Result<Self, KernelError> {
        let node = Arc::new(Node::new(NodeRole::Core));
        let scheduler = Scheduler::new();
        let mut memory = MemoryManager::new();
        memory.init();

        Ok(Self {
            node,
            scheduler,
            memory,
        })
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Cleanup: terminate any remaining tasks
        // Memory is automatically cleaned up
    }
}

/// Creates a minimal WASM-like binary for testing agents
fn create_test_wasm_binary() -> Vec<u8> {
    // Minimal WASM magic number + version
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

/// Creates test agent state data for serialization tests
fn create_test_agent_state() -> Vec<u8> {
    // Simulated agent state: counter value + some metadata
    let counter: u64 = 42;
    let mut state = counter.to_le_bytes().to_vec();
    state.extend_from_slice(b"test_state_metadata");
    state
}

// =============================================================================
// Module 1: Mesh + Cells Integration Tests
// =============================================================================

mod mesh_cells_integration {
    use super::*;

    /// Test deploying an agent to a mesh node
    #[tokio::test]
    async fn test_agent_deployment_to_mesh_node() {
        // Create a mesh node
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: false,
            enable_registry: true,
            enable_migration: false,
        };

        // Create mesh service
        let service_result = MeshService::new(node.clone(), config);
        assert!(
            service_result.is_ok(),
            "Failed to create mesh service: {:?}",
            service_result.err()
        );

        let mut service = match service_result {
            Ok(s) => s,
            Err(e) => panic!("Service creation failed: {:?}", e),
        };

        // Start the service
        let start_result = service.start().await;
        assert!(
            start_result.is_ok(),
            "Failed to start mesh service: {:?}",
            start_result.err()
        );

        // Create an agent
        let wasm_binary = create_test_wasm_binary();
        let mut agent = Agent::new(wasm_binary);

        // Start the agent
        let start_transition = agent.start();
        assert!(
            matches!(start_transition, TransitionResult::Success),
            "Failed to start agent"
        );

        // Register agent with mesh registry
        let agent_id = agent.id();
        let mut agent_id_array = [0u8; 16];
        agent_id_array.copy_from_slice(agent_id.as_bytes());
        let agent_address: SocketAddr = "127.0.0.1:9000".parse().expect("valid address");

        let register_result = service.register_agent(agent_id_array, agent_address).await;
        assert!(
            register_result.is_ok(),
            "Failed to register agent: {:?}",
            register_result.err()
        );

        // Verify agent is registered
        let count_result = service.local_agent_count().await;
        assert!(
            count_result.is_ok(),
            "Failed to get agent count: {:?}",
            count_result.err()
        );
        assert_eq!(
            count_result.expect("count should be available"),
            1,
            "Agent count should be 1"
        );

        // Query agent location
        let location_result = service.query_agent(agent_id_array).await;
        assert!(
            location_result.is_ok(),
            "Failed to query agent: {:?}",
            location_result.err()
        );

        let location = location_result.expect("location should be available");
        assert_eq!(location.agent_id, agent_id_array);

        // Deregister and stop
        let deregister_result = service.deregister_agent(agent_id_array).await;
        assert!(deregister_result.is_ok());

        let stop_result = service.stop().await;
        assert!(stop_result.is_ok());
    }

    /// Test querying agent status through mesh
    #[tokio::test]
    async fn test_query_agent_status_through_mesh() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: true,
            enable_registry: true,
            enable_migration: false,
        };

        let mut service = match MeshService::new(node.clone(), config) {
            Ok(s) => s,
            Err(e) => panic!("Service creation failed: {:?}", e),
        };

        if let Err(e) = service.start().await {
            panic!("Service start failed: {:?}", e);
        }

        // Create multiple agents with different states
        let agents: Vec<(Agent, [u8; 16])> = (0..3)
            .map(|_| {
                let wasm = create_test_wasm_binary();
                let agent = Agent::new(wasm);
                let mut id_array = [0u8; 16];
                id_array.copy_from_slice(agent.id().as_bytes());
                (agent, id_array)
            })
            .collect();

        // Register all agents
        for (i, (_, agent_id)) in agents.iter().enumerate() {
            let port = 9100 + i as u16;
            let addr: SocketAddr = format!("127.0.0.1:{}", port)
                .parse()
                .expect("valid address");
            if let Err(e) = service.register_agent(*agent_id, addr).await {
                panic!("Registration failed: {:?}", e);
            }
        }

        // Query each agent
        for (_, agent_id) in &agents {
            let location = match service.query_agent(*agent_id).await {
                Ok(loc) => loc,
                Err(e) => panic!("Query failed: {:?}", e),
            };
            assert_eq!(&location.agent_id, agent_id);
        }

        // Verify total count
        let count = match service.local_agent_count().await {
            Ok(c) => c,
            Err(e) => panic!("Count failed: {:?}", e),
        };
        assert_eq!(count, 3);

        // Cleanup
        for (_, agent_id) in agents {
            let _ = service.deregister_agent(agent_id).await;
        }
        let _ = service.stop().await;
    }

    /// Test agent communication across mesh (gossip integration)
    #[tokio::test]
    async fn test_agent_communication_via_gossip() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: true,
            enable_registry: true,
            enable_migration: false,
        };

        let mut service = match MeshService::new(node.clone(), config) {
            Ok(s) => s,
            Err(e) => panic!("Service creation failed: {:?}", e),
        };

        if let Err(e) = service.start().await {
            panic!("Service start failed: {:?}", e);
        }

        // Check gossip membership (should have local node)
        let member_stats = match service.get_member_stats().await {
            Ok(stats) => stats,
            Err(e) => panic!("Stats failed: {:?}", e),
        };
        let (alive, suspect, dead) = member_stats;
        assert_eq!(alive, 1, "Should have local node as alive member");
        assert_eq!(suspect, 0, "No suspected members initially");
        assert_eq!(dead, 0, "No dead members initially");

        // Get alive members
        let alive_members = match service.get_alive_members().await {
            Ok(members) => members,
            Err(e) => panic!("Members failed: {:?}", e),
        };
        assert_eq!(alive_members.len(), 1);

        let _ = service.stop().await;
    }

    /// Test mesh service handles agent not found gracefully
    #[tokio::test]
    async fn test_mesh_agent_not_found_error() {
        let node = Arc::new(Node::new(NodeRole::Edge));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: false,
            enable_registry: true,
            enable_migration: false,
        };

        let mut service = match MeshService::new(node.clone(), config) {
            Ok(s) => s,
            Err(e) => panic!("Service creation failed: {:?}", e),
        };

        if let Err(e) = service.start().await {
            panic!("Service start failed: {:?}", e);
        }

        // Query non-existent agent
        let non_existent_id = [0xFFu8; 16];
        let query_result = service.query_agent(non_existent_id).await;

        // Should return an error
        assert!(
            query_result.is_err(),
            "Query for non-existent agent should fail"
        );

        let _ = service.stop().await;
    }
}

// =============================================================================
// Module 2: Tensor + HAL Integration Tests
// =============================================================================

mod tensor_hal_integration {
    use super::*;

    /// Test hardware detection followed by SIMD selection for tensor operations
    #[test]
    fn test_hardware_detection_and_simd_selection() {
        // Detect hardware profile
        let profile = HardwareProfile::detect();

        // Verify architecture detection
        let arch = detect_architecture();
        assert_eq!(profile.architecture, arch);

        // Create tensor runtime with detected capabilities
        let runtime = TensorRuntime::new(profile.capabilities);

        // Get acceleration info (should be descriptive string)
        let accel_info = runtime.acceleration_info();
        assert!(!accel_info.is_empty());

        // Verify SIMD selection based on capabilities
        if profile.capabilities.contains(HardwareCapabilities::SVE2) {
            assert!(runtime.supports_sve2());
        }
        if profile.capabilities.contains(HardwareCapabilities::NEON) {
            assert!(runtime.supports_neon());
        }
        if profile.capabilities.contains(HardwareCapabilities::AVX2) {
            assert!(runtime.supports_avx2());
        }
    }

    /// Test automatic backend selection for tensor operations
    #[test]
    fn test_automatic_backend_selection() {
        let profile = HardwareProfile::detect();

        // Test with different capability sets
        let test_cases = [
            HardwareCapabilities::NONE,
            HardwareCapabilities::NEON,
            HardwareCapabilities::AVX2,
            HardwareCapabilities::SVE2,
        ];

        for caps in test_cases {
            let ops = TensorOps::new(caps);

            // Create test tensors
            let a = Tensor::vector(vec![1.0, 2.0, 3.0, 4.0]);
            let b = Tensor::vector(vec![5.0, 6.0, 7.0, 8.0]);

            // Dot product should work with any backend
            let dot_result = ops.dot(&a, &b);
            assert!(
                dot_result.is_some(),
                "Dot product should succeed with {:?}",
                caps
            );

            let result = match dot_result {
                Some(r) => r,
                None => panic!("Dot product returned None"),
            };
            // 1*5 + 2*6 + 3*7 + 4*8 = 5 + 12 + 21 + 32 = 70
            assert!(
                (result - 70.0).abs() < 1e-6,
                "Dot product value should be 70.0"
            );

            // Element-wise add should work with any backend
            let add_result = ops.add(&a, &b);
            assert!(add_result.is_some(), "Add should succeed with {:?}", caps);
        }

        // Verify actual profile uses one of the valid backends
        let max_width = profile.max_vector_width();
        assert!(
            max_width == 0 || max_width == 128 || max_width == 256 || max_width == 512,
            "Vector width should be valid: {}",
            max_width
        );
    }

    /// Test tensor operations with hardware-accelerated backends
    #[test]
    fn test_tensor_operations_with_acceleration() {
        let profile = HardwareProfile::detect();
        let runtime = TensorRuntime::new(profile.capabilities);
        let ops = runtime.ops();

        // Test matrix multiplication
        let a = match Tensor::matrix(vec![1.0, 2.0, 3.0, 4.0], 2, 2) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };
        let b = match Tensor::matrix(vec![5.0, 6.0, 7.0, 8.0], 2, 2) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };

        let matmul_result = ops.matmul(&a, &b);
        assert!(matmul_result.is_some(), "Matmul should succeed");

        let result = match matmul_result {
            Some(r) => r,
            None => panic!("Matmul returned None"),
        };
        assert_eq!(result.shape(), &[2, 2]);

        // Expected: [[1*5+2*7, 1*6+2*8], [3*5+4*7, 3*6+4*8]]
        //         = [[19, 22], [43, 50]]
        let val_00 = result.get(&[0, 0]);
        assert!(val_00.is_some());
        assert!((val_00.expect("value exists") - 19.0).abs() < 1e-6);

        let val_11 = result.get(&[1, 1]);
        assert!(val_11.is_some());
        assert!((val_11.expect("value exists") - 50.0).abs() < 1e-6);
    }

    /// Test broadcasting with SIMD backends
    #[test]
    fn test_broadcasting_with_simd() {
        let profile = HardwareProfile::detect();
        let ops = TensorOps::new(profile.capabilities);

        // Matrix + scalar broadcast
        let matrix = match Tensor::matrix(vec![1.0, 2.0, 3.0, 4.0], 2, 2) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };
        let scalar = Tensor::vector(vec![10.0]);

        let result = ops.add(&matrix, &scalar);
        assert!(result.is_some(), "Broadcast add should succeed");

        let result_tensor = match result {
            Some(r) => r,
            None => panic!("Add returned None"),
        };
        let result_data = result_tensor.data().to_vec();
        assert_eq!(result_data, vec![11.0, 12.0, 13.0, 14.0]);
    }

    /// Test tensor operations handle dimension mismatch correctly
    #[test]
    fn test_tensor_dimension_mismatch_error() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        // Incompatible shapes for matmul
        let a = match Tensor::matrix(vec![1.0, 2.0, 3.0], 1, 3) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };
        let b = match Tensor::matrix(vec![1.0, 2.0], 1, 2) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };

        let result = ops.matmul(&a, &b);
        assert!(
            result.is_none(),
            "Matmul with incompatible dimensions should fail"
        );

        // Incompatible broadcast
        let x = match Tensor::matrix(vec![1.0; 12], 3, 4) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };
        let y = match Tensor::matrix(vec![1.0; 15], 3, 5) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };

        let add_result = ops.add(&x, &y);
        assert!(
            add_result.is_none(),
            "Broadcast with incompatible shapes should fail"
        );
    }

    /// Test cache topology affects tensor operation performance selection
    #[test]
    fn test_cache_aware_tensor_operations() {
        let profile = HardwareProfile::detect();

        // Verify cache sizes are detected
        assert!(
            profile.l1_cache_size > 0 || profile.l2_cache_size > 0,
            "At least one cache level should be detected"
        );

        // Create tensors that fit in L1 cache
        let small_size = profile.l1_cache_size.max(4096) / 4; // f32 elements
        let small_size = small_size.min(1024); // Cap for test performance
        let small_data: Vec<f32> = (0..small_size).map(|i| i as f32).collect();
        let small_tensor = Tensor::vector(small_data);

        // Operations on small tensors should still work
        let sum = small_tensor.sum();
        let expected_sum: f32 = (0..small_size).map(|i| i as f32).sum();
        assert!(
            (sum - expected_sum).abs() < 1.0,
            "Sum should match expected"
        );
    }
}

// =============================================================================
// Module 3: Kernel + Cells Integration Tests
// =============================================================================

mod kernel_cells_integration {
    use super::*;

    /// Test task scheduling for agent execution
    #[test]
    fn test_task_scheduling_for_agent() {
        let mut scheduler = Scheduler::new();

        // Create an agent
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);

        // Spawn a task for the agent with priority
        let agent_priority = 100u8;
        let spawn_result = scheduler.spawn_task(agent_priority);
        assert!(spawn_result.is_ok(), "Task spawn should succeed");

        let task_id = match spawn_result {
            Ok(id) => id,
            Err(e) => panic!("Task spawn failed: {:?}", e),
        };

        // Start the agent
        let start_result = agent.start();
        assert!(matches!(start_result, TransitionResult::Success));
        assert_eq!(agent.state(), &AgentState::Running);

        // Schedule the task
        let scheduled = scheduler.schedule();
        assert!(scheduled.is_some(), "Task should be scheduled");

        // Verify task is running
        let task = scheduler.get_task(task_id);
        assert!(task.is_some(), "Task should exist");
        assert_eq!(
            task.expect("task exists").state(),
            TaskState::Running,
            "Task should be running"
        );

        // Simulate agent work, then yield
        scheduler.yield_task();

        // Agent can be paused
        let pause_result = agent.pause();
        assert!(matches!(pause_result, TransitionResult::Success));

        // Terminate task and agent
        scheduler.terminate_task(task_id);
        let term_result = agent.terminate();
        assert!(matches!(term_result, TransitionResult::Success));

        // Verify cleanup
        assert!(scheduler.get_task(task_id).is_none());
        assert_eq!(agent.state(), &AgentState::Terminated);
    }

    /// Test memory allocation for agent state
    #[test]
    fn test_memory_allocation_for_agent_state() {
        let mut memory = MemoryManager::default();

        // Simulate agent state size (e.g., 16KB)
        let agent_state_pages = 4; // 4 * 4KB = 16KB

        // Allocate memory for agent state
        let alloc_result = memory.allocate_pages(agent_state_pages);
        assert!(alloc_result.is_ok(), "Memory allocation should succeed");

        let agent_memory = match alloc_result {
            Ok(addr) => addr,
            Err(e) => panic!("Allocation failed: {:?}", e),
        };
        assert_eq!(agent_memory % PAGE_SIZE, 0, "Memory should be page-aligned");

        // Verify allocation stats
        assert_eq!(memory.allocated_pages(), agent_state_pages);

        // Create agent with memory
        let wasm = create_test_wasm_binary();
        let agent = Agent::new(wasm);

        // Agent ID can reference the allocated memory (conceptually)
        let _agent_id = agent.id();
        let _memory_region = agent_memory;

        // Free memory when agent is done
        let free_result = memory.free_pages(agent_memory, agent_state_pages);
        assert!(free_result.is_ok(), "Memory free should succeed");

        assert_eq!(memory.allocated_pages(), 0);
    }

    /// Test multiple agent tasks with priority scheduling
    #[test]
    fn test_priority_scheduling_multiple_agents() {
        let mut scheduler = Scheduler::new();

        // Create agents with different priorities
        let low_priority = 10u8;
        let med_priority = 50u8;
        let high_priority = 100u8;

        let low_task = match scheduler.spawn_task(low_priority) {
            Ok(id) => id,
            Err(e) => panic!("Low priority spawn failed: {:?}", e),
        };
        let med_task = match scheduler.spawn_task(med_priority) {
            Ok(id) => id,
            Err(e) => panic!("Med priority spawn failed: {:?}", e),
        };
        let high_task = match scheduler.spawn_task(high_priority) {
            Ok(id) => id,
            Err(e) => panic!("High priority spawn failed: {:?}", e),
        };

        // First schedule should pick highest priority
        let first_scheduled = scheduler.schedule();
        assert!(first_scheduled.is_some());

        // The scheduled task should be high priority (verified via metrics/state)
        // The scheduler selected a task, and high priority tasks get selected first
        let _ = first_scheduled; // Consume the index

        // We can verify by checking that high priority task is now Running
        let high_task_status = scheduler.get_task(high_task);
        assert!(high_task_status.is_some());
        assert_eq!(
            high_task_status.expect("task exists").state(),
            TaskState::Running
        );

        // Yield and schedule again
        scheduler.yield_task();

        // Should pick high priority again (all ready)
        let _ = scheduler.schedule();
        scheduler.yield_task();

        // Cleanup
        scheduler.terminate_task(low_task);
        scheduler.terminate_task(med_task);
        scheduler.terminate_task(high_task);

        assert_eq!(scheduler.active_tasks(), 0);
    }

    /// Test memory fragmentation with agent lifecycle
    #[test]
    fn test_memory_fragmentation_agent_lifecycle() {
        let mut memory = MemoryManager::with_strategy(AllocationStrategy::BestFit);

        // Simulate multiple agent allocations
        let mut allocations = Vec::new();

        for i in 0..5 {
            let pages = (i % 3) + 2; // 2-4 pages each
            if let Ok(addr) = memory.allocate_pages(pages) {
                allocations.push((addr, pages));
            }
        }

        // Free every other allocation to create fragmentation
        for i in (0..allocations.len()).step_by(2) {
            let (addr, pages) = allocations[i];
            if let Err(e) = memory.free_pages(addr, pages) {
                panic!("Free failed: {:?}", e);
            }
        }

        // Count free blocks (fragmentation indicator)
        let free_blocks = memory.free_block_count();
        assert!(free_blocks > 0, "Should have free blocks");

        // New allocations should fit in gaps
        let new_alloc = memory.allocate_pages(2);
        assert!(
            new_alloc.is_ok(),
            "Should be able to allocate in fragmented memory"
        );

        // Defragment
        let _coalesced = memory.defragment();

        // Free remaining allocations
        for (i, (addr, pages)) in allocations.iter().enumerate() {
            if i % 2 != 0 {
                let _ = memory.free_pages(*addr, *pages);
            }
        }
        if let Ok(addr) = new_alloc {
            let _ = memory.free_pages(addr, 2);
        }
    }

    /// Test kernel error handling for agent failures
    #[test]
    fn test_kernel_error_handling() {
        let mut memory = MemoryManager::default();

        // Test zero allocation error
        let zero_result = memory.allocate_pages(0);
        assert!(matches!(zero_result, Err(KernelError::ZeroAllocation)));

        // Test allocation too large
        let huge_result = memory.allocate_pages(MAX_PAGES + 1);
        assert!(matches!(
            huge_result,
            Err(KernelError::AllocationTooLarge { .. })
        ));

        // Test double free
        let addr = match memory.allocate_page() {
            Ok(a) => a,
            Err(e) => panic!("Allocation failed: {:?}", e),
        };
        if let Err(e) = memory.free_page(addr) {
            panic!("First free failed: {:?}", e);
        }
        let double_free = memory.free_page(addr);
        assert!(matches!(double_free, Err(KernelError::DoubleFree { .. })));
    }
}

// =============================================================================
// Module 4: Migration Flow Tests
// =============================================================================

mod migration_flow {
    use super::*;
    use serde::{Deserialize, Serialize};

    /// Represents serializable agent state for migration
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct AgentMigrationState {
        agent_id: [u8; 16],
        state: String,
        counter: u64,
        checksum: u32,
    }

    impl AgentMigrationState {
        fn new(agent: &Agent, counter: u64) -> Self {
            let mut id_array = [0u8; 16];
            id_array.copy_from_slice(agent.id().as_bytes());

            let state = format!("{:?}", agent.state());
            let checksum = Self::compute_checksum(&id_array, &state, counter);

            Self {
                agent_id: id_array,
                state,
                counter,
                checksum,
            }
        }

        fn compute_checksum(id: &[u8; 16], state: &str, counter: u64) -> u32 {
            let mut sum: u32 = 0;
            for &b in id {
                sum = sum.wrapping_add(b as u32);
            }
            for b in state.bytes() {
                sum = sum.wrapping_add(b as u32);
            }
            sum = sum.wrapping_add((counter & 0xFFFFFFFF) as u32);
            sum
        }

        fn verify(&self) -> bool {
            let expected = Self::compute_checksum(&self.agent_id, &self.state, self.counter);
            self.checksum == expected
        }
    }

    /// Test create agent -> serialize state -> deserialize -> verify
    #[test]
    fn test_migration_serialize_deserialize() {
        // Create source agent
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);
        agent.start();
        assert_eq!(agent.state(), &AgentState::Running);

        // Create migration state
        let migration_state = AgentMigrationState::new(&agent, 42);

        // Serialize using serde_json
        let serialized = serde_json::to_vec(&migration_state);
        assert!(serialized.is_ok(), "Serialization should succeed");

        let bytes = match serialized {
            Ok(b) => b,
            Err(e) => panic!("Serialization failed: {:?}", e),
        };

        // Deserialize
        let deserialized: Result<AgentMigrationState, _> = serde_json::from_slice(&bytes);
        assert!(deserialized.is_ok(), "Deserialization should succeed");

        let restored_state = match deserialized {
            Ok(s) => s,
            Err(e) => panic!("Deserialization failed: {:?}", e),
        };

        // Verify integrity
        assert!(restored_state.verify(), "Checksum should match");
        assert_eq!(restored_state.agent_id, migration_state.agent_id);
        assert_eq!(restored_state.counter, 42);
        assert_eq!(restored_state.state, "Running");
    }

    /// Test full migration flow: source -> serialize -> target -> verify
    #[tokio::test]
    async fn test_full_migration_flow() {
        // Source node
        let source_node = Arc::new(Node::new(NodeRole::Core));
        let source_config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: false,
            enable_registry: true,
            enable_migration: true,
        };

        let mut source_service = match MeshService::new(source_node.clone(), source_config) {
            Ok(s) => s,
            Err(e) => panic!("Source service creation failed: {:?}", e),
        };

        if let Err(e) = source_service.start().await {
            panic!("Source start failed: {:?}", e);
        }

        // Create agent on source
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);
        agent.start();

        let mut agent_id_array = [0u8; 16];
        agent_id_array.copy_from_slice(agent.id().as_bytes());

        // Register on source
        if let Err(e) = source_service
            .register_agent(agent_id_array, "127.0.0.1:9000".parse().expect("addr"))
            .await
        {
            panic!("Register failed: {:?}", e);
        }

        // Begin migration on agent
        let begin_result = agent.begin_migration();
        assert!(matches!(begin_result, TransitionResult::Success));
        assert_eq!(agent.state(), &AgentState::Migrating);

        // Serialize agent state
        let migration_state = AgentMigrationState::new(&agent, 100);
        let serialized = match serde_json::to_vec(&migration_state) {
            Ok(b) => b,
            Err(e) => panic!("Serialize failed: {:?}", e),
        };

        // Deregister from source
        if let Err(e) = source_service.deregister_agent(agent_id_array).await {
            panic!("Deregister failed: {:?}", e);
        }

        // --- Target side ---

        // Target node
        let target_node = Arc::new(Node::new(NodeRole::Core));
        let target_config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: false,
            enable_registry: true,
            enable_migration: true,
        };

        let mut target_service = match MeshService::new(target_node.clone(), target_config) {
            Ok(s) => s,
            Err(e) => panic!("Target service creation failed: {:?}", e),
        };

        if let Err(e) = target_service.start().await {
            panic!("Target start failed: {:?}", e);
        }

        // Deserialize on target
        let restored_state: AgentMigrationState = match serde_json::from_slice(&serialized) {
            Ok(s) => s,
            Err(e) => panic!("Deserialize failed: {:?}", e),
        };
        assert!(restored_state.verify(), "State should be valid");

        // Create new agent on target (in real scenario, would restore from wasm)
        let target_wasm = create_test_wasm_binary();
        let mut target_agent = Agent::new(target_wasm);

        // Complete migration - agent resumes running
        target_agent.start();
        assert_eq!(target_agent.state(), &AgentState::Running);

        // Register on target
        if let Err(e) = target_service
            .register_agent(agent_id_array, "127.0.0.1:9001".parse().expect("addr"))
            .await
        {
            panic!("Register on target failed: {:?}", e);
        }

        // Verify state is preserved
        assert_eq!(restored_state.counter, 100);
        assert_eq!(restored_state.agent_id, agent_id_array);

        // Cleanup
        let _ = source_service.stop().await;
        let _ = target_service.stop().await;
    }

    /// Test migration error handling - corrupted state
    #[test]
    fn test_migration_corrupted_state_detection() {
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);
        agent.start();

        let mut migration_state = AgentMigrationState::new(&agent, 50);

        // Corrupt the checksum
        migration_state.checksum = migration_state.checksum.wrapping_add(1);

        // Verification should fail
        assert!(
            !migration_state.verify(),
            "Corrupted state should fail verification"
        );
    }

    /// Test migration with memory allocation
    #[test]
    fn test_migration_with_memory_allocation() {
        let mut source_memory = MemoryManager::default();
        let mut target_memory = MemoryManager::default();

        // Allocate memory for agent state on source
        let state_pages = 4;
        let source_addr = match source_memory.allocate_pages(state_pages) {
            Ok(a) => a,
            Err(e) => panic!("Source allocation failed: {:?}", e),
        };

        // Simulate state data
        let state_data = create_test_agent_state();

        // Free source memory (agent left)
        if let Err(e) = source_memory.free_pages(source_addr, state_pages) {
            panic!("Source free failed: {:?}", e);
        }

        // Allocate on target
        let target_addr = match target_memory.allocate_pages(state_pages) {
            Ok(a) => a,
            Err(e) => panic!("Target allocation failed: {:?}", e),
        };

        assert_eq!(target_addr % PAGE_SIZE, 0);
        assert_eq!(target_memory.allocated_pages(), state_pages);

        // State data would be copied here
        let _ = state_data;

        // Cleanup
        if let Err(e) = target_memory.free_pages(target_addr, state_pages) {
            panic!("Target free failed: {:?}", e);
        }
    }
}

// =============================================================================
// Module 5: Full Pipeline Tests
// =============================================================================

mod full_pipeline {
    use super::*;

    /// Test complete pipeline: hardware detection -> mesh setup -> agent deployment -> tensor compute -> results
    #[tokio::test]
    async fn test_full_pipeline_integration() {
        // Step 1: Hardware Detection
        let profile = HardwareProfile::detect();
        let arch = detect_architecture();
        assert_eq!(profile.architecture, arch);

        // Step 2: Initialize Tensor Runtime with detected capabilities
        let runtime = TensorRuntime::new(profile.capabilities);
        let ops = runtime.ops();

        // Step 3: Mesh Setup
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: true,
            enable_registry: true,
            enable_migration: false,
        };

        let mut mesh_service = match MeshService::new(node.clone(), config) {
            Ok(s) => s,
            Err(e) => panic!("Mesh service creation failed: {:?}", e),
        };

        if let Err(e) = mesh_service.start().await {
            panic!("Mesh service start failed: {:?}", e);
        }

        // Step 4: Agent Deployment
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);
        agent.start();

        let mut agent_id = [0u8; 16];
        agent_id.copy_from_slice(agent.id().as_bytes());

        if let Err(e) = mesh_service
            .register_agent(agent_id, "127.0.0.1:9999".parse().expect("addr"))
            .await
        {
            panic!("Agent registration failed: {:?}", e);
        }

        // Step 5: Tensor Compute (agent workload)
        let input_a = Tensor::vector(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let input_b = Tensor::vector(vec![8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);

        // Dot product: 1*8 + 2*7 + 3*6 + 4*5 + 5*4 + 6*3 + 7*2 + 8*1
        //            = 8 + 14 + 18 + 20 + 20 + 18 + 14 + 8 = 120
        let dot_result = ops.dot(&input_a, &input_b);
        assert!(dot_result.is_some());
        let dot_value = match dot_result {
            Some(v) => v,
            None => panic!("Dot product failed"),
        };
        assert!(
            (dot_value - 120.0).abs() < 1e-6,
            "Dot product should be 120"
        );

        // Element-wise operations
        let sum_result = ops.add(&input_a, &input_b);
        assert!(sum_result.is_some());
        let sum_tensor = match sum_result {
            Some(t) => t,
            None => panic!("Add failed"),
        };
        assert_eq!(sum_tensor.data(), &[9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0]);

        // Step 6: Results Verification
        let agent_count = match mesh_service.local_agent_count().await {
            Ok(c) => c,
            Err(e) => panic!("Count failed: {:?}", e),
        };
        assert_eq!(agent_count, 1);

        let member_stats = match mesh_service.get_member_stats().await {
            Ok(s) => s,
            Err(e) => panic!("Stats failed: {:?}", e),
        };
        let (alive_members, _, _) = member_stats;
        assert_eq!(alive_members, 1);

        // Cleanup
        if let Err(e) = mesh_service.deregister_agent(agent_id).await {
            panic!("Deregister failed: {:?}", e);
        }
        agent.terminate();

        if let Err(e) = mesh_service.stop().await {
            panic!("Stop failed: {:?}", e);
        }

        assert_eq!(agent.state(), &AgentState::Terminated);
    }

    /// Test pipeline with error recovery
    #[tokio::test]
    async fn test_pipeline_error_recovery() {
        // Hardware detection
        let profile = HardwareProfile::detect();
        let runtime = TensorRuntime::new(profile.capabilities);
        let ops = runtime.ops();

        // Create agent
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);
        agent.start();

        // Simulate error during compute
        let error = AgentError::new("Compute failed").with_code(500);
        let error_result = agent.set_error(error);
        assert!(matches!(error_result, TransitionResult::Success));
        assert_eq!(agent.state(), &AgentState::Error);

        // Attempt recovery
        let recovery_result = agent.attempt_recovery();
        assert!(matches!(recovery_result, TransitionResult::Success));
        assert_eq!(agent.state(), &AgentState::Running);

        // Resume compute
        let a = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let b = Tensor::vector(vec![4.0, 5.0, 6.0]);
        let result = ops.dot(&a, &b);
        assert!(result.is_some());
        let dot_value = match result {
            Some(v) => v,
            None => panic!("Dot product failed"),
        };
        assert!((dot_value - 32.0).abs() < 1e-6);

        agent.terminate();
    }

    /// Test pipeline with scheduler integration
    #[test]
    fn test_pipeline_with_scheduler() {
        // Initialize kernel components
        let mut scheduler = Scheduler::new();
        let mut memory = MemoryManager::default();

        // Create agent
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);

        // Allocate memory for agent
        let mem_pages = 2;
        let mem_addr = match memory.allocate_pages(mem_pages) {
            Ok(a) => a,
            Err(e) => panic!("Alloc failed: {:?}", e),
        };

        // Spawn task for agent
        let task_id = match scheduler.spawn_task(100) {
            Ok(id) => id,
            Err(e) => panic!("Spawn failed: {:?}", e),
        };

        // Start agent
        agent.start();

        // Schedule and run
        let scheduled = scheduler.schedule();
        assert!(scheduled.is_some());

        // Create tensor runtime and perform compute
        let runtime = TensorRuntime::new(HardwareCapabilities::NONE);
        let _ops = runtime.ops();

        let input = Tensor::vector(vec![1.0, 1.0, 1.0, 1.0]);
        let sum = input.sum();
        assert!((sum - 4.0).abs() < 1e-6);

        // Yield and cleanup
        scheduler.yield_task();
        scheduler.terminate_task(task_id);
        if let Err(e) = memory.free_pages(mem_addr, mem_pages) {
            panic!("Free failed: {:?}", e);
        }
        agent.terminate();

        // Verify cleanup
        assert_eq!(scheduler.active_tasks(), 0);
        assert_eq!(memory.allocated_pages(), 0);
    }

    /// Test pipeline performance characteristics
    #[test]
    fn test_pipeline_performance_characteristics() {
        let profile = HardwareProfile::detect();

        // Record performance-related metrics
        let core_count = profile.core_count;
        let memory_size = profile.memory_size;
        let l1_cache = profile.l1_cache_size;
        let vector_width = profile.max_vector_width();

        assert!(core_count > 0, "Should detect cores");
        assert!(
            memory_size > 0 || l1_cache > 0,
            "Should detect some memory info"
        );

        // Create tensors sized for cache efficiency
        let tensor_elements = if l1_cache > 0 {
            (l1_cache / 4).min(1024) // Fit in L1, cap at 1024
        } else {
            256 // Default
        };

        let data: Vec<f32> = (0..tensor_elements).map(|i| (i as f32) * 0.001).collect();
        let tensor = Tensor::vector(data);

        // Operations should complete quickly for cache-sized data
        let sum = tensor.sum();
        assert!(sum >= 0.0, "Sum should be computed");

        let mean = tensor.mean();
        assert!(mean >= 0.0, "Mean should be computed");

        // Log performance info (would go to tracing in real use)
        let _ = (core_count, vector_width);
    }

    /// Test concurrent operations in pipeline
    #[tokio::test]
    async fn test_pipeline_concurrent_operations() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: true,
            enable_registry: true,
            enable_migration: false,
        };

        let mut service = match MeshService::new(node.clone(), config) {
            Ok(s) => s,
            Err(e) => panic!("Service creation failed: {:?}", e),
        };

        if let Err(e) = service.start().await {
            panic!("Start failed: {:?}", e);
        }

        // Spawn multiple concurrent tasks
        let mut handles = Vec::new();

        for i in 0..5 {
            let i = i;
            let handle = tokio::spawn(async move {
                let runtime = TensorRuntime::new(HardwareCapabilities::NONE);
                let ops = runtime.ops();

                let a = Tensor::vector(vec![i as f32; 100]);
                let b = Tensor::vector(vec![1.0; 100]);

                let result = ops.add(&a, &b);
                result.is_some()
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            let result = handle.await;
            assert!(result.is_ok());
            assert!(result.expect("task result"));
        }

        if let Err(e) = service.stop().await {
            panic!("Stop failed: {:?}", e);
        }
    }
}

// =============================================================================
// Module 6: Error Path Tests
// =============================================================================

mod error_paths {
    use super::*;

    /// Test mesh service not started errors
    #[tokio::test]
    async fn test_mesh_service_not_started_errors() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse::<SocketAddr>().expect("valid address"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: true,
            enable_registry: true,
            enable_migration: false,
        };

        let service = match MeshService::new(node.clone(), config) {
            Ok(s) => s,
            Err(e) => panic!("Service creation failed: {:?}", e),
        };

        // Service not started - all operations should fail
        let peers_result = service.get_peers().await;
        assert!(
            peers_result.is_err(),
            "get_peers should fail when not started"
        );

        let agent_id = [0u8; 16];
        let register_result = service
            .register_agent(agent_id, "127.0.0.1:9000".parse().expect("addr"))
            .await;
        assert!(
            register_result.is_err(),
            "register should fail when not started"
        );

        let query_result = service.query_agent(agent_id).await;
        assert!(query_result.is_err(), "query should fail when not started");
    }

    /// Test agent state transition errors
    #[test]
    fn test_agent_invalid_state_transitions() {
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);

        // Cannot pause from Created state
        let pause_result = agent.pause();
        assert!(
            matches!(pause_result, TransitionResult::InvalidTransition { .. }),
            "Cannot pause from Created"
        );

        // Note: resume() transitions to Running, which IS valid from Created
        // (same as start()), so we test a different invalid transition instead

        // Start agent
        agent.start();
        assert_eq!(agent.state(), &AgentState::Running);

        // Cannot start again (already running)
        let start_again = agent.start();
        assert!(
            matches!(start_again, TransitionResult::InvalidTransition { .. }),
            "Cannot start already running agent"
        );

        // Pause the agent
        agent.pause();
        assert_eq!(agent.state(), &AgentState::Paused);

        // Cannot error from Paused (invalid transition)
        let error_from_paused = agent.set_error(AgentError::new("test"));
        assert!(
            matches!(
                error_from_paused,
                TransitionResult::InvalidTransition { .. }
            ),
            "Cannot set error from Paused"
        );

        // Resume and then terminate
        agent.resume();
        agent.terminate();
        assert_eq!(agent.state(), &AgentState::Terminated);

        // Cannot do anything from Terminated
        let from_terminated = agent.start();
        assert!(
            matches!(from_terminated, TransitionResult::InvalidTransition { .. }),
            "Cannot transition from Terminated"
        );
    }

    /// Test memory allocation errors
    #[test]
    fn test_memory_allocation_errors() {
        let mut memory = MemoryManager::default();

        // Exhaust memory
        let all_pages = memory.allocate_pages(MAX_PAGES);
        assert!(all_pages.is_ok());

        // Should fail - no memory left
        let one_more = memory.allocate_page();
        assert!(matches!(one_more, Err(KernelError::OutOfMemory { .. })));

        // Free some and try again
        let addr = match all_pages {
            Ok(a) => a,
            Err(e) => panic!("All pages allocation failed: {:?}", e),
        };
        if let Err(e) = memory.free_pages(addr, 10) {
            panic!("Free failed: {:?}", e);
        }

        let retry = memory.allocate_pages(5);
        assert!(retry.is_ok(), "Should succeed after freeing memory");
    }

    /// Test scheduler task limit errors
    #[test]
    fn test_scheduler_task_limit() {
        let mut scheduler = Scheduler::new();

        // Spawn max tasks
        let max_tasks = 64; // MAX_TASKS from scheduler
        let mut task_ids = Vec::new();

        for _ in 0..max_tasks {
            if let Ok(id) = scheduler.spawn_task(50) {
                task_ids.push(id);
            }
        }

        // Should have spawned max tasks
        assert_eq!(task_ids.len(), max_tasks);

        // Next spawn should fail
        let overflow = scheduler.spawn_task(100);
        assert!(
            matches!(overflow, Err(KernelError::TaskSpawnFailed)),
            "Should fail when task slots exhausted"
        );

        // Cleanup
        for id in task_ids {
            scheduler.terminate_task(id);
        }
    }

    /// Test tensor operation errors
    #[test]
    fn test_tensor_operation_errors() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        // Dot product requires 1D tensors
        let matrix = match Tensor::matrix(vec![1.0, 2.0, 3.0, 4.0], 2, 2) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };
        let vector = Tensor::vector(vec![1.0, 2.0]);

        let dot_with_matrix = ops.dot(&matrix, &vector);
        assert!(dot_with_matrix.is_none(), "Dot product requires 1D tensors");

        // Dot product requires same length
        let a = Tensor::vector(vec![1.0, 2.0]);
        let b = Tensor::vector(vec![1.0, 2.0, 3.0]);

        let dot_diff_len = ops.dot(&a, &b);
        assert!(dot_diff_len.is_none(), "Dot product requires same length");

        // Matmul requires compatible dimensions
        let m1 = match Tensor::matrix(vec![1.0, 2.0, 3.0], 1, 3) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };
        let m2 = match Tensor::matrix(vec![1.0, 2.0], 1, 2) {
            Some(m) => m,
            None => panic!("Matrix creation failed"),
        };

        let matmul_incompatible = ops.matmul(&m1, &m2);
        assert!(
            matmul_incompatible.is_none(),
            "Matmul requires compatible dimensions"
        );
    }

    /// Test fatal agent error - no recovery
    #[test]
    fn test_fatal_agent_error_no_recovery() {
        let wasm = create_test_wasm_binary();
        let mut agent = Agent::new(wasm);
        agent.start();

        // Set fatal error
        let fatal_error = AgentError::new("Fatal error").fatal();
        agent.set_error(fatal_error);

        assert_eq!(agent.state(), &AgentState::Error);

        // Attempt recovery should be blocked
        let recovery = agent.attempt_recovery();
        assert!(
            matches!(recovery, TransitionResult::Blocked { .. }),
            "Fatal errors should block recovery"
        );

        // Agent should still be in Error state
        assert_eq!(agent.state(), &AgentState::Error);

        // But can still terminate
        let terminate = agent.terminate();
        assert!(matches!(terminate, TransitionResult::Success));
    }
}
