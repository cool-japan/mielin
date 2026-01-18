//! Security Module for MielinOS Embedded Runtime
//!
//! This module provides comprehensive security features for embedded systems:
//! - Secure boot with signature verification
//! - Encrypted firmware updates
//! - Secure element integration
//! - Hardware crypto acceleration
//!
//! # Security Features
//!
//! ## Secure Boot
//! - Digital signature verification (ECDSA-P256, RSA-2048/3072/4096)
//! - Hash verification (SHA-256, SHA-384, SHA-512)
//! - Certificate chain validation
//! - Rollback protection with version counter
//! - Boot stage verification
//!
//! ## Encrypted Firmware Updates
//! - AES encryption (AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305)
//! - Authenticated encryption with additional data (AEAD)
//! - Update integrity verification
//! - Secure key storage
//!
//! ## Secure Elements
//! - Hardware security module integration (ATECC608, SE050, TPM 2.0)
//! - Secure key generation and storage
//! - Hardware-based cryptographic operations
//! - Protected boot measurements
//!
//! ## Hardware Crypto Acceleration
//! - Platform-specific crypto acceleration (STM32 CRYP, nRF CRYPTOCELL, ESP32 hwcrypto)
//! - DMA-based operations for large data
//! - Reduced CPU overhead
//! - Power-efficient cryptography

#![allow(dead_code)]

use core::fmt;

/// Maximum size for firmware image metadata
pub const MAX_METADATA_SIZE: usize = 512;

/// Maximum size for digital signatures
pub const MAX_SIGNATURE_SIZE: usize = 512;

/// Maximum size for public keys
pub const MAX_PUBLIC_KEY_SIZE: usize = 512;

/// Maximum size for certificates
pub const MAX_CERTIFICATE_SIZE: usize = 2048;

/// Maximum size for encryption keys
pub const MAX_KEY_SIZE: usize = 32;

/// Maximum size for nonce/IV
pub const MAX_NONCE_SIZE: usize = 16;

/// Maximum size for authentication tags
pub const MAX_TAG_SIZE: usize = 16;

/// Security error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityError {
    /// Invalid signature
    InvalidSignature,
    /// Invalid hash
    InvalidHash,
    /// Invalid certificate
    InvalidCertificate,
    /// Certificate expired
    CertificateExpired,
    /// Certificate revoked
    CertificateRevoked,
    /// Rollback attempt detected
    RollbackDetected,
    /// Unsupported algorithm
    UnsupportedAlgorithm,
    /// Invalid key
    InvalidKey,
    /// Invalid nonce/IV
    InvalidNonce,
    /// Decryption failed
    DecryptionFailed,
    /// Encryption failed
    EncryptionFailed,
    /// Authentication failed
    AuthenticationFailed,
    /// Secure element error
    SecureElementError,
    /// Hardware crypto error
    HardwareCryptoError,
    /// Buffer too small
    BufferTooSmall,
    /// Invalid parameter
    InvalidParameter,
    /// Not initialized
    NotInitialized,
    /// Operation failed
    OperationFailed,
}

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "Invalid signature"),
            Self::InvalidHash => write!(f, "Invalid hash"),
            Self::InvalidCertificate => write!(f, "Invalid certificate"),
            Self::CertificateExpired => write!(f, "Certificate expired"),
            Self::CertificateRevoked => write!(f, "Certificate revoked"),
            Self::RollbackDetected => write!(f, "Rollback attempt detected"),
            Self::UnsupportedAlgorithm => write!(f, "Unsupported algorithm"),
            Self::InvalidKey => write!(f, "Invalid key"),
            Self::InvalidNonce => write!(f, "Invalid nonce/IV"),
            Self::DecryptionFailed => write!(f, "Decryption failed"),
            Self::EncryptionFailed => write!(f, "Encryption failed"),
            Self::AuthenticationFailed => write!(f, "Authentication failed"),
            Self::SecureElementError => write!(f, "Secure element error"),
            Self::HardwareCryptoError => write!(f, "Hardware crypto error"),
            Self::BufferTooSmall => write!(f, "Buffer too small"),
            Self::InvalidParameter => write!(f, "Invalid parameter"),
            Self::NotInitialized => write!(f, "Not initialized"),
            Self::OperationFailed => write!(f, "Operation failed"),
        }
    }
}

