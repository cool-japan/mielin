//! Power Management Subsystem
//!
//! This module provides power management features for the MielinOS kernel,
//! including CPU frequency scaling (P-states), sleep states (C-states),
//! and power-aware task scheduling.
//!
//! # Features
//!
//! - **P-States (Performance States)**: CPU frequency and voltage scaling
//! - **C-States (CPU Sleep States)**: Different levels of CPU idle states
//! - **Dynamic Voltage/Frequency Scaling (DVFS)**: Automatic power optimization
//! - **Power-Aware Scheduling**: Task scheduling based on power policies
//! - **Power Statistics**: Detailed telemetry for power consumption analysis
//!
//! # Architecture Support
//!
//! - **x86_64**: ACPI P-states and C-states, Intel SpeedStep, AMD PowerNow
//! - **AArch64**: ARM DVFS, CPU idle states
//! - **RISC-V**: Platform-specific power management
//!
//! # Example
//!
//! ```ignore
//! use mielin_kernel::power::{self, PowerPolicy};
//!
//! // Initialize power management
//! power::init().unwrap();
//!
//! // Set power policy to balanced
//! power::set_policy(PowerPolicy::Balanced).unwrap();
//!
//! // Get current CPU frequency
//! let freq = power::get_cpu_frequency(0).unwrap();
//! println!("CPU 0 frequency: {} MHz", freq);
//! ```

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

/// Power management errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerError {
    /// Power management not initialized
    NotInitialized,
    /// Already initialized
    AlreadyInitialized,
    /// Invalid CPU ID
    InvalidCpu,
    /// Invalid P-state
    InvalidPState,
    /// Invalid C-state
    InvalidCState,
    /// Invalid power policy
    InvalidPolicy,
    /// Hardware not supported
    HardwareNotSupported,
    /// Operation failed
    OperationFailed,
}

impl core::fmt::Display for PowerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "Power management not initialized"),
            Self::AlreadyInitialized => write!(f, "Power management already initialized"),
            Self::InvalidCpu => write!(f, "Invalid CPU ID"),
            Self::InvalidPState => write!(f, "Invalid P-state"),
            Self::InvalidCState => write!(f, "Invalid C-state"),
            Self::InvalidPolicy => write!(f, "Invalid power policy"),
            Self::HardwareNotSupported => write!(f, "Hardware not supported"),
            Self::OperationFailed => write!(f, "Operation failed"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PowerError {}

/// Power management policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerPolicy {
    /// Maximum performance (highest frequency, no idle)
    Performance,
    /// Balanced performance and power (adaptive)
    Balanced,
    /// Power saving (lower frequency, aggressive idle)
    PowerSaver,
    /// Custom policy (user-defined)
    Custom,
}

/// CPU Performance State (P-state)
///
/// P-states control CPU frequency and voltage.
/// Lower P-state numbers = higher performance/power.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PState {
    /// P-state number (0 = highest performance)
    pub state: u8,
    /// CPU frequency in MHz
    pub frequency_mhz: u32,
    /// CPU voltage in millivolts
    pub voltage_mv: u32,
    /// Power consumption in milliwatts (estimated)
    pub power_mw: u32,
}

impl PState {
    /// Create a new P-state
    pub const fn new(state: u8, frequency_mhz: u32, voltage_mv: u32, power_mw: u32) -> Self {
        Self {
            state,
            frequency_mhz,
            voltage_mv,
            power_mw,
        }
    }
}

/// CPU Sleep State (C-state)
///
/// C-states control CPU idle power consumption.
/// Higher C-state numbers = deeper sleep/lower power.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CState {
    /// C0: CPU active (not a sleep state)
    C0,
    /// C1: Halt (stops CPU clock, instant wake)
    C1,
    /// C2: Stop-Clock (stops internal clocks, ~80us wake)
    C2,
    /// C3: Sleep (stops more clocks, ~200us wake)
    C3,
    /// C6: Deep Sleep (lowest power, ~300us wake)
    C6,
}

