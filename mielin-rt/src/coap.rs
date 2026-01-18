//! CoAP (Constrained Application Protocol) Implementation
//!
//! RFC 7252 compliant CoAP implementation for IoT and embedded devices.
//! CoAP is a specialized web transfer protocol for use with constrained nodes
//! and constrained networks in the Internet of Things.
//!
//! ## Features
//!
//! - **Message Types**: CON (Confirmable), NON (Non-confirmable), ACK (Acknowledgment), RST (Reset)
//! - **Request Methods**: GET, POST, PUT, DELETE
//! - **Response Codes**: Success (2.xx), Client Error (4.xx), Server Error (5.xx)
//! - **Options**: URI-Path, Content-Format, Accept, Max-Age, ETag, etc.
//! - **Observe Pattern**: For pub/sub functionality (RFC 7641)
//! - **Block-wise Transfers**: For large payloads (RFC 7959)
//! - **Resource Discovery**: Via .well-known/core
//! - **Token Support**: For request/response matching
//! - **Retransmission**: With exponential backoff
//!
//! ## Example
//!
//! ```rust
//! use mielin_rt::coap::{CoapClient, CoapRequest, Method};
//!
//! let mut client = CoapClient::new();
//! let request = CoapRequest::new(Method::Get)
//!     .with_uri_path("/sensors/temperature")
//!     .with_token(&[0x12, 0x34]);
//!
//! // Send request and get response
//! // let response = client.send(request)?;
//! ```

#![allow(dead_code)]

use core::fmt;

/// CoAP version (always 1 for RFC 7252)
pub const COAP_VERSION: u8 = 1;

/// Default CoAP port
pub const COAP_DEFAULT_PORT: u16 = 5683;

/// CoAPs (secure) default port
pub const COAPS_DEFAULT_PORT: u16 = 5684;

/// Maximum message size for constrained devices
pub const MAX_MESSAGE_SIZE: usize = 1152;

/// Maximum token length (8 bytes per RFC 7252)
pub const MAX_TOKEN_LENGTH: usize = 8;

/// Maximum retransmission attempts
pub const MAX_RETRANSMIT: u8 = 4;

/// Initial retransmission timeout (2 seconds)
pub const ACK_TIMEOUT_MS: u32 = 2000;

/// Random factor for timeout calculation
pub const ACK_RANDOM_FACTOR: f32 = 1.5;

/// Message Type (4 values, 2 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Confirmable message (requires ACK)
    Confirmable = 0,
    /// Non-confirmable message
    NonConfirmable = 1,
    /// Acknowledgment
    Acknowledgment = 2,
    /// Reset (error)
    Reset = 3,
}

impl MessageType {
    /// Create from raw value
    pub fn from_u8(value: u8) -> Result<Self, CoapError> {
        match value & 0x03 {
            0 => Ok(MessageType::Confirmable),
            1 => Ok(MessageType::NonConfirmable),
            2 => Ok(MessageType::Acknowledgment),
            3 => Ok(MessageType::Reset),
            _ => Err(CoapError::InvalidMessageType),
        }
    }

    /// Convert to raw value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// CoAP Method Codes (0.xx)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Method {
    /// GET method
    Get = 1,
    /// POST method
    Post = 2,
    /// PUT method
    Put = 3,
    /// DELETE method
    Delete = 4,
}

impl Method {
    /// Create from code value
    pub fn from_code(code: u8) -> Result<Self, CoapError> {
        match code {
            1 => Ok(Method::Get),
            2 => Ok(Method::Post),
            3 => Ok(Method::Put),
            4 => Ok(Method::Delete),
            _ => Err(CoapError::InvalidMethod),
        }
    }

    /// Convert to code value (class = 0, detail = method)
    pub fn to_code(self) -> u8 {
        self as u8
    }
}

/// Response Code (class.detail format)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCode {
    // Success 2.xx
    /// 2.01 Created
    Created = 0x41,
    /// 2.02 Deleted
    Deleted = 0x42,
    /// 2.03 Valid
    Valid = 0x43,
    /// 2.04 Changed
    Changed = 0x44,
    /// 2.05 Content
    Content = 0x45,

    // Client Error 4.xx
    /// 4.00 Bad Request
    BadRequest = 0x80,
    /// 4.01 Unauthorized
    Unauthorized = 0x81,
    /// 4.02 Bad Option
    BadOption = 0x82,
    /// 4.03 Forbidden
    Forbidden = 0x83,
    /// 4.04 Not Found
    NotFound = 0x84,
    /// 4.05 Method Not Allowed
    MethodNotAllowed = 0x85,
    /// 4.06 Not Acceptable
    NotAcceptable = 0x86,
    /// 4.12 Precondition Failed
    PreconditionFailed = 0x8c,
    /// 4.13 Request Entity Too Large
    RequestEntityTooLarge = 0x8d,
    /// 4.15 Unsupported Content-Format
    UnsupportedContentFormat = 0x8f,

    // Server Error 5.xx
    /// 5.00 Internal Server Error
    InternalServerError = 0xa0,
    /// 5.01 Not Implemented
    NotImplemented = 0xa1,
    /// 5.02 Bad Gateway
    BadGateway = 0xa2,
    /// 5.03 Service Unavailable
    ServiceUnavailable = 0xa3,
    /// 5.04 Gateway Timeout
    GatewayTimeout = 0xa4,
    /// 5.05 Proxying Not Supported
    ProxyingNotSupported = 0xa5,
}

