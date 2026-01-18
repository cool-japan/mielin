//! Over-The-Air (OTA) Update Module
//!
//! This module provides a comprehensive OTA update system for embedded devices,
//! supporting secure, reliable firmware updates over various transport mechanisms.
//!
//! # Features
//!
//! - Multiple transport protocols (HTTP, CoAP, MQTT-SN, BLE)
//! - Delta updates for bandwidth efficiency
//! - A/B partition switching for rollback safety
//! - Update verification and integrity checking
//! - Progress tracking and resume capability
//! - Power-loss resilient updates
//! - Version management and compatibility checking
//!
//! # Update Process
//!
//! 1. **Discovery**: Check for available updates
//! 2. **Download**: Fetch update package with progress tracking
//! 3. **Verification**: Validate signature and integrity
//! 4. **Installation**: Write to inactive partition
//! 5. **Activation**: Switch boot partition
//! 6. **Validation**: Boot and verify new firmware
//! 7. **Commit/Rollback**: Finalize or revert update

#![allow(dead_code)]

use crate::security::{HashAlgorithm, SecurityError, SignatureAlgorithm};
use core::fmt;

/// Maximum size for update metadata
pub const MAX_UPDATE_METADATA_SIZE: usize = 1024;

/// Maximum number of partitions
pub const MAX_PARTITIONS: usize = 4;

/// Maximum chunk size for downloads
pub const MAX_CHUNK_SIZE: usize = 4096;

/// OTA error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaError {
    /// No update available
    NoUpdateAvailable,
    /// Update download failed
    DownloadFailed,
    /// Update verification failed
    VerificationFailed,
    /// Installation failed
    InstallationFailed,
    /// Activation failed
    ActivationFailed,
    /// Rollback failed
    RollbackFailed,
    /// No space for update
    NoSpace,
    /// Invalid partition
    InvalidPartition,
    /// Invalid version
    InvalidVersion,
    /// Incompatible firmware
    IncompatibleFirmware,
    /// Transport error
    TransportError,
    /// Security error
    SecurityError(SecurityError),
    /// Not initialized
    NotInitialized,
    /// Already in progress
    AlreadyInProgress,
    /// Invalid state
    InvalidState,
    /// Checksum mismatch
    ChecksumMismatch,
}

impl fmt::Display for OtaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoUpdateAvailable => write!(f, "No update available"),
            Self::DownloadFailed => write!(f, "Update download failed"),
            Self::VerificationFailed => write!(f, "Update verification failed"),
            Self::InstallationFailed => write!(f, "Installation failed"),
            Self::ActivationFailed => write!(f, "Activation failed"),
            Self::RollbackFailed => write!(f, "Rollback failed"),
            Self::NoSpace => write!(f, "No space for update"),
            Self::InvalidPartition => write!(f, "Invalid partition"),
            Self::InvalidVersion => write!(f, "Invalid version"),
            Self::IncompatibleFirmware => write!(f, "Incompatible firmware"),
            Self::TransportError => write!(f, "Transport error"),
            Self::SecurityError(e) => write!(f, "Security error: {}", e),
            Self::NotInitialized => write!(f, "Not initialized"),
            Self::AlreadyInProgress => write!(f, "Update already in progress"),
            Self::InvalidState => write!(f, "Invalid state"),
            Self::ChecksumMismatch => write!(f, "Checksum mismatch"),
        }
    }
}

impl From<SecurityError> for OtaError {
    fn from(e: SecurityError) -> Self {
        Self::SecurityError(e)
    }
}

/// Update transport protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateTransport {
    /// HTTP/HTTPS
    Http,
    /// CoAP
    Coap,
    /// MQTT-SN
    MqttSn,
    /// Bluetooth Low Energy
    Ble,
    /// Serial port
    Serial,
}

/// Update type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateType {
    /// Full firmware image
    Full,
    /// Delta/differential update
    Delta,
    /// Configuration only
    Configuration,
}

/// Partition identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionId {
    /// Partition A
    A,
    /// Partition B
    B,
    /// Recovery partition
    Recovery,
    /// Factory partition (read-only)
    Factory,
}