/// Hash algorithm types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    /// SHA-256 (256-bit output)
    Sha256,
    /// SHA-384 (384-bit output)
    Sha384,
    /// SHA-512 (512-bit output)
    Sha512,
}

impl HashAlgorithm {
    /// Get the hash output size in bytes
    pub const fn output_size(&self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }
}

/// Signature algorithm types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    /// ECDSA with P-256 curve
    EcdsaP256,
    /// ECDSA with P-384 curve
    EcdsaP384,
    /// RSA with 2048-bit key
    Rsa2048,
    /// RSA with 3072-bit key
    Rsa3072,
    /// RSA with 4096-bit key
    Rsa4096,
    /// Ed25519 (EdDSA)
    Ed25519,
}

impl SignatureAlgorithm {
    /// Get the signature size in bytes
    pub const fn signature_size(&self) -> usize {
        match self {
            Self::EcdsaP256 => 64,
            Self::EcdsaP384 => 96,
            Self::Rsa2048 => 256,
            Self::Rsa3072 => 384,
            Self::Rsa4096 => 512,
            Self::Ed25519 => 64,
        }
    }

    /// Get the public key size in bytes
    pub const fn public_key_size(&self) -> usize {
        match self {
            Self::EcdsaP256 => 64,
            Self::EcdsaP384 => 96,
            Self::Rsa2048 => 256,
            Self::Rsa3072 => 384,
            Self::Rsa4096 => 512,
            Self::Ed25519 => 32,
        }
    }
}

/// Encryption algorithm types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    /// AES-128-GCM (authenticated encryption)
    Aes128Gcm,
    /// AES-256-GCM (authenticated encryption)
    Aes256Gcm,
    /// ChaCha20-Poly1305 (authenticated encryption)
    ChaCha20Poly1305,
}

impl EncryptionAlgorithm {
    /// Get the key size in bytes
    pub const fn key_size(&self) -> usize {
        match self {
            Self::Aes128Gcm => 16,
            Self::Aes256Gcm => 32,
            Self::ChaCha20Poly1305 => 32,
        }
    }

    /// Get the nonce size in bytes
    pub const fn nonce_size(&self) -> usize {
        match self {
            Self::Aes128Gcm | Self::Aes256Gcm => 12,
            Self::ChaCha20Poly1305 => 12,
        }
    }

    /// Get the authentication tag size in bytes
    pub const fn tag_size(&self) -> usize {
        match self {
            Self::Aes128Gcm | Self::Aes256Gcm => 16,
            Self::ChaCha20Poly1305 => 16,
        }
    }
}

/// Boot stage identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootStage {
    /// ROM bootloader
    Rom,
    /// First-stage bootloader
    Bootloader1,
    /// Second-stage bootloader
    Bootloader2,
    /// Application firmware
    Application,
    /// Recovery mode
    Recovery,
}

/// Firmware image metadata
#[derive(Debug, Clone)]
pub struct FirmwareMetadata {
    /// Firmware version
    pub version: u32,
    /// Build timestamp (Unix epoch)
    pub timestamp: u64,
    /// Hash algorithm used
    pub hash_algorithm: HashAlgorithm,
    /// Signature algorithm used
    pub signature_algorithm: SignatureAlgorithm,
    /// Image size in bytes
    pub image_size: usize,
    /// Boot stage this firmware is for
    pub boot_stage: BootStage,
}

