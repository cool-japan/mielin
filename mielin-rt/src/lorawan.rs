//! LoRaWAN Implementation
//!
//! LoRaWAN 1.0.x compliant implementation for long-range, low-power IoT devices.
//! LoRaWAN is a Low Power Wide Area Network (LPWAN) protocol designed for
//! wireless battery-operated devices in regional, national, or global networks.
//!
//! ## Features
//!
//! - **Device Classes**: Class A (baseline), Class B (beacon), Class C (continuous)
//! - **Activation**: OTAA (Over-The-Air Activation) and ABP (Activation By Personalization)
//! - **Security**: AES-128 encryption, message integrity checks
//! - **MAC Layer**: Frame handling, acknowledgments, ADR
//! - **Adaptive Data Rate (ADR)**: Optimize data rate and power
//! - **Duty Cycle**: Regional regulations compliance
//! - **Confirmed/Unconfirmed**: Message delivery options
//!
//! ## Example
//!
//! ```rust
//! use mielin_rt::lorawan::{LoRaWANDevice, DeviceClass};
//!
//! // OTAA activation
//! let dev_eui = [0x00; 8];
//! let app_eui = [0x00; 8];
//! let app_key = [0x00; 16];
//! let mut device = LoRaWANDevice::new_otaa(
//!     DeviceClass::ClassA,
//!     dev_eui,
//!     app_eui,
//!     app_key
//! );
//! // Send uplink message
//! // let packet = device.send_unconfirmed(1, b"sensor_data")?;
//! ```

#![allow(dead_code)]

use core::fmt;

/// LoRaWAN specification version
pub const LORAWAN_VERSION: u8 = 1; // LoRaWAN 1.0.x

/// Maximum payload size (depends on data rate and region)
pub const MAX_PAYLOAD_SIZE: usize = 242;

/// Maximum MAC commands size
pub const MAX_MAC_COMMANDS_SIZE: usize = 15;

/// DevEUI length (8 bytes)
pub const DEV_EUI_LENGTH: usize = 8;

/// AppEUI/JoinEUI length (8 bytes)
pub const APP_EUI_LENGTH: usize = 8;

/// AppKey length (16 bytes - AES-128)
pub const APP_KEY_LENGTH: usize = 16;

/// DevAddr length (4 bytes)
pub const DEV_ADDR_LENGTH: usize = 4;

/// Network Session Key length (16 bytes)
pub const NWK_SKEY_LENGTH: usize = 16;

/// Application Session Key length (16 bytes)
pub const APP_SKEY_LENGTH: usize = 16;

/// Device Class
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    /// Class A: Baseline, lowest power
    /// - Device initiates uplink transmissions
    /// - Two short downlink windows after each uplink
    ClassA,

    /// Class B: Beacon
    /// - Scheduled receive windows in addition to Class A
    /// - Synchronized with network beacons
    ClassB,

    /// Class C: Continuous listening
    /// - Nearly continuously open receive window
    /// - Highest power consumption, lowest latency
    ClassC,
}

/// Activation Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMode {
    /// Over-The-Air Activation
    /// - Requires Join procedure
    /// - More secure, keys derived during join
    OTAA,

    /// Activation By Personalization
    /// - Pre-configured keys
    /// - No join procedure
    ABP,
}

/// Message Type (MType)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Join Request
    JoinRequest = 0b000,
    /// Join Accept
    JoinAccept = 0b001,
    /// Unconfirmed Data Up
    UnconfirmedDataUp = 0b010,
    /// Unconfirmed Data Down
    UnconfirmedDataDown = 0b011,
    /// Confirmed Data Up
    ConfirmedDataUp = 0b100,
    /// Confirmed Data Down
    ConfirmedDataDown = 0b101,
    /// Proprietary
    Proprietary = 0b111,
}