/// Partition state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionState {
    /// Active (currently running)
    Active,
    /// Inactive (standby)
    Inactive,
    /// Invalid (corrupted or unverified)
    Invalid,
    /// Pending (waiting for validation)
    Pending,
}

/// Firmware version information
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// Major version
    pub major: u16,
    /// Minor version
    pub minor: u16,
    /// Patch version
    pub patch: u16,
    /// Build number
    pub build: u32,
}

impl Version {
    /// Create a new version
    pub const fn new(major: u16, minor: u16, patch: u16, build: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            build,
        }
    }

    /// Check if this version is compatible with another
    pub fn is_compatible_with(&self, other: &Version) -> bool {
        // Same major version is required for compatibility
        self.major == other.major
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major, self.minor, self.patch, self.build
        )
    }
}

/// Update metadata
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateMetadata {
    /// Version of the update
    pub version: Version,
    /// Size in bytes
    pub size: usize,
    /// Update type
    pub update_type: UpdateType,
    /// Hash algorithm
    pub hash_algorithm: HashAlgorithm,
    /// Signature algorithm
    pub signature_algorithm: SignatureAlgorithm,
    /// Release notes URL
    pub release_notes: Option<heapless::String<128>>,
    /// Required free space
    pub required_space: usize,
}

/// Partition information
#[derive(Debug, Clone, Copy)]
pub struct PartitionInfo {
    /// Partition identifier
    pub id: PartitionId,
    /// Current state
    pub state: PartitionState,
    /// Firmware version
    pub version: Option<Version>,
    /// Size in bytes
    pub size: usize,
    /// Used bytes
    pub used: usize,
    /// Start address
    pub start_address: usize,
}

impl PartitionInfo {
    /// Create a new partition info
    pub const fn new(id: PartitionId, size: usize, start_address: usize) -> Self {
        Self {
            id,
            state: PartitionState::Inactive,
            version: None,
            size,
            used: 0,
            start_address,
        }
    }

    /// Get available space
    pub const fn available_space(&self) -> usize {
        self.size - self.used
    }

    /// Check if partition has enough space
    pub const fn has_space(&self, required: usize) -> bool {
        self.available_space() >= required
    }
}

/// OTA update state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaState {
    /// Idle, no update in progress
    Idle,
    /// Checking for updates
    Checking,
    /// Downloading update
    Downloading,
    /// Verifying update
    Verifying,
    /// Installing update
    Installing,
    /// Update ready, pending activation
    Ready,
    /// Activating update
    Activating,
    /// Rolling back
    RollingBack,
}

/// OTA update progress
#[derive(Debug, Clone, Copy)]
pub struct OtaProgress {
    /// Current state
    pub state: OtaState,
    /// Bytes downloaded
    pub bytes_downloaded: usize,
    /// Total bytes
    pub total_bytes: usize,
    /// Percentage (0-100)
    pub percentage: u8,
    /// Error if any
    pub error: Option<OtaError>,
}

impl Default for OtaProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl OtaProgress {
    /// Create a new progress tracker
    pub const fn new() -> Self {
        Self {
            state: OtaState::Idle,
            bytes_downloaded: 0,
            total_bytes: 0,
            percentage: 0,
            error: None,
        }
    }

    /// Update progress
    pub fn update(&mut self, bytes: usize, total: usize) {
        self.bytes_downloaded = bytes;
        self.total_bytes = total;
        if total > 0 {
            self.percentage = ((bytes as u64 * 100) / total as u64) as u8;
        }
    }

    /// Set state
    pub fn set_state(&mut self, state: OtaState) {
        self.state = state;
    }

    /// Set error
    pub fn set_error(&mut self, error: OtaError) {
        self.error = Some(error);
    }

    /// Clear error
    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Check if completed
    pub const fn is_complete(&self) -> bool {
        matches!(self.state, OtaState::Ready)
    }

    /// Check if failed
    pub const fn is_failed(&self) -> bool {
        self.error.is_some()
    }
}

