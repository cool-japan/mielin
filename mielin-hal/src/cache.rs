//! CPU Cache Information Detection
//!
//! Provides cache topology detection for performance optimization.
//!
//! ## Features
//!
//! - L1/L2/L3 cache size detection
//! - Cache line size detection
//! - Cache associativity information
//! - Multi-architecture support (x86_64, AArch64, RISC-V, ARM)

/// Cache level identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLevel {
    /// L1 Data cache
    L1Data,
    /// L1 Instruction cache
    L1Instruction,
    /// L1 Unified cache (data + instruction)
    L1Unified,
    /// L2 cache (typically unified)
    L2,
    /// L3 cache (shared across cores)
    L3,
}

impl CacheLevel {
    /// Get the level number (1, 2, or 3)
    pub fn level(&self) -> u8 {
        match self {
            CacheLevel::L1Data | CacheLevel::L1Instruction | CacheLevel::L1Unified => 1,
            CacheLevel::L2 => 2,
            CacheLevel::L3 => 3,
        }
    }

    /// Check if this is a data cache
    pub fn is_data(&self) -> bool {
        matches!(
            self,
            CacheLevel::L1Data | CacheLevel::L1Unified | CacheLevel::L2 | CacheLevel::L3
        )
    }

    /// Check if this is an instruction cache
    pub fn is_instruction(&self) -> bool {
        matches!(self, CacheLevel::L1Instruction | CacheLevel::L1Unified)
    }
}

/// Cache type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheType {
    /// Data cache only
    Data,
    /// Instruction cache only
    Instruction,
    /// Unified (data + instruction)
    Unified,
}

/// Information about a single cache level
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheInfo {
    /// Cache level
    pub level: u8,
    /// Cache type (data, instruction, unified)
    pub cache_type: u8, // 0=unknown, 1=data, 2=instruction, 3=unified
    /// Total cache size in bytes
    pub size: usize,
    /// Cache line size in bytes
    pub line_size: usize,
    /// Number of ways of associativity (0 = fully associative)
    pub associativity: u8,
    /// Number of sets
    pub sets: usize,
    /// Whether this cache is shared across cores
    pub shared: bool,
    /// Number of logical processors sharing this cache
    pub shared_by: u8,
}

impl CacheInfo {
    /// Create an empty/unknown cache info
    pub const fn unknown() -> Self {
        Self {
            level: 0,
            cache_type: 0,
            size: 0,
            line_size: 0,
            associativity: 0,
            sets: 0,
            shared: false,
            shared_by: 1,
        }
    }

    /// Check if this cache info is valid
    pub fn is_valid(&self) -> bool {
        self.level > 0 && self.size > 0 && self.line_size > 0
    }

    /// Get cache type as enum
    pub fn get_type(&self) -> CacheType {
        match self.cache_type {
            1 => CacheType::Data,
            2 => CacheType::Instruction,
            3 => CacheType::Unified,
            _ => CacheType::Unified,
        }
    }

    /// Calculate effective bandwidth hint (higher is better)
    /// Based on cache size and line size
    pub fn bandwidth_hint(&self) -> usize {
        if self.line_size == 0 {
            return 0;
        }
        self.size / self.line_size
    }
}

/// Complete cache topology information
#[derive(Debug, Clone, Default)]
pub struct CacheTopology {
    /// L1 Data cache info
    pub l1_data: CacheInfo,
    /// L1 Instruction cache info
    pub l1_instruction: CacheInfo,
    /// L2 cache info
    pub l2: CacheInfo,
    /// L3 cache info (may be unavailable on some systems)
    pub l3: CacheInfo,
    /// Default cache line size to use for optimizations
    pub default_line_size: usize,
}

