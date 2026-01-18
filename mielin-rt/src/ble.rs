//! BLE (Bluetooth Low Energy) Implementation
//!
//! Bluetooth Low Energy support for embedded IoT devices.
//! Implements GAP (Generic Access Profile), GATT (Generic Attribute Profile),
//! and ATT (Attribute Protocol) for low-power wireless communication.
//!
//! ## Features
//!
//! - **GAP (Generic Access Profile)**: Device discovery, connection, advertising
//! - **GATT (Generic Attribute Profile)**: Services and characteristics
//! - **ATT (Attribute Protocol)**: Attribute read/write operations
//! - **Advertising**: Broadcaster and observer roles
//! - **Connection Management**: Central and peripheral roles
//! - **Security**: Pairing, bonding, encryption
//! - **Low Power**: Sleep modes between events
//!
//! ## Example
//!
//! ```rust
//! use mielin_rt::ble::{BlePeripheral, GattService, GattCharacteristic};
//!
//! let mut peripheral = BlePeripheral::new(b"MyDevice");
//! // Add services and characteristics
//! // Start advertising
//! // peripheral.start_advertising()?;
//! ```

#![allow(dead_code)]

use core::fmt;

/// BLE address length (6 bytes)
pub const BLE_ADDR_LENGTH: usize = 6;

/// Maximum advertising data length
pub const MAX_ADV_DATA_LENGTH: usize = 31;

/// Maximum attribute value length
pub const MAX_ATTR_VALUE_LENGTH: usize = 512;

/// Maximum device name length
pub const MAX_DEVICE_NAME_LENGTH: usize = 29;

/// Default advertising interval (in ms)
pub const DEFAULT_ADV_INTERVAL_MS: u16 = 100;

/// Default connection interval (in ms)
pub const DEFAULT_CONN_INTERVAL_MS: u16 = 50;

/// BLE Address Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AddressType {
    /// Public device address
    Public = 0x00,
    /// Random device address
    Random = 0x01,
    /// Public identity address (resolved from RPA)
    PublicIdentity = 0x02,
    /// Random identity address (resolved from RPA)
    RandomIdentity = 0x03,
}

impl AddressType {
    /// Create from raw value
    pub fn from_u8(value: u8) -> Result<Self, BleError> {
        match value {
            0x00 => Ok(AddressType::Public),
            0x01 => Ok(AddressType::Random),
            0x02 => Ok(AddressType::PublicIdentity),
            0x03 => Ok(AddressType::RandomIdentity),
            _ => Err(BleError::InvalidAddressType),
        }
    }

    /// Convert to raw value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// BLE Address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BleAddress {
    /// Address type
    pub addr_type: AddressType,
    /// Address bytes (LSB first)
    pub addr: [u8; BLE_ADDR_LENGTH],
}

impl BleAddress {
    /// Create a new BLE address
    pub fn new(addr_type: AddressType, addr: [u8; BLE_ADDR_LENGTH]) -> Self {
        Self { addr_type, addr }
    }

    /// Create a public address
    pub fn public(addr: [u8; BLE_ADDR_LENGTH]) -> Self {
        Self::new(AddressType::Public, addr)
    }

    /// Create a random address
    pub fn random(addr: [u8; BLE_ADDR_LENGTH]) -> Self {
        Self::new(AddressType::Random, addr)
    }
}

/// Advertising Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AdvType {
    /// Connectable and scannable undirected advertising
    AdvInd = 0x00,
    /// Connectable directed advertising (high duty cycle)
    AdvDirectInd = 0x01,
    /// Scannable undirected advertising
    AdvScanInd = 0x02,
    /// Non-connectable undirected advertising
    AdvNonconnInd = 0x03,
    /// Scan response
    ScanRsp = 0x04,
}

impl AdvType {
    /// Create from raw value
    pub fn from_u8(value: u8) -> Result<Self, BleError> {
        match value {
            0x00 => Ok(AdvType::AdvInd),
            0x01 => Ok(AdvType::AdvDirectInd),
            0x02 => Ok(AdvType::AdvScanInd),
            0x03 => Ok(AdvType::AdvNonconnInd),
            0x04 => Ok(AdvType::ScanRsp),
            _ => Err(BleError::InvalidAdvType),
        }
    }

