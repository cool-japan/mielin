//! Power management for embedded devices
//!
//! Provides advanced power management including DVFS, peripheral power gating,
//! clock gating, and wake-up source configuration.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Power mode for the device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    /// Full performance mode
    Normal,
    /// Reduced power, still responsive
    LowPower,
    /// Minimal power, limited functionality
    UltraLowPower,
    /// Deep sleep, wake on interrupt
    Sleep,
    /// Standby mode with RAM retention
    Standby,
    /// Shutdown, no RAM retention
    Shutdown,
}

impl PowerMode {
    /// Get relative power consumption (0-100)
    pub fn power_consumption(&self) -> u8 {
        match self {
            PowerMode::Normal => 100,
            PowerMode::LowPower => 50,
            PowerMode::UltraLowPower => 20,
            PowerMode::Sleep => 5,
            PowerMode::Standby => 1,
            PowerMode::Shutdown => 0,
        }
    }

    /// Get wake-up latency in microseconds
    pub fn wake_latency_us(&self) -> u32 {
        match self {
            PowerMode::Normal => 0,
            PowerMode::LowPower => 1,
            PowerMode::UltraLowPower => 10,
            PowerMode::Sleep => 100,
            PowerMode::Standby => 1000,
            PowerMode::Shutdown => 100_000,
        }
    }

    /// Check if RAM is retained in this mode
    pub fn retains_ram(&self) -> bool {
        match self {
            PowerMode::Normal => true,
            PowerMode::LowPower => true,
            PowerMode::UltraLowPower => true,
            PowerMode::Sleep => true,
            PowerMode::Standby => true,
            PowerMode::Shutdown => false,
        }
    }
}

/// Battery status information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryStatus {
    pub level_percent: u8,
    pub is_charging: bool,
}

impl BatteryStatus {
    pub fn should_migrate(&self) -> bool {
        self.level_percent < 20 && !self.is_charging
    }
}

// ============================================================================
// DVFS (Dynamic Voltage and Frequency Scaling)
// ============================================================================

/// Performance level for DVFS
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PerformanceLevel {
    /// Minimum performance, maximum battery life
    Minimum,
    /// Low performance
    Low,
    /// Balanced performance/power
    Balanced,
    /// High performance
    High,
    /// Maximum performance, maximum power
    Maximum,
}

impl PerformanceLevel {
    /// Get all performance levels in order
    pub fn all() -> &'static [PerformanceLevel] {
        &[
            PerformanceLevel::Minimum,
            PerformanceLevel::Low,
            PerformanceLevel::Balanced,
            PerformanceLevel::High,
            PerformanceLevel::Maximum,
        ]
    }

    /// Get the next higher level
    pub fn higher(&self) -> Option<PerformanceLevel> {
        match self {
            PerformanceLevel::Minimum => Some(PerformanceLevel::Low),
            PerformanceLevel::Low => Some(PerformanceLevel::Balanced),
            PerformanceLevel::Balanced => Some(PerformanceLevel::High),
            PerformanceLevel::High => Some(PerformanceLevel::Maximum),
            PerformanceLevel::Maximum => None,
        }
    }

    /// Get the next lower level
    pub fn lower(&self) -> Option<PerformanceLevel> {
        match self {
            PerformanceLevel::Minimum => None,
            PerformanceLevel::Low => Some(PerformanceLevel::Minimum),
            PerformanceLevel::Balanced => Some(PerformanceLevel::Low),
            PerformanceLevel::High => Some(PerformanceLevel::Balanced),
            PerformanceLevel::Maximum => Some(PerformanceLevel::High),
        }
    }
}

/// DVFS operating point
#[derive(Debug, Clone, Copy)]
pub struct OperatingPoint {
    /// Performance level
    pub level: PerformanceLevel,
    /// Frequency in MHz
    pub frequency_mhz: u32,
    /// Voltage in millivolts
    pub voltage_mv: u32,
    /// Estimated power consumption in milliwatts
    pub power_mw: u32,
}

impl OperatingPoint {
    /// Create a new operating point
    pub const fn new(
        level: PerformanceLevel,
        frequency_mhz: u32,
        voltage_mv: u32,
        power_mw: u32,
    ) -> Self {
        Self {
            level,
            frequency_mhz,
            voltage_mv,
            power_mw,
        }
    }

    /// Calculate energy efficiency (MHz per mW)
    pub fn efficiency(&self) -> f32 {
        if self.power_mw > 0 {
            self.frequency_mhz as f32 / self.power_mw as f32
        } else {
            0.0
        }
    }
}

/// DVFS configuration and state
#[derive(Debug)]
pub struct DvfsController {
    /// Available operating points
    operating_points: Vec<OperatingPoint>,
    /// Current performance level
    current_level: PerformanceLevel,
    /// Minimum allowed level
    min_level: PerformanceLevel,
    /// Maximum allowed level
    max_level: PerformanceLevel,
    /// Auto-scaling enabled
    auto_scaling: bool,
    /// Utilization threshold for scaling up (percentage)
    scale_up_threshold: u8,
    /// Utilization threshold for scaling down (percentage)
    scale_down_threshold: u8,
    /// Transition count
    transitions: u64,
}

impl DvfsController {
    /// Create a new DVFS controller with default operating points
    pub fn new() -> Self {
        let operating_points = vec![
            OperatingPoint::new(PerformanceLevel::Minimum, 48, 900, 10),
            OperatingPoint::new(PerformanceLevel::Low, 96, 1000, 30),
            OperatingPoint::new(PerformanceLevel::Balanced, 168, 1100, 80),
            OperatingPoint::new(PerformanceLevel::High, 240, 1200, 150),
            OperatingPoint::new(PerformanceLevel::Maximum, 480, 1350, 350),
        ];

        Self {
            operating_points,
            current_level: PerformanceLevel::Balanced,
            min_level: PerformanceLevel::Minimum,
            max_level: PerformanceLevel::Maximum,
            auto_scaling: true,
            scale_up_threshold: 80,
            scale_down_threshold: 20,
            transitions: 0,
        }
    }

    /// Create with custom operating points
    pub fn with_operating_points(points: Vec<OperatingPoint>) -> Self {
        let current_level = points
            .first()
            .map(|p| p.level)
            .unwrap_or(PerformanceLevel::Balanced);
        Self {
            operating_points: points,
            current_level,
            min_level: PerformanceLevel::Minimum,
            max_level: PerformanceLevel::Maximum,
            auto_scaling: true,
            scale_up_threshold: 80,
            scale_down_threshold: 20,
            transitions: 0,
        }
    }

    /// Get current operating point
    pub fn current_point(&self) -> Option<&OperatingPoint> {
        self.operating_points
            .iter()
            .find(|p| p.level == self.current_level)
    }

    /// Get current performance level
    pub fn current_level(&self) -> PerformanceLevel {
        self.current_level
    }