impl CacheTopology {
    /// Detect cache topology for the current system
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            detect_x86_64()
        }

        #[cfg(target_arch = "aarch64")]
        {
            detect_aarch64()
        }

        #[cfg(target_arch = "riscv64")]
        {
            detect_riscv64()
        }

        #[cfg(target_arch = "arm")]
        {
            detect_arm()
        }

        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64",
            target_arch = "arm"
        )))]
        {
            Self::default_topology()
        }
    }

    /// Create a default topology with reasonable assumptions
    pub fn default_topology() -> Self {
        Self {
            l1_data: CacheInfo {
                level: 1,
                cache_type: 1,   // data
                size: 32 * 1024, // 32 KB typical
                line_size: 64,
                associativity: 8,
                sets: 64,
                shared: false,
                shared_by: 1,
            },
            l1_instruction: CacheInfo {
                level: 1,
                cache_type: 2,   // instruction
                size: 32 * 1024, // 32 KB typical
                line_size: 64,
                associativity: 8,
                sets: 64,
                shared: false,
                shared_by: 1,
            },
            l2: CacheInfo {
                level: 2,
                cache_type: 3,    // unified
                size: 256 * 1024, // 256 KB typical
                line_size: 64,
                associativity: 8,
                sets: 512,
                shared: false,
                shared_by: 1,
            },
            l3: CacheInfo {
                level: 3,
                cache_type: 3,         // unified
                size: 8 * 1024 * 1024, // 8 MB typical
                line_size: 64,
                associativity: 16,
                sets: 8192,
                shared: true,
                shared_by: 8,
            },
            default_line_size: 64,
        }
    }

    /// Get the cache info for a specific level
    pub fn get(&self, level: CacheLevel) -> &CacheInfo {
        match level {
            CacheLevel::L1Data | CacheLevel::L1Unified => &self.l1_data,
            CacheLevel::L1Instruction => &self.l1_instruction,
            CacheLevel::L2 => &self.l2,
            CacheLevel::L3 => &self.l3,
        }
    }

    /// Get total L1 cache size
    pub fn l1_total_size(&self) -> usize {
        self.l1_data.size + self.l1_instruction.size
    }

    /// Get the optimal blocking factor for matrix operations
    /// Returns a size in elements that fits well in L1 cache
    pub fn optimal_block_size<T>(&self) -> usize {
        let element_size = core::mem::size_of::<T>();
        if element_size == 0 {
            return 64;
        }

        // Target using ~75% of L1 data cache for blocking
        let target_bytes = (self.l1_data.size * 3) / 4;
        let elements = target_bytes / element_size;

        // For square blocks, take sqrt
        let block = isqrt(elements);

        // Round down to cache line multiple
        let elements_per_line = self.default_line_size / element_size;
        if elements_per_line > 0 {
            (block / elements_per_line) * elements_per_line
        } else {
            block
        }
    }

    /// Check if the system has L3 cache
    pub fn has_l3(&self) -> bool {
        self.l3.is_valid()
    }

    /// Get optimal prefetch distance in cache lines
    pub fn prefetch_distance(&self) -> usize {
        // Heuristic: prefetch 4-8 cache lines ahead
        if self.default_line_size > 0 {
            4
        } else {
            0
        }
    }
}

