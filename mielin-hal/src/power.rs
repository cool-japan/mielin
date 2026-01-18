//! Power Management Detection
//!
//! Provides CPU frequency detection, P-state/C-state enumeration,
//! and thermal monitoring capabilities.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// CPU frequency information in MHz
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuFrequency {
    /// Current frequency in MHz (0 if unknown)
    pub current_mhz: u32,
    /// Minimum frequency in MHz
    pub min_mhz: u32,
    /// Maximum frequency in MHz
    pub max_mhz: u32,
    /// Base/nominal frequency in MHz
    pub base_mhz: u32,
}

impl CpuFrequency {
    /// Create with unknown/default values
    pub const fn unknown() -> Self {
        Self {
            current_mhz: 0,
            min_mhz: 0,
            max_mhz: 0,
            base_mhz: 0,
        }
    }

    /// Create with known frequency range
    pub const fn new(min_mhz: u32, base_mhz: u32, max_mhz: u32) -> Self {
        Self {
            current_mhz: 0,
            min_mhz,
            max_mhz,
            base_mhz,
        }
    }

    /// Check if frequency information is available
    pub fn is_available(&self) -> bool {
        self.max_mhz > 0
    }

    /// Get frequency range as a ratio (max/min)
    pub fn frequency_ratio(&self) -> f32 {
        if self.min_mhz > 0 {
            self.max_mhz as f32 / self.min_mhz as f32
        } else {
            1.0
        }
    }

    /// Get turbo headroom (max - base) in MHz
    pub fn turbo_headroom(&self) -> u32 {
        self.max_mhz.saturating_sub(self.base_mhz)
    }
}

impl Default for CpuFrequency {
    fn default() -> Self {
        Self::unknown()
    }
}

/// Performance state (P-state) information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PState {
    /// P-state index (P0 = highest performance)
    pub index: u8,
    /// Frequency in MHz for this P-state
    pub frequency_mhz: u32,
    /// Voltage in millivolts (if known)
    pub voltage_mv: Option<u32>,
    /// Whether this is a turbo/boost state
    pub is_turbo: bool,
}

impl PState {
    /// Create a new P-state
    pub const fn new(index: u8, frequency_mhz: u32) -> Self {
        Self {
            index,
            frequency_mhz,
            voltage_mv: None,
            is_turbo: false,
        }
    }

    /// Create a turbo P-state
    pub const fn turbo(index: u8, frequency_mhz: u32) -> Self {
        Self {
            index,
            frequency_mhz,
            voltage_mv: None,
            is_turbo: true,
        }
    }

    /// Create with voltage
    pub const fn with_voltage(mut self, voltage_mv: u32) -> Self {
        self.voltage_mv = Some(voltage_mv);
        self
    }
}

/// Idle state (C-state) information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CState {
    /// C0 - Active state (CPU running)
    C0,
    /// C1 - Halt (clock gated)
    C1,
    /// C1E - Enhanced Halt (lower voltage)
    C1E,
    /// C2 - Stop Clock
    C2,
    /// C3 - Sleep (L1/L2 cache may flush)
    C3,
    /// C6 - Deep Power Down (L1/L2 flushed)
    C6,
    /// C7 - Deeper Sleep (LLC may flush)
    C7,
    /// C8 - Deepest Sleep
    C8,
    /// C10 - Package C10 (near shutdown)
    C10,
}

impl CState {
    /// Get the C-state index
    pub fn index(&self) -> u8 {
        match self {
            CState::C0 => 0,
            CState::C1 => 1,
            CState::C1E => 1,
            CState::C2 => 2,
            CState::C3 => 3,
            CState::C6 => 6,
            CState::C7 => 7,
            CState::C8 => 8,
            CState::C10 => 10,
        }
    }

    /// Get approximate exit latency in microseconds
    pub fn exit_latency_us(&self) -> u32 {
        match self {
            CState::C0 => 0,
            CState::C1 => 1,
            CState::C1E => 2,
            CState::C2 => 10,
            CState::C3 => 50,
            CState::C6 => 100,
            CState::C7 => 150,
            CState::C8 => 200,
            CState::C10 => 500,
        }
    }