impl CState {
    /// Get the numeric C-state value
    pub const fn value(&self) -> u8 {
        match self {
            Self::C0 => 0,
            Self::C1 => 1,
            Self::C2 => 2,
            Self::C3 => 3,
            Self::C6 => 6,
        }
    }

    /// Get wake-up latency in microseconds
    pub const fn wakeup_latency_us(&self) -> u32 {
        match self {
            Self::C0 => 0,
            Self::C1 => 1,
            Self::C2 => 80,
            Self::C3 => 200,
            Self::C6 => 300,
        }
    }

    /// Get estimated power savings percentage vs C0
    pub const fn power_savings_percent(&self) -> u8 {
        match self {
            Self::C0 => 0,
            Self::C1 => 20,
            Self::C2 => 50,
            Self::C3 => 70,
            Self::C6 => 90,
        }
    }
}

/// Per-CPU power state
#[repr(align(64))]
struct CpuPowerState {
    /// Current P-state
    current_pstate: AtomicUsize,
    /// Target P-state (for DVFS)
    target_pstate: AtomicUsize,
    /// Current C-state
    current_cstate: AtomicU32,
    /// Total time in each P-state (nanoseconds)
    #[allow(dead_code)] // Future use for power consumption tracking
    pstate_time: [AtomicU64; 8],
    /// Total time in each C-state (nanoseconds)
    #[allow(dead_code)] // Future use for power consumption tracking
    cstate_time: [AtomicU64; 7],
    /// Number of P-state transitions
    pstate_transitions: AtomicU64,
    /// Number of C-state transitions
    cstate_transitions: AtomicU64,
    /// Current CPU frequency (MHz)
    current_frequency_mhz: AtomicU32,
    /// Current CPU voltage (mV)
    current_voltage_mv: AtomicU32,
    /// Estimated power consumption (mW)
    estimated_power_mw: AtomicU32,
}

impl CpuPowerState {
    const fn new() -> Self {
        Self {
            current_pstate: AtomicUsize::new(0),
            target_pstate: AtomicUsize::new(0),
            current_cstate: AtomicU32::new(0),
            pstate_time: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            cstate_time: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            pstate_transitions: AtomicU64::new(0),
            cstate_transitions: AtomicU64::new(0),
            current_frequency_mhz: AtomicU32::new(0),
            current_voltage_mv: AtomicU32::new(0),
            estimated_power_mw: AtomicU32::new(0),
        }
    }
}

/// Maximum number of CPUs supported
const MAX_CPUS: usize = 16;

/// Maximum number of P-states per CPU
const MAX_PSTATES: usize = 8;

/// Global power management state
struct PowerState {
    /// Is power management initialized?
    initialized: AtomicBool,
    /// Current power policy
    policy: Mutex<PowerPolicy>,
    /// Number of CPUs
    num_cpus: AtomicUsize,
    /// Per-CPU power states
    cpu_states: [CpuPowerState; MAX_CPUS],
    /// Available P-states per CPU
    pstates: Mutex<[[Option<PState>; MAX_PSTATES]; MAX_CPUS]>,
    /// DVFS enabled
    dvfs_enabled: AtomicBool,
    /// Total P-state transitions (all CPUs)
    total_pstate_transitions: AtomicU64,
    /// Total C-state transitions (all CPUs)
    total_cstate_transitions: AtomicU64,
    /// Total energy saved (estimated, in millijoules)
    total_energy_saved_mj: AtomicU64,
}

impl PowerState {
    const fn new() -> Self {
        #[allow(clippy::declare_interior_mutable_const)]
        const CPU_STATE: CpuPowerState = CpuPowerState::new();
        const PSTATE_ARRAY: [Option<PState>; MAX_PSTATES] = [None; MAX_PSTATES];

        Self {
            initialized: AtomicBool::new(false),
            policy: Mutex::new(PowerPolicy::Balanced),
            num_cpus: AtomicUsize::new(0),
            cpu_states: [CPU_STATE; MAX_CPUS],
            pstates: Mutex::new([PSTATE_ARRAY; MAX_CPUS]),
            dvfs_enabled: AtomicBool::new(false),
            total_pstate_transitions: AtomicU64::new(0),
            total_cstate_transitions: AtomicU64::new(0),
            total_energy_saved_mj: AtomicU64::new(0),
        }
    }
}

