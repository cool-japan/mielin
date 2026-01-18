//! MQTT-SN (MQTT for Sensor Networks) Implementation
//!
//! MQTT-SN v1.2 compliant implementation for constrained devices and networks.
//! MQTT-SN is an optimized version of MQTT specifically designed for battery-powered
//! sensor/actuator devices with limited processing power and memory.
//!
//! ## Features
//!
//! - **Lightweight Protocol**: Optimized for constrained devices
//! - **Low Bandwidth**: Smaller message overhead than MQTT
//! - **UDP Transport**: No TCP connection overhead
//! - **Sleep Mode**: Built-in support for sleeping clients
//! - **Topic Registration**: Short topic IDs instead of strings
//! - **QoS Levels**: 0 (At most once), 1 (At least once), 2 (Exactly once), -1 (No connection)
//! - **Last Will**: Testament messages for ungraceful disconnections
//! - **Keep Alive**: Periodic ping to maintain connection
//! - **Gateway Discovery**: Automatic gateway finding
//!
//! ## Example
//!
//! ```rust
//! use mielin_rt::mqttsn::{MqttsnClient, QoS};
//!
//! let mut client = MqttsnClient::new(b"sensor01");
//! // Connect to gateway
//! // let packet = client.connect(60, true)?;
//! // Register topic
//! // let (packet, topic_id) = client.register_topic("temperature")?;
//! // Publish data
//! // let packet = client.publish(topic_id, b"25.5", QoS::Level1, false)?;
//! ```

#![allow(dead_code)]

use core::fmt;

/// MQTT-SN protocol version 1.2
pub const MQTTSN_PROTOCOL_VERSION: u8 = 1;

/// Default MQTT-SN port
pub const MQTTSN_DEFAULT_PORT: u16 = 1883;

/// Maximum message length for constrained devices
pub const MAX_MESSAGE_LENGTH: usize = 256;

/// Maximum client ID length
pub const MAX_CLIENT_ID_LENGTH: usize = 23;

/// Maximum topic name length
pub const MAX_TOPIC_NAME_LENGTH: usize = 64;

/// Broadcast radius for SEARCHGW and GWINFO
pub const DEFAULT_BROADCAST_RADIUS: u8 = 1;

/// MQTT-SN Message Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Advertise gateway
    Advertise = 0x00,
    /// Search for gateway
    SearchGw = 0x01,
    /// Gateway info
    GwInfo = 0x02,
    /// Connect
    Connect = 0x04,
    /// Connect acknowledgment
    ConnAck = 0x05,
    /// Will topic request
    WillTopicReq = 0x06,
    /// Will topic
    WillTopic = 0x07,
    /// Will message request
    WillMsgReq = 0x08,
    /// Will message
    WillMsg = 0x09,
    /// Register topic
    Register = 0x0a,
    /// Register acknowledgment
    RegAck = 0x0b,
    /// Publish
    Publish = 0x0c,
    /// Publish acknowledgment
    PubAck = 0x0d,
    /// Publish complete (QoS 2)
    PubComp = 0x0e,
    /// Publish received (QoS 2)
    PubRec = 0x0f,
    /// Publish release (QoS 2)
    PubRel = 0x10,
    /// Subscribe
    Subscribe = 0x12,
    /// Subscribe acknowledgment
    SubAck = 0x13,
    /// Unsubscribe
    Unsubscribe = 0x14,
    /// Unsubscribe acknowledgment
    UnsubAck = 0x15,
    /// Ping request
    PingReq = 0x16,
    /// Ping response
    PingResp = 0x17,
    /// Disconnect
    Disconnect = 0x18,
    /// Will topic update
    WillTopicUpd = 0x1a,
    /// Will topic update response
    WillTopicResp = 0x1b,
    /// Will message update
    WillMsgUpd = 0x1c,
    /// Will message update response
    WillMsgResp = 0x1d,
}

impl MessageType {
    /// Create from raw value
    pub fn from_u8(value: u8) -> Result<Self, MqttsnError> {
        match value {
            0x00 => Ok(MessageType::Advertise),
            0x01 => Ok(MessageType::SearchGw),
            0x02 => Ok(MessageType::GwInfo),
            0x04 => Ok(MessageType::Connect),
            0x05 => Ok(MessageType::ConnAck),
            0x06 => Ok(MessageType::WillTopicReq),
            0x07 => Ok(MessageType::WillTopic),
            0x08 => Ok(MessageType::WillMsgReq),
            0x09 => Ok(MessageType::WillMsg),
            0x0a => Ok(MessageType::Register),
            0x0b => Ok(MessageType::RegAck),
            0x0c => Ok(MessageType::Publish),
            0x0d => Ok(MessageType::PubAck),
            0x0e => Ok(MessageType::PubComp),
            0x0f => Ok(MessageType::PubRec),
            0x10 => Ok(MessageType::PubRel),
            0x12 => Ok(MessageType::Subscribe),
            0x13 => Ok(MessageType::SubAck),
            0x14 => Ok(MessageType::Unsubscribe),
            0x15 => Ok(MessageType::UnsubAck),
            0x16 => Ok(MessageType::PingReq),
            0x17 => Ok(MessageType::PingResp),
            0x18 => Ok(MessageType::Disconnect),
            0x1a => Ok(MessageType::WillTopicUpd),
            0x1b => Ok(MessageType::WillTopicResp),
            0x1c => Ok(MessageType::WillMsgUpd),
            0x1d => Ok(MessageType::WillMsgResp),
            _ => Err(MqttsnError::InvalidMessageType),
        }
    }

    /// Convert to raw value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Quality of Service Level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum QoS {
    /// At most once delivery (fire and forget)
    Level0 = 0,
    /// At least once delivery (acknowledged)
    Level1 = 1,
    /// Exactly once delivery (assured)
    Level2 = 2,
    /// No connection required (-1 in MQTT-SN)
    LevelMinus1 = 3,
}

