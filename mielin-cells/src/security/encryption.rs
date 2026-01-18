//! State snapshot encryption
//!
//! This module provides encryption/decryption for agent state snapshots using AES-256-GCM.

use crate::CellError;
use ring::aead::{
    Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, AES_256_GCM,
};
use ring::error::Unspecified;
use ring::rand::SecureRandom;
use serde::{Deserialize, Serialize};

/// Encryption key for state snapshots
#[derive(Clone)]
pub struct EncryptionKey {
    /// Key material (32 bytes for AES-256)
    key_bytes: Vec<u8>,
}

impl EncryptionKey {
    /// Generate a new random encryption key
    pub fn generate() -> Self {
        let rng = ring::rand::SystemRandom::new();
        let mut key_bytes = vec![0u8; 32]; // AES-256 requires 32 bytes
        rng.fill(&mut key_bytes)
            .expect("Failed to generate random key");

        Self { key_bytes }
    }

    /// Create a key from existing bytes
    pub fn from_bytes(key_bytes: Vec<u8>) -> Result<Self, CellError> {
        if key_bytes.len() != 32 {
            return Err(CellError::InvalidState(format!(
                "Key must be 32 bytes, got {}",
                key_bytes.len()
            )));
        }
        Ok(Self { key_bytes })
    }

    /// Get the key bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.key_bytes
    }

    /// Export key as hex string
    pub fn to_hex(&self) -> String {
        self.key_bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    /// Import key from hex string
    pub fn from_hex(hex: &str) -> Result<Self, CellError> {
        if hex.len() != 64 {
            return Err(CellError::InvalidState(
                "Hex string must be 64 characters (32 bytes)".to_string(),
            ));
        }

        let mut key_bytes = Vec::with_capacity(32);
        for i in (0..hex.len()).step_by(2) {
            let byte_str = &hex[i..i + 2];
            let byte = u8::from_str_radix(byte_str, 16)
                .map_err(|e| CellError::InvalidState(format!("Invalid hex: {}", e)))?;
            key_bytes.push(byte);
        }

        Ok(Self { key_bytes })
    }
}

impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionKey")
            .field("key_bytes", &"[REDACTED]")
            .finish()
    }
}

/// Encrypted snapshot containing encrypted state data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSnapshot {
    /// Agent ID
    pub agent_id: [u8; 16],
    /// Encrypted data
    pub ciphertext: Vec<u8>,
    /// Nonce used for encryption (must be unique per encryption)
    pub nonce: Vec<u8>,
    /// Authentication tag (included in ciphertext with AES-GCM)
    pub timestamp: u64,
    /// Metadata about encryption
    pub metadata: EncryptionMetadata,
}

/// Metadata about the encryption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionMetadata {
    /// Algorithm used
    pub algorithm: String,
    /// Original size before encryption
    pub original_size: usize,
    /// Encrypted at timestamp
    pub encrypted_at: u64,
}

/// State encryptor for encrypting/decrypting agent state
pub struct StateEncryptor {
    /// Encryption key
    key: EncryptionKey,
}

impl StateEncryptor {
    /// Create a new state encryptor with a generated key
    pub fn new() -> Self {
        Self {
            key: EncryptionKey::generate(),
        }
    }

    /// Create a state encryptor with a specific key
    pub fn with_key(key: EncryptionKey) -> Self {
        Self { key }
    }

    /// Get the encryption key
    pub fn key(&self) -> &EncryptionKey {
        &self.key
    }

    /// Encrypt a state snapshot
    pub fn encrypt(
        &self,
        agent_id: [u8; 16],
        plaintext: &[u8],
    ) -> Result<EncryptedSnapshot, CellError> {
        // Generate a unique nonce for this encryption
        let rng = ring::rand::SystemRandom::new();
        let mut nonce_bytes = [0u8; 12];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| CellError::InvalidState("Failed to generate nonce".to_string()))?;

        // Create the sealing key
        let unbound_key = UnboundKey::new(&AES_256_GCM, self.key.as_bytes())
            .map_err(|_| CellError::InvalidState("Failed to create encryption key".to_string()))?;

        struct FixedNonce([u8; 12]);
        impl NonceSequence for FixedNonce {
            fn advance(&mut self) -> Result<Nonce, Unspecified> {
                Nonce::try_assume_unique_for_key(&self.0)
            }
        }

        let mut sealing_key = SealingKey::new(unbound_key, FixedNonce(nonce_bytes));

        // Prepare data for encryption
        let mut in_out = plaintext.to_vec();

