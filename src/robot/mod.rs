//! Robot module for URDF-based robot representation
//!
//! This module provides data structures and algorithms for loading
//! and manipulating URDF robots with forward kinematics.
//!
//! # Example
//!
//! ```ignore
//! use makepad_urdf_player::robot::{load_robot, ForwardKinematics};
//!
//! let robot = load_robot("path/to/robot.urdf", "path/to/meshes")?;
//! // Robot is ready with forward kinematics computed
//! ```

pub mod collada;
mod model;
mod loader;
mod kinematics;

pub use model::{Robot, RobotLink, RobotJoint, JointType};
pub use loader::{
    load_any, load_mesh_as_robot, load_robot, load_robot_from_string, scan_folder,
    set_virtual_assets, validate_robot, LoadResult, ModelFile, ModelKind,
};
pub use kinematics::{ForwardKinematics, ForwardKinematicsMethods};