impl QoS {
    /// Create from raw value
    pub fn from_u8(value: u8) -> Result<Self, MqttsnError> {
        match value {
            0 => Ok(QoS::Level0),
            1 => Ok(QoS::Level1),
            2 => Ok(QoS::Level2),
            3 => Ok(QoS::LevelMinus1),
            _ => Err(MqttsnError::InvalidQoS),
        }
    }

    /// Convert to raw value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Topic ID Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicIdType {
    /// Normal topic ID
    Normal = 0b00,
    /// Predefined topic ID
    Predefined = 0b01,
    /// Short topic name (2 characters)
    Short = 0b10,
}

impl TopicIdType {
    /// Create from flags
    pub fn from_flags(flags: u8) -> Result<Self, MqttsnError> {
        match (flags >> 1) & 0x03 {
            0b00 => Ok(TopicIdType::Normal),
            0b01 => Ok(TopicIdType::Predefined),
            0b10 => Ok(TopicIdType::Short),
            _ => Err(MqttsnError::InvalidTopicIdType),
        }
    }

    /// Convert to flags bits
    pub fn to_flags(self) -> u8 {
        (self as u8) << 1
    }
}

/// Return Code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReturnCode {
    /// Accepted
    Accepted = 0x00,
    /// Rejected: congestion
    RejectedCongestion = 0x01,
    /// Rejected: invalid topic ID
    RejectedInvalidTopicId = 0x02,
    /// Rejected: not supported
    RejectedNotSupported = 0x03,
}

impl ReturnCode {
    /// Create from raw value
    pub fn from_u8(value: u8) -> Result<Self, MqttsnError> {
        match value {
            0x00 => Ok(ReturnCode::Accepted),
            0x01 => Ok(ReturnCode::RejectedCongestion),
            0x02 => Ok(ReturnCode::RejectedInvalidTopicId),
            0x03 => Ok(ReturnCode::RejectedNotSupported),
            _ => Err(MqttsnError::InvalidReturnCode),
        }
    }

    /// Convert to raw value
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Check if accepted
    pub fn is_accepted(self) -> bool {
        matches!(self, ReturnCode::Accepted)
    }
}

/// Flags field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
    /// Duplicate flag
    pub dup: bool,
    /// QoS level
    pub qos: QoS,
    /// Retain flag
    pub retain: bool,
    /// Will flag
    pub will: bool,
    /// Clean session flag
    pub clean_session: bool,
    /// Topic ID type
    pub topic_id_type: TopicIdType,
}

impl Flags {
    /// Create new flags
    pub fn new() -> Self {
        Self {
            dup: false,
            qos: QoS::Level0,
            retain: false,
            will: false,
            clean_session: false,
            topic_id_type: TopicIdType::Normal,
        }
    }

    /// Encode to byte
    pub fn encode(&self) -> u8 {
        let mut flags = 0u8;

        if self.dup {
            flags |= 0x80;
        }

        flags |= (self.qos.to_u8() & 0x03) << 5;

        if self.retain {
            flags |= 0x10;
        }

        if self.will {
            flags |= 0x08;
        }

        if self.clean_session {
            flags |= 0x04;
        }

        flags |= self.topic_id_type.to_flags();

        flags
    }

    /// Decode from byte
    pub fn decode(byte: u8) -> Result<Self, MqttsnError> {
        let dup = (byte & 0x80) != 0;
        let qos = QoS::from_u8((byte >> 5) & 0x03)?;
        let retain = (byte & 0x10) != 0;
        let will = (byte & 0x08) != 0;
        let clean_session = (byte & 0x04) != 0;
        let topic_id_type = TopicIdType::from_flags(byte)?;

        Ok(Self {
            dup,
            qos,
            retain,
            will,
            clean_session,
            topic_id_type,
        })
    }
}

impl Default for Flags {
    fn default() -> Self {
        Self::new()
    }
}

/// MQTT-SN Message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MqttsnMessage {
    /// Advertise message
    Advertise { gateway_id: u8, duration: u16 },

    /// Search gateway message
    SearchGw { radius: u8 },

    /// Gateway info message
    GwInfo {
        gateway_id: u8,
        gateway_address: Option<heapless::Vec<u8, 16>>,
    },

    /// Connect message
    Connect {
        flags: Flags,
        protocol_id: u8,
        duration: u16,
        client_id: heapless::Vec<u8, MAX_CLIENT_ID_LENGTH>,
    },

    /// Connect acknowledgment
    ConnAck { return_code: ReturnCode },

    /// Register message
    Register {
        topic_id: u16,
        msg_id: u16,
        topic_name: heapless::String<MAX_TOPIC_NAME_LENGTH>,
    },

    /// Register acknowledgment
    RegAck {
        topic_id: u16,
        msg_id: u16,
        return_code: ReturnCode,
    },

    /// Publish message
    Publish {
        flags: Flags,
        topic_id: u16,
        msg_id: u16,
        data: heapless::Vec<u8, 192>,
    },

    /// Publish acknowledgment
    PubAck {
        topic_id: u16,
        msg_id: u16,
        return_code: ReturnCode,
    },

    /// Subscribe message
    Subscribe {
        flags: Flags,
        msg_id: u16,
        topic: SubscribeTopic,
    },

    /// Subscribe acknowledgment
    SubAck {
        flags: Flags,
        topic_id: u16,
        msg_id: u16,
        return_code: ReturnCode,
    },

    /// Unsubscribe message
    Unsubscribe {
        flags: Flags,
        msg_id: u16,
        topic: SubscribeTopic,
    },

    /// Unsubscribe acknowledgment
    UnsubAck { msg_id: u16 },

    /// Ping request
    PingReq {
        client_id: Option<heapless::Vec<u8, MAX_CLIENT_ID_LENGTH>>,
    },

    /// Ping response
    PingResp,

    /// Disconnect message
    Disconnect { duration: Option<u16> },

    /// Will topic
    WillTopic {
        flags: Flags,
        will_topic: heapless::String<MAX_TOPIC_NAME_LENGTH>,
    },

    /// Will message
    WillMsg { will_msg: heapless::Vec<u8, 192> },
}

/// Subscribe Topic (can be name or ID)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscribeTopic {
    /// Topic name
    Name(heapless::String<MAX_TOPIC_NAME_LENGTH>),
    /// Topic ID
    Id(u16),
    /// Short topic (2 bytes)
    Short([u8; 2]),
}

