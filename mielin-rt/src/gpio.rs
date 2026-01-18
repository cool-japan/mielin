//! GPIO (General Purpose Input/Output) Peripheral Driver
//!
//! Provides a hardware abstraction layer for GPIO operations across different
//! microcontroller families (STM32, nRF52, ESP32, RP2040, etc.).
//!
//! ## Overview
//!
//! The GPIO module provides a safe, type-checked abstraction for GPIO pins
//! with support for:
//! - Input/Output modes
//! - Pull-up/Pull-down resistors
//! - Interrupt-driven input (rising/falling edge, both edges, level)
//! - Alternate functions (SPI, I2C, UART, etc.)
//! - Drive strength configuration
//! - Slew rate control
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::gpio::{GpioPin, PinMode, PullMode, Edge};
//!
//! // Configure pin as output
//! let mut led_pin = GpioPin::new(13);
//! led_pin.set_mode(PinMode::Output);
//! led_pin.set_high();
//!
//! // Configure pin as input with interrupt
//! let mut button_pin = GpioPin::new(14);
//! button_pin.set_mode(PinMode::Input);
//! button_pin.set_pull(PullMode::PullUp);
//! button_pin.enable_interrupt(Edge::Falling);
//! ```

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

// =============================================================================
// Pin Configuration
// =============================================================================

/// GPIO pin mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinMode {
    /// Input mode (high impedance)
    Input,

    /// Output mode (push-pull)
    Output,

    /// Output mode (open-drain)
    OpenDrain,

    /// Alternate function mode
    Alternate(u8),

    /// Analog mode (for ADC/DAC)
    Analog,
}

/// Pull-up/pull-down configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullMode {
    /// No pull resistor (floating)
    None,

    /// Pull-up resistor enabled
    PullUp,

    /// Pull-down resistor enabled
    PullDown,
}

/// Output drive strength
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveStrength {
    /// Low drive strength (2mA typical)
    Low,

    /// Medium drive strength (4mA typical)
    Medium,

    /// High drive strength (8mA typical)
    High,

    /// Maximum drive strength (12mA+ typical)
    Maximum,
}

/// Output slew rate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlewRate {
    /// Slow slew rate (reduces EMI)
    Slow,

    /// Fast slew rate (faster transitions)
    Fast,
}

/// Interrupt edge detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// Rising edge (low to high)
    Rising,

    /// Falling edge (high to low)
    Falling,

    /// Both edges
    Both,

    /// Level high
    LevelHigh,

    /// Level low
    LevelLow,
}

/// Pin state (high or low)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinState {
    /// Logic low (0V)
    Low,

    /// Logic high (VDD)
    High,
}

impl From<bool> for PinState {
    fn from(value: bool) -> Self {
        if value {
            PinState::High
        } else {
            PinState::Low
        }
    }
}

impl From<PinState> for bool {
    fn from(state: PinState) -> bool {
        matches!(state, PinState::High)
    }
}

// =============================================================================
// GPIO Pin
// =============================================================================

/// GPIO pin abstraction
pub struct GpioPin {
    /// Pin number (0-63 typically)
    pin: u8,

    /// Current pin mode
    mode: PinMode,

    /// Pull resistor configuration
    pull: PullMode,

    /// Drive strength
    drive: DriveStrength,

    /// Slew rate
    slew: SlewRate,

    /// Interrupt enabled
    interrupt_enabled: AtomicBool,

    /// Interrupt edge/level
    interrupt_edge: AtomicU8,

    /// Current pin state (for outputs)
    state: AtomicBool,
}

impl GpioPin {
    /// Create a new GPIO pin
    pub fn new(pin: u8) -> Self {
        Self {
            pin,
            mode: PinMode::Input,
            pull: PullMode::None,
            drive: DriveStrength::Medium,
            slew: SlewRate::Fast,
            interrupt_enabled: AtomicBool::new(false),
            interrupt_edge: AtomicU8::new(0),
            state: AtomicBool::new(false),
        }
    }

    /// Get pin number
    pub fn pin(&self) -> u8 {
        self.pin
    }

    /// Set pin mode
    pub fn set_mode(&mut self, mode: PinMode) {
        self.mode = mode;
        // In a real implementation, this would configure hardware registers
    }

    /// Get current pin mode
    pub fn mode(&self) -> PinMode {
        self.mode
    }

