//! RV64IMAC-specific system descriptors.
//!
//! Provides memory-map and system configuration for 64-bit RISC-V platforms
//! capable of running Linux or a bare-metal hypervisor:
//!
//! - **QEMU virt machine** — the canonical RISC-V testing platform
//! - **StarFive JH7100** — VisionFive 1 SoC (dual U74 @ 1.0 GHz)
//! - **StarFive JH7110** — VisionFive 2 SoC (quad U74 @ 1.5 GHz)
//! - **SiFive U74** — stand-alone application-class core
//!
//! ## Virtual Memory
//!
//! RV64 supports three page-table modes:
//! - **Sv39** — 3-level 39-bit virtual address space (512 GiB)
//! - **Sv48** — 4-level 48-bit virtual address space (256 TiB)
//! - **Sv57** — 5-level 57-bit virtual address space (128 PiB, rarely used on embedded)

use core::fmt;

// ---------------------------------------------------------------------------
// RV64IMAC Variant
// ---------------------------------------------------------------------------

/// SoC / board variant for rv64imac-based systems
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rv64ImacVariant {
    /// QEMU `virt` machine — used for bare-metal and OS testing
    QemuVirt,
    /// StarFive JH7100 (VisionFive 1) — dual U74, LPDDR4, Wi-Fi
    StarFiveJH7100,
    /// StarFive JH7110 (VisionFive 2) — quad U74-MC, LPDDR4, PCIe, GPU stub
    StarFiveJH7110,
    /// SiFive U74 application-class core (used in many SoCs)
    SifiveU74,
}

impl fmt::Display for Rv64ImacVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rv64ImacVariant::QemuVirt => write!(f, "QEMU RV64 virt"),
            Rv64ImacVariant::StarFiveJH7100 => write!(f, "StarFive JH7100 (VisionFive 1)"),
            Rv64ImacVariant::StarFiveJH7110 => write!(f, "StarFive JH7110 (VisionFive 2)"),
            Rv64ImacVariant::SifiveU74 => write!(f, "SiFive U74"),
        }
    }
}

// ---------------------------------------------------------------------------
// RV64IMAC System Descriptor
// ---------------------------------------------------------------------------

/// Complete system descriptor for a rv64imac platform.
///
/// Captures DRAM layout, hart count, and virtual-memory capability so that
/// an OS loader or bare-metal runtime can configure page tables, the CLINT,
/// and the PLIC without hard-coding per-board magic numbers.
pub struct Rv64ImacSystem {
    /// SoC variant
    pub variant: Rv64ImacVariant,
    /// Physical base address of the main RAM region
    pub ram_base: u64,
    /// Total usable RAM size in bytes
    pub ram_size: u64,
    /// Number of application-class harts (cores) present
    pub num_harts: usize,
    /// Whether the MMU supports Sv39 (3-level) paging
    pub supports_sv39: bool,
    /// Whether the MMU supports Sv48 (4-level) paging
    pub supports_sv48: bool,
    /// Whether the MMU supports Sv57 (5-level) paging (future platforms)
    pub supports_sv57: bool,
    /// CLINT base address for this platform
    pub clint_base: u64,
    /// PLIC base address for this platform
    pub plic_base: u64,
}

impl Rv64ImacSystem {
    /// Construct a system descriptor for the given variant using canonical defaults.
    pub fn new(variant: Rv64ImacVariant) -> Self {
        match variant {
            Rv64ImacVariant::QemuVirt => Self::qemu_virt(),
            Rv64ImacVariant::StarFiveJH7100 => Self::visionfive1(),
            Rv64ImacVariant::StarFiveJH7110 => Self::visionfive2(),
            Rv64ImacVariant::SifiveU74 => Self::sifive_u74(),
        }
    }

    /// QEMU `virt` machine descriptor.
    ///
    /// Memory layout from `qemu/hw/riscv/virt.c`:
    /// - DRAM starts at 0x8000_0000, default 256 MiB (expandable)
    /// - CLINT at 0x0200_0000
    /// - PLIC at 0x0C00_0000
    /// - 4 harts by default, Sv39 + Sv48 + Sv57 all supported
    pub fn qemu_virt() -> Self {
        Self {
            variant: Rv64ImacVariant::QemuVirt,
            ram_base: 0x8000_0000,
            ram_size: 256 * 1024 * 1024,
            num_harts: 4,
            supports_sv39: true,
            supports_sv48: true,
            supports_sv57: true,
            clint_base: 0x0200_0000,
            plic_base: 0x0C00_0000,
        }
    }

    /// StarFive JH7100 — VisionFive 1 board.
    ///
    /// - 8 GiB LPDDR4 DRAM at 0x8000_0000
    /// - Dual SiFive U74-MC cores (2 + 1 S7 management core)
    /// - Sv39 paging; U74 does not support Sv48
    /// - CLINT at 0x200_0000 (JH7100 specific)
    /// - PLIC at 0x0C00_0000
    pub fn visionfive1() -> Self {
        Self {
            variant: Rv64ImacVariant::StarFiveJH7100,
            ram_base: 0x8000_0000,
            ram_size: 8 * 1024 * 1024 * 1024,
            num_harts: 2,
            supports_sv39: true,
            supports_sv48: false, // U74 only supports Sv39
            supports_sv57: false,
            clint_base: 0x0200_0000,
            plic_base: 0x0C00_0000,
        }
    }

