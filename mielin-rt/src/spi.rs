//! SPI (Serial Peripheral Interface) Driver
//!
//! Provides a hardware abstraction layer for SPI communication. Supports both
//! master and slave modes with full-duplex operation.
//!
//! ## Overview
//!
//! The SPI module provides:
//! - Master and slave modes
//! - Full-duplex synchronous communication
//! - Configurable clock polarity (CPOL) and phase (CPHA)
//! - Multiple speed modes
//! - 8-bit and 16-bit data frames
//! - MSB/LSB first transmission
//! - Hardware and software chip select management
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::spi::{SpiMaster, SpiConfig, SpiMode, BitOrder};
//!
//! // Configure SPI master at 1 MHz, Mode 0
//! let config = SpiConfig::default()
//!     .with_mode(SpiMode::Mode0)
//!     .with_frequency_hz(1_000_000)
//!     .with_bit_order(BitOrder::MsbFirst);
//!
//! let mut spi = SpiMaster::new(0, config);
//! spi.init().unwrap();
//!
//! // Full-duplex transfer
//! let tx_data = [0x01, 0x02, 0x03];
//! let mut rx_data = [0u8; 3];
//! spi.transfer(&tx_data, &mut rx_data).unwrap();
//! ```

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// =============================================================================
// SPI Configuration
// =============================================================================

/// SPI mode (combination of CPOL and CPHA)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiMode {
    /// Mode 0: CPOL=0, CPHA=0 (clock idle low, sample on leading edge)
    Mode0,
    /// Mode 1: CPOL=0, CPHA=1 (clock idle low, sample on trailing edge)
    Mode1,
    /// Mode 2: CPOL=1, CPHA=0 (clock idle high, sample on leading edge)
    Mode2,
    /// Mode 3: CPOL=1, CPHA=1 (clock idle high, sample on trailing edge)
    Mode3,
}

impl SpiMode {
    /// Get clock polarity (CPOL)
    pub fn cpol(&self) -> bool {
        matches!(self, SpiMode::Mode2 | SpiMode::Mode3)
    }

    /// Get clock phase (CPHA)
    pub fn cpha(&self) -> bool {
        matches!(self, SpiMode::Mode1 | SpiMode::Mode3)
    }
}

/// Bit order for transmission
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOrder {
    /// Most significant bit first (default)
    MsbFirst,
    /// Least significant bit first
    LsbFirst,
}

/// Data frame size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSize {
    /// 8-bit data frames
    Bits8,
    /// 16-bit data frames
    Bits16,
}

/// Chip select mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipSelectMode {
    /// Hardware-controlled chip select
    Hardware,
    /// Software-controlled chip select
    Software,
}

/// SPI configuration
#[derive(Debug, Clone, Copy)]
pub struct SpiConfig {
    /// SPI mode (CPOL/CPHA combination)
    mode: SpiMode,
    /// Frequency in Hz
    frequency_hz: u32,
    /// Bit order
    bit_order: BitOrder,
    /// Data frame size
    data_size: DataSize,
    /// Chip select mode
    cs_mode: ChipSelectMode,
    /// Delay between frames in microseconds
    inter_frame_delay_us: u16,
}

impl SpiConfig {
    /// Create a new SPI configuration
    pub fn new() -> Self {
        Self {
            mode: SpiMode::Mode0,
            frequency_hz: 1_000_000, // 1 MHz default
            bit_order: BitOrder::MsbFirst,
            data_size: DataSize::Bits8,
            cs_mode: ChipSelectMode::Hardware,
            inter_frame_delay_us: 0,
        }
    }

    /// Set SPI mode
    pub fn with_mode(mut self, mode: SpiMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set frequency in Hz
    pub fn with_frequency_hz(mut self, frequency_hz: u32) -> Self {
        self.frequency_hz = frequency_hz;
        self
    }

    /// Set bit order
    pub fn with_bit_order(mut self, bit_order: BitOrder) -> Self {
        self.bit_order = bit_order;
        self
    }

    /// Set data frame size
    pub fn with_data_size(mut self, data_size: DataSize) -> Self {
        self.data_size = data_size;
        self
    }

    /// Set chip select mode
    pub fn with_cs_mode(mut self, cs_mode: ChipSelectMode) -> Self {
        self.cs_mode = cs_mode;
        self
    }

    /// Set inter-frame delay
    pub fn with_inter_frame_delay_us(mut self, delay_us: u16) -> Self {
        self.inter_frame_delay_us = delay_us;
        self
    }

    /// Get SPI mode
    pub fn mode(&self) -> SpiMode {
        self.mode
    }

    /// Get frequency
    pub fn frequency_hz(&self) -> u32 {
        self.frequency_hz
    }

    /// Get bit order
    pub fn bit_order(&self) -> BitOrder {
        self.bit_order
    }

    /// Get data size
    pub fn data_size(&self) -> DataSize {
        self.data_size
    }
}

impl Default for SpiConfig {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// SPI Master
// =============================================================================

/// SPI master controller
pub struct SpiMaster {
    /// Instance number (e.g., SPI0, SPI1, etc.)
    instance: u8,