/// Integer square root (no std dependency)
fn isqrt(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

// Architecture-specific detection functions

#[cfg(target_arch = "x86_64")]
fn detect_x86_64() -> CacheTopology {
    // Use CPUID instruction to get cache info
    // This is a simplified version - full implementation would use
    // CPUID leaves 0x04 and 0x18

    let mut topology = CacheTopology::default_topology();

    // Try to detect actual cache sizes using CPUID
    #[cfg(target_feature = "sse")]
    unsafe {
        use core::arch::x86_64::{__cpuid, __cpuid_count};

        // Check if extended cache info is available
        let max_basic = __cpuid(0).eax;

        if max_basic >= 4 {
            // Iterate through cache levels using CPUID leaf 4
            for idx in 0..8 {
                let result = __cpuid_count(4, idx);

                let cache_type = result.eax & 0x1F;
                if cache_type == 0 {
                    break; // No more caches
                }

                let level = ((result.eax >> 5) & 0x7) as u8;
                let line_size = ((result.ebx & 0xFFF) + 1) as usize;
                let partitions = (((result.ebx >> 12) & 0x3FF) + 1) as usize;
                let ways = (((result.ebx >> 22) & 0x3FF) + 1) as usize;
                let sets = (result.ecx + 1) as usize;
                let size = line_size * partitions * ways * sets;
                let shared_by = (((result.eax >> 14) & 0xFFF) + 1) as u8;

                let info = CacheInfo {
                    level,
                    cache_type: cache_type as u8,
                    size,
                    line_size,
                    associativity: ways as u8,
                    sets,
                    shared: shared_by > 1,
                    shared_by,
                };

                match (level, cache_type) {
                    (1, 1) => topology.l1_data = info,        // L1 Data
                    (1, 2) => topology.l1_instruction = info, // L1 Instruction
                    (1, 3) => {
                        // L1 Unified
                        topology.l1_data = info;
                        topology.l1_instruction = info;
                    }
                    (2, _) => topology.l2 = info,
                    (3, _) => topology.l3 = info,
                    _ => {}
                }
            }

            // Update default line size from L1
            if topology.l1_data.line_size > 0 {
                topology.default_line_size = topology.l1_data.line_size;
            }
        }
    }

    topology
}

#[cfg(target_arch = "aarch64")]
fn detect_aarch64() -> CacheTopology {
    // On AArch64, cache info can be read from system registers
    // CTR_EL0 for cache line size, CCSIDR_EL1 for detailed info
    // Note: CCSIDR_EL1 may require elevated privileges

    let mut topology = CacheTopology::default_topology();

    // Read CTR_EL0 (Cache Type Register) - accessible from EL0
    #[cfg(target_os = "linux")]
    unsafe {
        let ctr: u64;
        core::arch::asm!("mrs {}, ctr_el0", out(reg) ctr);

        // Extract cache line sizes
        let iminline = ((ctr >> 0) & 0xF) as usize; // log2(words)
        let dminline = ((ctr >> 16) & 0xF) as usize; // log2(words)

        let i_line_size = 4 << iminline; // words to bytes (word = 4 bytes)
        let d_line_size = 4 << dminline;

        topology.l1_instruction.line_size = i_line_size;
        topology.l1_data.line_size = d_line_size;
        topology.default_line_size = d_line_size;
    }

    // For non-Linux or when syscall-based detection isn't available,
    // use reasonable defaults for modern ARM cores
    #[cfg(not(target_os = "linux"))]
    {
        // Modern ARM cores (Cortex-A76, Apple M1, etc.) typically have:
        // L1D: 64KB, L1I: 64KB, L2: 256KB-512KB per core, L3: shared
        topology.l1_data.size = 64 * 1024;
        topology.l1_instruction.size = 64 * 1024;
        topology.l2.size = 256 * 1024;
        topology.l3.size = 4 * 1024 * 1024;
    }

    topology
}

#[cfg(target_arch = "riscv64")]
fn detect_riscv64() -> CacheTopology {
    // RISC-V cache info is typically obtained from device tree
    // or machine-specific registers
    // For now, return conservative defaults
    let mut topology = CacheTopology::default_topology();

    // Common RISC-V implementations have 64-byte cache lines
    topology.default_line_size = 64;
    topology.l1_data.line_size = 64;
    topology.l1_instruction.line_size = 64;
    topology.l2.line_size = 64;

    // SiFive U74 typical values
    topology.l1_data.size = 32 * 1024;
    topology.l1_instruction.size = 32 * 1024;
    topology.l2.size = 2 * 1024 * 1024;

    topology
}

#[cfg(target_arch = "arm")]
fn detect_arm() -> CacheTopology {
    // ARM Cortex-M series typically have smaller caches
    let mut topology = CacheTopology::default_topology();

    // Cortex-M7 typical values
    topology.l1_data.size = 32 * 1024;
    topology.l1_data.line_size = 32;
    topology.l1_instruction.size = 32 * 1024;
    topology.l1_instruction.line_size = 32;
    topology.default_line_size = 32;

    // Most Cortex-M don't have L2/L3
    topology.l2 = CacheInfo::unknown();
    topology.l3 = CacheInfo::unknown();

    topology
}

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

/// Global cache topology storage
struct CacheTopologyStorage {
    init_state: AtomicU8, // 0=uninit, 1=initializing, 2=ready
    data: UnsafeCell<core::mem::MaybeUninit<CacheTopology>>,
}

// Safety: We protect access with atomic state
unsafe impl Sync for CacheTopologyStorage {}

static CACHE_TOPOLOGY: CacheTopologyStorage = CacheTopologyStorage {
    init_state: AtomicU8::new(0),
    data: UnsafeCell::new(core::mem::MaybeUninit::uninit()),
};

/// Get the global cache topology
/// Performs detection on first call
pub fn topology() -> &'static CacheTopology {
    const UNINIT: u8 = 0;
    const INITIALIZING: u8 = 1;
    const READY: u8 = 2;

    loop {
        match CACHE_TOPOLOGY.init_state.load(Ordering::Acquire) {
            READY => {
                // Safety: State is READY, so data is initialized
                return unsafe { (*CACHE_TOPOLOGY.data.get()).assume_init_ref() };
            }
            UNINIT => {
                // Try to become the initializer
                if CACHE_TOPOLOGY
                    .init_state
                    .compare_exchange(UNINIT, INITIALIZING, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    // We're the initializer
                    unsafe {
                        (*CACHE_TOPOLOGY.data.get()).write(CacheTopology::detect());
                    }
                    CACHE_TOPOLOGY.init_state.store(READY, Ordering::Release);
                    return unsafe { (*CACHE_TOPOLOGY.data.get()).assume_init_ref() };
                }
                // CAS failed, another thread is initializing - retry
            }
            INITIALIZING => {
                // Wait for initialization to complete
                core::hint::spin_loop();
            }
            _ => unreachable!(),
        }
    }
}

