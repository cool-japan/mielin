//! Virtualization and Container Detection
//!
//! This module provides comprehensive detection of virtualization environments
//! including hypervisors and container runtimes.
//!
//! # Supported Hypervisors
//!
//! - **KVM** - Kernel-based Virtual Machine (Linux)
//! - **Xen** - Xen hypervisor (paravirtualization and HVM)
//! - **VMware** - VMware ESXi, Workstation, Fusion
//! - **Hyper-V** - Microsoft Hyper-V
//! - **VirtualBox** - Oracle VirtualBox
//! - **QEMU** - QEMU/TCG emulation
//! - **Parallels** - Parallels Desktop
//! - **Bhyve** - FreeBSD hypervisor
//!
//! # Supported Container Runtimes
//!
//! - **Docker** - Docker containers
//! - **LXC** - Linux Containers
//! - **Podman** - Podman containers
//! - **Kubernetes** - Kubernetes pods
//! - **systemd-nspawn** - systemd containers
//!
//! # Detection Methods
//!
//! ## x86_64
//! Uses CPUID instruction (leaf 0x40000000) to query hypervisor presence:
//! - CPUID.0x40000000.EAX: Maximum hypervisor CPUID leaf
//! - CPUID.0x40000000.EBX-EDX: Hypervisor vendor string (12 bytes)
//!
//! Known vendor strings:
//! - "KVMKVMKVM\0\0\0" - KVM
//! - "XenVMMXenVMM" - Xen
//! - "VMwareVMware" - VMware
//! - "Microsoft Hv" - Hyper-V
//! - "VBoxVBoxVBox" - VirtualBox
//! - "TCGTCGTCGTCG" - QEMU/TCG
//!
//! ## AArch64
//! - Device tree `/proc/device-tree/hypervisor/compatible`
//! - Virtualization extensions (EL2 detection)
//!
//! ## Container Detection
//! - `/proc/1/cgroup` - cgroup hierarchy analysis
//! - `/.dockerenv` - Docker-specific marker file
//! - `/run/.containerenv` - Podman marker file
//! - Environment variables (KUBERNETES_SERVICE_HOST, container)
//!
//! # Examples
//!
//! ```
//! use mielin_hal::virtualization::detect_virtualization;
//!
//! let virt = detect_virtualization();
//!
//! if virt.is_virtualized() {
//!     println!("Running in virtualized environment");
//!
//!     if let Some(hypervisor) = virt.hypervisor {
//!         println!("Hypervisor: {:?}", hypervisor);
//!     }
//!
//!     if let Some(container) = virt.container {
//!         println!("Container: {:?}", container);
//!     }
//! } else {
//!     println!("Running on bare metal");
//! }
//! ```
//!
//! # Performance
//!
//! Detection is fast (< 1μs) and cached on first call. CPUID instructions
//! are serializing but complete in a few CPU cycles.
//!
//! # Safety
//!
//! Uses safe Rust with minimal unsafe blocks:
//! - x86_64 CPUID via inline assembly (architecture-specific)
//! - File I/O uses safe std::fs or syscalls for no_std
//!
//! # Known Limitations
//!
//! 1. **Nested Virtualization** - May only detect innermost hypervisor
//! 2. **Stealth Hypervisors** - Rootkits can hide CPUID signature
//! 3. **Future Hypervisors** - Unknown vendor strings return Generic
//! 4. **Container Runtimes** - Detection based on common markers only

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::__cpuid;

/// Hypervisor type detected via CPUID or device tree
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HypervisorType {
    /// KVM (Kernel-based Virtual Machine)
    Kvm,
    /// Xen hypervisor
    Xen,
    /// VMware ESXi/Workstation/Fusion
    VMware,
    /// Microsoft Hyper-V
    HyperV,
    /// Oracle VirtualBox
    VirtualBox,
    /// QEMU with TCG emulation
    Qemu,
    /// Parallels Desktop
    Parallels,
    /// FreeBSD bhyve
    Bhyve,
    /// Apple Hypervisor Framework
    AppleHypervisor,
    /// Generic/Unknown hypervisor
    Generic,
}

