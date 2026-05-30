//! Security Module for MielinMesh v0.3.0
//!
//! Provides comprehensive security features:
//! - Mutual TLS (mTLS) for all connections
//! - Node identity verification with public key cryptography
//! - Access Control Lists (ACLs) for permission management
//! - Encrypted gossip protocol messages
//!
//! # Architecture
//!
//! The security module implements a defense-in-depth approach:
//!
//! 1. **Transport Security**: mTLS ensures all connections are authenticated
//!    and encrypted at the transport layer.
//!
//! 2. **Identity Verification**: Each node has a cryptographic identity that
//!    can be verified using Ed25519 signatures.
//!
//! 3. **Access Control**: Fine-grained ACLs control what operations each
//!    node or agent can perform.
//!
//! 4. **Message Encryption**: Gossip messages are encrypted using AES-256-GCM
//!    with per-cluster symmetric keys.

use crate::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::RwLock;

pub mod acl;
pub mod cert;
pub mod crypto;
pub mod identity;
pub mod kex;

pub use acl::*;
pub use cert::*;
pub use crypto::*;
pub use identity::*;
pub use kex::*;

#[cfg(test)]
mod tests;

// =============================================================================
// Security Error Types
// =============================================================================

/// Comprehensive security error types
#[derive(Debug, Error)]
pub enum SecurityError {
    /// Certificate-related errors
    #[error("Certificate error: {details}")]
    CertificateError { details: String },

    /// Signature verification failed
    #[error("Signature verification failed: {details}")]
    SignatureVerificationFailed { details: String },

    /// Invalid public key
    #[error("Invalid public key: {details}")]
    InvalidPublicKey { details: String },

    /// Key generation failed
    #[error("Key generation failed: {details}")]
    KeyGenerationFailed { details: String },

    /// Encryption failed
    #[error("Encryption failed: {details}")]
    EncryptionFailed { details: String },

    /// Decryption failed
    #[error("Decryption failed: {details}")]
    DecryptionFailed { details: String },

    /// Access denied by ACL
    #[error("Access denied: {operation} not permitted for {subject}")]
    AccessDenied { subject: String, operation: String },

    /// Identity not found
    #[error("Identity not found: {node_id}")]
    IdentityNotFound { node_id: String },

    /// Certificate chain validation failed
    #[error("Certificate chain validation failed: {details}")]
    CertificateChainInvalid { details: String },

    /// Certificate expired
    #[error("Certificate expired: valid until {expiry:?}")]
    CertificateExpired { expiry: SystemTime },

    /// Certificate not yet valid
    #[error("Certificate not yet valid: valid from {valid_from:?}")]
    CertificateNotYetValid { valid_from: SystemTime },

    /// Certificate revoked
    #[error("Certificate revoked: serial {serial}")]
    CertificateRevoked { serial: String },

    /// Key exchange failed
    #[error("Key exchange failed: {details}")]
    KeyExchangeFailed { details: String },

    /// Configuration error
    #[error("Security configuration error: {details}")]
    ConfigurationError { details: String },

    /// TLS handshake failed
    #[error("TLS handshake failed: {details}")]
    TlsHandshakeFailed { details: String },

    /// Nonce reuse detected
    #[error("Nonce reuse detected - potential replay attack")]
    NonceReuse,

    /// Message too old (potential replay)
    #[error("Message timestamp too old: {age_secs}s")]
    MessageTooOld { age_secs: u64 },

    /// Internal cryptographic error
    #[error("Cryptographic error: {details}")]
    CryptoError { details: String },
}

/// Result type for security operations
pub type SecurityResult<T> = Result<T, SecurityError>;