/// Get the default cache line size for the current architecture
pub fn line_size() -> usize {
    topology().default_line_size
}

/// Get L1 data cache size
pub fn l1_data_size() -> usize {
    topology().l1_data.size
}

/// Get L2 cache size
pub fn l2_size() -> usize {
    topology().l2.size
}

/// Get L3 cache size (0 if not available)
pub fn l3_size() -> usize {
    if topology().l3.is_valid() {
        topology().l3.size
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_level() {
        assert_eq!(CacheLevel::L1Data.level(), 1);
        assert_eq!(CacheLevel::L2.level(), 2);
        assert_eq!(CacheLevel::L3.level(), 3);

        assert!(CacheLevel::L1Data.is_data());
        assert!(!CacheLevel::L1Instruction.is_data());
        assert!(CacheLevel::L1Unified.is_data());
    }

    #[test]
    fn test_cache_info_unknown() {
        let info = CacheInfo::unknown();
        assert!(!info.is_valid());
        assert_eq!(info.bandwidth_hint(), 0);
    }

    #[test]
    fn test_cache_info_valid() {
        let info = CacheInfo {
            level: 1,
            cache_type: 1,
            size: 32 * 1024,
            line_size: 64,
            associativity: 8,
            sets: 64,
            shared: false,
            shared_by: 1,
        };

        assert!(info.is_valid());
        assert_eq!(info.get_type(), CacheType::Data);
        assert!(info.bandwidth_hint() > 0);
    }

    #[test]
    fn test_cache_topology_detect() {
        let topology = CacheTopology::detect();

        // Should have reasonable defaults
        assert!(topology.default_line_size >= 32);
        assert!(topology.l1_data.size > 0);
    }

    #[test]
    fn test_cache_topology_default() {
        let topology = CacheTopology::default_topology();

        assert!(topology.l1_data.is_valid());
        assert!(topology.l1_instruction.is_valid());
        assert!(topology.l2.is_valid());
        assert!(topology.l3.is_valid());
        assert_eq!(topology.default_line_size, 64);
    }

    #[test]
    fn test_l1_total_size() {
        let topology = CacheTopology::default_topology();
        assert_eq!(topology.l1_total_size(), 64 * 1024); // 32KB + 32KB
    }

    #[test]
    fn test_optimal_block_size() {
        let topology = CacheTopology::default_topology();

        // For f32 (4 bytes), block should be reasonable
        let block_f32 = topology.optimal_block_size::<f32>();
        assert!(block_f32 > 0);
        assert!(block_f32 <= 256); // Should fit in L1

        // For f64 (8 bytes), block should be smaller
        let block_f64 = topology.optimal_block_size::<f64>();
        assert!(block_f64 > 0);
        assert!(block_f64 <= block_f32);
    }

    #[test]
    fn test_has_l3() {
        let topology = CacheTopology::default_topology();
        assert!(topology.has_l3());

        let mut no_l3 = topology.clone();
        no_l3.l3 = CacheInfo::unknown();
        assert!(!no_l3.has_l3());
    }

    #[test]
    fn test_prefetch_distance() {
        let topology = CacheTopology::default_topology();
        let dist = topology.prefetch_distance();
        assert!(dist > 0);
    }

    #[test]
    fn test_global_functions() {
        // These use the global topology
        let line = line_size();
        assert!(line >= 32);

        let l1 = l1_data_size();
        assert!(l1 > 0);

        let l2 = l2_size();
        assert!(l2 > 0);
    }

    #[test]
    fn test_isqrt() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(16), 4);
        assert_eq!(isqrt(15), 3);
        assert_eq!(isqrt(100), 10);
    }

    #[test]
    fn test_get_cache_level() {
        let topology = CacheTopology::default_topology();

        let l1 = topology.get(CacheLevel::L1Data);
        assert_eq!(l1.level, 1);

        let l2 = topology.get(CacheLevel::L2);
        assert_eq!(l2.level, 2);

        let l3 = topology.get(CacheLevel::L3);
        assert_eq!(l3.level, 3);
    }
}