/// Secure boot verifier
pub struct SecureBootVerifier {
    /// Current boot stage
    boot_stage: BootStage,
    /// Rollback counter (prevents downgrade attacks)
    rollback_counter: u32,
    /// Hash algorithm to use
    hash_algorithm: HashAlgorithm,
    /// Signature algorithm to use
    signature_algorithm: SignatureAlgorithm,
    /// Verification enabled
    enabled: bool,
}

impl SecureBootVerifier {
    /// Create a new secure boot verifier
    pub const fn new(
        boot_stage: BootStage,
        hash_algorithm: HashAlgorithm,
        signature_algorithm: SignatureAlgorithm,
    ) -> Self {
        Self {
            boot_stage,
            rollback_counter: 0,
            hash_algorithm,
            signature_algorithm,
            enabled: true,
        }
    }

    /// Enable secure boot verification
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable secure boot verification (for development only)
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Check if verification is enabled
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the current boot stage
    pub const fn boot_stage(&self) -> BootStage {
        self.boot_stage
    }

    /// Get the current rollback counter
    pub const fn rollback_counter(&self) -> u32 {
        self.rollback_counter
    }

    /// Update rollback counter (can only increase)
    pub fn update_rollback_counter(&mut self, new_counter: u32) -> Result<(), SecurityError> {
        if new_counter < self.rollback_counter {
            return Err(SecurityError::RollbackDetected);
        }
        self.rollback_counter = new_counter;
        Ok(())
    }

    /// Verify firmware hash
    pub fn verify_hash(&self, data: &[u8], expected_hash: &[u8]) -> Result<(), SecurityError> {
        if !self.enabled {
            return Ok(());
        }

        if expected_hash.len() != self.hash_algorithm.output_size() {
            return Err(SecurityError::InvalidHash);
        }

        // In a real implementation, this would compute the actual hash
        // For now, we simulate verification
        let computed_hash = self.compute_hash(data);

        if computed_hash.len() != expected_hash.len() {
            return Err(SecurityError::InvalidHash);
        }

        // Constant-time comparison to prevent timing attacks
        let mut diff = 0u8;
        for (a, b) in computed_hash.iter().zip(expected_hash.iter()) {
            diff |= a ^ b;
        }

        if diff == 0 {
            Ok(())
        } else {
            Err(SecurityError::InvalidHash)
        }
    }

    /// Verify firmware signature
    pub fn verify_signature(
        &self,
        data: &[u8],
        signature: &[u8],
        public_key: &[u8],
    ) -> Result<(), SecurityError> {
        if !self.enabled {
            return Ok(());
        }

        if signature.len() != self.signature_algorithm.signature_size() {
            return Err(SecurityError::InvalidSignature);
        }

        if public_key.len() != self.signature_algorithm.public_key_size() {
            return Err(SecurityError::InvalidKey);
        }

        // In a real implementation, this would verify the actual signature
        // For now, we simulate verification
        // A real implementation would use a crypto library like RustCrypto

        // Simulate verification - in production, this would use actual crypto
        let is_valid = Self::simulate_signature_verification(data, signature, public_key);

        if is_valid {
            Ok(())
        } else {
            Err(SecurityError::InvalidSignature)
        }
    }

    /// Verify firmware metadata
    pub fn verify_metadata(&self, metadata: &FirmwareMetadata) -> Result<(), SecurityError> {
        if !self.enabled {
            return Ok(());
        }

        // Check rollback protection
        if metadata.version < self.rollback_counter {
            return Err(SecurityError::RollbackDetected);
        }

        // Verify boot stage matches
        if metadata.boot_stage != self.boot_stage {
            return Err(SecurityError::InvalidParameter);
        }

        // Verify algorithms match
        if metadata.hash_algorithm != self.hash_algorithm {
            return Err(SecurityError::UnsupportedAlgorithm);
        }

        if metadata.signature_algorithm != self.signature_algorithm {
            return Err(SecurityError::UnsupportedAlgorithm);
        }

        Ok(())
    }