impl MqttsnMessage {
    /// Get message type
    pub fn message_type(&self) -> MessageType {
        match self {
            MqttsnMessage::Advertise { .. } => MessageType::Advertise,
            MqttsnMessage::SearchGw { .. } => MessageType::SearchGw,
            MqttsnMessage::GwInfo { .. } => MessageType::GwInfo,
            MqttsnMessage::Connect { .. } => MessageType::Connect,
            MqttsnMessage::ConnAck { .. } => MessageType::ConnAck,
            MqttsnMessage::Register { .. } => MessageType::Register,
            MqttsnMessage::RegAck { .. } => MessageType::RegAck,
            MqttsnMessage::Publish { .. } => MessageType::Publish,
            MqttsnMessage::PubAck { .. } => MessageType::PubAck,
            MqttsnMessage::Subscribe { .. } => MessageType::Subscribe,
            MqttsnMessage::SubAck { .. } => MessageType::SubAck,
            MqttsnMessage::Unsubscribe { .. } => MessageType::Unsubscribe,
            MqttsnMessage::UnsubAck { .. } => MessageType::UnsubAck,
            MqttsnMessage::PingReq { .. } => MessageType::PingReq,
            MqttsnMessage::PingResp => MessageType::PingResp,
            MqttsnMessage::Disconnect { .. } => MessageType::Disconnect,
            MqttsnMessage::WillTopic { .. } => MessageType::WillTopic,
            MqttsnMessage::WillMsg { .. } => MessageType::WillMsg,
        }
    }