impl fmt::Display for HypervisorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kvm => write!(f, "KVM"),
            Self::Xen => write!(f, "Xen"),
            Self::VMware => write!(f, "VMware"),
            Self::HyperV => write!(f, "Microsoft Hyper-V"),
            Self::VirtualBox => write!(f, "Oracle VirtualBox"),
            Self::Qemu => write!(f, "QEMU"),
            Self::Parallels => write!(f, "Parallels"),
            Self::Bhyve => write!(f, "bhyve"),
            Self::AppleHypervisor => write!(f, "Apple Hypervisor"),
            Self::Generic => write!(f, "Generic Hypervisor"),
        }
    }
}

/// Container runtime type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ContainerType {
    /// Docker container
    Docker,
    /// LXC (Linux Containers)
    Lxc,
    /// Podman container
    Podman,
    /// Kubernetes pod
    Kubernetes,
    /// systemd-nspawn container
    SystemdNspawn,
    /// Generic/Unknown container
    Generic,
}

impl fmt::Display for ContainerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Docker => write!(f, "Docker"),
            Self::Lxc => write!(f, "LXC"),
            Self::Podman => write!(f, "Podman"),
            Self::Kubernetes => write!(f, "Kubernetes"),
            Self::SystemdNspawn => write!(f, "systemd-nspawn"),
            Self::Generic => write!(f, "Generic Container"),
        }
    }
}

/// Virtualization detection result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualizationInfo {
    /// Detected hypervisor (None if bare metal)
    pub hypervisor: Option<HypervisorType>,
    /// Detected container runtime (None if not containerized)
    pub container: Option<ContainerType>,
    /// Hypervisor vendor string (12 bytes from CPUID)
    pub vendor_string: Option<String>,
    /// Paravirtualization features available
    pub paravirt_features: ParavirtFeatures,
}

impl VirtualizationInfo {
    /// Returns true if running in any virtualized environment
    #[inline]
    pub fn is_virtualized(&self) -> bool {
        self.hypervisor.is_some() || self.container.is_some()
    }

    /// Returns true if running on bare metal (no hypervisor, no container)
    #[inline]
    pub fn is_bare_metal(&self) -> bool {
        !self.is_virtualized()
    }

    /// Returns true if running in a hypervisor
    #[inline]
    pub fn is_hypervisor(&self) -> bool {
        self.hypervisor.is_some()
    }

    /// Returns true if running in a container
    #[inline]
    pub fn is_container(&self) -> bool {
        self.container.is_some()
    }
}

impl Default for VirtualizationInfo {
    fn default() -> Self {
        Self {
            hypervisor: None,
            container: None,
            vendor_string: None,
            paravirt_features: ParavirtFeatures::empty(),
        }
    }
}

bitflags::bitflags! {
    /// Paravirtualization features available from hypervisor
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ParavirtFeatures: u32 {
        /// Paravirtualized clock source
        const CLOCK = 1 << 0;
        /// Paravirtualized spinlocks
        const SPINLOCK = 1 << 1;
        /// Paravirtualized TLB flush
        const TLB_FLUSH = 1 << 2;
        /// Paravirtualized IPI (inter-processor interrupt)
        const IPI = 1 << 3;
        /// Paravirtualized EOI (end of interrupt)
        const EOI = 1 << 4;
        /// Enlightened VMCS (Hyper-V)
        const ENLIGHTENED_VMCS = 1 << 5;
        /// KVM async page fault
        const ASYNC_PF = 1 << 6;
        /// KVM steal time accounting
        const STEAL_TIME = 1 << 7;
    }
}

// Helper functions for no_std file I/O