    /// StarFive JH7110 — VisionFive 2 board.
    ///
    /// - 8 GiB LPDDR4 DRAM at 0x4000_0000 (JH7110 different from JH7100!)
    /// - Quad SiFive U74-MC application cores + 1 S7 monitor core
    /// - Sv39 only (U74 core limitation)
    /// - PCIe, GPU stub, ISP present
    pub fn visionfive2() -> Self {
        Self {
            variant: Rv64ImacVariant::StarFiveJH7110,
            ram_base: 0x4000_0000,
            ram_size: 8 * 1024 * 1024 * 1024,
            num_harts: 4,
            supports_sv39: true,
            supports_sv48: false,
            supports_sv57: false,
            clint_base: 0x0200_0000,
            plic_base: 0x0C00_0000,
        }
    }

    /// SiFive U74 stand-alone core (generic configuration).
    ///
    /// The U74 is an application-class RV64GC core.  This descriptor uses
    /// conservative defaults suitable for the HiFive Unmatched board.
    pub fn sifive_u74() -> Self {
        Self {
            variant: Rv64ImacVariant::SifiveU74,
            ram_base: 0x8000_0000,
            ram_size: 16 * 1024 * 1024 * 1024,
            num_harts: 4,
            supports_sv39: true,
            supports_sv48: false, // U74 only supports Sv39
            supports_sv57: false,
            clint_base: 0x0200_0000,
            plic_base: 0x0C00_0000,
        }
    }

    /// Return the number of virtual-address levels for the highest supported
    /// page-table mode.
    ///
    /// | Mode  | Levels |
    /// |-------|--------|
    /// | Sv39  | 3      |
    /// | Sv48  | 4      |
    /// | Sv57  | 5      |
    ///
    /// Returns 0 if no virtual memory mode is supported (bare physical).
    pub fn page_table_levels(&self) -> u8 {
        if self.supports_sv57 {
            5
        } else if self.supports_sv48 {
            4
        } else if self.supports_sv39 {
            3
        } else {
            0
        }
    }

    /// Returns `true` if any virtual-memory mode is supported
    pub fn has_mmu(&self) -> bool {
        self.supports_sv39 || self.supports_sv48 || self.supports_sv57
    }

    /// Physical address of the last byte of RAM (inclusive end)
    pub fn ram_end(&self) -> u64 {
        self.ram_base + self.ram_size - 1
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qemu_virt_system() {
        let sys = Rv64ImacSystem::qemu_virt();
        assert_eq!(sys.variant, Rv64ImacVariant::QemuVirt);
        assert!(sys.num_harts >= 1, "QEMU virt must have at least one hart");
        assert_eq!(sys.ram_base, 0x8000_0000);
        assert!(sys.has_mmu());
        assert!(sys.supports_sv39);
        assert!(sys.supports_sv48);
    }

    #[test]
    fn test_visionfive2_system() {
        let sys = Rv64ImacSystem::visionfive2();
        assert_eq!(sys.variant, Rv64ImacVariant::StarFiveJH7110);
        assert_eq!(sys.num_harts, 4);
        // JH7110 DRAM starts at 0x4000_0000
        assert_eq!(sys.ram_base, 0x4000_0000);
        // U74 only supports Sv39
        assert!(sys.supports_sv39);
        assert!(!sys.supports_sv48);
    }

    #[test]
    fn test_sv39_page_table_levels() {
        // A system with only Sv39 should report 3 levels
        let mut sys = Rv64ImacSystem::qemu_virt();
        sys.supports_sv48 = false;
        sys.supports_sv57 = false;
        assert_eq!(sys.page_table_levels(), 3);
    }

    #[test]
    fn test_sv48_page_table_levels() {
        // A system with Sv48 (but not Sv57) should report 4 levels
        let mut sys = Rv64ImacSystem::qemu_virt();
        sys.supports_sv57 = false;
        sys.supports_sv48 = true;
        assert_eq!(sys.page_table_levels(), 4);
    }

    #[test]
    fn test_sv57_page_table_levels() {
        let sys = Rv64ImacSystem::qemu_virt();
        // QEMU virt supports Sv57
        assert_eq!(sys.page_table_levels(), 5);
    }

    #[test]
    fn test_no_mmu_returns_zero_levels() {
        let mut sys = Rv64ImacSystem::qemu_virt();
        sys.supports_sv39 = false;
        sys.supports_sv48 = false;
        sys.supports_sv57 = false;
        assert_eq!(sys.page_table_levels(), 0);
        assert!(!sys.has_mmu());
    }

    #[test]
    fn test_visionfive1_system() {
        let sys = Rv64ImacSystem::visionfive1();
        assert_eq!(sys.variant, Rv64ImacVariant::StarFiveJH7100);
        assert_eq!(sys.num_harts, 2);
        assert!(sys.supports_sv39);
        assert!(!sys.supports_sv48, "JH7100 U74 does not support Sv48");
    }

    #[test]
    fn test_new_dispatch_qemu_virt() {
        let sys = Rv64ImacSystem::new(Rv64ImacVariant::QemuVirt);
        assert_eq!(sys.variant, Rv64ImacVariant::QemuVirt);
    }

    #[test]
    fn test_new_dispatch_visionfive2() {
        let sys = Rv64ImacSystem::new(Rv64ImacVariant::StarFiveJH7110);
        assert_eq!(sys.variant, Rv64ImacVariant::StarFiveJH7110);
    }

    #[test]
    fn test_ram_end_calculation() {
        let sys = Rv64ImacSystem::qemu_virt();
        assert_eq!(sys.ram_end(), sys.ram_base + sys.ram_size - 1);
    }
}