    /// Set pull resistor configuration
    pub fn set_pull(&mut self, pull: PullMode) {
        self.pull = pull;
        // In a real implementation, this would configure hardware registers
    }

    /// Get pull resistor configuration
    pub fn pull(&self) -> PullMode {
        self.pull
    }

    /// Set drive strength (for output pins)
    pub fn set_drive_strength(&mut self, drive: DriveStrength) {
        self.drive = drive;
        // In a real implementation, this would configure hardware registers
    }

    /// Get drive strength
    pub fn drive_strength(&self) -> DriveStrength {
        self.drive
    }

    /// Set slew rate (for output pins)
    pub fn set_slew_rate(&mut self, slew: SlewRate) {
        self.slew = slew;
        // In a real implementation, this would configure hardware registers
    }

    /// Get slew rate
    pub fn slew_rate(&self) -> SlewRate {
        self.slew
    }

    /// Set pin high (output mode)
    pub fn set_high(&mut self) {
        self.state.store(true, Ordering::Release);
        // In a real implementation, this would set hardware register bit
    }

    /// Set pin low (output mode)
    pub fn set_low(&mut self) {
        self.state.store(false, Ordering::Release);
        // In a real implementation, this would clear hardware register bit
    }

    /// Set pin state
    pub fn set_state(&mut self, state: PinState) {
        match state {
            PinState::High => self.set_high(),
            PinState::Low => self.set_low(),
        }
    }

    /// Toggle pin state (output mode)
    pub fn toggle(&mut self) {
        let current = self.state.load(Ordering::Acquire);
        self.state.store(!current, Ordering::Release);
        // In a real implementation, this would use hardware toggle register
    }

    /// Read pin state (input or output mode)
    pub fn read(&self) -> PinState {
        // In a real implementation, this would read hardware input data register
        if self.state.load(Ordering::Acquire) {
            PinState::High
        } else {
            PinState::Low
        }
    }

    /// Check if pin is high
    pub fn is_high(&self) -> bool {
        self.read() == PinState::High
    }

    /// Check if pin is low
    pub fn is_low(&self) -> bool {
        self.read() == PinState::Low
    }

    /// Enable interrupt on this pin
    pub fn enable_interrupt(&mut self, edge: Edge) {
        self.interrupt_enabled.store(true, Ordering::Release);
        self.interrupt_edge.store(edge as u8, Ordering::Release);
        // In a real implementation, this would configure NVIC and EXTI
    }

    /// Disable interrupt on this pin
    pub fn disable_interrupt(&mut self) {
        self.interrupt_enabled.store(false, Ordering::Release);
        // In a real implementation, this would disable NVIC and EXTI
    }

    /// Check if interrupt is enabled
    pub fn is_interrupt_enabled(&self) -> bool {
        self.interrupt_enabled.load(Ordering::Acquire)
    }

    /// Clear interrupt pending flag
    pub fn clear_interrupt(&mut self) {
        // In a real implementation, this would clear hardware pending register
    }
}

// =============================================================================
// GPIO Port (multiple pins)
// =============================================================================

/// GPIO port (group of pins)
pub struct GpioPort {
    /// Port identifier (A, B, C, etc.)
    port: char,

    /// Pins in this port
    pins: Vec<GpioPin>,
}

impl GpioPort {
    /// Create a new GPIO port
    pub fn new(port: char, pin_count: usize) -> Self {
        let pins = (0..pin_count as u8).map(GpioPin::new).collect();

        Self { port, pins }
    }

    /// Get port identifier
    pub fn port(&self) -> char {
        self.port
    }

    /// Get a pin by number
    pub fn pin(&mut self, pin: u8) -> Option<&mut GpioPin> {
        self.pins.get_mut(pin as usize)
    }

    /// Get number of pins in this port
    pub fn pin_count(&self) -> usize {
        self.pins.len()
    }

    /// Set multiple pins at once (atomic operation)
    pub fn set_pins(&mut self, mask: u32, value: u32) {
        for (i, pin) in self.pins.iter_mut().enumerate() {
            if mask & (1 << i) != 0 {
                if value & (1 << i) != 0 {
                    pin.set_high();
                } else {
                    pin.set_low();
                }
            }
        }
    }

    /// Read all pins at once
    pub fn read_pins(&self) -> u32 {
        let mut value = 0u32;
        for (i, pin) in self.pins.iter().enumerate() {
            if pin.is_high() {
                value |= 1 << i;
            }
        }
        value
    }
}