    /// Set performance level
    pub fn set_level(&mut self, level: PerformanceLevel) -> Result<(), DvfsError> {
        if level < self.min_level {
            return Err(DvfsError::BelowMinimum);
        }
        if level > self.max_level {
            return Err(DvfsError::AboveMaximum);
        }
        if !self.operating_points.iter().any(|p| p.level == level) {
            return Err(DvfsError::UnsupportedLevel);
        }
        if self.current_level != level {
            self.current_level = level;
            self.transitions += 1;
        }
        Ok(())
    }

    /// Scale up to next performance level
    pub fn scale_up(&mut self) -> Result<bool, DvfsError> {
        if let Some(higher) = self.current_level.higher() {
            if higher <= self.max_level {
                self.set_level(higher)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Scale down to next performance level
    pub fn scale_down(&mut self) -> Result<bool, DvfsError> {
        if let Some(lower) = self.current_level.lower() {
            if lower >= self.min_level {
                self.set_level(lower)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Update based on CPU utilization (percentage)
    pub fn update_utilization(&mut self, utilization: u8) -> Result<bool, DvfsError> {
        if !self.auto_scaling {
            return Ok(false);
        }

        if utilization >= self.scale_up_threshold {
            self.scale_up()
        } else if utilization <= self.scale_down_threshold {
            self.scale_down()
        } else {
            Ok(false)
        }
    }

    /// Set minimum performance level
    pub fn set_min_level(&mut self, level: PerformanceLevel) {
        self.min_level = level;
        if self.current_level < level {
            let _ = self.set_level(level);
        }
    }

    /// Set maximum performance level
    pub fn set_max_level(&mut self, level: PerformanceLevel) {
        self.max_level = level;
        if self.current_level > level {
            let _ = self.set_level(level);
        }
    }

    /// Enable/disable auto-scaling
    pub fn set_auto_scaling(&mut self, enabled: bool) {
        self.auto_scaling = enabled;
    }

    /// Get transition count
    pub fn transitions(&self) -> u64 {
        self.transitions
    }

    /// Get all operating points
    pub fn operating_points(&self) -> &[OperatingPoint] {
        &self.operating_points
    }

    /// Find most efficient operating point for a target frequency
    pub fn most_efficient_for_frequency(&self, min_freq_mhz: u32) -> Option<&OperatingPoint> {
        self.operating_points
            .iter()
            .filter(|p| {
                p.frequency_mhz >= min_freq_mhz
                    && p.level >= self.min_level
                    && p.level <= self.max_level
            })
            .max_by(|a, b| {
                a.efficiency()
                    .partial_cmp(&b.efficiency())
                    .unwrap_or(core::cmp::Ordering::Equal)
            })
    }
}

impl Default for DvfsController {
    fn default() -> Self {
        Self::new()
    }
}

/// DVFS errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DvfsError {
    /// Requested level below minimum
    BelowMinimum,
    /// Requested level above maximum
    AboveMaximum,
    /// Unsupported performance level
    UnsupportedLevel,
    /// Hardware error during transition
    TransitionFailed,
}

// ============================================================================
// Peripheral Power Gating
// ============================================================================

/// Peripheral identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Peripheral {
    /// UART/Serial
    Uart(u8),
    /// SPI bus
    Spi(u8),
    /// I2C bus
    I2c(u8),
    /// USB controller
    Usb,
    /// GPIO port
    Gpio(u8),
    /// Timer
    Timer(u8),
    /// ADC
    Adc(u8),
    /// DAC
    Dac(u8),
    /// PWM
    Pwm(u8),
    /// DMA controller
    Dma,
    /// Ethernet MAC
    Ethernet,
    /// CAN bus
    Can(u8),
    /// SDIO/SD card
    Sdio,
    /// Crypto accelerator
    Crypto,
    /// RNG
    Rng,
    /// Display controller
    Display,
    /// Camera interface
    Camera,
    /// Audio codec
    Audio,
}

impl Peripheral {
    /// Get typical power consumption in microwatts
    pub fn typical_power_uw(&self) -> u32 {
        match self {
            Peripheral::Uart(_) => 100,
            Peripheral::Spi(_) => 200,
            Peripheral::I2c(_) => 150,
            Peripheral::Usb => 5000,
            Peripheral::Gpio(_) => 50,
            Peripheral::Timer(_) => 20,
            Peripheral::Adc(_) => 500,
            Peripheral::Dac(_) => 300,
            Peripheral::Pwm(_) => 100,
            Peripheral::Dma => 1000,
            Peripheral::Ethernet => 10000,
            Peripheral::Can(_) => 500,
            Peripheral::Sdio => 2000,
            Peripheral::Crypto => 5000,
            Peripheral::Rng => 200,
            Peripheral::Display => 50000,
            Peripheral::Camera => 30000,
            Peripheral::Audio => 10000,
        }
    }

    /// Get name for logging
    pub fn name(&self) -> String {
        match self {
            Peripheral::Uart(n) => alloc::format!("UART{}", n),
            Peripheral::Spi(n) => alloc::format!("SPI{}", n),
            Peripheral::I2c(n) => alloc::format!("I2C{}", n),
            Peripheral::Usb => String::from("USB"),
            Peripheral::Gpio(n) => alloc::format!("GPIO{}", n),
            Peripheral::Timer(n) => alloc::format!("TIM{}", n),
            Peripheral::Adc(n) => alloc::format!("ADC{}", n),
            Peripheral::Dac(n) => alloc::format!("DAC{}", n),
            Peripheral::Pwm(n) => alloc::format!("PWM{}", n),
            Peripheral::Dma => String::from("DMA"),
            Peripheral::Ethernet => String::from("ETH"),
            Peripheral::Can(n) => alloc::format!("CAN{}", n),
            Peripheral::Sdio => String::from("SDIO"),
            Peripheral::Crypto => String::from("CRYPTO"),
            Peripheral::Rng => String::from("RNG"),
            Peripheral::Display => String::from("DISPLAY"),
            Peripheral::Camera => String::from("CAM"),
            Peripheral::Audio => String::from("AUDIO"),
        }
    }
}

/// Power gating state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatingState {
    /// Peripheral is powered on
    PoweredOn,
    /// Peripheral is powered off (gated)
    PoweredOff,
    /// Peripheral is in retention mode (low power, state preserved)
    Retention,
}

/// Peripheral power gating controller
#[derive(Debug)]
pub struct PowerGating {
    /// State of each peripheral
    states: BTreeMap<u32, GatingState>,
    /// Reference count for each peripheral
    ref_counts: BTreeMap<u32, u32>,
    /// Total estimated power saved in microwatts
    power_saved_uw: u64,
}

impl PowerGating {
    /// Create a new power gating controller
    pub fn new() -> Self {
        Self {
            states: BTreeMap::new(),
            ref_counts: BTreeMap::new(),
            power_saved_uw: 0,
        }
    }

    fn peripheral_key(p: &Peripheral) -> u32 {
        match p {
            Peripheral::Uart(n) => *n as u32,
            Peripheral::Spi(n) => 0x100 | (*n as u32),
            Peripheral::I2c(n) => 0x200 | (*n as u32),
            Peripheral::Usb => 0x300,
            Peripheral::Gpio(n) => 0x400 | (*n as u32),
            Peripheral::Timer(n) => 0x500 | (*n as u32),
            Peripheral::Adc(n) => 0x600 | (*n as u32),
            Peripheral::Dac(n) => 0x700 | (*n as u32),
            Peripheral::Pwm(n) => 0x800 | (*n as u32),
            Peripheral::Dma => 0x900,
            Peripheral::Ethernet => 0xA00,
            Peripheral::Can(n) => 0xB00 | (*n as u32),
            Peripheral::Sdio => 0xC00,
            Peripheral::Crypto => 0xD00,
            Peripheral::Rng => 0xE00,
            Peripheral::Display => 0xF00,
            Peripheral::Camera => 0x1000,
            Peripheral::Audio => 0x1100,
        }
    }

    /// Acquire a peripheral (power on if needed, increment ref count)
    pub fn acquire(&mut self, peripheral: &Peripheral) -> GatingState {
        let key = Self::peripheral_key(peripheral);
        let count = self.ref_counts.entry(key).or_insert(0);
        *count += 1;

        let state = self.states.entry(key).or_insert(GatingState::PoweredOff);
        if *state != GatingState::PoweredOn {
            *state = GatingState::PoweredOn;
        }
        *state
    }

    /// Release a peripheral (decrement ref count, power off if zero)
    pub fn release(&mut self, peripheral: &Peripheral) -> GatingState {
        let key = Self::peripheral_key(peripheral);

        if let Some(count) = self.ref_counts.get_mut(&key) {
            if *count > 0 {
                *count -= 1;
            }
            if *count == 0 {
                if let Some(state) = self.states.get_mut(&key) {
                    *state = GatingState::PoweredOff;
                    self.power_saved_uw += peripheral.typical_power_uw() as u64;
                }
            }
        }

        self.get_state(peripheral)
    }

    /// Get current state of a peripheral
    pub fn get_state(&self, peripheral: &Peripheral) -> GatingState {
        let key = Self::peripheral_key(peripheral);
        self.states
            .get(&key)
            .copied()
            .unwrap_or(GatingState::PoweredOff)
    }

    /// Force power off a peripheral (ignoring ref count)
    pub fn force_off(&mut self, peripheral: &Peripheral) {
        let key = Self::peripheral_key(peripheral);
        self.states.insert(key, GatingState::PoweredOff);
        self.ref_counts.insert(key, 0);
        self.power_saved_uw += peripheral.typical_power_uw() as u64;
    }

    /// Set peripheral to retention mode
    pub fn set_retention(&mut self, peripheral: &Peripheral) {
        let key = Self::peripheral_key(peripheral);
        self.states.insert(key, GatingState::Retention);
    }

    /// Get estimated total power savings in microwatts
    pub fn power_saved(&self) -> u64 {
        self.power_saved_uw
    }

    /// Get current power consumption from active peripherals in microwatts
    pub fn active_power_uw(&self) -> u64 {
        // Would need to track which peripherals are active and sum their power
        0
    }

    /// Get number of powered-on peripherals
    pub fn active_count(&self) -> usize {
        self.states
            .values()
            .filter(|s| **s == GatingState::PoweredOn)
            .count()
    }

    /// Power off all peripherals
    pub fn power_off_all(&mut self) {
        for state in self.states.values_mut() {
            if *state == GatingState::PoweredOn {
                *state = GatingState::PoweredOff;
            }
        }
        self.ref_counts.clear();
    }
}

impl Default for PowerGating {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Clock Gating
// ============================================================================

/// Clock domain identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClockDomain {
    /// CPU core clock
    CpuCore,
    /// System bus clock (AHB)
    SystemBus,
    /// Peripheral bus 1 (APB1)
    PeripheralBus1,
    /// Peripheral bus 2 (APB2)
    PeripheralBus2,
    /// High-speed peripheral bus
    HighSpeedBus,
    /// Flash memory clock
    Flash,
    /// External memory clock
    ExternalMemory,
    /// USB clock
    Usb,
    /// Ethernet clock
    Ethernet,
    /// RTC clock
    Rtc,
    /// Watchdog clock
    Watchdog,
    /// PLL clock
    Pll,
}

impl ClockDomain {
    /// Check if this clock is essential (cannot be gated)
    pub fn is_essential(&self) -> bool {
        matches!(
            self,
            ClockDomain::CpuCore | ClockDomain::SystemBus | ClockDomain::Flash
        )
    }

    /// Get typical frequency in MHz
    pub fn typical_freq_mhz(&self) -> u32 {
        match self {
            ClockDomain::CpuCore => 168,
            ClockDomain::SystemBus => 168,
            ClockDomain::PeripheralBus1 => 42,
            ClockDomain::PeripheralBus2 => 84,
            ClockDomain::HighSpeedBus => 168,
            ClockDomain::Flash => 168,
            ClockDomain::ExternalMemory => 100,
            ClockDomain::Usb => 48,
            ClockDomain::Ethernet => 50,
            ClockDomain::Rtc => 0,      // 32.768 kHz
            ClockDomain::Watchdog => 0, // LSI ~32 kHz
            ClockDomain::Pll => 336,
        }
    }
}

/// Clock gating state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockState {
    /// Clock is running at full speed
    Running,
    /// Clock is running at reduced frequency
    Reduced,
    /// Clock is gated (stopped)
    Gated,
}

/// Clock gating controller
#[derive(Debug)]
pub struct ClockGating {
    /// State of each clock domain
    states: BTreeMap<u8, ClockState>,
    /// Current frequency of each domain
    frequencies: BTreeMap<u8, u32>,
}

impl ClockGating {
    /// Create a new clock gating controller
    pub fn new() -> Self {
        Self {
            states: BTreeMap::new(),
            frequencies: BTreeMap::new(),
        }
    }

    fn domain_key(domain: ClockDomain) -> u8 {
        match domain {
            ClockDomain::CpuCore => 0,
            ClockDomain::SystemBus => 1,
            ClockDomain::PeripheralBus1 => 2,
            ClockDomain::PeripheralBus2 => 3,
            ClockDomain::HighSpeedBus => 4,
            ClockDomain::Flash => 5,
            ClockDomain::ExternalMemory => 6,
            ClockDomain::Usb => 7,
            ClockDomain::Ethernet => 8,
            ClockDomain::Rtc => 9,
            ClockDomain::Watchdog => 10,
            ClockDomain::Pll => 11,
        }
    }

    /// Enable a clock domain
    pub fn enable(&mut self, domain: ClockDomain) -> Result<(), ClockError> {
        let key = Self::domain_key(domain);
        self.states.insert(key, ClockState::Running);
        self.frequencies.insert(key, domain.typical_freq_mhz());
        Ok(())
    }

    /// Gate (disable) a clock domain
    pub fn gate(&mut self, domain: ClockDomain) -> Result<(), ClockError> {
        if domain.is_essential() {
            return Err(ClockError::EssentialClock);
        }
        let key = Self::domain_key(domain);
        self.states.insert(key, ClockState::Gated);
        self.frequencies.insert(key, 0);
        Ok(())
    }

    /// Reduce clock frequency
    pub fn reduce(&mut self, domain: ClockDomain, freq_mhz: u32) -> Result<(), ClockError> {
        let key = Self::domain_key(domain);
        let typical = domain.typical_freq_mhz();
        if freq_mhz > typical {
            return Err(ClockError::FrequencyTooHigh);
        }
        self.states.insert(key, ClockState::Reduced);
        self.frequencies.insert(key, freq_mhz);
        Ok(())
    }

    /// Get clock state
    pub fn get_state(&self, domain: ClockDomain) -> ClockState {
        let key = Self::domain_key(domain);
        self.states
            .get(&key)
            .copied()
            .unwrap_or(ClockState::Running)
    }

    /// Get current frequency
    pub fn get_frequency(&self, domain: ClockDomain) -> u32 {
        let key = Self::domain_key(domain);
        self.frequencies
            .get(&key)
            .copied()
            .unwrap_or(domain.typical_freq_mhz())
    }

    /// Gate all non-essential clocks
    pub fn gate_non_essential(&mut self) {
        for domain in [
            ClockDomain::PeripheralBus1,
            ClockDomain::PeripheralBus2,
            ClockDomain::HighSpeedBus,
            ClockDomain::ExternalMemory,
            ClockDomain::Usb,
            ClockDomain::Ethernet,
            ClockDomain::Pll,
        ] {
            let _ = self.gate(domain);
        }
    }

    /// Enable all clocks
    pub fn enable_all(&mut self) {
        for domain in [
            ClockDomain::CpuCore,
            ClockDomain::SystemBus,
            ClockDomain::PeripheralBus1,
            ClockDomain::PeripheralBus2,
            ClockDomain::HighSpeedBus,
            ClockDomain::Flash,
            ClockDomain::ExternalMemory,
            ClockDomain::Usb,
            ClockDomain::Ethernet,
            ClockDomain::Rtc,
            ClockDomain::Watchdog,
            ClockDomain::Pll,
        ] {
            let _ = self.enable(domain);
        }
    }

    /// Count gated clocks
    pub fn gated_count(&self) -> usize {
        self.states
            .values()
            .filter(|s| **s == ClockState::Gated)
            .count()
    }
}

impl Default for ClockGating {
    fn default() -> Self {
        Self::new()
    }
}

/// Clock gating errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    /// Cannot gate essential clock
    EssentialClock,
    /// Frequency too high
    FrequencyTooHigh,
    /// Clock not available
    NotAvailable,
}

// ============================================================================
// Wake-up Sources
// ============================================================================

/// Wake-up source type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeSource {
    /// External interrupt pin
    ExternalInterrupt(u8),
    /// GPIO pin
    Gpio { port: u8, pin: u8 },
    /// RTC alarm
    RtcAlarm,
    /// RTC wakeup timer
    RtcWakeup,
    /// Watchdog timer
    Watchdog,
    /// USB activity
    Usb,
    /// UART activity
    Uart(u8),
    /// I2C address match
    I2c(u8),
    /// CAN message
    Can(u8),
    /// Ethernet wake-on-LAN
    Ethernet,
    /// Comparator output
    Comparator(u8),
    /// Low-power timer
    LpTimer,
    /// Touch sensor
    Touch,
}

