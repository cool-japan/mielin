//! Agent Migration Example
//!
//! Demonstrates the "Saltatory Conduction" (跳躍伝導) - ultra-fast agent migration
//! between nodes in the MielinMesh network.

use mielin_cells::{
    migration::{MigrationManager, MigrationSnapshot},
    Agent,
};
use mielin_hal::capabilities::HardwareProfile;
use mielin_mesh_core::{Node, NodeRole};
use mielin_mesh_wire::Message;

#[tokio::main]
async fn main() {
    println!("=== MielinOS: Agent Migration Demo ===\n");

    println!("Phase 1: Creating source and target nodes");
    let source_node = Node::new(NodeRole::Edge);
    let target_node = Node::new(NodeRole::Core);

    println!("  Source Node ID: {}", source_node.id());
    println!("  Target Node ID: {}", target_node.id());
    println!("  Source Role: {:?}", source_node.role());
    println!("  Target Role: {:?}\n", target_node.role());

    println!("Phase 2: Detecting hardware capabilities");
    let hw_profile = HardwareProfile::detect();
    println!("  Architecture: {}", hw_profile.architecture);
    println!("  CPU Cores: {}", hw_profile.core_count);
    println!("  SIMD Support: {}", hw_profile.has_simd());
    println!("  Tensor Ops: {}", hw_profile.supports_tensor_ops());
    println!(
        "  Max Vector Width: {} bits\n",
        hw_profile.max_vector_width()
    );

    println!("Phase 3: Creating agent with WASM binary");
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let agent = Agent::new(wasm_binary);
    println!("  Agent ID: {}", agent.id());
    println!("  Agent State: {:?}", agent.state());
    println!("  DNA Hash: {:x?}\n", agent.dna().hash());

    println!("Phase 4: Initiating migration (Saltatory Conduction)");
    let mut migration_manager = MigrationManager::new();

    let snapshot = migration_manager
        .initiate_migration(&agent, Some(*target_node.id().as_bytes()))
        .expect("Failed to create migration snapshot");

    println!("  Snapshot Size: {} bytes", snapshot.size_bytes());
    println!("  Timestamp: {}", snapshot.timestamp);
    println!(
        "  Pending Migrations: {}\n",
        migration_manager.pending_count()
    );

    println!("Phase 5: Serializing snapshot for network transfer");
    let serialized = snapshot.serialize().expect("Failed to serialize snapshot");
    println!("  Serialized Size: {} bytes\n", serialized.len());

    println!("Phase 6: Creating migration message");
    let migration_msg = Message::AgentMigration {
        agent_id: snapshot.agent_id,
        snapshot: serialized.clone(),
        priority: 10,
    };

    println!("  Message Type: AgentMigration");
    println!("  Is Critical: {}", migration_msg.is_critical());
    println!("  Requires ACK: {}\n", migration_msg.requires_ack());

    println!("Phase 7: Simulating network transfer");
    let msg_bytes = migration_msg
        .serialize()
        .expect("Failed to serialize message");
    println!("  Total Message Size: {} bytes\n", msg_bytes.len());

    println!("Phase 8: Deserializing on target node");
    let received_msg = Message::deserialize(&msg_bytes).expect("Failed to deserialize message");

    if let Message::AgentMigration {
        agent_id,
        snapshot: received_snapshot,
        priority,
    } = received_msg
    {
        println!("  Received Agent ID: {:x?}", agent_id);
        println!("  Priority: {}", priority);

        let restored_snapshot = MigrationSnapshot::deserialize(&received_snapshot)
            .expect("Failed to deserialize snapshot");

        println!(
            "  Snapshot Age: {} seconds\n",
            restored_snapshot.age_seconds()
        );

        println!("Phase 9: Restoring agent on target node");
        let restored_agent = restored_snapshot
            .restore()
            .expect("Failed to restore agent");

        println!("  Restored Agent ID: {}", restored_agent.id());
        println!("  Restored State: {:?}", restored_agent.state());
        println!(
            "  DNA Match: {}\n",
            restored_agent.dna().binary() == agent.dna().binary()
        );

        println!("Phase 10: Sending migration acknowledgment");
        let ack_msg = Message::MigrationAck {
            agent_id,
            success: true,
            error_msg: None,
        };

        let ack_bytes = ack_msg.serialize().expect("Failed to serialize ACK");
        println!("  ACK Size: {} bytes", ack_bytes.len());

        migration_manager.complete_migration(&agent_id);
        println!("  Migration Completed");
        println!(
            "  Pending Migrations: {}\n",
            migration_manager.pending_count()
        );
    }

    println!("=== Migration Complete: Saltatory Conduction Successful ===");
    println!("\nAgent successfully migrated from Edge to Core node");
    println!("This demonstrates MielinOS's ability to perform ultra-fast");
    println!("agent migration across heterogeneous hardware platforms.");
}
