//! Security and OTA Update Example
//!
//! This example demonstrates:
//! - Secure boot verification
//! - Encrypted firmware update process
//! - OTA update with A/B partitioning
//! - Rollback on failure
//! - Secure element integration

#![no_std]
#![no_main]

extern crate alloc;
extern crate panic_halt;

use core::alloc::Layout;

#[global_allocator]
static ALLOCATOR: DummyAllocator = DummyAllocator;

struct DummyAllocator;

unsafe impl core::alloc::GlobalAlloc for DummyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

use mielin_rt::ota::{OtaManager, PartitionId, PartitionInfo, UpdateTransport, Version};
use mielin_rt::security::{
    BootStage, EncryptedFirmwareUpdate, EncryptionAlgorithm, FirmwareMetadata, HashAlgorithm,
    SecureBootVerifier, SecureElement, SecureElementType, SignatureAlgorithm,
};

#[no_mangle]
pub extern "C" fn main() -> ! {
    // Example 1: Secure Boot Verification
    secure_boot_example();

    // Example 2: OTA Update with Security
    ota_update_example();

    // Example 3: Secure Element Integration
    secure_element_example();

    // Example 4: Encrypted Firmware Update
    encrypted_update_example();

    loop {}
}

fn secure_boot_example() {
    // Initialize secure boot verifier
    let mut verifier = SecureBootVerifier::new(
        BootStage::Application,
        HashAlgorithm::Sha256,
        SignatureAlgorithm::EcdsaP256,
    );

    // Firmware metadata
    let metadata = FirmwareMetadata {
        version: 1,
        timestamp: 1703865600, // 2024-12-29
        hash_algorithm: HashAlgorithm::Sha256,
        signature_algorithm: SignatureAlgorithm::EcdsaP256,
        image_size: 524288, // 512 KB
        boot_stage: BootStage::Application,
    };

    // Verify metadata
    match verifier.verify_metadata(&metadata) {
        Ok(_) => {
            // Metadata is valid

            // Simulate firmware hash
            let firmware_data = [0u8; 1024]; // Would be actual firmware
            let expected_hash = [0u8; 32];

            // Verify hash
            match verifier.verify_hash(&firmware_data, &expected_hash) {
                Ok(_) => {
                    // Hash verified

                    // Verify signature
                    let signature = [1u8; 64]; // Non-zero signature
                    let public_key = [2u8; 64];

                    match verifier.verify_signature(&firmware_data, &signature, &public_key) {
                        Ok(_) => {
                            // Boot firmware - all checks passed
                            // Update rollback counter
                            let _ = verifier.update_rollback_counter(metadata.version);
                        }
                        Err(_) => {
                            // Invalid signature - halt boot
                        }
                    }
                }
                Err(_) => {
                    // Invalid hash - halt boot
                }
            }
        }
        Err(_) => {
            // Invalid metadata - halt boot
        }
    }
}

fn ota_update_example() {
    let current_version = Version::new(1, 0, 0, 1);
    let mut ota_manager = OtaManager::new(current_version, PartitionId::A, UpdateTransport::Coap);

    // Register partitions
    let partition_a = PartitionInfo::new(PartitionId::A, 1024 * 1024, 0x08000000);
    let partition_b = PartitionInfo::new(PartitionId::B, 1024 * 1024, 0x08100000);
    let _ = ota_manager.register_partition(partition_a);
    let _ = ota_manager.register_partition(partition_b);

    // Check for updates
    match ota_manager.check_for_update() {
        Ok(Some(metadata)) => {
            // Update available
            let new_version = metadata.version;

            // Start download
            if ota_manager.start_download(metadata).is_ok() {
                // Download chunks
                let chunk = [0xAA; 1024];
                let mut offset = 0;

                while offset < 512 * 1024 {
                    if ota_manager.process_chunk(&chunk, offset).is_ok() {
                        offset += chunk.len();
                    } else {
                        break;
                    }
                }

                // Verify update
                let hash = [1u8; 32];
                let signature = [2u8; 64];
                let public_key = [3u8; 64];

                if ota_manager
                    .verify_update(&hash, &signature, &public_key)
                    .is_ok()
                {
                    // Install update
                    if ota_manager.install_update().is_ok() {
                        // Activate update (will reboot)
                        let _ = ota_manager.activate_update(new_version);

                        // After reboot, confirm update
                        // if ota_manager.confirm_update().is_ok() {
                        //     // Update successful
                        // } else {
                        //     // Rollback if confirmation fails
                        //     let _ = ota_manager.rollback();
                        // }
                    }
                }
            }
        }
        Ok(None) => {
            // No update available
        }
        Err(_) => {
            // Error checking for update
        }
    }

    // Check boot loop detection
    if ota_manager.is_boot_loop_detected(3) {
        // Too many failed boots - rollback
        let _ = ota_manager.rollback();
    }
}

fn secure_element_example() {
    let mut secure_element = SecureElement::new(SecureElementType::Atecc608);

    // Initialize secure element
    if secure_element.initialize().is_ok() {
        // Generate key in slot 0
        if secure_element.generate_key(0).is_ok() {
            // Sign data using secure element
            let data = b"Important data to sign";
            let mut signature = [0u8; 64];

            match secure_element.sign(0, data, &mut signature) {
                Ok(_sig_len) => {
                    // Signature generated

                    // Verify signature
                    match secure_element.verify(0, data, &signature) {
                        Ok(true) => {
                            // Signature valid
                        }
                        Ok(false) => {
                            // Signature invalid
                        }
                        Err(_) => {
                            // Verification error
                        }
                    }
                }
                Err(_) => {
                    // Signing error
                }
            }
        }
    }
}

fn encrypted_update_example() {
    let mut encrypted_update = EncryptedFirmwareUpdate::new(EncryptionAlgorithm::Aes256Gcm);

    // Start encrypted update
    let total_size = 512 * 1024;
    if encrypted_update.start_update(total_size).is_ok() {
        // Process encrypted chunks
        let encrypted_chunk = [0xCC; 1024];
        let nonce = [0x12; 12];
        let tag = [0x34; 16];
        let mut decrypted = [0u8; 1024];

        let mut processed = 0;
        while processed < total_size {
            match encrypted_update.process_chunk(&encrypted_chunk, &nonce, &tag, &mut decrypted) {
                Ok(len) => {
                    // Write decrypted data to flash
                    processed += len;

                    // Check progress
                    let progress = encrypted_update.progress();
                    if progress.is_multiple_of(10) {
                        // Update progress indicator
                    }
                }
                Err(_) => {
                    // Decryption error - cancel update
                    encrypted_update.cancel_update();
                    break;
                }
            }
        }

        // Finalize update
        if encrypted_update.finalize_update().is_ok() {
            // Update complete - verify and activate
        }
    }
}
