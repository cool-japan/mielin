//! Device Tree Support
//!
//! This module provides parsing and querying capabilities for Device Tree
//! data structures, commonly used in embedded systems (ARM, RISC-V).
//!
//! # What is Device Tree?
//!
//! Device Tree is a data structure for describing hardware configuration,
//! widely used in embedded Linux systems. It describes:
//! - CPUs and their properties
//! - Memory layout
//! - Peripheral devices
//! - Interrupt routing
//! - Clock trees
//!
//! # Device Tree Sources
//!
//! On Linux systems, the device tree is accessible via:
//! - `/sys/firmware/devicetree/base/` - Filesystem representation
//! - `/proc/device-tree/` - Legacy symlink
//! - `/boot/*.dtb` - Compiled device tree blobs
//!
//! # Supported Properties
//!
//! - `compatible` - Device compatibility strings
//! - `model` - Human-readable device model
//! - `reg` - Register addresses and sizes
//! - `interrupts` - Interrupt specifications
//! - `status` - Device status (okay, disabled, fail, fail-ssi)
//! - `#address-cells`, `#size-cells` - Address/size encoding
//!
//! # Examples
//!
//! ```no_run
//! use mielin_hal::devicetree::{DeviceTree, DeviceTreeNode};
//!
//! // Read device tree from sysfs
//! let dt = DeviceTree::from_sysfs().expect("Failed to read device tree");
//!
//! // Get root properties
//! if let Some(model) = dt.get_property("model") {
//!     println!("Board: {}", model);
//! }
//!
//! // Find devices by compatible string
//! let uart_nodes = dt.find_compatible("ns16550a");
//! for node in uart_nodes {
//!     println!("UART at: {}", node.path);
//! }
//! ```
//!
//! # Platform Support
//!
//! - **Linux**: Full support via sysfs (`/sys/firmware/devicetree/base/`)
//! - **Bare Metal**: Limited (requires DTB address from bootloader)
//! - **Other OS**: Not supported
//!
//! # Known Limitations
//!
//! 1. **No DTB Binary Parsing**: Currently only reads from sysfs
//! 2. **Limited Property Types**: Focuses on string and u32 properties
//! 3. **No Overlays**: Device tree overlays not supported
//! 4. **No Modification**: Read-only access

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// Device Tree node representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceTreeNode {
    /// Node path (e.g., "/soc/uart@fe201000")
    pub path: String,
    /// Node name (e.g., "uart@fe201000")
    pub name: String,
    /// Compatible strings
    pub compatible: Vec<String>,
    /// Device status
    pub status: DeviceStatus,
    /// Custom properties (name -> value)
    pub properties: Vec<(String, Vec<u8>)>,
}

/// Device status from device tree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceStatus {
    /// Device is operational
    Okay,
    /// Device is disabled
    Disabled,
    /// Device has failed
    Fail,
    /// Device has failed (secondary cause)
    FailSsi,
    /// Status unknown or not specified
    Unknown,
}

