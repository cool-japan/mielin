//! RV32IMAC-specific peripheral abstractions.
//!
//! This module provides GPIO, timer, and memory-map descriptors for the most
//! common rv32imac embedded MCUs:
//!
//! - **ESP32-C3** (Espressif, 160 MHz, 400 KiB SRAM, Wi-Fi/BLE)
//! - **GD32VF103** (GigaDevice, 108 MHz, 32 KiB SRAM, USB)
//! - **CH32V003** (WCH, 48 MHz, 2 KiB SRAM, smallest rv32ec variant)

use core::fmt;

// ---------------------------------------------------------------------------
// GPIO Errors
// ---------------------------------------------------------------------------

/// Errors returned by [`Rv32ImacGpio`] operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpioError {
    /// Pin index exceeds the hardware pin count
    InvalidPin { pin: u8, max: u8 },
    /// A write was attempted on a pin not configured as output
    NotConfiguredAsOutput(u8),
    /// A read was attempted on a pin not configured as input
    NotConfiguredAsInput(u8),
}

impl fmt::Display for GpioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpioError::InvalidPin { pin, max } => {
                write!(f, "GPIO pin {pin} out of range (max {max})")
            }
            GpioError::NotConfiguredAsOutput(pin) => {
                write!(f, "GPIO pin {pin} is not configured as an output")
            }
            GpioError::NotConfiguredAsInput(pin) => {
                write!(f, "GPIO pin {pin} is not configured as an input")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GPIO Interrupt Edge
// ---------------------------------------------------------------------------

/// Interrupt trigger edge for a GPIO pin
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptEdge {
    /// Trigger on rising edge (low-to-high transition)
    Rising,
    /// Trigger on falling edge (high-to-low transition)
    Falling,
    /// Trigger on both edges
    Both,
}

// ---------------------------------------------------------------------------
// GPIO
// ---------------------------------------------------------------------------

/// GPIO port peripheral for a generic rv32imac MCU.
///
/// Models the direction register (OE) and the output/input data registers
/// that are common across ESP32-C3, GD32VF103, and CH32V series devices.
///
/// On real hardware, register accesses use `read_volatile`/`write_volatile`
/// on the MMIO base address.  In simulation/test mode all accesses are
/// suppressed to allow host unit tests to run safely.
///
/// Register layout assumed (each 32-bit, one bit per pin):
/// ```text
/// base + 0x00  OE  (output enable; 1 = output)
/// base + 0x04  OUT (output data)
/// base + 0x08  IN  (input data, read-only)
/// base + 0x0C  INT_EN  (interrupt enable)
/// base + 0x10  INT_TYPE  (0 = rising, 1 = falling, 2 = both)
/// ```
pub struct Rv32ImacGpio {
    /// MMIO base address of the GPIO port
    pub base_addr: u32,
    /// Number of GPIO pins in this port (1-32)
    pub num_pins: u8,
    /// Whether this GPIO peripheral has a separate output-enable register
    pub has_output_enable: bool,
    // Internal shadow state used for simulation (host tests)
    direction_mask: u32, // 1 = output, 0 = input
    output_state: u32,
}

impl Rv32ImacGpio {
    /// Create a new GPIO abstraction at `base` MMIO address with `pins` total pins.
    ///
    /// All pins default to input mode with outputs low.
    pub fn new(base: u32, pins: u8) -> Self {
        let pins = pins.clamp(1, 32);
        Self {
            base_addr: base,
            num_pins: pins,
            has_output_enable: true,
            direction_mask: 0,
            output_state: 0,
        }
    }

    /// Validate that `pin` is within range for this port
    fn check_pin(&self, pin: u8) -> Result<(), GpioError> {
        if pin >= self.num_pins {
            Err(GpioError::InvalidPin {
                pin,
                max: self.num_pins - 1,
            })
        } else {
            Ok(())
        }
    }

    /// MMIO address of the OE (output-enable) register
    #[allow(dead_code)] // used inside target_arch = "riscv*" cfg blocks
    fn oe_addr(&self) -> u32 {
        self.base_addr
    }

    /// MMIO address of the output-data register
    #[allow(dead_code)]
    fn out_addr(&self) -> u32 {
        self.base_addr + 0x04
    }

    /// MMIO address of the input-data register
    #[allow(dead_code)]
    fn in_addr(&self) -> u32 {
        self.base_addr + 0x08
    }

    /// MMIO address of the interrupt-enable register
    #[allow(dead_code)]
    fn int_en_addr(&self) -> u32 {
        self.base_addr + 0x0C
    }

    /// MMIO address of the interrupt-type register
    #[allow(dead_code)]
    fn int_type_addr(&self) -> u32 {
        self.base_addr + 0x10
    }

    /// Configure a pin as output (`true`) or input (`false`).
    ///
    /// Updates the OE (output-enable) register for the entire port atomically.
    pub fn set_direction(&mut self, pin: u8, output: bool) -> Result<(), GpioError> {
        self.check_pin(pin)?;
        if output {
            self.direction_mask |= 1u32 << pin;
        } else {
            self.direction_mask &= !(1u32 << pin);
        }

        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            core::ptr::write_volatile(self.oe_addr() as *mut u32, self.direction_mask);
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            // Simulation: direction state already updated in shadow register
        }
        Ok(())
    }

    /// Drive an output pin high (`true`) or low (`false`).
    ///
    /// Returns `Err(GpioError::NotConfiguredAsOutput)` if the pin is in input mode.
    pub fn set_output(&mut self, pin: u8, high: bool) -> Result<(), GpioError> {
        self.check_pin(pin)?;
        if self.direction_mask & (1u32 << pin) == 0 {
            return Err(GpioError::NotConfiguredAsOutput(pin));
        }
        if high {
            self.output_state |= 1u32 << pin;
        } else {
            self.output_state &= !(1u32 << pin);
        }

        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            core::ptr::write_volatile(self.out_addr() as *mut u32, self.output_state);
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            // Shadow state already updated above
        }
        Ok(())
    }

    /// Read the current logical value of an input pin.
    ///
    /// Returns `Err(GpioError::NotConfiguredAsInput)` if the pin is an output.
    /// On the host simulation path, input pins always read back `false` (low).
    pub fn read_input(&self, pin: u8) -> Result<bool, GpioError> {
        self.check_pin(pin)?;
        if self.direction_mask & (1u32 << pin) != 0 {
            return Err(GpioError::NotConfiguredAsInput(pin));
        }

        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let reg = unsafe { core::ptr::read_volatile(self.in_addr() as *const u32) };
            Ok((reg >> pin) & 1 != 0)
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            // Simulation path: inputs read as 0
            Ok(false)
        }
    }

    /// Configure a pin as an interrupt source on the given edge.
    ///
    /// This enables the interrupt in the INT_EN register and sets the trigger
    /// type.  On most rv32imac MCUs the pin must first be configured as input.
    pub fn enable_interrupt(&mut self, pin: u8, edge: InterruptEdge) -> Result<(), GpioError> {
        self.check_pin(pin)?;

        let type_code: u32 = match edge {
            InterruptEdge::Rising => 0,
            InterruptEdge::Falling => 1,
            InterruptEdge::Both => 2,
        };

        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            // Enable interrupt for this pin
            let en = core::ptr::read_volatile(self.int_en_addr() as *const u32);
            core::ptr::write_volatile(self.int_en_addr() as *mut u32, en | (1u32 << pin));

            // Set 2-bit type field at bits [2*pin+1:2*pin] of INT_TYPE register
            let shift = (pin as u32) * 2;
            let mask = 0x3u32 << shift;
            let current = core::ptr::read_volatile(self.int_type_addr() as *const u32);
            core::ptr::write_volatile(
                self.int_type_addr() as *mut u32,
                (current & !mask) | (type_code << shift),
            );
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = type_code;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Timer
// ---------------------------------------------------------------------------

/// General-purpose timer peripheral for rv32imac MCUs.
///
/// Models the common RISC-V SoC timer structure: a free-running 32-bit
/// counter driven from the peripheral clock, a compare register that fires
/// an interrupt on match, and enable/disable controls.
///
/// Register layout (each 32-bit):
/// ```text
/// base + 0x00  CNT    (counter, read/write to reset)
/// base + 0x04  CMP    (compare value; interrupt fires when CNT >= CMP)
/// base + 0x08  CTRL   (bit 0 = enable, bit 1 = interrupt enable)
/// ```
pub struct Rv32ImacTimer {
    /// MMIO base address of the timer peripheral
    pub base_addr: u32,
    /// Input clock frequency in Hz (used for tick/us conversion)
    pub clock_hz: u32,
}

impl Rv32ImacTimer {
    /// Create a new timer abstraction at `base` address with `clock_hz` input clock.
    pub fn new(base: u32, clock_hz: u32) -> Self {
        Self {
            base_addr: base,
            clock_hz,
        }
    }

    #[allow(dead_code)] // used inside target_arch = "riscv*" cfg blocks
    fn cnt_addr(&self) -> u32 {
        self.base_addr
    }
    #[allow(dead_code)]
    fn cmp_addr(&self) -> u32 {
        self.base_addr + 0x04
    }
    #[allow(dead_code)]
    fn ctrl_addr(&self) -> u32 {
        self.base_addr + 0x08
    }

    /// Write the compare register (interrupt fires when counter reaches this value).
    pub fn set_compare(&self, ticks: u32) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            core::ptr::write_volatile(self.cmp_addr() as *mut u32, ticks);
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = ticks;
        }
    }

    /// Read the current counter value.
    ///
    /// Returns 0 in host simulation mode.
    pub fn read_counter(&self) -> u32 {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            unsafe { core::ptr::read_volatile(self.cnt_addr() as *const u32) }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            0
        }
    }

    /// Enable the timer (set CTRL bit 0).
    pub fn enable(&self) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            let v = core::ptr::read_volatile(self.ctrl_addr() as *const u32);
            core::ptr::write_volatile(self.ctrl_addr() as *mut u32, v | 0x1);
        }
    }

    /// Disable the timer (clear CTRL bit 0).
    pub fn disable(&self) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            let v = core::ptr::read_volatile(self.ctrl_addr() as *const u32);
            core::ptr::write_volatile(self.ctrl_addr() as *mut u32, v & !0x1);
        }
    }

    /// Convert a duration in microseconds to timer ticks.
    ///
    /// Uses integer arithmetic to avoid soft-float on bare-metal rv32imac.
    /// Returns a `u64` to avoid overflow for large `us` values.
    pub fn micros_to_ticks(&self, us: u64) -> u64 {
        us.saturating_mul(self.clock_hz as u64) / 1_000_000
    }

    /// Convert timer ticks to microseconds.
    pub fn ticks_to_micros(&self, ticks: u64) -> u64 {
        ticks.saturating_mul(1_000_000) / self.clock_hz as u64
    }
}

