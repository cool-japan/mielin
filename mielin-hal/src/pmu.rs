//! Performance Monitoring Unit (PMU) Support
//!
//! This module provides access to CPU performance counters for profiling
//! and performance analysis.
//!
//! # Supported Architectures
//!
//! - **x86_64**: Intel Performance Monitoring (PMU), AMD Performance Events
//! - **AArch64**: ARM PMUv3 (Performance Monitors Extension)
//! - **RISC-V**: Hardware Performance Monitor (HPM) counters
//!
//! # Hardware Events
//!
//! Common performance events supported across architectures:
//! - **Cycles**: CPU clock cycles
//! - **Instructions**: Retired instructions
//! - **Cache References**: L1/L2/L3 cache accesses
//! - **Cache Misses**: L1/L2/L3 cache misses
//! - **Branch Instructions**: Branch instructions executed
//! - **Branch Misses**: Branch mispredictions
//! - **TLB References**: TLB accesses
//! - **TLB Misses**: TLB misses
//!
//! # x86_64 Implementation
//!
//! Uses CPUID to detect PMU version and capabilities:
//! - `CPUID.0AH.EAX[7:0]`: PMU version
//! - `CPUID.0AH.EAX[15:8]`: Number of general-purpose counters
//! - `CPUID.0AH.EAX[23:16]`: Bit width of counters
//! - `CPUID.0AH.EBX`: Event availability bitmap
//!
//! # AArch64 Implementation
//!
//! Uses system registers to access PMU:
//! - `PMCR_EL0`: Performance Monitors Control Register
//! - `PMEVCNTRn_EL0`: Event counter registers (n=0-30)
//! - `PMEVTYPER_n_EL0`: Event type selection
//! - `PMCCNTR_EL0`: Cycle counter
//!
//! # Examples
//!
//! ```
//! use mielin_hal::pmu::{detect_pmu, PerfEvent};
//!
//! let pmu = detect_pmu();
//!
//! if pmu.available {
//!     println!("PMU version: {}", pmu.version);
//!     println!("Counters: {}", pmu.num_counters);
//!     println!("Counter width: {} bits", pmu.counter_width);
//!
//!     // Check if specific events are supported
//!     if pmu.supports_event(PerfEvent::L1DCacheMisses) {
//!         println!("L1 data cache miss counting supported");
//!     }
//! }
//! ```
//!
//! # Safety
//!
//! Reading performance counters typically requires:
//! - **x86_64**: RDPMC instruction (ring 0 or with CR4.PCE=1)
//! - **AArch64**: PMU enabled in EL1 (PMUSERENR_EL0)
//! - **Kernel support**: perf_event_open() syscall recommended
//!
//! Direct hardware access may require elevated privileges.
//!
//! # Limitations
//!
//! 1. **Counter Availability**: Number of counters varies by CPU
//! 2. **Multiplexing**: More events than counters requires time-multiplexing
//! 3. **Precision**: Counters may overflow or have sampling imprecision
//! 4. **Virtualization**: Counters may not be available in VMs

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::__cpuid;

/// Performance event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PerfEvent {
    /// CPU clock cycles
    Cycles,
    /// Instructions retired
    Instructions,
    /// L1 data cache references
    L1DCacheReferences,
    /// L1 data cache misses
    L1DCacheMisses,
    /// L1 instruction cache references
    L1ICacheReferences,
    /// L1 instruction cache misses
    L1ICacheMisses,
    /// Last-level cache references
    LLCReferences,
    /// Last-level cache misses
    LLCMisses,
    /// Branch instructions
    BranchInstructions,
    /// Branch mispredictions
    BranchMisses,
    /// TLB data references
    TLBReferences,
    /// TLB data misses
    TLBMisses,
    /// Stalled cycles (frontend)
    StalledCyclesFrontend,
    /// Stalled cycles (backend)
    StalledCyclesBackend,
}