// =============================================================================
// GPIO Controller
// =============================================================================

/// GPIO controller managing all ports
pub struct GpioController {
    /// All GPIO ports
    ports: Vec<GpioPort>,
}

impl GpioController {
    /// Create a new GPIO controller
    pub fn new() -> Self {
        Self { ports: Vec::new() }
    }

    /// Add a GPIO port
    pub fn add_port(&mut self, port: GpioPort) {
        self.ports.push(port);
    }

    /// Get a port by identifier
    pub fn port(&mut self, port: char) -> Option<&mut GpioPort> {
        self.ports.iter_mut().find(|p| p.port() == port)
    }

    /// Get number of ports
    pub fn port_count(&self) -> usize {
        self.ports.len()
    }

    /// Initialize GPIO subsystem
    pub fn init(&mut self) -> Result<(), GpioError> {
        // In a real implementation, this would:
        // 1. Enable GPIO peripheral clocks
        // 2. Configure default pin states
        // 3. Set up interrupt handlers
        Ok(())
    }
}

impl Default for GpioController {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Platform-Specific Implementations
// =============================================================================

/// Create GPIO controller for STM32F4xx
#[cfg(feature = "stm32f4")]
pub fn create_stm32f4_gpio() -> GpioController {
    let mut controller = GpioController::new();

    // STM32F4 has ports A-I, each with 16 pins
    for port_char in 'A'..='I' {
        controller.add_port(GpioPort::new(port_char, 16));
    }

    controller
}

/// Create GPIO controller for nRF52840
#[cfg(feature = "nrf52")]
pub fn create_nrf52_gpio() -> GpioController {
    let mut controller = GpioController::new();

    // nRF52 has 2 ports (P0, P1) with 32 pins each
    controller.add_port(GpioPort::new('0', 32));
    controller.add_port(GpioPort::new('1', 32));

    controller
}

/// Create GPIO controller for RP2040
#[cfg(feature = "rp2040")]
pub fn create_rp2040_gpio() -> GpioController {
    let mut controller = GpioController::new();

    // RP2040 has 30 GPIO pins (GPIO0-GPIO29)
    controller.add_port(GpioPort::new('0', 30));

    controller
}

/// Create GPIO controller for ESP32
#[cfg(feature = "esp32")]
pub fn create_esp32_gpio() -> GpioController {
    let mut controller = GpioController::new();

    // ESP32 has 40 GPIO pins (some are input-only)
    controller.add_port(GpioPort::new('0', 40));

    controller
}

// =============================================================================
// Errors
// =============================================================================

/// GPIO error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpioError {
    /// Invalid pin number
    InvalidPin,

    /// Invalid port identifier
    InvalidPort,

    /// Pin is not configured for requested operation
    InvalidMode,

    /// Interrupt configuration failed
    InterruptConfigFailed,

    /// Hardware error
    HardwareError,
}

impl core::fmt::Display for GpioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidPin => write!(f, "invalid pin number"),
            Self::InvalidPort => write!(f, "invalid port identifier"),
            Self::InvalidMode => write!(f, "pin not configured for requested operation"),
            Self::InterruptConfigFailed => write!(f, "interrupt configuration failed"),
            Self::HardwareError => write!(f, "hardware error"),
        }
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Calculate GPIO port and pin from absolute pin number
/// Example: pin 35 on STM32 = Port C, Pin 3 (assuming 16 pins per port)
pub fn absolute_to_port_pin(absolute_pin: u8, pins_per_port: u8) -> (char, u8) {
    let port_index = absolute_pin / pins_per_port;
    let pin_index = absolute_pin % pins_per_port;
    let port_char = (b'A' + port_index) as char;
    (port_char, pin_index)
}