impl ResponseCode {
    /// Create from raw code value
    pub fn from_u8(code: u8) -> Result<Self, CoapError> {
        match code {
            0x41 => Ok(ResponseCode::Created),
            0x42 => Ok(ResponseCode::Deleted),
            0x43 => Ok(ResponseCode::Valid),
            0x44 => Ok(ResponseCode::Changed),
            0x45 => Ok(ResponseCode::Content),
            0x80 => Ok(ResponseCode::BadRequest),
            0x81 => Ok(ResponseCode::Unauthorized),
            0x82 => Ok(ResponseCode::BadOption),
            0x83 => Ok(ResponseCode::Forbidden),
            0x84 => Ok(ResponseCode::NotFound),
            0x85 => Ok(ResponseCode::MethodNotAllowed),
            0x86 => Ok(ResponseCode::NotAcceptable),
            0x8c => Ok(ResponseCode::PreconditionFailed),
            0x8d => Ok(ResponseCode::RequestEntityTooLarge),
            0x8f => Ok(ResponseCode::UnsupportedContentFormat),
            0xa0 => Ok(ResponseCode::InternalServerError),
            0xa1 => Ok(ResponseCode::NotImplemented),
            0xa2 => Ok(ResponseCode::BadGateway),
            0xa3 => Ok(ResponseCode::ServiceUnavailable),
            0xa4 => Ok(ResponseCode::GatewayTimeout),
            0xa5 => Ok(ResponseCode::ProxyingNotSupported),
            _ => Err(CoapError::InvalidResponseCode),
        }
    }

    /// Convert to raw code value
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Get class (first digit)
    pub fn class(self) -> u8 {
        (self as u8) >> 5
    }

    /// Get detail (second two digits)
    pub fn detail(self) -> u8 {
        (self as u8) & 0x1f
    }

    /// Check if this is a success code (2.xx)
    pub fn is_success(self) -> bool {
        self.class() == 2
    }

    /// Check if this is a client error (4.xx)
    pub fn is_client_error(self) -> bool {
        self.class() == 4
    }

    /// Check if this is a server error (5.xx)
    pub fn is_server_error(self) -> bool {
        self.class() == 5
    }
}

/// CoAP Option Numbers (RFC 7252 & extensions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum OptionNumber {
    /// If-Match option
    IfMatch = 1,
    /// URI-Host option
    UriHost = 3,
    /// ETag option
    ETag = 4,
    /// If-None-Match option
    IfNoneMatch = 5,
    /// URI-Port option
    UriPort = 7,
    /// Location-Path option
    LocationPath = 8,
    /// URI-Path option
    UriPath = 11,
    /// Content-Format option
    ContentFormat = 12,
    /// Max-Age option
    MaxAge = 14,
    /// URI-Query option
    UriQuery = 15,
    /// Accept option
    Accept = 17,
    /// Location-Query option
    LocationQuery = 20,
    /// Proxy-Uri option
    ProxyUri = 35,
    /// Proxy-Scheme option
    ProxyScheme = 39,
    /// Size1 option
    Size1 = 60,
    /// Observe option (RFC 7641)
    Observe = 6,
    /// Block2 option (RFC 7959)
    Block2 = 23,
    /// Block1 option (RFC 7959)
    Block1 = 27,
    /// Size2 option (RFC 7959)
    Size2 = 28,
}

impl OptionNumber {
    /// Create from raw value
    pub fn from_u16(value: u16) -> Result<Self, CoapError> {
        match value {
            1 => Ok(OptionNumber::IfMatch),
            3 => Ok(OptionNumber::UriHost),
            4 => Ok(OptionNumber::ETag),
            5 => Ok(OptionNumber::IfNoneMatch),
            6 => Ok(OptionNumber::Observe),
            7 => Ok(OptionNumber::UriPort),
            8 => Ok(OptionNumber::LocationPath),
            11 => Ok(OptionNumber::UriPath),
            12 => Ok(OptionNumber::ContentFormat),
            14 => Ok(OptionNumber::MaxAge),
            15 => Ok(OptionNumber::UriQuery),
            17 => Ok(OptionNumber::Accept),
            20 => Ok(OptionNumber::LocationQuery),
            23 => Ok(OptionNumber::Block2),
            27 => Ok(OptionNumber::Block1),
            28 => Ok(OptionNumber::Size2),
            35 => Ok(OptionNumber::ProxyUri),
            39 => Ok(OptionNumber::ProxyScheme),
            60 => Ok(OptionNumber::Size1),
            _ => Err(CoapError::UnknownOption),
        }
    }

    /// Convert to raw value
    pub fn to_u16(self) -> u16 {
        self as u16
    }
}

/// Content Format values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ContentFormat {
    /// text/plain; charset=utf-8
    TextPlain = 0,
    /// application/link-format
    ApplicationLinkFormat = 40,
    /// application/xml
    ApplicationXml = 41,
    /// application/octet-stream
    ApplicationOctetStream = 42,
    /// application/exi
    ApplicationExi = 47,
    /// application/json
    ApplicationJson = 50,
    /// application/cbor
    ApplicationCbor = 60,
}

impl ContentFormat {
    /// Create from raw value
    pub fn from_u16(value: u16) -> Result<Self, CoapError> {
        match value {
            0 => Ok(ContentFormat::TextPlain),
            40 => Ok(ContentFormat::ApplicationLinkFormat),
            41 => Ok(ContentFormat::ApplicationXml),
            42 => Ok(ContentFormat::ApplicationOctetStream),
            47 => Ok(ContentFormat::ApplicationExi),
            50 => Ok(ContentFormat::ApplicationJson),
            60 => Ok(ContentFormat::ApplicationCbor),
            _ => Err(CoapError::UnsupportedContentFormat),
        }
    }

    /// Convert to raw value
    pub fn to_u16(self) -> u16 {
        self as u16
    }
}

/// CoAP Option
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoapOption {
    /// Option number
    pub number: u16,
    /// Option value
    pub value: heapless::Vec<u8, 256>,
}

impl CoapOption {
    /// Create a new option
    pub fn new(number: OptionNumber, value: &[u8]) -> Result<Self, CoapError> {
        let mut vec = heapless::Vec::new();
        vec.extend_from_slice(value)
            .map_err(|_| CoapError::OptionTooLarge)?;

        Ok(Self {
            number: number.to_u16(),
            value: vec,
        })
    }

    /// Create a URI-Path option
    pub fn uri_path(path: &str) -> Result<Self, CoapError> {
        Self::new(OptionNumber::UriPath, path.as_bytes())
    }

    /// Create a Content-Format option
    pub fn content_format(format: ContentFormat) -> Result<Self, CoapError> {
        let value = format.to_u16();
        if value < 256 {
            Self::new(OptionNumber::ContentFormat, &[value as u8])
        } else {
            Self::new(
                OptionNumber::ContentFormat,
                &[(value >> 8) as u8, value as u8],
            )
        }
    }

