//! Sandboxed execution for agents
//!
//! This module provides sandboxing capabilities to restrict agent execution
//! and enforce resource limits and capability constraints.

use super::attestation::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Sandbox configuration defining execution constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum memory usage in bytes
    pub max_memory_bytes: u64,
    /// Maximum CPU time in milliseconds
    pub max_cpu_time_ms: u64,
    /// Maximum number of file descriptors
    pub max_file_descriptors: u32,
    /// Maximum network connections
    pub max_network_connections: u32,
    /// Allowed capabilities
    pub allowed_capabilities: HashSet<Capability>,
    /// Blocked system calls (names)
    pub blocked_syscalls: HashSet<String>,
    /// Read-only file paths
    pub readonly_paths: Vec<String>,
    /// Writable file paths
    pub writable_paths: Vec<String>,
    /// Network access restrictions
    pub network_restrictions: NetworkRestrictions,
    /// Enable strict mode (deny-by-default)
    pub strict_mode: bool,
}

/// Network access restrictions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkRestrictions {
    /// Allow outbound connections
    pub allow_outbound: bool,
    /// Allow inbound connections
    pub allow_inbound: bool,
    /// Allowed destination IP ranges (CIDR notation)
    pub allowed_destinations: Vec<String>,
    /// Blocked destination IP ranges
    pub blocked_destinations: Vec<String>,
    /// Allowed ports
    pub allowed_ports: Vec<u16>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 512 * 1024 * 1024, // 512 MB
            max_cpu_time_ms: 60_000,             // 1 minute
            max_file_descriptors: 100,
            max_network_connections: 10,
            allowed_capabilities: HashSet::new(),
            blocked_syscalls: HashSet::new(),
            readonly_paths: vec!["/etc".to_string(), "/usr".to_string()],
            writable_paths: vec![std::env::temp_dir().to_string_lossy().into_owned()],
            network_restrictions: NetworkRestrictions::default(),
            strict_mode: true,
        }
    }
}

impl SandboxConfig {
    /// Create a minimal restrictive sandbox
    pub fn minimal() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            max_cpu_time_ms: 5_000,             // 5 seconds
            max_file_descriptors: 10,
            max_network_connections: 0,
            allowed_capabilities: HashSet::new(),
            blocked_syscalls: Self::default_blocked_syscalls(),
            readonly_paths: vec![],
            writable_paths: vec![],
            network_restrictions: NetworkRestrictions {
                allow_outbound: false,
                allow_inbound: false,
                allowed_destinations: Vec::new(),
                blocked_destinations: Vec::new(),
                allowed_ports: Vec::new(),
            },
            strict_mode: true,
        }
    }

    /// Create a permissive sandbox for development
    pub fn permissive() -> Self {
        let mut allowed_capabilities = HashSet::new();
        allowed_capabilities.insert(Capability::Network);
        allowed_capabilities.insert(Capability::FileSystem);
        allowed_capabilities.insert(Capability::Messaging);

        Self {
            max_memory_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
            max_cpu_time_ms: 300_000,                 // 5 minutes
            max_file_descriptors: 1000,
            max_network_connections: 100,
            allowed_capabilities,
            blocked_syscalls: HashSet::new(),
            readonly_paths: Vec::new(),
            writable_paths: vec![
                std::env::temp_dir().to_string_lossy().into_owned(),
                "/var/tmp".to_string(),
                "/home".to_string(),
            ],
            network_restrictions: NetworkRestrictions {
                allow_outbound: true,
                allow_inbound: true,
                allowed_destinations: Vec::new(),
                blocked_destinations: Vec::new(),
                allowed_ports: Vec::new(),
            },
            strict_mode: false,
        }
    }

    /// Default blocked system calls for security
    fn default_blocked_syscalls() -> HashSet<String> {
        let mut blocked = HashSet::new();
        blocked.insert("ptrace".to_string());
        blocked.insert("reboot".to_string());
        blocked.insert("kexec_load".to_string());
        blocked.insert("swapon".to_string());
        blocked.insert("swapoff".to_string());
        blocked
    }

    /// Grant a capability
    pub fn grant_capability(&mut self, capability: Capability) {
        self.allowed_capabilities.insert(capability);
    }

    /// Revoke a capability
    pub fn revoke_capability(&mut self, capability: &Capability) {
        self.allowed_capabilities.remove(capability);
    }

    /// Check if a capability is allowed
    pub fn has_capability(&self, capability: &Capability) -> bool {
        self.allowed_capabilities.contains(capability)
    }

    /// Add a blocked syscall
    pub fn block_syscall(&mut self, syscall: String) {
        self.blocked_syscalls.insert(syscall);
    }

    /// Check if a syscall is blocked
    pub fn is_syscall_blocked(&self, syscall: &str) -> bool {
        self.blocked_syscalls.contains(syscall)
    }

    /// Check if a file path is writable
    pub fn is_path_writable(&self, path: &str) -> bool {
        self.writable_paths.iter().any(|p| path.starts_with(p))
    }

    /// Check if a file path is readable
    pub fn is_path_readable(&self, path: &str) -> bool {
        self.writable_paths.iter().any(|p| path.starts_with(p))
            || self.readonly_paths.iter().any(|p| path.starts_with(p))
    }
}