    /// Convert to raw value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Advertising Data Type (AD Type)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AdType {
    /// Flags
    Flags = 0x01,
    /// Incomplete list of 16-bit Service UUIDs
    IncompleteList16BitUuids = 0x02,
    /// Complete list of 16-bit Service UUIDs
    CompleteList16BitUuids = 0x03,
    /// Incomplete list of 128-bit Service UUIDs
    IncompleteList128BitUuids = 0x06,
    /// Complete list of 128-bit Service UUIDs
    CompleteList128BitUuids = 0x07,
    /// Shortened local name
    ShortenedLocalName = 0x08,
    /// Complete local name
    CompleteLocalName = 0x09,
    /// TX Power Level
    TxPowerLevel = 0x0a,
    /// Manufacturer Specific Data
    ManufacturerData = 0xff,
}

impl AdType {
    /// Create from raw value
    pub fn from_u8(value: u8) -> Result<Self, BleError> {
        match value {
            0x01 => Ok(AdType::Flags),
            0x02 => Ok(AdType::IncompleteList16BitUuids),
            0x03 => Ok(AdType::CompleteList16BitUuids),
            0x06 => Ok(AdType::IncompleteList128BitUuids),
            0x07 => Ok(AdType::CompleteList128BitUuids),
            0x08 => Ok(AdType::ShortenedLocalName),
            0x09 => Ok(AdType::CompleteLocalName),
            0x0a => Ok(AdType::TxPowerLevel),
            0xff => Ok(AdType::ManufacturerData),
            _ => Err(BleError::UnknownAdType),
        }
    }

    /// Convert to raw value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Advertising Flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvFlags(pub u8);

impl AdvFlags {
    /// LE Limited Discoverable Mode
    pub const LE_LIMITED_DISCOVERABLE: u8 = 0x01;
    /// LE General Discoverable Mode
    pub const LE_GENERAL_DISCOVERABLE: u8 = 0x02;
    /// BR/EDR Not Supported
    pub const BR_EDR_NOT_SUPPORTED: u8 = 0x04;
    /// Simultaneous LE and BR/EDR Controller
    pub const LE_BR_EDR_CONTROLLER: u8 = 0x08;
    /// Simultaneous LE and BR/EDR Host
    pub const LE_BR_EDR_HOST: u8 = 0x10;

    /// Create new flags
    pub fn new() -> Self {
        Self(0)
    }

    /// Set flag
    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }

    /// Check if flag is set
    pub fn is_set(&self, flag: u8) -> bool {
        (self.0 & flag) != 0
    }
}

impl Default for AdvFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Advertising Data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisingData {
    /// Raw advertising data
    data: heapless::Vec<u8, MAX_ADV_DATA_LENGTH>,
}

impl AdvertisingData {
    /// Create new advertising data
    pub fn new() -> Self {
        Self {
            data: heapless::Vec::new(),
        }
    }

    /// Add flags
    pub fn add_flags(&mut self, flags: AdvFlags) -> Result<(), BleError> {
        self.add_element(AdType::Flags, &[flags.0])
    }

    /// Add complete local name
    pub fn add_complete_name(&mut self, name: &str) -> Result<(), BleError> {
        self.add_element(AdType::CompleteLocalName, name.as_bytes())
    }

    /// Add shortened local name
    pub fn add_shortened_name(&mut self, name: &str) -> Result<(), BleError> {
        self.add_element(AdType::ShortenedLocalName, name.as_bytes())
    }

    /// Add TX power level
    pub fn add_tx_power(&mut self, power: i8) -> Result<(), BleError> {
        self.add_element(AdType::TxPowerLevel, &[power as u8])
    }

    /// Add 16-bit service UUID
    pub fn add_service_uuid_16(&mut self, uuid: u16) -> Result<(), BleError> {
        let bytes = uuid.to_le_bytes();
        self.add_element(AdType::CompleteList16BitUuids, &bytes)
    }

    /// Add manufacturer data
    pub fn add_manufacturer_data(&mut self, company_id: u16, data: &[u8]) -> Result<(), BleError> {
        let mut payload = heapless::Vec::<u8, 29>::new();
        payload
            .extend_from_slice(&company_id.to_le_bytes())
            .map_err(|_| BleError::AdvDataTooLarge)?;
        payload
            .extend_from_slice(data)
            .map_err(|_| BleError::AdvDataTooLarge)?;

        self.add_element(AdType::ManufacturerData, &payload)
    }

    /// Add generic element
    fn add_element(&mut self, ad_type: AdType, data: &[u8]) -> Result<(), BleError> {
        let length = 1 + data.len(); // AD Type + data
        if length > 255 {
            return Err(BleError::AdvDataTooLarge);
        }

        // Check if we have space
        if self.data.len() + 1 + length > MAX_ADV_DATA_LENGTH {
            return Err(BleError::AdvDataTooLarge);
        }

        // Length byte
        self.data
            .push(length as u8)
            .map_err(|_| BleError::AdvDataTooLarge)?;

        // AD Type
        self.data
            .push(ad_type.to_u8())
            .map_err(|_| BleError::AdvDataTooLarge)?;

        // Data
        self.data
            .extend_from_slice(data)
            .map_err(|_| BleError::AdvDataTooLarge)?;

        Ok(())
    }