    /// Create an Observe option
    pub fn observe(register: bool) -> Result<Self, CoapError> {
        let value = if register { 0u8 } else { 1u8 };
        Self::new(OptionNumber::Observe, &[value])
    }

    /// Create a Max-Age option
    pub fn max_age(seconds: u32) -> Result<Self, CoapError> {
        let bytes = seconds.to_be_bytes();
        // Find first non-zero byte
        let start = bytes.iter().position(|&b| b != 0).unwrap_or(3);
        Self::new(OptionNumber::MaxAge, &bytes[start..])
    }

    /// Get option value as u32
    pub fn as_u32(&self) -> Result<u32, CoapError> {
        if self.value.len() > 4 {
            return Err(CoapError::InvalidOptionValue);
        }

        let mut result = 0u32;
        for &byte in self.value.iter() {
            result = (result << 8) | (byte as u32);
        }
        Ok(result)
    }

    /// Get option value as string
    pub fn as_str(&self) -> Result<&str, CoapError> {
        core::str::from_utf8(&self.value).map_err(|_| CoapError::InvalidOptionValue)
    }
}

/// CoAP Message Header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoapHeader {
    /// Version (always 1)
    pub version: u8,
    /// Message type
    pub msg_type: MessageType,
    /// Token length (0-8)
    pub token_length: u8,
    /// Code (method or response)
    pub code: u8,
    /// Message ID
    pub message_id: u16,
}

impl CoapHeader {
    /// Create a new header
    pub fn new(msg_type: MessageType, code: u8, message_id: u16) -> Self {
        Self {
            version: COAP_VERSION,
            msg_type,
            token_length: 0,
            code,
            message_id,
        }
    }

    /// Set token length
    pub fn with_token_length(mut self, length: u8) -> Result<Self, CoapError> {
        if length > MAX_TOKEN_LENGTH as u8 {
            return Err(CoapError::TokenTooLong);
        }
        self.token_length = length;
        Ok(self)
    }

    /// Encode header to bytes (4 bytes)
    pub fn encode(&self) -> [u8; 4] {
        let byte0 = (self.version << 6) | (self.msg_type.to_u8() << 4) | (self.token_length & 0x0f);
        let byte1 = self.code;
        let byte2 = (self.message_id >> 8) as u8;
        let byte3 = self.message_id as u8;

        [byte0, byte1, byte2, byte3]
    }

    /// Decode header from bytes
    pub fn decode(bytes: &[u8]) -> Result<Self, CoapError> {
        if bytes.len() < 4 {
            return Err(CoapError::MessageTooShort);
        }

        let version = bytes[0] >> 6;
        if version != COAP_VERSION {
            return Err(CoapError::UnsupportedVersion);
        }

        let msg_type = MessageType::from_u8((bytes[0] >> 4) & 0x03)?;
        let token_length = bytes[0] & 0x0f;

        if token_length > MAX_TOKEN_LENGTH as u8 {
            return Err(CoapError::TokenTooLong);
        }

        let code = bytes[1];
        let message_id = ((bytes[2] as u16) << 8) | (bytes[3] as u16);

        Ok(Self {
            version,
            msg_type,
            token_length,
            code,
            message_id,
        })
    }
}

/// CoAP Message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoapMessage {
    /// Message header
    pub header: CoapHeader,
    /// Token (0-8 bytes)
    pub token: heapless::Vec<u8, 8>,
    /// Options
    pub options: heapless::Vec<CoapOption, 16>,
    /// Payload
    pub payload: heapless::Vec<u8, 1024>,
}

impl CoapMessage {
    /// Create a new message
    pub fn new(msg_type: MessageType, code: u8, message_id: u16) -> Self {
        Self {
            header: CoapHeader::new(msg_type, code, message_id),
            token: heapless::Vec::new(),
            options: heapless::Vec::new(),
            payload: heapless::Vec::new(),
        }
    }

    /// Set token
    pub fn with_token(mut self, token: &[u8]) -> Result<Self, CoapError> {
        if token.len() > MAX_TOKEN_LENGTH {
            return Err(CoapError::TokenTooLong);
        }

        self.token.clear();
        self.token
            .extend_from_slice(token)
            .map_err(|_| CoapError::TokenTooLong)?;
        self.header.token_length = token.len() as u8;

        Ok(self)
    }

    /// Add option
    pub fn add_option(&mut self, option: CoapOption) -> Result<(), CoapError> {
        self.options
            .push(option)
            .map_err(|_| CoapError::TooManyOptions)
    }

    /// Set payload
    pub fn with_payload(mut self, payload: &[u8]) -> Result<Self, CoapError> {
        self.payload.clear();
        self.payload
            .extend_from_slice(payload)
            .map_err(|_| CoapError::PayloadTooLarge)?;
        Ok(self)
    }

    /// Sort options by option number (required for encoding)
    pub fn sort_options(&mut self) {
        // Simple bubble sort (sufficient for small number of options)
        let len = self.options.len();
        for i in 0..len {
            for j in 0..len - 1 - i {
                if self.options[j].number > self.options[j + 1].number {
                    self.options.swap(j, j + 1);
                }
            }
        }
    }

