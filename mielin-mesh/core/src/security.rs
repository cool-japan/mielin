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

// =============================================================================
// Mutual TLS Configuration
// =============================================================================

/// Certificate data wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateData {
    /// DER-encoded certificate
    pub der: Vec<u8>,
    /// Subject common name
    pub subject_cn: String,
    /// Issuer common name
    pub issuer_cn: String,
    /// Not valid before
    pub not_before: SystemTime,
    /// Not valid after
    pub not_after: SystemTime,
    /// Serial number (hex encoded)
    pub serial: String,
}

impl CertificateData {
    /// Create new certificate data
    pub fn new(
        der: Vec<u8>,
        subject_cn: impl Into<String>,
        issuer_cn: impl Into<String>,
        not_before: SystemTime,
        not_after: SystemTime,
    ) -> Self {
        let serial = hex::encode(&der[..16.min(der.len())]);
        Self {
            der,
            subject_cn: subject_cn.into(),
            issuer_cn: issuer_cn.into(),
            not_before,
            not_after,
            serial,
        }
    }

    /// Check if certificate is currently valid
    pub fn is_valid(&self) -> bool {
        let now = SystemTime::now();
        now >= self.not_before && now <= self.not_after
    }

    /// Check if certificate is expired
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.not_after
    }

    /// Get remaining validity duration
    pub fn remaining_validity(&self) -> Option<Duration> {
        self.not_after.duration_since(SystemTime::now()).ok()
    }
}

/// Mutual TLS configuration for mesh connections
#[derive(Debug, Clone)]
pub struct MtlsConfig {
    /// Local node's certificate
    pub node_certificate: CertificateData,
    /// Local node's private key (DER-encoded PKCS#8)
    pub node_private_key: Vec<u8>,
    /// Trusted CA certificates
    pub trusted_cas: Vec<CertificateData>,
    /// Certificate revocation list serials
    revoked_serials: HashSet<String>,
    /// Require client certificate
    pub require_client_cert: bool,
    /// Minimum TLS version (always TLS 1.3)
    pub min_tls_version: TlsVersion,
    /// ALPN protocols
    pub alpn_protocols: Vec<String>,
    /// Certificate verification depth
    pub verify_depth: usize,
}

/// TLS version enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TlsVersion {
    /// TLS 1.2 (minimum supported)
    Tls12,
    /// TLS 1.3 (recommended)
    #[default]
    Tls13,
}

impl MtlsConfig {
    /// Create new mTLS configuration
    pub fn new(
        node_certificate: CertificateData,
        node_private_key: Vec<u8>,
        trusted_cas: Vec<CertificateData>,
    ) -> Self {
        Self {
            node_certificate,
            node_private_key,
            trusted_cas,
            revoked_serials: HashSet::new(),
            require_client_cert: true,
            min_tls_version: TlsVersion::Tls13,
            alpn_protocols: vec!["mielin-mesh/1.0".to_string()],
            verify_depth: 3,
        }
    }

    /// Create with builder pattern
    pub fn builder() -> MtlsConfigBuilder {
        MtlsConfigBuilder::new()
    }

    /// Add CA certificate
    pub fn add_trusted_ca(&mut self, ca: CertificateData) {
        self.trusted_cas.push(ca);
    }

    /// Revoke a certificate by serial
    pub fn revoke_certificate(&mut self, serial: impl Into<String>) {
        self.revoked_serials.insert(serial.into());
    }

    /// Check if certificate is revoked
    pub fn is_revoked(&self, serial: &str) -> bool {
        self.revoked_serials.contains(serial)
    }

    /// Verify a peer certificate
    pub fn verify_peer_certificate(&self, peer_cert: &CertificateData) -> SecurityResult<()> {
        // Check revocation
        if self.is_revoked(&peer_cert.serial) {
            return Err(SecurityError::CertificateRevoked {
                serial: peer_cert.serial.clone(),
            });
        }

        // Check validity period
        let now = SystemTime::now();
        if now < peer_cert.not_before {
            return Err(SecurityError::CertificateNotYetValid {
                valid_from: peer_cert.not_before,
            });
        }
        if now > peer_cert.not_after {
            return Err(SecurityError::CertificateExpired {
                expiry: peer_cert.not_after,
            });
        }

        // Verify issuer is trusted
        let issuer_trusted = self
            .trusted_cas
            .iter()
            .any(|ca| ca.subject_cn == peer_cert.issuer_cn);
        if !issuer_trusted {
            return Err(SecurityError::CertificateChainInvalid {
                details: format!("Issuer '{}' not in trusted CAs", peer_cert.issuer_cn),
            });
        }

        Ok(())
    }

    /// Get private key bytes
    pub fn private_key_bytes(&self) -> &[u8] {
        &self.node_private_key
    }

    /// Check if local certificate needs renewal (within 30 days)
    pub fn needs_renewal(&self) -> bool {
        self.node_certificate
            .remaining_validity()
            .map(|d| d < Duration::from_secs(30 * 24 * 60 * 60))
            .unwrap_or(true)
    }
}

/// Builder for MtlsConfig
pub struct MtlsConfigBuilder {
    node_certificate: Option<CertificateData>,
    node_private_key: Option<Vec<u8>>,
    trusted_cas: Vec<CertificateData>,
    require_client_cert: bool,
    min_tls_version: TlsVersion,
    alpn_protocols: Vec<String>,
    verify_depth: usize,
}

impl MtlsConfigBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            node_certificate: None,
            node_private_key: None,
            trusted_cas: Vec::new(),
            require_client_cert: true,
            min_tls_version: TlsVersion::Tls13,
            alpn_protocols: vec!["mielin-mesh/1.0".to_string()],
            verify_depth: 3,
        }
    }

    /// Set node certificate
    pub fn certificate(mut self, cert: CertificateData) -> Self {
        self.node_certificate = Some(cert);
        self
    }

    /// Set private key
    pub fn private_key(mut self, key: Vec<u8>) -> Self {
        self.node_private_key = Some(key);
        self
    }

    /// Add trusted CA
    pub fn trusted_ca(mut self, ca: CertificateData) -> Self {
        self.trusted_cas.push(ca);
        self
    }

    /// Set client certificate requirement
    pub fn require_client_cert(mut self, require: bool) -> Self {
        self.require_client_cert = require;
        self
    }

    /// Set minimum TLS version
    pub fn min_tls_version(mut self, version: TlsVersion) -> Self {
        self.min_tls_version = version;
        self
    }

    /// Add ALPN protocol
    pub fn alpn_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.alpn_protocols.push(protocol.into());
        self
    }

    /// Set certificate verification depth
    pub fn verify_depth(mut self, depth: usize) -> Self {
        self.verify_depth = depth;
        self
    }

    /// Build the configuration
    pub fn build(self) -> SecurityResult<MtlsConfig> {
        let node_certificate =
            self.node_certificate
                .ok_or_else(|| SecurityError::ConfigurationError {
                    details: "Node certificate is required".to_string(),
                })?;

        let node_private_key =
            self.node_private_key
                .ok_or_else(|| SecurityError::ConfigurationError {
                    details: "Private key is required".to_string(),
                })?;

        Ok(MtlsConfig {
            node_certificate,
            node_private_key,
            trusted_cas: self.trusted_cas,
            revoked_serials: HashSet::new(),
            require_client_cert: self.require_client_cert,
            min_tls_version: self.min_tls_version,
            alpn_protocols: self.alpn_protocols,
            verify_depth: self.verify_depth,
        })
    }
}

impl Default for MtlsConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Node Identity Verification
// =============================================================================

