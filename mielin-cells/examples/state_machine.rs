//! Agent State Machine example
//!
//! Demonstrates:
//! - Agent lifecycle states
//! - Valid and invalid state transitions
//! - State transition hooks for monitoring
//! - Error states and recovery
//! - State history tracking
//! - Time tracking in states

use mielin_cells::{Agent, AgentError, AgentState, TransitionResult};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    println!("=== MielinOS Agent State Machine ===\n");

    // Example 1: Basic state transitions
    basic_state_transitions();

    // Example 2: Invalid transitions
    invalid_transitions();

    // Example 3: State validation helpers
    state_validation_helpers();

    // Example 4: Error handling and recovery
    error_handling_and_recovery();

    // Example 5: State history tracking
    state_history_tracking();

    // Example 6: Time tracking
    time_tracking();

    // Example 7: State transition hooks
    state_transition_hooks();

    // Example 8: Complex lifecycle simulation
    complex_lifecycle();
}

/// Example 1: Basic valid state transitions
fn basic_state_transitions() {
    println!("1. Basic State Transitions");
    println!("--------------------------");

    let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    println!("Agent created: {}", agent.id());
    println!("Initial state: {:?}", agent.state());

    // Created -> Running
    println!("\nStarting agent...");
    match agent.start() {
        TransitionResult::Success => {
            println!("✓ Transitioned to: {:?}", agent.state());
        }
        _ => println!("✗ Transition failed"),
    }

    // Running -> Paused
    println!("\nPausing agent...");
    match agent.pause() {
        TransitionResult::Success => {
            println!("✓ Transitioned to: {:?}", agent.state());
        }
        _ => println!("✗ Transition failed"),
    }

    // Paused -> Running
    println!("\nResuming agent...");
    match agent.resume() {
        TransitionResult::Success => {
            println!("✓ Transitioned to: {:?}", agent.state());
        }
        _ => println!("✗ Transition failed"),
    }

    // Running -> Suspended
    println!("\nSuspending agent (system)...");
    match agent.suspend() {
        TransitionResult::Success => {
            println!("✓ Transitioned to: {:?}", agent.state());
        }
        _ => println!("✗ Transition failed"),
    }

    // Suspended -> Running
    println!("\nResuming from suspension...");
    match agent.transition_to(AgentState::Running) {
        TransitionResult::Success => {
            println!("✓ Transitioned to: {:?}", agent.state());
        }
        _ => println!("✗ Transition failed"),
    }

    // Running -> Terminated
    println!("\nTerminating agent...");
    match agent.terminate() {
        TransitionResult::Success => {
            println!("✓ Transitioned to: {:?}", agent.state());
        }
        _ => println!("✗ Transition failed"),
    }

    println!();
}

/// Example 2: Demonstrating invalid transitions
fn invalid_transitions() {
    println!("2. Invalid Transitions");
    println!("----------------------");

    let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    println!("Agent state: {:?}", agent.state());

    // Try to pause without starting (Created -> Paused is invalid)
    println!("\nAttempting invalid transition: Created -> Paused");
    match agent.pause() {
        TransitionResult::Success => println!("✓ Success"),
        TransitionResult::InvalidTransition { from, to } => {
            println!("✗ Invalid transition from {:?} to {:?}", from, to);
        }
        TransitionResult::Blocked { reason } => {
            println!("✗ Blocked: {}", reason);
        }
    }

    // Start the agent
    agent.start();
    println!("\nAgent started: {:?}", agent.state());

    // Try to transition back to Created (invalid)
    println!("\nAttempting invalid transition: Running -> Created");
    match agent.transition_to(AgentState::Created) {
        TransitionResult::Success => println!("✓ Success"),
        TransitionResult::InvalidTransition { from, to } => {
            println!("✗ Invalid transition from {:?} to {:?}", from, to);
        }
        TransitionResult::Blocked { reason } => {
            println!("✗ Blocked: {}", reason);
        }
    }

    // Once terminated, can't transition anywhere
    agent.terminate();
    println!("\nAgent terminated: {:?}", agent.state());

    println!("\nAttempting to start terminated agent:");
    match agent.start() {
        TransitionResult::Success => println!("✓ Success"),
        TransitionResult::InvalidTransition { from, to } => {
            println!("✗ Invalid transition from {:?} to {:?}", from, to);
        }
        TransitionResult::Blocked { reason } => {
            println!("✗ Blocked: {}", reason);
        }
    }

    println!();
}