    /// Get relative power savings (0-100)
    pub fn power_savings(&self) -> u8 {
        match self {
            CState::C0 => 0,
            CState::C1 => 20,
            CState::C1E => 30,
            CState::C2 => 40,
            CState::C3 => 60,
            CState::C6 => 80,
            CState::C7 => 85,
            CState::C8 => 90,
            CState::C10 => 95,
        }
    }

    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            CState::C0 => "C0 (Active)",
            CState::C1 => "C1 (Halt)",
            CState::C1E => "C1E (Enhanced Halt)",
            CState::C2 => "C2 (Stop Clock)",
            CState::C3 => "C3 (Sleep)",
            CState::C6 => "C6 (Deep Power Down)",
            CState::C7 => "C7 (Deeper Sleep)",
            CState::C8 => "C8 (Deepest Sleep)",
            CState::C10 => "C10 (Package Sleep)",
        }
    }
}

/// C-state support information
#[derive(Debug, Clone, Default)]
pub struct CStateSupport {
    /// Supported C-states
    pub states: Vec<CState>,
    /// Whether hardware coordination is available
    pub hw_coordination: bool,
    /// Whether MWAIT instruction is supported
    pub mwait_supported: bool,
    /// Maximum C-state depth supported
    pub max_cstate: u8,
}

impl CStateSupport {
    /// Create with no C-state support
    pub fn none() -> Self {
        Self {
            states: vec![CState::C0],
            hw_coordination: false,
            mwait_supported: false,
            max_cstate: 0,
        }
    }

    /// Create with basic C-state support (C0, C1)
    pub fn basic() -> Self {
        Self {
            states: vec![CState::C0, CState::C1],
            hw_coordination: false,
            mwait_supported: true,
            max_cstate: 1,
        }
    }

    /// Create with full C-state support
    pub fn full() -> Self {
        Self {
            states: vec![
                CState::C0,
                CState::C1,
                CState::C1E,
                CState::C3,
                CState::C6,
                CState::C7,
            ],
            hw_coordination: true,
            mwait_supported: true,
            max_cstate: 7,
        }
    }

    /// Get deepest available C-state
    pub fn deepest_state(&self) -> Option<&CState> {
        self.states.iter().max_by_key(|s| s.index())
    }

    /// Check if a specific C-state is supported
    pub fn supports(&self, state: CState) -> bool {
        self.states.contains(&state)
    }
}

/// Temperature sensor type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalSensorType {
    /// Package/Die temperature
    Package,
    /// Individual core temperature
    Core,
    /// Socket/CPU temperature
    Socket,
    /// Memory controller temperature
    Memory,
    /// GPU temperature
    Gpu,
    /// Ambient/board temperature
    Ambient,
}

/// Temperature reading
#[derive(Debug, Clone, Copy)]
pub struct Temperature {
    /// Temperature in Celsius
    pub celsius: f32,
    /// Sensor type
    pub sensor_type: ThermalSensorType,
    /// Sensor ID/index
    pub sensor_id: u8,
}

impl Temperature {
    /// Create a new temperature reading
    pub fn new(celsius: f32, sensor_type: ThermalSensorType, sensor_id: u8) -> Self {
        Self {
            celsius,
            sensor_type,
            sensor_id,
        }
    }

    /// Get temperature in Fahrenheit
    pub fn fahrenheit(&self) -> f32 {
        self.celsius * 9.0 / 5.0 + 32.0
    }

    /// Get temperature in Kelvin
    pub fn kelvin(&self) -> f32 {
        self.celsius + 273.15
    }
}

/// Thermal monitoring capabilities
#[derive(Debug, Clone, Default)]
pub struct ThermalCapabilities {
    /// Available temperature sensors
    pub sensors: Vec<ThermalSensorType>,
    /// TJunction max (thermal throttle point) in Celsius
    pub tjunction_max: Option<u32>,
    /// Whether thermal throttling is supported
    pub throttling_supported: bool,
    /// Whether thermal trip points are configurable
    pub trip_points_configurable: bool,
    /// Number of thermal zones
    pub thermal_zones: u8,
}