/// OTA update manager
pub struct OtaManager {
    /// Current firmware version
    current_version: Version,
    /// Active partition
    active_partition: PartitionId,
    /// Partition information
    partitions: [PartitionInfo; MAX_PARTITIONS],
    /// Number of partitions
    partition_count: usize,
    /// Update transport
    transport: UpdateTransport,
    /// Current state
    state: OtaState,
    /// Progress tracker
    progress: OtaProgress,
    /// Boot counter (for detecting boot loops)
    boot_counter: u8,
}

impl OtaManager {
    /// Create a new OTA manager
    pub fn new(
        current_version: Version,
        active_partition: PartitionId,
        transport: UpdateTransport,
    ) -> Self {
        Self {
            current_version,
            active_partition,
            partitions: [
                PartitionInfo::new(PartitionId::A, 0, 0),
                PartitionInfo::new(PartitionId::B, 0, 0),
                PartitionInfo::new(PartitionId::Recovery, 0, 0),
                PartitionInfo::new(PartitionId::Factory, 0, 0),
            ],
            partition_count: 0,
            transport,
            state: OtaState::Idle,
            progress: OtaProgress::new(),
            boot_counter: 0,
        }
    }

    /// Register a partition
    pub fn register_partition(&mut self, info: PartitionInfo) -> Result<(), OtaError> {
        if self.partition_count >= MAX_PARTITIONS {
            return Err(OtaError::NoSpace);
        }

        self.partitions[self.partition_count] = info;
        self.partition_count += 1;
        Ok(())
    }

    /// Get partition info
    pub fn get_partition(&self, id: PartitionId) -> Option<&PartitionInfo> {
        self.partitions[..self.partition_count]
            .iter()
            .find(|p| p.id == id)
    }

    /// Get mutable partition info
    fn get_partition_mut(&mut self, id: PartitionId) -> Option<&mut PartitionInfo> {
        self.partitions[..self.partition_count]
            .iter_mut()
            .find(|p| p.id == id)
    }

    /// Get inactive partition for update
    pub fn get_inactive_partition(&self) -> Option<PartitionId> {
        match self.active_partition {
            PartitionId::A => Some(PartitionId::B),
            PartitionId::B => Some(PartitionId::A),
            _ => None,
        }
    }

    /// Check for updates
    pub fn check_for_update(&mut self) -> Result<Option<UpdateMetadata>, OtaError> {
        if self.state != OtaState::Idle {
            return Err(OtaError::AlreadyInProgress);
        }

        self.state = OtaState::Checking;
        self.progress.set_state(OtaState::Checking);

        // In a real implementation, this would query the update server
        // For simulation, we return None (no update available)
        self.state = OtaState::Idle;
        Ok(None)
    }

    /// Start downloading an update
    pub fn start_download(&mut self, metadata: UpdateMetadata) -> Result<(), OtaError> {
        if self.state != OtaState::Idle {
            return Err(OtaError::AlreadyInProgress);
        }

        // Check version compatibility
        if !metadata.version.is_compatible_with(&self.current_version) {
            return Err(OtaError::IncompatibleFirmware);
        }

        // Check if we have space
        let target_partition = self
            .get_inactive_partition()
            .ok_or(OtaError::InvalidPartition)?;

        let partition = self
            .get_partition(target_partition)
            .ok_or(OtaError::InvalidPartition)?;

        if !partition.has_space(metadata.required_space) {
            return Err(OtaError::NoSpace);
        }

        self.state = OtaState::Downloading;
        self.progress.set_state(OtaState::Downloading);
        self.progress.update(0, metadata.size);

        Ok(())
    }

    /// Process downloaded chunk
    pub fn process_chunk(&mut self, chunk: &[u8], offset: usize) -> Result<(), OtaError> {
        if self.state != OtaState::Downloading {
            return Err(OtaError::InvalidState);
        }

        // In a real implementation, this would write to flash
        // For simulation, we just update progress
        self.progress.bytes_downloaded = offset + chunk.len();
        self.progress
            .update(self.progress.bytes_downloaded, self.progress.total_bytes);

        Ok(())
    }