        // Encrypt the data
        let aad = Aad::from(&agent_id);
        sealing_key
            .seal_in_place_append_tag(aad, &mut in_out)
            .map_err(|_| CellError::InvalidState("Encryption failed".to_string()))?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("System time error".to_string()))?
            .as_secs();

        Ok(EncryptedSnapshot {
            agent_id,
            ciphertext: in_out,
            nonce: nonce_bytes.to_vec(),
            timestamp,
            metadata: EncryptionMetadata {
                algorithm: "AES-256-GCM".to_string(),
                original_size: plaintext.len(),
                encrypted_at: timestamp,
            },
        })
    }

    /// Decrypt an encrypted snapshot
    pub fn decrypt(&self, snapshot: &EncryptedSnapshot) -> Result<Vec<u8>, CellError> {
        // Verify nonce length
        if snapshot.nonce.len() != 12 {
            return Err(CellError::InvalidState("Invalid nonce length".to_string()));
        }

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&snapshot.nonce);

        // Create the opening key
        let unbound_key = UnboundKey::new(&AES_256_GCM, self.key.as_bytes())
            .map_err(|_| CellError::InvalidState("Failed to create decryption key".to_string()))?;

        struct FixedNonce([u8; 12]);
        impl NonceSequence for FixedNonce {
            fn advance(&mut self) -> Result<Nonce, Unspecified> {
                Nonce::try_assume_unique_for_key(&self.0)
            }
        }

        let mut opening_key = OpeningKey::new(unbound_key, FixedNonce(nonce_bytes));

        // Prepare data for decryption
        let mut in_out = snapshot.ciphertext.clone();

        // Decrypt the data
        let aad = Aad::from(&snapshot.agent_id);
        let plaintext = opening_key
            .open_in_place(aad, &mut in_out)
            .map_err(|_| CellError::InvalidState("Decryption failed".to_string()))?;

        Ok(plaintext.to_vec())
    }

    /// Rotate the encryption key
    pub fn rotate_key(&mut self) -> EncryptionKey {
        let old_key = self.key.clone();
        self.key = EncryptionKey::generate();
        old_key
    }
}

impl Default for StateEncryptor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_key_generation() {
        let key = EncryptionKey::generate();
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_encryption_key_from_bytes() {
        let key_bytes = vec![0u8; 32];
        let key = EncryptionKey::from_bytes(key_bytes).expect("Failed to create key");
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_encryption_key_from_bytes_invalid() {
        let key_bytes = vec![0u8; 16]; // Wrong size
        let result = EncryptionKey::from_bytes(key_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_encryption_key_hex() {
        let key = EncryptionKey::generate();
        let hex = key.to_hex();
        assert_eq!(hex.len(), 64);

        let key2 = EncryptionKey::from_hex(&hex).expect("Failed to parse hex");
        assert_eq!(key.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_state_encryptor_encrypt_decrypt() {
        let encryptor = StateEncryptor::new();
        let agent_id = [1u8; 16];
        let plaintext = b"Hello, MielinOS! This is a secret state.";

        let encrypted = encryptor
            .encrypt(agent_id, plaintext)
            .expect("Encryption failed");

        assert_eq!(encrypted.agent_id, agent_id);
        assert_ne!(encrypted.ciphertext, plaintext);

        let decrypted = encryptor.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_state_encryptor_decrypt_wrong_key() {
        let encryptor1 = StateEncryptor::new();
        let encryptor2 = StateEncryptor::new();

        let agent_id = [1u8; 16];
        let plaintext = b"Secret data";

        let encrypted = encryptor1
            .encrypt(agent_id, plaintext)
            .expect("Encryption failed");

        // Try to decrypt with wrong key
        let result = encryptor2.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_state_encryptor_with_key() {
        let key = EncryptionKey::generate();
        let encryptor1 = StateEncryptor::with_key(key.clone());
        let encryptor2 = StateEncryptor::with_key(key);

        let agent_id = [1u8; 16];
        let plaintext = b"Shared key test";

        let encrypted = encryptor1
            .encrypt(agent_id, plaintext)
            .expect("Encryption failed");

        let decrypted = encryptor2.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_state_encryptor_large_data() {
        let encryptor = StateEncryptor::new();
        let agent_id = [1u8; 16];
        let plaintext = vec![42u8; 1_000_000]; // 1 MB of data

        let encrypted = encryptor
            .encrypt(agent_id, &plaintext)
            .expect("Encryption failed");

        assert_eq!(encrypted.metadata.original_size, 1_000_000);

        let decrypted = encryptor.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_state_encryptor_rotate_key() {
        let mut encryptor = StateEncryptor::new();
        let old_key_bytes = encryptor.key().as_bytes().to_vec();

        let old_key = encryptor.rotate_key();

        assert_eq!(old_key.as_bytes(), old_key_bytes.as_slice());
        assert_ne!(encryptor.key().as_bytes(), old_key_bytes.as_slice());
    }

    #[test]
    fn test_encrypted_snapshot_metadata() {
        let encryptor = StateEncryptor::new();
        let agent_id = [1u8; 16];
        let plaintext = b"Test data";

        let encrypted = encryptor
            .encrypt(agent_id, plaintext)
            .expect("Encryption failed");

        assert_eq!(encrypted.metadata.algorithm, "AES-256-GCM");
        assert_eq!(encrypted.metadata.original_size, plaintext.len());
        assert!(encrypted.metadata.encrypted_at > 0);
    }
}