/// Violation of sandbox constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxViolation {
    /// Memory limit exceeded
    MemoryLimitExceeded { used: u64, limit: u64 },
    /// CPU time limit exceeded
    CpuTimeLimitExceeded { used_ms: u64, limit_ms: u64 },
    /// File descriptor limit exceeded
    FileDescriptorLimitExceeded { used: u32, limit: u32 },
    /// Network connection limit exceeded
    NetworkConnectionLimitExceeded { used: u32, limit: u32 },
    /// Capability not granted
    CapabilityDenied { capability: String },
    /// System call blocked
    SyscallBlocked { syscall: String },
    /// File access denied
    FileAccessDenied { path: String, operation: String },
    /// Network access denied
    NetworkAccessDenied { destination: String, port: u16 },
}

impl std::fmt::Display for SandboxViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MemoryLimitExceeded { used, limit } => {
                write!(
                    f,
                    "Memory limit exceeded: {} bytes used, {} bytes limit",
                    used, limit
                )
            }
            Self::CpuTimeLimitExceeded { used_ms, limit_ms } => {
                write!(
                    f,
                    "CPU time limit exceeded: {} ms used, {} ms limit",
                    used_ms, limit_ms
                )
            }
            Self::FileDescriptorLimitExceeded { used, limit } => {
                write!(
                    f,
                    "File descriptor limit exceeded: {} used, {} limit",
                    used, limit
                )
            }
            Self::NetworkConnectionLimitExceeded { used, limit } => {
                write!(
                    f,
                    "Network connection limit exceeded: {} used, {} limit",
                    used, limit
                )
            }
            Self::CapabilityDenied { capability } => {
                write!(f, "Capability denied: {}", capability)
            }
            Self::SyscallBlocked { syscall } => {
                write!(f, "System call blocked: {}", syscall)
            }
            Self::FileAccessDenied { path, operation } => {
                write!(f, "File access denied: {} operation on {}", operation, path)
            }
            Self::NetworkAccessDenied { destination, port } => {
                write!(f, "Network access denied: {}:{}", destination, port)
            }
        }
    }
}

/// Sandbox executor for enforcing sandbox constraints
#[derive(Debug)]
pub struct SandboxExecutor {
    /// Sandbox configuration
    config: SandboxConfig,
    /// Current resource usage
    usage: ResourceUsage,
    /// Violations recorded
    violations: Vec<SandboxViolation>,
}

/// Current resource usage
#[derive(Debug, Default, Clone)]
struct ResourceUsage {
    /// Current memory usage in bytes
    memory_bytes: u64,
    /// Total CPU time used in milliseconds
    cpu_time_ms: u64,
    /// Current number of open file descriptors
    file_descriptors: u32,
    /// Current number of network connections
    network_connections: u32,
}

