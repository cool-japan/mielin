//! Key exchange protocol for MielinMesh security.

use super::*;

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