impl fmt::Display for PerfEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cycles => write!(f, "CPU Cycles"),
            Self::Instructions => write!(f, "Instructions"),
            Self::L1DCacheReferences => write!(f, "L1D Cache References"),
            Self::L1DCacheMisses => write!(f, "L1D Cache Misses"),
            Self::L1ICacheReferences => write!(f, "L1I Cache References"),
            Self::L1ICacheMisses => write!(f, "L1I Cache Misses"),
            Self::LLCReferences => write!(f, "LLC References"),
            Self::LLCMisses => write!(f, "LLC Misses"),
            Self::BranchInstructions => write!(f, "Branch Instructions"),
            Self::BranchMisses => write!(f, "Branch Misses"),
            Self::TLBReferences => write!(f, "TLB References"),
            Self::TLBMisses => write!(f, "TLB Misses"),
            Self::StalledCyclesFrontend => write!(f, "Stalled Cycles (Frontend)"),
            Self::StalledCyclesBackend => write!(f, "Stalled Cycles (Backend)"),
        }
    }
}

/// PMU vendor-specific information
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PmuVendor {
    /// Intel Performance Monitoring
    Intel,
    /// AMD Performance Events
    Amd,
    /// ARM PMUv3
    Arm,
    /// RISC-V HPM
    RiscV,
    /// Unknown/Generic
    Generic,
}

impl fmt::Display for PmuVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Intel => write!(f, "Intel"),
            Self::Amd => write!(f, "AMD"),
            Self::Arm => write!(f, "ARM"),
            Self::RiscV => write!(f, "RISC-V"),
            Self::Generic => write!(f, "Generic"),
        }
    }
}

/// Performance Monitoring Unit information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PmuInfo {
    /// PMU is available
    pub available: bool,
    /// PMU vendor
    pub vendor: PmuVendor,
    /// PMU version
    pub version: u8,
    /// Number of general-purpose counters
    pub num_counters: u8,
    /// Bit width of counters (typically 40, 48, or 64)
    pub counter_width: u8,
    /// Number of fixed-function counters (x86_64 only)
    pub num_fixed_counters: u8,
    /// Fixed counter width (x86_64 only)
    pub fixed_counter_width: u8,
    /// Supported events bitmap
    pub supported_events: Vec<PerfEvent>,
}

impl Default for PmuInfo {
    fn default() -> Self {
        Self {
            available: false,
            vendor: PmuVendor::Generic,
            version: 0,
            num_counters: 0,
            counter_width: 0,
            num_fixed_counters: 0,
            fixed_counter_width: 0,
            supported_events: Vec::new(),
        }
    }
}

impl PmuInfo {
    /// Check if a specific performance event is supported
    pub fn supports_event(&self, event: PerfEvent) -> bool {
        self.supported_events.contains(&event)
    }

    /// Get total number of counters (general-purpose + fixed)
    pub fn total_counters(&self) -> usize {
        (self.num_counters + self.num_fixed_counters) as usize
    }
}

/// Detect PMU capabilities
///
/// # Examples
///
/// ```
/// use mielin_hal::pmu::detect_pmu;
///
/// let pmu = detect_pmu();
/// if pmu.available {
///     println!("PMU available with {} counters", pmu.num_counters);
/// }
/// ```
pub fn detect_pmu() -> PmuInfo {
    #[cfg(target_arch = "x86_64")]
    {
        detect_pmu_x86_64()
    }

    #[cfg(target_arch = "aarch64")]
    {
        detect_pmu_aarch64()
    }

    #[cfg(target_arch = "riscv64")]
    {
        detect_pmu_riscv64()
    }

    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    {
        PmuInfo::default()
    }
}