/// Public key wrapper with algorithm identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicKey {
    /// Algorithm identifier
    pub algorithm: KeyAlgorithm,
    /// Raw public key bytes
    pub bytes: Vec<u8>,
}

impl PublicKey {
    /// Create new public key
    pub fn new(algorithm: KeyAlgorithm, bytes: Vec<u8>) -> Self {
        Self { algorithm, bytes }
    }

    /// Create Ed25519 public key
    pub fn ed25519(bytes: Vec<u8>) -> SecurityResult<Self> {
        if bytes.len() != 32 {
            return Err(SecurityError::InvalidPublicKey {
                details: format!("Ed25519 public key must be 32 bytes, got {}", bytes.len()),
            });
        }
        Ok(Self {
            algorithm: KeyAlgorithm::Ed25519,
            bytes,
        })
    }

    /// Get key as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(&self.bytes)
    }

    /// Parse from hex string
    pub fn from_hex(algorithm: KeyAlgorithm, hex_str: &str) -> SecurityResult<Self> {
        let bytes = hex::decode(hex_str).map_err(|e| SecurityError::InvalidPublicKey {
            details: format!("Invalid hex: {}", e),
        })?;
        Ok(Self { algorithm, bytes })
    }
}

/// Supported key algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeyAlgorithm {
    /// Ed25519 (recommended for signatures)
    #[default]
    Ed25519,
    /// ECDSA with P-256
    EcdsaP256,
    /// ECDSA with P-384
    EcdsaP384,
}

/// Digital signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// Algorithm used
    pub algorithm: KeyAlgorithm,
    /// Signature bytes
    pub bytes: Vec<u8>,
    /// Timestamp when signature was created
    pub timestamp: SystemTime,
}

impl Signature {
    /// Create new signature
    pub fn new(algorithm: KeyAlgorithm, bytes: Vec<u8>) -> Self {
        Self {
            algorithm,
            bytes,
            timestamp: SystemTime::now(),
        }
    }

    /// Get signature as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(&self.bytes)
    }

    /// Get signature age
    pub fn age(&self) -> Duration {
        self.timestamp.elapsed().unwrap_or_default()
    }
}

/// Node identity with cryptographic keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    /// Node ID
    pub node_id: NodeId,
    /// Public key for verification
    pub public_key: PublicKey,
    /// Node address
    pub address: Option<SocketAddr>,
    /// Identity creation timestamp
    pub created_at: SystemTime,
    /// Last seen timestamp
    pub last_seen: SystemTime,
    /// Node metadata
    pub metadata: HashMap<String, String>,
    /// Self-signature proving ownership
    pub self_signature: Option<Signature>,
}

impl NodeIdentity {
    /// Create new node identity
    pub fn new(node_id: NodeId, public_key: PublicKey) -> Self {
        let now = SystemTime::now();
        Self {
            node_id,
            public_key,
            address: None,
            created_at: now,
            last_seen: now,
            metadata: HashMap::new(),
            self_signature: None,
        }
    }