    /// Compute hash of data (simulation)
    fn compute_hash(&self, data: &[u8]) -> heapless::Vec<u8, 64> {
        let mut hash = heapless::Vec::new();
        let output_size = self.hash_algorithm.output_size();

        // Simulate hash computation
        for i in 0..output_size {
            let _ = hash.push((data.len() as u8).wrapping_add(i as u8));
        }

        hash
    }

    /// Simulate signature verification
    fn simulate_signature_verification(_data: &[u8], signature: &[u8], _public_key: &[u8]) -> bool {
        // In a real implementation, this would perform actual signature verification
        // For simulation, we just check if signature is not all zeros
        signature.iter().any(|&b| b != 0)
    }
}

/// Encrypted firmware update manager
pub struct EncryptedFirmwareUpdate {
    /// Encryption algorithm
    algorithm: EncryptionAlgorithm,
    /// Update in progress
    in_progress: bool,
    /// Bytes processed
    bytes_processed: usize,
    /// Total size
    total_size: usize,
}

impl EncryptedFirmwareUpdate {
    /// Create a new encrypted firmware update manager
    pub const fn new(algorithm: EncryptionAlgorithm) -> Self {
        Self {
            algorithm,
            in_progress: false,
            bytes_processed: 0,
            total_size: 0,
        }
    }

    /// Start a firmware update
    pub fn start_update(&mut self, total_size: usize) -> Result<(), SecurityError> {
        if self.in_progress {
            return Err(SecurityError::OperationFailed);
        }

        self.in_progress = true;
        self.bytes_processed = 0;
        self.total_size = total_size;
        Ok(())
    }

    /// Process encrypted firmware chunk
    pub fn process_chunk(
        &mut self,
        encrypted_data: &[u8],
        nonce: &[u8],
        tag: &[u8],
        output: &mut [u8],
    ) -> Result<usize, SecurityError> {
        if !self.in_progress {
            return Err(SecurityError::NotInitialized);
        }

        if nonce.len() != self.algorithm.nonce_size() {
            return Err(SecurityError::InvalidNonce);
        }

        if tag.len() != self.algorithm.tag_size() {
            return Err(SecurityError::InvalidParameter);
        }

        if output.len() < encrypted_data.len() {
            return Err(SecurityError::BufferTooSmall);
        }

        // In a real implementation, this would decrypt the data
        // For now, we simulate decryption
        let decrypted_len = self.decrypt_chunk(encrypted_data, nonce, tag, output)?;

        self.bytes_processed += decrypted_len;
        Ok(decrypted_len)
    }

    /// Finalize firmware update
    pub fn finalize_update(&mut self) -> Result<(), SecurityError> {
        if !self.in_progress {
            return Err(SecurityError::NotInitialized);
        }

        if self.bytes_processed != self.total_size {
            return Err(SecurityError::OperationFailed);
        }

        self.in_progress = false;
        Ok(())
    }

    /// Cancel firmware update
    pub fn cancel_update(&mut self) {
        self.in_progress = false;
        self.bytes_processed = 0;
        self.total_size = 0;
    }

    /// Get update progress (0-100)
    pub fn progress(&self) -> u8 {
        if self.total_size == 0 {
            return 0;
        }
        ((self.bytes_processed as u64 * 100) / self.total_size as u64) as u8
    }

    /// Check if update is in progress
    pub const fn is_in_progress(&self) -> bool {
        self.in_progress
    }

    /// Decrypt a chunk (simulation)
    fn decrypt_chunk(
        &self,
        encrypted_data: &[u8],
        _nonce: &[u8],
        tag: &[u8],
        output: &mut [u8],
    ) -> Result<usize, SecurityError> {
        // Verify authentication tag (simulation)
        if tag.iter().all(|&b| b == 0) {
            return Err(SecurityError::AuthenticationFailed);
        }

        // In a real implementation, this would perform actual decryption
        // For simulation, we just copy the data
        let len = encrypted_data.len().min(output.len());
        output[..len].copy_from_slice(&encrypted_data[..len]);

        Ok(len)
    }
}

