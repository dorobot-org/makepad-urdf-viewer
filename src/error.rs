//! Error types for robot loading and validation

use std::path::PathBuf;

/// Errors that can occur during robot loading
#[derive(Debug)]
pub enum RobotError {
    /// Failed to read URDF file
    UrdfReadError {
        path: PathBuf,
        source: std::io::Error,
    },
    /// Failed to parse URDF content
    UrdfParseError(String),
    /// Mesh file not found
    MeshNotFound {
        path: PathBuf,
        link_name: String,
    },
    /// Failed to load mesh file
    MeshLoadError {
        path: PathBuf,
        reason: String,
    },
    /// Invalid joint reference in URDF
    InvalidJointReference {
        joint_name: String,
        missing_link: String,
    },
    /// Cycle detected in joint graph
    CycleDetected {
        joints: Vec<String>,
    },
    /// Invalid joint limits
    InvalidJointLimits {
        joint_name: String,
        lower: f32,
        upper: f32,
    },
    /// No root link found
    NoRootLink,
    /// Generic validation error
    ValidationError(String),
}

impl std::fmt::Display for RobotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RobotError::UrdfReadError { path, source } => {
                write!(f, "Failed to read URDF file '{}': {}", path.display(), source)
            }
            RobotError::UrdfParseError(msg) => {
                write!(f, "Failed to parse URDF: {}", msg)
            }
            RobotError::MeshNotFound { path, link_name } => {
                write!(f, "Mesh file not found for link '{}': {}", link_name, path.display())
            }
            RobotError::MeshLoadError { path, reason } => {
                write!(f, "Failed to load mesh '{}': {}", path.display(), reason)
            }
            RobotError::InvalidJointReference { joint_name, missing_link } => {
                write!(f, "Joint '{}' references missing link '{}'", joint_name, missing_link)
            }
            RobotError::CycleDetected { joints } => {
                write!(f, "Cycle detected in joint graph: {:?}", joints)
            }
            RobotError::InvalidJointLimits { joint_name, lower, upper } => {
                write!(f, "Joint '{}' has invalid limits: lower={} > upper={}", joint_name, lower, upper)
            }
            RobotError::NoRootLink => {
                write!(f, "No root link found in URDF")
            }
            RobotError::ValidationError(msg) => {
                write!(f, "Validation error: {}", msg)
            }
        }
    }
}

impl std::error::Error for RobotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RobotError::UrdfReadError { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl RobotError {
    /// Get a user-friendly error message
    pub fn user_message(&self) -> String {
        match self {
            RobotError::UrdfReadError { path, .. } => {
                format!(
                    "Could not open the robot file.\n\n\
                    Please check that '{}' exists and is readable.",
                    path.display()
                )
            }
            RobotError::UrdfParseError(_) => {
                "The robot file appears to be corrupted or invalid.\n\n\
                Please check that it is a valid URDF file.".to_string()
            }
            RobotError::MeshNotFound { path, link_name } => {
                format!(
                    "Could not find the 3D model for '{}'.\n\n\
                    Missing file: {}",
                    link_name, path.display()
                )
            }
            RobotError::MeshLoadError { path, reason } => {
                format!(
                    "Could not load the 3D model.\n\n\
                    File: {}\n\
                    Reason: {}",
                    path.display(), reason
                )
            }
            RobotError::InvalidJointReference { joint_name, missing_link } => {
                format!(
                    "The robot definition has an error.\n\n\
                    Joint '{}' references a link '{}' that doesn't exist.",
                    joint_name, missing_link
                )
            }
            RobotError::CycleDetected { .. } => {
                "The robot's joint structure forms a loop, which is not supported.\n\n\
                Please check the URDF file for circular joint references.".to_string()
            }
            RobotError::InvalidJointLimits { joint_name, .. } => {
                format!(
                    "Joint '{}' has invalid movement limits.\n\n\
                    The lower limit is greater than the upper limit.",
                    joint_name
                )
            }
            RobotError::NoRootLink => {
                "Could not find the robot's base link.\n\n\
                The URDF file may be incomplete.".to_string()
            }
            RobotError::ValidationError(msg) => {
                format!("Robot validation failed:\n\n{}", msg)
            }
        }
    }

