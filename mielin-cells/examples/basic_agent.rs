//! Basic agent lifecycle example
//!
//! This example demonstrates:
//! - Creating an agent
//! - Managing agent state transitions
//! - Setting policies
//! - Error handling

use mielin_cells::{Agent, AgentError, Policy, TransitionResult};

fn main() {
    println!("=== Basic Agent Lifecycle Example ===\n");

    // Create a new agent with a simple WASM binary
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d]; // WASM header
    let mut agent = Agent::new(wasm_binary);

    println!("1. Agent created with ID: {}", agent.id());
    println!("   Initial state: {:?}", agent.state());

    // Set a custom policy
    let policy = Policy {
        min_battery_percent: 30,
        max_latency_ms: 50,
        preferred_architectures: vec!["x86_64".to_string(), "aarch64".to_string()],
    };
    agent.set_policy(policy);
    println!("\n2. Policy set:");
    println!("   Min battery: {}%", agent.policy().min_battery_percent);
    println!("   Max latency: {}ms", agent.policy().max_latency_ms);

    // Start the agent
    println!("\n3. Starting agent...");
    match agent.start() {
        TransitionResult::Success => {
            println!("   ✓ Agent started successfully");
            println!("   Current state: {:?}", agent.state());
        }
        TransitionResult::InvalidTransition { from, to } => {
            println!("   ✗ Invalid transition from {:?} to {:?}", from, to);
        }
        TransitionResult::Blocked { reason } => {
            println!("   ✗ Transition blocked: {}", reason);
        }
    }

    // Pause and resume
    println!("\n4. Pausing agent...");
    if let TransitionResult::Success = agent.pause() {
        println!("   ✓ Agent paused");
        println!("   Time in paused state: {:?}", agent.time_in_state());
    }

    println!("\n5. Resuming agent...");
    if let TransitionResult::Success = agent.resume() {
        println!("   ✓ Agent resumed");
    }

    // Simulate an error
    println!("\n6. Simulating error...");
    let error = AgentError::new("Simulated computation error").with_code(1001);
    if let TransitionResult::Success = agent.set_error(error) {
        println!("   ✓ Error state set");
        if let Some(err) = agent.error() {
            println!("   Error message: {}", err.message);
            println!("   Error code: {:?}", err.code);
        }
    }

    // Attempt recovery
    println!("\n7. Attempting recovery...");
    match agent.attempt_recovery() {
        TransitionResult::Success => {
            println!("   ✓ Recovery successful");
            println!("   Current state: {:?}", agent.state());
        }
        TransitionResult::Blocked { reason } => {
            println!("   ✗ Recovery blocked: {}", reason);
        }
        _ => {}
    }

    // Terminate agent
    println!("\n8. Terminating agent...");
    if let TransitionResult::Success = agent.terminate() {
        println!("   ✓ Agent terminated");
        println!("   Final state: {:?}", agent.state());
    }

    // Show state history
    println!("\n9. State history:");
    for (state, _instant) in agent.state_history() {
        println!("   - {:?}", state);
    }

    println!("\n=== Example Complete ===");
}