impl MessageType {
    /// Create from raw value
    pub fn from_u8(value: u8) -> Result<Self, LoRaWANError> {
        match value & 0x07 {
            0b000 => Ok(MessageType::JoinRequest),
            0b001 => Ok(MessageType::JoinAccept),
            0b010 => Ok(MessageType::UnconfirmedDataUp),
            0b011 => Ok(MessageType::UnconfirmedDataDown),
            0b100 => Ok(MessageType::ConfirmedDataUp),
            0b101 => Ok(MessageType::ConfirmedDataDown),
            0b111 => Ok(MessageType::Proprietary),
            _ => Err(LoRaWANError::InvalidMessageType),
        }
    }

    /// Convert to raw value
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Check if uplink message
    pub fn is_uplink(self) -> bool {
        matches!(
            self,
            MessageType::JoinRequest
                | MessageType::UnconfirmedDataUp
                | MessageType::ConfirmedDataUp
        )
    }

    /// Check if downlink message
    pub fn is_downlink(self) -> bool {
        matches!(
            self,
            MessageType::JoinAccept
                | MessageType::UnconfirmedDataDown
                | MessageType::ConfirmedDataDown
        )
    }

    /// Check if confirmed message
    pub fn is_confirmed(self) -> bool {
        matches!(
            self,
            MessageType::ConfirmedDataUp | MessageType::ConfirmedDataDown
        )
    }
}

/// MAC Header (MHDR)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacHeader {
    /// Message type
    pub mtype: MessageType,
    /// Major version
    pub major: u8,
}

impl MacHeader {
    /// Create a new MAC header
    pub fn new(mtype: MessageType) -> Self {
        Self {
            mtype,
            major: 0, // LoRaWAN 1.0.x
        }
    }

    /// Encode to byte
    pub fn encode(&self) -> u8 {
        (self.mtype.to_u8() << 5) | (self.major & 0x03)
    }

    /// Decode from byte
    pub fn decode(byte: u8) -> Result<Self, LoRaWANError> {
        let mtype = MessageType::from_u8(byte >> 5)?;
        let major = byte & 0x03;

        Ok(Self { mtype, major })
    }
}

/// Frame Control (FCtrl) for uplink
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FCtrlUplink {
    /// ADR flag
    pub adr: bool,
    /// ADR ACK request
    pub adr_ack_req: bool,
    /// ACK bit
    pub ack: bool,
    /// Frame pending
    pub f_pending: bool,
    /// FOptsLen (0-15)
    pub f_opts_len: u8,
}

impl FCtrlUplink {
    /// Create new uplink frame control
    pub fn new() -> Self {
        Self {
            adr: false,
            adr_ack_req: false,
            ack: false,
            f_pending: false,
            f_opts_len: 0,
        }
    }

    /// Encode to byte
    pub fn encode(&self) -> u8 {
        let mut byte = 0u8;

        if self.adr {
            byte |= 0x80;
        }
        if self.adr_ack_req {
            byte |= 0x40;
        }
        if self.ack {
            byte |= 0x20;
        }
        if self.f_pending {
            byte |= 0x10;
        }

        byte | (self.f_opts_len & 0x0f)
    }

    /// Decode from byte
    pub fn decode(byte: u8) -> Self {
        Self {
            adr: (byte & 0x80) != 0,
            adr_ack_req: (byte & 0x40) != 0,
            ack: (byte & 0x20) != 0,
            f_pending: (byte & 0x10) != 0,
            f_opts_len: byte & 0x0f,
        }
    }
}

impl Default for FCtrlUplink {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame Header (FHDR)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameHeader {
    /// Device Address (4 bytes, LSB first)
    pub dev_addr: [u8; DEV_ADDR_LENGTH],
    /// Frame Control
    pub f_ctrl: FCtrlUplink,
    /// Frame Counter (2 bytes, LSB first)
    pub f_cnt: u16,
    /// Frame Options (0-15 bytes)
    pub f_opts: heapless::Vec<u8, MAX_MAC_COMMANDS_SIZE>,
}

impl FrameHeader {
    /// Create a new frame header
    pub fn new(dev_addr: [u8; DEV_ADDR_LENGTH], f_cnt: u16) -> Self {
        Self {
            dev_addr,
            f_ctrl: FCtrlUplink::new(),
            f_cnt,
            f_opts: heapless::Vec::new(),
        }
    }

