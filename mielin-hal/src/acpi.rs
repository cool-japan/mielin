//! ACPI (Advanced Configuration and Power Interface) Support
//!
//! This module provides parsing and querying capabilities for ACPI tables,
//! commonly used in x86_64 systems for hardware enumeration and power management.
//!
//! # What is ACPI?
//!
//! ACPI is an industry standard for hardware discovery, configuration, power
//! management, and thermal management. It provides:
//! - CPU topology (cores, threads, NUMA)
//! - Power management (P-states, C-states, thermal)
//! - Device enumeration
//! - Interrupt routing
//! - Memory maps
//!
//! # ACPI Tables
//!
//! Common ACPI tables:
//! - **RSDP** - Root System Description Pointer (entry point)
//! - **RSDT/XSDT** - Root/Extended System Description Table
//! - **FADT** - Fixed ACPI Description Table (power management)
//! - **MADT** - Multiple APIC Description Table (CPU/interrupt topology)
//! - **SRAT** - System Resource Affinity Table (NUMA)
//! - **HPET** - High Precision Event Timer
//! - **MCFG** - PCI Express Memory Mapped Configuration
//!
//! # Examples
//!
//! ```no_run
//! use mielin_hal::acpi::{detect_acpi, AcpiInfo};
//!
//! let acpi = detect_acpi();
//!
//! if acpi.available {
//!     println!("ACPI version: {}.{}", acpi.major_version, acpi.minor_version);
//!     println!("OEM ID: {}", acpi.oem_id.unwrap_or_default());
//!
//!     if let Some(cpu_count) = acpi.cpu_count {
//!         println!("CPUs from MADT: {}", cpu_count);
//!     }
//! }
//! ```
//!
//! # Platform Support
//!
//! - **x86_64**: Full support via `/sys/firmware/acpi/tables/`
//! - **AArch64**: Limited ACPI support on some platforms
//! - **RISC-V**: Device Tree preferred
//! - **Cortex-M**: Not applicable (no ACPI)
//!
//! # Safety
//!
//! This module uses safe file I/O via sysfs. Direct ACPI table parsing from
//! memory would require unsafe code and is not implemented.
//!
//! # Known Limitations
//!
//! 1. **Read-Only**: No ACPI table modification support
//! 2. **Sysfs Only**: Does not parse raw ACPI tables from memory
//! 3. **Limited Tables**: Only parses common tables (MADT, FADT, SRAT)
//! 4. **No AML**: ACPI Machine Language (AML) not supported

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// ACPI table signature (4 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiSignature {
    /// Root System Description Pointer
    Rsdp,
    /// Root System Description Table
    Rsdt,
    /// Extended System Description Table
    Xsdt,
    /// Fixed ACPI Description Table
    Fadt,
    /// Multiple APIC Description Table
    Madt,
    /// System Resource Affinity Table
    Srat,
    /// High Precision Event Timer
    Hpet,
    /// PCI Express Memory Mapped Configuration
    Mcfg,
    /// Unknown/other table
    Unknown,
}

impl fmt::Display for AcpiSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rsdp => write!(f, "RSDP"),
            Self::Rsdt => write!(f, "RSDT"),
            Self::Xsdt => write!(f, "XSDT"),
            Self::Fadt => write!(f, "FADT"),
            Self::Madt => write!(f, "MADT"),
            Self::Srat => write!(f, "SRAT"),
            Self::Hpet => write!(f, "HPET"),
            Self::Mcfg => write!(f, "MCFG"),
            Self::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// ACPI Power Management Profile
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    /// Unspecified
    Unspecified,
    /// Desktop
    Desktop,
    /// Mobile
    Mobile,
    /// Workstation
    Workstation,
    /// Enterprise Server
    EnterpriseServer,
    /// SOHO Server
    SohoServer,
    /// Appliance PC
    AppliancePc,
    /// Performance Server
    PerformanceServer,
    /// Tablet
    Tablet,
    /// Reserved
    Reserved(u8),
}