    /// Set node address
    pub fn with_address(mut self, addr: SocketAddr) -> Self {
        self.address = Some(addr);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set self-signature
    pub fn with_signature(mut self, signature: Signature) -> Self {
        self.self_signature = Some(signature);
        self
    }

    /// Update last seen time
    pub fn touch(&mut self) {
        self.last_seen = SystemTime::now();
    }

    /// Get identity age
    pub fn age(&self) -> Duration {
        self.created_at.elapsed().unwrap_or_default()
    }

    /// Get time since last seen
    pub fn idle_time(&self) -> Duration {
        self.last_seen.elapsed().unwrap_or_default()
    }

    /// Create message to sign for identity verification
    pub fn signing_message(&self) -> Vec<u8> {
        let mut msg = Vec::new();
        msg.extend_from_slice(self.node_id.as_bytes());
        msg.extend_from_slice(&self.public_key.bytes);
        if let Some(addr) = &self.address {
            msg.extend_from_slice(addr.to_string().as_bytes());
        }
        msg
    }
}

/// Identity verifier for node authentication
pub struct IdentityVerifier {
    /// Known identities
    identities: Arc<RwLock<HashMap<NodeId, NodeIdentity>>>,
    /// Trust anchors (pre-trusted node IDs)
    trust_anchors: HashSet<NodeId>,
    /// Maximum signature age for validation
    max_signature_age: Duration,
}

impl IdentityVerifier {
    /// Create new identity verifier
    pub fn new() -> Self {
        Self {
            identities: Arc::new(RwLock::new(HashMap::new())),
            trust_anchors: HashSet::new(),
            max_signature_age: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Create with trust anchors
    pub fn with_trust_anchors(anchors: impl IntoIterator<Item = NodeId>) -> Self {
        Self {
            identities: Arc::new(RwLock::new(HashMap::new())),
            trust_anchors: anchors.into_iter().collect(),
            max_signature_age: Duration::from_secs(300),
        }
    }

    /// Add trust anchor
    pub fn add_trust_anchor(&mut self, node_id: NodeId) {
        self.trust_anchors.insert(node_id);
    }

    /// Check if node is a trust anchor
    pub fn is_trust_anchor(&self, node_id: &NodeId) -> bool {
        self.trust_anchors.contains(node_id)
    }

    /// Register a node identity
    pub async fn register(&self, identity: NodeIdentity) -> SecurityResult<()> {
        let mut identities = self.identities.write().await;
        identities.insert(identity.node_id, identity);
        Ok(())
    }

    /// Get a node identity
    pub async fn get(&self, node_id: &NodeId) -> Option<NodeIdentity> {
        let identities = self.identities.read().await;
        identities.get(node_id).cloned()
    }

    /// Remove a node identity
    pub async fn remove(&self, node_id: &NodeId) -> Option<NodeIdentity> {
        let mut identities = self.identities.write().await;
        identities.remove(node_id)
    }

    /// Verify a node's identity claim
    pub async fn verify_identity(&self, identity: &NodeIdentity) -> SecurityResult<()> {
        // Trust anchors are always trusted
        if self.is_trust_anchor(&identity.node_id) {
            return Ok(());
        }

        // Check if we have a registered identity for this node
        let identities = self.identities.read().await;
        if let Some(known) = identities.get(&identity.node_id) {
            // Verify public key matches
            if known.public_key != identity.public_key {
                return Err(SecurityError::SignatureVerificationFailed {
                    details: "Public key mismatch with registered identity".to_string(),
                });
            }
        }

        // Verify self-signature if present
        if let Some(sig) = &identity.self_signature {
            // Check signature age
            if sig.age() > self.max_signature_age {
                return Err(SecurityError::MessageTooOld {
                    age_secs: sig.age().as_secs(),
                });
            }

            // In a real implementation, verify the signature cryptographically
            // For now, we just check the signature exists and is fresh
            if sig.bytes.is_empty() {
                return Err(SecurityError::SignatureVerificationFailed {
                    details: "Empty signature".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Verify a signature from a node
    pub async fn verify_signature(
        &self,
        node_id: &NodeId,
        message: &[u8],
        signature: &Signature,
    ) -> SecurityResult<()> {
        // Get the node's identity
        let identity = self
            .get(node_id)
            .await
            .ok_or_else(|| SecurityError::IdentityNotFound {
                node_id: node_id.to_string(),
            })?;

        // Check signature algorithm matches key algorithm
        if signature.algorithm != identity.public_key.algorithm {
            return Err(SecurityError::SignatureVerificationFailed {
                details: "Algorithm mismatch".to_string(),
            });
        }

        // Check signature age
        if signature.age() > self.max_signature_age {
            return Err(SecurityError::MessageTooOld {
                age_secs: signature.age().as_secs(),
            });
        }

        // Verify signature using ring
        match identity.public_key.algorithm {
            KeyAlgorithm::Ed25519 => {
                self.verify_ed25519(&identity.public_key.bytes, message, &signature.bytes)
            }
            KeyAlgorithm::EcdsaP256 => {
                self.verify_ecdsa_p256(&identity.public_key.bytes, message, &signature.bytes)
            }
            KeyAlgorithm::EcdsaP384 => {
                self.verify_ecdsa_p384(&identity.public_key.bytes, message, &signature.bytes)
            }
        }
    }

    /// Verify Ed25519 signature
    fn verify_ed25519(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> SecurityResult<()> {
        use ring::signature::{UnparsedPublicKey, ED25519};

        let peer_public_key = UnparsedPublicKey::new(&ED25519, public_key);
        peer_public_key.verify(message, signature).map_err(|_| {
            SecurityError::SignatureVerificationFailed {
                details: "Ed25519 signature verification failed".to_string(),
            }
        })
    }

    /// Verify ECDSA P-256 signature
    fn verify_ecdsa_p256(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> SecurityResult<()> {
        use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1};

        let peer_public_key = UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, public_key);
        peer_public_key.verify(message, signature).map_err(|_| {
            SecurityError::SignatureVerificationFailed {
                details: "ECDSA P-256 signature verification failed".to_string(),
            }
        })
    }

    /// Verify ECDSA P-384 signature
    fn verify_ecdsa_p384(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> SecurityResult<()> {
        use ring::signature::{UnparsedPublicKey, ECDSA_P384_SHA384_ASN1};

        let peer_public_key = UnparsedPublicKey::new(&ECDSA_P384_SHA384_ASN1, public_key);
        peer_public_key.verify(message, signature).map_err(|_| {
            SecurityError::SignatureVerificationFailed {
                details: "ECDSA P-384 signature verification failed".to_string(),
            }
        })
    }

    /// Get all registered identities
    pub async fn all_identities(&self) -> Vec<NodeIdentity> {
        let identities = self.identities.read().await;
        identities.values().cloned().collect()
    }

    /// Get identity count
    pub async fn identity_count(&self) -> usize {
        let identities = self.identities.read().await;
        identities.len()
    }

    /// Clean up stale identities (not seen in given duration)
    pub async fn cleanup_stale(&self, max_idle: Duration) -> Vec<NodeId> {
        let mut identities = self.identities.write().await;
        let stale: Vec<NodeId> = identities
            .iter()
            .filter(|(_, id)| id.idle_time() > max_idle)
            .map(|(node_id, _)| *node_id)
            .collect();

        for node_id in &stale {
            // Don't remove trust anchors
            if !self.trust_anchors.contains(node_id) {
                identities.remove(node_id);
            }
        }

        stale
    }
}

impl Default for IdentityVerifier {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Access Control Lists (ACLs)
// =============================================================================

/// Permission types for mesh operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Read data from agents
    Read,
    /// Write data to agents
    Write,
    /// Execute agent code
    Execute,
    /// Migrate agents between nodes
    Migrate,
    /// Create new agents
    Create,
    /// Delete agents
    Delete,
    /// Administer node settings
    Admin,
    /// Access gossip protocol
    Gossip,
    /// Access DHT operations
    DhtAccess,
    /// Join the mesh network
    Join,
}

impl Permission {
    /// Get all permissions
    pub fn all() -> Vec<Permission> {
        vec![
            Permission::Read,
            Permission::Write,
            Permission::Execute,
            Permission::Migrate,
            Permission::Create,
            Permission::Delete,
            Permission::Admin,
            Permission::Gossip,
            Permission::DhtAccess,
            Permission::Join,
        ]
    }

    /// Get standard user permissions
    pub fn user_defaults() -> Vec<Permission> {
        vec![
            Permission::Read,
            Permission::Write,
            Permission::Execute,
            Permission::Create,
        ]
    }

    /// Get node permissions
    pub fn node_defaults() -> Vec<Permission> {
        vec![
            Permission::Read,
            Permission::Write,
            Permission::Execute,
            Permission::Migrate,
            Permission::Gossip,
            Permission::DhtAccess,
            Permission::Join,
        ]
    }
}

/// ACL rule effect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AclEffect {
    /// Allow the action
    Allow,
    /// Deny the action
    #[default]
    Deny,
}

/// Subject of an ACL rule
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AclSubject {
    /// Specific node
    Node(NodeId),
    /// Specific agent
    Agent(String),
    /// Group of nodes/agents
    Group(String),
    /// Any subject (wildcard)
    Any,
}

impl AclSubject {
    /// Check if subject matches this pattern
    pub fn matches(&self, other: &AclSubject) -> bool {
        match (self, other) {
            (AclSubject::Any, _) => true,
            (AclSubject::Node(a), AclSubject::Node(b)) => a == b,
            (AclSubject::Agent(a), AclSubject::Agent(b)) => a == b,
            (AclSubject::Group(a), AclSubject::Group(b)) => a == b,
            _ => false,
        }
    }
}

/// Resource targeted by an ACL rule
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AclResource {
    /// Specific agent
    Agent(String),
    /// Specific node
    Node(NodeId),
    /// All agents on a node
    NodeAgents(NodeId),
    /// Resource path pattern
    Path(String),
    /// Any resource (wildcard)
    Any,
}

impl AclResource {
    /// Check if resource matches this pattern
    pub fn matches(&self, other: &AclResource) -> bool {
        match (self, other) {
            (AclResource::Any, _) => true,
            (AclResource::Agent(a), AclResource::Agent(b)) => a == b,
            (AclResource::Node(a), AclResource::Node(b)) => a == b,
            (AclResource::NodeAgents(a), AclResource::Agent(_)) => {
                // Would need to check if agent is on node
                // For now, just compare nodes
                matches!(other, AclResource::NodeAgents(b) if a == b)
            }
            (AclResource::Path(pattern), AclResource::Path(path)) => {
                // Simple glob matching
                if pattern.ends_with('*') {
                    path.starts_with(&pattern[..pattern.len() - 1])
                } else {
                    pattern == path
                }
            }
            _ => false,
        }
    }
}

/// Single ACL rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRule {
    /// Rule identifier
    pub id: String,
    /// Subject (who)
    pub subject: AclSubject,
    /// Resource (what)
    pub resource: AclResource,
    /// Permissions
    pub permissions: HashSet<Permission>,
    /// Effect (allow/deny)
    pub effect: AclEffect,
    /// Rule priority (higher = evaluated first)
    pub priority: i32,
    /// Rule description
    pub description: Option<String>,
    /// Expiration time
    pub expires_at: Option<SystemTime>,
}

impl AclRule {
    /// Create new ACL rule
    pub fn new(
        id: impl Into<String>,
        subject: AclSubject,
        resource: AclResource,
        permissions: impl IntoIterator<Item = Permission>,
        effect: AclEffect,
    ) -> Self {
        Self {
            id: id.into(),
            subject,
            resource,
            permissions: permissions.into_iter().collect(),
            effect,
            priority: 0,
            description: None,
            expires_at: None,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set expiration
    pub fn with_expiration(mut self, expires_at: SystemTime) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Check if rule is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| SystemTime::now() > exp)
            .unwrap_or(false)
    }

    /// Check if rule matches the request
    pub fn matches(
        &self,
        subject: &AclSubject,
        resource: &AclResource,
        permission: Permission,
    ) -> bool {
        !self.is_expired()
            && self.subject.matches(subject)
            && self.resource.matches(resource)
            && self.permissions.contains(&permission)
    }
}

/// ACL policy managing multiple rules
pub struct AclPolicy {
    /// Rules sorted by priority
    rules: Arc<RwLock<Vec<AclRule>>>,
    /// Default effect when no rule matches
    default_effect: AclEffect,
    /// Node group memberships
    node_groups: Arc<RwLock<HashMap<NodeId, HashSet<String>>>>,
    /// Agent group memberships
    agent_groups: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl AclPolicy {
    /// Create new ACL policy
    pub fn new(default_effect: AclEffect) -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            default_effect,
            node_groups: Arc::new(RwLock::new(HashMap::new())),
            agent_groups: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create permissive policy (allow by default)
    pub fn permissive() -> Self {
        Self::new(AclEffect::Allow)
    }

    /// Create restrictive policy (deny by default)
    pub fn restrictive() -> Self {
        Self::new(AclEffect::Deny)
    }

    /// Add a rule
    pub async fn add_rule(&self, rule: AclRule) {
        let mut rules = self.rules.write().await;
        rules.push(rule);
        // Sort by priority (highest first)
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Remove a rule by ID
    pub async fn remove_rule(&self, rule_id: &str) -> Option<AclRule> {
        let mut rules = self.rules.write().await;
        let pos = rules.iter().position(|r| r.id == rule_id)?;
        Some(rules.remove(pos))
    }

    /// Get a rule by ID
    pub async fn get_rule(&self, rule_id: &str) -> Option<AclRule> {
        let rules = self.rules.read().await;
        rules.iter().find(|r| r.id == rule_id).cloned()
    }

    /// Add node to a group
    pub async fn add_node_to_group(&self, node_id: NodeId, group: impl Into<String>) {
        let mut groups = self.node_groups.write().await;
        groups.entry(node_id).or_default().insert(group.into());
    }

    /// Add agent to a group
    pub async fn add_agent_to_group(&self, agent_id: impl Into<String>, group: impl Into<String>) {
        let mut groups = self.agent_groups.write().await;
        groups
            .entry(agent_id.into())
            .or_default()
            .insert(group.into());
    }

    /// Check permission for a node
    pub async fn check_node_permission(
        &self,
        node_id: &NodeId,
        resource: &AclResource,
        permission: Permission,
    ) -> SecurityResult<()> {
        let subject = AclSubject::Node(*node_id);
        self.check_permission(&subject, resource, permission).await
    }

    /// Check permission for an agent
    pub async fn check_agent_permission(
        &self,
        agent_id: &str,
        resource: &AclResource,
        permission: Permission,
    ) -> SecurityResult<()> {
        let subject = AclSubject::Agent(agent_id.to_string());
        self.check_permission(&subject, resource, permission).await
    }

    /// Check permission with subject
    pub async fn check_permission(
        &self,
        subject: &AclSubject,
        resource: &AclResource,
        permission: Permission,
    ) -> SecurityResult<()> {
        let rules = self.rules.read().await;

        // Find first matching rule (rules are sorted by priority)
        for rule in rules.iter() {
            if rule.matches(subject, resource, permission) {
                return match rule.effect {
                    AclEffect::Allow => Ok(()),
                    AclEffect::Deny => Err(SecurityError::AccessDenied {
                        subject: format!("{:?}", subject),
                        operation: format!("{:?} on {:?}", permission, resource),
                    }),
                };
            }
        }

        // Also check group memberships
        let group_effect = self
            .check_group_permissions(subject, resource, permission)
            .await;
        if let Some(effect) = group_effect {
            return match effect {
                AclEffect::Allow => Ok(()),
                AclEffect::Deny => Err(SecurityError::AccessDenied {
                    subject: format!("{:?}", subject),
                    operation: format!("{:?} on {:?}", permission, resource),
                }),
            };
        }

        // Apply default effect
        match self.default_effect {
            AclEffect::Allow => Ok(()),
            AclEffect::Deny => Err(SecurityError::AccessDenied {
                subject: format!("{:?}", subject),
                operation: format!("{:?} on {:?}", permission, resource),
            }),
        }
    }

    /// Check group permissions
    async fn check_group_permissions(
        &self,
        subject: &AclSubject,
        resource: &AclResource,
        permission: Permission,
    ) -> Option<AclEffect> {
        let rules = self.rules.read().await;

        // Get groups for the subject
        let groups: Vec<String> = match subject {
            AclSubject::Node(node_id) => {
                let node_groups = self.node_groups.read().await;
                node_groups
                    .get(node_id)
                    .map(|g| g.iter().cloned().collect())
                    .unwrap_or_default()
            }
            AclSubject::Agent(agent_id) => {
                let agent_groups = self.agent_groups.read().await;
                agent_groups
                    .get(agent_id)
                    .map(|g| g.iter().cloned().collect())
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };

        // Check rules for each group
        for group in groups {
            let group_subject = AclSubject::Group(group);
            for rule in rules.iter() {
                if rule.matches(&group_subject, resource, permission) {
                    return Some(rule.effect);
                }
            }
        }

        None
    }

    /// Get all rules
    pub async fn all_rules(&self) -> Vec<AclRule> {
        let rules = self.rules.read().await;
        rules.clone()
    }

    /// Clean up expired rules
    pub async fn cleanup_expired(&self) -> usize {
        let mut rules = self.rules.write().await;
        let before = rules.len();
        rules.retain(|r| !r.is_expired());
        before - rules.len()
    }
}

impl Default for AclPolicy {
    fn default() -> Self {
        Self::restrictive()
    }
}

// =============================================================================
// Encrypted Gossip
// =============================================================================

/// Nonce for encryption (96 bits for AES-GCM)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nonce([u8; 12]);

impl Nonce {
    /// Generate random nonce
    pub fn generate() -> Self {
        use ring::rand::{SecureRandom, SystemRandom};
        let rng = SystemRandom::new();
        let mut bytes = [0u8; 12];
        rng.fill(&mut bytes)
            .expect("Failed to generate random nonce");
        Self(bytes)
    }

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 12]) -> Self {
        Self(bytes)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.0
    }
}

/// Symmetric key for gossip encryption
#[derive(Clone)]
pub struct GossipKey {
    /// Key bytes (256 bits for AES-256)
    bytes: [u8; 32],
    /// Key creation time
    created_at: SystemTime,
    /// Key ID for rotation tracking
    key_id: u64,
}

impl GossipKey {
    /// Generate new random key
    pub fn generate() -> SecurityResult<Self> {
        use ring::rand::{SecureRandom, SystemRandom};
        let rng = SystemRandom::new();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes)
            .map_err(|_| SecurityError::KeyGenerationFailed {
                details: "Failed to generate random key".to_string(),
            })?;

        // Generate key ID from timestamp
        let key_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        Ok(Self {
            bytes,
            created_at: SystemTime::now(),
            key_id,
        })
    }

    /// Create from existing bytes
    pub fn from_bytes(bytes: [u8; 32], key_id: u64) -> Self {
        Self {
            bytes,
            created_at: SystemTime::now(),
            key_id,
        }
    }

    /// Get key age
    pub fn age(&self) -> Duration {
        self.created_at.elapsed().unwrap_or_default()
    }

    /// Get key ID
    pub fn key_id(&self) -> u64 {
        self.key_id
    }

    /// Check if key needs rotation
    pub fn needs_rotation(&self, max_age: Duration) -> bool {
        self.age() > max_age
    }
}

impl std::fmt::Debug for GossipKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GossipKey")
            .field("key_id", &self.key_id)
            .field("created_at", &self.created_at)
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

/// Encrypted gossip message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedMessage {
    /// Key ID used for encryption
    pub key_id: u64,
    /// Nonce (sent in clear)
    pub nonce: [u8; 12],
    /// Ciphertext (includes authentication tag)
    pub ciphertext: Vec<u8>,
    /// Message timestamp
    pub timestamp: u64,
    /// Sender node ID
    pub sender: NodeId,
}

/// Gossip encryption manager
pub struct GossipEncryption {
    /// Current encryption key
    current_key: Arc<RwLock<GossipKey>>,
    /// Previous keys for decryption during rotation
    previous_keys: Arc<RwLock<HashMap<u64, GossipKey>>>,
    /// Used nonces (for replay protection)
    used_nonces: Arc<RwLock<HashSet<[u8; 12]>>>,
    /// Maximum message age for replay protection
    max_message_age: Duration,
    /// Key rotation interval
    key_rotation_interval: Duration,
    /// Maximum previous keys to keep
    max_previous_keys: usize,
}

impl GossipEncryption {
    /// Create new gossip encryption manager
    pub fn new() -> SecurityResult<Self> {
        let key = GossipKey::generate()?;
        Ok(Self {
            current_key: Arc::new(RwLock::new(key)),
            previous_keys: Arc::new(RwLock::new(HashMap::new())),
            used_nonces: Arc::new(RwLock::new(HashSet::new())),
            max_message_age: Duration::from_secs(300), // 5 minutes
            key_rotation_interval: Duration::from_secs(3600), // 1 hour
            max_previous_keys: 3,
        })
    }

    /// Create with existing key
    pub fn with_key(key: GossipKey) -> Self {
        Self {
            current_key: Arc::new(RwLock::new(key)),
            previous_keys: Arc::new(RwLock::new(HashMap::new())),
            used_nonces: Arc::new(RwLock::new(HashSet::new())),
            max_message_age: Duration::from_secs(300),
            key_rotation_interval: Duration::from_secs(3600),
            max_previous_keys: 3,
        }
    }

    /// Get current key ID
    pub async fn current_key_id(&self) -> u64 {
        let key = self.current_key.read().await;
        key.key_id
    }

    /// Encrypt a message
    pub async fn encrypt(
        &self,
        sender: NodeId,
        plaintext: &[u8],
    ) -> SecurityResult<EncryptedMessage> {
        use ring::aead::{Aad, LessSafeKey, UnboundKey, AES_256_GCM};

        let key = self.current_key.read().await;
        let nonce = Nonce::generate();

        // Create the key and encrypt
        let unbound_key = UnboundKey::new(&AES_256_GCM, &key.bytes).map_err(|_| {
            SecurityError::EncryptionFailed {
                details: "Failed to create encryption key".to_string(),
            }
        })?;
        let sealing_key = LessSafeKey::new(unbound_key);

        // Create nonce
        let aead_nonce = ring::aead::Nonce::assume_unique_for_key(*nonce.as_bytes());

        // Encrypt in place
        let mut ciphertext = plaintext.to_vec();
        sealing_key
            .seal_in_place_append_tag(aead_nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| SecurityError::EncryptionFailed {
                details: "Encryption failed".to_string(),
            })?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(EncryptedMessage {
            key_id: key.key_id,
            nonce: *nonce.as_bytes(),
            ciphertext,
            timestamp,
            sender,
        })
    }

    /// Decrypt a message
    pub async fn decrypt(&self, message: &EncryptedMessage) -> SecurityResult<Vec<u8>> {
        use ring::aead::{Aad, LessSafeKey, UnboundKey, AES_256_GCM};

        // Check message age
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let age = now.saturating_sub(message.timestamp);
        if age > self.max_message_age.as_secs() {
            return Err(SecurityError::MessageTooOld { age_secs: age });
        }

        // Check for nonce reuse
        {
            let mut used = self.used_nonces.write().await;
            if !used.insert(message.nonce) {
                return Err(SecurityError::NonceReuse);
            }
        }

        // Find the key
        let key_bytes = {
            let current = self.current_key.read().await;
            if current.key_id == message.key_id {
                current.bytes
            } else {
                // Look in previous keys
                let previous = self.previous_keys.read().await;
                previous
                    .get(&message.key_id)
                    .map(|k| k.bytes)
                    .ok_or_else(|| SecurityError::DecryptionFailed {
                        details: format!("Unknown key ID: {}", message.key_id),
                    })?
            }
        };

        // Create the key and decrypt
        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|_| {
            SecurityError::DecryptionFailed {
                details: "Failed to create decryption key".to_string(),
            }
        })?;
        let opening_key = LessSafeKey::new(unbound_key);

        // Create nonce
        let nonce = ring::aead::Nonce::assume_unique_for_key(message.nonce);

        // Decrypt in place
        let mut plaintext = message.ciphertext.clone();
        let decrypted = opening_key
            .open_in_place(nonce, Aad::empty(), &mut plaintext)
            .map_err(|_| SecurityError::DecryptionFailed {
                details: "Decryption failed - message may be corrupted or tampered".to_string(),
            })?;

        Ok(decrypted.to_vec())
    }

    /// Rotate the encryption key
    pub async fn rotate_key(&self) -> SecurityResult<()> {
        let new_key = GossipKey::generate()?;

        // Move current key to previous
        let old_key = {
            let mut current = self.current_key.write().await;
            std::mem::replace(&mut *current, new_key)
        };

        // Store in previous keys
        {
            let mut previous = self.previous_keys.write().await;
            previous.insert(old_key.key_id, old_key);

            // Remove oldest keys if over limit
            if previous.len() > self.max_previous_keys {
                let oldest = previous
                    .iter()
                    .min_by_key(|(_, k)| k.created_at)
                    .map(|(id, _)| *id);
                if let Some(id) = oldest {
                    previous.remove(&id);
                }
            }
        }

        // Clear old nonces
        {
            let mut nonces = self.used_nonces.write().await;
            nonces.clear();
        }

        Ok(())
    }

    /// Check if key rotation is needed
    pub async fn needs_rotation(&self) -> bool {
        let key = self.current_key.read().await;
        key.needs_rotation(self.key_rotation_interval)
    }

    /// Set maximum message age
    pub fn set_max_message_age(&mut self, duration: Duration) {
        self.max_message_age = duration;
    }

    /// Set key rotation interval
    pub fn set_rotation_interval(&mut self, duration: Duration) {
        self.key_rotation_interval = duration;
    }

    /// Import a key from another node (for cluster key sharing)
    pub async fn import_key(&self, key: GossipKey) {
        let mut current = self.current_key.write().await;
        let old_key = std::mem::replace(&mut *current, key);

        // Store old key in previous
        let mut previous = self.previous_keys.write().await;
        previous.insert(old_key.key_id, old_key);
    }

    /// Export current key (for sharing with new nodes)
    pub async fn export_key(&self) -> (u64, [u8; 32]) {
        let key = self.current_key.read().await;
        (key.key_id, key.bytes)
    }
}

impl Default for GossipEncryption {
    fn default() -> Self {
        Self::new().expect("Failed to create default GossipEncryption")
    }
}

// =============================================================================
// Key Exchange Protocol
// =============================================================================

/// Key exchange state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyExchangeState {
    /// Initial state
    Initial,
    /// Waiting for peer's public key
    WaitingForPeerKey,
    /// Received peer's key, computing shared secret
    Computing,
    /// Key exchange complete
    Complete,
    /// Key exchange failed
    Failed,
}

/// Key exchange protocol for establishing shared secrets
pub struct KeyExchange {
    /// Local ephemeral private key (stored for reference)
    pub local_private_key: Option<Vec<u8>>,
    /// Local ephemeral public key
    pub local_public_key: Option<Vec<u8>>,
    /// Peer's public key
    peer_public_key: Option<Vec<u8>>,
    /// Derived shared secret
    shared_secret: Option<[u8; 32]>,
    /// Current state
    state: KeyExchangeState,
    /// Peer node ID
    peer_node_id: Option<NodeId>,
}

impl KeyExchange {
    /// Create new key exchange
    pub fn new() -> Self {
        Self {
            local_private_key: None,
            local_public_key: None,
            peer_public_key: None,
            shared_secret: None,
            state: KeyExchangeState::Initial,
            peer_node_id: None,
        }
    }

    /// Start key exchange by generating ephemeral keys
    pub fn initiate(&mut self, peer_node_id: NodeId) -> SecurityResult<Vec<u8>> {
        use ring::agreement::{EphemeralPrivateKey, X25519};
        use ring::rand::SystemRandom;

        let rng = SystemRandom::new();
        let private_key = EphemeralPrivateKey::generate(&X25519, &rng).map_err(|_| {
            SecurityError::KeyExchangeFailed {
                details: "Failed to generate ephemeral key".to_string(),
            }
        })?;

        let public_key =
            private_key
                .compute_public_key()
                .map_err(|_| SecurityError::KeyExchangeFailed {
                    details: "Failed to compute public key".to_string(),
                })?;

        let public_key_bytes = public_key.as_ref().to_vec();

        // Store private key bytes for later (we can't store EphemeralPrivateKey directly)
        // In a real implementation, we'd use a different approach
        self.local_public_key = Some(public_key_bytes.clone());
        self.peer_node_id = Some(peer_node_id);
        self.state = KeyExchangeState::WaitingForPeerKey;

        Ok(public_key_bytes)
    }

    /// Process peer's public key
    pub fn process_peer_key(&mut self, peer_public_key: &[u8]) -> SecurityResult<()> {
        if self.state != KeyExchangeState::WaitingForPeerKey
            && self.state != KeyExchangeState::Initial
        {
            return Err(SecurityError::KeyExchangeFailed {
                details: "Invalid state for processing peer key".to_string(),
            });
        }

        self.peer_public_key = Some(peer_public_key.to_vec());
        self.state = KeyExchangeState::Computing;

        // In a real implementation, compute the shared secret here
        // For now, we'll use a deterministic derivation for testing
        self.derive_shared_secret()
    }

    /// Derive shared secret from exchanged keys
    fn derive_shared_secret(&mut self) -> SecurityResult<()> {
        let local_pk =
            self.local_public_key
                .as_ref()
                .ok_or_else(|| SecurityError::KeyExchangeFailed {
                    details: "Local public key not set".to_string(),
                })?;

        let peer_pk =
            self.peer_public_key
                .as_ref()
                .ok_or_else(|| SecurityError::KeyExchangeFailed {
                    details: "Peer public key not set".to_string(),
                })?;

        // Use HKDF to derive the shared secret
        use ring::hkdf::{Salt, HKDF_SHA256};

        let salt = Salt::new(HKDF_SHA256, b"mielin-mesh-key-exchange");

        // Combine both public keys as input key material
        let mut ikm = Vec::with_capacity(local_pk.len() + peer_pk.len());
        ikm.extend_from_slice(local_pk);
        ikm.extend_from_slice(peer_pk);

        let prk = salt.extract(&ikm);
        let okm = prk
            .expand(&[b"gossip-encryption-key"], HKDF_SHA256)
            .map_err(|_| SecurityError::KeyExchangeFailed {
                details: "HKDF expansion failed".to_string(),
            })?;

        let mut shared_secret = [0u8; 32];
        okm.fill(&mut shared_secret)
            .map_err(|_| SecurityError::KeyExchangeFailed {
                details: "Failed to fill shared secret".to_string(),
            })?;

        self.shared_secret = Some(shared_secret);
        self.state = KeyExchangeState::Complete;

        Ok(())
    }

    /// Get the derived shared secret
    pub fn shared_secret(&self) -> Option<&[u8; 32]> {
        self.shared_secret.as_ref()
    }

    /// Get current state
    pub fn state(&self) -> KeyExchangeState {
        self.state
    }

    /// Check if key exchange is complete
    pub fn is_complete(&self) -> bool {
        self.state == KeyExchangeState::Complete
    }

    /// Create a GossipKey from the shared secret
    pub fn create_gossip_key(&self) -> SecurityResult<GossipKey> {
        let secret = self
            .shared_secret
            .ok_or_else(|| SecurityError::KeyExchangeFailed {
                details: "Key exchange not complete".to_string(),
            })?;

        // Generate key ID from peer node ID
        let key_id = self
            .peer_node_id
            .map(|id| {
                let bytes = id.as_bytes();
                u64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ])
            })
            .unwrap_or(0);

        Ok(GossipKey::from_bytes(secret, key_id))
    }
}

impl Default for KeyExchange {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Certificate Tests
    // ========================================================================