    /// Encode message to bytes
    pub fn encode(&mut self) -> Result<heapless::Vec<u8, MAX_MESSAGE_SIZE>, CoapError> {
        let mut buffer = heapless::Vec::new();

        // Encode header
        let header_bytes = self.header.encode();
        buffer
            .extend_from_slice(&header_bytes)
            .map_err(|_| CoapError::EncodingFailed)?;

        // Encode token
        buffer
            .extend_from_slice(&self.token)
            .map_err(|_| CoapError::EncodingFailed)?;

        // Sort options before encoding
        self.sort_options();

        // Encode options
        let mut prev_option_number = 0u16;
        for option in &self.options {
            // Calculate option delta
            let delta = option.number - prev_option_number;
            let delta_nibble = if delta < 13 {
                delta as u8
            } else if delta < 269 {
                13
            } else {
                14
            };

            // Calculate option length
            let length = option.value.len();
            let length_nibble = if length < 13 {
                length as u8
            } else if length < 269 {
                13
            } else {
                14
            };

            // Encode option header
            let option_header = (delta_nibble << 4) | length_nibble;
            buffer
                .push(option_header)
                .map_err(|_| CoapError::EncodingFailed)?;

            // Extended delta
            if delta_nibble == 13 {
                buffer
                    .push((delta - 13) as u8)
                    .map_err(|_| CoapError::EncodingFailed)?;
            } else if delta_nibble == 14 {
                let extended = delta - 269;
                buffer
                    .push((extended >> 8) as u8)
                    .map_err(|_| CoapError::EncodingFailed)?;
                buffer
                    .push(extended as u8)
                    .map_err(|_| CoapError::EncodingFailed)?;
            }

            // Extended length
            if length_nibble == 13 {
                buffer
                    .push((length - 13) as u8)
                    .map_err(|_| CoapError::EncodingFailed)?;
            } else if length_nibble == 14 {
                let extended = length - 269;
                buffer
                    .push((extended >> 8) as u8)
                    .map_err(|_| CoapError::EncodingFailed)?;
                buffer
                    .push(extended as u8)
                    .map_err(|_| CoapError::EncodingFailed)?;
            }

            // Encode option value
            buffer
                .extend_from_slice(&option.value)
                .map_err(|_| CoapError::EncodingFailed)?;

            prev_option_number = option.number;
        }

        // Payload marker and payload
        if !self.payload.is_empty() {
            buffer.push(0xff).map_err(|_| CoapError::EncodingFailed)?;
            buffer
                .extend_from_slice(&self.payload)
                .map_err(|_| CoapError::EncodingFailed)?;
        }

        Ok(buffer)
    }

    /// Decode message from bytes
    pub fn decode(bytes: &[u8]) -> Result<Self, CoapError> {
        if bytes.len() < 4 {
            return Err(CoapError::MessageTooShort);
        }

        // Decode header
        let header = CoapHeader::decode(bytes)?;

        let mut offset = 4;

        // Decode token
        let token_end = offset + header.token_length as usize;
        if token_end > bytes.len() {
            return Err(CoapError::MessageTooShort);
        }

        let mut token = heapless::Vec::new();
        token
            .extend_from_slice(&bytes[offset..token_end])
            .map_err(|_| CoapError::TokenTooLong)?;
        offset = token_end;

        // Decode options
        let mut options = heapless::Vec::new();
        let mut prev_option_number = 0u16;

        while offset < bytes.len() && bytes[offset] != 0xff {
            let option_header = bytes[offset];
            offset += 1;

            let mut delta = ((option_header >> 4) & 0x0f) as u16;
            let mut length = (option_header & 0x0f) as usize;

            // Extended delta
            if delta == 13 {
                if offset >= bytes.len() {
                    return Err(CoapError::MessageTooShort);
                }
                delta = bytes[offset] as u16 + 13;
                offset += 1;
            } else if delta == 14 {
                if offset + 1 >= bytes.len() {
                    return Err(CoapError::MessageTooShort);
                }
                delta = ((bytes[offset] as u16) << 8) | ((bytes[offset + 1] as u16) + 269);
                offset += 2;
            } else if delta == 15 {
                return Err(CoapError::InvalidOptionDelta);
            }

            // Extended length
            if length == 13 {
                if offset >= bytes.len() {
                    return Err(CoapError::MessageTooShort);
                }
                length = bytes[offset] as usize + 13;
                offset += 1;
            } else if length == 14 {
                if offset + 1 >= bytes.len() {
                    return Err(CoapError::MessageTooShort);
                }
                length = (((bytes[offset] as usize) << 8) | (bytes[offset + 1] as usize)) + 269;
                offset += 2;
            } else if length == 15 {
                return Err(CoapError::InvalidOptionLength);
            }

            // Decode option value
            if offset + length > bytes.len() {
                return Err(CoapError::MessageTooShort);
            }

            let option_number = prev_option_number + delta;
            let mut value = heapless::Vec::new();
            value
                .extend_from_slice(&bytes[offset..offset + length])
                .map_err(|_| CoapError::OptionTooLarge)?;

            options
                .push(CoapOption {
                    number: option_number,
                    value,
                })
                .map_err(|_| CoapError::TooManyOptions)?;

            prev_option_number = option_number;
            offset += length;
        }

        // Decode payload
        let mut payload = heapless::Vec::new();
        if offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1; // Skip payload marker
            payload
                .extend_from_slice(&bytes[offset..])
                .map_err(|_| CoapError::PayloadTooLarge)?;
        }

        Ok(Self {
            header,
            token,
            options,
            payload,
        })
    }
}

/// CoAP Request Builder
#[derive(Debug, Clone)]
pub struct CoapRequest {
    method: Method,
    uri_path: heapless::String<256>,
    uri_query: heapless::String<256>,
    token: heapless::Vec<u8, 8>,
    content_format: Option<ContentFormat>,
    accept: Option<ContentFormat>,
    observe: Option<bool>,
    payload: heapless::Vec<u8, 1024>,
}

impl CoapRequest {
    /// Create a new request
    pub fn new(method: Method) -> Self {
        Self {
            method,
            uri_path: heapless::String::new(),
            uri_query: heapless::String::new(),
            token: heapless::Vec::new(),
            content_format: None,
            accept: None,
            observe: None,
            payload: heapless::Vec::new(),
        }
    }

    /// Set URI path
    pub fn with_uri_path(mut self, path: &str) -> Self {
        self.uri_path.clear();
        let _ = self.uri_path.push_str(path);
        self
    }

    /// Set URI query
    pub fn with_uri_query(mut self, query: &str) -> Self {
        self.uri_query.clear();
        let _ = self.uri_query.push_str(query);
        self
    }

    /// Set token
    pub fn with_token(mut self, token: &[u8]) -> Self {
        self.token.clear();
        let _ = self.token.extend_from_slice(token);
        self
    }

    /// Set content format
    pub fn with_content_format(mut self, format: ContentFormat) -> Self {
        self.content_format = Some(format);
        self
    }