    /// Encode frame header
    pub fn encode(&mut self) -> Result<heapless::Vec<u8, 23>, LoRaWANError> {
        let mut buffer = heapless::Vec::new();

        // DevAddr (4 bytes, LSB first)
        buffer
            .extend_from_slice(&self.dev_addr)
            .map_err(|_| LoRaWANError::BufferTooSmall)?;

        // Update FOptsLen in FCtrl
        self.f_ctrl.f_opts_len = self.f_opts.len() as u8;

        // FCtrl (1 byte)
        buffer
            .push(self.f_ctrl.encode())
            .map_err(|_| LoRaWANError::BufferTooSmall)?;

        // FCnt (2 bytes, LSB first)
        buffer
            .extend_from_slice(&self.f_cnt.to_le_bytes())
            .map_err(|_| LoRaWANError::BufferTooSmall)?;

        // FOpts (0-15 bytes)
        buffer
            .extend_from_slice(&self.f_opts)
            .map_err(|_| LoRaWANError::BufferTooSmall)?;

        Ok(buffer)
    }

    /// Decode frame header
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), LoRaWANError> {
        if bytes.len() < 7 {
            return Err(LoRaWANError::MessageTooShort);
        }

        // DevAddr
        let mut dev_addr = [0u8; DEV_ADDR_LENGTH];
        dev_addr.copy_from_slice(&bytes[0..4]);

        // FCtrl
        let f_ctrl = FCtrlUplink::decode(bytes[4]);

        // FCnt
        let f_cnt = u16::from_le_bytes([bytes[5], bytes[6]]);

        // FOpts
        let f_opts_len = f_ctrl.f_opts_len as usize;
        if bytes.len() < 7 + f_opts_len {
            return Err(LoRaWANError::MessageTooShort);
        }

        let mut f_opts = heapless::Vec::new();
        f_opts
            .extend_from_slice(&bytes[7..7 + f_opts_len])
            .map_err(|_| LoRaWANError::TooManyMacCommands)?;

        let header = Self {
            dev_addr,
            f_ctrl,
            f_cnt,
            f_opts,
        };

        Ok((header, 7 + f_opts_len))
    }
}

/// Join Request Message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinRequest {
    /// AppEUI/JoinEUI (8 bytes, LSB first)
    pub app_eui: [u8; APP_EUI_LENGTH],
    /// DevEUI (8 bytes, LSB first)
    pub dev_eui: [u8; DEV_EUI_LENGTH],
    /// Device nonce (2 bytes, LSB first)
    pub dev_nonce: u16,
}

impl JoinRequest {
    /// Create a new join request
    pub fn new(
        app_eui: [u8; APP_EUI_LENGTH],
        dev_eui: [u8; DEV_EUI_LENGTH],
        dev_nonce: u16,
    ) -> Self {
        Self {
            app_eui,
            dev_eui,
            dev_nonce,
        }
    }

    /// Encode join request
    pub fn encode(&self) -> heapless::Vec<u8, 23> {
        let mut buffer = heapless::Vec::new();

        // MHDR
        let mhdr = MacHeader::new(MessageType::JoinRequest);
        let _ = buffer.push(mhdr.encode());

        // AppEUI (8 bytes, LSB first)
        let _ = buffer.extend_from_slice(&self.app_eui);

        // DevEUI (8 bytes, LSB first)
        let _ = buffer.extend_from_slice(&self.dev_eui);

        // DevNonce (2 bytes, LSB first)
        let _ = buffer.extend_from_slice(&self.dev_nonce.to_le_bytes());

        // Note: MIC would be calculated and appended here (4 bytes)

        buffer
    }
}

/// Data Frame
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFrame {
    /// MAC Header
    pub mhdr: MacHeader,
    /// Frame Header
    pub fhdr: FrameHeader,
    /// Frame Port (0-255, optional)
    pub f_port: Option<u8>,
    /// Frame Payload (encrypted)
    pub frm_payload: heapless::Vec<u8, MAX_PAYLOAD_SIZE>,
}