impl ThermalCapabilities {
    /// Create with no thermal monitoring
    pub fn none() -> Self {
        Self::default()
    }

    /// Create with basic thermal monitoring
    pub fn basic(tjunction_max: u32) -> Self {
        Self {
            sensors: vec![ThermalSensorType::Package],
            tjunction_max: Some(tjunction_max),
            throttling_supported: true,
            trip_points_configurable: false,
            thermal_zones: 1,
        }
    }

    /// Create with full thermal monitoring
    pub fn full(tjunction_max: u32, core_count: u8) -> Self {
        let mut sensors = vec![ThermalSensorType::Package];
        for _ in 0..core_count {
            sensors.push(ThermalSensorType::Core);
        }
        Self {
            sensors,
            tjunction_max: Some(tjunction_max),
            throttling_supported: true,
            trip_points_configurable: true,
            thermal_zones: core_count.saturating_add(1),
        }
    }

    /// Check if thermal monitoring is available
    pub fn is_available(&self) -> bool {
        !self.sensors.is_empty()
    }

    /// Get headroom before throttling
    pub fn thermal_headroom(&self, current_temp: f32) -> Option<f32> {
        self.tjunction_max.map(|tj| tj as f32 - current_temp)
    }
}

/// CPU power domain information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerDomain {
    /// Package/socket level
    Package,
    /// Power Plane 0 (typically cores)
    PP0,
    /// Power Plane 1 (typically GPU)
    PP1,
    /// DRAM domain
    Dram,
    /// Platform domain
    Platform,
}

/// Thermal Design Power (TDP) information
#[derive(Debug, Clone, Copy)]
pub struct TdpInfo {
    /// Base TDP in Watts
    pub base_tdp_watts: u32,
    /// Maximum turbo power in Watts
    pub max_turbo_watts: Option<u32>,
    /// Minimum power in Watts
    pub min_power_watts: Option<u32>,
    /// Power limit 1 (long term) in Watts
    pub pl1_watts: Option<u32>,
    /// Power limit 2 (short term) in Watts
    pub pl2_watts: Option<u32>,
    /// Power domain
    pub domain: PowerDomain,
}

impl TdpInfo {
    /// Create basic TDP info
    pub const fn new(base_tdp_watts: u32) -> Self {
        Self {
            base_tdp_watts,
            max_turbo_watts: None,
            min_power_watts: None,
            pl1_watts: None,
            pl2_watts: None,
            domain: PowerDomain::Package,
        }
    }

    /// Create with power limits
    pub const fn with_limits(base_tdp_watts: u32, pl1: u32, pl2: u32) -> Self {
        Self {
            base_tdp_watts,
            max_turbo_watts: Some(pl2),
            min_power_watts: None,
            pl1_watts: Some(pl1),
            pl2_watts: Some(pl2),
            domain: PowerDomain::Package,
        }
    }

    /// Power headroom above base TDP
    pub fn turbo_headroom(&self) -> Option<u32> {
        self.max_turbo_watts
            .map(|m| m.saturating_sub(self.base_tdp_watts))
    }
}

/// Power management governor/policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerGovernor {
    /// Maximum performance (always max frequency)
    Performance,
    /// Balanced (scale based on load)
    Balanced,
    /// Power saving (prefer lower frequencies)
    PowerSave,
    /// On-demand frequency scaling
    OnDemand,
    /// Conservative scaling (gradual changes)
    Conservative,
    /// Userspace (manual control)
    Userspace,
    /// Schedutil (scheduler-driven)
    Schedutil,
}