    /// Set accept format
    pub fn with_accept(mut self, format: ContentFormat) -> Self {
        self.accept = Some(format);
        self
    }

    /// Set observe flag
    pub fn with_observe(mut self, register: bool) -> Self {
        self.observe = Some(register);
        self
    }

    /// Set payload
    pub fn with_payload(mut self, payload: &[u8]) -> Result<Self, CoapError> {
        self.payload.clear();
        self.payload
            .extend_from_slice(payload)
            .map_err(|_| CoapError::PayloadTooLarge)?;
        Ok(self)
    }

    /// Build CoAP message
    pub fn build(self, message_id: u16) -> Result<CoapMessage, CoapError> {
        let msg_type = MessageType::Confirmable;
        let code = self.method.to_code();

        let mut message = CoapMessage::new(msg_type, code, message_id);

        // Set token
        if !self.token.is_empty() {
            message = message.with_token(&self.token)?;
        }

        // Add URI-Path options
        if !self.uri_path.is_empty() {
            for segment in self.uri_path.split('/') {
                if !segment.is_empty() {
                    message.add_option(CoapOption::uri_path(segment)?)?;
                }
            }
        }

        // Add URI-Query options
        if !self.uri_query.is_empty() {
            for param in self.uri_query.split('&') {
                if !param.is_empty() {
                    message
                        .add_option(CoapOption::new(OptionNumber::UriQuery, param.as_bytes())?)?;
                }
            }
        }

        // Add Content-Format option
        if let Some(format) = self.content_format {
            message.add_option(CoapOption::content_format(format)?)?;
        }

        // Add Accept option
        if let Some(format) = self.accept {
            let value = format.to_u16();
            if value < 256 {
                message.add_option(CoapOption::new(OptionNumber::Accept, &[value as u8])?)?;
            } else {
                message.add_option(CoapOption::new(
                    OptionNumber::Accept,
                    &[(value >> 8) as u8, value as u8],
                )?)?;
            }
        }

        // Add Observe option
        if let Some(register) = self.observe {
            message.add_option(CoapOption::observe(register)?)?;
        }

        // Set payload
        if !self.payload.is_empty() {
            message = message.with_payload(&self.payload)?;
        }

        Ok(message)
    }
}

/// CoAP Response
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoapResponse {
    /// Response code
    pub code: ResponseCode,
    /// Token
    pub token: heapless::Vec<u8, 8>,
    /// Options
    pub options: heapless::Vec<CoapOption, 16>,
    /// Payload
    pub payload: heapless::Vec<u8, 1024>,
}

impl CoapResponse {
    /// Create from CoAP message
    pub fn from_message(message: CoapMessage) -> Result<Self, CoapError> {
        let code = ResponseCode::from_u8(message.header.code)?;

        Ok(Self {
            code,
            token: message.token,
            options: message.options,
            payload: message.payload,
        })
    }

    /// Get payload as string
    pub fn payload_str(&self) -> Result<&str, CoapError> {
        core::str::from_utf8(&self.payload).map_err(|_| CoapError::InvalidPayload)
    }

    /// Find option by number
    pub fn find_option(&self, number: OptionNumber) -> Option<&CoapOption> {
        self.options
            .iter()
            .find(|opt| opt.number == number.to_u16())
    }

    /// Get content format
    pub fn content_format(&self) -> Option<ContentFormat> {
        self.find_option(OptionNumber::ContentFormat)
            .and_then(|opt| opt.as_u32().ok())
            .and_then(|val| ContentFormat::from_u16(val as u16).ok())
    }
}

/// CoAP Client
#[derive(Debug)]
pub struct CoapClient {
    /// Next message ID
    message_id: u16,
    /// Pending requests (for retransmission)
    pending: heapless::Vec<PendingRequest, 8>,
}

/// Pending request state
#[derive(Debug, Clone)]
struct PendingRequest {
    /// Message ID
    message_id: u16,
    /// Encoded message
    message: heapless::Vec<u8, MAX_MESSAGE_SIZE>,
    /// Retransmission count
    retransmit_count: u8,
    /// Next timeout (ms)
    timeout_ms: u32,
}

impl CoapClient {
    /// Create a new CoAP client
    pub fn new() -> Self {
        Self {
            message_id: 1,
            pending: heapless::Vec::new(),
        }
    }

    /// Get next message ID
    fn next_message_id(&mut self) -> u16 {
        let id = self.message_id;
        self.message_id = self.message_id.wrapping_add(1);
        id
    }

    /// Build and prepare request for sending
    pub fn prepare_request(
        &mut self,
        request: CoapRequest,
    ) -> Result<heapless::Vec<u8, MAX_MESSAGE_SIZE>, CoapError> {
        let message_id = self.next_message_id();
        let mut message = request.build(message_id)?;
        message.encode()
    }

    /// Add pending request for retransmission
    pub fn add_pending(
        &mut self,
        message_id: u16,
        message: heapless::Vec<u8, MAX_MESSAGE_SIZE>,
    ) -> Result<(), CoapError> {
        let pending = PendingRequest {
            message_id,
            message,
            retransmit_count: 0,
            timeout_ms: ACK_TIMEOUT_MS,
        };

        self.pending
            .push(pending)
            .map_err(|_| CoapError::TooManyPendingRequests)
    }

    /// Remove pending request
    pub fn remove_pending(&mut self, message_id: u16) {
        self.pending.retain(|req| req.message_id != message_id);
    }

    /// Process timeout and retransmissions
    pub fn process_timeouts(
        &mut self,
        elapsed_ms: u32,
    ) -> heapless::Vec<heapless::Vec<u8, MAX_MESSAGE_SIZE>, 8> {
        let mut to_retransmit = heapless::Vec::new();

        for pending in self.pending.iter_mut() {
            if pending.timeout_ms > elapsed_ms {
                pending.timeout_ms -= elapsed_ms;
            } else {
                // Timeout expired
                if pending.retransmit_count < MAX_RETRANSMIT {
                    // Retransmit
                    pending.retransmit_count += 1;
                    pending.timeout_ms = ACK_TIMEOUT_MS * 2u32.pow(pending.retransmit_count as u32);

                    let _ = to_retransmit.push(pending.message.clone());
                }
            }
        }

        // Remove expired requests (exceeded max retransmits)
        self.pending
            .retain(|req| req.retransmit_count <= MAX_RETRANSMIT);

        to_retransmit
    }