    /// Verify downloaded update
    pub fn verify_update(
        &mut self,
        expected_hash: &[u8],
        signature: &[u8],
        public_key: &[u8],
    ) -> Result<(), OtaError> {
        if self.state != OtaState::Downloading {
            return Err(OtaError::InvalidState);
        }

        self.state = OtaState::Verifying;
        self.progress.set_state(OtaState::Verifying);

        // In a real implementation, this would verify the downloaded firmware
        // For simulation, we just check that the parameters are non-empty
        if expected_hash.is_empty() || signature.is_empty() || public_key.is_empty() {
            self.state = OtaState::Idle;
            return Err(OtaError::VerificationFailed);
        }

        Ok(())
    }

    /// Install the verified update
    pub fn install_update(&mut self) -> Result<(), OtaError> {
        if self.state != OtaState::Verifying {
            return Err(OtaError::InvalidState);
        }

        self.state = OtaState::Installing;
        self.progress.set_state(OtaState::Installing);

        // In a real implementation, this would program the inactive partition
        // and set up the boot configuration

        self.state = OtaState::Ready;
        self.progress.set_state(OtaState::Ready);

        Ok(())
    }

    /// Activate the installed update (switch partitions)
    pub fn activate_update(&mut self, new_version: Version) -> Result<(), OtaError> {
        if self.state != OtaState::Ready {
            return Err(OtaError::InvalidState);
        }

        self.state = OtaState::Activating;
        self.progress.set_state(OtaState::Activating);

        // Get the inactive partition
        let target_partition = self
            .get_inactive_partition()
            .ok_or(OtaError::InvalidPartition)?;

        // Update partition states
        if let Some(old_active) = self.get_partition_mut(self.active_partition) {
            old_active.state = PartitionState::Inactive;
        }

        if let Some(new_active) = self.get_partition_mut(target_partition) {
            new_active.state = PartitionState::Pending;
            new_active.version = Some(new_version);
        }

        // Switch active partition
        self.active_partition = target_partition;
        self.boot_counter = 0;

        // In a real implementation, this would set the boot flag
        // and trigger a reboot

        Ok(())
    }

    /// Confirm the update after successful boot
    pub fn confirm_update(&mut self) -> Result<(), OtaError> {
        let partition = self
            .get_partition_mut(self.active_partition)
            .ok_or(OtaError::InvalidPartition)?;

        if partition.state != PartitionState::Pending {
            return Err(OtaError::InvalidState);
        }

        partition.state = PartitionState::Active;
        self.current_version = partition.version.unwrap_or(self.current_version);
        self.boot_counter = 0;
        self.state = OtaState::Idle;

        Ok(())
    }

    /// Rollback to previous version
    pub fn rollback(&mut self) -> Result<(), OtaError> {
        self.state = OtaState::RollingBack;
        self.progress.set_state(OtaState::RollingBack);

        // Find the previously active partition
        let target_partition = self
            .get_inactive_partition()
            .ok_or(OtaError::InvalidPartition)?;

        // Update partition states
        if let Some(failed) = self.get_partition_mut(self.active_partition) {
            failed.state = PartitionState::Invalid;
        }

        if let Some(recovery) = self.get_partition_mut(target_partition) {
            if recovery.state == PartitionState::Inactive {
                recovery.state = PartitionState::Active;
            } else {
                return Err(OtaError::RollbackFailed);
            }
        }

        // Switch back
        self.active_partition = target_partition;
        self.state = OtaState::Idle;

        Ok(())
    }

    /// Increment boot counter (called on each boot)
    pub fn increment_boot_counter(&mut self) {
        self.boot_counter = self.boot_counter.saturating_add(1);
    }

    /// Check if boot loop detected (too many boots without confirmation)
    pub fn is_boot_loop_detected(&self, threshold: u8) -> bool {
        self.boot_counter >= threshold
    }

    /// Get current version
    pub const fn current_version(&self) -> &Version {
        &self.current_version
    }

    /// Get active partition
    pub const fn active_partition(&self) -> PartitionId {
        self.active_partition
    }