/// Global power management state
static POWER_STATE: PowerState = PowerState::new();

/// Initialize the power management subsystem
pub fn init() -> Result<(), PowerError> {
    if POWER_STATE.initialized.load(Ordering::SeqCst) {
        // In test mode, allow re-initialization and reset state
        #[cfg(any(test, feature = "std"))]
        {
            // Reset policy to default
            *POWER_STATE.policy.lock() = PowerPolicy::Balanced;
            // Reset all CPUs to P0 with corresponding frequency/voltage/power
            let num_cpus = POWER_STATE.num_cpus.load(Ordering::SeqCst);
            let pstates = POWER_STATE.pstates.lock();
            for cpu in 0..num_cpus {
                POWER_STATE.cpu_states[cpu]
                    .current_pstate
                    .store(0, Ordering::SeqCst);
                POWER_STATE.cpu_states[cpu]
                    .target_pstate
                    .store(0, Ordering::SeqCst);

                // Reset frequency/voltage/power to P0 values if available
                if let Some(p0) = pstates[cpu][0] {
                    POWER_STATE.cpu_states[cpu]
                        .current_frequency_mhz
                        .store(p0.frequency_mhz, Ordering::SeqCst);
                    POWER_STATE.cpu_states[cpu]
                        .current_voltage_mv
                        .store(p0.voltage_mv, Ordering::SeqCst);
                    POWER_STATE.cpu_states[cpu]
                        .estimated_power_mw
                        .store(p0.power_mw, Ordering::SeqCst);
                }
            }
            drop(pstates);
            return Ok(());
        }
        #[cfg(not(any(test, feature = "std")))]
        {
            return Err(PowerError::AlreadyInitialized);
        }
    }

    // Detect number of CPUs
    let num_cpus = detect_num_cpus();
    POWER_STATE.num_cpus.store(num_cpus, Ordering::SeqCst);

    // Detect and initialize P-states for each CPU
    for cpu in 0..num_cpus {
        detect_pstates(cpu)?;
    }

    // Set default policy
    *POWER_STATE.policy.lock() = PowerPolicy::Balanced;

    // Mark as initialized
    POWER_STATE.initialized.store(true, Ordering::SeqCst);

    Ok(())
}

/// Detect number of CPUs
fn detect_num_cpus() -> usize {
    // Try to get from per-CPU module
    #[cfg(feature = "std")]
    {
        // In std mode, default to 1 CPU for testing
        1
    }
    #[cfg(not(feature = "std"))]
    {
        // In production, detect from hardware
        // For now, default to 1
        1
    }
}

/// Detect available P-states for a CPU
fn detect_pstates(cpu: usize) -> Result<(), PowerError> {
    if cpu >= MAX_CPUS {
        return Err(PowerError::InvalidCpu);
    }

    // Default P-states for testing/simulation
    // In production, these would be detected from ACPI or hardware
    let default_pstates = [
        Some(PState::new(0, 3000, 1200, 45000)), // P0: 3.0 GHz, 1.2V, 45W
        Some(PState::new(1, 2400, 1100, 25000)), // P1: 2.4 GHz, 1.1V, 25W
        Some(PState::new(2, 1800, 1000, 15000)), // P2: 1.8 GHz, 1.0V, 15W
        Some(PState::new(3, 1200, 900, 8000)),   // P3: 1.2 GHz, 0.9V, 8W
        None,
        None,
        None,
        None,
    ];

    let mut pstates = POWER_STATE.pstates.lock();
    pstates[cpu] = default_pstates;

    // Set initial frequency to P0 (maximum performance)
    let cpu_state = &POWER_STATE.cpu_states[cpu];
    cpu_state.current_pstate.store(0, Ordering::SeqCst);
    cpu_state.target_pstate.store(0, Ordering::SeqCst);
    cpu_state
        .current_frequency_mhz
        .store(3000, Ordering::SeqCst);
    cpu_state.current_voltage_mv.store(1200, Ordering::SeqCst);
    cpu_state.estimated_power_mw.store(45000, Ordering::SeqCst);

    Ok(())
}