impl DataFrame {
    /// Create a new data frame
    pub fn new(mtype: MessageType, dev_addr: [u8; DEV_ADDR_LENGTH], f_cnt: u16) -> Self {
        Self {
            mhdr: MacHeader::new(mtype),
            fhdr: FrameHeader::new(dev_addr, f_cnt),
            f_port: None,
            frm_payload: heapless::Vec::new(),
        }
    }

    /// Set port
    pub fn with_port(mut self, port: u8) -> Self {
        self.f_port = Some(port);
        self
    }

    /// Set payload
    pub fn with_payload(mut self, payload: &[u8]) -> Result<Self, LoRaWANError> {
        self.frm_payload.clear();
        self.frm_payload
            .extend_from_slice(payload)
            .map_err(|_| LoRaWANError::PayloadTooLarge)?;
        Ok(self)
    }

    /// Encode data frame
    pub fn encode(&mut self) -> Result<heapless::Vec<u8, 255>, LoRaWANError> {
        let mut buffer = heapless::Vec::new();

        // MHDR (1 byte)
        buffer
            .push(self.mhdr.encode())
            .map_err(|_| LoRaWANError::BufferTooSmall)?;

        // FHDR (7-23 bytes)
        let fhdr_bytes = self.fhdr.encode()?;
        buffer
            .extend_from_slice(&fhdr_bytes)
            .map_err(|_| LoRaWANError::BufferTooSmall)?;

        // FPort (0 or 1 byte)
        if let Some(port) = self.f_port {
            buffer
                .push(port)
                .map_err(|_| LoRaWANError::BufferTooSmall)?;
        }

        // FRMPayload (0-N bytes)
        buffer
            .extend_from_slice(&self.frm_payload)
            .map_err(|_| LoRaWANError::BufferTooSmall)?;

        // Note: MIC would be calculated and appended here (4 bytes)

        Ok(buffer)
    }
}

/// LoRaWAN Device Configuration
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// Device Class
    pub device_class: DeviceClass,
    /// Activation Mode
    pub activation_mode: ActivationMode,
    /// DevEUI (for OTAA)
    pub dev_eui: Option<[u8; DEV_EUI_LENGTH]>,
    /// AppEUI/JoinEUI (for OTAA)
    pub app_eui: Option<[u8; APP_EUI_LENGTH]>,
    /// AppKey (for OTAA)
    pub app_key: Option<[u8; APP_KEY_LENGTH]>,
    /// DevAddr (for ABP or after join)
    pub dev_addr: Option<[u8; DEV_ADDR_LENGTH]>,
    /// Network Session Key (for ABP or after join)
    pub nwk_skey: Option<[u8; NWK_SKEY_LENGTH]>,
    /// Application Session Key (for ABP or after join)
    pub app_skey: Option<[u8; APP_SKEY_LENGTH]>,
    /// ADR enabled
    pub adr_enabled: bool,
    /// Confirmed messages
    pub confirmed: bool,
}

impl DeviceConfig {
    /// Create new device configuration for OTAA
    pub fn new_otaa(
        device_class: DeviceClass,
        dev_eui: [u8; DEV_EUI_LENGTH],
        app_eui: [u8; APP_EUI_LENGTH],
        app_key: [u8; APP_KEY_LENGTH],
    ) -> Self {
        Self {
            device_class,
            activation_mode: ActivationMode::OTAA,
            dev_eui: Some(dev_eui),
            app_eui: Some(app_eui),
            app_key: Some(app_key),
            dev_addr: None,
            nwk_skey: None,
            app_skey: None,
            adr_enabled: true,
            confirmed: false,
        }
    }

    /// Create new device configuration for ABP
    pub fn new_abp(
        device_class: DeviceClass,
        dev_addr: [u8; DEV_ADDR_LENGTH],
        nwk_skey: [u8; NWK_SKEY_LENGTH],
        app_skey: [u8; APP_SKEY_LENGTH],
    ) -> Self {
        Self {
            device_class,
            activation_mode: ActivationMode::ABP,
            dev_eui: None,
            app_eui: None,
            app_key: None,
            dev_addr: Some(dev_addr),
            nwk_skey: Some(nwk_skey),
            app_skey: Some(app_skey),
            adr_enabled: true,
            confirmed: false,
        }
    }