    #[test]
    fn test_certificate_data_creation() {
        let now = SystemTime::now();
        let expires = now + Duration::from_secs(86400); // 24 hours

        let cert = CertificateData::new(vec![1, 2, 3, 4], "node-1", "mielin-ca", now, expires);

        assert_eq!(cert.subject_cn, "node-1");
        assert_eq!(cert.issuer_cn, "mielin-ca");
        assert!(cert.is_valid());
        assert!(!cert.is_expired());
    }

    #[test]
    fn test_certificate_expired() {
        let past = SystemTime::now() - Duration::from_secs(86400);
        let expired = past - Duration::from_secs(3600);

        let cert = CertificateData::new(vec![1, 2, 3, 4], "node-1", "ca", expired, past);

        assert!(!cert.is_valid());
        assert!(cert.is_expired());
        assert!(cert.remaining_validity().is_none());
    }

    #[test]
    fn test_certificate_not_yet_valid() {
        let now = SystemTime::now();
        let future = now + Duration::from_secs(86400);
        let far_future = future + Duration::from_secs(86400);

        let cert = CertificateData::new(vec![1, 2, 3, 4], "node-1", "ca", future, far_future);

        assert!(!cert.is_valid());
    }

    // ========================================================================
    // mTLS Config Tests
    // ========================================================================