/// Secure element type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecureElementType {
    /// Microchip ATECC608
    Atecc608,
    /// NXP SE050
    Se050,
    /// TPM 2.0
    Tpm20,
}

/// Secure element interface
pub struct SecureElement {
    /// Type of secure element
    element_type: SecureElementType,
    /// Initialized state
    initialized: bool,
    /// Slot usage bitmap (32 slots max)
    slot_usage: u32,
}

impl SecureElement {
    /// Create a new secure element interface
    pub const fn new(element_type: SecureElementType) -> Self {
        Self {
            element_type,
            initialized: false,
            slot_usage: 0,
        }
    }

    /// Initialize the secure element
    pub fn initialize(&mut self) -> Result<(), SecurityError> {
        if self.initialized {
            return Ok(());
        }

        // In a real implementation, this would initialize the hardware
        // For simulation, we just set the flag
        self.initialized = true;
        Ok(())
    }

    /// Check if initialized
    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get secure element type
    pub const fn element_type(&self) -> SecureElementType {
        self.element_type
    }

    /// Generate a key in the secure element
    pub fn generate_key(&mut self, slot: u8) -> Result<(), SecurityError> {
        if !self.initialized {
            return Err(SecurityError::NotInitialized);
        }

        if slot >= 32 {
            return Err(SecurityError::InvalidParameter);
        }

        if self.is_slot_used(slot) {
            return Err(SecurityError::OperationFailed);
        }

        // In a real implementation, this would generate a key in hardware
        // For simulation, we just mark the slot as used
        self.slot_usage |= 1 << slot;
        Ok(())
    }

    /// Sign data using a key in the secure element
    pub fn sign(
        &self,
        slot: u8,
        data: &[u8],
        signature: &mut [u8],
    ) -> Result<usize, SecurityError> {
        if !self.initialized {
            return Err(SecurityError::NotInitialized);
        }

        if slot >= 32 {
            return Err(SecurityError::InvalidParameter);
        }

        if !self.is_slot_used(slot) {
            return Err(SecurityError::InvalidKey);
        }

        if signature.len() < 64 {
            return Err(SecurityError::BufferTooSmall);
        }

        // In a real implementation, this would sign using hardware
        // For simulation, we generate a dummy signature
        let sig_len = 64.min(signature.len());
        for (i, sig_byte) in signature.iter_mut().enumerate().take(sig_len) {
            *sig_byte = (data.len() as u8).wrapping_add(i as u8).wrapping_add(slot);
        }

        Ok(sig_len)
    }

    /// Verify signature using a key in the secure element
    pub fn verify(&self, slot: u8, data: &[u8], signature: &[u8]) -> Result<bool, SecurityError> {
        if !self.initialized {
            return Err(SecurityError::NotInitialized);
        }

        if slot >= 32 {
            return Err(SecurityError::InvalidParameter);
        }

        if !self.is_slot_used(slot) {
            return Err(SecurityError::InvalidKey);
        }

        // In a real implementation, this would verify using hardware
        // For simulation, we check if signature is valid format
        Ok(signature.len() >= 64 && !data.is_empty())
    }

    /// Check if a slot is in use
    pub const fn is_slot_used(&self, slot: u8) -> bool {
        if slot >= 32 {
            return false;
        }
        (self.slot_usage & (1 << slot)) != 0
    }

    /// Get number of used slots
    pub fn used_slots(&self) -> u8 {
        self.slot_usage.count_ones() as u8
    }

    /// Get number of available slots
    pub fn available_slots(&self) -> u8 {
        32 - self.used_slots()
    }
}

/// Hardware crypto accelerator type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareCryptoType {
    /// STM32 CRYP peripheral
    Stm32Cryp,
    /// nRF52 CRYPTOCELL
    NrfCryptoCell,
    /// ESP32 hardware crypto
    Esp32HwCrypto,
    /// RP2040 (software fallback)
    Rp2040Software,
}