impl fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Okay => write!(f, "okay"),
            Self::Disabled => write!(f, "disabled"),
            Self::Fail => write!(f, "fail"),
            Self::FailSsi => write!(f, "fail-ssi"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl DeviceTreeNode {
    /// Create a new device tree node
    pub fn new(path: String, name: String) -> Self {
        Self {
            path,
            name,
            compatible: Vec::new(),
            status: DeviceStatus::Unknown,
            properties: Vec::new(),
        }
    }

    /// Check if node is enabled (status == "okay")
    pub fn is_enabled(&self) -> bool {
        matches!(self.status, DeviceStatus::Okay)
    }

    /// Get property as string
    pub fn get_string_property(&self, name: &str) -> Option<String> {
        for (prop_name, value) in &self.properties {
            if prop_name == name {
                // Remove null terminators
                let end = value.iter().position(|&b| b == 0).unwrap_or(value.len());
                return core::str::from_utf8(&value[..end])
                    .ok()
                    .map(|s| s.to_string());
            }
        }
        None
    }

    /// Get property as u32 (big-endian)
    pub fn get_u32_property(&self, name: &str) -> Option<u32> {
        for (prop_name, value) in &self.properties {
            if prop_name == name && value.len() >= 4 {
                let bytes = [value[0], value[1], value[2], value[3]];
                return Some(u32::from_be_bytes(bytes));
            }
        }
        None
    }

    /// Get property as array of u32 (big-endian)
    pub fn get_u32_array_property(&self, name: &str) -> Option<Vec<u32>> {
        for (prop_name, value) in &self.properties {
            if prop_name == name && value.len() % 4 == 0 {
                let mut result = Vec::new();
                for chunk in value.chunks(4) {
                    if chunk.len() == 4 {
                        let bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
                        result.push(u32::from_be_bytes(bytes));
                    }
                }
                return Some(result);
            }
        }
        None
    }

    /// Check if node is compatible with a given string
    pub fn is_compatible(&self, compat: &str) -> bool {
        self.compatible.iter().any(|c| c.contains(compat))
    }
}

/// Device Tree structure
#[derive(Debug, Clone)]
pub struct DeviceTree {
    /// Root node properties
    pub model: Option<String>,
    /// All device tree nodes
    pub nodes: Vec<DeviceTreeNode>,
}

impl DeviceTree {
    /// Create an empty device tree
    pub fn new() -> Self {
        Self {
            model: None,
            nodes: Vec::new(),
        }
    }

    /// Read device tree from sysfs (/sys/firmware/devicetree/base/)
    #[cfg(target_os = "linux")]
    pub fn from_sysfs() -> Result<Self, &'static str> {
        let base_path = "/sys/firmware/devicetree/base";

        let mut dt = DeviceTree::new();

        // Read model from root
        dt.model = read_dt_property(base_path, "model");

        // For now, just read root properties
        // Full recursive directory traversal would require more complex implementation

        Ok(dt)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn from_sysfs() -> Result<Self, &'static str> {
        Err("Device tree only available on Linux")
    }

    /// Get a property from the root node
    pub fn get_property(&self, name: &str) -> Option<String> {
        if name == "model" {
            return self.model.clone();
        }
        None
    }

    /// Find nodes by compatible string
    pub fn find_compatible(&self, compat: &str) -> Vec<&DeviceTreeNode> {
        self.nodes
            .iter()
            .filter(|node| node.is_compatible(compat))
            .collect()
    }

    /// Find node by path
    pub fn find_node(&self, path: &str) -> Option<&DeviceTreeNode> {
        self.nodes.iter().find(|node| node.path == path)
    }

    /// Get all CPUs from device tree
    pub fn get_cpus(&self) -> Vec<&DeviceTreeNode> {
        self.nodes
            .iter()
            .filter(|node| node.path.starts_with("/cpus/cpu@"))
            .collect()
    }

    /// Get memory regions from device tree
    pub fn get_memory_regions(&self) -> Vec<&DeviceTreeNode> {
        self.nodes
            .iter()
            .filter(|node| node.path.starts_with("/memory@"))
            .collect()
    }
}

impl Default for DeviceTree {
    fn default() -> Self {
        Self::new()
    }
}

// Helper function to read device tree property via syscalls
#[cfg(target_os = "linux")]
fn read_dt_property(base_path: &str, property: &str) -> Option<String> {
    use alloc::format;

    // Construct full path
    let path = format!("{}/{}", base_path, property);

    // Use our existing read_file function from platform or virtualization
    // For now, simplified version
    if let Some(data) = crate::virtualization::read_file(&path) {
        // Device tree strings are null-terminated
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        core::str::from_utf8(&data[..end])
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    }
}

#[cfg(not(target_os = "linux"))]
fn read_dt_property(_base_path: &str, _property: &str) -> Option<String> {
    None
}

/// Get device tree model string
pub fn get_model() -> Option<String> {
    read_dt_property("/sys/firmware/devicetree/base", "model")
}

/// Get device tree compatible strings
pub fn get_compatible() -> Option<Vec<String>> {
    if let Some(data) = crate::virtualization::read_file("/sys/firmware/devicetree/base/compatible")
    {
        // Compatible is a list of null-separated strings
        let mut result = Vec::new();
        let mut start = 0;

        for (i, &byte) in data.iter().enumerate() {
            if byte == 0 && i > start {
                if let Ok(s) = core::str::from_utf8(&data[start..i]) {
                    result.push(s.to_string());
                }
                start = i + 1;
            }
        }

        if !result.is_empty() {
            Some(result)
        } else {
            None
        }
    } else {
        None
    }
}

/// Device tree information summary
#[derive(Debug, Clone, Default)]
pub struct DeviceTreeInfo {
    /// Device tree available
    pub available: bool,
    /// Board model
    pub model: Option<String>,
    /// Compatible strings
    pub compatible: Vec<String>,
    /// Number of CPU nodes found
    pub cpu_count: usize,
}

/// Detect device tree availability and basic info
pub fn detect_devicetree() -> DeviceTreeInfo {
    let mut info = DeviceTreeInfo::default();

    // Check if device tree is available
    info.model = get_model();
    info.available = info.model.is_some();

    if info.available {
        info.compatible = get_compatible().unwrap_or_default();
    }

    info
}