    /// Encode message to bytes
    pub fn encode(&self) -> Result<heapless::Vec<u8, MAX_MESSAGE_LENGTH>, MqttsnError> {
        let mut buffer = heapless::Vec::new();

        // Reserve space for length (will fill in later)
        buffer.push(0).map_err(|_| MqttsnError::BufferTooSmall)?;

        // Message type
        buffer
            .push(self.message_type().to_u8())
            .map_err(|_| MqttsnError::BufferTooSmall)?;

        // Encode message-specific fields
        match self {
            MqttsnMessage::Advertise {
                gateway_id,
                duration,
            } => {
                buffer
                    .push(*gateway_id)
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&duration.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::SearchGw { radius } => {
                buffer
                    .push(*radius)
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::GwInfo {
                gateway_id,
                gateway_address,
            } => {
                buffer
                    .push(*gateway_id)
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                if let Some(addr) = gateway_address {
                    buffer
                        .extend_from_slice(addr)
                        .map_err(|_| MqttsnError::BufferTooSmall)?;
                }
            }

            MqttsnMessage::Connect {
                flags,
                protocol_id,
                duration,
                client_id,
            } => {
                buffer
                    .push(flags.encode())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .push(*protocol_id)
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&duration.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(client_id)
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::ConnAck { return_code } => {
                buffer
                    .push(return_code.to_u8())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::Register {
                topic_id,
                msg_id,
                topic_name,
            } => {
                buffer
                    .extend_from_slice(&topic_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&msg_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(topic_name.as_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::RegAck {
                topic_id,
                msg_id,
                return_code,
            } => {
                buffer
                    .extend_from_slice(&topic_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&msg_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .push(return_code.to_u8())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::Publish {
                flags,
                topic_id,
                msg_id,
                data,
            } => {
                buffer
                    .push(flags.encode())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&topic_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&msg_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(data)
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::PubAck {
                topic_id,
                msg_id,
                return_code,
            } => {
                buffer
                    .extend_from_slice(&topic_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&msg_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .push(return_code.to_u8())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::Subscribe {
                flags,
                msg_id,
                topic,
            } => {
                buffer
                    .push(flags.encode())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&msg_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;

                match topic {
                    SubscribeTopic::Name(name) => {
                        buffer
                            .extend_from_slice(name.as_bytes())
                            .map_err(|_| MqttsnError::BufferTooSmall)?;
                    }
                    SubscribeTopic::Id(id) => {
                        buffer
                            .extend_from_slice(&id.to_be_bytes())
                            .map_err(|_| MqttsnError::BufferTooSmall)?;
                    }
                    SubscribeTopic::Short(bytes) => {
                        buffer
                            .extend_from_slice(bytes)
                            .map_err(|_| MqttsnError::BufferTooSmall)?;
                    }
                }
            }

            MqttsnMessage::SubAck {
                flags,
                topic_id,
                msg_id,
                return_code,
            } => {
                buffer
                    .push(flags.encode())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&topic_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&msg_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .push(return_code.to_u8())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::Unsubscribe {
                flags,
                msg_id,
                topic,
            } => {
                buffer
                    .push(flags.encode())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(&msg_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;

                match topic {
                    SubscribeTopic::Name(name) => {
                        buffer
                            .extend_from_slice(name.as_bytes())
                            .map_err(|_| MqttsnError::BufferTooSmall)?;
                    }
                    SubscribeTopic::Id(id) => {
                        buffer
                            .extend_from_slice(&id.to_be_bytes())
                            .map_err(|_| MqttsnError::BufferTooSmall)?;
                    }
                    SubscribeTopic::Short(bytes) => {
                        buffer
                            .extend_from_slice(bytes)
                            .map_err(|_| MqttsnError::BufferTooSmall)?;
                    }
                }
            }

            MqttsnMessage::UnsubAck { msg_id } => {
                buffer
                    .extend_from_slice(&msg_id.to_be_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::PingReq { client_id } => {
                if let Some(id) = client_id {
                    buffer
                        .extend_from_slice(id)
                        .map_err(|_| MqttsnError::BufferTooSmall)?;
                }
            }

            MqttsnMessage::PingResp => {
                // No additional data
            }

            MqttsnMessage::Disconnect { duration } => {
                if let Some(dur) = duration {
                    buffer
                        .extend_from_slice(&dur.to_be_bytes())
                        .map_err(|_| MqttsnError::BufferTooSmall)?;
                }
            }

            MqttsnMessage::WillTopic { flags, will_topic } => {
                buffer
                    .push(flags.encode())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
                buffer
                    .extend_from_slice(will_topic.as_bytes())
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }

            MqttsnMessage::WillMsg { will_msg } => {
                buffer
                    .extend_from_slice(will_msg)
                    .map_err(|_| MqttsnError::BufferTooSmall)?;
            }
        }

        // Fill in length
        let len = buffer.len();
        if len > 255 {
            return Err(MqttsnError::MessageTooLong);
        }
        buffer[0] = len as u8;

        Ok(buffer)
    }

    /// Decode message from bytes
    pub fn decode(bytes: &[u8]) -> Result<Self, MqttsnError> {
        if bytes.len() < 2 {
            return Err(MqttsnError::MessageTooShort);
        }

        let length = bytes[0] as usize;
        if length != bytes.len() {
            return Err(MqttsnError::InvalidLength);
        }

        let msg_type = MessageType::from_u8(bytes[1])?;
        let payload = &bytes[2..];

        match msg_type {
            MessageType::Advertise => {
                if payload.len() < 3 {
                    return Err(MqttsnError::MessageTooShort);
                }
                Ok(MqttsnMessage::Advertise {
                    gateway_id: payload[0],
                    duration: u16::from_be_bytes([payload[1], payload[2]]),
                })
            }

            MessageType::SearchGw => {
                if payload.is_empty() {
                    return Err(MqttsnError::MessageTooShort);
                }
                Ok(MqttsnMessage::SearchGw { radius: payload[0] })
            }

            MessageType::GwInfo => {
                if payload.is_empty() {
                    return Err(MqttsnError::MessageTooShort);
                }
                let gateway_id = payload[0];
                let gateway_address = if payload.len() > 1 {
                    let mut addr = heapless::Vec::new();
                    addr.extend_from_slice(&payload[1..])
                        .map_err(|_| MqttsnError::BufferTooSmall)?;
                    Some(addr)
                } else {
                    None
                };

                Ok(MqttsnMessage::GwInfo {
                    gateway_id,
                    gateway_address,
                })
            }

            MessageType::Connect => {
                if payload.len() < 4 {
                    return Err(MqttsnError::MessageTooShort);
                }
                let flags = Flags::decode(payload[0])?;
                let protocol_id = payload[1];
                let duration = u16::from_be_bytes([payload[2], payload[3]]);

                let mut client_id = heapless::Vec::new();
                client_id
                    .extend_from_slice(&payload[4..])
                    .map_err(|_| MqttsnError::ClientIdTooLong)?;

                Ok(MqttsnMessage::Connect {
                    flags,
                    protocol_id,
                    duration,
                    client_id,
                })
            }

            MessageType::ConnAck => {
                if payload.is_empty() {
                    return Err(MqttsnError::MessageTooShort);
                }
                Ok(MqttsnMessage::ConnAck {
                    return_code: ReturnCode::from_u8(payload[0])?,
                })
            }

            MessageType::Register => {
                if payload.len() < 4 {
                    return Err(MqttsnError::MessageTooShort);
                }
                let topic_id = u16::from_be_bytes([payload[0], payload[1]]);
                let msg_id = u16::from_be_bytes([payload[2], payload[3]]);

                let topic_name_str =
                    core::str::from_utf8(&payload[4..]).map_err(|_| MqttsnError::InvalidUtf8)?;
                let mut topic_name = heapless::String::new();
                topic_name
                    .push_str(topic_name_str)
                    .map_err(|_| MqttsnError::TopicNameTooLong)?;

                Ok(MqttsnMessage::Register {
                    topic_id,
                    msg_id,
                    topic_name,
                })
            }

            MessageType::RegAck => {
                if payload.len() < 5 {
                    return Err(MqttsnError::MessageTooShort);
                }
                let topic_id = u16::from_be_bytes([payload[0], payload[1]]);
                let msg_id = u16::from_be_bytes([payload[2], payload[3]]);
                let return_code = ReturnCode::from_u8(payload[4])?;

                Ok(MqttsnMessage::RegAck {
                    topic_id,
                    msg_id,
                    return_code,
                })
            }

            MessageType::Publish => {
                if payload.len() < 5 {
                    return Err(MqttsnError::MessageTooShort);
                }
                let flags = Flags::decode(payload[0])?;
                let topic_id = u16::from_be_bytes([payload[1], payload[2]]);
                let msg_id = u16::from_be_bytes([payload[3], payload[4]]);

                let mut data = heapless::Vec::new();
                data.extend_from_slice(&payload[5..])
                    .map_err(|_| MqttsnError::PayloadTooLarge)?;

                Ok(MqttsnMessage::Publish {
                    flags,
                    topic_id,
                    msg_id,
                    data,
                })
            }

            MessageType::PubAck => {
                if payload.len() < 5 {
                    return Err(MqttsnError::MessageTooShort);
                }
                let topic_id = u16::from_be_bytes([payload[0], payload[1]]);
                let msg_id = u16::from_be_bytes([payload[2], payload[3]]);
                let return_code = ReturnCode::from_u8(payload[4])?;

                Ok(MqttsnMessage::PubAck {
                    topic_id,
                    msg_id,
                    return_code,
                })
            }

            MessageType::Subscribe => {
                if payload.len() < 3 {
                    return Err(MqttsnError::MessageTooShort);
                }
                let flags = Flags::decode(payload[0])?;
                let msg_id = u16::from_be_bytes([payload[1], payload[2]]);

                let topic = match flags.topic_id_type {
                    TopicIdType::Normal => {
                        let topic_str = core::str::from_utf8(&payload[3..])
                            .map_err(|_| MqttsnError::InvalidUtf8)?;
                        let mut topic_name = heapless::String::new();
                        topic_name
                            .push_str(topic_str)
                            .map_err(|_| MqttsnError::TopicNameTooLong)?;
                        SubscribeTopic::Name(topic_name)
                    }
                    TopicIdType::Predefined => {
                        if payload.len() < 5 {
                            return Err(MqttsnError::MessageTooShort);
                        }
                        SubscribeTopic::Id(u16::from_be_bytes([payload[3], payload[4]]))
                    }
                    TopicIdType::Short => {
                        if payload.len() < 5 {
                            return Err(MqttsnError::MessageTooShort);
                        }
                        SubscribeTopic::Short([payload[3], payload[4]])
                    }
                };

                Ok(MqttsnMessage::Subscribe {
                    flags,
                    msg_id,
                    topic,
                })
            }

            MessageType::SubAck => {
                if payload.len() < 6 {
                    return Err(MqttsnError::MessageTooShort);
                }
                let flags = Flags::decode(payload[0])?;
                let topic_id = u16::from_be_bytes([payload[1], payload[2]]);
                let msg_id = u16::from_be_bytes([payload[3], payload[4]]);
                let return_code = ReturnCode::from_u8(payload[5])?;

                Ok(MqttsnMessage::SubAck {
                    flags,
                    topic_id,
                    msg_id,
                    return_code,
                })
            }

            MessageType::Unsubscribe => {
                if payload.len() < 3 {
                    return Err(MqttsnError::MessageTooShort);
                }
                let flags = Flags::decode(payload[0])?;
                let msg_id = u16::from_be_bytes([payload[1], payload[2]]);

                let topic = match flags.topic_id_type {
                    TopicIdType::Normal => {
                        let topic_str = core::str::from_utf8(&payload[3..])
                            .map_err(|_| MqttsnError::InvalidUtf8)?;
                        let mut topic_name = heapless::String::new();
                        topic_name
                            .push_str(topic_str)
                            .map_err(|_| MqttsnError::TopicNameTooLong)?;
                        SubscribeTopic::Name(topic_name)
                    }
                    TopicIdType::Predefined => {
                        if payload.len() < 5 {
                            return Err(MqttsnError::MessageTooShort);
                        }
                        SubscribeTopic::Id(u16::from_be_bytes([payload[3], payload[4]]))
                    }
                    TopicIdType::Short => {
                        if payload.len() < 5 {
                            return Err(MqttsnError::MessageTooShort);
                        }
                        SubscribeTopic::Short([payload[3], payload[4]])
                    }
                };

                Ok(MqttsnMessage::Unsubscribe {
                    flags,
                    msg_id,
                    topic,
                })
            }

            MessageType::UnsubAck => {
                if payload.len() < 2 {
                    return Err(MqttsnError::MessageTooShort);
                }
                Ok(MqttsnMessage::UnsubAck {
                    msg_id: u16::from_be_bytes([payload[0], payload[1]]),
                })
            }

            MessageType::PingReq => {
                let client_id = if !payload.is_empty() {
                    let mut id = heapless::Vec::new();
                    id.extend_from_slice(payload)
                        .map_err(|_| MqttsnError::ClientIdTooLong)?;
                    Some(id)
                } else {
                    None
                };

                Ok(MqttsnMessage::PingReq { client_id })
            }

            MessageType::PingResp => Ok(MqttsnMessage::PingResp),

            MessageType::Disconnect => {
                let duration = if payload.len() >= 2 {
                    Some(u16::from_be_bytes([payload[0], payload[1]]))
                } else {
                    None
                };

                Ok(MqttsnMessage::Disconnect { duration })
            }

            MessageType::WillTopic => {
                if payload.is_empty() {
                    return Err(MqttsnError::MessageTooShort);
                }
                let flags = Flags::decode(payload[0])?;

                let topic_str =
                    core::str::from_utf8(&payload[1..]).map_err(|_| MqttsnError::InvalidUtf8)?;
                let mut will_topic = heapless::String::new();
                will_topic
                    .push_str(topic_str)
                    .map_err(|_| MqttsnError::TopicNameTooLong)?;

                Ok(MqttsnMessage::WillTopic { flags, will_topic })
            }

            MessageType::WillMsg => {
                let mut will_msg = heapless::Vec::new();
                will_msg
                    .extend_from_slice(payload)
                    .map_err(|_| MqttsnError::PayloadTooLarge)?;

                Ok(MqttsnMessage::WillMsg { will_msg })
            }

            _ => Err(MqttsnError::UnsupportedMessageType),
        }
    }
}

/// MQTT-SN Client
#[derive(Debug)]
pub struct MqttsnClient {
    /// Client ID
    client_id: heapless::Vec<u8, MAX_CLIENT_ID_LENGTH>,
    /// Next message ID
    msg_id: u16,
    /// Registered topics
    topics: heapless::Vec<TopicRegistration, 16>,
    /// Connection state
    connected: bool,
}

/// Topic Registration
#[derive(Debug, Clone, PartialEq, Eq)]
struct TopicRegistration {
    /// Topic name
    name: heapless::String<MAX_TOPIC_NAME_LENGTH>,
    /// Topic ID
    id: u16,
}

impl MqttsnClient {
    /// Create a new MQTT-SN client
    pub fn new(client_id: &[u8]) -> Result<Self, MqttsnError> {
        if client_id.len() > MAX_CLIENT_ID_LENGTH {
            return Err(MqttsnError::ClientIdTooLong);
        }

        let mut id = heapless::Vec::new();
        id.extend_from_slice(client_id)
            .map_err(|_| MqttsnError::ClientIdTooLong)?;

        Ok(Self {
            client_id: id,
            msg_id: 1,
            topics: heapless::Vec::new(),
            connected: false,
        })
    }

    /// Get next message ID
    fn next_msg_id(&mut self) -> u16 {
        let id = self.msg_id;
        self.msg_id = self.msg_id.wrapping_add(1);
        id
    }

    /// Search for gateway
    pub fn search_gateway(&self) -> Result<heapless::Vec<u8, MAX_MESSAGE_LENGTH>, MqttsnError> {
        let message = MqttsnMessage::SearchGw {
            radius: DEFAULT_BROADCAST_RADIUS,
        };
        message.encode()
    }

    /// Connect to gateway
    pub fn connect(
        &mut self,
        duration: u16,
        clean_session: bool,
    ) -> Result<heapless::Vec<u8, MAX_MESSAGE_LENGTH>, MqttsnError> {
        let mut flags = Flags::new();
        flags.clean_session = clean_session;

        let message = MqttsnMessage::Connect {
            flags,
            protocol_id: MQTTSN_PROTOCOL_VERSION,
            duration,
            client_id: self.client_id.clone(),
        };

        message.encode()
    }

    /// Handle connection acknowledgment
    pub fn handle_connack(&mut self, bytes: &[u8]) -> Result<bool, MqttsnError> {
        let message = MqttsnMessage::decode(bytes)?;

        match message {
            MqttsnMessage::ConnAck { return_code } => {
                self.connected = return_code.is_accepted();
                Ok(self.connected)
            }
            _ => Err(MqttsnError::UnexpectedMessage),
        }
    }

    /// Register topic
    pub fn register_topic(
        &mut self,
        topic_name: &str,
    ) -> Result<(heapless::Vec<u8, MAX_MESSAGE_LENGTH>, u16), MqttsnError> {
        if !self.connected {
            return Err(MqttsnError::NotConnected);
        }

        // Check if already registered
        if let Some(reg) = self.topics.iter().find(|t| t.name.as_str() == topic_name) {
            return Ok((heapless::Vec::new(), reg.id));
        }

        let msg_id = self.next_msg_id();
        let topic_id = 0; // Will be assigned by gateway

        let mut name = heapless::String::new();
        name.push_str(topic_name)
            .map_err(|_| MqttsnError::TopicNameTooLong)?;

        let message = MqttsnMessage::Register {
            topic_id,
            msg_id,
            topic_name: name,
        };

        Ok((message.encode()?, msg_id))
    }

    /// Handle register acknowledgment
    pub fn handle_regack(&mut self, bytes: &[u8]) -> Result<u16, MqttsnError> {
        let message = MqttsnMessage::decode(bytes)?;

        match message {
            MqttsnMessage::RegAck {
                topic_id,
                msg_id: _,
                return_code,
            } => {
                if return_code.is_accepted() {
                    Ok(topic_id)
                } else {
                    Err(MqttsnError::RegistrationRejected)
                }
            }
            _ => Err(MqttsnError::UnexpectedMessage),
        }
    }

    /// Add topic registration
    pub fn add_topic(&mut self, name: &str, id: u16) -> Result<(), MqttsnError> {
        let mut topic_name = heapless::String::new();
        topic_name
            .push_str(name)
            .map_err(|_| MqttsnError::TopicNameTooLong)?;

        let registration = TopicRegistration {
            name: topic_name,
            id,
        };

        self.topics
            .push(registration)
            .map_err(|_| MqttsnError::TooManyTopics)
    }

    /// Publish message
    pub fn publish(
        &mut self,
        topic_id: u16,
        data: &[u8],
        qos: QoS,
        retain: bool,
    ) -> Result<heapless::Vec<u8, MAX_MESSAGE_LENGTH>, MqttsnError> {
        if !self.connected {
            return Err(MqttsnError::NotConnected);
        }

        let mut flags = Flags::new();
        flags.qos = qos;
        flags.retain = retain;
        flags.topic_id_type = TopicIdType::Normal;

        let msg_id = if qos != QoS::Level0 {
            self.next_msg_id()
        } else {
            0
        };

        let mut payload = heapless::Vec::new();
        payload
            .extend_from_slice(data)
            .map_err(|_| MqttsnError::PayloadTooLarge)?;

        let message = MqttsnMessage::Publish {
            flags,
            topic_id,
            msg_id,
            data: payload,
        };

        message.encode()
    }

    /// Subscribe to topic
    pub fn subscribe(
        &mut self,
        topic_name: &str,
        qos: QoS,
    ) -> Result<heapless::Vec<u8, MAX_MESSAGE_LENGTH>, MqttsnError> {
        if !self.connected {
            return Err(MqttsnError::NotConnected);
        }

        let mut flags = Flags::new();
        flags.qos = qos;
        flags.topic_id_type = TopicIdType::Normal;

        let msg_id = self.next_msg_id();

        let mut name = heapless::String::new();
        name.push_str(topic_name)
            .map_err(|_| MqttsnError::TopicNameTooLong)?;

        let message = MqttsnMessage::Subscribe {
            flags,
            msg_id,
            topic: SubscribeTopic::Name(name),
        };

        message.encode()
    }

    /// Unsubscribe from topic
    pub fn unsubscribe(
        &mut self,
        topic_name: &str,
    ) -> Result<heapless::Vec<u8, MAX_MESSAGE_LENGTH>, MqttsnError> {
        if !self.connected {
            return Err(MqttsnError::NotConnected);
        }

        let mut flags = Flags::new();
        flags.topic_id_type = TopicIdType::Normal;

        let msg_id = self.next_msg_id();

        let mut name = heapless::String::new();
        name.push_str(topic_name)
            .map_err(|_| MqttsnError::TopicNameTooLong)?;

        let message = MqttsnMessage::Unsubscribe {
            flags,
            msg_id,
            topic: SubscribeTopic::Name(name),
        };

        message.encode()
    }

    /// Send ping
    pub fn ping(&self) -> Result<heapless::Vec<u8, MAX_MESSAGE_LENGTH>, MqttsnError> {
        let message = MqttsnMessage::PingReq { client_id: None };
        message.encode()
    }

    /// Disconnect
    pub fn disconnect(
        &mut self,
        duration: Option<u16>,
    ) -> Result<heapless::Vec<u8, MAX_MESSAGE_LENGTH>, MqttsnError> {
        let message = MqttsnMessage::Disconnect { duration };
        self.connected = false;
        message.encode()
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Get topic ID by name
    pub fn get_topic_id(&self, name: &str) -> Option<u16> {
        self.topics
            .iter()
            .find(|t| t.name.as_str() == name)
            .map(|t| t.id)
    }
}

/// MQTT-SN Error Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MqttsnError {
    /// Invalid message type
    InvalidMessageType,
    /// Invalid QoS level
    InvalidQoS,
    /// Invalid topic ID type
    InvalidTopicIdType,
    /// Invalid return code
    InvalidReturnCode,
    /// Buffer too small
    BufferTooSmall,
    /// Message too long
    MessageTooLong,
    /// Message too short
    MessageTooShort,
    /// Invalid length field
    InvalidLength,
    /// Invalid UTF-8
    InvalidUtf8,
    /// Client ID too long
    ClientIdTooLong,
    /// Topic name too long
    TopicNameTooLong,
    /// Payload too large
    PayloadTooLarge,
    /// Not connected
    NotConnected,
    /// Registration rejected
    RegistrationRejected,
    /// Unexpected message
    UnexpectedMessage,
    /// Too many topics
    TooManyTopics,
    /// Unsupported message type
    UnsupportedMessageType,
}

impl fmt::Display for MqttsnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MqttsnError::InvalidMessageType => write!(f, "Invalid MQTT-SN message type"),
            MqttsnError::InvalidQoS => write!(f, "Invalid QoS level"),
            MqttsnError::InvalidTopicIdType => write!(f, "Invalid topic ID type"),
            MqttsnError::InvalidReturnCode => write!(f, "Invalid return code"),
            MqttsnError::BufferTooSmall => write!(f, "Buffer too small"),
            MqttsnError::MessageTooLong => write!(f, "Message too long"),
            MqttsnError::MessageTooShort => write!(f, "Message too short"),
            MqttsnError::InvalidLength => write!(f, "Invalid length field"),
            MqttsnError::InvalidUtf8 => write!(f, "Invalid UTF-8 encoding"),
            MqttsnError::ClientIdTooLong => write!(f, "Client ID too long"),
            MqttsnError::TopicNameTooLong => write!(f, "Topic name too long"),
            MqttsnError::PayloadTooLarge => write!(f, "Payload too large"),
            MqttsnError::NotConnected => write!(f, "Not connected to gateway"),
            MqttsnError::RegistrationRejected => write!(f, "Topic registration rejected"),
            MqttsnError::UnexpectedMessage => write!(f, "Unexpected message type"),
            MqttsnError::TooManyTopics => write!(f, "Too many topics registered"),
            MqttsnError::UnsupportedMessageType => write!(f, "Unsupported message type"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_message_type_conversion() {
        assert_eq!(MessageType::from_u8(0x0c).unwrap(), MessageType::Publish);
        assert_eq!(MessageType::Publish.to_u8(), 0x0c);
    }

    #[test]
    fn test_qos_conversion() {
        assert_eq!(QoS::from_u8(0).unwrap(), QoS::Level0);
        assert_eq!(QoS::from_u8(1).unwrap(), QoS::Level1);
        assert_eq!(QoS::from_u8(2).unwrap(), QoS::Level2);
        assert_eq!(QoS::Level1.to_u8(), 1);
    }

    #[test]
    fn test_return_code() {
        assert!(ReturnCode::Accepted.is_accepted());
        assert!(!ReturnCode::RejectedCongestion.is_accepted());
    }

    #[test]
    fn test_flags_encode_decode() {
        let mut flags = Flags::new();
        flags.qos = QoS::Level1;
        flags.retain = true;
        flags.dup = false;

        let encoded = flags.encode();
        let decoded = Flags::decode(encoded).unwrap();

        assert_eq!(decoded.qos, QoS::Level1);
        assert!(decoded.retain);
        assert!(!decoded.dup);
    }

    #[test]
    fn test_searchgw_encode_decode() {
        let message = MqttsnMessage::SearchGw { radius: 1 };
        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::SearchGw { radius } => assert_eq!(radius, 1),
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_advertise_encode_decode() {
        let message = MqttsnMessage::Advertise {
            gateway_id: 5,
            duration: 900,
        };
        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::Advertise {
                gateway_id,
                duration,
            } => {
                assert_eq!(gateway_id, 5);
                assert_eq!(duration, 900);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_connect_encode_decode() {
        let mut flags = Flags::new();
        flags.clean_session = true;

        let mut client_id = heapless::Vec::new();
        client_id.extend_from_slice(b"test_client").unwrap();

        let message = MqttsnMessage::Connect {
            flags,
            protocol_id: MQTTSN_PROTOCOL_VERSION,
            duration: 60,
            client_id,
        };

        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::Connect {
                flags,
                protocol_id,
                duration,
                client_id,
            } => {
                assert!(flags.clean_session);
                assert_eq!(protocol_id, MQTTSN_PROTOCOL_VERSION);
                assert_eq!(duration, 60);
                assert_eq!(client_id.as_slice(), b"test_client");
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_publish_encode_decode() {
        let mut flags = Flags::new();
        flags.qos = QoS::Level1;
        flags.retain = false;

        let mut data = heapless::Vec::new();
        data.extend_from_slice(b"test data").unwrap();

        let message = MqttsnMessage::Publish {
            flags,
            topic_id: 10,
            msg_id: 100,
            data,
        };

        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::Publish {
                flags,
                topic_id,
                msg_id,
                data,
            } => {
                assert_eq!(flags.qos, QoS::Level1);
                assert_eq!(topic_id, 10);
                assert_eq!(msg_id, 100);
                assert_eq!(data.as_slice(), b"test data");
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_register_encode_decode() {
        let mut topic_name = heapless::String::new();
        topic_name.push_str("temperature").unwrap();

        let message = MqttsnMessage::Register {
            topic_id: 0,
            msg_id: 50,
            topic_name,
        };

        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::Register {
                topic_id,
                msg_id,
                topic_name,
            } => {
                assert_eq!(topic_id, 0);
                assert_eq!(msg_id, 50);
                assert_eq!(topic_name.as_str(), "temperature");
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_subscribe_encode_decode() {
        let mut flags = Flags::new();
        flags.qos = QoS::Level1;

        let mut name = heapless::String::new();
        name.push_str("sensor/data").unwrap();

        let message = MqttsnMessage::Subscribe {
            flags,
            msg_id: 25,
            topic: SubscribeTopic::Name(name),
        };

        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::Subscribe {
                flags,
                msg_id,
                topic,
            } => {
                assert_eq!(flags.qos, QoS::Level1);
                assert_eq!(msg_id, 25);
                match topic {
                    SubscribeTopic::Name(name) => assert_eq!(name.as_str(), "sensor/data"),
                    _ => panic!("Wrong topic type"),
                }
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_ping_encode_decode() {
        let message = MqttsnMessage::PingReq { client_id: None };
        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::PingReq { client_id } => assert!(client_id.is_none()),
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_disconnect_encode_decode() {
        let message = MqttsnMessage::Disconnect {
            duration: Some(120),
        };
        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::Disconnect { duration } => assert_eq!(duration, Some(120)),
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_client_creation() {
        let client = MqttsnClient::new(b"sensor01").unwrap();
        assert!(!client.is_connected());
    }

    #[test]
    fn test_client_connect() {
        let mut client = MqttsnClient::new(b"test").unwrap();
        let packet = client.connect(60, true).unwrap();
        assert!(!packet.is_empty());
    }

    #[test]
    fn test_client_search_gateway() {
        let client = MqttsnClient::new(b"test").unwrap();
        let packet = client.search_gateway().unwrap();
        assert!(!packet.is_empty());
    }

    #[test]
    fn test_client_register_topic() {
        let mut client = MqttsnClient::new(b"test").unwrap();
        client.connected = true; // Simulate connection

        let (packet, msg_id) = client.register_topic("temp").unwrap();
        assert!(!packet.is_empty());
        assert!(msg_id > 0);
    }

    #[test]
    fn test_client_publish() {
        let mut client = MqttsnClient::new(b"test").unwrap();
        client.connected = true;

        let packet = client.publish(10, b"25.5", QoS::Level1, false).unwrap();
        assert!(!packet.is_empty());
    }

    #[test]
    fn test_client_subscribe() {
        let mut client = MqttsnClient::new(b"test").unwrap();
        client.connected = true;

        let packet = client.subscribe("sensor/temp", QoS::Level1).unwrap();
        assert!(!packet.is_empty());
    }

    #[test]
    fn test_client_ping() {
        let client = MqttsnClient::new(b"test").unwrap();
        let packet = client.ping().unwrap();
        assert!(!packet.is_empty());
    }

    #[test]
    fn test_client_disconnect() {
        let mut client = MqttsnClient::new(b"test").unwrap();
        client.connected = true;

        let packet = client.disconnect(None).unwrap();
        assert!(!packet.is_empty());
        assert!(!client.is_connected());
    }

    #[test]
    fn test_client_not_connected_error() {
        let mut client = MqttsnClient::new(b"test").unwrap();
        let result = client.publish(10, b"data", QoS::Level0, false);
        assert_eq!(result.unwrap_err(), MqttsnError::NotConnected);
    }

    #[test]
    fn test_client_add_topic() {
        let mut client = MqttsnClient::new(b"test").unwrap();
        client.add_topic("temp", 5).unwrap();
        assert_eq!(client.get_topic_id("temp"), Some(5));
    }

    #[test]
    fn test_topic_id_type_flags() {
        let normal = TopicIdType::Normal.to_flags();
        let predefined = TopicIdType::Predefined.to_flags();
        let short = TopicIdType::Short.to_flags();

        assert_eq!(
            TopicIdType::from_flags(normal).unwrap(),
            TopicIdType::Normal
        );
        assert_eq!(
            TopicIdType::from_flags(predefined).unwrap(),
            TopicIdType::Predefined
        );
        assert_eq!(TopicIdType::from_flags(short).unwrap(), TopicIdType::Short);
    }

    #[test]
    fn test_error_display() {
        let err = MqttsnError::NotConnected;
        let display = format!("{}", err);
        assert!(display.contains("Not connected"));
    }

    #[test]
    fn test_message_id_increment() {
        let mut client = MqttsnClient::new(b"test").unwrap();
        let id1 = client.next_msg_id();
        let id2 = client.next_msg_id();
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    fn test_client_handle_connack() {
        let mut client = MqttsnClient::new(b"test").unwrap();

        let ack = MqttsnMessage::ConnAck {
            return_code: ReturnCode::Accepted,
        };
        let encoded = ack.encode().unwrap();

        let connected = client.handle_connack(&encoded).unwrap();
        assert!(connected);
        assert!(client.is_connected());
    }

    #[test]
    fn test_client_handle_regack() {
        let mut client = MqttsnClient::new(b"test").unwrap();

        let ack = MqttsnMessage::RegAck {
            topic_id: 15,
            msg_id: 1,
            return_code: ReturnCode::Accepted,
        };
        let encoded = ack.encode().unwrap();

        let topic_id = client.handle_regack(&encoded).unwrap();
        assert_eq!(topic_id, 15);
    }

    #[test]
    fn test_puback_encode_decode() {
        let message = MqttsnMessage::PubAck {
            topic_id: 20,
            msg_id: 5,
            return_code: ReturnCode::Accepted,
        };

        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::PubAck {
                topic_id,
                msg_id,
                return_code,
            } => {
                assert_eq!(topic_id, 20);
                assert_eq!(msg_id, 5);
                assert_eq!(return_code, ReturnCode::Accepted);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_will_topic_encode_decode() {
        let mut flags = Flags::new();
        flags.qos = QoS::Level1;
        flags.retain = true;

        let mut will_topic = heapless::String::new();
        will_topic.push_str("device/status").unwrap();

        let message = MqttsnMessage::WillTopic { flags, will_topic };

        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::WillTopic { flags, will_topic } => {
                assert_eq!(flags.qos, QoS::Level1);
                assert!(flags.retain);
                assert_eq!(will_topic.as_str(), "device/status");
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_will_msg_encode_decode() {
        let mut will_msg = heapless::Vec::new();
        will_msg.extend_from_slice(b"offline").unwrap();

        let message = MqttsnMessage::WillMsg { will_msg };

        let encoded = message.encode().unwrap();
        let decoded = MqttsnMessage::decode(&encoded).unwrap();

        match decoded {
            MqttsnMessage::WillMsg { will_msg } => {
                assert_eq!(will_msg.as_slice(), b"offline");
            }
            _ => panic!("Wrong message type"),
        }
    }
}