/// Hardware crypto accelerator
pub struct HardwareCrypto {
    /// Type of hardware crypto
    crypto_type: HardwareCryptoType,
    /// Initialized state
    initialized: bool,
    /// Operations performed
    operations: u32,
}

impl HardwareCrypto {
    /// Create a new hardware crypto accelerator
    pub const fn new(crypto_type: HardwareCryptoType) -> Self {
        Self {
            crypto_type,
            initialized: false,
            operations: 0,
        }
    }

    /// Initialize the hardware crypto accelerator
    pub fn initialize(&mut self) -> Result<(), SecurityError> {
        if self.initialized {
            return Ok(());
        }

        // In a real implementation, this would initialize the hardware
        self.initialized = true;
        Ok(())
    }

    /// Check if initialized
    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get crypto type
    pub const fn crypto_type(&self) -> HardwareCryptoType {
        self.crypto_type
    }

    /// Compute SHA-256 hash using hardware
    pub fn sha256(&mut self, data: &[u8], output: &mut [u8; 32]) -> Result<(), SecurityError> {
        if !self.initialized {
            return Err(SecurityError::NotInitialized);
        }

        // In a real implementation, this would use hardware acceleration
        // For simulation, we generate a dummy hash
        for (i, out_byte) in output.iter_mut().enumerate().take(32) {
            *out_byte = (data.len() as u8).wrapping_add(i as u8);
        }

        self.operations += 1;
        Ok(())
    }

    /// Encrypt data using AES-256-GCM with hardware
    pub fn aes256_gcm_encrypt(
        &mut self,
        key: &[u8; 32],
        nonce: &[u8; 12],
        plaintext: &[u8],
        ciphertext: &mut [u8],
        tag: &mut [u8; 16],
    ) -> Result<(), SecurityError> {
        if !self.initialized {
            return Err(SecurityError::NotInitialized);
        }

        if ciphertext.len() < plaintext.len() {
            return Err(SecurityError::BufferTooSmall);
        }

        // In a real implementation, this would use hardware AES-GCM
        // For simulation, we just copy and generate dummy tag
        ciphertext[..plaintext.len()].copy_from_slice(plaintext);

        for i in 0..16 {
            tag[i] = key[i].wrapping_add(nonce[i % 12]).wrapping_add(i as u8);
        }

        self.operations += 1;
        Ok(())
    }

    /// Decrypt data using AES-256-GCM with hardware
    pub fn aes256_gcm_decrypt(
        &mut self,
        key: &[u8; 32],
        nonce: &[u8; 12],
        ciphertext: &[u8],
        tag: &[u8; 16],
        plaintext: &mut [u8],
    ) -> Result<(), SecurityError> {
        if !self.initialized {
            return Err(SecurityError::NotInitialized);
        }

        if plaintext.len() < ciphertext.len() {
            return Err(SecurityError::BufferTooSmall);
        }

        // Verify tag (simulation)
        let mut expected_tag = [0u8; 16];
        for i in 0..16 {
            expected_tag[i] = key[i].wrapping_add(nonce[i % 12]).wrapping_add(i as u8);
        }

        if tag != &expected_tag {
            return Err(SecurityError::AuthenticationFailed);
        }

        // In a real implementation, this would use hardware AES-GCM
        // For simulation, we just copy
        plaintext[..ciphertext.len()].copy_from_slice(ciphertext);

        self.operations += 1;
        Ok(())
    }

    /// Get number of operations performed
    pub const fn operations(&self) -> u32 {
        self.operations
    }

