//! Cortex-M Hardware Detection Example
//!
//! This example demonstrates Cortex-M specific hardware detection including
//! MPU, FPU, DSP, and NVIC capabilities.
//!
//! This example only works on Cortex-M targets.
//! Build with: cargo build --example cortex_m_detection --target thumbv7em-none-eabihf

#![cfg_attr(all(target_arch = "arm", target_os = "none"), no_std)]
#![cfg_attr(all(target_arch = "arm", target_os = "none"), no_main)]

#[cfg(all(target_arch = "arm", target_os = "none"))]
use mielin_hal::arch::cortex_m::{CortexMCapabilities, FpuVariant};

#[cfg(all(target_arch = "arm", target_os = "none"))]
use core::panic::PanicInfo;

#[cfg(all(target_arch = "arm", target_os = "none"))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[cfg(all(target_arch = "arm", target_os = "none"))]
#[no_mangle]
pub extern "C" fn main() -> ! {
    // Detect all Cortex-M capabilities
    let caps = CortexMCapabilities::detect();

    // Processor information
    let processor_name = caps.processor_name();
    let _part_number = caps.part_number;
    let _revision = caps.revision;

    // MPU capabilities
    let mpu_present = caps.mpu.present;
    let _mpu_regions = caps.mpu.num_regions;
    let _mpu_separate = caps.mpu.separate_regions;

    // FPU capabilities
    let fpu_present = caps.fpu.variant != FpuVariant::None;
    let _fpu_variant = caps.fpu.variant;
    let _fpu_lazy = caps.fpu.lazy_context;

    // DSP capabilities
    let _dsp_present = caps.dsp.present;
    let _dsp_simd = caps.dsp.simd;

    // NVIC capabilities
    let _nvic_priority_levels = caps.nvic.priority_levels;
    let _nvic_priority_bits = caps.nvic.priority_bits();
    let _nvic_interrupts = caps.nvic.num_interrupts;

    // In a real embedded system, you would use these values
    // to configure peripherals, enable features, etc.
    let _ = (processor_name, mpu_present, fpu_present);

    loop {
        // Main loop
    }
}

#[cfg(not(all(target_arch = "arm", target_os = "none")))]
fn main() {
    println!("This example only works on Cortex-M targets.");
    println!("Build with: cargo build --example cortex_m_detection --target thumbv7em-none-eabihf");
}