/// Example 3: Using state validation helpers
fn state_validation_helpers() {
    println!("3. State Validation Helpers");
    println!("----------------------------");

    let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    println!("State: {:?}", agent.state());
    println!("  can_accept_work: {}", agent.state().can_accept_work());
    println!("  is_active: {}", agent.state().is_active());
    println!("  is_recoverable: {}", agent.state().is_recoverable());
    println!("  is_terminal: {}", agent.state().is_terminal());

    agent.start();
    println!("\nState: {:?}", agent.state());
    println!("  can_accept_work: {}", agent.state().can_accept_work());
    println!("  is_active: {}", agent.state().is_active());
    println!("  is_recoverable: {}", agent.state().is_recoverable());
    println!("  is_terminal: {}", agent.state().is_terminal());

    agent.pause();
    println!("\nState: {:?}", agent.state());
    println!("  can_accept_work: {}", agent.state().can_accept_work());
    println!("  is_active: {}", agent.state().is_active());
    println!("  is_recoverable: {}", agent.state().is_recoverable());
    println!("  is_terminal: {}", agent.state().is_terminal());

    agent.terminate();
    println!("\nState: {:?}", agent.state());
    println!("  can_accept_work: {}", agent.state().can_accept_work());
    println!("  is_active: {}", agent.state().is_active());
    println!("  is_recoverable: {}", agent.state().is_recoverable());
    println!("  is_terminal: {}", agent.state().is_terminal());

    println!();
}

/// Example 4: Error handling and recovery
fn error_handling_and_recovery() {
    println!("4. Error Handling and Recovery");
    println!("-------------------------------");

    let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    agent.start();

    println!("Agent running: {}", agent.id());
    println!("State: {:?}", agent.state());

    // Simulate an error
    println!("\nSimulating runtime error...");
    let error = AgentError::new("Division by zero").with_code(42);

    agent.set_error(error);
    println!("State: {:?}", agent.state());

    if let Some(err) = agent.error() {
        println!("Error details:");
        println!("  Message: {}", err.message);
        println!("  Code: {:?}", err.code);
        println!("  Recovery attempts: {}", err.recovery_attempts);
        println!("  Fatal: {}", err.fatal);
    }

    // Attempt recovery
    println!("\nAttempting recovery...");
    match agent.attempt_recovery() {
        TransitionResult::Success => {
            println!("✓ Recovery successful");
            println!("State: {:?}", agent.state());
        }
        _ => println!("✗ Recovery failed"),
    }

    // Simulate fatal error
    println!("\n--- Fatal Error Scenario ---");
    agent.transition_to(AgentState::Running);

    let fatal_error = AgentError::new("Critical system failure").fatal();
    agent.set_error(fatal_error);

    println!("Fatal error occurred");
    if let Some(err) = agent.error() {
        println!("  Message: {}", err.message);
        println!("  Fatal: {}", err.fatal);
    }

    println!("\nAttempting recovery from fatal error...");
    match agent.attempt_recovery() {
        TransitionResult::Success => println!("✓ Recovery successful"),
        TransitionResult::Blocked { reason } => {
            println!("✗ Blocked: {}", reason);
        }
        _ => println!("✗ Recovery failed"),
    }

    println!();
}

/// Example 5: State history tracking
fn state_history_tracking() {
    println!("5. State History Tracking");
    println!("-------------------------");

    let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    println!("Agent: {}", agent.id());

    // Perform series of transitions
    agent.start();
    std::thread::sleep(Duration::from_millis(10));

    agent.pause();
    std::thread::sleep(Duration::from_millis(10));

    agent.resume();
    std::thread::sleep(Duration::from_millis(10));

    agent.suspend();
    std::thread::sleep(Duration::from_millis(10));

    agent.transition_to(AgentState::Running);
    std::thread::sleep(Duration::from_millis(10));

    // Print state history
    println!(
        "\nState history (last {} entries):",
        agent.state_history().len()
    );
    for (i, (state, timestamp)) in agent.state_history().iter().enumerate() {
        println!("  {}. {:?} (at {:?})", i + 1, state, timestamp);
    }

    println!("\nCurrent state: {:?}", agent.state());

    println!();
}

/// Example 6: Time tracking in states
fn time_tracking() {
    println!("6. Time Tracking");
    println!("----------------");

    let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    println!("Agent: {}", agent.id());

    agent.start();
    println!("\nState: {:?}", agent.state());

    // Wait a bit
    std::thread::sleep(Duration::from_millis(100));

    println!("Time in state: {:?}", agent.time_in_state());

    // Change state
    agent.pause();
    println!("\nState: {:?}", agent.state());
    println!("Time in state: {:?}", agent.time_in_state());

    std::thread::sleep(Duration::from_millis(50));
    println!("Time in state after 50ms: {:?}", agent.time_in_state());

    println!();
}

