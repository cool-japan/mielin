//! System Information Detection
//!
//! Provides detection of memory size, CPU core count, and CPU topology.
//! Uses platform-specific APIs when available.

use crate::Architecture;

/// Memory information
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryInfo {
    /// Total physical memory in bytes
    pub total: usize,
    /// Available memory in bytes (if detectable)
    pub available: usize,
    /// Page size in bytes
    pub page_size: usize,
}

impl MemoryInfo {
    /// Detect memory information for the current platform
    pub fn detect() -> Self {
        let page_size = detect_page_size();
        let total = detect_total_memory();
        let available = detect_available_memory();

        Self {
            total,
            available,
            page_size,
        }
    }

    /// Get total memory in megabytes
    pub fn total_mb(&self) -> usize {
        self.total / (1024 * 1024)
    }

    /// Get total memory in gigabytes
    pub fn total_gb(&self) -> usize {
        self.total / (1024 * 1024 * 1024)
    }
}

/// CPU topology information
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuTopology {
    /// Number of physical CPU cores
    pub physical_cores: usize,
    /// Number of logical CPU cores (including hyperthreads)
    pub logical_cores: usize,
    /// Number of CPU sockets/packages
    pub sockets: usize,
    /// Number of NUMA nodes
    pub numa_nodes: usize,
    /// Whether hyperthreading/SMT is enabled
    pub hyperthreading: bool,
}

impl CpuTopology {
    /// Detect CPU topology for the current platform
    pub fn detect() -> Self {
        let logical_cores = detect_logical_cores();
        let physical_cores = detect_physical_cores();
        let sockets = detect_sockets();
        let numa_nodes = detect_numa_nodes();
        let hyperthreading = logical_cores > physical_cores;

        Self {
            physical_cores,
            logical_cores,
            sockets,
            numa_nodes,
            hyperthreading,
        }
    }

    /// Get cores per socket
    pub fn cores_per_socket(&self) -> usize {
        if self.sockets == 0 {
            return self.physical_cores;
        }
        self.physical_cores / self.sockets
    }

    /// Get threads per core
    pub fn threads_per_core(&self) -> usize {
        if self.physical_cores == 0 {
            return 1;
        }
        self.logical_cores / self.physical_cores
    }
}

/// Detect total physical memory
fn detect_total_memory() -> usize {
    #[cfg(all(target_os = "linux", not(target_arch = "arm")))]
    {
        // On Linux, read from /proc/meminfo or use sysconf
        // Since we're no_std, we use a syscall-based approach
        detect_memory_linux()
    }

    #[cfg(target_os = "macos")]
    {
        detect_memory_macos()
    }

    #[cfg(target_os = "windows")]
    {
        detect_memory_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        // Default fallback - return 0 (unknown)
        0
    }
}

/// Detect available (free) memory
fn detect_available_memory() -> usize {
    // Available memory is harder to detect reliably
    // Return 0 as "unknown" for now
    0
}

/// Detect page size
fn detect_page_size() -> usize {
    #[cfg(target_os = "linux")]
    {
        4096 // Standard page size on most Linux systems
    }

    #[cfg(target_os = "macos")]
    {
        4096 // macOS uses 4KB pages on Intel, 16KB on Apple Silicon but 4KB for compatibility
    }

    #[cfg(target_os = "windows")]
    {
        4096 // Windows uses 4KB pages
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        4096 // Default assumption
    }
}

/// Detect logical CPU cores
fn detect_logical_cores() -> usize {
    #[cfg(target_os = "linux")]
    {
        detect_cores_linux()
    }

    #[cfg(target_os = "macos")]
    {
        detect_cores_macos()
    }

    #[cfg(target_os = "windows")]
    {
        detect_cores_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        1
    }
}

/// Detect physical CPU cores
fn detect_physical_cores() -> usize {
    // On most systems without detailed topology info, assume physical = logical
    // This will be refined with CPUID on x86
    #[cfg(target_arch = "x86_64")]
    {
        detect_physical_cores_x86()
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // Assume no hyperthreading
        detect_logical_cores()
    }
}

/// Detect number of CPU sockets
fn detect_sockets() -> usize {
    // Most consumer systems have 1 socket
    // Server detection requires CPUID or /sys/devices/system/cpu
    1
}

/// Detect number of NUMA nodes
fn detect_numa_nodes() -> usize {
    #[cfg(target_os = "linux")]
    {
        // Could read from /sys/devices/system/node/
        // For now, assume single NUMA node
        1
    }

    #[cfg(not(target_os = "linux"))]
    {
        1
    }
}

// Platform-specific implementations