impl WakeSource {
    /// Get wake-up latency in microseconds
    pub fn wake_latency_us(&self) -> u32 {
        match self {
            WakeSource::ExternalInterrupt(_) => 5,
            WakeSource::Gpio { .. } => 5,
            WakeSource::RtcAlarm => 100,
            WakeSource::RtcWakeup => 100,
            WakeSource::Watchdog => 50,
            WakeSource::Usb => 1000,
            WakeSource::Uart(_) => 500,
            WakeSource::I2c(_) => 500,
            WakeSource::Can(_) => 500,
            WakeSource::Ethernet => 2000,
            WakeSource::Comparator(_) => 10,
            WakeSource::LpTimer => 50,
            WakeSource::Touch => 100,
        }
    }

    /// Check if source is available in standby mode
    pub fn available_in_standby(&self) -> bool {
        matches!(
            self,
            WakeSource::ExternalInterrupt(_)
                | WakeSource::Gpio { .. }
                | WakeSource::RtcAlarm
                | WakeSource::RtcWakeup
                | WakeSource::Watchdog
                | WakeSource::LpTimer
        )
    }
}

/// Wake-up edge trigger
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeEdge {
    /// Rising edge
    Rising,
    /// Falling edge
    Falling,
    /// Both edges
    Both,
    /// Level high
    High,
    /// Level low
    Low,
}

