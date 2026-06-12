//! Message encryption for MielinMesh gossip protocol.

use super::*;

// =============================================================================
// Encrypted Gossip
// =============================================================================

/// Nonce for encryption (96 bits for AES-GCM)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nonce(pub [u8; 12]);

impl Nonce {
    /// Generate random nonce
    pub fn generate() -> Self {
        let bytes: [u8; 12] =
            oxicrypto_rand::random_nonce().expect("Failed to generate random nonce");
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
    pub bytes: [u8; 32],
    /// Key creation time
    created_at: SystemTime,
    /// Key ID for rotation tracking
    key_id: u64,
}

impl GossipKey {
    /// Generate new random key
    pub fn generate() -> SecurityResult<Self> {
        let bytes: [u8; 32] =
            oxicrypto_rand::random_nonce().map_err(|_| SecurityError::KeyGenerationFailed {
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
        use oxicrypto_aead::Aes256Gcm;
        use oxicrypto_core::Aead;

        let key = self.current_key.read().await;
        let nonce = Nonce::generate();

        // Encrypt with AES-256-GCM (empty AAD); the 16-byte authentication tag
        // is appended to the ciphertext.
        let ciphertext = Aes256Gcm
            .seal_to_vec(&key.bytes, nonce.as_bytes(), &[], plaintext)
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
        use oxicrypto_aead::Aes256Gcm;
        use oxicrypto_core::Aead;

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

        // Decrypt and authenticate with AES-256-GCM (empty AAD). The trailing
        // 16-byte tag is verified and stripped, returning the recovered plaintext.
        let plaintext = Aes256Gcm
            .open_to_vec(&key_bytes, &message.nonce, &[], &message.ciphertext)
            .map_err(|_| SecurityError::DecryptionFailed {
                details: "Decryption failed - message may be corrupted or tampered".to_string(),
            })?;

        Ok(plaintext)
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
