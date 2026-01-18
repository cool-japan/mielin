//! Runtime Capability Switching Example
//!
//! This example demonstrates how to use the runtime capability switching
//! system to dynamically select optimal code paths based on available
//! hardware features.
//!
//! Run with: cargo run --example runtime_switching

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_hal::runtime::{DispatchTable, FallbackChain, FeatureRequirement, RuntimeSelector};

/// Example: Matrix multiplication with SIMD fallbacks
///
/// This demonstrates how to create a fallback chain for SIMD operations,
/// with automatic selection of the best available implementation.
fn matrix_multiply_example() {
    println!("=== Matrix Multiplication Example ===\n");

    // Create a fallback chain for matrix operations
    let mut chain = FallbackChain::new("matrix_multiply");

    // Add implementations in preference order (highest to lowest)
    // AVX-512: Best performance on supported hardware
    chain.add_requirement(
        FeatureRequirement::all_of(&[HardwareCapabilities::AVX512])
            .with_min_vector_width(512)
            .with_name("avx512_matmul"),
    );

    // AVX2 + FMA: Good performance on most modern CPUs
    chain.add_requirement(
        FeatureRequirement::all_of(&[HardwareCapabilities::AVX2, HardwareCapabilities::FMA])
            .with_min_vector_width(256)
            .with_preferred(HardwareCapabilities::AES_NI)
            .with_name("avx2_fma_matmul"),
    );

    // AVX: Fallback for older hardware
    chain.add_requirement(
        FeatureRequirement::all_of(&[HardwareCapabilities::AVX])
            .with_min_vector_width(256)
            .with_name("avx_matmul"),
    );

    // SSE4.2: Wider compatibility
    chain.add_requirement(
        FeatureRequirement::all_of(&[HardwareCapabilities::SSE4_2])
            .with_min_vector_width(128)
            .with_name("sse42_matmul"),
    );

    // Scalar fallback: Always available
    chain.add_requirement(FeatureRequirement::none().with_name("scalar_matmul"));

    // Select the best implementation
    let selector = RuntimeSelector::new();
    match selector.select(&chain) {
        Ok(selected) => {
            println!("Selected implementation: {}", selected.name);
            println!("  Priority score: {}", selected.priority);
            println!(
                "  Satisfaction score: {}",
                selected.satisfaction_score(selector.profile())
            );

            if selected.min_vector_width > 0 {
                println!("  Vector width: {} bits", selected.min_vector_width);
            }

            // Display what capabilities are required
            println!("\n  Required capabilities:");
            if selected.required.is_empty() {
                println!("    - None (scalar fallback)");
            } else {
                if selected.required.contains(HardwareCapabilities::AVX512) {
                    println!("    - AVX-512");
                }
                if selected.required.contains(HardwareCapabilities::AVX2) {
                    println!("    - AVX2");
                }
                if selected.required.contains(HardwareCapabilities::FMA) {
                    println!("    - FMA (Fused Multiply-Add)");
                }
                if selected.required.contains(HardwareCapabilities::AVX) {
                    println!("    - AVX");
                }
                if selected.required.contains(HardwareCapabilities::SSE4_2) {
                    println!("    - SSE4.2");
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    println!();
}

/// Example: Dispatch table for runtime function selection
///
/// This demonstrates how to use a dispatch table to automatically
/// select and call the optimal function implementation.
fn dispatch_table_example() {
    println!("=== Dispatch Table Example ===\n");

    // Scalar implementation (always available)
    fn compute_scalar() -> f64 {
        println!("  Using scalar implementation");
        let mut sum = 0.0;
        for i in 0..1000 {
            sum += (i as f64).sin();
        }
        sum
    }

    // AVX2 implementation (faster on modern CPUs)
    fn compute_avx2() -> f64 {
        println!("  Using AVX2 vectorized implementation");
        let mut sum = 0.0;
        for i in 0..1000 {
            sum += (i as f64).sin();
        }
        sum * 1.01 // Slight difference to show which was selected
    }

    // AVX-512 implementation (fastest on supported hardware)
    fn compute_avx512() -> f64 {
        println!("  Using AVX-512 vectorized implementation");
        let mut sum = 0.0;
        for i in 0..1000 {
            sum += (i as f64).sin();
        }
        sum * 1.02 // Slight difference to show which was selected
    }

    // Create dispatch table
    let mut table: DispatchTable<f64> = DispatchTable::new("trigonometric_sum");

    // Add implementations with requirements
    table.add_impl(FeatureRequirement::none(), compute_scalar);
    table.add_impl(
        FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]),
        compute_avx2,
    );
    table.add_impl(
        FeatureRequirement::all_of(&[HardwareCapabilities::AVX512]),
        compute_avx512,
    );

    // Select optimal implementation
    let selector = RuntimeSelector::new();
    match table.select(&selector) {
        Ok(()) => {
            println!("Implementation selected successfully");

            // Call the selected function
            match table.call() {
                Ok(result) => {
                    println!("  Result: {}", result);
                }
                Err(e) => {
                    println!("  Error calling function: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Error selecting implementation: {}", e);
        }
    }

    println!();
}

/// Example: Runtime verification
///
/// This demonstrates how to verify that selected features actually work
/// by running a test function before committing to an implementation.
fn runtime_verification_example() {
    println!("=== Runtime Verification Example ===\n");

    let mut chain = FallbackChain::new("crypto_operations");

    // Prefer AES-NI if available
    chain.add_requirement(
        FeatureRequirement::all_of(&[HardwareCapabilities::AES_NI]).with_name("aes_ni_crypto"),
    );

    // Fallback to software AES
    chain.add_requirement(FeatureRequirement::none().with_name("software_crypto"));

    // Verification function (checks if the feature actually works)
    let verify_fn = |req: &FeatureRequirement| {
        if req.required.contains(HardwareCapabilities::AES_NI) {
            // In a real scenario, you might actually try to use AES-NI
            // and catch any exceptions
            println!("  Verifying AES-NI availability...");
            println!("  ✓ AES-NI verified and working");
            true
        } else {
            println!("  Using software fallback (no verification needed)");
            true
        }
    };

    let mut selector = RuntimeSelector::new();
    match selector.select_verified(&chain, verify_fn) {
        Ok(selected) => {
            println!("\nSelected and verified: {}", selected.name);
            println!(
                "  Verification status: {}",
                if selector.is_verified() {
                    "Passed"
                } else {
                    "Not verified"
                }
            );
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    println!();
}

/// Example: Hot-reload scenario
///
/// This demonstrates how to refresh hardware detection when hardware
/// configuration changes (e.g., in virtualized environments or with
/// hot-plug devices).
fn hot_reload_example() {
    println!("=== Hot-Reload Example ===\n");

    let mut selector = RuntimeSelector::new();

    println!("Initial detection:");
    println!("  Version: {}", selector.version());
    println!("  Cores: {}", selector.profile().core_count);

    // Simulate hardware change (in real scenario, this might be triggered
    // by a system event or periodic polling)
    println!("\nSimulating hardware change...");
    selector.refresh();

    println!("\nAfter refresh:");
    println!("  Version: {}", selector.version());
    println!("  Cores: {}", selector.profile().core_count);
    println!("  Detection refreshed successfully");

    println!();
}

/// Example: Custom scoring
///
/// This demonstrates how different requirements score against the
/// current hardware profile.
fn scoring_example() {
    println!("=== Capability Scoring Example ===\n");

    let selector = RuntimeSelector::new();
    let profile = selector.profile();

    // Create different requirements
    let requirements = vec![
        FeatureRequirement::all_of(&[HardwareCapabilities::AVX512])
            .with_min_vector_width(512)
            .with_name("avx512_req"),
        FeatureRequirement::all_of(&[HardwareCapabilities::AVX2])
            .with_min_vector_width(256)
            .with_preferred(HardwareCapabilities::FMA)
            .with_name("avx2_req"),
        FeatureRequirement::all_of(&[HardwareCapabilities::SSE4_2])
            .with_min_vector_width(128)
            .with_name("sse42_req"),
        FeatureRequirement::none().with_name("scalar_req"),
    ];

    println!("Scoring requirements against current hardware:\n");

    for req in &requirements {
        let satisfied = req.is_satisfied_by(profile);
        let score = req.satisfaction_score(profile);

        println!("  {}", req.name);
        println!("    Satisfied: {}", satisfied);
        println!("    Score: {}", score);
        println!("    Priority: {}", req.priority);

        if satisfied {
            println!("    ✓ Can use this implementation");
        } else {
            println!("    ✗ Cannot use (requirements not met)");
        }
        println!();
    }
}

fn main() {
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  MielinOS HAL - Runtime Capability Switching Example  ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    // Display current hardware capabilities
    let profile = mielin_hal::capabilities::HardwareProfile::detect();
    println!("Current Hardware Profile:");
    println!("  Architecture: {}", profile.architecture);
    println!("  Cores: {}", profile.core_count);
    println!("  Memory: {} MB", profile.memory_size / 1024 / 1024);
    println!("  Max Vector Width: {} bits", profile.max_vector_width());

    println!("\n  Available Capabilities:");
    if profile.capabilities.contains(HardwareCapabilities::AVX512) {
        println!("    ✓ AVX-512");
    }
    if profile.capabilities.contains(HardwareCapabilities::AVX2) {
        println!("    ✓ AVX2");
    }
    if profile.capabilities.contains(HardwareCapabilities::FMA) {
        println!("    ✓ FMA");
    }
    if profile.capabilities.contains(HardwareCapabilities::AVX) {
        println!("    ✓ AVX");
    }
    if profile.capabilities.contains(HardwareCapabilities::SSE4_2) {
        println!("    ✓ SSE4.2");
    }
    if profile.capabilities.contains(HardwareCapabilities::AES_NI) {
        println!("    ✓ AES-NI");
    }
    if profile.capabilities.contains(HardwareCapabilities::NEON) {
        println!("    ✓ NEON");
    }
    if profile.capabilities.contains(HardwareCapabilities::SVE2) {
        println!("    ✓ SVE2");
    }

    println!("\n{}\n", "=".repeat(58));

    // Run examples
    matrix_multiply_example();
    dispatch_table_example();
    runtime_verification_example();
    hot_reload_example();
    scoring_example();

    println!("╔════════════════════════════════════════════════════════╗");
    println!("║  All examples completed successfully!                 ║");
    println!("╚════════════════════════════════════════════════════════╝\n");
}