// ---------------------------------------------------------------------------
// Memory Map
// ---------------------------------------------------------------------------

/// Memory-map descriptor for a rv32imac MCU.
///
/// Encodes the physical addresses and sizes of Flash, SRAM, and peripherals
/// for the most common rv32imac target devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rv32ImacMemoryMap {
    /// Base address of Flash / program memory
    pub flash_base: u32,
    /// Total flash size in bytes
    pub flash_size: u32,
    /// Base address of the SRAM data region
    pub sram_base: u32,
    /// Total SRAM size in bytes
    pub sram_size: u32,
    /// Base address of the peripheral MMIO region
    pub peripheral_base: u32,
}

impl Rv32ImacMemoryMap {
    /// Memory map for the **ESP32-C3** (Espressif RV32IMC, 160 MHz).
    ///
    /// - Flash: 4 MiB mapped at 0x4200_0000 (external + cache)
    /// - SRAM: 400 KiB at 0x3FC8_0000 (HP memory)
    /// - Peripherals: 0x6000_0000
    pub fn esp32c3() -> Self {
        Self {
            flash_base: 0x4200_0000,
            flash_size: 4 * 1024 * 1024,
            sram_base: 0x3FC8_0000,
            sram_size: 400 * 1024,
            peripheral_base: 0x6000_0000,
        }
    }

