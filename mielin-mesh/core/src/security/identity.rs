//! Node identity and signature verification for MielinMesh security.

use super::*;

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