    /// Get raw data
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl Default for AdvertisingData {
    fn default() -> Self {
        Self::new()
    }
}

/// GATT Attribute Permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttPermissions(pub u8);

impl AttPermissions {
    /// None
    pub const NONE: u8 = 0x00;
    /// Read
    pub const READ: u8 = 0x01;
    /// Write
    pub const WRITE: u8 = 0x02;
    /// Read and Write
    pub const READ_WRITE: u8 = 0x03;
    /// Authenticated Read
    pub const READ_AUTHEN: u8 = 0x04;
    /// Authenticated Write
    pub const WRITE_AUTHEN: u8 = 0x08;
    /// Encrypted Read
    pub const READ_ENCRYPT: u8 = 0x10;
    /// Encrypted Write
    pub const WRITE_ENCRYPT: u8 = 0x20;

    /// Create new permissions
    pub fn new(perms: u8) -> Self {
        Self(perms)
    }

    /// Check if readable
    pub fn is_readable(&self) -> bool {
        (self.0 & (Self::READ | Self::READ_AUTHEN | Self::READ_ENCRYPT)) != 0
    }

    /// Check if writable
    pub fn is_writable(&self) -> bool {
        (self.0 & (Self::WRITE | Self::WRITE_AUTHEN | Self::WRITE_ENCRYPT)) != 0
    }
}

/// GATT Characteristic Properties
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharProperties(pub u8);

impl CharProperties {
    /// Broadcast
    pub const BROADCAST: u8 = 0x01;
    /// Read
    pub const READ: u8 = 0x02;
    /// Write without response
    pub const WRITE_WITHOUT_RESPONSE: u8 = 0x04;
    /// Write
    pub const WRITE: u8 = 0x08;
    /// Notify
    pub const NOTIFY: u8 = 0x10;
    /// Indicate
    pub const INDICATE: u8 = 0x20;
    /// Authenticated signed writes
    pub const AUTHENTICATED_SIGNED_WRITES: u8 = 0x40;
    /// Extended properties
    pub const EXTENDED_PROPERTIES: u8 = 0x80;

    /// Create new properties
    pub fn new(props: u8) -> Self {
        Self(props)
    }

    /// Check if property is set
    pub fn has(&self, prop: u8) -> bool {
        (self.0 & prop) != 0
    }
}

/// UUID (16-bit or 128-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uuid {
    /// 16-bit UUID
    Uuid16(u16),
    /// 128-bit UUID
    Uuid128([u8; 16]),
}

impl Uuid {
    /// Create 16-bit UUID
    pub fn new_16bit(uuid: u16) -> Self {
        Uuid::Uuid16(uuid)
    }

    /// Create 128-bit UUID
    pub fn new_128bit(uuid: [u8; 16]) -> Self {
        Uuid::Uuid128(uuid)
    }

    /// Get as 16-bit UUID if possible
    pub fn as_u16(&self) -> Option<u16> {
        match self {
            Uuid::Uuid16(uuid) => Some(*uuid),
            Uuid::Uuid128(_) => None,
        }
    }
}

/// GATT Attribute
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GattAttribute {
    /// Attribute handle
    pub handle: u16,
    /// Attribute type (UUID)
    pub attr_type: Uuid,
    /// Permissions
    pub permissions: AttPermissions,
    /// Value
    pub value: heapless::Vec<u8, MAX_ATTR_VALUE_LENGTH>,
}

impl GattAttribute {
    /// Create a new attribute
    pub fn new(handle: u16, attr_type: Uuid, permissions: AttPermissions) -> Self {
        Self {
            handle,
            attr_type,
            permissions,
            value: heapless::Vec::new(),
        }
    }

    /// Set value
    pub fn set_value(&mut self, value: &[u8]) -> Result<(), BleError> {
        self.value.clear();
        self.value
            .extend_from_slice(value)
            .map_err(|_| BleError::AttributeValueTooLarge)
    }

    /// Get value
    pub fn get_value(&self) -> &[u8] {
        &self.value
    }
}

/// GATT Characteristic
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GattCharacteristic {
    /// Characteristic UUID
    pub uuid: Uuid,
    /// Properties
    pub properties: CharProperties,
    /// Value handle
    pub value_handle: u16,
    /// Value
    pub value: heapless::Vec<u8, MAX_ATTR_VALUE_LENGTH>,
    /// Client Characteristic Configuration Descriptor (CCCD)
    pub cccd: Option<u16>,
}