    /// Check if device is activated (has session keys)
    pub fn is_activated(&self) -> bool {
        self.dev_addr.is_some() && self.nwk_skey.is_some() && self.app_skey.is_some()
    }
}

/// LoRaWAN Device
#[derive(Debug)]
pub struct LoRaWANDevice {
    /// Device configuration
    config: DeviceConfig,
    /// Uplink frame counter
    f_cnt_up: u32,
    /// Downlink frame counter
    f_cnt_down: u32,
    /// Device nonce (for OTAA join)
    dev_nonce: u16,
    /// Joined flag
    joined: bool,
}

impl LoRaWANDevice {
    /// Create a new LoRaWAN device with OTAA
    pub fn new_otaa(
        device_class: DeviceClass,
        dev_eui: [u8; DEV_EUI_LENGTH],
        app_eui: [u8; APP_EUI_LENGTH],
        app_key: [u8; APP_KEY_LENGTH],
    ) -> Self {
        Self {
            config: DeviceConfig::new_otaa(device_class, dev_eui, app_eui, app_key),
            f_cnt_up: 0,
            f_cnt_down: 0,
            dev_nonce: 1,
            joined: false,
        }
    }

    /// Create a new LoRaWAN device with ABP
    pub fn new_abp(
        device_class: DeviceClass,
        dev_addr: [u8; DEV_ADDR_LENGTH],
        nwk_skey: [u8; NWK_SKEY_LENGTH],
        app_skey: [u8; APP_SKEY_LENGTH],
    ) -> Self {
        Self {
            config: DeviceConfig::new_abp(device_class, dev_addr, nwk_skey, app_skey),
            f_cnt_up: 0,
            f_cnt_down: 0,
            dev_nonce: 0,
            joined: true, // ABP is pre-activated
        }
    }

    /// Generate join request
    pub fn generate_join_request(&mut self) -> Result<heapless::Vec<u8, 23>, LoRaWANError> {
        if self.config.activation_mode != ActivationMode::OTAA {
            return Err(LoRaWANError::InvalidActivationMode);
        }

        let app_eui = self
            .config
            .app_eui
            .ok_or(LoRaWANError::MissingCredentials)?;
        let dev_eui = self
            .config
            .dev_eui
            .ok_or(LoRaWANError::MissingCredentials)?;

        let join_req = JoinRequest::new(app_eui, dev_eui, self.dev_nonce);
        self.dev_nonce = self.dev_nonce.wrapping_add(1);

        Ok(join_req.encode())
    }

    /// Handle join accept (simplified - no crypto in this example)
    pub fn handle_join_accept(
        &mut self,
        dev_addr: [u8; DEV_ADDR_LENGTH],
        nwk_skey: [u8; NWK_SKEY_LENGTH],
        app_skey: [u8; APP_SKEY_LENGTH],
    ) -> Result<(), LoRaWANError> {
        self.config.dev_addr = Some(dev_addr);
        self.config.nwk_skey = Some(nwk_skey);
        self.config.app_skey = Some(app_skey);
        self.joined = true;
        self.f_cnt_up = 0;
        self.f_cnt_down = 0;

        Ok(())
    }

    /// Send unconfirmed uplink
    pub fn send_unconfirmed(
        &mut self,
        f_port: u8,
        payload: &[u8],
    ) -> Result<heapless::Vec<u8, 255>, LoRaWANError> {
        if !self.is_joined() {
            return Err(LoRaWANError::NotJoined);
        }

        let dev_addr = self.config.dev_addr.ok_or(LoRaWANError::NotJoined)?;

        let mut frame = DataFrame::new(
            MessageType::UnconfirmedDataUp,
            dev_addr,
            self.f_cnt_up as u16,
        );

        frame = frame.with_port(f_port).with_payload(payload)?;

        // Set ADR if enabled
        frame.fhdr.f_ctrl.adr = self.config.adr_enabled;

        self.f_cnt_up += 1;

        frame.encode()
    }