/// Calculate absolute pin number from port and pin
pub fn port_pin_to_absolute(port: char, pin: u8, pins_per_port: u8) -> u8 {
    let port_index = (port as u8) - b'A';
    port_index * pins_per_port + pin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pin_creation() {
        let pin = GpioPin::new(13);
        assert_eq!(pin.pin(), 13);
        assert_eq!(pin.mode(), PinMode::Input);
    }

    #[test]
    fn test_pin_mode_config() {
        let mut pin = GpioPin::new(13);
        pin.set_mode(PinMode::Output);
        assert_eq!(pin.mode(), PinMode::Output);

        pin.set_mode(PinMode::Alternate(7));
        assert_eq!(pin.mode(), PinMode::Alternate(7));
    }

    #[test]
    fn test_pin_pull_config() {
        let mut pin = GpioPin::new(14);
        pin.set_pull(PullMode::PullUp);
        assert_eq!(pin.pull(), PullMode::PullUp);

        pin.set_pull(PullMode::PullDown);
        assert_eq!(pin.pull(), PullMode::PullDown);
    }

    #[test]
    fn test_output_operations() {
        let mut pin = GpioPin::new(13);
        pin.set_mode(PinMode::Output);

        pin.set_high();
        assert!(pin.is_high());
        assert!(!pin.is_low());

        pin.set_low();
        assert!(pin.is_low());
        assert!(!pin.is_high());

        pin.toggle();
        assert!(pin.is_high());

        pin.toggle();
        assert!(pin.is_low());
    }

    #[test]
    fn test_pin_state_conversion() {
        let state_high = PinState::from(true);
        assert_eq!(state_high, PinState::High);

        let state_low = PinState::from(false);
        assert_eq!(state_low, PinState::Low);

        assert!(bool::from(PinState::High));
        assert!(!bool::from(PinState::Low));
    }

    #[test]
    fn test_interrupt_config() {
        let mut pin = GpioPin::new(14);
        assert!(!pin.is_interrupt_enabled());

        pin.enable_interrupt(Edge::Rising);
        assert!(pin.is_interrupt_enabled());

        pin.disable_interrupt();
        assert!(!pin.is_interrupt_enabled());
    }

    #[test]
    fn test_drive_strength() {
        let mut pin = GpioPin::new(13);
        pin.set_drive_strength(DriveStrength::High);
        assert_eq!(pin.drive_strength(), DriveStrength::High);
    }

    #[test]
    fn test_slew_rate() {
        let mut pin = GpioPin::new(13);
        pin.set_slew_rate(SlewRate::Slow);
        assert_eq!(pin.slew_rate(), SlewRate::Slow);
    }

    #[test]
    fn test_gpio_port() {
        let mut port = GpioPort::new('A', 16);
        assert_eq!(port.port(), 'A');
        assert_eq!(port.pin_count(), 16);

        if let Some(pin) = port.pin(3) {
            pin.set_mode(PinMode::Output);
            assert_eq!(pin.mode(), PinMode::Output);
        }
    }

    #[test]
    fn test_port_set_pins() {
        let mut port = GpioPort::new('A', 16);

        // Set pins 0, 2, 4 to high
        port.set_pins(0b00010101, 0b00010101);

        assert!(port.pin(0).unwrap().is_high());
        assert!(port.pin(1).unwrap().is_low());
        assert!(port.pin(2).unwrap().is_high());
        assert!(port.pin(3).unwrap().is_low());
        assert!(port.pin(4).unwrap().is_high());
    }

    #[test]
    fn test_port_read_pins() {
        let mut port = GpioPort::new('A', 16);

        port.pin(0).unwrap().set_high();
        port.pin(2).unwrap().set_high();
        port.pin(4).unwrap().set_high();

        let value = port.read_pins();
        assert_eq!(value & 0b00010101, 0b00010101);
    }

    #[test]
    fn test_gpio_controller() {
        let mut controller = GpioController::new();
        assert_eq!(controller.port_count(), 0);

        controller.add_port(GpioPort::new('A', 16));
        controller.add_port(GpioPort::new('B', 16));

        assert_eq!(controller.port_count(), 2);

        if let Some(port_a) = controller.port('A') {
            assert_eq!(port_a.port(), 'A');
        }
    }

    #[test]
    fn test_absolute_to_port_pin() {
        // STM32-style: 16 pins per port
        let (port, pin) = absolute_to_port_pin(35, 16);
        assert_eq!(port, 'C'); // 35 / 16 = 2 = Port C
        assert_eq!(pin, 3); // 35 % 16 = 3
    }

    #[test]
    fn test_port_pin_to_absolute() {
        // STM32-style: 16 pins per port
        let absolute = port_pin_to_absolute('C', 3, 16);
        assert_eq!(absolute, 35); // 2 * 16 + 3 = 35
    }

    #[test]
    fn test_controller_init() {
        let mut controller = GpioController::new();
        let result = controller.init();
        assert!(result.is_ok());
    }
}