impl GattCharacteristic {
    /// Create a new characteristic
    pub fn new(uuid: Uuid, properties: CharProperties, value_handle: u16) -> Self {
        Self {
            uuid,
            properties,
            value_handle,
            value: heapless::Vec::new(),
            cccd: None,
        }
    }

    /// Set value
    pub fn set_value(&mut self, value: &[u8]) -> Result<(), BleError> {
        self.value.clear();
        self.value
            .extend_from_slice(value)
            .map_err(|_| BleError::AttributeValueTooLarge)
    }

    /// Get value
    pub fn get_value(&self) -> &[u8] {
        &self.value
    }

    /// Enable notifications (set CCCD value)
    pub fn enable_notifications(&mut self, cccd_value: u16) {
        self.cccd = Some(cccd_value);
    }

    /// Check if notifications are enabled
    pub fn notifications_enabled(&self) -> bool {
        self.cccd.map(|v| (v & 0x01) != 0).unwrap_or(false)
    }

    /// Check if indications are enabled
    pub fn indications_enabled(&self) -> bool {
        self.cccd.map(|v| (v & 0x02) != 0).unwrap_or(false)
    }
}

/// GATT Service
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GattService {
    /// Service UUID
    pub uuid: Uuid,
    /// Service handle
    pub handle: u16,
    /// End handle
    pub end_handle: u16,
    /// Characteristics
    pub characteristics: heapless::Vec<GattCharacteristic, 8>,
}

impl GattService {
    /// Create a new service
    pub fn new(uuid: Uuid, handle: u16, end_handle: u16) -> Self {
        Self {
            uuid,
            handle,
            end_handle,
            characteristics: heapless::Vec::new(),
        }
    }

    /// Add characteristic
    pub fn add_characteristic(
        &mut self,
        characteristic: GattCharacteristic,
    ) -> Result<(), BleError> {
        self.characteristics
            .push(characteristic)
            .map_err(|_| BleError::TooManyCharacteristics)
    }

    /// Find characteristic by UUID
    pub fn find_characteristic(&self, uuid: &Uuid) -> Option<&GattCharacteristic> {
        self.characteristics.iter().find(|c| c.uuid == *uuid)
    }

    /// Find characteristic by handle
    pub fn find_characteristic_by_handle(&self, handle: u16) -> Option<&GattCharacteristic> {
        self.characteristics
            .iter()
            .find(|c| c.value_handle == handle)
    }

    /// Find characteristic by handle (mutable)
    pub fn find_characteristic_by_handle_mut(
        &mut self,
        handle: u16,
    ) -> Option<&mut GattCharacteristic> {
        self.characteristics
            .iter_mut()
            .find(|c| c.value_handle == handle)
    }
}

/// Connection Parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionParams {
    /// Minimum connection interval (ms)
    pub interval_min: u16,
    /// Maximum connection interval (ms)
    pub interval_max: u16,
    /// Slave latency
    pub slave_latency: u16,
    /// Connection supervision timeout (ms)
    pub supervision_timeout: u16,
}

impl ConnectionParams {
    /// Create new connection parameters
    pub fn new(
        interval_min: u16,
        interval_max: u16,
        slave_latency: u16,
        supervision_timeout: u16,
    ) -> Self {
        Self {
            interval_min,
            interval_max,
            slave_latency,
            supervision_timeout,
        }
    }

    /// Create default parameters
    pub fn default_params() -> Self {
        Self {
            interval_min: 50,  // 50ms
            interval_max: 100, // 100ms
            slave_latency: 0,
            supervision_timeout: 4000, // 4s
        }
    }
}

impl Default for ConnectionParams {
    fn default() -> Self {
        Self::default_params()
    }
}

/// BLE Connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BleConnection {
    /// Connection handle
    pub handle: u16,
    /// Peer address
    pub peer_addr: BleAddress,
    /// Connection parameters
    pub params: ConnectionParams,
    /// Connected flag
    pub connected: bool,
}

impl BleConnection {
    /// Create a new connection
    pub fn new(handle: u16, peer_addr: BleAddress, params: ConnectionParams) -> Self {
        Self {
            handle,
            peer_addr,
            params,
            connected: true,
        }
    }

