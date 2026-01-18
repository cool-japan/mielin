//! UART (Universal Asynchronous Receiver/Transmitter) Driver
//!
//! Provides a hardware abstraction layer for UART serial communication.
//! Essential for debugging, console access, and serial protocols.
//!
//! ## Overview
//!
//! The UART module provides:
//! - Asynchronous serial communication
//! - Configurable baud rates (up to 10.5 Mbps typical)
//! - Data bits (5, 6, 7, 8, 9)
//! - Stop bits (1, 1.5, 2)
//! - Parity (none, even, odd, mark, space)
//! - Hardware flow control (RTS/CTS)
//! - FIFO buffers for TX and RX
//! - Interrupt and polling modes
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::uart::{Uart, UartConfig, Parity};
//!
//! // Configure UART at 115200 baud, 8N1
//! let config = UartConfig::default()
//!     .with_baud_rate(115200)
//!     .with_parity(Parity::None);
//!
//! let mut uart = Uart::new(0, config);
//! uart.init().unwrap();
//!
//! // Send data
//! let message = b"Hello, World!\r\n";
//! uart.write(message).unwrap();
//!
//! // Receive data
//! let mut buffer = [0u8; 32];
//! let count = uart.read(&mut buffer).unwrap();
//! ```

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// =============================================================================
// UART Configuration
// =============================================================================

/// Data bits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataBits {
    /// 5 data bits
    Bits5,
    /// 6 data bits
    Bits6,
    /// 7 data bits
    Bits7,
    /// 8 data bits (most common)
    Bits8,
    /// 9 data bits
    Bits9,
}

/// Stop bits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBits {
    /// 1 stop bit (most common)
    One,
    /// 1.5 stop bits
    OnePointFive,
    /// 2 stop bits
    Two,
}

/// Parity mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    /// No parity (most common)
    None,
    /// Even parity
    Even,
    /// Odd parity
    Odd,
    /// Mark parity (always 1)
    Mark,
    /// Space parity (always 0)
    Space,
}

/// Hardware flow control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowControl {
    /// No flow control
    None,
    /// RTS/CTS hardware flow control
    RtsCts,
}

/// UART configuration
#[derive(Debug, Clone, Copy)]
pub struct UartConfig {
    /// Baud rate (bits per second)
    baud_rate: u32,
    /// Data bits
    data_bits: DataBits,
    /// Stop bits
    stop_bits: StopBits,
    /// Parity
    parity: Parity,
    /// Flow control
    flow_control: FlowControl,
    /// Enable TX FIFO
    tx_fifo_enabled: bool,
    /// Enable RX FIFO
    rx_fifo_enabled: bool,
    /// RX timeout in bit periods (0 = disabled)
    rx_timeout: u16,
}

impl UartConfig {
    /// Create a new UART configuration
    pub fn new() -> Self {
        Self {
            baud_rate: 115200,
            data_bits: DataBits::Bits8,
            stop_bits: StopBits::One,
            parity: Parity::None,
            flow_control: FlowControl::None,
            tx_fifo_enabled: true,
            rx_fifo_enabled: true,
            rx_timeout: 0,
        }
    }

    /// Set baud rate
    pub fn with_baud_rate(mut self, baud_rate: u32) -> Self {
        self.baud_rate = baud_rate;
        self
    }

    /// Set data bits
    pub fn with_data_bits(mut self, data_bits: DataBits) -> Self {
        self.data_bits = data_bits;
        self
    }

    /// Set stop bits
    pub fn with_stop_bits(mut self, stop_bits: StopBits) -> Self {
        self.stop_bits = stop_bits;
        self
    }

    /// Set parity
    pub fn with_parity(mut self, parity: Parity) -> Self {
        self.parity = parity;
        self
    }

    /// Set flow control
    pub fn with_flow_control(mut self, flow_control: FlowControl) -> Self {
        self.flow_control = flow_control;
        self
    }

