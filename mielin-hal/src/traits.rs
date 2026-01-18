//! Common HAL Traits
//!
//! Provides shared traits and utilities for hardware abstraction,
//! reducing code duplication across GPU, accelerator, cache, and power modules.

/// Trait for types that have a human-readable name
pub trait Named {
    /// Get the display name for this item
    fn name(&self) -> &'static str;
}

/// Trait for hardware device information
pub trait DeviceInfo {
    /// Get the device vendor name
    fn vendor_name(&self) -> &'static str;

    /// Get the device model/product name
    fn model_name(&self) -> &str;

    /// Check if the device is currently available for use
    fn is_available(&self) -> bool;

    /// Get the device index (for multi-device systems)
    fn device_index(&self) -> u32;
}

/// Trait for compute-capable devices
pub trait ComputeDevice: DeviceInfo {
    /// Check if the device supports compute workloads
    fn supports_compute(&self) -> bool;

    /// Get the theoretical peak compute in TFLOPS (FP32)
    fn peak_tflops(&self) -> f32;

    /// Check if the device supports INT8 quantization
    fn supports_int8(&self) -> bool;

    /// Check if the device supports FP16
    fn supports_fp16(&self) -> bool;
}

/// Trait for memory-having devices
pub trait MemoryDevice {
    /// Get total memory in bytes
    fn memory_bytes(&self) -> u64;

    /// Get memory bandwidth in MB/s
    fn memory_bandwidth_mbps(&self) -> u32;

    /// Get memory in megabytes
    fn memory_mb(&self) -> u64 {
        self.memory_bytes() / (1024 * 1024)
    }

    /// Get memory in gigabytes
    fn memory_gb(&self) -> u64 {
        self.memory_bytes() / (1024 * 1024 * 1024)
    }

    /// Get bandwidth in GB/s
    fn bandwidth_gbps(&self) -> f32 {
        self.memory_bandwidth_mbps() as f32 / 1000.0
    }
}

/// Trait for power-aware devices
pub trait PowerDevice {
    /// Get thermal design power (TDP) in watts
    fn tdp_watts(&self) -> u32;

    /// Get current power consumption in watts (if available)
    fn current_power_watts(&self) -> Option<u32> {
        None
    }

    /// Get efficiency in operations per watt
    fn efficiency_ops_per_watt(&self) -> Option<f32> {
        None
    }
}

/// Device detection result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionStatus {
    /// Device was successfully detected
    Detected,
    /// Device detection is not supported on this platform
    NotSupported,
    /// Device detection failed with an error
    Error,
    /// No devices of this type were found
    NotFound,
}

impl DetectionStatus {
    /// Check if detection was successful
    pub fn is_success(&self) -> bool {
        matches!(self, DetectionStatus::Detected)
    }

    /// Check if any error occurred
    pub fn is_error(&self) -> bool {
        matches!(self, DetectionStatus::Error)
    }
}

/// Macro to implement Named trait for enum variants
#[macro_export]
macro_rules! impl_named_enum {
    ($enum_name:ident { $($variant:ident => $name:literal),+ $(,)? }) => {
        impl $crate::traits::Named for $enum_name {
            fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detection_status() {
        assert!(DetectionStatus::Detected.is_success());
        assert!(!DetectionStatus::NotFound.is_success());
        assert!(DetectionStatus::Error.is_error());
        assert!(!DetectionStatus::Detected.is_error());
    }

    // Test mock device implementing traits
    struct MockDevice {
        available: bool,
        memory: u64,
    }

    impl DeviceInfo for MockDevice {
        fn vendor_name(&self) -> &'static str {
            "TestVendor"
        }

        fn model_name(&self) -> &str {
            "TestModel"
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn device_index(&self) -> u32 {
            0
        }
    }

    impl MemoryDevice for MockDevice {
        fn memory_bytes(&self) -> u64 {
            self.memory
        }

        fn memory_bandwidth_mbps(&self) -> u32 {
            1000 * 1000 // 1 TB/s
        }
    }

    #[test]
    fn test_device_info_trait() {
        let device = MockDevice {
            available: true,
            memory: 0,
        };
        assert_eq!(device.vendor_name(), "TestVendor");
        assert!(device.is_available());
    }

    #[test]
    fn test_memory_device_trait() {
        let device = MockDevice {
            available: true,
            memory: 24 * 1024 * 1024 * 1024, // 24 GB
        };
        assert_eq!(device.memory_gb(), 24);
        assert_eq!(device.memory_mb(), 24 * 1024);
        assert!((device.bandwidth_gbps() - 1000.0).abs() < 0.1);
    }
}
