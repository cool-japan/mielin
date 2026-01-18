//! Enhanced Security Example
//!
//! Demonstrates security policy configuration and security contexts.

use mielin_wasm::security::{SecurityContext, SecurityPolicy};

fn main() -> anyhow::Result<()> {
    println!("=== Enhanced Security Example ===\n");

    // 1. Create a strict security context
    println!("1. Creating strict security context...");
    let _strict_security = SecurityContext::new(SecurityPolicy::strict());
    println!("   Security features enabled:");
    println!("   - Control Flow Integrity (CFI)");
    println!("   - Stack Canaries");
    println!("   - Address Space Layout Randomization (ASLR)");
    println!("   - Capability Attestation");
    println!();

    // 2. Security Policy Comparison
    println!("2. Security Policy Comparison:");

    println!("   Permissive Security:");
    let permissive = SecurityPolicy::permissive();
    println!("     CFI enabled: {}", permissive.enable_cfi);
    println!(
        "     Stack canaries enabled: {}",
        permissive.enable_stack_canaries
    );
    println!("     ASLR enabled: {}", permissive.enable_aslr);
    println!(
        "     Attestation enabled: {}",
        permissive.enable_attestation
    );
    println!("     Panic on violation: {}", permissive.panic_on_violation);
    println!();

    println!("   Standard Security:");
    let standard = SecurityPolicy::standard();
    println!("     CFI enabled: {}", standard.enable_cfi);
    println!(
        "     Stack canaries enabled: {}",
        standard.enable_stack_canaries
    );
    println!("     ASLR enabled: {}", standard.enable_aslr);
    println!("     Attestation enabled: {}", standard.enable_attestation);
    println!("     Panic on violation: {}", standard.panic_on_violation);
    println!();

    println!("   Strict Security:");
    let strict = SecurityPolicy::strict();
    println!("     CFI enabled: {}", strict.enable_cfi);
    println!(
        "     Stack canaries enabled: {}",
        strict.enable_stack_canaries
    );
    println!("     ASLR enabled: {}", strict.enable_aslr);
    println!("     Attestation enabled: {}", strict.enable_attestation);
    println!("     Panic on violation: {}", strict.panic_on_violation);
    println!("     Max violations: {}", strict.max_violations);
    println!();

    // 3. Security Context Features
    println!("3. Security Context Features:");
    let _ctx = SecurityContext::new(SecurityPolicy::strict());

    println!("   Starting with strict policy (all features enabled)...");
    println!("   ✓ CFI enabled");
    println!("   ✓ Stack canaries enabled");
    println!("   ✓ ASLR enabled");
    println!("   ✓ Capability attestation enabled");
    println!();

    // 4. Security Violations Tracking
    println!("4. Security Violations Tracking:");
    println!("   Creating contexts with different policies...");

    let permissive_ctx = SecurityContext::new(SecurityPolicy::permissive());
    println!(
        "   Permissive: {} violations tracked",
        permissive_ctx.get_violations().len()
    );

    let standard_ctx = SecurityContext::new(SecurityPolicy::standard());
    println!(
        "   Standard: {} violations tracked",
        standard_ctx.get_violations().len()
    );

    let strict_ctx = SecurityContext::new(SecurityPolicy::strict());
    println!(
        "   Strict: {} violations tracked",
        strict_ctx.get_violations().len()
    );
    println!();

    println!("=== Example completed successfully ===");

    Ok(())
}