/// Set power management policy
pub fn set_policy(policy: PowerPolicy) -> Result<(), PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    *POWER_STATE.policy.lock() = policy;

    // Apply policy to all CPUs
    let num_cpus = POWER_STATE.num_cpus.load(Ordering::SeqCst);
    for cpu in 0..num_cpus {
        apply_policy_to_cpu(cpu, policy)?;
    }

    Ok(())
}

/// Apply power policy to a specific CPU
fn apply_policy_to_cpu(cpu: usize, policy: PowerPolicy) -> Result<(), PowerError> {
    if cpu >= MAX_CPUS {
        return Err(PowerError::InvalidCpu);
    }

    // Determine target P-state based on policy
    let target_pstate = match policy {
        PowerPolicy::Performance => 0, // Maximum performance (P0)
        PowerPolicy::Balanced => 1,    // Balanced (P1)
        PowerPolicy::PowerSaver => 2,  // Power saving (P2)
        PowerPolicy::Custom => {
            // Keep current P-state for custom policy
            POWER_STATE.cpu_states[cpu]
                .current_pstate
                .load(Ordering::SeqCst)
        }
    };

    set_cpu_pstate(cpu, target_pstate)
}

/// Get current power policy
pub fn get_policy() -> Result<PowerPolicy, PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    Ok(*POWER_STATE.policy.lock())
}

/// Set CPU P-state (performance state)
pub fn set_cpu_pstate(cpu: usize, pstate: usize) -> Result<(), PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    if cpu >= MAX_CPUS {
        return Err(PowerError::InvalidCpu);
    }

    if pstate >= MAX_PSTATES {
        return Err(PowerError::InvalidPState);
    }

    // Check if P-state is available
    let pstates = POWER_STATE.pstates.lock();
    let pstate_info = pstates[cpu][pstate].ok_or(PowerError::InvalidPState)?;
    drop(pstates);

    // Update CPU state
    let cpu_state = &POWER_STATE.cpu_states[cpu];
    let old_pstate = cpu_state.current_pstate.swap(pstate, Ordering::SeqCst);

    // Only transition if different
    if old_pstate != pstate {
        // Update statistics
        cpu_state.pstate_transitions.fetch_add(1, Ordering::SeqCst);
        POWER_STATE
            .total_pstate_transitions
            .fetch_add(1, Ordering::SeqCst);

        // Update frequency and voltage
        cpu_state
            .current_frequency_mhz
            .store(pstate_info.frequency_mhz, Ordering::SeqCst);
        cpu_state
            .current_voltage_mv
            .store(pstate_info.voltage_mv, Ordering::SeqCst);
        cpu_state
            .estimated_power_mw
            .store(pstate_info.power_mw, Ordering::SeqCst);

        // Apply to hardware
        apply_pstate_to_hardware(cpu, pstate_info)?;
    }

    Ok(())
}

/// Apply P-state to hardware (architecture-specific)
fn apply_pstate_to_hardware(cpu: usize, pstate: PState) -> Result<(), PowerError> {
    #[cfg(target_arch = "x86_64")]
    {
        // On x86_64, write to MSR (Model Specific Register)
        // This is platform-specific (Intel vs AMD)
        // For now, simulate the operation
        let _ = (cpu, pstate);
        Ok(())
    }

    #[cfg(target_arch = "aarch64")]
    {
        // On ARM, use DVFS interface
        let _ = (cpu, pstate);
        Ok(())
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = (cpu, pstate);
        Ok(())
    }
}

/// Get current CPU P-state
pub fn get_cpu_pstate(cpu: usize) -> Result<usize, PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    if cpu >= MAX_CPUS {
        return Err(PowerError::InvalidCpu);
    }

    Ok(POWER_STATE.cpu_states[cpu]
        .current_pstate
        .load(Ordering::SeqCst))
}