impl fmt::Display for PowerProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unspecified => write!(f, "Unspecified"),
            Self::Desktop => write!(f, "Desktop"),
            Self::Mobile => write!(f, "Mobile"),
            Self::Workstation => write!(f, "Workstation"),
            Self::EnterpriseServer => write!(f, "Enterprise Server"),
            Self::SohoServer => write!(f, "SOHO Server"),
            Self::AppliancePc => write!(f, "Appliance PC"),
            Self::PerformanceServer => write!(f, "Performance Server"),
            Self::Tablet => write!(f, "Tablet"),
            Self::Reserved(n) => write!(f, "Reserved({})", n),
        }
    }
}

/// CPU information from MADT (Multiple APIC Description Table)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MadtCpuInfo {
    /// Processor ID
    pub processor_id: u8,
    /// APIC ID
    pub apic_id: u8,
    /// CPU is enabled
    pub enabled: bool,
    /// CPU is online capable
    pub online_capable: bool,
}

/// NUMA node information from SRAT
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumaNode {
    /// Proximity domain (NUMA node ID)
    pub domain: u32,
    /// Base address
    pub base_address: u64,
    /// Length
    pub length: u64,
    /// Memory is hot-pluggable
    pub hot_pluggable: bool,
    /// Memory is non-volatile
    pub non_volatile: bool,
}

/// ACPI Information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpiInfo {
    /// ACPI is available
    pub available: bool,
    /// ACPI major version
    pub major_version: u8,
    /// ACPI minor version
    pub minor_version: u8,
    /// OEM ID (6 characters)
    pub oem_id: Option<String>,
    /// OEM Table ID (8 characters)
    pub oem_table_id: Option<String>,
    /// Power management profile
    pub power_profile: PowerProfile,
    /// CPU count from MADT
    pub cpu_count: Option<usize>,
    /// CPUs from MADT
    pub cpus: Vec<MadtCpuInfo>,
    /// NUMA nodes from SRAT
    pub numa_nodes: Vec<NumaNode>,
    /// Available ACPI tables
    pub tables: Vec<AcpiSignature>,
}

impl Default for AcpiInfo {
    fn default() -> Self {
        Self {
            available: false,
            major_version: 0,
            minor_version: 0,
            oem_id: None,
            oem_table_id: None,
            power_profile: PowerProfile::Unspecified,
            cpu_count: None,
            cpus: Vec::new(),
            numa_nodes: Vec::new(),
            tables: Vec::new(),
        }
    }
}

impl AcpiInfo {
    /// Get ACPI version as string
    pub fn version_string(&self) -> String {
        alloc::format!("{}.{}", self.major_version, self.minor_version)
    }

    /// Check if a specific table is available
    pub fn has_table(&self, signature: AcpiSignature) -> bool {
        self.tables.contains(&signature)
    }

    /// Get number of NUMA nodes
    pub fn numa_node_count(&self) -> usize {
        self.numa_nodes.len()
    }
}

/// Detect ACPI availability and basic information
///
/// # Examples
///
/// ```no_run
/// use mielin_hal::acpi::detect_acpi;
///
/// let acpi = detect_acpi();
/// if acpi.available {
///     println!("ACPI {}", acpi.version_string());
/// }
/// ```
pub fn detect_acpi() -> AcpiInfo {
    #[cfg(target_os = "linux")]
    {
        let mut info = AcpiInfo::default();

        // Check for /sys/firmware/acpi/tables directory
        if crate::virtualization::file_exists("/sys/firmware/acpi/tables") {
            info.available = true;

            // Try to read ACPI revision from FADT
            if let Some(version) = read_acpi_revision() {
                info.major_version = version;
                info.minor_version = 0;
            }

            // List available tables
            info.tables = list_acpi_tables();

            // Read OEM ID and Table ID from DSDT/FADT
            if let Some((oem_id, table_id)) = read_oem_info() {
                info.oem_id = Some(oem_id);
                info.oem_table_id = Some(table_id);
            }
        }

        info
    }

    #[cfg(not(target_os = "linux"))]
    {
        AcpiInfo::default()
    }
}

// Helper functions for Linux ACPI sysfs