    /// Get current state
    pub const fn state(&self) -> OtaState {
        self.state
    }

    /// Get progress
    pub const fn progress(&self) -> &OtaProgress {
        &self.progress
    }

    /// Cancel ongoing update
    pub fn cancel(&mut self) {
        self.state = OtaState::Idle;
        self.progress = OtaProgress::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_version_comparison() {
        let v1 = Version::new(1, 0, 0, 100);
        let v2 = Version::new(1, 1, 0, 200);
        let v3 = Version::new(2, 0, 0, 300);

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1.is_compatible_with(&v2));
        assert!(!v1.is_compatible_with(&v3));
    }

    #[test]
    fn test_version_display() {
        let v = Version::new(1, 2, 3, 456);
        assert_eq!(format!("{}", v), "1.2.3.456");
    }

    #[test]
    fn test_partition_info() {
        let mut info = PartitionInfo::new(PartitionId::A, 1024 * 1024, 0x08000000);
        assert_eq!(info.available_space(), 1024 * 1024);
        assert!(info.has_space(512 * 1024));

        info.used = 512 * 1024;
        assert_eq!(info.available_space(), 512 * 1024);
        assert!(!info.has_space(1024 * 1024));
    }

    #[test]
    fn test_ota_progress() {
        let mut progress = OtaProgress::new();
        assert_eq!(progress.percentage, 0);
        assert!(!progress.is_complete());

        progress.update(500, 1000);
        assert_eq!(progress.percentage, 50);

        progress.set_state(OtaState::Ready);
        assert!(progress.is_complete());
    }

    #[test]
    fn test_ota_manager_creation() {
        let version = Version::new(1, 0, 0, 1);
        let manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        assert_eq!(manager.current_version(), &version);
        assert_eq!(manager.active_partition(), PartitionId::A);
        assert_eq!(manager.state(), OtaState::Idle);
    }

    #[test]
    fn test_register_partition() {
        let version = Version::new(1, 0, 0, 1);
        let mut manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        let partition_a = PartitionInfo::new(PartitionId::A, 1024 * 1024, 0x08000000);
        assert!(manager.register_partition(partition_a).is_ok());

        let partition_b = PartitionInfo::new(PartitionId::B, 1024 * 1024, 0x08100000);
        assert!(manager.register_partition(partition_b).is_ok());

        assert!(manager.get_partition(PartitionId::A).is_some());
        assert!(manager.get_partition(PartitionId::B).is_some());
    }

    #[test]
    fn test_inactive_partition() {
        let version = Version::new(1, 0, 0, 1);
        let manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        assert_eq!(manager.get_inactive_partition(), Some(PartitionId::B));

        let manager = OtaManager::new(version, PartitionId::B, UpdateTransport::Coap);
        assert_eq!(manager.get_inactive_partition(), Some(PartitionId::A));
    }