/// Get current CPU frequency in MHz
pub fn get_cpu_frequency(cpu: usize) -> Result<u32, PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    if cpu >= MAX_CPUS {
        return Err(PowerError::InvalidCpu);
    }

    Ok(POWER_STATE.cpu_states[cpu]
        .current_frequency_mhz
        .load(Ordering::SeqCst))
}

/// Enter CPU sleep state (C-state)
pub fn enter_cstate(cpu: usize, cstate: CState) -> Result<(), PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    if cpu >= MAX_CPUS {
        return Err(PowerError::InvalidCpu);
    }

    let cpu_state = &POWER_STATE.cpu_states[cpu];
    let old_cstate = cpu_state
        .current_cstate
        .swap(cstate.value() as u32, Ordering::SeqCst);

    if old_cstate != cstate.value() as u32 {
        cpu_state.cstate_transitions.fetch_add(1, Ordering::SeqCst);
        POWER_STATE
            .total_cstate_transitions
            .fetch_add(1, Ordering::SeqCst);
    }

    // Execute architecture-specific idle instruction
    execute_idle_instruction(cstate);

    Ok(())
}

/// Execute architecture-specific idle instruction
fn execute_idle_instruction(cstate: CState) {
    // In test/std mode, don't execute actual halt instructions
    #[cfg(feature = "std")]
    {
        let _ = cstate;
    }

    #[cfg(not(feature = "std"))]
    {
        match cstate {
            CState::C0 => {
                // Active state - do nothing
            }
            CState::C1 => {
                // Halt instruction (skip in test/std mode to avoid SIGILL)
                #[cfg(not(any(test, feature = "std")))]
                {
                    #[cfg(target_arch = "x86_64")]
                    unsafe {
                        core::arch::asm!("hlt");
                    }
                    #[cfg(target_arch = "aarch64")]
                    unsafe {
                        core::arch::asm!("wfi"); // Wait For Interrupt
                    }
                    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
                    {
                        core::hint::spin_loop();
                    }
                }
                #[cfg(any(test, feature = "std"))]
                {
                    // In test mode, just yield to avoid illegal instruction
                    core::hint::spin_loop();
                }
            }
            CState::C2 | CState::C3 | CState::C6 => {
                // Deeper sleep states would require ACPI or platform-specific support
                // For now, fall back to C1 (skip in test/std mode to avoid SIGILL)
                #[cfg(not(any(test, feature = "std")))]
                {
                    #[cfg(target_arch = "x86_64")]
                    unsafe {
                        core::arch::asm!("hlt");
                    }
                    #[cfg(target_arch = "aarch64")]
                    unsafe {
                        core::arch::asm!("wfi");
                    }
                    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
                    {
                        core::hint::spin_loop();
                    }
                }
                #[cfg(any(test, feature = "std"))]
                {
                    // In test mode, just yield to avoid illegal instruction
                    core::hint::spin_loop();
                }
            }
        }
    }
}

/// Enable Dynamic Voltage/Frequency Scaling (DVFS)
pub fn enable_dvfs() -> Result<(), PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    POWER_STATE.dvfs_enabled.store(true, Ordering::SeqCst);
    Ok(())
}

/// Disable DVFS
pub fn disable_dvfs() -> Result<(), PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    POWER_STATE.dvfs_enabled.store(false, Ordering::SeqCst);
    Ok(())
}

/// Check if DVFS is enabled
pub fn is_dvfs_enabled() -> bool {
    POWER_STATE.dvfs_enabled.load(Ordering::SeqCst)
}

/// Power management statistics
#[derive(Debug, Clone, Copy)]
pub struct PowerStats {
    /// Number of CPUs
    pub num_cpus: usize,
    /// Current power policy
    pub policy: PowerPolicy,
    /// DVFS enabled
    pub dvfs_enabled: bool,
    /// Total P-state transitions (all CPUs)
    pub total_pstate_transitions: u64,
    /// Total C-state transitions (all CPUs)
    pub total_cstate_transitions: u64,
    /// Total energy saved (estimated, millijoules)
    pub total_energy_saved_mj: u64,
}