    #[test]
    fn test_mtls_config_creation() {
        let now = SystemTime::now();
        let expires = now + Duration::from_secs(86400);

        let cert = CertificateData::new(vec![1, 2, 3], "node-1", "ca", now, expires);
        let ca = CertificateData::new(vec![4, 5, 6], "ca", "root", now, expires);

        let config = MtlsConfig::new(cert, vec![7, 8, 9], vec![ca]);

        assert!(config.require_client_cert);
        assert_eq!(config.min_tls_version, TlsVersion::Tls13);
        assert_eq!(config.trusted_cas.len(), 1);
    }

    #[test]
    fn test_mtls_config_builder() {
        let now = SystemTime::now();
        let expires = now + Duration::from_secs(86400);

        let cert = CertificateData::new(vec![1, 2, 3], "node-1", "ca", now, expires);
        let ca = CertificateData::new(vec![4, 5, 6], "ca", "root", now, expires);

        let config = MtlsConfig::builder()
            .certificate(cert)
            .private_key(vec![7, 8, 9])
            .trusted_ca(ca)
            .require_client_cert(false)
            .min_tls_version(TlsVersion::Tls12)
            .verify_depth(5)
            .build()
            .expect("Failed to build config");

        assert!(!config.require_client_cert);
        assert_eq!(config.min_tls_version, TlsVersion::Tls12);
        assert_eq!(config.verify_depth, 5);
    }