    /// Configuration
    config: SpiConfig,

    /// Initialized flag
    initialized: AtomicBool,

    /// Transfer count
    transfer_count: AtomicU32,

    /// Error count
    error_count: AtomicU32,
}

impl SpiMaster {
    /// Create a new SPI master instance
    pub fn new(instance: u8, config: SpiConfig) -> Self {
        Self {
            instance,
            config,
            initialized: AtomicBool::new(false),
            transfer_count: AtomicU32::new(0),
            error_count: AtomicU32::new(0),
        }
    }

    /// Get instance number
    pub fn instance(&self) -> u8 {
        self.instance
    }

    /// Initialize SPI master
    pub fn init(&mut self) -> Result<(), SpiError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        // In a real implementation, this would:
        // 1. Enable SPI peripheral clock
        // 2. Configure GPIO pins (MOSI, MISO, SCK, NSS/CS)
        // 3. Set baud rate prescaler based on frequency
        // 4. Configure CPOL, CPHA, bit order, data size
        // 5. Enable SPI peripheral in master mode

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Write data (transmit only, ignore received data)
    pub fn write(&mut self, data: &[u8]) -> Result<(), SpiError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(SpiError::NotInitialized);
        }

        if data.is_empty() {
            return Err(SpiError::InvalidData);
        }

        // In a real implementation, this would:
        // 1. Assert chip select (if hardware mode)
        // 2. For each byte:
        //    - Write to TX register
        //    - Wait for TX empty flag
        //    - Read RX register (discard)
        // 3. Deassert chip select

        self.transfer_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Read data (transmit dummy bytes, receive data)
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<(), SpiError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(SpiError::NotInitialized);
        }

        if buffer.is_empty() {
            return Err(SpiError::InvalidData);
        }

        // In a real implementation, this would:
        // 1. Assert chip select
        // 2. For each byte:
        //    - Write dummy byte (0xFF) to TX register
        //    - Wait for RX not empty flag
        //    - Read from RX register
        // 3. Deassert chip select

        // Simulate: fill with zeros
        for byte in buffer.iter_mut() {
            *byte = 0;
        }

        self.transfer_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Full-duplex transfer (simultaneous TX and RX)
    pub fn transfer(&mut self, tx_data: &[u8], rx_buffer: &mut [u8]) -> Result<(), SpiError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(SpiError::NotInitialized);
        }

        if tx_data.len() != rx_buffer.len() {
            return Err(SpiError::LengthMismatch);
        }

        if tx_data.is_empty() {
            return Err(SpiError::InvalidData);
        }

        // In a real implementation, this would:
        // 1. Assert chip select
        // 2. For each byte:
        //    - Write TX byte to TX register
        //    - Wait for RX not empty flag
        //    - Read RX byte from RX register
        // 3. Deassert chip select

        // Simulate: echo back TX data
        rx_buffer.copy_from_slice(tx_data);

        self.transfer_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// In-place transfer (TX and RX use same buffer)
    pub fn transfer_in_place(&mut self, buffer: &mut [u8]) -> Result<(), SpiError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(SpiError::NotInitialized);
        }

        if buffer.is_empty() {
            return Err(SpiError::InvalidData);
        }

        // In a real implementation, this would perform SPI transfer
        // and replace buffer contents with received data

        self.transfer_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Assert chip select (software mode only)
    pub fn assert_cs(&mut self) -> Result<(), SpiError> {
        if self.config.cs_mode != ChipSelectMode::Software {
            return Err(SpiError::InvalidMode);
        }

        // In a real implementation, this would set CS pin low
        Ok(())
    }

    /// Deassert chip select (software mode only)
    pub fn deassert_cs(&mut self) -> Result<(), SpiError> {
        if self.config.cs_mode != ChipSelectMode::Software {
            return Err(SpiError::InvalidMode);
        }

        // In a real implementation, this would set CS pin high
        Ok(())
    }

    /// Get transfer count
    pub fn transfer_count(&self) -> u32 {
        self.transfer_count.load(Ordering::Relaxed)
    }

    /// Get error count
    pub fn error_count(&self) -> u32 {
        self.error_count.load(Ordering::Relaxed)
    }

    /// Reset counters
    pub fn reset_counters(&mut self) {
        self.transfer_count.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
    }
}