#[cfg(target_os = "linux")]
fn detect_memory_linux() -> usize {
    // Use sysinfo syscall on Linux
    // sysinfo struct contains totalram, freeram, etc.
    // Since we're no_std, we do a raw syscall

    #[repr(C)]
    struct SysInfo {
        uptime: isize,
        loads: [usize; 3],
        totalram: usize,
        freeram: usize,
        sharedram: usize,
        bufferram: usize,
        totalswap: usize,
        freeswap: usize,
        procs: u16,
        pad: u16,
        totalhigh: usize,
        freehigh: usize,
        mem_unit: u32,
        _padding: [u8; 256], // Padding to ensure struct size
    }

    let mut info = SysInfo {
        uptime: 0,
        loads: [0; 3],
        totalram: 0,
        freeram: 0,
        sharedram: 0,
        bufferram: 0,
        totalswap: 0,
        freeswap: 0,
        procs: 0,
        pad: 0,
        totalhigh: 0,
        freehigh: 0,
        mem_unit: 0,
        _padding: [0; 256],
    };

    unsafe {
        // sysinfo syscall number is 99 on x86_64
        #[cfg(target_arch = "x86_64")]
        {
            let ret: isize;
            core::arch::asm!(
                "syscall",
                in("rax") 99_usize, // __NR_sysinfo
                in("rdi") &mut info as *mut SysInfo,
                lateout("rax") ret,
                out("rcx") _,
                out("r11") _,
                options(nostack)
            );
            if ret == 0 {
                return info.totalram * info.mem_unit as usize;
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            let ret: isize;
            core::arch::asm!(
                "svc #0",
                in("x8") 179_usize, // __NR_sysinfo on aarch64
                in("x0") &mut info as *mut SysInfo,
                lateout("x0") ret,
                options(nostack)
            );
            if ret == 0 {
                return info.totalram * info.mem_unit as usize;
            }
        }
    }

    0 // Fallback if syscall failed
}

#[cfg(target_os = "linux")]
fn detect_cores_linux() -> usize {
    // Use sched_getaffinity or read from /sys
    // For simplicity, we'll use a reasonable default based on architecture

    #[cfg(target_arch = "x86_64")]
    {
        // Try CPUID first
        let cores = detect_cores_cpuid();
        if cores > 0 {
            return cores;
        }
    }

    // Fallback to 1 core
    1
}

#[cfg(target_os = "macos")]
fn detect_memory_macos() -> usize {
    // On macOS, we'd use sysctl, but that's not no_std compatible
    // Return a sensible default (0 = unknown)
    0
}

#[cfg(target_os = "macos")]
fn detect_cores_macos() -> usize {
    // Would use sysctl hw.ncpu
    1
}

#[cfg(target_os = "windows")]
fn detect_memory_windows() -> usize {
    // Would use GetPhysicallyInstalledSystemMemory
    0
}

#[cfg(target_os = "windows")]
fn detect_cores_windows() -> usize {
    // Would use GetSystemInfo
    1
}

#[cfg(target_arch = "x86_64")]
fn detect_cores_cpuid() -> usize {
    // Use CPUID to get number of logical processors
    // EBX is reserved by LLVM, so we need to save/restore it manually
    unsafe {
        let ebx: u32;
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "mov {0:e}, ebx",
            "pop rbx",
            out(reg) ebx,
            out("eax") _,
            out("ecx") _,
            out("edx") _,
            options(nostack)
        );

        // Logical processor count is in bits 23:16 of EBX
        let logical_count = (ebx >> 16) & 0xff;

        if logical_count > 0 {
            logical_count as usize
        } else {
            1
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn detect_physical_cores_x86() -> usize {
    // CPUID leaf 4 (deterministic cache parameters) can give us core count
    // For simplicity, we'll check if hyperthreading is enabled via leaf 1
    unsafe {
        let ebx: u32;
        let edx: u32;
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "mov {0:e}, ebx",
            "mov {1:e}, edx",
            "pop rbx",
            out(reg) ebx,
            out(reg) edx,
            out("eax") _,
            out("ecx") _,
            options(nostack)
        );

        let logical_count = (ebx >> 16) & 0xff;
        let has_ht = (edx & (1 << 28)) != 0; // HTT bit

        let logical = if logical_count > 0 {
            logical_count as usize
        } else {
            1
        };

        if has_ht && logical > 1 {
            // Assume 2 threads per core for hyperthreading
            logical / 2
        } else {
            logical
        }
    }
}

/// Helper to get memory info for a specific architecture
pub fn get_memory_info(_arch: &Architecture) -> MemoryInfo {
    MemoryInfo::detect()
}

/// Helper to get CPU topology
pub fn get_cpu_topology() -> CpuTopology {
    CpuTopology::detect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_info_detect() {
        let info = MemoryInfo::detect();
        // Page size should always be positive
        assert!(info.page_size > 0);
        // Total may be 0 on unsupported platforms, but shouldn't panic
    }

    #[test]
    fn test_memory_info_conversions() {
        let info = MemoryInfo {
            total: 8 * 1024 * 1024 * 1024, // 8 GB
            available: 4 * 1024 * 1024 * 1024,
            page_size: 4096,
        };

        assert_eq!(info.total_gb(), 8);
        assert_eq!(info.total_mb(), 8 * 1024);
    }

    #[test]
    fn test_cpu_topology_detect() {
        let topology = CpuTopology::detect();
        // Should have at least 1 core
        assert!(topology.logical_cores >= 1);
        assert!(topology.physical_cores >= 1);
    }

    #[test]
    fn test_cpu_topology_threads_per_core() {
        let topology = CpuTopology {
            physical_cores: 4,
            logical_cores: 8,
            sockets: 1,
            numa_nodes: 1,
            hyperthreading: true,
        };

        assert_eq!(topology.threads_per_core(), 2);
        assert_eq!(topology.cores_per_socket(), 4);
    }

    #[test]
    fn test_cpu_topology_no_hyperthreading() {
        let topology = CpuTopology {
            physical_cores: 4,
            logical_cores: 4,
            sockets: 1,
            numa_nodes: 1,
            hyperthreading: false,
        };

        assert_eq!(topology.threads_per_core(), 1);
    }

    #[test]
    fn test_page_size_detection() {
        let info = MemoryInfo::detect();
        // Page size should be a power of 2
        assert!(info.page_size > 0);
        assert!((info.page_size & (info.page_size - 1)) == 0);
    }

    #[test]
    fn test_get_helpers() {
        let arch = crate::detect_architecture();
        let mem_info = get_memory_info(&arch);
        let cpu_topology = get_cpu_topology();

        assert!(mem_info.page_size > 0);
        assert!(cpu_topology.logical_cores >= 1);
    }
}