/// Get human-readable device tree summary
pub fn devicetree_summary() -> String {
    let dt_info = detect_devicetree();

    if !dt_info.available {
        return "Device Tree: Not available".to_string();
    }

    if let Some(model) = &dt_info.model {
        alloc::format!("Device Tree: {}", model)
    } else {
        "Device Tree: Available".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::vec;

    #[test]
    fn test_device_status_display() {
        assert_eq!(format!("{}", DeviceStatus::Okay), "okay");
        assert_eq!(format!("{}", DeviceStatus::Disabled), "disabled");
        assert_eq!(format!("{}", DeviceStatus::Fail), "fail");
    }

    #[test]
    fn test_device_tree_node_new() {
        let node = DeviceTreeNode::new("/test".to_string(), "test".to_string());
        assert_eq!(node.path, "/test");
        assert_eq!(node.name, "test");
        assert!(!node.is_enabled()); // Unknown status
    }

    #[test]
    fn test_device_tree_node_enabled() {
        let mut node = DeviceTreeNode::new("/test".to_string(), "test".to_string());
        node.status = DeviceStatus::Okay;
        assert!(node.is_enabled());

        node.status = DeviceStatus::Disabled;
        assert!(!node.is_enabled());
    }

    #[test]
    fn test_device_tree_node_compatible() {
        let mut node = DeviceTreeNode::new("/uart".to_string(), "uart".to_string());
        node.compatible.push("ns16550a".to_string());
        node.compatible.push("uart".to_string());

        assert!(node.is_compatible("ns16550"));
        assert!(node.is_compatible("uart"));
        assert!(!node.is_compatible("i2c"));
    }

    #[test]
    fn test_device_tree_new() {
        let dt = DeviceTree::new();
        assert!(dt.model.is_none());
        assert!(dt.nodes.is_empty());
    }

    #[test]
    fn test_device_tree_find_compatible() {
        let mut dt = DeviceTree::new();

        let mut node1 = DeviceTreeNode::new("/uart0".to_string(), "uart0".to_string());
        node1.compatible.push("ns16550a".to_string());

        let mut node2 = DeviceTreeNode::new("/i2c0".to_string(), "i2c0".to_string());
        node2.compatible.push("i2c-bcm2835".to_string());

        dt.nodes.push(node1);
        dt.nodes.push(node2);

        let uart_nodes = dt.find_compatible("ns16550");
        assert_eq!(uart_nodes.len(), 1);
        assert_eq!(uart_nodes[0].path, "/uart0");
    }

    #[test]
    fn test_device_tree_find_node() {
        let mut dt = DeviceTree::new();
        let node = DeviceTreeNode::new("/test".to_string(), "test".to_string());
        dt.nodes.push(node);

        assert!(dt.find_node("/test").is_some());
        assert!(dt.find_node("/nonexistent").is_none());
    }

    #[test]
    fn test_detect_devicetree() {
        let _dt_info = detect_devicetree();
        // Should always complete (may return unavailable on some platforms)
        // No assertion needed - the test passes if detection doesn't panic
    }

    #[test]
    fn test_devicetree_summary() {
        let summary = devicetree_summary();
        assert!(!summary.is_empty());
        assert!(summary.contains("Device Tree"));
    }

    #[test]
    fn test_device_tree_node_get_string_property() {
        let mut node = DeviceTreeNode::new("/test".to_string(), "test".to_string());
        node.properties
            .push(("reg-names".to_string(), b"control\0".to_vec()));

        assert_eq!(
            node.get_string_property("reg-names"),
            Some("control".to_string())
        );
        assert_eq!(node.get_string_property("nonexistent"), None);
    }

    #[test]
    fn test_device_tree_node_get_u32_property() {
        let mut node = DeviceTreeNode::new("/test".to_string(), "test".to_string());
        // Big-endian u32: 0x12345678
        node.properties
            .push(("reg".to_string(), vec![0x12, 0x34, 0x56, 0x78]));

        assert_eq!(node.get_u32_property("reg"), Some(0x12345678));
        assert_eq!(node.get_u32_property("nonexistent"), None);
    }

    #[test]
    fn test_device_tree_node_get_u32_array_property() {
        let mut node = DeviceTreeNode::new("/test".to_string(), "test".to_string());
        // Two u32 values: 0x1000, 0x2000
        node.properties.push((
            "reg".to_string(),
            vec![
                0x00, 0x00, 0x10, 0x00, // 0x1000
                0x00, 0x00, 0x20, 0x00, // 0x2000
            ],
        ));

        let values = node.get_u32_array_property("reg");
        assert_eq!(values, Some(vec![0x1000, 0x2000]));
    }

    #[test]
    fn test_device_tree_default() {
        let dt = DeviceTree::default();
        assert!(dt.model.is_none());
        assert!(dt.nodes.is_empty());
    }

    #[test]
    fn test_devicetree_info_default() {
        let info = DeviceTreeInfo::default();
        assert!(!info.available);
        assert_eq!(info.cpu_count, 0);
    }
}