    /// Memory map for the **GD32VF103** (GigaDevice RV32IMAC, 108 MHz).
    ///
    /// - Flash: 128 KiB at 0x0800_0000
    /// - SRAM: 32 KiB at 0x2000_0000
    /// - Peripherals: 0x4000_0000
    pub fn gd32vf103() -> Self {
        Self {
            flash_base: 0x0800_0000,
            flash_size: 128 * 1024,
            sram_base: 0x2000_0000,
            sram_size: 32 * 1024,
            peripheral_base: 0x4000_0000,
        }
    }

    /// Memory map for the **CH32V003** (WCH RV32EC, 48 MHz) — smallest variant.
    ///
    /// - Flash: 16 KiB at 0x0800_0000
    /// - SRAM: 2 KiB at 0x2000_0000
    /// - Peripherals: 0x4000_0000
    ///
    /// Note: CH32V003 uses the rv32ec ISA (embedded + compressed, no multiply)
    /// but the same ABI/toolchain as rv32imac, so it is grouped here.
    pub fn ch32v003() -> Self {
        Self {
            flash_base: 0x0800_0000,
            flash_size: 16 * 1024,
            sram_base: 0x2000_0000,
            sram_size: 2 * 1024,
            peripheral_base: 0x4000_0000,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- GPIO creation and pin validation ---

    #[test]
    fn test_rv32imac_gpio_new() {
        let gpio = Rv32ImacGpio::new(0x6000_0004, 32);
        assert_eq!(gpio.base_addr, 0x6000_0004);
        assert_eq!(gpio.num_pins, 32);
        assert!(gpio.has_output_enable);
        assert_eq!(gpio.direction_mask, 0);
        assert_eq!(gpio.output_state, 0);
    }

    #[test]
    fn test_rv32imac_gpio_invalid_pin() {
        let gpio = Rv32ImacGpio::new(0x6000_0004, 8);
        // The direction_mask write needs &mut so we use direct err check on check_pin path
        // by calling a read (which is &self)
        let result = gpio.read_input(8); // pin 8 >= num_pins 8 -> error
        assert!(matches!(
            result,
            Err(GpioError::InvalidPin { pin: 8, max: 7 })
        ));
    }

    #[test]
    fn test_rv32imac_gpio_not_configured_as_output() {
        let mut gpio = Rv32ImacGpio::new(0x6000_0004, 16);
        // All pins default to input; writing should fail
        let result = gpio.set_output(0, true);
        assert!(matches!(result, Err(GpioError::NotConfiguredAsOutput(0))));
    }

    #[test]
    fn test_rv32imac_gpio_not_configured_as_input() {
        let mut gpio = Rv32ImacGpio::new(0x6000_0004, 16);
        // Configure pin 0 as output
        gpio.set_direction(0, true).unwrap();
        // Reading an output pin returns error
        let result = gpio.read_input(0);
        assert!(matches!(result, Err(GpioError::NotConfiguredAsInput(0))));
    }

    #[test]
    fn test_rv32imac_gpio_direction_set_and_output() {
        let mut gpio = Rv32ImacGpio::new(0x6000_0004, 16);
        gpio.set_direction(3, true).unwrap();
        assert_eq!(gpio.direction_mask & (1 << 3), 1 << 3);
        gpio.set_output(3, true).unwrap();
        assert_eq!(gpio.output_state & (1 << 3), 1 << 3);
        gpio.set_output(3, false).unwrap();
        assert_eq!(gpio.output_state & (1 << 3), 0);
    }

    #[test]
    fn test_rv32imac_gpio_read_input_simulation() {
        let gpio = Rv32ImacGpio::new(0x6000_0004, 8);
        // All pins are inputs by default; simulation returns false
        let v = gpio.read_input(0).unwrap();
        assert!(!v);
    }

    // --- Timer conversion ---

    #[test]
    fn test_rv32imac_timer_micros_to_ticks() {
        // 1 MHz clock: 1000 us -> 1000 ticks
        let t = Rv32ImacTimer::new(0x4001_0000, 1_000_000);
        assert_eq!(t.micros_to_ticks(1000), 1000);
    }

    #[test]
    fn test_rv32imac_timer_ticks_to_micros() {
        let t = Rv32ImacTimer::new(0x4001_0000, 1_000_000);
        // Round-trip
        let us = 5_000u64;
        let ticks = t.micros_to_ticks(us);
        assert_eq!(t.ticks_to_micros(ticks), us);
    }

    #[test]
    fn test_rv32imac_timer_clock_hz_stored() {
        let t = Rv32ImacTimer::new(0x4001_0000, 48_000_000);
        assert_eq!(t.clock_hz, 48_000_000);
        assert_eq!(t.base_addr, 0x4001_0000);
    }

    #[test]
    fn test_rv32imac_timer_read_counter_simulation() {
        let t = Rv32ImacTimer::new(0x4001_0000, 1_000_000);
        // Simulation path always returns 0
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        assert_eq!(t.read_counter(), 0);
    }

    // --- Memory maps ---

    #[test]
    fn test_esp32c3_memory_map() {
        let m = Rv32ImacMemoryMap::esp32c3();
        assert_eq!(m.flash_base, 0x4200_0000);
        assert_eq!(m.flash_size, 4 * 1024 * 1024);
        assert_eq!(m.sram_base, 0x3FC8_0000);
        assert_eq!(m.sram_size, 400 * 1024);
        assert_eq!(m.peripheral_base, 0x6000_0000);
    }

    #[test]
    fn test_gd32vf103_memory_map() {
        let m = Rv32ImacMemoryMap::gd32vf103();
        assert_eq!(m.flash_base, 0x0800_0000);
        assert_eq!(m.flash_size, 128 * 1024);
        assert_eq!(m.sram_base, 0x2000_0000);
        assert_eq!(m.sram_size, 32 * 1024);
        assert_eq!(m.peripheral_base, 0x4000_0000);
    }

    #[test]
    fn test_ch32v003_memory_map() {
        let m = Rv32ImacMemoryMap::ch32v003();
        // CH32V003 has very small flash and SRAM
        assert_eq!(m.flash_size, 16 * 1024);
        assert_eq!(m.sram_size, 2 * 1024);
        assert!(
            m.sram_size < m.flash_size,
            "SRAM must be smaller than Flash on CH32V003"
        );
        assert_eq!(m.flash_base, 0x0800_0000);
    }
}
