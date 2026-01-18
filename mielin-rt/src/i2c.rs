//! I2C (Inter-Integrated Circuit) Peripheral Driver
//!
//! Provides a hardware abstraction layer for I2C communication across different
//! microcontroller families. Supports both master and slave modes, with emphasis
//! on master mode for typical embedded applications.
//!
//! ## Overview
//!
//! The I2C module provides:
//! - Master and slave modes
//! - Multiple speed modes (Standard, Fast, Fast+, High-speed)
//! - 7-bit and 10-bit addressing
//! - Read/write operations with error handling
//! - Clock stretching support
//! - Multi-master arbitration
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::i2c::{I2cMaster, I2cConfig, SpeedMode};
//!
//! // Configure I2C master at 400kHz (Fast mode)
//! let config = I2cConfig::default()
//!     .with_speed(SpeedMode::Fast)
//!     .with_timeout_ms(100);
//!
//! let mut i2c = I2cMaster::new(0, config);
//! i2c.init().unwrap();
//!
//! // Write to device at address 0x50
//! let data = [0x00, 0x01, 0x02];
//! i2c.write(0x50, &data).unwrap();
//!
//! // Read from device
//! let mut buffer = [0u8; 4];
//! i2c.read(0x50, &mut buffer).unwrap();
//! ```

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// =============================================================================
// I2C Configuration
// =============================================================================

/// I2C speed modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedMode {
    /// Standard mode: 100 kHz
    Standard,
    /// Fast mode: 400 kHz
    Fast,
    /// Fast mode plus: 1 MHz
    FastPlus,
    /// High-speed mode: 3.4 MHz
    HighSpeed,
}

impl SpeedMode {
    /// Get frequency in Hz
    pub fn frequency_hz(&self) -> u32 {
        match self {
            SpeedMode::Standard => 100_000,
            SpeedMode::Fast => 400_000,
            SpeedMode::FastPlus => 1_000_000,
            SpeedMode::HighSpeed => 3_400_000,
        }
    }
}

/// I2C addressing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressingMode {
    /// 7-bit addressing (default)
    SevenBit,
    /// 10-bit addressing
    TenBit,
}

/// I2C configuration
#[derive(Debug, Clone, Copy)]
pub struct I2cConfig {
    /// Speed mode
    speed: SpeedMode,
    /// Addressing mode
    addressing: AddressingMode,
    /// Enable clock stretching
    clock_stretching: bool,
    /// Timeout in milliseconds
    timeout_ms: u32,
    /// Enable general call addressing
    general_call: bool,
    /// Digital noise filter (0-15, 0 = disabled)
    noise_filter: u8,
}

impl I2cConfig {
    /// Create a new I2C configuration
    pub fn new() -> Self {
        Self {
            speed: SpeedMode::Fast,
            addressing: AddressingMode::SevenBit,
            clock_stretching: true,
            timeout_ms: 100,
            general_call: false,
            noise_filter: 0,
        }
    }

    /// Set speed mode
    pub fn with_speed(mut self, speed: SpeedMode) -> Self {
        self.speed = speed;
        self
    }

    /// Set addressing mode
    pub fn with_addressing(mut self, addressing: AddressingMode) -> Self {
        self.addressing = addressing;
        self
    }

    /// Enable/disable clock stretching
    pub fn with_clock_stretching(mut self, enabled: bool) -> Self {
        self.clock_stretching = enabled;
        self
    }

    /// Set timeout in milliseconds
    pub fn with_timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Enable general call addressing
    pub fn with_general_call(mut self, enabled: bool) -> Self {
        self.general_call = enabled;
        self
    }

    /// Set digital noise filter level (0-15)
    pub fn with_noise_filter(mut self, level: u8) -> Self {
        self.noise_filter = level.min(15);
        self
    }

    /// Get speed mode
    pub fn speed(&self) -> SpeedMode {
        self.speed
    }

    /// Get addressing mode
    pub fn addressing(&self) -> AddressingMode {
        self.addressing
    }

    /// Check if clock stretching is enabled
    pub fn clock_stretching_enabled(&self) -> bool {
        self.clock_stretching
    }

    /// Get timeout in milliseconds
    pub fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }
}

impl Default for I2cConfig {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// I2C Master
// =============================================================================

/// I2C master controller
pub struct I2cMaster {
    /// Instance number (e.g., I2C0, I2C1, etc.)
    instance: u8,

    /// Configuration
    config: I2cConfig,

    /// Initialized flag
    initialized: AtomicBool,

    /// Transfer count
    transfer_count: AtomicU32,