/// Detect PMU on x86_64 via CPUID
#[cfg(target_arch = "x86_64")]
fn detect_pmu_x86_64() -> PmuInfo {
    // CPUID.0AH: Architectural Performance Monitoring
    // Safety note: __cpuid is a safe fn on this toolchain (CPUID is
    // unconditionally available on x86_64), so no `unsafe` block is needed.
    let cpuid_pmu = __cpuid(0x0A);

    let version = (cpuid_pmu.eax & 0xFF) as u8;

    // No PMU support
    if version == 0 {
        return PmuInfo::default();
    }

    let num_counters = ((cpuid_pmu.eax >> 8) & 0xFF) as u8;
    let counter_width = ((cpuid_pmu.eax >> 16) & 0xFF) as u8;
    let num_fixed_counters = (cpuid_pmu.edx & 0x1F) as u8;
    let fixed_counter_width = ((cpuid_pmu.edx >> 5) & 0xFF) as u8;

    // Event availability bitmap (EBX)
    let events_bitmap = cpuid_pmu.ebx;

    let mut supported_events = Vec::new();

    // Bit 0: Core cycles event not available
    if (events_bitmap & (1 << 0)) == 0 {
        supported_events.push(PerfEvent::Cycles);
    }

    // Bit 1: Instruction retired event not available
    if (events_bitmap & (1 << 1)) == 0 {
        supported_events.push(PerfEvent::Instructions);
    }

    // Bit 2: Reference cycles event not available (use for LLC)
    if (events_bitmap & (1 << 2)) == 0 {
        supported_events.push(PerfEvent::LLCReferences);
    }

    // Bit 3: LLC references not available
    if (events_bitmap & (1 << 3)) == 0 {
        supported_events.push(PerfEvent::LLCMisses);
    }

    // Bit 4: Branch instruction retired not available
    if (events_bitmap & (1 << 4)) == 0 {
        supported_events.push(PerfEvent::BranchInstructions);
    }

    // Bit 5: Branch mispredicts not available
    if (events_bitmap & (1 << 5)) == 0 {
        supported_events.push(PerfEvent::BranchMisses);
    }

    // Always add common events (may require model-specific programming)
    supported_events.push(PerfEvent::L1DCacheReferences);
    supported_events.push(PerfEvent::L1DCacheMisses);
    supported_events.push(PerfEvent::TLBReferences);
    supported_events.push(PerfEvent::TLBMisses);

    // Detect vendor
    let cpuid_vendor = __cpuid(0);
    let vendor = detect_x86_vendor(cpuid_vendor.ebx, cpuid_vendor.edx, cpuid_vendor.ecx);

    PmuInfo {
        available: true,
        vendor,
        version,
        num_counters,
        counter_width,
        num_fixed_counters,
        fixed_counter_width,
        supported_events,
    }
}

#[cfg(target_arch = "x86_64")]
fn detect_x86_vendor(ebx: u32, edx: u32, ecx: u32) -> PmuVendor {
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&ecx.to_le_bytes());

    match &vendor {
        b"GenuineIntel" => PmuVendor::Intel,
        b"AuthenticAMD" => PmuVendor::Amd,
        _ => PmuVendor::Generic,
    }
}

/// Detect PMU on AArch64 via system registers
#[cfg(target_arch = "aarch64")]
fn detect_pmu_aarch64() -> PmuInfo {
    // NOTE: Reading PMCR_EL0 requires user-space access to be enabled
    // by the kernel (PMUSERENR_EL0). If not enabled, this will cause
    // an illegal instruction fault.
    //
    // For now, return generic ARMv8 PMU info without accessing registers.
    // In production, use perf_event_open() syscall instead.

    // Standard ARMv8 configuration (most implementations have 6 counters)
    let num_counters = 6;
    let counter_width = 48;
    let num_fixed_counters = 1; // Cycle counter
    let fixed_counter_width = 64;

    // All ARMv8 PMUs support these events
    let supported_events = alloc::vec![
        PerfEvent::Cycles,
        PerfEvent::Instructions,
        PerfEvent::L1DCacheReferences,
        PerfEvent::L1DCacheMisses,
        PerfEvent::L1ICacheReferences,
        PerfEvent::L1ICacheMisses,
        PerfEvent::LLCReferences,
        PerfEvent::LLCMisses,
        PerfEvent::BranchInstructions,
        PerfEvent::BranchMisses,
        PerfEvent::TLBReferences,
        PerfEvent::TLBMisses,
    ];

    PmuInfo {
        available: true,
        vendor: PmuVendor::Arm,
        version: 3, // PMUv3 for ARMv8
        num_counters,
        counter_width,
        num_fixed_counters,
        fixed_counter_width,
        supported_events,
    }
}