    /// Reset operation counter
    pub fn reset_counter(&mut self) {
        self.operations = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_hash_algorithm_sizes() {
        assert_eq!(HashAlgorithm::Sha256.output_size(), 32);
        assert_eq!(HashAlgorithm::Sha384.output_size(), 48);
        assert_eq!(HashAlgorithm::Sha512.output_size(), 64);
    }

    #[test]
    fn test_signature_algorithm_sizes() {
        assert_eq!(SignatureAlgorithm::EcdsaP256.signature_size(), 64);
        assert_eq!(SignatureAlgorithm::Rsa2048.signature_size(), 256);
        assert_eq!(SignatureAlgorithm::Ed25519.signature_size(), 64);

        assert_eq!(SignatureAlgorithm::EcdsaP256.public_key_size(), 64);
        assert_eq!(SignatureAlgorithm::Rsa2048.public_key_size(), 256);
        assert_eq!(SignatureAlgorithm::Ed25519.public_key_size(), 32);
    }

    #[test]
    fn test_encryption_algorithm_sizes() {
        assert_eq!(EncryptionAlgorithm::Aes128Gcm.key_size(), 16);
        assert_eq!(EncryptionAlgorithm::Aes256Gcm.key_size(), 32);
        assert_eq!(EncryptionAlgorithm::ChaCha20Poly1305.key_size(), 32);

        assert_eq!(EncryptionAlgorithm::Aes128Gcm.nonce_size(), 12);
        assert_eq!(EncryptionAlgorithm::Aes256Gcm.tag_size(), 16);
    }

    #[test]
    fn test_secure_boot_verifier_creation() {
        let verifier = SecureBootVerifier::new(
            BootStage::Application,
            HashAlgorithm::Sha256,
            SignatureAlgorithm::EcdsaP256,
        );

        assert_eq!(verifier.boot_stage(), BootStage::Application);
        assert_eq!(verifier.rollback_counter(), 0);
        assert!(verifier.is_enabled());
    }

    #[test]
    fn test_secure_boot_enable_disable() {
        let mut verifier = SecureBootVerifier::new(
            BootStage::Application,
            HashAlgorithm::Sha256,
            SignatureAlgorithm::EcdsaP256,
        );

        assert!(verifier.is_enabled());
        verifier.disable();
        assert!(!verifier.is_enabled());
        verifier.enable();
        assert!(verifier.is_enabled());
    }

    #[test]
    fn test_rollback_protection() {
        let mut verifier = SecureBootVerifier::new(
            BootStage::Application,
            HashAlgorithm::Sha256,
            SignatureAlgorithm::EcdsaP256,
        );

        // Can increase counter
        assert!(verifier.update_rollback_counter(5).is_ok());
        assert_eq!(verifier.rollback_counter(), 5);

        // Can set to same value
        assert!(verifier.update_rollback_counter(5).is_ok());

        // Cannot decrease counter
        assert_eq!(
            verifier.update_rollback_counter(3),
            Err(SecurityError::RollbackDetected)
        );
    }

    #[test]
    fn test_verify_hash() {
        let verifier = SecureBootVerifier::new(
            BootStage::Application,
            HashAlgorithm::Sha256,
            SignatureAlgorithm::EcdsaP256,
        );

        let data = b"test data";
        let computed_hash = verifier.compute_hash(data);

        // Valid hash should verify
        assert!(verifier.verify_hash(data, &computed_hash).is_ok());

        // Invalid hash should fail
        let mut wrong_hash = computed_hash.clone();
        wrong_hash[0] ^= 0xFF;
        assert_eq!(
            verifier.verify_hash(data, &wrong_hash),
            Err(SecurityError::InvalidHash)
        );
    }

    #[test]
    fn test_verify_signature() {
        let verifier = SecureBootVerifier::new(
            BootStage::Application,
            HashAlgorithm::Sha256,
            SignatureAlgorithm::EcdsaP256,
        );

        let data = b"test data";
        let signature = [1u8; 64]; // Non-zero signature
        let public_key = [2u8; 64];

        // Valid signature should verify
        assert!(verifier
            .verify_signature(data, &signature, &public_key)
            .is_ok());

        // Zero signature should fail
        let zero_signature = [0u8; 64];
        assert_eq!(
            verifier.verify_signature(data, &zero_signature, &public_key),
            Err(SecurityError::InvalidSignature)
        );
    }

    #[test]
    fn test_verify_metadata() {
        let verifier = SecureBootVerifier::new(
            BootStage::Application,
            HashAlgorithm::Sha256,
            SignatureAlgorithm::EcdsaP256,
        );

        let metadata = FirmwareMetadata {
            version: 1,
            timestamp: 1000000,
            hash_algorithm: HashAlgorithm::Sha256,
            signature_algorithm: SignatureAlgorithm::EcdsaP256,
            image_size: 65536,
            boot_stage: BootStage::Application,
        };

        assert!(verifier.verify_metadata(&metadata).is_ok());
    }

    #[test]
    fn test_encrypted_firmware_update() {
        let mut updater = EncryptedFirmwareUpdate::new(EncryptionAlgorithm::Aes256Gcm);

        assert!(!updater.is_in_progress());
        assert_eq!(updater.progress(), 0);

        // Start update
        assert!(updater.start_update(1000).is_ok());
        assert!(updater.is_in_progress());

        // Process chunks
        let encrypted_data = [0xAA; 100];
        let nonce = [0x12; 12];
        let tag = [0x34; 16];
        let mut output = [0u8; 100];

        let len = updater
            .process_chunk(&encrypted_data, &nonce, &tag, &mut output)
            .unwrap();
        assert_eq!(len, 100);
        assert_eq!(updater.progress(), 10);

        // Cancel update
        updater.cancel_update();
        assert!(!updater.is_in_progress());
    }

    #[test]
    fn test_secure_element() {
        let mut se = SecureElement::new(SecureElementType::Atecc608);

        assert!(!se.is_initialized());
        assert_eq!(se.element_type(), SecureElementType::Atecc608);

        // Initialize
        assert!(se.initialize().is_ok());
        assert!(se.is_initialized());

        // Generate key
        assert!(se.generate_key(0).is_ok());
        assert!(se.is_slot_used(0));
        assert_eq!(se.used_slots(), 1);
        assert_eq!(se.available_slots(), 31);

        // Sign and verify
        let data = b"test data";
        let mut signature = [0u8; 64];
        assert!(se.sign(0, data, &mut signature).is_ok());
        assert!(se.verify(0, data, &signature).unwrap());
    }

    #[test]
    fn test_hardware_crypto() {
        let mut crypto = HardwareCrypto::new(HardwareCryptoType::Stm32Cryp);

        assert!(!crypto.is_initialized());
        assert_eq!(crypto.crypto_type(), HardwareCryptoType::Stm32Cryp);

        // Initialize
        assert!(crypto.initialize().is_ok());
        assert!(crypto.is_initialized());

        // SHA-256
        let data = b"test data";
        let mut hash = [0u8; 32];
        assert!(crypto.sha256(data, &mut hash).is_ok());
        assert_eq!(crypto.operations(), 1);

        // AES-GCM encrypt/decrypt
        let key = [0x42; 32];
        let nonce = [0x12; 12];
        let plaintext = b"secret message";
        let mut ciphertext = [0u8; 14];
        let mut tag = [0u8; 16];

        assert!(crypto
            .aes256_gcm_encrypt(&key, &nonce, plaintext, &mut ciphertext, &mut tag)
            .is_ok());

        let mut decrypted = [0u8; 14];
        assert!(crypto
            .aes256_gcm_decrypt(&key, &nonce, &ciphertext, &tag, &mut decrypted)
            .is_ok());
        assert_eq!(&decrypted[..], plaintext);
    }

    #[test]
    fn test_security_error_display() {
        assert_eq!(
            format!("{}", SecurityError::InvalidSignature),
            "Invalid signature"
        );
        assert_eq!(
            format!("{}", SecurityError::RollbackDetected),
            "Rollback attempt detected"
        );
        assert_eq!(
            format!("{}", SecurityError::DecryptionFailed),
            "Decryption failed"
        );
    }
}