/// Wake-up source configuration
#[derive(Debug, Clone)]
pub struct WakeConfig {
    /// Wake source
    pub source: WakeSource,
    /// Edge/level trigger
    pub edge: WakeEdge,
    /// Enabled
    pub enabled: bool,
    /// Debounce time in microseconds
    pub debounce_us: u32,
}

impl WakeConfig {
    /// Create a new wake configuration
    pub fn new(source: WakeSource) -> Self {
        Self {
            source,
            edge: WakeEdge::Rising,
            enabled: true,
            debounce_us: 0,
        }
    }

    /// Set edge trigger
    pub fn with_edge(mut self, edge: WakeEdge) -> Self {
        self.edge = edge;
        self
    }

    /// Set debounce time
    pub fn with_debounce(mut self, us: u32) -> Self {
        self.debounce_us = us;
        self
    }

    /// Disable the source
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Wake-up controller
#[derive(Debug, Default)]
pub struct WakeController {
    /// Configured wake sources
    sources: Vec<WakeConfig>,
    /// Last wake source
    last_wake_source: Option<WakeSource>,
    /// Wake count
    wake_count: u64,
}

impl WakeController {
    /// Create a new wake controller
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a wake source
    pub fn add_source(&mut self, config: WakeConfig) {
        self.sources.push(config);
    }

    /// Remove a wake source
    pub fn remove_source(&mut self, source: &WakeSource) {
        self.sources.retain(|c| &c.source != source);
    }