impl SandboxExecutor {
    /// Create a new sandbox executor
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            usage: ResourceUsage::default(),
            violations: Vec::new(),
        }
    }

    /// Check if a capability is allowed
    pub fn check_capability(&mut self, capability: &Capability) -> Result<(), SandboxViolation> {
        if self.config.has_capability(capability) {
            Ok(())
        } else {
            let violation = SandboxViolation::CapabilityDenied {
                capability: capability.name(),
            };
            self.violations.push(violation.clone());
            Err(violation)
        }
    }

    /// Check if a syscall is allowed
    pub fn check_syscall(&mut self, syscall: &str) -> Result<(), SandboxViolation> {
        if self.config.is_syscall_blocked(syscall) {
            let violation = SandboxViolation::SyscallBlocked {
                syscall: syscall.to_string(),
            };
            self.violations.push(violation.clone());
            Err(violation)
        } else {
            Ok(())
        }
    }

    /// Check if a file operation is allowed
    pub fn check_file_access(&mut self, path: &str, write: bool) -> Result<(), SandboxViolation> {
        let allowed = if write {
            self.config.is_path_writable(path)
        } else {
            self.config.is_path_readable(path)
        };

        if allowed || !self.config.strict_mode {
            Ok(())
        } else {
            let violation = SandboxViolation::FileAccessDenied {
                path: path.to_string(),
                operation: if write {
                    "write".to_string()
                } else {
                    "read".to_string()
                },
            };
            self.violations.push(violation.clone());
            Err(violation)
        }
    }

    /// Check and update memory usage
    pub fn check_memory(&mut self, bytes: u64) -> Result<(), SandboxViolation> {
        self.usage.memory_bytes = bytes;

        if bytes > self.config.max_memory_bytes {
            let violation = SandboxViolation::MemoryLimitExceeded {
                used: bytes,
                limit: self.config.max_memory_bytes,
            };
            self.violations.push(violation.clone());
            Err(violation)
        } else {
            Ok(())
        }
    }

    /// Check and update CPU time usage
    pub fn check_cpu_time(&mut self, ms: u64) -> Result<(), SandboxViolation> {
        self.usage.cpu_time_ms = ms;

        if ms > self.config.max_cpu_time_ms {
            let violation = SandboxViolation::CpuTimeLimitExceeded {
                used_ms: ms,
                limit_ms: self.config.max_cpu_time_ms,
            };
            self.violations.push(violation.clone());
            Err(violation)
        } else {
            Ok(())
        }
    }

    /// Increment file descriptor count
    pub fn increment_file_descriptors(&mut self) -> Result<(), SandboxViolation> {
        let new_count = self.usage.file_descriptors + 1;

        if new_count > self.config.max_file_descriptors {
            let violation = SandboxViolation::FileDescriptorLimitExceeded {
                used: new_count,
                limit: self.config.max_file_descriptors,
            };
            self.violations.push(violation.clone());
            Err(violation)
        } else {
            self.usage.file_descriptors = new_count;
            Ok(())
        }
    }

    /// Decrement file descriptor count
    pub fn decrement_file_descriptors(&mut self) {
        self.usage.file_descriptors = self.usage.file_descriptors.saturating_sub(1);
    }

    /// Increment network connection count
    pub fn increment_network_connections(&mut self) -> Result<(), SandboxViolation> {
        let new_count = self.usage.network_connections + 1;

        if new_count > self.config.max_network_connections {
            let violation = SandboxViolation::NetworkConnectionLimitExceeded {
                used: new_count,
                limit: self.config.max_network_connections,
            };
            self.violations.push(violation.clone());
            Err(violation)
        } else {
            self.usage.network_connections = new_count;
            Ok(())
        }
    }

    /// Decrement network connection count
    pub fn decrement_network_connections(&mut self) {
        self.usage.network_connections = self.usage.network_connections.saturating_sub(1);
    }

    /// Get all recorded violations
    pub fn violations(&self) -> &[SandboxViolation] {
        &self.violations
    }

    /// Get violation count
    pub fn violation_count(&self) -> usize {
        self.violations.len()
    }

    /// Clear violations
    pub fn clear_violations(&mut self) {
        self.violations.clear();
    }

    /// Get current resource usage
    pub fn get_usage(&self) -> (u64, u64, u32, u32) {
        (
            self.usage.memory_bytes,
            self.usage.cpu_time_ms,
            self.usage.file_descriptors,
            self.usage.network_connections,
        )
    }

    /// Reset resource counters
    pub fn reset_usage(&mut self) {
        self.usage = ResourceUsage::default();
    }

    /// Get sandbox configuration
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Update sandbox configuration
    pub fn update_config(&mut self, config: SandboxConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_memory_bytes, 512 * 1024 * 1024);
        assert!(config.strict_mode);
    }

    #[test]
    fn test_sandbox_config_minimal() {
        let config = SandboxConfig::minimal();
        assert_eq!(config.max_memory_bytes, 64 * 1024 * 1024);
        assert_eq!(config.max_network_connections, 0);
    }

    #[test]
    fn test_sandbox_config_permissive() {
        let config = SandboxConfig::permissive();
        assert!(!config.strict_mode);
        assert!(config.has_capability(&Capability::Network));
    }

    #[test]
    fn test_sandbox_config_grant_revoke_capability() {
        let mut config = SandboxConfig::default();
        assert!(!config.has_capability(&Capability::Network));

        config.grant_capability(Capability::Network);
        assert!(config.has_capability(&Capability::Network));

        config.revoke_capability(&Capability::Network);
        assert!(!config.has_capability(&Capability::Network));
    }

    #[test]
    fn test_sandbox_config_syscall_blocking() {
        let mut config = SandboxConfig::default();
        config.block_syscall("dangerous_call".to_string());

        assert!(config.is_syscall_blocked("dangerous_call"));
        assert!(!config.is_syscall_blocked("safe_call"));
    }

    #[test]
    fn test_sandbox_config_path_access() {
        let config = SandboxConfig::default();

        let tmp_test = std::env::temp_dir()
            .join("test")
            .to_string_lossy()
            .into_owned();
        assert!(config.is_path_readable("/etc/config"));
        assert!(!config.is_path_writable("/etc/config"));
        assert!(config.is_path_writable(&tmp_test));
    }

    #[test]
    fn test_sandbox_executor_capability_check() {
        let mut config = SandboxConfig::default();
        config.grant_capability(Capability::Network);

        let mut executor = SandboxExecutor::new(config);

        assert!(executor.check_capability(&Capability::Network).is_ok());
        assert!(executor.check_capability(&Capability::FileSystem).is_err());
    }

    #[test]
    fn test_sandbox_executor_syscall_check() {
        let config = SandboxConfig::minimal();
        let mut executor = SandboxExecutor::new(config);

        assert!(executor.check_syscall("ptrace").is_err());
        assert!(executor.check_syscall("read").is_ok());
    }

    #[test]
    fn test_sandbox_executor_memory_limit() {
        let config = SandboxConfig::minimal();
        let mut executor = SandboxExecutor::new(config);

        assert!(executor.check_memory(32 * 1024 * 1024).is_ok());
        assert!(executor.check_memory(128 * 1024 * 1024).is_err());
        assert_eq!(executor.violation_count(), 1);
    }

    #[test]
    fn test_sandbox_executor_cpu_time_limit() {
        let config = SandboxConfig::minimal();
        let mut executor = SandboxExecutor::new(config);

        assert!(executor.check_cpu_time(1000).is_ok());
        assert!(executor.check_cpu_time(10_000).is_err());
    }

    #[test]
    fn test_sandbox_executor_file_descriptors() {
        let config = SandboxConfig::minimal();
        let mut executor = SandboxExecutor::new(config);

        for _ in 0..10 {
            assert!(executor.increment_file_descriptors().is_ok());
        }

        assert!(executor.increment_file_descriptors().is_err());

        executor.decrement_file_descriptors();
        assert!(executor.increment_file_descriptors().is_ok());
    }

    #[test]
    fn test_sandbox_executor_network_connections() {
        let config = SandboxConfig {
            max_network_connections: 5,
            ..Default::default()
        };

        let mut executor = SandboxExecutor::new(config);

        for _ in 0..5 {
            assert!(executor.increment_network_connections().is_ok());
        }

        assert!(executor.increment_network_connections().is_err());

        executor.decrement_network_connections();
        assert!(executor.increment_network_connections().is_ok());
    }

    #[test]
    fn test_sandbox_executor_violations() {
        let config = SandboxConfig::minimal();
        let mut executor = SandboxExecutor::new(config);

        executor.check_capability(&Capability::Admin).ok();
        executor.check_memory(1_000_000_000).ok();

        assert!(executor.violation_count() > 0);

        executor.clear_violations();
        assert_eq!(executor.violation_count(), 0);
    }

    #[test]
    fn test_sandbox_executor_file_access() {
        let config = SandboxConfig::default();
        let mut executor = SandboxExecutor::new(config);

        let tmp_test = std::env::temp_dir()
            .join("test")
            .to_string_lossy()
            .into_owned();
        assert!(executor.check_file_access(&tmp_test, true).is_ok());
        assert!(executor.check_file_access("/etc/passwd", false).is_ok());
        assert!(executor.check_file_access("/etc/passwd", true).is_err());
    }
}