    /// Enable/disable TX FIFO
    pub fn with_tx_fifo(mut self, enabled: bool) -> Self {
        self.tx_fifo_enabled = enabled;
        self
    }

    /// Enable/disable RX FIFO
    pub fn with_rx_fifo(mut self, enabled: bool) -> Self {
        self.rx_fifo_enabled = enabled;
        self
    }

    /// Set RX timeout in bit periods
    pub fn with_rx_timeout(mut self, timeout: u16) -> Self {
        self.rx_timeout = timeout;
        self
    }

    /// Get baud rate
    pub fn baud_rate(&self) -> u32 {
        self.baud_rate
    }

    /// Get data bits
    pub fn data_bits(&self) -> DataBits {
        self.data_bits
    }

    /// Get stop bits
    pub fn stop_bits(&self) -> StopBits {
        self.stop_bits
    }

    /// Get parity
    pub fn parity(&self) -> Parity {
        self.parity
    }
}

impl Default for UartConfig {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// UART Controller
// =============================================================================

/// UART controller
pub struct Uart {
    /// Instance number (e.g., UART0, UART1, etc.)
    instance: u8,

    /// Configuration
    #[allow(dead_code)]
    config: UartConfig,

    /// Initialized flag
    initialized: AtomicBool,

    /// TX buffer (software FIFO)
    #[allow(dead_code)]
    tx_buffer: Vec<u8>,

    /// RX buffer (software FIFO)
    rx_buffer: Vec<u8>,

    /// Bytes transmitted
    tx_count: AtomicU32,

    /// Bytes received
    rx_count: AtomicU32,

    /// Frame errors
    frame_errors: AtomicU32,

    /// Parity errors
    parity_errors: AtomicU32,

    /// Overrun errors
    overrun_errors: AtomicU32,
}

impl Uart {
    /// Create a new UART instance
    pub fn new(instance: u8, config: UartConfig) -> Self {
        Self {
            instance,
            config,
            initialized: AtomicBool::new(false),
            tx_buffer: Vec::new(),
            rx_buffer: Vec::new(),
            tx_count: AtomicU32::new(0),
            rx_count: AtomicU32::new(0),
            frame_errors: AtomicU32::new(0),
            parity_errors: AtomicU32::new(0),
            overrun_errors: AtomicU32::new(0),
        }
    }

    /// Get instance number
    pub fn instance(&self) -> u8 {
        self.instance
    }

    /// Initialize UART
    pub fn init(&mut self) -> Result<(), UartError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        // In a real implementation, this would:
        // 1. Enable UART peripheral clock
        // 2. Configure GPIO pins (TX, RX, optionally RTS/CTS)
        // 3. Calculate and set baud rate divisor
        // 4. Configure data bits, stop bits, parity
        // 5. Enable TX and RX
        // 6. Configure and enable FIFOs
        // 7. Enable interrupts if needed

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Write data (blocking)
    pub fn write(&mut self, data: &[u8]) -> Result<usize, UartError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(UartError::NotInitialized);
        }

        if data.is_empty() {
            return Ok(0);
        }

        // In a real implementation, this would:
        // 1. For each byte:
        //    - Wait for TX register empty
        //    - Write byte to TX register
        // 2. Or use DMA for large transfers

        let count = data.len();
        self.tx_count.fetch_add(count as u32, Ordering::Relaxed);