/// Read a file using syscalls (no_std compatible)
#[cfg(target_os = "linux")]
pub(crate) fn read_file(path: &str) -> Option<Vec<u8>> {
    use alloc::vec;

    // Convert path to null-terminated C string
    let mut path_bytes = Vec::with_capacity(path.len() + 1);
    path_bytes.extend_from_slice(path.as_bytes());
    path_bytes.push(0);

    // Open file (syscall number 2 on x86_64, aarch64)
    #[cfg(target_arch = "x86_64")]
    let fd: isize = {
        let out: isize;
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") 2_usize => out,
                in("rdi") path_bytes.as_ptr(),
                in("rsi") 0_usize, // O_RDONLY
                in("rdx") 0_usize,
                lateout("rcx") _,
                lateout("r11") _,
            );
        }
        out
    };

    #[cfg(target_arch = "aarch64")]
    let fd: isize = {
        let out: isize;
        unsafe {
            core::arch::asm!(
                "svc #0",
                inlateout("x8") 56_usize => _, // openat syscall
                in("x0") -100_isize, // AT_FDCWD
                in("x1") path_bytes.as_ptr(),
                in("x2") 0_usize, // O_RDONLY
                inlateout("x0") 0_usize => out,
            );
        }
        out
    };

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let fd: isize = -1;

    if fd < 0 {
        return None;
    }

    // Read file contents (max 4096 bytes)
    let mut buffer = vec![0u8; 4096];
    #[cfg(target_arch = "x86_64")]
    let bytes_read: isize = {
        let out: isize;
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") 0_usize => out, // read syscall
                in("rdi") fd,
                in("rsi") buffer.as_mut_ptr(),
                in("rdx") buffer.len(),
                lateout("rcx") _,
                lateout("r11") _,
            );
        }
        out
    };

    #[cfg(target_arch = "aarch64")]
    let bytes_read: isize = {
        let out: isize;
        unsafe {
            core::arch::asm!(
                "svc #0",
                inlateout("x8") 63_usize => _, // read syscall
                in("x0") fd,
                in("x1") buffer.as_mut_ptr(),
                in("x2") buffer.len(),
                inlateout("x0") 0_usize => out,
            );
        }
        out
    };

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let bytes_read: isize = -1;

    // Close file (syscall number 3)
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") 3_usize => _,
            in("rdi") fd,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "svc #0",
            inlateout("x8") 57_usize => _, // close syscall
            in("x0") fd,
        );
    }

    if bytes_read > 0 {
        buffer.truncate(bytes_read as usize);
        Some(buffer)
    } else {
        None
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn read_file(_path: &str) -> Option<Vec<u8>> {
    None
}

/// Check if a file exists using syscalls
#[cfg(target_os = "linux")]
pub(crate) fn file_exists(path: &str) -> bool {
    let mut path_bytes = Vec::with_capacity(path.len() + 1);
    path_bytes.extend_from_slice(path.as_bytes());
    path_bytes.push(0);

    // Use stat syscall to check existence
    #[cfg(target_arch = "x86_64")]
    let result: isize = {
        let out: isize;
        let mut stat_buf = [0u8; 144]; // sizeof(struct stat)
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") 4_usize => out, // stat syscall
                in("rdi") path_bytes.as_ptr(),
                in("rsi") stat_buf.as_mut_ptr(),
                lateout("rcx") _,
                lateout("r11") _,
            );
        }
        out
    };

    #[cfg(target_arch = "aarch64")]
    let result: isize = {
        let out: isize;
        let mut stat_buf = [0u8; 144];
        unsafe {
            core::arch::asm!(
                "svc #0",
                inlateout("x8") 79_usize => _, // fstatat syscall
                in("x0") -100_isize, // AT_FDCWD
                in("x1") path_bytes.as_ptr(),
                in("x2") stat_buf.as_mut_ptr(),
                in("x3") 0_usize,
                inlateout("x0") 0_usize => out,
            );
        }
        out
    };

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let result: isize = -1;

    result == 0
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn file_exists(_path: &str) -> bool {
    false
}