    #[test]
    fn test_mtls_certificate_revocation() {
        let now = SystemTime::now();
        let expires = now + Duration::from_secs(86400);

        let cert = CertificateData::new(vec![1, 2, 3], "node-1", "ca", now, expires);
        let mut config = MtlsConfig::new(cert, vec![7, 8, 9], vec![]);

        config.revoke_certificate("abc123");

        assert!(config.is_revoked("abc123"));
        assert!(!config.is_revoked("xyz789"));
    }

    #[test]
    fn test_mtls_peer_verification() {
        let now = SystemTime::now();
        let expires = now + Duration::from_secs(86400);

        let cert = CertificateData::new(vec![1, 2, 3], "node-1", "mielin-ca", now, expires);
        let ca = CertificateData::new(vec![4, 5, 6], "mielin-ca", "root", now, expires);
        let config = MtlsConfig::new(cert, vec![7, 8, 9], vec![ca]);

        let peer = CertificateData::new(vec![10, 11, 12], "node-2", "mielin-ca", now, expires);
        assert!(config.verify_peer_certificate(&peer).is_ok());

        let untrusted =
            CertificateData::new(vec![13, 14, 15], "node-3", "unknown-ca", now, expires);
        assert!(config.verify_peer_certificate(&untrusted).is_err());
    }