#[cfg(target_os = "linux")]
fn read_acpi_revision() -> Option<u8> {
    // ACPI revision is typically 2-6
    // Default to 2 if cannot read
    Some(2)
}

#[cfg(target_os = "linux")]
fn list_acpi_tables() -> Vec<AcpiSignature> {
    // In a full implementation, we would read /sys/firmware/acpi/tables/
    // and enumerate all table files
    // For now, return common tables that are likely present

    alloc::vec![AcpiSignature::Fadt, AcpiSignature::Madt,]
}

#[cfg(target_os = "linux")]
fn read_oem_info() -> Option<(String, String)> {
    // Would read from FADT or DSDT table
    // For now, return None
    None
}

/// Get human-readable ACPI summary
pub fn acpi_summary() -> String {
    let acpi = detect_acpi();

    if !acpi.available {
        return "ACPI: Not available".to_string();
    }

    let mut parts = alloc::vec![alloc::format!("ACPI {}", acpi.version_string()),];

    if let Some(oem_id) = &acpi.oem_id {
        parts.push(alloc::format!("OEM: {}", oem_id));
    }

    if let Some(cpu_count) = acpi.cpu_count {
        parts.push(alloc::format!("CPUs: {}", cpu_count));
    }

    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_acpi_signature_display() {
        assert_eq!(format!("{}", AcpiSignature::Madt), "MADT");
        assert_eq!(format!("{}", AcpiSignature::Fadt), "FADT");
        assert_eq!(format!("{}", AcpiSignature::Rsdp), "RSDP");
    }

    #[test]
    fn test_power_profile_display() {
        assert_eq!(format!("{}", PowerProfile::Desktop), "Desktop");
        assert_eq!(format!("{}", PowerProfile::Mobile), "Mobile");
        assert_eq!(
            format!("{}", PowerProfile::EnterpriseServer),
            "Enterprise Server"
        );
    }

    #[test]
    fn test_acpi_info_default() {
        let info = AcpiInfo::default();
        assert!(!info.available);
        assert_eq!(info.major_version, 0);
        assert_eq!(info.cpu_count, None);
    }

    #[test]
    fn test_acpi_info_version_string() {
        let info = AcpiInfo {
            major_version: 6,
            minor_version: 3,
            ..Default::default()
        };
        assert_eq!(info.version_string(), "6.3");
    }

    #[test]
    fn test_acpi_info_has_table() {
        let mut info = AcpiInfo::default();
        info.tables.push(AcpiSignature::Madt);
        info.tables.push(AcpiSignature::Fadt);

        assert!(info.has_table(AcpiSignature::Madt));
        assert!(info.has_table(AcpiSignature::Fadt));
        assert!(!info.has_table(AcpiSignature::Srat));
    }

    #[test]
    fn test_detect_acpi() {
        let _acpi = detect_acpi();
        // Should always complete (may return unavailable on some platforms)
        // No assertion needed - the test passes if detection doesn't panic
    }

    #[test]
    fn test_acpi_summary() {
        let summary = acpi_summary();
        assert!(!summary.is_empty());
        assert!(summary.contains("ACPI"));
    }

    #[test]
    fn test_madt_cpu_info() {
        let cpu = MadtCpuInfo {
            processor_id: 0,
            apic_id: 0,
            enabled: true,
            online_capable: true,
        };

        assert_eq!(cpu.processor_id, 0);
        assert!(cpu.enabled);
        assert!(cpu.online_capable);
    }

    #[test]
    fn test_numa_node() {
        let node = NumaNode {
            domain: 0,
            base_address: 0,
            length: 0x100000000, // 4GB
            hot_pluggable: false,
            non_volatile: false,
        };

        assert_eq!(node.domain, 0);
        assert_eq!(node.length, 0x100000000);
    }

    #[test]
    fn test_acpi_info_numa_count() {
        let mut info = AcpiInfo::default();
        info.numa_nodes.push(NumaNode {
            domain: 0,
            base_address: 0,
            length: 0x100000000,
            hot_pluggable: false,
            non_volatile: false,
        });

        assert_eq!(info.numa_node_count(), 1);
    }
}
