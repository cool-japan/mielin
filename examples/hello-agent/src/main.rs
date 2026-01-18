//! Hello Agent Example
//!
//! Demonstrates basic agent creation and execution.

use mielin_cells::Agent;

#[tokio::main]
async fn main() {
    println!("MielinOS - Hello Agent Example");

    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let agent = Agent::new(wasm_binary);

    println!("Created agent with ID: {}", agent.id());
    println!("Agent state: {:?}", agent.state());
    println!("Agent DNA hash: {:?}", agent.dna().hash());
}