/// Example 7: State transition hooks (custom monitoring)
fn state_transition_hooks() {
    println!("7. State Transition Hooks");
    println!("-------------------------");

    // Create a transition logger
    let transitions = Arc::new(Mutex::new(Vec::new()));
    let transitions_clone = Arc::clone(&transitions);

    let log_transition = move |from: &AgentState, to: &AgentState| {
        transitions_clone
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((from.clone(), to.clone()));
        println!("  [Hook] Transition: {:?} -> {:?}", from, to);
    };

    let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    println!("Agent: {}", agent.id());

    // Manually trigger transitions and log them
    println!("\nPerforming transitions with hooks:");

    let from = agent.state().clone();
    agent.start();
    log_transition(&from, agent.state());

    let from = agent.state().clone();
    agent.pause();
    log_transition(&from, agent.state());

    let from = agent.state().clone();
    agent.resume();
    log_transition(&from, agent.state());

    let from = agent.state().clone();
    agent.terminate();
    log_transition(&from, agent.state());

    // Print logged transitions
    println!("\nRecorded transitions:");
    let logged = transitions.lock().unwrap_or_else(|e| e.into_inner());
    for (i, (from, to)) in logged.iter().enumerate() {
        println!("  {}. {:?} -> {:?}", i + 1, from, to);
    }

    println!();
}

/// Example 8: Complex lifecycle simulation
fn complex_lifecycle() {
    println!("8. Complex Lifecycle Simulation");
    println!("--------------------------------");

    let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    println!("Agent: {}", agent.id());

    println!("\n--- Simulating agent lifecycle ---");

    // Creation
    println!("\n1. Created state");
    println!("   State: {:?}", agent.state());
    std::thread::sleep(Duration::from_millis(10));

    // Start execution
    println!("\n2. Starting execution");
    agent.start();
    println!("   State: {:?}", agent.state());
    println!("   Can accept work: {}", agent.state().can_accept_work());
    std::thread::sleep(Duration::from_millis(50));

    // User pauses
    println!("\n3. User requests pause");
    agent.pause();
    println!("   State: {:?}", agent.state());
    println!("   Time in previous state: ~50ms");
    std::thread::sleep(Duration::from_millis(30));

    // User resumes
    println!("\n4. User resumes");
    agent.resume();
    println!("   State: {:?}", agent.state());
    std::thread::sleep(Duration::from_millis(20));

    // System suspends due to resource constraints
    println!("\n5. System suspends (low resources)");
    agent.suspend();
    println!("   State: {:?}", agent.state());
    println!("   Is recoverable: {}", agent.state().is_recoverable());
    std::thread::sleep(Duration::from_millis(40));

    // Begin migration
    println!("\n6. Begin migration to another node");
    agent.transition_to(AgentState::Migrating);
    println!("   State: {:?}", agent.state());
    std::thread::sleep(Duration::from_millis(100));

    // Migration completes
    println!("\n7. Migration completed");
    agent.complete_migration();
    println!("   State: {:?}", agent.state());
    std::thread::sleep(Duration::from_millis(30));

    // Error occurs
    println!("\n8. Runtime error occurs");
    let error = AgentError::new("Network timeout");
    agent.set_error(error);
    println!("   State: {:?}", agent.state());
    if let Some(err) = agent.error() {
        println!("   Error: {}", err.message);
    }
    std::thread::sleep(Duration::from_millis(20));

    // Recovery
    println!("\n9. Attempting recovery");
    match agent.attempt_recovery() {
        TransitionResult::Success => {
            println!("   ✓ Recovery successful");
            println!("   State: {:?}", agent.state());
        }
        _ => println!("   ✗ Recovery failed"),
    }
    std::thread::sleep(Duration::from_millis(30));

    // Final termination
    println!("\n10. Graceful shutdown");
    agent.terminate();
    println!("   State: {:?}", agent.state());
    println!("   Is terminal: {}", agent.state().is_terminal());

    // Show final history
    println!("\n--- Final State History ---");
    for (i, (state, _)) in agent.state_history().iter().enumerate() {
        println!("  {}. {:?}", i + 1, state);
    }

    println!("\n=== All examples completed ===");
}