    /// Error count
    error_count: AtomicU32,
}

impl I2cMaster {
    /// Create a new I2C master instance
    pub fn new(instance: u8, config: I2cConfig) -> Self {
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

    /// Initialize I2C master
    pub fn init(&mut self) -> Result<(), I2cError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        // In a real implementation, this would:
        // 1. Enable I2C peripheral clock
        // 2. Configure GPIO pins for I2C (SDA, SCL)
        // 3. Set timing registers based on speed mode
        // 4. Enable I2C peripheral
        // 5. Configure filters and features

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Write data to a device
    pub fn write(&mut self, address: u8, data: &[u8]) -> Result<(), I2cError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(I2cError::NotInitialized);
        }

        if data.is_empty() {
            return Err(I2cError::InvalidData);
        }

        // In a real implementation, this would:
        // 1. Generate START condition
        // 2. Send address with write bit
        // 3. Wait for ACK
        // 4. Send data bytes
        // 5. Wait for ACK after each byte
        // 6. Generate STOP condition

        self.transfer_count.fetch_add(1, Ordering::Relaxed);

        // Simulate: check if address is valid
        if address > 0x7F && self.config.addressing == AddressingMode::SevenBit {
            self.error_count.fetch_add(1, Ordering::Relaxed);
            return Err(I2cError::InvalidAddress);
        }

        Ok(())
    }

    /// Read data from a device
    pub fn read(&mut self, _address: u8, buffer: &mut [u8]) -> Result<(), I2cError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(I2cError::NotInitialized);
        }

        if buffer.is_empty() {
            return Err(I2cError::InvalidData);
        }

        // In a real implementation, this would:
        // 1. Generate START condition
        // 2. Send address with read bit
        // 3. Wait for ACK
        // 4. Read data bytes, sending ACK after each (except last)
        // 5. Send NACK after last byte
        // 6. Generate STOP condition

        self.transfer_count.fetch_add(1, Ordering::Relaxed);

        // Simulate: fill buffer with zeros
        for byte in buffer.iter_mut() {
            *byte = 0;
        }

        Ok(())
    }

    /// Write then read (common pattern for register access)
    pub fn write_read(
        &mut self,
        address: u8,
        write_data: &[u8],
        read_buffer: &mut [u8],
    ) -> Result<(), I2cError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(I2cError::NotInitialized);
        }

        // In a real implementation, this would:
        // 1. Generate START condition
        // 2. Send address with write bit
        // 3. Send write data
        // 4. Generate repeated START condition
        // 5. Send address with read bit
        // 6. Read data
        // 7. Generate STOP condition

        self.write(address, write_data)?;
        self.read(address, read_buffer)?;

        Ok(())
    }

    /// Check if device is present on bus
    pub fn probe(&mut self, address: u8) -> Result<bool, I2cError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(I2cError::NotInitialized);
        }

        // In a real implementation, this would:
        // 1. Generate START condition
        // 2. Send address with write bit
        // 3. Check for ACK/NACK
        // 4. Generate STOP condition

        // Simulate: addresses 0x50-0x57 are present
        Ok((0x50..=0x57).contains(&address))
    }

    /// Scan bus for devices
    pub fn scan(&mut self) -> Result<Vec<u8>, I2cError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(I2cError::NotInitialized);
        }

        let mut devices = Vec::new();

        let max_addr = match self.config.addressing {
            AddressingMode::SevenBit => 0x7F,
            AddressingMode::TenBit => 0x3FF,
        };

        // Skip reserved addresses (0x00-0x07, 0x78-0x7F for 7-bit)
        let start = if self.config.addressing == AddressingMode::SevenBit {
            0x08
        } else {
            0x00
        };
        let end = if self.config.addressing == AddressingMode::SevenBit {
            0x77
        } else {
            max_addr
        };

        for addr in start..=end {
            if self.probe(addr as u8)? {
                devices.push(addr as u8);
            }
        }

        Ok(devices)
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
// I2C Slave
// =============================================================================

/// I2C slave controller
pub struct I2cSlave {
    /// Instance number
    instance: u8,

    /// Own address
    own_address: u16,

    /// Addressing mode
    addressing: AddressingMode,

    /// Initialized flag
    initialized: AtomicBool,

    /// Receive buffer
    rx_buffer: Vec<u8>,

    /// Transmit buffer
    tx_buffer: Vec<u8>,
}

impl I2cSlave {
    /// Create a new I2C slave instance
    pub fn new(instance: u8, own_address: u16, addressing: AddressingMode) -> Self {
        Self {
            instance,
            own_address,
            addressing,
            initialized: AtomicBool::new(false),
            rx_buffer: Vec::new(),
            tx_buffer: Vec::new(),
        }
    }

    /// Get instance number
    pub fn instance(&self) -> u8 {
        self.instance
    }

    /// Get own address
    pub fn own_address(&self) -> u16 {
        self.own_address
    }

