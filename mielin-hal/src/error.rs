//! Error types for HAL operations
//!
//! This module provides error types for hardware abstraction layer operations,
//! including errors for unsupported architectures, unavailable features,
//! and detection failures.

use alloc::string::String;
use core::fmt;

/// Result type for HAL operations
pub type Result<T> = core::result::Result<T, Error>;

/// HAL error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The requested operation is not supported on the current architecture
    UnsupportedArchitecture {
        /// The requested feature
        feature: String,
        /// The current architecture
        architecture: crate::Architecture,
    },

    /// The requested feature is not available on this hardware
    FeatureNotAvailable {
        /// The feature name
        feature: String,
    },

    /// Detection failed for the specified component
    DetectionFailed {
        /// The component that failed detection
        component: String,
        /// Optional error details
        details: Option<String>,
    },

    /// Invalid parameter provided
    InvalidParameter {
        /// Parameter name
        parameter: String,
        /// Description of why it's invalid
        reason: String,
    },

    /// Hardware access error
    HardwareAccessError {
        /// Description of the access error
        details: String,
    },

    /// The operation requires privileges that are not available
    InsufficientPrivileges {
        /// Description of required privileges
        required: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnsupportedArchitecture {
                feature,
                architecture,
            } => {
                write!(
                    f,
                    "Feature '{}' is not supported on architecture '{}'",
                    feature, architecture
                )
            }
            Error::FeatureNotAvailable { feature } => {
                write!(f, "Feature '{}' is not available on this hardware", feature)
            }
            Error::DetectionFailed { component, details } => {
                if let Some(details) = details {
                    write!(
                        f,
                        "Detection failed for component '{}': {}",
                        component, details
                    )
                } else {
                    write!(f, "Detection failed for component '{}'", component)
                }
            }
            Error::InvalidParameter { parameter, reason } => {
                write!(f, "Invalid parameter '{}': {}", parameter, reason)
            }
            Error::HardwareAccessError { details } => {
                write!(f, "Hardware access error: {}", details)
            }
            Error::InsufficientPrivileges { required } => {
                write!(f, "Insufficient privileges: {} required", required)
            }
        }
    }
}

/// Helper to check if an architecture supports a feature
pub fn check_architecture_support(
    feature: &str,
    supported_archs: &[crate::Architecture],
) -> Result<()> {
    let current_arch = crate::detect_architecture();

    if supported_archs.contains(&current_arch) {
        Ok(())
    } else {
        Err(Error::UnsupportedArchitecture {
            feature: feature.into(),
            architecture: current_arch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Architecture;

    #[test]
    fn test_unsupported_architecture_error() {
        let err = Error::UnsupportedArchitecture {
            feature: "test_feature".into(),
            architecture: Architecture::X86_64,
        };

        let msg = alloc::format!("{}", err);
        assert!(msg.contains("test_feature"));
        assert!(msg.contains("x86_64"));
    }

    #[test]
    fn test_feature_not_available_error() {
        let err = Error::FeatureNotAvailable {
            feature: "AVX512".into(),
        };

        let msg = alloc::format!("{}", err);
        assert!(msg.contains("AVX512"));
    }

    #[test]
    fn test_detection_failed_error() {
        let err = Error::DetectionFailed {
            component: "GPU".into(),
            details: Some("No GPU found".into()),
        };

        let msg = alloc::format!("{}", err);
        assert!(msg.contains("GPU"));
        assert!(msg.contains("No GPU found"));
    }

    #[test]
    fn test_detection_failed_no_details() {
        let err = Error::DetectionFailed {
            component: "Cache".into(),
            details: None,
        };

        let msg = alloc::format!("{}", err);
        assert!(msg.contains("Cache"));
    }

    #[test]
    fn test_invalid_parameter_error() {
        let err = Error::InvalidParameter {
            parameter: "cache_level".into(),
            reason: "must be between 1 and 3".into(),
        };

        let msg = alloc::format!("{}", err);
        assert!(msg.contains("cache_level"));
        assert!(msg.contains("must be between 1 and 3"));
    }

    #[test]
    fn test_hardware_access_error() {
        let err = Error::HardwareAccessError {
            details: "CPUID instruction failed".into(),
        };

        let msg = alloc::format!("{}", err);
        assert!(msg.contains("CPUID instruction failed"));
    }

    #[test]
    fn test_insufficient_privileges_error() {
        let err = Error::InsufficientPrivileges {
            required: "root access".into(),
        };

        let msg = alloc::format!("{}", err);
        assert!(msg.contains("root access"));
    }

    #[test]
    fn test_check_architecture_support_success() {
        let current_arch = crate::detect_architecture();
        let supported = [current_arch];

        let result = check_architecture_support("test_feature", &supported);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_architecture_support_failure() {
        let current_arch = crate::detect_architecture();

        // Find an architecture that's different from current
        let unsupported = match current_arch {
            Architecture::X86_64 => Architecture::AArch64,
            _ => Architecture::X86_64,
        };

        let supported = [unsupported];
        let result = check_architecture_support("test_feature", &supported);
        assert!(result.is_err());

        if let Err(Error::UnsupportedArchitecture { feature, .. }) = result {
            assert_eq!(feature, "test_feature");
        } else {
            panic!("Expected UnsupportedArchitecture error");
        }
    }

    #[test]
    fn test_error_equality() {
        let err1 = Error::FeatureNotAvailable {
            feature: "test".into(),
        };
        let err2 = Error::FeatureNotAvailable {
            feature: "test".into(),
        };
        let err3 = Error::FeatureNotAvailable {
            feature: "different".into(),
        };

        assert_eq!(err1, err2);
        assert_ne!(err1, err3);
    }
}
