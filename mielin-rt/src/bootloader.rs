//! Cortex-M Bootloader Module
//!
//! Provides a complete A/B partition bootloader for Cortex-M microcontrollers with:
//! - Image header parsing and validation (magic, size, hash)
//! - Anti-rollback protection via monotonic counter
//! - Boot state persistence with serialization/checksum
//! - Partition selection with trial/confirm semantics
//! - ARM-only jump-to-application trampoline (cfg-gated)

#![allow(dead_code)]

use core::fmt;

use crate::ota::{OtaError, PartitionId, PartitionInfo, MAX_PARTITIONS};
use crate::security::{
    BootStage, FirmwareMetadata, HardwareCrypto, HashAlgorithm, SecureBootVerifier, SecurityError,
    SignatureAlgorithm,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Magic number embedded at byte offset 0 of every valid image header.
/// Little-endian encoding of ASCII "MELN".
pub const IMAGE_MAGIC: u32 = 0x4E4C_454D;

/// Size of the image header in bytes.
/// Layout: magic(4) + version(4) + image_size(4) + initial_sp(4) +
///         reset_vector(4) + reserved(12) + hash(32) = 64 bytes.
pub const IMAGE_HEADER_SIZE: usize = 64;

/// Minimum number of entries in an ARM vector table (SP + Reset).
pub const VECTOR_TABLE_MIN_SIZE: usize = 8;

/// Magic number stored at the beginning of the persisted boot-state page.
/// Little-endian encoding of ASCII "BTSM".
pub const BOOT_STATE_MAGIC: u32 = 0x5453_5442;

/// Total size of the serialised boot-state record in bytes.
pub const BOOT_STATE_SIZE: usize = 64;

/// Default maximum number of consecutive boot attempts before a slot is
/// marked bad and the bootloader falls back.
pub const DEFAULT_MAX_BOOT_ATTEMPTS: u8 = 3;

/// Page size assumed for the in-memory flash simulation.
pub const FAKE_FLASH_PAGE_SIZE: usize = 256;

/// Byte value of an erased flash cell (NOR flash convention).
pub const FLASH_ERASED_BYTE: u8 = 0xFF;

// ---------------------------------------------------------------------------
// BootloaderError
// ---------------------------------------------------------------------------

/// All errors that can be produced by the bootloader module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootloaderError {
    /// No partition is in a bootable state.
    NoBootablePartition,
    /// Image header magic bytes do not match [`IMAGE_MAGIC`].
    InvalidMagic,
    /// Image size field is zero or exceeds the partition.
    InvalidImageSize,
    /// SHA-256 hash of the payload does not match the header hash.
    IntegrityCheckFailed,
    /// Image version is lower than the stored rollback counter.
    RollbackViolation,
    /// Initial stack-pointer value is outside RAM bounds or misaligned.
    InvalidStackPointer,
    /// Reset-vector address is implausible (outside flash or Thumb bit clear).
    InvalidResetVector,
    /// The partition is smaller than [`VECTOR_TABLE_MIN_SIZE`] bytes.
    ImageTooShort,
    /// The persisted boot-state page has a bad magic or checksum.
    CorruptBootState,
    /// Flash read operation failed.
    FlashReadFailed,
    /// Flash write operation failed.
    FlashWriteFailed,
    /// Flash erase operation failed.
    FlashEraseFailed,
    /// Address or length exceeds the flash region bounds.
    OutOfBounds,
    /// Write attempted to a flash cell that has not been erased.
    WriteNotErased,
    /// Boot-attempt counter has reached the configured maximum.
    BootAttemptsExhausted,
    /// Operation is not valid in the current state.
    InvalidState,
    /// Requested partition is not registered.
    PartitionNotFound,
    /// Wraps an OTA sub-error.
    Ota(OtaError),
    /// Wraps a security sub-error.
    Security(SecurityError),
}

impl fmt::Display for BootloaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBootablePartition => write!(f, "No bootable partition found"),
            Self::InvalidMagic => write!(f, "Invalid image magic bytes"),
            Self::InvalidImageSize => write!(f, "Invalid image size in header"),
            Self::IntegrityCheckFailed => write!(f, "Image integrity check failed"),
            Self::RollbackViolation => write!(f, "Image version violates rollback counter"),
            Self::InvalidStackPointer => write!(f, "Initial stack pointer is implausible"),
            Self::InvalidResetVector => write!(f, "Reset vector is implausible"),
            Self::ImageTooShort => write!(f, "Partition too small for a valid vector table"),
            Self::CorruptBootState => write!(f, "Persisted boot state is corrupt"),
            Self::FlashReadFailed => write!(f, "Flash read failed"),
            Self::FlashWriteFailed => write!(f, "Flash write failed"),
            Self::FlashEraseFailed => write!(f, "Flash erase failed"),
            Self::OutOfBounds => write!(f, "Flash access out of bounds"),
            Self::WriteNotErased => write!(f, "Flash write to non-erased cell"),
            Self::BootAttemptsExhausted => write!(f, "Boot attempt limit exceeded"),
            Self::InvalidState => write!(f, "Invalid bootloader state for this operation"),
            Self::PartitionNotFound => write!(f, "Partition not registered"),
            Self::Ota(e) => write!(f, "OTA error: {}", e),
            Self::Security(e) => write!(f, "Security error: {}", e),
        }
    }
}

impl From<OtaError> for BootloaderError {
    fn from(e: OtaError) -> Self {
        Self::Ota(e)
    }
}

impl From<SecurityError> for BootloaderError {
    fn from(e: SecurityError) -> Self {
        Self::Security(e)
    }
}

// ---------------------------------------------------------------------------
// FlashRegion trait
// ---------------------------------------------------------------------------

/// Abstraction over a flash memory region accessible by the bootloader.
pub trait FlashRegion {
    /// Total size of this region in bytes.
    fn size(&self) -> usize;

    /// Erase granularity in bytes.  Erases must be aligned to this boundary.
    fn page_size(&self) -> usize;

    /// Copy `buf.len()` bytes starting at `offset` into `buf`.
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<(), BootloaderError>;

    /// Erase `len` bytes starting at `offset`.  Both must be page-aligned.
    fn erase(&mut self, offset: usize, len: usize) -> Result<(), BootloaderError>;

    /// Write `data` to flash starting at `offset`.
    /// Every target byte must be in the erased state ([`FLASH_ERASED_BYTE`]).
    fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), BootloaderError>;

    /// Read a little-endian `u32` from `offset`.
    fn read_u32(&self, offset: usize) -> Result<u32, BootloaderError> {
        let mut b = [0u8; 4];
        self.read(offset, &mut b)?;
        Ok(u32::from_le_bytes(b))
    }
}

// ---------------------------------------------------------------------------
// MemoryFlash — in-memory simulation
// ---------------------------------------------------------------------------

/// Const-generic in-memory flash simulation used in tests and host builds.
pub struct MemoryFlash<const N: usize> {
    data: [u8; N],
    page_size: usize,
    enforce_erase: bool,
}

impl<const N: usize> MemoryFlash<N> {
    /// Create a new in-memory flash region, initialised to [`FLASH_ERASED_BYTE`].
    pub const fn new(page_size: usize) -> Self {
        Self {
            data: [FLASH_ERASED_BYTE; N],
            page_size,
            enforce_erase: true,
        }
    }

    /// Write the Cortex-M vector table stub (SP, Reset) at `offset`.
    /// Uses `load_bytes` internally so it bypasses the erase check — call
    /// this only on a freshly initialised region or after erasing.
    pub fn set_vector_table(
        &mut self,
        offset: usize,
        sp: u32,
        reset: u32,
    ) -> Result<(), BootloaderError> {
        if offset + 8 > N {
            return Err(BootloaderError::OutOfBounds);
        }
        let sp_bytes = sp.to_le_bytes();
        let rv_bytes = reset.to_le_bytes();
        self.data[offset..offset + 4].copy_from_slice(&sp_bytes);
        self.data[offset + 4..offset + 8].copy_from_slice(&rv_bytes);
        Ok(())
    }

    /// Force-write `data` at `offset`, ignoring the erase-enforcement flag.
    /// Useful for staging a complete image in tests.
    pub fn load_bytes(&mut self, offset: usize, data: &[u8]) {
        let end = (offset + data.len()).min(N);
        let copy_len = end - offset;
        self.data[offset..end].copy_from_slice(&data[..copy_len]);
    }