/// Get power management statistics
pub fn get_stats() -> Result<PowerStats, PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    Ok(PowerStats {
        num_cpus: POWER_STATE.num_cpus.load(Ordering::SeqCst),
        policy: *POWER_STATE.policy.lock(),
        dvfs_enabled: POWER_STATE.dvfs_enabled.load(Ordering::SeqCst),
        total_pstate_transitions: POWER_STATE.total_pstate_transitions.load(Ordering::SeqCst),
        total_cstate_transitions: POWER_STATE.total_cstate_transitions.load(Ordering::SeqCst),
        total_energy_saved_mj: POWER_STATE.total_energy_saved_mj.load(Ordering::SeqCst),
    })
}

/// Per-CPU power statistics
#[derive(Debug, Clone, Copy)]
pub struct CpuPowerStats {
    /// CPU ID
    pub cpu: usize,
    /// Current P-state
    pub current_pstate: usize,
    /// Current C-state
    pub current_cstate: u8,
    /// Current frequency (MHz)
    pub current_frequency_mhz: u32,
    /// Current voltage (mV)
    pub current_voltage_mv: u32,
    /// Estimated power consumption (mW)
    pub estimated_power_mw: u32,
    /// P-state transitions
    pub pstate_transitions: u64,
    /// C-state transitions
    pub cstate_transitions: u64,
}