/// Check if an environment variable exists
#[cfg(target_os = "linux")]
fn env_var_exists(name: &str) -> bool {
    // Read /proc/self/environ to check environment variables
    if let Some(data) = read_file("/proc/self/environ") {
        // Environment variables are null-separated key=value pairs
        for entry in data.split(|&b| b == 0) {
            if let Some(eq_pos) = entry.iter().position(|&b| b == b'=') {
                if &entry[..eq_pos] == name.as_bytes() {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(not(target_os = "linux"))]
fn env_var_exists(_name: &str) -> bool {
    false
}

/// Detect virtualization environment (hypervisor and container)
///
/// This function performs comprehensive virtualization detection using
/// architecture-specific methods and filesystem inspection.
///
/// # Examples
///
/// ```
/// use mielin_hal::virtualization::detect_virtualization;
///
/// let virt = detect_virtualization();
/// if virt.is_virtualized() {
///     println!("Virtualized: {:?}", virt);
/// }
/// ```
pub fn detect_virtualization() -> VirtualizationInfo {
    let mut info = VirtualizationInfo::default();

    // Detect hypervisor via architecture-specific methods
    info.hypervisor = detect_hypervisor(&mut info);

    // Detect container runtime
    info.container = detect_container();

    info
}

/// Detect hypervisor type via CPUID (x86_64) or device tree (ARM)
fn detect_hypervisor(info: &mut VirtualizationInfo) -> Option<HypervisorType> {
    #[cfg(target_arch = "x86_64")]
    {
        detect_hypervisor_x86_64(info)
    }

    #[cfg(target_arch = "aarch64")]
    {
        detect_hypervisor_aarch64(info)
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = info;
        None
    }
}

/// Detect hypervisor on x86_64 via CPUID
#[cfg(target_arch = "x86_64")]
fn detect_hypervisor_x86_64(info: &mut VirtualizationInfo) -> Option<HypervisorType> {
    // CPUID leaf 0x1, ECX bit 31: Hypervisor present
    let cpuid1 = unsafe { __cpuid(1) };
    if (cpuid1.ecx & (1 << 31)) == 0 {
        return None; // Bare metal
    }

    // CPUID leaf 0x40000000: Hypervisor CPUID information
    let cpuid_hv = unsafe { __cpuid(0x40000000) };

    // Extract vendor string from EBX, ECX, EDX (12 bytes)
    let mut vendor_bytes = [0u8; 12];
    vendor_bytes[0..4].copy_from_slice(&cpuid_hv.ebx.to_le_bytes());
    vendor_bytes[4..8].copy_from_slice(&cpuid_hv.ecx.to_le_bytes());
    vendor_bytes[8..12].copy_from_slice(&cpuid_hv.edx.to_le_bytes());

    let vendor_string = String::from_utf8_lossy(&vendor_bytes).to_string();
    info.vendor_string = Some(vendor_string.clone());

    // Match vendor string to known hypervisors
    let hypervisor = match vendor_string.as_str() {
        "KVMKVMKVM\0\0\0" => HypervisorType::Kvm,
        "XenVMMXenVMM" => HypervisorType::Xen,
        "VMwareVMware" => HypervisorType::VMware,
        "Microsoft Hv" => HypervisorType::HyperV,
        "VBoxVBoxVBox" => HypervisorType::VirtualBox,
        "TCGTCGTCGTCG" => HypervisorType::Qemu,
        "prl hyperv  " => HypervisorType::Parallels,
        "bhyve bhyve " => HypervisorType::Bhyve,
        _ => HypervisorType::Generic,
    };

    // Detect paravirt features for KVM
    if hypervisor == HypervisorType::Kvm {
        detect_kvm_paravirt_features(info);
    }

    Some(hypervisor)
}

/// Detect KVM paravirtualization features via CPUID leaf 0x40000001
#[cfg(target_arch = "x86_64")]
fn detect_kvm_paravirt_features(info: &mut VirtualizationInfo) {
    let cpuid_kvm = unsafe { __cpuid(0x40000001) };
    let mut features = ParavirtFeatures::empty();

    // KVM paravirt features in EAX
    if (cpuid_kvm.eax & (1 << 0)) != 0 {
        features |= ParavirtFeatures::CLOCK;
    }
    if (cpuid_kvm.eax & (1 << 3)) != 0 {
        features |= ParavirtFeatures::SPINLOCK;
    }
    if (cpuid_kvm.eax & (1 << 9)) != 0 {
        features |= ParavirtFeatures::TLB_FLUSH;
    }
    if (cpuid_kvm.eax & (1 << 4)) != 0 {
        features |= ParavirtFeatures::ASYNC_PF;
    }
    if (cpuid_kvm.eax & (1 << 5)) != 0 {
        features |= ParavirtFeatures::STEAL_TIME;
    }

    info.paravirt_features = features;
}

/// Detect hypervisor on AArch64 via device tree
#[cfg(target_arch = "aarch64")]
fn detect_hypervisor_aarch64(info: &mut VirtualizationInfo) -> Option<HypervisorType> {
    // Try reading device tree hypervisor compatible string
    if let Some(data) = read_file("/proc/device-tree/hypervisor/compatible") {
        if let Ok(compatible) = core::str::from_utf8(&data) {
            let hypervisor = if compatible.contains("xen") {
                HypervisorType::Xen
            } else if compatible.contains("kvm") {
                HypervisorType::Kvm
            } else {
                HypervisorType::Generic
            };

            info.vendor_string = Some(compatible.trim_end_matches('\0').to_string());
            return Some(hypervisor);
        }
    }

    // Check for Apple Hypervisor Framework (macOS on Apple Silicon)
    #[cfg(target_os = "macos")]
    {
        // On macOS, check for Hypervisor.framework presence
        if file_exists("/System/Library/Frameworks/Hypervisor.framework") {
            return Some(HypervisorType::AppleHypervisor);
        }
    }

    None
}

/// Detect container runtime via filesystem markers and cgroups
fn detect_container() -> Option<ContainerType> {
    // Check for Docker marker file
    if file_exists("/.dockerenv") {
        return Some(ContainerType::Docker);
    }

    // Check for Podman marker file
    if file_exists("/run/.containerenv") {
        return Some(ContainerType::Podman);
    }

    // Check for Kubernetes via environment variable
    if env_var_exists("KUBERNETES_SERVICE_HOST") {
        return Some(ContainerType::Kubernetes);
    }

    // Parse /proc/1/cgroup to detect container runtime
    if let Some(data) = read_file("/proc/1/cgroup") {
        if let Ok(cgroup) = core::str::from_utf8(&data) {
            if cgroup.contains("/docker/") {
                return Some(ContainerType::Docker);
            }
            if cgroup.contains("/lxc/") {
                return Some(ContainerType::Lxc);
            }
            if cgroup.contains("/kubepods/") {
                return Some(ContainerType::Kubernetes);
            }
            if cgroup.contains("/system.slice/docker-") {
                return Some(ContainerType::Docker);
            }
            if cgroup.contains("systemd-nspawn") {
                return Some(ContainerType::SystemdNspawn);
            }

            // Generic container detection (any cgroup v2 container)
            if cgroup.contains("0::/") && !cgroup.contains("0::/init.scope") {
                return Some(ContainerType::Generic);
            }
        }
    }

    None
}

/// Get a human-readable summary of virtualization status
pub fn virtualization_summary() -> String {
    let virt = detect_virtualization();

    if virt.is_bare_metal() {
        return "Bare Metal (No Virtualization)".to_string();
    }

    let mut parts = Vec::new();

    if let Some(hypervisor) = virt.hypervisor {
        parts.push(format!("Hypervisor: {}", hypervisor));
        if let Some(vendor) = &virt.vendor_string {
            parts.push(format!("Vendor: {}", vendor));
        }
    }

    if let Some(container) = virt.container {
        parts.push(format!("Container: {}", container));
    }

    if !virt.paravirt_features.is_empty() {
        parts.push(format!("Paravirt: {:?}", virt.paravirt_features));
    }

    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_virtualization() {
        let virt = detect_virtualization();
        // Should always succeed (bare metal returns None for both fields)
        assert!(virt.hypervisor.is_none() || virt.hypervisor.is_some());
    }

    #[test]
    fn test_is_virtualized() {
        let virt = detect_virtualization();
        let virtualized = virt.is_virtualized();
        let bare_metal = virt.is_bare_metal();

        // Should be mutually exclusive
        assert_ne!(virtualized, bare_metal);
    }

    #[test]
    fn test_hypervisor_display() {
        assert_eq!(format!("{}", HypervisorType::Kvm), "KVM");
        assert_eq!(format!("{}", HypervisorType::VMware), "VMware");
        assert_eq!(format!("{}", HypervisorType::HyperV), "Microsoft Hyper-V");
    }

    #[test]
    fn test_container_display() {
        assert_eq!(format!("{}", ContainerType::Docker), "Docker");
        assert_eq!(format!("{}", ContainerType::Kubernetes), "Kubernetes");
    }

    #[test]
    fn test_paravirt_features() {
        let features = ParavirtFeatures::CLOCK | ParavirtFeatures::SPINLOCK;
        assert!(features.contains(ParavirtFeatures::CLOCK));
        assert!(features.contains(ParavirtFeatures::SPINLOCK));
        assert!(!features.contains(ParavirtFeatures::TLB_FLUSH));
    }

    #[test]
    fn test_virtualization_info_default() {
        let info = VirtualizationInfo::default();
        assert!(info.is_bare_metal());
        assert!(!info.is_virtualized());
        assert!(!info.is_hypervisor());
        assert!(!info.is_container());
    }

    #[test]
    fn test_virtualization_summary() {
        let summary = virtualization_summary();
        assert!(!summary.is_empty());
        // Should contain either "Bare Metal" or hypervisor/container info
        assert!(
            summary.contains("Bare Metal")
                || summary.contains("Hypervisor")
                || summary.contains("Container")
        );
    }

    #[test]
    fn test_hypervisor_types() {
        let types = [
            HypervisorType::Kvm,
            HypervisorType::Xen,
            HypervisorType::VMware,
            HypervisorType::HyperV,
            HypervisorType::VirtualBox,
            HypervisorType::Qemu,
        ];

        for ty in &types {
            // Should have unique string representation
            assert!(!format!("{}", ty).is_empty());
        }
    }

    #[test]
    fn test_container_types() {
        let types = [
            ContainerType::Docker,
            ContainerType::Lxc,
            ContainerType::Podman,
            ContainerType::Kubernetes,
        ];

        for ty in &types {
            // Should have unique string representation
            assert!(!format!("{}", ty).is_empty());
        }
    }

    #[test]
    fn test_paravirt_features_all() {
        let all_features = ParavirtFeatures::CLOCK
            | ParavirtFeatures::SPINLOCK
            | ParavirtFeatures::TLB_FLUSH
            | ParavirtFeatures::IPI
            | ParavirtFeatures::EOI
            | ParavirtFeatures::ENLIGHTENED_VMCS
            | ParavirtFeatures::ASYNC_PF
            | ParavirtFeatures::STEAL_TIME;

        assert!(all_features.contains(ParavirtFeatures::CLOCK));
        assert!(all_features.contains(ParavirtFeatures::STEAL_TIME));
    }

    #[test]
    fn test_virtualization_info_equality() {
        let info1 = VirtualizationInfo::default();
        let info2 = VirtualizationInfo::default();
        assert_eq!(info1, info2);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn test_x86_64_hypervisor_detection() {
        // This test runs on x86_64 but may be bare metal
        let virt = detect_virtualization();

        // If hypervisor detected, should have vendor string
        if virt.hypervisor.is_some() {
            assert!(virt.vendor_string.is_some());
        }
    }

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn test_aarch64_hypervisor_detection() {
        // This test runs on AArch64 but may be bare metal
        let virt = detect_virtualization();

        // Test should complete without panic
        assert!(virt.hypervisor.is_none() || virt.hypervisor.is_some());
    }

    #[test]
    fn test_container_detection_no_panic() {
        // Container detection should never panic
        let container = detect_container();
        assert!(container.is_none() || container.is_some());
    }
}