        Ok(count)
    }

    /// Read data (non-blocking)
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, UartError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(UartError::NotInitialized);
        }

        if buffer.is_empty() {
            return Ok(0);
        }

        // In a real implementation, this would:
        // 1. Check RX data available flag
        // 2. Read bytes from RX register/FIFO
        // 3. Check for errors (frame, parity, overrun)

        // Simulate: read from software buffer
        let available = self.rx_buffer.len();
        if available == 0 {
            return Ok(0);
        }

        let to_read = available.min(buffer.len());
        buffer[..to_read].copy_from_slice(&self.rx_buffer[..to_read]);
        self.rx_buffer.drain(..to_read);

        self.rx_count.fetch_add(to_read as u32, Ordering::Relaxed);

        Ok(to_read)
    }

    /// Write a single byte
    pub fn write_byte(&mut self, byte: u8) -> Result<(), UartError> {
        self.write(&[byte])?;
        Ok(())
    }

    /// Read a single byte (blocking with timeout)
    pub fn read_byte(&mut self, timeout_ms: u32) -> Result<u8, UartError> {
        let _buffer = [0u8; 1];

        // In a real implementation, this would:
        // 1. Wait for RX data with timeout
        // 2. Read byte
        // 3. Return timeout error if expired

        let _ = timeout_ms; // Suppress unused warning
        if self.rx_buffer.is_empty() {
            return Err(UartError::Timeout);
        }

        let byte = self.rx_buffer.remove(0);
        self.rx_count.fetch_add(1, Ordering::Relaxed);

        Ok(byte)
    }

    /// Check if data is available to read
    pub fn available(&self) -> usize {
        // In a real implementation, this would check RX FIFO level
        self.rx_buffer.len()
    }

    /// Flush TX buffer (wait for transmission complete)
    pub fn flush(&mut self) -> Result<(), UartError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(UartError::NotInitialized);
        }

        // In a real implementation, this would:
        // 1. Wait for TX buffer empty flag
        // 2. Wait for transmission complete flag

        Ok(())
    }

    /// Simulate receiving data (for testing)
    #[cfg(test)]
    pub fn simulate_receive(&mut self, data: &[u8]) {
        self.rx_buffer.extend_from_slice(data);
    }

    /// Get TX count
    pub fn tx_count(&self) -> u32 {
        self.tx_count.load(Ordering::Relaxed)
    }

    /// Get RX count
    pub fn rx_count(&self) -> u32 {
        self.rx_count.load(Ordering::Relaxed)
    }

    /// Get frame error count
    pub fn frame_errors(&self) -> u32 {
        self.frame_errors.load(Ordering::Relaxed)
    }

    /// Get parity error count
    pub fn parity_errors(&self) -> u32 {
        self.parity_errors.load(Ordering::Relaxed)
    }

    /// Get overrun error count
    pub fn overrun_errors(&self) -> u32 {
        self.overrun_errors.load(Ordering::Relaxed)
    }

    /// Reset error counters
    pub fn reset_error_counters(&mut self) {
        self.frame_errors.store(0, Ordering::Relaxed);
        self.parity_errors.store(0, Ordering::Relaxed);
        self.overrun_errors.store(0, Ordering::Relaxed);
    }
}

// =============================================================================
// Errors
// =============================================================================

/// UART error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UartError {
    /// UART not initialized
    NotInitialized,
    /// Frame error (invalid stop bit)
    FrameError,
    /// Parity error
    ParityError,
    /// Overrun error (data lost)
    Overrun,
    /// Timeout
    Timeout,
    /// Buffer overflow
    BufferOverflow,
    /// Hardware error
    HardwareError,
}