    /// Initialize I2C slave
    pub fn init(&mut self) -> Result<(), I2cError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        // Validate address
        match self.addressing {
            AddressingMode::SevenBit if self.own_address > 0x7F => {
                return Err(I2cError::InvalidAddress);
            }
            AddressingMode::TenBit if self.own_address > 0x3FF => {
                return Err(I2cError::InvalidAddress);
            }
            _ => {}
        }

        // In a real implementation, this would:
        // 1. Enable I2C peripheral clock
        // 2. Configure GPIO pins for I2C
        // 3. Set own address registers
        // 4. Enable slave mode
        // 5. Enable interrupts for address match, rx, tx

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Set data to transmit when addressed
    pub fn set_transmit_data(&mut self, data: &[u8]) {
        self.tx_buffer.clear();
        self.tx_buffer.extend_from_slice(data);
    }

    /// Get received data
    pub fn get_received_data(&self) -> &[u8] {
        &self.rx_buffer
    }

    /// Clear receive buffer
    pub fn clear_receive_buffer(&mut self) {
        self.rx_buffer.clear();
    }
}

// =============================================================================
// I2C Transaction
// =============================================================================

/// I2C transaction type for batch operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    /// Write operation
    Write,
    /// Read operation
    Read,
}

/// I2C transaction for batch operations
pub struct I2cTransaction<'a> {
    /// Transaction type
    transaction_type: TransactionType,
    /// Data buffer
    buffer: &'a mut [u8],
}

impl<'a> I2cTransaction<'a> {
    /// Create a write transaction
    pub fn write(buffer: &'a mut [u8]) -> Self {
        Self {
            transaction_type: TransactionType::Write,
            buffer,
        }
    }

    /// Create a read transaction
    pub fn read(buffer: &'a mut [u8]) -> Self {
        Self {
            transaction_type: TransactionType::Read,
            buffer,
        }
    }

    /// Get transaction type
    pub fn transaction_type(&self) -> TransactionType {
        self.transaction_type
    }

    /// Get buffer
    pub fn buffer(&self) -> &[u8] {
        self.buffer
    }

    /// Get mutable buffer
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        self.buffer
    }
}

// =============================================================================
// Errors
// =============================================================================

/// I2C error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2cError {
    /// I2C not initialized
    NotInitialized,
    /// Invalid address
    InvalidAddress,
    /// Invalid data (e.g., empty buffer)
    InvalidData,
    /// NACK received (device not responding)
    Nack,
    /// Bus error (e.g., misplaced START/STOP)
    BusError,
    /// Arbitration lost (multi-master conflict)
    ArbitrationLost,
    /// Timeout waiting for operation
    Timeout,
    /// Buffer overflow
    BufferOverflow,
    /// Hardware error
    HardwareError,
}