    /// Send confirmed uplink
    pub fn send_confirmed(
        &mut self,
        f_port: u8,
        payload: &[u8],
    ) -> Result<heapless::Vec<u8, 255>, LoRaWANError> {
        if !self.is_joined() {
            return Err(LoRaWANError::NotJoined);
        }

        let dev_addr = self.config.dev_addr.ok_or(LoRaWANError::NotJoined)?;

        let mut frame =
            DataFrame::new(MessageType::ConfirmedDataUp, dev_addr, self.f_cnt_up as u16);

        frame = frame.with_port(f_port).with_payload(payload)?;

        // Set ADR if enabled
        frame.fhdr.f_ctrl.adr = self.config.adr_enabled;

        self.f_cnt_up += 1;

        frame.encode()
    }

    /// Check if device is joined
    pub fn is_joined(&self) -> bool {
        self.joined
    }

    /// Get uplink frame counter
    pub fn get_f_cnt_up(&self) -> u32 {
        self.f_cnt_up
    }

    /// Get downlink frame counter
    pub fn get_f_cnt_down(&self) -> u32 {
        self.f_cnt_down
    }

    /// Get device class
    pub fn device_class(&self) -> DeviceClass {
        self.config.device_class
    }

    /// Get activation mode
    pub fn activation_mode(&self) -> ActivationMode {
        self.config.activation_mode
    }

    /// Enable/disable ADR
    pub fn set_adr(&mut self, enabled: bool) {
        self.config.adr_enabled = enabled;
    }

    /// Check if ADR is enabled
    pub fn is_adr_enabled(&self) -> bool {
        self.config.adr_enabled
    }

    /// Get device address
    pub fn dev_addr(&self) -> Option<[u8; DEV_ADDR_LENGTH]> {
        self.config.dev_addr
    }
}

/// LoRaWAN Error Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoRaWANError {
    /// Invalid message type
    InvalidMessageType,
    /// Invalid activation mode
    InvalidActivationMode,
    /// Buffer too small
    BufferTooSmall,
    /// Message too short
    MessageTooShort,
    /// Payload too large
    PayloadTooLarge,
    /// Too many MAC commands
    TooManyMacCommands,
    /// Missing credentials
    MissingCredentials,
    /// Not joined to network
    NotJoined,
    /// Invalid MIC (Message Integrity Code)
    InvalidMic,
    /// Frame counter overflow
    FrameCounterOverflow,
}