impl core::fmt::Display for UartError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "UART not initialized"),
            Self::FrameError => write!(f, "frame error"),
            Self::ParityError => write!(f, "parity error"),
            Self::Overrun => write!(f, "overrun error - data lost"),
            Self::Timeout => write!(f, "timeout"),
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
    fn test_uart_config_default() {
        let config = UartConfig::default();
        assert_eq!(config.baud_rate(), 115200);
        assert_eq!(config.data_bits(), DataBits::Bits8);
        assert_eq!(config.stop_bits(), StopBits::One);
        assert_eq!(config.parity(), Parity::None);
    }

    #[test]
    fn test_uart_config_builder() {
        let config = UartConfig::default()
            .with_baud_rate(9600)
            .with_data_bits(DataBits::Bits7)
            .with_stop_bits(StopBits::Two)
            .with_parity(Parity::Even)
            .with_flow_control(FlowControl::RtsCts)
            .with_tx_fifo(false)
            .with_rx_fifo(false)
            .with_rx_timeout(100);

        assert_eq!(config.baud_rate(), 9600);
        assert_eq!(config.data_bits(), DataBits::Bits7);
        assert_eq!(config.stop_bits(), StopBits::Two);
        assert_eq!(config.parity(), Parity::Even);
    }

    #[test]
    fn test_uart_creation() {
        let config = UartConfig::default();
        let uart = Uart::new(0, config);
        assert_eq!(uart.instance(), 0);
        assert_eq!(uart.tx_count(), 0);
        assert_eq!(uart.rx_count(), 0);
    }

    #[test]
    fn test_uart_init() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        assert!(uart.init().is_ok());
    }

    #[test]
    fn test_uart_write_not_initialized() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        let data = b"test";
        assert_eq!(uart.write(data), Err(UartError::NotInitialized));
    }

    #[test]
    fn test_uart_write_success() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        let data = b"Hello, World!";
        let result = uart.write(data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), data.len());
        assert_eq!(uart.tx_count(), data.len() as u32);
    }

    #[test]
    fn test_uart_write_empty() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        let data = b"";
        let result = uart.write(data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_uart_write_byte() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        assert!(uart.write_byte(0x42).is_ok());
        assert_eq!(uart.tx_count(), 1);
    }

    #[test]
    fn test_uart_read_no_data() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        let mut buffer = [0u8; 10];
        let result = uart.read(&mut buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // No data available
    }

    #[test]
    fn test_uart_read_with_data() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        // Simulate receiving data
        uart.simulate_receive(b"test");

        let mut buffer = [0u8; 10];
        let result = uart.read(&mut buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 4);
        assert_eq!(&buffer[..4], b"test");
        assert_eq!(uart.rx_count(), 4);
    }

    #[test]
    fn test_uart_read_partial() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        // Simulate receiving more data than buffer can hold
        uart.simulate_receive(b"1234567890");

        let mut buffer = [0u8; 5];
        let result = uart.read(&mut buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
        assert_eq!(&buffer, b"12345");

        // Read remaining data
        let result = uart.read(&mut buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
        assert_eq!(&buffer, b"67890");
    }

    #[test]
    fn test_uart_read_byte_no_data() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        let result = uart.read_byte(100);
        assert_eq!(result, Err(UartError::Timeout));
    }

    #[test]
    fn test_uart_read_byte_with_data() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        uart.simulate_receive(b"A");
        let result = uart.read_byte(100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b'A');
    }

    #[test]
    fn test_uart_available() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        assert_eq!(uart.available(), 0);

        uart.simulate_receive(b"test");
        assert_eq!(uart.available(), 4);

        let mut buffer = [0u8; 2];
        uart.read(&mut buffer).unwrap();
        assert_eq!(uart.available(), 2);
    }

    #[test]
    fn test_uart_flush() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        uart.write(b"test").unwrap();
        assert!(uart.flush().is_ok());
    }

    #[test]
    fn test_uart_error_counters() {
        let config = UartConfig::default();
        let mut uart = Uart::new(0, config);
        uart.init().unwrap();

        assert_eq!(uart.frame_errors(), 0);
        assert_eq!(uart.parity_errors(), 0);
        assert_eq!(uart.overrun_errors(), 0);

        uart.reset_error_counters();
        assert_eq!(uart.frame_errors(), 0);
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", UartError::NotInitialized),
            "UART not initialized"
        );
        assert_eq!(format!("{}", UartError::FrameError), "frame error");
        assert_eq!(format!("{}", UartError::ParityError), "parity error");
        assert_eq!(
            format!("{}", UartError::Overrun),
            "overrun error - data lost"
        );
    }
}