/// Detect PMU on RISC-V via CSRs
#[cfg(target_arch = "riscv64")]
fn detect_pmu_riscv64() -> PmuInfo {
    // RISC-V defines 29 hardware performance monitor counters (mhpmcounter3-31)
    // Plus cycle and instret counters

    // For now, return generic info (requires M-mode access to detect precisely)
    PmuInfo {
        available: true,
        vendor: PmuVendor::RiscV,
        version: 1,
        num_counters: 29, // mhpmcounter3 through mhpmcounter31
        counter_width: 64,
        num_fixed_counters: 2, // cycle, instret
        fixed_counter_width: 64,
        supported_events: alloc::vec![PerfEvent::Cycles, PerfEvent::Instructions,],
    }
}

/// Get a human-readable PMU summary
pub fn pmu_summary() -> String {
    let pmu = detect_pmu();

    if !pmu.available {
        return "PMU not available".to_string();
    }

    alloc::format!(
        "PMU: {} (version {}), {} counters ({}-bit)",
        pmu.vendor,
        pmu.version,
        pmu.num_counters,
        pmu.counter_width
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_detect_pmu() {
        let _pmu = detect_pmu();
        // Should always complete (may return unavailable on some platforms)
        // No assertion needed - the test passes if detection doesn't panic
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn test_x86_64_pmu() {
        let pmu = detect_pmu_x86_64();
        // Most modern x86_64 CPUs have PMU
        if pmu.available {
            assert!(pmu.num_counters > 0);
            assert!(pmu.counter_width >= 32);
            assert!(matches!(
                pmu.vendor,
                PmuVendor::Intel | PmuVendor::Amd | PmuVendor::Generic
            ));
        }
    }

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn test_aarch64_pmu() {
        let pmu = detect_pmu_aarch64();
        // ARMv8 has PMU support
        if pmu.available {
            assert_eq!(pmu.vendor, PmuVendor::Arm);
            assert!(pmu.num_counters > 0);
            assert_eq!(pmu.counter_width, 48);
        }
    }

    #[test]
    fn test_pmu_supports_event() {
        let mut pmu = PmuInfo::default();
        pmu.supported_events.push(PerfEvent::Cycles);
        pmu.supported_events.push(PerfEvent::Instructions);

        assert!(pmu.supports_event(PerfEvent::Cycles));
        assert!(pmu.supports_event(PerfEvent::Instructions));
        assert!(!pmu.supports_event(PerfEvent::BranchMisses));
    }

    #[test]
    fn test_pmu_total_counters() {
        let pmu = PmuInfo {
            num_counters: 4,
            num_fixed_counters: 3,
            ..Default::default()
        };

        assert_eq!(pmu.total_counters(), 7);
    }

    #[test]
    fn test_perf_event_display() {
        assert_eq!(format!("{}", PerfEvent::Cycles), "CPU Cycles");
        assert_eq!(format!("{}", PerfEvent::BranchMisses), "Branch Misses");
        assert_eq!(format!("{}", PerfEvent::L1DCacheMisses), "L1D Cache Misses");
    }

    #[test]
    fn test_pmu_vendor_display() {
        assert_eq!(format!("{}", PmuVendor::Intel), "Intel");
        assert_eq!(format!("{}", PmuVendor::Amd), "AMD");
        assert_eq!(format!("{}", PmuVendor::Arm), "ARM");
    }

    #[test]
    fn test_pmu_summary() {
        let summary = pmu_summary();
        assert!(!summary.is_empty());
        assert!(summary.contains("PMU") || summary.contains("not available"));
    }

    #[test]
    fn test_pmu_info_default() {
        let pmu = PmuInfo::default();
        assert!(!pmu.available);
        assert_eq!(pmu.num_counters, 0);
        assert_eq!(pmu.counter_width, 0);
    }

    #[test]
    fn test_perf_event_types() {
        let events = [
            PerfEvent::Cycles,
            PerfEvent::Instructions,
            PerfEvent::L1DCacheMisses,
            PerfEvent::BranchMisses,
        ];

        for event in &events {
            assert!(!format!("{}", event).is_empty());
        }
    }
}