    /// Parse incoming response
    pub fn parse_response(&mut self, bytes: &[u8]) -> Result<CoapResponse, CoapError> {
        let message = CoapMessage::decode(bytes)?;

        // Remove from pending if ACK
        if message.header.msg_type == MessageType::Acknowledgment {
            self.remove_pending(message.header.message_id);
        }

        CoapResponse::from_message(message)
    }
}

impl Default for CoapClient {
    fn default() -> Self {
        Self::new()
    }
}

/// CoAP Server Resource
#[derive(Debug, Clone)]
pub struct CoapResource {
    /// Resource path (e.g., "/sensors/temperature")
    pub path: heapless::String<64>,
    /// Allowed methods
    pub methods: heapless::Vec<Method, 4>,
    /// Resource type
    pub resource_type: Option<heapless::String<32>>,
    /// Interface description
    pub interface: Option<heapless::String<32>>,
    /// Observable flag
    pub observable: bool,
}

impl CoapResource {
    /// Create a new resource
    pub fn new(path: &str) -> Result<Self, CoapError> {
        let mut path_str = heapless::String::new();
        path_str
            .push_str(path)
            .map_err(|_| CoapError::PathTooLong)?;

        Ok(Self {
            path: path_str,
            methods: heapless::Vec::new(),
            resource_type: None,
            interface: None,
            observable: false,
        })
    }

    /// Add allowed method
    pub fn with_method(mut self, method: Method) -> Self {
        let _ = self.methods.push(method);
        self
    }

    /// Set resource type
    pub fn with_resource_type(mut self, rt: &str) -> Self {
        let mut rt_str = heapless::String::new();
        let _ = rt_str.push_str(rt);
        self.resource_type = Some(rt_str);
        self
    }

    /// Set observable
    pub fn with_observable(mut self, observable: bool) -> Self {
        self.observable = observable;
        self
    }

    /// Check if method is allowed
    pub fn is_method_allowed(&self, method: Method) -> bool {
        self.methods.contains(&method)
    }
}

/// CoAP Server
#[derive(Debug)]
pub struct CoapServer {
    /// Registered resources
    resources: heapless::Vec<CoapResource, 16>,
}

impl CoapServer {
    /// Create a new CoAP server
    pub fn new() -> Self {
        Self {
            resources: heapless::Vec::new(),
        }
    }

    /// Register a resource
    pub fn register_resource(&mut self, resource: CoapResource) -> Result<(), CoapError> {
        self.resources
            .push(resource)
            .map_err(|_| CoapError::TooManyResources)
    }

    /// Find resource by path
    pub fn find_resource(&self, path: &str) -> Option<&CoapResource> {
        self.resources.iter().find(|res| res.path.as_str() == path)
    }

    /// Handle incoming request
    pub fn handle_request(&self, request_bytes: &[u8]) -> Result<CoapMessage, CoapError> {
        let request = CoapMessage::decode(request_bytes)?;

        // Extract method
        let method = Method::from_code(request.header.code)?;

        // Extract URI path
        let mut uri_path = heapless::String::<256>::new();
        for option in &request.options {
            if option.number == OptionNumber::UriPath.to_u16() {
                let _ = uri_path.push('/');
                let _ = uri_path.push_str(option.as_str()?);
            }
        }

        // Find resource
        let resource = self
            .find_resource(&uri_path)
            .ok_or(CoapError::ResourceNotFound)?;

        // Check if method is allowed
        if !resource.is_method_allowed(method) {
            // Return 4.05 Method Not Allowed
            let mut response = CoapMessage::new(
                MessageType::Acknowledgment,
                ResponseCode::MethodNotAllowed.to_u8(),
                request.header.message_id,
            );

            // Copy token
            if !request.token.is_empty() {
                response = response.with_token(&request.token)?;
            }

            return Ok(response);
        }

        // Create success response (application would provide actual data)
        let mut response = CoapMessage::new(
            MessageType::Acknowledgment,
            ResponseCode::Content.to_u8(),
            request.header.message_id,
        );

        // Copy token
        if !request.token.is_empty() {
            response = response.with_token(&request.token)?;
        }

        Ok(response)
    }

    /// Generate .well-known/core response (resource discovery)
    pub fn generate_core_link_format(&self) -> heapless::String<1024> {
        let mut result = heapless::String::new();

        for (i, resource) in self.resources.iter().enumerate() {
            if i > 0 {
                let _ = result.push(',');
            }

            let _ = result.push('<');
            let _ = result.push_str(&resource.path);
            let _ = result.push('>');

            if let Some(ref rt) = resource.resource_type {
                let _ = result.push_str(";rt=\"");
                let _ = result.push_str(rt);
                let _ = result.push('"');
            }

            if resource.observable {
                let _ = result.push_str(";obs");
            }
        }

        result
    }
}

impl Default for CoapServer {
    fn default() -> Self {
        Self::new()
    }
}

/// CoAP Error Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoapError {
    /// Invalid message type
    InvalidMessageType,
    /// Invalid method code
    InvalidMethod,
    /// Invalid response code
    InvalidResponseCode,
    /// Unknown option number
    UnknownOption,
    /// Unsupported CoAP version
    UnsupportedVersion,
    /// Token too long (>8 bytes)
    TokenTooLong,
    /// Message too short to parse
    MessageTooShort,
    /// Option too large
    OptionTooLarge,
    /// Too many options
    TooManyOptions,
    /// Payload too large
    PayloadTooLarge,
    /// Encoding failed
    EncodingFailed,
    /// Invalid option value
    InvalidOptionValue,
    /// Invalid option delta
    InvalidOptionDelta,
    /// Invalid option length
    InvalidOptionLength,
    /// Invalid payload
    InvalidPayload,
    /// Unsupported content format
    UnsupportedContentFormat,
    /// Path too long
    PathTooLong,
    /// Too many resources
    TooManyResources,
    /// Resource not found
    ResourceNotFound,
    /// Too many pending requests
    TooManyPendingRequests,
}

