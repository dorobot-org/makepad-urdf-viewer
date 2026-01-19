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

mod model;
mod loader;
mod kinematics;

pub use model::{Robot, RobotLink, RobotJoint, JointType};
pub use loader::{load_robot, load_robot_from_string, validate_robot, LoadResult};
pub use kinematics::{ForwardKinematics, ForwardKinematicsMethods};