    /// Enable a wake source
    pub fn enable(&mut self, source: &WakeSource) {
        if let Some(config) = self.sources.iter_mut().find(|c| &c.source == source) {
            config.enabled = true;
        }
    }

    /// Disable a wake source
    pub fn disable(&mut self, source: &WakeSource) {
        if let Some(config) = self.sources.iter_mut().find(|c| &c.source == source) {
            config.enabled = false;
        }
    }

    /// Get all enabled sources
    pub fn enabled_sources(&self) -> impl Iterator<Item = &WakeConfig> {
        self.sources.iter().filter(|c| c.enabled)
    }

    /// Get sources available in standby mode
    pub fn standby_sources(&self) -> impl Iterator<Item = &WakeConfig> {
        self.sources
            .iter()
            .filter(|c| c.enabled && c.source.available_in_standby())
    }

    /// Record a wake event
    pub fn record_wake(&mut self, source: WakeSource) {
        self.last_wake_source = Some(source);
        self.wake_count += 1;
    }

    /// Get last wake source
    pub fn last_wake_source(&self) -> Option<WakeSource> {
        self.last_wake_source
    }

    /// Get wake count
    pub fn wake_count(&self) -> u64 {
        self.wake_count
    }

    /// Get minimum wake latency across enabled sources
    pub fn min_wake_latency_us(&self) -> u32 {
        self.sources
            .iter()
            .filter(|c| c.enabled)
            .map(|c| c.source.wake_latency_us())
            .min()
            .unwrap_or(0)
    }

    /// Clear all sources
    pub fn clear(&mut self) {
        self.sources.clear();
    }
}

// ============================================================================
// Unified Power Manager
// ============================================================================

/// Advanced power manager combining all power features
#[derive(Debug)]
pub struct AdvancedPowerManager {
    /// Current power mode
    mode: PowerMode,
    /// DVFS controller
    dvfs: DvfsController,
    /// Peripheral power gating
    power_gating: PowerGating,
    /// Clock gating
    clock_gating: ClockGating,
    /// Wake-up controller
    wake_controller: WakeController,
    /// Mode transitions
    transitions: u64,
}

impl AdvancedPowerManager {
    /// Create a new advanced power manager
    pub fn new() -> Self {
        Self {
            mode: PowerMode::Normal,
            dvfs: DvfsController::new(),
            power_gating: PowerGating::new(),
            clock_gating: ClockGating::new(),
            wake_controller: WakeController::new(),
            transitions: 0,
        }
    }

    /// Get current power mode
    pub fn mode(&self) -> PowerMode {
        self.mode
    }