impl PowerGovernor {
    /// Get governor name
    pub fn name(&self) -> &'static str {
        match self {
            PowerGovernor::Performance => "performance",
            PowerGovernor::Balanced => "balanced",
            PowerGovernor::PowerSave => "powersave",
            PowerGovernor::OnDemand => "ondemand",
            PowerGovernor::Conservative => "conservative",
            PowerGovernor::Userspace => "userspace",
            PowerGovernor::Schedutil => "schedutil",
        }
    }

    /// Get all standard governors
    pub fn all() -> &'static [PowerGovernor] {
        &[
            PowerGovernor::Performance,
            PowerGovernor::Balanced,
            PowerGovernor::PowerSave,
            PowerGovernor::OnDemand,
            PowerGovernor::Conservative,
            PowerGovernor::Userspace,
            PowerGovernor::Schedutil,
        ]
    }
}

/// Energy Performance Preference (EPP)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergyPreference {
    /// Maximum performance (EPP 0)
    Performance,
    /// Balance performance (EPP ~64)
    BalancePerformance,
    /// Balance power (EPP ~128)
    BalancePower,
    /// Power saving (EPP ~192)
    Power,
    /// Default/HW managed
    Default,
}

impl EnergyPreference {
    /// Get EPP value (0-255)
    pub fn epp_value(&self) -> u8 {
        match self {
            EnergyPreference::Performance => 0,
            EnergyPreference::BalancePerformance => 64,
            EnergyPreference::BalancePower => 128,
            EnergyPreference::Power => 192,
            EnergyPreference::Default => 128,
        }
    }
}

/// Complete power management information
#[derive(Debug, Clone)]
pub struct PowerInfo {
    /// CPU frequency information
    pub frequency: CpuFrequency,
    /// Available P-states
    pub p_states: Vec<PState>,
    /// C-state support
    pub c_states: CStateSupport,
    /// Thermal capabilities
    pub thermal: ThermalCapabilities,
    /// TDP information
    pub tdp: Option<TdpInfo>,
    /// Available governors
    pub governors: Vec<PowerGovernor>,
    /// Current governor
    pub current_governor: Option<PowerGovernor>,
    /// Energy Performance Preference support
    pub epp_supported: bool,
    /// Speed Shift (HWP) support
    pub hwp_supported: bool,
    /// Turbo Boost support
    pub turbo_supported: bool,
    /// Turbo enabled
    pub turbo_enabled: bool,
}

impl PowerInfo {
    /// Create minimal power info
    pub fn minimal() -> Self {
        Self {
            frequency: CpuFrequency::unknown(),
            p_states: Vec::new(),
            c_states: CStateSupport::none(),
            thermal: ThermalCapabilities::none(),
            tdp: None,
            governors: Vec::new(),
            current_governor: None,
            epp_supported: false,
            hwp_supported: false,
            turbo_supported: false,
            turbo_enabled: false,
        }
    }

    /// Check if power management is available
    pub fn is_available(&self) -> bool {
        self.frequency.is_available() || !self.p_states.is_empty()
    }

    /// Get number of P-states
    pub fn p_state_count(&self) -> usize {
        self.p_states.len()
    }

    /// Get highest performance P-state
    pub fn highest_p_state(&self) -> Option<&PState> {
        self.p_states.first()
    }

    /// Get lowest power P-state
    pub fn lowest_p_state(&self) -> Option<&PState> {
        self.p_states.last()
    }
}

impl Default for PowerInfo {
    fn default() -> Self {
        Self::minimal()
    }
}

/// Power management summary for quick overview
#[derive(Debug, Clone)]
pub struct PowerSummary {
    /// Frequency range string
    pub frequency_range: String,
    /// Number of P-states
    pub p_state_count: usize,
    /// Deepest C-state supported
    pub max_cstate: u8,
    /// TDP in Watts
    pub tdp_watts: Option<u32>,
    /// Thermal limit in Celsius
    pub thermal_limit_c: Option<u32>,
    /// Whether turbo is available
    pub turbo_available: bool,
    /// Whether HWP is available
    pub hwp_available: bool,
}