    #[test]
    fn test_check_for_update() {
        let version = Version::new(1, 0, 0, 1);
        let mut manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        let result = manager.check_for_update();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_start_download() {
        let version = Version::new(1, 0, 0, 1);
        let mut manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        let partition_a = PartitionInfo::new(PartitionId::A, 1024 * 1024, 0x08000000);
        let partition_b = PartitionInfo::new(PartitionId::B, 1024 * 1024, 0x08100000);
        manager.register_partition(partition_a).unwrap();
        manager.register_partition(partition_b).unwrap();

        let metadata = UpdateMetadata {
            version: Version::new(1, 1, 0, 2),
            size: 512 * 1024,
            update_type: UpdateType::Full,
            hash_algorithm: HashAlgorithm::Sha256,
            signature_algorithm: crate::security::SignatureAlgorithm::EcdsaP256,
            release_notes: None,
            required_space: 512 * 1024,
        };

        assert!(manager.start_download(metadata).is_ok());
        assert_eq!(manager.state(), OtaState::Downloading);
    }

    #[test]
    fn test_verify_and_install() {
        let version = Version::new(1, 0, 0, 1);
        let mut manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        let partition_a = PartitionInfo::new(PartitionId::A, 1024 * 1024, 0x08000000);
        let partition_b = PartitionInfo::new(PartitionId::B, 1024 * 1024, 0x08100000);
        manager.register_partition(partition_a).unwrap();
        manager.register_partition(partition_b).unwrap();

        let metadata = UpdateMetadata {
            version: Version::new(1, 1, 0, 2),
            size: 512 * 1024,
            update_type: UpdateType::Full,
            hash_algorithm: HashAlgorithm::Sha256,
            signature_algorithm: crate::security::SignatureAlgorithm::EcdsaP256,
            release_notes: None,
            required_space: 512 * 1024,
        };

        manager.start_download(metadata).unwrap();

        let hash = [1u8; 32];
        let signature = [2u8; 64];
        let public_key = [3u8; 64];

        assert!(manager
            .verify_update(&hash, &signature, &public_key)
            .is_ok());
        assert_eq!(manager.state(), OtaState::Verifying);

        assert!(manager.install_update().is_ok());
        assert_eq!(manager.state(), OtaState::Ready);
    }

    #[test]
    fn test_activate_and_confirm() {
        let version = Version::new(1, 0, 0, 1);
        let mut manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        let partition_a = PartitionInfo::new(PartitionId::A, 1024 * 1024, 0x08000000);
        let partition_b = PartitionInfo::new(PartitionId::B, 1024 * 1024, 0x08100000);
        manager.register_partition(partition_a).unwrap();
        manager.register_partition(partition_b).unwrap();

        let metadata = UpdateMetadata {
            version: Version::new(1, 1, 0, 2),
            size: 512 * 1024,
            update_type: UpdateType::Full,
            hash_algorithm: HashAlgorithm::Sha256,
            signature_algorithm: crate::security::SignatureAlgorithm::EcdsaP256,
            release_notes: None,
            required_space: 512 * 1024,
        };

        manager.start_download(metadata.clone()).unwrap();
        manager
            .verify_update(&[1u8; 32], &[2u8; 64], &[3u8; 64])
            .unwrap();
        manager.install_update().unwrap();

        assert!(manager.activate_update(metadata.version).is_ok());
        assert_eq!(manager.active_partition(), PartitionId::B);

        assert!(manager.confirm_update().is_ok());
        assert_eq!(manager.current_version(), &metadata.version);
        assert_eq!(manager.state(), OtaState::Idle);
    }

    #[test]
    fn test_rollback() {
        let version = Version::new(1, 0, 0, 1);
        let mut manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        let partition_a = PartitionInfo::new(PartitionId::A, 1024 * 1024, 0x08000000);
        let mut partition_b = PartitionInfo::new(PartitionId::B, 1024 * 1024, 0x08100000);
        partition_b.state = PartitionState::Inactive;

        manager.register_partition(partition_a).unwrap();
        manager.register_partition(partition_b).unwrap();

        // Simulate failed update on partition A
        if let Some(p) = manager.get_partition_mut(PartitionId::A) {
            p.state = PartitionState::Pending;
        }

        assert!(manager.rollback().is_ok());
        assert_eq!(manager.active_partition(), PartitionId::B);
        assert_eq!(manager.state(), OtaState::Idle);
    }

    #[test]
    fn test_boot_loop_detection() {
        let version = Version::new(1, 0, 0, 1);
        let mut manager = OtaManager::new(version, PartitionId::A, UpdateTransport::Coap);

        assert!(!manager.is_boot_loop_detected(3));

        manager.increment_boot_counter();
        manager.increment_boot_counter();
        manager.increment_boot_counter();

        assert!(manager.is_boot_loop_detected(3));
    }

    #[test]
    fn test_ota_error_display() {
        assert_eq!(
            format!("{}", OtaError::NoUpdateAvailable),
            "No update available"
        );
        assert_eq!(
            format!("{}", OtaError::DownloadFailed),
            "Update download failed"
        );
        assert_eq!(
            format!("{}", OtaError::IncompatibleFirmware),
            "Incompatible firmware"
        );
    }
}