    /// Disconnect
    pub fn disconnect(&mut self) {
        self.connected = false;
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

/// BLE Peripheral (Server Role)
#[derive(Debug)]
pub struct BlePeripheral {
    /// Device name
    device_name: heapless::String<MAX_DEVICE_NAME_LENGTH>,
    /// Device address
    address: BleAddress,
    /// Services
    services: heapless::Vec<GattService, 8>,
    /// Advertising data
    adv_data: AdvertisingData,
    /// Scan response data
    scan_rsp_data: AdvertisingData,
    /// Advertising enabled
    advertising: bool,
    /// Connection
    connection: Option<BleConnection>,
    /// Next attribute handle
    next_handle: u16,
}

impl BlePeripheral {
    /// Create a new BLE peripheral
    pub fn new(name: &[u8]) -> Result<Self, BleError> {
        let mut device_name = heapless::String::new();
        let name_str = core::str::from_utf8(name).map_err(|_| BleError::InvalidDeviceName)?;
        device_name
            .push_str(name_str)
            .map_err(|_| BleError::DeviceNameTooLong)?;

        // Generate random address (for simulation)
        let address = BleAddress::random([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);

        Ok(Self {
            device_name,
            address,
            services: heapless::Vec::new(),
            adv_data: AdvertisingData::new(),
            scan_rsp_data: AdvertisingData::new(),
            advertising: false,
            connection: None,
            next_handle: 1,
        })
    }

    /// Get next handle
    fn allocate_handle(&mut self) -> u16 {
        let handle = self.next_handle;
        self.next_handle += 1;
        handle
    }

    /// Add service
    pub fn add_service(&mut self, uuid: Uuid) -> Result<u16, BleError> {
        let handle = self.allocate_handle();
        let end_handle = handle; // Will be updated when characteristics are added

        let service = GattService::new(uuid, handle, end_handle);
        self.services
            .push(service)
            .map_err(|_| BleError::TooManyServices)?;

        Ok(handle)
    }

    /// Add characteristic to service
    pub fn add_characteristic(
        &mut self,
        service_handle: u16,
        uuid: Uuid,
        properties: CharProperties,
    ) -> Result<u16, BleError> {
        let value_handle = self.allocate_handle();

        let characteristic = GattCharacteristic::new(uuid, properties, value_handle);

        // Find service and add characteristic
        let service = self
            .services
            .iter_mut()
            .find(|s| s.handle == service_handle)
            .ok_or(BleError::ServiceNotFound)?;

        service.add_characteristic(characteristic)?;

        // Update service end handle
        service.end_handle = value_handle;

        Ok(value_handle)
    }

    /// Set characteristic value
    pub fn set_characteristic_value(&mut self, handle: u16, value: &[u8]) -> Result<(), BleError> {
        for service in &mut self.services {
            if let Some(characteristic) = service.find_characteristic_by_handle_mut(handle) {
                return characteristic.set_value(value);
            }
        }
        Err(BleError::CharacteristicNotFound)
    }

    /// Get characteristic value
    pub fn get_characteristic_value(&self, handle: u16) -> Result<&[u8], BleError> {
        for service in &self.services {
            if let Some(characteristic) = service.find_characteristic_by_handle(handle) {
                return Ok(characteristic.get_value());
            }
        }
        Err(BleError::CharacteristicNotFound)
    }

    /// Set advertising data
    pub fn set_advertising_data(&mut self, data: AdvertisingData) {
        self.adv_data = data;
    }

    /// Set scan response data
    pub fn set_scan_response_data(&mut self, data: AdvertisingData) {
        self.scan_rsp_data = data;
    }

    /// Start advertising
    pub fn start_advertising(&mut self) -> Result<(), BleError> {
        // Build advertising data with device name
        let mut adv_data = AdvertisingData::new();

        // Add flags
        let mut flags = AdvFlags::new();
        flags.set(AdvFlags::LE_GENERAL_DISCOVERABLE);
        flags.set(AdvFlags::BR_EDR_NOT_SUPPORTED);
        adv_data.add_flags(flags)?;

        // Add device name
        adv_data.add_complete_name(&self.device_name)?;

        self.adv_data = adv_data;
        self.advertising = true;

        Ok(())
    }

    /// Stop advertising
    pub fn stop_advertising(&mut self) {
        self.advertising = false;
    }

    /// Check if advertising
    pub fn is_advertising(&self) -> bool {
        self.advertising
    }

    /// Handle connection
    pub fn handle_connection(
        &mut self,
        handle: u16,
        peer_addr: BleAddress,
        params: ConnectionParams,
    ) {
        self.connection = Some(BleConnection::new(handle, peer_addr, params));
        self.advertising = false;
    }

    /// Handle disconnection
    pub fn handle_disconnection(&mut self) {
        self.connection = None;
    }

    /// Get connection
    pub fn get_connection(&self) -> Option<&BleConnection> {
        self.connection.as_ref()
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connection
            .as_ref()
            .map(|c| c.is_connected())
            .unwrap_or(false)
    }

    /// Get device address
    pub fn address(&self) -> &BleAddress {
        &self.address
    }

    /// Get device name
    pub fn name(&self) -> &str {
        &self.device_name
    }

    /// Find service by UUID
    pub fn find_service(&self, uuid: &Uuid) -> Option<&GattService> {
        self.services.iter().find(|s| s.uuid == *uuid)
    }

    /// Get all services
    pub fn services(&self) -> &[GattService] {
        &self.services
    }
}

/// BLE Central (Client Role)
#[derive(Debug)]
pub struct BleCentral {
    /// Connections
    connections: heapless::Vec<BleConnection, 4>,
    /// Scanning enabled
    scanning: bool,
}

impl BleCentral {
    /// Create a new BLE central
    pub fn new() -> Self {
        Self {
            connections: heapless::Vec::new(),
            scanning: false,
        }
    }

    /// Start scanning
    pub fn start_scan(&mut self) {
        self.scanning = true;
    }

    /// Stop scanning
    pub fn stop_scan(&mut self) {
        self.scanning = false;
    }

    /// Check if scanning
    pub fn is_scanning(&self) -> bool {
        self.scanning
    }

    /// Connect to device
    pub fn connect(
        &mut self,
        peer_addr: BleAddress,
        params: ConnectionParams,
    ) -> Result<u16, BleError> {
        let handle = self.connections.len() as u16;
        let connection = BleConnection::new(handle, peer_addr, params);

        self.connections
            .push(connection)
            .map_err(|_| BleError::TooManyConnections)?;

        Ok(handle)
    }

    /// Disconnect
    pub fn disconnect(&mut self, handle: u16) -> Result<(), BleError> {
        let connection = self
            .connections
            .iter_mut()
            .find(|c| c.handle == handle)
            .ok_or(BleError::ConnectionNotFound)?;

        connection.disconnect();
        Ok(())
    }

    /// Get connection
    pub fn get_connection(&self, handle: u16) -> Option<&BleConnection> {
        self.connections.iter().find(|c| c.handle == handle)
    }

    /// Get all connections
    pub fn connections(&self) -> &[BleConnection] {
        &self.connections
    }
}

impl Default for BleCentral {
    fn default() -> Self {
        Self::new()
    }
}

/// BLE Error Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleError {
    /// Invalid address type
    InvalidAddressType,
    /// Invalid advertising type
    InvalidAdvType,
    /// Unknown AD type
    UnknownAdType,
    /// Advertising data too large
    AdvDataTooLarge,
    /// Attribute value too large
    AttributeValueTooLarge,
    /// Too many services
    TooManyServices,
    /// Too many characteristics
    TooManyCharacteristics,
    /// Too many connections
    TooManyConnections,
    /// Service not found
    ServiceNotFound,
    /// Characteristic not found
    CharacteristicNotFound,
    /// Connection not found
    ConnectionNotFound,
    /// Invalid device name
    InvalidDeviceName,
    /// Device name too long
    DeviceNameTooLong,
    /// Not connected
    NotConnected,
    /// Permission denied
    PermissionDenied,
}

impl fmt::Display for BleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BleError::InvalidAddressType => write!(f, "Invalid BLE address type"),
            BleError::InvalidAdvType => write!(f, "Invalid advertising type"),
            BleError::UnknownAdType => write!(f, "Unknown AD type"),
            BleError::AdvDataTooLarge => write!(f, "Advertising data too large"),
            BleError::AttributeValueTooLarge => write!(f, "Attribute value too large"),
            BleError::TooManyServices => write!(f, "Too many GATT services"),
            BleError::TooManyCharacteristics => write!(f, "Too many GATT characteristics"),
            BleError::TooManyConnections => write!(f, "Too many BLE connections"),
            BleError::ServiceNotFound => write!(f, "GATT service not found"),
            BleError::CharacteristicNotFound => write!(f, "GATT characteristic not found"),
            BleError::ConnectionNotFound => write!(f, "BLE connection not found"),
            BleError::InvalidDeviceName => write!(f, "Invalid device name"),
            BleError::DeviceNameTooLong => write!(f, "Device name too long"),
            BleError::NotConnected => write!(f, "Not connected"),
            BleError::PermissionDenied => write!(f, "Permission denied"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_address_creation() {
        let addr = BleAddress::public([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        assert_eq!(addr.addr_type, AddressType::Public);
        assert_eq!(addr.addr, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    }

    #[test]
    fn test_adv_flags() {
        let mut flags = AdvFlags::new();
        flags.set(AdvFlags::LE_GENERAL_DISCOVERABLE);
        flags.set(AdvFlags::BR_EDR_NOT_SUPPORTED);

        assert!(flags.is_set(AdvFlags::LE_GENERAL_DISCOVERABLE));
        assert!(flags.is_set(AdvFlags::BR_EDR_NOT_SUPPORTED));
        assert!(!flags.is_set(AdvFlags::LE_LIMITED_DISCOVERABLE));
    }

    #[test]
    fn test_advertising_data() {
        let mut adv_data = AdvertisingData::new();

        let mut flags = AdvFlags::new();
        flags.set(AdvFlags::LE_GENERAL_DISCOVERABLE);
        adv_data.add_flags(flags).unwrap();

        adv_data.add_complete_name("TestDevice").unwrap();

        let data = adv_data.as_bytes();
        assert!(!data.is_empty());
    }

    #[test]
    fn test_uuid() {
        let uuid16 = Uuid::new_16bit(0x1800);
        assert_eq!(uuid16.as_u16(), Some(0x1800));

        let uuid128 = Uuid::new_128bit([0u8; 16]);
        assert_eq!(uuid128.as_u16(), None);
    }

    #[test]
    fn test_characteristic_properties() {
        let props = CharProperties::new(CharProperties::READ | CharProperties::NOTIFY);
        assert!(props.has(CharProperties::READ));
        assert!(props.has(CharProperties::NOTIFY));
        assert!(!props.has(CharProperties::WRITE));
    }

    #[test]
    fn test_att_permissions() {
        let perms = AttPermissions::new(AttPermissions::READ_WRITE);
        assert!(perms.is_readable());
        assert!(perms.is_writable());
    }

    #[test]
    fn test_gatt_attribute() {
        let mut attr = GattAttribute::new(
            1,
            Uuid::new_16bit(0x2a00),
            AttPermissions::new(AttPermissions::READ),
        );

        attr.set_value(b"Test").unwrap();
        assert_eq!(attr.get_value(), b"Test");
    }

    #[test]
    fn test_gatt_characteristic() {
        let mut char = GattCharacteristic::new(
            Uuid::new_16bit(0x2a37),
            CharProperties::new(CharProperties::READ | CharProperties::NOTIFY),
            10,
        );

        char.set_value(b"100").unwrap();
        assert_eq!(char.get_value(), b"100");

        char.enable_notifications(0x01);
        assert!(char.notifications_enabled());
        assert!(!char.indications_enabled());
    }

    #[test]
    fn test_gatt_service() {
        let mut service = GattService::new(Uuid::new_16bit(0x180d), 1, 10);

        let char = GattCharacteristic::new(
            Uuid::new_16bit(0x2a37),
            CharProperties::new(CharProperties::NOTIFY),
            5,
        );

        service.add_characteristic(char).unwrap();

        let found = service.find_characteristic(&Uuid::new_16bit(0x2a37));
        assert!(found.is_some());

        let found_by_handle = service.find_characteristic_by_handle(5);
        assert!(found_by_handle.is_some());
    }

    #[test]
    fn test_connection_params() {
        let params = ConnectionParams::default_params();
        assert_eq!(params.interval_min, 50);
        assert_eq!(params.interval_max, 100);
    }

    #[test]
    fn test_ble_connection() {
        let addr = BleAddress::random([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        let params = ConnectionParams::default_params();
        let mut conn = BleConnection::new(1, addr, params);

        assert!(conn.is_connected());

        conn.disconnect();
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_peripheral_creation() {
        let peripheral = BlePeripheral::new(b"TestDevice").unwrap();
        assert_eq!(peripheral.name(), "TestDevice");
        assert!(!peripheral.is_advertising());
        assert!(!peripheral.is_connected());
    }

    #[test]
    fn test_peripheral_add_service() {
        let mut peripheral = BlePeripheral::new(b"Test").unwrap();

        let service_handle = peripheral.add_service(Uuid::new_16bit(0x180d)).unwrap();
        assert_eq!(service_handle, 1);

        let char_handle = peripheral
            .add_characteristic(
                service_handle,
                Uuid::new_16bit(0x2a37),
                CharProperties::new(CharProperties::NOTIFY),
            )
            .unwrap();

        assert_eq!(char_handle, 2);
    }

    #[test]
    fn test_peripheral_characteristic_value() {
        let mut peripheral = BlePeripheral::new(b"Test").unwrap();

        let service_handle = peripheral.add_service(Uuid::new_16bit(0x1800)).unwrap();
        let char_handle = peripheral
            .add_characteristic(
                service_handle,
                Uuid::new_16bit(0x2a00),
                CharProperties::new(CharProperties::READ),
            )
            .unwrap();

        peripheral
            .set_characteristic_value(char_handle, b"Device")
            .unwrap();

        let value = peripheral.get_characteristic_value(char_handle).unwrap();
        assert_eq!(value, b"Device");
    }

    #[test]
    fn test_peripheral_advertising() {
        let mut peripheral = BlePeripheral::new(b"TestDev").unwrap();

        peripheral.start_advertising().unwrap();
        assert!(peripheral.is_advertising());

        peripheral.stop_advertising();
        assert!(!peripheral.is_advertising());
    }

    #[test]
    fn test_peripheral_connection() {
        let mut peripheral = BlePeripheral::new(b"Test").unwrap();

        let peer_addr = BleAddress::random([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let params = ConnectionParams::default_params();

        peripheral.handle_connection(1, peer_addr, params);
        assert!(peripheral.is_connected());
        assert!(!peripheral.is_advertising());

        peripheral.handle_disconnection();
        assert!(!peripheral.is_connected());
    }

    #[test]
    fn test_central_creation() {
        let central = BleCentral::new();
        assert!(!central.is_scanning());
    }

    #[test]
    fn test_central_scanning() {
        let mut central = BleCentral::new();

        central.start_scan();
        assert!(central.is_scanning());

        central.stop_scan();
        assert!(!central.is_scanning());
    }

    #[test]
    fn test_central_connect() {
        let mut central = BleCentral::new();

        let peer_addr = BleAddress::public([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        let params = ConnectionParams::default_params();

        let handle = central.connect(peer_addr, params).unwrap();
        assert_eq!(handle, 0);

        let conn = central.get_connection(handle);
        assert!(conn.is_some());
        assert!(conn.unwrap().is_connected());
    }

    #[test]
    fn test_central_disconnect() {
        let mut central = BleCentral::new();

        let peer_addr = BleAddress::random([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let params = ConnectionParams::default_params();

        let handle = central.connect(peer_addr, params).unwrap();
        central.disconnect(handle).unwrap();

        let conn = central.get_connection(handle).unwrap();
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_address_type_conversion() {
        assert_eq!(AddressType::from_u8(0).unwrap(), AddressType::Public);
        assert_eq!(AddressType::from_u8(1).unwrap(), AddressType::Random);
    }

    #[test]
    fn test_adv_type_conversion() {
        assert_eq!(AdvType::from_u8(0).unwrap(), AdvType::AdvInd);
        assert_eq!(AdvType::from_u8(3).unwrap(), AdvType::AdvNonconnInd);
    }

    #[test]
    fn test_ad_type_conversion() {
        assert_eq!(AdType::from_u8(0x01).unwrap(), AdType::Flags);
        assert_eq!(AdType::from_u8(0x09).unwrap(), AdType::CompleteLocalName);
    }

    #[test]
    fn test_manufacturer_data() {
        let mut adv_data = AdvertisingData::new();
        adv_data
            .add_manufacturer_data(0x004c, &[0x01, 0x02, 0x03])
            .unwrap();

        let data = adv_data.as_bytes();
        assert!(!data.is_empty());
    }

    #[test]
    fn test_service_uuid_16() {
        let mut adv_data = AdvertisingData::new();
        adv_data.add_service_uuid_16(0x180d).unwrap();

        let data = adv_data.as_bytes();
        assert!(!data.is_empty());
    }

    #[test]
    fn test_tx_power() {
        let mut adv_data = AdvertisingData::new();
        adv_data.add_tx_power(-10).unwrap();

        let data = adv_data.as_bytes();
        assert!(!data.is_empty());
    }

    #[test]
    fn test_error_display() {
        let err = BleError::ServiceNotFound;
        let display = format!("{}", err);
        assert!(display.contains("not found"));
    }

    #[test]
    fn test_peripheral_find_service() {
        let mut peripheral = BlePeripheral::new(b"Test").unwrap();
        let uuid = Uuid::new_16bit(0x1800);

        peripheral.add_service(uuid).unwrap();

        let found = peripheral.find_service(&uuid);
        assert!(found.is_some());
    }

    #[test]
    fn test_advertising_data_clear() {
        let mut adv_data = AdvertisingData::new();
        adv_data.add_complete_name("Test").unwrap();
        assert!(!adv_data.as_bytes().is_empty());

        adv_data.clear();
        assert!(adv_data.as_bytes().is_empty());
    }
}