    /// Check if error is recoverable (can continue with partial robot)
    pub fn is_recoverable(&self) -> bool {
        matches!(self,
            RobotError::MeshNotFound { .. } |
            RobotError::MeshLoadError { .. } |
            RobotError::InvalidJointLimits { .. }
        )
    }
}

/// Warnings that don't prevent robot loading
#[derive(Debug, Clone)]
pub enum RobotWarning {
    /// Mesh file not found (robot still loads)
    MissingMesh { link_name: String, path: String },
    /// Joint limits seem unusual
    UnusualJointLimits { joint_name: String, lower: f32, upper: f32 },
    /// Joint type not fully supported
    UnsupportedJointType { joint_name: String, joint_type: String },
}

impl std::fmt::Display for RobotWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RobotWarning::MissingMesh { link_name, path } => {
                write!(f, "Missing mesh for '{}': {}", link_name, path)
            }
            RobotWarning::UnusualJointLimits { joint_name, lower, upper } => {
                write!(f, "Unusual limits for '{}': [{}, {}]", joint_name, lower, upper)
            }
            RobotWarning::UnsupportedJointType { joint_name, joint_type } => {
                write!(f, "Joint '{}' has unsupported type '{}', treating as fixed", joint_name, joint_type)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;
    use std::io::{Error, ErrorKind};

    #[test]
    fn test_urdf_read_error_display() {
        let err = RobotError::UrdfReadError {
            path: PathBuf::from("/path/to/robot.urdf"),
            source: Error::new(ErrorKind::NotFound, "file not found"),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("robot.urdf"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn test_urdf_parse_error_display() {
        let err = RobotError::UrdfParseError("invalid XML".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("invalid XML"));
    }

    #[test]
    fn test_mesh_not_found_display() {
        let err = RobotError::MeshNotFound {
            path: PathBuf::from("/path/to/mesh.stl"),
            link_name: "arm_link".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("mesh.stl"));
        assert!(msg.contains("arm_link"));
    }

    #[test]
    fn test_mesh_load_error_display() {
        let err = RobotError::MeshLoadError {
            path: PathBuf::from("/path/to/mesh.stl"),
            reason: "corrupted file".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("corrupted file"));
    }

    #[test]
    fn test_invalid_joint_reference_display() {
        let err = RobotError::InvalidJointReference {
            joint_name: "joint1".to_string(),
            missing_link: "missing_link".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("joint1"));
        assert!(msg.contains("missing_link"));
    }

    #[test]
    fn test_cycle_detected_display() {
        let err = RobotError::CycleDetected {
            joints: vec!["j1".to_string(), "j2".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Cycle"));
    }

    #[test]
    fn test_invalid_joint_limits_display() {
        let err = RobotError::InvalidJointLimits {
            joint_name: "joint1".to_string(),
            lower: 1.0,
            upper: -1.0,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("joint1"));
        assert!(msg.contains("invalid limits"));
    }

    #[test]
    fn test_no_root_link_display() {
        let err = RobotError::NoRootLink;
        let msg = format!("{}", err);
        assert!(msg.contains("root link"));
    }

    #[test]
    fn test_validation_error_display() {
        let err = RobotError::ValidationError("custom message".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("custom message"));
    }

    #[test]
    fn test_is_recoverable() {
        // Recoverable errors
        assert!(RobotError::MeshNotFound {
            path: PathBuf::new(),
            link_name: String::new(),
        }.is_recoverable());

        assert!(RobotError::MeshLoadError {
            path: PathBuf::new(),
            reason: String::new(),
        }.is_recoverable());

        assert!(RobotError::InvalidJointLimits {
            joint_name: String::new(),
            lower: 0.0,
            upper: 0.0,
        }.is_recoverable());

        // Non-recoverable errors
        assert!(!RobotError::NoRootLink.is_recoverable());
        assert!(!RobotError::UrdfParseError(String::new()).is_recoverable());
    }

    #[test]
    fn test_user_message_contains_helpful_text() {
        let err = RobotError::NoRootLink;
        let msg = err.user_message();
        assert!(msg.contains("base link"));

        let err = RobotError::CycleDetected { joints: vec![] };
        let msg = err.user_message();
        assert!(msg.contains("loop"));
    }

    #[test]
    fn test_warning_display() {
        let warn = RobotWarning::MissingMesh {
            link_name: "link1".to_string(),
            path: "/path/to/mesh.stl".to_string(),
        };
        let msg = format!("{}", warn);
        assert!(msg.contains("link1"));

        let warn = RobotWarning::UnusualJointLimits {
            joint_name: "joint1".to_string(),
            lower: -10.0,
            upper: 10.0,
        };
        let msg = format!("{}", warn);
        assert!(msg.contains("joint1"));

        let warn = RobotWarning::UnsupportedJointType {
            joint_name: "joint1".to_string(),
            joint_type: "floating".to_string(),
        };
        let msg = format!("{}", warn);
        assert!(msg.contains("floating"));
    }

    #[test]
    fn test_error_source() {
        let io_err = Error::new(ErrorKind::NotFound, "test");
        let err = RobotError::UrdfReadError {
            path: PathBuf::new(),
            source: io_err,
        };
        assert!(StdError::source(&err).is_some());

        let err = RobotError::NoRootLink;
        assert!(StdError::source(&err).is_none());
    }
}