/// Get per-CPU power statistics
pub fn get_cpu_stats(cpu: usize) -> Result<CpuPowerStats, PowerError> {
    if !POWER_STATE.initialized.load(Ordering::SeqCst) {
        return Err(PowerError::NotInitialized);
    }

    if cpu >= MAX_CPUS {
        return Err(PowerError::InvalidCpu);
    }

    let cpu_state = &POWER_STATE.cpu_states[cpu];

    Ok(CpuPowerStats {
        cpu,
        current_pstate: cpu_state.current_pstate.load(Ordering::SeqCst),
        current_cstate: cpu_state.current_cstate.load(Ordering::SeqCst) as u8,
        current_frequency_mhz: cpu_state.current_frequency_mhz.load(Ordering::SeqCst),
        current_voltage_mv: cpu_state.current_voltage_mv.load(Ordering::SeqCst),
        estimated_power_mw: cpu_state.estimated_power_mw.load(Ordering::SeqCst),
        pstate_transitions: cpu_state.pstate_transitions.load(Ordering::SeqCst),
        cstate_transitions: cpu_state.cstate_transitions.load(Ordering::SeqCst),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        init().unwrap();
        let stats = get_stats().unwrap();
        assert!(stats.num_cpus > 0);
    }

    #[test]
    fn test_power_policy() {
        init().unwrap();

        // Test setting different policies
        set_policy(PowerPolicy::Performance).unwrap();
        assert_eq!(get_policy().unwrap(), PowerPolicy::Performance);

        set_policy(PowerPolicy::Balanced).unwrap();
        assert_eq!(get_policy().unwrap(), PowerPolicy::Balanced);

        set_policy(PowerPolicy::PowerSaver).unwrap();
        assert_eq!(get_policy().unwrap(), PowerPolicy::PowerSaver);
    }

    #[test]
    fn test_pstate_transitions() {
        init().unwrap();

        let cpu = 0;
        let initial_stats = get_cpu_stats(cpu).unwrap();
        assert_eq!(initial_stats.current_pstate, 0); // Should start at P0

        // Transition to P1
        set_cpu_pstate(cpu, 1).unwrap();
        let stats = get_cpu_stats(cpu).unwrap();
        assert_eq!(stats.current_pstate, 1);
        assert!(stats.pstate_transitions > initial_stats.pstate_transitions);

        // Frequency should be lower in P1
        assert!(stats.current_frequency_mhz < initial_stats.current_frequency_mhz);
    }

    #[test]
    fn test_get_cpu_frequency() {
        init().unwrap();

        let cpu = 0;
        let freq = get_cpu_frequency(cpu).unwrap();
        assert!(freq > 0); // Should have some frequency
    }

    #[test]
    fn test_dvfs_enable_disable() {
        init().unwrap();

        assert!(!is_dvfs_enabled()); // Should be disabled by default

        enable_dvfs().unwrap();
        assert!(is_dvfs_enabled());

        disable_dvfs().unwrap();
        assert!(!is_dvfs_enabled());
    }

    #[test]
    fn test_cstate() {
        init().unwrap();

        let cpu = 0;

        // Test different C-states
        assert!(enter_cstate(cpu, CState::C0).is_ok());
        assert!(enter_cstate(cpu, CState::C1).is_ok());
    }

    #[test]
    fn test_cstate_values() {
        assert_eq!(CState::C0.value(), 0);
        assert_eq!(CState::C1.value(), 1);
        assert_eq!(CState::C2.value(), 2);
        assert_eq!(CState::C3.value(), 3);
        assert_eq!(CState::C6.value(), 6);
    }

    #[test]
    fn test_cstate_latency() {
        assert_eq!(CState::C0.wakeup_latency_us(), 0);
        assert_eq!(CState::C1.wakeup_latency_us(), 1);
        assert!(CState::C2.wakeup_latency_us() > CState::C1.wakeup_latency_us());
        assert!(CState::C3.wakeup_latency_us() > CState::C2.wakeup_latency_us());
        assert!(CState::C6.wakeup_latency_us() > CState::C3.wakeup_latency_us());
    }

    #[test]
    fn test_cstate_power_savings() {
        assert_eq!(CState::C0.power_savings_percent(), 0);
        assert!(CState::C1.power_savings_percent() > 0);
        assert!(CState::C2.power_savings_percent() > CState::C1.power_savings_percent());
        assert!(CState::C3.power_savings_percent() > CState::C2.power_savings_percent());
        assert!(CState::C6.power_savings_percent() > CState::C3.power_savings_percent());
    }

    #[test]
    fn test_invalid_cpu() {
        init().unwrap();

        let invalid_cpu = MAX_CPUS + 1;
        assert_eq!(
            get_cpu_frequency(invalid_cpu).unwrap_err(),
            PowerError::InvalidCpu
        );
        assert_eq!(
            set_cpu_pstate(invalid_cpu, 0).unwrap_err(),
            PowerError::InvalidCpu
        );
    }

    #[test]
    fn test_invalid_pstate() {
        init().unwrap();

        let cpu = 0;
        let invalid_pstate = MAX_PSTATES + 1;
        assert_eq!(
            set_cpu_pstate(cpu, invalid_pstate).unwrap_err(),
            PowerError::InvalidPState
        );
    }

    #[test]
    fn test_statistics() {
        init().unwrap();

        let stats = get_stats().unwrap();
        assert_eq!(stats.policy, PowerPolicy::Balanced); // Default policy

        // Change policy and verify
        set_policy(PowerPolicy::Performance).unwrap();
        let stats = get_stats().unwrap();
        assert_eq!(stats.policy, PowerPolicy::Performance);
    }

    #[test]
    fn test_cpu_stats() {
        init().unwrap();

        let cpu = 0;
        let stats = get_cpu_stats(cpu).unwrap();

        assert_eq!(stats.cpu, cpu);
        assert!(stats.current_frequency_mhz > 0);
        assert!(stats.current_voltage_mv > 0);
        assert!(stats.estimated_power_mw > 0);
    }

    #[test]
    fn test_pstate_info() {
        let pstate = PState::new(0, 3000, 1200, 45000);
        assert_eq!(pstate.state, 0);
        assert_eq!(pstate.frequency_mhz, 3000);
        assert_eq!(pstate.voltage_mv, 1200);
        assert_eq!(pstate.power_mw, 45000);
    }

    #[test]
    fn test_error_display() {
        let error = PowerError::NotInitialized;
        assert_eq!(format!("{}", error), "Power management not initialized");

        let error = PowerError::InvalidCpu;
        assert_eq!(format!("{}", error), "Invalid CPU ID");
    }
}