    /// Validate power mode transition
    fn validate_transition(&self, from: PowerMode, to: PowerMode) -> Result<(), PowerError> {
        // Check for invalid transitions
        match (from, to) {
            // Cannot transition from shutdown without reset
            (PowerMode::Shutdown, _) => {
                return Err(PowerError::InvalidTransition { from, to });
            }
            // Cannot transition directly to shutdown from sleep/standby
            // (should go through normal or low power first)
            (PowerMode::Sleep | PowerMode::Standby, PowerMode::Shutdown) => {
                return Err(PowerError::TransitionBlocked(
                    "Must wake up before shutdown",
                ));
            }
            // All other transitions are allowed
            _ => {}
        }

        // Validate wake sources for sleep modes
        match to {
            PowerMode::Sleep => {
                if self.wake_controller.enabled_sources().count() == 0 {
                    return Err(PowerError::NoWakeSource);
                }
            }
            PowerMode::Standby => {
                if self.wake_controller.standby_sources().count() == 0 {
                    return Err(PowerError::NoStandbyWakeSource);
                }
            }
            PowerMode::Shutdown => {
                // Verify no critical peripherals are active
                if self.power_gating.active_count() > 0 {
                    return Err(PowerError::TransitionBlocked(
                        "Active peripherals must be shut down first",
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Set power mode with comprehensive validation
    pub fn set_mode(&mut self, mode: PowerMode) -> Result<(), PowerError> {
        if self.mode == mode {
            return Ok(());
        }

        // Validate transition
        self.validate_transition(self.mode, mode)?;

        // Store old mode for rollback on error
        let old_mode = self.mode;

        // Configure subsystems for new mode
        let result = match mode {
            PowerMode::Normal => {
                self.dvfs.set_level(PerformanceLevel::Balanced)?;
                self.clock_gating.enable_all();
                Ok(())
            }
            PowerMode::LowPower => {
                self.dvfs.set_level(PerformanceLevel::Low)?;
                Ok(())
            }
            PowerMode::UltraLowPower => {
                self.dvfs.set_level(PerformanceLevel::Minimum)?;
                self.clock_gating.gate_non_essential();
                Ok(())
            }
            PowerMode::Sleep | PowerMode::Standby => {
                self.dvfs.set_level(PerformanceLevel::Minimum)?;
                self.clock_gating.gate_non_essential();
                self.power_gating.power_off_all();
                Ok(())
            }
            PowerMode::Shutdown => {
                // Ensure all peripherals are off
                self.power_gating.power_off_all();
                self.clock_gating.gate_non_essential();
                // Full shutdown - no recovery without reset
                Ok(())
            }
        };

        match result {
            Ok(()) => {
                self.mode = mode;
                self.transitions += 1;
                Ok(())
            }
            Err(e) => {
                // Rollback on error - attempt to restore old mode
                self.mode = old_mode;
                Err(e)
            }
        }
    }

    /// Safely transition through intermediate states
    pub fn safe_transition_to(&mut self, target: PowerMode) -> Result<(), PowerError> {
        // Define safe transition paths
        let path = match (self.mode, target) {
            // Already at target
            (current, target) if current == target => return Ok(()),

            // Waking up from sleep modes
            (PowerMode::Sleep | PowerMode::Standby, PowerMode::Normal) => {
                vec![PowerMode::LowPower, PowerMode::Normal]
            }
            (PowerMode::Sleep | PowerMode::Standby, PowerMode::LowPower) => {
                vec![PowerMode::LowPower]
            }

            // Going to sleep from active states
            (PowerMode::Normal | PowerMode::LowPower, PowerMode::Standby) => {
                vec![PowerMode::UltraLowPower, PowerMode::Standby]
            }

            // Shutdown requires going through low power states
            (_, PowerMode::Shutdown) if !matches!(self.mode, PowerMode::UltraLowPower) => {
                vec![PowerMode::UltraLowPower, PowerMode::Shutdown]
            }

            // Direct transition is safe
            _ => vec![target],
        };

        // Execute transitions in sequence
        for mode in path {
            self.set_mode(mode)?;
        }

        Ok(())
    }

    /// Get DVFS controller
    pub fn dvfs(&self) -> &DvfsController {
        &self.dvfs
    }

    /// Get mutable DVFS controller
    pub fn dvfs_mut(&mut self) -> &mut DvfsController {
        &mut self.dvfs
    }

    /// Get power gating controller
    pub fn power_gating(&self) -> &PowerGating {
        &self.power_gating
    }

    /// Get mutable power gating controller
    pub fn power_gating_mut(&mut self) -> &mut PowerGating {
        &mut self.power_gating
    }

    /// Get clock gating controller
    pub fn clock_gating(&self) -> &ClockGating {
        &self.clock_gating
    }

    /// Get mutable clock gating controller
    pub fn clock_gating_mut(&mut self) -> &mut ClockGating {
        &mut self.clock_gating
    }

    /// Get wake controller
    pub fn wake_controller(&self) -> &WakeController {
        &self.wake_controller
    }

    /// Get mutable wake controller
    pub fn wake_controller_mut(&mut self) -> &mut WakeController {
        &mut self.wake_controller
    }

    /// Get transition count
    pub fn transitions(&self) -> u64 {
        self.transitions
    }

    /// Enter sleep mode with configured wake sources
    pub fn enter_sleep(&mut self) -> Result<(), PowerError> {
        if self.wake_controller.enabled_sources().count() == 0 {
            return Err(PowerError::NoWakeSource);
        }
        self.set_mode(PowerMode::Sleep)
    }

    /// Enter standby mode (wake sources must support standby)
    pub fn enter_standby(&mut self) -> Result<(), PowerError> {
        if self.wake_controller.standby_sources().count() == 0 {
            return Err(PowerError::NoStandbyWakeSource);
        }
        self.set_mode(PowerMode::Standby)
    }

    /// Get power summary
    pub fn summary(&self) -> PowerSummary {
        PowerSummary {
            mode: self.mode,
            performance_level: self.dvfs.current_level(),
            frequency_mhz: self
                .dvfs
                .current_point()
                .map(|p| p.frequency_mhz)
                .unwrap_or(0),
            active_peripherals: self.power_gating.active_count(),
            gated_clocks: self.clock_gating.gated_count(),
            wake_sources: self.wake_controller.enabled_sources().count(),
            transitions: self.transitions,
        }
    }
}

impl Default for AdvancedPowerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Power manager errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerError {
    /// DVFS error
    Dvfs(DvfsError),
    /// No wake source configured
    NoWakeSource,
    /// No standby-compatible wake source
    NoStandbyWakeSource,
    /// Invalid mode transition
    InvalidTransition { from: PowerMode, to: PowerMode },
    /// Transition not allowed due to system state
    TransitionBlocked(&'static str),
    /// Hardware error during transition
    HardwareError,
    /// Timeout waiting for power state change
    TransitionTimeout,
    /// Peripheral cannot be powered down in current state
    PeripheralBusy,
}

impl From<DvfsError> for PowerError {
    fn from(e: DvfsError) -> Self {
        PowerError::Dvfs(e)
    }
}

impl core::fmt::Display for PowerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PowerError::Dvfs(e) => write!(f, "DVFS error: {:?}", e),
            PowerError::NoWakeSource => write!(f, "No wake source configured"),
            PowerError::NoStandbyWakeSource => write!(f, "No standby-compatible wake source"),
            PowerError::InvalidTransition { from, to } => {
                write!(f, "Invalid transition from {:?} to {:?}", from, to)
            }
            PowerError::TransitionBlocked(reason) => write!(f, "Transition blocked: {}", reason),
            PowerError::HardwareError => write!(f, "Hardware error during power mode transition"),
            PowerError::TransitionTimeout => write!(f, "Timeout during power mode transition"),
            PowerError::PeripheralBusy => write!(f, "Peripheral busy, cannot change power state"),
        }
    }
}

/// Power manager summary
#[derive(Debug, Clone)]
pub struct PowerSummary {
    /// Current power mode
    pub mode: PowerMode,
    /// Current performance level
    pub performance_level: PerformanceLevel,
    /// Current frequency in MHz
    pub frequency_mhz: u32,
    /// Number of active peripherals
    pub active_peripherals: usize,
    /// Number of gated clocks
    pub gated_clocks: usize,
    /// Number of enabled wake sources
    pub wake_sources: usize,
    /// Mode transitions
    pub transitions: u64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_battery_migration_logic() {
        let low_battery = BatteryStatus {
            level_percent: 15,
            is_charging: false,
        };
        assert!(low_battery.should_migrate());

        let charging = BatteryStatus {
            level_percent: 15,
            is_charging: true,
        };
        assert!(!charging.should_migrate());
    }

    #[test]
    fn test_power_mode_properties() {
        assert_eq!(PowerMode::Normal.power_consumption(), 100);
        assert_eq!(PowerMode::Sleep.power_consumption(), 5);
        assert!(PowerMode::Normal.retains_ram());
        assert!(!PowerMode::Shutdown.retains_ram());
    }

    #[test]
    fn test_power_mode_wake_latency() {
        assert_eq!(PowerMode::Normal.wake_latency_us(), 0);
        assert!(PowerMode::Sleep.wake_latency_us() > PowerMode::LowPower.wake_latency_us());
    }

    #[test]
    fn test_performance_level_ordering() {
        assert!(PerformanceLevel::Maximum > PerformanceLevel::Minimum);
        assert!(PerformanceLevel::Balanced > PerformanceLevel::Low);
    }

    #[test]
    fn test_performance_level_navigation() {
        assert_eq!(
            PerformanceLevel::Balanced.higher(),
            Some(PerformanceLevel::High)
        );
        assert_eq!(
            PerformanceLevel::Balanced.lower(),
            Some(PerformanceLevel::Low)
        );
        assert_eq!(PerformanceLevel::Maximum.higher(), None);
        assert_eq!(PerformanceLevel::Minimum.lower(), None);
    }

    #[test]
    fn test_operating_point_efficiency() {
        let point = OperatingPoint::new(PerformanceLevel::Balanced, 168, 1100, 80);
        assert!(point.efficiency() > 2.0);
    }

    #[test]
    fn test_dvfs_controller_new() {
        let dvfs = DvfsController::new();
        assert_eq!(dvfs.current_level(), PerformanceLevel::Balanced);
        assert!(!dvfs.operating_points().is_empty());
    }

    #[test]
    fn test_dvfs_set_level() {
        let mut dvfs = DvfsController::new();
        assert!(dvfs.set_level(PerformanceLevel::High).is_ok());
        assert_eq!(dvfs.current_level(), PerformanceLevel::High);
        assert_eq!(dvfs.transitions(), 1);
    }

    #[test]
    fn test_dvfs_scale_up_down() {
        let mut dvfs = DvfsController::new();
        dvfs.set_level(PerformanceLevel::Balanced).unwrap();

        assert!(dvfs.scale_up().unwrap());
        assert_eq!(dvfs.current_level(), PerformanceLevel::High);

        assert!(dvfs.scale_down().unwrap());
        assert_eq!(dvfs.current_level(), PerformanceLevel::Balanced);
    }

    #[test]
    fn test_dvfs_limits() {
        let mut dvfs = DvfsController::new();
        dvfs.set_min_level(PerformanceLevel::Low);
        dvfs.set_max_level(PerformanceLevel::High);

        assert!(dvfs.set_level(PerformanceLevel::Minimum).is_err());
        assert!(dvfs.set_level(PerformanceLevel::Maximum).is_err());
    }

    #[test]
    fn test_dvfs_auto_scaling() {
        let mut dvfs = DvfsController::new();
        dvfs.set_level(PerformanceLevel::Balanced).unwrap();

        // High utilization should scale up
        assert!(dvfs.update_utilization(90).unwrap());
        assert_eq!(dvfs.current_level(), PerformanceLevel::High);

        // Low utilization should scale down
        assert!(dvfs.update_utilization(10).unwrap());
        assert_eq!(dvfs.current_level(), PerformanceLevel::Balanced);
    }

    #[test]
    fn test_peripheral_power() {
        assert!(Peripheral::Display.typical_power_uw() > Peripheral::Timer(0).typical_power_uw());
    }

    #[test]
    fn test_peripheral_name() {
        assert_eq!(Peripheral::Uart(0).name(), "UART0");
        assert_eq!(Peripheral::Usb.name(), "USB");
    }

    #[test]
    fn test_power_gating_acquire_release() {
        let mut gating = PowerGating::new();
        let uart = Peripheral::Uart(0);

        assert_eq!(gating.get_state(&uart), GatingState::PoweredOff);

        gating.acquire(&uart);
        assert_eq!(gating.get_state(&uart), GatingState::PoweredOn);

        gating.release(&uart);
        assert_eq!(gating.get_state(&uart), GatingState::PoweredOff);
    }

    #[test]
    fn test_power_gating_ref_count() {
        let mut gating = PowerGating::new();
        let uart = Peripheral::Uart(0);

        gating.acquire(&uart);
        gating.acquire(&uart);
        gating.release(&uart);
        // Still powered on (ref count = 1)
        assert_eq!(gating.get_state(&uart), GatingState::PoweredOn);

        gating.release(&uart);
        // Now powered off (ref count = 0)
        assert_eq!(gating.get_state(&uart), GatingState::PoweredOff);
    }

    #[test]
    fn test_power_gating_force_off() {
        let mut gating = PowerGating::new();
        let uart = Peripheral::Uart(0);

        gating.acquire(&uart);
        gating.acquire(&uart);
        gating.force_off(&uart);

        assert_eq!(gating.get_state(&uart), GatingState::PoweredOff);
    }

    #[test]
    fn test_clock_domain_essential() {
        assert!(ClockDomain::CpuCore.is_essential());
        assert!(!ClockDomain::Usb.is_essential());
    }

    #[test]
    fn test_clock_gating_enable_gate() {
        let mut clocks = ClockGating::new();

        clocks.enable(ClockDomain::Usb).unwrap();
        assert_eq!(clocks.get_state(ClockDomain::Usb), ClockState::Running);

        clocks.gate(ClockDomain::Usb).unwrap();
        assert_eq!(clocks.get_state(ClockDomain::Usb), ClockState::Gated);
    }

    #[test]
    fn test_clock_gating_essential_error() {
        let mut clocks = ClockGating::new();
        assert_eq!(
            clocks.gate(ClockDomain::CpuCore),
            Err(ClockError::EssentialClock)
        );
    }

    #[test]
    fn test_clock_gating_reduce() {
        let mut clocks = ClockGating::new();
        clocks.reduce(ClockDomain::PeripheralBus1, 21).unwrap();
        assert_eq!(
            clocks.get_state(ClockDomain::PeripheralBus1),
            ClockState::Reduced
        );
        assert_eq!(clocks.get_frequency(ClockDomain::PeripheralBus1), 21);
    }

    #[test]
    fn test_wake_source_latency() {
        assert!(
            WakeSource::Gpio { port: 0, pin: 0 }.wake_latency_us()
                < WakeSource::Ethernet.wake_latency_us()
        );
    }

    #[test]
    fn test_wake_source_standby() {
        assert!(WakeSource::RtcAlarm.available_in_standby());
        assert!(!WakeSource::Usb.available_in_standby());
    }

    #[test]
    fn test_wake_controller_add_remove() {
        let mut wake = WakeController::new();

        let config = WakeConfig::new(WakeSource::RtcAlarm);
        wake.add_source(config);
        assert_eq!(wake.enabled_sources().count(), 1);

        wake.remove_source(&WakeSource::RtcAlarm);
        assert_eq!(wake.enabled_sources().count(), 0);
    }

    #[test]
    fn test_wake_controller_enable_disable() {
        let mut wake = WakeController::new();
        wake.add_source(WakeConfig::new(WakeSource::RtcAlarm));

        wake.disable(&WakeSource::RtcAlarm);
        assert_eq!(wake.enabled_sources().count(), 0);

        wake.enable(&WakeSource::RtcAlarm);
        assert_eq!(wake.enabled_sources().count(), 1);
    }

    #[test]
    fn test_wake_controller_record() {
        let mut wake = WakeController::new();
        wake.record_wake(WakeSource::RtcAlarm);

        assert_eq!(wake.last_wake_source(), Some(WakeSource::RtcAlarm));
        assert_eq!(wake.wake_count(), 1);
    }

    #[test]
    fn test_advanced_power_manager_new() {
        let pm = AdvancedPowerManager::new();
        assert_eq!(pm.mode(), PowerMode::Normal);
    }

    #[test]
    fn test_advanced_power_manager_set_mode() {
        let mut pm = AdvancedPowerManager::new();

        pm.set_mode(PowerMode::LowPower).unwrap();
        assert_eq!(pm.mode(), PowerMode::LowPower);
        assert_eq!(pm.dvfs().current_level(), PerformanceLevel::Low);
    }

    #[test]
    fn test_advanced_power_manager_ultra_low() {
        let mut pm = AdvancedPowerManager::new();

        pm.set_mode(PowerMode::UltraLowPower).unwrap();
        assert_eq!(pm.mode(), PowerMode::UltraLowPower);
        assert_eq!(pm.dvfs().current_level(), PerformanceLevel::Minimum);
    }

    #[test]
    fn test_advanced_power_manager_sleep_error() {
        let mut pm = AdvancedPowerManager::new();

        // No wake sources configured
        assert_eq!(pm.enter_sleep(), Err(PowerError::NoWakeSource));
    }

    #[test]
    fn test_advanced_power_manager_sleep_ok() {
        let mut pm = AdvancedPowerManager::new();

        pm.wake_controller_mut()
            .add_source(WakeConfig::new(WakeSource::RtcAlarm));
        assert!(pm.enter_sleep().is_ok());
        assert_eq!(pm.mode(), PowerMode::Sleep);
    }

    #[test]
    fn test_advanced_power_manager_standby_error() {
        let mut pm = AdvancedPowerManager::new();

        // USB is not available in standby
        pm.wake_controller_mut()
            .add_source(WakeConfig::new(WakeSource::Usb));
        assert_eq!(pm.enter_standby(), Err(PowerError::NoStandbyWakeSource));
    }

    #[test]
    fn test_advanced_power_manager_summary() {
        let pm = AdvancedPowerManager::new();
        let summary = pm.summary();

        assert_eq!(summary.mode, PowerMode::Normal);
        assert_eq!(summary.performance_level, PerformanceLevel::Balanced);
    }

    #[test]
    fn test_wake_config_builder() {
        let config = WakeConfig::new(WakeSource::Gpio { port: 0, pin: 5 })
            .with_edge(WakeEdge::Falling)
            .with_debounce(100);

        assert_eq!(config.edge, WakeEdge::Falling);
        assert_eq!(config.debounce_us, 100);
        assert!(config.enabled);
    }

    #[test]
    fn test_dvfs_most_efficient() {
        let dvfs = DvfsController::new();
        let efficient = dvfs.most_efficient_for_frequency(100);
        assert!(efficient.is_some());
    }

    #[test]
    fn test_power_error_display() {
        let err = PowerError::NoWakeSource;
        assert!(alloc::format!("{}", err).contains("wake source"));

        let err = PowerError::InvalidTransition {
            from: PowerMode::Shutdown,
            to: PowerMode::Normal,
        };
        assert!(alloc::format!("{}", err).contains("Invalid transition"));
    }

    #[test]
    fn test_invalid_transition_from_shutdown() {
        let mut pm = AdvancedPowerManager::new();
        pm.wake_controller_mut()
            .add_source(WakeConfig::new(WakeSource::RtcAlarm));

        // Force into shutdown state for testing
        pm.mode = PowerMode::Shutdown;

        // Should not be able to transition from shutdown
        let result = pm.set_mode(PowerMode::Normal);
        assert!(matches!(result, Err(PowerError::InvalidTransition { .. })));
    }

    #[test]
    fn test_transition_validation_wake_sources() {
        let mut pm = AdvancedPowerManager::new();

        // Try to sleep without wake sources
        assert!(matches!(
            pm.set_mode(PowerMode::Sleep),
            Err(PowerError::NoWakeSource)
        ));

        // Add wake source and try again
        pm.wake_controller_mut()
            .add_source(WakeConfig::new(WakeSource::RtcAlarm));
        assert!(pm.set_mode(PowerMode::Sleep).is_ok());
    }

    #[test]
    fn test_transition_validation_standby_sources() {
        let mut pm = AdvancedPowerManager::new();

        // Add USB wake source (not available in standby)
        pm.wake_controller_mut()
            .add_source(WakeConfig::new(WakeSource::Usb));

        // Should fail - USB not available in standby
        assert!(matches!(
            pm.set_mode(PowerMode::Standby),
            Err(PowerError::NoStandbyWakeSource)
        ));

        // Add RTC wake source (available in standby)
        pm.wake_controller_mut()
            .add_source(WakeConfig::new(WakeSource::RtcAlarm));

        // Should succeed now
        assert!(pm.set_mode(PowerMode::Standby).is_ok());
    }

    #[test]
    fn test_transition_validation_shutdown_with_active_peripherals() {
        let mut pm = AdvancedPowerManager::new();

        // Acquire a peripheral
        pm.power_gating_mut().acquire(&Peripheral::Uart(0));

        // Should fail to shutdown with active peripherals
        assert!(matches!(
            pm.set_mode(PowerMode::Shutdown),
            Err(PowerError::TransitionBlocked(_))
        ));

        // Release peripheral and try again
        pm.power_gating_mut().release(&Peripheral::Uart(0));
        assert!(pm.set_mode(PowerMode::Shutdown).is_ok());
    }

    #[test]
    fn test_safe_transition_wakeup_path() {
        let mut pm = AdvancedPowerManager::new();

        // Add wake source
        pm.wake_controller_mut()
            .add_source(WakeConfig::new(WakeSource::RtcAlarm));

        // Go to sleep
        pm.set_mode(PowerMode::Sleep).unwrap();

        // Wake up using safe transition
        assert!(pm.safe_transition_to(PowerMode::Normal).is_ok());
        assert_eq!(pm.mode(), PowerMode::Normal);
    }

    #[test]
    fn test_safe_transition_shutdown_path() {
        let mut pm = AdvancedPowerManager::new();

        // Start in normal mode
        assert_eq!(pm.mode(), PowerMode::Normal);

        // Safe transition to shutdown should go through intermediate states
        assert!(pm.safe_transition_to(PowerMode::Shutdown).is_ok());
        assert_eq!(pm.mode(), PowerMode::Shutdown);
    }

    #[test]
    fn test_transition_rollback_on_error() {
        let mut pm = AdvancedPowerManager::new();

        // Start in Normal mode and transition to LowPower
        pm.set_mode(PowerMode::LowPower).unwrap();
        let initial_mode = pm.mode();

        // Set max level below what Normal mode requires (Balanced)
        pm.dvfs_mut().set_max_level(PerformanceLevel::Low);

        // Try to transition back to Normal - should fail due to DVFS constraints
        let result = pm.set_mode(PowerMode::Normal);
        assert!(result.is_err());

        // Mode should be unchanged due to rollback
        assert_eq!(pm.mode(), initial_mode);
    }

    #[test]
    fn test_transition_counter() {
        let mut pm = AdvancedPowerManager::new();
        let initial_transitions = pm.transitions();

        pm.set_mode(PowerMode::LowPower).unwrap();
        assert_eq!(pm.transitions(), initial_transitions + 1);

        pm.set_mode(PowerMode::Normal).unwrap();
        assert_eq!(pm.transitions(), initial_transitions + 2);

        // Same mode shouldn't increment counter
        pm.set_mode(PowerMode::Normal).unwrap();
        assert_eq!(pm.transitions(), initial_transitions + 2);
    }

    #[test]
    fn test_blocked_transition_from_sleep_to_shutdown() {
        let mut pm = AdvancedPowerManager::new();

        // Add wake source and go to sleep
        pm.wake_controller_mut()
            .add_source(WakeConfig::new(WakeSource::RtcAlarm));
        pm.set_mode(PowerMode::Sleep).unwrap();

        // Cannot go directly to shutdown from sleep
        assert!(matches!(
            pm.set_mode(PowerMode::Shutdown),
            Err(PowerError::TransitionBlocked(_))
        ));
    }
}