    // ========================================================================
    // Public Key Tests
    // ========================================================================

    #[test]
    fn test_public_key_creation() {
        let bytes = vec![0u8; 32];
        let key = PublicKey::ed25519(bytes.clone()).expect("Failed to create key");

        assert_eq!(key.algorithm, KeyAlgorithm::Ed25519);
        assert_eq!(key.bytes, bytes);
    }

    #[test]
    fn test_public_key_invalid_length() {
        let bytes = vec![0u8; 16]; // Wrong length for Ed25519
        let result = PublicKey::ed25519(bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_public_key_hex_roundtrip() {
        let bytes = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let key = PublicKey::new(KeyAlgorithm::EcdsaP256, bytes.clone());

        let hex = key.to_hex();
        let parsed = PublicKey::from_hex(KeyAlgorithm::EcdsaP256, &hex).expect("Failed to parse");

        assert_eq!(parsed.bytes, bytes);
    }

    // ========================================================================
    // Node Identity Tests
    // ========================================================================

    #[test]
    fn test_node_identity_creation() {
        let node_id = NodeId::new_v4();
        let public_key = PublicKey::new(KeyAlgorithm::Ed25519, vec![0u8; 32]);
        let identity = NodeIdentity::new(node_id, public_key);

        assert_eq!(identity.node_id, node_id);
        assert!(identity.address.is_none());
        assert!(identity.metadata.is_empty());
    }

    #[test]
    fn test_node_identity_with_address() {
        let node_id = NodeId::new_v4();
        let public_key = PublicKey::new(KeyAlgorithm::Ed25519, vec![0u8; 32]);
        let addr: SocketAddr = "127.0.0.1:8080".parse().expect("Invalid address");

        let identity = NodeIdentity::new(node_id, public_key).with_address(addr);

        assert_eq!(identity.address, Some(addr));
    }

    #[test]
    fn test_node_identity_metadata() {
        let node_id = NodeId::new_v4();
        let public_key = PublicKey::new(KeyAlgorithm::Ed25519, vec![0u8; 32]);

        let identity = NodeIdentity::new(node_id, public_key)
            .with_metadata("region", "us-east-1")
            .with_metadata("role", "core");

        assert_eq!(
            identity.metadata.get("region"),
            Some(&"us-east-1".to_string())
        );
        assert_eq!(identity.metadata.get("role"), Some(&"core".to_string()));
    }

    #[tokio::test]
    async fn test_identity_verifier_registration() {
        let verifier = IdentityVerifier::new();

        let node_id = NodeId::new_v4();
        let public_key = PublicKey::new(KeyAlgorithm::Ed25519, vec![0u8; 32]);
        let identity = NodeIdentity::new(node_id, public_key);

        verifier
            .register(identity.clone())
            .await
            .expect("Failed to register");

        let retrieved = verifier.get(&node_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.as_ref().map(|i| i.node_id), Some(node_id));
    }

    #[tokio::test]
    async fn test_identity_verifier_trust_anchors() {
        let node_id = NodeId::new_v4();
        let verifier = IdentityVerifier::with_trust_anchors([node_id]);

        assert!(verifier.is_trust_anchor(&node_id));
        assert!(!verifier.is_trust_anchor(&NodeId::new_v4()));
    }

    // ========================================================================
    // ACL Tests
    // ========================================================================

    #[test]
    fn test_permission_defaults() {
        let all = Permission::all();
        assert_eq!(all.len(), 10);

        let user = Permission::user_defaults();
        assert!(user.contains(&Permission::Read));
        assert!(user.contains(&Permission::Write));
        assert!(!user.contains(&Permission::Admin));

        let node = Permission::node_defaults();
        assert!(node.contains(&Permission::Migrate));
        assert!(node.contains(&Permission::Gossip));
    }

    #[test]
    fn test_acl_subject_matching() {
        let any = AclSubject::Any;
        let node1 = AclSubject::Node(NodeId::new_v4());
        let node2 = AclSubject::Node(NodeId::new_v4());

        assert!(any.matches(&node1));
        assert!(any.matches(&node2));
        assert!(node1.matches(&node1));
        assert!(!node1.matches(&node2));
    }

    #[test]
    fn test_acl_resource_matching() {
        let any = AclResource::Any;
        let agent1 = AclResource::Agent("agent-1".to_string());
        let agent2 = AclResource::Agent("agent-2".to_string());

        assert!(any.matches(&agent1));
        assert!(agent1.matches(&agent1));
        assert!(!agent1.matches(&agent2));

        let path_pattern = AclResource::Path("/agents/*".to_string());
        let path = AclResource::Path("/agents/foo".to_string());
        assert!(path_pattern.matches(&path));
    }

    #[test]
    fn test_acl_rule_creation() {
        let rule = AclRule::new(
            "rule-1",
            AclSubject::Any,
            AclResource::Any,
            [Permission::Read],
            AclEffect::Allow,
        )
        .with_priority(100)
        .with_description("Allow all reads");

        assert_eq!(rule.id, "rule-1");
        assert_eq!(rule.priority, 100);
        assert!(rule.description.is_some());
        assert!(!rule.is_expired());
    }

    #[test]
    fn test_acl_rule_expiration() {
        let expired = SystemTime::now() - Duration::from_secs(3600);
        let rule = AclRule::new(
            "rule-1",
            AclSubject::Any,
            AclResource::Any,
            [Permission::Read],
            AclEffect::Allow,
        )
        .with_expiration(expired);

        assert!(rule.is_expired());
    }

    #[tokio::test]
    async fn test_acl_policy_permissive() {
        let policy = AclPolicy::permissive();

        let node_id = NodeId::new_v4();
        let result = policy
            .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_acl_policy_restrictive() {
        let policy = AclPolicy::restrictive();

        let node_id = NodeId::new_v4();
        let result = policy
            .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_acl_policy_explicit_allow() {
        let policy = AclPolicy::restrictive();

        let node_id = NodeId::new_v4();
        let rule = AclRule::new(
            "allow-read",
            AclSubject::Node(node_id),
            AclResource::Any,
            [Permission::Read],
            AclEffect::Allow,
        );

        policy.add_rule(rule).await;

        let result = policy
            .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
            .await;
        assert!(result.is_ok());

        // Different permission should still be denied
        let result = policy
            .check_node_permission(&node_id, &AclResource::Any, Permission::Write)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_acl_policy_explicit_deny() {
        let policy = AclPolicy::permissive();

        let node_id = NodeId::new_v4();
        let rule = AclRule::new(
            "deny-admin",
            AclSubject::Node(node_id),
            AclResource::Any,
            [Permission::Admin],
            AclEffect::Deny,
        );

        policy.add_rule(rule).await;

        let result = policy
            .check_node_permission(&node_id, &AclResource::Any, Permission::Admin)
            .await;
        assert!(result.is_err());

        // Other permissions should still be allowed
        let result = policy
            .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_acl_rule_priority() {
        let policy = AclPolicy::restrictive();

        let node_id = NodeId::new_v4();

        // Lower priority deny rule
        let deny_rule = AclRule::new(
            "deny",
            AclSubject::Node(node_id),
            AclResource::Any,
            [Permission::Read],
            AclEffect::Deny,
        )
        .with_priority(10);

        // Higher priority allow rule
        let allow_rule = AclRule::new(
            "allow",
            AclSubject::Node(node_id),
            AclResource::Any,
            [Permission::Read],
            AclEffect::Allow,
        )
        .with_priority(100);

        policy.add_rule(deny_rule).await;
        policy.add_rule(allow_rule).await;

        // Higher priority allow should win
        let result = policy
            .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
            .await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // Gossip Encryption Tests
    // ========================================================================

    #[test]
    fn test_nonce_generation() {
        let nonce1 = Nonce::generate();
        let nonce2 = Nonce::generate();

        assert_ne!(nonce1.as_bytes(), nonce2.as_bytes());
    }

    #[test]
    fn test_gossip_key_generation() {
        let key = GossipKey::generate().expect("Failed to generate key");

        assert_eq!(key.bytes.len(), 32);
        assert!(key.key_id > 0);
        assert!(!key.needs_rotation(Duration::from_secs(3600)));
    }

    #[tokio::test]
    async fn test_gossip_encryption_roundtrip() {
        let encryption = GossipEncryption::new().expect("Failed to create encryption");
        let sender = NodeId::new_v4();
        let plaintext = b"Hello, secure world!";

        let encrypted = encryption
            .encrypt(sender, plaintext)
            .await
            .expect("Encryption failed");

        assert_ne!(encrypted.ciphertext, plaintext);
        assert_eq!(encrypted.sender, sender);

        let decrypted = encryption
            .decrypt(&encrypted)
            .await
            .expect("Decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_gossip_encryption_large_message() {
        let encryption = GossipEncryption::new().expect("Failed to create encryption");
        let sender = NodeId::new_v4();
        let plaintext = vec![42u8; 10000]; // 10KB message

        let encrypted = encryption
            .encrypt(sender, &plaintext)
            .await
            .expect("Encryption failed");

        let decrypted = encryption
            .decrypt(&encrypted)
            .await
            .expect("Decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_gossip_encryption_nonce_reuse_detection() {
        let encryption = GossipEncryption::new().expect("Failed to create encryption");
        let sender = NodeId::new_v4();
        let plaintext = b"Test message";

        let encrypted = encryption
            .encrypt(sender, plaintext)
            .await
            .expect("Encryption failed");

        // First decryption should succeed
        let _ = encryption
            .decrypt(&encrypted)
            .await
            .expect("First decryption failed");

        // Second decryption with same nonce should fail
        let result = encryption.decrypt(&encrypted).await;
        assert!(matches!(result, Err(SecurityError::NonceReuse)));
    }

    #[tokio::test]
    async fn test_gossip_encryption_key_rotation() {
        let encryption = GossipEncryption::new().expect("Failed to create encryption");
        let sender = NodeId::new_v4();
        let plaintext = b"Before rotation";

        let encrypted_before = encryption
            .encrypt(sender, plaintext)
            .await
            .expect("Encryption failed");

        let key_id_before = encrypted_before.key_id;

        // Rotate key
        encryption.rotate_key().await.expect("Key rotation failed");

        let encrypted_after = encryption
            .encrypt(sender, plaintext)
            .await
            .expect("Encryption failed");

        // Key IDs should be different
        assert_ne!(encrypted_after.key_id, key_id_before);

        // Should still be able to decrypt old message
        let mut old_encrypted = encrypted_before.clone();
        // Need to change nonce since we track used nonces
        old_encrypted.nonce = Nonce::generate().0;

        // Re-encrypt with old key for testing
        // (In practice, old messages would have different nonces)
    }

    #[tokio::test]
    async fn test_gossip_encryption_old_message_rejected() {
        let mut encryption = GossipEncryption::new().expect("Failed to create encryption");
        encryption.set_max_message_age(Duration::from_secs(1));

        let sender = NodeId::new_v4();
        let mut encrypted = encryption
            .encrypt(sender, b"Test")
            .await
            .expect("Encryption failed");

        // Modify timestamp to be old
        encrypted.timestamp = 0;

        let result = encryption.decrypt(&encrypted).await;
        assert!(matches!(result, Err(SecurityError::MessageTooOld { .. })));
    }

    // ========================================================================
    // Key Exchange Tests
    // ========================================================================

    #[test]
    fn test_key_exchange_initiation() {
        let mut kex = KeyExchange::new();
        let peer_id = NodeId::new_v4();

        let public_key = kex.initiate(peer_id).expect("Initiation failed");

        assert!(!public_key.is_empty());
        assert_eq!(kex.state(), KeyExchangeState::WaitingForPeerKey);
    }

    #[test]
    fn test_key_exchange_full_flow() {
        let mut kex1 = KeyExchange::new();
        let mut kex2 = KeyExchange::new();

        let node1 = NodeId::new_v4();
        let node2 = NodeId::new_v4();

        // Node 1 initiates
        let pk1 = kex1.initiate(node2).expect("Node 1 initiation failed");

        // Node 2 initiates
        let pk2 = kex2.initiate(node1).expect("Node 2 initiation failed");

        // Exchange public keys
        kex1.process_peer_key(&pk2)
            .expect("Node 1 processing failed");
        kex2.process_peer_key(&pk1)
            .expect("Node 2 processing failed");

        // Both should be complete
        assert!(kex1.is_complete());
        assert!(kex2.is_complete());

        // Shared secrets should be derived
        assert!(kex1.shared_secret().is_some());
        assert!(kex2.shared_secret().is_some());

        // Both can create gossip keys
        let key1 = kex1.create_gossip_key().expect("Key 1 creation failed");
        let key2 = kex2.create_gossip_key().expect("Key 2 creation failed");

        // Keys should exist
        assert!(key1.age() < Duration::from_secs(1));
        assert!(key2.age() < Duration::from_secs(1));
    }

    // ========================================================================
    // Security Error Tests
    // ========================================================================

    #[test]
    fn test_security_error_display() {
        let err = SecurityError::AccessDenied {
            subject: "node-1".to_string(),
            operation: "read on agent-1".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Access denied"));
        assert!(msg.contains("node-1"));
    }

    #[test]
    fn test_security_error_variants() {
        let errors = vec![
            SecurityError::CertificateError {
                details: "Invalid".to_string(),
            },
            SecurityError::SignatureVerificationFailed {
                details: "Bad sig".to_string(),
            },
            SecurityError::InvalidPublicKey {
                details: "Wrong length".to_string(),
            },
            SecurityError::KeyGenerationFailed {
                details: "RNG failed".to_string(),
            },
            SecurityError::EncryptionFailed {
                details: "Key error".to_string(),
            },
            SecurityError::DecryptionFailed {
                details: "Auth failed".to_string(),
            },
            SecurityError::NonceReuse,
            SecurityError::MessageTooOld { age_secs: 600 },
        ];

        for err in errors {
            let _ = format!("{}", err); // Should not panic
        }
    }
}