// =============================================================================
// SPI Slave
// =============================================================================

/// SPI slave controller
pub struct SpiSlave {
    /// Instance number
    instance: u8,

    /// Configuration
    #[allow(dead_code)]
    config: SpiConfig,

    /// Initialized flag
    initialized: AtomicBool,
}

impl SpiSlave {
    /// Create a new SPI slave instance
    pub fn new(instance: u8, config: SpiConfig) -> Self {
        Self {
            instance,
            config,
            initialized: AtomicBool::new(false),
        }
    }

    /// Get instance number
    pub fn instance(&self) -> u8 {
        self.instance
    }

    /// Initialize SPI slave
    pub fn init(&mut self) -> Result<(), SpiError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        // In a real implementation, this would:
        // 1. Enable SPI peripheral clock
        // 2. Configure GPIO pins
        // 3. Configure CPOL, CPHA, bit order, data size
        // 4. Enable SPI peripheral in slave mode
        // 5. Enable interrupts for TX/RX

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Receive data from master
    pub fn receive(&mut self, _buffer: &mut [u8]) -> Result<usize, SpiError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(SpiError::NotInitialized);
        }

        // In a real implementation, this would read from RX FIFO
        // Returns actual number of bytes received

        Ok(0) // Simulated: no data
    }

    /// Transmit data to master
    pub fn transmit(&mut self, data: &[u8]) -> Result<usize, SpiError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(SpiError::NotInitialized);
        }

        // In a real implementation, this would write to TX FIFO
        // Returns actual number of bytes transmitted

        Ok(data.len()) // Simulated: all transmitted
    }
}

// =============================================================================
// Errors
// =============================================================================

/// SPI error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiError {
    /// SPI not initialized
    NotInitialized,
    /// Invalid data (e.g., empty buffer)
    InvalidData,
    /// TX and RX buffer length mismatch
    LengthMismatch,
    /// Mode overrun (data lost)
    Overrun,
    /// Invalid mode for operation
    InvalidMode,
    /// Frame format error
    FrameError,
    /// Hardware error
    HardwareError,
    /// Timeout
    Timeout,
}