    /// Return a slice of the raw backing buffer (read-only).
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl<const N: usize> FlashRegion for MemoryFlash<N> {
    fn size(&self) -> usize {
        N
    }

    fn page_size(&self) -> usize {
        self.page_size
    }

    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<(), BootloaderError> {
        if offset + buf.len() > N {
            return Err(BootloaderError::OutOfBounds);
        }
        buf.copy_from_slice(&self.data[offset..offset + buf.len()]);
        Ok(())
    }

    fn erase(&mut self, offset: usize, len: usize) -> Result<(), BootloaderError> {
        if offset + len > N {
            return Err(BootloaderError::OutOfBounds);
        }
        // Enforce page alignment for offset and len
        if !offset.is_multiple_of(self.page_size) || !len.is_multiple_of(self.page_size) {
            return Err(BootloaderError::FlashEraseFailed);
        }
        for b in &mut self.data[offset..offset + len] {
            *b = FLASH_ERASED_BYTE;
        }
        Ok(())
    }

    fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), BootloaderError> {
        if offset + data.len() > N {
            return Err(BootloaderError::OutOfBounds);
        }
        if self.enforce_erase {
            for &b in &self.data[offset..offset + data.len()] {
                if b != FLASH_ERASED_BYTE {
                    return Err(BootloaderError::WriteNotErased);
                }
            }
        }
        self.data[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AppImageEntry — ARM execution entry point
// ---------------------------------------------------------------------------

/// The two values read from a Cortex-M vector table needed to jump to an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppImageEntry {
    /// Initial stack pointer value loaded into MSP.
    pub initial_sp: u32,
    /// Reset handler address (must have Thumb bit set, i.e., LSB == 1).
    pub reset_vector: u32,
}

impl AppImageEntry {
    /// Construct an entry from literal values.
    pub const fn new(initial_sp: u32, reset_vector: u32) -> Self {
        Self {
            initial_sp,
            reset_vector,
        }
    }

    /// Return `true` if `initial_sp` lies within `[ram_start, ram_end]` and
    /// is 4-byte aligned.
    pub fn is_sp_plausible(&self, ram_start: u32, ram_end: u32) -> bool {
        self.initial_sp >= ram_start && self.initial_sp <= ram_end && self.initial_sp & 3 == 0
    }

    /// Return `true` if `reset_vector` has its Thumb bit set and the
    /// target address (without LSB) lies within `[flash_start, flash_end)`.
    pub fn is_reset_plausible(&self, flash_start: u32, flash_end: u32) -> bool {
        let addr = self.reset_vector & !1u32;
        self.reset_vector & 1 == 1 && addr >= flash_start && addr < flash_end
    }
}

// ---------------------------------------------------------------------------
// ImageHeader — parsed representation of the 64-byte image header
// ---------------------------------------------------------------------------

/// Parsed representation of the 64-byte image header.
///
/// Layout at offset 0 of a partition:
/// ```text
/// [0..4]   magic        u32 LE
/// [4..8]   version      u32 LE
/// [8..12]  image_size   u32 LE
/// [12..16] initial_sp   u32 LE
/// [16..20] reset_vector u32 LE
/// [20..32] reserved     12 bytes (zeroed)
/// [32..64] hash         32 bytes SHA-256 over bytes [64 .. 64+image_size]
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ImageHeader {
    /// Must equal [`IMAGE_MAGIC`].
    pub magic: u32,
    /// Firmware version; must be >= rollback counter.
    pub version: u32,
    /// Size of the payload that follows the 64-byte header, in bytes.
    pub image_size: u32,
    /// Initial main-stack-pointer value (loaded into MSP before jump).
    pub initial_sp: u32,
    /// Reset-handler address in Thumb representation (LSB = 1).
    pub reset_vector: u32,
    /// SHA-256 hash over the payload bytes.
    pub hash: [u8; 32],
}

/// Parse the 64-byte image header from a flash region at the given offset.
fn parse_header<F: FlashRegion>(flash: &F, offset: usize) -> Result<ImageHeader, BootloaderError> {
    if offset + IMAGE_HEADER_SIZE > flash.size() {
        return Err(BootloaderError::OutOfBounds);
    }

    let magic = flash.read_u32(offset)?;
    let version = flash.read_u32(offset + 4)?;
    let image_size = flash.read_u32(offset + 8)?;
    let initial_sp = flash.read_u32(offset + 12)?;
    let reset_vector = flash.read_u32(offset + 16)?;

    // Skip reserved bytes [20..32]
    let mut hash = [0u8; 32];
    flash.read(offset + 32, &mut hash)?;

    Ok(ImageHeader {
        magic,
        version,
        image_size,
        initial_sp,
        reset_vector,
        hash,
    })
}

/// Build a correctly-structured 64-byte header blob from an [`ImageHeader`].
/// Used in tests to stage a complete image.
pub fn encode_header(hdr: &ImageHeader) -> [u8; IMAGE_HEADER_SIZE] {
    let mut buf = [0u8; IMAGE_HEADER_SIZE];
    buf[0..4].copy_from_slice(&hdr.magic.to_le_bytes());
    buf[4..8].copy_from_slice(&hdr.version.to_le_bytes());
    buf[8..12].copy_from_slice(&hdr.image_size.to_le_bytes());
    buf[12..16].copy_from_slice(&hdr.initial_sp.to_le_bytes());
    buf[16..20].copy_from_slice(&hdr.reset_vector.to_le_bytes());
    // [20..32] reserved — already zero
    buf[32..64].copy_from_slice(&hdr.hash);
    buf
}

// ---------------------------------------------------------------------------
// PartitionId helpers
// ---------------------------------------------------------------------------

/// Convert a [`PartitionId`] to its fixed single-byte representation.
const fn partition_id_to_u8(id: PartitionId) -> u8 {
    match id {
        PartitionId::A => 0,
        PartitionId::B => 1,
        PartitionId::Recovery => 2,
        PartitionId::Factory => 3,
    }
}

/// Convert a byte back to a [`PartitionId`].  Returns `None` for unknown values.
const fn partition_id_from_u8(v: u8) -> Option<PartitionId> {
    match v {
        0 => Some(PartitionId::A),
        1 => Some(PartitionId::B),
        2 => Some(PartitionId::Recovery),
        3 => Some(PartitionId::Factory),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// BootSlotState
// ---------------------------------------------------------------------------

/// Per-partition runtime state tracked across reboots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootSlotState {
    /// Which partition this slot corresponds to.
    pub partition: PartitionId,
    /// The slot has booted successfully and been confirmed by the application.
    pub confirmed: bool,
    /// The slot is undergoing a trial boot (will be confirmed or rolled back).
    pub trial: bool,
    /// Number of boot attempts since the last confirmation.
    pub boot_attempts: u8,
    /// The slot has been marked bad (too many failures or explicit rejection).
    pub bad: bool,
    /// Firmware version stored in this slot (0 = unknown).
    pub version: u32,
}

impl BootSlotState {
    /// Construct a fresh slot state for `partition`.
    pub const fn new(partition: PartitionId) -> Self {
        Self {
            partition,
            confirmed: false,
            trial: false,
            boot_attempts: 0,
            bad: false,
            version: 0,
        }
    }

    /// `true` iff the slot has not been marked bad.
    pub const fn is_bootable(&self) -> bool {
        !self.bad
    }

    /// `true` iff the slot has been confirmed and is not bad.
    pub const fn is_known_good(&self) -> bool {
        self.confirmed && !self.bad
    }
}

// ---------------------------------------------------------------------------
// BootState
// ---------------------------------------------------------------------------

/// Global boot state persisted to flash.
#[derive(Debug, Clone, Copy)]
pub struct BootState {
    /// The partition selected for the current / next boot.
    pub selected: PartitionId,
    /// Monotonically increasing rollback counter.  Images with a version
    /// field below this value will be rejected.
    pub rollback_counter: u32,
    /// Maximum allowed consecutive boot attempts for a trial slot.
    pub max_attempts: u8,
    /// Per-slot state records (indexed 0..slot_count).
    pub slots: [BootSlotState; MAX_PARTITIONS],
    /// Number of valid entries in `slots`.
    pub slot_count: usize,
}

impl BootState {
    /// Construct a new boot state with default per-slot entries.
    pub fn new(selected: PartitionId, max_attempts: u8) -> Self {
        Self {
            selected,
            rollback_counter: 0,
            max_attempts,
            slots: [
                BootSlotState::new(PartitionId::A),
                BootSlotState::new(PartitionId::B),
                BootSlotState::new(PartitionId::Recovery),
                BootSlotState::new(PartitionId::Factory),
            ],
            slot_count: 0,
        }
    }

    /// Create a sane default state (used when the persisted page is corrupt).
    pub fn default_state(selected: PartitionId) -> Self {
        Self::new(selected, DEFAULT_MAX_BOOT_ATTEMPTS)
    }

    /// Look up a slot by partition id (immutable).
    pub fn slot(&self, id: PartitionId) -> Option<&BootSlotState> {
        self.slots[..self.slot_count]
            .iter()
            .find(|s| s.partition == id)
    }

    /// Look up a slot by partition id (mutable).
    pub fn slot_mut(&mut self, id: PartitionId) -> Option<&mut BootSlotState> {
        self.slots[..self.slot_count]
            .iter_mut()
            .find(|s| s.partition == id)
    }

    /// Serialise the boot state into a fixed [`BOOT_STATE_SIZE`]-byte array.
    ///
    /// Layout (bytes):
    /// ```text
    ///  0- 3   BOOT_STATE_MAGIC     u32 LE
    ///  4      selected             u8
    ///  5- 8   rollback_counter     u32 LE
    ///  9      max_attempts         u8
    /// 10      slot_count           u8
    /// 11-38   per-slot (7 bytes each × 4 slots max)
    ///         partition(1) + flags(1) + attempts(1) + version(4)
    /// 56-59   checksum             u32 LE (additive sum of bytes [0..56])
    /// 60-63   padding              zeroed
    /// ```
    pub fn serialize(&self) -> [u8; BOOT_STATE_SIZE] {
        let mut buf = [0u8; BOOT_STATE_SIZE];

        buf[0..4].copy_from_slice(&BOOT_STATE_MAGIC.to_le_bytes());
        buf[4] = partition_id_to_u8(self.selected);
        buf[5..9].copy_from_slice(&self.rollback_counter.to_le_bytes());
        buf[9] = self.max_attempts;
        buf[10] = self.slot_count as u8;

        let mut cursor = 11usize;
        for slot in &self.slots[..self.slot_count] {
            buf[cursor] = partition_id_to_u8(slot.partition);
            let flags =
                (slot.confirmed as u8) | ((slot.trial as u8) << 1) | ((slot.bad as u8) << 2);
            buf[cursor + 1] = flags;
            buf[cursor + 2] = slot.boot_attempts;
            buf[cursor + 3..cursor + 7].copy_from_slice(&slot.version.to_le_bytes());
            cursor += 7;
        }

        // Additive checksum over bytes [0..56]
        let checksum_offset = BOOT_STATE_SIZE - 8; // byte 56
        let checksum: u32 = buf[..checksum_offset]
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
        buf[checksum_offset..checksum_offset + 4].copy_from_slice(&checksum.to_le_bytes());

        buf
    }

    /// Deserialise a boot state from a byte slice.
    ///
    /// Returns [`BootloaderError::CorruptBootState`] if the magic word or
    /// checksum does not match.
    pub fn deserialize(bytes: &[u8]) -> Result<Self, BootloaderError> {
        if bytes.len() < BOOT_STATE_SIZE {
            return Err(BootloaderError::CorruptBootState);
        }

        // Verify magic
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != BOOT_STATE_MAGIC {
            return Err(BootloaderError::CorruptBootState);
        }

        // Verify checksum over bytes [0..56]
        let checksum_offset = BOOT_STATE_SIZE - 8;
        let stored_checksum = u32::from_le_bytes([
            bytes[checksum_offset],
            bytes[checksum_offset + 1],
            bytes[checksum_offset + 2],
            bytes[checksum_offset + 3],
        ]);
        let computed_checksum: u32 = bytes[..checksum_offset]
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
        if stored_checksum != computed_checksum {
            return Err(BootloaderError::CorruptBootState);
        }

        let selected = partition_id_from_u8(bytes[4]).ok_or(BootloaderError::CorruptBootState)?;
        let rollback_counter = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        let max_attempts = bytes[9];
        let slot_count = bytes[10] as usize;

        if slot_count > MAX_PARTITIONS {
            return Err(BootloaderError::CorruptBootState);
        }

        let mut slots = [
            BootSlotState::new(PartitionId::A),
            BootSlotState::new(PartitionId::B),
            BootSlotState::new(PartitionId::Recovery),
            BootSlotState::new(PartitionId::Factory),
        ];

        let mut cursor = 11usize;
        for slot in slots[..slot_count].iter_mut() {
            let partition =
                partition_id_from_u8(bytes[cursor]).ok_or(BootloaderError::CorruptBootState)?;
            let flags = bytes[cursor + 1];
            let confirmed = flags & 1 != 0;
            let trial = flags & 2 != 0;
            let bad = flags & 4 != 0;
            let boot_attempts = bytes[cursor + 2];
            let version = u32::from_le_bytes([
                bytes[cursor + 3],
                bytes[cursor + 4],
                bytes[cursor + 5],
                bytes[cursor + 6],
            ]);
            *slot = BootSlotState {
                partition,
                confirmed,
                trial,
                boot_attempts,
                bad,
                version,
            };
            cursor += 7;
        }

        Ok(Self {
            selected,
            rollback_counter,
            max_attempts,
            slots,
            slot_count,
        })
    }
}

// ---------------------------------------------------------------------------
// Bootloader
// ---------------------------------------------------------------------------

/// The main bootloader structure.
pub struct Bootloader<F: FlashRegion> {
    /// The flash region backing all partitions and boot state.
    flash: F,
    /// Base address offset of the flash region in the memory map.
    /// Used when checking address plausibility.
    flash_base: usize,
    /// Byte offset within `flash` where the boot-state page lives.
    state_offset: usize,
    /// Start address of RAM (for SP plausibility check).
    ram_start: u32,
    /// End address of RAM (inclusive upper bound for SP check).
    ram_end: u32,
    /// Registered partition metadata, indexed 0..partition_count.
    partitions: [PartitionInfo; MAX_PARTITIONS],
    /// Number of registered partitions.
    partition_count: usize,
    /// Secure-boot verifier (hash + metadata checks).
    verifier: SecureBootVerifier,
    /// Hardware crypto accelerator (SHA-256).
    crypto: HardwareCrypto,
    /// Current boot state (loaded from flash or defaulted).
    pub state: BootState,
}

impl<F: FlashRegion> Bootloader<F> {
    /// Create a new bootloader instance.
    ///
    /// `crypto` will be initialised immediately (calls `initialize()`).
    pub fn new(
        flash: F,
        flash_base: usize,
        state_offset: usize,
        ram_start: u32,
        ram_end: u32,
        verifier: SecureBootVerifier,
        mut crypto: HardwareCrypto,
    ) -> Self {
        // Best-effort initialisation; errors are non-fatal at construction time.
        let _ = crypto.initialize();
        Self {
            flash,
            flash_base,
            state_offset,
            ram_start,
            ram_end,
            partitions: [
                PartitionInfo::new(PartitionId::A, 0, 0),
                PartitionInfo::new(PartitionId::B, 0, 0),
                PartitionInfo::new(PartitionId::Recovery, 0, 0),
                PartitionInfo::new(PartitionId::Factory, 0, 0),
            ],
            partition_count: 0,
            verifier,
            crypto,
            state: BootState::default_state(PartitionId::A),
        }
    }

    /// Register a partition.  Up to [`MAX_PARTITIONS`] partitions may be
    /// registered; returns [`BootloaderError::Ota`] wrapping
    /// [`OtaError::NoSpace`] when the limit is reached.
    pub fn register_partition(&mut self, info: PartitionInfo) -> Result<(), BootloaderError> {
        if self.partition_count >= MAX_PARTITIONS {
            return Err(BootloaderError::Ota(OtaError::NoSpace));
        }
        self.partitions[self.partition_count] = info;
        self.partition_count += 1;
        Ok(())
    }

    /// Look up a registered partition by id.
    pub fn get_partition(&self, id: PartitionId) -> Option<&PartitionInfo> {
        self.partitions[..self.partition_count]
            .iter()
            .find(|p| p.id == id)
    }

    /// Compute the byte offset within `self.flash` for the start of a
    /// partition, given that partitions are addressed by `start_address`
    /// relative to `flash_base`.
    fn region_offset(&self, partition: &PartitionInfo) -> Result<usize, BootloaderError> {
        let abs = partition.start_address;
        if abs < self.flash_base {
            return Err(BootloaderError::OutOfBounds);
        }
        let offset = abs - self.flash_base;
        if offset + partition.size > self.flash.size() {
            return Err(BootloaderError::OutOfBounds);
        }
        Ok(offset)
    }

    // -----------------------------------------------------------------------
    // Partition selection
    // -----------------------------------------------------------------------

    /// Select the partition to boot, following this priority order:
    ///
    /// 1. Newest confirmed A/B slot (highest version, not bad, confirmed).
    /// 2. Trial A/B slot with `boot_attempts < max_attempts`.
    /// 3. The other confirmed A/B slot (fallback known-good).
    /// 4. Recovery partition (if registered and not bad).
    /// 5. Factory partition (if registered and not bad).
    /// 6. [`BootloaderError::NoBootablePartition`]
    ///
    /// Anti-rollback: any slot whose `version` < `state.rollback_counter`
    /// is skipped (except Recovery and Factory which are always trusted).
    pub fn select_boot_partition(&self) -> Result<PartitionId, BootloaderError> {
        let rc = self.state.rollback_counter;

        let ab_ids = [PartitionId::A, PartitionId::B];

        // Phase 1: best confirmed A/B with version >= rollback_counter
        let mut best_confirmed: Option<(PartitionId, u32)> = None;
        for &id in &ab_ids {
            if let Some(slot) = self.state.slot(id) {
                if slot.is_known_good() && slot.version >= rc {
                    match best_confirmed {
                        None => best_confirmed = Some((id, slot.version)),
                        Some((_, best_ver)) if slot.version > best_ver => {
                            best_confirmed = Some((id, slot.version));
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some((id, _)) = best_confirmed {
            return Ok(id);
        }

        // Phase 2: trial A/B slot with attempts left
        for &id in &ab_ids {
            if let Some(slot) = self.state.slot(id) {
                if slot.trial
                    && !slot.bad
                    && slot.boot_attempts < self.state.max_attempts
                    && slot.version >= rc
                {
                    return Ok(id);
                }
            }
        }

        // Phase 3: any non-bad A/B slot with version >= rollback_counter
        for &id in &ab_ids {
            if let Some(slot) = self.state.slot(id) {
                if slot.is_bootable() && slot.version >= rc {
                    return Ok(id);
                }
            }
        }

        // Phase 4: Recovery
        if let Some(slot) = self.state.slot(PartitionId::Recovery) {
            if slot.is_bootable() {
                return Ok(PartitionId::Recovery);
            }
        } else if self.get_partition(PartitionId::Recovery).is_some() {
            // Registered but no slot state — treat as bootable
            return Ok(PartitionId::Recovery);
        }

        // Phase 5: Factory
        if let Some(slot) = self.state.slot(PartitionId::Factory) {
            if slot.is_bootable() {
                return Ok(PartitionId::Factory);
            }
        } else if self.get_partition(PartitionId::Factory).is_some() {
            return Ok(PartitionId::Factory);
        }

        Err(BootloaderError::NoBootablePartition)
    }

    // -----------------------------------------------------------------------
    // Image validation
    // -----------------------------------------------------------------------

    /// Validate the image in `id`:
    ///
    /// 1. Parse 64-byte header.
    /// 2. Verify [`IMAGE_MAGIC`].
    /// 3. Verify `image_size` > 0 and fits in partition.
    /// 4. Verify `header.version >= state.rollback_counter`.
    /// 5. Verify SHA-256 of payload matches `header.hash`.
    /// 6. Verify `FirmwareMetadata` via the secure-boot verifier.
    ///
    /// `scratch` must be at least `image_size` bytes; the payload is read
    /// into it for hashing.
    pub fn validate_image(
        &mut self,
        id: PartitionId,
        scratch: &mut [u8],
    ) -> Result<ImageHeader, BootloaderError> {
        let partition = *self
            .get_partition(id)
            .ok_or(BootloaderError::PartitionNotFound)?;

        let part_offset = self.region_offset(&partition)?;

        // Parse header
        let hdr = parse_header(&self.flash, part_offset)?;

        // Check magic
        if hdr.magic != IMAGE_MAGIC {
            return Err(BootloaderError::InvalidMagic);
        }

        // Check image_size
        if hdr.image_size == 0
            || (hdr.image_size as usize) > partition.size.saturating_sub(IMAGE_HEADER_SIZE)
        {
            return Err(BootloaderError::InvalidImageSize);
        }

        // Anti-rollback check
        if hdr.version < self.state.rollback_counter {
            return Err(BootloaderError::RollbackViolation);
        }

        let payload_len = hdr.image_size as usize;

        // Verify metadata through SecureBootVerifier
        let metadata = FirmwareMetadata {
            version: hdr.version,
            timestamp: 0,
            hash_algorithm: HashAlgorithm::Sha256,
            signature_algorithm: SignatureAlgorithm::EcdsaP256,
            image_size: payload_len,
            boot_stage: BootStage::Application,
        };
        self.verifier
            .verify_metadata(&metadata)
            .map_err(BootloaderError::Security)?;

        // Read payload into scratch
        if scratch.len() < payload_len {
            return Err(BootloaderError::InvalidImageSize);
        }
        self.flash
            .read(part_offset + IMAGE_HEADER_SIZE, &mut scratch[..payload_len])?;

        // Compute SHA-256 hash of payload
        let mut computed_hash = [0u8; 32];
        self.crypto
            .sha256(&scratch[..payload_len], &mut computed_hash)
            .map_err(BootloaderError::Security)?;

        // Verify hash via SecureBootVerifier (uses its own hash computation)
        self.verifier
            .verify_hash(&scratch[..payload_len], &hdr.hash)
            .map_err(|_| BootloaderError::IntegrityCheckFailed)?;

        // Cross-check: hardware-computed hash must also match header hash
        if computed_hash != hdr.hash {
            return Err(BootloaderError::IntegrityCheckFailed);
        }

        Ok(hdr)
    }

    // -----------------------------------------------------------------------
    // prepare_jump
    // -----------------------------------------------------------------------

    /// Parse the image header at the start of the partition and extract the
    /// initial stack pointer and reset-vector entry point.
    ///
    /// The image header layout places `initial_sp` at bytes [12..16] and
    /// `reset_vector` at bytes [16..20].  Both values are validated for
    /// plausibility against the configured RAM and flash address ranges.
    pub fn prepare_jump(
        &self,
        partition: &PartitionInfo,
    ) -> Result<AppImageEntry, BootloaderError> {
        if partition.size < VECTOR_TABLE_MIN_SIZE {
            return Err(BootloaderError::ImageTooShort);
        }

        let offset = self.region_offset(partition)?;

        // initial_sp is at header offset 12; reset_vector at header offset 16
        let sp = self.flash.read_u32(offset + 12)?;
        let reset = self.flash.read_u32(offset + 16)?;

        let entry = AppImageEntry::new(sp, reset);

        let flash_start = self.flash_base as u32;
        let flash_end = flash_start + self.flash.size() as u32;

        if !entry.is_sp_plausible(self.ram_start, self.ram_end) {
            return Err(BootloaderError::InvalidStackPointer);
        }
        if !entry.is_reset_plausible(flash_start, flash_end) {
            return Err(BootloaderError::InvalidResetVector);
        }

        Ok(entry)
    }

    // -----------------------------------------------------------------------
    // Boot lifecycle
    // -----------------------------------------------------------------------

    /// Record a boot attempt for `id`.
    ///
    /// If the slot is in a trial/pending state:
    /// - Increment `boot_attempts`.
    /// - If attempts now exceed `max_attempts`, mark the slot bad and return
    ///   [`BootloaderError::BootAttemptsExhausted`].
    pub fn begin_boot(&mut self, id: PartitionId) -> Result<(), BootloaderError> {
        let max = self.state.max_attempts;
        let slot = self
            .state
            .slot_mut(id)
            .ok_or(BootloaderError::PartitionNotFound)?;

        if slot.trial || !slot.confirmed {
            slot.boot_attempts = slot.boot_attempts.saturating_add(1);
            if slot.boot_attempts > max {
                slot.bad = true;
                slot.trial = false;
                return Err(BootloaderError::BootAttemptsExhausted);
            }
        }
        Ok(())
    }

    /// Confirm the currently selected partition.
    ///
    /// - Marks the slot `confirmed`, clears `trial`, resets `boot_attempts`.
    /// - Advances `rollback_counter` to `max(current, slot.version)`.
    pub fn confirm(&mut self) -> Result<(), BootloaderError> {
        let selected = self.state.selected;
        let version = {
            let slot = self
                .state
                .slot_mut(selected)
                .ok_or(BootloaderError::PartitionNotFound)?;
            slot.confirmed = true;
            slot.trial = false;
            slot.boot_attempts = 0;
            slot.bad = false;
            slot.version
        };

        if version > self.state.rollback_counter {
            self.state.rollback_counter = version;
        }
        Ok(())
    }

    /// Mark a partition slot as bad (will not be selected again).
    pub fn mark_bad(&mut self, id: PartitionId) -> Result<(), BootloaderError> {
        let slot = self
            .state
            .slot_mut(id)
            .ok_or(BootloaderError::PartitionNotFound)?;
        slot.bad = true;
        slot.trial = false;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // State persistence
    // -----------------------------------------------------------------------

    /// Load the boot state from the flash page at `state_offset`.
    ///
    /// On [`BootloaderError::CorruptBootState`] the state is reset to a safe
    /// default rather than propagating the error.
    pub fn load_state(&mut self) -> Result<(), BootloaderError> {
        let mut buf = [0u8; BOOT_STATE_SIZE];
        self.flash.read(self.state_offset, &mut buf)?;

        match BootState::deserialize(&buf) {
            Ok(s) => {
                self.state = s;
                Ok(())
            }
            Err(BootloaderError::CorruptBootState) => {
                self.state = BootState::default_state(PartitionId::A);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Persist the current boot state to the flash page at `state_offset`.
    ///
    /// Erases the page first, then writes the serialised state.
    pub fn save_state(&mut self) -> Result<(), BootloaderError> {
        let page = self.flash.page_size();
        let offset = self.state_offset;

        // Align erase length to page boundary
        let erase_len = BOOT_STATE_SIZE.div_ceil(page) * page;
        self.flash.erase(offset, erase_len)?;

        let serialised = self.state.serialize();
        self.flash.write(offset, &serialised)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // High-level boot cycle
    // -----------------------------------------------------------------------

    /// Run a complete boot cycle:
    ///
    /// 1. Select the highest-priority bootable partition.
    /// 2. Validate its image.
    /// 3. Call `begin_boot` to record the attempt.
    /// 4. If any step fails, mark the partition bad and retry (up to 5 rounds).
    ///
    /// Returns the [`AppImageEntry`] for the chosen partition.  The caller is
    /// responsible for the actual jump (see [`jump_to_application`]).
    pub fn run_boot_cycle(&mut self, scratch: &mut [u8]) -> Result<AppImageEntry, BootloaderError> {
        const MAX_ROUNDS: usize = 5;

        for _ in 0..MAX_ROUNDS {
            let id = self.select_boot_partition()?;
            self.state.selected = id;

            // Validate image
            match self.validate_image(id, scratch) {
                Err(_) => {
                    // Best-effort mark bad; ignore if slot not found
                    let _ = self.mark_bad(id);
                    continue;
                }
                Ok(_hdr) => {}
            }

            // Record boot attempt
            match self.begin_boot(id) {
                Err(BootloaderError::BootAttemptsExhausted) => {
                    continue; // slot now bad, retry selection
                }
                Err(e) => return Err(e),
                Ok(()) => {}
            }

            // Prepare the jump entry
            let partition = *self
                .get_partition(id)
                .ok_or(BootloaderError::PartitionNotFound)?;
            let entry = self.prepare_jump(&partition)?;
            return Ok(entry);
        }

        Err(BootloaderError::NoBootablePartition)
    }
}

// ---------------------------------------------------------------------------
// ARM-only jump trampoline (not compiled on host)
// ---------------------------------------------------------------------------

/// Jump to the application at `entry` after relocating the vector table to
/// `table_base`.
///
/// # Safety
///
/// The caller must guarantee that:
/// - `table_base` is aligned to a 512-byte boundary (or the VTOR alignment
///   required by the specific Cortex-M variant).
/// - `entry.initial_sp` is a valid, aligned stack pointer within RAM.
/// - `entry.reset_vector` is the Thumb-mode reset handler of the application.
/// - All peripheral state required by the application has been prepared.
///
/// This function does **not** return.
#[cfg(target_arch = "arm")]
pub unsafe fn jump_to_application(table_base: u32, entry: AppImageEntry) -> ! {
    use cortex_m::peripheral::SCB;

    let scb = &*SCB::PTR;
    scb.vtor.write(table_base);

    cortex_m::asm::dsb();
    cortex_m::asm::isb();

    cortex_m::register::msp::write(entry.initial_sp);

    let reset_fn: unsafe extern "C" fn() -> ! = core::mem::transmute(entry.reset_vector as usize);
    reset_fn()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::HardwareCryptoType;
    use alloc::format;

    // -----------------------------------------------------------------------
    // Helper builders
    // -----------------------------------------------------------------------

    /// Build a default `SecureBootVerifier` for tests (Application stage).
    fn make_verifier() -> SecureBootVerifier {
        SecureBootVerifier::new(
            BootStage::Application,
            HashAlgorithm::Sha256,
            SignatureAlgorithm::EcdsaP256,
        )
    }

    /// Build a default `HardwareCrypto` for tests.
    fn make_crypto() -> HardwareCrypto {
        let mut c = HardwareCrypto::new(HardwareCryptoType::Rp2040Software);
        c.initialize().ok();
        c
    }

    /// Build a minimal `Bootloader` backed by a `MemoryFlash<N>`.
    fn make_bootloader<const N: usize>(
        flash: MemoryFlash<N>,
        state_offset: usize,
    ) -> Bootloader<MemoryFlash<N>> {
        Bootloader::new(
            flash,
            0, // flash_base = 0
            state_offset,
            0x2000_0000, // ram_start
            0x2001_0000, // ram_end
            make_verifier(),
            make_crypto(),
        )
    }

    /// Compute the expected hash that `HardwareCrypto::sha256` will produce
    /// for a given `data` slice (simulation: output[i] = data.len() + i).
    fn expected_hash(data_len: usize) -> [u8; 32] {
        let mut h = [0u8; 32];
        for (i, b) in h.iter_mut().enumerate() {
            *b = (data_len as u8).wrapping_add(i as u8);
        }
        h
    }

    /// Build a well-formed 64-byte header + payload blob of `payload_len` bytes.
    /// The hash field is filled with the simulated hash so that both
    /// `verify_hash` (which uses `compute_hash` internally in the verifier) and
    /// the hardware SHA-256 cross-check agree.
    fn build_valid_image(
        version: u32,
        payload_len: usize,
        initial_sp: u32,
        reset_vector: u32,
    ) -> heapless::Vec<u8, 8192> {
        // The SecureBootVerifier::compute_hash simulation produces:
        //   output[i] = (data.len() as u8).wrapping_add(i as u8)
        // HardwareCrypto::sha256 simulation produces the same formula.
        // So we use that same formula for the hash in the header.
        let hash = expected_hash(payload_len);
        let hdr = ImageHeader {
            magic: IMAGE_MAGIC,
            version,
            image_size: payload_len as u32,
            initial_sp,
            reset_vector,
            hash,
        };
        let hdr_bytes = encode_header(&hdr);

        let mut img: heapless::Vec<u8, 8192> = heapless::Vec::new();
        for &b in &hdr_bytes {
            img.push(b).ok();
        }
        // Fill payload with deterministic bytes
        for i in 0..payload_len {
            img.push(i as u8).ok();
        }
        img
    }

    // -----------------------------------------------------------------------
    // Flash fake tests (1-6)
    // -----------------------------------------------------------------------

    #[test]
    fn test_memory_flash_read_write_roundtrip() {
        let mut flash = MemoryFlash::<512>::new(FAKE_FLASH_PAGE_SIZE);
        flash.erase(0, FAKE_FLASH_PAGE_SIZE).unwrap();
        let payload = [0x11u8, 0x22, 0x33, 0x44, 0x55];
        flash.write(0, &payload).unwrap();
        let mut readback = [0u8; 5];
        flash.read(0, &mut readback).unwrap();
        assert_eq!(readback, payload);
    }

    #[test]
    fn test_memory_flash_erase_sets_ff() {
        let mut flash = MemoryFlash::<512>::new(FAKE_FLASH_PAGE_SIZE);
        // Force-load some non-FF bytes
        flash.load_bytes(0, &[0x00; FAKE_FLASH_PAGE_SIZE]);
        flash.erase(0, FAKE_FLASH_PAGE_SIZE).unwrap();
        let mut buf = [0u8; FAKE_FLASH_PAGE_SIZE];
        flash.read(0, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == FLASH_ERASED_BYTE));
    }

    #[test]
    fn test_memory_flash_write_requires_erase() {
        let mut flash = MemoryFlash::<512>::new(FAKE_FLASH_PAGE_SIZE);
        // Overwrite some bytes with non-FF values
        flash.load_bytes(0, &[0x00u8; 4]);
        // Now try a write to that non-erased region
        let result = flash.write(0, &[0xABu8; 4]);
        assert_eq!(result, Err(BootloaderError::WriteNotErased));
    }

    #[test]
    fn test_memory_flash_out_of_bounds() {
        let flash = MemoryFlash::<64>::new(64);
        let mut buf = [0u8; 65];
        let result = flash.read(0, &mut buf);
        assert_eq!(result, Err(BootloaderError::OutOfBounds));
    }

    #[test]
    fn test_memory_flash_read_u32_le() {
        let mut flash = MemoryFlash::<512>::new(FAKE_FLASH_PAGE_SIZE);
        // Write 0xDEADBEEF in LE at offset 8
        flash.load_bytes(8, &[0xEF, 0xBE, 0xAD, 0xDE]);
        let val = flash.read_u32(8).unwrap();
        assert_eq!(val, 0xDEAD_BEEF);
    }

    #[test]
    fn test_memory_flash_set_vector_table() {
        let mut flash = MemoryFlash::<512>::new(FAKE_FLASH_PAGE_SIZE);
        flash.set_vector_table(0, 0x2000_8000, 0x0800_0101).unwrap();
        assert_eq!(flash.read_u32(0).unwrap(), 0x2000_8000);
        assert_eq!(flash.read_u32(4).unwrap(), 0x0800_0101);
    }

    // -----------------------------------------------------------------------
    // AppImageEntry + prepare_jump (7-10)
    // -----------------------------------------------------------------------

    /// Helper: write `initial_sp` and `reset_vector` at the header-field offsets
    /// (12 and 16) within the partition so that `prepare_jump` can read them.
    fn write_header_sp_reset(
        flash: &mut MemoryFlash<{ 2 * 1024 }>,
        base: usize,
        sp: u32,
        reset: u32,
    ) {
        flash.load_bytes(base + 12, &sp.to_le_bytes());
        flash.load_bytes(base + 16, &reset.to_le_bytes());
    }

    #[test]
    fn test_prepare_jump_reads_sp_and_reset() {
        // prepare_jump reads initial_sp from header offset 12 and reset_vector
        // from header offset 16 within the partition region.
        let mut flash = MemoryFlash::<{ 2 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);
        let sp: u32 = 0x2000_8000;
        // reset within flash [0, 2048) with Thumb bit set
        let reset: u32 = 0x0000_0101;

        // Write SP at partition[12..16] and reset at partition[16..20]
        write_header_sp_reset(&mut flash, 0, sp, reset);

        let mut bl = make_bootloader(flash, 0x700);
        let part = PartitionInfo::new(PartitionId::A, 1024, 0); // start_address=0
        bl.register_partition(part).unwrap();

        let partition = *bl.get_partition(PartitionId::A).unwrap();
        let entry = bl.prepare_jump(&partition).unwrap();
        assert_eq!(entry.initial_sp, sp);
        assert_eq!(entry.reset_vector, reset);
    }

    #[test]
    fn test_prepare_jump_rejects_bad_sp() {
        let mut flash = MemoryFlash::<{ 2 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);
        // SP outside RAM [0x2000_0000, 0x2001_0000]
        let sp: u32 = 0x1000_0000;
        let reset: u32 = 0x0000_0101;
        write_header_sp_reset(&mut flash, 0, sp, reset);

        let mut bl = make_bootloader(flash, 0x700);
        let part = PartitionInfo::new(PartitionId::A, 1024, 0);
        bl.register_partition(part).unwrap();
        let partition = *bl.get_partition(PartitionId::A).unwrap();
        let err = bl.prepare_jump(&partition).unwrap_err();
        assert_eq!(err, BootloaderError::InvalidStackPointer);
    }

    #[test]
    fn test_prepare_jump_rejects_non_thumb() {
        let mut flash = MemoryFlash::<{ 2 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);
        let sp: u32 = 0x2000_8000;
        // LSB = 0 → not Thumb mode
        let reset: u32 = 0x0000_0100;
        write_header_sp_reset(&mut flash, 0, sp, reset);

        let mut bl = make_bootloader(flash, 0x700);
        let part = PartitionInfo::new(PartitionId::A, 1024, 0);
        bl.register_partition(part).unwrap();
        let partition = *bl.get_partition(PartitionId::A).unwrap();
        let err = bl.prepare_jump(&partition).unwrap_err();
        assert_eq!(err, BootloaderError::InvalidResetVector);
    }

    #[test]
    fn test_prepare_jump_image_too_short() {
        let flash = MemoryFlash::<{ 2 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);
        let mut bl = make_bootloader(flash, 0x700);
        // Register a partition smaller than VECTOR_TABLE_MIN_SIZE
        let part = PartitionInfo::new(PartitionId::A, 4, 0);
        bl.register_partition(part).unwrap();
        let partition = *bl.get_partition(PartitionId::A).unwrap();
        let err = bl.prepare_jump(&partition).unwrap_err();
        assert_eq!(err, BootloaderError::ImageTooShort);
    }

    // -----------------------------------------------------------------------
    // Image validation (11-16)
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_bad_magic() {
        const FLASH_SIZE: usize = 8 * 1024;
        let mut flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);

        let hdr = ImageHeader {
            magic: 0xDEAD_BEEF, // bad
            version: 1,
            image_size: 64,
            initial_sp: 0x2000_8000,
            reset_vector: 0x0000_0101,
            hash: [0u8; 32],
        };
        flash.load_bytes(0, &encode_header(&hdr));

        let mut bl = make_bootloader(flash, 0x1000);
        let part = PartitionInfo::new(PartitionId::A, 4096, 0);
        bl.register_partition(part).unwrap();

        let mut scratch = [0u8; 512];
        let err = bl.validate_image(PartitionId::A, &mut scratch).unwrap_err();
        assert_eq!(err, BootloaderError::InvalidMagic);
    }

    #[test]
    fn test_validate_image_size_zero() {
        const FLASH_SIZE: usize = 8 * 1024;
        let mut flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);

        let hdr = ImageHeader {
            magic: IMAGE_MAGIC,
            version: 1,
            image_size: 0, // bad
            initial_sp: 0x2000_8000,
            reset_vector: 0x0000_0101,
            hash: [0u8; 32],
        };
        flash.load_bytes(0, &encode_header(&hdr));

        let mut bl = make_bootloader(flash, 0x1000);
        let part = PartitionInfo::new(PartitionId::A, 4096, 0);
        bl.register_partition(part).unwrap();

        let mut scratch = [0u8; 512];
        let err = bl.validate_image(PartitionId::A, &mut scratch).unwrap_err();
        assert_eq!(err, BootloaderError::InvalidImageSize);
    }

    #[test]
    fn test_validate_integrity_pass() {
        const FLASH_SIZE: usize = 8 * 1024;
        let mut flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);

        let payload_len = 128usize;
        let img = build_valid_image(1, payload_len, 0x2000_8000, 0x0000_0101);
        flash.load_bytes(0, &img);

        let mut bl = make_bootloader(flash, 0x1000);
        let part = PartitionInfo::new(PartitionId::A, 4096, 0);
        bl.register_partition(part).unwrap();

        let mut scratch = [0u8; 512];
        let result = bl.validate_image(PartitionId::A, &mut scratch);
        assert!(result.is_ok(), "expected ok, got {:?}", result.err());
        assert_eq!(result.unwrap().version, 1);
    }

    #[test]
    fn test_validate_integrity_fail() {
        // The simulated hash only depends on payload length, not content.
        // To make the integrity check fail we must corrupt the hash stored
        // in the header so that verify_hash sees expected != computed.
        const FLASH_SIZE: usize = 8 * 1024;
        let mut flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);

        let payload_len = 128usize;
        let mut img = build_valid_image(1, payload_len, 0x2000_8000, 0x0000_0101);

        // Corrupt the first byte of the hash field (header bytes [32..64])
        img[32] ^= 0xFF;
        flash.load_bytes(0, &img);

        let mut bl = make_bootloader(flash, 0x1000);
        let part = PartitionInfo::new(PartitionId::A, 4096, 0);
        bl.register_partition(part).unwrap();

        let mut scratch = [0u8; 512];
        let err = bl.validate_image(PartitionId::A, &mut scratch).unwrap_err();
        assert_eq!(err, BootloaderError::IntegrityCheckFailed);
    }

    #[test]
    fn test_validate_rollback_violation() {
        const FLASH_SIZE: usize = 8 * 1024;
        let mut flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);

        let payload_len = 64usize;
        // Build image with version = 1
        let img = build_valid_image(1, payload_len, 0x2000_8000, 0x0000_0101);
        flash.load_bytes(0, &img);

        let mut bl = make_bootloader(flash, 0x1000);
        // Set rollback counter to 5 — version 1 should be rejected
        bl.state.rollback_counter = 5;

        let part = PartitionInfo::new(PartitionId::A, 4096, 0);
        bl.register_partition(part).unwrap();

        let mut scratch = [0u8; 512];
        let err = bl.validate_image(PartitionId::A, &mut scratch).unwrap_err();
        assert_eq!(err, BootloaderError::RollbackViolation);
    }

    #[test]
    fn test_validate_correct_image_full() {
        const FLASH_SIZE: usize = 8 * 1024;
        let mut flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);

        let payload_len = 200usize;
        let sp: u32 = 0x2000_8000;
        let reset: u32 = 0x0000_0101;
        let img = build_valid_image(42, payload_len, sp, reset);
        flash.load_bytes(0, &img);

        let mut bl = make_bootloader(flash, 0x1000);
        let part = PartitionInfo::new(PartitionId::A, 4096, 0);
        bl.register_partition(part).unwrap();

        let mut scratch = [0u8; 512];
        let hdr = bl.validate_image(PartitionId::A, &mut scratch).unwrap();
        assert_eq!(hdr.magic, IMAGE_MAGIC);
        assert_eq!(hdr.version, 42);
        assert_eq!(hdr.image_size, payload_len as u32);
        assert_eq!(hdr.initial_sp, sp);
        assert_eq!(hdr.reset_vector, reset);
    }

    // -----------------------------------------------------------------------
    // BootState (17-20)
    // -----------------------------------------------------------------------

    #[test]
    fn test_boot_state_serialize_deserialize() {
        let mut state = BootState::new(PartitionId::A, DEFAULT_MAX_BOOT_ATTEMPTS);
        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.confirmed = true;
        slot_a.version = 7;
        let mut slot_b = BootSlotState::new(PartitionId::B);
        slot_b.trial = true;
        slot_b.boot_attempts = 1;
        slot_b.version = 8;
        state.slots[0] = slot_a;
        state.slots[1] = slot_b;
        state.slot_count = 2;
        state.rollback_counter = 5;

        let bytes = state.serialize();
        let recovered = BootState::deserialize(&bytes).unwrap();

        assert_eq!(recovered.selected, PartitionId::A);
        assert_eq!(recovered.rollback_counter, 5);
        assert_eq!(recovered.max_attempts, DEFAULT_MAX_BOOT_ATTEMPTS);
        assert_eq!(recovered.slot_count, 2);

        let ra = recovered.slot(PartitionId::A).unwrap();
        assert!(ra.confirmed);
        assert_eq!(ra.version, 7);

        let rb = recovered.slot(PartitionId::B).unwrap();
        assert!(rb.trial);
        assert_eq!(rb.boot_attempts, 1);
        assert_eq!(rb.version, 8);
    }

    #[test]
    fn test_boot_state_corrupt_magic() {
        let state = BootState::new(PartitionId::A, DEFAULT_MAX_BOOT_ATTEMPTS);
        let mut bytes = state.serialize();
        // Overwrite magic
        bytes[0] = 0xDE;
        bytes[1] = 0xAD;
        bytes[2] = 0xBE;
        bytes[3] = 0xEF;
        let err = BootState::deserialize(&bytes).unwrap_err();
        assert_eq!(err, BootloaderError::CorruptBootState);
    }

    #[test]
    fn test_boot_state_corrupt_checksum() {
        let state = BootState::new(PartitionId::A, DEFAULT_MAX_BOOT_ATTEMPTS);
        let mut bytes = state.serialize();
        // Flip a bit in the checksum field
        let cs_offset = BOOT_STATE_SIZE - 8;
        bytes[cs_offset] ^= 0x01;
        let err = BootState::deserialize(&bytes).unwrap_err();
        assert_eq!(err, BootloaderError::CorruptBootState);
    }

    #[test]
    fn test_boot_state_default_on_corruption() {
        const FLASH_SIZE: usize = 4 * 1024;
        let flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);
        // Flash is all-FF — no valid magic; load_state should default gracefully
        let mut bl = make_bootloader(flash, 0);
        let result = bl.load_state();
        assert!(result.is_ok());
        // Default state: selected = A, rollback_counter = 0
        assert_eq!(bl.state.rollback_counter, 0);
    }

    // -----------------------------------------------------------------------
    // Partition selection (21-26)
    // -----------------------------------------------------------------------

    /// Create a bootloader with slot states already registered for A and B.
    fn make_bl_with_ab_slots<const N: usize>(
        flash: MemoryFlash<N>,
        slot_a: BootSlotState,
        slot_b: BootSlotState,
    ) -> Bootloader<MemoryFlash<N>> {
        let mut bl = make_bootloader(flash, N - FAKE_FLASH_PAGE_SIZE);
        let pa = PartitionInfo::new(PartitionId::A, 1024, 0);
        let pb = PartitionInfo::new(PartitionId::B, 1024, 1024);
        bl.register_partition(pa).ok();
        bl.register_partition(pb).ok();
        bl.state.slots[0] = slot_a;
        bl.state.slots[1] = slot_b;
        bl.state.slot_count = 2;
        bl
    }

    #[test]
    fn test_select_prefers_highest_version_confirmed() {
        let flash = MemoryFlash::<{ 8 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);

        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.confirmed = true;
        slot_a.version = 5;

        let mut slot_b = BootSlotState::new(PartitionId::B);
        slot_b.confirmed = true;
        slot_b.version = 10; // higher → prefer B

        let bl = make_bl_with_ab_slots(flash, slot_a, slot_b);
        assert_eq!(bl.select_boot_partition().unwrap(), PartitionId::B);
    }

    #[test]
    fn test_select_ab_rollback() {
        let flash = MemoryFlash::<{ 8 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);

        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.bad = true;

        let mut slot_b = BootSlotState::new(PartitionId::B);
        slot_b.confirmed = true;
        slot_b.version = 3;

        let bl = make_bl_with_ab_slots(flash, slot_a, slot_b);
        assert_eq!(bl.select_boot_partition().unwrap(), PartitionId::B);
    }

    #[test]
    fn test_select_trial_slot() {
        let flash = MemoryFlash::<{ 8 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);

        // A: trial, not yet exhausted
        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.trial = true;
        slot_a.boot_attempts = 1;
        slot_a.version = 2;

        // B: bad
        let mut slot_b = BootSlotState::new(PartitionId::B);
        slot_b.bad = true;

        let bl = make_bl_with_ab_slots(flash, slot_a, slot_b);
        assert_eq!(bl.select_boot_partition().unwrap(), PartitionId::A);
    }

    #[test]
    fn test_select_recovery_fallback() {
        let flash = MemoryFlash::<{ 8 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);

        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.bad = true;
        let mut slot_b = BootSlotState::new(PartitionId::B);
        slot_b.bad = true;

        let mut bl = make_bl_with_ab_slots(flash, slot_a, slot_b);

        // Register Recovery partition
        let pr = PartitionInfo::new(PartitionId::Recovery, 512, 2048);
        bl.register_partition(pr).ok();

        // Add recovery slot state
        bl.state.slots[2] = BootSlotState::new(PartitionId::Recovery);
        bl.state.slot_count = 3;

        assert_eq!(bl.select_boot_partition().unwrap(), PartitionId::Recovery);
    }

    #[test]
    fn test_select_factory_last_resort() {
        let flash = MemoryFlash::<{ 8 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);

        // No A/B registered, only factory
        let mut bl = make_bootloader(flash, 7 * 1024);
        let pf = PartitionInfo::new(PartitionId::Factory, 512, 0);
        bl.register_partition(pf).ok();

        let result = bl.select_boot_partition().unwrap();
        assert_eq!(result, PartitionId::Factory);
    }

    #[test]
    fn test_select_no_bootable() {
        let flash = MemoryFlash::<{ 8 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);

        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.bad = true;
        let mut slot_b = BootSlotState::new(PartitionId::B);
        slot_b.bad = true;

        let bl = make_bl_with_ab_slots(flash, slot_a, slot_b);
        let err = bl.select_boot_partition().unwrap_err();
        assert_eq!(err, BootloaderError::NoBootablePartition);
    }

    // -----------------------------------------------------------------------
    // Trial / confirm (27-30)
    // -----------------------------------------------------------------------

    #[test]
    fn test_begin_boot_then_confirm() {
        let flash = MemoryFlash::<{ 8 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);

        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.trial = true;
        slot_a.version = 3;

        let slot_b = BootSlotState::new(PartitionId::B);

        let mut bl = make_bl_with_ab_slots(flash, slot_a, slot_b);
        bl.state.selected = PartitionId::A;

        bl.begin_boot(PartitionId::A).unwrap();
        assert_eq!(bl.state.slot(PartitionId::A).unwrap().boot_attempts, 1);

        bl.confirm().unwrap();
        let s = bl.state.slot(PartitionId::A).unwrap();
        assert!(s.is_known_good());
        assert_eq!(s.boot_attempts, 0);
    }

    #[test]
    fn test_boot_attempts_exhausted() {
        let flash = MemoryFlash::<{ 8 * 1024 }>::new(FAKE_FLASH_PAGE_SIZE);

        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.trial = true;
        slot_a.boot_attempts = DEFAULT_MAX_BOOT_ATTEMPTS; // already at limit

        let slot_b = BootSlotState::new(PartitionId::B);
        let mut bl = make_bl_with_ab_slots(flash, slot_a, slot_b);

        let err = bl.begin_boot(PartitionId::A).unwrap_err();
        assert_eq!(err, BootloaderError::BootAttemptsExhausted);
        assert!(bl.state.slot(PartitionId::A).unwrap().bad);
    }

    #[test]
    fn test_run_boot_cycle_selects_good() {
        // build_valid_image stores initial_sp at header[12..16] and
        // reset_vector at header[16..20], which is exactly what
        // prepare_jump now reads.  No extra overwrite needed.
        const FLASH_SIZE: usize = 16 * 1024;
        const PART_OFFSET: usize = 4096;
        const STATE_OFFSET: usize = 12288;

        let mut flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);

        let sp: u32 = 0x2000_8000;
        let reset: u32 = 0x0000_0101; // Thumb, within flash [0, 16384)
        let payload_len = 100usize;
        let img = build_valid_image(5, payload_len, sp, reset);

        // Write the self-contained image (header already has sp/reset)
        flash.load_bytes(PART_OFFSET, &img);

        let mut bl = Bootloader::new(
            flash,
            0,
            STATE_OFFSET,
            0x2000_0000,
            0x2001_0000,
            make_verifier(),
            make_crypto(),
        );

        let pa = PartitionInfo::new(PartitionId::A, 4096, PART_OFFSET);
        bl.register_partition(pa).unwrap();

        // Slot A: confirmed, version=5
        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.confirmed = true;
        slot_a.version = 5;
        // Slot B: bad — excluded from selection
        let mut slot_b = BootSlotState::new(PartitionId::B);
        slot_b.bad = true;

        bl.state.slots[0] = slot_a;
        bl.state.slots[1] = slot_b;
        bl.state.slot_count = 2;

        let mut scratch = [0u8; 512];
        let entry = bl.run_boot_cycle(&mut scratch).unwrap();
        assert_eq!(entry.initial_sp, sp);
        assert_eq!(entry.reset_vector, reset);
    }

    #[test]
    fn test_run_boot_cycle_rolls_back_on_bad_b() {
        const FLASH_SIZE: usize = 16 * 1024;
        const PART_A_OFFSET: usize = 0;
        const PART_B_OFFSET: usize = 4096;
        const STATE_OFFSET: usize = 12288;

        let mut flash = MemoryFlash::<FLASH_SIZE>::new(FAKE_FLASH_PAGE_SIZE);

        let sp: u32 = 0x2000_8000;
        let reset: u32 = 0x0000_0101;
        let payload_len = 100usize;

        // Partition A: valid image with version=5
        let img_a = build_valid_image(5, payload_len, sp, reset);
        flash.load_bytes(PART_A_OFFSET, &img_a);

        // Partition B: corrupt (bad magic)
        let hdr_b = ImageHeader {
            magic: 0xBAD1_BAD1,
            version: 10,
            image_size: 64,
            initial_sp: sp,
            reset_vector: reset,
            hash: [0u8; 32],
        };
        flash.load_bytes(PART_B_OFFSET, &encode_header(&hdr_b));

        let mut bl = Bootloader::new(
            flash,
            0,
            STATE_OFFSET,
            0x2000_0000,
            0x2001_0000,
            make_verifier(),
            make_crypto(),
        );

        let pa = PartitionInfo::new(PartitionId::A, 4096, PART_A_OFFSET);
        let pb = PartitionInfo::new(PartitionId::B, 4096, PART_B_OFFSET);
        bl.register_partition(pa).unwrap();
        bl.register_partition(pb).unwrap();

        // A: not yet confirmed (so both A and B are candidates);
        // B: trial, attempts=0, higher version → selected first
        let mut slot_a = BootSlotState::new(PartitionId::A);
        slot_a.version = 5;
        // leave confirmed=false so trial B is preferred via phase 2 or
        // phase 3 where B has higher version — but B has bad magic so
        // it gets marked bad and then A is chosen via phase 3
        let mut slot_b = BootSlotState::new(PartitionId::B);
        slot_b.trial = true;
        slot_b.boot_attempts = 0;
        slot_b.version = 10;

        bl.state.slots[0] = slot_a;
        bl.state.slots[1] = slot_b;
        bl.state.slot_count = 2;

        let mut scratch = [0u8; 512];
        let entry = bl.run_boot_cycle(&mut scratch).unwrap();
        // After B fails validation and is marked bad, A is selected
        assert_eq!(entry.initial_sp, sp);
    }

    // -----------------------------------------------------------------------
    // Error display (31)
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_display_nonempty() {
        let variants = [
            BootloaderError::NoBootablePartition,
            BootloaderError::InvalidMagic,
            BootloaderError::InvalidImageSize,
            BootloaderError::IntegrityCheckFailed,
            BootloaderError::RollbackViolation,
            BootloaderError::InvalidStackPointer,
            BootloaderError::InvalidResetVector,
            BootloaderError::ImageTooShort,
            BootloaderError::CorruptBootState,
            BootloaderError::FlashReadFailed,
            BootloaderError::FlashWriteFailed,
            BootloaderError::FlashEraseFailed,
            BootloaderError::OutOfBounds,
            BootloaderError::WriteNotErased,
            BootloaderError::BootAttemptsExhausted,
            BootloaderError::InvalidState,
            BootloaderError::PartitionNotFound,
            BootloaderError::Ota(OtaError::NoUpdateAvailable),
            BootloaderError::Security(SecurityError::InvalidHash),
        ];
        for v in &variants {
            let s = format!("{}", v);
            assert!(!s.is_empty(), "Display for {:?} is empty", v);
        }
    }
}