impl PowerSummary {
    /// Create from PowerInfo
    pub fn from_info(info: &PowerInfo) -> Self {
        let frequency_range = if info.frequency.is_available() {
            alloc::format!(
                "{}-{} MHz (base: {} MHz)",
                info.frequency.min_mhz,
                info.frequency.max_mhz,
                info.frequency.base_mhz
            )
        } else {
            String::from("Unknown")
        };

        Self {
            frequency_range,
            p_state_count: info.p_states.len(),
            max_cstate: info.c_states.max_cstate,
            tdp_watts: info.tdp.map(|t| t.base_tdp_watts),
            thermal_limit_c: info.thermal.tjunction_max,
            turbo_available: info.turbo_supported,
            hwp_available: info.hwp_supported,
        }
    }
}

// ============================================================================
// Platform-specific detection
// ============================================================================

/// Detect CPU frequency information
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub fn detect_frequency() -> CpuFrequency {
    // Use CPUID for frequency detection
    detect_frequency_cpuid()
}

#[cfg(all(target_arch = "x86_64", not(target_os = "linux")))]
pub fn detect_frequency() -> CpuFrequency {
    detect_frequency_cpuid()
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
pub fn detect_frequency() -> CpuFrequency {
    // AArch64: Try to detect from system registers
    // CNTFRQ_EL0 gives timer frequency, not CPU frequency
    // In practice, need to read from /sys/devices/system/cpu/
    CpuFrequency::unknown()
}

#[cfg(all(target_arch = "aarch64", not(target_os = "linux")))]
pub fn detect_frequency() -> CpuFrequency {
    // AArch64 on non-Linux (e.g., macOS): No direct access to CPU frequency
    CpuFrequency::unknown()
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub fn detect_frequency() -> CpuFrequency {
    CpuFrequency::unknown()
}

/// Detect frequency using CPUID (x86_64)
#[cfg(target_arch = "x86_64")]
fn detect_frequency_cpuid() -> CpuFrequency {
    // CPUID leaf 0x16 provides frequency information on newer CPUs
    let result = unsafe { core::arch::x86_64::__cpuid(0) };
    let max_leaf = result.eax;

    if max_leaf >= 0x16 {
        let freq = unsafe { core::arch::x86_64::__cpuid(0x16) };
        let base_mhz = freq.eax;
        let max_mhz = freq.ebx;
        let bus_mhz = freq.ecx;

        if base_mhz > 0 && max_mhz > 0 {
            // Estimate min frequency (typically 800 MHz or lower)
            let min_mhz = if bus_mhz > 0 { bus_mhz } else { 800 };
            return CpuFrequency::new(min_mhz.min(base_mhz), base_mhz, max_mhz);
        }
    }

    // Fallback to brand string parsing for older CPUs
    detect_frequency_from_brand()
}

/// Parse frequency from CPUID brand string
#[cfg(target_arch = "x86_64")]
fn detect_frequency_from_brand() -> CpuFrequency {
    let mut brand = [0u32; 12];

    for i in 0..3usize {
        let result = unsafe { core::arch::x86_64::__cpuid(0x80000002 + i as u32) };
        brand[i * 4] = result.eax;
        brand[i * 4 + 1] = result.ebx;
        brand[i * 4 + 2] = result.ecx;
        brand[i * 4 + 3] = result.edx;
    }

    let brand_bytes: [u8; 48] = unsafe { core::mem::transmute(brand) };
    let brand_str = core::str::from_utf8(&brand_bytes).unwrap_or("");

    // Look for frequency pattern like "@ 3.60GHz" or "@ 2.4 GHz"
    if let Some(pos) = brand_str.find('@') {
        let freq_part = &brand_str[pos + 1..];
        if let Some(ghz_pos) = freq_part.to_ascii_lowercase().find("ghz") {
            let num_str = freq_part[..ghz_pos].trim();
            if let Ok(ghz) = num_str.parse::<f32>() {
                let base_mhz = (ghz * 1000.0) as u32;
                // Estimate range based on typical Intel/AMD behavior
                let min_mhz = 800;
                let max_mhz = base_mhz + (base_mhz / 4); // ~25% turbo
                return CpuFrequency::new(min_mhz, base_mhz, max_mhz);
            }
        }
    }

    CpuFrequency::unknown()
}

/// Detect P-states
#[cfg(target_arch = "x86_64")]
pub fn detect_p_states() -> Vec<PState> {
    let freq = detect_frequency();
    if !freq.is_available() {
        return Vec::new();
    }

    let mut states = Vec::new();

    // Generate typical P-states based on frequency range
    let range = freq.max_mhz.saturating_sub(freq.min_mhz);
    let step = if range > 0 { range / 8 } else { 100 };

    // P0 - Turbo (if available)
    if freq.max_mhz > freq.base_mhz {
        states.push(PState::turbo(0, freq.max_mhz));
    }

    // P1 - Base frequency
    states.push(PState::new(states.len() as u8, freq.base_mhz));

    // Additional P-states stepping down
    let mut current = freq.base_mhz.saturating_sub(step);
    while current >= freq.min_mhz && states.len() < 16 {
        states.push(PState::new(states.len() as u8, current));
        current = current.saturating_sub(step);
    }

    // Ensure minimum P-state is included
    if states.last().map(|s| s.frequency_mhz).unwrap_or(0) > freq.min_mhz {
        states.push(PState::new(states.len() as u8, freq.min_mhz));
    }

    states
}

#[cfg(not(target_arch = "x86_64"))]
pub fn detect_p_states() -> Vec<PState> {
    Vec::new()
}

/// Detect C-state support
#[cfg(target_arch = "x86_64")]
pub fn detect_c_states() -> CStateSupport {
    // Check CPUID for MWAIT/MONITOR support
    let result = unsafe { core::arch::x86_64::__cpuid(1) };
    let mwait_supported = (result.ecx & (1 << 3)) != 0;

    if !mwait_supported {
        return CStateSupport::none();
    }

    // CPUID leaf 5 provides MWAIT parameters
    let mwait = unsafe { core::arch::x86_64::__cpuid(5) };
    let _smallest_line = mwait.eax as u16;
    let _largest_line = mwait.ebx as u16;
    let extensions = mwait.ecx;
    let c_state_hints = mwait.edx;

    let hw_coordination = (extensions & 1) != 0;

    let mut states = vec![CState::C0];

    // Parse C-state sub-states from EDX
    // Bits 3:0 = C0, 7:4 = C1, etc.
    if c_state_hints & 0xF0 > 0 {
        states.push(CState::C1);
    }
    if c_state_hints & 0xF00 > 0 {
        states.push(CState::C2);
    }
    if c_state_hints & 0xF000 > 0 {
        states.push(CState::C3);
    }

    // Extended C-states (common on modern Intel)
    // Check for deeper C-states via extended feature check
    let max_leaf = unsafe { core::arch::x86_64::__cpuid(0) }.eax;
    if max_leaf >= 0x15 {
        states.push(CState::C6);
        states.push(CState::C7);
    }

    let max_cstate = states.iter().map(|s| s.index()).max().unwrap_or(0);

    CStateSupport {
        states,
        hw_coordination,
        mwait_supported: true,
        max_cstate,
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn detect_c_states() -> CStateSupport {
    CStateSupport::basic()
}

/// Detect thermal capabilities
#[cfg(target_arch = "x86_64")]
pub fn detect_thermal() -> ThermalCapabilities {
    // Check for thermal monitoring via CPUID
    let result = unsafe { core::arch::x86_64::__cpuid(1) };
    let has_tm = (result.edx & (1 << 29)) != 0; // Thermal Monitor
    let has_tm2 = (result.ecx & (1 << 8)) != 0; // Thermal Monitor 2

    if !has_tm && !has_tm2 {
        return ThermalCapabilities::none();
    }

    // Check leaf 6 for thermal and power management
    let thermal = unsafe { core::arch::x86_64::__cpuid(6) };
    let has_digital_sensor = (thermal.eax & 1) != 0;
    let has_turbo = (thermal.eax & (1 << 1)) != 0;
    let has_hwp = (thermal.eax & (1 << 7)) != 0;

    if !has_digital_sensor {
        return ThermalCapabilities::none();
    }

    // Typical TJunction max values
    let tjunction_max = if has_hwp { 100 } else { 105 };

    ThermalCapabilities {
        sensors: vec![ThermalSensorType::Package],
        tjunction_max: Some(tjunction_max),
        throttling_supported: has_tm || has_tm2,
        trip_points_configurable: has_turbo,
        thermal_zones: 1,
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn detect_thermal() -> ThermalCapabilities {
    ThermalCapabilities::none()
}

/// Detect HWP (Hardware P-states) support
#[cfg(target_arch = "x86_64")]
pub fn detect_hwp_support() -> bool {
    let result = unsafe { core::arch::x86_64::__cpuid(6) };
    (result.eax & (1 << 7)) != 0
}

#[cfg(not(target_arch = "x86_64"))]
pub fn detect_hwp_support() -> bool {
    false
}

/// Detect turbo boost support
#[cfg(target_arch = "x86_64")]
pub fn detect_turbo_support() -> bool {
    let result = unsafe { core::arch::x86_64::__cpuid(6) };
    (result.eax & (1 << 1)) != 0
}

#[cfg(not(target_arch = "x86_64"))]
pub fn detect_turbo_support() -> bool {
    false
}

/// Detect complete power management information
pub fn detect_power_info() -> PowerInfo {
    let frequency = detect_frequency();
    let p_states = detect_p_states();
    let c_states = detect_c_states();
    let thermal = detect_thermal();
    let hwp_supported = detect_hwp_support();
    let turbo_supported = detect_turbo_support();

    // Default governors (actual detection would need sysfs on Linux)
    let governors = vec![
        PowerGovernor::Performance,
        PowerGovernor::Balanced,
        PowerGovernor::PowerSave,
    ];

    PowerInfo {
        frequency,
        p_states,
        c_states,
        thermal,
        tdp: None, // Would need MSR access for Intel RAPL
        governors,
        current_governor: None,
        epp_supported: hwp_supported,
        hwp_supported,
        turbo_supported,
        turbo_enabled: turbo_supported, // Assume enabled if supported
    }
}

/// Get power management summary
pub fn get_power_summary() -> PowerSummary {
    let info = detect_power_info();
    PowerSummary::from_info(&info)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_frequency_unknown() {
        let freq = CpuFrequency::unknown();
        assert!(!freq.is_available());
        assert_eq!(freq.frequency_ratio(), 1.0);
    }

    #[test]
    fn test_cpu_frequency_new() {
        let freq = CpuFrequency::new(800, 3600, 5200);
        assert!(freq.is_available());
        assert_eq!(freq.min_mhz, 800);
        assert_eq!(freq.base_mhz, 3600);
        assert_eq!(freq.max_mhz, 5200);
    }

    #[test]
    fn test_cpu_frequency_ratio() {
        let freq = CpuFrequency::new(800, 2400, 4800);
        assert_eq!(freq.frequency_ratio(), 6.0);
    }

    #[test]
    fn test_cpu_frequency_turbo_headroom() {
        let freq = CpuFrequency::new(800, 3600, 5200);
        assert_eq!(freq.turbo_headroom(), 1600);
    }

    #[test]
    fn test_p_state_new() {
        let state = PState::new(1, 3600);
        assert_eq!(state.index, 1);
        assert_eq!(state.frequency_mhz, 3600);
        assert!(!state.is_turbo);
    }

    #[test]
    fn test_p_state_turbo() {
        let state = PState::turbo(0, 5200);
        assert_eq!(state.index, 0);
        assert!(state.is_turbo);
    }

    #[test]
    fn test_p_state_with_voltage() {
        let state = PState::new(1, 3600).with_voltage(1100);
        assert_eq!(state.voltage_mv, Some(1100));
    }

    #[test]
    fn test_c_state_index() {
        assert_eq!(CState::C0.index(), 0);
        assert_eq!(CState::C1.index(), 1);
        assert_eq!(CState::C6.index(), 6);
    }

    #[test]
    fn test_c_state_exit_latency() {
        assert_eq!(CState::C0.exit_latency_us(), 0);
        assert_eq!(CState::C1.exit_latency_us(), 1);
        assert!(CState::C6.exit_latency_us() > CState::C3.exit_latency_us());
    }

    #[test]
    fn test_c_state_power_savings() {
        assert_eq!(CState::C0.power_savings(), 0);
        assert!(CState::C6.power_savings() > CState::C3.power_savings());
    }

    #[test]
    fn test_c_state_support_none() {
        let support = CStateSupport::none();
        assert_eq!(support.states.len(), 1);
        assert_eq!(support.max_cstate, 0);
    }

    #[test]
    fn test_c_state_support_full() {
        let support = CStateSupport::full();
        assert!(support.states.len() > 4);
        assert!(support.hw_coordination);
        assert!(support.mwait_supported);
    }

    #[test]
    fn test_c_state_support_deepest() {
        let support = CStateSupport::full();
        let deepest = support
            .deepest_state()
            .expect("full support should have deepest state");
        assert!(deepest.index() >= 6);
    }

    #[test]
    fn test_temperature_conversions() {
        let temp = Temperature::new(25.0, ThermalSensorType::Package, 0);
        assert_eq!(temp.fahrenheit(), 77.0);
        assert!((temp.kelvin() - 298.15).abs() < 0.01);
    }

    #[test]
    fn test_thermal_capabilities_none() {
        let thermal = ThermalCapabilities::none();
        assert!(!thermal.is_available());
        assert!(thermal.tjunction_max.is_none());
    }

    #[test]
    fn test_thermal_capabilities_basic() {
        let thermal = ThermalCapabilities::basic(100);
        assert!(thermal.is_available());
        assert_eq!(thermal.tjunction_max, Some(100));
    }

    #[test]
    fn test_thermal_headroom() {
        let thermal = ThermalCapabilities::basic(100);
        assert_eq!(thermal.thermal_headroom(60.0), Some(40.0));
    }

    #[test]
    fn test_tdp_info_new() {
        let tdp = TdpInfo::new(125);
        assert_eq!(tdp.base_tdp_watts, 125);
        assert!(tdp.turbo_headroom().is_none());
    }

    #[test]
    fn test_tdp_info_with_limits() {
        let tdp = TdpInfo::with_limits(125, 125, 253);
        assert_eq!(tdp.base_tdp_watts, 125);
        assert_eq!(tdp.turbo_headroom(), Some(128));
    }

    #[test]
    fn test_power_governor_names() {
        assert_eq!(PowerGovernor::Performance.name(), "performance");
        assert_eq!(PowerGovernor::PowerSave.name(), "powersave");
    }

    #[test]
    fn test_energy_preference_values() {
        assert_eq!(EnergyPreference::Performance.epp_value(), 0);
        assert_eq!(EnergyPreference::Power.epp_value(), 192);
    }

    #[test]
    fn test_power_info_minimal() {
        let info = PowerInfo::minimal();
        assert!(!info.is_available());
        assert_eq!(info.p_state_count(), 0);
    }

    #[test]
    fn test_detect_frequency() {
        let freq = detect_frequency();
        // Should return something (even if unknown on non-x86)
        let _ = freq.is_available();
    }

    #[test]
    fn test_detect_p_states() {
        let states = detect_p_states();
        // States may be empty on non-x86 or VMs
        if !states.is_empty() {
            assert!(states[0].frequency_mhz > 0);
        }
    }

    #[test]
    fn test_detect_c_states() {
        let c_states = detect_c_states();
        // Should at least have C0
        assert!(!c_states.states.is_empty());
    }

    #[test]
    fn test_detect_thermal() {
        let thermal = detect_thermal();
        // May or may not be available depending on platform
        let _ = thermal.is_available();
    }

    #[test]
    fn test_detect_power_info() {
        let info = detect_power_info();
        // Verify structure is populated
        let _ = info.is_available();
    }

    #[test]
    fn test_power_summary() {
        let info = detect_power_info();
        let summary = PowerSummary::from_info(&info);
        assert!(!summary.frequency_range.is_empty());
    }
}