impl core::fmt::Display for SpiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "SPI not initialized"),
            Self::InvalidData => write!(f, "invalid data"),
            Self::LengthMismatch => write!(f, "TX/RX buffer length mismatch"),
            Self::Overrun => write!(f, "mode overrun - data lost"),
            Self::InvalidMode => write!(f, "invalid mode for operation"),
            Self::FrameError => write!(f, "frame format error"),
            Self::HardwareError => write!(f, "hardware error"),
            Self::Timeout => write!(f, "operation timeout"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::format;

    #[test]
    fn test_spi_mode_cpol_cpha() {
        assert!(!SpiMode::Mode0.cpol());
        assert!(!SpiMode::Mode0.cpha());

        assert!(!SpiMode::Mode1.cpol());
        assert!(SpiMode::Mode1.cpha());

        assert!(SpiMode::Mode2.cpol());
        assert!(!SpiMode::Mode2.cpha());

        assert!(SpiMode::Mode3.cpol());
        assert!(SpiMode::Mode3.cpha());
    }

    #[test]
    fn test_spi_config_default() {
        let config = SpiConfig::default();
        assert_eq!(config.mode(), SpiMode::Mode0);
        assert_eq!(config.frequency_hz(), 1_000_000);
        assert_eq!(config.bit_order(), BitOrder::MsbFirst);
        assert_eq!(config.data_size(), DataSize::Bits8);
    }

    #[test]
    fn test_spi_config_builder() {
        let config = SpiConfig::default()
            .with_mode(SpiMode::Mode3)
            .with_frequency_hz(8_000_000)
            .with_bit_order(BitOrder::LsbFirst)
            .with_data_size(DataSize::Bits16)
            .with_cs_mode(ChipSelectMode::Software)
            .with_inter_frame_delay_us(10);

        assert_eq!(config.mode(), SpiMode::Mode3);
        assert_eq!(config.frequency_hz(), 8_000_000);
        assert_eq!(config.bit_order(), BitOrder::LsbFirst);
        assert_eq!(config.data_size(), DataSize::Bits16);
    }

    #[test]
    fn test_spi_master_creation() {
        let config = SpiConfig::default();
        let master = SpiMaster::new(0, config);
        assert_eq!(master.instance(), 0);
        assert_eq!(master.transfer_count(), 0);
    }

    #[test]
    fn test_spi_master_init() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        assert!(master.init().is_ok());
    }

    #[test]
    fn test_spi_write_not_initialized() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        let data = [0x01, 0x02];
        assert_eq!(master.write(&data), Err(SpiError::NotInitialized));
    }

    #[test]
    fn test_spi_write_empty_data() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();
        let data = [];
        assert_eq!(master.write(&data), Err(SpiError::InvalidData));
    }

    #[test]
    fn test_spi_write_success() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();
        let data = [0x01, 0x02, 0x03];
        assert!(master.write(&data).is_ok());
        assert_eq!(master.transfer_count(), 1);
    }

    #[test]
    fn test_spi_read_success() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();
        let mut buffer = [0u8; 4];
        assert!(master.read(&mut buffer).is_ok());
        assert_eq!(master.transfer_count(), 1);
    }

    #[test]
    fn test_spi_transfer() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();

        let tx_data = [0xAA, 0xBB, 0xCC];
        let mut rx_buffer = [0u8; 3];

        assert!(master.transfer(&tx_data, &mut rx_buffer).is_ok());
        // Simulated: echo back
        assert_eq!(rx_buffer, tx_data);
        assert_eq!(master.transfer_count(), 1);
    }

    #[test]
    fn test_spi_transfer_length_mismatch() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();

        let tx_data = [0x01, 0x02, 0x03];
        let mut rx_buffer = [0u8; 2]; // Different length

        assert_eq!(
            master.transfer(&tx_data, &mut rx_buffer),
            Err(SpiError::LengthMismatch)
        );
    }

    #[test]
    fn test_spi_transfer_in_place() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();

        let mut buffer = [0x01, 0x02, 0x03];
        assert!(master.transfer_in_place(&mut buffer).is_ok());
        assert_eq!(master.transfer_count(), 1);
    }

    #[test]
    fn test_spi_cs_control() {
        let config = SpiConfig::default().with_cs_mode(ChipSelectMode::Software);
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();

        assert!(master.assert_cs().is_ok());
        assert!(master.deassert_cs().is_ok());
    }

    #[test]
    fn test_spi_cs_hardware_mode_error() {
        let config = SpiConfig::default().with_cs_mode(ChipSelectMode::Hardware);
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();

        assert_eq!(master.assert_cs(), Err(SpiError::InvalidMode));
        assert_eq!(master.deassert_cs(), Err(SpiError::InvalidMode));
    }

    #[test]
    fn test_spi_counter_reset() {
        let config = SpiConfig::default();
        let mut master = SpiMaster::new(0, config);
        master.init().unwrap();

        let data = [0x01];
        master.write(&data).unwrap();
        assert_eq!(master.transfer_count(), 1);

        master.reset_counters();
        assert_eq!(master.transfer_count(), 0);
    }

    #[test]
    fn test_spi_slave_creation() {
        let config = SpiConfig::default();
        let slave = SpiSlave::new(0, config);
        assert_eq!(slave.instance(), 0);
    }

    #[test]
    fn test_spi_slave_init() {
        let config = SpiConfig::default();
        let mut slave = SpiSlave::new(0, config);
        assert!(slave.init().is_ok());
    }

    #[test]
    fn test_spi_slave_receive() {
        let config = SpiConfig::default();
        let mut slave = SpiSlave::new(0, config);
        slave.init().unwrap();

        let mut buffer = [0u8; 4];
        let result = slave.receive(&mut buffer);
        assert!(result.is_ok());
    }

    #[test]
    fn test_spi_slave_transmit() {
        let config = SpiConfig::default();
        let mut slave = SpiSlave::new(0, config);
        slave.init().unwrap();

        let data = [0x01, 0x02, 0x03];
        let result = slave.transmit(&data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3);
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", SpiError::NotInitialized),
            "SPI not initialized"
        );
        assert_eq!(
            format!("{}", SpiError::LengthMismatch),
            "TX/RX buffer length mismatch"
        );
        assert_eq!(format!("{}", SpiError::Overrun), "mode overrun - data lost");
    }
}