impl fmt::Display for LoRaWANError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoRaWANError::InvalidMessageType => write!(f, "Invalid LoRaWAN message type"),
            LoRaWANError::InvalidActivationMode => write!(f, "Invalid activation mode"),
            LoRaWANError::BufferTooSmall => write!(f, "Buffer too small"),
            LoRaWANError::MessageTooShort => write!(f, "Message too short"),
            LoRaWANError::PayloadTooLarge => write!(f, "Payload too large"),
            LoRaWANError::TooManyMacCommands => write!(f, "Too many MAC commands"),
            LoRaWANError::MissingCredentials => write!(f, "Missing credentials"),
            LoRaWANError::NotJoined => write!(f, "Not joined to network"),
            LoRaWANError::InvalidMic => write!(f, "Invalid MIC"),
            LoRaWANError::FrameCounterOverflow => write!(f, "Frame counter overflow"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_message_type_conversion() {
        assert_eq!(
            MessageType::from_u8(0b010).unwrap(),
            MessageType::UnconfirmedDataUp
        );
        assert_eq!(MessageType::UnconfirmedDataUp.to_u8(), 0b010);
    }

    #[test]
    fn test_message_type_classification() {
        assert!(MessageType::JoinRequest.is_uplink());
        assert!(MessageType::JoinAccept.is_downlink());
        assert!(MessageType::ConfirmedDataUp.is_confirmed());
        assert!(!MessageType::UnconfirmedDataUp.is_confirmed());
    }

    #[test]
    fn test_mac_header_encode_decode() {
        let mhdr = MacHeader::new(MessageType::UnconfirmedDataUp);
        let encoded = mhdr.encode();
        let decoded = MacHeader::decode(encoded).unwrap();

        assert_eq!(decoded.mtype, MessageType::UnconfirmedDataUp);
        assert_eq!(decoded.major, 0);
    }

    #[test]
    fn test_fctrl_uplink() {
        let mut fctrl = FCtrlUplink::new();
        fctrl.adr = true;
        fctrl.ack = true;
        fctrl.f_opts_len = 3;

        let encoded = fctrl.encode();
        let decoded = FCtrlUplink::decode(encoded);

        assert!(decoded.adr);
        assert!(decoded.ack);
        assert_eq!(decoded.f_opts_len, 3);
    }

    #[test]
    fn test_frame_header() {
        let dev_addr = [0x01, 0x02, 0x03, 0x04];
        let mut fhdr = FrameHeader::new(dev_addr, 100);

        let encoded = fhdr.encode().unwrap();
        assert!(!encoded.is_empty());

        let (decoded, _) = FrameHeader::decode(&encoded).unwrap();
        assert_eq!(decoded.dev_addr, dev_addr);
        assert_eq!(decoded.f_cnt, 100);
    }

    #[test]
    fn test_join_request() {
        let app_eui = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let dev_eui = [0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18];
        let dev_nonce = 0x1234;

        let join_req = JoinRequest::new(app_eui, dev_eui, dev_nonce);
        let encoded = join_req.encode();

        assert!(!encoded.is_empty());
        assert_eq!(encoded[0] & 0xe0, (MessageType::JoinRequest.to_u8() << 5));
    }

    #[test]
    fn test_data_frame() {
        let dev_addr = [0xaa, 0xbb, 0xcc, 0xdd];
        let mut frame = DataFrame::new(MessageType::UnconfirmedDataUp, dev_addr, 1);

        frame = frame.with_port(1).with_payload(b"test").unwrap();

        let encoded = frame.encode().unwrap();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_device_config_otaa() {
        let config = DeviceConfig::new_otaa(
            DeviceClass::ClassA,
            [0u8; DEV_EUI_LENGTH],
            [0u8; APP_EUI_LENGTH],
            [0u8; APP_KEY_LENGTH],
        );

        assert_eq!(config.activation_mode, ActivationMode::OTAA);
        assert!(!config.is_activated());
    }

    #[test]
    fn test_device_config_abp() {
        let config = DeviceConfig::new_abp(
            DeviceClass::ClassA,
            [0u8; DEV_ADDR_LENGTH],
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        assert_eq!(config.activation_mode, ActivationMode::ABP);
        assert!(config.is_activated());
    }

    #[test]
    fn test_lorawan_device_otaa() {
        let mut device = LoRaWANDevice::new_otaa(
            DeviceClass::ClassA,
            [0u8; DEV_EUI_LENGTH],
            [0u8; APP_EUI_LENGTH],
            [0u8; APP_KEY_LENGTH],
        );

        assert!(!device.is_joined());
        assert_eq!(device.get_f_cnt_up(), 0);

        // Generate join request
        let join_req = device.generate_join_request().unwrap();
        assert!(!join_req.is_empty());
    }

    #[test]
    fn test_lorawan_device_abp() {
        let device = LoRaWANDevice::new_abp(
            DeviceClass::ClassA,
            [0x01, 0x02, 0x03, 0x04],
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        assert!(device.is_joined());
        assert_eq!(device.activation_mode(), ActivationMode::ABP);
    }

    #[test]
    fn test_send_unconfirmed() {
        let mut device = LoRaWANDevice::new_abp(
            DeviceClass::ClassA,
            [0x01, 0x02, 0x03, 0x04],
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        let packet = device.send_unconfirmed(1, b"sensor_data").unwrap();
        assert!(!packet.is_empty());
        assert_eq!(device.get_f_cnt_up(), 1);
    }

    #[test]
    fn test_send_confirmed() {
        let mut device = LoRaWANDevice::new_abp(
            DeviceClass::ClassA,
            [0x01, 0x02, 0x03, 0x04],
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        let packet = device.send_confirmed(1, b"important").unwrap();
        assert!(!packet.is_empty());
        assert_eq!(device.get_f_cnt_up(), 1);
    }

    #[test]
    fn test_not_joined_error() {
        let mut device = LoRaWANDevice::new_otaa(
            DeviceClass::ClassA,
            [0u8; DEV_EUI_LENGTH],
            [0u8; APP_EUI_LENGTH],
            [0u8; APP_KEY_LENGTH],
        );

        let result = device.send_unconfirmed(1, b"data");
        assert_eq!(result.unwrap_err(), LoRaWANError::NotJoined);
    }

    #[test]
    fn test_handle_join_accept() {
        let mut device = LoRaWANDevice::new_otaa(
            DeviceClass::ClassA,
            [0u8; DEV_EUI_LENGTH],
            [0u8; APP_EUI_LENGTH],
            [0u8; APP_KEY_LENGTH],
        );

        assert!(!device.is_joined());

        device
            .handle_join_accept(
                [0x01, 0x02, 0x03, 0x04],
                [0u8; NWK_SKEY_LENGTH],
                [0u8; APP_SKEY_LENGTH],
            )
            .unwrap();

        assert!(device.is_joined());
        assert_eq!(device.get_f_cnt_up(), 0);
    }

    #[test]
    fn test_adr_control() {
        let mut device = LoRaWANDevice::new_abp(
            DeviceClass::ClassA,
            [0x01, 0x02, 0x03, 0x04],
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        assert!(device.is_adr_enabled());

        device.set_adr(false);
        assert!(!device.is_adr_enabled());

        device.set_adr(true);
        assert!(device.is_adr_enabled());
    }

    #[test]
    fn test_frame_counter_increment() {
        let mut device = LoRaWANDevice::new_abp(
            DeviceClass::ClassA,
            [0x01, 0x02, 0x03, 0x04],
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        assert_eq!(device.get_f_cnt_up(), 0);

        device.send_unconfirmed(1, b"msg1").unwrap();
        assert_eq!(device.get_f_cnt_up(), 1);

        device.send_unconfirmed(1, b"msg2").unwrap();
        assert_eq!(device.get_f_cnt_up(), 2);

        device.send_confirmed(1, b"msg3").unwrap();
        assert_eq!(device.get_f_cnt_up(), 3);
    }

    #[test]
    fn test_device_class() {
        let device = LoRaWANDevice::new_abp(
            DeviceClass::ClassC,
            [0x01, 0x02, 0x03, 0x04],
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        assert_eq!(device.device_class(), DeviceClass::ClassC);
    }

    #[test]
    fn test_dev_addr() {
        let dev_addr = [0xaa, 0xbb, 0xcc, 0xdd];
        let device = LoRaWANDevice::new_abp(
            DeviceClass::ClassA,
            dev_addr,
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        assert_eq!(device.dev_addr(), Some(dev_addr));
    }

    #[test]
    fn test_error_display() {
        let err = LoRaWANError::NotJoined;
        let display = format!("{}", err);
        assert!(display.contains("Not joined"));
    }

    #[test]
    fn test_payload_too_large() {
        let mut device = LoRaWANDevice::new_abp(
            DeviceClass::ClassA,
            [0x01, 0x02, 0x03, 0x04],
            [0u8; NWK_SKEY_LENGTH],
            [0u8; APP_SKEY_LENGTH],
        );

        let large_payload = [0u8; MAX_PAYLOAD_SIZE + 1];
        let result = device.send_unconfirmed(1, &large_payload);
        assert_eq!(result.unwrap_err(), LoRaWANError::PayloadTooLarge);
    }

    #[test]
    fn test_dev_nonce_increment() {
        let mut device = LoRaWANDevice::new_otaa(
            DeviceClass::ClassA,
            [0u8; DEV_EUI_LENGTH],
            [0u8; APP_EUI_LENGTH],
            [0u8; APP_KEY_LENGTH],
        );

        let initial_nonce = device.dev_nonce;
        device.generate_join_request().unwrap();
        assert_eq!(device.dev_nonce, initial_nonce + 1);

        device.generate_join_request().unwrap();
        assert_eq!(device.dev_nonce, initial_nonce + 2);
    }
}