impl core::fmt::Display for I2cError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "I2C not initialized"),
            Self::InvalidAddress => write!(f, "invalid I2C address"),
            Self::InvalidData => write!(f, "invalid data"),
            Self::Nack => write!(f, "NACK received from device"),
            Self::BusError => write!(f, "I2C bus error"),
            Self::ArbitrationLost => write!(f, "arbitration lost"),
            Self::Timeout => write!(f, "operation timeout"),
            Self::BufferOverflow => write!(f, "buffer overflow"),
            Self::HardwareError => write!(f, "hardware error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::format;

    #[test]
    fn test_speed_mode_frequency() {
        assert_eq!(SpeedMode::Standard.frequency_hz(), 100_000);
        assert_eq!(SpeedMode::Fast.frequency_hz(), 400_000);
        assert_eq!(SpeedMode::FastPlus.frequency_hz(), 1_000_000);
        assert_eq!(SpeedMode::HighSpeed.frequency_hz(), 3_400_000);
    }

    #[test]
    fn test_i2c_config_default() {
        let config = I2cConfig::default();
        assert_eq!(config.speed(), SpeedMode::Fast);
        assert_eq!(config.addressing(), AddressingMode::SevenBit);
        assert!(config.clock_stretching_enabled());
        assert_eq!(config.timeout_ms(), 100);
    }

    #[test]
    fn test_i2c_config_builder() {
        let config = I2cConfig::default()
            .with_speed(SpeedMode::Standard)
            .with_addressing(AddressingMode::TenBit)
            .with_clock_stretching(false)
            .with_timeout_ms(200)
            .with_general_call(true)
            .with_noise_filter(5);

        assert_eq!(config.speed(), SpeedMode::Standard);
        assert_eq!(config.addressing(), AddressingMode::TenBit);
        assert!(!config.clock_stretching_enabled());
        assert_eq!(config.timeout_ms(), 200);
    }

    #[test]
    fn test_i2c_master_creation() {
        let config = I2cConfig::default();
        let master = I2cMaster::new(0, config);
        assert_eq!(master.instance(), 0);
        assert_eq!(master.transfer_count(), 0);
        assert_eq!(master.error_count(), 0);
    }

    #[test]
    fn test_i2c_master_init() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        assert!(master.init().is_ok());
    }

    #[test]
    fn test_i2c_write_not_initialized() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        let data = [0x01, 0x02];
        assert_eq!(master.write(0x50, &data), Err(I2cError::NotInitialized));
    }

    #[test]
    fn test_i2c_write_empty_data() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        master.init().unwrap();
        let data = [];
        assert_eq!(master.write(0x50, &data), Err(I2cError::InvalidData));
    }

    #[test]
    fn test_i2c_write_success() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        master.init().unwrap();
        let data = [0x01, 0x02, 0x03];
        assert!(master.write(0x50, &data).is_ok());
        assert_eq!(master.transfer_count(), 1);
    }

    #[test]
    fn test_i2c_read_success() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        master.init().unwrap();
        let mut buffer = [0u8; 4];
        assert!(master.read(0x50, &mut buffer).is_ok());
        assert_eq!(master.transfer_count(), 1);
    }

    #[test]
    fn test_i2c_write_read() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        master.init().unwrap();

        let write_data = [0x00]; // Register address
        let mut read_buffer = [0u8; 2];

        assert!(master
            .write_read(0x50, &write_data, &mut read_buffer)
            .is_ok());
        assert_eq!(master.transfer_count(), 2); // write + read
    }

    #[test]
    fn test_i2c_probe() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        master.init().unwrap();

        // Simulated: 0x50-0x57 are present
        assert!(master.probe(0x50).unwrap());
        assert!(master.probe(0x55).unwrap());
        assert!(!master.probe(0x60).unwrap());
    }

    #[test]
    fn test_i2c_scan() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        master.init().unwrap();

        let devices = master.scan().unwrap();
        // Simulated: 0x50-0x57 are present (8 devices)
        assert_eq!(devices.len(), 8);
        assert!(devices.contains(&0x50));
        assert!(devices.contains(&0x57));
    }

    #[test]
    fn test_i2c_invalid_address() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        master.init().unwrap();

        let data = [0x01];
        // Address > 0x7F in 7-bit mode
        assert_eq!(master.write(0xFF, &data), Err(I2cError::InvalidAddress));
        assert_eq!(master.error_count(), 1);
    }

    #[test]
    fn test_i2c_counter_reset() {
        let config = I2cConfig::default();
        let mut master = I2cMaster::new(0, config);
        master.init().unwrap();

        let data = [0x01];
        master.write(0x50, &data).unwrap();
        assert_eq!(master.transfer_count(), 1);

        master.reset_counters();
        assert_eq!(master.transfer_count(), 0);
        assert_eq!(master.error_count(), 0);
    }

    #[test]
    fn test_i2c_slave_creation() {
        let slave = I2cSlave::new(0, 0x50, AddressingMode::SevenBit);
        assert_eq!(slave.instance(), 0);
        assert_eq!(slave.own_address(), 0x50);
    }

    #[test]
    fn test_i2c_slave_init() {
        let mut slave = I2cSlave::new(0, 0x50, AddressingMode::SevenBit);
        assert!(slave.init().is_ok());
    }

    #[test]
    fn test_i2c_slave_invalid_address() {
        let mut slave = I2cSlave::new(0, 0xFF, AddressingMode::SevenBit);
        assert_eq!(slave.init(), Err(I2cError::InvalidAddress));
    }

    #[test]
    fn test_i2c_slave_transmit_receive() {
        let mut slave = I2cSlave::new(0, 0x50, AddressingMode::SevenBit);
        slave.init().unwrap();

        // Set transmit data
        let tx_data = [0x01, 0x02, 0x03];
        slave.set_transmit_data(&tx_data);

        // Clear receive buffer
        slave.clear_receive_buffer();
        assert_eq!(slave.get_received_data().len(), 0);
    }

    #[test]
    fn test_transaction_write() {
        let mut buffer = [1, 2, 3];
        let transaction = I2cTransaction::write(&mut buffer);
        assert_eq!(transaction.transaction_type(), TransactionType::Write);
        assert_eq!(transaction.buffer(), &[1, 2, 3]);
    }

    #[test]
    fn test_transaction_read() {
        let mut buffer = [0u8; 4];
        let mut transaction = I2cTransaction::read(&mut buffer);
        assert_eq!(transaction.transaction_type(), TransactionType::Read);

        // Modify buffer through transaction
        transaction.buffer_mut()[0] = 42;
        assert_eq!(buffer[0], 42);
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", I2cError::NotInitialized),
            "I2C not initialized"
        );
        assert_eq!(format!("{}", I2cError::Nack), "NACK received from device");
        assert_eq!(format!("{}", I2cError::Timeout), "operation timeout");
    }
}