impl fmt::Display for CoapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoapError::InvalidMessageType => write!(f, "Invalid CoAP message type"),
            CoapError::InvalidMethod => write!(f, "Invalid CoAP method code"),
            CoapError::InvalidResponseCode => write!(f, "Invalid CoAP response code"),
            CoapError::UnknownOption => write!(f, "Unknown CoAP option number"),
            CoapError::UnsupportedVersion => write!(f, "Unsupported CoAP version"),
            CoapError::TokenTooLong => write!(f, "CoAP token too long (max 8 bytes)"),
            CoapError::MessageTooShort => write!(f, "CoAP message too short to parse"),
            CoapError::OptionTooLarge => write!(f, "CoAP option value too large"),
            CoapError::TooManyOptions => write!(f, "Too many CoAP options"),
            CoapError::PayloadTooLarge => write!(f, "CoAP payload too large"),
            CoapError::EncodingFailed => write!(f, "CoAP message encoding failed"),
            CoapError::InvalidOptionValue => write!(f, "Invalid CoAP option value"),
            CoapError::InvalidOptionDelta => write!(f, "Invalid CoAP option delta"),
            CoapError::InvalidOptionLength => write!(f, "Invalid CoAP option length"),
            CoapError::InvalidPayload => write!(f, "Invalid CoAP payload"),
            CoapError::UnsupportedContentFormat => write!(f, "Unsupported content format"),
            CoapError::PathTooLong => write!(f, "Resource path too long"),
            CoapError::TooManyResources => write!(f, "Too many resources registered"),
            CoapError::ResourceNotFound => write!(f, "Resource not found"),
            CoapError::TooManyPendingRequests => write!(f, "Too many pending requests"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_message_type_conversion() {
        assert_eq!(MessageType::from_u8(0).unwrap(), MessageType::Confirmable);
        assert_eq!(
            MessageType::from_u8(1).unwrap(),
            MessageType::NonConfirmable
        );
        assert_eq!(
            MessageType::from_u8(2).unwrap(),
            MessageType::Acknowledgment
        );
        assert_eq!(MessageType::from_u8(3).unwrap(), MessageType::Reset);
    }

    #[test]
    fn test_method_conversion() {
        assert_eq!(Method::from_code(1).unwrap(), Method::Get);
        assert_eq!(Method::from_code(2).unwrap(), Method::Post);
        assert_eq!(Method::from_code(3).unwrap(), Method::Put);
        assert_eq!(Method::from_code(4).unwrap(), Method::Delete);

        assert_eq!(Method::Get.to_code(), 1);
        assert_eq!(Method::Post.to_code(), 2);
    }

    #[test]
    fn test_response_code_class() {
        assert_eq!(ResponseCode::Content.class(), 2);
        assert_eq!(ResponseCode::NotFound.class(), 4);
        assert_eq!(ResponseCode::InternalServerError.class(), 5);

        assert!(ResponseCode::Content.is_success());
        assert!(ResponseCode::NotFound.is_client_error());
        assert!(ResponseCode::InternalServerError.is_server_error());
    }

    #[test]
    fn test_header_encode_decode() {
        let header = CoapHeader::new(MessageType::Confirmable, 1, 0x1234);
        let header = header.with_token_length(4).unwrap();

        let encoded = header.encode();
        let decoded = CoapHeader::decode(&encoded).unwrap();

        assert_eq!(decoded.version, COAP_VERSION);
        assert_eq!(decoded.msg_type, MessageType::Confirmable);
        assert_eq!(decoded.token_length, 4);
        assert_eq!(decoded.code, 1);
        assert_eq!(decoded.message_id, 0x1234);
    }

    #[test]
    fn test_option_creation() {
        let opt = CoapOption::uri_path("/test").unwrap();
        assert_eq!(opt.number, 11);
        assert_eq!(opt.as_str().unwrap(), "/test");

        let opt = CoapOption::content_format(ContentFormat::ApplicationJson).unwrap();
        assert_eq!(opt.number, 12);
        assert_eq!(opt.as_u32().unwrap(), 50);
    }

    #[test]
    fn test_message_encode_decode() {
        let mut message = CoapMessage::new(MessageType::Confirmable, Method::Get.to_code(), 0x1234);
        message = message.with_token(&[0x12, 0x34]).unwrap();
        message
            .add_option(CoapOption::uri_path("sensors").unwrap())
            .unwrap();
        message
            .add_option(CoapOption::uri_path("temperature").unwrap())
            .unwrap();
        message = message.with_payload(b"25.5").unwrap();

        let encoded = message.encode().unwrap();
        let decoded = CoapMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.header.msg_type, MessageType::Confirmable);
        assert_eq!(decoded.header.code, Method::Get.to_code());
        assert_eq!(decoded.header.message_id, 0x1234);
        assert_eq!(decoded.token.as_slice(), &[0x12, 0x34]);
        assert_eq!(decoded.options.len(), 2);
        assert_eq!(decoded.payload.as_slice(), b"25.5");
    }

    #[test]
    fn test_request_builder() {
        let request = CoapRequest::new(Method::Get)
            .with_uri_path("/sensors/temperature")
            .with_token(&[0xab, 0xcd])
            .with_accept(ContentFormat::ApplicationJson);

        let message = request.build(0x5678).unwrap();
        assert_eq!(message.header.code, Method::Get.to_code());
        assert_eq!(message.header.message_id, 0x5678);
        assert_eq!(message.token.as_slice(), &[0xab, 0xcd]);
        assert!(message.options.len() >= 2); // URI-Path segments
    }

    #[test]
    fn test_client_message_id() {
        let mut client = CoapClient::new();
        let id1 = client.next_message_id();
        let id2 = client.next_message_id();
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    fn test_server_resource_registration() {
        let mut server = CoapServer::new();
        let resource = CoapResource::new("/sensors/temp")
            .unwrap()
            .with_method(Method::Get)
            .with_observable(true);

        server.register_resource(resource).unwrap();
        assert!(server.find_resource("/sensors/temp").is_some());
        assert!(server.find_resource("/nonexistent").is_none());
    }

    #[test]
    fn test_core_link_format() {
        let mut server = CoapServer::new();
        server
            .register_resource(
                CoapResource::new("/temp")
                    .unwrap()
                    .with_resource_type("temperature")
                    .with_observable(true),
            )
            .unwrap();

        let link_format = server.generate_core_link_format();
        assert!(link_format.contains("</temp>"));
        assert!(link_format.contains("rt=\"temperature\""));
        assert!(link_format.contains("obs"));
    }

    #[test]
    fn test_response_from_message() {
        let mut message = CoapMessage::new(
            MessageType::Acknowledgment,
            ResponseCode::Content.to_u8(),
            0x1234,
        );
        message = message.with_payload(b"test data").unwrap();

        let response = CoapResponse::from_message(message).unwrap();
        assert_eq!(response.code, ResponseCode::Content);
        assert_eq!(response.payload_str().unwrap(), "test data");
    }

    #[test]
    fn test_option_max_age() {
        let opt = CoapOption::max_age(60).unwrap();
        assert_eq!(opt.as_u32().unwrap(), 60);

        let opt = CoapOption::max_age(0).unwrap();
        assert_eq!(opt.as_u32().unwrap(), 0);
    }

    #[test]
    fn test_observe_option() {
        let opt = CoapOption::observe(true).unwrap();
        assert_eq!(opt.number, OptionNumber::Observe.to_u16());
        assert_eq!(opt.value[0], 0);

        let opt = CoapOption::observe(false).unwrap();
        assert_eq!(opt.value[0], 1);
    }

    #[test]
    fn test_empty_message() {
        let message = CoapMessage::new(MessageType::Reset, 0, 0x9999);
        let encoded = message.clone().encode().unwrap();
        let decoded = CoapMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.header.msg_type, MessageType::Reset);
        assert_eq!(decoded.header.message_id, 0x9999);
        assert!(decoded.token.is_empty());
        assert!(decoded.options.is_empty());
        assert!(decoded.payload.is_empty());
    }

    #[test]
    fn test_pending_request_management() {
        let mut client = CoapClient::new();
        let message = heapless::Vec::from_slice(&[1, 2, 3, 4]).unwrap();

        client.add_pending(100, message).unwrap();
        assert_eq!(client.pending.len(), 1);

        client.remove_pending(100);
        assert_eq!(client.pending.len(), 0);
    }

    #[test]
    fn test_resource_method_allowed() {
        let resource = CoapResource::new("/test")
            .unwrap()
            .with_method(Method::Get)
            .with_method(Method::Post);

        assert!(resource.is_method_allowed(Method::Get));
        assert!(resource.is_method_allowed(Method::Post));
        assert!(!resource.is_method_allowed(Method::Put));
        assert!(!resource.is_method_allowed(Method::Delete));
    }

    #[test]
    fn test_content_format_conversion() {
        assert_eq!(
            ContentFormat::from_u16(0).unwrap(),
            ContentFormat::TextPlain
        );
        assert_eq!(
            ContentFormat::from_u16(50).unwrap(),
            ContentFormat::ApplicationJson
        );
        assert!(ContentFormat::from_u16(999).is_err());
    }

    #[test]
    fn test_request_with_query() {
        let request = CoapRequest::new(Method::Get)
            .with_uri_path("/sensors")
            .with_uri_query("type=temp&limit=10");

        let message = request.build(1).unwrap();
        let query_options: heapless::Vec<_, 8> = message
            .options
            .iter()
            .filter(|opt| opt.number == OptionNumber::UriQuery.to_u16())
            .collect();

        assert_eq!(query_options.len(), 2);
    }

    #[test]
    fn test_error_display() {
        let err = CoapError::ResourceNotFound;
        let display = format!("{}", err);
        assert!(display.contains("not found"));
    }

    #[test]
    fn test_message_with_multiple_options() {
        let mut message = CoapMessage::new(MessageType::Confirmable, Method::Get.to_code(), 1);
        message
            .add_option(CoapOption::uri_path("a").unwrap())
            .unwrap();
        message
            .add_option(CoapOption::uri_path("b").unwrap())
            .unwrap();
        message
            .add_option(CoapOption::uri_path("c").unwrap())
            .unwrap();
        message
            .add_option(CoapOption::content_format(ContentFormat::TextPlain).unwrap())
            .unwrap();

        let encoded = message.encode().unwrap();
        let decoded = CoapMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.options.len(), 4);
    }

    #[test]
    fn test_token_length_validation() {
        let long_token = [0u8; 9];
        let result = CoapMessage::new(MessageType::Confirmable, 1, 1).with_token(&long_token);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CoapError::TokenTooLong);
    }

    #[test]
    fn test_server_handle_not_found() {
        let server = CoapServer::new();
        let mut request = CoapMessage::new(MessageType::Confirmable, Method::Get.to_code(), 1);
        request
            .add_option(CoapOption::uri_path("nonexistent").unwrap())
            .unwrap();

        let encoded = request.encode().unwrap();
        let result = server.handle_request(&encoded);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), CoapError::ResourceNotFound);
    }

    #[test]
    fn test_server_handle_method_not_allowed() {
        let mut server = CoapServer::new();
        server
            .register_resource(
                CoapResource::new("/readonly")
                    .unwrap()
                    .with_method(Method::Get),
            )
            .unwrap();

        let mut request = CoapMessage::new(MessageType::Confirmable, Method::Put.to_code(), 1);
        request
            .add_option(CoapOption::uri_path("readonly").unwrap())
            .unwrap();

        let encoded = request.encode().unwrap();
        let response = server.handle_request(&encoded).unwrap();

        assert_eq!(response.header.code, ResponseCode::MethodNotAllowed.to_u8());
    }

    #[test]
    fn test_client_prepare_request() {
        let mut client = CoapClient::new();
        let request = CoapRequest::new(Method::Post)
            .with_uri_path("/data")
            .with_payload(b"test")
            .unwrap();

        let encoded = client.prepare_request(request).unwrap();
        assert!(!encoded.is_empty());

        let decoded = CoapMessage::decode(&encoded).unwrap();
        assert_eq!(decoded.header.code, Method::Post.to_code());
    }
}
